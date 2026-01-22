//! Integration tests for alert rule conditions
//!
//! These tests verify the behavior of alert rule condition evaluation,
//! including NodeStatus, NodeFact, ReportMetric, LastReportTime,
//! ConsecutiveFailures, ConsecutiveChanges, and ClassChangeFrequency.

use crate::common::*;
use chrono::{Duration, Utc};
use serde_json::json;

#[tokio::test]
async fn test_node_status_condition_matches() {
    // Arrange: Create test app and a node with failed status
    let app = TestApp::spawn().await;
    let certname = "test-node-01.example.com";
    
    create_node(&app, certname, "production", "failed").await;
    
    // Create alert rule that matches failed status
    let rule_config = json!({
        "name": "Failed Nodes Alert",
        "description": "Alert when nodes have failed status",
        "enabled": true,
        "severity": "critical",
        "notification_channel_ids": [],
        "conditions": {
            "operator": "AND",
            "conditions": [{
                "type": "NodeStatus",
                "enabled": true,
                "config": {
                    "operator": "=",
                    "value": "failed"
                }
            }]
        }
    });
    
    let rule = create_alert_rule(&app, rule_config).await;
    
    // Act: Evaluate the rule
    let matches = app.evaluate_alert_rule(&rule.id).await;
    
    // Assert: Should match the node
    assert!(!matches.is_empty(), "Rule should match at least one node");
    assert_eq!(matches[0].certname, certname);
}

#[tokio::test]
async fn test_node_status_condition_does_not_match() {
    // Arrange: Create test app and a node with success status
    let app = TestApp::spawn().await;
    let certname = "test-node-02.example.com";
    
    create_node(&app, certname, "production", "unchanged").await;
    
    // Create alert rule that matches failed status
    let rule_config = json!({
        "name": "Failed Nodes Alert",
        "description": "Alert when nodes have failed status",
        "enabled": true,
        "severity": "critical",
        "notification_channel_ids": [],
        "conditions": {
            "operator": "AND",
            "conditions": [{
                "type": "NodeStatus",
                "enabled": true,
                "config": {
                    "operator": "=",
                    "value": "failed"
                }
            }]
        }
    });
    
    let rule = create_alert_rule(&app, rule_config).await;
    
    // Act: Evaluate the rule
    let matches = app.evaluate_alert_rule(&rule.id).await;
    
    // Assert: Should not match any nodes
    assert!(matches.is_empty(), "Rule should not match any nodes");
}

#[tokio::test]
async fn test_node_fact_condition_matches() {
    // Arrange: Create a node with specific fact value
    let app = TestApp::spawn().await;
    let certname = "centos-node.example.com";
    let facts = json!({
        "os": {
            "family": "RedHat",
            "name": "CentOS",
            "release": {
                "major": "7"
            }
        }
    });
    
    create_node_with_facts(&app, certname, "production", "unchanged", facts).await;
    
    // Create alert rule matching CentOS nodes
    let rule_config = json!({
        "name": "CentOS Nodes Alert",
        "description": "Alert for CentOS nodes",
        "enabled": true,
        "severity": "info",
        "notification_channel_ids": [],
        "conditions": {
            "operator": "AND",
            "conditions": [{
                "type": "NodeFact",
                "enabled": true,
                "config": {
                    "fact_path": "os.name",
                    "operator": "=",
                    "value": "CentOS"
                }
            }]
        }
    });
    
    let rule = create_alert_rule(&app, rule_config).await;
    
    // Act: Evaluate the rule
    let matches = app.evaluate_alert_rule(&rule.id).await;
    
    // Assert: Should match CentOS node
    assert!(!matches.is_empty(), "Rule should match CentOS nodes");
    assert_eq!(matches[0].certname, certname);
}

#[tokio::test]
async fn test_node_fact_condition_does_not_match() {
    // Arrange: Create a node with different fact value
    let app = TestApp::spawn().await;
    let certname = "ubuntu-node.example.com";
    let facts = json!({
        "os": {
            "family": "Debian",
            "name": "Ubuntu",
            "release": {
                "major": "20"
            }
        }
    });
    
    create_node_with_facts(&app, certname, "production", "unchanged", facts).await;
    
    // Create alert rule matching CentOS nodes
    let rule_config = json!({
        "name": "CentOS Nodes Alert",
        "description": "Alert for CentOS nodes",
        "enabled": true,
        "severity": "info",
        "notification_channel_ids": [],
        "conditions": {
            "operator": "AND",
            "conditions": [{
                "type": "NodeFact",
                "enabled": true,
                "config": {
                    "fact_path": "os.name",
                    "operator": "=",
                    "value": "CentOS"
                }
            }]
        }
    });
    
    let rule = create_alert_rule(&app, rule_config).await;
    
    // Act: Evaluate the rule
    let matches = app.evaluate_alert_rule(&rule.id).await;
    
    // Assert: Should not match Ubuntu node
    assert!(matches.is_empty(), "Rule should not match Ubuntu nodes");
}
