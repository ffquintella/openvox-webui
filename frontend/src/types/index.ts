// Node types
export type NodeStatus = 'changed' | 'unchanged' | 'failed' | 'unreported' | 'unknown';

export interface Node {
  certname: string;
  deactivated?: string | null;
  expired?: string | null;
  catalog_timestamp?: string | null;
  facts_timestamp?: string | null;
  report_timestamp?: string | null;
  catalog_environment?: string | null;
  facts_environment?: string | null;
  report_environment?: string | null;
  latest_report_status?: string | null;
  latest_report_corrective_change?: boolean | null;
  cached_catalog_status?: string | null;
}

// Node deletion response
export interface DeleteNodeResponse {
  success: boolean;
  message: string;
  pinned_associations_removed: number;
  certificate_revoked: boolean;
  puppetdb_deactivated: boolean;
}

// Group types
export type RuleMatchType = 'all' | 'any';

export type RuleOperator = '=' | '!=' | '~' | '!~' | '>' | '>=' | '<' | '<=' | 'in' | 'not_in';

export interface ClassificationRule {
  id: string;
  fact_path: string;
  operator: RuleOperator;
  value: unknown;
}

// Classes in Puppet Enterprise format: {"class_name": {"param": "value"}, ...}
export type PuppetClasses = Record<string, Record<string, unknown>>;

export interface NodeGroup {
  id: string;
  name: string;
  description?: string | null;
  parent_id?: string | null;
  environment?: string | null;
  /** When true, this group assigns its environment to matching nodes instead of filtering by it */
  is_environment_group?: boolean;
  rule_match_type: RuleMatchType;
  classes: PuppetClasses;
  variables: Record<string, unknown>;
  rules: ClassificationRule[];
  pinned_nodes: string[];
}

export interface CreateGroupRequest {
  name: string;
  description?: string;
  parent_id?: string;
  environment?: string;
  /** When true, this group assigns its environment to matching nodes instead of filtering by it */
  is_environment_group?: boolean;
  rule_match_type?: RuleMatchType;
  classes?: PuppetClasses;
  variables?: Record<string, unknown>;
}

export interface UpdateGroupRequest {
  name?: string;
  description?: string;
  parent_id?: string | null;
  environment?: string | null;
  /** When true, this group assigns its environment to matching nodes instead of filtering by it */
  is_environment_group?: boolean;
  rule_match_type?: RuleMatchType;
  classes?: PuppetClasses;
  variables?: Record<string, unknown>;
}

export interface CreateRuleRequest {
  fact_path: string;
  operator: RuleOperator;
  value: unknown;
}

export interface AddPinnedNodeRequest {
  certname: string;
}

// Report types
export type ReportStatus = 'changed' | 'unchanged' | 'failed';

export interface ReportMetrics {
  resources: {
    total: number;
    changed: number;
    failed: number;
    skipped: number;
  };
  time: {
    total: number;
    config_retrieval: number;
  };
  changes: number;
}

export interface Report {
  hash: string;
  certname: string;
  puppet_version?: string | null;
  report_format?: number | null;
  configuration_version?: string | null;
  start_time?: string | null;
  end_time?: string | null;
  producer_timestamp?: string | null;
  producer?: string | null;
  transaction_uuid?: string | null;
  status?: ReportStatus | null;
  corrective_change?: boolean | null;
  noop?: boolean | null;
  noop_pending?: boolean | null;
  environment?: string | null;
  metrics?: ReportMetrics | null;
}

// Resource Event types
export type EventStatus = 'success' | 'failure' | 'noop' | 'skipped';

export interface ResourceEvent {
  certname?: string | null;
  report?: string | null;
  resource_type: string;
  resource_title: string;
  property?: string | null;
  old_value?: unknown;
  new_value?: unknown;
  message?: string | null;
  status: EventStatus;
  timestamp?: string | null;
  containment_path?: string[] | null;
  corrective_change?: boolean | null;
  name?: string | null;
  file?: string | null;
  line?: number | null;
  containing_class?: string | null;
  run_start_time?: string | null;
  run_end_time?: string | null;
  configuration_version?: string | null;
  environment?: string | null;
}

// Fact types
export interface Fact {
  certname: string;
  name: string;
  value: unknown;
  environment?: string | null;
}

