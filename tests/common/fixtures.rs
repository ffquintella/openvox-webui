//! Test fixtures for common test data
//!
//! Fixtures provide pre-defined test data that can be used across multiple tests.

use chrono::Utc;
use uuid::Uuid;

use openvox_webui::models::{
    default_organization_uuid, Action, Node, NodeGroup, Permission, Report, ReportStatus, Resource,
    Role, RuleMatchType, Scope, SystemRole,
};

/// Fixed UUIDs for testing (reproducible tests)
pub mod ids {
    use uuid::Uuid;

    pub const TEST_USER_ID: Uuid = Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc);
    pub const TEST_GROUP_ID: Uuid = Uuid::from_u128(0xabcdef12_abcd_abcd_abcd_abcdef123456);
    pub const TEST_NODE_ID: Uuid = Uuid::from_u128(0x98765432_9876_9876_9876_987654321cba);
    pub const TEST_ROLE_ID: Uuid = Uuid::from_u128(0xfedcba98_fedc_fedc_fedc_fedcba987654);
}

/// Test user fixtures
pub struct UserFixtures;

impl UserFixtures {
    /// Create an admin user fixture
    pub fn admin() -> TestUser {
        TestUser {
            id: ids::TEST_USER_ID,
            username: "admin".to_string(),
            email: "admin@example.com".to_string(),
            roles: vec!["admin".to_string()],
            role_ids: vec![SystemRole::Admin.uuid()],
        }
    }

    /// Create an operator user fixture
    pub fn operator() -> TestUser {
        TestUser {
            id: Uuid::new_v4(),
            username: "operator".to_string(),
            email: "operator@example.com".to_string(),
            roles: vec!["operator".to_string()],
            role_ids: vec![SystemRole::Operator.uuid()],
        }
    }

    /// Create a viewer user fixture
    pub fn viewer() -> TestUser {
        TestUser {
            id: Uuid::new_v4(),
            username: "viewer".to_string(),
            email: "viewer@example.com".to_string(),
            roles: vec!["viewer".to_string()],
            role_ids: vec![SystemRole::Viewer.uuid()],
        }
    }
}

/// Test user structure
#[derive(Debug, Clone)]
pub struct TestUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub roles: Vec<String>,
    pub role_ids: Vec<Uuid>,
}

/// Test node fixtures
pub struct NodeFixtures;

impl NodeFixtures {
    /// Create a basic node fixture
    pub fn web_server() -> Node {
        Node {
            certname: "web1.example.com".to_string(),
            deactivated: None,
            expired: None,
            catalog_timestamp: Some(Utc::now()),
            facts_timestamp: Some(Utc::now()),
            report_timestamp: Some(Utc::now()),
            catalog_environment: Some("production".to_string()),
            facts_environment: Some("production".to_string()),
            report_environment: Some("production".to_string()),
            latest_report_status: Some("changed".to_string()),
            latest_report_corrective_change: Some(false),
            cached_catalog_status: None,
        }
    }

    /// Create a database server node fixture
    pub fn db_server() -> Node {
        Node {
            certname: "db1.example.com".to_string(),
            deactivated: None,
            expired: None,
            catalog_timestamp: Some(Utc::now()),
            facts_timestamp: Some(Utc::now()),
            report_timestamp: Some(Utc::now()),
            catalog_environment: Some("production".to_string()),
            facts_environment: Some("production".to_string()),
            report_environment: Some("production".to_string()),
            latest_report_status: Some("unchanged".to_string()),
            latest_report_corrective_change: Some(false),
            cached_catalog_status: None,
        }
    }

    /// Create a failed node fixture
    pub fn failed_node() -> Node {
        Node {
            certname: "failed.example.com".to_string(),
            deactivated: None,
            expired: None,
            catalog_timestamp: Some(Utc::now()),
            facts_timestamp: Some(Utc::now()),
            report_timestamp: Some(Utc::now()),
            catalog_environment: Some("production".to_string()),
            facts_environment: Some("production".to_string()),
            report_environment: Some("production".to_string()),
            latest_report_status: Some("failed".to_string()),
            latest_report_corrective_change: Some(true),
            cached_catalog_status: None,
        }
    }
}

/// Test group fixtures
pub struct GroupFixtures;

impl GroupFixtures {
    /// Create a production web servers group
    pub fn web_servers() -> NodeGroup {
        NodeGroup {
            id: ids::TEST_GROUP_ID,
            organization_id: default_organization_uuid(),
            name: "web_servers".to_string(),
            description: Some("Production web servers".to_string()),
            parent_id: None,
            environment: Some("production".to_string()),
            rule_match_type: RuleMatchType::All,
            classes: serde_json::json!({"role::webserver": {}}),
            variables: serde_json::json!({}),
            rules: vec![],
            pinned_nodes: vec![],
        }
    }

