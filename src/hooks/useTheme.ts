import { useEffect } from "react";
import { useSettings } from "@/contexts/SettingsContext";

export type ThemePreference = "system" | "light" | "dark";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Normalize a stored theme value to the known set. Unknown or missing
 * values fall back to "system" (the backend default).
 */
export function normalizeTheme(value: string | null | undefined): ThemePreference {
  return value === "dark" || value === "light" ? value : "system";
}

/**
 * Keeps `document.documentElement.classList` in sync with the user's theme:
 * - "dark" adds `.dark`, "light" removes it.
 * - "system" (and missing settings) follows `prefers-color-scheme`, and
 *   re-applies the class whenever the OS appearance changes.
 */
export function useTheme() {
  const { settings } = useSettings();
  const theme = normalizeTheme(settings?.theme);

  useEffect(() => {
    const media = window.matchMedia(DARK_QUERY);
    const apply = () => {
      const isDark = theme === "dark" || (theme === "system" && media.matches);
      document.documentElement.classList.toggle("dark", isDark);
    };

    apply();

    if (theme === "system") {
      media.addEventListener("change", apply);
      return () => media.removeEventListener("change", apply);
    }
    return undefined;
  }, [theme]);
}
