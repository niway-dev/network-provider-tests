// Hand-written from crates/nettest-core/src/record.rs (schema 1) and
// src-tauri/src/dto.rs. Only what the window reads; unknown keys are ignored.

export type Status = "PASS" | "WARN" | "FAIL" | "INFO" | "SKIP";

type Maybe<T> = T | null | undefined;

export interface LocalNetwork {
  status: Status;
  interface?: Maybe<string>;
  gateway?: Maybe<string>;
  link_kind?: Maybe<string>;
  reachable?: Maybe<boolean>;
  loss_pct?: Maybe<number>;
  avg_ms?: Maybe<number>;
}

export interface VpnDetection {
  status: Status;
  tunnels?: string[];
  primary_via_vpn?: Maybe<boolean>;
  primary_interface?: Maybe<string>;
}

export interface Block {
  ok?: Maybe<number>;
  total?: Maybe<number>;
}

export interface Reachability {
  status: Status;
  global_address?: Maybe<boolean>;
  icmp: Block;
  https: Block;
}

export interface RouteTrace {
  status: Status;
  target?: Maybe<string>;
  reached?: Maybe<boolean>;
  last_hop?: Maybe<number>;
  last_ip?: Maybe<string>;
  hop_count?: Maybe<number>;
}

export interface DnsServer {
  group: string;
  ip: string;
  result: string;
}

export interface Dns {
  status: Status;
  system: { status?: Maybe<string>; max_ms?: Maybe<number> };
  servers?: DnsServer[];
}

export interface HttpTiming {
  status: Status;
  failed?: Maybe<number>;
  total?: Maybe<number>;
  slow_phase?: Maybe<string>;
}

export interface Checks {
  local_network?: LocalNetwork;
  vpn_detection?: VpnDetection;
  ipv4_reachability?: Reachability;
  ipv6_reachability?: Reachability;
  route_trace?: RouteTrace;
  dns?: Dns;
  http_timing?: HttpTiming;
}

export type CheckKey = keyof Checks;

export interface DiagnosisEntry {
  code: string;
  summary: string;
}

export interface RunRecord {
  schema: number;
  ts: string;
  tool: string;
  tool_version: string;
  os: string;
  mode: "full" | "single";
  duration_ms: number;
  exit: number;
  checks: Checks;
  diagnosis: DiagnosisEntry[];
}

export interface CheckInfo {
  key: CheckKey;
  number: number;
  name: string;
  title: string;
}

export interface Line {
  kind: "status" | "hint" | "dim";
  status: Status | null;
  text: string;
}

export interface Report extends CheckInfo {
  status: Status;
  lines: Line[];
}

export interface Finding {
  code: string;
  summary: string;
  advice: string[];
}

export interface RunView {
  record: RunRecord;
  findings: Finding[];
  /** Null for runs read back from runs.jsonl: it stores numbers, not text. */
  reports: Report[] | null;
  save_error: string | null;
}

export interface History {
  runs: RunRecord[];
  skipped: number;
  file: string;
}

export interface AppStatus {
  running: boolean;
  watch_minutes: number | null;
  data_file: string;
}

export interface RunStarted {
  mode: "full" | "single";
  checks: CheckInfo[];
}
