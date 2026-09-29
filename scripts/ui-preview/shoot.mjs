import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { chromium } from "playwright-core";

const chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const output = path.resolve(".tmp/ui-preview");
const arg = process.argv.indexOf("--url");
const suppliedUrl = arg >= 0 ? process.argv[arg + 1] : null;
const onlyArg = process.argv.indexOf("--only");
const only = onlyArg >= 0 ? process.argv[onlyArg + 1] : null;
if (onlyArg >= 0 && (!only || only.startsWith("--"))) {
  throw new Error("--only requires a screenshot filename substring");
}
const port = suppliedUrl ? null : await new Promise((resolve, reject) => {
  const server = createServer();
  server.once("error", reject);
  server.listen(0, "127.0.0.1", () => {
    const address = server.address();
    const selected = typeof address === "object" && address ? address.port : 0;
    server.close(() => resolve(selected));
  });
});
const baseUrl = suppliedUrl ?? `http://127.0.0.1:${port}`;
const vite = suppliedUrl ? null : spawn(process.execPath, ["node_modules/vite/bin/vite.js", "--host", "127.0.0.1", "--port", String(port), "--strictPort"], { stdio: ["ignore", "pipe", "pipe"] });
let viteLog = "";
for (const stream of [vite?.stdout, vite?.stderr]) stream?.on("data", (chunk) => { viteLog += String(chunk); });

