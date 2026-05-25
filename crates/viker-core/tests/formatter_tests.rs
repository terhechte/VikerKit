use viker_core::formatter;
use viker_core::language::ToolInvocation;

#[test]
fn formatter_captures_stdout() {
    let invocation = ToolInvocation {
        command: "sh".to_string(),
        args: vec!["-c".to_string(), "tr a-z A-Z".to_string()],
    };
    let output = formatter::format_text(&invocation, std::path::Path::new("."), "hello\n")
        .expect("formatter command should run");
    assert_eq!(output, "HELLO\n");
}

#[test]
fn formatter_reports_stderr_on_failure() {
    let invocation = ToolInvocation {
        command: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "echo formatter exploded >&2; exit 2".to_string(),
        ],
    };
    let err = formatter::format_text(&invocation, std::path::Path::new("."), "")
        .expect_err("formatter should fail");
    assert!(err.to_string().contains("formatter exploded"));
}
