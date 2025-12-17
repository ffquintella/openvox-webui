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

// Group types
export type RuleMatchType = 'all' | 'any';

export type RuleOperator = '=' | '!=' | '~' | '!~' | '>' | '>=' | '<' | '<=' | 'in' | 'not_in';

export interface ClassificationRule {
  id: string;
  fact_path: string;
  operator: RuleOperator;
  value: unknown;
}

export interface NodeGroup {
  id: string;
  name: string;
  description?: string | null;
  parent_id?: string | null;
  environment?: string | null;
  rule_match_type: RuleMatchType;
  classes: string[];
  parameters: Record<string, unknown>;
  rules: ClassificationRule[];
  pinned_nodes: string[];
}

export interface CreateGroupRequest {
  name: string;
  description?: string;
  parent_id?: string;
  environment?: string;
  rule_match_type?: RuleMatchType;
  classes?: string[];
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
}

export interface AuthResponse {
  token: string;
  user: User;
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
  roles?: Array<{ id: string; name: string; display_name: string }>;
  created_at: string;
}

export interface CreateUserRequest {
  username: string;
  email: string;
  password: string;
  role_ids?: string[];
}

export interface UpdateUserRequest {
  username?: string;
  email?: string;
  password?: string;
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
