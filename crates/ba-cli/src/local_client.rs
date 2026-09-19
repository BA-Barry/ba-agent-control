use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use anyhow::Context;
use ba_server::ControlDescriptor;
use serde_json::Value;

use crate::app::DataPaths;

pub async fn post(paths: &DataPaths, route: &str, body: Value) -> anyhow::Result<Value> {
    post_with_timeout(paths, route, body, Duration::from_secs(30)).await
}

pub async fn post_with_timeout(
    paths: &DataPaths,
    route: &str,
    body: Value,
    read_timeout: Duration,
) -> anyhow::Result<Value> {
    let descriptor_path = paths.control.clone();
    let route = route.to_owned();
    tokio::task::spawn_blocking(move || {
        let source = std::fs::read_to_string(&descriptor_path).with_context(|| {
            format!(
                "BA Agent control service is not running; start `ba-agent-control ui` (missing {})",
                descriptor_path.display()
            )
        })?;
        let descriptor: ControlDescriptor = serde_json::from_str(&source)
            .context("local control descriptor is invalid; restart `ba-agent-control ui`")?;
        if !descriptor.address.ip().is_loopback()
            || descriptor.origin != format!("http://{}", descriptor.address)
        {
            anyhow::bail!("refusing a non-loopback or mismatched control descriptor");
        }
        send_post(&descriptor, &route, &body, read_timeout)
    })
    .await?
}

pub async fn get(paths: &DataPaths, route: &str) -> anyhow::Result<Value> {
    let descriptor_path = paths.control.clone();
    let route = route.to_owned();
    tokio::task::spawn_blocking(move || {
        let source = std::fs::read_to_string(&descriptor_path).with_context(|| {
            format!(
                "BA Agent control service is not running; start `ba-agent-control ui` (missing {})",
                descriptor_path.display()
            )
        })?;
        let descriptor: ControlDescriptor = serde_json::from_str(&source)
            .context("local control descriptor is invalid; restart `ba-agent-control ui`")?;
        if !descriptor.address.ip().is_loopback()
            || descriptor.origin != format!("http://{}", descriptor.address)
        {
            anyhow::bail!("refusing a non-loopback or mismatched control descriptor");
        }
        send_get(&descriptor, &route)
    })
    .await?
}

fn send_get(descriptor: &ControlDescriptor, route: &str) -> anyhow::Result<Value> {
    if !route.starts_with('/') || route.contains(['\r', '\n']) {
        anyhow::bail!("invalid local API route");
    }
    let mut stream = TcpStream::connect_timeout(&descriptor.address, Duration::from_secs(2))
        .context("BA Agent control service is not reachable; restart `ba-agent-control ui`")?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let request = format!(
        "GET {route} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nCookie: ba_session={session}\r\nConnection: close\r\n\r\n",
        address = descriptor.address,
        origin = descriptor.origin,
        session = descriptor.session_token,
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    parse_response(&response)
}

fn send_post(
    descriptor: &ControlDescriptor,
    route: &str,
    body: &Value,
    read_timeout: Duration,
) -> anyhow::Result<Value> {
    if !route.starts_with('/') || route.contains(['\r', '\n']) {
        anyhow::bail!("invalid local API route");
    }
    let body = serde_json::to_vec(body)?;
    let mut stream = TcpStream::connect_timeout(&descriptor.address, Duration::from_secs(2))
        .context("BA Agent control service is not reachable; restart `ba-agent-control ui`")?;
    stream.set_read_timeout(Some(read_timeout))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let request = format!(
        "POST {route} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nCookie: ba_session={session}\r\nx-ba-csrf-token: {csrf}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n",
        address = descriptor.address,
        origin = descriptor.origin,
        session = descriptor.session_token,
        csrf = descriptor.csrf_token,
        length = body.len(),
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    parse_response(&response)
}

fn parse_response(response: &[u8]) -> anyhow::Result<Value> {
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("local API returned an invalid HTTP response")?;
    let headers = std::str::from_utf8(&response[..separator])?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .context("local API response has no status code")?;
    let encoded_body = &response[(separator + 4)..];
    let body = if headers
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
    {
        decode_chunked(encoded_body)?
    } else {
        encoded_body.to_vec()
    };
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).context("local API returned non-JSON content")?
    };
    if !(200..300).contains(&status) {
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("local API request failed");
        anyhow::bail!("local API {status}: {message}");
    }
    Ok(value)
}

fn decode_chunked(mut input: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let line_end = input
            .windows(2)
            .position(|window| window == b"\r\n")
            .context("invalid chunked response")?;
        let size_text = std::str::from_utf8(&input[..line_end])?
            .split(';')
            .next()
            .context("missing chunk size")?;
        let size = usize::from_str_radix(size_text.trim(), 16)?;
        input = &input[(line_end + 2)..];
        if size == 0 {
            break;
        }
        if input.len() < size + 2 || &input[size..(size + 2)] != b"\r\n" {
            anyhow::bail!("truncated chunked response");
        }
        decoded.extend_from_slice(&input[..size]);
        input = &input[(size + 2)..];
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_length_and_chunked_json_responses() -> anyhow::Result<()> {
        assert_eq!(
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}")?,
            serde_json::json!({"ok": true})
        );
        assert_eq!(
            parse_response(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n6\r\n{\"ok\":\r\n5\r\ntrue}\r\n0\r\n\r\n"
            )?,
            serde_json::json!({"ok": true})
        );
        Ok(())
    }

    #[tokio::test]
    async fn post_uses_loopback_descriptor_cookie_origin_and_csrf() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let paths = DataPaths::resolve(Some(directory.path().to_path_buf()))?;
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        let descriptor = ControlDescriptor {
            address,
            origin: format!("http://{address}"),
            session_token: "session-fixture".into(),
            csrf_token: "csrf-fixture".into(),
            process_id: std::process::id(),
            started_at: "2026-08-24T00:00:00Z".into(),
        };
        std::fs::write(&paths.control, serde_json::to_vec(&descriptor)?)?;
        let server = std::thread::spawn(move || -> anyhow::Result<()> {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut request = Vec::new();
            let mut buffer = [0_u8; 2048];
            loop {
                let count = stream.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
                let Some(separator) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = std::str::from_utf8(&request[..separator])?;
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if request.len() >= separator + 4 + length {
                    break;
                }
            }
            let request = String::from_utf8(request)?;
            assert!(request.starts_with("POST /api/v1/dispatches HTTP/1.1\r\n"));
            assert!(request.contains(&format!("Origin: http://{address}\r\n")));
            assert!(request.contains("Cookie: ba_session=session-fixture\r\n"));
            assert!(request.contains("x-ba-csrf-token: csrf-fixture\r\n"));
            let body = br#"{"accepted":true}"#;
            write!(
                stream,
                "HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            stream.write_all(body)?;
            Ok(())
        });

        let response = post(
            &paths,
            "/api/v1/dispatches",
            serde_json::json!({"taskId": "01FIXTURE"}),
        )
        .await?;
        assert_eq!(response, serde_json::json!({"accepted": true}));
        server
            .join()
            .map_err(|_| anyhow::anyhow!("mock server panicked"))??;
        Ok(())
    }
}
