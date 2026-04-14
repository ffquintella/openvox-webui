//! Inventory repository.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::models::{
    ComplianceCategoryNode, CreateGroupUpdateScheduleRequest, FleetRepositoryConfig,
    GroupUpdateSchedule, HostApplicationInventoryItem, HostContainerInventoryItem, HostOsInventory,
    HostPackageInventoryItem, HostRepositoryConfig, HostRuntimeInventoryItem, HostUpdateStatus,
    HostUserInventoryItem, HostWebInventoryItem, InventoryDashboardReport,
    InventoryDistributionPoint, InventoryFleetStatusSummary, InventoryPayload,
    InventorySnapshotSummary, InventorySummary, NodeInventory, NodePendingUpdateJob,
    OutdatedInventoryItem, OutdatedSoftwareNodeDetail, PatchAgeBucket,
    RepositoryVersionCatalogEntry, SubmitUpdateJobResultRequest, TopOutdatedSoftwareItem,
    UpdateGroupUpdateScheduleRequest, UpdateJob, UpdateJobResult, UpdateJobStatus, UpdateJobTarget,
    UpdateOperationType, UpdateTargetStatus,
};

pub struct InventoryRepository {
    pool: SqlitePool,
}

impl InventoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn ingest_inventory(
        &self,
        certname: &str,
        payload: &InventoryPayload,
    ) -> Result<NodeInventory> {
        let snapshot_id = Uuid::new_v4().to_string();
        let collected_at = payload.collected_at.unwrap_or_else(Utc::now);
        let now = Utc::now();
        let raw_payload =
            serde_json::to_string(payload).context("Failed to serialize inventory payload")?;

        let mut tx = self
            .pool
            .begin()
            .await
            .context("Failed to begin inventory transaction")?;

        sqlx::query(
            r#"
            INSERT INTO host_inventory_snapshots (
                id, certname, collector_version, collected_at, is_full_snapshot,
                os_family, distribution, os_version, package_count, application_count,
                website_count, runtime_count, container_count, user_count, raw_payload, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
        )
        .bind(&snapshot_id)
        .bind(certname)
        .bind(&payload.collector_version)
        .bind(collected_at.to_rfc3339())
        .bind(payload.is_full_snapshot)
        .bind(&payload.os.os_family)
        .bind(&payload.os.distribution)
        .bind(&payload.os.os_version)
        .bind(payload.packages.len() as i64)
        .bind(payload.applications.len() as i64)
        .bind(payload.websites.len() as i64)
        .bind(payload.runtimes.len() as i64)
        .bind(payload.containers.len() as i64)
        .bind(payload.users.len() as i64)
        .bind(raw_payload)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await
        .context("Failed to insert inventory snapshot")?;

        sqlx::query(
            r#"
            INSERT INTO host_os_inventory (
                certname, snapshot_id, os_family, distribution, edition, architecture,
                kernel_version, os_version, patch_level, package_manager, update_channel,
                last_inventory_at, last_successful_update_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(certname) DO UPDATE SET
                snapshot_id = excluded.snapshot_id,
                os_family = excluded.os_family,
                distribution = excluded.distribution,
                edition = excluded.edition,
                architecture = excluded.architecture,
                kernel_version = excluded.kernel_version,
                os_version = excluded.os_version,
                patch_level = excluded.patch_level,
                package_manager = excluded.package_manager,
                update_channel = excluded.update_channel,
                last_inventory_at = excluded.last_inventory_at,
                last_successful_update_at = excluded.last_successful_update_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(certname)
        .bind(&snapshot_id)
        .bind(&payload.os.os_family)
        .bind(&payload.os.distribution)
        .bind(&payload.os.edition)
        .bind(&payload.os.architecture)
        .bind(&payload.os.kernel_version)
        .bind(&payload.os.os_version)
        .bind(&payload.os.patch_level)
        .bind(&payload.os.package_manager)
        .bind(&payload.os.update_channel)
        .bind(
            payload
                .os
                .last_inventory_at
                .unwrap_or(collected_at)
                .to_rfc3339(),
        )
        .bind(
            payload
                .os
                .last_successful_update_at
                .map(|ts| ts.to_rfc3339()),
        )
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await
        .context("Failed to upsert OS inventory")?;

        self.replace_packages(&mut tx, certname, &snapshot_id, &payload.packages, now)
            .await?;
        self.replace_applications(&mut tx, certname, &snapshot_id, &payload.applications, now)
            .await?;
        self.replace_websites(&mut tx, certname, &snapshot_id, &payload.websites, now)
            .await?;
        self.replace_runtimes(&mut tx, certname, &snapshot_id, &payload.runtimes, now)
            .await?;
        self.replace_containers(&mut tx, certname, &snapshot_id, &payload.containers, now)
            .await?;
        self.replace_users(&mut tx, certname, &snapshot_id, &payload.users, now)
            .await?;
        self.replace_repositories(
            &mut tx,
            certname,
            &snapshot_id,
            &payload.os.os_family,
            &payload.os.distribution,
            &payload.os.os_version,
            payload.os.package_manager.as_deref().unwrap_or(""),
            &payload.repositories,
            now,
        )
        .await?;

        tx.commit()
            .await
            .context("Failed to commit inventory transaction")?;

        // Refresh fleet-wide repository configs outside the transaction
        if !payload.repositories.is_empty() {
            if let Err(e) = self.refresh_fleet_repository_configs().await {
                tracing::warn!("Failed to refresh fleet repository configs: {}", e);
            }
        }

        self.get_current_inventory(certname)
            .await?
            .context("Inventory was ingested but could not be reloaded")
    }

    pub async fn get_current_inventory(&self, certname: &str) -> Result<Option<NodeInventory>> {
        let snapshot_row = sqlx::query_as::<_, InventorySnapshotRow>(
            r#"
            SELECT s.*
            FROM host_inventory_snapshots s
            INNER JOIN host_os_inventory o ON o.snapshot_id = s.id
            WHERE o.certname = ?1
            ORDER BY datetime(s.collected_at) DESC
            LIMIT 1
            "#,
        )
        .bind(certname)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch inventory snapshot")?;

        let Some(snapshot_row) = snapshot_row else {
            return Ok(None);
        };

        let os_row = sqlx::query_as::<_, HostOsInventoryRow>(
            "SELECT * FROM host_os_inventory WHERE certname = ?1",
        )
        .bind(certname)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch OS inventory")?;

        let Some(os_row) = os_row else {
            return Ok(None);
        };

        let packages: Vec<HostPackageInventoryItem> = sqlx::query_as::<_, HostPackageInventoryRow>(
            "SELECT * FROM host_package_inventory WHERE certname = ?1 ORDER BY name ASC, version ASC",
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch package inventory")?
        .into_iter()
        .map(Into::into)
        .collect();

        let applications: Vec<HostApplicationInventoryItem> =
            sqlx::query_as::<_, HostApplicationInventoryRow>(
            "SELECT * FROM host_application_inventory WHERE certname = ?1 ORDER BY name ASC, version ASC",
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch application inventory")?
        .into_iter()
        .map(Into::into)
        .collect();

        let websites: Vec<HostWebInventoryItem> = sqlx::query_as::<_, HostWebInventoryRow>(
            "SELECT * FROM host_web_inventory WHERE certname = ?1 ORDER BY server_type ASC, site_name ASC",
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch website inventory")?
        .into_iter()
        .map(Into::into)
        .collect();

        let runtimes: Vec<HostRuntimeInventoryItem> =
            sqlx::query_as::<_, HostRuntimeInventoryRow>(
            "SELECT * FROM host_runtime_inventory WHERE certname = ?1 ORDER BY runtime_type ASC, runtime_name ASC",
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch runtime inventory")?
        .into_iter()
        .map(Into::into)
        .collect();

        let containers: Vec<HostContainerInventoryItem> =
            sqlx::query_as::<_, HostContainerInventoryRow>(
            "SELECT container_id, name, image, status, status_detail, created_at, ports_json, mounts_json, runtime_type, metadata_json FROM host_container_inventory WHERE certname = ?1 ORDER BY runtime_type ASC, name ASC",
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch container inventory")?
        .into_iter()
        .map(Into::into)
        .collect();

        let users: Vec<HostUserInventoryItem> =
            sqlx::query_as::<_, HostUserInventoryRow>(
            "SELECT username, uid, sid, gid, home_directory, shell, user_type, groups_json, last_login, locked, gecos, metadata_json FROM host_user_inventory WHERE certname = ?1 ORDER BY user_type ASC, username ASC",
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch user inventory")?
        .into_iter()
        .map(Into::into)
        .collect();

        let snapshot: InventorySnapshotSummary = snapshot_row.into();
        let os: HostOsInventory = os_row.into();
        let update_status = self.get_host_update_status(certname).await?;
        let summary = InventorySummary {
            certname: certname.to_string(),
            os_family: os.os_family.clone(),
            distribution: os.distribution.clone(),
            os_version: os.os_version.clone(),
            patch_level: os.patch_level.clone(),
            architecture: os.architecture.clone(),
            package_manager: os.package_manager.clone(),
            update_channel: os.update_channel.clone(),
            last_inventory_at: os.last_inventory_at,
            last_successful_update_at: os.last_successful_update_at,
            package_count: packages.len(),
            application_count: applications.len(),
            website_count: websites.len(),
            runtime_count: runtimes.len(),
            container_count: containers.len(),
            user_count: users.len(),
            collected_at: snapshot.collected_at,
            collector_version: snapshot.collector_version.clone(),
            is_stale: snapshot.collected_at < Utc::now() - Duration::days(2),
        };

        Ok(Some(NodeInventory {
            snapshot,
            summary,
            update_status,
            os,
            packages,
            applications,
            websites,
            runtimes,
            containers,
            users,
        }))
    }

    pub async fn get_inventory_history(
        &self,
        certname: &str,
        limit: usize,
    ) -> Result<Vec<InventorySnapshotSummary>> {
        let rows = sqlx::query_as::<_, InventorySnapshotRow>(
            r#"
            SELECT *
            FROM host_inventory_snapshots
            WHERE certname = ?1
            ORDER BY datetime(collected_at) DESC
            LIMIT ?2
            "#,
        )
        .bind(certname)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch inventory history")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn refresh_version_catalog(&self) -> Result<usize> {
        let now = Utc::now();
        let package_rows = sqlx::query_as::<_, CatalogPackageObservationRow>(
            r#"
            SELECT o.os_family, o.distribution,
                   CASE WHEN INSTR(o.os_version, '.') > 0
                        THEN SUBSTR(o.os_version, 1, INSTR(o.os_version, '.') - 1)
                        ELSE o.os_version END AS os_version_pattern,
                   o.package_manager, p.name, p.repository_source,
                   p.version, p.release, MAX(s.collected_at) AS last_seen_at, COUNT(DISTINCT p.certname) AS observed_nodes
            FROM host_package_inventory p
            INNER JOIN host_os_inventory o ON o.certname = p.certname
            INNER JOIN host_inventory_snapshots s ON s.id = p.snapshot_id
            GROUP BY o.os_family, o.distribution,
                     CASE WHEN INSTR(o.os_version, '.') > 0
                          THEN SUBSTR(o.os_version, 1, INSTR(o.os_version, '.') - 1)
                          ELSE o.os_version END,
                     o.package_manager, p.name, p.repository_source, p.version, p.release
            "#
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load package observations for version catalog")?;

        let application_rows = sqlx::query_as::<_, CatalogApplicationObservationRow>(
            r#"
            SELECT o.os_family, o.distribution,
                   CASE WHEN INSTR(o.os_version, '.') > 0
                        THEN SUBSTR(o.os_version, 1, INSTR(o.os_version, '.') - 1)
                        ELSE o.os_version END AS os_version_pattern,
                   o.package_manager, a.name, a.publisher, a.application_type,
                   a.version, MAX(s.collected_at) AS last_seen_at, COUNT(DISTINCT a.certname) AS observed_nodes
            FROM host_application_inventory a
            INNER JOIN host_os_inventory o ON o.certname = a.certname
            INNER JOIN host_inventory_snapshots s ON s.id = a.snapshot_id
            GROUP BY o.os_family, o.distribution,
                     CASE WHEN INSTR(o.os_version, '.') > 0
                          THEN SUBSTR(o.os_version, 1, INSTR(o.os_version, '.') - 1)
                          ELSE o.os_version END,
                     o.package_manager, a.name, a.publisher, a.application_type, a.version
            "#
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load application observations for version catalog")?;

        sqlx::query("DELETE FROM repository_version_catalog")
            .execute(&self.pool)
            .await
            .context("Failed to clear repository version catalog")?;

        let mut inserted = 0usize;

        for entry in fold_package_catalog(package_rows) {
            self.insert_catalog_entry(&entry, now).await?;
            inserted += 1;
        }

        for entry in fold_application_catalog(application_rows) {
            self.insert_catalog_entry(&entry, now).await?;
            inserted += 1;
        }

        Ok(inserted)
    }

    pub async fn refresh_host_update_statuses(
        &self,
        stale_after_hours: i64,
    ) -> Result<InventoryFleetStatusSummary> {
        let now = Utc::now();
        let stale_before = now - Duration::hours(stale_after_hours);
        let snapshots = sqlx::query_as::<_, HostSnapshotStatusRow>(
            r#"
            SELECT o.certname, o.snapshot_id, s.collected_at
            FROM host_os_inventory o
            INNER JOIN host_inventory_snapshots s ON s.id = o.snapshot_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load host snapshots for update status")?;

        let catalogs = self.list_version_catalog().await?;
        let package_rows = sqlx::query_as::<_, HostPackageJoinedRow>(
            r#"
            SELECT p.certname, o.os_family, o.distribution, o.os_version, o.package_manager, p.name, p.version, p.release, p.repository_source
            FROM host_package_inventory p
            INNER JOIN host_os_inventory o ON o.certname = p.certname
            ORDER BY p.certname ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load package inventory for update status")?;
        let application_rows = sqlx::query_as::<_, HostApplicationJoinedRow>(
            r#"
            SELECT a.certname, o.os_family, o.distribution, o.os_version, o.package_manager, a.name, a.version, a.publisher, a.application_type
            FROM host_application_inventory a
            INNER JOIN host_os_inventory o ON o.certname = a.certname
            ORDER BY a.certname ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load application inventory for update status")?;

        let package_groups = group_packages_by_host(package_rows);
        let application_groups = group_applications_by_host(application_rows);

        sqlx::query("DELETE FROM host_update_status")
            .execute(&self.pool)
            .await
            .context("Failed to clear host update status")?;

        let mut stale_nodes = 0usize;
        let mut outdated_nodes = 0usize;
        let mut outdated_packages_total = 0usize;
        let mut outdated_applications_total = 0usize;

        for snapshot in &snapshots {
            let packages = package_groups
                .get(&snapshot.certname)
                .cloned()
                .unwrap_or_default();
            let applications = application_groups
                .get(&snapshot.certname)
                .cloned()
                .unwrap_or_default();
            let is_stale = parse_timestamp_required(&snapshot.collected_at) < stale_before;
            let stale_reason = is_stale.then_some(format!(
                "No inventory received since {}",
                stale_before.to_rfc3339()
            ));

            let outdated_packages = compare_packages(&packages, &catalogs);
            let outdated_applications = compare_applications(&applications, &catalogs);

            let outdated_items: Vec<OutdatedInventoryItem> = outdated_packages
                .iter()
                .cloned()
                .chain(outdated_applications.iter().cloned())
                .collect();

            if is_stale {
                stale_nodes += 1;
            }
            if !outdated_items.is_empty() {
                outdated_nodes += 1;
            }
            outdated_packages_total += outdated_packages.len();
            outdated_applications_total += outdated_applications.len();

            sqlx::query(
                r#"
                INSERT INTO host_update_status (
                    certname, snapshot_id, is_stale, stale_reason, outdated_packages,
                    outdated_applications, total_packages, total_applications, outdated_items_json, checked_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
            )
            .bind(&snapshot.certname)
            .bind(&snapshot.snapshot_id)
            .bind(is_stale)
            .bind(stale_reason)
            .bind(outdated_packages.len() as i64)
            .bind(outdated_applications.len() as i64)
            .bind(packages.len() as i64)
            .bind(applications.len() as i64)
            .bind(serde_json::to_string(&outdated_items).context("Failed to serialize outdated items")?)
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Failed to persist update status for '{}'", snapshot.certname))?;
        }

        let total_nodes = snapshots.len();
        Ok(InventoryFleetStatusSummary {
            total_nodes,
            stale_nodes,
            nodes_with_inventory: total_nodes,
            nodes_without_inventory: 0,
            outdated_nodes,
            outdated_packages: outdated_packages_total,
            outdated_applications: outdated_applications_total,
            generated_at: now,
        })
    }

    pub async fn get_host_update_status(&self, certname: &str) -> Result<Option<HostUpdateStatus>> {
        let row = sqlx::query_as::<_, HostUpdateStatusRow>(
            "SELECT * FROM host_update_status WHERE certname = ?1",
        )
        .bind(certname)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch host update status")?;

        Ok(row.map(Into::into))
    }

    pub async fn list_version_catalog(&self) -> Result<Vec<RepositoryVersionCatalogEntry>> {
        let rows = sqlx::query_as::<_, RepositoryVersionCatalogRow>(
            "SELECT * FROM repository_version_catalog ORDER BY platform_family, distribution, software_type, software_name",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch repository version catalog")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_fleet_status_summary(&self) -> Result<InventoryFleetStatusSummary> {
        let total_nodes = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM host_os_inventory")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count inventory nodes")? as usize;
        let stale_nodes = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM host_update_status WHERE is_stale = 1",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to count stale nodes")? as usize;
        let outdated_nodes = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM host_update_status WHERE outdated_packages > 0 OR outdated_applications > 0",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to count outdated nodes")? as usize;
        let status_rollup = sqlx::query_as::<_, UpdateStatusRollupRow>(
            "SELECT COALESCE(SUM(outdated_packages), 0) AS outdated_packages, COALESCE(SUM(outdated_applications), 0) AS outdated_applications FROM host_update_status",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to summarize update status")?;

        Ok(InventoryFleetStatusSummary {
            total_nodes,
            stale_nodes,
            nodes_with_inventory: total_nodes,
            nodes_without_inventory: 0,
            outdated_nodes,
            outdated_packages: status_rollup.outdated_packages.max(0) as usize,
            outdated_applications: status_rollup.outdated_applications.max(0) as usize,
            generated_at: Utc::now(),
        })
    }

    pub async fn get_dashboard_report(&self) -> Result<InventoryDashboardReport> {
        use std::collections::{HashMap, HashSet};

        let summary = self.get_fleet_status_summary().await?;
        let platform_distribution = sqlx::query_as::<_, DashboardDistributionRow>(
            r#"
            SELECT os_family AS label, COUNT(*) AS value
            FROM host_os_inventory
            GROUP BY os_family
            ORDER BY value DESC, os_family ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load platform distribution")?;
        let os_distribution = sqlx::query_as::<_, DashboardDistributionRow>(
            r#"
            SELECT
                CASE
                    WHEN TRIM(COALESCE(os_version, '')) = '' THEN distribution
                    ELSE distribution || ' ' || os_version
                END AS label,
                COUNT(*) AS value
            FROM host_os_inventory
            GROUP BY distribution, os_version
            ORDER BY value DESC, distribution ASC, os_version ASC
            LIMIT 8
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load OS distribution")?;
        let compliance_rows = sqlx::query_as::<_, DashboardComplianceRow>(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN is_stale = 1 THEN 1 ELSE 0 END), 0) AS stale_nodes,
                COALESCE(SUM(CASE WHEN is_stale = 0 AND (outdated_packages > 0 OR outdated_applications > 0) THEN 1 ELSE 0 END), 0) AS outdated_nodes,
                COALESCE(SUM(CASE WHEN is_stale = 0 AND outdated_packages = 0 AND outdated_applications = 0 THEN 1 ELSE 0 END), 0) AS compliant_nodes
            FROM host_update_status
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to load compliance distribution")?;
        let patch_rows = sqlx::query_as::<_, PatchAgeSourceRow>(
            r#"
            SELECT
                o.last_successful_update_at,
                o.last_inventory_at,
                (
                    SELECT MAX(p.install_time)
                    FROM host_package_inventory p
                    WHERE p.certname = o.certname
                      AND TRIM(COALESCE(p.install_time, '')) <> ''
                ) AS latest_package_install_at,
                (
                    SELECT MAX(a.install_date)
                    FROM host_application_inventory a
                    WHERE a.certname = o.certname
                      AND TRIM(COALESCE(a.install_date, '')) <> ''
                ) AS latest_application_install_at
            FROM host_os_inventory o
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load patch age data")?;
        let outdated_rows = sqlx::query_as::<_, DashboardOutdatedItemsRow>(
            "SELECT certname, outdated_items_json FROM host_update_status WHERE outdated_items_json IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load outdated item rollups")?;

        let mut outdated_nodes_per_software: HashMap<(String, String), HashSet<String>> =
            HashMap::new();
        for row in outdated_rows {
            let items: Vec<OutdatedInventoryItem> =
                serde_json::from_str(row.outdated_items_json.as_deref().unwrap_or("[]"))
                    .unwrap_or_default();
            for item in items {
                let key = (item.software_type, item.name);
                outdated_nodes_per_software
                    .entry(key)
                    .or_default()
                    .insert(row.certname.clone());
            }
        }

        let mut top_outdated_software: Vec<TopOutdatedSoftwareItem> = outdated_nodes_per_software
            .into_iter()
            .map(|((software_type, name), nodes)| TopOutdatedSoftwareItem {
                software_type,
                name,
                affected_nodes: nodes.len(),
            })
            .collect();
        top_outdated_software.sort_by(|left, right| {
            right
                .affected_nodes
                .cmp(&left.affected_nodes)
                .then_with(|| left.software_type.cmp(&right.software_type))
                .then_with(|| left.name.cmp(&right.name))
        });
        top_outdated_software.truncate(10);

        Ok(InventoryDashboardReport {
            summary,
            platform_distribution: map_distribution(platform_distribution),
            os_distribution: map_distribution(os_distribution),
            // Always include all compliance categories (even when 0) so the chart legend is complete
            update_compliance: vec![
                InventoryDistributionPoint {
                    label: "Compliant".to_string(),
                    value: compliance_rows.compliant_nodes.max(0) as usize,
                },
                InventoryDistributionPoint {
                    label: "Outdated".to_string(),
                    value: compliance_rows.outdated_nodes.max(0) as usize,
                },
                InventoryDistributionPoint {
                    label: "Stale".to_string(),
                    value: compliance_rows.stale_nodes.max(0) as usize,
                },
            ],
            patch_age_buckets: map_buckets(patch_rows),
            top_outdated_software,
        })
    }

    pub async fn get_nodes_for_outdated_software(
        &self,
        software_name: &str,
        software_type: Option<&str>,
    ) -> Result<Vec<OutdatedSoftwareNodeDetail>> {
        let rows = sqlx::query_as::<_, DashboardOutdatedItemsRow>(
            "SELECT certname, outdated_items_json FROM host_update_status WHERE outdated_items_json IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load outdated items for drill-down")?;

        let mut results: Vec<OutdatedSoftwareNodeDetail> = Vec::new();
        let mut seen_certnames = std::collections::HashSet::new();

        for row in rows {
            let items: Vec<OutdatedInventoryItem> =
                serde_json::from_str(row.outdated_items_json.as_deref().unwrap_or("[]"))
                    .unwrap_or_default();
            for item in items {
                if item.name == software_name
                    && software_type.map_or(true, |st| item.software_type == st)
                    && seen_certnames.insert(row.certname.clone())
                {
                    results.push(OutdatedSoftwareNodeDetail {
                        certname: row.certname.clone(),
                        installed_version: item.installed_version,
                        latest_version: item.latest_version,
                    });
                }
            }
        }

        results.sort_by(|a, b| a.certname.cmp(&b.certname));
        Ok(results)
    }

    pub async fn get_nodes_for_compliance_category(
        &self,
        category: &str,
    ) -> Result<Vec<ComplianceCategoryNode>> {
        let condition = match category {
            "stale" => "WHERE is_stale = 1",
            "outdated" => {
                "WHERE is_stale = 0 AND (outdated_packages > 0 OR outdated_applications > 0)"
            }
            "compliant" => {
                "WHERE is_stale = 0 AND outdated_packages = 0 AND outdated_applications = 0"
            }
            _ => return Err(anyhow::anyhow!("Invalid compliance category: {}", category)),
        };

        let query = format!(
            "SELECT certname, is_stale, outdated_packages, outdated_applications, checked_at FROM host_update_status {}",
            condition
        );

        let rows = sqlx::query_as::<_, ComplianceCategoryRow>(&query)
            .fetch_all(&self.pool)
            .await
            .context("Failed to load compliance category nodes")?;

        Ok(rows
            .into_iter()
            .map(|r| ComplianceCategoryNode {
                certname: r.certname,
                is_stale: r.is_stale != 0,
                outdated_packages: r.outdated_packages,
                outdated_applications: r.outdated_applications,
                checked_at: r.checked_at,
            })
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_update_job(
        &self,
        operation_type: UpdateOperationType,
        package_names: &[String],
        target_group_id: Option<&str>,
        certnames: &[String],
        requires_approval: bool,
        scheduled_for: Option<DateTime<Utc>>,
        maintenance_window_start: Option<DateTime<Utc>>,
        maintenance_window_end: Option<DateTime<Utc>>,
        requested_by: &str,
        approval_notes: Option<&str>,
    ) -> Result<UpdateJob> {
        let now = Utc::now();
        let job_id = Uuid::new_v4().to_string();
        let job_status = if requires_approval {
            UpdateJobStatus::PendingApproval
        } else {
            UpdateJobStatus::Approved
        };
        let target_status = if requires_approval {
            UpdateTargetStatus::PendingApproval
        } else {
            UpdateTargetStatus::Queued
        };

        let mut tx = self
            .pool
            .begin()
            .await
            .context("Failed to begin update job transaction")?;

        sqlx::query(
            r#"
            INSERT INTO update_jobs (
                id, status, operation_type, package_names_json, target_group_id, requires_approval,
                scheduled_for, maintenance_window_start, maintenance_window_end, requested_by,
                approved_by, approval_notes, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
        )
        .bind(&job_id)
        .bind(job_status.as_str())
        .bind(operation_type.as_str())
        .bind(serde_json::to_string(package_names).context("Failed to serialize package names")?)
        .bind(target_group_id)
        .bind(requires_approval)
        .bind(scheduled_for.map(|ts| ts.to_rfc3339()))
        .bind(maintenance_window_start.map(|ts| ts.to_rfc3339()))
        .bind(maintenance_window_end.map(|ts| ts.to_rfc3339()))
        .bind(requested_by)
        .bind(None::<String>)
        .bind(approval_notes)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await
        .context("Failed to insert update job")?;

        for certname in certnames {
            sqlx::query(
                r#"
                INSERT INTO update_job_targets (
                    id, job_id, certname, status, dispatched_at, completed_at,
                    last_error, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5, ?6)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&job_id)
            .bind(certname)
            .bind(target_status.as_str())
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to insert update target '{}'", certname))?;
        }

        tx.commit()
            .await
            .context("Failed to commit update job transaction")?;

        self.get_update_job(&job_id)
            .await?
            .context("Update job was created but could not be reloaded")
    }

    pub async fn list_update_jobs(&self, limit: usize) -> Result<Vec<UpdateJob>> {
        let rows = sqlx::query_as::<_, UpdateJobRow>(
            r#"
            SELECT *
            FROM update_jobs
            ORDER BY datetime(created_at) DESC
            LIMIT ?1
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch update jobs")?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            jobs.push(self.load_update_job(row).await?);
        }
        Ok(jobs)
    }

    pub async fn get_update_job(&self, job_id: &str) -> Result<Option<UpdateJob>> {
        let row = sqlx::query_as::<_, UpdateJobRow>("SELECT * FROM update_jobs WHERE id = ?1")
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to fetch update job")?;

        match row {
            Some(row) => Ok(Some(self.load_update_job(row).await?)),
            None => Ok(None),
        }
    }

    pub async fn approve_update_job(
        &self,
        job_id: &str,
        approved: bool,
        approved_by: &str,
        notes: Option<&str>,
    ) -> Result<Option<UpdateJob>> {
        let current = self.get_update_job(job_id).await?;
        let Some(job) = current else {
            return Ok(None);
        };
        if job.status != UpdateJobStatus::PendingApproval {
            anyhow::bail!("Update job is not pending approval");
        }

        let now = Utc::now();
        let new_job_status = if approved {
            UpdateJobStatus::Approved
        } else {
            UpdateJobStatus::Rejected
        };
        let new_target_status = if approved {
            UpdateTargetStatus::Queued
        } else {
            UpdateTargetStatus::Rejected
        };

        let mut tx = self
            .pool
            .begin()
            .await
            .context("Failed to begin update job approval transaction")?;

        sqlx::query(
            "UPDATE update_jobs SET status = ?1, approved_by = ?2, approval_notes = ?3, updated_at = ?4 WHERE id = ?5",
        )
        .bind(new_job_status.as_str())
        .bind(approved_by)
        .bind(notes)
        .bind(now.to_rfc3339())
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .context("Failed to update update job approval state")?;

        sqlx::query(
            "UPDATE update_job_targets SET status = ?1, updated_at = ?2 WHERE job_id = ?3 AND status = 'pending_approval'",
        )
        .bind(new_target_status.as_str())
        .bind(now.to_rfc3339())
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .context("Failed to update update job targets after approval")?;

        tx.commit()
            .await
            .context("Failed to commit update job approval transaction")?;

        self.get_update_job(job_id).await
    }

    pub async fn claim_pending_updates_for_node(
        &self,
        certname: &str,
    ) -> Result<Vec<NodePendingUpdateJob>> {
        let now = Utc::now();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("Failed to begin pending update claim transaction")?;

        let rows = sqlx::query_as::<_, PendingNodeUpdateRow>(
            r#"
            SELECT
                t.id AS target_id,
                t.job_id,
                j.operation_type,
                j.package_names_json,
                j.scheduled_for,
                j.maintenance_window_start,
                j.maintenance_window_end
            FROM update_job_targets t
            INNER JOIN update_jobs j ON j.id = t.job_id
            WHERE t.certname = ?1
              AND t.status = 'queued'
              AND j.status = 'approved'
              AND (j.scheduled_for IS NULL OR datetime(j.scheduled_for) <= datetime(?2))
            ORDER BY datetime(j.created_at) ASC
            "#,
        )
        .bind(certname)
        .bind(now.to_rfc3339())
        .fetch_all(&mut *tx)
        .await
        .context("Failed to fetch pending node updates")?;

        for row in &rows {
            sqlx::query(
                "UPDATE update_job_targets SET status = 'dispatched', dispatched_at = ?1, updated_at = ?1 WHERE id = ?2",
            )
            .bind(now.to_rfc3339())
            .bind(&row.target_id)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to dispatch target '{}'", row.target_id))?;

            sqlx::query(
                "UPDATE update_jobs SET status = 'in_progress', updated_at = ?1 WHERE id = ?2 AND status = 'approved'",
            )
            .bind(now.to_rfc3339())
            .bind(&row.job_id)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to transition job '{}' to in_progress", row.job_id))?;
        }

        tx.commit()
            .await
            .context("Failed to commit pending update claim transaction")?;

        Ok(rows
            .into_iter()
            .map(|row| NodePendingUpdateJob {
                job_id: row.job_id,
                target_id: row.target_id,
                operation_type: UpdateOperationType::from_str(&row.operation_type)
                    .unwrap_or_default(),
                package_names: serde_json::from_str(&row.package_names_json).unwrap_or_default(),
                scheduled_for: row.scheduled_for.as_deref().and_then(parse_timestamp),
                maintenance_window_start: row
                    .maintenance_window_start
                    .as_deref()
                    .and_then(parse_timestamp),
                maintenance_window_end: row
                    .maintenance_window_end
                    .as_deref()
                    .and_then(parse_timestamp),
            })
            .collect())
    }

    pub async fn submit_update_job_result(
        &self,
        job_id: &str,
        target_id: &str,
        certname: &str,
        request: &SubmitUpdateJobResultRequest,
    ) -> Result<Option<UpdateJob>> {
        let target = sqlx::query_as::<_, UpdateJobTargetStateRow>(
            "SELECT id, job_id, certname, status FROM update_job_targets WHERE id = ?1 AND job_id = ?2 AND certname = ?3",
        )
        .bind(target_id)
        .bind(job_id)
        .bind(certname)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch update target for result submission")?;

        let Some(target) = target else {
            return Ok(None);
        };

        let result_status = request.status;
        if !matches!(
            result_status,
            UpdateTargetStatus::Succeeded
                | UpdateTargetStatus::Failed
                | UpdateTargetStatus::Cancelled
        ) {
            anyhow::bail!("Node result status must be succeeded, failed, or cancelled");
        }

        if !matches!(
            UpdateTargetStatus::from_str(&target.status).unwrap_or_default(),
            UpdateTargetStatus::Dispatched | UpdateTargetStatus::Queued
        ) {
            anyhow::bail!("Update target is not dispatchable");
        }

        let now = Utc::now();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("Failed to begin update result transaction")?;

        sqlx::query(
            r#"
            INSERT INTO update_job_results (
                id, job_id, target_id, certname, status, summary, output,
                started_at, finished_at, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(job_id)
        .bind(target_id)
        .bind(certname)
        .bind(result_status.as_str())
        .bind(&request.summary)
        .bind(&request.output)
        .bind(request.started_at.map(|ts| ts.to_rfc3339()))
        .bind(request.finished_at.unwrap_or(now).to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await
        .context("Failed to insert update job result")?;

        sqlx::query(
            "UPDATE update_job_targets SET status = ?1, completed_at = ?2, last_error = ?3, updated_at = ?2 WHERE id = ?4",
        )
        .bind(result_status.as_str())
        .bind(now.to_rfc3339())
        .bind(if result_status == UpdateTargetStatus::Failed {
            request.summary.clone().or_else(|| request.output.clone())
        } else {
            None
        })
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .context("Failed to update target status after result submission")?;

        let target_rows = sqlx::query_as::<_, UpdateJobTargetStateRow>(
            "SELECT id, job_id, certname, status FROM update_job_targets WHERE job_id = ?1",
        )
        .bind(job_id)
        .fetch_all(&mut *tx)
        .await
        .context("Failed to fetch target states for rollup")?;

        let rolled_up_status = roll_up_update_job_status(&target_rows);
        sqlx::query("UPDATE update_jobs SET status = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(rolled_up_status.as_str())
            .bind(now.to_rfc3339())
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .context("Failed to update rolled-up job status")?;

        tx.commit()
            .await
            .context("Failed to commit update result transaction")?;

        self.get_update_job(job_id).await
    }

    async fn load_update_job(&self, row: UpdateJobRow) -> Result<UpdateJob> {
        let targets = sqlx::query_as::<_, UpdateJobTargetRow>(
            r#"
            SELECT id, certname, status, dispatched_at, completed_at, last_error
            FROM update_job_targets
            WHERE job_id = ?1
            ORDER BY certname ASC
            "#,
        )
        .bind(&row.id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load update job targets")?;

        let results = sqlx::query_as::<_, UpdateJobResultRow>(
            r#"
            SELECT id, target_id, certname, status, summary, output, started_at, finished_at
            FROM update_job_results
            WHERE job_id = ?1
            ORDER BY datetime(created_at) DESC
            "#,
        )
        .bind(&row.id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load update job results")?;

        Ok(UpdateJob {
            id: row.id,
            status: UpdateJobStatus::from_str(&row.status).unwrap_or_default(),
            operation_type: UpdateOperationType::from_str(&row.operation_type).unwrap_or_default(),
            package_names: serde_json::from_str(&row.package_names_json).unwrap_or_default(),
            target_group_id: row.target_group_id,
            target_nodes: targets
                .iter()
                .map(|target| target.certname.clone())
                .collect(),
            requires_approval: row.requires_approval,
            scheduled_for: row.scheduled_for.as_deref().and_then(parse_timestamp),
            maintenance_window_start: row
                .maintenance_window_start
                .as_deref()
                .and_then(parse_timestamp),
            maintenance_window_end: row
                .maintenance_window_end
                .as_deref()
                .and_then(parse_timestamp),
            requested_by: row.requested_by,
            approved_by: row.approved_by,
            approval_notes: row.approval_notes,
            created_at: parse_timestamp_required(&row.created_at),
            updated_at: parse_timestamp_required(&row.updated_at),
            targets: targets
                .into_iter()
                .map(|target| UpdateJobTarget {
                    id: target.id,
                    certname: target.certname,
                    status: UpdateTargetStatus::from_str(&target.status).unwrap_or_default(),
                    dispatched_at: target.dispatched_at.as_deref().and_then(parse_timestamp),
                    completed_at: target.completed_at.as_deref().and_then(parse_timestamp),
                    last_error: target.last_error,
                })
                .collect(),
            results: results
                .into_iter()
                .map(|result| UpdateJobResult {
                    id: result.id,
                    target_id: result.target_id,
                    certname: result.certname,
                    status: UpdateTargetStatus::from_str(&result.status).unwrap_or_default(),
                    summary: result.summary,
                    output: result.output,
                    started_at: result.started_at.as_deref().and_then(parse_timestamp),
                    finished_at: parse_timestamp_required(&result.finished_at),
                })
                .collect(),
        })
    }

    async fn replace_packages(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        certname: &str,
        snapshot_id: &str,
        packages: &[HostPackageInventoryItem],
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM host_package_inventory WHERE certname = ?1")
            .bind(certname)
            .execute(&mut **tx)
            .await
            .context("Failed to clear package inventory")?;

        for package in packages {
            sqlx::query(
                r#"
                INSERT INTO host_package_inventory (
                    id, certname, snapshot_id, name, epoch, version, release,
                    architecture, repository_source, install_path, install_time, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(certname)
            .bind(snapshot_id)
            .bind(&package.name)
            .bind(&package.epoch)
            .bind(&package.version)
            .bind(&package.release)
            .bind(&package.architecture)
            .bind(&package.repository_source)
            .bind(&package.install_path)
            .bind(package.install_time.map(|ts| ts.to_rfc3339()))
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await
            .with_context(|| {
                format!("Failed to insert package inventory item '{}'", package.name)
            })?;
        }

        Ok(())
    }

    async fn replace_applications(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        certname: &str,
        snapshot_id: &str,
        applications: &[HostApplicationInventoryItem],
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM host_application_inventory WHERE certname = ?1")
            .bind(certname)
            .execute(&mut **tx)
            .await
            .context("Failed to clear application inventory")?;

        for application in applications {
            sqlx::query(
                r#"
                INSERT INTO host_application_inventory (
                    id, certname, snapshot_id, name, publisher, version, architecture,
                    install_scope, install_path, application_type, bundle_identifier,
                    uninstall_identity, install_date, metadata_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(certname)
            .bind(snapshot_id)
            .bind(&application.name)
            .bind(&application.publisher)
            .bind(&application.version)
            .bind(&application.architecture)
            .bind(&application.install_scope)
            .bind(&application.install_path)
            .bind(&application.application_type)
            .bind(&application.bundle_identifier)
            .bind(&application.uninstall_identity)
            .bind(application.install_date.map(|ts| ts.to_rfc3339()))
            .bind(application.metadata.as_ref().map(|value| value.to_string()))
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await
            .with_context(|| {
                format!(
                    "Failed to insert application inventory item '{}'",
                    application.name
                )
            })?;
        }

        Ok(())
    }

    async fn replace_websites(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        certname: &str,
        snapshot_id: &str,
        websites: &[HostWebInventoryItem],
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM host_web_inventory WHERE certname = ?1")
            .bind(certname)
            .execute(&mut **tx)
            .await
            .context("Failed to clear website inventory")?;

        for website in websites {
            sqlx::query(
                r#"
                INSERT INTO host_web_inventory (
                    id, certname, snapshot_id, server_type, site_name, bindings_json,
                    document_root, application_pool, tls_certificate_reference, metadata_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(certname)
            .bind(snapshot_id)
            .bind(&website.server_type)
            .bind(&website.site_name)
            .bind(serde_json::to_string(&website.bindings).context("Failed to serialize bindings")?)
            .bind(&website.document_root)
            .bind(&website.application_pool)
            .bind(&website.tls_certificate_reference)
            .bind(website.metadata.as_ref().map(|value| value.to_string()))
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await
            .with_context(|| format!("Failed to insert website inventory item '{}'", website.site_name))?;
        }

        Ok(())
    }

    async fn replace_runtimes(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        certname: &str,
        snapshot_id: &str,
        runtimes: &[HostRuntimeInventoryItem],
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM host_runtime_inventory WHERE certname = ?1")
            .bind(certname)
            .execute(&mut **tx)
            .await
            .context("Failed to clear runtime inventory")?;

        for runtime in runtimes {
            sqlx::query(
                r#"
                INSERT INTO host_runtime_inventory (
                    id, certname, snapshot_id, runtime_type, runtime_name, runtime_version,
                    install_path, management_endpoint, deployed_units_json, metadata_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(certname)
            .bind(snapshot_id)
            .bind(&runtime.runtime_type)
            .bind(&runtime.runtime_name)
            .bind(&runtime.runtime_version)
            .bind(&runtime.install_path)
            .bind(&runtime.management_endpoint)
            .bind(
                serde_json::to_string(&runtime.deployed_units)
                    .context("Failed to serialize deployed units")?,
            )
            .bind(runtime.metadata.as_ref().map(|value| value.to_string()))
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await
            .with_context(|| format!("Failed to insert runtime inventory item '{}'", runtime.runtime_name))?;
        }

        Ok(())
    }

    async fn replace_containers(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        certname: &str,
        snapshot_id: &str,
        containers: &[HostContainerInventoryItem],
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM host_container_inventory WHERE certname = ?1")
            .bind(certname)
            .execute(&mut **tx)
            .await
            .context("Failed to clear container inventory")?;

        for container in containers {
            sqlx::query(
                r#"
                INSERT INTO host_container_inventory (
                    id, certname, snapshot_id, container_id, name, image, status, status_detail,
                    created_at, ports_json, mounts_json, runtime_type, metadata_json, row_created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(certname)
            .bind(snapshot_id)
            .bind(&container.container_id)
            .bind(&container.name)
            .bind(container.image.as_deref().unwrap_or(""))
            .bind(&container.status)
            .bind(&container.status_detail)
            .bind(&container.created_at)
            .bind(
                serde_json::to_string(&container.ports)
                    .context("Failed to serialize container ports")?,
            )
            .bind(
                serde_json::to_string(&container.mounts)
                    .context("Failed to serialize container mounts")?,
            )
            .bind(&container.runtime_type)
            .bind(container.metadata.as_ref().map(|value| value.to_string()))
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await
            .with_context(|| {
                format!(
                    "Failed to insert container inventory item '{}'",
                    container.name
                )
            })?;
        }

        Ok(())
    }

    async fn replace_users(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        certname: &str,
        snapshot_id: &str,
        users: &[HostUserInventoryItem],
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM host_user_inventory WHERE certname = ?1")
            .bind(certname)
            .execute(&mut **tx)
            .await
            .context("Failed to clear user inventory")?;

        for user in users {
            sqlx::query(
                r#"
                INSERT INTO host_user_inventory (
                    id, certname, snapshot_id, username, uid, sid, gid, home_directory, shell,
                    user_type, groups_json, last_login, locked, gecos, metadata_json, row_created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(certname)
            .bind(snapshot_id)
            .bind(&user.username)
            .bind(user.uid)
            .bind(&user.sid)
            .bind(user.gid)
            .bind(&user.home_directory)
            .bind(&user.shell)
            .bind(&user.user_type)
            .bind(serde_json::to_string(&user.groups).context("Failed to serialize user groups")?)
            .bind(&user.last_login)
            .bind(user.locked.map(|b| b as i64))
            .bind(&user.gecos)
            .bind(user.metadata.as_ref().map(|value| value.to_string()))
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await
            .with_context(|| format!("Failed to insert user inventory item '{}'", user.username))?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn replace_repositories(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        certname: &str,
        snapshot_id: &str,
        os_family: &str,
        distribution: &str,
        os_version: &str,
        package_manager: &str,
        repositories: &[HostRepositoryConfig],
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query("DELETE FROM node_repository_configs WHERE certname = ?1")
            .bind(certname)
            .execute(&mut **tx)
            .await
            .context("Failed to clear node repository configs")?;

        for repo in repositories {
            sqlx::query(
                r#"
                INSERT INTO node_repository_configs (
                    id, certname, snapshot_id, os_family, distribution, os_version,
                    package_manager, repo_id, repo_name, repo_type, base_url,
                    mirror_list_url, distribution_path, components, architectures,
                    enabled, gpg_check, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                ON CONFLICT(certname, repo_id, package_manager) DO UPDATE SET
                    snapshot_id = excluded.snapshot_id,
                    os_family = excluded.os_family,
                    distribution = excluded.distribution,
                    os_version = excluded.os_version,
                    repo_name = excluded.repo_name,
                    repo_type = excluded.repo_type,
                    base_url = excluded.base_url,
                    mirror_list_url = excluded.mirror_list_url,
                    distribution_path = excluded.distribution_path,
                    components = excluded.components,
                    architectures = excluded.architectures,
                    enabled = excluded.enabled,
                    gpg_check = excluded.gpg_check,
                    created_at = excluded.created_at
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(certname)
            .bind(snapshot_id)
            .bind(os_family)
            .bind(distribution)
            .bind(os_version)
            .bind(package_manager)
            .bind(&repo.repo_id)
            .bind(&repo.repo_name)
            .bind(&repo.repo_type)
            .bind(&repo.base_url)
            .bind(&repo.mirror_list_url)
            .bind(&repo.distribution_path)
            .bind(&repo.components)
            .bind(&repo.architectures)
            .bind(repo.enabled)
            .bind(repo.gpg_check.map(|b| b as i64))
            .bind(now.to_rfc3339())
            .execute(&mut **tx)
            .await
            .with_context(|| {
                format!(
                    "Failed to insert repository config '{}'",
                    repo.repo_id
                )
            })?;
        }

        Ok(())
    }

    /// Deduplicate node repository configs into fleet-wide configs.
    /// Groups by (os_family, distribution, major_version, package_manager, repo_id)
    /// and upserts into fleet_repository_configs.
    pub async fn refresh_fleet_repository_configs(&self) -> Result<usize> {
        let now = Utc::now();

        // Aggregate unique repos across all nodes, grouping by major version
        let rows = sqlx::query_as::<_, FleetRepoAggregateRow>(
            r#"
            SELECT
                os_family,
                distribution,
                -- Extract major version: take everything before the first '.'
                CASE
                    WHEN INSTR(os_version, '.') > 0
                    THEN SUBSTR(os_version, 1, INSTR(os_version, '.') - 1)
                    ELSE os_version
                END AS os_version_pattern,
                package_manager,
                repo_id,
                repo_type,
                -- Pick the most common values
                MAX(repo_name) AS repo_name,
                MAX(base_url) AS base_url,
                MAX(mirror_list_url) AS mirror_list_url,
                MAX(distribution_path) AS distribution_path,
                MAX(components) AS components,
                MAX(architectures) AS architectures,
                COUNT(DISTINCT certname) AS reporting_nodes
            FROM node_repository_configs
            WHERE enabled = 1
            GROUP BY os_family, distribution,
                     CASE
                         WHEN INSTR(os_version, '.') > 0
                         THEN SUBSTR(os_version, 1, INSTR(os_version, '.') - 1)
                         ELSE os_version
                     END,
                     package_manager, repo_id, repo_type
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to aggregate node repository configs")?;

        let mut upserted = 0usize;

        for row in &rows {
            sqlx::query(
                r#"
                INSERT INTO fleet_repository_configs (
                    id, os_family, distribution, os_version_pattern, package_manager,
                    repo_id, repo_name, repo_type, base_url, mirror_list_url,
                    distribution_path, components, architectures, enabled,
                    reporting_nodes, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14, ?15, ?16)
                ON CONFLICT(os_family, distribution, os_version_pattern, package_manager, repo_id)
                DO UPDATE SET
                    repo_name = excluded.repo_name,
                    repo_type = excluded.repo_type,
                    base_url = excluded.base_url,
                    mirror_list_url = excluded.mirror_list_url,
                    distribution_path = excluded.distribution_path,
                    components = excluded.components,
                    architectures = excluded.architectures,
                    reporting_nodes = excluded.reporting_nodes,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&row.os_family)
            .bind(&row.distribution)
            .bind(&row.os_version_pattern)
            .bind(&row.package_manager)
            .bind(&row.repo_id)
            .bind(&row.repo_name)
            .bind(&row.repo_type)
            .bind(&row.base_url)
            .bind(&row.mirror_list_url)
            .bind(&row.distribution_path)
            .bind(&row.components)
            .bind(&row.architectures)
            .bind(row.reporting_nodes)
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Failed to upsert fleet repository config '{}'", row.repo_id)
            })?;
            upserted += 1;
        }

        Ok(upserted)
    }

    /// List all fleet repository configs.
    pub async fn list_fleet_repository_configs(&self) -> Result<Vec<FleetRepositoryConfig>> {
        let rows = sqlx::query_as::<_, FleetRepositoryConfigRow>(
            "SELECT * FROM fleet_repository_configs WHERE enabled = 1 ORDER BY os_family, distribution, repo_id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to list fleet repository configs")?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Update the check status for a fleet repository config.
    pub async fn update_fleet_repo_check_status(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE fleet_repository_configs
            SET last_checked_at = ?1, last_check_status = ?2, last_check_error = ?3, updated_at = ?4
            WHERE id = ?5
            "#,
        )
        .bind(now.to_rfc3339())
        .bind(status)
        .bind(error)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update fleet repo check status")?;
        Ok(())
    }

    async fn insert_catalog_entry(
        &self,
        entry: &RepositoryVersionCatalogEntry,
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO repository_version_catalog (
                id, platform_family, distribution, os_version_pattern, package_manager, software_type,
                software_name, repository_source, latest_version, latest_release, source_kind,
                observed_nodes, last_seen_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.platform_family)
        .bind(&entry.distribution)
        .bind(&entry.os_version_pattern)
        .bind(&entry.package_manager)
        .bind(&entry.software_type)
        .bind(&entry.software_name)
        .bind(&entry.repository_source)
        .bind(&entry.latest_version)
        .bind(&entry.latest_release)
        .bind(&entry.source_kind)
        .bind(entry.observed_nodes as i64)
        .bind(entry.last_seen_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .with_context(|| format!("Failed to insert catalog entry '{}'", entry.software_name))?;

        Ok(())
    }

    /// Upsert a catalog entry, using ON CONFLICT to update if already present.
    /// Used by the repo checker to insert "repo-checked" catalog entries.
    pub async fn upsert_catalog_entry(&self, entry: &RepositoryVersionCatalogEntry) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO repository_version_catalog (
                id, platform_family, distribution, os_version_pattern, package_manager, software_type,
                software_name, repository_source, latest_version, latest_release, source_kind,
                observed_nodes, last_seen_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(platform_family, distribution, package_manager, software_type,
                        software_name, COALESCE(repository_source, ''), source_kind,
                        COALESCE(os_version_pattern, ''))
            DO UPDATE SET
                latest_version = excluded.latest_version,
                latest_release = excluded.latest_release,
                observed_nodes = excluded.observed_nodes,
                last_seen_at = excluded.last_seen_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.platform_family)
        .bind(&entry.distribution)
        .bind(&entry.os_version_pattern)
        .bind(&entry.package_manager)
        .bind(&entry.software_type)
        .bind(&entry.software_name)
        .bind(&entry.repository_source)
        .bind(&entry.latest_version)
        .bind(&entry.latest_release)
        .bind(&entry.source_kind)
        .bind(entry.observed_nodes as i64)
        .bind(entry.last_seen_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .with_context(|| format!("Failed to upsert catalog entry '{}'", entry.software_name))?;

        Ok(())
    }

    // ── Group Update Schedules ──────────────────────────────────────

    pub async fn list_group_update_schedules(
        &self,
        group_id: &str,
    ) -> Result<Vec<GroupUpdateSchedule>> {
        let rows = sqlx::query_as::<_, GroupUpdateScheduleRow>(
            "SELECT * FROM group_update_schedules WHERE group_id = ?1 ORDER BY created_at DESC",
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list group update schedules")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_group_update_schedule(&self, id: &str) -> Result<Option<GroupUpdateSchedule>> {
        let row = sqlx::query_as::<_, GroupUpdateScheduleRow>(
            "SELECT * FROM group_update_schedules WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch group update schedule")?;

        Ok(row.map(Into::into))
    }

    pub async fn create_group_update_schedule(
        &self,
        group_id: &str,
        request: &CreateGroupUpdateScheduleRequest,
        created_by: &str,
    ) -> Result<GroupUpdateSchedule> {
        use crate::services::scheduler::{calculate_next_run, validate_cron_expression};

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        // Validate and compute next_run_at
        let next_run_at = match request.schedule_type.as_str() {
            "recurring" => {
                let cron_expr = request.cron_expression.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("cron_expression is required for recurring schedules")
                })?;
                validate_cron_expression(cron_expr).map_err(|e| anyhow::anyhow!("{}", e))?;
                calculate_next_run(cron_expr, "UTC")
            }
            "one_time" => request.scheduled_for,
            _ => anyhow::bail!("Invalid schedule_type: must be 'one_time' or 'recurring'"),
        };

        let package_names_json = serde_json::to_string(&request.package_names)
            .context("Failed to serialize package names")?;

        sqlx::query(
            r#"
            INSERT INTO group_update_schedules (
                id, group_id, name, description, schedule_type, cron_expression, scheduled_for,
                operation_type, package_names_json, requires_approval,
                maintenance_window_start, maintenance_window_end,
                enabled, last_run_at, next_run_at, last_job_id,
                created_by, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, NULL, ?13, NULL, ?14, ?15, ?16)
            "#,
        )
        .bind(&id)
        .bind(group_id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.schedule_type)
        .bind(&request.cron_expression)
        .bind(request.scheduled_for.map(|ts| ts.to_rfc3339()))
        .bind(request.operation_type.as_str())
        .bind(&package_names_json)
        .bind(request.requires_approval)
        .bind(&request.maintenance_window_start)
        .bind(&request.maintenance_window_end)
        .bind(next_run_at.map(|ts| ts.to_rfc3339()))
        .bind(created_by)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("Failed to create group update schedule")?;

        self.get_group_update_schedule(&id)
            .await?
            .context("Schedule was created but could not be reloaded")
    }

    pub async fn update_group_update_schedule(
        &self,
        id: &str,
        request: &UpdateGroupUpdateScheduleRequest,
    ) -> Result<Option<GroupUpdateSchedule>> {
        use crate::services::scheduler::{calculate_next_run, validate_cron_expression};

        let existing = match self.get_group_update_schedule(id).await? {
            Some(s) => s,
            None => return Ok(None),
        };

        let now = Utc::now();
        let name = request.name.as_deref().unwrap_or(&existing.name);
        let description = request
            .description
            .as_deref()
            .or(existing.description.as_deref());
        let operation_type = request.operation_type.unwrap_or(existing.operation_type);
        let requires_approval = request
            .requires_approval
            .unwrap_or(existing.requires_approval);
        let enabled = request.enabled.unwrap_or(existing.enabled);
        let mw_start = request
            .maintenance_window_start
            .as_deref()
            .or(existing.maintenance_window_start.as_deref());
        let mw_end = request
            .maintenance_window_end
            .as_deref()
            .or(existing.maintenance_window_end.as_deref());
        let package_names = request
            .package_names
            .as_ref()
            .unwrap_or(&existing.package_names);
        let package_names_json =
            serde_json::to_string(package_names).context("Failed to serialize package names")?;

        // Recompute next_run_at if schedule changed
        let cron_expression = request
            .cron_expression
            .as_deref()
            .or(existing.cron_expression.as_deref());
        let scheduled_for = request.scheduled_for.or(existing.scheduled_for);

        let next_run_at = if enabled {
            match existing.schedule_type.as_str() {
                "recurring" => {
                    if let Some(cron_expr) = cron_expression {
                        validate_cron_expression(cron_expr)
                            .map_err(|e| anyhow::anyhow!("{}", e))?;
                        calculate_next_run(cron_expr, "UTC")
                    } else {
                        existing.next_run_at
                    }
                }
                "one_time" => scheduled_for,
                _ => existing.next_run_at,
            }
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE group_update_schedules SET
                name = ?1, description = ?2, cron_expression = ?3, scheduled_for = ?4,
                operation_type = ?5, package_names_json = ?6, requires_approval = ?7,
                maintenance_window_start = ?8, maintenance_window_end = ?9,
                enabled = ?10, next_run_at = ?11, updated_at = ?12
            WHERE id = ?13
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(cron_expression)
        .bind(scheduled_for.map(|ts| ts.to_rfc3339()))
        .bind(operation_type.as_str())
        .bind(&package_names_json)
        .bind(requires_approval)
        .bind(mw_start)
        .bind(mw_end)
        .bind(enabled)
        .bind(next_run_at.map(|ts| ts.to_rfc3339()))
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update group update schedule")?;

        self.get_group_update_schedule(id).await
    }

    pub async fn delete_group_update_schedule(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM group_update_schedules WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete group update schedule")?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_due_update_schedules(&self) -> Result<Vec<GroupUpdateSchedule>> {
        let now = Utc::now().to_rfc3339();
        let rows = sqlx::query_as::<_, GroupUpdateScheduleRow>(
            r#"
            SELECT * FROM group_update_schedules
            WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?1
            ORDER BY next_run_at ASC
            "#,
        )
        .bind(&now)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch due update schedules")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn update_schedule_after_run(
        &self,
        id: &str,
        last_run_at: DateTime<Utc>,
        next_run_at: Option<DateTime<Utc>>,
        last_job_id: &str,
        disable: bool,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE group_update_schedules SET
                last_run_at = ?1, next_run_at = ?2, last_job_id = ?3,
                enabled = CASE WHEN ?4 THEN 0 ELSE enabled END,
                updated_at = ?5
            WHERE id = ?6
            "#,
        )
        .bind(last_run_at.to_rfc3339())
        .bind(next_run_at.map(|ts| ts.to_rfc3339()))
        .bind(last_job_id)
        .bind(disable)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update schedule after run")?;

        Ok(())
    }
}

#[derive(Debug, FromRow)]
struct GroupUpdateScheduleRow {
    id: String,
    group_id: String,
    name: String,
    description: Option<String>,
    schedule_type: String,
    cron_expression: Option<String>,
    scheduled_for: Option<String>,
    operation_type: String,
    package_names_json: String,
    requires_approval: bool,
    maintenance_window_start: Option<String>,
    maintenance_window_end: Option<String>,
    enabled: bool,
    last_run_at: Option<String>,
    next_run_at: Option<String>,
    last_job_id: Option<String>,
    created_by: String,
    created_at: String,
    updated_at: String,
}

impl From<GroupUpdateScheduleRow> for GroupUpdateSchedule {
    fn from(row: GroupUpdateScheduleRow) -> Self {
        Self {
            id: row.id,
            group_id: row.group_id,
            name: row.name,
            description: row.description,
            schedule_type: row.schedule_type,
            cron_expression: row.cron_expression,
            scheduled_for: row.scheduled_for.as_deref().map(parse_timestamp_required),
            operation_type: UpdateOperationType::from_str(&row.operation_type)
                .unwrap_or(UpdateOperationType::SystemPatch),
            package_names: serde_json::from_str(&row.package_names_json).unwrap_or_default(),
            requires_approval: row.requires_approval,
            maintenance_window_start: row.maintenance_window_start,
            maintenance_window_end: row.maintenance_window_end,
            enabled: row.enabled,
            last_run_at: row.last_run_at.as_deref().map(parse_timestamp_required),
            next_run_at: row.next_run_at.as_deref().map(parse_timestamp_required),
            last_job_id: row.last_job_id,
            created_by: row.created_by,
            created_at: parse_timestamp_required(&row.created_at),
            updated_at: parse_timestamp_required(&row.updated_at),
        }
    }
}

#[derive(Debug, FromRow)]
struct InventorySnapshotRow {
    id: String,
    certname: String,
    collector_version: String,
    collected_at: String,
    is_full_snapshot: bool,
    os_family: String,
    distribution: String,
    os_version: String,
    package_count: i64,
    application_count: i64,
    website_count: i64,
    runtime_count: i64,
    container_count: i64,
    user_count: i64,
    created_at: String,
}

impl From<InventorySnapshotRow> for InventorySnapshotSummary {
    fn from(row: InventorySnapshotRow) -> Self {
        Self {
            id: row.id,
            certname: row.certname,
            collector_version: row.collector_version,
            collected_at: parse_timestamp_required(&row.collected_at),
            is_full_snapshot: row.is_full_snapshot,
            os_family: row.os_family,
            distribution: row.distribution,
            os_version: row.os_version,
            package_count: row.package_count.max(0) as usize,
            application_count: row.application_count.max(0) as usize,
            website_count: row.website_count.max(0) as usize,
            runtime_count: row.runtime_count.max(0) as usize,
            container_count: row.container_count.max(0) as usize,
            user_count: row.user_count.max(0) as usize,
            created_at: parse_timestamp_required(&row.created_at),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostOsInventoryRow {
    os_family: String,
    distribution: String,
    edition: Option<String>,
    architecture: Option<String>,
    kernel_version: Option<String>,
    os_version: String,
    patch_level: Option<String>,
    package_manager: Option<String>,
    update_channel: Option<String>,
    last_inventory_at: Option<String>,
    last_successful_update_at: Option<String>,
}

impl From<HostOsInventoryRow> for HostOsInventory {
    fn from(row: HostOsInventoryRow) -> Self {
        Self {
            os_family: row.os_family,
            distribution: row.distribution,
            edition: row.edition,
            architecture: row.architecture,
            kernel_version: row.kernel_version,
            os_version: row.os_version,
            patch_level: row.patch_level,
            package_manager: row.package_manager,
            update_channel: row.update_channel,
            last_inventory_at: row.last_inventory_at.as_deref().and_then(parse_timestamp),
            last_successful_update_at: row
                .last_successful_update_at
                .as_deref()
                .and_then(parse_timestamp),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostPackageInventoryRow {
    name: String,
    epoch: Option<String>,
    version: String,
    release: Option<String>,
    architecture: Option<String>,
    repository_source: Option<String>,
    install_path: Option<String>,
    install_time: Option<String>,
}

impl From<HostPackageInventoryRow> for HostPackageInventoryItem {
    fn from(row: HostPackageInventoryRow) -> Self {
        Self {
            name: row.name,
            epoch: row.epoch,
            version: row.version,
            release: row.release,
            architecture: row.architecture,
            repository_source: row.repository_source,
            install_path: row.install_path,
            install_time: row.install_time.as_deref().and_then(parse_timestamp),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostApplicationInventoryRow {
    name: String,
    publisher: Option<String>,
    version: String,
    architecture: Option<String>,
    install_scope: Option<String>,
    install_path: Option<String>,
    application_type: Option<String>,
    bundle_identifier: Option<String>,
    uninstall_identity: Option<String>,
    install_date: Option<String>,
    metadata_json: Option<String>,
}

impl From<HostApplicationInventoryRow> for HostApplicationInventoryItem {
    fn from(row: HostApplicationInventoryRow) -> Self {
        Self {
            name: row.name,
            publisher: row.publisher,
            version: row.version,
            architecture: row.architecture,
            install_scope: row.install_scope,
            install_path: row.install_path,
            application_type: row.application_type,
            bundle_identifier: row.bundle_identifier,
            uninstall_identity: row.uninstall_identity,
            install_date: row.install_date.as_deref().and_then(parse_timestamp),
            metadata: row
                .metadata_json
                .and_then(|value| serde_json::from_str(&value).ok()),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostWebInventoryRow {
    server_type: String,
    site_name: String,
    bindings_json: String,
    document_root: Option<String>,
    application_pool: Option<String>,
    tls_certificate_reference: Option<String>,
    metadata_json: Option<String>,
}

impl From<HostWebInventoryRow> for HostWebInventoryItem {
    fn from(row: HostWebInventoryRow) -> Self {
        Self {
            server_type: row.server_type,
            site_name: row.site_name,
            bindings: serde_json::from_str(&row.bindings_json).unwrap_or_default(),
            document_root: row.document_root,
            application_pool: row.application_pool,
            tls_certificate_reference: row.tls_certificate_reference,
            metadata: row
                .metadata_json
                .and_then(|value| serde_json::from_str(&value).ok()),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostRuntimeInventoryRow {
    runtime_type: String,
    runtime_name: String,
    runtime_version: Option<String>,
    install_path: Option<String>,
    management_endpoint: Option<String>,
    deployed_units_json: String,
    metadata_json: Option<String>,
}

impl From<HostRuntimeInventoryRow> for HostRuntimeInventoryItem {
    fn from(row: HostRuntimeInventoryRow) -> Self {
        Self {
            runtime_type: row.runtime_type,
            runtime_name: row.runtime_name,
            runtime_version: row.runtime_version,
            install_path: row.install_path,
            management_endpoint: row.management_endpoint,
            deployed_units: serde_json::from_str(&row.deployed_units_json).unwrap_or_default(),
            metadata: row
                .metadata_json
                .and_then(|value| serde_json::from_str(&value).ok()),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostContainerInventoryRow {
    container_id: String,
    name: String,
    image: Option<String>,
    status: String,
    status_detail: Option<String>,
    created_at: Option<String>,
    ports_json: String,
    mounts_json: String,
    runtime_type: String,
    metadata_json: Option<String>,
}

impl From<HostContainerInventoryRow> for HostContainerInventoryItem {
    fn from(row: HostContainerInventoryRow) -> Self {
        Self {
            container_id: row.container_id,
            name: row.name,
            image: row.image,
            status: row.status,
            status_detail: row.status_detail,
            created_at: row.created_at,
            ports: serde_json::from_str(&row.ports_json).unwrap_or_default(),
            mounts: serde_json::from_str(&row.mounts_json).unwrap_or_default(),
            runtime_type: row.runtime_type,
            metadata: row
                .metadata_json
                .and_then(|v| serde_json::from_str(&v).ok()),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostUserInventoryRow {
    username: String,
    uid: Option<i64>,
    sid: Option<String>,
    gid: Option<i64>,
    home_directory: Option<String>,
    shell: Option<String>,
    user_type: Option<String>,
    groups_json: String,
    last_login: Option<String>,
    locked: Option<bool>,
    gecos: Option<String>,
    metadata_json: Option<String>,
}

impl From<HostUserInventoryRow> for HostUserInventoryItem {
    fn from(row: HostUserInventoryRow) -> Self {
        Self {
            username: row.username,
            uid: row.uid,
            sid: row.sid,
            gid: row.gid,
            home_directory: row.home_directory,
            shell: row.shell,
            user_type: row.user_type,
            groups: serde_json::from_str(&row.groups_json).unwrap_or_default(),
            last_login: row.last_login,
            locked: row.locked,
            gecos: row.gecos,
            metadata: row
                .metadata_json
                .and_then(|v| serde_json::from_str(&v).ok()),
        }
    }
}

#[derive(Debug, FromRow)]
struct HostUpdateStatusRow {
    certname: String,
    snapshot_id: Option<String>,
    is_stale: bool,
    stale_reason: Option<String>,
    outdated_packages: i64,
    outdated_applications: i64,
    total_packages: i64,
    total_applications: i64,
    outdated_items_json: Option<String>,
    checked_at: String,
}

impl From<HostUpdateStatusRow> for HostUpdateStatus {
    fn from(row: HostUpdateStatusRow) -> Self {
        Self {
            certname: row.certname,
            snapshot_id: row.snapshot_id,
            is_stale: row.is_stale,
            stale_reason: row.stale_reason,
            outdated_packages: row.outdated_packages.max(0) as usize,
            outdated_applications: row.outdated_applications.max(0) as usize,
            total_packages: row.total_packages.max(0) as usize,
            total_applications: row.total_applications.max(0) as usize,
            checked_at: parse_timestamp_required(&row.checked_at),
            outdated_items: row
                .outdated_items_json
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, FromRow)]
struct RepositoryVersionCatalogRow {
    id: String,
    platform_family: String,
    distribution: String,
    os_version_pattern: Option<String>,
    package_manager: Option<String>,
    software_type: String,
    software_name: String,
    repository_source: Option<String>,
    latest_version: String,
    latest_release: Option<String>,
    source_kind: String,
    observed_nodes: i64,
    last_seen_at: String,
    created_at: String,
    updated_at: String,
}

impl From<RepositoryVersionCatalogRow> for RepositoryVersionCatalogEntry {
    fn from(row: RepositoryVersionCatalogRow) -> Self {
        Self {
            id: row.id,
            platform_family: row.platform_family,
            distribution: row.distribution,
            os_version_pattern: row.os_version_pattern,
            package_manager: row.package_manager,
            software_type: row.software_type,
            software_name: row.software_name,
            repository_source: row.repository_source,
            latest_version: row.latest_version,
            latest_release: row.latest_release,
            source_kind: row.source_kind,
            observed_nodes: row.observed_nodes.max(0) as usize,
            last_seen_at: parse_timestamp_required(&row.last_seen_at),
            created_at: parse_timestamp_required(&row.created_at),
            updated_at: parse_timestamp_required(&row.updated_at),
        }
    }
}

#[derive(Debug, FromRow)]
struct FleetRepoAggregateRow {
    os_family: String,
    distribution: String,
    os_version_pattern: String,
    package_manager: String,
    repo_id: String,
    repo_type: String,
    repo_name: Option<String>,
    base_url: Option<String>,
    mirror_list_url: Option<String>,
    distribution_path: Option<String>,
    components: Option<String>,
    architectures: Option<String>,
    reporting_nodes: i64,
}

#[derive(Debug, FromRow)]
struct FleetRepositoryConfigRow {
    id: String,
    os_family: String,
    distribution: String,
    os_version_pattern: String,
    package_manager: String,
    repo_id: String,
    repo_name: Option<String>,
    repo_type: String,
    base_url: Option<String>,
    mirror_list_url: Option<String>,
    distribution_path: Option<String>,
    components: Option<String>,
    architectures: Option<String>,
    enabled: bool,
    last_checked_at: Option<String>,
    last_check_status: Option<String>,
    last_check_error: Option<String>,
    reporting_nodes: i64,
    created_at: String,
    updated_at: String,
}

impl From<FleetRepositoryConfigRow> for FleetRepositoryConfig {
    fn from(row: FleetRepositoryConfigRow) -> Self {
        Self {
            id: row.id,
            os_family: row.os_family,
            distribution: row.distribution,
            os_version_pattern: row.os_version_pattern,
            package_manager: row.package_manager,
            repo_id: row.repo_id,
            repo_name: row.repo_name,
            repo_type: row.repo_type,
            base_url: row.base_url,
            mirror_list_url: row.mirror_list_url,
            distribution_path: row.distribution_path,
            components: row.components,
            architectures: row.architectures,
            enabled: row.enabled,
            last_checked_at: row.last_checked_at.map(|s| parse_timestamp_required(&s)),
            last_check_status: row.last_check_status,
            last_check_error: row.last_check_error,
            reporting_nodes: row.reporting_nodes.max(0) as usize,
            created_at: parse_timestamp_required(&row.created_at),
            updated_at: parse_timestamp_required(&row.updated_at),
        }
    }
}

#[derive(Debug, FromRow)]
struct CatalogPackageObservationRow {
    os_family: String,
    distribution: String,
    os_version_pattern: String,
    package_manager: Option<String>,
    name: String,
    repository_source: Option<String>,
    version: String,
    release: Option<String>,
    last_seen_at: String,
    observed_nodes: i64,
}

#[derive(Debug, FromRow)]
struct CatalogApplicationObservationRow {
    os_family: String,
    distribution: String,
    os_version_pattern: String,
    package_manager: Option<String>,
    name: String,
    publisher: Option<String>,
    application_type: Option<String>,
    version: String,
    last_seen_at: String,
    observed_nodes: i64,
}

#[derive(Debug, FromRow, Clone)]
struct HostSnapshotStatusRow {
    certname: String,
    snapshot_id: String,
    collected_at: String,
}

#[derive(Debug, FromRow)]
struct HostPackageJoinedRow {
    certname: String,
    os_family: String,
    distribution: String,
    os_version: String,
    package_manager: Option<String>,
    name: String,
    version: String,
    release: Option<String>,
    repository_source: Option<String>,
}

#[derive(Debug, Clone)]
struct HostPackageJoined {
    os_family: String,
    distribution: String,
    os_version_pattern: Option<String>,
    package_manager: Option<String>,
    name: String,
    version: String,
    release: Option<String>,
    repository_source: Option<String>,
}

#[derive(Debug, FromRow)]
struct HostApplicationJoinedRow {
    certname: String,
    os_family: String,
    distribution: String,
    os_version: String,
    package_manager: Option<String>,
    name: String,
    version: String,
    publisher: Option<String>,
    application_type: Option<String>,
}

#[derive(Debug, Clone)]
struct HostApplicationJoined {
    os_family: String,
    distribution: String,
    os_version_pattern: Option<String>,
    package_manager: Option<String>,
    name: String,
    version: String,
    publisher: Option<String>,
    application_type: Option<String>,
}

#[derive(Debug, FromRow)]
struct UpdateStatusRollupRow {
    outdated_packages: i64,
    outdated_applications: i64,
}

#[derive(Debug, FromRow)]
struct DashboardDistributionRow {
    label: String,
    value: i64,
}

#[derive(Debug, FromRow)]
struct DashboardComplianceRow {
    stale_nodes: i64,
    outdated_nodes: i64,
    compliant_nodes: i64,
}

#[derive(Debug, FromRow)]
struct PatchAgeSourceRow {
    last_successful_update_at: Option<String>,
    last_inventory_at: Option<String>,
    latest_package_install_at: Option<String>,
    latest_application_install_at: Option<String>,
}

#[derive(Debug, FromRow)]
struct DashboardOutdatedItemsRow {
    certname: String,
    outdated_items_json: Option<String>,
}

#[derive(Debug, FromRow)]
struct ComplianceCategoryRow {
    certname: String,
    is_stale: i64,
    outdated_packages: i64,
    outdated_applications: i64,
    checked_at: String,
}

#[derive(Debug, FromRow)]
struct UpdateJobRow {
    id: String,
    status: String,
    operation_type: String,
    package_names_json: String,
    target_group_id: Option<String>,
    requires_approval: bool,
    scheduled_for: Option<String>,
    maintenance_window_start: Option<String>,
    maintenance_window_end: Option<String>,
    requested_by: String,
    approved_by: Option<String>,
    approval_notes: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct UpdateJobTargetRow {
    id: String,
    certname: String,
    status: String,
    dispatched_at: Option<String>,
    completed_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, FromRow)]
struct UpdateJobResultRow {
    id: String,
    target_id: String,
    certname: String,
    status: String,
    summary: Option<String>,
    output: Option<String>,
    started_at: Option<String>,
    finished_at: String,
}

#[derive(Debug, FromRow)]
struct PendingNodeUpdateRow {
    target_id: String,
    job_id: String,
    operation_type: String,
    package_names_json: String,
    scheduled_for: Option<String>,
    maintenance_window_start: Option<String>,
    maintenance_window_end: Option<String>,
}

#[derive(Debug, FromRow)]
struct UpdateJobTargetStateRow {
    status: String,
}

fn fold_package_catalog(
    rows: Vec<CatalogPackageObservationRow>,
) -> Vec<RepositoryVersionCatalogEntry> {
    use std::collections::HashMap;
    let mut grouped: HashMap<
        (
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
        ),
        CatalogPackageObservationRow,
    > = HashMap::new();

    for row in rows {
        let key = (
            row.os_family.clone(),
            row.distribution.clone(),
            row.os_version_pattern.clone(),
            row.package_manager.clone(),
            row.name.clone(),
            row.repository_source.clone(),
        );

        match grouped.get(&key) {
            Some(current)
                if compare_version_triplets(
                    &row.version,
                    row.release.as_deref(),
                    &current.version,
                    current.release.as_deref(),
                )
                .is_gt() =>
            {
                grouped.insert(key, row);
            }
            None => {
                grouped.insert(key, row);
            }
            _ => {}
        }
    }

    grouped
        .into_values()
        .map(|row| RepositoryVersionCatalogEntry {
            id: Uuid::new_v4().to_string(),
            platform_family: row.os_family,
            distribution: row.distribution,
            os_version_pattern: Some(row.os_version_pattern),
            package_manager: row.package_manager,
            software_type: "package".to_string(),
            software_name: row.name,
            repository_source: row.repository_source,
            latest_version: row.version,
            latest_release: row.release,
            source_kind: "fleet-observed".to_string(),
            observed_nodes: row.observed_nodes.max(0) as usize,
            last_seen_at: parse_timestamp_required(&row.last_seen_at),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .collect()
}

fn fold_application_catalog(
    rows: Vec<CatalogApplicationObservationRow>,
) -> Vec<RepositoryVersionCatalogEntry> {
    use std::collections::HashMap;
    let mut grouped: HashMap<
        (
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        ),
        CatalogApplicationObservationRow,
    > = HashMap::new();

    for row in rows {
        let key = (
            row.os_family.clone(),
            row.distribution.clone(),
            row.os_version_pattern.clone(),
            row.package_manager.clone(),
            row.name.clone(),
            row.publisher.clone(),
            row.application_type.clone(),
        );

        match grouped.get(&key) {
            Some(current) if compare_versions(&row.version, &current.version).is_gt() => {
                grouped.insert(key, row);
            }
            None => {
                grouped.insert(key, row);
            }
            _ => {}
        }
    }

    grouped
        .into_values()
        .map(|row| RepositoryVersionCatalogEntry {
            id: Uuid::new_v4().to_string(),
            platform_family: row.os_family,
            distribution: row.distribution,
            os_version_pattern: Some(row.os_version_pattern),
            package_manager: row.package_manager,
            software_type: "application".to_string(),
            software_name: row.name,
            repository_source: row.publisher.or(row.application_type),
            latest_version: row.version,
            latest_release: None,
            source_kind: "fleet-observed".to_string(),
            observed_nodes: row.observed_nodes.max(0) as usize,
            last_seen_at: parse_timestamp_required(&row.last_seen_at),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .collect()
}

fn extract_major_version(os_version: &str) -> String {
    match os_version.find('.') {
        Some(pos) => os_version[..pos].to_string(),
        None => os_version.to_string(),
    }
}

fn group_packages_by_host(
    rows: Vec<HostPackageJoinedRow>,
) -> std::collections::HashMap<String, Vec<HostPackageJoined>> {
    use std::collections::HashMap;
    let mut grouped: HashMap<String, Vec<HostPackageJoined>> = HashMap::new();
    for row in rows {
        grouped
            .entry(row.certname)
            .or_default()
            .push(HostPackageJoined {
                os_family: row.os_family,
                distribution: row.distribution,
                os_version_pattern: Some(extract_major_version(&row.os_version)),
                package_manager: row.package_manager,
                name: row.name,
                version: row.version,
                release: row.release,
                repository_source: row.repository_source,
            });
    }
    grouped
}

fn group_applications_by_host(
    rows: Vec<HostApplicationJoinedRow>,
) -> std::collections::HashMap<String, Vec<HostApplicationJoined>> {
    use std::collections::HashMap;
    let mut grouped: HashMap<String, Vec<HostApplicationJoined>> = HashMap::new();
    for row in rows {
        grouped
            .entry(row.certname)
            .or_default()
            .push(HostApplicationJoined {
                os_family: row.os_family,
                distribution: row.distribution,
                os_version_pattern: Some(extract_major_version(&row.os_version)),
                package_manager: row.package_manager,
                name: row.name,
                version: row.version,
                publisher: row.publisher,
                application_type: row.application_type,
            });
    }
    grouped
}

fn compare_packages(
    packages: &[HostPackageJoined],
    catalogs: &[RepositoryVersionCatalogEntry],
) -> Vec<OutdatedInventoryItem> {
    // Partition catalogs: prefer "repo-checked" entries over "fleet-observed"
    let repo_checked: Vec<_> = catalogs
        .iter()
        .filter(|e| e.source_kind == "repo-checked" && e.software_type == "package")
        .collect();
    let fleet_observed: Vec<_> = catalogs
        .iter()
        .filter(|e| e.source_kind == "fleet-observed" && e.software_type == "package")
        .collect();

    packages
        .iter()
        .filter_map(|pkg| {
            // First try repo-checked: match by name + os_family + distribution + os_version_pattern + package_manager
            // (ignore repository_source since repo-checked uses repo_id, not vendor)
            let (catalog, source_kind) = repo_checked
                .iter()
                .find(|entry| {
                    entry.platform_family == pkg.os_family
                        && entry.distribution == pkg.distribution
                        && entry.os_version_pattern == pkg.os_version_pattern
                        && entry.package_manager == pkg.package_manager
                        && entry.software_name == pkg.name
                })
                .map(|e| (*e, "repo-checked"))
                // Fall back to fleet-observed (original behavior)
                .or_else(|| {
                    fleet_observed
                        .iter()
                        .find(|entry| {
                            entry.platform_family == pkg.os_family
                                && entry.distribution == pkg.distribution
                                && entry.os_version_pattern == pkg.os_version_pattern
                                && entry.package_manager == pkg.package_manager
                                && entry.software_name == pkg.name
                                && entry.repository_source == pkg.repository_source
                        })
                        .map(|e| (*e, "fleet-observed"))
                })?;

            (compare_version_triplets(
                &pkg.version,
                pkg.release.as_deref(),
                &catalog.latest_version,
                catalog.latest_release.as_deref(),
            )
            .is_lt())
            .then(|| OutdatedInventoryItem {
                software_type: "package".to_string(),
                name: pkg.name.clone(),
                installed_version: pkg.version.clone(),
                installed_release: pkg.release.clone(),
                latest_version: catalog.latest_version.clone(),
                latest_release: catalog.latest_release.clone(),
                repository_source: pkg.repository_source.clone(),
                source_kind: Some(source_kind.to_string()),
            })
        })
        .collect()
}

fn compare_applications(
    apps: &[HostApplicationJoined],
    catalogs: &[RepositoryVersionCatalogEntry],
) -> Vec<OutdatedInventoryItem> {
    apps.iter()
        .filter_map(|app| {
            let repo_source = app.publisher.clone().or(app.application_type.clone());
            let catalog = catalogs.iter().find(|entry| {
                entry.software_type == "application"
                    && entry.platform_family == app.os_family
                    && entry.distribution == app.distribution
                    && entry.os_version_pattern == app.os_version_pattern
                    && entry.package_manager == app.package_manager
                    && entry.software_name == app.name
                    && entry.repository_source == repo_source
            })?;

            (compare_versions(&app.version, &catalog.latest_version).is_lt()).then(|| {
                OutdatedInventoryItem {
                    software_type: "application".to_string(),
                    name: app.name.clone(),
                    installed_version: app.version.clone(),
                    installed_release: None,
                    latest_version: catalog.latest_version.clone(),
                    latest_release: None,
                    repository_source: repo_source,
                    source_kind: Some(catalog.source_kind.clone()),
                }
            })
        })
        .collect()
}

fn map_distribution(rows: Vec<DashboardDistributionRow>) -> Vec<InventoryDistributionPoint> {
    rows.into_iter()
        .filter(|row| row.value > 0)
        .map(|row| InventoryDistributionPoint {
            label: row.label,
            value: row.value as usize,
        })
        .collect()
}

fn map_buckets(rows: Vec<PatchAgeSourceRow>) -> Vec<PatchAgeBucket> {
    let now = Utc::now();
    let mut zero_to_seven = 0usize;
    let mut eight_to_thirty = 0usize;
    let mut thirty_one_to_ninety = 0usize;
    let mut ninety_plus = 0usize;
    let mut unknown = 0usize;

    for row in rows {
        let Some(timestamp) = resolve_patch_age_timestamp(&row) else {
            unknown += 1;
            continue;
        };

        let age_days = (now - timestamp).num_days().max(0);
        if age_days <= 7 {
            zero_to_seven += 1;
        } else if age_days <= 30 {
            eight_to_thirty += 1;
        } else if age_days <= 90 {
            thirty_one_to_ninety += 1;
        } else {
            ninety_plus += 1;
        }
    }

    [
        ("0-7d", zero_to_seven),
        ("8-30d", eight_to_thirty),
        ("31-90d", thirty_one_to_ninety),
        ("91d+", ninety_plus),
        ("Unknown", unknown),
    ]
    .into_iter()
    .map(|(label, value)| PatchAgeBucket {
        label: label.to_string(),
        value,
    })
    .collect()
}

fn resolve_patch_age_timestamp(row: &PatchAgeSourceRow) -> Option<DateTime<Utc>> {
    [
        row.last_successful_update_at.as_deref(),
        row.latest_package_install_at.as_deref(),
        row.latest_application_install_at.as_deref(),
        row.last_inventory_at.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(parse_timestamp)
    .max()
}

fn roll_up_update_job_status(targets: &[UpdateJobTargetStateRow]) -> UpdateJobStatus {
    let mut has_dispatched = false;
    let mut has_queued = false;
    let mut has_pending_approval = false;
    let mut has_failed = false;
    let mut has_succeeded = false;
    let mut has_rejected = false;
    let mut has_cancelled = false;

    for target in targets {
        match UpdateTargetStatus::from_str(&target.status).unwrap_or_default() {
            UpdateTargetStatus::PendingApproval => has_pending_approval = true,
            UpdateTargetStatus::Queued => has_queued = true,
            UpdateTargetStatus::Dispatched => has_dispatched = true,
            UpdateTargetStatus::Succeeded => has_succeeded = true,
            UpdateTargetStatus::Failed => has_failed = true,
            UpdateTargetStatus::Cancelled => has_cancelled = true,
            UpdateTargetStatus::Rejected => has_rejected = true,
        }
    }

    if has_pending_approval {
        UpdateJobStatus::PendingApproval
    } else if has_dispatched {
        UpdateJobStatus::InProgress
    } else if has_queued {
        UpdateJobStatus::Approved
    } else if has_failed {
        UpdateJobStatus::CompletedWithFailures
    } else if has_succeeded {
        UpdateJobStatus::Completed
    } else if has_cancelled {
        UpdateJobStatus::Cancelled
    } else if has_rejected {
        UpdateJobStatus::Rejected
    } else {
        UpdateJobStatus::Completed
    }
}

pub fn compare_version_triplets(
    lhs_version: &str,
    lhs_release: Option<&str>,
    rhs_version: &str,
    rhs_release: Option<&str>,
) -> std::cmp::Ordering {
    let version_cmp = compare_versions(lhs_version, rhs_version);
    if version_cmp != std::cmp::Ordering::Equal {
        return version_cmp;
    }
    compare_versions(lhs_release.unwrap_or(""), rhs_release.unwrap_or(""))
}

fn compare_versions(lhs: &str, rhs: &str) -> std::cmp::Ordering {
    let left = tokenize_version(lhs);
    let right = tokenize_version(rhs);
    let max_len = left.len().max(right.len());

    for index in 0..max_len {
        let l = left.get(index);
        let r = right.get(index);
        let ord = match (l, r) {
            (Some(VersionToken::Number(a)), Some(VersionToken::Number(b))) => a.cmp(b),
            (Some(VersionToken::Text(a)), Some(VersionToken::Text(b))) => a.cmp(b),
            (Some(VersionToken::Number(_)), Some(VersionToken::Text(_))) => {
                std::cmp::Ordering::Greater
            }
            (Some(VersionToken::Text(_)), Some(VersionToken::Number(_))) => {
                std::cmp::Ordering::Less
            }
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }

    std::cmp::Ordering::Equal
}

#[derive(Debug)]
enum VersionToken {
    Number(i64),
    Text(String),
}

fn tokenize_version(value: &str) -> Vec<VersionToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut is_numeric = None;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            let ch_numeric = ch.is_ascii_digit();
            match is_numeric {
                Some(flag) if flag == ch_numeric => current.push(ch),
                Some(flag) => {
                    tokens.push(to_token(&current, flag));
                    current.clear();
                    current.push(ch);
                    is_numeric = Some(ch_numeric);
                }
                None => {
                    current.push(ch);
                    is_numeric = Some(ch_numeric);
                }
            }
        } else if !current.is_empty() {
            tokens.push(to_token(&current, is_numeric.unwrap_or(false)));
            current.clear();
            is_numeric = None;
        }
    }

    if !current.is_empty() {
        tokens.push(to_token(&current, is_numeric.unwrap_or(false)));
    }

    tokens
}

fn to_token(value: &str, numeric: bool) -> VersionToken {
    if numeric {
        VersionToken::Number(value.parse::<i64>().unwrap_or(0))
    } else {
        VersionToken::Text(value.to_lowercase())
    }
}

fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    if s.is_empty() {
        return None;
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }

    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(dt.and_utc());
    }

    None
}

fn parse_timestamp_required(s: &str) -> DateTime<Utc> {
    parse_timestamp(s).unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn ingest_and_reload_inventory() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let repo = InventoryRepository::new(pool);
        let payload = InventoryPayload {
            collector_version: "phase10-test".to_string(),
            collected_at: Some(Utc::now()),
            is_full_snapshot: true,
            os: HostOsInventory {
                os_family: "RedHat".to_string(),
                distribution: "Rocky".to_string(),
                edition: None,
                architecture: Some("x86_64".to_string()),
                kernel_version: Some("5.14".to_string()),
                os_version: "9.5".to_string(),
                patch_level: Some("2026.03".to_string()),
                package_manager: Some("dnf".to_string()),
                update_channel: Some("stable".to_string()),
                last_inventory_at: Some(Utc::now()),
                last_successful_update_at: None,
            },
            packages: vec![HostPackageInventoryItem {
                name: "httpd".to_string(),
                epoch: None,
                version: "2.4.62".to_string(),
                release: Some("1.el9".to_string()),
                architecture: Some("x86_64".to_string()),
                repository_source: Some("baseos".to_string()),
                install_path: None,
                install_time: None,
            }],
            applications: vec![HostApplicationInventoryItem {
                name: "Apache HTTP Server".to_string(),
                publisher: Some("Rocky Linux".to_string()),
                version: "2.4.62".to_string(),
                architecture: Some("x86_64".to_string()),
                install_scope: Some("system".to_string()),
                install_path: Some("/usr/sbin/httpd".to_string()),
                application_type: Some("service".to_string()),
                bundle_identifier: None,
                uninstall_identity: None,
                install_date: None,
                metadata: None,
            }],
            websites: vec![HostWebInventoryItem {
                server_type: "apache".to_string(),
                site_name: "default".to_string(),
                bindings: vec!["*:80".to_string()],
                document_root: Some("/var/www/html".to_string()),
                application_pool: None,
                tls_certificate_reference: None,
                metadata: None,
            }],
            runtimes: vec![HostRuntimeInventoryItem {
                runtime_type: "tomcat".to_string(),
                runtime_name: "tomcat9".to_string(),
                runtime_version: Some("9.0.89".to_string()),
                install_path: Some("/opt/tomcat".to_string()),
                management_endpoint: None,
                deployed_units: vec!["ROOT.war".to_string()],
                metadata: None,
            }],
            containers: vec![],
            users: vec![],
            repositories: vec![],
        };

        let inventory = repo
            .ingest_inventory("node1.example.com", &payload)
            .await
            .expect("inventory ingested");

        assert_eq!(inventory.summary.package_count, 1);
        assert_eq!(inventory.summary.application_count, 1);
        assert_eq!(inventory.summary.website_count, 1);
        assert_eq!(inventory.summary.runtime_count, 1);

        let history = repo
            .get_inventory_history("node1.example.com", 10)
            .await
            .expect("inventory history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].collector_version, "phase10-test");
    }

    #[test]
    fn patch_age_buckets_fall_back_to_install_and_inventory_timestamps() {
        let now = Utc::now();
        let rows = vec![
            PatchAgeSourceRow {
                last_successful_update_at: Some((now - chrono::Duration::days(3)).to_rfc3339()),
                last_inventory_at: None,
                latest_package_install_at: None,
                latest_application_install_at: None,
            },
            PatchAgeSourceRow {
                last_successful_update_at: None,
                last_inventory_at: None,
                latest_package_install_at: Some((now - chrono::Duration::days(20)).to_rfc3339()),
                latest_application_install_at: None,
            },
            PatchAgeSourceRow {
                last_successful_update_at: None,
                last_inventory_at: None,
                latest_package_install_at: None,
                latest_application_install_at: Some(
                    (now - chrono::Duration::days(60)).to_rfc3339(),
                ),
            },
            PatchAgeSourceRow {
                last_successful_update_at: None,
                last_inventory_at: Some((now - chrono::Duration::days(120)).to_rfc3339()),
                latest_package_install_at: None,
                latest_application_install_at: None,
            },
            PatchAgeSourceRow {
                last_successful_update_at: None,
                last_inventory_at: None,
                latest_package_install_at: None,
                latest_application_install_at: None,
            },
        ];

        let buckets = map_buckets(rows);
        let counts = buckets
            .into_iter()
            .map(|bucket| (bucket.label, bucket.value))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(counts.get("0-7d"), Some(&1));
        assert_eq!(counts.get("8-30d"), Some(&1));
        assert_eq!(counts.get("31-90d"), Some(&1));
        assert_eq!(counts.get("91d+"), Some(&1));
        assert_eq!(counts.get("Unknown"), Some(&1));
    }

    #[tokio::test]
    async fn update_job_lifecycle_round_trips() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");

        let repo = InventoryRepository::new(pool);
        let job = repo
            .create_update_job(
                UpdateOperationType::PackageUpdate,
                &["httpd".to_string()],
                None,
                &["node1.example.com".to_string()],
                true,
                None,
                None,
                None,
                "admin",
                Some("maintenance window"),
            )
            .await
            .expect("create update job");

        assert_eq!(job.status, UpdateJobStatus::PendingApproval);
        assert_eq!(job.targets[0].status, UpdateTargetStatus::PendingApproval);

        let approved = repo
            .approve_update_job(&job.id, true, "operator", Some("approved"))
            .await
            .expect("approve update job")
            .expect("job exists");

        assert_eq!(approved.status, UpdateJobStatus::Approved);
        assert_eq!(approved.targets[0].status, UpdateTargetStatus::Queued);

        let pending = repo
            .claim_pending_updates_for_node("node1.example.com")
            .await
            .expect("claim pending updates");
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].operation_type,
            UpdateOperationType::PackageUpdate
        );
        assert_eq!(pending[0].package_names, vec!["httpd".to_string()]);

        let completed = repo
            .submit_update_job_result(
                &job.id,
                &pending[0].target_id,
                "node1.example.com",
                &SubmitUpdateJobResultRequest {
                    status: UpdateTargetStatus::Succeeded,
                    summary: Some("updated httpd".to_string()),
                    output: Some("ok".to_string()),
                    started_at: Some(Utc::now()),
                    finished_at: Some(Utc::now()),
                },
            )
            .await
            .expect("submit result")
            .expect("job exists");

        assert_eq!(completed.status, UpdateJobStatus::Completed);
        assert_eq!(completed.targets[0].status, UpdateTargetStatus::Succeeded);
        assert_eq!(completed.results.len(), 1);
        assert_eq!(completed.results[0].status, UpdateTargetStatus::Succeeded);
    }
}
