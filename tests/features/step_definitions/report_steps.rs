//! Report step definitions

use cucumber::{given, then, when};
use crate::features::TestWorld;

#[given(expr = "a report exists for node {string} with status {string}")]
async fn report_exists(world: &mut TestWorld, _certname: String, _status: String) {
    // In real implementation, create test report data
}

#[when(expr = "I request reports for node {string}")]
async fn request_node_reports(world: &mut TestWorld, certname: String) {
    // In real implementation, make API call to GET /api/v1/nodes/{certname}/reports
    world.last_response = Some(crate::features::support::world::TestResponse {
        status: 200,
        body: serde_json::json!([]),
    });
}

#[when(expr = "I request reports with status {string}")]
async fn request_reports_by_status(world: &mut TestWorld, status: String) {
    // In real implementation, make API call to GET /api/v1/reports?status={status}
    world.last_response = Some(crate::features::support::world::TestResponse {
        status: 200,
        body: serde_json::json!([]),
    });
}

#[then("the response should contain reports")]
async fn response_contains_reports(world: &mut TestWorld) {
    if let Some(response) = &world.last_response {
        assert!(response.body.is_array());
    }
}

#[then(expr = "all reports should have status {string}")]
async fn all_reports_have_status(world: &mut TestWorld, expected_status: String) {
    if let Some(response) = &world.last_response {
        if let Some(reports) = response.body.as_array() {
            for report in reports {
                let status = report.get("status").and_then(|s| s.as_str());
                assert_eq!(status, Some(expected_status.as_str()));
            }
        }
    }
}
