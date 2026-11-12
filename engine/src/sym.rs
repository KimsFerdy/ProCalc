//! Symbolic expressions in canonical form.
//!
//! Every `Sym` is built through [`add`], [`mul`] and [`pow`], which keep it canonical:
//!
//! - `Add` has 2+ terms, no nested `Add`, like terms combined, at most one number.
//! - `Mul` has 2+ factors, no nested `Mul`, equal bases merged (x·x = x²), and at most one
//!   rational coefficient, always first.
//! - Numeric roots are fully simplified "surds": `Pow(Num(n), Num(1/q))` with an integer
//!   n > 1 that has no q-th-power factor, so √8 is stored as 2·√2 and 1/√2 as ½·√2.
//! - Children are sorted, so equal expressions are structurally equal (`x+y == y+x`).

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational as Q;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::ast::{BinOp, Const, Expr, Func, SymOp};
use crate::calculus;
use crate::defs::Env;
use crate::eval::{self, Evaluator};
use crate::number::{self, Value, int};
use crate::{AngleMode, EngineError};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sym {
    Num(Q),
    Const(Const),
    Var(String),
    /// A physical unit, by canonical name (see `units`). Not a variable: it has no value to
    /// substitute, and `units::resolve` deals with it at the end.
    Unit(String),
    Add(Vec<Sym>),
    Mul(Vec<Sym>),
    Pow(Box<Sym>, Box<Sym>),
    Func(Func, Box<Sym>),
    /// log base `.0` of `.1`.
    Log(Box<Sym>, Box<Sym>),
    /// `n!` (step 1) or `n!!` (step 2).
    Factorial(Box<Sym>, u32),
    /// A definite integral with no closed form found, evaluated numerically.
    Integral { body: Box<Sym>, var: String, lo: Box<Sym>, hi: Box<Sym> },
    /// A sum with no closed form found (hi may be ∞), evaluated numerically.
    Sum { body: Box<Sym>, var: String, lo: Box<Sym>, hi: Box<Sym> },
    /// A matrix by rows (a vector is one column). Never a factor of a canonical `Mul` with
    /// another matrix: products don't commute, so the builder multiplies matrices itself.
    Matrix(Vec<Vec<Sym>>),
}

pub type SR = Result<Sym, EngineError>;

pub fn n(i: i64) -> Sym {
    Sym::Num(int(i))
}

pub fn q(num: i64, den: i64) -> Sym {
    Sym::Num(Q::new(num.into(), den.into()))
}

fn qq(num: i64, den: i64) -> Q {
    Q::new(num.into(), den.into())
}

impl Sym {
    pub fn as_num(&self) -> Option<&Q> {
        match self {
            Sym::Num(q) => Some(q),
            _ => None,
        }
    }

    pub fn is_zero(&self) -> bool {
        matches!(self, Sym::Num(q) if q.is_zero())
    }

    pub fn is_one(&self) -> bool {
        matches!(self, Sym::Num(q) if q.is_one())
    }

    pub fn has_vars(&self) -> bool {
        match self {
            Sym::Var(_) => true,
            Sym::Num(_) | Sym::Const(_) | Sym::Unit(_) => false,
            Sym::Add(v) | Sym::Mul(v) => v.iter().any(Sym::has_vars),
            Sym::Pow(a, b) | Sym::Log(a, b) => a.has_vars() || b.has_vars(),
            Sym::Func(_, a) | Sym::Factorial(a, _) => a.has_vars(),
            Sym::Matrix(m) => m.iter().flatten().any(Sym::has_vars),
            Sym::Integral { .. } | Sym::Sum { .. } => {
                let mut vars = std::collections::BTreeSet::new();
                crate::calculus::free_vars(self, &mut vars);
                !vars.is_empty()
            }
        }
    }

    /// True when a letter other than `x` appears.
    pub fn has_vars_except(&self, x: &str) -> bool {
        let mut vars = std::collections::BTreeSet::new();
        crate::calculus::free_vars(self, &mut vars);
        vars.remove(x);
        !vars.is_empty()
    }

    /// True when the expression is a "closed form" worth showing as the exact answer:
    /// numbers, surds, π, e and variables, but no unevaluated functions like sin(1).
    pub fn is_closed_form(&self) -> bool {
        match self {
            Sym::Num(_) | Sym::Const(_) | Sym::Var(_) | Sym::Unit(_) => true,
            Sym::Add(v) | Sym::Mul(v) => v.iter().all(Sym::is_closed_form),
            // A huge integer power we refused to expand (2^(10^9)) isn't useful as "exact".
            Sym::Pow(a, b) => {
                let unexpanded = matches!((&**a, &**b), (Sym::Num(_), Sym::Num(e)) if e.is_integer());
                !unexpanded && a.is_closed_form() && b.is_closed_form()
            }
            Sym::Matrix(m) => m.iter().flatten().all(Sym::is_closed_form),
            Sym::Func(..) | Sym::Log(..) | Sym::Factorial(..) | Sym::Integral { .. } | Sym::Sum { .. } => false,
        }
    }

    /// The expression with its rational coefficient removed: 3xy → xy.
    pub fn without_coeff(&self) -> Sym {
        split_coeff(self.clone()).1
    }

    /// Back to an [`Expr`] for high-precision numeric evaluation.
    pub fn to_expr(&self) -> Expr {
        fn num(i: &BigInt) -> Expr {
            Expr::Number(i.to_string())
        }
        fn fold(op: BinOp, items: &[Sym]) -> Expr {
            let mut it = items.iter().map(Sym::to_expr);
            let first = it.next().expect("canonical Add/Mul has children");
            it.fold(first, |acc, e| Expr::Binary(op, Box::new(acc), Box::new(e)))
        }
        match self {
            Sym::Num(r) => {
                let mag = if r.is_integer() {
                    num(&r.numer().abs())
                } else {
                    Expr::Binary(BinOp::Div, Box::new(num(&r.numer().abs())), Box::new(num(r.denom())))
                };
                if r.is_negative() { Expr::Neg(Box::new(mag)) } else { mag }
            }
            Sym::Const(c) => Expr::Const(*c),
            Sym::Var(v) => Expr::Var(v.clone()),
            Sym::Unit(u) => Expr::Unit(u.clone()),
            Sym::Add(v) => fold(BinOp::Add, v),
            Sym::Mul(v) => fold(BinOp::Mul, v),
            Sym::Pow(a, b) => Expr::Pow(Box::new(a.to_expr()), Box::new(b.to_expr())),
            Sym::Func(f, a) => Expr::Call(*f, Box::new(a.to_expr())),
            Sym::Log(b, x) => Expr::LogBase(Box::new(b.to_expr()), Box::new(x.to_expr())),
            Sym::Factorial(a, step) => Expr::Factorial(Box::new(a.to_expr()), *step),
            Sym::Integral { body, var, lo, hi } => Expr::Integral {
                body: Box::new(body.to_expr()),
                var: var.clone(),
                bounds: Some((Box::new(lo.to_expr()), Box::new(hi.to_expr()))),
            },
            Sym::Matrix(m) => Expr::Matrix(m.iter().map(|r| r.iter().map(Sym::to_expr).collect()).collect()),
            Sym::Sum { body, var, lo, hi } => Expr::Sum {
                body: Box::new(body.to_expr()),
                var: var.clone(),
                lo: Box::new(lo.to_expr()),
                hi: Box::new(hi.to_expr()),
                product: false,
            },
        }
    }
}

// ---------- Canonical constructors ----------

/// Splits `3·x·y` into (3, x·y); a plain number into (n, 1).
pub(crate) fn split_coeff(t: Sym) -> (Q, Sym) {
    match t {
        Sym::Num(q) => (q, n(1)),
        Sym::Mul(mut f) if matches!(f.first(), Some(Sym::Num(_))) => {
            let Sym::Num(c) = f.remove(0) else { unreachable!() };
            let rest = if f.len() == 1 { f.pop().unwrap() } else { Sym::Mul(f) };
            (c, rest)
        }
        other => (Q::one(), other),
    }
}

