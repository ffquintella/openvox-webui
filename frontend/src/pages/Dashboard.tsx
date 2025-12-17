import { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import {
  PieChart,
  Pie,
  Cell,
  ResponsiveContainer,
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
} from 'recharts';
import {
  Server,
  CheckCircle,
  XCircle,
  AlertCircle,
  Clock,
  Search,
  Filter,
  RefreshCw,
  Activity,
  ArrowRight,
} from 'lucide-react';
import { api } from '../services/api';
import type { Node } from '../types';

const COLORS = {
  changed: '#22c55e',
  unchanged: '#3b82f6',
  failed: '#ef4444',
  unreported: '#f59e0b',
};

// Node health status type
type HealthStatus = 'healthy' | 'warning' | 'critical' | 'unknown';

function getNodeHealth(node: Node): HealthStatus {
  if (!node.report_timestamp) return 'unknown';

  const lastReport = new Date(node.report_timestamp);
  const hoursSinceReport = (Date.now() - lastReport.getTime()) / (1000 * 60 * 60);

  if (hoursSinceReport > 24) return 'critical';
  if (hoursSinceReport > 6) return 'warning';

  switch (node.latest_report_status) {
    case 'failed':
      return 'critical';
    case 'changed':
    case 'unchanged':
      return 'healthy';
    default:
      return 'unknown';
  }
}

function getHealthColor(status: HealthStatus): string {
  switch (status) {
    case 'healthy':
      return 'bg-success-500';
    case 'warning':
      return 'bg-warning-500';
    case 'critical':
      return 'bg-danger-500';
    default:
      return 'bg-gray-400';
  }
}

function formatTimeAgo(dateString: string | null | undefined): string {
  if (!dateString) return 'Never';

  const date = new Date(dateString);
  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffMins = Math.floor(diffMs / (1000 * 60));
  const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffMins < 1) return 'Just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString();
}

