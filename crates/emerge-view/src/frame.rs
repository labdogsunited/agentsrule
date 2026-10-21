//! A frame: what a view reads at one tick, independent of where the world runs. The CPU world
//! fills one here; the GPU runtime fills one from its snapshot (`emerge-server`), so both backends
//! render the same way.

use emerge_ref::World;

#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    /// The full 64-bit entity ID.
    pub id: u64,
    pub lineage: usize,
    pub pos: [f32; 2],
    pub heading: [f32; 2],
    pub age: u64,
    pub expressed_genes: usize,
    /// µ of each substance carried, by substance index.
    pub amounts: Vec<u32>,
    /// µ of `Frame::made`'s substance this entity's own reactions made in its window (0 without one).
    pub made: u64,
}

/// The window `made` is counted over: what was counted, from which tick to the frame's tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Made {
    pub substance: String,
    pub from_tick: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub tick: u64,
    pub revision: u64,
    /// The space: width and height in world units, and whether it wraps.
    pub world_w: f32,
    pub world_h: f32,
    pub wrap: bool,
    /// The conserved field: its name, grid size, and µ of each substance per cell (cell-major).
    pub field: String,
    pub grid_w: usize,
    pub grid_h: usize,
    pub substances: Vec<String>,
    pub cells: Vec<u32>,
    pub lineages: Vec<String>,
    pub agents: Vec<Agent>,
    /// The window of `Agent::made`, if one was counted; without it a view's `made` is unavailable.
    pub made: Option<Made>,
    /// State fields, on the same grid as the conserved field.
    pub states: Vec<States>,
    /// The run's history so far, for charts (`None` for a frame drawn without one).
    pub history: Option<History>,
}

/// A run's history, sampled every `every` ticks (the interval doubles as samples are thinned).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct History {
    pub every: u64,
    pub samples: Vec<Sample>,
}

/// One sample: the state at `tick`, and what happened since the previous sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub tick: u64,
    pub revision: u64,
    /// Living entities, by founding lineage (in `Frame::lineages` order).
    pub by_lineage: Vec<u64>,
    pub births: u64,
    pub deaths: u64,
    /// Element totals against the ledger, in µ: `Some(0)` exact, `Some(d)` the largest drift of
    /// any element, `None` not audited.
    pub drift: Option<i128>,
}

/// Samples kept at most; past this, every other one is merged into the next and `every` doubles.
pub const MAX_SAMPLES: usize = 4_096;

impl History {
    pub fn new(every: u64) -> History {
        History { every: every.max(1), samples: Vec::new() }
    }

    /// Adds a sample, thinning the history if it is full.
    pub fn push(&mut self, s: Sample) {
        self.samples.push(s);
        if self.samples.len() > MAX_SAMPLES {
            let old = std::mem::take(&mut self.samples);
            let mut it = old.into_iter();
            while let Some(a) = it.next() {
                match it.next() {
                    Some(mut b) => {
                        // The pair becomes one sample at the later tick, covering both intervals.
                        b.births += a.births;
                        b.deaths += a.deaths;
                        b.drift = match (a.drift, b.drift) {
                            (Some(x), Some(y)) => Some(if x.abs() > y.abs() { x } else { y }),
                            _ => None,
                        };
                        self.samples.push(b);
                    }
                    None => self.samples.push(a),
                }
            }
            self.every *= 2;
        }
    }
}

/// A state field in a frame: its name, its states' names, and each cell's state (row-major).
#[derive(Debug, Clone, PartialEq)]
pub struct States {
    pub name: String,
    pub names: Vec<String>,
    pub cells: Vec<u8>,
}

impl Frame {
    /// µ of substance `s` in cell `c`.
    pub fn cell(&self, c: usize, s: usize) -> u32 {
        self.cells[c * self.substances.len() + s]
    }

    pub fn substance(&self, name: &str) -> Option<usize> {
        self.substances.iter().position(|s| s == name)
    }

    /// The frame of a CPU world. `field` names its conserved field; `lineages` its lineages, in order.
    /// `made` is left unset: whoever started the window says so.
    pub fn of_world(w: &World, field: &str, lineages: &[String], revision: u64) -> Frame {
        let space = w.space.as_ref().expect("a view needs a world with a space");
        let ns = w.chem.substances.len();
        Frame {
            tick: w.tick,
            revision,
            world_w: space.w,
            world_h: space.h,
            wrap: space.wrap,
            field: field.to_string(),
            grid_w: space.grid_w,
            grid_h: space.grid_h,
            substances: w.chem.substances.iter().map(|s| s.name.clone()).collect(),
            cells: w.cells.iter().flat_map(|c| c.amounts[..ns].to_vec()).collect(),
            lineages: lineages.to_vec(),
            agents: w
                .entities
                .iter()
                .map(|e| Agent { id: e.id, lineage: e.lineage, pos: e.pos, heading: e.heading, age: e.age, expressed_genes: e.genome.active_genes(), amounts: e.store.amounts[..ns].to_vec(), made: e.made })
                .collect(),
            made: None,
            states: w.states.iter().map(|f| States { name: f.name.clone(), names: f.names.clone(), cells: f.cells.clone() }).collect(),
            history: None,
        }
    }
}
