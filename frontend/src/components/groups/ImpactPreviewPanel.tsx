import { useState } from 'react';
import { AlertTriangle, CheckCircle, Loader2, Eye } from 'lucide-react';
import clsx from 'clsx';
import { usePreviewGroupChange } from '../../hooks/useClassification';
import { previewRequestFromDraft, type GroupGateDraft, type PreviewGateState } from './previewGate';
import type { GroupPreviewResult } from '../../types';

interface Props {
  groupId: string;
  original: GroupGateDraft;
  draft: GroupGateDraft;
  /** Signature of `draft` (see draftSignature) */
  signature: string;
  gate: PreviewGateState;
  /** Whether the current draft differs from the saved group */
  changed: boolean;
  /** Client-side validation problems; the preview cannot run while any exist */
  errors: string[];
  onPreviewed: (signature: string) => void;
  onConfirmChange: (confirmed: boolean) => void;
}

function errorMessage(error: unknown): string {
  const err = error as {
    response?: { data?: { error?: string; message?: string } };
    message?: string;
  };
  return (
    err?.response?.data?.message ?? err?.response?.data?.error ?? err?.message ?? 'Preview failed'
  );
}

function Stat({ label, value, tone }: { label: string; value: number; tone?: string }) {
  return (
    <div className="rounded border border-gray-200 bg-white p-2 text-center">
      <p className={clsx('text-lg font-semibold', tone ?? 'text-gray-900')}>{value}</p>
      <p className="text-xs text-gray-500">{label}</p>
    </div>
  );
}

function NodeList({ title, nodes, tone }: { title: string; nodes: string[]; tone: string }) {
  if (nodes.length === 0) return null;
  return (
    <div>
      <p className={clsx('text-xs font-medium mb-1', tone)}>
        {title} ({nodes.length})
      </p>
      <ul className="max-h-28 overflow-y-auto text-xs text-gray-700 font-mono space-y-0.5">
        {nodes.map((n) => (
          <li key={n}>{n}</li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Dry-run of the pending group change. Saving is gated on a preview that was run (and
 * confirmed) for the exact current form values; see usePreviewGate.
 */
export default function ImpactPreviewPanel({
  groupId,
  original,
  draft,
  signature,
  gate,
  changed,
  errors,
  onPreviewed,
  onConfirmChange,
}: Props) {
  const preview = usePreviewGroupChange();
  // The result is shown together with the signature it was computed for.
  const [result, setResult] = useState<{ signature: string; data: GroupPreviewResult } | null>(
    null
  );

  const runPreview = () => {
    const requestSignature = signature;
    preview.mutate(
      { id: groupId, request: previewRequestFromDraft(original, draft) },
      {
        onSuccess: (data) => {
          setResult({ signature: requestSignature, data });
          onPreviewed(requestSignature);
        },
      }
    );
  };

  const current = result && result.signature === signature ? result.data : null;
  const stale = result !== null && result.signature !== signature;
  const confirmed = gate.confirmed === signature && gate.previewed === signature;

  if (!changed && !result) {
    return (
      <div className="rounded border border-gray-200 bg-gray-50 p-3 text-sm text-gray-600">
        Impact preview: change the expression, behavior, priority or matching settings to preview
        their effect on nodes before saving.
      </div>
    );
  }

  return (
    <div
      className="rounded border border-amber-200 bg-amber-50 p-3 space-y-3"
      data-testid="impact-preview"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-gray-900">
          Impact preview (required before saving)
        </h3>
        <button
          type="button"
          className="btn btn-secondary flex items-center"
          onClick={runPreview}
          disabled={preview.isPending || errors.length > 0 || !changed}
        >
          {preview.isPending ? (
            <Loader2 className="w-4 h-4 mr-2 animate-spin" />
          ) : (
            <Eye className="w-4 h-4 mr-2" />
          )}
          {current ? 'Re-run preview' : 'Run preview'}
        </button>
      </div>

      {preview.isError && (
        <p className="text-sm text-red-700" role="alert">
          {errorMessage(preview.error)}
        </p>
      )}

      {stale && (
        <p className="text-sm text-amber-800 flex items-center gap-2">
          <AlertTriangle className="w-4 h-4" />
          The form changed after the last preview. Run the preview again.
        </p>
      )}

      {current && (
        <div className="space-y-3">
          <div className="grid grid-cols-2 sm:grid-cols-5 gap-2">
            <Stat label="Members before" value={current.members_before} />
            <Stat label="Members after" value={current.members_after} />
            <Stat label="Entering" value={current.entering_count} tone="text-green-700" />
            <Stat label="Leaving" value={current.leaving_count} tone="text-red-700" />
            <Stat label="Classification changed" value={current.classification_changed_count} />
          </div>
          <p className="text-xs text-gray-500">
            {current.nodes_evaluated} nodes evaluated
            {current.nodes_skipped > 0 && `, ${current.nodes_skipped} skipped`}.
          </p>
          {current.truncated && (
            <p className="text-xs text-amber-800">
              Lists are truncated; the counts above are complete.
            </p>
          )}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <NodeList title="Entering" nodes={current.entering} tone="text-green-700" />
            <NodeList title="Leaving" nodes={current.leaving} tone="text-red-700" />
          </div>
          {current.classification_changed.length > 0 && (
            <div>
              <p className="text-xs font-medium text-gray-700 mb-1">
                Nodes whose classification changes ({current.classification_changed_count})
              </p>
              <ul className="max-h-40 overflow-y-auto text-xs space-y-1">
                {current.classification_changed.map((c) => {
                  const what = [
                    c.classes_changed && 'classes',
                    c.variables_changed && 'variables',
                    c.environment_changed && 'environment',
                  ].filter(Boolean);
                  return (
                    <li key={c.certname} className="font-mono text-gray-700">
                      {c.certname}
                      {c.groups_added.length > 0 && (
                        <span className="text-green-700"> +{c.groups_added.join(', +')}</span>
                      )}
                      {c.groups_removed.length > 0 && (
                        <span className="text-red-700"> -{c.groups_removed.join(', -')}</span>
                      )}
                      {what.length > 0 && (
                        <span className="text-gray-500"> ({what.join(', ')} changed)</span>
                      )}
                    </li>
                  );
                })}
              </ul>
            </div>
          )}
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => onConfirmChange(e.target.checked)}
            />
            <span className="font-medium text-gray-800">
              I reviewed this impact and want to save
            </span>
            {confirmed && <CheckCircle className="w-4 h-4 text-green-600" />}
          </label>
        </div>
      )}
    </div>
  );
}
