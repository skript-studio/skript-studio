/**
 * SkriptHub documentation panel — right side of the IDE.
 *
 * Lists all syntax entries from the SkriptHub API (proxied via Tauri
 * or fetched directly), grouped by syntax type. Clicking an entry
 * expands its full documentation card inline.
 */

import { useEffect, useMemo } from "react";
import { Loader2, AlertCircle, BookOpen, RefreshCw } from "lucide-react";
import { useDocsStore } from "@/stores/docs-store";
import type { SkriptHubEntry, SkriptSyntaxType } from "@/types/skript";
import { SYNTAX_TYPE_LABELS } from "@/types/skript";
import { ScrollArea } from "@/components/ui/scroll-area";
import { DocsSearch } from "./DocsSearch";
import { DocsCategory } from "./DocsCategory";

const TYPE_ORDER: SkriptSyntaxType[] = [
  "event",
  "effect",
  "expression",
  "condition",
  "section",
  "other",
];

export function DocsPanel() {
  const entries = useDocsStore((s) => s.entries);
  const loading = useDocsStore((s) => s.loading);
  const error = useDocsStore((s) => s.error);
  const query = useDocsStore((s) => s.query);
  const typeFilter = useDocsStore((s) => s.typeFilter);
  const load = useDocsStore((s) => s.load);
  const selectedId = useDocsStore((s) => s.selectedId);

  // Load on first mount
  useEffect(() => {
    void load();
  }, [load]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries.filter((it) => {
      if (typeFilter.size > 0 && !typeFilter.has(it.syntax_type)) return false;
      if (!q) return true;
      return (
        it.title.toLowerCase().includes(q) ||
        it.description.toLowerCase().includes(q) ||
        it.syntax_pattern.toLowerCase().includes(q) ||
        it.addon.name.toLowerCase().includes(q)
      );
    });
  }, [entries, query, typeFilter]);

  const grouped = useMemo(() => {
    const map = new Map<SkriptSyntaxType, SkriptHubEntry[]>();
    for (const type of TYPE_ORDER) map.set(type, []);
    for (const e of filtered) map.get(e.syntax_type)?.push(e);
    return map;
  }, [filtered]);

  return (
    <div className="flex flex-col h-full bg-sidebar text-sidebar-foreground">
      <div className="panel-section flex items-center justify-between">
        <span>SkriptHub Docs</span>
        <div className="flex items-center gap-0.5">
          <button
            onClick={() => void load()}
            title="Reload"
            className="inline-flex items-center justify-center h-6 w-6 rounded text-muted-foreground hover:bg-accent hover:text-accent-foreground transition-colors"
          >
            <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
          </button>
        </div>
      </div>

      <DocsSearch />

      <ScrollArea className="flex-1">
        <div className="pb-3">
          {loading && entries.length === 0 && (
            <div className="flex flex-col items-center justify-center py-12 gap-2 text-muted-foreground">
              <Loader2 className="h-5 w-5 animate-spin" />
              <span className="text-xs">Loading syntax database…</span>
            </div>
          )}

          {error && (
            <div className="flex flex-col items-center justify-center py-12 gap-2 text-destructive">
              <AlertCircle className="h-5 w-5" />
              <span className="text-xs">{error}</span>
              <button
                onClick={() => void load()}
                className="text-xs text-primary hover:underline"
              >
                Retry
              </button>
            </div>
          )}

          {!loading && !error && filtered.length === 0 && (
            <div className="flex flex-col items-center justify-center py-12 gap-2 text-muted-foreground">
              <BookOpen className="h-5 w-5 opacity-50" />
              <span className="text-xs">
                {query
                  ? `No syntax matches "${query}"`
                  : "No syntax entries available"}
              </span>
            </div>
          )}

          {!loading &&
            !error &&
            TYPE_ORDER.map((type) => {
              const list = grouped.get(type) ?? [];
              if (list.length === 0) return null;
              return (
                <DocsCategory
                  key={type}
                  type={type}
                  entries={list}
                  selectedId={selectedId}
                />
              );
            })}
        </div>
      </ScrollArea>
    </div>
  );
}

function cn(...args: unknown[]): string {
  return args.filter(Boolean).join(" ");
}

void SYNTAX_TYPE_LABELS;
