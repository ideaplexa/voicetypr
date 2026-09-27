import fs from 'node:fs';

const FAST_PATH_PREFIXES = ['.github/', 'docs/', 'plans/'];
const MARKDOWN_DOCUMENTATION = /\.md$/i;
const FRONTEND_ONLY_PREFIXES = ['apps/site/', 'public/', 'src/'];
const FRONTEND_ONLY_FILES = [
  'components.json',
  'index.html',
  'pill.html',
  'toast.html',
  'tsconfig.json',
  'tsconfig.node.json',
  'vite.config.ts',
  'vitest.config.ts',
  '.oxfmtrc.json',
  '.oxlintrc.json',
];

export function isFrontendOnlyPath(filePath) {
  const normalized = filePath.replaceAll('\\', '/').replace(/^\.\//, '');
  return (
    FRONTEND_ONLY_PREFIXES.some((prefix) => normalized.startsWith(prefix)) ||
    FRONTEND_ONLY_FILES.includes(normalized)
  );
}

export function isWorkflowOrDocumentationPath(filePath) {
  const normalized = filePath.replaceAll('\\', '/').replace(/^\.\//, '');
  return (
    FAST_PATH_PREFIXES.some((prefix) => normalized.startsWith(prefix)) ||
    MARKDOWN_DOCUMENTATION.test(normalized)
  );
}

export function classifyChangedFiles(filePaths) {
  const changedFiles = filePaths.filter((filePath) => filePath.length > 0);

  if (changedFiles.length === 0) {
    return {
      applicationRequired: true,
      nativeRequired: true,
      reason: 'No changed files were detected; running application and native checks conservatively.',
    };
  }

  const applicationFiles = changedFiles.filter(
    (filePath) => !isWorkflowOrDocumentationPath(filePath),
  );

  if (applicationFiles.length > 0) {
    const nativeFiles = applicationFiles.filter((filePath) => !isFrontendOnlyPath(filePath));
    return {
      applicationRequired: true,
      nativeRequired: nativeFiles.length > 0,
      reason:
        nativeFiles.length > 0
          ? `Native/build inputs changed: ${nativeFiles.join(', ')}`
          : `Frontend-only inputs changed: ${applicationFiles.join(', ')}`,
    };
  }

  return {
    applicationRequired: false,
    nativeRequired: false,
    reason: 'Only workflow or documentation files changed.',
  };
}

function appendOutput(name, value) {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) return;
  fs.appendFileSync(outputPath, `${name}=${value}\n`);
}

function main() {
  const nullDelimited = process.argv.includes('--null');
  const input = fs.readFileSync(0, 'utf8');
  const filePaths = input.split(nullDelimited ? '\0' : /\r?\n/);
  const result = classifyChangedFiles(filePaths);

  appendOutput('application_required', String(result.applicationRequired));
  appendOutput('native_required', String(result.nativeRequired));
  console.log(result.reason);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}
