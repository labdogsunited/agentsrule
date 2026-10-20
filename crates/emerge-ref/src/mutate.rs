//! Variation at division (spec §8.1): point, wiring and growth mutations of a child's
//! genome, within the world's menus and the lineage's profile.
//!
//! The operators follow spec §8.1 "Exact operators": how draws are numbered, how a
//! parameter is perturbed, what an inserted gene looks like; and §4.3 for how a reaction ID
//! moves through the reaction space.

use crate::chem::{Mu, ReactionId, SubstanceId};
use crate::mind::{Curve, Gene, Genome, Input, Sense, Slot, ONE};
use crate::philox::Stream;

/// Per-gene and per-module rates of one lineage (`mutation { ... }`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rates {
    /// Point mutations, per gene (and per signal, for `keep`).
    pub rate: f32,
    /// Standard deviation of a parameter change, as a fraction of the parameter's range.
    pub param_sigma: f32,
    pub expression_flip: f32,
    pub rewire: f32,
    pub diverge: f32,
    pub duplicate_gene: f32,
    pub insert_gene: f32,
    pub delete_gene: f32,
    pub duplicate_module: f32,
    pub delete_module: f32,
    pub split_module: f32,
    pub merge_modules: f32,
}

/// A profile's capacity (spec §5.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub genes: usize,
    pub signals: usize,
    pub modules: usize,
}

/// An action on the menu: its parameter range, fee, and for `divide` its requirement.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionMenu {
    pub range: (f32, f32),
    pub fee: Mu,
    /// For substance actions, the substances its `s` may name (all of them for `any substance`).
    pub substances: Vec<SubstanceId>,
}

/// What mutation may produce: the world's menus as the runner supports them, and the
/// reaction space in its mutation order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Menus {
    /// Substances a sense or a `take`/`drop` may name.
    pub substances: Vec<SubstanceId>,
    /// Substances the conserved field holds, for `gradient`.
    pub field: Vec<SubstanceId>,
    pub inventory: bool,
    /// `delta`: change in an own amount since the previous reading.
    pub delta: bool,
    pub gradient: bool,
    /// `probe`: angle and distance ranges, over the field's substances.
    pub probe: Option<((f32, f32), (f32, f32))>,
    /// `crowd`: radius range. `neighbour`: rank range. Both need an interaction radius.
    pub crowd: Option<(f32, f32)>,
    pub neighbour: Option<(f32, f32)>,
    /// `clock`: period range.
    pub clock: Option<(f32, f32)>,
    /// `weigh`: bias range, curves, input counts and weight range.
    pub weigh: Option<WeighMenu>,
    pub product: bool,
    pub moves: Option<ActionMenu>,
    pub turn: Option<ActionMenu>,
    /// `take` and `drop` ranges are amounts, stored as `f32` µ.
    pub take: Option<ActionMenu>,
    pub drop: Option<ActionMenu>,
    pub give: Option<ActionMenu>,
    pub steal: Option<ActionMenu>,
    /// `guard`: its range is unused.
    pub guard: Option<ActionMenu>,
    pub divide: Option<(ActionMenu, (SubstanceId, Mu))>,
    /// The reaction space in mutation order, with each reaction's maximum gene rate.
    pub reactions: Vec<(ReactionId, Mu)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeighMenu {
    pub bias: (f32, f32),
    pub curves: Vec<Curve>,
    pub inputs: (usize, usize),
    pub weight: (f32, f32),
}

/// How far a reaction ID may move to a "nearby" entry of the reaction space.
pub const NEARBY: usize = 8;
/// Thresholds have no menu range; they move by `param_sigma` in signal units, within this.
pub const THRESHOLD_LIMIT: f32 = 16.0;

/// Sequential draws from one Philox stream.
pub struct Draws {
    pub stream: Stream,
    pub next: u64,
    /// Contract mode: `normal` is a sum of twelve uniform draws (`contract.rs`).
    pub contract: bool,
}

impl Draws {
    pub fn new(stream: Stream) -> Self {
        Draws { stream, next: 0, contract: false }
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        let d = self.stream.draw(self.next);
        self.next += 1;
        (d >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// `a·b + c`: fused in contract mode, where a GPU compiler may fuse it on its own.
    pub fn mad(&self, a: f32, b: f32, c: f32) -> f32 {
        if self.contract { crate::contract::fma(a, b, c) } else { a * b + c }
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// Uniform in `0..n`; `n` must be positive.
    pub fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f32) as usize).min(n - 1)
    }

