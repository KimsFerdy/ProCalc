//! Σ and Π.
//!
//! Exact first: Faulhaber's formulas for sums of powers, geometric series, ζ(2m) = Σ 1/k^{2m},
//! n! for Π k. Then term by term for short finite ranges, and otherwise numerically at the
//! working precision: alternating series by the Cohen–Villegas–Zagier acceleration, fast
//! (ratio < 1) series directly, and slow power-law series by Richardson extrapolation of the
//! partial sums. A series whose terms don't shrink, or shrink like 1/k, diverges.

use astro_float::BigFloat;
use num_bigint::BigInt;
use num_rational::BigRational as Q;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::ast::{Const, Expr, Func};
use crate::calculus::free_of;
use crate::eval::Evaluator;
use crate::latex::latex;
use crate::limits::{Lim, Point};
use crate::number::{RM, Value};
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow};
use crate::{EngineError, Method, Step};

type R<T> = Result<T, EngineError>;

/// Finite sums with no closed form are added term by term up to this many terms…
const MAX_EXACT_TERMS: i64 = 2_000;
/// …and numerically up to this many.
const MAX_NUMERIC_TERMS: i64 = 200_000;
/// A fast (ratio < 1) series has settled well before this many terms.
const FAST_TERMS: i64 = 50_000;
/// Largest power of k summed by Faulhaber's formula.
const MAX_POWER: i64 = 40;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn is_inf(s: &Sym) -> bool {
    *s == Sym::Const(Const::Inf)
}

/// B₀ … B_m with B₁ = +½ (the convention Faulhaber's formula wants).
fn bernoulli(m: usize) -> Vec<Q> {
    let mut b: Vec<Q> = vec![Q::one()];
    for k in 1..=m {
        // Σ_{j<k} C(k+1, j) Bⱼ + (k+1)B_k = 0
        let mut c = BigInt::one();
        let mut s = Q::zero();
        for (j, bj) in b.iter().enumerate() {
            s += Q::from_integer(c.clone()) * bj;
            c = c * BigInt::from(k + 1 - j) / BigInt::from(j + 1);
        }
        b.push(-s / Q::from_integer(BigInt::from(k + 1)));
    }
    if m >= 1 {
        b[1] = -b[1].clone();
    }
    b
}

fn binom(n: usize, k: usize) -> BigInt {
    (0..k).fold(BigInt::one(), |acc, i| acc * (n - i) / (i + 1))
}

/// Σ_{k=1}^{N} k^m as a polynomial in N.
fn faulhaber(m: usize, big_n: &Sym) -> Sym {
    let b = bernoulli(m);
    let terms = (0..=m)
        .map(|j| {
            let c = Q::from_integer(binom(m + 1, j)) * &b[j] / Q::from_integer(BigInt::from(m + 1));
            mul(vec![Sym::Num(c), sym::pow_or_keep(big_n.clone(), n((m + 1 - j) as i64))])
        })
        .collect();
    expand(add(terms))
}

fn expand(s: Sym) -> Sym {
    crate::poly::expand(&s).unwrap_or(s)
}

/// One term of the body, as coefficient·k^m·r^k (r = 1 when there is no geometric part).
struct Term {
    coeff: Sym,
    power: i64,
    ratio: Sym,
}

/// The body as a sum of c·k^m·r^k terms, if it is one.
fn terms_of(f: &Sym, k: &str) -> Option<Vec<Term>> {
    let ts = match expand(f.clone()) {
        Sym::Add(v) => v,
        other => vec![other],
    };
    let mut out = Vec::new();
    for t in ts {
        let factors = match t {
            Sym::Mul(v) => v,
            other => vec![other],
        };
        let mut term = Term { coeff: n(1), power: 0, ratio: n(1) };
        let mut coeff = Vec::new();
        for fac in factors {
            match &fac {
                Sym::Var(v) if v == k => term.power += 1,
                Sym::Pow(b, e) if matches!(&**b, Sym::Var(v) if v == k) => term.power += e.as_num().filter(|e| e.is_integer())?.to_i64()?,
                // r^{αk+β} = r^β·(r^α)^k
                Sym::Pow(b, e) if free_of(b, k) && !free_of(e, k) => {
                    let (alpha, beta) = linear_in(e, k)?;
                    coeff.push(pow((**b).clone(), beta).ok()?);
                    term.ratio = mul(vec![term.ratio, pow((**b).clone(), alpha).ok()?]);
                }
                other if free_of(other, k) => coeff.push(other.clone()),
                _ => return None,
            }
        }
        term.coeff = mul(coeff);
        if term.power < 0 && !term.ratio.is_one() {
            return None;
        }
        out.push(term);
    }
    Some(out)
}

