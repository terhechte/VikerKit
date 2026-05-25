#if os(macOS)
import AppKit

public struct VikerEditorColorScheme {
    public var editorBackground: NSColor
    public var editorForeground: NSColor
    public var toolbarBackground: NSColor
    public var toolbarBorder: NSColor
    public var toolbarText: NSColor
    public var commandLineBackground: NSColor
    public var errorBackground: NSColor
    public var errorForeground: NSColor
    public var modeBackground: NSColor
    public var modeText: NSColor
    public var cursor: NSColor
    public var gutterBackground: NSColor
    public var activeLineBackground: NSColor
    public var selectionBackground: NSColor
    public var lineNumber: NSColor
    public var controlBackground: NSColor
    public var syntaxColors: [String: NSColor]

    public init(
        editorBackground: NSColor,
        editorForeground: NSColor,
        toolbarBackground: NSColor,
        toolbarBorder: NSColor,
        toolbarText: NSColor,
        commandLineBackground: NSColor,
        errorBackground: NSColor,
        errorForeground: NSColor,
        modeBackground: NSColor,
        modeText: NSColor,
        cursor: NSColor,
        gutterBackground: NSColor,
        activeLineBackground: NSColor,
        selectionBackground: NSColor,
        lineNumber: NSColor,
        controlBackground: NSColor,
        syntaxColors: [String: NSColor] = [:]
    ) {
        self.editorBackground = editorBackground
        self.editorForeground = editorForeground
        self.toolbarBackground = toolbarBackground
        self.toolbarBorder = toolbarBorder
        self.toolbarText = toolbarText
        self.commandLineBackground = commandLineBackground
        self.errorBackground = errorBackground
        self.errorForeground = errorForeground
        self.modeBackground = modeBackground
        self.modeText = modeText
        self.cursor = cursor
        self.gutterBackground = gutterBackground
        self.activeLineBackground = activeLineBackground
        self.selectionBackground = selectionBackground
        self.lineNumber = lineNumber
        self.controlBackground = controlBackground
        self.syntaxColors = syntaxColors
    }

    public static let dark = VikerEditorColorScheme(
        editorBackground: NSColor(red: 0.105, green: 0.110, blue: 0.120, alpha: 1),
        editorForeground: NSColor(red: 0.830, green: 0.835, blue: 0.845, alpha: 1),
        toolbarBackground: NSColor(red: 0.135, green: 0.140, blue: 0.150, alpha: 1),
        toolbarBorder: NSColor(red: 0.240, green: 0.245, blue: 0.260, alpha: 1),
        toolbarText: NSColor(red: 0.640, green: 0.660, blue: 0.690, alpha: 1),
        commandLineBackground: NSColor(red: 0.150, green: 0.155, blue: 0.165, alpha: 1),
        errorBackground: NSColor(red: 0.250, green: 0.105, blue: 0.105, alpha: 1),
        errorForeground: NSColor(red: 1.000, green: 0.520, blue: 0.500, alpha: 1),
        modeBackground: NSColor(red: 0.170, green: 0.250, blue: 0.235, alpha: 1),
        modeText: NSColor(red: 0.620, green: 0.920, blue: 0.820, alpha: 1),
        cursor: NSColor(red: 0.970, green: 0.760, blue: 0.370, alpha: 1),
        gutterBackground: NSColor(red: 0.085, green: 0.090, blue: 0.100, alpha: 1),
        activeLineBackground: NSColor(red: 0.160, green: 0.165, blue: 0.175, alpha: 1),
        selectionBackground: NSColor(red: 0.290, green: 0.360, blue: 0.450, alpha: 0.72),
        lineNumber: NSColor(red: 0.420, green: 0.435, blue: 0.460, alpha: 1),
        controlBackground: NSColor(red: 0.190, green: 0.195, blue: 0.210, alpha: 1)
    )

    public static let light = VikerEditorColorScheme(
        editorBackground: .textBackgroundColor,
        editorForeground: .labelColor,
        toolbarBackground: .windowBackgroundColor,
        toolbarBorder: .separatorColor,
        toolbarText: .secondaryLabelColor,
        commandLineBackground: .controlBackgroundColor,
        errorBackground: NSColor.systemRed.withAlphaComponent(0.14),
        errorForeground: .systemRed,
        modeBackground: NSColor.controlAccentColor.withAlphaComponent(0.16),
        modeText: .controlAccentColor,
        cursor: .controlAccentColor,
        gutterBackground: .controlBackgroundColor,
        activeLineBackground: .selectedContentBackgroundColor.withAlphaComponent(0.08),
        selectionBackground: .selectedContentBackgroundColor.withAlphaComponent(0.42),
        lineNumber: .tertiaryLabelColor,
        controlBackground: .controlBackgroundColor
    )
}