    /// Standard normal, by Box–Muller from two draws.
    pub fn normal(&mut self) -> f32 {
        if self.contract {
            return crate::contract::normal(|| self.unit());
        }
        let (u1, u2) = (1.0 - self.unit(), self.unit());
        (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
    }
}

/// A genome as modules of genes, which growth operators edit more easily than flat slots.
struct Modules {
    modules: Vec<(bool, Vec<(Gene, bool)>)>,
    keep: Vec<f32>,
}

impl Modules {
    fn from(g: &Genome) -> Self {
        let mut modules: Vec<(bool, Vec<(Gene, bool)>)> = g.modules.iter().map(|&e| (e, Vec::new())).collect();
        for slot in &g.slots {
            modules[slot.module].1.push((slot.gene.clone(), slot.expressed));
        }
        Modules { modules, keep: g.keep.clone() }
    }

    fn into_genome(self) -> Genome {
        let mut slots = Vec::new();
        let mut expressed = Vec::new();
        for (m, (e, genes)) in self.modules.into_iter().enumerate() {
            expressed.push(e);
            slots.extend(genes.into_iter().map(|(gene, expressed)| Slot { gene, expressed, module: m }));
        }
        Genome { slots, modules: expressed, keep: self.keep }
    }

    fn genes(&self) -> usize {
        self.modules.iter().map(|m| m.1.len()).sum()
    }

