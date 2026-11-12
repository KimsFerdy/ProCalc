//! Sampling the lines of a sheet for the graph.
//!
//! What a line draws:
//! - a function of one letter: an expression (`x^2`), `f(x) := …`, `y := …`, or `y = …`;
//! - a definite integral `\int_a^b f\,dx`: f with the area from a to b shaded;
//! - `f'(a)` or `\left.\frac{d}{dx}f\right|_{x=a}`: the tangent to f at a;
//! - an equation in two letters (`x^2+y^2=1`): an implicit curve, sampled on a grid;
//! - `x = f(t), y = g(t)`: a parametric curve; `r = f(\theta)`: a polar curve.
//!
//! Everything is sampled at 16 digits, far finer than any screen.

use std::collections::BTreeSet;

use astro_float::BigFloat;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::ast::{Expr, Stmt};
use crate::calculus::{free_of, free_vars};
use crate::defs::Env;
use crate::eval::Evaluator;
use crate::number::{self, Prec, Value};
use crate::sym::{self, Builder, Sym};
use crate::{AngleMode, Settings, parser};

/// Most samples along a curve.
pub const MAX_SAMPLES: usize = 4000;
/// Grid cells across an implicit curve, at most.
const MAX_GRID: usize = 240;
/// Points along a parametric or polar curve.
const PATH_POINTS: usize = 900;

/// The part of the plane being drawn, and its size on screen in pixels.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Curve {
    /// What is plotted, as LaTeX: `x^{2}`, `f\left(x\right)`, `y`, `x^{2}+y^{2}=1`.
    pub label: String,
    #[serde(flatten)]
    pub kind: CurveKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CurveKind {
    /// y = f(x): one value per sample, evenly spaced across the view; null where undefined.
    /// `shade` marks the interval of a definite integral.
    Function { ys: Vec<Option<f64>>, shade: Option<(f64, f64)> },
    /// Parametric or polar: points in order; null breaks the line.
    Path { points: Vec<Option<(f64, f64)>> },
    /// F(x, y) on a grid of (nx+1)×(ny+1) corners across the view, row by row from the
    /// bottom; the curve is where it changes sign. For an inequality (`region` set, true when
    /// strict), F is positive where it holds and that area is shaded.
    Implicit { nx: usize, ny: usize, values: Vec<Option<f64>>, region: Option<bool> },
    /// z = f(x, y) on an (nx+1)×(ny+1) grid over [x0, x1]×[y0, y1], row by row from y0.
    Surface { nx: usize, ny: usize, x: (f64, f64), y: (f64, f64), zs: Vec<Option<f64>>, axes: (String, String, String) },
    /// The tangent line through (x0, y0) with this slope.
    Tangent { x0: f64, y0: f64, slope: f64 },
}

/// One entry per line: what that line draws, if anything.
pub fn plot_sheet(lines: &[String], settings: &Settings, view: View) -> Vec<Option<Curve>> {
    let mut ev = crate::evaluator(settings);
    let mut sampler = Evaluator {
        prec: Prec::new(16),
        angle: settings.angle,
        calls: 0,
        vars: Default::default(),
        work: Vec::new(),
    };
    let mut env = Env::default();
    lines
        .iter()
        .map(|line| {
            ev.calls = 0;
            let plot = plottable(&mut env, line, &mut ev)?;
            draw(plot, &view, &mut sampler)
        })
        .collect()
}

/// What a line asks to be drawn, before sampling.
enum Plot {
    /// `domain` limits x (from a condition like −1 ≤ x ≤ 1).
    Function { f: Sym, var: String, label: String, shade: Option<(f64, f64)>, domain: Option<(f64, f64)> },
    Tangent { f: Sym, var: String, at: Sym, label: String },
    Implicit { f: Sym, x: String, y: String, label: String },
    /// Inequalities: each slack is positive where its comparison holds.
    Region { slacks: Vec<Sym>, x: String, y: String, strict: bool, label: String },
    Parametric { fx: Sym, fy: Sym, t: String, label: String, range: Option<(f64, f64)> },
    Polar { r: Sym, theta: String, label: String, range: Option<(f64, f64)> },
    Surface { f: Sym, x: String, y: String, z: String, label: String, xr: Option<(f64, f64)>, yr: Option<(f64, f64)> },
    /// A differential equation's general solution with one free constant: a few members.
    Family { f: Sym, var: String, constant: String, label: String },
    /// A differential equation solved numerically from its initial conditions.
    OdeNumeric { rhs: Sym, x: String, ys: Vec<String>, x0: f64, start: Vec<f64>, label: String },
}

