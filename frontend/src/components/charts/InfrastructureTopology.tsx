import { useMemo, useState } from 'react';
import { Server, Folder, ChevronDown, ChevronRight, Circle } from 'lucide-react';
import type { Node, NodeGroup } from '../../types';

interface InfrastructureTopologyProps {
  nodes: Node[];
  groups: NodeGroup[];
  title?: string;
}

interface TreeNode {
  id: string;
  name: string;
  type: 'environment' | 'group' | 'node';
  children: TreeNode[];
  status?: string;
  nodeCount?: number;
}

function getStatusColor(status: string | null | undefined): string {
  switch (status) {
    case 'changed':
      return 'text-success-500';
    case 'unchanged':
      return 'text-primary-500';
    case 'failed':
      return 'text-danger-500';
    default:
      return 'text-gray-400';
  }
}

function TreeNodeComponent({
  node,
  level = 0,
  expanded,
  onToggle,
}: {
  node: TreeNode;
  level?: number;
  expanded: Set<string>;
  onToggle: (id: string) => void;
}) {
  const isExpanded = expanded.has(node.id);
  const hasChildren = node.children.length > 0;

  return (
    <div>
      <div
        className={`flex items-center gap-2 py-1.5 px-2 rounded-lg hover:bg-gray-50 cursor-pointer transition-colors`}
        style={{ paddingLeft: `${level * 16 + 8}px` }}
        onClick={() => hasChildren && onToggle(node.id)}
      >
        {/* Expand/Collapse button */}
        <div className="w-4 h-4 flex items-center justify-center">
          {hasChildren ? (
            isExpanded ? (
              <ChevronDown className="w-4 h-4 text-gray-400" />
            ) : (
              <ChevronRight className="w-4 h-4 text-gray-400" />
            )
          ) : (
            <div className="w-4" />
          )}
        </div>

        {/* Icon */}
        {node.type === 'environment' && (
          <Folder className="w-4 h-4 text-amber-500" />
        )}
        {node.type === 'group' && (
          <Folder className="w-4 h-4 text-primary-500" />
        )}
        {node.type === 'node' && (
          <Server className={`w-4 h-4 ${getStatusColor(node.status)}`} />
        )}

        {/* Name */}
        <span className="text-sm text-gray-900 flex-1">{node.name}</span>

        {/* Node count badge */}
        {node.nodeCount !== undefined && node.nodeCount > 0 && (
          <span className="text-xs text-gray-500 bg-gray-100 px-2 py-0.5 rounded-full">
            {node.nodeCount}
          </span>
        )}

        {/* Status indicator for nodes */}
        {node.type === 'node' && (
          <Circle
            className={`w-2 h-2 fill-current ${getStatusColor(node.status)}`}
          />
        )}
      </div>

      {/* Children */}
      {isExpanded && hasChildren && (
        <div>
          {node.children.map((child) => (
            <TreeNodeComponent
              key={child.id}
              node={child}
              level={level + 1}
              expanded={expanded}
              onToggle={onToggle}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export default function InfrastructureTopology({
  nodes,
  groups,
  title = 'Infrastructure Topology',
}: InfrastructureTopologyProps) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set(['root']));
  const [viewMode, setViewMode] = useState<'environment' | 'group'>('environment');

  const tree = useMemo(() => {
    if (viewMode === 'environment') {
      // Group by environment
      const envMap = new Map<string, Node[]>();

      nodes.forEach((node) => {
        const env = node.catalog_environment || 'production';
        const envNodes = envMap.get(env) || [];
        envNodes.push(node);
        envMap.set(env, envNodes);
      });

      const envNodes: TreeNode[] = Array.from(envMap.entries()).map(([env, envNodes]) => ({
        id: `env-${env}`,
        name: env,
        type: 'environment' as const,
        nodeCount: envNodes.length,
        children: envNodes.slice(0, 20).map((node) => ({
          id: `node-${node.certname}`,
          name: node.certname,
          type: 'node' as const,
          status: node.latest_report_status || undefined,
          children: [],
        })),
      }));

      return {
        id: 'root',
        name: 'Infrastructure',
        type: 'environment' as const,
        nodeCount: nodes.length,
        children: envNodes,
      };
    } else {
      // Group by node groups
      const groupNodes: TreeNode[] = groups.map((group) => ({
        id: `group-${group.id}`,
        name: group.name,
        type: 'group' as const,
        nodeCount: group.pinned_nodes.length,
        children: group.pinned_nodes.slice(0, 10).map((certname) => {
          const node = nodes.find((n) => n.certname === certname);
          return {
            id: `node-${certname}`,
            name: certname,
            type: 'node' as const,
            status: node?.latest_report_status || undefined,
            children: [],
          };
        }),
      }));

      // Add ungrouped nodes
      const groupedCertnames = new Set(groups.flatMap((g) => g.pinned_nodes));
      const ungroupedNodes = nodes.filter((n) => !groupedCertnames.has(n.certname));

      if (ungroupedNodes.length > 0) {
        groupNodes.push({
          id: 'ungrouped',
          name: 'Ungrouped Nodes',
          type: 'group' as const,
          nodeCount: ungroupedNodes.length,
          children: ungroupedNodes.slice(0, 20).map((node) => ({
            id: `node-${node.certname}`,
            name: node.certname,
            type: 'node' as const,
            status: node.latest_report_status || undefined,
            children: [],
          })),
        });
      }

      return {
        id: 'root',
        name: 'Node Groups',
        type: 'group' as const,
        nodeCount: nodes.length,
        children: groupNodes,
      };
    }
  }, [nodes, groups, viewMode]);

  const handleToggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const expandAll = () => {
    const allIds = new Set<string>();
    const collectIds = (node: TreeNode) => {
      allIds.add(node.id);
      node.children.forEach(collectIds);
    };
    collectIds(tree);
    setExpanded(allIds);
  };

  const collapseAll = () => {
    setExpanded(new Set(['root']));
  };

  return (
    <div className="w-full">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold text-gray-900">{title}</h3>
        <div className="flex items-center gap-2">
          {/* View mode toggle */}
          <div className="flex rounded-lg border border-gray-300 overflow-hidden">
            <button
              onClick={() => setViewMode('environment')}
              className={`px-3 py-1 text-sm ${
                viewMode === 'environment'
                  ? 'bg-primary-500 text-white'
                  : 'bg-white text-gray-600 hover:bg-gray-50'
              }`}
            >
              By Environment
            </button>
            <button
              onClick={() => setViewMode('group')}
              className={`px-3 py-1 text-sm ${
                viewMode === 'group'
                  ? 'bg-primary-500 text-white'
                  : 'bg-white text-gray-600 hover:bg-gray-50'
              }`}
            >
              By Group
            </button>
          </div>

          {/* Expand/Collapse buttons */}
          <button
            onClick={expandAll}
            className="text-xs text-primary-600 hover:text-primary-700"
          >
            Expand All
          </button>
          <span className="text-gray-300">|</span>
          <button
            onClick={collapseAll}
            className="text-xs text-primary-600 hover:text-primary-700"
          >
            Collapse All
          </button>
        </div>
      </div>

      {/* Legend */}
      <div className="flex items-center gap-4 mb-4 text-xs text-gray-500">
        <div className="flex items-center gap-1">
          <Circle className="w-2 h-2 fill-success-500 text-success-500" />
          <span>Changed</span>
        </div>
        <div className="flex items-center gap-1">
          <Circle className="w-2 h-2 fill-primary-500 text-primary-500" />
          <span>Unchanged</span>
        </div>
        <div className="flex items-center gap-1">
          <Circle className="w-2 h-2 fill-danger-500 text-danger-500" />
          <span>Failed</span>
        </div>
        <div className="flex items-center gap-1">
          <Circle className="w-2 h-2 fill-gray-400 text-gray-400" />
          <span>Unreported</span>
        </div>
      </div>

      {/* Tree view */}
      <div className="border border-gray-200 rounded-lg max-h-96 overflow-y-auto">
        {nodes.length === 0 ? (
          <div className="p-8 text-center text-gray-500">
            <Server className="w-12 h-12 mx-auto mb-3 text-gray-300" />
            <p>No nodes available</p>
          </div>
        ) : (
          <div className="p-2">
            <TreeNodeComponent
              node={tree}
              expanded={expanded}
              onToggle={handleToggle}
            />
          </div>
        )}
      </div>

      {/* Summary */}
      <div className="mt-4 flex items-center gap-4 text-sm text-gray-500">
        <span>{nodes.length} total nodes</span>
        <span>{groups.length} groups</span>
        <span>
          {new Set(nodes.map((n) => n.catalog_environment || 'production')).size} environments
        </span>
      </div>
    </div>
  );
}
