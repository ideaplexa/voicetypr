#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { closeSync, copyFileSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, realpathSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { basename, isAbsolute, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { CASES, PARAKEET_CACHE_SUBDIRS, compareReports, csv, parseArgs, positiveNumber, profilePaths, seedSettings, wer, words } from "./helpers.mjs";

const ROOT = resolve(import.meta.dirname, "..", "..");
const REAL_HOME = process.env.HOME;
const READY_LINE = "App state managed and ready";
const HELP = `Usage: node scripts/e2e/dictation-e2e.mjs --clips <dir> --engines <engine:model,...> [options]

  --check-setup                 Check BlackHole, ffmpeg, cliclick, macOS permissions
  --clips <dir>                 Directory with manifest.json [{file,lang,ref}]
  --engines <engine:model,...>  Engine selections, e.g. parakeet:parakeet-tdt-0.6b-v3
  --engine <name> --model <id>  Single selection alternative
  --cases <csv>                 ${CASES.join(",")}
  --bin <path>                  Default: src-tauri/target/debug/voicetypr
  --secure-store <path>         Separate encrypted secure.dat for cloud keys
  --tail-ms <ms>                Silence after playback before stop (default 250)
  --timeout-ms <ms>             Per-case text timeout (default 30000)
  --baseline <report.json>      Fail if WER or stop-to-text regresses
  --wer-threshold <fraction>    Allowed absolute WER increase (default 0.05)
  --latency-threshold-ms <ms>   Allowed stop-to-text increase (default 250)
  --help                        Show this message`;

