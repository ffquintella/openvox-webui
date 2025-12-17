import { useMemo } from 'react';
import {
  Treemap,
  ResponsiveContainer,
  Tooltip,
} from 'recharts';
import type { NodeGroup } from '../../types';

interface GroupMembershipChartProps {
  groups: NodeGroup[];
  title?: string;
}

interface TreemapNode {
  name: string;
  size: number;
  children?: TreemapNode[];
  fill?: string;
}

const COLORS = [
  '#3b82f6', // blue
  '#22c55e', // green
  '#f59e0b', // amber
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#06b6d4', // cyan
  '#f97316', // orange
  '#84cc16', // lime
];

function CustomContent(props: {
  x: number;
  y: number;
  width: number;
  height: number;
  name: string;
  size: number;
  fill: string;
}) {
  const { x, y, width, height, name, size, fill } = props;

  if (width < 40 || height < 30) {
    return (
      <g>
        <rect
          x={x}
          y={y}
          width={width}
          height={height}
          fill={fill}
          stroke="#fff"
          strokeWidth={2}
          rx={4}
        />
      </g>
    );
  }

  return (
    <g>
      <rect
        x={x}
        y={y}
        width={width}
        height={height}
        fill={fill}
        stroke="#fff"
        strokeWidth={2}
        rx={4}
      />
      <text
        x={x + width / 2}
        y={y + height / 2 - 8}
        textAnchor="middle"
        fill="#fff"
        fontSize={12}
        fontWeight="600"
      >
        {name.length > 15 ? `${name.slice(0, 12)}...` : name}
      </text>
      <text
        x={x + width / 2}
        y={y + height / 2 + 8}
        textAnchor="middle"
        fill="rgba(255,255,255,0.8)"
        fontSize={10}
      >
        {size} nodes
      </text>
    </g>
  );
}

function CustomTooltip({ active, payload }: { active?: boolean; payload?: Array<{ payload: TreemapNode }> }) {
  if (!active || !payload || !payload.length) return null;

  const data = payload[0].payload;
  return (
    <div className="bg-white shadow-lg rounded-lg p-3 border border-gray-200">
      <p className="font-semibold text-gray-900">{data.name}</p>
      <p className="text-sm text-gray-600">{data.size} pinned nodes</p>
    </div>
  );
}

export default function GroupMembershipChart({ groups, title = 'Node Group Membership' }: GroupMembershipChartProps) {
  const treemapData = useMemo(() => {
    // Build hierarchy based on parent relationships
    const rootGroups = groups.filter((g) => !g.parent_id);
    const childrenMap = new Map<string, NodeGroup[]>();

    groups.forEach((group) => {
      if (group.parent_id) {
        const children = childrenMap.get(group.parent_id) || [];
        children.push(group);
        childrenMap.set(group.parent_id, children);
      }
    });

    function buildNode(group: NodeGroup, colorIndex: number): TreemapNode {
      const children = childrenMap.get(group.id) || [];
      const nodeCount = group.pinned_nodes.length + (group.rules.length > 0 ? 1 : 0); // Estimate

      if (children.length > 0) {
        return {
          name: group.name,
          size: nodeCount,
          fill: COLORS[colorIndex % COLORS.length],
          children: children.map((child, i) => buildNode(child, colorIndex + i + 1)),
        };
      }

      return {
        name: group.name,
        size: Math.max(nodeCount, 1), // At least 1 for visibility
        fill: COLORS[colorIndex % COLORS.length],
      };
    }

    const data: TreemapNode[] = rootGroups.map((group, i) => buildNode(group, i));

    return {
      name: 'Groups',
      children: data.length > 0 ? data : [{ name: 'No groups', size: 1, fill: '#e5e7eb' }],
    };
  }, [groups]);

  const totalNodes = groups.reduce((sum, g) => sum + g.pinned_nodes.length, 0);

  return (
    <div className="w-full">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold text-gray-900">{title}</h3>
        <span className="text-sm text-gray-500">
          {groups.length} groups | {totalNodes} pinned nodes
        </span>
      </div>

      {groups.length === 0 ? (
        <div className="h-64 flex items-center justify-center text-gray-500">
          <p>No groups configured</p>
        </div>
      ) : (
        <div className="h-64">
          <ResponsiveContainer width="100%" height="100%">
            <Treemap
              data={treemapData.children}
              dataKey="size"
              aspectRatio={4 / 3}
              stroke="#fff"
              content={<CustomContent x={0} y={0} width={0} height={0} name="" size={0} fill="" />}
            >
              <Tooltip content={<CustomTooltip />} />
            </Treemap>
          </ResponsiveContainer>
        </div>
      )}

      {/* Group List */}
      <div className="mt-4 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
        {groups.slice(0, 8).map((group, i) => (
          <div key={group.id} className="flex items-center gap-2 text-sm">
            <div
              className="w-3 h-3 rounded-sm flex-shrink-0"
              style={{ backgroundColor: COLORS[i % COLORS.length] }}
            />
            <span className="truncate text-gray-700">{group.name}</span>
            <span className="text-gray-400 text-xs">({group.pinned_nodes.length})</span>
          </div>
        ))}
        {groups.length > 8 && (
          <div className="text-sm text-gray-500">+{groups.length - 8} more</div>
        )}
      </div>
    </div>
  );
}
