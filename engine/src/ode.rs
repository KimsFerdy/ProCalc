//! Ordinary differential equations, with the working.
//!
//! Exact methods, tried in order: linear equations with constant coefficients (the
//! characteristic equation, then undetermined coefficients or variation of parameters),
//! first-order linear equations (integrating factor), and separable equations. Initial
//! conditions fix the constants. Anything else with a full set of initial conditions is
//! solved numerically (RK4).

use astro_float::BigFloat;
use num_rational::BigRational as Q;
use num_traits::{Signed, Zero};

use crate::ast::{Const, Expr, Func};
use crate::calculus::{self, free_of};
use crate::eval::Evaluator;
use crate::latex::latex;
use crate::number::Value;
use crate::poly::{self, Poly};
use crate::solve::{self, Outcome, Root};
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow};
use crate::{EngineError, Method, Step};

type R<T> = Result<T, EngineError>;

/// Largest order solved.
const MAX_ORDER: u32 = 8;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn simp(s: Sym) -> Sym {
    sym::normalize_complex(poly::expand(&s).unwrap_or(s))
}

/// y, y', y'' as variable names.
fn yk(y: &str, k: u32) -> String {
    format!("{y}{}", "'".repeat(k as usize))
}

/// True when the expression mentions the unknown function anywhere.
pub fn mentions_ode(e: &Expr) -> bool {
    match e {
        Expr::OdeFn { .. } => true,
        Expr::Neg(a) | Expr::Call(_, a) | Expr::Factorial(a, _) | Expr::Percent(a) | Expr::Op(_, a) => mentions_ode(a),
        Expr::Binary(_, a, b) | Expr::Pow(a, b) | Expr::Root(a, b) | Expr::LogBase(a, b) => mentions_ode(a) || mentions_ode(b),
        Expr::Deriv { body, .. } => mentions_ode(body),
        Expr::UserCall(_, args) | Expr::NumCall(_, args) | Expr::MatCall(_, args) => args.iter().any(mentions_ode),
        _ => false,
    }
}

/// The independent variable when the input names it (\frac{dy}{dt}, d/dt y).
fn named_var(e: &Expr) -> Option<String> {
    match e {
        Expr::OdeFn { var, .. } => var.clone(),
        Expr::Deriv { body, var, .. } if matches!(**body, Expr::OdeFn { .. }) => Some(var.clone()),
        Expr::Neg(a) | Expr::Call(_, a) | Expr::Factorial(a, _) | Expr::Percent(a) | Expr::Op(_, a) => named_var(a),
        Expr::Binary(_, a, b) | Expr::Pow(a, b) | Expr::Root(a, b) | Expr::LogBase(a, b) => named_var(a).or_else(|| named_var(b)),
        _ => None,
    }
}

/// An initial condition y^(k)(x₀) = v.
struct Initial {
    order: u32,
    at: Sym,
    value: Sym,
}

/// A root of the characteristic equation: re ± im·i, with its multiplicity.
struct CharRoot {
    re: Sym,
    im: Sym,
    mult: u32,
}

/// What came out: y = f(x) (one or more branches), an implicit relation, or a numeric
/// solution for the graph.
pub enum OdeResult {
    Explicit(Vec<Sym>),
    Implicit(Sym, Sym),
    Numeric { rhs: Sym, x0: f64, start: Vec<f64>, at_one: f64 },
}

pub struct Ode {
    pub y: String,
    pub x: String,
    pub order: u32,
    pub result: OdeResult,
}

impl Ode {
    /// The answer as LaTeX: y = …, or the implicit relation.
    pub fn latex(&self) -> String {
        match &self.result {
            OdeResult::Explicit(branches) => {
                branches.iter().map(|f| format!("{}={}", self.y, latex(f))).collect::<Vec<_>>().join("\\quad\\text{or}\\quad ")
            }
            OdeResult::Implicit(l, r) => format!("{}={}", latex(l), latex(r)),
            OdeResult::Numeric { x0, at_one, .. } => {
                format!("\\text{{solved numerically: }}{}\\left({}\\right)\\approx {}", self.y, fmt_f64(x0 + 1.0), fmt_f64(*at_one))
            }
        }
    }
}

