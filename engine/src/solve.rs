//! Solving equations and systems of equations, with the working for each method.
//!
//! One unknown: polynomials exactly (linear; quadratic by formula, completing the square or
//! factoring; higher degrees by factoring), rational equations by clearing denominators,
//! other equations by isolating the unknown (inverse functions, general trig solutions with
//! an integer k), and a numeric sign-change search when nothing exact applies. Every candidate
//! root is checked against the original equation, so extraneous roots (from squaring) drop out.
//!
//! Systems (2 to 10 equations): linear ones exactly by Gaussian elimination, Gauss–Jordan,
//! substitution, Cramer's rule and the inverse matrix, each with its own steps; nonlinear ones
//! by substitution when some equation is linear in one unknown.
//!
//! Step notes are plain text with `$…$` around inline maths.

// Row operations read one row while writing another, which index loops say most plainly.
#![allow(clippy::needless_range_loop)]

use std::collections::{BTreeMap, BTreeSet};

use astro_float::BigFloat;
use num_bigint::BigInt;
use num_rational::BigRational as Q;
use num_traits::{Signed, ToPrimitive, Zero};

use crate::ast::{Const, Func, Rel};
use crate::calculus::{free_of, free_vars};
use crate::latex::latex;
use crate::number::{RM, Value};
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow, q};
use crate::{AngleMode, EngineError, Method, Step};

pub enum Root {
    Exact(Sym),
    /// A root found numerically.
    Approx(Value),
}

/// A chain of comparisons with its parts built: `0 \le x \le 2\pi`.
pub struct SymChain {
    pub parts: Vec<Sym>,
    pub rels: Vec<Rel>,
}

impl SymChain {
    fn latex(&self) -> String {
        let mut out = latex(&self.parts[0]);
        for (r, p) in self.rels.iter().zip(&self.parts[1..]) {
            out.push_str(r.latex());
            out.push_str(&latex(p));
        }
        out
    }

    /// Each comparison on its own: (left, rel, right).
    fn pairs(&self) -> impl Iterator<Item = (&Sym, Rel, &Sym)> {
        self.rels.iter().enumerate().map(|(k, r)| (&self.parts[k], *r, &self.parts[k + 1]))
    }
}

pub enum Outcome {
    /// Already written out: an inequality's intervals, a region.
    Text { latex: String, text: String, decimal: Option<(String, String)> },
    /// An equation without unknowns that holds (2+2=4) or doesn't.
    True,
    False,
    NoSolution,
    /// Holds for every value (x+1 = 1+x).
    AllValues,
    /// Solution sets, each giving a value for every variable in `Solution::vars`.
    Sets(Vec<Vec<Root>>),
}

pub struct Solution {
    pub vars: Vec<String>,
    pub outcome: Outcome,
    /// Shown after the solutions as LaTeX: "k \in \mathbb{Z}", "z \text{ is free}".
    pub note: Option<String>,
    pub methods: Vec<Method>,
}

type R<T> = Result<T, EngineError>;

/// Largest polynomial degree solved through coefficients.
const MAX_DEGREE: i64 = 100;
/// Interval and sample count for the numeric root search.
const SEARCH_RANGE: f64 = 100.0;
const SEARCH_SAMPLES: usize = 4000;

// ---------- Small helpers ----------

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn method(name: &str, steps: Vec<Step>) -> Method {
    Method { name: name.to_string(), steps }
}

fn eq(l: &Sym, r: &Sym) -> String {
    format!("{}={}", latex(l), latex(r))
}

/// `$…$` around LaTeX for use inside a note.
fn m(s: &Sym) -> String {
    format!("${}$", latex(s))
}

fn expand(s: Sym) -> Sym {
    crate::poly::expand(&s).unwrap_or(s)
}

fn tidy(s: Sym) -> Sym {
    sym::normalize_complex(expand(s))
}

fn var(x: &str) -> Sym {
    Sym::Var(x.to_string())
}

fn terms(s: &Sym) -> Vec<Sym> {
    match s {
        Sym::Add(t) => t.clone(),
        z if z.is_zero() => Vec::new(),
        other => vec![other.clone()],
    }
}

fn factors(s: &Sym) -> Vec<Sym> {
    match s {
        Sym::Mul(f) => f.clone(),
        other => vec![other.clone()],
    }
}

fn is_zero(s: &Sym) -> bool {
    s.is_zero() || expand(s.clone()).is_zero()
}

fn sqrt(s: Sym) -> Sym {
    sym::pow_or_keep(s, q(1, 2))
}

fn d(a: Sym, b: Sym) -> R<Sym> {
    div(a, b)
}

/// A number we can compare with 0, if the expression is a plain rational.
fn rational(s: &Sym) -> Option<&Q> {
    s.as_num()
}

// ---------- Entry point ----------

/// Solves equations, keeping only the solutions that meet the conditions.
pub fn solve(b: &mut Builder, eqs: Vec<(Sym, Sym)>, conditions: Vec<SymChain>) -> R<Solution> {
    let mut sol = solve_equations(b, eqs)?;
    if !conditions.is_empty() {
        apply_conditions(b, &mut sol, &conditions)?;
    }
    Ok(sol)
}

/// Largest |k| tried when a general solution (+2πk) is cut down by conditions.
const MAX_K: i64 = 2000;
/// Most solutions listed after conditions pick them out.
const MAX_KEPT: usize = 60;

/// Whether every comparison holds once the variables have these values (None: can't tell).
fn conditions_hold(b: &mut Builder, conditions: &[SymChain], values: &[(String, Sym)]) -> Option<bool> {
    for c in conditions {
        for (l, rel, r) in c.pairs() {
            let mut diff = tidy(add(vec![l.clone(), neg(r.clone())]));
            for (v, x) in values {
                diff = b.subst(&diff, v, x).ok()?;
            }
            if diff.has_vars() {
                return None;
            }
            // Complex numbers have no order, so 2i > 0 is simply false.
            if matches!(b.numeric(&diff), Ok(Value::Complex(..))) {
                return Some(false);
            }
            let d = real_value(b, &diff)?;
            // Equal to well past the digits shown counts as equal, so ≤ keeps its endpoint.
            let d = if d.abs() < 1e-25 { 0.0 } else { d };
            if !rel.holds(d) {
                return Some(false);
            }
        }
    }
    Some(true)
}

/// The value as f64, if it is real.
fn real_value(b: &mut Builder, s: &Sym) -> Option<f64> {
    match b.numeric(s).ok()? {
        Value::Complex(..) => None,
        v => crate::number::format_value(&v, &mut b.ev.prec).and_then(|d| d.text.parse().ok()),
    }
}

fn apply_conditions(b: &mut Builder, sol: &mut Solution, conditions: &[SymChain]) -> R<()> {
    let Outcome::Sets(sets) = &mut sol.outcome else { return Ok(()) };
    let shown = conditions.iter().map(SymChain::latex).collect::<Vec<_>>().join(",\\ ");
    let general = sol.note.as_deref().is_some_and(|n| n.contains("\\mathbb{Z}"));
    let k_name = if general { sol.note.as_deref().and_then(|n| n.split('\\').next()).unwrap_or("k").to_string() } else { String::new() };
    let mut kept: Vec<Vec<Root>> = Vec::new();
    let mut keys: Vec<Vec<f64>> = Vec::new();
    for set in sets.drain(..) {
        let exact: Option<Vec<Sym>> = set.iter().map(|r| if let Root::Exact(s) = r { Some(s.clone()) } else { None }).collect();
        let Some(exact) = exact else {
            // Numeric roots: check their decimal values.
            let vals: Vec<(String, Sym)> = sol.vars.iter().zip(&set).map(|(v, r)| (v.clone(), root_sym(b, r))).collect();
            if conditions_hold(b, conditions, &vals) != Some(false) {
                kept.push(set);
            }
            continue;
        };
        let uses_k = general && exact.iter().any(|s| !free_of(s, &k_name));
        if !uses_k {
            let vals: Vec<(String, Sym)> = sol.vars.iter().cloned().zip(exact).collect();
            if conditions_hold(b, conditions, &vals) != Some(false) {
                kept.push(set);
            }
            continue;
        }
        // A general solution: try k = 0, ±1, ±2, … and keep the values that fit.
        let mut misses = 0;
        for step in 0..=(2 * MAX_K) {
            let k = if step % 2 == 0 { step / 2 } else { -(step + 1) / 2 };
            let at: Vec<Sym> = exact.iter().map(|s| b.subst(s, &k_name, &n(k)).map(tidy)).collect::<R<_>>()?;
            let vals: Vec<(String, Sym)> = sol.vars.iter().cloned().zip(at.iter().cloned()).collect();
            match conditions_hold(b, conditions, &vals) {
                Some(true) => {
                    misses = 0;
                    let key: Vec<f64> = at.iter().map(|s| real_value(b, s).unwrap_or(f64::NAN)).collect();
                    if !keys.iter().any(|k0| k0.iter().zip(&key).all(|(a, c)| (a - c).abs() < 1e-12)) {
                        keys.push(key);
                        kept.push(at.into_iter().map(Root::Exact).collect());
                    }
                }
                _ => misses += 1,
            }
            // Past the range on both sides: stop.
            if misses > 40 || kept.len() >= MAX_KEPT {
                break;
            }
        }
    }
    if general {
        sol.note = None;
    }
    let found = kept.len();
    *sets = kept;
    let listed = if found == 0 { "none of them do".to_string() } else { format!("{found} remain") };
    for m in &mut sol.methods {
        m.steps.push(step(shown.clone(), format!("Keep the solutions that satisfy the condition: {listed}")));
    }
    if sets.is_empty() {
        sol.outcome = Outcome::NoSolution;
    } else {
        order_roots(b, sol);
    }
    Ok(())
}

// ---------- Inequalities ----------

/// One letter: the intervals where the inequalities hold, from a sign chart.
/// Two or more letters: a region, which the graph shades.
pub fn inequalities(b: &mut Builder, chains: Vec<SymChain>) -> R<Solution> {
    let mut letters_set = BTreeSet::new();
    for c in &chains {
        for p in &c.parts {
            free_vars(p, &mut letters_set);
        }
    }
    let letters: Vec<String> = letters_set.into_iter().collect();
    let shown = chains.iter().map(SymChain::latex).collect::<Vec<_>>().join(",\\ ");
    match letters.as_slice() {
        [] => {
            let holds = conditions_hold(b, &chains, &[]).unwrap_or(false);
            Ok(Solution {
                vars: vec![],
                outcome: if holds { Outcome::True } else { Outcome::False },
                note: None,
                methods: vec![method("Comparing", vec![step(shown, if holds { "It holds" } else { "It doesn't hold" })])],
            })
        }
        [v] => sign_chart(b, &chains, v),
        _ => Ok(Solution {
            vars: letters.clone(),
            outcome: Outcome::Text {
                latex: "\\text{a region: turn on Graph to see it shaded}".into(),
                text: "a region (see the graph)".into(),
                decimal: None,
            },
            note: None,
            methods: vec![],
        }),
    }
}

