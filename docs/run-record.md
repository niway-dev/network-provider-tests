# Run record

Every `nettest` run, a full run or a single check, appends **one JSON line** to
`runs.jsonl`. This file is the history that `nettest history` reads and that
you can hand to an ISP as evidence.

**This format is a contract.** A future Rust implementation of nettest must keep
reading and writing it, so history recorded by the bash tool survives the
rewrite. Schema version 1 rules:

- The envelope fields (the top-level keys below) are fixed: no renames, no
  removals, no type changes.
- Per-check fields may only be **added**, never renamed or removed. A reader must
  ignore keys it does not know. A new check gets a new key under `checks`.
- Diagnosis `code` values are stable machine identifiers: never rename one.
  Adding a code is fine.
- A change that breaks any of this needs `"schema": 2`, and readers must keep
  handling schema 1 lines.

## Where it lives

`$NETTEST_DATA_DIR/runs.jsonl`. The default directory is
`~/Library/Logs/network-provider-tests`. The file is append-only, UTF-8, one
JSON object per line (JSON Lines), no pretty-printing. A line that does not
parse must be skipped by readers, not treated as fatal. The `nettest-log` text
logs (`YYYY-MM-DD.log`) live in the same directory by default and are separate.

## Envelope

```json
{"schema":1,"ts":"2026-10-22T10:58:03-05:00","tool":"nettest-bash","tool_version":"0.2.0","os":"macos","mode":"full","duration_ms":21034,"exit":0,"checks":{"local_network":{"status":"PASS"}},"diagnosis":[{"code":"ipv4_outage","summary":"..."}]}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `schema` | number | Schema version, `1`. |
| `ts` | string | Start-of-write time, local time, ISO 8601 with a numeric offset including the colon (`-05:00`). |
| `tool` | string | `nettest-bash` for this implementation. A Rust implementation uses its own name, for example `nettest-rs`. |
| `tool_version` | string | Version of the tool that wrote the line (`0.2.0`). |
| `os` | string | Lowercase OS name: `macos`. |
| `mode` | string | `full` (every check) or `single` (one check was selected). |
| `duration_ms` | number | Wall-clock duration of the run in milliseconds. |
| `exit` | number | The exit status of that run: `0` when no check reported `FAIL`, `1` otherwise. |
| `checks` | object | One entry per check that ran, keyed by the snake_case check name. A single-check run has one entry. |
| `diagnosis` | array | The findings printed under "Diagnosis": `{ "code": string, "summary": string }`. Never empty: a clean run has `no_problem`. |

Conventions in the per-check objects: times are milliseconds as numbers, loss is
a percentage as a number, `null` means "not measured or no answer", and a
missing key means the same as `null` (readers must tolerate both).

## Check status

Every check object starts with `status`, one of `PASS`, `INFO`, `SKIP`, `WARN`,
`FAIL`:

- `FAIL` if the check printed any FAIL line, else
- `WARN` if it printed any WARN line, else
- `PASS` if it printed any PASS line, else
- `SKIP` if it only skipped, else
- `INFO` if it only informed.

INFO and SKIP lines next to PASS lines do not downgrade a check. A check that
crashed and recorded no status is stored as `FAIL`.

## Checks

### `local_network` (check 01)

| Field | Type | Meaning |
| --- | --- | --- |
| `interface` | string or null | Interface of the default route (`en0`). |
| `gateway` | string or null | Default gateway IPv4 address. |
| `link_kind` | string or null | `wired` or `wifi`. |
| `reachable` | boolean or null | Whether the gateway answered ping at least once. `false` also when there is no default route. |
| `loss_pct` | number or null | Packet loss to the gateway, percent. |
| `avg_ms` | number or null | Average gateway round trip in ms. |

### `vpn_detection` (check 02)

| Field | Type | Meaning |
| --- | --- | --- |
| `tunnels` | string[] | `utun` interfaces that carry an IPv4 address. |
| `primary_via_vpn` | boolean or null | The primary DNS resolver (resolver #1) is reached through a tunnel. |
| `primary_interface` | string or null | Interface used to reach the primary resolver. |
| `primary_servers` | string[] | Name servers of resolver #1, in order. |

### `ipv4_reachability` (check 03) and `ipv6_reachability` (check 04)

Both have the same `icmp` and `https` members. IPv6 also has `global_address`
(boolean or null): whether this Mac has a global IPv6 address. When it is
`false` the probes are skipped and `icmp` / `https` have `null` counts and no
targets.

| Field | Type | Meaning |
| --- | --- | --- |
| `icmp.ok`, `icmp.total` | number or null | Ping targets that answered at least once, and targets tried. |
| `icmp.targets[]` | object | `host` (string), `sent`, `received` (numbers), `loss_pct` (number), `avg_ms` (number or null when nothing answered). |
| `https.ok`, `https.total` | number or null | HTTPS probes that completed, and probes tried. |
| `https.targets[]` | object | `url`, `ok` (boolean), `http_code` (number or null), `time_ms` (number or null), `error` (string or null, for example `timed out after 6s`). |

### `route_trace` (check 05)

| Field | Type | Meaning |
| --- | --- | --- |
| `target` | string or null | Address traced (`1.1.1.1`). |
| `reached` | boolean or null | The target answered. |
| `last_hop` | number or null | Highest hop number that answered; `0` when none did. |
| `last_ip` | string or null | Address of that hop. |
| `hop_count` | number or null | Hops listed by traceroute. |
| `private_hops` | number or null | Hops after the router in private or CGNAT ranges (likely the ISP's internal network). |
| `hops[]` | object | `hop` (number), `ip` (string, `null` for no reply), `ms` (number or null). |

### `dns` (check 06)

| Field | Type | Meaning |
| --- | --- | --- |
| `system.status` | string or null | `PASS`, `WARN` (some lookup over 1 s) or `FAIL` (a lookup failed or took over 3 s) for the lookups through the system resolver. |
| `system.max_ms` | number or null | Slowest system lookup, ms. This is the "system DNS worst" figure. |
| `system.lookups[]` | object | `name`, `ms` (number), `rcode` (`NOERROR`, or `NONE` when nothing answered). |
| `servers[]` | object | One entry per DNS server queried directly (below). |
| `primary_timeouts` | string[] | Addresses of primary-resolver servers that timed out. |

`servers[]` entries: `group` (`primary` for resolver #1, `physical` for the
physical interface's DNS, `public` for 1.1.1.1 / 8.8.8.8 / 9.9.9.9), `ip`,
`result` (`ok`, `timeout`, `error`, `skipped`), `ms` (number, **`null` for a
timeout**, error or skip), `rcode` (string, only when `result` is `error`),
`via_vpn` (boolean: the server is reached through a VPN tunnel), `label`
(human-readable description).

### `http_timing` (check 07)

| Field | Type | Meaning |
| --- | --- | --- |
| `failed`, `total` | number or null | Probes that failed, and probes tried. |
| `slow_phase` | string or null | Slowest phase of the last slow probe: `dns`, `connect`, `tls` or `ttfb`. |
| `probes[]` | object | Successful: `url`, `ok: true`, `family` (`ipv4` or `ipv6`), `dns_ms`, `connect_ms`, `tls_ms`, `ttfb_ms`, `total_ms`, `http_code`. Failed: `url`, `ok: false`, `error`. |

## Diagnosis codes

The `diagnosis[].code` values. The `summary` is the human sentence and may be
reworded; the `code` may not.

| Code | Meaning |
| --- | --- |
| `no_problem` | No problem detected. |
| `unmatched_warnings` | Some check warned or failed but no rule explains it. |
| `local_network_down` | This Mac cannot reach its own router. |
| `gateway_icmp_ignored` | The router ignores ping but traffic works. Harmless. |
| `ipv4_outage` | IPv4 HTTPS mostly fails while IPv6 works (often CGNAT trouble at the ISP). |
| `isp_down` | IPv4 and IPv6 both fail past the router. |
| `ipv4_outage_no_ipv6` | IPv4 fails and the network has no IPv6 to compare. |
| `ipv4_outage_unverified` | IPv4 fails and IPv6 was not tested (single-check run). |
| `ipv4_degraded` | Most public IPv4 DNS servers (1.1.1.1, 8.8.8.8, 9.9.9.9) time out. The IPv4 path is unreliable, and a failing primary or VPN resolver is probably a consequence. |
| `primary_dns_timeout_vpn` | The primary resolver times out, public resolvers answer, and the primary resolver is a VPN's. |
| `primary_dns_timeout` | Same, for a primary resolver that is not a VPN's. |
| `system_dns_slow` | System lookups are slow but every server answered the direct test. |
| `icmp_deprioritized` | Ping loss while HTTPS works. Harmless. |

## Reading it without tools

```sh
nettest history 50              # table of the last 50 runs
nettest history --since 2h      # only the last two hours
tail -n 1 ~/Library/Logs/network-provider-tests/runs.jsonl | python3 -m json.tool
```

Where `jq` is installed: `jq -r '[.ts, .exit, (.diagnosis | map(.code) | join(","))] | @tsv' runs.jsonl`.
