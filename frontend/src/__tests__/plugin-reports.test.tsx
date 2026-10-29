// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import '@testing-library/jest-dom/vitest';
import type { ReactNode } from 'react';
import ExportMenu from '../components/analytics/ExportMenu';
import NewReportModal from '../components/analytics/NewReportModal';
import PluginReportView from '../components/analytics/PluginReportView';
import DatasetSourcePicker from '../components/analytics/DatasetSourcePicker';
import {
  annotationColumnsFor,
  buildDatasetFilter,
  buildParams,
  chartRows,
  datasetSourceToParams,
  EMPTY_DATASET_SOURCE,
  filterByAnnotation,
  isPluginReportType,
  parsePluginTypeId,
  schemaToFields,
  sortByAnnotation,
  validateParams,
} from '../components/analytics/pluginReportHelpers';
import Reports from '../pages/Reports';
import { api, pluginsApi } from '../services/api';
import type {
  DatasetInfo,
  JsonSchema,
  PluginReportResult,
  Report,
  ReportAnnotation,
  ReportAnnotationColumn,
  ReportTypeInfo,
} from '../types';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function wrap(children: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{children}</QueryClientProvider>);
}

const schema: JsonSchema = {
  type: 'object',
  properties: {
    team: { type: 'string', title: 'Team', description: 'Owning team' },
    days: { type: 'integer', title: 'Days', minimum: 1, maximum: 90, default: 7 },
    level: { type: 'string', enum: ['low', 'high'] },
    include_idle: { type: 'boolean', title: 'Include idle' },
    since: { type: 'string', format: 'date', title: 'Since' },
    extra: { type: 'array', title: 'Extra' },
  },
  required: ['team'],
};

const pluginType: ReportTypeInfo = {
  id: 'plugin:acme-cmdb:owners',
  label: 'Owners by team',
  description: 'Owners per team',
  source: 'plugin:acme-cmdb',
  params_schema: schema,
  schedulable: true,
};

const annColumn = (over: Partial<ReportAnnotationColumn> = {}): ReportAnnotationColumn => ({
  plugin_id: 'acme-scan',
  key: 'risk',
  label: 'Risk score',
  type: 'number',
  show_in: ['list_column', 'detail_panel'],
  sortable: true,
  filterable: true,
  ...over,
});

const annotation = (value: unknown, over: Partial<ReportAnnotation> = {}): ReportAnnotation => ({
  plugin_id: 'acme-scan',
  key: 'risk',
  label: 'Risk score',
  type: 'number',
  value,
  severity: null,
  created_at: '2026-10-29T10:00:00Z',
  show_in: ['list_column', 'detail_panel'],
  ...over,
});

