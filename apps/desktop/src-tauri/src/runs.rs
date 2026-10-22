//! Runs the checks without blocking the UI: the work happens on a blocking
//! thread, each finished check is announced with an event, and the record is
//! appended to the same `runs.jsonl` the CLI writes.

use std::time::Instant;

use nettest_core::model::CheckId;
use nettest_core::probes::os::SystemProbes;
use nettest_core::probes::Probes;
use nettest_core::record::Mode;
use nettest_core::runner::{self, CheckReport};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::dto::{CheckInfo, FindingDto, ReportDto, RunView};
use crate::state::AppState;
use crate::tray;

#[derive(Clone, Serialize)]
struct RunStarted {
    mode: Mode,
    checks: Vec<CheckInfo>,
}

/// Clears the "running" flag when the run ends, however it ends.
struct RunGuard(AppHandle);

impl Drop for RunGuard {
    fn drop(&mut self) {
        *self.0.state::<AppState>().running.lock().unwrap() = None;
    }
}

pub fn is_running(app: &AppHandle) -> bool {
    app.state::<AppState>().running.lock().unwrap().is_some()
}

/// Run `ids`, record the result, and update the tray and the window.
/// Returns `Err("busy")` if another run is in progress.
///
/// The run is its own task: stopping "Watch" (which aborts the timer task
/// that called this) never cancels a run that is already going, so the run
/// is still recorded and the "running" flag stays accurate until it ends.
pub async fn execute(app: &AppHandle, ids: Vec<CheckId>) -> Result<RunView, String> {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run_and_record(&app, ids).await })
        .await
        .map_err(|e| format!("the run stopped unexpectedly: {e}"))?
}

async fn run_and_record(app: &AppHandle, ids: Vec<CheckId>) -> Result<RunView, String> {
    let state = app.state::<AppState>();
    {
        let mut running = state.running.lock().unwrap();
        if running.is_some() {
            return Err("busy".into());
        }
        *running = Some(Instant::now());
    }
    let _guard = RunGuard(app.clone());

    let mode = if ids.len() == CheckId::ALL.len() {
        Mode::Full
    } else {
        Mode::Single
    };
    tray::set_running(app, true);
    let _ = app.emit(
        "run-started",
        RunStarted {
            mode,
            checks: ids.iter().map(|id| CheckInfo::from(*id)).collect(),
        },
    );

    let progress = app.clone();
    let handle = tokio::runtime::Handle::current();
    let out = tauri::async_runtime::spawn_blocking(move || {
        // The runner's progress callback is not `Send`, so the run stays on
        // this one blocking thread instead of being spawned across workers.
        handle.block_on(async move {
            let mut on_check = |r: &CheckReport| {
                let _ = progress.emit("check-finished", ReportDto::from(r));
            };
            runner::run(&SystemProbes as &dyn Probes, &ids, mode, &mut on_check).await
        })
    })
    .await
    .map_err(|e| format!("the run stopped unexpectedly: {e}"))?;

    let save_error = state
        .store
        .append(&out.record)
        .err()
        .map(|e| format!("could not write {}: {e}", state.store.file().display()));
    let view = RunView {
        record: out.record,
        findings: out.findings.iter().map(FindingDto::from).collect(),
        reports: Some(out.reports.iter().map(ReportDto::from).collect()),
        save_error,
    };
    *state.last.lock().unwrap() = Some(view.clone());
    tray::set_running(app, false);
    tray::show_last(app, &view.record);
    let _ = app.emit("run-finished", &view);
    Ok(view)
}

/// Fire and forget, for the tray menu and the watch timer.
pub fn spawn_full_run(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // "busy" just means a run is already going; nothing to do.
        let _ = execute(&app, CheckId::ALL.to_vec()).await;
    });
}
