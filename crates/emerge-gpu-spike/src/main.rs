//! GPU spike (archive/gpu-spike.md). `emerge-gpu-spike a1 | a2 | census | b`.
//! Kept as it was measured; style lints are not chased here.
#![allow(clippy::type_complexity, clippy::needless_range_loop)]

mod a1;
mod a2;
mod a3;
mod b;
mod bin_cost;
mod census;
mod gpu;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("a1") => a1::run(),
        Some("a2") => a2::run(),
        Some("a3") => a3::run(),
        Some("bin") => {
            let a: Vec<usize> = std::env::args().skip(2).filter_map(|v| v.parse().ok()).collect();
            bin_cost::run(if a.len() == 2 { Some((a[0], a[1])) } else { None })
        }
        Some("b") => b::run(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("census") => census::run(&std::env::args().skip(2).collect::<Vec<_>>()),
        _ => eprintln!("usage: emerge-gpu-spike a1 | a2 | census | b"),
    }
}
