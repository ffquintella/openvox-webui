# Alert Rules Conditions System

## Overview

The alert rules conditions system provides a flexible way to define conditions that trigger alerts based on node status, facts, and reports. It supports complex nested conditions with logical operators (AND, OR, NOT) for sophisticated alerting scenarios.

## Condition Structure

### Core Models

```rust
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub rule_type: AlertRuleType,
    pub severity: AlertSeverity,
    pub conditions: Vec<Condition>,
    pub logical_operator: LogicalOperator,  // AND or OR
    pub notification_channels: Vec<String>,
    pub organization_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertRuleType {
    NodeStatus,      // Triggered by node status
    Facts,           // Triggered by fact values
    Reports,         // Triggered by report metrics
    Compliance,      // Triggered by compliance violations
    Drift,           // Triggered by configuration drift
    Custom,          // Custom conditions
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,      // Informational
    Warning,   // Warning
    Critical,  // Critical
    Emergency, // Emergency
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogicalOperator {
    #[serde(rename = "AND")]
    And,
    #[serde(rename = "OR")]
    Or,
}

pub struct Condition {
    pub id: String,
    pub condition_type: ConditionType,
    pub operator: ConditionOperator,
    pub value: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionType {
    NodeStatus {
        statuses: Vec<String>,  // failed, success, noop, unknown
    },
    NodeFact {
        fact_path: String,      // e.g., "os.family", "processors.count"
        data_type: FactDataType,
    },
    ReportMetric {
        metric: ReportMetricType,
    },
    EnvironmentFilter {
        environments: Vec<String>,
    },
    GroupFilter {
        group_ids: Vec<String>,
    },
    NodeCountThreshold {
        threshold: u32,
    },
    TimeWindowFilter {
        minutes: u32,           // Only check in last X minutes
    },
    LastReportTime {
        hours: u32,             // Node hasn't reported in N hours
    },
    ConsecutiveFailures {
        count: u32,             // N consecutive failed reports
        within_hours: u32,      // Within last N hours
    },
    ConsecutiveChanges {
        count: u32,             // N consecutive reports with changes
        within_hours: u32,
    },
    ClassChangeFrequency {
        class_name: String,     // Puppet class name
        change_count: u32,      // More than N changes
        within_hours: u32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FactDataType {
    String,
    Integer,
    Float,
    Boolean,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ReportMetricType {
    ResourcesChanged,
    ResourcesFailed,
    FailurePercentage,
    AverageRunTime,
    ReportCount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionOperator {
    // String/General
    #[serde(rename = "=")]
    Equals,
    #[serde(rename = "!=")]
    NotEquals,
    #[serde(rename = "~")]
    Matches,           // Regex
    #[serde(rename = "!~")]
    NotMatches,        // Regex NOT
    #[serde(rename = "in")]
    In,                // Array contains
    #[serde(rename = "not_in")]
    NotIn,
    
    // Numeric
    #[serde(rename = ">")]
    GreaterThan,
    #[serde(rename = ">=")]
    GreaterThanOrEqual,
    #[serde(rename = "<")]
    LessThan,
    #[serde(rename = "<=")]
    LessThanOrEqual,
    
    // Exists
    #[serde(rename = "exists")]
    Exists,
    #[serde(rename = "not_exists")]
    NotExists,
    
    // Array operations
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "not_contains")]
    NotContains,
}
```

## Condition Examples

### 1. Node Status Condition
Trigger alert when nodes reach failed status.

```json
{
  "condition_type": "NodeStatus",
  "operator": "in",
  "value": {
    "statuses": ["failed", "unknown"]
  },
  "enabled": true
}
```

### 2. Fact Value Condition
Trigger alert when CPU count exceeds threshold.

```json
{
  "condition_type": "NodeFact",
  "operator": ">=",
  "value": {
    "fact_path": "processors.count",
    "data_type": "Integer",
    "threshold": 32
  },
  "enabled": true
}
```

### 3. Memory Threshold Condition
Trigger alert when available memory is low.

```json
{
  "condition_type": "NodeFact",
  "operator": "<",
  "value": {
    "fact_path": "memory.system_mb",
    "data_type": "Integer",
    "threshold": 1024
  },
  "enabled": true
}
```

### 4. Report Metric Condition
Trigger alert when failed resources exceed percentage.

