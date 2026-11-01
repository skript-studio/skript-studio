import { useEffect, useState } from "react";
import { AppLayout } from "@/components/layout/AppLayout";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useSettingsStore } from "@/stores/settings-store";
import { useEditorStore } from "@/stores/editor-store";
import { useTauriFs } from "@/hooks/use-tauri-fs";

export default function App() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const loadSettings = useSettingsStore((s) => s.load);
  const activeTab = useEditorStore((s) => s.activeTab);
  const openFiles = useEditorStore((s) => s.openFiles);
  const markClean = useEditorStore((s) => s.markClean);
  const fs = useTauriFs();

  // Hydrate settings on startup
  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  // Ctrl/Cmd+S to save the active file
  useEffect(() => {
    const handler = async (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        const file = openFiles.find((f) => f.path === activeTab);
        if (!file) return;
        try {
          await fs.writeFile(file.path, file.content);
          markClean(file.path);
        } catch (err) {
          console.error("Save failed:", err);
        }
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === ",") {
        e.preventDefault();
        setSettingsOpen(true);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [activeTab, openFiles, fs, markClean]);

  return (
    <TooltipProvider delayDuration={300}>
      <AppLayout onOpenSettings={() => setSettingsOpen(true)} />
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
    </TooltipProvider>
  );
}
