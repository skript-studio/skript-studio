/**
 * Output panel — shows LSP logs in real time.
 *
 * Sits at the bottom of the editor area inside the resizable vertical
 * panel group. Logs come from the `useLSPStore` log buffer.
 *
 * Tabs:
 *  - "Output"  — raw LSP stderr/log lines
 *  - "Problems" — diagnostics pushed by the LSP, grouped by file,
 *    with friendly code labels (`unknown-syntax`, `removed-syntax`,
 *    `requires-plugin`, `type-mismatch-N`).
 */

import { useEffect, useRef, useState } from "react";
import {
  Terminal,
  Trash2,
  AlertCircle,
  Info,
  Bug,
  TriangleAlert,
  MessageSquareWarning,
} from "lucide-react";
import { useLSPStore } from "@/stores/lsp-store";
import { useEditorStore } from "@/stores/editor-store";
import type { LSPLogEntry, Diagnostic } from "@/types/lsp";
import { describeDiagnosticCode, SKRIPT_DIAGNOSTIC_SOURCE } from "@/types/lsp";
import { cn, basename } from "@/lib/utils";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Badge } from "@/components/ui/badge";

export function OutputPanel() {
  const logs = useLSPStore((s) => s.logs);
  const clearLogs = useLSPStore((s) => s.clearLogs);
  const diagnosticsByFile = useLSPStore((s) => s.diagnosticsByFile);
  const endRef = useRef<HTMLDivElement>(null);
  const [activeTab, setActiveTab] = useState<"output" | "problems">("output");

  // Auto-scroll to bottom on new log
  useEffect(() => {
    if (activeTab === "output") {
      endRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, activeTab]);

  const totalProblems = Array.from(diagnosticsByFile.values()).reduce(
    (n, ds) => n + ds.length,
    0,
  );

  return (
    <div className="flex flex-col h-full bg-card border-t border-border">
      <div className="flex items-center justify-between h-7 px-2 border-b border-border shrink-0">
        <Tabs
          value={activeTab}
          onValueChange={(v) => setActiveTab(v as "output" | "problems")}
        >
          <TabsList className="h-6 p-0.5 bg-transparent">
            <TabsTrigger
              value="output"
              className="h-5 px-2 text-2xs font-semibold uppercase tracking-wider rounded data-[state=active]:bg-accent/10 data-[state=active]:text-foreground text-muted-foreground"
            >
              <Terminal className="h-3 w-3 mr-1" />
              Output
            </TabsTrigger>
            <TabsTrigger
              value="problems"
              className="h-5 px-2 text-2xs font-semibold uppercase tracking-wider rounded data-[state=active]:bg-accent/10 data-[state=active]:text-foreground text-muted-foreground"
            >
              <MessageSquareWarning className="h-3 w-3 mr-1" />
              Problems
              {totalProblems > 0 && (
                <span className="ml-1 inline-flex items-center justify-center min-w-[16px] h-4 px-1 rounded-full bg-amber-500 text-white text-2xs font-bold">
                  {totalProblems}
                </span>
              )}
            </TabsTrigger>
          </TabsList>
        </Tabs>
        <button
          onClick={clearLogs}
          className="text-muted-foreground hover:text-foreground transition-colors p-1"
          title="Clear output"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>

      <Tabs value={activeTab} className="flex-1 min-h-0 flex flex-col">
        <TabsContent
          value="output"
          className="flex-1 min-h-0 mt-0 overflow-y-auto scrollbar-thin p-2 font-mono text-xs"
        >
          {logs.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-1.5 py-4">
              <Terminal className="h-5 w-5 opacity-40" />
              <span>No output yet. LSP logs will appear here.</span>
            </div>
          ) : (
            <div className="space-y-0.5">
              {logs.map((entry, i) => (
                <LogLine key={i} entry={entry} />
              ))}
              <div ref={endRef} />
            </div>
          )}
        </TabsContent>

        <TabsContent
          value="problems"
          className="flex-1 min-h-0 mt-0 overflow-y-auto scrollbar-thin"
        >
          {totalProblems === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-1.5 py-4">
              <MessageSquareWarning className="h-5 w-5 opacity-40" />
              <span>No problems detected.</span>
            </div>
          ) : (
            <ProblemsList diagnosticsByFile={diagnosticsByFile} />
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}

function LogLine({ entry }: { entry: LSPLogEntry }) {
  const Icon =
    entry.level === "error"
      ? AlertCircle
      : entry.level === "debug"
        ? Bug
        : Info;
  const color =
    entry.level === "error"
      ? "text-destructive"
      : entry.level === "warn"
        ? "text-amber-500"
        : entry.level === "debug"
          ? "text-muted-foreground"
          : "text-foreground";

  return (
    <div className={cn("flex items-start gap-2 px-1 py-0.5", color)}>
      <Icon className="h-3 w-3 mt-0.5 shrink-0 opacity-70" />
      <span className="text-muted-foreground shrink-0">
        {new Date(entry.timestamp).toLocaleTimeString("en-US", { hour12: false })}
      </span>
      <span className="uppercase text-2xs font-semibold shrink-0 w-10">
        {entry.level}
      </span>
      <span className="break-all">{entry.message}</span>
    </div>
  );
}

function ProblemsList({
  diagnosticsByFile,
}: {
  diagnosticsByFile: Map<string, Diagnostic[]>;
}) {
  const setActiveTab = useEditorStore((s) => s.setActiveTab);

  const entries = Array.from(diagnosticsByFile.entries()).sort(([a], [b]) =>
    a.localeCompare(b),
  );

  return (
    <div className="py-1">
      {entries.map(([path, diags]) => (
        <div key={path} className="pb-1">
          <div className="flex items-center gap-1 px-2 py-1 text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            <TriangleAlert className="h-3 w-3 text-amber-500" />
            <span className="truncate font-mono normal-case">{basename(path)}</span>
            <span className="normal-case font-normal text-muted-foreground">
              ({diags.length})
            </span>
          </div>
          {diags.map((d, i) => (
            <button
              key={i}
              onClick={() => setActiveTab(path)}
              className="group flex items-start gap-2 w-full text-left px-3 py-1 hover:bg-accent/10 border-l-2 border-transparent hover:border-border transition-colors"
            >
              <SeverityIcon severity={d.severity} />
              <span className="text-2xs text-muted-foreground shrink-0 font-mono">
                {d.startLine}:{d.startColumn}
              </span>
              <span className="text-xs text-foreground/90 flex-1 break-words">
                {d.message}
              </span>
              {d.source === SKRIPT_DIAGNOSTIC_SOURCE && (
                <Badge variant="outline" className="text-2xs font-mono shrink-0">
                  {describeDiagnosticCode(d.source) ?? "skript"}
                </Badge>
              )}
            </button>
          ))}
        </div>
      ))}
    </div>
  );
}

function SeverityIcon({ severity }: { severity: Diagnostic["severity"] }) {
  // LSP severity: 1=Error, 2=Warning, 3=Information, 4=Hint
  switch (severity) {
    case 1:
      return <AlertCircle className="h-3 w-3 text-destructive shrink-0 mt-0.5" />;
    case 2:
      return <TriangleAlert className="h-3 w-3 text-amber-500 shrink-0 mt-0.5" />;
    default:
      return <Info className="h-3 w-3 text-info shrink-0 mt-0.5" />;
  }
}
