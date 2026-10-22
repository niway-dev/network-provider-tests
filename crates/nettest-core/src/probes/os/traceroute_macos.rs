//! Native unprivileged traceroute on macOS.
//!
//! A `SOCK_DGRAM` / `IPPROTO_ICMP` socket needs no root. We set `IP_TTL`, send
//! one echo request per hop, and the kernel delivers either the echo reply or
//! the ICMP Time Exceeded with the outer IP header (whose source is the hop).
//! All hops are probed concurrently (one socket each, started 60 ms apart) and
//! then truncated at the first echo reply.

use std::mem::MaybeUninit;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::probes::{Hop, Trace};

const WAIT: Duration = Duration::from_secs(1);
const STAGGER: Duration = Duration::from_millis(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    /// ICMP Time Exceeded or Destination Unreachable from an intermediate router.
    Router(Ipv4Addr),
    /// Echo reply: the target answered.
    Target(Ipv4Addr),
}

fn checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for c in data.chunks(2) {
        let w = if c.len() == 2 {
            u16::from_be_bytes([c[0], c[1]])
        } else {
            u16::from_be_bytes([c[0], 0])
        };
        sum += u32::from(w);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn echo_request(id: u16, seq: u16) -> Vec<u8> {
    let mut p = vec![8, 0, 0, 0];
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&seq.to_be_bytes());
    p.extend_from_slice(&[0x61; 32]);
    let c = checksum(&p);
    p[2..4].copy_from_slice(&c.to_be_bytes());
    p
}

/// Parse what a macOS ICMP datagram socket returns: an IPv4 header followed by
/// the ICMP message. Returns `None` for anything that is not an answer to our
/// echo request number `seq`.
pub fn parse_reply(buf: &[u8], seq: u16) -> Option<Reply> {
    if buf.len() < 20 || buf[0] >> 4 != 4 {
        return None;
    }
    let ihl = usize::from(buf[0] & 0x0f) * 4;
    let src = Ipv4Addr::new(buf[12], buf[13], buf[14], buf[15]);
    let icmp = buf.get(ihl..)?;
    match *icmp.first()? {
        0 => {
            // Echo reply.
            let got = u16::from_be_bytes([*icmp.get(6)?, *icmp.get(7)?]);
            (got == seq).then_some(Reply::Target(src))
        }
        3 | 11 => {
            // Error: 8 bytes of ICMP header, then our original IP header + echo request.
            let inner = icmp.get(8..)?;
            if inner.first()? >> 4 != 4 {
                return None;
            }
            let inner_ihl = usize::from(inner[0] & 0x0f) * 4;
            let echo = inner.get(inner_ihl..)?;
            if *echo.first()? != 8 {
                return None;
            }
            let got = u16::from_be_bytes([*echo.get(6)?, *echo.get(7)?]);
            (got == seq).then_some(Reply::Router(src))
        }
        _ => None,
    }
}

/// Probe one TTL, twice if the first probe gets no answer (routers rate-limit
/// ICMP, and all hops are probed at once). Blocking; call from `spawn_blocking`.
fn probe_hop(target: Ipv4Addr, ttl: u8) -> std::io::Result<Option<(Reply, f64)>> {
    for _ in 0..2 {
        if let Some(r) = probe_once(target, ttl)? {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

fn probe_once(target: Ipv4Addr, ttl: u8) -> std::io::Result<Option<(Reply, f64)>> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4))?;
    sock.set_ttl_v4(u32::from(ttl))?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let seq = u16::from(ttl);
    let dest = SockAddr::from(SocketAddrV4::new(target, 0));
    let started = Instant::now();
    sock.send_to(&echo_request(0x6e74, seq), &dest)?;
    let mut buf = [MaybeUninit::<u8>::uninit(); 1500];
    while started.elapsed() < WAIT {
        match sock.recv(&mut buf) {
            Ok(n) => {
                // SAFETY: `recv` initialized the first `n` bytes.
                let data: Vec<u8> = buf[..n]
                    .iter()
                    .map(|b| unsafe { b.assume_init() })
                    .collect();
                if let Some(r) = parse_reply(&data, seq) {
                    return Ok(Some((r, started.elapsed().as_secs_f64() * 1000.0)));
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

pub async fn traceroute(target: Ipv4Addr, max_hops: u8) -> std::io::Result<Trace> {
    let mut tasks = Vec::new();
    for ttl in 1..=max_hops {
        tasks.push(tokio::task::spawn_blocking(move || {
            // Stagger the probes so they do not queue behind each other on a slow
            // uplink; otherwise every round trip is inflated by the burst.
            std::thread::sleep(STAGGER * u32::from(ttl - 1));
            probe_hop(target, ttl)
        }));
    }
    let mut hops = Vec::new();
    for (i, t) in tasks.into_iter().enumerate() {
        let ttl = (i + 1) as u8;
        let res = t
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))??;
        match res {
            Some((Reply::Target(ip), ms)) | Some((Reply::Router(ip), ms)) => {
                hops.push(Hop {
                    hop: ttl,
                    ip: Some(ip.to_string()),
                    ms: Some(ms),
                });
                if matches!(res, Some((Reply::Target(_), _))) {
                    break;
                }
            }
            None => hops.push(Hop {
                hop: ttl,
                ip: None,
                ms: None,
            }),
        }
    }
    Ok(Trace { hops, error: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip_header(src: [u8; 4], proto: u8, total: u16) -> Vec<u8> {
        let mut h = vec![0x45, 0, 0, 0, 0, 0, 0, 0, 64, proto, 0, 0];
        h[2..4].copy_from_slice(&total.to_be_bytes());
        h.extend_from_slice(&src);
        h.extend_from_slice(&[192, 0, 2, 99]);
        h
    }

    fn time_exceeded(router: [u8; 4], seq: u16) -> Vec<u8> {
        let mut p = ip_header(router, 1, 56);
        p.extend_from_slice(&[11, 0, 0, 0, 0, 0, 0, 0]);
        p.extend_from_slice(&ip_header([192, 0, 2, 99], 1, 60));
        p.extend_from_slice(&echo_request(1, seq)[..8]);
        p
    }

    #[test]
    fn echo_request_checksum_is_valid() {
        let p = echo_request(0x1234, 7);
        assert_eq!(checksum(&p), 0, "a correct ICMP checksum sums to zero");
        assert_eq!(p[0], 8);
        assert_eq!(&p[6..8], &[0, 7]);
    }

    #[test]
    fn time_exceeded_names_the_router() {
        let p = time_exceeded([10, 20, 0, 1], 3);
        assert_eq!(
            parse_reply(&p, 3),
            Some(Reply::Router(Ipv4Addr::new(10, 20, 0, 1)))
        );
        assert_eq!(parse_reply(&p, 4), None, "another hop's answer is ignored");
    }

    #[test]
    fn echo_reply_means_the_target_answered() {
        let mut p = ip_header([1, 1, 1, 1], 1, 60);
        let mut icmp = echo_request(1, 9);
        icmp[0] = 0;
        p.extend_from_slice(&icmp);
        assert_eq!(
            parse_reply(&p, 9),
            Some(Reply::Target(Ipv4Addr::new(1, 1, 1, 1)))
        );
    }

    #[test]
    fn garbage_is_ignored() {
        assert_eq!(parse_reply(&[], 1), None);
        assert_eq!(parse_reply(&[0x45; 10], 1), None);
        let mut p = ip_header([1, 1, 1, 1], 1, 28);
        p.extend_from_slice(&[5, 0, 0, 0, 0, 0, 0, 0]); // redirect: not ours
        assert_eq!(parse_reply(&p, 1), None);
    }
}
