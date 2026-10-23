// The documentation button and F1: they open the page for the pane you are in, and the help site has the app guide.
// node scripts/browser-check.mjs URL scripts/browser/docs.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`JSON.stringify(${js})`));
  await page.wait(1500);
  await page.eval(`window.__opened = []; window.open = (u) => { window.__opened.push(u); return null; }`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  ok((await page.eval(`document.getElementById("helpbtn").textContent`)) === "Docs", "the header button is called Docs");
  await page.eval(`document.getElementById("helpbtn").click()`);
  let urls = await j(`window.__opened`);
  ok(/^\/help#guide\/using-/.test(urls[0]), `the button opens the help site on a page of the app guide (${urls[0]})`);
  // F1 in the Network opens the network editor page; in a science panel, the science page
  await page.eval(`window.__opened = []; activate("network")`);
  await page.key("F1", { vk: 112 });
  urls = await j(`window.__opened`);
  ok(urls[0] === "/help#guide/using-network", `F1 in the Network opens the network editor page (${urls[0]})`);
  await page.eval(`window.__opened = []; activate("rules")`);
  await page.key("F1", { vk: 112 });
  urls = await j(`window.__opened`);
  ok(urls[0] === "/help#guide/using-science", `F1 in the Rule Lens opens the science page (${urls[0]})`);
  // the help site itself
  const site = await page.eval(`fetch("/help").then(r => r.text())`);
  ok(/Using Emerge/.test(site) && /The network editor/.test(site) && /Command line/.test(site) && /Language Guide/.test(site), "the help site lists the app guide first, then the language guide");
  await page.eval(`location.href = "/help#guide/using-network"`);
  await page.wait(1200);
  const h1 = await page.eval(`(document.querySelector(".doc-title") || {}).textContent`);
  ok(/network editor/i.test(h1), `the page for the network editor opens from the link (${h1})`);
  const toc = await j(`[...document.querySelectorAll(".houdini-toc .toc-link")].map(a => a.textContent)`);
  ok(toc.includes("Boxes, ports and wires") && toc.includes("The inspector") && !toc.includes("Synopsis"), `the "on this page" list follows the page's own headings (${toc.join(" | ")})`);
  await page.shot(S + "docs-network.png");
};
