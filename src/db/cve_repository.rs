//! CVE vulnerability repository.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, Row, SqlitePool};
use uuid::Uuid;

use crate::models::{
    CveAffectedNode, CveDetailResponse, CveEntry, CveFeedSource, CveFeedType, CvePackageMatch,
    CveSeverity, HostVulnerabilityMatch, NodeVulnerabilitySummary, SeverityDistributionPoint,
    TopCveItem, TopVulnerableNode, VulnerabilityDashboardReport,
};

pub struct CveRepository {
    pool: SqlitePool,
}

// -- Row types for SQLx --

#[derive(Debug, FromRow)]
struct FeedSourceRow {
    id: String,
    name: String,
    feed_url: String,
    feed_type: String,
    enabled: bool,
    last_sync_at: Option<String>,
    last_sync_status: String,
    last_sync_error: Option<String>,
    sync_interval_secs: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct CveEntryRow {
    id: String,
    feed_source_id: String,
    description: Option<String>,
    severity: String,
    cvss_score: Option<f64>,
    cvss_vector: Option<String>,
    published_at: Option<String>,
    modified_at: Option<String>,
    references_json: Option<String>,
    affected_products_json: Option<String>,
    is_kev: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct PackageMatchRow {
    id: String,
    cve_id: String,
    package_name: String,
    version_start: Option<String>,
    version_end: Option<String>,
    platform_family: Option<String>,
    created_at: String,
}

#[derive(Debug, FromRow)]
struct VulnMatchRow {
    id: String,
    certname: String,
    cve_id: String,
    package_name: String,
    installed_version: String,
    severity: String,
    cvss_score: Option<f64>,
    is_kev: bool,
    matched_at: String,
}

impl CveRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // -- Feed source CRUD --

    pub async fn list_feed_sources(&self) -> Result<Vec<CveFeedSource>> {
        let rows =
            sqlx::query_as::<_, FeedSourceRow>("SELECT * FROM cve_feed_sources ORDER BY name ASC")
                .fetch_all(&self.pool)
                .await
                .context("Failed to list CVE feed sources")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_feed_source(&self, id: &str) -> Result<Option<CveFeedSource>> {
        let row =
            sqlx::query_as::<_, FeedSourceRow>("SELECT * FROM cve_feed_sources WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .context("Failed to get CVE feed source")?;

        Ok(row.map(Into::into))
    }

    pub async fn create_feed_source(
        &self,
        name: &str,
        feed_url: &str,
        feed_type: CveFeedType,
        enabled: bool,
        sync_interval_secs: u64,
    ) -> Result<CveFeedSource> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO cve_feed_sources (id, name, feed_url, feed_type, enabled, last_sync_status, sync_interval_secs, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, 'never', ?6, ?7, ?8)
            "#,
        )
        .bind(&id)
        .bind(name)
        .bind(feed_url)
        .bind(feed_type.as_str())
        .bind(enabled)
        .bind(sync_interval_secs as i64)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to create CVE feed source")?;

        self.get_feed_source(&id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Feed source not found after creation"))
    }

    pub async fn update_feed_source(
        &self,
        id: &str,
        name: Option<&str>,
        feed_url: Option<&str>,
        enabled: Option<bool>,
        sync_interval_secs: Option<u64>,
    ) -> Result<Option<CveFeedSource>> {
        let existing = self.get_feed_source(id).await?;
        if existing.is_none() {
            return Ok(None);
        }
        let existing = existing.unwrap();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE cve_feed_sources
            SET name = ?1, feed_url = ?2, enabled = ?3, sync_interval_secs = ?4, updated_at = ?5
            WHERE id = ?6
            "#,
        )
        .bind(name.unwrap_or(&existing.name))
        .bind(feed_url.unwrap_or(&existing.feed_url))
        .bind(enabled.unwrap_or(existing.enabled))
        .bind(sync_interval_secs.unwrap_or(existing.sync_interval_secs) as i64)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update CVE feed source")?;

        self.get_feed_source(id).await
    }

    pub async fn delete_feed_source(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM cve_feed_sources WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete CVE feed source")?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn update_feed_sync_status(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE cve_feed_sources
            SET last_sync_at = ?1, last_sync_status = ?2, last_sync_error = ?3, updated_at = ?4
            WHERE id = ?5
            "#,
        )
        .bind(&now)
        .bind(status)
        .bind(error)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update feed sync status")?;

        Ok(())
    }

    // -- CVE entry storage --

