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
    v4_known=1
    [ $((v4_ok * 2)) -lt "$v4_tot" ] && v4_fail=1
  fi
  if [ -n "$v6_tot" ] && [ "$v6_tot" -gt 0 ]; then
    v6_known=1
    [ $((v6_ok * 2)) -ge "$v6_tot" ] && v6_works=1
  fi

  # Public IPv4 DNS servers (the ones every Mac can reach regardless of its own
  # DNS settings). If most of them time out, IPv4 itself is degraded.
  local pub_tried pub_timeouts pub_names pub_degraded=0
  pub_tried=$(nt_get_all DNS_SERVER | awk -F'|' '$1 == "public" && $3 != "SKIP" { n++ } END { print n + 0 }')
  pub_timeouts=$(nt_get_all DNS_SERVER | awk -F'|' '$1 == "public" && $3 == "TIMEOUT" { n++ } END { print n + 0 }')
  pub_names=$(nt_get_all DNS_SERVER | awk -F'|' '$1 == "public" && $3 == "TIMEOUT" { printf "%s%s", (n++ ? ", " : ""), $2 }')
  [ "$pub_tried" -gt 0 ] && [ $((pub_timeouts * 2)) -gt "$pub_tried" ] && pub_degraded=1

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
        "Restart the router/ONT. If it persists, report to the ISP that $hop_txt, IPv6 works."
    elif [ "$v6_known" -eq 1 ]; then
      say isp_down "Both IPv4 and IPv6 fail past your router: the ISP connection is down." \
        "Restart the router/ONT. If it persists, report to the ISP that $hop_txt and IPv6 fails too."
    elif [ "$v6_global" = 0 ]; then
      say ipv4_outage_no_ipv6 "IPv4 fails past your router and this network has no IPv6 to compare." \
        "Most likely an ISP outage. Restart the router/ONT; if it persists, report that $hop_txt."
    else
      say ipv4_outage_unverified "IPv4 fails past your router." \
        "Run the full nettest (without arguments) to compare with IPv6. Restart the router/ONT; if it persists, report that $hop_txt."
    fi
  fi

  # 3. DNS. Blame the primary (VPN) resolver only when the public resolvers answer.
  # If the public IPv4 resolvers time out too, the evidence points at the IPv4 path.
  local any_dns_ok=0
  if nt_get_all DNS_SERVER | grep -q '|OK|'; then any_dns_ok=1; fi
  if [ "$pub_degraded" -eq 1 ] && [ "$local_down" -eq 0 ]; then
    local lines=()
    lines+=("Several public IPv4 DNS servers time out as well, so the problem is the IPv4 path, not one DNS server.")
    if [ -n "$prim_bad" ]; then
      if [ "$via_vpn" = 1 ]; then
        lines+=("The VPN resolver failure ($prim_bad, reached through $prim_if) is most likely a consequence, not its own fault.")
      else
        lines+=("The primary resolver failure ($prim_bad) is most likely a consequence, not its own fault.")
      fi
    fi
    lines+=("Re-run, or use nettest watch to see how often it happens. If it persists, report IPv4 instability to the ISP.")
    say ipv4_degraded "Degraded IPv4 path: public DNS servers timed out ($pub_names)." "${lines[@]}"
  elif { [ "$sys_status" = WARN ] || [ "$sys_status" = FAIL ]; } && [ "$any_dns_ok" -eq 1 ]; then
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

  # 4. ICMP loss with working HTTPS.
  if [ "$local_down" -eq 0 ] && [ "$v4_known" -eq 1 ] && [ "$v4_fail" -eq 0 ] \
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
      say no_problem "No problem detected right now." \
        "Checks run: ${ran:-none}. If the problem comes and goes, run nettest watch during an incident."
    fi
  fi
}
