//! The run journal (spec §13.3): an append-only file of JSON lines. A header with everything the
//! run was made from, one line per swap (committed or rejected) and an end line with the state's
//! digest. `replay` rebuilds a run from a journal alone and checks it on the way.

use crate::run::{Run, Spec};
use emerge_ref::swap::Rejected;
use serde_json::{json, Value};
use std::io::Write;

/// FNV-1a, 64 bits: a hash with no dependencies, for sources and state digests.
pub fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// The lines so far, and the file they are also written to (if any).
#[derive(Default)]
pub struct Journal {
    pub lines: Vec<Value>,
    file: Option<std::fs::File>,
}

impl Journal {
    /// A journal kept in memory only.
    pub fn memory() -> Journal {
        Journal::default()
    }

    /// A journal written to `path` as well. The file must not exist yet: a journal never
    /// overwrites anything.
    pub fn file(path: &std::path::Path) -> std::io::Result<Journal> {
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Journal { lines: Vec::new(), file: Some(file) })
    }

    fn push(&mut self, line: Value) {
        if let Some(f) = &mut self.file {
            // A journal that cannot be written is a lost record: say so, and keep running.
            if let Err(e) = writeln!(f, "{line}").and_then(|_| f.flush()) {
                eprintln!("journal: {e}");
            }
        }
        self.lines.push(line);
    }

    pub fn header(&mut self, spec: &Spec, device: &str) {
        self.push(json!({
            "kind": "header", "engine": concat!("emerge-server ", env!("CARGO_PKG_VERSION")),
            "backend": device, "contract": spec.contract, "mutation": spec.mutation, "seed": spec.seed,
            "lineages": spec.lineages, "source": spec.source, "source_hash": format!("{:016x}", fnv(&spec.source)),
        }));
    }

    pub fn swap(&mut self, tick: u64, base: u64, patch: &str, reason: &str, result: Result<u64, &Rejected>) {
        let mut line = json!({"kind": "swap", "tick": tick, "base_revision": base, "patch": patch, "reason": reason});
        match result {
            Ok(hash) => {
                line["result"] = json!("committed");
                line["source_hash"] = json!(format!("{hash:016x}"));
            }
            Err(e) => {
                line["result"] = json!("rejected");
                line["why"] = json!(e.to_string());
            }
        }
        self.push(line);
    }

    pub fn end(&mut self, tick: u64, digest: u64) {
        self.push(json!({"kind": "end", "tick": tick, "digest": format!("{digest:016x}")}));
    }

    /// The journal as text, one line per entry.
    pub fn text(&self) -> String {
        self.lines.iter().map(|l| format!("{l}\n")).collect()
    }
}

/// Rebuilds a run from journal `text` alone: the header's world, every swap at its tick (each must
/// end as it did: committed with the same source hash, or rejected), and the ticks up to the end
/// line, whose digest must match. Without an end line, it stops after the last swap.
pub fn replay(text: &str) -> Result<Run, String> {
    let lines: Vec<Value> = text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).map_err(|e| format!("journal: {e}"))).collect::<Result<_, _>>()?;
    let h = lines.first().filter(|h| h["kind"] == "header").ok_or("journal: no header")?;
    let spec = Spec {
        source: h["source"].as_str().ok_or("journal: no source")?.to_string(),
        lineages: h["lineages"].as_array().ok_or("journal: no lineages")?.iter().filter_map(|l| l.as_str().map(String::from)).collect(),
        seed: h["seed"].as_u64().ok_or("journal: no seed")?,
        mutation: h["mutation"].as_bool().ok_or("journal: no mutation flag")?,
        contract: h["contract"].as_bool().ok_or("journal: no contract flag")?,
        gpu: h["backend"].as_str().is_some_and(|b| b.starts_with("gpu")),
    };
    let mut run = Run::create(spec, Journal::memory()).map_err(|e| format!("journal: the header's world does not compile: {e}"))?;
    if format!("{:016x}", fnv(&run.spec.source)) != h["source_hash"] {
        return Err("journal: the header's source does not match its hash".into());
    }
    for (k, line) in lines.iter().enumerate().skip(1) {
        let tick = line["tick"].as_u64().ok_or(format!("journal line {}: no tick", k + 1))?;
        if tick < run.tick() {
            return Err(format!("journal line {}: tick {tick} is in the past", k + 1));
        }
        run.step(tick - run.tick());
        if run.tick() != tick {
            return Err(format!("journal line {}: the world died out at tick {}", k + 1, run.tick()));
        }
        match line["kind"].as_str() {
            Some("swap") => {
                let (patch, reason) = (line["patch"].as_str().unwrap_or(""), line["reason"].as_str().unwrap_or(""));
                let base = line["base_revision"].as_u64().ok_or(format!("journal line {}: no base revision", k + 1))?;
                let got = run.hotswap(patch, base, reason);
                match (line["result"].as_str(), &got) {
                    (Some("committed"), Ok(r)) if r["source_hash"] == line["source_hash"] => {}
                    (Some("rejected"), Err(_)) => {}
                    (want, _) => return Err(format!("journal line {}: the swap was {want:?} but replays as {got:?}", k + 1)),
                }
            }
            Some("end") => {
                let digest = format!("{:016x}", run.digest());
                if line["digest"] != digest {
                    return Err(format!("journal line {}: the state at tick {tick} differs (digest {digest}, journal {})", k + 1, line["digest"]));
                }
            }
            other => return Err(format!("journal line {}: unknown kind {other:?}", k + 1)),
        }
    }
    Ok(run)
}