fn fmt_f64(v: f64) -> String {
    let s = format!("{v:.10}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

impl Builder<'_> {
    /// Solves a differential equation given as equations (the ODE and its initial conditions).
    pub(crate) fn solve_ode(&mut self, eqs: &[(Expr, Expr)]) -> R<Ode> {
        let mut odes = Vec::new();
        let mut initial_exprs = Vec::new();
        for (l, r) in eqs {
            match (l, r) {
                (Expr::OdeFn { name, order, at: Some(a), .. }, v) | (v, Expr::OdeFn { name, order, at: Some(a), .. }) if !mentions_ode(v) => {
                    initial_exprs.push((name.clone(), *order, (**a).clone(), v.clone()));
                }
                _ => odes.push((l, r)),
            }
        }
        let [(l, r)] = odes.as_slice() else {
            return Err(EngineError::new("Write one differential equation, plus initial conditions like y(0) = 1"));
        };
        let y = first_name(l).or_else(|| first_name(r)).ok_or_else(|| EngineError::new("Couldn't find the unknown function"))?;
        let left = self.build(l)?;
        let right = self.build(r)?;
        let f = simp(add(vec![left.clone(), neg(right.clone())]));
        // y, y', … up to the order.
        let mut order = 0;
        let mut vars = std::collections::BTreeSet::new();
        calculus::free_vars(&f, &mut vars);
        for k in 0..=MAX_ORDER + 1 {
            if vars.contains(&yk(&y, k)) {
                order = k;
            }
        }
        if order == 0 {
            return Err(EngineError::new(format!("This has no derivative of {y} in it, so it isn't a differential equation")));
        }
        if order > MAX_ORDER {
            return Err(EngineError::new(format!("Differential equations up to order {MAX_ORDER}")));
        }
        let ys: Vec<String> = (0..=order).map(|k| yk(&y, k)).collect();
        // x unless the equation uses t (and not x); other letters (ω, k) are parameters.
        let x = named_var(l).or_else(|| named_var(r)).unwrap_or_else(|| {
            if vars.contains("t") && !vars.contains("x") { "t".into() } else { "x".into() }
        });
        let mut initials = Vec::new();
        for (name, k, a, v) in &initial_exprs {
            if *name != y {
                return Err(EngineError::new(format!("The initial condition is for {name}, but the equation is in {y}")));
            }
            if *k >= order {
                return Err(EngineError::new(if order == 1 {
                    format!("A first-order equation takes one condition, like {y}(0) = 1")
                } else {
                    format!("An order-{order} equation takes conditions on {y}, {y}', … up to {}", yk(&y, order - 1))
                }));
            }
            initials.push(Initial { order: *k, at: self.build(a)?, value: self.build(v)? });
        }
        if initials.len() > order as usize {
            return Err(EngineError::new(format!("An order-{order} equation takes at most {order} initial conditions")));
        }
        let mut steps = vec![step(format!("{}={}", latex(&left), latex(&right)), format!("A differential equation of order {order}"))];

        let general = self.exact_general(&f, &y, &x, order, &ys, &mut steps)?;
        let result = match general {
            Some(g) => self.apply_initials(g, &y, &x, &initials, &mut steps)?,
            None => {
                if initials.len() < order as usize {
                    return Err(EngineError::new(format!(
                        "I can't solve this one exactly yet; with {order} initial condition{} I can solve it numerically",
                        if order == 1 { "" } else { "s" }
                    )));
                }
                self.numeric_ode(&f, &y, &x, order, &ys, &initials, &mut steps)?
            }
        };
        let ode = Ode { y: y.clone(), x, order, result };
        steps.push(step(ode.latex(), "The solution"));
        self.ev.work.push(Method { name: "Differential equation".into(), steps });
        Ok(ode)
    }

    /// The general solution with its constants, when an exact method applies.
    fn exact_general(&mut self, f: &Sym, y: &str, x: &str, order: u32, ys: &[String], steps: &mut Vec<Step>) -> R<Option<General>> {
        if let Some((a, g)) = linear_parts(f, ys) {
            // First order: the integrating factor covers constant and varying coefficients.
            if order == 1 {
                return self.integrating_factor(&a, &g, y, x, steps);
            }
            if a.iter().all(|c| free_of(c, x)) {
                return self.constant_coefficients(&a, &g, y, x, steps);
            }
            return Ok(None);
        }
        if order == 1 {
            return self.separable(f, y, x, steps);
        }
        Ok(None)
    }

    /// aₙy⁽ⁿ⁾ + … + a₀y = g with constant aₖ.
    fn constant_coefficients(&mut self, a: &[Sym], g: &Sym, y: &str, x: &str, steps: &mut Vec<Step>) -> R<Option<General>> {
        let order = a.len() - 1;
        let r = Sym::Var("r".into());
        let char_poly = simp(add(a.iter().enumerate().map(|(k, c)| mul(vec![c.clone(), sym::pow_or_keep(r.clone(), n(k as i64))])).collect()));
        steps.push(step(format!("{}=0", latex(&char_poly)), format!("Try ${y}=e^{{r{x}}}$: the characteristic equation")));
        let Some(roots) = self.char_roots(a)? else {
            steps.push(step("", "Its roots have no exact form here"));
            return Ok(None);
        };
        let shown = roots
            .iter()
            .map(|c| {
                let v = if c.im.is_zero() { latex(&c.re) } else { format!("{}\\pm {}i", latex(&c.re), latex(&c.im)) };
                if c.mult > 1 { format!("r={v}\\ (\\times{})", c.mult) } else { format!("r={v}") }
            })
            .collect::<Vec<_>>()
            .join(",\\quad ");
        steps.push(step(shown, "Its roots"));
        let xs = Sym::Var(x.into());
        let mut basis = Vec::new();
        for c in &roots {
            let exp = pow(Sym::Const(Const::E), mul(vec![c.re.clone(), xs.clone()]))?;
            for j in 0..c.mult {
                let xj = sym::pow_or_keep(xs.clone(), n(j as i64));
                if c.im.is_zero() {
                    basis.push(simp(mul(vec![xj, exp.clone()])));
                } else {
                    let angle = mul(vec![c.im.clone(), xs.clone()]);
                    basis.push(mul(vec![xj.clone(), exp.clone(), self.func(Func::Cos, angle.clone())?]));
                    basis.push(mul(vec![xj, exp.clone(), self.func(Func::Sin, angle)?]));
                }
            }
        }
        let names = constant_names(order);
        let homogeneous = add(basis.iter().zip(&names).map(|(b, c)| mul(vec![Sym::Var(c.clone()), b.clone()])).collect());
        steps.push(step(format!("{y}_h={}", latex(&homogeneous)), "Real roots give $e^{rx}$, repeated ones an extra $x$ each, $\\alpha\\pm\\beta i$ gives $e^{\\alpha x}\\cos\\beta x$ and $e^{\\alpha x}\\sin\\beta x$"));
        let particular = if g.is_zero() {
            n(0)
        } else {
            match self.undetermined(a, g, &roots, x, steps)? {
                Some(p) => p,
                None if order == 2 => match self.variation_of_parameters(a, g, &basis, x, steps)? {
                    Some(p) => p,
                    None => return Ok(None),
                },
                None => return Ok(None),
            }
        };
        let solution = add(vec![homogeneous, particular]);
        Ok(Some(General::Explicit(vec![solution], names)))
    }

    /// Roots of Σ aₖrᵏ with multiplicities, exactly.
    fn char_roots(&mut self, a: &[Sym]) -> R<Option<Vec<CharRoot>>> {
        let order = a.len() - 1;
        let lead = a[order].clone();
        if order == 1 {
            return Ok(Some(vec![CharRoot { re: simp(div(neg(a[0].clone()), lead)?), im: n(0), mult: 1 }]));
        }
        let rational: Option<Vec<Q>> = a.iter().map(|c| c.as_num().cloned()).collect();
        if let Some(cs) = rational {
            let mut out = Vec::new();
            for (fct, m) in poly::factor_univariate(&Poly(cs).trim()) {
                match fct.deg() {
                    1 => out.push(CharRoot { re: Sym::Num(-&fct.0[0] / &fct.0[1]), im: n(0), mult: m }),
                    2 => out.extend(self.quadratic_roots(&Sym::Num(fct.0[2].clone()), &Sym::Num(fct.0[1].clone()), &Sym::Num(fct.0[0].clone()), m)?),
                    // a r⁴ + b r² + c: a quadratic in s = r², then r = ±√s.
                    4 if fct.0[1].is_zero() && fct.0[3].is_zero() => {
                        let (a, b, c) = (Sym::Num(fct.0[4].clone()), Sym::Num(fct.0[2].clone()), Sym::Num(fct.0[0].clone()));
                        let d = simp(add(vec![pow(b.clone(), n(2))?, mul(vec![n(-4), a.clone(), c])]));
                        let sq = sym::normalize_complex(pow(d, sym::q(1, 2))?);
                        for sign in [1, -1] {
                            let s = sym::normalize_complex(simp(div(add(vec![neg(b.clone()), mul(vec![n(sign), sq.clone()])]), mul(vec![n(2), a.clone()]))?));
                            for r in self.nth_roots(&s, 2)? {
                                let Some((re, im)) = sym::rect(&r) else { return Ok(None) };
                                let (re, im) = (simp(re), simp(im));
                                // One root of each conjugate pair stands for both.
                                let negative_im = match im.as_num() {
                                    Some(v) => v.is_negative(),
                                    None => sym::split_coeff(im.clone()).0.is_negative(),
                                };
                                if negative_im {
                                    continue;
                                }
                                if !out.iter().any(|c: &CharRoot| c.re == re && c.im == im) {
                                    out.push(CharRoot { re, im, mult: m });
                                }
                            }
                        }
                    }
                    _ => return Ok(None),
                }
            }
            return Ok(Some(out));
        }
        if order == 2 {
            return Ok(Some(self.quadratic_roots(&a[2], &a[1], &a[0], 1)?));
        }
        Ok(None)
    }

    /// Roots of a r² + b r + c, as real pair, double root or complex pair.
    fn quadratic_roots(&mut self, a: &Sym, b: &Sym, c: &Sym, mult: u32) -> R<Vec<CharRoot>> {
        let d = simp(add(vec![pow(b.clone(), n(2))?, mul(vec![n(-4), a.clone(), c.clone()])]));
        let two_a = mul(vec![n(2), a.clone()]);
        let re = simp(div(neg(b.clone()), two_a.clone())?);
        if d.is_zero() {
            return Ok(vec![CharRoot { re, im: n(0), mult: 2 * mult }]);
        }
        let negative = match d.as_num() {
            Some(v) => v.is_negative(),
            None => sym::split_coeff(d.clone()).0.is_negative(),
        };
        let root = |s: Sym| -> R<Sym> { Ok(simp(div(sqrt_simple(&s)?, two_a.clone())?)) };
        if negative {
            Ok(vec![CharRoot { re, im: root(neg(d))?, mult }])
        } else {
            let s = root(d)?;
            Ok(vec![
                CharRoot { re: simp(add(vec![re.clone(), neg(s.clone())])), im: n(0), mult },
                CharRoot { re: simp(add(vec![re, s])), im: n(0), mult },
            ])
        }
    }

    /// A particular solution for g made of polynomials × e^{ax} × cos/sin(bx).
    fn undetermined(&mut self, a: &[Sym], g: &Sym, roots: &[CharRoot], x: &str, steps: &mut Vec<Step>) -> R<Option<Sym>> {
        let xs = Sym::Var(x.into());
        let mut groups: Vec<(Sym, Sym, Vec<Sym>, u32)> = Vec::new(); // (α, β, terms, degree)
        for t in terms(&simp(g.clone())) {
            let Some((alpha, beta, deg)) = classify(&t, x) else { return Ok(None) };
            match groups.iter_mut().find(|(al, be, _, _)| *al == alpha && *be == beta) {
                Some(gr) => {
                    gr.2.push(t);
                    gr.3 = gr.3.max(deg);
                }
                None => groups.push((alpha, beta, vec![t], deg)),
            }
        }
        let mut unknowns = Vec::new();
        let mut guess = Vec::new();
        let mut equations = Vec::new();
        for (alpha, beta, ts, deg) in &groups {
            // Resonance: multiply by x^s where α + βi is a root s times.
            let s = roots.iter().find(|c| c.re == *alpha && c.im == *beta).map_or(0, |c| c.mult);
            let mut fresh = |unknowns: &mut Vec<String>| {
                let name = format!("A_{{{}}}", unknowns.len() + 1);
                unknowns.push(name.clone());
                Sym::Var(name)
            };
            let poly_guess = |unknowns: &mut Vec<String>, fresh: &mut dyn FnMut(&mut Vec<String>) -> Sym| {
                add((0..=*deg).map(|k| mul(vec![fresh(unknowns), sym::pow_or_keep(xs.clone(), n(k as i64))])).collect())
            };
            let exp = pow(Sym::Const(Const::E), mul(vec![alpha.clone(), xs.clone()]))?;
            let lead = mul(vec![sym::pow_or_keep(xs.clone(), n(s as i64)), exp.clone()]);
            let inner = if beta.is_zero() {
                poly_guess(&mut unknowns, &mut fresh)
            } else {
                let angle = mul(vec![beta.clone(), xs.clone()]);
                let cos_part = mul(vec![poly_guess(&mut unknowns, &mut fresh), self.func(Func::Cos, angle.clone())?]);
                let sin_part = mul(vec![poly_guess(&mut unknowns, &mut fresh), self.func(Func::Sin, angle)?]);
                add(vec![cos_part, sin_part])
            };
            let yp = mul(vec![lead, inner]);
            guess.push(yp.clone());
            // L[yp] − g_group = 0, divided by e^{αx}: match the coefficient of each x^k cos/sin.
            let mut ly = Vec::new();
            let mut d = yp.clone();
            for (k, c) in a.iter().enumerate() {
                if k > 0 {
                    d = calculus::diff(&d, x, self.ev.angle)?;
                }
                ly.push(mul(vec![c.clone(), d.clone()]));
            }
            let residual = simp(mul(vec![add(vec![add(ly), neg(add(ts.clone()))]), pow(Sym::Const(Const::E), mul(vec![neg(alpha.clone()), xs.clone()]))?]));
            equations.extend(coefficient_equations(&residual, &unknowns));
        }
        let guess_sym = add(guess);
        steps.push(step(format!("y_p={}", latex(&guess_sym)), "Guess a particular solution shaped like the right side (times $x^s$ where it repeats part of $y_h$)"));
        if equations.is_empty() {
            return Ok(None);
        }
        let sol = solve::solve_system_for(self, equations, &unknowns)?;
        let Outcome::Sets(sets) = sol.outcome else { return Ok(None) };
        let Some(values) = sets.into_iter().next() else { return Ok(None) };
        let mut yp = guess_sym;
        let mut found = Vec::new();
        for (u, v) in unknowns.iter().zip(values) {
            let Root::Exact(v) = v else { return Ok(None) };
            found.push(format!("{u}={}", latex(&v)));
            yp = self.subst(&yp, u, &v)?;
        }
        let yp = simp(yp);
        steps.push(step(found.join(",\\ "), "Substitute into the equation and match the coefficients"));
        steps.push(step(format!("y_p={}", latex(&yp)), "The particular solution"));
        Ok(Some(yp))
    }

    /// y_p = −y₁∫y₂g/(aW) + y₂∫y₁g/(aW) for a second-order equation.
    fn variation_of_parameters(&mut self, a: &[Sym], g: &Sym, basis: &[Sym], x: &str, steps: &mut Vec<Step>) -> R<Option<Sym>> {
        let angle = self.ev.angle;
        let (y1, y2) = (&basis[0], &basis[1]);
        let w = simp(add(vec![mul(vec![y1.clone(), calculus::diff(y2, x, angle)?]), neg(mul(vec![calculus::diff(y1, x, angle)?, y2.clone()]))]));
        if w.is_zero() {
            return Ok(None);
        }
        steps.push(step(format!("W=y_1y_2'-y_1'y_2={}", latex(&w)), "The Wronskian"));
        let gn = div(g.clone(), mul(vec![a[2].clone(), w.clone()]))?;
        let Some((i1, _)) = calculus::integrate_steps(&simp(mul(vec![y2.clone(), gn.clone()])), x, angle) else { return Ok(None) };
        let Some((i2, _)) = calculus::integrate_steps(&simp(mul(vec![y1.clone(), gn])), x, angle) else { return Ok(None) };
        let yp = simp(add(vec![neg(mul(vec![y1.clone(), i1])), mul(vec![y2.clone(), i2])]));
        steps.push(step(format!("y_p=-y_1\\int\\frac{{y_2g}}{{W}}+y_2\\int\\frac{{y_1g}}{{W}}={}", latex(&yp)), "Variation of parameters"));
        Ok(Some(yp))
    }

    /// a₁y' + a₀y = g: y = (∫μq dx + C)/μ with μ = e^{∫p dx}.
    fn integrating_factor(&mut self, a: &[Sym], g: &Sym, y: &str, x: &str, steps: &mut Vec<Step>) -> R<Option<General>> {
        let angle = self.ev.angle;
        let p = simp(div(a[0].clone(), a[1].clone())?);
        let q = simp(div(g.clone(), a[1].clone())?);
        let py = mul(vec![p.clone(), Sym::Var(y.into())]);
        let shown = if py.is_zero() { format!("{y}'") } else { latex(&add(vec![Sym::Var(yk(y, 1)), py])) };
        steps.push(step(format!("{shown}={}", latex(&q)), "Divide by the coefficient of $y'$: the form $y'+p y=q$"));
        let Some((big_p, _)) = calculus::integrate_steps(&p, x, angle) else { return Ok(None) };
        let mu = simp(pow(Sym::Const(Const::E), big_p.clone())?);
        let mu = strip_abs(&mu);
        steps.push(step(format!("\\mu=e^{{\\int p\\,d{x}}}={}", latex(&mu)), "The integrating factor"));
        let integrand = simp(mul(vec![mu.clone(), q]));
        let Some((integral, _)) = calculus::integrate_steps(&integrand, x, angle) else { return Ok(None) };
        steps.push(step(format!("\\left(\\mu {y}\\right)'=\\mu q\\ \\Rightarrow\\ \\mu {y}={}+C", latex(&integral)), "Multiply through by $\\mu$: the left side is $(\\mu y)'$; integrate"));
        let solution = div(add(vec![integral, Sym::Var("C".into())]), mu)?;
        let solution = simp(solution);
        Ok(Some(General::Explicit(vec![solution], vec!["C".into()])))
    }

    /// y' = f(x)·g(y): ∫dy/g(y) = ∫f(x)dx + C, solved for y when possible.
    fn separable(&mut self, f: &Sym, y: &str, x: &str, steps: &mut Vec<Step>) -> R<Option<General>> {
        let yp = yk(y, 1);
        // y' alone: f = c·y' + rest with c free of y'.
        let Some((c, rest)) = split_linear(f, &yp) else { return Ok(None) };
        let rhs = simp(div(neg(rest), c)?);
        let Some((fx, gy)) = separate(&rhs, x, y)? else { return Ok(None) };
        let angle = self.ev.angle;
        let ys = Sym::Var(y.into());
        steps.push(step(format!("\\frac{{d{y}}}{{d{x}}}={}", latex(&mul(vec![fx.clone(), gy.clone()]))), "Separable: a function of $x$ times a function of $y$"));
        let inv = simp(div(n(1), gy.clone())?);
        let Some((big_g, _)) = calculus::integrate_steps(&inv, y, angle) else { return Ok(None) };
        let Some((big_f, _)) = calculus::integrate_steps(&fx, x, angle) else { return Ok(None) };
        let cst = Sym::Var("C".into());
        let right = add(vec![big_f, cst.clone()]);
        steps.push(step(format!("\\int {}\\,d{y}=\\int {}\\,d{x}\\ \\Rightarrow\\ {}={}", latex(&inv), latex(&fx), latex(&big_g), latex(&right)), "Separate and integrate both sides"));
        // Explicit if y can be isolated.
        let sol = solve::solve_for(self, &big_g, &right, y);
        if let Ok(s) = sol
            && let Outcome::Sets(sets) = s.outcome
        {
            let branches: Vec<Sym> = sets.into_iter().filter_map(|mut set| match set.pop() {
                Some(Root::Exact(v)) if free_of(&v, y) => Some(simp(v)),
                _ => None,
            }).collect();
            if !branches.is_empty() {
                return Ok(Some(General::Explicit(branches, vec!["C".into()])));
            }
        }
        let _ = ys;
        Ok(Some(General::Implicit(big_g, right)))
    }

    /// Fixes the constants from the initial conditions.
    fn apply_initials(&mut self, g: General, y: &str, x: &str, initials: &[Initial], steps: &mut Vec<Step>) -> R<OdeResult> {
        if initials.is_empty() {
            return Ok(match g {
                General::Explicit(b, _) => OdeResult::Explicit(b),
                General::Implicit(l, r) => OdeResult::Implicit(l, r),
            });
        }
        let angle = self.ev.angle;
        match g {
            General::Explicit(branches, names) => {
                let mut solved = Vec::new();
                for branch in branches {
                    let mut eqs = Vec::new();
                    for ic in initials {
                        let mut d = branch.clone();
                        for _ in 0..ic.order {
                            d = calculus::diff(&d, x, angle)?;
                        }
                        eqs.push((simp(self.subst(&d, x, &ic.at)?), ic.value.clone()));
                    }
                    let used: Vec<String> = names.iter().filter(|c| eqs.iter().any(|(l, _)| !free_of(l, c))).cloned().collect();
                    let shown = eqs.iter().map(|(l, r)| format!("{}={}", latex(l), latex(r))).collect::<Vec<_>>().join(",\\quad ");
                    let Ok(sol) = solve::solve_system_for(self, eqs, &used) else { continue };
                    let Outcome::Sets(sets) = sol.outcome else { continue };
                    let Some(values) = sets.into_iter().next() else { continue };
                    let mut s = branch.clone();
                    let mut found = Vec::new();
                    for (c, v) in used.iter().zip(values) {
                        let Root::Exact(v) = v else { continue };
                        found.push(format!("{c}={}", latex(&v)));
                        s = self.subst(&s, c, &v)?;
                    }
                    steps.push(step(format!("{shown}\\ \\Rightarrow\\ {}", found.join(",\\ ")), "Use the initial conditions to find the constants"));
                    solved.push(simp(s));
                }
                if solved.is_empty() {
                    return Err(EngineError::new("No solution matches those initial conditions"));
                }
                Ok(OdeResult::Explicit(solved))
            }
            General::Implicit(l, r) => {
                let ic = &initials[0];
                let l_at = self.subst(&l, x, &ic.at)?;
                let l0 = self.subst(&l_at, y, &ic.value)?;
                let r_at = self.subst(&r, x, &ic.at)?;
                let r0 = self.subst(&r_at, y, &ic.value)?;
                let sol = solve::solve_for(self, &l0, &r0, "C")?;
                let Outcome::Sets(sets) = sol.outcome else { return Err(EngineError::new("No solution matches that initial condition")) };
                let Some(Root::Exact(cv)) = sets.into_iter().next().and_then(|mut s| s.pop()) else {
                    return Err(EngineError::new("No solution matches that initial condition"));
                };
                steps.push(step(format!("{}={}\\ \\Rightarrow\\ C={}", latex(&l0), latex(&r0), latex(&cv)), "Use the initial condition to find $C$"));
                let r = self.subst(&r, "C", &cv)?;
                Ok(OdeResult::Implicit(l, simp(r)))
            }
        }
    }

    /// y⁽ⁿ⁾ = R(x, y, …) by RK4 from the initial conditions: the value at x₀ + 1 for the answer,
    /// and the right side for the graph.
    #[allow(clippy::too_many_arguments)]
    fn numeric_ode(&mut self, f: &Sym, y: &str, x: &str, order: u32, ys: &[String], initials: &[Initial], steps: &mut Vec<Step>) -> R<OdeResult> {
        let top = yk(y, order);
        let Some((c, rest)) = split_linear(f, &top) else {
            return Err(EngineError::new(format!("I can't write this as {top} = …, so it can't be solved numerically")));
        };
        let rhs = simp(div(neg(rest), c)?);
        let x0 = initials[0].at.clone();
        if initials.iter().any(|ic| ic.at != x0) {
            return Err(EngineError::new("The initial conditions must all be at the same point"));
        }
        let x0f = self.value_f64(&x0)?;
        let mut start = vec![0.0; order as usize];
        for ic in initials {
            start[ic.order as usize] = self.value_f64(&ic.value)?;
        }
        steps.push(step(format!("{top}={}", latex(&rhs)), "No exact method applies: step it forward numerically (RK4) from the initial conditions"));
        let at_one = rk4(self.ev, &rhs, x, ys, x0f, &start, x0f + 1.0, 1000).ok_or_else(|| EngineError::new("The numeric solution blew up before reaching the next unit"))?;
        Ok(OdeResult::Numeric { rhs, x0: x0f, start, at_one })
    }

    fn value_f64(&mut self, s: &Sym) -> R<f64> {
        let v = self.numeric(s)?;
        let fl = self.ev.prec.float_of(&v);
        crate::number::float_to_f64(&fl).ok_or_else(|| EngineError::new("Initial conditions need real numbers"))
    }
}

/// The general solution: explicit branches y = f(x, C…), or G(y) = F(x) + C.
enum General {
    Explicit(Vec<Sym>, Vec<String>),
    Implicit(Sym, Sym),
}

fn first_name(e: &Expr) -> Option<String> {
    match e {
        Expr::OdeFn { name, .. } => Some(name.clone()),
        Expr::Neg(a) | Expr::Call(_, a) | Expr::Factorial(a, _) | Expr::Percent(a) | Expr::Op(_, a) => first_name(a),
        Expr::Binary(_, a, b) | Expr::Pow(a, b) | Expr::Root(a, b) | Expr::LogBase(a, b) => first_name(a).or_else(|| first_name(b)),
        Expr::Deriv { body, .. } => first_name(body),
        _ => None,
    }
}

fn constant_names(k: usize) -> Vec<String> {
    if k == 1 { vec!["C".into()] } else { (1..=k).map(|i| format!("C_{{{i}}}")).collect() }
}

fn terms(s: &Sym) -> Vec<Sym> {
    match s {
        Sym::Add(v) => v.clone(),
        z if z.is_zero() => Vec::new(),
        other => vec![other.clone()],
    }
}


/// f = Σ aₖ·y⁽ᵏ⁾ − g with each aₖ free of y and its derivatives: (a, g).
fn linear_parts(f: &Sym, ys: &[String]) -> Option<(Vec<Sym>, Sym)> {
    let mut a = vec![Vec::new(); ys.len()];
    let mut rest = Vec::new();
    for t in terms(f) {
        let (c, m) = poly::monomial(&t);
        let mut hit = None;
        let mut coeff = vec![Sym::Num(c)];
        for (atom, p) in m {
            match &atom {
                Sym::Var(v) if ys.contains(v) => {
                    if p != 1 || hit.is_some() {
                        return None;
                    }
                    hit = ys.iter().position(|y| y == v);
                }
                _ => {
                    if ys.iter().any(|y| !free_of(&atom, y)) {
                        return None;
                    }
                    coeff.push(poly::pow_int(&atom, p));
                }
            }
        }
        match hit {
            Some(k) => a[k].push(mul(coeff)),
            None => rest.push(mul(coeff)),
        }
    }
    let a: Vec<Sym> = a.into_iter().map(|v| simp(add(v))).collect();
    Some((a, simp(neg(add(rest)))))
}

/// f = c·v + rest with c free of v and rest free of v.
fn split_linear(f: &Sym, v: &str) -> Option<(Sym, Sym)> {
    let (mut c, mut rest) = (Vec::new(), Vec::new());
    for t in terms(f) {
        if free_of(&t, v) {
            rest.push(t);
            continue;
        }
        let (k, m) = poly::monomial(&t);
        let mut coeff = vec![Sym::Num(k)];
        let mut seen = false;
        for (atom, p) in m {
            if atom == Sym::Var(v.into()) && p == 1 && !seen {
                seen = true;
            } else if free_of(&atom, v) {
                coeff.push(poly::pow_int(&atom, p));
            } else {
                return None;
            }
        }
        c.push(mul(coeff));
    }
    let c = simp(add(c));
    if c.is_zero() {
        return None;
    }
    Some((c, add(rest)))
}

/// rhs as f(x)·g(y), if it splits that way.
fn separate(rhs: &Sym, x: &str, y: &str) -> R<Option<(Sym, Sym)>> {
    if free_of(rhs, y) {
        return Ok(Some((rhs.clone(), n(1))));
    }
    if free_of(rhs, x) {
        return Ok(Some((n(1), rhs.clone())));
    }
    let (top, bottom) = crate::ratfunc::fraction(rhs)?;
    let mut fx = Vec::new();
    let mut gy = Vec::new();
    for (part, up) in [(top, true), (bottom, false)] {
        let factored = poly::factor(&part)?;
        let factors = match factored {
            Sym::Mul(v) => v,
            other => vec![other],
        };
        for fct in factors {
            let fct = if up { fct } else { pow(fct, n(-1))? };
            if free_of(&fct, y) {
                fx.push(fct);
            } else if free_of(&fct, x) {
                gy.push(fct);
            } else if let Some((a, b)) = split_exp(&fct, x, y) {
                // e^{x+y} = eˣ·eʸ
                fx.push(a);
                gy.push(b);
            } else {
                return Ok(None);
            }
        }
    }
    Ok(Some((mul(fx), mul(gy))))
}

/// e^{f(x) + g(y)} as (e^{f(x)}, e^{g(y)}).
fn split_exp(s: &Sym, x: &str, y: &str) -> Option<(Sym, Sym)> {
    let Sym::Pow(b, e) = s else { return None };
    if **b != Sym::Const(Const::E) {
        return None;
    }
    let (mut ex, mut ey) = (Vec::new(), Vec::new());
    for t in terms(&simp((**e).clone())) {
        if free_of(&t, y) {
            ex.push(t);
        } else if free_of(&t, x) {
            ey.push(t);
        } else {
            return None;
        }
    }
    Some((pow(Sym::Const(Const::E), add(ex)).ok()?, pow(Sym::Const(Const::E), add(ey)).ok()?))
}

/// A term of g as c·xᵈ·e^{αx}·(cos βx or sin βx): (α, β, d).
fn classify(t: &Sym, x: &str) -> Option<(Sym, Sym, u32)> {
    let factors = match t {
        Sym::Mul(v) => v.clone(),
        other => vec![other.clone()],
    };
    let (mut alpha, mut beta, mut deg) = (n(0), n(0), 0u32);
    let mut trig = false;
    for f in factors {
        match &f {
            _ if free_of(&f, x) => {}
            Sym::Var(v) if v == x => deg += 1,
            Sym::Pow(b, e) if matches!(&**b, Sym::Var(v) if v == x) => deg += num_traits::ToPrimitive::to_u32(&e.as_num()?.to_integer())?,
            Sym::Pow(b, e) if **b == Sym::Const(Const::E) => {
                let (c, rest) = sym::split_coeff((**e).clone());
                if rest != Sym::Var(x.into()) {
                    return None;
                }
                alpha = simp(add(vec![alpha, Sym::Num(c)]));
            }
            Sym::Func(Func::Sin | Func::Cos, arg) if !trig => {
                let (c, rest) = sym::split_coeff((**arg).clone());
                if rest != Sym::Var(x.into()) {
                    return None;
                }
                beta = Sym::Num(c.abs());
                trig = true;
            }
            _ => return None,
        }
    }
    Some((alpha, beta, deg))
}

/// The coefficients of each x^k·cos/sin monomial in `residual`, set to 0 (linear in the
/// unknowns).
fn coefficient_equations(residual: &Sym, unknowns: &[String]) -> Vec<(Sym, Sym)> {
    let mut by_key: Vec<(Sym, Vec<Sym>)> = Vec::new();
    for t in terms(residual) {
        let (c, m) = poly::monomial(&t);
        let mut basis = Vec::new();
        let mut part = vec![Sym::Num(c)];
        for (atom, p) in m {
            if matches!(&atom, Sym::Var(v) if unknowns.contains(v)) {
                part.push(poly::pow_int(&atom, p));
            } else {
                basis.push(poly::pow_int(&atom, p));
            }
        }
        let key = mul(basis);
        match by_key.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1.push(mul(part)),
            None => by_key.push((key, vec![mul(part)])),
        }
    }
    by_key.into_iter().map(|(_, v)| (simp(add(v)), n(0))).collect()
}

