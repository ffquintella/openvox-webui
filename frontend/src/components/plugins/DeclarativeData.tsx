import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { CircleAlert, Info, Loader2 } from 'lucide-react';
import clsx from 'clsx';
import { t } from '../../i18n';
import { pluginsApi } from '../../services/api';
import { useDeclarative } from './declarativeContext';
import {
  asArray,
  buildBody,
  displayText,
  fieldType,
  fillRoute,
  filterQuery,
  findRoute,
  getPath,
  initialValues,
  isEmptyValue,
  isObject,
  matchesWhen,
  routeErrorMessage,
  staticQuery,
  statusTone,
  str,
  type FieldValues,
  type Json,
  type Tone,
} from './declarativeSpec';
import { dangerBtn, inputClass, primaryBtn, secondaryBtn } from './pluginHelpers';

/**
 * Data-bound declarative blocks (H-6): tables, details, stats, notices and
 * forms whose data and actions go only through the plugin's own routes via
 * the host API client, so the session, the step-up retry (StepUpDialog) and
 * the error handling are the host's. Every value is rendered as text.
 */

const ROUTE_QUERY = 'plugin-route';
const SOURCE_STALE_MS = 10_000;

const TONE_NOTE: Record<Tone, string> = {
  neutral:
    'border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 text-gray-700 dark:text-gray-300',
  info: 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20 text-blue-800 dark:text-blue-300',
  success:
    'border-success-500/40 bg-success-50 dark:bg-green-900/20 text-success-700 dark:text-green-300',
  warning:
    'border-warning-500/40 bg-warning-50 dark:bg-amber-900/20 text-warning-700 dark:text-amber-300',
  danger: 'border-danger-500/40 bg-danger-50 dark:bg-red-900/20 text-danger-700 dark:text-red-300',
};

const TONE_CHIP: Record<Tone, string> = {
  neutral: 'bg-gray-100 text-gray-700 dark:bg-gray-700 dark:text-gray-200',
  info: 'bg-blue-100 text-blue-800 dark:bg-blue-900/40 dark:text-blue-300',
  success: 'bg-green-100 text-green-800 dark:bg-green-900/40 dark:text-green-300',
  warning: 'bg-amber-100 text-amber-800 dark:bg-amber-900/40 dark:text-amber-300',
  danger: 'bg-red-100 text-red-800 dark:bg-red-900/40 dark:text-red-300',
};

function toneOf(v: unknown): Tone {
  return v === 'info' || v === 'success' || v === 'warning' || v === 'danger' ? v : 'neutral';
}

// ---------------------------------------------------------------------------
// Shared pieces
// ---------------------------------------------------------------------------

export function Note({ tone = 'neutral', children }: { tone?: Tone; children: ReactNode }) {
  return (
    <div
      role="note"
      className={clsx('flex items-start gap-2 rounded-md border p-3 text-sm', TONE_NOTE[tone])}
    >
      <Info aria-hidden="true" className="w-4 h-4 mt-0.5 flex-shrink-0" />
      <div>{children}</div>
    </div>
  );
}

export function Unsupported({ layout }: { layout?: string }) {
  return (
    <Note>
      {layout ? t('plugin_ui.unsupported_named', { layout }) : t('plugin_ui.unsupported')}
    </Note>
  );
}

/** Title and description of a block; the title labels the block's region. */
export function BlockHeader({ spec, id }: { spec: Json; id?: string }) {
  const title = str(spec.title);
  const description = str(spec.description);
  if (!title && !description) return null;
  return (
    <div className="mb-2">
      {title && (
        <h3 id={id} className="text-base font-semibold text-gray-900 dark:text-white">
          {title}
        </h3>
      )}
      {description && <p className="text-sm text-gray-500 dark:text-gray-400">{description}</p>}
    </div>
  );
}

function Section({ spec, children }: { spec: Json; children: ReactNode }) {
  const id = useId();
  const labelled = !!str(spec.title);
  return (
    <section aria-labelledby={labelled ? id : undefined} className="space-y-2">
      <BlockHeader spec={spec} id={id} />
      {children}
    </section>
  );
}

