//! Optimisation: minimum, maximum and every critical point, with the working.
//!
//! One variable: f′ = 0, classified by f″ (or the sign of f′ either side), with the ends of an
//! interval as candidates and limits at ±∞ to tell whether an extreme exists. Several
//! variables: ∇f = 0 and the Hessian (Sylvester's minors). Constraints: Lagrange multipliers,
//! ∇f = λ∇g with g = c, comparing f at the candidates.

use std::collections::BTreeSet;

use num_rational::BigRational as Q;

use crate::ast::{Arg, Goal, Rel};
use crate::calculus::{self, free_vars};
use crate::latex::latex;
use crate::limits::{Lim, Point};
use crate::solve::{self, Outcome, Root, SymChain};
use crate::sym::{self, Builder, Sym, add, mul, n};
use crate::{EngineError, Method, Step};

type R<T> = Result<T, EngineError>;
/// A point as (variable, value) pairs.
type Point2 = Vec<(String, Sym)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Min,
    Max,
    Saddle,
    /// f′ = 0 but f′ keeps its sign: an inflection point.
    Inflection,
    /// An end of the interval.
    End,
    /// Neither test decides.
    Unclassified,
}

pub struct Extremum {
    pub kind: Kind,
    pub point: Vec<(String, Sym)>,
    pub value: Sym,
    /// The point came from a numeric root: show decimals.
    pub approx: bool,
}

pub struct Optimum {
    pub goal: Goal,
    pub found: Vec<Extremum>,
    /// Shown after the answer: "local", "on 0 ≤ x ≤ 5".
    pub note: Option<String>,
}

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn simp(s: Sym) -> Sym {
    sym::normalize_complex(crate::poly::expand(&s).unwrap_or(s))
}

/// x = 2 or (x, y) = (1, 2)
pub fn point_latex(point: &[(String, Sym)]) -> String {
    match point {
        [(v, x)] => format!("{v}={}", latex(x)),
        _ => format!(
            "\\left({}\\right)=\\left({}\\right)",
            point.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>().join(","),
            point.iter().map(|(_, x)| latex(x)).collect::<Vec<_>>().join(",")
        ),
    }
}

