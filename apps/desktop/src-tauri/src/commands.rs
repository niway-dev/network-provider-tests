//! Tauri commands. The renderer never touches the network or the file system.

use nettest_core::model::CheckId;
use tauri::{AppHandle, State};

use crate::dto::{AppStatus, HistoryDto, RunView};
use crate::runs;
use crate::state::AppState;
use crate::watch;

#[tauri::command]
pub async fn run_all(app: AppHandle) -> Result<RunView, String> {
    runs::execute(&app, CheckId::ALL.to_vec()).await
}

#[tauri::command]
pub async fn run_check(app: AppHandle, name: String) -> Result<RunView, String> {
    let id = CheckId::parse(&name).ok_or_else(|| format!("no such check '{name}'"))?;
    runs::execute(&app, vec![id]).await
}

/// The last `limit` recorded runs, oldest first.
#[tauri::command]
pub async fn history(state: State<'_, AppState>, limit: usize) -> Result<HistoryDto, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let read = store
            .read_all()
            .map_err(|e| format!("could not read {}: {e}", store.file().display()))?;
        let mut runs = read.runs;
        let cut = runs.len().saturating_sub(limit);
        runs.drain(..cut);
        Ok(HistoryDto {
            runs,
            skipped: read.skipped,
            file: store.file().display().to_string(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The run to show first: this process's last run (with its lines), else the
/// newest line of `runs.jsonl`.
#[tauri::command]
pub async fn latest(state: State<'_, AppState>) -> Result<Option<RunView>, String> {
    if let Some(v) = state.last.lock().unwrap().clone() {
        return Ok(Some(v));
    }
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(store
            .read_all()
            .ok()
            .and_then(|r| r.runs.into_iter().last())
            .map(RunView::stored))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `minutes = None` stops watching.
#[tauri::command]
pub fn set_watch(app: AppHandle, minutes: Option<u32>) {
    watch::set(&app, minutes);
}

#[tauri::command]
pub fn app_status(app: AppHandle, state: State<'_, AppState>) -> AppStatus {
    AppStatus {
        running: runs::is_running(&app),
        watch_minutes: watch::minutes(&app),
        data_file: state.store.file().display().to_string(),
    }
}
