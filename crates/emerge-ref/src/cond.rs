//! Conditions of world rules (`proposals/rule-conditions.md`, stage 1 and stage 2a): the `when`
//! around an effect statement, evaluated in each cell on the start-of-tick snapshot.
//!
//! Everything is an exact integer (an amount is its µ), so there is no rounding and nothing that
//! could differ between runners. A condition is typed when it is built (`build`): integers
//! compare with integers, amounts with amounts, booleans combine with `and`, `or` and `not`.
//! Stage 2a adds the neighbouring cells: `above.<s>` and the others, and `sum`, `min`, `max` and
//! `count` over `moore.<s>` or `vonneumann.<s>`.

use crate::chem::{Mu, SubstanceId};
use emerge_lang::ast::{BinOp, Call, Expr, Lit, Path, Seg, Value};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Int,
    Amount,
    Bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A reduction over the neighbours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Red {
    Sum,
    Min,
    Max,
    Count,
}

/// Which neighbours a collection holds: the first 4 of `OFFSETS` or all 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Set {
    VonNeumann,
    Moore,
}

impl Set {
    pub fn len(self) -> usize {
        match self {
            Set::VonNeumann => 4,
            Set::Moore => 8,
        }
    }
}

/// The neighbours of a cell, in this order: N, E, S, W, NE, SE, SW, NW (row 0 at the top).
pub const OFFSETS: [(isize, isize); 8] = [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];

/// The neighbour of cell `c` in direction `d` (`OFFSETS`) on a `w × h` grid: wrapped if the grid
/// wraps, else `None` outside it. On a grid narrower than 3 that wraps, it can be `c` itself.
pub fn neighbour(w: usize, h: usize, wrap: bool, c: usize, d: usize) -> Option<usize> {
    let (dx, dy) = OFFSETS[d];
    let (x, y) = ((c % w) as isize + dx, (c / w) as isize + dy);
    let (w, h) = (w as isize, h as isize);
    let (x, y) = if wrap { (x.rem_euclid(w), y.rem_euclid(h)) } else { (x, y) };
    ((0..w).contains(&x) && (0..h).contains(&y)).then(|| (y * w + x) as usize)
}

/// An expression over the cell and the tick. A boolean is 0 or 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ex {
    /// An integer, or an amount in µ.
    Const(i64),
    Tick,
    /// `self.<substance>`: the cell's amount at the start of the tick.
    Own(SubstanceId),
    /// `cap(<substance>)`: the field's cap.
    Cap(SubstanceId),
    /// `above`, `right`, `below`, `left` (`OFFSETS` 0 to 3) `.<substance>`: 0 outside the grid.
    Nb(usize, SubstanceId),
    /// `moore.<substance>` or `vonneumann.<substance>` inside a reduction: the neighbour's amount.
    Elem(SubstanceId),
    /// A reduction of its body, evaluated for each neighbour of the set that exists, in `OFFSETS`
    /// order. `min` and `max` of no neighbour are 0.
    Reduce(Red, Set, Box<Ex>),
    Add(Box<Ex>, Box<Ex>),
    Sub(Box<Ex>, Box<Ex>),
    Mul(Box<Ex>, Box<Ex>),
    Cmp(Cmp, Box<Ex>, Box<Ex>),
    And(Box<Ex>, Box<Ex>),
    Or(Box<Ex>, Box<Ex>),
    Not(Box<Ex>),
}

/// What a condition reads: the tick, one cell as it was at the start of the tick, and its
/// neighbours (`neighbour(d)`: the amounts of the neighbour in direction `d` of `OFFSETS`, if any).
pub struct Ctx<'a> {
    pub tick: u64,
    pub amounts: &'a [Mu],
    pub caps: &'a [Mu],
    pub neighbour: &'a dyn Fn(usize) -> Option<&'a [Mu]>,
}

impl Ex {
    pub fn eval(&self, c: &Ctx) -> i64 {
        self.eval_at(c, None)
    }

