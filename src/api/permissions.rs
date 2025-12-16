//! Permissions API endpoints

use axum::{
    extract::State,
    routing::get,
    Json, Router,
};

use crate::{
    models::{Action, Permission, Resource},
    AppState,
};

/// Create routes for permission endpoints
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_permissions))
        .route("/resources", get(list_resources))
        .route("/actions", get(list_actions))
}

/// List all defined permissions
async fn list_permissions(State(_state): State<AppState>) -> Json<Vec<Permission>> {
    // Return all permission combinations
    // In production, this would come from the database
    Json(vec![])
}

/// List all available resources
async fn list_resources(State(_state): State<AppState>) -> Json<Vec<ResourceInfo>> {
    let resources: Vec<ResourceInfo> = Resource::all()
        .iter()
        .map(|r| ResourceInfo {
            name: r.as_str().to_string(),
            display_name: format_resource_name(r),
            description: get_resource_description(r),
            available_actions: get_resource_actions(r),
        })
        .collect();

    Json(resources)
}

/// List all available actions
async fn list_actions(State(_state): State<AppState>) -> Json<Vec<ActionInfo>> {
    let actions: Vec<ActionInfo> = Action::all()
        .iter()
        .map(|a| ActionInfo {
            name: a.as_str().to_string(),
            display_name: format_action_name(a),
            description: get_action_description(a),
        })
        .collect();

    Json(actions)
}

#[derive(serde::Serialize)]
pub struct ResourceInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub available_actions: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct ActionInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
}

fn format_resource_name(resource: &Resource) -> String {
    match resource {
        Resource::Nodes => "Nodes".to_string(),
        Resource::Groups => "Node Groups".to_string(),
        Resource::Reports => "Reports".to_string(),
        Resource::Facts => "Facts".to_string(),
        Resource::Users => "Users".to_string(),
        Resource::Roles => "Roles".to_string(),
        Resource::Settings => "Settings".to_string(),
        Resource::AuditLogs => "Audit Logs".to_string(),
        Resource::FacterTemplates => "Facter Templates".to_string(),
        Resource::ApiKeys => "API Keys".to_string(),
    }
}

fn get_resource_description(resource: &Resource) -> String {
    match resource {
        Resource::Nodes => "Infrastructure nodes from PuppetDB".to_string(),
        Resource::Groups => "Node classification groups".to_string(),
        Resource::Reports => "Puppet run reports".to_string(),
        Resource::Facts => "Node facts from Facter".to_string(),
        Resource::Users => "User accounts".to_string(),
        Resource::Roles => "RBAC roles".to_string(),
        Resource::Settings => "System configuration".to_string(),
        Resource::AuditLogs => "Activity audit logs".to_string(),
        Resource::FacterTemplates => "Templates for generating external facts".to_string(),
        Resource::ApiKeys => "API authentication keys".to_string(),
    }
}

fn get_resource_actions(resource: &Resource) -> Vec<String> {
    match resource {
        Resource::Nodes => vec!["read", "classify"],
        Resource::Groups => vec!["read", "create", "update", "delete", "admin"],
        Resource::Reports => vec!["read", "export"],
        Resource::Facts => vec!["read", "generate", "export"],
        Resource::Users => vec!["read", "create", "update", "delete", "admin"],
        Resource::Roles => vec!["read", "create", "update", "delete", "admin"],
        Resource::Settings => vec!["read", "update"],
        Resource::AuditLogs => vec!["read"],
        Resource::FacterTemplates => vec!["read", "create", "update", "delete"],
        Resource::ApiKeys => vec!["read", "create", "delete"],
    }
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn format_action_name(action: &Action) -> String {
    match action {
        Action::Read => "Read".to_string(),
        Action::Create => "Create".to_string(),
        Action::Update => "Update".to_string(),
        Action::Delete => "Delete".to_string(),
        Action::Admin => "Full Admin".to_string(),
        Action::Export => "Export".to_string(),
        Action::Classify => "Classify".to_string(),
        Action::Generate => "Generate".to_string(),
    }
}

fn get_action_description(action: &Action) -> String {
    match action {
        Action::Read => "View and list resources".to_string(),
        Action::Create => "Create new resources".to_string(),
        Action::Update => "Modify existing resources".to_string(),
        Action::Delete => "Remove resources".to_string(),
        Action::Admin => "Full administrative access including all actions".to_string(),
        Action::Export => "Export resource data".to_string(),
        Action::Classify => "Classify nodes into groups".to_string(),
        Action::Generate => "Generate derived data (e.g., facts)".to_string(),
    }
}
