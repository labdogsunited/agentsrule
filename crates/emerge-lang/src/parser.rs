//! Recursive-descent parser for the grammar of `language.md` §11.

use crate::ast::*;
use crate::lexer::{Tok, Token};
use crate::Diag;

const HARD: &[&str] = &["self", "true", "false", "and", "or", "not"];

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

type R<T> = Result<T, Diag>;

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, pos: 0 }
    }

    // ---------- helpers ----------

    fn tok(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn tok_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn err<T>(&self, code: &'static str, message: impl Into<String>) -> R<T> {
        let t = &self.toks[self.pos];
        Err(Diag { code, line: t.line, col: t.col, message: message.into() })
    }

    fn expected<T>(&self, what: &str) -> R<T> {
        self.err("E0001", format!("expected {what}, found {}", describe(self.tok())))
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.tok(), Tok::Punct(q) if *q == p)
    }

    fn is_punct_at(&self, k: usize, p: &str) -> bool {
        matches!(self.tok_at(k), Tok::Punct(q) if *q == p)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> R<()> {
        if self.eat_punct(p) { Ok(()) } else { self.expected(&format!("`{p}`")) }
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.tok(), Tok::Ident(x) if x == w)
    }

    fn is_word_at(&self, k: usize, w: &str) -> bool {
        matches!(self.tok_at(k), Tok::Ident(x) if x == w)
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.is_word(w) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, w: &str) -> R<()> {
        if self.eat_word(w) { Ok(()) } else { self.expected(&format!("`{w}`")) }
    }

    fn is_ident(&self) -> bool {
        matches!(self.tok(), Tok::Ident(x) if !HARD.contains(&x.as_str()))
    }

    fn ident(&mut self) -> R<String> {
        match self.tok().clone() {
            Tok::Ident(x) if !HARD.contains(&x.as_str()) => {
                self.bump();
                Ok(x)
            }
            _ => self.expected("a name"),
        }
    }

    fn lit(&mut self) -> R<Lit> {
        match self.tok().clone() {
            Tok::Lit(l) => {
                self.bump();
                Ok(l)
            }
            _ => self.expected("a literal"),
        }
    }

    fn range(&mut self) -> R<Lit> {
        match self.tok().clone() {
            Tok::Lit(l @ Lit::Range(..)) => {
                self.bump();
                Ok(l)
            }
            _ => self.expected("a range"),
        }
    }

    fn amount(&mut self) -> R<Lit> {
        match self.tok().clone() {
            Tok::Lit(l @ Lit::Amount(_)) => {
                self.bump();
                Ok(l)
            }
            _ => self.expected("an amount"),
        }
    }

    /// A float literal with an optional leading minus, as text.
    fn signed_float(&mut self) -> R<String> {
        let neg = self.eat_punct("-");
        match self.tok().clone() {
            Tok::Lit(Lit::Float(f)) => {
                self.bump();
                Ok(if neg { format!("-{f}") } else { f })
            }
            _ => self.expected("a float"),
        }
    }

    fn label(&mut self) -> Option<String> {
        if let Tok::Label(l) = self.tok().clone() {
            self.bump();
            Some(l)
        } else {
            None
        }
    }

    fn node<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<Node<T>> {
        let t = &self.toks[self.pos];
        let pos = Pos { line: t.line, col: t.col };
        let mut trivia = Trivia { comments: t.comments.clone(), blank_before: t.blank_before, trailing: None };
        let node = f(self)?;
        trivia.trailing = self.toks[self.pos - 1].trailing.clone();
        Ok(Node { trivia, pos, node })
    }

    fn sep<T>(&mut self, close: &str, mut f: impl FnMut(&mut Self) -> R<T>) -> R<Vec<T>> {
        let mut out = Vec::new();
        if self.eat_punct(close) {
            return Ok(out);
        }
        loop {
            out.push(f(self)?);
            if self.eat_punct(close) {
                return Ok(out);
            }
            self.expect(",")?;
        }
    }

    fn many<T>(&mut self, mut f: impl FnMut(&mut Self) -> R<T>) -> R<Vec<Node<T>>> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.is_punct("}") {
            if *self.tok() == Tok::Eof {
                return self.expected("`}`");
            }
            out.push(self.node(&mut f)?);
        }
        if !self.toks[self.pos].comments.is_empty() {
            return self.err("E0007", "a comment at the end of a block is not supported yet; put it before a statement");
        }
        self.bump();
        Ok(out)
    }

    // ---------- file ----------

    pub fn file(&mut self) -> R<File> {
        let mut items = Vec::new();
        while *self.tok() != Tok::Eof {
            items.push(self.node(Self::item)?);
        }
        let eof = &self.toks[self.pos];
        let mut tail = eof.comments.clone();
        if eof.blank_before && !tail.is_empty() {
            tail.insert(0, String::new());
        }
        Ok(File { items, tail })
    }

    fn item(&mut self) -> R<Item> {
        let word = match self.tok() {
            Tok::Ident(w) => w.clone(),
            _ => return self.expected("an item (`world`, `lineage`, ...)"),
        };
        self.bump();
        Ok(match word.as_str() {
            "world" => {
                let name = self.ident()?;
                Item::World(World { name, decls: self.many(Self::wdecl)? })
            }
            "lineage" => {
                let name = self.ident()?;
                let base = if self.eat_word("extends") { Some(self.ident()?) } else { None };
                self.expect_word("in")?;
                let world = self.ident()?;
                Item::Lineage(Lineage { name, base, world, decls: self.many(Self::ldecl)? })
            }
            "ruleset" => {
                let name = self.ident()?;
                Item::Ruleset(Ruleset { name, rules: self.many(Self::rewrite_rule)? })
            }
            "view" => {
                let name = self.ident()?;
                self.expect_word("for")?;
                let world = self.ident()?;
                Item::View(View { name, world, layers: self.many(Self::layer)? })
            }
            "chart" => {
                let name = self.ident()?;
                self.expect_word("for")?;
                let world = self.ident()?;
                let series = self.many(|p| {
                    let c = p.call()?;
                    p.expect(";")?;
                    Ok(c)
                })?;
                Item::Chart(Chart { name, world, series })
            }
            "explore" => {
                let name = self.ident()?;
                Item::Explore(Explore { name, items: self.many(Self::explore_item)? })
            }
            "patch" => {
                let target = self.ident()?;
                let base = if self.eat_word("base") {
                    match self.lit()? {
                        Lit::Int(i) => Some(i),
                        _ => return self.expected("a revision number"),
                    }
                } else {
                    None
                };
                Item::Patch(Patch { target, base, ops: self.many(Self::patch_op)? })
            }
            other => return self.err("E0002", format!("unknown item `{other}`")),
        })
    }

    // ---------- world ----------

    fn wdecl(&mut self) -> R<WDecl> {
        let word = match self.tok() {
            Tok::Ident(w) => w.clone(),
            _ => return self.expected("a declaration"),
        };
        if self.is_punct_at(1, ":") {
            return Ok(WDecl::Setting(self.entry()?));
        }
        if self.is_punct_at(1, "{") && !matches!(word.as_str(), "senses" | "actions" | "regulation" | "genesis") {
            self.bump();
            return Ok(WDecl::Config { name: word, entries: self.many(Self::entry)? });
        }
        self.bump();
        let decl = match word.as_str() {
            "element" => WDecl::Element(self.ident()?),
            "const" => {
                let name = self.ident()?;
                self.expect("=")?;
                WDecl::Const { name, value: self.expr()? }
            }
            "substance" => {
                let name = self.ident()?;
                self.expect("=")?;
                let comp = self.comp()?;
                self.expect_word("potential")?;
                let neg = self.eat_punct("-");
                let potential = match self.lit()? {
                    Lit::Int(i) => if neg { -i } else { i },
                    _ => return self.expected("an integer potential"),
                };
                WDecl::Substance { name, comp, potential }
            }
            "field" => {
                let name = self.ident()?;
                self.expect(":")?;
                let kind = self.call()?;
                let (holds, combine) = if self.eat_word("holds") {
                    self.expect("[")?;
                    let holds = self.sep("]", |p| {
                        let s = p.ident()?;
                        p.expect(":")?;
                        Ok((s, p.amount()?))
                    })?;
                    (Some(holds), None)
                } else {
                    self.expect_word("signal")?;
                    (None, if self.eat_word("combine") { Some(self.ident()?) } else { None })
                };
                WDecl::Field { name, kind, holds, combine }
            }
            "source" => {
                let name = self.ident()?;
                self.expect(":")?;
                self.expect("[")?;
                WDecl::Source { name, emits: self.sep("]", Self::ident)? }
            }
            "sink" => {
                let name = self.ident()?;
                self.expect(":")?;
                let accepts = if self.eat_word("any") {
                    SinkAccepts::Any
                } else {
                    self.expect("[")?;
                    SinkAccepts::List(self.sep("]", Self::ident)?)
                };
                WDecl::Sink { name, accepts }
            }
            "waste" => {
                let substance = self.ident()?;
                self.expect("->")?;
                WDecl::Waste { substance, sink: self.ident()? }
            }
            "reaction" => {
                let name = self.ident()?;
                self.expect(":")?;
                let formula = self.formula()?;
                let catalyze = if self.eat_word("catalyze") { Some(self.call()?) } else { None };
                WDecl::Reaction { name, formula, catalyze }
            }
            "senses" | "actions" | "regulation" => {
                return Ok(WDecl::Menu { kind: word, items: self.many(Self::menu_item)? });
            }
            "rule" => {
                let name = self.ident()?;
                self.expect_word("on")?;
                let on = self.ident()?;
                return Ok(WDecl::Rule { name, on, body: self.block()? });
            }
            "genesis" => return Ok(WDecl::Genesis(self.block()?)),
            other => return self.err("E0003", format!("unknown declaration `{other}`")),
        };
        self.expect(";")?;
        Ok(decl)
    }

    fn comp(&mut self) -> R<Vec<Term>> {
        let mut terms = vec![self.term()?];
        while self.eat_punct("+") {
            terms.push(self.term()?);
        }
        Ok(terms)
    }

    fn term(&mut self) -> R<Term> {
        let coeff = match self.tok().clone() {
            Tok::Lit(Lit::Int(i)) => {
                self.bump();
                Some(i)
            }
            _ => None,
        };
        Ok(Term { coeff, name: self.ident()? })
    }

    fn formula(&mut self) -> R<Formula> {
        let inputs = self.comp()?;
        self.expect("->")?;
        Ok(Formula { inputs, outputs: self.comp()? })
    }

    fn params(&mut self) -> R<Vec<(String, Param)>> {
        self.expect("(")?;
        self.sep(")", |p| {
            let name = p.ident()?;
            p.expect(":")?;
            let param = match p.tok().clone() {
                Tok::Lit(l @ Lit::Range(..)) => {
                    p.bump();
                    Param::Range(l)
                }
                Tok::Punct("[") => {
                    p.bump();
                    Param::Choices(p.sep("]", Self::ident)?)
                }
                Tok::Ident(w) if w == "any" => {
                    p.bump();
                    Param::Any(p.ident()?)
                }
                _ => Param::Name(p.ident()?),
            };
            Ok((name, param))
        })
    }

    fn menu_item(&mut self) -> R<MenuItem> {
        let item = if self.eat_word("sense") {
            let name = self.ident()?;
            MenuItem::Sense { name, params: self.params()? }
        } else if self.eat_word("action") {
            let name = self.ident()?;
            let params = self.params()?;
            self.expect_word("cost")?;
            let cost = self.amount()?;
            let per = if self.eat_word("per") { Some(self.ident()?) } else { None };
            let requires = if self.eat_word("requires") { Some(self.expr()?) } else { None };
            MenuItem::Action { name, params, cost, per, requires }
        } else if self.eat_word("gene") {
            let name = self.ident()?;
            let params = self.params()?;
            self.expect_word("inputs")?;
            let inputs = self.range()?;
            let weight = if self.eat_word("weight") { Some(self.range()?) } else { None };
            MenuItem::Gene { name, params, inputs, weight }
        } else {
            return self.expected("`sense`, `action` or `gene`");
        };
        self.expect(";")?;
        Ok(item)
    }

    fn entry(&mut self) -> R<Entry> {
        let key = self.ident()?;
        self.expect(":")?;
        let value = self.value()?;
        self.expect(";")?;
        Ok(Entry { key, value })
    }

    // ---------- statements ----------

    fn block(&mut self) -> R<Block> {
        self.many(Self::stmt)
    }

    fn stmt(&mut self) -> R<Stmt> {
        if self.eat_word("let") {
            let name = self.ident()?;
            self.expect("=")?;
            let value = self.expr()?;
            self.expect(";")?;
            return Ok(Stmt::Let { name, value });
        }
        if self.eat_word("when") {
            let cond = self.expr()?;
            let then = self.block()?;
            let otherwise = if self.eat_word("else") { Some(self.block()?) } else { None };
            return Ok(Stmt::When { cond, then, otherwise });
        }
        if self.is_word("for") && self.is_word_at(2, "in") {
            self.bump();
            let var = self.ident()?;
            self.expect_word("in")?;
            let field = self.ident()?;
            return Ok(Stmt::For { var, field, body: self.block()? });
        }
        if self.is_ident() && self.is_punct_at(1, ":") {
            let key = self.ident()?;
            self.bump();
            let value = self.expr()?;
            self.expect(";")?;
            return Ok(Stmt::Setting { key, value });
        }
        if self.is_ident() && self.is_punct_at(1, "(") {
            let call = self.call()?;
            let label = self.label();
            self.expect(";")?;
            return Ok(Stmt::Call { call, label });
        }
        let target = self.path()?;
        self.expect("=")?;
        let value = self.expr()?;
        self.expect(";")?;
        Ok(Stmt::Assign { target, value })
    }

    // ---------- expressions ----------

    fn value(&mut self) -> R<Value> {
        if self.is_word("any") && !self.is_punct_at(1, "(") && !self.is_punct_at(1, ".") {
            self.bump();
            return Ok(Value::Any);
        }
        if self.formula_ahead() {
            return Ok(Value::Formula(self.formula()?));
        }
        Ok(Value::Expr(self.expr()?))
    }

    /// Whether a `->` comes before the next `,`, `;`, `)` or `]` at this nesting level.
    fn formula_ahead(&self) -> bool {
        let mut depth = 0i32;
        for t in &self.toks[self.pos..] {
            match &t.tok {
                Tok::Punct("(" | "[" | "{") => depth += 1,
                Tok::Punct(")" | "]" | "}") if depth == 0 => return false,
                Tok::Punct(")" | "]" | "}") => depth -= 1,
                Tok::Punct("," | ";") if depth == 0 => return false,
                Tok::Punct("->") if depth == 0 => return true,
                Tok::Eof => return false,
                _ => {}
            }
        }
        false
    }

    pub fn expr(&mut self) -> R<Expr> {
        let mut e = self.or_expr()?;
        while self.eat_punct("|>") {
            e = Expr::Pipe(Box::new(e), self.call()?);
        }
        Ok(e)
    }

    fn or_expr(&mut self) -> R<Expr> {
        let mut e = self.and_expr()?;
        while self.eat_word("or") {
            e = Expr::Binary(BinOp::Or, Box::new(e), Box::new(self.and_expr()?));
        }
        Ok(e)
    }

    fn and_expr(&mut self) -> R<Expr> {
        let mut e = self.not_expr()?;
        while self.eat_word("and") {
            e = Expr::Binary(BinOp::And, Box::new(e), Box::new(self.not_expr()?));
        }
        Ok(e)
    }

    fn not_expr(&mut self) -> R<Expr> {
        if self.eat_word("not") {
            return Ok(Expr::Not(Box::new(self.not_expr()?)));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> R<Expr> {
        let l = self.additive()?;
        let op = match self.tok() {
            Tok::Punct("==") => BinOp::Eq,
            Tok::Punct("!=") => BinOp::Ne,
            Tok::Punct("<") => BinOp::Lt,
            Tok::Punct("<=") => BinOp::Le,
            Tok::Punct(">") => BinOp::Gt,
            Tok::Punct(">=") => BinOp::Ge,
            _ => return Ok(l),
        };
        self.bump();
        let r = self.additive()?;
        if matches!(self.tok(), Tok::Punct("==" | "!=" | "<" | "<=" | ">" | ">=")) {
            return self.err("E0004", "comparisons do not chain");
        }
        Ok(Expr::Binary(op, Box::new(l), Box::new(r)))
    }

    fn additive(&mut self) -> R<Expr> {
        let mut e = self.multiplicative()?;
        loop {
            let op = match self.tok() {
                Tok::Punct("+") => BinOp::Add,
                Tok::Punct("-") => BinOp::Sub,
                _ => return Ok(e),
            };
            self.bump();
            e = Expr::Binary(op, Box::new(e), Box::new(self.multiplicative()?));
        }
    }

    fn multiplicative(&mut self) -> R<Expr> {
        let mut e = self.unary()?;
        loop {
            let op = match self.tok() {
                Tok::Punct("*") => BinOp::Mul,
                Tok::Punct("/") => BinOp::Div,
                _ => return Ok(e),
            };
            self.bump();
            e = Expr::Binary(op, Box::new(e), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> R<Expr> {
        if self.eat_punct("-") {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> R<Expr> {
        match self.tok().clone() {
            Tok::Lit(l) => {
                self.bump();
                Ok(Expr::Lit(l))
            }
            Tok::Ident(w) if w == "true" || w == "false" => {
                self.bump();
                Ok(Expr::Lit(Lit::Bool(w == "true")))
            }
            Tok::Capture(c) => {
                self.bump();
                Ok(Expr::Capture(c))
            }
            Tok::Punct("(") => {
                self.bump();
                let e = self.expr()?;
                self.expect(")")?;
                Ok(Expr::Paren(Box::new(e)))
            }
            Tok::Punct("[") => {
                self.bump();
                Ok(Expr::List(self.sep("]", Self::value)?))
            }
            Tok::Ident(_) if self.is_punct_at(1, "(") => Ok(Expr::Call(self.call()?)),
            Tok::Ident(_) => Ok(Expr::Path(self.path()?)),
            _ => self.expected("an expression"),
        }
    }

    fn call(&mut self) -> R<Call> {
        let name = self.ident()?;
        self.expect("(")?;
        let mut first = true;
        let args = self.sep(")", |p| {
            let named = p.is_ident() && p.is_punct_at(1, ":");
            let arg = if named {
                let name = p.ident()?;
                p.bump();
                Arg { name: Some(name), value: p.value()? }
            } else if first {
                Arg { name: None, value: Value::Expr(p.expr()?) }
            } else {
                return p.err("E0005", "only the first argument may be unnamed");
            };
            first = false;
            Ok(arg)
        })?;
        Ok(Call { name, args })
    }

    fn path(&mut self) -> R<Path> {
        let root = match self.tok().clone() {
            Tok::Ident(w) if w == "self" || !HARD.contains(&w.as_str()) => {
                self.bump();
                w
            }
            _ => return self.expected("a path"),
        };
        let mut segs = Vec::new();
        loop {
            if self.is_punct(".") && matches!(self.tok_at(1), Tok::Ident(_)) {
                self.bump();
                let Tok::Ident(name) = self.bump() else { unreachable!() };
                segs.push(Seg::Field(name));
            } else if let Some(l) = self.label() {
                segs.push(Seg::Label(l));
            } else {
                return Ok(Path { root, segs });
            }
        }
    }

    // ---------- lineages ----------

    fn ldecl(&mut self) -> R<LDecl> {
        if self.eat_word("profile") {
            let name = self.ident()?;
            return Ok(LDecl::Profile { name, entries: self.many(Self::entry)? });
        }
        if self.eat_word("mutation") {
            return Ok(LDecl::Mutation(self.many(Self::entry)?));
        }
        if self.eat_word("genesis") {
            return Ok(LDecl::Genesis(self.block()?));
        }
        if self.eat_word("seed") {
            self.expect_word("genome")?;
            self.expect("{")?;
            let mut signals = Vec::new();
            while self.is_word("signal") {
                signals.push(self.node(|p| {
                    p.bump();
                    let name = p.ident()?;
                    p.expect_word("keep")?;
                    let keep = p.signed_float()?;
                    p.expect(";")?;
                    Ok((name, keep))
                })?);
            }
            let mut modules = Vec::new();
            while !self.is_punct("}") {
                modules.push(self.node(Self::module)?);
            }
            self.bump();
            if modules.is_empty() {
                return self.err("E0006", "a seed genome needs at least one module");
            }
            return Ok(LDecl::Seed { signals, modules });
        }
        self.expected("`profile`, `mutation`, `seed genome` or `genesis`")
    }

    fn module(&mut self) -> R<Module> {
        let dormant = self.eat_word("dormant");
        self.expect_word("module")?;
        let name = self.ident()?;
        Ok(Module { dormant, name, genes: self.many(Self::gene)? })
    }

    fn source(&mut self) -> R<Source> {
        if self.is_ident() {
            Ok(Source::Signal(self.ident()?))
        } else {
            Ok(Source::Const(self.signed_float()?))
        }
    }

    fn gene(&mut self) -> R<GeneDecl> {
        let dormant = self.eat_word("dormant");
        let body = if self.eat_word("sense") {
            let call = self.call()?;
            self.expect("->")?;
            let mut outs = vec![self.ident()?];
            if self.eat_punct(",") {
                outs.push(self.ident()?);
            }
            GeneBody::Sense { call, outs }
        } else if self.eat_word("regulate") {
            let call = self.call()?;
            self.expect("<-")?;
            self.expect("[")?;
            let inputs = self.sep("]", |p| {
                let weight = if p.is_ident() {
                    None
                } else {
                    let w = p.signed_float()?;
                    p.expect("*")?;
                    Some(w)
                };
                Ok(Input { weight, signal: p.ident()? })
            })?;
            self.expect("->")?;
            GeneBody::Regulate { call, inputs, out: self.ident()? }
        } else if self.eat_word("reaction") {
            let name = self.ident()?;
            self.expect_word("rate")?;
            let rate = self.amount()?;
            let source = if self.eat_punct("<-") { Some(self.source()?) } else { None };
            GeneBody::Reaction { name, rate, source }
        } else if self.eat_word("action") {
            let call = self.call()?;
            let source = if self.eat_punct("<-") { Some(self.source()?) } else { None };
            let threshold = if self.eat_word("threshold") { Some(self.signed_float()?) } else { None };
            GeneBody::Action { call, source, threshold }
        } else {
            return self.expected("a gene (`sense`, `regulate`, `reaction` or `action`)");
        };
        let label = self.label();
        let after = if self.eat_word("after") {
            match self.label() {
                Some(l) => Some(l),
                None => return self.expected("a gene label such as `@respiration`"),
            }
        } else {
            None
        };
        self.expect(";")?;
        Ok(GeneDecl { dormant, body, label, after })
    }

    // ---------- rulesets, views, explore, patches ----------

    fn pattern(&mut self) -> R<Pattern> {
        let kind = match self.tok() {
            Tok::Ident(w) if matches!(w.as_str(), "sense" | "regulate" | "reaction" | "action") => w.clone(),
            _ => return self.expected("a gene kind"),
        };
        self.bump();
        let (item, capture) = match self.tok().clone() {
            Tok::Capture(c) => {
                self.bump();
                (c, true)
            }
            _ => (self.ident()?, false),
        };
        self.expect("(")?;
        let args = self.sep(")", |p| {
            let name = p.ident()?;
            p.expect(":")?;
            Ok((name, p.expr()?))
        })?;
        Ok(Pattern { kind, item, capture, args })
    }

    fn rewrite_rule(&mut self) -> R<RewriteRule> {
        self.expect_word("rule")?;
        let name = self.ident()?;
        self.expect_word("weight")?;
        let weight = self.signed_float()?;
        self.expect(":")?;
        let from = self.pattern()?;
        let guard = if self.eat_word("where") { Some(self.expr()?) } else { None };
        self.expect("->")?;
        let to = self.pattern()?;
        self.expect(";")?;
        Ok(RewriteRule { name, weight, from, guard, to })
    }

    fn layer(&mut self) -> R<Layer> {
        self.expect_word("layer")?;
        let name = self.ident()?;
        self.expect("=")?;
        let source = self.call()?;
        Ok(Layer { name, source, entries: self.many(Self::entry)? })
    }

    fn explore_item(&mut self) -> R<ExploreItem> {
        if self.is_word("vary") && !self.is_punct_at(1, ":") {
            self.bump();
            let path = self.path()?;
            self.expect_word("in")?;
            let values = self.expr()?;
            self.expect(";")?;
            return Ok(ExploreItem::Vary { path, values });
        }
        Ok(ExploreItem::Entry(self.entry()?))
    }

    fn patch_op(&mut self) -> R<PatchOp> {
        if self.eat_word("set") {
            let path = self.path()?;
            self.expect("=")?;
            let value = self.value()?;
            let move_to = if self.eat_word("move_to") {
                self.expect(":")?;
                Some(self.ident()?)
            } else {
                None
            };
            self.expect(";")?;
            return Ok(PatchOp::Set { path, value, move_to });
        }
        if self.eat_word("add") {
            let position = if self.is_word("after") || self.is_word("before") {
                let after = self.is_word("after");
                self.bump();
                let path = self.path()?;
                self.expect(":")?;
                Some((after, path))
            } else {
                None
            };
            let gene_ahead = self.is_word("dormant")
                || self.is_word("sense")
                || self.is_word("regulate")
                || self.is_word("action")
                || (self.is_word("reaction") && self.is_word_at(2, "rate"));
            let menu_ahead = matches!(&position, Some((_, p)) if matches!(p.root.as_str(), "senses" | "actions" | "regulation"));
            let decl = if menu_ahead {
                AddItem::MenuItem(self.menu_item()?)
            } else if gene_ahead {
                AddItem::Gene(self.gene()?)
            } else {
                AddItem::Decl(self.wdecl()?)
            };
            return Ok(PatchOp::Add { position, decl: Box::new(decl) });
        }
        if self.eat_word("replace") {
            let path = self.path()?;
            return Ok(PatchOp::Replace { path, body: self.block()? });
        }
        if self.eat_word("remove") {
            let path = self.path()?;
            let move_to = if self.eat_word("move_to") {
                self.expect(":")?;
                Some(self.ident()?)
            } else {
                None
            };
            self.expect(";")?;
            return Ok(PatchOp::Remove { path, move_to });
        }
        if self.eat_word("rename") {
            let path = self.path()?;
            self.expect_word("to")?;
            let to = self.ident()?;
            self.expect(";")?;
            return Ok(PatchOp::Rename { path, to });
        }
        self.expected("a patch operation")
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(w) => format!("`{w}`"),
        Tok::Lit(_) => "a literal".into(),
        Tok::Label(l) => format!("`@{l}`"),
        Tok::Capture(c) => format!("`${c}`"),
        Tok::Punct(p) => format!("`{p}`"),
        Tok::Eof => "the end of the file".into(),
    }
}