@MainActor
enum VikerEditorDesign {
    enum Size {
        static let toolbarHeight: CGFloat = 36
        static let toolbarButtonWidth: CGFloat = 28
    }

    enum Radius {
        static let control: CGFloat = 5
    }

    enum Typography {
        static let monospaceBody: CGFloat = 13
        static let monospaceSmall: CGFloat = 11
    }

    @MainActor
    enum Color {
        static var editorBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.editorBackground }
        static var editorForeground: NSColor { VikerEditorThemeManager.shared.colorScheme.editorForeground }
        static var editorToolbarBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.toolbarBackground }
        static var editorToolbarBorder: NSColor { VikerEditorThemeManager.shared.colorScheme.toolbarBorder }
        static var editorToolbarText: NSColor { VikerEditorThemeManager.shared.colorScheme.toolbarText }
        static var editorCommandLineBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.commandLineBackground }
        static var editorErrorBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.errorBackground }
        static var editorErrorForeground: NSColor { VikerEditorThemeManager.shared.colorScheme.errorForeground }
        static var editorModeBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.modeBackground }
        static var editorModeText: NSColor { VikerEditorThemeManager.shared.colorScheme.modeText }
        static var editorCursor: NSColor { VikerEditorThemeManager.shared.colorScheme.cursor }
        static var editorGutterBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.gutterBackground }
        static var editorActiveLineBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.activeLineBackground }
        static var editorSelectionBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.selectionBackground }
        static var editorLineNumber: NSColor { VikerEditorThemeManager.shared.colorScheme.lineNumber }
        static var controlBackground: NSColor { VikerEditorThemeManager.shared.colorScheme.controlBackground }
    }
}

@MainActor
enum VikerEditorThemeDefaults {
    @MainActor
    enum Color {
        static let editorSyntaxText = VikerEditorDesign.Color.editorForeground
        static let editorSyntaxKeyword = NSColor(red: 0.950, green: 0.470, blue: 0.620, alpha: 1)
        static let editorSyntaxTypeName = NSColor(red: 0.470, green: 0.760, blue: 0.950, alpha: 1)
        static let editorSyntaxTag = NSColor(red: 0.970, green: 0.670, blue: 0.380, alpha: 1)
        static let editorSyntaxAttribute = NSColor(red: 0.740, green: 0.690, blue: 0.980, alpha: 1)
        static let editorSyntaxConstructor = NSColor(red: 0.660, green: 0.820, blue: 0.540, alpha: 1)
        static let editorSyntaxFunction = NSColor(red: 0.550, green: 0.780, blue: 1.000, alpha: 1)
        static let editorSyntaxMethod = NSColor(red: 0.530, green: 0.810, blue: 0.900, alpha: 1)
        static let editorSyntaxMacro = NSColor(red: 0.930, green: 0.650, blue: 0.970, alpha: 1)
        static let editorSyntaxString = NSColor(red: 0.590, green: 0.840, blue: 0.520, alpha: 1)
        static let editorSyntaxEscape = NSColor(red: 0.960, green: 0.820, blue: 0.450, alpha: 1)
        static let editorSyntaxCharacter = NSColor(red: 0.600, green: 0.850, blue: 0.660, alpha: 1)
        static let editorSyntaxNumber = NSColor(red: 0.960, green: 0.730, blue: 0.430, alpha: 1)
        static let editorSyntaxBoolean = NSColor(red: 0.960, green: 0.650, blue: 0.470, alpha: 1)
        static let editorSyntaxConstant = NSColor(red: 0.940, green: 0.760, blue: 0.480, alpha: 1)
        static let editorSyntaxComment = NSColor(red: 0.470, green: 0.520, blue: 0.540, alpha: 1)
        static let editorSyntaxVariable = NSColor(red: 0.850, green: 0.850, blue: 0.760, alpha: 1)
        static let editorSyntaxParameter = NSColor(red: 0.790, green: 0.790, blue: 0.640, alpha: 1)
        static let editorSyntaxProperty = NSColor(red: 0.660, green: 0.840, blue: 0.900, alpha: 1)
        static let editorSyntaxModule = NSColor(red: 0.700, green: 0.780, blue: 0.980, alpha: 1)
        static let editorSyntaxLabel = NSColor(red: 0.940, green: 0.690, blue: 0.540, alpha: 1)
        static let editorSyntaxPunctuation = NSColor(red: 0.700, green: 0.715, blue: 0.735, alpha: 1)
        static let editorSyntaxOperator = NSColor(red: 0.940, green: 0.590, blue: 0.550, alpha: 1)
        static let editorSyntaxHeading = NSColor(red: 0.960, green: 0.790, blue: 0.420, alpha: 1)
        static let editorSyntaxRawText = NSColor(red: 0.780, green: 0.800, blue: 0.760, alpha: 1)
        static let editorSyntaxLink = NSColor(red: 0.470, green: 0.760, blue: 0.980, alpha: 1)
        static let editorSyntaxLinkUrl = NSColor(red: 0.380, green: 0.700, blue: 0.920, alpha: 1)
        static let editorSyntaxEmphasis = NSColor(red: 0.910, green: 0.760, blue: 0.950, alpha: 1)
        static let editorSyntaxStrong = NSColor(red: 0.980, green: 0.860, blue: 0.540, alpha: 1)
        static let editorSyntaxUnknown = VikerEditorDesign.Color.editorForeground
    }
}

