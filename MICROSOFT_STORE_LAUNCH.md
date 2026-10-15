# Microsoft Store Launch Playbook

Use this for every Microsoft Store submission/update. Keep direct GitHub installer releases separate from Store MSIX releases.

## Recurring certification findings (check ALL every submission)

Voicetypr has been rejected for a different policy each round, so address all of these proactively:

- **11.16 Live Generative AI Content** — AI text-cleanup uses OpenAI/Anthropic/Gemini. Partner Center -> Properties -> Product declarations -> check "This product incorporates generative AI features...". No code.
- **10.2.4.1 Software Dependencies** — `vcomp140.dll` (VC++ OpenMP runtime, from whisper-rs `openmp`). Fixed by app-local bundling in `build-msix-store.ps1` (see "Bundled runtime" below). Also name it in the listing opening (see "Listing opening" below).
- **10.1.5 Software Distribution** — the app downloads Whisper models from Hugging Face/GitHub. Allowed (per Microsoft AI Dev Gallery), but disclose external model downloads in the listing opening (see below) and present model data clearly.
- **runFullTrust** — expected; the Partner Center warning is normal (see justification below).

### Listing opening — one disclosure covers 10.1.5 + 10.2.4.1

The description's first two lines must name the Microsoft Visual C++ Redistributable (10.2.4.1) and disclose external model downloads (10.1.5). Fold both into the opening instead of bolting on a separate jargon sentence:

> Voicetypr performs local speech-to-text on your Windows PC using speech recognition model files and the included Microsoft Visual C++ Redistributable. On first use, Voicetypr downloads the selected speech model once, stores it on your device, and then runs transcription locally without uploading your audio.

### Third-party runtime inventory (the Store MSIX ships more than the main exe)

| Ships in package | Third-party runtime | Status | Disclose? |
| --- | --- | --- | --- |
| `voicetypr.exe` | VC++ runtime (`vcomp140.dll` + CRT) | Bundled app-local | Flagged 10.2.4.1 -> disclose as hedge |
| `ffmpeg.exe` / `ffprobe.exe` | FFmpeg binaries | Bundled (integrated) | No |
| `whisper-vulkan-sidecar.exe` | Vulkan loader `vulkan-1.dll` | Not bundled; optional, auto CPU fallback | No (not required for primary functionality) |
| UI (WebView) | MS Edge WebView2 runtime | System-provided, ubiquitous (Win10 19041+/11) | No |

Do NOT bundle `vulkan-1.dll` — the loader needs the GPU driver's ICD, so bundling it alone is useless and can shadow the system loader.

## Build source of truth

- Store artifact is built by `.github/workflows/store-msix.yml` or locally with:
  ```powershell
  pnpm run windows:store:msix
  ```
- Store manifest template: `src-tauri/msix/Package.appxmanifest`
- Store config: `src-tauri/tauri.windows.store.conf.json`
- Store package script: `scripts/build-msix-store.ps1`
- Direct Windows installer path remains separate: `scripts/release-windows.ps1`

## Partner Center package identity

These values must match the product identity shown in Partner Center:

- Package/Identity/Name: `IdeaplexaLLC.Voicetypr`
- Package/Identity/Publisher: `CN=98864543-9DFC-4D03-A121-2CA8406CB004`
- Package/Properties/PublisherDisplayName: `Ideaplexa LLC`
- Package Family Name: `IdeaplexaLLC.Voicetypr_jtk8shd9pzsk0`
- Store ID: `9P8J3X9B2JG6`

Do not change these unless Partner Center product identity changes.

## MSIX manifest requirements

- `Identity Version` in the template stays `0.0.0.0`; `scripts/build-msix-store.ps1` replaces it at staging time from `package.json`.
- `TargetDeviceFamily MinVersion` must remain a real Windows version. Current value: `10.0.19041.0`.
- Never let version replacement touch `TargetDeviceFamily MinVersion`; Partner Center rejects packages targeting `Windows MinVersion <= 10.0.17134.0`.
- `runFullTrust` is expected for Tauri desktop MSIX packages.
- `internetClient` is needed for licensing/update/service calls.
- `microphone` is needed for recording.

## Bundled runtime (Visual C++)

- The Store `voicetypr.exe` is built with the static CRT (`-C target-feature=+crt-static`), but `whisper-rs` enables OpenMP on Windows, which dynamically links `vcomp140.dll` (a Visual C++ Redistributable component with no static MSVC variant).
- `scripts/build-msix-store.ps1` bundles the VC++ runtime DLLs (CRT + OpenMP) app-local, next to `voicetypr.exe` in the package, via `scripts/stage-windows-vc-runtime.ps1` (VS redist folder found with `vswhere`; the Store build passes `-Prerelease`). The build fails if `vcomp140.dll` or `vcruntime140.dll` is not staged.
- Bundling integrates the dependency, but the reviewer asks for it by name, so the listing opening still names the Microsoft Visual C++ Redistributable (see "Listing opening" above). Keep other dev jargon (Vulkan, sidecar, CPU/GPU internals) out of the description.
- The direct NSIS installer no longer ships or runs `vc_redist.x64.exe` (or the Vulkan runtime installer) either: it bundles the same app-local DLLs (staged by the same script) and its hook copies them beside `voicetypr.exe`. Both packages are self-contained; never rely on a machine-wide redistributable.

## Store vs direct updater rules

