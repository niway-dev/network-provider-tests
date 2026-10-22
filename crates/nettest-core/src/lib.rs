//! nettest core: checks, probes, diagnosis, run record and store.

pub mod checks;
pub mod diagnosis;
pub mod history;
pub mod model;
pub mod probes;
pub mod record;
pub mod render;
pub mod runner;
pub mod store;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
