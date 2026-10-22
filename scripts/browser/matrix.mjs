// Science panels S4: the read/write matrix in Rule Lens. node scripts/browser-check.mjs URL scripts/browser/matrix.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "long_night"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.wait(500);
  await page.eval(`loadDesktop("Science")`);
  await page.wait(800);
  await page.eval(`document.querySelector('input[name="rmode"][value="matrix"]').click()`);
  await page.wait(1000);
  const note = await page.eval(`document.getElementById("rulesnote").innerText`);
  ok(/not none exist/.test(note), "the matrix says a blank cell means none found");
  const th = await page.eval(`[...document.querySelectorAll("#rulesbody th.vcol")].map(x => x.dataset.c).join(",")`);
  ok(/light/.test(th) && /sun/.test(th) && /rot/.test(th), "columns: " + th);
  await page.eval(`document.querySelector('#rulesbody th.vcol[data-c="nitrate"]').click()`);
  await page.wait(800);
  ok(await page.eval(`document.querySelectorAll("#rulesbody td.mxc.on").length > 0`), "clicking the nitrate column marks it");
  await page.shot(S + "matrix.png");
};