/// Rebuilds `c · rest` where `rest` carries no coefficient of its own.
fn with_coeff(c: Q, rest: Sym) -> Sym {
    if c.is_zero() {
        return n(0);
    }
    if rest.is_one() {
        return Sym::Num(c);
    }
    if c.is_one() {
        return rest;
    }
    match rest {
        Sym::Mul(mut f) => {
            f.insert(0, Sym::Num(c));
            Sym::Mul(f)
        }
        other => Sym::Mul(vec![Sym::Num(c), other]),
    }
}

pub fn add(terms: Vec<Sym>) -> Sym {
    let mut constant = Q::zero();
    let mut like: BTreeMap<Sym, Q> = BTreeMap::new();
    let mut stack = terms;
    while let Some(t) = stack.pop() {
        if let Sym::Add(v) = t {
            stack.extend(v);
            continue;
        }
        let (c, rest) = split_coeff(t);
        if rest.is_one() {
            constant += c;
        } else {
            *like.entry(rest).or_insert_with(Q::zero) += c;
        }
    }
    let mut out: Vec<Sym> = like
        .into_iter()
        .filter(|(_, c)| !c.is_zero())
        .map(|(rest, c)| with_coeff(c, rest))
        .collect();
    if !constant.is_zero() {
        out.push(Sym::Num(constant));
    }
    out.sort();
    match out.len() {
        0 => n(0),
        1 => out.pop().unwrap(),
        _ => Sym::Add(out),
    }
}

pub fn neg(x: Sym) -> Sym {
    mul(vec![n(-1), x])
}

/// `pow` for internal recombination, where an error just means "leave it as written".
pub(crate) fn pow_or_keep(b: Sym, e: Sym) -> Sym {
    pow(b.clone(), e.clone()).unwrap_or_else(|_| Sym::Pow(Box::new(b), Box::new(e)))
}

/// A positive integer under a root. Negative bases never merge: for principal values
/// ⁸√−2 · ⁸√−3 is not ⁸√6.
fn is_numeric_surd(s: &Sym) -> bool {
    matches!(s, Sym::Pow(b, e) if matches!((&**b, &**e), (Sym::Num(bq), Sym::Num(eq)) if bq.is_integer() && bq.is_positive() && !eq.is_integer()))
}

