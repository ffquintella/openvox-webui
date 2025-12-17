# Puppet CA API

Base path: `/api/v1/ca`

## Endpoints
- `GET /status` — CA service status and counts (pending requests, signed certificates)
- `GET /requests` — List pending certificate signing requests (CSRs)
- `GET /certificates` — List signed certificates
- `GET /certificates/{certname}` — Get certificate details
- `POST /sign/{certname}` — Sign a CSR (optional `dns_alt_names` array)
- `POST /reject/{certname}` — Reject a CSR
- `DELETE /certificates/{certname}` — Revoke a signed certificate
- `POST /renew` — Renew the CA certificate (`{"days": <u32>}`)

## Request/Response Examples

### Sign a CSR
`POST /api/v1/ca/sign/node1.example.com`
```json
{
  "dns_alt_names": ["node1.example.com", "node1"]
}
```

### Renew CA certificate
`POST /api/v1/ca/renew`
```json
{
  "days": 3650
}
```

## Configuration
Add a `puppet_ca` block to `config.yaml`:
```yaml
puppet_ca:
  url: "https://puppetca.example.com:8140"
  timeout_secs: 30
  ssl_verify: true
  ssl_cert: "/etc/openvox-webui/ssl/ca_client.pem"
  ssl_key: "/etc/openvox-webui/ssl/ca_client.key"
  ssl_ca: "/etc/openvox-webui/ssl/ca.pem"
```

## RBAC
Resource: `certificates`
Actions: `read`, `sign`, `reject`, `revoke`, `admin`
