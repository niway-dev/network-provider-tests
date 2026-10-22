#!/usr/bin/env bash
# DESC: Default gateway reachability and latency (is the problem inside your home?)
set -u
# shellcheck source=../lib/common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/common.sh"

nt_begin local-network "01 Local network: router / gateway"

route_out=$(route -n get default 2>/dev/null)
gw=$(printf '%s\n' "$route_out" | awk '/gateway:/ { print $2; exit }')
ifc=$(printf '%s\n' "$route_out" | awk '/interface:/ { print $2; exit }')

if [ -z "$ifc" ]; then
  nt_fail "No default route: this Mac is not connected to any network"
  nt_hint "Check that Wi-Fi is on or the cable is plugged in, then re-run."
  nt_rec GW_OK 0
  nt_end
fi

# Is the interface Wi-Fi or wired? Wired links should have lower latency.
port=$(networksetup -listallhardwareports 2>/dev/null |
  awk -v dev="$ifc" '/^Hardware Port:/ { p = substr($0, 16) } /^Device:/ && $2 == dev { print p; exit }')
kind=wired
case "$port" in *[Ww]i-[Ff]i* | *AirPort*) kind=wifi ;; esac
threshold=5
[ "$kind" = wifi ] && threshold=20

nt_info "Default route: interface $ifc${port:+ ($port)}, gateway ${gw:-none}"
nt_rec GW_IF "$ifc"
nt_rec GW_IP "${gw:-}"
case "$ifc" in
  utun*) nt_info "The default route goes through a tunnel interface ($ifc): all traffic uses a VPN" ;;
esac

# A point-to-point or link-local default route has no IPv4 gateway to ping.
case "$gw" in
  [0-9]*.[0-9]*.[0-9]*.[0-9]*) ;;
  *)
    nt_skip "No IPv4 gateway address to ping (got '${gw:-none}')"
    nt_rec GW_OK ""
    nt_end
    ;;
esac

nt_ping "$gw" 5
nt_rec GW_LOSS "$PING_LOSS"
nt_rec GW_AVG "${PING_AVG:-}"

if [ "$PING_RECV" -eq 0 ]; then
  nt_fail "Gateway $gw does not answer ping (100% loss)"
  nt_hint "The problem is between this Mac and your router (Wi-Fi, cable, router), not the ISP. Some routers ignore ping, so confirm with the later checks."
  nt_rec GW_OK 0
elif nt_gt "$PING_LOSS" 0; then
  nt_warn "Gateway $gw: ${PING_LOSS}% loss, avg ${PING_AVG:-?} ms"
  nt_hint "Packet loss on the first hop points to a weak Wi-Fi signal, a bad cable or an overloaded router."
  nt_rec GW_OK 1
elif [ -n "${PING_AVG:-}" ] && nt_gt "$PING_AVG" "$threshold"; then
  nt_warn "Gateway $gw: avg ${PING_AVG} ms (expected under ${threshold} ms on $kind)"
  nt_hint "A slow first hop is local: Wi-Fi interference, a busy router or a flaky cable."
  nt_rec GW_OK 1
else
  nt_pass "Gateway $gw: 0% loss, avg ${PING_AVG:-?} ms ($kind)"
  nt_rec GW_OK 1
fi

nt_end
