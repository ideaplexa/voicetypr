#!/usr/bin/env node
'use strict';
// Cross-platform launcher for the dev-config `tauri dev`.
//
// Before launching it reaps any orphaned Voicetypr dev processes left behind by
// an interrupted previous session: a stale single-instance ghost otherwise makes
// the new launch defer to the invisible orphan and exit 143.
//
// Reaping is path-scoped and macOS/Linux only:
//   * `pkill -f` matches the debug-build command line, so it can never touch an
//     installed build under /Applications (different path).
//   * The [t]/[p] bracket class keeps the regex from matching pkill's own argv.
//   * Windows is skipped on purpose: the orphan/143 hang was only observed on
//     macOS, and there is no safe path-scoped kill there — `taskkill /IM
//     voicetypr.exe` matches by image name only and would also kill a running
//     *installed* voicetypr.exe (the macOS/Linux pkill -f is path-scoped).
const { spawnSync } = require('node:child_process');

if (process.platform !== 'win32') {
  for (const pattern of ['[t]arget/debug/voicetypr', '[t]arget/debug/parakeet-sidecar']) {
    try {
      spawnSync('pkill', ['-f', pattern], { stdio: 'ignore' });
    } catch {
      // best-effort cleanup; never block the dev launch on it
    }
  }
}

const res = spawnSync('tauri dev --config src-tauri/tauri.dev.conf.json', {
  stdio: 'inherit',
  shell: true,
});

if (res.error) {
  console.error('[tauri:dev] failed to launch tauri:', res.error.message);
  process.exit(1);
}
process.exit(res.status ?? 0);
