//! Check 07: HTTPS timing split into DNS, connect, TLS and first byte.

use std::time::Duration;

use futures_util::future::join_all;

use super::CheckRun;
use crate::model::Out;
use crate::probes::{Family, Probes};
use crate::record::{HttpProbe, HttpTiming, Metrics};

const URLS: [&str; 2] = ["https://www.google.com", "https://github.com"];

/// Slowest phase; ties go to the earlier phase (dns, connect, tls, ttfb).
pub fn slowest_phase(dns: u64, connect: u64, tls: u64, ttfb: u64) -> &'static str {
    let (mut best, mut name) = (dns, "dns");
    if connect > best {
        best = connect;
        name = "connect";
    }
    if tls > best {
        best = tls;
        name = "tls";
    }
    if ttfb > best {
        name = "ttfb";
    }
    name
}

pub async fn run(probes: &dyn Probes) -> CheckRun {
    let mut out = Out::new();
    let mut m = HttpTiming::default();

    let results = join_all(
        URLS.iter()
            .map(|u| probes.https(u, Family::Any, Duration::from_secs(10))),
    )
    .await;

    let (mut failed, mut total) = (0u32, 0u32);
    for (u, r) in URLS.iter().zip(results) {
        total += 1;
        let s = match r {
            Err(e) => {
                failed += 1;
                m.probes.push(HttpProbe {
                    url: u.to_string(),
                    ok: false,
                    error: Some(e.clone()),
                    ..Default::default()
                });
                out.warn(format!("{u}: request failed: {e}"));
                continue;
            }
            Ok(s) => s,
        };
        let fam = if s.family == Family::V6 {
            "IPv6"
        } else {
            "IPv4"
        };
        let slow = slowest_phase(s.dns_ms, s.connect_ms, s.tls_ms, s.ttfb_ms);
        let line = format!(
            "{u} via {fam}: dns {} ms, connect {} ms, tls {} ms, ttfb {} ms, total {} ms (HTTP {})",
            s.dns_ms, s.connect_ms, s.tls_ms, s.ttfb_ms, s.total_ms, s.code
        );
        if s.total_ms > 3000 || s.dns_ms > 1000 || s.connect_ms > 1000 {
            out.warn(line);
            out.hint(match slow {
                "dns" => "Slowest phase: DNS. Name lookups are slow; see check 06 for which resolver is responsible.",
                "connect" => "Slowest phase: TCP connect. The path to the server is slow or lossy (try check 03/05); an IPv6-then-IPv4 fallback also shows up here.",
                "tls" => "Slowest phase: TLS handshake. Usually packet loss or a congested path.",
                _ => "Slowest phase: waiting for the server's first byte. The server or the path beyond your ISP is slow.",
            });
            m.slow_phase = Some(slow.to_string());
        } else {
            out.pass(line);
        }
        m.probes.push(HttpProbe {
            url: u.to_string(),
            ok: true,
            family: Some(fam.to_lowercase()),
            dns_ms: Some(s.dns_ms),
            connect_ms: Some(s.connect_ms),
            tls_ms: Some(s.tls_ms),
            ttfb_ms: Some(s.ttfb_ms),
            total_ms: Some(s.total_ms),
            http_code: Some(s.code),
            error: None,
        });
    }
    m.failed = Some(failed);
    m.total = Some(total);
    if failed == total {
        out.fail("All HTTP timing probes failed");
        out.hint("No website could be fetched with the default stack. See checks 03, 04 and 06 to see whether IPv4, IPv6 or DNS is at fault.");
    }

    m.status = out.status();
    CheckRun {
        lines: out.into_lines(),
        metrics: Metrics::HttpTiming(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineKind, Status};
    use crate::probes::fake::{sample_ok, FakeProbes};

    fn texts(r: &CheckRun) -> Vec<String> {
        r.lines.iter().map(|l| l.text.clone()).collect()
    }
    fn m(r: &CheckRun) -> &HttpTiming {
        match &r.metrics {
            Metrics::HttpTiming(m) => m,
            _ => panic!(),
        }
    }

    #[test]
    fn slowest_phase_selection() {
        assert_eq!(slowest_phase(5, 20, 30, 40), "ttfb");
        assert_eq!(slowest_phase(5, 20, 300, 40), "tls");
        assert_eq!(slowest_phase(5, 2000, 30, 40), "connect");
        assert_eq!(slowest_phase(2000, 20, 30, 40), "dns");
        assert_eq!(
            slowest_phase(10, 10, 10, 10),
            "dns",
            "ties keep the earlier phase"
        );
    }

    #[tokio::test]
    async fn pass_line_wording() {
        let mut p = FakeProbes::new();
        p.default_https = Ok(sample_ok(200, 300));
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Pass);
        assert_eq!(
            texts(&r)[0],
            "https://www.google.com via IPv4: dns 5 ms, connect 20 ms, tls 30 ms, ttfb 245 ms, total 300 ms (HTTP 200)"
        );
        assert_eq!(m(&r).probes[0].family.as_deref(), Some("ipv4"));
        assert_eq!(m(&r).slow_phase, None);
        assert_eq!(m(&r).failed, Some(0));
    }

    #[tokio::test]
    async fn thresholds_3000_1000_1000() {
        for (dns, conn, total, warns) in [
            (1000, 1000, 3000, false),
            (1001, 10, 100, true),
            (10, 1001, 100, true),
            (10, 10, 3001, true),
        ] {
            let mut p = FakeProbes::new();
            let mut s = sample_ok(200, total);
            s.dns_ms = dns;
            s.connect_ms = conn;
            p.default_https = Ok(s);
            let r = run(&p).await;
            assert_eq!(m(&r).status == Status::Warn, warns, "{dns} {conn} {total}");
        }
    }

    #[tokio::test]
    async fn slow_probe_warns_with_a_phase_hint() {
        let mut p = FakeProbes::new();
        let mut s = sample_ok(200, 4000);
        s.tls_ms = 3500;
        s.ttfb_ms = 400;
        p.default_https = Ok(s);
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Warn);
        assert_eq!(m(&r).slow_phase.as_deref(), Some("tls"));
        assert_eq!(r.lines[1].kind, LineKind::Hint);
        assert_eq!(
            r.lines[1].text,
            "Slowest phase: TLS handshake. Usually packet loss or a congested path."
        );
    }

    #[tokio::test]
    async fn one_failure_warns_all_failures_fail() {
        let mut p = FakeProbes::new();
        p.https.insert(
            "https://github.com".into(),
            Err("timed out after 10s".into()),
        );
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Warn);
        assert_eq!(m(&r).failed, Some(1));
        assert_eq!(
            texts(&r)[1],
            "https://github.com: request failed: timed out after 10s"
        );
        assert!(!m(&r).probes[1].ok);
        assert_eq!(
            m(&r).probes[1].error.as_deref(),
            Some("timed out after 10s")
        );

        let mut p = FakeProbes::new();
        p.default_https = Err("could not resolve host".into());
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Fail);
        assert!(texts(&r).contains(&"All HTTP timing probes failed".to_string()));
    }
}
