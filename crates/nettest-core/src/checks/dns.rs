//! Check 06: DNS timing through the system resolver, then each server directly.

use std::time::Duration;

use futures_util::future::join_all;

use super::CheckRun;
use crate::model::Out;
use crate::probes::{DnsOutcome, Probes, ResolverSection};
use crate::record::{Dns, DnsLookup, DnsServer, Metrics};

const SYSTEM_NAMES: [&str; 4] = [
    "google.com",
    "github.com",
    "cloudflare.com",
    "wikipedia.org",
];
const PUBLIC_SERVERS: [&str; 3] = ["1.1.1.1", "8.8.8.8", "9.9.9.9"];

struct Planned {
    group: &'static str,
    server: String,
    label: String,
    via_vpn: bool,
}

/// The list of servers to query, in order, each address once: the primary
/// resolver (#1), the first resolver bound to a physical interface, then the
/// public resolvers.
fn plan_servers(probes: &dyn Probes) -> Vec<Planned> {
    let mut list: Vec<Planned> = Vec::new();
    let mut add = |group: &'static str, server: &str, label: &str, via_vpn: bool| {
        if list.iter().any(|p| p.server == server) {
            return;
        }
        list.push(Planned {
            group,
            server: server.to_string(),
            label: label.to_string(),
            via_vpn,
        });
    };

    let mut first = true;
    let mut phys_done = false;
    for r in probes.resolvers() {
        if r.section == ResolverSection::Main && first {
            first = false;
            let primary_if = probes
                .route_interface(r.nameservers.first().map(String::as_str).unwrap_or(""))
                .unwrap_or_default();
            let mut label = format!("resolver #{}, primary", r.index);
            let vpn = primary_if.starts_with("utun");
            if vpn {
                label.push_str(&format!(", via VPN tunnel {primary_if}"));
            } else if !primary_if.is_empty() {
                label.push_str(&format!(", via {primary_if}"));
            }
            for s in &r.nameservers {
                add("primary", s, &label, vpn);
            }
            continue;
        }
        // First resolver bound to a non-tunnel interface = the physical network's DNS.
        if !phys_done {
            if let Some(ifn) = r.interface.as_deref().filter(|i| !i.is_empty()) {
                if !ifn.starts_with("utun") {
                    phys_done = true;
                    for s in &r.nameservers {
                        add("physical", s, &format!("physical interface {ifn}"), false);
                    }
                }
            }
        }
    }
    for s in PUBLIC_SERVERS {
        add("public", s, "public", false);
    }
    list
}

