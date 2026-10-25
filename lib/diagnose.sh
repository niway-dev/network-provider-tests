#!/usr/bin/env bash
# shellcheck shell=bash disable=SC2034
# Diagnosis rules for network-provider-tests. Source this file after lib/common.sh.
#
# `diagnose` reads the KEY=value results recorded by the checks (file $NT_RESULTS),
# prints the findings, and records each one as a DIAG=<code>|<summary> line so the
# run record can store it. The codes are a stable machine contract, documented in
# docs/run-record.md: never rename one.
#
# The tests in tests/test-diagnose.sh feed `diagnose` synthetic result sets.

NT_DIAG_FOUND=0

# say <code> <summary> [advice line...]
say() {
  local code="$1"
  NT_DIAG_FOUND=$((NT_DIAG_FOUND + 1))
  nt_rec DIAG "$code|$2"
  shift
  printf '  %s*%s %s%s%s\n' "$C_BOLD" "$C_RESET" "$C_BOLD" "$1" "$C_RESET"
  shift
  local l
  for l in "$@"; do printf '      %s\n' "$l"; done
}

num() { local v="${1:-}"; case "$v" in ''|*[!0-9]*) echo "" ;; *) echo "$v" ;; esac; }

# secs1 <ms>: milliseconds as seconds with one decimal ("7.1"), integer math only.
secs1() { local t=$(( ($1 + 50) / 100 )); printf '%d.%d' $((t / 10)) $((t % 10)); }

# host_of <url>: the host part of an https URL.
host_of() { local u="${1#*://}"; printf '%s' "${u%%/*}"; }

