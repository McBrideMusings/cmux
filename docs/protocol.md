# Relay Protocol

The relay protocol runs over a single WebSocket connection between a client and `relayd`. All messages are JSON text frames.

## Client to Server

### list_sessions

Request the list of active sessions.

```json
{"type": "list_sessions"}
```

### create_session

Spawn a new PTY session.

```json
{"type": "create_session", "shell": "/bin/zsh", "cwd": "/path/to/dir"}
```

`shell` is optional; defaults to `$SHELL` or `/bin/sh`.
`cwd` is optional; defaults to the server's current directory.

### attach

Attach to a session and start streaming its output. On attach, the server sends any buffered scrollback as an initial `data` message before the live stream begins.

```json
{"type": "attach", "session_id": "uuid"}
```

### detach

Detach from the current session without killing it. The session continues running and buffering output.

```json
{"type": "detach"}
```

### kill_session

Terminate a session.

```json
{"type": "kill_session", "session_id": "uuid"}
```

### resize

Resize the attached session's PTY.

```json
{"type": "resize", "cols": 80, "rows": 24}
```

### data

Send input to the attached session's PTY. Payload is base64-encoded.

```json
{"type": "data", "payload": "base64..."}
```

### get_project_info

Request project metadata for a session.

```json
{"type": "get_project_info", "session_id": "uuid"}
```

## Server to Client

### sessions

Response to `list_sessions`.

```json
{"type": "sessions", "sessions": [{"id": "uuid", "shell": "/bin/zsh", "state": "attached", "cwd": "/path"}]}
```

Each session includes:
- `id` — session UUID
- `shell` — shell command
- `state` — `"attached"` or `"detached"`
- `cwd` — working directory

### session_created

Response to `create_session`.

```json
{"type": "session_created", "session": {"id": "uuid", "shell": "/bin/zsh", "state": "detached", "cwd": "/path"}}
```

### attached

Confirms attachment to a session. Followed by a scrollback `data` message (if any buffered output exists) and then a `project_info` message.

```json
{"type": "attached", "session_id": "uuid"}
```

### detached

Confirms detachment.

```json
{"type": "detached"}
```

### data

Terminal output from the attached session. Payload is base64-encoded.

```json
{"type": "data", "payload": "base64..."}
```

### session_ended

A session was terminated.

```json
{"type": "session_ended", "session_id": "uuid"}
```

### project_info

Project metadata for a session. Sent automatically on attach, or in response to `get_project_info`.

```json
{
  "type": "project_info",
  "info": {
    "session_id": "uuid",
    "project_name": "my-project",
    "git_branch": "main",
    "session_state": "attached",
    "cwd": "/path/to/project",
    "claude_code_detected": true
  }
}
```

### error

Error response.

```json
{"type": "error", "message": "description"}
```
