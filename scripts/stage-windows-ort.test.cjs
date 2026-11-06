const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { deflateRawSync } = require("node:zlib");
const {
  acquireArchive, readArchive, stageDll, stageMsix, verifyDll, verifyPackage,
} = require("./stage-windows-ort.cjs");
const provenance = require("../src-tauri/models/onnxruntime/provenance.json");
const repo = path.resolve(__dirname, "..");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

function peDll() {
  const bytes = Buffer.alloc(512);
  bytes.write("MZ");
  bytes.writeUInt32LE(64, 0x3c);
  bytes.write("PE\0\0", 64);
  bytes.writeUInt16LE(0x8664, 68);
  bytes.writeUInt16LE(0x2000, 86);
  bytes.writeUInt16LE(0x20b, 88);
  bytes.writeUInt32LE(504, 64 + 24 + 112 + 32);
  bytes.writeUInt32LE(8, 64 + 24 + 112 + 36);
  bytes.write("SIGNED!!", 504);
  return bytes;
}

// Tiny ZIP fixture, with stored and deflated entries. Hashes authenticate these
// bytes just as the production release hash authenticates Microsoft's ZIP.
function zip(entries) {
  const local = [];
  const central = [];
  let offset = 0;
  for (const [name, bytes] of entries) {
    const filename = Buffer.from(name);
    const method = name.endsWith(".dll") ? 0 : 8;
    const compressed = method === 0 ? bytes : deflateRawSync(bytes);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(method, 8);
    header.writeUInt32LE(compressed.length, 18);
    header.writeUInt32LE(bytes.length, 22);
    header.writeUInt16LE(filename.length, 26);
    const record = Buffer.alloc(46);
    record.writeUInt32LE(0x02014b50);
    record.writeUInt16LE(20, 4);
    record.writeUInt16LE(20, 6);
    record.writeUInt16LE(method, 10);
    record.writeUInt32LE(compressed.length, 20);
    record.writeUInt32LE(bytes.length, 24);
    record.writeUInt16LE(filename.length, 28);
    record.writeUInt32LE(offset, 42);
    local.push(header, filename, compressed);
    central.push(record, filename);
    offset += header.length + filename.length + compressed.length;
  }
  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, directory, end]);
}

async function fixture(run) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "voicetypr-silero-stage-"));
  const triple = "x86_64-pc-windows-msvc";
  const dll = peDll();
  const manifest = {
    ...provenance, dll_sha256: { [triple]: hash(dll) },
  };
  const entries = [
    [`${manifest.archive_root}/lib/onnxruntime.dll`, dll],
    ...Object.keys(manifest.notice_sha256).map((name) => [
      `${manifest.archive_root}/${name}`,
      fs.readFileSync(path.join(repo, "src-tauri/models/onnxruntime", name)),
    ]),
    [`${manifest.archive_root}/lib/onnxruntime.lib`, Buffer.from("never stage this")],
    [`${manifest.archive_root}/lib/onnxruntime_providers_shared.dll`, Buffer.from("not needed for CPU")],
  ];
  const archive = path.join(root, "runtime.zip");
  function writeArchive(nextEntries = entries) {
    const bytes = zip(nextEntries);
    fs.writeFileSync(archive, bytes);
    manifest.archive_sha256 = hash(bytes);
    return bytes;
  }
  writeArchive();
  const options = { archive, target: path.join(root, "target"), triple, manifest, layout: path.join(root, "stage") };
  try { await run({ root, dll, entries, options, writeArchive }); }
  finally { fs.rmSync(root, { recursive: true, force: true }); }
}

test("NSIS stages only the pinned DLL, preserving all bytes including signature", () => fixture(({ dll, options }) => {
  const destination = path.join(options.layout, "vcredist");
  stageDll({ ...options, destination });
  assert.deepEqual(fs.readdirSync(destination), ["onnxruntime.dll"]);
  assert.deepEqual(fs.readFileSync(path.join(destination, "onnxruntime.dll")), dll);
  fs.writeFileSync(path.join(destination, "DirectML.dll"), "stale static build");
  assert.throws(() => stageDll({ ...options, destination }), /DirectML/);
}));

