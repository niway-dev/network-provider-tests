//! Fallback traceroute: run the system `traceroute -n -w 1 -q 1 -m 15` and
//! parse it structurally (hop number, IP literal, `*`), never by words, so
//! localized output does not matter.

use std::net::IpAddr;
use std::time::Duration;

use crate::probes::{Hop, Trace};

/// Parse one output line into a hop. Lines that do not start with a hop
/// number (headers, notes) yield `None`.
pub fn parse_hop_line(line: &str) -> Option<Hop> {
    let mut tokens = line.split_whitespace();
    let hop: u8 = tokens.next()?.parse().ok()?;
    let rest: Vec<&str> = tokens.collect();
    let ip = rest.iter().find_map(|t| {
        t.trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']')
            .parse::<IpAddr>()
            .ok()
    });
    let Some(ip) = ip else {
        return Some(Hop {
            hop,
            ip: None,
            ms: None,
        });
    };
    // First number that is followed by `ms` (or ends in `ms`): `0.789 ms`, `<1 ms`, `12ms`.
    let mut ms = None;
    for (i, t) in rest.iter().enumerate() {
        let t = t.trim_start_matches('<');
        if let Some(num) = t.strip_suffix("ms") {
            if let Ok(v) = num.parse::<f64>() {
                ms = Some(v);
                break;
            }
        }
        if rest.get(i + 1) == Some(&"ms") {
            if let Ok(v) = t.parse::<f64>() {
                ms = Some(v);
                break;
            }
        }
    }
    Some(Hop {
        hop,
        ip: Some(ip.to_string()),
        ms,
    })
}

pub fn parse_output(text: &str) -> Vec<Hop> {
    text.lines().filter_map(parse_hop_line).collect()
}

pub async fn traceroute(target: &str, max_hops: u8) -> Trace {
    let run = tokio::process::Command::new("traceroute")
        .args([
            "-n",
            "-w",
            "1",
            "-q",
            "1",
            "-m",
            &max_hops.to_string(),
            target,
        ])
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(40), run).await {
        Ok(Ok(out)) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let hops = parse_output(&text);
            let error = if hops.is_empty() {
                text.lines().next().map(str::to_string)
            } else {
                None
            };
            Trace { hops, error }
        }
        Ok(Err(e)) => Trace {
            hops: vec![],
            error: Some(format!("could not run traceroute: {e}")),
        },
        Err(_) => Trace {
            hops: vec![],
            error: Some("traceroute timed out".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_english_output() {
        let text = "\
traceroute to 1.1.1.1 (1.1.1.1), 15 hops max, 40 byte packets
 1  192.0.2.1  6.551 ms
 2  *
 3  10.20.0.1  33.022 ms
 9  1.1.1.1  27.585 ms
";
        let hops = parse_output(text);
        assert_eq!(hops.len(), 4);
        assert_eq!(
            hops[0],
            Hop {
                hop: 1,
                ip: Some("192.0.2.1".into()),
                ms: Some(6.551)
            }
        );
        assert_eq!(
            hops[1],
            Hop {
                hop: 2,
                ip: None,
                ms: None
            }
        );
        assert_eq!(hops[3].hop, 9);
        assert_eq!(hops[3].ip.as_deref(), Some("1.1.1.1"));
    }

    #[test]
    fn spanish_windows_tracert_output() {
        // Localized words and `<1 ms` / `*` columns must not matter.
        let text = "\
Traza a 1.1.1.1 sobre un maximo de 15 saltos

  1    <1 ms    <1 ms    <1 ms  192.0.2.1
  2     *        *        *     Tiempo de espera agotado para esta solicitud.
  3    12 ms    11 ms    13 ms  203.0.113.5

Traza completa.
";
        let hops = parse_output(text);
        assert_eq!(hops.len(), 3);
        assert_eq!(
            hops[0],
            Hop {
                hop: 1,
                ip: Some("192.0.2.1".into()),
                ms: Some(1.0)
            }
        );
        assert_eq!(
            hops[1],
            Hop {
                hop: 2,
                ip: None,
                ms: None
            }
        );
        assert_eq!(
            hops[2],
            Hop {
                hop: 3,
                ip: Some("203.0.113.5".into()),
                ms: Some(12.0)
            }
        );
    }

    #[test]
    fn header_and_error_lines_are_ignored() {
        assert!(parse_output("traceroute: sendto: No route to host\n").is_empty());
        assert!(parse_output("").is_empty());
    }

    #[test]
    fn ipv6_hops_parse() {
        let h = parse_hop_line(" 2  2001:db8::1  1.5 ms").unwrap();
        assert_eq!(h.ip.as_deref(), Some("2001:db8::1"));
        assert_eq!(h.ms, Some(1.5));
    }
}
