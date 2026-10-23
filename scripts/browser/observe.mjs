// Agent tracking T3 to T5 (proposals/agent-tracking.md §17): a recording keeps its tick cadence whatever the page receives and
// survives a reconnect without duplicates; a watch shows as a Tracker node in the Network's Observe context, and editing the
// node edits the same object (definition revision, stale refusal, undo, display flag); procedural trackers (live query, a
// region captured from the Viewport, latched, family) are made from the form; nothing moves the world's revision.
// node scripts/browser-check.mjs URL scripts/browser/observe.mjs
// Start a viewer first: emerge-server serve --viewer --port N (the URL, token included, is printed). OUT=dir keeps screenshots.
export default async (page) => {
  const S = (process.env.OUT || "/tmp") + "/";
  const ok = (c, m) => { if (!c) throw new Error(m); console.log("ok:", m); };
  const j = async (js) => JSON.parse(await page.eval(`(async () => JSON.stringify(await (${js})))()`) ?? "null");
  await page.eval(`window.__errs = []; window.addEventListener("error", (e) => __errs.push(String(e.message))); window.addEventListener("unhandledrejection", (e) => __errs.push(String(e.reason && e.reason.message || e.reason)));`);
  await page.wait(1200);
  await page.eval(`(async () => { document.getElementById("example").value = "nursery"; document.getElementById("example").dispatchEvent(new Event("change")); await create(); })()`);
  await page.eval(`loadDesktop("Track")`);
  await page.wait(1500);
  const W = `document.getElementById("world").value`;
  await page.eval(`call("run.step", { world: ${W}, ticks: 20 })`);
  await page.wait(1200);
  // A watch, recorded from its card.
  const ids = await j(`call("entities.find", { world: ${W}, sort: "carried.body", descending: true, limit: 3 }).then(r => r.agents.map(a => a.id))`);
  await page.eval(`setAgent(${JSON.stringify(ids[0])}); trackAgent(${JSON.stringify(ids[0])})`);
  await page.wait(800);
  await page.eval(`activate("watches")`);
  await page.wait(1200);
  await page.eval(`[...document.querySelectorAll("#watchbody .wcard button")].find(b => b.textContent === "Record…").click()`);
  await page.wait(200);
  await page.eval(`(() => { const f = document.querySelector("#watchbody .wform"); f.querySelector(".ev").value = 2; f.querySelector(".go").click(); })()`);
  await page.wait(800);
  const t = await j(`track.list.trackers.find(x => x.kind === "watch")`);
  ok(t.recording && t.recording.on && t.recording.every === 2, `the card starts a recording every 2 ticks (first sample at tick ${t.recording.first_sample_tick})`);
  // The page receives one frame a second while the world runs: the recording still samples every 2 ticks.
  await page.eval(`(() => { document.getElementById("fps").value = 1; document.getElementById("fps").dispatchEvent(new Event("change")); })()`);
  await page.wait(500);
  await page.eval(`call("run.resume", { world: ${W} })`);
  await page.wait(2500);
  await page.eval(`call("run.pause", { world: ${W} })`);
  await page.wait(300);
  const s = await j(`call("tracker.samples", { world: ${W}, tracker: ${JSON.stringify(t.id)}, limit: 10000 })`);
  const ticks = [...new Set(s.rows.map((r) => r[1]))];
  const cov = s.coverage;
  ok(cov.sampled_ticks >= 5 && ticks.every((x, i) => i === 0 || x - ticks[i - 1] === 2), `samples every 2 ticks (${cov.sampled_ticks} sample ticks, ${cov.first} to ${cov.last}) though the page got about one frame a second`);
  // A reconnect: the recording went on without the page; asking after the last sequence number gives nothing twice.
  const last = s.last_seq;
  await page.eval(`(() => { ws.onclose = null; ws.close(); })()`);
  await page.wait(300);
  await page.eval(`connect()`);
  await page.wait(2500);
  await page.eval(`call("run.step", { world: ${W}, ticks: 6 })`);
  await page.wait(500);
  const s2 = await j(`call("tracker.samples", { world: ${W}, tracker: ${JSON.stringify(t.id)}, after: ${last}, limit: 10000 })`);
  ok(s2.coverage.sampled_ticks > cov.sampled_ticks && s2.rows.every((r) => r[0] > last), `after reconnecting, the recording went on (${s2.coverage.sampled_ticks} sample ticks) and samples after #${last} are new ones only (${s2.rows.length}${s2.events.some((e) => e.event === "death") ? "; the creature died, which the events say" : ""})`);
  // The recorded trail: drawn from the server's samples, apart from the dotted display trail.
  await page.eval(`activate("viewport"); activate("watches")`);
  await page.wait(1200);
  await page.eval(`[...document.querySelectorAll("#watchbody .wcard button")].find(b => b.textContent === "Recorded trail").click()`);
  await page.wait(1200);
  ok(await j(`track.rec.has(${JSON.stringify(t.id)}) && [...track.rec.get(${JSON.stringify(t.id)}).pts.values()][0].length`) >= 5 && await j(`!!document.querySelector("#trackov .rtrail")`), "the recorded trail is drawn from the server's samples");
  // Observe: the watch is a Tracker node; Run agents feed it; its outputs show.
  await page.eval(`openObserve(${JSON.stringify(t.id)})`);
  await page.wait(1500);
  const nodes = await j(`[...document.querySelectorAll("#graph .box")].map(b => b.dataset.id)`);
  ok(nodes.includes("obs:agents") && nodes.includes(`obs:${t.id}`) && nodes.includes(`obs:${t.id}:marker`) && nodes.includes(`obs:${t.id}:samples`), `the Observe network shows Run agents, the Tracker and its outputs (${nodes.length} nodes)`);
  ok(await j(`document.querySelectorAll("#wires path.wire").length`) >= 4, "wired by their typed sockets");
  ok(await j(`!!document.querySelector(".box[data-id='obs:${t.id}'] .recdot.on")`), "the recording indicator is on");
  await page.shot(S + "observe-network.png");
  // Parameters edit the same object: renaming the node renames the watch.
  await page.eval(`select("obs:${t.id}")`);
  await page.wait(500);
  ok(/Tracker · Agent/.test(await page.eval(`document.getElementById("picked").innerText`)), "selecting the node shows its Parameters");
  await page.eval(`(() => { const f = document.querySelector("#picked .tparm"); f.querySelector(".nm").value = "Interesting one"; f.querySelector(".ap").click(); })()`);
  await page.wait(900);
  ok(await j(`track.list.trackers.find(x => x.id === ${JSON.stringify(t.id)}).name`) === "Interesting one", "Apply on the node renamed the watch (one object)");
  ok(/Interesting one/.test(await page.eval(`document.getElementById("watchbody").innerText`)), "and the Watches card shows the new name");
  // A change made elsewhere meanwhile: the node's next Apply is refused as stale.
  await page.eval(`call("tracker.update", { world: ${W}, tracker: ${JSON.stringify(t.id)}, name: "Renamed elsewhere" })`);
  await page.eval(`(() => { const f = document.querySelector("#picked .tparm"); f.querySelector(".nm").value = "Late edit"; f.querySelector(".ap").click(); })()`);
  await page.wait(900);
  ok(/stale/.test(await page.eval(`document.querySelector("#picked .tparm .msg") ? document.querySelector("#picked .tparm .msg").textContent : ""`)) || await j(`track.list.trackers.find(x => x.id === ${JSON.stringify(t.id)}).name`) === "Renamed elsewhere", "an edit made against an old definition is refused, not written over");
  // The display flag on the node hides its markers; Undo shows them again.
  await page.eval(`document.querySelector(".box[data-id='obs:${t.id}'] .dflag").click()`);
  await page.wait(900);
  ok(await j(`!track.list.trackers.find(x => x.id === ${JSON.stringify(t.id)}).display.visible`) && !(await j(`[...document.querySelectorAll("#graph .box")].some(b => b.dataset.id === "obs:${t.id}:marker")`)), "the display flag turns its Marker off");
  await page.eval(`document.getElementById("obsundo").click()`);
  await page.wait(900);
  ok(await j(`track.list.trackers.find(x => x.id === ${JSON.stringify(t.id)}).display.visible`), "Undo turns it on again");
  // Moving the node is layout: no definition change.
  const def0 = await j(`track.list.trackers.find(x => x.id === ${JSON.stringify(t.id)}).definition`), rev0 = await j(`track.list.observation_revision`);
  await page.eval(`(() => { const p = placed.get("obs:${t.id}"); p.x += 40; savePosition("obs:${t.id}"); })()`);
  await page.wait(500);
  await page.eval(`loadTrackers()`);
  ok(await j(`track.list.trackers.find(x => x.id === ${JSON.stringify(t.id)}).definition`) === def0 && await j(`track.list.observation_revision`) === rev0, "moving the node changes no definition and no revision");
  // Procedural sets from the form: a live query, a capture of the Viewport's view, a family.
  await page.eval(`activate("watches"); newTrackerForm("live")`);
  await page.eval(`(() => { const f = document.getElementById("wnewform"); f.querySelector(".wh").value = "age < 5"; f.querySelector(".go").click(); })()`);
  await page.wait(900);
  const live = await j(`track.list.trackers.find(x => x.mode === "live")`);
  ok(live && /whoever matches now: age < 5/.test(live.means), `a live query tracker (${live && live.count_now} now)`);
  await page.eval(`activate("viewport")`);
  await page.eval(`(() => { const c = cams.picture, r = stage.getBoundingClientRect(); c.s = 8; c.x = r.width / 2 - 250 * 8; c.y = (r.height - chartsH) / 2 - 250 * 8; c.fit = false; applyCam(c); })()`);
  await page.wait(300);
  await page.eval(`activate("watches"); newTrackerForm("capture")`);
  await page.eval(`(() => { const f = document.getElementById("wnewform"); f.querySelector(".c-view").click(); f.querySelector(".go").click(); })()`);
  await page.wait(1200);
  const cap = await j(`track.list.trackers.find(x => /^Captured at tick/.test(x.name))`);
  ok(cap && cap.mode === "fixed" && /region x/.test(cap.name), `capturing the Viewport's view makes fixed individuals (${cap && cap.count_now}), named by the rectangle and tick`);
  await page.eval(`activate("watches"); newTrackerForm("descendants")`);
  await page.eval(`(() => { const f = document.getElementById("wnewform"); f.querySelector(".rt").value = ${JSON.stringify(ids[1])}; f.querySelector(".go").click(); })()`);
  await page.wait(900);
  ok(await j(`track.list.trackers.some(x => x.mode === "descendants")`), "a family tracker from the form");
  // Details shows a tracker's output with each member's status.
  await page.eval(`details.scope = "tracker:${live.id}"; activate("details"); loadDetails()`);
  await page.wait(1200);
  ok(/whoever matches now/.test(await page.eval(`document.getElementById("detailsnote").textContent`)), "Details shows the live tracker's output and says what it means");
  await page.shot(S + "observe-end.png");
  ok(await j(`call("stats.query", { world: ${W} }).then(r => r.revision)`) === 0, "nothing here moved the world's revision");
  const errs = await j(`__errs`);
  ok(errs.length === 0, `no page errors (${errs.join(" | ")})`);
};
