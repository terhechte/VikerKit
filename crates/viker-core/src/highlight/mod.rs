pub mod style;
pub mod theme;

use std::sync::LazyLock;

use ropey::Rope;
use streaming_iterator::StreamingIterator;
use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

use self::style::{SyntaxHighlight, SyntaxStyle, SyntaxToken};
use self::theme::{default_highlight, highlight_for_capture, style_for_token};

pub use crate::language::LanguageKind as SyntaxLanguage;

/// Per-line highlight spans: Vec of (start_col, end_col, SyntaxHighlight) per visible line.
pub type LineStyles = Vec<Vec<(usize, usize, SyntaxHighlight)>>;

static SYNTECT_SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

pub enum SyntaxState {
    TreeSitter(Tree),
    Syntect,
}

impl SyntaxState {
    fn as_tree_sitter(&self) -> Option<&Tree> {
        match self {
            Self::TreeSitter(tree) => Some(tree),
            Self::Syntect => None,
        }
    }
}

pub struct Highlighter {
    language: SyntaxLanguage,
    backend: HighlighterBackend,
}

enum HighlighterBackend {
    TreeSitter(TreeSitterHighlighter),
    Syntect(SyntectHighlighter),
}

struct TreeSitterHighlighter {
    parser: Parser,
    query: Query,
    inline_parser: Option<Parser>,
    inline_query: Option<Query>,
}

struct SyntectHighlighter {
    syntax: &'static SyntaxReference,
}

impl Highlighter {
    pub fn new(language: SyntaxLanguage) -> Option<Self> {
        let backend = TreeSitterHighlighter::new(language)
            .map(HighlighterBackend::TreeSitter)
            .or_else(|| SyntectHighlighter::new(language).map(HighlighterBackend::Syntect))?;

        Some(Self { language, backend })
    }

    pub fn language(&self) -> SyntaxLanguage {
        self.language
    }

    /// Parse (or reparse) the document. Returns syntax state for the active backend.
    pub fn parse(&mut self, rope: &Rope, old_state: Option<&SyntaxState>) -> Option<SyntaxState> {
        match &mut self.backend {
            HighlighterBackend::TreeSitter(backend) => backend
                .parse(rope, old_state.and_then(SyntaxState::as_tree_sitter))
                .map(SyntaxState::TreeSitter),
            HighlighterBackend::Syntect(_) => Some(SyntaxState::Syntect),
        }
    }

    /// Compute highlight spans for the given line range [start_line, end_line).
    pub fn highlight_lines(
        &mut self,
        state: &SyntaxState,
        rope: &Rope,
        start_line: usize,
        end_line: usize,
    ) -> LineStyles {
        match (&mut self.backend, state) {
            (HighlighterBackend::TreeSitter(backend), SyntaxState::TreeSitter(tree)) => {
                backend.highlight_lines(tree, rope, start_line, end_line)
            }
            (HighlighterBackend::Syntect(backend), SyntaxState::Syntect) => {
                backend.highlight_lines(rope, start_line, end_line)
            }
            _ => vec![vec![]; end_line.saturating_sub(start_line)],
        }
    }
}

