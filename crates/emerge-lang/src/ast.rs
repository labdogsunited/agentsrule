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
    /// `"…"`: printable characters other than `"` and `\\`, on one line, with no escapes.
    Str(String),
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
    /// A field holds substances (`holds`), one `f32` (`signal`), or one of named states (`states`).
    Field { name: String, kind: Call, holds: Option<Vec<(String, Lit)>>, combine: Option<String>, states: Option<Vec<String>> },
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
    /// `operator NAME version N { param…; rule… }`: a reusable definition (`proposals/reusable-operators.md`), written
    /// out by `instantiate` wherever a `use` names it. Its body holds `rule` declarations only.
    Operator { name: String, version: i64, params: Vec<Node<OParam>>, rules: Vec<Node<WDecl>> },
    /// `use NAME version N as INSTANCE(arg: value, …);`: an instance of a definition.
    Use { def: String, version: i64, name: String, args: Vec<Arg> },
}

/// A parameter of an operator: a kind (`substance`, `source`, `sink`, `field`, `reaction`, `amount`, `rate`, `int`,
/// `float`) and an optional default.
#[derive(Debug, Clone, PartialEq)]
pub struct OParam {
    pub name: String,
    pub kind: String,
    pub default: Option<Value>,
}

/// The kinds an operator parameter may have: the names first, then the values.
pub const PARAM_KINDS: [&str; 9] = ["substance", "source", "sink", "field", "reaction", "amount", "rate", "int", "float"];

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
    /// `for s in [carbon, nitrate] { ... }` in a world rule: the body written once for each name, worked out by `unroll`.
    Each { var: String, items: Vec<String>, body: Block },
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
    /// VEX-like statements (`vex-language.md` §7), after the entries. An attribute such as
    /// `@energy` is read as a `Path` whose root is `@energy`.
    pub body: Vec<Node<VStmt>>,
}

/// A statement of a VEX-like layer body.
#[derive(Debug, Clone, PartialEq)]
pub enum VStmt {
    /// `float fuel = …;`
    Local { ty: String, name: String, value: Expr },
    /// `@Cd = …;`: an attribute of the current element (`attr` without the `@`).
    Set { attr: String, value: Expr },
    /// `if (cond) …` with an optional `else`.
    If { cond: Expr, then: Vec<Node<VStmt>>, otherwise: Option<Vec<Node<VStmt>>> },
}

/// The types a local of a layer body may have.
pub const VTYPES: [&str; 6] = ["float", "int", "bool", "vector2", "color", "string"];

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
    /// `move_to`: where the excess goes when the value lowers a cap below current balances (a sink).
    Set { path: Path, value: Value, move_to: Option<String> },
    Add { position: Option<(bool, Path)>, decl: Box<AddItem> },
    Replace { path: Path, body: Block },
    Remove { path: Path, move_to: Option<String> },
    Rename { path: Path, to: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AddItem {
    Decl(WDecl),
    /// An entry of a menu, added `after` or `before` another entry of it.
    MenuItem(MenuItem),
    Gene(GeneDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    pub target: String,
    /// The revision the patch was written against; a patch applied to a file has none.
    pub base: Option<i64>,
    pub ops: Vec<Node<PatchOp>>,
}