```json
{
  "condition_type": "ReportMetric",
  "operator": ">",
  "value": {
    "metric": "FailurePercentage",
    "threshold": 10
  },
  "enabled": true
}
```

### 5. Environment Filter Condition
Only alert for production and staging environments.

```json
{
  "condition_type": "EnvironmentFilter",
  "operator": "in",
  "value": {
    "environments": ["production", "staging"]
  },
  "enabled": true
}
```

### 6. Group Filter Condition
Only alert for web server group.

```json
{
  "condition_type": "GroupFilter",
  "operator": "in",
  "value": {
    "group_ids": ["web-servers-id", "load-balancers-id"]
  },
  "enabled": true
}
```

### 7. Node Count Threshold
Trigger alert when more than N nodes fail.

```json
{
  "condition_type": "NodeCountThreshold",
  "operator": ">",
  "value": {
    "threshold": 5
  },
  "enabled": true
}
```

### 8. Time Window Filter
Only check for alerts in last 30 minutes.

```json
{
  "condition_type": "TimeWindowFilter",
  "operator": "exists",
  "value": {
    "minutes": 30
  },
  "enabled": true
}
```

### 9. Last Report Time
Alert when a node hasn't reported in N hours.

```json
{
  "condition_type": "LastReportTime",
  "operator": ">",
  "value": {
    "hours": 24
  },
  "enabled": true
}
```

### 10. Consecutive Failures
Alert when a node has N consecutive failed reports within last X hours.

```json
{
  "condition_type": "ConsecutiveFailures",
  "operator": ">=",
  "value": {
    "count": 5,
    "within_hours": 12
  },
  "enabled": true
}
```

### 11. Consecutive Changes
Alert when a node has N consecutive reports with resource changes.

```json
{
  "condition_type": "ConsecutiveChanges",
  "operator": ">=",
  "value": {
    "count": 10,
    "within_hours": 24
  },
  "enabled": true
}
```

### 12. Class Change Frequency
Alert when a specific Puppet class has more than N changes in last X hours.

```json
{
  "condition_type": "ClassChangeFrequency",
  "operator": ">",
  "value": {
    "class_name": "apache::server",
    "change_count": 20,
    "within_hours": 6
  },
  "enabled": true
}
```

## Complex Rule Examples

### Example 1: Critical Infrastructure Alert
Alert when any production web server fails.

```json
{
  "name": "Production Web Server Failure",
  "rule_type": "NodeStatus",
  "severity": "Critical",
  "logical_operator": "AND",
  "conditions": [
    {
      "condition_type": "NodeStatus",
      "operator": "in",
      "value": {
        "statuses": ["failed"]
      }
    },
    {
      "condition_type": "EnvironmentFilter",
      "operator": "=",
      "value": {
        "environments": ["production"]
      }
    },
    {
      "condition_type": "GroupFilter",
      "operator": "in",
      "value": {
        "group_ids": ["web-servers-id"]
      }
    }
  ]
}
```

### Example 2: Resource Constraint Alert
Alert when multiple nodes have low memory OR high CPU.

```json
{
  "name": "Resource Constraints",
  "rule_type": "Facts",
  "severity": "Warning",
  "logical_operator": "OR",
  "conditions": [
    {
      "condition_type": "NodeFact",
      "operator": "<",
      "value": {
        "fact_path": "memory.system_mb",
        "data_type": "Integer",
        "threshold": 2048
      }
    },
    {
      "condition_type": "NodeFact",
      "operator": ">",
      "value": {
        "fact_path": "processors.count",
        "data_type": "Integer",
        "threshold": 64
      }
    }
  ]
}
```

### Example 3: Report Quality Alert
Alert when many nodes have failing resources in production.

```json
{
  "name": "High Failure Rate Production",
  "rule_type": "Reports",
  "severity": "Critical",
  "logical_operator": "AND",
  "conditions": [
    {
      "condition_type": "EnvironmentFilter",
      "operator": "=",
      "value": {
        "environments": ["production"]
      }
    },
    {
      "condition_type": "ReportMetric",
      "operator": ">",
      "value": {
        "metric": "FailurePercentage",
        "threshold": 15
      }
    },
    {
      "condition_type": "NodeCountThreshold",
      "operator": ">",
      "value": {
        "threshold": 10
      }
    }
  ]
}
```

