//! PuppetDB data caching service
//!
//! Provides an in-memory caching layer for PuppetDB data to reduce
//! load on the PuppetDB server and improve response times.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::CacheConfig;
use crate::models::{Fact, Node, Report};
use crate::services::puppetdb::{Catalog, PuppetDbClient, Resource};

/// Cache entry with expiration tracking
#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    pub data: T,
    pub inserted_at: Instant,
    pub ttl: Duration,
}

impl<T> CacheEntry<T> {
    pub fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            inserted_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() > self.ttl
    }

    pub fn remaining_ttl(&self) -> Duration {
        self.ttl.saturating_sub(self.inserted_at.elapsed())
    }
}

/// Generic cache storage with TTL support
#[derive(Debug)]
pub struct Cache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    entries: RwLock<HashMap<K, CacheEntry<V>>>,
    max_entries: usize,
    default_ttl: Duration,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(max_entries: usize, default_ttl: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            max_entries,
            default_ttl,
        }
    }

    /// Get a value from cache if it exists and is not expired
    pub async fn get(&self, key: &K) -> Option<V> {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key) {
            if !entry.is_expired() {
                return Some(entry.data.clone());
            }
        }
        None
    }

    /// Get entry with metadata (including TTL info)
    pub async fn get_entry(&self, key: &K) -> Option<CacheEntry<V>> {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key) {
            if !entry.is_expired() {
                return Some(entry.clone());
            }
        }
        None
    }

    /// Set a value in cache with default TTL
    pub async fn set(&self, key: K, value: V) {
        self.set_with_ttl(key, value, self.default_ttl).await;
    }

    /// Set a value in cache with custom TTL
    pub async fn set_with_ttl(&self, key: K, value: V, ttl: Duration) {
        let mut entries = self.entries.write().await;

        // Evict expired entries if we're at capacity
        if entries.len() >= self.max_entries {
            self.evict_expired_locked(&mut entries);
        }

        // If still at capacity, remove oldest entry
        if entries.len() >= self.max_entries {
            if let Some(oldest_key) = self.find_oldest_key(&entries) {
                entries.remove(&oldest_key);
            }
        }

        entries.insert(key, CacheEntry::new(value, ttl));
    }

    /// Remove a value from cache
    pub async fn remove(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.write().await;
        entries.remove(key).map(|e| e.data)
    }

    /// Clear all entries from cache
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }

    /// Remove all expired entries
    pub async fn evict_expired(&self) -> usize {
        let mut entries = self.entries.write().await;
        self.evict_expired_locked(&mut entries)
    }

    fn evict_expired_locked(&self, entries: &mut HashMap<K, CacheEntry<V>>) -> usize {
        let before = entries.len();
        entries.retain(|_, entry| !entry.is_expired());
        before - entries.len()
    }

    fn find_oldest_key(&self, entries: &HashMap<K, CacheEntry<V>>) -> Option<K> {
        entries
            .iter()
            .min_by_key(|(_, entry)| entry.inserted_at)
            .map(|(k, _)| k.clone())
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read().await;
        let total = entries.len();
        let expired = entries.values().filter(|e| e.is_expired()).count();

        CacheStats {
            total_entries: total,
            expired_entries: expired,
            valid_entries: total - expired,
            max_entries: self.max_entries,
        }
    }

    /// Check if cache contains a non-expired entry for key
    pub async fn contains(&self, key: &K) -> bool {
        self.get(key).await.is_some()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub valid_entries: usize,
    pub max_entries: usize,
}

/// Type aliases for specific caches
pub type NodeCache = Cache<String, Node>;
pub type FactCache = Cache<String, Vec<Fact>>;
pub type ReportCache = Cache<String, Vec<Report>>;
pub type ResourceCache = Cache<String, Vec<Resource>>;
pub type CatalogCache = Cache<String, Catalog>;
pub type FactNamesCache = Cache<String, Vec<String>>;

/// PuppetDB cache service that wraps the PuppetDB client with caching
#[derive(Clone)]
pub struct CachedPuppetDbService {
    client: Arc<PuppetDbClient>,
    config: CacheConfig,
    nodes: Arc<NodeCache>,
    node_list: Arc<Cache<String, Vec<Node>>>,
    facts: Arc<FactCache>,
    fact_names: Arc<FactNamesCache>,
    reports: Arc<ReportCache>,
    resources: Arc<ResourceCache>,
    catalogs: Arc<CatalogCache>,
}

