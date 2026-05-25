import AppKit

enum VikerExampleDesign {
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

    enum Color {
        static let editorBackground = NSColor(red: 0.105, green: 0.110, blue: 0.120, alpha: 1)
        static let editorForeground = NSColor(red: 0.830, green: 0.835, blue: 0.845, alpha: 1)
        static let editorToolbarBackground = NSColor(red: 0.135, green: 0.140, blue: 0.150, alpha: 1)
        static let editorToolbarBorder = NSColor(red: 0.240, green: 0.245, blue: 0.260, alpha: 1)
        static let editorToolbarText = NSColor(red: 0.640, green: 0.660, blue: 0.690, alpha: 1)
        static let editorCommandLineBackground = NSColor(red: 0.150, green: 0.155, blue: 0.165, alpha: 1)
        static let editorErrorBackground = NSColor(red: 0.250, green: 0.105, blue: 0.105, alpha: 1)
        static let editorErrorForeground = NSColor(red: 1.000, green: 0.520, blue: 0.500, alpha: 1)
        static let editorModeBackground = NSColor(red: 0.170, green: 0.250, blue: 0.235, alpha: 1)
        static let editorModeText = NSColor(red: 0.620, green: 0.920, blue: 0.820, alpha: 1)
        static let editorCursor = NSColor(red: 0.970, green: 0.760, blue: 0.370, alpha: 1)
        static let editorGutterBackground = NSColor(red: 0.085, green: 0.090, blue: 0.100, alpha: 1)
        static let editorActiveLineBackground = NSColor(red: 0.160, green: 0.165, blue: 0.175, alpha: 1)
        static let editorSelectionBackground = NSColor(red: 0.290, green: 0.360, blue: 0.450, alpha: 0.72)
        static let editorLineNumber = NSColor(red: 0.420, green: 0.435, blue: 0.460, alpha: 1)
        static let controlBackground = NSColor(red: 0.190, green: 0.195, blue: 0.210, alpha: 1)
    }
}

enum VikerExampleThemeDefaults {
    enum Color {
        static let editorSyntaxText = VikerExampleDesign.Color.editorForeground
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
        static let editorSyntaxUnknown = VikerExampleDesign.Color.editorForeground
    }
}

@MainActor
final class VikerExampleThemeManager {
    static let shared = VikerExampleThemeManager()
    static let didChange = Notification.Name("VikerExampleThemeDidChange")

    func color(id: String, default defaultColor: NSColor) -> NSColor {
        defaultColor
    }
}

final class VikerExampleToolbarView: NSView {
    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        applyTheme()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func applyTheme() {
        layer?.backgroundColor = VikerExampleDesign.Color.editorToolbarBackground.cgColor
        layer?.borderColor = VikerExampleDesign.Color.editorToolbarBorder.cgColor
        layer?.borderWidth = 0.5
    }
}

enum VikerExampleTextStyle {
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
    static func exampleLabel(
        _ string: String,
        style: VikerExampleTextStyle,
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

enum VikerExampleButtonStyle {
    case toolbar
    case toolbarCompact
}

extension NSButton {
    func applyExampleButtonStyle(_ style: VikerExampleButtonStyle) {
        isBordered = false
        bezelStyle = .rounded
        focusRingType = .none
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        layer?.backgroundColor = VikerExampleDesign.Color.controlBackground.cgColor
        layer?.cornerRadius = VikerExampleDesign.Radius.control
        contentTintColor = VikerExampleDesign.Color.editorToolbarText
        font = style == .toolbarCompact
            ? NSFont.systemFont(ofSize: 11, weight: .semibold)
            : NSFont.systemFont(ofSize: 12, weight: .medium)
    }
}

extension NSView {
    func applyExampleLayer(backgroundColor: NSColor, cornerRadius: CGFloat) {
        wantsLayer = true
        layer?.backgroundColor = backgroundColor.cgColor
        layer?.cornerRadius = cornerRadius
    }
}