// Classification types
export type MatchType = 'rules' | 'pinned' | 'inherited';

export interface GroupMatch {
  id: string;
  name: string;
  match_type: MatchType;
  matched_rules: string[];
}

export interface ClassificationResult {
  certname: string;
  groups: GroupMatch[];
  classes: string[];
  parameters: Record<string, unknown>;
  environment?: string | null;
}

// Auth types
export interface User {
  id: string;
  username: string;
  email: string;
  role: 'admin' | 'user' | 'viewer';
  force_password_change?: boolean;
  auth_provider?: AuthProvider;
}

export interface AuthResponse {
  token: string;
  user: User;
}

export interface ChangePasswordRequest {
  current_password: string;
  new_password: string;
}

export interface ChangePasswordResponse {
  message: string;
}

// RBAC types
export type Resource =
  | 'nodes'
  | 'groups'
  | 'reports'
  | 'facts'
  | 'users'
  | 'roles'
  | 'settings'
  | 'audit_logs'
  | 'facter_templates'
  | 'api_keys';

export type Action =
  | 'read'
  | 'create'
  | 'update'
  | 'delete'
  | 'admin'
  | 'export'
  | 'classify'
  | 'generate';

export type Scope =
  | { type: 'all' }
  | { type: 'owned' }
  | { type: 'self' }
  | { type: 'specific' }
  | { type: 'environment'; value: string }
  | { type: 'group'; value: string };

export interface Permission {
  id: string;
  resource: Resource;
  action: Action;
  scope: Scope;
  constraint?: PermissionConstraint;
}

export type PermissionConstraint =
  | { type: 'ResourceIds'; value: string[] }
  | { type: 'Environments'; value: string[] }
  | { type: 'GroupIds'; value: string[] };

export interface Role {
  id: string;
  name: string;
  display_name: string;
  description?: string | null;
  is_system: boolean;
  parent_id?: string | null;
  permissions: Permission[];
  created_at: string;
  updated_at: string;
}

export interface CreateRoleRequest {
  name: string;
  display_name: string;
  description?: string;
  parent_id?: string;
  permissions?: Array<{
    resource: Resource;
    action: Action;
    scope?: Scope;
  }>;
}

export interface UserResponse {
  id: string;
  username: string;
  email: string;
  role: string;
  force_password_change?: boolean;
  auth_provider?: AuthProvider;
  roles?: Array<{ id: string; name: string; display_name: string }>;
  created_at: string;
}

export interface CreateUserRequest {
  username: string;
  email: string;
  password?: string; // Optional for SAML-only users
  role_ids?: string[];
  auth_provider?: AuthProvider;
  external_id?: string; // For SAML users
}

export interface UpdateUserRequest {
  username?: string;
  email?: string;
  password?: string;
  auth_provider?: AuthProvider;
  external_id?: string;
}

export interface EffectivePermissions {
  user_id: string;
  permissions: Permission[];
  roles: string[];
}

export interface ResourceInfo {
  name: string;
  display_name: string;
  description: string;
  available_actions: string[];
}

export interface ActionInfo {
  name: string;
  display_name: string;
  description: string;
}

// Permission Matrix types
export interface RoleInfo {
  id: string;
  name: string;
  display_name: string;
  is_system: boolean;
}

export interface ResourceWithActions {
  name: string;
  display_name: string;
  actions: string[];
}

export interface PermissionMatrix {
  roles: RoleInfo[];
  resources: ResourceWithActions[];
  matrix: Record<string, Record<string, Record<string, boolean>>>;
}

// Bulk permission operations
export type BulkOperationType = 'add' | 'remove' | 'replace';

export interface CreatePermissionRequest {
  resource: Resource;
  action: Action;
  scope?: Scope;
}

export interface BulkOperation {
  op: BulkOperationType;
  role_id: string;
  permission?: CreatePermissionRequest;
  permissions?: CreatePermissionRequest[];
}

export interface BulkPermissionRequest {
  operations: BulkOperation[];
}

export interface BulkOperationResult {
  index: number;
  success: boolean;
  error?: string;
  role?: Role;
}

export interface BulkPermissionResult {
  total: number;
  succeeded: number;
  failed: number;
  results: BulkOperationResult[];
}

