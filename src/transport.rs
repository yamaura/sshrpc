use crate::{HandshakeInformation, NetworkType, Protcol};
use serde::{Deserialize, Serialize};
use std::io::Write;
use tarpc::serde_transport::tcp::Incoming;
use tarpc::serde_transport::Transport;
use tarpc::tokio_serde::formats::Bincode;
use tokio::io::{AsyncRead, AsyncWrite};

/// create listener
/// This function is print the information to client, so you don't need care it.
pub async fn listen<Item, SinkItem>(
    app_protocol_version: u32,
) -> Result<
    Incoming<Item, SinkItem, Bincode<Item, SinkItem>, impl Fn() -> Bincode<Item, SinkItem>>,
    std::io::Error,
>
where
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
{
    let addr = "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap();
    let listener: Incoming<Item, SinkItem, _, _> =
        tarpc::serde_transport::tcp::listen(&addr, Bincode::default).await?;

    let info = HandshakeInformation {
        core_protcol_version: 1,
        app_protocol_version,
        network_type: NetworkType::Tcp,
        network_addr: listener.local_addr(),
        protcol: Protcol::TarpcBincode,
    };

    std::io::stdout()
        .lock()
        .write_fmt(format_args!("{}\n", info))?;
    std::io::stdout().lock().flush()?;
    Ok(listener)
}

pub(crate) fn stream2transport<S, Item, SinkItem>(
    stream: S,
) -> Transport<S, Item, SinkItem, Bincode<Item, SinkItem>>
where
    S: AsyncWrite + AsyncRead,
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
{
    tarpc::serde_transport::new(
        tokio_util::codec::Framed::new(stream, tokio_util::codec::LengthDelimitedCodec::new()),
        Bincode::default(),
    )
}
