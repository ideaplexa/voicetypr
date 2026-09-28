#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, isAbsolute, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";

const args = parseArgs(process.argv.slice(2));
if (!args.bin || !args.clips || !args.models) {
  fail("Usage: node scripts/model-bench.mjs --bin <path> --clips <dir> --models <name=path,...> [--reps 2] [--retries 0] [--out <dir>]");
}
const bin = resolve(args.bin);
const clipsDir = resolve(args.clips);
const outDir = resolve(args.out || process.cwd());
const reps = Number(args.reps || "2");
if (!Number.isSafeInteger(reps) || reps < 1) fail("--reps must be a positive integer");
const retries = Number(args.retries || "0");
if (!Number.isSafeInteger(retries) || retries < 0) fail("--retries must be a nonnegative integer");
if (!existsSync(bin)) fail("Missing CLI binary");
const models = parseModels(args.models);
const clips = readClips(join(clipsDir, "manifest.json"), clipsDir);
mkdirSync(outDir, { recursive: true });
const canMeasurePeakRss = process.platform === "darwin" && existsSync("/usr/bin/time") &&
  (() => {
    const probe = spawnSync("/usr/bin/time", ["-l", "/usr/bin/true"], { encoding: "utf8" });
    return probe.status === 0 && /\d+\s+maximum resident set size/.test(probe.stderr || "");
  })();

