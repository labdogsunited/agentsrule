//! Agent tracking (`proposals/agent-tracking.md`): exact lookups by ID, trackers (a watch on one
//! creature, a fixed cohort, a live or latched query, a family, a representative), and their
//! recordings. All of it observes the run and changes nothing in it: no source revision, no RNG
//! draw, no tick. A tracker's definition has its own revision and undo history; a recording keeps
//! samples at completed tick boundaries, chosen when it starts, whatever the browser receives.
//!
//! Identity is `(run_id, entity ID)`. IDs cross the API as decimal strings (a JavaScript number
//! cannot hold every `u64`); a number is accepted only where it is exact.

use crate::run::{AgentRow, Run};
use emerge_ref::world::Event;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Individuals that Track (a quick watch) keeps per world, in all.
pub const WATCH_LIMIT: usize = 32;
/// Members a tracker holds at most (a cohort, or the matches of a query).
pub const MEMBER_LIMIT: usize = 4096;
/// Individuals a recording samples at most, and samples it keeps per individual.
pub const RECORD_MEMBERS: usize = 32;
pub const RECORD_SAMPLES: usize = 4096;
/// Trackers per world.
pub const TRACKER_LIMIT: usize = 64;
/// Creatures a tracker remembers ever having held; the oldest former members are forgotten past it, and counted.
pub const HISTORY_LIMIT: usize = 50_000;
/// Rows a stored `entities.find` result keeps, and results kept at once.
const FIND_ROWS: usize = 200_000;
const FIND_KEPT: usize = 4;

/// An entity ID from the API: a decimal string, or a number small enough to be exact in JSON readers.
pub fn parse_id(v: &Value) -> Result<u64, String> {
    match v {
        Value::String(s) if !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()) => s.parse::<u64>().map_err(|_| format!("`{s}` is not an entity ID (too large)")),
        Value::String(s) => Err(format!("`{s}` is not an entity ID: write its decimal digits")),
        Value::Number(n) => match n.as_u64() {
            Some(x) if x <= (1u64 << 53) => Ok(x),
            Some(x) => Err(format!("{x} is above 2^53, where a JSON number may not be exact: send the ID as a string")),
            None => Err(format!("{n} is not an entity ID")),
        },
        _ => Err("an entity ID is a string of decimal digits".into()),
    }
}

pub fn parse_ids(v: &Value) -> Result<Vec<u64>, String> {
    v.as_array().ok_or("`ids` must be a list of entity IDs (strings)")?.iter().map(parse_id).collect()
}

/// An ID as the API gives it.
pub fn id_str(id: u64) -> Value {
    Value::String(id.to_string())
}

// ---- predicates (`proposals/agent-tracking.md` §9): attribute, comparison, value, in AND/OR groups.

#[derive(Debug, Clone, PartialEq)]
pub enum Pred {
    All(Vec<Pred>),
    Any(Vec<Pred>),
    /// `attr op value`; `attr` is id, lineage, age, x, y, active_genes or `carried.SUBSTANCE` (µ).
    Cmp { attr: String, op: String, value: Value },
    /// Within `radius` world units of a creature (a region that moves with it; the shortest way on a wrapping world).
    Near { id: u64, radius: f32 },
    /// Inside a fixed rectangle of the world; on a wrapping world a range may wrap (`from > to`).
    Region { x: [f32; 2], y: [f32; 2] },
}

const ATTRS: [&str; 6] = ["id", "lineage", "age", "x", "y", "active_genes"];

impl Pred {
    pub fn parse(v: &Value) -> Result<Pred, String> {
        if let Some(a) = v.get("all") {
            return Ok(Pred::All(a.as_array().ok_or("`all` is a list")?.iter().map(Pred::parse).collect::<Result<_, _>>()?));
        }
        if let Some(a) = v.get("any") {
            return Ok(Pred::Any(a.as_array().ok_or("`any` is a list")?.iter().map(Pred::parse).collect::<Result<_, _>>()?));
        }
        if let Some(n) = v.get("near") {
            let radius = n["radius"].as_f64().filter(|r| *r > 0.0).ok_or("`near.radius` is a positive number of world units")? as f32;
            return Ok(Pred::Near { id: parse_id(&n["id"])?, radius });
        }
        if let Some(r) = v.get("region") {
            let pair = |k: &str| -> Result<[f32; 2], String> {
                let a = r[k].as_array().filter(|a| a.len() == 2).ok_or(format!("`region.{k}` is [from, to]"))?;
                Ok([a[0].as_f64().ok_or("a number")? as f32, a[1].as_f64().ok_or("a number")? as f32])
            };
            return Ok(Pred::Region { x: pair("x")?, y: pair("y")? });
        }
        let attr = v["attr"].as_str().ok_or("a condition is {attr, op, value}, {all: [...]}, {any: [...]}, {near: {id, radius}} or {region: {x, y}}")?.to_string();
        if !(ATTRS.contains(&attr.as_str()) || attr.starts_with("carried.")) {
            return Err(format!("no attribute `{attr}`: use {} or carried.SUBSTANCE", ATTRS.join(", ")));
        }
        let op = v["op"].as_str().ok_or("`op` is one of < <= > >= == !=")?.to_string();
        if !["<", "<=", ">", ">=", "==", "!="].contains(&op.as_str()) {
            return Err(format!("`{op}` is not a comparison: use < <= > >= == !="));
        }
        if attr == "lineage" && !(op == "==" || op == "!=") {
            return Err("a lineage is compared with == or !=".into());
        }
        if attr == "id" {
            parse_id(&v["value"])?;
        } else if attr != "lineage" && !v["value"].is_number() {
            return Err(format!("`{attr}` is compared with a number"));
        }
        Ok(Pred::Cmp { attr, op, value: v["value"].clone() })
    }

    pub fn to_json(&self) -> Value {
        match self {
            Pred::All(v) => json!({"all": v.iter().map(Pred::to_json).collect::<Vec<_>>()}),
            Pred::Any(v) => json!({"any": v.iter().map(Pred::to_json).collect::<Vec<_>>()}),
            Pred::Cmp { attr, op, value } => json!({"attr": attr, "op": op, "value": value}),
            Pred::Near { id, radius } => json!({"near": {"id": id_str(*id), "radius": radius}}),
            Pred::Region { x, y } => json!({"region": {"x": x, "y": y}}),
        }
    }

    /// In words, for headers and exports.
    pub fn text(&self) -> String {
        match self {
            Pred::All(v) if v.is_empty() => "every creature".into(),
            Pred::All(v) => v.iter().map(|p| p.text()).collect::<Vec<_>>().join(" and "),
            Pred::Any(v) => format!("({})", v.iter().map(|p| p.text()).collect::<Vec<_>>().join(" or ")),
            Pred::Cmp { attr, op, value } => format!("{attr} {op} {}", value.as_str().map(String::from).unwrap_or(value.to_string())),
            Pred::Near { id, radius } => format!("within {radius} of #{id}"),
            Pred::Region { x, y } => format!("inside x {}..{}, y {}..{}", x[0], x[1], y[0], y[1]),
        }
    }

    /// The substances it names that the world does not have (a removed substance makes the predicate invalid, never a zero).
    fn missing(&self, substances: &[String], lineages: &[String]) -> Vec<String> {
        match self {
            Pred::All(v) | Pred::Any(v) => v.iter().flat_map(|p| p.missing(substances, lineages)).collect(),
            Pred::Cmp { attr, value, .. } => match attr.strip_prefix("carried.") {
                Some(s) if !substances.iter().any(|x| x == s) => vec![format!("substance `{s}`")],
                _ if attr == "lineage" && !lineages.iter().any(|l| Some(l.as_str()) == value.as_str()) => vec![format!("lineage `{}`", value.as_str().unwrap_or(""))],
                _ => vec![],
            },
            _ => vec![],
        }
    }
}

/// What a predicate is evaluated against: one observation of the run.
struct Scope {
    substances: Vec<String>,
    lineages: Vec<String>,
    space: (f32, f32, bool),
    /// Positions of the creatures `near` names, read in the same observation.
    anchors: HashMap<u64, [f32; 2]>,
}

fn wrapped(d: f32, span: f32, wrap: bool) -> f32 {
    if !wrap { return d.abs(); }
    let d = d.rem_euclid(span);
    d.min(span - d)
}

fn in_range(v: f32, r: [f32; 2], wrap: bool) -> bool {
    if r[0] <= r[1] { v >= r[0] && v < r[1] } else { wrap && (v >= r[0] || v < r[1]) }
}

