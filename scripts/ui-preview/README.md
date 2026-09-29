# UI preview

Run `pnpm ui:preview` to capture the real main-window UI in headless installed Google Chrome. It starts a temporary Vite server and writes screenshots to `.tmp/ui-preview/`. Pass `--url http://127.0.0.1:1420` to use an existing dev server, or `--only macos-light-history-transcribe-file` to capture filenames containing that substring. The preview page is `ui-preview.html` and accepts `theme=light|dark`, `platform=macos|windows`, and `empty=1` for empty History and Dictionary fixtures.

Fixtures contain invented demo data and mock Tauri commands. The script prints unknown commands; add typed fixtures for commands that a screen needs. `ui-preview.html` is a development entry only and is absent from production Vite build inputs.

`pnpm ui:preview --only pill-` captures all ten pill states for both platforms and both page backgrounds. Open `pill-preview.html?state=preview&theme=light&platform=macos` to inspect the vanilla DOM controller with mocked events. Supported states: `idle`, `listening`, `preview`, `transcribing`, `formatting`, `pasted`, `copied`, `no_permission`, `error`, `too_short`; add `style=compact` to inspect compact detail. Terminal feedback is frozen for capture. This development entry is excluded from production build inputs.

These previews use a wide browser viewport. The unchanged native pill window is 260 × 64px, so the spec's 340px live preview still requires native smoke verification; long Accessibility guidance wraps to fit the unchanged panel. Browser fixtures cannot prove focus or panel behavior.
