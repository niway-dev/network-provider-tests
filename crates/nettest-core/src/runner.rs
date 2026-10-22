//! Runs checks in order, wraps each in a timeout, and builds the run record.

use std::time::{Duration, Instant};

use crate::checks;
use crate::diagnosis::{diagnose, Finding};
use crate::model::{CheckId, Line, LineKind, Status};
use crate::probes::Probes;
use crate::record::{self, Checks, Metrics, Mode, RunRecord};

/// A check that has not finished after this long is recorded as FAIL.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(60);

/// What one check printed and how it ended.
#[derive(Clone, Debug)]
pub struct CheckReport {
    pub id: CheckId,
    pub lines: Vec<Line>,
    pub status: Status,
}

pub struct RunOutput {
    pub record: RunRecord,
    pub reports: Vec<CheckReport>,
    pub findings: Vec<Finding>,
}

pub async fn run(
    probes: &dyn Probes,
    ids: &[CheckId],
    mode: Mode,
    on_check: &mut dyn FnMut(&CheckReport),
) -> RunOutput {
    run_with_timeout(probes, ids, mode, CHECK_TIMEOUT, on_check).await
}

pub async fn run_with_timeout(
    probes: &dyn Probes,
    ids: &[CheckId],
    mode: Mode,
    timeout: Duration,
    on_check: &mut dyn FnMut(&CheckReport),
) -> RunOutput {
    let started = Instant::now();
    let mut checks_rec = Checks::default();
    let mut reports = Vec::new();
    let mut any_fail = false;

    for &id in ids {
        let (lines, metrics) = match tokio::time::timeout(timeout, checks::run(id, probes)).await {
            Ok(r) => (r.lines, r.metrics),
            Err(_) => {
                let line = Line {
                    kind: LineKind::Status(Status::Fail),
                    text: format!(
                        "check {} did not finish within {} s and was stopped",
                        id.stem(),
                        timeout.as_secs()
                    ),
                };
                (vec![line], Metrics::failed(id))
            }
        };
        let status = metrics.status();
        any_fail |= status == Status::Fail;
        checks_rec.set(metrics);
        let report = CheckReport { id, lines, status };
        on_check(&report);
        reports.push(report);
    }

    let mut record = RunRecord {
        schema: record::SCHEMA,
        ts: String::new(),
        tool: record::TOOL.to_string(),
        tool_version: crate::version().to_string(),
        os: std::env::consts::OS.to_string(),
        mode,
        duration_ms: started.elapsed().as_millis() as u64,
        exit: i32::from(any_fail),
        checks: checks_rec,
        diagnosis: vec![],
    };
    let findings = diagnose(&record);
    record.diagnosis = findings.iter().map(Finding::to_entry).collect();
    record.ts = record::iso_now();

    RunOutput {
        record,
        reports,
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probes::fake::FakeProbes;
    use crate::probes::{
        DefaultRoute, DnsOutcome, Family, HttpResult, PingStats, ResolverEntry, SystemDns, Trace,
        Tunnel,
    };
    use async_trait::async_trait;

    #[tokio::test]
    async fn full_run_builds_a_record() {
        let p = FakeProbes::new();
        let mut seen = Vec::new();
        let out = run(&p, &CheckId::ALL, Mode::Full, &mut |r| seen.push(r.id)).await;
        assert_eq!(seen, CheckId::ALL);
        assert_eq!(out.reports.len(), 7);
        let r = &out.record;
        assert_eq!(r.schema, 1);
        assert_eq!(r.tool, "nettest-rs");
        assert_eq!(r.mode, Mode::Full);
        assert_eq!(r.exit, 0);
        assert_eq!(r.checks.statuses().len(), 7);
        assert_eq!(r.diagnosis[0].code, "no_problem");
        assert!(r.ts.contains('T'));
        // The line parses back as a record.
        assert!(RunRecord::from_json_line(&r.to_json_line()).is_ok());
    }

    #[tokio::test]
    async fn single_run_records_one_check_and_diagnoses_what_is_known() {
        let mut p = FakeProbes::new();
        p.default_https = Err("timed out after 6s".into());
        let out = run(&p, &[CheckId::Ipv4Reachability], Mode::Single, &mut |_| {}).await;
        let r = &out.record;
        assert_eq!(r.mode, Mode::Single);
        assert_eq!(r.checks.statuses().len(), 1);
        assert_eq!(r.exit, 1);
        assert_eq!(r.diagnosis[0].code, "ipv4_outage_unverified");
    }

    /// Delegates to the fake except for a ping that never returns.
    struct HangingPing(FakeProbes);

    #[async_trait]
    impl Probes for HangingPing {
        async fn ping(&self, _t: &str, _c: u32, _to: Duration) -> PingStats {
            std::future::pending().await
        }
        async fn https(&self, u: &str, f: Family, t: Duration) -> HttpResult {
            self.0.https(u, f, t).await
        }
        async fn dns_query(&self, s: &str, n: &str, t: Duration) -> DnsOutcome {
            self.0.dns_query(s, n, t).await
        }
        async fn dns_system(&self, n: &str, t: Duration) -> SystemDns {
            self.0.dns_system(n, t).await
        }
        async fn traceroute(&self, t: &str, m: u8) -> Trace {
            self.0.traceroute(t, m).await
        }
        fn default_route(&self) -> Option<DefaultRoute> {
            self.0.default_route()
        }
        fn tunnels_with_ipv4(&self) -> Vec<Tunnel> {
            self.0.tunnels_with_ipv4()
        }
        fn resolvers(&self) -> Vec<ResolverEntry> {
            self.0.resolvers()
        }
        fn route_interface(&self, ip: &str) -> Option<String> {
            self.0.route_interface(ip)
        }
        fn has_global_ipv6(&self) -> bool {
            self.0.has_global_ipv6()
        }
    }

    #[tokio::test]
    async fn a_hanging_check_times_out_and_is_recorded_as_fail() {
        let p = HangingPing(FakeProbes::new());
        let ids = [CheckId::LocalNetwork, CheckId::RouteTrace];
        let out =
            run_with_timeout(&p, &ids, Mode::Full, Duration::from_millis(50), &mut |_| {}).await;
        assert_eq!(out.record.exit, 1);
        assert_eq!(out.reports[0].status, Status::Fail);
        assert!(out.reports[0].lines[0]
            .text
            .contains("did not finish within"));
        assert_eq!(
            out.record.checks.local_network.as_ref().unwrap().status,
            Status::Fail
        );
        // The run continues with the next check.
        assert_eq!(out.reports[1].status, Status::Pass);
    }
}
