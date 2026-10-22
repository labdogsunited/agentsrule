// Science panels S5: Ancestry in the real viewer. node scripts/browser-check.mjs URL scripts/browser/family.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.wait(500);
  await page.eval(`call("run.step", { world: document.getElementById("world").value, ticks: 80 })`);
  await page.eval(`loadDesktop("Science")`);
  await page.wait(500);
  await page.eval(`activate("family")`);
  await page.wait(800);
  await page.eval(`(() => { const s = document.querySelector("#familybody select"); s.selectedIndex = s.options.length - 1; s.onchange(); })()`);
  await page.wait(1000);
  const txt = await page.eval(`document.getElementById("familybody").innerText`);
  ok(/this creature/.test(txt) && /Descendants:/.test(txt), "family listed");
  ok(/reaches founder|nearest/.test(txt), "says whether the line is complete");
  await page.shot(S + "family.png");
};
