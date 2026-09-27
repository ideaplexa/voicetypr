#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";

const args = parseArgs(process.argv.slice(2));
const corpusDir = args.corpus ? resolve(args.corpus) : null;
if (!corpusDir) {
  fail("Usage: node scripts/perf-harness.mjs --corpus <dir> --engines whisper --models <csv> [--bin <path>] [--reps 5]");
}

const manifestPath = join(corpusDir, "manifest.jsonl");
if (!existsSync(manifestPath)) fail(`Missing manifest: ${manifestPath}`);

const bin = resolve(args.bin || join("src-tauri", "target", "debug", "voicetypr"));
const reps = Number.parseInt(args.reps || "5", 10);
if (!Number.isInteger(reps) || reps < 1) fail("--reps must be a positive integer");

const engines = csv(args.engines || "whisper");
const models = csv(args.models || "");
if (engines.length === 0) fail("--engines must contain at least one engine");
if (models.length === 0) fail("--models must contain at least one model");

const outDir = resolve(args.out || process.cwd());
mkdirSync(outDir, { recursive: true });

const manifest = readManifest(manifestPath, corpusDir);
const corpusType = readCorpusType(corpusDir);
const results = [];
const skipped = [];

for (const engine of engines.toSorted()) {
  for (const model of models.toSorted()) {
    let comboSkipped = null;
    for (const item of manifest) {
      for (let rep = 1; rep <= reps; rep += 1) {
        if (comboSkipped) break;
        const run = runCli({ bin, engine, model, item });
        if (run.skipped) {
          comboSkipped = run.reason;
          skipped.push({
            engine,
            model,
            bucket: "ALL",
            lang: "ALL",
            status: "SKIPPED",
            reason: run.reason,
          });
          break;
        }
        if (run.error) {
          results.push({
            engine,
            model,
            bucket: item.bucket,
            lang: item.lang,
            file: item.file,
            rep,
            status: "ERROR",
            error: run.error,
          });
          continue;
        }
        const hypothesis = run.payload.text || "";
        const werValue = wer(item.reference, hypothesis);
        results.push({
          engine,
          model,
          bucket: item.bucket,
          lang: item.lang,
          file: item.file,
          rep,
          status: "OK",
          text: hypothesis,
          reference: item.reference,
          wer: werValue,
          last_word_present: lastWordPresent(item.reference, hypothesis),
          timings_ms: extractTimings(run.payload),
        });
      }
      if (comboSkipped) break;
    }
  }
}

const report = buildReport({ corpusType, corpusDir, bin, reps, results, skipped });
writeFileSync(join(outDir, "perf-report.json"), `${JSON.stringify(report, null, 2)}\n`);
writeFileSync(join(outDir, "perf-report.md"), renderMarkdown(report));

console.log(`Wrote ${join(outDir, "perf-report.json")}`);
console.log(`Wrote ${join(outDir, "perf-report.md")}`);

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--") continue;
    if (!arg.startsWith("--")) fail(`Unexpected argument: ${arg}`);
    const key = arg.slice(2);
    const next = argv[i + 1];
    if (!next || next.startsWith("--")) parsed[key] = "true";
    else {
      parsed[key] = next;
      i += 1;
    }
  }
  return parsed;
}

function csv(value) {
  return value
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

function readManifest(path, baseDir) {
  return readFileSync(path, "utf8")
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line, index) => {
      const row = JSON.parse(line);
      for (const key of ["file", "lang", "reference", "bucket"]) {
        if (typeof row[key] !== "string" || row[key].length === 0) {
          fail(`manifest line ${index + 1} missing string ${key}`);
        }
      }
      const audioPath = isAbsolute(row.file) ? row.file : join(baseDir, row.file);
      return { ...row, audioPath };
    })
    .toSorted((a, b) =>
      `${a.lang}\0${a.bucket}\0${a.file}`.localeCompare(`${b.lang}\0${b.bucket}\0${b.file}`),
    );
}

function readCorpusType(baseDir) {
  const metaPath = join(baseDir, "corpus-meta.json");
  if (!existsSync(metaPath)) return "real";
  try {
    const meta = JSON.parse(readFileSync(metaPath, "utf8"));
    return meta.type === "synthetic" ? "synthetic" : "real";
  } catch {
    return "real";
  }
}

function runCli({ bin, engine, model, item }) {
  const run = spawnSync(
    bin,
    [
      "transcribe",
      "--file",
      item.audioPath,
      "--engine",
      engine,
      "--model",
      model,
      "--language",
      item.lang,
      "--json",
    ],
    { encoding: "utf8", maxBuffer: 1024 * 1024 * 64 },
  );
  const combined = `${run.stdout || ""}\n${run.stderr || ""}`;
  if (run.error) return { error: run.error.message };
  if (run.status !== 0) {
    if (isMissingModel(combined)) return { skipped: true, reason: firstLine(combined) };
    return { error: firstLine(combined) || `CLI exited ${run.status}` };
  }
  try {
    return { payload: JSON.parse(run.stdout) };
  } catch (error) {
    return { error: `Failed to parse CLI JSON: ${error.message}` };
  }
}

function isMissingModel(text) {
  return /not downloaded|not installed|model .*not found|failed to load model|no such file|unknown .*model/i.test(text);
}

function firstLine(text) {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) || "";
}

function extractTimings(payload) {
  const timings = payload?.metadata?.timings_ms;
  return timings && typeof timings === "object" && !Array.isArray(timings) ? timings : {};
}