/// e = α·k + β, with α, β free of k.
fn linear_in(e: &Sym, k: &str) -> Option<(Sym, Sym)> {
    let ts = match expand(e.clone()) {
        Sym::Add(v) => v,
        other => vec![other],
    };
    let (mut alpha, mut beta) = (Vec::new(), Vec::new());
    for t in ts {
        if free_of(&t, k) {
            beta.push(t);
        } else {
            let (c, rest) = sym::split_coeff(t);
            match rest {
                Sym::Var(v) if v == k => alpha.push(Sym::Num(c)),
                Sym::Mul(fs) if fs.iter().filter(|f| **f == Sym::Var(k.into())).count() == 1 => {
                    let others: Vec<Sym> = fs.into_iter().filter(|f| *f != Sym::Var(k.into())).collect();
                    if others.iter().any(|o| !free_of(o, k)) {
                        return None;
                    }
                    let mut v = vec![Sym::Num(c)];
                    v.extend(others);
                    alpha.push(mul(v));
                }
                _ => return None,
            }
        }
    }
    Some((add(alpha), add(beta)))
}

/// ζ(2m) = (−1)^{m+1} B_{2m} (2π)^{2m} / (2·(2m)!)
fn zeta_even(p: usize) -> R<Sym> {
    let b = bernoulli(p);
    let fact = (1..=p).fold(BigInt::one(), |acc, i| acc * i);
    let sign = if (p / 2) % 2 == 1 { Q::one() } else { -Q::one() };
    let c = sign * &b[p] * Q::from_integer(BigInt::from(2).pow(p as u32)) / Q::from_integer(fact * 2);
    Ok(mul(vec![Sym::Num(c), pow(Sym::Const(Const::Pi), n(p as i64))?]))
}

