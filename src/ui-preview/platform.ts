import type { PreviewOptions } from "./fixtures";

export function installPreviewPlatform(platform: PreviewOptions["platform"]) {
  window.__TAURI_OS_PLUGIN_INTERNALS__ = {
    os_type: platform,
    platform,
    version: platform === "macos" ? "14.0" : "10.0",
    family: platform === "macos" ? "unix" : "windows",
    arch: "x86_64",
    eol: platform === "macos" ? "\n" : "\r\n",
    exe_extension: platform === "macos" ? "" : "exe",
  };
}