impl Pred {
    fn eval(&self, a: &AgentRow, s: &Scope) -> bool {
        match self {
            Pred::All(v) => v.iter().all(|p| p.eval(a, s)),
            Pred::Any(v) => v.iter().any(|p| p.eval(a, s)),
            Pred::Near { id, radius } => match s.anchors.get(id) {
                Some(p) => {
                    let (dx, dy) = (wrapped(a.pos[0] - p[0], s.space.0, s.space.2), wrapped(a.pos[1] - p[1], s.space.1, s.space.2));
                    dx * dx + dy * dy <= radius * radius
                }
                None => false,
            },
            Pred::Region { x, y } => in_range(a.pos[0], *x, s.space.2) && in_range(a.pos[1], *y, s.space.2),
            Pred::Cmp { attr, op, value } => {
                if attr == "lineage" {
                    let eq = s.lineages.get(a.lineage).map(String::as_str) == value.as_str();
                    return if op == "==" { eq } else { !eq };
                }
                let x: f64 = match attr.as_str() {
                    "id" => return cmp_u64(a.id, op, parse_id(value).unwrap_or(0)),
                    "age" => a.age as f64,
                    "x" => a.pos[0] as f64,
                    "y" => a.pos[1] as f64,
                    "active_genes" => a.active_genes as f64,
                    c => match s.substances.iter().position(|n| Some(n.as_str()) == c.strip_prefix("carried.")).and_then(|i| a.carried.get(i)) {
                        Some(&v) => f64::from(v),
                        None => return false,
                    },
                };
                let y = value.as_f64().unwrap_or(f64::NAN);
                match op.as_str() { "<" => x < y, "<=" => x <= y, ">" => x > y, ">=" => x >= y, "==" => x == y, _ => x != y }
            }
        }
    }

    /// Whether it reads a carried amount (else the rows it is evaluated on need none).
    pub fn reads_carried(&self) -> bool {
        match self {
            Pred::All(v) | Pred::Any(v) => v.iter().any(Pred::reads_carried),
            Pred::Cmp { attr, .. } => attr.starts_with("carried."),
            _ => false,
        }
    }

    fn anchors(&self, out: &mut Vec<u64>) {
        match self {
            Pred::All(v) | Pred::Any(v) => v.iter().for_each(|p| p.anchors(out)),
            Pred::Near { id, .. } => out.push(*id),
            _ => {}
        }
    }
}

fn cmp_u64(x: u64, op: &str, y: u64) -> bool {
    match op { "<" => x < y, "<=" => x <= y, ">" => x > y, ">=" => x >= y, "==" => x == y, _ => x != y }
}

/// The value a ranking or a sort reads, with unknown substances refused.
fn attr_value(a: &AgentRow, attr: &str, substances: &[String]) -> Option<f64> {
    Some(match attr {
        "id" => a.id as f64,
        "age" => a.age as f64,
        "x" => a.pos[0] as f64,
        "y" => a.pos[1] as f64,
        "active_genes" => a.active_genes as f64,
        "lineage" => a.lineage as f64,
        c => f64::from(*a.carried.get(substances.iter().position(|n| Some(n.as_str()) == c.strip_prefix("carried."))?)?),
    })
}

// ---- lifecycle: what the birth and death events say about a creature.

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Life {
    pub born: Option<(u64, u64)>,
    pub died: Option<(u64, emerge_ref::Cause)>,
}

fn cause_name(c: emerge_ref::Cause) -> &'static str {
    match c { emerge_ref::Cause::Starved => "starved", emerge_ref::Cause::Age => "old age" }
}

// ---- trackers

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    /// These individuals, even after they die.
    Fixed,
    /// Whoever matches at each evaluation; members enter and leave.
    Live(Pred),
    /// Everyone who matches during the armed window `[from, until)`, kept afterwards.
    Latched { pred: Pred, from: u64, until: Option<u64> },
    /// A root and the children born to members, from the birth events.
    Descendants { root: u64, include_past: bool },
    /// The one creature first by `by` (`highest` or `lowest`), ties to the lowest ID; it may change, and each handoff is kept.
    Representative { pred: Pred, by: String, highest: bool },
}

impl Mode {
    fn name(&self) -> &'static str {
        match self { Mode::Fixed => "fixed", Mode::Live(_) => "live", Mode::Latched { .. } => "latched", Mode::Descendants { .. } => "descendants", Mode::Representative { .. } => "representative" }
    }

    fn text(&self) -> String {
        match self {
            Mode::Fixed => "these individuals, whatever they do later".into(),
            Mode::Live(p) => format!("whoever matches now: {}", p.text()),
            Mode::Latched { pred, from, until } => format!("everyone seen matching {} from tick {from}{}, kept afterwards", pred.text(), until.map_or(String::new(), |u| format!(" until tick {u}"))),
            Mode::Descendants { root, include_past } => format!("#{root} and its descendants{}", if *include_past { " (children born before tracking began included, from the birth events)" } else { " born from the start of tracking" }),
            Mode::Representative { pred, by, highest } => format!("the creature with the {} {by} among {}, ties to the lowest ID", if *highest { "highest" } else { "lowest" }, pred.text()),
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Mode::Fixed => json!({"mode": "fixed"}),
            Mode::Live(p) => json!({"mode": "live", "where": p.to_json()}),
            Mode::Latched { pred, from, until } => json!({"mode": "latched", "where": pred.to_json(), "from": from, "until": until}),
            Mode::Descendants { root, include_past } => json!({"mode": "descendants", "root": id_str(*root), "include_past": include_past}),
            Mode::Representative { pred, by, highest } => json!({"mode": "representative", "where": pred.to_json(), "by": by, "highest": highest}),
        }
    }

    /// Whether evaluating it reads carried amounts (its condition, or a ranking by one).
    fn reads_carried(&self) -> bool {
        self.query().is_some_and(Pred::reads_carried) || matches!(self, Mode::Representative { by, .. } if by.starts_with("carried."))
    }

    fn query(&self) -> Option<&Pred> {
        match self { Mode::Live(p) | Mode::Latched { pred: p, .. } | Mode::Representative { pred: p, .. } => Some(p), _ => None }
    }
}

/// A membership interval: in the tracker from `from` (a tick), until `until` if it left (a live query) or was
/// handed over (a representative). A death does not end membership.
#[derive(Debug, Clone, PartialEq)]
pub struct Stay {
    pub from: u64,
    pub until: Option<u64>,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    pub color: String,
    /// The display flag: whether its markers are drawn.
    pub visible: bool,
    pub label: bool,
    /// Received positions a viewer keeps as a recent display trail (0: none).
    pub trail: u32,
}

impl Display {
    fn to_json(&self) -> Value {
        json!({"color": self.color, "visible": self.visible, "label": self.label, "trail": self.trail})
    }
}

/// A recording's settings and what it has kept (`proposals/agent-tracking.md` §14).
#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    pub id: String,
    pub every: u64,
    /// Substances whose carried amount each sample keeps (indices at the start; their names are kept too).
    pub substances: Vec<(usize, String)>,
    pub cap: usize,
    /// `stop` at the cap, or `evict` the oldest samples of that individual.
    pub evict: bool,
    pub from: u64,
    pub first: u64,
    pub stop_at: Option<u64>,
    pub stopped: Option<(u64, String)>,
    pub samples: Vec<Sample>,
    pub next_seq: u64,
    /// Per individual: samples kept, and samples evicted (oldest first).
    pub kept: HashMap<u64, usize>,
    pub evicted: HashMap<u64, u64>,
    /// Sample ticks taken, and members over the 32 sampled at a tick (not recorded).
    pub ticks: Vec<u64>,
    pub over_limit: u64,
    /// Lifecycle events of members while recording (births of members' children, deaths).
    pub events: Vec<Value>,
    pub events_dropped_at_start: u64,
    /// Whether creature interactions are filtered from a transfer recording, and whose it is.
    pub interactions: Option<String>,
    /// Revision boundaries met while recording: (tick, revision).
    pub revisions: Vec<(u64, u64)>,
    /// The position in the event list the recording has read to.
    pub events_read: usize,
}

/// One kept sample: an individual's state at a completed tick boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub seq: u64,
    pub tick: u64,
    pub revision: u64,
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub age: u64,
    pub carried: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tracker {
    pub id: String,
    pub name: String,
    /// `watch` (made by Track, one click), or `tracker` (a node or a procedural set).
    pub kind: String,
    pub mode: Mode,
    pub members: BTreeMap<u64, Vec<Stay>>,
    /// Members past the cap that a query matched but could not keep, at the last evaluation.
    pub capped: u64,
    /// Former members whose stays were dropped to keep the history bounded (`HISTORY_LIMIT`).
    pub forgotten: u64,
    pub display: Display,
    /// The tracker's own definition revision (each change to it is one), and the world revision and tick it was made at.
    pub definition: u64,
    pub made_tick: u64,
    pub made_revision: u64,
    /// Membership definitions in order: (from tick, definition revision, mode in words). A change starts a new one.
    pub segments: Vec<(u64, u64, String)>,
    /// Ticks between evaluations of a query (live, latched, representative) or a family check.
    pub every: u64,
    pub last_eval: Option<u64>,
    /// Evaluations due but not run (the world stepped past them inside one call is never one: every tick boundary is seen).
    pub invalid: Option<String>,
    /// Stopped observing: no live updates, no recording; definition and data kept.
    pub stopped: Option<u64>,
    /// The last thing seen of each member: the tick, and its row then.
    pub last: HashMap<u64, (u64, AgentRow)>,
    pub rec: Option<Recording>,
    /// The position in the event list a family has read to.
    pub events_read: usize,
    /// A key a caller gave to `tracker.create` so that sending it twice makes one tracker.
    pub request: Option<String>,
    /// Where the node sits in the Observe network (presentation only).
    pub pos: Option<[f64; 2]>,
}