impl CachedPuppetDbService {
    /// Create a new cached PuppetDB service
    pub fn new(client: PuppetDbClient, config: CacheConfig) -> Self {
        let max_entries = config.max_entries;

        Self {
            client: Arc::new(client),
            nodes: Arc::new(NodeCache::new(
                max_entries,
                Duration::from_secs(config.node_ttl_secs),
            )),
            node_list: Arc::new(Cache::new(
                100, // Fewer node list entries
                Duration::from_secs(config.node_ttl_secs),
            )),
            facts: Arc::new(FactCache::new(
                max_entries,
                Duration::from_secs(config.fact_ttl_secs),
            )),
            fact_names: Arc::new(FactNamesCache::new(
                10, // Very few fact name entries
                Duration::from_secs(config.fact_ttl_secs),
            )),
            reports: Arc::new(ReportCache::new(
                max_entries,
                Duration::from_secs(config.report_ttl_secs),
            )),
            resources: Arc::new(ResourceCache::new(
                max_entries,
                Duration::from_secs(config.resource_ttl_secs),
            )),
            catalogs: Arc::new(CatalogCache::new(
                max_entries / 10, // Catalogs are large, store fewer
                Duration::from_secs(config.catalog_ttl_secs),
            )),
            config,
        }
    }

    /// Check if caching is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get access to the underlying PuppetDB client
    pub fn client(&self) -> &PuppetDbClient {
        &self.client
    }

    // ==================== Node Operations ====================

    /// Get all nodes (cached)
    pub async fn get_nodes(&self) -> Result<Vec<Node>> {
        if !self.config.enabled {
            return self.client.get_nodes().await;
        }

        let cache_key = "all_nodes".to_string();

        if let Some(nodes) = self.node_list.get(&cache_key).await {
            debug!("Cache hit: node list");
            return Ok(nodes);
        }

        debug!("Cache miss: node list");
        let nodes = self.client.get_nodes().await?;
        self.node_list.set(cache_key, nodes.clone()).await;

        // Also cache individual nodes
        for node in &nodes {
            self.nodes
                .set(node.certname.clone(), node.clone())
                .await;
        }

        Ok(nodes)
    }

    /// Get a specific node (cached)
    pub async fn get_node(&self, certname: &str) -> Result<Option<Node>> {
        if !self.config.enabled {
            return self.client.get_node(certname).await;
        }

        if let Some(node) = self.nodes.get(&certname.to_string()).await {
            debug!("Cache hit: node {}", certname);
            return Ok(Some(node));
        }

        debug!("Cache miss: node {}", certname);
        let node = self.client.get_node(certname).await?;

        if let Some(ref n) = node {
            self.nodes.set(certname.to_string(), n.clone()).await;
        }

        Ok(node)
    }

    // ==================== Fact Operations ====================

    /// Get facts for a node (cached)
    pub async fn get_node_facts(&self, certname: &str) -> Result<Vec<Fact>> {
        if !self.config.enabled {
            return self.client.get_node_facts(certname).await;
        }

        let cache_key = format!("facts:{}", certname);

        if let Some(facts) = self.facts.get(&cache_key).await {
            debug!("Cache hit: facts for {}", certname);
            return Ok(facts);
        }

        debug!("Cache miss: facts for {}", certname);
        let facts = self.client.get_node_facts(certname).await?;
        self.facts.set(cache_key, facts.clone()).await;

        Ok(facts)
    }

    /// Get all fact names (cached)
    pub async fn get_fact_names(&self) -> Result<Vec<String>> {
        if !self.config.enabled {
            return self.client.get_fact_names().await;
        }

        let cache_key = "all_fact_names".to_string();

        if let Some(names) = self.fact_names.get(&cache_key).await {
            debug!("Cache hit: fact names");
            return Ok(names);
        }

        debug!("Cache miss: fact names");
        let names = self.client.get_fact_names().await?;
        self.fact_names.set(cache_key, names.clone()).await;

        Ok(names)
    }

    // ==================== Report Operations ====================

    /// Get reports for a node (cached)
    pub async fn get_node_reports(
        &self,
        certname: &str,
        limit: Option<u32>,
    ) -> Result<Vec<Report>> {
        if !self.config.enabled {
            return self.client.get_node_reports(certname, limit).await;
        }

        let cache_key = format!("reports:{}:{:?}", certname, limit);

        if let Some(reports) = self.reports.get(&cache_key).await {
            debug!("Cache hit: reports for {}", certname);
            return Ok(reports);
        }

        debug!("Cache miss: reports for {}", certname);
        let reports = self.client.get_node_reports(certname, limit).await?;
        self.reports.set(cache_key, reports.clone()).await;

        Ok(reports)
    }

