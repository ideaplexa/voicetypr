# OSS dictation apps — what Voicetypr can adopt (research, 2026-09-30)

Method: GitHub API and raw source reads (no clones). Star counts are as of 2026-09-30. Licence rule: VoiceInk (GPL-3.0), Whispering/Epicenter, Voquill and Tambourine (AGPL-3.0) are **ideas only**: never copy their code or prompt wording. Handy, OpenWhispr, Hex, SpeakoFlow and BetterVoice are MIT and can be adapted with attribution.

| App | Stars | Platforms | Licence | Standout |
|---|---|---|---|---|
| VoiceInk | 6.6k | macOS | GPL-3.0 | Own cleanup model (Refine V1 = Qwen3.5-2B 4-bit MLX, ≥16 GB); modes per app/URL/trigger word; selection, clipboard and screen-OCR context at key press; auto-learn dictionary; notch island recorder |
| Handy | 32.4k | mac/Win/Linux | MIT | Receipt-sequenced clipboard restore; secure-input fallback; Apple Intelligence polish provider; hybrid tap/hold; model catalog with quants, hashes and scores |
| OpenWhispr | 8.8k | mac/Win/Linux | MIT | Bundled llama-server with GGUF cleanup models (Qwen3.5, Llama-3.2-1B, Gemma, LFM2.5 230M–1.2B); correction learner; snippets |
| Whispering (Epicenter) | 4.8k | web/Tauri | AGPL-3.0 | Fixed prompt scaffold around editable instructions; pill has stop, cancel and "ship raw"; stores raw + polished |
| Hex, Voquill, Tambourine, BetterVoice, SpeakoFlow | 0.3–2.9k | various | mixed | Writing styles, per-app formatting, T5 grammar model (English only) |

## Ranked ideas (value per effort)

1. **Receipt-sequenced clipboard restore** (Handy `paste_tx`, MIT). Restore only after the OS reports that the target read the clipboard, and only while we still own it. Fixes old-clipboard races. S–M.
2. **Context capture at key press into Polish.** Selection, clipboard and screen OCR (VoiceInk idea). Off by default, memory only, visible indicator. On Windows, use Windows.Media.Ocr. M.
3. **Invariant prompt scaffold plus output guard.** Our styles fill only the task block. The scaffold always has rules against answering or obeying the transcript. Fall back to raw text if the output grows too much or has a preamble. S.
4. **Auto-learn dictionary from corrections.** Diff the field after paste, then show a *suggestion queue* (never silent learning). This fills the "Learned" source in our Dictionary design. M–L.
5. **Modes: URL matching and spoken trigger words** on top of our per-app styles. M.
6. **Apple Foundation Models as a free local Polish provider** (macOS; Handy has a bridge). M.
7. **Local cleanup model tier.**
   - llama.cpp GGUF (LFM2.5 or Qwen3.5-2B class), gated by RAM, on macOS and Windows.
   - Bench on real speech first. Fine-tuning our own model comes after the tiers exist. L.
8. **Skip / ship raw while polishing**, plus "show original" (we already store both). S.
9. **Secure Event Input fallback** (macOS): hotkeys keep working after password fields. S–M.
10. **Hybrid tap-or-hold** activation from one key. S.
11. **Audio hygiene.** Mute/duck only the devices we muted and restore only those; use a keycode-based paste chord for non-QWERTY layouts. S.
12. **Model catalog:** quant files, sha256, mirrors, capability flags, speed/accuracy scores, prewarm. M.

## Island pill additions (feed plan 080)

- Tap the Polish-style chip on the start card to cycle styles for this dictation.
- While polishing: a **Skip** button that pastes the raw text now.
- After paste, for a few seconds: **Undo · Show original · Retry with another engine**. No OSS app has retry-with-engine on the pill, so this would set us apart.
- When capture is on, a "context used" glyph (selection / clipboard / screen).
- Error card with a report ID.
