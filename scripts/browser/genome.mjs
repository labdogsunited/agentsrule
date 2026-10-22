// Genome G1: signal channels, ordinals, flags, search, dragging a card. node scripts/browser-check.mjs URL scripts/browser/genome.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const root = resolve(import.meta.dirname, "../..");
  const src = readFileSync(root + "/examples/dish.em", "utf8") + "\n" + readFileSync(root + "/crates/emerge-server/tests/fixtures/looper-lineage.em", "utf8");
  await page.eval(`(async () => { const r = await call("world.create", { source: ${JSON.stringify(src)}, lineages: ["Looper"], seed: 1, mutation: false }); await refreshWorlds(r.world); await refreshViews(); await subscribe(); await stats(); })()`);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 3 })`);
  await page.eval(`loadDesktop("Genome")`);
  await page.wait(500);
  await page.eval(`activate("genome")`);
  await page.wait(1500);
  const info = await page.eval(`JSON.stringify({ cards: document.querySelectorAll("#genome .gene").length, chans: document.querySelectorAll("#genome .chan").length, wires: document.querySelectorAll("#gwires .wire").length, selfw: document.querySelectorAll("#gwires .wire.self").length, lonely: document.querySelectorAll("#genome .chan.lonely").length, first: document.querySelector("#genome .gene .t").textContent, flags: document.querySelector("#genome .gene .flags").textContent })`);
  console.log(info);
  const d = JSON.parse(info);
  const model = JSON.parse(await page.eval(`call("genome.graph", { world: document.getElementById("world").value, entity: +document.getElementById("entity").value }).then(g => { let w = 0, chans = new Set(); for (const x of g.genes) { w += x.reads.length + x.writes.length; x.reads.forEach(k => chans.add(k)); x.writes.forEach(k => chans.add(k)); } return JSON.stringify({ genes: g.genes.length, wires: w, chans: chans.size }); })`));
  ok(d.cards === model.genes && d.chans === model.chans && d.wires === model.wires, `every gene, channel and writer/reader link is drawn: ${JSON.stringify(model)}`);
  ok(d.selfw === 2, "the self-feedback gene has its two wires marked as feedback");
  ok(d.lonely >= 1, "a signal nothing reads is drawn and marked");
  ok(/^0 · /.test(d.first) && /gene on · module on · active/.test(d.flags), "ordinal and three separate state flags on the card");
  // search fades, hides nothing; typing a space does not toggle the run
  const ticks0 = await page.eval(`call("stats.query", { world: document.getElementById("world").value }).then(s => s.tick)`);
  await page.eval(`document.getElementById("gsearch").focus()`);
  for (const ch of "turn") await page.key(ch);
  await page.key(" ");
  await page.wait(800);
  const s = JSON.parse(await page.eval(`JSON.stringify({ n: document.querySelectorAll("#genome .gene").length, faded: document.querySelectorAll("#genome .gene.nomatch").length, found: document.getElementById("gfound").textContent, v: document.getElementById("gsearch").value, running: Object.values(runningNow).some(Boolean) })`));
  ok(s.n === d.cards && s.faded === d.cards - 1 && /1 of/.test(s.found), `search fades the others and hides none (${s.found})`);
  ok(!s.running && s.v === "turn ", "keys typed in the search box stay there; Space did not start the run");
  await page.eval(`document.getElementById("gsearch").value = ""; document.getElementById("gsearch").dispatchEvent(new Event("input"))`);
  // drag a card: it moves, the world does not change, and the position comes back after a reload
  const rect = JSON.parse(await page.eval(`(() => { const r = document.querySelector("#genome .gene[data-index='1']").getBoundingClientRect(); return JSON.stringify({ x: r.x + r.width / 2, y: r.y + 12 }); })()`));
  const before = await page.eval(`document.querySelector("#genome .gene[data-index='1']").style.left`);
  const journal0 = await page.eval(`call("journal.read", { world: document.getElementById("world").value }).then(j => j.lines.length)`);
  await page.mouse("mousePressed", rect.x, rect.y);
  for (let i = 1; i <= 6; i++) await page.mouse("mouseMoved", rect.x + i * 10, rect.y + i * 5);
  await page.mouse("mouseReleased", rect.x + 60, rect.y + 30, 0);
  await page.wait(300);
  const after = await page.eval(`document.querySelector("#genome .gene[data-index='1']").style.left`);
  ok(before !== after, `the card moved (${before} -> ${after})`);
  const journal1 = await page.eval(`call("journal.read", { world: document.getElementById("world").value }).then(j => j.lines.length)`);
  const ticks1 = await page.eval(`call("stats.query", { world: document.getElementById("world").value }).then(s => s.tick)`);
  ok(journal0 === journal1 && ticks0 === ticks1, "moving a card changed no revision, journal line or tick");
  await page.eval(`loadGenome()`);
  await page.wait(500);
  ok(after === await page.eval(`document.querySelector("#genome .gene[data-index='1']").style.left`), "the position is kept when the genome is drawn again");
  await page.shot(S + "genome.png");
};