    // ==================== Resource Operations ====================

    /// Get resources for a node (cached)
    pub async fn get_node_resources(&self, certname: &str) -> Result<Vec<Resource>> {
        if !self.config.enabled {
            return self.client.get_node_resources(certname).await;
        }

        let cache_key = format!("resources:{}", certname);

        if let Some(resources) = self.resources.get(&cache_key).await {
            debug!("Cache hit: resources for {}", certname);
            return Ok(resources);
        }

        debug!("Cache miss: resources for {}", certname);
        let resources = self.client.get_node_resources(certname).await?;
        self.resources.set(cache_key, resources.clone()).await;

        Ok(resources)
    }

    // ==================== Catalog Operations ====================

    /// Get catalog for a node (cached)
    pub async fn get_node_catalog(&self, certname: &str) -> Result<Option<Catalog>> {
        if !self.config.enabled {
            return self.client.get_node_catalog(certname).await;
        }

        if let Some(catalog) = self.catalogs.get(&certname.to_string()).await {
            debug!("Cache hit: catalog for {}", certname);
            return Ok(Some(catalog));
        }

        debug!("Cache miss: catalog for {}", certname);
        let catalog = self.client.get_node_catalog(certname).await?;

        if let Some(ref c) = catalog {
            self.catalogs.set(certname.to_string(), c.clone()).await;
        }

        Ok(catalog)
    }

    // ==================== Cache Management ====================

    /// Invalidate cache for a specific node
    pub async fn invalidate_node(&self, certname: &str) {
        info!("Invalidating cache for node: {}", certname);

        self.nodes.remove(&certname.to_string()).await;
        self.facts
            .remove(&format!("facts:{}", certname))
            .await;
        self.resources
            .remove(&format!("resources:{}", certname))
            .await;
        self.catalogs.remove(&certname.to_string()).await;

        // Invalidate node list cache
        self.node_list.remove(&"all_nodes".to_string()).await;

        // Invalidate all report caches for this node
        // Note: This is a simplification; in production you might want
        // a more efficient approach
        self.reports.clear().await;
    }

    /// Invalidate all caches
    pub async fn invalidate_all(&self) {
        info!("Invalidating all caches");

        self.nodes.clear().await;
        self.node_list.clear().await;
        self.facts.clear().await;
        self.fact_names.clear().await;
        self.reports.clear().await;
        self.resources.clear().await;
        self.catalogs.clear().await;
    }

    /// Evict expired entries from all caches
    pub async fn evict_expired(&self) -> CacheEvictionStats {
        let nodes = self.nodes.evict_expired().await;
        let node_list = self.node_list.evict_expired().await;
        let facts = self.facts.evict_expired().await;
        let fact_names = self.fact_names.evict_expired().await;
        let reports = self.reports.evict_expired().await;
        let resources = self.resources.evict_expired().await;
        let catalogs = self.catalogs.evict_expired().await;

        let total = nodes + node_list + facts + fact_names + reports + resources + catalogs;

        if total > 0 {
            debug!("Evicted {} expired cache entries", total);
        }

        CacheEvictionStats {
            nodes,
            node_list,
            facts,
            fact_names,
            reports,
            resources,
            catalogs,
            total,
        }
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheServiceStats {
        CacheServiceStats {
            enabled: self.config.enabled,
            nodes: self.nodes.stats().await,
            node_list: self.node_list.stats().await,
            facts: self.facts.stats().await,
            fact_names: self.fact_names.stats().await,
            reports: self.reports.stats().await,
            resources: self.resources.stats().await,
            catalogs: self.catalogs.stats().await,
        }
    }

    /// Warm up the cache by fetching commonly accessed data
    pub async fn warm_up(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        info!("Warming up cache...");

        // Fetch and cache all nodes
        let nodes = self.get_nodes().await?;
        info!("Cached {} nodes", nodes.len());

        // Fetch and cache fact names
        let fact_names = self.get_fact_names().await?;
        info!("Cached {} fact names", fact_names.len());

        Ok(())
    }
}

/// Cache eviction statistics
#[derive(Debug, Clone)]
pub struct CacheEvictionStats {
    pub nodes: usize,
    pub node_list: usize,
    pub facts: usize,
    pub fact_names: usize,
    pub reports: usize,
    pub resources: usize,
    pub catalogs: usize,
    pub total: usize,
}

/// Overall cache service statistics
#[derive(Debug, Clone)]
pub struct CacheServiceStats {
    pub enabled: bool,
    pub nodes: CacheStats,
    pub node_list: CacheStats,
    pub facts: CacheStats,
    pub fact_names: CacheStats,
    pub reports: CacheStats,
    pub resources: CacheStats,
    pub catalogs: CacheStats,
}

/// Background sync job for keeping cache fresh
pub struct CacheSyncJob {
    service: CachedPuppetDbService,
    interval: Duration,
}

impl CacheSyncJob {
    pub fn new(service: CachedPuppetDbService, interval_secs: u64) -> Self {
        Self {
            service,
            interval: Duration::from_secs(interval_secs),
        }
    }

