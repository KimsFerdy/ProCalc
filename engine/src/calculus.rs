//! Derivatives and antiderivatives of symbolic expressions, plus numeric integration.
//!
//! Differentiation is exact rule application (power, product, chain, every built-in
//! function). Integration tries, in order: linearity and constant factors, a table of
//! standard integrals (also for a linear inner argument, f(ax+b)), u-substitution,
//! integration by parts for polynomial × exp/sin/cos and polynomial × ln/arctan/arcsin,
//! a few quadratic forms (1/(x²+a²), 1/√(a²−x²)), and expanding polynomial products.
//! The caller checks every antiderivative by differentiating it back numerically, so a
//! slip in these rules shows up as "no antiderivative" rather than a wrong answer.

use std::collections::BTreeSet;

use astro_float::BigFloat;
use num_rational::BigRational as Q;
use num_traits::{Signed, Zero};

use crate::ast::{Const, Func};
use crate::eval::Evaluator;
use crate::number::{RM, Value};
use crate::latex::latex;
use crate::sym::{self, SR, Sym, add, div, mul, n, neg, pow, q};
use crate::{AngleMode, EngineError, Step};

// ---------- Free variables ----------

pub fn free_vars(s: &Sym, out: &mut BTreeSet<String>) {
    match s {
        Sym::Var(v) => {
            out.insert(v.clone());
        }
        Sym::Num(_) | Sym::Const(_) | Sym::Unit(_) => {}
        Sym::Add(v) | Sym::Mul(v) => v.iter().for_each(|t| free_vars(t, out)),
        Sym::Pow(a, b) | Sym::Log(a, b) => {
            free_vars(a, out);
            free_vars(b, out);
        }
        Sym::Func(_, a) | Sym::Factorial(a, _) => free_vars(a, out),
        Sym::Matrix(m) => m.iter().flatten().for_each(|e| free_vars(e, out)),
        Sym::Integral { body, var, lo, hi } | Sym::Sum { body, var, lo, hi } => {
            let mut inner = BTreeSet::new();
            free_vars(body, &mut inner);
            inner.remove(var);
            out.extend(inner);
            free_vars(lo, out);
            free_vars(hi, out);
        }
    }
}

pub fn free_of(s: &Sym, x: &str) -> bool {
    let mut vars = BTreeSet::new();
    free_vars(s, &mut vars);
    !vars.contains(x)
}

fn var(x: &str) -> Sym {
    Sym::Var(x.to_string())
}

fn recip(s: Sym) -> SR {
    pow(s, n(-1))
}

fn pi() -> Sym {
    Sym::Const(Const::Pi)
}

/// π/180 in degree mode (the inner derivative of sin x° is π/180), else 1.
fn trig_factor(angle: AngleMode) -> Sym {
    match angle {
        AngleMode::Radians => n(1),
        AngleMode::Degrees => mul(vec![pi(), q(1, 180)]),
    }
}

fn func(f: Func, u: Sym) -> Sym {
    Sym::Func(f, Box::new(u))
}

// ---------- Derivatives ----------

pub fn diff(s: &Sym, x: &str, angle: AngleMode) -> SR {
    if free_of(s, x) {
        return Ok(n(0));
    }
    Ok(match s {
        Sym::Var(_) => n(1),
        Sym::Add(t) => add(t.iter().map(|t| diff(t, x, angle)).collect::<Result<_, _>>()?),
        Sym::Mul(f) => {
            // Product rule: Σ f₁⋯f′ₖ⋯fₙ
            let mut terms = Vec::new();
            for k in 0..f.len() {
                if free_of(&f[k], x) {
                    continue;
                }
                let mut fs = f.clone();
                fs[k] = diff(&f[k], x, angle)?;
                terms.push(mul(fs));
            }
            add(terms)
        }
        Sym::Pow(b, e) => {
            let (b, e) = (&**b, &**e);
            if free_of(e, x) {
                // (bᵉ)′ = e·bᵉ⁻¹·b′
                mul(vec![e.clone(), pow(b.clone(), add(vec![e.clone(), n(-1)]))?, diff(b, x, angle)?])
            } else if free_of(b, x) {
                // (bᵉ)′ = bᵉ·ln b·e′
                mul(vec![s.clone(), sym::ln(b.clone())?, diff(e, x, angle)?])
            } else {
                // (bᵉ)′ = bᵉ·(e′ ln b + e b′/b)
                let inner = add(vec![
                    mul(vec![diff(e, x, angle)?, sym::ln(b.clone())?]),
                    mul(vec![e.clone(), diff(b, x, angle)?, recip(b.clone())?]),
                ]);
                mul(vec![s.clone(), inner])
            }
        }
        Sym::Func(f, u) => mul(vec![outer_derivative(*f, u, angle)?, diff(u, x, angle)?]),
        Sym::Log(b, u) => {
            if free_of(b, x) {
                // (log_b u)′ = u′/(u ln b)
                mul(vec![diff(u, x, angle)?, recip((**u).clone())?, recip(sym::ln((**b).clone())?)?])
            } else {
                let quotient = div(sym::ln((**u).clone())?, sym::ln((**b).clone())?)?;
                diff(&quotient, x, angle)?
            }
        }
        Sym::Factorial(..) => return Err(EngineError::new("Factorials can't be differentiated")),
        Sym::Matrix(m) => Sym::Matrix(m.iter().map(|r| r.iter().map(|e| diff(e, x, angle)).collect::<Result<_, _>>()).collect::<Result<_, _>>()?),
        Sym::Integral { .. } | Sym::Sum { .. } => {
            return Err(EngineError::new("Differentiating an integral isn't supported yet"));
        }
        Sym::Num(_) | Sym::Const(_) | Sym::Unit(_) => n(0),
    })
}

