//! Check 01: default gateway reachability and latency.

use std::net::Ipv4Addr;
use std::time::Duration;

use super::CheckRun;
use crate::model::Out;
use crate::probes::Probes;
use crate::record::{round3, LocalNetwork, Metrics};

pub async fn run(probes: &dyn Probes) -> CheckRun {
    let mut out = Out::new();
    let mut m = LocalNetwork::default();

    let Some(route) = probes.default_route() else {
        out.fail("No default route: this Mac is not connected to any network");
        out.hint("Check that Wi-Fi is on or the cable is plugged in, then re-run.");
        m.reachable = Some(false);
        m.status = out.status();
        return CheckRun {
            lines: out.into_lines(),
            metrics: Metrics::LocalNetwork(m),
        };
    };

    let ifc = route.interface.as_str();
    let gw = route.gateway.as_deref().filter(|g| !g.is_empty());
    let port = route.hardware_port.as_deref().filter(|p| !p.is_empty());
    let port_lc = port.unwrap_or("").to_lowercase();
    let wifi = port_lc.contains("wi-fi") || port_lc.contains("airport");
    let kind = if wifi { "wifi" } else { "wired" };
    let threshold = if wifi { 20.0 } else { 5.0 };

    let port_txt = port.map(|p| format!(" ({p})")).unwrap_or_default();
    out.info(format!(
        "Default route: interface {ifc}{port_txt}, gateway {}",
        gw.unwrap_or("none")
    ));
    m.interface = Some(ifc.to_string());
    m.link_kind = Some(kind.to_string());
    m.gateway = gw.map(str::to_string);
    if ifc.starts_with("utun") {
        out.info(format!(
            "The default route goes through a tunnel interface ({ifc}): all traffic uses a VPN"
        ));
    }

    // A point-to-point or link-local default route has no IPv4 gateway to ping.
    let Some(gw) = gw.filter(|g| g.parse::<Ipv4Addr>().is_ok()) else {
        out.skip(format!(
            "No IPv4 gateway address to ping (got '{}')",
            route
                .gateway
                .as_deref()
                .filter(|g| !g.is_empty())
                .unwrap_or("none")
        ));
        m.status = out.status();
        return CheckRun {
            lines: out.into_lines(),
            metrics: Metrics::LocalNetwork(m),
        };
    };

    let ping = probes.ping(gw, 5, Duration::from_secs(1)).await;
    m.loss_pct = Some(ping.loss_pct);
    m.avg_ms = ping.avg_ms.map(round3);
    let avg_txt = ping
        .avg_ms
        .map(|a| format!("{a:.3}"))
        .unwrap_or_else(|| "?".into());

    if ping.received == 0 {
        out.fail(format!("Gateway {gw} does not answer ping (100% loss)"));
        out.hint("The problem is between this Mac and your router (Wi-Fi, cable, router), not the ISP. Some routers ignore ping, so confirm with the later checks.");
        m.reachable = Some(false);
    } else if ping.loss_pct > 0.0 {
        out.warn(format!(
            "Gateway {gw}: {:.1}% loss, avg {avg_txt} ms",
            ping.loss_pct
        ));
        out.hint("Packet loss on the first hop points to a weak Wi-Fi signal, a bad cable or an overloaded router.");
        m.reachable = Some(true);
    } else if ping.avg_ms.is_some_and(|a| a > threshold) {
        out.warn(format!(
            "Gateway {gw}: avg {avg_txt} ms (expected under {threshold} ms on {kind})"
        ));
        out.hint("A slow first hop is local: Wi-Fi interference, a busy router or a flaky cable.");
        m.reachable = Some(true);
    } else {
        out.pass(format!("Gateway {gw}: 0% loss, avg {avg_txt} ms ({kind})"));
        m.reachable = Some(true);
    }

    m.status = out.status();
    CheckRun {
        lines: out.into_lines(),
        metrics: Metrics::LocalNetwork(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineKind, Status};
    use crate::probes::fake::FakeProbes;
    use crate::probes::{DefaultRoute, PingStats};

    fn texts(r: &CheckRun) -> Vec<String> {
        r.lines.iter().map(|l| l.text.clone()).collect()
    }

    fn local(r: &CheckRun) -> &LocalNetwork {
        match &r.metrics {
            Metrics::LocalNetwork(m) => m,
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn pass_on_wifi() {
        let mut p = FakeProbes::new();
        p.default_ping = PingStats::from_rtts(5, &[1.0, 2.0, 3.0, 4.0, 5.0]);
        let r = run(&p).await;
        assert_eq!(local(&r).status, Status::Pass);
        let t = texts(&r);
        assert_eq!(
            t[0],
            "Default route: interface en0 (Wi-Fi), gateway 192.0.2.1"
        );
        assert_eq!(t[1], "Gateway 192.0.2.1: 0% loss, avg 3.000 ms (wifi)");
        assert_eq!(local(&r).link_kind.as_deref(), Some("wifi"));
        assert_eq!(local(&r).reachable, Some(true));
        assert_eq!(local(&r).avg_ms, Some(3.0));
    }

    #[tokio::test]
    async fn warn_on_loss() {
        let mut p = FakeProbes::new();
        p.default_ping = PingStats::from_rtts(5, &[2.0, 2.0, 2.0, 2.0]);
        let r = run(&p).await;
        assert_eq!(local(&r).status, Status::Warn);
        assert_eq!(texts(&r)[1], "Gateway 192.0.2.1: 20.0% loss, avg 2.000 ms");
        assert_eq!(local(&r).reachable, Some(true));
    }

    #[tokio::test]
    async fn warn_on_latency_wired_threshold_is_5_ms() {
        let mut p = FakeProbes::new();
        p.route = Some(DefaultRoute {
            interface: "en4".into(),
            gateway: Some("192.0.2.1".into()),
            hardware_port: Some("Ethernet Adapter (en4)".into()),
        });
        p.default_ping = PingStats::from_rtts(5, &[6.0; 5]);
        let r = run(&p).await;
        assert_eq!(local(&r).status, Status::Warn);
        assert_eq!(
            texts(&r)[1],
            "Gateway 192.0.2.1: avg 6.000 ms (expected under 5 ms on wired)"
        );
        // The same latency is fine on Wi-Fi (threshold 20 ms).
        let mut p = FakeProbes::new();
        p.default_ping = PingStats::from_rtts(5, &[6.0; 5]);
        assert_eq!(local(&run(&p).await).status, Status::Pass);
    }

    #[tokio::test]
    async fn fail_when_gateway_is_silent() {
        let mut p = FakeProbes::new();
        p.default_ping = PingStats::from_rtts(5, &[]);
        let r = run(&p).await;
        assert_eq!(local(&r).status, Status::Fail);
        assert_eq!(local(&r).reachable, Some(false));
        assert_eq!(local(&r).avg_ms, None);
        assert_eq!(
            texts(&r)[1],
            "Gateway 192.0.2.1 does not answer ping (100% loss)"
        );
    }

    #[tokio::test]
    async fn fail_without_default_route() {
        let mut p = FakeProbes::new();
        p.route = None;
        let r = run(&p).await;
        assert_eq!(local(&r).status, Status::Fail);
        assert_eq!(local(&r).reachable, Some(false));
        assert_eq!(
            texts(&r)[0],
            "No default route: this Mac is not connected to any network"
        );
        assert_eq!(r.lines[1].kind, LineKind::Hint);
    }

    #[tokio::test]
    async fn skip_when_gateway_is_not_ipv4() {
        let mut p = FakeProbes::new();
        p.route = Some(DefaultRoute {
            interface: "utun3".into(),
            gateway: Some("link#14".into()),
            hardware_port: None,
        });
        let r = run(&p).await;
        let t = texts(&r);
        assert_eq!(local(&r).status, Status::Skip);
        assert_eq!(local(&r).reachable, None);
        assert_eq!(
            t[1],
            "The default route goes through a tunnel interface (utun3): all traffic uses a VPN"
        );
        assert_eq!(t[2], "No IPv4 gateway address to ping (got 'link#14')");
    }
}