describe('plugin report helpers', () => {
  it('recognises and parses plugin type ids', () => {
    expect(isPluginReportType('plugin:acme-cmdb:owners')).toBe(true);
    expect(isPluginReportType('node_health')).toBe(false);
    expect(isPluginReportType(undefined)).toBe(false);
    expect(parsePluginTypeId('plugin:acme-cmdb:owners')).toEqual({
      pluginId: 'acme-cmdb',
      typeId: 'owners',
    });
    expect(parsePluginTypeId('plugin:acme-cmdb')).toBeNull();
    expect(parsePluginTypeId('compliance')).toBeNull();
  });

  it('turns a schema into form fields', () => {
    const fields = schemaToFields(schema);
    expect(fields.map((f) => [f.name, f.kind, f.required])).toEqual([
      ['team', 'string', true],
      ['days', 'integer', false],
      ['level', 'enum', false],
      ['include_idle', 'boolean', false],
      ['since', 'string', false],
      ['extra', 'json', false],
    ]);
    expect(fields[1].defaultValue).toBe(7);
    expect(fields[4].format).toBe('date');
    expect(schemaToFields(null)).toEqual([]);
  });

  it('validates required, type, enum and range', () => {
    expect(validateParams(schema, {})).toEqual({ team: 'Required' });
    expect(
      validateParams(schema, { team: 'a', days: '10', level: 'low', since: '2026-10-01' })
    ).toEqual({});
    const errors = validateParams(schema, {
      team: 'a',
      days: '1.5',
      level: 'mid',
      since: 'yesterday',
      extra: '{',
    });
    expect(Object.keys(errors).sort()).toEqual(['days', 'extra', 'level', 'since']);
    expect(validateParams(schema, { team: 'a', days: '0' }).days).toMatch(/at least 1/);
    expect(validateParams(schema, { team: 'a', days: '91' }).days).toMatch(/at most 90/);
    expect(validateParams(schema, { team: 'a', days: 'x' }).days).toMatch(/number/);
  });

  it('builds typed params from form values', () => {
    expect(
      buildParams(schema, {
        team: 'a',
        days: '10',
        include_idle: true,
        level: '',
        extra: '[1,2]',
      })
    ).toEqual({ team: 'a', days: 10, include_idle: true, extra: [1, 2] });
  });

  it('merges chart series by x value', () => {
    expect(
      chartRows([
        {
          name: 'a',
          points: [
            { x: 'mon', y: 1 },
            { x: 'tue', y: 2 },
          ],
        },
        { name: 'b', points: [{ x: 'tue', y: 5 }] },
      ])
    ).toEqual([
      { x: 'mon', a: 1 },
      { x: 'tue', a: 2, b: 5 },
    ]);
  });

  it('builds dataset filters and custom params', () => {
    const columns = [
      { name: 'owner', type: 'string', label: null },
      { name: 'rack', type: 'integer', label: null },
      { name: 'active', type: 'boolean', label: null },
    ];
    expect(buildDatasetFilter(columns, { owner: 'a', rack: '4', active: 'true', x: 'y' })).toEqual({
      owner: 'a',
      rack: 4,
      active: true,
    });
    expect(datasetSourceToParams(EMPTY_DATASET_SOURCE, columns)).toBeNull();
    expect(
      datasetSourceToParams({ dataset: 'plugin:p:d', filter: { owner: 'a' }, limit: '25' }, columns)
    ).toEqual({ dataset: 'plugin:p:d', filter: { owner: 'a' }, limit: 25 });
  });

  it('selects, sorts and filters annotation columns', () => {
    const cols = [annColumn(), annColumn({ key: 'note', show_in: ['detail_panel'] })];
    expect(annotationColumnsFor(cols, 'list_column').map((c) => c.key)).toEqual(['risk']);
    expect(annotationColumnsFor(cols, 'detail_panel')).toHaveLength(2);
    expect(annotationColumnsFor(undefined, 'list_column')).toEqual([]);

    const rows = [{ hash: 'a' }, { hash: 'b' }, { hash: 'c' }];
    const map = { a: [annotation(30)], b: [annotation(5)] };
    expect(sortByAnnotation(rows, map, cols[0], 'asc').map((r) => r.hash)).toEqual(['b', 'a', 'c']);
    expect(sortByAnnotation(rows, map, cols[0], 'desc').map((r) => r.hash)).toEqual([
      'a',
      'b',
      'c',
    ]);
    expect(filterByAnnotation(rows, map, cols[0], '3').map((r) => r.hash)).toEqual(['a']);
    expect(filterByAnnotation(rows, map, cols[0], ' ')).toHaveLength(3);
  });
});