test("archive staging fails closed on missing, corrupt and wrong-architecture inputs", () => fixture(({ options }) => {
  assert.throws(() => readArchive(`${options.archive}.missing`, options.triple, options.manifest), /ENOENT/);
  assert.throws(() => readArchive(options.archive, "aarch64-pc-windows-msvc", options.manifest), /architecture/);
  assert.throws(() => readArchive(options.archive, options.triple, {
    ...options.manifest, dll_path: "lib/missing.dll",
  }), /Missing required/);
  fs.appendFileSync(options.archive, "corrupt");
  assert.throws(() => readArchive(options.archive, options.triple, options.manifest), /SHA256/);
  assert.equal(fs.existsSync(options.layout), false);
}));

test("archive validates the DLL and exact notices independently of the ZIP pin", () => fixture(({ entries, options, writeArchive }) => {
  writeArchive(entries.map(([name, bytes]) => [name, name.endsWith(".dll") ? Buffer.from("wrong") : bytes]));
  assert.throws(() => readArchive(options.archive, options.triple, options.manifest), /unrecognized/);
  writeArchive(entries.filter(([name]) => !name.endsWith("/LICENSE")));
  assert.throws(() => readArchive(options.archive, options.triple, options.manifest), /Missing required/);
  writeArchive(entries.map(([name, bytes]) => [name, name.endsWith("ThirdPartyNotices.txt") ? Buffer.from("wrong") : bytes]));
  assert.throws(() => readArchive(options.archive, options.triple, options.manifest), /notice SHA256/);
}));

test("PE validation rejects wrong architecture, non-DLL and lost signature even with matching hash", () => fixture(({ dll, options }) => {
  const mutations = [
    [(bytes) => bytes.writeUInt16LE(0xaa64, 68), /x64 PE DLL/],
    [(bytes) => bytes.writeUInt16LE(0, 86), /x64 PE DLL/],
    [(bytes) => bytes.writeUInt32LE(0, 64 + 24 + 112 + 32), /certificate/],
    [(bytes) => bytes.writeUInt32LE(512, 64 + 24 + 112 + 36), /certificate/],
  ];
  for (const [mutate, message] of mutations) {
    const bytes = Buffer.from(dll);
    mutate(bytes);
    const manifest = { ...options.manifest, dll_sha256: { [options.triple]: hash(bytes) } };
    assert.throws(() => verifyDll(bytes, options.triple, manifest), message);
  }
}));

test("MSIX root DLL and every notice are required and hash-verified", () => fixture(({ dll, options }) => {
  stageMsix(options);
  verifyPackage(options);
  assert.deepEqual(fs.readFileSync(path.join(options.layout, "onnxruntime.dll")), dll);
  const notices = [
    "models/silero/LICENSE",
    ...["LICENSE", "ThirdPartyNotices.txt", "ort-LICENSE-MIT", "README.txt", "provenance.json"]
      .map((name) => `models/onnxruntime/${name}`),
  ];
  for (const relative of notices) {
    const file = path.join(options.layout, relative);
    fs.unlinkSync(file);
    assert.throws(() => verifyPackage(options), /notice/);
    stageMsix(options);
    fs.appendFileSync(file, "modified");
    assert.throws(() => verifyPackage(options), /notice/);
    stageMsix(options);
  }
  fs.appendFileSync(path.join(options.layout, "onnxruntime.dll"), "re-signed");
  assert.throws(() => verifyPackage(options), /unrecognized/);
  stageMsix(options);
  fs.unlinkSync(path.join(options.layout, "onnxruntime.dll"));
  assert.throws(() => verifyPackage(options), /ENOENT/);
}));

test("MSIX rejects DirectML anywhere, including old notice files and mixed case", () => fixture(({ options }) => {
  stageMsix(options);
  for (const relative of ["dIrEcTmL.DlL", "models/onnxruntime/DirectML-LICENSE.txt"]) {
    const file = path.join(options.layout, relative);
    fs.writeFileSync(file, "obsolete");
    assert.throws(() => verifyPackage(options), /DirectML/);
    assert.throws(() => stageMsix(options), /DirectML/);
    fs.unlinkSync(file);
  }
}));

