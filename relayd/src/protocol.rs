use serde::{Deserialize, Serialize};
use uuid::Uuid;

// -- Client → Server --

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    ListSessions,
    CreateSession {
        #[serde(default = "default_shell")]
        shell: String,
    },
    Attach {
        session_id: Uuid,
    },
    Detach,
    KillSession {
        session_id: Uuid,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Data {
        payload: String, // base64-encoded
    },
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

// -- Server → Client --

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Sessions {
        sessions: Vec<SessionInfo>,
    },
    SessionCreated {
        session: SessionInfo,
    },
    Attached {
        session_id: Uuid,
    },
    Detached,
    Data {
        payload: String, // base64-encoded
    },
    SessionEnded {
        session_id: Uuid,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: Uuid,
    pub shell: String,
}
