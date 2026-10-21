//! The API over WebSocket on localhost (spec §12.2): one text frame per JSON-RPC message. One
//! engine thread owns every run (a GPU world cannot move between threads); each connection has a
//! thread that passes its messages to the engine and the replies back. Between calls the engine
//! ticks the worlds that are running.

use crate::api::Engine;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::Message;

type Call = (String, mpsc::Sender<Option<String>>);

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
    loop {
        let next = if engine.busy() { calls.try_recv().ok() } else { calls.recv().ok() };
        match next {
            Some((message, reply)) => {
                let _ = reply.send(engine.handle(&message));
            }
            // Nothing to answer: tick the running worlds for a moment, then look again.
            None if engine.busy() => engine.background(0.02),
            None => return, // every sender is gone
        }
    }
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
    loop {
        let text = match ws.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Close(_)) | Err(_) => return,
            Ok(_) => continue, // pings are answered by tungstenite; binary frames are not used yet
        };
        let (reply_to, reply) = mpsc::channel();
        if to_engine.send((text, reply_to)).is_err() {
            return;
        }
        if let Ok(Some(answer)) = reply.recv() {
            if ws.send(Message::Text(answer.into())).is_err() {
                return;
            }
        }
    }
}