impl TreeSitterHighlighter {
    fn new(language: SyntaxLanguage) -> Option<Self> {
        let mut parser = Parser::new();
        let (query, inline_parser, inline_query) = match language {
            SyntaxLanguage::Rust => {
                let language = tree_sitter_rust::LANGUAGE.into();
                let query =
                    setup_parser(&mut parser, language, tree_sitter_rust::HIGHLIGHTS_QUERY)?;
                (query, None, None)
            }
            SyntaxLanguage::Markdown => {
                let block_language = tree_sitter_md::LANGUAGE.into();
                let query = setup_parser(
                    &mut parser,
                    block_language,
                    tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
                )?;

                let inline_language = tree_sitter_md::INLINE_LANGUAGE;
                let mut inline_parser = Parser::new();
                inline_parser.set_language(&inline_language.into()).ok()?;
                let inline_query = Query::new(
                    &inline_language.into(),
                    tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                )
                .ok()?;
                (query, Some(inline_parser), Some(inline_query))
            }
            SyntaxLanguage::Html => {
                let language = tree_sitter_html::LANGUAGE.into();
                let query =
                    setup_parser(&mut parser, language, tree_sitter_html::HIGHLIGHTS_QUERY)?;
                (query, None, None)
            }
            SyntaxLanguage::Css => {
                let language = tree_sitter_css::LANGUAGE.into();
                let query = setup_parser(&mut parser, language, tree_sitter_css::HIGHLIGHTS_QUERY)?;
                (query, None, None)
            }
            SyntaxLanguage::JavaScript => {
                let language = tree_sitter_javascript::LANGUAGE.into();
                let query = setup_parser(
                    &mut parser,
                    language,
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                )?;
                (query, None, None)
            }
            SyntaxLanguage::Jsx => {
                let language = tree_sitter_javascript::LANGUAGE.into();
                let query = setup_parser(
                    &mut parser,
                    language,
                    &format!(
                        "{}\n{}",
                        tree_sitter_javascript::HIGHLIGHT_QUERY,
                        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
                    ),
                )?;
                (query, None, None)
            }
            SyntaxLanguage::TypeScript => {
                let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
                let query = setup_parser(
                    &mut parser,
                    language,
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                )?;
                (query, None, None)
            }
            SyntaxLanguage::Tsx => {
                let language = tree_sitter_typescript::LANGUAGE_TSX.into();
                let query = setup_parser(
                    &mut parser,
                    language,
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                )?;
                (query, None, None)
            }
            SyntaxLanguage::Python => {
                let language = tree_sitter_python::LANGUAGE.into();
                let query =
                    setup_parser(&mut parser, language, tree_sitter_python::HIGHLIGHTS_QUERY)?;
                (query, None, None)
            }
            SyntaxLanguage::Fish => {
                let language = tree_sitter_fish::language();
                let query =
                    setup_parser(&mut parser, language, tree_sitter_fish::HIGHLIGHTS_QUERY)?;
                (query, None, None)
            }
            SyntaxLanguage::Bash => {
                let language = tree_sitter_bash::LANGUAGE.into();
                let query = setup_parser(&mut parser, language, tree_sitter_bash::HIGHLIGHT_QUERY)?;
                (query, None, None)
            }
            SyntaxLanguage::Zsh => {
                let language = tree_sitter_zsh::LANGUAGE.into();
                let query = setup_parser(&mut parser, language, tree_sitter_zsh::HIGHLIGHT_QUERY)?;
                (query, None, None)
            }
            _ => return None,
        };

        Some(Self {
            parser,
            query,
            inline_parser,
            inline_query,
        })
    }

    /// Parse (or reparse) the document. Returns a new syntax tree.
    fn parse(&mut self, rope: &Rope, old_tree: Option<&Tree>) -> Option<Tree> {
        self.parser.parse(rope.to_string(), old_tree)
    }

    /// Compute highlight spans for the given line range [start_line, end_line).
    fn highlight_lines(
        &mut self,
        tree: &Tree,
        rope: &Rope,
        start_line: usize,
        end_line: usize,
    ) -> LineStyles {
        let num_lines = end_line.saturating_sub(start_line);
        let mut result: Vec<Vec<(usize, usize, SyntaxHighlight)>> = vec![vec![]; num_lines];

        let source = rope.to_string();
        let source_bytes = source.as_bytes();

        let start_byte = rope.line_to_byte(start_line);
        let end_byte = if end_line < rope.len_lines() {
            rope.line_to_byte(end_line)
        } else {
            rope.len_bytes()
        };

        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(start_byte..end_byte);

        let capture_names = self.query.capture_names();
        let mut captures = cursor.captures(&self.query, tree.root_node(), source_bytes);

        while let Some(&(ref match_, capture_idx)) = captures.next() {
            let capture = &match_.captures[capture_idx];
            let name = capture_names[capture.index as usize];
            let highlight = highlight_for_capture(name);

            let node = capture.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            for line in start_pos.row..=end_pos.row {
                if line < start_line || line >= end_line {
                    continue;
                }
                let rel_line = line - start_line;

                let col_start = if line == start_pos.row {
                    byte_col_to_char_col(rope, line, start_pos.column)
                } else {
                    0
                };

                let col_end = if line == end_pos.row {
                    byte_col_to_char_col(rope, line, end_pos.column)
                } else {
                    rope.line(line).len_chars()
                };

                if col_start < col_end {
                    result[rel_line].push((col_start, col_end, highlight));
                }
            }
        }

        if self.inline_query.is_some() {
            self.highlight_markdown_inline(tree, rope, start_line, end_line, &mut result);
        }

        result
    }

    fn highlight_markdown_inline(
        &mut self,
        tree: &Tree,
        rope: &Rope,
        start_line: usize,
        end_line: usize,
        result: &mut LineStyles,
    ) {
        let Some(inline_query) = &self.inline_query else {
            return;
        };
        let Some(inline_parser) = &mut self.inline_parser else {
            return;
        };

        let source = rope.to_string();
        let source_bytes = source.as_bytes();
        let mut stack = vec![tree.root_node()];

        while let Some(node) = stack.pop() {
            if node.kind() == "inline" {
                let start_byte = node.start_byte();
                let end_byte = node.end_byte().min(source_bytes.len());
                if start_byte < end_byte {
                    let inline_source = &source_bytes[start_byte..end_byte];
                    if let Some(inline_tree) = inline_parser.parse(inline_source, None) {
                        add_inline_captures(
                            inline_query,
                            &inline_tree,
                            inline_source,
                            start_byte,
                            rope,
                            start_line,
                            end_line,
                            result,
                        );
                    }
                }
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.end_position().row >= start_line && child.start_position().row < end_line {
                    stack.push(child);
                }
            }
        }
    }
}

