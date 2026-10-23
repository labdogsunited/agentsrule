//! The API over WebSocket on localhost (spec §12.2): one text frame per JSON-RPC message. One
//! engine thread owns every run (a GPU world cannot move between threads); each connection has a
//! thread that passes its messages to the engine and the replies back. Between calls the engine
//! ticks the worlds that are running, and pushes the frames that viewers subscribed to
//! (`view.subscribe`, `proposals/live-viewer.md`), which only this server offers.

use crate::api::Engine;
use crate::record::Recording;
use emerge_lang::ast::View;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::Message;

/// A connection as the engine sees it: where its frames go, and whether it has sent the last one
/// (a frame is never queued behind another, so a slow viewer gets fewer frames, not old ones).
struct Conn {
    push: mpsc::Sender<Vec<u8>>,
    ready: AtomicBool,
}

type Call = (String, mpsc::Sender<Option<String>>, Arc<Conn>);

/// A viewer's subscription to one view of one world.
struct Sub {
    id: u64,
    conn: Arc<Conn>,
    world: String,
    view: View,
    charts: Vec<crate::picture::Charts>,
    every: u64,
    min_gap: Duration,
    cell: u32,
    last_tick: Option<u64>,
    last_sent: Instant,
    /// Keep a pick index of the frames sent (`view.subscribe` with `pick: true`).
    pick: bool,
    /// The creatures this viewer selected (`view.watch`): their place travels in each frame's header.
    watch: Vec<u64>,
    /// The last frames sent, newest last, for `view.pick_frame` (`proposals/agent-tracking.md` §12).
    kept: std::collections::VecDeque<Kept>,
    /// The tracking revision of the last frame: a changed selection or tracker sends a fresh frame of the same tick,
    /// so a paused world's markers are not out of date.
    marks: Option<u64>,
}

/// What a sent frame showed, for picking against the picture on screen: the run, the tick and revision, and
/// every agent the view drew, with its centre and glyph radius in picture pixels.
struct Kept {
    frame: u64,
    tick: u64,
    revision: u64,
    run: String,
    map: crate::picture::MapInfo,
    wrap: bool,
    agents: Vec<(u64, [f32; 2], f32, [f32; 2], String)>,
}

/// Frames a subscription keeps for picking, and the bytes all subscriptions keep at most.
const KEPT_FRAMES: usize = 2;
const KEPT_BYTES: usize = 64 << 20;
/// Overlay entries a frame header carries at most.
const OVERLAY_MAX: usize = 512;

/// The browser viewer (`proposals/live-viewer.md`): the server answers `GET /` with its page, and
/// accepts a web page's socket only from its own origin, with this process's token.
pub struct Viewer {
    /// 128 random bits, in hex.
    pub token: String,
}

/// The exit status of a server that was asked to restart (`server.restart`); `agentsrule web`
/// starts it again. A stop exits with 0.
pub const RESTART_EXIT: i32 = 75;

impl Viewer {
    /// A viewer with the token in `EMERGE_VIEWER_TOKEN` (32 or more hex digits; `agentsrule web`
    /// sets it, so an open page reconnects after a restart), else a fresh one from the operating system.
    pub fn new() -> std::io::Result<Viewer> {
        if let Ok(t) = std::env::var("EMERGE_VIEWER_TOKEN") {
            if t.len() >= 32 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Ok(Viewer { token: t });
            }
        }
        let mut bytes = [0u8; 16];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(Viewer { token: bytes.iter().map(|b| format!("{b:02x}")).collect() })
    }

    /// The address to open: the token is in the fragment, which a browser never sends to the server.
    pub fn url(&self, port: u16) -> String {
        format!("http://127.0.0.1:{port}/#k={}", self.token)
    }
}

/// Serves on `listener` until the process ends. Journals go to `journal_dir`, if given.
pub fn serve(listener: TcpListener, journal_dir: Option<std::path::PathBuf>) {
    serve_with(listener, journal_dir, None);
}

