/**
 * Theme hook. Watches the settings store and applies the chosen theme
 * to:
 *  - the root `<html>` element (adds/removes the `dark` class)
 *  - the Monaco editor (via `applyTheme`)
 *
 * Also respects the `system` mode by listening to the OS color scheme.
 */

import { useEffect } from "react";
import { useSettingsStore } from "@/stores/settings-store";
import {
  applyTheme,
  defineSkriptThemes,
} from "@/lib/monaco-theme";
import type * as Monaco from "monaco-editor";

interface UseSkriptThemeArgs {
  /** Monaco namespace — passed in once the editor mounts. */
  monaco: typeof Monaco | null;
}

export function useSkriptTheme({ monaco }: UseSkriptThemeArgs): void {
  const theme = useSettingsStore((s) => s.appearance.theme);

  // Define themes once Monaco is available
  useEffect(() => {
    if (!monaco) return;
    defineSkriptThemes(monaco);
  }, [monaco]);

  // Apply theme to <html> + Monaco
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");

    const resolve = () => {
      const effective =
        theme === "system" ? (media.matches ? "dark" : "light") : theme;
      document.documentElement.classList.toggle("dark", effective === "dark");
      if (monaco) applyTheme(monaco, effective);
    };

    resolve();
    if (theme === "system") {
      media.addEventListener("change", resolve);
      return () => media.removeEventListener("change", resolve);
    }
  }, [theme, monaco]);
}
