import axios from 'axios';
import type {
  Node,
  NodeGroup,
  Report,
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

export const api = {
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
    return response.data;
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
};