/// f′(u), the derivative of the outer function at its argument.
fn outer_derivative(f: Func, u: &Sym, angle: AngleMode) -> SR {
    let u = u.clone();
    let k = trig_factor(angle);
    let one_minus_u2 = || add(vec![n(1), neg(pow_or_keep(u.clone(), 2))]);
    let u2_plus_one = || add(vec![pow_or_keep(u.clone(), 2), n(1)]);
    // Inverse trig in degree mode returns degrees, so its derivative carries 180/π.
    let inv_k = || match angle {
        AngleMode::Radians => n(1),
        AngleMode::Degrees => mul(vec![n(180), recip(pi()).expect("π is not zero")]),
    };
    Ok(match f {
        Func::Sin => mul(vec![k, func(Func::Cos, u)]),
        Func::Cos => neg(mul(vec![k, func(Func::Sin, u)])),
        Func::Tan => mul(vec![k, pow(func(Func::Cos, u), n(-2))?]),
        Func::Asin => mul(vec![inv_k(), pow(one_minus_u2(), q(-1, 2))?]),
        Func::Acos => neg(mul(vec![inv_k(), pow(one_minus_u2(), q(-1, 2))?])),
        Func::Atan => mul(vec![inv_k(), recip(u2_plus_one())?]),
        Func::Sinh => func(Func::Cosh, u),
        Func::Cosh => func(Func::Sinh, u),
        Func::Tanh => pow(func(Func::Cosh, u), n(-2))?,
        Func::Asinh => pow(u2_plus_one(), q(-1, 2))?,
        Func::Acosh => pow(add(vec![pow_or_keep(u, 2), n(-1)]), q(-1, 2))?,
        Func::Atanh => recip(one_minus_u2())?,
        Func::Ln => recip(u)?,
        // |u|′ = u/|u|
        Func::Abs => mul(vec![u.clone(), recip(func(Func::Abs, u))?]),
        Func::Floor | Func::Ceil => n(0),
        // Never stored: sqrt and exp become powers, log becomes Log.
        Func::Sqrt => mul(vec![q(1, 2), pow(u, q(-1, 2))?]),
        Func::Exp => pow(Sym::Const(Const::E), u)?,
        Func::Log => recip(mul(vec![u, sym::ln(n(10))?]))?,
        // Γ′ = Γ·ψ, and the digamma function ψ isn't here yet.
        Func::Gamma => return Err(EngineError::new("Differentiating Γ needs the digamma function, which isn't supported yet")),
        Func::Re | Func::Im | Func::Arg | Func::Conj => {
            return Err(EngineError::new(format!("{} can't be differentiated here", f.latex())));
        }
        // erf′(u) = 2/√π·e^{−u²}
        Func::Erf => mul(vec![n(2), pow(pi(), q(-1, 2))?, pow(Sym::Const(Const::E), neg(pow_or_keep(u, 2)))?]),
        // (Φ⁻¹)′(u) = √(2π)·e^{Φ⁻¹(u)²/2}
        Func::Probit => mul(vec![
            pow(mul(vec![n(2), pi()]), q(1, 2))?,
            pow(Sym::Const(Const::E), mul(vec![q(1, 2), pow_or_keep(func(Func::Probit, u), 2)]))?,
        ]),
    })
}

fn pow_or_keep(b: Sym, k: i64) -> Sym {
    sym::pow_or_keep(b, n(k))
}

// ---------- Derivative working ----------

/// The derivative with the rules that produce it, outermost first.
pub fn diff_steps(s: &Sym, x: &str, angle: AngleMode) -> Result<(Sym, Vec<Step>), EngineError> {
    let mut steps = Vec::new();
    let d = explain(s, x, angle, &mut steps)?;
    let d = sym::normalize_complex(d);
    let last = format!("{}={}", dd(s, x), latex(&d));
    if steps.last().is_none_or(|st| st.latex != last) {
        steps.push(step(last, "Simplify"));
    }
    Ok((d, steps))
}

/// `d/dx [s]` as LaTeX.
pub fn dd(s: &Sym, x: &str) -> String {
    let inner = match s {
        Sym::Var(_) | Sym::Num(_) | Sym::Const(_) => latex(s),
        _ => format!("\\left({}\\right)", latex(s)),
    };
    format!("\\frac{{\\mathrm{{d}}}}{{\\mathrm{{d}}{x}}}{inner}")
}

/// Pieces that need no explaining: x itself and constants.
fn trivial(s: &Sym, x: &str) -> bool {
    free_of(s, x) || *s == var(x)
}

