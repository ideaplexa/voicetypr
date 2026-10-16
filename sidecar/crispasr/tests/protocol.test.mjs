import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { test } from 'node:test';

test('sidecar keeps its protocol synchronized after invalid and stale requests', async () => {
  assert.ok(process.env.CRISPASR_SIDECAR, 'Set CRISPASR_SIDECAR to the built native executable');
  const child = spawn(process.env.CRISPASR_SIDECAR, [], { stdio: ['pipe', 'pipe', 'pipe'] });
  const requests = [
    '{invalid json}',
    JSON.stringify({ type: 'status', id: 1 }),
    JSON.stringify({ type: 'load_model', id: 2, model: 'test', backend: 'parakeet', model_path: 'private-missing-model.gguf', gpu: false }),
    JSON.stringify({ type: 'audio_chunk', id: 3, session_id: 91, pcm: 'AAAAAA==' }),
    JSON.stringify({ type: 'unknown', id: 4 }),
    JSON.stringify({ type: 'shutdown', id: 5 }),
  ];
  let stdout = '';
  let stderr = '';
  child.stdout.setEncoding('utf8').on('data', (data) => { stdout += data; });
  child.stderr.setEncoding('utf8').on('data', (data) => { stderr += data; });
  const exit = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => { child.kill(); reject(new Error('Sidecar protocol timeout')); }, 30_000);
    child.once('error', (error) => { clearTimeout(timeout); reject(error); });
    child.once('close', (code) => { clearTimeout(timeout); resolve(code); });
  });
  child.stdin.end(`${requests.join('\n')}\n`);
  assert.equal(await exit, 0);
  const messages = stdout.trim().split('\n').map((line) => JSON.parse(line));
  assert.deepEqual(messages.map((message) => message.id), [0, 1, 2, 3, 4, 5]);
  assert.equal(messages[0].code, 'invalid_request');
  assert.equal(messages[1].type, 'status');
  assert.equal(messages[1].protocol, 1);
  assert.equal(messages[1].loaded_model, null);
  assert.equal(messages[2].code, 'model_not_found');
  assert.equal(messages[3].code, 'session_mismatch');
  assert.equal(messages[4].code, 'unknown_command');
  assert.equal(messages[5].type, 'ok');
  assert.equal(stderr, '', 'Native diagnostics must not leak model paths or audio/text payloads');
  assert.ok(!stdout.includes('private-missing-model'));
});