impl Tracker {
    /// Members now: in the tracker, not left (dead ones stay).
    pub fn current(&self) -> Vec<u64> {
        self.members.iter().filter(|(_, s)| s.last().is_some_and(|x| x.until.is_none())).map(|(id, _)| *id).collect()
    }

    fn join(&mut self, id: u64, tick: u64, why: &str) -> bool {
        let v = self.members.entry(id).or_default();
        if v.last().is_some_and(|s| s.until.is_none()) { return false; }
        v.push(Stay { from: tick, until: None, why: why.into() });
        true
    }

    fn leave(&mut self, id: u64, tick: u64) {
        if let Some(s) = self.members.get_mut(&id).and_then(|v| v.last_mut()) {
            if s.until.is_none() { s.until = Some(tick); }
        }
    }

    /// Keeps a query's history bounded: past `HISTORY_LIMIT` creatures ever in it, those that left longest ago are
    /// forgotten (and counted), never current members.
    fn forget(&mut self) {
        if self.members.len() <= HISTORY_LIMIT { return; }
        let mut gone: Vec<(u64, u64)> = self.members.iter().filter_map(|(id, s)| s.last().and_then(|x| x.until).map(|u| (u, *id))).collect();
        gone.sort_unstable();
        for (_, id) in gone.into_iter().take(self.members.len() - HISTORY_LIMIT) {
            self.members.remove(&id);
            self.last.remove(&id);
            self.forgotten += 1;
        }
    }

    fn is_query(&self) -> bool {
        self.mode.query().is_some() || matches!(self.mode, Mode::Descendants { .. })
    }
}

/// A tracker's definition without what it observed: what undo and redo put back. For a fixed set the individuals are
/// the definition; a query's members are observations and are chosen again.
#[derive(Debug, Clone)]
struct Def {
    id: String,
    name: String,
    kind: String,
    mode: Mode,
    display: Display,
    every: u64,
    stopped: Option<u64>,
    request: Option<String>,
    pos: Option<[f64; 2]>,
    fixed: Vec<u64>,
    made_tick: u64,
    made_revision: u64,
}

fn def_of(t: &Tracker) -> Def {
    Def { id: t.id.clone(), name: t.name.clone(), kind: t.kind.clone(), mode: t.mode.clone(), display: t.display.clone(), every: t.every, stopped: t.stopped, request: t.request.clone(), pos: t.pos,
        fixed: if matches!(t.mode, Mode::Fixed) { t.current() } else { vec![] }, made_tick: t.made_tick, made_revision: t.made_revision }
}

/// Every tracker of a run, the definition history, and the caches of lookups.
#[derive(Debug, Default)]
pub struct Tracking {
    pub trackers: Vec<Tracker>,
    /// The observation revision: one more for every accepted definition change of any tracker.
    pub revision: u64,
    next: u64,
    next_rec: u64,
    /// The definitions before each change, for undo and redo: never what was observed or recorded.
    undo: Vec<(String, Vec<Def>)>,
    redo: Vec<(String, Vec<Def>)>,
    /// IDs below this were given at the start (founders: no parent recorded, none expected).
    pub founders_below: u64,
    life: HashMap<u64, Life>,
    life_read: usize,
    dropped: u64,
    have_events: bool,
    finds: Vec<Find>,
    next_find: u64,
}

/// A stored `entities.find` result: every match at one tick, sorted, paged without skipping or repeating.
#[derive(Debug)]
struct Find {
    cursor: String,
    tick: u64,
    revision: u64,
    rows: Vec<AgentRow>,
    total_matched: usize,
    /// 1, or the step of an even sample.
    every: usize,
}


/// Colours of trackers, in turn; distinct from lineage colours by being drawn as rings.
const COLORS: [&str; 8] = ["#ffcc33", "#33ddff", "#ff66cc", "#99ff66", "#ff8844", "#bb99ff", "#66ffcc", "#ffffff"];

impl Tracking {
    /// Whether anything must look at the run after this tick: a recording sample, a query or a family check.
    pub fn due(&self, tick: u64) -> bool {
        self.trackers.iter().any(|t| t.stopped.is_none() && t.invalid.is_none() && (t.is_query() && tick.is_multiple_of(t.every) || t.rec.as_ref().is_some_and(|r| r.stopped.is_none() && tick >= r.first && (tick - r.first).is_multiple_of(r.every))))
    }

    fn snapshot(&mut self, what: &str) {
        self.undo.push((what.to_string(), self.trackers.iter().map(def_of).collect()));
        if self.undo.len() > 100 { self.undo.remove(0); }
        self.redo.clear();
        self.revision += 1;
    }

    fn find(&self, id: &str) -> Result<usize, String> {
        self.trackers.iter().position(|t| t.id == id).ok_or_else(|| format!("no tracker `{id}`"))
    }

    /// Reads the birth and death events for these IDs (all events once for an ID never asked about, then only new ones).
    /// With `alive` the rows of a lookup: when every asked creature is living and already known, the events are
    /// not read at all (on the GPU that is a read-back of the whole log).
    fn lifecycle(&mut self, run: &Run, ids: &[u64], living: Option<&[Option<AgentRow>]>) -> (HashMap<u64, Life>, u64, bool) {
        let fresh: Vec<u64> = ids.iter().copied().filter(|id| !self.life.contains_key(id)).collect();
        if fresh.is_empty() && living.is_some_and(|rows| rows.iter().all(Option::is_some)) {
            return (ids.iter().map(|id| (*id, self.life[id])).collect(), self.dropped, self.have_events);
        }
        let scan = |events: &[Event], want: &HashSet<u64>, life: &mut HashMap<u64, Life>| {
            for e in events {
                match e {
                    Event::Born { tick, parent, child } if want.contains(child) => life.entry(*child).or_default().born = Some((*tick, *parent)),
                    Event::Died { tick, id, cause } if want.contains(id) => life.entry(*id).or_default().died = Some((*tick, *cause)),
                    _ => {}
                }
            }
        };
        // New IDs: the whole log once. Known ones: only what was logged since the last read.
        let known: HashSet<u64> = self.life.keys().copied().collect();
        let Ok((recent, next, dropped)) = run.events_since(self.life_read) else { self.have_events = false; return (HashMap::new(), 0, false) };
        scan(&recent, &known, &mut self.life);
        self.life_read = next;
        self.dropped = dropped;
        self.have_events = true;
        if !fresh.is_empty() {
            let want: HashSet<u64> = fresh.iter().copied().collect();
            for id in &fresh { self.life.insert(*id, Life::default()); }
            if let Ok((all, _)) = run.events() { scan(&all, &want, &mut self.life); }
        }
        if self.life.len() > 100_000 { self.life.retain(|k, _| ids.contains(k)); }
        (ids.iter().map(|id| (*id, self.life[id])).collect(), dropped, true)
    }

    /// `entities.get`: these creatures as they are now, each with its status, and what is known of its birth and death.
    pub fn get(&mut self, run: &Run, ids: &[u64]) -> Value {
        let rows = run.agents_by_id(ids);
        let (life, dropped, have_events) = self.lifecycle(run, ids, Some(&rows));
        let names = run.substance_names();
        let next = run.next_id();
        let list: Vec<Value> = ids.iter().zip(&rows).map(|(&id, row)| agent_json(run, id, row.as_ref(), life.get(&id).copied(), have_events, dropped, next, self.founders_below, &names)).collect();
        json!({"run": run.run_id, "tick": run.tick(), "revision": run.revision(), "backend": run.device, "agents": list,
            "complete": "a creature not in this lookup is not living now: the lookup reads every living creature",
            "events_dropped": dropped})
    }

