//! Table tests for `diagnose()`. Every row is a synthetic run record; the
//! first block ports the cases of `tests/test-diagnose.sh` from the bash tool.

use nettest_core::diagnosis::{diagnose, Finding};
use nettest_core::model::Status;
use nettest_core::record::*;

fn server(group: &str, ip: &str, result: &str, via_vpn: bool) -> DnsServer {
    DnsServer {
        group: group.into(),
        ip: ip.into(),
        result: result.into(),
        ms: if result == "ok" { Some(10) } else { None },
        rcode: None,
        via_vpn: Some(via_vpn),
        label: Some("label".into()),
    }
}

/// The healthy result set of the bash tests.
fn healthy() -> RunRecord {
    let mut r = healthy_core();
    r.checks.dns.as_mut().unwrap().servers = vec![
        server("primary", "192.0.2.1", "ok", false),
        server("public", "1.1.1.1", "ok", false),
        server("public", "8.8.8.8", "ok", false),
        server("public", "9.9.9.9", "ok", false),
    ];
    r
}

fn healthy_core() -> RunRecord {
    let mut r = RunRecord {
        schema: 1,
        ts: "2026-10-22T10:00:00-05:00".into(),
        tool: "test".into(),
        tool_version: "0".into(),
        os: "macos".into(),
        mode: Mode::Full,
        duration_ms: 1,
        exit: 0,
        checks: Checks::default(),
        diagnosis: vec![],
    };
    r.checks.local_network = Some(LocalNetwork {
        status: Status::Pass,
        reachable: Some(true),
        ..Default::default()
    });
    r.checks.vpn_detection = Some(VpnDetection {
        status: Status::Pass,
        primary_via_vpn: Some(false),
        primary_interface: Some("en0".into()),
        ..Default::default()
    });
    r.checks.ipv4_reachability = Some(Ipv4Reachability {
        status: Status::Pass,
        icmp: IcmpBlock {
            ok: Some(3),
            total: Some(3),
            targets: vec![],
        },
        https: HttpsBlock {
            ok: Some(4),
            total: Some(4),
            targets: vec![],
        },
    });
    r.checks.ipv6_reachability = Some(Ipv6Reachability {
        status: Status::Pass,
        global_address: Some(true),
        icmp: IcmpBlock::default(),
        https: HttpsBlock {
            ok: Some(2),
            total: Some(2),
            targets: vec![],
        },
    });
    r.checks.route_trace = Some(RouteTrace {
        status: Status::Pass,
        reached: Some(true),
        last_hop: Some(9),
        last_ip: Some("1.1.1.1".into()),
        ..Default::default()
    });
    r.checks.dns = Some(Dns {
        status: Status::Pass,
        system: DnsSystem {
            status: Some("PASS".into()),
            max_ms: Some(40),
            lookups: vec![],
        },
        servers: vec![],
        primary_timeouts: vec![],
    });
    r.checks.http_timing = Some(HttpTiming {
        status: Status::Pass,
        ..Default::default()
    });
    r
}

fn codes(r: &RunRecord) -> Vec<&'static str> {
    diagnose(r).iter().map(|f| f.code).collect()
}

fn find<'a>(fs: &'a [Finding], code: &str) -> &'a Finding {
    fs.iter().find(|f| f.code == code).unwrap()
}

