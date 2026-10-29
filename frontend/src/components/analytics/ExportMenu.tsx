import { useState } from 'react';
import { Download } from 'lucide-react';
import { api } from '../../services/api';
import { useExportFormats } from '../../hooks/useAnalytics';
import type { ExportFormatInfo } from '../../types';
import { getApiErrorMessage, secondaryBtn } from '../plugins/pluginHelpers';
import { CORE_EXPORT_FORMATS, exportFileName, saveBlob } from './pluginReportHelpers';

/** Lists the core and plugin export formats and downloads an execution in the chosen one. */
export default function ExportMenu({ executionId }: { executionId: string }) {
  const { data } = useExportFormats();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const formats = data && data.length > 0 ? data : CORE_EXPORT_FORMATS;

  const download = async (format: ExportFormatInfo) => {
    setBusy(format.id);
    setError(null);
    try {
      const blob = await api.exportExecution(executionId, format.id);
      saveBlob(blob, exportFileName(executionId, format));
      setOpen(false);
    } catch (e) {
      setError(getApiErrorMessage(e, 'Export failed'));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="relative inline-block text-left">
      <button
        type="button"
        className={`${secondaryBtn} flex items-center gap-2`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <Download className="w-4 h-4" />
        Export
      </button>
      {open && (
        <ul
          role="menu"
          aria-label="Export formats"
          className="absolute right-0 z-20 mt-1 w-56 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1"
        >
          {formats.map((f) => (
            <li key={f.id} role="none">
              <button
                type="button"
                role="menuitem"
                disabled={busy !== null}
                onClick={() => download(f)}
                className="w-full text-left px-3 py-2 text-sm hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50 flex items-center justify-between gap-2"
              >
                <span>{f.label}</span>
                {f.source !== 'core' && (
                  <span className="text-xs text-gray-500 dark:text-gray-400">plugin</span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
      {error && (
        <p role="alert" className="text-xs text-red-600 mt-1">
          {error}
        </p>
      )}
    </div>
  );
}
