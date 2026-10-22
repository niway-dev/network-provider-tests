# ADR 0002: Run history is an append-only JSONL file with a fixed envelope

- Status: proposed; not implemented in Rust. The bash tool on `main` is gaining
  the same format in parallel (`docs/run-record.md` there is the normative
  field list; this branch may not contain it yet).
- Date: 2026-10-22

## Context

Runs must be stored so that `history`, `watch`, the desktop app's table, and
ISP complaints can use them. The bash tool writes one JSON line per run with a
schema-1 envelope: `schema, ts, tool, tool_version, os, mode, duration_ms,
exit, checks{local_network, vpn_detection, ipv4_reachability,
ipv6_reachability, route_trace, dns, http_timing}, diagnosis[{code, summary}]`.
Each check carries a `status` and numeric metrics. Fields may be added, never
renamed. The Rust rewrite must not orphan that history.

## Decision

The Rust core reads and writes exactly this format, in the same file
(`runs.jsonl`) and, on macOS, the same directory as bash. Readers ignore unknown
fields; writers only add fields. Each record is written as one buffer ending in
a newline, with the file opened in append mode, so concurrent CLI and app runs
do not interleave. No index, no database.

## Alternatives rejected

1. SQLite (via `rusqlite`). Gives queries and indexes, but needs a schema
   migration story, a native build on Windows, and a separate reader for the
   bash tool; the data is a few hundred lines per month. Rejected as ceremony
   for a file `grep` can read.
2. One JSON file per run in a directory. Easy to write, but listing and
   sorting thousands of files is slower than one line scan, and bash already
   chose a single file. Rejected for consistency with `main`.
3. Keep the daily text logs only. Not machine-readable; the app would parse
   terminal text. Rejected.

## Consequences

- `nettest history` can be implemented as "read file, parse lines, skip bad
  lines with a warning" in both tools.
- The diagnosis is stored with the run, so old runs show the diagnosis of the
  rules at that time, not the current rules. Accepted; re-running the rules
  over history is a possible later command.
- File growth is linear and small; rotation is not needed for years. If it ever
  is, rotate by renaming, never by rewriting.

## Non-goals

- Querying across machines or syncing.
- Backward migrations: `schema` only increases, and a reader supports every
  schema it knows.

## What would reopen this

- A need for cross-run aggregates in the app that scanning a few MB of JSONL
  cannot serve in under a second.
- A second writer that cannot do atomic appends (for example a sandboxed app
  without write access to the log directory).
