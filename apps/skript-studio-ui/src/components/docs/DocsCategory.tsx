/**
 * A grouped section of syntax entries (e.g. all "Events").
 * Clicking an entry expands its full documentation inline.
 */

import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { SkriptHubEntry, SkriptSyntaxType } from "@/types/skript";
import { SYNTAX_TYPE_LABELS, SYNTAX_TYPE_COLORS } from "@/types/skript";
import { DocsEntry } from "./DocsEntry";
import { cn } from "@/lib/utils";

interface DocsCategoryProps {
  type: SkriptSyntaxType;
  entries: SkriptHubEntry[];
  selectedId: number | null;
}

export function DocsCategory({ type, entries }: DocsCategoryProps) {
  const [open, setOpen] = useState(true);
  const [expandedId, setExpandedId] = useState<number | null>(null);

  return (
    <div className="pb-1">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1 w-full px-3 py-1 text-2xs font-semibold uppercase tracking-wider text-muted-foreground hover:text-foreground transition-colors"
      >
        {open ? (
          <ChevronDown className="h-3 w-3" />
        ) : (
          <ChevronRight className="h-3 w-3" />
        )}
        <span
          className={cn(
            "h-1.5 w-1.5 rounded-full",
            SYNTAX_TYPE_COLORS[type].split(" ")[0],
          )}
        />
        <span>{SYNTAX_TYPE_LABELS[type]}</span>
        <span className="text-muted-foreground normal-case font-normal">
          ({entries.length})
        </span>
      </button>

      {open && (
        <div>
          {entries.map((entry) => (
            <div key={entry.id}>
              <button
                onClick={() =>
                  setExpandedId((p) => (p === entry.id ? null : entry.id))
                }
                className={cn(
                  "group flex items-start gap-2 w-full text-left px-3 py-1.5 hover:bg-accent/10 border-l-2 border-transparent hover:border-border transition-colors",
                  expandedId === entry.id && "bg-accent/10 border-primary",
                )}
              >
                <span className="text-sm font-medium text-foreground truncate flex-1">
                  {entry.title}
                </span>
                <span
                  className={cn(
                    "inline-flex items-center px-1.5 py-0.5 rounded-md text-2xs font-medium border shrink-0",
                    SYNTAX_TYPE_COLORS[type],
                  )}
                >
                  {SYNTAX_TYPE_LABELS[type]}
                </span>
              </button>
              {expandedId === entry.id && <DocsEntry entry={entry} />}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
