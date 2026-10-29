import {
  useClassificationBehaviors,
  useClassificationMergeStrategies,
} from '../../hooks/useClassification';
import {
  FALLBACK_MERGE_STRATEGIES,
  INHERITANCE_MODES,
  MATCH_MODES,
  type BehaviorFormState,
} from './expressionHelpers';
import type { InheritanceMode, MatchModeKind, MergeStrategyName } from '../../types';

interface Props {
  priority: string;
  onPriorityChange: (value: string) => void;
  behavior: BehaviorFormState;
  onBehaviorChange: (next: BehaviorFormState) => void;
  /** Validation problems for the behavior fields */
  errors: string[];
}

/** Priority and behavior settings of a group. */
export default function BehaviorSection({
  priority,
  onPriorityChange,
  behavior,
  onBehaviorChange,
  errors,
}: Props) {
  const mergeStrategies = useClassificationMergeStrategies();
  const behaviors = useClassificationBehaviors();

  const set = (patch: Partial<BehaviorFormState>) => onBehaviorChange({ ...behavior, ...patch });

  const strategies =
    mergeStrategies.data && mergeStrategies.data.length > 0
      ? mergeStrategies.data
      : FALLBACK_MERGE_STRATEGIES;
  const mergeDescription = strategies.find((s) => s.name === behavior.merge)?.description;
  const help = (key: string) => behaviors.data?.find((b) => b.key === key)?.description;

  return (
    <fieldset className="space-y-3 rounded border border-gray-200 p-3">
      <legend className="px-1 text-sm font-semibold text-gray-900">Behavior</legend>
      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="label" htmlFor="group-priority">
            Priority
          </label>
          <input
            id="group-priority"
            type="number"
            step={1}
            value={priority}
            onChange={(e) => onPriorityChange(e.target.value)}
            className="input"
          />
          <p className="text-xs text-gray-500 mt-1">Higher wins on conflicts (default 100).</p>
        </div>
        <div>
          <label className="label" htmlFor="group-merge">
            Merge strategy
          </label>
          <select
            id="group-merge"
            value={behavior.merge}
            onChange={(e) => set({ merge: e.target.value as MergeStrategyName })}
            className="input"
          >
            {strategies.map((s) => (
              <option key={s.name} value={s.name}>
                {s.name}
              </option>
            ))}
          </select>
          {mergeDescription && <p className="text-xs text-gray-500 mt-1">{mergeDescription}</p>}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="label" htmlFor="group-inheritance">
            Inheritance
          </label>
          <select
            id="group-inheritance"
            value={behavior.inheritanceMode}
            onChange={(e) => set({ inheritanceMode: e.target.value as InheritanceMode })}
            className="input"
          >
            {INHERITANCE_MODES.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </select>
          {help('inheritance') && (
            <p className="text-xs text-gray-500 mt-1">{help('inheritance')}</p>
          )}
        </div>
        {behavior.inheritanceMode === 'select' && (
          <div>
            <label className="label" htmlFor="group-inheritance-keys">
              Inherited keys
            </label>
            <input
              id="group-inheritance-keys"
              type="text"
              value={behavior.inheritanceKeys}
              onChange={(e) => set({ inheritanceKeys: e.target.value })}
              className="input"
              placeholder="comma separated class or variable names"
            />
          </div>
        )}
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="label" htmlFor="group-exclusive-set">
            Exclusive set
          </label>
          <input
            id="group-exclusive-set"
            type="text"
            value={behavior.exclusiveSet}
            onChange={(e) => set({ exclusiveSet: e.target.value })}
            className="input"
            placeholder="optional, e.g. os-family"
          />
          <p className="text-xs text-gray-500 mt-1">
            A node joins at most one group of the same set (highest priority wins).
          </p>
        </div>
        <div>
          <label className="label" htmlFor="group-match-mode">
            Match mode
          </label>
          <div className="flex gap-2">
            <select
              id="group-match-mode"
              value={behavior.matchMode}
              onChange={(e) => set({ matchMode: e.target.value as '' | MatchModeKind })}
              className="input"
            >
              {MATCH_MODES.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </select>
            {behavior.matchMode === 'n_of' && (
              <input
                aria-label="Minimum matching conditions (N)"
                type="number"
                min={1}
                step={1}
                value={behavior.matchK}
                onChange={(e) => set({ matchK: e.target.value })}
                className="input w-20"
              />
            )}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div>
          <label className="label" htmlFor="group-active-from">
            Active from
          </label>
          <input
            id="group-active-from"
            type="datetime-local"
            value={behavior.activeFrom}
            onChange={(e) => set({ activeFrom: e.target.value })}
            className="input"
          />
        </div>
        <div>
          <label className="label" htmlFor="group-expires-at">
            Expires at
          </label>
          <input
            id="group-expires-at"
            type="datetime-local"
            value={behavior.expiresAt}
            onChange={(e) => set({ expiresAt: e.target.value })}
            className="input"
          />
        </div>
      </div>

      {errors.length > 0 && (
        <ul className="text-xs text-red-700 space-y-0.5" role="alert">
          {errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      )}
    </fieldset>
  );
}
