#!/usr/bin/env node
// One static ORT for Parakeet and Silero. The pinned prebuilt requires the
// NuGet DirectML redistributable even when only CPU sessions are selected.
const fs = require("node:fs");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { parseArgs } = require("node:util");
const repo = path.resolve(__dirname, "..");
const provenance = require("../src-tauri/models/onnxruntime/DirectML-provenance.json");
const noticeFiles = [
  "models/silero/LICENSE",
  ...fs.readdirSync(path.join(repo, "src-tauri/models/onnxruntime"))
    .map((name) => `models/onnxruntime/${name}`),
];
const sha256 = (file) => createHash("sha256").update(fs.readFileSync(file)).digest("hex");

function verifyDll(file, triple, manifest = provenance) {
  if (!manifest.dll_sha256[triple] || !fs.existsSync(file)
      || sha256(file) !== manifest.dll_sha256[triple]) {
    throw new Error("Missing or unrecognized DirectML.dll. Refusing to package.");
  }
}

function stageDll({ target, triple, destination, manifest = provenance }) {
  const source = [
    path.join(target, triple, "release/DirectML.dll"),
    path.join(target, "release/DirectML.dll"),
  ].find((file) => fs.existsSync(file));
  if (!source) throw new Error("Missing DirectML.dll from the ORT build.");
  verifyDll(source, triple, manifest);
  fs.mkdirSync(destination, { recursive: true });
  fs.copyFileSync(source, path.join(destination, "DirectML.dll"));
}

function verifyPackage({ layout, triple, manifest = provenance }) {
  verifyDll(path.join(layout, "DirectML.dll"), triple, manifest);
  if (fs.existsSync(path.join(layout, "onnxruntime.dll"))) {
    throw new Error("A static ORT package must not contain onnxruntime.dll.");
  }
  for (const relative of noticeFiles) {
    const packaged = path.join(layout, relative);
    if (!fs.existsSync(packaged)
        || sha256(packaged) !== sha256(path.join(repo, "src-tauri", relative))) {
      throw new Error(`Missing or modified package notice: ${relative}`);
    }
  }
  for (const [name, hash] of Object.entries(provenance.notice_sha256)) {
    if (sha256(path.join(layout, "models/onnxruntime", name)) !== hash) {
      throw new Error("DirectML redistribution terms do not match the pinned package.");
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
  try {
    const { values } = parseArgs({ options: {
      "msix-layout": { type: "string" }, "check-layout": { type: "string" },
      "target-dir": { type: "string" }, triple: { type: "string" },
    } });
    const options = {
      target: path.resolve(values["target-dir"] || process.env.CARGO_TARGET_DIR || "src-tauri/target"),
      triple: values.triple || process.env.TAURI_ENV_TARGET_TRIPLE || "x86_64-pc-windows-msvc",
    };
    if (values["check-layout"]) {
      verifyPackage({ ...options, layout: path.resolve(values["check-layout"]) });
    } else if (values["msix-layout"]) {
      stageMsix({ ...options, layout: path.resolve(values["msix-layout"]) });
    } else {
      stageDll({ ...options, destination: path.resolve("src-tauri/windows/resources/vcredist") });
    }
    console.log("[stage-windows-ort] Verified app-local DirectML and requested notices.");
  } catch (error) {
    console.error(`[stage-windows-ort] ${error.message}`);
    process.exitCode = 1;
  }
}
module.exports = { stageDll, stageMsix, verifyPackage };
