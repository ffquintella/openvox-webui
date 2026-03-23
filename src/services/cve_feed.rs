//! CVE feed synchronization service.
//!
//! Fetches CVE data from NVD 2.0 API and CISA KEV catalog, parses entries,
//! maps affected products to package names, and stores results in the database.

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, error, info};

use crate::db::CveRepository;
use crate::models::{CveFeedSource, CveFeedType, CvePackageMatch, CveSeverity, FeedSyncResult};

pub struct CveFeedService {
    client: Client,
    repo: CveRepository,
}

// -- NVD 2.0 API response types --

#[derive(Debug, Deserialize)]
struct NvdResponse {
    #[serde(default)]
    vulnerabilities: Vec<NvdVulnerability>,
    #[serde(default, rename = "totalResults")]
    total_results: usize,
}

#[derive(Debug, Deserialize)]
struct NvdVulnerability {
    cve: NvdCve,
}

#[derive(Debug, Deserialize)]
struct NvdCve {
    id: String,
    #[serde(default)]
    descriptions: Vec<NvdDescription>,
    #[serde(default)]
    published: Option<String>,
    #[serde(default, rename = "lastModified")]
    last_modified: Option<String>,
    #[serde(default)]
    metrics: Option<NvdMetrics>,
    #[serde(default)]
    references: Vec<NvdReference>,
    #[serde(default)]
    configurations: Vec<NvdConfiguration>,
}

#[derive(Debug, Deserialize)]
struct NvdDescription {
    lang: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct NvdMetrics {
    #[serde(default, rename = "cvssMetricV31")]
    cvss_v31: Vec<NvdCvssMetric>,
    #[serde(default, rename = "cvssMetricV30")]
    cvss_v30: Vec<NvdCvssMetric>,
    #[serde(default, rename = "cvssMetricV2")]
    cvss_v2: Vec<NvdCvssMetricV2>,
}

#[derive(Debug, Deserialize)]
struct NvdCvssMetric {
    #[serde(rename = "cvssData")]
    cvss_data: NvdCvssData,
}

#[derive(Debug, Deserialize)]
struct NvdCvssData {
    #[serde(default, rename = "baseScore")]
    base_score: f64,
    #[serde(default, rename = "vectorString")]
    vector_string: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NvdCvssMetricV2 {
    #[serde(rename = "cvssData")]
    cvss_data: NvdCvssDataV2,
}

#[derive(Debug, Deserialize)]
struct NvdCvssDataV2 {
    #[serde(default, rename = "baseScore")]
    base_score: f64,
    #[serde(default, rename = "vectorString")]
    vector_string: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NvdReference {
    url: String,
}

#[derive(Debug, Deserialize)]
struct NvdConfiguration {
    #[serde(default)]
    nodes: Vec<NvdConfigNode>,
}

#[derive(Debug, Deserialize)]
struct NvdConfigNode {
    #[serde(default, rename = "cpeMatch")]
    cpe_match: Vec<NvdCpeMatch>,
}

#[derive(Debug, Deserialize)]
struct NvdCpeMatch {
    vulnerable: bool,
    criteria: String,
    #[serde(default, rename = "versionStartIncluding")]
    version_start_including: Option<String>,
    #[serde(default, rename = "versionEndExcluding")]
    version_end_excluding: Option<String>,
    #[serde(default, rename = "versionEndIncluding")]
    version_end_including: Option<String>,
}

// -- CISA KEV response types --

#[derive(Debug, Deserialize)]
struct CisaKevCatalog {
    #[serde(default)]
    vulnerabilities: Vec<CisaKevEntry>,
}

#[derive(Debug, Deserialize)]
struct CisaKevEntry {
    #[serde(rename = "cveID")]
    cve_id: String,
    #[serde(default, rename = "vendorProject")]
    vendor_project: String,
    #[serde(default)]
    product: String,
    #[serde(default, rename = "vulnerabilityName")]
    _vulnerability_name: String,
    #[serde(default, rename = "shortDescription")]
    short_description: String,
    #[serde(default, rename = "dateAdded")]
    date_added: String,
}

impl CveFeedService {
    pub fn new(repo: CveRepository) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .user_agent("OpenVox-WebUI/1.0")
            .build()
            .expect("Failed to build HTTP client");
        Self { client, repo }
    }

    /// Sync a feed source, dispatching by type.
    pub async fn sync_feed(&self, feed: &CveFeedSource) -> Result<FeedSyncResult> {
        info!(
            "Starting sync for feed '{}' ({})",
            feed.name,
            feed.feed_type.as_str()
        );

        let result = match feed.feed_type {
            CveFeedType::NvdJson => self.sync_nvd_feed(feed).await,
            CveFeedType::CisaKev => self.sync_cisa_kev(feed).await,
            CveFeedType::Custom => self.sync_nvd_feed(feed).await, // treat custom as NVD-compatible
        };

        match &result {
            Ok(r) => {
                self.repo
                    .update_feed_sync_status(&feed.id, "success", None)
                    .await?;
                info!(
                    "Feed '{}' sync complete: {} processed, {} new, {} updated",
                    feed.name, r.entries_processed, r.entries_new, r.entries_updated
                );
            }
            Err(e) => {
                let err_msg = format!("{:#}", e);
                self.repo
                    .update_feed_sync_status(&feed.id, "failed", Some(&err_msg))
                    .await
                    .ok();
                error!("Feed '{}' sync failed: {}", feed.name, err_msg);
            }
        }

        result
    }

