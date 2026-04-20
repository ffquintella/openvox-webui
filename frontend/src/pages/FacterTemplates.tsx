import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, FileCode2, Trash2, X, Copy, Play, Download } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import type {
  FactTemplate,
  FactDefinition,
  FactValueSource,
  FactValueSourceType,
  ExportFormat,
  Node,
} from '../types';

const VALUE_SOURCE_TYPES: { value: FactValueSourceType; label: string; description: string }[] = [
  { value: 'Static', label: 'Static', description: 'A fixed value' },
  { value: 'FromClassification', label: 'From Classification', description: 'Value from node classification' },
  { value: 'FromFact', label: 'From Fact', description: 'Copy value from another fact' },
  { value: 'Template', label: 'Template', description: 'Template string with variable substitution' },
];

export default function FacterTemplates() {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedTemplate, setSelectedTemplate] = useState<FactTemplate | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [isGenerateOpen, setIsGenerateOpen] = useState(false);
  const [generatedOutput, setGeneratedOutput] = useState<string | null>(null);
  const [exportFormat, setExportFormat] = useState<ExportFormat>('json');

  // Form state
  const [formName, setFormName] = useState('');
  const [formDescription, setFormDescription] = useState('');
  const [formFacts, setFormFacts] = useState<FactDefinition[]>([]);

  // Generate form state
  const [generateCertname, setGenerateCertname] = useState('');

  const queryClient = useQueryClient();

  const { data: templates = [], isLoading } = useQuery({
    queryKey: ['factTemplates'],
    queryFn: api.getFactTemplates,
  });

  const { data: nodes = [] } = useQuery<Node[]>({
    queryKey: ['nodes'],
    queryFn: api.getNodes,
  });

  const createMutation = useMutation({
    mutationFn: api.createFactTemplate,
    onSuccess: (newTemplate) => {
      queryClient.invalidateQueries({ queryKey: ['factTemplates'] });
      setIsCreateOpen(false);
      resetForm();
      setSelectedTemplate(newTemplate);
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof api.updateFactTemplate>[1] }) =>
      api.updateFactTemplate(id, data),
    onSuccess: (updatedTemplate) => {
      queryClient.invalidateQueries({ queryKey: ['factTemplates'] });
      setSelectedTemplate(updatedTemplate);
      setIsEditing(false);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteFactTemplate,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['factTemplates'] });
      setSelectedTemplate(null);
    },
  });

  const generateMutation = useMutation({
    mutationFn: (data: { certname: string; template: string }) =>
      api.exportFacts(data.certname, data.template, exportFormat),
    onSuccess: (output) => {
      setGeneratedOutput(output);
    },
  });

  const resetForm = () => {
    setFormName('');
    setFormDescription('');
    setFormFacts([]);
  };

  const startEditing = () => {
    if (selectedTemplate) {
      setFormName(selectedTemplate.name);
      setFormDescription(selectedTemplate.description || '');
      setFormFacts(selectedTemplate.facts);
      setIsEditing(true);
    }
  };

  const cancelEditing = () => {
    setIsEditing(false);
    resetForm();
  };

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({
      name: formName,
      description: formDescription || undefined,
      facts: formFacts,
    });
  };

  const handleUpdate = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedTemplate) return;
    updateMutation.mutate({
      id: selectedTemplate.id!,
      data: {
        name: formName,
        description: formDescription || undefined,
        facts: formFacts,
      },
    });
  };

  const handleGenerate = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedTemplate || !generateCertname) return;
    generateMutation.mutate({
      certname: generateCertname,
      template: selectedTemplate.name,
    });
  };

  const addFact = () => {
    setFormFacts([
      ...formFacts,
      { name: '', value: { type: 'Static', value: '' } },
    ]);
  };

  const updateFact = (index: number, updates: Partial<FactDefinition>) => {
    const newFacts = [...formFacts];
    newFacts[index] = { ...newFacts[index], ...updates };
    setFormFacts(newFacts);
  };

  const updateFactValue = (index: number, updates: Partial<FactValueSource>) => {
    const newFacts = [...formFacts];
    newFacts[index] = {
      ...newFacts[index],
      value: { ...newFacts[index].value, ...updates },
    };
    setFormFacts(newFacts);
  };

  const removeFact = (index: number) => {
    setFormFacts(formFacts.filter((_, i) => i !== index));
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  const getValueSourceLabel = (type: FactValueSourceType): string => {
    return VALUE_SOURCE_TYPES.find((t) => t.value === type)?.label || type;
  };

  const getValueDisplay = (source: FactValueSource): string => {
    if (source.type === 'Static') {
      return JSON.stringify(source.value);
    }
    if (source.type === 'FromFact' || source.type === 'FromClassification') {
      return String(source.value || '');
    }
    if (source.type === 'Template') {
      return String(source.value || '');
    }
    return '';
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-600" />
      </div>
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Facter Templates</h1>
          <p className="text-gray-500 mt-1">Define templates for generating external facts</p>
        </div>
        <button
          onClick={() => setIsCreateOpen(true)}
          className="btn btn-primary flex items-center"
        >
          <Plus className="w-4 h-4 mr-2" />
          New Template
        </button>
      </div>

      {/* Create Modal */}
      {isCreateOpen && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-2xl max-h-[90vh] overflow-y-auto">
            <h2 className="text-lg font-semibold mb-4">Create Fact Template</h2>
            <form onSubmit={handleCreate}>
              <div className="mb-4">
                <label className="label">Name</label>
                <input
                  type="text"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  className="input"
                  placeholder="e.g., webserver_facts"
                  required
                />
              </div>
              <div className="mb-4">
                <label className="label">Description</label>
                <textarea
                  value={formDescription}
                  onChange={(e) => setFormDescription(e.target.value)}
                  className="input"
                  rows={2}
                  placeholder="Describe what facts this template generates..."
                />
              </div>

              {/* Facts Editor */}
              <div className="mb-4">
                <div className="flex items-center justify-between mb-2">
                  <label className="label mb-0">Fact Definitions</label>
                  <button
                    type="button"
                    onClick={addFact}
                    className="btn btn-secondary text-sm"
                  >
                    <Plus className="w-4 h-4 mr-1" />
                    Add Fact
                  </button>
                </div>
                <div className="space-y-3">
                  {formFacts.map((fact, index) => (
                    <FactEditor
                      key={index}
                      fact={fact}
                      onChange={(updates) => updateFact(index, updates)}
                      onValueChange={(updates) => updateFactValue(index, updates)}
                      onRemove={() => removeFact(index)}
                    />
                  ))}
                  {formFacts.length === 0 && (
                    <p className="text-gray-500 text-sm text-center py-4 bg-gray-50 rounded">
                      No facts defined. Click "Add Fact" to start.
                    </p>
                  )}
                </div>
              </div>

              <div className="flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => {
                    setIsCreateOpen(false);
                    resetForm();
                  }}
                  className="btn btn-secondary"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={createMutation.isPending || formFacts.length === 0}
                  className="btn btn-primary"
                >
                  {createMutation.isPending ? 'Creating...' : 'Create'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Generate Modal */}
      {isGenerateOpen && selectedTemplate && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-2xl max-h-[90vh] overflow-y-auto">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold">Generate Facts</h2>
              <button
                onClick={() => {
                  setIsGenerateOpen(false);
                  setGeneratedOutput(null);
                  setGenerateCertname('');
                }}
                className="text-gray-400 hover:text-gray-600"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {!generatedOutput ? (
              <form onSubmit={handleGenerate}>
                <div className="mb-4">
                  <label className="label">Node (certname)</label>
                  <select
                    value={generateCertname}
                    onChange={(e) => setGenerateCertname(e.target.value)}
                    className="input"
                    required
                  >
                    <option value="">Select a node...</option>
                    {nodes.map((node) => (
                      <option key={node.certname} value={node.certname}>
                        {node.certname}
                      </option>
                    ))}
                  </select>
                  <p className="text-sm text-gray-500 mt-1">
                    Or enter a certname manually:
                  </p>
                  <input
                    type="text"
                    value={generateCertname}
                    onChange={(e) => setGenerateCertname(e.target.value)}
                    className="input mt-2"
                    placeholder="node.example.com"
                  />
                </div>
                <div className="mb-4">
                  <label className="label">Export Format</label>
                  <select
                    value={exportFormat}
                    onChange={(e) => setExportFormat(e.target.value as ExportFormat)}
                    className="input"
                  >
                    <option value="json">JSON</option>
                    <option value="yaml">YAML</option>
                    <option value="shell">Shell (export statements)</option>
                  </select>
                </div>
                <div className="flex justify-end gap-3">
                  <button
                    type="button"
                    onClick={() => {
                      setIsGenerateOpen(false);
                      setGenerateCertname('');
                    }}
                    className="btn btn-secondary"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={generateMutation.isPending || !generateCertname}
                    className="btn btn-primary flex items-center"
                  >
                    <Play className="w-4 h-4 mr-2" />
                    {generateMutation.isPending ? 'Generating...' : 'Generate'}
                  </button>
                </div>
              </form>
            ) : (
              <div>
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-medium text-gray-700">Generated Output</span>
                  <button
                    onClick={() => copyToClipboard(generatedOutput)}
                    className="btn btn-secondary text-sm flex items-center"
                  >
                    <Copy className="w-4 h-4 mr-1" />
                    Copy
                  </button>
                </div>
                <pre className="bg-gray-900 text-gray-100 p-4 rounded-lg overflow-x-auto text-sm max-h-96">
                  {generatedOutput}
                </pre>
                <div className="flex justify-end gap-3 mt-4">
                  <button
                    onClick={() => setGeneratedOutput(null)}
                    className="btn btn-secondary"
                  >
                    Generate Another
                  </button>
                  <button
                    onClick={() => {
                      setIsGenerateOpen(false);
                      setGeneratedOutput(null);
                      setGenerateCertname('');
                    }}
                    className="btn btn-primary"
                  >
                    Done
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Templates List */}
        <div className="lg:col-span-1">
          <div className="card p-0">
            <div className="p-4 border-b border-gray-200">
              <h2 className="font-semibold text-gray-900">All Templates</h2>
            </div>
            <div className="divide-y divide-gray-200">
              {templates.map((template: FactTemplate) => (
                <button
                  key={template.id || template.name}
                  onClick={() => {
                    setSelectedTemplate(template);
                    setIsEditing(false);
                  }}
                  className={clsx(
                    'w-full px-4 py-3 flex items-center justify-between text-left hover:bg-gray-50',
                    selectedTemplate?.name === template.name && 'bg-primary-50'
                  )}
                >
                  <div className="flex items-center">
                    <FileCode2 className="w-5 h-5 mr-3 text-gray-400" />
                    <div>
                      <p className="font-medium text-gray-900">{template.name}</p>
                      {template.description && (
                        <p className="text-sm text-gray-500 truncate max-w-[180px]">
                          {template.description}
                        </p>
                      )}
                    </div>
                  </div>
                  <span className="text-xs text-gray-400">
                    {template.facts.length} facts
                  </span>
                </button>
              ))}
              {templates.length === 0 && (
                <div className="p-4 text-center text-gray-500">
                  No templates defined
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Template Details */}
        <div className="lg:col-span-2">
          {selectedTemplate && !isEditing ? (
            <div className="card">
              <div className="flex items-center justify-between mb-6">
                <div className="flex items-center">
                  <FileCode2 className="w-8 h-8 text-primary-600 mr-3" />
                  <div>
                    <h2 className="text-xl font-semibold text-gray-900">
                      {selectedTemplate.name}
                    </h2>
                    {selectedTemplate.description && (
                      <p className="text-gray-500">{selectedTemplate.description}</p>
                    )}
                  </div>
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => setIsGenerateOpen(true)}
                    className="btn btn-secondary flex items-center"
                  >
                    <Download className="w-4 h-4 mr-2" />
                    Generate
                  </button>
                  <button
                    onClick={startEditing}
                    className="btn btn-secondary"
                  >
                    Edit
                  </button>
                  <button
                    onClick={() => deleteMutation.mutate(selectedTemplate.id!)}
                    className="btn btn-danger flex items-center"
                  >
                    <Trash2 className="w-4 h-4 mr-2" />
                    Delete
                  </button>
                </div>
              </div>

              {/* Facts Display */}
              <div>
                <h3 className="font-semibold text-gray-900 mb-3">
                  Fact Definitions ({selectedTemplate.facts.length})
                </h3>
                <div className="space-y-3">
                  {selectedTemplate.facts.map((fact, index) => (
                    <div
                      key={index}
                      className="bg-gray-50 rounded-lg p-4 border border-gray-200"
                    >
                      <div className="flex items-center justify-between mb-2">
                        <span className="font-mono font-medium text-gray-900">
                          {fact.name}
                        </span>
                        <span className="text-xs bg-primary-100 text-primary-700 px-2 py-1 rounded">
                          {getValueSourceLabel(fact.value.type)}
                        </span>
                      </div>
                      <div className="text-sm text-gray-600 font-mono bg-white px-3 py-2 rounded border">
                        {getValueDisplay(fact.value)}
                      </div>
                    </div>
                  ))}
                  {selectedTemplate.facts.length === 0 && (
                    <p className="text-gray-500 text-center py-4">
                      No facts defined in this template
                    </p>
                  )}
                </div>
              </div>
            </div>
          ) : selectedTemplate && isEditing ? (
            <div className="card">
              <h2 className="text-xl font-semibold text-gray-900 mb-6">Edit Template</h2>
              <form onSubmit={handleUpdate}>
                <div className="mb-4">
                  <label className="label">Name</label>
                  <input
                    type="text"
                    value={formName}
                    onChange={(e) => setFormName(e.target.value)}
                    className="input"
                    required
                  />
                </div>
                <div className="mb-4">
                  <label className="label">Description</label>
                  <textarea
                    value={formDescription}
                    onChange={(e) => setFormDescription(e.target.value)}
                    className="input"
                    rows={2}
                  />
                </div>

                {/* Facts Editor */}
                <div className="mb-4">
                  <div className="flex items-center justify-between mb-2">
                    <label className="label mb-0">Fact Definitions</label>
                    <button
                      type="button"
                      onClick={addFact}
                      className="btn btn-secondary text-sm"
                    >
                      <Plus className="w-4 h-4 mr-1" />
                      Add Fact
                    </button>
                  </div>
                  <div className="space-y-3">
                    {formFacts.map((fact, index) => (
                      <FactEditor
                        key={index}
                        fact={fact}
                        onChange={(updates) => updateFact(index, updates)}
                        onValueChange={(updates) => updateFactValue(index, updates)}
                        onRemove={() => removeFact(index)}
                      />
                    ))}
                    {formFacts.length === 0 && (
                      <p className="text-gray-500 text-sm text-center py-4 bg-gray-50 rounded">
                        No facts defined. Click "Add Fact" to start.
                      </p>
                    )}
                  </div>
                </div>

                <div className="flex justify-end gap-3">
                  <button
                    type="button"
                    onClick={cancelEditing}
                    className="btn btn-secondary"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={updateMutation.isPending}
                    className="btn btn-primary"
                  >
                    {updateMutation.isPending ? 'Saving...' : 'Save Changes'}
                  </button>
                </div>
              </form>
            </div>
          ) : (
            <div className="card flex items-center justify-center h-64">
              <div className="text-center text-gray-500">
                <FileCode2 className="w-12 h-12 mx-auto mb-4 text-gray-300" />
                <p>Select a template to view details</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// Fact Editor Component
interface FactEditorProps {
  fact: FactDefinition;
  onChange: (updates: Partial<FactDefinition>) => void;
  onValueChange: (updates: Partial<FactValueSource>) => void;
  onRemove: () => void;
}

function FactEditor({ fact, onChange, onValueChange, onRemove }: FactEditorProps) {
  const getValueInput = () => {
    switch (fact.value.type) {
      case 'Static':
        return (
          <input
            type="text"
            value={typeof fact.value.value === 'string' ? fact.value.value : JSON.stringify(fact.value.value)}
            onChange={(e) => {
              // Try to parse as JSON, fall back to string
              let value: unknown = e.target.value;
              try {
                value = JSON.parse(e.target.value);
              } catch {
                // Keep as string
              }
              onValueChange({ value });
            }}
            className="input"
            placeholder="Value (string or JSON)"
          />
        );
      case 'FromFact':
        return (
          <input
            type="text"
            value={String(fact.value.value || '')}
            onChange={(e) => onValueChange({ value: e.target.value })}
            className="input"
            placeholder="Source fact name (e.g., networking.ip)"
          />
        );
      case 'FromClassification':
        return (
          <input
            type="text"
            value={String(fact.value.value || '')}
            onChange={(e) => onValueChange({ value: e.target.value })}
            className="input"
            placeholder="Classification key (e.g., environment)"
          />
        );
      case 'Template':
        return (
          <input
            type="text"
            value={String(fact.value.value || '')}
            onChange={(e) => onValueChange({ value: e.target.value })}
            className="input"
            placeholder="Template (e.g., ${certname}-${environment})"
          />
        );
      default:
        return null;
    }
  };

  return (
    <div className="bg-gray-50 rounded-lg p-4 border border-gray-200">
      <div className="flex items-start gap-3">
        <div className="flex-1 space-y-3">
          <div>
            <label className="text-xs font-medium text-gray-500 mb-1 block">Fact Name</label>
            <input
              type="text"
              value={fact.name}
              onChange={(e) => onChange({ name: e.target.value })}
              className="input"
              placeholder="e.g., custom_fact_name"
              required
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="text-xs font-medium text-gray-500 mb-1 block">Value Source</label>
              <select
                value={fact.value.type}
                onChange={(e) =>
                  onValueChange({ type: e.target.value as FactValueSourceType, value: '' })
                }
                className="input"
              >
                {VALUE_SOURCE_TYPES.map((type) => (
                  <option key={type.value} value={type.value}>
                    {type.label}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-xs font-medium text-gray-500 mb-1 block">Value</label>
              {getValueInput()}
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={onRemove}
          className="text-gray-400 hover:text-red-600 mt-6"
        >
          <X className="w-5 h-5" />
        </button>
      </div>
    </div>
  );
}
