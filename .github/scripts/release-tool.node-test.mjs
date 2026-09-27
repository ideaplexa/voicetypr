import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, it } from 'node:test';

import {
  applyReleaseVersion,
  assertRemoteTagAvailable,
  calculateReleaseVersion,
  releaseTagRef,
} from './release-tool.mjs';

describe('release version calculation', () => {
  it('calculates stable release types from a stable version', () => {
    assert.deepEqual(
      calculateReleaseVersion({ current: '2.0.5', channel: 'stable', releaseType: 'current' }),
      { version: '2.0.5', tag: 'v2.0.5' },
    );
    assert.equal(
      calculateReleaseVersion({ current: '2.0.5', channel: 'stable', releaseType: 'patch' }).version,
      '2.0.6',
    );
    assert.equal(
      calculateReleaseVersion({ current: '2.0.5', channel: 'stable', releaseType: 'minor' }).version,
      '2.1.0',
    );
    assert.equal(
      calculateReleaseVersion({ current: '2.0.5', channel: 'stable', releaseType: 'major' }).version,
      '3.0.0',
    );
  });

  it('promotes the current prerelease base to stable', () => {
    assert.deepEqual(
      calculateReleaseVersion({
        current: '2.1.0-beta.7',
        channel: 'stable',
        releaseType: 'current',
      }),
      { version: '2.1.0', tag: 'v2.1.0' },
    );
  });

  it('creates beta versions only from a positive sequence number', () => {
    assert.deepEqual(
      calculateReleaseVersion({
        current: '2.0.5',
        channel: 'beta',
        releaseType: 'patch',
        betaNumber: '8',
      }),
      { version: '2.0.6-beta.8', tag: 'v2.0.6-beta.8' },
    );

    for (const betaNumber of ['', '0', '-1', '01', 'x']) {
      assert.throws(
        () =>
          calculateReleaseVersion({
            current: '2.0.5',
            channel: 'beta',
            releaseType: 'patch',
            betaNumber,
          }),
        /positive integer/,
      );
    }
    assert.throws(
      () =>
        calculateReleaseVersion({
          current: '2.0.5',
          channel: 'beta',
          releaseType: 'current',
          betaNumber: '1',
        }),
      /newer major, minor, or patch/,
    );
  });
});

describe('exact release tag lookup', () => {
  it('constructs one exact full tag ref', () => {
    assert.equal(releaseTagRef('v2.0.5'), 'refs/tags/v2.0.5');
    assert.throws(() => releaseTagRef('v2.0.5*'), /Invalid release tag/);
  });

  it('does not mistake a similarly prefixed beta tag for a stable tag', () => {
    const repository = fs.mkdtempSync(path.join(os.tmpdir(), 'voicetypr-release-tags-'));
    try {
      execFileSync('git', ['init', '--quiet'], { cwd: repository });
      execFileSync('git', ['config', 'user.name', 'Release Test'], { cwd: repository });
      execFileSync('git', ['config', 'user.email', 'release-test@example.invalid'], {
        cwd: repository,
      });
      fs.writeFileSync(path.join(repository, 'fixture.txt'), 'fixture\n');
      execFileSync('git', ['add', 'fixture.txt'], { cwd: repository });
      execFileSync('git', ['commit', '--quiet', '-m', 'fixture'], { cwd: repository });
      execFileSync('git', ['tag', 'v2.0.5-beta.7'], { cwd: repository });

      assert.doesNotThrow(() =>
        assertRemoteTagAvailable('v2.0.5', { cwd: repository, remote: repository }),
      );

      execFileSync('git', ['tag', 'v2.0.5'], { cwd: repository });
      assert.throws(
        () => assertRemoteTagAvailable('v2.0.5', { cwd: repository, remote: repository }),
        /already exists/,
      );
    } finally {
      fs.rmSync(repository, { recursive: true, force: true });
    }
  });
});

describe('release version files', () => {
  it('updates package.json and the Cargo package version together', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'voicetypr-release-version-'));
    try {
      fs.mkdirSync(path.join(root, 'src-tauri'));
      fs.writeFileSync(path.join(root, 'package.json'), '{"name":"voicetypr","version":"2.0.5"}\n');
      fs.writeFileSync(
        path.join(root, 'src-tauri', 'Cargo.toml'),
        '[package]\nname = "voicetypr"\nversion = "2.0.5"\n',
      );

      applyReleaseVersion(root, '2.0.6-beta.1');

      assert.equal(JSON.parse(fs.readFileSync(path.join(root, 'package.json'))).version, '2.0.6-beta.1');
      assert.match(
        fs.readFileSync(path.join(root, 'src-tauri', 'Cargo.toml'), 'utf8'),
        /^version = "2\.0\.6-beta\.1"$/m,
      );
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
