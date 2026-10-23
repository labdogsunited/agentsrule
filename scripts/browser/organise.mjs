// Node system, the rest of H1 to H3 (proposals/houdini-node-system.md §6, §12): rules as boxes, routing dots, arrange,
// the network and camera kept across a reload, paste into another world, and a sink dropped on a wire becoming the
// transfer's overflow. Presentation changes no revision; each semantic edit is one revision.
// node scripts/browser-check.mjs URL scripts/browser/organise.mjs
// Start a viewer first: emerge-server serve --viewer --port N [--state-dir DIR] (the URL, token included, is printed).
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  const errs = `window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`;
  await page.eval(errs);
  await page.wait(800);
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  const rev = () => j(`call("world.graph", { world: ${W} }).then(g => g.revision)`);
  // Rules as boxes: one box a rule, the statements' wires on it; Enter goes into the rule.
  await page.eval(`document.getElementById("netcollapse").click()`);
  await page.wait(600);
  const rb = await j(`({ rules: document.querySelectorAll("#graph .box.k-rule").length, stmts: document.querySelectorAll("#graph .box.k-transfer").length, wires: document.querySelectorAll("#wires path.wire").length, all: graphData.nodes.filter(n => n.kind === "rule").length })`);
  ok(rb.rules === rb.all && rb.stmts === 0 && rb.wires > 0, `rules as boxes: ${rb.rules} rule boxes, no statement boxes, ${rb.wires} wires`);
  await page.shot(S + "organise-rules.png");
  const weather = await j(`[...document.querySelectorAll("#graph .box.k-rule")].map(b => b.dataset.id).find(id => /weather/.test(id))`);
  await page.eval(`select(${JSON.stringify(weather)}); mode = "graph"`);
  await page.key("Enter", { vk: 13 });
  await page.wait(600);
  ok(await page.eval(`ctx`) === "rule:weather", "Enter on a rule box goes into that rule");
  await page.eval(`goContext("world")`);
  await page.eval(`document.getElementById("netcollapse").click()`);
  await page.wait(600);
  ok(await j(`document.querySelectorAll("#graph .box.k-transfer").length`) > 0, "and back to every statement");
  // A routing dot: double-click on a wire; it is kept, can be dragged, and a double-click removes it.
  const mid = await j(`(() => { const p = document.querySelector('#wires path.wire[data-k]'), L = p.getTotalLength(), q = p.getPointAtLength(L / 2), r = stages.graph.getBoundingClientRect(), c = cams.graph; return { k: p.dataset.k, x: r.left + c.x + q.x * c.s, y: r.top + c.y + q.y * c.s }; })()`);
  await page.eval(`(() => { const el = document.elementFromPoint(${mid.x}, ${mid.y}); el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: ${mid.x}, clientY: ${mid.y} })); })()`);
  await page.wait(700);
  const kept = await j(`call("world.graph", { world: ${W} }).then(g => g.routes[${JSON.stringify(mid.k)}])`);
  ok(Array.isArray(kept) && kept.length === 1 && await j(`document.querySelectorAll("#graph .rdot").length`) === 1, `a double-click on a wire adds a routing dot, kept beside the world (${JSON.stringify(kept)})`);
  const dot = await j(`(() => { const r = document.querySelector("#graph .rdot").getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  await page.mouse("mousePressed", dot.x, dot.y);
  for (let i = 1; i <= 5; i++) await page.mouse("mouseMoved", dot.x + i * 6, dot.y + i * 10);
  await page.mouse("mouseReleased", dot.x + 30, dot.y + 50, 0);
  await page.wait(600);
  const moved = await j(`call("world.graph", { world: ${W} }).then(g => g.routes[${JSON.stringify(mid.k)}][0])`);
  ok(moved[1] > kept[0][1] + 10, `dragging the dot moves the wire's route (${kept[0]} → ${moved})`);
  const d2 = await j(`(() => { const r = document.querySelector("#graph .rdot").getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  await page.eval(`document.elementFromPoint(${d2.x}, ${d2.y}).dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`);
  await page.wait(600);
  ok(await j(`document.querySelectorAll("#graph .rdot").length`) === 0, "a double-click on the dot removes it");
  // Arrange: three boxes aligned on their left edges and spread down, saved, no revision.
  const three = await j(`[...document.querySelectorAll("#graph .box.k-transfer")].slice(0, 3).map(b => b.dataset.id)`);
  await page.eval(`multi = new Set(${JSON.stringify(three)}); selected = ${JSON.stringify(three[2])}; markSel()`);
  await page.eval(`(() => { const s = document.getElementById("netarrange"); s.value = "left"; s.dispatchEvent(new Event("change")); })()`);
  await page.eval(`(() => { const s = document.getElementById("netarrange"); s.value = "dy"; s.dispatchEvent(new Event("change")); })()`);
  await page.wait(600);
  const xs = await j(`${JSON.stringify(three)}.map(id => placed.get(id).x)`), ys = await j(`${JSON.stringify(three)}.map(id => placed.get(id).y)`);
  const savedPos = await j(`call("world.graph", { world: ${W} }).then(g => ${JSON.stringify(three)}.map(id => g.positions[id]))`);
  ok(xs.every((x) => x === xs[0]) && new Set(ys).size === 3 && savedPos.every((p, i) => p && p[0] === xs[i]), `aligned left at ${xs[0]} and spread down, positions saved`);
  ok(await rev() === 0, "rules as boxes, dots and arrange moved no revision");
  // The network and its camera come back after a reload of the page.
  await page.eval(`goContext("rule:weather")`);
  await page.eval(`cams.graph.s = 0.77; cams.graph.fit = false; applyCam(cams.graph); saveNetView()`);
  await page.eval(`location.reload()`);
  await page.wait(2500);
  await page.eval(errs);
  await page.eval(`loadDesktop("Rules")`);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  ok(await page.eval(`ctx`) === "rule:weather" && Math.abs(await page.eval(`cams.graph.s`) - 0.77) < 0.01, "after a reload the network is the rule it was in, with its camera");
  await page.eval(`goContext("world")`);
  await page.wait(500);
  // Paste into another world: the text of the copied statements goes into new rules there.
  const first = await page.eval(W);
  await page.eval(`multi = new Set(["Dish.rules.weather@transfer_nitrate"]); selected = "Dish.rules.weather@transfer_nitrate"; copySelection()`);
  await page.eval(`(async () => { await create(); })()`);
  await page.wait(2000);
  const second = await page.eval(W);
  ok(second !== first, `a second world (${second})`);
  await page.eval(`activate("network")`);
  await page.wait(1200);
  await page.eval(`pasteClip()`);
  await page.wait(1500);
  const pasted = await j(`call("world.graph", { world: ${W} }).then(g => ({ rev: g.revision, box: g.nodes.some(n => n.id === "Dish.rules.pasted_1@transfer_nitrate") }))`);
  ok(pasted.rev === 1 && pasted.box, "Ctrl+V in another world pastes the statement there: one revision");
  // A sink dropped on a transfer's destination wire becomes its overflow.
  await page.eval(`graphOp({ op: "unwire", id: "Dish.rules.weather@transfer_carbon", port: "overflow" })`);
  await page.wait(1500);
  await page.eval(`frameAll()`);
  await page.wait(400);
  const geo = await j(`(() => {
    const p = [...document.querySelectorAll('#wires path.wire')].find(x => x.dataset.from === "Dish.rules.weather@transfer_carbon"), L = p.getTotalLength(), q = p.getPointAtLength(L * 0.6);
    const r = stages.graph.getBoundingClientRect(), c = cams.graph, b = document.querySelector('#graph .box[data-id="Dish.sinks.void"]').getBoundingClientRect();
    return { wx: r.left + c.x + q.x * c.s, wy: r.top + c.y + q.y * c.s, bx: b.x + b.width / 2, by: b.y + b.height - 8 };
  })()`);
  await page.mouse("mousePressed", geo.bx, geo.by);
  for (let i = 1; i <= 12; i++) await page.mouse("mouseMoved", geo.bx + (geo.wx - geo.bx) * i / 12, geo.by + (geo.wy - geo.by) * i / 12);
  await page.wait(900);
  ok(await j(`!!document.querySelector("#wires .hotwire")`) && /overflow|void/.test(await page.eval(`document.getElementById("wirehint").textContent`)), "dragging the sink over the wire lights it and says what a drop would write");
  await page.shot(S + "organise-insert.png");
  await page.mouse("mouseReleased", geo.wx, geo.wy, 0);
  await page.wait(1500);
  const ins = await j(`call("world.graph", { world: ${W} }).then(g => ({ rev: g.revision, text: g.nodes.find(n => n.id === "Dish.rules.weather@transfer_carbon").text }))`);
  ok(ins.rev === 3 && /overflow: void/.test(ins.text), "dropping it there made it the transfer's overflow (one revision)");
  const e = await j(`__errs`);
  ok(e.length === 0, `no page errors (${e.join(" | ")})`);
};