/// Numeric bounds per letter, from conditions like `0 \le t \le 4\pi` or `x > -1`.
type Ranges = Vec<(String, f64, f64)>;

fn ranges_of(b: &mut Builder, chains: &[crate::ast::Chain]) -> Ranges {
    let mut out: Ranges = Vec::new();
    for c in chains {
        let parts: Vec<Option<Sym>> = c.parts.iter().map(|p| b.build(p).ok()).collect();
        for (k, rel) in c.rels.iter().enumerate() {
            let (Some(l), Some(r)) = (&parts[k], &parts[k + 1]) else { continue };
            // var < number or number < var.
            let (var, num, var_left) = match (l, r) {
                (Sym::Var(v), other) => (v.clone(), real(b, other), true),
                (other, Sym::Var(v)) => (v.clone(), real(b, other), false),
                _ => continue,
            };
            let Some(num) = num else { continue };
            let upper = matches!(rel, crate::ast::Rel::Lt | crate::ast::Rel::Le) == var_left;
            let entry = match out.iter_mut().find(|(v, ..)| *v == var) {
                Some(e) => e,
                None => {
                    out.push((var, f64::NEG_INFINITY, f64::INFINITY));
                    out.last_mut().expect("just pushed")
                }
            };
            if upper {
                entry.2 = entry.2.min(num);
            } else {
                entry.1 = entry.1.max(num);
            }
        }
    }
    out
}

fn range_for(ranges: &Ranges, var: &str) -> Option<(f64, f64)> {
    ranges.iter().find(|(v, ..)| v == var).map(|(_, a, c)| (*a, *c))
}

/// Both ends finite.
fn bounded(r: Option<(f64, f64)>) -> Option<(f64, f64)> {
    r.filter(|(a, c)| a.is_finite() && c.is_finite() && a < c)
}

fn letters(s: &Sym) -> BTreeSet<String> {
    let mut v = BTreeSet::new();
    free_vars(s, &mut v);
    v
}

fn one_letter(s: &Sym) -> Option<String> {
    let v = letters(s);
    (v.len() == 1).then(|| v.into_iter().next().expect("one letter"))
}

fn tidy(s: Sym) -> Option<Sym> {
    let s = sym::normalize_complex(s);
    (!crate::units::contains_unit(&s)).then_some(s)
}

