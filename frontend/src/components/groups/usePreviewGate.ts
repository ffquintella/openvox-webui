import { useCallback, useMemo, useState } from 'react';
import {
  EMPTY_GATE,
  draftChanged,
  draftSignature,
  saveBlockedReason,
  type GroupGateDraft,
  type PreviewGateState,
} from './previewGate';

interface Options {
  /** True when creating a group (no preview possible, never gated) */
  isNew: boolean;
  /** The saved group's values; null while creating */
  original: GroupGateDraft | null;
  draft: GroupGateDraft;
  /** Client-side validation problems that also block Save */
  errors: string[];
}

/**
 * Tracks whether the impact preview was run (and confirmed) for the *current* draft.
 * Any edit changes the draft signature, which silently invalidates an earlier preview.
 */
export function usePreviewGate({ isNew, original, draft, errors }: Options) {
  const [state, setState] = useState<PreviewGateState>(EMPTY_GATE);
  const signature = useMemo(() => draftSignature(draft), [draft]);
  const changed = useMemo(
    () => (original ? draftChanged(original, draft) : false),
    [original, draft]
  );

  /** `forSignature` is the signature the preview request was built from (it may be stale on return). */
  const markPreviewed = useCallback((forSignature: string) => {
    setState({ previewed: forSignature, confirmed: null });
  }, []);

  const setConfirmed = useCallback((confirmed: boolean) => {
    setState((prev) => ({ ...prev, confirmed: confirmed ? prev.previewed : null }));
  }, []);

  const reset = useCallback(() => setState(EMPTY_GATE), []);

  const blockedReason = saveBlockedReason({ isNew, changed, signature, state, errors });

  return {
    state,
    signature,
    changed,
    /** A preview is mandatory before saving */
    previewRequired: !isNew && changed,
    blockedReason,
    canSave: blockedReason === null,
    markPreviewed,
    setConfirmed,
    reset,
  };
}