/// √s, simplified for squares of letters: √(4ω²) = 2ω.
fn sqrt_simple(s: &Sym) -> R<Sym> {
    let (c, rest) = sym::split_coeff(s.clone());
    let half_powers = match &rest {
        Sym::Pow(b, e) => e.as_num().filter(|k| k.is_integer() && k.to_integer() % 2 == 0.into()).map(|k| vec![pow((**b).clone(), Sym::Num(k / Q::from_integer(2.into())))]),
        Sym::Mul(fs) => fs
            .iter()
            .map(|f| match f {
                Sym::Pow(b, e) => e.as_num().filter(|k| k.is_integer() && k.to_integer() % 2 == 0.into()).map(|k| pow((**b).clone(), Sym::Num(k / Q::from_integer(2.into())))),
                _ => None,
            })
            .collect::<Option<Vec<_>>>(),
        _ => None,
    };
    match half_powers {
        Some(parts) => {
            let mut v = vec![pow(Sym::Num(c), sym::q(1, 2))?];
            for p in parts {
                v.push(p?);
            }
            Ok(mul(v))
        }
        None => pow(s.clone(), sym::q(1, 2)),
    }
}

/// e^{ln|x|} = |x| → x: an integrating factor's sign doesn't matter.
fn strip_abs(s: &Sym) -> Sym {
    match s {
        Sym::Func(Func::Abs, a) => (**a).clone(),
        Sym::Pow(b, e) => Sym::Pow(Box::new(strip_abs(b)), e.clone()),
        Sym::Mul(v) => mul(v.iter().map(strip_abs).collect()),
        other => other.clone(),
    }
}

