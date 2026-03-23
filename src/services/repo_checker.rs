//! Repository metadata checker service.
//!
//! Fetches package metadata from YUM/APT/Zypper repositories to determine
//! the actual latest versions available, producing "repo-checked" catalog entries.

use std::collections::HashMap;
use std::io::Read as IoRead;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use flate2::read::GzDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::db::InventoryRepository;
use crate::models::{FleetRepositoryConfig, RepositoryVersionCatalogEntry};

/// A package version observed from a repository.
#[derive(Debug, Clone)]
pub struct RepoPackageVersion {
    pub name: String,
    pub epoch: Option<String>,
    pub version: String,
    pub release: Option<String>,
    pub architecture: Option<String>,
}

/// Summary of a repo check cycle.
#[derive(Debug, Default)]
pub struct RepoCheckSummary {
    pub repos_checked: usize,
    pub repos_succeeded: usize,
    pub repos_failed: usize,
    pub catalog_entries_upserted: usize,
}

pub struct RepoCheckerService {
    client: reqwest::Client,
    repo: InventoryRepository,
    max_concurrent: usize,
}

impl RepoCheckerService {
    pub fn new(repo: InventoryRepository, timeout_secs: u64, max_concurrent: usize) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("OpenVox-RepoChecker/1.0")
            .build()
            .expect("Failed to build HTTP client for repo checker");

        Self {
            client,
            repo,
            max_concurrent,
        }
    }

    /// Check all enabled fleet repository configs and update the version catalog.
    pub async fn check_all_repos(&self) -> Result<RepoCheckSummary> {
        let configs = self.repo.list_fleet_repository_configs().await?;
        if configs.is_empty() {
            debug!("No fleet repository configs to check");
            return Ok(RepoCheckSummary::default());
        }

        info!("Checking {} fleet repositories", configs.len());
        let mut summary = RepoCheckSummary::default();

        // Process repos with bounded concurrency
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(self.max_concurrent));
        let results: Vec<_> = futures::future::join_all(configs.iter().map(|config| {
            let sem = semaphore.clone();
            let client = self.client.clone();
            let config = config.clone();
            async move {
                let _permit = sem.acquire().await;
                let result = check_single_repo(&client, &config).await;
                (config, result)
            }
        }))
        .await;

        for (config, result) in results {
            summary.repos_checked += 1;
            match result {
                Ok(packages) => {
                    let count = packages.len();
                    if let Err(e) = self
                        .upsert_repo_checked_catalog(&config, &packages)
                        .await
                    {
                        error!(
                            "Failed to upsert catalog for repo '{}': {}",
                            config.repo_id, e
                        );
                        self.repo
                            .update_fleet_repo_check_status(
                                &config.id,
                                "error",
                                Some(&format!("Catalog upsert failed: {}", e)),
                            )
                            .await
                            .ok();
                        summary.repos_failed += 1;
                    } else {
                        info!(
                            "Repo '{}' ({}/{}): {} packages indexed",
                            config.repo_id, config.os_family, config.distribution, count
                        );
                        self.repo
                            .update_fleet_repo_check_status(&config.id, "success", None)
                            .await
                            .ok();
                        summary.repos_succeeded += 1;
                        summary.catalog_entries_upserted += count;
                    }
                }
                Err(e) => {
                    warn!("Failed to check repo '{}': {}", config.repo_id, e);
                    self.repo
                        .update_fleet_repo_check_status(
                            &config.id,
                            "error",
                            Some(&e.to_string()),
                        )
                        .await
                        .ok();
                    summary.repos_failed += 1;
                }
            }
        }

        Ok(summary)
    }

    /// Upsert repo-checked catalog entries for a given repo.
    async fn upsert_repo_checked_catalog(
        &self,
        config: &FleetRepositoryConfig,
        packages: &[RepoPackageVersion],
    ) -> Result<()> {
        // Keep only the highest version per package name
        let mut best: HashMap<String, &RepoPackageVersion> = HashMap::new();
        for pkg in packages {
            let entry = best.entry(pkg.name.clone()).or_insert(pkg);
            if crate::db::inventory_repository::compare_version_triplets(
                &pkg.version,
                pkg.release.as_deref(),
                &entry.version,
                entry.release.as_deref(),
            )
            .is_gt()
            {
                best.insert(pkg.name.clone(), pkg);
            }
        }

        let now = Utc::now();
        for (name, pkg) in &best {
            let entry = RepositoryVersionCatalogEntry {
                id: Uuid::new_v4().to_string(),
                platform_family: config.os_family.clone(),
                distribution: config.distribution.clone(),
                package_manager: Some(config.package_manager.clone()),
                software_type: "package".to_string(),
                software_name: name.clone(),
                repository_source: Some(config.repo_id.clone()),
                latest_version: pkg.version.clone(),
                latest_release: pkg.release.clone(),
                source_kind: "repo-checked".to_string(),
                observed_nodes: 0,
                last_seen_at: now,
                created_at: now,
                updated_at: now,
            };
            self.repo
                .upsert_catalog_entry(&entry)
                .await
                .with_context(|| format!("Failed to upsert catalog entry for '{}'", name))?;
        }

        Ok(())
    }
}