/// `serve`, and with `viewer` also the browser viewer.
pub fn serve_with(listener: TcpListener, journal_dir: Option<std::path::PathBuf>, viewer: Option<Viewer>) {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let viewer = viewer.map(Arc::new);
    let (to_engine, calls) = mpsc::channel::<Call>();
    std::thread::spawn(move || engine(calls, journal_dir));
    for stream in listener.incoming().flatten() {
        let to_engine = to_engine.clone();
        let viewer = viewer.clone();
        std::thread::spawn(move || connection(stream, to_engine, viewer.as_deref(), port));
    }
}

fn engine(calls: mpsc::Receiver<Call>, journal_dir: Option<std::path::PathBuf>) {
    let mut engine = journal_dir.clone().map_or_else(Engine::new, Engine::with_journals);
    let (mut subs, mut next_sub) = (Vec::<Sub>::new(), 0u64);
    let mut recs = Vec::<Recording>::new();
    loop {
        let next = if engine.busy() {
            calls.try_recv().map_err(|e| e == mpsc::TryRecvError::Disconnected)
        } else if !subs.is_empty() {
            calls.recv_timeout(Duration::from_millis(10)).map_err(|e| e == mpsc::RecvTimeoutError::Disconnected)
        } else {
            calls.recv().map_err(|_| true)
        };
        match next {
            Ok((message, reply, conn)) => {
                let answer = recording(&message, &engine, &mut recs, &mut next_sub, journal_dir.as_deref())
                    .or_else(|| subscription(&message, &engine, &mut subs, &mut next_sub, conn))
                    .unwrap_or_else(|| engine.handle(&message));
                let _ = reply.send(answer);
            }
            Err(true) => return, // every sender is gone
            // Nothing to answer: tick the running worlds for a moment, then look again. While
            // something records, one tick at a time, so a frame lands on every tick it is due.
            Err(false) if engine.busy() && !recs.is_empty() => {
                let t = Instant::now();
                while engine.busy() && !recs.is_empty() && t.elapsed() < Duration::from_millis(20) {
                    engine.tick_running();
                    advance(&engine, &mut recs);
                }
            }
            Err(false) if engine.busy() => engine.background(0.02),
            Err(false) => {}
        }
        advance(&engine, &mut recs);
        push_frames(&engine, &mut subs);
    }
}

/// Writes the frames that are due; a finished (or failed) recording is dropped.
fn advance(engine: &Engine, recs: &mut Vec<Recording>) {
    recs.retain_mut(|r| r.advance(engine).unwrap_or(false));
}

fn reply(req: &Value, result: Result<Value, String>) -> Option<String> {
    let r = match result {
        Ok(v) => json!({"jsonrpc": "2.0", "id": req["id"], "result": v}),
        Err(e) => json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32602, "message": e}}),
    };
    req.get("id").map(|_| r.to_string())
}

/// `view.record` and `view.recordings`; `None` for any other call.
fn recording(message: &str, engine: &Engine, recs: &mut Vec<Recording>, next_id: &mut u64, root: Option<&std::path::Path>) -> Option<Option<String>> {
    let req: Value = serde_json::from_str(message).ok()?;
    match req["method"].as_str()? {
        "view.record" => {
            let result = root.ok_or_else(|| "recording needs the server's --journal-dir".to_string()).and_then(|root| {
                *next_id += 1;
                let r = Recording::start(engine, root, *next_id, &req["params"])?;
                let status = r.status();
                recs.push(r);
                Ok(status)
            });
            Some(reply(&req, result))
        }
        "view.recordings" => Some(reply(&req, Ok(json!({"recordings": recs.iter().map(Recording::status).collect::<Vec<_>>()})))),
        _ => None,
    }
}