function normalizeWords(text) {
  return text
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\p{P}\p{S}]+/gu, " ")
    .split(/\s+/)
    .filter(Boolean);
}

function wer(reference, hypothesis) {
  const ref = normalizeWords(reference);
  const hyp = normalizeWords(hypothesis);
  if (ref.length === 0) return hyp.length === 0 ? 0 : 1;
  const distance = levenshtein(ref, hyp);
  return distance / ref.length;
}

function levenshtein(a, b) {
  let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i += 1) {
    const curr = [i];
    for (let j = 1; j <= b.length; j += 1) {
      curr[j] = Math.min(
        prev[j] + 1,
        curr[j - 1] + 1,
        prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
    }
    prev = curr;
  }
  return prev[b.length];
}

function lastWordPresent(reference, hypothesis) {
  const ref = normalizeWords(reference);
  if (ref.length === 0) return true;
  const last = ref.at(-1);
  return normalizeWords(hypothesis).slice(-10).includes(last);
}

function percentile(values, p) {
  if (values.length === 0) return null;
  const sorted = values.toSorted((a, b) => a - b);
  const index = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(sorted.length - 1, index))];
}

function avg(values) {
  if (values.length === 0) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function buildReport({ corpusType, corpusDir, bin, reps, results, skipped }) {
  const ok = results.filter((row) => row.status === "OK");
  return {
    corpus_type: corpusType,
    corpus_dir: corpusDir,
    bin,
    reps,
    skipped: skipped.toSorted(compareRows),
    timing_summary: timingSummary(ok),
    wer_by_language: werByLanguage(ok),
    runs: results.toSorted(compareRows),
  };
}

function timingSummary(rows) {
  const groups = new Map();
  for (const row of rows) {
    for (const [span, value] of Object.entries(row.timings_ms || {})) {
      if (typeof value !== "number") continue;
      const key = `${row.engine}\0${row.model}\0${row.bucket}\0${span}`;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(value);
    }
  }
  return [...groups.entries()]
    .map(([key, values]) => {
      const [engine, model, bucket, span] = key.split("\0");
      return {
        engine,
        model,
        bucket,
        span,
        n: values.length,
        p50: percentile(values, 50),
        p95: percentile(values, 95),
      };
    })
    .toSorted(compareRows);
}

function werByLanguage(rows) {
  const groups = new Map();
  for (const row of rows) {
    const key = `${row.engine}\0${row.model}\0${row.lang}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(row);
  }
  return [...groups.entries()]
    .map(([key, group]) => {
      const [engine, model, lang] = key.split("\0");
      return {
        engine,
        model,
        lang,
        n: group.length,
        wer: avg(group.map((row) => row.wer)),
        last_word_pass_rate: avg(group.map((row) => (row.last_word_present ? 1 : 0))),
      };
    })
    .toSorted(compareRows);
}

function compareRows(a, b) {
  return JSON.stringify(a).localeCompare(JSON.stringify(b));
}

function renderMarkdown(report) {
  const lines = [];
  lines.push("# Voicetypr Perf Report", "");
  lines.push(`Corpus type: ${report.corpus_type}`);
  lines.push(`Corpus dir: ${report.corpus_dir}`);
  lines.push(`Binary: ${report.bin}`);
  lines.push(`Reps: ${report.reps}`, "");

  lines.push("## Timing Summary", "");
  lines.push("| engine | model | bucket | span | n | p50 ms | p95 ms |");
  lines.push("|---|---|---|---|---:|---:|---:|");
  for (const row of report.timing_summary) {
    lines.push(`| ${row.engine} | ${row.model} | ${row.bucket} | ${row.span} | ${row.n} | ${fmt(row.p50)} | ${fmt(row.p95)} |`);
  }
  for (const row of report.skipped) {
    lines.push(`| ${row.engine} | ${row.model} | ${row.bucket} | SKIPPED | 0 |  | ${escapePipe(row.reason)} |`);
  }
  lines.push("");

  lines.push("## WER By Language", "");
  lines.push("| engine | model | lang | n | WER | last-word pass |");
  lines.push("|---|---|---|---:|---:|---:|");
  for (const row of report.wer_by_language) {
    lines.push(`| ${row.engine} | ${row.model} | ${row.lang} | ${row.n} | ${fmt(row.wer)} | ${fmt(row.last_word_pass_rate)} |`);
  }
  for (const row of report.skipped) {
    lines.push(`| ${row.engine} | ${row.model} | ${row.lang} | 0 | SKIPPED | ${escapePipe(row.reason)} |`);
  }
  lines.push("");

  const errors = report.runs.filter((row) => row.status === "ERROR");
  if (errors.length > 0) {
    lines.push("## Run Errors", "");
    lines.push("| engine | model | lang | bucket | file | rep | error |");
    lines.push("|---|---|---|---|---|---:|---|");
    for (const row of errors) {
      lines.push(
        `| ${row.engine} | ${row.model} | ${row.lang} | ${row.bucket} | ${escapePipe(row.file)} | ${row.rep} | ${escapePipe(row.error)} |`,
      );
    }
    lines.push("");
  }
  return `${lines.join("\n")}\n`;
}

function fmt(value) {
  return value === null || value === undefined ? "" : Number(value).toFixed(3);
}

function escapePipe(value) {
  return String(value || "").replaceAll("|", "\\|");
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
