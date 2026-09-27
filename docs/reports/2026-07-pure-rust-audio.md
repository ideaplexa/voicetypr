# VoiceTypr got ~60% smaller — by deleting ffmpeg

**TL;DR:** We replaced the bundled ffmpeg + ffprobe binaries (~98 MB per platform) with
a pure-Rust audio pipeline. The macOS app bundle dropped **60%** (162 MB → 64 MB) and the
compressed installer dropped **64%** (70 MB → 24 MB), with **no loss of format support** —
still decoding mp3, m4a, mp4, webm, mkv, ogg, wav, flac, and opus. Same formats, a third
of the size, one fewer moving part, and it can no longer crash on a malformed file.

---

## The numbers (macOS, Apple Silicon)

All measured on the same build, holding the app binary constant so the delta is *exactly*
the ffmpeg removal — not build noise.

| | Before (with ffmpeg) | After (pure Rust) | Δ |
|---|---:|---:|---:|
| App bundle (`.app`, uncompressed) | 162 MB | **64 MB** | **−60%** |
| Installer (`.dmg`, compressed) | 70 MB | **24 MB** | **−64%** |
| Bundled 3rd-party binaries | 98 MB (ffmpeg 49 + ffprobe 49) | **0 MB** | **−98 MB** |

Cross-check against what we actually shipped: VoiceTypr **v2.0.4**'s Apple-Silicon `.dmg`
was **63 MB**. The new installer is **24 MB**. That is the number a user downloads.

**Windows:** the same ~98 MB of `ffmpeg.exe` + `ffprobe.exe` comes out of every Windows
package. CI-proven: the full **Windows Store MSIX** now builds and packs cleanly with **zero
ffmpeg** (`Voicetypr_2.0.4.0_x64.msix`, ~46 MB package), with the pure-Rust decoder compiled
in (symphonia + rubato + audiopus). The 308 MB figure is the NSIS `setup.exe`, which is a
different package dominated by the bundled GPU/Vulkan + VC runtime installers — its exact
delta lands with the next tagged release build; the ffmpeg removal takes the same ~98 MB out
of it regardless.

---

## What actually changed

**Before:** every time you transcribed anything that wasn't already a 16 kHz mono WAV —
an imported mp3, an m4a voice memo, a video's audio track — VoiceTypr shelled out to a
**bundled ffmpeg process**, wrote a temp WAV to disk, read it back, and cleaned up. Two
50 MB binaries rode along in every download to do it, and CI pulled them from a third-party
mirror (gyan.dev) on every build.

**After:** a single in-process Rust function, `audio::decode::normalize_to_wav`, does the
whole thing — demux, decode, downmix to mono, resample to 16 kHz, write s16 WAV — using
well-established Rust audio crates:

- **[symphonia](https://github.com/pdeljanov/Symphonia)** — pure-Rust demux + decode for
  aac / mp3 / flac / vorbis / pcm / alac inside mp4 / mkv / webm / ogg / wav
- **[rubato](https://github.com/HEnquist/rubato)** — high-quality sample-rate conversion
- **[audiopus](https://github.com/lakelezz/audiopus)** (libopus) — Opus, via a side-path
  (Opus decoders can't live in symphonia's static registry, so we decode those packets directly)

No format was dropped. webm and mkv — which we explicitly wanted to keep — still work.

---

## Why it's not just smaller

**Faster on the transcription path.** Decoding an imported/non-WAV file no longer spawns a
subprocess or round-trips a temp file through disk on the critical path — it's a direct
in-process call. One process spawn + one disk write + one disk read + one file delete,
removed from every non-WAV transcription.

**It can't crash on bad input.** The whole decode is wrapped in `catch_unwind`, so a
malformed or truncated file returns a clean error instead of taking the app down. That's
covered by tests that feed it a text file and a truncated WAV and assert "error, not panic."

**One fewer moving part.** No external binary to code-sign, notarize, keep patched against
ffmpeg CVEs, or re-download from a flaky mirror in CI. The audio path is now the same Rust
toolchain as the rest of the app, gated by the same `clippy -D warnings` and unit tests.

**Streaming / bounded memory.** Long files decode in a streaming loop (packet → downmix →
resample → write) rather than materializing the whole thing in RAM — proven by a 20-minute
bounded-memory test.

---

## How we know it's safe

- `cargo build` ✅ · `clippy --workspace --all-targets -D warnings` ✅ · decode unit tests ✅
- Real-file tests decode actual mp3 / mp4 / mkv / webm / opus fixtures (incl. real
  TikTok / pitch / WhatsApp exports) to 16 kHz mono s16 with **zero crashes**
- The macOS `.app` bundles cleanly with **no ffmpeg present** (verified: only the app binary
  + the Parakeet sidecar ship in `Contents/MacOS/`)
- Full cross-platform CI is **green** on the real bundles without ffmpeg: `build-macos`
  (aarch64 + Intel x86_64), `build-windows` (+ Vulkan sidecar), and the **Windows Store MSIX**
  packaging job all pass

---

## Draft launch copy — for review, not final

> _Marketing note: keep the "local / offline AI" framing; do not name the underlying model
> engine in public copy. These are drafts for founder sign-off._

**Short:**
> VoiceTypr just got ~60% smaller. Same offline transcription, same formats (mp3, m4a, mp4,
> webm, mkv…), a third of the download. We deleted 98 MB of bundled media binaries and rebuilt
> the audio pipeline in pure Rust — smaller, faster, and it can't crash on a weird file anymore.

**Thread beat list:**
1. The install got 60% smaller. Here's the before/after and how. 🧵
2. Old way: we shipped a 98 MB media toolchain just to convert audio before transcribing.
3. New way: one Rust function. Same formats in, 16 kHz mono out, in-process — no subprocess,
   no temp files, no 98 MB.
4. Bonus: malformed file? Clean error, not a crash. It's tested.
5. Result: 24 MB installer (was 63), decode on the hot path, one fewer thing to break.
