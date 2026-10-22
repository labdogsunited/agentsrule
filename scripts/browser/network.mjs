// Network H1: multi-select, group drag, H/G, Find, notes and boxes. node scripts/browser-check.mjs URL scripts/browser/network.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`JSON.stringify(${js})`));
  await page.eval(`(async () => { document.getElementById("example").value = "nursery"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  const rev0 = await page.eval(`call("world.graph", { world: ${W} }).then(g => g.revision)`);
  const ids = await j(`[...document.querySelectorAll("#graph .box")].slice(0, 12).map(b => b.dataset.id)`);
  const center = async (id) => j(`(() => { const r = document.querySelector('#graph .box[data-id="${id}"]').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  // pick three boxes that are on screen
  const vis = await j(`[...document.querySelectorAll("#graph .box")].filter(b => { const r = b.getBoundingClientRect(), s = document.getElementById("stage-graph").getBoundingClientRect(); return r.left > s.left + 5 && r.right < s.right - 5 && r.top > s.top + 5 && r.bottom < s.bottom - 5 && (document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) || {closest(){return null}}).closest(".box") === b; }).slice(0, 4).map(b => b.dataset.id)`);
  ok(vis.length >= 3, "boxes on screen: " + vis.length);
  // plain click selects one; shift-click adds
  const c0 = await center(vis[0]);
  await page.click(c0.x, c0.y);
  await page.wait(500);
  const shiftClick = async (x, y) => { for (const type of ["mousePressed", "mouseReleased"]) await page.shiftMouse(type, x, y); };
  const c1 = await center(vis[1]);
  await shiftClick(c1.x, c1.y);
  let m = await j(`({ multi: [...multi], sel: selected, msel: document.querySelectorAll("#graph .box.msel").length })`);
  ok(m.multi.length === 2 && m.msel === 2, "shift-click extends the selection to two boxes");
  // dragging one selected box moves both and saves them in the sidecar; no revision moves
  const before = await j(`({ a: placed.get(${JSON.stringify(vis[0])}).x, b: placed.get(${JSON.stringify(vis[1])}).x })`);
  const c1b = await center(vis[1]);
  await page.mouse("mousePressed", c1b.x, c1b.y);
  for (let i = 1; i <= 6; i++) await page.mouse("mouseMoved", c1b.x + i * 8, c1b.y + i * 4);
  await page.mouse("mouseReleased", c1b.x + 48, c1b.y + 24, 0);
  await page.wait(500);
  const after = await j(`({ a: placed.get(${JSON.stringify(vis[0])}).x, b: placed.get(${JSON.stringify(vis[1])}).x })`);
  ok(after.a > before.a + 10 && after.b > before.b + 10, `group drag moved both boxes (${before.a}->${after.a}, ${before.b}->${after.b})`);
  const saved = await page.eval(`call("world.graph", { world: ${W} }).then(g => JSON.stringify({ rev: g.revision, a: g.positions[${JSON.stringify(vis[0])}], b: g.positions[${JSON.stringify(vis[1])}] }))`);
  const sv = JSON.parse(saved);
  ok(sv.rev === rev0 && Math.abs(sv.a[0] - after.a) < 1 && Math.abs(sv.b[0] - after.b) < 1, "both positions were saved; the revision did not move");
  // keys: H frames all, G frames selection, and neither grids; typing in Find keeps normal typing
  const grid0 = await page.eval(`netGrid`);
  await page.eval(`cams.graph.fit = false; cams.graph.s = 0.3; applyCam(cams.graph)`);
  await page.eval(`mode = "graph"`);
  await page.key("h");
  ok(await page.eval(`cams.graph.fit === true`), "H frames everything");
  await page.key("g");
  const camG = await j(`({ s: cams.graph.s, fit: cams.graph.fit })`);
  ok(camG.fit === false && camG.s > 0.3 && (await page.eval(`netGrid`)) === grid0, "G frames the selection and no longer toggles the grid");
  await page.eval(`document.getElementById("netfind").focus()`);
  const sBefore = await page.eval(`cams.graph.s`);
  for (const ch of "sungh") await page.key(ch);
  ok((await page.eval(`document.getElementById("netfind").value`)) === "sungh" && (await page.eval(`cams.graph.s`)) === sBefore, "G and H typed in the Find box stay text");
  await page.eval(`document.getElementById("netfind").value = "sunlight"; applyNetFind()`);
  const f = await j(`({ faded: document.querySelectorAll("#graph .box.nomatch").length, total: document.querySelectorAll("#graph .box").length, note: document.getElementById("netfound").textContent })`);
  ok(f.faded > 0 && f.faded < f.total && /found/.test(f.note), `Find fades the rest (${f.note})`);
  await page.key("Enter");
  m = await j(`({ multi: [...multi] })`);
  ok(m.multi.length >= 1 && m.multi.every((id) => /sunlight/.test(id)), "Enter selects the matches");
  await page.eval(`document.getElementById("netfind").value = ""; applyNetFind(); document.getElementById("netfind").blur()`);
  // a note and a box persist in the sidecar and come back after a reload of the graph
  await page.eval(`document.getElementById("netnote").click()`);
  await page.wait(300);
  for (const ch of "hello") await page.key(ch);
  await page.eval(`document.activeElement.blur()`);
  await page.wait(500);
  await page.eval(`setMulti = null; multi = new Set(${JSON.stringify(vis.slice(0, 2))}); selected = ${JSON.stringify(vis[0])}; markSel(); document.getElementById("netbox").click()`);
  await page.wait(300);
  await page.key("l"); await page.key("i"); await page.key("g");
  await page.eval(`document.activeElement.blur()`);
  await page.wait(500);
  const rev1 = await page.eval(`call("world.graph", { world: ${W} }).then(g => g.revision)`);
  await page.eval(`loadGraph()`);
  await page.wait(800);
  const back = await j(`({ notes: Object.values(graphData.notes).map(n => n.text), frames: Object.values(graphData.frames).map(f => f.label), noteEls: document.querySelectorAll("#graph .nnote").length, frameEls: document.querySelectorAll("#graph .nframe").length })`);
  ok(back.notes.includes("hello") && back.frames.includes("lig") && back.noteEls === 1 && back.frameEls === 1, "note and box saved and drawn again after a reload: " + JSON.stringify(back));
  ok(rev1 === rev0, "no revision moved during any of it");
  // box-select: Shift-drag a rectangle from empty space over the boxes
  await page.eval(`select(null); cams.graph.fit = true; fitCam(cams.graph); applyCam(cams.graph)`);
  await page.wait(300);
  const target = await j(`(() => { const s = document.getElementById("stage-graph").getBoundingClientRect(); const pts = []; for (let dx = 6; dx < 120; dx += 6) for (let dy = 6; dy < 120; dy += 6) pts.push([s.right - dx, s.bottom - dy]); const p = pts.find(([x, y]) => { const e = document.elementFromPoint(x, y); return e && (e.id === "graph" || e.id === "stage-graph" || e.id === "wires"); }); return { start: p, rect: s.toJSON() }; })()`);
  const boxes = await j(`[...document.querySelectorAll("#graph .box")].map(b => { const r = b.getBoundingClientRect(); return { id: b.dataset.id, x0: r.left, y0: r.top, x1: r.right, y1: r.bottom }; })`);
  const bx0 = Math.min(...boxes.map(b => b.x0)) - 4, by0 = Math.min(...boxes.map(b => b.y0)) - 4;
  const [sx, sy] = target.start;
  await page.shiftMouse("mousePressed", sx, sy);
  for (let i = 1; i <= 8; i++) await page.shiftMouse("mouseMoved", sx + (Math.max(bx0, target.rect.left + 2) - sx) * i / 8, sy + (Math.max(by0, target.rect.top + 2) - sy) * i / 8);
  await page.shiftMouse("mouseReleased", Math.max(bx0, target.rect.left + 2), Math.max(by0, target.rect.top + 2), 0);
  await page.wait(300);
  m = await j(`({ n: multi.size, total: document.querySelectorAll("#graph .box").length })`);
  ok(m.n >= 3 && m.n <= m.total, `Shift-drag on empty space selects the boxes inside the rectangle (${m.n} of ${m.total})`);
  // a network box moves what is inside it
  const fr = await j(`(() => { const f = Object.values(graphData.frames)[0], id = Object.keys(graphData.frames)[0]; const h = document.querySelector('.nframe[data-id="' + id + '"] .nfh'); const r = h.getBoundingClientRect(); let pt = null; for (let x = r.left + 4; x < r.right - 20 && !pt; x += 5) { const y = r.top + 10; if (document.elementFromPoint(x, y) === h) pt = [x, y]; } return { id, x: pt && pt[0], y: pt && pt[1], fx: f.x, a: placed.get(${JSON.stringify(vis[0])}).x }; })()`);
  ok(fr.x != null, "a free spot on the box's header");
  await page.mouse("mousePressed", fr.x, fr.y);
  for (let i = 1; i <= 6; i++) await page.mouse("mouseMoved", fr.x + i * 5, fr.y + i * 3);
  await page.mouse("mouseReleased", fr.x + 30, fr.y + 18, 0);
  await page.wait(500);
  const fr2 = await j(`({ fx: Object.values(graphData.frames)[0].x, a: placed.get(${JSON.stringify(vis[0])}).x })`);
  ok(fr2.fx > fr.fx + 10 && fr2.a > fr.a + 10, "dragging a network box moves it and the nodes inside");
  const sv2 = JSON.parse(await page.eval(`call("world.graph", { world: ${W} }).then(g => JSON.stringify({ rev: g.revision, a: g.positions[${JSON.stringify(vis[0])}][0], fx: Object.values(g.frames)[0].x }))`));
  ok(sv2.rev === rev0 && Math.abs(sv2.a - fr2.a) < 1 && sv2.fx === fr2.fx, "the new positions are saved, the revision still has not moved");
  await page.shot(S + "network.png");
};