### Example 4: Stale Node Detection
Alert when nodes stop reporting for 24 hours.

```json
{
  "name": "Stale Nodes - No Reports",
  "rule_type": "NodeStatus",
  "severity": "Warning",
  "logical_operator": "AND",
  "conditions": [
    {
      "condition_type": "LastReportTime",
      "operator": ">",
      "value": {
        "hours": 24
      }
    },
    {
      "condition_type": "EnvironmentFilter",
      "operator": "=",
      "value": {
        "environments": ["production", "staging"]
      }
    }
  ]
}
```

### Example 5: Node Instability Alert
Alert when a node has 5+ consecutive failures in 12 hours.

```json
{
  "name": "Unstable Node - Consecutive Failures",
  "rule_type": "NodeStatus",
  "severity": "Critical",
  "logical_operator": "AND",
  "conditions": [
    {
      "condition_type": "ConsecutiveFailures",
      "operator": ">=",
      "value": {
        "count": 5,
        "within_hours": 12
      }
    },
    {
      "condition_type": "EnvironmentFilter",
      "operator": "in",
      "value": {
        "environments": ["production"]
      }
    }
  ]
}
```

### Example 6: Excessive Changes Alert
Alert when node has 10+ consecutive reports with changes in 24 hours (indicating possible misconfiguration).

```json
{
  "name": "Excessive Resource Changes",
  "rule_type": "Reports",
  "severity": "Warning",
  "logical_operator": "AND",
  "conditions": [
    {
      "condition_type": "ConsecutiveChanges",
      "operator": ">=",
      "value": {
        "count": 10,
        "within_hours": 24
      }
    },
    {
      "condition_type": "GroupFilter",
      "operator": "in",
      "value": {
        "group_ids": ["web-servers-id"]
      }
    }
  ]
}
```

### Example 7: Class Churn Alert
Alert when apache::server class changes 20+ times in 6 hours (possible deployment issue).

```json
{
  "name": "High Churn - Apache Class Changes",
  "rule_type": "Custom",
  "severity": "Warning",
  "logical_operator": "AND",
  "conditions": [
    {
      "condition_type": "ClassChangeFrequency",
      "operator": ">",
      "value": {
        "class_name": "apache::server",
        "change_count": 20,
        "within_hours": 6
      }
    },
    {
      "condition_type": "EnvironmentFilter",
      "operator": "=",
      "value": {
        "environments": ["production"]
      }
    }
  ]
}
```

## Evaluation Engine

### Rule Evaluation Logic

