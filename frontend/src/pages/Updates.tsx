import { useState, useMemo } from 'react';
import { Link } from 'react-router-dom';
import {
  RefreshCw,
  Package,
  Shield,
  ShieldAlert,
  AlertTriangle,
  Play,
  Search,
  ChevronDown,
  ChevronRight,
  Eye,
  X,
  Loader2,
} from 'lucide-react';
import {
  useUpdateJobs,
  useInventoryDashboard,
  useInventoryCatalog,
  useCreateUpdateJob,
  useApproveUpdateJob,
  useCancelUpdateJob,
  usePreviewUpdateJob,
  useOutdatedSoftwareNodes,
  useComplianceCategoryNodes,
} from '../hooks/useUpdates';
import {
  useVulnerabilityDashboard,
  useCveSearch,
} from '../hooks/useCve';
import { useGroups } from '../hooks/useGroups';
import type {
  UpdateJobStatus,
  UpdateOperationType,
  CreateUpdateJobRequest,
  UpdatePreviewResponse,
} from '../types';

type TabId = 'status' | 'jobs' | 'catalog' | 'vulnerabilities';

interface Tab {
  id: TabId;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}

const TABS: Tab[] = [
  { id: 'status', label: 'Update Status', icon: Package },
  { id: 'jobs', label: 'Update Jobs', icon: Play },
  { id: 'catalog', label: 'Version Catalog', icon: RefreshCw },
  { id: 'vulnerabilities', label: 'Vulnerabilities', icon: ShieldAlert },
];

function formatDate(dateString: string | null | undefined): string {
  if (!dateString) return 'Never';
  return new Date(dateString).toLocaleString();
}

