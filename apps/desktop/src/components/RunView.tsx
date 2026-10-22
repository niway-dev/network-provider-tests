import {
  CHECK_LABEL,
  CHECK_ORDER,
  formatDuration,
  formatTime,
  keyNumbers,
  worstStatus,
} from "../format";
import type { CheckInfo, CheckKey, Report, RunView as RunViewData } from "../types";
import { DiagnosisBlock } from "./DiagnosisBlock";
import { StatusBadge } from "./StatusBadge";

function Lines({ report }: { report: Report }) {
  return (
    <div className="lines">
      {report.lines.map((l, i) => (
        <p key={i} className={`line line-${l.kind}`}>
          {l.kind === "status" && (
            <span className={`tag st-${(l.status ?? "info").toLowerCase()}`}>{l.status}</span>
          )}
          {l.kind === "hint" && <span className="tag">→</span>}
          <span>{l.text}</span>
        </p>
      ))}
    </div>
  );
}

function Row({
  label,
  status,
  numbers,
  report,
}: {
  label: string;
  status?: Report["status"];
  numbers: string;
  report?: Report;
}) {
  const head = (
    <>
      <span className="check-name">{label}</span>
      <span className="check-numbers">{numbers}</span>
      <StatusBadge status={status} />
    </>
  );
  if (!report || report.lines.length === 0) return <li className="check-row">{head}</li>;
  return (
    <li className="check-row">
      <details>
        <summary>{head}</summary>
        <Lines report={report} />
      </details>
    </li>
  );
}

/** The latest run: one row per check with its key numbers, then the diagnosis. */
export function RunPanel({ view }: { view: RunViewData }) {
  const { record } = view;
  const overall = worstStatus(record);
  const byKey = new Map<string, Report>((view.reports ?? []).map((r) => [r.key, r]));
  const keys = CHECK_ORDER.filter((k) => record.checks[k]);
  return (
    <>
      <section className="card" aria-labelledby="run-title">
        <header className="card-head">
          <h2 id="run-title">{record.mode === "full" ? "Full run" : "Single check"}</h2>
          <span className="muted">
            {formatTime(record.ts)} · {formatDuration(record.duration_ms)} · {record.tool}{" "}
            {record.tool_version}
          </span>
          <StatusBadge status={overall} />
        </header>
        <ul className="checks">
          {keys.map((k) => (
            <Row
              key={k}
              label={`${CHECK_ORDER.indexOf(k) + 1}. ${CHECK_LABEL[k]}`}
              status={record.checks[k]?.status}
              numbers={keyNumbers(k, record)}
              report={byKey.get(k)}
            />
          ))}
        </ul>
        {view.reports === null && (
          <p className="muted small">
            Read from the history file. Run again to see the lines each check printed.
          </p>
        )}
        {view.save_error && <p className="error">{view.save_error}</p>}
      </section>
      <DiagnosisBlock findings={view.findings} />
    </>
  );
}

/** While a run is in progress: finished checks fill in, the rest wait. */
export function LivePanel({ checks, done }: { checks: CheckInfo[]; done: Map<CheckKey, Report> }) {
  return (
    <section className="card" aria-labelledby="live-title" aria-live="polite">
      <header className="card-head">
        <h2 id="live-title">Running…</h2>
        <span className="muted">
          {done.size} of {checks.length} checks finished
        </span>
      </header>
      <ul className="checks">
        {checks.map((c) => {
          const r = done.get(c.key);
          return (
            <Row
              key={c.key}
              label={`${c.number}. ${CHECK_LABEL[c.key]}`}
              status={r?.status}
              numbers={r ? "" : "waiting…"}
              report={r}
            />
          );
        })}
      </ul>
    </section>
  );
}