impl Builder<'_> {
    /// `\sum_{k=a}^{b} f` or `\prod_{k=a}^{b} f`.
    pub(crate) fn sum(&mut self, body: &Expr, k: &str, lo: &Expr, hi: &Expr, product: bool) -> R<Sym> {
        let f = self.build_free(body, &[k.to_string()])?;
        let (a, b) = (self.build(lo)?, self.build(hi)?);
        if is_inf(&a) || a.as_num().is_some_and(|v| !v.is_integer()) || b.as_num().is_some_and(|v| !v.is_integer()) {
            return Err(EngineError::new("A sum runs over whole numbers, from a whole number up"));
        }
        let symbol = if product { "\\prod" } else { "\\sum" };
        let shown = format!("{symbol}_{{{k}={}}}^{{{}}}{}", latex(&a), if is_inf(&b) { "\\infty".into() } else { latex(&b) }, latex(&f));
        let mut steps = vec![step(shown.clone(), if product { "The product" } else { "The sum" })];
        let count = match (a.as_num(), b.as_num()) {
            (Some(x), Some(y)) => (y - x).to_integer().to_i64().map(|c| c + 1),
            _ => None,
        };
        if count.is_some_and(|c| c <= 0) {
            steps.push(step(if product { "1" } else { "0" }, "The range is empty"));
            self.ev.work.push(Method { name: if product { "Product" } else { "Sum" }.into(), steps });
            return Ok(n(if product { 1 } else { 0 }));
        }
        let result = if product {
            self.product(&f, k, &a, &b, count, &mut steps)?
        } else {
            self.series(&f, k, &a, &b, count, &mut steps)?
        };
        if !matches!(result, Sym::Sum { .. }) {
            steps.push(step(format!("{shown}={}", latex(&result)), "The result"));
        }
        self.ev.work.push(Method { name: if product { "Product" } else { "Sum" }.into(), steps });
        Ok(result)
    }

    fn term_by_term(&mut self, f: &Sym, k: &str, a: &Sym, c: i64, product: bool, steps: &mut Vec<Step>) -> R<Sym> {
        let start = a.as_num().expect("numeric bounds").to_integer();
        let mut parts = Vec::new();
        for j in 0..c {
            parts.push(self.subst(f, k, &Sym::Num(Q::from_integer(&start + j)))?);
        }
        let shown: Vec<String> = parts.iter().take(5).map(latex).collect();
        let more = if c > 5 { if product { "\\cdots" } else { "+\\cdots" } } else { "" };
        let join = if product { "\\cdot " } else { "+" };
        steps.push(step(format!("{}{more}", shown.join(join)), format!("Write out the {c} terms and {}", if product { "multiply" } else { "add" })));
        Ok(if product { mul(parts) } else { add(parts) })
    }

    fn product(&mut self, f: &Sym, k: &str, a: &Sym, b: &Sym, count: Option<i64>, steps: &mut Vec<Step>) -> R<Sym> {
        if is_inf(b) {
            return Err(EngineError::new("Infinite products aren't supported yet"));
        }
        if free_of(f, k) {
            // c·c·…·c, (b − a + 1) times
            let times = add(vec![b.clone(), neg(a.clone()), n(1)]);
            steps.push(step(format!("{}^{{{}}}", latex(f), latex(&times)), "The same factor every time"));
            return pow(f.clone(), times);
        }
        if *f == Sym::Var(k.into()) && a.is_one() && b.has_vars() {
            steps.push(step(format!("{}!", latex(b)), "$1\\cdot 2\\cdots n=n!$"));
            return Ok(Sym::Factorial(Box::new(b.clone()), 1));
        }
        match count {
            Some(c) if c <= MAX_EXACT_TERMS => self.term_by_term(f, k, a, c, true, steps),
            _ => Err(EngineError::new("I can't find a closed form for this product")),
        }
    }

    fn series(&mut self, f: &Sym, k: &str, a: &Sym, b: &Sym, count: Option<i64>, steps: &mut Vec<Step>) -> R<Sym> {
        let infinite = is_inf(b);
        if let Some(s) = self.closed_sum(f, k, a, b, steps)? {
            return Ok(s);
        }
        if !infinite {
            return match count {
                Some(c) if c <= MAX_EXACT_TERMS => self.term_by_term(f, k, a, c, false, steps),
                Some(c) if c <= MAX_NUMERIC_TERMS && !f.has_vars_except(k) => {
                    steps.push(step("", format!("No closed form: add the {c} terms numerically")));
                    Ok(Sym::Sum { body: Box::new(f.clone()), var: k.into(), lo: Box::new(a.clone()), hi: Box::new(b.clone()) })
                }
                Some(_) if !f.has_vars_except(k) => Err(EngineError::new(format!(
                    "I can't find a closed form for this sum, and it has too many terms to add up (the limit is {MAX_NUMERIC_TERMS})"
                ))),
                _ => Err(EngineError::new("I can't find a closed form for this sum")),
            };
        }
        if f.has_vars_except(k) {
            return Err(EngineError::new("I can't find a closed form for this series"));
        }
        // ζ(2m) and its tails.
        if let Some(s) = self.zeta_sum(f, k, a, steps)? {
            return Ok(s);
        }
        self.divergence_tests(f, k, steps)?;
        steps.push(step(
            "",
            "No closed form: the series is summed numerically (alternating series are accelerated; others are extrapolated from partial sums)",
        ));
        Ok(Sym::Sum { body: Box::new(f.clone()), var: k.into(), lo: Box::new(a.clone()), hi: Box::new(b.clone()) })
    }

    /// Sums of c·k^m and c·r^k terms, from a to b (b may be ∞ for geometric terms).
    fn closed_sum(&mut self, f: &Sym, k: &str, a: &Sym, b: &Sym, steps: &mut Vec<Step>) -> R<Option<Sym>> {
        let Some(terms) = terms_of(f, k) else { return Ok(None) };
        let infinite = is_inf(b);
        let mut total = Vec::new();
        for t in &terms {
            if t.ratio.is_one() {
                if t.power < 0 || t.power > MAX_POWER {
                    return Ok(None);
                }
                if infinite {
                    if t.coeff.is_zero() {
                        continue;
                    }
                    let s = self.sign_of(&t.coeff).unwrap_or(1);
                    steps.push(step("", "The terms don't shrink to 0, so the series diverges"));
                    return Ok(Some(if s < 0 { neg(Sym::Const(Const::Inf)) } else { Sym::Const(Const::Inf) }));
                }
                let m = t.power as usize;
                // Σ_{a}^{b} k^m = S(b) − S(a−1)
                let upper = faulhaber(m, b);
                let lower = faulhaber(m, &add(vec![a.clone(), n(-1)]));
                let s = expand(add(vec![upper, neg(lower)]));
                let formula = match m {
                    0 => "$\\sum 1$ counts the terms".to_string(),
                    1 => "$\\sum_{k=1}^{n} k=\\frac{n(n+1)}{2}$".to_string(),
                    2 => "$\\sum_{k=1}^{n} k^2=\\frac{n(n+1)(2n+1)}{6}$".to_string(),
                    3 => "$\\sum_{k=1}^{n} k^3=\\left(\\frac{n(n+1)}{2}\\right)^2$".to_string(),
                    _ => format!("Faulhaber's formula for $\\sum k^{{{m}}}$"),
                };
                let km = latex(&sym::pow_or_keep(Sym::Var(k.into()), n(m as i64)));
                steps.push(step(format!("\\sum {km}={}", latex(&s)), formula));
                total.push(mul(vec![t.coeff.clone(), s]));
            } else {
                if t.power != 0 || !free_of(&t.ratio, k) {
                    return Ok(None);
                }
                let r = t.ratio.clone();
                let first = mul(vec![t.coeff.clone(), pow(r.clone(), a.clone())?]);
                if infinite {
                    if r.has_vars() {
                        return Ok(None);
                    }
                    // |r| < 1 converges to first/(1 − r).
                    let size = self.func(Func::Abs, r.clone())?;
                    if self.sign_of(&add(vec![size, n(-1)]))? >= 0 {
                        steps.push(step("", format!("A geometric series with ratio ${}$, and $|r|\\ge 1$: it diverges", latex(&r))));
                        return Err(EngineError::new("This series diverges"));
                    }
                    let s = div(first.clone(), add(vec![n(1), neg(r.clone())]))?;
                    steps.push(step(
                        format!("\\frac{{{}}}{{1-{}}}={}", latex(&first), wrap(&r), latex(&s)),
                        format!("A geometric series with ratio $r={}$, $|r|<1$: first term over $1-r$", latex(&r)),
                    ));
                    total.push(s);
                } else {
                    // first·(1 − r^{b−a+1})/(1 − r)
                    let count = add(vec![b.clone(), neg(a.clone()), n(1)]);
                    if r.is_one() {
                        total.push(mul(vec![t.coeff.clone(), count]));
                        continue;
                    }
                    let s = div(mul(vec![first.clone(), add(vec![n(1), neg(pow(r.clone(), count.clone())?)])]), add(vec![n(1), neg(r.clone())]))?;
                    let s = if s.has_vars() { s } else { expand(s) };
                    steps.push(step(
                        format!("{}\\cdot\\frac{{1-{}^{{{}}}}}{{1-{}}}={}", wrap(&first), wrap(&r), latex(&count), wrap(&r), latex(&s)),
                        format!("A geometric series: first term ${}$, ratio ${}$", latex(&first), latex(&r)),
                    ));
                    total.push(s);
                }
            }
        }
        let s = add(total);
        Ok(Some(if s.has_vars() { crate::poly::factor(&s).unwrap_or(s) } else { s }))
    }

    /// Σ_{k=a}^{∞} c/k^{2m} = c·(ζ(2m) − Σ_{k<a} 1/k^{2m}).
    fn zeta_sum(&mut self, f: &Sym, k: &str, a: &Sym, steps: &mut Vec<Step>) -> R<Option<Sym>> {
        let (c, rest) = sym::split_coeff(f.clone());
        let Sym::Pow(base, e) = &rest else { return Ok(None) };
        if **base != Sym::Var(k.into()) {
            return Ok(None);
        }
        let Some(p) = e.as_num().filter(|e| e.is_integer()).and_then(|e| (-e).to_integer().to_i64()) else { return Ok(None) };
        let Some(start) = a.as_num().and_then(|a| a.to_integer().to_i64()) else { return Ok(None) };
        if p < 2 || p % 2 == 1 || p > 60 || !(1..=1000).contains(&start) {
            return Ok(None);
        }
        let zeta = zeta_even(p as usize)?;
        let head: Vec<Sym> = (1..start).map(|j| Sym::Num(Q::new(BigInt::one(), BigInt::from(j).pow(p as u32)))).collect();
        let s = mul(vec![Sym::Num(c), add(vec![zeta.clone(), neg(add(head))])]);
        steps.push(step(format!("\\zeta({p})=\\sum_{{k=1}}^{{\\infty}}\\frac{{1}}{{k^{{{p}}}}}={}", latex(&zeta)), "Euler's value of $\\zeta$ at an even number"));
        Ok(Some(s))
    }

    /// Terms that don't go to 0, or shrink only like 1/k, make the series diverge.
    fn divergence_tests(&mut self, f: &Sym, k: &str, steps: &mut Vec<Step>) -> R<()> {
        let Ok((l, _)) = self.limit(f, k, Point::PosInf, 0) else { return Ok(()) };
        let nonzero = match &l {
            Lim::Fin(s) => !s.is_zero(),
            _ => true,
        };
        if nonzero {
            steps.push(step(format!("\\lim_{{{k}\\to\\infty}}{}\\ne 0", latex(f)), "The terms don't shrink to 0, so the series diverges"));
            return Err(EngineError::new("This series diverges: its terms don't go to 0"));
        }
        // k·f → c ≠ 0 with terms of one sign: compare with the harmonic series.
        let kf = mul(vec![Sym::Var(k.into()), f.clone()]);
        if let Ok((l, _)) = self.limit(&kf, k, Point::PosInf, 0) {
            let comparable = match &l {
                Lim::Fin(s) => !s.is_zero(),
                _ => true,
            };
            if comparable {
                steps.push(step(
                    format!("\\lim_{{{k}\\to\\infty}}{}\\ne 0", latex(&kf)),
                    "The terms shrink like $\\frac{1}{k}$ (or slower), and $\\sum\\frac{1}{k}$ diverges",
                ));
                return Err(EngineError::new("This series diverges: its terms shrink like 1/k or slower"));
            }
        }
        Ok(())
    }
}