```rust
pub struct RuleEvaluator;

impl RuleEvaluator {
    /// Evaluates an alert rule against the current infrastructure state
    pub async fn evaluate(
        rule: &AlertRule,
        puppetdb_service: &PuppetDbService,
        group_service: &GroupService,
    ) -> Result<Vec<AlertTrigger>, Error> {
        let mut matching_nodes = Vec::new();
        
        // Get all nodes from PuppetDB
        let nodes = puppetdb_service.list_nodes().await?;
        
        for node in nodes {
            if Self::node_matches_rule(rule, &node, puppetdb_service, group_service).await? {
                matching_nodes.push(node.certname.clone());
            }
        }
        
        // Generate alerts based on matching nodes
        Ok(Self::generate_triggers(&rule, matching_nodes))
    }
    
    /// Evaluates conditions against a node
    async fn node_matches_rule(
        rule: &AlertRule,
        node: &Node,
        puppetdb_service: &PuppetDbService,
        group_service: &GroupService,
    ) -> Result<bool, Error> {
        let enabled_conditions: Vec<_> = rule.conditions
            .iter()
            .filter(|c| c.enabled)
            .collect();
            
        match rule.logical_operator {
            LogicalOperator::And => {
                // All conditions must be true
                for condition in enabled_conditions {
                    if !Self::evaluate_condition(
                        condition,
                        node,
                        puppetdb_service,
                        group_service,
                    ).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            LogicalOperator::Or => {
                // At least one condition must be true
                for condition in enabled_conditions {
                    if Self::evaluate_condition(
                        condition,
                        node,
                        puppetdb_service,
                        group_service,
                    ).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
    
    /// Evaluates a single condition against a node
    async fn evaluate_condition(
        condition: &Condition,
        node: &Node,
        puppetdb_service: &PuppetDbService,
        group_service: &GroupService,
    ) -> Result<bool, Error> {
        match &condition.condition_type {
            ConditionType::NodeStatus { statuses } => {
                Ok(statuses.contains(&node.status))
            }
            
            ConditionType::NodeFact { fact_path, data_type } => {
                let facts = puppetdb_service
                    .get_node_facts(&node.certname)
                    .await?;
                
                Self::evaluate_fact_condition(
                    fact_path,
                    *data_type,
                    condition.operator,
                    &condition.value,
                    &facts,
                )
            }
            
            ConditionType::ReportMetric { metric } => {
                let latest_report = puppetdb_service
                    .get_latest_report(&node.certname)
                    .await?;
                    
                Self::evaluate_metric_condition(
                    *metric,
                    condition.operator,
                    &condition.value,
                    &latest_report,
                )
            }
            
            ConditionType::EnvironmentFilter { environments } => {
                Ok(environments.contains(&node.environment))
            }
            
            ConditionType::GroupFilter { group_ids } => {
                let node_groups = group_service
                    .get_node_groups(&node.certname)
                    .await?;
                    
                Ok(node_groups.iter().any(|g| group_ids.contains(&g.id)))
            }
            
            ConditionType::NodeCountThreshold { .. } => {
                // Handled at rule level
                Ok(true)
            }
            
            ConditionType::TimeWindowFilter { minutes } => {
                let cutoff = Utc::now() - Duration::minutes(*minutes as i64);
                Ok(node.report_timestamp > cutoff)
            }
            
            ConditionType::LastReportTime { hours } => {
                let cutoff = Utc::now() - Duration::hours(*hours as i64);
                Ok(node.report_timestamp < cutoff)
            }
            
            ConditionType::ConsecutiveFailures { count, within_hours } => {
                let reports = puppetdb_service
                    .get_node_reports(&node.certname, *within_hours as i32)
                    .await?;
                
                Self::evaluate_consecutive_failures(&reports, condition.operator, *count)
            }
            
            ConditionType::ConsecutiveChanges { count, within_hours } => {
                let reports = puppetdb_service
                    .get_node_reports(&node.certname, *within_hours as i32)
                    .await?;
                
                Self::evaluate_consecutive_changes(&reports, condition.operator, *count)
            }
            
            ConditionType::ClassChangeFrequency { class_name, change_count, within_hours } => {
                let reports = puppetdb_service
                    .get_node_reports(&node.certname, *within_hours as i32)
                    .await?;
                
                Self::evaluate_class_changes(&reports, class_name, condition.operator, *change_count)
            }
        }
    }
    
    /// Evaluates consecutive failure conditions
    fn evaluate_consecutive_failures(
        reports: &[Report],
        operator: ConditionOperator,
        threshold: u32,
    ) -> Result<bool, Error> {
        if reports.is_empty() {
            return Ok(false);
        }
        
        // Sort by timestamp descending (newest first)
        let mut sorted_reports = reports.to_vec();
        sorted_reports.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Count consecutive failures from most recent
        let mut consecutive_failures = 0;
        for report in sorted_reports {
            if report.status == "failed" {
                consecutive_failures += 1;
            } else {
                break; // Break on first non-failure
            }
        }
        
        Ok(match operator {
            ConditionOperator::GreaterThan => consecutive_failures > threshold as i32,
            ConditionOperator::GreaterThanOrEqual => consecutive_failures >= threshold as i32,
            ConditionOperator::Equals => consecutive_failures == threshold as i32,
            ConditionOperator::NotEquals => consecutive_failures != threshold as i32,
            ConditionOperator::LessThan => consecutive_failures < threshold as i32,
            ConditionOperator::LessThanOrEqual => consecutive_failures <= threshold as i32,
            _ => return Err(Error::InvalidOperator),
        })
    }
    
    /// Evaluates consecutive changes conditions
    fn evaluate_consecutive_changes(
        reports: &[Report],
        operator: ConditionOperator,
        threshold: u32,
    ) -> Result<bool, Error> {
        if reports.is_empty() {
            return Ok(false);
        }
        
        // Sort by timestamp descending (newest first)
        let mut sorted_reports = reports.to_vec();
        sorted_reports.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Count consecutive reports with changes from most recent
        let mut consecutive_changes = 0;
        for report in sorted_reports {
            let resources_changed = report.metrics.get("resources.changed").unwrap_or(&0);
            if *resources_changed > 0 {
                consecutive_changes += 1;
            } else {
                break; // Break on first report with no changes
            }
        }
        
        Ok(match operator {
            ConditionOperator::GreaterThan => consecutive_changes > threshold as i32,
            ConditionOperator::GreaterThanOrEqual => consecutive_changes >= threshold as i32,
            ConditionOperator::Equals => consecutive_changes == threshold as i32,
            ConditionOperator::NotEquals => consecutive_changes != threshold as i32,
            ConditionOperator::LessThan => consecutive_changes < threshold as i32,
            ConditionOperator::LessThanOrEqual => consecutive_changes <= threshold as i32,
            _ => return Err(Error::InvalidOperator),
        })
    }
    
    /// Evaluates class change frequency conditions
    fn evaluate_class_changes(
        reports: &[Report],
        class_name: &str,
        operator: ConditionOperator,
        threshold: u32,
    ) -> Result<bool, Error> {
        if reports.is_empty() {
            return Ok(false);
        }
        
        // Count how many reports have changes in the specified class
        let mut class_change_count = 0;
        
        for report in reports {
            // Parse resource changes to find class-specific changes
            for change in &report.resource_changes {
                // resource_type might be like "Class[Apache::Server]"
                if change.resource_type.contains(class_name) {
                    class_change_count += 1;
                    break; // Count once per report
                }
            }
        }
        
        Ok(match operator {
            ConditionOperator::GreaterThan => class_change_count > threshold as i32,
            ConditionOperator::GreaterThanOrEqual => class_change_count >= threshold as i32,
            ConditionOperator::Equals => class_change_count == threshold as i32,
            ConditionOperator::NotEquals => class_change_count != threshold as i32,
            ConditionOperator::LessThan => class_change_count < threshold as i32,
            ConditionOperator::LessThanOrEqual => class_change_count <= threshold as i32,
            _ => return Err(Error::InvalidOperator),
        })
    }
    
    /// Evaluates fact-based conditions
    fn evaluate_fact_condition(
        fact_path: &str,
        data_type: FactDataType,
        operator: ConditionOperator,
        value: &serde_json::Value,
        facts: &Facts,
    ) -> Result<bool, Error> {
        let fact_value = Self::extract_fact_value(fact_path, facts)?;
        
        match data_type {
            FactDataType::String => {
                Self::compare_string_values(operator, &fact_value, value)
            }
            FactDataType::Integer => {
                Self::compare_numeric_values(operator, &fact_value, value)
            }
            FactDataType::Float => {
                Self::compare_numeric_values(operator, &fact_value, value)
            }
            FactDataType::Boolean => {
                Self::compare_boolean_values(operator, &fact_value, value)
            }
        }
    }
    
    /// Evaluates report metric conditions
    fn evaluate_metric_condition(
        metric: ReportMetricType,
        operator: ConditionOperator,
        value: &serde_json::Value,
        report: &Report,
    ) -> Result<bool, Error> {
        let metric_value = match metric {
            ReportMetricType::ResourcesChanged => {
                report.metrics.get("resources.changed", 0)
            }
            ReportMetricType::ResourcesFailed => {
                report.metrics.get("resources.failed", 0)
            }
            ReportMetricType::FailurePercentage => {
                if report.metrics.get("resources.total", 1) == 0 {
                    0
                } else {
                    (report.metrics.get("resources.failed", 0) * 100) 
                        / report.metrics.get("resources.total", 1)
                }
            }
            ReportMetricType::AverageRunTime => {
                report.metrics.get("runtime", 0)
            }
            ReportMetricType::ReportCount => {
                1 // Will be aggregated at rule level
            }
        };
        
        Self::compare_numeric_values(
            operator,
            &json!(metric_value),
            value,
        )
    }
    
    // Helper comparison functions
    fn compare_string_values(
        operator: ConditionOperator,
        fact: &serde_json::Value,
        condition_value: &serde_json::Value,
    ) -> Result<bool, Error> {
        let fact_str = fact.as_str().ok_or(Error::InvalidFactValue)?;
        
        match operator {
            ConditionOperator::Equals => {
                Ok(fact_str == condition_value.as_str().unwrap_or(""))
            }
            ConditionOperator::NotEquals => {
                Ok(fact_str != condition_value.as_str().unwrap_or(""))
            }
            ConditionOperator::Matches => {
                let pattern = condition_value.as_str().unwrap_or("");
                Ok(regex::Regex::new(pattern)?.is_match(fact_str))
            }
            ConditionOperator::NotMatches => {
                let pattern = condition_value.as_str().unwrap_or("");
                Ok(!regex::Regex::new(pattern)?.is_match(fact_str))
            }
            ConditionOperator::In => {
                let values = condition_value.as_array().ok_or(Error::InvalidValue)?;
                Ok(values.iter().any(|v| v.as_str() == Some(fact_str)))
            }
            ConditionOperator::NotIn => {
                let values = condition_value.as_array().ok_or(Error::InvalidValue)?;
                Ok(values.iter().all(|v| v.as_str() != Some(fact_str)))
            }
            _ => Err(Error::InvalidOperator)
        }
    }
    
    fn compare_numeric_values(
        operator: ConditionOperator,
        fact: &serde_json::Value,
        condition_value: &serde_json::Value,
    ) -> Result<bool, Error> {
        let fact_num = fact.as_f64().ok_or(Error::InvalidFactValue)?;
        let condition_num = condition_value.as_f64().ok_or(Error::InvalidValue)?;
        
        Ok(match operator {
            ConditionOperator::Equals => (fact_num - condition_num).abs() < 0.001,
            ConditionOperator::NotEquals => (fact_num - condition_num).abs() >= 0.001,
            ConditionOperator::GreaterThan => fact_num > condition_num,
            ConditionOperator::GreaterThanOrEqual => fact_num >= condition_num,
            ConditionOperator::LessThan => fact_num < condition_num,
            ConditionOperator::LessThanOrEqual => fact_num <= condition_num,
            _ => return Err(Error::InvalidOperator),
        })
    }
    
    fn compare_boolean_values(
        operator: ConditionOperator,
        fact: &serde_json::Value,
        condition_value: &serde_json::Value,
    ) -> Result<bool, Error> {
        let fact_bool = fact.as_bool().ok_or(Error::InvalidFactValue)?;
        let condition_bool = condition_value.as_bool().ok_or(Error::InvalidValue)?;
        
        Ok(match operator {
            ConditionOperator::Equals => fact_bool == condition_bool,
            ConditionOperator::NotEquals => fact_bool != condition_bool,
            _ => return Err(Error::InvalidOperator),
        })
    }
    
    /// Generates alert triggers from matched nodes
    fn generate_triggers(
        rule: &AlertRule,
        matching_nodes: Vec<String>,
    ) -> Vec<AlertTrigger> {
        matching_nodes
            .into_iter()
            .map(|certname| AlertTrigger {
                id: uuid::Uuid::new_v4().to_string(),
                alert_rule_id: rule.id.clone(),
                certname,
                severity: rule.severity,
                triggered_at: Utc::now(),
                triggered_count: 1,
            })
            .collect()
    }
}

pub struct AlertTrigger {
    pub id: String,
    pub alert_rule_id: String,
    pub certname: String,
    pub severity: AlertSeverity,
    pub triggered_at: DateTime<Utc>,
    pub triggered_count: i32,
}
```

