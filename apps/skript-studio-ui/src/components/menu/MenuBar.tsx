/**
 * Top menu bar — File / Edit / View / Help dropdown menus.
 *
 * Uses Shadcn dropdown-menu. Holds the window title (draggable region
 * for Tauri) and brand mark.
 */

import { useState, type ReactNode } from "react";
import {
  FolderOpen,
  FilePlus2,
  Save,
  Settings as SettingsIcon,
  RefreshCw,
  Square,
  Minus,
  X,
  Sparkles,
  Github,
  HelpCircle,
  Eye,
  PanelLeft,
  PanelRight,
  PanelBottom,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
} from "@/components/ui/dropdown-menu";
import { useEditorStore } from "@/stores/editor-store";
import { useTauriFs } from "@/hooks/use-tauri-fs";
import { cn } from "@/lib/utils";

interface MenuBarProps {
  onOpenSettings: () => void;
  onToggleLeftPanel: () => void;
  onToggleRightPanel: () => void;
  onToggleBottomPanel: () => void;
}

export function MenuBar({
  onOpenSettings,
  onToggleLeftPanel,
  onToggleRightPanel,
  onToggleBottomPanel,
}: MenuBarProps) {
  const fs = useTauriFs();
  const activeTab = useEditorStore((s) => s.activeTab);
  const openFiles = useEditorStore((s) => s.openFiles);
  const setProjectRoot = useEditorStore((s) => s.setProjectRoot);
  const markClean = useEditorStore((s) => s.markClean);
  const activeFile = openFiles.find((f) => f.path === activeTab) ?? null;

  const handleOpenFolder = async () => {
    try {
      const path = await fs.openFolder();
      if (path) setProjectRoot(path);
    } catch (err) {
      console.error(err);
    }
  };

  const handleSave = async () => {
    if (!activeFile) return;
    try {
      await fs.writeFile(activeFile.path, activeFile.content);
      markClean(activeFile.path);
    } catch (err) {
      console.error("Save failed:", err);
    }
  };

  return (
    <header className="flex items-center h-8 bg-card border-b border-border select-none drag-region shrink-0">
      {/* Brand */}
      <div className="flex items-center gap-1.5 px-3 no-drag">
        <Sparkles className="h-3.5 w-3.5 text-primary" />
        <span className="text-xs font-semibold tracking-tight">
          Skript<span className="text-primary">Studio</span>
        </span>
      </div>

      {/* Menu items */}
      <nav className="flex items-center h-full no-drag">
        <MenuItem label="File">
          <DropdownMenuItem onClick={handleOpenFolder}>
            <FolderOpen className="h-3.5 w-3.5" />
            Open Folder…
            <DropdownMenuShortcut>⌘O</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem>
            <FilePlus2 className="h-3.5 w-3.5" />
            New File…
            <DropdownMenuShortcut>⌘N</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            onClick={handleSave}
            disabled={!activeFile}
          >
            <Save className="h-3.5 w-3.5" />
            Save
            <DropdownMenuShortcut>⌘S</DropdownMenuShortcut>
          </DropdownMenuItem>
        </MenuItem>

        <MenuItem label="Edit">
          <DropdownMenuItem onClick={() => document.execCommand("undo")}>
            Undo
            <DropdownMenuShortcut>⌘Z</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => document.execCommand("redo")}>
            Redo
            <DropdownMenuShortcut>⇧⌘Z</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem>
            Find
            <DropdownMenuShortcut>⌘F</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem>
            Replace
            <DropdownMenuShortcut>⌥⌘F</DropdownMenuShortcut>
          </DropdownMenuItem>
        </MenuItem>

        <MenuItem label="View">
          <DropdownMenuItem onClick={onToggleLeftPanel}>
            <PanelLeft className="h-3.5 w-3.5" />
            Toggle Explorer
            <DropdownMenuShortcut>⌘B</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onToggleRightPanel}>
            <PanelRight className="h-3.5 w-3.5" />
            Toggle Docs
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onToggleBottomPanel}>
            <PanelBottom className="h-3.5 w-3.5" />
            Toggle Output
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem>
            <RefreshCw className="h-3.5 w-3.5" />
            Reload LSP
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onOpenSettings}>
            <SettingsIcon className="h-3.5 w-3.5" />
            Settings
          </DropdownMenuItem>
        </MenuItem>

        <MenuItem label="Help">
          <DropdownMenuItem>
            <HelpCircle className="h-3.5 w-3.5" />
            Documentation
          </DropdownMenuItem>
          <DropdownMenuItem>
            <Github className="h-3.5 w-3.5" />
            View on GitHub
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem>
            <Eye className="h-3.5 w-3.5" />
            About Skript Studio
          </DropdownMenuItem>
        </MenuItem>
      </nav>

      {/* Center: active file indicator */}
      <div className="flex-1 flex items-center justify-center no-drag">
        {activeFile && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="font-mono">{activeFile.path}</span>
            {activeFile.isDirty && (
              <span className="w-1.5 h-1.5 rounded-full bg-primary" />
            )}
          </div>
        )}
      </div>

      {/* Right: window controls */}
      <div className="flex items-center no-drag">
        <button className="inline-flex items-center justify-center h-8 w-11 hover:bg-accent/10 transition-colors">
          <Minus className="h-3.5 w-3.5" />
        </button>
        <button className="inline-flex items-center justify-center h-8 w-11 hover:bg-accent/10 transition-colors">
          <Square className="h-2.5 w-2.5" />
        </button>
        <button className="inline-flex items-center justify-center h-8 w-11 hover:bg-destructive hover:text-white transition-colors">
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
    </header>
  );
}

function MenuItem({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <button
          className={cn(
            "px-2.5 h-8 text-xs font-medium text-muted-foreground hover:bg-accent/10 hover:text-foreground transition-colors",
            open && "bg-accent/10 text-foreground",
          )}
        >
          {label}
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        {children}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
