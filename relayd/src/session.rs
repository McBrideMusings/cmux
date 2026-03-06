use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::info;
use uuid::Uuid;

use crate::protocol::SessionInfo;

const DEFAULT_SCROLLBACK_SIZE: usize = 64 * 1024; // 64 KB

/// Fixed-size ring buffer for PTY scrollback.
pub struct ScrollbackBuffer {
    buf: Vec<u8>,
    capacity: usize,
    write_pos: usize,
    len: usize,
}

impl ScrollbackBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0u8; capacity],
            capacity,
            write_pos: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        for &byte in data {
            self.buf[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % self.capacity;
            if self.len < self.capacity {
                self.len += 1;
            }
        }
    }

    pub fn snapshot(&self) -> Vec<u8> {
        if self.len == 0 {
            return Vec::new();
        }
        if self.len < self.capacity {
            // Haven't wrapped yet
            self.buf[..self.len].to_vec()
        } else {
            // Wrapped: read from write_pos (oldest) to end, then start to write_pos
            let mut result = Vec::with_capacity(self.capacity);
            result.extend_from_slice(&self.buf[self.write_pos..]);
            result.extend_from_slice(&self.buf[..self.write_pos]);
            result
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Detached,
    Attached { client_count: usize },
}

pub struct Session {
    pub id: Uuid,
    pub shell: String,
    pub cwd: PathBuf,
    pub state: SessionState,
    pub scrollback: Arc<Mutex<ScrollbackBuffer>>,
    #[allow(dead_code)]
    pub created_at: Instant,
    pub pty_output_tx: broadcast::Sender<Vec<u8>>,
    pub reader_task: Option<JoinHandle<()>>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send>,
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        let state_str = match &self.state {
            SessionState::Detached => "detached".to_string(),
            SessionState::Attached { .. } => "attached".to_string(),
        };
        SessionInfo {
            id: self.id,
            shell: self.shell.clone(),
            state: state_str,
            cwd: self.cwd.to_string_lossy().to_string(),
        }
    }
}

/// Returned from `attach()` with everything a client needs.
pub struct AttachHandle {
    pub receiver: broadcast::Receiver<Vec<u8>>,
    pub scrollback_snapshot: Vec<u8>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

#[derive(Clone)]
pub struct SessionRegistry {
    sessions: Arc<Mutex<HashMap<Uuid, Session>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn spawn(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
        cwd: Option<PathBuf>,
    ) -> std::io::Result<SessionInfo> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(std::io::Error::other)?;

