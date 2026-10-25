//! Diagnosis: a pure function over a run record. Rules, order, thresholds and
//! wording are ported 1:1 from `lib/diagnose.sh`. The codes are a stable
//! machine contract (`docs/run-record.md`): never rename one.

use crate::model::Status;
use crate::record::{DiagnosisEntry, RunRecord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    /// One sentence; stored in the run record.
    pub summary: String,
    /// Extra lines printed under the summary; not stored.
    pub advice: Vec<String>,
}

impl Finding {
    fn new(code: &'static str, summary: impl Into<String>, advice: Vec<String>) -> Finding {
        Finding {
            code,
            summary: summary.into(),
            advice,
        }
    }

    pub fn to_entry(&self) -> DiagnosisEntry {
        DiagnosisEntry {
            code: self.code.to_string(),
            summary: self.summary.clone(),
        }
    }
}

fn s(v: &str) -> String {
    v.to_string()
}

/// Milliseconds as seconds with one decimal ("7.1"), integer math only so it
/// matches the bash tool exactly.
fn secs1(ms: u64) -> String {
    let t = (ms + 50) / 100;
    format!("{}.{}", t / 10, t % 10)
}

/// The host part of an https URL.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest)
}

/// Diagnose the checks present in `rec`. A check that did not run contributes
/// "unknown", never "failed".
pub fn diagnose(rec: &RunRecord) -> Vec<Finding> {
    let c = &rec.checks;
    let mut out: Vec<Finding> = Vec::new();

    let gw_ok = c.local_network.as_ref().and_then(|l| l.reachable);

    let v4_https = c.ipv4_reachability.as_ref().map(|r| &r.https);
    let v4_ok = v4_https.and_then(|h| h.ok);
    let v4_tot = v4_https.and_then(|h| h.total);
    let icmp = c.ipv4_reachability.as_ref().map(|r| &r.icmp);
    let icmp_ok = icmp.and_then(|h| h.ok);
    let icmp_tot = icmp.and_then(|h| h.total);

    let v6_global = c.ipv6_reachability.as_ref().and_then(|r| r.global_address);
    let v6_https = c.ipv6_reachability.as_ref().map(|r| &r.https);
    let v6_ok = v6_https.and_then(|h| h.ok);
    let v6_tot = v6_https.and_then(|h| h.total);

    let trace = c.route_trace.as_ref();
    let reached = trace.and_then(|t| t.reached);
    let last_hop = trace.and_then(|t| t.last_hop);
    let last_ip = trace.and_then(|t| t.last_ip.clone()).unwrap_or_default();

    let dns = c.dns.as_ref();
    let sys_status = dns
        .and_then(|d| d.system.status.clone())
        .unwrap_or_default();
    let prim_bad = dns
        .map(|d| d.primary_timeouts.join(", "))
        .unwrap_or_default();
    let via_vpn = c.vpn_detection.as_ref().and_then(|v| v.primary_via_vpn) == Some(true);
    let prim_if = c
        .vpn_detection
        .as_ref()
        .and_then(|v| v.primary_interface.clone())
        .unwrap_or_default();

    let mut v4_known = false;
    let mut v4_fail = false;
    if let Some(tot) = v4_tot.filter(|t| *t > 0) {
        v4_known = true;
        v4_fail = v4_ok.unwrap_or(0) * 2 < tot;
    }
    let mut v6_known = false;
    let mut v6_works = false;
    if let Some(tot) = v6_tot.filter(|t| *t > 0) {
        v6_known = true;
        v6_works = v6_ok.unwrap_or(0) * 2 >= tot;
    }

    // Public IPv4 DNS servers: the ones every Mac can reach regardless of its own
    // DNS settings. If most of them time out, IPv4 itself is degraded.
    let servers = dns.map(|d| d.servers.as_slice()).unwrap_or(&[]);
    let pub_tried = servers
        .iter()
        .filter(|x| x.group == "public" && x.result != "skipped")
        .count();
    let pub_timeouts: Vec<&str> = servers
        .iter()
        .filter(|x| x.group == "public" && x.result == "timeout")
        .map(|x| x.ip.as_str())
        .collect();
    let pub_names = pub_timeouts.join(", ");
    let pub_degraded = pub_tried > 0 && pub_timeouts.len() * 2 > pub_tried;

    // IPv6 fully OK (every probe answered): lets us say the line itself is fine.
    let v6_full = v6_known && v6_ok.unwrap_or(0) == v6_tot.unwrap_or(0);

    // IPv4 evidence short of an outage. Order: HTTPS probes, check 07, public DNS.
    let mut evidence: Vec<String> = Vec::new();
    let mut v4_partial = false;
    if let (true, false, Some(tot)) = (v4_known, v4_fail, v4_tot) {
        let ok = v4_ok.unwrap_or(0);
        if ok < tot {
            v4_partial = true;
            let nfail = (tot - ok) as usize;
            let ntimed = v4_https
                .map(|h| {
                    h.targets
                        .iter()
                        .filter(|t| {
                            !t.ok && t.error.as_deref().is_some_and(|e| e.contains("timed out"))
                        })
                        .count()
                })
                .unwrap_or(0);
            let word = if ntimed == nfail {
                "timed out"
            } else {
                "failed"
            };
            evidence.push(format!("{nfail} of {tot} IPv4 HTTPS probes {word}"));
        }
    }
    // Check 07 uses the default stack, so a failed probe means the IPv4 fallback
    // failed too, and a slow connect on an IPv4 probe is an IPv4 symptom. A probe
    // that failed only to resolve the name says nothing about IPv4.
    let mut c07: Vec<String> = Vec::new();
    if let Some(h) = c.http_timing.as_ref() {
        for p in h.probes.iter().filter(|p| !p.ok) {
            if p.error.as_deref().is_some_and(|e| e.contains("esolve")) {
                continue;
            }
            c07.push(format!("{} did not connect", host_of(&p.url)));
        }
        for p in h.probes.iter().filter(|p| p.ok) {
            if p.family.as_deref() == Some("ipv4") {
                if let Some(ms) = p.connect_ms.filter(|ms| *ms > 3000) {
                    c07.push(format!("{} connect took {} s", host_of(&p.url), secs1(ms)));
                }
            }
        }
    }
    if !c07.is_empty() {
        evidence.push(format!("{} (check 07)", c07.join(", ")));
    }
    if pub_degraded {
        evidence.push(format!("public DNS servers timed out ({pub_names})"));
    }

    // A failing primary/VPN resolver next to a bad IPv4 path is a consequence of it.
    let cons_line = if prim_bad.is_empty() {
        None
    } else if via_vpn {
        Some(format!(
            "The VPN resolver failure ({prim_bad}, reached through {prim_if}) is most likely a consequence, not its own fault."
        ))
    } else {
        Some(format!(
            "The primary resolver failure ({prim_bad}) is most likely a consequence, not its own fault."
        ))
    };
    // The IPv4 outage findings carry it themselves unless the degraded finding does.
    let out_cons = if pub_degraded {
        None
    } else {
        cons_line.clone()
    };

    // 1. Gateway.
    let mut local_down = false;
    if gw_ok == Some(false) {
        if v4_known && !v4_fail {
            out.push(Finding::new(
                "gateway_icmp_ignored",
                "The router does not answer ping, but internet traffic works.",
                vec![s("Harmless: many routers ignore ICMP. Nothing to fix.")],
            ));
        } else {
            local_down = true;
            out.push(Finding::new(
                "local_network_down",
                "Local network problem: this Mac cannot reach its own router.",
                vec![
                    s("The ISP is probably not the cause. Restart the router, check the cable or Wi-Fi signal,"),
                    s("and make sure this Mac is connected to the right network."),
                ],
            ));
        }
    }

    // 2. IPv4 / IPv6 / ISP.
    if !local_down && v4_fail {
        let hop_txt = match (reached, last_hop) {
            (Some(false), Some(h)) if h != 0 => {
                format!("IPv4 stops after hop {h} ({last_ip})")
            }
            _ => s("IPv4 stops somewhere past your router"),
        };
        let (code, summary, first) = if v6_known && v6_works {
            (
                "ipv4_outage",
                "ISP IPv4 outage (often their CGNAT): sites with IPv6 load, IPv4-only sites don't.",
                format!("Restart the router/ONT. If it persists, report to the ISP that {hop_txt}, IPv6 works."),
            )
        } else if v6_known {
            (
                "isp_down",
                "Both IPv4 and IPv6 fail past your router: the ISP connection is down.",
                format!("Restart the router/ONT. If it persists, report to the ISP that {hop_txt} and IPv6 fails too."),
            )
        } else if v6_global == Some(false) {
            (
                "ipv4_outage_no_ipv6",
                "IPv4 fails past your router and this network has no IPv6 to compare.",
                format!("Most likely an ISP outage. Restart the router/ONT; if it persists, report that {hop_txt}."),
            )
        } else {
            (
                "ipv4_outage_unverified",
                "IPv4 fails past your router.",
                format!("Run the full nettest (without arguments) to compare with IPv6. Restart the router/ONT; if it persists, report that {hop_txt}."),
            )
        };
        let mut advice = vec![first];
        advice.extend(out_cons.clone());
        out.push(Finding::new(code, summary, advice));
    }

    // 3. IPv4 degraded: partial HTTPS failures, a failed or slow IPv4 connect in
    // check 07, or most public IPv4 resolvers timing out. An outage (step 2) already
    // covers the first two, so only the DNS evidence adds a finding next to it.
    // The primary (VPN) resolver is blamed only when IPv4 itself looks fine.
    let degraded = !local_down && (pub_degraded || (!v4_fail && (v4_partial || !c07.is_empty())));
    let any_dns_ok = servers.iter().any(|x| x.result == "ok");
    if degraded {
        let mut lines = vec![if pub_degraded {
            s("Several public IPv4 DNS servers time out as well, so the problem is the IPv4 path, not one DNS server.")
        } else {
            s("Some IPv4 connections fail or are slow, so the IPv4 path is unreliable.")
        }];
        lines.extend(cons_line.clone());
        if v6_full {
            lines.push(s("IPv6 works while IPv4 fails: the line is fine; the IPv4 path (often the ISP's CGNAT) or something on this machine that handles only IPv4 traffic (VPN/security agent) is at fault. Compare with another device on the same network to tell which."));
        }
        lines.push(s(
            "Re-run, or use nettest watch to see how often it happens. If it persists, report IPv4 instability to the ISP.",
        ));
        out.push(Finding::new(
            "ipv4_degraded",
            format!("Degraded IPv4 path: {}.", evidence.join("; ")),
            lines,
        ));
    } else if !v4_fail && (sys_status == "WARN" || sys_status == "FAIL") && any_dns_ok {
        if !prim_bad.is_empty() {
            if via_vpn {
                out.push(Finding::new(
                    "primary_dns_timeout_vpn",
                    format!("The primary DNS resolver is timing out: {prim_bad} (a VPN resolver, reached through {prim_if})."),
                    vec![
                        s("Every lookup waits for that timeout before the OS falls back to another server, so sites feel slow."),
                        s("Disconnect the VPN or ask its administrator to fix its upstream DNS."),
                    ],
                ));
            } else {
                out.push(Finding::new(
                    "primary_dns_timeout",
                    format!("The primary DNS resolver is timing out: {prim_bad}."),
                    vec![
                        s("Every lookup waits for that timeout before the OS falls back. Other servers answer quickly;"),
                        s("remove the dead server or switch DNS in Network settings (router address or a public resolver)."),
                    ],
                ));
            }
        } else {
            out.push(Finding::new(
                "system_dns_slow",
                "System DNS is slow although the configured servers answered the direct tests.",
                vec![s(
                    "It may be intermittent. Re-run, and look at the 06-dns output to see which lookups were slow.",
                )],
            ));
        }
    }

    // 4. ICMP loss with fully working IPv4 HTTPS and no IPv4 trouble in check 07.
    let https_full = v4_known && v4_ok.unwrap_or(0) == v4_tot.unwrap_or(0);
    if !local_down && https_full && c07.is_empty() {
        if let (Some(ok), Some(tot)) = (icmp_ok, icmp_tot) {
            if ok < tot {
                out.push(Finding::new(
                    "icmp_deprioritized",
                    "Some pings to public servers got no reply, but HTTPS works.",
                    vec![s(
                        "Harmless: the ISP deprioritizes or drops ping. This is not an outage.",
                    )],
                ));
            }
        }
    }

    // 5. Nothing matched.
    if out.is_empty() {
        let any_bad = c
            .statuses()
            .iter()
            .any(|(_, st)| matches!(st, Status::Fail | Status::Warn));
        if any_bad {
            out.push(Finding::new(
                "unmatched_warnings",
                "No known pattern matched, but some checks reported WARN or FAIL.",
                vec![s(
                    "Read the WARN/FAIL lines above; the hint under each one explains what it means.",
                )],
            ));
        } else {
            let ran: Vec<&str> = c.statuses().iter().map(|(id, _)| id.name()).collect();
            let ran = if ran.is_empty() {
                s("none")
            } else {
                ran.join(" ")
            };
            out.push(Finding::new(
                "no_problem",
                "Connection working normally: every check passed.",
                vec![format!(
                    "Checks run: {ran}. If the problem comes and goes, run nettest watch during an incident."
                )],
            ));
        }
    }

    out
}
