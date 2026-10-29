// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import { useState } from 'react';
import ImpactPreviewPanel from '../components/groups/ImpactPreviewPanel';
import { usePreviewGate } from '../components/groups/usePreviewGate';
import {
  behaviorToForm,
  defaultBehavior,
  formToBehavior,
  parseExpressionText,
  rulesToExpression,
  validateBehaviorForm,
} from '../components/groups/expressionHelpers';
import {
  draftChanged,
  draftSignature,
  previewRequestFromDraft,
  saveBlockedReason,
  type GroupGateDraft,
} from '../components/groups/previewGate';
import ExprTraceTree from '../components/classification/ExprTraceTree';
import ProvenanceTable from '../components/classification/ProvenanceTable';
import ExplainView from '../components/classification/ExplainView';
import { flattenTrace, provenanceRows } from '../components/classification/explainHelpers';
import { api } from '../services/api';
import type { ClassificationExplain, ExprTrace, GroupPreviewResult } from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const baseDraft = (over: Partial<GroupGateDraft> = {}): GroupGateDraft => ({
  expression: null,
  behavior: defaultBehavior(),
  priority: 100,
  ruleMatchType: 'all',
  matchAllNodes: false,
  environment: '',
  parentId: '',
  ...over,
});

describe('rulesToExpression', () => {
  const rules = [
    { fact_path: 'os.family', operator: '=' as const, value: 'RedHat' },
    { fact_path: 'processors.count', operator: '>=' as const, value: 4 },
  ];

  it('wraps the flat rules in all for match type all', () => {
    expect(rulesToExpression(rules, 'all')).toEqual({
      all: [
        { fact: 'os.family', op: '=', value: 'RedHat' },
        { fact: 'processors.count', op: '>=', value: 4 },
      ],
    });
  });

  it('wraps the flat rules in any for match type any', () => {
    const expr = rulesToExpression(rules, 'any');
    expect(Object.keys(expr ?? {})).toEqual(['any']);
  });

  it('returns null when there are no rules', () => {
    expect(rulesToExpression([], 'all')).toBeNull();
  });
});

describe('parseExpressionText', () => {
  it('treats blank text and an empty object as no expression', () => {
    expect(parseExpressionText('  ')).toEqual({ ok: true, expression: null });
    expect(parseExpressionText('{}')).toEqual({ ok: true, expression: null });
  });

  it('parses a valid object', () => {
    expect(parseExpressionText('{"not":{"fact":"a","op":"=","value":1}}')).toEqual({
      ok: true,
      expression: { not: { fact: 'a', op: '=', value: 1 } },
    });
  });

  it('rejects malformed JSON and non-objects', () => {
    const bad = parseExpressionText('{"all": [');
    expect(bad.ok).toBe(false);
    expect(parseExpressionText('[1]')).toMatchObject({ ok: false });
    expect(parseExpressionText('"x"')).toMatchObject({ ok: false });
  });
});

describe('behavior form conversion', () => {
  it('round trips the default behavior', () => {
    expect(formToBehavior(behaviorToForm(defaultBehavior()))).toEqual(defaultBehavior());
  });

  it('builds select inheritance keys and n_of match mode', () => {
    const form = {
      ...behaviorToForm(defaultBehavior()),
      inheritanceMode: 'select' as const,
      inheritanceKeys: 'ntp, motd ,',
      matchMode: 'n_of' as const,
      matchK: '2',
      exclusiveSet: ' tier ',
    };
    const behavior = formToBehavior(form);
    expect(behavior.inheritance).toEqual({ mode: 'select', keys: ['ntp', 'motd'] });
    expect(behavior.match_mode).toEqual({ mode: 'n_of', k: 2 });
    expect(behavior.exclusive_set).toBe('tier');
  });

  it('validates select keys, n_of and the activation window', () => {
    const form = behaviorToForm(defaultBehavior());
    expect(validateBehaviorForm({ ...form, inheritanceMode: 'select' })).toHaveLength(1);
    expect(validateBehaviorForm({ ...form, matchMode: 'n_of', matchK: '0' })).toHaveLength(1);
    expect(
      validateBehaviorForm({
        ...form,
        activeFrom: '2026-10-02T10:00',
        expiresAt: '2026-10-01T10:00',
      })
    ).toHaveLength(1);
    expect(validateBehaviorForm(form)).toEqual([]);
  });
});

