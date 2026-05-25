import AppKit

@MainActor
final class VikerExampleAppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var window: NSWindow?
    private var editorContent: VikerExampleEditorContent?
    private var errorContent: VikerExampleEditorErrorContent?
    private var lspSession: VikerExampleLspWorkspaceSession?

    func applicationDidFinishLaunching(_ notification: Notification) {
        installMenu()
        openEditor(url: initialURL())
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func windowWillClose(_ notification: Notification) {
        editorContent?.willClose()
        lspSession?.stop()
        editorContent = nil
        errorContent = nil
        lspSession = nil
    }

    @objc private func openDocument(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.begin { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.openEditor(url: url)
        }
    }

    @objc private func saveDocument(_ sender: Any?) {
        editorContent?.save()
    }

    private func openEditor(url: URL, row: UInt64? = nil, column: UInt64? = nil) {
        let standardizedURL = url.standardizedFileURL
        let window = ensureWindow()
        editorContent?.willClose()
        editorContent = nil
        errorContent = nil

        do {
            let editor = try VikerExampleEditorContent(url: standardizedURL)
            editor.onTitleChange = { [weak self] title in
                self?.window?.title = title
            }
            editor.onOpenFile = { [weak self] url in
                self?.openEditor(url: url)
            }
            editor.onOpenLocation = { [weak self] location in
                self?.openEditor(url: location.url, row: location.row, column: location.column)
            }
            editor.onFileURLChange = { [weak self] url in
                self?.window?.title = url.lastPathComponent.isEmpty ? url.path : url.lastPathComponent
            }

            attachLspIfAvailable(to: editor, documentURL: standardizedURL)
            window.contentView = editor.view
            window.title = editor.title
            editorContent = editor

            if let row {
                editor.jumpTo(row: row, column: column ?? 0)
            }
            window.makeKeyAndOrderFront(nil)
            DispatchQueue.main.async {
                editor.makeFirstResponder()
            }
        } catch {
            let errorContent = VikerExampleEditorErrorContent(url: standardizedURL, error: error)
            window.contentView = errorContent.view
            window.title = errorContent.title
            self.errorContent = errorContent
            window.makeKeyAndOrderFront(nil)
        }
    }

    private func attachLspIfAvailable(to editor: VikerExampleEditorContent, documentURL: URL) {
        let rootURL = documentURL.deletingLastPathComponent().standardizedFileURL
        editor.setWorkspaceRoot(rootURL)

        do {
            if lspSession?.rootURL != rootURL {
                lspSession?.stop()
                lspSession = try VikerExampleLspWorkspaceSession(rootURL: rootURL)
            }
            if let lspSession {
                try editor.attachLspSession(lspSession)
            }
        } catch {
            editor.presentLspUnavailable(error)
        }
    }

    private func ensureWindow() -> NSWindow {
        if let window {
            return window
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1040, height: 720),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.center()
        window.minSize = NSSize(width: 640, height: 420)
        window.delegate = self
        self.window = window
        return window
    }

    private func installMenu() {
        let mainMenu = NSMenu()

        let appMenuItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(
            NSMenuItem(title: "Quit VikerExample", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        )
        appMenuItem.submenu = appMenu
        mainMenu.addItem(appMenuItem)

        let fileMenuItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        let openItem = NSMenuItem(title: "Open...", action: #selector(openDocument(_:)), keyEquivalent: "o")
        openItem.target = self
        fileMenu.addItem(openItem)
        let saveItem = NSMenuItem(title: "Save", action: #selector(saveDocument(_:)), keyEquivalent: "s")
        saveItem.target = self
        fileMenu.addItem(saveItem)
        fileMenuItem.submenu = fileMenu
        mainMenu.addItem(fileMenuItem)

        let editMenuItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(NSMenuItem(title: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x"))
        editMenu.addItem(NSMenuItem(title: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c"))
        editMenu.addItem(NSMenuItem(title: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v"))
        editMenu.addItem(NSMenuItem(title: "Select All", action: #selector(NSResponder.selectAll(_:)), keyEquivalent: "a"))
        editMenuItem.submenu = editMenu
        mainMenu.addItem(editMenuItem)

        NSApp.mainMenu = mainMenu
    }

    private func initialURL() -> URL {
        let arguments = CommandLine.arguments.dropFirst()
        if let path = arguments.first, !path.isEmpty {
            return URL(fileURLWithPath: NSString(string: path).expandingTildeInPath)
        }
        return Self.sampleFileURL()
    }

    private static func sampleFileURL() -> URL {
        let folderURL = FileManager.default.temporaryDirectory.appendingPathComponent("VikerExample", isDirectory: true)
        let fileURL = folderURL.appendingPathComponent("Sample.swift")
        try? FileManager.default.createDirectory(at: folderURL, withIntermediateDirectories: true)

        if !FileManager.default.fileExists(atPath: fileURL.path) {
            let sample = """
            import Foundation

            struct Greeting {
                let subject: String

                func message() -> String {
                    "Hello, \\(subject)"
                }
            }

            let greeting = Greeting(subject: "VikerKit")
            print(greeting.message())
            """
            try? sample.write(to: fileURL, atomically: true, encoding: .utf8)
        }

        return fileURL
    }
}

@main
struct VikerExampleMain {
    @MainActor
    static func main() {
        let app = NSApplication.shared
        let delegate = VikerExampleAppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        app.run()
    }
}