pub async fn run(probes: &dyn Probes) -> CheckRun {
    let mut out = Out::new();
    let mut m = Dns::default();

    // (a) System resolver timing.
    out.info("(a) Lookup time through the system's primary resolver");
    let lookups = join_all(
        SYSTEM_NAMES
            .iter()
            .map(|n| probes.dns_system(n, Duration::from_secs(3))),
    )
    .await;
    let mut sys_max = 0u64;
    let mut sys_status = "PASS";
    for (n, l) in SYSTEM_NAMES.iter().zip(&lookups) {
        m.system.lookups.push(DnsLookup {
            name: n.to_string(),
            ms: Some(l.ms),
            rcode: Some(l.rcode.clone()),
        });
        if l.rcode != "NOERROR" {
            out.fail(format!("{n}: no answer ({} ms, {})", l.ms, l.rcode));
            sys_status = "FAIL";
        } else if l.ms > 3000 {
            out.fail(format!("{n}: {} ms", l.ms));
            sys_status = "FAIL";
        } else if l.ms > 1000 {
            out.warn(format!("{n}: {} ms", l.ms));
            if sys_status == "PASS" {
                sys_status = "WARN";
            }
        } else {
            out.pass(format!("{n}: {} ms", l.ms));
        }
        sys_max = sys_max.max(l.ms);
    }
    m.system.status = Some(sys_status.to_string());
    m.system.max_ms = Some(sys_max);
    if sys_status != "PASS" {
        out.hint("The primary resolver is timing out and the OS is falling back to the next server. Every lookup pays that wait. Part (b) shows which server is the culprit.");
    }

    // (b) Each server directly.
    out.info("(b) Each DNS server queried directly (google.com, 2 s limit)");
    let planned = plan_servers(probes);
    let have_v6 = probes.has_global_ipv6();
    let results = join_all(planned.iter().map(|p| async {
        // IPv6 servers need IPv6 connectivity, except link-local ones (fe80::...%if).
        let is_v6 = p.server.contains(':');
        let link_local = p.server.to_lowercase().starts_with("fe80:");
        if is_v6 && !link_local && !have_v6 {
            return None;
        }
        Some(
            probes
                .dns_query(&p.server, "google.com", Duration::from_secs(2))
                .await,
        )
    }))
    .await;

    let mut primary_bad: Vec<String> = Vec::new();
    let (mut any_ok, mut any_tried) = (0u32, 0u32);
    for (p, res) in planned.iter().zip(results) {
        let (srv, label) = (&p.server, &p.label);
        let mut entry = DnsServer {
            group: p.group.to_string(),
            ip: p.server.clone(),
            result: String::new(),
            ms: None,
            rcode: None,
            via_vpn: Some(p.via_vpn),
            label: Some(p.label.clone()),
        };
        match res {
            None => {
                out.skip(format!(
                    "{srv} ({label}): IPv6 server but this Mac has no global IPv6 address"
                ));
                entry.result = "skipped".into();
            }
            Some(DnsOutcome::Ok { ms }) => {
                any_tried += 1;
                any_ok += 1;
                if ms > 500 {
                    out.warn(format!("{srv} ({label}): {ms} ms (slow)"));
                } else {
                    out.pass(format!("{srv} ({label}): {ms} ms"));
                }
                entry.result = "ok".into();
                entry.ms = Some(ms);
            }
            Some(DnsOutcome::Error { rcode }) => {
                any_tried += 1;
                any_ok += 1;
                out.warn(format!(
                    "{srv} ({label}): answered with error status {rcode}"
                ));
                entry.result = "error".into();
                entry.rcode = Some(rcode);
            }
            Some(DnsOutcome::Timeout) => {
                any_tried += 1;
                out.warn(format!("{srv} ({label}): TIMEOUT"));
                if p.group == "primary" {
                    primary_bad.push(srv.clone());
                }
                entry.result = "timeout".into();
            }
        }
        m.servers.push(entry);
    }
    m.primary_timeouts = primary_bad.clone();
    if !primary_bad.is_empty() {
        out.hint(format!(
            "These servers belong to your primary resolver and did not answer: {}. The system asks them first, so each lookup waits for the timeout before trying another server.",
            primary_bad.join(" ")
        ));
    }
    if any_tried > 0 && any_ok == 0 {
        out.fail("No DNS server answered: names cannot be resolved at all");
        out.hint(
            "Either the whole connection is down (see checks 01, 03, 04) or something blocks DNS.",
        );
    }

    m.status = out.status();
    CheckRun {
        lines: out.into_lines(),
        metrics: Metrics::Dns(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::probes::fake::FakeProbes;
    use crate::probes::{ResolverEntry, SystemDns};

    fn texts(r: &CheckRun) -> Vec<String> {
        r.lines.iter().map(|l| l.text.clone()).collect()
    }
    fn m(r: &CheckRun) -> &Dns {
        match &r.metrics {
            Metrics::Dns(m) => m,
            _ => panic!(),
        }
    }
    fn entry(
        section: ResolverSection,
        index: u32,
        ns: &[&str],
        ifn: Option<&str>,
    ) -> ResolverEntry {
        ResolverEntry {
            section,
            index,
            domain: None,
            nameservers: ns.iter().map(|s| s.to_string()).collect(),
            interface: ifn.map(str::to_string),
        }
    }

    /// A VPN primary resolver (#1), a physical resolver on en0, and the public ones.
    fn vpn_probes() -> FakeProbes {
        let mut p = FakeProbes::new();
        p.resolvers = vec![
            entry(
                ResolverSection::Main,
                1,
                &["198.51.100.1", "198.51.100.2"],
                None,
            ),
            entry(ResolverSection::Main, 2, &["192.0.2.1"], Some("en0")),
            entry(ResolverSection::Main, 3, &["198.51.100.1"], None),
            entry(ResolverSection::Scoped, 1, &["192.0.2.1"], Some("en0")),
        ];
        p.route_ifs.insert("198.51.100.1".into(), "utun4".into());
        p
    }

    #[tokio::test]
    async fn healthy_run_lists_primary_physical_and_public() {
        let r = run(&vpn_probes()).await;
        assert_eq!(m(&r).status, Status::Pass);
        let t = texts(&r);
        assert_eq!(
            t[0],
            "(a) Lookup time through the system's primary resolver"
        );
        assert_eq!(t[1], "google.com: 40 ms");
        assert_eq!(
            t[5],
            "(b) Each DNS server queried directly (google.com, 2 s limit)"
        );
        assert_eq!(
            t[6],
            "198.51.100.1 (resolver #1, primary, via VPN tunnel utun4): 10 ms"
        );
        assert_eq!(t[8], "192.0.2.1 (physical interface en0): 10 ms");
        assert_eq!(t[9], "1.1.1.1 (public): 10 ms");
        // 2 primary + 1 physical + 3 public; the scoped duplicate of 192.0.2.1 is dropped.
        assert_eq!(m(&r).servers.len(), 6);
        assert_eq!(m(&r).servers[0].via_vpn, Some(true));
        assert_eq!(m(&r).servers[2].group, "physical");
        assert_eq!(m(&r).system.max_ms, Some(40));
        assert!(m(&r).primary_timeouts.is_empty());
    }

    #[tokio::test]
    async fn non_vpn_primary_label_names_the_interface() {
        let mut p = FakeProbes::new();
        p.route_ifs.insert("192.0.2.1".into(), "en0".into());
        let r = run(&p).await;
        assert!(texts(&r).contains(&"192.0.2.1 (resolver #1, primary, via en0): 10 ms".to_string()));
        assert_eq!(m(&r).servers[0].via_vpn, Some(false));
    }

    #[tokio::test]
    async fn primary_timeout_is_recorded_and_hinted() {
        let mut p = vpn_probes();
        p.dns_queries
            .insert("198.51.100.2".into(), DnsOutcome::Timeout);
        let r = run(&p).await;
        let t = texts(&r);
        assert!(t.contains(
            &"198.51.100.2 (resolver #1, primary, via VPN tunnel utun4): TIMEOUT".to_string()
        ));
        assert!(t.contains(&"These servers belong to your primary resolver and did not answer: 198.51.100.2. The system asks them first, so each lookup waits for the timeout before trying another server.".to_string()));
        assert_eq!(m(&r).primary_timeouts, ["198.51.100.2"]);
        assert_eq!(m(&r).status, Status::Warn);
        assert_eq!(m(&r).servers[1].result, "timeout");
        assert_eq!(m(&r).servers[1].ms, None);
    }

    #[tokio::test]
    async fn system_lookup_thresholds() {
        let mut p = FakeProbes::new();
        p.dns_system.insert(
            "google.com".into(),
            SystemDns {
                ms: 1001,
                rcode: "NOERROR".into(),
            },
        );
        let r = run(&p).await;
        assert_eq!(m(&r).system.status.as_deref(), Some("WARN"));
        assert!(texts(&r).contains(&"google.com: 1001 ms".to_string()));
        assert!(texts(&r)
            .iter()
            .any(|t| t.starts_with("The primary resolver is timing out and the OS")));

        p.dns_system.insert(
            "github.com".into(),
            SystemDns {
                ms: 3001,
                rcode: "NOERROR".into(),
            },
        );
        let r = run(&p).await;
        assert_eq!(m(&r).system.status.as_deref(), Some("FAIL"));
        assert_eq!(m(&r).system.max_ms, Some(3001));
        assert_eq!(m(&r).status, Status::Fail);

        let mut p = FakeProbes::new();
        p.dns_system.insert(
            "wikipedia.org".into(),
            SystemDns {
                ms: 3002,
                rcode: "NONE".into(),
            },
        );
        let r = run(&p).await;
        assert!(texts(&r).contains(&"wikipedia.org: no answer (3002 ms, NONE)".to_string()));
        assert_eq!(m(&r).system.status.as_deref(), Some("FAIL"));
    }

    #[tokio::test]
    async fn slow_direct_answer_warns() {
        let mut p = FakeProbes::new();
        p.dns_queries
            .insert("1.1.1.1".into(), DnsOutcome::Ok { ms: 501 });
        let r = run(&p).await;
        assert!(texts(&r).contains(&"1.1.1.1 (public): 501 ms (slow)".to_string()));
    }

    #[tokio::test]
    async fn error_rcode_counts_as_an_answer() {
        let mut p = FakeProbes::new();
        p.default_dns_query = DnsOutcome::Error {
            rcode: "REFUSED".into(),
        };
        let r = run(&p).await;
        assert!(
            texts(&r).contains(&"1.1.1.1 (public): answered with error status REFUSED".to_string())
        );
        assert_eq!(m(&r).servers[0].result, "error");
        assert_eq!(m(&r).servers[0].rcode.as_deref(), Some("REFUSED"));
        assert!(!texts(&r)
            .contains(&"No DNS server answered: names cannot be resolved at all".to_string()));
    }

    #[tokio::test]
    async fn no_server_answering_is_a_fail() {
        let mut p = FakeProbes::new();
        p.default_dns_query = DnsOutcome::Timeout;
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Fail);
        assert!(texts(&r)
            .contains(&"No DNS server answered: names cannot be resolved at all".to_string()));
    }

    #[tokio::test]
    async fn ipv6_servers_are_skipped_without_global_ipv6_except_link_local() {
        let mut p = FakeProbes::new();
        p.global_ipv6 = false;
        p.resolvers = vec![entry(
            ResolverSection::Main,
            1,
            &["2001:db8::53", "fe80::1%en0"],
            None,
        )];
        let r = run(&p).await;
        let t = texts(&r);
        assert!(t.contains(&"2001:db8::53 (resolver #1, primary): IPv6 server but this Mac has no global IPv6 address".to_string()));
        assert!(t.contains(&"fe80::1%en0 (resolver #1, primary): 10 ms".to_string()));
        assert_eq!(m(&r).servers[0].result, "skipped");
        assert_eq!(m(&r).servers[0].ms, None);
    }
}