/// The plot a line describes, keeping its definitions in `env` either way.
fn plottable(env: &mut Env, line: &str, ev: &mut Evaluator) -> Option<Plot> {
    let lx = crate::latex::latex;
    match parser::parse_statement(line, &env.func_names()).ok()? {
        Stmt::Expr(expr) => {
            let mut b = Builder { ev: &mut *ev, env };
            match &expr {
                // ∫_a^b f dx: f, with the area shaded.
                Expr::Integral { body, var, bounds: Some((lo, hi)) } => {
                    let f = tidy(b.build_free(body, std::slice::from_ref(var)).ok()?)?;
                    let a = b.build(lo).ok()?;
                    let c = b.build(hi).ok()?;
                    let (a, c) = (real(&mut b, &a)?, real(&mut b, &c)?);
                    let only_x = letters(&f).iter().all(|v| v == var);
                    only_x.then(|| Plot::Function { label: lx(&f), f, var: var.clone(), shade: Some((a, c)), domain: None })
                }
                // f'(a): the tangent to f at a.
                Expr::UserDeriv { name, order: 1, args } if args.len() == 1 => {
                    let def = env.funcs.get(name)?.clone();
                    let [param] = def.params.as_slice() else { return None };
                    let mut scope = (*def.scope).clone();
                    scope.vars.remove(param);
                    let f = tidy(Builder { ev: &mut *ev, env: &scope }.build(&def.body).ok()?)?;
                    let at = Builder { ev: &mut *ev, env }.build(&args[0]).ok()?;
                    let label = format!("\\text{{tangent at }}{}", lx(&at));
                    Some(Plot::Tangent { f, var: param.clone(), at, label })
                }
                // (d/dx f)|_{x=a}: the tangent to f at a.
                Expr::Subst(inner, binds) if binds.len() == 1 => {
                    let Expr::Deriv { body, var, order: 1 } = &**inner else { return function_of(b.build(&expr).ok()?) };
                    if binds[0].0 != *var {
                        return None;
                    }
                    let f = tidy(b.build_free(body, std::slice::from_ref(var)).ok()?)?;
                    let at = b.build(&binds[0].1).ok()?;
                    let label = format!("\\text{{tangent at }}{}", lx(&at));
                    Some(Plot::Tangent { f, var: var.clone(), at, label })
                }
                _ => function_of(b.build(&expr).ok()?),
            }
        }
        Stmt::Define { name, params, body } => {
            let shown = tidy(env.define(name.clone(), params.clone(), body, ev).ok()?)?;
            let label = match params.as_slice() {
                [] => crate::ast::name_latex(&name),
                ps => format!("{}\\left({}\\right)", crate::ast::name_latex(&name), ps.join(",")),
            };
            function_or_surface(shown, label, &Vec::new())
        }
        Stmt::Equations(eqs, conds) => {
            let mut b = Builder { ev: &mut *ev, env };
            if eqs.iter().any(|(l, r)| crate::ode::mentions_ode(l) || crate::ode::mentions_ode(r)) {
                let sol = b.solve_ode(&eqs).ok()?;
                return ode_plot(sol);
            }
            let mut sides = Vec::new();
            for (l, r) in &eqs {
                let l = tidy(b.build(l).ok()?)?;
                sides.push((l, tidy(b.build(r).ok()?)?));
            }
            let ranges = ranges_of(&mut b, &conds);
            equation_plot(&sides, &ranges)
        }
        Stmt::Inequalities(chains) => {
            let mut b = Builder { ev: &mut *ev, env };
            let mut slacks = Vec::new();
            let mut strict = true;
            let mut shown = Vec::new();
            for c in &chains {
                let parts: Vec<Sym> = c.parts.iter().map(|p| b.build(p).ok().and_then(tidy)).collect::<Option<_>>()?;
                let mut s = lx(&parts[0]);
                for (k, rel) in c.rels.iter().enumerate() {
                    let (l, r) = (&parts[k], &parts[k + 1]);
                    // Positive where it holds.
                    let slack = match rel {
                        crate::ast::Rel::Lt | crate::ast::Rel::Le => sym::add(vec![r.clone(), sym::neg(l.clone())]),
                        _ => sym::add(vec![l.clone(), sym::neg(r.clone())]),
                    };
                    slacks.push(tidy(slack)?);
                    strict &= rel.strict();
                    s.push_str(rel.latex());
                    s.push_str(&lx(r));
                }
                shown.push(s);
            }
            let mut all = BTreeSet::new();
            for s in &slacks {
                free_vars(s, &mut all);
            }
            let vars: Vec<String> = all.into_iter().collect();
            // x across and y up; a single other letter runs across.
            let (x, y) = match vars.as_slice() {
                [v] if v == "y" => ("x".to_string(), v.clone()),
                [v] => (v.clone(), "y".to_string()),
                [a, c] if a == "y" && c == "x" => (c.clone(), a.clone()),
                [a, c] => (a.clone(), c.clone()),
                _ => return None,
            };
            Some(Plot::Region { slacks, x, y, strict, label: shown.join(",\\ ") })
        }
    }
}

fn function_of(s: Sym) -> Option<Plot> {
    let f = tidy(s)?;
    let label = crate::latex::latex(&f);
    function_or_surface(f, label, &Vec::new())
}

/// One letter: y = f(x). Two letters: a surface z = f(x, y).
fn function_or_surface(f: Sym, label: String, ranges: &Ranges) -> Option<Plot> {
    // A matrix of functions isn't one curve.
    if matches!(f, Sym::Matrix(_)) {
        return None;
    }
    let vars: Vec<String> = letters(&f).into_iter().collect();
    match vars.as_slice() {
        [v] => {
            let domain = range_for(ranges, v);
            Some(Plot::Function { f, var: v.clone(), label, shade: None, domain })
        }
        [a, c] => {
            let (x, y) = if a == "y" && c == "x" { (c.clone(), a.clone()) } else { (a.clone(), c.clone()) };
            let (xr, yr) = (bounded(range_for(ranges, &x)), bounded(range_for(ranges, &y)));
            Some(Plot::Surface { f, x, y, z: "z".into(), label, xr, yr })
        }
        _ => None,
    }
}

/// A real number for a variable-free expression.
fn real(b: &mut Builder, s: &Sym) -> Option<f64> {
    if s.has_vars() {
        return None;
    }
    to_f64(&b.numeric(s).ok()?)
}

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Exact(r) => r.to_f64(),
        Value::Real(f) => number::float_to_f64(f),
        Value::Complex(..) => None,
    }
    .filter(|y| y.is_finite())
}

