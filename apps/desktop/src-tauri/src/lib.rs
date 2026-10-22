//! nettest desktop: a menu-bar app around `nettest-core`. The core is linked
//! as a library (no sidecar), so a run here and a run from the CLI are the
//! same code writing the same `runs.jsonl`.

mod commands;
mod dto;
mod runs;
mod state;
mod tray;
mod watch;

use nettest_core::store::Store;
use tauri::{Manager, WindowEvent};

use crate::state::AppState;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new(Store::from_env()))
        .invoke_handler(tauri::generate_handler![
            commands::run_all,
            commands::run_check,
            commands::history,
            commands::latest,
            commands::set_watch,
            commands::app_status,
        ])
        .setup(|app| {
            // A tray app: no Dock icon, no app menu bar.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let last = {
                let state = handle.state::<AppState>();
                state
                    .store
                    .read_all()
                    .ok()
                    .and_then(|r| r.runs.into_iter().last())
            };
            tray::build(&handle, last.as_ref())?;

            // Development and smoke tests: start with a run or an open window.
            if std::env::var_os("NETTEST_APP_RUN_ON_START").is_some() {
                runs::spawn_full_run(&handle);
            }
            if std::env::var_os("NETTEST_APP_OPEN_ON_START").is_some() {
                tray::show_window(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it; the tray menu quits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building the nettest desktop app")
        .run(|_app, event| {
            // Only the tray's Quit (an explicit exit code) ends the app; the
            // implicit exit when the last window closes is refused.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                eprintln!("nettest-desktop: exit requested (code {code:?})");
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
