import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const VERSION_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;
const RELEASE_TYPES = new Set(['current', 'patch', 'minor', 'major']);
const CHANNELS = new Set(['stable', 'beta']);

export function parseVersion(version) {
  const match = VERSION_PATTERN.exec(version);
  if (!match) throw new Error(`Invalid current version: ${version}`);
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

export function calculateReleaseVersion({ current, channel, releaseType, betaNumber }) {
  if (!CHANNELS.has(channel)) throw new Error(`Unsupported release channel: ${channel}`);
  if (!RELEASE_TYPES.has(releaseType)) throw new Error(`Unsupported release type: ${releaseType}`);

  const parsed = parseVersion(current);
  let next;
  switch (releaseType) {
    case 'current':
      next = `${parsed.major}.${parsed.minor}.${parsed.patch}`;
      break;
    case 'patch':
      next = `${parsed.major}.${parsed.minor}.${parsed.patch + 1}`;
      break;
    case 'minor':
      next = `${parsed.major}.${parsed.minor + 1}.0`;
      break;
    case 'major':
      next = `${parsed.major + 1}.0.0`;
      break;
  }

  if (channel === 'beta') {
    if (releaseType === 'current') {
      throw new Error('Beta releases must target a newer major, minor, or patch version.');
    }
    if (!/^[1-9]\d*$/.test(betaNumber ?? '')) {
      throw new Error('beta_number must be a positive integer.');
    }
    next = `${next}-beta.${betaNumber}`;
  }

  return { version: next, tag: `v${next}` };
}

export function releaseTagRef(tag) {
  if (!tag.startsWith('v') || !VERSION_PATTERN.test(tag.slice(1))) {
    throw new Error(`Invalid release tag: ${tag}`);
  }
  return `refs/tags/${tag}`;
}

export function assertRemoteTagAvailable(
  tag,
  { cwd = process.cwd(), remote = 'origin', run = spawnSync } = {},
) {
  const ref = releaseTagRef(tag);
  const result = run('git', ['ls-remote', '--exit-code', '--tags', remote, ref], {
    cwd,
    encoding: 'utf8',
  });

  if (result.status === 2) return;
  if (result.status === 0) {
    throw new Error(`Tag ${tag} already exists. Use a different release type.`);
  }

  const detail = result.stderr?.trim() || `git exited with status ${result.status}`;
  throw new Error(`Failed to check tag ${tag}: ${detail}`);
}

export function applyReleaseVersion(root, version) {
  parseVersion(version);
  const packagePath = path.join(root, 'package.json');
  const cargoPath = path.join(root, 'src-tauri', 'Cargo.toml');

  const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
  packageJson.version = version;
  fs.writeFileSync(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);

  const cargo = fs.readFileSync(cargoPath, 'utf8');
  const updatedCargo = cargo.replace(/^version = "[^"]*"/m, `version = "${version}"`);
  if (updatedCargo === cargo && !cargo.includes(`version = "${version}"`)) {
    throw new Error('Could not find the package version in src-tauri/Cargo.toml.');
  }
  fs.writeFileSync(cargoPath, updatedCargo);
}

function appendOutput(name, value) {
  if (!process.env.GITHUB_OUTPUT) return;
  fs.appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}

function currentPackageVersion(root) {
  const packageJson = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  return packageJson.version;
}

export function runCli(args, root = process.cwd()) {
  const [command, ...commandArgs] = args;

  switch (command) {
    case 'calculate': {
      const [channel, releaseType, betaNumber] = commandArgs;
      const current = currentPackageVersion(root);
      const result = calculateReleaseVersion({ current, channel, releaseType, betaNumber });
      appendOutput('version', result.version);
      appendOutput('tag', result.tag);
      console.log(`::notice::Release: ${current} → ${result.version} (${channel}, ${releaseType})`);
      return;
    }
    case 'check-tag': {
      const [tag] = commandArgs;
      assertRemoteTagAvailable(tag, { cwd: root });
      console.log(`Release tag is available: ${tag}`);
      return;
    }
    case 'apply-version': {
      const [version] = commandArgs;
      applyReleaseVersion(root, version);
      console.log(`Applied release version ${version}`);
      return;
    }
    default:
      throw new Error(`Unknown release-tool command: ${command ?? '(missing)'}`);
  }
}

const isEntrypoint = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isEntrypoint) {
  try {
    runCli(process.argv.slice(2));
  } catch (error) {
    console.error(`::error::${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
