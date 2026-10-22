//! Re-diagnose records the bash tool wrote. `run-bash-sample.jsonl` (a healthy
//! and a degraded run) must reach the stored codes and summaries unchanged.
//! `run-bash-real.jsonl` holds three real runs taken during an ISP IPv4 outage
//! (addresses anonymized). They were recorded by bash 0.2.0, whose rules called
//! the partial-failure run 1 harmless ping loss. The expected values below are
//! the corrected diagnosis (bash 0.2.1 / Rust 0.1.1), not the stored ones.

use nettest_core::diagnosis::diagnose;
use nettest_core::record::RunRecord;

fn diagnosed(line: &str) -> Vec<(String, String)> {
    let rec = RunRecord::from_json_line(line).unwrap();
    diagnose(&rec)
        .iter()
        .map(|f| (f.code.to_string(), f.summary.clone()))
        .collect()
}

#[test]
fn rust_rules_reproduce_the_bash_diagnosis_on_the_sample() {
    let text = include_str!("../../../tests/fixtures/run-bash-sample.jsonl");
    let mut n = 0;
    for line in text.lines() {
        let rec = RunRecord::from_json_line(line).unwrap();
        let stored: Vec<(String, String)> = rec
            .diagnosis
            .iter()
            .map(|d| (d.code.clone(), d.summary.clone()))
            .collect();
        assert_eq!(diagnosed(line), stored, "sample, line {}", n + 1);
        n += 1;
    }
    assert!(n > 0, "sample is empty");
}

#[test]
fn real_outage_runs_get_the_corrected_diagnosis() {
    let text = include_str!("../../../tests/fixtures/run-bash-real.jsonl");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);

    // Line 1: 2 of 4 IPv4 HTTPS timed out, ping 100% lost, check 07 failed.
    // Stored (old): ipv4_degraded + icmp_deprioritized "HTTPS works".
    assert_eq!(
        diagnosed(lines[0]),
        [(
            "ipv4_degraded".to_string(),
            "Degraded IPv4 path: 2 of 4 IPv4 HTTPS probes timed out; github.com did not connect (check 07); public DNS servers timed out (1.1.1.1, 8.8.8.8).".to_string()
        )]
    );

    // Line 2: IPv4 HTTPS fully OK, only two public resolvers time out. Unchanged.
    assert_eq!(
        diagnosed(lines[1]),
        [(
            "ipv4_degraded".to_string(),
            "Degraded IPv4 path: public DNS servers timed out (8.8.8.8, 9.9.9.9).".to_string()
        )]
    );

    // Line 3: 0 of 4 IPv4 HTTPS. Unchanged.
    assert_eq!(
        diagnosed(lines[2]),
        [(
            "ipv4_outage".to_string(),
            "ISP IPv4 outage (often their CGNAT): sites with IPv6 load, IPv4-only sites don't."
                .to_string()
        )]
    );
}
