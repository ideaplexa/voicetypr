#!/usr/bin/env python3
"""Fetch a multilingual real-speech eval set (MLS test, CC BY 4.0) as 16 kHz mono WAV.

Usage: python fetch_eval.py <out_dir>   (needs ffmpeg on PATH)
"""
import json, os, subprocess, sys, time, urllib.request

PLAN = [("german", "de", 15, 3394), ("spanish", "es", 15, 2385), ("french", "fr", 6, 2426)]


def row(config, offset):
    url = ("https://datasets-server.huggingface.co/rows?dataset=facebook/multilingual_librispeech"
           f"&config={config}&split=test&offset={offset}&length=1")
    for attempt in range(4):
        try:
            return json.load(urllib.request.urlopen(url, timeout=60))["rows"][0]["row"]
        except Exception:
            time.sleep(2 * (attempt + 1))
    return None


def main(out):
    os.makedirs(out, exist_ok=True)
    manifest = []
    for config, lang, count, total in PLAN:
        step = total // 10 if lang == "fr" else total // count
        for i in range(count):
            offset = i * step + 7
            r = row(config, offset)
            if not r:
                print(f"skip {config} {offset}", flush=True)
                continue
            name = f"{lang}-{config}-{offset}"
            raw = os.path.join(out, name + ".ogg")
            urllib.request.urlretrieve(r["audio"][0]["src"], raw)
            subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", raw, "-ar", "16000", "-ac", "1",
                            os.path.join(out, name + ".wav")], check=True)
            os.remove(raw)
            manifest.append({"file": name + ".wav", "lang": lang, "ref": r["transcript"]})
    json.dump(manifest, open(os.path.join(out, "manifest.json"), "w"), indent=1, ensure_ascii=False)
    print(f"{len(manifest)} clips")


if __name__ == "__main__":
    main(sys.argv[1])
