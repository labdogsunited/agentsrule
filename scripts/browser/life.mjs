// Science panels S5: Life Histories in the real viewer. node scripts/browser-check.mjs URL scripts/browser/life.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); await call("run.step", { world: document.getElementById("world").value, ticks: 80 }); })()`);
  await page.wait(500);
  await page.eval(`loadDesktop("Science")`); await page.wait(500);
  await page.eval(`activate("life")`); await page.wait(1200);
  const txt = await page.eval(`document.getElementById("lifebody").innerText`);
  ok(/Age of the living/.test(txt) && /still alive \(censored\)/.test(txt), "ages and a cohort with censoring: " + txt.split("\n").find((l) => /born ·/.test(l)).slice(0, 110));
  await page.shot(S + "life.png");
};
