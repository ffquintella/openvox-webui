import type { JsonSchema } from '../../types';
import { inputClass } from '../plugins/pluginHelpers';
import { schemaToFields, type ParamField, type ParamValues } from './pluginReportHelpers';

interface Props {
  schema: JsonSchema | null | undefined;
  values: ParamValues;
  onChange: (values: ParamValues) => void;
  /** Field name to message, shown under the field. */
  errors?: Record<string, string>;
  idPrefix?: string;
}

function FieldInput({
  field,
  id,
  value,
  invalid,
  onChange,
}: {
  field: ParamField;
  id: string;
  value: unknown;
  invalid: boolean;
  onChange: (v: unknown) => void;
}) {
  const common = {
    id,
    'aria-invalid': invalid || undefined,
    'aria-required': field.required || undefined,
  };
  const text =
    typeof value === 'string' ? value : value === undefined || value === null ? '' : String(value);

  switch (field.kind) {
    case 'boolean':
      return (
        <input
          {...common}
          type="checkbox"
          checked={value === true}
          onChange={(e) => onChange(e.target.checked)}
          className="h-4 w-4 rounded border-gray-300"
        />
      );
    case 'enum':
      return (
        <select
          {...common}
          value={text}
          onChange={(e) => onChange(e.target.value)}
          className={inputClass}
        >
          <option value="">{field.required ? 'Select...' : 'Not set'}</option>
          {field.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      );
    case 'integer':
    case 'number':
      return (
        <input
          {...common}
          type="number"
          step={field.kind === 'integer' ? 1 : 'any'}
          min={field.minimum ?? undefined}
          max={field.maximum ?? undefined}
          value={text}
          onChange={(e) => onChange(e.target.value)}
          className={inputClass}
        />
      );
    case 'json':
      return (
        <textarea
          {...common}
          rows={3}
          value={text}
          onChange={(e) => onChange(e.target.value)}
          placeholder="JSON value"
          className={`${inputClass} font-mono`}
        />
      );
    default:
      return (
        <input
          {...common}
          type={
            field.format === 'date'
              ? 'date'
              : field.format === 'date-time'
                ? 'datetime-local'
                : 'text'
          }
          value={text}
          onChange={(e) => onChange(e.target.value)}
          className={inputClass}
        />
      );
  }
}

/** Renders the fields of a plugin report type's `params_schema`. */
export default function PluginParamsForm({
  schema,
  values,
  onChange,
  errors = {},
  idPrefix = 'param',
}: Props) {
  const fields = schemaToFields(schema);
  if (fields.length === 0) {
    return (
      <p className="text-sm text-gray-500 dark:text-gray-400">This report has no parameters.</p>
    );
  }
  return (
    <div className="space-y-4">
      {fields.map((f) => {
        const id = `${idPrefix}-${f.name}`;
        const error = errors[f.name];
        return (
          <div key={f.name}>
            <label
              htmlFor={id}
              className="block text-sm font-medium text-gray-700 dark:text-gray-200 mb-1"
            >
              {f.title}
              {f.required && <span className="text-red-600"> *</span>}
            </label>
            <FieldInput
              field={f}
              id={id}
              value={values[f.name]}
              invalid={!!error}
              onChange={(v) => onChange({ ...values, [f.name]: v })}
            />
            {f.description && (
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">{f.description}</p>
            )}
            {error && (
              <p role="alert" className="text-xs text-red-600 mt-1">
                {error}
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}
