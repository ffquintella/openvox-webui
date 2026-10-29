//! Versioned catalog of the UI slots plugins may contribute to (T85,
//! PLUGIN_PLATFORM.md section 8.1).
//!
//! The catalog is the single source of truth for slot names, the context
//! each slot hands to a contribution and how many contributions it shows.
//! Removing or renaming a slot needs a major bump of `SLOT_CATALOG_VERSION`.

use serde::Serialize;

pub const SLOT_CATALOG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    /// Menu entries declared under `contributes.menus`.
    Menu,
    /// A full screen under `/p/<plugin>/...`.
    Page,
    /// A card, tab or panel inside a host page.
    Panel,
    /// A button or small control in a host toolbar.
    Action,
    /// A column or data source contributed to a host table or builder.
    Column,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SlotSpec {
    pub name: &'static str,
    pub kind: SlotKind,
    /// Keys of the context object the slot passes to a contribution.
    pub context: &'static [&'static str],
    /// Most contributions shown at once; the rest are dropped by order.
    pub max_items: usize,
    /// Whether the current host UI renders this slot. The others are
    /// reserved for the milestone that adds the host feature (the catalog
    /// already accepts contributions so plugins do not need a new version).
    pub mounted: bool,
    /// Pixel height an iframe contribution starts with (pages fill the view).
    pub default_height: u32,
}

const fn slot(
    name: &'static str,
    kind: SlotKind,
    context: &'static [&'static str],
    max_items: usize,
    mounted: bool,
    default_height: u32,
) -> SlotSpec {
    SlotSpec {
        name,
        kind,
        context,
        max_items,
        mounted,
        default_height,
    }
}

use SlotKind::*;

pub const SLOT_CATALOG: &[SlotSpec] = &[
    slot("nav.main", Menu, &["user"], 20, true, 0),
    slot("nav.admin", Menu, &["user"], 20, true, 0),
    slot("nav.user-menu", Menu, &["user"], 10, true, 0),
    slot("toolbar.global", Action, &["user", "route"], 4, true, 40),
    slot("dashboard.widgets", Panel, &["period"], 12, true, 240),
    slot(
        "node.detail.tabs",
        Panel,
        &["certname", "environment"],
        6,
        true,
        480,
    ),
    slot(
        "node.detail.actions",
        Action,
        &["certname", "environment"],
        4,
        true,
        40,
    ),
    slot("group.detail.panels", Panel, &["group_id"], 4, true, 240),
    slot(
        "report.detail.panels",
        Panel,
        &["report_hash"],
        4,
        true,
        240,
    ),
    slot("footer.global", Panel, &[], 1, true, 32),
    slot("settings.sections", Panel, &[], 8, true, 320),
    slot("profile.security", Panel, &["user"], 4, false, 240),
    slot("admin.auth", Panel, &[], 4, false, 320),
    slot("node.detail.facts", Panel, &["certname"], 4, false, 240),
    slot("report.list.columns", Column, &["filters"], 6, false, 0),
    slot("report.export.actions", Action, &["filters"], 4, false, 40),
    slot("reports.builder.sources", Column, &[], 8, false, 0),
    slot("group.rule.editors", Panel, &["group_id"], 4, false, 240),
    slot("group.members.columns", Column, &["group_id"], 6, false, 0),
    slot(
        "classification.explain.panels",
        Panel,
        &["certname"],
        4,
        false,
        240,
    ),
    slot("page", Page, &["params"], 100, true, 600),
];

pub fn find(name: &str) -> Option<&'static SlotSpec> {
    SLOT_CATALOG.iter().find(|s| s.name == name)
}

pub fn is_known(name: &str) -> bool {
    find(name).is_some()
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotCatalog {
    pub version: u32,
    pub slots: Vec<SlotSpec>,
}

pub fn catalog() -> SlotCatalog {
    SlotCatalog {
        version: SLOT_CATALOG_VERSION,
        slots: SLOT_CATALOG.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn names_are_unique_and_limits_positive() {
        let names: BTreeSet<_> = SLOT_CATALOG.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), SLOT_CATALOG.len());
        assert!(SLOT_CATALOG.iter().all(|s| s.max_items > 0));
    }

    #[test]
    fn covers_every_slot_of_the_spec() {
        for name in [
            "nav.main",
            "nav.admin",
            "nav.user-menu",
            "toolbar.global",
            "dashboard.widgets",
            "node.detail.tabs",
            "node.detail.actions",
            "group.detail.panels",
            "report.detail.panels",
            "footer.global",
            "settings.sections",
            "profile.security",
            "admin.auth",
            "node.detail.facts",
            "report.list.columns",
            "report.export.actions",
            "reports.builder.sources",
            "group.rule.editors",
            "group.members.columns",
            "classification.explain.panels",
            "page",
        ] {
            assert!(is_known(name), "{name}");
        }
        assert!(!is_known("login.form"));
    }
}
