import { useState } from 'react';
import {
  HardDrive,
  Calendar,
  History,
  RefreshCw,
  Plus,
  Trash2,
  Download,
  Shield,
  Clock,
  Loader2,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  Lock,
  Unlock,
  Database,
  FileText,
  RotateCcw,
} from 'lucide-react';
import clsx from 'clsx';
import {
  useBackupFeatureStatus,
  useBackups,
  useCreateBackup,
  useDeleteBackup,
  useVerifyBackup,
  useRestoreBackup,
  useDownloadBackup,
  useBackupSchedule,
  useUpdateBackupSchedule,
  useBackupRestores,
} from '../hooks/useBackup';
import type {
  ServerBackup,
  BackupSchedule,
  BackupRestore,
  BackupStatus,
  CreateBackupRequest,
} from '../types';

type TabType = 'backups' | 'schedule' | 'history';

export default function Backup() {
  const [activeTab, setActiveTab] = useState<TabType>('backups');
  const [showCreateBackup, setShowCreateBackup] = useState(false);
  const [verifyingBackup, setVerifyingBackup] = useState<string | null>(null);
  const [restoringBackup, setRestoringBackup] = useState<string | null>(null);
  const [confirmAction, setConfirmAction] = useState<{
    type: 'delete' | 'restore' | 'verify';
    backup: ServerBackup;
  } | null>(null);

  const { data: featureStatus, isLoading: statusLoading } = useBackupFeatureStatus();
  const { data: backups = [], isLoading: backupsLoading, refetch: refetchBackups } = useBackups();
  const { data: schedule, isLoading: scheduleLoading } = useBackupSchedule();
  const { data: restores = [], isLoading: restoresLoading } = useBackupRestores();

  const createBackupMutation = useCreateBackup();
  const deleteBackupMutation = useDeleteBackup();
  const verifyBackupMutation = useVerifyBackup();
  const restoreBackupMutation = useRestoreBackup();
  const downloadBackupMutation = useDownloadBackup();
  const updateScheduleMutation = useUpdateBackupSchedule();

  const tabs = [
    { id: 'backups' as const, name: 'Backups', icon: HardDrive, badge: backups.length },
    { id: 'schedule' as const, name: 'Schedule', icon: Calendar },
    { id: 'history' as const, name: 'Restore History', icon: History, badge: restores.length > 0 ? restores.length : undefined },
  ];

  const handleCreateBackup = async (data: CreateBackupRequest) => {
    try {
      await createBackupMutation.mutateAsync(data);
      setShowCreateBackup(false);
    } catch {
      // Error handled by mutation
    }
  };

  const handleDeleteBackup = async (id: string) => {
    try {
      await deleteBackupMutation.mutateAsync(id);
      setConfirmAction(null);
    } catch {
      // Error handled by mutation
    }
  };

  const handleVerifyBackup = async (id: string, password: string) => {
    try {
      const result = await verifyBackupMutation.mutateAsync({ id, request: { password } });
      setVerifyingBackup(null);
      // Show result in a toast/alert
      if (result.valid) {
        alert(`Backup verified successfully!\nFiles: ${result.file_count}\nSize: ${result.total_size} bytes`);
      } else {
        alert(`Backup verification failed: ${result.error || 'Unknown error'}`);
      }
    } catch {
      // Error handled by mutation
    }
  };

  const handleRestoreBackup = async (id: string, password: string) => {
    try {
      await restoreBackupMutation.mutateAsync({ id, request: { password, confirm: true } });
      setRestoringBackup(null);
      alert('Backup restored successfully! The server may need to restart.');
    } catch {
      // Error handled by mutation
    }
  };

  const handleDownloadBackup = async (backup: ServerBackup) => {
    try {
      const blob = await downloadBackupMutation.mutateAsync(backup.id);
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = backup.filename;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    } catch {
      // Error handled by mutation
    }
  };

  const handleScheduleUpdate = async (updates: Partial<BackupSchedule>) => {
    try {
      await updateScheduleMutation.mutateAsync(updates);
    } catch {
      // Error handled by mutation
    }
  };

  if (statusLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="w-8 h-8 animate-spin text-blue-500" />
      </div>
    );
  }

  if (!featureStatus?.enabled) {
    return (
      <div className="max-w-4xl mx-auto">
        <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-700 rounded-lg p-6">
          <div className="flex items-start gap-4">
            <AlertTriangle className="w-6 h-6 text-yellow-500 flex-shrink-0 mt-0.5" />
            <div>
              <h3 className="text-lg font-medium text-yellow-800 dark:text-yellow-200">
                Backup Feature Not Enabled
              </h3>
              <p className="mt-2 text-yellow-700 dark:text-yellow-300">
                The backup feature is not enabled in the server configuration. Add a <code className="bg-yellow-100 dark:bg-yellow-900 px-1 rounded">backup:</code> section to your config.yaml to enable this feature.
              </p>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-gray-900 dark:text-white">Server Backup</h1>
          <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
            Backup and restore server data and configuration
          </p>
        </div>
        {activeTab === 'backups' && (
          <button
            onClick={() => setShowCreateBackup(true)}
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors"
          >
            <Plus className="w-4 h-4" />
            Create Backup
          </button>
        )}
      </div>

      {/* Status Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-lg">
              <HardDrive className="w-5 h-5 text-blue-600 dark:text-blue-400" />
            </div>
            <div>
              <p className="text-sm text-gray-500 dark:text-gray-400">Total Backups</p>
              <p className="text-xl font-semibold text-gray-900 dark:text-white">
                {featureStatus.total_backups}
              </p>
            </div>
          </div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-purple-100 dark:bg-purple-900/30 rounded-lg">
              <Database className="w-5 h-5 text-purple-600 dark:text-purple-400" />
            </div>
            <div>
              <p className="text-sm text-gray-500 dark:text-gray-400">Total Size</p>
              <p className="text-xl font-semibold text-gray-900 dark:text-white">
                {featureStatus.total_size_formatted}
              </p>
            </div>
          </div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div className="flex items-center gap-3">
            <div className={clsx(
              "p-2 rounded-lg",
              featureStatus.encryption_enabled
                ? "bg-green-100 dark:bg-green-900/30"
                : "bg-gray-100 dark:bg-gray-700"
            )}>
              {featureStatus.encryption_enabled ? (
                <Lock className="w-5 h-5 text-green-600 dark:text-green-400" />
              ) : (
                <Unlock className="w-5 h-5 text-gray-500 dark:text-gray-400" />
              )}
            </div>
            <div>
              <p className="text-sm text-gray-500 dark:text-gray-400">Encryption</p>
              <p className="text-xl font-semibold text-gray-900 dark:text-white">
                {featureStatus.encryption_enabled ? 'Enabled' : 'Disabled'}
              </p>
            </div>
          </div>
        </div>
        <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
          <div className="flex items-center gap-3">
            <div className={clsx(
              "p-2 rounded-lg",
              featureStatus.schedule_active
                ? "bg-green-100 dark:bg-green-900/30"
                : "bg-gray-100 dark:bg-gray-700"
            )}>
              <Clock className="w-5 h-5 text-gray-500 dark:text-gray-400" />
            </div>
            <div>
              <p className="text-sm text-gray-500 dark:text-gray-400">Schedule</p>
              <p className="text-xl font-semibold text-gray-900 dark:text-white">
                {featureStatus.schedule_active ? 'Active' : 'Inactive'}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-gray-200 dark:border-gray-700">
        <nav className="flex space-x-8">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={clsx(
                  'flex items-center gap-2 py-4 px-1 border-b-2 font-medium text-sm transition-colors',
                  activeTab === tab.id
                    ? 'border-blue-500 text-blue-600 dark:text-blue-400'
                    : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300 dark:text-gray-400 dark:hover:text-gray-300'
                )}
              >
                <Icon className="w-4 h-4" />
                {tab.name}
                {tab.badge !== undefined && (
                  <span className="ml-2 px-2 py-0.5 text-xs rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                    {tab.badge}
                  </span>
                )}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Tab Content */}
      {activeTab === 'backups' && (
        <BackupsTab
          backups={backups}
          loading={backupsLoading}
          onRefresh={refetchBackups}
          onVerify={(backup) => setVerifyingBackup(backup.id)}
          onRestore={(backup) => setRestoringBackup(backup.id)}
          onDownload={handleDownloadBackup}
          onDelete={(backup) => setConfirmAction({ type: 'delete', backup })}
        />
      )}

      {activeTab === 'schedule' && (
        <ScheduleTab
          schedule={schedule}
          loading={scheduleLoading}
          onUpdate={handleScheduleUpdate}
          updating={updateScheduleMutation.isPending}
        />
      )}

      {activeTab === 'history' && (
        <HistoryTab restores={restores} loading={restoresLoading} />
      )}

      {/* Create Backup Modal */}
      {showCreateBackup && (
        <CreateBackupModal
          encryptionEnabled={featureStatus.encryption_enabled}
          onClose={() => setShowCreateBackup(false)}
          onCreate={handleCreateBackup}
          creating={createBackupMutation.isPending}
        />
      )}

      {/* Verify Backup Modal */}
      {verifyingBackup && (
        <PasswordModal
          title="Verify Backup"
          description="Enter the encryption password to verify this backup's integrity."
          actionText="Verify"
          onClose={() => setVerifyingBackup(null)}
          onSubmit={(pwd) => handleVerifyBackup(verifyingBackup, pwd)}
          loading={verifyBackupMutation.isPending}
        />
      )}

      {/* Restore Backup Modal */}
      {restoringBackup && (
        <PasswordModal
          title="Restore Backup"
          description="WARNING: This will replace your current database and configuration. Enter the encryption password to restore this backup."
          actionText="Restore"
          variant="danger"
          onClose={() => setRestoringBackup(null)}
          onSubmit={(pwd) => handleRestoreBackup(restoringBackup, pwd)}
          loading={restoreBackupMutation.isPending}
        />
      )}

      {/* Delete Confirmation Modal */}
      {confirmAction?.type === 'delete' && (
        <ConfirmModal
          title="Delete Backup"
          message={`Are you sure you want to delete "${confirmAction.backup.filename}"? This action cannot be undone.`}
          confirmText="Delete"
          variant="danger"
          onClose={() => setConfirmAction(null)}
          onConfirm={() => handleDeleteBackup(confirmAction.backup.id)}
          loading={deleteBackupMutation.isPending}
        />
      )}
    </div>
  );
}

