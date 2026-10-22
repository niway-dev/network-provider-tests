import { CHECK_LABEL, CHECK_ORDER, formatTime, worstStatus } from "../format";
import type { History, RunRecord } from "../types";
import { Sparkline } from "./Sparkline";

export function HistoryTable({
  history,
  selectedTs,
  onSelect,
}: {
  history: History | null;
  selectedTs: string | null;
  onSelect: (r: RunRecord) => void;
}) {
  const runs = history?.runs ?? [];
  const newestFirst = [...runs].reverse();
  return (
    <section className="card" aria-labelledby="history-title">
      <header className="card-head">
        <h2 id="history-title">History</h2>
        <span className="muted">
          {runs.length ? `last ${runs.length} runs` : "no runs recorded yet"}
        </span>
      </header>
      <Sparkline runs={runs.slice(-40)} />
      {runs.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>Mode</th>
                {CHECK_ORDER.map((k, i) => (
                  <th key={k} title={CHECK_LABEL[k]} className="center">
                    {i + 1}
                  </th>
                ))}
                <th>Diagnosis</th>
              </tr>
            </thead>
            <tbody>
              {newestFirst.map((r, i) => (
                <tr
                  key={r.ts + i}
                  className={r.ts === selectedTs ? "selected" : undefined}
                  onClick={() => onSelect(r)}
                >
                  <td className="nowrap">
                    <button type="button" className="link" onClick={() => onSelect(r)}>
                      {formatTime(r.ts)}
                    </button>
                  </td>
                  <td>{r.mode}</td>
                  {CHECK_ORDER.map((k) => {
                    const s = r.checks[k]?.status;
                    return (
                      <td key={k} className="center">
                        <span
                          className={`dot st-${(s ?? "none").toLowerCase()}`}
                          title={`${CHECK_LABEL[k]}: ${s ?? "not run"}`}
                          role="img"
                          aria-label={`${CHECK_LABEL[k]}: ${s ?? "not run"}`}
                        />
                      </td>
                    );
                  })}
                  <td className={`codes st-text-${worstStatus(r).toLowerCase()}`}>
                    {r.diagnosis.map((d) => d.code).join(", ") || "-"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {history && history.skipped > 0 && (
        <p className="muted small">
          {history.skipped} line(s) in the file could not be read and were skipped.
        </p>
      )}
      {history && <p className="muted small">Source: {history.file}</p>}
    </section>
  );
}