    /// `entities.find`: every creature matching `pred` at this tick, sorted by `sort` (ties by ID), paged from one stored result.
    /// With `sample`, an even sample of that many matches (every k-th by ID), and the answer says so.
    #[allow(clippy::too_many_arguments)]
    pub fn find_agents(&mut self, run: &Run, pred: Option<Pred>, sort: Option<(String, bool)>, limit: usize, cursor: Option<&str>, offset: usize, sample: Option<usize>) -> Result<Value, String> {
        let names = run.substance_names();
        let k = match cursor {
            Some(c) => self.finds.iter().position(|f| f.cursor == c).ok_or_else(|| format!("cursor `{c}` expired: query again (results are kept for the {FIND_KEPT} newest queries)"))?,
            None => {
                if let Some(p) = &pred {
                    let miss = p.missing(&names, &run.spec.lineages);
                    if !miss.is_empty() { return Err(format!("the world has no {}", miss.join(", "))); }
                }
                let all = run.all_agents(true);
                let scope = self.scope(run, pred.as_ref(), &all);
                let mut rows: Vec<AgentRow> = all.into_iter().filter(|a| pred.as_ref().is_none_or(|p| p.eval(a, &scope))).collect();
                let matched = rows.len();
                let mut every = 1;
                if let Some(n) = sample.filter(|&n| n > 0 && rows.len() > n) {
                    rows.sort_by_key(|a| a.id);
                    every = rows.len().div_ceil(n);
                    rows = rows.into_iter().step_by(every).collect();
                }
                if let Some((by, desc)) = &sort {
                    if attr_value(&AgentRow { id: 0, lineage: 0, age: 0, pos: [0.0; 2], heading: [0.0; 2], active_genes: 0, carried: vec![0; names.len()] }, by, &names).is_none() {
                        return Err(format!("cannot sort by `{by}`"));
                    }
                    rows.sort_by(|a, b| {
                        let (x, y) = (attr_value(a, by, &names).unwrap_or(0.0), attr_value(b, by, &names).unwrap_or(0.0));
                        let o = x.total_cmp(&y);
                        (if *desc { o.reverse() } else { o }).then(a.id.cmp(&b.id))
                    });
                } else {
                    rows.sort_by_key(|a| a.id);
                }
                rows.truncate(FIND_ROWS);
                self.next_find += 1;
                let cursor = format!("f{}", self.next_find);
                self.finds.push(Find { cursor, tick: run.tick(), revision: run.revision(), rows, total_matched: matched, every });
                if self.finds.len() > FIND_KEPT { self.finds.remove(0); }
                self.finds.len() - 1
            }
        };
        let f = &self.finds[k];
        let page: Vec<Value> = f.rows.iter().skip(offset).take(limit).map(|a| row_json(run, a, &names)).collect();
        Ok(json!({"cursor": f.cursor, "tick": f.tick, "revision": f.revision, "matched": f.total_matched, "kept": f.rows.len(), "offset": offset, "agents": page,
            "observed": format!("every match at tick {}, one observation: pages of this cursor never skip or repeat a creature, and are not refreshed", f.tick),
            "sampled": (f.every > 1).then(|| format!("an even sample: every {}th of {} matches by ID", f.every, f.total_matched)),
            "truncated": f.every == 1 && f.total_matched > f.rows.len()}))
    }

    fn scope(&self, run: &Run, pred: Option<&Pred>, all: &[AgentRow]) -> Scope {
        let mut want = vec![];
        if let Some(p) = pred { p.anchors(&mut want); }
        let anchors = all.iter().filter(|a| want.contains(&a.id)).map(|a| (a.id, a.pos)).collect();
        Scope { substances: run.substance_names(), lineages: run.spec.lineages.clone(), space: run.space().unwrap_or((1.0, 1.0, false)), anchors }
    }

    /// `tracker.create`.
    pub fn create(&mut self, run: &Run, p: &Value) -> Result<Value, String> {
        if let Some(key) = p["request"].as_str() {
            if let Some(t) = self.trackers.iter().find(|t| t.request.as_deref() == Some(key)) {
                return Ok(json!({"tracker": t.id, "definition": t.definition, "observation_revision": self.revision, "existing": true}));
            }
        }
        if self.trackers.len() >= TRACKER_LIMIT { return Err(format!("a world holds at most {TRACKER_LIMIT} trackers: stop and remove one first")); }
        let def = if p["definition"].is_object() { &p["definition"] } else { p };
        let mode = parse_mode(def, run.tick())?;
        let kind = p["kind"].as_str().unwrap_or("tracker");
        if kind != "watch" && kind != "tracker" { return Err("`kind` is `watch` or `tracker`".into()); }
        let ids = if p["ids"].is_null() { vec![] } else { parse_ids(&p["ids"])? };
        if mode == Mode::Fixed && ids.is_empty() {
            return Err(if p["definition"].is_object() { "a fixed set names individuals of one run: choose them again in this run (`ids`)".into() } else { "a fixed set needs `ids`".into() });
        }
        if mode != Mode::Fixed && !ids.is_empty() { return Err("only a fixed set takes `ids`; a query chooses its own members".into()); }
        if ids.len() > MEMBER_LIMIT { return Err(format!("a tracker holds at most {MEMBER_LIMIT} creatures")); }
        if kind == "watch" { self.watch_room(&ids, None)?; }
        if let Some(q) = mode.query() {
            let miss = q.missing(&run.substance_names(), &run.spec.lineages);
            if !miss.is_empty() { return Err(format!("the world has no {}", miss.join(", "))); }
        }
        let every = def["every"].as_u64().unwrap_or(1).max(1);
        self.next += 1;
        let id = format!("t{}", self.next);
        let tick = run.tick();
        let name = p["name"].as_str().map(String::from).unwrap_or_else(|| match (&mode, ids.as_slice()) { (Mode::Fixed, [one]) => format!("Agent {one}"), _ => format!("Tracker {}", self.next) });
        let disp = &p["display"];
        let display = Display {
            color: disp["color"].as_str().map(String::from).unwrap_or_else(|| COLORS[(self.next as usize - 1) % COLORS.len()].to_string()),
            visible: disp["visible"].as_bool().unwrap_or(true),
            label: disp["label"].as_bool().unwrap_or(true),
            trail: disp["trail"].as_u64().unwrap_or(0).min(256) as u32,
        };
        let mut t = Tracker { id: id.clone(), name, kind: kind.into(), mode: mode.clone(), members: BTreeMap::new(), capped: 0, forgotten: 0, display, definition: 1, made_tick: tick, made_revision: run.revision(),
            segments: vec![(tick, 1, mode.text())], every, last_eval: None, invalid: None, stopped: None, last: HashMap::new(), rec: None, events_read: 0, request: p["request"].as_str().map(String::from), pos: None };
        let why = p["why"].as_str().unwrap_or("chosen");
        for &m in &ids { t.join(m, tick, why); }
        if let Mode::Descendants { root, include_past } = &t.mode {
            let (root, past) = (*root, *include_past);
            t.join(root, tick, "the root");
            if let Ok((events, _)) = run.events() {
                if past {
                    // Children born before tracking, in event order, so a grandchild follows its parent.
                    for e in events.iter() {
                        if let Event::Born { tick: bt, parent, child } = e {
                            if t.members.contains_key(parent) && t.members.len() < MEMBER_LIMIT { t.join(*child, tick, &format!("born at tick {bt} before tracking began (from the birth events)")); }
                        }
                    }
                }
                t.events_read = events.len();
            }
        }
        let q = t.mode.query().is_some().then(|| t.mode.reads_carried());
        self.snapshot(&format!("create {}", t.name));
        self.trackers.push(t);
        if let Some(carried) = q {
            let all = run.all_agents(carried);
            let k = self.trackers.len() - 1;
            self.evaluate(run, k, &all);
        }
        Ok(json!({"tracker": id, "definition": 1, "observation_revision": self.revision, "tick": tick}))
    }

    /// Room for `ids` among the quick watches (all `watch` trackers of the world, but `except`).
    fn watch_room(&self, ids: &[u64], except: Option<&str>) -> Result<(), String> {
        let mut all: HashSet<u64> = self.trackers.iter().filter(|t| t.kind == "watch" && Some(t.id.as_str()) != except).flat_map(|t| t.current()).collect();
        all.extend(ids);
        if all.len() > WATCH_LIMIT { return Err(format!("at most {WATCH_LIMIT} creatures are watched at once: stop watching one first, or make a tracker for a larger set")); }
        Ok(())
    }