fn wrap(s: &Sym) -> String {
    match s {
        Sym::Num(v) if v.is_integer() && !v.is_negative() => latex(s),
        Sym::Var(_) | Sym::Const(_) => latex(s),
        _ => format!("\\left({}\\right)", latex(s)),
    }
}

// ---------- Numeric sums ----------

/// f(k) at the evaluator's precision, as a real float.
fn term(f: &Expr, k: &str, at: i64, ev: &mut Evaluator) -> R<BigFloat> {
    ev.vars.insert(k.to_string(), Value::Exact(Q::from_integer(at.into())));
    match ev.eval(f)? {
        Value::Complex(..) => Err(EngineError::new("Numeric sums need real terms")),
        v => Ok(ev.prec.float_of(&v)),
    }
}

/// Σ_{k=a}^{b} f(k) numerically; `b` None is ∞.
pub fn numeric_sum(f: &Expr, k: &str, a: i64, b: Option<i64>, ev: &mut Evaluator) -> R<Value> {
    let saved = ev.vars.get(k).cloned();
    let result = match b {
        Some(b) => (|| {
            let p = ev.prec.bits;
            let mut s = BigFloat::from_word(0, p);
            for j in a..=b {
                s = s.add(&term(f, k, j, ev)?, p, RM);
            }
            Ok(Value::Real(s))
        })(),
        None => infinite_sum(f, k, a, ev),
    };
    match saved {
        Some(v) => ev.vars.insert(k.to_string(), v),
        None => ev.vars.remove(k),
    };
    result
}