    /// `elem`: the neighbour a reduction's body is evaluated for.
    fn eval_at(&self, c: &Ctx, elem: Option<&[Mu]>) -> i64 {
        let ev = |x: &Ex| x.eval_at(c, elem);
        match self {
            Ex::Const(v) => *v,
            Ex::Tick => c.tick as i64,
            Ex::Own(s) => i64::from(c.amounts[*s]),
            Ex::Cap(s) => i64::from(c.caps[*s]),
            Ex::Nb(d, s) => (c.neighbour)(*d).map_or(0, |a| i64::from(a[*s])),
            Ex::Elem(s) => i64::from(elem.expect("a collection is read inside a reduction only")[*s]),
            Ex::Reduce(red, set, body) => {
                let (mut acc, mut any, mut count) = (0i64, false, 0i64);
                for d in 0..set.len() {
                    let Some(nb) = (c.neighbour)(d) else { continue };
                    let v = body.eval_at(c, Some(nb));
                    match red {
                        Red::Sum => acc = acc.saturating_add(v),
                        Red::Count => count += i64::from(v != 0),
                        Red::Min => acc = if any { acc.min(v) } else { v },
                        Red::Max => acc = if any { acc.max(v) } else { v },
                    }
                    any = true;
                }
                if *red == Red::Count {
                    count
                } else {
                    acc
                }
            }
            Ex::Add(a, b) => ev(a).saturating_add(ev(b)),
            Ex::Sub(a, b) => ev(a).saturating_sub(ev(b)),
            Ex::Mul(a, b) => ev(a).saturating_mul(ev(b)),
            Ex::Cmp(op, a, b) => {
                let (x, y) = (ev(a), ev(b));
                i64::from(match op {
                    Cmp::Eq => x == y,
                    Cmp::Ne => x != y,
                    Cmp::Lt => x < y,
                    Cmp::Le => x <= y,
                    Cmp::Gt => x > y,
                    Cmp::Ge => x >= y,
                })
            }
            Ex::And(a, b) => i64::from(ev(a) != 0 && ev(b) != 0),
            Ex::Or(a, b) => i64::from(ev(a) != 0 || ev(b) != 0),
            Ex::Not(a) => i64::from(ev(a) == 0),
        }
    }

    pub fn holds(&self, c: &Ctx) -> bool {
        self.eval(c) != 0
    }

    /// `self` and `other` both hold.
    pub fn and(self, other: Ex) -> Ex {
        Ex::And(Box::new(self), Box::new(other))
    }

    /// The most values a postfix evaluation holds at once (the GPU's stack, `emerge_gpu::pack`): a
    /// reduction is one value where it stands, and its body is evaluated on a stack of its own.
    pub fn depth(&self) -> usize {
        let (outer, bodies) = self.depths();
        outer.max(bodies)
    }

    /// The depth where it stands, and the deepest reduction body inside it.
    fn depths(&self) -> (usize, usize) {
        match self {
            Ex::Const(_) | Ex::Tick | Ex::Own(_) | Ex::Cap(_) | Ex::Nb(..) | Ex::Elem(_) => (1, 0),
            Ex::Reduce(_, _, body) => (1, body.depth()),
            Ex::Not(a) => a.depths(),
            Ex::Add(a, b) | Ex::Sub(a, b) | Ex::Mul(a, b) | Ex::Cmp(_, a, b) | Ex::And(a, b) | Ex::Or(a, b) => {
                let ((da, ia), (db, ib)) = (a.depths(), b.depths());
                (da.max(1 + db), ia.max(ib))
            }
        }
    }
}

/// The values a condition may hold at once on the GPU: a deeper one runs on the CPU only.
pub const GPU_STACK: usize = 16;

