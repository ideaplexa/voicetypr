#!/usr/bin/env node
// ORT itself is static. Its Windows prebuilt still imports DirectML.dll even
// when sessions use CPU only. Ship that dependency beside the app, using the
// existing app-local DLL install/uninstall path rather than a second runtime.
const { copyFileSync, existsSync, mkdirSync, readFileSync } = require("node:fs");
const { join, resolve } = require("node:path");

const triple = process.env.TAURI_ENV_TARGET_TRIPLE || "x86_64-pc-windows-msvc";
const target = process.env.CARGO_TARGET_DIR || resolve("src-tauri", "target");
const candidates = [
  join(target, triple, "release", "DirectML.dll"),
  join(target, "release", "DirectML.dll"),
];
const source = candidates.find((path) => existsSync(path));
if (!source || readFileSync(source).subarray(0, 2).toString() !== "MZ") {
  console.error("[stage-windows-ort] Missing DirectML.dll from the ORT build. Refusing to bundle.");
  process.exit(1);
}
const destination = resolve("src-tauri", "windows", "resources", "vcredist");
mkdirSync(destination, { recursive: true });
copyFileSync(source, join(destination, "DirectML.dll"));
console.log("[stage-windows-ort] Staged the app-local DirectML dependency.");
