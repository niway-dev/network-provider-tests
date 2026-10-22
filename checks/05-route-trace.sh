#!/usr/bin/env bash
# DESC: traceroute to 1.1.1.1 (where along the path does traffic stop?)
set -u
# shellcheck source=../lib/common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/common.sh"

nt_begin route-trace "05 Route trace: where does IPv4 traffic stop?"

if ! nt_need traceroute; then nt_end; fi

TARGET=1.1.1.1
trace=$(nt_run_timeout 40 traceroute -n -w 1 -q 1 -m 15 "$TARGET" 2>&1)

# is_isp_internal <ip>: private / CGNAT ranges that sit inside an ISP network
# when they appear right after the home router.
is_isp_internal() {
  case "$1" in
    10.*|100.6[4-9].*|100.[7-9][0-9].*|100.1[01][0-9].*|100.12[0-7].*) return 0 ;;
    172.1[6-9].*|172.2[0-9].*|172.3[01].*) return 0 ;;
  esac
  return 1
}

last_hop=0; last_ip=""; reached=0; hops=0; internal=0
while read -r n ip rest; do
  case "$n" in *[!0-9]* | "") continue ;; esac
  hops=$n
  label=""
  if [ "$ip" = "*" ]; then
    line="$n  *  (no reply)"
  else
    last_hop=$n; last_ip=$ip
    [ "$ip" = "$TARGET" ] && reached=1
    if [ "$n" -gt 1 ] && is_isp_internal "$ip"; then
      label="  [private/CGNAT range: likely ISP internal network]"
      internal=$((internal + 1))
    fi
    line="$n  $ip  ${rest%% ms*} ms$label"
  fi
  nt_dim "$line"
done <<< "$trace"

nt_rec TRACE_REACHED "$reached"
nt_rec TRACE_LAST_HOP "$last_hop"
nt_rec TRACE_LAST_IP "$last_ip"
nt_rec TRACE_HOPS "$hops"

if [ "$hops" -eq 0 ]; then
  nt_warn "traceroute produced no hops: $(printf '%s' "$trace" | head -n 1)"
  nt_hint "This usually means the network is down or traceroute is blocked."
elif [ "$reached" -eq 1 ]; then
  nt_pass "Reached $TARGET at hop $last_hop"
else
  if [ "$last_hop" -eq 0 ]; then
    nt_warn "No hop answered at all (not even the router)"
  else
    nt_warn "Did not reach $TARGET; last responding hop: $last_hop ($last_ip)"
  fi
  nt_hint "Hops in 10.x or 100.64.x right after your router are the ISP's internal network. 'Dies after hop $last_hop inside the ISP' is evidence to give the ISP."
  nt_dim "Note: some routers drop traceroute probes even when traffic flows; trust this only together with the HTTPS results."
fi

nt_end
