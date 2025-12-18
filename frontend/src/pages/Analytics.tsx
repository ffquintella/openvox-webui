import { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  RefreshCw,
  BarChart3,
  FileText,
  ShieldCheck,
  GitCompare,
  Plus,
  Play,
  Trash2,
  Calendar,
} from 'lucide-react';
import { api } from '../services/api';
import {
  ResourceHeatmap,
  GroupMembershipChart,
  FactDistributionChart,
  InfrastructureTopology,
  TimeSeriesMetrics,
} from '../components/charts';
import {
  useSavedReports,
  useReportTemplates,
  useSchedules,
  useComplianceBaselines,
  useDriftBaselines,
  useCreateSavedReport,
  useDeleteSavedReport,
  useExecuteReport,
  useGenerateReportByType,
  useCreateComplianceBaseline,
  useDeleteComplianceBaseline,
  useCreateDriftBaseline,
  useDeleteDriftBaseline,
} from '../hooks/useAnalytics';
import type {
  ReportType,
  SavedReport,
  ReportTemplate,
  ComplianceBaseline,
  DriftBaseline,
  ReportResult,
  NodeHealthReport,
  ComplianceReport,
  ChangeTrackingReport,
  DriftReport,
} from '../types';

type TabId = 'overview' | 'heatmap' | 'groups' | 'facts' | 'topology' | 'reports' | 'compliance' | 'drift';

interface Tab {
  id: TabId;
  label: string;
  icon?: React.ComponentType<{ className?: string }>;
}

const TABS: Tab[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'heatmap', label: 'Activity Heatmap' },
  { id: 'groups', label: 'Group Membership' },
  { id: 'facts', label: 'Fact Distribution' },
  { id: 'topology', label: 'Topology' },
  { id: 'reports', label: 'Reports', icon: FileText },
  { id: 'compliance', label: 'Compliance', icon: ShieldCheck },
  { id: 'drift', label: 'Drift Detection', icon: GitCompare },
];

const REPORT_TYPE_LABELS: Record<ReportType, string> = {
  node_health: 'Node Health',
  compliance: 'Compliance',
  change_tracking: 'Change Tracking',
  drift_detection: 'Drift Detection',
  custom: 'Custom',
};

function formatDate(dateString: string | null | undefined): string {
  if (!dateString) return 'Never';
  return new Date(dateString).toLocaleString();
}

