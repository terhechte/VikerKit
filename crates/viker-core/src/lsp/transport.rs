use anyhow::{Result, bail};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

/// Read a JSON-RPC message from the LSP server's stdout.
pub async fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<Value> {
    let mut content_length: usize = 0;

    // Read headers
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            bail!("LSP server closed stdout");
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.parse()?;
        }
    }

    if content_length == 0 {
        bail!("Missing Content-Length header");
    }

    // Read body
    let mut buf = vec![0u8; content_length];
    reader.read_exact(&mut buf).await?;

    let msg: Value = serde_json::from_slice(&buf)?;
    Ok(msg)
}

/// Write a JSON-RPC message to the LSP server's stdin.
pub async fn write_message(
    writer: &mut tokio::io::BufWriter<ChildStdin>,
    msg: &Value,
) -> Result<()> {
    let body = serde_json::to_string(msg)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
