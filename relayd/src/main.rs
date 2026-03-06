mod protocol;
mod session;
mod ws;

use clap::Parser;
use tokio::net::TcpListener;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "relayd", about = "Terminal relay daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "7800")]
    port: u16,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("relayd=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    let addr = format!("0.0.0.0:{}", args.port);

    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
    info!("relayd listening on {}", addr);

    let registry = session::SessionRegistry::new();

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                info!("Connection from {}", peer);
                let registry = registry.clone();
                tokio::spawn(async move {
                    ws::handle_connection(stream, registry).await;
                });
            }
            Err(e) => {
                error!("Accept failed: {}", e);
            }
        }
    }
}
