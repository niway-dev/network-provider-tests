//! The seven checks. Each is `async fn run(&dyn Probes) -> CheckRun`; its
//! metrics are exactly the `checks.<name>` object of the run record.

use crate::model::{CheckId, Line};
use crate::probes::Probes;
use crate::record::Metrics;

pub mod dns;
pub mod http_timing;
pub mod ipv4_reachability;
pub mod ipv6_reachability;
pub mod local_network;
pub mod reach;
pub mod route_trace;
pub mod vpn_detection;

pub struct CheckRun {
    pub lines: Vec<Line>,
    pub metrics: Metrics,
}

pub async fn run(id: CheckId, probes: &dyn Probes) -> CheckRun {
    match id {
        CheckId::LocalNetwork => local_network::run(probes).await,
        CheckId::VpnDetection => vpn_detection::run(probes).await,
        CheckId::Ipv4Reachability => ipv4_reachability::run(probes).await,
        CheckId::Ipv6Reachability => ipv6_reachability::run(probes).await,
        CheckId::RouteTrace => route_trace::run(probes).await,
        CheckId::Dns => dns::run(probes).await,
        CheckId::HttpTiming => http_timing::run(probes).await,
    }
}

/// `Some(s)` unless `s` is empty (bash writes empty values as `null`).
pub(crate) fn non_empty(s: Option<&str>) -> Option<String> {
    s.filter(|v| !v.is_empty()).map(str::to_string)
}
