//! Test world for Cucumber scenarios

use cucumber::World;
use std::collections::HashMap;

/// Test world that maintains state across scenario steps
#[derive(Debug, Default, World)]
pub struct TestWorld {
    /// Authentication token for API calls
    pub auth_token: Option<String>,

    /// Current user context
    pub current_user: Option<TestUser>,

    /// Created resources for cleanup
    pub created_groups: Vec<String>,
    pub created_nodes: Vec<String>,

    /// Response from last API call
    pub last_response: Option<TestResponse>,

    /// Facts for test nodes
    pub node_facts: HashMap<String, serde_json::Value>,

    /// User roles mapping (username -> list of roles)
    pub user_roles: HashMap<String, Vec<String>>,

    /// User scoped permissions (username -> list of scoped group names)
    pub user_scoped_groups: HashMap<String, Vec<String>>,

    /// User environment-scoped permissions (username -> list of environments)
    pub user_scoped_environments: HashMap<String, Vec<String>>,

    /// Custom roles with parent relationships (role_name -> parent_role_name)
    pub role_parents: HashMap<String, String>,

    /// Classification rules by group name (simple equals rules for tests)
    pub group_rules: HashMap<String, Vec<(String, String)>>,

    /// Group parent relationships (child_group -> parent_group)
    pub group_parents: HashMap<String, String>,

    /// Group classes (group_name -> list of classes)
    pub group_classes: HashMap<String, Vec<String>>,

    /// Base URL for API calls
    #[allow(dead_code)]
    pub api_base_url: String,
}

#[derive(Debug, Clone)]
pub struct TestUser {
    #[allow(dead_code)]
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct TestResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

impl TestWorld {
    /// Create a new test world
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            api_base_url: "http://localhost:8080".to_string(),
            ..Default::default()
        }
    }

    /// Authenticate as admin
    pub async fn authenticate_admin(&mut self) {
        self.current_user = Some(TestUser {
            username: "admin".to_string(),
            role: "admin".to_string(),
        });
        self.auth_token = Some("test-admin-token".to_string());
    }

    /// Authenticate as regular user
    pub async fn authenticate_user(&mut self) {
        self.current_user = Some(TestUser {
            username: "user".to_string(),
            role: "user".to_string(),
        });
        self.auth_token = Some("test-user-token".to_string());
    }

    /// Create a test node group
    pub async fn create_group(&mut self, _name: &str) -> Result<String, String> {
        // In real implementation, this would make an API call
        let group_id = uuid::Uuid::new_v4().to_string();
        self.created_groups.push(group_id.clone());
        Ok(group_id)
    }

    /// Check if a group exists
    pub async fn group_exists(&self, _name: &str) -> bool {
        // In real implementation, this would query the API
        !self.created_groups.is_empty()
    }

    /// Add a node with facts
    pub fn add_node_with_facts(&mut self, certname: &str, facts: serde_json::Value) {
        self.node_facts.insert(certname.to_string(), facts);
        self.created_nodes.push(certname.to_string());
    }

    /// Cleanup created resources
    #[allow(dead_code)]
    pub async fn cleanup(&mut self) {
        // In real implementation, this would delete created resources
        self.created_groups.clear();
        self.created_nodes.clear();
        self.node_facts.clear();
    }
}