    async fn sync_nvd_feed(&self, feed: &CveFeedSource) -> Result<FeedSyncResult> {
        let mut result = FeedSyncResult {
            feed_id: feed.id.clone(),
            entries_processed: 0,
            entries_new: 0,
            entries_updated: 0,
            package_matches_created: 0,
            errors: Vec::new(),
            synced_at: Utc::now(),
        };

        // NVD API supports pagination with startIndex and resultsPerPage
        let mut start_index = 0;
        let page_size = 200;
        let max_results = 2000; // Limit to avoid hitting rate limits

        loop {
            let url = format!(
                "{}?startIndex={}&resultsPerPage={}",
                feed.feed_url, start_index, page_size
            );

            debug!("Fetching NVD page: startIndex={}", start_index);

            let response = self
                .client
                .get(&url)
                .send()
                .await
                .context("Failed to fetch NVD feed")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "NVD API returned status {}: {}",
                    status,
                    body.chars().take(500).collect::<String>()
                ));
            }

            let nvd_response: NvdResponse = response
                .json()
                .await
                .context("Failed to parse NVD response JSON")?;

            let page_count = nvd_response.vulnerabilities.len();

            for vuln in &nvd_response.vulnerabilities {
                match self.process_nvd_cve(&vuln.cve, &feed.id).await {
                    Ok((is_new, match_count)) => {
                        result.entries_processed += 1;
                        if is_new {
                            result.entries_new += 1;
                        } else {
                            result.entries_updated += 1;
                        }
                        result.package_matches_created += match_count;
                    }
                    Err(e) => {
                        result
                            .errors
                            .push(format!("Error processing {}: {}", vuln.cve.id, e));
                    }
                }
            }

            start_index += page_count;

            if page_count < page_size
                || start_index >= max_results
                || start_index >= nvd_response.total_results
            {
                break;
            }

            // NVD rate limiting: ~5 requests per 30 seconds without API key
            tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        }

        Ok(result)
    }

    async fn process_nvd_cve(&self, cve: &NvdCve, feed_source_id: &str) -> Result<(bool, usize)> {
        let existing = self.repo.get_cve_entry(&cve.id).await?;
        let is_new = existing.is_none();

        let description = cve
            .descriptions
            .iter()
            .find(|d| d.lang == "en")
            .map(|d| d.value.as_str());

        let (cvss_score, cvss_vector) = extract_cvss(&cve.metrics);
        let severity = cvss_score
            .map(CveSeverity::from_cvss)
            .unwrap_or(CveSeverity::Unknown);

        let refs: Vec<String> = cve.references.iter().map(|r| r.url.clone()).collect();
        let affected: Vec<String> = cve
            .configurations
            .iter()
            .flat_map(|c| &c.nodes)
            .flat_map(|n| &n.cpe_match)
            .filter(|m| m.vulnerable)
            .map(|m| m.criteria.clone())
            .collect();

        self.repo
            .upsert_cve_entry(
                &cve.id,
                feed_source_id,
                description,
                severity,
                cvss_score,
                cvss_vector.as_deref(),
                cve.published.as_deref(),
                cve.last_modified.as_deref(),
                &refs,
                &affected,
                false,
            )
            .await?;

        // Extract package matches from CPE strings
        let mut package_matches: Vec<CvePackageMatch> = Vec::new();
        for config in &cve.configurations {
            for node in &config.nodes {
                for cpe_match in &node.cpe_match {
                    if !cpe_match.vulnerable {
                        continue;
                    }
                    if let Some(pm) = parse_cpe_to_package_match(&cpe_match.criteria) {
                        let version_end = cpe_match
                            .version_end_excluding
                            .clone()
                            .or_else(|| cpe_match.version_end_including.clone());
                        let version_start = cpe_match.version_start_including.clone();

                        package_matches.push(CvePackageMatch {
                            id: String::new(), // set by repo
                            cve_id: cve.id.clone(),
                            package_name: pm.package_name,
                            version_start: version_start.or(pm.version_start),
                            version_end: version_end.or(pm.version_end),
                            platform_family: pm.platform_family,
                            created_at: Utc::now(),
                        });
                    }
                }
            }
        }

        let match_count = if !package_matches.is_empty() {
            self.repo
                .upsert_package_matches(&cve.id, &package_matches)
                .await?
        } else {
            0
        };

        Ok((is_new, match_count))
    }

    async fn sync_cisa_kev(&self, feed: &CveFeedSource) -> Result<FeedSyncResult> {
        let mut result = FeedSyncResult {
            feed_id: feed.id.clone(),
            entries_processed: 0,
            entries_new: 0,
            entries_updated: 0,
            package_matches_created: 0,
            errors: Vec::new(),
            synced_at: Utc::now(),
        };

        let response = self
            .client
            .get(&feed.feed_url)
            .send()
            .await
            .context("Failed to fetch CISA KEV catalog")?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow::anyhow!("CISA KEV API returned status {}", status));
        }

        let catalog: CisaKevCatalog = response
            .json()
            .await
            .context("Failed to parse CISA KEV JSON")?;

        for entry in &catalog.vulnerabilities {
            match self.process_kev_entry(entry, &feed.id).await {
                Ok(is_new) => {
                    result.entries_processed += 1;
                    if is_new {
                        result.entries_new += 1;
                    } else {
                        result.entries_updated += 1;
                    }
                }
                Err(e) => {
                    result
                        .errors
                        .push(format!("Error processing KEV {}: {}", entry.cve_id, e));
                }
            }
        }

        Ok(result)
    }

    async fn process_kev_entry(&self, entry: &CisaKevEntry, feed_source_id: &str) -> Result<bool> {
        let existing = self.repo.get_cve_entry(&entry.cve_id).await?;
        let is_new = existing.is_none();

        let description = if entry.short_description.is_empty() {
            None
        } else {
            Some(entry.short_description.as_str())
        };

        // KEV entries don't always have CVSS scores, default to high severity
        let severity = if let Some(ref existing_entry) = existing {
            existing_entry.severity
        } else {
            CveSeverity::High
        };
        let cvss_score = existing.as_ref().and_then(|e| e.cvss_score);
        let cvss_vector = existing.as_ref().and_then(|e| e.cvss_vector.clone());

        let affected = vec![format!("{}:{}", entry.vendor_project, entry.product)];

        self.repo
            .upsert_cve_entry(
                &entry.cve_id,
                feed_source_id,
                description,
                severity,
                cvss_score,
                cvss_vector.as_deref(),
                Some(&entry.date_added),
                None,
                &[],
                &affected,
                true, // is_kev = true
            )
            .await?;

        // Create package match from product name
        if !entry.product.is_empty() {
            let pm = CvePackageMatch {
                id: String::new(),
                cve_id: entry.cve_id.clone(),
                package_name: entry.product.to_lowercase(),
                version_start: None,
                version_end: None,
                platform_family: None,
                created_at: Utc::now(),
            };
            self.repo
                .upsert_package_matches(&entry.cve_id, &[pm])
                .await?;
        }

        Ok(is_new)
    }
}

