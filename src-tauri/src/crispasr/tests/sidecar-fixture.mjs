// Deterministic transport failures, without a model or GPU dependency.
import { createInterface } from 'node:readline';
const mode = process.argv[2];
const lines = createInterface({ input: process.stdin });
lines.on('line', (line) => {
  const request = JSON.parse(line);
  if (mode === 'wrong_id') {
    process.stdout.write(`${JSON.stringify({ type: 'status', protocol: 1, id: request.id + 1 })}\n`);
  }
  // "stall" intentionally keeps stdin open and never replies.
});
