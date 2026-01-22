# Phase 7.5: Puppet CA Management UI

## Completed Tasks

### 7.5.1 CA Dashboard - COMPLETE
- [x] CA status overview card (service health, certificate counts)
- [x] Pending certificate requests counter with badge
- [x] Quick actions for common CA operations
- [x] Certificate expiration warnings

### 7.5.2 Certificate Requests Management - COMPLETE
- [x] List pending certificate signing requests (CSRs)
- [x] CSR details view (certname, fingerprint, request time)
- [ ] Bulk sign/reject operations with confirmation dialogs
- [x] Individual sign/reject actions
- [ ] Filter CSRs by certname pattern
- [x] Auto-refresh for new requests (10 second interval)

### 7.5.3 Signed Certificates Management - COMPLETE
- [x] List all signed certificates with pagination
- [x] Certificate details view (certname, serial, expiration, fingerprint)
- [x] Revoke certificate with confirmation
- [x] Certificate expiration timeline/warnings
- [x] Filter/search by certname
- [x] Sort by expiration date, issue date, certname

### 7.5.4 CA Operations
- [ ] CA certificate renewal interface
- [ ] View CA certificate details
- [ ] Export CA certificate (for distribution)
- [ ] Certificate revocation list (CRL) status

## Details

Complete frontend for Puppet Certificate Authority management:

### CA Management Page

**Main Layout:**
- CA status card at top
- Tabbed interface:
  1. Certificate Requests
  2. Signed Certificates
  3. CA Operations (future)

### CA Status Card

Displays:
- Service status (running/stopped)
- Pending requests count (badge)
- Signed certificates count
- CA certificate expiration
- Last operation with timestamp
- Quick action buttons

### Certificate Requests Tab

**Features:**
- Table of pending CSRs
- Columns: Certname, Fingerprint, Request Time, Actions
- Auto-refresh (10 second interval)
- Individual sign action
- Individual reject action
- Confirmation dialog for operations
- Empty state when no requests
- Loading indicators

**Actions:**
- Click "Sign" button → Sign certificate
- Click "Reject" button → Reject CSR
- Both show confirmation dialog
- Operation status notification
- Auto-refresh after operation

### Signed Certificates Tab

**Features:**
- Paginated certificate list
- Columns: Certname, Serial, Not After, Status, Actions
- Status indicators (valid, expiring soon, expired, revoked)
- Color-coded expiration status:
  - Green: >30 days
  - Yellow: 7-30 days
  - Red: <7 days or expired
- Search by certname
- Sort controls (by: certname, expiration, issue date)
- Revoke action
- View details modal
- Bulk operations (future)

**Filters:**
- Status filter (valid, expiring, expired, revoked)
- Time range filter
- Environment filter (if applicable)

**Sorting:**
- By certname (ascending/descending)
- By not_after (ascending/descending)
- By issue date (ascending/descending)
- By serial (ascending/descending)

**Certificate Details Modal:**
- Full certificate information
- Certname and serial
- Fingerprint (copiable)
- Not before / Not after
- Subject and issuer
- Public key display
- Extensions display
- Copy buttons for key fields
- View certificate chain (future)

### Frontend Components

```
frontend/src/pages/
├── CA.tsx                           # Main CA page

frontend/src/pages/ca/
├── CertificateRequests.tsx         # CSR tab
├── SignedCertificates.tsx          # Certificates tab
├── CAStatus.tsx                    # Status card
└── CertificateDetails.tsx          # Details modal

frontend/src/components/ca/
├── CertificateTable.tsx            # Certificate listing table
├── CSRTable.tsx                    # CSR listing table
├── CertificateDetailsModal.tsx     # Details view
├── OperationConfirmation.tsx       # Confirm dialog
├── ExpirationWarning.tsx           # Expiration alert
└── CAStatusCard.tsx                # Status overview

frontend/src/hooks/
└── useCA.ts                        # API hooks for CA endpoints

frontend/src/services/
└── ca.ts                           # CA API client
```

### State Management

```typescript
// useCA.ts hooks:
useCAStatus()                 // Get CA status
usePendingRequests()          // List pending CSRs
useSignedCertificates()       // List certificates
useSignCertificate()          // Sign CSR
useRejectCertificate()        // Reject CSR
useRevokeCertificate()        # Revoke cert
useCertificateDetails()       // Get cert details
```

### API Integration

Integrates with backend endpoints:
- `GET /api/v1/ca/status` - CA status
- `GET /api/v1/ca/requests` - Pending CSRs
- `GET /api/v1/ca/certificates` - Signed certificates
- `POST /api/v1/ca/sign/:certname` - Sign CSR
- `POST /api/v1/ca/reject/:certname` - Reject CSR
- `DELETE /api/v1/ca/certificates/:certname` - Revoke cert
- `GET /api/v1/ca/certificates/:certname` - Get details

### Features

**Auto-refresh:**
- Certificate requests: 10 second interval
- Status card: 30 second interval
- Manual refresh button
- Last updated timestamp

**User Experience:**
- Toast notifications for actions
- Confirmation dialogs for destructive actions
- Loading states during operations
- Error messages with troubleshooting hints
- Empty state messaging
- Pagination controls
- Responsive table design

**Accessibility:**
- ARIA labels on buttons
- Keyboard navigation
- Focus management
- Screen reader friendly
- High contrast status indicators

**Performance:**
- Virtual scrolling for large lists
- Pagination instead of infinite scroll
- Debounced search input
- Memoized components
- React Query caching

### UI Components

**Status Indicators:**
- Green checkmark: Valid
- Yellow warning: Expiring soon
- Red X: Expired or revoked
- Gray question: Unknown

**Buttons:**
- Primary: Sign, Revoke
- Secondary: Reject, Details
- Danger: Revoke (with confirmation)

**Modals:**
- Certificate details modal
- Confirmation dialog
- Error alert dialog

## Key Files

- `frontend/src/pages/CA.tsx` - Main CA page
- `frontend/src/pages/ca/CertificateRequests.tsx` - CSR tab
- `frontend/src/pages/ca/SignedCertificates.tsx` - Certificates tab
- `frontend/src/hooks/useCA.ts` - API hooks
- `frontend/src/services/ca.ts` - API client
