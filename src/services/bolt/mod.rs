//! OpenBolt integration (M32, design in `docs/architecture/BOLT_INTEGRATION.md`).

pub mod config;
pub mod inventory;
pub mod model;
pub mod runner;
pub mod service;
pub mod store;

pub use config::{BoltConfig, OrchestrationConfig};
