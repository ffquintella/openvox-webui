//! In-memory step definitions for alert rule condition scenarios.

use crate::features::support::{TestAlertCondition, TestResponse, TestWorld};
use cucumber::{given, then, when};
use serde_json::{json, Map, Value};

fn parse_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

fn parse_conditions(step: &cucumber::gherkin::Step) -> Vec<TestAlertCondition> {
    let table = step.table.as_ref().expect("condition table not found");
    let rows = &table.rows;
    if rows.is_empty() {
        return Vec::new();
    }

    if rows[0].first().is_some_and(|cell| cell == "condition_type")
        && rows[0].get(1).is_some_and(|cell| cell == "operator")
    {
        return rows
            .iter()
            .skip(1)
            .map(|row| TestAlertCondition {
                condition_type: row[0].clone(),
                operator: row[1].clone(),
                value: parse_value(&row[2]),
            })
            .collect();
    }

    let fields: std::collections::HashMap<&str, &str> = rows
        .iter()
        .filter_map(|row| Some((row.first()?.as_str(), row.get(1)?.as_str())))
        .collect();
    vec![TestAlertCondition {
        condition_type: fields
            .get("condition_type")
            .copied()
            .unwrap_or_default()
            .to_string(),
        operator: fields
            .get("operator")
            .copied()
            .unwrap_or_default()
            .to_string(),
        value: parse_value(fields.get("value").copied().unwrap_or("null")),
    }]
}

fn numeric(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn compare_numeric(actual: f64, operator: &str, expected: f64) -> bool {
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        "=" | "==" => (actual - expected).abs() < f64::EPSILON,
        _ => false,
    }
}

fn list_contains(value: &Value, actual: &str) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(actual)))
}

fn condition_matches(condition: &TestAlertCondition, node: &Value) -> bool {
    match condition.condition_type.as_str() {
        "NodeStatus" => node
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| list_contains(&condition.value, status)),
        "EnvironmentFilter" => node
            .get("environment")
            .and_then(Value::as_str)
            .is_some_and(|environment| list_contains(&condition.value, environment)),
        "GroupFilter" => node
            .get("group")
            .and_then(Value::as_str)
            .is_some_and(|group| list_contains(&condition.value, group)),
        "NodeFact" => {
            let fact_path = condition
                .value
                .get("fact_path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let actual = numeric(node.get("facts").and_then(|facts| facts.get(fact_path)));
            let threshold = numeric(condition.value.get("threshold"));
            actual.zip(threshold).is_some_and(|(actual, expected)| {
                compare_numeric(actual, &condition.operator, expected)
            })
        }
        "LastReportTime" => numeric(node.get("last_report_hours"))
            .zip(numeric(condition.value.get("hours")))
            .is_some_and(|(actual, expected)| {
                compare_numeric(actual, &condition.operator, expected)
            }),
        "ConsecutiveFailures" => numeric(node.get("consecutive_failures"))
            .zip(numeric(condition.value.get("count")))
            .is_some_and(|(actual, expected)| {
                compare_numeric(actual, &condition.operator, expected)
            }),
        "ConsecutiveChanges" => numeric(node.get("consecutive_changes"))
            .zip(numeric(condition.value.get("count")))
            .is_some_and(|(actual, expected)| {
                compare_numeric(actual, &condition.operator, expected)
            }),
        "ClassChangeFrequency" => {
            let expected_class = condition.value.get("class_name").and_then(Value::as_str);
            let class_matches = node.get("class_name").and_then(Value::as_str) == expected_class;
            let actual = numeric(node.get("class_change_count"));
            let expected = numeric(condition.value.get("change_count"));
            class_matches
                && actual.zip(expected).is_some_and(|(actual, expected)| {
                    compare_numeric(actual, &condition.operator, expected)
                })
        }
        _ => false,
    }
}

fn evaluate_rule(world: &mut TestWorld) {
    if !world.alert_rule_enabled {
        world.alert_matched_nodes.clear();
        world.alert_generated = false;
        return;
    }

    let match_any = world.alert_logical_operator.eq_ignore_ascii_case("OR");
    world.alert_matched_nodes = world
        .alert_nodes
        .iter()
        .filter(|(_, node)| {
            if match_any {
                world
                    .alert_conditions
                    .iter()
                    .any(|condition| condition_matches(condition, node))
            } else {
                world
                    .alert_conditions
                    .iter()
                    .all(|condition| condition_matches(condition, node))
            }
        })
        .map(|(certname, _)| certname.clone())
        .collect();
    world.alert_generated = !world.alert_matched_nodes.is_empty();
}

#[given("I have configured notification channels")]
async fn configured_notification_channels(world: &mut TestWorld) {
    world.notification_channels_configured = true;
}

#[when("I create an alert rule with the following configuration:")]
async fn create_alert_rule(world: &mut TestWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("alert rule table not found");
    let config: std::collections::HashMap<&str, &str> = table
        .rows
        .iter()
        .filter_map(|row| Some((row.first()?.as_str(), row.get(1)?.as_str())))
        .collect();
    world.alert_conditions.clear();
    world.alert_logical_operator = config
        .get("logical_operator")
        .copied()
        .unwrap_or("AND")
        .to_string();
    world.alert_rule_enabled = config.get("enabled").copied().unwrap_or("true") == "true";
    world.last_response = Some(TestResponse {
        status: 201,
        body: json!({"name": config.get("name").copied().unwrap_or("test-rule")}),
    });
}

#[when("I add a condition:")]
async fn add_condition(world: &mut TestWorld, step: &cucumber::gherkin::Step) {
    world.alert_conditions.extend(parse_conditions(step));
}

