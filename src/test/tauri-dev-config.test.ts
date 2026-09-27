import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// `pnpm tauri:dev` runs `tauri dev --config src-tauri/tauri.dev.conf.json`.
// Tauri resolves config as base -> platform(host) -> --config, using RFC 7396
// JSON Merge Patch: objects deep-merge, arrays/scalars replace wholesale.
//
// Regression guard: the dev config must NOT redefine `bundle.externalBin` or
// the sidecar build commands. If it does, its (platform-agnostic) value is
// applied last and clobbers the macOS platform config, so the Parakeet sidecar
// is never copied next to the dev executable and every local transcription
// fails with "model unavailable / No such file or directory (os error 2)".

type Json = Record<string, unknown>;

function mergePatch(target: unknown, patch: unknown): unknown {
  if (patch === null || typeof patch !== "object" || Array.isArray(patch)) {
    return patch;
  }
  const base =
    target && typeof target === "object" && !Array.isArray(target) ? { ...(target as Json) } : {};
  for (const [key, value] of Object.entries(patch as Json)) {
    if (value === null) delete base[key];
    else base[key] = mergePatch(base[key], value);
  }
  return base;
}

const tauriDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../src-tauri");
const readConf = (name: string) =>
  JSON.parse(readFileSync(resolve(tauriDir, name), "utf8")) as Json;

const base = readConf("tauri.conf.json");
const macos = readConf("tauri.macos.conf.json");
const windows = readConf("tauri.windows.conf.json");
const dev = readConf("tauri.dev.conf.json");

const macDev = mergePatch(mergePatch(base, macos), dev) as Json;
const winDev = mergePatch(mergePatch(base, windows), dev) as Json;

describe("tauri dev config resolution (--config tauri.dev.conf.json)", () => {
  it("keeps the Parakeet sidecar registered in the macOS dev build", () => {
    const bundle = macDev.bundle as Json;
    const build = macDev.build as Json;
    expect((bundle.externalBin as string[]).some((b) => b.includes("parakeet-sidecar"))).toBe(true);
    expect(build.beforeDevCommand as string).toContain("sidecar:build");
    expect(build.devUrl).toBe("http://localhost:1420");
  });

  it("does not leak the Parakeet sidecar into the Windows dev build", () => {
    const bin = (winDev.bundle as Json).externalBin as string[];
    expect(bin.some((b) => b.includes("parakeet"))).toBe(false);
    expect(bin.some((b) => b.includes("whisper-vulkan"))).toBe(true);
  });

  it("preserves the dev-only bundle identifier", () => {
    expect(macDev.identifier).toBe("com.ideaplexa.voicetypr.dev");
  });

  it("labels the development window distinctly", () => {
    for (const config of [macDev, winDev]) {
      const windows = (config.app as Json).windows as Json[];
      expect(windows[0]?.title).toBe("Voicetypr Dev");
    }
  });
});