fn infinite_sum(f: &Expr, k: &str, a: i64, ev: &mut Evaluator) -> R<Value> {
    let p = ev.prec.bits;
    let digits = ev.prec.digits as f64;
    let sign = |x: &BigFloat| if x.is_zero() { 0 } else if crate::number::is_negative_float(x) { -1 } else { 1 };
    // Look at terms a little way out to choose a method.
    let probe: Vec<BigFloat> = (20..28).map(|j| term(f, k, a + j, ev)).collect::<R<_>>()?;
    let alternating = probe.windows(2).all(|w| sign(&w[0]) * sign(&w[1]) < 0);
    if alternating {
        return Ok(Value::Real(cvz(f, k, a, ev)?));
    }
    // Far out, a geometric-like series keeps a ratio below 1 while a power law creeps up to
    // 1: (1000/1001)^p ≈ 1 − p/1000.
    let (far, next) = (term(f, k, a + 1000, ev)?, term(f, k, a + 1001, ev)?);
    let ratio = if far.is_zero() { BigFloat::from_word(0, p) } else { next.div(&far, p, RM).abs() };
    let fast = crate::number::float_to_f64(&ratio).is_some_and(|r| r < 0.97);
    if fast {
        // Add terms until they fall below the precision.
        let mut s = BigFloat::from_word(0, p);
        let tiny = BigFloat::from_f64(10f64.powf(-(digits.min(300.0)) - 8.0), p);
        for j in 0..FAST_TERMS {
            let t = term(f, k, a + j, ev)?;
            s = s.add(&t, p, RM);
            if j > 10 && t.abs().cmp(&s.abs().mul(&tiny, p, RM)).is_some_and(|c| c < 0) {
                return Ok(Value::Real(s));
            }
        }
        return Err(EngineError::new("The series didn't settle numerically"));
    }
    // Richardson is only right when the tail is a series in whole powers of 1/N (1/k³ yes,
    // 1/k^1.5 and ln k/k² no). A wrong model still looks stable, so extrapolate from two
    // different sets of partial sums and insist that they agree.
    let first = richardson(f, k, a, 8, ev)?;
    let second = richardson(f, k, a, 11, ev)?;
    let gap = first.sub(&second, p, RM).abs();
    let tolerance = first.abs().add(&BigFloat::from_word(1, p), p, RM).mul(&BigFloat::from_f64(10f64.powf(-(digits.min(300.0)) - 4.0), p), p, RM);
    if gap.cmp(&tolerance).is_some_and(|c| c > 0) {
        return Err(EngineError::new("This series converges too slowly (or too irregularly) for me to sum it accurately"));
    }
    Ok(Value::Real(first))
}

