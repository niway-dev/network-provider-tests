//! Check 04: ICMPv6 ping and HTTPS over IPv6 (skipped without a global IPv6 address).

use super::reach::{probe, ReachSpec};
use super::CheckRun;
use crate::model::Out;
use crate::probes::{Family, Probes};
use crate::record::{HttpsBlock, IcmpBlock, Ipv6Reachability, Metrics};

const PING_TARGETS: [&str; 2] = ["2001:4860:4860::8888", "2606:4700:4700::1111"];
const HTTPS_TARGETS: [&str; 2] = ["https://www.google.com", "https://www.cloudflare.com"];

pub async fn run(probes: &dyn Probes) -> CheckRun {
    let mut out = Out::new();

    if !probes.has_global_ipv6() {
        out.info("This Mac has no global IPv6 address, so IPv6 is not in use on this network");
        out.skip("IPv6 ping and HTTPS probes");
        let m = Ipv6Reachability {
            status: out.status(),
            global_address: Some(false),
            icmp: IcmpBlock::default(),
            https: HttpsBlock::default(),
        };
        return CheckRun {
            lines: out.into_lines(),
            metrics: Metrics::Ipv6Reachability(m),
        };
    }
    out.info("Global IPv6 address present");

    let r = probe(
        probes,
        &ReachSpec {
            ping_label: "ping6",
            https_label: "HTTPS over IPv6",
            ping_targets: &PING_TARGETS,
            urls: &HTTPS_TARGETS,
            family: Family::V6,
            icmp_note: None,
        },
        &mut out,
    )
    .await;

    let (ok, total) = (r.https.ok.unwrap_or(0), r.https.total.unwrap_or(0));
    if ok * 2 < total {
        out.fail(format!(
            "Most IPv6 HTTPS probes failed ({ok} of {total} worked) although this Mac has an IPv6 address"
        ));
        out.hint("IPv6 traffic is not getting out. If IPv4 works, sites that prefer IPv6 may stall before falling back to IPv4.");
    }

    let m = Ipv6Reachability {
        status: out.status(),
        global_address: Some(true),
        icmp: r.icmp,
        https: r.https,
    };
    CheckRun {
        lines: out.into_lines(),
        metrics: Metrics::Ipv6Reachability(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::probes::fake::FakeProbes;

    fn texts(r: &CheckRun) -> Vec<String> {
        r.lines.iter().map(|l| l.text.clone()).collect()
    }
    fn m(r: &CheckRun) -> &Ipv6Reachability {
        match &r.metrics {
            Metrics::Ipv6Reachability(m) => m,
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn skip_without_global_ipv6() {
        let mut p = FakeProbes::new();
        p.global_ipv6 = false;
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Skip);
        assert_eq!(m(&r).global_address, Some(false));
        assert_eq!(m(&r).icmp.ok, None);
        assert_eq!(m(&r).https.total, None);
        assert!(m(&r).https.targets.is_empty());
        assert_eq!(
            texts(&r),
            [
                "This Mac has no global IPv6 address, so IPv6 is not in use on this network",
                "IPv6 ping and HTTPS probes"
            ]
        );
    }

    #[tokio::test]
    async fn pass_with_ipv6() {
        let r = run(&FakeProbes::new()).await;
        assert_eq!(m(&r).status, Status::Pass);
        assert_eq!(m(&r).global_address, Some(true));
        let t = texts(&r);
        assert_eq!(t[0], "Global IPv6 address present");
        assert_eq!(t[1], "ping6 2001:4860:4860::8888: 0% loss, avg 2.000 ms");
        assert_eq!(
            t[3],
            "HTTPS over IPv6 https://www.google.com: HTTP 200 in 0.300s"
        );
    }

    #[tokio::test]
    async fn fail_when_most_https_fails() {
        let mut p = FakeProbes::new();
        p.default_https = Err("connection refused or unreachable".into());
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Fail);
        assert!(texts(&r).contains(
            &"Most IPv6 HTTPS probes failed (0 of 2 worked) although this Mac has an IPv6 address"
                .to_string()
        ));
    }

    #[tokio::test]
    async fn one_of_two_failing_only_warns() {
        let mut p = FakeProbes::new();
        p.https.insert(
            "https://www.cloudflare.com".into(),
            Err("timed out after 6s".into()),
        );
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Warn);
    }
}