        let working_dir = cwd.unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
        });

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        cmd.cwd(&working_dir);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(std::io::Error::other)?;

        let id = Uuid::new_v4();

        let writer = pair
            .master
            .take_writer()
            .map_err(std::io::Error::other)?;
        let writer = Arc::new(Mutex::new(writer));

        let scrollback = Arc::new(Mutex::new(ScrollbackBuffer::new(DEFAULT_SCROLLBACK_SIZE)));

        let (pty_output_tx, _) = broadcast::channel::<Vec<u8>>(256);

        // Spawn persistent PTY reader task
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(std::io::Error::other)?;
        let scrollback_clone = scrollback.clone();
        let tx_clone = pty_output_tx.clone();
        let reader_task = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        scrollback_clone.lock().unwrap().push(&data);
                        // Ignore send errors — no receivers is fine (detached)
                        let _ = tx_clone.send(data);
                    }
                    Err(_) => break,
                }
            }
        });

        let session = Session {
            id,
            shell: shell.to_string(),
            cwd: working_dir,
            state: SessionState::Detached,
            scrollback,
            created_at: Instant::now(),
            pty_output_tx,
            reader_task: Some(reader_task),
            writer,
            master: pair.master,
            child,
        };

        let info = session.info();
        self.sessions.lock().unwrap().insert(id, session);
        Ok(info)
    }

    pub fn kill(&self, id: Uuid) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(mut session) = sessions.remove(&id) {
            let _ = session.child.kill();
            if let Some(task) = session.reader_task.take() {
                task.abort();
            }
            true
        } else {
            false
        }
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| s.info())
            .collect()
    }

    /// Attach to a session: subscribe to broadcast, get scrollback + writer handle.
    pub fn attach(&self, id: Uuid) -> Option<AttachHandle> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.get_mut(&id)?;

        let receiver = session.pty_output_tx.subscribe();
        let scrollback_snapshot = session.scrollback.lock().unwrap().snapshot();
        let writer = session.writer.clone();

        match &mut session.state {
            SessionState::Detached => {
                session.state = SessionState::Attached { client_count: 1 };
            }
            SessionState::Attached { client_count } => {
                *client_count += 1;
            }
        }

        Some(AttachHandle {
            receiver,
            scrollback_snapshot,
            writer,
        })
    }

    /// Detach from a session: decrement client count, update state.
    pub fn detach(&self, id: Uuid) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(&id) {
            match &mut session.state {
                SessionState::Attached { client_count } => {
                    if *client_count <= 1 {
                        session.state = SessionState::Detached;
                    } else {
                        *client_count -= 1;
                    }
                }
                SessionState::Detached => {}
            }
        }
    }

    pub fn resize(&self, id: Uuid, cols: u16, rows: u16) -> bool {
        let sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get(&id) {
            session
                .master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .is_ok()
        } else {
            false
        }
    }

    /// Remove sessions whose child process has exited.
    pub fn cleanup_dead(&self) -> Vec<Uuid> {
        let mut sessions = self.sessions.lock().unwrap();
        let mut dead = Vec::new();

        for (id, session) in sessions.iter_mut() {
            match session.child.try_wait() {
                Ok(Some(_status)) => {
                    dead.push(*id);
                }
                Ok(None) => {} // still running
                Err(_) => {
                    dead.push(*id);
                }
            }
        }

        for id in &dead {
            if let Some(mut session) = sessions.remove(id) {
                if let Some(task) = session.reader_task.take() {
                    task.abort();
                }
                info!("Cleaned up dead session {}", id);
            }
        }

        dead
    }

    /// Get the cwd for a session.
    pub fn get_cwd(&self, id: Uuid) -> Option<PathBuf> {
        self.sessions
            .lock()
            .unwrap()
            .get(&id)
            .map(|s| s.cwd.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollback_buffer_basic() {
        let mut buf = ScrollbackBuffer::new(8);
        buf.push(b"hello");
        assert_eq!(buf.snapshot(), b"hello");
    }

    #[test]
    fn scrollback_buffer_wrap() {
        let mut buf = ScrollbackBuffer::new(4);
        buf.push(b"abcdef");
        // capacity=4, wrote 6 bytes → oldest 2 dropped
        assert_eq!(buf.snapshot(), b"cdef");
    }

    #[tokio::test]
    async fn spawn_and_list() {
        let registry = SessionRegistry::new();
        let info = registry.spawn("/bin/sh", 80, 24, None).unwrap();
        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, info.id);
        registry.kill(info.id);
    }

    #[test]
    fn kill_returns_false_for_missing() {
        let registry = SessionRegistry::new();
        assert!(!registry.kill(Uuid::new_v4()));
    }

    #[tokio::test]
    async fn spawn_and_kill() {
        let registry = SessionRegistry::new();
        let info = registry.spawn("/bin/sh", 80, 24, None).unwrap();
        assert!(registry.kill(info.id));
        assert!(registry.list().is_empty());
    }

    #[tokio::test]
    async fn attach_and_detach() {
        let registry = SessionRegistry::new();
        let info = registry.spawn("/bin/sh", 80, 24, None).unwrap();

        let handle = registry.attach(info.id).unwrap();
        assert!(!handle.scrollback_snapshot.is_empty() || handle.scrollback_snapshot.is_empty());

        {
            let sessions = registry.sessions.lock().unwrap();
            let session = sessions.get(&info.id).unwrap();
            assert_eq!(session.state, SessionState::Attached { client_count: 1 });
        }

        registry.detach(info.id);
        {
            let sessions = registry.sessions.lock().unwrap();
            let session = sessions.get(&info.id).unwrap();
            assert_eq!(session.state, SessionState::Detached);
        }

        registry.kill(info.id);
    }
}
