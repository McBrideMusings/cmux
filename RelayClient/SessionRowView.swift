import SwiftUI

struct SessionRowView: View {
    let session: SessionInfo
    let projectInfo: ProjectInfo?
    let isAttached: Bool

    var body: some View {
        HStack(spacing: 8) {
            // State indicator
            Circle()
                .fill(stateColor)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(displayName)
                        .font(.body)
                        .fontWeight(isAttached ? .bold : .regular)

                    if projectInfo?.claudeCodeDetected == true {
                        Image(systemName: "terminal")
                            .font(.caption2)
                            .foregroundStyle(.purple)
                    }
                }

                HStack(spacing: 6) {
                    Text(session.shell.components(separatedBy: "/").last ?? session.shell)
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    if let branch = projectInfo?.gitBranch {
                        Label(branch, systemImage: "arrow.triangle.branch")
                            .font(.caption2)
                            .foregroundStyle(.orange)
                    }
                }
            }

            Spacer()

            Text(session.state)
                .font(.caption2)
                .foregroundStyle(session.state == "attached" ? .green : .secondary)
        }
        .opacity(session.state == "detached" && !isAttached ? 0.7 : 1.0)
    }

    private var displayName: String {
        if let info = projectInfo, !info.projectName.isEmpty {
            return info.projectName
        }
        // Fallback: use last path component of cwd, or session ID
        if !session.cwd.isEmpty {
            return (session.cwd as NSString).lastPathComponent
        }
        return session.id.uuidString.prefix(8).lowercased()
    }

    private var stateColor: Color {
        if isAttached { return .green }
        switch session.state {
        case "attached": return .blue
        case "detached": return .gray
        default: return .gray
        }
    }
}
