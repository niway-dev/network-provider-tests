//! nettest core: checks, probes, diagnosis, run record and store.

pub mod diagnosis;
pub mod model;
pub mod probes;
pub mod record;
pub mod store;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