function fail(message) { throw new Error(message); }
function command(program, args, options = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 30000, maxBuffer: 8 * 1024 * 1024, ...options });
  if (result.error) fail(`${program} ${args.join(" ")}: ${result.error.message}`);
  if (result.status !== 0 && !options.allowFailure) {
    fail(`${program} ${args.join(" ")} exited ${result.status}: ${(result.stderr || result.stdout || "").trim().slice(-1200)}`);
  }
  return `${result.stdout || ""}\n${result.stderr || ""}`;
}
function hasCommand(name) {
  const result = spawnSync("/usr/bin/which", [name], { encoding: "utf8" });
  return result.status === 0;
}
function deviceIndex(text, name) {
  const line = text.split(/\r?\n/u).find((item) => item.includes(name) && /\[\d+\]/u.test(item));
  return line ? Number(line.match(/\[(\d+)\]/u)[1]) : null;
}
function checkSetup() {
  if (process.platform !== "darwin") fail("This harness currently requires macOS 14+.");
  const issues = [];
  let outputIndex = null;
  if (!hasCommand("ffmpeg")) issues.push("ffmpeg is missing. Install it with: brew install ffmpeg");
  else {
    const inputList = command("ffmpeg", ["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""], { allowFailure: true });
    if (!/AVFoundation audio devices:/u.test(inputList) || deviceIndex(inputList, "BlackHole 2ch") === null) {
      issues.push("BlackHole 2ch is absent from ffmpeg AVFoundation audio inputs. Install BlackHole 2ch, run `sudo killall coreaudiod` (or restart macOS), and check System Settings → Sound → Input.");
    }
    const outputList = command("ffmpeg", ["-hide_banner", "-f", "lavfi", "-i", "anullsrc", "-t", "0", "-f", "audiotoolbox", "-list_devices", "true", "-"], { allowFailure: true });
    outputIndex = deviceIndex(outputList, "BlackHole 2ch");
    if (outputIndex === null) issues.push("BlackHole 2ch is absent from ffmpeg AudioToolbox outputs. Install BlackHole 2ch, run `sudo killall coreaudiod` (or restart macOS), and check System Settings → Sound → Output.");
  }
  if (!hasCommand("cliclick")) issues.push("cliclick is missing. Install it with: brew install cliclick");
  else {
    try { command("cliclick", ["kd:shift", "ku:shift"]); }
    catch (error) { issues.push(`cliclick could not post a harmless Shift key event: ${error.message}. Enable the calling Terminal or agent host in System Settings → Privacy & Security → Accessibility, then restart that host.`); }
  }
  if (!hasCommand("osascript")) issues.push("osascript is missing. macOS must provide /usr/bin/osascript.");
  else {
    try { command("osascript", ["-e", 'tell application "TextEdit" to get count of documents']); }
    catch (error) { issues.push(`TextEdit Automation probe failed: ${error.message}. Enable the calling Terminal or agent host in System Settings → Privacy & Security → Automation → TextEdit, then rerun --check-setup.`); }
  }
  if (!hasCommand("screencapture")) issues.push("screencapture is missing. macOS must provide /usr/sbin/screencapture.");
  if (issues.length) fail(`Setup incomplete:\n- ${issues.join("\n- ")}`);
  console.log(`Setup OK: BlackHole 2ch input and AudioToolbox output index ${outputIndex}; ffmpeg, cliclick, Accessibility and TextEdit Automation responded.`);
  return outputIndex;
}
function manifest(dir) {
  const path = join(dir, "manifest.json");
  if (!existsSync(path)) fail(`Missing ${path}. Expected a JSON array of {file,lang,ref}.`);
  let rows;
  try { rows = JSON.parse(readFileSync(path, "utf8")); }
  catch (error) { fail(`Cannot parse ${path}: ${error.message}`); }
  if (!Array.isArray(rows) || rows.length < 1) fail("manifest.json needs at least one real-speech clip.");
  return rows.map((row, index) => {
    for (const key of ["file", "lang", "ref"]) {
      if (typeof row?.[key] !== "string" || !row[key].trim()) fail(`manifest.json entry ${index + 1} needs nonempty ${key}`);
    }
    if (!/^[a-zA-Z0-9_-]+$/u.test(row.lang)) fail(`manifest.json entry ${index + 1} has invalid language code`);
    const file = isAbsolute(row.file) ? row.file : resolve(dir, row.file);
    if (!existsSync(file) || !statSync(file).isFile()) fail(`Clip does not exist: ${file}`);
    return { ...row, file };
  });
}
function linkTree(source, target) {
  if (!existsSync(source)) return;
  mkdirSync(target, { recursive: true });
  for (const entry of readdirSync(source, { withFileTypes: true })) {
    const from = join(source, entry.name);
    const to = join(target, entry.name);
    if (entry.isDirectory()) linkTree(from, to);
    else if (entry.isFile() || entry.isSymbolicLink()) symlinkSync(from, to);
  }
}
function prepareProfile(home, selection, livePreview, language, secureStore) {
  const paths = profilePaths(home);
  if (language !== "en" && ((selection.engine === "whisper" && selection.model.endsWith(".en")) || (selection.engine === "parakeet" && /-v2$|-unified-/u.test(selection.model)))) {
    fail(`${selection.engine}/${selection.model} is English-only but the manifest contains ${language}. Select a multilingual model.`);
  }
  mkdirSync(paths.data, { recursive: true });
  writeFileSync(paths.settings, `${JSON.stringify(seedSettings({ ...selection, livePreview, language }), null, 2)}\n`);
  const real = profilePaths(REAL_HOME);
  linkTree(real.whisperModels, paths.whisperModels);
  linkTree(real.fluidModels, paths.fluidModels);
  if (selection.engine === "whisper" && !existsSync(join(paths.whisperModels, `${selection.model}.bin`))) {
    fail(`Whisper model ${selection.model}.bin is missing in ${real.whisperModels}. Download it in Voicetypr first.`);
  }
  if (selection.engine === "parakeet" && !PARAKEET_CACHE_SUBDIRS[selection.model]) {
    fail(`Unknown Parakeet model ${selection.model}. Check src-tauri/src/parakeet/models.rs.`);
  }
  if (selection.engine === "parakeet" && !existsSync(join(paths.fluidModels, PARAKEET_CACHE_SUBDIRS[selection.model]))) {
    fail(`FluidAudio model ${selection.model} is missing in ${real.fluidModels}. Download it in Voicetypr first.`);
  }
  const isCloud = !new Set(["whisper", "parakeet"]).has(selection.engine);
  if (isCloud && !new Set(["soniox", "deepgram", "openai", "groq", "cohere"]).has(selection.engine)) fail(`Unknown engine ${selection.engine}.`);
  if (isCloud && !secureStore) fail(`${selection.engine} requires --secure-store <path> with its encrypted API key.`);
  // The secure store also carries the license, so local engines use it too when given.
  if (secureStore) {
    const source = resolve(secureStore);
    if (!existsSync(source) || !statSync(source).isFile()) fail(`Secure store file does not exist: ${source}`);
    if (realpathSync(source) === join(real.data, "secure.dat")) fail("Provide a separate secure.dat file, not the normal Voicetypr profile's secure store.");
    let values;
    try { values = JSON.parse(readFileSync(source, "utf8")); }
    catch { fail(`Secure store file is not valid JSON: ${source}`); }
    if (isCloud && typeof values?.[`stt_api_key_${selection.engine}`] !== "string") fail(`Separate secure.dat has no encrypted stt_api_key_${selection.engine} entry.`);
    copyFileSync(source, join(paths.data, "secure.dat"));
  }
  return paths;
}
function logLines(paths) {
  if (!existsSync(paths.logs)) return [];
  return readdirSync(paths.logs).filter((name) => /^voicetypr-\d{4}-\d{2}-\d{2}\.log(?:\.\d+)?$/u.test(name))
    .sort().flatMap((name) => readFileSync(join(paths.logs, name), "utf8").split(/\r?\n/u));
}
async function until(check, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const result = check();
    if (result) return result;
    await delay(150);
  }
  fail(`Timed out waiting for ${label} after ${timeoutMs} ms.`);
}
async function launch(bin, home, paths) {
  const fd = openSync(join(home, "app-stdout.log"), "w");
  // Tauri's dirs crate follows HOME; Foundation's homeDirectoryForCurrentUser
  // (used by FluidAudio's Swift sidecar) follows CFFIXED_USER_HOME on macOS.
  const child = spawn(bin, [], { env: { ...process.env, HOME: home, CFFIXED_USER_HOME: home }, stdio: ["ignore", fd, fd] });
  closeSync(fd);
  let spawnError;
  child.on("error", (error) => { spawnError = error; });
  try {
    await until(() => {
      if (spawnError) fail(`Could not launch ${bin}: ${spawnError.message}`);
      if (child.exitCode !== null) fail(`Voicetypr exited before ready (${child.exitCode}). See ${join(home, "app-stdout.log")}`);
      const lines = logLines(paths);
      const modelPathLine = lines.find((line) => line.includes("Models directory:"));
      if (modelPathLine && !modelPathLine.includes(paths.whisperModels)) {
        fail(`Voicetypr resolved a non-isolated model path. Aborting before any cases: ${modelPathLine}`);
      }
      return modelPathLine && lines.some((line) => line.includes(READY_LINE));
    }, 30000, "Voicetypr startup log");
  } catch (error) {
    child.kill("SIGTERM");
    throw error;
  }
  return child;
}
function appleScript(source) { return command("osascript", ["-e", source]).trim(); }
// The harness only ever touches the TextEdit document it opened: an empty local
// file under the run directory, so nothing is autosaved into the user's iCloud
// TextEdit folder, and it is closed without saving after each case.
let testDocument = null;
let documentCounter = 0;
async function newDocument(out) {
  documentCounter += 1;
  const file = join(out, `target-${documentCounter}.txt`);
  writeFileSync(file, "");
  testDocument = basename(file);
  command("open", ["-a", "TextEdit", file]);
  await until(() => appleScript(`tell application "TextEdit" to get (exists document "${testDocument}")`) === "true", 10000, "TextEdit test document");
  appleScript(`tell application "TextEdit"\nactivate\nset index of (first window whose name is "${testDocument}") to 1\nend tell`);
  if (documentText()) fail("TextEdit test document was not empty.");
}
function closeDocument() {
  if (!testDocument) return;
  try { appleScript(`tell application "TextEdit" to close (first document whose name is "${testDocument}") saving no`); } catch { /* already closed */ }
  testDocument = null;
}
function documentText() { return appleScript(`tell application "TextEdit" to get text of document "${testDocument}"`); }
// Voicetypr's key engine taps the HID stream, which never sees cliclick's
// session-level events; scripts/e2e/hidkey.swift posts at the HID tap instead.
let hidKeyBinary = null;
function hidKey(args) {
  if (!hidKeyBinary) {
    const outDir = join(ROOT, ".tmp", "e2e");
    mkdirSync(outDir, { recursive: true });
    hidKeyBinary = join(outDir, "hidkey");
    command("xcrun", ["swiftc", "-O", join(import.meta.dirname, "hidkey.swift"), "-o", hidKeyBinary], { timeout: 120000 });
  }
  command(hidKeyBinary, args);
}
function hotkey() { hidKey(["101", "ctrl", "alt"]); } // Control+Alt+F9 (HOTKEY)
// Voicetypr cancels on a double-tap of Escape (the first tap only arms it).
function escape() { hidKey(["53"]); spawnSync("sleep", ["0.3"]); hidKey(["53"]); }
function buildWindowFinder(out) {
  const swift = `import CoreGraphics\nimport Foundation\nlet owner = Int32(CommandLine.arguments.last!)!\nlet list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)! as! [[String: Any]]\nlet windows = list.filter { ($0[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == owner }.filter { ($0[kCGWindowBounds as String] as? [String: Double]).map { let w = $0["Width"] ?? 0; let h = $0["Height"] ?? 0; return (200.0...600.0).contains(w) && (40.0...180.0).contains(h) && (3.8...4.3).contains(w / h) } ?? false }\nif let id = windows.compactMap({ ($0[kCGWindowNumber as String] as? NSNumber)?.intValue }).first { print(id) }`;
  const source = join(out, "pill-window-id.swift");
  const binary = join(out, "pill-window-id");
  writeFileSync(source, swift);
  command("xcrun", ["swiftc", source, "-o", binary], { timeout: 120000 });
  return binary;
}
// Screenshots are evidence, not a pass condition: without Screen Recording for
// the calling host they are skipped with a warning instead of failing the case.
let screenshotWarned = false;
function pillScreenshot(windowFinder, pid, destination) {
  try {
    const id = command(windowFinder, [String(pid)]).trim();
    if (!/^\d+$/u.test(id)) throw new Error("pill window not found");
    // Prefer the Cap CLI (it has its own Screen Recording grant); fall back to screencapture.
    const cap = join(process.env.HOME ?? "", ".cap", "bin", "cap");
    if (existsSync(cap)) command(cap, ["screenshot", "--window", id, "--path", destination]);
    else command("screencapture", ["-x", "-l", id, destination]);
    if (!existsSync(destination) || statSync(destination).size === 0) throw new Error("empty screenshot");
    return destination;
  } catch (error) {
    if (!screenshotWarned) {
      console.warn(`Pill screenshot skipped (${error.message}). Allow Screen Recording for the calling host to capture it.`);
      screenshotWarned = true;
    }
    return null;
  }
}
async function playClip(file, outputIndex, pid, screenshot, windowFinder, onSpawn = () => {}) {
  const child = spawn("ffmpeg", ["-nostdin", "-hide_banner", "-loglevel", "error", "-re", "-i", file, "-f", "audiotoolbox", "-audio_device_index", String(outputIndex), "-"], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr += chunk.toString(); });
  const done = new Promise((accept, reject) => {
    child.once("error", reject);
    child.once("exit", (code) => code === 0 ? accept() : reject(new Error(`ffmpeg playback exited ${code}: ${stderr.trim()}`)));
  });
  try {
    onSpawn();
    await delay(350);
    if (child.exitCode === null) pillScreenshot(windowFinder, pid, screenshot);
    else fail("Clip ended before the pill screenshot could be taken; use a longer real-speech clip.");
    await done;
  } catch (error) {
    if (child.exitCode === null) child.kill("SIGTERM");
    await done.catch(() => {});
    throw error;
  }
}
async function stableText(timeoutMs, baseline, expectChange) {
  let previous = baseline;
  let since = Date.now();
  let firstChangedAt = null;
  return until(() => {
    const current = documentText();
    if (current !== previous) { previous = current; since = Date.now(); if (current !== baseline && firstChangedAt === null) firstChangedAt = since; }
    if ((expectChange ? current !== baseline : current === baseline) && Date.now() - since >= 500) return { text: current, at: firstChangedAt ?? since };
    return null;
  }, timeoutMs, "stable TextEdit text");
}
function timing(lines) {
  const entries = lines.filter((line) => line.includes("[REC TIMING]"));
  const first = entries.find((line) => line.includes("start_to_first_audio_ms"));
  const match = first?.match(/start_to_first_audio_ms[^\d]*(\d+)/u);
  return { start_to_first_audio_ms: match ? Number(match[1]) : null, rec_timing: entries.map((line) => line.slice(line.indexOf("[REC TIMING]"))) };
}
async function runCase(options) {
  try {
    return await runCaseInDocument(options);
  } finally {
    closeDocument();
  }
}
async function runCaseInDocument({ kind, clips, outputIndex, pid, paths, out, windowFinder, timeoutMs, tailMs, selection, livePreview, ordinal }) {
  await newDocument(out);
  const rows = kind === "back-to-back" ? [clips[0], clips[1] ?? clips[0]] : [clips[0]];
  const language = clips[0].lang;
  const textBefore = documentText();
  const logBefore = logLines(paths).length;
  let stoppedAt = null;
  let result;
  let screenshot = null;
  for (let index = 0; index < (kind === "no-speech" ? 1 : rows.length); index += 1) {
    const beforePass = documentText();
    const startLogOffset = logLines(paths).length;
    // first-word: speak as soon as the cue fires (a user reacting to it).
    // instant-speech: speech starts at the same instant as the key press; it
    // measures what a pre-roll buffer would recover, not a pass condition.
    if (kind !== "instant-speech") hotkey();
    const started = () => until(() => {
      const recent = logLines(paths).slice(startLogOffset);
      if (recent.some((line) => /Recording blocked|License required|License check failed|Cloud transcription key missing/u.test(line))) {
        fail("Recording was blocked by the isolated profile's license or engine readiness. Check trial connectivity/status and selected model; see the isolated app log.");
      }
      return recent.some((line) => line.includes("Toggle: Recording started successfully"));
    }, 10000, "recording start");
    if (kind !== "instant-speech") await started();
    if (kind === "no-speech") {
      await delay(400);
      screenshot = join(out, `${selection.engine}-${selection.model}-${language}-${livePreview ? "preview" : "regular"}-${kind}-${ordinal}-${index}.png`);
      pillScreenshot(windowFinder, pid, screenshot);
    } else {
      screenshot = join(out, `${selection.engine}-${selection.model}-${language}-${livePreview ? "preview" : "regular"}-${kind}-${ordinal}-${index}.png`);
      await playClip(rows[index].file, outputIndex, pid, screenshot, windowFinder, kind === "instant-speech" ? hotkey : undefined);
    }
    if (kind === "instant-speech") await started();
    if (kind === "cancel") escape();
    else {
      if (kind !== "last-word" && kind !== "first-word" && kind !== "instant-speech") await delay(tailMs);
      hotkey();
      stoppedAt = Date.now();
    }
    result = await stableText(timeoutMs, beforePass, !new Set(["cancel", "no-speech"]).has(kind));
    if (index < rows.length - 1) await delay(500);
  }
  const actual = result.text.slice(textBefore.length);
  if ((kind === "cancel" || kind === "no-speech") && actual.length) fail(`${kind} unexpectedly inserted text in TextEdit.`);
  const reference = rows.map((row) => row.ref).join(" ");
  const lines = logLines(paths).slice(logBefore);
  return {
    id: `${selection.engine}:${selection.model}:${language}:${livePreview ? "live_preview" : "regular"}:${kind}:${ordinal}:${rows.map((row) => basename(row.file)).join("+")}`,
    engine: selection.engine, model: selection.model, language, mode: livePreview ? "live_preview" : "regular", case: kind,
    clips: rows.map((row) => basename(row.file)), wer: kind === "cancel" || kind === "no-speech" ? null : wer(reference, actual),
    first_word_present: kind === "first-word" || kind === "instant-speech" ? words(actual)[0] === words(reference)[0] : null,
    last_word_present: kind === "last-word" ? words(actual).at(-1) === words(reference).at(-1) : null,
    stop_to_text_ms: stoppedAt && actual.length ? result.at - stoppedAt : null,
    ...timing(lines), screenshot: basename(screenshot),
    preview_events_seen: null,
  };
}
function summary(report, regressions) {
  const body = report.cases.map((item) => `| ${item.engine}/${item.model} | ${item.language} | ${item.mode} | ${item.case} | ${item.wer?.toFixed(3) ?? "—"} | ${item.stop_to_text_ms ?? "—"} | ${item.start_to_first_audio_ms ?? "—"} |`).join("\n");
  const baselineStatus = report.baseline ? (regressions.length ? `Regressions: ${regressions.join("; ")}` : "No baseline regressions.") : "No baseline supplied.";
  return `# Dictation E2E\n\n${report.cases.length} cases. ${baselineStatus}\n\n| Engine/model | Lang | Mode | Case | WER | Stop→text ms | First audio ms |\n|---|---|---|---|---:|---:|---:|\n${body}\n`;
}
async function stopApp(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill("SIGTERM");
  try { await until(() => child.exitCode !== null || child.signalCode !== null, 3000, "Voicetypr exit"); }
  catch {
    child.kill("SIGKILL");
    await until(() => child.exitCode !== null || child.signalCode !== null, 3000, "Voicetypr forced exit");
  }
}
async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) { console.log(HELP); return; }
  const outputIndex = checkSetup();
  if (args["check-setup"]) return;
  if (!REAL_HOME) fail("HOME is unset; cannot identify the installed model directory.");
  if (!args.clips) fail("--clips <dir> is required. Run --help for usage.");
  const clips = manifest(resolve(args.clips));
  const selections = args.engines ? csv(args.engines) : args.engine && args.model ? [`${args.engine}:${args.model}`] : [];
  if (!selections.length) fail("Specify --engines engine:model,... or --engine and --model.");
  const engines = selections.map((value) => {
    const [engine, model, extra] = value.split(":");
    if (!engine || !model || extra || !/^[a-z0-9_-]+$/u.test(engine) || !/^[a-zA-Z0-9._-]+$/u.test(model)) fail(`Invalid engine:model selection: ${value}`);
    return { engine, model };
  });
  const selected = args.cases ? csv(args.cases) : CASES;
  if (!selected.length) fail("--cases must name at least one case.");
  for (const item of selected) if (!CASES.includes(item)) fail(`Unknown case ${item}. Choose from ${CASES.join(", ")}.`);
  const tailMs = positiveNumber(args["tail-ms"] ?? 250, "--tail-ms", true);
  const timeoutMs = positiveNumber(args["timeout-ms"] ?? 30000, "--timeout-ms");
  const werThreshold = positiveNumber(args["wer-threshold"] ?? 0.05, "--wer-threshold", true);
  const latencyThresholdMs = positiveNumber(args["latency-threshold-ms"] ?? 250, "--latency-threshold-ms", true);
  const bin = resolve(args.bin || join(ROOT, "src-tauri", "target", "debug", "voicetypr"));
  if (!existsSync(bin)) fail(`Debug app binary is missing: ${bin}. Build it with pnpm tauri:dev first.`);
  const running = command("/usr/bin/pgrep", ["-x", "voicetypr"], { allowFailure: true }).trim();
  if (/\d/u.test(running)) fail("Voicetypr is already running. Quit it before the E2E run so the single-instance plugin cannot redirect this launch into the real profile.");
  const out = join(ROOT, ".tmp", "e2e", new Date().toISOString().replaceAll(":", "-"));
  mkdirSync(out, { recursive: true });
  const report = { generated_at: new Date().toISOString(), bin: basename(bin), baseline: Boolean(args.baseline), cases: [] };
  const languageGroups = new Map();
  for (const clip of clips) languageGroups.set(clip.lang, [...(languageGroups.get(clip.lang) ?? []), clip]);
  try {
    const windowFinder = buildWindowFinder(out);
    for (const selection of engines) {
      for (const [language, languageClips] of languageGroups) {
        for (const livePreview of [false, true]) {
          const wanted = selected.filter((kind) => (kind === "live-preview") === livePreview);
          if (!wanted.length) continue;
          const home = join(out, `profile-${selection.engine}-${selection.model}-${language}-${livePreview ? "preview" : "regular"}`);
          const paths = prepareProfile(home, selection, livePreview, language, args["secure-store"]);
          const child = await launch(bin, home, paths);
          try {
            for (const kind of wanted) {
              const clipSets = kind === "regular" || kind === "live-preview" ? languageClips.map((clip) => [clip]) : [languageClips];
              for (const [ordinal, clipSet] of clipSets.entries()) {
                console.log(`Running ${selection.engine}/${selection.model} ${language} ${kind} clip ${ordinal + 1}...`);
                const item = await runCase({ kind, clips: clipSet, outputIndex, pid: child.pid, paths, out, windowFinder, timeoutMs, tailMs, selection, livePreview, ordinal });
                report.cases.push(item);
              }
            }
          } finally {
            await stopApp(child);
          }
        }
      }
    }
    const baseline = args.baseline ? JSON.parse(readFileSync(resolve(args.baseline), "utf8")) : null;
    const regressions = baseline ? compareReports(report, baseline, { werThreshold, latencyThresholdMs }) : [];
    writeFileSync(join(out, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
    writeFileSync(join(out, "summary.md"), summary(report, regressions));
    console.log(`Wrote ${join(out, "report.json")} and summary.md`);
    if (regressions.length) fail(`Baseline regressions: ${regressions.join("; ")}`);
  } catch (error) {
    writeFileSync(join(out, "report.json"), `${JSON.stringify({ ...report, error: error.message }, null, 2)}\n`);
    fail(`${error.message} Partial report: ${join(out, "report.json")}`);
  }
}

main().catch((error) => { console.error(`E2E error: ${error.message}`); process.exitCode = 1; });
