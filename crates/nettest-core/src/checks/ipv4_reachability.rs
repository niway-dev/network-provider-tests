//! Check 03: ICMP ping and HTTPS over IPv4.

use super::reach::{probe, ReachSpec};
use super::CheckRun;
use crate::model::Out;
use crate::probes::{Family, Probes};
use crate::record::{Ipv4Reachability, Metrics};

const PING_TARGETS: [&str; 3] = ["1.1.1.1", "8.8.8.8", "9.9.9.9"];
const HTTPS_TARGETS: [&str; 4] = [
    "https://www.google.com",
    "https://1.1.1.1",
    "https://github.com",
    "https://www.cloudflare.com",
];

pub async fn run(probes: &dyn Probes) -> CheckRun {
    let mut out = Out::new();
    let r = probe(
        probes,
        &ReachSpec {
            ping_label: "ping",
            https_label: "HTTPS over IPv4",
            ping_targets: &PING_TARGETS,
            urls: &HTTPS_TARGETS,
            family: Family::V4,
            icmp_note: Some("Note: ICMP loss alone, with HTTPS still working, is usually the ISP deprioritizing ping, not an outage."),
        },
        &mut out,
    )
    .await;

    let (https_ok, https_total) = (r.https.ok.unwrap_or(0), r.https.total.unwrap_or(0));
    let (icmp_ok, icmp_total) = (r.icmp.ok.unwrap_or(0), r.icmp.total.unwrap_or(0));
    if https_ok * 2 < https_total {
        out.fail(format!(
            "Most IPv4 HTTPS probes failed ({https_ok} of {https_total} worked)"
        ));
        out.hint("IPv4 traffic is not getting out. If IPv6 works (next check), this is typically an ISP-side IPv4 problem.");
    } else if https_ok < https_total {
        out.info(format!(
            "Some IPv4 HTTPS probes failed ({https_ok} of {https_total} worked); the connection is up but unreliable"
        ));
    } else if icmp_ok < icmp_total {
        out.info("IPv4 HTTPS is fine; the missing ping replies are harmless");
    }

    let m = Ipv4Reachability {
        status: out.status(),
        icmp: r.icmp,
        https: r.https,
    };
    CheckRun {
        lines: out.into_lines(),
        metrics: Metrics::Ipv4Reachability(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::probes::fake::FakeProbes;
    use crate::probes::PingStats;

    fn texts(r: &CheckRun) -> Vec<String> {
        r.lines.iter().map(|l| l.text.clone()).collect()
    }
    fn m(r: &CheckRun) -> &Ipv4Reachability {
        match &r.metrics {
            Metrics::Ipv4Reachability(m) => m,
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn all_pass() {
        let p = FakeProbes::new();
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Pass);
        assert_eq!(m(&r).icmp.ok, Some(3));
        assert_eq!(m(&r).https.total, Some(4));
        let t = texts(&r);
        assert_eq!(t[0], "ping 1.1.1.1: 0% loss, avg 2.000 ms");
        assert_eq!(
            t[3],
            "Note: ICMP loss alone, with HTTPS still working, is usually the ISP deprioritizing ping, not an outage."
        );
        assert_eq!(
            t[4],
            "HTTPS over IPv4 https://www.google.com: HTTP 200 in 0.300s"
        );
    }

    #[tokio::test]
    async fn ping_loss_warns_and_counts_as_answered() {
        let mut p = FakeProbes::new();
        p.pings.insert(
            "8.8.8.8".into(),
            PingStats::from_rtts(5, &[10.0, 10.0, 10.0, 10.0]),
        );
        p.pings
            .insert("9.9.9.9".into(), PingStats::from_rtts(5, &[]));
        let r = run(&p).await;
        let t = texts(&r);
        assert_eq!(t[1], "ping 8.8.8.8: 20.0% loss, avg 10.000 ms");
        assert_eq!(t[2], "ping 9.9.9.9: no replies (100% loss)");
        assert_eq!(m(&r).icmp.ok, Some(2));
        assert_eq!(m(&r).status, Status::Warn);
        assert_eq!(
            t.last().unwrap(),
            "IPv4 HTTPS is fine; the missing ping replies are harmless"
        );
        assert_eq!(m(&r).icmp.targets[2].avg_ms, None);
    }

    #[tokio::test]
    async fn partial_https_failure_is_info_but_warns() {
        let mut p = FakeProbes::new();
        p.https.insert(
            "https://github.com".into(),
            Err("timed out after 6s".into()),
        );
        let r = run(&p).await;
        let t = texts(&r);
        assert!(t.contains(
            &"HTTPS over IPv4 https://github.com: failed (timed out after 6s)".to_string()
        ));
        assert_eq!(
            t.last().unwrap(),
            "Some IPv4 HTTPS probes failed (3 of 4 worked); the connection is up but unreliable"
        );
        assert_eq!(m(&r).status, Status::Warn);
        assert_eq!(
            m(&r).https.targets[2].error.as_deref(),
            Some("timed out after 6s")
        );
    }

    #[tokio::test]
    async fn most_https_failing_is_a_fail() {
        let mut p = FakeProbes::new();
        p.default_https = Err("could not resolve host".into());
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Fail);
        assert_eq!(m(&r).https.ok, Some(0));
        assert!(texts(&r).contains(&"Most IPv4 HTTPS probes failed (0 of 4 worked)".to_string()));
    }

    #[tokio::test]
    async fn exactly_half_working_is_not_most_failed() {
        // "most probes failed" is ok * 2 < total: 2 of 4 is not a FAIL.
        let mut p = FakeProbes::new();
        p.https.insert("https://github.com".into(), Err("x".into()));
        p.https.insert("https://1.1.1.1".into(), Err("x".into()));
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Warn);
    }
}
