import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { expect, it } from 'vitest';

const workflow = readFileSync(join(import.meta.dirname, '../.github/workflows/release.yml'), 'utf8');
const prepare = workflow.split('      - name: Prepare release body\n')[1].split('      - name: Generate latest.json')[0];
const script = prepare.split('        run: |\n')[1].replace(/^          /gm, '').split('${{ github.repository }}').join('owner/repo');

it.each(['', '\n', ' \t\n\r\n', 'Existing notes\n', 'lookup failure'])('prepares usable release notes for %j', body => {
  const cwd = mkdtempSync(join(tmpdir(), 'release-notes-'));
  try {
    writeFileSync(join(cwd, 'gh'), `#!/bin/bash
if [ "$1" = release ]; then
  [ "$DRAFT_BODY" != 'lookup failure' ] || exit 1
  printf '%s' "$DRAFT_BODY"
else
  printf 'Generated notes\\n'
fi
`, {mode:0o755});
    const result = spawnSync('bash', ['-e', '-c', script], {cwd, env:{...process.env, PATH:`${cwd}:${process.env.PATH}`, RELEASE_TAG:'vtest', DRAFT_BODY:body}, encoding:'utf8'});
    expect(result.status, result.stderr).toBe(0);
    expect(readFileSync(join(cwd, 'release-body.md'), 'utf8')).toBe(body === 'Existing notes\n' ? body : 'Generated notes\n');
  } finally { rmSync(cwd, {recursive:true, force:true}); }
});