const runs = [];
for (const model of models) {
  for (const clip of clips) {
    if (model.english_only && clip.lang !== "en") {
      runs.push({ model: model.name, file: clip.file, lang: clip.lang, status: "SKIPPED", reason: "English-only model" });
      continue;
    }
    for (let rep = 1; rep <= reps; rep += 1) {
      const attempt_errors = [];
      for (let attempt = 1; attempt <= retries + 1; attempt += 1) {
        const start = performance.now();
        const command = ["transcribe", "--file", clip.path, "--engine", "whisper",
          "--model-file", model.path, "--language", clip.lang, "--json"];
        // macOS time -l reports child peak RSS; spawnSync has no child resource usage API.
        const measureMemory = canMeasurePeakRss;
        const run = spawnSync(measureMemory ? "/usr/bin/time" : bin,
          measureMemory ? ["-l", bin, ...command] : command,
          { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
        const wall_ms = performance.now() - start;
        const stderr = redact(lastLines(run.stderr, 40), model, clip);
        const peak_rss_bytes = measureMemory
          ? finiteNumber(Number(run.stderr?.match(/(\d+)\s+maximum resident set size/)?.[1])) : null;
        let record;
        if (run.error || run.status !== 0) {
          const cliError = run.stderr?.split(/\r?\n/).map((line) => line.trim())
            .filter((line) => line.startsWith('{"error":') || line.startsWith("Error:")).at(-1);
          record = { status: "ERROR", error: redact(run.error?.message || cliError ||
            `CLI exited ${run.status}`, model, clip) };
        } else {
          try {
            const payload = JSON.parse(run.stdout);
            if (typeof payload.text !== "string") throw new Error("CLI JSON has no text field");
            if (!payload.text.trim()) throw new Error("CLI returned an empty transcript");
            const timings_ms = payload.metadata?.timings_ms || {};
            const transcribe_ms = finiteNumber(timings_ms.total) ?? finiteNumber(payload.metadata?.processing_duration_ms);
            record = { status: "OK", reference: clip.ref, transcript: payload.text,
              wer: wer(clip.ref, payload.text), transcribe_ms, timings_ms };
          } catch (error) {
            record = { status: "ERROR", error: redact(error.message, model, clip) };
          }
        }
        if (record.status === "ERROR") {
          attempt_errors.push({ attempt, error: record.error, stderr });
          console.error(`${model.name} ${clip.file} rep ${rep} attempt ${attempt}: ${record.error}`);
          if (stderr) console.error(stderr);
        }
        if (record.status === "OK" || attempt > retries) {
          runs.push({ model: model.name, file: clip.file, lang: clip.lang, rep, attempts: attempt,
            ...record, wall_ms, peak_rss_bytes, ...(record.status === "ERROR" ? { stderr } : {}),
            ...(attempt_errors.length ? { attempt_errors } : {}) });
          break;
        }
      }
    }
  }
}

const report = {
  platform: `${process.platform}-${process.arch}`,
  reps, retries,
  clips: clips.map(({ file, lang, ref }) => ({ file, lang, ref })),
  models: models.map((model) => summarize(model, runs)),
  runs,
};
writeFileSync(join(outDir, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
writeFileSync(join(outDir, "report.md"), renderMarkdown(report));
console.log("Wrote report.json and report.md");
if (runs.some((run) => run.status === "ERROR")) process.exitCode = 1;

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    if (!["--bin", "--clips", "--models", "--reps", "--retries", "--out"].includes(key) || !argv[i + 1]) {
      fail(`Invalid argument: ${key}`);
    }
    parsed[key.slice(2)] = argv[i + 1];
  }
  return parsed;
}

function parseModels(csv) {
  const names = new Set();
  return csv.split(",").map((entry) => {
    const equals = entry.indexOf("=");
    if (equals < 1 || equals === entry.length - 1) fail(`Invalid model entry: ${entry}`);
    const name = entry.slice(0, equals).trim();
    const path = resolve(entry.slice(equals + 1).trim());
    if (!name || names.has(name)) fail(`Missing or duplicate model name: ${name}`);
    if (!existsSync(path) || !statSync(path).isFile()) fail(`Missing model file for ${name}`);
    names.add(name);
    return { name, path, file_size_bytes: statSync(path).size,
      english_only: name.includes(".en") || basename(path).includes(".en") || name.toLowerCase().includes("distil") };
  });
}

function readClips(manifestPath, clipsDir) {
  if (!existsSync(manifestPath)) fail("Missing clip manifest.json");
  const rows = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (!Array.isArray(rows) || rows.length === 0) fail("Clip manifest must be a nonempty array");
  return rows.map((row) => {
    for (const key of ["file", "lang", "ref"]) {
      if (typeof row[key] !== "string" || !row[key].trim()) fail(`Clip missing ${key}`);
    }
    if (isAbsolute(row.file) || basename(row.file) !== row.file) fail(`Invalid clip filename: ${row.file}`);
    const path = join(clipsDir, row.file);
    if (!existsSync(path)) fail(`Missing clip: ${row.file}`);
    return { ...row, path };
  });
}

function finiteNumber(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function words(value) {
  return value.toLowerCase().normalize("NFKD").replace(/[\p{P}\p{S}]+/gu, " ").split(/\s+/).filter(Boolean);
}

function wer(reference, transcript) {
  const a = words(reference);
  const b = words(transcript);
  if (!a.length) return b.length ? 1 : 0;
  let previous = Array.from({ length: b.length + 1 }, (_, index) => index);
  for (let i = 1; i <= a.length; i += 1) {
    const current = [i];
    for (let j = 1; j <= b.length; j += 1) {
      current[j] = Math.min(previous[j] + 1, current[j - 1] + 1,
        previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    }
    previous = current;
  }
  return previous[b.length] / a.length;
}

function mean(values) {
  return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null;
}

function p95(values) {
  if (!values.length) return null;
  const sorted = values.toSorted((a, b) => a - b);
  return sorted[Math.ceil(sorted.length * 0.95) - 1];
}

function summarize(model, runs) {
  const matching = runs.filter((run) => run.model === model.name);
  const ok = matching.filter((run) => run.status === "OK");
  const by_language = Object.fromEntries([...new Set(ok.map((run) => run.lang))].sort().map((lang) => {
    const values = ok.filter((run) => run.lang === lang).map((run) => run.wer);
    return [lang, { n: values.length, mean_wer: mean(values) }];
  }));
  const transcribe = ok.map((run) => run.transcribe_ms).filter((value) => value !== null);
  const peakRss = matching.map((run) => run.peak_rss_bytes).filter((value) => value !== null);
  return { name: model.name, file_size_bytes: model.file_size_bytes, english_only: model.english_only,
    completed: ok.length, errors: matching.filter((run) => run.status === "ERROR").length,
    skipped: matching.filter((run) => run.status === "SKIPPED").length,
    retried: matching.filter((run) => run.attempts > 1).length,
    max_peak_rss_bytes: peakRss.length ? Math.max(...peakRss) : null,
    mean_wer: mean(ok.map((run) => run.wer)), by_language,
    mean_transcribe_ms: mean(transcribe), p95_transcribe_ms: p95(transcribe),
    mean_wall_ms: mean(ok.map((run) => run.wall_ms)), p95_wall_ms: p95(ok.map((run) => run.wall_ms)) };
}

function renderMarkdown(report) {
  const lines = ["# Whisper model bench", "", `Platform: ${report.platform}  `, `Repetitions: ${report.reps}  `, `Retries: ${report.retries}  `,
    "WER is a fraction (0.05 = 5%). Transcribe time uses CLI `timings_ms.total`, then `processing_duration_ms` if available. Wall time covers the entire CLI invocation.", "",
    "Peak RSS is available when macOS `/usr/bin/time -l` is permitted by the runner.", "",
    "| Model | Size MiB | Runs | Mean WER | WER by language | Mean transcribe ms | P95 transcribe ms | Mean wall ms | P95 wall ms | Peak RSS MiB | Retried | Errors |",
    "|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|"];
  for (const model of report.models) {
    const languages = Object.entries(model.by_language).map(([lang, value]) => `${lang}: ${fmt(value.mean_wer, 3)}`).join(", ");
    lines.push(`| ${model.name} | ${fmt(model.file_size_bytes / 1048576, 1)} | ${model.completed} | ${fmt(model.mean_wer, 3)} | ${languages} | ${fmt(model.mean_transcribe_ms, 0)} | ${fmt(model.p95_transcribe_ms, 0)} | ${fmt(model.mean_wall_ms, 0)} | ${fmt(model.p95_wall_ms, 0)} | ${fmt(model.max_peak_rss_bytes === null ? null : model.max_peak_rss_bytes / 1048576, 0)} | ${model.retried} | ${model.errors} |`);
  }
  const diagnosed = report.runs.filter((run) => run.attempt_errors?.length);
  if (diagnosed.length) {
    lines.push("", "## Failed attempt diagnostics", "");
    for (const run of diagnosed) {
      for (const failure of run.attempt_errors) {
        lines.push(`### ${run.model} · ${run.file} · rep ${run.rep} · attempt ${failure.attempt}`,
          "", failure.error, "", "```text", failure.stderr || "(no stderr)", "```", "");
      }
    }
  }
  return `${lines.join("\n")}\n`;
}

function fmt(value, digits) {
  return value === null ? "—" : value.toFixed(digits);
}

function lastLines(value, count) {
  return (value || "").split(/\r?\n/).filter((line) => line.trim()).slice(-count).join("\n");
}

function redact(value, model, clip) {
  return value.replaceAll(model.path, "[model file]").replaceAll(clip.path, "[clip]")
    .replaceAll(bin, "[CLI]");
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
