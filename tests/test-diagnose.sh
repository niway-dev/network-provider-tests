#!/usr/bin/env bash
# Feeds synthetic result sets (the KEY=value lines the checks record) to `diagnose`
# and checks the diagnosis codes and wording. No network access needed.
set -u
# shellcheck source=tests/lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

RES=$(mktemp "${TMPDIR:-/tmp}/nt-test.XXXXXX")
trap 'rm -f "$RES"' EXIT

# run_diag: diagnose the results written to $RES -> $OUT (text) and $CODES (comma-separated).
run_diag() {
  OUT=$(NT_RESULTS="$RES" NO_COLOR=1 bash -c '. "$1/lib/common.sh"; . "$1/lib/diagnose.sh"; diagnose "all"' _ "$ROOT" 2>&1)
  CODES=$(grep '^DIAG=' "$RES" | cut -d= -f2- | cut -d'|' -f1 | paste -sd, -)
}

# healthy [nodns]: a healthy result set. Pass "nodns" to leave out the DNS_SERVER
# lines so a case can define its own.
healthy() {
  healthy_core
  [ "${1:-}" = nodns ] || cat <<'R'
DNS_SERVER=primary|192.0.2.1|OK|5|0|resolver #1, primary
DNS_SERVER=public|1.1.1.1|OK|9|0|public
DNS_SERVER=public|8.8.8.8|OK|11|0|public
DNS_SERVER=public|9.9.9.9|OK|6|0|public
R
}
healthy_core() { cat <<'R'
GW_OK=1
V4_ICMP_OK=3
V4_ICMP_TOTAL=3
V4_HTTPS_OK=4
V4_HTTPS_TOTAL=4
V6_GLOBAL=1
V6_HTTPS_OK=2
V6_HTTPS_TOTAL=2
TRACE_REACHED=1
TRACE_LAST_HOP=9
TRACE_LAST_IP=1.1.1.1
DNS_SYS_STATUS=PASS
DNS_PRIMARY_VIA_VPN=0
DNS_PRIMARY_IF=en0
DNS_PRIMARY_TIMEOUTS=
R
}

T_NAME="healthy network"
healthy > "$RES"; run_diag
assert_eq "$CODES" "no_problem" "only no_problem"

# --- The bug this suite was added for ------------------------------------------
# Observed run: the VPN resolvers .2/.3 timed out AND public 1.1.1.1 / 8.8.8.8 timed
# out (9.9.9.9 answered). The old diagnosis blamed only the VPN DNS. The evidence
# points at the IPv4 path, so the VPN failure is reported as a likely consequence.
T_NAME="VPN + public resolvers time out"
{
  healthy nodns
  cat <<'R'
DNS_SYS_STATUS=WARN
DNS_PRIMARY_VIA_VPN=1
DNS_PRIMARY_IF=utun4
DNS_PRIMARY_TIMEOUTS=198.51.100.2 198.51.100.3
DNS_SERVER=primary|198.51.100.1|OK|8|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=primary|198.51.100.3|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=public|1.1.1.1|TIMEOUT|0|0|public
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
DNS_SERVER=public|9.9.9.9|OK|5|0|public
R
} > "$RES"; run_diag
assert_contains "$CODES" "ipv4_degraded" "reports a degraded IPv4 path"
assert_not_contains "$CODES" "primary_dns_timeout" "does not blame the VPN resolver"
assert_contains "$OUT" "most likely a consequence" "says the VPN failure is a consequence"
assert_contains "$OUT" "1.1.1.1, 8.8.8.8" "names the public resolvers that timed out"

T_NAME="VPN resolver times out, public resolvers fine"
{
  healthy
  cat <<'R'
DNS_SYS_STATUS=WARN
DNS_PRIMARY_VIA_VPN=1
DNS_PRIMARY_IF=utun4
DNS_PRIMARY_TIMEOUTS=198.51.100.2
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
R
} > "$RES"; run_diag
assert_eq "$CODES" "primary_dns_timeout_vpn" "still blames the VPN resolver"
assert_contains "$OUT" "a VPN resolver, reached through utun4"