/// r when e = r·π·i.
fn multiple_of_pi_i(e: &Sym) -> Option<Q> {
    match e {
        Sym::Mul(f) => match f.as_slice() {
            [Sym::Const(Const::Pi), Sym::Const(Const::I)] => Some(Q::one()),
            [Sym::Num(r), Sym::Const(Const::Pi), Sym::Const(Const::I)] => Some(r.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Zero, or a zero still carrying units (Mul [0, units…]), so 0 K → °C and 32 °F → °C work.
fn is_zeroish(s: &Sym) -> bool {
    s.is_zero() || matches!(s, Sym::Mul(f) if f.first().is_some_and(Sym::is_zero))
}

/// A zero product: keeps the units of its factors (0·5 m = 0 m), or plain 0.
fn zero_product(factors: &[Sym]) -> Sym {
    let units: Vec<Sym> = factors.iter().filter_map(crate::units::unit_part).collect();
    if units.is_empty() {
        return n(0);
    }
    let mut out = vec![n(0)];
    match mul(units) {
        Sym::Mul(fs) => out.extend(fs),
        other => out.push(other),
    }
    Sym::Mul(out)
}

pub fn mul(factors: Vec<Sym>) -> Sym {
    if factors.iter().any(is_zeroish) {
        return zero_product(&factors);
    }
    let mut coeff = Q::one();
    let mut pending = factors;
    let mut done: Vec<Sym> = Vec::new();
    // Merging can expose new simplifications (√2·√2 → 2, √2·√3 → √6), so repeat
    // until nothing changes. Each round strictly shrinks the surd part, so this ends fast.
    for _ in 0..16 {
        if pending.is_empty() {
            break;
        }
        let mut bases: BTreeMap<Sym, Vec<Sym>> = BTreeMap::new();
        let mut stack = std::mem::take(&mut pending);
        stack.append(&mut done);
        while let Some(f) = stack.pop() {
            match f {
                Sym::Num(c) => {
                    if c.is_zero() {
                        return n(0);
                    }
                    coeff *= c;
                }
                Sym::Mul(v) => stack.extend(v),
                Sym::Pow(b, e) => bases.entry(*b).or_default().push(*e),
                other => bases.entry(other).or_default().push(n(1)),
            }
        }
        // Numeric surds grouped by exponent, so √2·√3 can become √6.
        let mut surds: BTreeMap<Q, BigInt> = BTreeMap::new();
        for (b, exps) in bases {
            match pow_or_keep(b, add(exps)) {
                p @ (Sym::Num(_) | Sym::Mul(_)) => pending.push(p),
                p if is_numeric_surd(&p) => {
                    let Sym::Pow(b, e) = p else { unreachable!() };
                    let (Sym::Num(b), Sym::Num(e)) = (*b, *e) else { unreachable!() };
                    *surds.entry(e).or_insert_with(BigInt::one) *= b.to_integer();
                }
                p => done.push(p),
            }
        }
        for (e, b) in surds {
            match pow_or_keep(Sym::Num(Q::from_integer(b)), Sym::Num(e)) {
                p @ (Sym::Num(_) | Sym::Mul(_)) => pending.push(p),
                p => done.push(p),
            }
        }
    }
    // Round limit reached (shouldn't happen): keep whatever is left, unsimplified.
    for p in pending {
        match p {
            Sym::Num(c) => coeff *= c,
            other => done.push(other),
        }
    }
    if coeff.is_zero() {
        return n(0);
    }
    done.sort();
    if !coeff.is_one() {
        done.insert(0, Sym::Num(coeff));
    }
    match done.len() {
        0 => n(1),
        1 => done.pop().unwrap(),
        _ => Sym::Mul(done),
    }
}

pub fn div(a: Sym, b: Sym) -> SR {
    if b.is_zero() {
        return Err(EngineError::new("Division by zero"));
    }
    Ok(mul(vec![a, pow(b, n(-1))?]))
}

pub fn pow(b: Sym, e: Sym) -> SR {
    if e.is_zero() {
        return Ok(n(1));
    }
    if e.is_one() {
        return Ok(b);
    }
    if b.is_one() {
        return Ok(n(1));
    }
    if let (Sym::Num(bq), Sym::Num(eq)) = (&b, &e) {
        return num_pow(bq, eq);
    }
    // e^(ln x) = x
    if b == Sym::Const(Const::E)
        && let Sym::Func(Func::Ln, x) = &e {
            return Ok((**x).clone());
        }
    // Powers of i cycle: i² = −1, i³ = −i, i⁴ = 1.
    if b == Sym::Const(Const::I)
        && let Some(k) = e.as_num().filter(|k| k.is_integer())
    {
        let r = k.numer().mod_floor(&BigInt::from(4)).to_u32().unwrap_or(0);
        return Ok(match r {
            0 => n(1),
            1 => Sym::Const(Const::I),
            2 => n(-1),
            _ => neg(Sym::Const(Const::I)),
        });
    }
    // Euler: e^(rπi) = cos rπ + i sin rπ when that is a table value (e^(iπ) = −1).
    if b == Sym::Const(Const::E)
        && let Some(r) = multiple_of_pi_i(&e)
        && let (Some(c), Some(s)) = (sin_table(&(&r + qq(1, 2))), sin_table(&r))
    {
        return Ok(add(vec![c, mul(vec![s, Sym::Const(Const::I)])]));
    }
    if let Some(eq) = e.as_num() {
        if eq.is_integer() {
            match b {
                // (x^a)^n = x^(a·n) for integer n
                Sym::Pow(b0, e0) => return pow(*b0, mul(vec![*e0, e])),
                // (2x)^2 = 4x²
                Sym::Mul(fs) => {
                    return Ok(mul(fs.into_iter().map(|f| pow_or_keep(f, e.clone())).collect()));
                }
                _ => {}
            }
        } else if let Sym::Mul(fs) = &b {
            // A positive number comes out of a root: √(4x) = 2√x
            if let Some(Sym::Num(c)) = fs.first()
                && c.is_positive() {
                    let mut fs = fs.clone();
                    let c = fs.remove(0);
                    let rest = if fs.len() == 1 { fs.pop().unwrap() } else { Sym::Mul(fs) };
                    return Ok(mul(vec![pow(c, e.clone())?, Sym::Pow(Box::new(rest), Box::new(e))]));
                }
        }
    }
    Ok(Sym::Pow(Box::new(b), Box::new(e)))
}

fn raw_pow(b: Q, e: Q) -> Sym {
    Sym::Pow(Box::new(Sym::Num(b)), Box::new(Sym::Num(e)))
}

/// Rational base, rational exponent: exact rational, or a simplified surd.
fn num_pow(b: &Q, e: &Q) -> SR {
    if let Some(v) = eval::exact_pow(b, e)? {
        return Ok(Sym::Num(v));
    }
    if e.is_integer() {
        // Too large to expand exactly; the decimal path still evaluates it.
        return Ok(raw_pow(b.clone(), e.clone()));
    }
    let (Some(p), Some(qd)) = (e.numer().to_i64(), e.denom().to_u32().filter(|q| *q <= 1024)) else {
        return Ok(raw_pow(b.clone(), e.clone()));
    };
    if b.is_negative() {
        let mag = num_pow(&-b, e)?;
        if qd % 2 == 1 {
            // Odd root of a negative number is real: (-8)^(1/3) = -2
            return Ok(if p % 2 != 0 { neg(mag) } else { mag });
        }
        // Principal value: (−b)^e = b^e·(cos πe + i sin πe), so √−4 = 2i.
        let r = e.clone();
        return Ok(match (sin_table(&(&r + qq(1, 2))), sin_table(&r)) {
            (Some(c), Some(s)) => mul(vec![mag, add(vec![c, mul(vec![s, Sym::Const(Const::I)])])]),
            // No exact angle (as for the 8th root): left for the numeric layer.
            _ => raw_pow(b.clone(), e.clone()),
        });
    }
    // b = num/den > 0: num^(p/q) · den^(-p/q), each split into coefficient × surd.
    let top = int_pow_frac(b.numer(), p, qd);
    let bottom = int_pow_frac(b.denom(), -p, qd);
    let (Some((c1, s1)), Some((c2, s2))) = (top, bottom) else {
        return Ok(raw_pow(b.clone(), e.clone()));
    };
    // Assembled by hand rather than through `mul`, which calls back into `pow`.
    // num and den are coprime, so equal-index surds merge without further simplification.
    let mut surds: Vec<Sym> = [s1, s2].into_iter().filter(|s| !s.is_one()).collect();
    if let [Sym::Pow(b1, e1), Sym::Pow(b2, e2)] = surds.as_slice()
        && e1 == e2 {
            let (Sym::Num(x), Sym::Num(y)) = (&**b1, &**b2) else { unreachable!() };
            surds = vec![raw_pow(x * y, e1.as_num().unwrap().clone())];
        }
    surds.sort();
    let rest = match surds.len() {
        0 => n(1),
        1 => surds.pop().unwrap(),
        _ => Sym::Mul(surds),
    };
    Ok(with_coeff(c1 * c2, rest))
}

/// n^(p/q) for an integer n ≥ 1, as (rational coefficient, surd).
/// The exponent splits as p/q = f + r/q with 0 ≤ r < q, so negative powers
/// come out rationalised: 2^(-1/2) = ½ · 2^(1/2).
fn int_pow_frac(nn: &BigInt, p: i64, qd: u32) -> Option<(Q, Sym)> {
    let qd_i = qd as i64;
    let f = p.div_euclid(qd_i);
    let r = p.rem_euclid(qd_i) as u32;
    let bits = nn.bits();
    if bits.saturating_mul(f.unsigned_abs()) > eval::MAX_EXACT_BITS
        || bits.saturating_mul(r as u64) > eval::MAX_EXACT_BITS
    {
        return None;
    }
    let whole = Q::from_integer(nn.pow(f.unsigned_abs() as u32));
    let mut coeff = if f < 0 { whole.recip() } else { whole };
    if r == 0 {
        return Some((coeff, n(1)));
    }
    let m = nn.pow(r);
    let (k, rest, q2) = extract_root(&m, qd);
    coeff *= Q::from_integer(k);
    let surd = if rest.is_one() {
        n(1)
    } else {
        raw_pow(Q::from_integer(rest), qq(1, q2 as i64))
    };
    Some((coeff, surd))
}

/// Writes m^(1/q) as k · rest^(1/q2): pulls out q-th powers and lowers the root index
/// where possible, e.g. 72^(1/2) = 6·2^(1/2), 4^(1/4) = 2^(1/2).
fn extract_root(m: &BigInt, qd: u32) -> (BigInt, BigInt, u32) {
    if let Some(r) = number::exact_root_int(m, qd) {
        return (r, BigInt::one(), 1);
    }
    let mut k = BigInt::one();
    let mut parts: Vec<(BigInt, u32)> = Vec::new(); // (base, leftover exponent < q)
    let mut x = m.clone();
    let mut d: u64 = 2;
    while d < 100_000 && BigInt::from(d) * BigInt::from(d) <= x {
        let mut a = 0u32;
        while (&x % d).is_zero() {
            x /= d;
            a += 1;
        }
        if a > 0 {
            k *= BigInt::from(d).pow(a / qd);
            if !a.is_multiple_of(qd) {
                parts.push((BigInt::from(d), a % qd));
            }
        }
        d += if d == 2 { 1 } else { 2 };
    }
    if x > BigInt::one() {
        // Unfactored remainder: it may still be a perfect power.
        let (base, a) = (2..=qd)
            .rev()
            .find_map(|t| number::exact_root_int(&x, t).map(|y| (y, t)))
            .unwrap_or((x, 1));
        k *= base.pow(a / qd);
        if a % qd > 0 {
            parts.push((base, a % qd));
        }
    }
    let g = parts.iter().fold(qd, |g, (_, s)| g.gcd(s));
    let rest = parts.iter().fold(BigInt::one(), |acc, (b, s)| acc * b.pow(s / g));
    (k, rest, qd / g)
}

// ---------- Building from the parse tree ----------

pub struct Builder<'a> {
    pub ev: &'a mut Evaluator,
    /// User definitions: variables are replaced by their values.
    pub env: &'a Env,
}

impl Builder<'_> {
    pub fn build(&mut self, e: &Expr) -> SR {
        Ok(match e {
            Expr::Number(s) => Sym::Num(
                number::parse_exact(s).ok_or_else(|| EngineError::new(format!("Bad number '{s}'")))?,
            ),
            Expr::Const(c) => Sym::Const(*c),
            Expr::Var(v) => self.env.vars.get(v).cloned().unwrap_or_else(|| Sym::Var(v.clone())),
            Expr::Unit(u) => Sym::Unit(u.clone()),
            // In a differential equation y, y', y'' stand for themselves.
            Expr::OdeFn { name, order, at: None, .. } => Sym::Var(format!("{name}{}", "'".repeat(*order as usize))),
            Expr::OdeFn { name, at: Some(_), .. } => {
                return Err(EngineError::new(format!("{name}(…) with a value belongs in an initial condition, like {name}(0) = 1")));
            }
            // d/dx y in an equation is y'.
            Expr::Deriv { body, order, .. } if matches!(**body, Expr::OdeFn { at: None, .. }) => {
                let Expr::OdeFn { name, order: k, .. } = &**body else { unreachable!("matched above") };
                Sym::Var(format!("{name}{}", "'".repeat((k + order) as usize)))
            }
            Expr::Deriv { body, var, order } => {
                let f = self.build_free(body, std::slice::from_ref(var))?;
                let d = self.worked_derivative(f, var, *order)?;
                // With x defined, d/dx gives the slope there.
                match self.env.vars.get(var).cloned() {
                    Some(value) => {
                        let at = self.subst(&d, var, &value)?;
                        self.note_work(
                            format!("{}={}", crate::latex::latex(&d), crate::latex::latex(&at)),
                            format!("At ${var}={}$", crate::latex::latex(&value)),
                        );
                        at
                    }
                    None => d,
                }
            }
            Expr::UserDeriv { name, order, args } => self.user_derivative(name, *order, args)?,
            Expr::Integral { body, var, bounds } => self.integral(body, var, bounds.as_ref())?,
            Expr::Subst(body, binds) => {
                let names: Vec<String> = binds.iter().map(|(v, _)| v.clone()).collect();
                let mut s = self.build_free(body, &names)?;
                let mut steps = vec![crate::Step { latex: crate::latex::latex(&s), note: "The expression".into() }];
                for (v, e) in binds {
                    let value = self.build(e)?;
                    s = self.subst(&s, v, &value)?;
                    steps.push(crate::Step {
                        latex: crate::latex::latex(&s),
                        note: format!("Replace ${v}$ with ${}$ and simplify", crate::latex::latex(&value)),
                    });
                }
                self.ev.work.push(crate::Method { name: "Substitution".into(), steps });
                s
            }
            Expr::BarEval(body, lo, hi) => {
                let f = self.build(body)?;
                let mut vars = std::collections::BTreeSet::new();
                calculus::free_vars(&f, &mut vars);
                match vars.len() {
                    // A constant: F(b) − F(a) = 0.
                    0 => n(0),
                    1 => {
                        let v = vars.pop_first().expect("one variable");
                        let (a, b) = (self.build(lo)?, self.build(hi)?);
                        add(vec![self.subst(&f, &v, &b)?, neg(self.subst(&f, &v, &a)?)])
                    }
                    _ => return Err(EngineError::new("Name the variable to evaluate at, as in |_{x=1}")),
                }
            }
            Expr::Convert(x, target) => {
                let x = self.build(x)?;
                crate::units::convert(x, self.build(target)?)?
            }
            Expr::UserCall(name, args) => {
                let args = args.iter().map(|a| self.build(a)).collect::<Result<Vec<_>, _>>()?;
                crate::defs::call(self.env, name, args, self.ev)?
            }
            Expr::Limit { body, var, to, side } => self.limit_expr(body, var, to, *side)?,
            Expr::Sum { body, var, lo, hi, product } => self.sum(body, var, lo, hi, *product)?,
            Expr::Taylor { body, var, at, order } => self.taylor(body, var.as_deref(), at, *order)?,
            Expr::PolyDiv { top, bottom, part } => {
                let (a, d) = (self.build(top)?, self.build(bottom)?);
                let (q, r) = self.long_division(&a, &d)?;
                match part {
                    crate::ast::DivPart::Quotient => q,
                    crate::ast::DivPart::Remainder => r,
                    crate::ast::DivPart::Both => add(vec![q, div(r, d)?]),
                }
            }
            Expr::NumCall(f, args) => {
                let args = args.iter().map(|a| self.build(a)).collect::<Result<Vec<_>, _>>()?;
                crate::numtheory::apply(*f, &args, &mut self.ev.work)?
            }
            Expr::Matrix(rows) => {
                let mut out = Vec::new();
                for r in rows {
                    let mut built = Vec::new();
                    for e in r {
                        let v = self.build(e)?;
                        if matches!(v, Sym::Matrix(_)) {
                            return Err(EngineError::new("A matrix entry must be a number or an expression, not a matrix"));
                        }
                        built.push(v);
                    }
                    out.push(built);
                }
                Sym::Matrix(out)
            }
            // Inside a larger expression: the optimum value, or a shape's main measurement.
            Expr::Optimize(goal, args) => {
                let opt = self.optimize(*goal, args)?;
                match (goal, opt.found.first()) {
                    (crate::ast::Goal::All, _) | (_, None) => return Err(EngineError::new("extrema gives a list: put it on a line by itself")),
                    (_, Some(e)) => e.value.clone(),
                }
            }
            Expr::Geometry(f, args) => {
                let g = self.geometry(*f, args)?;
                g.primary.ok_or_else(|| EngineError::new("This gives several results: put it on a line by itself"))?
            }
            Expr::StatCall(f, name, args) => {
                let args = args.iter().map(|a| self.build(a)).collect::<Result<Vec<_>, _>>()?;
                self.stat_call(*f, name, args)?
            }
            Expr::MatCall(f, args) => {
                let args = args.iter().map(|a| self.build(a)).collect::<Result<Vec<_>, _>>()?;
                self.mat_call(*f, args)?
            }
            Expr::Neg(x) => match self.build(x)? {
                Sym::Matrix(m) => self.matrix_neg(&m),
                v => neg(v),
            },
            Expr::Binary(op, a, b) => {
                let (a, b) = (self.build(a)?, self.build(b)?);
                if matches!(a, Sym::Matrix(_)) || matches!(b, Sym::Matrix(_)) {
                    return self.matrix_binary(*op, a, b);
                }
                match op {
                    BinOp::Add => add(vec![a, b]),
                    BinOp::Sub => add(vec![a, neg(b)]),
                    BinOp::Mul => mul(vec![a, b]),
                    BinOp::Div => div(a, b)?,
                }
            }
            Expr::Pow(a, b) => {
                let a = self.build(a)?;
                if let Sym::Matrix(m) = &a {
                    // Aᵀ, A^{\top}: the transpose (unless T has a value).
                    let transpose = match &**b {
                        Expr::Var(t) => (t == "T" && !self.env.vars.contains_key("T")) || t == "\\top" || t == "\\intercal",
                        _ => false,
                    };
                    if transpose {
                        return self.mat_call(crate::ast::MatFn::Transpose, vec![a.clone()]);
                    }
                    let e = self.build(b)?;
                    return self.matrix_pow(m, &e);
                }
                pow(a, self.build(b)?)?
            }
            Expr::Root(k, x) => {
                let k = self.build(k)?;
                if k.is_zero() {
                    return Err(eval::zeroth_root_err());
                }
                pow(self.build(x)?, div(n(1), k)?)?
            }
            Expr::LogBase(b, x) => {
                let b = self.build(b)?;
                log_base(b, self.build(x)?)?
            }
            Expr::Factorial(x, step) => {
                let x = self.build(x)?;
                self.factorial(x, *step)?
            }
            Expr::Percent(x) => {
                let x = self.build(x)?;
                mul(vec![x, q(1, 100)])
            }
            Expr::Op(op, x) => {
                let x = self.build(x)?;
                match op {
                    SymOp::Expand => crate::poly::expand(&x)?,
                    SymOp::Factor => crate::poly::factor(&x)?,
                    // Inside a larger expression, polar form is just the value.
                    SymOp::Polar => x,
                    SymOp::Simplify => self.simplify_fraction(&x)?,
                    SymOp::Apart => self.apart(&x)?,
                }
            }
            Expr::Call(f, x) => {
                let trig = matches!(f, Func::Sin | Func::Cos | Func::Tan);
                match eval::as_explicit_degrees(x) {
                    // `sin 30°` in DEG mode: the argument is already in degrees.
                    Some(inner) if trig && self.ev.angle == AngleMode::Degrees => {
                        let arg = self.build(inner)?;
                        self.func(*f, arg)?
                    }
                    _ => {
                        let arg = self.build(x)?;
                        if let Sym::Matrix(m) = &arg {
                            // |A| is the determinant, |v| the length; other functions of
                            // matrices aren't entry by entry, so they aren't guessed.
                            return match f {
                                Func::Abs => self.matrix_abs(m),
                                _ => Err(EngineError::new(format!("{} of a matrix isn't supported", f.latex()))),
                            };
                        }
                        self.func(*f, arg)?
                    }
                }
            }
        })
    }

    /// Builds `e` with the named variables left free, even if they are defined.
    pub(crate) fn build_free(&mut self, e: &Expr, names: &[String]) -> SR {
        if names.iter().all(|v| !self.env.vars.contains_key(v)) {
            return self.build(e);
        }
        let mut local = self.env.clone();
        for v in names {
            local.vars.remove(v);
        }
        Builder { ev: &mut *self.ev, env: &local }.build(e)
    }

    /// Replaces the variable `x` with `v` and simplifies again: sin(x) at x = π/6 is ½.
    pub fn subst(&mut self, s: &Sym, x: &str, v: &Sym) -> SR {
        Ok(match s {
            Sym::Var(name) if name == x => v.clone(),
            Sym::Num(_) | Sym::Const(_) | Sym::Var(_) | Sym::Unit(_) => s.clone(),
            Sym::Add(t) => add(t.iter().map(|t| self.subst(t, x, v)).collect::<Result<_, _>>()?),
            Sym::Mul(f) => mul(f.iter().map(|t| self.subst(t, x, v)).collect::<Result<_, _>>()?),
            Sym::Pow(b, e) => {
                let b = self.subst(b, x, v)?;
                pow(b, self.subst(e, x, v)?)?
            }
            Sym::Func(f, a) => {
                let a = self.subst(a, x, v)?;
                self.func(*f, a)?
            }
            Sym::Log(b, a) => {
                let b = self.subst(b, x, v)?;
                log_base(b, self.subst(a, x, v)?)?
            }
            Sym::Factorial(a, k) => {
                let a = self.subst(a, x, v)?;
                self.factorial(a, *k)?
            }
            Sym::Integral { body, var, lo, hi } => {
                let body = if var == x { (**body).clone() } else { self.subst(body, x, v)? };
                Sym::Integral {
                    body: Box::new(body),
                    var: var.clone(),
                    lo: Box::new(self.subst(lo, x, v)?),
                    hi: Box::new(self.subst(hi, x, v)?),
                }
            }
            Sym::Matrix(m) => Sym::Matrix(m.iter().map(|r| r.iter().map(|e| self.subst(e, x, v)).collect::<Result<_, _>>()).collect::<Result<_, _>>()?),
            Sym::Sum { body, var, lo, hi } => {
                let body = if var == x { (**body).clone() } else { self.subst(body, x, v)? };
                Sym::Sum {
                    body: Box::new(body),
                    var: var.clone(),
                    lo: Box::new(self.subst(lo, x, v)?),
                    hi: Box::new(self.subst(hi, x, v)?),
                }
            }
        })
    }

    /// `f'(a)`: the derivative of a one-variable user function, at the argument.
    fn user_derivative(&mut self, name: &str, order: u32, args: &[Expr]) -> SR {
        let def = self
            .env
            .funcs
            .get(name)
            .cloned()
            .ok_or_else(|| EngineError::new(format!("{} isn't defined", crate::ast::name_latex(name))))?;
        let ([param], [arg]) = (def.params.as_slice(), args) else {
            return Err(EngineError::new("f′ works for functions of one variable"));
        };
        let arg = self.build(arg)?;
        let mut scope = (*def.scope).clone();
        scope.vars.remove(param);
        let body = Builder { ev: &mut *self.ev, env: &scope }.build(&def.body)?;
        let d = self.worked_derivative(body, param, order)?;
        let at = self.subst(&d, param, &arg)?;
        if arg != Sym::Var(param.clone()) {
            let primes = "'".repeat(order as usize);
            let fname = crate::ast::name_latex(name);
            self.note_work(
                format!("{fname}{primes}\\left({}\\right)={}", crate::latex::latex(&arg), crate::latex::latex(&at)),
                format!("Put ${param}={}$ into the derivative", crate::latex::latex(&arg)),
            );
        }
        Ok(at)
    }

    /// d^n f / dx^n, recording the working as a "Derivative" method.
    fn worked_derivative(&mut self, f: Sym, var: &str, order: u32) -> SR {
        const ORDINALS: [&str; 5] = ["First", "Second", "Third", "Fourth", "Fifth"];
        let mut steps = Vec::new();
        let mut d = f;
        for k in 0..order {
            let (next, work) = calculus::diff_steps(&d, var, self.ev.angle)?;
            if order > 1 {
                let label = ORDINALS.get(k as usize).map_or_else(|| format!("Derivative {}", k + 1), |o| format!("{o} derivative"));
                steps.push(crate::Step { latex: String::new(), note: label });
            }
            steps.extend(work);
            d = next;
        }
        self.ev.work.push(crate::Method { name: "Derivative".into(), steps });
        Ok(d)
    }

    /// Adds a step to the most recent piece of working.
    fn note_work(&mut self, latex: String, note: String) {
        if let Some(m) = self.ev.work.last_mut() {
            m.steps.push(crate::Step { latex, note });
        }
    }

    /// `∫ f dx` or `∫ₐᵇ f dx`.
    fn integral(&mut self, body: &Expr, var: &str, bounds: Option<&(Box<Expr>, Box<Expr>)>) -> SR {
        let f = self.build_free(body, &[var.to_string()])?;
        if crate::units::contains_unit(&f) {
            return Err(EngineError::new("Integrals with units aren't supported yet"));
        }
        // Every antiderivative is differentiated back and compared at sample points.
        let found = calculus::integrate_steps(&f, var, self.ev.angle)
            .filter(|(big_f, _)| calculus::check_antiderivative(big_f, &f, var, self.ev));
        let (antiderivative, mut steps) = match found {
            Some((big_f, steps)) => (Some(big_f), steps),
            None => (None, Vec::new()),
        };
        let lx = |s: &Sym| crate::latex::latex(s);
        let Some((lo, hi)) = bounds else {
            let big_f = antiderivative.ok_or_else(|| EngineError::new("I can't find an antiderivative for this yet"))?;
            steps.push(crate::Step {
                latex: format!("{}={}+C", calculus::int_latex(&f, var), lx(&big_f)),
                note: "Add the constant of integration (checked by differentiating back)".into(),
            });
            self.ev.work.push(crate::Method { name: "Integral".into(), steps });
            return Ok(big_f);
        };
        let (a, b) = (self.build(lo)?, self.build(hi)?);
        let infinite = crate::limits::point_of(&a)? != crate::limits::Point::At(a.clone())
            || crate::limits::point_of(&b)? != crate::limits::Point::At(b.clone());
        if infinite {
            return self.improper_integral(f, var, a, b, antiderivative, steps);
        }
        let numeric_bounds = !a.has_vars() && !b.has_vars();
        let mut others = std::collections::BTreeSet::new();
        calculus::free_vars(&f, &mut others);
        others.remove(var);
        // The integrand can only be sampled when x is its one letter.
        if numeric_bounds && others.is_empty() && !calculus::free_of(&f, var) {
            let (va, vb) = (self.numeric(&a)?, self.numeric(&b)?);
            calculus::check_defined_between(&f, var, &va, &vb, self.ev)?;
        }
        if let Some(big_f) = antiderivative {
            // F(b) − F(a)
            let fb = self.endpoint(&big_f, var, &b, -1)?;
            let fa = self.endpoint(&big_f, var, &a, 1)?;
            let result = add(vec![fb.clone(), neg(fa.clone())]);
            let bracket = format!("\\left[{}\\right]_{{{}}}^{{{}}}", lx(&big_f), lx(&a), lx(&b));
            steps.push(crate::Step { latex: format!("F({var})={}", lx(&big_f)), note: "An antiderivative".into() });
            steps.push(crate::Step {
                latex: format!("{bracket}=\\left({}\\right)-\\left({}\\right)={}", lx(&fb), lx(&fa), lx(&result)),
                note: "Evaluate $F(b)-F(a)$".into(),
            });
            self.ev.work.push(crate::Method { name: "Integral".into(), steps });
            return Ok(result);
        }
        if !others.is_empty() || !numeric_bounds {
            return Err(EngineError::new("I can't find an antiderivative for this yet, and it has letters, so it can't be done numerically"));
        }
        self.ev.work.push(crate::Method {
            name: "Numeric integral".into(),
            steps: vec![crate::Step {
                latex: format!("\\int_{{{}}}^{{{}}}{}\\,\\mathrm{{d}}{var}", lx(&a), lx(&b), lx(&f)),
                note: "No antiderivative from the rules, so tanh-sinh quadrature: sample the integrand at points that crowd toward the ends, until two precisions agree on every digit shown".into(),
            }],
        });
        // No closed form: left for numeric integration.
        Ok(Sym::Integral { body: Box::new(f), var: var.to_string(), lo: Box::new(a), hi: Box::new(b) })
    }

    /// Numeric value of a variable-free expression.
    pub(crate) fn numeric(&mut self, s: &Sym) -> Result<Value, EngineError> {
        self.ev.eval(&s.to_expr())
    }

    fn factorial(&mut self, x: Sym, step: u32) -> SR {
        if x.has_vars() {
            return Ok(Sym::Factorial(Box::new(x), step));
        }
        // x! = Γ(x+1), exact at the half-integers: (½)! = ½√π.
        if step == 1
            && let Some(v) = x.as_num()
            && !v.is_integer()
        {
            return self.func(Func::Gamma, Sym::Num(v + Q::one()));
        }
        match self.numeric(&x)? {
            v @ Value::Exact(_) => match self.ev.factorial(v, step)? {
                Value::Exact(r) => Ok(Sym::Num(r)),
                _ => Ok(Sym::Factorial(Box::new(x), step)),
            },
            Value::Real(_) => Ok(Sym::Factorial(Box::new(x), step)),
            Value::Complex(..) => Err(EngineError::new("Factorial needs a whole number ≥ 0, not a complex one")),
        }
    }

    /// −1, 0 or 1: the sign of a real, variable-free expression.
    pub(crate) fn sign_of(&mut self, s: &Sym) -> Result<i8, EngineError> {
        if let Some(v) = s.as_num() {
            return Ok(if v.is_zero() { 0 } else if v.is_positive() { 1 } else { -1 });
        }
        let v = self.numeric(s)?;
        let f = self.ev.prec.float_of(&v);
        Ok(if f.is_zero() { 0 } else if number::is_negative_float(&f) { -1 } else { 1 })
    }

    /// The angle of a + bi in (−π, π] (or degrees), exact at the standard angles.
    pub(crate) fn arg_of(&mut self, a: Sym, b: Sym) -> SR {
        let half_turn = match self.ev.angle {
            AngleMode::Radians => Sym::Const(Const::Pi),
            AngleMode::Degrees => n(180),
        };
        let (sa, sb) = (self.sign_of(&a)?, self.sign_of(&b)?);
        if sa == 0 {
            // arg 0 = 0, the usual convention.
            return Ok(mul(vec![q(sb.into(), 2), half_turn]));
        }
        // atan(b/a) is the angle in the right half; the left half is half a turn round.
        // atan is odd, so a negative ratio gives −atan(4/3), not atan(−4/3).
        let ratio = div(b, a)?;
        let t = if sa * sb < 0 {
            neg(inverse_trig(Func::Atan, neg(ratio), self.ev.angle)?)
        } else {
            inverse_trig(Func::Atan, ratio, self.ev.angle)?
        };
        Ok(match (sa, sb) {
            (1, _) => t,
            (_, -1) => add(vec![t, neg(half_turn)]),
            _ => add(vec![t, half_turn]),
        })
    }

    /// All k-th roots of a number, the principal one first: De Moivre, exact at the standard
    /// angles, and for square roots √(a+bi) = √((r+a)/2) + i·sgn(b)·√((r−a)/2).
    pub(crate) fn nth_roots(&mut self, z: &Sym, k: i64) -> Result<Vec<Sym>, EngineError> {
        if z.is_zero() {
            return Ok(vec![n(0)]);
        }
        let (re, im) = rect(z).ok_or_else(|| EngineError::new("Couldn't split this into real and imaginary parts"))?;
        let r = self.func(Func::Abs, z.clone())?;
        let i = || Sym::Const(Const::I);
        if k == 2 {
            let x = pow(div(add(vec![r.clone(), re.clone()]), n(2))?, q(1, 2))?;
            let y = pow(div(add(vec![r, neg(re)]), n(2))?, q(1, 2))?;
            let y = if self.sign_of(&im)? < 0 { neg(y) } else { y };
            let first = normalize_complex(add(vec![x, mul(vec![y, i()])]));
            return Ok(vec![first.clone(), normalize_complex(neg(first))]);
        }
        let theta = self.arg_of(re, im)?;
        let half_turn = match self.ev.angle {
            AngleMode::Radians => Sym::Const(Const::Pi),
            AngleMode::Degrees => n(180),
        };
        let size = pow(r, q(1, k))?;
        let mut roots = Vec::new();
        for j in 0..k {
            let angle = div(add(vec![theta.clone(), mul(vec![n(2 * j), half_turn.clone()])]), n(k))?;
            let (c, s) = (self.func(Func::Cos, angle.clone())?, self.func(Func::Sin, angle)?);
            roots.push(normalize_complex(mul(vec![size.clone(), add(vec![c, mul(vec![s, i()])])])));
        }
        Ok(roots)
    }

    pub(crate) fn func(&mut self, f: Func, x: Sym) -> SR {
        match f {
            Func::Sqrt => pow(x, q(1, 2)),
            Func::Exp => pow(Sym::Const(Const::E), x),
            Func::Abs => Ok(abs(x)),
            Func::Ln => ln(x),
            Func::Log => log_base(n(10), x),
            Func::Floor | Func::Ceil => {
                if x.has_vars() {
                    return Ok(Sym::Func(f, Box::new(x)));
                }
                let v = self.numeric(&x)?;
                match self.ev.call(f, v)? {
                    Value::Exact(r) => Ok(Sym::Num(r)),
                    _ => Ok(Sym::Func(f, Box::new(x))),
                }
            }
            Func::Sin | Func::Cos | Func::Tan => {
                if let Some(r) = pi_multiple(&x, self.ev.angle)
                    && let Some(v) = exact_trig(f, &r)? {
                        return Ok(v);
                    }
                Ok(Sym::Func(f, Box::new(x)))
            }
            Func::Asin | Func::Acos | Func::Atan => inverse_trig(f, x, self.ev.angle),
            Func::Re | Func::Im | Func::Conj | Func::Arg if !x.has_vars() => match rect(&x) {
                Some((a, b)) => match f {
                    Func::Re => Ok(a),
                    Func::Im => Ok(b),
                    Func::Conj => Ok(add(vec![a, neg(mul(vec![b, Sym::Const(Const::I)]))])),
                    _ => self.arg_of(a, b),
                },
                None => Ok(Sym::Func(f, Box::new(x))),
            },
            Func::Erf if x.is_zero() => Ok(n(0)),
            Func::Probit if x == q(1, 2) => Ok(n(0)),
            Func::Gamma => match x.as_num() {
                Some(v) => exact_gamma(v),
                None => Ok(Sym::Func(f, Box::new(x))),
            },
            Func::Sinh | Func::Tanh | Func::Asinh | Func::Atanh if x.is_zero() => Ok(n(0)),
            Func::Cosh if x.is_zero() => Ok(n(1)),
            Func::Acosh if x.is_one() => Ok(n(0)),
            Func::Atanh if matches!(x.as_num(), Some(v) if v.abs().is_one()) => {
                Err(EngineError::new("artanh is infinite at ±1"))
            }
            _ => Ok(Sym::Func(f, Box::new(x))),
        }
    }
}

/// Γ at a number: (n−1)! for whole n, a rational times √π at the half-integers, and left
/// for the numeric engine elsewhere.
fn exact_gamma(v: &Q) -> SR {
    const LIMIT: i64 = 5000;
    // Past this the exact fraction in front of √π is longer than it is useful.
    const HALF_LIMIT: i64 = 30;
    let fact = |k: i64| (1..=k).fold(BigInt::one(), |acc, i| acc * i);
    if v.is_integer() {
        let k = v.to_integer().to_i64().filter(|k| *k <= LIMIT);
        return match k {
            Some(k) if k >= 1 => Ok(Sym::Num(Q::from_integer(fact(k - 1)))),
            Some(_) => Err(EngineError::new("Γ is undefined at 0 and the negative whole numbers")),
            None => Ok(Sym::Func(Func::Gamma, Box::new(Sym::Num(v.clone())))),
        };
    }
    let twice = v * int(2);
    let half_integer = twice.is_integer();
    let k = (v - Q::new(BigInt::one(), BigInt::from(2))).to_integer().to_i64().filter(|k| k.abs() <= HALF_LIMIT);
    let (true, Some(k)) = (half_integer, k) else {
        return Ok(Sym::Func(Func::Gamma, Box::new(Sym::Num(v.clone()))));
    };
    // Γ(k+½) = (2k)!/(4ᵏ k!)·√π, and Γ(½−k) = (−4)ᵏ k!/(2k)!·√π.
    let m = k.unsigned_abs() as i64;
    let four = BigInt::from(4).pow(m as u32);
    let c = if k >= 0 {
        Q::new(fact(2 * m), four * fact(m))
    } else {
        let sign = if m % 2 == 1 { -1 } else { 1 };
        Q::new(four * fact(m) * sign, fact(2 * m))
    };
    Ok(mul(vec![Sym::Num(c), pow(Sym::Const(Const::Pi), q(1, 2))?]))
}

/// Things we know are positive: positive numbers, π, e, positive surds.
fn known_positive(s: &Sym) -> bool {
    match s {
        Sym::Num(v) => v.is_positive(),
        Sym::Const(c) => *c != Const::I,
        Sym::Unit(_) => true,
        Sym::Pow(b, e) => known_positive(b) && !e.has_vars(),
        Sym::Mul(f) => f.iter().all(known_positive),
        Sym::Add(t) => t.iter().all(known_positive),
        _ => false,
    }
}

fn abs(x: Sym) -> Sym {
    if let Sym::Num(v) = &x {
        return Sym::Num(v.abs());
    }
    // |a + bi| = √(a² + b²)
    if contains_i(&x)
        && !x.has_vars()
        && let Some((re, im)) = rect(&x)
    {
        let m = expand_or_keep(add(vec![pow_or_keep(re, n(2)), pow_or_keep(im, n(2))]));
        return pow_or_keep(m, q(1, 2));
    }
    if known_positive(&x) {
        return x;
    }
    let (c, rest) = split_coeff(x);
    if known_positive(&rest) {
        return with_coeff(c.abs(), rest);
    }
    if !c.is_one() {
        // |−3x| = 3|x|
        return mul(vec![Sym::Num(c.abs()), Sym::Func(Func::Abs, Box::new(rest))]);
    }
    Sym::Func(Func::Abs, Box::new(rest))
}

pub(crate) fn ln(x: Sym) -> SR {
    let pi_i = || mul(vec![Sym::Const(Const::Pi), Sym::Const(Const::I)]);
    match &x {
        Sym::Num(v) if v.is_zero() => return Err(EngineError::new("Logarithm of zero is undefined")),
        // Principal value: ln(−x) = ln x + iπ
        Sym::Num(v) if v.is_negative() => return Ok(add(vec![ln(Sym::Num(-v))?, pi_i()])),
        Sym::Num(v) if v.is_one() => return Ok(n(0)),
        Sym::Const(Const::I) => return Ok(mul(vec![q(1, 2), pi_i()])),
        Sym::Const(Const::E) => return Ok(n(1)),
        Sym::Pow(b, k) if **b == Sym::Const(Const::E) => return Ok((**k).clone()),
        _ => {}
    }
    Ok(Sym::Func(Func::Ln, Box::new(x)))
}

pub(crate) fn log_base(b: Sym, x: Sym) -> SR {
    if let Sym::Num(bv) = &b {
        if !bv.is_positive() {
            return Err(EngineError::new("Logarithm base must be positive"));
        }
        if bv.is_one() {
            return Err(EngineError::new("Logarithm base can't be 1"));
        }
    }
    match &x {
        Sym::Num(v) if v.is_zero() => return Err(EngineError::new("Logarithm of zero is undefined")),
        // log_b(−x) = ln(−x)/ln b, complex.
        Sym::Num(v) if v.is_negative() => {
            return div(ln(x.clone())?, ln(b)?);
        }
        _ => {}
    }
    if let (Sym::Num(bv), Sym::Num(xv)) = (&b, &x)
        && let Some(k) = eval::exact_log(bv, xv) {
            return Ok(Sym::Num(k));
        }
    if x == b {
        return Ok(n(1));
    }
    if let Sym::Pow(xb, k) = &x
        && **xb == b {
            return Ok((**k).clone());
        }
    if b == Sym::Const(Const::E) {
        return ln(x);
    }
    Ok(Sym::Log(Box::new(b), Box::new(x)))
}

// ---------- Complex numbers ----------

pub fn contains_i(s: &Sym) -> bool {
    match s {
        Sym::Const(c) => *c == Const::I,
        Sym::Num(_) | Sym::Var(_) | Sym::Unit(_) => false,
        Sym::Add(v) | Sym::Mul(v) => v.iter().any(contains_i),
        Sym::Pow(a, b) | Sym::Log(a, b) => contains_i(a) || contains_i(b),
        Sym::Func(_, a) | Sym::Factorial(a, _) => contains_i(a),
        Sym::Matrix(m) => m.iter().flatten().any(contains_i),
        Sym::Integral { body, lo, hi, .. } | Sym::Sum { body, lo, hi, .. } => contains_i(body) || contains_i(lo) || contains_i(hi),
    }
}

fn expand_or_keep(s: Sym) -> Sym {
    crate::poly::expand(&s).unwrap_or(s)
}

/// True when a variable-free expression is certainly real.
fn is_real(s: &Sym) -> bool {
    let in_range = |x: &Sym, ok: &dyn Fn(&Q) -> bool| matches!(x.as_num(), Some(v) if ok(v));
    match s {
        Sym::Num(_) | Sym::Unit(_) => true,
        Sym::Const(c) => *c != Const::I,
        Sym::Var(_) => false,
        Sym::Add(v) | Sym::Mul(v) => v.iter().all(is_real),
        Sym::Pow(b, e) => {
            is_real(b)
                && is_real(e)
                && (known_positive(b) || matches!(e.as_num(), Some(k) if k.denom().is_odd()))
        }
        Sym::Func(Func::Re | Func::Im | Func::Arg, _) => true,
        Sym::Func(f, x) => {
            is_real(x)
                && match f {
                    Func::Ln | Func::Log | Func::Sqrt => known_positive(x),
                    Func::Asin | Func::Acos => in_range(x, &|v| v.abs() <= Q::one()),
                    Func::Acosh => in_range(x, &|v| *v >= Q::one()),
                    Func::Atanh => in_range(x, &|v| v.abs() < Q::one()),
                    _ => true,
                }
        }
        Sym::Log(b, x) => known_positive(b) && known_positive(x),
        Sym::Factorial(x, _) => is_real(x),
        Sym::Matrix(m) => m.iter().flatten().all(is_real),
        Sym::Integral { body, lo, hi, .. } | Sym::Sum { body, lo, hi, .. } => is_real(body) && is_real(lo) && is_real(hi),
    }
}

/// (a + bi)(c + di)
fn cmul((a, b): (Sym, Sym), (c, d): (Sym, Sym)) -> (Sym, Sym) {
    let re = add(vec![mul(vec![a.clone(), c.clone()]), neg(mul(vec![b.clone(), d.clone()]))]);
    let im = add(vec![mul(vec![a, d]), mul(vec![b, c])]);
    (expand_or_keep(re), expand_or_keep(im))
}

/// Largest integer power of a complex expression multiplied out exactly.
const MAX_RECT_POWER: u64 = 64;

/// Exact real and imaginary parts of a variable-free expression, when they can be found.
pub fn rect(s: &Sym) -> Option<(Sym, Sym)> {
    if is_real(s) {
        return Some((s.clone(), n(0)));
    }
    match s {
        Sym::Const(Const::I) => Some((n(0), n(1))),
        Sym::Add(v) => {
            let (mut re, mut im) = (Vec::new(), Vec::new());
            for t in v {
                let (a, b) = rect(t)?;
                re.push(a);
                im.push(b);
            }
            Some((add(re), add(im)))
        }
        Sym::Mul(v) => v.iter().try_fold((n(1), n(0)), |acc, f| Some(cmul(acc, rect(f)?))),
        Sym::Func(Func::Conj, x) => rect(x).map(|(a, b)| (a, neg(b))),
        Sym::Pow(b, e) => {
            let k = e.as_num().filter(|k| k.is_integer())?.to_i64()?;
            if k.unsigned_abs() > MAX_RECT_POWER {
                return None;
            }
            let z = rect(b)?;
            let mut acc = (n(1), n(0));
            for _ in 0..k.unsigned_abs() {
                acc = cmul(acc, z.clone());
            }
            if k < 0 {
                // 1/(a + bi) = (a − bi)/(a² + b²)
                let (a, b) = acc;
                let m = expand_or_keep(add(vec![pow_or_keep(a.clone(), n(2)), pow_or_keep(b.clone(), n(2))]));
                acc = (div(a, m.clone()).ok()?, neg(div(b, m).ok()?));
                acc = (expand_or_keep(acc.0), expand_or_keep(acc.1));
            }
            Some(acc)
        }
        _ => None,
    }
}

/// (a, b) when the expression is a + bi with rational a and b ≠ 0.
pub fn rational_parts(s: &Sym) -> Option<(Q, Q)> {
    if !contains_i(s) {
        return None;
    }
    match rect(s)? {
        (Sym::Num(a), Sym::Num(b)) => Some((a, b)),
        _ => None,
    }
}

/// Rewrites a variable-free expression with i in it as a + bi, when the parts can be found.
pub fn normalize_complex(s: Sym) -> Sym {
    if s.has_vars() || !contains_i(&s) {
        return s;
    }
    match rect(&s) {
        Some((re, im)) => add(vec![re, mul(vec![im, Sym::Const(Const::I)])]),
        None => s,
    }
}

// ---------- Exact trigonometry ----------

/// The angle as a rational multiple of π, if it is one.
fn pi_multiple(x: &Sym, angle: AngleMode) -> Option<Q> {
    match angle {
        AngleMode::Degrees => x.as_num().map(|d| d / Q::from_integer(180.into())),
        AngleMode::Radians => match x {
            Sym::Num(v) if v.is_zero() => Some(Q::zero()),
            Sym::Const(Const::Pi) => Some(Q::one()),
            Sym::Mul(f) if f.len() == 2 && f[1] == Sym::Const(Const::Pi) => f[0].as_num().cloned(),
            _ => None,
        },
    }
}

fn sqrt_of(k: i64) -> Sym {
    raw_pow(int(k), qq(1, 2))
}

/// sin(r·π) for r with denominator 1, 2, 3, 4 or 6.
fn sin_table(r: &Q) -> Option<Sym> {
    let two = int(2);
    let t = r - &two * (r / &two).floor(); // r mod 2, in [0, 2)
    let half = || q(1, 2);
    let s3 = || mul(vec![q(1, 2), sqrt_of(3)]);
    let s2 = || mul(vec![q(1, 2), sqrt_of(2)]);
    let six = &t * int(6);
    if six.is_integer() {
        let v = match six.to_integer().to_i64()? {
            0 | 6 => n(0),
            1 | 5 => half(),
            2 | 4 => s3(),
            3 => n(1),
            7 | 11 => neg(half()),
            8 | 10 => neg(s3()),
            9 => n(-1),
            _ => return None,
        };
        return Some(v);
    }
    let four = &t * int(4);
    if four.is_integer() {
        let v = match four.to_integer().to_i64()? {
            1 | 3 => s2(),
            5 | 7 => neg(s2()),
            _ => return None,
        };
        return Some(v);
    }
    None
}

fn exact_trig(f: Func, r: &Q) -> Result<Option<Sym>, EngineError> {
    let quarter_turn = qq(1, 2);
    Ok(match f {
        Func::Sin => sin_table(r),
        Func::Cos => sin_table(&(r + quarter_turn)),
        Func::Tan => match (sin_table(r), sin_table(&(r + quarter_turn))) {
            (Some(_), Some(c)) if c.is_zero() => {
                return Err(EngineError::new("tan is undefined here (cos = 0)"));
            }
            (Some(s), Some(c)) => Some(div(s, c)?),
            _ => None,
        },
        _ => None,
    })
}

fn inverse_trig(f: Func, x: Sym, angle: AngleMode) -> SR {
    // (value, angle as a multiple of π) for the non-negative half; odd symmetry covers the rest.
    let sin_vals = [
        (n(0), qq(0, 1)),
        (q(1, 2), qq(1, 6)),
        (mul(vec![q(1, 2), sqrt_of(2)]), qq(1, 4)),
        (mul(vec![q(1, 2), sqrt_of(3)]), qq(1, 3)),
        (n(1), qq(1, 2)),
    ];
    let tan_vals = [
        (n(0), qq(0, 1)),
        (mul(vec![q(1, 3), sqrt_of(3)]), qq(1, 6)),
        (n(1), qq(1, 4)),
        (sqrt_of(3), qq(1, 3)),
    ];
    let lookup = |table: &[(Sym, Q)]| -> Option<Q> {
        let negx = neg(x.clone());
        table.iter().find_map(|(v, r)| {
            if *v == x {
                Some(r.clone())
            } else if *v == negx {
                Some(-r.clone())
            } else {
                None
            }
        })
    };
    let r = match f {
        Func::Asin => lookup(&sin_vals),
        Func::Acos => lookup(&sin_vals).map(|r| qq(1, 2) - r),
        Func::Atan => lookup(&tan_vals),
        _ => None,
    };
    Ok(match r {
        Some(r) => match angle {
            AngleMode::Radians => mul(vec![Sym::Num(r), Sym::Const(Const::Pi)]),
            AngleMode::Degrees => Sym::Num(r * int(180)),
        },
        None => Sym::Func(f, Box::new(x)),
    })
}
