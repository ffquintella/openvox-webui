//! Slot registry (T72): the menu, page and widget contributions of the
//! enabled plugins, built from verified packages and served to the UI.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use super::manifest::safe_package_path;
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

#[derive(Debug, Clone, Default)]
pub struct Registry {
    entries: Vec<RegistryEntry>,
    menu_ops: Vec<UiMenuOp>,
    bundles: BTreeMap<String, UiBundle>,
}

impl Registry {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.menu_ops.is_empty()
    }

    /// Adds the contributions of one verified, enabled plugin. Trusted
    /// modules (never allowed at install) and contributions whose spec or
    /// entry file is unusable are skipped, never served half-working.
    pub fn add_plugin(&mut self, pkg: &PluginPackage) {
        let m = &pkg.manifest;
        for s in &m.contributes.slots {
            if s.render == RenderKind::Module {
                tracing::debug!("plugin {}: module contribution '{}' skipped", m.id, s.id);
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
