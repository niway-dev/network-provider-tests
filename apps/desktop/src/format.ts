import type { CheckKey, RunRecord, Status } from "./types";

export const CHECK_ORDER: CheckKey[] = [
  "local_network",
  "vpn_detection",
  "ipv4_reachability",
  "ipv6_reachability",
  "route_trace",
  "dns",
  "http_timing",
];

export const CHECK_LABEL: Record<CheckKey, string> = {
  local_network: "Local network",
  vpn_detection: "VPN detection",
  ipv4_reachability: "IPv4 reachability",
  ipv6_reachability: "IPv6 reachability",
  route_trace: "Route trace",
  dns: "DNS",
  http_timing: "HTTP timing",
};

/** Same rule as nettest-core's `worst_status`: FAIL beats WARN beats PASS. */
export function worstStatus(r: RunRecord): "FAIL" | "WARN" | "PASS" {
  const statuses = Object.values(r.checks).map((c) => c.status);
  if (r.exit !== 0 || statuses.includes("FAIL")) return "FAIL";
  if (statuses.includes("WARN")) return "WARN";
  return "PASS";
}

export function statusClass(s: Status | undefined): string {
  return `st-${(s ?? "SKIP").toLowerCase()}`;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** `2026-10-22 11:58:33` in the viewer's local time. */
export function formatTime(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function formatDuration(ms: number): string {
  return ms < 10_000 ? `${(ms / 1000).toFixed(1)} s` : `${Math.round(ms / 1000)} s`;
}

const ms1 = (v: number) => `${v < 10 ? v.toFixed(1) : Math.round(v)} ms`;

function ratio(b: { ok?: number | null; total?: number | null } | undefined): string | null {
  return b && b.ok != null && b.total != null ? `${b.ok}/${b.total}` : null;
}

/** The headline numbers of one check, from the stored record. */
export function keyNumbers(key: CheckKey, r: RunRecord): string {
  const c = r.checks;
  switch (key) {
    case "local_network": {
      const m = c.local_network;
      if (!m) return "";
      if (!m.gateway) return "no default gateway";
      const parts = [`${m.gateway}${m.link_kind ? ` (${m.link_kind})` : ""}`];
      if (m.avg_ms != null) parts.push(ms1(m.avg_ms));
      if (m.loss_pct != null) parts.push(`${m.loss_pct}% loss`);
      return parts.join(" · ");
    }
    case "vpn_detection": {
      const m = c.vpn_detection;
      if (!m) return "";
      const n = m.tunnels?.length ?? 0;
      const dns = m.primary_via_vpn ? "primary DNS through the VPN" : "primary DNS not via VPN";
      return `${n} tunnel${n === 1 ? "" : "s"} · ${dns}`;
    }
    case "ipv4_reachability":
    case "ipv6_reachability": {
      const m = c[key];
      if (!m) return "";
      if (key === "ipv6_reachability" && m.global_address === false)
        return "no global IPv6 address";
      const parts = [];
      const h = ratio(m.https);
      const p = ratio(m.icmp);
      if (h) parts.push(`HTTPS ${h}`);
      if (p) parts.push(`ping ${p}`);
      return parts.join(" · ");
    }
    case "route_trace": {
      const m = c.route_trace;
      if (!m) return "";
      if (m.reached) return `reached ${m.target ?? "target"} in ${m.hop_count ?? "?"} hops`;
      return m.last_hop ? `stops after hop ${m.last_hop}` : "no hop answered";
    }
    case "dns": {
      const m = c.dns;
      if (!m) return "";
      const parts = [];
      if (m.system.max_ms != null) parts.push(`system worst ${m.system.max_ms} ms`);
      const servers = (m.servers ?? []).filter((s) => s.result !== "skipped");
      const bad = servers.filter((s) => s.result === "timeout").length;
      if (servers.length) parts.push(`${bad} of ${servers.length} servers timed out`);
      return parts.join(" · ");
    }
    case "http_timing": {
      const m = c.http_timing;
      if (!m) return "";
      const parts = [];
      if (m.failed != null && m.total != null) parts.push(`${m.failed} of ${m.total} failed`);
      if (m.slow_phase) parts.push(`slowest phase: ${m.slow_phase}`);
      return parts.join(" · ");
    }
  }
}
