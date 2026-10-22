//! Real probes for the machine this runs on.

use std::net::Ipv4Addr;
use std::time::Duration;

use async_trait::async_trait;

use super::{
    DefaultRoute, DnsOutcome, Family, HttpResult, PingStats, Probes, ResolverEntry, SystemDns,
    Trace, Tunnel,
};

pub mod dns;
pub mod https;
pub mod ping;
pub mod scutil;
pub mod traceroute_cmd;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod traceroute_macos;

/// Introspection on platforms without an implementation yet (Windows is M2):
/// everything reports "unknown".
#[cfg(not(target_os = "macos"))]
mod macos {
    use crate::probes::{DefaultRoute, ResolverEntry, Tunnel};
    pub fn default_route() -> Option<DefaultRoute> {
        None
    }
    pub fn route_interface(_ip: &str) -> Option<String> {
        None
    }
    pub fn resolvers() -> Vec<ResolverEntry> {
        vec![]
    }
    pub fn tunnels_with_ipv4() -> Vec<Tunnel> {
        vec![]
    }
    pub fn has_global_ipv6() -> bool {
        false
    }
}

/// The probes used by the CLI and the app.
#[derive(Default, Clone, Copy)]
pub struct SystemProbes;

#[async_trait]
impl Probes for SystemProbes {
    async fn ping(&self, target: &str, count: u32, timeout: Duration) -> PingStats {
        ping::ping(target, count, timeout).await
    }

    async fn https(&self, url: &str, family: Family, timeout: Duration) -> HttpResult {
        https::get(url, family, timeout).await
    }

    async fn dns_query(&self, server: &str, name: &str, timeout: Duration) -> DnsOutcome {
        dns::query(server, name, timeout).await
    }

    async fn dns_system(&self, name: &str, timeout: Duration) -> SystemDns {
        dns::system(name, timeout).await
    }

    async fn traceroute(&self, target: &str, max_hops: u8) -> Trace {
        #[cfg(target_os = "macos")]
        if let Ok(ip) = target.parse::<Ipv4Addr>() {
            if let Ok(trace) = traceroute_macos::traceroute(ip, max_hops).await {
                return trace;
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _: Option<Ipv4Addr> = None;
        traceroute_cmd::traceroute(target, max_hops).await
    }

    fn default_route(&self) -> Option<DefaultRoute> {
        macos::default_route()
    }

    fn tunnels_with_ipv4(&self) -> Vec<Tunnel> {
        macos::tunnels_with_ipv4()
    }

    fn resolvers(&self) -> Vec<ResolverEntry> {
        macos::resolvers()
    }

    fn route_interface(&self, ip: &str) -> Option<String> {
        macos::route_interface(ip)
    }

    fn has_global_ipv6(&self) -> bool {
        macos::has_global_ipv6()
    }
}
