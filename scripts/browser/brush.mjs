// Science panels: one selection across panels. node scripts/browser-check.mjs URL scripts/browser/brush.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); await call("run.step", { world: document.getElementById("world").value, ticks: 80 }); })()`);
  await page.wait(500);
  await page.eval(`loadDesktop("Science")`); await page.wait(800);
  // brush ticks 20..50 on the history: dispatch the same pointer events a drag makes
  await page.eval(`(() => { const w = document.querySelector("#lineagebody svg").parentElement, r = w.getBoundingClientRect(), ev = (t, x) => w.dispatchEvent(new PointerEvent(t, { clientX: x, clientY: r.top + 20, pointerId: 1, bubbles: true }));
    w.setPointerCapture = () => {}; ev("pointerdown", r.left + r.width * 0.25); ev("pointerup", r.left + r.width * 0.65); })()`);
  await page.wait(800);
  ok(await page.eval(`science.window && science.window.to > science.window.from`), "brushing sets a window: " + JSON.stringify(await page.eval(`science.window`)));
  ok(/selection: ticks \d+ to \d+/.test(await page.eval(`document.getElementById("lineagenote").innerText`)), "the history says what is selected");
  await page.eval(`activate("life")`); await page.wait(1000);
  ok(/cohort = births in the brushed ticks/.test(await page.eval(`document.getElementById("lifenote").innerText`)), "Life Histories uses the brushed ticks as its cohort");
  await page.eval(`activate("budget")`); await page.wait(600);
  await page.eval(`document.getElementById("budgetstart").click()`); await page.wait(600);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 5 })`);
  await page.eval(`loadBudget()`); await page.wait(800);
  ok(/cannot be observed afterwards: this budget covers ticks 80 to 85/.test(await page.eval(`document.getElementById("budgetnote").innerText`)), "Budget says the brushed ticks are not its scope and what its scope is");
  // lineage selection
  await page.eval(`activate("census")`); await page.wait(800);
  await page.eval(`setScience({ lineage: "Builder" })`); await page.wait(1000);
  ok(/lineage Builder/.test(await page.eval(`document.getElementById("censusnote").innerText`)), "the census follows the selected lineage");
  const atlas = await page.eval(`[...document.querySelectorAll("#censusbody .atlas table th")].map((x) => x.innerText).join("|")`);
  ok(/Builder/.test(atlas) && !/Drifter/.test(atlas), "the atlas shows only the selected lineage: " + atlas);
  await page.shot(S + "brush.png");
};