    fn fresh(&mut self, cap: &Capacity) -> Option<usize> {
        (self.keep.len() < cap.signals).then(|| {
            self.keep.push(0.0);
            self.keep.len() - 1
        })
    }
}

/// Every signal a gene reads or writes, in place, so they can be renumbered.
pub(crate) fn signals_mut(g: &mut Gene) -> Vec<&mut usize> {
    fn src(i: &mut Input) -> Option<&mut usize> {
        match i {
            Input::Signal(s) => Some(s),
            Input::Const(_) => None,
        }
    }
    match g {
        Gene::Sense { out, out2, .. } => std::iter::once(out).chain(out2.as_mut()).collect(),
        Gene::Weigh { inputs, out, .. } => inputs.iter_mut().map(|(_, s)| s).chain(std::iter::once(out)).collect(),
        Gene::Product { a, b, out } => vec![a, b, out],
        Gene::React { speed, .. } => src(speed).into_iter().collect(),
        Gene::Move { src: i, .. }
        | Gene::Turn { src: i, .. }
        | Gene::Take { src: i, .. }
        | Gene::Drop { src: i, .. }
        | Gene::Give { src: i, .. }
        | Gene::Steal { src: i, .. }
        | Gene::Guard { src: i, .. }
        | Gene::Divide { src: i, .. } => src(i).into_iter().collect(),
    }
}

fn perturb(d: &mut Draws, x: f32, (lo, hi): (f32, f32), sigma: f32) -> f32 {
    let n = d.normal();
    d.mad(n * sigma, hi - lo, x).clamp(lo, hi)
}

fn perturb_mu(d: &mut Draws, x: Mu, range: (f32, f32), sigma: f32) -> Mu {
    perturb(d, x as f32, range, sigma).round() as Mu
}

fn pick<T: Copy>(d: &mut Draws, xs: &[T]) -> Option<T> {
    (!xs.is_empty()).then(|| xs[d.below(xs.len())])
}

/// Moves a reaction ID to a nearby entry of the reaction space (half the time) or to a
/// random one, and keeps the gene's rate within the new reaction's maximum.
fn move_reaction(d: &mut Draws, menus: &Menus, reaction: &mut ReactionId, rate: &mut Mu) {
    let n = menus.reactions.len();
    if n < 2 {
        return;
    }
    let at = menus.reactions.iter().position(|r| r.0 == *reaction).unwrap_or(0);
    let to = if d.chance(0.5) {
        let step = 1 + d.below(NEARBY);
        if d.chance(0.5) { (at + step).min(n - 1) } else { at.saturating_sub(step) }
    } else {
        d.below(n)
    };
    let (r, max) = menus.reactions[to];
    *reaction = r;
    *rate = (*rate).min(max);
}

/// One point mutation of a gene's parameters (spec §8.1 step 1).
fn point(d: &mut Draws, g: &mut Gene, menus: &Menus, sigma: f32) {
    let threshold = |d: &mut Draws, t: &mut f32| {
        let n = d.normal();
        *t = d.mad(n, sigma, *t).clamp(-THRESHOLD_LIMIT, THRESHOLD_LIMIT)
    };
    match g {
        Gene::Sense { sense, .. } => match sense {
            Sense::Inventory(s) | Sense::Cell(s) | Sense::Delta(s) => *s = pick(d, &menus.substances).unwrap_or(*s),
            Sense::Gradient(s) => *s = pick(d, &menus.field).unwrap_or(*s),
            Sense::Probe { s, angle_deg, dist } => match (&menus.probe, d.below(3)) {
                (_, 0) => *s = pick(d, &menus.field).unwrap_or(*s),
                (Some((a, _)), 1) => *angle_deg = perturb(d, *angle_deg, *a, sigma),
                (Some((_, r)), _) => *dist = perturb(d, *dist, *r, sigma),
                (None, _) => {}
            },
            Sense::Crowd { radius } => {
                if let Some(r) = menus.crowd {
                    *radius = perturb(d, *radius, r, sigma);
                }
            }
            Sense::Neighbour { s, rank } => match (menus.neighbour, d.chance(0.5)) {
                (Some(r), true) => *rank = perturb(d, *rank as f32, r, sigma).round() as u32,
                _ => *s = pick(d, &menus.substances).unwrap_or(*s),
            },
            Sense::Clock { period } => {
                if let Some(r) = menus.clock {
                    *period = perturb(d, *period as f32, r, sigma).round().max(1.0) as u32;
                }
            }
        },
        Gene::Weigh { bias, curve, inputs, .. } => {
            let Some(m) = &menus.weigh else { return };
            match d.below(2 + inputs.len()) {
                0 => *bias = perturb(d, *bias, m.bias, sigma),
                1 => *curve = pick(d, &m.curves).unwrap_or(*curve),
                i => inputs[i - 2].0 = perturb(d, inputs[i - 2].0, m.weight, sigma),
            }
        }
        Gene::Product { .. } => {}
        Gene::React { reaction, rate, .. } => {
            if d.chance(0.5) {
                move_reaction(d, menus, reaction, rate);
            } else if let Some(&(_, max)) = menus.reactions.iter().find(|r| r.0 == *reaction) {
                *rate = perturb_mu(d, *rate, (0.0, max as f32), sigma);
            }
        }
        Gene::Move { speed, threshold: t, .. } => match (&menus.moves, d.chance(0.5)) {
            (Some(m), true) => *speed = perturb(d, *speed, m.range, sigma),
            _ => threshold(d, t),
        },
        Gene::Turn { angle_deg, threshold: t, .. } => match (&menus.turn, d.chance(0.5)) {
            (Some(m), true) => *angle_deg = perturb(d, *angle_deg, m.range, sigma),
            _ => threshold(d, t),
        },
        Gene::Take { s, max, threshold: t, .. } => substance_action(d, &menus.take, s, max, t, sigma),
        Gene::Drop { s, max, threshold: t, .. } => substance_action(d, &menus.drop, s, max, t, sigma),
        Gene::Give { s, max, threshold: t, .. } => substance_action(d, &menus.give, s, max, t, sigma),
        Gene::Steal { s, max, threshold: t, .. } => substance_action(d, &menus.steal, s, max, t, sigma),
        Gene::Guard { s, threshold: t, .. } => {
            if d.chance(0.5) {
                *s = menus.guard.as_ref().and_then(|a| pick(d, &a.substances)).unwrap_or(*s);
            } else {
                threshold(d, t);
            }
        }
        Gene::Divide { split_bp, threshold: t, .. } => match (&menus.divide, d.chance(0.5)) {
            (Some((m, _)), true) => {
                let split = perturb(d, *split_bp as f32 / 10_000.0, m.range, sigma);
                *split_bp = (split * 10_000.0).round() as u32;
            }
            _ => threshold(d, t),
        },
    }
}

fn substance_action(d: &mut Draws, menu: &Option<ActionMenu>, s: &mut SubstanceId, max: &mut Mu, t: &mut f32, sigma: f32) {
    match d.below(3) {
        0 => *s = menu.as_ref().and_then(|a| pick(d, &a.substances)).unwrap_or(*s),
        1 => {
            if let Some(a) = menu {
                *max = perturb_mu(d, *max, a.range, sigma);
            }
        }
        _ => {
            let n = d.normal();
            *t = d.mad(n, sigma, *t).clamp(-THRESHOLD_LIMIT, THRESHOLD_LIMIT)
        }
    }
}

/// A new gene drawn uniformly from the kinds on the menus (spec §8.1 `insert_gene`), with
/// parameters uniform in their ranges. Senses and regulation genes write fresh signals and
/// regulation genes read random existing ones; actions and reactions read `1.0`.
fn random_gene(d: &mut Draws, m: &mut Modules, menus: &Menus, cap: &Capacity) -> Option<Gene> {
    let uniform = |d: &mut Draws, (lo, hi): (f32, f32)| {
        let u = d.unit();
        d.mad(u, hi - lo, lo)
    };
    let mut kinds = Vec::new();
    if menus.inventory && !menus.substances.is_empty() {
        kinds.push(0);
    }
    if menus.gradient && !menus.field.is_empty() {
        kinds.push(1);
    }
    if menus.weigh.is_some() && !m.keep.is_empty() {
        kinds.push(2);
    }
    if menus.product && !m.keep.is_empty() {
        kinds.push(3);
    }
    if !menus.reactions.is_empty() {
        kinds.push(4);
    }
    for (k, on) in [
        (5, menus.moves.is_some()),
        (6, menus.turn.is_some()),
        (7, menus.take.is_some()),
        (8, menus.drop.is_some()),
        (9, menus.divide.is_some()),
        (10, menus.probe.is_some() && !menus.field.is_empty()),
        (11, menus.crowd.is_some()),
        (12, menus.neighbour.is_some()),
        (13, menus.clock.is_some()),
        (14, menus.give.is_some()),
        (15, menus.steal.is_some()),
        (16, menus.delta),
        (17, menus.guard.is_some()),
    ] {
        if on {
            kinds.push(k);
        }
    }
    let whole = |d: &mut Draws, (lo, hi): (f32, f32)| lo.ceil() as u32 + d.below((hi.floor() - lo.ceil()) as usize + 1) as u32;
    let existing = m.keep.len();
    let k = pick(d, &kinds)?;
    Some(match k {
        0 => {
            let s = pick(d, &menus.substances)?;
            Gene::Sense { sense: Sense::Inventory(s), out: m.fresh(cap)?, out2: None }
        }
        1 => {
            let s = pick(d, &menus.field)?;
            if m.keep.len() + 2 > cap.signals {
                return None;
            }
            Gene::Sense { sense: Sense::Gradient(s), out: m.fresh(cap)?, out2: Some(m.fresh(cap)?) }
        }
        2 => {
            let w = menus.weigh.clone()?;
            let n = w.inputs.0 + d.below(w.inputs.1 - w.inputs.0 + 1);
            let inputs = (0..n).map(|_| (uniform(d, w.weight), d.below(existing))).collect();
            let curve = pick(d, &w.curves)?;
            Gene::Weigh { bias: uniform(d, w.bias), curve, inputs, out: m.fresh(cap)? }
        }
        3 => Gene::Product { a: d.below(existing), b: d.below(existing), out: m.fresh(cap)? },
        4 => {
            let (reaction, max) = pick(d, &menus.reactions)?;
            Gene::React { reaction, rate: (d.unit() * max as f32).round() as Mu, speed: ONE }
        }
        5 => {
            let a = menus.moves.clone()?;
            Gene::Move { speed: uniform(d, a.range), fee_per_distance: a.fee, src: ONE, threshold: 0.0 }
        }
        6 => {
            let a = menus.turn.clone()?;
            Gene::Turn { angle_deg: uniform(d, a.range), fee: a.fee, src: ONE, threshold: 0.0 }
        }
        7 => {
            let a = menus.take.clone()?;
            let s = pick(d, &a.substances)?;
            Gene::Take { s, max: uniform(d, a.range).round() as Mu, fee: a.fee, src: ONE, threshold: 0.0 }
        }
        8 => {
            let a = menus.drop.clone()?;
            let s = pick(d, &a.substances)?;
            Gene::Drop { s, max: uniform(d, a.range).round() as Mu, fee: a.fee, src: ONE, threshold: 0.0 }
        }
        9 => {
            let (a, requires) = menus.divide.clone()?;
            let split_bp = (uniform(d, a.range) * 10_000.0).round() as u32;
            Gene::Divide { split_bp, requires, fee: a.fee, src: ONE, threshold: 0.0 }
        }
        10 => {
            let (angle, dist) = menus.probe?;
            let s = pick(d, &menus.field)?;
            Gene::Sense { sense: Sense::Probe { s, angle_deg: uniform(d, angle), dist: uniform(d, dist) }, out: m.fresh(cap)?, out2: None }
        }
        11 => Gene::Sense { sense: Sense::Crowd { radius: uniform(d, menus.crowd?) }, out: m.fresh(cap)?, out2: None },
        12 => {
            let rank = whole(d, menus.neighbour?);
            let s = pick(d, &menus.substances)?;
            Gene::Sense { sense: Sense::Neighbour { s, rank }, out: m.fresh(cap)?, out2: None }
        }
        16 => {
            let s = pick(d, &menus.substances)?;
            Gene::Sense { sense: Sense::Delta(s), out: m.fresh(cap)?, out2: None }
        }
        17 => {
            let a = menus.guard.as_ref()?;
            Gene::Guard { s: pick(d, &a.substances)?, fee: a.fee, src: ONE, threshold: 0.0 }
        }
        13 => Gene::Sense { sense: Sense::Clock { period: whole(d, menus.clock?).max(1) }, out: m.fresh(cap)?, out2: None },
        _ => {
            let a = if k == 14 { menus.give.clone()? } else { menus.steal.clone()? };
            let s = pick(d, &a.substances)?;
            let max = uniform(d, a.range).round() as Mu;
            if k == 14 {
                Gene::Give { s, max, fee: a.fee, src: ONE, threshold: 0.0 }
            } else {
                Gene::Steal { s, max, fee: a.fee, src: ONE, threshold: 0.0 }
            }
        }
    })
}

/// Removes signals no gene uses and renumbers the rest, keeping their order.
fn compact(m: &mut Modules) {
    let mut used = vec![false; m.keep.len()];
    for (_, genes) in &mut m.modules {
        for (g, _) in genes.iter_mut() {
            for s in signals_mut(g) {
                used[*s] = true;
            }
        }
    }
    let mut map = vec![usize::MAX; m.keep.len()];
    let mut keep = Vec::new();
    for (s, &u) in used.iter().enumerate() {
        if u {
            map[s] = keep.len();
            keep.push(m.keep[s]);
        }
    }
    for (_, genes) in &mut m.modules {
        for (g, _) in genes.iter_mut() {
            for s in signals_mut(g) {
                *s = map[*s];
            }
        }
    }
    m.keep = keep;
}

/// Returns the child's genome: `parent` changed by the lineage's rates, in the order of
/// spec §8.1 (point, wiring, growth), never beyond `cap`.
pub fn mutate(parent: &Genome, rates: &Rates, cap: &Capacity, menus: &Menus, d: &mut Draws) -> Genome {
    let mut m = Modules::from(parent);
    let sigma = rates.param_sigma;

    // 1. Point mutations: parameters, keep factors, expression bits.
    for (_, genes) in &mut m.modules {
        for (g, _) in genes.iter_mut() {
            if d.chance(rates.rate) {
                point(d, g, menus, sigma);
            }
        }
    }
    for k in &mut m.keep {
        if d.chance(rates.rate) {
            *k = perturb(d, *k, (0.0, 1.0), sigma);
        }
    }
    for (e, genes) in &mut m.modules {
        for (_, ge) in genes.iter_mut() {
            if d.chance(rates.expression_flip) {
                *ge = !*ge;
            }
        }
        if d.chance(rates.expression_flip) {
            *e = !*e;
        }
    }

    // 2. Wiring: rewire one input, or point one output at a fresh signal.
    for mi in 0..m.modules.len() {
        for gi in 0..m.modules[mi].1.len() {
            if d.chance(rates.rewire) && !m.keep.is_empty() {
                let n = m.keep.len();
                let g = &mut m.modules[mi].1[gi].0;
                match g {
                    Gene::Weigh { inputs, .. } if !inputs.is_empty() => {
                        let i = d.below(inputs.len());
                        inputs[i].1 = d.below(n);
                    }
                    Gene::Product { a, b, .. } => {
                        let to = d.below(n);
                        if d.chance(0.5) { *a = to } else { *b = to }
                    }
                    Gene::React { speed: src, .. }
                    | Gene::Move { src, .. }
                    | Gene::Turn { src, .. }
                    | Gene::Take { src, .. }
                    | Gene::Drop { src, .. }
                    | Gene::Give { src, .. }
                    | Gene::Steal { src, .. }
                    | Gene::Guard { src, .. }
                    | Gene::Divide { src, .. } => {
                        let to = d.below(n + 1);
                        *src = if to == n { ONE } else { Input::Signal(to) };
                    }
                    _ => {}
                }
            }
            if d.chance(rates.diverge) {
                let writes = !m.modules[mi].1[gi].0.writes().is_empty();
                if writes {
                    if let Some(fresh) = m.fresh(cap) {
                        match &mut m.modules[mi].1[gi].0 {
                            Gene::Sense { out, out2, .. } => match out2 {
                                Some(o2) if d.chance(0.5) => *o2 = fresh,
                                _ => *out = fresh,
                            },
                            Gene::Weigh { out, .. } | Gene::Product { out, .. } => *out = fresh,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // 3. Growth, gene by gene and then module by module. New material starts dormant.
    for mi in 0..m.modules.len() {
        let mut gi = 0;
        while gi < m.modules[mi].1.len() {
            if d.chance(rates.duplicate_gene) && m.genes() < cap.genes {
                let copy = m.modules[mi].1[gi].0.clone();
                m.modules[mi].1.insert(gi + 1, (copy, false));
                gi += 1;
            }
            if d.chance(rates.insert_gene) && m.genes() < cap.genes {
                if let Some(g) = random_gene(d, &mut m, menus, cap) {
                    m.modules[mi].1.insert(gi + 1, (g, false));
                    gi += 1;
                }
            }
            if d.chance(rates.delete_gene) {
                m.modules[mi].1.remove(gi);
                continue;
            }
            gi += 1;
        }
    }
    let mut mi = 0;
    while mi < m.modules.len() {
        if d.chance(rates.duplicate_module) && m.modules.len() < cap.modules && m.genes() + m.modules[mi].1.len() <= cap.genes {
            // Private signals (used by no gene outside the module) are replaced in the copy.
            let mut outside = vec![false; m.keep.len()];
            for (other, (_, genes)) in m.modules.iter_mut().enumerate() {
                if other != mi {
                    for (g, _) in genes.iter_mut() {
                        for s in signals_mut(g) {
                            outside[*s] = true;
                        }
                    }
                }
            }
            let mut copy = m.modules[mi].1.clone();
            let mut private: Vec<usize> = Vec::new();
            for (g, _) in &mut copy {
                for s in signals_mut(g) {
                    if !outside[*s] && !private.contains(s) {
                        private.push(*s);
                    }
                }
            }
            if m.keep.len() + private.len() <= cap.signals {
                let base = m.keep.len();
                for &s in &private {
                    let k = m.keep[s];
                    m.keep.push(k);
                }
                for (g, _) in &mut copy {
                    for s in signals_mut(g) {
                        if let Some(i) = private.iter().position(|p| p == s) {
                            *s = base + i;
                        }
                    }
                }
                m.modules.insert(mi + 1, (false, copy));
                mi += 1;
            }
        }
        if d.chance(rates.delete_module) && m.modules.len() > 1 {
            m.modules.remove(mi);
            continue;
        }
        if d.chance(rates.split_module) && m.modules[mi].1.len() >= 2 && m.modules.len() < cap.modules {
            let at = 1 + d.below(m.modules[mi].1.len() - 1);
            let tail = m.modules[mi].1.split_off(at);
            let e = m.modules[mi].0;
            m.modules.insert(mi + 1, (e, tail));
            mi += 1;
        }
        if d.chance(rates.merge_modules) && mi + 1 < m.modules.len() {
            let (_, next) = m.modules.remove(mi + 1);
            m.modules[mi].1.extend(next);
        }
        mi += 1;
    }
    compact(&mut m);
    m.into_genome()
}

/// Checks that a genome is one mutation may produce: within capacity, every signal in its
/// table, slots in module order, and every gene on the menus with parameters in range.
pub fn validate(g: &Genome, cap: &Capacity, menus: &Menus) -> Result<(), String> {
    if g.slots.len() > cap.genes || g.keep.len() > cap.signals || g.modules.len() > cap.modules {
        return Err(format!("over capacity: {} genes, {} signals, {} modules", g.slots.len(), g.keep.len(), g.modules.len()));
    }
    if g.slots.windows(2).any(|w| w[0].module > w[1].module) || g.slots.iter().any(|s| s.module >= g.modules.len()) {
        return Err("slots are not in module order".into());
    }
    if g.keep.iter().any(|k| !(0.0..=1.0).contains(k)) {
        return Err("a keep factor is outside 0.0..1.0".into());
    }
    let within = |x: f32, (lo, hi): (f32, f32)| (lo..=hi).contains(&x);
    for (i, slot) in g.slots.iter().enumerate() {
        let mut gene = slot.gene.clone();
        if signals_mut(&mut gene).iter().any(|s| **s >= g.keep.len()) {
            return Err(format!("gene {i} uses a signal outside the table"));
        }
        let t_ok = |t: f32| (-THRESHOLD_LIMIT..=THRESHOLD_LIMIT).contains(&t);
        let ok = match &slot.gene {
            Gene::Sense { sense: Sense::Inventory(s) | Sense::Cell(s), .. } => menus.inventory && menus.substances.contains(s),
            Gene::Sense { sense: Sense::Delta(s), out2, .. } => menus.delta && menus.substances.contains(s) && out2.is_none(),
            Gene::Sense { sense: Sense::Gradient(s), out2, .. } => menus.gradient && menus.field.contains(s) && out2.is_some(),
            Gene::Sense { sense: Sense::Probe { s, angle_deg, dist }, out2, .. } => {
                menus.probe.is_some_and(|(a, r)| within(*angle_deg, a) && within(*dist, r)) && menus.field.contains(s) && out2.is_none()
            }
            Gene::Sense { sense: Sense::Crowd { radius }, out2, .. } => menus.crowd.is_some_and(|r| within(*radius, r)) && out2.is_none(),
            Gene::Sense { sense: Sense::Neighbour { s, rank }, out2, .. } => {
                menus.neighbour.is_some_and(|r| within(*rank as f32, r)) && menus.substances.contains(s) && out2.is_none()
            }
            Gene::Sense { sense: Sense::Clock { period }, out2, .. } => {
                menus.clock.is_some_and(|r| within(*period as f32, r)) && *period >= 1 && out2.is_none()
            }
            Gene::Weigh { bias, curve, inputs, .. } => menus.weigh.as_ref().is_some_and(|w| {
                within(*bias, w.bias)
                    && w.curves.contains(curve)
                    && (w.inputs.0..=w.inputs.1).contains(&inputs.len())
                    && inputs.iter().all(|(x, _)| within(*x, w.weight))
            }),
            Gene::Product { .. } => menus.product,
            Gene::React { reaction, rate, .. } => menus.reactions.iter().any(|&(r, max)| r == *reaction && *rate <= max),
            Gene::Move { speed, threshold, .. } => menus.moves.as_ref().is_some_and(|a| within(*speed, a.range)) && t_ok(*threshold),
            Gene::Turn { angle_deg, threshold, .. } => menus.turn.as_ref().is_some_and(|a| within(*angle_deg, a.range)) && t_ok(*threshold),
            Gene::Take { s, max, threshold, .. } => {
                menus.take.as_ref().is_some_and(|a| within(*max as f32, a.range) && a.substances.contains(s)) && t_ok(*threshold)
            }
            Gene::Drop { s, max, threshold, .. } => {
                menus.drop.as_ref().is_some_and(|a| within(*max as f32, a.range) && a.substances.contains(s)) && t_ok(*threshold)
            }
            Gene::Give { s, max, threshold, .. } => {
                menus.give.as_ref().is_some_and(|a| within(*max as f32, a.range) && a.substances.contains(s)) && t_ok(*threshold)
            }
            Gene::Steal { s, max, threshold, .. } => {
                menus.steal.as_ref().is_some_and(|a| within(*max as f32, a.range) && a.substances.contains(s)) && t_ok(*threshold)
            }
            Gene::Guard { s, threshold, .. } => menus.guard.as_ref().is_some_and(|a| a.substances.contains(s)) && t_ok(*threshold),
            Gene::Divide { split_bp, threshold, .. } => {
                menus.divide.as_ref().is_some_and(|(a, _)| within(*split_bp as f32 / 10_000.0, a.range)) && t_ok(*threshold)
            }
        };
        if !ok {
            return Err(format!("gene {i} is not on the menus or out of range: {:?}", slot.gene));
        }
    }
    Ok(())
}
