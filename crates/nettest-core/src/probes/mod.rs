//! Everything that touches the network or the OS sits behind the [`Probes`]
//! trait, so the checks and the diagnosis can be tested without a network.

use std::time::Duration;

use async_trait::async_trait;

pub mod fake;
pub mod os;

/// Address family to force for an HTTPS probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    V4,
    V6,
    /// Whatever the resolver returns first.
    Any,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PingStats {
    pub sent: u32,
    pub received: u32,
    pub loss_pct: f64,
    /// Average round trip in ms; `None` when nothing answered.
    pub avg_ms: Option<f64>,
}

impl PingStats {
    pub fn from_rtts(sent: u32, rtts_ms: &[f64]) -> PingStats {
        let received = rtts_ms.len() as u32;
        let loss_pct = if sent == 0 {
            100.0
        } else {
            f64::from(sent - received) * 100.0 / f64::from(sent)
        };
        let avg_ms = if rtts_ms.is_empty() {
            None
        } else {
            Some(rtts_ms.iter().sum::<f64>() / rtts_ms.len() as f64)
        };
        PingStats {
            sent,
            received,
            loss_pct,
            avg_ms,
        }
    }
}

/// A completed HTTPS request with per-phase timings in milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpSample {
    pub code: u16,
    /// Family actually used.
    pub family: Family,
    pub dns_ms: u64,
    pub connect_ms: u64,
    pub tls_ms: u64,
    pub ttfb_ms: u64,
    pub total_ms: u64,
}

/// `Err` carries a short human reason (`timed out after 6s`, `TLS handshake failed`, ...).
pub type HttpResult = Result<HttpSample, String>;

#[derive(Clone, Debug, PartialEq)]
pub enum DnsOutcome {
    Ok {
        ms: u64,
    },
    Timeout,
    /// The server answered with a non-zero rcode (`SERVFAIL`, `REFUSED`, ...).
    Error {
        rcode: String,
    },
}

/// A lookup through the system's resolver. `rcode` is `NOERROR`, another DNS
/// status, or `NONE` when nothing answered.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemDns {
    pub ms: u64,
    pub rcode: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hop {
    pub hop: u8,
    /// `None` when the hop did not answer.
    pub ip: Option<String>,
    pub ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Trace {
    pub hops: Vec<Hop>,
    /// Why there are no hops, when the trace itself could not run.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefaultRoute {
    pub interface: String,
    /// Gateway as printed by the OS; may be absent or not an IPv4 address.
    pub gateway: Option<String>,
    /// Hardware port name (`Wi-Fi`, `Ethernet Adapter (en4)`), used to tell Wi-Fi from wired.
    pub hardware_port: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tunnel {
    pub name: String,
    pub addr: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolverSection {
    Main,
    Scoped,
}

/// One resolver from `scutil --dns`, in the order scutil prints them.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolverEntry {
    pub section: ResolverSection,
    pub index: u32,
    pub domain: Option<String>,
    pub nameservers: Vec<String>,
    pub interface: Option<String>,
}

#[async_trait]
pub trait Probes: Send + Sync {
    /// Send `count` echo requests about 400 ms apart, waiting up to `timeout` for each.
    async fn ping(&self, target: &str, count: u32, timeout: Duration) -> PingStats;
    /// HTTPS GET to `url` (not following redirects), timing each phase.
    async fn https(&self, url: &str, family: Family, timeout: Duration) -> HttpResult;
    /// One A query for `name` to a single server (`server` may carry a `%interface` scope).
    async fn dns_query(&self, server: &str, name: &str, timeout: Duration) -> DnsOutcome;
    /// A lookup through the system's configured resolver, `tries=1`, `timeout` per server.
    async fn dns_system(&self, name: &str, timeout: Duration) -> SystemDns;
    /// IPv4 traceroute with `max_hops` hops, one probe per hop.
    async fn traceroute(&self, target: &str, max_hops: u8) -> Trace;
    fn default_route(&self) -> Option<DefaultRoute>;
    fn tunnels_with_ipv4(&self) -> Vec<Tunnel>;
    fn resolvers(&self) -> Vec<ResolverEntry>;
    /// Interface the OS would use to reach `ip`.
    fn route_interface(&self, ip: &str) -> Option<String>;
    fn has_global_ipv6(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_stats_from_rtts() {
        let s = PingStats::from_rtts(5, &[10.0, 20.0, 30.0, 40.0]);
        assert_eq!(s.received, 4);
        assert!((s.loss_pct - 20.0).abs() < 1e-9);
        assert_eq!(s.avg_ms, Some(25.0));
        let none = PingStats::from_rtts(5, &[]);
        assert_eq!(none.loss_pct, 100.0);
        assert_eq!(none.avg_ms, None);
    }
}
