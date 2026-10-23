// Science panels S1: Rule Lens and Lineage History in the real viewer. node scripts/browser-check.mjs URL scripts/browser/science.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  // create the Long Night preset via the page's own API and start running
  await page.eval(`(async () => { document.getElementById("example").value = "long_night"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.wait(500);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 40 })`);
  await page.eval(`loadDesktop("Science")`);
  await page.wait(1500);
  await page.eval(`[...document.querySelectorAll("#rulesbody .lens-chip")].find(c => /sunlight/.test(c.textContent)).click()`);
  const txt = await page.eval(`document.getElementById("rulesbody").innerText`);
  ok(/Transfers/.test(txt) && /sunlight\s*on cells/.test(txt) && /when tick < 300/.test(txt), "Rule Lens lists the sunlight rule with its condition under Transfers");
  ok(/revision 0/.test(await page.eval(`document.getElementById("rulesnote").innerText`)), "revision shown");
  const ln = await page.eval(`document.getElementById("lineagenote").innerText + " | " + document.querySelectorAll("#lineagebody polyline").length`);
  ok(/samples of living creatures/.test(ln) && /\| 1$/.test(ln), "lineage history drawn with one line and its sampling label: " + ln);
  await page.shot(S + "science.png");
  // clicking a statement opens it in the network
  await page.eval(`document.querySelector(".lens-stmt").click()`);
  await page.wait(1200);
  const sel = await page.eval(`selected`);
  ok(/^Dish\.rules\./.test(sel || "") && await page.eval(`!!document.querySelector("#graph .box.sel")`), "clicking a statement selects its node: " + sel);
};
