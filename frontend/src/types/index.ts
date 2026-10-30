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

// Query parameters for the paginated node list
export interface NodesQueryParams {
  environment?: string;
  status?: string;
  search?: string;
  limit?: number;
  offset?: number;
  order_by?: string;
  order_dir?: 'asc' | 'desc';
}

// Paginated node list result (data plus total count from X-Total-Count header)
export interface PaginatedNodes {
  nodes: Node[];
  total: number;
}

// Aggregate node statistics (from GET /nodes/stats)
export interface NodeStats {
  total: number;
  by_status: Record<string, number>;
  by_environment: Record<string, number>;
  by_health: Record<string, number>;
}

// Node deletion response
export interface DeleteNodeResponse {
  success: boolean;
  message: string;
  pinned_associations_removed: number;
  certificate_revoked: boolean;
  puppetdb_deactivated: boolean;
}

export interface InventorySnapshotSummary {
  id: string;
  certname: string;
  collector_version: string;
  collected_at: string;
  is_full_snapshot: boolean;
  os_family: string;
  distribution: string;
  os_version: string;
  package_count: number;
  application_count: number;
  website_count: number;
  runtime_count: number;
  container_count: number;
  user_count: number;
  created_at: string;
}

export interface HostOsInventory {
  os_family: string;
  distribution: string;
  edition?: string | null;
  architecture?: string | null;
  kernel_version?: string | null;
  os_version: string;
  patch_level?: string | null;
  package_manager?: string | null;
  update_channel?: string | null;
  last_inventory_at?: string | null;
  last_successful_update_at?: string | null;
}

export interface HostPackageInventoryItem {
  name: string;
  epoch?: string | null;
  version: string;
  release?: string | null;
  architecture?: string | null;
  repository_source?: string | null;
  install_path?: string | null;
  install_time?: string | null;
}

export interface HostApplicationInventoryItem {
  name: string;
  publisher?: string | null;
  version: string;
  architecture?: string | null;
  install_scope?: string | null;
  install_path?: string | null;
  application_type?: string | null;
  bundle_identifier?: string | null;
  uninstall_identity?: string | null;
  install_date?: string | null;
  metadata?: unknown;
}

export interface HostWebInventoryItem {
  server_type: string;
  site_name: string;
  bindings: string[];
  document_root?: string | null;
  application_pool?: string | null;
  tls_certificate_reference?: string | null;
  metadata?: unknown;
}

export interface HostRuntimeInventoryItem {
  runtime_type: string;
  runtime_name: string;
  runtime_version?: string | null;
  install_path?: string | null;
  management_endpoint?: string | null;
  deployed_units: string[];
  metadata?: unknown;
}

export interface HostContainerInventoryItem {
  container_id: string;
  name: string;
  image?: string | null;
  status: string;
  status_detail?: string | null;
  created_at?: string | null;
  ports: string[];
  mounts: string[];
  runtime_type: string;
  metadata?: unknown;
}

export interface HostUserInventoryItem {
  username: string;
  uid?: number | null;
  sid?: string | null;
  gid?: number | null;
  home_directory?: string | null;
  shell?: string | null;
  user_type?: string | null;
  groups: string[];
  last_login?: string | null;
  locked?: boolean | null;
  gecos?: string | null;
  metadata?: unknown;
}

export interface InventorySummary {
  certname: string;
  os_family: string;
  distribution: string;
  os_version: string;
  patch_level?: string | null;
  architecture?: string | null;
  package_manager?: string | null;
  update_channel?: string | null;
  last_inventory_at?: string | null;
  last_successful_update_at?: string | null;
  package_count: number;
  application_count: number;
  website_count: number;
  runtime_count: number;
  container_count: number;
  user_count: number;
  collected_at: string;
  collector_version: string;
  is_stale: boolean;
}

export interface OutdatedInventoryItem {
  software_type: string;
  name: string;
  installed_version: string;
  installed_release?: string | null;
  latest_version: string;
  latest_release?: string | null;
  repository_source?: string | null;
  source_kind?: string | null;
}

export interface FleetRepositoryConfig {
  id: string;
  os_family: string;
  distribution: string;
  os_version_pattern: string;
  package_manager: string;
  repo_id: string;
  repo_name?: string | null;
  repo_type: string;
  base_url?: string | null;
  mirror_list_url?: string | null;
  distribution_path?: string | null;
  components?: string | null;
  architectures?: string | null;
  enabled: boolean;
  last_checked_at?: string | null;
  last_check_status?: string | null;
  last_check_error?: string | null;
  reporting_nodes: number;
  created_at: string;
  updated_at: string;
}

export interface HostUpdateStatus {
  certname: string;
  snapshot_id?: string | null;
  is_stale: boolean;
  stale_reason?: string | null;
  outdated_packages: number;
  outdated_applications: number;
  total_packages: number;
  total_applications: number;
  checked_at: string;
  outdated_items: OutdatedInventoryItem[];
}

export interface RepositoryVersionCatalogEntry {
  id: string;
  platform_family: string;
  distribution: string;
  os_version_pattern?: string | null;
  package_manager?: string | null;
  software_type: string;
  software_name: string;
  repository_source?: string | null;
  latest_version: string;
  latest_release?: string | null;
  source_kind: string;
  observed_nodes: number;
  last_seen_at: string;
  created_at: string;
  updated_at: string;
}

// Group Update Schedules

