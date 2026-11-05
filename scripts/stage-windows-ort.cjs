#!/usr/bin/env node
// Acquire Microsoft's CPU-only dynamic runtime at packaging time, never from
// Cargo's static ORT output. Copy the signed DLL without modifying or re-signing it.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { inflateRawSync } = require("node:zlib");
const { parseArgs } = require("node:util");
const repo = path.resolve(__dirname, "..");
const provenance = require("../src-tauri/models/onnxruntime/provenance.json");
const noticeFiles = require("../src-tauri/tauri.windows.conf.json").bundle.resources
  .filter((name) => name.startsWith("models/"));
if (noticeFiles.some((name) => /[*?[\]{}]/.test(name))) {
  throw new Error("Windows model notices must be explicit paths, not wildcards.");
}
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const sha256 = (file) => digest(fs.readFileSync(file));

function assertTriple(triple, manifest) {
  if (triple !== "x86_64-pc-windows-msvc" || !manifest.dll_sha256[triple]) {
    throw new Error("Unsupported ONNX Runtime architecture; only Windows x64 is pinned.");
  }
}

function verifyDll(bytes, triple, manifest = provenance) {
  assertTriple(triple, manifest);
  if (digest(bytes) !== manifest.dll_sha256[triple]) {
    throw new Error("Missing or unrecognized onnxruntime.dll. Refusing to package.");
  }
  // Check PE32+ AMD64 DLL and an intact embedded certificate table as well as
  // the exact release hash. The hash preserves Microsoft's Authenticode bytes.
  const pe = bytes.length >= 64 ? bytes.readUInt32LE(0x3c) : 0;
  if (bytes.toString("ascii", 0, 2) !== "MZ" || pe < 64 || pe + 184 > bytes.length
      || bytes.toString("ascii", pe, pe + 4) !== "PE\0\0"
      || bytes.readUInt16LE(pe + 4) !== 0x8664
      || !(bytes.readUInt16LE(pe + 22) & 0x2000)
      || bytes.readUInt16LE(pe + 24) !== 0x20b) {
    throw new Error("onnxruntime.dll is not a Windows x64 PE DLL.");
  }
  const certificate = bytes.readUInt32LE(pe + 24 + 112 + 8 * 4);
  const size = bytes.readUInt32LE(pe + 24 + 112 + 8 * 4 + 4);
  if (!certificate || size < 8 || certificate + size > bytes.length) {
    throw new Error("onnxruntime.dll has no intact Authenticode certificate table.");
  }
}

function rejectDirectML(directory) {
  if (!fs.existsSync(directory)) return;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (/^directml(?:[.-]|$)/i.test(entry.name)) {
      throw new Error("DirectML files are forbidden in CPU-only ORT packages.");
    }
    if (entry.isSymbolicLink()) throw new Error("Package resources must not contain symlinks.");
    if (entry.isDirectory()) rejectDirectML(path.join(directory, entry.name));
  }
}

function verifySourceNotices(manifest) {
  for (const [name, hash] of Object.entries(manifest.notice_sha256)) {
    if (!noticeFiles.includes(`models/onnxruntime/${name}`)) {
      throw new Error("Required ONNX Runtime notice is absent from Windows resources.");
    }
    if (sha256(path.join(repo, "src-tauri/models/onnxruntime", name)) !== hash) {
      throw new Error("ONNX Runtime notice does not match the pinned release archive.");
    }
  }
}

// Read only named members, never extract paths from the archive. The entire ZIP
// is hash-checked first; no PDB, import/static library or provider DLL is staged.
function readArchive(archive, triple, manifest = provenance) {
  assertTriple(triple, manifest);
  const zip = fs.readFileSync(archive);
  if (digest(zip) !== manifest.archive_sha256) {
    throw new Error("ONNX Runtime archive SHA256 mismatch. Refusing to package.");
  }
  const dllMember = `${manifest.archive_root}/${manifest.dll_path}`;
  const wanted = new Map([
    [dllMember, null],
    ...Object.keys(manifest.notice_sha256).map((name) => [`${manifest.archive_root}/${name}`, null]),
  ]);
  let end = zip.length - 22;
  for (; end >= Math.max(0, zip.length - 65557); end--) {
    if (zip.readUInt32LE(end) === 0x06054b50
        && end + 22 + zip.readUInt16LE(end + 20) === zip.length) break;
  }
  if (end < 0 || zip.readUInt32LE(end) !== 0x06054b50) throw new Error("Invalid ORT ZIP directory.");
  let offset = zip.readUInt32LE(end + 16);
  const count = zip.readUInt16LE(end + 10);
  for (let i = 0; i < count; i++) {
    if (zip.readUInt32LE(offset) !== 0x02014b50) throw new Error("Invalid ORT ZIP member.");
    const compressedSize = zip.readUInt32LE(offset + 20);
    const size = zip.readUInt32LE(offset + 24);
    const nameLength = zip.readUInt16LE(offset + 28);
    const name = zip.toString("utf8", offset + 46, offset + 46 + nameLength);
    if (wanted.has(name)) {
      if (wanted.get(name) !== null || size > 32 * 1024 * 1024) throw new Error("Invalid ORT ZIP payload.");
      const local = zip.readUInt32LE(offset + 42);
      if (zip.readUInt32LE(local) !== 0x04034b50) throw new Error("Invalid ORT ZIP header.");
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const compressed = zip.subarray(start, start + compressedSize);
      const method = zip.readUInt16LE(offset + 10);
      const bytes = method === 0 ? compressed
        : method === 8 ? inflateRawSync(compressed, { maxOutputLength: 32 * 1024 * 1024 }) : null;
      if (!bytes || bytes.length !== size) throw new Error("Invalid ORT ZIP compression or size.");
      wanted.set(name, bytes);
    }
    offset += 46 + nameLength + zip.readUInt16LE(offset + 30) + zip.readUInt16LE(offset + 32);
  }
  for (const bytes of wanted.values()) {
    if (bytes === null) throw new Error("Missing required file in ONNX Runtime archive.");
  }
  const dll = wanted.get(dllMember);
  verifyDll(dll, triple, manifest);
  for (const [name, hash] of Object.entries(manifest.notice_sha256)) {
    if (digest(wanted.get(`${manifest.archive_root}/${name}`)) !== hash) {
      throw new Error("ONNX Runtime archive notice SHA256 mismatch.");
    }
  }
  verifySourceNotices(manifest);
  return dll;
}

