#!/usr/bin/env bash
# shellcheck shell=bash
# Run record: build one JSON line per nettest run and append it to runs.jsonl.
# Source this file after lib/common.sh. The format is documented in
# docs/run-record.md and is a contract (schema 1): never rename a field.
#
# JSON is built in pure bash (no jq). Values come from the KEY=value results
# the checks recorded in $NT_RESULTS.

NT_SCHEMA=1

nt_data_dir() { printf '%s' "${NETTEST_DATA_DIR:-$HOME/Library/Logs/network-provider-tests}"; }

# --- JSON primitives -----------------------------------------------------------
# json_str <string>: a quoted, escaped JSON string; `null` if the argument is empty.
json_str() {
  local s="${1-}"
  if [ -z "$s" ]; then printf 'null'; return; fi
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  s=${s//$'\r'/\\r}
  s=${s//$'\t'/\\t}
  case "$s" in
    *[[:cntrl:]]*) s=$(printf '%s' "$s" | LC_ALL=C tr -d '\000-\037\177') ;;
  esac
  printf '"%s"' "$s"
}

# json_strv <string>: like json_str but an empty string stays "".
json_strv() { if [ -z "${1-}" ]; then printf '""'; else json_str "$1"; fi; }

# json_num <value>: the value if it is a valid JSON number, else `null`.
json_num() {
  local re='^(0|[1-9][0-9]*)(\.[0-9]+)?$'
  if [[ "${1-}" =~ $re ]]; then printf '%s' "$1"; else printf 'null'; fi
}

# json_bool <0|1>: true / false, `null` for anything else.
json_bool() {
  case "${1-}" in 1) printf 'true' ;; 0) printf 'false' ;; *) printf 'null' ;; esac
}

# json_strlist <space-separated words>: ["a","b"]
json_strlist() {
  local out="" w
  for w in ${1-}; do out="$out${out:+,}$(json_str "$w")"; done
  printf '[%s]' "$out"
}

# json_ping_list <KEY>: array of {host,sent,received,loss_pct,avg_ms} from KEY=host|sent|recv|loss|avg lines.
json_ping_list() {
  local out="" host sent recv loss avg
  while IFS='|' read -r host sent recv loss avg; do
    [ -n "$host" ] || continue
    out="$out${out:+,}{\"host\":$(json_str "$host"),\"sent\":$(json_num "$sent"),\"received\":$(json_num "$recv"),\"loss_pct\":$(json_num "$loss"),\"avg_ms\":$(json_num "$avg")}"
  done < <(nt_get_all "$1")
  printf '[%s]' "$out"
}

# json_https_list <KEY>: array of {url,ok,http_code,time_ms,error} from KEY=url|ok|code|ms|error lines.
json_https_list() {
  local out="" url ok code ms err
  while IFS='|' read -r url ok code ms err; do
    [ -n "$url" ] || continue
    out="$out${out:+,}{\"url\":$(json_str "$url"),\"ok\":$(json_bool "$ok"),\"http_code\":$(json_num "$code"),\"time_ms\":$(json_num "$ms"),\"error\":$(json_str "$err")}"
  done < <(nt_get_all "$1")
  printf '[%s]' "$out"
}

# json_reach_block <prefix V4|V6>: {"icmp":{...},"https":{...}} members (without braces).
json_reach_block() {
  local p="$1"
  printf '"icmp":{"ok":%s,"total":%s,"targets":%s},"https":{"ok":%s,"total":%s,"targets":%s}' \
    "$(json_num "$(nt_get "${p}_ICMP_OK")")" "$(json_num "$(nt_get "${p}_ICMP_TOTAL")")" "$(json_ping_list "${p}_PING")" \
    "$(json_num "$(nt_get "${p}_HTTPS_OK")")" "$(json_num "$(nt_get "${p}_HTTPS_TOTAL")")" "$(json_https_list "${p}_HTTPS")"
}

