import { Loader2 } from 'lucide-react';
import { useReportExecutions } from '../../hooks/useAnalytics';
import type { ReportExecution } from '../../types';
import ExportMenu from './ExportMenu';

/**
 * Shows a saved report execution. Plugin reports start as pending/running, so the executions
 * list is polled until this one finishes; `render` draws the completed output.
 */
export default function ExecutionResult({
  reportId,
  initial,
  render,
}: {
  reportId: string;
  initial: ReportExecution;
  render: (output: unknown) => React.ReactNode;
}) {
  const { data: executions } = useReportExecutions(reportId);
  const current = executions?.find((e) => e.id === initial.id) ?? initial;
  const unfinished = current.status === 'pending' || current.status === 'running';

  return (
    <div className="space-y-4">
      {unfinished && (
        <div role="status" className="flex items-center gap-2 text-sm text-gray-600">
          <Loader2 className="w-4 h-4 animate-spin" />
          Report is {current.status}...
        </div>
      )}
      {current.status === 'failed' && (
        <p role="alert" className="text-sm text-red-600">
          Report failed{current.error_message ? `: ${current.error_message}` : '.'}
        </p>
      )}
      {current.status === 'completed' && (
        <>
          <div className="flex justify-end">
            <ExportMenu executionId={current.id} />
          </div>
          {current.output_data ? (
            render(current.output_data)
          ) : (
            <p className="text-sm text-gray-500">The report produced no output.</p>
          )}
        </>
      )}
    </div>
  );
}
