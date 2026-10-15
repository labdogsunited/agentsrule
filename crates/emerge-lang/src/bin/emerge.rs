//! `emerge fmt <file>`: prints the canonical form of a file. With `--check`, prints nothing
//! and fails if the file is not already canonical.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (check, files): (Vec<&String>, Vec<&String>) = args.iter().skip(1).partition(|a| *a == "--check");
    if args.first().map(String::as_str) != Some("fmt") || files.len() != 1 {
        eprintln!("usage: emerge fmt [--check] <file.em>");
        return ExitCode::from(2);
    }
    let src = match std::fs::read_to_string(files[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", files[0]);
            return ExitCode::from(2);
        }
    };
    let file = match emerge_lang::parse(&src) {
        Ok(f) => f,
        Err(d) => {
            eprintln!("{}: {d}", files[0]);
            return ExitCode::FAILURE;
        }
    };
    let out = emerge_lang::format(&file);
    if check.is_empty() {
        print!("{out}");
        ExitCode::SUCCESS
    } else if out == src {
        ExitCode::SUCCESS
    } else {
        eprintln!("{}: not in canonical form (run `emerge fmt`)", files[0]);
        ExitCode::FAILURE
    }
}