// Facter types
export type FactValueSourceType = 'Static' | 'FromClassification' | 'FromFact' | 'Template';

export interface FactValueSource {
  type: FactValueSourceType;
  value: unknown;
}

export interface FactDefinition {
  name: string;
  value: FactValueSource;
}

export interface FactTemplate {
  id?: string;
  name: string;
  description?: string | null;
  facts: FactDefinition[];
}

export interface CreateFactTemplateRequest {
  name: string;
  description?: string;
  facts: FactDefinition[];
}

export interface UpdateFactTemplateRequest {
  name?: string;
  description?: string;
  facts?: FactDefinition[];
}

export interface GenerateFactsRequest {
  certname: string;
  template: string;
  existing_facts?: Record<string, unknown>;
}

export interface GeneratedFacts {
  certname: string;
  template: string;
  facts: Record<string, unknown>;
}

export type ExportFormat = 'json' | 'yaml' | 'shell';

// Settings types
export interface ServerSettings {
  host: string;
  port: number;
  workers: number;
}

export interface PuppetDbSettings {
  url: string;
  timeout_secs: number;
  ssl_verify: boolean;
  ssl_configured: boolean;
}

export interface PuppetCASettings {
  url: string;
  timeout_secs: number;
  ssl_verify: boolean;
  ssl_configured: boolean;
}

export interface AuthSettings {
  token_expiry_hours: number;
  refresh_token_expiry_days: number;
  password_min_length: number;
}

export interface DatabaseSettings {
  url_masked: string;
  max_connections: number;
  min_connections: number;
}

export interface LoggingSettings {
  level: string;
  format: string;
  file?: string | null;
}

export interface CacheSettings {
  enabled: boolean;
  node_ttl_secs: number;
  fact_ttl_secs: number;
  report_ttl_secs: number;
  max_entries: number;
}

export interface DashboardConfig {
  default_time_range: string;
  refresh_interval_secs: number;
  nodes_per_page: number;
  reports_per_page: number;
  show_inactive_nodes: boolean;
  inactive_threshold_hours: number;
  theme: string;
  widgets: WidgetConfig[];
}

export interface WidgetConfig {
  id: string;
  type: string;
  title?: string;
  enabled: boolean;
  position?: WidgetPosition;
  config?: Record<string, unknown>;
}

export interface WidgetPosition {
  row: number;
  col: number;
  width: number;
  height: number;
}

export interface RbacSettings {
  default_role: string;
  session_timeout_minutes: number;
  max_failed_logins: number;
  lockout_duration_minutes: number;
  custom_roles_count: number;
}

export interface NodeBootstrapSettings {
  openvox_server_url?: string | null;
  repository_base_url?: string | null;
  agent_package_name: string;
}

export interface SettingsResponse {
  server: ServerSettings;
  puppetdb?: PuppetDbSettings | null;
  puppet_ca?: PuppetCASettings | null;
  auth: AuthSettings;
  database: DatabaseSettings;
  logging: LoggingSettings;
  cache: CacheSettings;
  dashboard: DashboardConfig;
  rbac: RbacSettings;
  node_bootstrap?: NodeBootstrapSettings | null;
}

export interface RbacConfigResponse {
  default_role: string;
  session_timeout_minutes: number;
  max_failed_logins: number;
  lockout_duration_minutes: number;
  roles: RoleDefinition[];
}

export interface RoleDefinition {
  name: string;
  display_name?: string;
  description?: string;
  is_system: boolean;
  permissions: PermissionDefinition[];
}

export interface PermissionDefinition {
  resource: string;
  action: string;
  scope: string;
  scope_value?: string;
}

export interface ExportConfigResponse {
  content: string;
  format: string;
  timestamp: string;
}

export interface ImportConfigResponse {
  success: boolean;
  message: string;
  validation_errors: string[];
  dry_run: boolean;
}

export interface ValidationError {
  path: string;
  message: string;
  line?: number | null;
}

export interface ValidateConfigResponse {
  valid: boolean;
  errors: ValidationError[];
  warnings: string[];
}

export interface ConfigHistoryEntry {
  id: string;
  timestamp: string;
  user: string;
  action: string;
  changes_summary: string;
}