impl SyntectHighlighter {
    fn new(language: SyntaxLanguage) -> Option<Self> {
        let spec = language.spec();
        let syntax_name = spec.syntect_name?;
        let syntax_set = syntect_syntax_set();
        let syntax = syntax_set
            .find_syntax_by_name(syntax_name)
            .or_else(|| {
                spec.extensions
                    .iter()
                    .find_map(|extension| syntax_set.find_syntax_by_extension(extension))
            })
            .or_else(|| {
                spec.filenames
                    .iter()
                    .find_map(|filename| syntax_set.find_syntax_by_token(filename))
            })
            .or_else(|| syntax_set.find_syntax_by_token(spec.id))?;

        Some(Self { syntax })
    }

    fn highlight_lines(&self, rope: &Rope, start_line: usize, end_line: usize) -> LineStyles {
        let num_lines = end_line.saturating_sub(start_line);
        let mut result: Vec<Vec<(usize, usize, SyntaxHighlight)>> = vec![vec![]; num_lines];
        if num_lines == 0 {
            return result;
        }

        let source = rope.to_string();
        let syntax_set = syntect_syntax_set();
        let mut parse_state = ParseState::new(self.syntax);
        let mut scope_stack = ScopeStack::new();

        for (line_idx, line) in LinesWithEndings::from(&source).enumerate() {
            if line_idx >= end_line {
                break;
            }

            let Ok(ops) = parse_state.parse_line(line, syntax_set) else {
                continue;
            };

            let mut column = 0;
            for (segment, op) in ScopeRegionIterator::new(&ops, line) {
                if scope_stack.apply(op).is_err() {
                    return result;
                }

                let segment_len = segment.chars().count();
                if line_idx >= start_line {
                    let token = token_for_syntect_scope_stack(&scope_stack);
                    let visible_segment = segment.trim_end_matches(&['\r', '\n'][..]);
                    let visible_len = visible_segment.chars().count();
                    if visible_len > 0
                        && token != SyntaxToken::Text
                        && token != SyntaxToken::Unknown
                    {
                        let highlight = SyntaxHighlight::new(token, style_for_token(token));
                        result[line_idx - start_line].push((
                            column,
                            column + visible_len,
                            highlight,
                        ));
                    }
                }
                column += segment_len;
            }
        }

        result
    }
}

fn syntect_syntax_set() -> &'static SyntaxSet {
    &SYNTECT_SYNTAX_SET
}

fn token_for_syntect_scope_stack(stack: &ScopeStack) -> SyntaxToken {
    stack
        .scopes
        .iter()
        .rev()
        .find_map(|scope| token_for_syntect_scope(&scope.to_string()))
        .unwrap_or(SyntaxToken::Text)
}

fn token_for_syntect_scope(scope: &str) -> Option<SyntaxToken> {
    if scope.starts_with("comment") {
        Some(SyntaxToken::Comment)
    } else if scope.starts_with("constant.character.escape") {
        Some(SyntaxToken::Escape)
    } else if scope.starts_with("constant.character") {
        Some(SyntaxToken::Character)
    } else if scope.starts_with("string") {
        Some(SyntaxToken::StringLiteral)
    } else if scope.starts_with("constant.numeric") {
        Some(SyntaxToken::NumberLiteral)
    } else if scope.starts_with("constant.language.boolean") {
        Some(SyntaxToken::BooleanLiteral)
    } else if scope.starts_with("constant") {
        Some(SyntaxToken::Constant)
    } else if scope.starts_with("keyword.operator") {
        Some(SyntaxToken::Operator)
    } else if scope.starts_with("keyword") || scope.starts_with("storage") {
        Some(SyntaxToken::Keyword)
    } else if scope.starts_with("entity.name.tag") {
        Some(SyntaxToken::Tag)
    } else if scope.starts_with("entity.other.attribute-name") {
        Some(SyntaxToken::Attribute)
    } else if scope.starts_with("entity.name.function") || scope.starts_with("support.function") {
        Some(SyntaxToken::Function)
    } else if scope.starts_with("entity.name.type")
        || scope.starts_with("entity.name.class")
        || scope.starts_with("support.type")
        || scope.starts_with("support.class")
    {
        Some(SyntaxToken::TypeName)
    } else if scope.starts_with("entity.name.namespace") {
        Some(SyntaxToken::Module)
    } else if scope.starts_with("entity.name.section") {
        Some(SyntaxToken::Label)
    } else if scope.starts_with("support.constant") {
        Some(SyntaxToken::Constant)
    } else if scope.starts_with("support.variable.property")
        || scope.starts_with("variable.other.member")
    {
        Some(SyntaxToken::Property)
    } else if scope.starts_with("variable.parameter") {
        Some(SyntaxToken::Parameter)
    } else if scope.starts_with("variable") {
        Some(SyntaxToken::Variable)
    } else if scope.starts_with("punctuation") {
        Some(SyntaxToken::Punctuation)
    } else if scope.starts_with("markup.heading") {
        Some(SyntaxToken::Heading)
    } else if scope.starts_with("markup.bold") {
        Some(SyntaxToken::Strong)
    } else if scope.starts_with("markup.italic") {
        Some(SyntaxToken::Emphasis)
    } else if scope.starts_with("markup.underline.link") {
        Some(SyntaxToken::LinkUrl)
    } else if scope.starts_with("markup.raw") {
        Some(SyntaxToken::RawText)
    } else {
        None
    }
}

