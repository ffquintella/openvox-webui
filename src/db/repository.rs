//! Repository pattern implementations for database access

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{
    ClassificationRule, CreateGroupRequest, CreateRuleRequest, NodeGroup, RuleMatchType,
    RuleOperator, UpdateGroupRequest,
};

/// Repository for node group operations
pub struct GroupRepository<'a> {
    pool: &'a SqlitePool,
}

/// Row returned from node_groups table
#[derive(Debug, sqlx::FromRow)]
struct GroupRow {
    id: String,
    name: String,
    description: Option<String>,
    parent_id: Option<String>,
    environment: Option<String>,
    rule_match_type: String,
    classes: String,
    parameters: String,
}

/// Row returned from classification_rules table
#[derive(Debug, sqlx::FromRow)]
struct RuleRow {
    id: String,
    fact_path: String,
    operator: String,
    value: String,
}

/// Row returned from pinned_nodes table
#[derive(Debug, sqlx::FromRow)]
struct PinnedNodeRow {
    certname: String,
}

impl<'a> GroupRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all node groups with their rules and pinned nodes
    pub async fn get_all(&self) -> Result<Vec<NodeGroup>> {
        let rows = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT id, name, description, parent_id, environment,
                   rule_match_type, classes, parameters
            FROM node_groups
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch groups")?;

        let mut groups = Vec::with_capacity(rows.len());
        for row in rows {
            let group = self.row_to_group(row).await?;
            groups.push(group);
        }
        Ok(groups)
    }

    /// Get a node group by ID with its rules and pinned nodes
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<NodeGroup>> {
        let row = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT id, name, description, parent_id, environment,
                   rule_match_type, classes, parameters
            FROM node_groups
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch group")?;

        match row {
            Some(r) => Ok(Some(self.row_to_group(r).await?)),
            None => Ok(None),
        }
    }

    /// Create a new node group
    pub async fn create(&self, req: &CreateGroupRequest) -> Result<NodeGroup> {
        let id = Uuid::new_v4();
        let rule_match_type = req
            .rule_match_type
            .unwrap_or(RuleMatchType::All)
            .to_string();
        let classes = serde_json::to_string(&req.classes.clone().unwrap_or_default())
            .unwrap_or_else(|_| "[]".to_string());
        let parameters = req
            .parameters
            .clone()
            .map(|p| serde_json::to_string(&p).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());

        sqlx::query(
            r#"
            INSERT INTO node_groups (id, name, description, parent_id, environment,
                                     rule_match_type, classes, parameters)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(req.parent_id.map(|p| p.to_string()))
        .bind(&req.environment)
        .bind(&rule_match_type)
        .bind(&classes)
        .bind(&parameters)
        .execute(self.pool)
        .await
        .context("Failed to create group")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created group"))
    }

    /// Update a node group
    pub async fn update(&self, id: Uuid, req: &UpdateGroupRequest) -> Result<Option<NodeGroup>> {
        // First check if the group exists
        let existing = self.get_by_id(id).await?;
        if existing.is_none() {
            return Ok(None);
        }
        let existing = existing.unwrap();

        // Build the update with existing values as fallback
        let name = req.name.clone().unwrap_or(existing.name);
        let description = req.description.clone().or(existing.description);
        let parent_id = req.parent_id.or(existing.parent_id);
        let environment = req.environment.clone().or(existing.environment);
        let rule_match_type = req
            .rule_match_type
            .unwrap_or(existing.rule_match_type)
            .to_string();
        let classes = req
            .classes
            .clone()
            .map(|c| serde_json::to_string(&c).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| {
                serde_json::to_string(&existing.classes).unwrap_or_else(|_| "[]".to_string())
            });
        let parameters = req
            .parameters
            .clone()
            .map(|p| serde_json::to_string(&p).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| {
                serde_json::to_string(&existing.parameters).unwrap_or_else(|_| "{}".to_string())
            });

        sqlx::query(
            r#"
            UPDATE node_groups
            SET name = ?, description = ?, parent_id = ?, environment = ?,
                rule_match_type = ?, classes = ?, parameters = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(&name)
        .bind(&description)
        .bind(parent_id.map(|p| p.to_string()))
        .bind(&environment)
        .bind(&rule_match_type)
        .bind(&classes)
        .bind(&parameters)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update group")?;

        self.get_by_id(id).await
    }

    /// Delete a node group (rules and pinned nodes are deleted via CASCADE)
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM node_groups WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete group")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get all rules for a group
    pub async fn get_rules(&self, group_id: Uuid) -> Result<Vec<ClassificationRule>> {
        let rows = sqlx::query_as::<_, RuleRow>(
            r#"
            SELECT id, fact_path, operator, value
            FROM classification_rules
            WHERE group_id = ?
            ORDER BY fact_path
            "#,
        )
        .bind(group_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch rules")?;

        Ok(rows.into_iter().map(|r| row_to_rule(r)).collect())
    }

    /// Add a rule to a group
    pub async fn add_rule(
        &self,
        group_id: Uuid,
        req: &CreateRuleRequest,
    ) -> Result<ClassificationRule> {
        let id = Uuid::new_v4();
        let operator = operator_to_string(&req.operator);
        let value = serde_json::to_string(&req.value).unwrap_or_else(|_| "null".to_string());

        sqlx::query(
            r#"
            INSERT INTO classification_rules (id, group_id, fact_path, operator, value)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(group_id.to_string())
        .bind(&req.fact_path)
        .bind(&operator)
        .bind(&value)
        .execute(self.pool)
        .await
        .context("Failed to add rule")?;

        Ok(ClassificationRule {
            id,
            fact_path: req.fact_path.clone(),
            operator: req.operator,
            value: req.value.clone(),
        })
    }

    /// Delete a rule
    pub async fn delete_rule(&self, group_id: Uuid, rule_id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM classification_rules WHERE id = ? AND group_id = ?")
            .bind(rule_id.to_string())
            .bind(group_id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete rule")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get all pinned nodes for a group
    pub async fn get_pinned_nodes(&self, group_id: Uuid) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, PinnedNodeRow>(
            r#"
            SELECT certname
            FROM pinned_nodes
            WHERE group_id = ?
            ORDER BY certname
            "#,
        )
        .bind(group_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch pinned nodes")?;

        Ok(rows.into_iter().map(|r| r.certname).collect())
    }

    /// Add a pinned node to a group
    pub async fn add_pinned_node(&self, group_id: Uuid, certname: &str) -> Result<()> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO pinned_nodes (id, group_id, certname)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(group_id.to_string())
        .bind(certname)
        .execute(self.pool)
        .await
        .context("Failed to add pinned node")?;

        Ok(())
    }

    /// Remove a pinned node from a group
    pub async fn remove_pinned_node(&self, group_id: Uuid, certname: &str) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM pinned_nodes WHERE group_id = ? AND certname = ?")
                .bind(group_id.to_string())
                .bind(certname)
                .execute(self.pool)
                .await
                .context("Failed to remove pinned node")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get nodes that match a group (pinned + classified by rules)
    /// For now, returns only pinned nodes. Full classification requires PuppetDB integration.
    pub async fn get_group_nodes(&self, group_id: Uuid) -> Result<Vec<String>> {
        // For now, just return pinned nodes
        // Full implementation would query PuppetDB and run classification
        self.get_pinned_nodes(group_id).await
    }

    /// Convert a database row to a NodeGroup with rules and pinned nodes
    async fn row_to_group(&self, row: GroupRow) -> Result<NodeGroup> {
        let id = Uuid::parse_str(&row.id).context("Invalid group ID")?;

        let rules = self.get_rules(id).await?;
        let pinned_nodes = self.get_pinned_nodes(id).await?;

        let classes: Vec<String> =
            serde_json::from_str(&row.classes).unwrap_or_default();
        let parameters: serde_json::Value =
            serde_json::from_str(&row.parameters).unwrap_or(serde_json::json!({}));

        Ok(NodeGroup {
            id,
            name: row.name,
            description: row.description,
            parent_id: row.parent_id.and_then(|p| Uuid::parse_str(&p).ok()),
            environment: row.environment,
            rule_match_type: parse_rule_match_type(&row.rule_match_type),
            classes,
            parameters,
            rules,
            pinned_nodes,
        })
    }
}

/// Convert RuleMatchType to string for storage
impl std::fmt::Display for RuleMatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleMatchType::All => write!(f, "all"),
            RuleMatchType::Any => write!(f, "any"),
        }
    }
}

/// Parse rule match type from string
fn parse_rule_match_type(s: &str) -> RuleMatchType {
    match s.to_lowercase().as_str() {
        "any" => RuleMatchType::Any,
        _ => RuleMatchType::All,
    }
}

/// Convert a rule row to ClassificationRule
fn row_to_rule(row: RuleRow) -> ClassificationRule {
    ClassificationRule {
        id: Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::new_v4()),
        fact_path: row.fact_path,
        operator: parse_operator(&row.operator),
        value: serde_json::from_str(&row.value).unwrap_or(serde_json::Value::Null),
    }
}

