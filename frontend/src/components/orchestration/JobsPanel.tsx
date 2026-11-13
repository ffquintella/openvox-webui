import { useState } from 'react';
import { Loader2 } from 'lucide-react';
import { useBoltJobs, useDecideBoltJob } from '../../hooks/useOrchestration';
import {
  BOLT_JOB_STATUSES,
  decisionState,
  errorText,
  statusLabel,
  type JobActor,
} from '../../utils/orchestration';
import type { BoltJob, BoltJobStatus } from '../../types';
import BoltStatusBadge from './BoltStatusBadge';
import JobDetail from './JobDetail';

function fmt(value?: string | null): string {
  return value ? new Date(value).toLocaleString() : '-';
}

/** Jobs table with a status filter; a row opens the detail drawer. */
export default function JobsPanel({
  actor,
  initialJobId,
}: {
  actor: JobActor;
  initialJobId?: string | null;
}) {
  const [status, setStatus] = useState<BoltJobStatus | ''>('');
  const [selected, setSelected] = useState<string | null>(initialJobId ?? null);
  const [actionError, setActionError] = useState<string | null>(null);
  const decide = useDecideBoltJob();
  const {
    data: jobs = [],
    isLoading,
    error,
  } = useBoltJobs({
    status: status || undefined,
    limit: 100,
  });

  const onDecide = (id: string, approve: boolean) => {
    setActionError(null);
    decide.mutate({ id, approve }, { onError: (e) => setActionError(errorText(e)) });
  };

  return (
    <div>
      <div className="flex items-end gap-4 mb-4">
        <label className="text-sm">
          <span className="block text-gray-500 mb-1">Status</span>
          <select
            className="input"
            aria-label="Filter by status"
            value={status}
            onChange={(e) => setStatus(e.target.value as BoltJobStatus | '')}
          >
            <option value="">All</option>
            {BOLT_JOB_STATUSES.map((s) => (
              <option key={s} value={s}>
                {statusLabel(s)}
              </option>
            ))}
          </select>
        </label>
      </div>

      {(error || actionError) && (
        <div role="alert" className="mb-4 p-3 rounded bg-red-50 text-red-700 text-sm">
          {actionError ?? errorText(error)}
        </div>
      )}

      <div className="card overflow-x-auto">
        {isLoading ? (
          <Loader2 className="w-5 h-5 animate-spin" />
        ) : jobs.length === 0 ? (
          <p className="text-sm text-gray-500">No jobs found.</p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-gray-500">
                <th className="py-2">Job</th>
                <th>Status</th>
                <th>Targets</th>
                <th>Requested by</th>
                <th>Created</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {jobs.map((job: BoltJob) => {
                const decision = decisionState(job, actor);
                return (
                  <tr key={job.id} className="border-t border-gray-200 dark:border-gray-700">
                    <td className="py-2">
                      <button
                        type="button"
                        className="font-mono text-left text-primary-600 hover:underline"
                        onClick={() => setSelected(job.id)}
                      >
                        {job.name}
                      </button>
                      <span className="ml-2 text-xs text-gray-500">{job.kind}</span>
                    </td>
                    <td>
                      <BoltStatusBadge status={job.status} />
                    </td>
                    <td>{job.target_count}</td>
                    <td>{job.requested_by}</td>
                    <td>{fmt(job.created_at)}</td>
                    <td className="text-right">
                      {decision.visible && (
                        <span className="inline-flex gap-2">
                          <button
                            type="button"
                            className="btn btn-primary text-xs"
                            disabled={decision.disabled || decide.isPending}
                            title={decision.reason}
                            aria-label={`Approve ${job.name}`}
                            onClick={() => onDecide(job.id, true)}
                          >
                            Approve
                          </button>
                          <button
                            type="button"
                            className="btn btn-danger text-xs"
                            disabled={decision.disabled || decide.isPending}
                            title={decision.reason}
                            aria-label={`Reject ${job.name}`}
                            onClick={() => onDecide(job.id, false)}
                          >
                            Reject
                          </button>
                        </span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {selected && <JobDetail jobId={selected} actor={actor} onClose={() => setSelected(null)} />}
    </div>
  );
}
