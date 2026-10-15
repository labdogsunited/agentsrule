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

#[derive(Debug, Clone)]
pub struct Reaction {
    pub name: String,
    pub inputs: Vec<(SubstanceId, u32)>,
    pub outputs: Vec<(SubstanceId, u32)>,
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
        let r = Reaction { name: name.to_string(), inputs: inputs.to_vec(), outputs: outputs.to_vec() };
        self.check(&r)?;
        self.reactions.push(r);
        Ok(self.reactions.len() - 1)
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
