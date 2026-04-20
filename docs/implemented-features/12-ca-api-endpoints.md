# Phase 3.5: CA API Endpoints

## Completed Tasks

- [x] GET /api/v1/ca/status - CA service status
- [x] GET /api/v1/ca/requests - List pending certificate requests
- [x] GET /api/v1/ca/certificates - List signed certificates
- [x] POST /api/v1/ca/sign/:certname - Sign a certificate request
- [x] POST /api/v1/ca/reject/:certname - Reject a certificate request
- [x] DELETE /api/v1/ca/certificates/:certname - Revoke a certificate
- [x] POST /api/v1/ca/renew - Renew CA certificate
- [x] GET /api/v1/ca/certificates/:certname - Get certificate details

## Details

Complete API for Puppet CA certificate management operations:

### CA Status Endpoint

**GET /api/v1/ca/status**

Returns current CA health and metrics:

```json
{
  "service_status": "running",
  "ca_certificate": {
    "not_after": "2026-01-22T00:00:00Z",
    "not_before": "2021-01-22T00:00:00Z"
  },
  "pending_requests": 5,
  "signed_certificates": 245,
  "last_operation": {
    "type": "sign",
    "certname": "node.example.com",
    "timestamp": "2026-01-22T15:30:00Z",
    "status": "success"
  }
}
```

**Requires permission:** `ca:read`

### Certificate Requests Endpoints

**GET /api/v1/ca/requests**

List all pending certificate signing requests:

```json
{
  "data": [
    {
      "certname": "new-node.example.com",
      "fingerprint": "AB:CD:EF:...",
      "request_time": "2026-01-22T14:00:00Z",
      "status": "pending"
    }
  ],
  "total": 3,
  "limit": 50,
  "offset": 0
}
```

**Query parameters:**
- `limit` - Results per page (default: 50, max: 1000)
- `offset` - Result offset (default: 0)
- `certname` - Filter by certname pattern (optional)

**Requires permission:** `ca:read`

**POST /api/v1/ca/sign/:certname**

Sign a pending certificate request:

```json
{
  "certname": "new-node.example.com"
}
```

Response:
```json
{
  "certname": "new-node.example.com",
  "serial": "123456789",
  "fingerprint": "AB:CD:EF:...",
  "not_before": "2026-01-22T15:30:00Z",
  "not_after": "2036-01-20T15:30:00Z",
  "status": "valid"
}
```

**Requires permission:** `ca:sign`

Error responses:
- 404 - CSR not found
- 409 - Certificate already signed
- 503 - CA service unavailable

**POST /api/v1/ca/reject/:certname**

Reject a pending certificate request:

```json
{
  "reason": "Unauthorized node configuration"
}
```

Response:
```json
{
  "message": "Certificate request rejected",
  "certname": "new-node.example.com",
  "timestamp": "2026-01-22T15:31:00Z"
}
```

**Requires permission:** `ca:reject`

### Signed Certificates Endpoints

**GET /api/v1/ca/certificates**

List all signed certificates with pagination and filtering:

```json
{
  "data": [
    {
      "certname": "node1.example.com",
      "serial": "123456789",
      "fingerprint": "AB:CD:EF:...",
      "not_before": "2024-01-22T00:00:00Z",
      "not_after": "2026-01-22T00:00:00Z",
      "status": "valid",
      "days_until_expiry": 365
    }
  ],
  "total": 245,
  "limit": 50,
  "offset": 0
}
```

**Query parameters:**
- `limit` - Results per page (default: 50, max: 1000)
- `offset` - Result offset (default: 0)
- `certname` - Filter by certname (optional)
- `status` - Filter by status: valid, expired, revoked (optional)
- `sort_by` - Sort field: certname, not_after, not_before (default: certname)
- `sort_order` - asc or desc (default: asc)

**Requires permission:** `ca:read`

**GET /api/v1/ca/certificates/:certname**

Get detailed certificate information:

```json
{
  "certname": "node1.example.com",
  "serial": "123456789",
  "fingerprint": "AB:CD:EF:...",
  "issuer": "CN=Puppet CA,OU=Puppet,O=Puppet,ST=CA,C=US",
  "subject": "CN=node1.example.com,OU=Puppet,O=Puppet,ST=CA,C=US",
  "not_before": "2024-01-22T00:00:00Z",
  "not_after": "2026-01-22T00:00:00Z",
  "status": "valid",
  "extensions": [
    {
      "name": "keyUsage",
      "value": "digitalSignature, keyEncipherment"
    },
    {
      "name": "extendedKeyUsage",
      "value": "clientAuth, serverAuth"
    }
  ],
  "public_key": "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
}
```

**Requires permission:** `ca:read`

**DELETE /api/v1/ca/certificates/:certname**

Revoke a signed certificate:

```json
{
  "reason": "Node decommissioned"
}
```

Response:
```json
{
  "message": "Certificate revoked",
  "certname": "node1.example.com",
  "status": "revoked",
  "timestamp": "2026-01-22T15:32:00Z",
  "revocation_reason": "Node decommissioned"
}
```

**Requires permission:** `ca:revoke`

Error responses:
- 404 - Certificate not found
- 409 - Certificate already revoked
- 503 - CA service unavailable

### CA Renewal Endpoint

**POST /api/v1/ca/renew**

Initiate CA certificate renewal:

```json
{
  "backup": true
}
```

Response:
```json
{
  "message": "CA certificate renewal initiated",
  "old_certificate": {
    "not_after": "2026-01-22T00:00:00Z",
    "fingerprint": "AB:CD:EF:..."
  },
  "new_certificate": {
    "not_before": "2026-01-22T15:33:00Z",
    "not_after": "2036-01-20T15:33:00Z",
    "fingerprint": "12:34:56:..."
  },
  "backup_path": "/path/to/backup/ca_certificate.backup",
  "timestamp": "2026-01-22T15:33:00Z"
}
```

**Requires permission:** `ca:admin`

Error responses:
- 503 - CA service unavailable
- 500 - Renewal failed (check logs)

### Error Response Format

All endpoints use consistent error format:

```json
{
  "error": "Error message",
  "code": "ERROR_CODE",
  "details": {
    "certname": "affected-node",
    "timestamp": "2026-01-22T15:34:00Z"
  }
}
```

Common error codes:
- `CSR_NOT_FOUND` - Certificate request not found
- `CERT_NOT_FOUND` - Certificate not found
- `CERT_ALREADY_SIGNED` - CSR already signed
- `CERT_ALREADY_REVOKED` - Certificate already revoked
- `CA_UNAVAILABLE` - CA service not running
- `PERMISSION_DENIED` - Insufficient permissions
- `INVALID_OPERATION` - Operation not allowed

### Pagination & Filtering

All list endpoints support:
- Limit/offset pagination
- Filtering by various fields
- Sorting capabilities
- Search by certname pattern
- Status filtering

Standard query parameters:
- `limit` - 1-1000 (default: 50)
- `offset` - 0+ (default: 0)
- `search` - Pattern match on certname
- `sort_by` - Field to sort by
- `sort_order` - 'asc' or 'desc'

## Key Files

- `src/handlers/ca.rs` - Certificate endpoints
- `src/services/puppet_ca/` - CA service layer
- `src/models/certificate.rs` - Certificate data models
- `src/error.rs` - Error handling and codes