export default function Dashboard() {
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');

  const { data: nodes = [], isLoading: nodesLoading, refetch: refetchNodes } = useQuery({
    queryKey: ['nodes'],
    queryFn: api.getNodes,
  });

  const { data: reports = [], isLoading: reportsLoading, refetch: refetchReports } = useQuery({
    queryKey: ['reports', { limit: 20 }],
    queryFn: () => api.getReports({ limit: 20 }),
  });

  // Calculate stats from real node data
  const stats = useMemo(() => {
    const statusCounts = {
      changed: 0,
      unchanged: 0,
      failed: 0,
      unreported: 0,
    };

    const healthCounts = {
      healthy: 0,
      warning: 0,
      critical: 0,
      unknown: 0,
    };

    nodes.forEach((node) => {
      // Count by status
      const status = node.latest_report_status;
      if (status === 'changed') statusCounts.changed++;
      else if (status === 'unchanged') statusCounts.unchanged++;
      else if (status === 'failed') statusCounts.failed++;
      else statusCounts.unreported++;

      // Count by health
      const health = getNodeHealth(node);
      healthCounts[health]++;
    });

    return { statusCounts, healthCounts };
  }, [nodes]);

  // Filter nodes for search
  const filteredNodes = useMemo(() => {
    let result = nodes;

    // Apply search filter
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      result = result.filter(
        (node) =>
          node.certname.toLowerCase().includes(query) ||
          node.catalog_environment?.toLowerCase().includes(query)
      );
    }

    // Apply status filter
    if (statusFilter !== 'all') {
      result = result.filter((node) => {
        if (statusFilter === 'unreported') {
          return !node.latest_report_status;
        }
        return node.latest_report_status === statusFilter;
      });
    }

    return result;
  }, [nodes, searchQuery, statusFilter]);

  // Build chart data
  const statusData = [
    { name: 'Changed', value: stats.statusCounts.changed, color: COLORS.changed },
    { name: 'Unchanged', value: stats.statusCounts.unchanged, color: COLORS.unchanged },
    { name: 'Failed', value: stats.statusCounts.failed, color: COLORS.failed },
    { name: 'Unreported', value: stats.statusCounts.unreported, color: COLORS.unreported },
  ];

  // Generate trend data from reports (last 7 days)
  const trendData = useMemo(() => {
    const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
    const today = new Date();
    const data: { date: string; changed: number; failed: number }[] = [];

    for (let i = 6; i >= 0; i--) {
      const date = new Date(today);
      date.setDate(date.getDate() - i);
      const dayName = days[date.getDay()];

      const dayReports = reports.filter((r) => {
        if (!r.start_time) return false;
        const reportDate = new Date(r.start_time);
        return reportDate.toDateString() === date.toDateString();
      });

      data.push({
        date: dayName,
        changed: dayReports.filter((r) => r.status === 'changed').length,
        failed: dayReports.filter((r) => r.status === 'failed').length,
      });
    }

    return data;
  }, [reports]);

  const statsCards = [
    {
      name: 'Total Nodes',
      value: nodes.length,
      icon: Server,
      color: 'text-primary-600',
      bg: 'bg-primary-50',
    },
    {
      name: 'Healthy',
      value: stats.healthCounts.healthy,
      icon: CheckCircle,
      color: 'text-success-500',
      bg: 'bg-success-50',
    },
    {
      name: 'Failed',
      value: stats.statusCounts.failed,
      icon: XCircle,
      color: 'text-danger-500',
      bg: 'bg-danger-50',
    },
    {
      name: 'Unreported',
      value: stats.statusCounts.unreported,
      icon: AlertCircle,
      color: 'text-warning-500',
      bg: 'bg-warning-50',
    },
  ];

  const isLoading = nodesLoading || reportsLoading;

  const handleRefresh = () => {
    refetchNodes();
    refetchReports();
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  return (
    <div>
      {/* Header with Search and Refresh */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <h1 className="text-2xl font-bold text-gray-900">Dashboard</h1>

        <div className="flex items-center gap-3">
          {/* Quick Search */}
          <div className="relative">
            <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
              <Search className="h-4 w-4 text-gray-400" />
            </div>
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search nodes..."
              className="block w-full sm:w-64 pl-10 pr-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
            />
          </div>

          {/* Status Filter */}
          <div className="relative">
            <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
              <Filter className="h-4 w-4 text-gray-400" />
            </div>
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              className="block w-full pl-10 pr-8 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary-500 focus:border-primary-500 bg-white"
            >
              <option value="all">All Status</option>
              <option value="changed">Changed</option>
              <option value="unchanged">Unchanged</option>
              <option value="failed">Failed</option>
              <option value="unreported">Unreported</option>
            </select>
          </div>

          {/* Refresh Button */}
          <button
            onClick={handleRefresh}
            className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors"
            title="Refresh data"
          >
            <RefreshCw className="h-5 w-5" />
          </button>
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
        {statsCards.map((stat) => (
          <div key={stat.name} className="card">
            <div className="flex items-center">
              <div className={`p-3 rounded-lg ${stat.bg}`}>
                <stat.icon className={`w-6 h-6 ${stat.color}`} />
              </div>
              <div className="ml-4">
                <p className="text-sm font-medium text-gray-500">{stat.name}</p>
                <p className="text-2xl font-bold text-gray-900">{stat.value}</p>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Main Content Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-8">
        {/* Status Distribution Chart */}
        <div className="card">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Node Status Distribution</h2>
          <div className="h-64">
            <ResponsiveContainer width="100%" height="100%">
              <PieChart>
                <Pie
                  data={statusData}
                  cx="50%"
                  cy="50%"
                  innerRadius={60}
                  outerRadius={80}
                  paddingAngle={5}
                  dataKey="value"
                >
                  {statusData.map((entry, index) => (
                    <Cell key={`cell-${index}`} fill={entry.color} />
                  ))}
                </Pie>
                <Tooltip />
                <Legend />
              </PieChart>
            </ResponsiveContainer>
          </div>
        </div>

        {/* Activity Trend Chart */}
        <div className="card lg:col-span-2">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Weekly Activity Trend</h2>
          <div className="h-64">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={trendData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="date" />
                <YAxis />
                <Tooltip />
                <Legend />
                <Line
                  type="monotone"
                  dataKey="changed"
                  stroke={COLORS.changed}
                  name="Changed"
                  strokeWidth={2}
                />
                <Line
                  type="monotone"
                  dataKey="failed"
                  stroke={COLORS.failed}
                  name="Failed"
                  strokeWidth={2}
                />
              </LineChart>
            </ResponsiveContainer>
          </div>
        </div>
      </div>

      {/* Bottom Section: Node Health and Recent Activity */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Node Health Status */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-gray-900">Node Health Status</h2>
            <Link
              to="/nodes"
              className="text-sm text-primary-600 hover:text-primary-700 flex items-center gap-1"
            >
              View all <ArrowRight className="h-4 w-4" />
            </Link>
          </div>

          {filteredNodes.length === 0 ? (
            <div className="text-center py-8 text-gray-500">
              <Server className="h-12 w-12 mx-auto mb-3 text-gray-300" />
              <p>No nodes found</p>
              {searchQuery && (
                <p className="text-sm mt-1">Try adjusting your search criteria</p>
              )}
            </div>
          ) : (
            <div className="space-y-3 max-h-80 overflow-y-auto">
              {filteredNodes.slice(0, 10).map((node) => {
                const health = getNodeHealth(node);
                return (
                  <Link
                    key={node.certname}
                    to={`/nodes/${encodeURIComponent(node.certname)}`}
                    className="flex items-center justify-between p-3 hover:bg-gray-50 rounded-lg transition-colors"
                  >
                    <div className="flex items-center gap-3">
                      <div className={`w-2.5 h-2.5 rounded-full ${getHealthColor(health)}`} />
                      <div>
                        <p className="font-medium text-gray-900 text-sm">{node.certname}</p>
                        <p className="text-xs text-gray-500">
                          {node.catalog_environment || 'production'}
                        </p>
                      </div>
                    </div>
                    <div className="text-right">
                      <p className="text-xs text-gray-500">
                        {formatTimeAgo(node.report_timestamp)}
                      </p>
                      <p className="text-xs capitalize text-gray-400">
                        {node.latest_report_status || 'unreported'}
                      </p>
                    </div>
                  </Link>
                );
              })}
              {filteredNodes.length > 10 && (
                <p className="text-center text-sm text-gray-500 pt-2">
                  +{filteredNodes.length - 10} more nodes
                </p>
              )}
            </div>
          )}
        </div>

        {/* Recent Activity Timeline */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-gray-900">Recent Activity</h2>
            <Link
              to="/reports"
              className="text-sm text-primary-600 hover:text-primary-700 flex items-center gap-1"
            >
              View all <ArrowRight className="h-4 w-4" />
            </Link>
          </div>

          {reports.length === 0 ? (
            <div className="text-center py-8 text-gray-500">
              <Activity className="h-12 w-12 mx-auto mb-3 text-gray-300" />
              <p>No recent activity</p>
            </div>
          ) : (
            <div className="space-y-4 max-h-80 overflow-y-auto">
              {reports.slice(0, 10).map((report) => (
                <div key={report.hash} className="flex gap-3">
                  <div className="flex-shrink-0 mt-1">
                    <div
                      className={`w-2 h-2 rounded-full ${
                        report.status === 'failed'
                          ? 'bg-danger-500'
                          : report.status === 'changed'
                          ? 'bg-success-500'
                          : 'bg-primary-500'
                      }`}
                    />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between">
                      <Link
                        to={`/nodes/${encodeURIComponent(report.certname)}`}
                        className="font-medium text-gray-900 text-sm hover:text-primary-600 truncate"
                      >
                        {report.certname}
                      </Link>
                      <span className="text-xs text-gray-500 flex items-center gap-1">
                        <Clock className="h-3 w-3" />
                        {formatTimeAgo(report.start_time)}
                      </span>
                    </div>
                    <p className="text-xs text-gray-500 mt-0.5">
                      <span
                        className={`capitalize ${
                          report.status === 'failed'
                            ? 'text-danger-600'
                            : report.status === 'changed'
                            ? 'text-success-600'
                            : 'text-primary-600'
                        }`}
                      >
                        {report.status}
                      </span>
                      {report.environment && ` in ${report.environment}`}
                      {report.metrics?.changes !== undefined && report.metrics.changes > 0 && (
                        <span> - {report.metrics.changes} changes</span>
                      )}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
