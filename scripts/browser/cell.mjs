// Node system H5, inspection of a cell (proposals/houdini-node-system.md §10, §14 step 7): a click on the picture where no
// creature is selects the cell; the Inspector shows what it holds at one tick, the statements that touch its field (each a
// click away in the Network), and a bounded recording of what moved through it.
// node scripts/browser-check.mjs URL scripts/browser/cell.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed).
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  await page.eval(`window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`);
  await page.wait(1000);
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Track")`);
  await page.wait(800);
  const W = `document.getElementById("world").value`;
  await page.eval(`call("run.step", { world: ${W}, ticks: 10 })`);
  await page.wait(1500);
  await page.eval(`activate("inspect")`);
  await page.eval(`(() => { const c = cams.picture, r = stage.getBoundingClientRect(); c.s = 8; c.x = r.width / 2 - 520 * 8; c.y = (r.height - chartsH) / 2 - 520 * 8; c.fit = false; applyCam(c); })()`);
  await page.wait(1500);
  // A point of the picture with no creature near it: ask the frame, then click there.
  const spot = await j(`(async () => {
    const r = stage.getBoundingClientRect(), h = track.frame;
    for (let i = 0; i < 60; i++) {
      const x = r.left + r.width * (0.3 + 0.4 * ((i * 37) % 60) / 60), y = r.top + (r.height - chartsH) * (0.3 + 0.4 * ((i * 23) % 60) / 60), p = pointToWorld(x, y);
      if (!p) continue;
      const a = await call("view.pick_frame", { subscription: sub, frame: h.frame, x: p.px, y: p.py, tolerance: 10 / cams.picture.s, limit: 1 });
      if (!a.candidates.length) return { x, y };
    }
    return null;
  })()`);
  ok(spot, "a spot of the picture with no creature");
  await page.click(spot.x, spot.y);
  await page.wait(1500);
  ok(await j(`!!track.cell && track.inspCell`) && await j(`!!document.querySelector("#trackov .cellsel")`), `the click selected the cell (${await j(`track.cell && [track.cell.ix, track.cell.iy]`)}) and outlines it`);
  const ins = await page.eval(`document.getElementById("inspbody").innerText`);
  ok(/Cell \(\d+, \d+\)/.test(ins) && /OBSERVED/i.test(ins) && /carbon/.test(ins), "the Inspector shows what the cell holds");
  ok(/weather/.test(ins) && /DECLARED/i.test(ins), "and the statements that touch its field");
  // A bounded recording of what moves through the cell.
  await page.eval(`[...document.querySelectorAll("#inspbody button")].find(b => /Record what moves/.test(b.textContent)).click()`);
  await page.wait(800);
  await page.eval(`call("run.step", { world: ${W}, ticks: 12 })`);
  await page.wait(500);
  await page.eval(`loadInspect()`);
  await page.wait(1500);
  const ms = await page.eval(`document.getElementById("inspbody").innerText`);
  ok(/requests kept/.test(ms) && /complete/.test(ms) && /cell \d+/.test(ms), "after the ticks, the recording lists what moved through it, and says whether it is complete");
  await page.shot(S + "cell-inspector.png");
  const mine = await j(`call("trace.query", { world: ${W}, limit: 5000 }).then(t => t.records.every(r => r.from === "cell " + track.cell.index || r.to === "cell " + track.cell.index))`);
  ok(mine, "every kept request has the cell at one end");
  // A statement in the list goes to the Network.
  await page.eval(`[...document.querySelectorAll("#inspbody a.go")].find(a => /weather/.test(a.textContent)).click()`);
  await page.wait(1200);
  ok(/weather/.test(await page.eval(`selected || ""`)) && await page.eval(`ctx`) === "rule:weather", "a statement of the list is shown in its rule's network");
  ok(await j(`call("stats.query", { world: ${W} }).then(r => r.revision)`) === 0, "nothing moved the world's revision");
  const e = await j(`__errs`);
  ok(e.length === 0, `no page errors (${e.join(" | ")})`);
};
