# Phase 3.3: PuppetDB API Endpoints

## Completed Tasks

- [x] GET /api/v1/nodes - List all nodes
- [x] GET /api/v1/nodes/:certname - Get node details
- [x] GET /api/v1/nodes/:certname/facts - Get node facts
- [x] GET /api/v1/nodes/:certname/reports - Get node reports
- [x] GET /api/v1/facts - Query facts across nodes
- [x] GET /api/v1/reports - Query reports
- [x] POST /api/v1/query - Execute PQL queries

## Details

RESTful API endpoints for accessing PuppetDB data:

### Node Endpoints

**List all nodes:**
```
GET /api/v1/nodes?limit=20&offset=0&environment=production
```

Response: Array of nodes with metadata
- certname
- environment
- report_timestamp
- catalog_timestamp
- facts_timestamp
- status (success, failed, noop, unknown)
- latest_report_noop (boolean)

**Get node details:**
```
GET /api/v1/nodes/:certname
```

Response: Single node with all metadata and related data

**Get node facts:**
```
GET /api/v1/nodes/:certname/facts?limit=50&offset=0
```

Response: Array of facts
- name
- value
- environment

**Get node reports:**
```
GET /api/v1/nodes/:certname/reports?limit=10&offset=0
```

Response: Array of reports with metrics
- certname
- environment
- report_timestamp
- status
- metrics (resources_total, resources_changed, etc.)

### Fact Endpoints

**Query facts across all nodes:**
```
GET /api/v1/facts?name=os&limit=50
```

Response: Array of facts
- certname
- name
- value

### Report Endpoints

**Query reports:**
```
GET /api/v1/reports?status=failed&limit=20&environment=production
```

Response: Array of reports with metadata

### Custom Queries

**Execute PQL query:**
```
POST /api/v1/query
Content-Type: application/json

{
  "entity": "nodes",
  "query": [
    "and",
    ["=", "status", "failed"],
    ["=", "environment", "production"]
  ],
  "limit": 50,
  "offset": 0
}
```

Response: Query results matching the PQL expression

## Implementation Notes

- List endpoints return empty array when PuppetDB is not configured (backward compatibility)
- Detailed endpoints return 404 when resource is not found
- Advanced querying supports AST builder and pagination via QueryParams
- All endpoints support filtering and sorting
- Results are cached according to cache configuration
- Pagination defaults: limit=50, offset=0
- Maximum limit: 1000 entries

## Error Handling

- 400 Bad Request - Invalid query or parameters
- 401 Unauthorized - Authentication required
- 403 Forbidden - Insufficient permissions
- 404 Not Found - Resource not found
- 500 Internal Server Error - PuppetDB connection issues
- 503 Service Unavailable - PuppetDB service down

## Query Response Format

All responses include:
- `data` - Result set
- `total` - Total matching records (when applicable)
- `limit` - Applied limit
- `offset` - Applied offset
- `timestamp` - Response generation time

## Key Files

- `src/handlers/nodes.rs` - Node endpoints
- `src/handlers/facts.rs` - Fact endpoints
- `src/handlers/reports.rs` - Report endpoints
- `src/handlers/query.rs` - Custom query endpoint
- `src/services/puppetdb/` - PuppetDB service layer