/// Σ (−1)^j a_j by Cohen, Villegas and Zagier's Algorithm 1: error about 5.8^{−n}.
fn cvz(f: &Expr, k: &str, a: i64, ev: &mut Evaluator) -> R<BigFloat> {
    let p = ev.prec.bits;
    let terms = (ev.prec.digits as f64 * 1.31 + 8.0).ceil() as i64;
    let nn = BigFloat::from_word(terms as u64, p);
    let root8 = BigFloat::from_word(8, p).sqrt(p, RM);
    let mut d = BigFloat::from_word(3, p).add(&root8, p, RM).powi(terms as usize, p, RM);
    d = d.add(&BigFloat::from_word(1, p).div(&d, p, RM), p, RM).div(&BigFloat::from_word(2, p), p, RM);
    let mut b = BigFloat::from_word(1, p).neg();
    let mut c = d.neg();
    let mut s = BigFloat::from_word(0, p);
    let half = BigFloat::from_word(1, p).div(&BigFloat::from_word(2, p), p, RM);
    for j in 0..terms {
        // a_j = (−1)^j t_j, so the series is Σ (−1)^j a_j.
        let t = term(f, k, a + j, ev)?;
        let aj = if j % 2 == 0 { t } else { t.neg() };
        c = b.sub(&c, p, RM);
        s = s.add(&c.mul(&aj, p, RM), p, RM);
        let jf = BigFloat::from_word(j as u64, p);
        let num = jf.add(&nn, p, RM).mul(&jf.sub(&nn, p, RM), p, RM);
        let den = jf.add(&half, p, RM).mul(&jf.add(&BigFloat::from_word(1, p), p, RM), p, RM);
        b = b.mul(&num, p, RM).div(&den, p, RM);
    }
    Ok(s.div(&d, p, RM))
}

