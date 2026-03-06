import SwiftUI
import SwiftTerm

struct TerminalSessionView: NSViewRepresentable {
    @ObservedObject var connection: RelayConnection

    func makeNSView(context: Context) -> TerminalView {
        let terminalView = TerminalView(frame: .zero)
        terminalView.terminalDelegate = context.coordinator

        connection.onData = { data in
            let bytes = [UInt8](data)
            terminalView.feed(byteArray: ArraySlice(bytes))
        }

        return terminalView
    }

    func updateNSView(_ nsView: TerminalView, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(connection: connection)
    }

    class Coordinator: NSObject, TerminalViewDelegate {
        let connection: RelayConnection

        init(connection: RelayConnection) {
            self.connection = connection
        }

        func send(source: TerminalView, data: ArraySlice<UInt8>) {
            connection.sendInput(Data(data))
        }

        func scrolled(source: TerminalView, position: Double) {}

        func setTerminalTitle(source: TerminalView, title: String) {}

        func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
            connection.resize(cols: newCols, rows: newRows)
        }

        func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}

        func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {}

        func bell(source: TerminalView) {}

        func clipboardCopy(source: TerminalView, content: Data) {}

        func iTermContent(source: TerminalView, content: ArraySlice<UInt8>) {}

        func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}
    }
}
