import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Bell,
  BellOff,
  Plus,
  Trash2,
  Check,
  X,
  AlertTriangle,
  Info,
  AlertCircle,
  RefreshCw,
  Play,
  Settings,
  Send,
  Clock,
  Volume2,
  VolumeX,
  Pencil,
} from 'lucide-react';
import { api } from '../services/api';
import type {
  Alert,
  AlertRule,
  NotificationChannel,
  AlertSeverity,
  AlertStatus,
  AlertRuleType,
  ChannelType,
  CreateChannelRequest,
  CreateAlertRuleRequest,
  UpdateAlertRuleRequest,
  CreateSilenceRequest,
  AlertCondition,
} from '../types';

type TabId = 'alerts' | 'rules' | 'channels' | 'silences';

const TABS: { id: TabId; label: string; icon: React.ComponentType<{ className?: string }> }[] = [
  { id: 'alerts', label: 'Active Alerts', icon: Bell },
  { id: 'rules', label: 'Alert Rules', icon: Settings },
  { id: 'channels', label: 'Notification Channels', icon: Send },
  { id: 'silences', label: 'Silences', icon: BellOff },
];

const SEVERITY_COLORS: Record<AlertSeverity, string> = {
  info: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200',
  warning: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200',
  critical: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
};

const STATUS_COLORS: Record<AlertStatus, string> = {
  active: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
  acknowledged: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200',
  resolved: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  silenced: 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300',
};

const RULE_TYPE_LABELS: Record<AlertRuleType, string> = {
  node_status: 'Node Status',
  compliance: 'Compliance',
  drift: 'Drift Detection',
  report_failure: 'Report Failure',
  custom: 'Custom',
};

const CHANNEL_TYPE_LABELS: Record<ChannelType, string> = {
  webhook: 'Webhook',
  email: 'Email',
  slack: 'Slack',
  teams: 'Microsoft Teams',
};

function SeverityIcon({ severity }: { severity: AlertSeverity }) {
  const className = 'h-5 w-5';
  switch (severity) {
    case 'critical':
      return <AlertCircle className={`${className} text-red-500`} />;
    case 'warning':
      return <AlertTriangle className={`${className} text-yellow-500`} />;
    default:
      return <Info className={`${className} text-blue-500`} />;
  }
}

function formatDate(dateString: string | null | undefined): string {
  if (!dateString) return 'Never';
  return new Date(dateString).toLocaleString();
}

function formatRelativeTime(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (minutes < 1) return 'Just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return `${days}d ago`;
}

