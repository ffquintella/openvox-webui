# Phase 3.2: Data Caching Layer

## Completed Tasks

- [x] Implement caching strategy for PuppetDB data
- [x] Background sync jobs for data freshness
- [x] Cache invalidation mechanisms
- [x] Configurable cache TTLs
- [x] Generic Cache<K,V> with TTL and max entries
- [x] CachedPuppetDbService wrapper
- [x] CacheSyncJob for background refresh
- [x] Per-resource TTL configuration (nodes, facts, reports, resources, catalogs)

## Details

Performance-optimized caching layer for PuppetDB data:

### Caching Strategy

- In-memory cache with time-based expiration
- Configurable TTL per resource type
- Maximum entry limits to prevent memory bloat
- Cache hit/miss metrics
- Automatic cache warming

### Generic Cache Implementation

```rust
pub struct Cache<K, V> {
    entries: Arc<DashMap<K, CacheEntry<V>>>,
    config: CacheConfig,
}
```

Features:
- Thread-safe concurrent access
- TTL-based expiration
- LRU eviction policy (when max entries exceeded)
- Size limiting

### Resource-Specific TTLs

| Resource | Default TTL | Configurable |
|----------|-------------|--------------|
| Nodes | 5 minutes | Yes |
| Facts | 10 minutes | Yes |
| Reports | 15 minutes | Yes |
| Resources | 10 minutes | Yes |
| Catalogs | 30 minutes | Yes |

### Background Sync

**CacheSyncJob:**
- Periodic background refresh of cached data
- Configurable sync interval (default: 1 minute)
- Selective refresh (only expired data)
- Error handling and retry logic
- Graceful degradation on PuppetDB unavailability

### Cache Invalidation

Multiple invalidation strategies:

1. **Time-based:** Entries expire after TTL
2. **Event-based:** Manual invalidation on external events
3. **Dependency-based:** Invalidate dependent data
4. **Selective:** Invalidate specific resources

### CachedPuppetDbService

Wrapper service providing:
- Transparent caching
- Cache hit monitoring
- Fallback to live queries
- Cache statistics

```rust
let cached_service = CachedPuppetDbService::new(
    puppetdb_client,
    cache_config,
);
```

### Configuration

```yaml
cache:
  enabled: true
  ttl:
    nodes: 300        # 5 minutes
    facts: 600        # 10 minutes
    reports: 900      # 15 minutes
    resources: 600
    catalogs: 1800    # 30 minutes
  max_entries: 10000
  sync_interval: 60   # seconds
```

### Performance Benefits

- Reduced PuppetDB server load
- Faster response times for cached queries
- Improved API latency (sub-millisecond cache hits)
- Bandwidth reduction
- Network resilience (stale cache fallback)

## Key Files

- `src/cache/mod.rs` - Cache implementation
- `src/cache/generic_cache.rs` - Generic cache structure
- `src/cache/sync_job.rs` - Background sync job
- `src/services/cached_puppetdb.rs` - Caching wrapper
- `src/config/cache_config.rs` - Cache configuration
