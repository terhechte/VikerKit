use serde::{Deserialize, Serialize};

/// Frontend-independent semantic syntax token.
/// Frontends can map this into their own theme at draw time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SyntaxToken {
    #[default]
    Text,
    Keyword,
    TypeName,
    Tag,
    Attribute,
    Constructor,
    Function,
    Method,
    Macro,
    StringLiteral,
    Escape,
    Character,
    NumberLiteral,
    BooleanLiteral,
    Constant,
    Comment,
    Variable,
    Parameter,
    Property,
    Module,
    Label,
    Punctuation,
    Operator,
    Heading,
    RawText,
    Link,
    LinkUrl,
    Emphasis,
    Strong,
    Unknown,
}

/// Frontend-independent syntax style.
/// Frontends convert this into their native render style at draw time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxStyle {
    pub fg: Option<RgbColor>,
    pub italic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxHighlight {
    pub token: SyntaxToken,
    pub style: SyntaxStyle,
}

impl SyntaxHighlight {
    pub fn new(token: SyntaxToken, style: SyntaxStyle) -> Self {
        Self { token, style }
    }
}

impl SyntaxStyle {
    pub fn fg(mut self, color: RgbColor) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
}
