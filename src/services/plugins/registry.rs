//! Slot registry (T72): the menu, page and widget contributions of the
//! enabled plugins, built from verified packages and served to the UI.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use super::manifest::safe_package_path;
use super::modules::{is_module_file, sri_matches, DEFAULT_EXPOSE};
use super::package::PluginPackage;
use super::slots as catalog;
use super::ui_assets::is_bundle_file;
use crate::models::{
    MenuEntry, RenderKind, Requires, UiContribution, UiContributionDetail, UiMenuOp, UiSlots,
};

const MAX_SPEC_BYTES: usize = 256 * 1024;
const MAX_BUNDLE_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_PLUGIN_ORDER: i32 = 1000;

#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub contribution: UiContribution,
    pub requires: Option<Requires>,
    pub spec: Option<Value>,
    /// Iframe contributions: the HTML entry inside the plugin's web bundle.
    pub src: Option<String>,
}

/// The static files of one enabled plugin that iframe contributions load.
#[derive(Debug, Clone)]
pub struct UiBundle {
    pub version: String,
    pub files: BTreeMap<String, Arc<Vec<u8>>>,
}

/// What the host needs to open an iframe contribution.
#[derive(Debug, Clone)]
pub struct IframeTarget {
    pub plugin_id: String,
    pub version: String,
    pub src: String,
}

/// The entry file of a trusted `module` contribution (M31).
#[derive(Debug, Clone)]
pub struct ModuleAsset {
    pub plugin_id: String,
    pub version: String,
    pub contribution_id: String,
    pub path: String,
    pub integrity: String,
    pub expose: String,
    pub bytes: Arc<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct Registry {
    entries: Vec<RegistryEntry>,
    menu_ops: Vec<UiMenuOp>,
    bundles: BTreeMap<String, UiBundle>,
    /// Trusted module entries by (plugin id, contribution id).
    modules: BTreeMap<(String, String), ModuleAsset>,
}

impl Registry {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.menu_ops.is_empty()
    }

    /// Adds the contributions of one verified, enabled plugin. `module`
    /// contributions are added only when `trusted_modules` (the
    /// configuration allows them and an administrator trusted this exact
    /// package); contributions whose spec or entry file is unusable are
    /// skipped, never served half-working.
    pub fn add_plugin(&mut self, pkg: &PluginPackage, trusted_modules: bool) {
        let m = &pkg.manifest;
        for s in &m.contributes.slots {
            if s.render == RenderKind::Module {
                if trusted_modules {
                    self.add_module(pkg, s);
                } else {
                    tracing::debug!("plugin {}: untrusted module '{}' skipped", m.id, s.id);
                }
                continue;
            }
            if s.render == RenderKind::Iframe {
                let Some(src) = s.src.clone().filter(|f| {
                    is_bundle_file(f)
                        && f.rsplit_once('.').is_some_and(|(_, e)| {
                            e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm")
                        })
                        && pkg.files.contains_key(f)
                }) else {
                    tracing::warn!("plugin {}: entry of '{}' is unusable, skipped", m.id, s.id);
                    continue;
                };
                self.add_bundle(pkg);
                self.entries.push(RegistryEntry {
                    contribution: UiContribution {
                        plugin_id: m.id.clone(),
                        plugin_name: m.name.clone(),
                        slot: s.slot.clone(),
                        id: s.id.clone(),
                        render: s.render,
                        title: s.title.clone(),
                        path: s.path.clone(),
                        order: s.order.unwrap_or(DEFAULT_PLUGIN_ORDER),
                        label: None,
                        icon: None,
                        href: None,
                        integrity: None,
                        expose: None,
                    },
                    requires: s.requires.clone(),
                    spec: None,
                    src: Some(src),
                });
                continue;
            }
            let Some(spec) = s.spec.as_deref().and_then(|f| load_spec(pkg, f)) else {
                tracing::warn!("plugin {}: spec of '{}' is unusable, skipped", m.id, s.id);
                continue;
            };
            self.entries.push(RegistryEntry {
                contribution: UiContribution {
                    plugin_id: m.id.clone(),
                    plugin_name: m.name.clone(),
                    slot: s.slot.clone(),
                    id: s.id.clone(),
                    render: s.render,
                    title: s.title.clone(),
                    path: s.path.clone(),
                    order: s.order.unwrap_or(DEFAULT_PLUGIN_ORDER),
                    label: None,
                    icon: None,
                    href: None,
                    integrity: None,
                    expose: None,
                },
                requires: s.requires.clone(),
                spec: Some(spec),
                src: None,
            });
        }
        for entry in &m.contributes.menus {
            match entry {
                MenuEntry::Item(i) => self.entries.push(RegistryEntry {
                    contribution: UiContribution {
                        plugin_id: m.id.clone(),
                        plugin_name: m.name.clone(),
                        slot: format!("nav.{}", i.section),
                        id: i.id.clone(),
                        render: RenderKind::Declarative,
                        title: None,
                        path: None,
                        order: i.order.unwrap_or(DEFAULT_PLUGIN_ORDER),
                        label: Some(i.label.clone()),
                        icon: i.icon.clone(),
                        href: Some(i.href.clone()),
                        integrity: None,
                        expose: None,
                    },
                    requires: i.requires.clone(),
                    spec: None,
                    src: None,
                }),
                MenuEntry::Op(o) => self.menu_ops.push(UiMenuOp {
                    plugin_id: m.id.clone(),
                    target: o.target.clone(),
                    op: o.op.clone(),
                    label: o.label.clone(),
                }),
            }
        }
    }

