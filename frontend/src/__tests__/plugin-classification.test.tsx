// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import type { ReactNode } from 'react';
import ClassificationTab from '../components/plugins/ClassificationTab';
import MembershipSection from '../components/groups/MembershipSection';
import {
  extensionCount,
  heldSummary,
  instanceState,
  parseJsonObject,
  pluginOfRef,
  providerNeedsAttention,
  providerStatusLabel,
} from '../components/plugins/classificationHelpers';
import {
  behaviorOf,
  behaviorToForm,
  defaultBehavior,
  formToBehavior,
  isGeneratedGroup,
  providerKey,
  splitProviderKey,
  validateBehaviorForm,
} from '../components/groups/expressionHelpers';
import { api, pluginsApi } from '../services/api';
import type {
  GroupTemplateInstance,
  HeldMembership,
  MembershipProviderStatus,
  PluginClassificationOverview,
  PluginClassificationView,
} from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function wrap(children: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{children}</QueryClientProvider>);
}

const status = (over: Partial<MembershipProviderStatus> = {}): MembershipProviderStatus => ({
  group_id: 'g1',
  plugin_id: 'acme-cls',
  provider_id: 'critical',
  last_refresh_at: '2026-11-03T10:00:00Z',
  last_status: 'ok',
  last_error: null,
  members: 10,
  stale: false,
  held: false,
  ...over,
});

const held = (): HeldMembership => ({
  group_id: 'g1',
  plugin_id: 'acme-cls',
  provider_id: 'critical',
  added: ['new.example.com'],
  removed: ['a.example.com', 'b.example.com'],
  current: 10,
  proposed: 9,
  churn_pct: 30,
  limit_pct: 20,
  created_at: '2026-11-03T10:00:00Z',
});

const instance = (over: Partial<GroupTemplateInstance> = {}): GroupTemplateInstance => ({
  id: 'i1',
  plugin_id: 'acme-cls',
  template_id: 'by-dc',
  name: 'dc',
  parent_id: null,
  classes: {},
  variables: {},
  approved: false,
  approved_by: null,
  approved_at: null,
  enabled: true,
  last_run_at: null,
  last_status: null,
  generated_groups: 0,
  created_at: '2026-11-03T10:00:00Z',
  ...over,
});

const view = (over: Partial<PluginClassificationView> = {}): PluginClassificationView => ({
  plugin_id: 'acme-cls',
  version: '1.0.0',
  functions: [{ id: 'is_prod', args: ['string'], returns: 'bool', handler: 'h' }],
  operators: [{ id: 'starts', arg_types: ['string', 'string'], handler: 'h' }],
  merge_strategies: [{ id: 'concat', handler: 'h' }],
  providers: [
    {
      id: 'critical',
      label: 'Critical assets',
      refresh: { cron: '*/15 * * * *' },
      handler: 'members',
      groups: [status({ held: true })],
    },
  ],
  templates: [
    {
      id: 'by-dc',
      group_by: "fact('datacenter')",
      max_groups: 2,
      requires_approval: true,
      instances: [instance()],
    },
  ],
  ...over,
});

const overview = (
  over: Partial<PluginClassificationOverview> = {}
): PluginClassificationOverview => ({
  plugins: [view()],
  held: [held()],
  unavailable: {},
  ...over,
});

describe('classification helpers', () => {
  it('labels provider status and flags what needs attention', () => {
    expect(providerStatusLabel(status())).toBe('ok');
    expect(providerStatusLabel(status({ stale: true }))).toBe('stale');
    expect(providerStatusLabel(status({ held: true }))).toBe('update held');
    expect(providerStatusLabel(status({ last_status: 'never' }))).toBe('never refreshed');
    expect(providerNeedsAttention(status())).toBe(false);
    expect(providerNeedsAttention(status({ last_status: 'failed' }))).toBe(true);
    expect(providerNeedsAttention(status({ stale: true }))).toBe(true);
  });

  it('summarises a held update and the state of an instance', () => {
    expect(heldSummary(held())).toBe('adds 1 and removes 2 of 10 members (30%, limit 20%)');
    expect(instanceState(instance())).toBe('pending');
    expect(instanceState(instance({ approved: true }))).toBe('active');
    expect(instanceState(instance({ approved: true, enabled: false }))).toBe('paused');
  });

  it('counts extensions and reads plugin references', () => {
    expect(extensionCount(view())).toBe(5);
    expect(pluginOfRef('plugin:acme-cls:is_prod')).toBe('acme-cls');
    expect(pluginOfRef('lower')).toBeNull();
    expect(pluginOfRef('plugin:broken')).toBeNull();
  });

  it('parses JSON objects typed by the administrator', () => {
    expect(parseJsonObject('')).toEqual({ ok: true, value: {} });
    expect(parseJsonObject('{"a": 1}')).toEqual({ ok: true, value: { a: 1 } });
    expect(parseJsonObject('[1]').ok).toBe(false);
    expect(parseJsonObject('{').ok).toBe(false);
  });
});

