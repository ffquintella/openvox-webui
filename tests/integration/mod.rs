//! Integration tests for OpenVox WebUI
//!
//! These tests verify the behavior of the API endpoints with a real
//! (in-memory) database and all middleware.

mod alert_conditions_tests;
mod auth_pipeline_tests;
mod classification_tests;
mod directory_tests;
mod mfa_fido2_e2e_tests;
mod plugin_tests;
mod scim_tests;
