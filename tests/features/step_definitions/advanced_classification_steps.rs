//! Step definitions for expressions, group policies and explain (M26).
//! Unlike the simulated classification steps, these run the real engine.

use crate::features::support::{TestResponse, TestWorld};
use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use openvox_webui::models::{
    ClassificationRule, GroupBehavior, GroupOutcome, NodeGroup, RuleOperator,
};
use openvox_webui::services::classification::ClassificationService;
use openvox_webui::services::classification_expr;
use serde_json::{json, Value};
use uuid::Uuid;

fn docstring(step: &Step) -> Value {
    let text = step.docstring.as_deref().expect("step needs a docstring");
    serde_json::from_str(text).expect("docstring is JSON")
}

/// Build groups from a loose JSON description: `parent` names another group.
fn build_groups(spec: &Value) -> Vec<NodeGroup> {
    let items = spec.as_array().expect("a list of groups");
    let mut groups: Vec<NodeGroup> = items
        .iter()
        .map(|g| {
            let mut group = NodeGroup {
                name: g["name"].as_str().expect("name").to_string(),
                ..Default::default()
            };
            if let Some(v) = g.get("expression") {
                group.expression = Some(v.clone());
            }
            if let Some(v) = g.get("priority") {
                group.priority = v.as_i64().unwrap() as i32;
            }
            if let Some(v) = g.get("behavior") {
                group.behavior = serde_json::from_value::<GroupBehavior>(v.clone()).unwrap();
            }
            if let Some(v) = g.get("classes") {
                group.classes = v.clone();
            }
            if let Some(v) = g.get("variables") {
                group.variables = v.clone();
            }
            group.match_all_nodes = g["match_all_nodes"].as_bool().unwrap_or(false);
            group.environment = g["environment"].as_str().map(str::to_string);
            group.is_environment_group = g["is_environment_group"].as_bool().unwrap_or(false);
            group.pinned_nodes = g["pinned_nodes"]
                .as_array()
                .map(|a| a.iter().map(|n| n.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            group.rules = g["rules"]
                .as_array()
                .map(|rules| {
                    rules
                        .iter()
                        .map(|r| ClassificationRule {
                            id: Uuid::new_v4(),
                            fact_path: r[0].as_str().unwrap().to_string(),
                            operator: RuleOperator::Equals,
                            value: r[1].clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            group
        })
        .collect();
    let ids: Vec<(String, Uuid)> = groups.iter().map(|g| (g.name.clone(), g.id)).collect();
    for (group, spec) in groups.iter_mut().zip(items) {
        if let Some(parent) = spec["parent"].as_str() {
            group.parent_id = ids.iter().find(|(n, _)| n == parent).map(|(_, id)| *id);
        }
    }
    groups
}

#[given("the advanced classification groups:")]
async fn given_groups(world: &mut TestWorld, step: &Step) {
    world.engine_groups = build_groups(&docstring(step));
}

#[given(expr = "a node {string} with facts:")]
async fn given_node_facts(world: &mut TestWorld, certname: String, step: &Step) {
    let mut facts = docstring(step);
    facts["certname"] = json!(certname);
    world.node_facts.insert(certname, facts);
}

#[when(expr = "I classify {string} with the engine")]
async fn classify(world: &mut TestWorld, certname: String) {
    let facts = world
        .node_facts
        .get(&certname)
        .cloned()
        .expect("node facts");
    let service = ClassificationService::new(world.engine_groups.clone());
    let (result, explain) = service.classify_explained(&certname, &facts);
    let plain = service.classify(&certname, &facts);
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        serde_json::to_value(&result).unwrap(),
        "explaining must not change the result"
    );
    world.engine_result = Some(result);
    world.engine_explain = Some(explain);
}

#[then(expr = "the node should be in the groups {string}")]
async fn then_groups(world: &mut TestWorld, expected: String) {
    let result = world.engine_result.as_ref().expect("classified");
    let mut got: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
    got.sort();
    let mut want: Vec<&str> = expected
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    want.sort();
    assert_eq!(got, want);
}

#[then("the node should be in no group")]
async fn then_no_group(world: &mut TestWorld) {
    assert!(world.engine_result.as_ref().unwrap().groups.is_empty());
}

#[then(expr = "the class {string} should be present")]
async fn then_class_present(world: &mut TestWorld, class: String) {
    assert!(world
        .engine_result
        .as_ref()
        .unwrap()
        .classes
        .get(&class)
        .is_some());
}

#[then(expr = "the class {string} should be absent")]
async fn then_class_absent(world: &mut TestWorld, class: String) {
    assert!(world
        .engine_result
        .as_ref()
        .unwrap()
        .classes
        .get(&class)
        .is_none());
}

#[then(expr = "the variable {string} should be {string}")]
async fn then_variable(world: &mut TestWorld, key: String, value: String) {
    let r = world.engine_result.as_ref().unwrap();
    assert_eq!(r.variables[&key], json!(value));
}

#[then("the classification should report a merge conflict")]
async fn then_conflict(world: &mut TestWorld) {
    assert!(world
        .engine_result
        .as_ref()
        .unwrap()
        .conflict_error
        .is_some());
}

#[then(expr = "the group {string} should be explained as {string}")]
async fn then_explained(world: &mut TestWorld, group: String, outcome: String) {
    let explain = world.engine_explain.as_ref().unwrap();
    let g = explain
        .groups
        .iter()
        .find(|g| g.name == group)
        .expect("group");
    let want: GroupOutcome = serde_json::from_value(json!(outcome)).expect("outcome name");
    assert_eq!(g.outcome, want, "reason: {}", g.reason);
}

#[then(expr = "the provenance of the variable {string} should be {string}")]
async fn then_provenance(world: &mut TestWorld, key: String, groups: String) {
    let explain = world.engine_explain.as_ref().unwrap();
    let got: Vec<&str> = explain.provenance.variables[&key]
        .iter()
        .map(|o| o.group_name.as_str())
        .collect();
    let want: Vec<&str> = groups.split(',').map(str::trim).collect();
    assert_eq!(got, want);
}

#[then(expr = "the expression trace of the group {string} should show {string}")]
async fn then_trace(world: &mut TestWorld, group: String, label: String) {
    fn labels(t: &openvox_webui::models::ExprTrace, out: &mut Vec<String>) {
        out.push(t.label.clone());
        t.children.iter().for_each(|c| labels(c, out));
    }
    let explain = world.engine_explain.as_ref().unwrap();
    let g = explain
        .groups
        .iter()
        .find(|g| g.name == group)
        .expect("group");
    let mut all = vec![];
    labels(
        g.expression.as_ref().expect("an expression trace"),
        &mut all,
    );
    assert!(all.iter().any(|l| l == &label), "{all:?}");
}

#[when("I validate the expression:")]
async fn validate_expression(world: &mut TestWorld, step: &Step) {
    let expression = docstring(step);
    let (status, message) = match classification_expr::validate(&expression) {
        Ok(()) => (200, Value::Null),
        Err(e) => (400, json!(e)),
    };
    world.last_response = Some(TestResponse {
        status,
        body: json!({"error": message}),
    });
}

#[then(expr = "the expression should be rejected mentioning {string}")]
async fn then_rejected(world: &mut TestWorld, text: String) {
    let r = world.last_response.as_ref().unwrap();
    assert_eq!(r.status, 400);
    assert!(
        r.body["error"].as_str().unwrap().contains(&text),
        "{}",
        r.body
    );
}
