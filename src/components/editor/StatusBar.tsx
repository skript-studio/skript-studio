/**
 * Bottom status bar — shows LSP status, cursor position, language, encoding.
 */

import {
  Circle,
  Loader2,
  AlertTriangle,
  Check,
  RefreshCw,
  MessageSquareWarning,
} from "lucide-react";
import { useLSPStore } from "@/stores/lsp-store";
import { useEditorStore } from "@/stores/editor-store";
import { useLSP } from "@/hooks/use-lsp";
import type * as Monaco from "monaco-editor";
import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

interface StatusBarProps {
  monaco: typeof Monaco | null;
}

export function StatusBar({ monaco }: StatusBarProps) {
  const lspStatus = useLSPStore((s) => s.status);
  const lspPort = useLSPStore((s) => s.port);
  const diagnosticsByFile = useLSPStore((s) => s.diagnosticsByFile);
  const activeTab = useEditorStore((s) => s.activeTab);
  const openFiles = useEditorStore((s) => s.openFiles);
  const { restart } = useLSP({ monaco });

  const [cursor, setCursor] = useState<{ line: number; col: number }>({
    line: 1,
    col: 1,
  });

  const activeFile = openFiles.find((f) => f.path === activeTab);
  const activeFileDiagnostics = activeTab
    ? (diagnosticsByFile.get(activeTab) ?? [])
    : [];
  const totalIssues = Array.from(diagnosticsByFile.values()).reduce(
    (n, ds) => n + ds.length,
    0,
  );

  // Subscribe to cursor position from Monaco
  useEffect(() => {
    if (!monaco) return;
    const editor = monaco.editor.getEditors()[0];
    if (!editor) return;
    const d = editor.onDidChangeCursorPosition((e) =>
      setCursor({ line: e.position.lineNumber, col: e.position.column }),
    );
    return () => d.dispose();
  }, [monaco, activeTab]);

  const statusDisplay = {
    connected: {
      icon: Check,
      label: "LSP Connected",
      color: "hover:bg-emerald-600",
    },
    starting: {
      icon: Loader2,
      label: "Starting LSP…",
      color: "hover:bg-amber-600",
      spin: true,
    },
    disconnected: {
      icon: Circle,
      label: "LSP Off",
      color: "hover:bg-zinc-600",
    },
    error: {
      icon: AlertTriangle,
      label: "LSP Error",
      color: "hover:bg-destructive",
    },
  }[lspStatus];

  const StatusIcon = statusDisplay.icon;

  return (
    <footer
      className={cn(
        "flex items-center justify-between h-6 bg-statusbar text-statusbar-foreground text-2xs font-medium shrink-0 select-none",
      )}
    >
      {/* Left */}
      <div className="flex items-center h-full">
        <button
          onClick={() => void restart()}
          className="flex items-center gap-1.5 px-3 h-full hover:bg-primary/80 transition-colors"
          title="Restart LSP"
        >
          <StatusIcon
            className={cn("h-3 w-3", statusDisplay.spin && "animate-spin")}
          />
          {statusDisplay.label}
          {lspPort != null && lspStatus !== "disconnected" && (
            <span className="opacity-70">:{lspPort}</span>
          )}
        </button>
        <button
          className="flex items-center gap-1.5 px-3 h-full hover:bg-primary/80 transition-colors"
          title="Reload"
        >
          <RefreshCw className="h-3 w-3" />
          Reload
        </button>
        <button
          className="flex items-center gap-1.5 px-3 h-full hover:bg-primary/80 transition-colors"
          title={`${totalIssues} problem${totalIssues === 1 ? "" : "s"} across all open files`}
        >
          <MessageSquareWarning className="h-3 w-3" />
          {totalIssues === 0 ? "No issues" : `${totalIssues} issue${totalIssues === 1 ? "" : "s"}`}
        </button>
      </div>

      {/* Right */}
      <div className="flex items-center h-full">
        {activeFile && (
          <>
            <span className="px-3 flex items-center gap-1.5">
              <Circle
                className={cn(
                  "h-2 w-2 fill-current",
                  activeFile.isDirty ? "text-white" : "text-white/40",
                )}
              />
              {activeFile.isDirty ? "Unsaved" : "Saved"}
            </span>
            {activeFileDiagnostics.length > 0 && (
              <span
                className="px-3 flex items-center gap-1.5"
                title={`${activeFileDiagnostics.length} problem${activeFileDiagnostics.length === 1 ? "" : "s"} in this file`}
              >
                <AlertTriangle className="h-3 w-3" />
                {activeFileDiagnostics.length}
              </span>
            )}
            <span className="px-3">
              Ln {cursor.line}, Col {cursor.col}
            </span>
            <span className="px-3">UTF-8</span>
            <span className="px-3">LF</span>
            <span className="px-3">Skript</span>
            <span className="px-3">
              {activeFile.content.split("\n").length} lines
            </span>
          </>
        )}
      </div>
    </footer>
  );
}
