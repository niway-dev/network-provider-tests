import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppStatus, History, Report, RunStarted, RunView } from "./types";

export const runAll = () => invoke<RunView>("run_all");
export const runCheck = (name: string) => invoke<RunView>("run_check", { name });
export const history = (limit: number) => invoke<History>("history", { limit });
export const latest = () => invoke<RunView | null>("latest");
export const setWatch = (minutes: number | null) => invoke<void>("set_watch", { minutes });
export const appStatus = () => invoke<AppStatus>("app_status");

export interface Handlers {
  onRunStarted: (e: RunStarted) => void;
  onCheckFinished: (r: Report) => void;
  onRunFinished: (v: RunView) => void;
  onWatchChanged: (minutes: number | null) => void;
}

/** Subscribe to the backend's events; returns one function that unsubscribes all. */
export async function subscribe(h: Handlers): Promise<UnlistenFn> {
  const offs = await Promise.all([
    listen<RunStarted>("run-started", (e) => h.onRunStarted(e.payload)),
    listen<Report>("check-finished", (e) => h.onCheckFinished(e.payload)),
    listen<RunView>("run-finished", (e) => h.onRunFinished(e.payload)),
    listen<number | null>("watch-changed", (e) => h.onWatchChanged(e.payload)),
  ]);
  return () => offs.forEach((off) => off());
}
