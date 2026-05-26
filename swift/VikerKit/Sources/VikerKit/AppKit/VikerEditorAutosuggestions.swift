#if os(macOS)
import AppKit

public struct VikerEditorContextSuggestion {
    public var id: String
    public var title: String
    public var subtitle: String?
    public var detail: String?
    public var category: String
    public var systemImageName: String?
    public var insertText: String?
    public var fileURL: URL?
    public var action: (() -> Void)?

    public init(
        id: String,
        title: String,
        subtitle: String? = nil,
        detail: String? = nil,
        category: String = "Context",
        systemImageName: String? = nil,
        insertText: String? = nil,
        fileURL: URL? = nil,
        action: (() -> Void)? = nil
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.detail = detail
        self.category = category
        self.systemImageName = systemImageName
        self.insertText = insertText
        self.fileURL = fileURL?.standardizedFileURL
        self.action = action
    }
}

public struct VikerEditorContextSuggestionRequest {
    public let query: String
    public let currentFileURL: URL?
    public let workspaceRootURL: URL?

    public init(query: String, currentFileURL: URL?, workspaceRootURL: URL?) {
        self.query = query
        self.currentFileURL = currentFileURL
        self.workspaceRootURL = workspaceRootURL?.standardizedFileURL
    }
}

public typealias VikerEditorContextSuggestionProvider = @MainActor (VikerEditorContextSuggestionRequest) -> [VikerEditorContextSuggestion]

struct EditorAutosuggestionViewItem {
    let id: String
    let title: String
    let subtitle: String?
    let detail: String?
    let badge: String?
    let systemImageName: String?
    let isEnabled: Bool
}

@MainActor
final class VikerEditorAutosuggestionView: NSView {
    private enum Metrics {
        static let width: CGFloat = 440
        static let rowHeight: CGFloat = 46
        static let headerHeight: CGFloat = 40
        static let emptyHeight: CGFloat = 76
        static let maxVisibleRows = 8
        static let cornerRadius: CGFloat = 8
    }

    var onHover: ((Int) -> Void)?
    var onCommit: ((Int) -> Void)?

    private let headerLabel = NSTextField.vikerEditorLabel("", style: .captionMedium)
    private let queryLabel = NSTextField.vikerEditorLabel("", style: .monospaceCaption, color: .secondaryLabelColor)
    private let statusLabel = NSTextField.vikerEditorLabel("", style: .caption, color: .secondaryLabelColor)
    private let scrollView = NSScrollView()
    private let documentView = NSView()
    private let stackView = NSStackView()

    private var rowViews: [VikerEditorAutosuggestionRowView] = []
    private var items: [EditorAutosuggestionViewItem] = []
    private var selectedIndex: Int = 0
    private var colorScheme = VikerEditorColorScheme.dark

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupViews()
        applyTheme(colorScheme: VikerEditorThemeManager.shared.colorScheme)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var acceptsFirstResponder: Bool { false }

    var preferredSize: NSSize {
        if items.isEmpty {
            return NSSize(width: Metrics.width, height: Metrics.emptyHeight)
        }
        let visibleRows = min(max(items.count, 1), Metrics.maxVisibleRows)
        return NSSize(
            width: Metrics.width,
            height: Metrics.headerHeight + CGFloat(visibleRows) * Metrics.rowHeight
        )
    }

    func update(
        title: String,
        query: String,
        status: String?,
        items: [EditorAutosuggestionViewItem],
        selectedIndex: Int
    ) {
        self.items = items
        self.selectedIndex = clamped(selectedIndex, lowerBound: 0, upperBound: max(items.count - 1, 0))

        headerLabel.stringValue = title
        queryLabel.stringValue = query.isEmpty ? "" : query
        queryLabel.toolTip = query.isEmpty ? nil : query
        statusLabel.stringValue = status ?? (items.isEmpty ? "No suggestions" : "")
        statusLabel.isHidden = !items.isEmpty && status == nil
        scrollView.isHidden = items.isEmpty

        rebuildRows()
        setFrameSize(preferredSize)
        needsLayout = true
        needsDisplay = true
        scrollSelectedRowToVisible()
    }

    func applyTheme(colorScheme: VikerEditorColorScheme) {
        self.colorScheme = colorScheme
        layer?.backgroundColor = colorScheme.commandLineBackground.cgColor
        layer?.borderColor = colorScheme.toolbarBorder.cgColor
        headerLabel.textColor = colorScheme.editorForeground
        queryLabel.textColor = colorScheme.toolbarText
        statusLabel.textColor = colorScheme.toolbarText
        scrollView.backgroundColor = colorScheme.commandLineBackground
        scrollView.contentView.backgroundColor = colorScheme.commandLineBackground
        rowViews.forEach { $0.applyTheme(colorScheme: colorScheme) }
    }

