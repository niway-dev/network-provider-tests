import { statusClass } from "../format";
import type { Status } from "../types";

export function StatusBadge({ status }: { status: Status | undefined }) {
  return <span className={`badge ${statusClass(status)}`}>{status ?? "…"}</span>;
}
