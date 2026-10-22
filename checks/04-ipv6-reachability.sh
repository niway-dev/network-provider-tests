#!/usr/bin/env bash
# DESC: IPv6 ICMP ping and HTTPS over IPv6 (skipped if there is no global IPv6 address)
set -u
# shellcheck source=../lib/common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/common.sh"

nt_begin ipv6-reachability "04 IPv6 reachability: ping6 and HTTPS"

if ! nt_need ifconfig curl; then nt_end; fi

if ! nt_has_global_ipv6; then
  nt_info "This Mac has no global IPv6 address, so IPv6 is not in use on this network"
  nt_skip "IPv6 ping and HTTPS probes"
  nt_rec V6_GLOBAL 0
  nt_end
fi
nt_rec V6_GLOBAL 1
nt_info "Global IPv6 address present"

ping_targets="2001:4860:4860::8888 2606:4700:4700::1111"
https_targets="https://www.google.com https://www.cloudflare.com"

i=0
if nt_have ping6; then
  for t in $ping_targets; do
    i=$((i + 1))
    (
      nt_ping "$t" 5 v6
      echo "$PING_SENT $PING_RECV $PING_LOSS ${PING_AVG:--}" > "$NT_TMP/p6.$i"
    ) &
  done
fi
i=0
for u in $https_targets; do
  i=$((i + 1))
  nt_http_probe 6 "$u" 6 > "$NT_TMP/h6.$i" &
done
wait

icmp_ok=0; icmp_total=0; i=0
if nt_have ping6; then
  for t in $ping_targets; do
    i=$((i + 1))
    read -r _ recv loss avg < "$NT_TMP/p6.$i"
    icmp_total=$((icmp_total + 1))
    if [ "$recv" -eq 0 ]; then
      nt_warn "ping6 $t: no replies (100% loss)"
    elif nt_gt "$loss" 0; then
      icmp_ok=$((icmp_ok + 1))
      nt_warn "ping6 $t: ${loss}% loss, avg ${avg} ms"
    else
      icmp_ok=$((icmp_ok + 1))
      nt_pass "ping6 $t: 0% loss, avg ${avg} ms"
    fi
  done
else
  nt_skip "ping6 not available"
fi

https_ok=0; https_total=0; i=0
for u in $https_targets; do
  i=$((i + 1))
  read -r status a rest < "$NT_TMP/h6.$i"
  https_total=$((https_total + 1))
  if [ "$status" = OK ]; then
    https_ok=$((https_ok + 1))
    nt_pass "HTTPS over IPv6 $u: HTTP $a in ${rest}s"
  else
    nt_warn "HTTPS over IPv6 $u: failed ($rest)"
  fi
done

nt_rec V6_ICMP_OK "$icmp_ok"
nt_rec V6_ICMP_TOTAL "$icmp_total"
nt_rec V6_HTTPS_OK "$https_ok"
nt_rec V6_HTTPS_TOTAL "$https_total"

if [ $((https_ok * 2)) -lt "$https_total" ]; then
  nt_fail "Most IPv6 HTTPS probes failed ($https_ok of $https_total worked) although this Mac has an IPv6 address"
  nt_hint "IPv6 traffic is not getting out. If IPv4 works, sites that prefer IPv6 may stall before falling back to IPv4."
fi

nt_end