/// RK4 for y⁽ⁿ⁾ = rhs(x, y, …, y⁽ⁿ⁻¹⁾) from x₀ to x₁ in `count` steps: the points (x, y)
/// along the way, stopping early where the solution blows up or leaves the domain.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rk4_path(ev: &mut Evaluator, rhs: &Sym, x: &str, ys: &[String], x0: f64, start: &[f64], x1: f64, count: usize) -> Vec<(f64, f64)> {
    let path = rk4_inner(ev, rhs, x, ys, x0, start, x1, count);
    // The sample values must not leak into later lines.
    for v in ys {
        ev.vars.remove(v);
    }
    ev.vars.remove(x);
    path
}

/// y(x₁) by RK4, or None if the solution doesn't get there.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rk4(ev: &mut Evaluator, rhs: &Sym, x: &str, ys: &[String], x0: f64, start: &[f64], x1: f64, count: usize) -> Option<f64> {
    let path = rk4_path(ev, rhs, x, ys, x0, start, x1, count);
    (path.len() == count + 1).then(|| path[count].1)
}

#[allow(clippy::too_many_arguments)]
fn rk4_inner(ev: &mut Evaluator, rhs: &Sym, x: &str, ys: &[String], x0: f64, start: &[f64], x1: f64, count: usize) -> Vec<(f64, f64)> {
    let expr = rhs.to_expr();
    let order = start.len();
    let h = (x1 - x0) / count as f64;
    let mut state = start.to_vec();
    let mut t = x0;
    let mut path = vec![(t, state[0])];
    let f = |ev: &mut Evaluator, t: f64, s: &[f64]| -> Option<Vec<f64>> {
        let p = ev.prec.bits;
        ev.vars.insert(x.to_string(), Value::Real(BigFloat::from_f64(t, p)));
        for (k, v) in s.iter().enumerate() {
            ev.vars.insert(ys[k].clone(), Value::Real(BigFloat::from_f64(*v, p)));
        }
        let v = ev.eval(&expr).ok()?;
        let top = crate::number::float_to_f64(&ev.prec.float_of(&v)).filter(|v| v.is_finite())?;
        let mut d: Vec<f64> = s[1..].to_vec();
        d.push(top);
        Some(d)
    };
    for _ in 0..count {
        let mut step = |ev: &mut Evaluator| -> Option<()> {
            let k1 = f(ev, t, &state)?;
            let s2: Vec<f64> = state.iter().zip(&k1).map(|(s, k)| s + h / 2.0 * k).collect();
            let k2 = f(ev, t + h / 2.0, &s2)?;
            let s3: Vec<f64> = state.iter().zip(&k2).map(|(s, k)| s + h / 2.0 * k).collect();
            let k3 = f(ev, t + h / 2.0, &s3)?;
            let s4: Vec<f64> = state.iter().zip(&k3).map(|(s, k)| s + h * k).collect();
            let k4 = f(ev, t + h, &s4)?;
            for i in 0..order {
                state[i] += h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
            }
            Some(())
        };
        if step(ev).is_none() {
            break;
        }
        t += h;
        if state.iter().any(|v| !v.is_finite() || v.abs() > 1e100) {
            break;
        }
        path.push((t, state[0]));
    }
    path
}

