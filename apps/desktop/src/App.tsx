import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "./api";
import { HistoryTable } from "./components/HistoryTable";
import { LivePanel, RunPanel } from "./components/RunView";
import type { CheckInfo, CheckKey, History, Report, RunRecord, RunView } from "./types";

const HISTORY_LIMIT = 60;
const WATCH_CHOICES = [1, 5, 15, 30];

export function App() {
  const [latest, setLatest] = useState<RunView | null>(null);
  const [selected, setSelected] = useState<RunView | null>(null);
  const [history, setHistory] = useState<History | null>(null);
  const [running, setRunning] = useState(false);
  const [live, setLive] = useState<{ checks: CheckInfo[]; done: Map<CheckKey, Report> } | null>(
    null,
  );
  const [watch, setWatch] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);

  const refreshHistory = useCallback(async () => {
    try {
      setHistory(await api.history(HISTORY_LIMIT));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    let unsubscribe: (() => void) | undefined;
    void (async () => {
      unsubscribe = await api.subscribe({
        onRunStarted: (e) => {
          setRunning(true);
          setSelected(null);
          setLive({ checks: e.checks, done: new Map() });
        },
        onCheckFinished: (r) =>
          setLive((cur) =>
            cur ? { checks: cur.checks, done: new Map(cur.done).set(r.key, r) } : cur,
          ),
        onRunFinished: (v) => {
          setRunning(false);
          setLive(null);
          setLatest(v);
          void refreshHistory();
        },
        onWatchChanged: setWatch,
      });
      try {
        const status = await api.appStatus();
        setRunning(status.running);
        setWatch(status.watch_minutes);
        setLatest(await api.latest());
      } catch (e) {
        setError(String(e));
      }
      await refreshHistory();
    })();
    return () => {
      mounted.current = false;
      unsubscribe?.();
    };
  }, [refreshHistory]);

  const runNow = async () => {
    setError(null);
    try {
      await api.runAll();
    } catch (e) {
      // "busy" means the tray or the watch timer started a run first.
      if (String(e) !== "busy") setError(String(e));
    }
  };

  const changeWatch = (value: string) => {
    api.setWatch(value === "off" ? null : Number(value)).catch((e) => setError(String(e)));
  };

  const select = (r: RunRecord) => {
    if (latest && r.ts === latest.record.ts) setSelected(null);
    else
      setSelected({
        record: r,
        findings: r.diagnosis.map((d) => ({ ...d, advice: [] })),
        reports: null,
        save_error: null,
      });
  };

  const shown = selected ?? latest;

  return (
    <main>
      <header className="top">
        <div>
          <h1>nettest</h1>
          <p className="muted">Is it your network, your VPN, or your ISP?</p>
        </div>
        <div className="controls">
          <label className="watch">
            <span>Watch</span>
            <select value={watch ?? "off"} onChange={(e) => changeWatch(e.target.value)}>
              <option value="off">off</option>
              {WATCH_CHOICES.map((m) => (
                <option key={m} value={m}>
                  every {m} min
                </option>
              ))}
            </select>
          </label>
          <button type="button" className="primary" onClick={runNow} disabled={running}>
            {running ? "Running…" : "Run now"}
          </button>
        </div>
      </header>

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      {running && live ? (
        <LivePanel checks={live.checks} done={live.done} />
      ) : shown ? (
        <>
          {selected && (
            <p className="notice">
              Showing an older run.{" "}
              <button type="button" className="link" onClick={() => setSelected(null)}>
                Back to the latest
              </button>
            </p>
          )}
          <RunPanel view={shown} />
        </>
      ) : (
        <section className="card">
          <h2>No runs yet</h2>
          <p className="muted">
            Press Run now. A full run takes 6 to 30 seconds and is saved to the history file.
          </p>
        </section>
      )}

      <HistoryTable history={history} selectedTs={selected?.record.ts ?? null} onSelect={select} />
    </main>
  );
}
