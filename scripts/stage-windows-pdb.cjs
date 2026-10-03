#!/usr/bin/env node
// Stage the Windows release PDB as a bundled resource.
//
// MSVC keeps debug symbols in a separate voicetypr.pdb (never embedded in the
// .exe), so without it shipped, release crash stacks in Bugsink are all
// `<unknown>`. The PostHog SDK resolves frames in-process at capture via dbghelp,
// which finds symbols in the module's own directory — so the .pdb must end up
// next to voicetypr.exe on the user's machine.
//
// Tauri does not auto-bundle the .pdb. This script (wired as the Windows-only
// `build.beforeBundleCommand` in src-tauri/tauri.windows.conf.json) runs AFTER
// cargo build and BEFORE the bundler collects resources, copying the freshly
// built .pdb into src-tauri/windows/resources/ so it is bundled; the NSIS
// installer hook then copies it beside the exe (see windows/installer-hooks.nsh).
//
// cwd is the project root (Tauri runs before-commands from there). Requires
// `[profile.release] debug = "line-tables-only"` so MSVC emits the .pdb.

const { copyFileSync, existsSync, mkdirSync } = require("node:fs");
const { dirname, join, resolve } = require("node:path");

const triple = process.env.TAURI_ENV_TARGET_TRIPLE || "x86_64-pc-windows-msvc";
const targetDir = process.env.CARGO_TARGET_DIR || resolve("src-tauri", "target");

// `--target <triple>` nests output under <triple>/release; a plain build uses release/.
const candidates = [
  join(targetDir, triple, "release", "voicetypr.pdb"),
  join(targetDir, "release", "voicetypr.pdb"),
];

const src = candidates.find((p) => existsSync(p));
if (!src) {
  console.error(
    "[stage-windows-pdb] voicetypr.pdb not found. Looked in:\n  " +
      candidates.join("\n  ")
  );
  console.error(
    '[stage-windows-pdb] Ensure [profile.release] debug = "line-tables-only" is set so MSVC emits a PDB.'
  );
  process.exit(1);
}

const dest = resolve("src-tauri", "windows", "resources", "voicetypr.pdb");
mkdirSync(dirname(dest), { recursive: true });
copyFileSync(src, dest);
console.log(`[stage-windows-pdb] Copied ${src} -> ${dest}`);