describe('group kind in the behavior form', () => {
  const dynamic = {
    ...defaultBehavior(),
    kind: 'dynamic' as const,
    membership: { plugin_id: 'acme-cls', provider_id: 'critical', params: { tag: 'x' } },
  };

  it('keeps a dynamic group dynamic when the form is saved again', () => {
    const form = behaviorToForm(dynamic);
    expect(form.kind).toBe('dynamic');
    expect(form.provider).toBe('acme-cls/critical');
    expect(formToBehavior(form)).toEqual(dynamic);
  });

  it('sends nothing extra for a static group and validates a dynamic one', () => {
    expect(formToBehavior(behaviorToForm(defaultBehavior()))).toEqual(defaultBehavior());
    const form = { ...behaviorToForm(defaultBehavior()), kind: 'dynamic' as const };
    expect(validateBehaviorForm(form)).toEqual(['A dynamic group needs a membership provider.']);
    expect(
      validateBehaviorForm({ ...form, provider: 'acme-cls/critical', providerParams: '[1]' })
    ).toEqual(['Provider parameters must be a JSON object.']);
    expect(
      validateBehaviorForm({ ...form, provider: 'acme-cls/critical', providerParams: '{"a":1}' })
    ).toEqual([]);
  });

  it('splits provider keys and recognises generated groups', () => {
    expect(splitProviderKey(providerKey('p', 'q'))).toEqual({ plugin_id: 'p', provider_id: 'q' });
    expect(splitProviderKey('nope')).toBeNull();
    expect(isGeneratedGroup(behaviorOf({ ...defaultBehavior(), kind: 'generated' }))).toBe(true);
    expect(isGeneratedGroup(defaultBehavior())).toBe(false);
  });
});

describe('ClassificationTab', () => {
  it('shows the vocabulary, providers, held updates and templates', async () => {
    vi.spyOn(pluginsApi, 'getPluginClassification').mockResolvedValue(overview());
    vi.spyOn(api, 'getGroups').mockResolvedValue([]);
    wrap(<ClassificationTab canAdmin />);
    expect(await screen.findByText('plugin:acme-cls:is_prod')).toBeInTheDocument();
    expect(screen.getByText('plugin:acme-cls:starts')).toBeInTheDocument();
    expect(screen.getByText('Critical assets')).toBeInTheDocument();
    expect(screen.getByText(/Update held by the churn guard/)).toBeInTheDocument();
    expect(screen.getByText('waiting for approval')).toBeInTheDocument();
  });

  it('asks for confirmation before applying a held update', async () => {
    vi.spyOn(pluginsApi, 'getPluginClassification').mockResolvedValue(overview());
    vi.spyOn(api, 'getGroups').mockResolvedValue([]);
    const approve = vi.spyOn(pluginsApi, 'approveHeldMembership').mockResolvedValue({
      applied: true,
      held: false,
      added: 1,
      removed: 2,
      members: 9,
      churn_pct: 30,
    });
    wrap(<ClassificationTab canAdmin />);
    fireEvent.click(await screen.findByText('Review and approve'));
    expect(screen.getByText('new.example.com')).toBeInTheDocument();
    expect(approve).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText('Approve update'));
    await waitFor(() => expect(approve).toHaveBeenCalledWith('g1'));
  });

  it('approves a template instance and blocks changes without plugins:admin', async () => {
    vi.spyOn(pluginsApi, 'getPluginClassification').mockResolvedValue(overview());
    vi.spyOn(api, 'getGroups').mockResolvedValue([]);
    const approve = vi.spyOn(pluginsApi, 'approveTemplateInstance').mockResolvedValue();
    wrap(<ClassificationTab canAdmin />);
    fireEvent.click(await screen.findByText('Approve'));
    await waitFor(() => expect(approve).toHaveBeenCalledWith('i1'));
    cleanup();
    wrap(<ClassificationTab canAdmin={false} />);
    expect(await screen.findByText('Approve')).toBeDisabled();
    expect(screen.getByText('Use template')).toBeDisabled();
  });

  it('warns about groups that need a plugin which is not enabled', async () => {
    vi.spyOn(pluginsApi, 'getPluginClassification').mockResolvedValue(
      overview({
        plugins: [],
        held: [],
        unavailable: { prod: ['plugin:acme-cls:is_prod needs the plugin'] },
      })
    );
    vi.spyOn(api, 'getGroups').mockResolvedValue([]);
    wrap(<ClassificationTab canAdmin />);
    expect(await screen.findByText(/need a plugin which is not enabled/)).toBeInTheDocument();
    expect(screen.getByText(/never match until the plugin is enabled/)).toBeInTheDocument();
    expect(screen.getByText(/No enabled plugin extends classification/)).toBeInTheDocument();
  });
});

describe('MembershipSection', () => {
  it('stays out of the way without providers and shows the choices for a dynamic group', async () => {
    vi.spyOn(pluginsApi, 'getPluginClassification').mockResolvedValue(
      overview({ plugins: [view()] })
    );
    vi.spyOn(pluginsApi, 'getHeldMembership').mockResolvedValue(null);
    const onChange = vi.fn();
    wrap(
      <MembershipSection
        behavior={behaviorToForm(defaultBehavior())}
        onBehaviorChange={onChange}
        groupId={null}
        canAdmin
      />
    );
    const kind = await screen.findByLabelText('Group kind');
    fireEvent.change(kind, { target: { value: 'dynamic' } });
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ kind: 'dynamic' }));
  });

  it('renders nothing for a static group when no plugin offers providers', async () => {
    vi.spyOn(pluginsApi, 'getPluginClassification').mockResolvedValue(
      overview({ plugins: [view({ providers: [] })] })
    );
    const { container } = wrap(
      <MembershipSection
        behavior={behaviorToForm(defaultBehavior())}
        onBehaviorChange={vi.fn()}
        groupId={null}
        canAdmin
      />
    );
    await waitFor(() => expect(pluginsApi.getPluginClassification).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });
});
