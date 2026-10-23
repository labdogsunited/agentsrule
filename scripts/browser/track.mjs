// Agent tracking T1/T2 (proposals/agent-tracking.md §17, §18): a click selects the creature the picture shows, the outline and
// the Details row follow it as it moves, Track keeps it watched while another is selected, Follow camera and its Stop, the
// Inspector shows the creature and stays when pinned, a drag still pans, and nothing moves the world's revision.
// node scripts/browser-check.mjs URL scripts/browser/track.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  await page.eval(`window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`);
  await page.wait(1200);
  await page.eval(`(async () => { document.getElementById("example").value = "nursery"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Track")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  await page.eval(`call("run.step", { world: ${W}, ticks: 20 })`);
  await page.wait(1500);
  ok(await j(`!!(track.frame && track.frame.pickable && track.frame.drawn_agents > 0)`), "frames carry a frame id and a pick index of the agents they drew");
  // The legend folded (it sits over the picture's corner), then zoom in so creatures are apart, and click one where the frame drew it.
  await page.eval(`legendOpen = false; document.getElementById("legendpanel").classList.add("closed")`);
  await page.eval(`(() => { const c = cams.picture, r = stage.getBoundingClientRect(); c.s = 6; c.x = r.width / 2 - 300 * 6; c.y = (r.height - chartsH) / 2 - 300 * 6; c.fit = false; applyCam(c); })()`);
  await page.wait(1200);
  const target = await j(`(async () => {
    const r = stage.getBoundingClientRect(), h = track.frame, k = h.map.w / h.map.world_w;
    const all = await call("entities.find", { world: ${W}, limit: 1000 });
    // A Builder (it lives through the test; a Drifter of the dish may starve while it is followed).
    for (const a of [...all.agents.filter(a => a.lineage === "Builder"), ...all.agents]) { const s = toScreen({ px: a.x * k, py: a.y * k }); if (s.x > 60 && s.y > 60 && s.x < r.width - 60 && s.y < r.height - chartsH - 60) return { id: a.id, x: r.left + s.x, y: r.top + s.y }; }
    return null;
  })()`);
  ok(target, `a creature on screen: #${target && target.id}`);
  await page.click(target.x, target.y);
  await page.wait(700);
  let cands = await j(`track.cands && track.cands.candidates.map(c => c.id)`);
  if (cands) { ok(cands.includes(target.id), `overlapping creatures give a list (${cands.length}), nearest first`); await page.eval(`chooseAgent(${JSON.stringify(target.id)})`); }
  ok(await j(`track.sel && track.sel.id`) === target.id, "the click selected the creature the picture showed");
  await page.wait(800);
  ok(await j(`!!document.querySelector("#trackov .selring")`), "the selection has an outline");
  ok(/#\d+/.test(await page.eval(`document.getElementById("selbar").innerText`)), "the selection toolbar names it");
  await page.shot(S + "track-selected.png");
  // It moves; the outline and the Details row still name the same ID.
  const before = await j(`track.frame.agents.find(a => a.selected)`);
  await page.eval(`call("run.step", { world: ${W}, ticks: 30 })`);
  await page.wait(1800);
  const after = await j(`track.frame.agents.find(a => a.selected)`);
  ok(after && after.id === target.id && after.x !== before.x, `the outline follows #${target.id} as it moves (${before.x.toFixed(1)} → ${after.x.toFixed(1)})`);
  await page.eval(`activate("details")`);
  await page.wait(1500);
  ok(await j(`(() => { const r = document.querySelector("#detailstable tr.sel"); return r && r.dataset.id; })()`) === target.id, "Details highlights its row (in the strip if the sample does not hold it)");
  // Sorting does not lose it.
  await page.eval(`(() => { const th = [...document.querySelectorAll("#detailstable th")].find(t => /^age/.test(t.textContent)); th.click(); })()`);
  await page.wait(300);
  ok(await j(`(() => { const r = document.querySelector("#detailstable tr.sel"); return r && r.dataset.id; })()`) === target.id, "the selection survives sorting");
  ok(/sorted within these rows/.test(await page.eval(`document.getElementById("detailsnote").textContent`)), "sorting a sample says it sorts the rows shown");
  // A creature outside the sample, by its exact ID.
  const outside = await j(`(async () => { const s = new Set(details.rows.map(r => r.id)); const all = await call("entities.find", { world: ${W}, limit: 1000 }); const a = all.agents.find(x => !s.has(x.id) && x.id !== ${JSON.stringify(target.id)}); return a && a.id; })()`);
  if (outside) {
    await page.eval(`(() => { const i = document.getElementById("dsearch"); i.value = ${JSON.stringify(outside)}; i.dispatchEvent(new Event("change")); })()`);
    await page.wait(1200);
    ok(await j(`track.sel.id`) === outside && await j(`details.rows.some(r => r.id === ${JSON.stringify(outside)})`), `a creature absent from the sample is found by ID (#${outside}) and shown`);
  }
  // Track the first; select the other; the first stays watched.
  await page.eval(`setAgent(${JSON.stringify(target.id)})`);
  await page.eval(`activate("viewport")`);
  await page.wait(500);
  await page.eval(`trackAgent(${JSON.stringify(target.id)})`);
  await page.wait(800);
  ok(await j(`!!watchOf(${JSON.stringify(target.id)})`), "Track made a watch");
  if (outside) await page.eval(`setAgent(${JSON.stringify(outside)})`);
  await page.eval(`call("run.step", { world: ${W}, ticks: 5 })`);
  await page.wait(1800);
  const ov = await j(`track.frame.agents.find(a => a.id === ${JSON.stringify(target.id)})`);
  ok(ov && ov.trackers.length === 1 && !ov.selected, "after selecting another, the watched one still comes with each frame, named by its tracker");
  await page.eval(`frameAgent(${JSON.stringify(target.id)})`);
  await page.wait(300);
  ok(await j(`!!document.querySelector("#trackov .tring")`), "and has its tracker-coloured ring (Frame brought it into view)");
  await page.shot(S + "track-watched.png");
  // Watches lists it; the Inspector shows the creature and stays when pinned.
  await page.eval(`activate("watches")`);
  await page.wait(1300);
  ok(/Agent \d+/.test(await page.eval(`document.getElementById("watchbody").innerText`)), "the Watches pane lists the watch");
  await page.eval(`activate("inspect")`);
  await page.eval(`setAgent(${JSON.stringify(target.id)})`);
  await page.wait(1300);
  let ins = await page.eval(`document.getElementById("inspbody").innerText`);
  ok(new RegExp(`Agent #${target.id}`).test(ins) && /OBSERVED/i.test(ins) && /tick \d+, revision 0/.test(ins), "the Inspector shows the creature with its stamp");
  await page.eval(`[...document.querySelectorAll("#inspbody .acts button")].find(b => /Pin Inspector/.test(b.textContent)).click()`);
  await page.wait(600);
  if (outside) await page.eval(`setAgent(${JSON.stringify(outside)})`);
  await page.wait(1200);
  ins = await page.eval(`document.getElementById("inspbody").innerText`);
  ok(new RegExp(`Agent #${target.id}`).test(ins), "a pinned Inspector stays on its creature while the selection moves");
  await page.shot(S + "track-inspector.png");
  await page.eval(`track.pin = null`);
  // Follow camera, a drag pauses it, Stop ends it.
  await page.eval(`activate("viewport")`);
  await page.wait(500);
  await page.eval(`followAgent(${JSON.stringify(target.id)})`);
  for (let i = 0; i < 6; i++) { await page.eval(`call("run.step", { world: ${W}, ticks: 5 })`); await page.wait(400); }
  const fol = await j(`(() => { const a = track.frame.agents.find(x => x.id === ${JSON.stringify(target.id)}); if (!a) return null; const r = stage.getBoundingClientRect(), s = toScreen(a); return { x: s.x / r.width, y: s.y / (r.height - chartsH) }; })()`);
  ok(fol && fol.x > 0.25 && fol.x < 0.75 && fol.y > 0.25 && fol.y < 0.75, `the camera keeps the followed creature in the middle (${fol && fol.x.toFixed(2)}, ${fol && fol.y.toFixed(2)})`);
  ok(/Follow camera: #/.test(await page.eval(`document.getElementById("followbar").innerText`)), "the follow bar shows with its Stop");
  const sel0 = await j(`track.sel.id`);
  const r = await j(`(() => { const b = stage.getBoundingClientRect(); return { x: b.left + b.width * 0.3, y: b.top + b.height * 0.3 }; })()`);
  await page.mouse("mousePressed", r.x, r.y); await page.mouse("mouseMoved", r.x + 60, r.y + 40); await page.mouse("mouseReleased", r.x + 60, r.y + 40, 0);
  await page.wait(400);
  ok(await j(`track.sel.id`) === sel0, "a drag pans and selects nothing");
  ok(await j(`track.follow.suspended`) && /paused/.test(await page.eval(`document.getElementById("followbar").innerText`)), "panning by hand pauses the follow, with Resume");
  await page.eval(`[...document.querySelectorAll("#followbar button")].find(b => b.textContent === "Stop").click()`);
  ok(await j(`track.follow === null`), "Stop ends it");
  // Track on click: one click selects and adds to the Watch set; Escape leaves the tool and keeps the watches.
  await page.eval(`setClickTool(true)`);
  if (outside) { await page.eval(`chooseAgent(${JSON.stringify(outside)})`); await page.wait(800); ok(await j(`track.list.trackers.some(t => t.name === "Watch set" && t.members_now.includes(${JSON.stringify(outside)}))`), "Track on click adds to the Watch set"); }
  await page.key("Escape", { vk: 27 });
  ok(await j(`!track.clickTool`) && await j(`track.list.trackers.length >= 1`), "Escape leaves the tool; the watches stay");
  await page.key("Escape", { vk: 27 });
  await page.wait(200);
  ok(await j(`track.sel === null`), "Escape clears the selection");
  ok(await j(`(async () => (await call("stats.query", { world: ${W} })).revision)()`) === 0, "nothing in tracking moved the world's revision");
  const errs = await j(`__errs`);
  ok(errs.length === 0, `no page errors (${errs.join(" | ")})`);
  await page.shot(S + "track-end.png");
};
