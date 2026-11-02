/**
 * Native (direct API key) provider effort/fast-mode helpers.
 * Mirrors the Rust contract: effort values are ordered none < minimal < low <
 * medium < high; xhigh/max are never exposed.
 */

const NATIVE_PROVIDER_IDS = ["openai", "anthropic", "gemini"] as const;

export const NATIVE_EFFORT_LEVELS = ["none", "minimal", "low", "medium", "high"] as const;
export const DEFAULT_NATIVE_EFFORT = "medium";

export const isNativeProvider = (providerId: string): boolean =>
  (NATIVE_PROVIDER_IDS as readonly string[]).includes(providerId);

/** Keep only the effort levels the UI is allowed to expose. */
export const nativeEffortLevels = (levels: readonly string[] | undefined): string[] =>
  (levels ?? []).filter((level) =>
    (NATIVE_EFFORT_LEVELS as readonly string[]).includes(level),
  );

/**
 * Resolve which effort a model ends up using for a stored preference:
 * the stored level if supported, else "medium", else the highest supported
 * level below medium, else the lowest supported level; undefined when the
 * model supports no effort levels at all.
 */
export const resolveNativeEffort = (
  stored: string | undefined,
  supportedLevels: readonly string[],
): string | undefined => {
  const levels = nativeEffortLevels(supportedLevels);
  if (levels.length === 0) return undefined;
  if (stored && levels.includes(stored)) return stored;
  if (levels.includes(DEFAULT_NATIVE_EFFORT)) return DEFAULT_NATIVE_EFFORT;
  const below = levels.filter(
    (level) =>
      (NATIVE_EFFORT_LEVELS as readonly string[]).indexOf(level) <
      NATIVE_EFFORT_LEVELS.indexOf(DEFAULT_NATIVE_EFFORT),
  );
  return below[below.length - 1] ?? levels[0];
};

/** Display label for a native effort level (stored "none" reads as Off). */
export const formatNativeEffort = (level: string): string =>
  level === "none" ? "Off" : level.charAt(0).toUpperCase() + level.slice(1);
