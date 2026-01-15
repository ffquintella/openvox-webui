//! Bootstrap API endpoints for node enrollment
//!
//! Provides PUBLIC endpoints for downloading bootstrap scripts
//! that configure new Puppet agents to connect to the infrastructure.
//!
//! These endpoints are intentionally public (no authentication required)
//! so that new nodes can easily download and run the bootstrap script.

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::AppState;

/// Public routes (no authentication required)
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_bootstrap_config))
        .route("/script", get(get_bootstrap_script))
}

/// Response containing bootstrap configuration
#[derive(Debug, Serialize)]
pub struct BootstrapConfigResponse {
    /// Puppet Server URL that agents will connect to
    pub puppet_server_url: Option<String>,
    /// Custom repository base URL for packages
    pub repository_base_url: Option<String>,
    /// Package name to install
    pub agent_package_name: String,
    /// WebUI URL (for display in instructions)
    pub webui_url: String,
}

/// GET /api/v1/bootstrap/config
///
/// Returns bootstrap configuration as JSON.
/// This is useful for the frontend to display configuration details.
pub async fn get_bootstrap_config(State(state): State<AppState>) -> Json<BootstrapConfigResponse> {
    let config = state.config.node_bootstrap.as_ref();

    // Build webui_url from server config
    let protocol = if state.config.server.tls.is_some() {
        "https"
    } else {
        "http"
    };
    let webui_url = format!(
        "{}://{}:{}",
        protocol, state.config.server.host, state.config.server.port
    );

    Json(BootstrapConfigResponse {
        puppet_server_url: config.and_then(|c| c.puppet_server_url.clone()),
        repository_base_url: config.and_then(|c| c.repository_base_url.clone()),
        agent_package_name: config
            .map(|c| c.agent_package_name.clone())
            .unwrap_or_else(|| "openvox-agent".to_string()),
        webui_url,
    })
}

/// GET /api/v1/bootstrap/script
///
/// Returns a dynamically generated bootstrap shell script.
/// The script will detect the OS and install/configure the Puppet agent.
pub async fn get_bootstrap_script(State(state): State<AppState>) -> Response {
    let config = state.config.node_bootstrap.as_ref();

    let puppet_server = config
        .and_then(|c| c.puppet_server_url.clone())
        .unwrap_or_else(|| "puppet".to_string());

    let repo_url = config
        .and_then(|c| c.repository_base_url.clone())
        .unwrap_or_default();

    let package_name = config
        .map(|c| c.agent_package_name.clone())
        .unwrap_or_else(|| "openvox-agent".to_string());

    let script = generate_bootstrap_script(&puppet_server, &repo_url, &package_name);

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"bootstrap-openvox-agent.sh\"",
            ),
        ],
        script,
    )
        .into_response()
}

/// Generate the bootstrap script with configuration values injected
fn generate_bootstrap_script(puppet_server: &str, repo_url: &str, package_name: &str) -> String {
    let script_template = include_str!("../../scripts/bootstrap-agent.sh");

    script_template
        .replace("{{PUPPET_SERVER}}", puppet_server)
        .replace("{{REPO_BASE_URL}}", repo_url)
        .replace("{{PACKAGE_NAME}}", package_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_bootstrap_script_replaces_placeholders() {
        let script = generate_bootstrap_script(
            "puppet.example.com",
            "https://yum.example.com/openvox",
            "openvox-agent",
        );

        assert!(script.contains("puppet.example.com"));
        assert!(script.contains("https://yum.example.com/openvox"));
        assert!(script.contains("openvox-agent"));
        assert!(!script.contains("{{PUPPET_SERVER}}"));
        assert!(!script.contains("{{REPO_BASE_URL}}"));
        assert!(!script.contains("{{PACKAGE_NAME}}"));
    }
}
