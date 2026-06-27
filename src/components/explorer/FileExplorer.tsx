/**
 * File explorer panel — left side of the IDE.
 *
 * Shows a tree of the currently-open project folder. Clicking a `.sk`
 * file opens it in the editor. Uses `useTauriFs` for all FS operations.
 *
 * When no project folder is open, shows an "Open Folder" prompt.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  FolderOpen,
  RefreshCw,
  FilePlus2,
  ChevronRight,
  ChevronDown,
  FileCode2,
  Folder,
  FolderTree,
} from "lucide-react";
import { useEditorStore } from "@/stores/editor-store";
import { useTauriFs } from "@/hooks/use-tauri-fs";
import { cn, basename, extname, joinPath } from "@/lib/utils";
import type { DirEntry } from "@/lib/tauri-commands";
import { NewFileDialog } from "./NewFileDialog";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import { ScrollArea } from "@/components/ui/scroll-area";

interface TreeNode {
  name: string;
  path: string;
  isDir: boolean;
  children?: TreeNode[];
  loaded?: boolean;
}

export function FileExplorer() {
  const fs = useTauriFs();
  const projectRoot = useEditorStore((s) => s.projectRoot);
  const setProjectRoot = useEditorStore((s) => s.setProjectRoot);
  const openFile = useEditorStore((s) => s.openFile);
  const activeTab = useEditorStore((s) => s.activeTab);

  const [tree, setTree] = useState<TreeNode | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [newFileDialog, setNewFileDialog] = useState<{
    open: boolean;
    parent: string | null;
  }>({ open: false, parent: null });

  const handleOpenFolder = async () => {
    try {
      const path = await fs.openFolder();
      if (path) {
        setProjectRoot(path);
        setExpanded(new Set([path]));
      }
    } catch (err) {
      console.error("Failed to open folder:", err);
    }
  };

  const refresh = useCallback(async () => {
    if (!projectRoot) return;
    try {
      const entries = await fs.listDir(projectRoot);
      const node = buildTreeNode(projectRoot, basename(projectRoot), entries);
      setTree(node);
    } catch (err) {
      console.error("Failed to list dir:", err);
    }
  }, [projectRoot, fs]);

  // Refresh when project root changes
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const loadChildren = async (node: TreeNode): Promise<TreeNode> => {
    if (node.loaded) return node;
    try {
      const entries = await fs.listDir(node.path);
      node.children = entries
        .filter((e) => e.is_dir || e.extension === "sk")
        .map((e) => ({
          name: e.name,
          path: e.path,
          isDir: e.is_dir,
        }))
        .sort((a, b) => {
          if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
          return a.name.localeCompare(b.name);
        });
      node.loaded = true;
    } catch (err) {
      console.error("Failed to load children:", err);
    }
    return node;
  };

  const toggle = async (path: string) => {
    const next = new Set(expanded);
    if (next.has(path)) {
      next.delete(path);
    } else {
      next.add(path);
      // Lazy-load children
      const node = findNode(tree, path);
      if (node && !node.loaded) {
        await loadChildren(node);
        setTree({ ...tree! });
      }
    }
    setExpanded(next);
  };

  const handleOpen = async (path: string) => {
    try {
      const content = await fs.readFile(path);
      openFile(path, content);
    } catch (err) {
      console.error("Failed to read file:", err);
    }
  };

  const handleCreateFile = async (name: string) => {
    const parent = newFileDialog.parent ?? projectRoot;
    if (!parent) return;
    const fullPath = joinPath(parent, name.endsWith(".sk") ? name : `${name}.sk`);
    try {
      await fs.createFile(fullPath);
      await refresh();
      // Auto-open
      await handleOpen(fullPath);
    } catch (err) {
      console.error("Failed to create file:", err);
    }
  };

  const handleDelete = async (path: string) => {
    try {
      await fs.deleteFile(path);
      await refresh();
    } catch (err) {
      console.error("Failed to delete file:", err);
    }
  };

  // Browser dev fallback: show prompt
  if (!fs.available) {
    return (
      <div className="flex flex-col h-full">
        <Header />
        <div className="flex-1 flex flex-col items-center justify-center p-6 text-center gap-3">
          <FolderTree className="h-8 w-8 text-muted-foreground opacity-50" />
          <div className="space-y-1">
            <p className="text-sm font-medium">Tauri backend unavailable</p>
            <p className="text-xs text-muted-foreground">
              Run this app via <code className="font-mono">bun tauri dev</code>{" "}
              to enable the file explorer.
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-sidebar text-sidebar-foreground">
      <Header
        onOpenFolder={handleOpenFolder}
        onRefresh={refresh}
        onNewFile={() =>
          setNewFileDialog({ open: true, parent: projectRoot })
        }
      />

      {!projectRoot ? (
        <div className="flex-1 flex flex-col items-center justify-center p-6 text-center gap-3">
          <FolderOpen className="h-8 w-8 text-muted-foreground opacity-50" />
          <div className="space-y-1">
            <p className="text-sm font-medium">No folder open</p>
            <p className="text-xs text-muted-foreground">
              Open a Skript scripts folder to start editing.
            </p>
          </div>
          <button
            onClick={handleOpenFolder}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-primary text-primary-foreground text-xs font-medium hover:bg-primary/90 transition-colors"
          >
            <FolderOpen className="h-3.5 w-3.5" />
            Open Folder
          </button>
        </div>
      ) : (
        <ScrollArea className="flex-1">
          <div className="py-1">
            <div className="flex items-center gap-1 px-2 py-1 text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
              {basename(projectRoot)}
            </div>
            {tree?.children?.map((child) => (
              <TreeRow
                key={child.path}
                node={child}
                depth={0}
                expanded={expanded}
                activePath={activeTab}
                onToggle={toggle}
                onOpen={handleOpen}
                onDelete={handleDelete}
                onNewFile={(parent) =>
                  setNewFileDialog({ open: true, parent })
                }
              />
            ))}
          </div>
        </ScrollArea>
      )}

      <NewFileDialog
        open={newFileDialog.open}
        onOpenChange={(open) =>
          setNewFileDialog({ open, parent: newFileDialog.parent })
        }
        onCreate={handleCreateFile}
        parent={newFileDialog.parent ?? ""}
      />
    </div>
  );
}

function Header({
  onOpenFolder,
  onRefresh,
  onNewFile,
}: {
  onOpenFolder?: () => void;
  onRefresh?: () => void;
  onNewFile?: () => void;
}) {
  return (
    <div className="panel-section flex items-center justify-between">
      <span>Explorer</span>
      <div className="flex items-center gap-0.5">
        {onNewFile && (
          <button
            onClick={onNewFile}
            title="New file"
            className="inline-flex items-center justify-center h-6 w-6 rounded text-muted-foreground hover:bg-accent hover:text-accent-foreground transition-colors"
          >
            <FilePlus2 className="h-3.5 w-3.5" />
          </button>
        )}
        {onOpenFolder && (
          <button
            onClick={onOpenFolder}
            title="Open folder"
            className="inline-flex items-center justify-center h-6 w-6 rounded text-muted-foreground hover:bg-accent hover:text-accent-foreground transition-colors"
          >
            <FolderOpen className="h-3.5 w-3.5" />
          </button>
        )}
        {onRefresh && (
          <button
            onClick={onRefresh}
            title="Refresh"
            className="inline-flex items-center justify-center h-6 w-6 rounded text-muted-foreground hover:bg-accent hover:text-accent-foreground transition-colors"
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
    </div>
  );
}

interface TreeRowProps {
  node: TreeNode;
  depth: number;
  expanded: Set<string>;
  activePath: string | null;
  onToggle: (path: string) => void;
  onOpen: (path: string) => void;
  onDelete: (path: string) => void;
  onNewFile: (parent: string) => void;
}

function TreeRow({
  node,
  depth,
  expanded,
  activePath,
  onToggle,
  onOpen,
  onDelete,
  onNewFile,
}: TreeRowProps) {
  const isExpanded = expanded.has(node.path);
  const isActive = activePath === node.path;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          onClick={() => (node.isDir ? onToggle(node.path) : onOpen(node.path))}
          className={cn(
            "flex items-center gap-1 h-7 cursor-pointer rounded-sm transition-colors group",
            isActive
              ? "bg-primary/15 text-primary"
              : "hover:bg-accent/10 hover:text-accent",
          )}
          style={{ paddingLeft: depth * 12 + 4, paddingRight: 8 }}
        >
          {node.isDir ? (
            isExpanded ? (
              <ChevronDown className="h-3.5 w-3.5 shrink-0" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 shrink-0" />
            )
          ) : (
            <span className="w-3.5 shrink-0" />
          )}
          {node.isDir ? (
            <Folder className="h-3.5 w-3.5 shrink-0 text-amber-500" />
          ) : (
            <FileCode2
              className={cn(
                "h-3.5 w-3.5 shrink-0",
                isActive ? "text-primary" : "text-muted-foreground",
              )}
            />
          )}
          <span className="truncate text-sm">{node.name}</span>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        {node.isDir ? (
          <>
            <ContextMenuItem onClick={() => onNewFile(node.path)}>
              <FilePlus2 className="h-3.5 w-3.5" />
              New Skript file…
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem onClick={() => onToggle(node.path)}>
              {isExpanded ? "Collapse" : "Expand"}
            </ContextMenuItem>
          </>
        ) : (
          <>
            <ContextMenuItem onClick={() => onOpen(node.path)}>
              <FileCode2 className="h-3.5 w-3.5" />
              Open
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem
              onClick={() => onDelete(node.path)}
              className="text-destructive focus:text-destructive"
            >
              Delete
            </ContextMenuItem>
          </>
        )}
      </ContextMenuContent>
      {node.isDir && isExpanded && node.children && (
        <div>
          {node.children.map((child) => (
            <TreeRow
              key={child.path}
              node={child}
              depth={depth + 1}
              expanded={expanded}
              activePath={activePath}
              onToggle={onToggle}
              onOpen={onOpen}
              onDelete={onDelete}
              onNewFile={onNewFile}
            />
          ))}
        </div>
      )}
    </ContextMenu>
  );
}

// --- Helpers --------------------------------------------------------------

function buildTreeNode(
  path: string,
  name: string,
  entries: DirEntry[],
): TreeNode {
  return {
    name,
    path,
    isDir: true,
    loaded: true,
    children: entries
      .filter((e) => e.is_dir || e.extension === "sk")
      .map((e) => ({
        name: e.name,
        path: e.path,
        isDir: e.is_dir,
        loaded: !e.is_dir,
      }))
      .sort((a, b) => {
        if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
        return a.name.localeCompare(b.name);
      }),
  };
}

function findNode(node: TreeNode | null, path: string): TreeNode | null {
  if (!node) return null;
  if (node.path === path) return node;
  if (!node.children) return null;
  for (const child of node.children) {
    const found = findNode(child, path);
    if (found) return found;
  }
  return null;
}

// Use imports to silence linters when extname isn't used elsewhere
void extname;
