#!/usr/bin/env bash
# Builds run records from synthetic results (including awkward strings) and checks
# that every line is valid JSON with the fixed envelope. Needs python3, else skips.
set -u
# shellcheck source=tests/lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

if ! command -v python3 >/dev/null 2>&1; then
  echo "test-record.sh: python3 not found, skipping JSON validation"
  exit 0
fi

DATA=$(mktemp -d "${TMPDIR:-/tmp}/nt-test-data.XXXXXX")
RES="$DATA/results"
trap 'rm -rf "$DATA"' EXIT

# Source the libraries in this shell with a synthetic result file.
export NT_RESULTS="$RES" NO_COLOR=1 NETTEST_DATA_DIR="$DATA"
: > "$RES"
# shellcheck source=lib/common.sh
. "$ROOT/lib/common.sh"
# shellcheck source=lib/record.sh
. "$ROOT/lib/record.sh"
# shellcheck source=lib/diagnose.sh
. "$ROOT/lib/diagnose.sh"

fill_full() {
  cat > "$RES" <<'R'
STATUS_local_network=PASS
GW_IF=en0
GW_KIND=wired
GW_IP=192.168.1.1
GW_LOSS=0.0
GW_AVG=0.67
GW_OK=1
STATUS_vpn_detection=INFO
VPN_TUNNELS=utun4 utun5
DNS_PRIMARY_VIA_VPN=1
DNS_PRIMARY_IF=utun4
DNS_PRIMARY_SERVERS=198.51.100.1 198.51.100.2
STATUS_ipv4_reachability=FAIL
V4_PING=1.1.1.1|5|0|100.0|-
V4_PING=8.8.8.8|5|4|20.0|12.5
V4_ICMP_OK=1
V4_ICMP_TOTAL=2
V4_HTTPS=https://www.example.com|0|||timed out after 6s
V4_HTTPS=https://example.org|1|200|594|
V4_HTTPS_OK=1
V4_HTTPS_TOTAL=2
STATUS_ipv6_reachability=SKIP
V6_GLOBAL=0
STATUS_route_trace=WARN
TRACE_TARGET=1.1.1.1
TRACE_REACHED=0
TRACE_LAST_HOP=4
TRACE_LAST_IP=10.95.156.50
TRACE_HOPS=5
TRACE_INTERNAL_HOPS=2
TRACE_HOP=1|192.168.1.1|0.9
TRACE_HOP=5||
STATUS_dns=WARN
DNS_SYS=google.com|5022|NONE
DNS_SYS=github.com|6|NOERROR
DNS_SYS_STATUS=FAIL
DNS_SYS_MAX_MS=5022
DNS_SERVER=primary|198.51.100.1|OK|8|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=public|9.9.9.9|ERR|REFUSED|0|public
DNS_SERVER=physical|2001:db8::53|SKIP|0|0|physical interface en0
DNS_PRIMARY_TIMEOUTS=198.51.100.2
STATUS_http_timing=WARN
HTTP_TIMING=https://example.org|ipv4|2|1090|93|91|1731|200
HTTP_FAIL=https://bad.example|28|curl: (28) say "hello" \ back	tab
HTTP_FAILED=1
HTTP_TOTAL=2
HTTP_SLOW_PHASE=connect
DIAG=ipv4_outage|It's "quoted", has a back\slash and a	tab
DIAG=system_dns_slow|plain
R
}

ALL="local-network vpn-detection ipv4-reachability ipv6-reachability route-trace dns http-timing"
OUT="$DATA/runs.jsonl"

T_NAME="record: full run"
fill_full
# shellcheck disable=SC2086 # $ALL is a list of check names
nt_append_record "$(nt_build_record full 21034 1 $ALL)"
T_NAME="record: single-check run"
: > "$RES"
printf 'STATUS_dns=PASS\nDNS_SYS_STATUS=PASS\nDNS_SYS_MAX_MS=5\nDIAG=no_problem|No problem detected right now.\n' >> "$RES"
nt_append_record "$(nt_build_record single 812 0 dns)"
T_NAME="record: empty run (no results at all)"
: > "$RES"
# shellcheck disable=SC2086
nt_append_record "$(nt_build_record full 1 1 $ALL)"