/// `view.subscribe` and `view.unsubscribe`, which need the connection; `None` for any other call.
fn subscription(message: &str, engine: &Engine, subs: &mut Vec<Sub>, next_sub: &mut u64, conn: Arc<Conn>) -> Option<Option<String>> {
    let req: Value = serde_json::from_str(message).ok()?;
    let method = req["method"].as_str()?;
    if !["view.subscribe", "view.unsubscribe", "view.watch", "view.pick_frame"].contains(&method) {
        return None;
    }
    let p = &req["params"];
    let result = if method == "view.watch" {
        // The creatures this viewer has selected: their place comes with every frame, from that frame.
        (|| {
            let id = p["subscription"].as_u64().ok_or("`subscription` must be a subscription id")?;
            let s = subs.iter_mut().find(|s| s.id == id).ok_or("no such subscription")?;
            let ids = if p["ids"].is_null() { vec![] } else { crate::track::parse_ids(&p["ids"])? };
            if ids.len() > 64 { return Err("a viewer selects at most 64 creatures at once".to_string()); }
            s.watch = ids;
            s.marks = None;
            Ok(json!({"subscription": id, "watching": s.watch.iter().map(|x| x.to_string()).collect::<Vec<_>>()}))
        })()
    } else if method == "view.pick_frame" {
        pick_frame(p, subs)
    } else if method == "view.unsubscribe" {
        let id = p["subscription"].as_u64();
        let before = subs.len();
        subs.retain(|s| Some(s.id) != id);
        if subs.len() < before { Ok(json!({"unsubscribed": id})) } else { Err("no such subscription".to_string()) }
    } else {
        (|| {
            let world = p["world"].as_str().ok_or("`world` must be a string")?;
            let run = engine.run(world).ok_or(format!("no world `{world}`"))?;
            let view = crate::picture::pick_view(run, p["views"].as_str(), p["view"].as_str())?;
            let names: Vec<String> = p["charts"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default();
            let charts = crate::picture::pick_charts(run, p["views"].as_str(), &names)?;
            let num = |k: &str, d: u64| p[k].as_u64().unwrap_or(d);
            *next_sub += 1;
            let fps = num("max_fps", 10).clamp(1, 60);
            let watch = if p["watch"].is_null() { vec![] } else { crate::track::parse_ids(&p["watch"])? };
            subs.push(Sub { id: *next_sub, conn, world: world.to_string(), view, charts, every: num("every", 1).max(1), min_gap: Duration::from_secs_f64(1.0 / fps as f64), cell: num("cell", 0) as u32, last_tick: None, last_sent: Instant::now(),
                pick: p["pick"].as_bool().unwrap_or(false), watch, kept: Default::default(), marks: None });
            Ok(json!({"subscription": *next_sub, "run": run.run_id}))
        })()
    };
    let reply = match result {
        Ok(v) => json!({"jsonrpc": "2.0", "id": req["id"], "result": v}),
        Err(e) if e.starts_with("frame expired") => json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32011, "message": e, "data": {"kind": "frame_expired"}}}),
        Err(e) => json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32602, "message": e}}),
    };
    Some(req.get("id").map(|_| reply.to_string()))
}