# --- per-check objects ---------------------------------------------------------
# Each prints one JSON object. "status" is always the first member; a check that
# recorded no status (it crashed) is reported as FAIL.
rec_status() { local s; s=$(nt_get "STATUS_$1"); json_str "${s:-FAIL}"; }

rec_local_network() {
  printf '{"status":%s,"interface":%s,"gateway":%s,"link_kind":%s,"reachable":%s,"loss_pct":%s,"avg_ms":%s}' \
    "$(rec_status local_network)" "$(json_str "$(nt_get GW_IF)")" "$(json_str "$(nt_get GW_IP)")" \
    "$(json_str "$(nt_get GW_KIND)")" "$(json_bool "$(nt_get GW_OK)")" \
    "$(json_num "$(nt_get GW_LOSS)")" "$(json_num "$(nt_get GW_AVG)")"
}

rec_vpn_detection() {
  printf '{"status":%s,"tunnels":%s,"primary_via_vpn":%s,"primary_interface":%s,"primary_servers":%s}' \
    "$(rec_status vpn_detection)" "$(json_strlist "$(nt_get VPN_TUNNELS)")" \
    "$(json_bool "$(nt_get DNS_PRIMARY_VIA_VPN)")" "$(json_str "$(nt_get DNS_PRIMARY_IF)")" \
    "$(json_strlist "$(nt_get DNS_PRIMARY_SERVERS)")"
}

rec_ipv4_reachability() {
  printf '{"status":%s,%s}' "$(rec_status ipv4_reachability)" "$(json_reach_block V4)"
}

rec_ipv6_reachability() {
  printf '{"status":%s,"global_address":%s,%s}' "$(rec_status ipv6_reachability)" \
    "$(json_bool "$(nt_get V6_GLOBAL)")" "$(json_reach_block V6)"
}

rec_route_trace() {
  local hops="" n ip ms
  while IFS='|' read -r n ip ms; do
    [ -n "$n" ] || continue
    hops="$hops${hops:+,}{\"hop\":$(json_num "$n"),\"ip\":$(json_str "$ip"),\"ms\":$(json_num "$ms")}"
  done < <(nt_get_all TRACE_HOP)
  printf '{"status":%s,"target":%s,"reached":%s,"last_hop":%s,"last_ip":%s,"hop_count":%s,"private_hops":%s,"hops":[%s]}' \
    "$(rec_status route_trace)" "$(json_str "$(nt_get TRACE_TARGET)")" "$(json_bool "$(nt_get TRACE_REACHED)")" \
    "$(json_num "$(nt_get TRACE_LAST_HOP)")" "$(json_str "$(nt_get TRACE_LAST_IP)")" \
    "$(json_num "$(nt_get TRACE_HOPS)")" "$(json_num "$(nt_get TRACE_INTERNAL_HOPS)")" "$hops"
}

rec_dns() {
  local lookups="" servers="" name ms rc group server res val via label out rcode msv
  while IFS='|' read -r name ms rc; do
    [ -n "$name" ] || continue
    lookups="$lookups${lookups:+,}{\"name\":$(json_str "$name"),\"ms\":$(json_num "$ms"),\"rcode\":$(json_str "$rc")}"
  done < <(nt_get_all DNS_SYS)
  while IFS='|' read -r group server res val via label; do
    [ -n "$server" ] || continue
    msv=null; rcode=null
    case "$res" in
      OK) out=ok; msv=$(json_num "$val") ;;
      TIMEOUT) out=timeout ;;
      SKIP) out=skipped ;;
      *) out=error; rcode=$(json_str "$val") ;;
    esac
    servers="$servers${servers:+,}{\"group\":$(json_str "$group"),\"ip\":$(json_str "$server"),\"result\":\"$out\",\"ms\":$msv,\"rcode\":$rcode,\"via_vpn\":$(json_bool "$via"),\"label\":$(json_str "$label")}"
  done < <(nt_get_all DNS_SERVER)
  printf '{"status":%s,"system":{"status":%s,"max_ms":%s,"lookups":[%s]},"servers":[%s],"primary_timeouts":%s}' \
    "$(rec_status dns)" "$(json_str "$(nt_get DNS_SYS_STATUS)")" "$(json_num "$(nt_get DNS_SYS_MAX_MS)")" \
    "$lookups" "$servers" "$(json_strlist "$(nt_get DNS_PRIMARY_TIMEOUTS)")"
}

