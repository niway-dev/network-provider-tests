//! `nettest history`: recent runs as a table. Same columns and rules as the
//! bash tool's python engine, so both tools print the same rows.

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};

use crate::model::Status;
use crate::record::RunRecord;

/// `30s`, `30m`, `2h`, `1d`, `1w`.
pub fn parse_duration(v: &str) -> Option<Duration> {
    let unit = v.chars().last()?;
    let n: u64 = v[..v.len() - unit.len_utf8()].parse().ok()?;
    let secs = match unit {
        's' => n,
        'm' => n * 60,
        'h' => n * 3600,
        'd' => n * 86400,
        'w' => n * 604800,
        _ => return None,
    };
    Some(Duration::from_secs(secs))
}

fn result_of(r: &RunRecord) -> Status {
    let statuses = r.checks.statuses();
    if r.exit != 0 || statuses.iter().any(|(_, s)| *s == Status::Fail) {
        Status::Fail
    } else if statuses.iter().any(|(_, s)| *s == Status::Warn) {
        Status::Warn
    } else {
        Status::Pass
    }
}

fn ratio(block: Option<(Option<u32>, Option<u32>)>) -> String {
    match block {
        Some((Some(ok), Some(tot))) => format!("{ok}/{tot}"),
        _ => "-".into(),
    }
}

/// Worst check status of a run: FAIL, WARN or PASS (used by `watch`).
pub fn worst_status(r: &RunRecord) -> Status {
    result_of(r)
}

const HEADER: (&str, &str, &str, &str, &str, &str, &str) = (
    "TIME",
    "RESULT",
    "GW_MS",
    "V4_HTTPS",
    "V6_HTTPS",
    "DNS_MS",
    "DIAGNOSIS",
);

fn row(cols: (&str, &str, &str, &str, &str, &str, &str)) -> String {
    format!(
        "{:<19}  {:<6}  {:>6}  {:>8}  {:>8}  {:>6}  {}",
        cols.0, cols.1, cols.2, cols.3, cols.4, cols.5, cols.6
    )
}