describe('preview gating logic', () => {
  const original = baseDraft();

  it('detects changes that need a preview', () => {
    expect(draftChanged(original, baseDraft())).toBe(false);
    expect(draftChanged(original, baseDraft({ priority: 200 }))).toBe(true);
    expect(draftChanged(original, baseDraft({ expression: { all: [] } }))).toBe(true);
  });

  it('is independent of key order in the expression', () => {
    const a = baseDraft({ expression: { fact: 'x', op: '=', value: 1 } });
    const b = baseDraft({ expression: { value: 1, op: '=', fact: 'x' } });
    expect(draftSignature(a)).toBe(draftSignature(b));
  });

  it('blocks save until previewed and confirmed for the same signature', () => {
    const signature = draftSignature(baseDraft({ priority: 5 }));
    const input = { isNew: false, changed: true, signature, errors: [] as string[] };
    expect(saveBlockedReason({ ...input, state: { previewed: null, confirmed: null } })).toMatch(
      /Run the impact preview/
    );
    expect(
      saveBlockedReason({ ...input, state: { previewed: signature, confirmed: null } })
    ).toMatch(/Confirm/);
    expect(
      saveBlockedReason({ ...input, state: { previewed: 'other', confirmed: 'other' } })
    ).toMatch(/Run the impact preview/);
    expect(
      saveBlockedReason({ ...input, state: { previewed: signature, confirmed: signature } })
    ).toBeNull();
  });

  it('never gates new groups or unchanged groups, but validation errors always block', () => {
    const state = { previewed: null, confirmed: null };
    expect(
      saveBlockedReason({ isNew: true, changed: false, signature: 's', state, errors: [] })
    ).toBeNull();
    expect(
      saveBlockedReason({ isNew: false, changed: false, signature: 's', state, errors: [] })
    ).toBeNull();
    expect(
      saveBlockedReason({
        isNew: true,
        changed: false,
        signature: 's',
        state,
        errors: ['Invalid JSON'],
      })
    ).toBe('Invalid JSON');
  });

  it('sends {} to clear an existing expression and omits it when there was none', () => {
    const withExpr = baseDraft({ expression: { all: [] } });
    expect(previewRequestFromDraft(withExpr, baseDraft()).expression).toEqual({});
    expect(previewRequestFromDraft(baseDraft(), baseDraft()).expression).toBeUndefined();
    expect(previewRequestFromDraft(baseDraft(), withExpr).expression).toEqual({ all: [] });
  });
});

const previewResult = (over: Partial<GroupPreviewResult> = {}): GroupPreviewResult => ({
  group_id: 'g1',
  nodes_evaluated: 10,
  nodes_skipped: 1,
  members_before: 3,
  members_after: 5,
  entering_count: 2,
  leaving_count: 0,
  entering: ['web1.example.com', 'web2.example.com'],
  leaving: [],
  truncated: false,
  classification_changed_count: 1,
  classification_changed: [
    {
      certname: 'web1.example.com',
      groups_added: ['webservers'],
      groups_removed: [],
      classes_changed: true,
      variables_changed: false,
      environment_changed: false,
    },
  ],
  ...over,
});

