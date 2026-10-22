//! A minimal DNS client: one UDP A query to one server, with the rcode.
//!
//! Hand-rolled instead of `hickory-resolver`: the checks need the raw rcode
//! and exact per-server timeouts, and `dig +tries=1` semantics for the system
//! path. See the spec's deviations section.

use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::probes::{DnsOutcome, SystemDns};

/// Build an A query for `name` with recursion desired.
pub fn build_query(id: u16, name: &str) -> Vec<u8> {
    let mut q = Vec::with_capacity(32 + name.len());
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00]); // RD
    q.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // QD=1
    for label in name.trim_end_matches('.').split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&[0, 1, 0, 1]); // A, IN
    q
}

pub fn rcode_name(code: u8) -> String {
    match code {
        0 => "NOERROR".into(),
        1 => "FORMERR".into(),
        2 => "SERVFAIL".into(),
        3 => "NXDOMAIN".into(),
        4 => "NOTIMP".into(),
        5 => "REFUSED".into(),
        n => format!("RCODE{n}"),
    }
}

/// Rcode of a response to query `id`, or `None` if the packet is not that response.
pub fn parse_response(id: u16, buf: &[u8]) -> Option<u8> {
    if buf.len() < 12 || u16::from_be_bytes([buf[0], buf[1]]) != id || buf[2] & 0x80 == 0 {
        return None;
    }
    Some(buf[3] & 0x0f)
}

/// Parse `1.2.3.4`, `2001:db8::1` or `fe80::1%en0` (scope = interface name or index).
pub fn parse_server(server: &str) -> Option<SocketAddr> {
    if let Some((ip, scope)) = server.split_once('%') {
        let ip: Ipv6Addr = ip.parse().ok()?;
        let scope_id = scope.parse::<u32>().ok().or_else(|| {
            let c = std::ffi::CString::new(scope).ok()?;
            // SAFETY: `c` is a valid NUL-terminated string for the duration of the call.
            let idx = unsafe { libc::if_nametoindex(c.as_ptr()) };
            (idx != 0).then_some(idx)
        })?;
        return Some(SocketAddr::V6(SocketAddrV6::new(ip, 53, 0, scope_id)));
    }
    let ip: IpAddr = server.parse().ok()?;
    Some(SocketAddr::new(ip, 53))
}

fn next_id() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static N: AtomicU16 = AtomicU16::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (t as u16).wrapping_add(N.fetch_add(0x9e37, Ordering::Relaxed))
}

/// One query to one server; `Timeout` also covers a server we cannot reach at all.
pub async fn query(server: &str, name: &str, timeout: Duration) -> DnsOutcome {
    let Some(addr) = parse_server(server) else {
        return DnsOutcome::Timeout;
    };
    let bind: SocketAddr = if addr.is_ipv4() {
        "0.0.0.0:0".parse().unwrap()
    } else {
        "[::]:0".parse().unwrap()
    };
    let started = Instant::now();
    let attempt = async {
        let sock = UdpSocket::bind(bind).await.ok()?;
        sock.connect(addr).await.ok()?;
        let id = next_id();
        sock.send(&build_query(id, name)).await.ok()?;
        let mut buf = [0u8; 1500];
        loop {
            let n = sock.recv(&mut buf).await.ok()?;
            if let Some(rc) = parse_response(id, &buf[..n]) {
                return Some(rc);
            }
        }
    };
    match tokio::time::timeout(timeout, attempt).await {
        Ok(Some(0)) => DnsOutcome::Ok {
            ms: started.elapsed().as_millis() as u64,
        },
        Ok(Some(rc)) => DnsOutcome::Error {
            rcode: rcode_name(rc),
        },
        Ok(None) | Err(_) => DnsOutcome::Timeout,
    }
}

/// Nameservers from resolver configuration text (`/etc/resolv.conf`).
pub fn parse_resolv_conf(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            (it.next() == Some("nameserver")).then(|| it.next().map(str::to_string))?
        })
        .collect()
}

/// Like `dig +tries=1 +time=<timeout> name`: ask each configured server once, in order.
pub async fn system(name: &str, timeout: Duration) -> SystemDns {
    let started = Instant::now();
    let servers = std::fs::read_to_string("/etc/resolv.conf")
        .map(|t| parse_resolv_conf(&t))
        .unwrap_or_default();
    for s in servers.iter().take(3) {
        match query(s, name, timeout).await {
            DnsOutcome::Ok { .. } => {
                return SystemDns {
                    ms: started.elapsed().as_millis() as u64,
                    rcode: "NOERROR".into(),
                }
            }
            DnsOutcome::Error { rcode } => {
                return SystemDns {
                    ms: started.elapsed().as_millis() as u64,
                    rcode,
                }
            }
            DnsOutcome::Timeout => continue,
        }
    }
    SystemDns {
        ms: started.elapsed().as_millis() as u64,
        rcode: "NONE".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_packet_layout() {
        let q = build_query(0xabcd, "google.com");
        assert_eq!(&q[..4], &[0xab, 0xcd, 0x01, 0x00]);
        assert_eq!(&q[4..6], &[0, 1]); // one question
        assert_eq!(&q[12..19], b"\x06google");
        assert_eq!(&q[19..23], b"\x03com");
        assert_eq!(&q[q.len() - 5..], &[0, 0, 1, 0, 1]);
    }

    #[test]
    fn response_parsing() {
        let mut resp = vec![0xab, 0xcd, 0x81, 0x83];
        resp.extend_from_slice(&[0; 8]);
        assert_eq!(parse_response(0xabcd, &resp), Some(3));
        assert_eq!(rcode_name(3), "NXDOMAIN");
        assert_eq!(parse_response(0x1111, &resp), None, "wrong id");
        resp[2] = 0x01; // not a response
        assert_eq!(parse_response(0xabcd, &resp), None);
        assert_eq!(parse_response(0xabcd, &[1, 2, 3]), None);
    }

    #[test]
    fn server_parsing() {
        assert_eq!(parse_server("1.1.1.1"), Some("1.1.1.1:53".parse().unwrap()));
        assert_eq!(
            parse_server("2001:db8::53"),
            Some("[2001:db8::53]:53".parse().unwrap())
        );
        match parse_server("fe80::1%12") {
            Some(SocketAddr::V6(a)) => assert_eq!(a.scope_id(), 12),
            other => panic!("{other:?}"),
        }
        assert_eq!(parse_server("garbage"), None);
    }

    #[test]
    fn resolv_conf_nameservers() {
        let text =
            "# comment\nsearch example.test\nnameserver 198.51.100.1\nnameserver 2001:db8::1\n";
        assert_eq!(parse_resolv_conf(text), ["198.51.100.1", "2001:db8::1"]);
    }
}
