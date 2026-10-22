#!/usr/bin/env bash
# `nettest history` on a hand-written runs.jsonl: both engines must agree, --since
# must filter, rows must be colored on request. Timestamps use UTC-5 and the test
# pins TZ so the python engine (which converts to local time) matches awk.
set -u
# shellcheck source=tests/lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

DATA=$(mktemp -d "${TMPDIR:-/tmp}/nt-test-hist.XXXXXX")
trap 'rm -rf "$DATA"' EXIT
export NETTEST_DATA_DIR="$DATA" NO_COLOR=1 TZ=America/Bogota

now=$(date '+%Y-%m-%dT%H:%M:%S-05:00')
cat > "$DATA/runs.jsonl" <<R
{"schema":1,"ts":"2026-10-22T09:00:00-05:00","tool":"nettest-bash","tool_version":"0.2.0","os":"macos","mode":"full","duration_ms":20000,"exit":0,"checks":{"local_network":{"status":"PASS","interface":"en0","gateway":"192.168.1.1","link_kind":"wired","reachable":true,"loss_pct":0.0,"avg_ms":0.67},"ipv4_reachability":{"status":"PASS","icmp":{"ok":3,"total":3,"targets":[]},"https":{"ok":4,"total":4,"targets":[]}},"ipv6_reachability":{"status":"PASS","global_address":true,"icmp":{"ok":2,"total":2,"targets":[]},"https":{"ok":2,"total":2,"targets":[]}},"dns":{"status":"PASS","system":{"status":"PASS","max_ms":43,"lookups":[]},"servers":[],"primary_timeouts":[]}},"diagnosis":[{"code":"no_problem","summary":"No problem detected right now."}]}
{"schema":1,"ts":"2026-10-22T10:58:03-05:00","tool":"nettest-bash","tool_version":"0.2.0","os":"macos","mode":"full","duration_ms":21034,"exit":1,"checks":{"local_network":{"status":"PASS","interface":"en0","gateway":"192.168.1.1","link_kind":"wired","reachable":true,"loss_pct":0.0,"avg_ms":6.2},"ipv4_reachability":{"status":"FAIL","icmp":{"ok":1,"total":3,"targets":[]},"https":{"ok":0,"total":4,"targets":[]}},"ipv6_reachability":{"status":"FAIL","global_address":true,"icmp":{"ok":2,"total":2,"targets":[]},"https":{"ok":0,"total":2,"targets":[]}},"dns":{"status":"WARN","system":{"status":"WARN","max_ms":5022,"lookups":[]},"servers":[],"primary_timeouts":[]}},"diagnosis":[{"code":"isp_down","summary":"x"},{"code":"ipv4_degraded","summary":"y"}]}
{"schema":1,"ts":"$now","tool":"nettest-bash","tool_version":"0.2.0","os":"macos","mode":"single","duration_ms":812,"exit":0,"checks":{"dns":{"status":"WARN","system":{"status":"WARN","max_ms":1204,"lookups":[]},"servers":[],"primary_timeouts":[]}},"diagnosis":[{"code":"system_dns_slow","summary":"z"}]}
R

run() { NETTEST_HISTORY_ENGINE="$1" "$ROOT/bin/nettest" history "${@:2}"; }

for engine in awk python; do
  if [ "$engine" = python ] && ! command -v python3 >/dev/null 2>&1; then continue; fi
  T_NAME="history ($engine)"
  out=$(run "$engine")
  assert_contains "$out" "TIME" "header"
  assert_contains "$out" "2026-10-22 09:00:00  PASS       0.7       4/4       2/2      43  -" "healthy row"
  assert_contains "$out" "2026-10-22 10:58:03  FAIL       6.2       0/4       0/2    5022  isp_down,ipv4_degraded" "failing row"
  assert_contains "$out" "WARN         -         -         -    1204  system_dns_slow" "single-check row shows dashes"
  assert_eq "$(run "$engine" 1 | wc -l | tr -d ' ')" "2" "N=1 prints header plus one row"
  since=$(run "$engine" --since 10m)
  assert_contains "$since" "system_dns_slow" "--since keeps the new run"
  assert_not_contains "$since" "isp_down" "--since drops old runs"
done

if command -v python3 >/dev/null 2>&1; then
  T_NAME="history engines agree"
  assert_eq "$(run awk)" "$(run python)" "identical output"
fi

T_NAME="history color"
# shellcheck source=lib/common.sh
. "$ROOT/lib/common.sh"
# shellcheck source=lib/record.sh
. "$ROOT/lib/record.sh"
# shellcheck source=lib/history.sh
. "$ROOT/lib/history.sh"
red=$(printf '\033[31m'); yel=$(printf '\033[33m')
out=$(awk -v n=20 -v color=1 -v cutoff="" "$NT_HISTORY_AWK" "$DATA/runs.jsonl")
assert_contains "$out" "${red}2026-10-22 10:58:03" "awk: FAIL row is red"
assert_contains "$out" "${yel}" "awk: WARN row is yellow"
if command -v python3 >/dev/null 2>&1; then
  out=$(python3 -I -c "$NT_HISTORY_PY" "$DATA/runs.jsonl" 20 0 1)
  assert_contains "$out" "${red}2026-10-22 10:58:03" "python: FAIL row is red"
  assert_contains "$out" "${yel}" "python: WARN row is yellow"
fi

T_NAME="history errors"
assert_contains "$("$ROOT/bin/nettest" history --since 5x 2>&1)" "bad --since" "rejects a bad duration"
empty=$(mktemp -d "${TMPDIR:-/tmp}/nt-test-empty.XXXXXX")
assert_contains "$(NETTEST_DATA_DIR="$empty" "$ROOT/bin/nettest" history)" "No runs recorded yet" "no file yet"
rm -rf "$empty"

t_finish
