import { formatTime, worstStatus } from "../format";
import type { RunRecord } from "../types";

const BAR = 7;
const GAP = 3;
const H = 32;

/** One bar per run: height is the share of IPv4 HTTPS probes that worked, color is the run's result. */
export function Sparkline({ runs }: { runs: RunRecord[] }) {
  if (runs.length === 0) return null;
  const width = runs.length * (BAR + GAP) - GAP;
  return (
    <svg
      className="spark"
      viewBox={`0 0 ${width} ${H}`}
      width={width}
      height={H}
      role="img"
      aria-label={`Last ${runs.length} runs: bar height is the share of IPv4 HTTPS probes that worked`}
    >
      {runs.map((r, i) => {
        const h = r.checks.ipv4_reachability?.https;
        const share = h?.ok != null && h.total ? h.ok / h.total : 1;
        const bh = Math.max(3, Math.round(share * H));
        return (
          <rect
            key={r.ts + i}
            className={`bar bar-${worstStatus(r).toLowerCase()}`}
            x={i * (BAR + GAP)}
            y={H - bh}
            width={BAR}
            height={bh}
            rx={2}
          >
            <title>{`${formatTime(r.ts)}: ${worstStatus(r)}${h?.total ? `, IPv4 HTTPS ${h.ok}/${h.total}` : ""}`}</title>
          </rect>
        );
      })}
    </svg>
  );
}