/// Differentiates while recording each rule; returns the derivative.
fn explain(s: &Sym, x: &str, angle: AngleMode, steps: &mut Vec<Step>) -> SR {
    if free_of(s, x) {
        return Ok(n(0));
    }
    let sub = |t: &Sym, steps: &mut Vec<Step>| -> SR { if trivial(t, x) { diff(t, x, angle) } else { explain(t, x, angle, steps) } };
    match s {
        Sym::Var(_) => Ok(n(1)),
        Sym::Add(t) => {
            let split = t.iter().filter(|t| !free_of(t, x)).map(|t| dd(t, x)).collect::<Vec<_>>().join("+");
            steps.push(step(format!("{}={split}", dd(s, x)), "Sum rule: differentiate term by term (constants give 0)"));
            let parts = t.iter().map(|t| sub(t, steps)).collect::<Result<Vec<_>, _>>()?;
            let r = add(parts);
            steps.push(step(format!("{}={}", dd(s, x), latex(&r)), "Add the pieces"));
            Ok(r)
        }
        Sym::Mul(f) => {
            let (consts, deps): (Vec<Sym>, Vec<Sym>) = f.iter().cloned().partition(|t| free_of(t, x));
            if !consts.is_empty() {
                let c = mul(consts);
                let rest = mul(deps);
                steps.push(step(format!("{}={}{}", dd(s, x), wrap_factor(&c), dd(&rest, x)), "Constant multiple rule"));
                let d = sub(&rest, steps)?;
                return Ok(mul(vec![c, d]));
            }
            // A quotient f/g when some factors have negative powers.
            let (bottom, top): (Vec<Sym>, Vec<Sym>) =
                deps.iter().cloned().partition(|t| matches!(t, Sym::Pow(_, e) if e.as_num().is_some_and(|k| k.is_negative())));
            if !bottom.is_empty() && !top.is_empty() {
                let num = mul(top);
                let den = mul(bottom.iter().map(|t| match t {
                    Sym::Pow(b, e) => sym::pow_or_keep((**b).clone(), neg((**e).clone())),
                    other => other.clone(),
                }).collect());
                steps.push(step(
                    format!(
                        "{}=\\frac{{{}\\cdot {}-{}\\cdot {}}}{{{}^{{2}}}}",
                        dd(s, x),
                        dd(&num, x),
                        wrap_factor(&den),
                        wrap_factor(&num),
                        dd(&den, x),
                        wrap_factor(&den)
                    ),
                    "Quotient rule: $\\left(\\frac{f}{g}\\right)' = \\frac{f'g-fg'}{g^2}$",
                ));
                let dn = sub(&num, steps)?;
                let dden = sub(&den, steps)?;
                let top = add(vec![mul(vec![dn, den.clone()]), neg(mul(vec![num, dden]))]);
                let top = crate::poly::expand(&top).unwrap_or(top);
                let r = div(top, pow_or_keep(den, 2))?;
                steps.push(step(format!("{}={}", dd(s, x), latex(&r)), "Put the pieces into the quotient rule"));
                return Ok(r);
            }
            let u = deps[0].clone();
            let v = mul(deps[1..].to_vec());
            steps.push(step(
                format!("{}={}\\cdot {}+{}\\cdot {}", dd(s, x), dd(&u, x), wrap_factor(&v), wrap_factor(&u), dd(&v, x)),
                "Product rule: $(uv)' = u'v+uv'$",
            ));
            let du = sub(&u, steps)?;
            let dv = sub(&v, steps)?;
            let r = add(vec![mul(vec![du, v]), mul(vec![u, dv])]);
            steps.push(step(format!("{}={}", dd(s, x), latex(&r)), "Put the pieces into the product rule"));
            Ok(r)
        }
        Sym::Pow(b, e) => {
            let (b, e) = (&**b, &**e);
            if free_of(e, x) {
                let lowered = sym::pow_or_keep(b.clone(), add(vec![e.clone(), n(-1)]));
                let outer = mul(vec![e.clone(), lowered]);
                if *b == var(x) {
                    steps.push(step(format!("{}={}", dd(s, x), latex(&outer)), format!("Power rule: $\\frac{{\\mathrm{{d}}}}{{\\mathrm{{d}}{x}}}{x}^n = n{x}^{{n-1}}$")));
                    return Ok(outer);
                }
                steps.push(step(
                    format!("{}={}\\cdot {}", dd(s, x), latex(&outer), dd(b, x)),
                    "Power rule with the chain rule: bring the power down, then multiply by the inside's derivative",
                ));
                let db = sub(b, steps)?;
                let r = mul(vec![outer, db]);
                steps.push(step(format!("{}={}", dd(s, x), latex(&r)), ""));
                return Ok(r);
            }
            if free_of(b, x) {
                let lnb = sym::ln(b.clone())?;
                let note = if *b == Sym::Const(Const::E) {
                    "Exponential rule: $(e^u)' = e^u\\cdot u'$".to_string()
                } else {
                    "Exponential rule: $(b^u)' = b^u\\ln b\\cdot u'$".to_string()
                };
                let outer = mul(vec![s.clone(), lnb]);
                if *e == var(x) {
                    steps.push(step(format!("{}={}", dd(s, x), latex(&outer)), note));
                    return Ok(outer);
                }
                steps.push(step(format!("{}={}\\cdot {}", dd(s, x), latex(&outer), dd(e, x)), note));
                let de = sub(e, steps)?;
                let r = mul(vec![outer, de]);
                steps.push(step(format!("{}={}", dd(s, x), latex(&r)), ""));
                return Ok(r);
            }
            // f^g with both depending on x: write it as e^(g ln f).
            let r = diff(s, x, angle)?;
            steps.push(step(
                format!("{}={}", dd(s, x), latex(&r)),
                format!("Write ${}$ as $e^{{{}\\ln {}}}$ and use the chain and product rules", latex(s), wrap_factor(e), wrap_factor(b)),
            ));
            Ok(r)
        }
        Sym::Func(f, u) => {
            let outer = outer_derivative(*f, u, angle)?;
            let name = f.latex().trim_start_matches('\\').trim_start_matches("operatorname{").trim_end_matches('}').to_string();
            if **u == var(x) {
                steps.push(step(format!("{}={}", dd(s, x), latex(&outer)), format!("Derivative of {name}")));
                return Ok(outer);
            }
            steps.push(step(
                format!("{}={}\\cdot {}", dd(s, x), wrap_factor(&outer), dd(u, x)),
                format!("Chain rule: the derivative of {name} at the inside, times the inside's derivative"),
            ));
            let du = sub(u, steps)?;
            let r = mul(vec![outer, du]);
            steps.push(step(format!("{}={}", dd(s, x), latex(&r)), ""));
            Ok(r)
        }
        _ => {
            let r = diff(s, x, angle)?;
            steps.push(step(format!("{}={}", dd(s, x), latex(&r)), ""));
            Ok(r)
        }
    }
}

// ---------- Antiderivatives ----------

const MAX_DEPTH: u32 = 8;

/// An antiderivative of `s` with respect to `x`, with the working that found it.
pub fn integrate_steps(s: &Sym, x: &str, angle: AngleMode) -> Option<(Sym, Vec<Step>)> {
    let mut steps = Vec::new();
    let big_f = antiderivative(s, x, angle, 0, &mut steps)?;
    Some((big_f, steps))
}

/// `∫ f dx` as LaTeX.
pub fn int_latex(f: &Sym, x: &str) -> String {
    let body = match f {
        Sym::Add(_) => format!("\\left({}\\right)", latex(f)),
        _ => latex(f),
    };
    format!("\\int {body}\\,\\mathrm{{d}}{x}")
}

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

/// (a, c) when u = a·x + c with a, c free of x and a ≠ 0.
fn linear(u: &Sym, x: &str) -> Option<(Sym, Sym)> {
    let a = diff(u, x, AngleMode::Radians).ok()?;
    if a.is_zero() || !free_of(&a, x) {
        return None;
    }
    let c = add(vec![u.clone(), neg(mul(vec![a.clone(), var(x)]))]);
    free_of(&c, x).then_some((a, c))
}

fn ln_abs(u: Sym) -> Sym {
    func(Func::Ln, func(Func::Abs, u))
}

/// ∫ s dx. Steps go to `tr` only along the path that succeeds.
fn antiderivative(s: &Sym, x: &str, angle: AngleMode, depth: u32, tr: &mut Vec<Step>) -> Option<Sym> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mark = tr.len();
    let found = antiderivative_inner(s, x, angle, depth, tr);
    if found.is_none() {
        tr.truncate(mark);
    }
    found
}