impl Builder<'_> {
    fn real_f64(&mut self, s: &Sym) -> R<f64> {
        let v = self.numeric(s)?;
        let fl = self.ev.prec.float_of(&v);
        crate::number::float_to_f64(&fl).ok_or_else(|| EngineError::new("The value isn't a real number"))
    }

    /// A root as a symbol: exact, or a fraction close to the decimal (marked approximate).
    fn root_sym(&mut self, r: Root) -> R<(Sym, bool)> {
        Ok(match r {
            Root::Exact(s) => (s, false),
            Root::Approx(v) => {
                let d = crate::number::format_value(&v, &mut self.ev.prec).ok_or_else(|| EngineError::new("A critical point is out of range"))?;
                let q = crate::number::parse_exact(&d.text.replace(['e', 'E'], "e")).unwrap_or_else(|| Q::from_integer(0.into()));
                (Sym::Num(q), true)
            }
        })
    }

    fn at_point(&mut self, f: &Sym, point: &[(String, Sym)]) -> R<Sym> {
        let mut s = f.clone();
        for (v, x) in point {
            s = self.subst(&s, v, x)?;
        }
        Ok(simp(s))
    }

    /// `minimize(f, …)`, `maximize(f, …)`, `extrema(f, …)`.
    pub(crate) fn optimize(&mut self, goal: Goal, args: &[Arg]) -> R<Optimum> {
        let [Arg::Expr(fe), rest @ ..] = args else {
            return Err(EngineError::new("Write minimize(f), maximize(f, x+y=10) or extrema(f, 0 ≤ x ≤ 5)"));
        };
        let f = self.build(fe)?;
        if matches!(f, Sym::Matrix(_)) {
            return Err(EngineError::new("Optimisation needs a single expression, not a matrix"));
        }
        let mut constraints = Vec::new();
        let mut chains = Vec::new();
        let mut plain = Vec::new();
        for a in rest {
            match a {
                Arg::Eq(l, r) => constraints.push((self.build(l)?, self.build(r)?)),
                Arg::Chain(c) => {
                    let parts = c.parts.iter().map(|p| self.build(p)).collect::<R<Vec<_>>>()?;
                    chains.push(SymChain { parts, rels: c.rels.clone() });
                }
                Arg::Expr(e) => plain.push(self.build(e)?),
            }
        }
        let mut letters = BTreeSet::new();
        free_vars(&f, &mut letters);
        for (l, r) in &constraints {
            free_vars(l, &mut letters);
            free_vars(r, &mut letters);
        }
        let vars: Vec<String> = letters.into_iter().collect();
        if vars.is_empty() {
            return Err(EngineError::new("There's no variable to change"));
        }
        // minimize(f, a, b): the interval a ≤ x ≤ b.
        if let [a, b] = plain.as_slice() {
            let [v] = vars.as_slice() else { return Err(EngineError::new("An interval a, b works for one variable; write 0 ≤ x ≤ 5 for several")) };
            chains.push(SymChain { parts: vec![a.clone(), Sym::Var(v.clone()), b.clone()], rels: vec![Rel::Le, Rel::Le] });
        } else if !plain.is_empty() {
            return Err(EngineError::new("Give an interval as two numbers or as 0 ≤ x ≤ 5"));
        }
        if !constraints.is_empty() {
            return self.lagrange(goal, &f, &vars, &constraints);
        }
        if let [v] = vars.as_slice() {
            return self.optimize_one(goal, &f, v, &chains);
        }
        if !chains.is_empty() {
            return Err(EngineError::new("Intervals work for one variable for now"));
        }
        self.optimize_many(goal, &f, &vars)
    }

    fn optimize_one(&mut self, goal: Goal, f: &Sym, v: &str, chains: &[SymChain]) -> R<Optimum> {
        let angle = self.ev.angle;
        let d1 = simp(calculus::diff(f, v, angle)?);
        let d2 = simp(calculus::diff(&d1, v, angle)?);
        let mut steps = vec![step(format!("f'({v})={}", latex(&d1)), "Differentiate")];
        let sol = solve::solve_for_within(self, &d1, &n(0), v, chains)?;
        if sol.note.as_deref().is_some_and(|nt| nt.contains("\\mathbb{Z}")) {
            return Err(EngineError::new(format!("f has infinitely many critical points: give an interval, like 0 ≤ {v} ≤ 2π")));
        }
        let roots: Vec<Root> = match sol.outcome {
            Outcome::Sets(sets) => sets.into_iter().filter_map(|mut s| s.pop()).collect(),
            Outcome::AllValues => return Err(EngineError::new("f is constant: every point is the same")),
            _ => Vec::new(),
        };
        let mut found = Vec::new();
        let mut shown = Vec::new();
        for r in roots {
            let (p, approx) = self.root_sym(r)?;
            if p.has_vars() || sym::contains_i(&p) {
                continue;
            }
            // A numeric root that makes f′ exactly 0 (like −1 for xeˣ) is exact after all.
            let approx = approx && !self.subst(&d1, v, &p).map(simp).is_ok_and(|s| s.is_zero());
            let point = vec![(v.to_string(), p.clone())];
            let value = self.at_point(f, &point)?;
            let curvature = self.at_point(&d2, &point)?;
            let c = self.real_f64(&curvature)?;
            let (kind, why) = if c.abs() > 1e-20 {
                if c > 0.0 { (Kind::Min, format!("f''={}>0", latex(&curvature))) } else { (Kind::Max, format!("f''={}<0", latex(&curvature))) }
            } else {
                // f″ = 0: the sign of f′ either side.
                let px = self.real_f64(&p)?;
                let h = 1e-6 * px.abs().max(1.0);
                let side = |b: &mut Self, x: f64| -> R<f64> {
                    let at = Sym::Num(Q::from_float(x).unwrap_or_else(|| Q::from_integer(0.into())));
                    let s = b.subst(&d1, v, &at)?;
                    b.real_f64(&s)
                };
                let (l, r) = (side(self, px - h)?, side(self, px + h)?);
                match (l < 0.0, r > 0.0, l > 0.0, r < 0.0) {
                    (true, true, _, _) => (Kind::Min, "f' goes from − to +".to_string()),
                    (_, _, true, true) => (Kind::Max, "f' goes from + to −".to_string()),
                    _ => (Kind::Inflection, "f' doesn't change sign: an inflection point".to_string()),
                }
            };
            shown.push(format!("{}:\\ {why}", point_latex(&point)));
            found.push(Extremum { kind, point, value, approx });
        }
        steps.push(step(
            if found.is_empty() { "\\text{none}".to_string() } else { found.iter().map(|e| point_latex(&e.point)).collect::<Vec<_>>().join(",\\quad ") },
            format!("Critical points: solve $f'({v})=0$"),
        ));
        if !shown.is_empty() {
            steps.push(step(shown.join(",\\quad "), format!("Second derivative: $f''({v})={}$", latex(&d2))));
        }
        // The ends of an interval.
        let mut ends = Vec::new();
        for c in chains {
            if let [lo, Sym::Var(x), hi] = c.parts.as_slice()
                && x == v
            {
                for e in [lo, hi] {
                    let point = vec![(v.to_string(), e.clone())];
                    let value = self.at_point(f, &point)?;
                    ends.push(Extremum { kind: Kind::End, point, value, approx: false });
                }
            }
        }
        let interval = !ends.is_empty();
        if interval {
            steps.push(step(
                ends.iter().map(|e| format!("f({})={}", latex(&e.point[0].1), latex(&e.value))).collect::<Vec<_>>().join(",\\quad "),
                "The ends of the interval are candidates too",
            ));
        }
        let note = interval.then(|| chains.iter().map(chain_latex).collect::<Vec<_>>().join(",\\ "));
        let result = match goal {
            Goal::All => {
                let mut all = found;
                if interval {
                    all.extend(ends);
                }
                Optimum { goal, found: all, note }
            }
            Goal::Min | Goal::Max => {
                let want = if goal == Goal::Min { Kind::Min } else { Kind::Max };
                let mut candidates: Vec<Extremum> = found.into_iter().filter(|e| interval || e.kind == want).collect();
                if interval {
                    candidates.extend(ends);
                } else {
                    // Without an interval, f must not run off to ∓∞.
                    for (p, name) in [(Point::PosInf, "\\infty"), (Point::NegInf, "-\\infty")] {
                        if let Ok((l, _)) = self.limit(f, v, p, 0) {
                            let escapes = matches!((&l, goal), (Lim::Neg, Goal::Min) | (Lim::Pos, Goal::Max));
                            if escapes {
                                let which = if goal == Goal::Min { "minimum" } else { "maximum" };
                                let to = if goal == Goal::Min { "-\\infty" } else { "\\infty" };
                                steps.push(step(format!("\\lim_{{{v}\\to {name}}}f={to}"), format!("No {which}: f is unbounded")));
                                self.ev.work.push(Method { name: "Optimisation".into(), steps });
                                return Err(EngineError::new(format!("There's no {which}: f goes to {}∞ as {v} → {}∞", if goal == Goal::Min { "−" } else { "" }, if name.starts_with('-') { "−" } else { "" })));
                            }
                        }
                    }
                }
                if candidates.is_empty() {
                    let which = if goal == Goal::Min { "minimum" } else { "maximum" };
                    self.ev.work.push(Method { name: "Optimisation".into(), steps });
                    return Err(EngineError::new(format!("No local {which} found")));
                }
                let best = self.pick(goal, candidates)?;
                steps.push(step(
                    format!("{}\\ f={}", if goal == Goal::Min { "\\min" } else { "\\max" }, latex(&best[0].value)),
                    if interval { "The best of the candidates" } else { "The best local one, and f doesn't run off to infinity" },
                ));
                Optimum { goal, found: best, note }
            }
        };
        self.ev.work.push(Method { name: "Optimisation".into(), steps });
        Ok(result)
    }

    /// The candidates with the smallest (Min) or largest (Max) value; ties all kept.
    fn pick(&mut self, goal: Goal, candidates: Vec<Extremum>) -> R<Vec<Extremum>> {
        let mut keyed = Vec::new();
        for c in candidates {
            let v = self.real_f64(&c.value)?;
            keyed.push((v, c));
        }
        let best = keyed.iter().map(|(v, _)| *v).fold(if goal == Goal::Min { f64::INFINITY } else { f64::NEG_INFINITY }, |a, b| if goal == Goal::Min { a.min(b) } else { a.max(b) });
        let tol = 1e-12 * best.abs().max(1.0);
        let mut out: Vec<Extremum> = keyed.into_iter().filter(|(v, _)| (v - best).abs() <= tol).map(|(_, c)| c).collect();
        for e in &mut out {
            e.kind = if goal == Goal::Min { Kind::Min } else { Kind::Max };
        }
        Ok(out)
    }

    /// The solution sets as points, each marked approximate when it came from a numeric root.
    fn solutions(&mut self, sol: solve::Solution) -> R<Vec<(Point2, bool)>> {
        let Outcome::Sets(sets) = sol.outcome else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for set in sets {
            let mut point = Vec::new();
            let mut approx = false;
            for (v, r) in sol.vars.iter().zip(set) {
                let (s, a) = self.root_sym(r)?;
                approx |= a;
                point.push((v.clone(), s));
            }
            if point.iter().any(|(_, s)| s.has_vars() || sym::contains_i(s)) {
                continue;
            }
            out.push((point, approx));
        }
        Ok(out)
    }

    fn optimize_many(&mut self, goal: Goal, f: &Sym, vars: &[String]) -> R<Optimum> {
        let angle = self.ev.angle;
        let grad: Vec<Sym> = vars.iter().map(|v| calculus::diff(f, v, angle).map(simp)).collect::<R<_>>()?;
        let mut steps = vec![step(
            format!("\\nabla f=\\left({}\\right)", grad.iter().map(latex).collect::<Vec<_>>().join(",")),
            "The partial derivatives",
        )];
        let eqs: Vec<(Sym, Sym)> = grad.iter().map(|g| (g.clone(), n(0))).collect();
        let sol = solve::solve_system_for(self, eqs, vars)?;
        let points = self.solutions(sol)?;
        steps.push(step(
            if points.is_empty() { "\\text{none}".into() } else { points.iter().map(|(p, _)| point_latex(p)).collect::<Vec<_>>().join(",\\quad ") },
            "Critical points: solve $\\nabla f=0$",
        ));
        let hessian: Vec<Vec<Sym>> = grad.iter().map(|g| vars.iter().map(|v| calculus::diff(g, v, angle).map(simp)).collect::<R<_>>()).collect::<R<_>>()?;
        steps.push(step(format!("H={}", crate::matrix::matrix_latex(&hessian)), "The Hessian: second partial derivatives"));
        let mut found = Vec::new();
        for (point, approx) in points {
            let h: Vec<Vec<Sym>> = hessian.iter().map(|r| r.iter().map(|e| self.at_point(e, &point)).collect::<R<_>>()).collect::<R<_>>()?;
            // Sylvester: the leading principal minors.
            let mut minors = Vec::new();
            for k in 1..=vars.len() {
                let sub: Vec<Vec<Sym>> = h[..k].iter().map(|r| r[..k].to_vec()).collect();
                minors.push(self.real_f64(&simp(crate::matrix::det_cofactor(&sub)))?);
            }
            let eps = 1e-20;
            let kind = if minors.iter().all(|m| *m > eps) {
                Kind::Min
            } else if minors.iter().enumerate().all(|(i, m)| if i % 2 == 0 { *m < -eps } else { *m > eps }) {
                Kind::Max
            } else if minors.last().is_some_and(|d| d.abs() > eps) {
                Kind::Saddle
            } else {
                Kind::Unclassified
            };
            let value = self.at_point(f, &point)?;
            let label = match kind {
                Kind::Min => "minimum",
                Kind::Max => "maximum",
                Kind::Saddle => "saddle point",
                Kind::Unclassified | Kind::Inflection | Kind::End => "the test can't tell",
            };
            let det = simp(crate::matrix::det_cofactor(&h));
            steps.push(step(format!("{}:\\ \\det H={}", point_latex(&point), latex(&det)), format!("A {label} (signs of the leading minors of $H$)")));
            found.push(Extremum { kind, point, value, approx });
        }
        let result = match goal {
            Goal::All => Optimum { goal, found, note: None },
            Goal::Min | Goal::Max => {
                let want = if goal == Goal::Min { Kind::Min } else { Kind::Max };
                let candidates: Vec<Extremum> = found.into_iter().filter(|e| e.kind == want).collect();
                if candidates.is_empty() {
                    self.ev.work.push(Method { name: "Optimisation".into(), steps });
                    return Err(EngineError::new(format!("No local {} found", if goal == Goal::Min { "minimum" } else { "maximum" })));
                }
                Optimum { goal, found: self.pick(goal, candidates)?, note: Some("\\text{local}".into()) }
            }
        };
        self.ev.work.push(Method { name: "Optimisation".into(), steps });
        Ok(result)
    }

    /// ∇f = Σλⱼ∇gⱼ with gⱼ = cⱼ.
    fn lagrange(&mut self, goal: Goal, f: &Sym, vars: &[String], constraints: &[(Sym, Sym)]) -> R<Optimum> {
        let angle = self.ev.angle;
        let lambdas: Vec<String> = if constraints.len() == 1 { vec!["\\lambda".into()] } else { (1..=constraints.len()).map(|i| format!("\\lambda_{{{i}}}")).collect() };
        let mut eqs = Vec::new();
        for v in vars {
            let df = calculus::diff(f, v, angle)?;
            let mut rhs = Vec::new();
            for ((g, _), l) in constraints.iter().zip(&lambdas) {
                rhs.push(mul(vec![Sym::Var(l.clone()), calculus::diff(g, v, angle)?]));
            }
            eqs.push((simp(df), simp(add(rhs))));
        }
        for (g, c) in constraints {
            eqs.push((g.clone(), c.clone()));
        }
        let mut steps = vec![step(
            eqs.iter().map(|(l, r)| format!("{}={}", latex(l), latex(r))).collect::<Vec<_>>().join(",\\quad "),
            "Lagrange: $\\nabla f=\\lambda\\nabla g$ together with the constraint",
        )];
        // One constraint: eliminate λ first, f_x g_y − f_y g_x = 0 (each pair with the first
        // variable), which often leaves a linear equation (x + y on a circle: y = x).
        let sol = if constraints.len() == 1 && vars.len() >= 2 {
            let (g, c) = &constraints[0];
            let df0 = calculus::diff(f, &vars[0], angle)?;
            let dg0 = calculus::diff(g, &vars[0], angle)?;
            let mut reduced = Vec::new();
            for v in &vars[1..] {
                let df = calculus::diff(f, v, angle)?;
                let dg = calculus::diff(g, v, angle)?;
                reduced.push((simp(add(vec![mul(vec![df0.clone(), dg]), sym::neg(mul(vec![df, dg0.clone()]))])), n(0)));
            }
            reduced.push((g.clone(), c.clone()));
            steps.push(step(
                reduced.iter().map(|(l, r)| format!("{}={}", latex(l), latex(r))).collect::<Vec<_>>().join(",\\quad "),
                "Eliminate $\\lambda$: $f_xg_y-f_yg_x=0$ with the constraint",
            ));
            solve::solve_system_for(self, reduced, vars)?
        } else {
            let mut unknowns: Vec<String> = vars.to_vec();
            unknowns.extend(lambdas.iter().cloned());
            solve::solve_system_for(self, eqs, &unknowns)?
        };
        let mut found = Vec::new();
        for (point, approx) in self.solutions(sol)? {
            let point: Vec<(String, Sym)> = point.into_iter().filter(|(v, _)| vars.contains(v)).collect();
            let value = self.at_point(f, &point)?;
            found.push(Extremum { kind: Kind::Unclassified, point, value, approx });
        }
        if found.is_empty() {
            self.ev.work.push(Method { name: "Lagrange multipliers".into(), steps });
            return Err(EngineError::new("No candidate points found"));
        }
        steps.push(step(
            found.iter().map(|e| format!("{}:\\ f={}", point_latex(&e.point), latex(&e.value))).collect::<Vec<_>>().join(",\\quad "),
            "The candidates and f at each",
        ));
        let result = match goal {
            Goal::All => Optimum { goal, found, note: None },
            _ => {
                let best = self.pick(goal, found)?;
                steps.push(step(format!("{}\\ f={}", if goal == Goal::Min { "\\min" } else { "\\max" }, latex(&best[0].value)), "Compare the candidates"));
                Optimum { goal, found: best, note: Some(constraints.iter().map(|(g, c)| format!("{}={}", latex(g), latex(c))).collect::<Vec<_>>().join(",\\ ")) }
            }
        };
        self.ev.work.push(Method { name: "Lagrange multipliers".into(), steps });
        Ok(result)
    }
}

fn chain_latex(c: &SymChain) -> String {
    let mut out = latex(&c.parts[0]);
    for (r, p) in c.rels.iter().zip(&c.parts[1..]) {
        out.push_str(r.latex());
        out.push_str(&latex(p));
    }
    out
}
