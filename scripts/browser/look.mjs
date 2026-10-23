// Node system H5, the Look network (proposals/houdini-node-system.md §11): the view as a typed network of its own pieces,
// previews of each node, a checked edit (a refused one keeps the last valid view), the display flag and its way back, the
// views' own undo, a layer added; the world's revision never moves.
// node scripts/browser-check.mjs URL scripts/browser/look.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed).
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  await page.eval(`window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`);
  await page.wait(1000);
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("look")`);
  await page.wait(800);
  const W = `document.getElementById("world").value`;
  await page.eval(`call("run.step", { world: ${W}, ticks: 20 })`);
  ok(await j(`call("views.get", { world: ${W} }).then(r => r.saved)`), "the new world's views are kept on the server");
  await page.eval(`(async () => { document.getElementById("view").value = "Energy"; await subscribe(); await loadLook(); })()`);
  await page.wait(1500);
  const g = await j(`({ boxes: document.querySelectorAll("#lookcanvas .lbox").length, nodes: look.graph.nodes.length, wires: document.querySelectorAll("#lookwires path").length, types: [...document.querySelectorAll("#lookcanvas .lbox .ty")].map(t => t.textContent) })`);
  ok(g.boxes === g.nodes && g.wires > 5, `the view is a network: ${g.boxes} nodes, ${g.wires} wires`);
  ok(g.types.includes("scaled") && g.types.includes("colour") && g.types.includes("cells") && g.types.includes("agents"), `each socket says what flows: ${[...new Set(g.types)].join(", ")}`);
  await page.wait(3000);
  const pv = await j(`[...document.querySelectorAll("#lookcanvas img.pv")].filter(i => i.src.startsWith("data:image/png")).length`);
  ok(pv >= 6, `each node has a preview drawn from the frame (${pv})`);
  await page.shot(S + "look-network.png");
  const rev0 = await j(`look.rev`);
  // A checked edit: the scale's range.
  await page.eval(`(() => { look.sel = "layer:ground/color/1"; renderLookParm(); const ins = document.querySelectorAll("#lookparm input"); ins[1].value = "300"; ins[1].dispatchEvent(new Event("change")); })()`);
  await page.wait(1500);
  ok(await j(`look.rev`) === rev0 + 1 && /range: 0\.0\.\.300\.0/.test(await j(`look.graph.text`)), "changing the scale's range is one change of the views");
  // A palette from its list.
  await page.eval(`(() => { look.sel = "layer:ground/color/2"; renderLookParm(); const sel = document.querySelector("#lookparm select"); sel.value = "viridis"; sel.dispatchEvent(new Event("change")); })()`);
  await page.wait(1500);
  ok(/palette\(viridis\)/.test(await j(`look.graph.text`)), "the palette is set from its list");
  // A refused edit keeps the last valid view and says why.
  const before = await j(`look.graph.text`);
  await page.eval(`(() => { look.sel = "layer:ground/color/0"; renderLookParm(); const i = document.querySelector("#lookparm input"); i.value = "nosuch"; i.dispatchEvent(new Event("change")); })()`);
  await page.wait(1500);
  ok(await j(`look.graph.text`) === before && /last valid/.test(await page.eval(`document.querySelector("#lookparm .msg.bad") ? document.querySelector("#lookparm .msg.bad").textContent : ""`)), "a refused edit keeps the last valid view and says why");
  // The display flag shows one node in the viewport; the bar takes it back.
  await page.eval(`setDisplay("layer:ground/color/1")`);
  await page.wait(1800);
  ok(await page.eval(`getComputedStyle(document.getElementById("displaybar")).display`) === "flex" && /__display$/.test(await j(`lookDisplay.name`)), "the display flag makes the viewport show that node, and says so");
  await page.shot(S + "look-display.png");
  await page.eval(`[...document.querySelectorAll("#displaybar button")].find(b => b.textContent === "Whole view").click()`);
  await page.wait(1500);
  ok(await page.eval(`getComputedStyle(document.getElementById("displaybar")).display`) === "none" && await j(`lookDisplay`) === null, "Whole view takes it back");
  // Undo of the views.
  await page.eval(`lookStep("views.undo")`);
  await page.wait(1500);
  ok(/palette\(cividis\)/.test(await j(`look.graph.text`)), "Undo takes the palette back");
  // A layer added from the composite.
  await page.eval(`(() => { look.sel = "composite"; renderLookParm(); const i = document.querySelector("#lookparm input"); i.value = "nitrate"; [...document.querySelectorAll("#lookparm button")].find(b => b.textContent === "Add layer on top").click(); })()`);
  await page.wait(1800);
  ok(await j(`look.graph.nodes.some(n => n.id === "layer:nitrate")`), "a layer is added on top");
  ok(await j(`call("stats.query", { world: ${W} }).then(r => r.revision)`) === 0, "nothing in the Look changed the world's revision");
  await page.shot(S + "look-end.png");
  const e = await j(`__errs`);
  ok(e.length === 0, `no page errors (${e.join(" | ")})`);
};
