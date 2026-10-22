#!/usr/bin/env bash
# shellcheck shell=bash
# `nettest history`: print recent runs from runs.jsonl as a table.
# Source this file after lib/common.sh and lib/record.sh.
#
# (The scripts below are loaded with `read -d ''`: bash 3.2, the macOS default,
# mis-parses heredocs inside $( ).)
#
# Engine: python3 when available (robust JSON parsing, any key order or
# whitespace), otherwise awk (relies on the layout written by this tool). Set
# NETTEST_HISTORY_ENGINE=awk or =python to force one.

nt_history_usage() {
  cat <<'USAGE'
Usage: nettest history [N] [--since DURATION]

  N                 show the last N runs (default 20)
  --since DURATION  only runs newer than DURATION: 30m, 2h, 1d, 1w

Reads $NETTEST_DATA_DIR/runs.jsonl (default ~/Library/Logs/network-provider-tests).
Rows are colored when stdout is a terminal: FAIL red, WARN yellow.
USAGE
}

# nt_duration_seconds <30m|2h|1d|1w>: seconds, empty on bad input.
nt_duration_seconds() {
  local v="$1" n u
  case "$v" in
    [0-9]*[smhdw]) ;;
    *) return 1 ;;
  esac
  n="${v%?}"; u="${v#"$n"}"
  case "$n" in *[!0-9]*|'') return 1 ;; esac
  case "$u" in
    s) echo "$n" ;; m) echo $((n * 60)) ;; h) echo $((n * 3600)) ;;
    d) echo $((n * 86400)) ;; w) echo $((n * 604800)) ;;
  esac
}

read -r -d '' NT_HISTORY_PY <<'PYEOF'
import datetime, json, sys

path, n, since, color = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4] == "1"
FMT = "%-19s  %-6s  %6s  %8s  %8s  %6s  %s"
now = datetime.datetime.now(datetime.timezone.utc)
rows = []
for line in open(path, encoding="utf-8", errors="replace"):
    line = line.strip()
    if not line:
        continue
    try:
        r = json.loads(line)
        ts = datetime.datetime.fromisoformat(r["ts"].replace("Z", "+00:00"))
    except (ValueError, KeyError, TypeError, AttributeError):
        continue
    if since and (now - ts).total_seconds() > since:
        continue
    checks = r.get("checks") or {}
    statuses = [(c or {}).get("status") for c in checks.values()]
    if r.get("exit", 0) != 0 or "FAIL" in statuses:
        res = "FAIL"
    elif "WARN" in statuses:
        res = "WARN"
    else:
        res = "PASS"

    def dig(*keys):
        v = r
        for k in keys:
            v = v.get(k) if isinstance(v, dict) else None
        return v

    def ratio(check):
        ok, tot = dig("checks", check, "https", "ok"), dig("checks", check, "https", "total")
        return "-" if ok is None or tot is None else "%d/%d" % (ok, tot)

    gw = dig("checks", "local_network", "avg_ms")
    dns = dig("checks", "dns", "system", "max_ms")
    codes = [d.get("code", "?") for d in (r.get("diagnosis") or []) if d.get("code") != "no_problem"]
    rows.append((res, FMT % (
        ts.astimezone().strftime("%Y-%m-%d %H:%M:%S"), res,
        "-" if gw is None else "%.1f" % gw, ratio("ipv4_reachability"), ratio("ipv6_reachability"),
        "-" if dns is None else "%d" % dns, ",".join(codes) or "-")))

print(FMT % ("TIME", "RESULT", "GW_MS", "V4_HTTPS", "V6_HTTPS", "DNS_MS", "DIAGNOSIS"))
for res, text in rows[-n:]:
    if color and res == "FAIL":
        text = "\033[31m" + text + "\033[0m"
    elif color and res == "WARN":
        text = "\033[33m" + text + "\033[0m"
    print(text)
if not rows:
    print("(no runs in range)")
PYEOF