## API Endpoints

### Create Alert Rule with Conditions

```
POST /api/v1/alerting/rules
Content-Type: application/json

{
  "name": "Production Failure Alert",
  "description": "Alert on any production node failure",
  "enabled": true,
  "rule_type": "NodeStatus",
  "severity": "Critical",
  "logical_operator": "AND",
  "conditions": [
    {
      "condition_type": "NodeStatus",
      "operator": "in",
      "value": {
        "statuses": ["failed"]
      },
      "enabled": true
    },
    {
      "condition_type": "EnvironmentFilter",
      "operator": "=",
      "value": {
        "environments": ["production"]
      },
      "enabled": true
    }
  ],
  "notification_channels": ["email-admin", "slack-ops"]
}
```

### Response

```json
{
  "id": "rule-uuid",
  "name": "Production Failure Alert",
  "enabled": true,
  "rule_type": "NodeStatus",
  "severity": "Critical",
  "conditions": [
    {
      "id": "condition-uuid-1",
      "condition_type": "NodeStatus",
      "operator": "in",
      "value": {
        "statuses": ["failed"]
      },
      "enabled": true
    },
    {
      "id": "condition-uuid-2",
      "condition_type": "EnvironmentFilter",
      "operator": "=",
      "value": {
        "environments": ["production"]
      },
      "enabled": true
    }
  ],
  "logical_operator": "AND",
  "notification_channels": ["email-admin", "slack-ops"],
  "created_at": "2026-01-22T16:00:00Z",
  "updated_at": "2026-01-22T16:00:00Z"
}
```

