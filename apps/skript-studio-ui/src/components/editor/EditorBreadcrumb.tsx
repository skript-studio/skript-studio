/**
 * Path breadcrumb above the editor. Shows the active file's path
 * segments as clickable breadcrumbs (navigation planned for v2).
 */

import { ChevronRight } from "lucide-react";
import { useEditorStore } from "@/stores/editor-store";

export function EditorBreadcrumb() {
  const activeTab = useEditorStore((s) => s.activeTab);
  if (!activeTab) return null;

  const segments = activeTab.replace(/\\/g, "/").split("/").filter(Boolean);

  return (
    <div className="flex items-center gap-0.5 h-6 px-3 text-2xs text-muted-foreground bg-background border-b border-border overflow-x-auto scrollbar-thin shrink-0">
      {segments.map((seg, i) => (
        <div key={i} className="flex items-center gap-0.5 shrink-0">
          {i > 0 && <ChevronRight className="h-3 w-3 opacity-50" />}
          <span
            className={
              i === segments.length - 1
                ? "text-foreground font-medium"
                : "hover:text-foreground cursor-pointer transition-colors"
            }
          >
            {seg}
          </span>
        </div>
      ))}
    </div>
  );
}
