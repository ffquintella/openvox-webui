import { useQuery } from '@tanstack/react-query';
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
import { Server, CheckCircle, XCircle, AlertCircle } from 'lucide-react';
import { api } from '../services/api';

const COLORS = {
  changed: '#22c55e',
  unchanged: '#3b82f6',
  failed: '#ef4444',
  unreported: '#f59e0b',
};

export default function Dashboard() {
  const { data: nodes, isLoading } = useQuery({
    queryKey: ['nodes'],
    queryFn: api.getNodes,
  });

  // Mock data for demonstration
  const statusData = [
    { name: 'Changed', value: 12, color: COLORS.changed },
    { name: 'Unchanged', value: 45, color: COLORS.unchanged },
    { name: 'Failed', value: 3, color: COLORS.failed },
    { name: 'Unreported', value: 5, color: COLORS.unreported },
  ];

  const trendData = [
    { date: 'Mon', changed: 10, failed: 2 },
    { date: 'Tue', changed: 15, failed: 1 },
    { date: 'Wed', changed: 8, failed: 3 },
    { date: 'Thu', changed: 12, failed: 2 },
    { date: 'Fri', changed: 20, failed: 1 },
    { date: 'Sat', changed: 5, failed: 0 },
    { date: 'Sun', changed: 3, failed: 0 },
  ];

  const stats = [
    {
      name: 'Total Nodes',
      value: nodes?.length ?? 65,
      icon: Server,
      color: 'text-primary-600',
      bg: 'bg-primary-50',
    },
    {
      name: 'Healthy',
      value: 57,
      icon: CheckCircle,
      color: 'text-success-500',
      bg: 'bg-success-50',
    },
    {
      name: 'Failed',
      value: 3,
      icon: XCircle,
      color: 'text-danger-500',
      bg: 'bg-danger-50',
    },
    {
      name: 'Unreported',
      value: 5,
      icon: AlertCircle,
      color: 'text-warning-500',
      bg: 'bg-warning-50',
    },
  ];

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-2xl font-bold text-gray-900 mb-8">Dashboard</h1>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
        {stats.map((stat) => (
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

      {/* Charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Status Distribution */}
        <div className="card">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">
            Node Status Distribution
          </h2>
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

        {/* Activity Trend */}
        <div className="card">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">
            Weekly Activity Trend
          </h2>
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
                />
                <Line
                  type="monotone"
                  dataKey="failed"
                  stroke={COLORS.failed}
                  name="Failed"
                />
              </LineChart>
            </ResponsiveContainer>
          </div>
        </div>
      </div>
    </div>
  );
}
