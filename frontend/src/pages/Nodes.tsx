import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { Search, Filter, ChevronRight } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import { Node, NodeStatus } from '../types';

const statusColors: Record<NodeStatus, string> = {
  changed: 'bg-success-500',
  unchanged: 'bg-blue-500',
  failed: 'bg-danger-500',
  unreported: 'bg-warning-500',
  unknown: 'bg-gray-400',
};

const statusLabels: Record<NodeStatus, string> = {
  changed: 'Changed',
  unchanged: 'Unchanged',
  failed: 'Failed',
  unreported: 'Unreported',
  unknown: 'Unknown',
};

export default function Nodes() {
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<NodeStatus | 'all'>('all');

  const { data: nodes = [], isLoading } = useQuery({
    queryKey: ['nodes'],
    queryFn: api.getNodes,
  });

  const filteredNodes = nodes.filter((node: Node) => {
    const matchesSearch = node.certname
      .toLowerCase()
      .includes(search.toLowerCase());
    const matchesStatus =
      statusFilter === 'all' || node.latest_report_status === statusFilter;
    return matchesSearch && matchesStatus;
  });

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
        <span className="text-sm text-gray-500">
          {filteredNodes.length} nodes
        </span>
      </div>

      {/* Filters */}
      <div className="flex flex-col sm:flex-row gap-4 mb-6">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" />
          <input
            type="text"
            placeholder="Search nodes..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="input pl-10"
          />
        </div>
        <div className="relative">
          <Filter className="absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" />
          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value as NodeStatus | 'all')}
            className="input pl-10 pr-8"
          >
            <option value="all">All Statuses</option>
            {Object.entries(statusLabels).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
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
          <tbody className="bg-white divide-y divide-gray-200">
            {filteredNodes.map((node: Node) => (
              <tr key={node.certname} className="hover:bg-gray-50">
                <td className="px-6 py-4 whitespace-nowrap">
                  <span className="font-medium text-gray-900">
                    {node.certname}
                  </span>
                </td>
                <td className="px-6 py-4 whitespace-nowrap">
                  <span className="flex items-center">
                    <span
                      className={clsx(
                        'w-2 h-2 rounded-full mr-2',
                        statusColors[node.latest_report_status as NodeStatus] ||
                          statusColors.unknown
                      )}
                    />
                    <span className="text-sm text-gray-600">
                      {statusLabels[node.latest_report_status as NodeStatus] ||
                        'Unknown'}
                    </span>
                  </span>
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
            ))}
          </tbody>
        </table>

        {filteredNodes.length === 0 && (
          <div className="text-center py-12 text-gray-500">
            No nodes found matching your criteria
          </div>
        )}
      </div>
    </div>
  );
}
