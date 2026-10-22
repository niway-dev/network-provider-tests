//! `nettest`: diagnose home internet / ISP problems.
//!
//! Exit status: 0 if no check reported FAIL, 1 otherwise, 2 on usage errors.

use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use chrono::{Local, Utc};
use clap::{Parser, Subcommand};
use nettest_core::diagnosis::Finding;
use nettest_core::history::{parse_duration, render_table, worst_status};
use nettest_core::model::{CheckId, Status};
use nettest_core::probes::os::SystemProbes;
use nettest_core::probes::Probes;
use nettest_core::record::{Mode, RunRecord};
use nettest_core::render::{self, Style};
use nettest_core::runner::{self, CheckReport};
use nettest_core::store::Store;

/// Diagnose home internet / ISP problems.
///
/// Every run appends one JSON line to $NETTEST_DATA_DIR/runs.jsonl (default
/// ~/Library/Logs/network-provider-tests on macOS). Set NO_COLOR=1 to disable colors.
#[derive(Parser)]
#[command(name = "nettest", version, args_conflicts_with_subcommands = true)]
struct Cli {
    /// Run one check: by name ("dns"), file stem ("06-dns") or number ("06")
    check: Option<String>,

    /// List the checks
    #[arg(short, long)]
    list: bool,

    /// Print the run record as JSON instead of the text report
    #[arg(long)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Table of the last N recorded runs (default 20)
    History {
        /// Show the last N runs
        n: Option<usize>,
        /// Same as the positional N
        #[arg(short = 'n', long = "last", value_name = "N")]
        last: Option<usize>,
        /// Only runs newer than DURATION: 30m, 2h, 1d, 1w
        #[arg(long, value_name = "DURATION")]
        since: Option<String>,
        /// Print the stored JSON lines instead of the table
        #[arg(long)]
        json: bool,
    },
    /// Run the full suite every N minutes (default 5) until Ctrl-C
    Watch {
        /// Minutes between runs
        minutes: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let store = Store::from_env();
    match cli.command {
        Some(Command::History {
            n,
            last,
            since,
            json,
        }) => history(&store, last.or(n).unwrap_or(20), since, json),
        Some(Command::Watch { minutes }) => watch(&store, minutes.as_deref()).await,
        None => {
            if cli.list {
                print!("{}", render::render_list());
                return ExitCode::SUCCESS;
            }
            let ids = match cli.check.as_deref() {
                None => CheckId::ALL.to_vec(),
                Some(name) => match CheckId::parse(name) {
                    Some(id) => vec![id],
                    None => {
                        eprintln!("nettest: no such check '{name}'. Available checks:");
                        eprint!("{}", render::render_list());
                        return ExitCode::from(2);
                    }
                },
            };
            run_once(&store, &ids, cli.json).await
        }
    }
}

fn computer_name() -> String {
    let get = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    get("scutil", &["--get", "ComputerName"])
        .or_else(|| get("hostname", &[]))
        .unwrap_or_else(|| "this machine".into())
}

fn save(store: &Store, record: &RunRecord) {
    if store.append(record).is_err() {
        eprintln!(
            "nettest: could not write the run record to {}",
            store.file().display()
        );
    }
}

/// Run `ids`, optionally printing the report as it goes; returns the output.
async fn execute(
    ids: &[CheckId],
    style: Style,
    print: bool,
) -> (RunRecord, Vec<CheckReport>, Vec<Finding>) {
    let probes = SystemProbes;
    let mode = if ids.len() == CheckId::ALL.len() {
        Mode::Full
    } else {
        Mode::Single
    };
    if print {
        let date = Local::now().format("%Y-%m-%d %H:%M:%S %z").to_string();
        print!("{}", render::render_banner(style, &date, &computer_name()));
        let _ = std::io::stdout().flush();
    }
    let mut on_check = |r: &CheckReport| {
        if print {
            print!("{}", render::render_check(style, r));
            let _ = std::io::stdout().flush();
        }
    };
    let out = runner::run(&probes as &dyn Probes, ids, mode, &mut on_check).await;
    (out.record, out.reports, out.findings)
}

async fn run_once(store: &Store, ids: &[CheckId], json: bool) -> ExitCode {
    let style = Style::detect();
    let (record, reports, findings) = execute(ids, style, !json).await;
    if json {
        println!("{}", record.to_json_line());
    } else {
        print!("{}", render::render_footer(style, &reports, &findings));
    }
    save(store, &record);
    ExitCode::from(u8::from(record.exit != 0))
}

fn history(store: &Store, n: usize, since: Option<String>, json: bool) -> ExitCode {
    let since = match since.as_deref() {
        None => None,
        Some(v) => match parse_duration(v) {
            Some(d) => Some(d),
            None => {
                eprintln!("nettest history: bad --since value '{v}' (use 30m, 2h, 1d, 1w)");
                return ExitCode::from(2);
            }
        },
    };
    let file = store.file();
    let read = match store.read_all() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("nettest history: could not read {}: {e}", file.display());
            return ExitCode::FAILURE;
        }
    };
    if read.runs.is_empty() && read.skipped == 0 {
        println!(
            "No runs recorded yet in {}. Run nettest first.",
            file.display()
        );
        return ExitCode::SUCCESS;
    }
    let now = Utc::now();
    if json {
        let cutoff = since.map(|s| now - chrono::Duration::seconds(s.as_secs() as i64));
        let keep: Vec<&RunRecord> = read
            .runs
            .iter()
            .filter(
                |r| match (cutoff, chrono::DateTime::parse_from_rfc3339(&r.ts)) {
                    (Some(c), Ok(ts)) => ts.with_timezone(&Utc) >= c,
                    _ => true,
                },
            )
            .collect();
        for r in &keep[keep.len().saturating_sub(n)..] {
            println!("{}", r.to_json_line());
        }
    } else {
        let color = Style::detect().color;
        print!("{}", render_table(&read.runs, n, since, now, color, &Local));
    }
    if read.skipped > 0 {
        eprintln!(
            "nettest history: skipped {} line(s) that did not parse",
            read.skipped
        );
    }
    ExitCode::SUCCESS
}

