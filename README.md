# network-provider-tests

Shell scripts that tell you whether a slow or broken home internet connection is
your router, your ISP, IPv4 vs IPv6, or your DNS. They run on macOS with the
tools it already ships (`scutil`, `route`, `dig`, `curl`, `ping`, `ping6`,
`traceroute`). No dependencies, no AI assistant needed.

Each check prints `PASS` / `WARN` / `FAIL` / `INFO` / `SKIP` lines with a short
plain-English explanation of what a failure means. `nettest` then combines the
results into a diagnosis and tells you what to do.

## Install

```sh
git clone https://github.com/niway-dev/network-provider-tests.git
cd network-provider-tests
./bin/nettest
```

Optional: put the commands on your PATH.

```sh
mkdir -p ~/.local/bin
ln -s "$PWD/bin/nettest" ~/.local/bin/nettest
ln -s "$PWD/bin/nettest-log" ~/.local/bin/nettest-log
```

## Usage

```sh
nettest              # run every check, then print a diagnosis (about 30-60 s)
nettest dns          # run one check: by name ("dns"), file stem ("06-dns") or number ("06")
nettest --list       # list the checks
nettest-log          # run everything and append the output to a daily log
nettest-log -q       # same, but print nothing (handy for cron or launchd)
```

Exit status of `nettest` is `0` when no check printed `FAIL`, `1` otherwise.
Set `NO_COLOR=1` to turn colors off.

Logs go to `~/Library/Logs/network-provider-tests/YYYY-MM-DD.log` (override the
directory with `NETTEST_LOG_DIR`). Run `nettest-log` whenever the connection
misbehaves: a folder of timestamped runs is the evidence your ISP asks for.

## Checks

| Check | What it does | What it detects |
| --- | --- | --- |
| `01-local-network` | Finds the default gateway and interface, pings the gateway 5 times | Router, cable or Wi-Fi trouble (the ISP is not involved). Warns above 5 ms wired / 20 ms Wi-Fi |
| `02-vpn-detection` | Lists `utun` tunnels with an IPv4 address and the DNS resolvers from `scutil --dns` | A VPN acting as the primary DNS resolver |
| `03-ipv4-reachability` | Pings 1.1.1.1, 8.8.8.8, 9.9.9.9, then fetches four HTTPS sites with `curl -4` | IPv4 outage; ping-only loss, which is usually harmless |
| `04-ipv6-reachability` | If there is a global IPv6 address: `ping6` two resolvers and `curl -6` two sites | IPv6 outage, or IPv6 working while IPv4 is not |
| `05-route-trace` | `traceroute` to 1.1.1.1 | The last hop that answers, and whether it is inside the ISP network |
| `06-dns` | Times lookups through the system resolver, then queries every configured and public DNS server directly | A dead or slow resolver that makes every lookup wait for a timeout |
| `07-http-timing` | `curl` timing split into DNS, connect, TLS and first byte | Which phase of a page load is slow |

Notes on how to read them:

- Packet loss on `ping` with HTTPS still working is almost always the ISP
  deprioritizing ICMP, not an outage.
- In `05-route-trace`, hops in `10.x` or `100.64.x` right after your router are
  the ISP's internal network. "The trace dies after hop N inside the ISP" is
  evidence worth giving them.
- Some routers do not answer ping or traceroute even when everything works, so
  the diagnosis never relies on one of those results alone.

## Diagnosis rules

After the checks, `nettest` prints the most likely explanation:

| Evidence | Diagnosis | What to do |
| --- | --- | --- |
| Gateway does not answer and HTTPS fails | Local network problem | Restart the router, check the cable or Wi-Fi |
| Gateway does not answer but HTTPS works | The router ignores ping | Nothing, harmless |
| IPv4 HTTPS mostly fails, IPv6 works | ISP IPv4 outage (often their CGNAT) | Restart the router or ONT; if it persists, tell the ISP that IPv4 stops after hop N and IPv6 works |
| IPv4 and IPv6 both fail past the gateway | ISP down entirely | Restart the router or ONT; report to the ISP |
| IPv4 fails and there is no IPv6 to compare | Probably an ISP outage | Same as above |
| System DNS is slow, but direct queries to some servers are fast | The primary resolver is timing out; the diagnosis names the server, and says so when it is a VPN resolver | Disconnect the VPN or fix its upstream DNS; or change the DNS servers |
| Ping loss but HTTPS works | The ISP deprioritizes ping | Nothing, harmless |
| Nothing wrong | "No problem detected right now." | Run `nettest-log` during the next incident |

## Example output

Illustrative only; addresses use documentation ranges.

```text
== 06 DNS: system resolver and each server ==
  INFO  (a) Lookup time through the system's primary resolver
  FAIL  example.org: no answer (5022 ms, NONE)
  INFO  (b) Each DNS server queried directly (google.com, 2 s limit)
  PASS  198.51.100.1 (resolver #1, primary, via VPN tunnel utun4): 13 ms
  WARN  198.51.100.2 (resolver #1, primary, via VPN tunnel utun4): TIMEOUT
  PASS  2001:db8::53 (physical interface en0): 3 ms
  PASS  1.1.1.1 (public): 10 ms

== Diagnosis ==
  * The primary DNS resolver is timing out: 198.51.100.2 (a VPN resolver, reached through utun4).
      Every lookup waits for that timeout before the OS falls back to another server, so sites feel slow.
      Disconnect the VPN or ask its administrator to fix its upstream DNS.
```

## Incidents this caught

1. **ISP IPv4 outage while IPv6 kept working.** Some sites loaded and others hung.
   HTTPS over IPv4 failed, IPv6 worked, and the trace stopped inside the ISP's
   private address space. Restarting the router fixed it.
2. **A VPN's DNS resolver timing out.** Every page took about 5 seconds to start
   loading. The VPN was the primary resolver; one of its servers did not answer,
   so each lookup waited for the timeout before the OS tried another server.

## Reporting to the ISP

[`templates/isp-complaint.es.md`](templates/isp-complaint.es.md) is a Spanish
complaint template. Fill its placeholders from a failing `nettest` run (the
logs written by `nettest-log` keep the timestamps).

## Roadmap

A desktop (menu bar) app that runs these checks is planned. The scripts stay
the source of truth.

## License

MIT, see [LICENSE](LICENSE).
