//! Reporting and analytics service
//!
//! This service handles report generation, execution, and export functionality.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use printpdf::{
    BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Pt, TextItem,
};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::repository::{
    ComplianceBaselineRepository, DriftBaselineRepository, ReportExecutionRepository,
};
use crate::models::{
    ChangeSummary, ChangeTrackingReport, ChangeTypeBreakdown, ComplianceReport, ComplianceSummary,
    ComplianceViolation, DriftReport, DriftSummary, DriftedFact, DriftedNode, EnvironmentHealth,
    ExecuteReportRequest, NodeHealthDetail, NodeHealthReport, NodeHealthSummary, OutputFormat,
    ReportExecution, ReportQueryConfig, ReportResult, ReportType, SavedReport, SeverityBreakdown,
    SeverityLevel,
};
use crate::models::{DatasetQueryRequest, EventActor, PluginReportResult, ReportSection};
use crate::services::plugins::events::EventBus;
use crate::services::plugins::reports::DatasetReader;
use crate::services::plugins::PluginManager;
use crate::services::PuppetDbClient;

/// Service for generating and executing reports
#[derive(Clone)]
pub struct ReportingService {
    pool: SqlitePool,
    puppetdb: Option<Arc<PuppetDbClient>>,
    /// Plugin report types and datasets (M25); `None` where no plugin
    /// runtime exists, and then plugin report types fail.
    plugins: Option<Arc<PluginManager>>,
    /// Run plugin report executions as background jobs (API requests)
    /// instead of waiting for them (scheduled runs).
    background: bool,
}

/// Who asked for a report, for the `report.generated` event.
fn requester(user_id: Option<Uuid>) -> String {
    user_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| "schedule".to_string())
}

impl ReportingService {
    pub fn new(pool: SqlitePool, puppetdb: Option<Arc<PuppetDbClient>>) -> Self {
        Self {
            pool,
            puppetdb,
            plugins: None,
            background: false,
        }
    }

    /// Enables plugin report types, export formats and datasets.
    pub fn with_plugins(mut self, plugins: Arc<PluginManager>) -> Self {
        self.plugins = Some(plugins);
        self
    }

    /// Plugin report executions return at once and complete in the background.
    pub fn in_background(mut self) -> Self {
        self.background = true;
        self
    }

    /// Execute a saved report
    ///
    /// Core report types run to completion before returning. A plugin report
    /// type runs as a background job when the service was built with
    /// [`in_background`](Self::in_background): the execution is returned as
    /// `running` and completed (or failed) by the job.
    pub async fn execute_report(
        &self,
        report: &SavedReport,
        req: &ExecuteReportRequest,
        user_id: Option<Uuid>,
    ) -> Result<ReportExecution> {
        let exec_repo = ReportExecutionRepository::new(&self.pool);

        // Create execution record
        let mut execution = exec_repo
            .create(report.id, None, user_id, req.output_format)
            .await?;

        // Mark as running
        exec_repo.mark_running(execution.id).await?;

        // Use override config if provided, otherwise use report's config
        let config = req
            .query_config_override
            .clone()
            .unwrap_or_else(|| report.query_config.clone());

        if self.background && matches!(report.report_type, ReportType::Plugin { .. }) {
            let svc = self.clone();
            let report = report.clone();
            let id = execution.id;
            tokio::spawn(async move {
                if let Err(e) = svc.run_execution(id, &report, &config, user_id).await {
                    tracing::error!("plugin report execution {} could not be stored: {}", id, e);
                }
            });
            execution.status = crate::models::ExecutionStatus::Running;
            return Ok(execution);
        }

        let (status, row_count, output_data, error, execution_time_ms) = self
            .run_execution(execution.id, report, &config, user_id)
            .await?;
        execution.status = status;
        if status == crate::models::ExecutionStatus::Completed {
            execution.output_data = output_data;
            execution.row_count = Some(row_count);
        } else {
            execution.error_message = error;
        }
        execution.execution_time_ms = Some(execution_time_ms);
        Ok(execution)
    }

    /// Generates the report of one execution and stores its outcome.
    #[allow(clippy::type_complexity)]
    async fn run_execution(
        &self,
        execution_id: Uuid,
        report: &SavedReport,
        config: &ReportQueryConfig,
        user_id: Option<Uuid>,
    ) -> Result<(
        crate::models::ExecutionStatus,
        i32,
        Option<serde_json::Value>,
        Option<String>,
        i32,
    )> {
        let exec_repo = ReportExecutionRepository::new(&self.pool);
        let start_time = std::time::Instant::now();
        let result = self
            .generate_report_as(&report.report_type, config, &requester(user_id))
            .await;
        let execution_time_ms = start_time.elapsed().as_millis() as i32;
        let outcome = match result {
            Ok((report_result, row_count)) => {
                let output_data = serde_json::to_value(&report_result).ok();
                exec_repo
                    .complete(
                        execution_id,
                        row_count,
                        output_data.clone(),
                        None,
                        execution_time_ms,
                    )
                    .await?;
                (
                    crate::models::ExecutionStatus::Completed,
                    row_count,
                    output_data,
                    None,
                    execution_time_ms,
                )
            }
            Err(e) => {
                exec_repo
                    .fail(execution_id, &e.to_string(), execution_time_ms)
                    .await?;
                (
                    crate::models::ExecutionStatus::Failed,
                    0,
                    None,
                    Some(e.to_string()),
                    execution_time_ms,
                )
            }
        };
        self.emit_generated(
            &report.report_type,
            config,
            Some(report.id),
            Some(execution_id),
            &requester(user_id),
            outcome.0.as_str(),
            outcome.1,
            execution_time_ms,
        )
        .await;
        Ok(outcome)
    }

