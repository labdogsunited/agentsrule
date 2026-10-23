// Network H3 viewer: duplicate, copy and paste, naming a statement, one edit for several statements, Tab while connecting.
// node scripts/browser-check.mjs URL scripts/browser/commands.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`JSON.stringify(${js})`));
  const CTRL = 2;
  await page.wait(1500);
  // the nursery dish with one unlabelled statement (the rotting reaction), to be named
  await page.eval(`(async () => {
    document.getElementById("example").value = "nursery"; document.getElementById("example").dispatchEvent(new Event("change"));
    const x = chosenExample(); const source = x.source.replace(" @react_rot;", ";");
    if (source === x.source) throw new Error("the label to remove is not in the example");
    const r = await call("world.create", { source, lineages: ["Builder"], seed: 1, backend: "cpu", mutation: false });
    await refreshWorlds(r.world); await refreshViews();
  })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  await page.eval(`loadGraph()`);
  await page.eval(`mode = "graph"`);
  const state = () => j(`({ rev: graphData.revision, n: graphData.nodes.filter(n => ["transfer", "react", "diffuse", "code"].includes(n.kind)).length, undo: graphData.history.undo.length, sel: selected, msg: document.getElementById("error").textContent })`);
  const s0 = await state();
  // duplicate: Ctrl+D on a statement is one revision and one undo
  const sun = await j(`graphData.nodes.find(n => /rules\\.sunlight@/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(sun)})`);
  await page.key("d", { modifiers: CTRL });
  await page.wait(600);
  let s = await state();
  ok(s.rev === s0.rev + 1 && s.n === s0.n + 1 && s.undo === s0.undo + 1 && s.sel && s.sel !== sun, `Ctrl+D duplicates the statement: one revision, one undo, the copy is selected (${s.sel}) ${JSON.stringify(s0)} ${JSON.stringify(s)}`);
  await page.eval(`stepHistory("world.undo")`);
  await page.wait(500);
  s = await state();
  ok(s.n === s0.n, "one undo takes the copy away");
  // copy and paste, twice
  const carbon = await j(`graphData.nodes.find(n => /rules\\.weather@transfer_carbon$/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(carbon)})`);
  await page.key("c", { modifiers: CTRL });
  ok((await page.eval(`document.getElementById("netfound").textContent`)) === "1 copied", "Ctrl+C says what it copied");
  await page.key("v", { modifiers: CTRL });
  await page.wait(500);
  await page.key("v", { modifiers: CTRL });
  await page.wait(500);
  s = await state();
  ok(s.n === s0.n + 2, `Ctrl+V pastes a copy each time (${s.n - s0.n} copies)`);
  await page.eval(`stepHistory("world.undo")`); await page.wait(400);
  await page.eval(`stepHistory("world.undo")`); await page.wait(400);
  // naming an unlabelled statement
  const rot = await j(`graphData.nodes.find(n => /rules\\.rotting#0$/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(rot)})`);
  const namer = await j(`({ has: !!document.querySelector('#picked input[data-role="label"]') })`);
  ok(namer.has, "an unlabelled statement offers a place to name it");
  const rev = (await state()).rev;
  await page.eval(`(() => { const i = document.querySelector('#picked input[data-role="label"]'); i.value = "slow_rot"; i.parentNode.querySelector("button").click(); })()`);
  await page.wait(600);
  s = await state();
  const named = await j(`graphData.nodes.some(n => /rules\\.rotting@slow_rot$/.test(n.id))`);
  ok(named && s.rev === rev + 1, "naming it is one revision and the box is now `rotting@slow_rot`");
  ok(!(await j(`!!document.querySelector('#picked input[data-role="label"]')`)), "and it no longer asks for a name");
  // several statements, one edit
  const nit = await j(`graphData.nodes.find(n => /rules\\.weather@transfer_nitrate$/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(carbon)}); multi = new Set([${JSON.stringify(carbon)}, ${JSON.stringify(nit)}]); markSel(); renderParm()`);
  const b = await j(`({ head: (document.querySelector("#picked .batch h4") || {}).textContent, names: [...document.querySelectorAll("#picked .batch input")].map(i => i.dataset.param) })`);
  ok(/2 selected/.test(b.head) && b.names.includes("amount"), `two selected statements show the parameters they share (${b.names.join(", ")})`);
  const before = await state();
  await page.eval(`(() => { const i = document.querySelector('#picked .batch input[data-param="amount"]'); i.value = "7mu"; i.dispatchEvent(new Event("change")); })()`);
  await page.wait(700);
  s = await state();
  const amounts = await j(`[${JSON.stringify(carbon)}, ${JSON.stringify(nit)}].map(id => graphData.nodes.find(n => n.id === id).params.find(p => p[0] === "amount")[1])`);
  ok(s.rev === before.rev + 1 && s.undo === before.undo + 1 && amounts.every((a) => a === "7mu"), `one edit set both amounts to 7mu: one revision, one undo (${amounts.join(", ")})`);
  await page.shot(S + "commands-batch.png");
  // a value one of them refuses leaves both as they were
  const r1 = (await state()).rev;
  await page.eval(`select(${JSON.stringify(carbon)}); multi = new Set([${JSON.stringify(carbon)}, ${JSON.stringify(nit)}]); markSel(); renderParm()`);
  await page.eval(`(() => { const i = document.querySelector('#picked .batch input[data-param="amount"]'); i.value = "banana"; i.dispatchEvent(new Event("change")); })()`);
  await page.wait(700);
  s = await state();
  const kept = await j(`[${JSON.stringify(carbon)}, ${JSON.stringify(nit)}].map(id => graphData.nodes.find(n => n.id === id).params.find(p => p[0] === "amount")[1])`);
  ok(s.rev === r1 && kept.every((a) => a === "7mu") && s.msg.length > 3, `a refused value changes neither statement and says why ("${s.msg.slice(0, 70)}")`);
  // Tab while pulling a wire: a new transfer from the source, already connected to the field
  await page.eval(`select(null); fitCam(cams.graph); applyCam(cams.graph)`);
  const spark = await j(`graphData.nodes.find(n => /sources\\.spark$/.test(n.id)).id`);
  const rock = await j(`graphData.nodes.find(n => /sources\\.rock$/.test(n.id)).id`);
  const cells = await j(`graphData.nodes.find(n => /fields\\.cells$/.test(n.id)).id`);
  const centre = (id, sel = "") => j(`(() => { const r = document.querySelector('#graph .box[data-id="${id}"] ${sel}').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  const cp = await centre(cells);
  const pull = async (src) => {
    const sp = await centre(src, ".port.out");
    await page.mouse("mousePressed", sp.x, sp.y);
    for (let i = 1; i <= 6; i++) await page.mouse("mouseMoved", sp.x + (cp.x - sp.x) * i / 6, sp.y + (cp.y - sp.y) * i / 6);
    await page.key("Tab", { vk: 9 });
    await page.wait(300);
  };
  // a source whose substance the field cannot hold is refused before anything is made, with the reason
  await pull(spark);
  const refused = await j(`({ shown: document.getElementById("tabmenu").style.display, err: document.getElementById("error").textContent })`);
  await page.mouse("mouseReleased", cp.x, cp.y, 0);
  ok(refused.shown !== "block" && /holds none of them/.test(refused.err), `Tab from a source the field cannot take says so: "${refused.err.slice(0, 80)}"`);
  const before2 = await state();
  await pull(rock);
  const menu = await j(`({ shown: document.getElementById("tabmenu").style.display, rows: [...document.querySelectorAll("#tablist div")].map(d => d.textContent) })`);
  ok(menu.shown === "block" && menu.rows.some((r) => /transfer carbon/.test(r)) && !menu.rows.some((r) => /energy/.test(r)), `Tab during the pull offers what the source can make: ${menu.rows.join(" | ")}`);
  await page.mouse("mouseReleased", cp.x, cp.y, 0);
  console.log("before Enter", await page.eval(`JSON.stringify({ active: document.activeElement && (document.activeElement.id || document.activeElement.tagName), menu: document.getElementById("tabmenu").style.display, mode: !!tabMode, err: document.getElementById("error").textContent })`));
  await page.key("Enter", { vk: 13 });
  await page.wait(700);
  console.log("after Enter", await page.eval(`JSON.stringify({ menu: document.getElementById("tabmenu").style.display, err: document.getElementById("error").textContent, rev: graphData.revision })`));
  s = await state();
  const made = await j(`(() => { const n = graphData.nodes.find(x => x.id === selected); const w = graphData.wires.filter(w => w.to === selected || w.from === selected); return { id: selected, text: n && n.text, wires: w.map(x => x.from.split(".").pop() + "->" + x.to.split(".").pop()) }; })()`);
  console.log(JSON.stringify(made));
  ok(s.rev === before2.rev + 1 && s.n === before2.n + 1 && /transfer\(carbon, from: rock/.test(made.text) && made.wires.length === 2, "Enter creates the transfer, wired from the source to the field, in one revision");
  await page.shot(S + "commands-tab.png");
};
