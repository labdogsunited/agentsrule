//! `emerge-gpu m1 DISH LINEAGES TICKS SEED EVERY`: runs the CPU runner in contract mode, and at
//! every EVERY-th tick compares the GPU's Set phase with the CPU's (proposals/gpu-runtime.md, M1).

use emerge_gpu::{m1, m2, m3};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("m1") => m1::run(&args[1..]),
        Some("m2") => m2::run(&args[1..]),
        Some("m3") => m3::run(&args[1..]),
        Some("m3r") => m3::run_restructure_cmd(&args[1..]),
        _ => eprintln!("usage: emerge-gpu m1 DISH LINEAGES TICKS SEED EVERY"),
    }
}