rec_http_timing() {
  local probes="" url fam d c t f tot code rc err
  while IFS='|' read -r url fam d c t f tot code; do
    [ -n "$url" ] || continue
    probes="$probes${probes:+,}{\"url\":$(json_str "$url"),\"ok\":true,\"family\":$(json_str "$fam"),\"dns_ms\":$(json_num "$d"),\"connect_ms\":$(json_num "$c"),\"tls_ms\":$(json_num "$t"),\"ttfb_ms\":$(json_num "$f"),\"total_ms\":$(json_num "$tot"),\"http_code\":$(json_num "$code")}"
  done < <(nt_get_all HTTP_TIMING)
  while IFS='|' read -r url rc err; do
    [ -n "$url" ] || continue
    probes="$probes${probes:+,}{\"url\":$(json_str "$url"),\"ok\":false,\"error\":$(json_str "$err")}"
  done < <(nt_get_all HTTP_FAIL)
  printf '{"status":%s,"failed":%s,"total":%s,"slow_phase":%s,"probes":[%s]}' \
    "$(rec_status http_timing)" "$(json_num "$(nt_get HTTP_FAILED)")" "$(json_num "$(nt_get HTTP_TOTAL)")" \
    "$(json_str "$(nt_get HTTP_SLOW_PHASE)")" "$probes"
}

# --- the record ----------------------------------------------------------------
# nt_iso_now: local time as ISO 8601 with a colon in the offset.
nt_iso_now() { date '+%Y-%m-%dT%H:%M:%S%z' | sed 's/\([0-9][0-9]\)$/:\1/'; }

# nt_build_record <mode> <duration_ms> <exit> <check name>...
# Prints the JSON line (no trailing newline) for the results in $NT_RESULTS.
nt_build_record() {
  local mode="$1" dur="$2" rc="$3" os checks="" name key code summary diag=""
  shift 3
  case "$(uname -s)" in Darwin) os=macos ;; *) os=$(uname -s | tr '[:upper:]' '[:lower:]') ;; esac
  for name in "$@"; do
    key=$(printf '%s' "$name" | tr '-' '_')
    case "$key" in
      local_network|vpn_detection|ipv4_reachability|ipv6_reachability|route_trace|dns|http_timing)
        checks="$checks${checks:+,}\"$key\":$("rec_$key")" ;;
    esac
  done
  while IFS='|' read -r code summary; do
    [ -n "$code" ] || continue
    diag="$diag${diag:+,}{\"code\":$(json_str "$code"),\"summary\":$(json_strv "$summary")}"
  done < <(nt_get_all DIAG)
  printf '{"schema":%s,"ts":"%s","tool":"nettest-bash","tool_version":"%s","os":"%s","mode":"%s","duration_ms":%s,"exit":%s,"checks":{%s},"diagnosis":[%s]}' \
    "$NT_SCHEMA" "$(nt_iso_now)" "$NT_VERSION" "$os" "$mode" "$(json_num "$dur")" "$(json_num "$rc")" "$checks" "$diag"
}

# nt_append_record <line>: append to $NETTEST_DATA_DIR/runs.jsonl. A failure to write
# is reported but never fails the run.
nt_append_record() {
  local dir; dir=$(nt_data_dir)
  if ! mkdir -p "$dir" 2>/dev/null || ! printf '%s\n' "$1" >> "$dir/runs.jsonl" 2>/dev/null; then
    echo "nettest: could not write the run record to $dir/runs.jsonl" >&2
    return 1
  fi
}
