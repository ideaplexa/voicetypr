#!/usr/bin/env node
// Reproducible, incremental sidecar build. Source and ggml revisions are pinned;
// model downloads belong to the app, never to this build or the native runtime.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, copyFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const revision = '340d7085eaa53c40a46dcb73a6d3d0448a480006';
const ggmlRevision = '2f5a80d258c46e6ac8eee95f1328c0f58376d7ee';
const root = fileURLToPath(new URL('../', import.meta.url));
const { values } = parseArgs({ options: {
  source: { type: 'string' }, 'build-dir': { type: 'string' },
  variant: { type: 'string', default: 'cpu' }, target: { type: 'string' },
} });
const architecture = process.arch === 'arm64' ? 'aarch64' : 'x86_64';
const target = values.target ?? `${architecture}-${process.platform === 'win32' ? 'pc-windows-msvc' : process.platform === 'darwin' ? 'apple-darwin' : 'unknown-linux-gnu'}`;
const variant = values.variant;
if (!['cpu', 'gpu'].includes(variant)) throw new Error('variant must be cpu or gpu');
const source = path.resolve(values.source ?? path.join(root, '.tmp', 'crispasr-source'));
const build = path.resolve(values['build-dir'] ?? path.join(root, '.tmp', `crispasr-${target}-${variant}`));

function run(command, args, cwd = root, capture = false, env = process.env) {
  const result = spawnSync(command, args, { cwd, env, encoding: 'utf8', stdio: capture ? 'pipe' : 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed (${result.status})${capture ? `: ${result.stderr}` : ''}`);
  return result.stdout?.trim();
}

try {
  if (!existsSync(path.join(source, '.git'))) {
    mkdirSync(path.dirname(source), { recursive: true });
    run('git', ['clone', '--quiet', '--depth', '1', '--filter=blob:none', '--no-checkout', '--branch', 'v0.8.41', 'https://github.com/CrispStrobe/CrispASR.git', source]);
    run('git', ['sparse-checkout', 'set', '--skip-checks', 'src', 'include', 'examples/cli', 'examples/talk-llama', 'bindings/javascript', 'cmake', 'ggml', 'glint', 'crisp_audio', 'third_party'], source);
    run('git', ['checkout', '--quiet', '--detach', revision], source);
  }
  if (run('git', ['rev-parse', 'HEAD'], source, true) !== revision) throw new Error('CrispASR source revision mismatch');
  // An initialized checkout is read-only during incremental builds. Re-running
  // submodule update rewrites its config and races CPU/GPU builds sharing it.
  if (!existsSync(path.join(source, 'ggml', 'include', 'ggml.h'))) {
    run('git', ['submodule', 'update', '--init', '--depth', '1', 'ggml'], source);
  }
  if (run('git', ['rev-parse', 'HEAD'], path.join(source, 'ggml'), true) !== ggmlRevision) throw new Error('CrispASR ggml revision mismatch');
  run('git', ['diff', '--quiet', '--', 'CMakeLists.txt', 'src', 'include', 'examples/cli', 'examples/json.hpp', 'examples/grammar-parser.h', 'cmake', 'ggml', 'crisp_audio'], source);
  mkdirSync(build, { recursive: true });
  const metal = variant === 'gpu' && target.endsWith('apple-darwin') && target.startsWith('aarch64');
  const vulkan = variant === 'gpu' && !target.endsWith('apple-darwin');
  const configure = ['-S', source, '-B', build,
    `-DCMAKE_PROJECT_crispasr_INCLUDE=${path.join(root, 'sidecar', 'crispasr', 'targets.cmake')}`,
    `-DCRISPASR_SOURCE_DIR=${source}`, '-DCMAKE_BUILD_TYPE=Release',
    `-DGGML_METAL=${metal ? 'ON' : 'OFF'}`, `-DGGML_VULKAN=${vulkan ? 'ON' : 'OFF'}`,
    '-DGGML_METAL_EMBED_LIBRARY=ON'];
  if (process.platform === 'win32') configure.push('-A', target.startsWith('aarch64') ? 'ARM64' : 'x64');
  if (process.platform === 'darwin') configure.push('-DCMAKE_OSX_DEPLOYMENT_TARGET=14.0', `-DCMAKE_OSX_ARCHITECTURES=${target.startsWith('aarch64') ? 'arm64' : 'x86_64'}`);
  run('cmake', configure);
  const buildArgs = ['--build', build, '--config', 'Release', '--parallel', '4', '--target', 'crispasr-sidecar', 'crispasr-audio-test', 'crispasr-qwen3-failure-test'];
  if (process.platform === 'win32') buildArgs.push('--', '/verbosity:quiet');
  run('cmake', buildArgs);
  run('ctest', ['--test-dir', build, '-C', 'Release', '--output-on-failure']);
  const extension = target.includes('windows') ? '.exe' : '';
  const dist = path.join(root, 'sidecar', 'crispasr', 'dist');
  mkdirSync(dist, { recursive: true });
  const executable = path.join(dist, `crispasr-sidecar-${variant}-${target}${extension}`);
  copyFileSync(path.join(build, 'bin', `crispasr-sidecar${extension}`), executable);
  copyFileSync(path.join(source, 'LICENSE'), path.join(dist, 'CrispASR-LICENSE'));
  copyFileSync(path.join(source, 'ggml', 'LICENSE'), path.join(dist, 'ggml-LICENSE'));
  run(process.execPath, ['--test', path.join(root, 'sidecar/crispasr/tests/protocol.test.mjs')], root, false,
    { ...process.env, CRISPASR_SIDECAR: executable });
  console.log(`Staged CrispASR ${variant} sidecar for ${target}.`);
} catch (error) {
  console.error(`[crispasr:build] ${error.message}`);
  process.exitCode = 1;
}