    /// Create a database servers group
    pub fn db_servers() -> NodeGroup {
        NodeGroup {
            id: Uuid::new_v4(),
            organization_id: default_organization_uuid(),
            name: "db_servers".to_string(),
            description: Some("Database servers".to_string()),
            parent_id: None,
            environment: Some("production".to_string()),
            rule_match_type: RuleMatchType::All,
            classes: serde_json::json!({"role::database": {}}),
            variables: serde_json::json!({}),
            rules: vec![],
            pinned_nodes: vec![],
        }
    }
}

/// Test report fixtures
pub struct ReportFixtures;

impl ReportFixtures {
    /// Create a successful report
    pub fn success() -> Report {
        Report {
            hash: "report_success_123".to_string(),
            certname: "web1.example.com".to_string(),
            puppet_version: Some("8.0.0".to_string()),
            report_format: Some(12),
            configuration_version: Some("1234567890".to_string()),
            start_time: Some(Utc::now()),
            end_time: Some(Utc::now()),
            producer_timestamp: Some(Utc::now()),
            producer: Some("puppetserver.example.com".to_string()),
            transaction_uuid: Some(Uuid::new_v4().to_string()),
            status: Some(ReportStatus::Changed),
            corrective_change: Some(false),
            noop: Some(false),
            noop_pending: Some(false),
            environment: Some("production".to_string()),
            catalog_uuid: Some(Uuid::new_v4().to_string()),
            code_id: Some("abc123".to_string()),
            cached_catalog_status: None,
            report_type: Some("agent".to_string()),
            job_id: None,
            receive_time: Some(Utc::now()),
            metrics: None,
            resource_events: None,
            logs: None,
        }
    }

    /// Create a failed report
    pub fn failed() -> Report {
        let mut report = Self::success();
        report.hash = "report_failed_456".to_string();
        report.status = Some(ReportStatus::Failed);
        report
    }
}

/// Test role fixtures
pub struct RoleFixtures;

impl RoleFixtures {
    /// Create a custom role fixture
    pub fn custom_role() -> Role {
        Role {
            id: ids::TEST_ROLE_ID,
            name: "custom_role".to_string(),
            display_name: "Custom Role".to_string(),
            description: Some("A custom test role".to_string()),
            is_system: false,
            parent_id: None,
            permissions: vec![Permission {
                id: Uuid::new_v4(),
                resource: Resource::Nodes,
                action: Action::Read,
                scope: Scope::All,
                constraint: None,
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Test facts fixtures
pub struct FactsFixtures;

impl FactsFixtures {
    /// Create facts for a RedHat server
    pub fn redhat_server() -> serde_json::Value {
        serde_json::json!({
            "os": {
                "family": "RedHat",
                "name": "Rocky",
                "release": {
                    "major": "9",
                    "minor": "2",
                    "full": "9.2"
                }
            },
            "kernel": "Linux",
            "kernelversion": "5.14.0",
            "networking": {
                "ip": "192.168.1.10",
                "hostname": "web1",
                "fqdn": "web1.example.com"
            },
            "memory": {
                "system": {
                    "total": "16.00 GiB",
                    "available": "12.00 GiB"
                }
            },
            "processors": {
                "count": 4,
                "models": ["Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz"]
            }
        })
    }

    /// Create facts for a Debian server
    pub fn debian_server() -> serde_json::Value {
        serde_json::json!({
            "os": {
                "family": "Debian",
                "name": "Ubuntu",
                "release": {
                    "major": "22",
                    "minor": "04",
                    "full": "22.04"
                }
            },
            "kernel": "Linux",
            "kernelversion": "5.15.0",
            "networking": {
                "ip": "192.168.1.20",
                "hostname": "db1",
                "fqdn": "db1.example.com"
            },
            "memory": {
                "system": {
                    "total": "32.00 GiB",
                    "available": "28.00 GiB"
                }
            },
            "processors": {
                "count": 8,
                "models": ["AMD EPYC 7542 32-Core Processor"]
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_fixtures() {
        let admin = UserFixtures::admin();
        assert_eq!(admin.username, "admin");
        assert!(admin.roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_node_fixtures() {
        let node = NodeFixtures::web_server();
        assert_eq!(node.certname, "web1.example.com");
    }

    #[test]
    fn test_facts_fixtures() {
        let facts = FactsFixtures::redhat_server();
        assert_eq!(facts["os"]["family"], "RedHat");
    }
}
