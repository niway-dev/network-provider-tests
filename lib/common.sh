#!/usr/bin/env bash
# shellcheck shell=bash disable=SC2034  # variables are consumed by the scripts that source this file
# Shared helpers for network-provider-tests. Source this file; do not execute it.
#
# Conventions:
#   - A failed probe is data, not an error: nothing here uses `set -e`.
#   - Result lines are recorded as KEY=value in the file named by $NT_RESULTS so
#     that `nettest` can combine the output of every check into a diagnosis.
#   - Do not call the nt_pass/nt_warn/nt_fail helpers from a subshell (pipes,
#     command substitution): the counters would be lost. Use `done < <(cmd)`.

NT_VERSION="0.2.1"

NT_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NT_ROOT="$(cd "$NT_LIB_DIR/.." && pwd)"

# --- colors (only when stdout is a terminal and NO_COLOR is unset) -----------
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'
  C_BLUE=$'\033[34m'; C_GRAY=$'\033[90m'; C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'
else
  C_RED=''; C_GREEN=''; C_YELLOW=''; C_BLUE=''; C_GRAY=''; C_BOLD=''; C_RESET=''
fi

# --- scratch space and result file -------------------------------------------
NT_TMP="$(mktemp -d "${TMPDIR:-/tmp}/nettest.XXXXXX")"
trap 'rm -rf "$NT_TMP"' EXIT
if [ -z "${NT_RESULTS:-}" ]; then
  NT_RESULTS="$NT_TMP/results"
  export NT_RESULTS
fi
: >> "$NT_RESULTS"

NT_FAILS=0
NT_WARNS=0
NT_PASSES=0
NT_INFOS=0
NT_SKIPS=0
NT_CHECK_NAME=""

# --- output helpers -----------------------------------------------------------
nt_tag() { # color label text
  printf '  %s%-4s%s  %s\n' "$1" "$2" "$C_RESET" "$3"
}
nt_pass() { NT_PASSES=$((NT_PASSES + 1)); nt_tag "$C_GREEN" PASS "$*"; }
nt_info() { NT_INFOS=$((NT_INFOS + 1)); nt_tag "$C_BLUE" INFO "$*"; }
nt_skip() { NT_SKIPS=$((NT_SKIPS + 1)); nt_tag "$C_GRAY" SKIP "$*"; }
nt_warn() { NT_WARNS=$((NT_WARNS + 1)); nt_tag "$C_YELLOW" WARN "$*"; }
nt_fail() { NT_FAILS=$((NT_FAILS + 1)); nt_tag "$C_RED" FAIL "$*"; }
nt_hint() { printf '        %s-> %s%s\n' "$C_GRAY" "$*" "$C_RESET"; }
nt_dim()  { printf '        %s%s%s\n' "$C_GRAY" "$*" "$C_RESET"; }

# nt_begin <name> <title>: print the check header and remember the check name.
nt_begin() {
  NT_CHECK_NAME="$1"
  printf '\n%s== %s ==%s\n' "$C_BOLD" "$2" "$C_RESET"
}

# nt_end: record the check status and exit 1 if any FAIL was printed.
# Status: FAIL if any line was FAIL, else WARN if any was WARN, else PASS if the
# check passed anything, else SKIP if it only skipped, else INFO if it only
# informed. (SKIP/INFO lines next to PASS lines do not downgrade a check.)
nt_end() {
  local status=PASS key
  if [ "$NT_FAILS" -gt 0 ]; then status=FAIL
  elif [ "$NT_WARNS" -gt 0 ]; then status=WARN
  elif [ "$NT_PASSES" -gt 0 ]; then status=PASS
  elif [ "$NT_SKIPS" -gt 0 ]; then status=SKIP
  elif [ "$NT_INFOS" -gt 0 ]; then status=INFO
  fi
  key="STATUS_$(printf '%s' "$NT_CHECK_NAME" | tr '-' '_')"
  nt_rec "$key" "$status"
  [ "$NT_FAILS" -gt 0 ] && exit 1
  exit 0
}

# --- result recording ---------------------------------------------------------
nt_rec() { # KEY value...
  local key="$1"; shift
  printf '%s=%s\n' "$key" "$*" >> "$NT_RESULTS"
}
nt_get() { # KEY -> last recorded value (empty if none)
  grep "^$1=" "$NT_RESULTS" 2>/dev/null | tail -n 1 | cut -d= -f2-
}
nt_get_all() { # KEY -> every recorded value, one per line
  grep "^$1=" "$NT_RESULTS" 2>/dev/null | cut -d= -f2-
}

# --- tool availability --------------------------------------------------------
nt_have() { command -v "$1" >/dev/null 2>&1; }

# nt_need <tool>...: returns 0 if all exist, otherwise prints SKIP and returns 1.
nt_need() {
  local t
  for t in "$@"; do
    if ! nt_have "$t"; then
      nt_skip "required tool '$t' not found on this machine"
      return 1
    fi
  done
  return 0
}

# --- portable timeout (macOS has no `timeout`) --------------------------------
# nt_run_timeout <seconds> <cmd> [args...]
# Runs cmd, killing it after <seconds>. Returns cmd's status, or 143 on timeout.
nt_run_timeout() {
  local secs="$1" pid watcher rc
  shift
  "$@" &
  pid=$!
  ( sleep "$secs"; kill -TERM "$pid" 2>/dev/null ) >/dev/null 2>&1 &
  watcher=$!
  wait "$pid" 2>/dev/null
  rc=$?
  pkill -P "$watcher" 2>/dev/null
  kill "$watcher" 2>/dev/null
  wait "$watcher" 2>/dev/null
  return "$rc"
}