export interface ServerInfoResponse {
  version: string;
  rust_version: string;
  build_timestamp?: string | null;
  git_commit?: string | null;
  uptime_secs: number;
  config_file_path?: string | null;
  features: string[];
  saml: SamlFeatureInfo;
}

// SAML 2.0 types
export interface SamlFeatureInfo {
  enabled: boolean;
  configured: boolean;
  sp_entity_id?: string;
  idp_entity_id?: string;
  login_url?: string;
}

export type AuthProvider = 'local' | 'saml' | 'both';

// CA (Certificate Authority) types
export type CertificateStatus = 'requested' | 'signed' | 'rejected' | 'revoked';

export interface CertificateRequest {
  certname: string;
  requested_at: string;
  dns_alt_names: string[];
  fingerprint: string;
  state: CertificateStatus;
}

export interface Certificate {
  certname: string;
  serial: string;
  not_before: string;
  not_after: string;
  dns_alt_names: string[];
  fingerprint: string;
  state: CertificateStatus;
}

export interface CAStatus {
  available: boolean;
  ca_fingerprint?: string | null;
  ca_expires_at?: string | null;
  pending_requests: number;
  signed_certificates: number;
}

export interface SignRequest {
  dns_alt_names?: string[];
}

export interface SignResponse {
  certificate: Certificate;
  message: string;
}

export interface RejectResponse {
  certname: string;
  message: string;
}

export interface RevokeResponse {
  certname: string;
  message: string;
}

export interface RenewCARequest {
  days: number;
}

export interface RenewCAResponse {
  fingerprint: string;
  expires_at: string;
  message: string;
}

// Analytics & Reporting types
export type ReportType = 'node_health' | 'compliance' | 'change_tracking' | 'drift_detection' | 'custom';
export type OutputFormat = 'json' | 'csv' | 'pdf';
export type ExecutionStatus = 'pending' | 'running' | 'completed' | 'failed';
export type SeverityLevel = 'low' | 'medium' | 'high' | 'critical';

export interface ReportQueryConfig {
  time_range?: string;
  status_filter?: string[];
  environment_filter?: string[];
  node_group_filter?: string[];
  certname_pattern?: string;
  group_by?: string;
  include_resources?: boolean;
  include_error_details?: boolean;
  metrics?: string[];
  severity_filter?: string[];
  compare_mode?: string;
  ignore_volatile_facts?: boolean;
  custom_params?: Record<string, unknown>;
}

