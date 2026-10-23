// Panes (the user's list of 2026-10-23): tab strips scroll with the wheel, the Look network pans and zooms, the Flow Map
// is three columns that pan, zoom and highlight one node's wires, the Rule Lens shows one tick as columns of steps, and the
// Source shows one part of the world at a time.
// node scripts/browser-check.mjs URL scripts/browser/panes.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed).
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  await page.eval(`window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`);
  await page.wait(1000);
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Simulate")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  await page.eval(`call("run.step", { world: ${W}, ticks: 20 })`);
  // Tabs: the strip with many tabs scrolls sideways with the wheel.
  const strip = await j(`(() => { const t = [...document.querySelectorAll(".lhead .tabs")].find(x => x.scrollWidth > x.clientWidth); if (!t) return null; const r = t.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, before: t.scrollLeft }; })()`);
  ok(strip, "a tab strip wider than its pane");
  await page.eval(`(() => { const t = document.elementFromPoint(${strip.x}, ${strip.y}).closest(".tabs"); t.dispatchEvent(new WheelEvent("wheel", { deltaY: 240, bubbles: true, cancelable: true })); })()`);
  await page.wait(200);
  ok(await j(`[...document.querySelectorAll(".lhead .tabs")].some(x => x.scrollLeft > 0)`), "the wheel scrolls the tabs sideways");
  // Look: pan and zoom.
  await page.eval(`activate("look")`);
  await page.wait(2000);
  const c0 = await j(`({ ...lookCam })`);
  await page.eval(`(() => { const sc = document.getElementById("lookscroll"), r = sc.getBoundingClientRect(); sc.dispatchEvent(new WheelEvent("wheel", { deltaY: -300, clientX: r.left + 100, clientY: r.top + 100, bubbles: true, cancelable: true })); })()`);
  ok(await j(`lookCam.s`) > c0.s, `the Look network zooms (${c0.s.toFixed(2)} → ${(await j(`lookCam.s`)).toFixed(2)})`);
  const sc = await j(`(() => { const r = document.getElementById("lookscroll").getBoundingClientRect(); return { x: r.right - 20, y: r.bottom - 20 }; })()`);
  const x0 = await j(`lookCam.x`);
  await page.mouse("mousePressed", sc.x, sc.y); for (let i = 1; i <= 5; i++) await page.mouse("mouseMoved", sc.x - i * 12, sc.y - i * 6); await page.mouse("mouseReleased", sc.x - 60, sc.y - 30, 0);
  ok(await j(`lookCam.x`) < x0 - 30, "and pans with a drag on empty space");
  // Flow Map.
  await page.eval(`activate("flow")`);
  await page.wait(1500);
  const fm = await j(`({ cols: ["in:", "r:", "out:"].map(p => [...document.querySelectorAll("#flowg .fn")].filter(n => n.dataset.id.startsWith(p)).length), shown: document.getElementById("flowshow").value, all: document.querySelectorAll("#flowg .fe").length })`);
  ok(fm.cols.every((n) => n > 0) && fm.shown === "used", `the Flow Map is three columns: ${fm.cols.join(" consumed, ")} made (named and active reactions)`);
  await page.eval(`(() => { const n = [...document.querySelectorAll("#flowg .fn")].find(n => n.dataset.id === "r:photosynthesis"); n.dispatchEvent(new MouseEvent("click", { bubbles: true })); })()`);
  const dim = await j(`({ dim: document.querySelectorAll("#flowg .fe.dim").length, all: document.querySelectorAll("#flowg .fe").length })`);
  ok(dim.dim > 0 && dim.dim < dim.all, `clicking a reaction keeps only its wires bright (${dim.all - dim.dim} of ${dim.all})`);
  const f0 = await j(`flowCam.s`);
  await page.eval(`(() => { const b = document.getElementById("flowmap"), r = b.getBoundingClientRect(); b.dispatchEvent(new WheelEvent("wheel", { deltaY: -300, clientX: r.left + 50, clientY: r.top + 50, bubbles: true, cancelable: true })); })()`);
  ok(await j(`flowCam.s`) > f0, "the Flow Map zooms");
  await page.shot(S + "panes-flow.png");
  // Rule Lens: one tick as columns.
  await page.eval(`activate("rules")`);
  await page.wait(1500);
  const lens = await j(`({ cols: document.querySelectorAll("#rulesbody .lens-col").length, chips: document.querySelectorAll("#rulesbody .lens-chip").length, text: document.getElementById("rulesbody").innerText.slice(0, 200) })`);
  ok(lens.cols === 4 && lens.chips >= 4 && /One tick/.test(lens.text), `the Rule Lens is one tick in ${lens.cols} columns with ${lens.chips} rules placed`);
  await page.eval(`[...document.querySelectorAll("#rulesbody .lens-chip")].find(c => /weather/.test(c.textContent)).click()`);
  ok(await j(`[...document.querySelectorAll("#rulesbody .lens-stmts")].some(s => !s.hidden && /transfer\\(carbon/.test(s.textContent))`), "a rule opens to its statements");
  await page.shot(S + "panes-lens.png");
  // Source: one part at a time.
  await page.eval(`activate("source")`);
  await page.wait(1500);
  const parts = await j(`[...document.getElementById("codepart").options].map(o => o.textContent)`);
  ok(parts.some((p) => /Chemistry/.test(p)) && parts.some((p) => /Lineage/.test(p)), `the Source lists its parts: ${parts.slice(1, 6).join(" | ")}…`);
  await page.eval(`(() => { const s = document.getElementById("codepart"); s.value = [...s.options].find(o => /Chemistry/.test(o.textContent)).value; s.dispatchEvent(new Event("change")); })()`);
  const shown = await j(`[...document.querySelectorAll("#codebody .cl:not(.hide)")].map(r => r.textContent)`);
  ok(shown.some((l) => /reaction photosynthesis/.test(l)) && !shown.some((l) => /rule weather/.test(l)) && await j(`document.querySelectorAll("#codebody .cfold").length`) > 0, `choosing Chemistry shows its ${shown.length} lines and folds the rest`);
  await page.shot(S + "panes-source.png");
  const e = await j(`__errs`);
  ok(e.length === 0, `no page errors (${e.join(" | ")})`);
};
