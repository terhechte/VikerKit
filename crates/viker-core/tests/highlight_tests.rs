use viker_core::highlight::style::{RgbColor, SyntaxToken};
use viker_core::highlight::{Highlighter, SyntaxLanguage};

#[test]
fn markdown_highlighter_styles_block_and_inline_markup() {
    let rope = ropey::Rope::from_str("# Title\n\nSome **bold** and [link](https://example.com).\n");
    let mut highlighter = Highlighter::new(SyntaxLanguage::Markdown).unwrap();
    let tree = highlighter.parse(&rope, None).unwrap();
    let styles = highlighter.highlight_lines(&tree, &rope, 0, rope.len_lines());

    let heading_style = styles[0]
        .iter()
        .find(|(start, end, _)| *start <= 2 && 2 < *end)
        .map(|(_, _, highlight)| (highlight.token, highlight.style.fg));
    assert_eq!(
        heading_style,
        Some((SyntaxToken::Heading, Some(RgbColor(97, 175, 239))))
    );

    let bold_style = styles[2]
        .iter()
        .find(|(start, end, _)| *start <= 7 && 7 < *end)
        .map(|(_, _, highlight)| (highlight.token, highlight.style.fg));
    assert_eq!(
        bold_style,
        Some((SyntaxToken::Strong, Some(RgbColor(224, 108, 117))))
    );

    let link_style = styles[2]
        .iter()
        .find(|(start, end, _)| *start <= 20 && 20 < *end)
        .map(|(_, _, highlight)| (highlight.token, highlight.style.fg));
    assert_eq!(
        link_style,
        Some((SyntaxToken::Link, Some(RgbColor(229, 192, 123))))
    );
}

#[test]
fn syntax_language_is_selected_from_file_extension() {
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("README.md"))),
        Some(SyntaxLanguage::Markdown)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("src/main.rs"))),
        Some(SyntaxLanguage::Rust)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("notes.txt"))),
        None
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("index.html"))),
        Some(SyntaxLanguage::Html)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("style.css"))),
        Some(SyntaxLanguage::Css)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("app.js"))),
        Some(SyntaxLanguage::JavaScript)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("app.ts"))),
        Some(SyntaxLanguage::TypeScript)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("tool.py"))),
        Some(SyntaxLanguage::Python)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("config.fish"))),
        Some(SyntaxLanguage::Fish)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new("script.sh"))),
        Some(SyntaxLanguage::Bash)
    );
    assert_eq!(
        SyntaxLanguage::from_path(Some(std::path::Path::new(".zshrc"))),
        Some(SyntaxLanguage::Zsh)
    );
}

#[test]
fn requested_language_highlighters_produce_spans() {
    let cases = [
        (SyntaxLanguage::Html, "<main class=\"hero\">Hello</main>\n"),
        (SyntaxLanguage::Css, ".hero { color: red; }\n"),
        (
            SyntaxLanguage::JavaScript,
            "function greet(name) { return `hi ${name}`; }\n",
        ),
        (
            SyntaxLanguage::TypeScript,
            "const greet = (name: string): string => `hi ${name}`;\n",
        ),
        (
            SyntaxLanguage::Python,
            "def greet(name: str) -> str:\n    return f'hi {name}'\n",
        ),
        (SyntaxLanguage::Fish, "function greet\n    echo hi\nend\n"),
        (SyntaxLanguage::Bash, "greet() {\n  echo \"hi\"\n}\n"),
        (SyntaxLanguage::Zsh, "autoload -Uz compinit\ncompinit\n"),
    ];

    for (language, sample) in cases {
        let rope = ropey::Rope::from_str(sample);
        let mut highlighter = Highlighter::new(language).expect("highlighter should initialize");
        let tree = highlighter.parse(&rope, None).expect("sample should parse");
        let styles = highlighter.highlight_lines(&tree, &rope, 0, rope.len_lines());
        assert!(
            styles.iter().any(|line| !line.is_empty()),
            "{language:?} should produce at least one highlight span"
        );
    }
}
