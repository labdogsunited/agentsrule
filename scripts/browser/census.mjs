// Science panels S2: Genome Census in the real viewer. node scripts/browser-check.mjs URL scripts/browser/census.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.wait(500);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 10 })`);
  await page.eval(`loadDesktop("Science")`);
  await page.wait(500);
  await page.eval(`[...document.querySelectorAll(".tabs *")].find((e) => e.children.length === 0 && e.textContent.trim() === "Genome Census")?.click()`);
  await page.wait(1500);
  const txt = await page.eval(`document.getElementById("censusbody").innerText`);
  ok(/Drifter/.test(txt) && /Builder/.test(txt) && /min \/ median/.test(txt), "census lists both lineages");
  ok(/exact/.test(await page.eval(`document.getElementById("censusnote").innerText`)), "coverage is stated");
  await page.eval(`document.querySelector("#censusbody button").click()`);
  await page.wait(1500);
  const cmp = await page.eval(`document.querySelector("#censusbody .cmpres").innerText`);
  ok(/shared/.test(cmp) && /only in A/.test(cmp), "comparison shown: " + cmp.split("\n")[0].slice(0, 120));
  await page.shot(S + "census.png");
};
