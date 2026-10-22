import type { Finding } from "../types";

export function DiagnosisBlock({ findings }: { findings: Finding[] }) {
  if (findings.length === 0) return null;
  return (
    <section className="card" aria-labelledby="diagnosis-title">
      <h2 id="diagnosis-title">Diagnosis</h2>
      <ul className="findings">
        {findings.map((f) => (
          <li key={f.code}>
            <p className="finding-summary">{f.summary}</p>
            {f.advice.map((line) => (
              <p className="finding-advice" key={line}>
                {line}
              </p>
            ))}
            <code className="finding-code">{f.code}</code>
          </li>
        ))}
      </ul>
    </section>
  );
}