/// Check a single repository configuration and return all package versions found.
async fn check_single_repo(
    client: &reqwest::Client,
    config: &FleetRepositoryConfig,
) -> Result<Vec<RepoPackageVersion>> {
    match config.repo_type.as_str() {
        "yum" => check_yum_repo(client, config).await,
        "apt" => check_apt_repo(client, config).await,
        "zypper" => check_yum_repo(client, config).await, // Zypper uses same repodata format
        _ => {
            warn!("Unsupported repo type: {}", config.repo_type);
            Ok(vec![])
        }
    }
}

// ---- YUM/DNF repository checking ----

async fn check_yum_repo(
    client: &reqwest::Client,
    config: &FleetRepositoryConfig,
) -> Result<Vec<RepoPackageVersion>> {
    let base_url = resolve_yum_base_url(client, config).await?;

    // Fetch repomd.xml
    let repomd_url = format!("{}/repodata/repomd.xml", base_url.trim_end_matches('/'));
    debug!("Fetching repomd.xml from {}", repomd_url);
    let repomd_response = client
        .get(&repomd_url)
        .send()
        .await
        .context("Failed to fetch repomd.xml")?;

    if !repomd_response.status().is_success() {
        anyhow::bail!(
            "repomd.xml returned status {}",
            repomd_response.status()
        );
    }

    let repomd_text = repomd_response
        .text()
        .await
        .context("Failed to read repomd.xml body")?;

    // Parse repomd.xml to find primary.xml location
    let primary_href = parse_repomd_primary_href(&repomd_text)
        .context("Failed to find primary data in repomd.xml")?;

    // Fetch primary.xml.gz
    let primary_url = format!("{}/{}", base_url.trim_end_matches('/'), primary_href);
    debug!("Fetching primary.xml from {}", primary_url);
    let primary_response = client
        .get(&primary_url)
        .send()
        .await
        .context("Failed to fetch primary.xml")?;

    if !primary_response.status().is_success() {
        anyhow::bail!(
            "primary.xml returned status {}",
            primary_response.status()
        );
    }

    let primary_bytes = primary_response
        .bytes()
        .await
        .context("Failed to read primary.xml body")?;

    // Decompress if gzipped
    let primary_xml = if primary_href.ends_with(".gz") {
        let mut decoder = GzDecoder::new(&primary_bytes[..]);
        let mut decompressed = String::new();
        decoder
            .read_to_string(&mut decompressed)
            .context("Failed to decompress primary.xml.gz")?;
        decompressed
    } else {
        String::from_utf8(primary_bytes.to_vec()).context("primary.xml is not valid UTF-8")?
    };

    // Parse primary.xml for package info
    parse_yum_primary_xml(&primary_xml)
}

