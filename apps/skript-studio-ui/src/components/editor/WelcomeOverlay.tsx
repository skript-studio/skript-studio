/**
 * Welcome overlay — shown over the Monaco editor when no files are open.
 * Provides quick-start actions and a Skript syntax cheat sheet.
 */

import {
  FilePlus2,
  FolderOpen,
  BookOpen,
  Sparkles,
  ArrowRight,
} from "lucide-react";
import { useEditorStore } from "@/stores/editor-store";
import { useTauriFs } from "@/hooks/use-tauri-fs";
import { useDocsStore } from "@/stores/docs-store";

export function WelcomeOverlay() {
  const fs = useTauriFs();
  const setProjectRoot = useEditorStore((s) => s.setProjectRoot);
  const openFile = useEditorStore((s) => s.openFile);
  const loadDocs = useDocsStore((s) => s.load);

  const handleOpenFolder = async () => {
    try {
      const path = await fs.openFolder();
      if (path) setProjectRoot(path);
    } catch (err) {
      console.error(err);
    }
  };

  const handleNewFile = () => {
    openFile("untitled-1.sk", "# New Skript file\n\non join:\n    send \"&aWelcome!\" to player\n");
  };

  return (
    <div className="absolute inset-0 z-10 bg-background/95 backdrop-blur-sm overflow-y-auto scrollbar-thin">
      <div className="min-h-full flex items-center justify-center p-8">
        <div className="max-w-3xl w-full">
          {/* Hero */}
          <div className="text-center mb-10">
            <div className="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-gradient-to-br from-primary to-primary/60 mb-3 shadow-lg shadow-primary/20">
              <Sparkles className="h-7 w-7 text-primary-foreground" />
            </div>
            <h1 className="text-3xl font-bold tracking-tight mb-1.5">
              Skript<span className="text-primary">Studio</span>
            </h1>
            <p className="text-muted-foreground max-w-xl mx-auto text-sm">
              A dedicated IDE for Minecraft Skript — powered by a Rust LSP,
              SkriptHub syntax database, and Monaco editor.
            </p>
          </div>

          {/* Quick actions */}
          <div className="grid grid-cols-3 gap-3 mb-8">
            <ActionCard
              icon={FilePlus2}
              title="New file"
              description="Start a fresh Skript script"
              onClick={handleNewFile}
              primary
            />
            <ActionCard
              icon={FolderOpen}
              title="Open folder"
              description="Browse your scripts folder"
              onClick={handleOpenFolder}
            />
            <ActionCard
              icon={BookOpen}
              title="Browse syntax"
              description="Explore SkriptHub docs"
              onClick={() => void loadDocs()}
            />
          </div>

          {/* Cheat sheet */}
          <div className="border-t border-border pt-5">
            <h3 className="text-sm font-semibold mb-3">Quick reference</h3>
            <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 text-sm">
              {[
                { code: "on join:", desc: "Triggers when a player joins" },
                { code: "command /heal:", desc: "Declares a custom command" },
                { code: "set {x} to 1", desc: "Assigns to a global variable" },
                { code: 'send "text" to player', desc: "Sends a chat message" },
                { code: "if {x} > 10:", desc: "Conditional branch" },
                { code: "loop all players:", desc: "Iterate over players" },
              ].map((item) => (
                <div
                  key={item.code}
                  className="flex items-center justify-between gap-3 py-1 px-2 -mx-2 rounded hover:bg-accent/10 cursor-default transition-colors"
                >
                  <code className="font-mono text-xs text-primary">{item.code}</code>
                  <span className="text-xs text-muted-foreground text-right">
                    {item.desc}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function ActionCard({
  icon: Icon,
  title,
  description,
  onClick,
  primary,
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  description: string;
  onClick: () => void;
  primary?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`group flex items-start gap-3 p-3.5 rounded-lg border text-left transition-all ${
        primary
          ? "border-primary/40 bg-primary/10 hover:border-primary hover:bg-primary/15"
          : "border-border bg-card hover:border-ring hover:bg-accent/10"
      }`}
    >
      <div
        className={`shrink-0 w-8 h-8 rounded-md flex items-center justify-center ${
          primary ? "bg-primary/20 text-primary" : "bg-muted text-muted-foreground"
        }`}
      >
        <Icon className="h-4 w-4" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-sm font-medium">{title}</span>
          <ArrowRight className="h-3 w-3 text-muted-foreground opacity-0 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all" />
        </div>
        <p className="text-xs text-muted-foreground mt-0.5">{description}</p>
      </div>
    </button>
  );
}
