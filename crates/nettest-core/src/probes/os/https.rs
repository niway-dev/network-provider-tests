//! HTTPS GET with per-phase timing: our own TCP connect + rustls handshake, so
//! the address family can be forced and each phase timed. No redirects.

use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::probes::{Family, HttpResult, HttpSample};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Stop draining the body after this much; the timing is long over by then.
const MAX_BODY: usize = 4 * 1024 * 1024;

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CFG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("ring supports the default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    })
    .clone()
}

/// Split `https://host[:port][/path]` into `(host, port, path)`.
pub fn split_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("https://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') || h.ends_with(']') => {
            (h.trim_matches(['[', ']']), p.parse().ok()?)
        }
        _ => (authority, 443),
    };
    (!host.is_empty()).then(|| (host.to_string(), port, path))
}

/// Status code from the first line of a response (`HTTP/1.1 200 OK`).
pub fn parse_status_line(head: &[u8]) -> Option<u16> {
    let line = head.split(|b| *b == b'\n').next()?;
    let line = std::str::from_utf8(line).ok()?;
    let mut it = line.split_whitespace();
    if !it.next()?.starts_with("HTTP/") {
        return None;
    }
    it.next()?.parse().ok()
}

fn describe(e: &io::Error) -> String {
    use io::ErrorKind::*;
    match e.kind() {
        ConnectionRefused | HostUnreachable | NetworkUnreachable | AddrNotAvailable => {
            "connection refused or unreachable".into()
        }
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof => {
            "connection dropped".into()
        }
        TimedOut => "connection timed out".into(),
        _ => format!("network error: {e}"),
    }
}

pub async fn get(url: &str, family: Family, timeout: Duration) -> HttpResult {
    match tokio::time::timeout(timeout, get_inner(url, family)).await {
        Ok(r) => r,
        Err(_) => Err(format!("timed out after {}s", timeout.as_secs())),
    }
}

async fn get_inner(url: &str, family: Family) -> HttpResult {
    let (host, port, path) = split_url(url).ok_or_else(|| format!("bad URL {url}"))?;
    let t0 = Instant::now();

    // --- DNS ---
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|_| "could not resolve host".to_string())?
        .filter(|a| match family {
            Family::V4 => a.is_ipv4(),
            Family::V6 => a.is_ipv6(),
            Family::Any => true,
        })
        .collect();
    if addrs.is_empty() {
        return Err("could not resolve host".into());
    }
    let dns_ms = t0.elapsed().as_millis() as u64;

    // --- TCP connect: first address that answers ---
    let t_connect = Instant::now();
    let mut last_err = "connection refused or unreachable".to_string();
    let mut connected = None;
    let count = addrs.len();
    for (i, addr) in addrs.into_iter().enumerate() {
        // Give up on a silent address after a few seconds so the next one gets a
        // chance; the last one may use the whole budget, like curl, so a dead
        // path reports "timed out after Ns".
        let attempt = if i + 1 < count {
            tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await
        } else {
            Ok(TcpStream::connect(addr).await)
        };
        match attempt {
            Ok(Ok(s)) => {
                connected = Some((s, addr));
                break;
            }
            Ok(Err(e)) => last_err = describe(&e),
            Err(_) => last_err = "connection timed out".into(),
        }
    }
    let (tcp, addr) = connected.ok_or(last_err)?;
    let _ = tcp.set_nodelay(true);
    let connect_ms = t_connect.elapsed().as_millis() as u64;

    // --- TLS handshake ---
    let t_tls = Instant::now();
    let name =
        ServerName::try_from(host.clone()).map_err(|_| "TLS handshake failed".to_string())?;
    let mut tls = TlsConnector::from(tls_config())
        .connect(name, tcp)
        .await
        .map_err(|_| "TLS handshake failed".to_string())?;
    let tls_ms = t_tls.elapsed().as_millis() as u64;

    // --- request, first byte, body ---
    let t_req = Instant::now();
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: nettest\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    tls.write_all(req.as_bytes())
        .await
        .map_err(|e| describe(&e))?;
    let mut buf = vec![0u8; 16 * 1024];
    let n = tls.read(&mut buf).await.map_err(|e| describe(&e))?;
    if n == 0 {
        return Err("connection dropped".into());
    }
    let ttfb_ms = t_req.elapsed().as_millis() as u64;
    let code = parse_status_line(&buf[..n]).ok_or("not an HTTP response")?;
    // Drain the body like `curl -o /dev/null` does, so the total is comparable.
    let mut got = n;
    while got < MAX_BODY {
        match tls.read(&mut buf).await {
            Ok(0) | Err(_) => break, // a missing close_notify surfaces as an error: still the end
            Ok(m) => got += m,
        }
    }
    let total_ms = t0.elapsed().as_millis() as u64;

    Ok(HttpSample {
        code,
        family: if addr.is_ipv6() {
            Family::V6
        } else {
            Family::V4
        },
        dns_ms,
        connect_ms,
        tls_ms,
        ttfb_ms,
        total_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_splitting() {
        assert_eq!(
            split_url("https://www.google.com"),
            Some(("www.google.com".into(), 443, "/".into()))
        );
        assert_eq!(
            split_url("https://1.1.1.1/dns?x=1"),
            Some(("1.1.1.1".into(), 443, "/dns?x=1".into()))
        );
        assert_eq!(
            split_url("https://example.test:8443/a"),
            Some(("example.test".into(), 8443, "/a".into()))
        );
        assert_eq!(split_url("http://example.test"), None);
    }

    #[test]
    fn status_line() {
        assert_eq!(
            parse_status_line(b"HTTP/1.1 301 Moved\r\nLocation: x\r\n"),
            Some(301)
        );
        assert_eq!(parse_status_line(b"HTTP/2 200\r\n"), Some(200));
        assert_eq!(parse_status_line(b"garbage"), None);
        assert_eq!(parse_status_line(b"HTTP/1.1 abc"), None);
    }
}
