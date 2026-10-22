# 2026-10-22: intermittent IPv4 failures on the home connection

Status: ongoing at the time of writing. Written from measurements taken by hand
and with `nettest` on one machine. Times are local (UTC-5). "Morning" means
before 10:00; the exact time was not recorded, which is itself a lesson (see
"What to measure next time").

The tool did not yet record runs when this started, so the only evidence is
what was copied from the terminal. From version 0.2.0 each run is stored in
`runs.jsonl` ([run record](../run-record.md)).

## Timeline

| Time | What was measured | Result |
| --- | --- | --- |
| Morning, first check | Gateway ping | 0.67 ms average, 0% loss |
| | Ping 1.1.1.1 and 8.8.8.8 | 100% loss on both |
| | System DNS lookups (google, github, anthropic.com) | About 5 s each; one cached name took 6 ms |
| | DNS servers | The primary resolver (a work VPN acting as primary DNS) timed out; the ISP's own resolver answered in 2 ms |
| | Traceroute to 1.1.1.1 | 192.168.1.1, 10.161.218.1, 10.150.156.249, 10.95.156.50, then no replies |
| | HTTPS over IPv4 | google, cloudflare (1.1.1.1) and fastly timed out (5 to 8 s); one Microsoft endpoint answered in 2.1 s |
| | HTTPS over IPv6 to google | HTTP 200 in 0.59 s |
| | ping6 to Google DNS | 0% loss, 107 ms |
| After the first router restart | HTTPS over IPv4 | google 0.27 s, github 0.85 s, cloudflare 0.72 s |
| | System DNS, VPN DNS | 0.02 s and 16 ms |
| | Ping 1.1.1.1 | Still 100% loss, yet traceroute reached 1.1.1.1 at hop 9 (hops 5 to 8 silent) |
| | Ping 8.8.8.8 | 40% loss |
| | Gateway ping | 6 to 10 ms (the router had just rebooted) |
| About 5 minutes later | GitHub API over IPv4 | Timed out again |
| | Router | Restarted a second time |
| 10:58 | HTTPS over IPv4 to google, github, cloudflare, 1.1.1.1 | All timed out at 8 s |
| | HTTPS over IPv6 to google | Also timed out |
| | Ping 8.8.8.8 | 40% loss |
| | Traceroute to 8.8.8.8 | ... 10.95.156.50, hops 5 and 6 silent, 10.95.156.18, then Google's network (72.14.217.132, 216.239.54.167, 142.251.78.33) at about 43 ms |
| About 11:00 to 11:15, full `nettest` run | HTTPS over IPv4 and IPv6 | All passing |
| | DNS | VPN resolvers 2 and 3 timed out (resolver 1 answered in 8 ms); public 1.1.1.1 and 8.8.8.8 timed out; 9.9.9.9 answered in 5 ms |
| | System lookups | 1 to 2 s |
| About 11:20 | GitHub API over IPv4 | Timed out again; `git push` succeeded on retry |
| 11:20, `nettest` 0.2.0 run | HTTPS over IPv4 and IPv6 | Both failed (0/4, 0/2), diagnosis `isp_down`; ping6 still answered and traceroute reached 1.1.1.1 at hop 13 |
| | System lookups | 1.2 s worst |
| By 11:25, `nettest watch 1` | Full suite | Healthy again (`PASS`) |

## Interpretation

### Confirmed by measurements

- **The home network was healthy throughout.** The gateway answered in under a
  millisecond with 0% loss before the restarts. The 6 to 10 ms after the first
  restart is explained by the router having just rebooted.
- **The failure is inside the ISP network, after hop 10.95.156.50.** Traceroutes
  to two different targets lost their replies there, in the private address
  range that sits right behind the home router.
- **IPv4 was affected more than IPv6.** In the morning IPv6 HTTPS worked while
  IPv4 HTTPS timed out. At 10:58 both failed, so IPv6 was not immune.
- **Ping loss alone is not conclusive.** Ping to 1.1.1.1 showed 100% loss while
  traceroute and HTTPS reached the same host. ISPs deprioritize ICMP, so a
  failing ping with a working page load is not an outage.
- **The problem is intermittent.** After each restart things worked for minutes,
  then failed again; at 11:20 a request timed out and the retry worked.

### Hypotheses (not confirmed)

- **Carrier-grade NAT (CGNAT) trouble.** CGNAT means many customers share public
  IPv4 addresses behind the ISP's equipment. The pattern (IPv4 hit harder than
  IPv6, a silent hop inside the ISP's private range) is consistent with it, but
  nothing measured here proves it.
- **A router restart gets a new IPv4 session, which helps for a few minutes.**
  If true, the fault is upstream of the router, not in it. The two restarts
  both helped only briefly, which fits, but two samples are thin.
- **The work VPN's DNS timeouts are a consequence of the IPv4 path.** The VPN
  resolvers and the public 1.1.1.1 and 8.8.8.8 resolvers timed out in the same
  run while 9.9.9.9 answered. If the VPN were the cause, the public resolvers
  would have been fine. This is why `nettest` 0.2.0 reports `ipv4_degraded`
  instead of blaming the VPN in that situation.

## What to measure next time

1. **Run `nettest watch 5` as soon as the problem appears and leave it in a
   terminal tab.** It records a full run every 5 minutes to `runs.jsonl`, with
   timestamps, so an intermittent outage becomes a list with start and end times
   instead of a memory.
2. Run `nettest history --since 2h` afterwards and note which runs failed and
   which diagnosis codes (`ipv4_outage`, `ipv4_degraded`, `isp_down`) they carry.
3. Do the first run **before** restarting the router. A restart erases the
   evidence and the "fixed" state lasts only minutes.
4. Compare IPv4 and IPv6 in the same run: the full `nettest`, not a single check.
5. Keep the traceroute (check 05): the last responding hop and where replies
   stop are what the ISP's engineers need.
6. Write down the time the router was restarted next to the run times.

## Reporting to the ISP

Use [`templates/isp-complaint.es.md`](../../templates/isp-complaint.es.md)
(Spanish) and fill it from the failing runs. The fields in
[`docs/run-record.md`](../run-record.md) show where each number comes from.
