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

# --- Partial IPv4 failure: HTTPS is not "working" (observed twice in a real outage) ---
# The first two cases mirror lines 1 and 2 of tests/fixtures/run-bash-real.jsonl
# (documentation addresses; the same data the Rust rediagnose test reads).
T_NAME="real run 1: 2 of 4 IPv4 HTTPS time out, ping 100% lost, check 07 fails, 2 of 3 public resolvers time out"
{
  healthy nodns
  cat <<'R'
V4_ICMP_OK=0
V4_HTTPS_OK=2
V4_HTTPS=https://www.google.com|0|||timed out after 6s
V4_HTTPS=https://1.1.1.1|1|301|5055|
V4_HTTPS=https://github.com|0|||timed out after 6s
V4_HTTPS=https://www.cloudflare.com|1|200|2562|
TRACE_LAST_HOP=7
HTTP_FAIL=https://github.com|28|curl: (28) Connection timed out after 10006 milliseconds
HTTP_TIMING=https://www.google.com|ipv6|2|107|110|146|589|200
DNS_PRIMARY_TIMEOUTS=198.51.100.2 198.51.100.3 198.51.100.4
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|0|resolver #1, primary, via en8
DNS_SERVER=primary|198.51.100.3|TIMEOUT|0|0|resolver #1, primary, via en8
DNS_SERVER=primary|198.51.100.4|TIMEOUT|0|0|resolver #1, primary, via en8
DNS_SERVER=public|1.1.1.1|TIMEOUT|0|0|public
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
DNS_SERVER=public|9.9.9.9|OK|4|0|public
R
} > "$RES"; run_diag
assert_eq "$CODES" "ipv4_degraded" "no icmp_deprioritized: HTTPS did not fully work"
assert_contains "$OUT" "Degraded IPv4 path: 2 of 4 IPv4 HTTPS probes timed out; github.com did not connect (check 07); public DNS servers timed out (1.1.1.1, 8.8.8.8)." "names every piece of evidence"
assert_contains "$OUT" "The primary resolver failure (198.51.100.2, 198.51.100.3, 198.51.100.4) is most likely a consequence" "resolver failure is a consequence"
assert_contains "$OUT" "IPv6 works while IPv4 fails: the line is fine;" "compares with IPv6"

T_NAME="real run 2: HTTPS fine, two public resolvers time out, IPv6 fine"
{
  healthy nodns
  cat <<'R'
TRACE_REACHED=0
TRACE_LAST_HOP=4
TRACE_LAST_IP=198.51.100.7
DNS_PRIMARY_TIMEOUTS=198.51.100.4
HTTP_TIMING=https://github.com|ipv4|1|100|105|103|810|200
DNS_SERVER=primary|198.51.100.4|TIMEOUT|0|0|resolver #1, primary, via en8
DNS_SERVER=public|1.1.1.1|OK|4|0|public
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
DNS_SERVER=public|9.9.9.9|TIMEOUT|0|0|public
R
} > "$RES"; run_diag
assert_eq "$CODES" "ipv4_degraded"
assert_contains "$OUT" "Degraded IPv4 path: public DNS servers timed out (8.8.8.8, 9.9.9.9)." "DNS-only evidence keeps its wording"
assert_contains "$OUT" "IPv6 works while IPv4 fails:"

T_NAME="ping loss with 3 of 4 IPv4 HTTPS OK"
{ healthy; printf 'V4_ICMP_OK=0\nV4_HTTPS_OK=3\n'; } > "$RES"; run_diag
assert_eq "$CODES" "ipv4_degraded" "partial failure is degraded, not a harmless ping loss"
assert_contains "$OUT" "Degraded IPv4 path: 1 of 4 IPv4 HTTPS probes failed." "no error text recorded: says failed"

T_NAME="IPv4 HTTPS fine but check 07 IPv4 connect took 7.1 s"
{
  healthy
  cat <<'R'
V4_ICMP_OK=0
HTTP_TIMING=https://www.google.com|ipv6|2|107|110|146|589|200
HTTP_TIMING=https://github.com|ipv4|1|7149|7200|103|7500|200
R
} > "$RES"; run_diag
assert_eq "$CODES" "ipv4_degraded" "slow IPv4 connect blocks icmp_deprioritized"
assert_contains "$OUT" "Degraded IPv4 path: github.com connect took 7.1 s (check 07)." "names the slow connect"