/// Resolve the actual base URL for a YUM repo, handling mirrorlist/metalink.
async fn resolve_yum_base_url(
    client: &reqwest::Client,
    config: &FleetRepositoryConfig,
) -> Result<String> {
    // Prefer base_url if available
    if let Some(ref base_url) = config.base_url {
        if !base_url.is_empty() {
            return Ok(base_url.clone());
        }
    }

    // Try mirrorlist
    if let Some(ref mirror_url) = config.mirror_list_url {
        if !mirror_url.is_empty() {
            debug!("Resolving mirrorlist: {}", mirror_url);
            let response = client
                .get(mirror_url)
                .send()
                .await
                .context("Failed to fetch mirrorlist")?;

            if response.status().is_success() {
                let body = response.text().await.context("Failed to read mirrorlist")?;

                // Metalink (XML) detection
                if body.trim_start().starts_with("<?xml") || body.contains("<metalink") {
                    if let Some(url) = parse_metalink_url(&body) {
                        return Ok(url);
                    }
                }

                // Plain mirrorlist: one URL per line
                for line in body.lines() {
                    let line = line.trim();
                    if !line.is_empty() && !line.starts_with('#') && line.starts_with("http") {
                        return Ok(line.to_string());
                    }
                }
            }
        }
    }

    anyhow::bail!(
        "No resolvable base URL for repo '{}' (no baseurl or mirrorlist)",
        config.repo_id
    )
}

