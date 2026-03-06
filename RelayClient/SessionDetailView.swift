import SwiftUI

struct SessionDetailView: View {
    @ObservedObject var connection: RelayConnection
    let sessionId: UUID

    var body: some View {
        ZStack {
            TerminalSessionView(connection: connection)

            if !isAttachedToThis {
                detachedOverlay
            }
        }
        .toolbar {
            ToolbarItemGroup(placement: .automatic) {
                if let info = connection.projectInfos[sessionId] {
                    if let branch = info.gitBranch {
                        Label(branch, systemImage: "arrow.triangle.branch")
                            .font(.caption)
                    }
                    if info.claudeCodeDetected {
                        Image(systemName: "terminal")
                            .foregroundStyle(.purple)
                            .help("Claude Code detected")
                    }
                }

                if isAttachedToThis {
                    Button("Detach") {
                        connection.detach()
                    }
                }

                Button(role: .destructive) {
                    connection.killSession(sessionId: sessionId)
                } label: {
                    Image(systemName: "xmark.circle")
                }
                .help("Kill session")
            }
        }
    }

    private var isAttachedToThis: Bool {
        if case .attached(let id) = connection.state, id == sessionId {
            return true
        }
        return false
    }

    private var detachedOverlay: some View {
        VStack(spacing: 16) {
            Image(systemName: "pause.circle")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)

            Text("Session Detached")
                .font(.title2)
                .foregroundStyle(.secondary)

            Button("Reattach") {
                connection.attach(sessionId: sessionId)
            }
            .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.ultraThinMaterial)
    }
}