/// Partial sums S(N) at N = c, 2c, 3c, … extrapolated to N = ∞ as a polynomial in 1/N
/// (Neville's scheme): right for terms with an expansion in powers of 1/k, like 1/k³.
fn richardson(f: &Expr, k: &str, a: i64, step_n: i64, ev: &mut Evaluator) -> R<BigFloat> {
    let p = ev.prec.bits;
    let levels = (ev.prec.digits as f64 * 0.5 + 12.0).ceil() as usize;
    let mut s = BigFloat::from_word(0, p);
    let mut next = a;
    let mut table: Vec<BigFloat> = Vec::new();
    let mut hs: Vec<BigFloat> = Vec::new();
    for level in 1..=levels {
        let upto = a + step_n * level as i64;
        while next < upto {
            s = s.add(&term(f, k, next, ev)?, p, RM);
            next += 1;
        }
        // The tail is a series in 1/k at the last index k (not in 1/count: a sum may start far out).
        let last = (a - 1).max(0) + step_n * level as i64;
        hs.push(BigFloat::from_word(1, p).div(&BigFloat::from_word(last as u64, p), p, RM));
        table.push(s.clone());
        // Neville: P_{i..j}(0) from P_{i..j−1} and P_{i+1..j}.
        let j = table.len() - 1;
        for i in (0..j).rev() {
            let num = table[i + 1].mul(&hs[i], p, RM).sub(&table[i].mul(&hs[j], p, RM), p, RM);
            table[i] = num.div(&hs[i].sub(&hs[j], p, RM), p, RM);
        }
    }
    Ok(table[0].clone())
}

/// `Expr::Sum` in the numeric evaluator (only the sums the symbolic layer left).
pub fn eval_sum(body: &Expr, k: &str, lo: &Expr, hi: &Expr, ev: &mut Evaluator) -> R<Value> {
    let bound = |e: &Expr, ev: &mut Evaluator| -> R<i64> {
        match ev.eval(e)? {
            Value::Exact(q) if q.is_integer() => q.to_integer().to_i64().ok_or_else(|| EngineError::new("Sum bounds are too large")),
            _ => Err(EngineError::new("Sum bounds must be whole numbers")),
        }
    };
    let a = bound(lo, ev)?;
    let b = match hi {
        Expr::Const(Const::Inf) => None,
        e => Some(bound(e, ev)?),
    };
    if b.is_some_and(|b| b - a >= MAX_NUMERIC_TERMS) {
        return Err(EngineError::new(format!("Too many terms to add up numerically (the limit is {MAX_NUMERIC_TERMS})")));
    }
    numeric_sum(body, k, a, b, ev)
}
