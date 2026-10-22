//! Re-diagnose records the bash tool wrote: the Rust rules must reach the same
//! codes and summaries from the same stored data. `run-bash-real.jsonl` holds
//! three real runs taken during an ISP IPv4 outage (addresses anonymized);
//! `run-bash-sample.jsonl` holds a healthy and a degraded run.

use nettest_core::diagnosis::diagnose;
use nettest_core::record::RunRecord;

fn check(file: &str, text: &str) {
    let mut n = 0;
    for line in text.lines() {
        let rec = RunRecord::from_json_line(line).unwrap();
        let got: Vec<(String, String)> = diagnose(&rec)
            .iter()
            .map(|f| (f.code.to_string(), f.summary.clone()))
            .collect();
        let stored: Vec<(String, String)> = rec
            .diagnosis
            .iter()
            .map(|d| (d.code.clone(), d.summary.clone()))
            .collect();
        assert_eq!(got, stored, "{file}, line {}", n + 1);
        n += 1;
    }
    assert!(n > 0, "{file} is empty");
}

#[test]
fn rust_rules_reproduce_the_bash_diagnosis() {
    check(
        "run-bash-real.jsonl",
        include_str!("../../../tests/fixtures/run-bash-real.jsonl"),
    );
    check(
        "run-bash-sample.jsonl",
        include_str!("../../../tests/fixtures/run-bash-sample.jsonl"),
    );
}
