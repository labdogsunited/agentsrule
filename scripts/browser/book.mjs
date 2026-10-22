// Science panels S5: Notebook in the real viewer. node scripts/browser-check.mjs URL scripts/browser/book.mjs
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  await page.eval(`(async () => { document.getElementById("example").value = "dish"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); await call("run.step", { world: document.getElementById("world").value, ticks: 30 }); })()`);
  await page.wait(500);
  await page.eval(`loadDesktop("Science")`); await page.wait(500);
  await page.eval(`activate("book")`); await page.wait(800);
  await page.eval(`(() => { const i = document.querySelectorAll("#bookbody input[type=text]"); i[0].value = "first look"; i[1].value = "why is Builder rising?"; [...document.querySelectorAll("#bookbody button")].find((b) => /Bookmark/.test(b.textContent)).click(); })()`);
  await page.wait(1000);
  const txt = await page.eval(`document.getElementById("bookbody").innerText`);
  ok(/Bookmark 1: first look \(tick 30/.test(txt) && /evidence kept: source, stats/.test(txt), "a bookmark with its evidence: " + txt.split("\n").find((l) => /Bookmark 1/.test(l)).slice(0, 110));
  ok(/bookmark/.test(txt), "it is on the timeline");
  await page.shot(S + "book.png");
};