/** One value, as text; `status` values are chips that always carry text. */
export function ValueView({ value, display }: { value: unknown; display: Json }) {
  const text = displayText(value, display);
  if (display.format === 'status' && !isEmptyValue(value)) {
    return (
      <span
        className={clsx(
          'inline-flex rounded-full px-2 py-0.5 text-xs font-medium',
          TONE_CHIP[statusTone(value, display)]
        )}
      >
        {text}
      </span>
    );
  }
  return <>{text}</>;
}

function valueOf(item: Json, data: unknown): unknown {
  return item.key !== undefined ? getPath(data, item.key) : item.value;
}

function StatusLine({ status }: { status: { ok: boolean; text: string } | null }) {
  return (
    <div aria-live="polite">
      {status && (
        <p
          role={status.ok ? 'status' : 'alert'}
          className={clsx('text-sm', status.ok ? 'text-success-700' : 'text-danger-700')}
        >
          {status.text}
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Data sources
// ---------------------------------------------------------------------------

type SourceResult =
  | { state: 'unbound' | 'unavailable' | 'forbidden' | 'missing' | 'loading' }
  | { state: 'error'; error: string }
  | { state: 'ready'; data: unknown; root: unknown };

/**
 * GET data from the plugin's own route: only a route the server listed as
 * allowed for this user, filled from the slot context.
 */
function useSource(source: unknown, extraQuery?: Record<string, string>): SourceResult {
  const env = useDeclarative();
  const src = isObject(source) ? source : undefined;
  const route = str(src?.route) ?? '';
  const access = findRoute(env?.routes, 'GET', route);
  let path: string | null = null;
  try {
    path = src ? fillRoute(route, src.params, undefined, env?.params ?? {}) : null;
  } catch {
    path = null;
  }
  const query = { ...staticQuery(src?.query), ...(extraQuery ?? {}) };
  const enabled = !!env && !!access?.allowed && path !== null;
  const q = useQuery({
    queryKey: [ROUTE_QUERY, env?.pluginId, path, query],
    queryFn: () => pluginsApi.callPluginRoute(env?.pluginId ?? '', 'GET', path ?? '', { query }),
    enabled,
    retry: false,
    staleTime: SOURCE_STALE_MS,
    refetchOnWindowFocus: false,
  });
  if (!src) return { state: 'unbound' };
  if (!env) return { state: 'unavailable' };
  if (!access?.allowed) return { state: 'forbidden' };
  if (path === null) return { state: 'missing' };
  if (q.isPending) return { state: 'loading' };
  if (q.isError) return { state: 'error', error: routeErrorMessage(q.error) };
  const root = q.data;
  return { state: 'ready', root, data: src.path !== undefined ? getPath(root, src.path) : root };
}

/** Loading, error and access states of a bound block; null when ready. */
function SourceStatus({ result }: { result: SourceResult }) {
  switch (result.state) {
    case 'unavailable':
      return <Unsupported />;
    case 'forbidden':
      return <Note>{t('plugin_ui.no_access')}</Note>;
    case 'missing':
      return <Note>{t('plugin_ui.missing_param')}</Note>;
    case 'loading':
      return (
        <p role="status" className="flex items-center gap-2 text-sm text-gray-500">
          <Loader2 aria-hidden="true" className="w-4 h-4 animate-spin" />
          {t('plugin_ui.loading')}
        </p>
      );
    case 'error':
      return (
        <p role="alert" className="flex items-start gap-2 text-sm text-danger-700">
          <CircleAlert aria-hidden="true" className="w-4 h-4 mt-0.5 flex-shrink-0" />
          {t('plugin_ui.load_failed', { reason: result.error })}
        </p>
      );
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Calls that change state
// ---------------------------------------------------------------------------

/** Calls a mutating plugin route and refreshes the plugin's data afterwards. */
function useRouteCall() {
  const env = useDeclarative();
  const queryClient = useQueryClient();
  return async (spec: Json, row: unknown, body: Json | undefined): Promise<unknown> => {
    const method = str(spec.method) ?? '';
    if (!env || !findRoute(env.routes, method, spec.route)?.allowed) {
      throw Object.assign(new Error('forbidden'), { response: { status: 403 } });
    }
    const path = fillRoute(str(spec.route) ?? '', spec.params, row, env.params);
    const result = await pluginsApi.callPluginRoute(
      env.pluginId,
      method,
      path,
      method === 'DELETE' ? {} : { body: body ?? {} }
    );
    await queryClient.invalidateQueries({ queryKey: [ROUTE_QUERY, env.pluginId] });
    return result;
  };
}

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

export function FieldInput({
  field,
  value,
  error,
  idPrefix,
  emptyLabel = '',
  onChange,
}: {
  field: Json;
  value: string | boolean;
  error?: string;
  idPrefix: string;
  emptyLabel?: string;
  onChange: (v: string | boolean) => void;
}) {
  const name = str(field.name) ?? '';
  const label = str(field.label) ?? name;
  const type = fieldType(field);
  const id = `${idPrefix}-${name}`;
  const help = str(field.help);
  const helpId = `${id}-help`;
  const errorId = `${id}-error`;
  const describedBy = [help ? helpId : '', error ? errorId : ''].filter(Boolean).join(' ');
  const common = {
    id,
    name,
    'aria-describedby': describedBy || undefined,
    'aria-invalid': error ? true : undefined,
    'aria-required': field.required === true ? true : undefined,
  };
  const text = typeof value === 'string' ? value : '';
  let control: ReactNode;
  if (type === 'checkbox') {
    return (
      <div>
        <div className="flex items-center gap-2">
          <input
            {...common}
            type="checkbox"
            checked={value === true}
            onChange={(e) => onChange(e.target.checked)}
          />
          <label htmlFor={id} className="text-sm text-gray-700 dark:text-gray-300">
            {label}
          </label>
        </div>
        {help && (
          <p id={helpId} className="text-xs text-gray-500">
            {help}
          </p>
        )}
      </div>
    );
  }
  if (type === 'select') {
    control = (
      <select
        {...common}
        className={inputClass}
        value={text}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">{emptyLabel}</option>
        {asArray(field.options).map((o, i) => (
          <option key={i} value={String(o.value)}>
            {str(o.label) ?? String(o.value)}
          </option>
        ))}
      </select>
    );
  } else if (type === 'textarea' || type === 'json') {
    control = (
      <textarea
        {...common}
        className={clsx(inputClass, type === 'json' && 'font-mono')}
        rows={type === 'json' ? 8 : 3}
        spellCheck={type !== 'json'}
        placeholder={str(field.placeholder)}
        value={text}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  } else {
    control = (
      <input
        {...common}
        type={type === 'number' ? 'number' : 'text'}
        inputMode={type === 'number' ? 'decimal' : undefined}
        className={inputClass}
        placeholder={str(field.placeholder)}
        value={text}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }
  return (
    <div className="space-y-1">
      <label htmlFor={id} className="block text-sm font-medium text-gray-700 dark:text-gray-300">
        {label}
        {field.required === true && (
          <span aria-hidden="true" className="text-danger-700">
            {' '}
            *
          </span>
        )}
      </label>
      {control}
      {help && (
        <p id={helpId} className="text-xs text-gray-500">
          {help}
        </p>
      )}
      {error && (
        <p id={errorId} className="text-xs text-danger-700">
          {error}
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Dialog for actions and confirmations
// ---------------------------------------------------------------------------

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

function ActionDialog({
  title,
  message,
  fields,
  row,
  submitLabel,
  danger,
  stepUp,
  busy,
  error,
  onSubmit,
  onCancel,
}: {
  title: string;
  message?: string;
  fields: Json[];
  row?: unknown;
  submitLabel: string;
  danger: boolean;
  stepUp: boolean;
  busy: boolean;
  error: string | null;
  onSubmit: (body: Json) => void;
  onCancel: () => void;
}) {
  const titleId = useId();
  const messageId = useId();
  const prefix = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const [values, setValues] = useState<FieldValues>(() => initialValues(fields, row));
  const [errors, setErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    const back = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const first = dialogRef.current?.querySelector<HTMLElement>(
      'input:not([disabled]), select:not([disabled]), textarea:not([disabled])'
    );
    (first ?? dialogRef.current)?.focus();
    return () => back?.focus?.();
  }, []);

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      if (!busy) onCancel();
      return;
    }
    if (e.key !== 'Tab' || !dialogRef.current) return;
    const items = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (
      e.shiftKey &&
      (document.activeElement === first || document.activeElement === dialogRef.current)
    ) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const built = buildBody(fields, values);
    setErrors(built.errors);
    if (Object.keys(built.errors).length === 0) onSubmit(built.body);
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={message ? messageId : undefined}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="bg-white dark:bg-gray-800 rounded-xl shadow-xl w-full max-w-md p-6 focus:outline-none"
      >
        <form onSubmit={submit} noValidate className="space-y-4">
          <h2 id={titleId} className="text-lg font-semibold text-gray-900 dark:text-white">
            {title}
          </h2>
          {message && (
            <p id={messageId} className="text-sm text-gray-600 dark:text-gray-300">
              {message}
            </p>
          )}
          {fields.map((f) => {
            const name = str(f.name) ?? '';
            return (
              <FieldInput
                key={name}
                field={f}
                idPrefix={prefix}
                value={values[name] ?? ''}
                error={errors[name]}
                onChange={(v) => setValues((cur) => ({ ...cur, [name]: v }))}
              />
            );
          })}
          {stepUp && <p className="text-xs text-gray-500">{t('plugin_ui.step_up_hint')}</p>}
          {error && (
            <p role="alert" className="text-sm text-danger-700">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <button type="button" className={secondaryBtn} onClick={onCancel} disabled={busy}>
              {t('plugin_ui.cancel')}
            </button>
            <button type="submit" className={danger ? dangerBtn : primaryBtn} disabled={busy}>
              {busy ? t('plugin_ui.working') : submitLabel}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

/**
 * One action (row, block or detail): runs at once when it declares no
 * fields and no confirmation, otherwise opens a dialog. Hidden when the
 * user may not call its route.
 */
function ActionButton({
  action,
  row,
  onStatus,
}: {
  action: Json;
  row?: unknown;
  onStatus: (s: { ok: boolean; text: string }) => void;
}) {
  const env = useDeclarative();
  const call = useRouteCall();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const access = findRoute(env?.routes, str(action.method) ?? '', action.route);
  if (!access?.allowed || !matchesWhen(action.when, row)) return null;

  const fields = asArray(action.fields);
  const confirm = isObject(action.confirm) ? action.confirm : undefined;
  const danger = action.danger === true || action.method === 'DELETE';
  const label = str(action.label) ?? '';
  const success = str(action.success) ?? t('plugin_ui.done');

  const run = async (body: Json | undefined) => {
    setBusy(true);
    setError(null);
    try {
      await call(action, row, body);
      setOpen(false);
      onStatus({ ok: true, text: success });
    } catch (err) {
      const text = routeErrorMessage(err);
      if (open) setError(text);
      else onStatus({ ok: false, text });
    } finally {
      setBusy(false);
    }
  };

  const start = () => {
    // Destructive calls always get a confirmation step, even if the spec
    // (validated at install) somehow had none.
    if (fields.length === 0 && !confirm && !danger) {
      void run(undefined);
    } else {
      setError(null);
      setOpen(true);
    }
  };

  return (
    <>
      <button
        type="button"
        onClick={start}
        disabled={busy}
        className={clsx(
          'text-sm font-medium hover:underline disabled:opacity-50',
          danger ? 'text-danger-700' : 'text-primary-600'
        )}
      >
        {label}
      </button>
      {open && (
        <ActionDialog
          title={str(confirm?.title) ?? label}
          message={str(confirm?.message)}
          fields={fields}
          row={row}
          submitLabel={str(confirm?.confirm_label) ?? label}
          danger={danger}
          stepUp={access.step_up}
          busy={busy}
          error={error}
          onSubmit={(body) => void run(body)}
          onCancel={() => setOpen(false)}
        />
      )}
    </>
  );
}

function ActionBar({
  actions,
  row,
  onStatus,
}: {
  actions: Json[];
  row?: unknown;
  onStatus: (s: { ok: boolean; text: string }) => void;
}) {
  if (actions.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-3">
      {actions.map((a, i) => (
        <ActionButton key={str(a.id) ?? i} action={a} row={row} onStatus={onStatus} />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

function NoticeView({ spec }: { spec: Json }) {
  const title = str(spec.title);
  return (
    <Note tone={toneOf(spec.tone)}>
      {title && <strong className="block">{title}</strong>}
      {str(spec.text)}
    </Note>
  );
}

function BoundNotice({ spec }: { spec: Json }) {
  const result = useSource(spec.source);
  // A bound notice shows only once its data says so.
  if (result.state !== 'ready' || !matchesWhen(spec.when, result.data)) return null;
  return <NoticeView spec={spec} />;
}

/**
 * How a block renders: from its own values, from a plugin route, or not at
 * all when it is bound but no plugin context was given.
 */
function useBinding(spec: Json): 'static' | 'bound' | 'unsupported' {
  const env = useDeclarative();
  if (!isObject(spec.source)) return 'static';
  return env ? 'bound' : 'unsupported';
}

export function NoticeBlock({ spec }: { spec: Json }) {
  const binding = useBinding(spec);
  if (binding === 'unsupported') return <Unsupported />;
  return binding === 'bound' ? <BoundNotice spec={spec} /> : <NoticeView spec={spec} />;
}

function BoundStats({ spec }: { spec: Json }) {
  const result = useSource(spec.source);
  if (result.state !== 'ready') {
    return (
      <Section spec={spec}>
        <SourceStatus result={result} />
      </Section>
    );
  }
  return <StatsView spec={spec} data={result.data} />;
}

export function StatsBlock({ spec }: { spec: Json }) {
  const binding = useBinding(spec);
  if (binding === 'unsupported') return <Unsupported />;
  return binding === 'bound' ? (
    <BoundStats spec={spec} />
  ) : (
    <StatsView spec={spec} data={undefined} />
  );
}

function StatsView({ spec, data }: { spec: Json; data: unknown }) {
  const items = asArray(spec.items);
  return (
    <Section spec={spec}>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        {items.map((item, i) => (
          <div key={i} className="rounded-lg border border-gray-200 dark:border-gray-700 p-4">
            <p className="text-sm text-gray-500 dark:text-gray-400">{str(item.label)}</p>
            <p className="text-2xl font-semibold text-gray-900 dark:text-white">
              <ValueView value={valueOf(item, data)} display={item} />
            </p>
            {str(item.hint) && (
              <p className="text-xs text-gray-400 dark:text-gray-500">{str(item.hint)}</p>
            )}
          </div>
        ))}
      </div>
    </Section>
  );
}

function DetailList({ fields, data }: { fields: Json[]; data: unknown }) {
  return (
    <dl className="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-3">
      {fields.map((field, i) => (
        <div key={i}>
          <dt className="text-xs uppercase text-gray-500 dark:text-gray-400">{str(field.label)}</dt>
          <dd className="text-sm text-gray-900 dark:text-gray-100 break-words">
            <ValueView value={valueOf(field, data)} display={field} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

function DetailView({ spec, data, row }: { spec: Json; data: unknown; row?: unknown }) {
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  return (
    <Section spec={spec}>
      <DetailList fields={asArray(spec.fields)} data={data} />
      <ActionBar actions={asArray(spec.actions)} row={row} onStatus={setStatus} />
      <StatusLine status={status} />
    </Section>
  );
}

function BoundDetail({ spec }: { spec: Json }) {
  const result = useSource(spec.source);
  if (result.state !== 'ready') {
    return (
      <Section spec={spec}>
        <SourceStatus result={result} />
      </Section>
    );
  }
  // A bound detail shows one record, which its actions act on.
  return <DetailView spec={spec} data={result.data} row={result.data} />;
}

export function DetailBlock({ spec }: { spec: Json }) {
  const binding = useBinding(spec);
  if (binding === 'unsupported') return <Unsupported />;
  return binding === 'bound' ? (
    <BoundDetail spec={spec} />
  ) : (
    <DetailView spec={spec} data={undefined} />
  );
}

function TableView({
  spec,
  rows,
  rowActions,
  onStatus,
}: {
  spec: Json;
  rows: Json[];
  rowActions: Json[];
  onStatus: (s: { ok: boolean; text: string }) => void;
}) {
  const columns = asArray(spec.columns);
  const title = str(spec.title);
  const withActions = rowActions.length > 0;
  return (
    <div className="overflow-x-auto">
      <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
        {title && <caption className="sr-only">{title}</caption>}
        <thead className="bg-gray-50 dark:bg-gray-800">
          <tr>
            {columns.map((col, i) => (
              <th
                key={i}
                scope="col"
                className="px-4 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase"
              >
                {str(col.label) ?? str(col.key)}
              </th>
            ))}
            {withActions && (
              <th
                scope="col"
                className="px-4 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase"
              >
                {t('plugin_ui.actions')}
              </th>
            )}
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
          {rows.length === 0 ? (
            <tr>
              <td
                colSpan={Math.max(columns.length + (withActions ? 1 : 0), 1)}
                className="px-4 py-3 text-sm text-gray-500 dark:text-gray-400"
              >
                {str(spec.empty) ?? t('plugin_ui.no_rows')}
              </td>
            </tr>
          ) : (
            rows.map((row, r) => (
              <tr key={r}>
                {columns.map((col, c) => (
                  <td key={c} className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300">
                    <ValueView value={getPath(row, col.key)} display={col} />
                  </td>
                ))}
                {withActions && (
                  <td className="px-4 py-2 text-sm">
                    <ActionBar actions={rowActions} row={row} onStatus={onStatus} />
                  </td>
                )}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

function StaticTable({ spec }: { spec: Json }) {
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  return (
    <Section spec={spec}>
      <ActionBar actions={asArray(spec.actions)} onStatus={setStatus} />
      <TableView spec={spec} rows={asArray(spec.rows)} rowActions={[]} onStatus={setStatus} />
      <StatusLine status={status} />
    </Section>
  );
}

function BoundTable({ spec }: { spec: Json }) {
  const env = useDeclarative();
  const filters = asArray(spec.filters);
  const paging = isObject(spec.paging) ? spec.paging : undefined;
  const cursorParam = str(paging?.cursor_param);
  const filterPrefix = useId();
  const [draft, setDraft] = useState<FieldValues>(() => initialValues(filters));
  const [applied, setApplied] = useState<Record<string, string>>(() =>
    filterQuery(initialValues(filters))
  );
  const [cursors, setCursors] = useState<string[]>([]);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const cursor = cursors.length > 0 ? cursors[cursors.length - 1] : undefined;
  const result = useSource(spec.source, {
    ...applied,
    ...(cursorParam && cursor ? { [cursorParam]: cursor } : {}),
  });
  const rowActions = asArray(spec.row_actions).filter(
    (a) => findRoute(env?.routes, str(a.method) ?? '', a.route)?.allowed
  );
  const rows =
    result.state === 'ready' && Array.isArray(result.data) ? result.data.filter(isObject) : [];
  const next =
    paging && result.state === 'ready' ? str(getPath(result.root, paging.next_key)) : undefined;

  const applyFilters = (e: FormEvent) => {
    e.preventDefault();
    setApplied(filterQuery(draft));
    setCursors([]);
  };
  const clearFilters = () => {
    const empty = initialValues(filters);
    setDraft(empty);
    setApplied(filterQuery(empty));
    setCursors([]);
  };

  return (
    <Section spec={spec}>
      {filters.length > 0 && (
        <form
          onSubmit={applyFilters}
          aria-label={t('plugin_ui.filters')}
          className="flex flex-wrap items-end gap-3"
        >
          {filters.map((f) => {
            const name = str(f.name) ?? '';
            return (
              <div key={name} className="min-w-[10rem]">
                <FieldInput
                  field={{ ...f, required: false }}
                  idPrefix={filterPrefix}
                  value={draft[name] ?? ''}
                  emptyLabel={t('plugin_ui.any')}
                  onChange={(v) => setDraft((cur) => ({ ...cur, [name]: v }))}
                />
              </div>
            );
          })}
          <button type="submit" className={primaryBtn}>
            {t('plugin_ui.apply_filters')}
          </button>
          <button type="button" className={secondaryBtn} onClick={clearFilters}>
            {t('plugin_ui.clear_filters')}
          </button>
        </form>
      )}
      <ActionBar actions={asArray(spec.actions)} onStatus={setStatus} />
      {result.state === 'ready' ? (
        <TableView spec={spec} rows={rows} rowActions={rowActions} onStatus={setStatus} />
      ) : (
        <SourceStatus result={result} />
      )}
      {paging && result.state === 'ready' && (cursors.length > 0 || next) && (
        <div className="flex gap-2">
          <button
            type="button"
            className={secondaryBtn}
            disabled={cursors.length === 0}
            onClick={() => setCursors((c) => c.slice(0, -1))}
          >
            {t('plugin_ui.previous_page')}
          </button>
          <button
            type="button"
            className={secondaryBtn}
            disabled={!next}
            onClick={() => next && setCursors((c) => [...c, next])}
          >
            {t('plugin_ui.next_page')}
          </button>
        </div>
      )}
      <StatusLine status={status} />
    </Section>
  );
}

export function TableBlock({ spec }: { spec: Json }) {
  const binding = useBinding(spec);
  if (binding === 'unsupported') return <Unsupported />;
  return binding === 'bound' ? <BoundTable spec={spec} /> : <StaticTable spec={spec} />;
}

export function FormBlock({ spec }: { spec: Json }) {
  return useDeclarative() ? <BoundForm spec={spec} /> : <Unsupported />;
}

function BoundForm({ spec }: { spec: Json }) {
  const env = useDeclarative();
  const call = useRouteCall();
  const prefix = useId();
  const fields = asArray(spec.fields);
  const submit = isObject(spec.submit) ? spec.submit : {};
  const [values, setValues] = useState<FieldValues>(() => initialValues(fields));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<Json | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [result, setResult] = useState<unknown>(undefined);
  const [dialogError, setDialogError] = useState<string | null>(null);

  const access = findRoute(env?.routes, str(submit.method) ?? '', submit.route);
  if (!access?.allowed) return null;
  const confirm = isObject(submit.confirm) ? submit.confirm : undefined;
  const danger = submit.danger === true || submit.method === 'DELETE';
  const label = str(submit.label) ?? '';
  const resultFields = isObject(spec.result) ? asArray(spec.result.fields) : [];

  const send = async (body: Json, fromDialog: boolean) => {
    setBusy(true);
    setStatus(null);
    setDialogError(null);
    try {
      const response = await call(submit, undefined, body);
      setPending(null);
      setResult(response);
      setStatus({ ok: true, text: str(submit.success) ?? t('plugin_ui.done') });
    } catch (err) {
      const text = routeErrorMessage(err);
      if (fromDialog) setDialogError(text);
      else setStatus({ ok: false, text });
    } finally {
      setBusy(false);
    }
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const built = buildBody(fields, values);
    setErrors(built.errors);
    if (Object.keys(built.errors).length > 0) return;
    if (confirm || danger) setPending(built.body);
    else void send(built.body, false);
  };

  return (
    <Section spec={spec}>
      <form onSubmit={onSubmit} noValidate className="space-y-3">
        {fields.map((f) => {
          const name = str(f.name) ?? '';
          return (
            <FieldInput
              key={name}
              field={f}
              idPrefix={prefix}
              value={values[name] ?? ''}
              error={errors[name]}
              onChange={(v) => setValues((cur) => ({ ...cur, [name]: v }))}
            />
          );
        })}
        {access.step_up && <p className="text-xs text-gray-500">{t('plugin_ui.step_up_hint')}</p>}
        <button type="submit" className={danger ? dangerBtn : primaryBtn} disabled={busy}>
          {busy ? t('plugin_ui.working') : label}
        </button>
      </form>
      <StatusLine status={status} />
      {result !== undefined && resultFields.length > 0 && (
        <section aria-label={t('plugin_ui.result')} className="pt-2">
          <DetailList fields={resultFields} data={result} />
        </section>
      )}
      {pending && (
        <ActionDialog
          title={str(confirm?.title) ?? label}
          message={str(confirm?.message)}
          fields={[]}
          submitLabel={str(confirm?.confirm_label) ?? t('plugin_ui.confirm')}
          danger={danger}
          stepUp={access.step_up}
          busy={busy}
          error={dialogError}
          onSubmit={() => void send(pending, true)}
          onCancel={() => setPending(null)}
        />
      )}
    </Section>
  );
}