/// A point where the sign can change: exact when possible.
struct Breakpoint {
    exact: Option<Sym>,
    value: f64,
}

fn sign_chart(b: &mut Builder, chains: &[SymChain], v: &str) -> R<Solution> {
    let mut steps = vec![step(chains.iter().map(SymChain::latex).collect::<Vec<_>>().join(",\\ "), "The inequality")];
    let comps: Vec<(Sym, Rel)> = chains.iter().flat_map(|c| c.pairs().map(|(l, r, rh)| (tidy(add(vec![l.clone(), neg(rh.clone())])), r)).collect::<Vec<_>>()).collect();
    for (f, r) in &comps {
        steps.push(step(format!("{}{}0", latex(f), r.latex()), "Move everything to one side"));
    }
    // Where each side can change sign: its zeros, and where it is undefined (denominators).
    let mut points: Vec<Breakpoint> = Vec::new();
    for (f, _) in &comps {
        let mut zero_sets: Vec<Sym> = vec![f.clone()];
        if let Some((_, den)) = clear_denominators(f, v) {
            zero_sets.push(den);
        }
        // Where a root or logarithm stops being defined (√x at 0) the sign can change too.
        domain_edges(f, v, &mut zero_sets);
        for g in zero_sets {
            let roots = match solve_one(b, &g, &n(0), &g, v) {
                Ok(Solution { outcome: Outcome::Sets(sets), .. }) => sets.into_iter().map(|mut s| s.remove(0)).collect::<Vec<_>>(),
                Ok(_) => Vec::new(),
                Err(_) => sign_change_roots(b, &g, v, -SEARCH_RANGE, SEARCH_RANGE)?.into_iter().map(Root::Approx).collect(),
            };
            for r in roots {
                let (exact, value) = match &r {
                    Root::Exact(s) if !s.has_vars() => (Some(s.clone()), real_value(b, s)),
                    Root::Approx(x) => (None, to_f64(b, x)),
                    _ => continue,
                };
                if let Some(value) = value
                    && !points.iter().any(|p| (p.value - value).abs() < 1e-12)
                {
                    points.push(Breakpoint { exact, value });
                }
            }
        }
    }
    points.sort_by(|a, c| a.value.total_cmp(&c.value));
    let shown_points = points.iter().map(|p| point_latex(b, p)).collect::<Vec<_>>().join(",\\ ");
    steps.push(step(
        if points.is_empty() { "\\text{no sign changes}".into() } else { format!("{v}={shown_points}") },
        "Where a side is zero or undefined: the only places its sign can change",
    ));

    // Test each open interval at a point inside it, and each breakpoint itself.
    let holds_at = |b: &mut Builder, x: Sym| -> bool {
        comps.iter().all(|(f, r)| {
            let Ok(at) = b.subst(f, v, &x) else { return false };
            match real_value(b, &at) {
                Some(d) => r.holds(if d.abs() < 1e-25 { 0.0 } else { d }),
                None => false,
            }
        })
    };
    let float_sym = |x: f64| -> Sym {
        let r = num_rational::BigRational::from_float(x).unwrap_or_else(Q::zero);
        Sym::Num(r)
    };
    let mut segments: Vec<bool> = Vec::new();
    let mut tests: Vec<String> = Vec::new();
    for k in 0..=points.len() {
        let x = match (k.checked_sub(1).map(|j| points[j].value), points.get(k).map(|p| p.value)) {
            (None, None) => 0.0,
            (None, Some(hi)) => hi - 1.0,
            (Some(lo), None) => lo + 1.0,
            (Some(lo), Some(hi)) => (lo + hi) / 2.0,
        };
        let ok = holds_at(b, float_sym(x));
        segments.push(ok);
        tests.push(format!("{v}={}:\\ {}", trim_float(x), if ok { "\\checkmark" } else { "\\times" }));
        if let Some(p) = points.get(k) {
            let at = p.exact.clone().unwrap_or_else(|| float_sym(p.value));
            segments.push(holds_at(b, at));
        }
    }
    steps.push(step(tests.join(",\\quad "), "Test a point in each interval"));

    // Merge into intervals: segments alternate open interval, point, open interval…
    let mut pieces: Vec<(Option<usize>, bool, Option<usize>, bool)> = Vec::new(); // (lo point, lo closed, hi point, hi closed)
    let mut start: Option<(Option<usize>, bool)> = None;
    for (idx, &ok) in segments.iter().enumerate() {
        let is_point = idx % 2 == 1;
        let point = idx / 2;
        match (ok, start) {
            (true, None) => start = Some(if is_point { (Some(point), true) } else { (if idx == 0 { None } else { Some(point - 1) }, false) }),
            (false, Some((lo, lo_closed))) => {
                let hi = if is_point { (Some(point), false) } else { (Some(point - 1), true) };
                pieces.push((lo, lo_closed, hi.0, hi.1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some((lo, lo_closed)) = start {
        pieces.push((lo, lo_closed, None, false));
    }
    let bound = |b: &mut Builder, i: usize| point_latex(b, &points[i]);
    let bound_text = |i: usize| trim_float(points[i].value);
    let mut parts_latex = Vec::new();
    let mut parts_text = Vec::new();
    for (lo, lo_c, hi, hi_c) in &pieces {
        let (l, t) = match (lo, hi) {
            (None, None) => (format!("\\text{{every real }}{v}"), format!("every real {v}")),
            (None, Some(h)) => (format!("{v}{}{}", if *hi_c { "\\le " } else { "<" }, bound(b, *h)), format!("{v} {} {}", if *hi_c { "≤" } else { "<" }, bound_text(*h))),
            (Some(l), None) => (format!("{v}{}{}", if *lo_c { "\\ge " } else { ">" }, bound(b, *l)), format!("{v} {} {}", if *lo_c { "≥" } else { ">" }, bound_text(*l))),
            (Some(l), Some(h)) if l == h => (format!("{v}={}", bound(b, *l)), format!("{v} = {}", bound_text(*l))),
            (Some(l), Some(h)) => (
                format!("{}{}{v}{}{}", bound(b, *l), if *lo_c { "\\le " } else { "<" }, if *hi_c { "\\le " } else { "<" }, bound(b, *h)),
                format!("{} {} {v} {} {}", bound_text(*l), if *lo_c { "≤" } else { "<" }, if *hi_c { "≤" } else { "<" }, bound_text(*h)),
            ),
        };
        parts_latex.push(l);
        parts_text.push(t);
    }
    let outcome = if pieces.is_empty() {
        Outcome::NoSolution
    } else {
        let latex = parts_latex.join("\\quad\\text{or}\\quad ");
        let text = parts_text.join(" or ");
        steps.push(step(latex.clone(), "Join the intervals where it holds (endpoints included where ≤ or ≥ hold there)"));
        let exact_points = points.iter().all(|p| p.exact.as_ref().is_some_and(|e| e.as_num().is_some()));
        let decimal = (!exact_points).then(|| (text.replace("≤", "\\le ").replace("≥", "\\ge ").replace(" or ", "\\quad\\text{or}\\quad "), text.clone()));
        Outcome::Text { latex, text, decimal }
    };
    Ok(Solution { vars: vec![v.into()], outcome, note: None, methods: vec![method("Sign chart", steps)] })
}

/// Expressions whose zeros are edges of the real domain: what's under an even root or a
/// fractional power, and inside a logarithm.
fn domain_edges(s: &Sym, v: &str, out: &mut Vec<Sym>) {
    if free_of(s, v) {
        return;
    }
    match s {
        Sym::Pow(base, e) => {
            if e.as_num().is_some_and(|k| !k.is_integer()) && !free_of(base, v) {
                out.push((**base).clone());
            }
            domain_edges(base, v, out);
            domain_edges(e, v, out);
        }
        Sym::Func(f, a) => {
            if matches!(f, Func::Ln | Func::Log | Func::Asin | Func::Acos | Func::Acosh | Func::Atanh) {
                out.push((**a).clone());
                if matches!(f, Func::Asin | Func::Acos | Func::Atanh) {
                    // The edges are where the argument is ±1.
                    out.push(add(vec![(**a).clone(), n(-1)]));
                    out.push(add(vec![(**a).clone(), n(1)]));
                }
            }
            domain_edges(a, v, out);
        }
        Sym::Log(base, a) => {
            out.push((**a).clone());
            domain_edges(base, v, out);
            domain_edges(a, v, out);
        }
        Sym::Add(t) | Sym::Mul(t) => t.iter().for_each(|t| domain_edges(t, v, out)),
        _ => {}
    }
}

fn to_f64(b: &mut Builder, v: &Value) -> Option<f64> {
    crate::number::format_value(v, &mut b.ev.prec).and_then(|d| d.text.parse().ok())
}

fn point_latex(b: &mut Builder, p: &Breakpoint) -> String {
    match &p.exact {
        Some(s) => latex(s),
        None => decimal_latex(b, &Value::Real(BigFloat::from_f64(p.value, b.ev.prec.bits))),
    }
}

fn trim_float(x: f64) -> String {
    let s = format!("{:.6}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn solve_equations(b: &mut Builder, eqs: Vec<(Sym, Sym)>) -> R<Solution> {
    for (l, r) in &eqs {
        if crate::units::contains_unit(l) || crate::units::contains_unit(r) {
            return Err(EngineError::new("Equations with units aren't supported yet"));
        }
    }
    let fs: Vec<Sym> = eqs.iter().map(|(l, r)| tidy(add(vec![l.clone(), neg(r.clone())]))).collect();
    let mut unknown_set = BTreeSet::new();
    for f in &fs {
        free_vars(f, &mut unknown_set);
    }
    let mut unknowns: Vec<String> = unknown_set.into_iter().collect();
    if eqs.len() > 1 && unknowns.len() > eqs.len() {
        unknowns = choose_unknowns(unknowns, eqs.len());
    }
    if unknowns.is_empty() {
        let mut mentioned = BTreeSet::new();
        for (l, r) in &eqs {
            free_vars(l, &mut mentioned);
            free_vars(r, &mut mentioned);
        }
        let holds = fs.iter().all(|f| numerically_zero(b, f));
        let steps = eqs.iter().zip(&fs).map(|((l, r), f)| step(eq(l, r), format!("Both sides differ by {}", m(f)))).collect();
        let outcome = match (mentioned.is_empty(), holds) {
            (true, true) => Outcome::True,
            (true, false) => Outcome::False,
            (false, true) => Outcome::AllValues,
            (false, false) => Outcome::NoSolution,
        };
        return Ok(Solution {
            vars: mentioned.into_iter().collect(),
            outcome,
            note: None,
            methods: vec![method("Simplifying", steps)],
        });
    }
    if eqs.len() == 1 {
        let v = choose_var(&eqs[0], &unknowns);
        return single(b, &eqs[0].0, &eqs[0].1, &fs[0], &v);
    }
    if unknowns.len() > crate::parser::MAX_EQUATIONS {
        return Err(EngineError::new(format!(
            "Systems can have up to {} unknowns for now",
            crate::parser::MAX_EQUATIONS
        )));
    }
    if let Some(sys) = linear_form(&fs, &unknowns) {
        return linear(b, &sys, &unknowns);
    }
    nonlinear(b, &eqs, &fs, &unknowns)
}

/// l = r solved for the letter `v`, with every other letter a parameter.
pub fn solve_for(b: &mut Builder, l: &Sym, r: &Sym, v: &str) -> R<Solution> {
    let f = tidy(add(vec![l.clone(), neg(r.clone())]));
    single(b, l, r, &f, v)
}

/// l = r solved for `v`, keeping the solutions that satisfy the conditions (this picks the
/// k of a periodic family that land in a range).
pub fn solve_for_within(b: &mut Builder, l: &Sym, r: &Sym, v: &str, conditions: &[SymChain]) -> R<Solution> {
    let mut sol = solve_for(b, l, r, v)?;
    if !conditions.is_empty() {
        apply_conditions(b, &mut sol, conditions)?;
    }
    Ok(sol)
}

/// A system solved for the given unknowns (the other letters are parameters).
pub fn solve_system_for(b: &mut Builder, eqs: Vec<(Sym, Sym)>, unknowns: &[String]) -> R<Solution> {
    if eqs.len() == 1 && unknowns.len() == 1 {
        return solve_for(b, &eqs[0].0, &eqs[0].1, &unknowns[0]);
    }
    let fs: Vec<Sym> = eqs.iter().map(|(l, r)| tidy(add(vec![l.clone(), neg(r.clone())]))).collect();
    match linear_form(&fs, unknowns) {
        Some(sys) => linear(b, &sys, unknowns),
        None => nonlinear(b, &eqs, &fs, unknowns),
    }
}

/// Which letter to solve for in one equation: the one standing alone on a side (y = 2x+1),
/// else x, else the first alphabetically.
fn choose_var((l, r): &(Sym, Sym), unknowns: &[String]) -> String {
    for (side, other) in [(l, r), (r, l)] {
        if let Sym::Var(v) = side
            && unknowns.contains(v)
            && free_of(other, v)
        {
            return v.clone();
        }
    }
    if unknowns.iter().any(|u| u == "x") { "x".into() } else { unknowns[0].clone() }
}

/// With more letters than equations, the textbook unknowns (x, y, z, w, then later letters
/// first, subscripted ones with their letter) and the rest are parameters: in
/// ax + y = 1, x + ay = 1 the unknowns are x and y.
fn choose_unknowns(mut letters: Vec<String>, count: usize) -> Vec<String> {
    let rank = |v: &str| -> (usize, String) {
        let base = v.trim_start_matches('\\').chars().next().unwrap_or('a');
        let r = match base {
            'x' => 0,
            'y' => 1,
            'z' => 2,
            'w' => 3,
            c if c.is_ascii_lowercase() => 4 + (b'z' - c as u8) as usize,
            _ => 40,
        };
        (r, v.to_string())
    };
    letters.sort_by_key(|v| rank(v));
    letters.truncate(count);
    letters.sort();
    letters
}

/// True when a variable-free expression is 0, exactly or to well past the digits shown.
fn numerically_zero(b: &mut Builder, f: &Sym) -> bool {
    if f.is_zero() {
        return true;
    }
    match b.numeric(f) {
        Ok(Value::Exact(r)) => r.is_zero(),
        Ok(v) => {
            let mag = magnitude(b, &v);
            let tol = BigFloat::from_f64(10f64.powi(-(b.ev.prec.digits.min(300) as i32) - 5), b.ev.prec.bits);
            mag.cmp(&tol).is_some_and(|c| c <= 0)
        }
        Err(_) => false,
    }
}

fn magnitude(b: &mut Builder, v: &Value) -> BigFloat {
    match v {
        Value::Complex(re, im) => crate::complex::abs(&crate::complex::C::new(re.clone(), im.clone()), &b.ev.prec),
        other => b.ev.prec.float_of(other).abs(),
    }
}

// ---------- One unknown ----------

/// Coefficients c₀…cₙ when `f` is a polynomial in `v` (cₙ ≠ 0).
fn poly_coeffs(f: &Sym, v: &str) -> Option<Vec<Sym>> {
    let mut c: Vec<Vec<Sym>> = Vec::new();
    for t in terms(&expand(f.clone())) {
        let mut k: i64 = 0;
        let mut coeff = Vec::new();
        for fac in factors(&t) {
            match &fac {
                Sym::Var(x) if x == v => k += 1,
                Sym::Pow(base, e) if matches!(&**base, Sym::Var(x) if x == v) => {
                    let e = e.as_num().filter(|e| e.is_integer() && !e.is_negative())?.to_integer().to_i64()?;
                    k += e;
                }
                other if free_of(other, v) => coeff.push(other.clone()),
                _ => return None,
            }
        }
        if k > MAX_DEGREE {
            return None;
        }
        while c.len() <= k as usize {
            c.push(Vec::new());
        }
        c[k as usize].push(mul(coeff));
    }
    let mut out: Vec<Sym> = c.into_iter().map(|ts| tidy(add(ts))).collect();
    while out.len() > 1 && out.last().is_some_and(is_zero) {
        out.pop();
    }
    if out.is_empty() {
        out.push(n(0));
    }
    Some(out)
}

/// c₀ + c₁v + … as a symbolic expression.
fn poly_sym(c: &[Sym], v: &str) -> Sym {
    add(c.iter().enumerate().map(|(k, ck)| mul(vec![ck.clone(), sym::pow_or_keep(var(v), n(k as i64))])).collect())
}

/// Most numeric roots listed; the ones nearest 0 are kept.
const MAX_LISTED_ROOTS: usize = 10;

fn single(b: &mut Builder, l: &Sym, r: &Sym, f: &Sym, v: &str) -> R<Solution> {
    let mut sol = solve_one(b, l, r, f, v)?;
    order_roots(b, &mut sol);
    Ok(sol)
}

/// Real roots in increasing order; past `MAX_LISTED_ROOTS`, only those nearest 0.
fn order_roots(b: &mut Builder, sol: &mut Solution) {
    let Outcome::Sets(sets) = &mut sol.outcome else { return };
    if sol.vars.len() != 1 {
        return;
    }
    let mut keyed: Vec<(Option<f64>, Vec<Root>)> = sets
        .drain(..)
        .map(|set| {
            let key = match &set[0] {
                Root::Exact(s) if !s.has_vars() => b.numeric(s).ok(),
                Root::Approx(x) => Some(x.clone()),
                _ => None,
            }
            .and_then(|v| match v {
                Value::Complex(..) => None,
                // The decimal text ("1.414…", "1e40") parses as f64, plenty for ordering.
                other => crate::number::format_value(&other, &mut b.ev.prec).and_then(|d| d.text.parse().ok()),
            });
            (key, set)
        })
        .collect();
    if keyed.iter().all(|(k, _)| k.is_some()) {
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if keyed.len() > MAX_LISTED_ROOTS {
            let total = keyed.len();
            keyed.sort_by(|a, b| a.0.map(f64::abs).partial_cmp(&b.0.map(f64::abs)).unwrap_or(std::cmp::Ordering::Equal));
            keyed.truncate(MAX_LISTED_ROOTS);
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            sol.note = Some(format!("\\text{{the {MAX_LISTED_ROOTS} of {total} roots nearest 0}}"));
        }
    }
    *sets = keyed.into_iter().map(|(_, s)| s).collect();
}

fn solve_one(b: &mut Builder, l: &Sym, r: &Sym, f: &Sym, v: &str) -> R<Solution> {
    let given = step(eq(l, r), "The equation");
    if let Some(c) = poly_coeffs(f, v) {
        // aₙxⁿ + a₀ = 0 for n ≥ 3 has an exact root by isolating: x³ = 2 gives ∛2.
        let two_terms = c.len() > 3 && c.iter().filter(|x| !is_zero(x)).count() == 2 && !is_zero(&c[0]);
        if two_terms {
            let lhs = mul(vec![c[c.len() - 1].clone(), sym::pow_or_keep(var(v), n(c.len() as i64 - 1))]);
            let rhs = tidy(neg(c[0].clone()));
            let mut steps = vec![given.clone(), step(eq(&lhs, &rhs), "Rearrange")];
            let mut uses_k = false;
            if let Some(roots) = isolate(b, lhs, rhs.clone(), v, &mut steps, "k", &mut uses_k, 0)? {
                let mut roots = check_roots(b, f, v, dedupe(roots), &mut steps);
                let power = c.len() as i64 - 1;
                let z = tidy(d(rhs.clone(), c[c.len() - 1].clone())?);
                if roots.is_empty() && !z.has_vars() {
                    // No real root (x⁴ = −16): all the complex ones, like x² = −4 gives ±2i.
                    roots = b.nth_roots(&z, power)?;
                    steps.push(step(
                        roots.iter().map(|r| format!("{v}={}", latex(r))).collect::<Vec<_>>().join(",\\quad "),
                        format!("No real root, so the {power} complex ones: $\\sqrt[{power}]{{r}}$ at angles $\\frac{{\\theta+2\\pi k}}{{{power}}}$ (De Moivre)"),
                    ));
                } else {
                    steps.push(step("", "Only the real roots are listed"));
                }
                let outcome = if roots.is_empty() { Outcome::NoSolution } else { Outcome::Sets(roots.into_iter().map(|r| vec![Root::Exact(r)]).collect()) };
                return Ok(Solution { vars: vec![v.into()], outcome, note: None, methods: vec![method(&format!("Isolating {v}"), steps)] });
            }
        }
        return polynomial(b, vec![given], &c, v, f);
    }
    if let Some((g, den)) = clear_denominators(f, v)
        && let Some(c) = poly_coeffs(&g, v)
    {
        let intro = vec![
            given.clone(),
            step(format!("{}=0", latex(&g)), format!("Multiply both sides by {} to clear the fractions", m(&den))),
        ];
        let mut sol = polynomial(b, intro, &c, v, f)?;
        // Roots that make a denominator zero aren't solutions.
        if let Outcome::Sets(sets) = &mut sol.outcome {
            let mut excluded = Vec::new();
            let mut kept = Vec::new();
            for set in sets.drain(..) {
                let bad = match &set[0] {
                    Root::Exact(x) => {
                        let at = b.subst(&den, v, x)?;
                        is_zero(&at)
                    }
                    Root::Approx(_) => false,
                };
                if bad {
                    if let Root::Exact(x) = &set[0] {
                        excluded.push(x.clone());
                    }
                } else {
                    kept.push(set);
                }
            }
            *sets = kept;
            for x in excluded {
                for meth in &mut sol.methods {
                    meth.steps.push(step(
                        format!("{v}\\ne {}", latex(&x)),
                        format!("${v}={}$ makes a denominator zero, so it is not a solution", latex(&x)),
                    ));
                }
            }
            if sets.is_empty() {
                sol.outcome = Outcome::NoSolution;
            }
        }
        return Ok(sol);
    }
    if let Some(sol) = radical(b, &given, l, r, f, v)? {
        return Ok(sol);
    }
    let (il, ir) = if free_of(l, v) && !free_of(r, v) {
        (r.clone(), l.clone())
    } else if !free_of(r, v) {
        (f.clone(), n(0))
    } else {
        (l.clone(), r.clone())
    };
    let mut steps = vec![given.clone()];
    if il != *l {
        steps.push(step(eq(&il, &ir), "Rearrange"));
    }
    let k_name = if free_of(f, "k") { "k" } else { "n" };
    let mut uses_k = false;
    if let Some(roots) = isolate(b, il, ir, v, &mut steps, k_name, &mut uses_k, 0)? {
        let roots = check_roots(b, f, v, dedupe(roots), &mut steps);
        let outcome = if roots.is_empty() { Outcome::NoSolution } else { Outcome::Sets(roots.into_iter().map(|r| vec![Root::Exact(r)]).collect()) };
        let note = uses_k.then(|| format!("{k_name}\\in\\mathbb{{Z}}"));
        return Ok(Solution { vars: vec![v.into()], outcome, note, methods: vec![method(&format!("Isolating {v}"), steps)] });
    }
    numeric_solution(b, given, f, v, -SEARCH_RANGE, SEARCH_RANGE)
}

/// √u = w with the unknown on both sides: square to u = w², solve the polynomial, and drop
/// the roots that squaring added.
fn radical(b: &mut Builder, given: &Step, l: &Sym, r: &Sym, f: &Sym, v: &str) -> R<Option<Solution>> {
    let is_sqrt = |s: &Sym| matches!(s, Sym::Pow(_, e) if **e == q(1, 2));
    let (root, other) = if is_sqrt(l) && !free_of(r, v) {
        (l, r)
    } else if is_sqrt(r) && !free_of(l, v) {
        (r, l)
    } else {
        return Ok(None);
    };
    let Sym::Pow(u, _) = root else { return Ok(None) };
    let squared = tidy(sym::pow_or_keep(other.clone(), n(2)));
    let g = tidy(add(vec![(**u).clone(), neg(squared.clone())]));
    let Some(c) = poly_coeffs(&g, v) else { return Ok(None) };
    let intro = vec![given.clone(), step(eq(u, &squared), "Square both sides (roots are checked at the end)")];
    let mut sol = polynomial(b, intro, &c, v, &g)?;
    if let Outcome::Sets(sets) = &mut sol.outcome {
        let mut kept = Vec::new();
        for set in sets.drain(..) {
            let ok = match &set[0] {
                Root::Exact(x) => satisfies(b, f, v, x),
                Root::Approx(_) => true,
            };
            if ok {
                kept.push(set);
            } else if let Root::Exact(x) = &set[0] {
                for meth in &mut sol.methods {
                    meth.steps.push(step(
                        format!("{v}\\ne {}", latex(x)),
                        format!("Checking ${v}={}$ in the original equation fails: squaring added it", latex(x)),
                    ));
                }
            }
        }
        if kept.is_empty() {
            sol.outcome = Outcome::NoSolution;
        } else {
            *sets = kept;
        }
    }
    Ok(Some(sol))
}

fn dedupe(xs: Vec<Sym>) -> Vec<Sym> {
    let mut out: Vec<Sym> = Vec::new();
    for x in xs {
        let x = tidy(x);
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

/// Keeps the roots that satisfy f = 0 (checked numerically when they have no letters).
fn check_roots(b: &mut Builder, f: &Sym, v: &str, roots: Vec<Sym>, steps: &mut Vec<Step>) -> Vec<Sym> {
    let mut kept = Vec::new();
    for r in roots {
        if satisfies(b, f, v, &r) {
            kept.push(r);
        } else {
            steps.push(step(
                format!("{v}\\ne {}", latex(&r)),
                format!("Checking ${v}={}$ in the original equation fails, so it is extraneous", latex(&r)),
            ));
        }
    }
    kept
}

/// Numerically, f(root) ≈ 0. Roots with letters (parameters, k) pass unchecked.
fn satisfies(b: &mut Builder, f: &Sym, v: &str, root: &Sym) -> bool {
    if root.has_vars() {
        return true;
    }
    let Ok(at) = b.subst(f, v, root) else { return false };
    let scale: Sym = add(terms(f).into_iter().map(|t| sym::Sym::Func(Func::Abs, Box::new(t))).collect());
    let Ok(scale) = b.subst(&scale, v, root) else { return false };
    let (Ok(val), Ok(sc)) = (b.numeric(&at), b.numeric(&scale)) else { return false };
    let (val, sc) = (magnitude(b, &val), magnitude(b, &sc));
    let p = b.ev.prec.bits;
    let tol = sc.add(&BigFloat::from_word(1, p), p, RM).mul(&BigFloat::from_f64(1e-20, p), p, RM);
    val.cmp(&tol).is_some_and(|c| c <= 0)
}

/// Multiplies a rational equation through by its denominators: f·D, and D.
fn clear_denominators(f: &Sym, v: &str) -> Option<(Sym, Sym)> {
    let mut dens: BTreeMap<Sym, i64> = BTreeMap::new();
    for t in terms(f) {
        for fac in factors(&t) {
            if let Sym::Pow(base, e) = &fac
                && !free_of(base, v)
                && let Some(k) = e.as_num().filter(|k| k.is_integer() && k.is_negative())
            {
                let k = (-k).to_integer().to_i64()?;
                let entry = dens.entry((**base).clone()).or_insert(0);
                *entry = (*entry).max(k);
            }
        }
    }
    if dens.is_empty() {
        return None;
    }
    let den = mul(dens.into_iter().map(|(base, k)| sym::pow_or_keep(base, n(k))).collect());
    let g = tidy(mul(vec![f.clone(), den.clone()]));
    Some((g, den))
}

fn polynomial(b: &mut Builder, intro: Vec<Step>, c: &[Sym], v: &str, f: &Sym) -> R<Solution> {
    let deg = c.len() - 1;
    let vars = vec![v.to_string()];
    let p = poly_sym(c, v);
    let mut base = intro;
    let standard = format!("{}=0", latex(&p));
    if base.last().is_none_or(|s| s.latex != standard) {
        base.push(step(standard, "Bring everything to one side"));
    }
    let exact_sets = |roots: Vec<Sym>| Outcome::Sets(roots.into_iter().map(|r| vec![Root::Exact(r)]).collect());
    match deg {
        0 => {
            let all = is_zero(&c[0]);
            let note = if all { "Everything cancels: true for every value" } else { "Everything with the unknown cancels, leaving a false statement" };
            base.push(step(format!("{}=0", latex(&c[0])), note));
            let outcome = if all { Outcome::AllValues } else { Outcome::NoSolution };
            Ok(Solution { vars, outcome, note: None, methods: vec![method("Simplifying", base)] })
        }
        1 => {
            let x = tidy(neg(d(c[0].clone(), c[1].clone())?));
            let mut steps = base;
            let lhs = mul(vec![c[1].clone(), var(v)]);
            let moved = tidy(neg(c[0].clone()));
            steps.push(step(eq(&lhs, &moved), format!("Move {} to the other side", m(&c[0]))));
            let mut note = format!("Divide both sides by {}", m(&c[1]));
            if c[1].has_vars() {
                note.push_str(&format!(" (assuming {} ≠ 0)", m(&c[1])));
            }
            steps.push(step(eq(&var(v), &x), note));
            Ok(Solution { vars, outcome: exact_sets(vec![x]), note: None, methods: vec![method(&format!("Isolating {v}"), steps)] })
        }
        2 => quadratic(base, &c[0], &c[1], &c[2], v),
        _ => higher(b, base, &p, v, f),
    }
}

/// The two roots of ax² + bx + c, smaller-sign first, and the discriminant.
fn quadratic_roots(a: &Sym, bb: &Sym, cc: &Sym) -> R<(Vec<Sym>, Sym)> {
    let disc = tidy(add(vec![sym::pow_or_keep(bb.clone(), n(2)), neg(mul(vec![n(4), a.clone(), cc.clone()]))]));
    let two_a = mul(vec![n(2), a.clone()]);
    if is_zero(&disc) {
        return Ok((vec![tidy(d(neg(bb.clone()), two_a)?)], disc));
    }
    let s = sqrt(disc.clone());
    let r1 = tidy(d(add(vec![neg(bb.clone()), neg(s.clone())]), two_a.clone())?);
    let r2 = tidy(d(add(vec![neg(bb.clone()), s]), two_a)?);
    Ok((vec![r1, r2], disc))
}

fn quadratic(base: Vec<Step>, cc: &Sym, bb: &Sym, a: &Sym, v: &str) -> R<Solution> {
    let (roots, disc) = quadratic_roots(a, bb, cc)?;
    let vl = v.to_string();
    let roots_latex = roots.iter().map(|r| format!("{vl}={}", latex(r))).collect::<Vec<_>>().join("\\quad\\text{or}\\quad ");
    let disc_note = match rational(&disc) {
        Some(dq) if dq.is_negative() => "The discriminant is negative: two complex roots",
        _ if roots.len() == 1 => "The discriminant is 0: one double root",
        Some(_) => "The discriminant is positive: two real roots",
        None => "The discriminant",
    };

    // Quadratic formula
    let mut formula = base.clone();
    formula.push(step(
        format!("a={},\\quad b={},\\quad c={}", latex(a), latex(bb), latex(cc)),
        format!("Read off the coefficients of $a{vl}^2+b{vl}+c=0$"),
    ));
    formula.push(step(format!("D=b^{{2}}-4ac={}", latex(&disc)), disc_note));
    formula.push(step(
        format!(
            "{vl}=\\frac{{-b\\pm\\sqrt{{D}}}}{{2a}}=\\frac{{{}\\pm\\sqrt{{{}}}}}{{{}}}",
            latex(&tidy(neg(bb.clone()))),
            latex(&disc),
            latex(&mul(vec![n(2), a.clone()]))
        ),
        "The quadratic formula",
    ));
    formula.push(step(roots_latex.clone(), "Simplify"));

    // Completing the square: v² + pv + s = 0, (v + p/2)² = p²/4 − s
    let mut square = base.clone();
    let p_ = tidy(d(bb.clone(), a.clone())?);
    let s_ = tidy(d(cc.clone(), a.clone())?);
    let half = tidy(mul(vec![q(1, 2), p_.clone()]));
    let rhs = tidy(add(vec![sym::pow_or_keep(half.clone(), n(2)), neg(s_.clone())]));
    let shifted = add(vec![var(v), half.clone()]);
    if !is_zero(&add(vec![a.clone(), n(-1)])) {
        square.push(step(
            format!("{}=0", latex(&add(vec![sym::pow_or_keep(var(v), n(2)), mul(vec![p_.clone(), var(v)]), s_.clone()]))),
            format!("Divide by {}", m(a)),
        ));
    }
    square.push(step(
        format!("{}^{{2}}={}", wrap(&shifted), latex(&rhs)),
        format!("Add $\\left({}\\right)^2$ to both sides to complete the square", latex(&half)),
    ));
    if roots.len() == 2 {
        square.push(step(format!("{}=\\pm{}", latex(&shifted), latex(&sqrt(rhs.clone()))), "Take the square root of both sides"));
    }
    let undo = match half.as_num() {
        Some(h) if h.is_negative() => format!("Add {}", m(&Sym::Num(-h))),
        _ => format!("Subtract {}", m(&half)),
    };
    square.push(step(roots_latex.clone(), undo));

    let mut methods = vec![method("Quadratic formula", formula), method("Completing the square", square)];

    // Factoring, when the roots are rational.
    if roots.iter().all(|r| rational(r).is_some()) {
        let mut fac = base;
        let factored = mul(std::iter::once(a.clone()).chain(roots.iter().map(|r| add(vec![var(v), neg(r.clone())]))).collect());
        let factored = if roots.len() == 1 { mul(vec![a.clone(), sym::pow_or_keep(add(vec![var(v), neg(roots[0].clone())]), n(2))]) } else { factored };
        fac.push(step(format!("{}=0", latex(&factored)), "Factor"));
        let each = roots
            .iter()
            .map(|r| format!("{}=0", latex(&add(vec![var(v), neg(r.clone())]))))
            .collect::<Vec<_>>()
            .join("\\quad\\text{or}\\quad ");
        fac.push(step(each, "A product is zero when one of its factors is"));
        fac.push(step(roots_latex, "Solve each"));
        methods.push(method("Factoring", fac));
    }
    let outcome = Outcome::Sets(roots.into_iter().map(|r| vec![Root::Exact(r)]).collect());
    Ok(Solution { vars: vec![v.into()], outcome, note: None, methods })
}

fn wrap(s: &Sym) -> String {
    match s {
        Sym::Add(_) => format!("\\left({}\\right)", latex(s)),
        _ => latex(s),
    }
}

/// Degree 3 and up: factor, then solve each factor (numerically past degree 2).
fn higher(b: &mut Builder, mut steps: Vec<Step>, p: &Sym, v: &str, f: &Sym) -> R<Solution> {
    let fact = crate::poly::factor(p).unwrap_or_else(|_| p.clone());
    if fact != *p {
        steps.push(step(format!("{}=0", latex(&fact)), "Factor"));
        steps.push(step("", "A product is zero when one of its factors is"));
    }
    let pieces: Vec<Sym> = factors(&fact)
        .into_iter()
        .map(|fac| match fac {
            Sym::Pow(base, e) if e.as_num().is_some_and(|k| k.is_integer() && k.is_positive()) => *base,
            other => other,
        })
        .filter(|fac| !free_of(fac, v))
        .collect();
    let mut roots: Vec<Root> = Vec::new();
    let mut exact: Vec<Sym> = Vec::new();
    let mut approx_note = false;
    for piece in pieces {
        let Some(c) = poly_coeffs(&piece, v) else { continue };
        match c.len() - 1 {
            1 => {
                let x = tidy(neg(d(c[0].clone(), c[1].clone())?));
                steps.push(step(format!("{}=0\\;\\Rightarrow\\;{v}={}", latex(&piece), latex(&x)), ""));
                exact.push(x);
            }
            2 => {
                let (rs, _) = quadratic_roots(&c[2], &c[1], &c[0])?;
                let shown = rs.iter().map(|r| format!("{v}={}", latex(r))).collect::<Vec<_>>().join(",\\ ");
                steps.push(step(format!("{}=0\\;\\Rightarrow\\;{shown}", latex(&piece)), "Quadratic formula"));
                exact.extend(rs);
            }
            _ => {
                let bound = cauchy_bound(&c);
                let found = sign_change_roots(b, &piece, v, -bound, bound)?;
                approx_note = true;
                let shown = found.iter().map(|r| decimal_latex(b, r)).collect::<Vec<_>>().join(",\\ ");
                steps.push(step(
                    format!("{}=0\\;\\Rightarrow\\;{v}\\approx {shown}", latex(&piece)),
                    "No exact formula: real roots found numerically (complex ones aren't listed)",
                ));
                roots.extend(found.into_iter().map(Root::Approx));
            }
        }
    }
    let exact = dedupe(exact);
    let exact = check_roots(b, f, v, exact, &mut steps);
    let mut all: Vec<Root> = exact.into_iter().map(Root::Exact).collect();
    all.extend(roots);
    let name = if approx_note { "Factoring and numeric roots" } else { "Factoring" };
    let outcome = if all.is_empty() { Outcome::NoSolution } else { Outcome::Sets(all.into_iter().map(|r| vec![r]).collect()) };
    Ok(Solution { vars: vec![v.into()], outcome, note: None, methods: vec![method(name, steps)] })
}

/// All real roots of a polynomial lie within 1 + max|cₖ/cₙ|.
fn cauchy_bound(c: &[Sym]) -> f64 {
    let lead = c.last().and_then(|x| x.as_num()).and_then(|x| x.to_f64());
    let Some(lead) = lead.filter(|l| *l != 0.0) else { return 1e3 };
    let biggest = c[..c.len() - 1]
        .iter()
        .map(|x| x.as_num().and_then(|v| v.to_f64()).map(|v| (v / lead).abs()).unwrap_or(1e3))
        .fold(0.0, f64::max);
    (1.0 + biggest).min(1e6)
}

fn decimal_latex(b: &mut Builder, v: &Value) -> String {
    crate::number::format_value(v, &mut b.ev.prec).map(|d| d.latex).unwrap_or_else(|| "?".into())
}

/// Isolates `v` in l = r by undoing one operation at a time. `None` when v isn't in a single
/// place; `Some(vec![])` when there is provably no real solution.
#[allow(clippy::too_many_arguments)]
fn isolate(
    b: &mut Builder,
    l: Sym,
    r: Sym,
    v: &str,
    steps: &mut Vec<Step>,
    k: &str,
    uses_k: &mut bool,
    depth: u32,
) -> R<Option<Vec<Sym>>> {
    if depth > 24 {
        return Ok(None);
    }
    let mut next: Vec<(Sym, Sym)> = Vec::new();
    match &l {
        Sym::Var(x) if x == v => return Ok(Some(vec![r])),
        Sym::Add(ts) => {
            let (with, without): (Vec<Sym>, Vec<Sym>) = ts.iter().cloned().partition(|t| !free_of(t, v));
            let [inner] = with.as_slice() else { return Ok(None) };
            let moved = add(without);
            let r2 = tidy(add(vec![r, neg(moved.clone())]));
            steps.push(step(eq(inner, &r2), format!("Subtract {} from both sides", m(&moved))));
            next.push((inner.clone(), r2));
        }
        Sym::Mul(fs) => {
            let (with, without): (Vec<Sym>, Vec<Sym>) = fs.iter().cloned().partition(|t| !free_of(t, v));
            let [inner] = with.as_slice() else { return Ok(None) };
            let c = mul(without);
            let r2 = tidy(d(r, c.clone())?);
            steps.push(step(eq(inner, &r2), format!("Divide both sides by {}", m(&c))));
            next.push((inner.clone(), r2));
        }
        Sym::Pow(base, e) if free_of(e, v) => {
            let base = (**base).clone();
            let ek = e.as_num().cloned();
            match ek {
                Some(k1) if k1 == Q::from_integer((-1).into()) => {
                    if is_zero(&r) {
                        steps.push(step(eq(&l, &r), "A reciprocal is never 0"));
                        return Ok(Some(vec![]));
                    }
                    let r2 = tidy(d(n(1), r)?);
                    steps.push(step(eq(&base, &r2), "Take the reciprocal of both sides"));
                    next.push((base, r2));
                }
                Some(kk) if kk.is_integer() && kk.is_positive() => {
                    let even = kk.to_integer() % BigInt::from(2) == BigInt::zero();
                    if even && rational(&r).is_some_and(|x| x.is_negative()) {
                        steps.push(step(eq(&l, &r), "An even power is never negative: no real solution"));
                        return Ok(Some(vec![]));
                    }
                    let root = tidy(sym::pow_or_keep(r, Sym::Num(kk.recip())));
                    if even {
                        steps.push(step(format!("{}=\\pm {}", latex(&base), latex(&root)), "Take the root of both sides (±)"));
                        next.push((base.clone(), root.clone()));
                        next.push((base, tidy(neg(root))));
                    } else {
                        steps.push(step(eq(&base, &root), "Take the root of both sides"));
                        next.push((base, root));
                    }
                }
                Some(kk) if kk.is_positive() && kk.numer() == &BigInt::from(1) => {
                    if rational(&r).is_some_and(|x| x.is_negative()) {
                        steps.push(step(eq(&l, &r), "A root is never negative: no real solution"));
                        return Ok(Some(vec![]));
                    }
                    let r2 = tidy(sym::pow_or_keep(r, Sym::Num(kk.recip())));
                    steps.push(step(eq(&base, &r2), "Raise both sides to undo the root (roots are checked at the end)"));
                    next.push((base, r2));
                }
                _ => {
                    let r2 = tidy(sym::pow_or_keep(r, d(n(1), (**e).clone())?));
                    steps.push(step(eq(&base, &r2), format!("Raise both sides to the power $1/{}$", latex(e))));
                    next.push((base, r2));
                }
            }
        }
        Sym::Pow(base, e) if free_of(base, v) => {
            if rational(&r).is_some_and(|x| !x.is_positive()) {
                steps.push(step(eq(&l, &r), "A positive base to any power is positive: no real solution"));
                return Ok(Some(vec![]));
            }
            let (r2, note) = if **base == Sym::Const(Const::E) {
                (sym::ln(r)?, "Take ln of both sides".to_string())
            } else {
                (sym::log_base((**base).clone(), r)?, format!("Take $\\log_{{{}}}$ of both sides", latex(base)))
            };
            let r2 = tidy(r2);
            steps.push(step(eq(e, &r2), note));
            next.push(((**e).clone(), r2));
        }
        Sym::Log(base, u) if free_of(base, v) => {
            let r2 = tidy(pow((**base).clone(), r)?);
            steps.push(step(eq(u, &r2), format!("Raise {} to both sides", m(base))));
            next.push(((**u).clone(), r2));
        }
        Sym::Func(f, u) => {
            let u = (**u).clone();
            let inv = |b: &mut Builder, g: Func, x: Sym| b.func(g, x);
            match f {
                Func::Ln => {
                    let r2 = tidy(pow(Sym::Const(Const::E), r)?);
                    steps.push(step(eq(&u, &r2), "Raise $e$ to both sides"));
                    next.push((u, r2));
                }
                Func::Abs => {
                    if rational(&r).is_some_and(|x| x.is_negative()) {
                        steps.push(step(eq(&l, &r), "An absolute value is never negative: no solution"));
                        return Ok(Some(vec![]));
                    }
                    steps.push(step(format!("{}=\\pm {}", latex(&u), latex(&r)), "$|u|=c$ means $u=c$ or $u=-c$"));
                    next.push((u.clone(), r.clone()));
                    next.push((u, tidy(neg(r))));
                }
                Func::Sin | Func::Cos | Func::Tan => {
                    if *f != Func::Tan && rational(&r).is_some_and(|x| x.abs() > Q::from_integer(1.into())) {
                        steps.push(step(eq(&l, &r), "sin and cos stay between −1 and 1: no real solution"));
                        return Ok(Some(vec![]));
                    }
                    let (period, half) = match b.ev.angle {
                        AngleMode::Radians => (mul(vec![n(2), Sym::Const(Const::Pi)]), Sym::Const(Const::Pi)),
                        AngleMode::Degrees => (n(360), n(180)),
                    };
                    let kv = var(k);
                    *uses_k = true;
                    let branches: Vec<Sym> = match f {
                        Func::Sin => {
                            let a = inv(b, Func::Asin, r.clone())?;
                            vec![
                                add(vec![a.clone(), mul(vec![period.clone(), kv.clone()])]),
                                add(vec![half.clone(), neg(a), mul(vec![period, kv])]),
                            ]
                        }
                        Func::Cos => {
                            let a = inv(b, Func::Acos, r.clone())?;
                            vec![
                                add(vec![a.clone(), mul(vec![period.clone(), kv.clone()])]),
                                add(vec![neg(a), mul(vec![period, kv])]),
                            ]
                        }
                        _ => {
                            let a = inv(b, Func::Atan, r.clone())?;
                            vec![add(vec![a, mul(vec![half, kv])])]
                        }
                    };
                    let branches = dedupe(branches);
                    let shown = branches.iter().map(|x| format!("{}={}", latex(&u), latex(x))).collect::<Vec<_>>().join("\\quad\\text{or}\\quad ");
                    steps.push(step(shown, format!("Every solution of this trig equation, with ${k}$ any integer")));
                    next.extend(branches.into_iter().map(|x| (u.clone(), x)));
                }
                _ => {
                    let g = match f {
                        Func::Asin => Func::Sin,
                        Func::Acos => Func::Cos,
                        Func::Atan => Func::Tan,
                        Func::Sinh => Func::Asinh,
                        Func::Tanh => Func::Atanh,
                        Func::Asinh => Func::Sinh,
                        Func::Acosh => Func::Cosh,
                        Func::Atanh => Func::Tanh,
                        Func::Cosh => {
                            let a = tidy(inv(b, Func::Acosh, r)?);
                            steps.push(step(format!("{}=\\pm {}", latex(&u), latex(&a)), "Apply arcosh (±)"));
                            next.push((u.clone(), a.clone()));
                            next.push((u, tidy(neg(a))));
                            return finish(b, next, v, steps, k, uses_k, depth);
                        }
                        _ => return Ok(None),
                    };
                    let r2 = tidy(inv(b, g, r)?);
                    steps.push(step(eq(&u, &r2), "Apply the inverse function to both sides"));
                    next.push((u, r2));
                }
            }
        }
        _ => return Ok(None),
    }
    finish(b, next, v, steps, k, uses_k, depth)
}

fn finish(
    b: &mut Builder,
    next: Vec<(Sym, Sym)>,
    v: &str,
    steps: &mut Vec<Step>,
    k: &str,
    uses_k: &mut bool,
    depth: u32,
) -> R<Option<Vec<Sym>>> {
    let mut out = Vec::new();
    for (l, r) in next {
        match isolate(b, l, r, v, steps, k, uses_k, depth + 1)? {
            Some(rs) => out.extend(rs),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

fn numeric_solution(b: &mut Builder, given: Step, f: &Sym, v: &str, lo: f64, hi: f64) -> R<Solution> {
    let mut others = BTreeSet::new();
    free_vars(f, &mut others);
    others.remove(v);
    if !others.is_empty() {
        return Err(EngineError::new(format!("I can't solve this for {v} exactly, and the other letters stop a numeric search")));
    }
    let roots = sign_change_roots(b, f, v, lo, hi)?;
    let shown = roots.iter().map(|r| decimal_latex(b, r)).collect::<Vec<_>>().join(",\\ ");
    let steps = vec![
        given,
        step(format!("{}=0", latex(f)), "Bring everything to one side"),
        step(
            if roots.is_empty() { "\\text{no sign changes}".into() } else { format!("{v}\\approx {shown}") },
            format!(
                "No exact method applies: search $[{lo}, {hi}]$ for sign changes and narrow each one down (roots where the graph only touches zero can be missed)"
            ),
        ),
    ];
    let outcome = if roots.is_empty() { Outcome::NoSolution } else { Outcome::Sets(roots.into_iter().map(|r| vec![Root::Approx(r)]).collect()) };
    Ok(Solution { vars: vec![v.into()], outcome, note: None, methods: vec![method("Numeric search", steps)] })
}

/// Real roots of f in [lo, hi]: sample for sign changes, then bisect each to full precision.
fn sign_change_roots(b: &mut Builder, f: &Sym, v: &str, lo: f64, hi: f64) -> R<Vec<Value>> {
    let expr = f.to_expr();
    let p = b.ev.prec.bits;
    let saved = std::mem::take(&mut b.ev.vars);
    let eval = |b: &mut Builder, x: &BigFloat| -> Option<BigFloat> {
        b.ev.vars.insert(v.to_string(), Value::Real(x.clone()));
        match b.ev.eval(&expr).ok()? {
            Value::Complex(..) => None,
            other => Some(b.ev.prec.float_of(&other)),
        }
    };
    let mut roots: Vec<BigFloat> = Vec::new();
    let step_size = (hi - lo) / SEARCH_SAMPLES as f64;
    let mut prev: Option<(BigFloat, BigFloat)> = None;
    for k in 0..=SEARCH_SAMPLES {
        let x = BigFloat::from_f64(lo + step_size * k as f64, p);
        let Some(fx) = eval(b, &x) else {
            prev = None;
            continue;
        };
        if fx.is_zero() {
            roots.push(x.clone());
        } else if let Some((px, pf)) = &prev
            && !pf.is_zero()
            && pf.is_negative() != fx.is_negative()
        {
            // Bisect [px, x].
            let (mut a, mut c) = (px.clone(), x.clone());
            let a_neg = pf.is_negative();
            let two = BigFloat::from_word(2, p);
            let mut ok = true;
            for _ in 0..(p + 20) {
                let mid = a.add(&c, p, RM).div(&two, p, RM);
                let Some(fm) = eval(b, &mid) else {
                    ok = false;
                    break;
                };
                if fm.is_zero() {
                    a = mid.clone();
                    c = mid;
                    break;
                }
                if fm.is_negative() == a_neg {
                    a = mid;
                } else {
                    c = mid;
                }
            }
            // A sign change across a pole (1/x at 0) isn't a root: the value there is huge.
            if ok {
                let mid = a.add(&c, p, RM).div(&two, p, RM);
                if let Some(fm) = eval(b, &mid)
                    && fm.abs().cmp(&BigFloat::from_f64(1e-10, p)).is_some_and(|o| o < 0)
                {
                    roots.push(mid);
                }
            }
        }
        prev = Some((x, fx));
    }
    b.ev.vars = saved;
    Ok(roots.into_iter().map(Value::Real).collect())
}

// ---------- Linear systems ----------

struct Linear {
    /// Coefficients: one row per equation, one column per unknown.
    a: Vec<Vec<Sym>>,
    rhs: Vec<Sym>,
}

/// The system as A·x = b, if every equation is linear in the unknowns.
fn linear_form(fs: &[Sym], unknowns: &[String]) -> Option<Linear> {
    let mut a = Vec::new();
    let mut rhs = Vec::new();
    for f in fs {
        let mut row = vec![n(0); unknowns.len()];
        let mut constant = Vec::new();
        for t in terms(f) {
            let mut idx = None;
            let mut coeff = Vec::new();
            for fac in factors(&t) {
                match &fac {
                    Sym::Var(x) if unknowns.contains(x) => {
                        if idx.is_some() {
                            return None;
                        }
                        idx = unknowns.iter().position(|u| u == x);
                    }
                    other if unknowns.iter().all(|u| free_of(other, u)) => coeff.push(other.clone()),
                    _ => return None,
                }
            }
            match idx {
                Some(j) => row[j] = add(vec![row[j].clone(), mul(coeff)]),
                None => constant.push(mul(coeff)),
            }
        }
        a.push(row.into_iter().map(tidy).collect());
        rhs.push(tidy(neg(add(constant))));
    }
    Some(Linear { a, rhs })
}

fn row_eq(row: &[Sym], rhs: &Sym, unknowns: &[String]) -> String {
    let lhs = add(row.iter().zip(unknowns).map(|(c, u)| mul(vec![c.clone(), var(u)])).collect());
    eq(&lhs, rhs)
}

fn system_latex(a: &[Vec<Sym>], rhs: &[Sym], unknowns: &[String]) -> String {
    let rows: Vec<String> = a.iter().zip(rhs).map(|(row, r)| row_eq(row, r, unknowns)).collect();
    format!("\\begin{{cases}}{}\\end{{cases}}", rows.join("\\\\"))
}

/// A matrix in brackets; with `bar`, a line before the last column (augmented matrix).
fn matrix_latex(rows: &[Vec<Sym>], bar: bool, delims: (&str, &str)) -> String {
    let cols = rows.first().map_or(0, Vec::len);
    let spec = if bar && cols > 0 { format!("{}|c", "c".repeat(cols - 1)) } else { "c".repeat(cols) };
    let body: Vec<String> = rows.iter().map(|r| r.iter().map(latex).collect::<Vec<_>>().join("&")).collect();
    format!("\\left{}\\begin{{array}}{{{spec}}}{}\\end{{array}}\\right{}", delims.0, body.join("\\\\"), delims.1)
}

fn bracket(rows: &[Vec<Sym>], bar: bool) -> String {
    matrix_latex(rows, bar, ("[", "]"))
}

fn bars(rows: &[Vec<Sym>]) -> String {
    matrix_latex(rows, false, ("|", "|"))
}

/// "R_2 → R_2 − 3R_1" as LaTeX.
fn row_op(target: usize, factor: &Sym, source: usize) -> String {
    let (neg_f, mag) = match factor.as_num() {
        Some(x) if x.is_negative() => (true, Sym::Num(-x)),
        _ => (false, factor.clone()),
    };
    let sign = if neg_f { "+" } else { "-" };
    let coef = if mag.is_one() { String::new() } else { wrap(&mag) };
    format!("R_{{{t}}}\\to R_{{{t}}}{sign}{coef}R_{{{s}}}", t = target + 1, s = source + 1)
}

enum LinResult {
    /// Values for every unknown, free ones being themselves.
    Solved(Vec<Sym>, Vec<usize>),
    Inconsistent,
}

fn pick_pivot(m: &[Vec<Sym>], from: usize, col: usize) -> Option<usize> {
    let candidates: Vec<usize> = (from..m.len()).filter(|&i| !is_zero(&m[i][col])).collect();
    // A plain ±1 keeps the numbers small, then any number, then anything non-zero.
    candidates
        .iter()
        .find(|&&i| matches!(m[i][col].as_num(), Some(x) if x.abs() == Q::from_integer(1.into())))
        .or_else(|| candidates.iter().find(|&&i| m[i][col].as_num().is_some()))
        .or(candidates.first())
        .copied()
}

/// Gaussian elimination (back substitution) or Gauss–Jordan (reduced row echelon form).
fn eliminate(sys: &Linear, unknowns: &[String], jordan: bool) -> R<(Method, LinResult)> {
    let mut mtx: Vec<Vec<Sym>> = sys.a.iter().zip(&sys.rhs).map(|(row, r)| row.iter().cloned().chain([r.clone()]).collect()).collect();
    let ncols = unknowns.len();
    let mut steps = vec![
        step(system_latex(&sys.a, &sys.rhs, unknowns), "The system"),
        step(bracket(&mtx, true), "Write it as an augmented matrix"),
    ];
    let mut pivots: Vec<(usize, usize)> = Vec::new();
    let mut row = 0;
    for col in 0..ncols {
        if row >= mtx.len() {
            break;
        }
        let Some(p) = pick_pivot(&mtx, row, col) else { continue };
        if p != row {
            mtx.swap(p, row);
            steps.push(step(bracket(&mtx, true), format!("Swap $R_{{{}}}$ and $R_{{{}}}$", row + 1, p + 1)));
        }
        if jordan && !mtx[row][col].is_one() {
            let pv = mtx[row][col].clone();
            for x in mtx[row].iter_mut() {
                *x = tidy(d(x.clone(), pv.clone())?);
            }
            steps.push(step(bracket(&mtx, true), format!("$R_{{{0}}}\\to R_{{{0}}}\\div {1}$", row + 1, wrap(&pv))));
        }
        let targets: Vec<usize> = if jordan { (0..mtx.len()).filter(|&i| i != row).collect() } else { (row + 1..mtx.len()).collect() };
        let mut ops = Vec::new();
        for i in targets {
            if is_zero(&mtx[i][col]) {
                continue;
            }
            let factor = tidy(d(mtx[i][col].clone(), mtx[row][col].clone())?);
            for k in 0..=ncols {
                let v = add(vec![mtx[i][k].clone(), neg(mul(vec![factor.clone(), mtx[row][k].clone()]))]);
                mtx[i][k] = tidy(v);
            }
            ops.push(row_op(i, &factor, row));
        }
        if !ops.is_empty() {
            steps.push(step(
                bracket(&mtx, true),
                format!("${}$ to clear ${}$ {}", ops.join(",\\ "), unknowns[col], if jordan { "from the other rows" } else { "below the pivot" }),
            ));
        }
        pivots.push((row, col));
        row += 1;
    }
    for i in row..mtx.len() {
        if !is_zero(&mtx[i][ncols]) {
            steps.push(step(format!("0={}", latex(&mtx[i][ncols])), format!("Row {} says $0={}$: the system has no solution", i + 1, latex(&mtx[i][ncols]))));
            let name = if jordan { "Gauss–Jordan" } else { "Elimination" };
            return Ok((method(name, steps), LinResult::Inconsistent));
        }
    }
    let pivot_cols: Vec<usize> = pivots.iter().map(|&(_, c)| c).collect();
    let free: Vec<usize> = (0..ncols).filter(|c| !pivot_cols.contains(c)).collect();
    let mut values: Vec<Sym> = unknowns.iter().map(|u| var(u)).collect();
    // From the last pivot up: x_c = (rhs − Σ others) / pivot
    for &(r, c) in pivots.iter().rev() {
        let others = add((c + 1..ncols).map(|k| mul(vec![mtx[r][k].clone(), values[k].clone()])).collect());
        let x = tidy(d(add(vec![mtx[r][ncols].clone(), neg(others)]), mtx[r][c].clone())?);
        let note = if jordan { format!("Read off row {}", r + 1) } else { format!("Back-substitute in row {}", r + 1) };
        steps.push(step(eq(&var(&unknowns[c]), &x), note));
        values[c] = x;
    }
    if !free.is_empty() {
        let names = free.iter().map(|&c| unknowns[c].clone()).collect::<Vec<_>>().join(", ");
        steps.push(step("", format!("No pivot for ${names}$: free to take any value")));
    }
    let name = if jordan { "Gauss–Jordan" } else { "Elimination" };
    Ok((method(name, steps), LinResult::Solved(values, free)))
}

/// Determinant by elimination, exactly.
fn det(a: &[Vec<Sym>]) -> R<Sym> {
    let mut mtx = a.to_vec();
    let size = mtx.len();
    let mut result = n(1);
    for col in 0..size {
        let Some(p) = pick_pivot(&mtx, col, col) else { return Ok(n(0)) };
        if p != col {
            mtx.swap(p, col);
            result = neg(result);
        }
        result = tidy(mul(vec![result, mtx[col][col].clone()]));
        for i in col + 1..size {
            if is_zero(&mtx[i][col]) {
                continue;
            }
            let factor = tidy(d(mtx[i][col].clone(), mtx[col][col].clone())?);
            for k in col..size {
                let v = add(vec![mtx[i][k].clone(), neg(mul(vec![factor.clone(), mtx[col][k].clone()]))]);
                mtx[i][k] = tidy(v);
            }
        }
    }
    Ok(result)
}

fn cramer(sys: &Linear, unknowns: &[String]) -> R<Method> {
    let mut steps = vec![step(system_latex(&sys.a, &sys.rhs, unknowns), "The system")];
    if sys.a.len() != unknowns.len() {
        steps.push(step("", "Cramer's rule needs as many equations as unknowns"));
        return Ok(method("Cramer's rule", steps));
    }
    let dd = det(&sys.a)?;
    steps.push(step(format!("D={}={}", bars(&sys.a), latex(&dd)), "The determinant of the coefficients"));
    if is_zero(&dd) {
        steps.push(step("", "$D=0$, so Cramer's rule doesn't apply: there is no single solution"));
        return Ok(method("Cramer's rule", steps));
    }
    for (j, u) in unknowns.iter().enumerate() {
        let aj: Vec<Vec<Sym>> = sys.a.iter().zip(&sys.rhs).map(|(row, r)| {
            let mut row = row.clone();
            row[j] = r.clone();
            row
        }).collect();
        let dj = det(&aj)?;
        steps.push(step(
            format!("D_{{{u}}}={}={}", bars(&aj), latex(&dj)),
            format!("Replace the ${u}$ column with the constants"),
        ));
        let x = tidy(d(dj, dd.clone())?);
        steps.push(step(format!("{u}=\\frac{{D_{{{u}}}}}{{D}}={}", latex(&x)), ""));
    }
    Ok(method("Cramer's rule", steps))
}

fn inverse_matrix(sys: &Linear, unknowns: &[String]) -> R<Method> {
    let size = unknowns.len();
    let mut steps = vec![step(system_latex(&sys.a, &sys.rhs, unknowns), "The system")];
    let column = |xs: &[Sym]| xs.iter().map(|x| vec![x.clone()]).collect::<Vec<_>>();
    let names: Vec<Sym> = unknowns.iter().map(|u| var(u)).collect();
    steps.push(step(
        format!("{}{}={}", bracket(&sys.a, false), bracket(&column(&names), false), bracket(&column(&sys.rhs), false)),
        "As a matrix equation $AX=B$",
    ));
    if sys.a.len() != size || is_zero(&det(&sys.a)?) {
        steps.push(step("", "$A$ has no inverse (it isn't square, or its determinant is 0)"));
        return Ok(method("Inverse matrix", steps));
    }
    // Gauss–Jordan on [A | I].
    let mut mtx: Vec<Vec<Sym>> = sys.a.iter().enumerate().map(|(i, row)| {
        row.iter().cloned().chain((0..size).map(|k| n(if k == i { 1 } else { 0 }))).collect()
    }).collect();
    for col in 0..size {
        let p = pick_pivot(&mtx, col, col).expect("invertible");
        mtx.swap(p, col);
        let pv = mtx[col][col].clone();
        for x in mtx[col].iter_mut() {
            *x = tidy(d(x.clone(), pv.clone())?);
        }
        for i in (0..size).filter(|&i| i != col) {
            let factor = mtx[i][col].clone();
            if is_zero(&factor) {
                continue;
            }
            for k in 0..2 * size {
                let v = add(vec![mtx[i][k].clone(), neg(mul(vec![factor.clone(), mtx[col][k].clone()]))]);
                mtx[i][k] = tidy(v);
            }
        }
    }
    let inv: Vec<Vec<Sym>> = mtx.iter().map(|row| row[size..].to_vec()).collect();
    steps.push(step(format!("A^{{-1}}={}", bracket(&inv, false)), "Invert $A$ (Gauss–Jordan on $[A\\,|\\,I]$)"));
    let x: Vec<Sym> = inv.iter().map(|row| tidy(add(row.iter().zip(&sys.rhs).map(|(a, b)| mul(vec![a.clone(), b.clone()])).collect()))).collect();
    steps.push(step(
        format!("X=A^{{-1}}B={}{}={}", bracket(&inv, false), bracket(&column(&sys.rhs), false), bracket(&column(&x), false)),
        "Multiply",
    ));
    Ok(method("Inverse matrix", steps))
}

/// Substitution for a linear system: solve one equation for one unknown, put it into the
/// others, repeat, then work back.
fn substitution_linear(b: &mut Builder, sys: &Linear, unknowns: &[String]) -> R<Method> {
    let mut a = sys.a.clone();
    let mut rhs = sys.rhs.clone();
    let mut steps = vec![step(system_latex(&a, &rhs, unknowns), "The system")];
    let mut remaining: Vec<usize> = (0..a.len()).collect();
    let mut solved: Vec<(usize, Sym)> = Vec::new();
    while !remaining.is_empty() {
        // An equation and unknown with coefficient ±1 if possible.
        let choice = remaining
            .iter()
            .flat_map(|&i| (0..unknowns.len()).map(move |j| (i, j)))
            .filter(|&(i, j)| !is_zero(&a[i][j]))
            .min_by_key(|&(i, j)| match a[i][j].as_num() {
                Some(x) if x.abs() == Q::from_integer(1.into()) => 0,
                Some(_) => 1,
                None => 2,
            });
        let Some((i, j)) = choice else { break };
        let others = add((0..unknowns.len()).filter(|&k| k != j).map(|k| mul(vec![a[i][k].clone(), var(&unknowns[k])])).collect());
        let expr = tidy(d(add(vec![rhs[i].clone(), neg(others)]), a[i][j].clone())?);
        steps.push(step(eq(&var(&unknowns[j]), &expr), format!("Solve equation {} for ${}$", i + 1, unknowns[j])));
        remaining.retain(|&e| e != i);
        let mut changed = Vec::new();
        for &e in &remaining {
            if is_zero(&a[e][j]) {
                continue;
            }
            let factor = tidy(d(a[e][j].clone(), a[i][j].clone())?);
            for k in 0..unknowns.len() {
                a[e][k] = tidy(add(vec![a[e][k].clone(), neg(mul(vec![factor.clone(), a[i][k].clone()]))]));
            }
            rhs[e] = tidy(add(vec![rhs[e].clone(), neg(mul(vec![factor, rhs[i].clone()]))]));
            changed.push(e);
        }
        if !changed.is_empty() {
            let rows: Vec<String> = changed.iter().map(|&e| row_eq(&a[e], &rhs[e], unknowns)).collect();
            let shown = if rows.len() == 1 { rows[0].clone() } else { format!("\\begin{{cases}}{}\\end{{cases}}", rows.join("\\\\")) };
            steps.push(step(shown, format!("Substitute into the other equation{}", if changed.len() == 1 { "" } else { "s" })));
        }
        solved.push((j, expr));
        // An equation left with no unknowns is either 0 = 0 or a contradiction.
        for &e in &remaining {
            if a[e].iter().all(is_zero) && !is_zero(&rhs[e]) {
                steps.push(step(format!("0={}", latex(&rhs[e])), "A contradiction: no solution"));
                return Ok(method("Substitution", steps));
            }
        }
    }
    let mut values: BTreeMap<usize, Sym> = BTreeMap::new();
    for (j, expr) in solved.iter().rev() {
        let mut x = expr.clone();
        for (k, val) in &values {
            x = b.subst(&x, &unknowns[*k], val)?;
        }
        let x = tidy(x);
        if solved.len() > 1 && x != *expr {
            steps.push(step(eq(&var(&unknowns[*j]), &x), "Substitute the values found so far"));
        }
        values.insert(*j, x);
    }
    Ok(method("Substitution", steps))
}

fn linear(b: &mut Builder, sys: &Linear, unknowns: &[String]) -> R<Solution> {
    let (gauss, result) = eliminate(sys, unknowns, false)?;
    let (jordan, _) = eliminate(sys, unknowns, true)?;
    let methods = vec![substitution_linear(b, sys, unknowns)?, gauss, jordan, cramer(sys, unknowns)?, inverse_matrix(sys, unknowns)?];
    let (outcome, vars, note) = match result {
        LinResult::Inconsistent => (Outcome::NoSolution, unknowns.to_vec(), None),
        LinResult::Solved(values, free) => {
            let shown: Vec<usize> = (0..unknowns.len()).filter(|c| !free.contains(c)).collect();
            let vars = shown.iter().map(|&c| unknowns[c].clone()).collect();
            let set = shown.iter().map(|&c| Root::Exact(values[c].clone())).collect();
            let note = (!free.is_empty()).then(|| {
                let names = free.iter().map(|&c| unknowns[c].clone()).collect::<Vec<_>>().join(", ");
                format!("{names}\\text{{ free}}")
            });
            (Outcome::Sets(vec![set]), vars, note)
        }
    };
    Ok(Solution { vars, outcome, note, methods })
}

// ---------- Nonlinear systems ----------

type Assignment = BTreeMap<String, Root>;

fn nonlinear(b: &mut Builder, eqs: &[(Sym, Sym)], fs: &[Sym], unknowns: &[String]) -> R<Solution> {
    let mut steps = vec![step(
        format!("\\begin{{cases}}{}\\end{{cases}}", eqs.iter().map(|(l, r)| eq(l, r)).collect::<Vec<_>>().join("\\\\")),
        "The system",
    )];
    let sets = substitute_solve(b, fs.to_vec(), unknowns.to_vec(), &mut steps, 0)?;
    // Keep the sets that satisfy every original equation.
    let mut kept = Vec::new();
    for set in sets {
        let mut ok = true;
        for f in fs {
            let mut at = f.clone();
            let mut numeric = true;
            for (v, r) in &set {
                match r {
                    Root::Exact(x) => at = b.subst(&at, v, x)?,
                    Root::Approx(_) => numeric = false,
                }
            }
            if numeric && !at.has_vars() && !numerically_zero(b, &at) {
                ok = false;
            }
        }
        if ok {
            kept.push(set);
        }
    }
    // Sets in increasing order of the first unknown, then the next…
    let mut keyed: Vec<(Vec<f64>, Assignment)> = kept
        .into_iter()
        .map(|set| {
            let key = unknowns
                .iter()
                .map(|u| {
                    let v = match set.get(u) {
                        Some(Root::Exact(s)) if !s.has_vars() => b.numeric(s).ok(),
                        Some(Root::Approx(x)) => Some(x.clone()),
                        _ => None,
                    };
                    v.and_then(|v| crate::number::format_value(&v, &mut b.ev.prec))
                        .and_then(|d| d.text.parse().ok())
                        .unwrap_or(f64::NAN)
                })
                .collect();
            (key, set)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let outcome = if keyed.is_empty() {
        Outcome::NoSolution
    } else {
        Outcome::Sets(
            keyed
                .into_iter()
                .map(|(_, mut set)| unknowns.iter().map(|u| set.remove(u).unwrap_or(Root::Exact(var(u)))).collect())
                .collect(),
        )
    };
    Ok(Solution { vars: unknowns.to_vec(), outcome, note: None, methods: vec![method("Substitution", steps)] })
}

fn root_sym(b: &mut Builder, r: &Root) -> Sym {
    match r {
        Root::Exact(s) => s.clone(),
        // A numeric root carried on as the exact rational of its digits.
        Root::Approx(v) => match crate::number::format_value(v, &mut b.ev.prec) {
            Some(d) => crate::number::parse_exact(d.text.trim_start_matches('-'))
                .map(|q0| Sym::Num(if d.text.starts_with('-') { -q0 } else { q0 }))
                .unwrap_or_else(|| n(0)),
            None => n(0),
        },
    }
}

fn substitute_solve(b: &mut Builder, fs: Vec<Sym>, vars: Vec<String>, steps: &mut Vec<Step>, depth: u32) -> R<Vec<Assignment>> {
    if depth > 12 {
        return Err(EngineError::new("This system is too tangled to solve by substitution"));
    }
    let fs: Vec<Sym> = fs.into_iter().map(tidy).filter(|f| !is_zero(f)).collect();
    if vars.is_empty() || fs.is_empty() {
        if fs.iter().any(|f| !f.has_vars()) {
            return Ok(vec![]);
        }
        if !vars.is_empty() {
            return Err(EngineError::new("This system has more unknowns than independent equations"));
        }
        return Ok(vec![Assignment::new()]);
    }
    // An equation in a single unknown: solve it directly.
    for (i, f) in fs.iter().enumerate() {
        let mut in_f = BTreeSet::new();
        free_vars(f, &mut in_f);
        if in_f.len() == 1 {
            let v = in_f.pop_first().expect("one variable");
            let sol = single(b, f, &n(0), f, &v)?;
            if let Some(first) = sol.methods.first() {
                steps.push(step("", format!("Solve ${}=0$ for ${v}$:", latex(f))));
                steps.extend(first.steps.iter().skip(1).cloned());
            }
            let roots = match sol.outcome {
                Outcome::Sets(sets) => sets.into_iter().map(|mut s| s.remove(0)).collect(),
                _ => Vec::new(),
            };
            let rest_fs: Vec<Sym> = fs.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, g)| g.clone()).collect();
            let rest_vars: Vec<String> = vars.iter().filter(|u| **u != v).cloned().collect();
            let mut out = Vec::new();
            for root in roots {
                let value = root_sym(b, &root);
                let mut subbed = Vec::new();
                for g in &rest_fs {
                    subbed.push(b.subst(g, &v, &value)?);
                }
                for mut set in substitute_solve(b, subbed, rest_vars.clone(), steps, depth + 1)? {
                    set.insert(v.clone(), match &root {
                        Root::Exact(s) => Root::Exact(s.clone()),
                        Root::Approx(x) => Root::Approx(x.clone()),
                    });
                    out.push(set);
                }
            }
            return Ok(out);
        }
    }
    // An equation linear in some unknown with a numeric coefficient: express that unknown.
    for (i, f) in fs.iter().enumerate() {
        for v in &vars {
            let Some(c) = poly_coeffs(f, v) else { continue };
            if c.len() != 2 || c[1].has_vars() || is_zero(&c[1]) {
                continue;
            }
            let expr = tidy(neg(d(c[0].clone(), c[1].clone())?));
            steps.push(step(eq(&var(v), &expr), format!("Solve equation for ${v}$")));
            let mut rest = Vec::new();
            for (j, g) in fs.iter().enumerate() {
                if j != i {
                    rest.push(tidy(b.subst(g, v, &expr)?));
                }
            }
            let shown = rest.iter().map(|g| format!("{}=0", latex(g))).collect::<Vec<_>>().join(",\\quad ");
            steps.push(step(shown, format!("Substitute ${v}={}$ into the other equations", latex(&expr))));
            let rest_vars: Vec<String> = vars.iter().filter(|u| *u != v).cloned().collect();
            let mut out = Vec::new();
            for mut set in substitute_solve(b, rest, rest_vars, steps, depth + 1)? {
                let mut value = expr.clone();
                let mut approx = false;
                for (u, r) in &set {
                    if matches!(r, Root::Approx(_)) {
                        approx = true;
                    }
                    let rs = root_sym(b, r);
                    value = b.subst(&value, u, &rs)?;
                }
                let value = tidy(value);
                let root = if approx {
                    match b.numeric(&value) {
                        Ok(x) => Root::Approx(x),
                        Err(_) => Root::Exact(value),
                    }
                } else {
                    Root::Exact(value)
                };
                set.insert(v.clone(), root);
                out.push(set);
            }
            return Ok(out);
        }
    }
    Err(EngineError::new("I can't solve this nonlinear system yet (no equation is linear in one unknown)"))
}