/// The last `n` runs (newer than `since`, if given) as a table, one line per run.
pub fn render_table<Tz: TimeZone>(
    runs: &[RunRecord],
    n: usize,
    since: Option<Duration>,
    now: DateTime<Utc>,
    color: bool,
    tz: &Tz,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut rows: Vec<(Status, String)> = Vec::new();
    for r in runs {
        let Ok(ts) = DateTime::parse_from_rfc3339(&r.ts) else {
            continue;
        };
        if let Some(s) = since {
            if (now - ts.with_timezone(&Utc)).num_seconds() > s.as_secs() as i64 {
                continue;
            }
        }
        let res = result_of(r);
        let gw = r
            .checks
            .local_network
            .as_ref()
            .and_then(|l| l.avg_ms)
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "-".into());
        let v4 = ratio(
            r.checks
                .ipv4_reachability
                .as_ref()
                .map(|c| (c.https.ok, c.https.total)),
        );
        let v6 = ratio(
            r.checks
                .ipv6_reachability
                .as_ref()
                .map(|c| (c.https.ok, c.https.total)),
        );
        let dns = r
            .checks
            .dns
            .as_ref()
            .and_then(|d| d.system.max_ms)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".into());
        let codes: Vec<&str> = r
            .diagnosis
            .iter()
            .map(|d| d.code.as_str())
            .filter(|c| *c != "no_problem")
            .collect();
        let codes = if codes.is_empty() {
            "-".to_string()
        } else {
            codes.join(",")
        };
        let when = ts.with_timezone(tz).format("%Y-%m-%d %H:%M:%S").to_string();
        rows.push((res, row((&when, res.tag(), &gw, &v4, &v6, &dns, &codes))));
    }

    let mut out = row(HEADER);
    out.push('\n');
    let start = rows.len().saturating_sub(n);
    for (res, text) in &rows[start..] {
        match (color, res) {
            (true, Status::Fail) => out.push_str(&format!("\x1b[31m{text}\x1b[0m")),
            (true, Status::Warn) => out.push_str(&format!("\x1b[33m{text}\x1b[0m")),
            _ => out.push_str(text),
        }
        out.push('\n');
    }
    if rows.is_empty() {
        out.push_str("(no runs in range)\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    /// Same three records as `tests/test-history.sh` (the third at "now").
    fn runs(now: &str) -> Vec<RunRecord> {
        let lines = [
            r#"{"schema":1,"ts":"2026-10-22T09:00:00-05:00","tool":"nettest-bash","tool_version":"0.2.0","os":"macos","mode":"full","duration_ms":20000,"exit":0,"checks":{"local_network":{"status":"PASS","interface":"en0","gateway":"192.168.1.1","link_kind":"wired","reachable":true,"loss_pct":0.0,"avg_ms":0.67},"ipv4_reachability":{"status":"PASS","icmp":{"ok":3,"total":3,"targets":[]},"https":{"ok":4,"total":4,"targets":[]}},"ipv6_reachability":{"status":"PASS","global_address":true,"icmp":{"ok":2,"total":2,"targets":[]},"https":{"ok":2,"total":2,"targets":[]}},"dns":{"status":"PASS","system":{"status":"PASS","max_ms":43,"lookups":[]},"servers":[],"primary_timeouts":[]}},"diagnosis":[{"code":"no_problem","summary":"No problem detected right now."}]}"#.to_string(),
            r#"{"schema":1,"ts":"2026-10-22T10:58:03-05:00","tool":"nettest-bash","tool_version":"0.2.0","os":"macos","mode":"full","duration_ms":21034,"exit":1,"checks":{"local_network":{"status":"PASS","interface":"en0","gateway":"192.168.1.1","link_kind":"wired","reachable":true,"loss_pct":0.0,"avg_ms":6.2},"ipv4_reachability":{"status":"FAIL","icmp":{"ok":1,"total":3,"targets":[]},"https":{"ok":0,"total":4,"targets":[]}},"ipv6_reachability":{"status":"FAIL","global_address":true,"icmp":{"ok":2,"total":2,"targets":[]},"https":{"ok":0,"total":2,"targets":[]}},"dns":{"status":"WARN","system":{"status":"WARN","max_ms":5022,"lookups":[]},"servers":[],"primary_timeouts":[]}},"diagnosis":[{"code":"isp_down","summary":"x"},{"code":"ipv4_degraded","summary":"y"}]}"#.to_string(),
            format!(r#"{{"schema":1,"ts":"{now}","tool":"nettest-bash","tool_version":"0.2.0","os":"macos","mode":"single","duration_ms":812,"exit":0,"checks":{{"dns":{{"status":"WARN","system":{{"status":"WARN","max_ms":1204,"lookups":[]}},"servers":[],"primary_timeouts":[]}}}},"diagnosis":[{{"code":"system_dns_slow","summary":"z"}}]}}"#),
        ];
        lines
            .iter()
            .map(|l| RunRecord::from_json_line(l).unwrap())
            .collect()
    }

    fn tz() -> FixedOffset {
        FixedOffset::west_opt(5 * 3600).unwrap()
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-22T12:00:00-05:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn rows_match_the_bash_history_output() {
        let out = render_table(
            &runs("2026-10-22T11:59:00-05:00"),
            20,
            None,
            now(),
            false,
            &tz(),
        );
        assert!(out.contains(
            "TIME                 RESULT   GW_MS  V4_HTTPS  V6_HTTPS  DNS_MS  DIAGNOSIS"
        ));
        // Lines asserted by tests/test-history.sh.
        assert!(
            out.contains("2026-10-22 09:00:00  PASS       0.7       4/4       2/2      43  -"),
            "{out}"
        );
        assert!(out.contains("2026-10-22 10:58:03  FAIL       6.2       0/4       0/2    5022  isp_down,ipv4_degraded"), "{out}");
        assert!(
            out.contains("WARN         -         -         -    1204  system_dns_slow"),
            "{out}"
        );
    }

    #[test]
    fn n_limits_rows_and_since_filters() {
        let r = runs("2026-10-22T11:59:00-05:00");
        let one = render_table(&r, 1, None, now(), false, &tz());
        assert_eq!(one.lines().count(), 2);
        let since = render_table(&r, 20, Some(Duration::from_secs(600)), now(), false, &tz());
        assert!(since.contains("system_dns_slow"));
        assert!(!since.contains("isp_down"));
    }

    #[test]
    fn colors_fail_red_and_warn_yellow() {
        let out = render_table(
            &runs("2026-10-22T11:59:00-05:00"),
            20,
            None,
            now(),
            true,
            &tz(),
        );
        assert!(out.contains("\x1b[31m2026-10-22 10:58:03"));
        assert!(out.contains("\x1b[33m2026-10-22 11:59:00"));
        assert!(!out.contains("\x1b[31m2026-10-22 09:00:00"));
    }

    #[test]
    fn empty_range() {
        let out = render_table(&[], 20, None, now(), false, &tz());
        assert!(out.ends_with("(no runs in range)\n"));
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("30m"), Some(Duration::from_secs(1800)));
        assert_eq!(parse_duration("2h"), Some(Duration::from_secs(7200)));
        assert_eq!(parse_duration("1d"), Some(Duration::from_secs(86400)));
        assert_eq!(parse_duration("1w"), Some(Duration::from_secs(604800)));
        assert_eq!(parse_duration("5x"), None);
        assert_eq!(parse_duration("m"), None);
        assert_eq!(parse_duration(""), None);
    }
}
