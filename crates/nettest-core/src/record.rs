//! Run record: the schema-1 JSON line described in `docs/run-record.md`.
//!
//! This is a contract shared with the bash tool. Field names and order mirror
//! what `lib/record.sh` writes (the bash `history` awk engine relies on the
//! order). Readers ignore unknown keys; `null` and a missing key mean the same.

use serde::{Deserialize, Serialize};

use crate::model::{CheckId, Status};

pub const SCHEMA: u32 = 1;
pub const TOOL: &str = "nettest-rs";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Full,
    Single,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosisEntry {
    pub code: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub schema: u32,
    /// Local time, ISO 8601 with a numeric offset including the colon.
    pub ts: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub tool_version: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub exit: i32,
    #[serde(default)]
    pub checks: Checks,
    #[serde(default)]
    pub diagnosis: Vec<DiagnosisEntry>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Checks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_network: Option<LocalNetwork>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vpn_detection: Option<VpnDetection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipv4_reachability: Option<Ipv4Reachability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipv6_reachability: Option<Ipv6Reachability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_trace: Option<RouteTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<Dns>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_timing: Option<HttpTiming>,
}

impl Checks {
    /// `(check, status)` for every check present, in run order.
    pub fn statuses(&self) -> Vec<(CheckId, Status)> {
        let mut v = Vec::new();
        if let Some(c) = &self.local_network {
            v.push((CheckId::LocalNetwork, c.status));
        }
        if let Some(c) = &self.vpn_detection {
            v.push((CheckId::VpnDetection, c.status));
        }
        if let Some(c) = &self.ipv4_reachability {
            v.push((CheckId::Ipv4Reachability, c.status));
        }
        if let Some(c) = &self.ipv6_reachability {
            v.push((CheckId::Ipv6Reachability, c.status));
        }
        if let Some(c) = &self.route_trace {
            v.push((CheckId::RouteTrace, c.status));
        }
        if let Some(c) = &self.dns {
            v.push((CheckId::Dns, c.status));
        }
        if let Some(c) = &self.http_timing {
            v.push((CheckId::HttpTiming, c.status));
        }
        v
    }

    pub fn set(&mut self, metrics: Metrics) {
        match metrics {
            Metrics::LocalNetwork(m) => self.local_network = Some(m),
            Metrics::VpnDetection(m) => self.vpn_detection = Some(m),
            Metrics::Ipv4Reachability(m) => self.ipv4_reachability = Some(m),
            Metrics::Ipv6Reachability(m) => self.ipv6_reachability = Some(m),
            Metrics::RouteTrace(m) => self.route_trace = Some(m),
            Metrics::Dns(m) => self.dns = Some(m),
            Metrics::HttpTiming(m) => self.http_timing = Some(m),
        }
    }
}

/// The `checks.<name>` object of one check, as produced by that check.
#[derive(Clone, Debug, PartialEq)]
pub enum Metrics {
    LocalNetwork(LocalNetwork),
    VpnDetection(VpnDetection),
    Ipv4Reachability(Ipv4Reachability),
    Ipv6Reachability(Ipv6Reachability),
    RouteTrace(RouteTrace),
    Dns(Dns),
    HttpTiming(HttpTiming),
}

impl Metrics {
    pub fn status(&self) -> Status {
        match self {
            Metrics::LocalNetwork(m) => m.status,
            Metrics::VpnDetection(m) => m.status,
            Metrics::Ipv4Reachability(m) => m.status,
            Metrics::Ipv6Reachability(m) => m.status,
            Metrics::RouteTrace(m) => m.status,
            Metrics::Dns(m) => m.status,
            Metrics::HttpTiming(m) => m.status,
        }
    }