async fn watch(store: &Store, minutes: Option<&str>) -> ExitCode {
    let minutes_txt = minutes.unwrap_or("5");
    let mins: f64 = match minutes_txt.parse() {
        Ok(m) if f64::is_finite(m) && m > 0.0 => m,
        Ok(_) => {
            eprintln!("nettest watch: minutes must be greater than 0");
            return ExitCode::from(2);
        }
        Err(_) => {
            eprintln!("nettest watch: minutes must be a positive number, got '{minutes_txt}'");
            return ExitCode::from(2);
        }
    };
    let secs = ((mins * 60.0) as u64).max(1);
    let style = Style::detect();
    println!(
        "{}nettest watch{}: a full run every {minutes_txt} min, recorded in {}. Ctrl-C to stop.",
        style.bold(),
        style.reset(),
        store.file().display()
    );

    let mut runs = 0u32;
    let stop = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    tokio::pin!(stop);
    loop {
        let cycle = async {
            let (record, _, _) = execute(&CheckId::ALL, Style::PLAIN, false).await;
            record
        };
        let record = tokio::select! {
            r = cycle => r,
            _ = &mut stop => break,
        };
        save(store, &record);
        runs += 1;
        let res = worst_status(&record);
        let col = match res {
            Status::Fail => style.red(),
            Status::Warn => style.yellow(),
            _ => style.green(),
        };
        let codes: Vec<&str> = record.diagnosis.iter().map(|d| d.code.as_str()).collect();
        println!(
            "{}  {col}{:<4}{}  {}  ({} s)",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            res.tag(),
            style.reset(),
            if codes.is_empty() {
                "-".to_string()
            } else {
                codes.join(",")
            },
            record.duration_ms / 1000
        );
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
            _ = &mut stop => break,
        }
    }
    println!(
        "\nnettest watch: stopped after {runs} run(s). Runs are in {}",
        store.file().display()
    );
    ExitCode::SUCCESS
}