fn setup_parser(
    parser: &mut Parser,
    language: tree_sitter::Language,
    query: &str,
) -> Option<Query> {
    parser.set_language(&language).ok()?;
    Query::new(&language, query).ok()
}

#[allow(clippy::too_many_arguments)]
fn add_inline_captures(
    query: &Query,
    tree: &Tree,
    source: &[u8],
    base_byte: usize,
    rope: &Rope,
    start_line: usize,
    end_line: usize,
    result: &mut LineStyles,
) {
    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(query, tree.root_node(), source);

    while let Some(&(ref match_, capture_idx)) = captures.next() {
        let capture = &match_.captures[capture_idx];
        let name = capture_names[capture.index as usize];
        let highlight = highlight_for_capture(name);
        let start_byte = base_byte + capture.node.start_byte();
        let end_byte = base_byte + capture.node.end_byte();
        add_byte_span(
            rope, start_byte, end_byte, start_line, end_line, highlight, result,
        );
    }
}

fn add_byte_span(
    rope: &Rope,
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
    highlight: SyntaxHighlight,
    result: &mut LineStyles,
) {
    if start_byte >= end_byte || start_byte >= rope.len_bytes() {
        return;
    }
    let end_byte = end_byte.min(rope.len_bytes());
    let span_start_line = rope.byte_to_line(start_byte);
    let span_end_line = rope.byte_to_line(end_byte.saturating_sub(1));

    for line in span_start_line..=span_end_line {
        if line < start_line || line >= end_line {
            continue;
        }
        let rel_line = line - start_line;
        let line_start = rope.line_to_byte(line);
        let line_end = if line + 1 < rope.len_lines() {
            rope.line_to_byte(line + 1)
        } else {
            rope.len_bytes()
        };
        let col_start = byte_col_to_char_col(rope, line, start_byte.saturating_sub(line_start));
        let col_end = byte_col_to_char_col(
            rope,
            line,
            end_byte.min(line_end).saturating_sub(line_start),
        );
        if col_start < col_end {
            result[rel_line].push((col_start, col_end, highlight));
        }
    }
}

/// Convert a byte column offset within a line to a char column offset.
fn byte_col_to_char_col(rope: &Rope, line: usize, byte_col: usize) -> usize {
    let line_byte_start = rope.line_to_byte(line);
    let abs_byte = line_byte_start + byte_col;
    let abs_byte = abs_byte.min(rope.len_bytes());
    let abs_char = rope.byte_to_char(abs_byte);
    let line_char_start = rope.line_to_char(line);
    abs_char.saturating_sub(line_char_start)
}

/// Look up the syntax highlight for a specific position.
pub fn highlight_at(
    line_styles: &[Vec<(usize, usize, SyntaxHighlight)>],
    rel_line: usize,
    col: usize,
) -> SyntaxHighlight {
    if rel_line < line_styles.len() {
        let mut result = default_highlight();
        for &(start, end, highlight) in &line_styles[rel_line] {
            if col >= start && col < end {
                result = highlight;
            }
        }
        result
    } else {
        default_highlight()
    }
}

/// Look up the highlight style for a specific position.
pub fn style_at(
    line_styles: &[Vec<(usize, usize, SyntaxHighlight)>],
    rel_line: usize,
    col: usize,
) -> SyntaxStyle {
    highlight_at(line_styles, rel_line, col).style
}