### Test Rule Evaluation

```
POST /api/v1/alerting/rules/:id/test

Response:
{
  "rule_id": "rule-uuid",
  "matched_nodes": 5,
  "nodes": [
    "web01.example.com",
    "web02.example.com",
    "db01.example.com",
    "db02.example.com",
    "cache01.example.com"
  ],
  "evaluation_time_ms": 145
}
```

### Manually Trigger Rule Evaluation

```
POST /api/v1/alerting/evaluate
Content-Type: application/json

{
  "rule_id": "rule-uuid"
}
```

## Frontend Components

### Alert Rule Builder

```typescript
interface AlertRuleBuilderProps {
  rule?: AlertRule;
  onSave: (rule: AlertRule) => Promise<void>;
  onCancel: () => void;
}

// Condition selector component
interface ConditionSelectorProps {
  conditions: Condition[];
  onAdd: (condition: Condition) => void;
  onRemove: (conditionId: string) => void;
  onUpdate: (condition: Condition) => void;
}

// Condition editor component
interface ConditionEditorProps {
  condition: Condition;
  onChange: (condition: Condition) => void;
}
```

### UI Layout

1. **Rule Details Tab:**
   - Name, Description
   - Type (NodeStatus, Facts, Reports, etc.)
   - Severity (Info, Warning, Critical, Emergency)
   - Enabled toggle