/** Mirrors how Groups.tsx wires the gate: a draft, the preview panel and a Save button. */
function GateHarness() {
  const [priority, setPriority] = useState(100);
  const original = baseDraft();
  const draft = baseDraft({ priority });
  const gate = usePreviewGate({ isNew: false, original, draft, errors: [] });
  return (
    <div>
      <input
        aria-label="priority"
        type="number"
        value={priority}
        onChange={(e) => setPriority(Number(e.target.value))}
      />
      <ImpactPreviewPanel
        groupId="g1"
        original={original}
        draft={draft}
        signature={gate.signature}
        gate={gate.state}
        changed={gate.changed}
        errors={[]}
        onPreviewed={gate.markPreviewed}
        onConfirmChange={gate.setConfirmed}
      />
      <button disabled={!gate.canSave}>Save</button>
    </div>
  );
}

function renderHarness() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <GateHarness />
    </QueryClientProvider>
  );
}

describe('Save gating with the impact preview', () => {
  it('keeps Save enabled while nothing changed', () => {
    renderHarness();
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
  });

  it('disables Save after an edit until a preview ran and was confirmed', async () => {
    const spy = vi.spyOn(api, 'previewGroupChange').mockResolvedValue(previewResult());
    renderHarness();

    fireEvent.change(screen.getByLabelText('priority'), { target: { value: '200' } });
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: /Run preview/ }));
    await waitFor(() => expect(screen.getByText('Members after')).toBeInTheDocument());
    expect(spy).toHaveBeenCalledWith('g1', expect.objectContaining({ priority: 200 }));
    expect(screen.getByText('web2.example.com')).toBeInTheDocument();
    // Previewed but not yet confirmed.
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    fireEvent.click(screen.getByRole('checkbox'));
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
  });

  it('invalidates the preview when the draft changes again', async () => {
    vi.spyOn(api, 'previewGroupChange').mockResolvedValue(previewResult());
    renderHarness();

    fireEvent.change(screen.getByLabelText('priority'), { target: { value: '200' } });
    fireEvent.click(screen.getByRole('button', { name: /Run preview/ }));
    await waitFor(() => expect(screen.getByRole('checkbox')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('checkbox'));
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();

    fireEvent.change(screen.getByLabelText('priority'), { target: { value: '300' } });
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.getByText(/form changed after the last preview/)).toBeInTheDocument();
  });

  it('keeps Save disabled when the preview request fails', async () => {
    vi.spyOn(api, 'previewGroupChange').mockRejectedValue(new Error('boom'));
    renderHarness();

    fireEvent.change(screen.getByLabelText('priority'), { target: { value: '200' } });
    fireEvent.click(screen.getByRole('button', { name: /Run preview/ }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('boom'));
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });
});

const trace: ExprTrace = {
  kind: 'all',
  label: 'all',
  matched: false,
  children: [
    {
      kind: 'compare',
      label: 'os.family = "RedHat"',
      matched: true,
      left: 'RedHat',
      right: 'RedHat',
      children: [],
    },
    {
      kind: 'not',
      label: 'not',
      matched: false,
      children: [
        {
          kind: 'fn',
          label: 'semver_gte(os.release.major, "9")',
          matched: true,
          value: true,
          children: [],
        },
      ],
    },
    {
      kind: 'compare',
      label: 'uptime_days > 30',
      matched: false,
      error: 'fact missing',
      children: [],
    },
  ],
};

describe('ExprTrace rendering', () => {
  it('flattens depth first with increasing depth', () => {
    const rows = flattenTrace(trace);
    expect(rows.map((r) => [r.trace.label, r.depth])).toEqual([
      ['all', 0],
      ['os.family = "RedHat"', 1],
      ['not', 1],
      ['semver_gte(os.release.major, "9")', 2],
      ['uptime_days > 30', 1],
    ]);
    expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length);
  });

  it('renders matched markers, indentation, operands and errors', () => {
    render(<ExprTraceTree trace={trace} />);
    const items = screen.getAllByRole('treeitem');
    expect(items).toHaveLength(5);
    expect(items[0]).toHaveAttribute('data-matched', 'false');
    expect(items[1]).toHaveAttribute('data-matched', 'true');
    expect(items[1]).toHaveAttribute('aria-level', '2');
    expect(items[1]).toHaveStyle({ paddingLeft: '16px' });
    expect(items[3]).toHaveStyle({ paddingLeft: '32px' });
    expect(within(items[1]).getByLabelText('matched')).toBeInTheDocument();
    expect(within(items[2]).getByLabelText('not matched')).toBeInTheDocument();
    expect(items[1]).toHaveTextContent('left = "RedHat"');
    expect(items[4]).toHaveTextContent('error: fact missing');
  });
});

