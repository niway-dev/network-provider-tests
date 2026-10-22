#!/usr/bin/env bash
# DESC: IPv4 ICMP ping to public resolvers and HTTPS over IPv4 (is IPv4 working?)
set -u
# shellcheck source=../lib/common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/common.sh"

nt_begin ipv4-reachability "03 IPv4 reachability: ping and HTTPS"

if ! nt_need ping curl; then nt_end; fi

ping_targets="1.1.1.1 8.8.8.8 9.9.9.9"
https_targets="https://www.google.com https://1.1.1.1 https://github.com https://www.cloudflare.com"

# Run every probe in parallel, each writing its own result file.
i=0
for t in $ping_targets; do
  i=$((i + 1))
  (
    nt_ping "$t" 5
    echo "$PING_SENT $PING_RECV $PING_LOSS ${PING_AVG:--}" > "$NT_TMP/p4.$i"
  ) &
done
i=0
for u in $https_targets; do
  i=$((i + 1))
  nt_http_probe 4 "$u" 6 > "$NT_TMP/h4.$i" &
done
wait

# --- ICMP ---------------------------------------------------------------------
icmp_ok=0; icmp_total=0; i=0
for t in $ping_targets; do
  i=$((i + 1))
  read -r sent recv loss avg < "$NT_TMP/p4.$i"
  nt_rec V4_PING "$t|$sent|$recv|$loss|$avg"
  icmp_total=$((icmp_total + 1))
  if [ "$recv" -eq 0 ]; then
    nt_warn "ping $t: no replies (100% loss)"
  elif nt_gt "$loss" 0; then
    icmp_ok=$((icmp_ok + 1))
    nt_warn "ping $t: ${loss}% loss, avg ${avg} ms"
  else
    icmp_ok=$((icmp_ok + 1))
    nt_pass "ping $t: 0% loss, avg ${avg} ms"
  fi
done
nt_dim "Note: ICMP loss alone, with HTTPS still working, is usually the ISP deprioritizing ping, not an outage."

# --- HTTPS over IPv4 ----------------------------------------------------------
https_ok=0; https_total=0; i=0
for u in $https_targets; do
  i=$((i + 1))
  read -r status a rest < "$NT_TMP/h4.$i"
  https_total=$((https_total + 1))
  if [ "$status" = OK ]; then
    nt_rec V4_HTTPS "$u|1|$a|$(nt_secs_to_ms "$rest")|"
    https_ok=$((https_ok + 1))
    nt_pass "HTTPS over IPv4 $u: HTTP $a in ${rest}s"
  else
    nt_rec V4_HTTPS "$u|0|||$rest"
    nt_warn "HTTPS over IPv4 $u: failed ($rest)"
  fi
done

nt_rec V4_ICMP_OK "$icmp_ok"
nt_rec V4_ICMP_TOTAL "$icmp_total"
nt_rec V4_HTTPS_OK "$https_ok"
nt_rec V4_HTTPS_TOTAL "$https_total"

if [ $((https_ok * 2)) -lt "$https_total" ]; then
  nt_fail "Most IPv4 HTTPS probes failed ($https_ok of $https_total worked)"
  nt_hint "IPv4 traffic is not getting out. If IPv6 works (next check), this is typically an ISP-side IPv4 problem."
elif [ "$https_ok" -lt "$https_total" ]; then
  nt_info "Some IPv4 HTTPS probes failed ($https_ok of $https_total worked); the connection is up but unreliable"
elif [ "$icmp_ok" -lt "$icmp_total" ]; then
  nt_info "IPv4 HTTPS is fine; the missing ping replies are harmless"
fi

nt_end
