//! Test factories for generating test data
//!
//! Factories create randomized test data, useful for property-based testing
//! and when you need unique data for each test.

use chrono::Utc;
use rand::Rng;
use uuid::Uuid;

use openvox_webui::models::{
    default_organization_uuid, Action, Node, NodeGroup, Permission, Report, ReportStatus, Resource,
    Role, RuleMatchType, Scope,
};

/// Factory for creating test users
pub struct UserFactory {
    counter: std::sync::atomic::AtomicU64,
}

impl Default for UserFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl UserFactory {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Create a unique test user
    pub fn create(&self) -> TestUserBuilder {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        TestUserBuilder {
            id: Uuid::new_v4(),
            username: format!("testuser_{}", n),
            email: format!("testuser_{}@example.com", n),
            roles: vec!["viewer".to_string()],
            role_ids: vec![],
        }
    }
}

/// Builder for test users
pub struct TestUserBuilder {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub roles: Vec<String>,
    pub role_ids: Vec<Uuid>,
}

impl TestUserBuilder {
    pub fn with_username(mut self, username: &str) -> Self {
        self.username = username.to_string();
        self.email = format!("{}@example.com", username);
        self
    }

    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    pub fn with_role_ids(mut self, role_ids: Vec<Uuid>) -> Self {
        self.role_ids = role_ids;
        self
    }

    pub fn build(self) -> crate::common::fixtures::TestUser {
        crate::common::fixtures::TestUser {
            id: self.id,
            username: self.username,
            email: self.email,
            roles: self.roles,
            role_ids: self.role_ids,
        }
    }
}

/// Factory for creating test nodes
pub struct NodeFactory {
    counter: std::sync::atomic::AtomicU64,
}

impl Default for NodeFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeFactory {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Create a unique test node
    pub fn create(&self) -> NodeBuilder {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        NodeBuilder {
            certname: format!("node{}.example.com", n),
            environment: "production".to_string(),
            status: "unchanged".to_string(),
            deactivated: false,
        }
    }
}

/// Builder for test nodes
pub struct NodeBuilder {
    certname: String,
    environment: String,
    status: String,
    deactivated: bool,
}

impl NodeBuilder {
    pub fn with_certname(mut self, certname: &str) -> Self {
        self.certname = certname.to_string();
        self
    }

    pub fn with_environment(mut self, env: &str) -> Self {
        self.environment = env.to_string();
        self
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = status.to_string();
        self
    }

    pub fn deactivated(mut self) -> Self {
        self.deactivated = true;
        self
    }

    pub fn build(self) -> Node {
        let now = Utc::now();
        Node {
            certname: self.certname.clone(),
            deactivated: if self.deactivated { Some(now) } else { None },
            expired: None,
            catalog_timestamp: Some(now),
            facts_timestamp: Some(now),
            report_timestamp: Some(now),
            catalog_environment: Some(self.environment.clone()),
            facts_environment: Some(self.environment.clone()),
            report_environment: Some(self.environment),
            latest_report_status: Some(self.status),
            latest_report_corrective_change: Some(false),
            cached_catalog_status: None,
        }
    }
}

/// Factory for creating test node groups
pub struct GroupFactory {
    counter: std::sync::atomic::AtomicU64,
}

impl Default for GroupFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupFactory {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Create a unique test group
    pub fn create(&self) -> GroupBuilder {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        GroupBuilder {
            id: Uuid::new_v4(),
            name: format!("group_{}", n),
            description: Some(format!("Test group {}", n)),
            environment: Some("production".to_string()),
            parent_id: None,
            classes: serde_json::json!({}),
        }
    }
}

/// Builder for test groups
pub struct GroupBuilder {
    id: Uuid,
    name: String,
    description: Option<String>,
    environment: Option<String>,
    parent_id: Option<Uuid>,
    classes: serde_json::Value,
}

impl GroupBuilder {
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn with_environment(mut self, env: &str) -> Self {
        self.environment = Some(env.to_string());
        self
    }

    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    pub fn with_classes(mut self, classes: serde_json::Value) -> Self {
        self.classes = classes;
        self
    }

