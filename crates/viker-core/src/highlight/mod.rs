pub mod style;
pub mod theme;

use ropey::Rope;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

use self::style::{SyntaxHighlight, SyntaxStyle};
use self::theme::{default_highlight, highlight_for_capture};

pub use crate::language::LanguageKind as SyntaxLanguage;

/// Per-line highlight spans: Vec of (start_col, end_col, SyntaxHighlight) per visible line.
pub type LineStyles = Vec<Vec<(usize, usize, SyntaxHighlight)>>;

pub struct Highlighter {
    language: SyntaxLanguage,
    parser: Parser,
    query: Query,
    inline_parser: Option<Parser>,
    inline_query: Option<Query>,
}

impl Highlighter {
    pub fn new(language: SyntaxLanguage) -> Option<Self> {
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
        };

        Some(Self {
            language,
            parser,
            query,
            inline_parser,
            inline_query,
        })
    }

    pub fn language(&self) -> SyntaxLanguage {
        self.language
    }

    /// Parse (or reparse) the document. Returns a new syntax tree.
    pub fn parse(&mut self, rope: &Rope, old_tree: Option<&Tree>) -> Option<Tree> {
        self.parser.parse(rope.to_string(), old_tree)
    }

    /// Compute highlight spans for the given line range [start_line, end_line).
    pub fn highlight_lines(
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

        if self.language == SyntaxLanguage::Markdown {
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
