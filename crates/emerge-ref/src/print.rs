//! Prints a genome in `.em` syntax (`language.md` §8.2), for reading evolved genomes.
//!
//! Signals are named `s0`, `s1`, … and modules `m0`, `m1`, … by their position, and genes
//! have no labels. Generated reactions are named by their formula, so the result is meant
//! to be read, not parsed back.

use crate::chem::{Mu, U};
use crate::mind::{Curve, Gene, Genome, Input, Sense};
use crate::World;

/// `field` is the name of the conserved field, for `gradient` and `probe`.
pub fn genome(w: &World, g: &Genome, field: &str) -> String {
    let mut out = String::from("genome {\n");
    for (s, &k) in g.keep.iter().enumerate() {
        if k != 0.0 {
            out += &format!("    signal s{s} keep {k:?};\n");
        }
    }
    for (m, &on) in g.modules.iter().enumerate() {
        out += &format!("    {}module m{m} {{\n", if on { "" } else { "dormant " });
        for slot in g.slots.iter().filter(|x| x.module == m) {
            let text = gene(w, &slot.gene, field);
            out += &format!("        {}{text};\n", if slot.expressed { "" } else { "dormant " });
        }
        out += "    }\n";
    }
    out + "}\n"
}

/// One gene in `.em` syntax, without its `dormant` marker or the final `;`.
pub fn gene(w: &World, gene: &Gene, field: &str) -> String {
    let substance = |s: usize| w.chem.substances[s].name.as_str();
    match gene {
        Gene::Sense { sense, out, out2 } => {
            let call = match *sense {
                Sense::Inventory(s) => format!("inventory(s: {})", substance(s)),
                Sense::Cell(s) => format!("cell(s: {})", substance(s)),
                Sense::Gradient(s) => format!("gradient(q: {field}.{})", substance(s)),
                Sense::Probe { s, angle_deg, dist } => {
                    format!("probe(q: {field}.{}, angle_deg: {angle_deg:?}, dist: {dist:?})", substance(s))
                }
                Sense::Crowd { radius } => format!("crowd(radius: {radius:?})"),
                Sense::Neighbour { s, rank } => format!("neighbour(s: {}, rank: {rank})", substance(s)),
                Sense::Clock { period } => format!("clock(period: {period})"),
                Sense::Delta(s) => format!("delta(s: {})", substance(s)),
            };
            let outs = std::iter::once(out).chain(out2).map(|s| format!("s{s}")).collect::<Vec<_>>().join(", ");
            format!("sense {call} -> {outs}")
        }
        Gene::Weigh { bias, curve, inputs, out } => {
            let mut params = Vec::new();
            if *bias != 0.0 {
                params.push(format!("bias: {bias:?}"));
            }
            if *curve != Curve::Linear {
                params.push(format!("curve: {}", format!("{curve:?}").to_lowercase()));
            }
            let ins = inputs
                .iter()
                .map(|&(wt, s)| if wt == 1.0 { format!("s{s}") } else { format!("{wt:?} * s{s}") })
                .collect::<Vec<_>>()
                .join(", ");
            format!("regulate weigh({}) <- [{ins}] -> s{out}", params.join(", "))
        }
        Gene::Product { a, b, out } => format!("regulate product() <- [s{a}, s{b}] -> s{out}"),
        Gene::React { reaction, rate, speed } => {
            format!("reaction {} rate {}{}", w.chem.reactions[*reaction].name, amount(*rate), source(*speed, 0.0))
        }
        Gene::Move { speed, src, threshold, .. } => format!("action move(speed: {speed:?}){}", source(*src, *threshold)),
        Gene::Turn { angle_deg, src, threshold, .. } => {
            format!("action turn(angle_deg: {angle_deg:?}){}", source(*src, *threshold))
        }
        Gene::Take { s, max, src, threshold, .. } => {
            format!("action take(s: {}, max: {}){}", substance(*s), amount(*max), source(*src, *threshold))
        }
        Gene::Drop { s, max, src, threshold, .. } => {
            format!("action drop(s: {}, max: {}){}", substance(*s), amount(*max), source(*src, *threshold))
        }
        Gene::Give { s, max, src, threshold, .. } => {
            format!("action give(s: {}, max: {}, to: neighbour){}", substance(*s), amount(*max), source(*src, *threshold))
        }
        Gene::Steal { s, max, src, threshold, .. } => {
            format!("action steal(s: {}, max: {}, from: neighbour){}", substance(*s), amount(*max), source(*src, *threshold))
        }
        Gene::Guard { s, src, threshold, .. } => {
            format!("action guard(s: {}){}", substance(*s), source(*src, *threshold))
        }
        Gene::Divide { split_bp, src, threshold, .. } => {
            format!("action divide(split: {:?}){}", *split_bp as f32 / 10_000.0, source(*src, *threshold))
        }
    }
}

