#!/usr/bin/env node
// LunarG's Qt installer embeds several 7z archives. Opening it as one archive
// extracts only Bin; parser mode exposes the remaining Include/Lib payloads.
// Extracting those files provides an isolated SDK without running its installer.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync } from 'node:fs';
import path from 'node:path';

const [installer, output, sevenZip] = process.argv.slice(2);
if (!installer || !output || !sevenZip) throw new Error('Usage: node scripts/extract-vulkan-sdk.mjs <installer> <output> <7z.exe>');
const parts = path.join(output, 'installer-parts');
mkdirSync(parts, { recursive: true });
function extract(args) {
  const result = spawnSync(sevenZip, args, { stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`SDK extraction failed (${result.status})`);
}
extract(['x', '-t#', installer, `-o${parts}`, '*.7z', '-y', '-bso0', '-bsp0']);
for (const filename of readdirSync(parts).filter((name) => name.endsWith('.7z'))) {
  extract(['x', path.join(parts, filename), `-o${output}`, '-y', '-bso0', '-bsp0']);
}
for (const required of ['Include/vulkan/vulkan.h', 'Lib/vulkan-1.lib', 'Bin/glslc.exe']) {
  if (!existsSync(path.join(output, required))) throw new Error(`SDK component missing: ${required}`);
}
console.log('Prepared isolated Vulkan SDK headers, libraries and shader compiler.');
