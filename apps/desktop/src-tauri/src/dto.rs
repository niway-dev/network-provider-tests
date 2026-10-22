//! What crosses the Tauri boundary. The renderer never sees core types it
//! cannot deserialize from plain JSON; `RunRecord` is the schema-1 record as is.

use nettest_core::diagnosis::Finding;
use nettest_core::model::{CheckId, Line, LineKind, Status};
use nettest_core::record::RunRecord;
use nettest_core::runner::CheckReport;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct LineDto {
    /// `status`, `hint` or `dim`.
    pub kind: &'static str,
    /// Set when `kind` is `status`.
    pub status: Option<Status>,
    pub text: String,
}

impl From<&Line> for LineDto {
    fn from(l: &Line) -> Self {
        match l.kind {
            LineKind::Status(s) => LineDto {
                kind: "status",
                status: Some(s),
                text: l.text.clone(),
            },
            LineKind::Hint => LineDto {
                kind: "hint",
                status: None,
                text: l.text.clone(),
            },
            LineKind::Dim => LineDto {
                kind: "dim",
                status: None,
                text: l.text.clone(),
            },
        }
    }
}

#[derive(Clone, Serialize)]
pub struct CheckInfo {
    /// Key under `checks` in the run record: `local_network`.
    pub key: &'static str,
    pub number: u8,
    /// Command-line name: `local-network`.
    pub name: &'static str,
    pub title: &'static str,
}

impl From<CheckId> for CheckInfo {
    fn from(id: CheckId) -> Self {
        CheckInfo {
            key: id.key(),
            number: id.number(),
            name: id.name(),
            title: id.title(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct ReportDto {
    #[serde(flatten)]
    pub check: CheckInfo,
    pub status: Status,
    pub lines: Vec<LineDto>,
}

impl From<&CheckReport> for ReportDto {
    fn from(r: &CheckReport) -> Self {
        ReportDto {
            check: r.id.into(),
            status: r.status,
            lines: r.lines.iter().map(LineDto::from).collect(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct FindingDto {
    pub code: String,
    pub summary: String,
    /// Extra lines the CLI prints under the summary; empty for stored runs.
    pub advice: Vec<String>,
}

impl From<&Finding> for FindingDto {
    fn from(f: &Finding) -> Self {
        FindingDto {
            code: f.code.to_string(),
            summary: f.summary.clone(),
            advice: f.advice.clone(),
        }
    }
}

/// One run as the window shows it.
#[derive(Clone, Serialize)]
pub struct RunView {
    pub record: RunRecord,
    pub findings: Vec<FindingDto>,
    /// The lines each check printed. Only known for runs made by this process:
    /// `runs.jsonl` stores numbers, not text.
    pub reports: Option<Vec<ReportDto>>,
    /// Set when the run happened but appending it to `runs.jsonl` failed.
    pub save_error: Option<String>,
}

impl RunView {
    /// A run read back from `runs.jsonl`: the stored diagnosis, no advice, no lines.
    pub fn stored(record: RunRecord) -> RunView {
        let findings = record
            .diagnosis
            .iter()
            .map(|d| FindingDto {
                code: d.code.clone(),
                summary: d.summary.clone(),
                advice: vec![],
            })
            .collect();
        RunView {
            record,
            findings,
            reports: None,
            save_error: None,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct HistoryDto {
    /// Oldest first, at most `limit`.
    pub runs: Vec<RunRecord>,
    /// Lines in `runs.jsonl` that did not parse.
    pub skipped: usize,
    pub file: String,
}

#[derive(Clone, Serialize)]
pub struct AppStatus {
    pub running: bool,
    pub watch_minutes: Option<u32>,
    pub data_file: String,
}