    /// Adds one trusted module whose entry file is in the package, within
    /// the size limit and matching its SRI (checked again here).
    fn add_module(&mut self, pkg: &PluginPackage, s: &crate::models::SlotContribution) {
        let m = &pkg.manifest;
        let found = s
            .src
            .as_ref()
            .zip(s.integrity.as_ref())
            .and_then(|(src, sri)| {
                let bytes = pkg.files.get(src)?;
                (is_module_file(src)
                    && bytes.len() <= MAX_BUNDLE_FILE_BYTES
                    && sri_matches(sri, bytes))
                .then(|| (src.clone(), sri.clone(), bytes))
            });
        let Some((src, integrity, bytes)) = found else {
            tracing::warn!(
                "plugin {}: module entry of '{}' is unusable, skipped",
                m.id,
                s.id
            );
            return;
        };
        let expose = s
            .expose
            .clone()
            .unwrap_or_else(|| DEFAULT_EXPOSE.to_string());
        self.modules.insert(
            (m.id.clone(), s.id.clone()),
            ModuleAsset {
                plugin_id: m.id.clone(),
                version: m.version.clone(),
                contribution_id: s.id.clone(),
                path: src.clone(),
                integrity: integrity.clone(),
                expose: expose.clone(),
                bytes: Arc::new(bytes.clone()),
            },
        );
        self.entries.push(RegistryEntry {
            contribution: UiContribution {
                plugin_id: m.id.clone(),
                plugin_name: m.name.clone(),
                slot: s.slot.clone(),
                id: s.id.clone(),
                render: RenderKind::Module,
                title: s.title.clone(),
                path: s.path.clone(),
                order: s.order.unwrap_or(DEFAULT_PLUGIN_ORDER),
                label: None,
                icon: None,
                href: None,
                integrity: Some(integrity),
                expose: Some(expose),
            },
            requires: s.requires.clone(),
            spec: None,
            src: Some(src),
        });
    }

    /// True when the plugin has trusted modules in play.
    pub fn has_modules(&self, plugin_id: &str) -> bool {
        self.modules.keys().any(|(p, _)| p == plugin_id)
    }

    /// The trusted module of a contribution the caller may see.
    pub fn find_module(
        &self,
        plugin_id: &str,
        contribution_id: &str,
        allow: &dyn Fn(&Option<Requires>) -> bool,
    ) -> Option<ModuleAsset> {
        let visible = self.entries.iter().any(|e| {
            e.contribution.plugin_id == plugin_id
                && e.contribution.id == contribution_id
                && e.contribution.render == RenderKind::Module
                && allow(&e.requires)
        });
        visible
            .then(|| {
                self.modules
                    .get(&(plugin_id.to_string(), contribution_id.to_string()))
                    .cloned()
            })
            .flatten()
    }

    /// The module entry of a contribution, only while that exact version is
    /// the enabled and trusted one.
    pub fn module_asset(
        &self,
        plugin_id: &str,
        version: &str,
        contribution_id: &str,
    ) -> Option<ModuleAsset> {
        self.modules
            .get(&(plugin_id.to_string(), contribution_id.to_string()))
            .filter(|a| a.version == version)
            .cloned()
    }

