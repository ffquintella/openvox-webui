import { useEffect, useMemo, useState } from 'react';
import { Loader2, Play, X } from 'lucide-react';
import NodeAutocomplete from '../NodeAutocomplete';
import ParameterForm from './ParameterForm';
import { useGroups, useGroupNodes } from '../../hooks/useGroups';
import { useBoltPlans, useBoltTasks, useCreateBoltJob } from '../../hooks/useOrchestration';
import {
  buildParameters,
  errorText,
  exceedsTargetLimit,
  initialParamValues,
  parameterFields,
  runNeedsApproval,
  type ParamValue,
  type ParamValues,
} from '../../utils/orchestration';
import type {
  BoltJob,
  BoltKind,
  BoltRunnable,
  BoltStatus,
  CreateBoltJobRequest,
} from '../../types';

function toIso(local: string): string | undefined {
  if (!local) return undefined;
  const d = new Date(local);
  return Number.isNaN(d.getTime()) ? undefined : d.toISOString();
}

/** Form that runs an allowlisted task or plan (or, for administrators, a command). */
export default function RunPanel({
  status,
  canRunCommands,
  initialNode,
  onCreated,
}: {
  status: BoltStatus;
  canRunCommands: boolean;
  initialNode?: string | null;
  onCreated: (job: BoltJob) => void;
}) {
  const commandsEnabled = canRunCommands && status.allow_arbitrary_commands === true;
  const { data: tasks = [], isLoading: tasksLoading } = useBoltTasks(true);
  const { data: plans = [], isLoading: plansLoading } = useBoltPlans(true);
  const { data: groups = [] } = useGroups();
  const create = useCreateBoltJob();

  const [kind, setKind] = useState<BoltKind>('task');
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [values, setValues] = useState<ParamValues>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [targetMode, setTargetMode] = useState<'nodes' | 'group'>('nodes');
  const [nodes, setNodes] = useState<string[]>(initialNode ? [initialNode] : []);
  const [autoKey, setAutoKey] = useState(0);
  const [groupId, setGroupId] = useState('');
  const [noop, setNoop] = useState(false);
  const [winStart, setWinStart] = useState('');
  const [winEnd, setWinEnd] = useState('');
  const [error, setError] = useState<string | null>(null);

  const { data: groupNodes } = useGroupNodes(
    targetMode === 'group' ? groupId || undefined : undefined
  );

  useEffect(() => {
    if (initialNode) setNodes((n) => (n.includes(initialNode) ? n : [...n, initialNode]));
  }, [initialNode]);

  const items: BoltRunnable[] = kind === 'task' ? tasks : kind === 'plan' ? plans : [];
  const item = items.find((i) => i.name === name) ?? null;
  const fields = useMemo(() => parameterFields(item?.parameters), [item]);

  const pick = (next: string) => {
    setName(next);
    const picked = items.find((i) => i.name === next);
    setValues(initialParamValues(parameterFields(picked?.parameters)));
    setErrors({});
    setNoop(false);
  };

  const changeKind = (next: BoltKind) => {
    setKind(next);
    setName('');
    setValues({});
    setErrors({});
    setNoop(false);
  };

  const targetCount = targetMode === 'nodes' ? nodes.length : (groupNodes?.length ?? 0);
  const tooMany = exceedsTargetLimit(targetCount, status.limits);
  const approval = kind === 'command' || runNeedsApproval(item, targetCount, status.limits);
  const hasTargets = targetMode === 'nodes' ? nodes.length > 0 : !!groupId;
  const hasSubject = kind === 'command' ? command.trim() !== '' : !!item;
  const canSubmit = hasSubject && hasTargets && !tooMany && !create.isPending;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    let parameters: Record<string, unknown> = {};
    if (kind !== 'command') {
      const built = buildParameters(fields, values);
      setErrors(built.errors);
      if (Object.keys(built.errors).length > 0) return;
      parameters = built.params;
    }
    const request: CreateBoltJobRequest = {
      kind,
      name: kind === 'command' ? command.trim() : name,
      parameters,
      nodes: targetMode === 'nodes' ? nodes : [],
      group_id: targetMode === 'group' ? groupId : undefined,
      noop: kind !== 'command' && item?.supports_noop === true && noop,
      maintenance_window_start: toIso(winStart),
      maintenance_window_end: toIso(winEnd),
    };
    create.mutate(request, {
      onSuccess: (job) => onCreated(job),
      onError: (err) => setError(errorText(err)),
    });
  };

  const loading = kind === 'task' ? tasksLoading : kind === 'plan' ? plansLoading : false;

  return (
    <form className="card space-y-6" onSubmit={submit}>
      {error && (
        <div role="alert" className="p-3 rounded bg-red-50 text-red-700 text-sm">
          {error}
        </div>
      )}

      <div className="flex flex-wrap gap-4 items-end">
        <label className="text-sm">
          <span className="block text-gray-500 mb-1">Type</span>
          <select
            className="input"
            aria-label="Run type"
            value={kind}
            onChange={(e) => changeKind(e.target.value as BoltKind)}
          >
            <option value="task">Task</option>
            <option value="plan">Plan</option>
            {commandsEnabled && <option value="command">Command</option>}
          </select>
        </label>
        {kind !== 'command' && (
          <label className="flex-1 min-w-[240px] text-sm">
            <span className="block text-gray-500 mb-1">{kind === 'task' ? 'Task' : 'Plan'}</span>
            <select
              className="input w-full"
              aria-label="Item to run"
              value={name}
              onChange={(e) => pick(e.target.value)}
            >
              <option value="">{loading ? 'Loading...' : 'Select...'}</option>
              {items.map((i) => (
                <option key={i.name} value={i.name}>
                  {i.name}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>

      {kind !== 'command' && !loading && items.length === 0 && (
        <p className="text-sm text-gray-500">
          No {kind}s are allowed yet. An administrator can enable them on the Allowlist tab.
        </p>
      )}
      {item?.description && <p className="text-sm text-gray-500">{item.description}</p>}

      {kind === 'command' && (
        <label className="block text-sm">
          <span className="block text-gray-500 mb-1">Command</span>
          <input
            className="input w-full font-mono"
            aria-label="Command"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
          />
          <span className="block text-xs text-gray-500 mt-1">
            Runs as written on every target and always needs approval from another user.
          </span>
        </label>
      )}

      {item && (
        <div>
          <h3 className="font-medium mb-2">Parameters</h3>
          <ParameterForm
            fields={fields}
            values={values}
            errors={errors}
            onChange={(n: string, v: ParamValue) => setValues((prev) => ({ ...prev, [n]: v }))}
          />
        </div>
      )}

      <div>
        <h3 className="font-medium mb-2">Targets</h3>
        <div className="flex gap-4 mb-3 text-sm">
          <label className="flex items-center gap-1">
            <input
              type="radio"
              name="target-mode"
              checked={targetMode === 'nodes'}
              onChange={() => setTargetMode('nodes')}
            />
            Nodes
          </label>
          <label className="flex items-center gap-1">
            <input
              type="radio"
              name="target-mode"
              checked={targetMode === 'group'}
              onChange={() => setTargetMode('group')}
            />
            Group
          </label>
        </div>
        {targetMode === 'nodes' ? (
          <div className="space-y-2">
            <NodeAutocomplete
              key={autoKey}
              value=""
              excluded={nodes}
              placeholder="Search nodes to add..."
              onChange={(c) => {
                if (!c) return;
                setNodes((prev) => (prev.includes(c) ? prev : [...prev, c]));
                setAutoKey((k) => k + 1);
              }}
            />
            {nodes.length > 0 && (
              <ul className="flex flex-wrap gap-2" aria-label="Selected nodes">
                {nodes.map((n) => (
                  <li
                    key={n}
                    className="inline-flex items-center gap-1 px-2 py-1 rounded bg-gray-100 dark:bg-gray-700 text-xs font-mono"
                  >
                    {n}
                    <button
                      type="button"
                      aria-label={`Remove ${n}`}
                      onClick={() => setNodes((prev) => prev.filter((x) => x !== n))}
                    >
                      <X className="w-3 h-3" />
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : (
          <select
            className="input"
            aria-label="Target group"
            value={groupId}
            onChange={(e) => setGroupId(e.target.value)}
          >
            <option value="">Select a group...</option>
            {groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        )}
        <p className="text-xs text-gray-500 mt-2">
          {targetCount} target{targetCount === 1 ? '' : 's'}
        </p>
      </div>

      {item?.supports_noop && kind === 'task' && (
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={noop} onChange={(e) => setNoop(e.target.checked)} />
          No-op (report what would change without changing it)
        </label>
      )}

      <div className="flex flex-wrap gap-4 text-sm">
        <label>
          <span className="block text-gray-500 mb-1">Window start (optional)</span>
          <input
            type="datetime-local"
            className="input"
            aria-label="Maintenance window start"
            value={winStart}
            onChange={(e) => setWinStart(e.target.value)}
          />
        </label>
        <label>
          <span className="block text-gray-500 mb-1">Window end (optional)</span>
          <input
            type="datetime-local"
            className="input"
            aria-label="Maintenance window end"
            value={winEnd}
            onChange={(e) => setWinEnd(e.target.value)}
          />
        </label>
      </div>

      {tooMany && (
        <div role="alert" className="p-3 rounded bg-red-50 text-red-700 text-sm">
          A run may have at most {status.limits?.max_targets_per_run} targets; this one has{' '}
          {targetCount}.
        </div>
      )}
      {!tooMany && approval && hasSubject && (
        <div role="status" className="p-3 rounded bg-yellow-50 text-yellow-800 text-sm">
          This run needs approval from another user before it starts.
        </div>
      )}

      <button type="submit" className="btn btn-primary" disabled={!canSubmit}>
        {create.isPending ? (
          <Loader2 className="w-4 h-4 animate-spin" />
        ) : (
          <Play className="w-4 h-4" />
        )}
        <span className="ml-1">{approval ? 'Request run' : 'Run'}</span>
      </button>
    </form>
  );
}