fn antiderivative_inner(s: &Sym, x: &str, angle: AngleMode, depth: u32, tr: &mut Vec<Step>) -> Option<Sym> {
    if free_of(s, x) {
        let r = mul(vec![s.clone(), var(x)]);
        tr.push(step(format!("{}={}", int_latex(s, x), latex(&r)), "The integral of a constant $c$ is $cx$"));
        return Some(r);
    }
    match s {
        Sym::Var(_) => {
            let r = mul(vec![q(1, 2), pow_or_keep(var(x), 2)]);
            tr.push(step(format!("{}={}", int_latex(s, x), latex(&r)), format!("Power rule: $\\int {x}^n = \\frac{{{x}^{{n+1}}}}{{n+1}}$")));
            Some(r)
        }
        Sym::Add(t) => {
            let split = t.iter().map(|t| int_latex(t, x)).collect::<Vec<_>>().join("+");
            tr.push(step(format!("{}={split}", int_latex(s, x)), "Integrate term by term"));
            let parts = t.iter().map(|t| antiderivative(t, x, angle, depth + 1, tr)).collect::<Option<Vec<_>>>()?;
            Some(add(parts))
        }
        Sym::Mul(f) => {
            let (consts, rest): (Vec<Sym>, Vec<Sym>) = f.iter().cloned().partition(|t| free_of(t, x));
            if !consts.is_empty() {
                let c = mul(consts);
                let r = mul(rest);
                tr.push(step(
                    format!("{}={}{}", int_latex(s, x), wrap_factor(&c), int_latex(&r, x)),
                    "Take the constant factor outside",
                ));
                return Some(mul(vec![c, antiderivative(&r, x, angle, depth + 1, tr)?]));
            }
            if let Some(r) = substitution(&rest, x, angle, depth, tr) {
                return Some(r);
            }
            if let Some(r) = by_parts(&rest, x, angle, depth, tr) {
                return Some(r);
            }
            expanded(s, x, angle, depth, tr)
        }
        Sym::Pow(b, e) => power(s, b, e, x, angle, depth, tr),
        Sym::Func(f, u) => {
            let (a, _) = linear(u, x)?;
            let base = table(*f, (**u).clone(), angle)?;
            let r = mul(vec![base, recip(a.clone()).ok()?]);
            let note = if **u == var(x) {
                format!("Standard integral of ${}$", latex(&func(*f, var("u"))).replace("\\left(u\\right)", ""))
            } else {
                format!("Standard integral, divided by ${}$ because the inside is ${}$", latex(&a), latex(u))
            };
            tr.push(step(format!("{}={}", int_latex(s, x), latex(&r)), note));
            Some(r)
        }
        Sym::Log(b, u) if free_of(b, x) => {
            // log_b u = ln u / ln b
            let ln_u = func(Func::Ln, (**u).clone());
            let lnb = sym::ln((**b).clone()).ok()?;
            tr.push(step(
                format!("{}=\\frac{{1}}{{{}}}{}", int_latex(s, x), latex(&lnb), int_latex(&ln_u, x)),
                "Change of base: $\\log_b u = \\frac{\\ln u}{\\ln b}$",
            ));
            Some(mul(vec![antiderivative(&ln_u, x, angle, depth + 1, tr)?, recip(lnb).ok()?]))
        }
        _ => None,
    }
}

fn wrap_factor(s: &Sym) -> String {
    match s {
        Sym::Add(_) => format!("\\left({}\\right)", latex(s)),
        _ => latex(s),
    }
}

/// ∫ f(u) du for a built-in function, u being the argument as written.
fn table(f: Func, u: Sym, angle: AngleMode) -> Option<Sym> {
    let k = trig_factor(angle);
    let one_over_k = recip(k.clone()).ok()?;
    // Inverse trig gives degrees in DEG mode: asin° = (180/π)·asin.
    let deg = match angle {
        AngleMode::Radians => n(1),
        AngleMode::Degrees => mul(vec![n(180), recip(pi()).ok()?]),
    };
    let sqrt = |s: Sym| pow(s, q(1, 2)).ok();
    let u2 = || pow_or_keep(u.clone(), 2);
    Some(match f {
        Func::Sin => neg(mul(vec![one_over_k, func(Func::Cos, u)])),
        Func::Cos => mul(vec![one_over_k, func(Func::Sin, u)]),
        Func::Tan => neg(mul(vec![one_over_k, ln_abs(func(Func::Cos, u))])),
        Func::Sinh => func(Func::Cosh, u),
        Func::Cosh => func(Func::Sinh, u),
        Func::Tanh => func(Func::Ln, func(Func::Cosh, u)),
        // ∫ ln u = u ln u − u
        Func::Ln => add(vec![mul(vec![u.clone(), func(Func::Ln, u.clone())]), neg(u)]),
        // ∫ asin u = u·asin u + √(1−u²)   (times 180/π in degree mode for the second part)
        Func::Asin => add(vec![
            mul(vec![u.clone(), func(Func::Asin, u.clone())]),
            mul(vec![deg, sqrt(add(vec![n(1), neg(u2())]))?]),
        ]),
        Func::Acos => add(vec![
            mul(vec![u.clone(), func(Func::Acos, u.clone())]),
            neg(mul(vec![deg, sqrt(add(vec![n(1), neg(u2())]))?])),
        ]),
        // ∫ atan u = u·atan u − ½ ln(1+u²)
        Func::Atan => add(vec![
            mul(vec![u.clone(), func(Func::Atan, u.clone())]),
            neg(mul(vec![deg, q(1, 2), func(Func::Ln, add(vec![u2(), n(1)]))])),
        ]),
        Func::Asinh => add(vec![mul(vec![u.clone(), func(Func::Asinh, u.clone())]), neg(sqrt(add(vec![u2(), n(1)]))?)]),
        Func::Atanh => add(vec![
            mul(vec![u.clone(), func(Func::Atanh, u.clone())]),
            mul(vec![q(1, 2), func(Func::Ln, add(vec![n(1), neg(u2())]))]),
        ]),
        // ∫ |u| = u|u|/2
        Func::Abs => mul(vec![q(1, 2), u.clone(), func(Func::Abs, u)]),
        Func::Acosh | Func::Floor | Func::Ceil | Func::Sqrt | Func::Exp | Func::Log | Func::Gamma | Func::Re | Func::Im | Func::Arg | Func::Conj | Func::Erf | Func::Probit => {
            return None;
        }
    })
}

