//! Integration tests for alert rule payload handling.

use crate::common::*;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_create_node_status_alert_rule() {
    let app = TestApp::new().await;

    let rule = create_alert_rule(
        &app,
        json!({
            "name": "Failed Nodes Alert",
            "description": "Alert when nodes have failed status",
            "rule_type": "node_status",
            "severity": "critical",
            "condition_operator": "all",
            "channel_ids": [],
            "conditions": [{
                "operator": "=",
                "value": "failed",
                "field": "status",
                "enabled": true
            }]
        }),
    )
    .await;

    assert_eq!(rule.name, "Failed Nodes Alert");
}

#[tokio::test]
async fn test_list_alert_rules_includes_created_rule() {
    let app = TestApp::new().await;

    let created = create_alert_rule(
        &app,
        json!({
            "name": "Compliance Alert",
            "description": "Alert on compliance drift",
            "rule_type": "compliance",
            "severity": "warning",
            "condition_operator": "all",
            "channel_ids": [],
            "conditions": [{
                "operator": ">",
                "value": 10,
                "field": "compliance.failed",
                "enabled": true
            }]
        }),
    )
    .await;

    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );
    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/v1/alerting/rules")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;
    response.assert_ok();

    let json: serde_json::Value = response.json();
    let rules = json["data"].as_array().expect("rules array");
    assert!(rules
        .iter()
        .any(|rule| rule["id"] == created.id.to_string()));
}

#[tokio::test]
async fn test_get_alert_rule_returns_created_rule() {
    let app = TestApp::new().await;

    let created = create_alert_rule(
        &app,
        json!({
            "name": "Custom Alert",
            "description": "Custom alert rule",
            "rule_type": "custom",
            "severity": "info",
            "condition_operator": "any",
            "channel_ids": [],
            "conditions": [{
                "operator": "contains",
                "value": "error",
                "field": "message",
                "enabled": true
            }]
        }),
    )
    .await;

    let token = generate_test_token(
        &app.state.config,
        Uuid::new_v4(),
        "admin",
        vec!["admin".to_string()],
    );
    let request = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/api/v1/alerting/rules/{}", created.id))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.request_with_auth(request, &token).await;
    response.assert_ok();

    let json: serde_json::Value = response.json();
    assert_eq!(json["data"]["name"], "Custom Alert");
    assert_eq!(json["data"]["rule_type"], "custom");
}

#[tokio::test]
async fn test_create_alert_rule_with_multiple_conditions() {
    let app = TestApp::new().await;

    let rule = create_alert_rule(
        &app,
        json!({
            "name": "Multi Condition Alert",
            "description": "Multiple conditions",
            "rule_type": "report_failure",
            "severity": "critical",
            "condition_operator": "all",
            "channel_ids": [],
            "conditions": [
                {
                    "operator": "=",
                    "value": "failed",
                    "field": "status",
                    "enabled": true
                },
                {
                    "operator": ">",
                    "value": 3,
                    "field": "metrics.resources.failed",
                    "enabled": true
                }
            ]
        }),
    )
    .await;

    assert_eq!(rule.name, "Multi Condition Alert");
}
