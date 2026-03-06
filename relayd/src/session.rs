use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::protocol::SessionInfo;

pub struct Session {
    pub id: Uuid,
    pub shell: String,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send>,
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id,
            shell: self.shell.clone(),
        }
    }
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

    pub fn spawn(&self, shell: &str, cols: u16, rows: u16) -> std::io::Result<SessionInfo> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(std::io::Error::other)?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(std::io::Error::other)?;

        let id = Uuid::new_v4();
        let info = SessionInfo {
            id,
            shell: shell.to_string(),
        };

        let session = Session {
            id,
            shell: shell.to_string(),
            master: pair.master,
            child,
        };

        self.sessions.lock().unwrap().insert(id, session);
        Ok(info)
    }

    pub fn kill(&self, id: Uuid) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(mut session) = sessions.remove(&id) {
            let _ = session.child.kill();
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

    /// Remove a session from the registry and return it (for attaching).
    pub fn take(&self, id: Uuid) -> Option<Session> {
        self.sessions.lock().unwrap().remove(&id)
    }

    /// Return a session back to the registry (on detach).
    pub fn put_back(&self, session: Session) {
        self.sessions.lock().unwrap().insert(session.id, session);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_and_list() {
        let registry = SessionRegistry::new();
        let info = registry.spawn("/bin/sh", 80, 24).unwrap();
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

    #[test]
    fn spawn_and_kill() {
        let registry = SessionRegistry::new();
        let info = registry.spawn("/bin/sh", 80, 24).unwrap();
        assert!(registry.kill(info.id));
        assert!(registry.list().is_empty());
    }
}