    /// `tracker.update`: name, display, cadence, or a new membership definition (which starts a new segment;
    /// past membership is kept as it was). `base` is the definition revision the change was made against.
    pub fn update(&mut self, run: &Run, p: &Value) -> Result<Value, String> {
        let k = self.find(p["tracker"].as_str().ok_or("`tracker` must be a tracker id")?)?;
        let t = &self.trackers[k];
        if let Some(b) = p["base"].as_u64() {
            if b != t.definition { return Err(format!("stale: the tracker is at definition {}, the change was made against {b}", t.definition)); }
        }
        let mut n = t.clone();
        let tick = run.tick();
        let mut what = vec![];
        if let Some(s) = p["name"].as_str() { if s.trim().is_empty() { return Err("a name cannot be empty".into()); } n.name = s.trim().to_string(); what.push("name"); }
        let d = &p["display"];
        if d.is_object() {
            if let Some(c) = d["color"].as_str() { if !(c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())) { return Err("a colour is #rrggbb".into()); } n.display.color = c.into(); }
            if let Some(v) = d["visible"].as_bool() { n.display.visible = v; }
            if let Some(v) = d["label"].as_bool() { n.display.label = v; }
            if let Some(v) = d["trail"].as_u64() { n.display.trail = v.min(256) as u32; }
            what.push("display");
        }
        if let Some(e) = p["every"].as_u64() { n.every = e.max(1); what.push("cadence"); }
        if let Some(pos) = p["pos"].as_array() { if let [Some(x), Some(y)] = [pos.first().and_then(Value::as_f64), pos.get(1).and_then(Value::as_f64)] { n.pos = Some([x, y]); } }
        let mut fresh_query = false;
        if !p["mode"].is_null() || !p["where"].is_null() {
            let mut def = n.mode.to_json();
            for key in ["mode", "where", "from", "until", "root", "include_past", "by", "highest"] { if !p[key].is_null() { def[key] = p[key].clone(); } }
            let mode = parse_mode(&def, tick)?;
            if let Some(q) = mode.query() {
                let miss = q.missing(&run.substance_names(), &run.spec.lineages);
                if !miss.is_empty() { return Err(format!("the world has no {}", miss.join(", "))); }
            }
            // Turning a query into a fixed set captures its current members; any other new definition starts
            // from no members (the earlier ones keep their past stays) and chooses again.
            if mode != Mode::Fixed {
                for id in n.current() { n.leave(id, tick); }
            }
            if let Mode::Descendants { root, .. } = &mode { n.join(*root, tick, "the root"); n.events_read = run.events().map(|(e, _)| e.len()).unwrap_or(0); }
            n.mode = mode;
            n.invalid = None;
            fresh_query = n.mode.query().is_some();
            what.push("membership");
        }
        if let Some(add) = p.get("add").filter(|v| !v.is_null()) {
            if !matches!(n.mode, Mode::Fixed) { return Err("members are added to a fixed set; a query chooses its own".into()); }
            let ids = parse_ids(add)?;
            if n.kind == "watch" { self.watch_room(&ids.iter().copied().chain(n.current()).collect::<Vec<_>>(), Some(&n.id))?; }
            for id in ids { n.join(id, tick, p["why"].as_str().unwrap_or("added")); }
            if n.members.len() > MEMBER_LIMIT { return Err(format!("a tracker holds at most {MEMBER_LIMIT} creatures")); }
            what.push("members");
        }
        if let Some(rm) = p.get("remove").filter(|v| !v.is_null()) {
            if !matches!(n.mode, Mode::Fixed) { return Err("members leave a query on their own; change its condition instead".into()); }
            for id in parse_ids(rm)? { n.leave(id, tick); }
            what.push("members");
        }
        if what.is_empty() && n.pos != t.pos {
            // Where the node sits is layout: no definition change, no undo step, no revision.
            self.trackers[k].pos = n.pos;
            return Ok(json!({"tracker": n.id, "definition": n.definition, "observation_revision": self.revision, "changed": ["position"], "presentation_only": true}));
        }
        if what.is_empty() { return Err("nothing to change: give name, display, every, mode/where, add or remove".into()); }
        let presentation_only = what.iter().all(|w| *w == "display" || *w == "name");
        n.definition += 1;
        if what.contains(&"membership") || what.contains(&"members") { n.segments.push((tick, n.definition, n.mode.text())); }
        self.snapshot(&format!("{} of {}", what.join(", "), n.name));
        self.trackers[k] = n;
        if fresh_query {
            let all = run.all_agents(self.trackers[k].mode.reads_carried());
            self.evaluate(run, k, &all);
        }
        let t = &self.trackers[k];
        Ok(json!({"tracker": t.id, "definition": t.definition, "observation_revision": self.revision, "changed": what, "presentation_only": presentation_only}))
    }

    /// `tracker.stop`: stops observing (no live updates, no recording); the definition and the data stay.
    pub fn stop(&mut self, run: &Run, id: &str) -> Result<Value, String> {
        let k = self.find(id)?;
        let tick = run.tick();
        self.snapshot(&format!("stop {}", self.trackers[k].name));
        let t = &mut self.trackers[k];
        t.stopped = Some(tick);
        if let Some(r) = &mut t.rec { if r.stopped.is_none() { r.stopped = Some((tick, "the tracker stopped observing".into())); } }
        t.definition += 1;
        Ok(json!({"tracker": t.id, "stopped_at": tick, "definition": t.definition, "observation_revision": self.revision, "kept": "the definition, its membership history and any recorded samples"}))
    }

    /// `tracker.resume`: observes again from now (a gap in coverage stays a gap).
    pub fn resume(&mut self, run: &Run, id: &str) -> Result<Value, String> {
        let k = self.find(id)?;
        self.snapshot(&format!("resume {}", self.trackers[k].name));
        let t = &mut self.trackers[k];
        t.stopped = None;
        t.definition += 1;
        Ok(json!({"tracker": t.id, "resumed_at": run.tick(), "definition": t.definition, "observation_revision": self.revision}))
    }

    /// `tracker.remove`: deletes a definition. One that holds recorded samples needs `delete_data`.
    pub fn remove(&mut self, id: &str, delete_data: bool) -> Result<Value, String> {
        let k = self.find(id)?;
        let has = self.trackers[k].rec.as_ref().is_some_and(|r| !r.samples.is_empty() || !r.events.is_empty());
        if has && !delete_data { return Err(format!("`{id}` holds a recording: stop it and keep it, or pass delete_data: true to delete the recording with it")); }
        self.snapshot(&format!("remove {}", self.trackers[k].name));
        let t = self.trackers.remove(k);
        Ok(json!({"removed": t.id, "observation_revision": self.revision, "deleted_samples": t.rec.map_or(0, |r| r.samples.len())}))
    }

    /// `tracker.undo` / `tracker.redo`: definitions only; recordings and what was observed are never undone.
    pub fn step_history(&mut self, run: &Run, undo: bool) -> Result<Value, String> {
        let (from, to) = if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let (what, defs) = from.last().cloned().ok_or(if undo { "nothing to undo" } else { "nothing to redo" })?;
        // A tracker the restored definitions lack would be removed: not one that holds a recording (delete that explicitly).
        if let Some(t) = self.trackers.iter().find(|t| !defs.iter().any(|d| d.id == t.id) && t.rec.as_ref().is_some_and(|r| !r.samples.is_empty())) {
            return Err(format!("cannot {} `{what}`: {} holds a recording; remove it with tracker.remove and delete_data", if undo { "undo" } else { "redo" }, t.name));
        }
        from.pop();
        to.push((what.clone(), self.trackers.iter().map(def_of).collect()));
        let tick = run.tick();
        let why = if undo { "put back by undo" } else { "put back by redo" };
        let mut old: HashMap<String, Tracker> = self.trackers.drain(..).map(|t| (t.id.clone(), t)).collect();
        for d in defs {
            let t = match old.remove(&d.id) {
                Some(mut t) => {
                    // What it observed stays; only the definition goes back. A changed membership rule starts a segment.
                    t.name = d.name; t.kind = d.kind; t.display = d.display; t.every = d.every; t.stopped = d.stopped; t.request = d.request; t.pos = d.pos;
                    if t.mode != d.mode {
                        if d.mode != Mode::Fixed { for id in t.current() { t.leave(id, tick); } }
                        if let Mode::Descendants { root, .. } = &d.mode { t.join(*root, tick, "the root"); t.events_read = run.events().map(|(e, _)| e.len()).unwrap_or(0); }
                        t.mode = d.mode;
                        t.invalid = None;
                        t.segments.push((tick, t.definition + 1, t.mode.text()));
                    }
                    if matches!(t.mode, Mode::Fixed) {
                        let want: HashSet<u64> = d.fixed.iter().copied().collect();
                        for id in t.current() { if !want.contains(&id) { t.leave(id, tick); } }
                        for id in d.fixed { t.join(id, tick, why); }
                    }
                    t.definition += 1;
                    t
                }
                None => {
                    // Put back with no data: what it observed before it was removed is gone.
                    let mut t = Tracker { id: d.id, name: d.name, kind: d.kind, mode: d.mode.clone(), members: BTreeMap::new(), capped: 0, forgotten: 0, display: d.display, definition: 1, made_tick: d.made_tick, made_revision: d.made_revision,
                        segments: vec![(tick, 1, d.mode.text())], every: d.every, last_eval: None, invalid: None, stopped: d.stopped, last: HashMap::new(), rec: None, events_read: run.events().map(|(e, _)| e.len()).unwrap_or(0), request: d.request, pos: d.pos };
                    for id in d.fixed { t.join(id, tick, why); }
                    if let Mode::Descendants { root, .. } = &d.mode { t.join(*root, tick, "the root"); }
                    t
                }
            };
            self.trackers.push(t);
        }
        self.revision += 1;
        Ok(json!({"undone": undo, "what": what, "observation_revision": self.revision, "trackers": self.trackers.len(), "note": "definitions only: what trackers observed and recorded is kept; a tracker put back after removal starts with no data"}))
    }

    /// Evaluates a query tracker against one observation of every creature.
    fn evaluate(&mut self, run: &Run, k: usize, all: &[AgentRow]) {
        let tick = run.tick();
        let names = run.substance_names();
        let pred = match self.trackers[k].mode.query() { Some(p) => p.clone(), None => return };
        let miss = pred.missing(&names, &run.spec.lineages);
        let scope = self.scope(run, Some(&pred), all);
        let t = &mut self.trackers[k];
        t.last_eval = Some(tick);
        if !miss.is_empty() {
            t.invalid = Some(format!("the condition names {}, which the world no longer has (a revision removed it): it is not treated as zero; change the condition", miss.join(", ")));
            return;
        }
        let matched: Vec<&AgentRow> = all.iter().filter(|a| pred.eval(a, &scope)).collect();
        match t.mode.clone() {
            Mode::Live(_) => {
                let now: HashSet<u64> = matched.iter().map(|a| a.id).collect();
                let cur: HashSet<u64> = t.current().into_iter().collect();
                for id in cur.difference(&now) { t.leave(*id, tick); }
                let (mut count, mut capped) = (cur.intersection(&now).count(), 0);
                for a in &matched {
                    if cur.contains(&a.id) { continue; }
                    if count >= MEMBER_LIMIT { capped += 1; continue; }
                    t.join(a.id, tick, "matched");
                    count += 1;
                }
                t.capped = capped;
            }
            Mode::Latched { from, until, .. } => {
                if tick < from || until.is_some_and(|u| tick >= u) { return; }
                let mut capped = 0;
                for a in &matched {
                    if !t.members.contains_key(&a.id) && t.members.len() >= MEMBER_LIMIT { capped += 1; continue; }
                    t.join(a.id, tick, &format!("first seen matching at tick {tick}"));
                }
                t.capped += capped;
            }
            Mode::Representative { by, highest, .. } => {
                let best = matched.iter().filter_map(|a| attr_value(a, &by, &names).map(|v| (v, a.id))).min_by(|x, y| {
                    let o = x.0.total_cmp(&y.0);
                    (if highest { o.reverse() } else { o }).then(x.1.cmp(&y.1))
                }).map(|x| x.1);
                let cur = t.current();
                if cur.first().copied() != best {
                    for id in cur { t.leave(id, tick); }
                    if let Some(b) = best { t.join(b, tick, &format!("first by {by} at tick {tick}")); }
                }
            }
            _ => {}
        }
        for a in matched { if t.members.contains_key(&a.id) { t.last.insert(a.id, (tick, a.clone())); } }
        t.forget();
    }

    /// After a tick: queries and families that are due, then recording samples. Called by `Run::step`.
    pub fn observe(&mut self, run: &Run) {
        let tick = run.tick();
        let queries: Vec<usize> = (0..self.trackers.len()).filter(|&k| { let t = &self.trackers[k]; t.stopped.is_none() && t.invalid.is_none() && t.mode.query().is_some() && tick.is_multiple_of(t.every) }).collect();
        if !queries.is_empty() {
            let all = run.all_agents(queries.iter().any(|&k| self.trackers[k].mode.reads_carried()));
            for k in queries { self.evaluate(run, k, &all); }
        }
        // Families: children born to members since the last look.
        let fams: Vec<usize> = (0..self.trackers.len()).filter(|&k| { let t = &self.trackers[k]; t.stopped.is_none() && matches!(t.mode, Mode::Descendants { .. }) && tick.is_multiple_of(t.every) }).collect();
        if !fams.is_empty() {
            let from = fams.iter().map(|&k| self.trackers[k].events_read).min().unwrap_or(0);
            if let Ok((events, next, _)) = run.events_since(from) {
                for k in fams {
                    let t = &mut self.trackers[k];
                    for e in &events[(t.events_read - from).min(events.len())..] {
                        if let Event::Born { tick: bt, parent, child } = e {
                            if t.members.contains_key(parent) {
                                if t.members.len() >= MEMBER_LIMIT { t.capped += 1; } else { t.join(*child, tick, &format!("born to #{parent} at tick {bt}")); }
                            }
                        }
                    }
                    t.events_read = next;
                }
            }
        }
        // Recordings that are due take one sample of each current member (at most 32), from one read of the run.
        let due: Vec<usize> = (0..self.trackers.len()).filter(|&k| self.trackers[k].stopped.is_none() && self.trackers[k].rec.as_ref().is_some_and(|r| r.stopped.is_none() && tick >= r.first && (tick - r.first).is_multiple_of(r.every))).collect();
        if due.is_empty() { return; }
        let mut ids: Vec<u64> = due.iter().flat_map(|&k| self.trackers[k].current().into_iter().take(RECORD_MEMBERS)).collect();
        ids.sort_unstable();
        ids.dedup();
        let rows = run.agents_by_id(&ids);
        let by: HashMap<u64, &AgentRow> = rows.iter().flatten().map(|r| (r.id, r)).collect();
        // What the events say is read only for members missing for the first time.
        let noted: HashSet<u64> = due.iter().filter_map(|&k| self.trackers[k].rec.as_ref()).flat_map(|r| r.events.iter().filter(|e| e["event"] != "child").filter_map(|e| e["id"].as_str().and_then(|x| x.parse().ok()))).collect();
        let missing: Vec<u64> = ids.iter().copied().filter(|id| !by.contains_key(id) && !noted.contains(id)).collect();
        let (life, _, _) = if missing.is_empty() { (HashMap::new(), 0, true) } else { self.lifecycle(run, &missing, None) };
        let rev = run.revision();
        for k in due {
            let t = &mut self.trackers[k];
            let cur = t.current();
            let Some(r) = t.rec.as_mut() else { continue };
            if r.stop_at.is_some_and(|s| tick > s) { r.stopped = Some((tick, format!("reached its stop tick {}", r.stop_at.unwrap_or(0)))); continue; }
            if r.revisions.last().is_none_or(|x| x.1 != rev) {
                // A hot swap can add or remove substances: the columns follow their names, and a removed one ends the recording.
                let names = run.substance_names();
                let mut gone = vec![];
                for (i, name) in r.substances.iter_mut() { match names.iter().position(|n| n == name) { Some(j) => *i = j, None => gone.push(name.clone()) } }
                if !gone.is_empty() { r.stopped = Some((tick, format!("revision {rev} removed {}, a column of this recording", gone.join(", ")))); continue; }
                r.revisions.push((tick, rev));
            }
            r.ticks.push(tick);
            r.over_limit += cur.len().saturating_sub(RECORD_MEMBERS) as u64;
            for id in cur.into_iter().take(RECORD_MEMBERS) {
                let Some(a) = by.get(&id) else {
                    // Not living: one lifecycle note, the first time it is missed.
                    if !r.events.iter().any(|e| e["id"] == id_str(id) && e["event"] != "child") {
                        let l = life.get(&id).copied().unwrap_or_default();
                        r.events.push(match l.died {
                            Some((dt, c)) => json!({"event": "death", "id": id_str(id), "tick": dt, "cause": cause_name(c), "noticed_at_sample": tick, "note": "the event's tick is the tick the death happened in; the last sample before it is the last observed state, not the state at death"}),
                            None => json!({"event": "missing", "id": id_str(id), "noticed_at_sample": tick, "note": "not living at this sample, and no death is recorded (the event log may have dropped it): no cause or tick is given"}),
                        });
                    }
                    continue;
                };
                let n = r.kept.entry(id).or_default();
                if *n >= r.cap {
                    if !r.evict { r.stopped = Some((tick, format!("an individual reached the cap of {} samples", r.cap))); break; }
                    if let Some(i) = r.samples.iter().position(|s| s.id == id) { r.samples.remove(i); *r.evicted.entry(id).or_default() += 1; *n -= 1; }
                }
                *n += 1;
                r.samples.push(Sample { seq: r.next_seq, tick, revision: rev, id, x: a.pos[0], y: a.pos[1], age: a.age, carried: r.substances.iter().map(|(i, _)| a.carried.get(*i).copied().unwrap_or(0)).collect() });
                r.next_seq += 1;
                t.last.insert(id, (tick, (*a).clone()));
            }
        }
        // Children of recorded members, from the events (the recording's lifecycle links).
        let recs: Vec<usize> = (0..self.trackers.len()).filter(|&k| self.trackers[k].rec.as_ref().is_some_and(|r| r.stopped.is_none())).collect();
        let from = recs.iter().filter_map(|&k| self.trackers[k].rec.as_ref().map(|r| r.events_read)).min().unwrap_or(0);
        if recs.is_empty() { return; }
        if let Ok((events, next, _)) = run.events_since(from) {
            for &k in &recs {
                let t = &mut self.trackers[k];
                let members: HashSet<u64> = t.members.keys().copied().collect();
                let r = t.rec.as_mut().unwrap();
                for e in &events[(r.events_read - from).min(events.len())..] {
                    if let Event::Born { tick: bt, parent, child } = e {
                        if members.contains(parent) { r.events.push(json!({"event": "child", "id": id_str(*parent), "child": id_str(*child), "tick": bt})); }
                    }
                }
                r.events_read = next;
            }
        }
    }

    /// `tracker.record_start`.
    pub fn record_start(&mut self, run: &Run, p: &Value, gpu: bool) -> Result<Value, String> {
        let k = self.find(p["tracker"].as_str().ok_or("`tracker` must be a tracker id")?)?;
        let tick = run.tick();
        let every = p["every"].as_u64().unwrap_or(if gpu { 10 } else { 1 });
        if every == 0 { return Err("`every` is at least 1 tick".into()); }
        if gpu && every < GPU_MIN_EVERY { return Err(format!("on the GPU a recording samples at most every {GPU_MIN_EVERY} ticks (each sample reads the entity table back from the device); ask for every: {GPU_MIN_EVERY} or more")); }
        let cap = p["cap"].as_u64().unwrap_or(RECORD_SAMPLES as u64) as usize;
        if cap == 0 || cap > RECORD_SAMPLES { return Err(format!("`cap` is 1 to {RECORD_SAMPLES} samples per individual")); }
        let evict = match p["policy"].as_str().unwrap_or("stop") { "stop" => false, "evict" => true, o => return Err(format!("`policy` is stop or evict, not {o}")) };
        let names = run.substance_names();
        let substances: Vec<(usize, String)> = match &p["substances"] {
            Value::Null => names.iter().cloned().enumerate().collect(),
            v => v.as_array().ok_or("`substances` is a list of names")?.iter().map(|s| { let s = s.as_str().unwrap_or(""); names.iter().position(|n| n == s).map(|i| (i, s.to_string())).ok_or(format!("the world has no substance `{s}`")) }).collect::<Result<_, _>>()?,
        };
        let interactions = if p["interactions"].as_bool().unwrap_or(false) { Some(String::new()) } else { None };
        let t = &mut self.trackers[k];
        if t.stopped.is_some() { return Err(format!("`{}` has stopped observing: resume it first", t.id)); }
        if t.rec.as_ref().is_some_and(|r| r.stopped.is_none()) { return Err(format!("`{}` is already recording", t.id)); }
        if t.rec.as_ref().is_some_and(|r| !r.samples.is_empty()) && !p["replace"].as_bool().unwrap_or(false) {
            return Err(format!("`{}` holds a finished recording: export it, then pass replace: true to start a new one", t.id));
        }
        let (_, dropped) = run.events().unwrap_or((std::borrow::Cow::Borrowed(&[]), 0));
        self.next_rec += 1;
        let first = tick + 1;
        let stop_at = p["stop_at"].as_u64();
        let read_from = run.events().map(|(e, _)| e.len()).unwrap_or(0);
        t.rec = Some(Recording { id: format!("rec{}", self.next_rec), every, substances: substances.clone(), cap, evict, from: tick, first, stop_at, stopped: None, samples: vec![], next_seq: 0, kept: HashMap::new(), evicted: HashMap::new(), ticks: vec![], over_limit: 0, events: vec![], events_dropped_at_start: dropped, interactions, revisions: vec![], events_read: read_from });
        let members = t.current().len();
        Ok(json!({"recording": t.rec.as_ref().unwrap().id, "tracker": t.id, "first_sample_tick": first, "every": every, "cap": cap, "policy": if evict { "evict" } else { "stop" },
            "substances": substances.iter().map(|s| &s.1).collect::<Vec<_>>(), "members_recorded": members.min(RECORD_MEMBERS), "members_not_recorded": members.saturating_sub(RECORD_MEMBERS),
            "continues": "on the server, whatever the browser receives or whether it is open, until stop_at, the cap, or tracker.record_stop",
            "sample_convention": "a sample at tick T is the state after T ticks have run (a completed tick boundary); an event at tick T happened during tick T"}))
    }

    pub fn record_stop(&mut self, run: &Run, id: &str) -> Result<Value, String> {
        let k = self.find(id)?;
        let t = &mut self.trackers[k];
        let r = t.rec.as_mut().ok_or(format!("`{id}` is not recording"))?;
        if r.stopped.is_none() { r.stopped = Some((run.tick(), "stopped by request".into())); }
        Ok(json!({"tracker": t.id, "recording": r.id, "stopped_at": r.stopped.as_ref().unwrap().0, "samples": r.samples.len()}))
    }

    /// `tracker.query`: membership with each member's status and current state, coverage, and the definition.
    pub fn query(&mut self, run: &Run, id: &str) -> Result<Value, String> {
        let k = self.find(id)?;
        let ids: Vec<u64> = self.trackers[k].members.keys().copied().collect();
        let rows = run.agents_by_id(&ids);
        let (life, dropped, have_events) = self.lifecycle(run, &ids, Some(&rows));
        let names = run.substance_names();
        let next = run.next_id();
        let tick = run.tick();
        let founders = self.founders_below;
        let t = &mut self.trackers[k];
        if t.stopped.is_none() { for r in rows.iter().flatten() { t.last.insert(r.id, (tick, r.clone())); } }
        let members: Vec<Value> = ids.iter().zip(&rows).map(|(&id, row)| {
            let stays = &t.members[&id];
            let mut a = if t.stopped.is_some() {
                // Stopped: the last observation, not a live one.
                let mut v = json!({"id": id_str(id), "status": "not observed", "why": format!("the tracker stopped observing at tick {}", t.stopped.unwrap_or(0))});
                if let Some((lt, r)) = t.last.get(&id) { v["last_observed"] = json!({"tick": lt, "state": row_json(run, r, &names)}); }
                v
            } else {
                agent_json(run, id, row.as_ref(), life.get(&id).copied(), have_events, dropped, next, founders, &names)
            };
            a["member"] = json!(stays.last().is_some_and(|s| s.until.is_none()));
            a["stays"] = json!(stays.iter().map(|s| json!({"from": s.from, "until": s.until, "why": s.why})).collect::<Vec<_>>());
            if row.is_none() { if let Some((lt, r)) = t.last.get(&id) { a["last_observed"] = json!({"tick": lt, "state": row_json(run, r, &names), "note": "the last time this tracker saw it, not its state at death"}); } }
            a
        }).collect();
        Ok(json!({"run": run.run_id, "tick": tick, "revision": run.revision(), "backend": run.device, "observation_revision": self.revision,
            "tracker": tracker_json(t), "members": members, "events_dropped": dropped}))
    }

    pub fn list(&self) -> Value {
        json!({"observation_revision": self.revision, "trackers": self.trackers.iter().map(tracker_json).collect::<Vec<_>>(), "watch_limit": WATCH_LIMIT,
            "can_undo": self.undo.last().map(|u| &u.0), "can_redo": self.redo.last().map(|u| &u.0)})
    }

    /// `tracker.samples`: kept samples after `after` (a sequence number: a client that reconnects asks from the
    /// last one it has, and gets none twice), at most `limit`, with the schema and coverage.
    pub fn samples(&self, run: &Run, id: &str, after: Option<u64>, limit: usize) -> Result<Value, String> {
        let t = &self.trackers[self.find(id)?];
        let r = t.rec.as_ref().ok_or(format!("`{id}` has no recording: start one with tracker.record_start"))?;
        let page: Vec<&Sample> = r.samples.iter().filter(|s| after.is_none_or(|a| s.seq > a)).take(limit).collect();
        let more = r.samples.iter().filter(|s| after.is_none_or(|a| s.seq > a)).count() > page.len();
        let cols: Vec<String> = ["seq", "tick", "revision", "id", "x", "y", "age"].iter().map(|s| s.to_string()).chain(r.substances.iter().map(|s| format!("carried.{} (µ)", s.1))).collect();
        Ok(json!({"run": run.run_id, "tracker": t.id, "recording": r.id,
            "schema": {"columns": cols},
            "rows": page.iter().map(|s| { let mut v = vec![json!(s.seq), json!(s.tick), json!(s.revision), id_str(s.id), json!(s.x), json!(s.y), json!(s.age)]; v.extend(s.carried.iter().map(|c| json!(c))); v }).collect::<Vec<_>>(),
            "more": more, "last_seq": page.last().map(|s| s.seq),
            "events": r.events, "coverage": coverage(r, run)}))
    }

    /// `tracker.export`: CSV or JSON of a recording, with its provenance.
    pub fn export(&self, run: &Run, id: &str, format: &str, interactions: Option<Value>) -> Result<Value, String> {
        let t = &self.trackers[self.find(id)?];
        let r = t.rec.as_ref().ok_or(format!("`{id}` has no recording"))?;
        let prov = json!({"run": run.run_id, "source_hash": format!("{:016x}", crate::journal::fnv(&run.live.source)), "seed": run.spec.seed, "mutation": run.spec.mutation, "backend": run.device,
            "build": env!("CARGO_PKG_VERSION"), "tracker": tracker_json(t), "selection_rule": t.mode.text(), "segments": t.segments, "sampling": {"every": r.every, "first_tick": r.first, "convention": "state after the tick (a completed tick boundary)"},
            "revisions": r.revisions, "coverage": coverage(r, run), "units": "positions in world units; carried amounts in µ (integer)"});
        let cols: Vec<String> = ["seq", "tick", "revision", "id", "x", "y", "age"].iter().map(|s| s.to_string()).chain(r.substances.iter().map(|s| format!("carried_{}_mu", s.1))).collect();
        match format {
            "json" => Ok(json!({"format": "json", "provenance": prov, "columns": cols,
                "rows": r.samples.iter().map(|s| { let mut v = vec![json!(s.seq), json!(s.tick), json!(s.revision), id_str(s.id), json!(s.x), json!(s.y), json!(s.age)]; v.extend(s.carried.iter().map(|c| json!(c))); v }).collect::<Vec<_>>(),
                "events": r.events, "interactions": interactions})),
            "csv" => {
                let mut out = String::new();
                for line in serde_json::to_string_pretty(&prov).unwrap_or_default().lines() { out += "# "; out += line; out += "\n"; }
                out += &cols.join(",");
                out += "\n";
                for s in &r.samples {
                    out += &format!("{},{},{},{},{},{},{}", s.seq, s.tick, s.revision, s.id, s.x, s.y, s.age);
                    for c in &s.carried { out += &format!(",{c}"); }
                    out += "\n";
                }
                Ok(json!({"format": "csv", "text": out, "events": r.events}))
            }
            f => Err(format!("`format` is csv or json, not {f}")),
        }
    }

    /// The IDs a viewer should draw markers for: every visible, observing tracker's current members, with the trackers each is in.
    pub fn shown(&self) -> BTreeMap<u64, Vec<String>> {
        let mut out: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        for t in self.trackers.iter().filter(|t| t.display.visible && t.stopped.is_none()) {
            for id in t.current() { out.entry(id).or_default().push(t.id.clone()); }
        }
        out
    }

    /// The trackers that own a recording of interactions, by tracker id.
    pub fn interaction_owner(&self) -> Option<String> {
        self.trackers.iter().find(|t| t.rec.as_ref().is_some_and(|r| r.stopped.is_none() && r.interactions.is_some())).map(|t| t.id.clone())
    }

    /// Marks a recording as filtering this trace (`owner` is who holds the transfer-recording slot).
    pub fn attach_interactions(&mut self, id: &str, owner: &str) {
        if let Some(r) = self.trackers.iter_mut().find(|t| t.id == id).and_then(|t| t.rec.as_mut()) { r.interactions = Some(owner.to_string()); }
    }

    pub fn tracker(&self, id: &str) -> Option<&Tracker> {
        self.trackers.iter().find(|t| t.id == id)
    }
}

