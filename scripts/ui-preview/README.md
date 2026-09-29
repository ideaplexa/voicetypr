# UI preview

Run `pnpm ui:preview` to capture the real main-window UI in headless installed Google Chrome. It starts a temporary Vite server and writes screenshots to `.tmp/ui-preview/`. Pass `--url http://127.0.0.1:1420` to use an existing dev server, or `--only macos-light-history-transcribe-file` to capture filenames containing that substring. The preview page is `ui-preview.html` and accepts `theme=light|dark`, `platform=macos|windows`, and `empty=1` for empty History and Dictionary fixtures.

Fixtures contain invented demo data and mock Tauri commands. The script prints unknown commands; add typed fixtures for commands that a screen needs. `ui-preview.html` is a development entry only and is absent from production Vite build inputs.
