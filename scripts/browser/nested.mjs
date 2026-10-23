// Node system H4, the rest (proposals/houdini-node-system.md §5, §8): statements inside a `when` have their ports and wires,
// a condition's reads are value-dependency wires, the condition is set from Parameters, a statement is added inside the
// `when`, and the code box completes names, checks while typing and jumps to a declaration.
// node scripts/browser-check.mjs URL scripts/browser/nested.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed).
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  await page.eval(`window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`);
  await page.wait(1000);
  // The dish with a `when` around two transfers (and an `else`), created from the page.
  await page.eval(`(async () => {
    const rules = "    rule wind on cells {\\n        when self.carbon >= 5u and below.nitrate < 3u {\\n            transfer(carbon, from: self, to: right, amount: 3mu) @wind_carbon;\\n        } else {\\n            transfer(nitrate, from: self, to: left, amount: 2mu) @wind_else;\\n        }\\n    }\\n\\n";
    const src = example.source.replace("    rule spread on cells", rules + "    rule spread on cells").replace("Grid(w: 256, h: 256)", "Grid(w: 64, h: 64)").replace(/count: 2_000;/g, "count: 200;");
    const { world } = await call("world.create", { source: src, lineages: ["Builder"], seed: 1, mutation: false });
    await refreshWorlds(world); await refreshViews(); await subscribe();
  })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  await page.eval(`goContext("rule:wind")`);
  await page.wait(800);
  const n = await j(`({ nested: [...document.querySelectorAll("#graph .box.nested")].map(b => ({ id: b.dataset.id, socks: b.querySelectorAll(".port.sock").length })), wires: [...document.querySelectorAll("#wires path.wire")].map(p => ({ to: p.dataset.to, from: p.dataset.from, dep: p.classList.contains("dep") })) })`);
  ok(n.nested.length === 2 && n.nested.every((x) => x.socks === 2), `the statements inside the when have their sockets (${JSON.stringify(n.nested)})`);
  ok(n.wires.some((w) => w.to === "Dish.rules.wind#0/t0") && n.wires.some((w) => w.from === "Dish.rules.wind#0/e0"), "and their wires to the cells they move between");
  ok(n.wires.some((w) => w.dep && w.to === "Dish.rules.wind#0"), "the condition's reads are drawn as value-dependency wires into the when");
  await page.shot(S + "nested-wires.png");
  // The condition, from Parameters: one revision.
  await page.eval(`select("Dish.rules.wind#0")`);
  await page.wait(400);
  ok(await page.eval(`document.querySelector('#picked input[data-role="condition"]').value`) === "self.carbon >= 5u and below.nitrate < 3u", "Parameters shows the when's condition");
  ok(/In rule wind on cells/.test(await page.eval(`document.querySelector("#picked .ctxline").textContent`)), "and where it runs and what it reads");
  await page.eval(`(() => { const i = document.querySelector('#picked input[data-role="condition"]'); i.value = "self.carbon >= 6u"; i.dispatchEvent(new Event("change")); })()`);
  await page.wait(1500);
  ok(await j(`call("world.graph", { world: ${W} }).then(g => ({ rev: g.revision, c: g.nodes.find(x => x.id === "Dish.rules.wind#0").contract.condition }))`).then((r) => r.rev === 1 && r.c === "self.carbon >= 6u"), "changing it sets the condition in one revision");
  // A statement added inside the when.
  await page.eval(`(() => { const t = document.querySelector('#picked textarea[data-role="addinside"]'); t.value = "transfer(body, from: self, to: right, amount: 1mu) @wind_body;"; [...document.querySelectorAll("#picked button")].find(b => b.textContent === "Add inside").click(); })()`);
  await page.wait(1600);
  ok(await j(`document.querySelectorAll("#graph .box.nested").length`) === 3 && await j(`call("world.graph", { world: ${W} }).then(g => g.revision)`) === 2, "Add inside puts a third statement in the when (one revision)");
  // Completion in the code box.
  await page.eval(`select("Dish.rules.wind#0/t0")`);
  await page.wait(500);
  await page.eval(`(() => { const ta = document.querySelector("#picked textarea:not([data-role])"); ta.focus(); ta.value = "transfer(car"; ta.selectionStart = ta.selectionEnd = ta.value.length; ta.dispatchEvent(new Event("input")); })()`);
  await page.wait(600);
  const list = await j(`[...document.querySelectorAll("#complist div")].map(d => d.firstChild.textContent)`);
  ok(list[0] === "carbon", `typing offers the names in scope, substances first (${list.slice(0, 4).join(", ")})`);
  await page.key("Enter", { vk: 13 });
  await page.wait(200);
  ok(await page.eval(`document.querySelector("#picked textarea:not([data-role])").value`) === "transfer(carbon", "Enter completes the name");
  // The inline check: wrong, then right.
  await page.eval(`(() => { const ta = document.querySelector("#picked textarea:not([data-role])"); ta.value = "transfer(carbn, from: self, to: right, amount: 3mu) @wind_carbon;"; ta.dispatchEvent(new Event("input")); })()`);
  await page.wait(1400);
  const bad = await page.eval(`document.querySelector("#picked .lint").textContent`);
  ok(/✗/.test(bad) && /carbn/.test(bad), `a draft the world would refuse says why while typing (${bad.slice(0, 90)})`);
  await page.eval(`(() => { const ta = document.querySelector("#picked textarea:not([data-role])"); ta.value = "transfer(carbon, from: self, to: right, amount: 4mu) @wind_carbon;"; ta.dispatchEvent(new Event("input")); })()`);
  await page.wait(1400);
  ok(/✓/.test(await page.eval(`document.querySelector("#picked .lint").textContent`)), "a good one says it would be accepted");
  ok(await j(`call("world.graph", { world: ${W} }).then(g => g.revision)`) === 2, "checking applied nothing");
  // Jump to a declaration.
  await page.eval(`(() => { const ta = document.querySelector("#picked textarea:not([data-role])"); ta.focus(); const i = ta.value.indexOf("carbon") + 2; ta.selectionStart = ta.selectionEnd = i; })()`);
  await page.key("F12", { vk: 123 });
  await page.wait(800);
  ok(await page.eval(`ctx`) === "world" && await page.eval(`selected`) === "Dish.substances.carbon", "F12 on a name goes to its declaration");
  await page.shot(S + "nested-end.png");
  const e = await j(`__errs`);
  ok(e.length === 0, `no page errors (${e.join(" | ")})`);
};
