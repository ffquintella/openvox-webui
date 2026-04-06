import { useState } from 'react';
import { Link } from 'react-router-dom';
import {
  PieChart,
  Pie,
  Cell,
  Legend,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts';
import {
  Package,
  AlertTriangle,
  Clock,
  Server,
  ChevronDown,
  ChevronRight,
  X,
  Loader2,
} from 'lucide-react';
import {
  useInventoryDashboard,
  useUpdateJobs,
  useOutdatedSoftwareNodes,
  useComplianceCategoryNodes,
} from '../../hooks/useUpdates';
import type { UpdateJobStatus } from '../../types';

const COMPLIANCE_COLORS: Record<string, string> = {
  Compliant: '#22c55e',
  Outdated: '#f97316',
  Stale: '#9ca3af',
};

const BAR_COLORS = ['#3b82f6', '#6366f1', '#8b5cf6', '#a855f7', '#ec4899', '#f43f5e', '#ef4444', '#f97316'];

function JobStatusBadge({ status }: { status: UpdateJobStatus }) {
  const config: Record<string, { label: string; className: string }> = {
    pending_approval: { label: 'Pending', className: 'bg-yellow-100 text-yellow-800' },
    approved: { label: 'Approved', className: 'bg-blue-100 text-blue-800' },
    rejected: { label: 'Rejected', className: 'bg-red-100 text-red-800' },
    in_progress: { label: 'In Progress', className: 'bg-indigo-100 text-indigo-800' },
    completed: { label: 'Completed', className: 'bg-green-100 text-green-800' },
    completed_with_failures: { label: 'Partial', className: 'bg-orange-100 text-orange-800' },
    cancelled: { label: 'Cancelled', className: 'bg-gray-100 text-gray-800' },
  };
  const c = config[status] || { label: status, className: 'bg-gray-100 text-gray-800' };
  return (
    <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${c.className}`}>
      {c.label}
    </span>
  );
}

function formatOperationType(op: string): string {
  return op.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function formatDate(dateString: string | null | undefined): string {
  if (!dateString) return '-';
  return new Date(dateString).toLocaleString();
}

export default function UpdatesTab() {
  const { data: dashboard, isLoading: dashboardLoading } = useInventoryDashboard();
  const { data: jobs = [], isLoading: jobsLoading } = useUpdateJobs();

  const [selectedSoftware, setSelectedSoftware] = useState<{
    name: string;
    softwareType: string;
  } | null>(null);
  const [selectedCompliance, setSelectedCompliance] = useState<string | null>(null);
  const [expandedJobId, setExpandedJobId] = useState<string | null>(null);

  const { data: softwareNodes = [], isLoading: softwareNodesLoading } =
    useOutdatedSoftwareNodes(
      selectedSoftware?.name ?? null,
      selectedSoftware?.softwareType
    );

  const { data: complianceNodes = [], isLoading: complianceNodesLoading } =
    useComplianceCategoryNodes(selectedCompliance);

  const summary = dashboard?.summary;
  const complianceData = dashboard?.update_compliance ?? [];
  const patchAgeData = dashboard?.patch_age_buckets ?? [];
  const platformData = dashboard?.platform_distribution ?? [];
  const osData = dashboard?.os_distribution ?? [];
  const topOutdated = dashboard?.top_outdated_software ?? [];

  if (dashboardLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Summary Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="card text-center">
          <div className="flex items-center justify-center mb-2">
            <Server className="w-5 h-5 text-primary-500" />
          </div>
          <p className="text-3xl font-bold text-gray-900">
            {summary?.nodes_with_inventory ?? 0}
          </p>
          <p className="text-sm text-gray-500">Nodes with Inventory</p>
        </div>
        <button
          className="card text-center hover:ring-2 hover:ring-orange-300 transition-all cursor-pointer"
          onClick={() => setSelectedCompliance('outdated')}
        >
          <div className="flex items-center justify-center mb-2">
            <Package className="w-5 h-5 text-orange-500" />
          </div>
          <p className="text-3xl font-bold text-orange-600">
            {summary?.outdated_nodes ?? 0}
          </p>
          <p className="text-sm text-gray-500">Outdated Nodes</p>
        </button>
        <div className="card text-center">
          <div className="flex items-center justify-center mb-2">
            <AlertTriangle className="w-5 h-5 text-red-500" />
          </div>
          <p className="text-3xl font-bold text-red-600">
            {(summary?.outdated_packages ?? 0) + (summary?.outdated_applications ?? 0)}
          </p>
          <p className="text-sm text-gray-500">Outdated Packages</p>
        </div>
        <button
          className="card text-center hover:ring-2 hover:ring-gray-300 transition-all cursor-pointer"
          onClick={() => setSelectedCompliance('stale')}
        >
          <div className="flex items-center justify-center mb-2">
            <Clock className="w-5 h-5 text-gray-500" />
          </div>
          <p className="text-3xl font-bold text-gray-600">
            {summary?.stale_nodes ?? 0}
          </p>
          <p className="text-sm text-gray-500">Stale Nodes</p>
        </button>
      </div>

      {/* Charts Row 1 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Compliance Donut */}
        <div className="card">
          <h3 className="text-lg font-semibold text-gray-900 mb-4">Update Compliance</h3>
          {complianceData.length === 0 || complianceData.every((d) => d.value === 0) ? (
            <div className="h-64 flex items-center justify-center text-gray-500">
              No compliance data available
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={280}>
              <PieChart>
                <Pie
                  data={complianceData}
                  cx="50%"
                  cy="50%"
                  innerRadius={60}
                  outerRadius={100}
                  dataKey="value"
                  nameKey="label"
                  onClick={(_data, idx) => {
                    const cat = complianceData[idx]?.label?.toLowerCase();
                    if (cat && ['compliant', 'outdated', 'stale'].includes(cat)) {
                      setSelectedCompliance(cat);
                    }
                  }}
                  cursor="pointer"
                >
                  {complianceData.map((entry, idx) => (
                    <Cell
                      key={idx}
                      fill={COMPLIANCE_COLORS[entry.label] ?? '#d1d5db'}
                    />
                  ))}
                </Pie>
                <Tooltip formatter={(value: number, name: string) => [String(value), name]} />
                <Legend
                  formatter={(value: string) => {
                    const entry = complianceData.find((d) => d.label === value);
                    return `${value}: ${entry?.value ?? 0}`;
                  }}
                />
              </PieChart>
            </ResponsiveContainer>
          )}
          <div className="flex justify-center gap-6 mt-2">
            {['Compliant', 'Outdated', 'Stale']
              .filter((cat) => !complianceData.some((d) => d.label === cat))
              .map((cat) => (
                <button
                  key={cat}
                  className="flex items-center gap-2 text-sm text-gray-400"
                  onClick={() => setSelectedCompliance(cat.toLowerCase())}
                >
                  <div
                    className="w-3 h-3 rounded-full"
                    style={{ backgroundColor: COMPLIANCE_COLORS[cat] ?? '#d1d5db' }}
                  />
                  {cat}: 0
                </button>
              ))}
          </div>
        </div>

        {/* Patch Age */}
        <div className="card">
          <h3 className="text-lg font-semibold text-gray-900 mb-4">Patch Age Distribution</h3>
          {patchAgeData.length === 0 ? (
            <div className="h-64 flex items-center justify-center text-gray-500">
              No patch age data available
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={patchAgeData}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="label" tick={{ fontSize: 12 }} />
                <YAxis allowDecimals={false} />
                <Tooltip />
                <Bar dataKey="value" name="Nodes" fill="#6366f1" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          )}
        </div>
      </div>

      {/* Charts Row 2 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Platform Distribution */}
        <div className="card">
          <h3 className="text-lg font-semibold text-gray-900 mb-4">Platform Distribution</h3>
          {platformData.length === 0 ? (
            <div className="h-64 flex items-center justify-center text-gray-500">
              No platform data available
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={platformData} layout="vertical">
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis type="number" allowDecimals={false} />
                <YAxis type="category" dataKey="label" width={120} tick={{ fontSize: 12 }} />
                <Tooltip />
                <Bar dataKey="value" name="Nodes">
                  {platformData.map((_, idx) => (
                    <Cell key={idx} fill={BAR_COLORS[idx % BAR_COLORS.length]} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          )}
        </div>

        {/* OS Distribution */}
        <div className="card">
          <h3 className="text-lg font-semibold text-gray-900 mb-4">OS Distribution</h3>
          {osData.length === 0 ? (
            <div className="h-64 flex items-center justify-center text-gray-500">
              No OS data available
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={osData} layout="vertical">
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis type="number" allowDecimals={false} />
                <YAxis type="category" dataKey="label" width={150} tick={{ fontSize: 12 }} />
                <Tooltip />
                <Bar dataKey="value" name="Nodes">
                  {osData.map((_, idx) => (
                    <Cell key={idx} fill={BAR_COLORS[idx % BAR_COLORS.length]} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          )}
        </div>
      </div>

      {/* Top Outdated Software */}
      <div className="card">
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Top Outdated Software</h3>
        {topOutdated.length === 0 ? (
          <div className="h-32 flex items-center justify-center text-gray-500">
            <div className="text-center">
              <Package className="w-10 h-10 mx-auto mb-2 text-gray-300" />
              <p>No outdated software detected</p>
            </div>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">#</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Software</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
                  <th className="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase">Affected Nodes</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {topOutdated.map((item, idx) => (
                  <tr
                    key={`${item.software_type}-${item.name}`}
                    className="hover:bg-gray-50 cursor-pointer transition-colors"
                    onClick={() =>
                      setSelectedSoftware({
                        name: item.name,
                        softwareType: item.software_type,
                      })
                    }
                  >
                    <td className="px-4 py-3 text-sm text-gray-500">{idx + 1}</td>
                    <td className="px-4 py-3 text-sm font-medium text-gray-900">{item.name}</td>
                    <td className="px-4 py-3 text-sm text-gray-500">{item.software_type}</td>
                    <td className="px-4 py-3 text-sm text-right font-semibold text-orange-600">
                      {item.affected_nodes}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Update Job History */}
      <div className="card">
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Recent Update Jobs</h3>
        {jobsLoading ? (
          <div className="h-32 flex items-center justify-center">
            <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary-600" />
          </div>
        ) : jobs.length === 0 ? (
          <div className="h-32 flex items-center justify-center text-gray-500">
            No update jobs found
          </div>
        ) : (
          <div className="space-y-2">
            {jobs.slice(0, 20).map((job) => (
              <div key={job.id} className="border rounded-lg">
                <button
                  className="w-full px-4 py-3 flex items-center justify-between hover:bg-gray-50 transition-colors text-left"
                  onClick={() =>
                    setExpandedJobId(expandedJobId === job.id ? null : job.id)
                  }
                >
                  <div className="flex items-center gap-3">
                    {expandedJobId === job.id ? (
                      <ChevronDown className="w-4 h-4 text-gray-400" />
                    ) : (
                      <ChevronRight className="w-4 h-4 text-gray-400" />
                    )}
                    <JobStatusBadge status={job.status} />
                    <span className="text-sm font-medium text-gray-900">
                      {formatOperationType(job.operation_type)}
                    </span>
                    {job.package_names && job.package_names.length > 0 && (
                      <span className="text-xs text-gray-500">
                        ({job.package_names.length} package{job.package_names.length !== 1 ? 's' : ''})
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-4 text-sm text-gray-500">
                    <span>{job.targets?.length ?? 0} target{(job.targets?.length ?? 0) !== 1 ? 's' : ''}</span>
                    <span>{formatDate(job.created_at)}</span>
                  </div>
                </button>

                {expandedJobId === job.id && job.targets && job.targets.length > 0 && (
                  <div className="px-4 pb-3 border-t bg-gray-50">
                    <table className="min-w-full mt-2">
                      <thead>
                        <tr className="text-xs text-gray-500 uppercase">
                          <th className="text-left py-2 px-2">Node</th>
                          <th className="text-left py-2 px-2">Status</th>
                          <th className="text-left py-2 px-2">Dispatched</th>
                          <th className="text-left py-2 px-2">Completed</th>
                          <th className="text-left py-2 px-2">Error</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-gray-200">
                        {job.targets.map((target) => (
                          <tr key={target.id} className="text-sm">
                            <td className="py-2 px-2">
                              <Link
                                to={`/nodes/${encodeURIComponent(target.certname)}`}
                                className="text-primary-600 hover:underline font-mono"
                              >
                                {target.certname}
                              </Link>
                            </td>
                            <td className="py-2 px-2">
                              <JobStatusBadge status={target.status as UpdateJobStatus} />
                            </td>
                            <td className="py-2 px-2 text-gray-500">{formatDate(target.dispatched_at)}</td>
                            <td className="py-2 px-2 text-gray-500">{formatDate(target.completed_at)}</td>
                            <td className="py-2 px-2 text-red-600 text-xs">{target.last_error || '-'}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Outdated Software Drill-Down Modal */}
      {selectedSoftware && (
        <div
          className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
          onClick={() => setSelectedSoftware(null)}
        >
          <div
            className="bg-white rounded-xl shadow-xl max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-6 py-4 border-b">
              <h3 className="text-lg font-semibold text-gray-900">
                Nodes with outdated "{selectedSoftware.name}"
              </h3>
              <button
                onClick={() => setSelectedSoftware(null)}
                className="p-1 hover:bg-gray-100 rounded"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
            <div className="overflow-y-auto p-6">
              {softwareNodesLoading ? (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="w-6 h-6 animate-spin text-primary-600" />
                </div>
              ) : softwareNodes.length === 0 ? (
                <p className="text-center text-gray-500 py-8">No affected nodes found</p>
              ) : (
                <table className="min-w-full divide-y divide-gray-200">
                  <thead className="bg-gray-50">
                    <tr>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Node</th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Installed</th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Latest</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-200">
                    {softwareNodes.map((node) => (
                      <tr key={node.certname} className="hover:bg-gray-50">
                        <td className="px-4 py-3">
                          <Link
                            to={`/nodes/${encodeURIComponent(node.certname)}`}
                            className="text-primary-600 hover:underline font-mono text-sm"
                            onClick={() => setSelectedSoftware(null)}
                          >
                            {node.certname}
                          </Link>
                        </td>
                        <td className="px-4 py-3 text-sm font-mono text-red-600">
                          {node.installed_version}
                        </td>
                        <td className="px-4 py-3 text-sm font-mono text-green-600">
                          {node.latest_version}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Compliance Category Drill-Down Modal */}
      {selectedCompliance && (
        <div
          className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
          onClick={() => setSelectedCompliance(null)}
        >
          <div
            className="bg-white rounded-xl shadow-xl max-w-3xl w-full mx-4 max-h-[80vh] flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-6 py-4 border-b">
              <h3 className="text-lg font-semibold text-gray-900">
                {selectedCompliance.charAt(0).toUpperCase() + selectedCompliance.slice(1)} Nodes
              </h3>
              <button
                onClick={() => setSelectedCompliance(null)}
                className="p-1 hover:bg-gray-100 rounded"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
            <div className="overflow-y-auto p-6">
              {complianceNodesLoading ? (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="w-6 h-6 animate-spin text-primary-600" />
                </div>
              ) : complianceNodes.length === 0 ? (
                <p className="text-center text-gray-500 py-8">No nodes in this category</p>
              ) : (
                <table className="min-w-full divide-y divide-gray-200">
                  <thead className="bg-gray-50">
                    <tr>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Node</th>
                      <th className="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase">Outdated Pkgs</th>
                      <th className="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase">Outdated Apps</th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Last Checked</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-200">
                    {complianceNodes.map((node) => (
                      <tr key={node.certname} className="hover:bg-gray-50">
                        <td className="px-4 py-3">
                          <Link
                            to={`/nodes/${encodeURIComponent(node.certname)}`}
                            className="text-primary-600 hover:underline font-mono text-sm"
                            onClick={() => setSelectedCompliance(null)}
                          >
                            {node.certname}
                          </Link>
                        </td>
                        <td className="px-4 py-3 text-sm text-right">
                          {node.outdated_packages > 0 ? (
                            <span className="text-orange-600 font-semibold">{node.outdated_packages}</span>
                          ) : (
                            <span className="text-green-600">0</span>
                          )}
                        </td>
                        <td className="px-4 py-3 text-sm text-right">
                          {node.outdated_applications > 0 ? (
                            <span className="text-orange-600 font-semibold">{node.outdated_applications}</span>
                          ) : (
                            <span className="text-green-600">0</span>
                          )}
                        </td>
                        <td className="px-4 py-3 text-sm text-gray-500">
                          {formatDate(node.checked_at)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
