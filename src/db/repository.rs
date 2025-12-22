//! Repository pattern implementations for database access

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::{
    ClassificationRule, CreateGroupRequest, CreateRuleRequest, FactDefinition, FactTemplate,
    NodeGroup, RuleMatchType, RuleOperator, UpdateGroupRequest,
};

/// Repository for node group operations
pub struct GroupRepository<'a> {
    pool: &'a SqlitePool,
}

/// Row returned from node_groups table
#[derive(Debug, sqlx::FromRow)]
struct GroupRow {
    id: String,
    organization_id: String,
    name: String,
    description: Option<String>,
    parent_id: Option<String>,
    environment: Option<String>,
    rule_match_type: String,
    classes: String,
    #[allow(dead_code)] // Kept for database backward compatibility
    parameters: String,
    variables: String,
}

/// Row returned from classification_rules table (with group_id for batch loading)
#[derive(Debug, sqlx::FromRow)]
struct RuleRowWithGroup {
    id: String,
    group_id: String,
    fact_path: String,
    operator: String,
    value: String,
}

/// Row returned from classification_rules table
#[derive(Debug, sqlx::FromRow)]
struct RuleRow {
    id: String,
    fact_path: String,
    operator: String,
    value: String,
}

/// Row returned from pinned_nodes table (with group_id for batch loading)
#[derive(Debug, sqlx::FromRow)]
struct PinnedNodeRowWithGroup {
    group_id: String,
    certname: String,
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
    ///
    /// Optimized to use batch loading instead of N+1 queries.
    /// Previously executed 1 + 2N queries, now executes only 3 queries total.
    pub async fn get_all(&self, organization_id: Uuid) -> Result<Vec<NodeGroup>> {
        let rows = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT id, organization_id, name, description, parent_id, environment,
                   rule_match_type, classes, parameters, variables
            FROM node_groups
            WHERE organization_id = ?
            ORDER BY name
            "#,
        )
        .bind(organization_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch groups")?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Collect all group IDs for batch loading
        let group_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();

        // Batch load all rules for these groups (single query)
        let rules_map = self.batch_get_rules(&group_ids).await?;

        // Batch load all pinned nodes for these groups (single query)
        let pinned_map = self.batch_get_pinned_nodes(&group_ids).await?;

        // Convert rows to groups using the pre-loaded data
        let mut groups = Vec::with_capacity(rows.len());
        for row in rows {
            let group = self.row_to_group_with_data(row, &rules_map, &pinned_map)?;
            groups.push(group);
        }
        Ok(groups)
    }

    /// Get all node groups from ALL organizations with their rules and pinned nodes
    ///
    /// This is used for public classification where we need to classify a node
    /// against all organizations and detect conflicts.
    pub async fn get_all_across_organizations(&self) -> Result<Vec<NodeGroup>> {
        let rows = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT id, organization_id, name, description, parent_id, environment,
                   rule_match_type, classes, parameters, variables
            FROM node_groups
            ORDER BY organization_id, name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch groups across organizations")?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Collect all group IDs for batch loading
        let group_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();

        // Batch load all rules for these groups (single query)
        let rules_map = self.batch_get_rules(&group_ids).await?;

        // Batch load all pinned nodes for these groups (single query)
        let pinned_map = self.batch_get_pinned_nodes(&group_ids).await?;

        // Convert rows to groups using the pre-loaded data
        let mut groups = Vec::with_capacity(rows.len());
        for row in rows {
            let group = self.row_to_group_with_data(row, &rules_map, &pinned_map)?;
            groups.push(group);
        }
        Ok(groups)
    }