/// On the GPU, a recording's smallest interval (each sample is a read-back of the entity table).
pub const GPU_MIN_EVERY: u64 = 10;

fn coverage(r: &Recording, run: &Run) -> Value {
    json!({"recording_from": r.from, "first_sample_tick": r.first, "every": r.every, "sampled_ticks": r.ticks.len(), "first": r.ticks.first(), "last": r.ticks.last(),
        "stopped": r.stopped.as_ref().map(|(t, why)| json!({"tick": t, "why": why})), "cap_per_individual": r.cap, "policy": if r.evict { "evict" } else { "stop" },
        "evicted": r.evicted.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "members_over_limit": r.over_limit, "events_dropped_before_start": r.events_dropped_at_start,
        "events_dropped_now": run.events().map(|(_, d)| d).unwrap_or(0),
        "gaps": "none: the run's own step takes every sample that is due; a slow or closed browser changes nothing here",
        "interactions": r.interactions})
}

fn parse_mode(p: &Value, tick: u64) -> Result<Mode, String> {
    let wher = || p.get("where").filter(|v| !v.is_null()).map(Pred::parse).unwrap_or(Ok(Pred::All(vec![])));
    Ok(match p["mode"].as_str().unwrap_or("fixed") {
        "fixed" => Mode::Fixed,
        "live" => Mode::Live(wher()?),
        "latched" => Mode::Latched { pred: wher()?, from: p["from"].as_u64().unwrap_or(tick), until: p["until"].as_u64() },
        "descendants" => Mode::Descendants { root: parse_id(&p["root"]).map_err(|e| format!("`root`: {e}"))?, include_past: p["include_past"].as_bool().unwrap_or(false) },
        "representative" => {
            let by = p["by"].as_str().unwrap_or("age").to_string();
            if !(ATTRS.contains(&by.as_str()) || by.starts_with("carried.")) { return Err(format!("cannot rank by `{by}`")); }
            Mode::Representative { pred: wher()?, by, highest: p["highest"].as_bool().unwrap_or(true) }
        }
        m => return Err(format!("`mode` is fixed, live, latched, descendants or representative, not {m}")),
    })
}

