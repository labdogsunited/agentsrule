// Science panels S4: Region in the real viewer. node scripts/browser-check.mjs URL scripts/browser/region.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); await call("run.step", { world: document.getElementById("world").value, ticks: 20 }); })()`);
  await page.wait(500);
  await page.eval(`loadDesktop("Science")`); await page.wait(500);
  await page.eval(`activate("region")`); await page.wait(1000);
  const txt = await page.eval(`document.getElementById("regionbody").innerText`);
  ok(/creatures \(\d+\)/.test(txt) && /cells \(\d+/.test(txt), "region totals shown: " + txt.slice(0, 80).replace(/\n/g, " "));
  await page.eval(`(() => { const i = document.querySelectorAll("#regionbody input")[1]; i.value = 100000; i.onchange(); })()`);
  await page.wait(800);
  ok(/inside 0 to/.test(await page.eval(`document.getElementById("regionnote").innerText`)), "a range outside the world is refused in words");
  await page.shot(S + "region.png");
};
