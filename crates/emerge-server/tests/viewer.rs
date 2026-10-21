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

const DISH: &str = include_str!("../../../examples/dish.em");
const VIEWS: &str = include_str!("../../../examples/views/dish.em");

/// The next binary frame: its JSON header and its PNG bytes.
fn frame(ws: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>) -> (serde_json::Value, Vec<u8>) {
    loop {
        if let Message::Binary(b) = ws.read().unwrap() {
            let n = u32::from_le_bytes(b[..4].try_into().unwrap()) as usize;
            return (serde_json::from_slice(&b[4..4 + n]).unwrap(), b[4 + n..].to_vec());
        }
    }
}

#[test]
fn a_live_frame_is_the_same_picture_as_render() {
    use emerge_server::journal::Journal;
    use emerge_server::run::{Run, Spec};
    use serde_json::json;
    let (port, _) = start();
    let (mut ws, _) = connect(format!("ws://127.0.0.1:{port}/")).unwrap();
    let mut call = |ws: &mut tungstenite::WebSocket<_>, id: u64, method: &str, params: serde_json::Value| {
        ws.send(Message::Text(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string().into())).unwrap();
        loop {
            if let Message::Text(t) = ws.read().unwrap() {
                let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                assert!(v.get("error").is_none(), "{method}: {v}");
                return v["result"].clone();
            }
        }
    };
    let source = DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 48, h: 48)").replace("count: 2_000;", "count: 150;");
    let w = call(&mut ws, 1, "world.create", json!({"source": source, "lineages": ["Drifter", "Builder"], "seed": 4}))["world"].clone();
    let sub = call(&mut ws, 2, "view.subscribe", json!({"world": w, "views": VIEWS, "view": "Lineages", "every": 10, "cell": 4}));
    assert_eq!(sub["subscription"], 1);
    let (h0, _) = frame(&mut ws);
    assert_eq!((h0["tick"].as_u64(), h0["view"].as_str(), h0["format"].as_str()), (Some(0), Some("Lineages"), Some("png")));
    call(&mut ws, 3, "run.step", json!({"world": w, "ticks": 25}));
    let (h, png) = frame(&mut ws);
    assert_eq!(h["tick"], 25, "{h}");
    // The same run, drawn the way `emerge-server render` draws it.
    let mut run = Run::create(Spec { source, lineages: vec!["Drifter".into(), "Builder".into()], seed: 4, mutation: true, contract: false, gpu: false }, Journal::memory()).unwrap();
    run.step(25);
    let view = emerge_server::picture::pick_view(&run, Some(VIEWS), Some("Lineages")).unwrap();
    let (expected, w_px, h_px) = emerge_server::picture::png(&run, &view, 4).unwrap();
    assert_eq!((h["width"].as_u64(), h["height"].as_u64()), (Some(w_px as u64), Some(h_px as u64)));
    assert!(png == expected, "the live frame differs from render's picture");
    // A bad view is refused, and unsubscribing stops the frames.
    ws.send(Message::Text(json!({"jsonrpc": "2.0", "id": 4, "method": "view.subscribe", "params": {"world": w, "view": "Nope"}}).to_string().into())).unwrap();
    let refused = loop {
        if let Message::Text(t) = ws.read().unwrap() {
            break serde_json::from_str::<serde_json::Value>(&t).unwrap();
        }
    };
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
    assert_eq!(call(&mut ws, 5, "view.unsubscribe", json!({"subscription": 1}))["unsubscribed"], 1);
}