    private func setupViews() {
        translatesAutoresizingMaskIntoConstraints = true
        wantsLayer = true
        isHidden = true
        layer?.cornerRadius = Metrics.cornerRadius
        layer?.borderWidth = 0.5
        layer?.masksToBounds = false
        shadow = NSShadow()
        shadow?.shadowBlurRadius = 18
        shadow?.shadowOffset = NSSize(width: 0, height: -6)
        shadow?.shadowColor = NSColor.black.withAlphaComponent(0.22)

        headerLabel.lineBreakMode = .byTruncatingTail
        headerLabel.translatesAutoresizingMaskIntoConstraints = false
        queryLabel.lineBreakMode = .byTruncatingMiddle
        queryLabel.alignment = .right
        queryLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.alignment = .center
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.translatesAutoresizingMaskIntoConstraints = false

        documentView.translatesAutoresizingMaskIntoConstraints = false

        stackView.orientation = .vertical
        stackView.spacing = 0
        stackView.alignment = .width
        stackView.distribution = .fill
        stackView.translatesAutoresizingMaskIntoConstraints = false
        documentView.addSubview(stackView)
        scrollView.documentView = documentView

        addSubview(headerLabel)
        addSubview(queryLabel)
        addSubview(scrollView)
        addSubview(statusLabel)

        NSLayoutConstraint.activate([
            headerLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            headerLabel.topAnchor.constraint(equalTo: topAnchor, constant: 11),
            headerLabel.trailingAnchor.constraint(lessThanOrEqualTo: queryLabel.leadingAnchor, constant: -8),

            queryLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            queryLabel.centerYAnchor.constraint(equalTo: headerLabel.centerYAnchor),
            queryLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 160),

            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: topAnchor, constant: Metrics.headerHeight),
            scrollView.bottomAnchor.constraint(equalTo: bottomAnchor),

            documentView.widthAnchor.constraint(equalTo: scrollView.contentView.widthAnchor),
            stackView.leadingAnchor.constraint(equalTo: documentView.leadingAnchor),
            stackView.trailingAnchor.constraint(equalTo: documentView.trailingAnchor),
            stackView.topAnchor.constraint(equalTo: documentView.topAnchor),
            stackView.bottomAnchor.constraint(equalTo: documentView.bottomAnchor),

            statusLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            statusLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            statusLabel.topAnchor.constraint(equalTo: headerLabel.bottomAnchor, constant: 8),
            statusLabel.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -12),
        ])
    }

    private func rebuildRows() {
        for rowView in rowViews {
            stackView.removeArrangedSubview(rowView)
            rowView.removeFromSuperview()
        }

        rowViews = items.enumerated().map { index, item in
            let row = VikerEditorAutosuggestionRowView(index: index)
            row.translatesAutoresizingMaskIntoConstraints = false
            row.heightAnchor.constraint(equalToConstant: Metrics.rowHeight).isActive = true
            row.onHover = { [weak self] index in self?.onHover?(index) }
            row.onCommit = { [weak self] index in self?.onCommit?(index) }
            row.configure(item: item, selected: index == selectedIndex)
            row.applyTheme(colorScheme: colorScheme)
            stackView.addArrangedSubview(row)
            return row
        }
    }

    private func scrollSelectedRowToVisible() {
        guard rowViews.indices.contains(selectedIndex) else { return }
        rowViews[selectedIndex].scrollToVisible(rowViews[selectedIndex].bounds)
    }

    private func clamped(_ value: Int, lowerBound: Int, upperBound: Int) -> Int {
        min(max(value, lowerBound), upperBound)
    }
}

@MainActor
private final class VikerEditorAutosuggestionRowView: NSView {
    let index: Int
    var onHover: ((Int) -> Void)?
    var onCommit: ((Int) -> Void)?

    private let iconView = NSImageView()
    private let titleLabel = NSTextField.vikerEditorLabel("", style: .captionMedium)
    private let subtitleLabel = NSTextField.vikerEditorLabel("", style: .caption, color: .secondaryLabelColor)
    private let detailLabel = NSTextField.vikerEditorLabel("", style: .caption, color: .secondaryLabelColor)
    private let badgeLabel = NSTextField.vikerEditorLabel("", style: .monospaceCaptionSemibold, color: .secondaryLabelColor)