fn is_angle_letter(v: &str) -> bool {
    matches!(v, "\\theta" | "\\varphi" | "\\phi" | "\\vartheta")
}

/// y = f(x), r = f(θ), z = f(x, y), x = f(t) with y = g(t), or an implicit curve in two
/// letters; conditions give ranges for the letters.
/// A differential equation's solution: the curve when the constants are fixed, a family for
/// one free constant (C = −2 … 2), or the numeric solution from the initial conditions.
fn ode_plot(sol: crate::ode::Ode) -> Option<Plot> {
    use crate::ode::OdeResult;
    let label = sol.y.clone();
    let only_x = |s: &Sym| letters(s).iter().all(|v| *v == sol.x);
    match sol.result {
        OdeResult::Explicit(branches) => {
            let f = branches.into_iter().next()?;
            if only_x(&f) {
                return Some(Plot::Function { f, var: sol.x, label, shade: None, domain: None });
            }
            let mut free = letters(&f);
            free.remove(&sol.x);
            let Ok([c]) = <[String; 1]>::try_from(free.into_iter().collect::<Vec<_>>()) else { return None };
            Some(Plot::Family { f, var: sol.x, constant: c, label })
        }
        OdeResult::Implicit(l, r) => {
            let f = sym::add(vec![l, sym::neg(r)]);
            let vars = letters(&f);
            (vars.len() <= 2 && vars.iter().all(|v| *v == sol.x || *v == sol.y)).then_some(Plot::Implicit { f, x: sol.x, y: sol.y, label })
        }
        OdeResult::Numeric { rhs, x0, start, .. } => {
            let ys = (0..sol.order).map(|k| format!("{}{}", sol.y, "'".repeat(k as usize))).collect();
            Some(Plot::OdeNumeric { rhs, x: sol.x, ys, x0, start, label })
        }
    }
}

fn equation_plot(sides: &[(Sym, Sym)], ranges: &Ranges) -> Option<Plot> {
    let lx = crate::latex::latex;
    match sides {
        [(l, r)] => {
            // r = f(θ) is polar, including a constant radius (r = 2, a circle).
            for (lhs, rhs) in [(l, r), (r, l)] {
                if *lhs == Sym::Var("r".into()) {
                    let vars = letters(rhs);
                    if vars.len() <= 1 && vars.iter().all(|v| is_angle_letter(v)) {
                        let theta = vars.into_iter().next().unwrap_or_else(|| "\\theta".into());
                        let range = bounded(range_for(ranges, &theta));
                        return Some(Plot::Polar { r: rhs.clone(), theta, label: format!("r={}", lx(rhs)), range });
                    }
                }
            }
            for (lhs, rhs) in [(l, r), (r, l)] {
                if let Sym::Var(name) = lhs
                    && free_of(rhs, name)
                {
                    let vars = letters(rhs);
                    // y = f(x): any single letter on the left over one on the right.
                    if vars.len() == 1 {
                        let v = vars.into_iter().next().expect("one letter");
                        let domain = range_for(ranges, &v);
                        return Some(Plot::Function { f: rhs.clone(), var: v, label: name.clone(), shade: None, domain });
                    }
                    // z = f(x, y): a surface.
                    if vars.len() == 2 {
                        let Some(Plot::Surface { f, x, y, xr, yr, .. }) = function_or_surface(rhs.clone(), String::new(), ranges) else {
                            return None;
                        };
                        let label = format!("{name}={}", lx(rhs));
                        return Some(Plot::Surface { f, x, y, z: name.clone(), label, xr, yr });
                    }
                }
            }
            let f = tidy(sym::add(vec![l.clone(), sym::neg(r.clone())]))?;
            let vars: Vec<String> = letters(&f).into_iter().collect();
            let [a, c] = vars.as_slice() else { return None };
            // x across, y up; otherwise alphabetical.
            let (x, y) = if c == "x" && a == "y" { (c.clone(), a.clone()) } else { (a.clone(), c.clone()) };
            Some(Plot::Implicit { f, x, y, label: format!("{}={}", lx(l), lx(r)) })
        }
        [(l1, r1), (l2, r2)] => {
            let (Sym::Var(a), Sym::Var(c)) = (l1, l2) else { return None };
            let t = one_letter(r1)?;
            if one_letter(r2)? != t || a == c {
                return None;
            }
            let (fx, fy) = if c == "x" { (r2.clone(), r1.clone()) } else { (r1.clone(), r2.clone()) };
            let label = format!("\\left({},\\ {}\\right)", lx(&fx), lx(&fy));
            let range = bounded(range_for(ranges, &t));
            Some(Plot::Parametric { fx, fy, t, label, range })
        }
        _ => None,
    }
}

