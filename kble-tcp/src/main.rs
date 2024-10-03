use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing_subscriber::{prelude::*, EnvFilter};

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Listener {
        #[clap(long, short, default_value_t = Ipv4Addr::UNSPECIFIED.into())]
        ip: IpAddr,
        port: u16,
    },
    Client {
        addr: SocketAddr,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::io::stderr),
        )
        .with(EnvFilter::from_default_env())
        .init();
    let args = Args::parse();

    match args.command {
        Commands::Listener { ip, port } => run_listener(ip, port).await,
        Commands::Client { addr } => run_client(addr).await,
    }
}

async fn run_listener(ip: IpAddr, port: u16) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", ip, port));
    let listener = listener.await?;
    loop {
        let (tcp_stream, _) = listener.accept().await?;
        let (mut tcp_upstream, mut tcp_downstream) = tokio::io::split(tcp_stream);
        let (mut tx, mut rx) = kble_socket::from_stdio().await;
        let to_tcp = async {
            while let Some(body) = rx.next().await {
                let body = body?;
                tcp_downstream.write_all(&body).await?;
            }
            anyhow::Ok(())
        };
        let from_tcp = async {
            let mut buffer = [0; 8192];
            loop {
                match tcp_upstream.read(&mut buffer).await? {
                    0 => break,
                    n => {
                        tx.send(buffer[..n].to_vec().into()).await?;
                    }
                }
            }
            anyhow::Ok(())
        };
        tokio::select! {
            _ = to_tcp => {
                continue;
            },
            _ = from_tcp => {
                continue;
            }
        }
    }
}

async fn run_client(addr: SocketAddr) -> Result<()> {
    let tcp_stream = TcpStream::connect(addr).await?;
    let (mut tcp_upstream, mut tcp_downstream) = tokio::io::split(tcp_stream);
    let (mut tx, mut rx) = kble_socket::from_stdio().await;
    let to_tcp = async {
        while let Some(body) = rx.next().await {
            let body = body?;
            tcp_downstream.write_all(&body).await?;
        }
        anyhow::Ok(())
    };
    let from_tcp = async {
        let mut buffer = [0; 8192];
        loop {
            match tcp_upstream.read(&mut buffer).await? {
                0 => break,
                n => {
                    tx.send(buffer[..n].to_vec().into()).await?;
                }
            }
        }
        anyhow::Ok(())
    };
    tokio::select! {
        _ = to_tcp => {
            std::process::exit(0);
        },
        _ = from_tcp => {
            std::process::exit(0);
        }
    }
}
