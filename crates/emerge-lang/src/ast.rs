//! The syntax tree of an `.em` file (`language.md` §11). Comments and blank lines are kept
//! as trivia so the formatter can print them back.

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trivia {
    pub comments: Vec<String>,
    pub blank_before: bool,
    pub trailing: Option<String>,
}

/// A source position, 1-based. `Pos::default()` (line 0) means unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct Node<T> {
    pub trivia: Trivia,
    /// Where the node starts. Not part of equality, so formatting a file keeps its AST.
    pub pos: Pos,
    pub node: T,
}

impl<T: PartialEq> PartialEq for Node<T> {
    fn eq(&self, other: &Self) -> bool {
        self.trivia == other.trivia && self.node == other.node
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub items: Vec<Node<Item>>,
    /// Comments after the last item.
    pub tail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    World(World),
    Lineage(Lineage),
    Ruleset(Ruleset),
    View(View),
    Chart(Chart),
    Explore(Explore),
    Patch(Patch),
}

// ---------- literals and expressions ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    /// The literal's digits without `_`, with its sign: `-180.0`, `1e-3`.
    Float(String),
    /// µ.
    Amount(u64),
    /// Basis points.
    Rate(u64),
    Bool(bool),
    Range(Box<Lit>, Box<Lit>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Seg {
    Field(String),
    Label(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// `self` or a name.
    pub root: String,
    pub segs: Vec<Seg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: String,
    pub args: Vec<Arg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    /// `None` for the subject argument.
    pub name: Option<String>,
    pub value: Value,
}

/// `cterm := INT? IDENT`
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    pub coeff: Option<i64>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    pub inputs: Vec<Term>,
    pub outputs: Vec<Term>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Expr(Expr),
    Formula(Formula),
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Lit),
    Path(Path),
    Call(Call),
    /// List items are expressions or formulas.
    List(Vec<Value>),
    Capture(String),
    Paren(Box<Expr>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Pipe(Box<Expr>, Call),
}

// ---------- world ----------

#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub name: String,
    pub decls: Vec<Node<WDecl>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SinkAccepts {
    Any,
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WDecl {
    Element(String),
    /// `const NAME = expr;`, worked out by `resolve` before checking.
    Const { name: String, value: Expr },
    Substance { name: String, comp: Vec<Term>, potential: i64 },
    /// `holds: None` is a signal field, whose writes combine by `combine` if several
    /// rules write it (`field scent : Grid(...) signal combine max;`).
    Field { name: String, kind: Call, holds: Option<Vec<(String, Lit)>>, combine: Option<String> },
    Source { name: String, emits: Vec<String> },
    Sink { name: String, accepts: SinkAccepts },
    Waste { substance: String, sink: String },
    Reaction { name: String, formula: Formula, catalyze: Option<Call> },
    Menu { kind: String, items: Vec<Node<MenuItem>> },
    Rule { name: String, on: String, body: Block },
    Genesis(Block),
    /// `second_law`, `reaction_space`, `upkeep`, `limits`.
    Config { name: String, entries: Vec<Node<Entry>> },
    /// `space`, `remains`, `max_age`, `child_offset`, `mind_steps`.
    Setting(Entry),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Range(Lit),
    Any(String),
    Choices(Vec<String>),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuItem {
    Sense { name: String, params: Vec<(String, Param)> },
    Action { name: String, params: Vec<(String, Param)>, cost: Lit, per: Option<String>, requires: Option<Expr> },
    Gene { name: String, params: Vec<(String, Param)>, inputs: Lit, weight: Option<Lit> },
}

pub type Block = Vec<Node<Stmt>>;

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let { name: String, value: Expr },
    When { cond: Expr, then: Block, otherwise: Option<Block> },
    For { var: String, field: String, body: Block },
    Assign { target: Path, value: Expr },
    Call { call: Call, label: Option<String> },
    /// Lineage genesis: `count: 2_000;`
    Setting { key: String, value: Expr },
}

// ---------- lineages and genomes ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Lineage {
    pub name: String,
    /// `lineage B extends A in W`: the lineage `B` starts from (see `expand`).
    pub base: Option<String>,
    pub world: String,
    pub decls: Vec<Node<LDecl>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LDecl {
    Profile { name: String, entries: Vec<Node<Entry>> },
    Mutation(Vec<Node<Entry>>),
    Seed { signals: Vec<Node<(String, String)>>, modules: Vec<Node<Module>> },
    Genesis(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub dormant: bool,
    pub name: String,
    pub genes: Vec<Node<GeneDecl>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Signal(String),
    /// The float literal, with its sign.
    Const(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    /// The weight literal, with its sign; `None` is `1.0`.
    pub weight: Option<String>,
    pub signal: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeneBody {
    Sense { call: Call, outs: Vec<String> },
    Regulate { call: Call, inputs: Vec<Input>, out: String },
    Reaction { name: String, rate: Lit, source: Option<Source> },
    Action { call: Call, source: Option<Source>, threshold: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneDecl {
    pub dormant: bool,
    pub body: GeneBody,
    pub label: Option<String>,
    /// `after @label`, in a lineage that extends another: where the gene goes in the module.
    pub after: Option<String>,
}

// ---------- rulesets, views, charts, explore, patches ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Ruleset {
    pub name: String,
    pub rules: Vec<Node<RewriteRule>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: String,
    /// A name or a capture (`$kind`).
    pub item: String,
    pub capture: bool,
    pub args: Vec<(String, Expr)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RewriteRule {
    pub name: String,
    pub weight: String,
    pub from: Pattern,
    pub guard: Option<Expr>,
    pub to: Pattern,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub name: String,
    pub world: String,
    pub layers: Vec<Node<Layer>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    pub source: Call,
    pub entries: Vec<Node<Entry>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub name: String,
    pub world: String,
    pub series: Vec<Node<Call>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExploreItem {
    Vary { path: Path, values: Expr },
    Entry(Entry),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Explore {
    pub name: String,
    pub items: Vec<Node<ExploreItem>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatchOp {
    Set { path: Path, value: Value },
    Add { position: Option<(bool, Path)>, decl: Box<AddItem> },
    Replace { path: Path, body: Block },
    Remove { path: Path, move_to: Option<String> },
    Rename { path: Path, to: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AddItem {
    Decl(WDecl),
    Gene(GeneDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    pub target: String,
    pub base: i64,
    pub ops: Vec<Node<PatchOp>>,
}
