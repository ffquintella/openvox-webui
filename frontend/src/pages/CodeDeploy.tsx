import { useState } from 'react';
import {
  GitBranch,
  Server,
  Rocket,
  Key,
  RefreshCw,
  Plus,
  Trash2,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  Clock,
  Loader2,
  Copy,
  ChevronRight,
  GitCommit,
  Pencil,
} from 'lucide-react';
import clsx from 'clsx';
import {
  useCodeDeployFeatureStatus,
  useCodeRepositories,
  useCreateCodeRepository,
  useUpdateCodeRepository,
  useDeleteCodeRepository,
  useSyncCodeRepository,
  useCodeEnvironments,
  useUpdateCodeEnvironment,
  useCodeDeployments,
  useApproveDeployment,
  useRejectDeployment,
  useRetryDeployment,
  useSshKeys,
  useCreateSshKey,
  useDeleteSshKey,
  usePatTokens,
  useCreatePatToken,
  useUpdatePatToken,
  useDeletePatToken,
} from '../hooks/useCodeDeploy';
import type {
  CodeRepository,
  CodeEnvironment,
  CodeDeployment,
  CodePatToken,
  DeploymentStatus,
  CreateRepositoryRequest,
  UpdateRepositoryRequest,
  CreateSshKeyRequest,
  CreatePatTokenRequest,
  UpdatePatTokenRequest,
} from '../types';

type TabType = 'repositories' | 'environments' | 'deployments' | 'ssh-keys' | 'pat-tokens';