    pub async fn upsert_cve_entry(
        &self,
        cve_id: &str,
        feed_source_id: &str,
        description: Option<&str>,
        severity: CveSeverity,
        cvss_score: Option<f64>,
        cvss_vector: Option<&str>,
        published_at: Option<&str>,
        modified_at: Option<&str>,
        references: &[String],
        affected_products: &[String],
        is_kev: bool,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let refs_json = serde_json::to_string(references)?;
        let products_json = serde_json::to_string(affected_products)?;

        sqlx::query(
            r#"
            INSERT INTO cve_entries (id, feed_source_id, description, severity, cvss_score, cvss_vector, published_at, modified_at, references_json, affected_products_json, is_kev, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id) DO UPDATE SET
                description = COALESCE(?3, description),
                severity = ?4,
                cvss_score = ?5,
                cvss_vector = ?6,
                modified_at = COALESCE(?8, modified_at),
                references_json = ?9,
                affected_products_json = ?10,
                is_kev = MAX(is_kev, ?11),
                updated_at = ?13
            "#,
        )
        .bind(cve_id)
        .bind(feed_source_id)
        .bind(description)
        .bind(severity.as_str())
        .bind(cvss_score)
        .bind(cvss_vector)
        .bind(published_at)
        .bind(modified_at)
        .bind(&refs_json)
        .bind(&products_json)
        .bind(is_kev)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to upsert CVE entry")?;

        Ok(())
    }

