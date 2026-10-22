# Rust core + desktop app design

Date: 2026-10-22. Status: design, nothing implemented. Plan:
[`docs/plans/2026-10-22-rust-core-desktop.md`](../plans/2026-10-22-rust-core-desktop.md).

## Summary (one minute)

The seven bash checks and the diagnosis rules are rewritten once, in Rust, in a
single library crate (`nettest-core`). A thin CLI (`nettest`) and a Tauri 2
menu-bar/tray app (`apps/desktop`) both call that crate directly; there is no
sidecar and no second implementation. Every run is appended as one JSON line to
`runs.jsonl` in the same schema-1 envelope the bash tool writes, so history
survives the rewrite. Probes use unprivileged OS APIs on macOS and Windows (no
admin, no raw sockets, no OpenSSL); the one hard check, traceroute, is done
natively on both OSes with a shell-out fallback. Order of delivery: Rust CLI on
macOS with parity, then Windows, then the desktop app, then delete the bash
scripts.

| # | Decision | Why | ADR |
| --- | --- | --- | --- |
| 1 | One Rust core library is the single source of truth for checks and diagnosis; CLI and desktop are thin front ends | Two implementations drift; the diagnosis rules must stay identical everywhere | [0001](../adr/0001-rust-core-single-source-of-truth.md) |
| 2 | Targets are macOS and Windows; Linux compiles if free but is not tested | Owner's machines | none |
| 3 | Run history stays append-only JSONL in the schema-1 envelope; Rust reads and writes the exact format bash writes | Keeps bash-era history, trivially diffable, no migrations | [0002](../adr/0002-run-record-jsonl-contract.md) |
| 4 | Probes behind a `Probes` trait; diagnosis is a pure function over the run record | Rules get table tests without a network; front ends never touch sockets | none (detail below) |
| 5 | Ping: `ping-async` (ICMP datagram socket on macOS, `IcmpSendEcho2Ex` on Windows) | Unprivileged on both OSes, IPv4 and IPv6, exposes TTL | none |
| 6 | Traceroute: own module. macOS `SOCK_DGRAM`/`IPPROTO_ICMP` + `IP_TTL`; Windows `IcmpSendEcho2Ex` with TTL and `IP_TTL_EXPIRED_TRANSIT`. Fallback: shell `traceroute -n` / `tracert -d` parsed structurally | Native path needs no admin on either OS; fallback keeps the check alive if the native path fails on some machine | none (detail below) |
| 7 | DNS: `hickory-resolver` with one `NameServerConfig` per server for direct queries; the OS resolver for the "system path" timing | Pure Rust, per-server timeout control, same on both OSes | none |
| 8 | HTTPS probes and phase timing: manual TCP connect to a v4/v6 address + `tokio-rustls` + a one-line HTTP/1.1 request; no `reqwest` | Only way to force the address family and time each phase; no OpenSSL on Windows | none |
| 9 | OS introspection: `netdev` (default gateway/interface both OSes); `scutil --dns` and `route -n get` on macOS; `ipconfig` crate + `GetBestInterfaceEx` on Windows | `scutil` output is structured and not localized; Windows gets the same data from `GetAdaptersAddresses` through `ipconfig` | none |
| 10 | Store: macOS `~/Library/Logs/network-provider-tests/`, Windows `%LOCALAPPDATA%\network-provider-tests\`; `NETTEST_LOG_DIR` overrides both | macOS path matches the bash tool, so history is shared during the transition | none |
| 11 | Desktop: Tauri 2 tray app, `src-tauri` depends on `nettest-core` as a path crate; renderer React + TS + Vite, Bun, oxlint + oxfmt | Same shape as the owner's other Tauri app; OS work stays in Rust | none |
| 12 | No distribution work: local `cargo` / `bun tauri dev` builds only | Internal use | none |
| 13 | Bash is deleted when the Rust CLI meets the parity criterion below, not before | Bash is the working tool today | none |

## Open questions

Each has a recommended default; the implementer takes the default unless told
otherwise.

1. Should the desktop app's periodic watch run the full suite (30 to 60 s of
   probes) or a cheap subset? Default: a "light" run (gateway ping, one IPv4 and
   one IPv6 HTTPS probe, system DNS timing), stored with `"mode":"watch"`, full
   run only on demand.
2. Does the Windows build also keep `nettest-log`-style daily text logs?
   Default: no; `runs.jsonl` is the log on both OSes, and `nettest history`
   prints it.
3. Traceroute over IPv6: bash only traces `1.1.1.1`. Default: IPv4 only, same as
   bash; add IPv6 later only if a real incident needs it.
4. Does the Tauri app need auto-start at login? Default: no for M3; the
   `tauri-plugin-autostart` plugin can be added later in one task.

## Out of scope

- Signing, notarization, installers, updater, CI release pipelines.
- Linux support beyond "it may compile".
- New checks or new diagnosis rules. Parity first; new rules are separate work.
- Throughput / speed tests, Wi-Fi signal analysis, packet capture.
- Any server, cloud sync, or telemetry.

## Failures this must protect against

Input for tests. Each row becomes at least one test in the plan.

| Failure | How it shows up | Guard |
| --- | --- | --- |
| Diagnosis rule drifts from bash | A synthetic record that bash classifies as "ISP IPv4 outage" gets another code in Rust | Table tests on `diagnose()` reuse the bash synthetic cases (one record per README row) |
| A probe hangs | A check never finishes; the app looks frozen | Every probe has a hard timeout in `Probes`; the runner wraps each check in `tokio::time::timeout` and records `status:"skip"` with an `error` field |
| Writing a run record breaks old readers | `nettest history` in bash cannot parse a Rust line, or vice versa | Golden-file test: a record written by Rust deserializes with the documented schema; a bash-written sample deserializes in Rust |
| A field gets renamed | History silently shows empty columns | Serde structs use `#[serde(deny_unknown_fields)]` only in tests; production readers ignore unknown fields and never rename |
| Traceroute parser trusts localized text | Windows in Spanish prints "Tiempo de espera agotado" and the parser breaks | Fallback parser keys only on hop number, IP literals and `*`; tests feed English and Spanish samples |
| ICMP blocked but HTTPS fine | A false "outage" | The rules never fail on ICMP alone (ported from bash rule 4) |
| No IPv6 on the network | IPv6 check reported as failure | `ipv6_reachability.status = "skip"` and `v6_global:false`; rules treat "unknown" and "fails" differently (bash rule 2) |
| Concurrent writers to `runs.jsonl` | CLI and app run at the same time and interleave lines | Each record is one `write_all` of a single buffer ending in `\n`, file opened with append; tests write from two threads and parse every line |
| Admin rights requested | A Windows firewall or UAC prompt during a run | No raw sockets anywhere; a test asserts the probe layer never opens `SOCK_RAW` (grep in CI plus a manual check) |
| The desktop app blocks its UI | Tray menu freezes during a run | Tauri commands are `async` and run on the core's tokio runtime; a run in progress is reported through an event, not a blocking call |