    pub fn set_status(&mut self, s: Status) {
        match self {
            Metrics::LocalNetwork(m) => m.status = s,
            Metrics::VpnDetection(m) => m.status = s,
            Metrics::Ipv4Reachability(m) => m.status = s,
            Metrics::Ipv6Reachability(m) => m.status = s,
            Metrics::RouteTrace(m) => m.status = s,
            Metrics::Dns(m) => m.status = s,
            Metrics::HttpTiming(m) => m.status = s,
        }
    }

    /// An empty metrics object for a check that crashed or timed out (status FAIL).
    pub fn failed(id: CheckId) -> Metrics {
        let mut m = match id {
            CheckId::LocalNetwork => Metrics::LocalNetwork(LocalNetwork::default()),
            CheckId::VpnDetection => Metrics::VpnDetection(VpnDetection::default()),
            CheckId::Ipv4Reachability => Metrics::Ipv4Reachability(Ipv4Reachability::default()),
            CheckId::Ipv6Reachability => Metrics::Ipv6Reachability(Ipv6Reachability::default()),
            CheckId::RouteTrace => Metrics::RouteTrace(RouteTrace::default()),
            CheckId::Dns => Metrics::Dns(Dns::default()),
            CheckId::HttpTiming => Metrics::HttpTiming(HttpTiming::default()),
        };
        m.set_status(Status::Fail);
        m
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct LocalNetwork {
    #[serde(default)]
    pub status: Status,
    pub interface: Option<String>,
    pub gateway: Option<String>,
    pub link_kind: Option<String>,
    pub reachable: Option<bool>,
    pub loss_pct: Option<f64>,
    pub avg_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct VpnDetection {
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub tunnels: Vec<String>,
    pub primary_via_vpn: Option<bool>,
    pub primary_interface: Option<String>,
    #[serde(default)]
    pub primary_servers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct PingTarget {
    pub host: String,
    pub sent: Option<u32>,
    pub received: Option<u32>,
    pub loss_pct: Option<f64>,
    pub avg_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct IcmpBlock {
    pub ok: Option<u32>,
    pub total: Option<u32>,
    #[serde(default)]
    pub targets: Vec<PingTarget>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct HttpsTarget {
    pub url: String,
    pub ok: bool,
    pub http_code: Option<u16>,
    pub time_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct HttpsBlock {
    pub ok: Option<u32>,
    pub total: Option<u32>,
    #[serde(default)]
    pub targets: Vec<HttpsTarget>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Ipv4Reachability {
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub icmp: IcmpBlock,
    #[serde(default)]
    pub https: HttpsBlock,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Ipv6Reachability {
    #[serde(default)]
    pub status: Status,
    pub global_address: Option<bool>,
    #[serde(default)]
    pub icmp: IcmpBlock,
    #[serde(default)]
    pub https: HttpsBlock,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct HopRecord {
    pub hop: u32,
    pub ip: Option<String>,
    pub ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct RouteTrace {
    #[serde(default)]
    pub status: Status,
    pub target: Option<String>,
    pub reached: Option<bool>,
    pub last_hop: Option<u32>,
    pub last_ip: Option<String>,
    pub hop_count: Option<u32>,
    pub private_hops: Option<u32>,
    #[serde(default)]
    pub hops: Vec<HopRecord>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct DnsLookup {
    pub name: String,
    pub ms: Option<u64>,
    pub rcode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct DnsSystem {
    pub status: Option<String>,
    pub max_ms: Option<u64>,
    #[serde(default)]
    pub lookups: Vec<DnsLookup>,
}

/// Result of one direct DNS query: `ok`, `timeout`, `error` or `skipped`.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct DnsServer {
    pub group: String,
    pub ip: String,
    pub result: String,
    pub ms: Option<u64>,
    pub rcode: Option<String>,
    pub via_vpn: Option<bool>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Dns {
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub system: DnsSystem,
    #[serde(default)]
    pub servers: Vec<DnsServer>,
    #[serde(default)]
    pub primary_timeouts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct HttpProbe {
    pub url: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttfb_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_code: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct HttpTiming {
    #[serde(default)]
    pub status: Status,
    pub failed: Option<u32>,
    pub total: Option<u32>,
    pub slow_phase: Option<String>,
    #[serde(default)]
    pub probes: Vec<HttpProbe>,
}

impl RunRecord {
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("run record serializes")
    }

    pub fn from_json_line(line: &str) -> Result<RunRecord, serde_json::Error> {
        serde_json::from_str(line)
    }
}

/// Round to 3 decimals so stored numbers stay readable (`0.675`, not `0.6750000000000002`).
pub fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Local time now, ISO 8601 with a colon in the offset (`2026-10-22T10:58:03-05:00`).
pub fn iso_now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASH_SAMPLE: &str = include_str!("../../../tests/fixtures/run-bash-sample.jsonl");

    #[test]
    fn bash_fixture_deserializes() {
        let line = BASH_SAMPLE.lines().next().unwrap();
        let r = RunRecord::from_json_line(line).unwrap();
        assert_eq!(r.schema, 1);
        assert_eq!(r.tool, "nettest-bash");
        assert_eq!(r.mode, Mode::Full);
        let ln = r.checks.local_network.as_ref().unwrap();
        assert_eq!(ln.reachable, Some(true));
        assert_eq!(ln.avg_ms, Some(19.542));
        let dns = r.checks.dns.as_ref().unwrap();
        assert_eq!(dns.servers.len(), 7);
        assert_eq!(dns.servers[1].result, "timeout");
        assert_eq!(dns.servers[1].ms, None);
        assert_eq!(r.checks.route_trace.as_ref().unwrap().hops.len(), 9);
        assert_eq!(r.diagnosis[0].code, "ipv4_degraded");
    }

    #[test]
    fn every_bash_fixture_line_round_trips() {
        for line in BASH_SAMPLE.lines() {
            let r = RunRecord::from_json_line(line).unwrap();
            let again = RunRecord::from_json_line(&r.to_json_line()).unwrap();
            assert_eq!(r, again);
        }
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let line = r#"{"schema":1,"ts":"2026-10-22T10:00:00-05:00","future":{"a":1},"checks":{"dns":{"status":"PASS","new_thing":[1,2],"system":{"status":"PASS","max_ms":10,"lookups":[]},"servers":[],"primary_timeouts":[]}},"diagnosis":[]}"#;
        let r = RunRecord::from_json_line(line).unwrap();
        assert_eq!(r.checks.dns.unwrap().system.max_ms, Some(10));
    }

    #[test]
    fn writer_emits_the_bash_layout() {
        let mut r = RunRecord::from_json_line(BASH_SAMPLE.lines().next().unwrap()).unwrap();
        r.tool = TOOL.into();
        let s = r.to_json_line();
        assert!(!s.contains('\n'));
        // The bash awk history engine greps these exact byte sequences.
        assert!(s.starts_with(r#"{"schema":1,"ts":""#));
        assert!(s.contains(r#""https":{"ok":4,"total":4,"targets":["#));
        assert!(s.contains(r#""exit":0"#));
        assert!(s.contains(r#""status":"PASS""#));
        // Envelope key order.
        let keys = [
            "\"schema\"",
            "\"ts\"",
            "\"tool\"",
            "\"tool_version\"",
            "\"os\"",
            "\"mode\"",
            "\"duration_ms\"",
            "\"exit\"",
            "\"checks\"",
            "\"diagnosis\"",
        ];
        let pos: Vec<_> = keys.iter().map(|k| s.find(k).unwrap()).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn failed_metrics_have_fail_status() {
        for id in CheckId::ALL {
            assert_eq!(Metrics::failed(id).status(), Status::Fail);
        }
    }

    #[test]
    fn iso_now_has_offset_with_colon() {
        let ts = iso_now();
        let tail = &ts[ts.len() - 6..];
        assert!(
            tail.starts_with(['+', '-']) && tail.as_bytes()[3] == b':',
            "{ts}"
        );
    }
}
