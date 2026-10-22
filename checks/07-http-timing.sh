#!/usr/bin/env bash
# DESC: curl timing breakdown (dns, connect, tls, first byte) with the default IP stack
set -u
# shellcheck source=../lib/common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/common.sh"

nt_begin http-timing "07 HTTP timing: which phase is slow?"

if ! nt_need curl; then nt_end; fi

urls="https://www.google.com https://github.com"
FMT='%{time_namelookup} %{time_connect} %{time_appconnect} %{time_starttransfer} %{time_total} %{http_code} %{remote_ip}'

i=0
for u in $urls; do
  i=$((i + 1))
  ( curl -sS -o /dev/null -m 10 -w "$FMT" "$u" > "$NT_TMP/t.$i" 2>"$NT_TMP/t.$i.err"; echo "$?" > "$NT_TMP/t.$i.rc" ) &
done
wait

failed=0; total=0; i=0
for u in $urls; do
  i=$((i + 1))
  total=$((total + 1))
  read -r rc < "$NT_TMP/t.$i.rc"
  if [ "${rc:-1}" != 0 ]; then
    failed=$((failed + 1))
    nt_warn "$u: curl failed (exit ${rc:-?}): $(head -n 1 "$NT_TMP/t.$i.err")"
    continue
  fi
  read -r dns conn tls ttfb tot code ip < "$NT_TMP/t.$i"
  # Phase durations in milliseconds (curl reports cumulative seconds).
  phases=$(awk -v d="$dns" -v c="$conn" -v t="$tls" -v f="$ttfb" -v o="$tot" 'BEGIN {
    dn = d * 1000; cn = (c - d) * 1000; tl = (t > 0 ? (t - c) * 1000 : 0)
    fb = (f - (t > 0 ? t : c)) * 1000; tt = o * 1000
    printf "%d %d %d %d %d", dn, cn, tl, fb, tt }')
  read -r p_dns p_conn p_tls p_ttfb p_total <<< "$phases"
  fam=IPv4; case "$ip" in *:*) fam=IPv6 ;; esac
  # Slowest phase.
  slow=$(awk -v a="$p_dns" -v b="$p_conn" -v c="$p_tls" -v d="$p_ttfb" 'BEGIN {
    m = a; n = "dns"
    if (b > m) { m = b; n = "connect" }
    if (c > m) { m = c; n = "tls" }
    if (d > m) { m = d; n = "ttfb" }
    print n }')
  line="$u via $fam: dns ${p_dns} ms, connect ${p_conn} ms, tls ${p_tls} ms, ttfb ${p_ttfb} ms, total ${p_total} ms (HTTP $code)"
  if [ "$p_total" -gt 3000 ] || [ "$p_dns" -gt 1000 ] || [ "$p_conn" -gt 1000 ]; then
    nt_warn "$line"
    case "$slow" in
      dns) nt_hint "Slowest phase: DNS. Name lookups are slow; see check 06 for which resolver is responsible." ;;
      connect) nt_hint "Slowest phase: TCP connect. The path to the server is slow or lossy (try check 03/05); an IPv6-then-IPv4 fallback also shows up here." ;;
      tls) nt_hint "Slowest phase: TLS handshake. Usually packet loss or a congested path." ;;
      ttfb) nt_hint "Slowest phase: waiting for the server's first byte. The server or the path beyond your ISP is slow." ;;
    esac
    nt_rec HTTP_SLOW_PHASE "$slow"
  else
    nt_pass "$line"
  fi
  nt_rec HTTP_TIMING "$u|$fam|$p_dns|$p_conn|$p_tls|$p_ttfb|$p_total"
done

nt_rec HTTP_FAILED "$failed"
nt_rec HTTP_TOTAL "$total"
if [ "$failed" -eq "$total" ]; then
  nt_fail "All HTTP timing probes failed"
  nt_hint "No website could be fetched with the default stack. See checks 03, 04 and 06 to see whether IPv4, IPv6 or DNS is at fault."
fi

nt_end
