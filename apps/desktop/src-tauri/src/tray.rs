//! Tray icon and menu: "Last: ...", Run now, Watch, Open window, Quit.

use nettest_core::history::worst_status;
use nettest_core::model::Status;
use nettest_core::record::RunRecord;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::runs;
use crate::state::{AppState, TrayHandles};
use crate::watch;

const NEUTRAL: &[u8] = include_bytes!("../icons/tray-neutral.png");
const PASS: &[u8] = include_bytes!("../icons/tray-pass.png");
const WARN: &[u8] = include_bytes!("../icons/tray-warn.png");
const FAIL: &[u8] = include_bytes!("../icons/tray-fail.png");

fn image(bytes: &'static [u8]) -> Image<'static> {
    Image::from_bytes(bytes).expect("bundled tray icon is a valid PNG")
}

fn watch_label(minutes: Option<u32>) -> String {
    format!(
        "Watch every {} min",
        minutes.unwrap_or(watch::DEFAULT_MINUTES)
    )
}

/// "Last: <first finding>, 12:04", shortened for a menu.
fn last_label(record: &RunRecord) -> String {
    let summary = record
        .diagnosis
        .first()
        .map(|d| d.summary.as_str())
        .unwrap_or("run finished");
    // `ts` is `2026-10-22T10:58:03-05:00`: the local clock time is chars 11..16.
    let time = record.ts.get(11..16).unwrap_or("");
    let mut text: String = summary.chars().take(70).collect();
    if summary.chars().count() > 70 {
        text.push('…');
    }
    format!("Last: {text}, {time}")
}

pub fn build(app: &AppHandle, last: Option<&RunRecord>) -> tauri::Result<()> {
    let last_item = MenuItem::with_id(
        app,
        "last",
        last.map(last_label)
            .unwrap_or_else(|| "Last: no runs yet".into()),
        false,
        None::<&str>,
    )?;
    let run = MenuItem::with_id(app, "run", "Run now", true, None::<&str>)?;
    let watch_item =
        CheckMenuItem::with_id(app, "watch", watch_label(None), true, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&last_item, &sep1, &run, &watch_item, &open, &sep2, &quit],
    )?;

    let (icon, template) = icon_for(last.map(worst_status));
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(template)
        .tooltip("nettest")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "run" => runs::spawn_full_run(app),
            "watch" => {
                // The check mark flips on click; follow it.
                let on = app
                    .state::<AppState>()
                    .tray
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|t| t.watch.is_checked().ok())
                    .unwrap_or(false);
                watch::set(app, on.then_some(watch::DEFAULT_MINUTES));
            }
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    *app.state::<AppState>().tray.lock().unwrap() = Some(TrayHandles {
        icon: tray,
        last: last_item,
        run,
        watch: watch_item,
    });
    Ok(())
}

/// Neutral (never run, or running) is a template image so macOS tints it for
/// light and dark menu bars; pass/warn/fail are colored.
fn icon_for(status: Option<Status>) -> (Image<'static>, bool) {
    match status {
        Some(Status::Fail) => (image(FAIL), false),
        Some(Status::Warn) => (image(WARN), false),
        Some(Status::Pass) | Some(Status::Info) => (image(PASS), false),
        _ => (image(NEUTRAL), true),
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// While a run is in progress: neutral icon, "Running..." label, "Run now" off.
pub fn set_running(app: &AppHandle, running: bool) {
    let state = app.state::<AppState>();
    let guard = state.tray.lock().unwrap();
    let Some(t) = guard.as_ref() else { return };
    if running {
        let _ = t.last.set_text("Running checks…");
        let _ = t.icon.set_icon(Some(image(NEUTRAL)));
        let _ = t.icon.set_icon_as_template(true);
        let _ = t.icon.set_tooltip(Some("nettest: running checks"));
    }
    let _ = t.run.set_enabled(!running);
}

/// Update the "Last:" line and the icon tint after a run.
pub fn show_last(app: &AppHandle, record: &RunRecord) {
    let state = app.state::<AppState>();
    let guard = state.tray.lock().unwrap();
    let Some(t) = guard.as_ref() else { return };
    let _ = t.last.set_text(last_label(record));
    let _ = t.run.set_enabled(true);
    let (icon, template) = icon_for(Some(worst_status(record)));
    let _ = t.icon.set_icon(Some(icon));
    let _ = t.icon.set_icon_as_template(template);
    let _ = t.icon.set_tooltip(Some(&last_label(record)));
}

/// Keep the check mark and label in step with the watch state.
pub fn show_watch(app: &AppHandle, minutes: Option<u32>) {
    let state = app.state::<AppState>();
    let guard = state.tray.lock().unwrap();
    let Some(t) = guard.as_ref() else { return };
    let _ = t.watch.set_checked(minutes.is_some());
    let _ = t.watch.set_text(watch_label(minutes));
}
