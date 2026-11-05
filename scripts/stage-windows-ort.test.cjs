const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { stageDll, stageMsix, verifyPackage } = require("./stage-windows-ort.cjs");

function fixture(run) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "voicetypr-silero-stage-"));
  const triple = "x86_64-pc-windows-msvc";
  const dll = Buffer.from("MZfixture");
  const manifest = { dll_sha256: { [triple]: createHash("sha256").update(dll).digest("hex") } };
  const options = { target: path.join(root, "target"), triple, manifest, layout: path.join(root, "stage") };
  const source = path.join(options.target, triple, "release/DirectML.dll");
  try { run({ root, dll, options, source }); }
  finally { fs.rmSync(root, { recursive: true, force: true }); }
}

test("NSIS DLL staging rejects missing, changed and wrong-architecture builds", () => fixture(({ dll, options, source }) => {
  const destination = path.join(options.layout, "vcredist");
  assert.throws(() => stageDll({ ...options, destination }), /Missing/);
  fs.mkdirSync(path.dirname(source), { recursive: true });
  fs.writeFileSync(source, "MZnot-the-pinned-DLL");
  assert.throws(() => stageDll({ ...options, destination }), /unrecognized/);
  fs.writeFileSync(source, dll);
  stageDll({ ...options, destination });
  assert.deepEqual(fs.readFileSync(path.join(destination, "DirectML.dll")), dll);
  assert.throws(() => stageDll({ ...options, triple: "wrong-architecture", destination }), /Missing/);
}));

test("MSIX explicitly contains the DLL and every licence, including Microsoft terms", () => fixture(({ dll, options, source }) => {
  fs.mkdirSync(path.dirname(source), { recursive: true });
  fs.writeFileSync(source, dll);
  stageMsix(options);
  verifyPackage(options);
  const licence = path.join(options.layout, "models/onnxruntime/DirectML-LICENSE.txt");
  assert.match(fs.readFileSync(licence, "utf8"), /MICROSOFT SOFTWARE LICENSE TERMS/);
  fs.unlinkSync(licence);
  assert.throws(() => verifyPackage(options), /notice/);
  stageMsix(options);
  fs.writeFileSync(path.join(options.layout, "models/silero/LICENSE"), "wrong");
  assert.throws(() => verifyPackage(options), /notice/);
  stageMsix(options);
  fs.writeFileSync(path.join(options.layout, "onnxruntime.dll"), "unexpected");
  assert.throws(() => verifyPackage(options), /static ORT/);
}));

test("Store build validates both the staged layout and the packed MSIX", () => {
  const source = fs.readFileSync(path.join(__dirname, "build-msix-store.ps1"), "utf8");
  assert.match(source, /--msix-layout/);
  assert.match(source, /ZipFile\]::ExtractToDirectory/);
  assert.match(source, /--check-layout/);
});