fn eval_at(f: &crate::ast::Expr, var: &str, x: f64, ev: &mut Evaluator) -> Option<f64> {
    ev.vars.insert(var.to_string(), Value::Real(BigFloat::from_f64(x, ev.prec.bits)));
    ev.calls = 0;
    to_f64(&ev.eval(f).ok()?)
}

fn has_trig(s: &Sym) -> bool {
    match s {
        Sym::Func(f, a) => {
            use crate::ast::Func::*;
            matches!(f, Sin | Cos | Tan) || has_trig(a)
        }
        Sym::Add(v) | Sym::Mul(v) => v.iter().any(has_trig),
        Sym::Pow(a, b) | Sym::Log(a, b) => has_trig(a) || has_trig(b),
        Sym::Factorial(a, _) => has_trig(a),
        _ => false,
    }
}

/// Surface grid: cells per side.
const SURFACE_GRID: usize = 48;
/// Default square a surface is drawn over when no condition gives one.
const SURFACE_SPAN: f64 = 5.0;

/// F(x, y) at the corners of a grid over the view (cells of about 4 px).
fn grid(view: &View, x: &str, y: &str, ev: &mut Evaluator, f: &mut dyn FnMut(&mut Evaluator) -> Option<f64>) -> (usize, usize, Vec<Option<f64>>) {
    let nx = ((view.width / 4.0).round() as usize).clamp(20, MAX_GRID);
    let ny = ((view.height / 4.0).round() as usize).clamp(15, MAX_GRID);
    let bits = ev.prec.bits;
    let mut values = Vec::with_capacity((nx + 1) * (ny + 1));
    for j in 0..=ny {
        let yv = view.y_min + (view.y_max - view.y_min) * j as f64 / ny as f64;
        ev.vars.insert(y.to_string(), Value::Real(BigFloat::from_f64(yv, bits)));
        for i in 0..=nx {
            let xv = view.x_min + (view.x_max - view.x_min) * i as f64 / nx as f64;
            ev.vars.insert(x.to_string(), Value::Real(BigFloat::from_f64(xv, bits)));
            ev.calls = 0;
            values.push(f(ev));
        }
    }
    (nx, ny, values)
}