/// Convert RuleOperator to string for storage
fn operator_to_string(op: &RuleOperator) -> String {
    match op {
        RuleOperator::Equals => "=".to_string(),
        RuleOperator::NotEquals => "!=".to_string(),
        RuleOperator::Regex => "~".to_string(),
        RuleOperator::NotRegex => "!~".to_string(),
        RuleOperator::GreaterThan => ">".to_string(),
        RuleOperator::GreaterThanOrEqual => ">=".to_string(),
        RuleOperator::LessThan => "<".to_string(),
        RuleOperator::LessThanOrEqual => "<=".to_string(),
        RuleOperator::In => "in".to_string(),
        RuleOperator::NotIn => "not_in".to_string(),
    }
}

/// Parse operator string to RuleOperator
fn parse_operator(s: &str) -> RuleOperator {
    match s {
        "=" => RuleOperator::Equals,
        "!=" => RuleOperator::NotEquals,
        "~" => RuleOperator::Regex,
        "!~" => RuleOperator::NotRegex,
        ">" => RuleOperator::GreaterThan,
        ">=" => RuleOperator::GreaterThanOrEqual,
        "<" => RuleOperator::LessThan,
        "<=" => RuleOperator::LessThanOrEqual,
        "in" => RuleOperator::In,
        "not_in" => RuleOperator::NotIn,
        _ => RuleOperator::Equals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_match_type_display() {
        assert_eq!(RuleMatchType::All.to_string(), "all");
        assert_eq!(RuleMatchType::Any.to_string(), "any");
    }

    #[test]
    fn test_parse_rule_match_type() {
        assert_eq!(parse_rule_match_type("all"), RuleMatchType::All);
        assert_eq!(parse_rule_match_type("any"), RuleMatchType::Any);
        assert_eq!(parse_rule_match_type("ALL"), RuleMatchType::All);
        assert_eq!(parse_rule_match_type("invalid"), RuleMatchType::All);
    }

    #[test]
    fn test_operator_roundtrip() {
        let operators = vec![
            RuleOperator::Equals,
            RuleOperator::NotEquals,
            RuleOperator::Regex,
            RuleOperator::NotRegex,
            RuleOperator::GreaterThan,
            RuleOperator::GreaterThanOrEqual,
            RuleOperator::LessThan,
            RuleOperator::LessThanOrEqual,
            RuleOperator::In,
            RuleOperator::NotIn,
        ];

        for op in operators {
            let s = operator_to_string(&op);
            let parsed = parse_operator(&s);
            assert_eq!(op, parsed);
        }
    }
}