export default function Analytics() {
  const [activeTab, setActiveTab] = useState<TabId>('overview');
  const [selectedFact, setSelectedFact] = useState<string>('os.family');
  const [showNewReportModal, setShowNewReportModal] = useState(false);
  const [showNewComplianceModal, setShowNewComplianceModal] = useState(false);
  const [showNewDriftModal, setShowNewDriftModal] = useState(false);
  const [reportResult, setReportResult] = useState<ReportResult | null>(null);
  const [generatingReport, setGeneratingReport] = useState<string | null>(null);

  // Existing analytics queries
  const {
    data: nodes = [],
    isLoading: nodesLoading,
    refetch: refetchNodes,
  } = useQuery({
    queryKey: ['nodes'],
    queryFn: api.getNodes,
  });

  const {
    data: groups = [],
    isLoading: groupsLoading,
    refetch: refetchGroups,
  } = useQuery({
    queryKey: ['groups'],
    queryFn: api.getGroups,
  });

  const {
    data: reports = [],
    isLoading: reportsLoading,
    refetch: refetchReports,
  } = useQuery({
    queryKey: ['reports', { limit: 500 }],
    queryFn: () => api.getReports({ limit: 500 }),
  });

  const {
    data: factNames = [],
    isLoading: factNamesLoading,
  } = useQuery({
    queryKey: ['factNames'],
    queryFn: api.getFactNames,
  });

  const {
    data: facts = [],
    isLoading: factsLoading,
  } = useQuery({
    queryKey: ['facts', selectedFact],
    queryFn: () => api.getFacts({ name: selectedFact }),
    enabled: activeTab === 'facts' || activeTab === 'overview',
  });

  // Reporting queries
  const { data: savedReports = [], isLoading: savedReportsLoading, refetch: refetchSavedReports } = useSavedReports();
  const { data: reportTemplates = [], isLoading: templatesLoading } = useReportTemplates();
  const { data: schedules = [], isLoading: schedulesLoading } = useSchedules();
  const { data: complianceBaselines = [], isLoading: complianceLoading, refetch: refetchCompliance } = useComplianceBaselines();
  const { data: driftBaselines = [], isLoading: driftLoading, refetch: refetchDrift } = useDriftBaselines();

  // Mutations
  const createReport = useCreateSavedReport();
  const deleteReport = useDeleteSavedReport();
  const executeReport = useExecuteReport();
  const generateReport = useGenerateReportByType();
  const createComplianceBaseline = useCreateComplianceBaseline();
  const deleteComplianceBaseline = useDeleteComplianceBaseline();
  const createDriftBaseline = useCreateDriftBaseline();
  const deleteDriftBaseline = useDeleteDriftBaseline();

  // Transform reports to heatmap data
  const heatmapData = useMemo(() => {
    return reports
      .filter((r) => r.start_time && r.metrics?.changes !== undefined)
      .map((r) => ({
        timestamp: r.start_time!,
        changes: r.metrics!.changes,
      }));
  }, [reports]);

  const isLoading = nodesLoading || groupsLoading || reportsLoading;

  const handleRefresh = () => {
    refetchNodes();
    refetchGroups();
    refetchReports();
    refetchSavedReports();
    refetchCompliance();
    refetchDrift();
  };

  const handleGenerateReport = async (reportType: ReportType) => {
    setGeneratingReport(reportType);
    try {
      const result = await generateReport.mutateAsync({ reportType });
      setReportResult(result as ReportResult);
    } catch (error) {
      console.error('Failed to generate report:', error);
    } finally {
      setGeneratingReport(null);
    }
  };

  const handleExecuteReport = async (reportId: string) => {
    try {
      const execution = await executeReport.mutateAsync({ id: reportId });
      if (execution.output_data) {
        setReportResult(execution.output_data as ReportResult);
      }
    } catch (error) {
      console.error('Failed to execute report:', error);
    }
  };

  const handleDeleteReport = async (id: string) => {
    if (window.confirm('Are you sure you want to delete this report?')) {
      await deleteReport.mutateAsync(id);
    }
  };

  const handleDeleteComplianceBaseline = async (id: string) => {
    if (window.confirm('Are you sure you want to delete this compliance baseline?')) {
      await deleteComplianceBaseline.mutateAsync(id);
    }
  };

  const handleDeleteDriftBaseline = async (id: string) => {
    if (window.confirm('Are you sure you want to delete this drift baseline?')) {
      await deleteDriftBaseline.mutateAsync(id);
    }
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
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Analytics & Reporting</h1>
          <p className="text-gray-500 mt-1">
            Visualize infrastructure metrics, generate reports, and track compliance
          </p>
        </div>
        <button
          onClick={handleRefresh}
          className="flex items-center gap-2 px-4 py-2 text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-lg transition-colors"
        >
          <RefreshCw className="h-4 w-4" />
          Refresh
        </button>
      </div>

      {/* Tabs */}
      <div className="border-b border-gray-200 mb-6">
        <nav className="flex gap-4 overflow-x-auto">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`py-3 px-1 border-b-2 font-medium text-sm transition-colors whitespace-nowrap flex items-center gap-2 ${
                activeTab === tab.id
                  ? 'border-primary-500 text-primary-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
              }`}
            >
              {tab.icon && <tab.icon className="w-4 h-4" />}
              {tab.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Tab Content */}
      {activeTab === 'overview' && (
        <div className="space-y-6">
          {/* Quick Stats */}
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <div className="card text-center">
              <p className="text-3xl font-bold text-gray-900">{nodes.length}</p>
              <p className="text-sm text-gray-500">Total Nodes</p>
            </div>
            <div className="card text-center">
              <p className="text-3xl font-bold text-gray-900">{groups.length}</p>
              <p className="text-sm text-gray-500">Node Groups</p>
            </div>
            <div className="card text-center">
              <p className="text-3xl font-bold text-gray-900">{reports.length}</p>
              <p className="text-sm text-gray-500">Reports</p>
            </div>
            <div className="card text-center">
              <p className="text-3xl font-bold text-gray-900">
                {new Set(nodes.map((n) => n.catalog_environment || 'production')).size}
              </p>
              <p className="text-sm text-gray-500">Environments</p>
            </div>
          </div>

          {/* Time Series Chart */}
          <div className="card">
            <TimeSeriesMetrics reports={reports} />
          </div>

          {/* Two Column Layout */}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <div className="card">
              <GroupMembershipChart groups={groups} />
            </div>
            <div className="card">
              <FactDistributionChart
                facts={facts}
                factName={selectedFact}
                title="OS Distribution"
              />
            </div>
          </div>
        </div>
      )}

      {activeTab === 'heatmap' && (
        <div className="card">
          {heatmapData.length === 0 ? (
            <div className="h-64 flex items-center justify-center text-gray-500">
              <div className="text-center">
                <BarChart3 className="w-12 h-12 mx-auto mb-3 text-gray-300" />
                <p>No activity data available</p>
                <p className="text-sm mt-1">Reports with change metrics will appear here</p>
              </div>
            </div>
          ) : (
            <ResourceHeatmap data={heatmapData} />
          )}
        </div>
      )}

      {activeTab === 'groups' && (
        <div className="card">
          <GroupMembershipChart groups={groups} />
        </div>
      )}

      {activeTab === 'facts' && (
        <div className="space-y-6">
          {/* Fact selector */}
          <div className="card">
            <div className="flex items-center gap-4 mb-4">
              <label className="text-sm font-medium text-gray-700">Select Fact:</label>
              <select
                value={selectedFact}
                onChange={(e) => setSelectedFact(e.target.value)}
                className="block w-64 px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
                disabled={factNamesLoading}
              >
                {factNamesLoading ? (
                  <option>Loading...</option>
                ) : factNames.length === 0 ? (
                  <>
                    <option value="os.family">os.family</option>
                    <option value="os.name">os.name</option>
                    <option value="os.release.major">os.release.major</option>
                    <option value="kernel">kernel</option>
                    <option value="virtual">virtual</option>
                  </>
                ) : (
                  factNames.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))
                )}
              </select>
            </div>

            {factsLoading ? (
              <div className="h-64 flex items-center justify-center">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
              </div>
            ) : (
              <FactDistributionChart facts={facts} factName={selectedFact} />
            )}
          </div>
        </div>
      )}

      {activeTab === 'topology' && (
        <div className="card">
          <InfrastructureTopology nodes={nodes} groups={groups} />
        </div>
      )}

      {activeTab === 'reports' && (
        <div className="space-y-6">
          {/* Quick Generate */}
          <div className="card">
            <h3 className="text-lg font-semibold text-gray-900 mb-4">Quick Generate Report</h3>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              {(['node_health', 'compliance', 'change_tracking', 'drift_detection'] as ReportType[]).map((type) => (
                <button
                  key={type}
                  onClick={() => handleGenerateReport(type)}
                  disabled={generatingReport === type}
                  className="flex items-center justify-center gap-2 p-4 border border-gray-200 rounded-lg hover:bg-gray-50 transition-colors disabled:opacity-50"
                >
                  {generatingReport === type ? (
                    <div className="animate-spin rounded-full h-5 w-5 border-b-2 border-primary-600" />
                  ) : (
                    <Play className="w-5 h-5 text-primary-600" />
                  )}
                  <span className="font-medium">{REPORT_TYPE_LABELS[type]}</span>
                </button>
              ))}
            </div>
          </div>

          {/* Report Result */}
          {reportResult && (
            <div className="card">
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-lg font-semibold text-gray-900">Report Result</h3>
                <button
                  onClick={() => setReportResult(null)}
                  className="text-gray-400 hover:text-gray-600"
                >
                  &times;
                </button>
              </div>
              <ReportResultView result={reportResult} />
            </div>
          )}

          {/* Saved Reports */}
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-gray-900">Saved Reports</h3>
              <button
                onClick={() => setShowNewReportModal(true)}
                className="btn-primary flex items-center gap-2"
              >
                <Plus className="w-4 h-4" />
                New Report
              </button>
            </div>

            {savedReportsLoading ? (
              <div className="flex items-center justify-center h-32">
                <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary-600" />
              </div>
            ) : savedReports.length === 0 ? (
              <div className="text-center py-8 text-gray-500">
                <FileText className="w-12 h-12 mx-auto mb-3 text-gray-300" />
                <p>No saved reports yet</p>
                <p className="text-sm mt-1">Create a report to save and run it later</p>
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-gray-200">
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Name</th>
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Type</th>
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Created</th>
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Public</th>
                      <th className="px-4 py-3 text-right text-sm font-medium text-gray-500">Actions</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-100">
                    {savedReports.map((report: SavedReport) => (
                      <tr key={report.id} className="hover:bg-gray-50">
                        <td className="px-4 py-3">
                          <div>
                            <p className="font-medium text-gray-900">{report.name}</p>
                            {report.description && (
                              <p className="text-sm text-gray-500">{report.description}</p>
                            )}
                          </div>
                        </td>
                        <td className="px-4 py-3">
                          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-primary-100 text-primary-800">
                            {REPORT_TYPE_LABELS[report.report_type]}
                          </span>
                        </td>
                        <td className="px-4 py-3 text-sm text-gray-500">
                          {formatDate(report.created_at)}
                        </td>
                        <td className="px-4 py-3">
                          {report.is_public ? (
                            <span className="text-success-600">Yes</span>
                          ) : (
                            <span className="text-gray-400">No</span>
                          )}
                        </td>
                        <td className="px-4 py-3 text-right">
                          <div className="flex items-center justify-end gap-2">
                            <button
                              onClick={() => handleExecuteReport(report.id)}
                              className="p-1.5 text-primary-600 hover:bg-primary-50 rounded"
                              title="Run Report"
                            >
                              <Play className="w-4 h-4" />
                            </button>
                            <button
                              onClick={() => handleDeleteReport(report.id)}
                              className="p-1.5 text-danger-600 hover:bg-danger-50 rounded"
                              title="Delete"
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
            )}
          </div>

          {/* Report Templates */}
          <div className="card">
            <h3 className="text-lg font-semibold text-gray-900 mb-4">Report Templates</h3>
            {templatesLoading ? (
              <div className="flex items-center justify-center h-32">
                <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary-600" />
              </div>
            ) : reportTemplates.length === 0 ? (
              <div className="text-center py-8 text-gray-500">
                <FileText className="w-12 h-12 mx-auto mb-3 text-gray-300" />
                <p>No report templates available</p>
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {reportTemplates.map((template: ReportTemplate) => (
                  <div key={template.id} className="border border-gray-200 rounded-lg p-4">
                    <div className="flex items-start justify-between">
                      <div>
                        <h4 className="font-medium text-gray-900">{template.name}</h4>
                        {template.description && (
                          <p className="text-sm text-gray-500 mt-1">{template.description}</p>
                        )}
                      </div>
                      {template.is_system && (
                        <span className="text-xs text-gray-400">System</span>
                      )}
                    </div>
                    <div className="mt-3">
                      <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-700">
                        {REPORT_TYPE_LABELS[template.report_type]}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Schedules */}
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-gray-900">Scheduled Reports</h3>
            </div>
            {schedulesLoading ? (
              <div className="flex items-center justify-center h-32">
                <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary-600" />
              </div>
            ) : schedules.length === 0 ? (
              <div className="text-center py-8 text-gray-500">
                <Calendar className="w-12 h-12 mx-auto mb-3 text-gray-300" />
                <p>No scheduled reports</p>
                <p className="text-sm mt-1">Schedule a report to run automatically</p>
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-gray-200">
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Schedule</th>
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Timezone</th>
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Last Run</th>
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Next Run</th>
                      <th className="px-4 py-3 text-left text-sm font-medium text-gray-500">Status</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-100">
                    {schedules.map((schedule) => (
                      <tr key={schedule.id} className="hover:bg-gray-50">
                        <td className="px-4 py-3 font-mono text-sm">{schedule.schedule_cron}</td>
                        <td className="px-4 py-3 text-sm text-gray-500">{schedule.timezone}</td>
                        <td className="px-4 py-3 text-sm text-gray-500">
                          {formatDate(schedule.last_run_at)}
                        </td>
                        <td className="px-4 py-3 text-sm text-gray-500">
                          {formatDate(schedule.next_run_at)}
                        </td>
                        <td className="px-4 py-3">
                          {schedule.is_enabled ? (
                            <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-success-100 text-success-800">
                              Enabled
                            </span>
                          ) : (
                            <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-800">
                              Disabled
                            </span>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      )}

      {activeTab === 'compliance' && (
        <div className="space-y-6">
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-gray-900">Compliance Baselines</h3>
              <button
                onClick={() => setShowNewComplianceModal(true)}
                className="btn-primary flex items-center gap-2"
              >
                <Plus className="w-4 h-4" />
                New Baseline
              </button>
            </div>

            {complianceLoading ? (
              <div className="flex items-center justify-center h-32">
                <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary-600" />
              </div>
            ) : complianceBaselines.length === 0 ? (
              <div className="text-center py-8 text-gray-500">
                <ShieldCheck className="w-12 h-12 mx-auto mb-3 text-gray-300" />
                <p>No compliance baselines defined</p>
                <p className="text-sm mt-1">Create a baseline to check node compliance against expected values</p>
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {complianceBaselines.map((baseline: ComplianceBaseline) => (
                  <div key={baseline.id} className="border border-gray-200 rounded-lg p-4">
                    <div className="flex items-start justify-between">
                      <div>
                        <h4 className="font-medium text-gray-900">{baseline.name}</h4>
                        {baseline.description && (
                          <p className="text-sm text-gray-500 mt-1">{baseline.description}</p>
                        )}
                      </div>
                      <button
                        onClick={() => handleDeleteComplianceBaseline(baseline.id)}
                        className="p-1 text-danger-600 hover:bg-danger-50 rounded"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                    <div className="mt-3 flex items-center gap-4 text-sm text-gray-500">
                      <span>{baseline.rules.length} rules</span>
                      <span className={`capitalize ${
                        baseline.severity_level === 'critical' ? 'text-danger-600' :
                        baseline.severity_level === 'high' ? 'text-warning-600' :
                        baseline.severity_level === 'medium' ? 'text-primary-600' :
                        'text-gray-600'
                      }`}>
                        {baseline.severity_level} severity
                      </span>
                    </div>
                    <div className="mt-2 text-xs text-gray-400">
                      Created {formatDate(baseline.created_at)}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {activeTab === 'drift' && (
        <div className="space-y-6">
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-gray-900">Drift Baselines</h3>
              <button
                onClick={() => setShowNewDriftModal(true)}
                className="btn-primary flex items-center gap-2"
              >
                <Plus className="w-4 h-4" />
                New Baseline
              </button>
            </div>

            {driftLoading ? (
              <div className="flex items-center justify-center h-32">
                <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary-600" />
              </div>
            ) : driftBaselines.length === 0 ? (
              <div className="text-center py-8 text-gray-500">
                <GitCompare className="w-12 h-12 mx-auto mb-3 text-gray-300" />
                <p>No drift baselines defined</p>
                <p className="text-sm mt-1">Create a baseline to detect configuration drift across nodes</p>
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {driftBaselines.map((baseline: DriftBaseline) => (
                  <div key={baseline.id} className="border border-gray-200 rounded-lg p-4">
                    <div className="flex items-start justify-between">
                      <div>
                        <h4 className="font-medium text-gray-900">{baseline.name}</h4>
                        {baseline.description && (
                          <p className="text-sm text-gray-500 mt-1">{baseline.description}</p>
                        )}
                      </div>
                      <button
                        onClick={() => handleDeleteDriftBaseline(baseline.id)}
                        className="p-1 text-danger-600 hover:bg-danger-50 rounded"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                    <div className="mt-3 flex items-center gap-4 text-sm text-gray-500">
                      <span>{Object.keys(baseline.baseline_facts).length} facts tracked</span>
                      {baseline.node_group_id && (
                        <span>Group: {baseline.node_group_id}</span>
                      )}
                    </div>
                    <div className="mt-2 text-xs text-gray-400">
                      Created {formatDate(baseline.created_at)}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* New Report Modal */}
      {showNewReportModal && (
        <NewReportModal
          onClose={() => setShowNewReportModal(false)}
          onCreate={async (data) => {
            await createReport.mutateAsync(data);
            setShowNewReportModal(false);
          }}
        />
      )}

      {/* New Compliance Baseline Modal */}
      {showNewComplianceModal && (
        <NewComplianceBaselineModal
          onClose={() => setShowNewComplianceModal(false)}
          onCreate={async (data) => {
            await createComplianceBaseline.mutateAsync(data);
            setShowNewComplianceModal(false);
          }}
        />
      )}

      {/* New Drift Baseline Modal */}
      {showNewDriftModal && (
        <NewDriftBaselineModal
          onClose={() => setShowNewDriftModal(false)}
          onCreate={async (data) => {
            await createDriftBaseline.mutateAsync(data);
            setShowNewDriftModal(false);
          }}
        />
      )}
    </div>
  );
}

// Report Result View Component
function ReportResultView({ result }: { result: ReportResult }) {
  if (result.report_type === 'node_health') {
    const data = result as NodeHealthReport;
    return (
      <div className="space-y-4">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="text-center p-3 bg-gray-50 rounded-lg">
            <p className="text-2xl font-bold text-gray-900">{data.summary.total_nodes}</p>
            <p className="text-sm text-gray-500">Total Nodes</p>
          </div>
          <div className="text-center p-3 bg-success-50 rounded-lg">
            <p className="text-2xl font-bold text-success-600">{data.summary.unchanged_count}</p>
            <p className="text-sm text-gray-500">Unchanged</p>
          </div>
          <div className="text-center p-3 bg-warning-50 rounded-lg">
            <p className="text-2xl font-bold text-warning-600">{data.summary.changed_count}</p>
            <p className="text-sm text-gray-500">Changed</p>
          </div>
          <div className="text-center p-3 bg-danger-50 rounded-lg">
            <p className="text-2xl font-bold text-danger-600">{data.summary.failed_count}</p>
            <p className="text-sm text-gray-500">Failed</p>
          </div>
        </div>
        <div className="text-sm text-gray-500">
          Compliance Rate: {(data.summary.compliance_rate * 100).toFixed(1)}%
        </div>
      </div>
    );
  }

  if (result.report_type === 'compliance') {
    const data = result as ComplianceReport;
    return (
      <div className="space-y-4">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="text-center p-3 bg-gray-50 rounded-lg">
            <p className="text-2xl font-bold text-gray-900">{data.summary.total_nodes}</p>
            <p className="text-sm text-gray-500">Total Nodes</p>
          </div>
          <div className="text-center p-3 bg-success-50 rounded-lg">
            <p className="text-2xl font-bold text-success-600">{data.summary.compliant_nodes}</p>
            <p className="text-sm text-gray-500">Compliant</p>
          </div>
          <div className="text-center p-3 bg-danger-50 rounded-lg">
            <p className="text-2xl font-bold text-danger-600">{data.summary.non_compliant_nodes}</p>
            <p className="text-sm text-gray-500">Non-Compliant</p>
          </div>
          <div className="text-center p-3 bg-warning-50 rounded-lg">
            <p className="text-2xl font-bold text-warning-600">{data.summary.total_violations}</p>
            <p className="text-sm text-gray-500">Violations</p>
          </div>
        </div>
        <div className="text-sm text-gray-500">
          Compliance Rate: {(data.summary.compliance_rate * 100).toFixed(1)}%
        </div>
      </div>
    );
  }

  if (result.report_type === 'change_tracking') {
    const data = result as ChangeTrackingReport;
    return (
      <div className="space-y-4">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="text-center p-3 bg-gray-50 rounded-lg">
            <p className="text-2xl font-bold text-gray-900">{data.summary.total_changes}</p>
            <p className="text-sm text-gray-500">Total Changes</p>
          </div>
          <div className="text-center p-3 bg-primary-50 rounded-lg">
            <p className="text-2xl font-bold text-primary-600">{data.summary.nodes_affected}</p>
            <p className="text-sm text-gray-500">Nodes Affected</p>
          </div>
          <div className="text-center p-3 bg-warning-50 rounded-lg">
            <p className="text-2xl font-bold text-warning-600">{data.summary.resources_changed}</p>
            <p className="text-sm text-gray-500">Resources Changed</p>
          </div>
          <div className="text-center p-3 bg-danger-50 rounded-lg">
            <p className="text-2xl font-bold text-danger-600">{data.summary.resources_failed}</p>
            <p className="text-sm text-gray-500">Resources Failed</p>
          </div>
        </div>
        <div className="text-sm text-gray-500">
          Time Range: {data.time_range}
        </div>
      </div>
    );
  }

  if (result.report_type === 'drift_detection') {
    const data = result as DriftReport;
    return (
      <div className="space-y-4">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="text-center p-3 bg-gray-50 rounded-lg">
            <p className="text-2xl font-bold text-gray-900">{data.summary.total_nodes}</p>
            <p className="text-sm text-gray-500">Total Nodes</p>
          </div>
          <div className="text-center p-3 bg-danger-50 rounded-lg">
            <p className="text-2xl font-bold text-danger-600">{data.summary.nodes_with_drift}</p>
            <p className="text-sm text-gray-500">With Drift</p>
          </div>
          <div className="text-center p-3 bg-success-50 rounded-lg">
            <p className="text-2xl font-bold text-success-600">{data.summary.nodes_without_drift}</p>
            <p className="text-sm text-gray-500">No Drift</p>
          </div>
          <div className="text-center p-3 bg-warning-50 rounded-lg">
            <p className="text-2xl font-bold text-warning-600">{data.summary.total_drifted_facts}</p>
            <p className="text-sm text-gray-500">Drifted Facts</p>
          </div>
        </div>
        <div className="text-sm text-gray-500">
          Drift Rate: {(data.summary.drift_rate * 100).toFixed(1)}%
        </div>
      </div>
    );
  }

  return (
    <pre className="text-sm bg-gray-50 p-4 rounded-lg overflow-auto">
      {JSON.stringify(result, null, 2)}
    </pre>
  );
}

// New Report Modal
function NewReportModal({
  onClose,
  onCreate,
}: {
  onClose: () => void;
  onCreate: (data: { name: string; description?: string; report_type: ReportType; query_config: Record<string, unknown>; is_public?: boolean }) => Promise<void>;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [reportType, setReportType] = useState<ReportType>('node_health');
  const [isPublic, setIsPublic] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    try {
      await onCreate({
        name,
        description: description || undefined,
        report_type: reportType,
        query_config: {},
        is_public: isPublic,
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-md mx-4">
        <div className="px-6 py-4 border-b border-gray-200">
          <h3 className="text-lg font-semibold text-gray-900">New Saved Report</h3>
        </div>
        <form onSubmit={handleSubmit}>
          <div className="px-6 py-4 space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Name</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Description</label>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
                rows={3}
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Report Type</label>
              <select
                value={reportType}
                onChange={(e) => setReportType(e.target.value as ReportType)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
              >
                {Object.entries(REPORT_TYPE_LABELS).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex items-center">
              <input
                type="checkbox"
                id="isPublic"
                checked={isPublic}
                onChange={(e) => setIsPublic(e.target.checked)}
                className="h-4 w-4 text-primary-600 focus:ring-primary-500 border-gray-300 rounded"
              />
              <label htmlFor="isPublic" className="ml-2 text-sm text-gray-700">
                Make this report public
              </label>
            </div>
          </div>
          <div className="px-6 py-4 border-t border-gray-200 flex justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-gray-700 hover:bg-gray-100 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || !name}
              className="btn-primary"
            >
              {isSubmitting ? 'Creating...' : 'Create Report'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// New Compliance Baseline Modal
function NewComplianceBaselineModal({
  onClose,
  onCreate,
}: {
  onClose: () => void;
  onCreate: (data: { name: string; description?: string; rules: Array<{ id: string; name: string; fact_name: string; operator: string; expected_value: unknown; severity: 'low' | 'medium' | 'high' | 'critical' }>; severity_level?: 'low' | 'medium' | 'high' | 'critical' }) => Promise<void>;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [severity, setSeverity] = useState<'low' | 'medium' | 'high' | 'critical'>('medium');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    try {
      await onCreate({
        name,
        description: description || undefined,
        rules: [],
        severity_level: severity,
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-md mx-4">
        <div className="px-6 py-4 border-b border-gray-200">
          <h3 className="text-lg font-semibold text-gray-900">New Compliance Baseline</h3>
        </div>
        <form onSubmit={handleSubmit}>
          <div className="px-6 py-4 space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Name</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Description</label>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
                rows={3}
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Severity Level</label>
              <select
                value={severity}
                onChange={(e) => setSeverity(e.target.value as 'low' | 'medium' | 'high' | 'critical')}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
              >
                <option value="low">Low</option>
                <option value="medium">Medium</option>
                <option value="high">High</option>
                <option value="critical">Critical</option>
              </select>
            </div>
          </div>
          <div className="px-6 py-4 border-t border-gray-200 flex justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-gray-700 hover:bg-gray-100 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || !name}
              className="btn-primary"
            >
              {isSubmitting ? 'Creating...' : 'Create Baseline'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// New Drift Baseline Modal
function NewDriftBaselineModal({
  onClose,
  onCreate,
}: {
  onClose: () => void;
  onCreate: (data: { name: string; description?: string; baseline_facts: Record<string, unknown>; node_group_id?: string }) => Promise<void>;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    try {
      await onCreate({
        name,
        description: description || undefined,
        baseline_facts: {},
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-md mx-4">
        <div className="px-6 py-4 border-b border-gray-200">
          <h3 className="text-lg font-semibold text-gray-900">New Drift Baseline</h3>
        </div>
        <form onSubmit={handleSubmit}>
          <div className="px-6 py-4 space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Name</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Description</label>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500"
                rows={3}
              />
            </div>
          </div>
          <div className="px-6 py-4 border-t border-gray-200 flex justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-gray-700 hover:bg-gray-100 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || !name}
              className="btn-primary"
            >
              {isSubmitting ? 'Creating...' : 'Create Baseline'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