describe('NewReportModal with a plugin report type', () => {
  const setup = (onCreate = vi.fn().mockResolvedValue(undefined)) => {
    vi.spyOn(api, 'getReportTypes').mockResolvedValue([pluginType]);
    vi.spyOn(api, 'getDatasets').mockResolvedValue([]);
    wrap(<NewReportModal onClose={() => {}} onCreate={onCreate} />);
    return onCreate;
  };

  const pickPluginType = async () => {
    await screen.findByRole('option', { name: /Owners by team/ });
    fireEvent.change(screen.getByLabelText('Report Type'), {
      target: { value: pluginType.id },
    });
  };

  it('renders the schema fields and blocks submit while a required field is empty', async () => {
    const onCreate = setup();
    await pickPluginType();
    expect(screen.getByLabelText(/Team/)).toBeInTheDocument();
    expect(screen.getByLabelText('Days')).toHaveValue(7);
    expect(screen.getByLabelText('level')).toBeInTheDocument();
    expect(screen.getByLabelText('Include idle')).toHaveAttribute('type', 'checkbox');
    expect(screen.getByLabelText('Since')).toHaveAttribute('type', 'date');
    expect(screen.getByLabelText('Extra')).toBeInstanceOf(HTMLTextAreaElement);
    expect(screen.getByText('Owning team')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'My report' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create Report' }));
    expect(await screen.findByText('Required')).toBeInTheDocument();
    expect(onCreate).not.toHaveBeenCalled();
  });

  it('sends the typed parameters in query_config.custom_params', async () => {
    const onCreate = setup();
    await pickPluginType();
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'My report' } });
    fireEvent.change(screen.getByLabelText(/Team/), { target: { value: 'platform' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create Report' }));
    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    expect(onCreate.mock.calls[0][0]).toMatchObject({
      name: 'My report',
      report_type: 'plugin:acme-cmdb:owners',
      query_config: { custom_params: { team: 'platform', days: 7, include_idle: false } },
    });
  });

  it('shows the server message when it rejects the parameters', async () => {
    const onCreate = setup(
      vi.fn().mockRejectedValue({ response: { data: { message: 'days must be at most 30' } } })
    );
    await pickPluginType();
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'My report' } });
    fireEvent.change(screen.getByLabelText(/Team/), { target: { value: 'platform' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create Report' }));
    expect(await screen.findByText('days must be at most 30')).toBeInTheDocument();
    expect(onCreate).toHaveBeenCalled();
  });
});

describe('DatasetSourcePicker', () => {
  const dataset: DatasetInfo = {
    id: 'plugin:acme-cmdb:servers',
    plugin_id: 'acme-cmdb',
    dataset_id: 'servers',
    label: 'Servers',
    description: null,
    columns: [
      { name: 'owner', type: 'string', label: 'Owner' },
      { name: 'rack', type: 'integer', label: null },
    ],
    rows: 10,
    updated_at: null,
  };

  it('offers filters for the chosen dataset and previews through the query endpoint', async () => {
    const query = vi.spyOn(api, 'queryDataset').mockResolvedValue({
      dataset: dataset.id,
      columns: dataset.columns,
      rows: [{ owner: 'team-a', rack: 4 }],
      total: 1,
    });
    const onChange = vi.fn();
    wrap(
      <DatasetSourcePicker
        datasets={[dataset]}
        value={{ dataset: dataset.id, filter: { owner: 'team-a' }, limit: '50' }}
        onChange={onChange}
      />
    );
    expect(screen.getByLabelText('Owner')).toHaveValue('team-a');
    expect(screen.getByLabelText('rack')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    await waitFor(() =>
      expect(query).toHaveBeenCalledWith({
        dataset: dataset.id,
        filter: { owner: 'team-a' },
        limit: 5,
      })
    );
    expect(await screen.findByText('Showing 1 of 1 matching rows')).toBeInTheDocument();
    expect(screen.getByText('team-a')).toBeInTheDocument();
  });

  it('shows only the choice when no dataset is selected', () => {
    wrap(
      <DatasetSourcePicker datasets={[dataset]} value={EMPTY_DATASET_SOURCE} onChange={vi.fn()} />
    );
    expect(screen.getByLabelText('Data source')).toHaveValue('');
    expect(screen.queryByText('Filters (equals)')).not.toBeInTheDocument();
  });
});

describe('PluginReportView', () => {
  const result: PluginReportResult = {
    report_type: 'plugin',
    type_id: 'plugin:acme-cmdb:owners',
    plugin_id: 'acme-cmdb',
    title: 'Owners by team',
    generated_at: '2026-10-29T10:00:00Z',
    params: {},
    row_count: 2,
    truncated: true,
    sections: [
      {
        kind: 'kpi',
        id: 'k',
        title: 'Totals',
        items: [
          { label: 'Servers', value: 42 },
          { label: 'Disk', value: 1.5, unit: 'TB' },
        ],
      },
      {
        kind: 'table',
        id: 't',
        title: 'Owners',
        columns: [
          { key: 'team', label: 'Team' },
          { key: 'count', label: 'Servers' },
        ],
        rows: [
          { team: 'platform', count: 30 },
          { team: 'data', count: 12 },
        ],
      },
      {
        kind: 'chart',
        id: 'c',
        title: 'Per team',
        chart: 'bar',
        series: [{ name: 'servers', points: [{ x: 'platform', y: 30 }] }],
      },
      { kind: 'text', id: 'x', title: 'Notes', text: '<script>alert(1)</script>' },
    ],
  };

  it('renders kpi, table, chart and text sections', () => {
    const { container } = wrap(<PluginReportView result={result} />);
    expect(screen.getByText('Totals')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
    expect(screen.getByText('TB')).toBeInTheDocument();
    const table = screen.getByRole('table');
    expect(within(table).getByText('platform')).toBeInTheDocument();
    expect(within(table).getByText('12')).toBeInTheDocument();
    expect(
      within(table)
        .getAllByRole('columnheader')
        .map((h) => h.textContent)
    ).toEqual(['Team', 'Servers']);
    expect(screen.getByTestId('chart-c')).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent(/truncated/);
    expect(container.querySelector('script')).toBeNull();
  });

  it('shows text sections as literal text, never as elements', () => {
    const { container } = wrap(<PluginReportView result={result} />);
    expect(screen.getByText('<script>alert(1)</script>')).toBeInTheDocument();
    expect(container.querySelector('script')).toBeNull();
  });
});

describe('ExportMenu', () => {
  it('lists the core and plugin formats and downloads in the chosen one', async () => {
    vi.spyOn(api, 'getExportFormats').mockResolvedValue([
      {
        id: 'json',
        label: 'JSON',
        content_type: 'application/json',
        extension: 'json',
        source: 'core',
      },
      {
        id: 'plugin:acme-cmdb:xlsx',
        label: 'Excel workbook',
        content_type: 'application/vnd.ms-excel',
        extension: 'xlsx',
        source: 'plugin:acme-cmdb',
      },
    ]);
    const exportExecution = vi.spyOn(api, 'exportExecution').mockResolvedValue(new Blob(['x']));
    const createUrl = vi.fn(() => 'blob:fake');
    const revokeUrl = vi.fn();
    Object.assign(URL, { createObjectURL: createUrl, revokeObjectURL: revokeUrl });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});

    wrap(<ExportMenu executionId="exec-1" />);
    const trigger = screen.getByRole('button', { name: /Export/ });
    await waitFor(() => expect(api.getExportFormats).toHaveBeenCalled());
    fireEvent.click(trigger);
    expect(await screen.findByRole('menuitem', { name: /JSON/ })).toBeInTheDocument();
    const plugin = await screen.findByRole('menuitem', { name: /Excel workbook/ });
    fireEvent.click(plugin);
    await waitFor(() =>
      expect(exportExecution).toHaveBeenCalledWith('exec-1', 'plugin:acme-cmdb:xlsx')
    );
    await waitFor(() => expect(click).toHaveBeenCalled());
    expect(createUrl).toHaveBeenCalled();
    expect(revokeUrl).toHaveBeenCalled();
  });

  it('falls back to the core formats when the catalog cannot be loaded', async () => {
    vi.spyOn(api, 'getExportFormats').mockRejectedValue(new Error('down'));
    wrap(<ExportMenu executionId="exec-1" />);
    fireEvent.click(screen.getByRole('button', { name: /Export/ }));
    expect(await screen.findByRole('menuitem', { name: /CSV/ })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: /PDF/ })).toBeInTheDocument();
  });
});