    pub async fn get_cve_entry(&self, cve_id: &str) -> Result<Option<CveEntry>> {
        let row = sqlx::query_as::<_, CveEntryRow>("SELECT * FROM cve_entries WHERE id = ?1")
            .bind(cve_id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to get CVE entry")?;

        Ok(row.map(Into::into))
    }

    pub async fn search_cves(
        &self,
        query: Option<&str>,
        severity: Option<&str>,
        is_kev: Option<bool>,
        limit: usize,
    ) -> Result<Vec<CveEntry>> {
        let mut sql = String::from("SELECT * FROM cve_entries WHERE 1=1");
        let mut binds: Vec<String> = Vec::new();

        if let Some(q) = query {
            sql.push_str(" AND (id LIKE ?1 OR description LIKE ?1)");
            binds.push(format!("%{}%", q));
        }
        if let Some(sev) = severity {
            let idx = binds.len() + 1;
            sql.push_str(&format!(" AND severity = ?{}", idx));
            binds.push(sev.to_string());
        }
        if let Some(kev) = is_kev {
            let idx = binds.len() + 1;
            sql.push_str(&format!(" AND is_kev = ?{}", idx));
            binds.push(if kev {
                "1".to_string()
            } else {
                "0".to_string()
            });
        }

        sql.push_str(&format!(
            " ORDER BY published_at DESC LIMIT {}",
            limit.min(500)
        ));

        let mut q = sqlx::query_as::<_, CveEntryRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .context("Failed to search CVEs")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    // -- Package match storage --

    pub async fn upsert_package_matches(
        &self,
        cve_id: &str,
        matches: &[CvePackageMatch],
    ) -> Result<usize> {
        // Delete existing matches for this CVE then re-insert
        sqlx::query("DELETE FROM cve_package_matches WHERE cve_id = ?1")
            .bind(cve_id)
            .execute(&self.pool)
            .await
            .context("Failed to clear old package matches")?;

        let now = Utc::now().to_rfc3339();
        let mut count = 0;
        for m in matches {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO cve_package_matches (id, cve_id, package_name, version_start, version_end, platform_family, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(&id)
            .bind(cve_id)
            .bind(&m.package_name)
            .bind(&m.version_start)
            .bind(&m.version_end)
            .bind(&m.platform_family)
            .bind(&now)
            .execute(&self.pool)
            .await
            .context("Failed to insert package match")?;
            count += 1;
        }

        Ok(count)
    }

    // -- Vulnerability matching (materialized refresh) --

    pub async fn refresh_host_vulnerability_matches(&self) -> Result<usize> {
        // Clear existing matches
        sqlx::query("DELETE FROM host_vulnerability_matches")
            .execute(&self.pool)
            .await
            .context("Failed to clear host vulnerability matches")?;

        let now = Utc::now().to_rfc3339();

        // Cross-join installed packages with CVE package matches.
        // We match on package name (case-insensitive) and check version is within range.
        // Version comparison is simplified: if version_start is set, installed >= start;
        // if version_end is set, installed < end. If neither is set, all versions match.
        let inserted = sqlx::query(
            r#"
            INSERT INTO host_vulnerability_matches (id, certname, cve_id, package_name, installed_version, severity, cvss_score, is_kev, matched_at)
            SELECT
                lower(hex(randomblob(16))),
                pkg.certname,
                cpm.cve_id,
                pkg.name,
                pkg.version,
                ce.severity,
                ce.cvss_score,
                ce.is_kev,
                ?1
            FROM host_package_inventory pkg
            INNER JOIN cve_package_matches cpm
                ON LOWER(pkg.name) = LOWER(cpm.package_name)
                AND (cpm.platform_family IS NULL OR cpm.platform_family = (
                    SELECT LOWER(os.os_family) FROM host_os_inventory os WHERE os.certname = pkg.certname LIMIT 1
                ))
            INNER JOIN cve_entries ce ON ce.id = cpm.cve_id
            WHERE
                (cpm.version_end IS NULL OR pkg.version < cpm.version_end)
                AND (cpm.version_start IS NULL OR pkg.version >= cpm.version_start)
            ON CONFLICT(certname, cve_id, package_name) DO UPDATE SET
                installed_version = excluded.installed_version,
                severity = excluded.severity,
                cvss_score = excluded.cvss_score,
                is_kev = excluded.is_kev,
                matched_at = excluded.matched_at
            "#,
        )
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("Failed to refresh host vulnerability matches")?;

        Ok(inserted.rows_affected() as usize)
    }

    // -- Query methods --

    pub async fn get_node_vulnerabilities(
        &self,
        certname: &str,
    ) -> Result<Vec<HostVulnerabilityMatch>> {
        let rows = sqlx::query_as::<_, VulnMatchRow>(
            "SELECT * FROM host_vulnerability_matches WHERE certname = ?1 ORDER BY CASE severity WHEN 'critical' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END, cvss_score DESC",
        )
        .bind(certname)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get node vulnerabilities")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_vulnerability_summary(
        &self,
        certname: &str,
    ) -> Result<NodeVulnerabilitySummary> {
        let row = sqlx::query(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN severity = 'critical' THEN 1 ELSE 0 END), 0) AS critical_count,
                COALESCE(SUM(CASE WHEN severity = 'high' THEN 1 ELSE 0 END), 0) AS high_count,
                COALESCE(SUM(CASE WHEN severity = 'medium' THEN 1 ELSE 0 END), 0) AS medium_count,
                COALESCE(SUM(CASE WHEN severity = 'low' THEN 1 ELSE 0 END), 0) AS low_count,
                COALESCE(SUM(CASE WHEN is_kev = 1 THEN 1 ELSE 0 END), 0) AS kev_count,
                COUNT(*) AS total_count,
                MAX(matched_at) AS last_checked_at
            FROM host_vulnerability_matches
            WHERE certname = ?1
            "#,
        )
        .bind(certname)
        .fetch_one(&self.pool)
        .await
        .context("Failed to get vulnerability summary")?;

        Ok(NodeVulnerabilitySummary {
            certname: certname.to_string(),
            critical_count: row.get::<i64, _>("critical_count") as usize,
            high_count: row.get::<i64, _>("high_count") as usize,
            medium_count: row.get::<i64, _>("medium_count") as usize,
            low_count: row.get::<i64, _>("low_count") as usize,
            kev_count: row.get::<i64, _>("kev_count") as usize,
            total_count: row.get::<i64, _>("total_count") as usize,
            last_checked_at: row
                .get::<Option<String>, _>("last_checked_at")
                .and_then(|s| parse_timestamp(&s)),
        })
    }

    pub async fn get_fleet_vulnerability_dashboard(&self) -> Result<VulnerabilityDashboardReport> {
        // Total vulnerable nodes
        let total_vulnerable_nodes = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT certname) FROM host_vulnerability_matches",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to count vulnerable nodes")? as usize;