async function acquireArchive({ archive, target, triple, manifest = provenance }) {
  assertTriple(triple, manifest);
  verifySourceNotices(manifest);
  const cached = archive || path.join(target, "onnxruntime", `${manifest.archive_root}.zip`);
  // An explicit missing archive or a corrupt cache is an error, not a fallback
  // to a different download, local build, version or architecture.
  if (archive || fs.existsSync(cached)) {
    readArchive(cached, triple, manifest);
    return cached;
  }
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "voicetypr-ort-download-"));
  try {
    const response = await fetch(manifest.source, { signal: AbortSignal.timeout(180000) });
    if (!response.ok) throw new Error(`ONNX Runtime download failed (HTTP ${response.status}).`);
    const download = path.join(temporary, "runtime.zip");
    fs.writeFileSync(download, Buffer.from(await response.arrayBuffer()));
    readArchive(download, triple, manifest);
    fs.mkdirSync(path.dirname(cached), { recursive: true });
    fs.copyFileSync(download, cached);
    return cached;
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

function stageDll({ archive, triple, destination, manifest = provenance }) {
  rejectDirectML(destination);
  const dll = readArchive(archive, triple, manifest);
  fs.mkdirSync(destination, { recursive: true });
  fs.writeFileSync(path.join(destination, "onnxruntime.dll"), dll);
  verifyDll(fs.readFileSync(path.join(destination, "onnxruntime.dll")), triple, manifest);
}

function verifyPackage({ layout, triple, manifest = provenance }) {
  rejectDirectML(layout);
  verifyDll(fs.readFileSync(path.join(layout, "onnxruntime.dll")), triple, manifest);
  verifySourceNotices(manifest);
  for (const relative of noticeFiles) {
    const packaged = path.join(layout, relative);
    if (!fs.existsSync(packaged)
        || sha256(packaged) !== sha256(path.join(repo, "src-tauri", relative))) {
      throw new Error(`Missing or modified package notice: ${relative}`);
    }
  }
}

function stageMsix(options) {
  stageDll({ ...options, destination: options.layout });
  for (const relative of noticeFiles) {
    const destination = path.join(options.layout, relative);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(path.join(repo, "src-tauri", relative), destination);
  }
  verifyPackage(options);
}

if (require.main === module) {
  (async () => {
    const { values } = parseArgs({ options: {
      "msix-layout": { type: "string" }, "check-layout": { type: "string" },
      "target-dir": { type: "string" }, triple: { type: "string" },
      archive: { type: "string" },
    } });
    const options = {
      target: path.resolve(values["target-dir"] || process.env.CARGO_TARGET_DIR || path.join(repo, "src-tauri/target")),
      triple: values.triple || process.env.TAURI_ENV_TARGET_TRIPLE || "x86_64-pc-windows-msvc",
    };
    if (values["check-layout"]) {
      verifyPackage({ ...options, layout: path.resolve(values["check-layout"]) });
    } else {
      options.archive = await acquireArchive({
        ...options, archive: values.archive ? path.resolve(values.archive) : undefined,
      });
      if (values["msix-layout"]) {
        stageMsix({ ...options, layout: path.resolve(values["msix-layout"]) });
      } else {
        stageDll({ ...options, destination: path.join(repo, "src-tauri/windows/resources/vcredist") });
      }
    }
    console.log("[stage-windows-ort] Verified signed CPU onnxruntime.dll and pinned notices.");
  })().catch((error) => {
    console.error(`[stage-windows-ort] ${error.message}`);
    process.exitCode = 1;
  });
}
module.exports = { acquireArchive, readArchive, stageDll, stageMsix, verifyDll, verifyPackage };
