#if os(macOS)
import AppKit

public struct VikerEditorToolbarItems: OptionSet {
    public let rawValue: Int

    public init(rawValue: Int) {
        self.rawValue = rawValue
    }

    public static let save = VikerEditorToolbarItems(rawValue: 1 << 0)
    public static let workspaceSymbols = VikerEditorToolbarItems(rawValue: 1 << 1)
    public static let lsp = VikerEditorToolbarItems(rawValue: 1 << 2)
    public static let mode = VikerEditorToolbarItems(rawValue: 1 << 3)
    public static let path = VikerEditorToolbarItems(rawValue: 1 << 4)
    public static let all: VikerEditorToolbarItems = [.save, .workspaceSymbols, .lsp, .mode, .path]
}

public enum VikerEditorInitialMode {
    case normal
    case insert
}

public struct VikerEditorConfiguration {
    public var colorScheme: VikerEditorColorScheme
    public var showsStatusBar: Bool
    public var toolbarItems: VikerEditorToolbarItems
    public var loadsLSPs: Bool
    public var initialMode: VikerEditorInitialMode
    public var disablesNormalMode: Bool
    public var showsLineNumbers: Bool
    public var autosaves: Bool
    public var autosaveDelay: TimeInterval
    public var forcedSyntaxLanguage: VikerSyntaxLanguage?
    public var workspaceRootURL: URL?

    public init(
        colorScheme: VikerEditorColorScheme = .dark,
        showsStatusBar: Bool = true,
        toolbarItems: VikerEditorToolbarItems = .all,
        loadsLSPs: Bool = true,
        initialMode: VikerEditorInitialMode = .normal,
        disablesNormalMode: Bool = false,
        showsLineNumbers: Bool = true,
        autosaves: Bool = false,
        autosaveDelay: TimeInterval = 0.7,
        forcedSyntaxLanguage: VikerSyntaxLanguage? = nil,
        workspaceRootURL: URL? = nil
    ) {
        self.colorScheme = colorScheme
        self.showsStatusBar = showsStatusBar
        self.toolbarItems = toolbarItems
        self.loadsLSPs = loadsLSPs
        self.initialMode = initialMode
        self.disablesNormalMode = disablesNormalMode
        self.showsLineNumbers = showsLineNumbers
        self.autosaves = autosaves
        self.autosaveDelay = autosaveDelay
        self.forcedSyntaxLanguage = forcedSyntaxLanguage
        self.workspaceRootURL = workspaceRootURL?.standardizedFileURL
    }
}

@MainActor
public final class VikerEditorComponent: NSObject {
    private static let maximumTextHistoryEntries = 200

    private let containerView = NSView()
    private let toolbar = VikerEditorToolbarView()
    private let saveButton = NSButton()
    private let symbolsButton = NSButton()
    private let lspButton = NSButton()
    private let modeLabel = NSTextField.vikerEditorLabel("", style: .monospaceCaptionSemibold)
    private let pathLabel = NSTextField.vikerEditorLabel("", style: .captionMedium, color: .secondaryLabelColor)
    private let statusLabel = NSTextField.vikerEditorLabel("", style: .caption, color: .secondaryLabelColor)
    private let scrollView = NSScrollView()
    private let editorView: VikerEditorCanvasView
    private let footerStack = NSStackView()
    private let commandLineView = NSView()
    private let commandLineLabel = NSTextField.vikerEditorLabel("", style: .monospaceCaption, color: .secondaryLabelColor)
    private let errorBar = NSView()
    private let errorLabel = NSTextField.vikerEditorLabel("", style: .caption, color: .systemRed)
    private let copyErrorButton = NSButton()

    private let editor: VikerEditor
    private var snapshot: VikerSnapshot
    private var lspSession: VikerEditorLspWorkspaceSession?
    private var lspDocument: VikerLspDocument?
    private var lspServerStatus: VikerLspServerStatus?
    private var lspDiagnostics: [VikerDiagnostic] = []
    private var lspMessage: String?
    private var lastLspSyncedText: String?
    private var pendingFormatSaveRequestID: UInt64?
    private var pendingWorkspaceSymbolsRequestID: UInt64?
    private var statusOverride: String?
    private var currentFileURL: URL?
    private var workspaceRootURL: URL?
    private var editorErrors: [EditorErrorSource: String] = [:]
    private var mouseSelectionAnchor: EditorMouseCell?
    private var mouseSelectionMode: VikerSelectionMode = .character
    private var isExtendingMouseSelection = false
    private var mouseSelectionPreservesEditorMode = false
    private var mouseTextSelectionAnchor: EditorTextSelectionAnchor?
    private var transientSelection: EditorTransientSelection?
    private var transientSelectionDrag: EditorTransientSelectionDrag?
    private var editorUndoStack: [EditorTextHistoryEntry] = []
    private var editorRedoStack: [EditorTextHistoryEntry] = []
    private let configuration: VikerEditorConfiguration
    private let showsToolbar: Bool
    private let autosaves: Bool
    private let autosaveDelay: TimeInterval
    private var autosaveRequestID: UInt64 = 0
    private var pendingAutosaveRequestID: UInt64?
    private var themeObserverToken: NSObjectProtocol?

    public private(set) var title: String
    public var onTitleChange: ((String) -> Void)?
    public var onBecomeActive: (() -> Void)?
    public var onOpenFile: ((URL) -> Void)?
    public var onOpenLocation: ((VikerEditorLocation) -> Void)?
    public var onFileURLChange: ((URL) -> Void)?
    public var currentDocumentURL: URL? { currentFileURL }
    public var vikerEditor: VikerEditor { editor }

    public convenience init(
        url: URL,
        configuration: VikerEditorConfiguration = VikerEditorConfiguration()
    ) throws {
        let standardizedURL = url.standardizedFileURL
        let openedEditor = try VikerEditor.open(path: standardizedURL.path)
        try self.init(editor: openedEditor, url: standardizedURL, configuration: configuration)
    }

    public init(
        editor openedEditor: VikerEditor,
        url: URL? = nil,
        configuration: VikerEditorConfiguration = VikerEditorConfiguration()
    ) throws {
        VikerEditorThemeManager.shared.colorScheme = configuration.colorScheme
        if let forcedSyntaxLanguage = configuration.forcedSyntaxLanguage {
            try openedEditor.setLanguage(language: forcedSyntaxLanguage)
        }
        if configuration.disablesNormalMode || configuration.initialMode == .insert {
            _ = try openedEditor.processKey(event: VikerKeyEvent(key: .character, text: "i", ctrl: false, alt: false))
        }
        let initialSnapshot = try openedEditor.snapshot()
        var initialRenderError: String?
        let initialRenderState = try Self.makeRenderState(
            editor: openedEditor,
            snapshot: initialSnapshot,
            diagnostics: [],
            renderError: &initialRenderError
        )

        self.editor = openedEditor
        self.snapshot = initialSnapshot
        self.editorView = VikerEditorCanvasView(
            renderState: initialRenderState,
            colorScheme: configuration.colorScheme,
            showsLineNumbers: configuration.showsLineNumbers
        )
        self.currentFileURL = initialSnapshot.filePath.map(Self.fileURL(fromPath:)) ?? url?.standardizedFileURL
        self.title = Self.title(from: initialSnapshot, fallbackURL: url?.standardizedFileURL)
        self.configuration = configuration
        self.showsToolbar = !configuration.toolbarItems.isEmpty
        self.autosaves = configuration.autosaves
        self.autosaveDelay = configuration.autosaveDelay

        super.init()

        setupViews()
        wireEditorView()
        applySnapshot(initialSnapshot, notifyTitleChange: false, notifyFileURLChange: false)
        setEditorError(initialRenderError, source: .render)
        configureLspIfNeeded()
    }

    public var view: NSView { containerView }

    public var isProcessAlive: Bool { false }

    public func makeFirstResponder() {
        editorView.window?.makeFirstResponder(editorView)
    }

    public func viewportDidChange() {
        do {
            try updateEditorViewportSize()
            applySnapshot(try editor.snapshot(), notifyTitleChange: false, notifyFileURLChange: false)
        } catch {
            present(error)
        }
    }

    public func willClose() {
        flushPendingAutosave()
        lspSession?.closeDocument(editor: editor, owner: self)
        lspSession = nil
        removeThemeObserver()
    }

    public func flushPendingAutosave() {
        pendingAutosaveRequestID = nil
        autosaveNowIfNeeded()
    }

    func attachLspSession(_ session: VikerEditorLspWorkspaceSession) throws {
        if lspSession !== session {
            lspSession?.closeDocument(editor: editor, owner: self)
            lspSession = session
        }
        workspaceRootURL = session.rootURL
        updatePathLabel()
        try session.openDocument(editor: editor, owner: self)
        lastLspSyncedText = snapshot.text
        clearEditorError(source: .lsp)
        refreshLspState(message: nil)
    }

    private func configureLspIfNeeded() {
        guard configuration.loadsLSPs else { return }
        let rootURL = configuration.workspaceRootURL
            ?? currentFileURL?.deletingLastPathComponent().standardizedFileURL
        guard let rootURL else { return }

        do {
            let session = try VikerEditorLspWorkspaceSession(rootURL: rootURL)
            try attachLspSession(session)
            if let language = try editor.syntaxLanguage() {
                startLsp(language: language)
            }
        } catch {
            presentLspUnavailable(error)
        }
    }

    public func setWorkspaceRoot(_ rootURL: URL?) {
        workspaceRootURL = rootURL?.standardizedFileURL
        updatePathLabel()
    }

    func presentLspUnavailable(_ error: Error) {
        lspDocument = nil
        lspServerStatus = nil
        lspDiagnostics = []
        lspMessage = "LSP unavailable"
        setEditorError("LSP unavailable: \(error.localizedDescription)", source: .lsp)
        updateStatusLabel()
    }

    public func jumpTo(row: UInt64, column: UInt64) {
        do {
            statusOverride = nil
            _ = try editor.setCursor(row: row, column: column)
            refreshSnapshot(syncLsp: false)
            makeFirstResponder()
        } catch {
            present(error)
        }
    }

