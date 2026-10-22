#!/usr/bin/env bash
# DESC: VPN tunnels (utun) and whether a VPN is acting as the primary DNS resolver
set -u
# shellcheck source=../lib/common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/common.sh"

nt_begin vpn-detection "02 VPN detection: tunnels and DNS routing"

if ! nt_need ifconfig scutil route; then nt_end; fi

# --- tunnel interfaces that carry an IPv4 address ----------------------------
tunnels=""
for ifc in $(ifconfig -l); do
  case "$ifc" in
    utun*)
      addr=$(ifconfig "$ifc" 2>/dev/null | awk '/^[[:space:]]+inet / { print $2; exit }')
      if [ -n "$addr" ]; then
        nt_info "Tunnel $ifc has IPv4 address $addr"
        tunnels="$tunnels $ifc"
      fi
      ;;
  esac
done
tunnels="${tunnels# }"
nt_rec VPN_TUNNELS "$tunnels"
if [ -z "$tunnels" ]; then
  nt_pass "No utun interface with an IPv4 address (no VPN-style tunnel is active)"
fi

# --- configured resolvers (from scutil --dns) --------------------------------
nt_info "Resolvers with a domain or search scope (from scutil --dns):"
shown=0
while IFS='|' read -r section idx dom ns _; do
  [ "$section" = main ] || continue
  [ -n "$dom" ] || continue
  first="${ns%% *}"
  rif=$(nt_route_if "$first")
  nt_dim "resolver #$idx  domain $dom -> $ns  (first server routed via ${rif:-unknown})"
  shown=$((shown + 1))
done < <(nt_resolvers)
[ "$shown" -eq 0 ] && nt_dim "none (a single global resolver is configured)"

# --- who is resolver #1? ------------------------------------------------------
primary_ns=""
primary_if=""
while IFS='|' read -r section idx dom ns _; do
  [ "$section" = main ] || continue
  primary_ns="${ns%% *}"
  primary_if=$(nt_route_if "$primary_ns")
  nt_rec DNS_PRIMARY_RESOLVER "$idx"
  nt_rec DNS_PRIMARY_SERVERS "$ns"
  break
done < <(nt_resolvers)

if [ -z "$primary_ns" ]; then
  nt_skip "Could not read a primary resolver from scutil --dns"
  nt_rec DNS_PRIMARY_VIA_VPN ""
  nt_end
fi

nt_rec DNS_PRIMARY_NS "$primary_ns"
nt_rec DNS_PRIMARY_IF "$primary_if"
case "$primary_if" in
  utun*)
    nt_rec DNS_PRIMARY_VIA_VPN 1
    nt_info "Primary resolver #1 starts with $primary_ns, which is reached through VPN tunnel $primary_if, not the physical network"
    nt_hint "A VPN is acting as your primary DNS; if its DNS stops answering, every lookup waits for a timeout before falling back."
    ;;
  *)
    nt_rec DNS_PRIMARY_VIA_VPN 0
    nt_pass "Primary resolver #1 starts with $primary_ns, reached through ${primary_if:-unknown} (physical network)"
    ;;
esac

nt_end