test("acquisition downloads once, verifies before caching and rejects a corrupt cache", (t) => fixture(async ({ options }) => {
  const bytes = fs.readFileSync(options.archive);
  let downloads = 0;
  t.mock.method(globalThis, "fetch", async (url) => {
    downloads++;
    assert.equal(url, provenance.source);
    return new Response(bytes);
  });
  const acquire = { ...options, archive: undefined };
  const cached = await acquireArchive(acquire);
  assert.deepEqual(fs.readFileSync(cached), bytes);
  assert.equal(downloads, 1);
  assert.equal(await acquireArchive(acquire), cached);
  assert.equal(downloads, 1);
  fs.appendFileSync(cached, "corrupt");
  await assert.rejects(acquireArchive(acquire), /SHA256/);
  await assert.rejects(acquireArchive({ ...options, archive: `${options.archive}.missing` }), /ENOENT/);
  await assert.rejects(acquireArchive({ ...acquire, triple: "i686-pc-windows-msvc" }), /architecture/);
  assert.equal(downloads, 1);
}));

test("failed or changed downloads never populate the cache", (t) => fixture(async ({ options }) => {
  const acquire = { ...options, archive: undefined };
  const cached = path.join(options.target, "onnxruntime", `${provenance.archive_root}.zip`);
  t.mock.method(globalThis, "fetch", async () => new Response("error", { status: 404 }));
  await assert.rejects(acquireArchive(acquire), /HTTP 404/);
  assert.equal(fs.existsSync(cached), false);
  globalThis.fetch = async () => new Response("changed upstream");
  await assert.rejects(acquireArchive(acquire), /SHA256/);
  assert.equal(fs.existsSync(cached), false);
}));

test("Windows resources explicitly include the verified notices, not a legacy wildcard", () => {
  for (const name of ["tauri.windows.conf.json"]) {
    const { bundle } = JSON.parse(fs.readFileSync(path.join(repo, "src-tauri", name), "utf8"));
    const notices = bundle.resources.filter((file) => file.startsWith("models/onnxruntime/"));
    assert.deepEqual(notices, ["LICENSE", "ThirdPartyNotices.txt", "ort-LICENSE-MIT", "README.txt", "provenance.json"]
      .map((file) => `models/onnxruntime/${file}`));
  }
  const shared = JSON.parse(fs.readFileSync(path.join(repo, "src-tauri/tauri.conf.json"), "utf8"));
  assert.equal(shared.bundle.resources.some((file) => file.startsWith("models/onnxruntime/")), false);
  const hooks = fs.readFileSync(path.join(repo, "src-tauri/windows/installer-hooks.nsh"), "utf8");
  assert.match(hooks, /CopyFiles \/SILENT "\$INSTDIR\\windows\\resources\\vcredist\\\$R1" "\$INSTDIR\\\$R1"/);
  assert.match(hooks, /\$\{OrIfNot\} \$\{FileExists\} "\$INSTDIR\\onnxruntime\.dll"/);
});

test("Store generates a MAP with preserved flags and checks layout plus packed ZIP", () => {
  const source = fs.readFileSync(path.join(__dirname, "build-msix-store.ps1"), "utf8");
  assert.match(source, /--msix-layout/);
  assert.match(source, /ZipFile\]::ExtractToDirectory/);
  assert.match(source, /--check-layout/);
  assert.match(source, /-MapPath \$MainMap/);
  assert.match(source, /link-arg=\/MAP:\$MainMap/);
  assert.match(source, /\$OriginalEncodedRustFlags = \$env:CARGO_ENCODED_RUSTFLAGS/);
  assert.match(source, /\$env:RUSTFLAGS -split/);
  assert.match(source, /target-feature=\+crt-static/);
});

function psRegex(source, marker) {
  const line = source.split("\n").find((value) => value.includes(marker));
  const pattern = /'(\(\?[a-z]+\)[^']*)'/.exec(line)?.[1];
  assert.ok(pattern, `Missing PowerShell pattern: ${marker}`);
  const flags = /^\(\?([a-z]+)\)/.exec(pattern)[1];
  return new RegExp(pattern.replace(/^\(\?[a-z]+\)/, ""), flags);
}

