/**
 * Appearance settings tab — theme, font, tab size, minimap, word wrap.
 */

import { useSettingsStore } from "@/stores/settings-store";
import { FONT_FAMILY_OPTIONS, type ThemeMode } from "@/types/settings";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

const THEME_OPTIONS: { value: ThemeMode; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
  { value: "system", label: "System" },
];

const TAB_SIZE_OPTIONS: Array<2 | 4 | 8> = [2, 4, 8];

export function AppearanceSettings() {
  const appearance = useSettingsStore((s) => s.appearance);
  const update = useSettingsStore((s) => s.updateAppearance);
  const reset = useSettingsStore((s) => s.reset);

  return (
    <div className="space-y-5 py-2 max-h-[50vh] overflow-y-auto scrollbar-thin pr-1">
      <Field label="Theme" hint="Choose the color scheme for the entire IDE.">
        <div className="flex gap-1.5">
          {THEME_OPTIONS.map((opt) => (
            <Button
              key={opt.value}
              size="sm"
              variant={appearance.theme === opt.value ? "default" : "outline"}
              onClick={() => update({ theme: opt.value })}
            >
              {opt.label}
            </Button>
          ))}
        </div>
      </Field>

      <Separator />

      <Field label="Font family" hint="Monospace font for the editor.">
        <select
          value={appearance.fontFamily}
          onChange={(e) => update({ fontFamily: e.target.value })}
          className="h-8 rounded-md border border-input bg-background px-2.5 py-1 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
        >
          {FONT_FAMILY_OPTIONS.map((opt) => (
            <option key={opt.label} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </Field>

      <Field label="Font size" hint="Editor font size in pixels (10–24).">
        <input
          type="number"
          min={10}
          max={24}
          value={appearance.fontSize}
          onChange={(e) =>
            update({ fontSize: clamp(Number(e.target.value), 10, 24) })
          }
          className="h-8 w-20 rounded-md border border-input bg-background px-2.5 py-1 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
        />
      </Field>

      <Field label="Tab size" hint="Number of spaces per indentation level.">
        <div className="flex gap-1.5">
          {TAB_SIZE_OPTIONS.map((size) => (
            <Button
              key={size}
              size="sm"
              variant={appearance.tabSize === size ? "default" : "outline"}
              onClick={() => update({ tabSize: size })}
            >
              {size}
            </Button>
          ))}
        </div>
      </Field>

      <Separator />

      <Toggle
        label="Minimap"
        hint="Show a code overview on the right edge of the editor."
        checked={appearance.minimap}
        onChange={(v) => update({ minimap: v })}
      />

      <Toggle
        label="Word wrap"
        hint="Wrap long lines instead of horizontal scrolling."
        checked={appearance.wordWrap}
        onChange={(v) => update({ wordWrap: v })}
      />

      <Separator />

      <div className="flex justify-end pt-2">
        <Button variant="outline" size="sm" onClick={reset}>
          Reset to defaults
        </Button>
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

function clamp(n: number, min: number, max: number): number {
  if (Number.isNaN(n)) return min;
  return Math.max(min, Math.min(max, n));
}
