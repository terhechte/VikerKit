import AppKit
import VikerKit

@MainActor
final class VikerExampleAppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var window: NSWindow?
    private var editorContent: VikerEditorComponent?
    private var errorContent: ExampleErrorContent?

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
        editorContent = nil
        errorContent = nil
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
            let configuration = VikerEditorConfiguration(
                loadsLSPs: true,
                initialMode: .normal,
                showsLineNumbers: true,
                autosaves: false
            )
            let editor = try VikerEditorComponent(url: standardizedURL, configuration: configuration)
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
            let errorContent = ExampleErrorContent(url: standardizedURL, error: error)
            window.contentView = errorContent.view
            window.title = errorContent.title
            self.errorContent = errorContent
            window.makeKeyAndOrderFront(nil)
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
        editMenu.addItem(NSMenuItem(title: "Undo", action: NSSelectorFromString("undo:"), keyEquivalent: "z"))
        let redoItem = NSMenuItem(title: "Redo", action: NSSelectorFromString("redo:"), keyEquivalent: "z")
        redoItem.keyEquivalentModifierMask = [.command, .shift]
        editMenu.addItem(redoItem)
        editMenu.addItem(.separator())
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

@MainActor
private final class ExampleErrorContent {
    let view: NSView
    let title: String

    init(url: URL, error: Error) {
        title = "Unable to open \(url.lastPathComponent.isEmpty ? url.path : url.lastPathComponent)"

        let containerView = NSView()
        containerView.translatesAutoresizingMaskIntoConstraints = false
        containerView.wantsLayer = true
        containerView.layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor

        let titleLabel = NSTextField(labelWithString: title)
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.font = .preferredFont(forTextStyle: .title2)
        titleLabel.textColor = .labelColor

        let detailLabel = NSTextField(wrappingLabelWithString: String(describing: error))
        detailLabel.translatesAutoresizingMaskIntoConstraints = false
        detailLabel.font = .preferredFont(forTextStyle: .body)
        detailLabel.textColor = .secondaryLabelColor

        containerView.addSubview(titleLabel)
        containerView.addSubview(detailLabel)
        NSLayoutConstraint.activate([
            titleLabel.leadingAnchor.constraint(equalTo: containerView.leadingAnchor, constant: 24),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: containerView.trailingAnchor, constant: -24),
            titleLabel.topAnchor.constraint(equalTo: containerView.topAnchor, constant: 24),
            detailLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            detailLabel.trailingAnchor.constraint(equalTo: containerView.trailingAnchor, constant: -24),
            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 8)
        ])

        view = containerView
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
