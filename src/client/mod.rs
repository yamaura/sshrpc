pub mod russh;

use crate::transport::stream2transport;
use crate::HandshakeInformation;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

/// Represents an active SSH session
pub struct SshRpcSession<C, S>
where
    S: AsyncRead + AsyncWrite,
{
    pub handshake_information: HandshakeInformation,
    /// channel of execution or other stuff
    pub channel: C,
    /// stream of tcp channel (This is usually a port forward)
    pub stream: S,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum AppProtocolError {
    #[error("App protocol version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u32, got: u32 },
}

impl<C, S> SshRpcSession<C, S>
where
    S: AsyncRead + AsyncWrite,
{
    pub fn try_into_transport<Item, SinkItem>(
        self,
        app_protocol_version: u32,
    ) -> Result<
        (
            C,
            tarpc::serde_transport::Transport<
                S,
                Item,
                SinkItem,
                tarpc::tokio_serde::formats::Bincode<Item, SinkItem>,
            >,
        ),
        AppProtocolError,
    >
    where
        Item: for<'de> Deserialize<'de>,
        SinkItem: Serialize,
    {
        // impl Stream<Item = Result<Item, std::io::Error>> + Sink<SinkItem>
        if self.handshake_information.app_protocol_version != app_protocol_version {
            return Err(
                AppProtocolError::VersionMismatch {
                expected: app_protocol_version,
                got: self.handshake_information.app_protocol_version,
            });
        }
        let transport = stream2transport(self.stream);
        Ok((self.channel, transport))
    }
}

/// launch rpc server on remote ssh server
pub trait SshRpcExt<C, S>
where
    S: AsyncRead + AsyncWrite,
{
    type Error;

    /// Asynchronously executes a remote procedure call server on a remote SSH server.
    /// This function launches a specified binary program with provided arguments on the remote host.
    /// The `binary` program must write `HandshakeInformation` to stdout on the first line.
    /// This function parses this line and then creates an SSH port forward.
    ///
    /// # Parameters
    /// - `binary`: A type that implements `AsyncRead` and `Unpin`, representing the executable binary.
    /// - `args`: A type that can be converted into a `Vec<u8>`, representing the arguments for the binary.
    ///
    /// # Returns
    /// A result containing an `SshRpcSession` on success, or an error of type `Self::Error` on failure.
    async fn exec_rpc_server<R, A>(
        &self,
        binary: R,
        args: A,
    ) -> Result<SshRpcSession<C, S>, Self::Error>
    where
        R: tokio::io::AsyncRead + Unpin,
        A: Into<Vec<u8>>;

    /// Read handshake information from channel and return SshRpcSession from it.
    async fn read_handshake_information(
        &self,
        channel: C,
    ) -> Result<SshRpcSession<C, S>, Self::Error>;
}
