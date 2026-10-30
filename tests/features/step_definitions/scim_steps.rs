//! Step definitions for inbound SCIM (M31). They run the real filter, PATCH
//! and mapping logic of `services::scim`.

use crate::features::support::TestWorld;
use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use openvox_webui::services::directory::mapping::RoleMapping;
use openvox_webui::services::scim::{
    filter, mapping,
    patch::{apply_user_patch, parse_patch},
    resources::{GroupRef, UserAttrs},
};
use serde_json::Value;

fn docstring(step: &Step) -> Value {
    let text = step.docstring.as_deref().expect("step needs a docstring");
    serde_json::from_str(text).expect("docstring is JSON")
}

#[given("a SCIM user resource:")]
async fn given_user(world: &mut TestWorld, step: &Step) {
    let v = docstring(step);
    world.scim_user = Some(UserAttrs {
        user_name: v["userName"].as_str().unwrap().to_string(),
        external_id: v["externalId"].as_str().map(str::to_string),
        email: v["emails"][0]["value"].as_str().map(str::to_string),
        active: v["active"].as_bool().unwrap_or(true),
        ..Default::default()
    });
    world.scim_resource = Some(v);
}

#[when(expr = "I evaluate the SCIM filter {string}")]
async fn when_filter(world: &mut TestWorld, f: String) {
    let resource = world.scim_resource.clone().expect("a resource");
    match filter::parse_filter(&f) {
        Ok(parsed) => {
            world.scim_matched = Some(filter::matches(&parsed, &resource));
            world.scim_error = None;
        }
        Err(e) => world.scim_error = e.scim_type.map(str::to_string),
    }
}

#[then("the SCIM filter should match")]
async fn then_match(world: &mut TestWorld) {
    assert_eq!(world.scim_matched, Some(true));
}

#[then("the SCIM filter should not match")]
async fn then_no_match(world: &mut TestWorld) {
    assert_eq!(world.scim_matched, Some(false));
}

#[when("I apply the SCIM patch:")]
async fn when_patch(world: &mut TestWorld, step: &Step) {
    let mut attrs = world.scim_user.clone().expect("a user");
    let result = parse_patch(&docstring(step)).and_then(|ops| apply_user_patch(&mut attrs, &ops));
    match result {
        Ok(()) => {
            world.scim_user = Some(attrs);
            world.scim_error = None;
        }
        Err(e) => world.scim_error = e.scim_type.map(str::to_string),
    }
}

#[then("the SCIM user should be inactive")]
async fn then_inactive(world: &mut TestWorld) {
    assert!(world.scim_error.is_none(), "{:?}", world.scim_error);
    assert!(!world.scim_user.as_ref().unwrap().active);
}

#[then(expr = "the SCIM request should be refused with {string}")]
async fn then_refused(world: &mut TestWorld, scim_type: String) {
    assert_eq!(world.scim_error.as_deref(), Some(scim_type.as_str()));
}

#[given("the SCIM groups of the user:")]
async fn given_groups(world: &mut TestWorld, step: &Step) {
    world.scim_groups = docstring(step)
        .as_array()
        .unwrap()
        .iter()
        .map(|g| GroupRef {
            id: g["id"].as_str().unwrap().to_string(),
            display_name: g["display_name"].as_str().unwrap().to_string(),
            external_id: g["external_id"].as_str().map(str::to_string),
        })
        .collect();
}

#[given("the SCIM role mappings:")]
async fn given_mappings(world: &mut TestWorld, step: &Step) {
    world.scim_mappings = docstring(step)
        .as_array()
        .unwrap()
        .iter()
        .map(|m| RoleMapping {
            id: m["ref"].as_str().unwrap().to_string(),
            directory_id: "scim".into(),
            group_ref: m["ref"].as_str().unwrap().to_string(),
            group_ref_kind: m["kind"].as_str().unwrap().to_string(),
            group_label: None,
            role_id: m["role_id"].as_str().unwrap().to_string(),
            role_name: m["role"].as_str().unwrap().to_string(),
            created_by: None,
            created_at: String::new(),
        })
        .collect();
}

#[then(expr = "the SCIM roles granted should be {string}")]
async fn then_roles(world: &mut TestWorld, roles: String) {
    let got: Vec<String> = mapping::desired_roles(&world.scim_mappings, &world.scim_groups)
        .into_iter()
        .collect();
    let want: Vec<String> = roles.split(',').map(|r| r.trim().to_string()).collect();
    assert_eq!(got, want);
}