    private var trackingArea: NSTrackingArea?
    private var selected = false
    private var enabled = true
    private var colorScheme = VikerEditorColorScheme.dark

    init(index: Int) {
        self.index = index
        super.init(frame: .zero)
        setupViews()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var acceptsFirstResponder: Bool { false }

    func configure(item: EditorAutosuggestionViewItem, selected: Bool) {
        self.selected = selected
        self.enabled = item.isEnabled

        titleLabel.stringValue = item.title
        titleLabel.toolTip = item.title

        subtitleLabel.stringValue = item.subtitle ?? ""
        subtitleLabel.toolTip = item.subtitle
        subtitleLabel.isHidden = item.subtitle?.isEmpty ?? true

        detailLabel.stringValue = item.detail ?? ""
        detailLabel.toolTip = item.detail
        detailLabel.isHidden = item.detail?.isEmpty ?? true

        badgeLabel.stringValue = item.badge ?? ""
        badgeLabel.isHidden = item.badge?.isEmpty ?? true

        if let systemImageName = item.systemImageName {
            iconView.image = NSImage(systemSymbolName: systemImageName, accessibilityDescription: item.title)
        } else {
            iconView.image = nil
        }
        iconView.isHidden = iconView.image == nil

        applyTheme(colorScheme: colorScheme)
    }

    func applyTheme(colorScheme: VikerEditorColorScheme) {
        self.colorScheme = colorScheme
        wantsLayer = true
        layer?.cornerRadius = 0
        layer?.backgroundColor = selected
            ? NSColor.controlAccentColor.withAlphaComponent(0.24).cgColor
            : NSColor.clear.cgColor

        let secondary = enabled ? colorScheme.toolbarText : NSColor.tertiaryLabelColor
        titleLabel.textColor = enabled ? colorScheme.editorForeground : NSColor.tertiaryLabelColor
        subtitleLabel.textColor = secondary
        detailLabel.textColor = secondary
        badgeLabel.textColor = selected ? colorScheme.editorForeground : secondary
        iconView.contentTintColor = selected ? NSColor.controlAccentColor : secondary
    }

    override func updateTrackingAreas() {
        if let trackingArea {
            removeTrackingArea(trackingArea)
        }
        let nextTrackingArea = NSTrackingArea(
            rect: bounds,
            options: [.activeAlways, .mouseEnteredAndExited, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(nextTrackingArea)
        trackingArea = nextTrackingArea
        super.updateTrackingAreas()
    }

    override func mouseEntered(with event: NSEvent) {
        guard enabled else { return }
        onHover?(index)
    }

    override func mouseDown(with event: NSEvent) {
        guard enabled else { return }
        onCommit?(index)
    }

    private func setupViews() {
        wantsLayer = true

        iconView.symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 13, weight: .medium)
        iconView.imageScaling = .scaleProportionallyDown
        iconView.translatesAutoresizingMaskIntoConstraints = false

        titleLabel.lineBreakMode = .byTruncatingTail
        subtitleLabel.lineBreakMode = .byTruncatingMiddle
        detailLabel.lineBreakMode = .byTruncatingTail
        badgeLabel.alignment = .right

        let titleStack = NSStackView(views: [titleLabel, subtitleLabel])
        titleStack.orientation = .horizontal
        titleStack.spacing = 6
        titleStack.alignment = .firstBaseline
        titleStack.distribution = .fill
        titleStack.translatesAutoresizingMaskIntoConstraints = false

        let textStack = NSStackView(views: [titleStack, detailLabel])
        textStack.orientation = .vertical
        textStack.spacing = 1
        textStack.alignment = .leading
        textStack.distribution = .fill
        textStack.translatesAutoresizingMaskIntoConstraints = false

        addSubview(iconView)
        addSubview(textStack)
        addSubview(badgeLabel)

        NSLayoutConstraint.activate([
            iconView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            iconView.centerYAnchor.constraint(equalTo: centerYAnchor),
            iconView.widthAnchor.constraint(equalToConstant: 18),
            iconView.heightAnchor.constraint(equalToConstant: 18),

            textStack.leadingAnchor.constraint(equalTo: iconView.trailingAnchor, constant: 9),
            textStack.centerYAnchor.constraint(equalTo: centerYAnchor),
            textStack.trailingAnchor.constraint(lessThanOrEqualTo: badgeLabel.leadingAnchor, constant: -10),

            badgeLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            badgeLabel.centerYAnchor.constraint(equalTo: centerYAnchor),
            badgeLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 90),
        ])
    }
}
#endif