    /// Get a node group by ID with its rules and pinned nodes
    pub async fn get_by_id(&self, organization_id: Uuid, id: Uuid) -> Result<Option<NodeGroup>> {
        let row = sqlx::query_as::<_, GroupRow>(
            r#"
            SELECT id, organization_id, name, description, parent_id, environment,
                   rule_match_type, classes, parameters, variables
            FROM node_groups
            WHERE organization_id = ? AND id = ?
            "#,
        )
        .bind(organization_id.to_string())
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
    pub async fn create(
        &self,
        organization_id: Uuid,
        req: &CreateGroupRequest,
    ) -> Result<NodeGroup> {
        let id = Uuid::new_v4();
        let rule_match_type = req
            .rule_match_type
            .unwrap_or(RuleMatchType::All)
            .to_string();
        // Classes are now in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
        let classes = serde_json::to_string(&req.classes.clone().unwrap_or(serde_json::json!({})))
            .unwrap_or_else(|_| "{}".to_string());
        let variables = req
            .variables
            .clone()
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());

        sqlx::query(
            r#"
            INSERT INTO node_groups (id, organization_id, name, description, parent_id, environment,
                                     rule_match_type, classes, parameters, variables)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(organization_id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(req.parent_id.map(|p| p.to_string()))
        .bind(&req.environment)
        .bind(&rule_match_type)
        .bind(&classes)
        .bind("{}") // parameters column kept for backward compatibility but now empty
        .bind(&variables)
        .execute(self.pool)
        .await
        .context("Failed to create group")?;

        self.get_by_id(organization_id, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created group"))
    }

    /// Update a node group
    pub async fn update(
        &self,
        organization_id: Uuid,
        id: Uuid,
        req: &UpdateGroupRequest,
    ) -> Result<Option<NodeGroup>> {
        // First check if the group exists
        let existing = self.get_by_id(organization_id, id).await?;
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
        // Classes are now in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
        let classes = req
            .classes
            .clone()
            .map(|c| serde_json::to_string(&c).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| {
                serde_json::to_string(&existing.classes).unwrap_or_else(|_| "{}".to_string())
            });
        let variables = req
            .variables
            .clone()
            .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| {
                serde_json::to_string(&existing.variables).unwrap_or_else(|_| "{}".to_string())
            });

        sqlx::query(
            r#"
            UPDATE node_groups
            SET name = ?, description = ?, parent_id = ?, environment = ?,
                rule_match_type = ?, classes = ?, parameters = ?, variables = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE organization_id = ? AND id = ?
            "#,
        )
        .bind(&name)
        .bind(&description)
        .bind(parent_id.map(|p| p.to_string()))
        .bind(&environment)
        .bind(&rule_match_type)
        .bind(&classes)
        .bind("{}") // parameters column kept for backward compatibility but now empty
        .bind(&variables)
        .bind(organization_id.to_string())
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update group")?;

        self.get_by_id(organization_id, id).await
    }

    /// Delete a node group (rules and pinned nodes are deleted via CASCADE)
    pub async fn delete(&self, organization_id: Uuid, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM node_groups WHERE organization_id = ? AND id = ?")
            .bind(organization_id.to_string())
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
        let result = sqlx::query("DELETE FROM pinned_nodes WHERE group_id = ? AND certname = ?")
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
        let organization_id =
            Uuid::parse_str(&row.organization_id).context("Invalid organization ID")?;

        let rules = self.get_rules(id).await?;
        let pinned_nodes = self.get_pinned_nodes(id).await?;

        // Parse classes - support both old array format and new object format
        let classes: serde_json::Value = serde_json::from_str(&row.classes)
            .map(|v: serde_json::Value| {
                // If it's an array (old format), convert to object format
                if let Some(arr) = v.as_array() {
                    let mut obj = serde_json::Map::new();
                    for class_name in arr {
                        if let Some(name) = class_name.as_str() {
                            obj.insert(name.to_string(), serde_json::json!({}));
                        }
                    }
                    serde_json::Value::Object(obj)
                } else {
                    v
                }
            })
            .unwrap_or(serde_json::json!({}));
        let variables: serde_json::Value =
            serde_json::from_str(&row.variables).unwrap_or(serde_json::json!({}));

        Ok(NodeGroup {
            id,
            organization_id,
            name: row.name,
            description: row.description,
            parent_id: row.parent_id.and_then(|p| Uuid::parse_str(&p).ok()),
            environment: row.environment,
            rule_match_type: parse_rule_match_type(&row.rule_match_type),
            classes,
            variables,
            rules,
            pinned_nodes,
        })
    }

    /// Convert a database row to a NodeGroup using pre-loaded rules and pinned nodes
    ///
    /// This is used by batch loading operations to avoid N+1 queries.
    fn row_to_group_with_data(
        &self,
        row: GroupRow,
        rules_map: &HashMap<String, Vec<ClassificationRule>>,
        pinned_map: &HashMap<String, Vec<String>>,
    ) -> Result<NodeGroup> {
        let id = Uuid::parse_str(&row.id).context("Invalid group ID")?;
        let organization_id =
            Uuid::parse_str(&row.organization_id).context("Invalid organization ID")?;

        let rules = rules_map.get(&row.id).cloned().unwrap_or_default();
        let pinned_nodes = pinned_map.get(&row.id).cloned().unwrap_or_default();

        // Parse classes - support both old array format and new object format
        let classes: serde_json::Value = serde_json::from_str(&row.classes)
            .map(|v: serde_json::Value| {
                // If it's an array (old format), convert to object format
                if let Some(arr) = v.as_array() {
                    let mut obj = serde_json::Map::new();
                    for class_name in arr {
                        if let Some(name) = class_name.as_str() {
                            obj.insert(name.to_string(), serde_json::json!({}));
                        }
                    }
                    serde_json::Value::Object(obj)
                } else {
                    v
                }
            })
            .unwrap_or(serde_json::json!({}));
        let variables: serde_json::Value =
            serde_json::from_str(&row.variables).unwrap_or(serde_json::json!({}));

        Ok(NodeGroup {
            id,
            organization_id,
            name: row.name,
            description: row.description,
            parent_id: row.parent_id.and_then(|p| Uuid::parse_str(&p).ok()),
            environment: row.environment,
            rule_match_type: parse_rule_match_type(&row.rule_match_type),
            classes,
            variables,
            rules,
            pinned_nodes,
        })
    }

    /// Batch load all rules for multiple groups in a single query
    ///
    /// This reduces N queries to 1 query for rule loading.
    async fn batch_get_rules(
        &self,
        group_ids: &[String],
    ) -> Result<HashMap<String, Vec<ClassificationRule>>> {
        if group_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Build the IN clause placeholders
        let placeholders: Vec<&str> = group_ids.iter().map(|_| "?").collect();
        let query = format!(
            r#"
            SELECT id, group_id, fact_path, operator, value
            FROM classification_rules
            WHERE group_id IN ({})
            ORDER BY group_id, fact_path
            "#,
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, RuleRowWithGroup>(&query);
        for id in group_ids {
            query_builder = query_builder.bind(id);
        }

        let rows = query_builder
            .fetch_all(self.pool)
            .await
            .context("Failed to batch fetch rules")?;

        // Group rules by group_id
        let mut rules_map: HashMap<String, Vec<ClassificationRule>> = HashMap::new();
        for row in rows {
            let rule = ClassificationRule {
                id: Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::new_v4()),
                fact_path: row.fact_path,
                operator: parse_operator(&row.operator),
                value: serde_json::from_str(&row.value).unwrap_or(serde_json::Value::Null),
            };
            rules_map
                .entry(row.group_id)
                .or_insert_with(Vec::new)
                .push(rule);
        }

        Ok(rules_map)
    }

    /// Batch load all pinned nodes for multiple groups in a single query
    ///
    /// This reduces N queries to 1 query for pinned node loading.
    async fn batch_get_pinned_nodes(
        &self,
        group_ids: &[String],
    ) -> Result<HashMap<String, Vec<String>>> {
        if group_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Build the IN clause placeholders
        let placeholders: Vec<&str> = group_ids.iter().map(|_| "?").collect();
        let query = format!(
            r#"
            SELECT group_id, certname
            FROM pinned_nodes
            WHERE group_id IN ({})
            ORDER BY group_id, certname
            "#,
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, PinnedNodeRowWithGroup>(&query);
        for id in group_ids {
            query_builder = query_builder.bind(id);
        }

        let rows = query_builder
            .fetch_all(self.pool)
            .await
            .context("Failed to batch fetch pinned nodes")?;

        // Group pinned nodes by group_id
        let mut pinned_map: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            pinned_map
                .entry(row.group_id)
                .or_insert_with(Vec::new)
                .push(row.certname);
        }

        Ok(pinned_map)
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

// ============================================================================
// Fact Template Repository
// ============================================================================

/// Row returned from fact_templates table
#[derive(Debug, sqlx::FromRow)]
struct FactTemplateRow {
    id: String,
    organization_id: String,
    name: String,
    description: Option<String>,
    facts: String,
}

/// Repository for fact template operations
pub struct FactTemplateRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> FactTemplateRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all fact templates
    pub async fn get_all(&self, organization_id: Uuid) -> Result<Vec<FactTemplate>> {
        let rows = sqlx::query_as::<_, FactTemplateRow>(
            r#"
            SELECT id, organization_id, name, description, facts
            FROM fact_templates
            WHERE organization_id = ?
            ORDER BY name
            "#,
        )
        .bind(organization_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch fact templates")?;

        Ok(rows.into_iter().map(row_to_template).collect())
    }

    /// Get a fact template by ID
    pub async fn get_by_id(&self, organization_id: Uuid, id: Uuid) -> Result<Option<FactTemplate>> {
        let row = sqlx::query_as::<_, FactTemplateRow>(
            r#"
            SELECT id, organization_id, name, description, facts
            FROM fact_templates
            WHERE organization_id = ? AND id = ?
            "#,
        )
        .bind(organization_id.to_string())
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch fact template")?;

        Ok(row.map(row_to_template))
    }

    /// Get a fact template by name
    pub async fn get_by_name(
        &self,
        organization_id: Uuid,
        name: &str,
    ) -> Result<Option<FactTemplate>> {
        let row = sqlx::query_as::<_, FactTemplateRow>(
            r#"
            SELECT id, organization_id, name, description, facts
            FROM fact_templates
            WHERE organization_id = ? AND name = ?
            "#,
        )
        .bind(organization_id.to_string())
        .bind(name)
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch fact template")?;

        Ok(row.map(row_to_template))
    }

    /// Create a new fact template
    pub async fn create(
        &self,
        organization_id: Uuid,
        name: &str,
        description: Option<&str>,
        facts: &[FactDefinition],
    ) -> Result<FactTemplate> {
        let id = Uuid::new_v4();
        let facts_json = serde_json::to_string(facts).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO fact_templates (id, organization_id, name, description, facts)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(organization_id.to_string())
        .bind(name)
        .bind(description)
        .bind(&facts_json)
        .execute(self.pool)
        .await
        .context("Failed to create fact template")?;

        Ok(FactTemplate {
            id: Some(id.to_string()),
            organization_id,
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            facts: facts.to_vec(),
        })
    }

    /// Update a fact template
    pub async fn update(
        &self,
        organization_id: Uuid,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        facts: Option<&[FactDefinition]>,
    ) -> Result<Option<FactTemplate>> {
        // First check if template exists
        let existing = self.get_by_id(organization_id, id).await?;
        if existing.is_none() {
            return Ok(None);
        }
        let existing = existing.unwrap();

        let new_name = name.unwrap_or(&existing.name);
        let new_description = description.or(existing.description.as_deref());
        let new_facts = facts.unwrap_or(&existing.facts);
        let facts_json = serde_json::to_string(new_facts).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            UPDATE fact_templates
            SET name = ?, description = ?, facts = ?, updated_at = CURRENT_TIMESTAMP
            WHERE organization_id = ? AND id = ?
            "#,
        )
        .bind(new_name)
        .bind(new_description)
        .bind(&facts_json)
        .bind(organization_id.to_string())
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update fact template")?;

        self.get_by_id(organization_id, id).await
    }

    /// Delete a fact template
    pub async fn delete(&self, organization_id: Uuid, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM fact_templates WHERE organization_id = ? AND id = ?")
            .bind(organization_id.to_string())
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete fact template")?;

        Ok(result.rows_affected() > 0)
    }
}

/// Convert a database row to a FactTemplate
fn row_to_template(row: FactTemplateRow) -> FactTemplate {
    let facts: Vec<FactDefinition> = serde_json::from_str(&row.facts).unwrap_or_default();

    FactTemplate {
        id: Some(row.id),
        organization_id: Uuid::parse_str(&row.organization_id)
            .unwrap_or_else(|_| crate::models::default_organization_uuid()),
        name: row.name,
        description: row.description,
        facts,
    }
}

// ============================================================================
// Saved Reports Repository
// ============================================================================

use crate::models::{
    ComplianceBaseline, ComplianceRule, CreateComplianceBaselineRequest,
    CreateDriftBaselineRequest, CreateSavedReportRequest, CreateScheduleRequest, DriftBaseline,
    DriftToleranceConfig, ExecutionStatus, OutputFormat, ReportExecution, ReportQueryConfig,
    ReportSchedule, ReportTemplate, ReportType, SavedReport, SeverityLevel,
    UpdateSavedReportRequest, UpdateScheduleRequest,
};
use chrono::{DateTime, Utc};

/// Row returned from saved_reports table
#[derive(Debug, sqlx::FromRow)]
struct SavedReportRow {
    id: String,
    name: String,
    description: Option<String>,
    report_type: String,
    query_config: String,
    created_by: String,
    is_public: bool,
    created_at: String,
    updated_at: String,
}

/// Row returned from report_schedules table
#[derive(Debug, sqlx::FromRow)]
struct ReportScheduleRow {
    id: String,
    report_id: String,
    schedule_cron: String,
    timezone: String,
    is_enabled: bool,
    output_format: String,
    email_recipients: Option<String>,
    last_run_at: Option<String>,
    next_run_at: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Row returned from report_executions table
#[derive(Debug, sqlx::FromRow)]
struct ReportExecutionRow {
    id: String,
    report_id: String,
    schedule_id: Option<String>,
    executed_by: Option<String>,
    status: String,
    started_at: String,
    completed_at: Option<String>,
    row_count: Option<i32>,
    output_format: String,
    output_data: Option<String>,
    output_file_path: Option<String>,
    error_message: Option<String>,
    execution_time_ms: Option<i32>,
}

/// Row returned from compliance_baselines table
#[derive(Debug, sqlx::FromRow)]
struct ComplianceBaselineRow {
    id: String,
    name: String,
    description: Option<String>,
    rules: String,
    severity_level: String,
    created_by: String,
    created_at: String,
    updated_at: String,
}

/// Row returned from drift_baselines table
#[derive(Debug, sqlx::FromRow)]
struct DriftBaselineRow {
    id: String,
    name: String,
    description: Option<String>,
    node_group_id: Option<String>,
    baseline_facts: String,
    tolerance_config: Option<String>,
    created_by: String,
    created_at: String,
    updated_at: String,
}

/// Row returned from report_templates table
#[derive(Debug, sqlx::FromRow)]
struct ReportTemplateRow {
    id: String,
    name: String,
    description: Option<String>,
    report_type: String,
    query_config: String,
    is_system: bool,
    created_at: String,
}

/// Repository for saved reports operations
pub struct SavedReportRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> SavedReportRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all saved reports
    pub async fn get_all(&self) -> Result<Vec<SavedReport>> {
        let rows = sqlx::query_as::<_, SavedReportRow>(
            r#"
            SELECT id, name, description, report_type, query_config,
                   created_by, is_public, created_at, updated_at
            FROM saved_reports
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch saved reports")?;

        Ok(rows.into_iter().map(row_to_saved_report).collect())
    }

    /// Get saved reports by user (including public reports)
    pub async fn get_by_user(&self, user_id: Uuid) -> Result<Vec<SavedReport>> {
        let rows = sqlx::query_as::<_, SavedReportRow>(
            r#"
            SELECT id, name, description, report_type, query_config,
                   created_by, is_public, created_at, updated_at
            FROM saved_reports
            WHERE created_by = ? OR is_public = TRUE
            ORDER BY name
            "#,
        )
        .bind(user_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch saved reports")?;

        Ok(rows.into_iter().map(row_to_saved_report).collect())
    }

    /// Get saved reports by type
    pub async fn get_by_type(&self, report_type: ReportType) -> Result<Vec<SavedReport>> {
        let rows = sqlx::query_as::<_, SavedReportRow>(
            r#"
            SELECT id, name, description, report_type, query_config,
                   created_by, is_public, created_at, updated_at
            FROM saved_reports
            WHERE report_type = ?
            ORDER BY name
            "#,
        )
        .bind(report_type.as_str())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch saved reports")?;

        Ok(rows.into_iter().map(row_to_saved_report).collect())
    }

    /// Get a saved report by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<SavedReport>> {
        let row = sqlx::query_as::<_, SavedReportRow>(
            r#"
            SELECT id, name, description, report_type, query_config,
                   created_by, is_public, created_at, updated_at
            FROM saved_reports
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch saved report")?;

        Ok(row.map(row_to_saved_report))
    }

    /// Create a new saved report
    pub async fn create(
        &self,
        req: &CreateSavedReportRequest,
        user_id: Uuid,
    ) -> Result<SavedReport> {
        let id = Uuid::new_v4();
        let query_config =
            serde_json::to_string(&req.query_config).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            INSERT INTO saved_reports (id, name, description, report_type, query_config, created_by, is_public)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(req.report_type.as_str())
        .bind(&query_config)
        .bind(user_id.to_string())
        .bind(req.is_public)
        .execute(self.pool)
        .await
        .context("Failed to create saved report")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created report"))
    }

    /// Update a saved report
    pub async fn update(
        &self,
        id: Uuid,
        req: &UpdateSavedReportRequest,
    ) -> Result<Option<SavedReport>> {
        let existing = self.get_by_id(id).await?;
        if existing.is_none() {
            return Ok(None);
        }

        let existing = existing.unwrap();
        let name = req.name.as_ref().unwrap_or(&existing.name);
        let description = req.description.as_ref().or(existing.description.as_ref());
        let query_config = req.query_config.as_ref().unwrap_or(&existing.query_config);
        let is_public = req.is_public.unwrap_or(existing.is_public);
        let query_config_str =
            serde_json::to_string(query_config).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            UPDATE saved_reports
            SET name = ?, description = ?, query_config = ?, is_public = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(&query_config_str)
        .bind(is_public)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update saved report")?;

        self.get_by_id(id).await
    }

    /// Delete a saved report
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM saved_reports WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete saved report")?;

        Ok(result.rows_affected() > 0)
    }
}

fn row_to_saved_report(row: SavedReportRow) -> SavedReport {
    let query_config: ReportQueryConfig =
        serde_json::from_str(&row.query_config).unwrap_or_default();

    SavedReport {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        description: row.description,
        report_type: ReportType::from_str(&row.report_type).unwrap_or_default(),
        query_config,
        created_by: Uuid::parse_str(&row.created_by).unwrap_or_default(),
        is_public: row.is_public,
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Report Schedule Repository
// ============================================================================

/// Repository for report schedule operations
pub struct ReportScheduleRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ReportScheduleRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all schedules
    pub async fn get_all(&self) -> Result<Vec<ReportSchedule>> {
        let rows = sqlx::query_as::<_, ReportScheduleRow>(
            r#"
            SELECT id, report_id, schedule_cron, timezone, is_enabled,
                   output_format, email_recipients, last_run_at, next_run_at,
                   created_at, updated_at
            FROM report_schedules
            ORDER BY next_run_at
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch schedules")?;

        Ok(rows.into_iter().map(row_to_schedule).collect())
    }

    /// Get all enabled schedules that are due to run now
    pub async fn get_due(&self) -> Result<Vec<ReportSchedule>> {
        self.get_due_schedules(Utc::now()).await
    }

    /// Get enabled schedules that are due to run
    pub async fn get_due_schedules(&self, before: DateTime<Utc>) -> Result<Vec<ReportSchedule>> {
        let rows = sqlx::query_as::<_, ReportScheduleRow>(
            r#"
            SELECT id, report_id, schedule_cron, timezone, is_enabled,
                   output_format, email_recipients, last_run_at, next_run_at,
                   created_at, updated_at
            FROM report_schedules
            WHERE is_enabled = TRUE AND next_run_at <= ?
            ORDER BY next_run_at
            "#,
        )
        .bind(before.to_rfc3339())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch due schedules")?;

        Ok(rows.into_iter().map(row_to_schedule).collect())
    }

    /// Get schedules for a report
    pub async fn get_by_report(&self, report_id: Uuid) -> Result<Vec<ReportSchedule>> {
        let rows = sqlx::query_as::<_, ReportScheduleRow>(
            r#"
            SELECT id, report_id, schedule_cron, timezone, is_enabled,
                   output_format, email_recipients, last_run_at, next_run_at,
                   created_at, updated_at
            FROM report_schedules
            WHERE report_id = ?
            "#,
        )
        .bind(report_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch schedules")?;

        Ok(rows.into_iter().map(row_to_schedule).collect())
    }

    /// Get a schedule by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<ReportSchedule>> {
        let row = sqlx::query_as::<_, ReportScheduleRow>(
            r#"
            SELECT id, report_id, schedule_cron, timezone, is_enabled,
                   output_format, email_recipients, last_run_at, next_run_at,
                   created_at, updated_at
            FROM report_schedules
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch schedule")?;

        Ok(row.map(row_to_schedule))
    }

    /// Create a new schedule
    pub async fn create(&self, req: &CreateScheduleRequest) -> Result<ReportSchedule> {
        let id = Uuid::new_v4();
        let email_recipients = req
            .email_recipients
            .as_ref()
            .map(|r| serde_json::to_string(r).unwrap_or_else(|_| "[]".to_string()));

        sqlx::query(
            r#"
            INSERT INTO report_schedules (id, report_id, schedule_cron, timezone, is_enabled, output_format, email_recipients)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(req.report_id.to_string())
        .bind(&req.schedule_cron)
        .bind(&req.timezone)
        .bind(req.is_enabled)
        .bind(req.output_format.as_str())
        .bind(&email_recipients)
        .execute(self.pool)
        .await
        .context("Failed to create schedule")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created schedule"))
    }

    /// Update a schedule
    pub async fn update(
        &self,
        id: Uuid,
        req: &UpdateScheduleRequest,
    ) -> Result<Option<ReportSchedule>> {
        let existing = self.get_by_id(id).await?;
        if existing.is_none() {
            return Ok(None);
        }

        let existing = existing.unwrap();
        let schedule_cron = req
            .schedule_cron
            .as_ref()
            .unwrap_or(&existing.schedule_cron);
        let timezone = req.timezone.as_ref().unwrap_or(&existing.timezone);
        let is_enabled = req.is_enabled.unwrap_or(existing.is_enabled);
        let output_format = req.output_format.unwrap_or(existing.output_format);
        let email_recipients = req
            .email_recipients
            .as_ref()
            .or(existing.email_recipients.as_ref())
            .map(|r| serde_json::to_string(r).unwrap_or_else(|_| "[]".to_string()));

        sqlx::query(
            r#"
            UPDATE report_schedules
            SET schedule_cron = ?, timezone = ?, is_enabled = ?, output_format = ?,
                email_recipients = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(schedule_cron)
        .bind(timezone)
        .bind(is_enabled)
        .bind(output_format.as_str())
        .bind(&email_recipients)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update schedule")?;

        self.get_by_id(id).await
    }

    /// Update next run time
    pub async fn update_run_times(
        &self,
        id: Uuid,
        last_run: DateTime<Utc>,
        next_run: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE report_schedules
            SET last_run_at = ?, next_run_at = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(last_run.to_rfc3339())
        .bind(next_run.map(|t| t.to_rfc3339()))
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update schedule run times")?;

        Ok(())
    }

    /// Delete a schedule
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM report_schedules WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete schedule")?;

        Ok(result.rows_affected() > 0)
    }
}

fn row_to_schedule(row: ReportScheduleRow) -> ReportSchedule {
    let email_recipients: Option<Vec<String>> = row
        .email_recipients
        .and_then(|s| serde_json::from_str(&s).ok());

    ReportSchedule {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        report_id: Uuid::parse_str(&row.report_id).unwrap_or_default(),
        schedule_cron: row.schedule_cron,
        timezone: row.timezone,
        is_enabled: row.is_enabled,
        output_format: OutputFormat::from_str(&row.output_format).unwrap_or_default(),
        email_recipients,
        last_run_at: row.last_run_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        next_run_at: row.next_run_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Report Execution Repository
// ============================================================================

/// Repository for report execution operations
pub struct ReportExecutionRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ReportExecutionRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get executions for a report
    pub async fn get_by_report(
        &self,
        report_id: Uuid,
        limit: Option<u32>,
    ) -> Result<Vec<ReportExecution>> {
        let limit = limit.unwrap_or(100);
        let rows = sqlx::query_as::<_, ReportExecutionRow>(
            r#"
            SELECT id, report_id, schedule_id, executed_by, status, started_at,
                   completed_at, row_count, output_format, output_data,
                   output_file_path, error_message, execution_time_ms
            FROM report_executions
            WHERE report_id = ?
            ORDER BY started_at DESC
            LIMIT ?
            "#,
        )
        .bind(report_id.to_string())
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch executions")?;

        Ok(rows.into_iter().map(row_to_execution).collect())
    }

    /// Get an execution by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<ReportExecution>> {
        let row = sqlx::query_as::<_, ReportExecutionRow>(
            r#"
            SELECT id, report_id, schedule_id, executed_by, status, started_at,
                   completed_at, row_count, output_format, output_data,
                   output_file_path, error_message, execution_time_ms
            FROM report_executions
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch execution")?;

        Ok(row.map(row_to_execution))
    }

    /// Create a new execution
    pub async fn create(
        &self,
        report_id: Uuid,
        schedule_id: Option<Uuid>,
        executed_by: Option<Uuid>,
        output_format: OutputFormat,
    ) -> Result<ReportExecution> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO report_executions (id, report_id, schedule_id, executed_by, status, output_format)
            VALUES (?, ?, ?, ?, 'pending', ?)
            "#,
        )
        .bind(id.to_string())
        .bind(report_id.to_string())
        .bind(schedule_id.map(|s| s.to_string()))
        .bind(executed_by.map(|u| u.to_string()))
        .bind(output_format.as_str())
        .execute(self.pool)
        .await
        .context("Failed to create execution")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created execution"))
    }

    /// Update execution status to running
    pub async fn mark_running(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE report_executions SET status = 'running' WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to update execution status")?;

        Ok(())
    }

    /// Complete an execution successfully
    pub async fn complete(
        &self,
        id: Uuid,
        row_count: i32,
        output_data: Option<serde_json::Value>,
        output_file_path: Option<&str>,
        execution_time_ms: i32,
    ) -> Result<()> {
        let output_data_str = output_data.map(|d| serde_json::to_string(&d).unwrap_or_default());

        sqlx::query(
            r#"
            UPDATE report_executions
            SET status = 'completed', completed_at = CURRENT_TIMESTAMP,
                row_count = ?, output_data = ?, output_file_path = ?,
                execution_time_ms = ?
            WHERE id = ?
            "#,
        )
        .bind(row_count)
        .bind(&output_data_str)
        .bind(output_file_path)
        .bind(execution_time_ms)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to complete execution")?;

        Ok(())
    }

    /// Fail an execution
    pub async fn fail(&self, id: Uuid, error_message: &str, execution_time_ms: i32) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE report_executions
            SET status = 'failed', completed_at = CURRENT_TIMESTAMP,
                error_message = ?, execution_time_ms = ?
            WHERE id = ?
            "#,
        )
        .bind(error_message)
        .bind(execution_time_ms)
        .bind(id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to update execution")?;

        Ok(())
    }

    /// Delete old executions
    pub async fn delete_old(&self, older_than: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query("DELETE FROM report_executions WHERE started_at < ?")
            .bind(older_than.to_rfc3339())
            .execute(self.pool)
            .await
            .context("Failed to delete old executions")?;

        Ok(result.rows_affected())
    }
}

fn row_to_execution(row: ReportExecutionRow) -> ReportExecution {
    let output_data: Option<serde_json::Value> =
        row.output_data.and_then(|s| serde_json::from_str(&s).ok());

    ReportExecution {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        report_id: Uuid::parse_str(&row.report_id).unwrap_or_default(),
        schedule_id: row.schedule_id.and_then(|s| Uuid::parse_str(&s).ok()),
        executed_by: row.executed_by.and_then(|s| Uuid::parse_str(&s).ok()),
        status: ExecutionStatus::from_str(&row.status).unwrap_or_default(),
        started_at: DateTime::parse_from_rfc3339(&row.started_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        completed_at: row.completed_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        row_count: row.row_count,
        output_format: OutputFormat::from_str(&row.output_format).unwrap_or_default(),
        output_data,
        output_file_path: row.output_file_path,
        error_message: row.error_message,
        execution_time_ms: row.execution_time_ms,
    }
}

// ============================================================================
// Compliance Baseline Repository
// ============================================================================

/// Repository for compliance baseline operations
pub struct ComplianceBaselineRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ComplianceBaselineRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all baselines
    pub async fn get_all(&self) -> Result<Vec<ComplianceBaseline>> {
        let rows = sqlx::query_as::<_, ComplianceBaselineRow>(
            r#"
            SELECT id, name, description, rules, severity_level, created_by, created_at, updated_at
            FROM compliance_baselines
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch compliance baselines")?;

        Ok(rows.into_iter().map(row_to_compliance_baseline).collect())
    }

    /// Get a baseline by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<ComplianceBaseline>> {
        let row = sqlx::query_as::<_, ComplianceBaselineRow>(
            r#"
            SELECT id, name, description, rules, severity_level, created_by, created_at, updated_at
            FROM compliance_baselines
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch compliance baseline")?;

        Ok(row.map(row_to_compliance_baseline))
    }

    /// Create a new baseline
    pub async fn create(
        &self,
        req: &CreateComplianceBaselineRequest,
        user_id: Uuid,
    ) -> Result<ComplianceBaseline> {
        let id = Uuid::new_v4();
        let rules_json = serde_json::to_string(&req.rules).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"
            INSERT INTO compliance_baselines (id, name, description, rules, severity_level, created_by)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(&rules_json)
        .bind(req.severity_level.as_str())
        .bind(user_id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to create compliance baseline")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created baseline"))
    }

    /// Delete a baseline
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM compliance_baselines WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete compliance baseline")?;

        Ok(result.rows_affected() > 0)
    }
}

fn row_to_compliance_baseline(row: ComplianceBaselineRow) -> ComplianceBaseline {
    let rules: Vec<ComplianceRule> = serde_json::from_str(&row.rules).unwrap_or_default();

    ComplianceBaseline {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        description: row.description,
        rules,
        severity_level: SeverityLevel::from_str(&row.severity_level).unwrap_or_default(),
        created_by: Uuid::parse_str(&row.created_by).unwrap_or_default(),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Drift Baseline Repository
// ============================================================================

/// Repository for drift baseline operations
pub struct DriftBaselineRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> DriftBaselineRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all baselines
    pub async fn get_all(&self) -> Result<Vec<DriftBaseline>> {
        let rows = sqlx::query_as::<_, DriftBaselineRow>(
            r#"
            SELECT id, name, description, node_group_id, baseline_facts,
                   tolerance_config, created_by, created_at, updated_at
            FROM drift_baselines
            ORDER BY name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch drift baselines")?;

        Ok(rows.into_iter().map(row_to_drift_baseline).collect())
    }

    /// Get a baseline by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<DriftBaseline>> {
        let row = sqlx::query_as::<_, DriftBaselineRow>(
            r#"
            SELECT id, name, description, node_group_id, baseline_facts,
                   tolerance_config, created_by, created_at, updated_at
            FROM drift_baselines
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch drift baseline")?;

        Ok(row.map(row_to_drift_baseline))
    }

    /// Get baselines by node group
    pub async fn get_by_group(&self, group_id: Uuid) -> Result<Vec<DriftBaseline>> {
        let rows = sqlx::query_as::<_, DriftBaselineRow>(
            r#"
            SELECT id, name, description, node_group_id, baseline_facts,
                   tolerance_config, created_by, created_at, updated_at
            FROM drift_baselines
            WHERE node_group_id = ?
            "#,
        )
        .bind(group_id.to_string())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch drift baselines")?;

        Ok(rows.into_iter().map(row_to_drift_baseline).collect())
    }

    /// Create a new baseline
    pub async fn create(
        &self,
        req: &CreateDriftBaselineRequest,
        user_id: Uuid,
    ) -> Result<DriftBaseline> {
        let id = Uuid::new_v4();
        let baseline_facts_json =
            serde_json::to_string(&req.baseline_facts).unwrap_or_else(|_| "{}".to_string());
        let tolerance_config_json = req
            .tolerance_config
            .as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_else(|_| "{}".to_string()));

        sqlx::query(
            r#"
            INSERT INTO drift_baselines (id, name, description, node_group_id, baseline_facts, tolerance_config, created_by)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(req.node_group_id.map(|g| g.to_string()))
        .bind(&baseline_facts_json)
        .bind(&tolerance_config_json)
        .bind(user_id.to_string())
        .execute(self.pool)
        .await
        .context("Failed to create drift baseline")?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created baseline"))
    }

    /// Delete a baseline
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM drift_baselines WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await
            .context("Failed to delete drift baseline")?;

        Ok(result.rows_affected() > 0)
    }
}

fn row_to_drift_baseline(row: DriftBaselineRow) -> DriftBaseline {
    let baseline_facts: serde_json::Value = serde_json::from_str(&row.baseline_facts)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let tolerance_config: Option<DriftToleranceConfig> = row
        .tolerance_config
        .and_then(|s| serde_json::from_str(&s).ok());

    DriftBaseline {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        description: row.description,
        node_group_id: row.node_group_id.and_then(|s| Uuid::parse_str(&s).ok()),
        baseline_facts,
        tolerance_config,
        created_by: Uuid::parse_str(&row.created_by).unwrap_or_default(),
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

// ============================================================================
// Report Template Repository
// ============================================================================

/// Repository for report template operations
pub struct ReportTemplateRepository<'a> {
    pool: &'a SqlitePool,
}

impl<'a> ReportTemplateRepository<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all templates
    pub async fn get_all(&self) -> Result<Vec<ReportTemplate>> {
        let rows = sqlx::query_as::<_, ReportTemplateRow>(
            r#"
            SELECT id, name, description, report_type, query_config, is_system, created_at
            FROM report_templates
            ORDER BY is_system DESC, name
            "#,
        )
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch report templates")?;

        Ok(rows.into_iter().map(row_to_template_report).collect())
    }

    /// Get templates by type
    pub async fn get_by_type(&self, report_type: ReportType) -> Result<Vec<ReportTemplate>> {
        let rows = sqlx::query_as::<_, ReportTemplateRow>(
            r#"
            SELECT id, name, description, report_type, query_config, is_system, created_at
            FROM report_templates
            WHERE report_type = ?
            ORDER BY is_system DESC, name
            "#,
        )
        .bind(report_type.as_str())
        .fetch_all(self.pool)
        .await
        .context("Failed to fetch report templates")?;

        Ok(rows.into_iter().map(row_to_template_report).collect())
    }

    /// Get a template by ID
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<ReportTemplate>> {
        let row = sqlx::query_as::<_, ReportTemplateRow>(
            r#"
            SELECT id, name, description, report_type, query_config, is_system, created_at
            FROM report_templates
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await
        .context("Failed to fetch report template")?;

        Ok(row.map(row_to_template_report))
    }
}

fn row_to_template_report(row: ReportTemplateRow) -> ReportTemplate {
    let query_config: ReportQueryConfig =
        serde_json::from_str(&row.query_config).unwrap_or_default();

    ReportTemplate {
        id: Uuid::parse_str(&row.id).unwrap_or_default(),
        name: row.name,
        description: row.description,
        report_type: ReportType::from_str(&row.report_type).unwrap_or_default(),
        query_config,
        is_system: row.is_system,
        created_at: DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
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
