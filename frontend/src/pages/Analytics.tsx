import { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { RefreshCw, BarChart3 } from 'lucide-react';
import { api } from '../services/api';
import {
  ResourceHeatmap,
  GroupMembershipChart,
  FactDistributionChart,
  InfrastructureTopology,
  TimeSeriesMetrics,
} from '../components/charts';

type TabId = 'overview' | 'heatmap' | 'groups' | 'facts' | 'topology';

interface Tab {
  id: TabId;
  label: string;
}

const TABS: Tab[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'heatmap', label: 'Activity Heatmap' },
  { id: 'groups', label: 'Group Membership' },
  { id: 'facts', label: 'Fact Distribution' },
  { id: 'topology', label: 'Topology' },
];

export default function Analytics() {
  const [activeTab, setActiveTab] = useState<TabId>('overview');
  const [selectedFact, setSelectedFact] = useState<string>('os.family');

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
          <h1 className="text-2xl font-bold text-gray-900">Analytics</h1>
          <p className="text-gray-500 mt-1">
            Visualize infrastructure metrics and trends
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
        <nav className="flex gap-4">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`py-3 px-1 border-b-2 font-medium text-sm transition-colors ${
                activeTab === tab.id
                  ? 'border-primary-500 text-primary-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
              }`}
            >
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
    </div>
  );
}