/// Sends each subscription its next frame if one is due: the world has moved `every` ticks since
/// the last one (the first is sent at once), the viewer has sent the last, and `max_fps` allows it.
/// A frame is a binary message: the length of a JSON header (4 bytes, little-endian), the header,
/// then the PNG.
fn push_frames(engine: &Engine, subs: &mut Vec<Sub>) {
    subs.retain_mut(|s| {
        let Some(run) = engine.run(&s.world) else { return false };
        let marks_moved = s.last_tick.is_some() && s.marks != Some(run.track.revision);
        let due = (s.last_tick.is_none_or(|t| run.tick() >= t + s.every) || marks_moved) && s.conn.ready.load(Ordering::Acquire) && (s.last_tick.is_none() || s.last_sent.elapsed() >= s.min_gap);
        if !due {
            return true;
        }
        let Ok(shot) = crate::picture::shot(run, &s.view, s.cell, &s.charts, s.pick) else { return false };
        let (map, f) = (shot.map, &shot.frame);
        let frame_id = NEXT_FRAME.fetch_add(1, Ordering::Relaxed);
        // Overlays from this same frame: the viewer's selection and every visible tracker's members.
        let mut want: std::collections::BTreeMap<u64, (bool, Vec<String>)> = run.track.shown().into_iter().map(|(id, t)| (id, (false, t))).collect();
        for id in &s.watch { want.entry(*id).or_insert((false, vec![])).0 = true; }
        let (kx, ky) = (map.w as f32 / f.world_w, map.h as f32 / f.world_h);
        let mut overlay = vec![];
        let mut absent = vec![];
        if !want.is_empty() {
            let at: std::collections::HashMap<u64, usize> = f.agents.iter().enumerate().filter(|(_, a)| want.contains_key(&a.id)).map(|(i, a)| (a.id, i)).collect();
            // The selection first, so a full list never loses it.
            let mut order: Vec<(&u64, &(bool, Vec<String>))> = want.iter().collect();
            order.sort_by_key(|(_, (sel, _))| !*sel);
            for (id, (sel, trackers)) in order {
                match at.get(id) {
                    Some(&i) if overlay.len() < OVERLAY_MAX => {
                        let a = &f.agents[i];
                        overlay.push(json!({"id": id.to_string(), "x": a.pos[0], "y": a.pos[1], "px": a.pos[0] * kx, "py": a.pos[1] * ky, "heading": a.heading, "lineage": f.lineages.get(a.lineage), "selected": sel, "trackers": trackers}));
                    }
                    Some(_) => {}
                    None => absent.push(id.to_string()),
                }
            }
        }
        let header = json!({"subscription": s.id, "world": s.world, "run": run.run_id, "frame": frame_id, "view": s.view.name, "tick": f.tick, "revision": f.revision, "width": shot.w, "height": shot.h, "format": "png",
            "map": {"w": map.w, "h": map.h, "grid_w": map.grid_w, "grid_h": map.grid_h, "world_w": map.world_w, "world_h": map.world_h, "wrap": f.wrap},
            "tracking_revision": run.track.revision, "agents": overlay, "absent": absent, "agents_truncated": want.len().saturating_sub(overlay.len() + absent.len()),
            "pickable": s.pick, "drawn_agents": shot.hits.len()}).to_string();
        if s.pick {
            let agents = shot.hits.iter().map(|h| { let a = &f.agents[h.agent]; (a.id, h.px, h.r, a.pos, f.lineages.get(a.lineage).cloned().unwrap_or_default()) }).collect();
            s.kept.push_back(Kept { frame: frame_id, tick: f.tick, revision: f.revision, run: run.run_id.clone(), map, wrap: f.wrap, agents });
            while s.kept.len() > KEPT_FRAMES { s.kept.pop_front(); }
        }
        let png = shot.png;
        let mut frame = (header.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(header.as_bytes());
        frame.extend_from_slice(&png);
        s.conn.ready.store(false, Ordering::Release);
        s.marks = Some(run.track.revision);
        (s.last_tick, s.last_sent) = (Some(run.tick()), Instant::now());
        s.conn.push.send(frame).is_ok() // a closed connection ends its subscriptions
    });
    // All kept pick indexes within the byte budget: the oldest frames go first.
    let size = |k: &Kept| k.agents.len() * 56 + 64;
    let mut total: usize = subs.iter().flat_map(|s| s.kept.iter()).map(size).sum();
    while total > KEPT_BYTES {
        let Some(s) = subs.iter_mut().filter(|s| s.kept.len() > 1).min_by_key(|s| s.kept.front().map_or(u64::MAX, |k| k.frame)) else { break };
        if let Some(k) = s.kept.pop_front() { total -= size(&k); }
    }
}

static NEXT_FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// `view.pick_frame`: the creatures under a point of a frame this subscription was sent, from what that frame drew
/// (never the world as it is now). `x`, `y` are picture pixels of the map; `tolerance` is the smallest hit radius in
/// pixels (the viewer sends 8 CSS pixels at its zoom). Candidates are nearest first, ties by ID; a frame no longer
/// kept is `frame expired`.
fn pick_frame(p: &Value, subs: &[Sub]) -> Result<Value, String> {
    let id = p["subscription"].as_u64().ok_or("`subscription` must be a subscription id")?;
    let s = subs.iter().find(|s| s.id == id).ok_or("no such subscription")?;
    if !s.pick { return Err("this subscription keeps no pick index: subscribe with pick: true".into()); }
    let frame = p["frame"].as_u64().ok_or("`frame` must be the frame id of a received frame")?;
    let Some(k) = s.kept.iter().find(|k| k.frame == frame) else {
        return Err(format!("frame expired: frame {frame} is no longer kept (the newest is {}); pick on a fresh frame", s.kept.back().map_or("none".into(), |k| k.frame.to_string())));
    };
    let (x, y) = (p["x"].as_f64().ok_or("`x` must be a number")? as f32, p["y"].as_f64().ok_or("`y` must be a number")? as f32);
    let tol = p["tolerance"].as_f64().unwrap_or(8.0).clamp(0.0, 64.0) as f32;
    let limit = p["limit"].as_u64().unwrap_or(8).clamp(1, 50) as usize;
    let (w, h) = (k.map.w as f32, k.map.h as f32);
    let d = |a: f32, b: f32, span: f32| { let d = (a - b).abs(); if k.wrap { d.min(span - d) } else { d } };
    let mut hits: Vec<(f32, &(u64, [f32; 2], f32, [f32; 2], String))> = k.agents.iter().filter_map(|a| {
        let (dx, dy) = (d(a.1[0], x, w), d(a.1[1], y, h));
        let dist = (dx * dx + dy * dy).sqrt();
        (dist <= a.2.max(tol)).then_some((dist, a))
    }).collect();
    hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1 .0.cmp(&b.1 .0)));
    let total = hits.len();
    Ok(json!({"frame": k.frame, "run": k.run, "tick": k.tick, "revision": k.revision, "x": x, "y": y, "tolerance": tol,
        "candidates": hits.iter().take(limit).map(|(dist, a)| json!({"id": a.0.to_string(), "lineage": a.4, "distance_px": dist, "x": a.3[0], "y": a.3[1], "radius_px": a.2})).collect::<Vec<_>>(),
        "total": total, "truncated": total > limit, "scope": "the agents this view drew in that frame (a layer's `where` leaves some out)"}))
}

