//! Plugin platform services (M19): manifest, packages, signatures, manager
//! and the slot registry.

pub mod events;
pub mod filter;
pub mod invoke;
pub mod jobs;
pub mod manager;
pub mod manifest;
pub mod package;
pub mod registry;
pub mod runtime;
pub mod webhooks;

pub use manager::{
    BuiltinPlugin, InstallOutcome, PluginError, PluginManager, TrustRequest, AUDIT_PLUGINS,
    AUDIT_TRUST,
};
pub use registry::Registry;
pub use runtime::PluginRuntime;
