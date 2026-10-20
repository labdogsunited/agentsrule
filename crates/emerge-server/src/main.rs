//! `emerge-server exit2 DISH [--gpu] [--ticks N] [--journal FILE]`: exit test 2 (40 swaps, the ledger
//! checked every tick).
//! `emerge-server replay JOURNAL`: rebuilds a run from its journal and checks it.
//! `emerge-server serve [--port N]`: the JSON-RPC API over WebSocket on 127.0.0.1 (default port 7878).

use emerge_server::journal::{replay, Journal};
use std::process::ExitCode;

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("exit2") if args.len() >= 2 => {
            let source = std::fs::read_to_string(&args[1]).expect("read the dish");
            let ticks = flag(&args, "--ticks").map_or(100_000, |t| t.parse().expect("--ticks N"));
            let journal = flag(&args, "--journal").map_or_else(Journal::memory, |p| Journal::file(p.as_ref()).expect("create the journal"));
            if emerge_server::exit2::run(&source, args.iter().any(|a| a == "--gpu"), ticks, journal) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Some("serve") => {
            let port = flag(&args, "--port").unwrap_or("7878");
            let listener = std::net::TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind the port");
            eprintln!("emerge-server: JSON-RPC over WebSocket on ws://{}", listener.local_addr().unwrap());
            emerge_server::serve::serve(listener);
            ExitCode::SUCCESS
        }
        Some("replay") if args.len() >= 2 => {
            let text = std::fs::read_to_string(&args[1]).expect("read the journal");
            match replay(&text) {
                Ok(run) => {
                    println!("replayed to tick {}, revision {}, digest {:016x}: matches the journal", run.tick(), run.revision(), run.digest());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: emerge-server exit2 DISH [--gpu] [--ticks N] [--journal FILE]\n       emerge-server replay JOURNAL\n       emerge-server serve [--port N]");
            ExitCode::FAILURE
        }
    }
}
