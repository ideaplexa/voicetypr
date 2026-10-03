import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const sourceRoot = process.env.O1_REVIEW_SOURCE_ROOT ?? 'src-tauri/src';
const rust = (file: string) => readFileSync(`${sourceRoot}/${file}.rs`, 'utf8');
// Source integration gates supplement Rust runtime tests, which run outside this sandbox.
const section = (source: string, start: string, end: string) =>
  source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));
function loggingCalls(source: string): string[] {
  const calls: string[] = [];
  for (const match of source.matchAll(/log::(?:warn|error|debug|info|trace)!\(/g)) {
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
  return calls;
}
describe('O1 review integration contracts', () => {
  it('removes only pinned SDK enrichment before schema validation', () => {
    const source = rust('product_analytics');
    const scrubber = section(source, 'fn scrub_event(', 'fn validated_dynamic_properties(');
    expect(scrubber.indexOf('event.remove_prop(key)')).toBeGreaterThan(0);
    expect(scrubber.indexOf('event.remove_prop(key)')).toBeLessThan(scrubber.indexOf('validated_dynamic_properties'));
    expect(source).toContain('"$lib_version__patch"');
    expect(scrubber).not.toMatch(/starts_with\([^)]*\$/);
  });
  it('serializes generic migration and persistence with the dedicated consent lock', () => {
    const source = rust('commands/settings');
    const transaction = section(source, 'pub(crate) async fn persist_generic_settings', '#[derive(Serialize');
    expect(transaction).toContain('CONSENT_MUTATION_LOCK');
    expect(transaction.indexOf('CONSENT_MUTATION_LOCK')).toBeLessThan(transaction.indexOf('migrated_consent'));
    expect(transaction).toContain('store.save()');
    expect(section(source, 'pub async fn save_settings(', 'pub async fn set_global_shortcut(')).toContain('persist_generic_settings(&store).await?');
    expect(rust('commands/telemetry')).toContain('CONSENT_MUTATION_LOCK.lock().await');
  });
  it('revokes the epoch before synchronously draining the SDK queue', () => {
    const source = rust('product_analytics');
    expect(source).toContain('revoke_with_flush(posthog_rs::flush)');
    const revoke = section(source, 'fn revoke_with_flush(', 'pub(crate) fn consent_epoch(');
    expect(revoke.indexOf('state.generation =')).toBeGreaterThan(0);
    expect(revoke.indexOf('state.generation =')).toBeLessThan(revoke.indexOf('flush()'));
    expect(revoke.indexOf('drop(state)')).toBeLessThan(revoke.indexOf('flush()'));
    expect(source).toContain('.max_capture_attempts(1)');
  });
  it('keeps HTTP and websocket response bodies out of provider logging', () => {
    const common = rust('cloud_stt/common');
    expect(common).not.toContain('response body: {snippet}');
    expect(common).toContain('http_failure_log(label, status.as_u16(), &err)');
    const files = ['cloud_stt', 'ai'].flatMap(dir => readdirSync(`${sourceRoot}/${dir}`)
      .filter(file => file.endsWith('.rs')).map(file => join(dir, file).replace(/\.rs$/, '')));
    const leaks = files.flatMap(file => loggingCalls(rust(file)).filter(call =>
      /\b(?:body|snippet|response_text|error_body)\b/.test(call.replace(/"(?:[^"\\]|\\.)*"/g, '""'))
      || /\{(?:body|snippet|response_text|error_body)\}/.test(call)));
    expect(leaks).toEqual([]);
    for (const file of ['cloud_stt/soniox_ws', 'cloud_stt/deepgram_ws']) {
      expect(loggingCalls(rust(file)).filter(call => /(?:connect failed|read error).*\{error\}/.test(call))).toEqual([]);
    }
  });
  it('pins outstanding aliases and resolves from the saved kept trace', () => {
    const source = rust('observability');
    expect(source).toContain('outstanding: HashMap');
    expect(section(source, 'fn inherit_trace(', 'pub const BLOCKED')).not.toContain('t.id != trace.id');
    expect(section(rust('recording/kept'), 'fn resolve_clip(', '#[cfg(test)]')).toContain('clip.trace.as_ref()');
    expect(rust('recording/kept')).toContain('_retry_trace_lease = crate::observability::pin(generation)');
  });
  it('threads captured generations through stages and the Polish pipeline', () => {
    const audio = rust('commands/audio');
    const guards = section(audio, 'struct DecodeJourneyGuard', '#[cfg(test)]');
    expect(guards).not.toMatch(/product_analytics::capture\(/);
    expect(guards.match(/self\.generation/g)).toHaveLength(2);
    expect(audio).toContain('DeliveryJourneyGuard::new(task_generation)');
    expect(audio).not.toMatch(/product_analytics::capture\(/);
    expect(audio).not.toContain('island_notice::notice(');
    expect(rust('commands/island_notice')).toContain('Some(generation)');
    expect(audio.match(/writing::process_transcription_at\(/g)).toHaveLength(2);
    const polish = section(rust('commands/ai'), 'pub async fn polish_text_typed(', '#[derive(Debug, Serialize');
    expect(polish).not.toContain('current_recording_generation');
    expect(polish).toContain('generation: u64');
    expect(rust('writing/pipeline')).toContain('request.generation,');
  });
  it('claims one retry terminal and distinguishes cancellation from failure', () => {
    const source = rust('recording/kept');
    const retry = section(source, 'async fn retry(', 'fn retry_setup');
    expect(retry).toContain('Err(RetryError::Cancelled)');
    expect(retry).toContain('if claim_terminal(&lease.terminal)');
    expect(retry).toContain('if resolution == "retried_failed"');
    expect(section(source, 'fn resolve_clip(', '#[cfg(test)]')).toContain('claim_terminal(&clip.terminal)');
  });
  it('stamps peek counts and checks their epoch at capture', () => {
    const source = rust('observability');
    const peeks = section(source, 'struct PeekCounter', '#[cfg(test)]');
    expect(peeks).toContain('current != Some(accepted)');
    expect(peeks).toContain('self.epoch != current');
    expect(peeks).toContain('capture_at_epoch');
    expect(peeks).toContain('Some(epoch)');
  });
  it('writes sanitized panic location and time without PanicHookInfo debug data', () => {
    const source = rust('lib');
    const hook = section(source, '// Try to save panic info', '// Forward to the prior');
    expect(hook).not.toContain('Full info');
    expect(hook).not.toMatch(/format![\s\S]*panic_info/);
    expect(hook).toContain('local_panic_record');
  });
});
