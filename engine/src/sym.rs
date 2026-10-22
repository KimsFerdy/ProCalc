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
use crate::eval::{self, Evaluator};
use crate::number::{self, Value, int};
use crate::{AngleMode, EngineError};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sym {
    Num(Q),
    Const(Const),
    Var(String),
    Add(Vec<Sym>),
    Mul(Vec<Sym>),
    Pow(Box<Sym>, Box<Sym>),
    Func(Func, Box<Sym>),
    /// log base `.0` of `.1`.
    Log(Box<Sym>, Box<Sym>),
    /// `n!` (step 1) or `n!!` (step 2).
    Factorial(Box<Sym>, u32),
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
            Sym::Num(_) | Sym::Const(_) => false,
            Sym::Add(v) | Sym::Mul(v) => v.iter().any(Sym::has_vars),
            Sym::Pow(a, b) | Sym::Log(a, b) => a.has_vars() || b.has_vars(),
            Sym::Func(_, a) | Sym::Factorial(a, _) => a.has_vars(),
        }
    }

    /// True when the expression is a "closed form" worth showing as the exact answer:
    /// numbers, surds, π, e and variables, but no unevaluated functions like sin(1).
    pub fn is_closed_form(&self) -> bool {
        match self {
            Sym::Num(_) | Sym::Const(_) | Sym::Var(_) => true,
            Sym::Add(v) | Sym::Mul(v) => v.iter().all(Sym::is_closed_form),
            // A huge integer power we refused to expand (2^(10^9)) isn't useful as "exact".
            Sym::Pow(a, b) => {
                let unexpanded = matches!((&**a, &**b), (Sym::Num(_), Sym::Num(e)) if e.is_integer());
                !unexpanded && a.is_closed_form() && b.is_closed_form()
            }
            Sym::Func(..) | Sym::Log(..) | Sym::Factorial(..) => false,
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
            Sym::Add(v) => fold(BinOp::Add, v),
            Sym::Mul(v) => fold(BinOp::Mul, v),
            Sym::Pow(a, b) => Expr::Pow(Box::new(a.to_expr()), Box::new(b.to_expr())),
            Sym::Func(f, a) => Expr::Call(*f, Box::new(a.to_expr())),
            Sym::Log(b, x) => Expr::LogBase(Box::new(b.to_expr()), Box::new(x.to_expr())),
            Sym::Factorial(a, step) => Expr::Factorial(Box::new(a.to_expr()), *step),
        }
    }
}

// ---------- Canonical constructors ----------

/// Splits `3·x·y` into (3, x·y); a plain number into (n, 1).
fn split_coeff(t: Sym) -> (Q, Sym) {
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
fn pow_or_keep(b: Sym, e: Sym) -> Sym {
    pow(b.clone(), e.clone()).unwrap_or_else(|_| Sym::Pow(Box::new(b), Box::new(e)))
}

fn is_numeric_surd(s: &Sym) -> bool {
    matches!(s, Sym::Pow(b, e) if matches!((&**b, &**e), (Sym::Num(bq), Sym::Num(eq)) if bq.is_integer() && !eq.is_integer()))
}

pub fn mul(factors: Vec<Sym>) -> Sym {
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
        if qd % 2 == 0 {
            return Err(eval::complex_err());
        }
        // Odd root of a negative number is real: (-8)^(1/3) = -2
        let mag = num_pow(&-b, e)?;
        return Ok(if p % 2 != 0 { neg(mag) } else { mag });
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
}

impl Builder<'_> {
    pub fn build(&mut self, e: &Expr) -> SR {
        Ok(match e {
            Expr::Number(s) => Sym::Num(
                number::parse_exact(s).ok_or_else(|| EngineError::new(format!("Bad number '{s}'")))?,
            ),
            Expr::Const(c) => Sym::Const(*c),
            Expr::Var(v) => Sym::Var(v.clone()),
            Expr::Neg(x) => neg(self.build(x)?),
            Expr::Binary(op, a, b) => {
                let (a, b) = (self.build(a)?, self.build(b)?);
                match op {
                    BinOp::Add => add(vec![a, b]),
                    BinOp::Sub => add(vec![a, neg(b)]),
                    BinOp::Mul => mul(vec![a, b]),
                    BinOp::Div => div(a, b)?,
                }
            }
            Expr::Pow(a, b) => {
                let a = self.build(a)?;
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
                        self.func(*f, arg)?
                    }
                }
            }
        })
    }

    /// Numeric value of a variable-free expression.
    fn numeric(&mut self, s: &Sym) -> Result<Value, EngineError> {
        self.ev.eval(&s.to_expr())
    }

    fn factorial(&mut self, x: Sym, step: u32) -> SR {
        if x.has_vars() {
            return Ok(Sym::Factorial(Box::new(x), step));
        }
        match self.numeric(&x)? {
            v @ Value::Exact(_) => match self.ev.factorial(v, step)? {
                Value::Exact(r) => Ok(Sym::Num(r)),
                Value::Real(_) => Ok(Sym::Factorial(Box::new(x), step)),
            },
            Value::Real(_) => Ok(Sym::Factorial(Box::new(x), step)),
        }
    }

    fn func(&mut self, f: Func, x: Sym) -> SR {
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
                    Value::Real(_) => Ok(Sym::Func(f, Box::new(x))),
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
            Func::Sinh | Func::Tanh | Func::Asinh | Func::Atanh if x.is_zero() => Ok(n(0)),
            Func::Cosh if x.is_zero() => Ok(n(1)),
            Func::Acosh if x.is_one() => Ok(n(0)),
            Func::Atanh if matches!(x.as_num(), Some(v) if v.abs() >= Q::one()) => {
                Err(EngineError::new("artanh needs |x| < 1"))
            }
            Func::Acosh if matches!(x.as_num(), Some(v) if *v < Q::one()) => Err(eval::complex_err()),
            _ => Ok(Sym::Func(f, Box::new(x))),
        }
    }
}

/// Things we know are positive: positive numbers, π, e, positive surds.
fn known_positive(s: &Sym) -> bool {
    match s {
        Sym::Num(v) => v.is_positive(),
        Sym::Const(_) => true,
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

fn ln(x: Sym) -> SR {
    match &x {
        Sym::Num(v) if v.is_zero() => return Err(EngineError::new("Logarithm of zero is undefined")),
        Sym::Num(v) if v.is_negative() => return Err(eval::complex_err()),
        Sym::Num(v) if v.is_one() => return Ok(n(0)),
        Sym::Const(Const::E) => return Ok(n(1)),
        Sym::Pow(b, k) if **b == Sym::Const(Const::E) => return Ok((**k).clone()),
        _ => {}
    }
    Ok(Sym::Func(Func::Ln, Box::new(x)))
}

fn log_base(b: Sym, x: Sym) -> SR {
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
        Sym::Num(v) if v.is_negative() => return Err(eval::complex_err()),
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
    if let Some(v) = x.as_num()
        && matches!(f, Func::Asin | Func::Acos) && v.abs() > Q::one() {
            return Err(eval::complex_err());
        }
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