/// A handshake with an `Origin` header came from a web page: browsers let any site open a socket
/// to localhost, so such a page could drive the engine (DNS rebinding and cross-site requests).
/// Programs (the CLI, tests, MCP) send no `Origin` and are accepted. A page is accepted only with
/// a viewer, from the server's own origin, with the server's own `Host`, and with the token in
/// `Sec-WebSocket-Protocol` (`emerge.k.<token>`); everything else gets 403.
#[allow(clippy::result_large_err)]
fn admit(req: &Request, mut resp: Response, viewer: Option<&Viewer>, port: u16) -> Result<Response, ErrorResponse> {
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
    let Some(origin) = header("origin") else { return Ok(resp) };
    let own = |v: &str| v == format!("127.0.0.1:{port}") || v == format!("localhost:{port}");
    let offered = header("sec-websocket-protocol").unwrap_or("").split(',').map(str::trim).find_map(|p| p.strip_prefix("emerge.k."));
    let admitted = match (viewer, offered) {
        (Some(v), Some(token)) => origin.strip_prefix("http://").is_some_and(own) && header("host").is_some_and(own) && same(token.as_bytes(), v.token.as_bytes()),
        _ => false,
    };
    if !admitted {
        let mut refusal = ErrorResponse::new(Some("requests from web pages are refused".into()));
        *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
        return Err(refusal);
    }
    // A browser closes a socket whose server does not name the protocol it offered.
    let chosen = format!("emerge.k.{}", offered.unwrap_or(""));
    resp.headers_mut().insert("sec-websocket-protocol", chosen.parse().expect("a token is hex"));
    Ok(resp)
}

/// Equal bytes, in time that does not depend on where they first differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

const PAGE: &str = include_str!("viewer/index.html");
const HELP_PAGE: &str = include_str!("../../../docs/index.html");
const HELP_CSS: &str = include_str!("../../../docs/style.css");
const HELP_JS: &str = include_str!("../../../docs/help.js");

