import { useEffect, useState } from 'react';
import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { Search, Filter, ChevronRight, ChevronLeft, CheckCircle2, XCircle, Clock, HelpCircle, AlertTriangle, Plus } from 'lucide-react';
import clsx from 'clsx';
import { api, nodeRemovalApi } from '../services/api';
import { Node, NodeStatus, PendingNodeRemoval } from '../types';

// Page size for the server-side paginated node list.
const PAGE_SIZE = 100;

const statusLabels: Record<NodeStatus, string> = {
  changed: 'Changed',
  unchanged: 'Unchanged',
  failed: 'Failed',
  unreported: 'Unreported',
  unknown: 'Unknown',
};

// Status Badge Component
function StatusBadge({ status }: { status?: NodeStatus }) {
  if (!status) {
    return <span className="text-sm text-gray-400">-</span>;
  }

  const config: Record<
    NodeStatus,
    { icon: typeof CheckCircle2; color: string; bg: string; text: string }
  > = {
    changed: { icon: CheckCircle2, color: 'text-green-600', bg: 'bg-green-100', text: 'Changed' },
    unchanged: { icon: CheckCircle2, color: 'text-blue-600', bg: 'bg-blue-100', text: 'Unchanged' },
    failed: { icon: XCircle, color: 'text-red-600', bg: 'bg-red-100', text: 'Failed' },
    unreported: { icon: Clock, color: 'text-yellow-600', bg: 'bg-yellow-100', text: 'Unreported' },
    unknown: { icon: HelpCircle, color: 'text-gray-600', bg: 'bg-gray-100', text: 'Unknown' },
  };

  const { icon: Icon, color, bg, text } = config[status];

  return (
    <span className={clsx('inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-full', bg, color)}>
      <Icon className="w-3 h-3" />
      {text}
    </span>
  );
}

// Pending Removal Badge Component
function PendingRemovalBadge({ removal }: { removal: PendingNodeRemoval }) {
  const reasonLabels: Record<string, string> = {
    revoked_certificate: 'Revoked Certificate',
    no_certificate: 'No Certificate',
    manual: 'Manual',
  };

  const reasonLabel = reasonLabels[removal.removal_reason] || removal.removal_reason;
  const isOverdue = removal.is_overdue;
  const daysRemaining = removal.days_remaining;

  return (
    <span
      className={clsx(
        'inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-full',
        isOverdue ? 'bg-red-100 text-red-700' : 'bg-orange-100 text-orange-700'
      )}
      title={`Reason: ${reasonLabel}. ${isOverdue ? 'Overdue for removal' : `${daysRemaining} days until removal`}`}
    >
      <AlertTriangle className="w-3 h-3" />
      {isOverdue ? 'Pending Removal (Overdue)' : `Pending Removal (${daysRemaining}d)`}
    </span>
  );
}

