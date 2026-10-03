import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
describe('recording log privacy', () => {
  it('never sends recording paths or filenames to logging calls', () => {
    const source = readFileSync('src-tauri/src/commands/audio.rs', 'utf8');
    const calls: string[] = [];
    for (const match of source.matchAll(/(?:log::(?:info|warn|error|debug|trace)!|log_with_context|log_file_operation)\(/g)) {
      let depth = 1, quoted = false, escaped = false, end = match.index + match[0].length;
      for (; end < source.length && depth; end++) {
        const c = source[end];
        if (quoted) { if (escaped) escaped = false; else if (c === '\\') escaped = true; else if (c === '"') quoted = false; }
        else if (c === '"') quoted = true;
        else if (c === '(') depth++;
        else if (c === ')') depth--;
      }
      calls.push(source.slice(match.index, end));
    }
    const leaks = calls.filter(call => {
      // String literals alone can mention files; sensitive values cannot be arguments.
      const args = call.replace(/"(?:[^"\\]|\\.)*"/g, '""');
      return /\b(?:audio_path|audio_path_str|normalized_path|dest_path|filename|file_name|path|window_title)\b/.test(args);
    });
    expect(leaks).toEqual([]);
  });
});
