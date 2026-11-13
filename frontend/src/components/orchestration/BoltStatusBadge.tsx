import { BOLT_STATUS_CLASSES, statusLabel } from '../../utils/orchestration';
import type { BoltJobStatus } from '../../types';

export default function BoltStatusBadge({ status }: { status: BoltJobStatus | string }) {
  const cls = BOLT_STATUS_CLASSES[status as BoltJobStatus] ?? 'bg-gray-100 text-gray-800';
  return (
    <span
      className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${cls}`}
      data-status={status}
    >
      {statusLabel(status)}
    </span>
  );
}