@MainActor
final class VikerEditorThemeManager {
    static let shared = VikerEditorThemeManager()
    static let didChange = Notification.Name("VikerEditorThemeDidChange")

    var colorScheme = VikerEditorColorScheme.dark

    func color(id: String, default defaultColor: NSColor) -> NSColor {
        colorScheme.syntaxColors[id] ?? defaultColor
    }
}

final class VikerEditorToolbarView: NSView {
    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        applyTheme(colorScheme: .dark)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func applyTheme(colorScheme: VikerEditorColorScheme) {
        layer?.backgroundColor = colorScheme.toolbarBackground.cgColor
        layer?.borderColor = colorScheme.toolbarBorder.cgColor
        layer?.borderWidth = 0.5
    }
}

enum VikerEditorTextStyle {
    case titleSmall
    case captionMedium
    case caption
    case monospaceCaption
    case monospaceCaptionSemibold

    var font: NSFont {
        switch self {
        case .titleSmall:
            NSFont.systemFont(ofSize: 15, weight: .semibold)
        case .captionMedium:
            NSFont.systemFont(ofSize: 12, weight: .medium)
        case .caption:
            NSFont.systemFont(ofSize: 12, weight: .regular)
        case .monospaceCaption:
            NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        case .monospaceCaptionSemibold:
            NSFont.monospacedSystemFont(ofSize: 11, weight: .semibold)
        }
    }
}

extension NSTextField {
    static func vikerEditorLabel(
        _ string: String,
        style: VikerEditorTextStyle,
        color: NSColor = .labelColor
    ) -> NSTextField {
        let label = NSTextField(labelWithString: string)
        label.font = style.font
        label.textColor = color
        label.isSelectable = false
        label.translatesAutoresizingMaskIntoConstraints = false
        return label
    }
}

enum VikerEditorButtonStyle {
    case toolbar
    case toolbarCompact
}

extension NSButton {
    func applyVikerEditorButtonStyle(_ style: VikerEditorButtonStyle) {
        isBordered = false
        bezelStyle = .rounded
        focusRingType = .none
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        layer?.backgroundColor = VikerEditorDesign.Color.controlBackground.cgColor
        layer?.cornerRadius = VikerEditorDesign.Radius.control
        contentTintColor = VikerEditorDesign.Color.editorToolbarText
        font = style == .toolbarCompact
            ? NSFont.systemFont(ofSize: 11, weight: .semibold)
            : NSFont.systemFont(ofSize: 12, weight: .medium)
    }
}

extension NSView {
    func applyVikerEditorLayer(backgroundColor: NSColor, cornerRadius: CGFloat) {
        wantsLayer = true
        layer?.backgroundColor = backgroundColor.cgColor
        layer?.cornerRadius = cornerRadius
    }
}
#endif