    fn add_bundle(&mut self, pkg: &PluginPackage) {
        let id = &pkg.manifest.id;
        if self.bundles.contains_key(id) {
            return;
        }
        let mut total = 0usize;
        let mut files = BTreeMap::new();
        for (path, bytes) in &pkg.files {
            if !is_bundle_file(path) {
                continue;
            }
            if bytes.len() > MAX_BUNDLE_FILE_BYTES || total + bytes.len() > MAX_BUNDLE_BYTES {
                tracing::warn!("plugin {id}: web file '{path}' is over the size limit, skipped");
                continue;
            }
            total += bytes.len();
            files.insert(path.clone(), Arc::new(bytes.clone()));
        }
        self.bundles.insert(
            id.clone(),
            UiBundle {
                version: pkg.manifest.version.clone(),
                files,
            },
        );
    }

    /// One file of a plugin's web bundle, only while that exact version is
    /// the enabled one.
    pub fn bundle_file(&self, plugin_id: &str, version: &str, path: &str) -> Option<Arc<Vec<u8>>> {
        let b = self.bundles.get(plugin_id)?;
        (b.version == version).then(|| b.files.get(path).cloned())?
    }

    /// The iframe entry of a contribution the caller may see.
    pub fn find_iframe(
        &self,
        plugin_id: &str,
        contribution_id: &str,
        allow: &dyn Fn(&Option<Requires>) -> bool,
    ) -> Option<IframeTarget> {
        let e = self.entries.iter().find(|e| {
            e.contribution.plugin_id == plugin_id
                && e.contribution.id == contribution_id
                && e.contribution.render == RenderKind::Iframe
                && allow(&e.requires)
        })?;
        Some(IframeTarget {
            plugin_id: plugin_id.to_string(),
            version: self.bundles.get(plugin_id)?.version.clone(),
            src: e.src.clone()?,
        })
    }

    /// Distinct permissions the registry's contributions are gated on.
    pub fn required_permissions(&self) -> Vec<Requires> {
        let mut out: Vec<Requires> = Vec::new();
        for r in self.entries.iter().filter_map(|e| e.requires.as_ref()) {
            if !out.contains(r) {
                out.push(r.clone());
            }
        }
        out
    }

    /// The registry as the UI sees it, keeping only what `allow` lets through.
    pub fn to_ui(&self, allow: &dyn Fn(&Option<Requires>) -> bool) -> UiSlots {
        let mut slots: BTreeMap<String, Vec<UiContribution>> = BTreeMap::new();
        for e in self.entries.iter().filter(|e| allow(&e.requires)) {
            slots
                .entry(e.contribution.slot.clone())
                .or_default()
                .push(e.contribution.clone());
        }
        for (name, list) in slots.iter_mut() {
            list.sort_by(|a, b| {
                (a.order, &a.plugin_id, &a.id).cmp(&(b.order, &b.plugin_id, &b.id))
            });
            // Each slot shows at most `max_items` contributions.
            if let Some(spec) = catalog::find(name) {
                list.truncate(spec.max_items);
            }
        }
        let mut menu_ops = self.menu_ops.clone();
        // `hide` wins over `rename`; otherwise the order of plugin ids is stable.
        menu_ops
            .sort_by(|a, b| (a.op != "hide", &a.plugin_id).cmp(&(b.op != "hide", &b.plugin_id)));
        UiSlots { slots, menu_ops }
    }

    pub fn find(
        &self,
        plugin_id: &str,
        contribution_id: &str,
        allow: &dyn Fn(&Option<Requires>) -> bool,
    ) -> Option<UiContributionDetail> {
        self.entries
            .iter()
            .find(|e| {
                e.contribution.plugin_id == plugin_id
                    && e.contribution.id == contribution_id
                    && e.spec.is_some()
                    && allow(&e.requires)
            })
            .map(|e| UiContributionDetail {
                contribution: e.contribution.clone(),
                spec: e.spec.clone(),
            })
    }
}

fn load_spec(pkg: &PluginPackage, file: &str) -> Option<Value> {
    if !safe_package_path(file) {
        return None;
    }
    let bytes = pkg.files.get(file)?;
    if bytes.len() > MAX_SPEC_BYTES {
        return None;
    }
    serde_json::from_slice(bytes).ok()
}