fn amount(m: Mu) -> String {
    if m.is_multiple_of(U) { format!("{}u", m / U) } else { format!("{m}mu") }
}

/// ` <- source threshold t`, leaving out the defaults (`1.0` and `0.0`).
fn source(src: Input, threshold: f32) -> String {
    let mut s = match src {
        Input::Signal(i) => format!(" <- s{i}"),
        Input::Const(c) if c != 1.0 => format!(" <- {c:?}"),
        Input::Const(_) => String::new(),
    };
    if threshold != 0.0 {
        s += &format!(" threshold {threshold:?}");
    }
    s
}

/// A genome in plain language (spec §12.1 `explain`): one line per module saying what its expressed
/// genes sense and do, then the signal network as `.em` text (`genome`).
pub fn explain(w: &World, g: &Genome, field: &str) -> String {
    let name = |s: usize| w.chem.substances[s].name.clone();
    let when = |src: &Input, threshold: f32| match src {
        Input::Signal(k) => format!(" when s{k} is above {threshold}"),
        Input::Const(_) => String::new(),
    };
    let mut out = String::new();
    for (m, &on) in g.modules.iter().enumerate() {
        let phrases: Vec<String> = g
            .slots
            .iter()
            .filter(|x| x.module == m && x.expressed)
            .map(|slot| match &slot.gene {
                Gene::Sense { sense, out, out2 } => {
                    let what = match sense {
                        Sense::Inventory(s) => format!("its own {}", name(*s)),
                        Sense::Cell(s) => format!("the {} in its cell", name(*s)),
                        Sense::Gradient(s) => format!("where {field}.{} rises (ahead and to the left)", name(*s)),
                        Sense::Probe { s, angle_deg, dist } => format!("the {} {dist} ahead at {angle_deg}°", name(*s)),
                        Sense::Crowd { radius } => format!("how many others are within {radius}"),
                        Sense::Neighbour { s, rank } => format!("the {} carried by neighbour {rank}", name(*s)),
                        Sense::Clock { period } => format!("a clock of period {period}"),
                        Sense::Delta(s) => format!("how its {} changed", name(*s)),
                    };
                    let into = out2.map_or(format!("s{out}"), |o| format!("s{out}, s{o}"));
                    format!("senses {what} into {into}")
                }
                Gene::Weigh { inputs, out, .. } => format!("sets s{out} from {}", inputs.iter().map(|(_, k)| format!("s{k}")).collect::<Vec<_>>().join(", ")),
                Gene::Product { a, b, out } => format!("sets s{out} to s{a} × s{b}"),
                Gene::React { reaction, rate, .. } => format!("runs {} at up to {}", w.chem.formula(&w.chem.reactions[*reaction].inputs, &w.chem.reactions[*reaction].outputs), amount(*rate)),
                Gene::Move { speed, src, threshold, .. } => format!("moves at {speed}{}", when(src, *threshold)),
                Gene::Turn { angle_deg, src, threshold, .. } => format!("turns by {angle_deg}°{}", when(src, *threshold)),
                Gene::Take { s, max, src, threshold, .. } => format!("takes up to {} of {}{}", amount(*max), name(*s), when(src, *threshold)),
                Gene::Drop { s, max, src, threshold, .. } => format!("drops up to {} of {}{}", amount(*max), name(*s), when(src, *threshold)),
                Gene::Give { s, max, src, threshold, .. } => format!("gives up to {} of {} to a neighbour{}", amount(*max), name(*s), when(src, *threshold)),
                Gene::Steal { s, max, src, threshold, .. } => format!("steals up to {} of {} from a neighbour{}", amount(*max), name(*s), when(src, *threshold)),
                Gene::Guard { s, src, threshold, .. } => format!("guards its {}{}", name(*s), when(src, *threshold)),
                Gene::Divide { requires, src, threshold, .. } => format!("divides once it holds {} of {}{}", amount(requires.1), name(requires.0), when(src, *threshold)),
            })
            .collect();
        if phrases.is_empty() {
            continue;
        }
        out += &format!("module m{m}{}: {}.\n", if on { "" } else { " (dormant)" }, phrases.join("; "));
    }
    out + "\nSignal network:\n" + &genome(w, g, field)
}