    private func setupViews() {
        containerView.autoresizingMask = [.width, .height]
        containerView.wantsLayer = true

        saveButton.image = NSImage(systemSymbolName: "square.and.arrow.down", accessibilityDescription: "Save")
        saveButton.imagePosition = .imageOnly
        saveButton.applyVikerEditorButtonStyle(.toolbar)
        saveButton.target = self
        saveButton.action = #selector(saveDocument)
        saveButton.toolTip = "Save"
        saveButton.isHidden = !configuration.toolbarItems.contains(.save)

        symbolsButton.image = NSImage(systemSymbolName: "list.bullet.rectangle", accessibilityDescription: "Workspace Symbols")
        symbolsButton.imagePosition = .imageOnly
        symbolsButton.applyVikerEditorButtonStyle(.toolbar)
        symbolsButton.target = self
        symbolsButton.action = #selector(showWorkspaceSymbols)
        symbolsButton.toolTip = "Workspace Symbols"
        symbolsButton.isHidden = !configuration.toolbarItems.contains(.workspaceSymbols)

        lspButton.title = "LSP"
        lspButton.applyVikerEditorButtonStyle(.toolbarCompact)
        lspButton.target = self
        lspButton.action = #selector(showLspMenu)
        lspButton.toolTip = "LSP Off"
        lspButton.isHidden = !configuration.toolbarItems.contains(.lsp)

        modeLabel.alignment = .center
        modeLabel.applyVikerEditorLayer(
            backgroundColor: NSColor.controlAccentColor.withAlphaComponent(0.16),
            cornerRadius: VikerEditorDesign.Radius.control
        )
        modeLabel.translatesAutoresizingMaskIntoConstraints = false
        modeLabel.isHidden = !configuration.toolbarItems.contains(.mode)

        pathLabel.lineBreakMode = .byTruncatingMiddle
        pathLabel.translatesAutoresizingMaskIntoConstraints = false
        pathLabel.isHidden = !configuration.toolbarItems.contains(.path)

        statusLabel.lineBreakMode = .byTruncatingTail
        statusLabel.alignment = .right
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.isHidden = !configuration.showsStatusBar

        commandLineView.wantsLayer = true
        commandLineView.isHidden = true
        commandLineView.translatesAutoresizingMaskIntoConstraints = false

        commandLineLabel.lineBreakMode = .byTruncatingTail
        commandLineLabel.translatesAutoresizingMaskIntoConstraints = false
        commandLineView.addSubview(commandLineLabel)

        errorBar.wantsLayer = true
        errorBar.isHidden = true
        errorBar.translatesAutoresizingMaskIntoConstraints = false

        errorLabel.lineBreakMode = .byTruncatingTail
        errorLabel.translatesAutoresizingMaskIntoConstraints = false

        copyErrorButton.image = NSImage(systemSymbolName: "doc.on.doc", accessibilityDescription: "Copy Error")
        copyErrorButton.imagePosition = .imageOnly
        copyErrorButton.applyVikerEditorButtonStyle(.toolbar)
        copyErrorButton.target = self
        copyErrorButton.action = #selector(copyEditorError)
        copyErrorButton.toolTip = "Copy error"
        copyErrorButton.translatesAutoresizingMaskIntoConstraints = false

        errorBar.addSubview(errorLabel)
        errorBar.addSubview(copyErrorButton)

        footerStack.orientation = .vertical
        footerStack.spacing = 0
        footerStack.alignment = .width
        footerStack.distribution = .fill
        footerStack.isHidden = true
        footerStack.translatesAutoresizingMaskIntoConstraints = false
        footerStack.addArrangedSubview(commandLineView)
        footerStack.addArrangedSubview(errorBar)

        scrollView.documentView = editorView
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.translatesAutoresizingMaskIntoConstraints = false

        containerView.addSubview(scrollView)
        containerView.addSubview(footerStack)

        var constraints: [NSLayoutConstraint] = [
            scrollView.leadingAnchor.constraint(equalTo: containerView.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: containerView.trailingAnchor),
            scrollView.bottomAnchor.constraint(equalTo: footerStack.topAnchor),

            footerStack.leadingAnchor.constraint(equalTo: containerView.leadingAnchor),
            footerStack.trailingAnchor.constraint(equalTo: containerView.trailingAnchor),
            footerStack.bottomAnchor.constraint(equalTo: containerView.bottomAnchor),

            commandLineView.heightAnchor.constraint(equalToConstant: 26),
            commandLineLabel.leadingAnchor.constraint(equalTo: commandLineView.leadingAnchor, constant: 12),
            commandLineLabel.trailingAnchor.constraint(equalTo: commandLineView.trailingAnchor, constant: -12),
            commandLineLabel.centerYAnchor.constraint(equalTo: commandLineView.centerYAnchor),

            errorBar.heightAnchor.constraint(equalToConstant: 30),
            errorLabel.leadingAnchor.constraint(equalTo: errorBar.leadingAnchor, constant: 12),
            errorLabel.trailingAnchor.constraint(equalTo: copyErrorButton.leadingAnchor, constant: -8),
            errorLabel.centerYAnchor.constraint(equalTo: errorBar.centerYAnchor),
            copyErrorButton.trailingAnchor.constraint(equalTo: errorBar.trailingAnchor, constant: -8),
            copyErrorButton.centerYAnchor.constraint(equalTo: errorBar.centerYAnchor),
            copyErrorButton.widthAnchor.constraint(equalToConstant: VikerEditorDesign.Size.toolbarButtonWidth),
            copyErrorButton.heightAnchor.constraint(equalToConstant: 22),
        ]

        if showsToolbar {
            let spacer = NSView()
            spacer.translatesAutoresizingMaskIntoConstraints = false
            spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)

            let toolStack = NSStackView(views: [saveButton, symbolsButton, lspButton, modeLabel, pathLabel, spacer, statusLabel])
            toolStack.orientation = .horizontal
            toolStack.spacing = 8
            toolStack.alignment = .centerY
            toolStack.distribution = .fill
            toolStack.translatesAutoresizingMaskIntoConstraints = false
            toolbar.addSubview(toolStack)
            containerView.addSubview(toolbar)

            constraints.append(contentsOf: [
                toolbar.leadingAnchor.constraint(equalTo: containerView.leadingAnchor),
                toolbar.trailingAnchor.constraint(equalTo: containerView.trailingAnchor),
                toolbar.topAnchor.constraint(equalTo: containerView.topAnchor),
                toolbar.heightAnchor.constraint(equalToConstant: VikerEditorDesign.Size.toolbarHeight),

                toolStack.leadingAnchor.constraint(equalTo: toolbar.leadingAnchor, constant: 8),
                toolStack.trailingAnchor.constraint(equalTo: toolbar.trailingAnchor, constant: -8),
                toolStack.centerYAnchor.constraint(equalTo: toolbar.centerYAnchor),

                saveButton.widthAnchor.constraint(equalToConstant: VikerEditorDesign.Size.toolbarButtonWidth),
                saveButton.heightAnchor.constraint(equalToConstant: 22),
                symbolsButton.widthAnchor.constraint(equalToConstant: VikerEditorDesign.Size.toolbarButtonWidth),
                symbolsButton.heightAnchor.constraint(equalToConstant: 22),
                lspButton.widthAnchor.constraint(equalToConstant: 40),
                lspButton.heightAnchor.constraint(equalToConstant: 22),
                modeLabel.widthAnchor.constraint(greaterThanOrEqualToConstant: 48),
                modeLabel.heightAnchor.constraint(equalToConstant: 20),
                statusLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 190),
                scrollView.topAnchor.constraint(equalTo: toolbar.bottomAnchor),
            ])
        } else {
            constraints.append(scrollView.topAnchor.constraint(equalTo: containerView.topAnchor))
        }

        NSLayoutConstraint.activate(constraints)
        installThemeObserver()
        applyTheme(refreshSnapshot: false)
    }

    private func wireEditorView() {
        editorView.onFocus = { [weak self] in
            self?.onBecomeActive?()
        }
        editorView.onKeyDown = { [weak self] event in
            self?.handleKeyDown(event) ?? false
        }
        editorView.onPaste = { [weak self] text in
            self?.paste(text)
        }
        editorView.onCopy = { [weak self] in
            self?.copySelection() ?? false
        }
        editorView.onCut = { [weak self] in
            do {
                return try self?.cutSelection() ?? false
            } catch {
                self?.present(error)
                return true
            }
        }
        editorView.onSelectAll = { [weak self] in
            do {
                return try self?.selectAllInInsertMode() ?? false
            } catch {
                self?.present(error)
                return true
            }
        }
        editorView.onUndo = { [weak self] in
            do {
                return try self?.performUndoOrRedo(redo: false) ?? false
            } catch {
                self?.present(error)
                return true
            }
        }
        editorView.onRedo = { [weak self] in
            do {
                return try self?.performUndoOrRedo(redo: true) ?? false
            } catch {
                self?.present(error)
                return true
            }
        }
        editorView.onSave = { [weak self] in
            self?.saveDocument()
        }
        editorView.onMouseDown = { [weak self] cell in
            self?.handleMouseDown(cell)
        }
        editorView.onMouseDragged = { [weak self] cell in
            self?.handleMouseDragged(cell)
        }
        editorView.onMouseUp = { [weak self] in
            self?.finishMouseSelection()
        }
    }

    private func handleKeyDown(_ event: NSEvent) -> Bool {
        if configuration.disablesNormalMode, event.keyCode == 53 {
            transientSelection = nil
            clearEditorError(source: .operation)
            refreshSnapshot(syncLsp: false)
            return true
        }

        if configuration.disablesNormalMode, !Self.isTextSelectionMode(snapshot.mode) {
            do {
                try enterInsertModeIfNeeded()
                refreshSnapshot(syncLsp: false)
            } catch {
                present(error)
            }
        }

        if handleInsertEditingShortcut(event) {
            return true
        }

        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.command) {
            guard event.charactersIgnoringModifiers?.lowercased() == "s" else { return false }
            saveDocument()
            return true
        }

        guard let keyEvent = Self.vikerKeyEvent(from: event) else { return false }

        do {
            statusOverride = nil
            transientSelectionDrag = nil
            let historyBefore = try currentTextHistorySnapshot()
            let deletesOnly = try handleTransientSelectionReplacement(event, keyEvent: keyEvent)
            if deletesOnly {
                try recordTextHistory(before: historyBefore)
                clearEditorError(source: .operation)
                refreshSnapshot()
                scheduleAutosaveIfNeeded()
                return true
            }

            var effects = try editor.processKey(event: keyEvent)
            effects += try enterInsertModeIfNeeded()
            try recordTextHistory(before: historyBefore)
            applyEffects(effects)
            clearEditorError(source: .operation)
            refreshSnapshot()
            scheduleAutosaveIfNeeded()
        } catch {
            present(error)
        }
        return true
    }

    @discardableResult
    private func enterInsertModeIfNeeded() throws -> [VikerEffect] {
        guard configuration.disablesNormalMode,
              !Self.isTextSelectionMode(try editor.mode()) else {
            return []
        }

        return try editor.processKey(event: VikerKeyEvent(key: .character, text: "i", ctrl: false, alt: false))
    }

    private func paste(_ text: String) {
        guard !text.isEmpty else { return }
        do {
            statusOverride = nil
            transientSelectionDrag = nil
            let historyBefore = try currentTextHistorySnapshot()
            if Self.isTextSelectionMode(snapshot.mode), transientSelection != nil {
                _ = try deleteTransientSelectionContents()
            } else {
                transientSelection = nil
            }
            let effects = try editor.inputText(text: text)
            try recordTextHistory(before: historyBefore)
            applyEffects(effects)
            clearEditorError(source: .operation)
            refreshSnapshot()
            scheduleAutosaveIfNeeded()
        } catch {
            present(error)
        }
    }

    private func handleMouseDown(_ cell: EditorMouseCell) {
        do {
            statusOverride = nil
            mouseSelectionAnchor = nil
            isExtendingMouseSelection = false
            mouseSelectionPreservesEditorMode = false
            mouseTextSelectionAnchor = nil
            transientSelectionDrag = nil

            if Self.isTextSelectionMode(snapshot.mode) {
                try handleInsertMouseDown(cell)
            } else if cell.clickCount >= 3 {
                transientSelection = nil
                try selectLine(at: cell)
            } else if cell.clickCount == 2 {
                transientSelection = nil
                try selectWord(at: cell)
            } else if cell.modifierFlags.contains(.shift) {
                transientSelection = nil
                let position = try position(for: cell)
                _ = try editor.extendSelection(row: position.row, column: position.column)
            } else {
                transientSelection = nil
                try editor.clearSelection()
                let position = try position(for: cell)
                _ = try editor.setCursor(row: position.row, column: position.column)
                mouseSelectionAnchor = cell
                mouseSelectionMode = cell.modifierFlags.contains(.option) ? .block : .character
                mouseSelectionPreservesEditorMode = snapshot.mode == .insert || snapshot.mode == .replace
            }

            clearEditorError(source: .operation)
            refreshSnapshot()
        } catch {
            present(error)
        }
    }

    private func handleMouseDragged(_ cell: EditorMouseCell) {
        do {
            statusOverride = nil
            if var selectionDrag = transientSelectionDrag {
                let dropPosition = try position(for: cell)
                selectionDrag.dropPosition = dropPosition
                selectionDrag.hasMoved = true
                selectionDrag.copies = cell.modifierFlags.contains(.option)
                transientSelectionDrag = selectionDrag
                _ = try editor.setCursor(row: dropPosition.row, column: dropPosition.column)
                clearEditorError(source: .operation)
                refreshSnapshot()
                return
            }

            if mouseSelectionPreservesEditorMode {
                let cursorPosition = try position(for: cell)
                if let mouseTextSelectionAnchor {
                    try extendMouseTextSelection(from: mouseTextSelectionAnchor, to: cursorPosition)
                } else if let anchor = mouseSelectionAnchor {
                    let anchorPosition = try position(for: anchor)
                    try setTransientSelection(
                        anchor: anchorPosition,
                        cursor: cursorPosition,
                        mode: mouseSelectionMode,
                        usesInsertionEndpoints: true
                    )
                }
                clearEditorError(source: .operation)
                refreshSnapshot()
                return
            }

            if let anchor = mouseSelectionAnchor, !isExtendingMouseSelection {
                let position = try position(for: anchor)
                _ = try editor.beginSelection(row: position.row, column: position.column, mode: mouseSelectionMode)
                isExtendingMouseSelection = true
            }

            let position = try position(for: cell)
            _ = try editor.extendSelection(row: position.row, column: position.column)
            clearEditorError(source: .operation)
            refreshSnapshot()
        } catch {
            present(error)
        }
    }

    private func finishMouseSelection() {
        do {
            if let selectionDrag = transientSelectionDrag, selectionDrag.hasMoved {
                try finishTransientSelectionDrag(selectionDrag)
            }
            mouseSelectionAnchor = nil
            isExtendingMouseSelection = false
            mouseSelectionPreservesEditorMode = false
            mouseTextSelectionAnchor = nil
            transientSelectionDrag = nil
        } catch {
            transientSelectionDrag = nil
            present(error)
        }
    }

    private func handleInsertMouseDown(_ cell: EditorMouseCell) throws {
        let position = try position(for: cell)

        if cell.clickCount == 1,
           !cell.modifierFlags.contains(.shift),
           try beginTransientSelectionDragIfNeeded(at: position, copies: cell.modifierFlags.contains(.option)) {
            return
        }

        if cell.clickCount >= 3 {
            let paragraphRange = try paragraphSelectionRange(at: position)
            try setTransientSelection(
                anchor: paragraphRange.start,
                cursor: paragraphRange.end,
                mode: .line,
                usesInsertionEndpoints: false
            )
            mouseTextSelectionAnchor = EditorTextSelectionAnchor(
                lowerBound: paragraphRange.start,
                upperBound: paragraphRange.end,
                granularity: .paragraph
            )
            mouseSelectionPreservesEditorMode = true
        } else if cell.clickCount == 2 {
            if let wordRange = try wordSelectionRange(at: position) {
                try setTransientSelection(
                    anchor: wordRange.start,
                    cursor: wordRange.end,
                    mode: .character,
                    usesInsertionEndpoints: true
                )
                mouseTextSelectionAnchor = EditorTextSelectionAnchor(
                    lowerBound: wordRange.start,
                    upperBound: wordRange.end,
                    granularity: .word
                )
                mouseSelectionPreservesEditorMode = true
            } else {
                try editor.clearSelection()
                _ = try editor.setCursor(row: position.row, column: position.column)
                transientSelection = nil
            }
        } else if cell.modifierFlags.contains(.shift) {
            let anchor = transientSelection?.anchor ?? snapshot.cursor
            try setTransientSelection(
                anchor: anchor,
                cursor: position,
                mode: .character,
                usesInsertionEndpoints: true
            )
        } else {
            try editor.clearSelection()
            _ = try editor.setCursor(row: position.row, column: position.column)
            transientSelection = nil
            mouseSelectionAnchor = cell
            mouseSelectionMode = cell.modifierFlags.contains(.option) ? .block : .character
            mouseSelectionPreservesEditorMode = true
            mouseTextSelectionAnchor = EditorTextSelectionAnchor(
                lowerBound: position,
                upperBound: position,
                granularity: .character
            )
        }
    }

    private func handleInsertEditingShortcut(_ event: NSEvent) -> Bool {
        guard Self.isTextSelectionMode(snapshot.mode) else { return false }

        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        do {
            if flags.contains(.command), !flags.contains(.option), !flags.contains(.control) {
                switch event.charactersIgnoringModifiers?.lowercased() {
                case "a":
                    return try selectAllInInsertMode()
                case "c":
                    return copySelection()
                case "x":
                    return try cutSelection()
                case "z" where flags.contains(.shift):
                    return try performUndoOrRedo(redo: true)
                case "z":
                    return try performUndoOrRedo(redo: false)
                default:
                    break
                }
            }

            if try handleInsertDeleteShortcut(event, flags: flags) {
                return true
            }

            guard [123, 124, 125, 126].contains(Int(event.keyCode)) else { return false }

            let current = transientSelection?.cursor ?? snapshot.cursor
            let target: VikerPosition
            if flags.contains(.command) {
                switch event.keyCode {
                case 123:
                    target = VikerPosition(row: current.row, column: 0)
                case 124:
                    target = try lineEndPosition(row: current.row)
                case 125:
                    target = try documentEndPosition()
                case 126:
                    target = VikerPosition(row: 0, column: 0)
                default:
                    return false
                }
            } else if flags.contains(.option) {
                switch event.keyCode {
                case 123:
                    target = try previousWordBoundary(from: current)
                case 124:
                    target = try nextWordBoundary(from: current)
                case 125:
                    target = try paragraphBoundary(from: current, direction: .forward)
                case 126:
                    target = try paragraphBoundary(from: current, direction: .backward)
                default:
                    return false
                }
            } else if flags.contains(.shift) {
                switch event.keyCode {
                case 123:
                    target = try previousInsertionPosition(from: current)
                case 124:
                    target = try nextInsertionPosition(from: current)
                case 125:
                    target = try verticalInsertionPosition(from: current, rowOffset: 1)
                case 126:
                    target = try verticalInsertionPosition(from: current, rowOffset: -1)
                default:
                    return false
                }
            } else {
                return false
            }

            statusOverride = nil
            transientSelectionDrag = nil

            if flags.contains(.shift) {
                let anchor = transientSelection?.anchor ?? snapshot.cursor
                try setTransientSelection(
                    anchor: anchor,
                    cursor: target,
                    mode: .character,
                    usesInsertionEndpoints: true
                )
            } else {
                transientSelection = nil
                _ = try editor.setCursor(row: target.row, column: target.column)
            }
            clearEditorError(source: .operation)
            refreshSnapshot()
            return true
        } catch {
            present(error)
            return true
        }
    }

    private func handleInsertDeleteShortcut(_ event: NSEvent, flags: NSEvent.ModifierFlags) throws -> Bool {
        let keyCode = Int(event.keyCode)
        let controlKey = flags.contains(.control) && !flags.contains(.command) && !flags.contains(.option)
            ? event.charactersIgnoringModifiers?.lowercased()
            : nil
        let deletesBackwardLine = keyCode == 51 && flags.contains(.command)
        let deletesBackwardWord = keyCode == 51 && flags.contains(.option)
        let deletesBackwardCharacter = controlKey == "h"
        let deletesForwardLine = (keyCode == 117 && flags.contains(.command)) || controlKey == "k"
        let deletesForwardWord = keyCode == 117 && flags.contains(.option)
        let deletesForward = keyCode == 117
        let deletesForwardCharacter = deletesForward || controlKey == "d"
        guard deletesBackwardLine
            || deletesBackwardWord
            || deletesBackwardCharacter
            || deletesForwardLine
            || deletesForwardWord
            || deletesForwardCharacter else {
            return false
        }

        statusOverride = nil
        transientSelectionDrag = nil
        let historyBefore = try currentTextHistorySnapshot()

        if transientSelection != nil {
            _ = try deleteTransientSelectionContents()
            try recordTextHistory(before: historyBefore)
            clearEditorError(source: .operation)
            refreshSnapshot()
            scheduleAutosaveIfNeeded()
            return true
        }

        let text = snapshot.text
        let cursorOffset = Self.scalarOffset(for: snapshot.cursor, in: text)
        let scalarCount = text.unicodeScalars.count
        let range: Range<Int>

        if deletesBackwardLine {
            let start = Self.scalarOffset(for: VikerPosition(row: snapshot.cursor.row, column: 0), in: text)
            range = start..<cursorOffset
        } else if deletesBackwardWord {
            let start = Self.scalarOffset(for: try previousWordBoundary(from: snapshot.cursor), in: text)
            range = start..<cursorOffset
        } else if deletesBackwardCharacter {
            let start = Self.scalarOffset(for: try previousInsertionPosition(from: snapshot.cursor), in: text)
            range = start..<cursorOffset
        } else if deletesForwardLine {
            let lineEndOffset = Self.scalarOffset(for: try lineEndPosition(row: snapshot.cursor.row), in: text)
            if cursorOffset < lineEndOffset {
                range = cursorOffset..<lineEndOffset
            } else {
                range = cursorOffset..<min(cursorOffset + 1, scalarCount)
            }
        } else if flags.contains(.option) {
            let end = Self.scalarOffset(for: try nextWordBoundary(from: snapshot.cursor), in: text)
            range = cursorOffset..<end
        } else {
            range = cursorOffset..<min(cursorOffset + 1, scalarCount)
        }

        guard try replaceText(range: range, with: "", cursorOffset: range.lowerBound) else {
            return true
        }
        try recordTextHistory(before: historyBefore)
        clearEditorError(source: .operation)
        refreshSnapshot()
        scheduleAutosaveIfNeeded()
        return true
    }

    private func handleTransientSelectionReplacement(_ event: NSEvent, keyEvent: VikerKeyEvent) throws -> Bool {
        guard Self.isTextSelectionMode(snapshot.mode), transientSelection != nil else {
            transientSelection = nil
            return false
        }

        if keyEvent.key == .escape {
            transientSelection = nil
            return false
        }

        if keyEvent.key == .backspace || event.keyCode == 117 {
            _ = try deleteTransientSelectionContents()
            return true
        }

        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let isPlainTextInput = keyEvent.key == .character
            && !flags.contains(.command)
            && !flags.contains(.control)
            && !flags.contains(.option)
        let replacesSelection = isPlainTextInput || keyEvent.key == .enter || keyEvent.key == .tab

        if replacesSelection {
            _ = try deleteTransientSelectionContents()
        } else {
            transientSelection = nil
        }

        return false
    }

    private func setTransientSelection(
        anchor: VikerPosition,
        cursor: VikerPosition,
        mode: VikerSelectionMode,
        usesInsertionEndpoints: Bool
    ) throws {
        _ = try editor.setCursor(row: cursor.row, column: cursor.column)

        if mode == .character, usesInsertionEndpoints, anchor == cursor {
            transientSelection = nil
            return
        }

        transientSelection = EditorTransientSelection(
            anchor: anchor,
            cursor: cursor,
            mode: mode,
            usesInsertionEndpoints: usesInsertionEndpoints
        )
    }

    private func extendMouseTextSelection(from anchor: EditorTextSelectionAnchor, to position: VikerPosition) throws {
        switch anchor.granularity {
        case .character:
            try setTransientSelection(
                anchor: anchor.lowerBound,
                cursor: position,
                mode: .character,
                usesInsertionEndpoints: true
            )
        case .word:
            let targetRange = try wordSelectionRange(at: position) ?? (start: position, end: position)
            let extendsBackward = Self.precedesPosition(position, anchor.lowerBound)
            try setTransientSelection(
                anchor: extendsBackward ? anchor.upperBound : anchor.lowerBound,
                cursor: extendsBackward ? targetRange.start : targetRange.end,
                mode: .character,
                usesInsertionEndpoints: true
            )
        case .paragraph:
            let targetRange = try paragraphSelectionRange(at: position)
            let extendsBackward = Self.precedesPosition(position, anchor.lowerBound)
            try setTransientSelection(
                anchor: extendsBackward ? anchor.upperBound : anchor.lowerBound,
                cursor: extendsBackward ? targetRange.start : targetRange.end,
                mode: .line,
                usesInsertionEndpoints: false
            )
        }
    }

    private func beginTransientSelectionDragIfNeeded(at position: VikerPosition, copies: Bool) throws -> Bool {
        guard let selectedText = selectedTransientText(),
              !selectedText.isEmpty,
              let selectedRange = transientTextRange(in: snapshot.text),
              !selectedRange.isEmpty else {
            return false
        }

        let clickedOffset = Self.scalarOffset(for: position, in: snapshot.text)
        guard selectedRange.contains(clickedOffset) else {
            return false
        }

        transientSelectionDrag = EditorTransientSelectionDrag(
            sourceRange: selectedRange,
            selectedText: selectedText,
            dropPosition: position,
            hasMoved: false,
            copies: copies
        )
        return true
    }

    private func finishTransientSelectionDrag(_ selectionDrag: EditorTransientSelectionDrag) throws {
        let sourceRange = selectionDrag.sourceRange
        let selectedScalarCount = selectionDrag.selectedText.unicodeScalars.count
        guard selectedScalarCount > 0 else { return }

        let originalText = snapshot.text
        let originalScalarCount = originalText.unicodeScalars.count
        let dropOffset = Self.scalarOffset(for: selectionDrag.dropPosition, in: originalText)
        let historyBefore = try currentTextHistorySnapshot()

        let insertionOffset: Int
        let newText: String
        if selectionDrag.copies {
            insertionOffset = Self.clamped(dropOffset, lowerBound: 0, upperBound: originalScalarCount)
            newText = Self.replacingUnicodeScalars(
                in: originalText,
                range: insertionOffset..<insertionOffset,
                with: selectionDrag.selectedText
            )
        } else {
            guard dropOffset < sourceRange.lowerBound || dropOffset > sourceRange.upperBound else {
                transientSelectionDrag = nil
                return
            }

            let withoutSelection = Self.replacingUnicodeScalars(
                in: originalText,
                range: sourceRange,
                with: ""
            )
            insertionOffset = dropOffset > sourceRange.upperBound
                ? dropOffset - (sourceRange.upperBound - sourceRange.lowerBound)
                : dropOffset
            newText = Self.replacingUnicodeScalars(
                in: withoutSelection,
                range: insertionOffset..<insertionOffset,
                with: selectionDrag.selectedText
            )
        }

        try editor.setText(text: newText)
        let anchor = Self.position(forScalarOffset: insertionOffset, in: newText)
        let cursor = Self.position(forScalarOffset: insertionOffset + selectedScalarCount, in: newText)
        transientSelection = EditorTransientSelection(
            anchor: anchor,
            cursor: cursor,
            mode: .character,
            usesInsertionEndpoints: true
        )
        _ = try editor.setCursor(row: cursor.row, column: cursor.column)
        try recordTextHistory(before: historyBefore)
        clearEditorError(source: .operation)
        refreshSnapshot()
        scheduleAutosaveIfNeeded()
    }

    private func copySelection() -> Bool {
        guard Self.isTextSelectionMode(snapshot.mode),
              let text = selectedTransientText(),
              !text.isEmpty else {
            return false
        }

        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        return true
    }

    private func cutSelection() throws -> Bool {
        guard copySelection() else { return false }
        let historyBefore = try currentTextHistorySnapshot()
        _ = try deleteTransientSelectionContents()
        try recordTextHistory(before: historyBefore)
        clearEditorError(source: .operation)
        refreshSnapshot()
        scheduleAutosaveIfNeeded()
        return true
    }

    private func selectAllInInsertMode() throws -> Bool {
        guard Self.isTextSelectionMode(snapshot.mode) else { return false }
        try setTransientSelection(
            anchor: VikerPosition(row: 0, column: 0),
            cursor: documentEndPosition(),
            mode: .character,
            usesInsertionEndpoints: true
        )
        clearEditorError(source: .operation)
        refreshSnapshot()
        return true
    }

    private func performUndoOrRedo(redo: Bool) throws -> Bool {
        let wasTextSelectionMode = Self.isTextSelectionMode(snapshot.mode)
        guard wasTextSelectionMode || snapshot.mode == .normal else { return false }

        statusOverride = nil
        transientSelection = nil
        transientSelectionDrag = nil

        if redo {
            guard let entry = editorRedoStack.popLast() else { return true }
            try restoreTextHistorySnapshot(entry.after)
            editorUndoStack.append(entry)
        } else {
            guard let entry = editorUndoStack.popLast() else { return true }
            try restoreTextHistorySnapshot(entry.before)
            editorRedoStack.append(entry)
        }

        clearEditorError(source: .operation)
        refreshSnapshot()
        scheduleAutosaveIfNeeded()
        return true
    }

    private func currentTextHistorySnapshot() throws -> EditorTextHistorySnapshot {
        EditorTextHistorySnapshot(
            text: try editor.text(),
            cursor: try editor.cursor()
        )
    }

    private func recordTextHistory(before: EditorTextHistorySnapshot) throws {
        let after = try currentTextHistorySnapshot()
        guard before.text != after.text else { return }

        editorUndoStack.append(EditorTextHistoryEntry(before: before, after: after))
        if editorUndoStack.count > Self.maximumTextHistoryEntries {
            editorUndoStack.removeFirst(editorUndoStack.count - Self.maximumTextHistoryEntries)
        }
        editorRedoStack.removeAll()
    }

    private func restoreTextHistorySnapshot(_ historySnapshot: EditorTextHistorySnapshot) throws {
        try editor.setText(text: historySnapshot.text)
        _ = try editor.setCursor(row: historySnapshot.cursor.row, column: historySnapshot.cursor.column)
    }

    @discardableResult
    private func deleteTransientSelectionContents() throws -> Bool {
        guard let selectionRange = transientTextRange(in: snapshot.text), !selectionRange.isEmpty else {
            transientSelection = nil
            return false
        }

        _ = try replaceText(range: selectionRange, with: "", cursorOffset: selectionRange.lowerBound)
        transientSelection = nil
        return true
    }

    @discardableResult
    private func replaceText(range: Range<Int>, with replacement: String, cursorOffset: Int) throws -> Bool {
        guard !range.isEmpty || !replacement.isEmpty else { return false }
        let newText = Self.replacingUnicodeScalars(
            in: snapshot.text,
            range: range,
            with: replacement
        )
        try editor.setText(text: newText)
        let cursor = Self.position(forScalarOffset: cursorOffset, in: newText)
        _ = try editor.setCursor(row: cursor.row, column: cursor.column)
        return true
    }

    private func selectedTransientText() -> String? {
        guard let selectionRange = transientTextRange(in: snapshot.text), !selectionRange.isEmpty else {
            return nil
        }

        return Self.stringBySelectingUnicodeScalars(in: snapshot.text, range: selectionRange)
    }

    private func transientTextRange(in text: String) -> Range<Int>? {
        guard let transientSelection else { return nil }
        let scalarCount = text.unicodeScalars.count

        if transientSelection.mode == .line {
            let starts = Self.lineStartOffsets(in: text)
            guard !starts.isEmpty else { return nil }
            let startRow = min(Self.intClamped(min(transientSelection.anchor.row, transientSelection.cursor.row)), starts.count - 1)
            let endRow = min(Self.intClamped(max(transientSelection.anchor.row, transientSelection.cursor.row)), starts.count - 1)
            let startOffset = starts[startRow]
            let endOffset = endRow + 1 < starts.count ? starts[endRow + 1] : scalarCount
            return startOffset..<endOffset
        }

        var anchorOffset = Self.scalarOffset(for: transientSelection.anchor, in: text)
        var cursorOffset = Self.scalarOffset(for: transientSelection.cursor, in: text)
        if !transientSelection.usesInsertionEndpoints {
            if anchorOffset <= cursorOffset {
                cursorOffset = min(cursorOffset + 1, scalarCount)
            } else {
                anchorOffset = min(anchorOffset + 1, scalarCount)
            }
        }

        return min(anchorOffset, cursorOffset)..<max(anchorOffset, cursorOffset)
    }

    private func wordSelectionRange(at position: VikerPosition) throws -> (start: VikerPosition, end: VikerPosition)? {
        let line = try editor.line(row: position.row)
        let scalars = Array(line.unicodeScalars)
        guard !scalars.isEmpty else { return nil }

        var index = min(Self.intClamped(position.column), scalars.count - 1)
        if !Self.isWordScalar(scalars[index]), index > 0, Self.isWordScalar(scalars[index - 1]) {
            index -= 1
        }
        guard Self.isWordScalar(scalars[index]) else { return nil }

        var start = index
        while start > 0, Self.isWordScalar(scalars[start - 1]) {
            start -= 1
        }

        var end = index + 1
        while end < scalars.count, Self.isWordScalar(scalars[end]) {
            end += 1
        }

        return (
            VikerPosition(row: position.row, column: UInt64(start)),
            VikerPosition(row: position.row, column: UInt64(end))
        )
    }

    private func paragraphSelectionRange(at position: VikerPosition) throws -> (start: VikerPosition, end: VikerPosition) {
        let lineCount = max(Self.intClamped(try editor.lineCount()), 1)
        let row = Self.clamped(Self.intClamped(position.row), lowerBound: 0, upperBound: lineCount - 1)

        var startRow = row
        var endRow = row
        if try !editor.line(row: UInt64(row)).isEmpty {
            while startRow > 0, try !editor.line(row: UInt64(startRow - 1)).isEmpty {
                startRow -= 1
            }
            while endRow + 1 < lineCount, try !editor.line(row: UInt64(endRow + 1)).isEmpty {
                endRow += 1
            }
        }

        return (
            VikerPosition(row: UInt64(startRow), column: 0),
            VikerPosition(row: UInt64(endRow), column: 0)
        )
    }

    private func paragraphBoundary(from position: VikerPosition, direction: EditorTextDirection) throws -> VikerPosition {
        let lineCount = max(Self.intClamped(try editor.lineCount()), 1)
        let paragraphRange = try paragraphSelectionRange(at: position)

        switch direction {
        case .backward:
            if position != paragraphRange.start {
                return paragraphRange.start
            }
            var row = Self.intClamped(paragraphRange.start.row) - 1
            while row > 0, try editor.line(row: UInt64(row)).isEmpty {
                row -= 1
            }
            guard row >= 0 else { return VikerPosition(row: 0, column: 0) }
            return try paragraphSelectionRange(at: VikerPosition(row: UInt64(row), column: 0)).start
        case .forward:
            let paragraphEnd = try lineEndPosition(row: paragraphRange.end.row)
            if position != paragraphEnd {
                return paragraphEnd
            }
            var row = Self.intClamped(paragraphRange.end.row) + 1
            while row + 1 < lineCount, try editor.line(row: UInt64(row)).isEmpty {
                row += 1
            }
            guard row < lineCount else { return paragraphEnd }
            let nextRange = try paragraphSelectionRange(at: VikerPosition(row: UInt64(row), column: 0))
            return try lineEndPosition(row: nextRange.end.row)
        }
    }

    private func previousInsertionPosition(from position: VikerPosition) throws -> VikerPosition {
        if position.column > 0 {
            return VikerPosition(row: position.row, column: position.column - 1)
        }

        guard position.row > 0 else { return position }
        let row = position.row - 1
        return try lineEndPosition(row: row)
    }

    private func nextInsertionPosition(from position: VikerPosition) throws -> VikerPosition {
        let lineEnd = try lineEndPosition(row: position.row)
        if position.column < lineEnd.column {
            return VikerPosition(row: position.row, column: position.column + 1)
        }

        let lineCount = try editor.lineCount()
        guard position.row + 1 < lineCount else { return lineEnd }
        return VikerPosition(row: position.row + 1, column: 0)
    }

    private func verticalInsertionPosition(from position: VikerPosition, rowOffset: Int) throws -> VikerPosition {
        let lineCount = max(Self.intClamped(try editor.lineCount()), 1)
        let row = Self.clamped(Self.intClamped(position.row) + rowOffset, lowerBound: 0, upperBound: lineCount - 1)
        let lineEnd = try lineEndPosition(row: UInt64(row))
        return VikerPosition(row: UInt64(row), column: min(position.column, lineEnd.column))
    }

    private func previousWordBoundary(from position: VikerPosition) throws -> VikerPosition {
        var row = position.row
        var scalars = Array(try editor.line(row: row).unicodeScalars)
        var index = min(Self.intClamped(position.column), scalars.count)

        while true {
            if index > 0 {
                index -= 1
                while index > 0, !Self.isWordScalar(scalars[index]) {
                    index -= 1
                }
                guard Self.isWordScalar(scalars[index]) else {
                    return VikerPosition(row: row, column: 0)
                }
                while index > 0, Self.isWordScalar(scalars[index - 1]) {
                    index -= 1
                }
                return VikerPosition(row: row, column: UInt64(index))
            }

            guard row > 0 else {
                return VikerPosition(row: 0, column: 0)
            }
            row -= 1
            scalars = Array(try editor.line(row: row).unicodeScalars)
            index = scalars.count
        }
    }

    private func nextWordBoundary(from position: VikerPosition) throws -> VikerPosition {
        let lineCount = try editor.lineCount()
        var row = position.row
        var scalars = Array(try editor.line(row: row).unicodeScalars)
        var index = min(Self.intClamped(position.column), scalars.count)

        while true {
            if index < scalars.count {
                while index < scalars.count, !Self.isWordScalar(scalars[index]) {
                    index += 1
                }
                while index < scalars.count, Self.isWordScalar(scalars[index]) {
                    index += 1
                }
                return VikerPosition(row: row, column: UInt64(index))
            }

            guard row + 1 < lineCount else {
                return VikerPosition(row: row, column: UInt64(index))
            }
            row += 1
            scalars = Array(try editor.line(row: row).unicodeScalars)
            index = 0
        }
    }

    private func lineEndPosition(row: UInt64) throws -> VikerPosition {
        let line = try editor.line(row: row)
        return VikerPosition(row: row, column: UInt64(line.unicodeScalars.count))
    }

    private func documentEndPosition() throws -> VikerPosition {
        let lineCount = max(try editor.lineCount(), 1)
        return try lineEndPosition(row: lineCount - 1)
    }

    private func selectWord(at cell: EditorMouseCell) throws {
        let position = try position(for: cell)
        if try !editor.selectWordAt(row: position.row, column: position.column) {
            try editor.clearSelection()
            _ = try editor.setCursor(row: position.row, column: position.column)
        }
    }

    private func selectLine(at cell: EditorMouseCell) throws {
        let position = try position(for: cell)
        if try !editor.selectLineAt(row: position.row) {
            try editor.clearSelection()
            _ = try editor.setCursor(row: position.row, column: position.column)
        }
    }

    private func position(for cell: EditorMouseCell) throws -> VikerPosition {
        VikerPosition(row: cell.row, column: try bufferColumn(row: cell.row, displayColumn: cell.column))
    }

    // Viker's ViewCell APIs are viewport-relative. This AppKit canvas renders a
    // full scroll view document, so mouse cells are absolute document rows.
    private func bufferColumn(row: UInt64, displayColumn: UInt64) throws -> UInt64 {
        var lineEnd: UInt64 = 0

        for cell in try editor.displayCells(row: row) {
            let cellStart = cell.cellStart
            let cellWidth = max(cell.cellWidth, 1)
            let cellEnd = cellStart + cellWidth
            lineEnd = max(lineEnd, cell.charEnd)

            if displayColumn <= cellStart {
                return cell.charStart
            }

            if displayColumn < cellEnd {
                return cellWidth > 1 && displayColumn > cellStart ? cell.charEnd : cell.charStart
            }

            if displayColumn == cellEnd {
                return cell.charEnd
            }
        }

        return lineEnd
    }

    @objc private func saveDocument() {
        pendingAutosaveRequestID = nil
        guard pendingFormatSaveRequestID == nil else { return }

        guard let lspSession, isLspRunning else {
            finishSavingDocument()
            return
        }

        do {
            statusOverride = "Formatting"
            updateStatusLabel()
            let request = try lspSession.formatDocumentBeforeSave(editor: editor, owner: self)
            guard let requestID = request.id, !request.completed else {
                refreshSnapshot(syncLsp: false)
                finishSavingDocument()
                return
            }
            pendingFormatSaveRequestID = requestID
            scheduleFormatSaveTimeout(requestID)
        } catch {
            statusOverride = "Format unavailable; saving"
            updateStatusLabel()
            finishSavingDocument()
        }
    }

    public func save() {
        saveDocument()
    }

    private func finishSavingDocument() {
        do {
            let effects = try editor.save()
            statusOverride = "Saved"
            applyEffects(effects)
            var didSyncLspOnSave = false
            if let lspSession, isLspRunning {
                do {
                    try lspSession.saveDocument(editor: editor, owner: self)
                    didSyncLspOnSave = true
                    clearEditorError(source: .lsp)
                } catch {
                    statusOverride = "Saved; LSP save failed"
                    lspMessage = "LSP save failed"
                    setEditorError("LSP save failed: \(error.localizedDescription)", source: .lsp)
                }
            }
            clearEditorError(source: .operation)
            refreshSnapshot(syncLsp: false)
            if didSyncLspOnSave {
                lastLspSyncedText = snapshot.text
            }
        } catch {
            present(error)
        }
    }

    private func scheduleAutosaveIfNeeded() {
        guard autosaves, snapshot.modified else { return }
        autosaveRequestID &+= 1
        let requestID = autosaveRequestID
        pendingAutosaveRequestID = requestID

        DispatchQueue.main.asyncAfter(deadline: .now() + autosaveDelay) { [weak self] in
            guard let self, self.pendingAutosaveRequestID == requestID else { return }
            self.pendingAutosaveRequestID = nil
            self.autosaveNowIfNeeded()
        }
    }

    private func autosaveNowIfNeeded() {
        guard autosaves, snapshot.modified else { return }
        saveDocument()
    }

    private func refreshSnapshot(syncLsp: Bool = true) {
        do {
            applySnapshot(try editor.snapshot(), notifyTitleChange: true, notifyFileURLChange: true)
            if syncLsp {
                syncLspDocumentIfNeeded()
            }
        } catch {
            present(error)
        }
    }

    private func applySnapshot(
        _ snapshot: VikerSnapshot,
        notifyTitleChange: Bool,
        notifyFileURLChange: Bool
    ) {
        self.snapshot = snapshot

        do {
            try updateEditorViewportSize()
            var renderError: String?
            editorView.setRenderState(
                try Self.makeRenderState(
                    editor: editor,
                    snapshot: snapshot,
                    diagnostics: lspDiagnostics,
                    transientSelection: transientSelection,
                    renderError: &renderError
                ),
                viewportWidth: scrollView.contentView.bounds.width
            )
            setEditorError(renderError, source: .render)
        } catch {
            present(error)
        }

        let nextTitle = Self.title(from: snapshot, fallbackURL: currentFileURL)
        if nextTitle != title {
            title = nextTitle
            if notifyTitleChange {
                onTitleChange?(nextTitle)
            }
        }

        if let filePath = snapshot.filePath {
            let nextURL = Self.fileURL(fromPath: filePath)
            if nextURL != currentFileURL {
                currentFileURL = nextURL
                if notifyFileURLChange {
                    onFileURLChange?(nextURL)
                }
            }
        }

        updatePathLabel()
        modeLabel.stringValue = Self.label(for: snapshot.mode)
        updateSaveButtonTint()

        updateCommandLine()
        updateLspButtons()
        updateStatusLabel()
    }

    private func applyEffects(_ effects: [VikerEffect]) {
        for effect in effects {
            switch effect.kind {
            case .didSave:
                statusOverride = "Saved"
            case .openFile:
                guard let url = fileURL(from: effect.payload) else { break }
                onOpenFile?(url)
            case .rename, .syncFileUri:
                guard let url = fileURL(from: effect.payload) else { break }
                if url != currentFileURL {
                    currentFileURL = url
                    onFileURLChange?(url)
                    reopenLspDocument()
                }
            case .shellCommand:
                statusOverride = effect.payload ?? "Shell command requested"
            case .formatDocument:
                statusOverride = effect.payload ?? "Format requested"
            case .playMacro:
                statusOverride = effect.payload ?? "Macro requested"
            case .git:
                statusOverride = effect.payload ?? "Git operation requested"
            }
        }
    }

    private func syncLspDocumentIfNeeded() {
        guard let lspSession, isLspRunning, snapshot.text != lastLspSyncedText else { return }
        do {
            try lspSession.syncDocument(editor: editor, owner: self)
            lastLspSyncedText = snapshot.text
            clearEditorError(source: .lsp)
            refreshLspState(message: nil)
        } catch {
            lspMessage = "LSP sync failed"
            setEditorError("LSP sync failed: \(error.localizedDescription)", source: .lsp)
            updateStatusLabel()
        }
    }

    private func reopenLspDocument() {
        guard let lspSession else { return }
        do {
            lspSession.closeDocument(editor: editor, owner: self)
            try lspSession.openDocument(editor: editor, owner: self)
            lastLspSyncedText = snapshot.text
            refreshLspState(message: nil)
        } catch {
            presentLspUnavailable(error)
        }
    }

    private func refreshLspState(message: String?) {
        guard let lspSession else { return }
        do {
            let state = try lspSession.state(for: editor)
            lspDocument = state.document
            lspServerStatus = state.serverStatus
            lspDiagnostics = state.diagnostics
            lspMessage = message ?? state.message
            clearEditorError(source: .lsp)
            applySnapshot(snapshot, notifyTitleChange: false, notifyFileURLChange: false)
        } catch {
            lspMessage = "LSP unavailable"
            setEditorError("LSP unavailable: \(error.localizedDescription)", source: .lsp)
            updateStatusLabel()
        }
    }

    func handleLspWorkspaceEvent(_ event: VikerLspWorkspaceEvent, session: VikerEditorLspWorkspaceSession) {
        guard lspSession === session else { return }

        switch event.kind {
        case .formattingApplied:
            refreshSnapshot(syncLsp: false)
            if event.requestId == pendingFormatSaveRequestID {
                pendingFormatSaveRequestID = nil
                finishSavingDocument()
            } else {
                refreshLspState(message: event.message)
            }
        case .workspaceSymbolsUpdated:
            guard event.requestId == pendingWorkspaceSymbolsRequestID,
                  let requestID = event.requestId else {
                refreshLspState(message: event.message)
                return
            }
            pendingWorkspaceSymbolsRequestID = nil
            do {
                presentWorkspaceSymbols(try session.workspaceSymbols(requestID: requestID))
                statusOverride = nil
                refreshLspState(message: event.message)
            } catch {
                present(error)
            }
        case .diagnosticsUpdated, .ready:
            if statusOverride == "Starting LSP" {
                statusOverride = nil
            }
            refreshLspState(message: event.message)
        case .error:
            if event.requestId == pendingFormatSaveRequestID {
                pendingFormatSaveRequestID = nil
                finishSavingDocument()
            }
            if event.requestId == pendingWorkspaceSymbolsRequestID {
                pendingWorkspaceSymbolsRequestID = nil
            }
            lspMessage = "LSP error"
            setEditorError(event.message ?? "LSP error", source: .lsp)
            updateStatusLabel()
        case .completionUpdated, .hoverUpdated, .referencesUpdated, .renameApplied:
            refreshLspState(message: event.message)
        }
    }

    private func updateStatusLabel() {
        let baseStatus: String
        if let statusOverride {
            baseStatus = statusOverride
            statusLabel.textColor = .secondaryLabelColor
        } else if let message = snapshot.statusMessage, !message.isEmpty {
            baseStatus = message
            statusLabel.textColor = .secondaryLabelColor
        } else {
            baseStatus = snapshot.modified ? "Modified" : "Saved"
            statusLabel.textColor = snapshot.modified ? .controlAccentColor : .tertiaryLabelColor
        }

        if let lspStatus = lspStatusSummary, !lspStatus.isEmpty {
            statusLabel.stringValue = "\(baseStatus) | \(lspStatus)"
        } else {
            statusLabel.stringValue = baseStatus
        }
    }

    private var isLspRunning: Bool {
        lspServerStatus?.running == true
    }

    private var isLspActive: Bool {
        lspServerStatus?.running == true || lspServerStatus?.initialized == true
    }

    private func updateLspButtons() {
        let hasLanguage = (try? editor.syntaxLanguage()) != nil
        lspButton.isEnabled = lspSession != nil && hasLanguage

        if isLspActive {
            lspButton.contentTintColor = .controlAccentColor
            lspButton.toolTip = lspServerStatus?.initialized == true ? "LSP Active" : "LSP Starting"
            symbolsButton.isEnabled = true
            symbolsButton.contentTintColor = .secondaryLabelColor
        } else {
            lspButton.contentTintColor = lspButton.isEnabled ? .secondaryLabelColor : .tertiaryLabelColor
            lspButton.toolTip = lspButton.isEnabled ? "LSP Off" : "No LSP for this file"
            symbolsButton.isEnabled = false
            symbolsButton.contentTintColor = .tertiaryLabelColor
        }
    }

    private func updatePathLabel() {
        let url = snapshot.filePath.map(Self.fileURL(fromPath:)) ?? currentFileURL
        pathLabel.stringValue = Self.displayPath(for: url, relativeTo: workspaceRootURL)
        pathLabel.toolTip = url?.path
    }

    private func updateCommandLine() {
        let text: String?
        switch snapshot.mode {
        case .command:
            text = ":\(snapshot.commandBuffer)"
        case .search:
            text = "/\(snapshot.searchQuery)"
        default:
            text = nil
        }

        commandLineLabel.stringValue = text ?? ""
        commandLineLabel.toolTip = text
        commandLineView.isHidden = text == nil
        updateFooterVisibility()
    }

    private func setEditorError(_ message: String?, source: EditorErrorSource) {
        if let message, !message.isEmpty {
            editorErrors[source] = message
        } else {
            editorErrors.removeValue(forKey: source)
        }
        updateErrorBar()
    }

    private func clearEditorError(source: EditorErrorSource) {
        guard editorErrors[source] != nil else { return }
        editorErrors.removeValue(forKey: source)
        updateErrorBar()
    }

    private func updateErrorBar() {
        let message = activeEditorError
        errorLabel.stringValue = message ?? ""
        errorLabel.toolTip = message
        errorBar.toolTip = message
        copyErrorButton.toolTip = message.map { "Copy error: \($0)" } ?? "Copy error"
        errorBar.isHidden = message == nil
        updateFooterVisibility()
    }

    private var activeEditorError: String? {
        editorErrors[.operation] ?? editorErrors[.render] ?? editorErrors[.lsp]
    }

    private func updateFooterVisibility() {
        footerStack.isHidden = commandLineView.isHidden && errorBar.isHidden
    }

    @objc private func copyEditorError() {
        guard let message = activeEditorError else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(message, forType: .string)
    }

    private var lspStatusSummary: String? {
        let errors = lspDiagnostics.filter { $0.severity == 1 }.count
        let warnings = lspDiagnostics.filter { $0.severity == 2 }.count
        if errors > 0 || warnings > 0 {
            if errors > 0 && warnings > 0 {
                return "LSP \(errors)E \(warnings)W"
            }
            if errors > 0 {
                return "LSP \(errors)E"
            }
            return "LSP \(warnings)W"
        }

        if let lspMessage, !lspMessage.isEmpty {
            return "LSP \(lspMessage)"
        }

        if let lspServerStatus {
            if let message = lspServerStatus.message, !message.isEmpty {
                return "LSP \(message)"
            }
            if lspServerStatus.initialized {
                return "LSP ready"
            }
            if lspServerStatus.running {
                return "LSP starting"
            }
        }

        return nil
    }

    private func scheduleFormatSaveTimeout(_ requestID: UInt64) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
            guard let self, self.pendingFormatSaveRequestID == requestID else { return }
            self.pendingFormatSaveRequestID = nil
            self.statusOverride = "Format timed out; saving"
            self.finishSavingDocument()
        }
    }

    @objc private func showLspMenu() {
        let menu = NSMenu()
        guard let lspSession,
              let language = try? editor.syntaxLanguage() else {
            let item = NSMenuItem(title: "No LSP for this file", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
            popUpLspMenu(menu)
            return
        }

        do {
            let state = try lspSession.state(for: editor)
            lspServerStatus = state.serverStatus
            lspDiagnostics = state.diagnostics

            if state.serverStatus?.running == true {
                let status = NSMenuItem(title: "LSP Active", action: nil, keyEquivalent: "")
                status.isEnabled = false
                menu.addItem(status)
                menu.addItem(NSMenuItem.separator())
                menu.addItem(lspMenuItem(title: "Off", action: #selector(turnOffLsp(_:)), payload: EditorLspMenuPayload(language: language)))
                menu.addItem(lspMenuItem(title: "Restart", action: #selector(restartLsp(_:)), payload: EditorLspMenuPayload(language: language)))
            } else if let serverInfo = try lspSession.serverInfo(for: language) {
                let status = NSMenuItem(title: "LSP Off", action: nil, keyEquivalent: "")
                status.isEnabled = false
                menu.addItem(status)
                menu.addItem(NSMenuItem.separator())

                if serverInfo.installed {
                    menu.addItem(lspMenuItem(
                        title: "Start \(serverInfo.name)",
                        action: #selector(startLsp(_:)),
                        payload: EditorLspMenuPayload(language: language, serverInfo: serverInfo)
                    ))
                } else if serverInfo.installable {
                    menu.addItem(lspMenuItem(
                        title: "Install \(serverInfo.name)...",
                        action: #selector(showLspInstall(_:)),
                        payload: EditorLspMenuPayload(language: language, serverInfo: serverInfo)
                    ))
                } else {
                    let item = NSMenuItem(title: "\(serverInfo.name) is not installed", action: nil, keyEquivalent: "")
                    item.isEnabled = false
                    menu.addItem(item)
                }

                if let hint = serverInfo.installHint, !hint.isEmpty {
                    let hintItem = NSMenuItem(title: hint, action: nil, keyEquivalent: "")
                    hintItem.isEnabled = false
                    menu.addItem(hintItem)
                }
            } else {
                let item = NSMenuItem(title: "No LSP server configured", action: nil, keyEquivalent: "")
                item.isEnabled = false
                menu.addItem(item)
            }
        } catch {
            let item = NSMenuItem(title: "LSP unavailable", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
            setEditorError("LSP unavailable: \(error.localizedDescription)", source: .lsp)
        }

        popUpLspMenu(menu)
    }

    private func lspMenuItem(title: String, action: Selector, payload: EditorLspMenuPayload) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        item.representedObject = payload
        return item
    }

    private func popUpLspMenu(_ menu: NSMenu) {
        menu.popUp(
            positioning: nil,
            at: NSPoint(x: 0, y: lspButton.bounds.height + 2),
            in: lspButton
        )
    }

    @objc private func startLsp(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? EditorLspMenuPayload else { return }
        startLsp(language: payload.language)
    }

    @objc private func restartLsp(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? EditorLspMenuPayload,
              let lspSession else { return }
        do {
            try lspSession.stopLsp(language: payload.language)
            lspServerStatus = nil
            lspDiagnostics = []
            lastLspSyncedText = nil
            startLsp(language: payload.language)
        } catch {
            setEditorError("LSP restart failed: \(error.localizedDescription)", source: .lsp)
            updateStatusLabel()
        }
    }

    @objc private func turnOffLsp(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? EditorLspMenuPayload,
              let lspSession else { return }
        do {
            try lspSession.stopLsp(language: payload.language)
            lspServerStatus = nil
            lspDiagnostics = []
            lspMessage = nil
            lastLspSyncedText = nil
            statusOverride = nil
            clearEditorError(source: .lsp)
            applySnapshot(snapshot, notifyTitleChange: false, notifyFileURLChange: false)
        } catch {
            setEditorError("LSP stop failed: \(error.localizedDescription)", source: .lsp)
            updateStatusLabel()
        }
    }

    @objc private func showLspInstall(_ sender: NSMenuItem) {
        guard let payload = sender.representedObject as? EditorLspMenuPayload,
              let serverInfo = payload.serverInfo else { return }

        let installHint = serverInfo.installHint?.trimmingCharacters(in: .whitespacesAndNewlines)
        let detail = installHint?.isEmpty == false
            ? installHint!
            : "\(serverInfo.command) is not available on PATH."

        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(detail, forType: .string)

        statusOverride = "Install hint copied"
        setEditorError("Install \(serverInfo.name): \(detail)", source: .lsp)
        updateStatusLabel()
    }

    private func startLsp(language: VikerSyntaxLanguage) {
        guard let lspSession else { return }
        do {
            statusOverride = "Starting LSP"
            lspMessage = "starting"
            updateStatusLabel()

            let status = try lspSession.startLsp(language: language, owner: self)
            lspServerStatus = status
            lspMessage = status.message ?? "starting"
            lastLspSyncedText = nil
            clearEditorError(source: .lsp)

            try lspSession.syncDocument(editor: editor, owner: self)
            lastLspSyncedText = snapshot.text
            statusOverride = nil
            refreshLspState(message: status.message ?? "starting")
        } catch {
            lspServerStatus = nil
            lspDiagnostics = []
            lspMessage = nil
            setEditorError("LSP start failed: \(error.localizedDescription)", source: .lsp)
            updateStatusLabel()
        }
    }

    @objc private func showWorkspaceSymbols() {
        guard pendingWorkspaceSymbolsRequestID == nil else { return }
        guard let lspSession, isLspRunning else {
            showLspMenu()
            return
        }

        do {
            guard let language = try editor.syntaxLanguage() else {
                statusOverride = "No symbol provider"
                updateStatusLabel()
                return
            }

            statusOverride = "Loading symbols"
            updateStatusLabel()
            let request = try lspSession.requestWorkspaceSymbols(language: language, query: "", owner: self)
            guard let requestID = request.id else {
                statusOverride = nil
                presentWorkspaceSymbols([])
                updateStatusLabel()
                return
            }

            if request.completed {
                pendingWorkspaceSymbolsRequestID = nil
                presentWorkspaceSymbols(try lspSession.workspaceSymbols(requestID: requestID))
                statusOverride = nil
                updateStatusLabel()
            } else {
                pendingWorkspaceSymbolsRequestID = requestID
            }
        } catch {
            present(error)
        }
    }

    private func presentWorkspaceSymbols(_ symbols: [VikerWorkspaceSymbol]) {
        let menu = NSMenu()
        let targets = symbols.compactMap { symbol -> (String, EditorSymbolTarget)? in
            guard let url = Self.fileURL(fromURI: symbol.uri) else { return nil }
            let suffix = symbol.kindLabel.isEmpty ? "" : " - \(symbol.kindLabel)"
            return (
                "\(symbol.name)\(suffix)",
                EditorSymbolTarget(url: url, row: symbol.startLine, column: symbol.startColumn)
            )
        }

        if targets.isEmpty {
            let item = NSMenuItem(title: "No Symbols", action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
        } else {
            for (title, target) in targets.prefix(80) {
                let item = NSMenuItem(title: title, action: #selector(openWorkspaceSymbol(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = target
                menu.addItem(item)
            }
        }

        menu.popUp(
            positioning: nil,
            at: NSPoint(x: 0, y: symbolsButton.bounds.height + 2),
            in: symbolsButton
        )
    }

    @objc private func openWorkspaceSymbol(_ sender: NSMenuItem) {
        guard let target = sender.representedObject as? EditorSymbolTarget else { return }
        if target.url.standardizedFileURL == currentFileURL?.standardizedFileURL {
            jumpTo(row: target.row, column: target.column)
        } else {
            onOpenLocation?(VikerEditorLocation(url: target.url, row: target.row, column: target.column))
        }
    }

    private func fileURL(from payload: String?) -> URL? {
        guard let raw = payload?.trimmingCharacters(in: .whitespacesAndNewlines), !raw.isEmpty else {
            return nil
        }

        if let url = URL(string: raw), url.isFileURL {
            return url.standardizedFileURL
        }

        let expanded: String
        if raw == "~" {
            expanded = FileManager.default.homeDirectoryForCurrentUser.path
        } else if raw.hasPrefix("~/") {
            expanded = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(raw.dropFirst(2)))
                .path
        } else {
            expanded = raw
        }

        if expanded.hasPrefix("/") {
            return URL(fileURLWithPath: expanded).standardizedFileURL
        }

        let baseURL = currentFileURL?.deletingLastPathComponent()
            ?? URL(fileURLWithPath: FileManager.default.currentDirectoryPath, isDirectory: true)
        return baseURL.appendingPathComponent(expanded).standardizedFileURL
    }

    private func present(_ error: Error) {
        statusOverride = "Error"
        setEditorError(error.localizedDescription, source: .operation)
        updateStatusLabel()
    }

    private func installThemeObserver() {
        guard themeObserverToken == nil else { return }
        themeObserverToken = NotificationCenter.default.addObserver(
            forName: VikerEditorThemeManager.didChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.applyTheme(refreshSnapshot: true)
            }
        }
    }

    private func removeThemeObserver() {
        if let themeObserverToken {
            NotificationCenter.default.removeObserver(themeObserverToken)
            self.themeObserverToken = nil
        }
    }

    private func applyTheme(refreshSnapshot shouldRefreshSnapshot: Bool) {
        let colorScheme = configuration.colorScheme
        VikerEditorThemeManager.shared.colorScheme = colorScheme
        containerView.layer?.backgroundColor = colorScheme.editorBackground.cgColor
        toolbar.applyTheme(colorScheme: colorScheme)
        pathLabel.textColor = colorScheme.toolbarText
        statusLabel.textColor = colorScheme.toolbarText
        commandLineView.layer?.backgroundColor = colorScheme.commandLineBackground.cgColor
        commandLineLabel.textColor = colorScheme.toolbarText
        errorBar.layer?.backgroundColor = colorScheme.errorBackground.cgColor
        errorLabel.textColor = colorScheme.errorForeground
        modeLabel.textColor = colorScheme.modeText
        modeLabel.applyVikerEditorLayer(
            backgroundColor: colorScheme.modeBackground,
            cornerRadius: VikerEditorDesign.Radius.control
        )
        updateSaveButtonTint()

        editorView.applyTheme(colorScheme: colorScheme, viewportWidth: scrollView.contentView.bounds.width)
        scrollView.drawsBackground = true
        scrollView.backgroundColor = colorScheme.editorBackground
        scrollView.contentView.backgroundColor = colorScheme.editorBackground
        scrollView.needsDisplay = true

        if shouldRefreshSnapshot {
            refreshSnapshot(syncLsp: false)
        }
    }

    private func updateSaveButtonTint() {
        let colorScheme = configuration.colorScheme
        saveButton.contentTintColor = snapshot.modified
            ? colorScheme.cursor
            : colorScheme.toolbarText
    }

    private func updateEditorViewportSize() throws {
        let size = editorView.viewportGridSize(viewportSize: scrollView.contentView.bounds.size)
        try editor.setViewportSize(width: size.width, height: size.height)
    }

    private static func makeRenderState(
        editor: VikerEditor,
        snapshot: VikerSnapshot,
        diagnostics: [VikerDiagnostic],
        transientSelection: EditorTransientSelection? = nil,
        renderError: inout String?
    ) throws -> EditorRenderState {
        let lineCount = max(intClamped(snapshot.lineCount), 1)
        var displayRows: [[VikerDisplayCell]] = []
        displayRows.reserveCapacity(lineCount)
        var lineDisplayWidths: [Int] = []
        lineDisplayWidths.reserveCapacity(lineCount)
        var highlightRows = Array(repeating: [VikerHighlightSpan](), count: lineCount)
        var diagnosticRows = Array(repeating: [VikerDiagnostic](), count: lineCount)

        for row in 0..<lineCount {
            let rowValue = UInt64(row)
            if rowValue < snapshot.lineCount {
                displayRows.append(try editor.displayCells(row: rowValue))
                lineDisplayWidths.append(intClamped(try editor.lineDisplayWidth(row: rowValue)))
            } else {
                displayRows.append([])
                lineDisplayWidths.append(0)
            }
        }

        do {
            for span in try editor.highlightSpans(start: 0, count: UInt64(lineCount)) {
                let row = intClamped(span.row)
                guard highlightRows.indices.contains(row) else { continue }
                highlightRows[row].append(span)
            }
        } catch {
            renderError = "Syntax highlighting failed: \(error.localizedDescription)"
        }

        for diagnostic in diagnostics {
            let startRow = clamped(intClamped(diagnostic.startLine), lowerBound: 0, upperBound: lineCount - 1)
            let endRow = clamped(intClamped(diagnostic.endLine), lowerBound: startRow, upperBound: lineCount - 1)
            for row in startRow...endRow {
                diagnosticRows[row].append(diagnostic)
            }
        }

        let cursorViewCell = Self.viewCell(
            for: snapshot.cursor,
            displayRows: displayRows,
            lineDisplayWidths: lineDisplayWidths
        )
        let visualAnchorViewCell = snapshot.visualAnchor.map {
            Self.viewCell(for: $0, displayRows: displayRows, lineDisplayWidths: lineDisplayWidths)
        }
        let renderSelection = Self.renderSelection(
            snapshot: snapshot,
            transientSelection: transientSelection,
            cursorViewCell: cursorViewCell,
            visualAnchorViewCell: visualAnchorViewCell,
            displayRows: displayRows,
            lineDisplayWidths: lineDisplayWidths
        )

        return EditorRenderState(
            snapshot: snapshot,
            displayRows: displayRows,
            lineDisplayWidths: lineDisplayWidths,
            highlightRows: highlightRows,
            diagnosticRows: diagnosticRows,
            syntaxLanguage: try editor.syntaxLanguage(),
            cursorViewCell: cursorViewCell,
            visualAnchorViewCell: visualAnchorViewCell,
            selection: renderSelection
        )
    }

    private static func renderSelection(
        snapshot: VikerSnapshot,
        transientSelection: EditorTransientSelection?,
        cursorViewCell: VikerViewCell?,
        visualAnchorViewCell: VikerViewCell?,
        displayRows: [[VikerDisplayCell]],
        lineDisplayWidths: [Int]
    ) -> EditorRenderSelection? {
        if let transientSelection {
            return EditorRenderSelection(
                anchor: viewCell(
                    for: transientSelection.anchor,
                    displayRows: displayRows,
                    lineDisplayWidths: lineDisplayWidths
                ),
                cursor: viewCell(
                    for: transientSelection.cursor,
                    displayRows: displayRows,
                    lineDisplayWidths: lineDisplayWidths
                ),
                mode: transientSelection.mode,
                usesInsertionEndpoints: transientSelection.usesInsertionEndpoints
            )
        }

        guard let cursorViewCell,
              let visualAnchorViewCell,
              let mode = selectionMode(for: snapshot.mode) else {
            return nil
        }

        return EditorRenderSelection(
            anchor: visualAnchorViewCell,
            cursor: cursorViewCell,
            mode: mode,
            usesInsertionEndpoints: false
        )
    }

    private static func selectionMode(for mode: VikerMode) -> VikerSelectionMode? {
        switch mode {
        case .visual:
            return .character
        case .visualLine:
            return .line
        case .visualBlock:
            return .block
        default:
            return nil
        }
    }

    private static func isTextSelectionMode(_ mode: VikerMode) -> Bool {
        mode == .insert || mode == .replace
    }

    private static func isWordScalar(_ scalar: UnicodeScalar) -> Bool {
        scalar == "_" || CharacterSet.alphanumerics.contains(scalar)
    }

    private static func precedesPosition(_ lhs: VikerPosition, _ rhs: VikerPosition) -> Bool {
        lhs.row < rhs.row || (lhs.row == rhs.row && lhs.column < rhs.column)
    }

    private static func lineStartOffsets(in text: String) -> [Int] {
        var starts = [0]
        for (offset, scalar) in text.unicodeScalars.enumerated() where scalar == "\n" {
            starts.append(offset + 1)
        }
        return starts
    }

    private static func scalarOffset(for position: VikerPosition, in text: String) -> Int {
        let starts = lineStartOffsets(in: text)
        guard !starts.isEmpty else { return 0 }

        let row = clamped(intClamped(position.row), lowerBound: 0, upperBound: starts.count - 1)
        let scalarCount = text.unicodeScalars.count
        let lineStart = starts[row]
        let lineEnd = row + 1 < starts.count ? starts[row + 1] - 1 : scalarCount
        return clamped(lineStart + intClamped(position.column), lowerBound: lineStart, upperBound: lineEnd)
    }

    private static func position(forScalarOffset offset: Int, in text: String) -> VikerPosition {
        let starts = lineStartOffsets(in: text)
        guard !starts.isEmpty else {
            return VikerPosition(row: 0, column: 0)
        }

        let scalarCount = text.unicodeScalars.count
        let clampedOffset = clamped(offset, lowerBound: 0, upperBound: scalarCount)
        var row = 0
        for index in starts.indices {
            if starts[index] <= clampedOffset {
                row = index
            } else {
                break
            }
        }

        let lineStart = starts[row]
        let lineEnd = row + 1 < starts.count ? starts[row + 1] - 1 : scalarCount
        let column = clamped(clampedOffset - lineStart, lowerBound: 0, upperBound: max(lineEnd - lineStart, 0))
        return VikerPosition(row: UInt64(row), column: UInt64(column))
    }

    private static func stringBySelectingUnicodeScalars(in text: String, range: Range<Int>) -> String {
        let scalars = Array(text.unicodeScalars)
        let clampedRange = clampedUnicodeScalarRange(range, count: scalars.count)
        var view = String.UnicodeScalarView()
        view.append(contentsOf: scalars[clampedRange])
        return String(view)
    }

    private static func replacingUnicodeScalars(in text: String, range: Range<Int>, with replacement: String) -> String {
        let scalars = Array(text.unicodeScalars)
        let clampedRange = clampedUnicodeScalarRange(range, count: scalars.count)
        var view = String.UnicodeScalarView()
        view.append(contentsOf: scalars[..<clampedRange.lowerBound])
        view.append(contentsOf: replacement.unicodeScalars)
        view.append(contentsOf: scalars[clampedRange.upperBound...])
        return String(view)
    }

    private static func clampedUnicodeScalarRange(_ range: Range<Int>, count: Int) -> Range<Int> {
        let lowerBound = clamped(range.lowerBound, lowerBound: 0, upperBound: count)
        let upperBound = clamped(range.upperBound, lowerBound: lowerBound, upperBound: count)
        return lowerBound..<upperBound
    }

    private static func intClamped(_ value: UInt64) -> Int {
        Int(min(value, UInt64(Int.max)))
    }

    private static func clamped(_ value: Int, lowerBound: Int, upperBound: Int) -> Int {
        min(max(value, lowerBound), upperBound)
    }

    private static func viewCell(
        for position: VikerPosition,
        displayRows: [[VikerDisplayCell]],
        lineDisplayWidths: [Int]
    ) -> VikerViewCell {
        guard !displayRows.isEmpty else {
            return VikerViewCell(row: position.row, column: 0)
        }

        let rowIndex = clamped(intClamped(position.row), lowerBound: 0, upperBound: displayRows.count - 1)
        let lineEnd = lineDisplayWidths.indices.contains(rowIndex) ? UInt64(lineDisplayWidths[rowIndex]) : 0

        for cell in displayRows[rowIndex] {
            let cellWidth = max(cell.cellWidth, 1)
            if position.column <= cell.charStart {
                return VikerViewCell(row: position.row, column: cell.cellStart)
            }

            if position.column < cell.charEnd {
                return VikerViewCell(row: position.row, column: cell.cellStart)
            }

            if position.column == cell.charEnd {
                return VikerViewCell(row: position.row, column: cell.cellStart + cellWidth)
            }
        }

        return VikerViewCell(row: position.row, column: lineEnd)
    }

    private static func vikerKeyEvent(from event: NSEvent) -> VikerKeyEvent? {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let ctrl = flags.contains(.control)
        let alt = flags.contains(.option)
        let shift = flags.contains(.shift)

        let key: VikerKey
        let text: String?

        switch event.keyCode {
        case 36, 76:
            key = .enter
            text = nil
        case 48:
            key = shift ? .backtab : .tab
            text = nil
        case 51:
            key = .backspace
            text = nil
        case 53:
            key = .escape
            text = nil
        case 123:
            key = .left
            text = nil
        case 124:
            key = .right
            text = nil
        case 125:
            key = .down
            text = nil
        case 126:
            key = .up
            text = nil
        default:
            let characters: String?
            if ctrl || alt {
                characters = event.charactersIgnoringModifiers
            } else {
                characters = event.characters
            }
            guard let characters, !characters.isEmpty else { return nil }
            key = .character
            text = characters
        }

        return VikerKeyEvent(key: key, text: text, ctrl: ctrl, alt: alt)
    }

    private static func title(from snapshot: VikerSnapshot, fallbackURL: URL?) -> String {
        if !snapshot.fileName.isEmpty {
            return snapshot.fileName
        }
        if let fallbackURL {
            let name = fallbackURL.lastPathComponent
            return name.isEmpty ? fallbackURL.path : name
        }
        return "Editor"
    }

    private static func fileURL(fromPath path: String) -> URL {
        URL(fileURLWithPath: NSString(string: path).expandingTildeInPath).standardizedFileURL
    }

    private static func fileURL(fromURI uri: String) -> URL? {
        if let url = URL(string: uri), url.isFileURL {
            return url.standardizedFileURL
        }
        guard uri.hasPrefix("/") else { return nil }
        return URL(fileURLWithPath: uri).standardizedFileURL
    }

    private static func displayPath(for url: URL?, relativeTo rootURL: URL?) -> String {
        guard let url else { return "Untitled" }
        let standardizedURL = url.standardizedFileURL

        if let rootURL {
            let rootPath = rootURL.standardizedFileURL.path
            let filePath = standardizedURL.path
            if filePath == rootPath {
                return standardizedURL.lastPathComponent.isEmpty ? "." : standardizedURL.lastPathComponent
            }
            if filePath.hasPrefix(rootPath + "/") {
                return String(filePath.dropFirst(rootPath.count + 1))
            }
        }

        return shortPath(standardizedURL.path)
    }

    private static func shortPath(_ path: String) -> String {
        let standardized = NSString(string: path).expandingTildeInPath
        let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        if standardized == home { return "~" }
        if standardized.hasPrefix(home + "/") {
            return "~" + String(standardized.dropFirst(home.count))
        }
        return standardized
    }

    private static func label(for mode: VikerMode) -> String {
        switch mode {
        case .normal: return "NORM"
        case .insert: return "INS"
        case .replace: return "REP"
        case .visual: return "VIS"
        case .visualLine: return "V-LN"
        case .visualBlock: return "V-BK"
        case .command: return "CMD"
        case .search: return "SRCH"
        }
    }
}

private enum EditorErrorSource: Hashable {
    case operation
    case render
    case lsp
}

@MainActor
final class VikerEditorErrorContent {
    private let containerView = NSView()
    private let titleLabel = NSTextField.vikerEditorLabel("Editor unavailable", style: .titleSmall)
    private let detailLabel = NSTextField.vikerEditorLabel("", style: .caption, color: .secondaryLabelColor)

    let title: String
    var onTitleChange: ((String) -> Void)?
    var onBecomeActive: (() -> Void)?

    init(url: URL, error: Error) {
        self.title = url.lastPathComponent.isEmpty ? url.path : url.lastPathComponent
        detailLabel.stringValue = "\(url.path)\n\n\(error.localizedDescription)"
        setupViews()
    }

    var view: NSView { containerView }
    var isProcessAlive: Bool { false }

    func makeFirstResponder() {
        containerView.window?.makeFirstResponder(containerView)
    }

    func willClose() {}

    private func setupViews() {
        containerView.wantsLayer = true
        containerView.layer?.backgroundColor = VikerEditorDesign.Color.editorBackground.cgColor

        titleLabel.alignment = .center
        titleLabel.textColor = VikerEditorDesign.Color.editorForeground
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        detailLabel.alignment = .center
        detailLabel.textColor = VikerEditorDesign.Color.editorToolbarText
        detailLabel.maximumNumberOfLines = 4
        detailLabel.lineBreakMode = .byWordWrapping
        detailLabel.translatesAutoresizingMaskIntoConstraints = false

        containerView.addSubview(titleLabel)
        containerView.addSubview(detailLabel)

        NSLayoutConstraint.activate([
            titleLabel.centerXAnchor.constraint(equalTo: containerView.centerXAnchor),
            titleLabel.bottomAnchor.constraint(equalTo: containerView.centerYAnchor, constant: -6),
            titleLabel.leadingAnchor.constraint(greaterThanOrEqualTo: containerView.leadingAnchor, constant: 24),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: containerView.trailingAnchor, constant: -24),

            detailLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 8),
            detailLabel.leadingAnchor.constraint(equalTo: containerView.leadingAnchor, constant: 30),
            detailLabel.trailingAnchor.constraint(equalTo: containerView.trailingAnchor, constant: -30),
        ])
    }
}

@MainActor
private struct EditorMouseCell {
    let row: UInt64
    let column: UInt64
    let clickCount: Int
    let modifierFlags: NSEvent.ModifierFlags
}

private enum EditorTextSelectionGranularity {
    case character
    case word
    case paragraph
}

private enum EditorTextDirection {
    case backward
    case forward
}

private struct EditorTextSelectionAnchor {
    let lowerBound: VikerPosition
    let upperBound: VikerPosition
    let granularity: EditorTextSelectionGranularity
}

private struct EditorTextHistorySnapshot {
    let text: String
    let cursor: VikerPosition
}

private struct EditorTextHistoryEntry {
    let before: EditorTextHistorySnapshot
    let after: EditorTextHistorySnapshot
}

private struct EditorTransientSelection {
    let anchor: VikerPosition
    let cursor: VikerPosition
    let mode: VikerSelectionMode
    let usesInsertionEndpoints: Bool
}

private struct EditorTransientSelectionDrag {
    let sourceRange: Range<Int>
    let selectedText: String
    var dropPosition: VikerPosition
    var hasMoved: Bool
    var copies: Bool
}

private struct EditorRenderSelection {
    let anchor: VikerViewCell
    let cursor: VikerViewCell
    let mode: VikerSelectionMode
    let usesInsertionEndpoints: Bool
}

private struct EditorRenderState {
    let snapshot: VikerSnapshot
    let displayRows: [[VikerDisplayCell]]
    let lineDisplayWidths: [Int]
    let highlightRows: [[VikerHighlightSpan]]
    let diagnosticRows: [[VikerDiagnostic]]
    let syntaxLanguage: VikerSyntaxLanguage?
    let cursorViewCell: VikerViewCell?
    let visualAnchorViewCell: VikerViewCell?
    let selection: EditorRenderSelection?

    var rowCount: Int {
        max(max(displayRows.count, lineDisplayWidths.count), 1)
    }

    var longestDisplayWidth: Int {
        lineDisplayWidths.max() ?? 0
    }

    func displayCells(row: Int) -> [VikerDisplayCell] {
        guard displayRows.indices.contains(row) else { return [] }
        return displayRows[row]
    }

    func lineDisplayWidth(row: Int) -> Int {
        guard lineDisplayWidths.indices.contains(row) else { return 0 }
        return lineDisplayWidths[row]
    }

    func highlightSpans(row: Int) -> [VikerHighlightSpan] {
        guard highlightRows.indices.contains(row) else { return [] }
        return highlightRows[row]
    }

    func diagnostics(row: Int) -> [VikerDiagnostic] {
        guard diagnosticRows.indices.contains(row) else { return [] }
        return diagnosticRows[row]
    }
}

@MainActor
private final class VikerEditorCanvasView: NSView {
    var onFocus: (() -> Void)?
    var onKeyDown: ((NSEvent) -> Bool)?
    var onPaste: ((String) -> Void)?
    var onCopy: (() -> Bool)?
    var onCut: (() -> Bool)?
    var onSelectAll: (() -> Bool)?
    var onUndo: (() -> Bool)?
    var onRedo: (() -> Bool)?
    var onSave: (() -> Void)?
    var onMouseDown: ((EditorMouseCell) -> Void)?
    var onMouseDragged: ((EditorMouseCell) -> Void)?
    var onMouseUp: (() -> Void)?

    private var renderState: EditorRenderState
    private var colorScheme: VikerEditorColorScheme
    private let showsLineNumbers: Bool

    private var font = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
    private var lineNumberFont = NSFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular)
    private var textAttributes: [NSAttributedString.Key: Any] = [:]
    private var lineNumberAttributes: [NSAttributedString.Key: Any] = [:]
    private var italicFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
    private var charWidth: CGFloat = 8

    private var lineHeight: CGFloat {
        ceil(font.ascender - font.descender + font.leading + 4)
    }

    private var gutterWidth: CGFloat { showsLineNumbers ? 54 : 0 }
    private let textInsetX: CGFloat = 10
    private let verticalInset: CGFloat = 8

    init(renderState: EditorRenderState, colorScheme: VikerEditorColorScheme, showsLineNumbers: Bool) {
        self.renderState = renderState
        self.colorScheme = colorScheme
        self.showsLineNumbers = showsLineNumbers
        super.init(frame: .zero)
        wantsLayer = true
        refreshThemeMetrics()
        layer?.backgroundColor = colorScheme.editorBackground.cgColor
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var acceptsFirstResponder: Bool { true }
    override var isFlipped: Bool { true }

    func setRenderState(_ renderState: EditorRenderState, viewportWidth: CGFloat) {
        self.renderState = renderState
        updateDocumentSize(viewportWidth: viewportWidth)
        needsDisplay = true
        scrollCursorToVisible()
    }

    func applyTheme(colorScheme: VikerEditorColorScheme, viewportWidth: CGFloat) {
        self.colorScheme = colorScheme
        refreshThemeMetrics()
        layer?.backgroundColor = colorScheme.editorBackground.cgColor
        updateDocumentSize(viewportWidth: viewportWidth)
        needsDisplay = true
    }

    func viewportGridSize(viewportSize: NSSize) -> (width: UInt64, height: UInt64) {
        let textWidth = max(viewportSize.width - gutterWidth - (textInsetX * 2), charWidth)
        let textHeight = max(viewportSize.height - (verticalInset * 2), lineHeight)
        let width = max(Int(floor(textWidth / charWidth)), 1)
        let height = max(Int(floor(textHeight / lineHeight)), 1)
        return (UInt64(width), UInt64(height))
    }

    func updateDocumentSize(viewportWidth: CGFloat) {
        let longestLine = renderState.longestDisplayWidth
        let contentWidth = gutterWidth + (textInsetX * 2) + CGFloat(max(longestLine + 2, 20)) * charWidth
        let contentHeight = verticalInset * 2 + CGFloat(renderState.rowCount) * lineHeight
        let viewportHeight = enclosingScrollView?.contentView.bounds.height ?? bounds.height
        setFrameSize(NSSize(width: max(viewportWidth, contentWidth), height: max(viewportHeight, contentHeight)))
    }

    override func draw(_ dirtyRect: NSRect) {
        colorScheme.editorBackground.setFill()
        dirtyRect.fill()

        drawGutter(in: dirtyRect)
        drawActiveLine(in: dirtyRect)
        drawSelection(in: dirtyRect)
        drawLines(in: dirtyRect)
        drawDiagnostics(in: dirtyRect)
        drawCursor()
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        onFocus?()
        onMouseDown?(mouseCell(for: event))
    }

    override func mouseDragged(with event: NSEvent) {
        onMouseDragged?(mouseCell(for: event))
    }

    override func mouseUp(with event: NSEvent) {
        onMouseUp?()
    }

    override func becomeFirstResponder() -> Bool {
        onFocus?()
        needsDisplay = true
        return true
    }

    override func resignFirstResponder() -> Bool {
        needsDisplay = true
        return true
    }

    override func keyDown(with event: NSEvent) {
        if onKeyDown?(event) == true {
            return
        }
        super.keyDown(with: event)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard flags.contains(.command),
              !flags.contains(.option),
              !flags.contains(.control),
              let key = event.charactersIgnoringModifiers?.lowercased() else {
            return super.performKeyEquivalent(with: event)
        }

        switch key {
        case "s":
            onSave?()
            return true
        case "z" where flags.contains(.shift):
            return onRedo?() ?? super.performKeyEquivalent(with: event)
        case "z":
            return onUndo?() ?? super.performKeyEquivalent(with: event)
        case "a":
            return onSelectAll?() ?? super.performKeyEquivalent(with: event)
        case "c":
            return onCopy?() ?? super.performKeyEquivalent(with: event)
        case "x":
            return onCut?() ?? super.performKeyEquivalent(with: event)
        default:
            return super.performKeyEquivalent(with: event)
        }
    }

    @objc func copy(_ sender: Any?) {
        _ = onCopy?()
    }

    @objc func cut(_ sender: Any?) {
        _ = onCut?()
    }

    override func selectAll(_ sender: Any?) {
        _ = onSelectAll?()
    }

    @objc func undo(_ sender: Any?) {
        _ = onUndo?()
    }

    @objc func redo(_ sender: Any?) {
        _ = onRedo?()
    }

    @objc func paste(_ sender: Any?) {
        guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
        onPaste?(text)
    }

    private func drawGutter(in dirtyRect: NSRect) {
        guard showsLineNumbers else { return }
        let gutterRect = NSRect(x: 0, y: dirtyRect.minY, width: gutterWidth, height: dirtyRect.height)
        colorScheme.gutterBackground.setFill()
        gutterRect.fill()
    }

    private func drawLines(in dirtyRect: NSRect) {
        let range = visibleLineRange(in: dirtyRect)
        guard !range.isEmpty else { return }

        for index in range {
            let rowY = verticalInset + CGFloat(index) * lineHeight
            if showsLineNumbers {
                let lineNumber = "\(index + 1)" as NSString
                let numberSize = lineNumber.size(withAttributes: lineNumberAttributes)
                lineNumber.draw(
                    at: NSPoint(x: gutterWidth - numberSize.width - 8, y: rowY + 2),
                    withAttributes: lineNumberAttributes
                )
            }

            drawLine(renderState.displayCells(row: index), row: index, y: rowY + 2)
        }
    }

    private func drawActiveLine(in dirtyRect: NSRect) {
        let range = visibleLineRange(in: dirtyRect)
        guard !range.isEmpty else { return }

        let activeRow = renderState.cursorViewCell.map {
            Self.clamped(Self.intClamped($0.row), lowerBound: 0, upperBound: renderState.rowCount - 1)
        } ?? Self.clamped(Self.intClamped(renderState.snapshot.cursor.row), lowerBound: 0, upperBound: renderState.rowCount - 1)
        guard range.contains(activeRow) else { return }

        colorScheme.activeLineBackground.setFill()
        let rowY = verticalInset + CGFloat(activeRow) * lineHeight
        NSRect(x: gutterWidth, y: rowY, width: bounds.width - gutterWidth, height: lineHeight).fill()
    }

    private func drawSelection(in dirtyRect: NSRect) {
        guard let selection = renderState.selection else {
            return
        }

        let visible = visibleLineRange(in: dirtyRect)
        guard !visible.isEmpty else { return }

        let start: VikerViewCell
        let end: VikerViewCell
        if Self.precedes(selection.cursor, selection.anchor) {
            start = selection.cursor
            end = selection.anchor
        } else {
            start = selection.anchor
            end = selection.cursor
        }

        let startRow = Self.clamped(Self.intClamped(start.row), lowerBound: 0, upperBound: renderState.rowCount - 1)
        let endRow = Self.clamped(Self.intClamped(end.row), lowerBound: 0, upperBound: renderState.rowCount - 1)

        colorScheme.selectionBackground.setFill()
        for row in visible where row >= startRow && row <= endRow {
            let y = verticalInset + CGFloat(row) * lineHeight
            if selection.usesInsertionEndpoints, selection.mode == .character {
                insertionRangeSelectionRect(
                    start: start,
                    end: end,
                    row: row,
                    y: y,
                    startRow: startRow,
                    endRow: endRow
                )?.fill()
                continue
            }

            switch selection.mode {
            case .line:
                fullLineSelectionRect(row: row, y: y).fill()
            case .block:
                blockSelectionRect(start: start, end: end, row: row, y: y).fill()
            case .character:
                characterSelectionRect(start: start, end: end, row: row, y: y, startRow: startRow, endRow: endRow).fill()
            }
        }
    }

    private func drawCursor() {
        guard window?.firstResponder === self else { return }
        guard let cursor = renderState.cursorViewCell else { return }
        let row = Self.clamped(Self.intClamped(cursor.row), lowerBound: 0, upperBound: renderState.rowCount - 1)
        let column = max(Self.intClamped(cursor.column), 0)
        let y = verticalInset + CGFloat(row) * lineHeight
        let x = textOriginX + CGFloat(column) * charWidth
        let width: CGFloat = renderState.snapshot.mode == .insert ? 1.5 : max(2, charWidth)

        colorScheme.cursor.setFill()
        NSRect(x: x, y: y + 2, width: width, height: lineHeight - 4).fill()
    }

    private func drawLine(_ cells: [VikerDisplayCell], row: Int, y: CGFloat) {
        for cell in cells where !cell.text.isEmpty {
            let glyph = displayText(for: cell)
            let column = Self.intClamped(cell.cellStart)
            (glyph as NSString).draw(
                at: NSPoint(x: textOriginX + CGFloat(column) * charWidth, y: y),
                withAttributes: textAttributes(for: highlightSpan(for: cell, row: row))
            )
        }
    }

    private func drawDiagnostics(in dirtyRect: NSRect) {
        let range = visibleLineRange(in: dirtyRect)
        guard !range.isEmpty else { return }

        for row in range {
            let diagnostics = renderState.diagnostics(row: row)
            guard !diagnostics.isEmpty else { continue }

            for (offset, diagnostic) in diagnostics.enumerated() {
                guard let displayRange = diagnosticDisplayRange(diagnostic, row: row) else { continue }
                diagnosticColor(for: diagnostic).setStroke()
                let y = verticalInset + CGFloat(row) * lineHeight + lineHeight - 3 - CGFloat(offset % 2) * 2
                let path = NSBezierPath()
                path.lineWidth = 1.2
                path.move(to: NSPoint(x: textOriginX + CGFloat(displayRange.lowerBound) * charWidth, y: y))
                path.line(to: NSPoint(x: textOriginX + CGFloat(displayRange.upperBound) * charWidth, y: y))
                path.stroke()
            }
        }
    }

    private func scrollCursorToVisible() {
        guard let cursor = renderState.cursorViewCell else { return }
        let row = Self.clamped(Self.intClamped(cursor.row), lowerBound: 0, upperBound: renderState.rowCount - 1)
        let column = max(Self.intClamped(cursor.column), 0)
        let rect = NSRect(
            x: textOriginX + CGFloat(column) * charWidth,
            y: verticalInset + CGFloat(row) * lineHeight,
            width: charWidth,
            height: lineHeight
        ).insetBy(dx: -80, dy: -40)
        scrollToVisible(rect)
    }

    private func visibleLineRange(in dirtyRect: NSRect) -> Range<Int> {
        guard renderState.rowCount > 0,
              lineHeight.isFinite,
              lineHeight > 0,
              dirtyRect.minY.isFinite,
              dirtyRect.maxY.isFinite else {
            return 0..<0
        }

        let rawFirst = Int(floor((dirtyRect.minY - verticalInset) / lineHeight))
        let rawLast = Int(ceil((dirtyRect.maxY - verticalInset) / lineHeight)) + 1
        let first = Self.clamped(rawFirst, lowerBound: 0, upperBound: renderState.rowCount)
        let last = Self.clamped(rawLast, lowerBound: first, upperBound: renderState.rowCount)
        return first..<last
    }

    private var textOriginX: CGFloat {
        gutterWidth + textInsetX
    }

    private func mouseCell(for event: NSEvent) -> EditorMouseCell {
        let point = documentPoint(for: event)
        let row = Int(floor((point.y - verticalInset) / lineHeight))
        let column = Int(floor((point.x - textOriginX) / charWidth))
        let clampedRow = Self.clamped(row, lowerBound: 0, upperBound: renderState.rowCount - 1)
        let maxColumn = renderState.lineDisplayWidth(row: clampedRow)
        let clampedColumn = min(max(column, 0), maxColumn)

        return EditorMouseCell(
            row: UInt64(clampedRow),
            column: UInt64(clampedColumn),
            clickCount: event.clickCount,
            modifierFlags: event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        )
    }

    private func documentPoint(for event: NSEvent) -> NSPoint {
        guard let scrollView = enclosingScrollView else {
            return convert(event.locationInWindow, from: nil)
        }

        let clipView = scrollView.contentView
        let visible = clipView.documentVisibleRect
        guard !visible.isEmpty else {
            return convert(event.locationInWindow, from: nil)
        }

        let pointInScrollView = scrollView.convert(event.locationInWindow, from: nil)
        let clipFrame = clipView.frame
        let localX = Self.clamped(
            pointInScrollView.x - clipFrame.minX,
            lowerBound: 0,
            upperBound: clipView.bounds.width
        )
        let localY = Self.clamped(
            pointInScrollView.y - clipFrame.minY,
            lowerBound: 0,
            upperBound: clipView.bounds.height
        )

        let documentY: CGFloat
        if clipView.isFlipped == isFlipped {
            documentY = visible.minY + localY
        } else {
            documentY = visible.maxY - localY
        }

        return NSPoint(
            x: visible.minX + localX,
            y: documentY
        )
    }

    private func fullLineSelectionRect(row: Int, y: CGFloat) -> NSRect {
        let lineWidth = max(renderState.lineDisplayWidth(row: row), 1)
        return NSRect(
            x: textOriginX,
            y: y,
            width: CGFloat(lineWidth) * charWidth,
            height: lineHeight
        )
    }

    private func blockSelectionRect(start: VikerViewCell, end: VikerViewCell, row: Int, y: CGFloat) -> NSRect {
        let startColumn = min(Self.intClamped(start.column), Self.intClamped(end.column))
        let endColumn = max(Self.intClamped(start.column), Self.intClamped(end.column))
        return NSRect(
            x: textOriginX + CGFloat(startColumn) * charWidth,
            y: y,
            width: CGFloat(max(endColumn - startColumn + 1, 1)) * charWidth,
            height: lineHeight
        )
    }

    private func characterSelectionRect(
        start: VikerViewCell,
        end: VikerViewCell,
        row: Int,
        y: CGFloat,
        startRow: Int,
        endRow: Int
    ) -> NSRect {
        let startColumn: Int
        let endColumn: Int

        if startRow == endRow {
            startColumn = min(Self.intClamped(start.column), Self.intClamped(end.column))
            let endpointColumn = max(Self.intClamped(start.column), Self.intClamped(end.column))
            endColumn = selectionEndColumn(row: row, column: endpointColumn)
        } else if row == startRow {
            startColumn = Self.intClamped(start.column)
            endColumn = max(renderState.lineDisplayWidth(row: row), startColumn + 1)
        } else if row == endRow {
            startColumn = 0
            endColumn = selectionEndColumn(row: row, column: Self.intClamped(end.column))
        } else {
            startColumn = 0
            endColumn = max(renderState.lineDisplayWidth(row: row), 1)
        }

        return NSRect(
            x: textOriginX + CGFloat(max(startColumn, 0)) * charWidth,
            y: y,
            width: CGFloat(max(endColumn - startColumn, 1)) * charWidth,
            height: lineHeight
        )
    }

    private func insertionRangeSelectionRect(
        start: VikerViewCell,
        end: VikerViewCell,
        row: Int,
        y: CGFloat,
        startRow: Int,
        endRow: Int
    ) -> NSRect? {
        let startColumn: Int
        let endColumn: Int

        if startRow == endRow {
            startColumn = min(Self.intClamped(start.column), Self.intClamped(end.column))
            endColumn = max(Self.intClamped(start.column), Self.intClamped(end.column))
        } else if row == startRow {
            startColumn = Self.intClamped(start.column)
            endColumn = max(renderState.lineDisplayWidth(row: row), startColumn)
        } else if row == endRow {
            startColumn = 0
            endColumn = Self.intClamped(end.column)
        } else {
            startColumn = 0
            endColumn = renderState.lineDisplayWidth(row: row)
        }

        guard endColumn > startColumn else { return nil }
        return NSRect(
            x: textOriginX + CGFloat(max(startColumn, 0)) * charWidth,
            y: y,
            width: CGFloat(endColumn - startColumn) * charWidth,
            height: lineHeight
        )
    }

    private func selectionEndColumn(row: Int, column: Int) -> Int {
        for cell in renderState.displayCells(row: row) {
            let start = Self.intClamped(cell.cellStart)
            let width = max(Self.intClamped(cell.cellWidth), 1)
            let end = start + width
            if column >= start && column < end {
                return end
            }
        }
        return max(column + 1, 1)
    }

    private func displayText(for cell: VikerDisplayCell) -> String {
        if cell.text == "\t" {
            return String(repeating: " ", count: max(Self.intClamped(cell.cellWidth), 1))
        }
        return cell.text
    }

    private func highlightSpan(for cell: VikerDisplayCell, row: Int) -> VikerHighlightSpan? {
        let cellStart = cell.charStart
        let cellEnd = max(cell.charEnd, cell.charStart + 1)
        var fallbackSpan: VikerHighlightSpan?

        for span in renderState.highlightSpans(row: row) {
            let spanStart = span.startColumn
            let spanEnd = max(span.endColumn, span.startColumn + 1)
            guard spanStart < cellEnd && spanEnd > cellStart else { continue }
            if spanStart <= cellStart && cellStart < spanEnd {
                return span
            }
            fallbackSpan = fallbackSpan ?? span
        }

        return fallbackSpan
    }

    private func diagnosticDisplayRange(_ diagnostic: VikerDiagnostic, row: Int) -> Range<Int>? {
        let startLine = Self.intClamped(diagnostic.startLine)
        let endLine = Self.intClamped(diagnostic.endLine)
        guard row >= startLine && row <= endLine else { return nil }

        let startColumn: UInt64 = row == startLine ? diagnostic.startColumn : 0
        let endColumn: UInt64
        if row == endLine {
            endColumn = max(diagnostic.endColumn, startColumn + 1)
        } else {
            endColumn = UInt64(max(renderState.lineDisplayWidth(row: row), 1))
        }

        let displayStart = displayColumn(row: row, bufferColumn: startColumn, endpoint: false)
        let displayEnd = max(
            displayColumn(row: row, bufferColumn: endColumn, endpoint: true),
            displayStart + 1
        )
        return displayStart..<displayEnd
    }

    private func displayColumn(row: Int, bufferColumn: UInt64, endpoint: Bool) -> Int {
        let cells = renderState.displayCells(row: row)
        for cell in cells {
            let charStart = cell.charStart
            let charEnd = max(cell.charEnd, cell.charStart + 1)
            let cellStart = Self.intClamped(cell.cellStart)
            let cellWidth = max(Self.intClamped(cell.cellWidth), 1)

            if bufferColumn <= charStart {
                return cellStart
            }

            if bufferColumn < charEnd {
                return endpoint ? cellStart + cellWidth : cellStart
            }

            if bufferColumn == charEnd {
                return cellStart + cellWidth
            }
        }

        return renderState.lineDisplayWidth(row: row)
    }

    private func diagnosticColor(for diagnostic: VikerDiagnostic) -> NSColor {
        switch diagnostic.severity {
        case 1:
            return .systemRed
        case 2:
            return .systemOrange
        default:
            return .systemBlue
        }
    }

    private func refreshThemeMetrics() {
        font = .monospacedSystemFont(ofSize: VikerEditorDesign.Typography.monospaceBody, weight: .regular)
        lineNumberFont = .monospacedDigitSystemFont(ofSize: VikerEditorDesign.Typography.monospaceSmall, weight: .regular)
        textAttributes = [
            .font: font,
            .foregroundColor: colorScheme.editorForeground,
        ]
        lineNumberAttributes = [
            .font: lineNumberFont,
            .foregroundColor: colorScheme.lineNumber,
        ]
        italicFont = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
        charWidth = (" " as NSString).size(withAttributes: [.font: font]).width
    }

    private func textAttributes(for span: VikerHighlightSpan?) -> [NSAttributedString.Key: Any] {
        guard let span else { return textAttributes }
        var attributes = textAttributes
        attributes[.foregroundColor] = syntaxColor(for: span.token)
        if span.style.italic {
            attributes[.font] = italicFont
        }
        return attributes
    }

    private func syntaxColor(for token: VikerSyntaxToken) -> NSColor {
        let theme = Self.syntaxTheme(for: token)
        if let color = colorScheme.syntaxColors[theme.id] {
            return color
        }
        switch token {
        case .text, .unknown:
            return colorScheme.editorForeground
        default:
            return theme.defaultColor
        }
    }

    private static func syntaxTheme(for token: VikerSyntaxToken) -> (id: String, defaultColor: NSColor) {
        switch token {
        case .text:
            return ("color.editorSyntaxText", VikerEditorThemeDefaults.Color.editorSyntaxText)
        case .keyword:
            return ("color.editorSyntaxKeyword", VikerEditorThemeDefaults.Color.editorSyntaxKeyword)
        case .typeName:
            return ("color.editorSyntaxTypeName", VikerEditorThemeDefaults.Color.editorSyntaxTypeName)
        case .tag:
            return ("color.editorSyntaxTag", VikerEditorThemeDefaults.Color.editorSyntaxTag)
        case .attribute:
            return ("color.editorSyntaxAttribute", VikerEditorThemeDefaults.Color.editorSyntaxAttribute)
        case .constructor:
            return ("color.editorSyntaxConstructor", VikerEditorThemeDefaults.Color.editorSyntaxConstructor)
        case .function:
            return ("color.editorSyntaxFunction", VikerEditorThemeDefaults.Color.editorSyntaxFunction)
        case .method:
            return ("color.editorSyntaxMethod", VikerEditorThemeDefaults.Color.editorSyntaxMethod)
        case .macro:
            return ("color.editorSyntaxMacro", VikerEditorThemeDefaults.Color.editorSyntaxMacro)
        case .stringLiteral:
            return ("color.editorSyntaxString", VikerEditorThemeDefaults.Color.editorSyntaxString)
        case .escape:
            return ("color.editorSyntaxEscape", VikerEditorThemeDefaults.Color.editorSyntaxEscape)
        case .character:
            return ("color.editorSyntaxCharacter", VikerEditorThemeDefaults.Color.editorSyntaxCharacter)
        case .numberLiteral:
            return ("color.editorSyntaxNumber", VikerEditorThemeDefaults.Color.editorSyntaxNumber)
        case .booleanLiteral:
            return ("color.editorSyntaxBoolean", VikerEditorThemeDefaults.Color.editorSyntaxBoolean)
        case .constant:
            return ("color.editorSyntaxConstant", VikerEditorThemeDefaults.Color.editorSyntaxConstant)
        case .comment:
            return ("color.editorSyntaxComment", VikerEditorThemeDefaults.Color.editorSyntaxComment)
        case .variable:
            return ("color.editorSyntaxVariable", VikerEditorThemeDefaults.Color.editorSyntaxVariable)
        case .parameter:
            return ("color.editorSyntaxParameter", VikerEditorThemeDefaults.Color.editorSyntaxParameter)
        case .property:
            return ("color.editorSyntaxProperty", VikerEditorThemeDefaults.Color.editorSyntaxProperty)
        case .module:
            return ("color.editorSyntaxModule", VikerEditorThemeDefaults.Color.editorSyntaxModule)
        case .label:
            return ("color.editorSyntaxLabel", VikerEditorThemeDefaults.Color.editorSyntaxLabel)
        case .punctuation:
            return ("color.editorSyntaxPunctuation", VikerEditorThemeDefaults.Color.editorSyntaxPunctuation)
        case .operatorToken:
            return ("color.editorSyntaxOperator", VikerEditorThemeDefaults.Color.editorSyntaxOperator)
        case .heading:
            return ("color.editorSyntaxHeading", VikerEditorThemeDefaults.Color.editorSyntaxHeading)
        case .rawText:
            return ("color.editorSyntaxRawText", VikerEditorThemeDefaults.Color.editorSyntaxRawText)
        case .link:
            return ("color.editorSyntaxLink", VikerEditorThemeDefaults.Color.editorSyntaxLink)
        case .linkUrl:
            return ("color.editorSyntaxLinkUrl", VikerEditorThemeDefaults.Color.editorSyntaxLinkUrl)
        case .emphasis:
            return ("color.editorSyntaxEmphasis", VikerEditorThemeDefaults.Color.editorSyntaxEmphasis)
        case .strong:
            return ("color.editorSyntaxStrong", VikerEditorThemeDefaults.Color.editorSyntaxStrong)
        case .unknown:
            return ("color.editorSyntaxUnknown", VikerEditorThemeDefaults.Color.editorSyntaxUnknown)
        }
    }

    private static func precedes(_ lhs: VikerViewCell, _ rhs: VikerViewCell) -> Bool {
        lhs.row < rhs.row || (lhs.row == rhs.row && lhs.column < rhs.column)
    }

    private static func intClamped(_ value: UInt64) -> Int {
        Int(min(value, UInt64(Int.max)))
    }

    private static func clamped(_ value: Int, lowerBound: Int, upperBound: Int) -> Int {
        min(max(value, lowerBound), upperBound)
    }

    private static func clamped(_ value: CGFloat, lowerBound: CGFloat, upperBound: CGFloat) -> CGFloat {
        min(max(value, lowerBound), upperBound)
    }
}

public struct VikerEditorLocation {
    public let url: URL
    public let row: UInt64
    public let column: UInt64

    public init(url: URL, row: UInt64, column: UInt64) {
        self.url = url
        self.row = row
        self.column = column
    }
}

@MainActor
final class VikerEditorLspWorkspaceSession: NSObject {
    let rootURL: URL

    private let workspace: VikerLspWorkspace
    private var documentByEditorID: [ObjectIdentifier: VikerLspDocument] = [:]
    private var ownerByURI: [String: WeakVikerEditorComponent] = [:]
    private var pendingOwnerByRequestID: [UInt64: WeakVikerEditorComponent] = [:]
    private var pollTimer: Timer?

    init(rootURL: URL) throws {
        self.rootURL = rootURL.standardizedFileURL
        self.workspace = try VikerLspWorkspace.open(rootPath: self.rootURL.path)
        super.init()
    }

    func openDocument(editor: VikerEditor, owner: VikerEditorComponent) throws {
        let document = try workspace.openDocument(editor: editor)
        documentByEditorID[ObjectIdentifier(editor)] = document
        ownerByURI[document.uri] = WeakVikerEditorComponent(owner)
        if isLanguageRunning(document.language) {
            ensurePolling()
            pollSoon()
        }
    }

    func closeDocument(editor: VikerEditor, owner: VikerEditorComponent) {
        let editorID = ObjectIdentifier(editor)
        guard let document = documentByEditorID.removeValue(forKey: editorID) else { return }
        ownerByURI.removeValue(forKey: document.uri)
        try? workspace.closeDocument(uri: document.uri)
        compactOwners()
        if documentByEditorID.isEmpty {
            pollTimer?.invalidate()
            pollTimer = nil
        }
    }

    func syncDocument(editor: VikerEditor, owner: VikerEditorComponent) throws {
        if documentByEditorID[ObjectIdentifier(editor)] == nil {
            try openDocument(editor: editor, owner: owner)
            return
        }

        let document = try workspace.syncDocument(editor: editor)
        documentByEditorID[ObjectIdentifier(editor)] = document
        ownerByURI[document.uri] = WeakVikerEditorComponent(owner)
        pollSoon()
    }

    func saveDocument(editor: VikerEditor, owner: VikerEditorComponent) throws {
        try syncDocument(editor: editor, owner: owner)
        try workspace.saveDocument(editor: editor)
        pollSoon()
    }

    func formatDocumentBeforeSave(editor: VikerEditor, owner: VikerEditorComponent) throws -> VikerLspRequest {
        try syncDocument(editor: editor, owner: owner)
        let request = try workspace.formatDocument(editor: editor)
        if let requestID = request.id {
            pendingOwnerByRequestID[requestID] = WeakVikerEditorComponent(owner)
        }
        pollSoon()
        return request
    }

    func requestWorkspaceSymbols(
        language: VikerSyntaxLanguage,
        query: String,
        owner: VikerEditorComponent
    ) throws -> VikerLspRequest {
        let request = try workspace.requestWorkspaceSymbols(language: language, query: query)
        if let requestID = request.id {
            pendingOwnerByRequestID[requestID] = WeakVikerEditorComponent(owner)
        }
        ensurePolling()
        pollSoon()
        return request
    }

    func workspaceSymbols(requestID: UInt64) throws -> [VikerWorkspaceSymbol] {
        try workspace.workspaceSymbols(requestId: requestID)
    }

    func serverInfo(for language: VikerSyntaxLanguage) throws -> VikerLspServerInfo? {
        try workspace.listLspServers().first { $0.language == language }
    }

    func startLsp(language: VikerSyntaxLanguage, owner: VikerEditorComponent) throws -> VikerLspServerStatus {
        let status = try workspace.startLsp(language: language)
        ensurePolling()
        broadcast(VikerLspWorkspaceEvent(
            kind: .ready,
            language: language,
            uri: nil,
            requestId: nil,
            message: status.message ?? "starting"
        ))
        pollSoon()
        return status
    }

    func stopLsp(language: VikerSyntaxLanguage) throws {
        try workspace.stopLsp(language: language)
        broadcast(VikerLspWorkspaceEvent(
            kind: .ready,
            language: language,
            uri: nil,
            requestId: nil,
            message: nil
        ))
        pollSoon()
    }

    fileprivate func state(for editor: VikerEditor) throws -> EditorLspState {
        let editorID = ObjectIdentifier(editor)
        let document = documentByEditorID[editorID]
        let status = try workspace.status()
        let serverStatus = document.flatMap { document in
            status.servers.first { $0.language == document.language }
        }
        let diagnostics: [VikerDiagnostic]
        if document == nil {
            diagnostics = []
        } else {
            diagnostics = (try? workspace.diagnosticsForEditor(editor: editor)) ?? []
        }
        return EditorLspState(
            document: document,
            serverStatus: serverStatus,
            diagnostics: diagnostics,
            message: serverStatus?.message
        )
    }

    private func isLanguageRunning(_ language: VikerSyntaxLanguage) -> Bool {
        guard let status = try? workspace.status() else { return false }
        return status.servers.contains { $0.language == language && $0.running }
    }

    func stop() {
        pollTimer?.invalidate()
        pollTimer = nil
        try? workspace.stopAllLsp()
        documentByEditorID.removeAll()
        ownerByURI.removeAll()
        pendingOwnerByRequestID.removeAll()
    }

    private func ensurePolling() {
        guard pollTimer == nil else { return }
        let timer = Timer(timeInterval: 0.35, target: self, selector: #selector(pollTimerFired(_:)), userInfo: nil, repeats: true)
        RunLoop.main.add(timer, forMode: .common)
        pollTimer = timer
    }

    @objc private func pollTimerFired(_ timer: Timer) {
        poll()
    }

    private func pollSoon() {
        Task { @MainActor [weak self] in
            self?.poll()
        }
    }

    private func poll() {
        compactOwners()
        do {
            for event in try workspace.pollLsp() {
                dispatch(event)
            }
        } catch {
            let event = VikerLspWorkspaceEvent(
                kind: .error,
                language: nil,
                uri: nil,
                requestId: nil,
                message: error.localizedDescription
            )
            broadcast(event)
        }
    }

    private func dispatch(_ event: VikerLspWorkspaceEvent) {
        var deliveredOwnerIDs = Set<ObjectIdentifier>()

        if let requestID = event.requestId,
           let owner = pendingOwnerByRequestID[requestID]?.value {
            owner.handleLspWorkspaceEvent(event, session: self)
            deliveredOwnerIDs.insert(ObjectIdentifier(owner))
            if isTerminalEvent(event.kind) {
                pendingOwnerByRequestID.removeValue(forKey: requestID)
            }
        }

        if let uri = event.uri,
           let owner = ownerByURI[uri]?.value,
           !deliveredOwnerIDs.contains(ObjectIdentifier(owner)) {
            owner.handleLspWorkspaceEvent(event, session: self)
            deliveredOwnerIDs.insert(ObjectIdentifier(owner))
        }

        if deliveredOwnerIDs.isEmpty {
            broadcast(event)
        }
    }

    private func broadcast(_ event: VikerLspWorkspaceEvent) {
        for owner in liveOwners() {
            owner.handleLspWorkspaceEvent(event, session: self)
        }
    }

    private func liveOwners() -> [VikerEditorComponent] {
        var seen = Set<ObjectIdentifier>()
        var owners: [VikerEditorComponent] = []
        for weakOwner in Array(ownerByURI.values) + Array(pendingOwnerByRequestID.values) {
            guard let owner = weakOwner.value else { continue }
            let id = ObjectIdentifier(owner)
            guard !seen.contains(id) else { continue }
            seen.insert(id)
            owners.append(owner)
        }
        return owners
    }

    private func compactOwners() {
        ownerByURI = ownerByURI.filter { $0.value.value != nil }
        pendingOwnerByRequestID = pendingOwnerByRequestID.filter { $0.value.value != nil }
    }

    private func isTerminalEvent(_ kind: VikerLspEventKind) -> Bool {
        switch kind {
        case .completionUpdated, .hoverUpdated, .referencesUpdated, .workspaceSymbolsUpdated, .formattingApplied, .renameApplied, .error:
            return true
        case .ready, .diagnosticsUpdated:
            return false
        }
    }
}

fileprivate struct EditorLspState {
    let document: VikerLspDocument?
    let serverStatus: VikerLspServerStatus?
    let diagnostics: [VikerDiagnostic]
    let message: String?
}

private final class WeakVikerEditorComponent {
    weak var value: VikerEditorComponent?

    init(_ value: VikerEditorComponent) {
        self.value = value
    }
}

private final class EditorSymbolTarget: NSObject {
    let url: URL
    let row: UInt64
    let column: UInt64

    init(url: URL, row: UInt64, column: UInt64) {
        self.url = url
        self.row = row
        self.column = column
    }
}

private final class EditorLspMenuPayload: NSObject {
    let language: VikerSyntaxLanguage
    let serverInfo: VikerLspServerInfo?

    init(language: VikerSyntaxLanguage, serverInfo: VikerLspServerInfo? = nil) {
        self.language = language
        self.serverInfo = serverInfo
    }
}
#endif
