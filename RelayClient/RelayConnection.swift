import Foundation
import Combine

enum ConnectionState {
    case disconnected
    case connecting
    case connected
    case attached(sessionId: UUID)
}

struct SessionInfo: Codable, Identifiable {
    let id: UUID
    let shell: String
}

class RelayConnection: ObservableObject {
    @Published var state: ConnectionState = .disconnected
    @Published var lastError: String?

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
    }

    func disconnect() {
        webSocketTask?.cancel(with: .normalClosure, reason: nil)
        webSocketTask = nil
        state = .disconnected
    }

    func createSession(shell: String = "") {
        let shellValue = shell.isEmpty
            ? ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
            : shell
        send(["type": "create_session", "shell": shellValue])
    }

    func attach(sessionId: UUID) {
        send(["type": "attach", "session_id": sessionId.uuidString.lowercased()])
    }

    func detach() {
        send(["type": "detach"])
    }

    func sendInput(_ data: Data) {
        let payload = data.base64EncodedString()
        send(["type": "data", "payload": payload])
    }

    func resize(cols: Int, rows: Int) {
        send(["type": "resize", "cols": cols, "rows": rows])
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
            case "session_created":
                if let session = json["session"] as? [String: Any],
                   let idStr = session["id"] as? String,
                   let id = UUID(uuidString: idStr) {
                    self.attach(sessionId: id)
                }

            case "attached":
                if let idStr = json["session_id"] as? String,
                   let id = UUID(uuidString: idStr) {
                    self.state = .attached(sessionId: id)
                }

            case "detached":
                self.state = .connected

            case "data":
                if let payload = json["payload"] as? String,
                   let decoded = Data(base64Encoded: payload) {
                    self.onData?(decoded)
                }

            case "session_ended":
                self.state = .connected

            case "error":
                self.lastError = json["message"] as? String

            default:
                break
            }
        }
    }
}