2. **Conditions Tab:**
   - Logical operator selector (AND/OR)
   - List of conditions with ability to add/remove
   - Each condition shows:
     - Type selector
     - Operator selector
     - Value editor (context-dependent)
     - Enabled toggle

3. **Notification Tab:**
   - Multi-select for notification channels
   - Preview of condition evaluation results

4. **Testing Tab:**
   - Test button to evaluate rule
   - Shows matched nodes
   - Shows evaluation time

## Database Schema

```sql
CREATE TABLE alert_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    rule_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    logical_operator TEXT NOT NULL DEFAULT 'AND',
    organization_id TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    created_by TEXT NOT NULL,
    FOREIGN KEY (organization_id) REFERENCES organizations(id)
);

CREATE TABLE alert_rule_conditions (
    id TEXT PRIMARY KEY,
    alert_rule_id TEXT NOT NULL,
    condition_type TEXT NOT NULL,
    operator TEXT NOT NULL,
    value JSONB NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    position INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    FOREIGN KEY (alert_rule_id) REFERENCES alert_rules(id) ON DELETE CASCADE
);

CREATE TABLE alert_triggers (
    id TEXT PRIMARY KEY,
    alert_rule_id TEXT NOT NULL,
    certname TEXT NOT NULL,
    severity TEXT NOT NULL,
    triggered_at TIMESTAMP NOT NULL,
    triggered_count INTEGER NOT NULL DEFAULT 1,
    acknowledged_at TIMESTAMP,
    resolved_at TIMESTAMP,
    organization_id TEXT NOT NULL,
    FOREIGN KEY (alert_rule_id) REFERENCES alert_rules(id)
);
```

## Condition Types Reference

| Condition Type | Purpose | Parameters | Operators |
|---|---|---|---|
| **NodeStatus** | Detect node status | statuses (array) | `in`, `not_in` |
| **NodeFact** | Match fact values with threshold | fact_path, data_type, threshold | `=`, `!=`, `>`, `>=`, `<`, `<=`, `~`, `!~` |
| **ReportMetric** | Monitor report metrics | metric, threshold | `=`, `!=`, `>`, `>=`, `<`, `<=` |
| **EnvironmentFilter** | Scope to environments | environments (array) | `=`, `in`, `not_in` |
| **GroupFilter** | Scope to node groups | group_ids (array) | `in`, `not_in` |
| **NodeCountThreshold** | Alert when N+ nodes match | threshold | `>`, `>=`, `<`, `<=`, `=` |
| **TimeWindowFilter** | Only check recent events | minutes | `exists` |
| **LastReportTime** | Detect stale nodes | hours | `>`, `>=`, `<`, `<=` |
| **ConsecutiveFailures** | Detect node instability | count, within_hours | `>`, `>=`, `<`, `<=`, `=` |
| **ConsecutiveChanges** | Excessive resource changes | count, within_hours | `>`, `>=`, `<`, `<=`, `=` |
| **ClassChangeFrequency** | Detect class churn | class_name, change_count, within_hours | `>`, `>=`, `<`, `<=`, `=` |

