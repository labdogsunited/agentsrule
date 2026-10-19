//! Elements, substances and reactions (spec §3.1, §3.2, §4).

/// Amounts are whole micro-units (µ). `1u = 1_000 µ`.
pub type Mu = u32;

pub const U: Mu = 1_000;

pub type ElementId = usize;
pub type SubstanceId = usize;
pub type ReactionId = usize;

#[derive(Debug, Clone)]
pub struct Substance {
    pub name: String,
    /// µ of each element per µ of the substance, indexed by element.
    pub comp: Vec<u32>,
    pub potential: i64,
}

/// One side of a reaction: `(substance, coefficient)` terms in substance order.
pub type Side = Vec<(SubstanceId, u32)>;
/// A reaction's inputs and outputs.
pub type Side2 = (Side, Side);

#[derive(Debug, Clone)]
pub struct Reaction {
    pub name: String,
    pub inputs: Vec<(SubstanceId, u32)>,
    pub outputs: Vec<(SubstanceId, u32)>,
    /// Not part of the formula, so the ledger does not see it (spec §4.5).
    pub catalyst: Option<Catalyst>,
}

/// A substance that speeds a reaction up where it is present, without being consumed (spec §4.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Catalyst {
    pub s: SubstanceId,
    pub half: Mu,
    pub boost: f32,
}

impl Catalyst {
    /// `cap × (1 + (boost − 1) × c / (c + half))` with `c` µ of the catalyst present, rounded down.
    pub fn scale(&self, cap: Mu, c: Mu) -> Mu {
        let (c, half) = (c as f64, self.half as f64);
        (cap as f64 * (1.0 + (self.boost as f64 - 1.0) * c / (c + half))).floor() as Mu
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChemError {
    Unbalanced { reaction: String, element: String, inputs: u64, outputs: u64 },
    Uphill { reaction: String, delta_g: i64, min_drop: i64 },
    NotCanonical { reaction: String },
    PaymentRoute { reason: String },
}

#[derive(Debug, Clone, Default)]
pub struct Chemistry {
    pub elements: Vec<String>,
    pub substances: Vec<Substance>,
    pub reactions: Vec<Reaction>,
    pub min_drop: i64,
}

impl Chemistry {
    pub fn new(elements: &[&str]) -> Self {
        Chemistry {
            elements: elements.iter().map(|e| e.to_string()).collect(),
            substances: Vec::new(),
            reactions: Vec::new(),
            min_drop: 1,
        }
    }

    /// Declares a substance from `(element, count)` terms.
    pub fn substance(&mut self, name: &str, terms: &[(&str, u32)], potential: i64) -> SubstanceId {
        let mut comp = vec![0; self.elements.len()];
        for (e, n) in terms {
            let i = self.element(e);
            comp[i] += n;
        }
        self.substances.push(Substance { name: name.to_string(), comp, potential });
        self.substances.len() - 1
    }

    /// Declares a reaction and checks it is balanced, canonical and downhill (§3.2, §4.4).
    pub fn reaction(
        &mut self,
        name: &str,
        inputs: &[(SubstanceId, u32)],
        outputs: &[(SubstanceId, u32)],
    ) -> Result<ReactionId, ChemError> {
        let r = Reaction { name: name.to_string(), inputs: inputs.to_vec(), outputs: outputs.to_vec(), catalyst: None };
        self.check(&r)?;
        self.reactions.push(r);
        Ok(self.reactions.len() - 1)
    }

    /// Every canonical reaction with at most `max_inputs` and `max_outputs` distinct
    /// substances, coefficients up to `max_coeff`, that balances and is downhill (spec §4.3),
    /// in lexicographic order of `(inputs, outputs)` as `(substance, coefficient)` lists.
    /// Neighbouring entries share their leading terms, so they are similar reactions.
    pub fn balanced(&self, max_inputs: usize, max_outputs: usize, max_coeff: u32) -> Vec<Side2> {
        let sides = |max: usize| {
            let mut out: Vec<Side> = Vec::new();
            let mut stack: Vec<Side> = vec![vec![]];
            while let Some(side) = stack.pop() {
                if !side.is_empty() {
                    out.push(side.clone());
                }
                if side.len() < max {
                    let from = side.last().map_or(0, |t| t.0 + 1);
                    for s in from..self.substances.len() {
                        for c in 1..=max_coeff {
                            let mut next = side.clone();
                            next.push((s, c));
                            stack.push(next);
                        }
                    }
                }
            }
            out.sort();
            out
        };
        let (ins, outs) = (sides(max_inputs), sides(max_outputs));
        let mut found = Vec::new();
        for i in &ins {
            for o in &outs {
                let r = Reaction { name: String::new(), inputs: i.clone(), outputs: o.clone(), catalyst: None };
                if self.check(&r).is_ok() {
                    found.push((i.clone(), o.clone()));
                }
            }
        }
        found
    }

    /// `light + carbon -> sugar`.
    pub fn formula(&self, inputs: &[(SubstanceId, u32)], outputs: &[(SubstanceId, u32)]) -> String {
        let side = |terms: &[(SubstanceId, u32)]| {
            terms
                .iter()
                .map(|&(s, c)| if c == 1 { self.substances[s].name.clone() } else { format!("{c} {}", self.substances[s].name) })
                .collect::<Vec<_>>()
                .join(" + ")
        };
        format!("{} -> {}", side(inputs), side(outputs))
    }

    pub fn element(&self, name: &str) -> ElementId {
        self.elements.iter().position(|e| e == name).unwrap_or_else(|| panic!("unknown element {name}"))
    }

    pub fn delta_g(&self, r: &Reaction) -> i64 {
        let side = |terms: &[(SubstanceId, u32)]| -> i64 {
            terms.iter().map(|&(s, c)| c as i64 * self.substances[s].potential).sum()
        };
        side(&r.outputs) - side(&r.inputs)
    }

    fn check(&self, r: &Reaction) -> Result<(), ChemError> {
        // Canonical form: both sides non-empty, terms in substance order without repeats,
        // no substance on both sides, coefficients positive with no common factor.
        let sorted = |terms: &[(SubstanceId, u32)]| terms.windows(2).all(|w| w[0].0 < w[1].0);
        let all = || r.inputs.iter().chain(&r.outputs);
        let canonical = !r.inputs.is_empty()
            && !r.outputs.is_empty()
            && sorted(&r.inputs)
            && sorted(&r.outputs)
            && all().all(|t| t.1 > 0)
            && all().fold(0, |g, t| gcd(g, t.1)) == 1
            && !r.inputs.iter().any(|i| r.outputs.iter().any(|o| o.0 == i.0));
        if !canonical {
            return Err(ChemError::NotCanonical { reaction: r.name.clone() });
        }
        for (e, name) in self.elements.iter().enumerate() {
            let total = |terms: &[(SubstanceId, u32)]| -> u64 {
                terms.iter().map(|&(s, c)| c as u64 * self.substances[s].comp[e] as u64).sum()
            };
            let (i, o) = (total(&r.inputs), total(&r.outputs));
            if i != o {
                return Err(ChemError::Unbalanced { reaction: r.name.clone(), element: name.clone(), inputs: i, outputs: o });
            }
        }
        let dg = self.delta_g(r);
        if dg > -self.min_drop {
            return Err(ChemError::Uphill { reaction: r.name.clone(), delta_g: dg, min_drop: self.min_drop });
        }
        Ok(())
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}