    /// Publishes `report.generated` (M25); never fails the report.
    #[allow(clippy::too_many_arguments)]
    async fn emit_generated(
        &self,
        report_type: &ReportType,
        config: &ReportQueryConfig,
        report_id: Option<Uuid>,
        execution_id: Option<Uuid>,
        requested_by: &str,
        status: &str,
        row_count: i32,
        duration_ms: i32,
    ) {
        EventBus::new(self.pool.clone())
            .emit(
                "report.generated",
                serde_json::json!({
                    "report_type": report_type.id(),
                    "report_id": report_id,
                    "execution_id": execution_id,
                    "parameters": config,
                    "status": status,
                    "row_count": row_count,
                    "duration_ms": duration_ms,
                    "requested_by": requested_by,
                }),
                EventActor::system(),
            )
            .await;
    }

    /// On-demand generation (no saved report) that also publishes
    /// `report.generated`.
    pub async fn generate_on_demand(
        &self,
        report_type: &ReportType,
        config: &ReportQueryConfig,
        user_id: Option<Uuid>,
    ) -> Result<(ReportResult, i32)> {
        let started = std::time::Instant::now();
        let result = self
            .generate_report_as(report_type, config, &requester(user_id))
            .await;
        let (status, rows) = match &result {
            Ok((_, n)) => ("completed", *n),
            Err(_) => ("failed", 0),
        };
        self.emit_generated(
            report_type,
            config,
            None,
            None,
            &requester(user_id),
            status,
            rows,
            started.elapsed().as_millis() as i32,
        )
        .await;
        result
    }

    /// Generate a report based on type and configuration
    pub async fn generate_report(
        &self,
        report_type: &ReportType,
        config: &ReportQueryConfig,
    ) -> Result<(ReportResult, i32)> {
        self.generate_report_as(report_type, config, "system").await
    }

