# Phase 8: Advanced Features (Reporting, Alerting, Multi-Tenancy)

## Completed Tasks

### 8.1 Reporting & Analytics - COMPLETE
- [x] Custom report builder (SavedReports with ReportQueryConfig)
- [x] Scheduled report generation (ReportSchedule with cron expressions)
- [x] Report export (CSV, JSON formats - PDF also complete)
- [x] Compliance reporting (ComplianceBaseline, ComplianceReport)
- [x] Drift detection reports (DriftBaseline, DriftReport)
- [x] Background cron job for scheduled report execution
- [x] PDF export format using printpdf library

### 8.2 Alerting & Notifications - COMPLETE
- [x] Alert rule configuration with conditions
- [x] Condition evaluation engine (node status, facts, reports, filters)
- [x] Logical operators for complex rules (AND, OR)
- [x] Webhook notifications
- [x] Email notifications
- [x] Slack/Teams integration
- [x] Alert history and acknowledgment
- [x] Rule testing and validation

### 8.3 Multi-tenancy & Advanced RBAC - COMPLETE
- [x] Organization/tenant support
- [x] Tenant isolation (tenant-scoped queries for core resources)
- [x] Cross-tenant admin roles (`super_admin`)
- [x] Environment-based permissions (RBAC scope type)
- [x] API key management with scoped permissions
- [x] Comprehensive audit logging (foundation: API + persisted events)

## Details

Advanced features for enterprise deployments:

### Reporting System

**Report Types:**
- Node Health Report - Overall infrastructure health
- Compliance Report - Drift from baseline configuration
- Change Tracking Report - Resource changes over time
- Custom Report - User-defined queries and metrics

**Report Generation:**
- On-demand execution
- Scheduled via cron expressions
- Background job execution
- Execution history tracking
- Performance metrics

**Export Formats:**
- CSV - For spreadsheet analysis
- JSON - For API integration
- PDF - For distribution and printing

**Scheduling:**
```
Schedule: 0 0 * * * (daily at midnight)
Send to: webhook, email, slack
```

**Database Tables:**
- saved_reports - Saved report definitions
- report_schedules - Scheduled report configurations
- report_executions - Execution history
- compliance_baselines - Baseline configurations
- drift_baselines - Drift detection baselines
- report_templates - Pre-built templates

### Alerting System

**Alert Types:**
- Node Status - Node failure or degradation
- Compliance - Policy violations
- Drift - Configuration changes
- Report Failure - Failed scheduled reports
- Custom - User-defined conditions

**Condition Types:**
- Node Status - Match specific node statuses (failed, success, noop, unknown)
- Node Facts - Match fact values with operators (=, !=, >, <, >=, <=, ~, !~, in, not_in)
- Report Metrics - Match report metrics (resources changed, failed, failure percentage, runtime)
- Environment Filter - Scope to specific environments
- Group Filter - Scope to specific node groups
- Node Count Threshold - Trigger when N+ nodes match
- Time Window Filter - Only check recent events (last X minutes)

**Condition Operators:**
- String: `=`, `!=`, `~` (regex), `!~`, `in`, `not_in`, `exists`, `not_exists`
- Numeric: `=`, `!=`, `>`, `>=`, `<`, `<=`, `in`, `not_in`
- Boolean: `=`, `!=`
- Array: `contains`, `not_contains`, `in`, `not_in`

**Logical Operators:**
- AND - All conditions must be true
- OR - At least one condition must be true

**Rule Evaluation:**
- Background job runs every 5 minutes
- Conditions evaluated against PuppetDB data
- Cached node facts reduce load (10 min TTL)
- Matched nodes generate alert triggers
- Debouncing: 2 consecutive failures before alert

**Notification Channels:**
- Webhook - HTTP POST to custom endpoints
- Email - Direct email delivery
- Slack - Slack channel messages
- Microsoft Teams - Teams channel notifications

**Alert Management:**
- Create/edit/delete rules with complex conditions
- Test rules before enabling
- Manage notification channels
- Test channels
- Alert history and metrics
- Acknowledgment and resolution
- Alert silencing

**Features:**
- Flexible rule conditions with logical operators
- Multiple channels per rule
- Alert aggregation and deduplication
- Escalation policies
- Alert templates
- Rule testing and validation endpoint

For detailed condition structure, operators, and examples, see [ALERT_RULES_CONDITIONS.md](../ALERT_RULES_CONDITIONS.md).