## Workspace layout

```
Cargo.toml                  # workspace: crates/*, apps/desktop/src-tauri
crates/nettest-core/        # checks, probes, diagnosis, run record, store
crates/nettest-cli/         # binary `nettest`: run, <check>, --list, history, watch
apps/desktop/               # Tauri 2 app: src-tauri/ (Rust) + src/ (React + TS + Vite)
docs/                       # adr/, specs/, plans/, run-record.md (from main)
```

`apps/desktop/src-tauri/Cargo.toml` depends on `nettest-core` by path. No
sidecar binary: the app links the library.

## Core model

```rust
// crates/nettest-core, illustrative only
pub enum Status { Pass, Warn, Fail, Info, Skip }

pub struct Line { pub status: Status, pub text: String, pub hint: Option<String> }

pub struct CheckOutput<M> { pub status: Status, pub lines: Vec<Line>, pub metrics: M }

#[async_trait]
pub trait Probes: Send + Sync {
    async fn ping(&self, target: IpAddr, count: u8, timeout: Duration) -> PingStats;
    async fn https(&self, url: &Url, family: Family, timeout: Duration) -> HttpsResult;
    async fn dns_query(&self, server: SocketAddr, name: &str, timeout: Duration) -> DnsResult;
    async fn dns_system(&self, name: &str, timeout: Duration) -> DnsResult;
    async fn traceroute(&self, target: Ipv4Addr, max_hops: u8) -> Vec<Hop>;
    fn default_route(&self) -> Option<DefaultRoute>;       // gateway, interface, link kind
    fn tunnels_with_ipv4(&self) -> Vec<Tunnel>;
    fn resolvers(&self) -> Vec<ResolverEntry>;               // ordered like scutil --dns
    fn route_interface(&self, ip: IpAddr) -> Option<String>;
    fn has_global_ipv6(&self) -> bool;
}
```

- One module per check (`checks/local_network.rs` ... `checks/http_timing.rs`),
  each an `async fn run(probes: &dyn Probes) -> CheckOutput<XMetrics>`. The
  metrics struct of each check is exactly its `checks.<name>` object in the run
  record, so the record is built by serializing the outputs.
