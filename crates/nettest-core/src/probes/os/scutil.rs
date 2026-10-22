//! Parser for `scutil --dns` output (not localized).

use crate::probes::{ResolverEntry, ResolverSection};

/// Resolvers in the order scutil prints them. Resolvers without nameservers
/// (mDNS and friends) are dropped.
pub fn parse_scutil_dns(text: &str) -> Vec<ResolverEntry> {
    let mut out = Vec::new();
    let mut section = ResolverSection::Main;
    let mut cur: Option<ResolverEntry> = None;

    fn flush(cur: &mut Option<ResolverEntry>, out: &mut Vec<ResolverEntry>) {
        if let Some(e) = cur.take() {
            if !e.nameservers.is_empty() {
                out.push(e);
            }
        }
    }

    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("DNS configuration (for scoped queries)") {
            flush(&mut cur, &mut out);
            section = ResolverSection::Scoped;
            continue;
        }
        if line.starts_with("DNS configuration") {
            flush(&mut cur, &mut out);
            section = ResolverSection::Main;
            continue;
        }
        if let Some(rest) = line.strip_prefix("resolver #") {
            flush(&mut cur, &mut out);
            let index = rest.trim().parse().unwrap_or(0);
            cur = Some(ResolverEntry {
                section,
                index,
                domain: None,
                nameservers: vec![],
                interface: None,
            });
            continue;
        }
        let Some(entry) = cur.as_mut() else { continue };
        let Some((key, val)) = line.split_once(':') else {
            continue;
        };
        let (key, val) = (key.trim(), val.trim());
        if key == "domain" {
            entry.domain = Some(val.to_string());
        } else if key.starts_with("nameserver[") {
            entry.nameservers.push(val.to_string());
        } else if key == "if_index" {
            // `14 (en0)`
            if let (Some(a), Some(b)) = (val.find('('), val.rfind(')')) {
                if a < b {
                    entry.interface = Some(val[a + 1..b].to_string());
                }
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured on macOS with a VPN connected, then anonymized.
    const SAMPLE: &str = "\
DNS configuration

resolver #1
  search domain[0] : corp.example
  search domain[1] : internal.example
  nameserver[0] : 198.51.100.1
  nameserver[1] : 198.51.100.2
  nameserver[2] : 198.51.100.3
  flags    : Supplemental, Request A records
  reach    : 0x00030002 (Reachable,Local Address,Directly Reachable Address)
  order    : 104200

resolver #2
  nameserver[0] : 192.0.2.1
  nameserver[1] : fe80::1%en0
  if_index : 14 (en0)
  flags    : Request A records
  reach    : 0x00020002 (Reachable,Directly Reachable Address)
  order    : 200000

resolver #3
  domain   : internal.example
  nameserver[0] : 198.51.100.1
  nameserver[1] : 198.51.100.2
  flags    : Supplemental, Request A records
  order    : 104203

resolver #4
  domain   : local
  options  : mdns
  timeout  : 5
  flags    : Request A records
  order    : 300000

DNS configuration (for scoped queries)

resolver #1
  nameserver[0] : 192.0.2.1
  if_index : 14 (en0)
  flags    : Scoped, Request A records
  order    : 1
";

    #[test]
    fn parses_main_and_scoped_resolvers() {
        let r = parse_scutil_dns(SAMPLE);
        assert_eq!(r.len(), 4, "mDNS resolver without nameservers is dropped");

        assert_eq!(r[0].section, ResolverSection::Main);
        assert_eq!(r[0].index, 1);
        assert_eq!(r[0].domain, None, "`search domain[n]` is not `domain`");
        assert_eq!(
            r[0].nameservers,
            ["198.51.100.1", "198.51.100.2", "198.51.100.3"]
        );
        assert_eq!(r[0].interface, None);

        assert_eq!(r[1].index, 2);
        assert_eq!(r[1].nameservers, ["192.0.2.1", "fe80::1%en0"]);
        assert_eq!(r[1].interface.as_deref(), Some("en0"));

        assert_eq!(r[2].domain.as_deref(), Some("internal.example"));

        assert_eq!(r[3].section, ResolverSection::Scoped);
        assert_eq!(r[3].index, 1);
        assert_eq!(r[3].interface.as_deref(), Some("en0"));
    }

    #[test]
    fn empty_output_has_no_resolvers() {
        assert!(parse_scutil_dns("").is_empty());
        assert!(parse_scutil_dns("DNS configuration\n").is_empty());
    }
}