T_NAME="one public resolver times out (not a majority)"
{
  healthy nodns
  cat <<'R'
DNS_SERVER=public|1.1.1.1|OK|9|0|public
DNS_SERVER=public|9.9.9.9|OK|6|0|public
DNS_SYS_STATUS=WARN
DNS_PRIMARY_VIA_VPN=1
DNS_PRIMARY_IF=utun4
DNS_PRIMARY_TIMEOUTS=198.51.100.2
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
R
} > "$RES"; run_diag
assert_eq "$CODES" "primary_dns_timeout_vpn" "a single public timeout is not enough"

T_NAME="non-VPN primary resolver times out"
{
  healthy
  cat <<'R'
DNS_SYS_STATUS=WARN
DNS_PRIMARY_TIMEOUTS=192.0.2.9
DNS_SERVER=primary|192.0.2.9|TIMEOUT|0|0|resolver #1, primary, via en0
R
} > "$RES"; run_diag
assert_eq "$CODES" "primary_dns_timeout"

T_NAME="public resolvers time out while IPv4 HTTPS also fails"
{
  healthy nodns
  cat <<'R'
V4_HTTPS_OK=0
V4_ICMP_OK=0
TRACE_REACHED=0
TRACE_LAST_HOP=4
TRACE_LAST_IP=10.95.156.50
DNS_SYS_STATUS=FAIL
DNS_PRIMARY_VIA_VPN=1
DNS_PRIMARY_IF=utun4
DNS_PRIMARY_TIMEOUTS=198.51.100.2
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=public|1.1.1.1|TIMEOUT|0|0|public
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
R
} > "$RES"; run_diag
assert_eq "$CODES" "ipv4_outage,ipv4_degraded" "both findings, no VPN blame"
assert_contains "$OUT" "hop 4 (10.95.156.50)"

T_NAME="no system DNS slowness, public resolvers time out"
{
  healthy nodns
  cat <<'R'
DNS_SERVER=primary|192.0.2.1|OK|5|0|resolver #1, primary
DNS_SERVER=public|9.9.9.9|OK|6|0|public
DNS_SERVER=public|1.1.1.1|TIMEOUT|0|0|public
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
R
} > "$RES"; run_diag
assert_contains "$CODES" "ipv4_degraded" "public timeouts alone are reported"

# --- Existing rules ---------------------------------------------------------------
T_NAME="IPv4 outage while IPv6 works"
{ healthy; printf 'V4_HTTPS_OK=1\n'; } > "$RES"; run_diag
assert_contains "$CODES" "ipv4_outage"
assert_not_contains "$CODES" "isp_down"

T_NAME="IPv4 and IPv6 both down"
{ healthy; printf 'V4_HTTPS_OK=0\nV6_HTTPS_OK=0\n'; } > "$RES"; run_diag
assert_contains "$CODES" "isp_down"

T_NAME="IPv4 down, no IPv6 on this network"
{ healthy; printf 'V4_HTTPS_OK=0\nV6_GLOBAL=0\nV6_HTTPS_OK=\nV6_HTTPS_TOTAL=\n'; } > "$RES"; run_diag
assert_contains "$CODES" "ipv4_outage_no_ipv6"

T_NAME="gateway down and HTTPS failing"
{ healthy; printf 'GW_OK=0\nV4_HTTPS_OK=0\n'; } > "$RES"; run_diag
assert_contains "$CODES" "local_network_down"
assert_not_contains "$CODES" "ipv4_outage"

T_NAME="gateway ignores ping but HTTPS works"
{ healthy; printf 'GW_OK=0\n'; } > "$RES"; run_diag
assert_eq "$CODES" "gateway_icmp_ignored"

T_NAME="ping loss with working HTTPS"
{ healthy; printf 'V4_ICMP_OK=1\n'; } > "$RES"; run_diag
assert_eq "$CODES" "icmp_deprioritized"

T_NAME="slow system DNS with healthy servers"
{ healthy; printf 'DNS_SYS_STATUS=WARN\n'; } > "$RES"; run_diag
assert_eq "$CODES" "system_dns_slow"

T_NAME="warnings with no matching pattern"
{ healthy; printf 'STATUS_http_timing=WARN\n'; } > "$RES"; run_diag
assert_eq "$CODES" "unmatched_warnings"

t_finish