async function waitForServer() {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (vite && vite.exitCode !== null) throw new Error(`Vite exited early:\n${viteLog}`);
    try {
      const response = await fetch(`${baseUrl}/ui-preview.html`);
      if (response.ok) return;
    } catch { /* Server is starting. */ }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Vite did not become ready:\n${viteLog}`);
}

const macScreens = [
  ["Home", "home"], ["History", "history"], ["Transcription", "transcription"],
  ["Polish", "polish"], ["Dictionary", "dictionary"], ["Recording", "recording"], ["Settings", "settings-general"],
  ["Help & feedback", "help"],
];
const panes = [
  ["Shortcuts", "settings-shortcuts"], ["Privacy", "settings-privacy"], ["Storage", "settings-storage"], ["Network sharing", "settings-network"],
  ["CLI & API", "settings-agent"], ["Troubleshooting", "settings-advanced"],
];
const shouldCapture = (platform, theme, name) => !only || `${platform}-${theme}-${name}.png`.includes(only);
const errors = [];
const unknown = new Set();
const shots = [];
let browser;
let runError;
try {
  await waitForServer();
  await mkdir(output, { recursive: true });
  browser = await chromium.launch({ executablePath: chrome, headless: true, args: ["--no-sandbox"] });
  for (const platform of ["macos", "windows"]) {
    for (const theme of ["light", "dark"]) {
      for (const phase of [1, 2, 3]) {
        const name = `onboarding-${phase}`;
        if (!shouldCapture(platform, theme, name)) continue;
        const page = await browser.newPage({ viewport: { width: 1000, height: 680 }, deviceScaleFactor: 2 });
        page.on("pageerror", (error) => errors.push(`${platform}/${theme}/${name}: ${error.stack ?? error}`));
        page.on("console", (message) => { if (message.text().startsWith("[ui-preview] unknown command:")) unknown.add(message.text()); });
        await page.goto(`${baseUrl}/ui-preview.html?platform=${platform}&theme=${theme}&onboarding=${phase}`, { waitUntil: "networkidle" });
        await page.getByRole("progressbar", { name: "Setup phase" }).waitFor();
        await page.evaluate(() => document.fonts.ready);
        await page.screenshot({ path: path.join(output, `${platform}-${theme}-${name}.png`), animations: "disabled" });
        shots.push(`${platform}-${theme}-${name}.png`);
        await page.close();
      }
    }
  }
  for (const platform of ["macos", "windows"]) {
    for (const theme of ["light", "dark"]) {
      for (const state of ["idle", "listening", "preview", "transcribing", "formatting", "pasted", "copied", "no_permission", "error", "too_short"]) {
        const name = `pill-${state}`;
        if (!shouldCapture(platform, theme, name)) continue;
        const page = await browser.newPage({ viewport: { width: 760, height: 180 }, deviceScaleFactor: 2 });
        page.on("pageerror", (error) => errors.push(`${platform}/${theme}/${name}: ${error.stack ?? error}`));
        await page.goto(`${baseUrl}/pill-preview.html?platform=${platform}&theme=${theme}&state=${state}`, { waitUntil: "networkidle" });
        await page.waitForSelector('html[data-pill-preview-ready="true"]');
        await page.evaluate(() => document.fonts.ready);
        await page.screenshot({ path: path.join(output, `${platform}-${theme}-${name}.png`), animations: "disabled" });
        shots.push(`${platform}-${theme}-${name}.png`);
        await page.close();
      }
    }
  }
  for (const platform of ["macos", "windows"]) {
    for (const theme of ["light", "dark"]) {
      const screens = platform === "macos" ? macScreens : macScreens.filter(([, id]) => ["home", "history", "recording", "settings-general", "help"].includes(id));
      const selectedPanes = panes;
      const hasShot = screens.some(([, id]) => shouldCapture(platform, theme, id) || (id === "history" && ["history-detail", "history-empty", "history-transcribe-file"].some((shot) => shouldCapture(platform, theme, shot))) || (id === "settings-general" && selectedPanes.some(([, paneId]) => shouldCapture(platform, theme, paneId)))) || shouldCapture(platform, theme, "license");
      if (!hasShot) continue;
      const page = await browser.newPage({ viewport: { width: 1000, height: 680 }, deviceScaleFactor: 2 });
      page.on("pageerror", (error) => errors.push(`${platform}/${theme}: ${error.stack ?? error}`));
      page.on("console", (message) => {
        const line = message.text();
        if (line.startsWith("[ui-preview] unknown command:")) unknown.add(line);
      });
      await page.goto(`${baseUrl}/ui-preview.html?platform=${platform}&theme=${theme}`, { waitUntil: "networkidle" });
      const capture = async (name) => {
        if (!shouldCapture(platform, theme, name)) return;
        await page.screenshot({ path: path.join(output, `${platform}-${theme}-${name}.png`), animations: "disabled" });
        shots.push(`${platform}-${theme}-${name}.png`);
      };
      for (const [label, id] of screens) {
        if (!shouldCapture(platform, theme, id) && !(id === "history" && ["history-detail", "history-empty", "history-transcribe-file"].some((shot) => shouldCapture(platform, theme, shot))) && !(id === "settings-general" && selectedPanes.some(([, paneId]) => shouldCapture(platform, theme, paneId)))) continue;
        await page.getByRole("navigation", { name: label === "Settings" || label === "Help & feedback" ? "Support navigation" : "Main navigation" }).getByRole("button", { name: label, exact: true }).click();
        await page.waitForTimeout(180);
        await capture(id);
        if (id === "history") {
          await capture("history-detail");
          if (shouldCapture(platform, theme, "history-empty")) {
            const emptyPage = await browser.newPage({ viewport: { width: 1000, height: 680 }, deviceScaleFactor: 2 });
            emptyPage.on("pageerror", (error) => errors.push(`${platform}/${theme}/empty: ${error.stack ?? error}`));
            await emptyPage.goto(`${baseUrl}/ui-preview.html?platform=${platform}&theme=${theme}&empty=1`, { waitUntil: "networkidle" });
            await emptyPage.getByRole("navigation", { name: "Main navigation" }).getByRole("button", { name: "History", exact: true }).click();
            await emptyPage.waitForTimeout(180);
            await emptyPage.screenshot({ path: path.join(output, `${platform}-${theme}-history-empty.png`), animations: "disabled" });
            shots.push(`${platform}-${theme}-history-empty.png`);
            await emptyPage.close();
          }
        }
        if (id === "history" && shouldCapture(platform, theme, "history-transcribe-file")) {
          await page.getByRole("button", { name: "Transcribe a file…" }).click();
          await page.getByRole("dialog", { name: "Transcribe a file…" }).waitFor();
          await capture("history-transcribe-file");
          await page.keyboard.press("Escape");
        }
        if (id === "settings-general") {
          for (const [paneLabel, paneId] of selectedPanes) {
            if (!shouldCapture(platform, theme, paneId)) continue;
            await page.getByRole("navigation", { name: "Settings panes" }).getByRole("button", { name: paneLabel, exact: true }).click();
            await page.waitForTimeout(180);
            await capture(paneId);
          }
        }
      }
      if (shouldCapture(platform, theme, "license")) {
        await page.getByRole("button", { name: /Pro\. Open License/ }).click();
        await page.waitForTimeout(180);
        await capture("license");
      }
      await page.close();
    }
  }
} catch (error) {
  runError = error;
  const message = error instanceof Error ? error.message : String(error);
  console.error(`UI preview failed: ${message.includes("SIGABRT") ? "Installed Chrome exited with SIGABRT before opening a page" : message.split("\n")[0]}`);
} finally {
  await browser?.close();
  vite?.kill("SIGTERM");
  console.log(`Screenshots (${shots.length}): ${shots.join(", ")}`);
  console.log(shots.length === 0 && runError ? "Unknown commands: not observed (no page opened)" : `Unknown commands (${unknown.size}): ${[...unknown].join(", ") || "none"}`);
  if (errors.length) console.error(`Page errors:\n${errors.join("\n")}`);
}
if (errors.length || runError) process.exitCode = 1;
