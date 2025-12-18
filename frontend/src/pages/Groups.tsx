import { useState, useMemo } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Plus,
  FolderTree,
  ChevronRight,
  Trash2,
  X,
  Loader2,
  Server,
  Filter,
  Pin,
  Settings,
  GitBranch,
  Edit2,
  AlertCircle,
  Variable,
} from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';
import type {
  NodeGroup,
  ClassificationRule,
  RuleOperator,
  RuleMatchType,
  CreateRuleRequest,
} from '../types';

const RULE_OPERATORS: { value: RuleOperator; label: string; description: string }[] = [
  { value: '=', label: '=', description: 'Equals' },
  { value: '!=', label: '!=', description: 'Not equals' },
  { value: '~', label: '~', description: 'Matches regex' },
  { value: '!~', label: '!~', description: 'Does not match regex' },
  { value: '>', label: '>', description: 'Greater than' },
  { value: '>=', label: '>=', description: 'Greater than or equal' },
  { value: '<', label: '<', description: 'Less than' },
  { value: '<=', label: '<=', description: 'Less than or equal' },
  { value: 'in', label: 'in', description: 'Value is in list' },
  { value: 'not_in', label: 'not_in', description: 'Value is not in list' },
];

export default function Groups() {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [selectedGroup, setSelectedGroup] = useState<NodeGroup | null>(null);
  const [activeTab, setActiveTab] = useState<'rules' | 'pinned' | 'classes' | 'variables'>('rules');

  // Create/Edit form state
  const [formName, setFormName] = useState('');
  const [formDescription, setFormDescription] = useState('');
  const [formParentId, setFormParentId] = useState<string>('');
  const [formEnvironment, setFormEnvironment] = useState('');
  const [formMatchType, setFormMatchType] = useState<RuleMatchType>('all');

  // Rule form state
  const [isAddRuleOpen, setIsAddRuleOpen] = useState(false);
  const [newRuleFactPath, setNewRuleFactPath] = useState('');
  const [newRuleOperator, setNewRuleOperator] = useState<RuleOperator>('=');
  const [newRuleValue, setNewRuleValue] = useState('');

  // Pinned node form state
  const [isAddPinnedOpen, setIsAddPinnedOpen] = useState(false);
  const [newPinnedNode, setNewPinnedNode] = useState('');

  // Class form state
  const [isAddClassOpen, setIsAddClassOpen] = useState(false);
  const [newClassName, setNewClassName] = useState('');

  // Parameter form state
  const [isAddParamOpen, setIsAddParamOpen] = useState(false);
  const [newParamKey, setNewParamKey] = useState('');
  const [newParamValue, setNewParamValue] = useState('');

  // Variable form state (for facter facts)
  const [isAddVarOpen, setIsAddVarOpen] = useState(false);
  const [newVarKey, setNewVarKey] = useState('');
  const [newVarValue, setNewVarValue] = useState('');

  const queryClient = useQueryClient();

  const { data: groups = [], isLoading } = useQuery({
    queryKey: ['groups'],
    queryFn: api.getGroups,
  });

  const { data: nodes = [] } = useQuery({
    queryKey: ['nodes'],
    queryFn: api.getNodes,
  });

  const { data: matchedNodes = [] } = useQuery({
    queryKey: ['group-nodes', selectedGroup?.id],
    queryFn: () => api.getGroupNodes(selectedGroup!.id),
    enabled: !!selectedGroup,
  });

  // Build group hierarchy for display
  const groupHierarchy = useMemo(() => {
    const rootGroups: NodeGroup[] = [];
    const childrenMap = new Map<string, NodeGroup[]>();

    groups.forEach((group: NodeGroup) => {
      if (!group.parent_id) {
        rootGroups.push(group);
      } else {
        const children = childrenMap.get(group.parent_id) || [];
        children.push(group);
        childrenMap.set(group.parent_id, children);
      }
    });

    return { rootGroups, childrenMap };
  }, [groups]);

  const createMutation = useMutation({
    mutationFn: api.createGroup,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      setIsCreateOpen(false);
      resetForm();
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof api.updateGroup>[1] }) =>
      api.updateGroup(id, data),
    onSuccess: (updatedGroup) => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      setSelectedGroup(updatedGroup);
      setIsEditOpen(false);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteGroup,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      setSelectedGroup(null);
    },
  });

  const addRuleMutation = useMutation({
    mutationFn: ({ groupId, rule }: { groupId: string; rule: CreateRuleRequest }) =>
      api.addGroupRule(groupId, rule),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      if (selectedGroup) {
        api.getGroup(selectedGroup.id).then((group) => {
          if (group) setSelectedGroup(group);
        });
      }
      setIsAddRuleOpen(false);
      resetRuleForm();
    },
  });

  const deleteRuleMutation = useMutation({
    mutationFn: ({ groupId, ruleId }: { groupId: string; ruleId: string }) =>
      api.deleteGroupRule(groupId, ruleId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      if (selectedGroup) {
        api.getGroup(selectedGroup.id).then((group) => {
          if (group) setSelectedGroup(group);
        });
      }
    },
  });

  const addPinnedMutation = useMutation({
    mutationFn: ({ groupId, certname }: { groupId: string; certname: string }) =>
      api.addPinnedNode(groupId, certname),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['group-nodes', selectedGroup?.id] });
      if (selectedGroup) {
        api.getGroup(selectedGroup.id).then((group) => {
          if (group) setSelectedGroup(group);
        });
      }
      setIsAddPinnedOpen(false);
      setNewPinnedNode('');
    },
  });

  const removePinnedMutation = useMutation({
    mutationFn: ({ groupId, certname }: { groupId: string; certname: string }) =>
      api.removePinnedNode(groupId, certname),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['groups'] });
      queryClient.invalidateQueries({ queryKey: ['group-nodes', selectedGroup?.id] });
      if (selectedGroup) {
        api.getGroup(selectedGroup.id).then((group) => {
          if (group) setSelectedGroup(group);
        });
      }
    },
  });

  const resetForm = () => {
    setFormName('');
    setFormDescription('');
    setFormParentId('');
    setFormEnvironment('');
    setFormMatchType('all');
  };

  const resetRuleForm = () => {
    setNewRuleFactPath('');
    setNewRuleOperator('=');
    setNewRuleValue('');
  };

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({
      name: formName,
      description: formDescription || undefined,
      parent_id: formParentId || undefined,
      environment: formEnvironment || undefined,
      rule_match_type: formMatchType,
    });
  };

  const handleEdit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedGroup) return;
    updateMutation.mutate({
      id: selectedGroup.id,
      data: {
        name: formName,
        description: formDescription || undefined,
        parent_id: formParentId || null,
        environment: formEnvironment || null,
        rule_match_type: formMatchType,
      },
    });
  };

  const handleAddRule = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedGroup) return;

    let parsedValue: unknown = newRuleValue;

    // Parse value based on operator
    if (newRuleOperator === 'in' || newRuleOperator === 'not_in') {
      try {
        parsedValue = JSON.parse(newRuleValue);
      } catch {
        // Try to split by comma if not valid JSON
        parsedValue = newRuleValue.split(',').map(s => s.trim());
      }
    } else {
      // Try to parse as number or boolean
      if (newRuleValue === 'true') parsedValue = true;
      else if (newRuleValue === 'false') parsedValue = false;
      else if (!isNaN(Number(newRuleValue)) && newRuleValue !== '') parsedValue = Number(newRuleValue);
    }

    addRuleMutation.mutate({
      groupId: selectedGroup.id,
      rule: {
        fact_path: newRuleFactPath,
        operator: newRuleOperator,
        value: parsedValue,
      },
    });
  };

  const handleAddClass = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedGroup || !newClassName.trim()) return;

    const updatedClasses = [...selectedGroup.classes, newClassName.trim()];
    updateMutation.mutate({
      id: selectedGroup.id,
      data: { classes: updatedClasses },
    });
    setIsAddClassOpen(false);
    setNewClassName('');
  };

  const handleRemoveClass = (className: string) => {
    if (!selectedGroup) return;
    const updatedClasses = selectedGroup.classes.filter(c => c !== className);
    updateMutation.mutate({
      id: selectedGroup.id,
      data: { classes: updatedClasses },
    });
  };

  const handleAddParameter = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedGroup || !newParamKey.trim()) return;

    let parsedValue: unknown = newParamValue;
    try {
      parsedValue = JSON.parse(newParamValue);
    } catch {
      // Keep as string if not valid JSON
    }

    const updatedParams = {
      ...(selectedGroup.parameters as Record<string, unknown>),
      [newParamKey.trim()]: parsedValue,
    };
    updateMutation.mutate({
      id: selectedGroup.id,
      data: { parameters: updatedParams },
    });
    setIsAddParamOpen(false);
    setNewParamKey('');
    setNewParamValue('');
  };

  const handleRemoveParameter = (key: string) => {
    if (!selectedGroup) return;
    const params = { ...(selectedGroup.parameters as Record<string, unknown>) };
    delete params[key];
    updateMutation.mutate({
      id: selectedGroup.id,
      data: { parameters: params },
    });
  };

  const handleAddVariable = (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedGroup || !newVarKey.trim()) return;

    let parsedValue: unknown = newVarValue;
    try {
      parsedValue = JSON.parse(newVarValue);
    } catch {
      // Keep as string if not valid JSON
    }

    const updatedVars = {
      ...(selectedGroup.variables as Record<string, unknown>),
      [newVarKey.trim()]: parsedValue,
    };
    updateMutation.mutate({
      id: selectedGroup.id,
      data: { variables: updatedVars },
    });
    setIsAddVarOpen(false);
    setNewVarKey('');
    setNewVarValue('');
  };

  const handleRemoveVariable = (key: string) => {
    if (!selectedGroup) return;
    const vars = { ...(selectedGroup.variables as Record<string, unknown>) };
    delete vars[key];
    updateMutation.mutate({
      id: selectedGroup.id,
      data: { variables: vars },
    });
  };

  const openEditModal = () => {
    if (!selectedGroup) return;
    setFormName(selectedGroup.name);
    setFormDescription(selectedGroup.description || '');
    setFormParentId(selectedGroup.parent_id || '');
    setFormEnvironment(selectedGroup.environment || '');
    setFormMatchType(selectedGroup.rule_match_type);
    setIsEditOpen(true);
  };

  const getParentGroup = (parentId: string | null | undefined): NodeGroup | undefined => {
    if (!parentId) return undefined;
    return groups.find((g: NodeGroup) => g.id === parentId);
  };

  const renderGroupItem = (group: NodeGroup, depth: number = 0) => {
    const children = groupHierarchy.childrenMap.get(group.id) || [];
    const isSelected = selectedGroup?.id === group.id;

    return (
      <div key={group.id}>
        <button
          onClick={() => setSelectedGroup(group)}
          className={clsx(
            'w-full px-4 py-3 flex items-center justify-between text-left hover:bg-gray-50 transition-colors',
            isSelected && 'bg-primary-50 border-l-2 border-primary-500'
          )}
          style={{ paddingLeft: `${16 + depth * 20}px` }}
        >
          <div className="flex items-center min-w-0">
            {depth > 0 && (
              <GitBranch className="w-4 h-4 text-gray-300 mr-2 flex-shrink-0" />
            )}
            <FolderTree
              className={clsx(
                'w-5 h-5 mr-3 flex-shrink-0',
                isSelected ? 'text-primary-600' : 'text-gray-400'
              )}
            />
            <div className="min-w-0">
              <p className={clsx('font-medium truncate', isSelected ? 'text-primary-900' : 'text-gray-900')}>
                {group.name}
              </p>
              {group.description && (
                <p className="text-sm text-gray-500 truncate">{group.description}</p>
              )}
            </div>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0 ml-2">
            <span className="text-xs text-gray-400">{group.rules?.length || 0} rules</span>
            <ChevronRight className="w-4 h-4 text-gray-400" />
          </div>
        </button>
        {children.map(child => renderGroupItem(child, depth + 1))}
      </div>
    );
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
          <h1 className="text-2xl font-bold text-gray-900">Node Groups</h1>
          <p className="text-gray-500 mt-1">Organize nodes with classification rules</p>
        </div>
        <button
          onClick={() => {
            resetForm();
            setIsCreateOpen(true);
          }}
          className="btn btn-primary flex items-center"
        >
          <Plus className="w-4 h-4 mr-2" />
          New Group
        </button>
      </div>

      {/* Create/Edit Modal */}
      {(isCreateOpen || isEditOpen) && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-lg">
            <h2 className="text-lg font-semibold mb-4">
              {isCreateOpen ? 'Create Node Group' : 'Edit Node Group'}
            </h2>
            <form onSubmit={isCreateOpen ? handleCreate : handleEdit}>
              <div className="space-y-4">
                <div>
                  <label className="label">Name</label>
                  <input
                    type="text"
                    value={formName}
                    onChange={(e) => setFormName(e.target.value)}
                    className="input"
                    placeholder="e.g., webservers"
                    required
                  />
                </div>
                <div>
                  <label className="label">Description</label>
                  <textarea
                    value={formDescription}
                    onChange={(e) => setFormDescription(e.target.value)}
                    className="input"
                    rows={2}
                    placeholder="Optional description..."
                  />
                </div>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="label">Parent Group</label>
                    <select
                      value={formParentId}
                      onChange={(e) => setFormParentId(e.target.value)}
                      className="input"
                    >
                      <option value="">None (root group)</option>
                      {groups
                        .filter((g: NodeGroup) => g.id !== selectedGroup?.id)
                        .map((g: NodeGroup) => (
                          <option key={g.id} value={g.id}>
                            {g.name}
                          </option>
                        ))}
                    </select>
                  </div>
                  <div>
                    <label className="label">Environment</label>
                    <input
                      type="text"
                      value={formEnvironment}
                      onChange={(e) => setFormEnvironment(e.target.value)}
                      className="input"
                      placeholder="e.g., production"
                    />
                  </div>
                </div>
                <div>
                  <label className="label">Rule Match Type</label>
                  <div className="flex gap-4">
                    <label className="flex items-center">
                      <input
                        type="radio"
                        value="all"
                        checked={formMatchType === 'all'}
                        onChange={() => setFormMatchType('all')}
                        className="mr-2"
                      />
                      <span className="text-sm">All rules must match (AND)</span>
                    </label>
                    <label className="flex items-center">
                      <input
                        type="radio"
                        value="any"
                        checked={formMatchType === 'any'}
                        onChange={() => setFormMatchType('any')}
                        className="mr-2"
                      />
                      <span className="text-sm">Any rule can match (OR)</span>
                    </label>
                  </div>
                </div>
              </div>
              <div className="flex justify-end gap-3 mt-6">
                <button
                  type="button"
                  onClick={() => {
                    setIsCreateOpen(false);
                    setIsEditOpen(false);
                    resetForm();
                  }}
                  className="btn btn-secondary"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={createMutation.isPending || updateMutation.isPending}
                  className="btn btn-primary"
                >
                  {(createMutation.isPending || updateMutation.isPending) ? (
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  ) : null}
                  {isCreateOpen ? 'Create' : 'Save Changes'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Groups List */}
        <div className="lg:col-span-1">
          <div className="card p-0">
            <div className="p-4 border-b border-gray-200">
              <h2 className="font-semibold text-gray-900">All Groups ({groups.length})</h2>
            </div>
            <div className="divide-y divide-gray-100 max-h-[calc(100vh-280px)] overflow-y-auto">
              {groupHierarchy.rootGroups.map(group => renderGroupItem(group))}
              {groups.length === 0 && (
                <div className="p-8 text-center text-gray-500">
                  <FolderTree className="w-12 h-12 mx-auto mb-4 text-gray-300" />
                  <p>No groups yet</p>
                  <p className="text-sm mt-1">Create a group to get started</p>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Group Details */}
        <div className="lg:col-span-2">
          {selectedGroup ? (
            <div className="card">
              {/* Header */}
              <div className="flex items-start justify-between mb-6">
                <div className="flex items-center">
                  <FolderTree className="w-8 h-8 text-primary-600 mr-3" />
                  <div>
                    <h2 className="text-xl font-semibold text-gray-900">{selectedGroup.name}</h2>
                    {selectedGroup.description && (
                      <p className="text-gray-500">{selectedGroup.description}</p>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={openEditModal}
                    className="btn btn-secondary flex items-center"
                  >
                    <Edit2 className="w-4 h-4 mr-2" />
                    Edit
                  </button>
                  <button
                    onClick={() => deleteMutation.mutate(selectedGroup.id)}
                    disabled={deleteMutation.isPending}
                    className="btn btn-danger flex items-center"
                  >
                    {deleteMutation.isPending ? (
                      <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                    ) : (
                      <Trash2 className="w-4 h-4 mr-2" />
                    )}
                    Delete
                  </button>
                </div>
              </div>

              {/* Group Info */}
              <div className="grid grid-cols-3 gap-4 mb-6">
                <div className="bg-gray-50 rounded-lg p-3">
                  <p className="text-xs text-gray-500 uppercase tracking-wide">Parent Group</p>
                  <p className="font-medium text-gray-900 mt-1">
                    {getParentGroup(selectedGroup.parent_id)?.name || 'None (root)'}
                  </p>
                </div>
                <div className="bg-gray-50 rounded-lg p-3">
                  <p className="text-xs text-gray-500 uppercase tracking-wide">Environment</p>
                  <p className="font-medium text-gray-900 mt-1">
                    {selectedGroup.environment || 'Any'}
                  </p>
                </div>
                <div className="bg-gray-50 rounded-lg p-3">
                  <p className="text-xs text-gray-500 uppercase tracking-wide">Match Type</p>
                  <p className="font-medium text-gray-900 mt-1">
                    {selectedGroup.rule_match_type === 'all' ? 'All rules (AND)' : 'Any rule (OR)'}
                  </p>
                </div>
              </div>

              {/* Matched Nodes Count */}
              <div className="bg-primary-50 border border-primary-200 rounded-lg p-4 mb-6">
                <div className="flex items-center justify-between">
                  <div className="flex items-center">
                    <Server className="w-5 h-5 text-primary-600 mr-2" />
                    <span className="font-medium text-primary-900">
                      {matchedNodes.length} nodes matched
                    </span>
                  </div>
                  {matchedNodes.length > 0 && (
                    <span className="text-sm text-primary-600">
                      {matchedNodes.slice(0, 3).join(', ')}
                      {matchedNodes.length > 3 && ` +${matchedNodes.length - 3} more`}
                    </span>
                  )}
                </div>
              </div>

              {/* Tabs */}
              <div className="border-b border-gray-200 mb-4">
                <nav className="-mb-px flex gap-6">
                  {[
                    { id: 'rules', label: 'Classification Rules', icon: Filter },
                    { id: 'pinned', label: 'Pinned Nodes', icon: Pin },
                    { id: 'classes', label: 'Classes', icon: Settings },
                    { id: 'variables', label: 'Variables', icon: Variable },
                  ].map(tab => (
                    <button
                      key={tab.id}
                      onClick={() => setActiveTab(tab.id as typeof activeTab)}
                      className={clsx(
                        'flex items-center pb-3 px-1 border-b-2 text-sm font-medium transition-colors',
                        activeTab === tab.id
                          ? 'border-primary-500 text-primary-600'
                          : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
                      )}
                    >
                      <tab.icon className="w-4 h-4 mr-2" />
                      {tab.label}
                      {tab.id === 'rules' && (
                        <span className="ml-2 bg-gray-100 text-gray-600 px-2 py-0.5 rounded-full text-xs">
                          {selectedGroup.rules?.length || 0}
                        </span>
                      )}
                      {tab.id === 'pinned' && (
                        <span className="ml-2 bg-gray-100 text-gray-600 px-2 py-0.5 rounded-full text-xs">
                          {selectedGroup.pinned_nodes?.length || 0}
                        </span>
                      )}
                      {tab.id === 'classes' && (
                        <span className="ml-2 bg-gray-100 text-gray-600 px-2 py-0.5 rounded-full text-xs">
                          {selectedGroup.classes?.length || 0}
                        </span>
                      )}
                      {tab.id === 'variables' && (
                        <span className="ml-2 bg-gray-100 text-gray-600 px-2 py-0.5 rounded-full text-xs">
                          {Object.keys(selectedGroup.variables || {}).length}
                        </span>
                      )}
                    </button>
                  ))}
                </nav>
              </div>

              {/* Rules Tab */}
              {activeTab === 'rules' && (
                <div>
                  <div className="flex items-center justify-between mb-4">
                    <p className="text-sm text-gray-600">
                      Define rules to automatically classify nodes based on their facts.
                    </p>
                    <button
                      onClick={() => setIsAddRuleOpen(true)}
                      className="btn btn-secondary text-sm flex items-center"
                    >
                      <Plus className="w-4 h-4 mr-1" />
                      Add Rule
                    </button>
                  </div>

                  {/* Add Rule Form */}
                  {isAddRuleOpen && (
                    <div className="bg-gray-50 rounded-lg p-4 mb-4 border border-gray-200">
                      <form onSubmit={handleAddRule} className="space-y-4">
                        <div className="grid grid-cols-3 gap-4">
                          <div>
                            <label className="label">Fact Path</label>
                            <input
                              type="text"
                              value={newRuleFactPath}
                              onChange={(e) => setNewRuleFactPath(e.target.value)}
                              className="input"
                              placeholder="e.g., os.family"
                              required
                            />
                          </div>
                          <div>
                            <label className="label">Operator</label>
                            <select
                              value={newRuleOperator}
                              onChange={(e) => setNewRuleOperator(e.target.value as RuleOperator)}
                              className="input"
                            >
                              {RULE_OPERATORS.map(op => (
                                <option key={op.value} value={op.value}>
                                  {op.label} ({op.description})
                                </option>
                              ))}
                            </select>
                          </div>
                          <div>
                            <label className="label">Value</label>
                            <input
                              type="text"
                              value={newRuleValue}
                              onChange={(e) => setNewRuleValue(e.target.value)}
                              className="input"
                              placeholder={newRuleOperator === 'in' || newRuleOperator === 'not_in'
                                ? 'value1, value2, ...'
                                : 'e.g., RedHat'}
                              required
                            />
                          </div>
                        </div>
                        <div className="flex justify-end gap-2">
                          <button
                            type="button"
                            onClick={() => {
                              setIsAddRuleOpen(false);
                              resetRuleForm();
                            }}
                            className="btn btn-secondary text-sm"
                          >
                            Cancel
                          </button>
                          <button
                            type="submit"
                            disabled={addRuleMutation.isPending}
                            className="btn btn-primary text-sm"
                          >
                            {addRuleMutation.isPending ? 'Adding...' : 'Add Rule'}
                          </button>
                        </div>
                      </form>
                    </div>
                  )}

                  {/* Rules List */}
                  <div className="space-y-2">
                    {selectedGroup.rules && selectedGroup.rules.length > 0 ? (
                      selectedGroup.rules.map((rule: ClassificationRule) => (
                        <div
                          key={rule.id}
                          className="flex items-center justify-between bg-white border border-gray-200 rounded-lg px-4 py-3"
                        >
                          <div className="flex items-center gap-3 font-mono text-sm">
                            <span className="text-primary-600 font-medium">{rule.fact_path}</span>
                            <span className="bg-gray-100 text-gray-700 px-2 py-1 rounded">
                              {rule.operator}
                            </span>
                            <span className="text-gray-900">
                              {typeof rule.value === 'object'
                                ? JSON.stringify(rule.value)
                                : String(rule.value)}
                            </span>
                          </div>
                          <button
                            onClick={() => deleteRuleMutation.mutate({
                              groupId: selectedGroup.id,
                              ruleId: rule.id
                            })}
                            disabled={deleteRuleMutation.isPending}
                            className="text-gray-400 hover:text-red-600 transition-colors"
                          >
                            <X className="w-4 h-4" />
                          </button>
                        </div>
                      ))
                    ) : (
                      <div className="text-center py-8 text-gray-500 bg-gray-50 rounded-lg">
                        <Filter className="w-8 h-8 mx-auto mb-2 text-gray-300" />
                        <p>No classification rules defined</p>
                        <p className="text-sm mt-1">Add rules to automatically classify nodes</p>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {/* Pinned Nodes Tab */}
              {activeTab === 'pinned' && (
                <div>
                  <div className="flex items-center justify-between mb-4">
                    <p className="text-sm text-gray-600">
                      Manually pin specific nodes to this group regardless of rules.
                    </p>
                    <button
                      onClick={() => setIsAddPinnedOpen(true)}
                      className="btn btn-secondary text-sm flex items-center"
                    >
                      <Plus className="w-4 h-4 mr-1" />
                      Pin Node
                    </button>
                  </div>

                  {/* Add Pinned Form */}
                  {isAddPinnedOpen && (
                    <div className="bg-gray-50 rounded-lg p-4 mb-4 border border-gray-200">
                      <form onSubmit={(e) => {
                        e.preventDefault();
                        if (selectedGroup && newPinnedNode) {
                          addPinnedMutation.mutate({
                            groupId: selectedGroup.id,
                            certname: newPinnedNode,
                          });
                        }
                      }} className="flex gap-4">
                        <div className="flex-1">
                          <select
                            value={newPinnedNode}
                            onChange={(e) => setNewPinnedNode(e.target.value)}
                            className="input"
                            required
                          >
                            <option value="">Select a node...</option>
                            {nodes
                              .filter(n => !selectedGroup.pinned_nodes?.includes(n.certname))
                              .map(node => (
                                <option key={node.certname} value={node.certname}>
                                  {node.certname}
                                </option>
                              ))}
                          </select>
                        </div>
                        <button
                          type="button"
                          onClick={() => {
                            setIsAddPinnedOpen(false);
                            setNewPinnedNode('');
                          }}
                          className="btn btn-secondary"
                        >
                          Cancel
                        </button>
                        <button
                          type="submit"
                          disabled={addPinnedMutation.isPending || !newPinnedNode}
                          className="btn btn-primary"
                        >
                          {addPinnedMutation.isPending ? 'Adding...' : 'Pin Node'}
                        </button>
                      </form>
                    </div>
                  )}

                  {/* Pinned Nodes List */}
                  <div className="space-y-2">
                    {selectedGroup.pinned_nodes && selectedGroup.pinned_nodes.length > 0 ? (
                      selectedGroup.pinned_nodes.map((certname: string) => (
                        <div
                          key={certname}
                          className="flex items-center justify-between bg-white border border-gray-200 rounded-lg px-4 py-3"
                        >
                          <div className="flex items-center gap-3">
                            <Pin className="w-4 h-4 text-primary-500" />
                            <span className="font-medium text-gray-900">{certname}</span>
                          </div>
                          <button
                            onClick={() => removePinnedMutation.mutate({
                              groupId: selectedGroup.id,
                              certname,
                            })}
                            disabled={removePinnedMutation.isPending}
                            className="text-gray-400 hover:text-red-600 transition-colors"
                          >
                            <X className="w-4 h-4" />
                          </button>
                        </div>
                      ))
                    ) : (
                      <div className="text-center py-8 text-gray-500 bg-gray-50 rounded-lg">
                        <Pin className="w-8 h-8 mx-auto mb-2 text-gray-300" />
                        <p>No pinned nodes</p>
                        <p className="text-sm mt-1">Pin specific nodes to always include them</p>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {/* Classes Tab */}
              {activeTab === 'classes' && (
                <div>
                  {/* Classes Section */}
                  <div className="mb-6">
                    <div className="flex items-center justify-between mb-4">
                      <div>
                        <h3 className="font-semibold text-gray-900">Puppet Classes</h3>
                        <p className="text-sm text-gray-600">Classes applied to nodes in this group</p>
                      </div>
                      <button
                        onClick={() => setIsAddClassOpen(true)}
                        className="btn btn-secondary text-sm flex items-center"
                      >
                        <Plus className="w-4 h-4 mr-1" />
                        Add Class
                      </button>
                    </div>

                    {isAddClassOpen && (
                      <div className="bg-gray-50 rounded-lg p-4 mb-4 border border-gray-200">
                        <form onSubmit={handleAddClass} className="flex gap-4">
                          <input
                            type="text"
                            value={newClassName}
                            onChange={(e) => setNewClassName(e.target.value)}
                            className="input flex-1"
                            placeholder="e.g., profile::webserver"
                            required
                          />
                          <button
                            type="button"
                            onClick={() => {
                              setIsAddClassOpen(false);
                              setNewClassName('');
                            }}
                            className="btn btn-secondary"
                          >
                            Cancel
                          </button>
                          <button type="submit" className="btn btn-primary">
                            Add
                          </button>
                        </form>
                      </div>
                    )}

                    <div className="flex flex-wrap gap-2">
                      {selectedGroup.classes && selectedGroup.classes.length > 0 ? (
                        selectedGroup.classes.map((className: string) => (
                          <span
                            key={className}
                            className="inline-flex items-center bg-primary-100 text-primary-700 px-3 py-1.5 rounded-full text-sm"
                          >
                            {className}
                            <button
                              onClick={() => handleRemoveClass(className)}
                              className="ml-2 text-primary-500 hover:text-primary-700"
                            >
                              <X className="w-3 h-3" />
                            </button>
                          </span>
                        ))
                      ) : (
                        <p className="text-gray-500 text-sm">No classes assigned</p>
                      )}
                    </div>
                  </div>

                  {/* Parameters Section */}
                  <div>
                    <div className="flex items-center justify-between mb-4">
                      <div>
                        <h3 className="font-semibold text-gray-900">Class Parameters</h3>
                        <p className="text-sm text-gray-600">Parameters passed to Puppet classes</p>
                      </div>
                      <button
                        onClick={() => setIsAddParamOpen(true)}
                        className="btn btn-secondary text-sm flex items-center"
                      >
                        <Plus className="w-4 h-4 mr-1" />
                        Add Parameter
                      </button>
                    </div>

                    {isAddParamOpen && (
                      <div className="bg-gray-50 rounded-lg p-4 mb-4 border border-gray-200">
                        <form onSubmit={handleAddParameter} className="space-y-4">
                          <div className="grid grid-cols-2 gap-4">
                            <div>
                              <label className="label">Key</label>
                              <input
                                type="text"
                                value={newParamKey}
                                onChange={(e) => setNewParamKey(e.target.value)}
                                className="input"
                                placeholder="e.g., port"
                                required
                              />
                            </div>
                            <div>
                              <label className="label">Value (JSON or string)</label>
                              <input
                                type="text"
                                value={newParamValue}
                                onChange={(e) => setNewParamValue(e.target.value)}
                                className="input"
                                placeholder='e.g., 8080 or ["a", "b"]'
                                required
                              />
                            </div>
                          </div>
                          <div className="flex justify-end gap-2">
                            <button
                              type="button"
                              onClick={() => {
                                setIsAddParamOpen(false);
                                setNewParamKey('');
                                setNewParamValue('');
                              }}
                              className="btn btn-secondary text-sm"
                            >
                              Cancel
                            </button>
                            <button type="submit" className="btn btn-primary text-sm">
                              Add Parameter
                            </button>
                          </div>
                        </form>
                      </div>
                    )}

                    <div className="space-y-2">
                      {selectedGroup.parameters && Object.keys(selectedGroup.parameters as Record<string, unknown>).length > 0 ? (
                        Object.entries(selectedGroup.parameters as Record<string, unknown>).map(([key, value]) => (
                          <div
                            key={key}
                            className="flex items-center justify-between bg-white border border-gray-200 rounded-lg px-4 py-3"
                          >
                            <div className="flex items-center gap-3 font-mono text-sm">
                              <span className="font-medium text-gray-900">{key}</span>
                              <span className="text-gray-400">=</span>
                              <span className="text-primary-600">
                                {typeof value === 'object' ? JSON.stringify(value) : String(value)}
                              </span>
                            </div>
                            <button
                              onClick={() => handleRemoveParameter(key)}
                              className="text-gray-400 hover:text-red-600 transition-colors"
                            >
                              <X className="w-4 h-4" />
                            </button>
                          </div>
                        ))
                      ) : (
                        <div className="text-center py-6 text-gray-500 bg-gray-50 rounded-lg">
                          <AlertCircle className="w-6 h-6 mx-auto mb-2 text-gray-300" />
                          <p className="text-sm">No parameters defined</p>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              )}

              {/* Variables Tab */}
              {activeTab === 'variables' && (
                <div>
                  <div className="flex items-center justify-between mb-4">
                    <div>
                      <h3 className="font-semibold text-gray-900">Facter Variables</h3>
                      <p className="text-sm text-gray-600">
                        Variables that will be exported as external facts (key =&gt; value)
                      </p>
                    </div>
                    <button
                      onClick={() => setIsAddVarOpen(true)}
                      className="btn btn-secondary text-sm flex items-center"
                    >
                      <Plus className="w-4 h-4 mr-1" />
                      Add Variable
                    </button>
                  </div>

                  {isAddVarOpen && (
                    <div className="bg-gray-50 rounded-lg p-4 mb-4 border border-gray-200">
                      <form onSubmit={handleAddVariable} className="space-y-4">
                        <div className="grid grid-cols-2 gap-4">
                          <div>
                            <label className="label">Key (fact name)</label>
                            <input
                              type="text"
                              value={newVarKey}
                              onChange={(e) => setNewVarKey(e.target.value)}
                              className="input"
                              placeholder="e.g., role, datacenter, tier"
                              required
                            />
                          </div>
                          <div>
                            <label className="label">Value (JSON or string)</label>
                            <input
                              type="text"
                              value={newVarValue}
                              onChange={(e) => setNewVarValue(e.target.value)}
                              className="input"
                              placeholder='e.g., webserver, ["a", "b"]'
                              required
                            />
                          </div>
                        </div>
                        <div className="flex justify-end gap-2">
                          <button
                            type="button"
                            onClick={() => {
                              setIsAddVarOpen(false);
                              setNewVarKey('');
                              setNewVarValue('');
                            }}
                            className="btn btn-secondary text-sm"
                          >
                            Cancel
                          </button>
                          <button type="submit" className="btn btn-primary text-sm">
                            Add Variable
                          </button>
                        </div>
                      </form>
                    </div>
                  )}

                  <div className="space-y-2">
                    {selectedGroup.variables && Object.keys(selectedGroup.variables as Record<string, unknown>).length > 0 ? (
                      Object.entries(selectedGroup.variables as Record<string, unknown>).map(([key, value]) => (
                        <div
                          key={key}
                          className="flex items-center justify-between bg-white border border-gray-200 rounded-lg px-4 py-3"
                        >
                          <div className="flex items-center gap-3 font-mono text-sm">
                            <Variable className="w-4 h-4 text-green-500" />
                            <span className="font-medium text-gray-900">{key}</span>
                            <span className="text-gray-400">=&gt;</span>
                            <span className="text-green-600">
                              {typeof value === 'object' ? JSON.stringify(value) : String(value)}
                            </span>
                          </div>
                          <button
                            onClick={() => handleRemoveVariable(key)}
                            className="text-gray-400 hover:text-red-600 transition-colors"
                          >
                            <X className="w-4 h-4" />
                          </button>
                        </div>
                      ))
                    ) : (
                      <div className="text-center py-8 text-gray-500 bg-gray-50 rounded-lg">
                        <Variable className="w-8 h-8 mx-auto mb-2 text-gray-300" />
                        <p>No variables defined</p>
                        <p className="text-sm mt-1">Add variables to export as external facts via Facter</p>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="card flex items-center justify-center h-96">
              <div className="text-center text-gray-500">
                <FolderTree className="w-12 h-12 mx-auto mb-4 text-gray-300" />
                <p className="font-medium">Select a group to view details</p>
                <p className="text-sm mt-1">Or create a new group to get started</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
