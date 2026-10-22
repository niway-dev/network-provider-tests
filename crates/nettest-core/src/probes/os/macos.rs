//! Network introspection on macOS: `route`, `scutil`, `networksetup` and the
//! interface list (via `netdev`). These commands print structured, non-localized
//! text and need no privileges.

use std::process::Command;

use crate::probes::os::scutil::parse_scutil_dns;
use crate::probes::{DefaultRoute, ResolverEntry, Tunnel};

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Value after `key:` in the first line that starts with it (`route -n get` output).
pub fn route_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix(key)?.strip_prefix(':')?;
        let v = rest.trim();
        (!v.is_empty()).then(|| v.to_string())
    })
}

/// Hardware port name for a device from `networksetup -listallhardwareports`.
pub fn hardware_port_for(text: &str, device: &str) -> Option<String> {
    let mut port: Option<String> = None;
    for l in text.lines() {
        let l = l.trim();
        if let Some(p) = l.strip_prefix("Hardware Port:") {
            port = Some(p.trim().to_string());
        } else if let Some(d) = l.strip_prefix("Device:") {
            if d.trim() == device {
                return port;
            }
        }
    }
    None
}

pub fn default_route() -> Option<DefaultRoute> {
    let text = run("route", &["-n", "get", "default"])?;
    let interface = route_field(&text, "interface")?;
    let gateway = route_field(&text, "gateway");
    let hardware_port = run("networksetup", &["-listallhardwareports"])
        .and_then(|t| hardware_port_for(&t, &interface));
    Some(DefaultRoute {
        interface,
        gateway,
        hardware_port,
    })
}

pub fn route_interface(ip: &str) -> Option<String> {
    if let Some((_, scope)) = ip.split_once('%') {
        return Some(scope.to_string());
    }
    let text = if ip.contains(':') {
        run("route", &["-n", "get", "-inet6", ip])?
    } else {
        run("route", &["-n", "get", ip])?
    };
    route_field(&text, "interface")
}

pub fn resolvers() -> Vec<ResolverEntry> {
    run("scutil", &["--dns"])
        .map(|t| parse_scutil_dns(&t))
        .unwrap_or_default()
}

/// Sort `utun2` before `utun10`.
fn natural_key(name: &str) -> (String, u32) {
    let digits_at = name
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(name.len());
    (
        name[..digits_at].to_string(),
        name[digits_at..].parse().unwrap_or(0),
    )
}

/// `utun*` interfaces that carry an IPv4 address.
pub fn tunnels_with_ipv4() -> Vec<Tunnel> {
    let mut v: Vec<Tunnel> = netdev::get_interfaces()
        .into_iter()
        .filter(|i| i.name.starts_with("utun"))
        .filter_map(|i| {
            i.ipv4.first().map(|n| Tunnel {
                name: i.name.clone(),
                addr: n.addr().to_string(),
            })
        })
        .collect();
    v.sort_by_key(|t| natural_key(&t.name));
    v
}

/// Any interface address in 2000::/3 (the bash tool greps for a leading 2 or 3).
pub fn has_global_ipv6() -> bool {
    netdev::get_interfaces()
        .iter()
        .flat_map(|i| i.ipv6.iter())
        .any(|n| n.addr().octets()[0] & 0xe0 == 0x20)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: &str = "   route to: default
destination: default
       mask: default
    gateway: 192.0.2.1
  interface: en0
      flags: <UP,GATEWAY,DONE,STATIC,PRCLONING,GLOBAL>
";

    #[test]
    fn route_output() {
        assert_eq!(route_field(ROUTE, "gateway").as_deref(), Some("192.0.2.1"));
        assert_eq!(route_field(ROUTE, "interface").as_deref(), Some("en0"));
        assert_eq!(
            route_field(
                "route: writing to routing socket: not in table",
                "interface"
            ),
            None
        );
    }

    #[test]
    fn hardware_ports() {
        let t = "\nHardware Port: Ethernet Adapter (en4)\nDevice: en4\nEthernet Address: aa:bb\n\nHardware Port: Wi-Fi\nDevice: en0\nEthernet Address: cc:dd\n";
        assert_eq!(hardware_port_for(t, "en0").as_deref(), Some("Wi-Fi"));
        assert_eq!(
            hardware_port_for(t, "en4").as_deref(),
            Some("Ethernet Adapter (en4)")
        );
        assert_eq!(hardware_port_for(t, "en9"), None);
    }

    #[test]
    fn natural_sort() {
        let mut v = vec!["utun10", "utun2", "utun0"];
        v.sort_by_key(|n| natural_key(n));
        assert_eq!(v, ["utun0", "utun2", "utun10"]);
    }
}
