//! Check model: statuses, printed lines and the list of checks.

use serde::{Deserialize, Serialize};

/// Outcome of one line or one check. Serialized as `PASS`, `WARN`, ...
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Status {
    Pass,
    Warn,
    /// Also the default: a check that recorded no status counts as failed.
    #[default]
    Fail,
    Info,
    Skip,
}

impl Status {
    pub fn tag(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
            Status::Info => "INFO",
            Status::Skip => "SKIP",
        }
    }
}

/// What a printed line is: a status line, an indented hint (`->`) or a dim note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Status(Status),
    Hint,
    Dim,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

/// Collects the lines of one check and derives its status the way the bash
/// `nt_end` does: FAIL beats WARN beats PASS beats SKIP beats INFO. INFO and
/// SKIP lines next to PASS lines do not downgrade a check.
#[derive(Default)]
pub struct Out {
    lines: Vec<Line>,
    pass: u32,
    warn: u32,
    fail: u32,
    info: u32,
    skip: u32,
}

impl Out {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, kind: LineKind, text: impl Into<String>) {
        self.lines.push(Line {
            kind,
            text: text.into(),
        });
    }

    pub fn pass(&mut self, text: impl Into<String>) {
        self.pass += 1;
        self.push(LineKind::Status(Status::Pass), text);
    }
    pub fn warn(&mut self, text: impl Into<String>) {
        self.warn += 1;
        self.push(LineKind::Status(Status::Warn), text);
    }
    pub fn fail(&mut self, text: impl Into<String>) {
        self.fail += 1;
        self.push(LineKind::Status(Status::Fail), text);
    }
    pub fn info(&mut self, text: impl Into<String>) {
        self.info += 1;
        self.push(LineKind::Status(Status::Info), text);
    }
    pub fn skip(&mut self, text: impl Into<String>) {
        self.skip += 1;
        self.push(LineKind::Status(Status::Skip), text);
    }
    pub fn hint(&mut self, text: impl Into<String>) {
        self.push(LineKind::Hint, text);
    }
    pub fn dim(&mut self, text: impl Into<String>) {
        self.push(LineKind::Dim, text);
    }

    pub fn status(&self) -> Status {
        if self.fail > 0 {
            Status::Fail
        } else if self.warn > 0 {
            Status::Warn
        } else if self.pass > 0 {
            Status::Pass
        } else if self.skip > 0 {
            Status::Skip
        } else if self.info > 0 {
            Status::Info
        } else {
            // A check that printed nothing counts as passed, like bash.
            Status::Pass
        }
    }

    pub fn into_lines(self) -> Vec<Line> {
        self.lines
    }
}

/// The seven checks, in run order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckId {
    LocalNetwork,
    VpnDetection,
    Ipv4Reachability,
    Ipv6Reachability,
    RouteTrace,
    Dns,
    HttpTiming,
}

impl CheckId {
    pub const ALL: [CheckId; 7] = [
        CheckId::LocalNetwork,
        CheckId::VpnDetection,
        CheckId::Ipv4Reachability,
        CheckId::Ipv6Reachability,
        CheckId::RouteTrace,
        CheckId::Dns,
        CheckId::HttpTiming,
    ];

    pub fn number(self) -> u8 {
        match self {
            CheckId::LocalNetwork => 1,
            CheckId::VpnDetection => 2,
            CheckId::Ipv4Reachability => 3,
            CheckId::Ipv6Reachability => 4,
            CheckId::RouteTrace => 5,
            CheckId::Dns => 6,
            CheckId::HttpTiming => 7,
        }
    }

