use crate::client::{SshRpcExt, SshRpcSession};
use crate::HandshakeInformation;
use russh::client::{Handle, Handler, Msg};

use russh::{Channel, ChannelMsg, ChannelStream};
use tracing::{debug, error, warn};

fn drop_last_newline(s: &str) -> &str {
    s.strip_suffix('\n').unwrap_or(s)
}

fn trace_msg(command: &str, msg: &ChannelMsg) {
    match msg {
        ChannelMsg::Data { ref data } => {
            let line = String::from_utf8_lossy(data);
            let line = drop_last_newline(&line);
            debug!("{}:stdout: {}", command, line);
        }
        ChannelMsg::ExtendedData { ref data, ext: 1 } => {
            let line = String::from_utf8_lossy(data);
            let line = drop_last_newline(&line);
            warn!("{}:stderr: {}", command, line);
        }
        ChannelMsg::ExitStatus { exit_status } => {
            debug!("{}:exit: Exit status: {:?}", command, exit_status);
        }
        _ => (),
    }
}

async fn wait_until_exit(command: &str, mut channel: Channel<Msg>) -> (Channel<Msg>, Option<u32>) {
    let mut code = None;
    loop {
        let Some(msg) = channel.wait().await else {
            break;
        };
        trace_msg(command, &msg);
        if let ChannelMsg::ExitStatus { exit_status } = msg {
            code = Some(exit_status);
        }
    }
    (channel, code)
}

#[derive(Debug)]
enum ExitStatus {
    Code(u32),
    Processing,
}

impl ExitStatus {
    fn sucess(&self) -> bool {
        match self {
            ExitStatus::Code(code) => *code == 0,
            ExitStatus::Processing => false,
        }
    }

    fn code(&self) -> Option<u32> {
        match self {
            ExitStatus::Code(code) => Some(*code),
            ExitStatus::Processing => None,
        }
    }
}

#[derive(Debug)]
struct Output {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: ExitStatus,
}

trait OutputExt {
    type Error;
    async fn output<A: Into<Vec<u8>>>(&self, command: A) -> Result<Output, Self::Error>;
}

impl<H> OutputExt for Handle<H>
where
    H: Handler,
{
    type Error = russh::Error;
    async fn output<A: Into<Vec<u8>>>(&self, command: A) -> Result<Output, Self::Error> {
        let mut channel = self.channel_open_session().await?;
        channel.exec(true, command).await?;

        let mut stdout = vec![];
        let mut stderr = vec![];
        let mut code = ExitStatus::Processing;
        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.extend_from_slice(data);
                }
                ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                    stderr.extend_from_slice(data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    code = ExitStatus::Code(exit_status);
                }
                _ => {}
            }
        }
        Ok(Output {
            stdout: stdout.strip_suffix(b"\n").unwrap_or(&stdout).to_vec(),
            stderr: stderr.strip_suffix(b"\n").unwrap_or(&stderr).to_vec(),
            code,
        })
    }
}

struct CleanupGuard {
    inner: Channel<Msg>,
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        debug!("CleanupGuard: drop");
        let _ = futures::executor::block_on(self.inner.eof());
    }
}

impl CleanupGuard {
    fn new(inner: Channel<Msg>) -> Self {
        Self { inner }
    }

