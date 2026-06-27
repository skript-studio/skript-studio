/**
 * Main app layout — three-column resizable layout.
 *
 *   ┌─────────────────────────────────────────────────────────┐
 *   │  MenuBar (fixed height)                                 │
 *   ├──────────┬──────────────────────────────┬───────────────┤
 *   │ Explorer │  EditorTabs / Editor          │  Docs         │
 *   │          │  ─────────────────────────── │               │
 *   │          │  OutputPanel                  │               │
 *   ├──────────┴──────────────────────────────┴───────────────┤
 *   │  StatusBar (fixed height)                               │
 *   └─────────────────────────────────────────────────────────┘
 */

import { useState } from "react";
import {
  PanelGroup,
  Panel,
  PanelResizeHandle,
  type ImperativePanelHandle,
} from "react-resizable-panels";
import { MenuBar } from "@/components/menu/MenuBar";
import { FileExplorer } from "@/components/explorer/FileExplorer";
import { EditorTabs } from "@/components/editor/EditorTabs";
import { EditorBreadcrumb } from "@/components/editor/EditorBreadcrumb";
import { MonacoEditor } from "@/components/editor/MonacoEditor";
import { OutputPanel } from "@/components/editor/OutputPanel";
import { StatusBar } from "@/components/editor/StatusBar";
import { DocsPanel } from "@/components/docs/DocsPanel";
import { WelcomeOverlay } from "@/components/editor/WelcomeOverlay";
import { useEditorStore } from "@/stores/editor-store";
import { useLSP } from "@/hooks/use-lsp";
import { registerSkriptLanguage } from "@/lib/monaco-skript";
import type * as Monaco from "monaco-editor";
import { cn } from "@/lib/utils";

interface AppLayoutProps {
  onOpenSettings: () => void;
}

export function AppLayout({ onOpenSettings }: AppLayoutProps) {
  const [monaco, setMonaco] = useState<typeof Monaco | null>(null);
  const openFiles = useEditorStore((s) => s.openFiles);

  // Wire up LSP once Monaco is ready
  useLSP({ monaco });

  const handleMonacoReady = (m: typeof Monaco) => {
    registerSkriptLanguage(m);
    setMonaco(m);
  };

  // Panel visibility & refs for menu toggles
  const [leftVisible, setLeftVisible] = useState(true);
  const [rightVisible, setRightVisible] = useState(true);
  const [bottomVisible, setBottomVisible] = useState(false);

  const toggleLeft = () => setLeftVisible((v) => !v);
  const toggleRight = () => setRightVisible((v) => !v);
  const toggleBottom = () => setBottomVisible((v) => !v);

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden">
      <MenuBar
        onOpenSettings={onOpenSettings}
        onToggleLeftPanel={toggleLeft}
        onToggleRightPanel={toggleRight}
        onToggleBottomPanel={toggleBottom}
      />

      <PanelGroup direction="horizontal" className="flex-1 min-h-0">
        {leftVisible && (
          <>
            <Panel
              defaultSize={18}
              minSize={12}
              maxSize={30}
              collapsible
              className="bg-sidebar"
            >
              <FileExplorer />
            </Panel>
            <ResizeHandle direction="horizontal" />
          </>
        )}

        <Panel defaultSize={62} minSize={30}>
          <PanelGroup direction="vertical">
            <Panel defaultSize={bottomVisible ? 72 : 100} minSize={20}>
              <div className="flex flex-col h-full bg-background">
                <EditorTabs />
                <EditorBreadcrumb />
                <div className="flex-1 min-h-0 relative">
                  <MonacoEditor onMonacoReady={handleMonacoReady} />
                  {openFiles.length === 0 && <WelcomeOverlay />}
                </div>
              </div>
            </Panel>
            {bottomVisible && (
              <>
                <ResizeHandle direction="vertical" />
                <Panel defaultSize={28} minSize={10} maxSize={70}>
                  <OutputPanel />
                </Panel>
              </>
            )}
          </PanelGroup>
        </Panel>

        {rightVisible && (
          <>
            <ResizeHandle direction="horizontal" />
            <Panel
              defaultSize={20}
              minSize={12}
              maxSize={35}
              collapsible
              className="bg-sidebar"
            >
              <DocsPanel />
            </Panel>
          </>
        )}
      </PanelGroup>

      <StatusBar monaco={monaco} />
    </div>
  );
}

function ResizeHandle({ direction }: { direction: "horizontal" | "vertical" }) {
  return (
    <PanelResizeHandle
      className={cn(
        "relative bg-border hover:bg-primary/50 transition-colors",
        direction === "horizontal" ? "w-px" : "h-px",
      )}
    />
  );
}

// Silence unused-import warning for ImperativePanelHandle (kept for future panel API use)
void (null as unknown as ImperativePanelHandle);
