// Science panels S6: Compare Runs in the real viewer. node scripts/browser-check.mjs URL scripts/browser/compare.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const make = async (seed, ticks) => page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); document.getElementById("seed").value = ${seed}; await create(); await call("run.step", { world: document.getElementById("world").value, ticks: ${ticks} }); })()`);
  await make(1, 60); await page.wait(300); await make(2, 60); await page.wait(300);
  await page.eval(`loadDesktop("Science")`); await page.wait(500);
  await page.eval(`activate("compare")`); await page.wait(1200);
  const txt = await page.eval(`document.getElementById("comparebody").innerText`);
  ok(/Differences: seed \(\d against \d\)/.test(txt) && /single trials/.test(txt), "setup difference named: " + txt.slice(0, 160).replace(/\n/g, " "));
  ok(await page.eval(`document.querySelectorAll("#comparebody polyline[stroke-dasharray='5 4']").length > 0`), "B drawn dashed");
  ok(/Outcomes: every world/.test(txt + await page.eval(`document.getElementById("comparebody").innerText`)), "the outcome grid is shown");
  await page.eval(`(() => { const i = document.querySelector("#comparebody input[type=text]"); i.value = "same source, seeds 1 and 2"; [...document.querySelectorAll("#comparebody button")].find((b) => /pair/.test(b.textContent)).click(); })()`);
  await page.wait(1000);
  const after = await page.eval(`document.getElementById("comparebody").innerText`);
  ok(/Declared pair: w\d and w\d \(same source, seeds 1 and 2\)/.test(after), "a declared pair is shown as declared");
  ok(await page.eval(`[...document.querySelectorAll("#comparebody button")].some((b) => /Export CSV/.test(b.textContent)) && [...document.querySelectorAll("#comparebody button")].some((b) => /Export SVG/.test(b.textContent))`), "exports offered");
  await page.shot(S + "compare.png");
};
