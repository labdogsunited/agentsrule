// Network H5: the Inspector keeps declared, observed and measured apart, each with its own stamp, and can hold a reading.
// node scripts/browser-check.mjs URL scripts/browser/inspector.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`JSON.stringify(${js})`));
  await page.wait(1500);
  await page.eval(`(async () => { document.getElementById("example").value = "nursery"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 30 })`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1200);
  await page.eval(`activate("inspect")`);
  await page.wait(500);
  const W = `document.getElementById("world").value`;
  const text = () => page.eval(`document.getElementById("inspbody").innerText`);
  ok(/Select a box/.test(await text()), "with nothing selected it says what to do");
  const sun = await j(`graphData.nodes.find(n => /rules\\.sunlight@/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(sun)})`);
  await page.wait(1200);
  let secs = await j(`({ d: !!document.querySelector("#inspbody .kind-declared"), o: !!document.querySelector("#inspbody .kind-observed"), m: !!document.querySelector("#inspbody .kind-measured"), heads: [...document.querySelectorAll("#inspbody h3")].map(h => h.textContent) })`);
  console.log(JSON.stringify(secs.heads));
  ok(secs.d && secs.o && secs.m, "a statement shows Declared, Observed and Measured, apart");
  let t = await text();
  ok(/transfer\(light, from: sun/.test(t) && /runs in\s+Transfers/.test(t) && /writes/.test(t), "declared: its source, its step, what it reads and writes");
  ok(/counted since tick 0, read at tick 30/.test(secs.heads.join("|")) && /delivered \(units\)/.test(t) && /by the cpu/.test(t), "measured: the runner's counts since it began, stamped with the tick they were read at");
  const delivered = await j(`(() => { const r = [...document.querySelectorAll("#inspbody .kind-measured tr")].find(x => /delivered/.test(x.textContent)); return r ? r.cells[1].textContent : null; })()`);
  ok(delivered && delivered !== "0" && delivered !== "–", `the number is a count, not zero (${delivered} units delivered)`);
  await page.shot(S + "inspector-statement.png");
  // a field: observed amounts, all of one tick
  const cells = await j(`graphData.nodes.find(n => /fields\\.cells$/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(cells)})`);
  await page.wait(1200);
  secs = await j(`({ heads: [...document.querySelectorAll("#inspbody h3")].map(h => h.textContent), rows: [...document.querySelectorAll("#inspbody .kind-observed tr")].map(r => r.textContent) })`);
  ok(/tick 30, revision 0/.test(secs.heads.join("|")) && secs.rows.some((r) => /carbon/.test(r)), `observed: the field's substances, read at one tick (${secs.rows.length} rows)`);
  // a reaction: measured extent by creatures and by statements
  const rx = await j(`graphData.nodes.find(n => n.kind === "reaction" && /photosynthesis/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(rx)})`);
  await page.wait(1200);
  t = await text();
  ok(/creatures' reaction genes/.test(t), "a reaction shows the extent its creatures ran");
  // hold: the numbers stay while the world moves on, and the note says how far
  await page.eval(`select(${JSON.stringify(sun)})`);
  await page.wait(1000);
  const before = await text();
  await page.eval(`document.getElementById("insphold").click()`);
  await page.eval(`call("run.step", { world: ${W}, ticks: 10 }).then(() => stats())`);
  await page.wait(1500);
  const held = await j(`({ note: document.getElementById("inspnote").textContent, btn: document.getElementById("insphold").textContent, body: document.getElementById("inspbody").innerText })`);
  ok(/HELD: read at tick 30/.test(held.note) && /10 later/.test(held.note) && held.btn === "Release" && held.body === before, `a held reading keeps its numbers and says how old it is (${held.note.slice(0, 90)})`);
  await page.eval(`document.getElementById("insphold").click()`);
  await page.wait(1200);
  const live = await j(`({ note: document.getElementById("inspnote").textContent })`);
  ok(/read at tick 40/.test(live.note), "released, it reads the world as it is now");
  // reading changes nothing
  ok((await page.eval(`graphData.revision`)) === 0, "no revision moved");
  // a GPU world cannot read cell amounts: it says so instead of showing zeros
  await page.eval(`(async () => { const x = chosenExample(); const r = await call("world.create", { source: x.source.replace("Grid(w: 256, h: 256)", "Grid(w: 64, h: 64)").replace(/count: 2_000;/g, "count: 300;"), lineages: ["Builder"], seed: 1, backend: "gpu", mutation: false }); await refreshWorlds(r.world); await loadGraph(); await call("run.step", { world: r.world, ticks: 5 }); })()`);
  await page.wait(500);
  const cells2 = await j(`graphData.nodes.find(n => /fields\\.cells$/.test(n.id)).id`);
  await page.eval(`select(${JSON.stringify(cells2)})`);
  await page.wait(1500);
  t = await text();
  ok(/Not available/.test(t), "on a GPU world the field's amounts are said to be unavailable, not drawn as zero");
};