// ============================================================================
// Backups Tab
// ============================================================================

function BackupsTab({
  backups,
  loading,
  onRefresh,
  onVerify,
  onRestore,
  onDownload,
  onDelete,
}: {
  backups: ServerBackup[];
  loading: boolean;
  onRefresh: () => void;
  onVerify: (backup: ServerBackup) => void;
  onRestore: (backup: ServerBackup) => void;
  onDownload: (backup: ServerBackup) => void;
  onDelete: (backup: ServerBackup) => void;
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="w-8 h-8 animate-spin text-blue-500" />
      </div>
    );
  }

  if (backups.length === 0) {
    return (
      <div className="text-center py-12">
        <HardDrive className="w-12 h-12 mx-auto text-gray-400" />
        <h3 className="mt-4 text-lg font-medium text-gray-900 dark:text-white">No backups yet</h3>
        <p className="mt-2 text-gray-500 dark:text-gray-400">
          Create your first backup to protect your data.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <button
          onClick={() => onRefresh()}
          className="flex items-center gap-2 px-3 py-1.5 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
        >
          <RefreshCw className="w-4 h-4" />
          Refresh
        </button>
      </div>

      <div className="bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
        <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
          <thead className="bg-gray-50 dark:bg-gray-700">
            <tr>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
                Backup
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
                Status
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
                Size
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
                Contents
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
                Created
              </th>
              <th className="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
                Actions
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
            {backups.map((backup) => (
              <tr key={backup.id} className="hover:bg-gray-50 dark:hover:bg-gray-700/50">
                <td className="px-6 py-4">
                  <div className="flex items-center gap-3">
                    {backup.is_encrypted ? (
                      <Lock className="w-4 h-4 text-green-500" />
                    ) : (
                      <Unlock className="w-4 h-4 text-gray-400" />
                    )}
                    <div>
                      <div className="font-medium text-gray-900 dark:text-white">
                        {backup.filename}
                      </div>
                      <div className="text-sm text-gray-500 dark:text-gray-400">
                        {backup.trigger_type === 'manual' ? 'Manual' : 'Scheduled'}
                        {backup.created_by_username && ` by ${backup.created_by_username}`}
                      </div>
                    </div>
                  </div>
                </td>
                <td className="px-6 py-4">
                  <StatusBadge status={backup.status} />
                </td>
                <td className="px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                  {backup.file_size_formatted}
                </td>
                <td className="px-6 py-4">
                  <div className="flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400">
                    {backup.includes_database && (
                      <span className="flex items-center gap-1">
                        <Database className="w-3 h-3" /> DB
                      </span>
                    )}
                    {backup.includes_config && (
                      <span className="flex items-center gap-1">
                        <FileText className="w-3 h-3" /> Config
                      </span>
                    )}
                  </div>
                </td>
                <td className="px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                  {formatDate(backup.created_at)}
                </td>
                <td className="px-6 py-4 text-right">
                  <div className="flex items-center justify-end gap-2">
                    {backup.status === 'completed' && (
                      <>
                        <button
                          onClick={() => onVerify(backup)}
                          className="p-1.5 text-gray-400 hover:text-blue-600 dark:hover:text-blue-400 transition-colors"
                          title="Verify backup"
                        >
                          <Shield className="w-4 h-4" />
                        </button>
                        <button
                          onClick={() => onDownload(backup)}
                          className="p-1.5 text-gray-400 hover:text-green-600 dark:hover:text-green-400 transition-colors"
                          title="Download backup"
                        >
                          <Download className="w-4 h-4" />
                        </button>
                        <button
                          onClick={() => onRestore(backup)}
                          className="p-1.5 text-gray-400 hover:text-yellow-600 dark:hover:text-yellow-400 transition-colors"
                          title="Restore backup"
                        >
                          <RotateCcw className="w-4 h-4" />
                        </button>
                      </>
                    )}
                    <button
                      onClick={() => onDelete(backup)}
                      className="p-1.5 text-gray-400 hover:text-red-600 dark:hover:text-red-400 transition-colors"
                      title="Delete backup"
                    >
                      <Trash2 className="w-4 h-4" />
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

// ============================================================================
// Schedule Tab
// ============================================================================

function ScheduleTab({
  schedule,
  loading,
  onUpdate,
  updating,
}: {
  schedule: BackupSchedule | null | undefined;
  loading: boolean;
  onUpdate: (updates: Partial<BackupSchedule>) => void;
  updating: boolean;
}) {
  const [editMode, setEditMode] = useState(false);
  const [formData, setFormData] = useState({
    is_active: schedule?.is_active ?? false,
    frequency: schedule?.frequency ?? 'daily',
    time_of_day: schedule?.time_of_day ?? '02:00',
    day_of_week: schedule?.day_of_week ?? 0,
    retention_count: schedule?.retention_count ?? 7,
  });

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="w-8 h-8 animate-spin text-blue-500" />
      </div>
    );
  }

  if (!schedule) {
    return (
      <div className="text-center py-12">
        <Calendar className="w-12 h-12 mx-auto text-gray-400" />
        <h3 className="mt-4 text-lg font-medium text-gray-900 dark:text-white">No schedule configured</h3>
        <p className="mt-2 text-gray-500 dark:text-gray-400">
          Configure a backup schedule in your server configuration.
        </p>
      </div>
    );
  }

  const handleSave = () => {
    onUpdate(formData);
    setEditMode(false);
  };

  const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-6">
        <h3 className="text-lg font-medium text-gray-900 dark:text-white">Backup Schedule</h3>
        {!editMode ? (
          <button
            onClick={() => setEditMode(true)}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors"
          >
            Edit Schedule
          </button>
        ) : (
          <div className="flex gap-2">
            <button
              onClick={() => setEditMode(false)}
              className="px-4 py-2 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleSave}
              disabled={updating}
              className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 transition-colors flex items-center gap-2"
            >
              {updating && <Loader2 className="w-4 h-4 animate-spin" />}
              Save Changes
            </button>
          </div>
        )}
      </div>

      <div className="space-y-6">
        {/* Active Toggle */}
        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm font-medium text-gray-900 dark:text-white">Schedule Active</label>
            <p className="text-sm text-gray-500 dark:text-gray-400">Enable automatic backups</p>
          </div>
          <button
            onClick={() => editMode && setFormData({ ...formData, is_active: !formData.is_active })}
            disabled={!editMode}
            className={clsx(
              'relative inline-flex h-6 w-11 items-center rounded-full transition-colors',
              editMode ? 'cursor-pointer' : 'cursor-default',
              formData.is_active ? 'bg-blue-600' : 'bg-gray-200 dark:bg-gray-700'
            )}
          >
            <span
              className={clsx(
                'inline-block h-4 w-4 transform rounded-full bg-white transition-transform',
                formData.is_active ? 'translate-x-6' : 'translate-x-1'
              )}
            />
          </button>
        </div>

        {/* Frequency */}
        <div>
          <label className="block text-sm font-medium text-gray-900 dark:text-white mb-2">
            Frequency
          </label>
          <select
            value={formData.frequency}
            onChange={(e) => setFormData({ ...formData, frequency: e.target.value })}
            disabled={!editMode}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white disabled:opacity-50"
          >
            <option value="hourly">Hourly</option>
            <option value="daily">Daily</option>
            <option value="weekly">Weekly</option>
          </select>
        </div>

        {/* Time of Day (for daily/weekly) */}
        {(formData.frequency === 'daily' || formData.frequency === 'weekly') && (
          <div>
            <label className="block text-sm font-medium text-gray-900 dark:text-white mb-2">
              Time of Day (UTC)
            </label>
            <input
              type="time"
              value={formData.time_of_day}
              onChange={(e) => setFormData({ ...formData, time_of_day: e.target.value })}
              disabled={!editMode}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white disabled:opacity-50"
            />
          </div>
        )}

        {/* Day of Week (for weekly) */}
        {formData.frequency === 'weekly' && (
          <div>
            <label className="block text-sm font-medium text-gray-900 dark:text-white mb-2">
              Day of Week
            </label>
            <select
              value={formData.day_of_week}
              onChange={(e) => setFormData({ ...formData, day_of_week: parseInt(e.target.value) })}
              disabled={!editMode}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white disabled:opacity-50"
            >
              {days.map((day, idx) => (
                <option key={day} value={idx}>{day}</option>
              ))}
            </select>
          </div>
        )}

        {/* Retention Count */}
        <div>
          <label className="block text-sm font-medium text-gray-900 dark:text-white mb-2">
            Retention Count
          </label>
          <p className="text-sm text-gray-500 dark:text-gray-400 mb-2">
            Number of backups to keep before automatic cleanup
          </p>
          <input
            type="number"
            min="1"
            max="100"
            value={formData.retention_count}
            onChange={(e) => setFormData({ ...formData, retention_count: parseInt(e.target.value) })}
            disabled={!editMode}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white disabled:opacity-50"
          />
        </div>

        {/* Schedule Info */}
        <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-gray-500 dark:text-gray-400">Last Run:</span>
              <span className="ml-2 text-gray-900 dark:text-white">
                {schedule.last_run_at ? formatDate(schedule.last_run_at) : 'Never'}
              </span>
            </div>
            <div>
              <span className="text-gray-500 dark:text-gray-400">Next Run:</span>
              <span className="ml-2 text-gray-900 dark:text-white">
                {schedule.next_run_at ? formatDate(schedule.next_run_at) : 'Not scheduled'}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// History Tab
// ============================================================================

function HistoryTab({
  restores,
  loading,
}: {
  restores: BackupRestore[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="w-8 h-8 animate-spin text-blue-500" />
      </div>
    );
  }

  if (restores.length === 0) {
    return (
      <div className="text-center py-12">
        <History className="w-12 h-12 mx-auto text-gray-400" />
        <h3 className="mt-4 text-lg font-medium text-gray-900 dark:text-white">No restore history</h3>
        <p className="mt-2 text-gray-500 dark:text-gray-400">
          Restore operations will appear here.
        </p>
      </div>
    );
  }

  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow overflow-hidden">
      <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
        <thead className="bg-gray-50 dark:bg-gray-700">
          <tr>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
              Backup
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
              Status
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
              Restored By
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
              Date
            </th>
            <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-300 uppercase tracking-wider">
              Duration
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
          {restores.map((restore) => (
            <tr key={restore.id} className="hover:bg-gray-50 dark:hover:bg-gray-700/50">
              <td className="px-6 py-4 text-sm text-gray-900 dark:text-white">
                {restore.backup_filename || restore.backup_id}
              </td>
              <td className="px-6 py-4">
                <StatusBadge status={restore.status} />
              </td>
              <td className="px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                {restore.restored_by_username || 'Unknown'}
              </td>
              <td className="px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                {formatDate(restore.created_at)}
              </td>
              <td className="px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                {restore.duration_seconds ? `${restore.duration_seconds}s` : '-'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ============================================================================
// Create Backup Modal
// ============================================================================

function CreateBackupModal({
  encryptionEnabled,
  onClose,
  onCreate,
  creating,
}: {
  encryptionEnabled: boolean;
  onClose: () => void;
  onCreate: (data: CreateBackupRequest) => void;
  creating: boolean;
}) {
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [notes, setNotes] = useState('');
  const [includeDatabase, setIncludeDatabase] = useState(true);
  const [includeConfig, setIncludeConfig] = useState(true);

  const canSubmit = (!encryptionEnabled || (password && password === confirmPassword)) &&
                   (includeDatabase || includeConfig);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;

    onCreate({
      password: encryptionEnabled ? password : undefined,
      notes: notes || undefined,
      include_database: includeDatabase,
      include_config: includeConfig,
    });
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4">
        <div className="p-6">
          <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-4">
            Create Backup
          </h3>

          <form onSubmit={handleSubmit} className="space-y-4">
            {/* What to include */}
            <div>
              <label className="block text-sm font-medium text-gray-900 dark:text-white mb-2">
                Include in Backup
              </label>
              <div className="space-y-2">
                <label className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={includeDatabase}
                    onChange={(e) => setIncludeDatabase(e.target.checked)}
                    className="rounded border-gray-300 dark:border-gray-600"
                  />
                  <Database className="w-4 h-4 text-gray-500" />
                  <span className="text-sm text-gray-700 dark:text-gray-300">Database</span>
                </label>
                <label className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={includeConfig}
                    onChange={(e) => setIncludeConfig(e.target.checked)}
                    className="rounded border-gray-300 dark:border-gray-600"
                  />
                  <FileText className="w-4 h-4 text-gray-500" />
                  <span className="text-sm text-gray-700 dark:text-gray-300">Configuration Files</span>
                </label>
              </div>
            </div>

            {/* Password fields (if encryption enabled) */}
            {encryptionEnabled && (
              <>
                <div>
                  <label className="block text-sm font-medium text-gray-900 dark:text-white mb-1">
                    Encryption Password
                  </label>
                  <input
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="Enter password"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-900 dark:text-white mb-1">
                    Confirm Password
                  </label>
                  <input
                    type="password"
                    value={confirmPassword}
                    onChange={(e) => setConfirmPassword(e.target.value)}
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="Confirm password"
                  />
                  {password && confirmPassword && password !== confirmPassword && (
                    <p className="mt-1 text-sm text-red-500">Passwords do not match</p>
                  )}
                </div>
              </>
            )}

            {/* Notes */}
            <div>
              <label className="block text-sm font-medium text-gray-900 dark:text-white mb-1">
                Notes (optional)
              </label>
              <textarea
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                rows={2}
                className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                placeholder="Add a note about this backup"
              />
            </div>

            {/* Actions */}
            <div className="flex justify-end gap-3 pt-4">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={!canSubmit || creating}
                className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 transition-colors flex items-center gap-2"
              >
                {creating && <Loader2 className="w-4 h-4 animate-spin" />}
                Create Backup
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Password Modal
// ============================================================================

function PasswordModal({
  title,
  description,
  actionText,
  variant = 'default',
  onClose,
  onSubmit,
  loading,
}: {
  title: string;
  description: string;
  actionText: string;
  variant?: 'default' | 'danger';
  onClose: () => void;
  onSubmit: (password: string) => void;
  loading: boolean;
}) {
  const [password, setPassword] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (password) {
      onSubmit(password);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4">
        <div className="p-6">
          <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-2">
            {title}
          </h3>
          <p className={clsx(
            "text-sm mb-4",
            variant === 'danger' ? "text-red-600 dark:text-red-400" : "text-gray-500 dark:text-gray-400"
          )}>
            {description}
          </p>

          <form onSubmit={handleSubmit}>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white mb-4"
              placeholder="Enter encryption password"
              autoFocus
            />

            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={!password || loading}
                className={clsx(
                  "px-4 py-2 text-sm text-white rounded-lg disabled:opacity-50 transition-colors flex items-center gap-2",
                  variant === 'danger' ? "bg-red-600 hover:bg-red-700" : "bg-blue-600 hover:bg-blue-700"
                )}
              >
                {loading && <Loader2 className="w-4 h-4 animate-spin" />}
                {actionText}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Confirm Modal
// ============================================================================

function ConfirmModal({
  title,
  message,
  confirmText,
  variant = 'default',
  onClose,
  onConfirm,
  loading,
}: {
  title: string;
  message: string;
  confirmText: string;
  variant?: 'default' | 'danger';
  onClose: () => void;
  onConfirm: () => void;
  loading: boolean;
}) {
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-md w-full mx-4">
        <div className="p-6">
          <h3 className="text-lg font-medium text-gray-900 dark:text-white mb-2">
            {title}
          </h3>
          <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">
            {message}
          </p>

          <div className="flex justify-end gap-3">
            <button
              onClick={onClose}
              className="px-4 py-2 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={onConfirm}
              disabled={loading}
              className={clsx(
                "px-4 py-2 text-sm text-white rounded-lg disabled:opacity-50 transition-colors flex items-center gap-2",
                variant === 'danger' ? "bg-red-600 hover:bg-red-700" : "bg-blue-600 hover:bg-blue-700"
              )}
            >
              {loading && <Loader2 className="w-4 h-4 animate-spin" />}
              {confirmText}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Helper Components
// ============================================================================

function StatusBadge({ status }: { status: BackupStatus }) {
  const config = {
    pending: { icon: Clock, color: 'bg-gray-100 text-gray-700 dark:bg-gray-700 dark:text-gray-300' },
    in_progress: { icon: Loader2, color: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400' },
    completed: { icon: CheckCircle2, color: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400' },
    failed: { icon: XCircle, color: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400' },
    deleted: { icon: Trash2, color: 'bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400' },
  }[status] || { icon: AlertTriangle, color: 'bg-yellow-100 text-yellow-700' };

  const Icon = config.icon;
  const animate = status === 'in_progress';

  return (
    <span className={clsx('inline-flex items-center gap-1.5 px-2 py-1 text-xs font-medium rounded-full', config.color)}>
      <Icon className={clsx('w-3 h-3', animate && 'animate-spin')} />
      {status.replace('_', ' ')}
    </span>
  );
}

function formatDate(dateString: string): string {
  const date = new Date(dateString);
  return date.toLocaleString();
}
