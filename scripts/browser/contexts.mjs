// Network H2 viewer, part 2: contexts (breadcrumbs, Enter and U, one camera each), ghosts, statements inside a `when`.
// node scripts/browser-check.mjs URL scripts/browser/contexts.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`JSON.stringify(${js})`));
  await page.wait(1500);
  // the nursery dish with one more rule that has a `when` and an `else` around its statements
  await page.eval(`(async () => {
    document.getElementById("example").value = "nursery"; document.getElementById("example").dispatchEvent(new Event("change"));
    const x = chosenExample(); const at = x.source.indexOf("    rule spread on cells");
    const source = x.source.slice(0, at) + "    rule wind on cells {\\n        when self.carbon >= 5u {\\n            transfer(carbon, from: self, to: right, amount: 3mu) @wind_carbon;\\n        } else {\\n            transfer(nitrate, from: self, to: left, amount: 2mu) @wind_nitrate;\\n        }\\n        transfer(body, from: rock, to: self, amount: 1mu, overflow: void) @wind_body;\\n    }\\n\\n" + x.source.slice(at);
    const r = await call("world.create", { source, lineages: ["Builder"], seed: 1, backend: "cpu", mutation: false });
    await refreshWorlds(r.world); await refreshViews();
  })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  // the world network: one crumb, a menu of all networks
  let c = await j(`({ ctx, crumbs: document.getElementById("netcrumbs").innerText, opts: [...document.getElementById("netctx").options].map(o => o.value) })`);
  ok(c.ctx === "world" && /Dish/.test(c.crumbs) && c.opts.includes("rule:sunlight") && c.opts.includes("rule:wind"), `the world network lists its rules: ${c.opts.join(", ")}`);
  const worldBoxes = await j(`document.querySelectorAll("#graph .box").length`);
  // select the sunlight statement and press Enter: its rule's network
  const sun = await j(`graphData.nodes.find(n => /rules\\.sunlight@/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(sun)})`);
  await page.eval(`mode = "graph"`);
  await page.key("Enter", { vk: 13 });
  await page.wait(300);
  c = await j(`({ ctx, crumbs: document.getElementById("netcrumbs").innerText, boxes: document.querySelectorAll("#graph .box").length, ghosts: document.querySelectorAll("#graph .box.ghost").length, wires: document.querySelectorAll("#wires path.wire").length })`);
  ok(c.ctx === "rule:sunlight" && /sunlight/.test(c.crumbs), `Enter on a statement goes into its rule (${c.crumbs.replace(/\n/g, " ")})`);
  ok(c.boxes === 3 && c.ghosts === 2 && c.wires >= 2, `it shows the statement and ghosts of the source and store it touches, with their wires (${c.boxes} boxes, ${c.ghosts} ghosts, ${c.wires} wires)`);
  await page.shot(S + "context-rule.png");
  // a camera of its own: zoom here, go up, come back
  await page.eval(`cams.graph.fit = false; cams.graph.s = 1.7; cams.graph.x = 33; cams.graph.y = 44; applyCam(cams.graph)`);
  await page.key("u");
  c = await j(`({ ctx, boxes: document.querySelectorAll("#graph .box").length, sel: selected, fit: cams.graph.fit })`);
  ok(c.ctx === "world" && c.boxes === worldBoxes && c.sel === sun && c.fit === true, "U goes back up to the whole network, with the same selection and its own (fitted) camera");
  await page.eval(`goContext("rule:sunlight")`);
  c = await j(`({ s: cams.graph.s, x: cams.graph.x, y: cams.graph.y, fit: cams.graph.fit })`);
  ok(Math.abs(c.s - 1.7) < 1e-9 && c.x === 33 && c.y === 44 && !c.fit, "coming back restores the camera that network had");
  // positions are kept for each network: drag the statement here; the world network's position does not move
  const worldPos = await j(`(() => { const g = graphData; return g.positions[${JSON.stringify(sun)}] || null; })()`);
  const before = await j(`({ x: placed.get(${JSON.stringify(sun)}).x, y: placed.get(${JSON.stringify(sun)}).y })`);
  await page.eval(`fitCam(cams.graph); applyCam(cams.graph)`);
  const r = await j(`(() => { const b = document.querySelector('#graph .box[data-id=${JSON.stringify(sun)}] .t').getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
  await page.mouse("mousePressed", r.x, r.y);
  for (let i = 1; i <= 6; i++) await page.mouse("mouseMoved", r.x + i * 7, r.y + i * 5);
  await page.mouse("mouseReleased", r.x + 42, r.y + 30, 0);
  await page.wait(500);
  const saved = JSON.parse(await page.eval(`call("world.graph", { world: ${W} }).then(g => JSON.stringify({ rev: g.revision, here: g.positions["rule:sunlight|" + ${JSON.stringify(sun)}] || null, world: g.positions[${JSON.stringify(sun)}] || null }))`));
  ok(saved.here && saved.rev === 0 && JSON.stringify(saved.world) === JSON.stringify(worldPos), `the position is saved for this network only (${JSON.stringify(saved.here)}), no revision moved`);
  // double-click a ghost goes to the world network and selects it
  const ghost = await j(`document.querySelector("#graph .box.ghost").dataset.id`);
  const gp = await j(`(() => { const b = document.querySelector("#graph .box.ghost .t").getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
  await page.eval(`enterBox(${JSON.stringify(ghost)})`);
  c = await j(`({ ctx, sel: selected })`);
  ok(c.ctx === "world" && c.sel === ghost, `a ghost leads to the real thing in the world network (${ghost})`);
  // the rule with a `when`: its statements sit in dashed frames that say when they run, and say they are read-only
  await page.eval(`document.getElementById("netctx").value = "rule:wind"; document.getElementById("netctx").onchange()`);
  await page.wait(300);
  const n = await j(`({ frames: [...document.querySelectorAll("#graph .nwhen span")].map(s => s.textContent), nested: [...document.querySelectorAll("#graph .box.nested")].map(b => b.dataset.id), members: [...document.querySelectorAll("#graph .box:not(.nested):not(.ghost)")].map(b => b.dataset.id) })`);
  console.log(JSON.stringify(n));
  ok(n.frames.length === 2 && n.frames.some((f) => /^when self\.carbon >= 5u/.test(f)) && n.frames.some((f) => /^when not/.test(f)) && n.nested.length === 2, "the statements inside the `when` and its `else` are in two frames that say when they run");
  ok(n.members.length >= 1, "and the statement outside it is an ordinary box");
  await page.eval(`select(${JSON.stringify(n.nested[0])})`);
  const p = await j(`({ text: document.getElementById("picked").innerText, inputs: [...document.querySelectorAll("#picked input")].map(i => i.disabled), areas: document.querySelectorAll("#picked textarea").length })`);
  ok(/Runs only where: self\.carbon >= 5u/.test(p.text) && p.areas === 1 && p.inputs.length > 0 && p.inputs.every((d) => !d), "selecting one shows when it runs and offers the same editing as any statement");
  // setting its amount is one revision; the box inside the frame shows it; one undo takes it back
  await page.eval(`(() => { const i = [...document.querySelectorAll("#picked .params input")].find(x => x.previousSibling && x.previousSibling.textContent === "amount"); i.value = "9mu"; i.dispatchEvent(new Event("change")); })()`);
  await page.wait(800);
  const e = await j(`({ rev: graphData.revision, text: graphData.nested.find(x => x.id === ${JSON.stringify(n.nested[0])}).text, boxes: document.querySelectorAll("#graph .box.nested").length, ctx })`);
  ok(e.rev === 1 && /amount: 9mu/.test(e.text) && e.boxes === 2 && e.ctx === "rule:wind", "one edit inside the `when`: one revision, the network stays where it was");
  await page.eval(`stepHistory("world.undo")`);
  await page.wait(600);
  ok((await page.eval(`graphData.revision`)) === 2 && /amount: 3mu/.test(await page.eval(`graphData.nested.find(x => x.id === ${JSON.stringify(n.nested[0])}).text`)), "and one undo puts it back");
  await page.shot(S + "context-when.png");

};
