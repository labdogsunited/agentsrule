// Science panels S3/S4: Flow Map in the real viewer. node scripts/browser-check.mjs URL scripts/browser/flow.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "long_night"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); await call("run.step", { world: document.getElementById("world").value, ticks: 20 }); })()`);
  await page.wait(500);
  await page.eval(`loadDesktop("Science")`); await page.wait(500);
  await page.eval(`activate("flow")`); await page.wait(1000);
  ok(await page.eval(`[...document.querySelectorAll("#flowg .fn")].filter(n => n.dataset.id.startsWith("r:")).length >= 4`), "declared map has a node per reaction");
  await page.eval(`document.getElementById("flowbase").click()`); await page.wait(300);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 20 })`);
  await page.eval(`(() => { const m = document.getElementById("flowmode"); m.value = "measured"; m.onchange(); })()`); await page.wait(1000);
  await page.eval(`document.querySelector("#flowbody details").open = true`);
  const txt = await page.eval(`document.getElementById("flowbody").innerText + " | " + document.getElementById("flownote").innerText`);
  ok(/interval ticks 20 to 40/.test(txt) && /Dish\.rules\.rotting/.test(txt), "measured over the interval, naming the statement: " + txt.slice(-160).replace(/\n/g, " "));
  await page.shot(S + "flow.png");
};