    /// Name as used on the command line and in the "Checks run" advice: `local-network`.
    pub fn name(self) -> &'static str {
        match self {
            CheckId::LocalNetwork => "local-network",
            CheckId::VpnDetection => "vpn-detection",
            CheckId::Ipv4Reachability => "ipv4-reachability",
            CheckId::Ipv6Reachability => "ipv6-reachability",
            CheckId::RouteTrace => "route-trace",
            CheckId::Dns => "dns",
            CheckId::HttpTiming => "http-timing",
        }
    }

    /// Key under `checks` in the run record: `local_network`.
    pub fn key(self) -> &'static str {
        match self {
            CheckId::LocalNetwork => "local_network",
            CheckId::VpnDetection => "vpn_detection",
            CheckId::Ipv4Reachability => "ipv4_reachability",
            CheckId::Ipv6Reachability => "ipv6_reachability",
            CheckId::RouteTrace => "route_trace",
            CheckId::Dns => "dns",
            CheckId::HttpTiming => "http_timing",
        }
    }

    /// File-stem style name: `01-local-network`.
    pub fn stem(self) -> String {
        format!("{:02}-{}", self.number(), self.name())
    }

    pub fn title(self) -> &'static str {
        match self {
            CheckId::LocalNetwork => "01 Local network: router / gateway",
            CheckId::VpnDetection => "02 VPN detection: tunnels and DNS routing",
            CheckId::Ipv4Reachability => "03 IPv4 reachability: ping and HTTPS",
            CheckId::Ipv6Reachability => "04 IPv6 reachability: ping6 and HTTPS",
            CheckId::RouteTrace => "05 Route trace: where does IPv4 traffic stop?",
            CheckId::Dns => "06 DNS: system resolver and each server",
            CheckId::HttpTiming => "07 HTTP timing: which phase is slow?",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            CheckId::LocalNetwork => {
                "Default gateway reachability and latency (is the problem inside your home?)"
            }
            CheckId::VpnDetection => {
                "VPN tunnels (utun) and whether a VPN is acting as the primary DNS resolver"
            }
            CheckId::Ipv4Reachability => {
                "IPv4 ICMP ping to public resolvers and HTTPS over IPv4 (is IPv4 working?)"
            }
            CheckId::Ipv6Reachability => {
                "IPv6 ICMP ping and HTTPS over IPv6 (skipped if there is no global IPv6 address)"
            }
            CheckId::RouteTrace => "traceroute to 1.1.1.1 (where along the path does traffic stop?)",
            CheckId::Dns => {
                "DNS timing through the system resolver, then each configured and public server directly"
            }
            CheckId::HttpTiming => {
                "HTTPS timing breakdown (dns, connect, tls, first byte) with the default IP stack"
            }
        }
    }

    /// Select a check by name (`dns`), stem (`06-dns`), number (`06`, `6`) or record key (`http_timing`).
    pub fn parse(s: &str) -> Option<CheckId> {
        let s = s.trim();
        CheckId::ALL.into_iter().find(|c| {
            s == c.name()
                || s == c.stem()
                || s == c.key()
                || s == format!("{:02}", c.number())
                || s == c.number().to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_rule_matches_bash_nt_end() {
        let mut o = Out::new();
        o.info("x");
        assert_eq!(o.status(), Status::Info);
        o.skip("x");
        assert_eq!(o.status(), Status::Skip);
        o.pass("x");
        assert_eq!(
            o.status(),
            Status::Pass,
            "INFO and SKIP do not downgrade PASS"
        );
        o.warn("x");
        assert_eq!(o.status(), Status::Warn);
        o.fail("x");
        assert_eq!(o.status(), Status::Fail);
    }

    #[test]
    fn parse_check_selectors() {
        assert_eq!(CheckId::parse("dns"), Some(CheckId::Dns));
        assert_eq!(CheckId::parse("06-dns"), Some(CheckId::Dns));
        assert_eq!(CheckId::parse("06"), Some(CheckId::Dns));
        assert_eq!(CheckId::parse("7"), Some(CheckId::HttpTiming));
        assert_eq!(CheckId::parse("http_timing"), Some(CheckId::HttpTiming));
        assert_eq!(CheckId::parse("nope"), None);
    }

    #[test]
    fn stems_match_bash_file_names() {
        let stems: Vec<_> = CheckId::ALL.iter().map(|c| c.stem()).collect();
        assert_eq!(stems[0], "01-local-network");
        assert_eq!(stems[6], "07-http-timing");
    }
}
