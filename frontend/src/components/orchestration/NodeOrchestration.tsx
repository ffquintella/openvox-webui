import { Link } from 'react-router-dom';
import { Terminal } from 'lucide-react';
import { useBoltStatus, useNodeBoltJobs } from '../../hooks/useOrchestration';
import { usePermissionsStore } from '../../stores/permissionsStore';
import BoltStatusBadge from './BoltStatusBadge';

/** Header action of the node page: opens the Run tab with this node selected. */
export function NodeRunTaskButton({ certname }: { certname: string }) {
  const canCreate = usePermissionsStore((s) => s.hasPermission('orchestration', 'create'));
  const { data: status } = useBoltStatus();
  if (!canCreate || !status?.enabled) return null;
  return (
    <Link
      to={`/orchestration?tab=run&node=${encodeURIComponent(certname)}`}
      className="flex items-center gap-2 px-4 py-2 text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-lg transition-colors"
    >
      <Terminal className="w-4 h-4" />
      Run task
    </Link>
  );
}

/** The last orchestration jobs that targeted this node. */
export function NodeOrchestrationHistory({ certname }: { certname: string }) {
  const canRead = usePermissionsStore((s) => s.hasPermission('orchestration', 'read'));
  const { data: status } = useBoltStatus();
  const enabled = canRead && status?.enabled === true;
  const { data: jobs = [], isLoading } = useNodeBoltJobs(certname, enabled);
  if (!enabled) return null;

  return (
    <div className="card mt-6">
      <h3 className="text-lg font-semibold text-gray-900 mb-3">Orchestration history</h3>
      {isLoading ? (
        <p className="text-sm text-gray-500">Loading...</p>
      ) : jobs.length === 0 ? (
        <p className="text-sm text-gray-500">No orchestration jobs have targeted this node.</p>
      ) : (
        <ul className="divide-y divide-gray-200">
          {jobs.map((job) => (
            <li key={job.id} className="py-2 flex items-center justify-between gap-3 text-sm">
              <Link
                to={`/orchestration?tab=jobs&job=${encodeURIComponent(job.id)}`}
                className="font-mono text-primary-600 hover:underline break-all"
              >
                {job.name}
              </Link>
              <span className="flex items-center gap-3 shrink-0">
                <BoltStatusBadge status={job.status} />
                <span className="text-xs text-gray-500">
                  {new Date(job.created_at).toLocaleString()}
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
