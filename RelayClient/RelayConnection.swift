import Foundation
import Combine

enum ConnectionState: Equatable {
    case disconnected
    case connecting
    case connected
    case attaching(sessionId: UUID)
    case attached(sessionId: UUID)
    case detaching
}

struct SessionInfo: Identifiable, Equatable {
    let id: UUID
    let shell: String
    let state: String
    let cwd: String
}

struct ProjectInfo: Equatable {
    let sessionId: UUID
    let projectName: String
    let gitBranch: String?
    let sessionState: String
    let cwd: String
    let claudeCodeDetected: Bool
}

class RelayConnection: ObservableObject {
    @Published var state: ConnectionState = .disconnected
    @Published var lastError: String?
    @Published var sessions: [SessionInfo] = []
    @Published var projectInfos: [UUID: ProjectInfo] = [:]

    private var webSocketTask: URLSessionWebSocketTask?
    private var urlSession: URLSession = .shared

    var onData: ((Data) -> Void)?

    func connect(host: String, port: Int) {
        guard case .disconnected = state else { return }
        state = .connecting
        lastError = nil

        guard let url = URL(string: "ws://\(host):\(port)/ws") else {
            state = .disconnected
            lastError = "Invalid URL"
            return
        }

        let task = urlSession.webSocketTask(with: url)
        webSocketTask = task
        task.resume()

        state = .connected
        receiveLoop()

        // Auto-fetch session list on connect
        listSessions()
    }

    func disconnect() {
        webSocketTask?.cancel(with: .normalClosure, reason: nil)
        webSocketTask = nil
        state = .disconnected
        sessions = []
        projectInfos = [:]
    }

    func listSessions() {
        send(["type": "list_sessions"])
    }

    func createSession(shell: String = "", cwd: String? = nil) {
        let shellValue = shell.isEmpty
            ? ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
            : shell
        var msg: [String: Any] = ["type": "create_session", "shell": shellValue]
        if let cwd = cwd {
            msg["cwd"] = cwd
        }
        send(msg)
    }

    func attach(sessionId: UUID) {
        state = .attaching(sessionId: sessionId)
        send(["type": "attach", "session_id": sessionId.uuidString.lowercased()])
    }

    func detach() {
        state = .detaching
        send(["type": "detach"])
    }

    func killSession(sessionId: UUID) {
        send(["type": "kill_session", "session_id": sessionId.uuidString.lowercased()])
    }

    func sendInput(_ data: Data) {
        let payload = data.base64EncodedString()
        send(["type": "data", "payload": payload])
    }

    func resize(cols: Int, rows: Int) {
        send(["type": "resize", "cols": cols, "rows": rows])
    }

    func getProjectInfo(sessionId: UUID) {
        send(["type": "get_project_info", "session_id": sessionId.uuidString.lowercased()])
    }

    // MARK: - Private

    private func send(_ dict: [String: Any]) {
        guard let jsonData = try? JSONSerialization.data(withJSONObject: dict),
              let text = String(data: jsonData, encoding: .utf8) else {
            return
        }
        webSocketTask?.send(.string(text)) { [weak self] error in
            if let error {
                DispatchQueue.main.async {
                    self?.lastError = error.localizedDescription
                }
            }
        }
    }

    private func receiveLoop() {
        webSocketTask?.receive { [weak self] result in
            guard let self else { return }

            switch result {
            case .success(let message):
                switch message {
                case .string(let text):
                    self.handleMessage(text)
                case .data(let data):
                    if let text = String(data: data, encoding: .utf8) {
                        self.handleMessage(text)
                    }
                @unknown default:
                    break
                }
                self.receiveLoop()

            case .failure(let error):
                DispatchQueue.main.async {
                    self.state = .disconnected
                    self.lastError = error.localizedDescription
                }
            }
        }
    }

    private func handleMessage(_ text: String) {
        guard let data = text.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = json["type"] as? String else {
            return
        }

        DispatchQueue.main.async {
            switch type {
            case "sessions":
                if let list = json["sessions"] as? [[String: Any]] {
                    self.sessions = list.compactMap { dict in
                        guard let idStr = dict["id"] as? String,
                              let id = UUID(uuidString: idStr),
                              let shell = dict["shell"] as? String else {
                            return nil
                        }
                        let state = dict["state"] as? String ?? "unknown"
                        let cwd = dict["cwd"] as? String ?? ""
                        return SessionInfo(id: id, shell: shell, state: state, cwd: cwd)
                    }
                }

            case "session_created":
                if let session = json["session"] as? [String: Any],
                   let idStr = session["id"] as? String,
                   let id = UUID(uuidString: idStr) {
                    // Refresh session list, then attach
                    self.listSessions()
                    self.attach(sessionId: id)
                }

            case "attached":
                if let idStr = json["session_id"] as? String,
                   let id = UUID(uuidString: idStr) {
                    self.state = .attached(sessionId: id)
                    // Refresh session list to get updated states
                    self.listSessions()
                }

            case "detached":
                self.state = .connected
                self.listSessions()

            case "data":
                if let payload = json["payload"] as? String,
                   let decoded = Data(base64Encoded: payload) {
                    self.onData?(decoded)
                }

            case "session_ended":
                if let idStr = json["session_id"] as? String,
                   let id = UUID(uuidString: idStr) {
                    self.projectInfos.removeValue(forKey: id)
                    if case .attached(let currentId) = self.state, currentId == id {
                        self.state = .connected
                    }
                    self.listSessions()
                }

            case "project_info":
                if let infoDict = json["info"] as? [String: Any],
                   let idStr = infoDict["session_id"] as? String,
                   let id = UUID(uuidString: idStr) {
                    let info = ProjectInfo(
                        sessionId: id,
                        projectName: infoDict["project_name"] as? String ?? "Unknown",
                        gitBranch: infoDict["git_branch"] as? String,
                        sessionState: infoDict["session_state"] as? String ?? "unknown",
                        cwd: infoDict["cwd"] as? String ?? "",
                        claudeCodeDetected: infoDict["claude_code_detected"] as? Bool ?? false
                    )
                    self.projectInfos[id] = info
                }

            case "error":
                self.lastError = json["message"] as? String

            default:
                break
            }
        }
    }
}
