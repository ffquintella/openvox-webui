//! Advanced classification API tests (M26)

use crate::common::{generate_test_token_with_session, TestApp};
use uuid::Uuid;

fn json_request(
    method: &str,
    uri: &str,
    body: serde_json::Value,
) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap()
}

async fn admin_token(app: &TestApp) -> String {
    generate_test_token_with_session(
        app,
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        "admin",
        vec!["super_admin".into()],
    )
    .await
}

#[tokio::test]
async fn test_group_expression_priority_and_behavior_round_trip() {
    let app = TestApp::new().await;
    let token = admin_token(&app).await;
    let expression = serde_json::json!({"any": [
        {"fact": "os.family", "op": "=", "value": "RedHat"},
        {"fn": "in_cidr", "args": [{"fact": "networking.ip"}, "10.0.0.0/8"]}]});

    let created = app
        .request_with_auth(
            json_request(
                "POST",
                "/api/v1/groups",
                serde_json::json!({
                    "name": "m26-expr",
                    "priority": 250,
                    "expression": expression,
                    "behavior": {"inheritance": {"mode": "block"}, "merge": "union",
                                 "exclusive_set": "tier"},
                }),
            ),
            &token,
        )
        .await;
    created.assert_created();
    let group: serde_json::Value = created.json();
    assert_eq!(group["priority"], 250);
    assert_eq!(group["expression"], expression);
    assert_eq!(group["behavior"]["inheritance"]["mode"], "block");
    assert_eq!(group["behavior"]["merge"], "union");
    let id = group["id"].as_str().unwrap().to_string();

    // A plain update keeps the M26 fields.
    let updated = app
        .request_with_auth(
            json_request(
                "PUT",
                &format!("/api/v1/groups/{id}"),
                serde_json::json!({"description": "d"}),
            ),
            &token,
        )
        .await;
    updated.assert_ok();
    let group: serde_json::Value = updated.json();
    assert_eq!(group["expression"], expression);
    assert_eq!(group["priority"], 250);

    // An empty object clears the expression.
    let cleared = app
        .request_with_auth(
            json_request(
                "PUT",
                &format!("/api/v1/groups/{id}"),
                serde_json::json!({"expression": {}}),
            ),
            &token,
        )
        .await;
    cleared.assert_ok();
    let group: serde_json::Value = cleared.json();
    assert!(group.get("expression").is_none());
    assert_eq!(group["priority"], 250);
}

#[tokio::test]
async fn test_group_rejects_invalid_expression_behavior_and_priority() {
    let app = TestApp::new().await;
    let token = admin_token(&app).await;
    for (n, body) in [
        serde_json::json!({"name": "bad1", "expression": {"fn": "nope", "args": []}}),
        serde_json::json!({"name": "bad2", "expression": {"all": []}}),
        serde_json::json!({"name": "bad3", "priority": 1_000_000}),
        serde_json::json!({"name": "bad4", "behavior": {"match_mode": {"mode": "n_of", "k": 0}}}),
        serde_json::json!({"name": "bad5", "behavior": {"inheritance": {"mode": "select", "keys": []}}}),
    ]
    .into_iter()
    .enumerate()
    {
        let response = app
            .request_with_auth(json_request("POST", "/api/v1/groups", body), &token)
            .await;
        assert_eq!(response.status.as_u16(), 400, "case {n}");
    }
    // Unknown behavior keys are not silently dropped.
    let response = app
        .request_with_auth(
            json_request(
                "POST",
                "/api/v1/groups",
                serde_json::json!({"name": "bad6", "behavior": {"inheritence": {"mode": "block"}}}),
            ),
            &token,
        )
        .await;
    assert!(response.status.is_client_error());
}

#[tokio::test]
async fn test_classification_catalogs() {
    let app = TestApp::new().await;
    let token = admin_token(&app).await;
    for (path, key) in [
        ("functions", "name"),
        ("operators", "op"),
        ("behaviors", "key"),
        ("merge-strategies", "name"),
    ] {
        let request = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/api/v1/classification/{path}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.request_with_auth(request, &token).await;
        response.assert_ok();
        let items: Vec<serde_json::Value> = response.json();
        assert!(
            !items.is_empty() && items.iter().all(|i| i.get(key).is_some()),
            "{path}"
        );
    }
}

#[tokio::test]
async fn test_group_preview_needs_puppetdb_and_validates() {
    let app = TestApp::new().await;
    let token = admin_token(&app).await;
    let created = app
        .request_with_auth(
            json_request(
                "POST",
                "/api/v1/groups",
                serde_json::json!({"name": "m26-preview"}),
            ),
            &token,
        )
        .await;
    created.assert_created();
    let group: serde_json::Value = created.json();
    let uri = format!("/api/v1/groups/{}/preview", group["id"].as_str().unwrap());

    let bad = app
        .request_with_auth(
            json_request("POST", &uri, serde_json::json!({"expression": {"nope": 1}})),
            &token,
        )
        .await;
    bad.assert_bad_request();

    let unavailable = app
        .request_with_auth(json_request("POST", &uri, serde_json::json!({})), &token)
        .await;
    assert_eq!(
        unavailable.status.as_u16(),
        503,
        "no PuppetDB in the test app"
    );

    let missing = app
        .request_with_auth(
            json_request(
                "POST",
                &format!("/api/v1/groups/{}/preview", Uuid::new_v4()),
                serde_json::json!({}),
            ),
            &token,
        )
        .await;
    assert!(missing.status.as_u16() == 404 || missing.status.as_u16() == 503);
}
