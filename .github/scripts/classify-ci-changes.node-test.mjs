import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import {
  classifyChangedFiles,
  isFrontendOnlyPath,
  isWorkflowOrDocumentationPath,
} from './classify-ci-changes.mjs';

describe('CI change classification', () => {
  it('recognizes workflow paths and Markdown documentation anywhere', () => {
    for (const filePath of [
      '.github/workflows/release.yml',
      '.github/scripts/release-tool.mjs',
      'plans/042-release-workflow-speed.md',
      'docs/release.md',
      'README.md',
      'CHANGELOG.md',
      'scripts/README.md',
      'sidecar/parakeet-swift/README.md',
    ]) {
      assert.equal(isWorkflowOrDocumentationPath(filePath), true, filePath);
    }
  });

  it('separates frontend-only paths from native and packaging inputs', () => {
    for (const filePath of [
      'src/App.tsx',
      'public/logo.png',
      'apps/site/src/app.tsx',
      'vite.config.ts',
    ]) {
      assert.equal(isFrontendOnlyPath(filePath), true, filePath);
      const result = classifyChangedFiles(['README.md', filePath]);
      assert.equal(result.applicationRequired, true, filePath);
      assert.equal(result.nativeRequired, false, filePath);
    }

    for (const filePath of [
      'src-tauri/src/lib.rs',
      'sidecar/parakeet-swift/Package.swift',
      'scripts/build-msix-store.ps1',
      'package.json',
      'pnpm-lock.yaml',
    ]) {
      const result = classifyChangedFiles(['.github/workflows/ci.yml', filePath]);
      assert.equal(result.applicationRequired, true, filePath);
      assert.equal(result.nativeRequired, true, filePath);
    }
  });

  it('requires application checks for both sides of a source-to-docs rename', () => {
    assert.equal(
      classifyChangedFiles(['src/removed.ts', 'docs/removed.ts']).applicationRequired,
      true,
    );
  });

  it('uses the fast path only when every changed path is workflow or documentation', () => {
    const result = classifyChangedFiles([
      '.github/workflows/ci.yml',
      '.github/scripts/classify-ci-changes.mjs',
      'plans/042-release-workflow-speed.md',
    ]);

    assert.equal(result.applicationRequired, false);
    assert.equal(result.nativeRequired, false);
  });

  it('runs application and native checks conservatively when the diff is empty', () => {
    const result = classifyChangedFiles([]);
    assert.equal(result.applicationRequired, true);
    assert.equal(result.nativeRequired, true);
  });
});