## Condition Operators Reference

| Data Type | Operators |
|-----------|-----------|
| String | `=`, `!=`, `~`, `!~`, `in`, `not_in`, `exists`, `not_exists` |
| Integer | `=`, `!=`, `>`, `>=`, `<`, `<=`, `in`, `not_in` |
| Float | `=`, `!=`, `>`, `>=`, `<`, `<=` |
| Boolean | `=`, `!=` |
| Threshold-based | `=`, `!=`, `>`, `>=`, `<`, `<=` |

## Performance Considerations

- **Rule Evaluation Frequency:** Default 5 minutes, configurable
- **Caching:** Cache node facts for 10 minutes to reduce PuppetDB load
- **Report History:** Cache recent reports (last 24-48 hours) for consecutive condition checks
- **Batch Processing:** Evaluate rules in background job
- **Debouncing:** Wait 2 consecutive failures before alerting
- **Query Optimization:** Use database indexes on timestamps for time-window queries
- **Metrics:** Track evaluation time, matched nodes, alert frequency

## Query Performance Tips

1. **Consecutive Conditions:** Limit time windows to reduce report queries (use 12-24 hours max)
2. **Class Frequency:** Use specific class names rather than wildcards
3. **Fact Lookups:** Cache fact paths to avoid repeated parsing
4. **Batch Aggregation:** Group similar rules to reuse query results
5. **Alert Deduplication:** Use report timestamp to avoid duplicate triggers within same run

## Best Practices

1. **Use Filters First:** Add EnvironmentFilter or GroupFilter to reduce scope
2. **Combine Conditions Wisely:** Use AND for stricter, OR for broader alerts
3. **Time Windows:** Add TimeWindowFilter for recent events or LastReportTime for stale detection
4. **Node Counts:** Use NodeCountThreshold to avoid alerting on single node failures
5. **Consecutive Thresholds:** Set reasonable counts for ConsecutiveFailures/Changes (typically 3-5 minimum)
6. **Class Monitoring:** Use ClassChangeFrequency to detect deployment issues early
7. **Testing:** Use the test endpoint before enabling rules
8. **Documentation:** Add descriptions to all alert rules for maintainability
9. **Review Regularly:** Audit alert rules quarterly for relevance and accuracy
10. **Gradual Rollout:** Start with Warning severity, escalate to Critical only after tuning

## Common Alert Scenarios

### Infrastructure Monitoring
- Detect stale nodes: `LastReportTime > 24 hours`
- Alert on node crashes: `ConsecutiveFailures >= 3 in 6 hours`
- Monitor instability: `ConsecutiveFailures >= 5 in 12 hours + EnvironmentFilter=production`

### Configuration Management
- Excessive changes: `ConsecutiveChanges >= 10 in 24 hours`
- Class churn detection: `ClassChangeFrequency > 20 in 6 hours`
- Problematic deployments: `ConsecutiveChanges >= 8 AND ReportMetric.FailurePercentage > 5`

### Resource Compliance
- Low memory nodes: `NodeFact(memory.system_mb) < 2048 AND EnvironmentFilter=production`
- High CPU count: `NodeFact(processors.count) > 64 AND GroupFilter=database-servers`
- OS version tracking: `NodeFact(os.release) ~= "^8\." AND EnvironmentFilter=production`

### Operational Health
- Multi-node failures: `ReportMetric.FailurePercentage > 15 AND NodeCountThreshold > 10`
- Change rate monitoring: `ConsecutiveChanges >= 5 in 4 hours`
- Report quality: `ReportMetric.ResourcesFailed > 100 AND TimeWindowFilter=last 1 hour`
