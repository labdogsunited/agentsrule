//! Genomes, signals and minds (spec §5.1–§5.6).

use crate::chem::{Mu, ReactionId, SubstanceId};

/// Internal signals are clamped to this magnitude after every mind step (spec §10.1).
pub const SIGNAL_LIMIT: f32 = 1e4;
/// Sense readings are clamped to this magnitude (spec §10.1).
pub const READING_LIMIT: f32 = 1e6;

/// What a gene reads: a signal, or a constant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Const(f32),
    Signal(usize),
}

/// The default source: genes that read nothing read `1.0` (spec §5.2).
pub const ONE: Input = Input::Const(1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sense {
    /// `inventory(s)`: the entity's own amount, in units.
    Inventory(SubstanceId),
    /// The amount in the entity's cell, in units, from the snapshot.
    Cell(SubstanceId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    Linear,
    Step,
    Sigmoid,
    Tanh,
}

impl Curve {
    pub fn apply(self, x: f32) -> f32 {
        match self {
            Curve::Linear => x,
            Curve::Step => {
                if x > 0.0 { 1.0 } else { 0.0 }
            }
            Curve::Sigmoid => 1.0 / (1.0 + (-x).exp()),
            Curve::Tanh => x.tanh(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Gene {
    /// Sense gene: writes a reading of the snapshot into `out`.
    Sense { sense: Sense, out: usize },
    /// Regulation gene `weigh`: `out ← curve(bias + Σ weight · input)`.
    Weigh { bias: f32, curve: Curve, inputs: Vec<(f32, usize)>, out: usize },
    /// Regulation gene `product`: `out ← a · b`.
    Product { a: usize, b: usize, out: usize },
    /// Reaction gene: runs at up to `floor(rate × clamp(speed, 0, 1))` µ of extent.
    React { reaction: ReactionId, rate: Mu, speed: Input },
    /// Action genes fire when their source is above `threshold` (spec §5.5).
    Take { s: SubstanceId, max: Mu, fee: Mu, src: Input, threshold: f32 },
    Drop { s: SubstanceId, max: Mu, fee: Mu, src: Input, threshold: f32 },
    /// `split_bp` is the child's share in basis points; `requires` is `(substance, amount)`.
    Divide { split_bp: u32, requires: (SubstanceId, Mu), fee: Mu, src: Input, threshold: f32 },
}

impl Gene {
    pub fn take(s: SubstanceId, max: Mu, fee: Mu) -> Gene {
        Gene::Take { s, max, fee, src: ONE, threshold: 0.0 }
    }

    pub fn drop(s: SubstanceId, max: Mu, fee: Mu) -> Gene {
        Gene::Drop { s, max, fee, src: ONE, threshold: 0.0 }
    }

    pub fn react(reaction: ReactionId, rate: Mu) -> Gene {
        Gene::React { reaction, rate, speed: ONE }
    }

    pub fn divide(split_bp: u32, requires: (SubstanceId, Mu), fee: Mu) -> Gene {
        Gene::Divide { split_bp, requires, fee, src: ONE, threshold: 0.0 }
    }

    /// The signal this gene writes, if any.
    pub fn writes(&self) -> Option<usize> {
        match *self {
            Gene::Sense { out, .. } | Gene::Weigh { out, .. } | Gene::Product { out, .. } => Some(out),
            _ => None,
        }
    }

    /// The signals this gene reads.
    pub fn reads(&self) -> Vec<usize> {
        let signal = |i: Input| match i {
            Input::Signal(s) => vec![s],
            Input::Const(_) => vec![],
        };
        match self {
            Gene::Sense { .. } => vec![],
            Gene::Weigh { inputs, .. } => inputs.iter().map(|&(_, s)| s).collect(),
            Gene::Product { a, b, .. } => vec![*a, *b],
            Gene::React { speed, .. } => signal(*speed),
            Gene::Take { src, .. } | Gene::Drop { src, .. } | Gene::Divide { src, .. } => signal(*src),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub gene: Gene,
    pub expressed: bool,
    pub module: usize,
}

/// Genes in genome order, the `expressed` bit of each module, and each signal's `keep`.
#[derive(Debug, Clone, PartialEq)]
pub struct Genome {
    pub slots: Vec<Slot>,
    pub modules: Vec<bool>,
    pub keep: Vec<f32>,
}

impl Genome {
    /// One expressed module of expressed genes, with every signal they use at `keep 0.0`.
    pub fn simple(genes: Vec<Gene>) -> Genome {
        let n = genes
            .iter()
            .flat_map(|g| g.reads().into_iter().chain(g.writes()))
            .max()
            .map_or(0, |m| m + 1);
        Genome {
            slots: genes.into_iter().map(|gene| Slot { gene, expressed: true, module: 0 }).collect(),
            modules: vec![true],
            keep: vec![0.0; n],
        }
    }

    /// A gene is active if it and its module are both expressed (spec §5.2).
    pub fn active(&self, g: usize) -> bool {
        let slot = &self.slots[g];
        slot.expressed && self.modules[slot.module]
    }

    pub fn active_genes(&self) -> usize {
        (0..self.slots.len()).filter(|&g| self.active(g)).count()
    }

    /// Signals written by an active gene and read by an active gene (spec §5.1).
    pub fn signals_in_use(&self) -> Vec<usize> {
        let mut written = vec![false; self.keep.len()];
        let mut read = vec![false; self.keep.len()];
        for g in (0..self.slots.len()).filter(|&g| self.active(g)) {
            if let Some(s) = self.slots[g].gene.writes() {
                written[s] = true;
            }
            for s in self.slots[g].gene.reads() {
                read[s] = true;
            }
        }
        (0..self.keep.len()).filter(|&s| written[s] && read[s]).collect()
    }
}

pub fn sanitize(x: f32, limit: f32) -> f32 {
    if x.is_finite() { x.clamp(-limit, limit) } else { 0.0 }
}

pub fn value(signals: &[f32], i: Input) -> f32 {
    match i {
        Input::Const(c) => c,
        Input::Signal(s) => signals[s],
    }
}

/// Runs `steps` mind steps (spec §5.4). `readings[g]` is the reading of sense gene `g`.
///
/// In every step, every active regulation gene computes its output from the signals at
/// the start of the step; then every signal becomes `keep · s` plus the readings and
/// outputs written to it, summed in gene order, and is sanitized.
pub fn think(genome: &Genome, signals: &mut [f32], readings: &[f32], steps: u32) {
    for _ in 0..steps {
        let mut next: Vec<f32> = signals.iter().zip(&genome.keep).map(|(s, k)| k * s).collect();
        for g in (0..genome.slots.len()).filter(|&g| genome.active(g)) {
            match &genome.slots[g].gene {
                Gene::Sense { out, .. } => next[*out] += readings[g],
                Gene::Weigh { bias, curve, inputs, out } => {
                    let x = inputs.iter().fold(*bias, |acc, &(w, s)| acc + w * signals[s]);
                    next[*out] += curve.apply(x);
                }
                Gene::Product { a, b, out } => next[*out] += signals[*a] * signals[*b],
                _ => {}
            }
        }
        for (s, n) in signals.iter_mut().zip(next) {
            *s = sanitize(n, SIGNAL_LIMIT);
        }
    }
}