/// ∫ bᵉ dx.
fn power(s: &Sym, b: &Sym, e: &Sym, x: &str, angle: AngleMode, depth: u32, tr: &mut Vec<Step>) -> Option<Sym> {
    let done = |tr: &mut Vec<Step>, r: Sym, note: String| {
        tr.push(step(format!("{}={}", int_latex(s, x), latex(&r)), note));
        Some(r)
    };
    if free_of(e, x) {
        if let Some((a, _)) = linear(b, x) {
            let inner = if *b == var(x) { String::new() } else { format!(", divided by ${}$ for the inside ${}$", latex(&a), latex(b)) };
            // ∫ uⁿ = uⁿ⁺¹/((n+1)a), ∫ u⁻¹ = ln|u|/a
            if *e == n(-1) {
                let r = mul(vec![ln_abs(b.clone()), recip(a).ok()?]);
                return done(tr, r, format!("$\\int \\frac{{1}}{{u}} = \\ln|u|${inner}"));
            }
            let e1 = add(vec![e.clone(), n(1)]);
            let r = mul(vec![pow(b.clone(), e1.clone()).ok()?, recip(mul(vec![e1, a])).ok()?]);
            return done(tr, r, format!("Power rule: $\\int u^n = \\frac{{u^{{n+1}}}}{{n+1}}${inner}"));
        }
        // ∫ 1/cos²u = tan u / a
        if let (Sym::Func(Func::Cos, u), Some(k)) = (b, e.as_num())
            && *k == Q::from_integer((-2).into())
            && let Some((a, _)) = linear(u, x)
        {
            let factor = mul(vec![a, trig_factor(angle)]);
            let r = mul(vec![func(Func::Tan, (**u).clone()), recip(factor).ok()?]);
            return done(tr, r, "Standard integral: $\\int \\frac{1}{\\cos^2 u} = \\tan u$".into());
        }
        if let Some((r, note)) = quadratic_form(b, e, x) {
            return done(tr, r, note);
        }
        // A polynomial to a positive integer power: multiply out.
        if e.as_num().is_some_and(|k| k.is_integer() && k.is_positive()) {
            return expanded(s, x, angle, depth, tr);
        }
        return None;
    }
    if free_of(b, x) {
        // ∫ b^(ax+c) = b^(ax+c) / (a ln b)
        let (a, _) = linear(e, x)?;
        let r = mul(vec![s.clone(), recip(mul(vec![a, sym::ln(b.clone()).ok()?])).ok()?]);
        let note = if *b == Sym::Const(Const::E) {
            "$\\int e^{u} = e^{u}$, divided by the coefficient of the exponent".to_string()
        } else {
            "$\\int b^{u} = \\frac{b^{u}}{\\ln b}$, divided by the coefficient of the exponent".to_string()
        };
        return done(tr, r, note);
    }
    None
}

/// 1/(px² + r) and 1/√(px² + r): arctan, ln, arcsin and arsinh forms, with the rule used.
fn quadratic_form(b: &Sym, e: &Sym, x: &str) -> Option<(Sym, String)> {
    let k = e.as_num()?;
    let Sym::Add(terms) = b else { return None };
    let mut p = None;
    let mut r = Q::zero();
    for t in terms {
        match t {
            Sym::Num(c) => r += c,
            _ => {
                let c = div(t.clone(), pow_or_keep(var(x), 2)).ok()?;
                p = Some(c.as_num()?.clone());
            }
        }
    }
    let p = p?;
    if r.is_zero() {
        return None;
    }
    let sqrt = |v: Q| pow(Sym::Num(v), q(1, 2)).ok();
    let xs = var(x);
    if *k == Q::from_integer((-1).into()) {
        if p.is_positive() == r.is_positive() {
            // ∫ 1/(px²+r) = atan(x√(p/r)) / √(pr)   (p, r same sign)
            let s = if p.is_positive() { 1 } else { -1 };
            let inner = mul(vec![xs, sqrt(&p / &r)?]);
            let res = mul(vec![n(s), func(Func::Atan, inner), recip(sqrt(&p * &r)?).ok()?]);
            return Some((res, "Standard form: $\\int \\frac{1}{x^2+a^2} = \\frac{1}{a}\\arctan\\frac{x}{a}$".into()));
        }
        // ∫ 1/(px² − |r|): partial fractions, ½√… · ln|(x−a)/(x+a)|
        let a = sqrt(-(&r / &p))?;
        let ratio = div(add(vec![xs.clone(), neg(a.clone())]), add(vec![xs, a.clone()])).ok()?;
        let res = mul(vec![recip(mul(vec![n(2), Sym::Num(p), a])).ok()?, ln_abs(ratio)]);
        return Some((res, "Partial fractions: $\\int \\frac{1}{x^2-a^2} = \\frac{1}{2a}\\ln\\left|\\frac{x-a}{x+a}\\right|$".into()));
    }
    if *k == Q::new((-1).into(), 2.into()) {
        if p.is_negative() && r.is_positive() {
            // ∫ 1/√(r − |p|x²) = asin(x√(|p|/r)) / √|p|
            let inner = mul(vec![xs, sqrt(-(&p / &r))?]);
            let res = mul(vec![func(Func::Asin, inner), recip(sqrt(-p)?).ok()?]);
            return Some((res, "Standard form: $\\int \\frac{1}{\\sqrt{a^2-x^2}} = \\arcsin\\frac{x}{a}$".into()));
        }
        if p.is_positive() {
            // ∫ 1/√(px² + r) = ln|x + √(x² + r/p)| / √p
            let root = pow(add(vec![pow_or_keep(xs.clone(), 2), Sym::Num(&r / &p)]), q(1, 2)).ok()?;
            let res = mul(vec![ln_abs(add(vec![xs, root])), recip(sqrt(p)?).ok()?]);
            return Some((res, "Standard form: $\\int \\frac{1}{\\sqrt{x^2+a}} = \\ln\\left|x+\\sqrt{x^2+a}\\right|$".into()));
        }
    }
    None
}