fn all_text(fs: &[Finding]) -> String {
    fs.iter()
        .flat_map(|f| std::iter::once(f.summary.clone()).chain(f.advice.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn healthy_network() {
    let fs = diagnose(&healthy());
    assert_eq!(fs.len(), 1);
    assert_eq!(fs[0].code, "no_problem");
    assert_eq!(fs[0].summary, "Connection working normally: every check passed.");
    assert!(fs[0].advice[0].starts_with(
        "Checks run: local-network vpn-detection ipv4-reachability ipv6-reachability route-trace dns http-timing."
    ));
}

#[test]
fn vpn_and_public_resolvers_time_out() {
    // Observed run: VPN resolvers .2/.3 and public 1.1.1.1 / 8.8.8.8 timed out.
    let mut r = healthy_core();
    r.checks.vpn_detection.as_mut().unwrap().primary_via_vpn = Some(true);
    r.checks.vpn_detection.as_mut().unwrap().primary_interface = Some("utun4".into());
    let d = r.checks.dns.as_mut().unwrap();
    d.system.status = Some("WARN".into());
    d.primary_timeouts = vec!["198.51.100.2".into(), "198.51.100.3".into()];
    d.servers = vec![
        server("primary", "198.51.100.1", "ok", true),
        server("primary", "198.51.100.2", "timeout", true),
        server("primary", "198.51.100.3", "timeout", true),
        server("public", "1.1.1.1", "timeout", false),
        server("public", "8.8.8.8", "timeout", false),
        server("public", "9.9.9.9", "ok", false),
    ];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_degraded"]);
    let f = find(&fs, "ipv4_degraded");
    assert_eq!(
        f.summary,
        "Degraded IPv4 path: public DNS servers timed out (1.1.1.1, 8.8.8.8)."
    );
    let text = all_text(&fs);
    assert!(text.contains("most likely a consequence"));
    assert!(text
        .contains("The VPN resolver failure (198.51.100.2, 198.51.100.3, reached through utun4)"));
}

fn vpn_primary_timeout() -> RunRecord {
    let mut r = healthy();
    r.checks.vpn_detection.as_mut().unwrap().primary_via_vpn = Some(true);
    r.checks.vpn_detection.as_mut().unwrap().primary_interface = Some("utun4".into());
    let d = r.checks.dns.as_mut().unwrap();
    d.system.status = Some("WARN".into());
    d.primary_timeouts = vec!["198.51.100.2".into()];
    d.servers
        .push(server("primary", "198.51.100.2", "timeout", true));
    r
}

#[test]
fn vpn_resolver_times_out_public_fine() {
    let r = vpn_primary_timeout();
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["primary_dns_timeout_vpn"]);
    assert_eq!(
        fs[0].summary,
        "The primary DNS resolver is timing out: 198.51.100.2 (a VPN resolver, reached through utun4)."
    );
}

#[test]
fn one_public_timeout_is_not_a_majority() {
    let mut r = vpn_primary_timeout();
    let d = r.checks.dns.as_mut().unwrap();
    d.servers = vec![
        server("public", "1.1.1.1", "ok", false),
        server("public", "9.9.9.9", "ok", false),
        server("primary", "198.51.100.2", "timeout", true),
        server("public", "8.8.8.8", "timeout", false),
    ];
    assert_eq!(codes(&r), ["primary_dns_timeout_vpn"]);
}

#[test]
fn non_vpn_primary_times_out() {
    let mut r = healthy();
    let d = r.checks.dns.as_mut().unwrap();
    d.system.status = Some("WARN".into());
    d.primary_timeouts = vec!["192.0.2.9".into()];
    d.servers
        .push(server("primary", "192.0.2.9", "timeout", false));
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["primary_dns_timeout"]);
    assert_eq!(
        fs[0].summary,
        "The primary DNS resolver is timing out: 192.0.2.9."
    );
}

#[test]
fn public_resolvers_time_out_while_ipv4_https_fails() {
    let mut r = healthy_core();
    {
        let v4 = r.checks.ipv4_reachability.as_mut().unwrap();
        v4.https.ok = Some(0);
        v4.icmp.ok = Some(0);
    }
    {
        let t = r.checks.route_trace.as_mut().unwrap();
        t.reached = Some(false);
        t.last_hop = Some(4);
        t.last_ip = Some("10.20.0.4".into());
    }
    r.checks.vpn_detection.as_mut().unwrap().primary_via_vpn = Some(true);
    r.checks.vpn_detection.as_mut().unwrap().primary_interface = Some("utun4".into());
    let d = r.checks.dns.as_mut().unwrap();
    d.system.status = Some("FAIL".into());
    d.primary_timeouts = vec!["198.51.100.2".into()];
    d.servers = vec![
        server("primary", "198.51.100.2", "timeout", true),
        server("public", "1.1.1.1", "timeout", false),
        server("public", "8.8.8.8", "timeout", false),
    ];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_outage", "ipv4_degraded"]);
    assert!(all_text(&fs).contains("hop 4 (10.20.0.4)"));
}

#[test]
fn public_timeouts_alone_are_reported() {
    let mut r = healthy_core();
    r.checks.dns.as_mut().unwrap().servers = vec![
        server("primary", "192.0.2.1", "ok", false),
        server("public", "9.9.9.9", "ok", false),
        server("public", "1.1.1.1", "timeout", false),
        server("public", "8.8.8.8", "timeout", false),
    ];
    assert!(codes(&r).contains(&"ipv4_degraded"));
}

#[test]
fn ipv4_outage_while_ipv6_works() {
    let mut r = healthy();
    r.checks.ipv4_reachability.as_mut().unwrap().https.ok = Some(1);
    let c = codes(&r);
    assert!(c.contains(&"ipv4_outage"));
    assert!(!c.contains(&"isp_down"));
    assert_eq!(
        diagnose(&r)[0].summary,
        "ISP IPv4 outage (often their CGNAT): sites with IPv6 load, IPv4-only sites don't."
    );
}

#[test]
fn ipv4_outage_hop_text_without_a_last_hop() {
    let mut r = healthy();
    r.checks.ipv4_reachability.as_mut().unwrap().https.ok = Some(1);
    let t = r.checks.route_trace.as_mut().unwrap();
    t.reached = Some(false);
    t.last_hop = Some(0);
    t.last_ip = None;
    let text = all_text(&diagnose(&r));
    assert!(
        text.contains("report to the ISP that IPv4 stops somewhere past your router, IPv6 works.")
    );
}

#[test]
fn ipv4_outage_hop_text_with_a_last_hop() {
    let mut r = healthy();
    r.checks.ipv4_reachability.as_mut().unwrap().https.ok = Some(1);
    let t = r.checks.route_trace.as_mut().unwrap();
    t.reached = Some(false);
    t.last_hop = Some(4);
    t.last_ip = Some("10.20.0.4".into());
    let text = all_text(&diagnose(&r));
    assert!(text.contains("report to the ISP that IPv4 stops after hop 4 (10.20.0.4), IPv6 works."));
}

#[test]
fn ipv4_and_ipv6_both_down() {
    let mut r = healthy();
    r.checks.ipv4_reachability.as_mut().unwrap().https.ok = Some(0);
    r.checks.ipv6_reachability.as_mut().unwrap().https.ok = Some(0);
    assert!(codes(&r).contains(&"isp_down"));
    assert_eq!(
        diagnose(&r)[0].summary,
        "Both IPv4 and IPv6 fail past your router: the ISP connection is down."
    );
}

#[test]
fn ipv4_down_no_ipv6_on_this_network() {
    let mut r = healthy();
    r.checks.ipv4_reachability.as_mut().unwrap().https.ok = Some(0);
    let v6 = r.checks.ipv6_reachability.as_mut().unwrap();
    v6.global_address = Some(false);
    v6.https = HttpsBlock::default();
    assert!(codes(&r).contains(&"ipv4_outage_no_ipv6"));
}

#[test]
fn ipv4_down_ipv6_not_tested_is_unverified() {
    // A single-check run of 03: IPv6 is unknown, not absent.
    let mut r = healthy();
    r.mode = Mode::Single;
    r.checks.ipv4_reachability.as_mut().unwrap().https.ok = Some(0);
    r.checks.ipv6_reachability = None;
    r.checks.local_network = None;
    r.checks.route_trace = None;
    assert_eq!(codes(&r), ["ipv4_outage_unverified"]);
    assert_eq!(diagnose(&r)[0].summary, "IPv4 fails past your router.");
}

#[test]
fn gateway_down_and_https_failing() {
    let mut r = healthy();
    r.checks.local_network.as_mut().unwrap().reachable = Some(false);
    r.checks.ipv4_reachability.as_mut().unwrap().https.ok = Some(0);
    let c = codes(&r);
    assert!(c.contains(&"local_network_down"));
    assert!(!c.contains(&"ipv4_outage"));
    assert_eq!(
        diagnose(&r)[0].summary,
        "Local network problem: this Mac cannot reach its own router."
    );
}

#[test]
fn gateway_ignores_ping_but_https_works() {
    let mut r = healthy();
    r.checks.local_network.as_mut().unwrap().reachable = Some(false);
    assert_eq!(codes(&r), ["gateway_icmp_ignored"]);
    assert_eq!(
        diagnose(&r)[0].summary,
        "The router does not answer ping, but internet traffic works."
    );
}

#[test]
fn gateway_down_with_nothing_known_about_ipv4() {
    // Single-check run of 01: no IPv4 data, so the router verdict stands.
    let mut r = healthy();
    r.mode = Mode::Single;
    r.checks.local_network.as_mut().unwrap().reachable = Some(false);
    r.checks.ipv4_reachability = None;
    assert_eq!(codes(&r), ["local_network_down"]);
}

#[test]
fn ping_loss_with_working_https() {
    let mut r = healthy();
    r.checks.ipv4_reachability.as_mut().unwrap().icmp.ok = Some(1);
    assert_eq!(codes(&r), ["icmp_deprioritized"]);
    assert_eq!(
        diagnose(&r)[0].summary,
        "Some pings to public servers got no reply, but HTTPS works."
    );
}

#[test]
fn slow_system_dns_with_healthy_servers() {
    let mut r = healthy();
    r.checks.dns.as_mut().unwrap().system.status = Some("WARN".into());
    assert_eq!(codes(&r), ["system_dns_slow"]);
    assert_eq!(
        diagnose(&r)[0].summary,
        "System DNS is slow although the configured servers answered the direct tests."
    );
}

#[test]
fn warnings_with_no_matching_pattern() {
    let mut r = healthy();
    r.checks.http_timing.as_mut().unwrap().status = Status::Warn;
    assert_eq!(codes(&r), ["unmatched_warnings"]);
    assert_eq!(
        diagnose(&r)[0].summary,
        "No known pattern matched, but some checks reported WARN or FAIL."
    );
}

#[test]
fn info_and_skip_statuses_are_not_warnings() {
    let mut r = healthy();
    r.checks.vpn_detection.as_mut().unwrap().status = Status::Info;
    r.checks.ipv6_reachability.as_mut().unwrap().status = Status::Skip;
    assert_eq!(codes(&r), ["no_problem"]);
}

#[test]
fn empty_record_is_no_problem_with_none_ran() {
    let mut r = healthy();
    r.checks = Checks::default();
    let fs = diagnose(&r);
    assert_eq!(fs[0].code, "no_problem");
    assert!(fs[0].advice[0].starts_with("Checks run: none."));
}

#[test]
fn skipped_public_servers_are_not_counted_as_tried() {
    let mut r = healthy_core();
    r.checks.dns.as_mut().unwrap().servers = vec![
        server("public", "1.1.1.1", "ok", false),
        server("public", "2606:4700:4700::1111", "skipped", false),
    ];
    assert_eq!(codes(&r), ["no_problem"]);
}

// --- Partial IPv4 failure: HTTPS is not "working" (observed twice in a real outage) ---

fn https_probe(url: &str, ok: bool, error: Option<&str>) -> HttpsTarget {
    HttpsTarget {
        url: url.into(),
        ok,
        http_code: if ok { Some(200) } else { None },
        time_ms: if ok { Some(300) } else { None },
        error: error.map(str::to_string),
    }
}

fn http_probe_ipv4(url: &str, connect_ms: u64) -> HttpProbe {
    HttpProbe {
        url: url.into(),
        ok: true,
        family: Some("ipv4".into()),
        connect_ms: Some(connect_ms),
        ..Default::default()
    }
}

fn set_v4(r: &mut RunRecord, ok: u32, icmp_ok: u32) {
    let v4 = r.checks.ipv4_reachability.as_mut().unwrap();
    v4.https.ok = Some(ok);
    v4.icmp.ok = Some(icmp_ok);
}

#[test]
fn real_run_1_partial_https_and_check_07_failure() {
    // Mirrors line 1 of tests/fixtures/run-bash-real.jsonl.
    let mut r = healthy_core();
    set_v4(&mut r, 2, 0);
    r.checks.ipv4_reachability.as_mut().unwrap().https.targets = vec![
        https_probe("https://www.google.com", false, Some("timed out after 6s")),
        https_probe("https://1.1.1.1", true, None),
        https_probe("https://github.com", false, Some("timed out after 6s")),
        https_probe("https://www.cloudflare.com", true, None),
    ];
    r.checks.http_timing.as_mut().unwrap().probes = vec![HttpProbe {
        url: "https://github.com".into(),
        ok: false,
        error: Some("curl: (28) Connection timed out after 10006 milliseconds".into()),
        ..Default::default()
    }];
    let d = r.checks.dns.as_mut().unwrap();
    d.primary_timeouts = vec!["198.51.100.2".into(), "198.51.100.3".into()];
    d.servers = vec![
        server("primary", "198.51.100.2", "timeout", false),
        server("public", "1.1.1.1", "timeout", false),
        server("public", "8.8.8.8", "timeout", false),
        server("public", "9.9.9.9", "ok", false),
    ];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_degraded"], "no icmp_deprioritized");
    assert_eq!(
        fs[0].summary,
        "Degraded IPv4 path: 2 of 4 IPv4 HTTPS probes timed out; github.com did not connect (check 07); public DNS servers timed out (1.1.1.1, 8.8.8.8)."
    );
    let text = all_text(&fs);
    assert!(text.contains(
        "The primary resolver failure (198.51.100.2, 198.51.100.3) is most likely a consequence"
    ));
    assert!(text.contains("IPv6 works while IPv4 fails: the line is fine;"));
}

#[test]
fn real_run_2_public_dns_only_keeps_its_wording() {
    // Mirrors line 2 of the fixture: HTTPS fine, two public resolvers time out.
    let mut r = healthy_core();
    r.checks.dns.as_mut().unwrap().servers = vec![
        server("public", "1.1.1.1", "ok", false),
        server("public", "8.8.8.8", "timeout", false),
        server("public", "9.9.9.9", "timeout", false),
    ];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_degraded"]);
    assert_eq!(
        fs[0].summary,
        "Degraded IPv4 path: public DNS servers timed out (8.8.8.8, 9.9.9.9)."
    );
    assert!(all_text(&fs).contains("IPv6 works while IPv4 fails:"));
}

#[test]
fn partial_https_with_ping_loss_is_degraded_not_harmless() {
    let mut r = healthy();
    set_v4(&mut r, 3, 0);
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_degraded"]);
    assert_eq!(
        fs[0].summary,
        "Degraded IPv4 path: 1 of 4 IPv4 HTTPS probes failed."
    );
}

#[test]
fn slow_ipv4_connect_in_check_07_is_degraded() {
    let mut r = healthy();
    set_v4(&mut r, 4, 0);
    r.checks.http_timing.as_mut().unwrap().probes = vec![
        HttpProbe {
            url: "https://www.google.com".into(),
            ok: true,
            family: Some("ipv6".into()),
            connect_ms: Some(107),
            ..Default::default()
        },
        http_probe_ipv4("https://github.com", 7149),
    ];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_degraded"]);
    assert_eq!(
        fs[0].summary,
        "Degraded IPv4 path: github.com connect took 7.1 s (check 07)."
    );
}

#[test]
fn connect_of_exactly_3000_ms_and_ipv6_slowness_are_not_degraded() {
    let mut r = healthy();
    set_v4(&mut r, 4, 0);
    r.checks.http_timing.as_mut().unwrap().probes =
        vec![http_probe_ipv4("https://github.com", 3000)];
    assert_eq!(codes(&r), ["icmp_deprioritized"]);

    let mut r = healthy();
    r.checks.http_timing.as_mut().unwrap().probes = vec![HttpProbe {
        url: "https://github.com".into(),
        ok: true,
        family: Some("ipv6".into()),
        connect_ms: Some(7000),
        ..Default::default()
    }];
    assert_eq!(codes(&r), ["no_problem"]);
}

#[test]
fn check_07_failed_to_resolve_is_not_ipv4_evidence() {
    let mut r = healthy();
    set_v4(&mut r, 4, 0);
    r.checks.http_timing.as_mut().unwrap().probes = vec![HttpProbe {
        url: "https://github.com".into(),
        ok: false,
        error: Some("curl: (6) Could not resolve host: github.com".into()),
        ..Default::default()
    }];
    assert_eq!(codes(&r), ["icmp_deprioritized"]);
}

#[test]
fn check_07_failure_alone_is_degraded() {
    let mut r = healthy();
    r.checks.http_timing.as_mut().unwrap().probes = vec![HttpProbe {
        url: "https://github.com".into(),
        ok: false,
        error: Some("curl: (28) Connection timed out after 10006 milliseconds".into()),
        ..Default::default()
    }];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_degraded"]);
    assert_eq!(
        fs[0].summary,
        "Degraded IPv4 path: github.com did not connect (check 07)."
    );
}

#[test]
fn no_ipv6_comparison_when_ipv6_is_only_partly_ok() {
    let mut r = healthy();
    set_v4(&mut r, 3, 3);
    r.checks.ipv6_reachability.as_mut().unwrap().https.ok = Some(1);
    assert_eq!(codes(&r), ["ipv4_degraded"]);
    assert!(!all_text(&diagnose(&r)).contains("IPv6 works while IPv4 fails"));
}

#[test]
fn partial_https_failure_comes_before_vpn_resolver_blame() {
    let mut r = vpn_primary_timeout();
    set_v4(&mut r, 3, 3);
    r.checks.dns.as_mut().unwrap().servers = vec![
        server("primary", "198.51.100.2", "timeout", true),
        server("public", "1.1.1.1", "ok", false),
        server("public", "8.8.8.8", "timeout", false),
        server("public", "9.9.9.9", "ok", false),
    ];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_degraded"]);
    assert!(all_text(&fs).contains(
        "The VPN resolver failure (198.51.100.2, reached through utun4) is most likely a consequence"
    ));
}

#[test]
fn outage_does_not_blame_the_vpn_resolver() {
    let mut r = vpn_primary_timeout();
    set_v4(&mut r, 0, 3);
    r.checks.dns.as_mut().unwrap().servers = vec![
        server("primary", "198.51.100.2", "timeout", true),
        server("public", "1.1.1.1", "ok", false),
        server("public", "8.8.8.8", "timeout", false),
        server("public", "9.9.9.9", "ok", false),
    ];
    let fs = diagnose(&r);
    assert_eq!(codes(&r), ["ipv4_outage"]);
    assert!(all_text(&fs).contains(
        "The VPN resolver failure (198.51.100.2, reached through utun4) is most likely a consequence"
    ));
}

#[test]
fn ping_loss_with_full_https_and_fast_check_07_stays_harmless() {
    let mut r = healthy();
    set_v4(&mut r, 4, 0);
    r.checks.http_timing.as_mut().unwrap().probes =
        vec![http_probe_ipv4("https://github.com", 100)];
    assert_eq!(codes(&r), ["icmp_deprioritized"]);
}