/// Parse repomd.xml to find the href of the primary data file.
fn parse_repomd_primary_href(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut in_primary_data = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == b"data" {
                    // Check if type="primary"
                    in_primary_data = e.attributes().filter_map(|a| a.ok()).any(|a| {
                        a.key.local_name().as_ref() == b"type"
                            && a.unescape_value().map(|v| v == "primary").unwrap_or(false)
                    });
                }
                if in_primary_data && local.as_ref() == b"location" {
                    for attr in e.attributes().filter_map(|a| a.ok()) {
                        if attr.key.local_name().as_ref() == b"href" {
                            if let Ok(val) = attr.unescape_value() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if e.local_name().as_ref() == b"data" {
                    in_primary_data = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("Error parsing repomd.xml: {}", e);
                break;
            }
            _ => {}
        }
    }
    None
}

/// Parse a metalink XML response and extract the first mirror URL.
fn parse_metalink_url(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut in_url = false;
    let mut url_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                if e.local_name().as_ref() == b"url" {
                    // Check for protocol="https" or protocol="http"
                    let is_http = e.attributes().filter_map(|a| a.ok()).any(|a| {
                        a.key.local_name().as_ref() == b"protocol"
                            && a.unescape_value()
                                .map(|v| v == "https" || v == "http")
                                .unwrap_or(false)
                    });
                    if is_http {
                        in_url = true;
                        url_text.clear();
                    }
                }
            }
            Ok(Event::Text(e)) if in_url => {
                url_text.push_str(&String::from_utf8_lossy(e.as_ref()));
            }
            Ok(Event::End(ref e)) if e.local_name().as_ref() == b"url" && in_url => {
                let trimmed = url_text.trim().to_string();
                if !trimmed.is_empty() {
                    // Strip the filename from the URL to get the base
                    if let Some(pos) = trimmed.rfind('/') {
                        let base = &trimmed[..pos];
                        // Go up one more level to get repo base (strip /repodata)
                        if let Some(pos2) = base.rfind('/') {
                            return Some(base[..pos2].to_string());
                        }
                    }
                    return Some(trimmed);
                }
                in_url = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    None
}

/// Parse YUM primary.xml content to extract package versions.
fn parse_yum_primary_xml(xml: &str) -> Result<Vec<RepoPackageVersion>> {
    let mut packages = Vec::new();
    let mut reader = Reader::from_str(xml);

    let mut in_package = false;
    let mut in_name = false;
    let mut current_name = String::new();
    let mut current_epoch: Option<String> = None;
    let mut current_version = String::new();
    let mut current_release: Option<String> = None;
    let mut current_arch: Option<String> = None;
    let mut in_arch = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"package" => {
                        in_package = true;
                        current_name.clear();
                        current_epoch = None;
                        current_version.clear();
                        current_release = None;
                        current_arch = None;
                    }
                    b"name" if in_package => {
                        in_name = true;
                    }
                    b"arch" if in_package => {
                        in_arch = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                // <version epoch="0" ver="1.2.3" rel="1.el9"/>
                if in_package && e.local_name().as_ref() == b"version" {
                    for attr in e.attributes().filter_map(|a| a.ok()) {
                        match attr.key.local_name().as_ref() {
                            b"epoch" => {
                                let val = attr.unescape_value().unwrap_or_default().to_string();
                                if val != "0" && !val.is_empty() {
                                    current_epoch = Some(val);
                                }
                            }
                            b"ver" => {
                                current_version =
                                    attr.unescape_value().unwrap_or_default().to_string();
                            }
                            b"rel" => {
                                let val = attr.unescape_value().unwrap_or_default().to_string();
                                if !val.is_empty() {
                                    current_release = Some(val);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = String::from_utf8_lossy(e.as_ref());
                if in_name {
                    current_name.push_str(&text);
                }
                if in_arch {
                    current_arch = Some(text.to_string());
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"package" => {
                        if !current_name.is_empty() && !current_version.is_empty() {
                            packages.push(RepoPackageVersion {
                                name: current_name.clone(),
                                epoch: current_epoch.take(),
                                version: current_version.clone(),
                                release: current_release.take(),
                                architecture: current_arch.take(),
                            });
                        }
                        in_package = false;
                    }
                    b"name" => in_name = false,
                    b"arch" => in_arch = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("Error parsing primary.xml: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(packages)
}

// ---- APT repository checking ----

async fn check_apt_repo(
    client: &reqwest::Client,
    config: &FleetRepositoryConfig,
) -> Result<Vec<RepoPackageVersion>> {
    let base_url = config
        .base_url
        .as_deref()
        .context("APT repo has no base_url")?;
    let dist = config
        .distribution_path
        .as_deref()
        .context("APT repo has no distribution_path")?;

    let components: Vec<&str> = config
        .components
        .as_deref()
        .unwrap_or("main")
        .split_whitespace()
        .collect();

    let arch = config
        .architectures
        .as_deref()
        .unwrap_or("amd64");

    let mut all_packages = Vec::new();

    for component in &components {
        let packages_url = format!(
            "{}/dists/{}/{}/binary-{}/Packages.gz",
            base_url.trim_end_matches('/'),
            dist,
            component,
            arch
        );

        debug!("Fetching APT Packages from {}", packages_url);
        let response = client.get(&packages_url).send().await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                let bytes = resp.bytes().await.context("Failed to read Packages.gz")?;
                let mut decoder = GzDecoder::new(&bytes[..]);
                let mut decompressed = String::new();
                decoder
                    .read_to_string(&mut decompressed)
                    .context("Failed to decompress Packages.gz")?;

                let packages = parse_apt_packages(&decompressed)?;
                all_packages.extend(packages);
            }
            Ok(resp) => {
                debug!(
                    "APT Packages.gz returned {} for {}/{}",
                    resp.status(),
                    dist,
                    component
                );
            }
            Err(e) => {
                debug!(
                    "Failed to fetch APT Packages for {}/{}: {}",
                    dist, component, e
                );
            }
        }
    }

    Ok(all_packages)
}

/// Parse APT Packages file content (Debian control-file format).
fn parse_apt_packages(content: &str) -> Result<Vec<RepoPackageVersion>> {
    let mut packages = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_version: Option<String> = None;
    let mut current_arch: Option<String> = None;

    for line in content.lines() {
        if line.is_empty() {
            // End of package block
            if let (Some(name), Some(version_raw)) = (current_name.take(), current_version.take()) {
                let (epoch, version, release) = parse_debian_version(&version_raw);
                packages.push(RepoPackageVersion {
                    name,
                    epoch,
                    version,
                    release,
                    architecture: current_arch.take(),
                });
            }
            current_name = None;
            current_version = None;
            current_arch = None;
            continue;
        }

        if let Some(value) = line.strip_prefix("Package: ") {
            current_name = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Version: ") {
            current_version = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Architecture: ") {
            current_arch = Some(value.trim().to_string());
        }
    }

    // Handle last block if file doesn't end with blank line
    if let (Some(name), Some(version_raw)) = (current_name, current_version) {
        let (epoch, version, release) = parse_debian_version(&version_raw);
        packages.push(RepoPackageVersion {
            name,
            epoch,
            version,
            release,
            architecture: current_arch,
        });
    }

    Ok(packages)
}

/// Parse a Debian version string: [epoch:]upstream_version[-debian_revision]
fn parse_debian_version(raw: &str) -> (Option<String>, String, Option<String>) {
    let (epoch, remainder) = if let Some(pos) = raw.find(':') {
        let epoch = raw[..pos].to_string();
        let epoch = if epoch == "0" { None } else { Some(epoch) };
        (epoch, &raw[pos + 1..])
    } else {
        (None, raw)
    };

    let (version, release) = if let Some(pos) = remainder.rfind('-') {
        (
            remainder[..pos].to_string(),
            Some(remainder[pos + 1..].to_string()),
        )
    } else {
        (remainder.to_string(), None)
    };

    (epoch, version, release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_debian_version() {
        let (epoch, ver, rel) = parse_debian_version("2:4.10.2-2.0.1.el9_7");
        assert_eq!(epoch, Some("2".to_string()));
        assert_eq!(ver, "4.10.2");
        assert_eq!(rel, Some("2.0.1.el9_7".to_string()));

        let (epoch, ver, rel) = parse_debian_version("1.2.3-4ubuntu5");
        assert_eq!(epoch, None);
        assert_eq!(ver, "1.2.3");
        assert_eq!(rel, Some("4ubuntu5".to_string()));

        let (epoch, ver, rel) = parse_debian_version("0:1.0.0");
        assert_eq!(epoch, None);
        assert_eq!(ver, "1.0.0");
        assert_eq!(rel, None);
    }

    #[test]
    fn test_parse_repomd_primary_href() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<repomd xmlns="http://linux.duke.edu/metadata/repo">
  <data type="primary">
    <location href="repodata/abc123-primary.xml.gz"/>
    <checksum type="sha256">deadbeef</checksum>
  </data>
  <data type="filelists">
    <location href="repodata/abc123-filelists.xml.gz"/>
  </data>
</repomd>"#;

        let href = parse_repomd_primary_href(xml);
        assert_eq!(href, Some("repodata/abc123-primary.xml.gz".to_string()));
    }

    #[test]
    fn test_parse_yum_primary_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata xmlns="http://linux.duke.edu/metadata/common" packages="2">
  <package type="rpm">
    <name>bash</name>
    <arch>x86_64</arch>
    <version epoch="0" ver="5.1.8" rel="9.el9"/>
    <summary>The GNU Bourne Again shell</summary>
  </package>
  <package type="rpm">
    <name>kernel</name>
    <arch>x86_64</arch>
    <version epoch="0" ver="5.14.0" rel="427.el9"/>
    <summary>The Linux kernel</summary>
  </package>
</metadata>"#;

        let packages = parse_yum_primary_xml(xml).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "bash");
        assert_eq!(packages[0].version, "5.1.8");
        assert_eq!(packages[0].release, Some("9.el9".to_string()));
        assert_eq!(packages[1].name, "kernel");
    }

    #[test]
    fn test_parse_apt_packages() {
        let content = "Package: bash\nVersion: 5.1-6ubuntu1\nArchitecture: amd64\n\nPackage: coreutils\nVersion: 8.32-4.1ubuntu1\nArchitecture: amd64\n";

        let packages = parse_apt_packages(content).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "bash");
        assert_eq!(packages[0].version, "5.1");
        assert_eq!(packages[0].release, Some("6ubuntu1".to_string()));
    }
}