export default function Alerting() {
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<TabId>('alerts');
  const [showNewChannelModal, setShowNewChannelModal] = useState(false);
  const [showNewRuleModal, setShowNewRuleModal] = useState(false);
  const [showNewSilenceModal, setShowNewSilenceModal] = useState(false);
  const [_selectedAlert, _setSelectedAlert] = useState<Alert | null>(null);
  const [editingRule, setEditingRule] = useState<AlertRule | null>(null);

  // Queries
  const { data: alerts = [], isLoading: alertsLoading, refetch: refetchAlerts } = useQuery({
    queryKey: ['alerts'],
    queryFn: () => api.getAlerts(),
  });

  const { data: alertStats } = useQuery({
    queryKey: ['alertStats'],
    queryFn: api.getAlertStats,
  });

  const { data: rules = [], isLoading: rulesLoading } = useQuery({
    queryKey: ['alertRules'],
    queryFn: () => api.getRules(),
  });

  const { data: channels = [], isLoading: channelsLoading } = useQuery({
    queryKey: ['channels'],
    queryFn: api.getChannels,
  });

  const { data: silences = [], isLoading: silencesLoading } = useQuery({
    queryKey: ['silences'],
    queryFn: api.getSilences,
  });

  // Mutations
  const acknowledgeMutation = useMutation({
    mutationFn: api.acknowledgeAlert,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alerts'] });
      queryClient.invalidateQueries({ queryKey: ['alertStats'] });
    },
  });

  const resolveMutation = useMutation({
    mutationFn: api.resolveAlert,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alerts'] });
      queryClient.invalidateQueries({ queryKey: ['alertStats'] });
    },
  });

  const silenceAlertMutation = useMutation({
    mutationFn: api.silenceAlert,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alerts'] });
      queryClient.invalidateQueries({ queryKey: ['alertStats'] });
    },
  });

  const deleteChannelMutation = useMutation({
    mutationFn: api.deleteChannel,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['channels'] }),
  });

  const deleteRuleMutation = useMutation({
    mutationFn: api.deleteRule,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['alertRules'] }),
  });

  const deleteSilenceMutation = useMutation({
    mutationFn: api.deleteSilence,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['silences'] }),
  });

  const testChannelMutation = useMutation({
    mutationFn: (id: string) => api.testChannel(id),
  });

  const evaluateMutation = useMutation({
    mutationFn: api.evaluateRules,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alerts'] });
      queryClient.invalidateQueries({ queryKey: ['alertStats'] });
    },
  });

  const getRuleName = (ruleId: string) => {
    const rule = rules.find((r) => r.id === ruleId);
    return rule?.name || 'Unknown Rule';
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-gray-900 dark:text-gray-100">
            Alerting & Notifications
          </h1>
          <p className="text-sm text-gray-500 dark:text-gray-400">
            Monitor alerts and configure notification channels
          </p>
        </div>
        <div className="flex items-center space-x-3">
          <button
            onClick={() => evaluateMutation.mutate()}
            disabled={evaluateMutation.isPending}
            className="flex items-center space-x-2 rounded-md bg-green-600 px-4 py-2 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50"
          >
            <Play className="h-4 w-4" />
            <span>Evaluate Rules</span>
          </button>
          <button
            onClick={() => refetchAlerts()}
            className="flex items-center space-x-2 rounded-md bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 dark:bg-gray-700 dark:text-gray-300 dark:hover:bg-gray-600"
          >
            <RefreshCw className="h-4 w-4" />
            <span>Refresh</span>
          </button>
        </div>
      </div>

      {/* Stats Cards */}
      {alertStats && (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <div className="rounded-lg bg-white p-6 shadow dark:bg-gray-800">
            <div className="flex items-center">
              <div className="rounded-full bg-red-100 p-3 dark:bg-red-900">
                <AlertCircle className="h-6 w-6 text-red-600 dark:text-red-400" />
              </div>
              <div className="ml-4">
                <p className="text-sm font-medium text-gray-500 dark:text-gray-400">Active Alerts</p>
                <p className="text-2xl font-semibold text-gray-900 dark:text-gray-100">
                  {alertStats.total_active}
                </p>
              </div>
            </div>
          </div>
          <div className="rounded-lg bg-white p-6 shadow dark:bg-gray-800">
            <div className="flex items-center">
              <div className="rounded-full bg-red-100 p-3 dark:bg-red-900">
                <AlertCircle className="h-6 w-6 text-red-600 dark:text-red-400" />
              </div>
              <div className="ml-4">
                <p className="text-sm font-medium text-gray-500 dark:text-gray-400">Critical</p>
                <p className="text-2xl font-semibold text-red-600 dark:text-red-400">
                  {alertStats.by_severity.critical}
                </p>
              </div>
            </div>
          </div>
          <div className="rounded-lg bg-white p-6 shadow dark:bg-gray-800">
            <div className="flex items-center">
              <div className="rounded-full bg-yellow-100 p-3 dark:bg-yellow-900">
                <AlertTriangle className="h-6 w-6 text-yellow-600 dark:text-yellow-400" />
              </div>
              <div className="ml-4">
                <p className="text-sm font-medium text-gray-500 dark:text-gray-400">Warning</p>
                <p className="text-2xl font-semibold text-yellow-600 dark:text-yellow-400">
                  {alertStats.by_severity.warning}
                </p>
              </div>
            </div>
          </div>
          <div className="rounded-lg bg-white p-6 shadow dark:bg-gray-800">
            <div className="flex items-center">
              <div className="rounded-full bg-blue-100 p-3 dark:bg-blue-900">
                <Clock className="h-6 w-6 text-blue-600 dark:text-blue-400" />
              </div>
              <div className="ml-4">
                <p className="text-sm font-medium text-gray-500 dark:text-gray-400">Today</p>
                <p className="text-2xl font-semibold text-gray-900 dark:text-gray-100">
                  {alertStats.total_today}
                </p>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Tabs */}
      <div className="border-b border-gray-200 dark:border-gray-700">
        <nav className="-mb-px flex space-x-8">
          {TABS.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center space-x-2 border-b-2 px-1 py-4 text-sm font-medium ${
                  activeTab === tab.id
                    ? 'border-blue-500 text-blue-600 dark:border-blue-400 dark:text-blue-400'
                    : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300'
                }`}
              >
                <Icon className="h-4 w-4" />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </nav>
      </div>

      {/* Tab Content */}
      <div className="rounded-lg bg-white shadow dark:bg-gray-800">
        {/* Alerts Tab */}
        {activeTab === 'alerts' && (
          <div className="p-6">
            {alertsLoading ? (
              <div className="flex items-center justify-center py-12">
                <RefreshCw className="h-8 w-8 animate-spin text-gray-400" />
              </div>
            ) : alerts.length === 0 ? (
              <div className="py-12 text-center">
                <Bell className="mx-auto h-12 w-12 text-gray-400" />
                <h3 className="mt-2 text-sm font-medium text-gray-900 dark:text-gray-100">
                  No active alerts
                </h3>
                <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
                  All systems are operating normally.
                </p>
              </div>
            ) : (
              <div className="space-y-4">
                {alerts.map((alert) => (
                  <div
                    key={alert.id}
                    className="flex items-start justify-between rounded-lg border border-gray-200 p-4 dark:border-gray-700"
                  >
                    <div className="flex items-start space-x-4">
                      <SeverityIcon severity={alert.severity} />
                      <div>
                        <h4 className="font-medium text-gray-900 dark:text-gray-100">
                          {alert.title}
                        </h4>
                        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
                          {alert.message}
                        </p>
                        <div className="mt-2 flex items-center space-x-4 text-xs text-gray-500 dark:text-gray-400">
                          <span>Rule: {getRuleName(alert.rule_id)}</span>
                          <span>Triggered: {formatRelativeTime(alert.triggered_at)}</span>
                          <span
                            className={`rounded-full px-2 py-0.5 text-xs font-medium ${STATUS_COLORS[alert.status]}`}
                          >
                            {alert.status}
                          </span>
                          <span
                            className={`rounded-full px-2 py-0.5 text-xs font-medium ${SEVERITY_COLORS[alert.severity]}`}
                          >
                            {alert.severity}
                          </span>
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center space-x-2">
                      {alert.status === 'active' && (
                        <>
                          <button
                            onClick={() => acknowledgeMutation.mutate(alert.id)}
                            disabled={acknowledgeMutation.isPending}
                            className="rounded-md bg-yellow-100 p-2 text-yellow-700 hover:bg-yellow-200 dark:bg-yellow-900 dark:text-yellow-300"
                            title="Acknowledge"
                          >
                            <Check className="h-4 w-4" />
                          </button>
                          <button
                            onClick={() => silenceAlertMutation.mutate(alert.id)}
                            disabled={silenceAlertMutation.isPending}
                            className="rounded-md bg-gray-100 p-2 text-gray-700 hover:bg-gray-200 dark:bg-gray-700 dark:text-gray-300"
                            title="Silence"
                          >
                            <VolumeX className="h-4 w-4" />
                          </button>
                        </>
                      )}
                      {(alert.status === 'active' || alert.status === 'acknowledged') && (
                        <button
                          onClick={() => resolveMutation.mutate(alert.id)}
                          disabled={resolveMutation.isPending}
                          className="rounded-md bg-green-100 p-2 text-green-700 hover:bg-green-200 dark:bg-green-900 dark:text-green-300"
                          title="Resolve"
                        >
                          <X className="h-4 w-4" />
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Rules Tab */}
        {activeTab === 'rules' && (
          <div className="p-6">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">Alert Rules</h3>
              <button
                onClick={() => setShowNewRuleModal(true)}
                className="flex items-center space-x-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
              >
                <Plus className="h-4 w-4" />
                <span>New Rule</span>
              </button>
            </div>
            {rulesLoading ? (
              <div className="flex items-center justify-center py-12">
                <RefreshCw className="h-8 w-8 animate-spin text-gray-400" />
              </div>
            ) : rules.length === 0 ? (
              <div className="py-12 text-center">
                <Settings className="mx-auto h-12 w-12 text-gray-400" />
                <h3 className="mt-2 text-sm font-medium text-gray-900 dark:text-gray-100">
                  No alert rules
                </h3>
                <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
                  Create your first alert rule to start monitoring.
                </p>
              </div>
            ) : (
              <div className="overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700">
                <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                  <thead className="bg-gray-50 dark:bg-gray-900">
                    <tr>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Name
                      </th>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Type
                      </th>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Severity
                      </th>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Channels
                      </th>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Status
                      </th>
                      <th className="px-6 py-3 text-right text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Actions
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-200 bg-white dark:divide-gray-700 dark:bg-gray-800">
                    {rules.map((rule) => (
                      <tr key={rule.id}>
                        <td className="whitespace-nowrap px-6 py-4">
                          <div>
                            <div className="font-medium text-gray-900 dark:text-gray-100">
                              {rule.name}
                            </div>
                            {rule.description && (
                              <div className="text-sm text-gray-500 dark:text-gray-400">
                                {rule.description}
                              </div>
                            )}
                          </div>
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                          {RULE_TYPE_LABELS[rule.rule_type]}
                        </td>
                        <td className="whitespace-nowrap px-6 py-4">
                          <span
                            className={`rounded-full px-2 py-1 text-xs font-medium ${SEVERITY_COLORS[rule.severity]}`}
                          >
                            {rule.severity}
                          </span>
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                          {rule.channels.length} channel(s)
                        </td>
                        <td className="whitespace-nowrap px-6 py-4">
                          <span
                            className={`inline-flex items-center rounded-full px-2 py-1 text-xs font-medium ${
                              rule.is_enabled
                                ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
                                : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300'
                            }`}
                          >
                            {rule.is_enabled ? 'Enabled' : 'Disabled'}
                          </span>
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 text-right">
                          <div className="flex items-center justify-end space-x-2">
                            <button
                              onClick={() => setEditingRule(rule)}
                              className="text-blue-600 hover:text-blue-900 dark:text-blue-400"
                              title="Edit rule"
                            >
                              <Pencil className="h-4 w-4" />
                            </button>
                            <button
                              onClick={() => deleteRuleMutation.mutate(rule.id)}
                              disabled={deleteRuleMutation.isPending}
                              className="text-red-600 hover:text-red-900 dark:text-red-400"
                              title="Delete rule"
                            >
                              <Trash2 className="h-4 w-4" />
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}

        {/* Channels Tab */}
        {activeTab === 'channels' && (
          <div className="p-6">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
                Notification Channels
              </h3>
              <button
                onClick={() => setShowNewChannelModal(true)}
                className="flex items-center space-x-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
              >
                <Plus className="h-4 w-4" />
                <span>New Channel</span>
              </button>
            </div>
            {channelsLoading ? (
              <div className="flex items-center justify-center py-12">
                <RefreshCw className="h-8 w-8 animate-spin text-gray-400" />
              </div>
            ) : channels.length === 0 ? (
              <div className="py-12 text-center">
                <Send className="mx-auto h-12 w-12 text-gray-400" />
                <h3 className="mt-2 text-sm font-medium text-gray-900 dark:text-gray-100">
                  No notification channels
                </h3>
                <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
                  Create a channel to receive alert notifications.
                </p>
              </div>
            ) : (
              <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                {channels.map((channel) => (
                  <div
                    key={channel.id}
                    className="rounded-lg border border-gray-200 p-4 dark:border-gray-700"
                  >
                    <div className="flex items-start justify-between">
                      <div>
                        <h4 className="font-medium text-gray-900 dark:text-gray-100">
                          {channel.name}
                        </h4>
                        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
                          {CHANNEL_TYPE_LABELS[channel.channel_type]}
                        </p>
                      </div>
                      <span
                        className={`inline-flex items-center rounded-full px-2 py-1 text-xs font-medium ${
                          channel.is_enabled
                            ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
                            : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300'
                        }`}
                      >
                        {channel.is_enabled ? 'Enabled' : 'Disabled'}
                      </span>
                    </div>
                    <div className="mt-4 flex items-center space-x-2">
                      <button
                        onClick={() => testChannelMutation.mutate(channel.id)}
                        disabled={testChannelMutation.isPending}
                        className="flex items-center space-x-1 rounded-md bg-gray-100 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-200 dark:bg-gray-700 dark:text-gray-300"
                      >
                        <Volume2 className="h-3 w-3" />
                        <span>Test</span>
                      </button>
                      <button
                        onClick={() => deleteChannelMutation.mutate(channel.id)}
                        disabled={deleteChannelMutation.isPending}
                        className="rounded-md p-1.5 text-red-600 hover:bg-red-100 dark:text-red-400"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Silences Tab */}
        {activeTab === 'silences' && (
          <div className="p-6">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
                Alert Silences
              </h3>
              <button
                onClick={() => setShowNewSilenceModal(true)}
                className="flex items-center space-x-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
              >
                <Plus className="h-4 w-4" />
                <span>New Silence</span>
              </button>
            </div>
            {silencesLoading ? (
              <div className="flex items-center justify-center py-12">
                <RefreshCw className="h-8 w-8 animate-spin text-gray-400" />
              </div>
            ) : silences.length === 0 ? (
              <div className="py-12 text-center">
                <BellOff className="mx-auto h-12 w-12 text-gray-400" />
                <h3 className="mt-2 text-sm font-medium text-gray-900 dark:text-gray-100">
                  No silences
                </h3>
                <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
                  Create a silence to temporarily suppress alerts.
                </p>
              </div>
            ) : (
              <div className="overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700">
                <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                  <thead className="bg-gray-50 dark:bg-gray-900">
                    <tr>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Rule
                      </th>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Reason
                      </th>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Starts
                      </th>
                      <th className="px-6 py-3 text-left text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Ends
                      </th>
                      <th className="px-6 py-3 text-right text-xs font-medium uppercase tracking-wider text-gray-500 dark:text-gray-400">
                        Actions
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-200 bg-white dark:divide-gray-700 dark:bg-gray-800">
                    {silences.map((silence) => (
                      <tr key={silence.id}>
                        <td className="whitespace-nowrap px-6 py-4 text-sm text-gray-900 dark:text-gray-100">
                          {silence.rule_id ? getRuleName(silence.rule_id) : 'All rules'}
                        </td>
                        <td className="px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                          {silence.reason}
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                          {formatDate(silence.starts_at)}
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 text-sm text-gray-500 dark:text-gray-400">
                          {formatDate(silence.ends_at)}
                        </td>
                        <td className="whitespace-nowrap px-6 py-4 text-right">
                          <button
                            onClick={() => deleteSilenceMutation.mutate(silence.id)}
                            disabled={deleteSilenceMutation.isPending}
                            className="text-red-600 hover:text-red-900 dark:text-red-400"
                          >
                            <Trash2 className="h-4 w-4" />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Modals would go here - simplified for now */}
      {showNewChannelModal && (
        <NewChannelModal onClose={() => setShowNewChannelModal(false)} />
      )}
      {showNewRuleModal && (
        <RuleModal channels={channels} onClose={() => setShowNewRuleModal(false)} />
      )}
      {editingRule && (
        <RuleModal
          channels={channels}
          rule={editingRule}
          onClose={() => setEditingRule(null)}
        />
      )}
      {showNewSilenceModal && (
        <NewSilenceModal rules={rules} onClose={() => setShowNewSilenceModal(false)} />
      )}
    </div>
  );
}

// Simple modal components
function NewChannelModal({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState('');
  const [channelType, setChannelType] = useState<ChannelType>('webhook');
  const [webhookUrl, setWebhookUrl] = useState('');

  const createMutation = useMutation({
    mutationFn: api.createChannel,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['channels'] });
      onClose();
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const request: CreateChannelRequest = {
      name,
      channel_type: channelType,
      config: channelType === 'webhook' ? { url: webhookUrl } : { webhook_url: webhookUrl },
    };
    createMutation.mutate(request);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black bg-opacity-50">
      <div className="w-full max-w-md rounded-lg bg-white p-6 dark:bg-gray-800">
        <h3 className="mb-4 text-lg font-medium text-gray-900 dark:text-gray-100">
          New Notification Channel
        </h3>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              Name
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
              required
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              Type
            </label>
            <select
              value={channelType}
              onChange={(e) => setChannelType(e.target.value as ChannelType)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
            >
              <option value="webhook">Webhook</option>
              <option value="slack">Slack</option>
              <option value="teams">Microsoft Teams</option>
              <option value="email">Email</option>
            </select>
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              {channelType === 'webhook' ? 'Webhook URL' : 'Webhook/SMTP URL'}
            </label>
            <input
              type="url"
              value={webhookUrl}
              onChange={(e) => setWebhookUrl(e.target.value)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
              required
            />
          </div>
          <div className="flex justify-end space-x-3">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 dark:bg-gray-700 dark:text-gray-300"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={createMutation.isPending}
              className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            >
              Create
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function RuleModal({
  channels,
  rule,
  onClose,
}: {
  channels: NotificationChannel[];
  rule?: AlertRule;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const isEditing = !!rule;

  const [name, setName] = useState(rule?.name || '');
  const [description, setDescription] = useState(rule?.description || '');
  const [ruleType, setRuleType] = useState<AlertRuleType>(rule?.rule_type || 'node_status');
  const [severity, setSeverity] = useState<AlertSeverity>(rule?.severity || 'warning');
  const [isEnabled, setIsEnabled] = useState(rule?.is_enabled ?? true);
  const [selectedChannels, setSelectedChannels] = useState<string[]>(rule?.channels || []);
  const [useAdvancedFormat, setUseAdvancedFormat] = useState(false);
  const [conditions, setConditions] = useState<AlertCondition[]>(
    rule?.conditions || [
      { field: 'node.status', operator: 'eq', value: 'failed' },
    ]
  );
  const [conditionOperator, setConditionOperator] = useState<'all' | 'any'>(
    rule?.condition_operator || 'all'
  );

  const createMutation = useMutation({
    mutationFn: api.createRule,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alertRules'] });
      onClose();
    },
    onError: (error: any) => {
      console.error('Failed to create alert rule:', error);
      console.error('Error response:', error.response?.data);
      const errorMsg = error.response?.data?.error || error.response?.data?.message || error.message || 'Unknown error';
      alert(`Failed to create alert rule: ${errorMsg}`);
    },
  });

  const updateMutation = useMutation({
    mutationFn: (request: UpdateAlertRuleRequest) => api.updateRule(rule!.id, request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alertRules'] });
      onClose();
    },
    onError: (error: any) => {
      console.error('Failed to update alert rule:', error);
      console.error('Error response:', error.response?.data);
      const errorMsg = error.response?.data?.error || error.response?.data?.message || error.message || 'Unknown error';
      alert(`Failed to update alert rule: ${errorMsg}`);
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    
    // Validate conditions
    const invalidConditions = conditions.map((c, idx) => {
      const errors: string[] = [];
      if (useAdvancedFormat) {
        if (!c.type) errors.push('type');
        if (!c.operator) errors.push('operator');
      } else {
        if (!c.field) errors.push('field');
        if (!c.operator) errors.push('operator');
        if (c.value === undefined || c.value === '') errors.push('value');
      }
      return errors.length > 0 ? { index: idx + 1, errors } : null;
    }).filter(Boolean);
    
    if (invalidConditions.length > 0) {
      const errorMsg = invalidConditions.map(item => 
        `Condition ${item!.index}: missing ${item!.errors.join(', ')}`
      ).join('\n');
      alert('Please complete all required fields:\n\n' + errorMsg);
      return;
    }
    
    console.log('Form submitted', { name, description, ruleType, severity, selectedChannels, conditions, conditionOperator });
    if (isEditing) {
      const request: UpdateAlertRuleRequest = {
        name,
        description: description || undefined,
        conditions,
        condition_operator: conditionOperator,
        severity,
        is_enabled: isEnabled,
        channel_ids: selectedChannels,
      };
      console.log('Update request:', request);
      updateMutation.mutate(request);
    } else {
      const request: CreateAlertRuleRequest = {
        name,
        description: description || undefined,
        rule_type: ruleType,
        conditions,
        condition_operator: conditionOperator,
        severity,
        channel_ids: selectedChannels,
      };
      console.log('Create request:', request);
      createMutation.mutate(request);
    }
  };

  const isPending = createMutation.isPending || updateMutation.isPending;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-black bg-opacity-50 p-4">
      <div className="my-8 w-full max-w-4xl rounded-lg bg-white p-6 dark:bg-gray-800">
        <h3 className="mb-4 text-lg font-medium text-gray-900 dark:text-gray-100">
          {isEditing ? 'Edit Alert Rule' : 'New Alert Rule'}
        </h3>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              Name
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
              required
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              Description
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
              rows={2}
            />
          </div>
          <div>
            <div className="mb-2 flex items-center justify-between">
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
                Conditions
              </label>
              <button
                type="button"
                onClick={() => setUseAdvancedFormat(!useAdvancedFormat)}
                className="text-xs font-medium text-blue-600 hover:text-blue-700 dark:text-blue-400"
              >
                {useAdvancedFormat ? 'Switch to Simple' : 'Switch to Advanced'}
              </button>
            </div>
            <div className="space-y-3 rounded-md border border-gray-300 p-4 dark:border-gray-600">
              {useAdvancedFormat ? (
                <div className="text-sm text-blue-600 dark:text-blue-400">
                  Advanced format: Create conditions with specific types (NodeStatus, LastReportTime, etc.)
                </div>
              ) : (
                <div className="text-sm text-gray-600 dark:text-gray-400">
                  Simple format: Define conditions with field, operator, and value
                </div>
              )}
              {conditions.map((condition, index) => (
                <div key={index} className="flex gap-2">
                  <div className="flex-1 grid grid-cols-3 gap-2">
                    {useAdvancedFormat ? (
                      <>
                        <select
                          value={condition.type || ''}
                          onChange={(e) => {
                            const newConditions = [...conditions];
                            newConditions[index] = { ...condition, type: e.target.value as any };
                            setConditions(newConditions);
                          }}
                          className="rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
                        >
                          <option value="">Select Type...</option>
                          <option value="NodeStatus">Node Status</option>
                          <option value="NodeFact">Node Fact</option>
                          <option value="ReportMetric">Report Metric</option>
                          <option value="EnvironmentFilter">Environment Filter</option>
                          <option value="GroupFilter">Group Filter</option>
                          <option value="NodeCountThreshold">Node Count Threshold</option>
                          <option value="TimeWindowFilter">Time Window Filter</option>
                          <option value="LastReportTime">Last Report Time</option>
                          <option value="ConsecutiveFailures">Consecutive Failures</option>
                          <option value="ConsecutiveChanges">Consecutive Changes</option>
                          <option value="ClassChangeFrequency">Class Change Frequency</option>
                        </select>
                        <select
                          value={condition.operator || ''}
                          onChange={(e) => {
                            const newConditions = [...conditions];
                            newConditions[index] = { ...condition, operator: e.target.value };
                            setConditions(newConditions);
                          }}
                          className="rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
                        >
                          <option value="">Select Operator...</option>
                          <option value="eq">equals</option>
                          <option value="ne">not equals</option>
                          <option value="gt">greater than</option>
                          <option value="gte">greater or equal</option>
                          <option value="lt">less than</option>
                          <option value="lte">less or equal</option>
                          <option value="contains">contains</option>
                          <option value="regex">regex</option>
                          <option value="in">in</option>
                          <option value="not_in">not in</option>
                          <option value="exists">exists</option>
                          <option value="not_exists">not exists</option>
                        </select>
                        <textarea
                          placeholder="Config (JSON)"
                          value={JSON.stringify(condition.config || {})}
                          onChange={(e) => {
                            try {
                              const newConditions = [...conditions];
                              newConditions[index] = { ...condition, config: JSON.parse(e.target.value) };
                              setConditions(newConditions);
                            } catch {
                              // Ignore JSON parse errors while typing
                            }
                          }}
                          className="rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
                          rows={2}
                        />
                      </>
                    ) : (
                      <>
                        <select
                          value={condition.field || ''}
                          onChange={(e) => {
                            const newConditions = [...conditions];
                            newConditions[index] = { ...condition, field: e.target.value };
                            setConditions(newConditions);
                          }}
                          className="rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
                          required
                        >
                          <option value="">Select Field...</option>
                          <option value="node.status">node.status</option>
                          <option value="node.name">node.name</option>
                          <option value="node.environment">node.environment</option>
                          <option value="node.group">node.group</option>
                          <option value="node.last_report">node.last_report</option>
                          <option value="report.status">report.status</option>
                          <option value="report.changed">report.changed</option>
                          <option value="report.failed">report.failed</option>
                          <option value="facts">facts</option>
                        </select>
                        <select
                          value={condition.operator || ''}
                          onChange={(e) => {
                            const newConditions = [...conditions];
                            newConditions[index] = { ...condition, operator: e.target.value };
                            setConditions(newConditions);
                          }}
                          className="rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
                          required
                        >
                          <option value="">Select Operator...</option>
                          <option value="eq">equals</option>
                          <option value="ne">not equals</option>
                          <option value="gt">greater than</option>
                          <option value="gte">greater or equal</option>
                          <option value="lt">less than</option>
                          <option value="lte">less or equal</option>
                          <option value="contains">contains</option>
                          <option value="regex">regex</option>
                          <option value="in">in</option>
                          <option value="not_in">not in</option>
                          <option value="exists">exists</option>
                          <option value="not_exists">not exists</option>
                        </select>
                        <input
                          type="text"
                          placeholder="Value (e.g., failed, 24, 2024-01-22)"
                          value={String(condition.value || '')}
                          onChange={(e) => {
                            const newConditions = [...conditions];
                            newConditions[index] = { ...condition, value: e.target.value };
                            setConditions(newConditions);
                          }}
                          className="rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
                          required
                        />
                      </>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => {
                      const newConditions = conditions.filter((_, i) => i !== index);
                      setConditions(newConditions.length > 0 ? newConditions : [{ field: '', operator: 'eq', value: '' }]);
                    }}
                    className="flex h-10 w-10 items-center justify-center rounded-md bg-red-100 text-red-600 hover:bg-red-200 dark:bg-red-900 dark:text-red-300"
                    title="Remove condition"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              ))}
              <button
                type="button"
                onClick={() => {
                  setConditions([...conditions, { field: '', operator: 'eq', value: '' }]);
                }}
                className="flex w-full items-center justify-center gap-2 rounded-md border-2 border-dashed border-gray-300 py-2 text-sm font-medium text-gray-600 hover:border-gray-400 hover:text-gray-700 dark:border-gray-600 dark:text-gray-400"
              >
                <Plus className="h-4 w-4" />
                Add Condition
              </button>
              <div className="flex items-center gap-2">
                <select
                  value={conditionOperator}
                  onChange={(e) => setConditionOperator(e.target.value as 'all' | 'any')}
                  className="rounded-md border border-gray-300 px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  <option value="all">ALL (AND) - All conditions must match</option>
                  <option value="any">ANY (OR) - Any condition can match</option>
                </select>
                <span className="text-sm text-gray-600 dark:text-gray-400">between conditions</span>
              </div>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
                Type
              </label>
              <select
                value={ruleType}
                onChange={(e) => setRuleType(e.target.value as AlertRuleType)}
                className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
                disabled={isEditing}
              >
                <option value="node_status">Node Status - Monitor node availability and health</option>
                <option value="compliance">Compliance - Track configuration compliance</option>
                <option value="drift">Drift Detection - Detect configuration drift</option>
                <option value="report_failure">Report Failure - Alert on failed Puppet runs</option>
                <option value="custom">Custom - Custom alert conditions</option>
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
                Severity
              </label>
              <select
                value={severity}
                onChange={(e) => setSeverity(e.target.value as AlertSeverity)}
                className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
              >
                <option value="info">Info</option>
                <option value="warning">Warning</option>
                <option value="critical">Critical</option>
              </select>
            </div>
          </div>
          {isEditing && (
            <div>
              <label className="flex items-center space-x-2">
                <input
                  type="checkbox"
                  checked={isEnabled}
                  onChange={(e) => setIsEnabled(e.target.checked)}
                  className="rounded border-gray-300"
                />
                <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
                  Enabled
                </span>
              </label>
            </div>
          )}
          {channels.length > 0 && (
            <div>
              <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
                Notification Channels
              </label>
              <div className="mt-2 space-y-2">
                {channels.map((channel) => {
                  const isChecked = selectedChannels.includes(channel.id);
                  return (
                    <label key={channel.id} className="flex items-center space-x-2 cursor-pointer">
                      <input
                        type="checkbox"
                        id={`channel-${channel.id}`}
                        checked={isChecked}
                        onChange={(e) => {
                          e.stopPropagation();
                          console.log('Checkbox changed:', { channelId: channel.id, channelName: channel.name, checked: e.target.checked, currentSelected: selectedChannels });
                          if (e.target.checked) {
                            const newSelected = [...selectedChannels, channel.id];
                            console.log('Adding channel, new selection:', newSelected);
                            setSelectedChannels(newSelected);
                          } else {
                            const newSelected = selectedChannels.filter((id) => id !== channel.id);
                            console.log('Removing channel, new selection:', newSelected);
                            setSelectedChannels(newSelected);
                          }
                        }}
                        className="rounded border-gray-300 focus:ring-blue-500"
                      />
                      <span className="text-sm text-gray-700 dark:text-gray-300">{channel.name}</span>
                    </label>
                  );
                })}
              </div>
            </div>
          )}
          <div className="flex justify-end space-x-3">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 dark:bg-gray-700 dark:text-gray-300"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isPending}
              className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            >
              {isEditing ? 'Save' : 'Create'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function NewSilenceModal({ rules, onClose }: { rules: AlertRule[]; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [ruleId, setRuleId] = useState<string>('');
  const [reason, setReason] = useState('');
  const [duration, setDuration] = useState('1');

  const createMutation = useMutation({
    mutationFn: api.createSilence,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['silences'] });
      onClose();
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const endsAt = new Date();
    endsAt.setHours(endsAt.getHours() + parseInt(duration));

    const request: CreateSilenceRequest = {
      rule_id: ruleId || undefined,
      reason,
      ends_at: endsAt.toISOString(),
    };
    createMutation.mutate(request);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black bg-opacity-50">
      <div className="w-full max-w-md rounded-lg bg-white p-6 dark:bg-gray-800">
        <h3 className="mb-4 text-lg font-medium text-gray-900 dark:text-gray-100">New Silence</h3>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              Rule (optional)
            </label>
            <select
              value={ruleId}
              onChange={(e) => setRuleId(e.target.value)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
            >
              <option value="">All rules</option>
              {rules.map((rule) => (
                <option key={rule.id} value={rule.id}>
                  {rule.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              Duration
            </label>
            <select
              value={duration}
              onChange={(e) => setDuration(e.target.value)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
            >
              <option value="1">1 hour</option>
              <option value="4">4 hours</option>
              <option value="8">8 hours</option>
              <option value="24">24 hours</option>
              <option value="168">1 week</option>
            </select>
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
              Reason
            </label>
            <textarea
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className="mt-1 block w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-700"
              rows={2}
              required
            />
          </div>
          <div className="flex justify-end space-x-3">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md bg-gray-100 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-200 dark:bg-gray-700 dark:text-gray-300"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={createMutation.isPending}
              className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
            >
              Create
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
