//! `emerge-server exit2 DISH [--gpu] [--ticks N] [--journal FILE]`: exit test 2 (40 swaps, the ledger
//! checked every tick).
//! `emerge-server replay JOURNAL`: rebuilds a run from its journal and checks it.
//! `emerge-server serve [--port N] [--journal-dir DIR] [--viewer]`: the JSON-RPC API over WebSocket on 127.0.0.1
//! (default port 7878); with `--journal-dir`, every world's journal is written there as `<world>.jsonl`.
//! With `--viewer`, the page can stop the server (exit 0) or restart it (exit 75); see `scripts/agentsrule`.
//! `emerge-server mcp [--journal-dir DIR]`: the MCP adapter over stdio (for Claude and other MCP clients).
//! `emerge-server render (DISH --lineages A[,B] | --journal FILE) [--tick T] [--seed S] [--mutation off]
//! [--contract] [--gpu] [--made S --window W] [--views FILE] [--view NAME] [--chart default|NAME]... [--cell PX] [--out PNG]`: draws
//! a view of the run at tick T (a journal is replayed, a dish is run from its seed), so any experiment
//! can be drawn; with `--made`, each agent's `made` is the µ of S its reactions made in ticks T−W to T.

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
        Some("render") if args.len() >= 2 => match render(&args[1..]) {
            Ok(path) => {
                println!("{path}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some("mcp") => {
            emerge_server::mcp::stdio(flag(&args, "--journal-dir").map(Into::into));
            ExitCode::SUCCESS
        }
        Some("serve") => {
            let port = flag(&args, "--port").unwrap_or("7878");
            let listener = std::net::TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind the port");
            eprintln!("emerge-server: JSON-RPC over WebSocket on ws://{}", listener.local_addr().unwrap());
            let viewer = args.iter().any(|a| a == "--viewer").then(|| emerge_server::serve::Viewer::new().expect("draw a token"));
            if let Some(v) = &viewer {
                eprintln!("emerge-server: viewer at {}", v.url(listener.local_addr().unwrap().port()));
            }
            emerge_server::serve::serve_with(listener, flag(&args, "--journal-dir").map(Into::into), viewer);
            ExitCode::SUCCESS
        }
        Some("mov") if args.len() >= 2 => {
            let fps = flag(&args, "--fps").map_or(10, |f| f.parse().expect("--fps N"));
            match emerge_server::record::mov(args[1].as_ref(), fps) {
                Ok(path) => {
                    println!("{}", path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
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
            eprintln!("usage: emerge-server exit2 DISH [--gpu] [--ticks N] [--journal FILE]\n       emerge-server replay JOURNAL\n       emerge-server mov RECORDING_DIR [--fps N]\n       emerge-server serve [--port N] [--journal-dir DIR] [--viewer]\n       emerge-server mcp [--journal-dir DIR]\n       emerge-server render (DISH --lineages A[,B] | --journal FILE) [--tick T] [--made S --window W] [--views FILE] [--view NAME] [--chart default|NAME]... [--out PNG]");
            ExitCode::FAILURE
        }
    }
}

/// `render`: the run at the tick asked for, one of its views, a PNG.
fn render(args: &[String]) -> Result<String, String> {
    use emerge_server::run::{Run, Spec};
    let tick: u64 = flag(args, "--tick").map_or(Ok(0), |t| t.parse().map_err(|_| "--tick N"))?;
    let mut run = match flag(args, "--journal") {
        Some(j) => replay(&std::fs::read_to_string(j).map_err(|e| format!("{j}: {e}"))?)?,
        None => {
            let dish = &args[0];
            let source = std::fs::read_to_string(dish).map_err(|e| format!("{dish}: {e}"))?;
            let lineages = flag(args, "--lineages").ok_or("--lineages A[,B]")?.split(',').filter(|l| !l.is_empty()).map(String::from).collect();
            let seed = flag(args, "--seed").map_or(Ok(1), |s| s.parse().map_err(|_| "--seed N"))?;
            let spec = Spec { source, lineages, seed, mutation: flag(args, "--mutation") != Some("off"), contract: args.iter().any(|a| a == "--contract"), gpu: args.iter().any(|a| a == "--gpu") };
            Run::create(spec, Journal::memory()).map_err(|e| e.to_string())?
        }
    };
    if tick < run.tick() {
        return Err(format!("the journal ends at tick {}; ask for that tick or a later one", run.tick()));
    }
    // `made` counts over an explicit window: `--made S --window W` counts S in the last W ticks.
    match (flag(args, "--made"), flag(args, "--window")) {
        (Some(s), Some(w)) => {
            let w: u64 = w.parse().map_err(|_| "--window W")?;
            let from = tick.checked_sub(w).filter(|&f| f >= run.tick()).ok_or(format!("--window {w} reaches before tick {}", run.tick()))?;
            run.step(from - run.tick());
            run.count_made(s)?;
        }
        (None, None) => {}
        _ => return Err("--made S and --window W go together".into()),
    }
    run.step(tick - run.tick());
    // The views: the run's own, then those of `--views`, checked against the run's world.
    let extra = match flag(args, "--views") {
        Some(v) => Some(std::fs::read_to_string(v).map_err(|e| format!("{v}: {e}"))?),
        None => None,
    };
    let view = emerge_server::picture::pick_view(&run, extra.as_deref(), flag(args, "--view"))?;
    let cell_px = flag(args, "--cell").map_or(Ok(0), |c| c.parse().map_err(|_| "--cell PX"))?;
    let names: Vec<String> = args.windows(2).filter(|w| w[0] == "--chart").map(|w| w[1].clone()).collect();
    let charts = emerge_server::picture::pick_charts(&run, extra.as_deref(), &names)?;
    let (png, _, _) = emerge_server::picture::png_with(&run, &view, cell_px, &charts)?;
    let out = flag(args, "--out").map(String::from).unwrap_or(format!("{}-tick{}.png", view.name, run.tick()));
    std::fs::write(&out, png).map_err(|e| format!("{out}: {e}"))?;
    Ok(out)
}
