# Plan: Rust core, CLI, Windows, desktop app

Working document for the implementer. The PR of each milestone links this file;
the shipping PR of M4 deletes it (git keeps the history). Spec:
[`docs/specs/2026-10-22-rust-core-desktop-design.md`](../specs/2026-10-22-rust-core-desktop-design.md).
ADRs: 0001, 0002.

Rules for every task:

- Do not touch `bin/`, `checks/`, `lib/` until M4.
- Every public function in `nettest-core` has a unit test or a table test.
  Probes are faked through the `Probes` trait; no test opens a socket.
- Run `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and
  `cargo test --workspace` before marking a task done.
- Commit per task. Commit messages in English.

## M1: Rust CLI on macOS writing `runs.jsonl` compatible with bash

### T1.1 Workspace skeleton (done)

Files: `Cargo.toml` (workspace), `crates/nettest-core/Cargo.toml`,
`crates/nettest-core/src/lib.rs`, `crates/nettest-cli/Cargo.toml`,
`crates/nettest-cli/src/main.rs`, `rust-toolchain.toml` (stable), `.gitignore`
(`target/`).
Accept: `cargo build --workspace` succeeds; `cargo run -p nettest-cli -- --help`
prints usage.

### T1.2 Run record types (done)

Files: `crates/nettest-core/src/record.rs`, `tests/fixtures/run-bash-sample.jsonl`
(copy a real line from the bash tool on `main` once it exists; until then,
hand-write one that follows the envelope in ADR 0002 and the field mapping in
the spec).
Accept: `cargo test -p nettest-core record` passes: the fixture deserializes;
serialize then deserialize is identity; unknown fields are ignored.

### T1.3 Store (done)

Files: `crates/nettest-core/src/store.rs`.
Behavior: directory from `NETTEST_DATA_DIR`, else macOS
`~/Library/Logs/network-provider-tests`, else `dirs::data_local_dir()/network-provider-tests`.
`append` writes one buffer plus `\n` in append mode; `read_all` skips bad lines
and returns their count.
Accept: tests use a temp dir; two threads append 100 records each and every
line parses.

### T1.4 Check model, `Probes` trait, fake probes (done)

Files: `crates/nettest-core/src/model.rs`, `src/probes/mod.rs`,
`src/probes/fake.rs` (test-only builder that scripts each method's answer).
Accept: `cargo test -p nettest-core` compiles with the fake; no behavior yet.

### T1.5 Diagnosis as a pure function with table tests (done)

Files: `crates/nettest-core/src/diagnosis.rs`, `tests/diagnosis.rs`.
Port the bash `diagnose()` rule by rule (spec section "Diagnosis rules").
Table rows, one per README "Diagnosis rules" line plus the two recorded
incidents: local_down, router_ignores_ping, isp_ipv4_outage (with and without a
last hop), isp_down, isp_ipv4_no_v6, ipv4_fail_partial_run,
dns_primary_timeout_vpn, dns_primary_timeout, dns_slow_intermittent,
icmp_deprioritized, unmatched_warn, no_problem.
Accept: `cargo test -p nettest-core diagnosis` passes; each row asserts the
exact `code` list in order and the first sentence of each summary.

### T1.6 Checks 01, 03, 04 (gateway, IPv4, IPv6) against the fake (done)

Files: `src/checks/local_network.rs`, `ipv4_reachability.rs`,
`ipv6_reachability.rs`, tests next to each.
Thresholds and wording copied from the bash scripts (5 ms wired / 20 ms Wi-Fi,
"most probes failed" = `ok * 2 < total`).
Accept: tests cover pass, warn (loss, latency), fail, skip (no gateway, no
global IPv6); line texts match the bash strings.

### T1.7 Checks 02, 05, 06, 07 against the fake (done)

Files: `vpn_detection.rs`, `route_trace.rs`, `dns.rs`, `http_timing.rs`.
`route_trace` includes `is_isp_internal(ip)` with a table test for `10/8`,
`100.64/10`, `172.16/12` boundaries. `http_timing` includes the slowest-phase
selection and the 3000/1000/1000 ms thresholds.
Accept: tests cover each bash branch, including DNS "primary timeouts via VPN".

### T1.8 Runner and text renderer (done)

Files: `src/runner.rs`, `src/render.rs`.
Runner: ordered checks, per-check `tokio::time::timeout` (60 s), envelope
fields, `exit` = 1 if any `fail`. Renderer: the bash layout (`  PASS  text`,
hints indented with `->`, `== Summary ==`, `== Diagnosis ==`), colors off when
`NO_COLOR` is set or stdout is not a tty.
Accept: a snapshot test renders a fake full run and compares to a checked-in
text file; the file is reviewed once against real bash output.

### T1.9 Real probes on macOS: ping, HTTPS, DNS (done)

Files: `src/probes/os/mod.rs`, `ping.rs` (ping-async), `https.rs`
(tokio + tokio-rustls + webpki-roots, HEAD request, phase timestamps),
`dns.rs` (hickory-resolver: system resolver for the "system path", one
single-server resolver per direct query, `attempts = 0`, cache off).
Accept: `cargo run -p nettest-cli -- ipv4-reachability` and `-- dns` on the
owner's Mac print lines equivalent to bash checks 03 and 06; no sudo prompt.

### T1.10 Real probes on macOS: introspection (done)

Files: `src/probes/os/macos.rs`: `netdev` for default gateway/interface and
global IPv6; `scutil --dns` parsed into `ResolverEntry` (section main/scoped,
index, domain, nameservers, interface) with a unit test on a captured sample;
`route -n get <ip>` for `route_interface`; `utun*` interfaces with IPv4 from
`netdev`.
Accept: `nettest local-network`, `nettest vpn-detection` match bash 01 and 02
on the owner's Mac, with and without a VPN connected.

### T1.11 Traceroute on macOS (done)

Files: `src/probes/os/traceroute_macos.rs` (socket2 `SOCK_DGRAM`/`IPPROTO_ICMP`,
`IP_TTL` 1..15, 1 s wait, one probe per hop, parse the IP header plus ICMP
Time Exceeded or Echo Reply), `src/probes/os/traceroute_cmd.rs` (fallback:
spawn `traceroute -n -w 1 -q 1 -m 15`, parse hop number, IP literal, `*`).
Spike first: if the datagram socket does not deliver Time Exceeded on the
owner's macOS version, ship the fallback alone and note it in the spec's open
questions.
Accept: `nettest route-trace` lists hops like bash 05; parser unit tests with
English and Spanish `traceroute`/`tracert` samples (hop lines, `*` lines,
header lines ignored).

### T1.12 CLI commands (done)

Files: `crates/nettest-cli/src/main.rs` (clap): default run, `<check>` by name,
stem or number, `--list`, `--json`, `history [-n N] [--json]`,
`watch [minutes]`, exit codes 0/1/2.
Accept: `nettest --list` prints the seven checks; `nettest` appends one line to
`runs.jsonl`; `nettest history -n 3` prints the last three runs;
`nettest watch 1` runs twice in two minutes and exits cleanly on Ctrl-C.

### T1.13 Parity pass against bash on macOS

Run bash `./bin/nettest` and Rust `nettest` back to back three times; diff the
lines ignoring numbers. Fix wording gaps. Confirm the bash `nettest history`
on `main` reads a Rust-written line and Rust reads a bash-written line.
Accept: the four criteria in the spec's "Parity criterion" hold; record the
diff output in the PR description.

## M2: Windows support, check by check

### T2.1 Windows build

Files: `Cargo.toml` target-specific deps (`windows-sys`, `ipconfig`),
`src/probes/os/windows.rs` stub returning "unsupported" for every method.
Accept: `cargo build --workspace` on a Windows machine; `nettest --list` works.

### T2.2 Ping and HTTPS and DNS on Windows

`ping-async` and the HTTPS/DNS probes are cross-platform; verify only.
Accept: `nettest ipv4-reachability`, `ipv6-reachability`, `http-timing`, `dns`
run without an admin prompt; a firewall prompt at first run is acceptable and
documented in the README task (M3).

### T2.3 Introspection on Windows

Files: `windows.rs`: `netdev` for gateway/interface; `ipconfig::get_adapters()`
for resolvers in adapter order (the default-route adapter first, then others
with DNS servers); tunnels = adapters with `if_type` tunnel/PPP/proprietary
virtual or a description containing `Wintun`, `WireGuard`, `TAP-Windows`,
`OpenVPN`, that have an IPv4 address; `route_interface` through
`GetBestInterfaceEx` (`windows-sys`), mapped to the adapter's friendly name.
Accept: `nettest local-network` and `nettest vpn-detection` on the owner's
Windows machine with and without the VPN; unit tests on a captured adapter
list serialized to JSON.

### T2.4 Traceroute on Windows

Files: `src/probes/os/traceroute_windows.rs`: `IcmpSendEcho2Ex` with
`IP_OPTION_INFORMATION.Ttl` 1..15, 1000 ms timeout; `IP_TTL_EXPIRED_TRANSIT`
yields the hop address, `IP_SUCCESS` means reached, timeout means `*`.
Fallback: `tracert -d -w 1000 -h 15` through the shared structural parser.
Accept: `nettest route-trace` lists hops on Windows; no UAC prompt.

### T2.5 Store path and history on Windows

Accept: `runs.jsonl` lands in `%LOCALAPPDATA%\network-provider-tests\`;
`nettest history` and `watch 1` work.

## M3: Tauri 2 tray app

### T3.1 Scaffold

Files: `apps/desktop/` from `bun create tauri-app` (React + TS + Vite),
`src-tauri/Cargo.toml` with `nettest-core = { path = "../../../crates/nettest-core" }`
and the `tray-icon` feature; `oxlint` + `oxfmt` configs copied from the owner's
other app; `bun.lock` committed. Add `apps/desktop/src-tauri` to the workspace.
Accept: `bun install && bun tauri dev` opens an empty window with a tray icon
on macOS.

### T3.2 Tauri commands and events

Files: `src-tauri/src/commands.rs`: `run_all`, `run_check(name)`,
`history(limit)`, `set_watch(minutes)`; a `RunState` (`Mutex<Option<Instant>>`)
so a second `run_all` while one is running returns "busy"; events
`run-started`, `run-finished { record }`.
Accept: from the devtools console, `invoke('run_all')` returns a record and the
line appears in `runs.jsonl`.

### T3.3 Tray menu

Files: `src-tauri/src/tray.rs`: items "Last: <summary> <time>", "Run now",
"Watch every 5 min" (checkbox), "Open window", "Quit"; icon tint by last
status (three PNGs under `src-tauri/icons/`). On macOS set the icon as
template when the tint is neutral.
Accept: clicking "Run now" updates the "Last:" item when the run finishes;
"Watch" toggles a tokio interval task that calls the runner with
`mode = "watch"`.

### T3.4 Window: latest run, diagnosis, history

Files: `src/App.tsx`, `src/components/{RunView,DiagnosisBlock,HistoryTable}.tsx`,
`src/api.ts` (typed `invoke` wrappers; types generated by hand from
`record.rs`, kept in `src/types.ts`).
Accept: the window shows the same lines the CLI prints for the latest run, the
diagnosis block, and a history table with timestamp, mode, seven status cells,
diagnosis codes; "Run now" button disabled while a run is in progress.

### T3.5 Windows app build

Accept: `bun tauri dev` on Windows shows the tray app and runs all checks;
note any firewall prompt in the README.

### T3.6 README update

Files: `README.md`: build instructions for CLI (`cargo install --path crates/nettest-cli`)
and app (`bun tauri dev` / `bun tauri build` for a local bundle), the Windows
prerequisites link (Tauri docs), store paths per OS. Keep the checks and
diagnosis tables; they are unchanged.

## M4: delete bash after parity

### T4.1 Confirm parity once more

Re-run T1.13 on the current `main` (which may have gained `history`/`watch`
changes in bash). Any gap is fixed in Rust first.

### T4.2 Delete

Files removed: `bin/nettest`, `bin/nettest-log`, `checks/`, `lib/`,
`.shellcheckrc`. README loses the bash install lines and the "scripts stay the
source of truth" roadmap note; `docs/run-record.md` is updated to say both the
old bash tool and the Rust tool wrote the format. This PR also deletes this
plan file and flips ADR 0001 and 0002 to "accepted; implemented".
Accept: `git grep -n 'bin/nettest\|checks/0' -- README.md docs` returns nothing
stale; `cargo test --workspace` green on macOS and Windows.