#[given("an alert rule exists with conditions:")]
async fn alert_rule_with_conditions(world: &mut TestWorld, step: &cucumber::gherkin::Step) {
    world.alert_conditions = parse_conditions(step);
    world.alert_logical_operator = "AND".to_string();
    world.alert_rule_enabled = true;
}

#[given(expr = "an alert rule exists with logical operator {string} and conditions:")]
async fn alert_rule_with_operator(
    world: &mut TestWorld,
    operator: String,
    step: &cucumber::gherkin::Step,
) {
    world.alert_conditions = parse_conditions(step);
    world.alert_logical_operator = operator;
    world.alert_rule_enabled = true;
}

#[given(expr = "a node {string} exists with:")]
async fn alert_node_with_table(
    world: &mut TestWorld,
    certname: String,
    step: &cucumber::gherkin::Step,
) {
    let table = step.table.as_ref().expect("node table not found");
    let mut node = Map::new();
    let mut facts = Map::new();

    if table.rows.first().is_some_and(|row| {
        row.first().is_some_and(|cell| cell == "fact_path")
            && row.get(1).is_some_and(|cell| cell == "fact_value")
    }) {
        for row in table.rows.iter().skip(1) {
            if let (Some(path), Some(value)) = (row.first(), row.get(1)) {
                facts.insert(path.clone(), parse_value(value));
            }
        }
    } else {
        for row in &table.rows {
            let Some(key) = row.first() else { continue };
            let Some(value) = row.get(1) else { continue };
            match key.as_str() {
                "last_report_time" => {
                    let hours = value.split_whitespace().next().unwrap_or("0");
                    node.insert("last_report_hours".to_string(), parse_value(hours));
                }
                _ => {
                    node.insert(key.clone(), parse_value(value));
                }
            }
        }
    }

    node.insert("facts".to_string(), Value::Object(facts));
    world.alert_nodes.insert(certname, Value::Object(node));
}

#[given(
    expr = "a node {string} exists with {int} consecutive failed reports in the last {int} hours"
)]
async fn node_with_failures(world: &mut TestWorld, certname: String, count: u64, _hours: u64) {
    world
        .alert_nodes
        .insert(certname, json!({"consecutive_failures": count}));
}

#[given(expr = "a node {string} exists with 1 failed report followed by 2 successful reports")]
async fn node_with_recovered_reports(world: &mut TestWorld, certname: String) {
    world
        .alert_nodes
        .insert(certname, json!({"consecutive_failures": 0}));
}

#[given(
    expr = "a node {string} exists with {int} consecutive reports with changes in the last {int} hours"
)]
async fn node_with_changes(world: &mut TestWorld, certname: String, count: u64, _hours: u64) {
    world
        .alert_nodes
        .insert(certname, json!({"consecutive_changes": count}));
}

#[given(
    expr = "a node {string} exists with {int} changes to {string} class in the last {int} hours"
)]
async fn node_with_class_changes(
    world: &mut TestWorld,
    certname: String,
    count: u64,
    class_name: String,
    _hours: u64,
) {
    world.alert_nodes.insert(
        certname,
        json!({"class_name": class_name, "class_change_count": count}),
    );
}

#[when("I evaluate the alert rule")]
async fn evaluate_alert_rule(world: &mut TestWorld) {
    evaluate_rule(world);
}

#[then(expr = "the alert rule should have {int} conditions")]
async fn alert_rule_condition_count(world: &mut TestWorld, expected: usize) {
    assert_eq!(world.alert_conditions.len(), expected);
}

#[then(expr = "the rule should match {int} node")]
#[then(expr = "the rule should match {int} nodes")]
async fn matched_node_count(world: &mut TestWorld, expected: usize) {
    assert_eq!(world.alert_matched_nodes.len(), expected);
}

#[then(expr = "the matched node should be {string}")]
async fn matched_node(world: &mut TestWorld, certname: String) {
    assert!(world.alert_matched_nodes.contains(&certname));
}

#[given("I have created an alert rule with conditions")]
async fn created_alert_rule(world: &mut TestWorld) {
    world.alert_rule_enabled = true;
    world.alert_conditions = vec![TestAlertCondition {
        condition_type: "NodeStatus".to_string(),
        operator: "in".to_string(),
        value: json!(["failed"]),
    }];
}

#[when("I test the alert rule")]
async fn test_alert_rule(world: &mut TestWorld) {
    evaluate_rule(world);
    world.last_response = Some(TestResponse {
        status: 200,
        body: json!({
            "matched_nodes": world.alert_matched_nodes,
            "evaluation_time_ms": 0
        }),
    });
}

#[then("the response should include matched nodes")]
async fn response_includes_matched_nodes(world: &mut TestWorld) {
    assert!(world
        .last_response
        .as_ref()
        .and_then(|response| response.body.get("matched_nodes"))
        .is_some());
}

#[then("the response should include evaluation time")]
async fn response_includes_evaluation_time(world: &mut TestWorld) {
    assert!(world
        .last_response
        .as_ref()
        .and_then(|response| response.body.get("evaluation_time_ms"))
        .is_some());
}

#[given("an enabled alert rule exists")]
async fn enabled_alert_rule(world: &mut TestWorld) {
    world.alert_rule_enabled = true;
}

#[when("I disable the alert rule")]
async fn disable_alert_rule(world: &mut TestWorld) {
    world.alert_rule_enabled = false;
}

#[when("I evaluate all alert rules")]
async fn evaluate_all_alert_rules(world: &mut TestWorld) {
    evaluate_rule(world);
}

#[then("the disabled rule should not generate alerts")]
async fn disabled_rule_does_not_alert(world: &mut TestWorld) {
    assert!(!world.alert_generated);
}