export default function CodeDeploy() {
  const [activeTab, setActiveTab] = useState<TabType>('repositories');
  const [showCreateRepo, setShowCreateRepo] = useState(false);
  const [editingRepo, setEditingRepo] = useState<CodeRepository | null>(null);
  const [showCreateKey, setShowCreateKey] = useState(false);
  const [showCreatePatToken, setShowCreatePatToken] = useState(false);
  const [editingPatToken, setEditingPatToken] = useState<CodePatToken | null>(null);
  const [selectedDeployment, setSelectedDeployment] = useState<string | null>(null);
  const [rejectReason, setRejectReason] = useState('');
  const [confirmAction, setConfirmAction] = useState<{
    type: 'delete-repo' | 'delete-key' | 'delete-pat-token' | 'approve' | 'reject';
    id: string;
    name?: string;
  } | null>(null);

  const { data: featureStatus } = useCodeDeployFeatureStatus();
  const { data: repositories = [], isLoading: reposLoading } = useCodeRepositories();
  const { data: environments = [], isLoading: envsLoading } = useCodeEnvironments();
  const { data: deployments = [], isLoading: deploysLoading } = useCodeDeployments({ limit: 50 });
  const { data: sshKeys = [], isLoading: keysLoading } = useSshKeys();
  const { data: patTokens = [], isLoading: tokensLoading } = usePatTokens();

  const createRepoMutation = useCreateCodeRepository();
  const updateRepoMutation = useUpdateCodeRepository();
  const deleteRepoMutation = useDeleteCodeRepository();
  const syncRepoMutation = useSyncCodeRepository();
  const updateEnvMutation = useUpdateCodeEnvironment();
  const approveMutation = useApproveDeployment();
  const rejectMutation = useRejectDeployment();
  const retryMutation = useRetryDeployment();
  const createKeyMutation = useCreateSshKey();
  const deleteKeyMutation = useDeleteSshKey();
  const createPatTokenMutation = useCreatePatToken();
  const updatePatTokenMutation = useUpdatePatToken();
  const deletePatTokenMutation = useDeletePatToken();

  const pendingApprovals = deployments.filter((d) => d.status === 'pending').length;
  const activeDeployments = deployments.filter((d) => d.status === 'deploying').length;
  const expiringTokens = patTokens.filter((t) => t.is_expiring_soon || t.is_expired).length;

  const tabs = [
    { id: 'repositories' as const, name: 'Repositories', icon: GitBranch, badge: repositories.length },
    { id: 'environments' as const, name: 'Environments', icon: Server, badge: environments.length },
    {
      id: 'deployments' as const,
      name: 'Deployments',
      icon: Rocket,
      badge: pendingApprovals > 0 ? pendingApprovals : undefined,
    },
    { id: 'ssh-keys' as const, name: 'SSH Keys', icon: Key },
    {
      id: 'pat-tokens' as const,
      name: 'PAT Tokens',
      icon: Key,
      badge: expiringTokens > 0 ? expiringTokens : undefined,
      badgeColor: expiringTokens > 0 ? 'bg-yellow-500' : undefined,
    },
  ];

  const handleConfirmAction = async () => {
    if (!confirmAction) return;

    try {
      if (confirmAction.type === 'delete-repo') {
        await deleteRepoMutation.mutateAsync(confirmAction.id);
      } else if (confirmAction.type === 'delete-key') {
        await deleteKeyMutation.mutateAsync(confirmAction.id);
      } else if (confirmAction.type === 'delete-pat-token') {
        await deletePatTokenMutation.mutateAsync(confirmAction.id);
      } else if (confirmAction.type === 'approve') {
        await approveMutation.mutateAsync({ id: confirmAction.id });
      } else if (confirmAction.type === 'reject') {
        await rejectMutation.mutateAsync({ id: confirmAction.id, request: { reason: rejectReason } });
      }
      setConfirmAction(null);
      setRejectReason('');
    } catch {
      // Error handled by mutation
    }
  };

  const handleCreateRepo = async (data: CreateRepositoryRequest) => {
    try {
      await createRepoMutation.mutateAsync(data);
      setShowCreateRepo(false);
    } catch {
      // Error handled by mutation
    }
  };

  const handleUpdateRepo = async (id: string, data: UpdateRepositoryRequest) => {
    try {
      await updateRepoMutation.mutateAsync({ id, request: data });
      setEditingRepo(null);
    } catch {
      // Error handled by mutation
    }
  };

  const handleCreateKey = async (data: CreateSshKeyRequest) => {
    try {
      await createKeyMutation.mutateAsync(data);
      setShowCreateKey(false);
    } catch {
      // Error handled by mutation
    }
  };

  const handleCreatePatToken = async (data: CreatePatTokenRequest) => {
    try {
      await createPatTokenMutation.mutateAsync(data);
      setShowCreatePatToken(false);
    } catch {
      // Error handled by mutation
    }
  };

  const handleUpdatePatToken = async (id: string, data: UpdatePatTokenRequest) => {
    try {
      await updatePatTokenMutation.mutateAsync({ id, request: data });
      setEditingPatToken(null);
    } catch {
      // Error handled by mutation
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Code Deploy</h1>
          <p className="mt-1 text-sm text-gray-500">
            Manage Git repositories, environments, and Puppet code deployments
          </p>
        </div>
        <div className="flex items-center gap-4">
          {activeDeployments > 0 && (
            <div className="flex items-center gap-2 px-3 py-1.5 bg-blue-100 text-blue-800 rounded-full text-sm">
              <Loader2 className="w-4 h-4 animate-spin" />
              {activeDeployments} deploying
            </div>
          )}
          {pendingApprovals > 0 && (
            <div className="flex items-center gap-2 px-3 py-1.5 bg-yellow-100 text-yellow-800 rounded-full text-sm">
              <Clock className="w-4 h-4" />
              {pendingApprovals} pending approval
            </div>
          )}
        </div>
      </div>

      {/* Feature Disabled Banner */}
      {featureStatus && !featureStatus.enabled && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-6">
          <div className="flex items-start gap-4">
            <AlertTriangle className="w-6 h-6 text-yellow-600 flex-shrink-0 mt-0.5" />
            <div className="flex-1">
              <h3 className="text-lg font-semibold text-yellow-900 mb-2">
                Code Deploy Feature Not Enabled
              </h3>
              <div className="text-sm text-yellow-800 space-y-2">
                <p>
                  The Code Deploy feature is currently disabled. To enable it, add the following configuration to your <code className="bg-yellow-100 px-1.5 py-0.5 rounded">config.yaml</code> file:
                </p>
                {featureStatus.message && (
                  <pre className="bg-yellow-100 p-4 rounded mt-3 overflow-x-auto text-xs">
                    {featureStatus.message}
                  </pre>
                )}
                <p className="mt-3">
                  After updating the configuration, restart the OpenVox WebUI service for the changes to take effect.
                </p>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Tabs */}
      <div className={clsx('border-b border-gray-200', featureStatus && !featureStatus.enabled && 'opacity-50 pointer-events-none')}>
        <nav className="-mb-px flex space-x-8">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={clsx(
                'flex items-center gap-2 py-4 px-1 border-b-2 font-medium text-sm',
                activeTab === tab.id
                  ? 'border-primary-500 text-primary-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
              )}
            >
              <tab.icon className="w-5 h-5" />
              {tab.name}
              {tab.badge !== undefined && (
                <span
                  className={clsx(
                    'ml-2 text-xs font-medium px-2 py-0.5 rounded-full',
                    tab.id === 'deployments' && tab.badge > 0
                      ? 'bg-yellow-100 text-yellow-800'
                      : 'bg-gray-100 text-gray-600'
                  )}
                >
                  {tab.badge}
                </span>
              )}
            </button>
          ))}
        </nav>
      </div>

      {/* Tab Content */}
      <div className={clsx(featureStatus && !featureStatus.enabled && 'opacity-50 pointer-events-none')}>
      {activeTab === 'repositories' && (
        <RepositoriesTab
          repositories={repositories}
          isLoading={reposLoading}
          onSync={(id) => syncRepoMutation.mutate(id)}
          isSyncing={syncRepoMutation.isPending}
          syncingRepoId={syncRepoMutation.variables}
          onEdit={(repo) => setEditingRepo(repo)}
          onDelete={(id, name) => setConfirmAction({ type: 'delete-repo', id, name })}
          onCreateNew={() => setShowCreateRepo(true)}
        />
      )}

      {activeTab === 'environments' && (
        <EnvironmentsTab
          environments={environments}
          isLoading={envsLoading}
          onUpdateEnvironment={(id, request) => updateEnvMutation.mutate({ id, request })}
          onApprove={(id) => setConfirmAction({ type: 'approve', id })}
        />
      )}

      {activeTab === 'deployments' && (
        <DeploymentsTab
          deployments={deployments}
          isLoading={deploysLoading}
          selectedId={selectedDeployment}
          onSelect={setSelectedDeployment}
          onApprove={(id) => setConfirmAction({ type: 'approve', id })}
          onReject={(id) => setConfirmAction({ type: 'reject', id })}
          onRetry={(id) => retryMutation.mutate(id)}
        />
      )}

      {activeTab === 'ssh-keys' && (
        <SshKeysTab
          keys={sshKeys}
          isLoading={keysLoading}
          onDelete={(id, name) => setConfirmAction({ type: 'delete-key', id, name })}
          onCreateNew={() => setShowCreateKey(true)}
        />
      )}

      {activeTab === 'pat-tokens' && (
        <PatTokensTab
          tokens={patTokens}
          isLoading={tokensLoading}
          onDelete={(id, name) => setConfirmAction({ type: 'delete-pat-token', id, name })}
          onEdit={(token) => setEditingPatToken(token)}
          onCreateNew={() => setShowCreatePatToken(true)}
        />
      )}
      </div>

      {/* Create Repository Modal */}
      {showCreateRepo && (
        <CreateRepositoryModal
          sshKeys={sshKeys}
          patTokens={patTokens}
          onClose={() => setShowCreateRepo(false)}
          onCreate={handleCreateRepo}
          isCreating={createRepoMutation.isPending}
        />
      )}

      {/* Edit Repository Modal */}
      {editingRepo && (
        <EditRepositoryModal
          repository={editingRepo}
          sshKeys={sshKeys}
          patTokens={patTokens}
          onClose={() => setEditingRepo(null)}
          onUpdate={(data) => handleUpdateRepo(editingRepo.id, data)}
          isUpdating={updateRepoMutation.isPending}
        />
      )}

      {/* Create SSH Key Modal */}
      {showCreateKey && (
        <CreateSshKeyModal
          onClose={() => setShowCreateKey(false)}
          onCreate={handleCreateKey}
          isCreating={createKeyMutation.isPending}
        />
      )}

      {/* Create PAT Token Modal */}
      {showCreatePatToken && (
        <CreatePatTokenModal
          onClose={() => setShowCreatePatToken(false)}
          onCreate={handleCreatePatToken}
          isCreating={createPatTokenMutation.isPending}
        />
      )}

      {/* Edit PAT Token Modal */}
      {editingPatToken && (
        <EditPatTokenModal
          token={editingPatToken}
          onClose={() => setEditingPatToken(null)}
          onUpdate={(data) => handleUpdatePatToken(editingPatToken.id, data)}
          isUpdating={updatePatTokenMutation.isPending}
        />
      )}

      {/* Confirmation Modal */}
      {confirmAction && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-white rounded-lg shadow-xl max-w-md w-full mx-4 p-6">
            <h3 className="text-lg font-semibold text-gray-900">
              {confirmAction.type === 'delete-repo' && 'Delete Repository'}
              {confirmAction.type === 'delete-key' && 'Delete SSH Key'}
              {confirmAction.type === 'delete-pat-token' && 'Delete PAT Token'}
              {confirmAction.type === 'approve' && 'Approve Deployment'}
              {confirmAction.type === 'reject' && 'Reject Deployment'}
            </h3>
            <div className="mt-2 text-sm text-gray-600">
              {confirmAction.type === 'delete-repo' && (
                <>
                  Are you sure you want to delete repository{' '}
                  <span className="font-mono font-medium">{confirmAction.name}</span>? This will also
                  delete all associated environments and deployments.
                </>
              )}
              {confirmAction.type === 'delete-key' && (
                <>
                  Are you sure you want to delete SSH key{' '}
                  <span className="font-mono font-medium">{confirmAction.name}</span>?
                </>
              )}
              {confirmAction.type === 'delete-pat-token' && (
                <>
                  Are you sure you want to delete PAT token{' '}
                  <span className="font-mono font-medium">{confirmAction.name}</span>? Repositories using
                  this token will no longer be able to authenticate.
                </>
              )}
              {confirmAction.type === 'approve' && (
                <>Are you sure you want to approve this deployment? It will be deployed immediately.</>
              )}
              {confirmAction.type === 'reject' && (
                <div className="space-y-3">
                  <p>Please provide a reason for rejecting this deployment:</p>
                  <textarea
                    value={rejectReason}
                    onChange={(e) => setRejectReason(e.target.value)}
                    className="w-full px-3 py-2 border border-gray-300 rounded-md"
                    rows={3}
                    placeholder="Rejection reason..."
                  />
                </div>
              )}
            </div>
            <div className="mt-6 flex justify-end gap-3">
              <button
                onClick={() => {
                  setConfirmAction(null);
                  setRejectReason('');
                }}
                className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
              >
                Cancel
              </button>
              <button
                onClick={handleConfirmAction}
                disabled={confirmAction.type === 'reject' && !rejectReason.trim()}
                className={clsx(
                  'px-4 py-2 text-sm font-medium text-white rounded-md flex items-center gap-2',
                  (confirmAction.type === 'delete-repo' || confirmAction.type === 'delete-key') &&
                    'bg-red-600 hover:bg-red-700',
                  confirmAction.type === 'approve' && 'bg-green-600 hover:bg-green-700',
                  confirmAction.type === 'reject' && 'bg-yellow-600 hover:bg-yellow-700',
                  confirmAction.type === 'reject' && !rejectReason.trim() && 'opacity-50 cursor-not-allowed'
                )}
              >
                {confirmAction.type.includes('delete') && 'Delete'}
                {confirmAction.type === 'approve' && 'Approve'}
                {confirmAction.type === 'reject' && 'Reject'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// Repositories Tab
function RepositoriesTab({
  repositories,
  isLoading,
  onSync,
  isSyncing,
  syncingRepoId,
  onEdit,
  onDelete,
  onCreateNew,
}: {
  repositories: CodeRepository[];
  isLoading: boolean;
  onSync: (id: string) => void;
  isSyncing: boolean;
  syncingRepoId?: string;
  onEdit: (repo: CodeRepository) => void;
  onDelete: (id: string, name: string) => void;
  onCreateNew: () => void;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-8 h-8 text-primary-600 animate-spin" />
      </div>
    );
  }

  if (repositories.length === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
        <GitBranch className="w-12 h-12 text-gray-300 mx-auto" />
        <h3 className="mt-4 text-lg font-medium text-gray-900">No Repositories</h3>
        <p className="mt-2 text-sm text-gray-500">
          Add a Git repository to start managing your Puppet code.
        </p>
        <button
          onClick={onCreateNew}
          className="mt-4 inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700"
        >
          <Plus className="w-4 h-4" />
          Add Repository
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <button
          onClick={onCreateNew}
          className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700"
        >
          <Plus className="w-4 h-4" />
          Add Repository
        </button>
      </div>

      <div className="grid gap-4">
        {repositories.map((repo) => (
          <div key={repo.id} className="bg-white rounded-lg border border-gray-200 p-6">
            <div className="flex items-start justify-between">
              <div className="flex-1">
                <div className="flex items-center gap-3">
                  <h3 className="text-lg font-medium text-gray-900">{repo.name}</h3>
                  {repo.is_control_repo && (
                    <span className="px-2 py-0.5 text-xs font-medium bg-purple-100 text-purple-800 rounded">
                      Control Repo
                    </span>
                  )}
                </div>
                <p className="mt-1 text-sm text-gray-500 font-mono">{repo.url}</p>

                <div className="mt-4 flex flex-wrap items-center gap-4 text-sm">
                  <div className="flex items-center gap-1 text-gray-600">
                    <GitBranch className="w-4 h-4" />
                    <span>Pattern: {repo.branch_pattern}</span>
                  </div>
                  <div className="flex items-center gap-1 text-gray-600">
                    <Server className="w-4 h-4" />
                    <span>{repo.environment_count} environments</span>
                  </div>
                  <div>
                    <span
                      className={clsx(
                        'px-2 py-0.5 text-xs font-medium rounded',
                        repo.auth_type === 'ssh' && 'bg-blue-100 text-blue-800',
                        repo.auth_type === 'pat' && 'bg-green-100 text-green-800',
                        repo.auth_type === 'none' && 'bg-gray-100 text-gray-800'
                      )}
                    >
                      {repo.auth_type === 'ssh' && 'SSH'}
                      {repo.auth_type === 'pat' && 'PAT'}
                      {repo.auth_type === 'none' && 'Public'}
                    </span>
                  </div>
                  {repo.ssh_key_name && (
                    <div className="flex items-center gap-1 text-gray-600">
                      <Key className="w-4 h-4" />
                      <span>{repo.ssh_key_name}</span>
                    </div>
                  )}
                  {repo.has_pat && (
                    <div className="flex items-center gap-1 text-gray-600">
                      <Key className="w-4 h-4" />
                      <span>PAT configured</span>
                    </div>
                  )}
                  <div className="flex items-center gap-1 text-gray-600">
                    <Clock className="w-4 h-4" />
                    <span>Poll: {repo.poll_interval_seconds}s</span>
                  </div>
                </div>

                {repo.last_error && (
                  <div className="mt-3 flex items-start gap-2 p-2 bg-red-50 rounded text-sm text-red-700">
                    <AlertTriangle className="w-4 h-4 mt-0.5 flex-shrink-0" />
                    <span>{repo.last_error}</span>
                  </div>
                )}

                {repo.webhook_url && (
                  <div className="mt-3 flex items-center gap-2">
                    <span className="text-sm text-gray-500">Webhook:</span>
                    <code className="text-xs bg-gray-100 px-2 py-1 rounded text-gray-700 font-mono">
                      {repo.webhook_url}
                    </code>
                    <button
                      onClick={() => navigator.clipboard.writeText(repo.webhook_url || '')}
                      className="p-1 text-gray-400 hover:text-gray-600"
                      title="Copy webhook URL"
                    >
                      <Copy className="w-4 h-4" />
                    </button>
                  </div>
                )}
              </div>

              <div className="flex items-center gap-2">
                <button
                  onClick={() => onSync(repo.id)}
                  disabled={isSyncing}
                  className="inline-flex items-center gap-2 px-3 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 disabled:opacity-50"
                >
                  <RefreshCw
                    className={clsx('w-4 h-4', isSyncing && syncingRepoId === repo.id && 'animate-spin')}
                  />
                  Sync
                </button>
                <button
                  onClick={() => onEdit(repo)}
                  className="p-2 text-gray-400 hover:text-primary-600"
                  title="Edit repository"
                >
                  <Pencil className="w-5 h-5" />
                </button>
                <button
                  onClick={() => onDelete(repo.id, repo.name)}
                  className="p-2 text-gray-400 hover:text-red-600"
                  title="Delete repository"
                >
                  <Trash2 className="w-5 h-5" />
                </button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// Environments Tab
function EnvironmentsTab({
  environments,
  isLoading,
  onUpdateEnvironment,
  onApprove,
}: {
  environments: CodeEnvironment[];
  isLoading: boolean;
  onUpdateEnvironment: (id: string, request: { auto_deploy?: boolean; requires_approval?: boolean }) => void;
  onApprove: (id: string) => void;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-8 h-8 text-primary-600 animate-spin" />
      </div>
    );
  }

  if (environments.length === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
        <Server className="w-12 h-12 text-gray-300 mx-auto" />
        <h3 className="mt-4 text-lg font-medium text-gray-900">No Environments</h3>
        <p className="mt-2 text-sm text-gray-500">
          Environments are discovered automatically when you sync a repository.
        </p>
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
      <table className="min-w-full divide-y divide-gray-200">
        <thead className="bg-gray-50">
          <tr>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
              Environment
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
              Repository
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
              Current Commit
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Status</th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
              Settings
            </th>
            <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">
              Actions
            </th>
          </tr>
        </thead>
        <tbody className="bg-white divide-y divide-gray-200">
          {environments.map((env) => (
            <tr key={env.id} className="hover:bg-gray-50">
              <td className="px-6 py-4 whitespace-nowrap">
                <div className="flex items-center gap-2">
                  <GitBranch className="w-4 h-4 text-gray-400" />
                  <span className="font-medium text-gray-900">{env.name}</span>
                </div>
              </td>
              <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                {env.repository_name}
              </td>
              <td className="px-6 py-4">
                {env.current_commit ? (
                  <div className="flex items-center gap-2">
                    <GitCommit className="w-4 h-4 text-gray-400" />
                    <code className="text-xs bg-gray-100 px-2 py-1 rounded font-mono">
                      {env.current_commit.substring(0, 7)}
                    </code>
                    {env.current_commit_message && (
                      <span className="text-sm text-gray-500 truncate max-w-xs">
                        {env.current_commit_message}
                      </span>
                    )}
                  </div>
                ) : (
                  <span className="text-sm text-gray-400">Not deployed</span>
                )}
              </td>
              <td className="px-6 py-4 whitespace-nowrap">
                <DeploymentStatusBadge status={env.latest_deployment_status} />
              </td>
              <td className="px-6 py-4 whitespace-nowrap">
                <div className="flex items-center gap-4">
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={env.auto_deploy}
                      onChange={(e) => onUpdateEnvironment(env.id, { auto_deploy: e.target.checked })}
                      className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
                    />
                    Auto-deploy
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={env.requires_approval}
                      onChange={(e) =>
                        onUpdateEnvironment(env.id, { requires_approval: e.target.checked })
                      }
                      className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
                    />
                    Require approval
                  </label>
                </div>
              </td>
              <td className="px-6 py-4 whitespace-nowrap text-right">
                {env.pending_deployment && (
                  <button
                    onClick={() => onApprove(env.pending_deployment!.id)}
                    className="inline-flex items-center gap-1 px-3 py-1.5 text-sm font-medium text-green-700 bg-green-100 rounded-md hover:bg-green-200"
                  >
                    <CheckCircle2 className="w-4 h-4" />
                    Approve
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// Deployments Tab
function DeploymentsTab({
  deployments,
  isLoading,
  selectedId,
  onSelect,
  onApprove,
  onReject,
  onRetry,
}: {
  deployments: CodeDeployment[];
  isLoading: boolean;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onApprove: (id: string) => void;
  onReject: (id: string) => void;
  onRetry: (id: string) => void;
}) {
  const selectedDeployment = deployments.find((d) => d.id === selectedId);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-8 h-8 text-primary-600 animate-spin" />
      </div>
    );
  }

  if (deployments.length === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
        <Rocket className="w-12 h-12 text-gray-300 mx-auto" />
        <h3 className="mt-4 text-lg font-medium text-gray-900">No Deployments</h3>
        <p className="mt-2 text-sm text-gray-500">
          Deployments will appear here when commits are pushed or synced.
        </p>
      </div>
    );
  }

  return (
    <div className="flex gap-6">
      {/* Deployment List */}
      <div className="flex-1 bg-white rounded-lg border border-gray-200 overflow-hidden">
        <div className="divide-y divide-gray-200">
          {deployments.map((deployment) => (
            <div
              key={deployment.id}
              onClick={() => onSelect(deployment.id)}
              className={clsx(
                'p-4 cursor-pointer hover:bg-gray-50',
                selectedId === deployment.id && 'bg-primary-50 border-l-4 border-l-primary-500'
              )}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <DeploymentStatusBadge status={deployment.status} />
                  <div>
                    <p className="font-medium text-gray-900">{deployment.environment_name}</p>
                    <p className="text-sm text-gray-500">{deployment.repository_name}</p>
                  </div>
                </div>
                <ChevronRight className="w-5 h-5 text-gray-400" />
              </div>
              <div className="mt-2 flex items-center gap-4 text-sm text-gray-500">
                <div className="flex items-center gap-1">
                  <GitCommit className="w-4 h-4" />
                  <code className="font-mono">{deployment.commit_sha.substring(0, 7)}</code>
                </div>
                <span>{new Date(deployment.created_at).toLocaleString()}</span>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Deployment Details */}
      {selectedDeployment && (
        <div className="w-96 bg-white rounded-lg border border-gray-200 p-6">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-lg font-medium text-gray-900">Deployment Details</h3>
            <button onClick={() => onSelect(null)} className="text-gray-400 hover:text-gray-600">
              <XCircle className="w-5 h-5" />
            </button>
          </div>

          <div className="space-y-4">
            <div>
              <label className="text-xs font-medium text-gray-500 uppercase">Status</label>
              <div className="mt-1">
                <DeploymentStatusBadge status={selectedDeployment.status} />
              </div>
            </div>

            <div>
              <label className="text-xs font-medium text-gray-500 uppercase">Environment</label>
              <p className="mt-1 text-sm text-gray-900">{selectedDeployment.environment_name}</p>
            </div>

            <div>
              <label className="text-xs font-medium text-gray-500 uppercase">Commit</label>
              <p className="mt-1 font-mono text-sm text-gray-900">{selectedDeployment.commit_sha}</p>
              {selectedDeployment.commit_message && (
                <p className="mt-1 text-sm text-gray-500">{selectedDeployment.commit_message}</p>
              )}
            </div>

            {selectedDeployment.commit_author && (
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase">Author</label>
                <p className="mt-1 text-sm text-gray-900">{selectedDeployment.commit_author}</p>
              </div>
            )}

            {selectedDeployment.duration_seconds && (
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase">Duration</label>
                <p className="mt-1 text-sm text-gray-900">{selectedDeployment.duration_seconds}s</p>
              </div>
            )}

            {selectedDeployment.error_message && (
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase">Error</label>
                <p className="mt-1 text-sm text-red-600">{selectedDeployment.error_message}</p>
              </div>
            )}

            {selectedDeployment.rejection_reason && (
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase">Rejection Reason</label>
                <p className="mt-1 text-sm text-yellow-600">{selectedDeployment.rejection_reason}</p>
              </div>
            )}

            {selectedDeployment.r10k_output && (
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase">r10k Output</label>
                <pre className="mt-1 p-2 bg-gray-100 rounded text-xs overflow-auto max-h-48">
                  {selectedDeployment.r10k_output}
                </pre>
              </div>
            )}

            {/* Actions */}
            <div className="pt-4 border-t border-gray-200 flex gap-2">
              {selectedDeployment.status === 'pending' && (
                <>
                  <button
                    onClick={() => onApprove(selectedDeployment.id)}
                    className="flex-1 inline-flex items-center justify-center gap-2 px-4 py-2 text-sm font-medium text-white bg-green-600 rounded-md hover:bg-green-700"
                  >
                    <CheckCircle2 className="w-4 h-4" />
                    Approve
                  </button>
                  <button
                    onClick={() => onReject(selectedDeployment.id)}
                    className="flex-1 inline-flex items-center justify-center gap-2 px-4 py-2 text-sm font-medium text-white bg-yellow-600 rounded-md hover:bg-yellow-700"
                  >
                    <XCircle className="w-4 h-4" />
                    Reject
                  </button>
                </>
              )}
              {(selectedDeployment.status === 'failed' || selectedDeployment.status === 'rejected') && (
                <button
                  onClick={() => onRetry(selectedDeployment.id)}
                  className="flex-1 inline-flex items-center justify-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700"
                >
                  <RefreshCw className="w-4 h-4" />
                  Retry
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// SSH Keys Tab
function SshKeysTab({
  keys,
  isLoading,
  onDelete,
  onCreateNew,
}: {
  keys: { id: string; name: string; public_key: string; created_at: string }[];
  isLoading: boolean;
  onDelete: (id: string, name: string) => void;
  onCreateNew: () => void;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-8 h-8 text-primary-600 animate-spin" />
      </div>
    );
  }

  if (keys.length === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
        <Key className="w-12 h-12 text-gray-300 mx-auto" />
        <h3 className="mt-4 text-lg font-medium text-gray-900">No SSH Keys</h3>
        <p className="mt-2 text-sm text-gray-500">
          Add SSH keys to authenticate with private Git repositories.
        </p>
        <button
          onClick={onCreateNew}
          className="mt-4 inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700"
        >
          <Plus className="w-4 h-4" />
          Add SSH Key
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <button
          onClick={onCreateNew}
          className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700"
        >
          <Plus className="w-4 h-4" />
          Add SSH Key
        </button>
      </div>

      <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
        <table className="min-w-full divide-y divide-gray-200">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Name</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                Public Key
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                Created
              </th>
              <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">
                Actions
              </th>
            </tr>
          </thead>
          <tbody className="bg-white divide-y divide-gray-200">
            {keys.map((key) => (
              <tr key={key.id} className="hover:bg-gray-50">
                <td className="px-6 py-4 whitespace-nowrap">
                  <div className="flex items-center gap-2">
                    <Key className="w-4 h-4 text-gray-400" />
                    <span className="font-medium text-gray-900">{key.name}</span>
                  </div>
                </td>
                <td className="px-6 py-4">
                  <div className="flex items-center gap-2">
                    <code className="text-xs bg-gray-100 px-2 py-1 rounded font-mono text-gray-700 truncate max-w-md">
                      {key.public_key.substring(0, 60)}...
                    </code>
                    <button
                      onClick={() => navigator.clipboard.writeText(key.public_key)}
                      className="p-1 text-gray-400 hover:text-gray-600"
                      title="Copy public key"
                    >
                      <Copy className="w-4 h-4" />
                    </button>
                  </div>
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                  {new Date(key.created_at).toLocaleDateString()}
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-right">
                  <button
                    onClick={() => onDelete(key.id, key.name)}
                    className="p-2 text-gray-400 hover:text-red-600"
                  >
                    <Trash2 className="w-5 h-5" />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// PAT Tokens Tab
function PatTokensTab({
  tokens,
  isLoading,
  onDelete,
  onEdit,
  onCreateNew,
}: {
  tokens: CodePatToken[];
  isLoading: boolean;
  onDelete: (id: string, name: string) => void;
  onEdit: (token: CodePatToken) => void;
  onCreateNew: () => void;
}) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-8 h-8 text-primary-600 animate-spin" />
      </div>
    );
  }

  if (tokens.length === 0) {
    return (
      <div className="bg-white rounded-lg border border-gray-200 p-12 text-center">
        <Key className="w-12 h-12 text-gray-300 mx-auto" />
        <h3 className="mt-4 text-lg font-medium text-gray-900">No PAT Tokens</h3>
        <p className="mt-2 text-sm text-gray-500">
          Add Personal Access Tokens to authenticate with Git repositories using HTTPS.
        </p>
        <button
          onClick={onCreateNew}
          className="mt-4 inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700"
        >
          <Plus className="w-4 h-4" />
          Add PAT Token
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <button
          onClick={onCreateNew}
          className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700"
        >
          <Plus className="w-4 h-4" />
          Add PAT Token
        </button>
      </div>

      <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
        <table className="min-w-full divide-y divide-gray-200">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Name</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                Description
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                Expiration
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                Last Used
              </th>
              <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase">
                Actions
              </th>
            </tr>
          </thead>
          <tbody className="bg-white divide-y divide-gray-200">
            {tokens.map((token) => (
              <tr key={token.id} className="hover:bg-gray-50">
                <td className="px-6 py-4 whitespace-nowrap">
                  <div className="flex items-center gap-2">
                    <Key className="w-4 h-4 text-gray-400" />
                    <span className="font-medium text-gray-900">{token.name}</span>
                    {token.is_expired && (
                      <span className="px-2 py-0.5 text-xs font-medium text-red-700 bg-red-100 rounded-full">
                        Expired
                      </span>
                    )}
                    {!token.is_expired && token.is_expiring_soon && (
                      <span className="px-2 py-0.5 text-xs font-medium text-yellow-700 bg-yellow-100 rounded-full">
                        Expiring Soon
                      </span>
                    )}
                  </div>
                </td>
                <td className="px-6 py-4 text-sm text-gray-500 max-w-xs truncate">
                  {token.description || '-'}
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-sm">
                  {token.expires_at ? (
                    <span className={clsx(
                      token.is_expired ? 'text-red-600' : token.is_expiring_soon ? 'text-yellow-600' : 'text-gray-500'
                    )}>
                      {new Date(token.expires_at).toLocaleDateString()}
                      {token.days_until_expiration !== undefined && token.days_until_expiration >= 0 && (
                        <span className="text-xs ml-1">({token.days_until_expiration}d)</span>
                      )}
                    </span>
                  ) : (
                    <span className="text-gray-400">Never</span>
                  )}
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                  {token.last_validated_at
                    ? new Date(token.last_validated_at).toLocaleDateString()
                    : 'Never'}
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-right">
                  <div className="flex justify-end gap-2">
                    <button
                      onClick={() => onEdit(token)}
                      className="p-2 text-gray-400 hover:text-primary-600"
                      title="Edit token"
                    >
                      <Pencil className="w-5 h-5" />
                    </button>
                    <button
                      onClick={() => onDelete(token.id, token.name)}
                      className="p-2 text-gray-400 hover:text-red-600"
                      title="Delete token"
                    >
                      <Trash2 className="w-5 h-5" />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// Create PAT Token Modal
function CreatePatTokenModal({
  onClose,
  onCreate,
  isCreating,
}: {
  onClose: () => void;
  onCreate: (data: CreatePatTokenRequest) => void;
  isCreating: boolean;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [token, setToken] = useState('');
  const [expiresAt, setExpiresAt] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    // Convert date to ISO 8601 format for the backend
    const expiresAtIso = expiresAt ? new Date(expiresAt + 'T23:59:59Z').toISOString() : undefined;
    onCreate({
      name,
      description: description || undefined,
      token,
      expires_at: expiresAtIso,
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl max-w-lg w-full mx-4 p-6">
        <h3 className="text-lg font-semibold text-gray-900">Add PAT Token</h3>
        <form onSubmit={handleSubmit} className="mt-4 space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700">Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder="github-pat"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Description (optional)</label>
            <input
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder="Token for control repo access"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Token</label>
            <input
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500 font-mono"
              placeholder="ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
            />
            <p className="mt-1 text-xs text-gray-500">
              GitHub tokens start with ghp_, GitLab tokens are random strings
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Expiration Date (optional)</label>
            <input
              type="date"
              value={expiresAt}
              onChange={(e) => setExpiresAt(e.target.value)}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
            />
            <p className="mt-1 text-xs text-gray-500">
              Set this to match your token's expiration. You'll be warned when it's about to expire.
            </p>
          </div>

          <div className="flex justify-end gap-3 pt-4">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 rounded-md hover:bg-gray-200"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isCreating}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700 disabled:opacity-50"
            >
              {isCreating && <Loader2 className="w-4 h-4 animate-spin" />}
              Add Token
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// Edit PAT Token Modal
function EditPatTokenModal({
  token: initialToken,
  onClose,
  onUpdate,
  isUpdating,
}: {
  token: CodePatToken;
  onClose: () => void;
  onUpdate: (data: UpdatePatTokenRequest) => void;
  isUpdating: boolean;
}) {
  const [name, setName] = useState(initialToken.name);
  const [description, setDescription] = useState(initialToken.description || '');
  const [newToken, setNewToken] = useState('');
  const [expiresAt, setExpiresAt] = useState(
    initialToken.expires_at ? initialToken.expires_at.split('T')[0] : ''
  );

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    // Convert date to ISO 8601 format for the backend
    const expiresAtIso = expiresAt ? new Date(expiresAt + 'T23:59:59Z').toISOString() : undefined;
    onUpdate({
      name: name !== initialToken.name ? name : undefined,
      description: description !== (initialToken.description || '') ? description : undefined,
      token: newToken || undefined,
      expires_at: expiresAtIso,
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl max-w-lg w-full mx-4 p-6">
        <h3 className="text-lg font-semibold text-gray-900">Edit PAT Token</h3>
        <form onSubmit={handleSubmit} className="mt-4 space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700">Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Description</label>
            <input
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">New Token (leave empty to keep current)</label>
            <input
              type="password"
              value={newToken}
              onChange={(e) => setNewToken(e.target.value)}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500 font-mono"
              placeholder="ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Expiration Date</label>
            <input
              type="date"
              value={expiresAt}
              onChange={(e) => setExpiresAt(e.target.value)}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
            />
          </div>

          <div className="flex justify-end gap-3 pt-4">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 rounded-md hover:bg-gray-200"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isUpdating}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700 disabled:opacity-50"
            >
              {isUpdating && <Loader2 className="w-4 h-4 animate-spin" />}
              Update Token
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// Deployment Status Badge
function DeploymentStatusBadge({ status }: { status?: DeploymentStatus }) {
  if (!status) {
    return <span className="text-sm text-gray-400">-</span>;
  }

  const config: Record<
    DeploymentStatus,
    { icon: typeof CheckCircle2; color: string; bg: string; text: string }
  > = {
    pending: { icon: Clock, color: 'text-yellow-600', bg: 'bg-yellow-100', text: 'Pending' },
    approved: { icon: CheckCircle2, color: 'text-blue-600', bg: 'bg-blue-100', text: 'Approved' },
    rejected: { icon: XCircle, color: 'text-orange-600', bg: 'bg-orange-100', text: 'Rejected' },
    deploying: { icon: Loader2, color: 'text-blue-600', bg: 'bg-blue-100', text: 'Deploying' },
    success: { icon: CheckCircle2, color: 'text-green-600', bg: 'bg-green-100', text: 'Success' },
    failed: { icon: XCircle, color: 'text-red-600', bg: 'bg-red-100', text: 'Failed' },
    cancelled: { icon: XCircle, color: 'text-gray-600', bg: 'bg-gray-100', text: 'Cancelled' },
  };

  const { icon: Icon, color, bg, text } = config[status];

  return (
    <span className={clsx('inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-full', bg, color)}>
      <Icon className={clsx('w-3 h-3', status === 'deploying' && 'animate-spin')} />
      {text}
    </span>
  );
}

// Create Repository Modal
function CreateRepositoryModal({
  sshKeys,
  patTokens,
  onClose,
  onCreate,
  isCreating,
}: {
  sshKeys: { id: string; name: string }[];
  patTokens: CodePatToken[];
  onClose: () => void;
  onCreate: (data: CreateRepositoryRequest) => void;
  isCreating: boolean;
}) {
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [branchPattern, setBranchPattern] = useState('*');
  const [authType, setAuthType] = useState<'ssh' | 'pat' | 'none'>('ssh');
  const [sshKeyId, setSshKeyId] = useState<string>('');
  const [patTokenId, setPatTokenId] = useState<string>('');
  const [pollInterval, setPollInterval] = useState(300);
  const [isControlRepo, setIsControlRepo] = useState(false);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onCreate({
      name,
      url,
      branch_pattern: branchPattern,
      auth_type: authType,
      ssh_key_id: authType === 'ssh' ? (sshKeyId || undefined) : undefined,
      pat_token_id: authType === 'pat' ? (patTokenId || undefined) : undefined,
      poll_interval_seconds: pollInterval,
      is_control_repo: isControlRepo,
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl max-w-lg w-full mx-4 p-6">
        <h3 className="text-lg font-semibold text-gray-900">Add Repository</h3>
        <form onSubmit={handleSubmit} className="mt-4 space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700">Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder="control-repo"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Git URL</label>
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder={authType === 'ssh' ? 'git@github.com:org/repo.git' : 'https://github.com/org/repo.git'}
            />
            <p className="mt-1 text-xs text-gray-500">
              {authType === 'ssh' && 'Use SSH URL format (git@github.com:...)'}
              {authType === 'pat' && 'Use HTTPS URL format (https://github.com/...)'}
              {authType === 'none' && 'Public repository URL'}
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Branch Pattern</label>
            <input
              type="text"
              value={branchPattern}
              onChange={(e) => setBranchPattern(e.target.value)}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder="* (all branches)"
            />
            <p className="mt-1 text-xs text-gray-500">Glob pattern for branch filtering (e.g., *, feature/*, production)</p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Authentication Method</label>
            <div className="mt-2 space-y-2">
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  name="auth-type"
                  value="ssh"
                  checked={authType === 'ssh'}
                  onChange={(e) => setAuthType(e.target.value as 'ssh')}
                  className="text-primary-600 focus:ring-primary-500"
                />
                <span className="text-sm text-gray-700">SSH Key</span>
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  name="auth-type"
                  value="pat"
                  checked={authType === 'pat'}
                  onChange={(e) => setAuthType(e.target.value as 'pat')}
                  className="text-primary-600 focus:ring-primary-500"
                />
                <span className="text-sm text-gray-700">Personal Access Token (PAT)</span>
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  name="auth-type"
                  value="none"
                  checked={authType === 'none'}
                  onChange={(e) => setAuthType(e.target.value as 'none')}
                  className="text-primary-600 focus:ring-primary-500"
                />
                <span className="text-sm text-gray-700">None (Public Repository)</span>
              </label>
            </div>
          </div>

          {authType === 'ssh' && (
            <div>
              <label className="block text-sm font-medium text-gray-700">SSH Key</label>
              <select
                value={sshKeyId}
                onChange={(e) => setSshKeyId(e.target.value)}
                className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              >
                <option value="">Select an SSH key</option>
                {sshKeys.map((key) => (
                  <option key={key.id} value={key.id}>
                    {key.name}
                  </option>
                ))}
              </select>
            </div>
          )}

          {authType === 'pat' && (
            <div>
              <label className="block text-sm font-medium text-gray-700">PAT Token</label>
              <select
                value={patTokenId}
                onChange={(e) => setPatTokenId(e.target.value)}
                className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              >
                <option value="">Select a PAT token</option>
                {patTokens.map((token) => (
                  <option key={token.id} value={token.id} disabled={token.is_expired}>
                    {token.name}
                    {token.is_expired && ' (Expired)'}
                    {!token.is_expired && token.is_expiring_soon && ' (Expiring Soon)'}
                  </option>
                ))}
              </select>
              {patTokens.length === 0 && (
                <p className="mt-1 text-xs text-yellow-600">
                  No PAT tokens available. Create one in the PAT Tokens tab first.
                </p>
              )}
            </div>
          )}

          <div>
            <label className="block text-sm font-medium text-gray-700">Poll Interval (seconds)</label>
            <input
              type="number"
              value={pollInterval}
              onChange={(e) => setPollInterval(parseInt(e.target.value))}
              min={0}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
            />
            <p className="mt-1 text-xs text-gray-500">Set to 0 to disable polling (webhook only)</p>
          </div>

          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="is-control-repo"
              checked={isControlRepo}
              onChange={(e) => setIsControlRepo(e.target.checked)}
              className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
            />
            <label htmlFor="is-control-repo" className="text-sm text-gray-700">
              This is a control repository (contains Puppetfile)
            </label>
          </div>

          <div className="pt-4 flex justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={
                isCreating ||
                !name ||
                !url ||
                (authType === 'ssh' && !sshKeyId) ||
                (authType === 'pat' && !patTokenId)
              }
              className="px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700 disabled:opacity-50 flex items-center gap-2"
            >
              {isCreating && <Loader2 className="w-4 h-4 animate-spin" />}
              Add Repository
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// Edit Repository Modal
function EditRepositoryModal({
  repository,
  sshKeys,
  patTokens,
  onClose,
  onUpdate,
  isUpdating,
}: {
  repository: CodeRepository;
  sshKeys: { id: string; name: string }[];
  patTokens: CodePatToken[];
  onClose: () => void;
  onUpdate: (data: UpdateRepositoryRequest) => void;
  isUpdating: boolean;
}) {
  const [name, setName] = useState(repository.name);
  const [url, setUrl] = useState(repository.url);
  const [branchPattern, setBranchPattern] = useState(repository.branch_pattern);
  const [authType, setAuthType] = useState<'ssh' | 'pat' | 'none'>(repository.auth_type);
  const [sshKeyId, setSshKeyId] = useState<string>(repository.ssh_key_id || '');
  const [patTokenId, setPatTokenId] = useState<string>(repository.pat_token_id || '');
  const [clearSshKey, setClearSshKey] = useState(false);
  const [clearPatToken, setClearPatToken] = useState(false);
  const [pollInterval, setPollInterval] = useState(repository.poll_interval_seconds);
  const [isControlRepo, setIsControlRepo] = useState(repository.is_control_repo);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const updateData: UpdateRepositoryRequest = {
      name: name !== repository.name ? name : undefined,
      url: url !== repository.url ? url : undefined,
      branch_pattern: branchPattern !== repository.branch_pattern ? branchPattern : undefined,
      auth_type: authType !== repository.auth_type ? authType : undefined,
      poll_interval_seconds: pollInterval !== repository.poll_interval_seconds ? pollInterval : undefined,
      is_control_repo: isControlRepo !== repository.is_control_repo ? isControlRepo : undefined,
    };

    // Handle SSH key changes
    if (authType === 'ssh') {
      if (clearSshKey) {
        updateData.clear_ssh_key = true;
      } else if (sshKeyId && sshKeyId !== repository.ssh_key_id) {
        updateData.ssh_key_id = sshKeyId;
      }
    }

    // Handle PAT token changes
    if (authType === 'pat') {
      if (clearPatToken) {
        updateData.clear_pat_token = true;
      } else if (patTokenId && patTokenId !== repository.pat_token_id) {
        updateData.pat_token_id = patTokenId;
      }
    }

    onUpdate(updateData);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl max-w-lg w-full mx-4 p-6 max-h-[90vh] overflow-y-auto">
        <h3 className="text-lg font-semibold text-gray-900">Edit Repository</h3>
        <form onSubmit={handleSubmit} className="mt-4 space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700">Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder="control-repo"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Git URL</label>
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder={authType === 'ssh' ? 'git@github.com:org/repo.git' : 'https://github.com/org/repo.git'}
            />
            <p className="mt-1 text-xs text-gray-500">
              {authType === 'ssh' && 'Use SSH URL format (git@github.com:...)'}
              {authType === 'pat' && 'Use HTTPS URL format (https://github.com/...)'}
              {authType === 'none' && 'Public repository URL'}
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Branch Pattern</label>
            <input
              type="text"
              value={branchPattern}
              onChange={(e) => setBranchPattern(e.target.value)}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder="* (all branches)"
            />
            <p className="mt-1 text-xs text-gray-500">Glob pattern for branch filtering (e.g., *, feature/*, production)</p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Authentication Method</label>
            <div className="mt-2 space-y-2">
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  name="auth-type"
                  value="ssh"
                  checked={authType === 'ssh'}
                  onChange={(e) => setAuthType(e.target.value as 'ssh')}
                  className="text-primary-600 focus:ring-primary-500"
                />
                <span className="text-sm text-gray-700">SSH Key</span>
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  name="auth-type"
                  value="pat"
                  checked={authType === 'pat'}
                  onChange={(e) => setAuthType(e.target.value as 'pat')}
                  className="text-primary-600 focus:ring-primary-500"
                />
                <span className="text-sm text-gray-700">Personal Access Token (PAT)</span>
              </label>
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  name="auth-type"
                  value="none"
                  checked={authType === 'none'}
                  onChange={(e) => setAuthType(e.target.value as 'none')}
                  className="text-primary-600 focus:ring-primary-500"
                />
                <span className="text-sm text-gray-700">None (Public Repository)</span>
              </label>
            </div>
          </div>

          {authType === 'ssh' && (
            <div className="space-y-3">
              <div>
                <label className="block text-sm font-medium text-gray-700">SSH Key</label>
                <select
                  value={sshKeyId}
                  onChange={(e) => {
                    setSshKeyId(e.target.value);
                    setClearSshKey(false);
                  }}
                  disabled={clearSshKey}
                  className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500 disabled:opacity-50"
                >
                  <option value="">Select an SSH key</option>
                  {sshKeys.map((key) => (
                    <option key={key.id} value={key.id}>
                      {key.name}
                    </option>
                  ))}
                </select>
                {repository.ssh_key_name && (
                  <p className="mt-1 text-xs text-gray-500">
                    Current: {repository.ssh_key_name}
                  </p>
                )}
              </div>
              {repository.ssh_key_id && (
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="clear-ssh-key"
                    checked={clearSshKey}
                    onChange={(e) => {
                      setClearSshKey(e.target.checked);
                      if (e.target.checked) setSshKeyId('');
                    }}
                    className="rounded border-gray-300 text-red-600 focus:ring-red-500"
                  />
                  <label htmlFor="clear-ssh-key" className="text-sm text-red-600">
                    Remove SSH key
                  </label>
                </div>
              )}
            </div>
          )}

          {authType === 'pat' && (
            <div className="space-y-3">
              <div>
                <label className="block text-sm font-medium text-gray-700">PAT Token</label>
                <select
                  value={patTokenId}
                  onChange={(e) => {
                    setPatTokenId(e.target.value);
                    setClearPatToken(false);
                  }}
                  disabled={clearPatToken}
                  className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500 disabled:opacity-50"
                >
                  <option value="">Select a PAT token</option>
                  {patTokens.map((token) => (
                    <option key={token.id} value={token.id} disabled={token.is_expired}>
                      {token.name}
                      {token.is_expired && ' (Expired)'}
                      {!token.is_expired && token.is_expiring_soon && ' (Expiring Soon)'}
                    </option>
                  ))}
                </select>
                {repository.pat_token_name && (
                  <p className="mt-1 text-xs text-gray-500">
                    Current: {repository.pat_token_name}
                  </p>
                )}
                {patTokens.length === 0 && (
                  <p className="mt-1 text-xs text-yellow-600">
                    No PAT tokens available. Create one in the PAT Tokens tab first.
                  </p>
                )}
              </div>
              {repository.pat_token_id && (
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="clear-pat-token"
                    checked={clearPatToken}
                    onChange={(e) => {
                      setClearPatToken(e.target.checked);
                      if (e.target.checked) setPatTokenId('');
                    }}
                    className="rounded border-gray-300 text-red-600 focus:ring-red-500"
                  />
                  <label htmlFor="clear-pat-token" className="text-sm text-red-600">
                    Remove PAT Token
                  </label>
                </div>
              )}
            </div>
          )}

          <div>
            <label className="block text-sm font-medium text-gray-700">Poll Interval (seconds)</label>
            <input
              type="number"
              value={pollInterval}
              onChange={(e) => setPollInterval(parseInt(e.target.value))}
              min={0}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
            />
            <p className="mt-1 text-xs text-gray-500">Set to 0 to disable polling (webhook only)</p>
          </div>

          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="is-control-repo-edit"
              checked={isControlRepo}
              onChange={(e) => setIsControlRepo(e.target.checked)}
              className="rounded border-gray-300 text-primary-600 focus:ring-primary-500"
            />
            <label htmlFor="is-control-repo-edit" className="text-sm text-gray-700">
              This is a control repository (contains Puppetfile)
            </label>
          </div>

          <div className="pt-4 flex justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isUpdating}
              className="px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700 disabled:opacity-50 flex items-center gap-2"
            >
              {isUpdating && <Loader2 className="w-4 h-4 animate-spin" />}
              Update Repository
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// Create SSH Key Modal
function CreateSshKeyModal({
  onClose,
  onCreate,
  isCreating,
}: {
  onClose: () => void;
  onCreate: (data: CreateSshKeyRequest) => void;
  isCreating: boolean;
}) {
  const [name, setName] = useState('');
  const [privateKey, setPrivateKey] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onCreate({ name, private_key: privateKey });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-lg shadow-xl max-w-lg w-full mx-4 p-6">
        <h3 className="text-lg font-semibold text-gray-900">Add SSH Key</h3>
        <form onSubmit={handleSubmit} className="mt-4 space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700">Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
              placeholder="github-deploy-key"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700">Private Key (OpenSSH format)</label>
            <textarea
              value={privateKey}
              onChange={(e) => setPrivateKey(e.target.value)}
              required
              rows={10}
              className="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500 font-mono text-xs"
              placeholder="-----BEGIN OPENSSH PRIVATE KEY-----&#10;...&#10;-----END OPENSSH PRIVATE KEY-----"
            />
            <p className="mt-1 text-xs text-gray-500">
              Paste your SSH private key (OpenSSH format). It will be encrypted and stored securely.
              Supported: RSA, Ed25519, ECDSA keys.
            </p>
          </div>

          <div className="pt-4 flex justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isCreating || !name || !privateKey}
              className="px-4 py-2 text-sm font-medium text-white bg-primary-600 rounded-md hover:bg-primary-700 disabled:opacity-50 flex items-center gap-2"
            >
              {isCreating && <Loader2 className="w-4 h-4 animate-spin" />}
              Add SSH Key
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
