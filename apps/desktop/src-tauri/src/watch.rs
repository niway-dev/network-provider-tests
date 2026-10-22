//! "Watch": a full run now and then every N minutes, like `nettest watch`.

use std::time::Duration;

use nettest_core::model::CheckId;
use tauri::{AppHandle, Emitter, Manager};

use crate::runs;
use crate::state::{AppState, Watch};
use crate::tray;

pub const DEFAULT_MINUTES: u32 = 5;

pub fn minutes(app: &AppHandle) -> Option<u32> {
    app.state::<AppState>()
        .watch
        .lock()
        .unwrap()
        .as_ref()
        .map(|w| w.minutes)
}

/// Start (Some(minutes)) or stop (None) the periodic run.
pub fn set(app: &AppHandle, minutes: Option<u32>) {
    let state = app.state::<AppState>();
    let minutes = minutes.filter(|m| *m > 0);
    {
        let mut slot = state.watch.lock().unwrap();
        if let Some(old) = slot.take() {
            old.task.abort();
        }
        if let Some(m) = minutes {
            let app = app.clone();
            let task = tauri::async_runtime::spawn(async move {
                loop {
                    // A run that is already going makes this tick a no-op.
                    let _ = runs::execute(&app, CheckId::ALL.to_vec()).await;
                    tokio::time::sleep(Duration::from_secs(u64::from(m) * 60)).await;
                }
            });
            *slot = Some(Watch { minutes: m, task });
        }
    }
    tray::show_watch(app, minutes);
    let _ = app.emit("watch-changed", minutes);
}
