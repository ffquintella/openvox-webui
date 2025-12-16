import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { FileText, CheckCircle, XCircle, Clock } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import { Report } from '../types';

export default function Reports() {
  const [statusFilter, setStatusFilter] = useState<string>('all');

  const { data: reports = [], isLoading } = useQuery({
    queryKey: ['reports', statusFilter],
    queryFn: () => api.getReports({ status: statusFilter === 'all' ? undefined : statusFilter }),
  });

  const getStatusIcon = (status?: string) => {
    switch (status) {
      case 'changed':
        return <CheckCircle className="w-5 h-5 text-success-500" />;
      case 'failed':
        return <XCircle className="w-5 h-5 text-danger-500" />;
      default:
        return <Clock className="w-5 h-5 text-gray-400" />;
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
      <div className="flex items-center justify-between mb-8">
        <h1 className="text-2xl font-bold text-gray-900">Reports</h1>
        <select
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
          className="input w-auto"
        >
          <option value="all">All Reports</option>
          <option value="changed">Changed</option>
          <option value="unchanged">Unchanged</option>
          <option value="failed">Failed</option>
        </select>
      </div>

      {/* Reports List */}
      <div className="space-y-4">
        {reports.map((report: Report) => (
          <div key={report.hash} className="card">
            <div className="flex items-center justify-between">
              <div className="flex items-center">
                <div className="mr-4">{getStatusIcon(report.status)}</div>
                <div>
                  <h3 className="font-medium text-gray-900">{report.certname}</h3>
                  <p className="text-sm text-gray-500">
                    {report.start_time
                      ? new Date(report.start_time).toLocaleString()
                      : 'Unknown time'}
                  </p>
                </div>
              </div>
              <div className="flex items-center gap-6 text-sm">
                <div>
                  <span className="text-gray-500">Environment: </span>
                  <span className="font-medium">{report.environment || '-'}</span>
                </div>
                <div>
                  <span className="text-gray-500">Duration: </span>
                  <span className="font-medium">
                    {report.metrics?.time?.total
                      ? `${report.metrics.time.total.toFixed(2)}s`
                      : '-'}
                  </span>
                </div>
                <div
                  className={clsx(
                    'px-3 py-1 rounded-full text-xs font-medium',
                    report.status === 'changed' && 'bg-success-50 text-success-700',
                    report.status === 'unchanged' && 'bg-blue-50 text-blue-700',
                    report.status === 'failed' && 'bg-danger-50 text-danger-700'
                  )}
                >
                  {report.status || 'unknown'}
                </div>
              </div>
            </div>
          </div>
        ))}

        {reports.length === 0 && (
          <div className="text-center py-12 text-gray-500">
            <FileText className="w-12 h-12 mx-auto mb-4 text-gray-300" />
            No reports found
          </div>
        )}
      </div>
    </div>
  );
}