/// Replaces every occurrence of the subexpression `from` with `to`.
fn replace(s: &Sym, from: &Sym, to: &Sym) -> Sym {
    if s == from {
        return to.clone();
    }
    match s {
        Sym::Add(t) => add(t.iter().map(|t| replace(t, from, to)).collect()),
        Sym::Mul(f) => mul(f.iter().map(|t| replace(t, from, to)).collect()),
        Sym::Pow(b, e) => sym::pow_or_keep(replace(b, from, to), replace(e, from, to)),
        Sym::Func(f, a) => func(*f, replace(a, from, to)),
        Sym::Log(b, a) => Sym::Log(Box::new(replace(b, from, to)), Box::new(replace(a, from, to))),
        other => other.clone(),
    }
}

/// u-substitution: a factor g(u) times something that is a constant multiple of u′.
fn substitution(factors: &[Sym], x: &str, angle: AngleMode, depth: u32, tr: &mut Vec<Step>) -> Option<Sym> {
    let whole = mul(factors.to_vec());
    // The new variable's name: u, or w if u is taken.
    let t = if free_of(&whole, "u") { "u" } else if free_of(&whole, "w") { "w" } else { "#t" };
    for (k, g) in factors.iter().enumerate() {
        // Candidates for u: the factor itself, its argument, or its base/exponent.
        let mut candidates = vec![g.clone()];
        match g {
            Sym::Func(_, u) => candidates.push((**u).clone()),
            Sym::Pow(b, e) => {
                candidates.push((**b).clone());
                candidates.push((**e).clone());
            }
            _ => {}
        }
        for u in candidates {
            if free_of(&u, x) || u == var(x) {
                continue;
            }
            let others = mul(factors.iter().enumerate().filter(|(j, _)| *j != k).map(|(_, f)| f.clone()).collect());
            let du = diff(&u, x, angle).ok()?;
            if du.is_zero() {
                continue;
            }
            let Ok(c) = div(others, du.clone()) else { continue };
            if !free_of(&c, x) {
                continue;
            }
            let g_t = replace(g, &u, &var(t));
            if !free_of(&g_t, x) {
                continue;
            }
            let mark = tr.len();
            tr.push(step(
                format!("{t}={},\\quad \\mathrm{{d}}{t}={}\\,\\mathrm{{d}}{x}", latex(&u), latex(&du)),
                "Substitute: the rest of the integrand is a constant times $\\mathrm{d}u$",
            ));
            let in_t = mul(vec![c.clone(), g_t.clone()]);
            tr.push(step(format!("{}={}", int_latex(&whole, x), int_latex(&in_t, t)), format!("Rewrite in terms of ${t}$")));
            if let Some(big_g) = antiderivative(&g_t, t, angle, depth + 1, tr) {
                let in_t_result = mul(vec![c.clone(), big_g.clone()]);
                let back = mul(vec![c, replace(&big_g, &var(t), &u)]);
                tr.push(step(
                    format!("{}={}", latex(&in_t_result), latex(&back)),
                    format!("Put back ${t}={}$", latex(&u)),
                ));
                return Some(back);
            }
            tr.truncate(mark);
        }
    }
    None
}

/// A polynomial in x: sums and products of x, numbers and constants, integer powers ≥ 0.
fn is_polynomial(s: &Sym, x: &str) -> bool {
    match s {
        _ if free_of(s, x) => true,
        Sym::Var(_) => true,
        Sym::Add(v) | Sym::Mul(v) => v.iter().all(|t| is_polynomial(t, x)),
        Sym::Pow(b, e) => is_polynomial(b, x) && e.as_num().is_some_and(|k| k.is_integer() && !k.is_negative()),
        _ => false,
    }
}

/// Integration by parts for P(x)·g with P a polynomial.
fn by_parts(factors: &[Sym], x: &str, angle: AngleMode, depth: u32, tr: &mut Vec<Step>) -> Option<Sym> {
    let (poly, other): (Vec<Sym>, Vec<Sym>) = factors.iter().cloned().partition(|f| is_polynomial(f, x));
    if poly.is_empty() || other.len() != 1 {
        return None;
    }
    let p = mul(poly);
    let g = &other[0];
    let whole = mul(factors.to_vec());
    let dx = format!("\\,\\mathrm{{d}}{x}");
    let differentiates_to_poly = matches!(g, Sym::Func(Func::Ln | Func::Atan | Func::Asin | Func::Acos, _));
    if differentiates_to_poly {
        // ∫ P·L = Q·L − ∫ Q·L′ where Q = ∫ P
        let big_q = antiderivative(&p, x, angle, depth + 1, &mut Vec::new())?;
        let dl = diff(g, x, angle).ok()?;
        let rest = sym::normalize_complex(mul(vec![big_q.clone(), dl.clone()]));
        let rest = crate::poly::expand(&rest).unwrap_or(rest);
        tr.push(step(
            format!("u={},\\quad \\mathrm{{d}}v={}{dx}\\;\\Rightarrow\\;\\mathrm{{d}}u={}{dx},\\quad v={}", latex(g), wrap_factor(&p), latex(&dl), latex(&big_q)),
            "Integration by parts, $\\int u\\,\\mathrm{d}v = uv-\\int v\\,\\mathrm{d}u$, differentiating the logarithm or inverse function",
        ));
        tr.push(step(
            format!("{}={}-{}", int_latex(&whole, x), latex(&mul(vec![big_q.clone(), g.clone()])), int_latex(&rest, x)),
            "",
        ));
        let r = antiderivative(&rest, x, angle, depth + 1, tr)?;
        return Some(add(vec![mul(vec![big_q, g.clone()]), neg(r)]));
    }
    // ∫ P·g = P·G − ∫ P′·G, ending when P′ = 0 (g is exp, sin, cos, sinh, cosh of ax+b)
    let repeats = matches!(g, Sym::Pow(b, e) if free_of(b, x) && linear(e, x).is_some())
        || matches!(g, Sym::Func(Func::Sin | Func::Cos | Func::Sinh | Func::Cosh, u) if linear(u, x).is_some());
    if !repeats {
        return None;
    }
    let big_g = antiderivative(g, x, angle, depth + 1, &mut Vec::new())?;
    let dp = diff(&p, x, angle).ok()?;
    tr.push(step(
        format!("u={},\\quad \\mathrm{{d}}v={}{dx}\\;\\Rightarrow\\;\\mathrm{{d}}u={}{dx},\\quad v={}", latex(&p), latex(g), latex(&dp), latex(&big_g)),
        "Integration by parts, $\\int u\\,\\mathrm{d}v = uv-\\int v\\,\\mathrm{d}u$, differentiating the polynomial",
    ));
    let next = mul(vec![dp.clone(), big_g.clone()]);
    if dp.is_zero() {
        return Some(mul(vec![p, big_g]));
    }
    tr.push(step(format!("{}={}-{}", int_latex(&whole, x), latex(&mul(vec![p.clone(), big_g.clone()])), int_latex(&next, x)), ""));
    let tail = antiderivative(&next, x, angle, depth + 1, tr)?;
    Some(add(vec![mul(vec![p, big_g]), neg(tail)]))
}

