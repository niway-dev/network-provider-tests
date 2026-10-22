//! ICMP + HTTPS probing shared by checks 03 (IPv4) and 04 (IPv6).

use std::time::Duration;

use futures_util::future::join_all;

use crate::model::Out;
use crate::probes::{Family, Probes};
use crate::record::{round3, HttpsBlock, HttpsTarget, IcmpBlock, PingTarget};

pub struct ReachSpec<'a> {
    /// `ping` or `ping6`, as printed.
    pub ping_label: &'a str,
    /// `HTTPS over IPv4`, as printed.
    pub https_label: &'a str,
    pub ping_targets: &'a [&'a str],
    pub urls: &'a [&'a str],
    pub family: Family,
    /// Printed (dim) after the ICMP lines.
    pub icmp_note: Option<&'a str>,
}

pub struct ReachResult {
    pub icmp: IcmpBlock,
    pub https: HttpsBlock,
}

/// Run every ping and HTTPS probe in parallel, then print the lines in order.
pub async fn probe(probes: &dyn Probes, spec: &ReachSpec<'_>, out: &mut Out) -> ReachResult {
    let pings = join_all(
        spec.ping_targets
            .iter()
            .map(|t| probes.ping(t, 5, Duration::from_secs(1))),
    );
    let https = join_all(
        spec.urls
            .iter()
            .map(|u| probes.https(u, spec.family, Duration::from_secs(6))),
    );
    let (pings, https) = tokio::join!(pings, https);

    // --- ICMP ---
    let mut icmp = IcmpBlock {
        ok: Some(0),
        total: Some(0),
        targets: vec![],
    };
    for (t, p) in spec.ping_targets.iter().zip(&pings) {
        let label = spec.ping_label;
        icmp.total = icmp.total.map(|n| n + 1);
        icmp.targets.push(PingTarget {
            host: t.to_string(),
            sent: Some(p.sent),
            received: Some(p.received),
            loss_pct: Some(p.loss_pct),
            avg_ms: p.avg_ms.map(round3),
        });
        let avg = p
            .avg_ms
            .map(|a| format!("{a:.3}"))
            .unwrap_or_else(|| "-".into());
        if p.received == 0 {
            out.warn(format!("{label} {t}: no replies (100% loss)"));
        } else if p.loss_pct > 0.0 {
            icmp.ok = icmp.ok.map(|n| n + 1);
            out.warn(format!(
                "{label} {t}: {:.1}% loss, avg {avg} ms",
                p.loss_pct
            ));
        } else {
            icmp.ok = icmp.ok.map(|n| n + 1);
            out.pass(format!("{label} {t}: 0% loss, avg {avg} ms"));
        }
    }
    if let Some(note) = spec.icmp_note {
        out.dim(note);
    }

    // --- HTTPS ---
    let mut block = HttpsBlock {
        ok: Some(0),
        total: Some(0),
        targets: vec![],
    };
    for (u, r) in spec.urls.iter().zip(&https) {
        block.total = block.total.map(|n| n + 1);
        match r {
            Ok(s) => {
                block.ok = block.ok.map(|n| n + 1);
                block.targets.push(HttpsTarget {
                    url: u.to_string(),
                    ok: true,
                    http_code: Some(s.code),
                    time_ms: Some(s.total_ms),
                    error: None,
                });
                out.pass(format!(
                    "{} {u}: HTTP {} in {:.3}s",
                    spec.https_label,
                    s.code,
                    s.total_ms as f64 / 1000.0
                ));
            }
            Err(e) => {
                block.targets.push(HttpsTarget {
                    url: u.to_string(),
                    ok: false,
                    http_code: None,
                    time_ms: None,
                    error: Some(e.clone()),
                });
                out.warn(format!("{} {u}: failed ({e})", spec.https_label));
            }
        }
    }

    ReachResult { icmp, https: block }
}
