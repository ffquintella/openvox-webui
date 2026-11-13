import { useState } from 'react';
import { Loader2, X } from 'lucide-react';
import { useBoltJob, useCancelBoltJob, useDecideBoltJob } from '../../hooks/useOrchestration';
import { cancelState, decisionState, errorText, type JobActor } from '../../utils/orchestration';
import PluginSlot from '../plugins/PluginSlot';
import BoltStatusBadge from './BoltStatusBadge';

function fmt(value?: string | null): string {
  return value ? new Date(value).toLocaleString() : '-';
}

/** Detail drawer of one job: targets, output and the approve/reject/cancel actions. */
export default function JobDetail({
  jobId,
  actor,
  onClose,
}: {
  jobId: string;
  actor: JobActor;
  onClose: () => void;
}) {
  const { data: job, isLoading, error: loadError } = useBoltJob(jobId);
  const decide = useDecideBoltJob();
  const cancel = useCancelBoltJob();
  const [notes, setNotes] = useState('');
  const [error, setError] = useState<string | null>(null);

  const decision = job ? decisionState(job, actor) : null;
  const cancelBtn = job ? cancelState(job, actor) : null;

  const onDecide = (approve: boolean) => {
    setError(null);
    decide.mutate(
      { id: jobId, approve, notes: notes.trim() || undefined },
      { onError: (e) => setError(errorText(e)) }
    );
  };

  const onCancel = () => {
    if (!window.confirm('Cancel this job?')) return;
    setError(null);
    cancel.mutate(jobId, { onError: (e) => setError(errorText(e)) });
  };

  return (
    <div className="fixed inset-0 z-40 flex justify-end bg-black/40" role="presentation">
      <aside
        role="dialog"
        aria-label="Job detail"
        className="w-full max-w-2xl h-full overflow-y-auto bg-white dark:bg-gray-800 shadow-xl p-6"
      >
        <div className="flex items-start justify-between mb-4">
          <h2 className="text-lg font-semibold">Job detail</h2>
          <button type="button" onClick={onClose} aria-label="Close" className="text-gray-500">
            <X className="w-5 h-5" />
          </button>
        </div>

        {isLoading && <Loader2 className="w-5 h-5 animate-spin" />}
        {loadError && (
          <div role="alert" className="p-3 rounded bg-red-50 text-red-700 text-sm">
            {errorText(loadError)}
          </div>
        )}
        {error && (
          <div role="alert" className="mb-4 p-3 rounded bg-red-50 text-red-700 text-sm">
            {error}
          </div>
        )}

        {job && (
          <div className="space-y-4 text-sm">
            <div className="flex items-center gap-3">
              <span className="font-mono font-medium break-all">{job.name}</span>
              <BoltStatusBadge status={job.status} />
              {job.noop && <span className="text-xs text-gray-500">noop</span>}
            </div>
            <dl className="grid grid-cols-2 gap-x-4 gap-y-1">
              <dt className="text-gray-500">Kind</dt>
              <dd>{job.kind}</dd>
              <dt className="text-gray-500">Requested by</dt>
              <dd>{job.requested_by}</dd>
              <dt className="text-gray-500">Targets</dt>
              <dd>{job.target_count}</dd>
              <dt className="text-gray-500">Created</dt>
              <dd>{fmt(job.created_at)}</dd>
              <dt className="text-gray-500">Started</dt>
              <dd>{fmt(job.started_at)}</dd>
              <dt className="text-gray-500">Finished</dt>
              <dd>{fmt(job.finished_at)}</dd>
              {job.requires_approval && (
                <>
                  <dt className="text-gray-500">Approval</dt>
                  <dd>
                    {job.approved_by
                      ? `Decided by ${job.approved_by}`
                      : job.status === 'pending_approval'
                        ? 'Required, pending'
                        : 'Required'}
                  </dd>
                </>
              )}
              {job.approval_notes && (
                <>
                  <dt className="text-gray-500">Notes</dt>
                  <dd className="whitespace-pre-wrap">{job.approval_notes}</dd>
                </>
              )}
              {(job.maintenance_window_start || job.maintenance_window_end) && (
                <>
                  <dt className="text-gray-500">Maintenance window</dt>
                  <dd>
                    {fmt(job.maintenance_window_start)} to {fmt(job.maintenance_window_end)}
                  </dd>
                </>
              )}
            </dl>

            {job.error && (
              <div role="alert" className="p-3 rounded bg-red-50 text-red-700 whitespace-pre-wrap">
                {job.error}
              </div>
            )}

            {job.parameters && Object.keys(job.parameters).length > 0 && (
              <div>
                <h3 className="font-medium mb-1">Parameters</h3>
                <pre className="p-2 rounded bg-gray-100 dark:bg-gray-900 overflow-x-auto text-xs">
                  {JSON.stringify(job.parameters, null, 2)}
                </pre>
              </div>
            )}

            {(decision?.visible || cancelBtn?.visible) && (
              <div className="border-t border-gray-200 dark:border-gray-700 pt-4 space-y-2">
                {decision?.visible && (
                  <>
                    <label className="block">
                      <span className="block text-gray-500 mb-1">Notes (optional)</span>
                      <input
                        className="input w-full"
                        aria-label="Decision notes"
                        value={notes}
                        onChange={(e) => setNotes(e.target.value)}
                      />
                    </label>
                    <div className="flex gap-2">
                      <button
                        type="button"
                        className="btn btn-primary"
                        disabled={decision.disabled || decide.isPending}
                        title={decision.reason}
                        onClick={() => onDecide(true)}
                      >
                        Approve
                      </button>
                      <button
                        type="button"
                        className="btn btn-danger"
                        disabled={decision.disabled || decide.isPending}
                        title={decision.reason}
                        onClick={() => onDecide(false)}
                      >
                        Reject
                      </button>
                    </div>
                    {decision.reason && <p className="text-xs text-gray-500">{decision.reason}</p>}
                  </>
                )}
                {cancelBtn?.visible && (
                  <button
                    type="button"
                    className="btn btn-secondary"
                    disabled={cancelBtn.disabled || cancel.isPending}
                    onClick={onCancel}
                  >
                    Cancel job
                  </button>
                )}
              </div>
            )}

            <div>
              <h3 className="font-medium mb-2">Targets</h3>
              {job.targets.length === 0 ? (
                <p className="text-gray-500">No results yet.</p>
              ) : (
                <ul className="space-y-3">
                  {job.targets.map((t) => (
                    <li
                      key={t.certname}
                      className="border border-gray-200 dark:border-gray-700 rounded p-3"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono break-all">{t.certname}</span>
                        <span className="flex items-center gap-2">
                          <BoltStatusBadge status={t.status} />
                          <span className="text-xs text-gray-500">exit {t.exit_code ?? '-'}</span>
                        </span>
                      </div>
                      {t.output ? (
                        <pre className="mt-2 p-2 rounded bg-gray-100 dark:bg-gray-900 overflow-x-auto text-xs whitespace-pre-wrap break-words max-h-80">
                          {t.output}
                        </pre>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <PluginSlot
              slot="orchestration.job.panels"
              params={{ job_id: job.id }}
              className="space-y-4"
            />
          </div>
        )}
      </aside>
    </div>
  );
}
