#!/usr/bin/env node
// Shared by development, CI and packaging: build both variants from one pinned
// source checkout. Keep them sequential because upstream configures a generated
// package file in that checkout, even when the output directories differ.
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const root = fileURLToPath(new URL('../', import.meta.url));
const { values } = parseArgs({ options: { target: { type: 'string' }, cache: { type: 'string' }, source: { type: 'string' } } });
const cache = path.resolve(values.cache ?? process.env.VOICETYPR_CRISPASR_CACHE ?? path.join(root, '.tmp', 'crispasr'));
const source = values.source ?? path.join(cache, 'source');
for (const variant of ['cpu', 'gpu']) {
  const args = [path.join(root, 'scripts', 'build-crispasr-sidecar.mjs'), '--variant', variant,
    '--source', source, '--build-dir', path.join(cache, `${values.target ?? process.arch}-${variant}`)];
  if (values.target) args.push('--target', values.target);
  const result = spawnSync(process.execPath, args, { cwd: root, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