    fn spawn(mut self) {
        tokio::spawn(async move {
            loop {
                let Some(msg) = self.inner.wait().await else {
                    break;
                };
                trace_msg("CleanupGuard", &msg);
            }
        });
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub enum RpcStartError {
    IoError(#[from] std::io::Error),
    #[error("Handshake information not received")]
    HandshakeInformationNotReceived,
    #[error("Failed to get handshake information: {0}")]
    InvalidHandshakeInformation(String),
    #[error("Failed to launch: status code={0}")]
    LaunchFail(u32),
    ParseHandshakeInformation(#[from] crate::ParseHandshakeError),
    RusshError(#[from] russh::Error),
}

/// Implementation of `SshRpcExt` for `russh`
impl<H> SshRpcExt<Channel<Msg>, ChannelStream<Msg>> for Handle<H>
where
    H: Handler,
{
    type Error = RpcStartError;

    /// Implementation of `SshRpcExt` for `russh`
    async fn exec_rpc_server<R, A>(
        &self,
        mut binary: R,
        args: A,
    ) -> Result<SshRpcSession<Channel<Msg>, ChannelStream<Msg>>, Self::Error>
    where
        R: tokio::io::AsyncRead + Unpin,
        A: Into<Vec<u8>>,
    {
        debug!("which elfexec on remote");
        let has_elfexec = self.output(b"which elfexec").await?.code.sucess();

        let mut channel = if has_elfexec {
            debug!("elfexec is available. using it");
            let mut command = b"elfexec ".to_vec();
            command.extend_from_slice(&args.into());

            let channel = self.channel_open_session().await?;
            channel.exec(true, command).await?;
            tokio::io::copy(&mut binary, &mut channel.make_writer()).await?;
            channel.eof().await?;

            channel
        } else {
            debug!("fall back to write to tmp file (exec only mode)");

            // create tempfile
            let tmpfile = self.output(b"mktemp").await?;
            if !tmpfile.code.sucess() {
                error!("mktemp: {}", String::from_utf8_lossy(&tmpfile.stderr));
                error!("mktemp: status code={:?}", tmpfile.code);
                return Err(RpcStartError::LaunchFail(tmpfile.code.code().unwrap_or(1)));
            }

            let tmpfile = tmpfile.stdout;
            debug!("create tmpfile: {}", String::from_utf8_lossy(&tmpfile));
            // copy
            let mut command = b"cat > ".to_vec();
            command.extend_from_slice(&tmpfile);
            let channel = self.channel_open_session().await?;
            channel.exec(true, command).await?;
            tokio::io::copy(&mut binary, &mut channel.make_writer()).await?;
            channel.eof().await?;

            let (_, status) = wait_until_exit("copy", channel).await;
            if status != Some(0) {
                return Err(RpcStartError::LaunchFail(status.unwrap_or(1)));
            }

            // chmod
            let mut command = b"chmod +x ".to_vec();
            command.extend_from_slice(&tmpfile);
            let chmod = self.output(command).await?;
            if !chmod.code.sucess() {
                error!("chmod: {}", String::from_utf8_lossy(&chmod.stderr));
                error!("chmod: status code={:?}", chmod.code);
                return Err(RpcStartError::LaunchFail(chmod.code.code().unwrap_or(1)));
            }

            // launch cleanup process
            let mut command = b"bash -c \"cat;rm -f \"".to_vec();
            command.extend_from_slice(&tmpfile);
            let trap = self.channel_open_session().await?;
            trap.exec(true, command).await?;
            CleanupGuard::new(trap).spawn();

            // exec
            let mut command = tmpfile;
            command.extend_from_slice(b" ");
            command.extend_from_slice(&args.into());

            let channel = self.channel_open_session().await?;
            channel.exec(true, command).await?;
            channel.eof().await?;

            channel
        };

        let handshake_information: HandshakeInformation = loop {
            let Some(msg) = channel.wait().await else {
                break None;
            };
            match msg {
                ChannelMsg::Data { ref data } => {
                    let line = String::from_utf8_lossy(data);
                    let line = line.split('\n').next().unwrap();
                    let handshake_information: HandshakeInformation = line.parse()?;
                    break Some(handshake_information);
                }
                ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                    let line = String::from_utf8_lossy(data);
                    error!("{}", line);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    return Err(RpcStartError::LaunchFail(exit_status));
                }
                _ => {}
            }
        }
        .ok_or(RpcStartError::HandshakeInformationNotReceived)?;

        let addr = handshake_information.network_addr;

        let stream = self
            .channel_open_direct_tcpip(&addr.ip().to_string(), addr.port() as u32, "localhost", 0)
            .await?;

        Ok(SshRpcSession {
            handshake_information,
            channel,
            stream: stream.into_stream(),
        })
    }
}
