import { useState, useEffect, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { FileText, CheckCircle, XCircle, Clock, AlertCircle, ArrowRightLeft, Loader2 } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import { Report, ResourceEvent } from '../types';

// Helper function to get event status color
function getEventStatusColor(status: string): string {
  switch (status) {
    case 'success':
      return 'text-success-600 bg-success-50 border-success-200';
    case 'failure':
      return 'text-danger-600 bg-danger-50 border-danger-200';
    case 'noop':
      return 'text-amber-600 bg-amber-50 border-amber-200';
    case 'skipped':
      return 'text-gray-600 bg-gray-50 border-gray-200';
    default:
      return 'text-gray-600 bg-gray-50 border-gray-200';
  }
}

// Helper to format value for display
function formatEventValue(value: unknown): string {
  if (value === null || value === undefined) return 'nil';
  if (typeof value === 'string') return value;
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return String(value);
  if (Array.isArray(value)) return JSON.stringify(value);
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

// Resource Events Component
function ResourceEventsPanel({ reportHash }: { reportHash: string }) {
  const [events, setEvents] = useState<ResourceEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<'all' | 'success' | 'failure' | 'noop'>('all');

  useEffect(() => {
    setLoading(true);
    setError(null);
    api.getReportEvents(reportHash)
      .then((data) => {
        setEvents(data);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message || 'Failed to load events');
        setLoading(false);
      });
  }, [reportHash]);

  const filteredEvents = useMemo(() => {
    if (filter === 'all') return events;
    return events.filter((e) => e.status === filter);
  }, [events, filter]);

  const failureCount = events.filter((e) => e.status === 'failure').length;
  const successCount = events.filter((e) => e.status === 'success').length;
  const noopCount = events.filter((e) => e.status === 'noop').length;

  if (loading) {
    return (
      <div className="flex items-center justify-center py-6">
        <Loader2 className="w-5 h-5 animate-spin text-primary-500" />
        <span className="ml-2 text-sm text-gray-500">Loading events...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="text-center py-6">
        <AlertCircle className="w-8 h-8 mx-auto text-danger-500 mb-2" />
        <p className="text-sm text-danger-600">{error}</p>
      </div>
    );
  }

  if (events.length === 0) {
    return (
      <div className="text-center py-6 text-gray-500">
        <CheckCircle className="w-8 h-8 mx-auto text-gray-300 mb-2" />
        <p className="text-sm">No resource events in this report</p>
        <p className="text-xs text-gray-400 mt-1">This usually means no changes were made</p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Filter tabs */}
      <div className="flex gap-2 flex-wrap">
        <button
          onClick={() => setFilter('all')}
          className={`px-3 py-1 text-xs rounded-full transition-colors ${
            filter === 'all'
              ? 'bg-primary-100 text-primary-700 border border-primary-300'
              : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
          }`}
        >
          All ({events.length})
        </button>
        {failureCount > 0 && (
          <button
            onClick={() => setFilter('failure')}
            className={`px-3 py-1 text-xs rounded-full transition-colors ${
              filter === 'failure'
                ? 'bg-danger-100 text-danger-700 border border-danger-300'
                : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
            }`}
          >
            Failed ({failureCount})
          </button>
        )}
        {successCount > 0 && (
          <button
            onClick={() => setFilter('success')}
            className={`px-3 py-1 text-xs rounded-full transition-colors ${
              filter === 'success'
                ? 'bg-success-100 text-success-700 border border-success-300'
                : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
            }`}
          >
            Changed ({successCount})
          </button>
        )}
        {noopCount > 0 && (
          <button
            onClick={() => setFilter('noop')}
            className={`px-3 py-1 text-xs rounded-full transition-colors ${
              filter === 'noop'
                ? 'bg-amber-100 text-amber-700 border border-amber-300'
                : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
            }`}
          >
            Noop ({noopCount})
          </button>
        )}
      </div>

      {/* Events list */}
      <div className="space-y-2 max-h-96 overflow-y-auto">
        {filteredEvents.map((event, idx) => (
          <div
            key={`${event.resource_type}-${event.resource_title}-${idx}`}
            className={`p-3 rounded-lg border ${getEventStatusColor(event.status)}`}
          >
            <div className="flex items-start justify-between gap-2">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium uppercase tracking-wide">
                    {event.status}
                  </span>
                  {event.corrective_change && (
                    <span className="text-xs px-1.5 py-0.5 bg-amber-200 text-amber-800 rounded">
                      Corrective
                    </span>
                  )}
                </div>
                <p className="font-mono text-sm mt-1 break-all">
                  {event.resource_type}[{event.resource_title}]
                </p>
                {event.property && (
                  <p className="text-xs text-gray-600 mt-1">
                    Property: <span className="font-mono">{event.property}</span>
                  </p>
                )}
              </div>
            </div>

            {/* Value change display */}
            {(event.old_value !== undefined || event.new_value !== undefined) && (
              <div className="mt-2 pt-2 border-t border-current/10">
                <div className="flex items-center gap-2 text-xs">
                  <div className="flex-1 min-w-0">
                    <p className="text-gray-500 mb-0.5">Old value:</p>
                    <p className="font-mono bg-white/50 px-2 py-1 rounded truncate">
                      {formatEventValue(event.old_value)}
                    </p>
                  </div>
                  <ArrowRightLeft className="w-4 h-4 flex-shrink-0 text-gray-400" />
                  <div className="flex-1 min-w-0">
                    <p className="text-gray-500 mb-0.5">New value:</p>
                    <p className="font-mono bg-white/50 px-2 py-1 rounded truncate">
                      {formatEventValue(event.new_value)}
                    </p>
                  </div>
                </div>
              </div>
            )}

            {/* Message */}
            {event.message && (
              <div className="mt-2 pt-2 border-t border-current/10">
                <p className="text-xs text-gray-700 whitespace-pre-wrap">{event.message}</p>
              </div>
            )}

            {/* Source location and containment path */}
            {(event.file || event.containing_class || (event.containment_path && event.containment_path.length > 0)) && (
              <div className="mt-2 pt-2 border-t border-current/10 space-y-1">
                {event.containing_class && (
                  <p className="text-xs text-gray-500">
                    Class: <span className="font-mono">{event.containing_class}</span>
                  </p>
                )}
                {event.file && (
                  <p className="text-xs text-gray-500">
                    File: <span className="font-mono">{event.file}{event.line ? `:${event.line}` : ''}</span>
                  </p>
                )}
                {event.containment_path && event.containment_path.length > 0 && (
                  <p className="text-xs text-gray-500">
                    Path: {event.containment_path.join(' → ')}
                  </p>
                )}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function Reports() {
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [expandedReport, setExpandedReport] = useState<string | null>(null);

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
        {reports.map((report: Report) => {
          const isExpanded = expandedReport === report.hash;

          return (
            <div key={report.hash} className={clsx(
              'card transition-all cursor-pointer',
              isExpanded && 'ring-2 ring-primary-200'
            )}
            onClick={() => setExpandedReport(isExpanded ? null : report.hash)}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center">
                  <div className="mr-4">{getStatusIcon(report.status ?? undefined)}</div>
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

              {/* Expanded Details */}
              {isExpanded && (
                <div className="mt-6 pt-6 border-t border-gray-200 space-y-4" onClick={(e) => e.stopPropagation()}>
                  {/* Report metadata */}
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
                    <div className="bg-gray-50 rounded-lg p-4 border border-gray-100">
                      <p className="text-sm font-medium text-gray-700 mb-3">Resource Metrics</p>
                      <div className="grid grid-cols-4 gap-4 text-center text-sm">
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

                  {/* Resource Events */}
                  <div className="bg-white rounded-lg p-4 border border-gray-100">
                    <p className="text-sm font-medium text-gray-700 mb-3">Resource Events</p>
                    <ResourceEventsPanel reportHash={report.hash} />
                  </div>
                </div>
              )}
            </div>
          );
        })}

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
