#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const args = parseArgs(process.argv.slice(2));
const outDir = resolve(args.out || join("perf-corpus", "synthetic"));
mkdirSync(outDir, { recursive: true });

const voices = parseVoices();
const manifest = [];
const skipped = [];

const corpus = {
  en: {
    required: true,
    localePrefixes: ["en_"],
    texts: {
      "2s": "Today we measure the final word clearly.",
      "5s": "The voice transcription harness records latency and accuracy for every model before a risky change ships.",
      "15s":
        "A careful benchmark needs short medium and long samples because performance can change when the audio window grows and the last phrase must still appear in the transcript.",
    },
  },
  fr: {
    required: true,
    localePrefixes: ["fr_"],
    texts: {
      "2s": "Bonjour nous mesurons le dernier mot clairement.",
      "5s": "Le harnais de transcription compare la latence et la precision pour chaque modele local.",
      "15s":
        "Un banc d essai fiable contient des phrases courtes moyennes et longues afin de verifier que la fin du message reste presente dans la transcription.",
    },
  },
  de: {
    required: true,
    localePrefixes: ["de_"],
    texts: {
      "2s": "Heute messen wir das letzte Wort deutlich.",
      "5s": "Der Transkriptions Test vergleicht Latenz und Genauigkeit fuer jedes lokale Modell.",
      "15s":
        "Ein verlaesslicher Messlauf braucht kurze mittlere und lange Aufnahmen damit die Leistung vergleichbar bleibt und das letzte Wort erhalten wird.",
    },
  },
  es: {
    required: true,
    localePrefixes: ["es_"],
    texts: {
      "2s": "Hoy medimos claramente la ultima palabra.",
      "5s": "El arnes de transcripcion compara latencia y precision para cada modelo local.",
      "15s":
        "Una prueba confiable necesita muestras cortas medianas y largas para confirmar que el rendimiento cambia sin perder la frase final.",
    },
  },
  it: {
    required: true,
    localePrefixes: ["it_"],
    texts: {
      "2s": "Oggi misuriamo chiaramente l ultima parola.",
      "5s": "Il sistema di trascrizione confronta latenza e accuratezza per ogni modello locale.",
      "15s":
        "Un controllo affidabile usa campioni brevi medi e lunghi per verificare le prestazioni e assicurare che la frase finale rimanga nel testo.",
    },
  },
  pt: {
    required: true,
    localePrefixes: ["pt_"],
    texts: {
      "2s": "Hoje medimos claramente a ultima palavra.",
      "5s": "O teste de transcricao compara latencia e precisao para cada modelo local.",
      "15s":
        "Uma avaliacao confiavel usa amostras curtas medias e longas para medir desempenho e confirmar que a ultima frase aparece no resultado.",
    },
  },
  nl: {
    required: true,
    localePrefixes: ["nl_"],
    texts: {
      "2s": "Vandaag meten we duidelijk het laatste woord.",
      "5s": "De transcriptie test vergelijkt vertraging en nauwkeurigheid voor elk lokaal model.",
      "15s":
        "Een betrouwbare meting gebruikt korte middelgrote en lange opnames zodat prestaties vergelijkbaar blijven en de laatste zin zichtbaar is.",
    },
  },
  sv: {
    required: false,
    localePrefixes: ["sv_"],
    texts: {
      "2s": "I dag maeter vi det sista ordet tydligt.",
      "5s": "Transkriptions testet jaemfoer latens och noggrannhet foer varje lokal modell.",
      "15s":
        "En tillfoerlitlig maetning anvaender korta medellaanga och laanga exempel foer att kontrollera prestanda och sista frasen.",
    },
  },
  pl: {
    required: false,
    localePrefixes: ["pl_"],
    texts: {
      "2s": "Dzisiaj wyraznie mierzymy ostatnie slowo.",
      "5s": "Test transkrypcji porownuje opoznienie i dokladnosc dla kazdego modelu lokalnego.",
      "15s":
        "Wiarygodny pomiar uzywa krotkich srednich i dlugich nagran aby porownac wydajnosc oraz sprawdzic ostatnia fraze.",
    },
  },
};

for (const [lang, config] of Object.entries(corpus)) {
  const voice = findVoice(voices, config.localePrefixes);
  if (!voice) {
    skipped.push(`${lang}: no macOS say voice installed`);
    continue;
  }
  for (const [bucket, reference] of Object.entries(config.texts)) {
    const stem = `${lang}-${bucket}`;
    const aiffPath = join(outDir, `${stem}.aiff`);
    const wavPath = join(outDir, `${stem}.wav`);
    run("say", ["-v", voice.name, "-o", aiffPath, reference]);
    run("afconvert", ["-f", "WAVE", "-d", "LEI16@16000", aiffPath, wavPath]);
    if (existsSync(aiffPath)) rmSync(aiffPath);
    manifest.push({ file: `${stem}.wav`, lang, reference, bucket });
  }
}

manifest.sort((a, b) => `${a.lang}\0${a.bucket}`.localeCompare(`${b.lang}\0${b.bucket}`));
writeFileSync(join(outDir, "manifest.jsonl"), manifest.map((row) => JSON.stringify(row)).join("\n") + "\n");
writeFileSync(
  join(outDir, "corpus-meta.json"),
  `${JSON.stringify({ type: "synthetic", generator: "scripts/gen-corpus.mjs" }, null, 2)}\n`,
);

console.log(`Wrote ${manifest.length} samples to ${outDir}`);
for (const line of skipped) console.warn(`Skipped ${line}`);
console.log("Synthetic TTS is valid for RELATIVE regression deltas, not absolute WER claims.");

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--") continue;
    if (!arg.startsWith("--")) fail(`Unexpected argument: ${arg}`);
    const key = arg.slice(2);
    const value = argv[i + 1];
    if (!value || value.startsWith("--")) parsed[key] = "true";
    else {
      parsed[key] = value;
      i += 1;
    }
  }
  return parsed;
}

function parseVoices() {
  const result = spawnSync("say", ["-v", "?"], { encoding: "utf8" });
  if (result.status !== 0) fail(`say -v ? failed: ${result.stderr || result.stdout}`);
  return result.stdout
    .split(/\r?\n/)
    .map((line) => {
      const match = line.match(/^(\S+)\s+([a-z]{2}_[A-Z]{2})\s+#/);
      return match ? { name: match[1], locale: match[2] } : null;
    })
    .filter(Boolean);
}

function findVoice(voices, prefixes) {
  return voices.find((voice) => prefixes.some((prefix) => voice.locale.startsWith(prefix))) || null;
}

function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, { encoding: "utf8" });
  if (result.status !== 0) fail(`${command} failed: ${result.stderr || result.stdout}`);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
