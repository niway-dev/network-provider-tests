# ADR 0001: One Rust core library is the single source of truth

- Status: proposed; not implemented. The repository on this branch contains only
  the bash tool.
- Date: 2026-10-22

## Context

The checks exist as macOS bash scripts that shell out to `ping`, `curl`, `dig`,
`traceroute`, `scutil` and `route`. The owner wants the same checks on Windows
and in a menu-bar desktop app on both OSes. The diagnosis rules are the product;
if two implementations exist they will disagree the day it matters.

## Decision

Rewrite the seven checks, the diagnosis rules, the run record and the store in
one Rust library crate, `nettest-core`. A CLI binary and a Tauri 2 desktop app
depend on that crate directly. Probes sit behind a trait so the rules are
testable without a network. Bash stays until the Rust CLI meets the parity
criterion in the design spec, then it is deleted.

## Alternatives rejected

1. Bash on macOS plus a PowerShell twin on Windows. Two rule engines, two
   output parsers for every OS command, and the desktop app would still need a
   third thing to call them and parse their text. Rejected for drift and triple
   maintenance.
2. TypeScript on Bun compiled to a binary, used as a Tauri sidecar. JavaScript
   cannot open ICMP sockets or set TTL on probes, so ping and traceroute would
   still shell out to `ping`/`tracert`, whose output is localized on Windows and
   differs between OSes. The sidecar also doubles the process model of the app.
   Rejected because it keeps the hardest part (ICMP, traceroute) as text parsing.
3. Keep bash as the source of truth and have the app run it. Does not run on
   Windows; the app would parse terminal text. Rejected.

## Consequences

- One language for all OS-touching work, matching the policy of the owner's
  other Tauri app (OS work in Rust, renderer only renders).
- The owner does not maintain the Rust by hand; agents do. The design therefore
  prefers well-known crates and small modules over clever code.
- Unprivileged ICMP on macOS and `IcmpSendEcho2Ex` on Windows avoid admin
  prompts; raw sockets are forbidden in the core.
- A compile toolchain (`cargo`, and for the app `bun` + Tauri prerequisites) is
  needed on each machine. Accepted: internal use, local builds.

## Non-goals

- Distribution (signing, installers, updater).
- Linux support.
- Reusing third-party suites such as NetPulse wholesale.

## What would reopen this

- A platform where no unprivileged ICMP path exists and shelling out is the
  only option; then a per-OS adapter inside the same crate, not a new tool.
- The owner stops using Windows; the Rust core still stands, but the second
  rejected alternative's main cost disappears and a bash-only tool becomes
  viable again for the CLI.
