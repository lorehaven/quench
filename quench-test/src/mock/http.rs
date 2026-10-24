//! The little HTTP/1.1 the mock server speaks: reading a request, writing a response.

use super::{Request, ResponseTemplate};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// The largest request body the server accepts.
const MAX_BODY: usize = 64 << 20;

const fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// Reads one request, or `None` when the peer closed the connection (or sent something that is not HTTP).
pub async fn read_request<R, W>(
    reader: &mut BufReader<R>,
    writer: &mut W,
) -> Option<(Request, bool)>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        if !line.trim().is_empty() {
            break;
        }
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();

    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let (name, value) = trimmed.split_once(':')?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };

    if header("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")) {
        writer
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .await
            .ok()?;
    }
    let mut body = Vec::new();
    if header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        loop {
            line.clear();
            reader.read_line(&mut line).await.ok()?;
            let size = usize::from_str_radix(line.trim().split(';').next()?.trim(), 16).ok()?;
            if size == 0 {
                // Trailers, then the blank line.
                loop {
                    line.clear();
                    if reader.read_line(&mut line).await.ok()? == 0 || line.trim().is_empty() {
                        break;
                    }
                }
                break;
            }
            if body.len() + size > MAX_BODY {
                return None;
            }
            let start = body.len();
            body.resize(start + size, 0);
            reader.read_exact(&mut body[start..]).await.ok()?;
            let mut crlf = [0u8; 2];
            reader.read_exact(&mut crlf).await.ok()?;
        }
    } else if let Some(length) = header("content-length").and_then(|v| v.parse::<usize>().ok()) {
        if length > MAX_BODY {
            return None;
        }
        body.resize(length, 0);
        reader.read_exact(&mut body).await.ok()?;
    }
    let keep_alive = match header("connection").map(str::to_ascii_lowercase).as_deref() {
        Some("close") => false,
        Some("keep-alive") => true,
        _ => version == "HTTP/1.1",
    };
    Some((Request::new(method, target, headers, body), keep_alive))
}

/// Writes a response (after its delay), closing or keeping the connection as asked.
pub async fn write_response<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    response: &ResponseTemplate,
    keep_alive: bool,
) -> std::io::Result<()> {
    if response.delay > Duration::ZERO {
        tokio::time::sleep(response.delay).await;
    }
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason(response.status)
    );
    let mut has_length = false;
    for (name, value) in &response.headers {
        if name.eq_ignore_ascii_case("content-length") {
            has_length = true;
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !has_length {
        head.push_str(&format!("content-length: {}\r\n", response.body.len()));
    }
    if !keep_alive {
        head.push_str("connection: close\r\n");
    }
    head.push_str("\r\n");
    writer.write_all(head.as_bytes()).await?;
    writer.write_all(&response.body).await?;
    writer.flush().await
}