assert_eq "$(wc -l < "$OUT" | tr -d ' ')" "3" "three lines appended"

T_NAME="record: every line is valid JSON"
if python3 -I -c 'import json,sys;[json.loads(l) for l in open(sys.argv[1])]' "$OUT"; then t_ok; else t_bad "invalid JSON in $OUT"; fi

T_NAME="record: envelope and values"
if python3 -I - "$OUT" <<'PY'
import json, sys
full, single, empty = [json.loads(l) for l in open(sys.argv[1])]
keys = ["schema", "ts", "tool", "tool_version", "os", "mode", "duration_ms", "exit", "checks", "diagnosis"]
for r in (full, single, empty):
    assert list(r.keys()) == keys, list(r.keys())
    assert r["schema"] == 1 and r["tool"] == "nettest-bash" and r["tool_version"] == "0.2.0"
    assert len(r["ts"]) == 25 and r["ts"][22] == ":", r["ts"]
assert full["mode"] == "full" and full["duration_ms"] == 21034 and full["exit"] == 1
assert single["mode"] == "single" and list(single["checks"]) == ["dns"]
c = full["checks"]
assert list(c) == ["local_network", "vpn_detection", "ipv4_reachability", "ipv6_reachability", "route_trace", "dns", "http_timing"]
assert c["local_network"] == {"status": "PASS", "interface": "en0", "gateway": "192.168.1.1", "link_kind": "wired", "reachable": True, "loss_pct": 0.0, "avg_ms": 0.67}, c["local_network"]
assert c["vpn_detection"]["tunnels"] == ["utun4", "utun5"] and c["vpn_detection"]["primary_via_vpn"] is True
v4 = c["ipv4_reachability"]
assert v4["icmp"]["targets"][0]["avg_ms"] is None and v4["icmp"]["targets"][1]["avg_ms"] == 12.5
assert v4["https"]["ok"] == 1 and v4["https"]["total"] == 2
assert v4["https"]["targets"][0]["ok"] is False and v4["https"]["targets"][0]["error"] == "timed out after 6s"
assert v4["https"]["targets"][1]["http_code"] == 200 and v4["https"]["targets"][1]["time_ms"] == 594
assert c["ipv6_reachability"]["status"] == "SKIP" and c["ipv6_reachability"]["global_address"] is False
rt = c["route_trace"]
assert rt["reached"] is False and rt["last_hop"] == 4 and rt["hops"][1] == {"hop": 5, "ip": None, "ms": None}
d = c["dns"]
assert d["system"]["max_ms"] == 5022 and d["system"]["lookups"][0]["rcode"] == "NONE"
s = d["servers"]
assert s[1]["result"] == "timeout" and s[1]["ms"] is None and s[1]["via_vpn"] is True
assert s[0]["result"] == "ok" and s[0]["ms"] == 8
assert s[2]["result"] == "error" and s[2]["rcode"] == "REFUSED" and s[3]["result"] == "skipped"
assert d["primary_timeouts"] == ["198.51.100.2"]
h = c["http_timing"]
assert h["probes"][0]["family"] == "ipv4" and h["probes"][0]["connect_ms"] == 1090
assert h["probes"][1]["ok"] is False and 'say "hello" \\ back\ttab' in h["probes"][1]["error"], h["probes"][1]
assert full["diagnosis"][0] == {"code": "ipv4_outage", "summary": 'It\'s "quoted", has a back\\slash and a\ttab'}, full["diagnosis"][0]
# A run with no results still parses and marks every check FAIL (it recorded no status).
assert all(v["status"] == "FAIL" for v in empty["checks"].values())
assert empty["diagnosis"] == []
PY
then t_ok; else t_bad "value assertions failed (see python error above)"; fi

T_NAME="record: unwritable data dir is not fatal"
if NETTEST_DATA_DIR="/dev/null/nope" nt_append_record '{"x":1}' 2>/dev/null; then t_bad "expected a non-zero return"; else t_ok; fi

t_finish