        // Total distinct CVEs matched
        let total_cves_matched = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT cve_id) FROM host_vulnerability_matches",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to count matched CVEs")? as usize;

        // KEV count
        let kev_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT cve_id) FROM host_vulnerability_matches WHERE is_kev = 1",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to count KEV matches")? as usize;

        // Severity distribution
        let sev_rows = sqlx::query(
            "SELECT severity, COUNT(DISTINCT cve_id || '|' || certname) AS cnt FROM host_vulnerability_matches GROUP BY severity ORDER BY CASE severity WHEN 'critical' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to get severity distribution")?;

        let severity_distribution: Vec<SeverityDistributionPoint> = sev_rows
            .iter()
            .map(|r| SeverityDistributionPoint {
                severity: r.get::<String, _>("severity"),
                count: r.get::<i64, _>("cnt") as usize,
            })
            .collect();

        // Top CVEs by affected node count
        let top_cve_rows = sqlx::query(
            r#"
            SELECT hvm.cve_id, hvm.severity, hvm.cvss_score, hvm.is_kev,
                   COUNT(DISTINCT hvm.certname) AS affected_nodes,
                   ce.description
            FROM host_vulnerability_matches hvm
            LEFT JOIN cve_entries ce ON ce.id = hvm.cve_id
            GROUP BY hvm.cve_id
            ORDER BY affected_nodes DESC, hvm.cvss_score DESC
            LIMIT 20
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to get top CVEs")?;

        let top_cves: Vec<TopCveItem> = top_cve_rows
            .iter()
            .map(|r| TopCveItem {
                cve_id: r.get::<String, _>("cve_id"),
                severity: CveSeverity::from_str(&r.get::<String, _>("severity")),
                cvss_score: r.get::<Option<f64>, _>("cvss_score"),
                affected_nodes: r.get::<i64, _>("affected_nodes") as usize,
                description: r.get::<Option<String>, _>("description"),
                is_kev: r.get::<bool, _>("is_kev"),
            })
            .collect();

        // Top vulnerable nodes
        let top_node_rows = sqlx::query(
            r#"
            SELECT certname,
                   COUNT(*) AS total_vulns,
                   SUM(CASE WHEN severity = 'critical' THEN 1 ELSE 0 END) AS critical_count,
                   SUM(CASE WHEN is_kev = 1 THEN 1 ELSE 0 END) AS kev_count
            FROM host_vulnerability_matches
            GROUP BY certname
            ORDER BY critical_count DESC, total_vulns DESC
            LIMIT 20
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to get top vulnerable nodes")?;

        let top_vulnerable_nodes: Vec<TopVulnerableNode> = top_node_rows
            .iter()
            .map(|r| TopVulnerableNode {
                certname: r.get::<String, _>("certname"),
                total_vulns: r.get::<i64, _>("total_vulns") as usize,
                critical_count: r.get::<i64, _>("critical_count") as usize,
                kev_count: r.get::<i64, _>("kev_count") as usize,
            })
            .collect();

        Ok(VulnerabilityDashboardReport {
            total_vulnerable_nodes,
            total_cves_matched,
            severity_distribution,
            top_cves,
            top_vulnerable_nodes,
            kev_count,
            generated_at: Utc::now(),
        })
    }

    pub async fn get_nodes_affected_by_cve(&self, cve_id: &str) -> Result<Vec<CveAffectedNode>> {
        let rows = sqlx::query(
            r#"
            SELECT certname, package_name, installed_version, matched_at
            FROM host_vulnerability_matches
            WHERE cve_id = ?1
            ORDER BY certname
            "#,
        )
        .bind(cve_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get nodes affected by CVE")?;

        Ok(rows
            .iter()
            .map(|r| CveAffectedNode {
                certname: r.get::<String, _>("certname"),
                package_name: r.get::<String, _>("package_name"),
                installed_version: r.get::<String, _>("installed_version"),
                matched_at: parse_timestamp_required(&r.get::<String, _>("matched_at")),
            })
            .collect())
    }

    pub async fn get_cve_detail(&self, cve_id: &str) -> Result<Option<CveDetailResponse>> {
        let entry = self.get_cve_entry(cve_id).await?;
        let entry = match entry {
            Some(e) => e,
            None => return Ok(None),
        };

        let affected_nodes = self.get_nodes_affected_by_cve(cve_id).await?;

        let match_rows = sqlx::query_as::<_, PackageMatchRow>(
            "SELECT * FROM cve_package_matches WHERE cve_id = ?1",
        )
        .bind(cve_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get package matches for CVE")?;

        let package_matches: Vec<CvePackageMatch> =
            match_rows.into_iter().map(Into::into).collect();

        Ok(Some(CveDetailResponse {
            entry,
            affected_nodes,
            package_matches,
        }))
    }

    /// Get vulnerable package names for a set of certnames (used for SecurityPatch resolution)
    pub async fn get_vulnerable_packages_for_nodes(
        &self,
        certnames: &[String],
    ) -> Result<Vec<(String, Vec<String>)>> {
        if certnames.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = (1..=certnames.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT certname, package_name FROM host_vulnerability_matches WHERE certname IN ({}) GROUP BY certname, package_name ORDER BY certname",
            placeholders.join(", ")
        );

        let mut q = sqlx::query(&sql);
        for cn in certnames {
            q = q.bind(cn);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .context("Failed to get vulnerable packages for nodes")?;

        // Group by certname
        let mut result: Vec<(String, Vec<String>)> = Vec::new();
        let mut current_certname: Option<String> = None;
        let mut current_packages: Vec<String> = Vec::new();

        for row in &rows {
            let certname: String = row.get("certname");
            let pkg: String = row.get("package_name");

            if current_certname.as_deref() != Some(&certname) {
                if let Some(cn) = current_certname.take() {
                    result.push((cn, std::mem::take(&mut current_packages)));
                }
                current_certname = Some(certname);
            }
            current_packages.push(pkg);
        }
        if let Some(cn) = current_certname {
            result.push((cn, current_packages));
        }

        Ok(result)
    }

    /// Get CVE IDs affecting a specific package on a specific node
    pub async fn get_cve_ids_for_package(
        &self,
        certname: &str,
        package_name: &str,
    ) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT cve_id FROM host_vulnerability_matches WHERE certname = ?1 AND package_name = ?2",
        )
        .bind(certname)
        .bind(package_name)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get CVE IDs for package")?;

        Ok(rows)
    }

    /// Seed default feed sources if table is empty
    pub async fn seed_default_feeds(&self) -> Result<()> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cve_feed_sources")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count feed sources")?;

        if count > 0 {
            return Ok(());
        }

        self.create_feed_source(
            "NVD - National Vulnerability Database",
            "https://services.nvd.nist.gov/rest/json/cves/2.0",
            CveFeedType::NvdJson,
            true,
            3600,
        )
        .await?;

        self.create_feed_source(
            "CISA Known Exploited Vulnerabilities",
            "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json",
            CveFeedType::CisaKev,
            true,
            3600,
        )
        .await?;

        Ok(())
    }
}