fn draw(plot: Plot, view: &View, ev: &mut Evaluator) -> Option<Curve> {
    let full_turn = match ev.angle {
        AngleMode::Radians => std::f64::consts::TAU,
        AngleMode::Degrees => 360.0,
    };
    match plot {
        Plot::Function { f, var, label, shade, domain } => {
            let n = (view.width.round() as usize).clamp(2, MAX_SAMPLES);
            let expr = f.to_expr();
            let (lo, hi) = domain.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
            let ys = (0..n)
                .map(|k| {
                    let x = view.x_min + (view.x_max - view.x_min) * k as f64 / (n - 1) as f64;
                    // Outside a condition's range: no curve.
                    if x < lo || x > hi { None } else { eval_at(&expr, &var, x, ev) }
                })
                .collect();
            Some(Curve { label, kind: CurveKind::Function { ys, shade } })
        }
        Plot::Family { f, var, constant, label } => {
            let n = (view.width.round() as usize).clamp(2, MAX_SAMPLES) / 2;
            let mut points = Vec::new();
            for c in -2..=2 {
                let member = Builder { ev, env: &crate::defs::Env::default() }.subst(&f, &constant, &sym::n(c)).ok()?;
                let expr = member.to_expr();
                for k in 0..n {
                    let x = view.x_min + (view.x_max - view.x_min) * k as f64 / (n - 1) as f64;
                    points.push(eval_at(&expr, &var, x, ev).map(|y| (x, y)));
                }
                // A break between members.
                points.push(None);
            }
            Some(Curve { label, kind: CurveKind::Path { points } })
        }
        Plot::OdeNumeric { rhs, x, ys, x0, start, label } => {
            let count = (view.width.round() as usize).clamp(50, MAX_SAMPLES);
            let h = (view.x_max - view.x_min) / count as f64;
            // Out to each side of the view from x₀.
            let steps = |to: f64| (((to - x0) / h).abs().ceil() as usize).max(1);
            let back = crate::ode::rk4_path(ev, &rhs, &x, &ys, x0, &start, view.x_min.min(x0), steps(view.x_min));
            let fwd = crate::ode::rk4_path(ev, &rhs, &x, &ys, x0, &start, view.x_max.max(x0), steps(view.x_max));
            let points = back.into_iter().rev().chain(fwd.into_iter().skip(1)).map(Some).collect();
            Some(Curve { label, kind: CurveKind::Path { points } })
        }
        Plot::Tangent { f, var, at, label } => {
            let x0 = {
                let e = at.to_expr();
                to_f64(&ev.eval(&e).ok()?)?
            };
            let y0 = eval_at(&f.to_expr(), &var, x0, ev)?;
            let df = crate::calculus::diff(&f, &var, ev.angle).ok()?;
            let slope = eval_at(&df.to_expr(), &var, x0, ev)?;
            Some(Curve { label, kind: CurveKind::Tangent { x0, y0, slope } })
        }
        Plot::Implicit { f, x, y, label } => {
            let expr = f.to_expr();
            let (nx, ny, values) = grid(view, &x, &y, ev, &mut |ev| to_f64(&ev.eval(&expr).ok()?));
            Some(Curve { label, kind: CurveKind::Implicit { nx, ny, values, region: None } })
        }
        Plot::Region { slacks, x, y, strict, label } => {
            // All comparisons hold where the smallest slack is positive.
            let exprs: Vec<_> = slacks.iter().map(Sym::to_expr).collect();
            let (nx, ny, values) = grid(view, &x, &y, ev, &mut |ev| {
                exprs.iter().map(|e| to_f64(&ev.eval(e).ok()?)).collect::<Option<Vec<f64>>>().map(|v| v.into_iter().fold(f64::INFINITY, f64::min))
            });
            Some(Curve { label, kind: CurveKind::Implicit { nx, ny, values, region: Some(strict) } })
        }
        Plot::Parametric { fx, fy, t, label, range } => {
            // Periodic-looking curves go once round; others run over −10…10.
            let default = if has_trig(&fx) || has_trig(&fy) { (0.0, full_turn) } else { (-10.0, 10.0) };
            let (t0, t1) = range.unwrap_or(default);
            let (ex, ey) = (fx.to_expr(), fy.to_expr());
            let points = (0..=PATH_POINTS)
                .map(|k| {
                    let tv = t0 + (t1 - t0) * k as f64 / PATH_POINTS as f64;
                    Some((eval_at(&ex, &t, tv, ev)?, eval_at(&ey, &t, tv, ev)?))
                })
                .collect();
            Some(Curve { label, kind: CurveKind::Path { points } })
        }
        Plot::Polar { r, theta, label, range } => {
            let er = r.to_expr();
            let to_rad = std::f64::consts::TAU / full_turn;
            let (t0, t1) = range.unwrap_or((0.0, full_turn));
            let points = (0..=PATH_POINTS)
                .map(|k| {
                    let th = t0 + (t1 - t0) * k as f64 / PATH_POINTS as f64;
                    let rv = eval_at(&er, &theta, th, ev)?;
                    Some((rv * (th * to_rad).cos(), rv * (th * to_rad).sin()))
                })
                .collect();
            Some(Curve { label, kind: CurveKind::Path { points } })
        }
        Plot::Surface { f, x, y, z, label, xr, yr } => {
            let (x0, x1) = xr.unwrap_or((-SURFACE_SPAN, SURFACE_SPAN));
            let (y0, y1) = yr.unwrap_or((-SURFACE_SPAN, SURFACE_SPAN));
            let n = SURFACE_GRID;
            let expr = f.to_expr();
            let bits = ev.prec.bits;
            let mut zs = Vec::with_capacity((n + 1) * (n + 1));
            for j in 0..=n {
                let yv = y0 + (y1 - y0) * j as f64 / n as f64;
                ev.vars.insert(y.clone(), Value::Real(BigFloat::from_f64(yv, bits)));
                for i in 0..=n {
                    zs.push(eval_at(&expr, &x, x0 + (x1 - x0) * i as f64 / n as f64, ev));
                }
            }
            Some(Curve { label, kind: CurveKind::Surface { nx: n, ny: n, x: (x0, x1), y: (y0, y1), zs, axes: (x, y, z) } })
        }
    }
}
