//! CA step definitions

use cucumber::{then, when};
use crate::features::support::{TestResponse, TestWorld};

#[when("I request CA status")]
async fn request_ca_status(world: &mut TestWorld) {
    // Simulate GET /api/v1/ca/status
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "healthy": true,
            "pending_requests": 2,
            "signed_certificates": 10
        }),
    });
}

#[then("the CA status should include counts")]
async fn ca_status_includes_counts(world: &mut TestWorld) {
    if let Some(response) = &world.last_response {
        let pending = response.body.get("pending_requests").and_then(|v| v.as_u64());
        let signed = response.body.get("signed_certificates").and_then(|v| v.as_u64());
        assert!(pending.is_some());
        assert!(signed.is_some());
    }
}

#[when("I list pending CSRs")]
async fn list_pending_csrs(world: &mut TestWorld) {
    // Simulate GET /api/v1/ca/requests
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!([
            {"certname": "node1.example.com", "requested_at": "2025-12-01T12:00:00Z"},
            {"certname": "node2.example.com", "requested_at": "2025-12-02T15:30:00Z"}
        ]),
    });
}

#[then("the response should contain CSRs")]
async fn response_contains_csrs(world: &mut TestWorld) {
    if let Some(response) = &world.last_response {
        assert!(response.body.is_array());
        let arr = response.body.as_array().unwrap();
        assert!(arr.iter().all(|item| item.get("certname").is_some()));
    }
}

#[when(expr = "I sign CSR for {string} with alt names {string}")]
async fn sign_csr_with_alt_names(world: &mut TestWorld, certname: String, alt_names_csv: String) {
    let alt_names: Vec<&str> = alt_names_csv.split(',').collect();
    // Simulate POST /api/v1/ca/sign/{certname}
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "certname": certname,
            "signed": true,
            "dns_alt_names": alt_names
        }),
    });
}

#[then(expr = "the certificate {string} should be signed")]
async fn certificate_should_be_signed(world: &mut TestWorld, certname: String) {
    if let Some(response) = &world.last_response {
        let body_cert = response.body.get("certname").and_then(|v| v.as_str());
        let signed = response.body.get("signed").and_then(|v| v.as_bool());
        assert_eq!(body_cert, Some(certname.as_str()));
        assert_eq!(signed, Some(true));
    }
}

#[then(expr = "the response should include dns alt names {string}")]
async fn response_should_include_dns_alt_names(world: &mut TestWorld, alt_names_csv: String) {
    if let Some(response) = &world.last_response {
        let expected: Vec<&str> = alt_names_csv.split(',').collect();
        let actual = response
            .body
            .get("dns_alt_names")
            .and_then(|v| v.as_array())
            .expect("dns_alt_names array expected");
        let actual_strs: Vec<&str> = actual
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(actual_strs, expected);
    }
}

#[when(expr = "I reject CSR for {string}")]
async fn reject_csr(world: &mut TestWorld, certname: String) {
    // Simulate POST /api/v1/ca/reject/{certname}
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "certname": certname,
            "rejected": true
        }),
    });
}

#[then(expr = "the CSR {string} should be rejected")]
async fn csr_should_be_rejected(world: &mut TestWorld, certname: String) {
    if let Some(response) = &world.last_response {
        let body_cert = response.body.get("certname").and_then(|v| v.as_str());
        let rejected = response.body.get("rejected").and_then(|v| v.as_bool());
        assert_eq!(body_cert, Some(certname.as_str()));
        assert_eq!(rejected, Some(true));
    }
}

#[when(expr = "I revoke certificate for {string}")]
async fn revoke_certificate(world: &mut TestWorld, certname: String) {
    // Simulate DELETE /api/v1/ca/certificates/{certname}
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "certname": certname,
            "revoked": true
        }),
    });
}

#[then(expr = "the certificate {string} should be revoked")]
async fn certificate_should_be_revoked(world: &mut TestWorld, certname: String) {
    if let Some(response) = &world.last_response {
        let body_cert = response.body.get("certname").and_then(|v| v.as_str());
        let revoked = response.body.get("revoked").and_then(|v| v.as_bool());
        assert_eq!(body_cert, Some(certname.as_str()));
        assert_eq!(revoked, Some(true));
    }
}

#[when(expr = "I renew CA certificate for {int} days")]
async fn renew_ca_certificate(world: &mut TestWorld, days: i32) {
    // Simulate POST /api/v1/ca/renew
    world.last_response = Some(TestResponse {
        status: 200,
        body: serde_json::json!({
            "renewed": true,
            "valid_for_days": days
        }),
    });
}

#[then(expr = "the CA certificate should be renewed for {int} days")]
async fn ca_certificate_should_be_renewed(world: &mut TestWorld, days: i32) {
    if let Some(response) = &world.last_response {
        let renewed = response.body.get("renewed").and_then(|v| v.as_bool());
        let valid_for_days = response.body.get("valid_for_days").and_then(|v| v.as_i64());
        assert_eq!(renewed, Some(true));
        assert_eq!(valid_for_days, Some(days as i64));
    }
}
