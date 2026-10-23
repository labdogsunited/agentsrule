// Node system H6, reuse (proposals/reusable-operators.md): an instance is one box with its bindings as sockets, Enter shows
// the rules it writes out (read-only), its parameter is set from Parameters (one revision), Edit as a new version moves only
// it; statements kept as a template and inserted from Tab with other bindings; rules made into an operator from the toolbar.
// node scripts/browser-check.mjs URL scripts/browser/reuse.mjs
// Start a viewer first: emerge-server serve --viewer --port N [--state-dir DIR] (the URL, token included, is printed).
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  await page.eval(`window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`);
  await page.wait(1000);
  await page.eval(`(async () => { document.getElementById("example").value = "operators"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  const rev = () => j(`call("world.graph", { world: ${W} }).then(g => g.revision)`);
  // The instance: one box, its bindings as sockets.
  const box = await j(`(() => { const b = document.querySelector('#graph .box[data-id="Soil.uses.carbon_in"]'); return b && { socks: [...b.querySelectorAll(".port.sock")].map(s => s.dataset.port) }; })()`);
  ok(box && box.socks.join(",") === "from,into,spill", `an instance is one box with its bindings as sockets (${box && box.socks})`);
  ok(await j(`[...document.querySelectorAll("#wires path.wire")].some(p => p.dataset.from === "Soil.sources.rock" && p.dataset.to === "Soil.uses.carbon_in")`), "the source it is bound to is wired into it");
  await page.shot(S + "reuse-world.png");
  // Enter: the rules it writes out, read-only.
  await page.eval(`select("Soil.uses.carbon_in"); mode = "graph"`);
  await page.key("Enter", { vk: 13 });
  await page.wait(800);
  ok(await page.eval(`ctx`) === "use:carbon_in" && await j(`[...document.querySelectorAll("#graph .box.generated")].map(b => b.dataset.id)`).then((x) => x.includes("Soil.rules.carbon_in_supply@supply")), "Enter shows the statement it writes out, named as the runner names it");
  await page.eval(`select("Soil.rules.carbon_in_supply@supply")`);
  await page.wait(300);
  ok(/written out by instance `carbon_in`/.test(await page.eval(`document.getElementById("picked").innerText`)), "and it is read-only, saying where it changes");
  await page.eval(`goContext("world", "Soil.uses.carbon_in")`);
  await page.wait(500);
  // Its parameter, from Parameters: one revision, the written-out amount follows.
  await page.eval(`(() => { const i = [...document.querySelectorAll("#picked .params input")].find(x => x.previousSibling.textContent === "ration"); i.value = "30mu"; i.dispatchEvent(new Event("input")); i.dispatchEvent(new Event("change")); })()`);
  await page.wait(1500);
  ok(await rev() === 1 && await j(`graphData.generated.find(x => x.id === "Soil.rules.carbon_in_supply@supply").text`).then((t) => /30mu/.test(t)), "setting its ration is one revision, and the rule it writes out follows");
  // Edit as a new version: only this instance moves.
  await page.eval(`select("Soil.uses.humus_in")`);
  await page.wait(300);
  await page.eval(`[...document.querySelectorAll("#picked .reuse button")].find(b => b.textContent === "Edit as a new version").click()`);
  await page.eval(`(() => { const ta = document.querySelector("#picked .reuse textarea"); ta.value = ta.value.replace("overflow: spill", "overflow: void"); [...document.querySelectorAll("#picked .reuse button")].find(b => b.textContent === "Make the new version").click(); })()`);
  await page.wait(1500);
  const v = await j(`({ humus: graphData.nodes.find(n => n.id === "Soil.uses.humus_in").text, carbon: graphData.nodes.find(n => n.id === "Soil.uses.carbon_in").text, v2: graphData.nodes.some(n => n.id === "Soil.operators.NutrientSupply@v2") })`);
  ok(v.v2 && /version 2/.test(v.humus) && /version 1/.test(v.carbon), "Edit as a new version adds version 2 and moves only that instance");
  // A template from a statement, inserted from Tab with another binding.
  await page.eval(`goContext("world")`);
  await page.wait(300);
  await page.eval(`multi = new Set(["Soil.rules.rotting@rot"]); selected = "Soil.rules.rotting@rot"; markSel(); document.getElementById("nettemplate").click()`);
  await page.eval(`(() => { const i = document.querySelectorAll("#picked .reuse input"); i[0].value = "Rotting"; i[1].value = "a reaction in every cell"; [...document.querySelectorAll("#picked .reuse button")].find(b => b.textContent === "Keep in the library").click(); })()`);
  await page.wait(800);
  ok(/Kept as template Rotting/.test(await page.eval(`document.getElementById("picked").innerText`)), "the statement is kept in the library as a template");
  await page.eval(`(async () => { await loadLibrary(true); openTabMenu(); document.getElementById("tabq").value = "rotting"; drawTabList(); })()`);
  await page.wait(500);
  ok(/template Rotting/.test(await page.eval(`document.getElementById("tablist").innerText`)), "Tab lists it");
  await page.eval(`createFromTab("tpl:Rotting")`);
  await page.wait(400);
  await page.eval(`(() => { const i = [...document.querySelectorAll("#picked .reuse input")].find(x => x.previousSibling.textContent === "rot.rate"); i.value = "20bp"; [...document.querySelectorAll("#picked .reuse button")].find(b => b.textContent === "Insert").click(); })()`);
  await page.wait(1500);
  ok(await j(`graphData.nodes.some(n => n.id === "Soil.rules.rotting_rotting@rot" && /rate: 20bp/.test(n.text))`), "inserting it writes an ordinary rule with its control set (one revision)");
  // Rules made into an operator from the toolbar.
  await page.eval(`multi = new Set(["Soil.rules.spread@spread"]); selected = "Soil.rules.spread@spread"; markSel(); document.getElementById("netoperator").click()`);
  await page.eval(`(() => { const i = document.querySelectorAll("#picked .reuse input"); i[0].value = "Spread"; [...document.querySelectorAll("#picked .reuse button")].find(b => b.textContent === "Make the operator").click(); })()`);
  await page.wait(1800);
  ok(await j(`graphData.nodes.some(n => n.id === "Soil.uses.spread_1") && !graphData.nodes.some(n => n.id === "Soil.rules.spread")`), "the rule became an operator with an instance in its place");
  ok(await j(`loadLibrary(true).then(l => l.operators.some(o => o.name === "Spread"))`), "and the operator is in the library");
  await page.shot(S + "reuse-end.png");
  const e = await j(`__errs`);
  ok(e.length === 0, `no page errors (${e.join(" | ")})`);
};
