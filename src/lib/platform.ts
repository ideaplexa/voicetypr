import { type } from "@tauri-apps/plugin-os";

export type Platform = "darwin" | "windows" | "linux";

export const isMacOS: boolean = type() === "macos";

export const isWindows: boolean = type() === "windows";

export const isLinux: boolean = type() === "linux";
