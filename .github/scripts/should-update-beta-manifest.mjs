function parseSemver(input) {
  const match = input
    .trim()
    .replace(/^v/, "")
    .match(/^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/);
  if (!match) {
    throw new Error(`Invalid semantic version: ${input}`);
  }

  return {
    core: [Number(match[1]), Number(match[2]), Number(match[3])],
    prerelease: match[4]?.split(".") ?? [],
  };
}

export function releaseChannelForVersion(input) {
  const { prerelease } = parseSemver(input);
  if (prerelease.length === 0) return "stable";
  if (
    prerelease.length === 2 &&
    prerelease[0] === "beta" &&
    /^[1-9]\d*$/.test(prerelease[1])
  ) {
    return "beta";
  }
  throw new Error(`Unsupported release version: ${input}`);
}

export function releaseTagForVersion(input) {
  releaseChannelForVersion(input);
  return `v${input.trim().replace(/^v/, "")}`;
}

export function compareSemver(leftInput, rightInput) {
  const left = parseSemver(leftInput);
  const right = parseSemver(rightInput);

  for (let index = 0; index < left.core.length; index += 1) {
    if (left.core[index] !== right.core[index]) {
      return left.core[index] > right.core[index] ? 1 : -1;
    }
  }

  if (left.prerelease.length === 0 || right.prerelease.length === 0) {
    if (left.prerelease.length === right.prerelease.length) return 0;
    return left.prerelease.length === 0 ? 1 : -1;
  }

  const length = Math.max(left.prerelease.length, right.prerelease.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = left.prerelease[index];
    const rightPart = right.prerelease[index];
    if (leftPart === undefined) return -1;
    if (rightPart === undefined) return 1;
    if (leftPart === rightPart) continue;

    const leftNumber = /^\d+$/.test(leftPart) ? Number(leftPart) : null;
    const rightNumber = /^\d+$/.test(rightPart) ? Number(rightPart) : null;
    if (leftNumber !== null && rightNumber !== null) {
      return leftNumber > rightNumber ? 1 : -1;
    }
    if (leftNumber !== null) return -1;
    if (rightNumber !== null) return 1;
    return leftPart > rightPart ? 1 : -1;
  }

  return 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [candidate, current] = process.argv.slice(2);
  if (!candidate || !current) {
    console.error("Usage: node should-update-beta-manifest.mjs <candidate> <current>");
    process.exit(2);
  }
  process.stdout.write(compareSemver(candidate, current) >= 0 ? "true" : "false");
}
