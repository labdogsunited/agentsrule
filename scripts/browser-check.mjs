// Drives the viewer in a real (headless) Chromium over the DevTools protocol, for checks that an
// API test cannot make: focus, keys, drag, what is on screen.
//   node scripts/browser-check.mjs URL SCRIPT.mjs
// SCRIPT.mjs exports `default async (page) => {...}`; `page` has eval(js), key(...), click(x, y),
// shot(path), wait(ms). Exit status 1 if the script throws. Chromium: $CHROMIUM or the Playwright copy.
import { spawn } from "node:child_process";
import { readdirSync, writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [url, script] = process.argv.slice(2);
const pw = join(homedir(), ".cache/ms-playwright");
const chrome = process.env.CHROMIUM || join(pw, readdirSync(pw).find((d) => d.startsWith("chromium-")), "chrome-linux64/chrome");
const dir = mkdtempSync(join(tmpdir(), "emerge-browser-"));
const port = 9300 + Math.floor(Math.random() * 500);
const proc = spawn(chrome, ["--headless=new", "--no-sandbox", `--remote-debugging-port=${port}`, `--user-data-dir=${dir}`, "--window-size=1500,950", "--hide-scrollbars", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let targets;
for (let i = 0; i < 75 && !targets; i++) {
  try { const t = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (t.some((x) => x.type === "page")) targets = t; else await sleep(200); } catch (_) { await sleep(200); }
}
if (!targets) { console.error("FAILED: no page target from", chrome); proc.kill(); process.exit(1); }
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const pending = new Map();
ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } };
const send = (method, params = {}) => new Promise((r) => { pending.set(++id, r); ws.send(JSON.stringify({ id, method, params })); });
const page = {
  async eval(js) { const r = await send("Runtime.evaluate", { expression: js, awaitPromise: true, returnByValue: true }); if (r.result.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "eval failed"); return r.result.result.value; },
  async key(key, opts = {}) { const code = key.length === 1 ? `Key${key.toUpperCase()}` : key; const base = { key, code, windowsVirtualKeyCode: key.length === 1 ? key.toUpperCase().charCodeAt(0) : opts.vk || 0, modifiers: opts.modifiers || 0 }; await send("Input.dispatchKeyEvent", { type: "keyDown", text: key.length === 1 ? key : undefined, ...base }); await send("Input.dispatchKeyEvent", { type: "keyUp", ...base }); },
  async click(x, y) { for (const type of ["mousePressed", "mouseReleased"]) await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 }); },
  async shiftMouse(type, x, y, buttons = 1) { await send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons, clickCount: 1, modifiers: 8 }); },
  async mouse(type, x, y, buttons = 1) { await send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons }); },
  async shot(path) { const r = await send("Page.captureScreenshot", { format: "png" }); writeFileSync(path, Buffer.from(r.result.data, "base64")); },
  wait: sleep,
};
let code = 0;
try {
  await send("Page.enable");
  await send("Page.navigate", { url });
  await sleep(1500);
  await (await import(pathToFileURL(resolve(script)).href)).default(page);
} catch (e) { console.error("FAILED:", e.message); code = 1; }
ws.close(); proc.kill();
process.exit(code);
