/**
 * A single expanded SkriptHub syntax entry card.
 */

import { useState } from "react";
import {
  Copy,
  Check,
  Package,
  TrendingUp,
  AlertTriangle,
  Plug,
} from "lucide-react";
import type { SkriptHubEntry } from "@/types/skript";
import {
  parsePattern,
  SYNTAX_TYPE_LABELS,
  SYNTAX_TYPE_COLORS,
} from "@/types/skript";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export function DocsEntry({ entry }: { entry: SkriptHubEntry }) {
  const [copied, setCopied] = useState(false);
  const variants = parsePattern(entry.syntax_pattern);

  const copyPattern = () => {
    navigator.clipboard?.writeText(entry.syntax_pattern);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  return (
    <div className="px-3 py-2.5 space-y-2 animate-fade-in">
      {/* Pattern variants */}
      <div className="space-y-1">
        {variants.map((v, i) => (
          <div
            key={i}
            className="font-mono text-xs bg-muted/50 border border-border rounded px-2 py-1.5 flex items-start gap-2"
          >
            <code className="flex-1 break-all">
              {v.segments.map((seg, j) => (
                <span key={j} className={segmentClass(seg)}>
                  {renderSegment(seg)}
                </span>
              ))}
            </code>
            <button
              onClick={copyPattern}
              className="shrink-0 p-0.5 text-muted-foreground hover:text-primary transition-colors"
              title="Copy pattern"
            >
              {copied ? (
                <Check className="h-3 w-3 text-emerald-500" />
              ) : (
                <Copy className="h-3 w-3" />
              )}
            </button>
          </div>
        ))}
      </div>

      {/* Description */}
      <p className="text-xs text-muted-foreground leading-relaxed">
        {entry.description}
      </p>

      {/* Meta */}
      <div className="flex flex-wrap items-center gap-3 text-2xs text-muted-foreground">
        <span className="inline-flex items-center gap-1">
          <Package className="h-3 w-3" />
          {entry.addon.name}
        </span>
        <span className="inline-flex items-center gap-1">
          <TrendingUp className="h-3 w-3" />
          {entry.addon.usage_score}/100
        </span>
        {entry.return_type && (
          <span>
            returns{" "}
            <code className="font-mono text-foreground">
              {entry.return_type}
            </code>
          </span>
        )}
        {entry.event_cancellable && (
          <Badge variant="success">cancellable</Badge>
        )}
        {entry.mark_as_removed && (
          <Badge variant="destructive">
            <AlertTriangle className="h-3 w-3 mr-0.5" />
            removed{entry.removed_since ? ` in ${entry.removed_since}` : ""}
          </Badge>
        )}
      </div>

      {/* Usage meter */}
      <div className="h-1 bg-muted rounded overflow-hidden">
        <div
          className="h-full bg-gradient-to-r from-primary/60 to-primary"
          style={{ width: `${entry.addon.usage_score}%` }}
        />
      </div>

      {/* Required plugins */}
      {entry.required_plugins.length > 0 && (
        <div className="flex flex-wrap items-center gap-1 pt-1">
          <Plug className="h-3 w-3 text-muted-foreground" />
          <span className="text-2xs text-muted-foreground">requires:</span>
          {entry.required_plugins.map((p) => (
            <Badge key={p.name} variant="outline" className="font-mono text-2xs">
              {p.name}
            </Badge>
          ))}
        </div>
      )}

      {/* Event values */}
      {entry.event_values && (
        <div className="text-2xs text-muted-foreground">
          <span className="font-medium">Event values:</span>{" "}
          <code className="font-mono">{entry.event_values}</code>
        </div>
      )}
    </div>
  );
}

type Segment =
  | { kind: "text"; value: string }
  | { kind: "required"; value: string }
  | { kind: "optional"; value: string }
  | { kind: "choice"; options: string[] };

function segmentClass(seg: Segment): string {
  switch (seg.kind) {
    case "required":
      return "text-primary font-semibold";
    case "optional":
      return "text-muted-foreground italic";
    case "choice":
      return "text-purple-500 dark:text-purple-300";
    default:
      return "text-foreground";
  }
}

function renderSegment(seg: Segment): React.ReactNode {
  switch (seg.kind) {
    case "required":
      return `<${seg.value}>`;
    case "optional":
      return `[${seg.value}]`;
    case "choice":
      return `(${seg.options.join("|")})`;
    default:
      return seg.value;
  }
}

void SYNTAX_TYPE_LABELS;
void SYNTAX_TYPE_COLORS;