describe('ProvenanceTable', () => {
  const entries = {
    'profile::ntp': [
      { group_id: 'g1', group_name: 'base' },
      { group_id: 'g2', group_name: 'webservers' },
    ],
    'profile::apache': [{ group_id: 'g2', group_name: 'webservers' }],
  };

  it('sorts keys and treats the last contributor as the winner', () => {
    const rows = provenanceRows(entries);
    expect(rows.map((r) => r.key)).toEqual(['profile::apache', 'profile::ntp']);
    expect(rows[1].winner).toEqual({ group_id: 'g2', group_name: 'webservers' });
    expect(provenanceRows(undefined)).toEqual([]);
  });

  it('highlights only the winning group', () => {
    render(<ProvenanceTable title="Classes" keyLabel="Class" entries={entries} />);
    const ntpRow = screen.getByText('profile::ntp').closest('tr') as HTMLElement;
    const chips = ntpRow.querySelectorAll('[data-winner]');
    expect(chips).toHaveLength(2);
    expect(chips[0]).toHaveAttribute('data-winner', 'false');
    expect(chips[0]).toHaveTextContent('base');
    expect(chips[1]).toHaveAttribute('data-winner', 'true');
    expect(chips[1]).toHaveTextContent('webservers (wins)');
  });

  it('renders nothing for an empty map', () => {
    const { container } = render(<ProvenanceTable title="Classes" keyLabel="Class" entries={{}} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe('ExplainView', () => {
  const explain: ClassificationExplain = {
    evaluated_at: '2026-10-29T10:00:00Z',
    elapsed_us: 1500,
    groups: [
      {
        id: 'g1',
        name: 'webservers',
        parent_id: null,
        priority: 100,
        outcome: 'matched',
        match_type: 'rules',
        reason: 'All rules matched',
        rules: [
          {
            rule_id: 'r1',
            fact_path: 'os.family',
            operator: '=',
            expected: 'RedHat',
            actual: 'Debian',
            matched: false,
          },
        ],
        expression: trace,
      },
      {
        id: 'g2',
        name: 'staging',
        parent_id: null,
        priority: 50,
        outcome: 'environment_mismatch',
        match_type: null,
        reason: 'Node is in environment production',
        rules: [],
        expression: null,
      },
    ],
    provenance: {
      classes: { 'profile::ntp': [{ group_id: 'g1', group_name: 'webservers' }] },
      variables: {},
      environment: { group_id: 'g1', group_name: 'webservers' },
    },
    merge_errors: ['conflict on key ntp_servers'],
  };

  it('shows outcomes, reasons, rule evaluations, environment origin and merge errors', () => {
    render(<ExplainView explain={explain} />);
    const web = screen.getByTestId('explain-group-g1');
    expect(within(web).getByText('Matched')).toBeInTheDocument();
    expect(within(web).getByText('All rules matched')).toBeInTheDocument();
    expect(within(web).getByText('"Debian"')).toBeInTheDocument();
    expect(within(web).getByRole('tree')).toBeInTheDocument();
    const staging = screen.getByTestId('explain-group-g2');
    expect(within(staging).getByText('Environment mismatch')).toBeInTheDocument();
    expect(screen.getByTestId('environment-origin')).toHaveTextContent('webservers');
    expect(screen.getByRole('alert')).toHaveTextContent('conflict on key ntp_servers');
    expect(screen.getByText('profile::ntp')).toBeInTheDocument();
  });
});
