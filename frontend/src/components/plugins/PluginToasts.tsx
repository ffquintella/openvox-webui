import { useEffect } from 'react';
import { X } from 'lucide-react';
import clsx from 'clsx';
import { usePluginToastStore, type PluginToast } from '../../stores/pluginToastStore';

const TOAST_MS = 6000;

const LEVEL_STYLES: Record<PluginToast['level'], string> = {
  info: 'border-blue-300 bg-blue-50 text-blue-900',
  success: 'border-green-300 bg-green-50 text-green-900',
  warning: 'border-yellow-300 bg-yellow-50 text-yellow-900',
  error: 'border-red-300 bg-red-50 text-red-900',
};

function Toast({ toast }: { toast: PluginToast }) {
  const dismiss = usePluginToastStore((s) => s.dismiss);
  useEffect(() => {
    const t = setTimeout(() => dismiss(toast.id), TOAST_MS);
    return () => clearTimeout(t);
  }, [toast.id, dismiss]);
  return (
    <div
      role="status"
      className={clsx('rounded-lg border px-4 py-3 shadow-lg text-sm', LEVEL_STYLES[toast.level])}
    >
      <div className="flex items-start gap-3">
        <div className="flex-1">
          <p className="text-xs opacity-70">{toast.source}</p>
          <p>{toast.text}</p>
        </div>
        <button type="button" aria-label="Dismiss" onClick={() => dismiss(toast.id)}>
          <X className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}

/** Notifications requested by plugin pages; the text is plain, never HTML. */
export default function PluginToasts() {
  const toasts = usePluginToastStore((s) => s.toasts);
  if (toasts.length === 0) return null;
  return (
    <div className="fixed bottom-4 right-4 z-50 w-80 space-y-2" data-testid="plugin-toasts">
      {toasts.map((t) => (
        <Toast key={t.id} toast={t} />
      ))}
    </div>
  );
}