describe('Reports page annotations', () => {
  const report = (hash: string, certname: string): Report => ({
    hash,
    certname,
    status: 'changed',
    start_time: '2026-10-29T09:00:00Z',
    environment: 'production',
  });

  const setup = (columns: ReportAnnotationColumn[] | Error) => {
    vi.spyOn(api, 'getReports').mockResolvedValue([
      report('h1', 'web01.example.com'),
      report('h2', 'db01.example.com'),
    ]);
    const colSpy = vi.spyOn(api, 'getReportAnnotationColumns');
    if (columns instanceof Error) colSpy.mockRejectedValue(columns);
    else colSpy.mockResolvedValue(columns);
    const lookup = vi.spyOn(api, 'lookupReportAnnotations').mockResolvedValue({
      h1: [annotation(80, { severity: 'high' })],
      h2: [annotation(10)],
    });
    vi.spyOn(api, 'getReportEvents').mockResolvedValue([]);
    vi.spyOn(api, 'getReportAnnotations').mockResolvedValue([
      annotation(80, { severity: 'high' }),
      annotation('hidden', { key: 'other', label: 'Other', show_in: ['list_column'] }),
    ]);
    vi.spyOn(pluginsApi, 'getUiSlots').mockResolvedValue({ slots: {}, menu_ops: [] });
    wrap(<Reports />);
    return lookup;
  };

  const order = () => screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent);

  it('adds one field per list column with the values looked up for the loaded reports', async () => {
    const lookup = setup([annColumn()]);
    const field1 = await screen.findAllByTestId('annotation-acme-scan:risk');
    await waitFor(() => expect(field1[0]).toHaveTextContent('Risk score: 80'));
    expect(field1[1]).toHaveTextContent('Risk score: 10');
    expect(within(field1[0]).getByText('high')).toBeInTheDocument();
    expect(lookup).toHaveBeenCalledWith(['h1', 'h2']);
  });

  it('sorts and filters the loaded page by the annotation', async () => {
    setup([annColumn()]);
    await waitFor(() =>
      expect(screen.getAllByTestId('annotation-acme-scan:risk')[0]).toHaveTextContent('80')
    );
    expect(order()).toEqual(['web01.example.com', 'db01.example.com']);
    fireEvent.click(screen.getByRole('button', { name: 'Sort by Risk score' }));
    expect(order()).toEqual(['db01.example.com', 'web01.example.com']);
    fireEvent.change(screen.getByLabelText('Filter Risk score'), { target: { value: '8' } });
    expect(order()).toEqual(['web01.example.com']);
  });

  it('shows the detail panel annotations of an expanded report only', async () => {
    setup([annColumn()]);
    fireEvent.click(await screen.findByText('web01.example.com'));
    expect(await screen.findByText('Plugin annotations')).toBeInTheDocument();
    const panel = screen.getByText('Plugin annotations').closest('div')!;
    expect(within(panel).getByText('Risk score')).toBeInTheDocument();
    expect(within(panel).getByText('acme-scan')).toBeInTheDocument();
    expect(within(panel).queryByText('Other')).not.toBeInTheDocument();
  });

  it('renders nothing extra when no annotation column is declared', async () => {
    const lookup = setup([]);
    expect(await screen.findByText('web01.example.com')).toBeInTheDocument();
    await waitFor(() => expect(api.getReportAnnotationColumns).toHaveBeenCalled());
    expect(screen.queryByTestId('annotation-acme-scan:risk')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Plugin annotation columns')).not.toBeInTheDocument();
    expect(lookup).not.toHaveBeenCalled();
  });

  it('does not break when the annotation endpoints fail', async () => {
    setup(new Error('boom'));
    expect(await screen.findByText('web01.example.com')).toBeInTheDocument();
    expect(screen.getByText('db01.example.com')).toBeInTheDocument();
    expect(screen.queryByLabelText('Plugin annotation columns')).not.toBeInTheDocument();
  });
});
