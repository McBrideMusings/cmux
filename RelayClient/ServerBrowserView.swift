import SwiftUI

struct ServerBrowserView: View {
    @StateObject private var connection = RelayConnection()
    @State private var selectedSessionId: UUID?
    @State private var showConnectSheet = false

    var body: some View {
        NavigationSplitView {
            sidebar
        } detail: {
            detail
        }
        .sheet(isPresented: $showConnectSheet) {
            ConnectSheet(connection: connection, isPresented: $showConnectSheet)
        }
    }

    // MARK: - Sidebar

    private var sidebar: some View {
        List(selection: $selectedSessionId) {
            Section {
                connectionHeader
            }

            if !connection.sessions.isEmpty {
                Section("Sessions") {
                    ForEach(connection.sessions) { session in
                        SessionRowView(
                            session: session,
                            projectInfo: connection.projectInfos[session.id],
                            isAttached: isAttached(to: session.id)
                        )
                        .tag(session.id)
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("Relay")
        .toolbar {
            ToolbarItemGroup(placement: .automatic) {
                if isConnected {
                    Button {
                        connection.createSession()
                    } label: {
                        Image(systemName: "plus")
                    }
                    .help("New session")

                    Button {
                        connection.listSessions()
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .help("Refresh")
                }
            }
        }
        .onChange(of: selectedSessionId) { _, newId in
            handleSessionSelection(newId)
        }
    }

    private var connectionHeader: some View {
        HStack {
            Circle()
                .fill(isConnected ? .green : .red)
                .frame(width: 8, height: 8)

            Text(isConnected ? "Connected" : "Disconnected")
                .font(.caption)
                .foregroundStyle(.secondary)

            Spacer()

            if isConnected {
                Button("Disconnect") {
                    connection.disconnect()
                    selectedSessionId = nil
                }
                .font(.caption)
                .buttonStyle(.plain)
                .foregroundStyle(.red)
            } else {
                Button("Connect") {
                    showConnectSheet = true
                }
                .font(.caption)
                .buttonStyle(.plain)
            }
        }
    }

    // MARK: - Detail

    @ViewBuilder
    private var detail: some View {
        if let sessionId = selectedSessionId, isConnected {
            SessionDetailView(connection: connection, sessionId: sessionId)
        } else if !isConnected {
            ContentUnavailableView(
                "Not Connected",
                systemImage: "network.slash",
                description: Text("Connect to a relay server to manage sessions.")
            )
        } else {
            ContentUnavailableView(
                "No Session Selected",
                systemImage: "terminal",
                description: Text("Select a session from the sidebar or create a new one.")
            )
        }
    }

    // MARK: - Helpers

    private var isConnected: Bool {
        switch connection.state {
        case .disconnected, .connecting:
            return false
        default:
            return true
        }
    }

    private func isAttached(to sessionId: UUID) -> Bool {
        if case .attached(let id) = connection.state, id == sessionId {
            return true
        }
        return false
    }

    private func handleSessionSelection(_ sessionId: UUID?) {
        guard let sessionId else { return }

        // Detach current if attached to a different session
        if case .attached(let currentId) = connection.state, currentId != sessionId {
            connection.detach()
            // Small delay to let detach complete before attaching
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                connection.attach(sessionId: sessionId)
            }
        } else if case .attached(let currentId) = connection.state, currentId == sessionId {
            // Already attached to this session, nothing to do
        } else {
            connection.attach(sessionId: sessionId)
        }
    }
}

// MARK: - Connect Sheet

struct ConnectSheet: View {
    @ObservedObject var connection: RelayConnection
    @Binding var isPresented: Bool
    @State private var host = "localhost"
    @State private var port = "7800"

    var body: some View {
        VStack(spacing: 16) {
            Text("Connect to Relay Server")
                .font(.title2)

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
                    .foregroundStyle(.red)
                    .font(.caption)
            }

            HStack {
                Button("Cancel") {
                    isPresented = false
                }
                .keyboardShortcut(.cancelAction)

                Button("Connect") {
                    let portNum = Int(port) ?? 7800
                    connection.connect(host: host, port: portNum)
                    isPresented = false
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(30)
        .frame(width: 400)
    }
}