pub fn tracker_json(t: &Tracker) -> Value {
    let cur = t.current();
    json!({"id": t.id, "name": t.name, "kind": t.kind, "definition": t.definition, "membership": t.mode.to_json(), "means": t.mode.text(), "mode": t.mode.name(),
        "members_now": cur.iter().map(|x| id_str(*x)).collect::<Vec<_>>(), "count_now": cur.len(), "ever": t.members.len() as u64 + t.forgotten, "forgotten": t.forgotten, "capped": t.capped, "member_limit": MEMBER_LIMIT,
        "display": t.display.to_json(), "made_tick": t.made_tick, "made_revision": t.made_revision, "segments": t.segments.iter().map(|s| json!({"from": s.0, "definition": s.1, "means": s.2})).collect::<Vec<_>>(),
        "every": t.every, "last_evaluated": t.last_eval, "invalid": t.invalid, "stopped": t.stopped, "pos": t.pos,
        "recording": t.rec.as_ref().map(|r| json!({"id": r.id, "every": r.every, "first_sample_tick": r.first, "samples": r.samples.len(), "on": r.stopped.is_none(), "stopped": r.stopped.as_ref().map(|(t, w)| json!({"tick": t, "why": w})), "interactions": r.interactions.is_some()}))})
}

fn row_json(run: &Run, a: &AgentRow, names: &[String]) -> Value {
    let carried: serde_json::Map<String, Value> = names.iter().enumerate().map(|(i, n)| (n.clone(), json!(a.carried.get(i).copied().unwrap_or(0)))).collect();
    json!({"id": id_str(a.id), "lineage": run.spec.lineages.get(a.lineage), "age": a.age, "x": a.pos[0], "y": a.pos[1], "heading": a.heading, "active_genes": a.active_genes, "carried_mu": carried})
}

