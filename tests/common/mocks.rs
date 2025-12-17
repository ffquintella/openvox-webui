//! Mock services for testing
//!
//! Provides mock implementations of external services like PuppetDB
//! for isolated testing without external dependencies.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use openvox_webui::models::{Node, Report};

/// Mock PuppetDB service for testing
pub struct MockPuppetDb {
    nodes: Arc<RwLock<HashMap<String, Node>>>,
    reports: Arc<RwLock<HashMap<String, Report>>>,
    facts: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    /// Simulate errors when set
    pub error_mode: Arc<RwLock<Option<MockError>>>,
}

/// Types of errors the mock can simulate
#[derive(Debug, Clone)]
pub enum MockError {
    /// Connection refused
    ConnectionRefused,
    /// Timeout
    Timeout,
    /// Internal server error
    InternalError(String),
    /// Not found
    NotFound,
}

impl Default for MockPuppetDb {
    fn default() -> Self {
        Self::new()
    }
}

impl MockPuppetDb {
    /// Create a new mock PuppetDB service
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            reports: Arc::new(RwLock::new(HashMap::new())),
            facts: Arc::new(RwLock::new(HashMap::new())),
            error_mode: Arc::new(RwLock::new(None)),
        }
    }

    /// Set error mode to simulate failures
    pub fn set_error_mode(&self, error: MockError) {
        *self.error_mode.write().unwrap() = Some(error);
    }

    /// Clear error mode
    pub fn clear_error_mode(&self) {
        *self.error_mode.write().unwrap() = None;
    }

    /// Check if an error should be returned
    fn check_error(&self) -> Result<(), MockError> {
        if let Some(ref error) = *self.error_mode.read().unwrap() {
            return Err(error.clone());
        }
        Ok(())
    }

    /// Add a node to the mock
    pub fn add_node(&self, node: Node) {
        self.nodes
            .write()
            .unwrap()
            .insert(node.certname.clone(), node);
    }

    /// Add multiple nodes to the mock
    pub fn add_nodes(&self, nodes: Vec<Node>) {
        let mut store = self.nodes.write().unwrap();
        for node in nodes {
            store.insert(node.certname.clone(), node);
        }
    }

    /// Get a node by certname
    pub fn get_node(&self, certname: &str) -> Result<Option<Node>, MockError> {
        self.check_error()?;
        Ok(self.nodes.read().unwrap().get(certname).cloned())
    }

    /// Get all nodes
    pub fn get_nodes(&self) -> Result<Vec<Node>, MockError> {
        self.check_error()?;
        Ok(self.nodes.read().unwrap().values().cloned().collect())
    }

    /// Query nodes with a filter
    pub fn query_nodes<F>(&self, filter: F) -> Result<Vec<Node>, MockError>
    where
        F: Fn(&Node) -> bool,
    {
        self.check_error()?;
        Ok(self
            .nodes
            .read()
            .unwrap()
            .values()
            .filter(|n| filter(n))
            .cloned()
            .collect())
    }

    /// Add a report to the mock
    pub fn add_report(&self, report: Report) {
        self.reports
            .write()
            .unwrap()
            .insert(report.hash.clone(), report);
    }

    /// Get a report by hash
    pub fn get_report(&self, hash: &str) -> Result<Option<Report>, MockError> {
        self.check_error()?;
        Ok(self.reports.read().unwrap().get(hash).cloned())
    }

    /// Get reports for a node
    pub fn get_reports_for_node(&self, certname: &str) -> Result<Vec<Report>, MockError> {
        self.check_error()?;
        Ok(self
            .reports
            .read()
            .unwrap()
            .values()
            .filter(|r| r.certname == certname)
            .cloned()
            .collect())
    }

    /// Add facts for a node
    pub fn add_facts(&self, certname: &str, facts: serde_json::Value) {
        self.facts
            .write()
            .unwrap()
            .insert(certname.to_string(), facts);
    }

    /// Get facts for a node
    pub fn get_facts(&self, certname: &str) -> Result<Option<serde_json::Value>, MockError> {
        self.check_error()?;
        Ok(self.facts.read().unwrap().get(certname).cloned())
    }

    /// Clear all data
    pub fn clear(&self) {
        self.nodes.write().unwrap().clear();
        self.reports.write().unwrap().clear();
        self.facts.write().unwrap().clear();
        self.clear_error_mode();
    }

    /// Seed with test data
    pub fn seed_test_data(&self) {
        use crate::common::fixtures::{FactsFixtures, NodeFixtures, ReportFixtures};

        // Add nodes
        self.add_node(NodeFixtures::web_server());
        self.add_node(NodeFixtures::db_server());
        self.add_node(NodeFixtures::failed_node());

        // Add reports
        self.add_report(ReportFixtures::success());
        self.add_report(ReportFixtures::failed());

        // Add facts
        self.add_facts("web1.example.com", FactsFixtures::redhat_server());
        self.add_facts("db1.example.com", FactsFixtures::debian_server());
    }
}

