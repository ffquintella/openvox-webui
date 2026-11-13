import type { ParamField, ParamValue, ParamValues } from '../../utils/orchestration';

/** Inputs generated from the task/plan parameter metadata. */
export default function ParameterForm({
  fields,
  values,
  errors,
  onChange,
}: {
  fields: ParamField[];
  values: ParamValues;
  errors: Record<string, string>;
  onChange: (name: string, value: ParamValue) => void;
}) {
  if (fields.length === 0) {
    return <p className="text-sm text-gray-500">This item takes no parameters.</p>;
  }
  return (
    <div className="space-y-4">
      {fields.map((f) => {
        const id = `param-${f.name}`;
        const value = values[f.name];
        const text = typeof value === 'string' ? value : '';
        const placeholder = f.hasDefault
          ? `Default: ${typeof f.defaultValue === 'string' ? f.defaultValue : JSON.stringify(f.defaultValue)}`
          : undefined;
        return (
          <div key={f.name} className="text-sm">
            {f.kind === 'boolean' ? (
              <label className="flex items-center gap-2">
                <input
                  id={id}
                  type="checkbox"
                  checked={value === true}
                  onChange={(e) => onChange(f.name, e.target.checked)}
                />
                <span className="font-medium">{f.name}</span>
              </label>
            ) : (
              <label htmlFor={id} className="block font-medium mb-1">
                {f.name}
                {!f.optional && <span className="text-red-600"> *</span>}
              </label>
            )}
            {f.kind === 'enum' ? (
              <select
                id={id}
                className="input w-full"
                value={text}
                onChange={(e) => onChange(f.name, e.target.value)}
              >
                <option value="">{f.optional ? '(not set)' : 'Select...'}</option>
                {f.options?.map((o) => (
                  <option key={o} value={o}>
                    {o}
                  </option>
                ))}
              </select>
            ) : f.kind === 'array' || f.kind === 'hash' || f.kind === 'json' ? (
              <textarea
                id={id}
                className="input w-full font-mono text-xs"
                rows={3}
                spellCheck={false}
                placeholder={
                  placeholder ??
                  (f.kind === 'array'
                    ? '["a", "b"]'
                    : f.kind === 'hash'
                      ? '{"key": "value"}'
                      : 'JSON value')
                }
                value={text}
                onChange={(e) => onChange(f.name, e.target.value)}
              />
            ) : f.kind === 'boolean' ? null : (
              <input
                id={id}
                className="input w-full"
                type={
                  f.sensitive
                    ? 'password'
                    : f.kind === 'integer' || f.kind === 'float'
                      ? 'number'
                      : 'text'
                }
                step={f.kind === 'float' ? 'any' : undefined}
                autoComplete={f.sensitive ? 'new-password' : 'off'}
                placeholder={placeholder}
                value={text}
                onChange={(e) => onChange(f.name, e.target.value)}
              />
            )}
            <p className="text-xs text-gray-500 mt-1">
              <code>{f.typeLabel}</code>
              {f.sensitive && ' (sensitive, never stored in clear)'}
              {f.description ? ` - ${f.description}` : ''}
            </p>
            {errors[f.name] && (
              <p role="alert" className="text-xs text-red-600 mt-1">
                {errors[f.name]}
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}