T_NAME="IPv4 connect of exactly 3000 ms is not degraded"
{ healthy; printf 'V4_ICMP_OK=0\nHTTP_TIMING=https://github.com|ipv4|1|3000|3100|103|3300|200\n'; } > "$RES"; run_diag
assert_eq "$CODES" "icmp_deprioritized"

T_NAME="slow connect on an IPv6 probe is not an IPv4 symptom"
{ healthy; printf 'HTTP_TIMING=https://github.com|ipv6|1|7000|7100|103|7300|200\n'; } > "$RES"; run_diag
assert_eq "$CODES" "no_problem"

T_NAME="check 07 failed to resolve a name"
{ healthy; printf 'V4_ICMP_OK=0\nHTTP_FAIL=https://github.com|6|curl: (6) Could not resolve host: github.com\n'; } > "$RES"; run_diag
assert_eq "$CODES" "icmp_deprioritized" "a name that does not resolve says nothing about IPv4"

T_NAME="check 07 IPv4 probe failed, everything else fine"
{ healthy; printf 'HTTP_FAIL=https://github.com|28|curl: (28) Connection timed out after 10006 milliseconds\n'; } > "$RES"; run_diag
assert_eq "$CODES" "ipv4_degraded"
assert_contains "$OUT" "Degraded IPv4 path: github.com did not connect (check 07)."

T_NAME="no IPv6 comparison line when IPv6 is only partly OK"
{ healthy; printf 'V4_HTTPS_OK=3\nV6_HTTPS_OK=1\n'; } > "$RES"; run_diag
assert_eq "$CODES" "ipv4_degraded"
assert_not_contains "$OUT" "IPv6 works while IPv4 fails"

T_NAME="partial HTTPS failure: one public resolver and the VPN resolver time out"
{
  healthy nodns
  cat <<'R'
V4_HTTPS_OK=3
DNS_SYS_STATUS=WARN
DNS_PRIMARY_VIA_VPN=1
DNS_PRIMARY_IF=utun4
DNS_PRIMARY_TIMEOUTS=198.51.100.2
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=public|1.1.1.1|OK|9|0|public
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
DNS_SERVER=public|9.9.9.9|OK|6|0|public
R
} > "$RES"; run_diag
assert_eq "$CODES" "ipv4_degraded" "IPv4 trouble comes before VPN blame"
assert_contains "$OUT" "The VPN resolver failure (198.51.100.2, reached through utun4) is most likely a consequence"

T_NAME="IPv4 outage plus a timing-out VPN resolver"
{
  healthy nodns
  cat <<'R'
V4_HTTPS_OK=0
DNS_SYS_STATUS=WARN
DNS_PRIMARY_VIA_VPN=1
DNS_PRIMARY_IF=utun4
DNS_PRIMARY_TIMEOUTS=198.51.100.2
DNS_SERVER=primary|198.51.100.2|TIMEOUT|0|1|resolver #1, primary, via VPN tunnel utun4
DNS_SERVER=public|1.1.1.1|OK|9|0|public
DNS_SERVER=public|8.8.8.8|TIMEOUT|0|0|public
DNS_SERVER=public|9.9.9.9|OK|6|0|public
R
} > "$RES"; run_diag
assert_eq "$CODES" "ipv4_outage" "no VPN blame during an outage"
assert_contains "$OUT" "The VPN resolver failure (198.51.100.2, reached through utun4) is most likely a consequence" "mentioned inside the outage finding"

T_NAME="ping loss with HTTPS fully OK and a fast check 07"
{ healthy; printf 'V4_ICMP_OK=0\nHTTP_TIMING=https://github.com|ipv4|1|100|105|103|810|200\n'; } > "$RES"; run_diag
assert_eq "$CODES" "icmp_deprioritized" "still harmless when nothing else is wrong"

t_finish
