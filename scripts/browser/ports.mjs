// Network H2 viewer, part 1: named sockets, wires on their ports, connection preview. node scripts/browser-check.mjs URL scripts/browser/ports.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`JSON.stringify(${js})`));
  await page.wait(1500);
  await page.eval(`(async () => { document.getElementById("example").value = "nursery"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Rules")`);
  await page.wait(500);
  await page.eval(`activate("network")`);
  await page.wait(1500);
  // every box with ports in the contract draws one named socket for each, with its name
  const a = await j(`(() => { const out = []; for (const n of graphData.nodes) { const ports = (n.contract && n.contract.ports) || []; if (!ports.length) continue; const el = document.querySelector('#graph .box[data-id="' + n.id + '"]'); if (!el) continue; out.push({ id: n.id, want: ports.map(p => p.key), got: [...el.querySelectorAll(".sock")].map(s => s.dataset.port), labels: [...el.querySelectorAll(".plabel")].map(l => l.textContent) }); } return out; })()`);
  ok(a.length > 5 && a.every((x) => JSON.stringify(x.want) === JSON.stringify(x.got) && x.labels.length === x.want.length), `${a.length} boxes draw exactly the sockets of their contract, each with its name`);
  const tr = a.find((x) => x.want.join() === "from,to");
  ok(!!tr && tr.labels.join() === "from,to", `a transfer shows its from and to sockets (${tr && tr.id})`);
  // a wire ends where its port is: compare the path's end points with the socket centres
  const w = await j(`(() => { const g = graphData; const wire = g.wires.find(w => w.to_port === "from"); const el = document.querySelector('#graph .box[data-id="' + wire.to + '"]'); const sock = el.querySelector('.sock[data-port="' + wire.to_port + '"]'); const gr = document.getElementById("graph").getBoundingClientRect(), s = cams.graph.s; const sr = sock.getBoundingClientRect(); const cy = (sr.top + sr.height / 2 - gr.top) / s; const paths = [...document.querySelectorAll("#wires path.wire")]; const ends = paths.map(p => { const d = p.getAttribute("d"); const m = d.match(/ ([\\d.]+),([\\d.]+)$/); return m ? +m[2] : null; }); return { cy, near: ends.some(y => y !== null && Math.abs(y - cy) < 2.5) }; })()`);
  ok(w.near, "a wire ends at the vertical centre of the socket it stands for");
  await page.shot(S + "ports.png");
  // pull a wire from the sun source over the sunlight transfer: the box says what the connection would do, or why not
  const src = await j(`graphData.nodes.find(n => /sources\\.sun$/.test(n.id)).id`);
  const tbox = await j(`graphData.nodes.find(n => /rules\\.sunlight@/.test(n.id)).id`);
  const pos = async (id, sel) => j(`(() => { const r = document.querySelector('#graph .box[data-id="${id}"] ${sel}').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  await page.eval(`fitCam(cams.graph); applyCam(cams.graph)`);
  const sp = await pos(src, ".port.out");
  const centre = async (id) => j(`(() => { const r = document.querySelector('#graph .box[data-id="${id}"]').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  const tc = await centre(tbox);
  const rev0 = await page.eval(`graphData.revision`);
  await page.mouse("mousePressed", sp.x, sp.y);
  for (let i = 1; i <= 8; i++) await page.mouse("mouseMoved", sp.x + (tc.x - sp.x) * i / 8, sp.y + (tc.y - sp.y) * i / 8);
  await page.wait(700);
  let h = await j(`({ shown: document.getElementById("wirehint").style.display, text: document.getElementById("wirehint").textContent, ok: !!document.querySelector("#graph .box.okdrop"), bad: !!document.querySelector("#graph .box.nodrop") })`);
  ok(h.shown === "block" && (h.ok || h.bad) && h.text.length > 3, `hovering a box says what the connection would do or why not: "${h.text}" (${h.ok ? "accepted" : "refused"})`);
  await page.shot(S + "ports-preview.png");
  // over a box that cannot take it (a reaction), it is refused with a reason
  const rx = await j(`graphData.nodes.find(n => n.kind === "reaction").id`);
  const rc = await centre(rx);
  await page.mouse("mouseMoved", rc.x, rc.y);
  await page.wait(700);
  h = await j(`({ text: document.getElementById("wirehint").textContent, bad: !!document.querySelector("#graph .box.nodrop"), ok: !!document.querySelector("#graph .box.okdrop") })`);
  console.log("over a reaction:", h.text);
  ok(h.bad && !h.ok && h.text.length > 3, "a box that cannot take the wire turns red and says why");
  // letting go over empty space changes nothing and clears the preview
  await page.mouse("mouseMoved", 5, 5);
  await page.mouse("mouseReleased", 5, 5, 0);
  await page.wait(300);
  h = await j(`({ shown: document.getElementById("wirehint").style.display, marks: document.querySelectorAll("#graph .box.okdrop, #graph .box.nodrop").length, rev: graphData.revision })`);
  ok(h.shown === "none" && h.marks === 0 && h.rev === rev0, "letting go elsewhere clears the preview and changes no revision");
};