#[allow(clippy::too_many_arguments)]
fn agent_json(run: &Run, id: u64, row: Option<&AgentRow>, life: Option<Life>, have_events: bool, dropped: u64, next: u64, founders: u64, names: &[String]) -> Value {
    let l = life.unwrap_or_default();
    let born = match l.born {
        Some((t, p)) => json!({"tick": t, "parent": id_str(p)}),
        None if id < founders => json!({"founder": true, "note": "placed at the start: no parent"}),
        None if !have_events => json!({"unknown": "this run keeps no birth events"}),
        None => json!({"unknown": if dropped > 0 { "no birth recorded: the event log dropped records" } else { "no birth recorded" }}),
    };
    match row {
        Some(a) => { let mut v = row_json(run, a, names); v["status"] = json!("alive"); v["born"] = born; v }
        None if id == 0 || id >= next => json!({"id": id_str(id), "status": "unknown", "why": format!("no creature of this run has had this ID (IDs given so far: 1 to {})", next.saturating_sub(1))}),
        None => match l.died {
            Some((t, c)) => json!({"id": id_str(id), "status": "dead", "death": {"tick": t, "cause": cause_name(c)}, "born": born}),
            None => json!({"id": id_str(id), "status": "not living", "why": if dropped > 0 { "not in the complete lookup of the living; the event log dropped records, so its death may be one of them" } else if have_events { "not in the complete lookup of the living, and no death is recorded" } else { "not in the complete lookup of the living; this run keeps no death events" }, "born": born}),
        },
    }
}
