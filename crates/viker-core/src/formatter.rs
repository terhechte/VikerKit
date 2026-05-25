use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::language::ToolInvocation;

pub fn format_text(invocation: &ToolInvocation, cwd: &Path, input: &str) -> Result<String> {
    let mut child = Command::new(&invocation.command)
        .args(&invocation.args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("formatter failed");
        bail!("{message}");
    }

    Ok(String::from_utf8(output.stdout)?)
}