- `runner::run_all(probes, selection) -> RunRecord` runs the checks in the bash
  order (01..07), each under a timeout, and fills the envelope (`ts`, `tool`,
  `tool_version`, `os`, `mode`, `duration_ms`, `exit`).
- `diagnosis::diagnose(&RunRecord) -> Vec<Diagnosis { code, summary, advice }>`
  is pure. It needs no network and no `Probes`.
- `store::append(&RunRecord)`, `store::read_all() -> Vec<RunRecord>`.
- Rendering (colored text) lives in `render.rs` inside the core so the CLI and
  the app print identical lines; the app shows the same `Line`s in a list.

## Diagnosis rules, ported 1:1

The bash `diagnose()` reads variables from `NT_RESULTS`. Each maps to a field
of the run record; the rule bodies are copied in order, same thresholds.

| Bash input | Run record field |
| --- | --- |
| `GW_OK` | `checks.local_network.gateway_ok` (bool, absent when no IPv4 gateway) |
| `V4_HTTPS_OK/TOTAL`, `V4_ICMP_OK/TOTAL` | `checks.ipv4_reachability.https_ok/https_total/icmp_ok/icmp_total` |
| `V6_GLOBAL`, `V6_HTTPS_OK/TOTAL` | `checks.ipv6_reachability.v6_global/https_ok/https_total` |
| `TRACE_REACHED/LAST_HOP/LAST_IP` | `checks.route_trace.reached/last_hop/last_ip` |
| `DNS_SYS_STATUS`, `DNS_PRIMARY_TIMEOUTS`, `DNS_PRIMARY_VIA_VPN`, `DNS_PRIMARY_IF`, `DNS_SERVER` rows | `checks.dns.system_status/primary_timeouts[]/primary_via_vpn/primary_interface/servers[]` |
| `STATUS_*` | each `checks.<name>.status` |

Rules keep their numbering and short codes: `local_down`, `router_ignores_ping`,
`isp_ipv4_outage`, `isp_down`, `isp_ipv4_no_v6`, `ipv4_fail_partial_run`,
`dns_primary_timeout_vpn`, `dns_primary_timeout`, `dns_slow_intermittent`,
`icmp_deprioritized`, `unmatched_warn`, `no_problem`. The `summary` strings are
the bash sentences verbatim. If `docs/run-record.md` on `main` fixes different
field or code names, `main` wins; the mapping above adapts, never the schema.

## Crate per check, and how Windows differs

