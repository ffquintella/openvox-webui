import axios from 'axios';
import type {
  Node,
  NodeGroup,
  Report,
  ResourceEvent,
  CreateGroupRequest,
  UpdateGroupRequest,
  CreateRuleRequest,
  ClassificationRule,
  Role,
  Permission,
  CreateRoleRequest,
  UserResponse,
  CreateUserRequest,
  UpdateUserRequest,
  EffectivePermissions,
  ResourceInfo,
  ActionInfo,
  PermissionMatrix,
  BulkPermissionRequest,
  BulkPermissionResult,
  CreatePermissionRequest,
  FactTemplate,
  CreateFactTemplateRequest,
  UpdateFactTemplateRequest,
  GenerateFactsRequest,
  GeneratedFacts,
  ExportFormat,
  SettingsResponse,
  DashboardConfig,
  RbacConfigResponse,
  ExportConfigResponse,
  ImportConfigResponse,
  ValidateConfigResponse,
  ConfigHistoryEntry,
  ServerInfoResponse,
  CAStatus,
  CertificateRequest,
  Certificate,
  SignRequest,
  SignResponse,
  RejectResponse,
  RevokeResponse,
  RenewCARequest,
  RenewCAResponse,
  SavedReport,
  CreateSavedReportRequest,
  UpdateSavedReportRequest,
  ReportSchedule,
  CreateScheduleRequest,
  UpdateScheduleRequest,
  ReportExecution,
  ExecuteReportRequest,
  ReportTemplate,
  ComplianceBaseline,
  CreateComplianceBaselineRequest,
  DriftBaseline,
  CreateDriftBaselineRequest,
  GenerateReportRequest,
  ReportType,
  ReportQueryConfig,
  // Alerting types
  NotificationChannel,
  CreateChannelRequest,
  UpdateChannelRequest,
  AlertRule,
  CreateAlertRuleRequest,
  UpdateAlertRuleRequest,
  Alert,
  AlertSilence,
  CreateSilenceRequest,
  AlertStats,
  TestChannelRequest,
  TestChannelResponse,
  TriggerAlertRequest,
  AlertRuleType,
  AlertSeverity,
  AlertStatus,
  // Code Deploy types
  CodeSshKey,
  CreateSshKeyRequest,
  CodeRepository,
  CreateRepositoryRequest,
  UpdateRepositoryRequest,
  CodeEnvironment,
  UpdateEnvironmentRequest,
  CodeDeployment,
  TriggerDeploymentRequest,
  ApproveDeploymentRequest,
  RejectDeploymentRequest,
  ListDeploymentsQuery,
  ListEnvironmentsQuery,
} from '../types';

const client = axios.create({
  baseURL: '/api/v1',
  headers: {
    'Content-Type': 'application/json',
  },
});

// Add auth interceptor
client.interceptors.request.use((config) => {
  const token = localStorage.getItem('auth_token');
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

// Auth response types
interface LoginResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
  user: {
    id: string;
    username: string;
    email: string;
    role: string;
    force_password_change?: boolean;
  };
}

interface RefreshResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
}

