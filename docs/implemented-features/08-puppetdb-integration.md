# Phase 3.1: PuppetDB Integration

## Completed Tasks

- [x] Implement PuppetDB API client
- [x] Support for PQL (Puppet Query Language)
- [x] Node queries (`/pdb/query/v4/nodes`)
- [x] Facts queries (`/pdb/query/v4/facts`)
- [x] Reports queries (`/pdb/query/v4/reports`)
- [x] Resources queries (`/pdb/query/v4/resources`)
- [x] Events queries (`/pdb/query/v4/events`)
- [x] Catalogs queries (`/pdb/query/v4/catalogs`)
- [x] SSL/TLS support with client certificates
- [x] QueryBuilder for AST queries
- [x] Pagination support (QueryParams)
- [x] Environment queries

## Details

Comprehensive integration with PuppetDB for infrastructure data access:

### PuppetDB Client

- HTTP/HTTPS client with SSL/TLS support
- Configurable connection pooling
- Request/response serialization
- Error handling and retry logic
- Timeout configuration

### Supported Queries

**Nodes:**
- List all nodes
- Node details with metadata
- Node status information
- Environment filtering
- Certification timestamps

**Facts:**
- Core facts per node
- Custom facts support
- Fact value querying
- Fact type/value filtering

**Reports:**
- Report history per node
- Report status and metrics
- Report timestamp filtering
- Report kind (apply, noop, etc.)

**Resources:**
- Resource catalog queries
- Resource type filtering
- Resource parameter queries
- Resource status

**Events:**
- Report events per node
- Event status filtering
- Event timestamp queries
- Event property filtering

**Catalogs:**
- Node catalog retrieval
- Edge relationships
- Resource dependencies

**Environment Queries:**
- Environment listing
- Environment-specific node queries
- Environment metadata

### PQL (Puppet Query Language)

- AST-based query building
- Operator support: =, !=, ~, !~, >, >=, <, <=
- Array operators: in, not_in
- Boolean operators: and, or, not
- Fact path traversal: facts.memory.physical_bytes

### QueryBuilder

Type-safe query construction:

```rust
QueryBuilder::new()
    .filter("certname", Operator::Eq, "node.example.com")
    .and()
    .filter("status", Operator::Eq, "failed")
    .build()
```

### Pagination

- Limit and offset support
- Per-page configuration
- Total result counting
- Efficient cursor-based pagination

### SSL/TLS Configuration

- Client certificate support
- CA certificate verification
- Certificate path configuration
- Custom CA bundle support

## API Endpoints

- `GET /api/v1/nodes` - List all nodes
- `GET /api/v1/nodes/:certname` - Get node details
- `GET /api/v1/nodes/:certname/facts` - Get node facts
- `GET /api/v1/nodes/:certname/reports` - Get node reports
- `GET /api/v1/facts` - Query facts across nodes
- `GET /api/v1/reports` - Query reports
- `POST /api/v1/query` - Execute PQL queries

## Key Files

- `src/services/puppetdb/` - PuppetDB client
- `src/services/puppetdb/client.rs` - HTTP client
- `src/services/puppetdb/query.rs` - Query building
- `src/models/puppetdb/` - PuppetDB data models
- `src/handlers/puppetdb.rs` - API endpoints