// -- Helper functions --

fn extract_cvss(metrics: &Option<NvdMetrics>) -> (Option<f64>, Option<String>) {
    let metrics = match metrics {
        Some(m) => m,
        None => return (None, None),
    };

    // Prefer CVSS v3.1, then v3.0, then v2
    if let Some(m) = metrics.cvss_v31.first() {
        return (
            Some(m.cvss_data.base_score),
            m.cvss_data.vector_string.clone(),
        );
    }
    if let Some(m) = metrics.cvss_v30.first() {
        return (
            Some(m.cvss_data.base_score),
            m.cvss_data.vector_string.clone(),
        );
    }
    if let Some(m) = metrics.cvss_v2.first() {
        return (
            Some(m.cvss_data.base_score),
            m.cvss_data.vector_string.clone(),
        );
    }

    (None, None)
}

/// Parse a CPE 2.3 string into a package match.
/// CPE format: cpe:2.3:part:vendor:product:version:update:edition:language:sw_edition:target_sw:target_hw:other
fn parse_cpe_to_package_match(cpe: &str) -> Option<CpeParseResult> {
    let parts: Vec<&str> = cpe.split(':').collect();
    if parts.len() < 6 {
        return None;
    }

    let part = parts.get(2)?; // a=application, o=os, h=hardware
    let _vendor = parts.get(3)?;
    let product = parts.get(4)?;
    let version = parts.get(5)?;

    if product.is_empty() || *product == "*" {
        return None;
    }

    // Map CPE part type to platform family hint
    let platform_family = match *part {
        "o" => {
            // OS-level product, try to infer platform from vendor/product
            let product_lower = product.to_lowercase();
            if product_lower.contains("linux")
                || product_lower.contains("ubuntu")
                || product_lower.contains("debian")
                || product_lower.contains("redhat")
                || product_lower.contains("centos")
                || product_lower.contains("oracle")
                || product_lower.contains("suse")
            {
                Some("linux".to_string())
            } else if product_lower.contains("windows") {
                Some("windows".to_string())
            } else if product_lower.contains("macos") || product_lower.contains("mac_os") {
                Some("macos".to_string())
            } else {
                None
            }
        }
        _ => None,
    };

    let (version_start, version_end) = if *version != "*" && !version.is_empty() {
        // Specific version: vulnerable at exactly this version
        (Some(version.to_string()), None)
    } else {
        (None, None)
    };

    Some(CpeParseResult {
        package_name: product.to_lowercase().replace('_', "-"),
        version_start,
        version_end,
        platform_family,
    })
}

struct CpeParseResult {
    package_name: String,
    version_start: Option<String>,
    version_end: Option<String>,
    platform_family: Option<String>,
}