export interface GroupUpdateSchedule {
  id: string;
  group_id: string;
  name: string;
  description?: string | null;
  schedule_type: 'one_time' | 'recurring';
  cron_expression?: string | null;
  scheduled_for?: string | null;
  operation_type: UpdateOperationType;
  package_names: string[];
  requires_approval: boolean;
  maintenance_window_start?: string | null;
  maintenance_window_end?: string | null;
  enabled: boolean;
  last_run_at?: string | null;
  next_run_at?: string | null;
  last_job_id?: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CreateGroupUpdateScheduleRequest {
  name: string;
  description?: string;
  schedule_type: 'one_time' | 'recurring';
  cron_expression?: string;
  scheduled_for?: string;
  operation_type: UpdateOperationType;
  package_names?: string[];
  requires_approval?: boolean;
  maintenance_window_start?: string;
  maintenance_window_end?: string;
}

export interface UpdateGroupUpdateScheduleRequest {
  name?: string;
  description?: string;
  cron_expression?: string;
  scheduled_for?: string;
  operation_type?: UpdateOperationType;
  package_names?: string[];
  requires_approval?: boolean;
  maintenance_window_start?: string;
  maintenance_window_end?: string;
  enabled?: boolean;
}

export interface InventoryFleetStatusSummary {
  total_nodes: number;
  stale_nodes: number;
  nodes_with_inventory: number;
  nodes_without_inventory: number;
  outdated_nodes: number;
  outdated_packages: number;
  outdated_applications: number;
  generated_at: string;
}

export interface InventoryDistributionPoint {
  label: string;
  value: number;
}

export interface PatchAgeBucket {
  label: string;
  value: number;
}

export interface TopOutdatedSoftwareItem {
  software_type: string;
  name: string;
  affected_nodes: number;
}

export interface InventoryDashboardReport {
  summary: InventoryFleetStatusSummary;
  platform_distribution: InventoryDistributionPoint[];
  os_distribution: InventoryDistributionPoint[];
  update_compliance: InventoryDistributionPoint[];
  patch_age_buckets: PatchAgeBucket[];
  top_outdated_software: TopOutdatedSoftwareItem[];
}

export interface OutdatedSoftwareNodeDetail {
  certname: string;
  installed_version: string;
  latest_version: string;
}

export interface ComplianceCategoryNode {
  certname: string;
  is_stale: boolean;
  outdated_packages: number;
  outdated_applications: number;
  checked_at: string;
}

export interface PatchAgeBucketNode {
  certname: string;
  age_days: number | null;
  last_patched_at: string | null;
}

export type UpdateJobStatus =
  | 'pending_approval'
  | 'approved'
  | 'rejected'
  | 'in_progress'
  | 'completed'
  | 'completed_with_failures'
  | 'cancelled';

export type UpdateTargetStatus =
  'pending_approval' | 'queued' | 'dispatched' | 'succeeded' | 'failed' | 'cancelled' | 'rejected';

export type UpdateOperationType =
  'package_update' | 'package_install' | 'package_remove' | 'system_patch' | 'security_patch';

export interface UpdateJobTarget {
  id: string;
  certname: string;
  status: UpdateTargetStatus;
  dispatched_at?: string | null;
  completed_at?: string | null;
  last_error?: string | null;
}

export interface UpdateJobResult {
  id: string;
  target_id: string;
  certname: string;
  status: UpdateTargetStatus;
  summary?: string | null;
  output?: string | null;
  started_at?: string | null;
  finished_at: string;
}

export interface UpdateJob {
  id: string;
  status: UpdateJobStatus;
  operation_type: UpdateOperationType;
  package_names: string[];
  target_group_id?: string | null;
  target_nodes: string[];
  requires_approval: boolean;
  scheduled_for?: string | null;
  maintenance_window_start?: string | null;
  maintenance_window_end?: string | null;
  requested_by: string;
  approved_by?: string | null;
  approval_notes?: string | null;
  created_at: string;
  updated_at: string;
  targets: UpdateJobTarget[];
  results: UpdateJobResult[];
}

export interface CreateUpdateJobRequest {
  operation_type: UpdateOperationType;
  package_names?: string[];
  certnames?: string[];
  group_id?: string | null;
  target_all_outdated?: boolean;
  requires_approval?: boolean;
  scheduled_for?: string | null;
  maintenance_window_start?: string | null;
  maintenance_window_end?: string | null;
  approval_notes?: string | null;
}

export interface ApproveUpdateJobRequest {
  approved: boolean;
  notes?: string | null;
}

export interface NodeInventory {
  snapshot: InventorySnapshotSummary;
  summary: InventorySummary;
  update_status?: HostUpdateStatus | null;
  os: HostOsInventory;
  packages: HostPackageInventoryItem[];
  applications: HostApplicationInventoryItem[];
  websites: HostWebInventoryItem[];
  runtimes: HostRuntimeInventoryItem[];
  containers: HostContainerInventoryItem[];
  users: HostUserInventoryItem[];
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
  /** When true, groups with no rules will match all nodes (within parent context) */
  match_all_nodes?: boolean;
  rule_match_type: RuleMatchType;
  classes: PuppetClasses;
  variables: Record<string, unknown>;
  rules: ClassificationRule[];
  pinned_nodes: string[];
  /** Conflict priority (default 100; higher wins). Absent on servers without advanced classification. */
  priority?: number;
  /** Composite classification expression (canonical JSON), or null when only flat rules apply */
  expression?: ClassificationExpression | null;
  /** Inheritance, exclusivity, match mode, merge strategy and activation window */
  behavior?: GroupBehavior;
}

export interface CreateGroupRequest {
  name: string;
  description?: string;
  parent_id?: string;
  environment?: string;
  /** When true, this group assigns its environment to matching nodes instead of filtering by it */
  is_environment_group?: boolean;
  /** When true, groups with no rules will match all nodes (within parent context) */
  match_all_nodes?: boolean;
  rule_match_type?: RuleMatchType;
  classes?: PuppetClasses;
  variables?: Record<string, unknown>;
  priority?: number;
  expression?: ClassificationExpression;
  behavior?: GroupBehavior;
}

export interface UpdateGroupRequest {
  name?: string;
  description?: string;
  parent_id?: string | null;
  environment?: string | null;
  /** When true, this group assigns its environment to matching nodes instead of filtering by it */
  is_environment_group?: boolean;
  /** When true, groups with no rules will match all nodes (within parent context) */
  match_all_nodes?: boolean;
  rule_match_type?: RuleMatchType;
  classes?: PuppetClasses;
  variables?: Record<string, unknown>;
  priority?: number;
  /** Send an empty object to clear the expression */
  expression?: ClassificationExpression;
  behavior?: GroupBehavior;
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
  resources?: {
    total: number;
    changed: number;
    failed: number;
    skipped: number;
  };
  time?: {
    total: number;
    config_retrieval: number;
  };
  changes?: number;
  // PuppetDB raw format fields
  data?: Array<{ name: string; category?: string; value: unknown }>;
  href?: string;
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
  /** "plugin:<id>" for plugin facts; absent for PuppetDB facts. */
  source?: string;
  computed_at?: string;
  stale?: boolean;
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
  /** Combined variables from all matched groups, exported to the node as external facts */
  variables?: Record<string, unknown>;
  environment?: string | null;
  /** Present only when requested with `explain=true` */
  explain?: ClassificationExplain;
}

// Advanced classification (M26)

/** Composite expression in canonical JSON: all/any/not, compare leaves and fn leaves. */
export type ClassificationExpression = Record<string, unknown>;

export type InheritanceMode = 'inherit' | 'independent' | 'block' | 'select';

export interface GroupInheritance {
  mode: InheritanceMode;
  /** Only for the `select` mode */
  keys?: string[];
}

export type MatchModeKind = 'all' | 'any' | 'n_of';

export interface GroupMatchMode {
  mode: MatchModeKind;
  /** Only for the `n_of` mode */
  k?: number;
}

/** `deep`, `replace`, `union`, `fail_on_conflict` or `plugin:<plugin-id>:<strategy>` (M27). */
export type MergeStrategyName = string;

export type GroupKind = 'static' | 'dynamic' | 'generated';

/** The membership provider that feeds a dynamic group (M27). */
export interface DynamicGroupConfig {
  plugin_id: string;
  provider_id: string;
  params: Record<string, unknown>;
}

/** A group kept by a group template; read-only (M27). */
export interface GeneratedGroupInfo {
  instance_id: string;
  key: string;
}

export interface GroupBehavior {
  inheritance: GroupInheritance;
  exclusive_set?: string | null;
  match_mode?: GroupMatchMode | null;
  merge: MergeStrategyName;
  /** RFC3339 */
  active_from?: string | null;
  /** RFC3339 */
  expires_at?: string | null;
  /** Omitted for static groups (M27) */
  kind?: GroupKind;
  membership?: DynamicGroupConfig | null;
  owner_plugin_id?: string | null;
  generated?: GeneratedGroupInfo | null;
}

/** Where a catalog entry comes from (M27). */
export type CatalogSource = 'core' | 'plugin';

export interface ClassificationFunctionInfo {
  name: string;
  min_args: number;
  max_args: number;
  returns: 'bool' | 'value';
  description: string;
  example: string;
  source?: CatalogSource;
  plugin_id?: string;
}

export interface ClassificationOperatorInfo {
  op: string;
  description: string;
  source?: CatalogSource;
  plugin_id?: string;
}

export interface ClassificationMergeStrategyInfo {
  name: string;
  description: string;
  source?: CatalogSource;
  plugin_id?: string;
}

export type ClassificationValueType = 'string' | 'number' | 'bool' | 'list' | 'any';

export type OnStalePolicy = 'keep' | 'drop' | 'freeze';

export interface MembershipProviderDecl {
  id: string;
  label: string;
  description?: string | null;
  refresh?: { cron: string } | null;
  max_stale_s?: number | null;
  on_stale?: OnStalePolicy | null;
  churn_limit_pct?: number | null;
  max_members?: number | null;
  params_schema?: Record<string, unknown> | null;
  handler: string;
}

export interface MembershipProviderStatus {
  group_id: string;
  plugin_id: string;
  provider_id: string;
  last_refresh_at: string | null;
  /** never | ok | failed | held */
  last_status: string;
  last_error: string | null;
  members: number;
  stale: boolean;
  held: boolean;
}

export interface HeldMembership {
  group_id: string;
  plugin_id: string;
  provider_id: string;
  added: string[];
  removed: string[];
  current: number;
  proposed: number;
  churn_pct: number;
  limit_pct: number;
  created_at: string;
}

export interface GroupTemplateInstance {
  id: string;
  plugin_id: string;
  template_id: string;
  name: string;
  parent_id: string | null;
  classes: Record<string, unknown>;
  variables: Record<string, unknown>;
  approved: boolean;
  approved_by: string | null;
  approved_at: string | null;
  enabled: boolean;
  last_run_at: string | null;
  last_status: string | null;
  generated_groups: number;
  created_at: string;
}

export interface GroupTemplateDecl {
  id: string;
  label?: string | null;
  group_by: string;
  max_groups?: number | null;
  requires_approval?: boolean | null;
}

export interface PluginClassificationProvider extends MembershipProviderDecl {
  groups: MembershipProviderStatus[];
}

export interface PluginClassificationTemplate extends GroupTemplateDecl {
  instances: GroupTemplateInstance[];
}

export interface PluginClassificationView {
  plugin_id: string;
  version: string;
  functions: {
    id: string;
    description?: string | null;
    args: ClassificationValueType[];
    returns: ClassificationValueType;
    handler: string;
  }[];
  operators: {
    id: string;
    description?: string | null;
    arg_types: ClassificationValueType[];
    handler: string;
  }[];
  merge_strategies: { id: string; description?: string | null; handler: string }[];
  providers: PluginClassificationProvider[];
  templates: PluginClassificationTemplate[];
}

export interface PluginClassificationOverview {
  plugins: PluginClassificationView[];
  held: HeldMembership[];
  /** Group name -> why it needs a plugin that is not enabled */
  unavailable: Record<string, string[]>;
}

export interface GroupMemberEntry {
  certname: string;
  source: 'pinned' | 'dynamic' | 'rule';
  computed_at: string | null;
}

export interface MembershipOutcome {
  applied: boolean;
  held: boolean;
  added: number;
  removed: number;
  members: number;
  churn_pct: number;
}

export interface CreateTemplateInstanceRequest {
  name: string;
  parent_id?: string | null;
  classes?: Record<string, unknown>;
  variables?: Record<string, unknown>;
}

export interface TemplateRun {
  created: number;
  updated: number;
  removed: number;
  groups: number;
  truncated: boolean;
}

export interface ClassificationBehaviorInfo {
  key: string;
  description: string;
  values: string[];
}

export type ExplainOutcome =
  'matched' | 'not_matched' | 'environment_mismatch' | 'inactive' | 'excluded' | 'not_evaluated';

export interface ExplainRuleEvaluation {
  rule_id: string;
  fact_path: string;
  operator: string;
  expected: unknown;
  actual: unknown | null;
  matched: boolean;
}

export type ExprTraceKind = 'all' | 'any' | 'not' | 'compare' | 'fn';

export interface ExprTrace {
  kind: ExprTraceKind;
  label: string;
  matched: boolean;
  left?: unknown;
  right?: unknown;
  value?: unknown;
  error?: string;
  children: ExprTrace[];
}

export interface ExplainGroup {
  id: string;
  name: string;
  parent_id: string | null;
  priority: number;
  outcome: ExplainOutcome;
  match_type: MatchType | null;
  reason: string;
  rules: ExplainRuleEvaluation[];
  expression: ExprTrace | null;
}

export interface ProvenanceSource {
  group_id: string;
  group_name: string;
}

export interface ExplainProvenance {
  /** Contributing groups per class; the last entry is the winner */
  classes: Record<string, ProvenanceSource[]>;
  /** Contributing groups per variable; the last entry is the winner */
  variables: Record<string, ProvenanceSource[]>;
  environment: ProvenanceSource | null;
}

export interface ClassificationExplain {
  evaluated_at: string;
  elapsed_us: number;
  groups: ExplainGroup[];
  provenance: ExplainProvenance;
  merge_errors: string[];
  /** Plugin extensions that were unavailable and what that changed (M27) */
  degraded?: string[];
}

export interface GroupPreviewRequest {
  expression?: ClassificationExpression;
  rules?: CreateRuleRequest[];
  rule_match_type?: RuleMatchType;
  match_all_nodes?: boolean;
  environment?: string | null;
  priority?: number;
  behavior?: GroupBehavior;
  parent_id?: string | null;
}

export interface GroupPreviewClassificationChange {
  certname: string;
  groups_added: string[];
  groups_removed: string[];
  classes_changed: boolean;
  variables_changed: boolean;
  environment_changed: boolean;
}

export interface GroupPreviewResult {
  group_id: string;
  nodes_evaluated: number;
  nodes_skipped: number;
  members_before: number;
  members_after: number;
  entering_count: number;
  leaving_count: number;
  entering: string[];
  leaving: string[];
  truncated: boolean;
  classification_changed_count: number;
  classification_changed: GroupPreviewClassificationChange[];
}

// Auth types
export interface User {
  id: string;
  username: string;
  email: string;
  role: 'admin' | 'user' | 'viewer';
  force_password_change?: boolean;
  auth_provider?: AuthProvider;
  /** Directory that owns the identity fields (read-only in the UI). */
  directory_id?: string | null;
  active?: boolean;
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
  | 'api_keys'
  | 'plugins'
  | 'plugin_trust';

export type Action =
  'read' | 'create' | 'update' | 'delete' | 'admin' | 'export' | 'classify' | 'generate';

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
  /** Directory that owns username, e-mail and password (M30). */
  directory_id?: string | null;
  /** `false` once a directory sync deactivated the account. */
  active?: boolean;
  deactivated_at?: string | null;
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

export interface SmtpSettings {
  host: string;
  port: number;
  username?: string | null;
  password?: string | null;
  from_address: string;
  use_tls: boolean;
  configured: boolean;
}

export interface UpdateSmtpSettingsRequest {
  host: string;
  port: number;
  username?: string | null;
  password?: string | null;
  from_address: string;
  use_tls: boolean;
}

export interface UpdateJobSettings {
  max_runtime_minutes: number;
}

export interface UpdateUpdateJobSettingsRequest {
  max_runtime_minutes: number;
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
export type ReportType =
  'node_health' | 'compliance' | 'change_tracking' | 'drift_detection' | 'custom';
export type OutputFormat = 'json' | 'csv' | 'pdf';
export type ExecutionStatus = 'pending' | 'running' | 'completed' | 'failed';
export type SeverityLevel = 'low' | 'medium' | 'high' | 'critical';

export interface VariableAssignment {
  machine: string;
  variable: string;
  value: unknown;
  environment?: string | null;
  groups: string[];
}

export interface VariableAssignmentsQuery {
  machine?: string;
  variable?: string;
  value?: string;
  limit?: number;
  offset?: number;
}

export interface VariableAssignmentsResponse {
  assignments: VariableAssignment[];
  total: number;
}

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
  /** Core type id or a plugin type id (`plugin:<plugin-id>:<type-id>`). */
  report_type: ReportType | string;
  query_config: ReportQueryConfig;
  created_by: string;
  is_public: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateSavedReportRequest {
  name: string;
  description?: string;
  report_type: ReportType | string;
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
  report_type: ReportType | string;
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

export interface UpdateComplianceBaselineRequest {
  name?: string;
  description?: string | null;
  rules?: ComplianceRule[];
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

export interface UpdateDriftBaselineRequest {
  name?: string;
  description?: string | null;
  node_group_id?: string | null;
  baseline_facts?: Record<string, unknown>;
  tolerance_config?: DriftToleranceConfig | null;
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

// Custom report; with a plugin dataset as its data source it carries the rows.
export interface CustomReportResult {
  report_type: 'custom';
  data?: unknown;
  dataset?: string;
  columns?: DatasetColumn[];
  rows?: Record<string, unknown>[];
  total?: number;
}

export type ReportResult =
  | ({ report_type: 'node_health' } & NodeHealthReport)
  | ({ report_type: 'compliance' } & ComplianceReport)
  | ({ report_type: 'change_tracking' } & ChangeTrackingReport)
  | ({ report_type: 'drift_detection' } & DriftReport)
  | CustomReportResult
  | PluginReportResult;

export interface GenerateReportRequest {
  report_type: ReportType | string;
  config?: ReportQueryConfig;
  output_format?: OutputFormat;
}

// Plugin report types, export formats and datasets (M25)

export interface JsonSchemaProperty {
  type?: string;
  title?: string;
  description?: string;
  enum?: unknown[];
  format?: string;
  default?: unknown;
  minimum?: number;
  maximum?: number;
}

export interface JsonSchema {
  type?: string;
  properties?: Record<string, JsonSchemaProperty>;
  required?: string[];
}

export interface ReportTypeInfo {
  id: string;
  label: string;
  description: string | null;
  /** 'core' or 'plugin:<plugin-id>' */
  source: string;
  params_schema: JsonSchema | null;
  schedulable: boolean;
}

export interface PluginReportKpiItem {
  label: string;
  value: number | string;
  unit?: string | null;
}

export interface PluginReportColumn {
  key: string;
  label: string;
  type?: string | null;
}

export interface PluginReportSeries {
  name: string;
  points: { x: string | number; y: number }[];
}

export type PluginReportSection =
  | { kind: 'kpi'; id: string; title: string | null; items: PluginReportKpiItem[] }
  | {
      kind: 'table';
      id: string;
      title: string | null;
      columns: PluginReportColumn[];
      rows: Record<string, unknown>[];
    }
  | {
      kind: 'chart';
      id: string;
      title: string | null;
      chart: 'bar' | 'line' | 'pie';
      series: PluginReportSeries[];
    }
  | { kind: 'text'; id: string; title: string | null; text: string };

export interface PluginReportResult {
  report_type: 'plugin';
  /** plugin:<plugin-id>:<type-id> */
  type_id: string;
  plugin_id: string;
  title: string | null;
  generated_at: string;
  params: Record<string, unknown>;
  row_count: number;
  truncated: boolean;
  sections: PluginReportSection[];
}

export interface ExportFormatInfo {
  id: string;
  label: string;
  content_type: string;
  extension: string;
  /** 'core' or 'plugin:<plugin-id>' */
  source: string;
}

export interface DatasetColumn {
  name: string;
  type: string;
  label: string | null;
}

export interface DatasetInfo {
  /** plugin:<plugin-id>:<dataset-id> */
  id: string;
  plugin_id: string;
  dataset_id: string;
  label: string;
  description: string | null;
  columns: DatasetColumn[];
  rows: number;
  updated_at: string | null;
}

export interface DatasetQueryRequest {
  dataset: string;
  filter?: Record<string, string | number | boolean>;
  limit?: number;
  offset?: number;
}

export interface DatasetQueryResponse {
  dataset: string;
  columns: DatasetColumn[];
  rows: Record<string, unknown>[];
  total: number;
}

// Plugin annotations on Puppet run reports (M25)

export type AnnotationShowIn = 'list_column' | 'detail_panel';
export type AnnotationSeverity = 'info' | 'low' | 'medium' | 'high' | 'critical';

export interface ReportAnnotationColumn {
  plugin_id: string;
  key: string;
  label: string;
  type: string;
  show_in: AnnotationShowIn[];
  sortable: boolean;
  filterable: boolean;
}

export interface ReportAnnotation {
  plugin_id: string;
  key: string;
  label: string;
  type: string;
  value: unknown;
  severity: AnnotationSeverity | null;
  created_at: string;
  show_in: string[];
}

// ============================================================================
// Alerting Types
// ============================================================================

export type ChannelType = 'webhook' | 'email' | 'slack' | 'teams';
export type AlertRuleType =
  'node_status' | 'compliance' | 'drift' | 'report_failure' | 'update_job' | 'custom';
export type AlertSeverity = 'info' | 'warning' | 'critical';
export type ConditionOperator = 'all' | 'any';
export type AlertStatus = 'active' | 'acknowledged' | 'resolved' | 'silenced';
export type NotificationStatus = 'pending' | 'sent' | 'failed' | 'retrying';

// Alert Condition Types
export type AlertConditionType =
  | 'NodeStatus'
  | 'NodeFact'
  | 'ReportMetric'
  | 'EnvironmentFilter'
  | 'GroupFilter'
  | 'NodeCountThreshold'
  | 'TimeWindowFilter'
  | 'LastReportTime'
  | 'ConsecutiveFailures'
  | 'ConsecutiveChanges'
  | 'ClassChangeFrequency';

export type ConditionValueOperator =
  | '='
  | '!='
  | '~'
  | '!~'
  | '>'
  | '>='
  | '<'
  | '<='
  | 'in'
  | 'not_in'
  | 'exists'
  | 'not_exists'
  | 'contains'
  | 'not_contains';

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
  config: ChannelConfig | Record<string, unknown>; // Allow partial configs for creation
  is_enabled?: boolean;
}

export interface UpdateChannelRequest {
  name?: string;
  config?: ChannelConfig;
  is_enabled?: boolean;
}

// Enhanced AlertCondition supporting both simple and complex formats
export interface AlertCondition {
  // New complex format
  type?: AlertConditionType;
  enabled?: boolean;
  config?: AlertConditionConfig;

  // Legacy simple format (backwards compatibility)
  field?: string;
  operator: string;
  value?: unknown;
}

export interface AlertConditionConfig {
  operator?: ConditionValueOperator;
  value?: unknown;
  fact_path?: string;
  metric_name?: string;
  threshold?: number;
  threshold_hours?: number;
  time_window_hours?: number;
  class_name?: string;
  environment?: string;
  group_id?: string;
  min_count?: number;
  max_count?: number;
  start_hour?: number;
  end_hour?: number;
  days_of_week?: number[];
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

export type DeploymentStatus =
  'pending' | 'approved' | 'rejected' | 'deploying' | 'success' | 'failed' | 'cancelled';

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

// ============================================================================
// CVE / Vulnerability Types
// ============================================================================

export type CveFeedType = 'nvd_json' | 'cisa_kev' | 'custom';
export type CveSeverity = 'critical' | 'high' | 'medium' | 'low' | 'unknown';

export interface CveFeedSource {
  id: string;
  name: string;
  feed_url: string;
  feed_type: CveFeedType;
  enabled: boolean;
  last_sync_at?: string | null;
  last_sync_status: string;
  last_sync_error?: string | null;
  sync_interval_secs: number;
  created_at: string;
  updated_at: string;
}

export interface CreateCveFeedSourceRequest {
  name: string;
  feed_url: string;
  feed_type: CveFeedType;
  enabled?: boolean;
  sync_interval_secs?: number;
}

export interface UpdateCveFeedSourceRequest {
  name?: string;
  feed_url?: string;
  enabled?: boolean;
  sync_interval_secs?: number;
}

export interface CveEntry {
  id: string;
  feed_source_id: string;
  description?: string | null;
  severity: CveSeverity;
  cvss_score?: number | null;
  cvss_vector?: string | null;
  published_at?: string | null;
  modified_at?: string | null;
  references: string[];
  affected_products: string[];
  is_kev: boolean;
  created_at: string;
  updated_at: string;
}

export interface HostVulnerabilityMatch {
  id: string;
  certname: string;
  cve_id: string;
  package_name: string;
  installed_version: string;
  severity: CveSeverity;
  cvss_score?: number | null;
  is_kev: boolean;
  matched_at: string;
}

export interface NodeVulnerabilitySummary {
  certname: string;
  critical_count: number;
  high_count: number;
  medium_count: number;
  low_count: number;
  kev_count: number;
  total_count: number;
  last_checked_at?: string | null;
}

export interface SeverityDistributionPoint {
  severity: string;
  count: number;
}

export interface TopCveItem {
  cve_id: string;
  severity: CveSeverity;
  cvss_score?: number | null;
  affected_nodes: number;
  description?: string | null;
  is_kev: boolean;
}

export interface TopVulnerableNode {
  certname: string;
  total_vulns: number;
  critical_count: number;
  kev_count: number;
}

export interface VulnerabilityDashboardReport {
  total_vulnerable_nodes: number;
  total_cves_matched: number;
  severity_distribution: SeverityDistributionPoint[];
  top_cves: TopCveItem[];
  top_vulnerable_nodes: TopVulnerableNode[];
  kev_count: number;
  generated_at: string;
}

export interface CveDetailResponse {
  entry: CveEntry;
  affected_nodes: CveAffectedNode[];
  package_matches: CvePackageMatch[];
}

export interface CveAffectedNode {
  certname: string;
  package_name: string;
  installed_version: string;
  matched_at: string;
}

export interface CvePackageMatch {
  id: string;
  cve_id: string;
  package_name: string;
  version_start?: string | null;
  version_end?: string | null;
  platform_family?: string | null;
  created_at: string;
}

export interface FeedSyncResult {
  feed_id: string;
  entries_processed: number;
  entries_new: number;
  entries_updated: number;
  package_matches_created: number;
  errors: string[];
  synced_at: string;
}

export interface UpdatePreviewRequest {
  operation_type: UpdateOperationType;
  package_names?: string[];
  certnames?: string[];
  group_id?: string | null;
  target_all_outdated?: boolean;
}

export interface UpdatePreviewResponse {
  targets: UpdatePreviewTarget[];
  total_packages: number;
  total_nodes: number;
}

export interface UpdatePreviewTarget {
  certname: string;
  packages_to_update: UpdatePreviewPackage[];
}

export interface UpdatePreviewPackage {
  name: string;
  from_version: string;
  to_version: string;
  cve_ids: string[];
}

// ============================================================================
// Plugin platform (M19)
// ============================================================================

export type PluginState =
  'installed' | 'configured' | 'enabled' | 'disabled' | 'error' | 'update_pending';

export type PluginRuntimeKind = 'wasm' | 'remote' | 'builtin';
export type PluginRiskLevel = 'low' | 'medium' | 'high' | 'critical';

/** Which newer stored versions the update scheduler may activate (M24). */
export type PluginAutoUpdatePolicy = 'off' | 'patch' | 'minor' | 'any';

export interface PluginSummary {
  id: string;
  name: string;
  publisher: string;
  active_version: string | null;
  state: PluginState;
  state_reason: string | null;
  /** Newer version waiting for the capabilities it adds to be approved. */
  pending_version: string | null;
  runtime_kind: PluginRuntimeKind | null;
  signer_key_id: string | null;
  signer_label: string | null;
  installed_by: string | null;
  installed_at: string;
  updated_at: string;
  /** Newest stored version above the active one that may be activated. */
  available_version: string | null;
  auto_update: PluginAutoUpdatePolicy;
  /** Five-field UTC cron expression; null means at any time. */
  update_window: string | null;
  /** Version the update scheduler skips (failed automatic update or rollback). */
  update_hold_version: string | null;
  update_hold_reason: string | null;
}

export interface PluginCapabilityInfo {
  name: string;
  scope: unknown;
  risk: PluginRiskLevel;
  description: string;
}

export interface PluginVersionRecord {
  plugin_id: string;
  version: string;
  api_version: number;
  package_sha256: string;
  signed_by_key_id: string;
  signed_at: string;
  revoked: boolean;
  revoked_reason: string | null;
  revoked_by: string | null;
  revoked_at: string | null;
  added_at: string;
}

export interface PluginDetail extends PluginSummary {
  manifest: Record<string, unknown> | null;
  capabilities: PluginCapabilityInfo[];
  versions: PluginVersionRecord[];
}

export type PluginSignatureStatus =
  'valid' | 'untrusted_key' | 'invalid' | 'key_revoked' | 'key_expired' | 'key_scope';

export interface PluginSignatureReport {
  status: PluginSignatureStatus;
  key_id: string;
  key_label: string | null;
  key_origin: 'builtin' | 'admin' | null;
  signed_at: string;
  message: string | null;
}

export interface PluginManifestSummary {
  id: string;
  name: string;
  version: string;
  publisher: string;
  api_version: number;
  license: string | null;
  runtime_kind: PluginRuntimeKind;
  min_webui_version: string | null;
}

export interface PluginPreflightResult {
  manifest: PluginManifestSummary;
  package_sha256: string;
  signature: PluginSignatureReport;
  capabilities: PluginCapabilityInfo[];
  declared_pii_fields: string[];
  installed_version: string | null;
  issues: string[];
  can_install: boolean;
  /** Capabilities this version adds over the installed one (update waits for approval). */
  added_capabilities: string[];
  /** Set when going back from this version to the installed one would be blocked. */
  rollback_warning: string | null;
}

export interface PluginInstallOptions {
  /** First 8 hex characters of the signer fingerprint (the key id). */
  trustKeyConfirm?: string;
  trustKeyLabel?: string;
  /** Current password, required when trusting a key during install. */
  reauthPassword?: string;
  /** Store a newer version of an installed plugin without activating it. */
  stage?: boolean;
}

export interface TrustedKeyView {
  key_id: string;
  fingerprint: string;
  label: string;
  origin: 'builtin' | 'admin';
  allowed_plugin_ids: string[] | null;
  expires_at: string | null;
  state: 'active' | 'revoked' | 'expired';
  expires_soon: boolean;
  added_by: string | null;
  added_at: string;
  revoked_at: string | null;
  revoked_reason: string | null;
  dependent_plugins: string[];
}

export interface AddTrustedKeyRequest {
  public_key: string;
  label: string;
  expires_at?: string;
  allowed_plugin_ids?: string[];
  password: string;
}

export interface UpdateTrustedKeyRequest {
  label?: string;
  /** null clears the expiry */
  expires_at?: string | null;
  /** null clears the scope */
  allowed_plugin_ids?: string[] | null;
}

export interface RevokeTrustedKeyRequest {
  reason: string;
  password: string;
}

export type PluginRenderKind = 'declarative' | 'iframe' | 'module';

export interface UiContribution {
  plugin_id: string;
  plugin_name: string;
  slot: string;
  id: string;
  render: PluginRenderKind;
  title: string | null;
  path: string | null;
  order: number;
  label: string | null;
  icon: string | null;
  href: string | null;
}

export interface UiMenuOp {
  plugin_id: string;
  target: string;
  op: 'hide' | 'rename';
  label: string | null;
}

export interface UiSlots {
  slots: Record<string, UiContribution[]>;
  menu_ops: UiMenuOp[];
}

export interface UiContributionDetail extends UiContribution {
  spec: unknown | null;
}

// Plugin UI sessions and slot catalog (M23)
export type PluginSlotKind = 'menu' | 'page' | 'panel' | 'action' | 'column';

export interface PluginSlotSpec {
  name: string;
  kind: PluginSlotKind;
  context: string[];
  max_items: number;
  mounted: boolean;
  default_height: number;
}

export interface PluginSlotCatalog {
  version: number;
  slots: PluginSlotSpec[];
}

export interface PluginUiSession {
  url: string;
  expires_in_secs: number;
  api_scopes: string[];
}

// Plugin governance (M22)
export type PluginCircuitState = 'closed' | 'half_open' | 'open';
export type PluginHealth = 'healthy' | 'degraded' | 'circuit_open' | 'disabled' | 'error';

export interface PluginCircuitView {
  state: PluginCircuitState;
  trips: number;
  retry_at: string | null;
  last_error: string | null;
}

export interface PluginUsageTotals {
  calls: number;
  errors: number;
  traps: number;
  timeouts: number;
  quota_rejections: number;
  circuit_rejections: number;
  error_rate: number;
  avg_ms: number | null;
  p50_ms: number | null;
  p95_ms: number | null;
  p99_ms: number | null;
  max_ms: number;
}

export interface PluginGovernanceEntry {
  id: string;
  name: string;
  version: string | null;
  state: string;
  state_reason: string | null;
  health: PluginHealth;
  circuit: PluginCircuitView;
  invocations_per_minute_limit: number | null;
  last_hour: PluginUsageTotals;
  last_24h: PluginUsageTotals;
  revoked_versions: string[];
}

export interface PluginGovernanceOverview {
  plugins: PluginGovernanceEntry[];
  top_consumers: string[];
  open_circuits: number;
  generated_at: string;
}

export interface PluginUsagePoint {
  hour: string;
  calls: number;
  errors: number;
  traps: number;
  timeouts: number;
  quota_rejections: number;
  circuit_rejections: number;
  p95_ms: number | null;
}

export interface PluginMetrics {
  plugin_id: string;
  hours: number;
  circuit: PluginCircuitView;
  totals: PluginUsageTotals;
  by_version: { version: string; totals: PluginUsageTotals }[];
  series: PluginUsagePoint[];
}

// ============================================================================
// Plugin updates, rollback and revocation (M24)
// ============================================================================

export type PluginVersionRelation = 'active' | 'newer' | 'older';

export interface PluginVersionView extends PluginVersionRecord {
  relation: PluginVersionRelation;
  pending: boolean;
  /** The package hash is listed in plugins.blocked_hashes. */
  blocked: boolean;
  schema_level: number;
  added_capabilities: string[];
  /** Newer: can be applied now. Older: a rollback to it is possible now. */
  can_activate: boolean;
  blocked_reason: string | null;
  /** Newer only: after this update a rollback to the active version would be blocked. */
  rollback_warning: string | null;
  usage: PluginUsageTotals;
}

export interface PluginUpdateInfo {
  plugin_id: string;
  name: string;
  state: PluginState;
  active_version: string | null;
  available_version: string | null;
  pending_version: string | null;
  added_capabilities: string[];
  requires_approval: boolean;
  rollback_warning: string | null;
  auto_update: PluginAutoUpdatePolicy;
  update_window: string | null;
  next_window_at: string | null;
  update_hold_version: string | null;
  update_hold_reason: string | null;
}

export type PluginUpdateOutcome = 'updated' | 'pending_approval' | 'up_to_date' | 'failed';

export interface PluginUpdateResult {
  plugin_id: string;
  outcome: PluginUpdateOutcome;
  from_version: string | null;
  to_version: string | null;
  added_capabilities: string[];
  code: string | null;
  message: string | null;
}

export interface PluginRollbackResult {
  plugin_id: string;
  from_version: string;
  to_version: string;
  schema_from: number;
  schema_to: number;
  plugin: PluginDetail;
}

export interface PluginRevokeVersionResult {
  plugin_id: string;
  version: string;
  deactivated: boolean;
  plugin: PluginDetail;
}

export interface PluginUpdatePolicyRequest {
  auto_update: PluginAutoUpdatePolicy;
  update_window: string | null;
}

// ============================================================================
// Plugin facts (M25)
// ============================================================================

export type PluginFactType = 'string' | 'integer' | 'float' | 'bool' | 'list' | 'object';
export type PluginFactSensitivity = 'none' | 'pii';
export type PluginFactExportCapability =
  'approved' | 'not_approved' | 'not_declared' | 'denied_by_policy';

export interface PluginFactInfo {
  /** Full name, for example "acme_cmdb.owner". */
  name: string;
  local_name: string;
  type: PluginFactType;
  description: string | null;
  enum: unknown[] | null;
  ttl_s: number;
  max_stale_s: number;
  sensitivity: PluginFactSensitivity;
  /** The manifest offers usable_in_rules (an administrator may enable it). */
  offers_rules: boolean;
  /** The manifest offers exportable (an administrator may enable it). */
  offers_export: boolean;
  usable_in_rules: boolean;
  exportable: boolean;
  approved_by: string | null;
  approved_at: string | null;
  requires: { resource: string; action: string } | null;
  /** Nodes with a stored value. */
  nodes: number;
  /** Of those, nodes whose value is older than max_stale_s. */
  stale_nodes: number;
  /** Collides with a fact reported by agents: PuppetDB wins, the plugin value is hidden. */
  conflict: boolean;
}

export interface PluginFactProviderStatus {
  last_success_at: string | null;
  last_failure_at: string | null;
  last_error: string | null;
  failures: number;
  rejected: number;
}

export interface PluginFactCatalog {
  plugin_id: string;
  plugin_name: string;
  state: PluginState;
  active_version: string | null;
  namespace: string;
  mode: 'push' | 'pull';
  handler: string | null;
  ttl_s: number;
  max_stale_s: number;
  export_capability: PluginFactExportCapability;
  facts: PluginFactInfo[];
  provider: PluginFactProviderStatus;
  /** False when conflicts could not be checked (PuppetDB unavailable). */
  puppetdb_checked: boolean;
}

export interface PluginFactSettingsRequest {
  usable_in_rules?: boolean;
  exportable?: boolean;
  /** Full fact name; required when enabling exportable. */
  confirm?: string;
}

export interface PluginFactPreviewRow {
  certname: string;
  value: unknown;
  computed_at: string;
  stale: boolean;
  bytes: number;
}

export interface PluginFactPreview {
  plugin_id: string;
  name: string;
  sensitivity: PluginFactSensitivity;
  exportable: boolean;
  total: number;
  limit: number;
  rows: PluginFactPreviewRow[];
}

/** One entry of GET /facts/paths; `source` is set on plugin entries only. */
export interface FactPathEntry {
  path: string[];
  type: string;
  source?: string;
}

// Directories (M30)

export interface DirectorySyncRun {
  id: string;
  directory_id: string;
  trigger_kind: 'schedule' | 'manual';
  status: 'running' | 'ok' | 'partial' | 'failed';
  started_at: string;
  finished_at?: string | null;
  created: number;
  updated: number;
  deactivated: number;
  reactivated: number;
  errors: number;
  error?: string | null;
  started_by?: string | null;
}

export interface DirectoryInfo {
  id: string;
  label: string;
  kind: 'ldap' | 'active_directory';
  enabled: boolean;
  servers: string[];
  start_tls: boolean;
  ca_file?: string | null;
  bind_dn: string;
  base_dn: string;
  user_filter: string;
  domains: string[];
  attributes: { username: string; email: string; display_name: string; id: string[] };
  groups: { base_dn: string; filter: string; nested: boolean };
  modes: { lookup: boolean; jit: boolean; sync: { enabled: boolean; interval_secs: number } };
  deny_without_role: boolean;
  sync_running: boolean;
  last_sync?: DirectorySyncRun | null;
  mappings?: number;
}

export interface DirectoryGroupEntry {
  dn: string;
  name: string;
  guid?: string | null;
  sid?: string | null;
}

export interface DirectoryRoleMapping {
  id: string;
  directory_id: string;
  group_ref: string;
  group_ref_kind: 'dn' | 'sid' | 'guid';
  group_label?: string | null;
  role_id: string;
  role_name: string;
  created_by?: string | null;
  created_at: string;
}

export interface DirectoryTestResult {
  ok: boolean;
  stage?: string;
  server?: string | null;
  error?: string;
  message?: string;
  users_found?: boolean;
  groups_found?: boolean;
  elapsed_ms?: number;
}