/// An error code and its message.
pub type Fault = (&'static str, String);

fn unsupported(what: &str) -> Fault {
    ("E1001", format!("{what} is not run by emerge-ref in a rule's `when` yet (it reads `self.<substance>`, `cap(<substance>)`, `tick`, `above`/`below`/`left`/`right.<substance>`, and `sum`, `min`, `max`, `count` over `moore` or `vonneumann`; see proposals/rule-conditions.md)"))
}

fn type_error(message: String) -> Fault {
    ("E0205", message)
}

/// The expression typed and built. `substance` resolves a substance name, and `lets` are the names
/// `let` gave earlier in the rule (their expressions are substituted).
pub fn build(e: &Expr, substance: &dyn Fn(&str) -> Option<SubstanceId>, lets: &[(String, (Ex, Ty))]) -> Result<(Ex, Ty), Fault> {
    build_in(e, substance, lets, None)
}

/// `coll`: inside a reduction's body, the collection it reads (set by the first one met); `None`
/// outside a reduction.
fn build_in(e: &Expr, substance: &dyn Fn(&str) -> Option<SubstanceId>, lets: &[(String, (Ex, Ty))], coll: Option<&Cell<Option<Set>>>) -> Result<(Ex, Ty), Fault> {
    let want = |got: Ty, want: Ty, what: &str| if got == want { Ok(()) } else { Err(type_error(format!("{what} must be {want:?}, not {got:?}"))) };
    let sub = |e: &Expr| build_in(e, substance, lets, coll);
    let field = |root: &str, f: &str| substance(f).ok_or_else(|| unsupported(&format!("`{root}.{f}` (only a substance of the field can be read; signals and other fields are stage 2)")));
    Ok(match e {
        Expr::Lit(Lit::Int(i)) => (Ex::Const(*i), Ty::Int),
        Expr::Lit(Lit::Amount(a)) => (Ex::Const(i64::try_from(*a).map_err(|_| type_error("amount too large".into()))?), Ty::Amount),
        Expr::Lit(Lit::Bool(b)) => (Ex::Const(i64::from(*b)), Ty::Bool),
        Expr::Lit(_) => return Err(unsupported("a literal of this kind")),
        Expr::Paren(inner) => sub(inner)?,
        Expr::Neg(inner) => {
            let (x, t) = sub(inner)?;
            want(t, Ty::Int, "a negated value")?;
            (Ex::Sub(Box::new(Ex::Const(0)), Box::new(x)), Ty::Int)
        }
        Expr::Not(inner) => {
            let (x, t) = sub(inner)?;
            want(t, Ty::Bool, "the operand of `not`")?;
            (Ex::Not(Box::new(x)), Ty::Bool)
        }
        Expr::Path(Path { root, segs }) => match (root.as_str(), segs.as_slice()) {
            ("tick", []) => (Ex::Tick, Ty::Int),
            ("self", [Seg::Field(f)]) => (Ex::Own(field("self", f)?), Ty::Amount),
            (dir @ ("above" | "right" | "below" | "left"), [Seg::Field(f)]) => {
                let d = ["above", "right", "below", "left"].iter().position(|&x| x == dir).unwrap();
                (Ex::Nb(d, field(dir, f)?), Ty::Amount)
            }
            (set @ ("moore" | "vonneumann"), [Seg::Field(f)]) => {
                let set = if set == "moore" { Set::Moore } else { Set::VonNeumann };
                let Some(coll) = coll else {
                    return Err(type_error(format!("`{root}.{f}` is a collection of the neighbours; read it inside `sum`, `min`, `max` or `count`")));
                };
                match coll.get() {
                    Some(other) if other != set => return Err(type_error("a reduction reads one kind of collection, `moore` or `vonneumann`, not both".into())),
                    _ => coll.set(Some(set)),
                }
                (Ex::Elem(field(root, f)?), Ty::Amount)
            }
            (name, []) => match lets.iter().rev().find(|(n, _)| n == name) {
                Some((_, (x, t))) => (x.clone(), *t),
                None => return Err(("E0114", format!("unknown name `{name}`"))),
            },
            (root, _) => return Err(unsupported(&format!("`{root}…` (other fields and other reads are stage 2)"))),
        },
        Expr::Call(Call { name, args }) if name == "cap" => {
            let named = args.first().and_then(|a| match &a.value {
                Value::Expr(Expr::Path(Path { root, segs })) if segs.is_empty() && a.name.is_none() => Some(root.as_str()),
                _ => None,
            });
            match named.and_then(substance) {
                Some(s) => (Ex::Cap(s), Ty::Amount),
                None => return Err(type_error("`cap(...)` takes a substance of the field".into())),
            }
        }
        Expr::Call(Call { name, args }) if matches!(name.as_str(), "sum" | "min" | "max" | "count") => {
            if coll.is_some() {
                return Err(unsupported(&format!("`{name}(...)` inside another reduction")));
            }
            let body = match args.as_slice() {
                [a] if a.name.is_none() => match &a.value {
                    Value::Expr(x) => x,
                    _ => return Err(type_error(format!("`{name}(...)` takes an expression over `moore` or `vonneumann`"))),
                },
                _ => return Err(unsupported(&format!("`{name}` of {} values (it takes one collection, such as `{name}(moore.<substance>)`)", args.len()))),
            };
            let seen = Cell::new(None);
            let (x, t) = build_in(body, substance, lets, Some(&seen))?;
            let Some(set) = seen.get() else {
                return Err(type_error(format!("`{name}(...)` needs a collection of the neighbours: `moore.<substance>` or `vonneumann.<substance>`")));
            };
            let red = match name.as_str() {
                "sum" => Red::Sum,
                "min" => Red::Min,
                "max" => Red::Max,
                _ => Red::Count,
            };
            let ty = if red == Red::Count {
                want(t, Ty::Bool, "what `count` counts")?;
                Ty::Int
            } else if t == Ty::Bool {
                return Err(type_error(format!("`{name}` of a condition: use `count(...)` to count where it holds")));
            } else {
                t
            };
            (Ex::Reduce(red, set, Box::new(x)), ty)
        }
        Expr::Call(Call { name, .. }) => return Err(unsupported(&format!("the call `{name}(...)`"))),
        Expr::Binary(op, l, r) => {
            let ((a, ta), (b, tb)) = (sub(l)?, sub(r)?);
            let (a, b) = (Box::new(a), Box::new(b));
            match op {
                BinOp::Or | BinOp::And => {
                    want(ta, Ty::Bool, "the left side of `and`/`or`")?;
                    want(tb, Ty::Bool, "the right side of `and`/`or`")?;
                    (if *op == BinOp::Or { Ex::Or(a, b) } else { Ex::And(a, b) }, Ty::Bool)
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    if ta != tb {
                        return Err(type_error(format!("a comparison needs two values of one kind, not {ta:?} and {tb:?} (compare amounts with amounts: `5u`, and integers with integers)")));
                    }
                    let cmp = match op {
                        BinOp::Eq => Cmp::Eq,
                        BinOp::Ne => Cmp::Ne,
                        BinOp::Lt => Cmp::Lt,
                        BinOp::Le => Cmp::Le,
                        BinOp::Gt => Cmp::Gt,
                        _ => Cmp::Ge,
                    };
                    (Ex::Cmp(cmp, a, b), Ty::Bool)
                }
                BinOp::Add | BinOp::Sub => {
                    if ta != tb || ta == Ty::Bool {
                        return Err(type_error(format!("`+` and `-` need two integers or two amounts, not {ta:?} and {tb:?}")));
                    }
                    (if *op == BinOp::Add { Ex::Add(a, b) } else { Ex::Sub(a, b) }, ta)
                }
                BinOp::Mul => match (ta, tb) {
                    (Ty::Int, Ty::Int) => (Ex::Mul(a, b), Ty::Int),
                    (Ty::Amount, Ty::Int) | (Ty::Int, Ty::Amount) => (Ex::Mul(a, b), Ty::Amount),
                    _ => return Err(type_error(format!("`*` needs an integer on one side, not {ta:?} and {tb:?}"))),
                },
                BinOp::Div => return Err(unsupported("division")),
            }
        }
        Expr::Pipe(..) | Expr::Capture(_) | Expr::List(_) => return Err(unsupported("this expression")),
    })
}