| Check | macOS | Windows | Crate / API |
| --- | --- | --- | --- |
| 01 local network | `netdev::get_default_gateway()` + `get_default_interface()`; link kind from the interface type (Wi-Fi vs wired) | Same calls; `netdev` wraps `GetAdaptersAddresses` | [netdev 0.46](https://docs.rs/netdev/latest/netdev/) |
| 01 gateway ping, 03/04 pings | `ping-async` opens an ICMP datagram socket (no root) | `ping-async` calls `IcmpSendEcho2Ex` / `Icmp6SendEcho2` (no admin) | [ping-async 1.0.2](https://docs.rs/ping-async/latest/ping_async/) |
| 02 VPN detection | Interfaces named `utun*` with an IPv4 address; `scutil --dns` for resolver order; `route -n get <ip>` for the interface a server is reached through | Adapters whose `if_type` is tunnel/PPP/proprietary-virtual or whose description matches `Wintun`, `WireGuard`, `TAP-Windows`, `OpenVPN`; `ipconfig::get_adapters()` for DNS servers; `GetBestInterfaceEx` for the route | [ipconfig 0.3](https://docs.rs/ipconfig/latest/ipconfig/), `windows-sys` |
| 03/04 HTTPS over v4/v6 | Resolve with `tokio::net::lookup_host`, keep addresses of the wanted family, `TcpStream::connect`, `tokio-rustls` handshake, send `HEAD / HTTP/1.1`, read status line | Identical; rustls uses `aws-lc-rs` or `ring`, no OpenSSL | [tokio-rustls 0.26](https://docs.rs/tokio-rustls/latest/tokio_rustls/), `webpki-roots` |
| 04 global IPv6 present | Any interface address in `2000::/3` from `netdev` | Same | netdev |
| 05 traceroute | `socket2` socket `(AF_INET, SOCK_DGRAM, IPPROTO_ICMP)`, set `IP_TTL` per hop, send echo, read reply including IP header (macOS delivers Time Exceeded on this socket type per [icmp(4)](https://www.unix.com/man-page/osx/4/icmp/)) | `IcmpSendEcho2Ex` with `Ttl` in `IP_OPTION_INFORMATION`; status `IP_TTL_EXPIRED_TRANSIT` carries the hop address in `ICMP_ECHO_REPLY.Address` ([IcmpSendEcho2Ex](https://learn.microsoft.com/en-us/windows/win32/api/icmpapi/nf-icmpapi-icmpsendecho2ex)) | own module on `socket2` and `windows-sys`; fallback `traceroute -n -w 1 -q 1 -m 15` / `tracert -d -w 1000 -h 15` parsed structurally |
| 06 DNS system path | `hickory-resolver` with the system config (`system-config` feature reads SystemConfiguration) and `timeout = 3 s`, `attempts = 0`; wall-clock measured around the lookup | Same; hickory reads adapters through `ipconfig` | [hickory-resolver 0.26](https://docs.rs/hickory-resolver/latest/hickory_resolver/) |
| 06 DNS direct | One resolver per server: `ResolverConfig::from_parts(None, [], [NameServerConfig::udp(ip)])`, `timeout = 2 s`, `attempts = 0`, cache off | Same | hickory-resolver |
| 07 HTTP timing | Same stack as 03 with the OS's default family order (first address returned), timestamps between phases: dns, connect, tls, ttfb, total | Same | tokio-rustls |
| Store | `~/Library/Logs/network-provider-tests/runs.jsonl` | `dirs::data_local_dir()/network-provider-tests/runs.jsonl` | [dirs](https://docs.rs/dirs/latest/dirs/) (only for the Windows path) |

Why not these: `surge-ping` and `ping-tokio` need raw sockets or do not support
Windows ([ping-tokio 0.4.0](https://docs.rs/crate/ping-tokio/0.4.0));
`trippy-core` has unprivileged mode on macOS only and needs admin on Windows
([trippy](https://docs.rs/crate/trippy/latest)); `ftr` says root may be needed
on macOS for IPv4 ([ftr 0.10.0](https://docs.rs/crate/ftr/latest)); `reqwest`
cannot force an address family without binding a source address and exposes no
per-phase timing. NetPulse ([github.com/shellrow/netpulse](https://github.com/shellrow/netpulse))
is a full Rust + Tauri diagnostics suite by the author of `netdev`; its
ping/traceroute go through its own low-level crates that need raw sockets on
Windows, so only `netdev` is reused. (The NetPulse repository could not be
fetched during this design; verify before the traceroute task if curious.)

## CLI surface

Same commands as bash, same output lines, plus the record commands that `main`
adds to bash.

```
nettest                 run every check, print summary + diagnosis, append a run record
nettest dns | 06-dns | 06   run one check, diagnose what is known
nettest --list          list checks
nettest history [-n N] [--json]   print stored runs (table; --json streams the lines)
nettest watch [minutes] run every N minutes (default 5) until Ctrl-C, appending records
nettest --json          print the run record instead of text
```

Exit status 0 when no check is `fail`, 1 otherwise, 2 on usage errors.
`NO_COLOR` and a non-tty stdout disable colors (crate `owo-colors` or `anstyle`;
either, smallest wins). `NETTEST_LOG_DIR` overrides the store directory.

## Desktop app

- Tray icon with a menu: last result line (for example "No problem detected,
  12:04"), "Run now", "Watch every 5 min" (toggle), "Open window", "Quit". Icon
  tint reflects the last run: green pass, yellow warn, red fail.
- Window (one page, React): latest run with the same `Line`s the CLI prints,
  the diagnosis block, and a history table (timestamp, mode, per-check status,
  diagnosis codes) read from `runs.jsonl`.
- Tauri commands in `src-tauri`: `run_all() -> RunRecord`, `run_check(name)`,
  `history(limit) -> Vec<RunRecord>`, `set_watch(minutes: Option<u32>)`. A run
  emits `run-started` / `run-finished` events; the renderer never touches the
  network or the file system.
- Renderer tooling: Bun + Vite + React + TypeScript, `oxlint` and `oxfmt`,
  because the owner's other app already uses them and the renderer is small
  enough that no other choice pays for itself.

## Parity criterion for deleting bash

Bash is deleted (M4) when, on the owner's macOS machine:

1. `nettest` (Rust) prints the same PASS/WARN/FAIL/INFO/SKIP lines for all seven
   checks on a healthy connection, modulo timing numbers.
2. The diagnosis table tests pass for every README row and for the two recorded
   incidents.
3. A Rust-written `runs.jsonl` line is accepted by `nettest history` of the bash
   tool on `main`, and vice versa.
4. `nettest history` and `nettest watch` work with a `runs.jsonl` that mixes
   bash and Rust lines.