export const api = {
  // Auth
  login: async (username: string, password: string): Promise<LoginResponse> => {
    const response = await client.post('/auth/login', { username, password });
    return response.data;
  },

  logout: async (): Promise<void> => {
    await client.post('/auth/logout');
  },

  refreshToken: async (refreshToken: string): Promise<RefreshResponse> => {
    const response = await client.post('/auth/refresh', { refresh_token: refreshToken });
    return response.data;
  },

  changePassword: async (currentPassword: string, newPassword: string): Promise<{ message: string }> => {
    const response = await client.post('/auth/change-password', {
      current_password: currentPassword,
      new_password: newPassword,
    });
    return response.data;
  },

  getCurrentUser: async (): Promise<UserResponse> => {
    const response = await client.get('/auth/me');
    return response.data;
  },

  // Health
  health: async () => {
    const response = await client.get('/health');
    return response.data;
  },

  // Nodes
  getNodes: async (): Promise<Node[]> => {
    const response = await client.get('/nodes');
    return response.data;
  },

  getNode: async (certname: string): Promise<Node | null> => {
    const response = await client.get(`/nodes/${certname}`);
    return response.data;
  },

  getNodeFacts: async (certname: string): Promise<Record<string, unknown>> => {
    const response = await client.get(`/nodes/${certname}/facts`);
    return response.data;
  },

  getNodeReports: async (certname: string): Promise<Report[]> => {
    const response = await client.get(`/nodes/${certname}/reports`);
    return response.data;
  },

  // Groups
  getGroups: async (): Promise<NodeGroup[]> => {
    const response = await client.get('/groups');
    return response.data;
  },

  getGroup: async (id: string): Promise<NodeGroup | null> => {
    const response = await client.get(`/groups/${id}`);
    return response.data;
  },

  createGroup: async (data: CreateGroupRequest): Promise<NodeGroup> => {
    const response = await client.post('/groups', data);
    return response.data;
  },

  updateGroup: async (id: string, data: UpdateGroupRequest): Promise<NodeGroup> => {
    const response = await client.put(`/groups/${id}`, data);
    return response.data;
  },

  deleteGroup: async (id: string): Promise<boolean> => {
    const response = await client.delete(`/groups/${id}`);
    return response.data;
  },

  getGroupNodes: async (id: string): Promise<string[]> => {
    const response = await client.get(`/groups/${id}/nodes`);
    return response.data;
  },

  // Group Rules
  getGroupRules: async (id: string): Promise<ClassificationRule[]> => {
    const response = await client.get(`/groups/${id}/rules`);
    return response.data;
  },

  addGroupRule: async (id: string, rule: CreateRuleRequest): Promise<ClassificationRule> => {
    const response = await client.post(`/groups/${id}/rules`, rule);
    return response.data;
  },

  deleteGroupRule: async (groupId: string, ruleId: string): Promise<void> => {
    await client.delete(`/groups/${groupId}/rules/${ruleId}`);
  },

  // Pinned Nodes
  addPinnedNode: async (groupId: string, certname: string): Promise<void> => {
    await client.post(`/groups/${groupId}/pinned`, { certname });
  },

  removePinnedNode: async (groupId: string, certname: string): Promise<void> => {
    await client.delete(`/groups/${groupId}/pinned/${encodeURIComponent(certname)}`);
  },

  // Facts
  getFacts: async (params?: { name?: string; certname?: string }): Promise<Array<{ certname: string; name: string; value: unknown }>> => {
    const response = await client.get('/facts', { params });
    // Backend returns { facts: [...], total: N } wrapper, extract the facts array
    return response.data.facts || response.data;
  },

  getFactNames: async (): Promise<string[]> => {
    const response = await client.get('/facts/names');
    return response.data;
  },

  // Reports
  getReports: async (params?: { certname?: string; status?: string; limit?: number }): Promise<Report[]> => {
    const response = await client.get('/reports', { params });
    return response.data;
  },

  getReport: async (hash: string): Promise<Report | null> => {
    const response = await client.get(`/reports/${hash}`);
    return response.data;
  },

  getReportEvents: async (hash: string, params?: { status?: string; type?: string }): Promise<ResourceEvent[]> => {
    const response = await client.get(`/reports/${hash}/events`, { params });
    return response.data;
  },

  // Query
  executeQuery: async (query: string): Promise<unknown[]> => {
    const response = await client.post('/query', { query });
    return response.data;
  },

  // Roles
  getRoles: async (): Promise<Role[]> => {
    const response = await client.get('/roles');
    return response.data;
  },

  getRole: async (id: string): Promise<Role | null> => {
    const response = await client.get(`/roles/${id}`);
    return response.data;
  },

  createRole: async (data: CreateRoleRequest): Promise<Role> => {
    const response = await client.post('/roles', data);
    return response.data;
  },

  updateRole: async (id: string, data: CreateRoleRequest): Promise<Role> => {
    const response = await client.put(`/roles/${id}`, data);
    return response.data;
  },

  deleteRole: async (id: string): Promise<boolean> => {
    const response = await client.delete(`/roles/${id}`);
    return response.data;
  },

  getRolePermissions: async (id: string): Promise<Permission[]> => {
    const response = await client.get(`/roles/${id}/permissions`);
    return response.data;
  },

  updateRolePermissions: async (id: string, permissions: Permission[]): Promise<Permission[]> => {
    const response = await client.put(`/roles/${id}/permissions`, permissions);
    return response.data;
  },

  // Users
  getUsers: async (): Promise<UserResponse[]> => {
    const response = await client.get('/users');
    return response.data;
  },

  getUser: async (id: string): Promise<UserResponse | null> => {
    const response = await client.get(`/users/${id}`);
    return response.data;
  },

  createUser: async (data: CreateUserRequest): Promise<UserResponse> => {
    const response = await client.post('/users', data);
    return response.data;
  },

  updateUser: async (id: string, data: UpdateUserRequest): Promise<UserResponse> => {
    const response = await client.put(`/users/${id}`, data);
    return response.data;
  },

  deleteUser: async (id: string): Promise<boolean> => {
    const response = await client.delete(`/users/${id}`);
    return response.data;
  },

  getUserRoles: async (id: string): Promise<Role[]> => {
    const response = await client.get(`/users/${id}/roles`);
    return response.data;
  },

  assignUserRoles: async (id: string, roleIds: string[]): Promise<Role[]> => {
    const response = await client.put(`/users/${id}/roles`, { role_ids: roleIds });
    return response.data;
  },

  getUserPermissions: async (id: string): Promise<EffectivePermissions> => {
    const response = await client.get(`/users/${id}/permissions`);
    return response.data;
  },

  // Permissions
  getPermissions: async (): Promise<Permission[]> => {
    const response = await client.get('/permissions');
    return response.data;
  },

  getResources: async (): Promise<ResourceInfo[]> => {
    const response = await client.get('/permissions/resources');
    return response.data;
  },

  getActions: async (): Promise<ActionInfo[]> => {
    const response = await client.get('/permissions/actions');
    return response.data;
  },

  getPermissionMatrix: async (): Promise<PermissionMatrix> => {
    const response = await client.get('/permissions/matrix');
    return response.data;
  },

  bulkUpdatePermissions: async (request: BulkPermissionRequest): Promise<BulkPermissionResult> => {
    const response = await client.post('/permissions/bulk', request);
    return response.data;
  },

  addPermissionToRole: async (roleId: string, permission: CreatePermissionRequest): Promise<Role> => {
    const response = await client.post(`/roles/${roleId}/permissions`, permission);
    return response.data;
  },

  removePermissionFromRole: async (roleId: string, permissionId: string): Promise<void> => {
    await client.delete(`/roles/${roleId}/permissions/${permissionId}`);
  },

  // Facter Templates
  getFactTemplates: async (): Promise<FactTemplate[]> => {
    const response = await client.get('/facter/templates');
    return response.data;
  },

  getFactTemplate: async (id: string): Promise<FactTemplate> => {
    const response = await client.get(`/facter/templates/${id}`);
    return response.data;
  },

  createFactTemplate: async (data: CreateFactTemplateRequest): Promise<FactTemplate> => {
    const response = await client.post('/facter/templates', data);
    return response.data;
  },

  updateFactTemplate: async (id: string, data: UpdateFactTemplateRequest): Promise<FactTemplate> => {
    const response = await client.put(`/facter/templates/${id}`, data);
    return response.data;
  },

  deleteFactTemplate: async (id: string): Promise<void> => {
    await client.delete(`/facter/templates/${id}`);
  },

  generateFacts: async (data: GenerateFactsRequest): Promise<GeneratedFacts> => {
    const response = await client.post('/facter/generate', data);
    return response.data;
  },

  exportFacts: async (certname: string, template: string, format: ExportFormat = 'json'): Promise<string> => {
    const response = await client.get(`/facter/export/${encodeURIComponent(certname)}`, {
      params: { template, format },
    });
    return response.data;
  },

  // Settings
  getSettings: async (): Promise<SettingsResponse> => {
    const response = await client.get('/settings');
    return response.data;
  },

  getDashboardConfig: async (): Promise<DashboardConfig> => {
    const response = await client.get('/settings/dashboard');
    return response.data;
  },

  updateDashboardConfig: async (config: Partial<DashboardConfig>): Promise<DashboardConfig> => {
    const response = await client.put('/settings/dashboard', config);
    return response.data;
  },

  getRbacConfig: async (): Promise<RbacConfigResponse> => {
    const response = await client.get('/settings/rbac');
    return response.data;
  },

  exportConfig: async (): Promise<ExportConfigResponse> => {
    const response = await client.get('/settings/export');
    return response.data;
  },

  importConfig: async (content: string, dryRun: boolean = false): Promise<ImportConfigResponse> => {
    const response = await client.post('/settings/import', { content, format: 'yaml', dry_run: dryRun });
    return response.data;
  },

  validateConfig: async (content: string): Promise<ValidateConfigResponse> => {
    const response = await client.post('/settings/validate', { content, format: 'yaml' });
    return response.data;
  },

  getConfigHistory: async (): Promise<ConfigHistoryEntry[]> => {
    const response = await client.get('/settings/history');
    return response.data;
  },

  getServerInfo: async (): Promise<ServerInfoResponse> => {
    const response = await client.get('/settings/server');
    return response.data;
  },

  // CA (Certificate Authority)
  getCAStatus: async (): Promise<CAStatus> => {
    const response = await client.get('/ca/status');
    return response.data;
  },

  getCertificateRequests: async (): Promise<CertificateRequest[]> => {
    const response = await client.get('/ca/requests');
    return response.data;
  },

  getCertificates: async (): Promise<Certificate[]> => {
    const response = await client.get('/ca/certificates');
    return response.data;
  },

  getCertificate: async (certname: string): Promise<Certificate> => {
    const response = await client.get(`/ca/certificates/${encodeURIComponent(certname)}`);
    return response.data;
  },

  signCertificate: async (certname: string, request?: SignRequest): Promise<SignResponse> => {
    const response = await client.post(`/ca/sign/${encodeURIComponent(certname)}`, request || {});
    return response.data;
  },

  rejectCertificate: async (certname: string): Promise<RejectResponse> => {
    const response = await client.post(`/ca/reject/${encodeURIComponent(certname)}`);
    return response.data;
  },

  revokeCertificate: async (certname: string): Promise<RevokeResponse> => {
    const response = await client.delete(`/ca/certificates/${encodeURIComponent(certname)}`);
    return response.data;
  },

  renewCA: async (request: RenewCARequest): Promise<RenewCAResponse> => {
    const response = await client.post('/ca/renew', request);
    return response.data;
  },

  // Analytics & Reporting
  getSavedReports: async (reportType?: ReportType): Promise<SavedReport[]> => {
    const params = reportType ? { report_type: reportType } : {};
    const response = await client.get('/analytics/saved-reports', { params });
    return response.data;
  },

  getSavedReport: async (id: string): Promise<SavedReport> => {
    const response = await client.get(`/analytics/saved-reports/${id}`);
    return response.data;
  },

  createSavedReport: async (request: CreateSavedReportRequest): Promise<SavedReport> => {
    const response = await client.post('/analytics/saved-reports', request);
    return response.data;
  },

  updateSavedReport: async (id: string, request: UpdateSavedReportRequest): Promise<SavedReport> => {
    const response = await client.put(`/analytics/saved-reports/${id}`, request);
    return response.data;
  },

  deleteSavedReport: async (id: string): Promise<void> => {
    await client.delete(`/analytics/saved-reports/${id}`);
  },

  executeReport: async (id: string, request?: ExecuteReportRequest): Promise<ReportExecution> => {
    const response = await client.post(`/analytics/saved-reports/${id}/execute`, request || {});
    return response.data;
  },

  getReportExecutions: async (id: string, limit?: number): Promise<ReportExecution[]> => {
    const params = limit ? { limit } : {};
    const response = await client.get(`/analytics/saved-reports/${id}/executions`, { params });
    return response.data;
  },

  getReportTemplates: async (reportType?: ReportType): Promise<ReportTemplate[]> => {
    const params = reportType ? { report_type: reportType } : {};
    const response = await client.get('/analytics/templates', { params });
    return response.data;
  },

  getReportTemplate: async (id: string): Promise<ReportTemplate> => {
    const response = await client.get(`/analytics/templates/${id}`);
    return response.data;
  },

  getSchedules: async (): Promise<ReportSchedule[]> => {
    const response = await client.get('/analytics/schedules');
    return response.data;
  },

  getSchedule: async (id: string): Promise<ReportSchedule> => {
    const response = await client.get(`/analytics/schedules/${id}`);
    return response.data;
  },

  createSchedule: async (request: CreateScheduleRequest): Promise<ReportSchedule> => {
    const response = await client.post('/analytics/schedules', request);
    return response.data;
  },

  updateSchedule: async (id: string, request: UpdateScheduleRequest): Promise<ReportSchedule> => {
    const response = await client.put(`/analytics/schedules/${id}`, request);
    return response.data;
  },

  deleteSchedule: async (id: string): Promise<void> => {
    await client.delete(`/analytics/schedules/${id}`);
  },

  generateReport: async (request: GenerateReportRequest): Promise<unknown> => {
    const response = await client.post('/analytics/generate', request);
    return response.data;
  },

  generateReportByType: async (reportType: ReportType, config?: ReportQueryConfig): Promise<unknown> => {
    const response = await client.post(`/analytics/generate/${reportType}`, config || {});
    return response.data;
  },

  getComplianceBaselines: async (): Promise<ComplianceBaseline[]> => {
    const response = await client.get('/analytics/compliance-baselines');
    return response.data;
  },

  getComplianceBaseline: async (id: string): Promise<ComplianceBaseline> => {
    const response = await client.get(`/analytics/compliance-baselines/${id}`);
    return response.data;
  },

  createComplianceBaseline: async (request: CreateComplianceBaselineRequest): Promise<ComplianceBaseline> => {
    const response = await client.post('/analytics/compliance-baselines', request);
    return response.data;
  },

  deleteComplianceBaseline: async (id: string): Promise<void> => {
    await client.delete(`/analytics/compliance-baselines/${id}`);
  },

  getDriftBaselines: async (): Promise<DriftBaseline[]> => {
    const response = await client.get('/analytics/drift-baselines');
    return response.data;
  },

  getDriftBaseline: async (id: string): Promise<DriftBaseline> => {
    const response = await client.get(`/analytics/drift-baselines/${id}`);
    return response.data;
  },

  createDriftBaseline: async (request: CreateDriftBaselineRequest): Promise<DriftBaseline> => {
    const response = await client.post('/analytics/drift-baselines', request);
    return response.data;
  },

  deleteDriftBaseline: async (id: string): Promise<void> => {
    await client.delete(`/analytics/drift-baselines/${id}`);
  },

  exportExecution: async (id: string, format?: string): Promise<Blob> => {
    const params = format ? { format } : {};
    const response = await client.get(`/analytics/executions/${id}/export`, {
      params,
      responseType: 'blob',
    });
    return response.data;
  },

  // ============================================================================
  // Alerting
  // ============================================================================

  // Notification Channels
  getChannels: async (): Promise<NotificationChannel[]> => {
    const response = await client.get('/alerting/channels');
    return response.data.data;
  },

  getChannel: async (id: string): Promise<NotificationChannel> => {
    const response = await client.get(`/alerting/channels/${id}`);
    return response.data.data;
  },

  createChannel: async (request: CreateChannelRequest): Promise<NotificationChannel> => {
    const response = await client.post('/alerting/channels', request);
    return response.data.data;
  },

  updateChannel: async (id: string, request: UpdateChannelRequest): Promise<NotificationChannel> => {
    const response = await client.put(`/alerting/channels/${id}`, request);
    return response.data.data;
  },

  deleteChannel: async (id: string): Promise<void> => {
    await client.delete(`/alerting/channels/${id}`);
  },

  testChannel: async (id: string, request?: TestChannelRequest): Promise<TestChannelResponse> => {
    const response = await client.post(`/alerting/channels/${id}/test`, request || {});
    return response.data.data;
  },

  // Alert Rules
  getRules: async (ruleType?: AlertRuleType, enabled?: boolean): Promise<AlertRule[]> => {
    const params: Record<string, string | boolean> = {};
    if (ruleType) params.rule_type = ruleType;
    if (enabled !== undefined) params.enabled = enabled;
    const response = await client.get('/alerting/rules', { params });
    return response.data.data;
  },

  getRule: async (id: string): Promise<AlertRule> => {
    const response = await client.get(`/alerting/rules/${id}`);
    return response.data.data;
  },

  createRule: async (request: CreateAlertRuleRequest): Promise<AlertRule> => {
    const response = await client.post('/alerting/rules', request);
    return response.data.data;
  },

  updateRule: async (id: string, request: UpdateAlertRuleRequest): Promise<AlertRule> => {
    const response = await client.put(`/alerting/rules/${id}`, request);
    return response.data.data;
  },

  deleteRule: async (id: string): Promise<void> => {
    await client.delete(`/alerting/rules/${id}`);
  },

  // Alerts
  getAlerts: async (
    status?: AlertStatus,
    severity?: AlertSeverity,
    ruleId?: string,
    limit?: number
  ): Promise<Alert[]> => {
    const params: Record<string, string | number> = {};
    if (status) params.status = status;
    if (severity) params.severity = severity;
    if (ruleId) params.rule_id = ruleId;
    if (limit) params.limit = limit;
    const response = await client.get('/alerting/alerts', { params });
    return response.data.data;
  },

  getAlert: async (id: string): Promise<Alert> => {
    const response = await client.get(`/alerting/alerts/${id}`);
    return response.data.data;
  },

  getAlertStats: async (): Promise<AlertStats> => {
    const response = await client.get('/alerting/alerts/stats');
    return response.data.data;
  },

  acknowledgeAlert: async (id: string): Promise<Alert> => {
    const response = await client.post(`/alerting/alerts/${id}/acknowledge`);
    return response.data.data;
  },

  resolveAlert: async (id: string): Promise<Alert> => {
    const response = await client.post(`/alerting/alerts/${id}/resolve`);
    return response.data.data;
  },

  silenceAlert: async (id: string): Promise<Alert> => {
    const response = await client.post(`/alerting/alerts/${id}/silence`);
    return response.data.data;
  },

  // Silences
  getSilences: async (): Promise<AlertSilence[]> => {
    const response = await client.get('/alerting/silences');
    return response.data.data;
  },

  createSilence: async (request: CreateSilenceRequest): Promise<AlertSilence> => {
    const response = await client.post('/alerting/silences', request);
    return response.data.data;
  },

  deleteSilence: async (id: string): Promise<void> => {
    await client.delete(`/alerting/silences/${id}`);
  },

  // Trigger & Evaluate
  triggerAlert: async (request: TriggerAlertRequest): Promise<Alert> => {
    const response = await client.post('/alerting/trigger', request);
    return response.data.data;
  },

  evaluateRules: async (): Promise<{ alerts_triggered: number; alerts: Alert[] }> => {
    const response = await client.post('/alerting/evaluate');
    return response.data.data;
  },

  // ============================================================================
  // Code Deploy
  // ============================================================================

  // SSH Keys
  getSshKeys: async (): Promise<CodeSshKey[]> => {
    const response = await client.get('/code/ssh-keys');
    return response.data;
  },

  getSshKey: async (id: string): Promise<CodeSshKey> => {
    const response = await client.get(`/code/ssh-keys/${id}`);
    return response.data;
  },

  createSshKey: async (request: CreateSshKeyRequest): Promise<CodeSshKey> => {
    const response = await client.post('/code/ssh-keys', request);
    return response.data;
  },

  deleteSshKey: async (id: string): Promise<void> => {
    await client.delete(`/code/ssh-keys/${id}`);
  },

  // Repositories
  getCodeRepositories: async (): Promise<CodeRepository[]> => {
    const response = await client.get('/code/repositories');
    return response.data;
  },

  getCodeRepository: async (id: string): Promise<CodeRepository> => {
    const response = await client.get(`/code/repositories/${id}`);
    return response.data;
  },

  createCodeRepository: async (request: CreateRepositoryRequest): Promise<CodeRepository> => {
    const response = await client.post('/code/repositories', request);
    return response.data;
  },

  updateCodeRepository: async (id: string, request: UpdateRepositoryRequest): Promise<CodeRepository> => {
    const response = await client.put(`/code/repositories/${id}`, request);
    return response.data;
  },

  deleteCodeRepository: async (id: string): Promise<void> => {
    await client.delete(`/code/repositories/${id}`);
  },

  syncCodeRepository: async (id: string): Promise<CodeEnvironment[]> => {
    const response = await client.post(`/code/repositories/${id}/sync`);
    return response.data;
  },

  // Environments
  getCodeEnvironments: async (query?: ListEnvironmentsQuery): Promise<CodeEnvironment[]> => {
    const response = await client.get('/code/environments', { params: query });
    return response.data;
  },

  getCodeEnvironment: async (id: string): Promise<CodeEnvironment> => {
    const response = await client.get(`/code/environments/${id}`);
    return response.data;
  },

  updateCodeEnvironment: async (id: string, request: UpdateEnvironmentRequest): Promise<CodeEnvironment> => {
    const response = await client.put(`/code/environments/${id}`, request);
    return response.data;
  },

  // Deployments
  getCodeDeployments: async (query?: ListDeploymentsQuery): Promise<CodeDeployment[]> => {
    const response = await client.get('/code/deployments', { params: query });
    return response.data;
  },

  getCodeDeployment: async (id: string): Promise<CodeDeployment> => {
    const response = await client.get(`/code/deployments/${id}`);
    return response.data;
  },

  triggerDeployment: async (request: TriggerDeploymentRequest): Promise<CodeDeployment> => {
    const response = await client.post('/code/deployments', request);
    return response.data;
  },

  approveDeployment: async (id: string, request?: ApproveDeploymentRequest): Promise<CodeDeployment> => {
    const response = await client.post(`/code/deployments/${id}/approve`, request || {});
    return response.data;
  },

  rejectDeployment: async (id: string, request: RejectDeploymentRequest): Promise<CodeDeployment> => {
    const response = await client.post(`/code/deployments/${id}/reject`, request);
    return response.data;
  },

  retryDeployment: async (id: string): Promise<CodeDeployment> => {
    const response = await client.post(`/code/deployments/${id}/retry`);
    return response.data;
  },

  // Deployment Queue Stats
  getDeploymentQueueStats: async (): Promise<{
    pending: number;
    approved: number;
    deploying: number;
    recent_failures: number;
  }> => {
    const response = await client.get('/code/deployments/stats');
    return response.data;
  },
};
