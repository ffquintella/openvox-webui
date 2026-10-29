import type {
  ClassificationExpression,
  GroupBehavior,
  GroupPreviewRequest,
  NodeGroup,
  RuleMatchType,
} from '../../types';
import { DEFAULT_PRIORITY, behaviorOf, stableStringify } from './expressionHelpers';

/**
 * The part of the group form that changes who is a member or what the node receives.
 * Only these fields are covered by the mandatory impact preview.
 */
export interface GroupGateDraft {
  expression: ClassificationExpression | null;
  behavior: GroupBehavior;
  priority: number;
  ruleMatchType: RuleMatchType;
  matchAllNodes: boolean;
  environment: string;
  parentId: string;
}

export function draftFromGroup(group: NodeGroup): GroupGateDraft {
  return {
    expression:
      group.expression && Object.keys(group.expression).length > 0 ? group.expression : null,
    behavior: behaviorOf(group.behavior),
    priority: group.priority ?? DEFAULT_PRIORITY,
    ruleMatchType: group.rule_match_type,
    matchAllNodes: group.match_all_nodes ?? false,
    environment: group.environment ?? '',
    parentId: group.parent_id ?? '',
  };
}

/** Stable text form of a draft, used to tell whether a preview still matches the form. */
export function draftSignature(draft: GroupGateDraft): string {
  return stableStringify({
    ...draft,
    behavior: behaviorOf(draft.behavior),
    environment: draft.environment.trim(),
  });
}

export function draftChanged(original: GroupGateDraft, draft: GroupGateDraft): boolean {
  return draftSignature(original) !== draftSignature(draft);
}

/**
 * Request body for the preview endpoint. Fields that equal the saved group are still sent
 * (they are harmless); a cleared expression is sent as `{}`, mirroring the PUT contract.
 */
export function previewRequestFromDraft(
  original: GroupGateDraft,
  draft: GroupGateDraft
): GroupPreviewRequest {
  const request: GroupPreviewRequest = {
    priority: draft.priority,
    behavior: draft.behavior,
    rule_match_type: draft.ruleMatchType,
    match_all_nodes: draft.matchAllNodes,
    environment: draft.environment.trim() === '' ? null : draft.environment.trim(),
    parent_id: draft.parentId === '' ? null : draft.parentId,
  };
  if (draft.expression) {
    request.expression = draft.expression;
  } else if (original.expression) {
    request.expression = {};
  }
  return request;
}

export interface PreviewGateState {
  /** Signature of the draft a preview was last run for */
  previewed: string | null;
  /** Signature of the draft the user confirmed the impact of */
  confirmed: string | null;
}

export const EMPTY_GATE: PreviewGateState = { previewed: null, confirmed: null };

export interface SaveGateInput {
  /** Creating a group: there is no id to preview against */
  isNew: boolean;
  changed: boolean;
  signature: string;
  state: PreviewGateState;
  /** Client-side validation problems (invalid JSON etc.) */
  errors: string[];
}

/** Why Save must stay disabled, or null when saving is allowed. */
export function saveBlockedReason(input: SaveGateInput): string | null {
  if (input.errors.length > 0) return input.errors[0];
  if (input.isNew || !input.changed) return null;
  if (input.state.previewed !== input.signature) {
    return 'Run the impact preview for these changes before saving.';
  }
  if (input.state.confirmed !== input.signature) {
    return 'Confirm the previewed impact before saving.';
  }
  return null;
}
