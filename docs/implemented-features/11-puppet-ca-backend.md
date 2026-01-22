# Phase 3.4: Puppet CA Management (Backend)

## Completed Tasks

- [x] Implement Puppet CA client integration
- [x] Certificate signing request (CSR) listing
- [x] Sign node certificate requests
- [x] Reject node certificate requests
- [x] Revoke signed node certificates
- [x] CA certificate renewal operations
- [x] Certificate status monitoring
- [x] RBAC: CA management permissions

## Details

Backend integration with Puppet Certificate Authority for certificate lifecycle management:

### Puppet CA Client

- Command-line interface to Puppet CA (`puppet cert` commands)
- Process execution with error handling
- Output parsing for certificate data
- Configuration path management

### Certificate Operations

**List Pending CSRs:**
```
GET /api/v1/ca/requests
```

Returns array of pending certificate signing requests with:
- certname
- fingerprint
- request_time
- status: "pending"

**Sign Certificate Request:**
```
POST /api/v1/ca/sign/:certname
```

- Verify CSR exists
- Execute sign operation
- Return signed certificate details
- Log operation for audit trail

**Reject Certificate Request:**
```
POST /api/v1/ca/reject/:certname
```

- Verify CSR exists
- Execute reject operation
- Remove CSR from pending queue
- Log rejection with reason

**List Signed Certificates:**
```
GET /api/v1/ca/certificates
```

Returns array of signed certificates with pagination:
- certname
- serial
- not_before
- not_after
- fingerprint
- status: "valid", "expired", "revoked"

**Revoke Certificate:**
```
DELETE /api/v1/ca/certificates/:certname
```

- Verify certificate exists
- Execute revoke operation
- Update certificate status
- Log revocation

**Get Certificate Details:**
```
GET /api/v1/ca/certificates/:certname
```

Returns detailed certificate information:
- certname
- serial
- fingerprint
- issuer
- subject
- not_before
- not_after
- extensions
- status

**CA Status:**
```
GET /api/v1/ca/status
```

Returns CA health information:
- service_status: "running", "stopped", "unknown"
- ca_certificate_expiration
- pending_requests_count
- signed_certificates_count
- last_operation_time

**Renew CA Certificate:**
```
POST /api/v1/ca/renew
```

- Initiate CA certificate renewal
- Backup existing certificate
- Generate new CA certificate
- Update CA configuration
- Return renewal status

### Data Models

**CertificateRequest:**
- certname: String
- fingerprint: String
- request_time: DateTime
- status: CertStatus

**Certificate:**
- certname: String
- serial: String
- fingerprint: String
- not_before: DateTime
- not_after: DateTime
- status: CertStatus
- issuer: String
- subject: String

**CAStatus:**
- service_status: ServiceStatus
- ca_certificate: Certificate
- pending_requests: Vec<CertificateRequest>
- signed_certificates: Vec<Certificate>

### RBAC Integration

CA management requires specific permissions:

| Operation | Permission | Role |
|-----------|-----------|------|
| View requests | `ca:read` | Admin, Operator |
| Sign request | `ca:sign` | Admin |
| Reject request | `ca:reject` | Admin |
| List certificates | `ca:read` | Admin, Operator |
| Revoke certificate | `ca:revoke` | Admin |
| Renew CA | `ca:admin` | Admin |

### Configuration

```yaml
puppet_ca:
  enabled: true
  ca_path: /etc/puppetlabs/puppet/ssl/ca
  command_timeout: 30
  verify_ssl: true
```

### Error Handling

- CSR not found (404)
- Certificate not found (404)
- CA service unavailable (503)
- Permission denied (403)
- Operation timeout (504)

### Audit Logging

All CA operations logged with:
- Operation type (sign, reject, revoke, renew)
- Certname involved
- User performing operation
- Timestamp
- Result (success/failure)
- Error details if failed

## Key Files

- `src/services/puppet_ca/` - Puppet CA service
- `src/services/puppet_ca/client.rs` - CA command execution
- `src/models/certificate.rs` - Certificate data models
- `src/handlers/ca.rs` - CA API endpoints
- `src/middleware/ca_permissions.rs` - CA RBAC checks
