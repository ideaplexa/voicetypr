#!/usr/bin/env node
// Refresh the pinned source archive, never Cargo's shared registry checkout.
// Native sources remain byte-for-byte upstream; the small build-script patch
// and generated Windows ABI snapshots are reviewed separately in this repo.
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const directory = fileURLToPath(new URL('../src-tauri/vendor/whisper-rs-sys/upstream/', import.meta.url));
const filename = `${directory}/whisper-rs-sys-0.15.0.crate`;
const expected = '6986c0fe081241d391f09b9a071fbcbb59720c3563628c3c829057cf69f2a56f';

function verified(bytes) {
  if (createHash('sha256').update(bytes).digest('hex') !== expected) {
    throw new Error('whisper-rs-sys source archive failed SHA-256 verification');
  }
  return bytes;
}

try {
  let bytes;
  try {
    bytes = verified(await readFile(filename));
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    const response = await fetch('https://static.crates.io/crates/whisper-rs-sys/whisper-rs-sys-0.15.0.crate');
    if (!response.ok) throw new Error(`Source archive download failed (${response.status})`);
    bytes = verified(Buffer.from(await response.arrayBuffer()));
    await mkdir(directory, { recursive: true });
    await writeFile(filename, bytes);
  }
  console.log(`Verified whisper-rs-sys 0.15.0 source archive (${bytes.length} bytes).`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
