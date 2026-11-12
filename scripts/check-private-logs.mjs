// A deliberately small deny-list of values that have leaked into logs before.
// This is a guardrail, not a substitute for reviewing logs and error values.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

function* rustFiles(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'tests') yield* rustFiles(path);
    } else if (entry.name.endsWith('.rs') && !entry.name.endsWith('_tests.rs')) {
      yield path;
    }
  }
}

// Find the closing delimiter without being confused by strings and comments.
function closing(source, start, open, close) {
  let depth = 0, quote = '', lineComment = false, blockComment = 0;
  for (let i = start; i < source.length; i++) {
    const c = source[i], next = source[i + 1];
    if (lineComment) { if (c === '\n') lineComment = false; continue; }
    if (blockComment) {
      if (c === '/' && next === '*') { blockComment++; i++; }
      else if (c === '*' && next === '/') { blockComment--; i++; }
      continue;
    }
    if (quote) {
      if (c === '\\') i++;
      else if (c === quote) quote = '';
      continue;
    }
    if (c === '/' && next === '/') { lineComment = true; i++; continue; }
    if (c === '/' && next === '*') { blockComment++; i++; continue; }
    if (c === '"') { quote = c; continue; }
    if (c === open) depth++;
    if (c === close && --depth === 0) return i;
  }
  return source.length - 1;
}

let failures = 0;
function report(path, source, offset) {
  console.error(`${path}:${source.slice(0, offset).split('\n').length}: sensitive value in log`);
  failures++;
}
for (const path of rustFiles('src-tauri/src')) {
  let source = readFileSync(path, 'utf8');
  // Mask inline test modules, preserving offsets and line numbers.
  const testModules = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/g;
  for (const match of source.matchAll(testModules)) {
    const start = match.index + match[0].length - 1;
    const end = closing(source, start, '{', '}');
    source = source.slice(0, match.index) + source.slice(match.index, end + 1).replace(/[^\n]/g, ' ') + source.slice(end + 1);
  }
  const macros = /\b(?:log::)?(?:trace|debug|info|warn|error)!\s*\(/g;
  for (const match of source.matchAll(macros)) {
    const end = closing(source, match.index + match[0].length - 1, '(', ')');
    const call = source.slice(match.index, end + 1);
    // Direct arguments or captured format fields, not counts such as text.len().
    const names = 'base_url|model_path|file_path|log_path|models_dir|output_path|recording_file|text|transcript';
    const captures = new RegExp(`\\{(?:${names})(?:[}:])`).test(call);
    const args = call.replace(/"(?:\\.|[^"\\])*"/gs, '""');
    const direct = new RegExp(`[,\\s]\\s*(?:&\\s*)?(?:${names})\\s*[,)]`).test(args);
    const pathDisplay = /\b(?:model_path|file_path|log_path|models_dir|output_path|recording_file)\s*\.\s*display\s*\(/.test(args);
    if (captures || direct || pathDisplay) report(path, source, match.index);
  }
}
const swiftPath = 'sidecar/parakeet-swift/Sources/main.swift';
const swift = readFileSync(swiftPath, 'utf8');
for (const match of swift.matchAll(/\blog\s*\([^\n]*/g)) {
  if (/\\\((?:audioPath|expectedPath\.path|path\.path|toks\.suffix|(?:text|transcript)(?:\)|\.)|error(?:\)|\.localizedDescription))/.test(match[0])) report(swiftPath, swift, match.index);
}
if (failures) process.exitCode = 1;
