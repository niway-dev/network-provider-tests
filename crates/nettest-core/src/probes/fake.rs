//! A scripted [`Probes`] implementation for tests. It never touches the network.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;

use super::*;

/// Builder-style fake: set what each method answers, everything else has a
/// healthy default.
pub struct FakeProbes {
    pub route: Option<DefaultRoute>,
    pub tunnels: Vec<Tunnel>,
    pub resolvers: Vec<ResolverEntry>,
    pub route_ifs: HashMap<String, String>,
    pub global_ipv6: bool,
    pub pings: HashMap<String, PingStats>,
    pub default_ping: PingStats,
    pub https: HashMap<String, HttpResult>,
    pub default_https: HttpResult,
    pub dns_queries: HashMap<String, DnsOutcome>,
    pub default_dns_query: DnsOutcome,
    pub dns_system: HashMap<String, SystemDns>,
    pub default_dns_system: SystemDns,
    pub trace: Trace,
}

pub fn sample_ok(code: u16, total_ms: u64) -> HttpSample {
    HttpSample {
        code,
        family: Family::V4,
        dns_ms: 5,
        connect_ms: 20,
        tls_ms: 30,
        ttfb_ms: total_ms.saturating_sub(55),
        total_ms,
    }
}

impl Default for FakeProbes {
    fn default() -> Self {
        FakeProbes {
            route: Some(DefaultRoute {
                interface: "en0".into(),
                gateway: Some("192.0.2.1".into()),
                hardware_port: Some("Wi-Fi".into()),
            }),
            tunnels: vec![],
            resolvers: vec![ResolverEntry {
                section: ResolverSection::Main,
                index: 1,
                domain: None,
                nameservers: vec!["192.0.2.1".into()],
                interface: None,
            }],
            route_ifs: HashMap::new(),
            global_ipv6: true,
            pings: HashMap::new(),
            default_ping: PingStats::from_rtts(5, &[2.0; 5]),
            https: HashMap::new(),
            default_https: Ok(sample_ok(200, 300)),
            dns_queries: HashMap::new(),
            default_dns_query: DnsOutcome::Ok { ms: 10 },
            dns_system: HashMap::new(),
            default_dns_system: SystemDns {
                ms: 40,
                rcode: "NOERROR".into(),
            },
            trace: Trace {
                hops: vec![
                    Hop {
                        hop: 1,
                        ip: Some("192.0.2.1".into()),
                        ms: Some(1.5),
                    },
                    Hop {
                        hop: 2,
                        ip: Some("1.1.1.1".into()),
                        ms: Some(12.0),
                    },
                ],
                error: None,
            },
        }
    }
}

impl FakeProbes {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Probes for FakeProbes {
    async fn ping(&self, target: &str, _count: u32, _timeout: Duration) -> PingStats {
        self.pings
            .get(target)
            .cloned()
            .unwrap_or_else(|| self.default_ping.clone())
    }

    async fn https(&self, url: &str, _family: Family, _timeout: Duration) -> HttpResult {
        self.https
            .get(url)
            .cloned()
            .unwrap_or_else(|| self.default_https.clone())
    }

    async fn dns_query(&self, server: &str, _name: &str, _timeout: Duration) -> DnsOutcome {
        self.dns_queries
            .get(server)
            .cloned()
            .unwrap_or_else(|| self.default_dns_query.clone())
    }

    async fn dns_system(&self, name: &str, _timeout: Duration) -> SystemDns {
        self.dns_system
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.default_dns_system.clone())
    }

    async fn traceroute(&self, _target: &str, _max_hops: u8) -> Trace {
        self.trace.clone()
    }

    fn default_route(&self) -> Option<DefaultRoute> {
        self.route.clone()
    }

    fn tunnels_with_ipv4(&self) -> Vec<Tunnel> {
        self.tunnels.clone()
    }

    fn resolvers(&self) -> Vec<ResolverEntry> {
        self.resolvers.clone()
    }

    fn route_interface(&self, ip: &str) -> Option<String> {
        self.route_ifs.get(ip).cloned()
    }

    fn has_global_ipv6(&self) -> bool {
        self.global_ipv6
    }
}
