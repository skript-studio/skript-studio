/**
 * Editor tab bar. Shows one tab per open file with a close button and
 * an unsaved-changes dot. Supports click-to-switch, middle-click to
 * close, and drag-to-reorder (planned for v2).
 */

import { X, Circle } from "lucide-react";
import { useEditorStore, tabDisplayName } from "@/stores/editor-store";
import { cn, dirname, basename } from "@/lib/utils";
import { FileCode2 } from "lucide-react";

export function EditorTabs() {
  const openFiles = useEditorStore((s) => s.openFiles);
  const activeTab = useEditorStore((s) => s.activeTab);
  const setActiveTab = useEditorStore((s) => s.setActiveTab);
  const closeTab = useEditorStore((s) => s.closeTab);

  if (openFiles.length === 0) return null;

  return (
    <div
      className="flex items-stretch h-9 bg-card border-b border-border overflow-x-auto scrollbar-thin shrink-0 select-none"
      role="tablist"
    >
      {openFiles.map((file) => {
        const isActive = file.path === activeTab;
        const dir = dirname(file.path);
        return (
          <div
            key={file.path}
            role="tab"
            aria-selected={isActive}
            tabIndex={0}
            onClick={() => setActiveTab(file.path)}
            onMouseDown={(e) => {
              if (e.button === 1) {
                // Middle-click closes the tab
                e.preventDefault();
                closeTab(file.path);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") setActiveTab(file.path);
              if (e.key === "Delete" && e.metaKey) closeTab(file.path);
            }}
            className={cn(
              "group relative flex items-center gap-2 pl-3 pr-2 border-r border-border cursor-pointer transition-colors min-w-0 max-w-[240px]",
              isActive
                ? "bg-background text-foreground"
                : "bg-card text-muted-foreground hover:bg-accent/10",
            )}
          >
            {isActive && (
              <span className="absolute top-0 left-0 right-0 h-0.5 bg-primary" />
            )}
            <FileCode2
              className={cn(
                "h-3.5 w-3.5 shrink-0",
                isActive ? "text-primary" : "text-muted-foreground",
              )}
            />
            <div className="flex flex-col min-w-0 py-1">
              <span className="text-xs truncate font-medium">
                {basename(file.path)}
              </span>
              {dir && (
                <span className="text-2xs text-muted-foreground truncate -mt-0.5">
                  {dir}
                </span>
              )}
            </div>
            <button
              onClick={(e) => {
                e.stopPropagation();
                closeTab(file.path);
              }}
              className={cn(
                "shrink-0 rounded p-0.5 transition-opacity",
                "hover:bg-accent hover:text-accent-foreground",
                file.isDirty
                  ? "opacity-100"
                  : "opacity-0 group-hover:opacity-100",
              )}
              aria-label="Close tab"
            >
              {file.isDirty ? (
                <Circle className="h-2.5 w-2.5 fill-current" />
              ) : (
                <X className="h-3.5 w-3.5" />
              )}
            </button>
          </div>
        );
      })}
    </div>
  );
}

void tabDisplayName; // kept for future use
