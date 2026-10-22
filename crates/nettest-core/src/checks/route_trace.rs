//! Check 05: traceroute to 1.1.1.1.

use std::net::Ipv4Addr;

use super::{non_empty, CheckRun};
use crate::model::Out;
use crate::probes::Probes;
use crate::record::{HopRecord, Metrics, RouteTrace};

const TARGET: &str = "1.1.1.1";
const MAX_HOPS: u8 = 15;

/// Private and CGNAT ranges that sit inside an ISP network when they appear
/// right after the home router: 10/8, 100.64/10, 172.16/12.
pub fn is_isp_internal(ip: &str) -> bool {
    let Ok(ip) = ip.parse::<Ipv4Addr>() else {
        return false;
    };
    let o = ip.octets();
    o[0] == 10
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        || (o[0] == 172 && (16..=31).contains(&o[1]))
}

pub async fn run(probes: &dyn Probes) -> CheckRun {
    let mut out = Out::new();
    let trace = probes.traceroute(TARGET, MAX_HOPS).await;

    let (mut last_hop, mut last_ip, mut reached, mut hops, mut internal) =
        (0u32, String::new(), false, 0u32, 0u32);
    let mut records = Vec::new();
    for h in &trace.hops {
        let n = u32::from(h.hop);
        hops = n;
        match &h.ip {
            None => {
                records.push(HopRecord {
                    hop: n,
                    ip: None,
                    ms: None,
                });
                out.dim(format!("{n}  *  (no reply)"));
            }
            Some(ip) => {
                last_hop = n;
                last_ip = ip.clone();
                if ip == TARGET {
                    reached = true;
                }
                let mut label = "";
                if n > 1 && is_isp_internal(ip) {
                    label = "  [private/CGNAT range: likely ISP internal network]";
                    internal += 1;
                }
                let ms = h.ms.map(crate::record::round3);
                records.push(HopRecord {
                    hop: n,
                    ip: Some(ip.clone()),
                    ms,
                });
                let ms_txt = ms.map(|v| format!("{v:.3}")).unwrap_or_else(|| "?".into());
                out.dim(format!("{n}  {ip}  {ms_txt} ms{label}"));
            }
        }
    }

    if hops == 0 {
        out.warn(format!(
            "traceroute produced no hops: {}",
            trace.error.as_deref().unwrap_or("")
        ));
        out.hint("This usually means the network is down or traceroute is blocked.");
    } else if reached {
        out.pass(format!("Reached {TARGET} at hop {last_hop}"));
    } else {
        if last_hop == 0 {
            out.warn("No hop answered at all (not even the router)");
        } else {
            out.warn(format!(
                "Did not reach {TARGET}; last responding hop: {last_hop} ({last_ip})"
            ));
        }
        out.hint(format!("Hops in 10.x or 100.64.x right after your router are the ISP's internal network. 'Dies after hop {last_hop} inside the ISP' is evidence to give the ISP."));
        out.dim("Note: some routers drop traceroute probes even when traffic flows; trust this only together with the HTTPS results.");
    }

    let m = RouteTrace {
        status: out.status(),
        target: Some(TARGET.into()),
        reached: Some(reached),
        last_hop: Some(last_hop),
        last_ip: non_empty(Some(&last_ip)),
        hop_count: Some(hops),
        private_hops: Some(internal),
        hops: records,
    };
    CheckRun {
        lines: out.into_lines(),
        metrics: Metrics::RouteTrace(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::probes::fake::FakeProbes;
    use crate::probes::{Hop, Trace};

    fn texts(r: &CheckRun) -> Vec<String> {
        r.lines.iter().map(|l| l.text.clone()).collect()
    }
    fn m(r: &CheckRun) -> &RouteTrace {
        match &r.metrics {
            Metrics::RouteTrace(m) => m,
            _ => panic!(),
        }
    }
    fn hop(n: u8, ip: Option<&str>, ms: Option<f64>) -> Hop {
        Hop {
            hop: n,
            ip: ip.map(str::to_string),
            ms,
        }
    }

    #[test]
    fn isp_internal_boundaries() {
        for ip in [
            "10.0.0.1",
            "10.255.255.255",
            "100.64.0.1",
            "100.127.255.254",
            "172.16.0.1",
            "172.31.255.255",
        ] {
            assert!(is_isp_internal(ip), "{ip}");
        }
        for ip in [
            "9.255.255.255",
            "11.0.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "192.168.1.1",
            "1.1.1.1",
            "not-an-ip",
        ] {
            assert!(!is_isp_internal(ip), "{ip}");
        }
    }

    #[tokio::test]
    async fn reached_target() {
        let mut p = FakeProbes::new();
        p.trace = Trace {
            hops: vec![
                hop(1, Some("192.0.2.1"), Some(6.5)),
                hop(2, None, None),
                hop(3, Some("10.20.0.1"), Some(33.0)),
                hop(4, Some("1.1.1.1"), Some(27.5)),
            ],
            error: None,
        };
        let r = run(&p).await;
        let t = texts(&r);
        assert_eq!(t[0], "1  192.0.2.1  6.500 ms");
        assert_eq!(t[1], "2  *  (no reply)");
        assert_eq!(
            t[2],
            "3  10.20.0.1  33.000 ms  [private/CGNAT range: likely ISP internal network]"
        );
        assert_eq!(t[4], "Reached 1.1.1.1 at hop 4");
        assert_eq!(m(&r).status, Status::Pass);
        assert_eq!(m(&r).reached, Some(true));
        assert_eq!(m(&r).last_hop, Some(4));
        assert_eq!(m(&r).hop_count, Some(4));
        assert_eq!(m(&r).private_hops, Some(1));
        assert_eq!(m(&r).hops[1].ip, None);
    }

    #[tokio::test]
    async fn first_hop_is_never_counted_as_isp_internal() {
        let mut p = FakeProbes::new();
        p.trace = Trace {
            hops: vec![
                hop(1, Some("10.0.0.1"), Some(1.0)),
                hop(2, Some("1.1.1.1"), Some(5.0)),
            ],
            error: None,
        };
        assert_eq!(m(&run(&p).await).private_hops, Some(0));
    }

    #[tokio::test]
    async fn did_not_reach_target() {
        let mut p = FakeProbes::new();
        p.trace = Trace {
            hops: vec![
                hop(1, Some("192.0.2.1"), Some(1.0)),
                hop(2, Some("10.20.0.4"), Some(20.0)),
                hop(3, None, None),
            ],
            error: None,
        };
        let r = run(&p).await;
        assert_eq!(m(&r).status, Status::Warn);
        assert_eq!(m(&r).reached, Some(false));
        assert_eq!(m(&r).last_hop, Some(2));
        assert_eq!(m(&r).last_ip.as_deref(), Some("10.20.0.4"));
        assert_eq!(m(&r).hop_count, Some(3));
        assert!(texts(&r)
            .contains(&"Did not reach 1.1.1.1; last responding hop: 2 (10.20.0.4)".to_string()));
    }

    #[tokio::test]
    async fn no_hop_answered_at_all() {
        let mut p = FakeProbes::new();
        p.trace = Trace {
            hops: vec![hop(1, None, None), hop(2, None, None)],
            error: None,
        };
        let r = run(&p).await;
        assert!(texts(&r).contains(&"No hop answered at all (not even the router)".to_string()));
        assert_eq!(m(&r).last_hop, Some(0));
        assert_eq!(m(&r).last_ip, None);
    }

    #[tokio::test]
    async fn no_hops_warns_with_the_reason() {
        let mut p = FakeProbes::new();
        p.trace = Trace {
            hops: vec![],
            error: Some("traceroute: sendto: No route to host".into()),
        };
        let r = run(&p).await;
        assert_eq!(
            texts(&r)[0],
            "traceroute produced no hops: traceroute: sendto: No route to host"
        );
        assert_eq!(m(&r).hop_count, Some(0));
        assert_eq!(m(&r).status, Status::Warn);
    }
}
