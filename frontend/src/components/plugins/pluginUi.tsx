import type { ReactNode } from 'react';
import { X } from 'lucide-react';
import clsx from 'clsx';
import type { PluginRiskLevel, PluginState } from '../../types';

export function Modal({
  title,
  onClose,
  children,
  wide,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={clsx(
          'bg-white dark:bg-gray-800 rounded-xl shadow-xl w-full max-h-[90vh] flex flex-col',
          wide ? 'max-w-3xl' : 'max-w-lg'
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
          <h3 className="text-lg font-semibold text-gray-900 dark:text-white">{title}</h3>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
        <div className="overflow-y-auto p-6">{children}</div>
      </div>
    </div>
  );
}

const RISK_STYLES: Record<PluginRiskLevel, string> = {
  low: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300',
  medium: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300',
  high: 'bg-orange-100 text-orange-800 dark:bg-orange-900/30 dark:text-orange-300',
  critical: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300',
};

export function RiskBadge({ risk }: { risk: PluginRiskLevel }) {
  return (
    <span className={clsx('px-2 py-0.5 text-xs rounded-full font-medium', RISK_STYLES[risk])}>
      {risk}
    </span>
  );
}

const STATE_STYLES: Record<PluginState, string> = {
  enabled: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300',
  installed: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300',
  configured: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300',
  disabled: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300',
  error: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300',
  update_pending: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300',
};

export function StateBadge({ state, reason }: { state: PluginState; reason?: string | null }) {
  return (
    <span
      title={reason ?? undefined}
      className={clsx('px-2 py-0.5 text-xs rounded-full font-medium', STATE_STYLES[state])}
    >
      {state.replace('_', ' ')}
    </span>
  );
}

export function ErrorBox({ message }: { message: string }) {
  return (
    <div
      role="alert"
      className="rounded-md border border-red-200 bg-red-50 dark:bg-red-900/20 dark:border-red-800 p-3 text-sm text-red-700 dark:text-red-300"
    >
      {message}
    </div>
  );
}
