use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info};
use uuid::Uuid;

use crate::protocol::{ClientMessage, ServerMessage};
use crate::session::SessionRegistry;

pub async fn handle_connection(stream: TcpStream, registry: SessionRegistry) {
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
    let mut pty_read_task: Option<tokio::task::JoinHandle<()>> = None;
    let pty_writer: Arc<Mutex<Option<Box<dyn Write + Send>>>> = Arc::new(Mutex::new(None));

    // Channel for PTY output → WebSocket sender
    let (pty_tx, mut pty_rx) = mpsc::channel::<ServerMessage>(256);

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

    // Forward PTY output to the outgoing channel
    let outgoing_tx2 = outgoing_tx.clone();
    let pty_forward_task = tokio::spawn(async move {
        while let Some(msg) = pty_rx.recv().await {
            if outgoing_tx2.send(msg).await.is_err() {
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

            ClientMessage::CreateSession { shell } => match registry.spawn(&shell, 80, 24) {
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
            },

            ClientMessage::Attach { session_id } => {
                // Detach from current session if any
                if attached_session.take().is_some() {
                    if let Some(task) = pty_read_task.take() {
                        task.abort();
                    }
                    *pty_writer.lock().unwrap() = None;
                    let _ = outgoing_tx.send(ServerMessage::Detached).await;
                }

                // Take the session out to get reader/writer
                let session = match registry.take(session_id) {
                    Some(s) => s,
                    None => {
                        let _ = outgoing_tx
                            .send(ServerMessage::Error {
                                message: format!("Session {} not found", session_id),
                            })
                            .await;
                        continue;
                    }
                };

                let reader = match session.master.try_clone_reader() {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = outgoing_tx
                            .send(ServerMessage::Error {
                                message: format!("Failed to get PTY reader: {}", e),
                            })
                            .await;
                        registry.put_back(session);
                        continue;
                    }
                };

                let writer = match session.master.take_writer() {
                    Ok(w) => w,
                    Err(e) => {
                        let _ = outgoing_tx
                            .send(ServerMessage::Error {
                                message: format!("Failed to get PTY writer: {}", e),
                            })
                            .await;
                        registry.put_back(session);
                        continue;
                    }
                };

                // Put session back for resize/kill
                registry.put_back(session);

                *pty_writer.lock().unwrap() = Some(writer);
                attached_session = Some(session_id);
                let _ = outgoing_tx
                    .send(ServerMessage::Attached { session_id })
                    .await;

                // Spawn task to read PTY output
                let pty_tx_clone = pty_tx.clone();
                let mut reader = reader;
                pty_read_task = Some(tokio::task::spawn_blocking(move || {
                    let mut buf = [0u8; 4096];
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                let payload =
                                    base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                                if pty_tx_clone
                                    .blocking_send(ServerMessage::Data { payload })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }));
            }

            ClientMessage::Detach => {
                if attached_session.take().is_some() {
                    if let Some(task) = pty_read_task.take() {
                        task.abort();
                    }
                    *pty_writer.lock().unwrap() = None;
                    let _ = outgoing_tx.send(ServerMessage::Detached).await;
                }
            }

            ClientMessage::KillSession { session_id } => {
                if attached_session == Some(session_id) {
                    attached_session = None;
                    if let Some(task) = pty_read_task.take() {
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
                        if let Some(ref mut writer) = *pty_writer.lock().unwrap() {
                            let _ = writer.write_all(&bytes);
                        }
                    }
                }
            }
        }
    }

    // Cleanup
    if let Some(task) = pty_read_task.take() {
        task.abort();
    }
    drop(outgoing_tx);
    drop(pty_tx);
    let _ = send_task.await;
    let _ = pty_forward_task.await;

    info!("WebSocket connection closed");
}