/// With a viewer, a request that is not a WebSocket upgrade is plain HTTP: `GET /` gets the page,
/// `/help` gets the language help docs, anything else 404. Returns true if it answered (the connection is then done).
fn answer_http(stream: &mut TcpStream) -> bool {
    let mut head = [0u8; 4096];
    let mut seen = 0;
    // Look at the request head without taking it, so a WebSocket handshake can still read it.
    for _ in 0..50 {
        seen = stream.peek(&mut head).unwrap_or(0);
        if seen == 0 || head[..seen].windows(4).any(|w| w == b"\r\n\r\n") || seen == head.len() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let text = String::from_utf8_lossy(&head[..seen]).to_ascii_lowercase();
    if text.contains("upgrade: websocket") {
        return false;
    }
    let _ = stream.read(&mut head[..seen]);
    let reply = if text.starts_with("get / ") {
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{PAGE}", PAGE.len())
    } else if text.starts_with("get /help ") || text.starts_with("get /help/ ") || text.starts_with("get /docs ") || text.starts_with("get /docs/ ") {
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{HELP_PAGE}", HELP_PAGE.len())
    } else if text.starts_with("get /style.css ") {
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/css; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{HELP_CSS}", HELP_CSS.len())
    } else if text.starts_with("get /help.js ") {
        format!("HTTP/1.1 200 OK\r\nContent-Type: application/javascript; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{HELP_JS}", HELP_JS.len())
    } else {
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
    };
    let _ = stream.write_all(reply.as_bytes());
    true
}

/// With a viewer, the page can stop or restart the server: `server.stop` ends the process with
/// status 0, `server.restart` with `RESTART_EXIT`. Worlds in memory are lost. The calls exist only
/// with `--viewer`; the answer carries the call's `id`.
fn shutdown_call(text: &str) -> Option<(Value, i32)> {
    let v: Value = serde_json::from_str(text).ok()?;
    let code = match v.get("method")?.as_str()? {
        "server.stop" => 0,
        "server.restart" => RESTART_EXIT,
        _ => return None,
    };
    Some((v.get("id").cloned().unwrap_or(Value::Null), code))
}

fn connection(mut stream: TcpStream, to_engine: mpsc::Sender<Call>, viewer: Option<&Viewer>, port: u16) {
    if viewer.is_some() && answer_http(&mut stream) {
        return;
    }
    #[allow(clippy::result_large_err)] // tungstenite's handshake callback returns its own error type
    let Ok(mut ws) = tungstenite::accept_hdr(stream, |req: &Request, resp: Response| admit(req, resp, viewer, port)) else { return };
    // Reads time out, so frames pushed by the engine go out between messages.
    let _ = ws.get_ref().set_read_timeout(Some(Duration::from_millis(20)));
    let (push, frames) = mpsc::channel();
    let conn = Arc::new(Conn { push, ready: AtomicBool::new(true) });
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Some((id, code)) = viewer.and(shutdown_call(&t)) {
                    let _ = ws.send(Message::Text(json!({"jsonrpc": "2.0", "id": id, "result": {"ok": true}}).to_string().into()));
                    std::process::exit(code);
                }
                let (reply_to, reply) = mpsc::channel();
                if to_engine.send((t.to_string(), reply_to, conn.clone())).is_err() {
                    return;
                }
                if let Ok(Some(answer)) = reply.recv() {
                    if ws.send(Message::Text(answer.into())).is_err() {
                        return;
                    }
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {} // pings are answered by tungstenite; a client sends no binary frames
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
        while let Ok(frame) = frames.try_recv() {
            if ws.send(Message::Binary(frame.into())).is_err() {
                return;
            }
            conn.ready.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_two_server_calls_end_the_process() {
        assert_eq!(shutdown_call(r#"{"jsonrpc":"2.0","id":7,"method":"server.stop"}"#), Some((json!(7), 0)));
        assert_eq!(shutdown_call(r#"{"id":"a","method":"server.restart"}"#), Some((json!("a"), RESTART_EXIT)));
        assert_eq!(shutdown_call(r#"{"id":1,"method":"world.list"}"#), None);
        assert_eq!(shutdown_call("not json"), None);
    }
}
