import { useEffect, useState } from 'react';
import { Loader2 } from 'lucide-react';
import { usePluginConfigStatus, useSavePluginConfig } from '../../hooks/usePlugins';
import { ErrorBox, WarningBox } from './pluginUi';
import { getApiErrorMessage, inputClass, primaryBtn } from './pluginHelpers';
import {
  addressSuggestions,
  fieldKind,
  linesToList,
  listToLines,
  pruneConfigObject,
} from './configForm';
import type { PluginConfigSchema } from '../../types';

type Obj = Record<string, unknown>;

function asObj(v: unknown): Obj {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Obj) : {};
}

function JsonField({ value, onChange }: { value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(value === undefined ? '' : JSON.stringify(value, null, 2));
  const [bad, setBad] = useState(false);
  return (
    <>
      <textarea
        className={`${inputClass} font-mono`}
        rows={4}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          if (!e.target.value.trim()) {
            setBad(false);
            onChange(undefined);
            return;
          }
          try {
            onChange(JSON.parse(e.target.value));
            setBad(false);
          } catch {
            setBad(true);
          }
        }}
      />
      {bad && <p className="mt-1 text-xs text-red-600">Not valid JSON</p>}
    </>
  );
}

function Field({
  name,
  schema,
  required,
  value,
  onChange,
}: {
  name: string;
  schema: PluginConfigSchema;
  required: boolean;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const kind = fieldKind(schema);
  const id = `cfg-${name}`;
  const label = (
    <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-200">
      {name.split('.').pop()}
      {required && <span className="text-red-600"> *</span>}
    </label>
  );
  const hint = schema.description ? (
    <p className="mt-1 text-xs text-gray-500">{schema.description}</p>
  ) : null;

  if (kind === 'object') {
    const props = schema.properties ?? {};
    const current = asObj(value);
    return (
      <fieldset className="space-y-3 rounded-lg border border-gray-200 dark:border-gray-700 p-3">
        <legend className="px-1 text-sm font-semibold text-gray-900 dark:text-white">
          {name.split('.').pop()}
          {required && <span className="text-red-600"> *</span>}
        </legend>
        {hint}
        {Object.entries(props).map(([k, sub]) => (
          <Field
            key={k}
            name={`${name}.${k}`}
            schema={sub}
            required={schema.required?.includes(k) ?? false}
            value={current[k]}
            onChange={(v) => onChange({ ...current, [k]: v })}
          />
        ))}
      </fieldset>
    );
  }

  const suggestions =
    kind === 'string' || kind === 'strings'
      ? addressSuggestions(kind === 'strings' ? schema.items : schema, window.location)
      : [];
  const suggest = suggestions.map((s) => (
    <button
      key={s}
      type="button"
      className="mt-1 mr-2 text-xs text-blue-600 hover:underline"
      onClick={() => onChange(kind === 'strings' ? linesToList(`${listToLines(value)}\n${s}`) : s)}
    >
      Use {s}
    </button>
  ));

  return (
    <div>
      {label}
      {kind === 'enum' && (
        <select
          id={id}
          className={inputClass}
          value={value === undefined ? '' : String(value)}
          onChange={(e) =>
            onChange(schema.enum?.find((o) => String(o) === e.target.value) ?? undefined)
          }
        >
          <option value="">
            {schema.default !== undefined ? `Default (${String(schema.default)})` : '—'}
          </option>
          {schema.enum?.map((o) => (
            <option key={String(o)} value={String(o)}>
              {String(o)}
            </option>
          ))}
        </select>
      )}
      {kind === 'boolean' && (
        <input
          id={id}
          type="checkbox"
          className="mt-1 block"
          checked={value === undefined ? schema.default === true : value === true}
          onChange={(e) => onChange(e.target.checked)}
        />
      )}
      {kind === 'number' && (
        <input
          id={id}
          type="number"
          className={inputClass}
          min={schema.minimum}
          max={schema.maximum}
          placeholder={schema.default !== undefined ? String(schema.default) : undefined}
          value={value === undefined ? '' : String(value)}
          onChange={(e) => onChange(e.target.value === '' ? undefined : Number(e.target.value))}
        />
      )}
      {kind === 'string' && (
        <input
          id={id}
          type="text"
          className={inputClass}
          maxLength={schema.maxLength}
          placeholder={schema.default !== undefined ? String(schema.default) : undefined}
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
      {kind === 'strings' && (
        <textarea
          id={id}
          className={`${inputClass} font-mono`}
          rows={3}
          placeholder="One entry per line"
          value={listToLines(value)}
          onChange={(e) => onChange(e.target.value.split('\n').map((l) => l.trim()))}
        />
      )}
      {kind === 'json' && <JsonField value={value} onChange={onChange} />}
      {hint}
      {suggest}
    </div>
  );
}

/**
 * Settings form generated from the plugin's `config_schema`. A plugin whose
 * settings are incomplete cannot be enabled, so the missing ones are listed.
 */
export default function PluginConfigPanel({
  pluginId,
  canAdmin,
}: {
  pluginId: string;
  canAdmin: boolean;
}) {
  const { data, isLoading, error } = usePluginConfigStatus(pluginId);
  const save = useSavePluginConfig();
  const [values, setValues] = useState<Obj>({});
  const [saved, setSaved] = useState(false);

  const stored = JSON.stringify(data?.config ?? {});
  useEffect(() => {
    setValues(asObj(JSON.parse(stored)));
  }, [stored]);

  if (isLoading) return <Loader2 className="w-5 h-5 animate-spin text-blue-500" />;
  if (error)
    return <ErrorBox message={getApiErrorMessage(error, 'Settings could not be loaded')} />;
  if (!data?.schema) return null;
  const schema = data.schema;

  return (
    <section>
      <h4 className="text-sm font-semibold text-gray-900 dark:text-white">Settings</h4>
      {!data.valid && (
        <div className="mt-2">
          <WarningBox
            messages={[
              'The plugin cannot be enabled until its settings are complete:',
              ...data.errors,
            ]}
          />
        </div>
      )}
      <form
        className="mt-3 space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          setSaved(false);
          save.mutate(
            { id: pluginId, config: pruneConfigObject(values) },
            { onSuccess: () => setSaved(true) }
          );
        }}
      >
        {Object.entries(schema.properties ?? {}).map(([k, sub]) => (
          <Field
            key={k}
            name={k}
            schema={sub}
            required={schema.required?.includes(k) ?? false}
            value={values[k]}
            onChange={(v) => {
              setSaved(false);
              setValues((prev) => ({ ...prev, [k]: v }));
            }}
          />
        ))}
        {save.error && (
          <ErrorBox message={getApiErrorMessage(save.error, 'Settings were not saved')} />
        )}
        {saved && <p className="text-sm text-green-700 dark:text-green-300">Settings saved.</p>}
        {canAdmin && (
          <button type="submit" className={primaryBtn} disabled={save.isPending}>
            Save settings
          </button>
        )}
      </form>
    </section>
  );
}