read -r -d '' NT_HISTORY_AWK <<'AWKEOF'
function sect(s, key,   i, rest, j, k, p) {
  i = index(s, "\"" key "\":{")
  if (!i) return ""
  rest = substr(s, i + length(key) + 4)
  j = 0
  for (k = 1; k <= nmark; k++) {
    p = index(rest, mark[k])
    if (p && (j == 0 || p < j)) j = p
  }
  return j ? substr(rest, 1, j - 1) : rest
}
function grab(s, re,   v) {
  if (!match(s, re)) return "-"
  v = substr(s, RSTART, RLENGTH); sub(/^"[a-z_]+":/, "", v); gsub(/"/, "", v)
  return v
}
function ratio(s,   a, b) {
  if (!match(s, /"https":[{]"ok":[0-9]+,"total":[0-9]+/)) return "-"
  a = substr(s, RSTART, RLENGTH)
  gsub(/[^0-9,]/, "", a); sub(/,/, "/", a)
  return a
}
BEGIN {
  FMT = "%-19s  %-6s  %6s  %8s  %8s  %6s  %s\n"
  nmark = split("\"local_network\":{ \"vpn_detection\":{ \"ipv4_reachability\":{ \"ipv6_reachability\":{ \"route_trace\":{ \"dns\":{ \"http_timing\":{ \"diagnosis\":[", mark, " ")
  count = 0
}
/^[ \t]*[{]/ {
  ts = grab($0, "\"ts\":\"[^\"]*\"")
  if (ts == "-") next
  shown = substr(ts, 1, 10) " " substr(ts, 12, 8)
  if (cutoff != "" && substr(ts, 1, 19) < cutoff) next
  res = "PASS"
  if (grab($0, "\"exit\":[0-9]+") != "0" || index($0, "\"status\":\"FAIL\"")) res = "FAIL"
  else if (index($0, "\"status\":\"WARN\"")) res = "WARN"
  gw = grab(sect($0, "local_network"), "\"avg_ms\":[0-9.]+")
  if (gw != "-") gw = sprintf("%.1f", gw)
  v4 = ratio(sect($0, "ipv4_reachability"))
  v6 = ratio(sect($0, "ipv6_reachability"))
  dns = grab(sect($0, "dns"), "\"max_ms\":[0-9]+")
  codes = ""
  d = $0; i = index(d, "\"diagnosis\":[")
  if (i) d = substr(d, i); else d = ""
  while (match(d, /"code":"[^"]*"/)) {
    c = substr(d, RSTART + 8, RLENGTH - 9)
    if (c != "no_problem") codes = codes (codes == "" ? "" : ",") c
    d = substr(d, RSTART + RLENGTH)
  }
  if (codes == "") codes = "-"
  count++
  row[count] = sprintf(FMT, shown, res, gw, v4, v6, dns, codes)
  sub(/\n$/, "", row[count])
  rcls[count] = res
}
END {
  printf FMT, "TIME", "RESULT", "GW_MS", "V4_HTTPS", "V6_HTTPS", "DNS_MS", "DIAGNOSIS"
  first = count - n + 1; if (first < 1) first = 1
  for (i = first; i <= count; i++) {
    if (color && rcls[i] == "FAIL") print "\033[31m" row[i] "\033[0m"
    else if (color && rcls[i] == "WARN") print "\033[33m" row[i] "\033[0m"
    else print row[i]
  }
  if (count == 0) print "(no runs in range)"
}
AWKEOF

# nt_history [N] [--since DURATION]
nt_history() {
  local n=20 since="" secs=0 file color=0 cutoff=""
  while [ $# -gt 0 ]; do
    case "$1" in
      -h|--help) nt_history_usage; return 0 ;;
      --since)
        shift
        since="${1:-}"
        if [ -z "$since" ]; then echo "nettest history: --since needs a value (30m, 2h, 1d, 1w)" >&2; return 2; fi
        ;;
      --since=*) since="${1#--since=}" ;;
      *[!0-9]*|'') echo "nettest history: unexpected argument '$1'" >&2; nt_history_usage >&2; return 2 ;;
      *) n="$1" ;;
    esac
    shift
  done
  if [ -n "$since" ]; then
    secs=$(nt_duration_seconds "$since") || {
      echo "nettest history: bad --since value '$since' (use 30m, 2h, 1d, 1w)" >&2
      return 2
    }
  fi
  file="$(nt_data_dir)/runs.jsonl"
  if [ ! -s "$file" ]; then
    echo "No runs recorded yet in $file. Run nettest first."
    return 0
  fi
  [ -n "$C_RED" ] && color=1   # set by common.sh only on a terminal without NO_COLOR

  local engine="${NETTEST_HISTORY_ENGINE:-}"
  if [ -z "$engine" ]; then
    if nt_have python3 && python3 -c 'import sys' >/dev/null 2>&1; then engine='python'; else engine='awk'; fi
  fi
  if [ "$engine" = python ]; then
    python3 -I -c "$NT_HISTORY_PY" "$file" "$n" "$secs" "$color"
  else
    if [ "$secs" -gt 0 ]; then
      cutoff=$(date -v-"${secs}S" '+%Y-%m-%dT%H:%M:%S' 2>/dev/null) || cutoff=""
    fi
    awk -v n="$n" -v color="$color" -v cutoff="$cutoff" "$NT_HISTORY_AWK" "$file"
  fi
}
