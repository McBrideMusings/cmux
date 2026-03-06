import SwiftUI

struct ConnectView: View {
    @StateObject private var connection = RelayConnection()
    @State private var host = "localhost"
    @State private var port = "7800"

    var body: some View {
        switch connection.state {
        case .disconnected, .connecting:
            connectForm
        case .connected:
            connectForm
        case .attached:
            TerminalSessionView(connection: connection)
        }
    }

    private var connectForm: some View {
        VStack(spacing: 16) {
            Text("Relay Client")
                .font(.title)

            HStack {
                TextField("Host", text: $host)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 200)

                TextField("Port", text: $port)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 80)
            }

            if let error = connection.lastError {
                Text(error)
                    .foregroundColor(.red)
                    .font(.caption)
            }

            Button(isConnected ? "Create Session" : "Connect") {
                if isConnected {
                    connection.createSession()
                } else {
                    let portNum = Int(port) ?? 7800
                    connection.connect(host: host, port: portNum)
                }
            }
            .keyboardShortcut(.defaultAction)
            .disabled(isConnecting)
        }
        .padding(40)
        .frame(minWidth: 400, minHeight: 300)
    }

    private var isConnected: Bool {
        if case .connected = connection.state { return true }
        return false
    }

    private var isConnecting: Bool {
        if case .connecting = connection.state { return true }
        return false
    }
}