    pub fn build(self) -> NodeGroup {
        NodeGroup {
            id: self.id,
            organization_id: default_organization_uuid(),
            name: self.name,
            description: self.description,
            parent_id: self.parent_id,
            environment: self.environment,
            is_environment_group: false,
            match_all_nodes: false,
            rule_match_type: RuleMatchType::All,
            classes: self.classes,
            variables: serde_json::json!({}),
            rules: vec![],
            pinned_nodes: vec![],
        }
    }
}

/// Factory for creating test reports
pub struct ReportFactory {
    counter: std::sync::atomic::AtomicU64,
}

impl Default for ReportFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ReportFactory {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Create a unique test report
    pub fn create(&self) -> ReportBuilder {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ReportBuilder {
            hash: format!("report_hash_{}", n),
            certname: format!("node{}.example.com", n),
            status: ReportStatus::Changed,
            environment: "production".to_string(),
        }
    }
}

/// Builder for test reports
pub struct ReportBuilder {
    hash: String,
    certname: String,
    status: ReportStatus,
    environment: String,
}

impl ReportBuilder {
    pub fn with_certname(mut self, certname: &str) -> Self {
        self.certname = certname.to_string();
        self
    }

    pub fn with_status(mut self, status: ReportStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_environment(mut self, env: &str) -> Self {
        self.environment = env.to_string();
        self
    }

    pub fn build(self) -> Report {
        let now = Utc::now();
        Report {
            hash: self.hash,
            certname: self.certname,
            puppet_version: Some("8.0.0".to_string()),
            report_format: Some(12),
            configuration_version: Some("1234567890".to_string()),
            start_time: Some(now),
            end_time: Some(now),
            producer_timestamp: Some(now),
            producer: Some("puppetserver.example.com".to_string()),
            transaction_uuid: Some(Uuid::new_v4().to_string()),
            status: Some(self.status),
            corrective_change: Some(false),
            noop: Some(false),
            noop_pending: Some(false),
            environment: Some(self.environment),
            catalog_uuid: Some(Uuid::new_v4().to_string()),
            code_id: Some("abc123".to_string()),
            cached_catalog_status: None,
            report_type: Some("agent".to_string()),
            job_id: None,
            receive_time: Some(now),
            metrics: None,
            resource_events: None,
            logs: None,
        }
    }
}

/// Factory for creating test roles
pub struct RoleFactory {
    counter: std::sync::atomic::AtomicU64,
}

impl Default for RoleFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RoleFactory {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Create a unique test role
    pub fn create(&self) -> RoleBuilder {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        RoleBuilder {
            id: Uuid::new_v4(),
            name: format!("role_{}", n),
            display_name: format!("Role {}", n),
            description: Some(format!("Test role {}", n)),
            permissions: vec![],
        }
    }
}

/// Builder for test roles
pub struct RoleBuilder {
    id: Uuid,
    name: String,
    display_name: String,
    description: Option<String>,
    permissions: Vec<Permission>,
}

impl RoleBuilder {
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = display_name.to_string();
        self
    }

    pub fn with_permission(mut self, resource: Resource, action: Action) -> Self {
        self.permissions.push(Permission {
            id: Uuid::new_v4(),
            resource,
            action,
            scope: Scope::All,
            constraint: None,
        });
        self
    }

    pub fn with_scoped_permission(
        mut self,
        resource: Resource,
        action: Action,
        scope: Scope,
    ) -> Self {
        self.permissions.push(Permission {
            id: Uuid::new_v4(),
            resource,
            action,
            scope,
            constraint: None,
        });
        self
    }

