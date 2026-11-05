const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

test("ORT packaging fails closed without its DLL and stages the target-triple build", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "voicetypr-silero-stage-"));
  try {
    const target = path.join(root, "target");
    const script = path.join(__dirname, "stage-windows-ort.cjs");
    const options = {
      cwd: root,
      env: { ...process.env, CARGO_TARGET_DIR: target, TAURI_ENV_TARGET_TRIPLE: "x86_64-pc-windows-msvc" },
      encoding: "utf8",
    };
    assert.equal(spawnSync(process.execPath, [script], options).status, 1);
    const release = path.join(target, "x86_64-pc-windows-msvc", "release");
    fs.mkdirSync(release, { recursive: true });
    fs.writeFileSync(path.join(release, "DirectML.dll"), "MZfixture");
    assert.equal(spawnSync(process.execPath, [script], options).status, 0);
    assert.equal(fs.readFileSync(path.join(root, "src-tauri/windows/resources/vcredist/DirectML.dll"), "utf8"), "MZfixture");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
