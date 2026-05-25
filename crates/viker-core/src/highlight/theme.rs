use super::style::{RgbColor, SyntaxHighlight, SyntaxStyle, SyntaxToken};

/// One Dark inspired color scheme.
/// Maps tree-sitter capture names to semantic syntax tokens.
pub fn token_for_capture(name: &str) -> SyntaxToken {
    match name {
        "keyword"
        | "keyword.control"
        | "keyword.control.rust"
        | "keyword.modifier"
        | "keyword.type"
        | "keyword.function"
        | "keyword.operator"
        | "keyword.import"
        | "keyword.repeat"
        | "keyword.return"
        | "keyword.conditional"
        | "keyword.exception"
        | "keyword.storage"
        | "keyword.coroutine"
        | "keyword.directive" => SyntaxToken::Keyword,
        "type" | "type.builtin" | "type.qualifier" => SyntaxToken::TypeName,
        "tag" | "tag.builtin" => SyntaxToken::Tag,
        "attribute" | "attribute.builtin" | "tag.attribute" => SyntaxToken::Attribute,
        "constructor" => SyntaxToken::Constructor,
        "function" | "function.call" | "function.builtin" => SyntaxToken::Function,
        "function.method" | "function.method.call" => SyntaxToken::Method,
        "function.macro" => SyntaxToken::Macro,
        "string" | "string.special" | "string.escape" | "string.regexp" => {
            SyntaxToken::StringLiteral
        }
        "text.title" | "markup.heading" | "markup.heading.1" | "markup.heading.2" => {
            SyntaxToken::Heading
        }
        "text.literal" | "markup.raw" | "markup.raw.block" | "markup.raw.inline" => {
            SyntaxToken::RawText
        }
        "text.uri" | "markup.link.url" => SyntaxToken::LinkUrl,
        "text.reference" | "markup.link.label" | "markup.link.text" => SyntaxToken::Link,
        "text.emphasis" | "markup.italic" => SyntaxToken::Emphasis,
        "text.strong" | "markup.bold" => SyntaxToken::Strong,
        "none" => SyntaxToken::Text,
        "character" | "character.special" => SyntaxToken::Character,
        "number" | "number.float" | "float" => SyntaxToken::NumberLiteral,
        "boolean" => SyntaxToken::BooleanLiteral,
        "constant.builtin" | "constant" => SyntaxToken::Constant,
        "comment" | "comment.line" | "comment.block" | "comment.documentation" => {
            SyntaxToken::Comment
        }
        "variable.builtin" => SyntaxToken::Variable,
        "variable.parameter" => SyntaxToken::Parameter,
        "label" => SyntaxToken::Label,
        "punctuation" | "punctuation.bracket" | "punctuation.delimiter" | "punctuation.special" => {
            SyntaxToken::Punctuation
        }
        "operator" => SyntaxToken::Operator,
        "property" | "variable.member" => SyntaxToken::Property,
        "escape" | "string.special.symbol" => SyntaxToken::Escape,
        "module" | "namespace" => SyntaxToken::Module,
        _ => SyntaxToken::Unknown,
    }
}

/// Maps tree-sitter capture names to syntax tokens plus the built-in style.
pub fn highlight_for_capture(name: &str) -> SyntaxHighlight {
    let token = token_for_capture(name);
    SyntaxHighlight::new(token, style_for_token(token))
}

/// Maps tree-sitter capture names to the built-in style.
pub fn style_for_capture(name: &str) -> SyntaxStyle {
    style_for_token(token_for_capture(name))
}

/// Maps a semantic syntax token to the built-in One Dark inspired style.
pub fn style_for_token(token: SyntaxToken) -> SyntaxStyle {
    match token {
        SyntaxToken::Keyword => SyntaxStyle::default().fg(RgbColor(198, 120, 221)), // purple
        SyntaxToken::TypeName
        | SyntaxToken::Attribute
        | SyntaxToken::Constructor
        | SyntaxToken::Link
        | SyntaxToken::Module => SyntaxStyle::default().fg(RgbColor(229, 192, 123)), // yellow
        SyntaxToken::Tag
        | SyntaxToken::Strong
        | SyntaxToken::Variable
        | SyntaxToken::Parameter
        | SyntaxToken::Property => SyntaxStyle::default().fg(RgbColor(224, 108, 117)), // red
        SyntaxToken::Function | SyntaxToken::Method | SyntaxToken::Macro | SyntaxToken::Heading => {
            SyntaxStyle::default().fg(RgbColor(97, 175, 239))
        } // blue
        SyntaxToken::StringLiteral | SyntaxToken::Character | SyntaxToken::RawText => {
            SyntaxStyle::default().fg(RgbColor(152, 195, 121)) // green
        }
        SyntaxToken::LinkUrl | SyntaxToken::Escape => {
            SyntaxStyle::default().fg(RgbColor(86, 182, 194)) // cyan
        }
        SyntaxToken::Emphasis => SyntaxStyle::default().fg(RgbColor(171, 178, 191)).italic(),
        SyntaxToken::NumberLiteral
        | SyntaxToken::BooleanLiteral
        | SyntaxToken::Constant
        | SyntaxToken::Label => SyntaxStyle::default().fg(RgbColor(209, 154, 102)), // orange
        SyntaxToken::Comment => SyntaxStyle::default().fg(RgbColor(92, 99, 112)).italic(), // gray italic
        SyntaxToken::Punctuation
        | SyntaxToken::Operator
        | SyntaxToken::Text
        | SyntaxToken::Unknown => default_style(),
    }
}

/// Default text color (used when no capture matches).
pub fn default_style() -> SyntaxStyle {
    SyntaxStyle::default().fg(RgbColor(171, 178, 191))
}

/// Default syntax highlight (used when no capture matches).
pub fn default_highlight() -> SyntaxHighlight {
    SyntaxHighlight::new(SyntaxToken::Text, default_style())
}
