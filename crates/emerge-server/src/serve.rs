//! The API over WebSocket on localhost (spec §12.2): one text frame per JSON-RPC message. One
//! engine thread owns every run (a GPU world cannot move between threads); each connection has a
//! thread that passes its messages to the engine and the replies back. Between calls the engine
//! ticks the worlds that are running, and pushes the frames that viewers subscribed to
//! (`view.subscribe`, `proposals/live-viewer.md`), which only this server offers.

use crate::api::Engine;
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
    every: u64,
    min_gap: Duration,
    cell: u32,
    last_tick: Option<u64>,
    last_sent: Instant,
}

/// The browser viewer (`proposals/live-viewer.md`): the server answers `GET /` with its page, and
/// accepts a web page's socket only from its own origin, with this process's token.
pub struct Viewer {
    /// 128 random bits, in hex.
    pub token: String,
}

impl Viewer {
    /// A viewer with a fresh token from the operating system.
    pub fn new() -> std::io::Result<Viewer> {
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
    let mut engine = journal_dir.map_or_else(Engine::new, Engine::with_journals);
    let (mut subs, mut next_sub) = (Vec::<Sub>::new(), 0u64);
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
                let answer = subscription(&message, &engine, &mut subs, &mut next_sub, conn).unwrap_or_else(|| engine.handle(&message));
                let _ = reply.send(answer);
            }
            Err(true) => return, // every sender is gone
            // Nothing to answer: tick the running worlds for a moment, then look again.
            Err(false) if engine.busy() => engine.background(0.02),
            Err(false) => {}
        }
        push_frames(&engine, &mut subs);
    }
}

/// `view.subscribe` and `view.unsubscribe`, which need the connection; `None` for any other call.
fn subscription(message: &str, engine: &Engine, subs: &mut Vec<Sub>, next_sub: &mut u64, conn: Arc<Conn>) -> Option<Option<String>> {
    let req: Value = serde_json::from_str(message).ok()?;
    let method = req["method"].as_str()?;
    if method != "view.subscribe" && method != "view.unsubscribe" {
        return None;
    }
    let p = &req["params"];
    let result = if method == "view.unsubscribe" {
        let id = p["subscription"].as_u64();
        let before = subs.len();
        subs.retain(|s| Some(s.id) != id);
        if subs.len() < before { Ok(json!({"unsubscribed": id})) } else { Err("no such subscription".to_string()) }
    } else {
        (|| {
            let world = p["world"].as_str().ok_or("`world` must be a string")?;
            let run = engine.run(world).ok_or(format!("no world `{world}`"))?;
            let view = crate::picture::pick_view(run, p["views"].as_str(), p["view"].as_str())?;
            let num = |k: &str, d: u64| p[k].as_u64().unwrap_or(d);
            *next_sub += 1;
            let fps = num("max_fps", 10).clamp(1, 60);
            subs.push(Sub { id: *next_sub, conn, world: world.to_string(), view, every: num("every", 1).max(1), min_gap: Duration::from_secs_f64(1.0 / fps as f64), cell: num("cell", 0) as u32, last_tick: None, last_sent: Instant::now() });
            Ok(json!({"subscription": *next_sub}))
        })()
    };
    let reply = match result {
        Ok(v) => json!({"jsonrpc": "2.0", "id": req["id"], "result": v}),
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
        let due = s.last_tick.is_none_or(|t| run.tick() >= t + s.every) && s.conn.ready.load(Ordering::Acquire) && (s.last_tick.is_none() || s.last_sent.elapsed() >= s.min_gap);
        if !due {
            return true;
        }
        let Ok((png, w, h)) = crate::picture::png(run, &s.view, s.cell) else { return false };
        let header = json!({"subscription": s.id, "world": s.world, "view": s.view.name, "tick": run.tick(), "revision": run.revision(), "width": w, "height": h, "format": "png"}).to_string();
        let mut frame = (header.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(header.as_bytes());
        frame.extend_from_slice(&png);
        s.conn.ready.store(false, Ordering::Release);
        (s.last_tick, s.last_sent) = (Some(run.tick()), Instant::now());
        s.conn.push.send(frame).is_ok() // a closed connection ends its subscriptions
    });
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

/// With a viewer, a request that is not a WebSocket upgrade is plain HTTP: `GET /` gets the page,
/// anything else 404. Returns true if it answered (the connection is then done).
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
    } else {
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
    };
    let _ = stream.write_all(reply.as_bytes());
    true
}

fn connection(mut stream: TcpStream, to_engine: mpsc::Sender<Call>, viewer: Option<&Viewer>, port: u16) {
    if viewer.is_some() && answer_http(&mut stream) {
        return;
    }
    let Ok(mut ws) = tungstenite::accept_hdr(stream, |req: &Request, resp: Response| admit(req, resp, viewer, port)) else { return };
    // Reads time out, so frames pushed by the engine go out between messages.
    let _ = ws.get_ref().set_read_timeout(Some(Duration::from_millis(20)));
    let (push, frames) = mpsc::channel();
    let conn = Arc::new(Conn { push, ready: AtomicBool::new(true) });
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
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
