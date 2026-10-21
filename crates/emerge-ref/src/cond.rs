//! Conditions of world rules (`proposals/rule-conditions.md`, stage 1): the `when` around an effect
//! statement, evaluated in each cell on the start-of-tick snapshot.
//!
//! Everything is an exact integer (an amount is its µ), so there is no rounding and nothing that
//! could differ between runners. A condition is typed when it is built (`build`): integers
//! compare with integers, amounts with amounts, booleans combine with `and`, `or` and `not`.

use crate::chem::{Mu, SubstanceId};
use emerge_lang::ast::{BinOp, Call, Expr, Lit, Path, Seg, Value};

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
    Add(Box<Ex>, Box<Ex>),
    Sub(Box<Ex>, Box<Ex>),
    Mul(Box<Ex>, Box<Ex>),
    Cmp(Cmp, Box<Ex>, Box<Ex>),
    And(Box<Ex>, Box<Ex>),
    Or(Box<Ex>, Box<Ex>),
    Not(Box<Ex>),
}

/// What a condition reads: the tick and one cell as it was at the start of the tick.
pub struct Ctx<'a> {
    pub tick: u64,
    pub amounts: &'a [Mu],
    pub caps: &'a [Mu],
}

impl Ex {
    pub fn eval(&self, c: &Ctx) -> i64 {
        match self {
            Ex::Const(v) => *v,
            Ex::Tick => c.tick as i64,
            Ex::Own(s) => i64::from(c.amounts[*s]),
            Ex::Cap(s) => i64::from(c.caps[*s]),
            Ex::Add(a, b) => a.eval(c).saturating_add(b.eval(c)),
            Ex::Sub(a, b) => a.eval(c).saturating_sub(b.eval(c)),
            Ex::Mul(a, b) => a.eval(c).saturating_mul(b.eval(c)),
            Ex::Cmp(op, a, b) => {
                let (x, y) = (a.eval(c), b.eval(c));
                i64::from(match op {
                    Cmp::Eq => x == y,
                    Cmp::Ne => x != y,
                    Cmp::Lt => x < y,
                    Cmp::Le => x <= y,
                    Cmp::Gt => x > y,
                    Cmp::Ge => x >= y,
                })
            }
            Ex::And(a, b) => i64::from(a.eval(c) != 0 && b.eval(c) != 0),
            Ex::Or(a, b) => i64::from(a.eval(c) != 0 || b.eval(c) != 0),
            Ex::Not(a) => i64::from(a.eval(c) == 0),
        }
    }

    pub fn holds(&self, c: &Ctx) -> bool {
        self.eval(c) != 0
    }

    /// `self` and `other` both hold.
    pub fn and(self, other: Ex) -> Ex {
        Ex::And(Box::new(self), Box::new(other))
    }
}

/// An error code and its message.
pub type Fault = (&'static str, String);

fn unsupported(what: &str) -> Fault {
    ("E1001", format!("{what} is not run by emerge-ref in a rule's `when` yet (stage 1 reads `self.<substance>`, `cap(<substance>)` and `tick`; see proposals/rule-conditions.md)"))
}

fn type_error(message: String) -> Fault {
    ("E0205", message)
}

/// The expression typed and built. `substance` resolves a substance name, and `lets` are the names
/// `let` gave earlier in the rule (their expressions are substituted).
pub fn build(e: &Expr, substance: &dyn Fn(&str) -> Option<SubstanceId>, lets: &[(String, (Ex, Ty))]) -> Result<(Ex, Ty), Fault> {
    let want = |got: Ty, want: Ty, what: &str| if got == want { Ok(()) } else { Err(type_error(format!("{what} must be {want:?}, not {got:?}"))) };
    Ok(match e {
        Expr::Lit(Lit::Int(i)) => (Ex::Const(*i), Ty::Int),
        Expr::Lit(Lit::Amount(a)) => (Ex::Const(i64::try_from(*a).map_err(|_| type_error("amount too large".into()))?), Ty::Amount),
        Expr::Lit(Lit::Bool(b)) => (Ex::Const(i64::from(*b)), Ty::Bool),
        Expr::Lit(_) => return Err(unsupported("a literal of this kind")),
        Expr::Paren(inner) => build(inner, substance, lets)?,
        Expr::Neg(inner) => {
            let (x, t) = build(inner, substance, lets)?;
            want(t, Ty::Int, "a negated value")?;
            (Ex::Sub(Box::new(Ex::Const(0)), Box::new(x)), Ty::Int)
        }
        Expr::Not(inner) => {
            let (x, t) = build(inner, substance, lets)?;
            want(t, Ty::Bool, "the operand of `not`")?;
            (Ex::Not(Box::new(x)), Ty::Bool)
        }
        Expr::Path(Path { root, segs }) => match (root.as_str(), segs.as_slice()) {
            ("tick", []) => (Ex::Tick, Ty::Int),
            ("self", [Seg::Field(f)]) => match substance(f) {
                Some(s) => (Ex::Own(s), Ty::Amount),
                None => return Err(unsupported(&format!("`self.{f}` (only a substance of the field can be read; signals and other fields are stage 2)"))),
            },
            (name, []) => match lets.iter().rev().find(|(n, _)| n == name) {
                Some((_, (x, t))) => (x.clone(), *t),
                None => return Err(("E0114", format!("unknown name `{name}`"))),
            },
            (root, _) => return Err(unsupported(&format!("`{root}…` (neighbour and other-field reads are stage 2)"))),
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
        Expr::Call(Call { name, .. }) => return Err(unsupported(&format!("the call `{name}(...)`"))),
        Expr::Binary(op, l, r) => {
            let ((a, ta), (b, tb)) = (build(l, substance, lets)?, build(r, substance, lets)?);
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
