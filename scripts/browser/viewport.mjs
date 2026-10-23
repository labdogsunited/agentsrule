// The viewport (the user's list of 2026-10-23): creatures drawn as vector glyphs (sharp at any zoom), the legend beside the
// picture and not part of it, the cell grid, rulers and the crosshair, the onion skin, one Play/Pause button, and the
// scrubber over the kept frames (the world is not rewound).
// node scripts/browser-check.mjs URL scripts/browser/viewport.mjs
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
  await page.eval(`call("run.step", { world: ${W}, ticks: 5 })`);
  await page.wait(1500);
  const f = await j(`({ glyphs: pic.count, img: !!pic.img, w: lastImg && lastImg.nw, map: lastImg && lastImg.map && lastImg.map.w, legend: getComputedStyle(document.getElementById("legendpanel")).display })`);
  ok(f.glyphs > 100 && f.w === f.map, `creatures come as ${f.glyphs} vector glyphs and the picture is the map alone (${f.w} px wide, no legend in it)`);
  ok(f.legend === "block" && /founding lineage/.test(await page.eval(`document.getElementById("legendpanel").innerText`)), "the legend is a panel beside the picture, saying what the colours are");
  // Zoom far in: the glyphs are drawn at the screen's resolution.
  await page.eval(`(() => { const c = cams.picture, r = stage.getBoundingClientRect(); const a = new DataView(pic.glyphs.buffer, pic.glyphs.byteOffset, 24); const px = a.getFloat32(0, true), py = a.getFloat32(4, true); c.s = 30; c.x = r.width / 2 - px * 30; c.y = (r.height - chartsH) / 2 - py * 30; c.fit = false; applyCam(c); })()`);
  await page.wait(500);
  const lp = await j(`document.getElementById("legendpanel").getBoundingClientRect().right <= stage.getBoundingClientRect().right + 1`);
  ok(lp, "zooming in leaves the legend where it is");
  await page.shot(S + "viewport-zoom.png");
  // The grid, the rulers, the crosshair.
  await page.eval(`document.getElementById("gridbtn").click(); document.getElementById("rulerbtn").click(); document.getElementById("crossbtn").click()`);
  const r = await j(`(() => { const b = stage.getBoundingClientRect(); return { x: b.left + b.width * 0.4, y: b.top + b.height * 0.3 }; })()`);
  await page.mouse("mouseMoved", r.x, r.y, 0);
  await page.wait(300);
  ok(await j(`showGrid && guides.ruler && guides.cross && !!guides.ptr`), "grid, rulers and crosshair are on and follow the pointer");
  await page.shot(S + "viewport-guides.png");
  // Onion skin.
  await page.eval(`document.getElementById("onionbtn").click()`);
  await page.eval(`(() => { const c = cams.picture; c.fit = true; fitCam(c); applyCam(c); })()`);
  for (let i = 0; i < 4; i++) { await page.eval(`call("run.step", { world: ${W}, ticks: 1 })`); await page.wait(700); }
  ok(await j(`pic.onion.length`) >= 2, "the onion skin keeps the last frames' creatures");
  // One Play/Pause button.
  await page.eval(`document.getElementById("play").click()`);
  await page.wait(1500);
  ok(/Pause/.test(await page.eval(`document.getElementById("play").textContent`)), "Play becomes Pause while the world runs");
  await page.eval(`document.getElementById("play").click()`);
  await page.wait(1200);
  ok(/Play/.test(await page.eval(`document.getElementById("play").textContent`)), "and back");
  // Scrub back to a kept frame, then Live.
  await page.eval(`loadTimeline()`);
  await page.wait(300);
  const tl = await j(`scrub.ticks`);
  ok(tl.length >= 3, `kept frames to scrub over: ${tl.slice(0, 6).join(", ")}…`);
  await page.eval(`showTick(${tl[1]})`);
  await page.wait(1200);
  const past = await j(`({ tick: pic.tick, info: document.getElementById("scrubinfo").textContent, live: scrub.live })`);
  ok(past.tick === tl[1] && /kept frame/.test(past.info), `scrubbing shows the kept frame of tick ${tl[1]} (${past.info})`);
  ok(await j(`call("stats.query", { world: ${W} }).then(x => x.tick)`) === past.live, "and the world was not rewound");
  await page.shot(S + "viewport-scrub.png");
  await page.eval(`document.getElementById("scrublive").click()`);
  await page.wait(1200);
  ok(await j(`pic.tick`) === past.live && /live/.test(await page.eval(`document.getElementById("scrubinfo").textContent`)), "Live comes back to the world as it is");
  const e = await j(`__errs`);
  ok(e.length === 0, `no page errors (${e.join(" | ")})`);
};
