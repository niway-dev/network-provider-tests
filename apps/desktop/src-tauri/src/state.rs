//! Shared app state, managed by Tauri.

use std::sync::Mutex;
use std::time::Instant;

use nettest_core::store::Store;
use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::tray::TrayIcon;
use tauri::Wry;

use crate::dto::RunView;

/// The periodic runner started by "Watch".
pub struct Watch {
    pub minutes: u32,
    pub task: tauri::async_runtime::JoinHandle<()>,
}

/// Pieces of the tray that change after a run.
pub struct TrayHandles {
    pub icon: TrayIcon<Wry>,
    pub last: MenuItem<Wry>,
    pub run: MenuItem<Wry>,
    pub watch: CheckMenuItem<Wry>,
}

pub struct AppState {
    pub store: Store,
    /// `Some(start)` while a run is in progress: a second run is refused as "busy".
    pub running: Mutex<Option<Instant>>,
    pub last: Mutex<Option<RunView>>,
    pub watch: Mutex<Option<Watch>>,
    pub tray: Mutex<Option<TrayHandles>>,
}

impl AppState {
    pub fn new(store: Store) -> AppState {
        AppState {
            store,
            running: Mutex::new(None),
            last: Mutex::new(None),
            watch: Mutex::new(None),
            tray: Mutex::new(None),
        }
    }
}