export default function Nodes() {
  const [searchInput, setSearchInput] = useState('');
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<NodeStatus | 'all'>('all');
  const [page, setPage] = useState(0);

  // Debounce the search box so each keystroke doesn't trigger a query.
  useEffect(() => {
    const timer = setTimeout(() => setSearch(searchInput.trim()), 300);
    return () => clearTimeout(timer);
  }, [searchInput]);

  // Reset to the first page whenever the filters change.
  useEffect(() => {
    setPage(0);
  }, [search, statusFilter]);

  // Server-side paginated/filtered query. Filtering and pagination happen in
  // PuppetDB, so the full fleet is never pulled into the browser at once.
  const { data, isLoading, isFetching, isPlaceholderData } = useQuery({
    queryKey: ['nodes', { search, statusFilter, page }],
    queryFn: () =>
      api.getNodesPaginated({
        search: search || undefined,
        status: statusFilter === 'all' ? undefined : statusFilter,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      }),
    placeholderData: keepPreviousData,
  });

  const nodes = data?.nodes ?? [];
  const total = data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const rangeStart = total === 0 ? 0 : page * PAGE_SIZE + 1;
  const rangeEnd = page * PAGE_SIZE + nodes.length;

  // Fetch pending removals to show status on nodes
  const { data: pendingRemovals = [] } = useQuery({
    queryKey: ['pendingRemovals'],
    queryFn: nodeRemovalApi.listPendingRemovals,
    // Silently fail if the feature is not enabled
    retry: false,
  });

  // Create a lookup map for pending removals by certname
  const pendingRemovalMap = new Map<string, PendingNodeRemoval>(
    pendingRemovals.map((removal) => [removal.certname, removal])
  );

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-8">
        <h1 className="text-2xl font-bold text-gray-900">Nodes</h1>
        <div className="flex items-center gap-4">
          <span className="text-sm text-gray-500">
            {total.toLocaleString()} {total === 1 ? 'node' : 'nodes'}
          </span>
          <Link
            to="/nodes/add"
            className="btn btn-primary flex items-center gap-2"
          >
            <Plus className="w-4 h-4" />
            Add Node
          </Link>
        </div>
      </div>

      {/* Filters */}
      <div className="flex flex-col sm:flex-row gap-4 mb-6">
        <div className="flex-1">
          <div className="flex items-stretch border border-gray-300 rounded-lg bg-white hover:border-gray-400 focus-within:border-primary-600 focus-within:ring-1 focus-within:ring-primary-600 transition-colors">
            <div className="flex items-center justify-center px-3 py-2 border-r border-gray-200">
              <Search className="w-5 h-5 text-gray-400" />
            </div>
            <input
              type="text"
              placeholder="Search nodes..."
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              className="flex-1 px-3 py-2 outline-none bg-transparent rounded-r-lg"
            />
          </div>
        </div>
        <div className="sm:w-64">
          <div className="flex items-stretch border border-gray-300 rounded-lg bg-white hover:border-gray-400 focus-within:border-primary-600 focus-within:ring-1 focus-within:ring-primary-600 transition-colors relative">
            <div className="flex items-center justify-center px-3 py-2 border-r border-gray-200">
              <Filter className="w-5 h-5 text-gray-400" />
            </div>
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value as NodeStatus | 'all')}
              className="flex-1 px-3 py-2 pr-10 outline-none bg-transparent appearance-none rounded-r-lg"
            >
              <option value="all">All Statuses</option>
              {Object.entries(statusLabels).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
            <div className="absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none">
              <svg className="w-4 h-4 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
              </svg>
            </div>
          </div>
        </div>
      </div>

      {/* Node List */}
      <div className="card p-0 overflow-hidden">
        <table className="min-w-full divide-y divide-gray-200">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Node
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Status
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Environment
              </th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Last Report
              </th>
              <th className="px-6 py-3"></th>
            </tr>
          </thead>
          <tbody className={clsx('bg-white divide-y divide-gray-200', isFetching && isPlaceholderData && 'opacity-60')}>
            {nodes.map((node: Node) => {
              const pendingRemoval = pendingRemovalMap.get(node.certname);
              return (
                <tr key={node.certname} className={clsx('hover:bg-gray-50', pendingRemoval && 'bg-orange-50')}>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <span className="font-medium text-gray-900">
                      {node.certname}
                    </span>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <div className="flex flex-col gap-1">
                      <StatusBadge status={node.latest_report_status as NodeStatus} />
                      {pendingRemoval && <PendingRemovalBadge removal={pendingRemoval} />}
                    </div>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                    {node.catalog_environment || '-'}
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                    {node.report_timestamp
                      ? new Date(node.report_timestamp).toLocaleString()
                      : '-'}
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap text-right">
                    <Link
                      to={`/nodes/${node.certname}`}
                      className="text-primary-600 hover:text-primary-800"
                    >
                      <ChevronRight className="w-5 h-5" />
                    </Link>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>

        {nodes.length === 0 && (
          <div className="text-center py-12 text-gray-500">
            No nodes found matching your criteria
          </div>
        )}
      </div>

      {/* Pagination controls */}
      {total > 0 && (
        <div className="flex items-center justify-between mt-4">
          <span className="text-sm text-gray-500">
            Showing {rangeStart.toLocaleString()}–{rangeEnd.toLocaleString()} of{' '}
            {total.toLocaleString()}
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              disabled={page === 0}
              className="btn btn-secondary flex items-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <ChevronLeft className="w-4 h-4" />
              Previous
            </button>
            <span className="text-sm text-gray-600">
              Page {page + 1} of {totalPages}
            </span>
            <button
              type="button"
              onClick={() => setPage((p) => (p + 1 < totalPages ? p + 1 : p))}
              disabled={page + 1 >= totalPages}
              className="btn btn-secondary flex items-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Next
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
