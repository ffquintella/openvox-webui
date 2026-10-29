import type { DatasetInfo } from '../../types';
import { useDatasetQuery } from '../../hooks/useAnalytics';
import { getApiErrorMessage, inputClass, secondaryBtn } from '../plugins/pluginHelpers';
import { RowsTable } from './PluginReportView';
import {
  DEFAULT_DATASET_LIMIT,
  datasetSourceToParams,
  type DatasetSource,
} from './pluginReportHelpers';

const PREVIEW_ROWS = 5;

interface Props {
  datasets: DatasetInfo[];
  value: DatasetSource;
  onChange: (value: DatasetSource) => void;
}

/** "Data source" choice of the custom report builder: none, or a plugin dataset with filters. */
export default function DatasetSourcePicker({ datasets, value, onChange }: Props) {
  const preview = useDatasetQuery();
  const selected = datasets.find((d) => d.id === value.dataset);

  const choose = (id: string) => {
    preview.reset();
    onChange({ dataset: id, filter: {}, limit: String(DEFAULT_DATASET_LIMIT) });
  };

  const runPreview = () => {
    const params = datasetSourceToParams(value, selected?.columns ?? []);
    if (!params) return;
    preview.mutate({
      dataset: params.dataset,
      filter: params.filter,
      limit: Math.min(params.limit, PREVIEW_ROWS),
    });
  };

  return (
    <div className="space-y-3">
      <div>
        <label
          htmlFor="dataset-source"
          className="block text-sm font-medium text-gray-700 dark:text-gray-200 mb-1"
        >
          Data source
        </label>
        <select
          id="dataset-source"
          value={value.dataset}
          onChange={(e) => choose(e.target.value)}
          className={inputClass}
        >
          <option value="">None (built-in custom report)</option>
          {datasets.map((d) => (
            <option key={d.id} value={d.id}>
              {d.label} ({d.plugin_id})
            </option>
          ))}
        </select>
        {selected?.description && (
          <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">{selected.description}</p>
        )}
      </div>

      {selected && (
        <>
          <fieldset className="space-y-2">
            <legend className="text-sm font-medium text-gray-700 dark:text-gray-200">
              Filters (equals)
            </legend>
            {selected.columns.map((c) => (
              <div key={c.name} className="flex items-center gap-2">
                <label
                  htmlFor={`dataset-filter-${c.name}`}
                  className="w-1/3 text-sm text-gray-600 dark:text-gray-300 truncate"
                  title={c.name}
                >
                  {c.label || c.name}
                </label>
                {c.type === 'boolean' ? (
                  <select
                    id={`dataset-filter-${c.name}`}
                    value={value.filter[c.name] ?? ''}
                    onChange={(e) =>
                      onChange({ ...value, filter: { ...value.filter, [c.name]: e.target.value } })
                    }
                    className={inputClass}
                  >
                    <option value="">Any</option>
                    <option value="true">true</option>
                    <option value="false">false</option>
                  </select>
                ) : (
                  <input
                    id={`dataset-filter-${c.name}`}
                    type={c.type === 'integer' || c.type === 'number' ? 'number' : 'text'}
                    value={value.filter[c.name] ?? ''}
                    onChange={(e) =>
                      onChange({ ...value, filter: { ...value.filter, [c.name]: e.target.value } })
                    }
                    placeholder="Any"
                    className={inputClass}
                  />
                )}
              </div>
            ))}
          </fieldset>

          <div>
            <label
              htmlFor="dataset-limit"
              className="block text-sm font-medium text-gray-700 dark:text-gray-200 mb-1"
            >
              Row limit
            </label>
            <input
              id="dataset-limit"
              type="number"
              min={1}
              value={value.limit}
              onChange={(e) => onChange({ ...value, limit: e.target.value })}
              className={inputClass}
            />
          </div>

          <div>
            <button
              type="button"
              className={secondaryBtn}
              onClick={runPreview}
              disabled={preview.isPending}
            >
              {preview.isPending ? 'Loading...' : 'Preview'}
            </button>
            {preview.isError && (
              <p role="alert" className="text-xs text-red-600 mt-2">
                {getApiErrorMessage(preview.error, 'Preview failed')}
              </p>
            )}
            {preview.data && (
              <div className="mt-3">
                <p className="text-xs text-gray-500 dark:text-gray-400 mb-1">
                  Showing {preview.data.rows.length} of {preview.data.total} matching rows
                </p>
                <RowsTable
                  columns={preview.data.columns.map((c) => ({
                    key: c.name,
                    label: c.label || c.name,
                  }))}
                  rows={preview.data.rows}
                />
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}