test("MAP public-symbol pattern rejects a static definition but permits loader strings", () => {
  const source = fs.readFileSync(path.join(repo, "src-tauri/windows/assert-no-onnxruntime-import.ps1"), "utf8");
  const definition = psRegex(source, "\\s+OrtGetApiBase\\s+");
  assert.ok(definition.test(" 0001:00000100       OrtGetApiBase       0000000140001100 f   onnxruntime:api.obj"));
  for (const value of [
    "OrtGetApiBase", // GetProcAddress string, not a native definition
    " 0002:00000100       ??_C@_0O@abcdef@OrtGetApiBase?$AA@ 0000000140002100 voicetypr.obj",
    " 0001:00000100       _ZN3ort13OrtGetApiBase17h0123E 0000000140001100 f voicetypr.obj",
  ]) assert.equal(definition.test(value), false);
  assert.match(source, /\[Parameter\(Mandatory = \$true\)\]\s*\[string\]\$MapPath/);
  assert.match(source, /Test-Path -LiteralPath \$MapPath -PathType Leaf/);
  assert.match(source, /GetFileNameWithoutExtension\(\$ExePath\)/);
  assert.match(source, /ToUInt32\(\$pe, \$header \+ 8\)/);
  assert.match(source, /ToUInt64\(\$pe, \$header \+ 48\)/);
  const imageSuffix = /\+ '([^']*\[0-9a-f\][^']*)'/.exec(source)?.[1];
  assert.ok(imageSuffix, "Cargo-linked main image suffix must be accepted");
  const image = new RegExp(`^voicetypr${imageSuffix}`, "i");
  for (const name of ["voicetypr", "voicetypr-0123456789abcdef"]) assert.ok(image.test(name));
  for (const name of ["build_script_build", "whisper-vulkan-sidecar", "voicetypr_lib", "voicetypr-junk"]) {
    assert.equal(image.test(name), false);
  }
});

test("native import pattern rejects ORT and DirectML, not unrelated system imports", () => {
  const source = fs.readFileSync(path.join(repo, "src-tauri/windows/assert-no-onnxruntime-import.ps1"), "utf8");
  const imports = psRegex(source, "DirectML)\\");
  for (const value of ["    onnxruntime.dll", " DirectML.dll", " ONNXRUNTIME.DLL", " onnxruntime_providers_shared.dll"]) {
    assert.ok(imports.test(value), value);
  }
  assert.equal(imports.test("    KERNEL32.dll"), false);
});

test("every Windows artifact uploader runs the Ivy Bridge gate first", () => {
  let uploaders = 0;
  const ungated = [];
  for (const file of ["ci.yml", "native-ci.yml", "release.yml", "store-msix.yml", "windows-check.yml"]) {
    const source = fs.readFileSync(path.join(repo, ".github/workflows", file), "utf8");
    for (const job of source.split(/(?=^  [\w-]+:)/m)) {
      const upload = job.indexOf("uses: actions/upload-artifact@");
      if (!/runs-on:\s*windows-/.test(job) || upload < 0) continue;
      uploaders += 1;
      const smoke = job.indexOf("uses: ./.github/actions/ivy-bridge-smoke");
      if (smoke < 0 || smoke > upload) ungated.push(file);
      else assert.match(job.slice(smoke, upload), /timeout-minutes: 15/);
    }
  }
  assert.ok(uploaders >= 3, "Expected preview, release and Store Windows uploaders");
  assert.deepEqual(ungated, []);
});

test("Mac native caches and RUSTFLAGS preserve the CPU VAD patch epoch", () => {
  let jobs = 0;
  for (const file of ["ci.yml", "native-ci.yml", "release.yml"]) {
    // Windows checkouts may convert workflows to CRLF; the job and cache patterns assume LF.
    const source = fs.readFileSync(path.join(repo, ".github/workflows", file), "utf8").replace(/\r\n/g, "\n");
    for (const job of source.split(/(?=^  [\w-]+:)/m)) {
      if (!/runs-on:\s*macos-/.test(job) || !job.includes("uses: Swatinem/rust-cache@")) continue;
      jobs += 1;
      const cache = job.match(/uses: Swatinem\/rust-cache@[^\n]*\n\s+with:\n([\s\S]*?)(?=\n      -|$)/)?.[1];
      assert.match(cache ?? "", /prefix-key: v0-rust-cpu-vad-v1/);
      const overrides = job.split("\n").filter((line) => /echo "RUSTFLAGS=/.test(line));
      assert.ok(overrides.length > 0);
      for (const line of overrides) assert.match(line, /--cfg voicetypr_vad_cpu_only_v1/);
    }
  }
  assert.equal(jobs, 4, "Expected CI Intel, native ARM, release ARM and release Intel");
});