### Multi-Tenancy

**Organization Model:**
- Organizations/tenants
- Users per organization
- Resources scoped to organization
- Cross-tenant admin (super_admin role)

**Tenant Isolation:**
- Queries scoped by organization_id
- API keys per tenant
- Separate audit logs
- Independent configurations
- Role assignments per tenant

**API Key Management:**
```
- Create API keys for programmatic access
- Scope permissions to key
- Organization-specific keys
- User-specific keys
- Key rotation support
- Key revocation
```

**Audit Logging:**
- API endpoints accessed
- Configuration changes
- User actions
- Failed operations
- Timestamp and user tracking
- Searchable audit trail

### Database Schema

**Multi-tenancy Tables:**
- organizations - Tenant definitions
- api_keys - API key management
- api_key_roles - Role assignments to keys
- audit_logs - Audit trail

**Reporting Tables:**
- saved_reports
- report_schedules
- report_executions
- compliance_baselines
- drift_baselines
- report_templates

**Alerting Tables:**
- notification_channels
- alert_rules
- alert_rule_channels
- alerts
- notification_history
- alert_silences

### API Endpoints

**Analytics/Reporting:**
```
GET/POST   /api/v1/analytics/saved-reports
POST       /api/v1/analytics/saved-reports/:id/execute
GET        /api/v1/analytics/saved-reports/:id/executions
GET        /api/v1/analytics/templates
GET/POST   /api/v1/analytics/schedules
POST       /api/v1/analytics/generate
POST       /api/v1/analytics/generate/:report_type
GET/POST   /api/v1/analytics/compliance-baselines
GET/POST   /api/v1/analytics/drift-baselines
GET        /api/v1/analytics/executions/:id/export
```

**Alerting:**
```
GET/POST   /api/v1/alerting/channels
POST       /api/v1/alerting/channels/:id/test
GET/POST   /api/v1/alerting/rules
POST       /api/v1/alerting/rules/:id/test           # Test rule evaluation
GET        /api/v1/alerting/alerts
GET        /api/v1/alerting/alerts/stats
POST       /api/v1/alerting/alerts/:id/acknowledge
POST       /api/v1/alerting/alerts/:id/resolve
POST       /api/v1/alerting/alerts/:id/silence
GET/POST   /api/v1/alerting/silences
POST       /api/v1/alerting/trigger
POST       /api/v1/alerting/evaluate              # Manually evaluate rule
```

**Multi-tenancy:**
```
GET/POST   /api/v1/organizations
GET        /api/v1/organizations/current
GET/POST   /api/v1/api-keys
DELETE     /api/v1/api-keys/:id
GET        /api/v1/audit-logs
```

### Frontend Components

**Analytics Page:**
- Tabbed interface (Reports, Compliance, Drift)
- Quick report generation buttons
- Report result visualization
- Saved reports management
- Report templates display
- Scheduled reports display

**Alerting Page:**
- Active alerts list with acknowledge/resolve
- Alert rules management
- Notification channel configuration
- Alert silences management
- Statistics dashboard
- Alert history

**Organization Management (Admin):**
- Organization list
- Create/edit/delete organizations
- User management per org
- API key generation
- Audit log viewer

### Frontend Hooks

```typescript
useAnalytics()              // Report functions
useAlerts()                 // Alert functions
useNotificationChannels()   // Channel management
useAlertRules()             // Rule management
useOrganizations()          // Org management
useAuditLogs()              // Audit log viewing
```

## Key Files

- `src/services/reporting.rs` - Report generation
- `src/services/alerting.rs` - Alert management
- `src/services/alerting/evaluator.rs` - Rule evaluation engine
- `src/services/alerting/conditions.rs` - Condition types and evaluation
- `src/handlers/analytics.rs` - Analytics endpoints
- `src/handlers/alerting.rs` - Alert endpoints
- `src/repositories/report_repository.rs` - Report persistence
- `src/repositories/alert_repository.rs` - Alert persistence
- `src/models/alert_rule.rs` - Alert rule models with conditions
- `docs/ALERT_RULES_CONDITIONS.md` - Conditions documentation
- `frontend/src/pages/Alerting.tsx` - Alerting UI
- `frontend/src/pages/alerting/RuleBuilder.tsx` - Rule builder component
- `frontend/src/pages/alerting/ConditionEditor.tsx` - Condition editor