// -- Conversions --

impl From<FeedSourceRow> for CveFeedSource {
    fn from(row: FeedSourceRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            feed_url: row.feed_url,
            feed_type: CveFeedType::from_str(&row.feed_type).unwrap_or(CveFeedType::Custom),
            enabled: row.enabled,
            last_sync_at: row.last_sync_at.and_then(|s| parse_timestamp(&s)),
            last_sync_status: row.last_sync_status,
            last_sync_error: row.last_sync_error,
            sync_interval_secs: row.sync_interval_secs.max(0) as u64,
            created_at: parse_timestamp_required(&row.created_at),
            updated_at: parse_timestamp_required(&row.updated_at),
        }
    }
}

impl From<CveEntryRow> for CveEntry {
    fn from(row: CveEntryRow) -> Self {
        Self {
            id: row.id,
            feed_source_id: row.feed_source_id,
            description: row.description,
            severity: CveSeverity::from_str(&row.severity),
            cvss_score: row.cvss_score,
            cvss_vector: row.cvss_vector,
            published_at: row.published_at.and_then(|s| parse_timestamp(&s)),
            modified_at: row.modified_at.and_then(|s| parse_timestamp(&s)),
            references: row
                .references_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            affected_products: row
                .affected_products_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            is_kev: row.is_kev,
            created_at: parse_timestamp_required(&row.created_at),
            updated_at: parse_timestamp_required(&row.updated_at),
        }
    }
}

impl From<PackageMatchRow> for CvePackageMatch {
    fn from(row: PackageMatchRow) -> Self {
        Self {
            id: row.id,
            cve_id: row.cve_id,
            package_name: row.package_name,
            version_start: row.version_start,
            version_end: row.version_end,
            platform_family: row.platform_family,
            created_at: parse_timestamp_required(&row.created_at),
        }
    }
}

impl From<VulnMatchRow> for HostVulnerabilityMatch {
    fn from(row: VulnMatchRow) -> Self {
        Self {
            id: row.id,
            certname: row.certname,
            cve_id: row.cve_id,
            package_name: row.package_name,
            installed_version: row.installed_version,
            severity: CveSeverity::from_str(&row.severity),
            cvss_score: row.cvss_score,
            is_kev: row.is_kev,
            matched_at: parse_timestamp_required(&row.matched_at),
        }
    }
}

// -- Timestamp helpers (same pattern as inventory_repository) --

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
