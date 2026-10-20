//! The API over WebSocket on localhost (spec §12.2): one text frame per JSON-RPC message. One
//! engine thread owns every run (a GPU world cannot move between threads); each connection has a
//! thread that passes its messages to the engine and the replies back. Between calls the engine
//! ticks the worlds that are running.

use crate::api::Engine;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::Message;

type Call = (String, mpsc::Sender<Option<String>>);

/// Serves on `listener` until the process ends. Journals go to `journal_dir`, if given.
pub fn serve(listener: TcpListener, journal_dir: Option<std::path::PathBuf>) {
    let (to_engine, calls) = mpsc::channel::<Call>();
    std::thread::spawn(move || engine(calls, journal_dir));
    for stream in listener.incoming().flatten() {
        let to_engine = to_engine.clone();
        std::thread::spawn(move || connection(stream, to_engine));
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
/// No page is allowed yet; programs (the CLI, tests, MCP) send no `Origin`.
#[allow(clippy::result_large_err)]
fn no_web_pages(req: &Request, resp: Response) -> Result<Response, ErrorResponse> {
    if req.headers().contains_key("origin") {
        let mut refusal = ErrorResponse::new(Some("requests from web pages are refused".into()));
        *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
        return Err(refusal);
    }
    Ok(resp)
}

fn connection(stream: TcpStream, to_engine: mpsc::Sender<Call>) {
    let Ok(mut ws) = tungstenite::accept_hdr(stream, no_web_pages) else { return };
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
