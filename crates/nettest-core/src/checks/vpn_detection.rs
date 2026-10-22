//! Check 02: VPN tunnels and whether a VPN is the primary DNS resolver.

use super::{non_empty, CheckRun};
use crate::model::Out;
use crate::probes::{Probes, ResolverSection};
use crate::record::{Metrics, VpnDetection};

pub async fn run(probes: &dyn Probes) -> CheckRun {
    let mut out = Out::new();
    let mut m = VpnDetection::default();

    // --- tunnel interfaces that carry an IPv4 address ---
    let tunnels = probes.tunnels_with_ipv4();
    for t in &tunnels {
        out.info(format!("Tunnel {} has IPv4 address {}", t.name, t.addr));
    }
    m.tunnels = tunnels.iter().map(|t| t.name.clone()).collect();
    if tunnels.is_empty() {
        out.pass("No utun interface with an IPv4 address (no VPN-style tunnel is active)");
    }

    // --- configured resolvers ---
    let resolvers = probes.resolvers();
    out.info("Resolvers with a domain or search scope (from scutil --dns):");
    let mut shown = 0;
    for r in resolvers
        .iter()
        .filter(|r| r.section == ResolverSection::Main)
    {
        let Some(dom) = r.domain.as_deref().filter(|d| !d.is_empty()) else {
            continue;
        };
        let first = r.nameservers.first().map(String::as_str).unwrap_or("");
        let rif = probes.route_interface(first);
        out.dim(format!(
            "resolver #{}  domain {} -> {}  (first server routed via {})",
            r.index,
            dom,
            r.nameservers.join(" "),
            rif.as_deref().unwrap_or("unknown")
        ));
        shown += 1;
    }
    if shown == 0 {
        out.dim("none (a single global resolver is configured)");
    }

    // --- who is resolver #1? ---
    let primary = resolvers
        .iter()
        .find(|r| r.section == ResolverSection::Main && !r.nameservers.is_empty());
    let Some(primary) = primary else {
        out.skip("Could not read a primary resolver from scutil --dns");
        m.status = out.status();
        return CheckRun {
            lines: out.into_lines(),
            metrics: Metrics::VpnDetection(m),
        };
    };
    let primary_ns = primary.nameservers[0].as_str();
    let primary_if = probes.route_interface(primary_ns).unwrap_or_default();
    m.primary_servers = primary.nameservers.clone();
    m.primary_interface = non_empty(Some(&primary_if));
    if primary_if.starts_with("utun") {
        m.primary_via_vpn = Some(true);
        out.info(format!(
            "Primary resolver #1 starts with {primary_ns}, which is reached through VPN tunnel {primary_if}, not the physical network"
        ));
        out.hint("A VPN is acting as your primary DNS; if its DNS stops answering, every lookup waits for a timeout before falling back.");
    } else {
        m.primary_via_vpn = Some(false);
        let shown_if = if primary_if.is_empty() {
            "unknown"
        } else {
            primary_if.as_str()
        };
        out.pass(format!(
            "Primary resolver #1 starts with {primary_ns}, reached through {shown_if} (physical network)"
        ));
    }

    m.status = out.status();
    CheckRun {
        lines: out.into_lines(),
        metrics: Metrics::VpnDetection(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineKind, Status};
    use crate::probes::fake::FakeProbes;
    use crate::probes::{ResolverEntry, Tunnel};

    fn texts(r: &CheckRun) -> Vec<String> {
        r.lines.iter().map(|l| l.text.clone()).collect()
    }
    fn m(r: &CheckRun) -> &VpnDetection {
        match &r.metrics {
            Metrics::VpnDetection(m) => m,
            _ => panic!(),
        }
    }
    fn entry(
        section: ResolverSection,
        index: u32,
        domain: Option<&str>,
        ns: &[&str],
    ) -> ResolverEntry {
        ResolverEntry {
            section,
            index,
            domain: domain.map(str::to_string),
            nameservers: ns.iter().map(|s| s.to_string()).collect(),
            interface: None,
        }
    }

    #[tokio::test]
    async fn physical_primary_resolver_passes() {
        let mut p = FakeProbes::new();
        p.route_ifs.insert("192.0.2.1".into(), "en0".into());
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Pass);
        assert_eq!(m(&r).primary_via_vpn, Some(false));
        assert_eq!(m(&r).primary_interface.as_deref(), Some("en0"));
        let t = texts(&r);
        assert_eq!(
            t[0],
            "No utun interface with an IPv4 address (no VPN-style tunnel is active)"
        );
        assert_eq!(t[2], "none (a single global resolver is configured)");
        assert_eq!(
            t[3],
            "Primary resolver #1 starts with 192.0.2.1, reached through en0 (physical network)"
        );
    }

    #[tokio::test]
    async fn vpn_primary_resolver_is_info_with_hint() {
        let mut p = FakeProbes::new();
        p.tunnels = vec![Tunnel {
            name: "utun4".into(),
            addr: "198.51.100.77".into(),
        }];
        p.resolvers = vec![
            entry(
                ResolverSection::Main,
                1,
                None,
                &["198.51.100.1", "198.51.100.2"],
            ),
            entry(
                ResolverSection::Main,
                3,
                Some("corp.example"),
                &["198.51.100.1", "198.51.100.2"],
            ),
            entry(
                ResolverSection::Scoped,
                1,
                Some("ignored.example"),
                &["192.0.2.1"],
            ),
        ];
        p.route_ifs.insert("198.51.100.1".into(), "utun4".into());
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Info);
        assert_eq!(m(&r).tunnels, ["utun4"]);
        assert_eq!(m(&r).primary_via_vpn, Some(true));
        assert_eq!(m(&r).primary_servers, ["198.51.100.1", "198.51.100.2"]);
        let t = texts(&r);
        assert_eq!(t[0], "Tunnel utun4 has IPv4 address 198.51.100.77");
        assert_eq!(
            t[2],
            "resolver #3  domain corp.example -> 198.51.100.1 198.51.100.2  (first server routed via utun4)"
        );
        assert_eq!(
            t[3],
            "Primary resolver #1 starts with 198.51.100.1, which is reached through VPN tunnel utun4, not the physical network"
        );
        assert_eq!(r.lines[2].kind, LineKind::Dim);
        assert_eq!(r.lines[4].kind, LineKind::Hint);
    }

    #[tokio::test]
    async fn no_resolver_skips() {
        let mut p = FakeProbes::new();
        p.resolvers = vec![];
        let r = run(&p).await;
        assert_eq!(m(&r).primary_via_vpn, None);
        assert_eq!(m(&r).status, Status::Pass); // the "no tunnel" PASS line still counts
        assert!(
            texts(&r).contains(&"Could not read a primary resolver from scutil --dns".to_string())
        );
    }
}