export interface SavedReport {
  id: string;
  name: string;
  description?: string;
  report_type: ReportType;
  query_config: ReportQueryConfig;
  created_by: string;
  is_public: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateSavedReportRequest {
  name: string;
  description?: string;
  report_type: ReportType;
  query_config: ReportQueryConfig;
  is_public?: boolean;
}

export interface UpdateSavedReportRequest {
  name?: string;
  description?: string;
  query_config?: ReportQueryConfig;
  is_public?: boolean;
}

export interface ReportSchedule {
  id: string;
  report_id: string;
  schedule_cron: string;
  timezone: string;
  is_enabled: boolean;
  output_format: OutputFormat;
  email_recipients?: string[];
  last_run_at?: string;
  next_run_at?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateScheduleRequest {
  report_id: string;
  schedule_cron: string;
  timezone?: string;
  is_enabled?: boolean;
  output_format?: OutputFormat;
  email_recipients?: string[];
}

export interface UpdateScheduleRequest {
  schedule_cron?: string;
  timezone?: string;
  is_enabled?: boolean;
  output_format?: OutputFormat;
  email_recipients?: string[];
}

export interface ReportExecution {
  id: string;
  report_id: string;
  schedule_id?: string;
  executed_by?: string;
  status: ExecutionStatus;
  started_at: string;
  completed_at?: string;
  row_count?: number;
  output_format: OutputFormat;
  output_data?: unknown;
  output_file_path?: string;
  error_message?: string;
  execution_time_ms?: number;
}

export interface ExecuteReportRequest {
  output_format?: OutputFormat;
  query_config_override?: ReportQueryConfig;
}

export interface ReportTemplate {
  id: string;
  name: string;
  description?: string;
  report_type: ReportType;
  query_config: ReportQueryConfig;
  is_system: boolean;
  created_at: string;
}

export interface ComplianceRule {
  id: string;
  name: string;
  description?: string;
  fact_name: string;
  operator: string;
  expected_value: unknown;
  severity: SeverityLevel;
}

export interface ComplianceBaseline {
  id: string;
  name: string;
  description?: string;
  rules: ComplianceRule[];
  severity_level: SeverityLevel;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CreateComplianceBaselineRequest {
  name: string;
  description?: string;
  rules: ComplianceRule[];
  severity_level?: SeverityLevel;
}

export interface DriftToleranceConfig {
  ignored_facts?: string[];
  numeric_tolerance_percent?: number;
  allow_minor_version_drift?: boolean;
}

export interface DriftBaseline {
  id: string;
  name: string;
  description?: string;
  node_group_id?: string;
  baseline_facts: Record<string, unknown>;
  tolerance_config?: DriftToleranceConfig;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CreateDriftBaselineRequest {
  name: string;
  description?: string;
  node_group_id?: string;
  baseline_facts: Record<string, unknown>;
  tolerance_config?: DriftToleranceConfig;
}

// Report Results
export interface NodeHealthSummary {
  total_nodes: number;
  changed_count: number;
  unchanged_count: number;
  failed_count: number;
  noop_count: number;
  unreported_count: number;
  compliance_rate: number;
}

export interface EnvironmentHealth {
  environment: string;
  total_nodes: number;
  changed_count: number;
  unchanged_count: number;
  failed_count: number;
}

export interface NodeHealthDetail {
  certname: string;
  environment?: string;
  status: string;
  last_report_at?: string;
  failed_resources?: number;
  changed_resources?: number;
}

export interface NodeHealthReport {
  generated_at: string;
  time_range: string;
  summary: NodeHealthSummary;
  by_environment?: EnvironmentHealth[];
  nodes?: NodeHealthDetail[];
}

export interface ComplianceSummary {
  total_nodes: number;
  compliant_nodes: number;
  non_compliant_nodes: number;
  compliance_rate: number;
  total_violations: number;
}

export interface SeverityBreakdown {
  severity: SeverityLevel;
  violation_count: number;
  affected_nodes: number;
}

export interface ComplianceViolation {
  certname: string;
  rule_id: string;
  rule_name: string;
  fact_name: string;
  expected_value: unknown;
  actual_value: unknown;
  severity: SeverityLevel;
}

export interface ComplianceReport {
  generated_at: string;
  baseline_name: string;
  summary: ComplianceSummary;
  by_severity: SeverityBreakdown[];
  violations: ComplianceViolation[];
}

export interface ChangeSummary {
  total_changes: number;
  nodes_affected: number;
  resources_changed: number;
  resources_failed: number;
}

export interface ChangeTypeBreakdown {
  resource_type: string;
  change_count: number;
}

export interface ChangeDetail {
  certname: string;
  report_time: string;
  resource_type: string;
  resource_title: string;
  property?: string;
  old_value?: unknown;
  new_value?: unknown;
  status: string;
}

export interface ChangeTrackingReport {
  generated_at: string;
  time_range: string;
  summary: ChangeSummary;
  changes_by_type: ChangeTypeBreakdown[];
  changes: ChangeDetail[];
}

export interface DriftSummary {
  total_nodes: number;
  nodes_with_drift: number;
  nodes_without_drift: number;
  drift_rate: number;
  total_drifted_facts: number;
}

export interface DriftedFact {
  fact_name: string;
  baseline_value: unknown;
  current_value: unknown;
  drift_severity: SeverityLevel;
}

export interface DriftedNode {
  certname: string;
  drift_count: number;
  drifted_facts: DriftedFact[];
}

export interface DriftReport {
  generated_at: string;
  baseline_name: string;
  summary: DriftSummary;
  drifted_nodes: DriftedNode[];
}

export type ReportResult =
  | { report_type: 'node_health' } & NodeHealthReport
  | { report_type: 'compliance' } & ComplianceReport
  | { report_type: 'change_tracking' } & ChangeTrackingReport
  | { report_type: 'drift_detection' } & DriftReport
  | { report_type: 'custom'; data: unknown };

export interface GenerateReportRequest {
  report_type: ReportType;
  config?: ReportQueryConfig;
  output_format?: OutputFormat;
}

// ============================================================================
// Alerting Types
// ============================================================================

export type ChannelType = 'webhook' | 'email' | 'slack' | 'teams';
export type AlertRuleType = 'node_status' | 'compliance' | 'drift' | 'report_failure' | 'custom';
export type AlertSeverity = 'info' | 'warning' | 'critical';
export type ConditionOperator = 'all' | 'any';
export type AlertStatus = 'active' | 'acknowledged' | 'resolved' | 'silenced';
export type NotificationStatus = 'pending' | 'sent' | 'failed' | 'retrying';

export interface WebhookConfig {
  url: string;
  method?: string;
  headers?: Record<string, string>;
  timeout_secs?: number;
  retry_count?: number;
}

export interface EmailConfig {
  smtp_host: string;
  smtp_port?: number;
  smtp_username?: string;
  smtp_password?: string;
  from: string;
  to: string[];
  use_tls?: boolean;
}

export interface SlackConfig {
  webhook_url: string;
  channel?: string;
  username?: string;
  icon_emoji?: string;
}

export interface TeamsConfig {
  webhook_url: string;
}

export type ChannelConfig = WebhookConfig | EmailConfig | SlackConfig | TeamsConfig;

export interface NotificationChannel {
  id: string;
  name: string;
  channel_type: ChannelType;
  config: ChannelConfig;
  is_enabled: boolean;
  created_by?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateChannelRequest {
  name: string;
  channel_type: ChannelType;
  config: ChannelConfig;
  is_enabled?: boolean;
}

export interface UpdateChannelRequest {
  name?: string;
  config?: ChannelConfig;
  is_enabled?: boolean;
}

export interface AlertCondition {
  field: string;
  operator: string;
  value: unknown;
}

export interface AlertRule {
  id: string;
  name: string;
  description?: string;
  rule_type: AlertRuleType;
  conditions: AlertCondition[];
  condition_operator: ConditionOperator;
  severity: AlertSeverity;
  cooldown_minutes: number;
  is_enabled: boolean;
  created_by?: string;
  created_at: string;
  updated_at: string;
  channels: string[];
}

export interface CreateAlertRuleRequest {
  name: string;
  description?: string;
  rule_type: AlertRuleType;
  conditions: AlertCondition[];
  condition_operator?: ConditionOperator;
  severity?: AlertSeverity;
  cooldown_minutes?: number;
  is_enabled?: boolean;
  channel_ids?: string[];
}

export interface UpdateAlertRuleRequest {
  name?: string;
  description?: string;
  conditions?: AlertCondition[];
  condition_operator?: ConditionOperator;
  severity?: AlertSeverity;
  cooldown_minutes?: number;
  is_enabled?: boolean;
  channel_ids?: string[];
}

export interface Alert {
  id: string;
  rule_id: string;
  title: string;
  message: string;
  severity: AlertSeverity;
  context?: Record<string, unknown>;
  status: AlertStatus;
  acknowledged_by?: string;
  acknowledged_at?: string;
  resolved_at?: string;
  triggered_at: string;
  last_notified_at?: string;
}

export interface AlertSilence {
  id: string;
  rule_id?: string;
  matchers?: Record<string, unknown>;
  starts_at: string;
  ends_at: string;
  reason: string;
  created_by?: string;
  created_at: string;
}

export interface CreateSilenceRequest {
  rule_id?: string;
  matchers?: Record<string, unknown>;
  starts_at?: string;
  ends_at: string;
  reason: string;
}

export interface AlertStats {
  total_active: number;
  by_severity: {
    info: number;
    warning: number;
    critical: number;
  };
  total_today: number;
  total_acknowledged: number;
}

export interface TestChannelRequest {
  message?: string;
}

export interface TestChannelResponse {
  success: boolean;
  message: string;
  response_code?: number;
}

export interface TriggerAlertRequest {
  rule_id: string;
  title: string;
  message: string;
  context?: Record<string, unknown>;
}

export interface NotificationHistory {
  id: string;
  alert_id: string;
  channel_id: string;
  status: NotificationStatus;
  attempt_count: number;
  response_code?: number;
  response_body?: string;
  error_message?: string;
  sent_at?: string;
  created_at: string;
}

// ============================================================================
// Code Deploy Types
// ============================================================================

export type DeploymentStatus = 'pending' | 'approved' | 'rejected' | 'deploying' | 'success' | 'failed' | 'cancelled';

export interface CodeSshKey {
  id: string;
  name: string;
  public_key: string;
  created_at: string;
  updated_at: string;
}

export interface CreateSshKeyRequest {
  name: string;
  private_key: string;
}

// PAT Token types
export interface CodePatToken {
  id: string;
  name: string;
  description?: string;
  username?: string;
  expires_at?: string;
  last_validated_at?: string;
  days_until_expiration?: number;
  is_expired: boolean;
  is_expiring_soon: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreatePatTokenRequest {
  name: string;
  description?: string;
  username?: string;
  token: string;
  expires_at?: string;
}

export interface UpdatePatTokenRequest {
  name?: string;
  description?: string;
  username?: string;
  token?: string;
  expires_at?: string;
}

export type AuthType = 'ssh' | 'pat' | 'none';

export interface CodeRepository {
  id: string;
  name: string;
  url: string;
  branch_pattern: string;
  auth_type: AuthType;
  ssh_key_id?: string;
  ssh_key_name?: string;
  pat_token_id?: string;
  pat_token_name?: string;
  has_pat: boolean;
  webhook_url?: string;
  poll_interval_seconds: number;
  is_control_repo: boolean;
  last_error?: string;
  last_error_at?: string;
  environment_count: number;
  created_at: string;
  updated_at: string;
}

export interface CreateRepositoryRequest {
  name: string;
  url: string;
  branch_pattern?: string;
  auth_type?: AuthType;
  ssh_key_id?: string;
  pat_token_id?: string;
  github_pat?: string; // Deprecated, use pat_token_id
  poll_interval_seconds?: number;
  is_control_repo?: boolean;
}

export interface UpdateRepositoryRequest {
  name?: string;
  url?: string;
  branch_pattern?: string;
  auth_type?: AuthType;
  ssh_key_id?: string;
  clear_ssh_key?: boolean;
  pat_token_id?: string;
  clear_pat_token?: boolean;
  github_pat?: string; // Deprecated, use pat_token_id
  clear_github_pat?: boolean;
  poll_interval_seconds?: number;
  is_control_repo?: boolean;
  regenerate_webhook_secret?: boolean;
}

export interface CodeDeploymentSummary {
  id: string;
  commit_sha: string;
  commit_message?: string;
  status: DeploymentStatus;
  created_at: string;
}

export interface CodeEnvironment {
  id: string;
  repository_id: string;
  repository_name: string;
  name: string;
  branch: string;
  current_commit?: string;
  current_commit_message?: string;
  current_commit_author?: string;
  current_commit_date?: string;
  last_synced_at?: string;
  auto_deploy: boolean;
  requires_approval: boolean;
  pending_deployment?: CodeDeploymentSummary;
  latest_deployment_status?: DeploymentStatus;
  created_at: string;
  updated_at: string;
}

export interface UpdateEnvironmentRequest {
  auto_deploy?: boolean;
  requires_approval?: boolean;
}

export interface CodeDeployment {
  id: string;
  environment_id: string;
  environment_name: string;
  repository_name: string;
  commit_sha: string;
  commit_message?: string;
  commit_author?: string;
  status: DeploymentStatus;
  requested_by?: string;
  requested_by_username?: string;
  approved_by?: string;
  approved_by_username?: string;
  approved_at?: string;
  rejected_at?: string;
  rejection_reason?: string;
  started_at?: string;
  completed_at?: string;
  duration_seconds?: number;
  error_message?: string;
  r10k_output?: string;
  created_at: string;
  updated_at: string;
}

export interface TriggerDeploymentRequest {
  environment_id: string;
  commit_sha?: string;
}

export interface ApproveDeploymentRequest {
  comment?: string;
}

export interface RejectDeploymentRequest {
  reason: string;
}

export interface ListDeploymentsQuery {
  environment_id?: string;
  repository_id?: string;
  status?: DeploymentStatus;
  limit?: number;
  offset?: number;
}

export interface ListEnvironmentsQuery {
  repository_id?: string;
  auto_deploy?: boolean;
  has_pending?: boolean;
}

// ============================================================================
// Group-Scoped Permissions Types
// ============================================================================

export interface GroupPermissionInfo {
  permission_id: string;
  group_id: string;
  group_name?: string;
  action: Action;
}

export interface AddGroupPermissionRequest {
  group_id: string;
  action: Action;
}

// ============================================================================
// Backup Types
// ============================================================================

export type BackupStatus = 'pending' | 'in_progress' | 'completed' | 'failed' | 'deleted';
export type BackupTrigger = 'manual' | 'scheduled';

export interface ServerBackup {
  id: string;
  filename: string;
  file_size: number;
  file_size_formatted: string;
  is_encrypted: boolean;
  trigger_type: BackupTrigger;
  status: BackupStatus;
  error_message?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  duration_seconds?: number | null;
  created_by?: string | null;
  created_by_username?: string | null;
  includes_database: boolean;
  includes_config: boolean;
  database_version?: string | null;
  notes?: string | null;
  created_at: string;
}

export interface BackupSchedule {
  id: string;
  name: string;
  is_active: boolean;
  frequency: string;
  cron_expression?: string | null;
  time_of_day: string;
  day_of_week: number;
  day_of_week_name: string;
  retention_count: number;
  last_run_at?: string | null;
  next_run_at?: string | null;
}

export interface BackupRestore {
  id: string;
  backup_id: string;
  backup_filename?: string | null;
  status: BackupStatus;
  error_message?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  duration_seconds?: number | null;
  restored_by?: string | null;
  restored_by_username?: string | null;
  created_at: string;
}

export interface BackupFeatureStatus {
  enabled: boolean;
  backup_dir: string;
  backup_dir_exists: boolean;
  backup_dir_writable: boolean;
  encryption_enabled: boolean;
  schedule_active: boolean;
  total_backups: number;
  total_size: number;
  total_size_formatted: string;
  last_backup_at?: string | null;
  next_scheduled_backup?: string | null;
}

export interface CreateBackupRequest {
  password?: string;
  notes?: string;
  include_database?: boolean;
  include_config?: boolean;
}

export interface RestoreBackupRequest {
  password: string;
  confirm: boolean;
}

export interface VerifyBackupRequest {
  password: string;
}

export interface VerifyBackupResponse {
  valid: boolean;
  checksum_match: boolean;
  can_decrypt: boolean;
  file_count?: number | null;
  total_size?: number | null;
  error?: string | null;
}

export interface UpdateBackupScheduleRequest {
  is_active?: boolean;
  frequency?: string;
  cron_expression?: string | null;
  time_of_day?: string | null;
  day_of_week?: number | null;
  retention_count?: number;
}

export interface ListBackupsQuery {
  status?: string;
  trigger_type?: string;
  limit?: number;
  offset?: number;
}

// ============================================================================
// Node Removal Types
// ============================================================================

export type RemovalReason = 'revoked_certificate' | 'no_certificate' | 'manual';
export type RemovalAuditAction = 'marked' | 'unmarked' | 'removed' | 'extended';

export interface PendingNodeRemoval {
  id: string;
  certname: string;
  removal_reason: RemovalReason;
  marked_at: string;
  scheduled_removal_at: string;
  removed_at?: string | null;
  notes?: string | null;
  marked_by?: string | null;
  days_remaining: number;
  is_overdue: boolean;
  created_at: string;
  updated_at: string;
}

export interface NodeRemovalAudit {
  id: string;
  certname: string;
  action: RemovalAuditAction;
  reason?: string | null;
  performed_by?: string | null;
  details?: string | null;
  created_at: string;
}

export interface PendingRemovalStats {
  total_pending: number;
  revoked_certificate_count: number;
  no_certificate_count: number;
  manual_count: number;
  due_today: number;
  overdue: number;
}

export interface NodeRemovalFeatureStatus {
  enabled: boolean;
  retention_days: number;
  check_interval_secs: number;
  puppetdb_connected: boolean;
  puppet_ca_connected: boolean;
}

export interface MarkNodeForRemovalRequest {
  certname: string;
  reason?: RemovalReason;
  notes?: string;
}

export interface ExtendRemovalDeadlineRequest {
  certname: string;
  extend_days: number;
  notes?: string;
}

// ============================================================================
// Node Bootstrap Types
// ============================================================================

export interface BootstrapConfigResponse {
  openvox_server_url?: string | null;
  repository_base_url?: string | null;
  agent_package_name: string;
  webui_url: string;
}
