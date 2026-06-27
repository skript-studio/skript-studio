/**
 * Monaco editor wrapper.
 *
 * Owns the Monaco instance lifecycle and exposes:
 *  - the Monaco namespace (for sibling components that need to register
 *    languages/themes/completion providers)
 *  - the editor instance (for cursor tracking, format commands, etc.)
 *
 * Wires content changes back to the editor store on every keystroke.
 * Also mirrors Monaco markers (which monaco-languageclient populates
 * from LSP diagnostics) into the lsp-store so the OutputPanel
 * "Problems" tab and StatusBar can render them.
 */

import { useEffect, useRef } from "react";
import Editor, { loader, type OnMount } from "@monaco-editor/react";
// Import Monaco from the npm package (bundled by Vite) instead of fetching it
// from a CDN at runtime. The packaged app's CSP blocks the CDN, which left the
// editor stuck on "Loading editor…". Bundling also makes the IDE fully offline.
import * as monaco from "monaco-editor";
import type * as Monaco from "monaco-editor";
import { SKRIPT_LANGUAGE_ID } from "@/lib/monaco-skript";
import { useEditorStore } from "@/stores/editor-store";
import { useSettingsStore } from "@/stores/settings-store";
import { useLSPStore } from "@/stores/lsp-store";
import { useSkriptTheme } from "@/hooks/use-skript-theme";
import type { Diagnostic } from "@/types/lsp";
import { fileUriToPath } from "@/lib/utils";

interface MonacoEditorProps {
  onMonacoReady?: (monaco: typeof Monaco) => void;
  onEditorReady?: (editor: Monaco.editor.IStandaloneCodeEditor) => void;
}

export function MonacoEditor({
  onMonacoReady,
  onEditorReady,
}: MonacoEditorProps) {
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof Monaco | null>(null);

  const activeTab = useEditorStore((s) => s.activeTab);
  const openFiles = useEditorStore((s) => s.openFiles);
  const updateContent = useEditorStore((s) => s.updateContent);
  const saveCursor = useEditorStore((s) => s.saveCursor);
  const appearance = useSettingsStore((s) => s.appearance);
  const setDiagnostics = useLSPStore((s) => s.setDiagnostics);

  const activeFile = openFiles.find((f) => f.path === activeTab) ?? null;

  // Configure the Monaco loader once: use the locally-bundled Monaco instance
  // so the editor works offline and isn't blocked by the app CSP.
  useEffect(() => {
    loader.config({ monaco });
  }, []);

  useSkriptTheme({ monaco: monacoRef.current });

  // Mirror Monaco markers → lsp-store whenever they change. monaco-languageclient
  // pushes LSP diagnostics into Monaco's marker service; we subscribe to that
  // and forward the relevant ones into our store so the OutputPanel "Problems"
  // tab and StatusBar can read them.
  useEffect(() => {
    const monaco = monacoRef.current;
    if (!monaco) return;

    const sync = () => {
      const model = editorRef.current?.getModel();
      if (!model) return;
      const uri = model.uri.toString();
      const path = fileUriToPath(uri);
      if (!path) return;
      const markers = monaco.editor.getModelMarkers({ resource: model.uri });
      const diags: Diagnostic[] = markers.map((m) => ({
        startLine: m.startLineNumber,
        endLine: m.endLineNumber,
        startColumn: m.startColumn,
        endColumn: m.endColumn,
        severity: m.severity as 1 | 2 | 3 | 4,
        message: m.message,
        source: m.source,
      }));
      setDiagnostics(path, diags);
    };

    // Subscribe to marker changes
    const disposable = monaco.editor.onDidChangeMarkers(() => sync());
    // Also sync on model changes (tab switches)
    const modelDisp = editorRef.current?.onDidChangeModel(() => sync());

    return () => {
      disposable.dispose();
      modelDisp?.dispose();
    };
  }, [setDiagnostics, activeTab]);

  const handleMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
    onMonacoReady?.(monaco);
    onEditorReady?.(editor);

    // Save cursor position on change
    editor.onDidChangeCursorPosition((e) => {
      if (activeTab) {
        saveCursor(activeTab, e.position.lineNumber, e.position.column);
      }
    });
  };

  // Restore cursor when switching tabs
  useEffect(() => {
    if (!editorRef.current || !activeFile) return;
    if (activeFile.cursorLine && activeFile.cursorColumn) {
      editorRef.current.setPosition({
        lineNumber: activeFile.cursorLine,
        column: activeFile.cursorColumn,
      });
      editorRef.current.revealLineInCenterIfOutsideViewport(
        activeFile.cursorLine,
      );
      editorRef.current.focus();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab]);

  return (
    <Editor
      path={activeFile?.path ?? undefined}
      language={SKRIPT_LANGUAGE_ID}
      value={activeFile?.content ?? ""}
      onChange={(value) => {
        if (activeTab) updateContent(activeTab, value ?? "");
      }}
      onMount={handleMount}
      theme={
        appearance.theme === "light"
          ? "skript-studio-light"
          : "skript-studio-dark"
      }
      loading={
        <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
          Loading editor…
        </div>
      }
      options={{
        fontFamily: appearance.fontFamily,
        fontSize: appearance.fontSize,
        fontLigatures: true,
        lineHeight: Math.round(appearance.fontSize * 1.55),
        letterSpacing: 0.3,
        minimap: { enabled: appearance.minimap, renderCharacters: false },
        scrollBeyondLastLine: false,
        smoothScrolling: true,
        cursorBlinking: "smooth",
        cursorSmoothCaretAnimation: "on",
        cursorWidth: 2,
        renderWhitespace: "selection",
        renderLineHighlight: "all",
        bracketPairColorization: { enabled: true },
        guides: { bracketPairs: true, indentation: true },
        tabSize: appearance.tabSize,
        insertSpaces: false,
        wordWrap: appearance.wordWrap ? "on" : "off",
        automaticLayout: true,
        padding: { top: 12, bottom: 12 },
        scrollbar: {
          verticalScrollbarSize: 10,
          horizontalScrollbarSize: 10,
          useShadows: false,
        },
        overviewRulerBorder: false,
        fixedOverflowWidgets: true,
        quickSuggestions: { other: true, comments: false, strings: true },
        suggestOnTriggerCharacters: true,
        acceptSuggestionOnEnter: "on",
        formatOnPaste: true,
        formatOnType: true,
        stickyScroll: { enabled: true },
      }}
    />
  );
}