# --- time ---------------------------------------------------------------------
# nt_now_ms: current time in milliseconds (perl ships with macOS).
nt_now_ms() {
  if nt_have perl; then
    perl -MTime::HiRes=time -e 'printf "%d\n", time*1000'
  else
    echo "$(date +%s)000"
  fi
}

# nt_gt <a> <b>: true if a > b (floating point).
nt_gt() { awk -v a="$1" -v b="$2" 'BEGIN { exit !(a + 0 > b + 0) }'; }

# nt_secs_to_ms <seconds>: "0.594" -> "594" (integer milliseconds; empty on bad input).
nt_secs_to_ms() {
  case "${1:-}" in ''|*[!0-9.]*) echo ""; return ;; esac
  awk -v s="$1" 'BEGIN { printf "%d\n", s * 1000 + 0.5 }'
}

# --- ping ---------------------------------------------------------------------
# nt_ping <host> <count> [v6]
# Sets PING_SENT, PING_RECV, PING_LOSS (percent, number) and PING_AVG (ms, may be empty).
nt_ping() {
  local host="$1" count="${2:-5}" out
  if [ "${3:-}" = v6 ]; then
    out=$(nt_run_timeout $((count + 8)) ping6 -c "$count" -i 1 "$host" 2>&1)
  else
    out=$(nt_run_timeout $((count + 8)) ping -c "$count" -i 0.4 -W 1000 -t $((count + 5)) "$host" 2>&1)
  fi
  PING_SENT=$(printf '%s\n' "$out" | awk '/packets transmitted/ { print $1; exit }')
  PING_RECV=$(printf '%s\n' "$out" | awk '/packets transmitted/ { print $4; exit }')
  PING_LOSS=$(printf '%s\n' "$out" | sed -n 's/.* \([0-9.]*\)% packet loss.*/\1/p' | head -n 1)
  PING_AVG=$(printf '%s\n' "$out" | sed -n 's/.*= *[^/]*\/\([^/]*\)\/.*/\1/p' | head -n 1)
  : "${PING_SENT:=0}" "${PING_RECV:=0}" "${PING_LOSS:=100}"
}

# --- HTTP probe ---------------------------------------------------------------
# nt_http_probe <4|6|0> <url> [max-seconds]
# Prints one line: "OK <http_code> <seconds>" or "FAIL <curl_exit> <reason>".
nt_http_probe() {
  local fam="$1" url="$2" max="${3:-6}" flag out rc reason
  case "$fam" in 4) flag=-4 ;; 6) flag=-6 ;; *) flag= ;; esac
  # shellcheck disable=SC2086 # $flag is intentionally unquoted (may be empty)
  out=$(curl $flag -sS -o /dev/null -m "$max" -w '%{http_code} %{time_total}' "$url" 2>/dev/null)
  rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "OK $out"
  else
    case "$rc" in
      6) reason="could not resolve host" ;;
      7) reason="connection refused or unreachable" ;;
      28) reason="timed out after ${max}s" ;;
      35|51|58|60) reason="TLS handshake failed" ;;
      52|56) reason="connection dropped" ;;
      *) reason="curl error $rc" ;;
    esac
    echo "FAIL $rc $reason"
  fi
}

# --- network introspection ----------------------------------------------------
# nt_has_global_ipv6: true if any interface has a global unicast IPv6 address (2000::/3).
nt_has_global_ipv6() {
  ifconfig 2>/dev/null | awk '/^[[:space:]]+inet6 / { print $2 }' | grep -Eq '^[23]'
}

# nt_route_if <ip>: name of the interface macOS would use to reach <ip>.
nt_route_if() {
  local ip="$1" fam=""
  case "$ip" in
    *%*) printf '%s\n' "${ip#*%}"; return 0 ;;
    *:*) fam="-inet6" ;;
  esac
  # shellcheck disable=SC2086
  route -n get $fam "$ip" 2>/dev/null | awk '/interface:/ { print $2; exit }'
}

# nt_resolvers [scoped]: one line per resolver from `scutil --dns`.
# Format: section|index|domain|nameservers(space separated)|interface
# section is "main" or "scoped". Resolvers without nameservers (mDNS) are dropped.
nt_resolvers() {
  scutil --dns 2>/dev/null | awk '
    function emit() {
      if (idx != "" && ns != "") print section "|" idx "|" dom "|" ns "|" ifn
      idx = ""; dom = ""; ns = ""; ifn = ""
    }
    /^DNS configuration \(for scoped queries\)/ { emit(); section = "scoped"; next }
    /^DNS configuration/ { section = "main"; next }
    /^resolver #/ { emit(); idx = $2; sub("#", "", idx); next }
    idx != "" {
      key = $1
      val = $0; sub(/^[^:]*: */, "", val)
      if (key == "domain") dom = val
      else if ($1 ~ /^nameserver\[/) ns = (ns == "" ? val : ns " " val)
      else if (key == "if_index") { if (match(val, /\(.*\)/)) ifn = substr(val, RSTART + 1, RLENGTH - 2) }
    }
    END { emit() }
  '
}
