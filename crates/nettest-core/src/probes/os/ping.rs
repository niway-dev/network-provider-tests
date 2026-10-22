//! ICMP echo through an unprivileged datagram socket (`ping-async`).

use std::net::IpAddr;
use std::time::Duration;

use ping_async::{IcmpEchoRequestor, IcmpEchoStatus};

use crate::probes::PingStats;

const INTERVAL: Duration = Duration::from_millis(400);

/// `count` echo requests about 400 ms apart; each waits up to `timeout`.
pub async fn ping(target: &str, count: u32, timeout: Duration) -> PingStats {
    let Ok(addr) = target.parse::<IpAddr>() else {
        return PingStats::from_rtts(count, &[]);
    };
    let Ok(requestor) = IcmpEchoRequestor::new(addr, None, None, Some(timeout)) else {
        return PingStats::from_rtts(count, &[]);
    };
    let mut rtts = Vec::new();
    for i in 0..count {
        let started = tokio::time::Instant::now();
        if let Ok(reply) = requestor.send().await {
            if reply.status() == IcmpEchoStatus::Success {
                rtts.push(reply.round_trip_time().as_secs_f64() * 1000.0);
            }
        }
        if i + 1 < count {
            tokio::time::sleep_until(started + INTERVAL).await;
        }
    }
    PingStats::from_rtts(count, &rtts)
}