- Store installs must not use the GitHub/Tauri updater.
- Store builds disable the updater in `src-tauri/tauri.windows.store.conf.json`.
- Runtime detection uses `get_distribution_info` and `is_store_install()`.
- Direct installer users keep GitHub updater behavior.
- When changing Windows behavior, check both distribution paths: Store MSIX and direct NSIS installer.

## Required validation before uploading

For any Store MSIX artifact:

1. Run the Store MSIX workflow or local Store build.
2. Confirm the package manifest inside the artifact has:
   ```xml
   <Identity ... Version="x.y.z.0" ... />
   <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" ... />
   ```
   - `vcomp140.dll` and the VC++ CRT DLLs sit next to `voicetypr.exe` in the package (bundled app-local by the build script).
3. Confirm CI passes for frontend, macOS, and Windows.
4. Upload the `.msix` to Partner Center.
5. Partner Center warning for `runFullTrust` is expected; a hard package validation error is not.

## Restricted capability justification

Partner Center limits this field, so use the concise version (the full 3-paragraph one gets truncated):

```text
Voicetypr is a Tauri packaged desktop app; runFullTrust launches its Win32 executable from the MSIX. It needs native desktop APIs for microphone recording, system-wide global hotkeys, local audio processing, launching bundled helper binaries, and inserting transcription into the user's active app. It does not bypass user consent, access unrelated data, or run hidden background services.
```

Even shorter, if the field is tighter:

```text
Voicetypr is a Tauri MSIX desktop app. runFullTrust launches its Win32 executable and uses native desktop APIs for microphone recording, global hotkeys, local audio processing, bundled helpers, and pasting text into the active app. No hidden services or unrelated data access.
```

## Partner Center product declarations

- Properties -> Product declarations: check "This product incorporates generative AI features, which utilize artificial intelligence to create new content - including text, images, audio, video, and code."
- Required because Voicetypr's optional text cleanup sends transcripts to LLM providers (OpenAI / Anthropic / Google Gemini) that generate new text. This satisfies Store policy 11.16 (Live Generative AI Content). Keep it checked while AI enhancement ships.

## Store listing copy rules

Write for customers, not developers.

Avoid in the public description:

- Vulkan
- sidecar
- CPU/GPU implementation details
- Whisper internals
- Tauri/MSIX/package details

Use technical terms only where customers expect them, such as optional feature bullets or support docs.

## Store description

```text
Voicetypr performs local speech-to-text on your Windows PC using speech recognition model files and the included Microsoft Visual C++ Redistributable. On first use, Voicetypr downloads the selected speech model once, stores it on your device, and then runs transcription locally without uploading your audio.

Voicetypr helps you type with your voice in the apps you already use.

Press a hotkey, speak naturally, and Voicetypr places the text where your cursor is. Use it in email, chat, notes, documents, browsers, writing tools, coding tools, and everyday text fields without switching to a separate editor.

Voicetypr is built for people who want to write faster, reduce typing, capture thoughts quickly, and stay focused. It works well for quick replies, long-form dictation, notes, prompts, emails, documents, and daily writing.

Dictation works offline by default, so your voice does not need to be sent to a cloud service for everyday transcription. You can also use optional text cleanup features when you want rough dictation turned into cleaner writing.

Voicetypr includes a free trial and a lifetime license option. No subscription is required.
```

## Product features

Add as separate feature rows:

```text
Type with your voice in any app
Works in email, chat, notes, documents, and browsers
Global hotkey recording
Push-to-talk mode
Toggle recording mode
Offline dictation by default
No cloud transcription required for everyday use
Private on-device voice transcription
Optional cleaner text for emails and writing
Audio and video file transcription
Searchable local transcript history
Copy and reuse previous transcripts
Choose speed or accuracy
Manage local transcription models
Works on Windows 10 and later
Designed for fast replies and long-form writing
Helpful for reducing typing strain
Useful for writing prompts, notes, and documents
No subscription required
Lifetime license option
```

## Keywords

Partner Center allows up to 7 keywords and no more than 21 total words across all keywords. Use this safe set:

```text
voice typing
dictation app
speech to text
offline dictation
transcription
hands free typing
productivity
```

Do not use `desktop voice recorder`; it attracts the wrong user intent.

## Other listing fields

- Copyright: `© 2026 Ideaplexa LLC. All rights reserved.`
- Additional license terms: leave blank unless legal terms change.
- Developed by: `Ideaplexa LLC`
- Category: Productivity
- Pricing: if using existing external licensing, Store price can be free; disclose trial/license in the description.
- Visibility: public and discoverable when ready for real launch.
- Markets: all worldwide markets unless there is a legal/support reason to restrict.

## Screenshots and assets

Required:

- At least 1 desktop screenshot.
- Recommended: 5-8 high-quality screenshots per Microsoft guidance.

Use screenshots that show real customer workflows:

1. Main app/settings screen.
2. Recording/pill UI.
3. Model/settings screen.
4. Text appearing in email/chat/docs/browser.
5. History or file transcription if ready.

Rules:

- PNG.
- At least 1366x768.
- No private data, keys, emails, customer names, or debug logs.
- Do not upload Xbox screenshots unless Xbox is actually supported.
- Trailers are optional; skip for first launch unless a polished demo exists.

## Microsoft docs

- Submission FAQ: https://learn.microsoft.com/en-us/windows/apps/publish/faq/submit-your-app
- Store listing info for MSIX apps: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/add-and-edit-store-listing-info
