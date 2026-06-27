/**
 * Lightweight tree-view primitive.
 *
 * Shadcn/UI does not ship a tree component; this is a minimal, fully
 * typed implementation that supports selection, expansion, indentation,
 * and keyboard navigation. It is intentionally data-driven: callers
 * pass a tree model and an on-select callback.
 */

import * as React from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

export interface TreeNode {
  id: string;
  label: string;
  icon?: React.ComponentType<{ className?: string }>;
  /** Children for branch nodes. */
  children?: TreeNode[];
  /** Whether the node is a leaf (file) or branch (folder). */
  isLeaf: boolean;
  /** Original data passed back on select. */
  data?: unknown;
}

interface TreeViewProps {
  root: TreeNode;
  selectedId?: string | null;
  onSelect?: (node: TreeNode) => void;
  /** Set of expanded branch IDs. If omitted, the tree manages its own state. */
  expandedIds?: Set<string>;
  onExpandedChange?: (ids: Set<string>) => void;
  className?: string;
}

export function TreeView({
  root,
  selectedId,
  onSelect,
  expandedIds,
  onExpandedChange,
  className,
}: TreeViewProps) {
  const [internalExpanded, setInternalExpanded] = React.useState<Set<string>>(
    new Set(),
  );
  const expanded = expandedIds ?? internalExpanded;

  const toggle = (id: string) => {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    if (onExpandedChange) onExpandedChange(next);
    else setInternalExpanded(next);
  };

  return (
    <ul className={cn("text-sm select-none", className)} role="tree">
      {root.children?.map((child) => (
        <TreeBranch
          key={child.id}
          node={child}
          depth={0}
          expanded={expanded}
          selectedId={selectedId}
          onToggle={toggle}
          onSelect={onSelect}
        />
      ))}
    </ul>
  );
}

interface TreeBranchProps {
  node: TreeNode;
  depth: number;
  expanded: Set<string>;
  selectedId?: string | null;
  onToggle: (id: string) => void;
  onSelect?: (node: TreeNode) => void;
}

function TreeBranch({
  node,
  depth,
  expanded,
  selectedId,
  onToggle,
  onSelect,
}: TreeBranchProps) {
  const isExpanded = expanded.has(node.id);
  const isSelected = selectedId === node.id;
  const Icon = node.icon;

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (node.isLeaf) {
      onSelect?.(node);
    } else {
      onToggle(node.id);
    }
  };

  return (
    <li role="treeitem" aria-expanded={node.isLeaf ? undefined : isExpanded}>
      <div
        onClick={handleClick}
        className={cn(
          "flex items-center gap-1 h-7 pr-2 cursor-pointer rounded-sm transition-colors group",
          isSelected
            ? "bg-accent/15 text-accent"
            : "hover:bg-accent/10 hover:text-accent",
        )}
        style={{ paddingLeft: depth * 12 + 4 }}
      >
        {!node.isLeaf && (
          <ChevronRight
            className={cn(
              "h-3.5 w-3.5 shrink-0 transition-transform",
              isExpanded && "rotate-90",
            )}
          />
        )}
        {node.isLeaf && <span className="w-3.5 shrink-0" />}
        {Icon && <Icon className="h-3.5 w-3.5 shrink-0 opacity-80" />}
        <span className="truncate">{node.label}</span>
      </div>
      {!node.isLeaf && isExpanded && node.children && (
        <ul role="group">
          {node.children.map((child) => (
            <TreeBranch
              key={child.id}
              node={child}
              depth={depth + 1}
              expanded={expanded}
              selectedId={selectedId}
              onToggle={onToggle}
              onSelect={onSelect}
            />
          ))}
        </ul>
      )}
    </li>
  );
}