    /// Start the background sync job
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("Starting cache sync job with interval {:?}", self.interval);

            let mut interval = tokio::time::interval(self.interval);

            loop {
                interval.tick().await;

                // Evict expired entries
                let eviction_stats = self.service.evict_expired().await;
                if eviction_stats.total > 0 {
                    debug!(
                        "Cache sync: evicted {} expired entries",
                        eviction_stats.total
                    );
                }

                // Refresh node list
                match self.service.client.get_nodes().await {
                    Ok(nodes) => {
                        debug!("Cache sync: refreshed {} nodes", nodes.len());
                        // The get_nodes call will automatically cache the results
                        // through the CachedPuppetDbService
                    }
                    Err(e) => {
                        warn!("Cache sync: failed to refresh nodes: {}", e);
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));

        // Set and get
        cache.set("key1".to_string(), 42).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));

        // Non-existent key
        assert_eq!(cache.get(&"key2".to_string()).await, None);

        // Remove
        assert_eq!(cache.remove(&"key1".to_string()).await, Some(42));
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_millis(50));

        cache.set("key1".to_string(), 42).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(60)).await;

        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_cache_max_entries() {
        let cache: Cache<String, i32> = Cache::new(3, Duration::from_secs(60));

        cache.set("key1".to_string(), 1).await;
        cache.set("key2".to_string(), 2).await;
        cache.set("key3".to_string(), 3).await;

        // Adding a 4th entry should evict the oldest
        cache.set("key4".to_string(), 4).await;

        let stats = cache.stats().await;
        assert!(stats.total_entries <= 3);
    }

    #[tokio::test]
    async fn test_cache_evict_expired() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_millis(50));

        cache.set("key1".to_string(), 1).await;
        cache.set("key2".to_string(), 2).await;

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Add a non-expired entry
        cache
            .set_with_ttl("key3".to_string(), 3, Duration::from_secs(60))
            .await;

        let evicted = cache.evict_expired().await;
        assert_eq!(evicted, 2);

        let stats = cache.stats().await;
        assert_eq!(stats.valid_entries, 1);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));

        cache.set("key1".to_string(), 1).await;
        cache.set("key2".to_string(), 2).await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.valid_entries, 2);
        assert_eq!(stats.expired_entries, 0);
        assert_eq!(stats.max_entries, 100);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));

        cache.set("key1".to_string(), 1).await;
        cache.set("key2".to_string(), 2).await;

        cache.clear().await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_cache_contains() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));

        cache.set("key1".to_string(), 1).await;

        assert!(cache.contains(&"key1".to_string()).await);
        assert!(!cache.contains(&"key2".to_string()).await);
    }

    #[tokio::test]
    async fn test_cache_entry_remaining_ttl() {
        let cache: Cache<String, i32> = Cache::new(100, Duration::from_secs(60));

        cache.set("key1".to_string(), 1).await;

        let entry = cache.get_entry(&"key1".to_string()).await.unwrap();
        assert!(entry.remaining_ttl() > Duration::from_secs(59));
        assert!(entry.remaining_ttl() <= Duration::from_secs(60));
    }

    #[test]
    fn test_cache_config_defaults() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.node_ttl_secs, 300);
        assert_eq!(config.fact_ttl_secs, 300);
        assert_eq!(config.report_ttl_secs, 60);
        assert_eq!(config.resource_ttl_secs, 600);
        assert_eq!(config.catalog_ttl_secs, 600);
        assert_eq!(config.max_entries, 10000);
        assert_eq!(config.sync_interval_secs, 0);
    }
}
