use anyhow::Result;
use futures_util::StreamExt;
use futures_util::{future, Future};
use russh::*;
use sshrpc::client::SshRpcExt;
use sshrpc::{Error};
use std::io::Write;
///
/// Run this example with:
/// cargo run --example simple <host>:<port> <user> <password>
///
use std::sync::Arc;
use std::time::Duration;
use tarpc::context;
use tarpc::server::Channel;

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}

// implement of tarpc
#[tarpc::service]
pub trait World {
    async fn hello(name: String) -> Result<String, Error>;
}

#[derive(Clone)]
pub struct HelloServer;

impl World for HelloServer {
    async fn hello(self, _: context::Context, name: String) -> Result<String, Error> {
        eprintln!(" INFO(server): called hello({})", name);
        Ok(format!("Hello, {}!", name))
    }
}

// Implement for russh
struct Client {}

#[async_trait::async_trait]
impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[allow(dead_code)]
async fn do_server() -> Result<(), anyhow::Error> {
    let mut listener = sshrpc::transport::listen(1).await?;

    listener.config_mut().max_frame_length(usize::MAX);

    listener
        // Ignore accept errors.
        .filter_map(|r| future::ready(r.ok()))
        .map(tarpc::server::BaseChannel::with_defaults)
        .map(|channel| {
            let server = HelloServer;
            channel.execute(server.serve()).for_each(spawn)
        })
        // only one for single connection
        .take(1)
        .buffer_unordered(1)
        .for_each(|_| async {})
        .await;
    Ok(())
}

async fn do_client(server: &str, user: &str, pass: &str) -> Result<(), anyhow::Error> {
    let config = client::Config {
        inactivity_timeout: Some(Duration::from_secs(5)),
        ..<_>::default()
    };
    let config = Arc::new(config);

    let mut session = client::connect(config, server, Client {}).await?;

    session.authenticate_password(user, pass).await?;

    // Use this example serever
    let bin = tokio::fs::File::open("/proc/self/exe").await?;
    let s = session.exec_rpc_server(bin, "").await?;
    let (mut channel, transport) = s.try_into_transport(1)?;

    let client = WorldClient::new(tarpc::client::Config::default(), transport).spawn();

    // Call the hello service on background
    tokio::spawn(async move {
        println!(
            "{}",
            client
                .hello(context::current(), "students".to_string())
                .await
                .unwrap()
                .unwrap()
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!(
            "{}",
            client
                .hello(context::current(), "teacher".to_string())
                .await
                .unwrap()
                .unwrap()
        );
    });

    let mut code = None;
    let mut stdout = std::io::stdout().lock();

    loop {
        // There's an event available on the session channel
        let Some(msg) = channel.wait().await else {
            break;
        };
        match msg {
            // stdout of server
            ChannelMsg::Data { ref data } => {
                stdout.write_all(data)?;
                stdout.flush()?;
            }
            // stderr of server
            ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                stdout.write_all(data)?;
                stdout.flush()?;
            }
            // exit code of server
            ChannelMsg::ExitStatus { exit_status } => {
                code = Some(exit_status);
                // cannot leave the loop immediately, there might still be more data to receive
            }
            _ => {}
        }
    }

    println!("Exit code: {:?}", code);

    session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await?;

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() == 4 {
        do_client(&args[1], &args[2], &args[3]).await
    } else {
        do_server().await
    }
}