    async fn generate_report_as(
        &self,
        report_type: &ReportType,
        config: &ReportQueryConfig,
        requested_by: &str,
    ) -> Result<(ReportResult, i32)> {
        match report_type {
            ReportType::NodeHealth => {
                let report = self.generate_node_health_report(config).await?;
                let row_count = report.summary.total_nodes as i32;
                Ok((ReportResult::NodeHealth(report), row_count))
            }
            ReportType::Compliance => {
                let report = self.generate_compliance_report(config).await?;
                let row_count = report.violations.len() as i32;
                Ok((ReportResult::Compliance(report), row_count))
            }
            ReportType::ChangeTracking => {
                let report = self.generate_change_tracking_report(config).await?;
                let row_count = report.changes.len() as i32;
                Ok((ReportResult::ChangeTracking(report), row_count))
            }
            ReportType::DriftDetection => {
                let report = self.generate_drift_report(config).await?;
                let row_count = report.drifted_nodes.len() as i32;
                Ok((ReportResult::DriftDetection(report), row_count))
            }
            ReportType::Custom => {
                // A custom report reads a plugin dataset when one is chosen
                // (M25); otherwise it stays an empty object.
                let dataset = config
                    .custom_params
                    .as_ref()
                    .and_then(|p| p.get("dataset"))
                    .and_then(serde_json::Value::as_str);
                match (dataset, &self.plugins) {
                    (Some(dataset), Some(plugins)) => {
                        let p = config.custom_params.clone().unwrap_or_default();
                        let req = DatasetQueryRequest {
                            dataset: dataset.to_string(),
                            filter: p
                                .get("filter")
                                .and_then(serde_json::Value::as_object)
                                .cloned()
                                .unwrap_or_default(),
                            limit: p
                                .get("limit")
                                .and_then(serde_json::Value::as_u64)
                                .map(|l| l.min(u32::MAX as u64) as u32),
                            offset: None,
                        };
                        let r = plugins
                            .query_dataset(DatasetReader::System, &req)
                            .await
                            .map_err(anyhow::Error::new)?;
                        let rows = r.rows.len() as i32;
                        Ok((ReportResult::Custom(serde_json::to_value(&r)?), rows))
                    }
                    (Some(_), None) => Err(anyhow::anyhow!(
                        "plugin datasets are not available in this process"
                    )),
                    (None, _) => Ok((ReportResult::Custom(serde_json::json!({})), 0)),
                }
            }
            ReportType::Plugin { plugin_id, type_id } => {
                let plugins = self.plugins.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("plugin report types are not available in this process")
                })?;
                let params = config
                    .custom_params
                    .clone()
                    .unwrap_or(serde_json::Value::Null);
                let (result, rows) = plugins
                    .generate_plugin_report(plugin_id, type_id, &params, requested_by)
                    .await
                    .map_err(anyhow::Error::new)?;
                Ok((
                    ReportResult::Plugin(result),
                    rows.min(i32::MAX as u64) as i32,
                ))
            }
        }
    }

    /// Generate node health report
    async fn generate_node_health_report(
        &self,
        config: &ReportQueryConfig,
    ) -> Result<NodeHealthReport> {
        let puppetdb = self
            .puppetdb
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("PuppetDB not configured"))?;

        let time_range = config
            .time_range
            .clone()
            .unwrap_or_else(|| "24h".to_string());

        // Get all nodes
        let nodes = puppetdb.get_nodes().await?;
        let total_nodes = nodes.len() as i64;

        // Get recent reports to determine status
        let reports = puppetdb.query_reports(None, None, Some(1000)).await?;

        // Build certname -> latest status map
        let mut node_statuses: HashMap<String, (String, Option<DateTime<Utc>>)> = HashMap::new();
        for report in &reports {
            let status = report
                .status
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("unchanged");
            let certname = &report.certname;

            // Only keep the most recent report per node
            if !node_statuses.contains_key(certname) {
                node_statuses.insert(certname.clone(), (status.to_string(), report.end_time));
            }
        }

        // Count statuses
        let mut changed_count = 0i64;
        let mut unchanged_count = 0i64;
        let mut failed_count = 0i64;
        let mut noop_count = 0i64;

        for (status, _) in node_statuses.values() {
            match status.as_str() {
                "changed" => changed_count += 1,
                "unchanged" => unchanged_count += 1,
                "failed" => failed_count += 1,
                "noop" => noop_count += 1,
                _ => unchanged_count += 1,
            }
        }

        let unreported_count =
            total_nodes - (changed_count + unchanged_count + failed_count + noop_count);
        let compliance_rate = if total_nodes > 0 {
            ((total_nodes - failed_count) as f64 / total_nodes as f64) * 100.0
        } else {
            100.0
        };

        // Build environment breakdown if requested
        let by_environment = if config.group_by.as_deref() == Some("environment") {
            let mut env_map: HashMap<String, (i64, i64, i64, i64)> = HashMap::new();

            for node in &nodes {
                let env = node
                    .report_environment
                    .clone()
                    .unwrap_or_else(|| "production".to_string());
                let status = node_statuses
                    .get(&node.certname)
                    .map(|(s, _)| s.as_str())
                    .unwrap_or("unchanged");

                let entry = env_map.entry(env).or_insert((0, 0, 0, 0));
                entry.0 += 1; // total
                match status {
                    "changed" => entry.1 += 1,
                    "unchanged" => entry.2 += 1,
                    "failed" => entry.3 += 1,
                    _ => entry.2 += 1,
                }
            }

            Some(
                env_map
                    .into_iter()
                    .map(
                        |(env, (total, changed, unchanged, failed))| EnvironmentHealth {
                            environment: env,
                            total_nodes: total,
                            changed_count: changed,
                            unchanged_count: unchanged,
                            failed_count: failed,
                        },
                    )
                    .collect(),
            )
        } else {
            None
        };

        // Build node details if requested
        let nodes_detail = if config.include_error_details {
            Some(
                nodes
                    .iter()
                    .map(|n| {
                        let (status, last_report_at) = node_statuses
                            .get(&n.certname)
                            .cloned()
                            .unwrap_or_else(|| ("unreported".to_string(), None));

                        NodeHealthDetail {
                            certname: n.certname.clone(),
                            environment: n.report_environment.clone(),
                            status,
                            last_report_at,
                            failed_resources: None,
                            changed_resources: None,
                        }
                    })
                    .collect(),
            )
        } else {
            None
        };

        Ok(NodeHealthReport {
            generated_at: Utc::now(),
            time_range,
            summary: NodeHealthSummary {
                total_nodes,
                changed_count,
                unchanged_count,
                failed_count,
                noop_count,
                unreported_count,
                compliance_rate,
            },
            by_environment,
            by_group: None, // TODO: Implement group breakdown
            nodes: nodes_detail,
        })
    }

    /// Generate compliance report
    async fn generate_compliance_report(
        &self,
        _config: &ReportQueryConfig,
    ) -> Result<ComplianceReport> {
        let puppetdb = self
            .puppetdb
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("PuppetDB not configured"))?;

        let baseline_repo = ComplianceBaselineRepository::new(&self.pool);
        let baselines = baseline_repo.get_all().await?;

        if baselines.is_empty() {
            // Return empty report if no baselines defined
            return Ok(ComplianceReport {
                generated_at: Utc::now(),
                baseline_name: "No baseline defined".to_string(),
                summary: ComplianceSummary {
                    total_nodes: 0,
                    compliant_nodes: 0,
                    non_compliant_nodes: 0,
                    compliance_rate: 100.0,
                    total_violations: 0,
                },
                by_severity: vec![],
                violations: vec![],
            });
        }

        // Use first baseline for now (TODO: allow selection)
        let baseline = &baselines[0];

        // Get all nodes and their facts
        let nodes = puppetdb.get_nodes().await?;
        let mut violations = Vec::new();
        let mut compliant_nodes = 0i64;
        let mut non_compliant_nodes = 0i64;
        let mut severity_counts: HashMap<SeverityLevel, (i64, i64)> = HashMap::new();

        for node in &nodes {
            let facts = puppetdb
                .get_node_facts(&node.certname)
                .await
                .unwrap_or_default();
            let facts_map: HashMap<String, serde_json::Value> =
                facts.into_iter().map(|f| (f.name, f.value)).collect();

            let mut node_has_violation = false;

            for rule in &baseline.rules {
                let actual_value = facts_map.get(&rule.fact_name);
                let is_compliant = match actual_value {
                    Some(actual) => check_compliance(&rule.operator, &rule.expected_value, actual),
                    None => false, // Missing fact is non-compliant
                };

                if !is_compliant {
                    node_has_violation = true;
                    violations.push(ComplianceViolation {
                        certname: node.certname.clone(),
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        fact_name: rule.fact_name.clone(),
                        expected_value: rule.expected_value.clone(),
                        actual_value: actual_value.cloned().unwrap_or(serde_json::Value::Null),
                        severity: rule.severity,
                    });

                    let entry = severity_counts.entry(rule.severity).or_insert((0, 0));
                    entry.0 += 1; // violation count
                }
            }

            if node_has_violation {
                non_compliant_nodes += 1;
                for severity in [
                    SeverityLevel::Low,
                    SeverityLevel::Medium,
                    SeverityLevel::High,
                    SeverityLevel::Critical,
                ] {
                    let entry = severity_counts.entry(severity).or_insert((0, 0));
                    entry.1 += 1; // affected nodes (counted once per node)
                }
            } else {
                compliant_nodes += 1;
            }
        }

        let total_nodes = nodes.len() as i64;
        let compliance_rate = if total_nodes > 0 {
            (compliant_nodes as f64 / total_nodes as f64) * 100.0
        } else {
            100.0
        };

        let by_severity: Vec<SeverityBreakdown> = severity_counts
            .into_iter()
            .filter(|(_, (count, _))| *count > 0)
            .map(
                |(severity, (violation_count, affected_nodes))| SeverityBreakdown {
                    severity,
                    violation_count,
                    affected_nodes,
                },
            )
            .collect();

        Ok(ComplianceReport {
            generated_at: Utc::now(),
            baseline_name: baseline.name.clone(),
            summary: ComplianceSummary {
                total_nodes,
                compliant_nodes,
                non_compliant_nodes,
                compliance_rate,
                total_violations: violations.len() as i64,
            },
            by_severity,
            violations,
        })
    }

    /// Generate change tracking report
    async fn generate_change_tracking_report(
        &self,
        config: &ReportQueryConfig,
    ) -> Result<ChangeTrackingReport> {
        let puppetdb = self
            .puppetdb
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("PuppetDB not configured"))?;

        let time_range = config
            .time_range
            .clone()
            .unwrap_or_else(|| "24h".to_string());
        let status_filter = config.status_filter.as_ref();

        // Get reports (filter by status if specified)
        let status = status_filter.and_then(|f| f.first().map(|s| s.as_str()));
        let reports = puppetdb.query_reports(None, status, Some(500)).await?;

        let mut changes = Vec::new();
        let mut nodes_affected: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut resource_type_counts: HashMap<String, i64> = HashMap::new();
        let mut resources_changed = 0i64;
        let mut resources_failed = 0i64;

        for report in &reports {
            // Only include reports with changes
            let report_status = report.status.as_ref().map(|s| s.as_str()).unwrap_or("");
            if report_status != "changed" && report_status != "failed" {
                continue;
            }

            nodes_affected.insert(report.certname.clone());

            // Get resource events for this report if we need details
            if config.include_resources {
                if let Some(events) = &report.resource_events {
                    if let Some(data) = &events.data {
                        for event_val in data {
                            if let Ok(event) =
                                serde_json::from_value::<ResourceEventData>(event_val.clone())
                            {
                                let resource_type = event
                                    .resource_type
                                    .clone()
                                    .unwrap_or_else(|| "Unknown".to_string());
                                *resource_type_counts
                                    .entry(resource_type.clone())
                                    .or_insert(0) += 1;

                                if event.status.as_deref() == Some("success") {
                                    resources_changed += 1;
                                } else if event.status.as_deref() == Some("failure") {
                                    resources_failed += 1;
                                }

                                changes.push(crate::models::ChangeDetail {
                                    certname: report.certname.clone(),
                                    report_time: report.end_time.unwrap_or_else(Utc::now),
                                    resource_type,
                                    resource_title: event.resource_title.unwrap_or_default(),
                                    property: event.property,
                                    old_value: event.old_value,
                                    new_value: event.new_value,
                                    status: event.status.unwrap_or_else(|| "unknown".to_string()),
                                });
                            }
                        }
                    }
                }
            }
        }

        let changes_by_type: Vec<ChangeTypeBreakdown> = resource_type_counts
            .into_iter()
            .map(|(resource_type, change_count)| ChangeTypeBreakdown {
                resource_type,
                change_count,
            })
            .collect();

        Ok(ChangeTrackingReport {
            generated_at: Utc::now(),
            time_range,
            summary: ChangeSummary {
                total_changes: changes.len() as i64,
                nodes_affected: nodes_affected.len() as i64,
                resources_changed,
                resources_failed,
            },
            changes_by_type,
            changes,
        })
    }

    /// Generate drift detection report
    async fn generate_drift_report(&self, config: &ReportQueryConfig) -> Result<DriftReport> {
        let puppetdb = self
            .puppetdb
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("PuppetDB not configured"))?;

        let baseline_repo = DriftBaselineRepository::new(&self.pool);
        let baselines = baseline_repo.get_all().await?;

        if baselines.is_empty() {
            return Ok(DriftReport {
                generated_at: Utc::now(),
                baseline_name: "No baseline defined".to_string(),
                summary: DriftSummary {
                    total_nodes: 0,
                    nodes_with_drift: 0,
                    nodes_without_drift: 0,
                    drift_rate: 0.0,
                    total_drifted_facts: 0,
                },
                drifted_nodes: vec![],
            });
        }

        let baseline = &baselines[0];
        let nodes = puppetdb.get_nodes().await?;

        let mut drifted_nodes = Vec::new();
        let mut total_drifted_facts = 0i64;

        // Get ignored facts from tolerance config
        let ignored_facts: Vec<String> = baseline
            .tolerance_config
            .as_ref()
            .map(|c| c.ignored_facts.clone())
            .unwrap_or_default();

        // Default volatile facts to ignore
        let default_ignored = vec![
            "system_uptime",
            "uptime",
            "uptime_seconds",
            "uptime_days",
            "uptime_hours",
            "memoryfree",
            "memoryfree_mb",
            "swapfree",
            "swapfree_mb",
        ];

        for node in &nodes {
            let facts = puppetdb
                .get_node_facts(&node.certname)
                .await
                .unwrap_or_default();
            let facts_map: HashMap<String, serde_json::Value> =
                facts.into_iter().map(|f| (f.name, f.value)).collect();

            let mut node_drifted_facts = Vec::new();

            if let Some(baseline_facts) = baseline.baseline_facts.as_object() {
                for (fact_name, expected_value) in baseline_facts {
                    // Skip ignored facts
                    if ignored_facts.contains(fact_name) {
                        continue;
                    }
                    if config.ignore_volatile_facts
                        && default_ignored.iter().any(|f| fact_name.contains(f))
                    {
                        continue;
                    }

                    let actual_value = facts_map.get(fact_name);

                    let has_drift = match actual_value {
                        Some(actual) => actual != expected_value,
                        None => true, // Missing fact is drift
                    };

                    if has_drift {
                        node_drifted_facts.push(DriftedFact {
                            fact_name: fact_name.clone(),
                            baseline_value: expected_value.clone(),
                            current_value: actual_value.cloned().unwrap_or(serde_json::Value::Null),
                            drift_severity: SeverityLevel::Medium, // TODO: Make configurable
                        });
                    }
                }
            }

            if !node_drifted_facts.is_empty() {
                total_drifted_facts += node_drifted_facts.len() as i64;
                drifted_nodes.push(DriftedNode {
                    certname: node.certname.clone(),
                    drift_count: node_drifted_facts.len() as i64,
                    drifted_facts: node_drifted_facts,
                });
            }
        }

        let total_nodes = nodes.len() as i64;
        let nodes_with_drift = drifted_nodes.len() as i64;
        let nodes_without_drift = total_nodes - nodes_with_drift;
        let drift_rate = if total_nodes > 0 {
            (nodes_with_drift as f64 / total_nodes as f64) * 100.0
        } else {
            0.0
        };

        Ok(DriftReport {
            generated_at: Utc::now(),
            baseline_name: baseline.name.clone(),
            summary: DriftSummary {
                total_nodes,
                nodes_with_drift,
                nodes_without_drift,
                drift_rate,
                total_drifted_facts,
            },
            drifted_nodes,
        })
    }

    /// Export report data to specified format
    pub fn export_report(&self, result: &ReportResult, format: OutputFormat) -> Result<Vec<u8>> {
        match format {
            OutputFormat::Json => {
                let json = serde_json::to_vec_pretty(result)
                    .context("Failed to serialize report to JSON")?;
                Ok(json)
            }
            OutputFormat::Csv => {
                let csv = self.export_to_csv(result)?;
                Ok(csv.into_bytes())
            }
            OutputFormat::Pdf => {
                let pdf = self.export_to_pdf(result)?;
                Ok(pdf)
            }
        }
    }

    /// Export report to PDF format
    ///
    /// Creates a simple PDF document with the report content.
    /// Uses printpdf for PDF generation.
    fn export_to_pdf(&self, result: &ReportResult) -> Result<Vec<u8>> {
        // Get the title and content based on report type
        let (title, content) = match result {
            ReportResult::NodeHealth(report) => (
                "Node Health Report",
                self.format_node_health_for_pdf(report),
            ),
            ReportResult::Compliance(report) => {
                ("Compliance Report", self.format_compliance_for_pdf(report))
            }
            ReportResult::ChangeTracking(report) => (
                "Change Tracking Report",
                self.format_change_tracking_for_pdf(report),
            ),
            ReportResult::DriftDetection(report) => {
                ("Drift Detection Report", self.format_drift_for_pdf(report))
            }
            ReportResult::Custom(data) => (
                "Custom Report",
                serde_json::to_string_pretty(data).unwrap_or_default(),
            ),
            ReportResult::Plugin(report) => ("Plugin Report", plugin_report_text(report)),
        };

        // Create PDF document
        let mut doc = PdfDocument::new(title);

        // Get font references for builtin fonts
        let font = BuiltinFont::Helvetica;
        let font_bold = BuiltinFont::HelveticaBold;

        // Build page operations
        let mut ops = vec![
            Op::StartTextSection,
            // Title - position at top of page (A4: 210mm x 297mm)
            Op::SetTextCursor {
                pos: printpdf::Point {
                    x: Pt::from(Mm(20.0)),
                    y: Pt::from(Mm(280.0)),
                },
            },
            Op::SetFont {
                font: PdfFontHandle::Builtin(font_bold),
                size: Pt(16.0),
            },
            Op::SetLineHeight { lh: Pt(16.0) },
            Op::ShowText {
                items: vec![TextItem::Text(title.to_string())],
            },
        ];

        // Content - split by lines and render
        let lines: Vec<&str> = content.lines().collect();
        let mut y_pos = 265.0;
        let line_height = 5.0;

        for line in lines {
            if y_pos < 20.0 {
                // Would need pagination for very long reports
                // For now, truncate
                break;
            }

            let is_heading = line.starts_with("===") || line.starts_with("---");
            let font_to_use = if is_heading { font_bold } else { font };

            // Clean up section markers
            let display_line = line
                .trim_start_matches("===")
                .trim_end_matches("===")
                .trim_start_matches("---")
                .trim_end_matches("---")
                .trim();

            if !display_line.is_empty() {
                ops.push(Op::SetTextCursor {
                    pos: printpdf::Point {
                        x: Pt::from(Mm(20.0)),
                        y: Pt::from(Mm(y_pos)),
                    },
                });
                ops.push(Op::SetFont {
                    font: PdfFontHandle::Builtin(font_to_use),
                    size: Pt(if is_heading { 12.0 } else { 10.0 }),
                });
                ops.push(Op::SetLineHeight {
                    lh: Pt(if is_heading { 12.0 } else { 10.0 }),
                });
                ops.push(Op::ShowText {
                    items: vec![TextItem::Text(display_line.to_string())],
                });
            }

            y_pos -= line_height;
        }

        ops.push(Op::EndTextSection);

        // Create A4 page with operations
        let page = PdfPage::new(Mm(210.0), Mm(297.0), ops);
        doc.with_pages(vec![page]);

        // Save to bytes
        let mut warnings = Vec::new();
        let buffer = doc.save(&PdfSaveOptions::default(), &mut warnings);

        Ok(buffer)
    }

    fn format_node_health_for_pdf(&self, report: &NodeHealthReport) -> String {
        let mut content = String::new();
        content.push_str(&format!(
            "Generated: {}\n",
            report.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        content.push_str(&format!("Time Range: {}\n\n", report.time_range));

        content.push_str("=== Summary ===\n");
        content.push_str(&format!("Total Nodes: {}\n", report.summary.total_nodes));
        content.push_str(&format!("Changed: {}\n", report.summary.changed_count));
        content.push_str(&format!("Unchanged: {}\n", report.summary.unchanged_count));
        content.push_str(&format!("Failed: {}\n", report.summary.failed_count));
        content.push_str(&format!("Noop: {}\n", report.summary.noop_count));
        content.push_str(&format!(
            "Unreported: {}\n",
            report.summary.unreported_count
        ));
        content.push_str(&format!(
            "Compliance Rate: {:.2}%\n\n",
            report.summary.compliance_rate
        ));

        if let Some(ref by_env) = report.by_environment {
            content.push_str("=== By Environment ===\n");
            for env in by_env {
                content.push_str(&format!(
                    "{}: {} total, {} changed, {} failed\n",
                    env.environment, env.total_nodes, env.changed_count, env.failed_count
                ));
            }
        }

        content
    }

    fn format_compliance_for_pdf(&self, report: &ComplianceReport) -> String {
        let mut content = String::new();
        content.push_str(&format!(
            "Generated: {}\n",
            report.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        content.push_str(&format!("Baseline: {}\n\n", report.baseline_name));

        content.push_str("=== Summary ===\n");
        content.push_str(&format!("Total Nodes: {}\n", report.summary.total_nodes));
        content.push_str(&format!("Compliant: {}\n", report.summary.compliant_nodes));
        content.push_str(&format!(
            "Non-Compliant: {}\n",
            report.summary.non_compliant_nodes
        ));
        content.push_str(&format!(
            "Compliance Rate: {:.2}%\n",
            report.summary.compliance_rate
        ));
        content.push_str(&format!(
            "Total Violations: {}\n\n",
            report.summary.total_violations
        ));

        if !report.violations.is_empty() {
            content.push_str("=== Violations ===\n");
            for v in report.violations.iter().take(30) {
                content.push_str(&format!(
                    "{}: {} [{}]\n",
                    v.certname,
                    v.fact_name,
                    v.severity.as_str()
                ));
            }
            if report.violations.len() > 30 {
                content.push_str(&format!("... and {} more\n", report.violations.len() - 30));
            }
        }

        content
    }

    fn format_change_tracking_for_pdf(&self, report: &ChangeTrackingReport) -> String {
        let mut content = String::new();
        content.push_str(&format!(
            "Generated: {}\n",
            report.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        content.push_str(&format!("Time Range: {}\n\n", report.time_range));

        content.push_str("=== Summary ===\n");
        content.push_str(&format!(
            "Total Changes: {}\n",
            report.summary.total_changes
        ));
        content.push_str(&format!(
            "Nodes Affected: {}\n",
            report.summary.nodes_affected
        ));
        content.push_str(&format!(
            "Resources Changed: {}\n",
            report.summary.resources_changed
        ));
        content.push_str(&format!(
            "Resources Failed: {}\n\n",
            report.summary.resources_failed
        ));

        if !report.changes_by_type.is_empty() {
            content.push_str("=== Changes by Type ===\n");
            for ct in &report.changes_by_type {
                content.push_str(&format!("{}: {}\n", ct.resource_type, ct.change_count));
            }
        }

        content
    }

    fn format_drift_for_pdf(&self, report: &DriftReport) -> String {
        let mut content = String::new();
        content.push_str(&format!(
            "Generated: {}\n",
            report.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        content.push_str(&format!("Baseline: {}\n\n", report.baseline_name));

        content.push_str("=== Summary ===\n");
        content.push_str(&format!("Total Nodes: {}\n", report.summary.total_nodes));
        content.push_str(&format!(
            "Nodes With Drift: {}\n",
            report.summary.nodes_with_drift
        ));
        content.push_str(&format!(
            "Nodes Without Drift: {}\n",
            report.summary.nodes_without_drift
        ));
        content.push_str(&format!("Drift Rate: {:.2}%\n", report.summary.drift_rate));
        content.push_str(&format!(
            "Total Drifted Facts: {}\n\n",
            report.summary.total_drifted_facts
        ));

        if !report.drifted_nodes.is_empty() {
            content.push_str("=== Drifted Nodes ===\n");
            for node in report.drifted_nodes.iter().take(15) {
                content.push_str(&format!("{}: {} facts\n", node.certname, node.drift_count));
            }
            if report.drifted_nodes.len() > 15 {
                content.push_str(&format!(
                    "... and {} more\n",
                    report.drifted_nodes.len() - 15
                ));
            }
        }

        content
    }

    /// Export report to CSV format
    fn export_to_csv(&self, result: &ReportResult) -> Result<String> {
        let mut csv = String::new();

        match result {
            ReportResult::NodeHealth(report) => {
                csv.push_str("Node Health Report\n");
                csv.push_str(&format!("Generated At,{}\n", report.generated_at));
                csv.push_str(&format!("Time Range,{}\n\n", report.time_range));

                csv.push_str("Summary\n");
                csv.push_str("Metric,Value\n");
                csv.push_str(&format!("Total Nodes,{}\n", report.summary.total_nodes));
                csv.push_str(&format!("Changed,{}\n", report.summary.changed_count));
                csv.push_str(&format!("Unchanged,{}\n", report.summary.unchanged_count));
                csv.push_str(&format!("Failed,{}\n", report.summary.failed_count));
                csv.push_str(&format!("Noop,{}\n", report.summary.noop_count));
                csv.push_str(&format!("Unreported,{}\n", report.summary.unreported_count));
                csv.push_str(&format!(
                    "Compliance Rate,{:.2}%\n",
                    report.summary.compliance_rate
                ));

                if let Some(nodes) = &report.nodes {
                    csv.push_str("\nNode Details\n");
                    csv.push_str("Certname,Environment,Status,Last Report\n");
                    for node in nodes {
                        csv.push_str(&format!(
                            "{},{},{},{}\n",
                            node.certname,
                            node.environment.as_deref().unwrap_or(""),
                            node.status,
                            node.last_report_at
                                .map(|t| t.to_rfc3339())
                                .unwrap_or_default()
                        ));
                    }
                }
            }
            ReportResult::Compliance(report) => {
                csv.push_str("Compliance Report\n");
                csv.push_str(&format!("Generated At,{}\n", report.generated_at));
                csv.push_str(&format!("Baseline,{}\n\n", report.baseline_name));

                csv.push_str("Summary\n");
                csv.push_str(&format!("Total Nodes,{}\n", report.summary.total_nodes));
                csv.push_str(&format!("Compliant,{}\n", report.summary.compliant_nodes));
                csv.push_str(&format!(
                    "Non-Compliant,{}\n",
                    report.summary.non_compliant_nodes
                ));
                csv.push_str(&format!(
                    "Compliance Rate,{:.2}%\n",
                    report.summary.compliance_rate
                ));

                csv.push_str("\nViolations\n");
                csv.push_str("Certname,Rule,Fact,Expected,Actual,Severity\n");
                for v in &report.violations {
                    csv.push_str(&format!(
                        "{},{},{},{},{},{}\n",
                        v.certname,
                        v.rule_name,
                        v.fact_name,
                        v.expected_value,
                        v.actual_value,
                        v.severity.as_str()
                    ));
                }
            }
            ReportResult::ChangeTracking(report) => {
                csv.push_str("Change Tracking Report\n");
                csv.push_str(&format!("Generated At,{}\n", report.generated_at));
                csv.push_str(&format!("Time Range,{}\n\n", report.time_range));

                csv.push_str("Summary\n");
                csv.push_str(&format!("Total Changes,{}\n", report.summary.total_changes));
                csv.push_str(&format!(
                    "Nodes Affected,{}\n",
                    report.summary.nodes_affected
                ));
                csv.push_str(&format!(
                    "Resources Changed,{}\n",
                    report.summary.resources_changed
                ));
                csv.push_str(&format!(
                    "Resources Failed,{}\n",
                    report.summary.resources_failed
                ));

                csv.push_str("\nChanges\n");
                csv.push_str("Certname,Time,Resource Type,Title,Property,Status\n");
                for c in &report.changes {
                    csv.push_str(&format!(
                        "{},{},{},{},{},{}\n",
                        c.certname,
                        c.report_time.to_rfc3339(),
                        c.resource_type,
                        c.resource_title,
                        c.property.as_deref().unwrap_or(""),
                        c.status
                    ));
                }
            }
            ReportResult::DriftDetection(report) => {
                csv.push_str("Drift Detection Report\n");
                csv.push_str(&format!("Generated At,{}\n", report.generated_at));
                csv.push_str(&format!("Baseline,{}\n\n", report.baseline_name));

                csv.push_str("Summary\n");
                csv.push_str(&format!("Total Nodes,{}\n", report.summary.total_nodes));
                csv.push_str(&format!(
                    "Nodes With Drift,{}\n",
                    report.summary.nodes_with_drift
                ));
                csv.push_str(&format!(
                    "Nodes Without Drift,{}\n",
                    report.summary.nodes_without_drift
                ));
                csv.push_str(&format!("Drift Rate,{:.2}%\n", report.summary.drift_rate));

                csv.push_str("\nDrifted Nodes\n");
                csv.push_str("Certname,Fact,Baseline Value,Current Value,Severity\n");
                for node in &report.drifted_nodes {
                    for fact in &node.drifted_facts {
                        csv.push_str(&format!(
                            "{},{},{},{},{}\n",
                            node.certname,
                            fact.fact_name,
                            fact.baseline_value,
                            fact.current_value,
                            fact.drift_severity.as_str()
                        ));
                    }
                }
            }
            ReportResult::Custom(data) => {
                csv.push_str(&serde_json::to_string_pretty(data).unwrap_or_default());
            }
            ReportResult::Plugin(report) => csv.push_str(&plugin_report_csv(report)),
        }

        Ok(csv)
    }
}

/// One CSV cell: quoted when needed, and prefixed with `'` when it would
/// start a spreadsheet formula (plugin values are not trusted).
fn csv_cell(v: &serde_json::Value) -> String {
    let text = match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    let text = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) && !v.is_number() {
        format!("'{}", text)
    } else {
        text
    };
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

fn csv_text(s: &str) -> String {
    csv_cell(&serde_json::Value::String(s.to_string()))
}

/// CSV of a plugin report: one block per section (M25).
fn plugin_report_csv(r: &PluginReportResult) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}\nGenerated At,{}\n",
        csv_text(r.title.as_deref().unwrap_or(&r.type_id)),
        r.generated_at
    ));
    for s in &r.sections {
        out.push('\n');
        match s {
            ReportSection::Kpi { id, title, items } => {
                out.push_str(&format!(
                    "{}\nMetric,Value,Unit\n",
                    csv_text(title.as_deref().unwrap_or(id))
                ));
                for i in items {
                    out.push_str(&format!(
                        "{},{},{}\n",
                        csv_text(&i.label),
                        csv_cell(&i.value),
                        csv_text(i.unit.as_deref().unwrap_or(""))
                    ));
                }
            }
            ReportSection::Table {
                id,
                title,
                columns,
                rows,
            } => {
                out.push_str(&format!("{}\n", csv_text(title.as_deref().unwrap_or(id))));
                let header: Vec<String> = columns.iter().map(|c| csv_text(&c.label)).collect();
                out.push_str(&header.join(","));
                out.push('\n');
                for row in rows {
                    let cells: Vec<String> = columns
                        .iter()
                        .map(|c| csv_cell(row.get(&c.key).unwrap_or(&serde_json::Value::Null)))
                        .collect();
                    out.push_str(&cells.join(","));
                    out.push('\n');
                }
            }
            ReportSection::Chart {
                id, title, series, ..
            } => {
                out.push_str(&format!(
                    "{}\nSeries,X,Y\n",
                    csv_text(title.as_deref().unwrap_or(id))
                ));
                for se in series {
                    for p in &se.points {
                        out.push_str(&format!(
                            "{},{},{}\n",
                            csv_text(&se.name),
                            csv_cell(&p.x),
                            p.y
                        ));
                    }
                }
            }
            ReportSection::Text { id, title, text } => {
                out.push_str(&format!(
                    "{}\n{}\n",
                    csv_text(title.as_deref().unwrap_or(id)),
                    csv_text(text)
                ));
            }
        }
    }
    out
}

/// Plain text lines of a plugin report for the PDF export (M25).
fn plugin_report_text(r: &PluginReportResult) -> String {
    let mut out = format!(
        "{}\nGenerated: {}\n",
        r.title.as_deref().unwrap_or(&r.type_id),
        r.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    let cell = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    for s in &r.sections {
        match s {
            ReportSection::Kpi { id, title, items } => {
                out.push_str(&format!("=== {} ===\n", title.as_deref().unwrap_or(id)));
                for i in items {
                    out.push_str(&format!(
                        "{}: {} {}\n",
                        i.label,
                        cell(&i.value),
                        i.unit.as_deref().unwrap_or("")
                    ));
                }
            }
            ReportSection::Table {
                id,
                title,
                columns,
                rows,
            } => {
                out.push_str(&format!("=== {} ===\n", title.as_deref().unwrap_or(id)));
                let header: Vec<&str> = columns.iter().map(|c| c.label.as_str()).collect();
                out.push_str(&format!("--- {} ---\n", header.join(" | ")));
                for row in rows {
                    let cells: Vec<String> = columns
                        .iter()
                        .map(|c| row.get(&c.key).map(cell).unwrap_or_default())
                        .collect();
                    out.push_str(&format!("{}\n", cells.join(" | ")));
                }
            }
            ReportSection::Chart {
                id, title, series, ..
            } => {
                out.push_str(&format!("=== {} ===\n", title.as_deref().unwrap_or(id)));
                for se in series {
                    let points: Vec<String> = se
                        .points
                        .iter()
                        .map(|p| format!("{}={}", cell(&p.x), p.y))
                        .collect();
                    out.push_str(&format!("{}: {}\n", se.name, points.join(", ")));
                }
            }
            ReportSection::Text { id, title, text } => {
                out.push_str(&format!(
                    "=== {} ===\n{}\n",
                    title.as_deref().unwrap_or(id),
                    text
                ));
            }
        }
    }
    out
}

/// Helper struct for parsing resource events
#[derive(Debug, serde::Deserialize)]
struct ResourceEventData {
    resource_type: Option<String>,
    resource_title: Option<String>,
    property: Option<String>,
    old_value: Option<serde_json::Value>,
    new_value: Option<serde_json::Value>,
    status: Option<String>,
}

/// Check if a fact value complies with a rule
fn check_compliance(
    operator: &str,
    expected: &serde_json::Value,
    actual: &serde_json::Value,
) -> bool {
    match operator {
        "=" | "==" | "equals" => actual == expected,
        "!=" | "not_equals" => actual != expected,
        ">" | "greater_than" => match (actual.as_f64(), expected.as_f64()) {
            (Some(a), Some(e)) => a > e,
            _ => false,
        },
        ">=" | "greater_than_or_equal" => match (actual.as_f64(), expected.as_f64()) {
            (Some(a), Some(e)) => a >= e,
            _ => false,
        },
        "<" | "less_than" => match (actual.as_f64(), expected.as_f64()) {
            (Some(a), Some(e)) => a < e,
            _ => false,
        },
        "<=" | "less_than_or_equal" => match (actual.as_f64(), expected.as_f64()) {
            (Some(a), Some(e)) => a <= e,
            _ => false,
        },
        "~" | "matches" | "regex" => {
            if let (Some(pattern), Some(value)) = (expected.as_str(), actual.as_str()) {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(value))
                    .unwrap_or(false)
            } else {
                false
            }
        }
        "in" => {
            if let Some(arr) = expected.as_array() {
                arr.contains(actual)
            } else {
                false
            }
        }
        "not_in" => {
            if let Some(arr) = expected.as_array() {
                !arr.contains(actual)
            } else {
                true
            }
        }
        _ => actual == expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_compliance_equals() {
        let expected = serde_json::json!("ubuntu");
        let actual = serde_json::json!("ubuntu");
        assert!(check_compliance("=", &expected, &actual));
        assert!(check_compliance("equals", &expected, &actual));
    }

    #[test]
    fn test_check_compliance_not_equals() {
        let expected = serde_json::json!("ubuntu");
        let actual = serde_json::json!("centos");
        assert!(check_compliance("!=", &expected, &actual));
    }

    #[test]
    fn test_check_compliance_numeric() {
        let expected = serde_json::json!(10);
        let actual = serde_json::json!(15);
        assert!(check_compliance(">", &expected, &actual));
        assert!(check_compliance(">=", &expected, &actual));
        assert!(!check_compliance("<", &expected, &actual));
    }

    #[test]
    fn test_check_compliance_in() {
        let expected = serde_json::json!(["ubuntu", "debian", "centos"]);
        let actual = serde_json::json!("ubuntu");
        assert!(check_compliance("in", &expected, &actual));

        let actual_missing = serde_json::json!("fedora");
        assert!(!check_compliance("in", &expected, &actual_missing));
    }

    #[test]
    fn test_check_compliance_regex() {
        let expected = serde_json::json!("^ubuntu.*");
        let actual = serde_json::json!("ubuntu-22.04");
        assert!(check_compliance("~", &expected, &actual));
        assert!(check_compliance("regex", &expected, &actual));
    }
}
