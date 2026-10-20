//! GPU spike (proposals/gpu-spike.md). `emerge-gpu-spike a1 | a2 | census | b`.

mod a1;
mod a2;
mod gpu;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("a1") => a1::run(),
        Some("a2") => a2::run(),
        _ => eprintln!("usage: emerge-gpu-spike a1 | a2 | census | b"),
    }
}