/// Mock RBAC service for testing permission scenarios
pub struct MockRbacService {
    /// Override all permission checks to return this result
    pub override_permission: Arc<RwLock<Option<bool>>>,
    /// Specific permission overrides (resource:action -> allowed)
    pub permission_overrides: Arc<RwLock<HashMap<String, bool>>>,
}

impl Default for MockRbacService {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRbacService {
    pub fn new() -> Self {
        Self {
            override_permission: Arc::new(RwLock::new(None)),
            permission_overrides: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Allow all permissions
    pub fn allow_all(&self) {
        *self.override_permission.write().unwrap() = Some(true);
    }

    /// Deny all permissions
    pub fn deny_all(&self) {
        *self.override_permission.write().unwrap() = Some(false);
    }

    /// Reset to normal behavior
    pub fn reset(&self) {
        *self.override_permission.write().unwrap() = None;
        self.permission_overrides.write().unwrap().clear();
    }

    /// Set a specific permission
    pub fn set_permission(&self, resource: &str, action: &str, allowed: bool) {
        let key = format!("{}:{}", resource, action);
        self.permission_overrides
            .write()
            .unwrap()
            .insert(key, allowed);
    }

    /// Check if a permission is allowed
    pub fn check(&self, resource: &str, action: &str) -> bool {
        // Check global override first
        if let Some(override_result) = *self.override_permission.read().unwrap() {
            return override_result;
        }

        // Check specific override
        let key = format!("{}:{}", resource, action);
        if let Some(&allowed) = self.permission_overrides.read().unwrap().get(&key) {
            return allowed;
        }

        // Default to allowed
        true
    }
}

/// Builder for creating test scenarios with pre-configured mocks
pub struct MockScenario {
    pub puppetdb: MockPuppetDb,
    pub rbac: MockRbacService,
}

impl Default for MockScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl MockScenario {
    pub fn new() -> Self {
        Self {
            puppetdb: MockPuppetDb::new(),
            rbac: MockRbacService::new(),
        }
    }

    /// Create a scenario with seeded test data
    pub fn with_test_data() -> Self {
        let scenario = Self::new();
        scenario.puppetdb.seed_test_data();
        scenario
    }

    /// Create a scenario simulating PuppetDB failure
    pub fn with_puppetdb_failure(error: MockError) -> Self {
        let scenario = Self::new();
        scenario.puppetdb.set_error_mode(error);
        scenario
    }

    /// Create a scenario with restricted permissions
    pub fn with_restricted_permissions() -> Self {
        let scenario = Self::new();
        scenario.rbac.deny_all();
        scenario
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::NodeFixtures;

    #[test]
    fn test_mock_puppetdb_basic() {
        let mock = MockPuppetDb::new();
        mock.add_node(NodeFixtures::web_server());

        let node = mock.get_node("web1.example.com").unwrap();
        assert!(node.is_some());
        assert_eq!(node.unwrap().certname, "web1.example.com");
    }

    #[test]
    fn test_mock_puppetdb_error_mode() {
        let mock = MockPuppetDb::new();
        mock.set_error_mode(MockError::ConnectionRefused);

        let result = mock.get_nodes();
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_rbac_allow_all() {
        let mock = MockRbacService::new();
        mock.allow_all();

        assert!(mock.check("nodes", "delete"));
        assert!(mock.check("reports", "update"));
    }

    #[test]
    fn test_mock_rbac_deny_all() {
        let mock = MockRbacService::new();
        mock.deny_all();

        assert!(!mock.check("nodes", "read"));
        assert!(!mock.check("reports", "read"));
    }

    #[test]
    fn test_mock_rbac_specific_permission() {
        let mock = MockRbacService::new();
        mock.set_permission("nodes", "read", true);
        mock.set_permission("nodes", "delete", false);

        assert!(mock.check("nodes", "read"));
        assert!(!mock.check("nodes", "delete"));
    }

    #[test]
    fn test_mock_scenario_with_test_data() {
        let scenario = MockScenario::with_test_data();
        let nodes = scenario.puppetdb.get_nodes().unwrap();
        assert!(!nodes.is_empty());
    }
}