/// Multiplies out a polynomial product and integrates term by term.
fn expanded(s: &Sym, x: &str, angle: AngleMode, depth: u32, tr: &mut Vec<Step>) -> Option<Sym> {
    let e = crate::poly::expand(s).ok()?;
    if e == *s {
        return None;
    }
    tr.push(step(format!("{}={}", int_latex(s, x), int_latex(&e, x)), "Multiply out first"));
    antiderivative(&e, x, angle, depth + 1, tr)
}

// ---------- Checking and numeric integration ----------

/// Points for checking an antiderivative, chosen to avoid special values.
const CHECK_POINTS: [(i64, i64); 5] = [(3183, 10000), (7071, 10000), (14142, 10000), (23, 10), (-6, 10)];

/// True when `big_f′ = f` holds numerically at the check points where both are defined.
/// Other free variables get fixed arbitrary values.
pub fn check_antiderivative(big_f: &Sym, f: &Sym, x: &str, ev: &mut Evaluator) -> bool {
    let Ok(d) = diff(big_f, x, ev.angle) else { return false };
    let residual = add(vec![d, neg(f.clone())]);
    // Letters from f and F too: the residual often simplifies to 0 and names none.
    let mut others = BTreeSet::new();
    free_vars(&residual, &mut others);
    free_vars(f, &mut others);
    free_vars(big_f, &mut others);
    others.remove(x);
    let saved = std::mem::take(&mut ev.vars);
    for (k, v) in others.iter().enumerate() {
        ev.vars.insert(v.clone(), Value::Exact(Q::new((137 + 61 * k as i64).into(), 100.into())));
    }
    let scale_expr = f.to_expr();
    let residual_expr = residual.to_expr();
    let mut checked = 0;
    let mut ok = true;
    for (num, den) in CHECK_POINTS {
        ev.vars.insert(x.to_string(), Value::Exact(Q::new(num.into(), den.into())));
        let (Ok(r), Ok(scale)) = (ev.eval(&residual_expr), ev.eval(&scale_expr)) else { continue };
        let (r, scale) = (magnitude(&r, ev), magnitude(&scale, ev));
        checked += 1;
        // |F′ − f| must be tiny next to |f|: rounding noise, not a different function.
        let p = ev.prec.bits;
        let tol = scale.add(&BigFloat::from_word(1, p), p, RM).mul(&BigFloat::from_f64(1e-25, p), p, RM);
        if r.cmp(&tol).is_none_or(|c| c > 0) {
            ok = false;
            break;
        }
    }
    ev.vars = saved;
    ok && checked > 0
}

fn magnitude(v: &Value, ev: &mut Evaluator) -> BigFloat {
    match v {
        Value::Complex(re, im) => crate::complex::abs(&crate::complex::C::new(re.clone(), im.clone()), &ev.prec),
        other => ev.prec.float_of(other).abs(),
    }
}

/// Points where an integrand is checked to be defined across [a, b] before an antiderivative
/// is trusted there (∫₋₁¹ 1/x² must not come out as −2).
const INTERIOR_SAMPLES: usize = 32;

/// Fails if the integrand is undefined somewhere it is sampled inside [a, b].
pub fn check_defined_between(f: &Sym, x: &str, a: &Value, b: &Value, ev: &mut Evaluator) -> Result<(), EngineError> {
    let p = ev.prec.bits;
    let (fa, fb) = (ev.prec.float_of(a), ev.prec.float_of(b));
    let width = fb.sub(&fa, p, RM);
    let body = f.to_expr();
    let saved = ev.vars.get(x).cloned();
    let mut result = Ok(());
    // The ends themselves may be singular (1/√x at 0): tanh-sinh never samples them, and an
    // antiderivative is taken there as a limit.
    for k in 1..INTERIOR_SAMPLES {
        let t = BigFloat::from_word(k as u64, p).div(&BigFloat::from_word(INTERIOR_SAMPLES as u64, p), p, RM);
        let point = fa.add(&width.mul(&t, p, RM), p, RM);
        // Exact sample points, so a pole at a simple value like 0 is hit exactly.
        let value = match (a, b) {
            (Value::Exact(qa), Value::Exact(qb)) => {
                Value::Exact(qa + (qb - qa) * Q::new((k as i64).into(), (INTERIOR_SAMPLES as i64).into()))
            }
            _ => Value::Real(point),
        };
        ev.vars.insert(x.to_string(), value);
        if let Err(e) = ev.eval(&body) {
            result = Err(EngineError::new(format!(
                "The integral doesn't converge or the integrand is undefined inside the interval ({})",
                e.message
            )));
            break;
        }
    }
    match saved {
        Some(v) => ev.vars.insert(x.to_string(), v),
        None => ev.vars.remove(x),
    };
    result
}

/// An infinite range of integration.
pub enum Range {
    /// [a, ∞)
    Above(Value),
    /// (−∞, b]
    Below(Value),
    /// (−∞, ∞)
    Whole,
}

/// ∫ₐᵇ f dx numerically by tanh-sinh quadrature, to the evaluator's precision.
pub fn quadrature(f: &crate::ast::Expr, x: &str, a: &Value, b: &Value, ev: &mut Evaluator) -> Result<Value, EngineError> {
    tanh_sinh(f, x, a, b, None, ev)
}

