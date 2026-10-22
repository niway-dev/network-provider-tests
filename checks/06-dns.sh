#!/usr/bin/env bash
# DESC: DNS timing through the system resolver, then each configured and public server directly
set -u
# shellcheck source=../lib/common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/common.sh"

nt_begin dns "06 DNS: system resolver and each server"

if ! nt_need dig scutil; then nt_end; fi

# ------------------------------------------------------------------------------
# (a) System resolver timing. dig follows the primary resolver (resolver #1) and
# falls back to the next server after its timeout, so wall-clock time reveals
# a dead primary even when the lookup eventually succeeds.
# ------------------------------------------------------------------------------
nt_info "(a) Lookup time through the system's primary resolver"
names="google.com github.com cloudflare.com wikipedia.org"
i=0
for n in $names; do
  i=$((i + 1))
  (
    t0=$(nt_now_ms)
    out=$(dig +tries=1 +time=3 "$n" 2>&1)
    t1=$(nt_now_ms)
    st=$(printf '%s\n' "$out" | sed -n 's/.*status: \([A-Z]*\).*/\1/p' | head -n 1)
    echo "$((t1 - t0)) ${st:-NONE}" > "$NT_TMP/s.$i"
  ) &
done
wait

sys_max=0; sys_status=PASS; i=0
for n in $names; do
  i=$((i + 1))
  read -r ms st < "$NT_TMP/s.$i"
  nt_rec DNS_SYS "$n|$ms|$st"
  if [ "$st" != NOERROR ]; then
    nt_fail "$n: no answer (${ms} ms, ${st})"
    sys_status=FAIL
  elif [ "$ms" -gt 3000 ]; then
    nt_fail "$n: ${ms} ms"
    sys_status=FAIL
  elif [ "$ms" -gt 1000 ]; then
    nt_warn "$n: ${ms} ms"
    [ "$sys_status" = PASS ] && sys_status=WARN
  else
    nt_pass "$n: ${ms} ms"
  fi
  [ "$ms" -gt "$sys_max" ] && sys_max=$ms
done
nt_rec DNS_SYS_STATUS "$sys_status"
nt_rec DNS_SYS_MAX_MS "$sys_max"
if [ "$sys_status" != PASS ]; then
  nt_hint "The primary resolver is timing out and the OS is falling back to the next server. Every lookup pays that wait. Part (b) shows which server is the culprit."
fi

# ------------------------------------------------------------------------------
# (b) Query each server directly.
# ------------------------------------------------------------------------------
nt_info "(b) Each DNS server queried directly (google.com, 2 s limit)"

# Build the server list: "group|server|label". A server appears once.
servers_file="$NT_TMP/servers"
: > "$servers_file"
seen=" "
add_server() { # group server label via_vpn
  case "$seen" in *" $2 "*) return ;; esac
  seen="$seen$2 "
  printf '%s|%s|%s|%s\n' "$1" "$2" "$3" "${4:-0}" >> "$servers_file"
}

primary_if=""
first=1
phys_done=0
while IFS='|' read -r section idx _ ns ifn; do
  if [ "$section" = main ] && [ "$first" -eq 1 ]; then
    first=0
    primary_if=$(nt_route_if "${ns%% *}")
    label="resolver #$idx, primary"
    primary_vpn=0
    case "$primary_if" in utun*) primary_vpn=1; label="$label, via VPN tunnel $primary_if" ;; *) [ -n "$primary_if" ] && label="$label, via $primary_if" ;; esac
    for s in $ns; do add_server primary "$s" "$label" "$primary_vpn"; done
    continue
  fi
  # First resolver bound to a non-tunnel interface = the physical network's DNS.
  if [ "$phys_done" -eq 0 ] && [ -n "$ifn" ]; then
    case "$ifn" in
      utun*) ;;
      *)
        phys_done=1
        for s in $ns; do add_server physical "$s" "physical interface $ifn"; done
        ;;
    esac
  fi
done < <(nt_resolvers)
for s in 1.1.1.1 8.8.8.8 9.9.9.9; do add_server public "$s" "public"; done

have_v6=0
nt_has_global_ipv6 && have_v6=1

j=0
while IFS='|' read -r group server label via_vpn; do
  j=$((j + 1))
  # IPv6 servers need IPv6 connectivity, except link-local ones (fe80::...%if).
  case "$server" in
    fe80:*|FE80:*) ;;
    *:*)
      if [ "$have_v6" -eq 0 ]; then
        echo "SKIP 0" > "$NT_TMP/d.$j"
        continue
      fi
      ;;
  esac
  (
    out=$(dig "@$server" +tries=1 +time=2 google.com 2>&1)
    st=$(printf '%s\n' "$out" | sed -n 's/.*status: \([A-Z]*\).*/\1/p' | head -n 1)
    ms=$(printf '%s\n' "$out" | sed -n 's/.*Query time: \([0-9]*\) msec.*/\1/p' | head -n 1)
    if [ "${st:-}" = NOERROR ]; then
      echo "OK ${ms:-0}" > "$NT_TMP/d.$j"
    elif [ -n "${st:-}" ]; then
      echo "ERR $st" > "$NT_TMP/d.$j"
    else
      echo "TIMEOUT 0" > "$NT_TMP/d.$j"
    fi
  ) &
done < "$servers_file"
wait

primary_bad=""; any_ok=0; any_tried=0; j=0
while IFS='|' read -r group server label via_vpn; do
  j=$((j + 1))
  read -r res val < "$NT_TMP/d.$j"
  case "$res" in
    SKIP)
      nt_skip "$server ($label): IPv6 server but this Mac has no global IPv6 address"
      nt_rec DNS_SERVER "$group|$server|SKIP|0|$via_vpn|$label"
      continue ;;
    OK)
      any_tried=$((any_tried + 1)); any_ok=$((any_ok + 1))
      if [ "$val" -gt 500 ]; then
        nt_warn "$server ($label): ${val} ms (slow)"
      else
        nt_pass "$server ($label): ${val} ms"
      fi ;;
    ERR)
      any_tried=$((any_tried + 1)); any_ok=$((any_ok + 1))
      nt_warn "$server ($label): answered with error status $val" ;;
    *)
      any_tried=$((any_tried + 1))
      nt_warn "$server ($label): TIMEOUT"
      [ "$group" = primary ] && primary_bad="$primary_bad $server"
      ;;
  esac
  nt_rec DNS_SERVER "$group|$server|$res|$val|$via_vpn|$label"
done < "$servers_file"

nt_rec DNS_PRIMARY_TIMEOUTS "${primary_bad# }"
if [ -n "$primary_bad" ]; then
  nt_hint "These servers belong to your primary resolver and did not answer:${primary_bad}. The system asks them first, so each lookup waits for the timeout before trying another server."
fi
if [ "$any_tried" -gt 0 ] && [ "$any_ok" -eq 0 ]; then
  nt_fail "No DNS server answered: names cannot be resolved at all"
  nt_hint "Either the whole connection is down (see checks 01, 03, 04) or something blocks DNS."
fi

nt_end
