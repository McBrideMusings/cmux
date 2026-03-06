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
{"type": "create_session", "shell": "/bin/zsh"}
```

`shell` is optional; defaults to `$SHELL` or `/bin/sh`.

### attach

Attach to a session and start streaming its output.

```json
{"type": "attach", "session_id": "uuid"}
```

### detach

Detach from the current session without killing it.

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

## Server to Client

### sessions

Response to `list_sessions`.

```json
{"type": "sessions", "sessions": [{"id": "uuid", "shell": "/bin/zsh"}]}
```

### session_created

Response to `create_session`.

```json
{"type": "session_created", "session": {"id": "uuid", "shell": "/bin/zsh"}}
```

### attached

Confirms attachment to a session.

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

### error

Error response.

```json
{"type": "error", "message": "description"}
```
