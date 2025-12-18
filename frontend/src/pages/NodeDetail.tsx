import { useState, useMemo } from 'react';
import { useParams, Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import {
  ArrowLeft,
  Server,
  Clock,
  Folder,
  Database,
  FileText,
  Search,
  ChevronRight,
  ChevronDown,
  Copy,
  Check,
  AlertCircle,
  CheckCircle,
  XCircle,
  RefreshCw,
  FolderTree,
  Tag,
} from 'lucide-react';
import { api } from '../services/api';
import type { Report, NodeGroup } from '../types';

type TabId = 'overview' | 'facts' | 'reports' | 'groups';

interface Tab {
  id: TabId;
  label: string;
  icon: typeof Server;
}

const TABS: Tab[] = [
  { id: 'overview', label: 'Overview', icon: Server },
  { id: 'facts', label: 'Facts', icon: Database },
  { id: 'reports', label: 'Reports', icon: FileText },
  { id: 'groups', label: 'Groups', icon: FolderTree },
];

function getStatusColor(status: string | null | undefined): string {
  switch (status) {
    case 'changed':
      return 'text-success-600 bg-success-50';
    case 'unchanged':
      return 'text-primary-600 bg-primary-50';
    case 'failed':
      return 'text-danger-600 bg-danger-50';
    default:
      return 'text-gray-600 bg-gray-50';
  }
}

function getStatusIcon(status: string | null | undefined) {
  switch (status) {
    case 'changed':
      return CheckCircle;
    case 'unchanged':
      return CheckCircle;
    case 'failed':
      return XCircle;
    default:
      return AlertCircle;
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
  if (diffMins < 60) return `${diffMins} minutes ago`;
  if (diffHours < 24) return `${diffHours} hours ago`;
  if (diffDays < 7) return `${diffDays} days ago`;

  return date.toLocaleDateString();
}

// Type for PuppetDB fact format
interface PuppetDBFact {
  certname: string;
  name: string;
  value: unknown;
  environment?: string;
}

// Facts Browser Component
function FactsBrowser({ facts }: { facts: Record<string, unknown> }) {
  const [searchQuery, setSearchQuery] = useState('');
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set(['']));
  const [copiedPath, setCopiedPath] = useState<string | null>(null);

  const flattenedFacts = useMemo(() => {
    const result: Array<{ path: string; value: unknown; depth: number }> = [];

    const flatten = (obj: unknown, path: string, depth: number) => {
      if (obj && typeof obj === 'object' && !Array.isArray(obj)) {
        Object.entries(obj as Record<string, unknown>).forEach(([key, value]) => {
          const newPath = path ? `${path}.${key}` : key;
          result.push({ path: newPath, value, depth });
          if (value && typeof value === 'object') {
            flatten(value, newPath, depth + 1);
          }
        });
      } else if (Array.isArray(obj)) {
        obj.forEach((item, index) => {
          const newPath = `${path}[${index}]`;
          result.push({ path: newPath, value: item, depth });
          if (item && typeof item === 'object') {
            flatten(item, newPath, depth + 1);
          }
        });
      }
    };

    flatten(facts, '', 0);
    return result;
  }, [facts]);

  const filteredFacts = useMemo(() => {
    if (!searchQuery.trim()) return flattenedFacts;

    const query = searchQuery.toLowerCase();
    return flattenedFacts.filter(
      (fact) =>
        fact.path.toLowerCase().includes(query) ||
        String(fact.value).toLowerCase().includes(query)
    );
  }, [flattenedFacts, searchQuery]);

  const togglePath = (path: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const copyToClipboard = (path: string) => {
    navigator.clipboard.writeText(path);
    setCopiedPath(path);
    setTimeout(() => setCopiedPath(null), 2000);
  };

  const isValueExpandable = (value: unknown): boolean => {
    return value !== null && typeof value === 'object';
  };

  const getParentPath = (path: string): string => {
    const lastDot = path.lastIndexOf('.');
    const lastBracket = path.lastIndexOf('[');
    const lastSep = Math.max(lastDot, lastBracket);
    return lastSep > 0 ? path.substring(0, lastSep) : '';
  };

  const isVisible = (path: string): boolean => {
    if (searchQuery.trim()) return true;

    const parent = getParentPath(path);
    if (!parent) return true;

    return expandedPaths.has(parent);
  };

  const renderValue = (value: unknown): string => {
    if (value === null) return 'null';
    if (value === undefined) return 'undefined';
    if (typeof value === 'boolean') return value ? 'true' : 'false';
    if (typeof value === 'number') return String(value);
    if (typeof value === 'string') return `"${value}"`;
    if (Array.isArray(value)) return `Array(${value.length})`;
    if (typeof value === 'object') return `Object(${Object.keys(value as object).length})`;
    return String(value);
  };

  return (
    <div>
      {/* Search */}
      <div className="relative mb-4">
        <div className="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
          <Search className="h-4 w-4 text-gray-400" />
        </div>
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Search facts by path or value..."
          className="block w-full pl-10 pr-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
        />
      </div>

      {/* Facts list */}
      <div className="border border-gray-200 rounded-lg overflow-hidden">
        <div className="max-h-[500px] overflow-y-auto">
          {filteredFacts.length === 0 ? (
            <div className="p-8 text-center text-gray-500">
              <Database className="w-12 h-12 mx-auto mb-3 text-gray-300" />
              <p>No facts found</p>
            </div>
          ) : (
            <div className="divide-y divide-gray-100">
              {filteredFacts
                .filter((fact) => isVisible(fact.path))
                .map((fact) => {
                  const expandable = isValueExpandable(fact.value);
                  const isExpanded = expandedPaths.has(fact.path);
                  const pathParts = fact.path.split('.');
                  const key = pathParts[pathParts.length - 1];

                  return (
                    <div
                      key={fact.path}
                      className="flex items-center gap-2 px-3 py-2 hover:bg-gray-50 group"
                      style={{ paddingLeft: `${fact.depth * 16 + 12}px` }}
                    >
                      {/* Expand button */}
                      <button
                        onClick={() => expandable && togglePath(fact.path)}
                        className={`w-4 h-4 flex items-center justify-center ${
                          expandable ? 'cursor-pointer' : 'cursor-default'
                        }`}
                      >
                        {expandable ? (
                          isExpanded ? (
                            <ChevronDown className="w-4 h-4 text-gray-400" />
                          ) : (
                            <ChevronRight className="w-4 h-4 text-gray-400" />
                          )
                        ) : (
                          <span className="w-1 h-1 bg-gray-300 rounded-full" />
                        )}
                      </button>

                      {/* Key */}
                      <span className="font-medium text-gray-700 text-sm">{key}</span>

                      {/* Value */}
                      {!expandable && (
                        <>
                          <span className="text-gray-400 mx-1">:</span>
                          <span
                            className={`text-sm ${
                              typeof fact.value === 'string'
                                ? 'text-success-600'
                                : typeof fact.value === 'number'
                                ? 'text-primary-600'
                                : typeof fact.value === 'boolean'
                                ? 'text-amber-600'
                                : 'text-gray-600'
                            }`}
                          >
                            {renderValue(fact.value)}
                          </span>
                        </>
                      )}

                      {/* Type badge for objects/arrays */}
                      {expandable && (
                        <span className="text-xs text-gray-400 ml-1">
                          {renderValue(fact.value)}
                        </span>
                      )}

                      {/* Copy button */}
                      <button
                        onClick={() => copyToClipboard(fact.path)}
                        className="ml-auto opacity-0 group-hover:opacity-100 transition-opacity p-1 hover:bg-gray-200 rounded"
                        title="Copy path"
                      >
                        {copiedPath === fact.path ? (
                          <Check className="w-3 h-3 text-success-500" />
                        ) : (
                          <Copy className="w-3 h-3 text-gray-400" />
                        )}
                      </button>
                    </div>
                  );
                })}
            </div>
          )}
        </div>
      </div>

      {/* Stats */}
      <div className="mt-3 text-sm text-gray-500">
        {flattenedFacts.length} facts total
        {searchQuery && ` | ${filteredFacts.length} matching`}
      </div>
    </div>
  );
}

// Reports Timeline Component
function ReportsTimeline({ reports }: { reports: Report[] }) {
  const [expandedReport, setExpandedReport] = useState<string | null>(null);

  if (reports.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500">
        <FileText className="w-12 h-12 mx-auto mb-3 text-gray-300" />
        <p>No reports available</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {reports.map((report, index) => {
        const StatusIcon = getStatusIcon(report.status);
        const isExpanded = expandedReport === report.hash;

        return (
          <div key={report.hash} className="relative">
            {/* Timeline line */}
            {index < reports.length - 1 && (
              <div className="absolute left-4 top-10 bottom-0 w-0.5 bg-gray-200" />
            )}

            <div
              className={`relative flex gap-4 p-4 rounded-lg border transition-all cursor-pointer ${
                isExpanded
                  ? 'border-primary-200 bg-primary-50'
                  : 'border-gray-200 hover:border-gray-300 hover:bg-gray-50'
              }`}
              onClick={() => setExpandedReport(isExpanded ? null : report.hash)}
            >
              {/* Status icon */}
              <div
                className={`flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center ${getStatusColor(
                  report.status
                )}`}
              >
                <StatusIcon className="w-4 h-4" />
              </div>

              {/* Content */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between">
                  <span className={`text-sm font-medium capitalize ${getStatusColor(report.status).split(' ')[0]}`}>
                    {report.status || 'Unknown'}
                  </span>
                  <span className="text-xs text-gray-500 flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {formatTimeAgo(report.start_time)}
                  </span>
                </div>

                <p className="text-sm text-gray-600 mt-1">
                  {report.environment && `Environment: ${report.environment}`}
                  {report.metrics?.changes !== undefined && (
                    <span className="ml-2">
                      {report.metrics.changes} changes
                    </span>
                  )}
                </p>

                {/* Expanded details */}
                {isExpanded && (
                  <div className="mt-4 pt-4 border-t border-gray-200 space-y-3">
                    <div className="grid grid-cols-2 gap-4 text-sm">
                      <div>
                        <p className="text-gray-500">Report Hash</p>
                        <p className="font-mono text-xs text-gray-700 truncate">
                          {report.hash}
                        </p>
                      </div>
                      <div>
                        <p className="text-gray-500">Puppet Version</p>
                        <p className="text-gray-700">{report.puppet_version || 'N/A'}</p>
                      </div>
                      <div>
                        <p className="text-gray-500">Configuration Version</p>
                        <p className="text-gray-700 truncate">
                          {report.configuration_version || 'N/A'}
                        </p>
                      </div>
                      <div>
                        <p className="text-gray-500">Transaction UUID</p>
                        <p className="font-mono text-xs text-gray-700 truncate">
                          {report.transaction_uuid || 'N/A'}
                        </p>
                      </div>
                    </div>

                    {/* Metrics */}
                    {report.metrics && (
                      <div className="bg-white rounded-lg p-3 border border-gray-100">
                        <p className="text-sm font-medium text-gray-700 mb-2">Resource Metrics</p>
                        <div className="grid grid-cols-4 gap-2 text-center text-sm">
                          <div>
                            <p className="text-lg font-semibold text-gray-900">
                              {report.metrics.resources?.total ?? 0}
                            </p>
                            <p className="text-xs text-gray-500">Total</p>
                          </div>
                          <div>
                            <p className="text-lg font-semibold text-success-600">
                              {report.metrics.resources?.changed ?? 0}
                            </p>
                            <p className="text-xs text-gray-500">Changed</p>
                          </div>
                          <div>
                            <p className="text-lg font-semibold text-danger-600">
                              {report.metrics.resources?.failed ?? 0}
                            </p>
                            <p className="text-xs text-gray-500">Failed</p>
                          </div>
                          <div>
                            <p className="text-lg font-semibold text-gray-600">
                              {report.metrics.resources?.skipped ?? 0}
                            </p>
                            <p className="text-xs text-gray-500">Skipped</p>
                          </div>
                        </div>
                      </div>
                    )}

                    {/* Times */}
                    <div className="text-xs text-gray-500">
                      {report.start_time && (
                        <p>Started: {new Date(report.start_time).toLocaleString()}</p>
                      )}
                      {report.end_time && (
                        <p>Ended: {new Date(report.end_time).toLocaleString()}</p>
                      )}
                      {report.metrics?.time?.total !== undefined && (
                        <p>Duration: {report.metrics.time.total.toFixed(2)}s</p>
                      )}
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

// Group Membership Component
function GroupMembership({
  certname,
  groups,
}: {
  certname: string;
  groups: NodeGroup[];
}) {
  const matchedGroups = useMemo(() => {
    return groups.filter((group) => group.pinned_nodes.includes(certname));
  }, [certname, groups]);

  const potentialGroups = useMemo(() => {
    return groups.filter(
      (group) => !group.pinned_nodes.includes(certname) && group.rules.length > 0
    );
  }, [certname, groups]);

  return (
    <div className="space-y-6">
      {/* Matched Groups */}
      <div>
        <h3 className="text-sm font-medium text-gray-700 mb-3 flex items-center gap-2">
          <Tag className="w-4 h-4" />
          Pinned to Groups ({matchedGroups.length})
        </h3>
        {matchedGroups.length === 0 ? (
          <p className="text-sm text-gray-500">
            This node is not pinned to any groups.
          </p>
        ) : (
          <div className="space-y-2">
            {matchedGroups.map((group) => (
              <Link
                key={group.id}
                to={`/groups?selected=${group.id}`}
                className="flex items-center justify-between p-3 bg-primary-50 border border-primary-200 rounded-lg hover:bg-primary-100 transition-colors"
              >
                <div className="flex items-center gap-3">
                  <FolderTree className="w-5 h-5 text-primary-600" />
                  <div>
                    <p className="font-medium text-primary-900">{group.name}</p>
                    {group.description && (
                      <p className="text-sm text-primary-700">{group.description}</p>
                    )}
                  </div>
                </div>
                <div className="text-right text-sm">
                  <p className="text-primary-600">
                    {group.classes.length} classes
                  </p>
                  <p className="text-primary-500">
                    {group.pinned_nodes.length} nodes
                  </p>
                </div>
              </Link>
            ))}
          </div>
        )}
      </div>

      {/* Classification Rules */}
      <div>
        <h3 className="text-sm font-medium text-gray-700 mb-3 flex items-center gap-2">
          <Database className="w-4 h-4" />
          Groups with Classification Rules ({potentialGroups.length})
        </h3>
        <p className="text-xs text-gray-500 mb-3">
          These groups have rules that may match this node based on its facts.
        </p>
        {potentialGroups.length === 0 ? (
          <p className="text-sm text-gray-500">No groups with classification rules.</p>
        ) : (
          <div className="space-y-2">
            {potentialGroups.slice(0, 5).map((group) => (
              <div
                key={group.id}
                className="flex items-center justify-between p-3 border border-gray-200 rounded-lg"
              >
                <div className="flex items-center gap-3">
                  <FolderTree className="w-5 h-5 text-gray-400" />
                  <div>
                    <p className="font-medium text-gray-900">{group.name}</p>
                    <p className="text-xs text-gray-500">
                      {group.rules.length} rule{group.rules.length !== 1 ? 's' : ''} •{' '}
                      Match {group.rule_match_type}
                    </p>
                  </div>
                </div>
                <Link
                  to={`/groups?selected=${group.id}`}
                  className="text-sm text-primary-600 hover:text-primary-700"
                >
                  View Rules
                </Link>
              </div>
            ))}
            {potentialGroups.length > 5 && (
              <p className="text-sm text-gray-500 text-center">
                +{potentialGroups.length - 5} more groups
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export default function NodeDetail() {
  const { certname } = useParams<{ certname: string }>();
  const [activeTab, setActiveTab] = useState<TabId>('overview');

  const {
    data: node,
    isLoading: nodeLoading,
    refetch: refetchNode,
  } = useQuery({
    queryKey: ['node', certname],
    queryFn: () => api.getNode(certname!),
    enabled: !!certname,
  });

  const {
    data: facts = {},
    isLoading: factsLoading,
    refetch: refetchFacts,
  } = useQuery({
    queryKey: ['node-facts', certname],
    queryFn: () => api.getNodeFacts(certname!),
    enabled: !!certname,
  });

  const {
    data: reports = [],
    isLoading: reportsLoading,
    refetch: refetchReports,
  } = useQuery({
    queryKey: ['node-reports', certname],
    queryFn: () => api.getNodeReports(certname!),
    enabled: !!certname,
  });

  const {
    data: groups = [],
    isLoading: groupsLoading,
    refetch: refetchGroups,
  } = useQuery({
    queryKey: ['groups'],
    queryFn: api.getGroups,
  });

  const handleRefresh = () => {
    refetchNode();
    refetchFacts();
    refetchReports();
    refetchGroups();
  };

  // Normalize facts from PuppetDB array format to object format
  const normalizedFacts = useMemo(() => {
    if (Array.isArray(facts)) {
      const result: Record<string, unknown> = {};
      for (const fact of facts as PuppetDBFact[]) {
        if (fact && typeof fact === 'object' && 'name' in fact && 'value' in fact) {
          result[fact.name] = fact.value;
        }
      }
      return result;
    }
    return facts as Record<string, unknown>;
  }, [facts]);

  const isLoading = nodeLoading || factsLoading;

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  if (!node) {
    return (
      <div className="text-center py-12">
        <Server className="w-16 h-16 mx-auto mb-4 text-gray-300" />
        <p className="text-gray-500 text-lg">Node not found</p>
        <p className="text-gray-400 text-sm mt-1">{certname}</p>
        <Link
          to="/nodes"
          className="inline-flex items-center gap-2 mt-4 text-primary-600 hover:text-primary-700"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to nodes
        </Link>
      </div>
    );
  }

  const StatusIcon = getStatusIcon(node.latest_report_status);
  const matchedGroupsCount = groups.filter((g) =>
    g.pinned_nodes.includes(certname!)
  ).length;

  return (
    <div>
      {/* Header */}
      <div className="mb-6">
        <Link
          to="/nodes"
          className="inline-flex items-center text-gray-500 hover:text-gray-700 mb-4"
        >
          <ArrowLeft className="w-4 h-4 mr-2" />
          Back to nodes
        </Link>

        <div className="flex items-start justify-between">
          <div className="flex items-center gap-4">
            <div className="p-4 bg-primary-50 rounded-xl">
              <Server className="w-10 h-10 text-primary-600" />
            </div>
            <div>
              <h1 className="text-2xl font-bold text-gray-900">{node.certname}</h1>
              <div className="flex items-center gap-3 mt-1">
                <span className="text-gray-500">
                  {node.catalog_environment || 'production'}
                </span>
                <span
                  className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-sm ${getStatusColor(
                    node.latest_report_status
                  )}`}
                >
                  <StatusIcon className="w-3 h-3" />
                  <span className="capitalize">
                    {node.latest_report_status || 'Unknown'}
                  </span>
                </span>
              </div>
            </div>
          </div>

          <button
            onClick={handleRefresh}
            className="flex items-center gap-2 px-4 py-2 text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-lg transition-colors"
          >
            <RefreshCw className="w-4 h-4" />
            Refresh
          </button>
        </div>
      </div>

      {/* Quick Stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        <div className="card">
          <div className="flex items-center gap-3">
            <Clock className="w-5 h-5 text-gray-400" />
            <div>
              <p className="text-xs text-gray-500">Last Report</p>
              <p className="text-sm font-medium text-gray-900">
                {formatTimeAgo(node.report_timestamp)}
              </p>
            </div>
          </div>
        </div>
        <div className="card">
          <div className="flex items-center gap-3">
            <Folder className="w-5 h-5 text-gray-400" />
            <div>
              <p className="text-xs text-gray-500">Environment</p>
              <p className="text-sm font-medium text-gray-900">
                {node.catalog_environment || 'production'}
              </p>
            </div>
          </div>
        </div>
        <div className="card">
          <div className="flex items-center gap-3">
            <Database className="w-5 h-5 text-gray-400" />
            <div>
              <p className="text-xs text-gray-500">Facts</p>
              <p className="text-sm font-medium text-gray-900">
                {Object.keys(normalizedFacts).length} top-level
              </p>
            </div>
          </div>
        </div>
        <div className="card">
          <div className="flex items-center gap-3">
            <FolderTree className="w-5 h-5 text-gray-400" />
            <div>
              <p className="text-xs text-gray-500">Groups</p>
              <p className="text-sm font-medium text-gray-900">
                {matchedGroupsCount} matched
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-gray-200 mb-6">
        <nav className="flex gap-4">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`flex items-center gap-2 py-3 px-1 border-b-2 font-medium text-sm transition-colors ${
                activeTab === tab.id
                  ? 'border-primary-500 text-primary-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
              }`}
            >
              <tab.icon className="w-4 h-4" />
              {tab.label}
              {tab.id === 'reports' && reports.length > 0 && (
                <span className="ml-1 px-1.5 py-0.5 text-xs rounded-full bg-gray-100 text-gray-600">
                  {reports.length}
                </span>
              )}
            </button>
          ))}
        </nav>
      </div>

      {/* Tab Content */}
      <div className="card">
        {activeTab === 'overview' && (
          <div className="space-y-6">
            <div>
              <h3 className="text-lg font-semibold text-gray-900 mb-4">
                Node Information
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Certificate Name</p>
                  <p className="font-mono text-sm text-gray-900">{node.certname}</p>
                </div>
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Catalog Environment</p>
                  <p className="text-sm text-gray-900">
                    {node.catalog_environment || 'Not set'}
                  </p>
                </div>
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Facts Environment</p>
                  <p className="text-sm text-gray-900">
                    {node.facts_environment || 'Not set'}
                  </p>
                </div>
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Report Environment</p>
                  <p className="text-sm text-gray-900">
                    {node.report_environment || 'Not set'}
                  </p>
                </div>
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Catalog Timestamp</p>
                  <p className="text-sm text-gray-900">
                    {node.catalog_timestamp
                      ? new Date(node.catalog_timestamp).toLocaleString()
                      : 'Never'}
                  </p>
                </div>
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Facts Timestamp</p>
                  <p className="text-sm text-gray-900">
                    {node.facts_timestamp
                      ? new Date(node.facts_timestamp).toLocaleString()
                      : 'Never'}
                  </p>
                </div>
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Status</p>
                  <p className="text-sm text-gray-900 capitalize">
                    {node.latest_report_status || 'Unknown'}
                    {node.latest_report_corrective_change && ' (corrective)'}
                  </p>
                </div>
                <div className="p-3 bg-gray-50 rounded-lg">
                  <p className="text-sm text-gray-500">Cached Catalog</p>
                  <p className="text-sm text-gray-900">
                    {node.cached_catalog_status || 'Not cached'}
                  </p>
                </div>
              </div>
            </div>

            {/* Quick Facts */}
            <div>
              <h3 className="text-lg font-semibold text-gray-900 mb-4">
                Key Facts
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                {[
                  ['os.family', normalizedFacts.os && (normalizedFacts.os as Record<string, unknown>).family],
                  ['os.name', normalizedFacts.os && (normalizedFacts.os as Record<string, unknown>).name],
                  ['os.release.full', normalizedFacts.os && (normalizedFacts.os as Record<string, unknown>).release && ((normalizedFacts.os as Record<string, unknown>).release as Record<string, unknown>).full],
                  ['kernel', normalizedFacts.kernel],
                  ['kernelrelease', normalizedFacts.kernelrelease],
                  ['virtual', normalizedFacts.virtual],
                  ['is_virtual', normalizedFacts.is_virtual],
                  ['processors.count', normalizedFacts.processors && (normalizedFacts.processors as Record<string, unknown>).count],
                  ['memory.system.total', normalizedFacts.memory && (normalizedFacts.memory as Record<string, unknown>).system && ((normalizedFacts.memory as Record<string, unknown>).system as Record<string, unknown>).total],
                ]
                  .filter(([, value]) => value !== undefined)
                  .map(([key, value]) => (
                    <div key={key as string} className="p-3 bg-gray-50 rounded-lg">
                      <p className="text-xs text-gray-500">{key as string}</p>
                      <p className="text-sm font-medium text-gray-900">
                        {String(value)}
                      </p>
                    </div>
                  ))}
              </div>
            </div>
          </div>
        )}

        {activeTab === 'facts' && <FactsBrowser facts={normalizedFacts} />}

        {activeTab === 'reports' && (
          reportsLoading ? (
            <div className="flex items-center justify-center h-64">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
            </div>
          ) : (
            <ReportsTimeline reports={reports} />
          )
        )}

        {activeTab === 'groups' && (
          groupsLoading ? (
            <div className="flex items-center justify-center h-64">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
            </div>
          ) : (
            <GroupMembership certname={certname!} groups={groups} />
          )
        )}
      </div>
    </div>
  );
}
