// Builds the three self-contained first-run prototypes from src/.
// Usage: node design/prototypes/first-run/build.mjs [node_modules dir holding @fontsource-variable]
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const src = (name) => readFileSync(join(here, "src", name), "utf8");
const modules = [process.argv[2], join(here, "../../../node_modules")].find(
  (dir) => dir && existsSync(join(dir, "@fontsource-variable/geist")),
);
if (!modules) throw new Error("Geist not found: pass a node_modules directory that has @fontsource-variable");
const font = (pkg, file, family) =>
  '@font-face{font-family:"' + family + '";font-weight:100 900;font-display:block;src:url(data:font/woff2;base64,' +
  readFileSync(join(modules, "@fontsource-variable", pkg, "files", file)).toString("base64") + ') format("woff2")}';
const fonts =
  font("geist", "geist-latin-wght-normal.woff2", "Geist") + "\n" +
  font("geist-mono", "geist-mono-latin-wght-normal.woff2", "Geist Mono");

const dark = "color-scheme:dark;--background:#141415;--foreground:#F2F2F0;--card:#1C1C1E;--primary:#F2F2F0;--primary-foreground:#141415;--secondary:#242426;--muted:#242426;--muted-foreground:#A3A3A0;--accent:#2a2a2d;--destructive:#F28B7A;--border:#2C2C2F;--sage:#8FD1A8;--sage-foreground:#141415;--sage-bg:#1F3328;--text-3:#8E8E8A;--warn:#F2B35B;--warn-bg:#3A2B18;--shadow-sm:0 1px 2px rgba(0,0,0,.22);--shadow-lg:0 10px 30px -12px rgba(0,0,0,.38);--shadow-xl:0 24px 60px -24px rgba(0,0,0,.46);--shadow-2xl:0 30px 90px -30px rgba(0,0,0,.55);--os-panel:rgba(44,44,46,.96);--os-text:#f2f2f2;--os-sub:#b4b4b8;--os-btn:#5a5a5e;--os-line:rgba(255,255,255,.14);--scrim:rgba(0,0,0,.45)";
const darkCss =
  "@media (prefers-color-scheme:dark){:root:not([data-theme=\"light\"]){" + dark + "}}\n:root[data-theme=\"dark\"]{" + dark + "}";

const css = src("base.css").replace("/*FONTS*/", () => fonts).replace("/*DARK*/", () => darkCss);
const js = src("base.js");
for (const name of ["a-quiet-page", "b-big-sentence", "c-founder-note"]) {
  const html = src(name + ".html").replace("/*BASE_CSS*/", () => css).replace("/*BASE_JS*/", () => js);
  writeFileSync(join(here, "first-run-" + name + ".html"), html);
  console.log("first-run-" + name + ".html", Math.round(html.length / 1024) + " KB");
}
