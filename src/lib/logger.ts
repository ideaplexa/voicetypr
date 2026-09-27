import {
  trace as tauriTrace,
  debug as tauriDebug,
  info as tauriInfo,
  warn as tauriWarn,
  error as tauriError,
} from "@tauri-apps/plugin-log";

/**
 * App logger.
 *
 * Frontend logs are forwarded to the backend `tauri-plugin-log` instance, so
 * they land in the SAME rotated file + stdout stream as the Rust logs (and in
 * anything the Report Bug flow collects). Prefer a scoped logger so messages
 * are taggable and filterable:
 *
 *   const log = createLogger("models");
 *   log.debug("status refreshed", { downloaded: 2 });
 *
 * Levels follow `log`/`tauri-plugin-log`: trace < debug < info < warn < error.
 * Use `debug`/`trace` for routine, high-frequency, or per-item detail and
 * reserve `info` for meaningful lifecycle events.
 */

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

const LEVEL_WEIGHT: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
};

// Mirror the backend tauri-plugin-log filter (Debug in dev builds, Info in
// release). Gating on the client BEFORE the IPC call matters: plugin-log's
// functions each `invoke()` into Rust where the level filter runs, so a
// sub-threshold call would otherwise pay a full round-trip just to be dropped.
// import.meta.env.VITEST is injected by vitest. In tests the logger must behave
// like the console.* it replaced: no IPC, original args, all levels.
const isTest = !!import.meta.env.VITEST || import.meta.env.MODE === "test";

let threshold = LEVEL_WEIGHT[isTest ? "trace" : import.meta.env.DEV ? "debug" : "info"];

/** Override the client-side log threshold at runtime (e.g. while debugging). */
export function setLogLevel(level: LogLevel): void {
  threshold = LEVEL_WEIGHT[level];
}

// Tauri v2 injects `__TAURI_INTERNALS__` on the global; it is absent in unit
// tests and plain browsers, where we must not attempt the IPC call.
const isTauri =
  typeof window !== "undefined" &&
  Object.prototype.hasOwnProperty.call(window, "__TAURI_INTERNALS__");

// Mirror to the devtools console in dev and in tests (where it is the only sink
// and keeps console-spy assertions working), and whenever not inside Tauri.
const echoToConsole = import.meta.env.DEV || isTest || !isTauri;

const TAURI_SINKS: Record<LogLevel, (message: string) => Promise<void>> = {
  trace: tauriTrace,
  debug: tauriDebug,
  info: tauriInfo,
  warn: tauriWarn,
  error: tauriError,
};

// Resolve the console method by NAME at call time (not a reference bound at
// module load) so a test's vi.spyOn(console, ...) is honoured.
const CONSOLE_METHOD: Record<LogLevel, "debug" | "info" | "warn" | "error"> = {
  trace: "debug",
  debug: "debug",
  info: "info",
  warn: "warn",
  error: "error",
};

function stringify(part: unknown): string {
  if (typeof part === "string") return part;
  if (part instanceof Error) return part.stack ?? `${part.name}: ${part.message}`;
  try {
    return JSON.stringify(part);
  } catch {
    return String(part);
  }
}

function emit(level: LogLevel, scope: string | undefined, parts: unknown[]): void {
  if (LEVEL_WEIGHT[level] < threshold) return;

  if (isTauri && !isTest) {
    const body = parts.map(stringify).join(" ");
    // Fire-and-forget; logging must never throw or block the caller.
    void TAURI_SINKS[level](scope ? `[${scope}] ${body}` : body).catch(() => {});
  }

  if (echoToConsole) {
    const method = CONSOLE_METHOD[level];
    // In tests, pass the original args unprefixed so existing console-spy
    // assertions keep matching; elsewhere prefix the scope for readability.
    if (scope && !isTest) console[method](`[${scope}]`, ...parts);
    else console[method](...parts);
  }
}

export interface Logger {
  trace: (...args: unknown[]) => void;
  debug: (...args: unknown[]) => void;
  info: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
}

function makeLogger(scope?: string): Logger {
  return {
    trace: (...args) => emit("trace", scope, args),
    debug: (...args) => emit("debug", scope, args),
    info: (...args) => emit("info", scope, args),
    warn: (...args) => emit("warn", scope, args),
    error: (...args) => emit("error", scope, args),
  };
}

/** Create a logger whose messages are prefixed with `[scope]`. */
export function createLogger(scope: string): Logger {
  return makeLogger(scope);
}

/** App-wide logger. Prefer `createLogger("feature")` for taggable logs. */
export const logger: Logger = makeLogger();
