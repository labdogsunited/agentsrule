// Science panels S3: Resource Budget in the real viewer. node scripts/browser-check.mjs URL scripts/browser/budget.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.wait(500);
  await page.eval(`loadDesktop("Science")`);
  await page.wait(500);
  await page.eval(`activate("budget")`);
  await page.wait(800);
  await page.eval(`document.getElementById("budgetstart").click()`);
  await page.wait(500);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 60 })`);
  await page.eval(`loadBudget()`);
  await page.wait(800);
  const txt = await page.eval(`document.getElementById("budgetbody").innerText`);
  ok(/Drifter/.test(txt) && /upkeep/.test(txt) && /unreconciled/.test(txt), "budget lists lineages and account lines");
  ok(/the lines explain the change exactly/.test(txt), "reconciles exactly");
  ok(await page.eval(`document.querySelectorAll("#budgetbody svg rect").length > 4`), "a waterfall is drawn");
  await page.shot(S + "budget.png");
};
