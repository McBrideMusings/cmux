mod claude_detect;
mod project;
mod protocol;
mod session;
mod ws;

use std::time::Duration;

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

    // Start Claude process detector
    let claude_detector = claude_detect::ClaudeDetector::new();
    claude_detector.start_polling(Duration::from_secs(10));

    // Start dead session cleanup task
    let cleanup_registry = registry.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let dead = cleanup_registry.cleanup_dead();
            for id in dead {
                info!("Session {} process exited, removed", id);
            }
        }
    });

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                info!("Connection from {}", peer);
                let registry = registry.clone();
                let detector = claude_detector.clone();
                tokio::spawn(async move {
                    ws::handle_connection(stream, registry, detector).await;
                });
            }
            Err(e) => {
                error!("Accept failed: {}", e);
            }
        }
    }
}
