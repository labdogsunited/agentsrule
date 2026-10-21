//! The browser viewer's connection (`proposals/live-viewer.md` L1): the server serves its page, and
//! accepts a web page's socket only from its own origin and host with its token. Programs (no
//! `Origin`) connect as before. CPU only.

use emerge_server::serve::{serve_with, Viewer};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use tungstenite::client::IntoClientRequest;
use tungstenite::{connect, Message};

/// A server with a viewer; its port and token.
fn start() -> (u16, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let viewer = Viewer::new().unwrap();
    let token = viewer.token.clone();
    assert_eq!(token.len(), 32);
    std::thread::spawn(move || serve_with(listener, None, Some(viewer)));
    (port, token)
}

/// A handshake with these headers: the HTTP status it got.
fn status(port: u16, headers: &[(&str, String)]) -> u16 {
    let mut req = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
    for (k, v) in headers {
        req.headers_mut().insert(tungstenite::http::HeaderName::from_bytes(k.as_bytes()).unwrap(), v.parse().unwrap());
    }
    match connect(req) {
        Ok((mut ws, resp)) => {
            // Admitted: the API answers on it.
            ws.send(Message::Text(r#"{"jsonrpc":"2.0","id":1,"method":"docs.query","params":{}}"#.into())).unwrap();
            assert!(matches!(ws.read().unwrap(), Message::Text(_)));
            resp.status().as_u16()
        }
        Err(tungstenite::Error::Http(resp)) => resp.status().as_u16(),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn only_the_servers_own_page_with_its_token_gets_in() {
    let (port, token) = start();
    let own = format!("http://127.0.0.1:{port}");
    let k = |t: &str| ("Sec-WebSocket-Protocol", format!("emerge.k.{t}"));
    assert_eq!(status(port, &[("Origin", own.clone()), k(&token)]), 101, "the viewer");
    assert_eq!(status(port, &[("Origin", format!("http://localhost:{port}")), k(&token)]), 101, "the viewer, as localhost");
    assert_eq!(status(port, &[]), 101, "a program, with no Origin");
    assert_eq!(status(port, &[("Origin", own.clone())]), 403, "no token");
    assert_eq!(status(port, &[("Origin", own.clone()), k("0123456789abcdef0123456789abcdef")]), 403, "a wrong token");
    assert_eq!(status(port, &[("Origin", "https://evil.example".into()), k(&token)]), 403, "another origin, even with the token");
    assert_eq!(status(port, &[("Origin", format!("http://127.0.0.1:{}", port + 1)), k(&token)]), 403, "another port");
    assert_eq!(status(port, &[("Origin", own), ("Host", "rebound.example".into()), k(&token)]), 403, "a host that is not the server (DNS rebinding)");
}

#[test]
fn the_server_serves_its_page() {
    let (port, _) = start();
    let get = |path: &str| {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").unwrap();
        let mut reply = String::new();
        s.read_to_string(&mut reply).unwrap();
        reply
    };
    let page = get("/");
    assert!(page.starts_with("HTTP/1.1 200") && page.contains("<title>Emerge viewer</title>"), "{page}");
    assert!(get("/journal.jsonl").starts_with("HTTP/1.1 404"));
}
