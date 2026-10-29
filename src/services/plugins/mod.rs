//! Plugin platform services (M19): manifest, packages, signatures, manager
//! and the slot registry.

pub mod bootstrap;
pub mod events;
pub mod facts;
pub mod filter;
pub mod governance;
pub mod host;
pub mod invoke;
pub mod jobs;
pub mod manager;
pub mod manifest;
pub mod package;
pub mod registry;
pub mod reports;
pub mod runtime;
pub mod slots;
pub mod storage;
pub mod ui_assets;
pub mod updates;
pub mod wasm;
pub mod webhooks;

pub use manager::{
    BuiltinPlugin, InstallOptions, InstallOutcome, PluginError, PluginManager, TrustRequest,
    AUDIT_PLUGINS, AUDIT_TRUST,
};
pub use registry::Registry;
pub use runtime::PluginRuntime;
pub use updates::UpdateActor;