    pub fn build(self) -> Role {
        let now = Utc::now();
        Role {
            id: self.id,
            name: self.name,
            display_name: self.display_name,
            description: self.description,
            is_system: false,
            parent_id: None,
            permissions: self.permissions,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Generate random facts for a node
pub fn random_facts() -> serde_json::Value {
    let mut rng = rand::thread_rng();
    let os_families = ["RedHat", "Debian", "Windows"];
    let os_family = os_families[rng.gen_range(0..os_families.len())];

    let (os_name, major, minor) = match os_family {
        "RedHat" => {
            let names = ["Rocky", "AlmaLinux", "CentOS", "RHEL"];
            (names[rng.gen_range(0..names.len())], "9", "2")
        }
        "Debian" => {
            let names = ["Ubuntu", "Debian"];
            (names[rng.gen_range(0..names.len())], "22", "04")
        }
        "Windows" => ("Windows", "2022", ""),
        _ => ("Linux", "1", "0"),
    };

    let memory_gb = rng.gen_range(4..128);
    let cpu_count = rng.gen_range(1..64);

    serde_json::json!({
        "os": {
            "family": os_family,
            "name": os_name,
            "release": {
                "major": major,
                "minor": minor,
                "full": format!("{}.{}", major, minor)
            }
        },
        "kernel": if os_family == "Windows" { "windows" } else { "Linux" },
        "networking": {
            "ip": format!("192.168.{}.{}", rng.gen_range(1..255), rng.gen_range(1..255)),
            "hostname": format!("host{}", rng.gen_range(1..1000)),
            "fqdn": format!("host{}.example.com", rng.gen_range(1..1000))
        },
        "memory": {
            "system": {
                "total": format!("{}.00 GiB", memory_gb),
                "available": format!("{}.00 GiB", memory_gb / 2)
            }
        },
        "processors": {
            "count": cpu_count
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_factory_creates_unique_users() {
        let factory = UserFactory::new();
        let user1 = factory.create().build();
        let user2 = factory.create().build();

        assert_ne!(user1.id, user2.id);
        assert_ne!(user1.username, user2.username);
    }

    #[test]
    fn test_node_factory_creates_unique_nodes() {
        let factory = NodeFactory::new();
        let node1 = factory.create().build();
        let node2 = factory.create().build();

        assert_ne!(node1.certname, node2.certname);
    }

    #[test]
    fn test_role_builder_with_permissions() {
        let factory = RoleFactory::new();
        let role = factory
            .create()
            .with_permission(Resource::Nodes, Action::Read)
            .with_permission(Resource::Nodes, Action::Update)
            .build();

        assert_eq!(role.permissions.len(), 2);
    }

    #[test]
    fn test_random_facts() {
        let facts = random_facts();
        assert!(facts["os"]["family"].is_string());
        assert!(facts["memory"]["system"]["total"].is_string());
    }
}
// Helper functions for alert rule testing

use serde_json::Value;
use chrono::Duration;

/// Create an alert rule for testing
pub async fn create_alert_rule(app: &crate::common::test_app::TestApp, config: Value) -> AlertRule {
    let response = app
        .client
        .post(format!("{}/api/v1/alerting/rules", app.address))
        .json(&config)
        .send()
        .await
        .expect("Failed to create alert rule");
    
    assert_eq!(response.status(), 201);
    response.json().await.expect("Failed to parse alert rule")
}

/// Create a node for testing
pub async fn create_node(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    environment: &str,
    status: &str,
) -> Node {
    create_node_with_timestamp(app, certname, environment, status, Utc::now()).await
}

/// Create a node with specific timestamp
pub async fn create_node_with_timestamp(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    environment: &str,
    status: &str,
    report_timestamp: chrono::DateTime<Utc>,
) -> Node {
    let node = Node {
        certname: certname.to_string(),
        environment: environment.to_string(),
        status: status.to_string(),
        report_timestamp,
        catalog_timestamp: report_timestamp,
        facts_timestamp: report_timestamp,
        cached_catalog_status: None,
        latest_report_noop: false,
        latest_report_noop_pending: false,
        latest_report_hash: None,
    };
    
    // Insert node into test database
    app.insert_node(node.clone()).await;
    node
}

/// Create a node with specific facts
pub async fn create_node_with_facts(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    environment: &str,
    status: &str,
    facts: Value,
) -> Node {
    let node = create_node(app, certname, environment, status).await;
    app.insert_node_facts(certname, facts).await;
    node
}

/// Create consecutive failed reports for a node
pub async fn create_consecutive_failed_reports(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    count: usize,
    within_hours: i64,
) {
    let base_time = Utc::now() - Duration::hours(within_hours);
    let interval = Duration::hours(within_hours) / count as i32;
    
    for i in 0..count {
        let timestamp = base_time + (interval * i as i32);
        create_report_with_status(app, certname, "failed", timestamp).await;
    }
}

/// Create consecutive changed reports for a node
pub async fn create_consecutive_changed_reports(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    count: usize,
    within_hours: i64,
) {
    let base_time = Utc::now() - Duration::hours(within_hours);
    let interval = Duration::hours(within_hours) / count as i32;
    
    for i in 0..count {
        let timestamp = base_time + (interval * i as i32);
        create_report_with_changes(app, certname, 5, timestamp).await;
    }
}

/// Create mixed status reports for a node
pub async fn create_mixed_reports(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    statuses: Vec<&str>,
) {
    let base_time = Utc::now() - Duration::hours(12);
    let interval = Duration::hours(12) / statuses.len() as i32;
    
    for (i, status) in statuses.iter().enumerate() {
        let timestamp = base_time + (interval * i as i32);
        create_report_with_status(app, certname, status, timestamp).await;
    }
}

/// Create reports with specific class changes
pub async fn create_class_change_reports(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    class_name: &str,
    count: usize,
    within_hours: i64,
) {
    let base_time = Utc::now() - Duration::hours(within_hours);
    let interval = Duration::hours(within_hours) / count as i32;
    
    for i in 0..count {
        let timestamp = base_time + (interval * i as i32);
        create_report_with_class_change(app, certname, class_name, timestamp).await;
    }
}

/// Create a report with specific status
async fn create_report_with_status(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    status: &str,
    timestamp: chrono::DateTime<Utc>,
) {
    let report = Report {
        hash: Uuid::new_v4().to_string(),
        certname: certname.to_string(),
        environment: "production".to_string(),
        status: status.to_string(),
        timestamp,
        configuration_version: "1".to_string(),
        transaction_uuid: Uuid::new_v4().to_string(),
        report_format: 10,
        puppet_version: "7.0.0".to_string(),
        start_time: timestamp,
        end_time: timestamp + Duration::minutes(5),
        producer_timestamp: timestamp,
        noop: false,
        noop_pending: false,
        corrective_change: false,
        catalog_uuid: None,
        cached_catalog_status: None,
        code_id: None,
        job_id: None,
        metrics: serde_json::json!({
            "resources": {
                "total": 100,
                "changed": 0,
                "failed": if status == "failed" { 5 } else { 0 }
            }
        }),
        logs: vec![],
        resource_events: vec![],
    };
    
    app.insert_report(report).await;
}

/// Create a report with resource changes
async fn create_report_with_changes(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    change_count: i32,
    timestamp: chrono::DateTime<Utc>,
) {
    let report = Report {
        hash: Uuid::new_v4().to_string(),
        certname: certname.to_string(),
        environment: "production".to_string(),
        status: "changed".to_string(),
        timestamp,
        configuration_version: "1".to_string(),
        transaction_uuid: Uuid::new_v4().to_string(),
        report_format: 10,
        puppet_version: "7.0.0".to_string(),
        start_time: timestamp,
        end_time: timestamp + Duration::minutes(5),
        producer_timestamp: timestamp,
        noop: false,
        noop_pending: false,
        corrective_change: false,
        catalog_uuid: None,
        cached_catalog_status: None,
        code_id: None,
        job_id: None,
        metrics: serde_json::json!({
            "resources": {
                "total": 100,
                "changed": change_count,
                "failed": 0
            }
        }),
        logs: vec![],
        resource_events: vec![],
    };
    
    app.insert_report(report).await;
}

/// Create a report with specific class change
async fn create_report_with_class_change(
    app: &crate::common::test_app::TestApp,
    certname: &str,
    class_name: &str,
    timestamp: chrono::DateTime<Utc>,
) {
    let report = Report {
        hash: Uuid::new_v4().to_string(),
        certname: certname.to_string(),
        environment: "production".to_string(),
        status: "changed".to_string(),
        timestamp,
        configuration_version: "1".to_string(),
        transaction_uuid: Uuid::new_v4().to_string(),
        report_format: 10,
        puppet_version: "7.0.0".to_string(),
        start_time: timestamp,
        end_time: timestamp + Duration::minutes(5),
        producer_timestamp: timestamp,
        noop: false,
        noop_pending: false,
        corrective_change: false,
        catalog_uuid: None,
        cached_catalog_status: None,
        code_id: None,
        job_id: None,
        metrics: serde_json::json!({
            "resources": {
                "total": 100,
                "changed": 1,
                "failed": 0
            }
        }),
        logs: vec![],
        resource_events: vec![{
            "resource_type": format!("Class[{}]", class_name),
            "status": "changed",
            "timestamp": timestamp.to_rfc3339(),
        }],
    };
    
    app.insert_report(report).await;
}

// Placeholder models for alert rules
#[derive(Debug, serde::Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct AlertMatch {
    pub certname: String,
}