function StatusBadge({ status }: { status: UpdateJobStatus }) {
  const config: Record<UpdateJobStatus, { label: string; className: string }> = {
    pending_approval: { label: 'Pending Approval', className: 'bg-yellow-100 text-yellow-800' },
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

function SeverityBadge({ severity, count }: { severity: string; count?: number }) {
  const colors: Record<string, string> = {
    critical: 'bg-red-100 text-red-800',
    high: 'bg-orange-100 text-orange-800',
    medium: 'bg-yellow-100 text-yellow-800',
    low: 'bg-blue-100 text-blue-800',
    unknown: 'bg-gray-100 text-gray-800',
  };
  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${colors[severity] || colors.unknown}`}>
      {severity}{count !== undefined ? ` (${count})` : ''}
    </span>
  );
}

// ============================================================================
// Update Status Tab
// ============================================================================

function UpdateStatusTab({ onSwitchTab }: { onSwitchTab: (tab: TabId) => void }) {
  const { data: dashboard, isLoading } = useInventoryDashboard();
  const { data: vulnDashboard } = useVulnerabilityDashboard();
  const [showDispatcher, setShowDispatcher] = useState(false);
  const [selectedSoftware, setSelectedSoftware] = useState<{ name: string; softwareType: string } | null>(null);
  const [selectedCompliance, setSelectedCompliance] = useState<string | null>(null);

  const { data: softwareNodes = [], isLoading: softwareNodesLoading } =
    useOutdatedSoftwareNodes(selectedSoftware?.name ?? null, selectedSoftware?.softwareType);
  const { data: complianceNodes = [], isLoading: complianceNodesLoading } =
    useComplianceCategoryNodes(selectedCompliance);

  if (isLoading) {
    return <div className="flex justify-center py-12"><RefreshCw className="h-8 w-8 animate-spin text-gray-400" /></div>;
  }

  const summary = dashboard?.summary;

  return (
    <div className="space-y-6">
      {/* Summary cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="bg-white rounded-lg border p-4">
          <div className="text-sm font-medium text-gray-500">Nodes with Inventory</div>
          <div className="mt-1 text-2xl font-semibold">{summary?.nodes_with_inventory ?? 0}</div>
        </div>
        <button
          className="bg-white rounded-lg border p-4 text-left hover:ring-2 hover:ring-orange-300 transition-all cursor-pointer"
          onClick={() => setSelectedCompliance('outdated')}
        >
          <div className="text-sm font-medium text-gray-500">Outdated Nodes</div>
          <div className="mt-1 text-2xl font-semibold text-orange-600">{summary?.outdated_nodes ?? 0}</div>
        </button>
        <button
          className="bg-white rounded-lg border p-4 text-left hover:ring-2 hover:ring-orange-300 transition-all cursor-pointer"
          onClick={() => setSelectedCompliance('outdated')}
        >
          <div className="text-sm font-medium text-gray-500">Outdated Packages</div>
          <div className="mt-1 text-2xl font-semibold text-orange-600">{summary?.outdated_packages ?? 0}</div>
        </button>
        <button
          className="bg-white rounded-lg border p-4 text-left hover:ring-2 hover:ring-red-300 transition-all cursor-pointer"
          onClick={() => onSwitchTab('vulnerabilities')}
        >
          <div className="text-sm font-medium text-gray-500">Vulnerable Nodes</div>
          <div className="mt-1 text-2xl font-semibold text-red-600">{vulnDashboard?.total_vulnerable_nodes ?? 0}</div>
        </button>
      </div>

      {/* Action bar */}
      <div className="flex justify-between items-center">
        <h3 className="text-lg font-medium text-gray-900">Update Management</h3>
        <button
          onClick={() => setShowDispatcher(!showDispatcher)}
          className="inline-flex items-center px-4 py-2 border border-transparent rounded-md shadow-sm text-sm font-medium text-white bg-primary-600 hover:bg-primary-700"
        >
          <Play className="h-4 w-4 mr-2" />
          Dispatch Updates
        </button>
      </div>

      {showDispatcher && <UpdateDispatcher onClose={() => setShowDispatcher(false)} />}

      {/* Top outdated software */}
      {dashboard?.top_outdated_software && dashboard.top_outdated_software.length > 0 && (
        <div className="bg-white rounded-lg border">
          <div className="px-4 py-3 border-b">
            <h4 className="font-medium text-gray-900">Top Outdated Software</h4>
          </div>
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Software</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Affected Nodes</th>
                </tr>
              </thead>
              <tbody className="bg-white divide-y divide-gray-200">
                {dashboard.top_outdated_software.map((item) => (
                  <tr
                    key={`${item.software_type}-${item.name}`}
                    className="hover:bg-gray-50 cursor-pointer transition-colors"
                    onClick={() => setSelectedSoftware({ name: item.name, softwareType: item.software_type })}
                  >
                    <td className="px-4 py-3 text-sm text-gray-900">{item.name}</td>
                    <td className="px-4 py-3 text-sm text-gray-500">{item.software_type}</td>
                    <td className="px-4 py-3 text-sm font-semibold text-orange-600">{item.affected_nodes}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

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
                Nodes with outdated &ldquo;{selectedSoftware.name}&rdquo;
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

// ============================================================================
// Update Dispatcher
// ============================================================================

type TargetMode = 'single' | 'group' | 'all_outdated';
type UpdateScope = 'all' | 'security' | 'select';

function UpdateDispatcher({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState(1);
  const [targetMode, setTargetMode] = useState<TargetMode>('single');
  const [selectedCertname, setSelectedCertname] = useState('');
  const [selectedGroupId, setSelectedGroupId] = useState('');
  const [updateScope, setUpdateScope] = useState<UpdateScope>('all');
  const [selectedPackages, setSelectedPackages] = useState<string[]>([]);
  const [scheduleMode, setScheduleMode] = useState<'now' | 'later'>('now');
  const [scheduledFor, setScheduledFor] = useState('');
  const [requiresApproval, setRequiresApproval] = useState(false);
  const [preview, setPreview] = useState<UpdatePreviewResponse | null>(null);

  const { data: groups } = useGroups();
  const createJob = useCreateUpdateJob();
  const previewJob = usePreviewUpdateJob();

  const operationType: UpdateOperationType =
    updateScope === 'all' ? 'system_patch' :
    updateScope === 'security' ? 'security_patch' : 'package_update';

  const buildRequest = (): CreateUpdateJobRequest => ({
    operation_type: operationType,
    package_names: updateScope === 'select' ? selectedPackages : [],
    certnames: targetMode === 'single' ? [selectedCertname] : [],
    group_id: targetMode === 'group' ? selectedGroupId : undefined,
    requires_approval: requiresApproval,
    scheduled_for: scheduleMode === 'later' ? scheduledFor : undefined,
  });

  const handlePreview = async () => {
    try {
      const result = await previewJob.mutateAsync({
        operation_type: operationType,
        package_names: updateScope === 'select' ? selectedPackages : [],
        certnames: targetMode === 'single' ? [selectedCertname] : [],
        group_id: targetMode === 'group' ? selectedGroupId : undefined,
      });
      setPreview(result);
    } catch { /* error handled by mutation */ }
  };

  const handleSubmit = async () => {
    try {
      await createJob.mutateAsync(buildRequest());
      onClose();
    } catch { /* error handled by mutation */ }
  };

  return (
    <div className="bg-white rounded-lg border shadow-sm p-6 space-y-6">
      <div className="flex justify-between items-center">
        <h3 className="text-lg font-medium text-gray-900">Dispatch Updates</h3>
        <button onClick={onClose} className="text-gray-400 hover:text-gray-600">&times;</button>
      </div>

      {/* Step indicators */}
      <div className="flex space-x-4 text-sm">
        {[1, 2, 3, 4].map(s => (
          <button
            key={s}
            onClick={() => setStep(s)}
            className={`px-3 py-1 rounded ${step === s ? 'bg-primary-100 text-primary-700 font-medium' : 'text-gray-500 hover:text-gray-700'}`}
          >
            Step {s}
          </button>
        ))}
      </div>

      {/* Step 1: Target Selection */}
      {step === 1 && (
        <div className="space-y-4">
          <h4 className="font-medium text-gray-700">Select Targets</h4>
          <div className="space-y-3">
            <label className="flex items-center space-x-3 cursor-pointer">
              <input type="radio" checked={targetMode === 'single'} onChange={() => setTargetMode('single')} className="text-primary-600" />
              <span className="text-sm">Single node</span>
            </label>
            {targetMode === 'single' && (
              <input
                type="text"
                value={selectedCertname}
                onChange={e => setSelectedCertname(e.target.value)}
                placeholder="Enter node certname..."
                className="ml-7 block w-full max-w-md rounded-md border-gray-300 shadow-sm focus:border-primary-500 focus:ring-primary-500 text-sm"
              />
            )}

            <label className="flex items-center space-x-3 cursor-pointer">
              <input type="radio" checked={targetMode === 'group'} onChange={() => setTargetMode('group')} className="text-primary-600" />
              <span className="text-sm">Node group</span>
            </label>
            {targetMode === 'group' && (
              <select
                value={selectedGroupId}
                onChange={e => setSelectedGroupId(e.target.value)}
                className="ml-7 block w-full max-w-md rounded-md border-gray-300 shadow-sm focus:border-primary-500 focus:ring-primary-500 text-sm"
              >
                <option value="">Select a group...</option>
                {groups?.map(g => (
                  <option key={g.id} value={g.id}>{g.name}</option>
                ))}
              </select>
            )}

            <label className="flex items-center space-x-3 cursor-pointer">
              <input type="radio" checked={targetMode === 'all_outdated'} onChange={() => setTargetMode('all_outdated')} className="text-primary-600" />
              <span className="text-sm">All outdated nodes</span>
            </label>
          </div>
          <div className="flex justify-end">
            <button onClick={() => setStep(2)} className="px-4 py-2 bg-primary-600 text-white rounded-md text-sm hover:bg-primary-700">
              Next
            </button>
          </div>
        </div>
      )}

      {/* Step 2: Update Scope */}
      {step === 2 && (
        <div className="space-y-4">
          <h4 className="font-medium text-gray-700">Update Scope</h4>
          <div className="space-y-3">
            <label className="flex items-start space-x-3 cursor-pointer p-3 rounded-lg border hover:bg-gray-50">
              <input type="radio" checked={updateScope === 'all'} onChange={() => setUpdateScope('all')} className="mt-0.5 text-primary-600" />
              <div>
                <div className="text-sm font-medium">All updates</div>
                <div className="text-xs text-gray-500">Apply all available package and OS updates</div>
              </div>
            </label>

            <label className="flex items-start space-x-3 cursor-pointer p-3 rounded-lg border hover:bg-gray-50">
              <input type="radio" checked={updateScope === 'security'} onChange={() => setUpdateScope('security')} className="mt-0.5 text-primary-600" />
              <div>
                <div className="text-sm font-medium flex items-center gap-2">
                  Security updates only
                  <Shield className="h-3.5 w-3.5 text-red-500" />
                </div>
                <div className="text-xs text-gray-500">Update only packages with known CVE vulnerabilities</div>
              </div>
            </label>

            <label className="flex items-start space-x-3 cursor-pointer p-3 rounded-lg border hover:bg-gray-50">
              <input type="radio" checked={updateScope === 'select'} onChange={() => setUpdateScope('select')} className="mt-0.5 text-primary-600" />
              <div>
                <div className="text-sm font-medium">Select packages</div>
                <div className="text-xs text-gray-500">Choose specific packages to update</div>
              </div>
            </label>
          </div>

          {updateScope === 'select' && (
            <div className="ml-7">
              <textarea
                value={selectedPackages.join('\n')}
                onChange={e => setSelectedPackages(e.target.value.split('\n').filter(Boolean))}
                placeholder="Enter package names (one per line)..."
                rows={4}
                className="block w-full max-w-md rounded-md border-gray-300 shadow-sm focus:border-primary-500 focus:ring-primary-500 text-sm"
              />
            </div>
          )}

          <div className="flex justify-between">
            <button onClick={() => setStep(1)} className="px-4 py-2 text-gray-700 border rounded-md text-sm hover:bg-gray-50">Back</button>
            <button onClick={() => setStep(3)} className="px-4 py-2 bg-primary-600 text-white rounded-md text-sm hover:bg-primary-700">Next</button>
          </div>
        </div>
      )}

      {/* Step 3: Scheduling & Safety */}
      {step === 3 && (
        <div className="space-y-4">
          <h4 className="font-medium text-gray-700">Scheduling & Safety</h4>

          <div className="space-y-3">
            <label className="flex items-center space-x-3">
              <input type="radio" checked={scheduleMode === 'now'} onChange={() => setScheduleMode('now')} className="text-primary-600" />
              <span className="text-sm">Execute now</span>
            </label>
            <label className="flex items-center space-x-3">
              <input type="radio" checked={scheduleMode === 'later'} onChange={() => setScheduleMode('later')} className="text-primary-600" />
              <span className="text-sm">Schedule for later</span>
            </label>
            {scheduleMode === 'later' && (
              <input
                type="datetime-local"
                value={scheduledFor}
                onChange={e => setScheduledFor(e.target.value)}
                className="ml-7 rounded-md border-gray-300 shadow-sm focus:border-primary-500 focus:ring-primary-500 text-sm"
              />
            )}
          </div>

          <label className="flex items-center space-x-3">
            <input
              type="checkbox"
              checked={requiresApproval}
              onChange={e => setRequiresApproval(e.target.checked)}
              className="rounded text-primary-600"
            />
            <span className="text-sm">Require approval before execution</span>
          </label>

          <div className="flex justify-between items-center">
            <button onClick={() => setStep(2)} className="px-4 py-2 text-gray-700 border rounded-md text-sm hover:bg-gray-50">Back</button>
            <div className="flex space-x-3">
              <button
                onClick={handlePreview}
                disabled={previewJob.isPending}
                className="inline-flex items-center px-4 py-2 border border-gray-300 rounded-md text-sm text-gray-700 hover:bg-gray-50"
              >
                <Eye className="h-4 w-4 mr-2" />
                {previewJob.isPending ? 'Loading...' : 'Preview'}
              </button>
              <button onClick={() => setStep(4)} className="px-4 py-2 bg-primary-600 text-white rounded-md text-sm hover:bg-primary-700">Next</button>
            </div>
          </div>

          {preview && (
            <PreviewResult preview={preview} />
          )}
        </div>
      )}

      {/* Step 4: Confirmation */}
      {step === 4 && (
        <div className="space-y-4">
          <h4 className="font-medium text-gray-700">Confirm Update Job</h4>

          <div className="bg-gray-50 rounded-lg p-4 space-y-2 text-sm">
            <div><span className="font-medium">Scope:</span> {updateScope === 'all' ? 'All updates' : updateScope === 'security' ? 'Security updates only' : `Selected packages (${selectedPackages.length})`}</div>
            <div><span className="font-medium">Target:</span> {targetMode === 'single' ? selectedCertname : targetMode === 'group' ? `Group: ${groups?.find(g => g.id === selectedGroupId)?.name || selectedGroupId}` : 'All outdated nodes'}</div>
            <div><span className="font-medium">Schedule:</span> {scheduleMode === 'now' ? 'Immediate' : `Scheduled: ${scheduledFor}`}</div>
            <div><span className="font-medium">Approval required:</span> {requiresApproval ? 'Yes' : 'No'}</div>
          </div>

          <div className="flex justify-between">
            <button onClick={() => setStep(3)} className="px-4 py-2 text-gray-700 border rounded-md text-sm hover:bg-gray-50">Back</button>
            <button
              onClick={handleSubmit}
              disabled={createJob.isPending}
              className="inline-flex items-center px-4 py-2 bg-primary-600 text-white rounded-md text-sm hover:bg-primary-700 disabled:opacity-50"
            >
              {createJob.isPending ? 'Creating...' : 'Create Update Job'}
            </button>
          </div>

          {createJob.isError && (
            <div className="bg-red-50 border border-red-200 rounded-md p-3 text-sm text-red-700">
              Failed to create update job. Please try again.
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function PreviewResult({ preview }: { preview: UpdatePreviewResponse }) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  return (
    <div className="bg-blue-50 border border-blue-200 rounded-lg p-4 space-y-3">
      <div className="text-sm font-medium text-blue-900">
        Preview: {preview.total_nodes} node(s), {preview.total_packages} package(s) to update
      </div>
      {preview.targets.map(target => (
        <div key={target.certname} className="text-sm">
          <button
            onClick={() => setExpanded(prev => ({ ...prev, [target.certname]: !prev[target.certname] }))}
            className="flex items-center gap-1 font-medium text-blue-800 hover:text-blue-600"
          >
            {expanded[target.certname] ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
            {target.certname} ({target.packages_to_update.length} packages)
          </button>
          {expanded[target.certname] && (
            <div className="ml-5 mt-1 space-y-1">
              {target.packages_to_update.map((pkg, idx) => (
                <div key={idx} className="flex items-center gap-2 text-xs text-blue-700">
                  <span className="font-mono">{pkg.name}</span>
                  <span>{pkg.from_version} &rarr; {pkg.to_version}</span>
                  {pkg.cve_ids.length > 0 && (
                    <span className="text-red-600">({pkg.cve_ids.length} CVE{pkg.cve_ids.length !== 1 ? 's' : ''})</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

// ============================================================================
// Update Jobs Tab
// ============================================================================

function UpdateJobsTab() {
  const { data: jobs, isLoading } = useUpdateJobs();
  const approveJob = useApproveUpdateJob();
  const cancelJob = useCancelUpdateJob();
  const [expandedJob, setExpandedJob] = useState<string | null>(null);

  if (isLoading) {
    return <div className="flex justify-center py-12"><RefreshCw className="h-8 w-8 animate-spin text-gray-400" /></div>;
  }

  if (!jobs || jobs.length === 0) {
    return (
      <div className="text-center py-12 text-gray-500">
        <Package className="h-12 w-12 mx-auto mb-4 text-gray-300" />
        <p>No update jobs yet. Use the Update Status tab to dispatch updates.</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {jobs.map(job => (
        <div key={job.id} className="bg-white rounded-lg border">
          <div
            className="px-4 py-3 flex items-center justify-between cursor-pointer hover:bg-gray-50"
            onClick={() => setExpandedJob(expandedJob === job.id ? null : job.id)}
          >
            <div className="flex items-center gap-4">
              {expandedJob === job.id ? <ChevronDown className="h-4 w-4 text-gray-400" /> : <ChevronRight className="h-4 w-4 text-gray-400" />}
              <div>
                <div className="text-sm font-medium text-gray-900">
                  {job.operation_type.replace(/_/g, ' ')} &mdash; {job.target_nodes.length} node(s)
                </div>
                <div className="text-xs text-gray-500">
                  Created {formatDate(job.created_at)} by {job.requested_by}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <StatusBadge status={job.status} />
              {job.status === 'pending_approval' && (
                <div className="flex gap-2">
                  <button
                    onClick={e => { e.stopPropagation(); approveJob.mutate({ jobId: job.id, request: { approved: true } }); }}
                    className="px-3 py-1 bg-green-600 text-white text-xs rounded hover:bg-green-700"
                  >
                    Approve
                  </button>
                  <button
                    onClick={e => { e.stopPropagation(); approveJob.mutate({ jobId: job.id, request: { approved: false } }); }}
                    className="px-3 py-1 bg-red-600 text-white text-xs rounded hover:bg-red-700"
                  >
                    Reject
                  </button>
                </div>
              )}
              {(job.status === 'pending_approval' || job.status === 'approved' || job.status === 'in_progress') && (
                <button
                  onClick={e => {
                    e.stopPropagation();
                    if (window.confirm('Cancel this update job? All pending and in-progress targets will be marked as cancelled.')) {
                      cancelJob.mutate(job.id);
                    }
                  }}
                  disabled={cancelJob.isPending}
                  className="px-3 py-1 bg-gray-500 text-white text-xs rounded hover:bg-gray-600 disabled:opacity-50"
                >
                  Cancel
                </button>
              )}
            </div>
          </div>

          {expandedJob === job.id && (
            <div className="px-4 py-3 border-t bg-gray-50 text-sm space-y-2">
              <div className="grid grid-cols-2 gap-4">
                <div><span className="font-medium">Operation:</span> {job.operation_type}</div>
                <div><span className="font-medium">Requires Approval:</span> {job.requires_approval ? 'Yes' : 'No'}</div>
                {job.approved_by && <div><span className="font-medium">Approved by:</span> {job.approved_by}</div>}
                {job.scheduled_for && <div><span className="font-medium">Scheduled for:</span> {formatDate(job.scheduled_for)}</div>}
              </div>
              {job.package_names.length > 0 && (
                <div><span className="font-medium">Packages:</span> {job.package_names.join(', ')}</div>
              )}
              {job.targets.length > 0 && (
                <div>
                  <span className="font-medium">Targets:</span>
                  <div className="mt-1 space-y-1">
                    {job.targets.map(target => (
                      <div key={target.id} className="flex items-center gap-2 text-xs">
                        <span className="font-mono">{target.certname}</span>
                        <StatusBadge status={target.status as UpdateJobStatus} />
                        {target.last_error && <span className="text-red-600">{target.last_error}</span>}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

// ============================================================================
// Version Catalog Tab
// ============================================================================

function VersionCatalogTab() {
  const { data: catalog, isLoading } = useInventoryCatalog();
  const [search, setSearch] = useState('');
  const [platformFilter, setPlatformFilter] = useState('');

  const filtered = useMemo(() => {
    if (!catalog) return [];
    return catalog.filter(entry => {
      const matchesSearch = !search || entry.software_name.toLowerCase().includes(search.toLowerCase());
      const matchesPlatform = !platformFilter || entry.platform_family === platformFilter;
      return matchesSearch && matchesPlatform;
    });
  }, [catalog, search, platformFilter]);

  const platforms = useMemo(() => {
    if (!catalog) return [];
    return [...new Set(catalog.map(e => e.platform_family))].sort();
  }, [catalog]);

  if (isLoading) {
    return <div className="flex justify-center py-12"><RefreshCw className="h-8 w-8 animate-spin text-gray-400" /></div>;
  }

  return (
    <div className="space-y-4">
      <div className="flex gap-4">
        <div className="relative flex-1 max-w-md">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-400" />
          <input
            type="text"
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder="Search software..."
            className="pl-10 block w-full rounded-md border-gray-300 shadow-sm focus:border-primary-500 focus:ring-primary-500 text-sm"
          />
        </div>
        <select
          value={platformFilter}
          onChange={e => setPlatformFilter(e.target.value)}
          className="rounded-md border-gray-300 shadow-sm text-sm"
        >
          <option value="">All platforms</option>
          {platforms.map(p => <option key={p} value={p}>{p}</option>)}
        </select>
      </div>

      <div className="bg-white rounded-lg border overflow-x-auto">
        <table className="min-w-full divide-y divide-gray-200">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Software</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Platform</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Latest Version</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Nodes</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200">
            {filtered.slice(0, 100).map(entry => (
              <tr key={entry.id}>
                <td className="px-4 py-3 text-sm font-mono text-gray-900">{entry.software_name}</td>
                <td className="px-4 py-3 text-sm text-gray-500">{entry.platform_family}</td>
                <td className="px-4 py-3 text-sm text-gray-500">{entry.software_type}</td>
                <td className="px-4 py-3 text-sm text-gray-900">{entry.latest_version}</td>
                <td className="px-4 py-3 text-sm text-gray-500">{entry.observed_nodes}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {filtered.length > 100 && (
          <div className="px-4 py-2 text-xs text-gray-500 bg-gray-50">Showing 100 of {filtered.length} entries</div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Vulnerabilities Tab
// ============================================================================

function VulnerabilitiesTab() {
  const { data: dashboard, isLoading } = useVulnerabilityDashboard();
  const [cveSearch, setCveSearch] = useState('');
  const [severityFilter, setSeverityFilter] = useState('');
  const { data: cveResults } = useCveSearch(
    cveSearch || undefined,
    severityFilter || undefined,
    undefined
  );

  if (isLoading) {
    return <div className="flex justify-center py-12"><RefreshCw className="h-8 w-8 animate-spin text-gray-400" /></div>;
  }

  if (!dashboard || (dashboard.total_vulnerable_nodes === 0 && dashboard.total_cves_matched === 0)) {
    return (
      <div className="text-center py-12 text-gray-500">
        <Shield className="h-12 w-12 mx-auto mb-4 text-gray-300" />
        <p>No vulnerability data available. Configure CVE feeds in Settings to start scanning.</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Summary cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="bg-white rounded-lg border p-4">
          <div className="text-sm font-medium text-gray-500">Vulnerable Nodes</div>
          <div className="mt-1 text-2xl font-semibold text-red-600">{dashboard.total_vulnerable_nodes}</div>
        </div>
        <div className="bg-white rounded-lg border p-4">
          <div className="text-sm font-medium text-gray-500">CVEs Matched</div>
          <div className="mt-1 text-2xl font-semibold">{dashboard.total_cves_matched}</div>
        </div>
        <div className="bg-white rounded-lg border p-4">
          <div className="text-sm font-medium text-gray-500">Known Exploited (KEV)</div>
          <div className="mt-1 text-2xl font-semibold text-red-600">{dashboard.kev_count}</div>
        </div>
        <div className="bg-white rounded-lg border p-4">
          <div className="text-sm font-medium text-gray-500">Severity Breakdown</div>
          <div className="mt-1 flex gap-1 flex-wrap">
            {dashboard.severity_distribution.map(s => (
              <SeverityBadge key={s.severity} severity={s.severity} count={s.count} />
            ))}
          </div>
        </div>
      </div>

      {/* Top CVEs */}
      {dashboard.top_cves.length > 0 && (
        <div className="bg-white rounded-lg border">
          <div className="px-4 py-3 border-b">
            <h4 className="font-medium text-gray-900">Top CVEs by Affected Nodes</h4>
          </div>
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">CVE ID</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Severity</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">CVSS</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Affected Nodes</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">KEV</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Description</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {dashboard.top_cves.map(cve => (
                  <tr key={cve.cve_id}>
                    <td className="px-4 py-3 text-sm font-mono text-primary-600">{cve.cve_id}</td>
                    <td className="px-4 py-3"><SeverityBadge severity={cve.severity} /></td>
                    <td className="px-4 py-3 text-sm">{cve.cvss_score?.toFixed(1) ?? '-'}</td>
                    <td className="px-4 py-3 text-sm font-medium">{cve.affected_nodes}</td>
                    <td className="px-4 py-3">{cve.is_kev ? <AlertTriangle className="h-4 w-4 text-red-500" /> : '-'}</td>
                    <td className="px-4 py-3 text-sm text-gray-500 max-w-md truncate">{cve.description || '-'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Top Vulnerable Nodes */}
      {dashboard.top_vulnerable_nodes.length > 0 && (
        <div className="bg-white rounded-lg border">
          <div className="px-4 py-3 border-b">
            <h4 className="font-medium text-gray-900">Most Vulnerable Nodes</h4>
          </div>
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Node</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Total Vulns</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Critical</th>
                  <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">KEV</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {dashboard.top_vulnerable_nodes.map(node => (
                  <tr key={node.certname}>
                    <td className="px-4 py-3 text-sm font-mono text-primary-600">{node.certname}</td>
                    <td className="px-4 py-3 text-sm">{node.total_vulns}</td>
                    <td className="px-4 py-3 text-sm">{node.critical_count > 0 ? <span className="text-red-600 font-medium">{node.critical_count}</span> : '0'}</td>
                    <td className="px-4 py-3 text-sm">{node.kev_count > 0 ? <span className="text-red-600 font-medium">{node.kev_count}</span> : '0'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* CVE Search */}
      <div className="bg-white rounded-lg border">
        <div className="px-4 py-3 border-b">
          <h4 className="font-medium text-gray-900">Search CVEs</h4>
        </div>
        <div className="p-4 space-y-4">
          <div className="flex gap-4">
            <div className="relative flex-1 max-w-md">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-400" />
              <input
                type="text"
                value={cveSearch}
                onChange={e => setCveSearch(e.target.value)}
                placeholder="Search CVE ID or description..."
                className="pl-10 block w-full rounded-md border-gray-300 shadow-sm focus:border-primary-500 focus:ring-primary-500 text-sm"
              />
            </div>
            <select
              value={severityFilter}
              onChange={e => setSeverityFilter(e.target.value)}
              className="rounded-md border-gray-300 shadow-sm text-sm"
            >
              <option value="">All severities</option>
              <option value="critical">Critical</option>
              <option value="high">High</option>
              <option value="medium">Medium</option>
              <option value="low">Low</option>
            </select>
          </div>

          {cveResults && cveResults.length > 0 && (
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">CVE ID</th>
                  <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Severity</th>
                  <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">CVSS</th>
                  <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Published</th>
                  <th className="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Description</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {cveResults.slice(0, 50).map(entry => (
                  <tr key={entry.id}>
                    <td className="px-4 py-2 text-sm font-mono">
                      {entry.id}
                      {entry.is_kev && <AlertTriangle className="inline h-3 w-3 ml-1 text-red-500" />}
                    </td>
                    <td className="px-4 py-2"><SeverityBadge severity={entry.severity} /></td>
                    <td className="px-4 py-2 text-sm">{entry.cvss_score?.toFixed(1) ?? '-'}</td>
                    <td className="px-4 py-2 text-sm text-gray-500">{formatDate(entry.published_at)}</td>
                    <td className="px-4 py-2 text-sm text-gray-500 max-w-sm truncate">{entry.description || '-'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Main Updates Page
// ============================================================================

export default function Updates() {
  const [activeTab, setActiveTab] = useState<TabId>('status');

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold text-gray-900">Updates</h1>

      {/* Tab navigation */}
      <div className="border-b border-gray-200">
        <nav className="-mb-px flex space-x-8">
          {TABS.map(tab => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-2 py-4 px-1 border-b-2 font-medium text-sm ${
                  isActive
                    ? 'border-primary-500 text-primary-600'
                    : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
                }`}
              >
                <Icon className="h-4 w-4" />
                {tab.label}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Tab content */}
      {activeTab === 'status' && <UpdateStatusTab onSwitchTab={setActiveTab} />}
      {activeTab === 'jobs' && <UpdateJobsTab />}
      {activeTab === 'catalog' && <VersionCatalogTab />}
      {activeTab === 'vulnerabilities' && <VulnerabilitiesTab />}
    </div>
  );
}