/// ∫ f dx over an infinite range: u ∈ [0, 1) maps to s = u/(1 − u) ∈ [0, ∞), with
/// ds = du/(1 − u)², and x = a + s, b − s, or ±s for the whole line.
pub fn quadrature_infinite(f: &crate::ast::Expr, x: &str, range: Range, ev: &mut Evaluator) -> Result<Value, EngineError> {
    let (zero, one) = (Value::Exact(Q::zero()), Value::Exact(Q::from_integer(1.into())));
    tanh_sinh(f, x, &zero, &one, Some(range), ev)
}

fn tanh_sinh(f: &crate::ast::Expr, x: &str, a: &Value, b: &Value, range: Option<Range>, ev: &mut Evaluator) -> Result<Value, EngineError> {
    let p = ev.prec.bits;
    let (fa, fb) = (ev.prec.float_of(a), ev.prec.float_of(b));
    if fa.is_nan() || fb.is_nan() {
        return Err(EngineError::new("Integral bounds must be real numbers"));
    }
    let shift = match &range {
        Some(Range::Above(v) | Range::Below(v)) => {
            let s = ev.prec.float_of(v);
            if s.is_nan() {
                return Err(EngineError::new("Integral bounds must be real numbers"));
            }
            s
        }
        _ => BigFloat::from_word(0, p),
    };
    let two = BigFloat::from_word(2, p);
    let half_width = fb.sub(&fa, p, RM).div(&two, p, RM);
    if half_width.is_zero() {
        return Ok(Value::Exact(Q::zero()));
    }
    let half_pi = ev.prec.cc.pi(p, RM).div(&two, p, RM);
    // Nodes beyond t_max have weights below the target accuracy, even against an integrand
    // that blows up at an end like (1 − u)^{−0.9} (1/√x at 0, or a slow tail mapped from ∞):
    // the weight falls like exp(−π/2·eᵗ) and the factor 10 leaves room for that growth.
    let digits = ev.prec.digits as f64;
    let t_max = (10.0 * (digits * std::f64::consts::LN_10 + 20.0) / std::f64::consts::PI).asinh();
    let eps = BigFloat::from_f64(10f64.powi(-(ev.prec.digits.min(300) as i32) - 3), p);
    let saved = ev.vars.get(x).cloned();

    let eval_at = |t: f64, ev: &mut Evaluator| -> Result<Option<BigFloat>, EngineError> {
        let tb = BigFloat::from_f64(t, p);
        let cc = &mut ev.prec.cc;
        let u = half_pi.mul(&tb.sinh(p, RM, cc), p, RM);
        let cosh_u = u.cosh(p, RM, cc);
        // Distance from the nearer endpoint, computed directly so it never rounds to 0:
        // 1 − tanh|u| = 2/(e^{2|u|} + 1).
        let e2u = u.abs().mul(&two, p, RM).exp(p, RM, cc);
        let gap = two.div(&e2u.add(&BigFloat::from_word(1, p), p, RM), p, RM);
        let offset = half_width.mul(&gap, p, RM);
        let point = if t >= 0.0 { fb.sub(&offset, p, RM) } else { fa.add(&offset, p, RM) };
        let weight = half_pi.mul(&tb.cosh(p, RM, cc), p, RM).div(&cosh_u.mul(&cosh_u, p, RM), p, RM);
        if gap.is_zero() || weight.is_zero() {
            return Ok(None);
        }
        let f_at = |at: BigFloat, ev: &mut Evaluator| -> Result<BigFloat, EngineError> {
            ev.vars.insert(x.to_string(), Value::Real(at));
            match ev.eval(f)? {
                Value::Complex(..) => Err(EngineError::new("Numeric integration needs a real integrand")),
                v => Ok(ev.prec.float_of(&v)),
            }
        };
        let Some(range) = &range else {
            return Ok(Some(f_at(point, ev)?.mul(&weight, p, RM)));
        };
        // u near 1 keeps 1 − u exact as the offset itself.
        let one = BigFloat::from_word(1, p);
        let (u, rest) = if t >= 0.0 { (one.sub(&offset, p, RM), offset.clone()) } else { (offset.clone(), one.sub(&offset, p, RM)) };
        if rest.is_zero() {
            return Ok(None);
        }
        let s = u.div(&rest, p, RM);
        let jacobian = one.div(&rest.mul(&rest, p, RM), p, RM);
        let fx = match range {
            Range::Above(_) => f_at(shift.add(&s, p, RM), ev)?,
            Range::Below(_) => f_at(shift.sub(&s, p, RM), ev)?,
            Range::Whole => f_at(s.clone(), ev)?.add(&f_at(s.neg(), ev)?, p, RM),
        };
        if fx.is_zero() {
            return Ok(Some(fx));
        }
        Ok(Some(fx.mul(&jacobian, p, RM).mul(&weight, p, RM)))
    };

    let result = (|| {
        let mut h = 0.5f64;
        let mut sum = eval_at(0.0, ev)?.unwrap_or_else(|| BigFloat::from_word(0, p));
        let mut k = 1;
        while (k as f64) * h <= t_max {
            for t in [k as f64 * h, -(k as f64) * h] {
                if let Some(v) = eval_at(t, ev)? {
                    sum = sum.add(&v, p, RM);
                }
            }
            k += 1;
        }
        let mut estimate = sum.mul(&BigFloat::from_f64(h, p), p, RM);
        for _level in 0..14 {
            h /= 2.0;
            // New nodes are the odd multiples of the halved step.
            let mut k = 1;
            while (k as f64) * h <= t_max {
                for t in [k as f64 * h, -(k as f64) * h] {
                    if let Some(v) = eval_at(t, ev)? {
                        sum = sum.add(&v, p, RM);
                    }
                }
                k += 2;
            }
            let next = sum.mul(&BigFloat::from_f64(h, p), p, RM);
            let change = next.sub(&estimate, p, RM).abs();
            let size = next.abs().add(&BigFloat::from_word(1, p), p, RM);
            estimate = next;
            if change.cmp(&size.mul(&eps, p, RM)).is_some_and(|c| c <= 0) {
                return Ok(Value::Real(estimate.mul(&half_width, p, RM)));
            }
        }
        Err(EngineError::new("The numeric integral didn't settle: it may diverge or need more precision"))
    })();
    match saved {
        Some(v) => ev.vars.insert(x.to_string(), v),
        None => ev.vars.remove(x),
    };
    result
}
