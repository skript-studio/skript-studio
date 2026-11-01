/**
 * LSP settings tab — SkriptHub API URL, log level, auto-restart.
 */

import { useSettingsStore } from "@/stores/settings-store";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";

const LOG_LEVELS = ["error", "warn", "info", "debug", "trace"] as const;

export function LSPSettings() {
  const lsp = useSettingsStore((s) => s.lsp);
  const update = useSettingsStore((s) => s.updateLSP);

  return (
    <div className="space-y-5 py-2 max-h-[50vh] overflow-y-auto scrollbar-thin pr-1">
      <Field
        label="SkriptHub API URL"
        hint="Override the default API endpoint. Leave as-is unless you're running a local mirror."
      >
        <Input
          type="url"
          value={lsp.skripthubApiUrl}
          onChange={(e) => update({ skripthubApiUrl: e.target.value })}
          className="font-mono text-xs w-72"
        />
      </Field>

      <Separator />

      <Field
        label="Log level"
        hint="Verbosity of LSP output written to the Output panel."
      >
        <select
          value={lsp.logLevel}
          onChange={(e) =>
            update({ logLevel: e.target.value as (typeof LOG_LEVELS)[number] })
          }
          className="h-8 rounded-md border border-input bg-background px-2.5 py-1 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
        >
          {LOG_LEVELS.map((l) => (
            <option key={l} value={l}>
              {l.toUpperCase()}
            </option>
          ))}
        </select>
      </Field>

      <Separator />

      <Toggle
        label="Auto-restart on crash"
        hint="If the LSP sidecar exits unexpectedly, restart it automatically."
        checked={lsp.autoRestart}
        onChange={(v) => update({ autoRestart: v })}
      />

      <Separator />

      <div className="text-xs text-muted-foreground space-y-1 pt-1">
        <p>
          The LSP sidecar is managed by the Tauri backend. Restart it any time
          from the status bar.
        </p>
        <p>
          Logs are also written to disk in the OS app-data directory for
          debugging.
        </p>
      </div>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium">{label}</div>
        {hint && (
          <div className="text-xs text-muted-foreground mt-0.5">{hint}</div>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex-1">
        <div className="text-sm font-medium">{label}</div>
        {hint && (
          <div className="text-xs text-muted-foreground mt-0.5">{hint}</div>
        )}
      </div>
      <button
        onClick={() => onChange(!checked)}
        className={`relative w-9 h-5 rounded-full transition-colors shrink-0 ${
          checked ? "bg-primary" : "bg-muted"
        }`}
      >
        <span
          className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
            checked ? "translate-x-4" : ""
          }`}
        />
      </button>
    </div>
  );
}

void Button;
