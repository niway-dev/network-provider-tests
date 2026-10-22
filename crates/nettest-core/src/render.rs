//! Text renderer: the bash layout (`  PASS  text`, indented hints, summary,
//! diagnosis). Colors are off when `NO_COLOR` is set or stdout is not a tty.

use std::io::IsTerminal;

use crate::diagnosis::Finding;
use crate::model::{CheckId, Line, LineKind, Status};
use crate::runner::CheckReport;

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub color: bool,
}

const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const GRAY: &str = "\x1b[90m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

impl Style {
    pub const PLAIN: Style = Style { color: false };

    /// Colors when stdout is a terminal and `NO_COLOR` is unset or empty.
    pub fn detect() -> Style {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Style {
            color: !no_color && std::io::stdout().is_terminal(),
        }
    }

    fn c(self, code: &'static str) -> &'static str {
        if self.color {
            code
        } else {
            ""
        }
    }

    pub fn red(self) -> &'static str {
        self.c(RED)
    }
    pub fn yellow(self) -> &'static str {
        self.c(YELLOW)
    }
    pub fn green(self) -> &'static str {
        self.c(GREEN)
    }
    pub fn bold(self) -> &'static str {
        self.c(BOLD)
    }
    pub fn reset(self) -> &'static str {
        self.c(RESET)
    }

    fn status_color(self, s: Status) -> &'static str {
        match s {
            Status::Pass => self.c(GREEN),
            Status::Info => self.c(BLUE),
            Status::Skip => self.c(GRAY),
            Status::Warn => self.c(YELLOW),
            Status::Fail => self.c(RED),
        }
    }
}

pub fn render_line(style: Style, line: &Line) -> String {
    let reset = style.reset();
    match line.kind {
        LineKind::Status(s) => format!(
            "  {}{:<4}{}  {}\n",
            style.status_color(s),
            s.tag(),
            reset,
            line.text
        ),
        LineKind::Hint => format!("        {}-> {}{}\n", style.c(GRAY), line.text, reset),
        LineKind::Dim => format!("        {}{}{}\n", style.c(GRAY), line.text, reset),
    }
}

/// Header plus lines of one check, preceded by a blank line.
pub fn render_check(style: Style, report: &CheckReport) -> String {
    let mut s = format!(
        "\n{}== {} =={}\n",
        style.bold(),
        report.id.title(),
        style.reset()
    );
    for l in &report.lines {
        s.push_str(&render_line(style, l));
    }
    s
}

/// `nettest  <date>  on <host>` banner.
pub fn render_banner(style: Style, date: &str, host: &str) -> String {
    format!(
        "{}nettest{}  {date}  on {host}\n",
        style.bold(),
        style.reset()
    )
}

/// The `== Summary ==` block: one line per check that ran.
pub fn render_summary(style: Style, reports: &[CheckReport]) -> String {
    let mut s = format!("\n{}== Summary =={}\n", style.bold(), style.reset());
    for r in reports {
        let col = match r.status {
            Status::Pass | Status::Warn | Status::Fail => style.status_color(r.status),
            _ => style.c(GRAY),
        };
        s.push_str(&format!(
            "  {}{:<4}{}  {}\n",
            col,
            r.status.tag(),
            style.reset(),
            r.id.stem()
        ));
    }
    s
}

pub fn render_diagnosis(style: Style, findings: &[Finding]) -> String {
    let mut s = format!("\n{}== Diagnosis =={}\n", style.bold(), style.reset());
    for f in findings {
        s.push_str(&format!(
            "  {b}*{r} {b}{}{r}\n",
            f.summary,
            b = style.bold(),
            r = style.reset()
        ));
        for a in &f.advice {
            s.push_str(&format!("      {a}\n"));
        }
    }
    s
}

/// Everything after the per-check output: summary and diagnosis.
pub fn render_footer(style: Style, reports: &[CheckReport], findings: &[Finding]) -> String {
    format!(
        "{}{}",
        render_summary(style, reports),
        render_diagnosis(style, findings)
    )
}

/// A whole run as text.
pub fn render_run(
    style: Style,
    banner: &str,
    reports: &[CheckReport],
    findings: &[Finding],
) -> String {
    let mut s = String::new();
    s.push_str(banner);
    for r in reports {
        s.push_str(&render_check(style, r));
    }
    s.push_str(&render_footer(style, reports, findings));
    s
}

/// Lines for `--list`: `01-local-network         Default gateway ...`.
pub fn render_list() -> String {
    CheckId::ALL
        .iter()
        .map(|c| format!("{:<22} {}\n", c.stem(), c.description()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probes::fake::FakeProbes;
    use crate::record::Mode;
    use crate::runner::run;

    #[test]
    fn list_has_seven_checks() {
        let l = render_list();
        assert_eq!(l.lines().count(), 7);
        assert!(l.starts_with("01-local-network       Default gateway"));
    }

    #[test]
    fn colors_wrap_the_tag_only() {
        let line = Line {
            kind: LineKind::Status(Status::Fail),
            text: "boom".into(),
        };
        let s = render_line(Style { color: true }, &line);
        assert_eq!(s, "  \x1b[31mFAIL\x1b[0m  boom\n");
        assert_eq!(render_line(Style::PLAIN, &line), "  FAIL  boom\n");
    }

    #[test]
    fn hints_and_notes_are_indented() {
        let hint = Line {
            kind: LineKind::Hint,
            text: "do this".into(),
        };
        let dim = Line {
            kind: LineKind::Dim,
            text: "note".into(),
        };
        assert_eq!(render_line(Style::PLAIN, &hint), "        -> do this\n");
        assert_eq!(render_line(Style::PLAIN, &dim), "        note\n");
    }

    /// Snapshot of a fake healthy run. Regenerate with `UPDATE_SNAPSHOTS=1 cargo test`.
    #[tokio::test]
    async fn full_run_snapshot() {
        let mut p = FakeProbes::new();
        p.route_ifs.insert("192.0.2.1".into(), "en0".into());
        let mut reports = Vec::new();
        let out = run(&p, &CheckId::ALL, Mode::Full, &mut |r| {
            reports.push(r.clone())
        })
        .await;
        let text = render_run(
            Style::PLAIN,
            &render_banner(Style::PLAIN, "2026-10-22 11:00:00 -0500", "test-host"),
            &reports,
            &out.findings,
        );
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/full-run.txt");
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(path, &text).unwrap();
        }
        let want = std::fs::read_to_string(path).expect("snapshot file");
        assert_eq!(text, want);
    }
}