# diagnose [checks-ran]: print the diagnosis for the results in $NT_RESULTS.
diagnose() {
  local ran="${1:-}"
  local gw_ok v4_ok v4_tot v6_global v6_ok v6_tot icmp_ok icmp_tot
  local reached last_hop last_ip sys_status prim_bad via_vpn prim_if
  gw_ok=$(nt_get GW_OK)
  v4_ok=$(num "$(nt_get V4_HTTPS_OK)");  v4_tot=$(num "$(nt_get V4_HTTPS_TOTAL)")
  v6_global=$(nt_get V6_GLOBAL)
  v6_ok=$(num "$(nt_get V6_HTTPS_OK)");  v6_tot=$(num "$(nt_get V6_HTTPS_TOTAL)")
  icmp_ok=$(num "$(nt_get V4_ICMP_OK)"); icmp_tot=$(num "$(nt_get V4_ICMP_TOTAL)")
  reached=$(nt_get TRACE_REACHED); last_hop=$(nt_get TRACE_LAST_HOP); last_ip=$(nt_get TRACE_LAST_IP)
  sys_status=$(nt_get DNS_SYS_STATUS); prim_bad=$(nt_get DNS_PRIMARY_TIMEOUTS); prim_bad="${prim_bad// /, }"
  via_vpn=$(nt_get DNS_PRIMARY_VIA_VPN); prim_if=$(nt_get DNS_PRIMARY_IF)

  local v4_known=0 v4_fail=0 v6_known=0 v6_works=0 local_down=0
  if [ -n "$v4_tot" ] && [ "$v4_tot" -gt 0 ]; then
    v4_known=1; v4_ok="${v4_ok:-0}"
    [ $((v4_ok * 2)) -lt "$v4_tot" ] && v4_fail=1
  fi
  if [ -n "$v6_tot" ] && [ "$v6_tot" -gt 0 ]; then
    v6_known=1; v6_ok="${v6_ok:-0}"
    [ $((v6_ok * 2)) -ge "$v6_tot" ] && v6_works=1
  fi

  # Public IPv4 DNS servers (the ones every Mac can reach regardless of its own
  # DNS settings). If most of them time out, IPv4 itself is degraded.
  local pub_tried pub_timeouts pub_names pub_degraded=0
  pub_tried=$(nt_get_all DNS_SERVER | awk -F'|' '$1 == "public" && $3 != "SKIP" { n++ } END { print n + 0 }')
  pub_timeouts=$(nt_get_all DNS_SERVER | awk -F'|' '$1 == "public" && $3 == "TIMEOUT" { n++ } END { print n + 0 }')
  pub_names=$(nt_get_all DNS_SERVER | awk -F'|' '$1 == "public" && $3 == "TIMEOUT" { printf "%s%s", (n++ ? ", " : ""), $2 }')
  [ "$pub_tried" -gt 0 ] && [ $((pub_timeouts * 2)) -gt "$pub_tried" ] && pub_degraded=1

  # IPv6 fully OK (every probe answered): lets us say the line itself is fine.
  local v6_full=0
  [ "$v6_known" -eq 1 ] && [ "$v6_ok" -eq "$v6_tot" ] && v6_full=1

  # IPv4 evidence short of an outage. Order: HTTPS probes, check 07, public DNS.
  local v4_partial=0 ev="" ev_n=0
  if [ "$v4_known" -eq 1 ] && [ "$v4_fail" -eq 0 ] && [ "$v4_ok" -lt "$v4_tot" ]; then
    v4_partial=1
    local nfail=$((v4_tot - v4_ok)) ntimed word=failed
    ntimed=$(nt_get_all V4_HTTPS | awk -F'|' '$2 == "0" && index($5, "timed out") { n++ } END { print n + 0 }')
    [ "$ntimed" -eq "$nfail" ] && word="timed out"
    ev="$nfail of $v4_tot IPv4 HTTPS probes $word"; ev_n=$((ev_n + 1))
  fi
  # Check 07 uses the default stack, so a failed probe means the IPv4 fallback
  # failed too, and a slow connect on an IPv4 probe is an IPv4 symptom. A probe
  # that failed only to resolve the name says nothing about IPv4.
  local c07_n=0 c07_ev="" c07_url c07_rc c07_err c07_fam c07_conn c07_rest
  while IFS='|' read -r c07_url c07_rc c07_err; do
    [ -n "$c07_url" ] || continue
    case "$c07_err" in *[Rr]esolve*) continue ;; esac
    c07_n=$((c07_n + 1)); c07_ev="$c07_ev${c07_ev:+, }$(host_of "$c07_url") did not connect"
  done < <(nt_get_all HTTP_FAIL)
  while IFS='|' read -r c07_url c07_fam c07_rest; do
    [ "$c07_fam" = ipv4 ] || continue
    c07_conn=$(num "$(printf '%s' "$c07_rest" | cut -d'|' -f2)")
    if [ -n "$c07_conn" ] && [ "$c07_conn" -gt 3000 ]; then
      c07_n=$((c07_n + 1)); c07_ev="$c07_ev${c07_ev:+, }$(host_of "$c07_url") connect took $(secs1 "$c07_conn") s"
    fi
  done < <(nt_get_all HTTP_TIMING)
  if [ "$c07_n" -gt 0 ]; then
    ev="$ev${ev:+; }$c07_ev (check 07)"; ev_n=$((ev_n + 1))
  fi
  if [ "$pub_degraded" -eq 1 ]; then
    ev="$ev${ev:+; }public DNS servers timed out ($pub_names)"; ev_n=$((ev_n + 1))
  fi

  # A failing primary/VPN resolver next to a bad IPv4 path is a consequence of it.
  local cons_line=""
  if [ -n "$prim_bad" ]; then
    if [ "$via_vpn" = 1 ]; then
      cons_line="The VPN resolver failure ($prim_bad, reached through $prim_if) is most likely a consequence, not its own fault."
    else
      cons_line="The primary resolver failure ($prim_bad) is most likely a consequence, not its own fault."
    fi
  fi
  # The IPv4 outage findings carry it themselves unless the degraded finding does.
  local out_cons=""
  [ "$pub_degraded" -eq 0 ] && out_cons="$cons_line"

  # 1. Gateway.
  if [ "$gw_ok" = 0 ]; then
    if [ "$v4_known" -eq 1 ] && [ "$v4_fail" -eq 0 ]; then
      say gateway_icmp_ignored "The router does not answer ping, but internet traffic works." \
        "Harmless: many routers ignore ICMP. Nothing to fix."
    else
      local_down=1
      say local_network_down "Local network problem: this Mac cannot reach its own router." \
        "The ISP is probably not the cause. Restart the router, check the cable or Wi-Fi signal," \
        "and make sure this Mac is connected to the right network."
    fi
  fi

  # 2. IPv4 / IPv6 / ISP.
  if [ "$local_down" -eq 0 ] && [ "$v4_fail" -eq 1 ]; then
    local hop_txt="IPv4 stops somewhere past your router"
    if [ "$reached" = 0 ] && [ -n "$last_hop" ] && [ "$last_hop" != 0 ]; then
      hop_txt="IPv4 stops after hop $last_hop ($last_ip)"
    fi
    if [ "$v6_known" -eq 1 ] && [ "$v6_works" -eq 1 ]; then
      say ipv4_outage "ISP IPv4 outage (often their CGNAT): sites with IPv6 load, IPv4-only sites don't." \
        "Restart the router/ONT. If it persists, report to the ISP that $hop_txt, IPv6 works." \
        ${out_cons:+"$out_cons"}
    elif [ "$v6_known" -eq 1 ]; then
      say isp_down "Both IPv4 and IPv6 fail past your router: the ISP connection is down." \
        "Restart the router/ONT. If it persists, report to the ISP that $hop_txt and IPv6 fails too." \
        ${out_cons:+"$out_cons"}
    elif [ "$v6_global" = 0 ]; then
      say ipv4_outage_no_ipv6 "IPv4 fails past your router and this network has no IPv6 to compare." \
        "Most likely an ISP outage. Restart the router/ONT; if it persists, report that $hop_txt." \
        ${out_cons:+"$out_cons"}
    else
      say ipv4_outage_unverified "IPv4 fails past your router." \
        "Run the full nettest (without arguments) to compare with IPv6. Restart the router/ONT; if it persists, report that $hop_txt." \
        ${out_cons:+"$out_cons"}
    fi
  fi

  # 3. IPv4 degraded: partial HTTPS failures, a failed or slow IPv4 connect in
  # check 07, or most public IPv4 resolvers timing out. An outage (step 2) already
  # covers the first two, so only the DNS evidence adds a finding next to it.
  # The primary (VPN) resolver is blamed only when IPv4 itself looks fine.
  local degraded=0
  if [ "$local_down" -eq 0 ]; then
    if [ "$pub_degraded" -eq 1 ]; then degraded=1
    elif [ "$v4_fail" -eq 0 ] && { [ "$v4_partial" -eq 1 ] || [ "$c07_n" -gt 0 ]; }; then degraded=1
    fi
  fi
  local any_dns_ok=0
  if nt_get_all DNS_SERVER | grep -q '|OK|'; then any_dns_ok=1; fi
  if [ "$degraded" -eq 1 ]; then
    local lines=()
    if [ "$pub_degraded" -eq 1 ]; then
      lines+=("Several public IPv4 DNS servers time out as well, so the problem is the IPv4 path, not one DNS server.")
    else
      lines+=("Some IPv4 connections fail or are slow, so the IPv4 path is unreliable.")
    fi
    [ -n "$cons_line" ] && lines+=("$cons_line")
    if [ "$v6_full" -eq 1 ]; then
      lines+=("IPv6 works while IPv4 fails: the line is fine; the IPv4 path (often the ISP's CGNAT) or something on this machine that handles only IPv4 traffic (VPN/security agent) is at fault. Compare with another device on the same network to tell which.")
    fi
    lines+=("Re-run, or use nettest watch to see how often it happens. If it persists, report IPv4 instability to the ISP.")
    say ipv4_degraded "Degraded IPv4 path: $ev." "${lines[@]}"
  elif [ "$v4_fail" -eq 0 ] && { [ "$sys_status" = WARN ] || [ "$sys_status" = FAIL ]; } && [ "$any_dns_ok" -eq 1 ]; then
    if [ -n "$prim_bad" ]; then
      if [ "$via_vpn" = 1 ]; then
        say primary_dns_timeout_vpn "The primary DNS resolver is timing out: $prim_bad (a VPN resolver, reached through $prim_if)." \
          "Every lookup waits for that timeout before the OS falls back to another server, so sites feel slow." \
          "Disconnect the VPN or ask its administrator to fix its upstream DNS."
      else
        say primary_dns_timeout "The primary DNS resolver is timing out: $prim_bad." \
          "Every lookup waits for that timeout before the OS falls back. Other servers answer quickly;" \
          "remove the dead server or switch DNS in Network settings (router address or a public resolver)."
      fi
    else
      say system_dns_slow "System DNS is slow although the configured servers answered the direct tests." \
        "It may be intermittent. Re-run, and look at the 06-dns output to see which lookups were slow."
    fi
  fi

  # 4. ICMP loss with fully working IPv4 HTTPS and no IPv4 trouble in check 07.
  if [ "$local_down" -eq 0 ] && [ "$v4_known" -eq 1 ] && [ "$v4_ok" -eq "$v4_tot" ] && [ "$c07_n" -eq 0 ] \
    && [ -n "$icmp_tot" ] && [ "$icmp_ok" -lt "$icmp_tot" ]; then
    say icmp_deprioritized "Some pings to public servers got no reply, but HTTPS works." \
      "Harmless: the ISP deprioritizes or drops ping. This is not an outage."
  fi

  # 5. Nothing matched.
  if [ "$NT_DIAG_FOUND" -eq 0 ]; then
    if grep -Eq '^STATUS_.*=(FAIL|WARN)$' "$NT_RESULTS"; then
      say unmatched_warnings "No known pattern matched, but some checks reported WARN or FAIL." \
        "Read the WARN/FAIL lines above; the hint under each one explains what it means."
    else
      say no_problem "Connection working normally: every check passed." \
        "Checks run: ${ran:-none}. If the problem comes and goes, run nettest watch during an incident."
    fi
  fi
}
