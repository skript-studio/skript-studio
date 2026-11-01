/**
 * Docs search input + type filter chips.
 */

import { Search, Filter, X } from "lucide-react";
import { useDocsStore } from "@/stores/docs-store";
import {
  SYNTAX_TYPE_LABELS,
  SYNTAX_TYPE_COLORS,
  type SkriptSyntaxType,
} from "@/types/skript";
import { cn } from "@/lib/utils";
import { useState } from "react";

const TYPE_ORDER: SkriptSyntaxType[] = [
  "event",
  "effect",
  "expression",
  "condition",
  "section",
  "other",
];

export function DocsSearch() {
  const query = useDocsStore((s) => s.query);
  const setQuery = useDocsStore((s) => s.setQuery);
  const typeFilter = useDocsStore((s) => s.typeFilter);
  const toggleType = useDocsStore((s) => s.toggleType);
  const clearTypes = useDocsStore((s) => s.clearTypes);
  const [showFilter, setShowFilter] = useState(typeFilter.size > 0);

  return (
    <div className="px-3 pb-2 space-y-2">
      <div className="relative">
        <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search syntax…"
          className="w-full bg-background border border-border rounded-md pl-7 pr-14 py-1.5 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring transition-shadow"
        />
        <div className="absolute right-1.5 top-1/2 -translate-y-1/2 flex items-center gap-0.5">
          {query && (
            <button
              onClick={() => setQuery("")}
              className="p-0.5 rounded text-muted-foreground hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            onClick={() => setShowFilter((v) => !v)}
            className={cn(
              "p-0.5 rounded transition-colors",
              typeFilter.size > 0 || showFilter
                ? "text-primary bg-primary/10"
                : "text-muted-foreground hover:text-foreground",
            )}
            title="Filter by type"
          >
            <Filter className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {showFilter && (
        <div className="flex flex-wrap gap-1 animate-fade-in">
          {TYPE_ORDER.map((type) => (
            <button
              key={type}
              onClick={() => toggleType(type)}
              className={cn(
                "px-1.5 py-0.5 rounded-md text-2xs font-medium border transition-colors",
                typeFilter.has(type)
                  ? SYNTAX_TYPE_COLORS[type]
                  : "border-border text-muted-foreground hover:border-ring hover:text-foreground",
              )}
            >
              {SYNTAX_TYPE_LABELS[type]}
            </button>
          ))}
          {typeFilter.size > 0 && (
            <button
              onClick={clearTypes}
              className="px-1.5 py-0.5 rounded-md text-2xs font-medium border border-border text-muted-foreground hover:text-foreground"
            >
              Clear
            </button>
          )}
        </div>
      )}
    </div>
  );
}
