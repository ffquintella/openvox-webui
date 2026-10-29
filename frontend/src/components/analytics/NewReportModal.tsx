import { useMemo, useState } from 'react';
import { useDatasets, useReportTypes } from '../../hooks/useAnalytics';
import type { ReportQueryConfig, ReportTypeInfo } from '../../types';
import { getApiErrorMessage } from '../plugins/pluginHelpers';
import DatasetSourcePicker from './DatasetSourcePicker';
import PluginParamsForm from './PluginParamsForm';
import {
  EMPTY_DATASET_SOURCE,
  buildParams,
  datasetSourceToParams,
  initialParamValues,
  isPluginReportType,
  reportTypeLabel,
  validateParams,
  type DatasetSource,
  type ParamValues,
} from './pluginReportHelpers';

export interface NewReportData {
  name: string;
  description?: string;
  report_type: string;
  query_config: ReportQueryConfig;
  is_public?: boolean;
}

const CORE_TYPES: ReportTypeInfo[] = (
  ['node_health', 'compliance', 'change_tracking', 'drift_detection', 'custom'] as const
).map((id) => ({
  id,
  label: reportTypeLabel(id),
  description: null,
  source: 'core',
  params_schema: null,
  schedulable: true,
}));

const fieldClass =
  'w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-primary-500';

export default function NewReportModal({
  onClose,
  onCreate,
}: {
  onClose: () => void;
  onCreate: (data: NewReportData) => Promise<void>;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [reportType, setReportType] = useState('node_health');
  const [isPublic, setIsPublic] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [paramValues, setParamValues] = useState<ParamValues>({});
  const [paramErrors, setParamErrors] = useState<Record<string, string>>({});
  const [datasetSource, setDatasetSource] = useState<DatasetSource>(EMPTY_DATASET_SOURCE);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const { data: serverTypes } = useReportTypes();
  const { data: datasets = [] } = useDatasets();

  // Core types are always offered, even when the catalog cannot be loaded.
  const types = useMemo(() => {
    const fromServer = serverTypes ?? [];
    const ids = new Set(fromServer.map((t) => t.id));
    return [...CORE_TYPES.filter((t) => !ids.has(t.id)), ...fromServer];
  }, [serverTypes]);

  const current = types.find((t) => t.id === reportType);
  const isPlugin = isPluginReportType(reportType);

  const changeType = (id: string) => {
    setReportType(id);
    setParamErrors({});
    setSubmitError(null);
    const info = types.find((t) => t.id === id);
    setParamValues(isPluginReportType(id) ? initialParamValues(info?.params_schema) : {});
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitError(null);
    let query_config: ReportQueryConfig = {};

    if (isPlugin) {
      const errors = validateParams(current?.params_schema, paramValues);
      setParamErrors(errors);
      if (Object.keys(errors).length > 0) return;
      query_config = { custom_params: buildParams(current?.params_schema, paramValues) };
    } else if (reportType === 'custom') {
      const ds = datasets.find((d) => d.id === datasetSource.dataset);
      const params = datasetSourceToParams(datasetSource, ds?.columns ?? []);
      if (params) query_config = { custom_params: params };
    }

    setIsSubmitting(true);
    try {
      await onCreate({
        name,
        description: description || undefined,
        report_type: reportType,
        query_config,
        is_public: isPublic,
      });
    } catch (err) {
      // The server validates plugin params and answers 400 with a message.
      setSubmitError(getApiErrorMessage(err, 'Failed to create the report'));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl w-full max-w-md mx-4 max-h-[90vh] flex flex-col">
        <div className="px-6 py-4 border-b border-gray-200">
          <h3 className="text-lg font-semibold text-gray-900">New Saved Report</h3>
        </div>
        <form onSubmit={handleSubmit} noValidate className="flex flex-col min-h-0">
          <div className="px-6 py-4 space-y-4 overflow-y-auto">
            <div>
              <label htmlFor="report-name" className="block text-sm font-medium text-gray-700 mb-1">
                Name
              </label>
              <input
                id="report-name"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className={fieldClass}
                required
              />
            </div>
            <div>
              <label
                htmlFor="report-description"
                className="block text-sm font-medium text-gray-700 mb-1"
              >
                Description
              </label>
              <textarea
                id="report-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className={fieldClass}
                rows={3}
              />
            </div>
            <div>
              <label htmlFor="report-type" className="block text-sm font-medium text-gray-700 mb-1">
                Report Type
              </label>
              <select
                id="report-type"
                value={reportType}
                onChange={(e) => changeType(e.target.value)}
                className={fieldClass}
              >
                {types.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.source === 'core'
                      ? t.label
                      : `${t.label} (${t.source.replace('plugin:', '')})`}
                  </option>
                ))}
              </select>
              {current?.description && (
                <p className="text-xs text-gray-500 mt-1">{current.description}</p>
              )}
            </div>

            {isPlugin && (
              <PluginParamsForm
                schema={current?.params_schema}
                values={paramValues}
                onChange={setParamValues}
                errors={paramErrors}
              />
            )}
            {reportType === 'custom' && (
              <DatasetSourcePicker
                datasets={datasets}
                value={datasetSource}
                onChange={setDatasetSource}
              />
            )}

            <div className="flex items-center">
              <input
                type="checkbox"
                id="isPublic"
                checked={isPublic}
                onChange={(e) => setIsPublic(e.target.checked)}
                className="h-4 w-4 text-primary-600 focus:ring-primary-500 border-gray-300 rounded"
              />
              <label htmlFor="isPublic" className="ml-2 text-sm text-gray-700">
                Make this report public
              </label>
            </div>
            {submitError && (
              <p role="alert" className="text-sm text-red-600">
                {submitError}
              </p>
            )}
          </div>
          <div className="px-6 py-4 border-t border-gray-200 flex justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-gray-700 hover:bg-gray-100 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button type="submit" disabled={isSubmitting || !name} className="btn-primary">
              {isSubmitting ? 'Creating...' : 'Create Report'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
