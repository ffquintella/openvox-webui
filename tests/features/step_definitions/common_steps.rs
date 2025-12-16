//! Common step definitions used across features

use cucumber::{given, then};
use crate::features::support::TestWorld;

#[given("I am authenticated as an admin")]
async fn authenticated_as_admin(world: &mut TestWorld) {
    world.authenticate_admin().await;
}

#[given("I am authenticated as a user")]
async fn authenticated_as_user(world: &mut TestWorld) {
    world.authenticate_user().await;
}

#[given("I am not authenticated")]
async fn not_authenticated(world: &mut TestWorld) {
    world.auth_token = None;
    world.current_user = None;
}

#[then(expr = "the response status should be {int}")]
async fn response_status(world: &mut TestWorld, status: u16) {
    if let Some(response) = &world.last_response {
        assert_eq!(response.status, status);
    } else {
        panic!("No response available");
    }
}

#[then("the response should contain an error")]
async fn response_contains_error(world: &mut TestWorld) {
    if let Some(response) = &world.last_response {
        assert!(response.body.get("error").is_some());
    } else {
        panic!("No response available");
    }
}
