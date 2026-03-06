use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info};
use uuid::Uuid;

use crate::claude_detect::ClaudeDetector;
use crate::project;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::session::SessionRegistry;

pub async fn handle_connection(
    stream: TcpStream,
    registry: SessionRegistry,
    claude_detector: ClaudeDetector,
) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    info!("New WebSocket connection");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let mut attached_session: Option<Uuid> = None;
    let mut broadcast_forward_task: Option<tokio::task::JoinHandle<()>> = None;
    type WriterHandle = Arc<Mutex<Box<dyn Write + Send>>>;
    let pty_writer: Arc<Mutex<Option<WriterHandle>>> = Arc::new(Mutex::new(None));

    // Outgoing message channel
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<ServerMessage>(256);

    let send_task = tokio::spawn(async move {
        while let Some(msg) = outgoing_rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    continue;
                }
            };
            if ws_sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_receiver.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let client_msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                let _ = outgoing_tx
                    .send(ServerMessage::Error {
                        message: format!("Invalid message: {}", e),
                    })
                    .await;
                continue;
            }
        };

        match client_msg {
            ClientMessage::ListSessions => {
                let sessions = registry.list();
                let _ = outgoing_tx
                    .send(ServerMessage::Sessions { sessions })
                    .await;
            }

            ClientMessage::CreateSession { shell, cwd } => {
                let cwd_path = cwd.map(PathBuf::from);
                match registry.spawn(&shell, 80, 24, cwd_path) {
                    Ok(info) => {
                        let _ = outgoing_tx
                            .send(ServerMessage::SessionCreated { session: info })
                            .await;
                    }
                    Err(e) => {
                        let _ = outgoing_tx
                            .send(ServerMessage::Error {
                                message: format!("Failed to create session: {}", e),
                            })
                            .await;
                    }
                }
            }

            ClientMessage::Attach { session_id } => {
                // Detach from current session if any
                if let Some(prev_id) = attached_session.take() {
                    if let Some(task) = broadcast_forward_task.take() {
                        task.abort();
                    }
                    *pty_writer.lock().unwrap() = None;
                    registry.detach(prev_id);
                    let _ = outgoing_tx.send(ServerMessage::Detached).await;
                }

                // Attach to the requested session
                let handle = match registry.attach(session_id) {
                    Some(h) => h,
                    None => {
                        let _ = outgoing_tx
                            .send(ServerMessage::Error {
                                message: format!("Session {} not found", session_id),
                            })
                            .await;
                        continue;
                    }
                };

                *pty_writer.lock().unwrap() = Some(handle.writer);
                attached_session = Some(session_id);

                let _ = outgoing_tx
                    .send(ServerMessage::Attached { session_id })
                    .await;

                // Send scrollback snapshot as initial data
                if !handle.scrollback_snapshot.is_empty() {
                    let payload = base64::engine::general_purpose::STANDARD
                        .encode(&handle.scrollback_snapshot);
                    let _ = outgoing_tx
                        .send(ServerMessage::Data { payload })
                        .await;
                }

                // Send project info automatically on attach
                if registry.get_cwd(session_id).is_some() {
                    let info =
                        project::build_project_info(session_id, &registry, &claude_detector);
                    let _ = outgoing_tx
                        .send(ServerMessage::ProjectInfo { info })
                        .await;
                }

                // Spawn task to forward broadcast → outgoing
                let mut receiver = handle.receiver;
                let outgoing_tx_clone = outgoing_tx.clone();
                broadcast_forward_task = Some(tokio::spawn(async move {
                    loop {
                        match receiver.recv().await {
                            Ok(data) => {
                                let payload =
                                    base64::engine::general_purpose::STANDARD.encode(&data);
                                if outgoing_tx_clone
                                    .send(ServerMessage::Data { payload })
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                // Missed some messages, continue
                                info!("Broadcast receiver lagged by {} messages", n);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }));
            }

            ClientMessage::Detach => {
                if let Some(session_id) = attached_session.take() {
                    if let Some(task) = broadcast_forward_task.take() {
                        task.abort();
                    }
                    *pty_writer.lock().unwrap() = None;
                    registry.detach(session_id);
                    let _ = outgoing_tx.send(ServerMessage::Detached).await;
                }
            }

            ClientMessage::KillSession { session_id } => {
                if attached_session == Some(session_id) {
                    attached_session = None;
                    if let Some(task) = broadcast_forward_task.take() {
                        task.abort();
                    }
                    *pty_writer.lock().unwrap() = None;
                }

                if registry.kill(session_id) {
                    let _ = outgoing_tx
                        .send(ServerMessage::SessionEnded { session_id })
                        .await;
                } else {
                    let _ = outgoing_tx
                        .send(ServerMessage::Error {
                            message: format!("Session {} not found", session_id),
                        })
                        .await;
                }
            }

            ClientMessage::Resize { cols, rows } => {
                if let Some(session_id) = attached_session {
                    registry.resize(session_id, cols, rows);
                }
            }

            ClientMessage::Data { payload } => {
                if attached_session.is_some() {
                    if let Ok(bytes) =
                        base64::engine::general_purpose::STANDARD.decode(&payload)
                    {
                        let writer_guard = pty_writer.lock().unwrap();
                        if let Some(ref writer) = *writer_guard {
                            let _ = writer.lock().unwrap().write_all(&bytes);
                        }
                    }
                }
            }

            ClientMessage::GetProjectInfo { session_id } => {
                let info =
                    project::build_project_info(session_id, &registry, &claude_detector);
                let _ = outgoing_tx
                    .send(ServerMessage::ProjectInfo { info })
                    .await;
            }
        }
    }

    // Cleanup: detach if still attached
    if let Some(session_id) = attached_session.take() {
        if let Some(task) = broadcast_forward_task.take() {
            task.abort();
        }
        registry.detach(session_id);
    }
    drop(outgoing_tx);
    let _ = send_task.await;

    info!("WebSocket connection closed");
}
