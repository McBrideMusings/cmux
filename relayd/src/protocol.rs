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
        cwd: Option<String>,
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
    GetProjectInfo {
        session_id: Uuid,
    },
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

// -- Server → Client --

#[derive(Debug, Serialize, Clone)]
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
    ProjectInfo {
        info: ProjectInfo,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: Uuid,
    pub shell: String,
    pub state: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectInfo {
    pub session_id: Uuid,
    pub project_name: String,
    pub git_branch: Option<String>,
    pub session_state: String,
    pub cwd: String,
    pub claude_code_detected: bool,
}
