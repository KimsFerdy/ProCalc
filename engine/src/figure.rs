//! The geometry workspace's solver: a sketched figure plus the measurements given for it, in;
//! the figure to scale, every part (given, found or unknown) and its quantities with formulas
//! and working, out.
//!
//! Triangles, rectangles, squares and circles are solved exactly. Any other polygon is solved
//! numerically, starting from the sketch, so it keeps the drawn orientation and, where two
//! shapes fit, the one closer to the drawing. Angles are in degrees.

// Corners, sides and angles are matched up by index across several arrays.
#![allow(clippy::needless_range_loop)]

use serde::{Deserialize, Serialize};

use crate::ast::Func;
use crate::sym::{self, Builder, Sym, add, div, mul, n, pow};
use crate::{AngleMode, Settings, Step};

/// A figure as drawn: the corners in order (a circle: its centre and a point on it), and what
/// was typed for each side (corner i to corner i+1) and each angle.
#[derive(Debug, Clone, Deserialize)]
pub struct FigureInput {
    /// "triangle", "rectangle", "square", "polygon" or "circle".
    pub kind: String,
    #[serde(default)]
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub sides: Vec<Option<String>>,
    #[serde(default)]
    pub angles: Vec<Option<String>>,
    /// For a circle: which measurement was given ("r", "d", "C" or "A") and its value.
    #[serde(default)]
    pub circle: Option<CircleGiven>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CircleGiven {
    pub which: String,
    pub value: String,
}

/// A side or an angle and what is known about it.
#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub name: String,
    /// "given", "found", "fixed" (a rectangle's right angles) or "unknown".
    pub status: &'static str,
    pub latex: Option<String>,
    pub text: Option<String>,
    pub value: Option<f64>,
}

/// Something that can be asked for: area, perimeter, …
#[derive(Debug, Clone, Serialize)]
pub struct Quantity {
    pub id: &'static str,
    pub name: &'static str,
    /// 1 for a length, 2 for an area, 0 for an angle or a count.
    pub dim: u8,
    pub latex: String,
    pub text: String,
    pub value: f64,
    /// The formula, in LaTeX.
    pub formula: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FigureResult {
    /// "solved", "missing" (not enough given), "conflict" (the values don't fit) or "error".
    pub status: &'static str,
    pub message: String,
    /// The corners to scale when solved (the sketch otherwise), in the length unit.
    pub points: Vec<[f64; 2]>,
    pub sides: Vec<Part>,
    pub angles: Vec<Part>,
    pub quantities: Vec<Quantity>,
    /// A circle's radius to scale.
    pub radius: Option<f64>,
}

type R<T> = Result<T, String>;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn simp(s: Sym) -> Sym {
    sym::normalize_complex(crate::poly::expand(&s).unwrap_or(s))
}

/// Corner names: A, B, C, …, Z, then A₁…
fn corner(i: usize) -> String {
    let letter = (b'A' + (i % 26) as u8) as char;
    if i < 26 { letter.to_string() } else { format!("{letter}_{{{}}}", i / 26) }
}

fn side_name(i: usize, count: usize) -> String {
    format!("{}{}", corner(i), corner((i + 1) % count))
}

/// A number with about ten significant digits, for the numeric parts.
fn fmt(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let digits = (10 - v.abs().log10().floor() as i32 - 1).clamp(0, 12) as usize;
    let s = format!("{v:.digits$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

struct Ctx<'a> {
    settings: Settings,
    b: Builder<'a>,
}

impl Ctx<'_> {
    /// A typed value: a positive number (an angle in degrees unless it says otherwise).
    fn given(&mut self, typed: &Option<String>, angle: bool, what: &str) -> R<Option<Sym>> {
        let Some(t) = typed.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) else { return Ok(None) };
        let e = crate::parser::parse(t).map_err(|e| format!("{what}: {}", e.message))?;
        let v = if angle { self.b.angle_value(&e) } else { self.b.build(&e) }.map_err(|e| format!("{what}: {}", e.message))?;
        if v.has_vars() {
            return Err(format!("{what} needs a number"));
        }
        if self.b.sign_of(&v).map_err(|e| e.message)? <= 0 {
            return Err(format!("{what} must be positive"));
        }
        Ok(Some(simp(v)))
    }

    /// (LaTeX, text, value) for a found or given value.
    fn show(&mut self, s: &Sym) -> R<(String, String, f64)> {
        let a = crate::value_answer(s, &self.settings, self.b.ev, None).map_err(|e| e.message)?;
        let d = a.decimal.ok_or("A value couldn't be worked out")?;
        let v = d.text.parse::<f64>().unwrap_or(f64::NAN);
        Ok((a.exact_latex.unwrap_or_else(|| d.latex.clone()), d.text, v))
    }

    fn part(&mut self, name: String, status: &'static str, s: Option<&Sym>) -> R<Part> {
        Ok(match s {
            Some(s) => {
                let (latex, text, value) = self.show(s)?;
                Part { name, status, latex: Some(latex), text: Some(text), value: Some(value) }
            }
            None => Part { name, status: "unknown", latex: None, text: None, value: None },
        })
    }

    fn quantity(&mut self, id: &'static str, name: &'static str, dim: u8, s: &Sym, formula: &str, steps: Vec<Step>) -> R<Quantity> {
        let s = simp(s.clone());
        let (latex, text, value) = self.show(&s)?;
        Ok(Quantity { id, name, dim, latex, text, value, formula: formula.into(), steps })
    }

    fn f64_of(&mut self, s: &Sym) -> R<f64> {
        Ok(self.show(s)?.2)
    }
}

/// Solves a figure. Errors in the typed values come back as an "error" result, not a failure.
pub fn solve_figure(input: &FigureInput, settings: &Settings) -> FigureResult {
    let mut st = settings.clone();
    // Geometry is in degrees.
    st.angle = AngleMode::Degrees;
    st.digits = settings.digits.clamp(6, 32);
    let mut ev = crate::evaluator(&st);
    let env = crate::defs::Env::default();
    let mut c = Ctx { settings: st, b: Builder { ev: &mut ev, env: &env } };
    let sketch = input.points.clone();
    let result = match input.kind.as_str() {
        "circle" => circle(&mut c, input),
        "rectangle" | "square" => rectangle(&mut c, input),
        "triangle" => triangle(&mut c, input),
        "polygon" if input.points.len() == 3 => triangle(&mut c, input),
        "polygon" => polygon(&mut c, input),
        other => Err(format!("Unknown figure: {other}")),
    };
    result.unwrap_or_else(|message| FigureResult {
        status: "error",
        message,
        points: sketch,
        sides: Vec::new(),
        angles: Vec::new(),
        quantities: Vec::new(),
        radius: None,
    })
}

/// +1 when the corners go anticlockwise, −1 clockwise.
fn orientation(p: &[[f64; 2]]) -> f64 {
    let k = p.len();
    let twice: f64 = (0..k).map(|i| p[i][0] * p[(i + 1) % k][1] - p[(i + 1) % k][0] * p[i][1]).sum();
    if twice >= 0.0 { 1.0 } else { -1.0 }
}

fn circle(c: &mut Ctx, input: &FigureInput) -> R<FigureResult> {
    let centre = *input.points.first().ok_or("A circle needs its centre")?;
    let sketch_r = input.points.get(1).map(|p| ((p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2)).sqrt()).unwrap_or(1.0);
    let Some(g) = &input.circle else {
        return Ok(FigureResult {
            status: "missing",
            message: "Give the radius, the diameter, the circumference or the area".into(),
            points: vec![centre],
            sides: Vec::new(),
            angles: Vec::new(),
            quantities: Vec::new(),
            radius: Some(sketch_r),
        });
    };
    let v = c.given(&Some(g.value.clone()), false, "The measurement")?.ok_or("Give the radius, the diameter, the circumference or the area")?;
    let pi = || Sym::Const(crate::ast::Const::Pi);
    let (r, how) = match g.which.as_str() {
        "r" => (v.clone(), "The radius is given".to_string()),
        "d" => (div(v.clone(), n(2)).map_err(|e| e.message)?, "$r=\\frac{d}{2}$".into()),
        "C" => (div(v.clone(), mul(vec![n(2), pi()])).map_err(|e| e.message)?, "$r=\\frac{C}{2\\pi}$".into()),
        "A" => (pow(div(v.clone(), pi()).map_err(|e| e.message)?, sym::q(1, 2)).map_err(|e| e.message)?, "$r=\\sqrt{\\frac{A}{\\pi}}$".into()),
        other => return Err(format!("A circle's measurement is r, d, C or A, not {other}")),
    };
    let r = simp(r);
    let rl = c.show(&r)?.0;
    let first = step(format!("r={rl}"), how);
    let d = mul(vec![n(2), r.clone()]);
    let circ = mul(vec![n(2), pi(), r.clone()]);
    let area = mul(vec![pi(), pow(r.clone(), n(2)).map_err(|e| e.message)?]);
    let quantities = vec![
        c.quantity("radius", "Radius", 1, &r, "r", vec![first.clone()])?,
        c.quantity("diameter", "Diameter", 1, &d, "d=2r", vec![first.clone(), step(format!("d=2\\cdot {rl}"), "Twice the radius")])?,
        c.quantity("circumference", "Circumference", 1, &circ, "C=2\\pi r", vec![first.clone(), step(format!("C=2\\pi\\cdot {rl}"), "Once round the circle")])?,
        c.quantity("area", "Area", 2, &area, "A=\\pi r^2", vec![first, step(format!("A=\\pi\\left({rl}\\right)^2"), "π times the radius squared")])?,
    ];
    let radius = c.f64_of(&r)?;
    Ok(FigureResult { status: "solved", message: String::new(), points: vec![centre], sides: Vec::new(), angles: Vec::new(), quantities, radius: Some(radius) })
}

fn rectangle(c: &mut Ctx, input: &FigureInput) -> R<FigureResult> {
    let p = &input.points;
    if p.len() != 4 {
        return Err("A rectangle has four corners".into());
    }
    let square = input.kind == "square";
    let mut vals = Vec::new();
    for i in 0..4 {
        vals.push(c.given(input.sides.get(i).unwrap_or(&None), false, &format!("Side {}", side_name(i, 4)))?);
    }
    // AB = CD is the width, BC = DA the height (one side for a square).
    let pick = |a: &Option<Sym>, b: &Option<Sym>, what: &str| -> R<Option<Sym>> {
        match (a, b) {
            (Some(x), Some(y)) if x != y => Err(format!("Opposite sides of a rectangle are equal, but the two {what} sides differ")),
            (Some(x), _) | (_, Some(x)) => Ok(Some(x.clone())),
            _ => Ok(None),
        }
    };
    let (mut w, mut h) = (pick(&vals[0], &vals[2], "horizontal")?, pick(&vals[1], &vals[3], "vertical")?);
    if square {
        let s = match (&w, &h) {
            (Some(x), Some(y)) if x != y => return Err("A square's sides are all equal".into()),
            (Some(x), _) | (_, Some(x)) => Some(x.clone()),
            _ => None,
        };
        w = s.clone();
        h = s;
    }
    let mut angles = Vec::new();
    for i in 0..4 {
        let mut a = c.part(corner(i), "fixed", Some(&n(90)))?;
        a.status = "fixed";
        angles.push(a);
    }
    let (Some(w), Some(h)) = (w.clone(), h.clone()) else {
        let mut sides = Vec::new();
        for i in 0..4 {
            let known = if i % 2 == 0 { &w } else { &h };
            let status = if vals[i].is_some() { "given" } else { "found" };
            sides.push(c.part(side_name(i, 4), status, known.as_ref())?);
        }
        let message = if square { "Give one side".to_string() } else if w.is_none() && h.is_none() { "Give the width and the height (two sides that meet)".into() } else { "Give one more side: one that meets the side you gave".into() };
        return Ok(FigureResult { status: "missing", message, points: p.clone(), sides, angles, quantities: Vec::new(), radius: None });
    };
    let mut sides = Vec::new();
    for i in 0..4 {
        let v = if i % 2 == 0 { &w } else { &h };
        sides.push(c.part(side_name(i, 4), if vals[i].is_some() { "given" } else { "found" }, Some(v))?);
    }
    // To scale: A where it was drawn, AB along the drawn direction, D on the drawn side.
    let (wf, hf) = (c.f64_of(&w)?, c.f64_of(&h)?);
    let (ux, uy) = unit(p[0], p[1]);
    let s = orientation(p);
    let a = p[0];
    let b = [a[0] + wf * ux, a[1] + wf * uy];
    let d = [a[0] - s * hf * uy, a[1] + s * hf * ux];
    let cc = [b[0] + d[0] - a[0], b[1] + d[1] - a[1]];
    let (wl, hl) = (c.show(&w)?.0, c.show(&h)?.0);
    let diag = pow(add(vec![pow(w.clone(), n(2)).map_err(|e| e.message)?, pow(h.clone(), n(2)).map_err(|e| e.message)?]), sym::q(1, 2)).map_err(|e| e.message)?;
    let quantities = if square {
        vec![
            c.quantity("perimeter", "Perimeter", 1, &mul(vec![n(4), w.clone()]), "P=4s", vec![step(format!("P=4\\cdot {wl}"), "Four equal sides")])?,
            c.quantity("area", "Area", 2, &pow(w.clone(), n(2)).map_err(|e| e.message)?, "A=s^2", vec![step(format!("A=\\left({wl}\\right)^2"), "The side squared")])?,
            c.quantity("diagonal", "Diagonal", 1, &diag, "d=s\\sqrt{2}", vec![step(format!("d={wl}\\sqrt{{2}}"), "Pythagoras: $d^2=s^2+s^2$")])?,
        ]
    } else {
        vec![
            c.quantity("perimeter", "Perimeter", 1, &mul(vec![n(2), add(vec![w.clone(), h.clone()])]), "P=2(w+h)", vec![step(format!("P=2\\left({wl}+{hl}\\right)"), "Twice the width plus the height")])?,
            c.quantity("area", "Area", 2, &mul(vec![w.clone(), h.clone()]), "A=wh", vec![step(format!("A={wl}\\cdot {hl}"), "Width times height")])?,
            c.quantity("diagonal", "Diagonal", 1, &diag, "d=\\sqrt{w^2+h^2}", vec![step(format!("d=\\sqrt{{\\left({wl}\\right)^2+\\left({hl}\\right)^2}}"), "Pythagoras")])?,
        ]
    };
    Ok(FigureResult { status: "solved", message: String::new(), points: vec![a, b, cc, d], sides, angles, quantities, radius: None })
}

fn unit(a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 { (1.0, 0.0) } else { (dx / len, dy / len) }
}

/// The interior angle at each corner of a sketch, in degrees.
fn sketch_angles(p: &[[f64; 2]]) -> Vec<f64> {
    let k = p.len();
    let s = orientation(p);
    (0..k)
        .map(|i| {
            let (a, b, cc) = (p[(i + k - 1) % k], p[i], p[(i + 1) % k]);
            interior(a, b, cc, s).to_degrees()
        })
        .collect()
}

/// The interior angle at b between b→c and b→a, for a polygon of orientation s, in [0, 2π).
fn interior(a: [f64; 2], b: [f64; 2], c: [f64; 2], s: f64) -> f64 {
    let (u, v) = ([c[0] - b[0], c[1] - b[1]], [a[0] - b[0], a[1] - b[1]]);
    let cross = u[0] * v[1] - u[1] * v[0];
    let dot = u[0] * v[0] + u[1] * v[1];
    let t = (s * cross).atan2(dot);
    if t < 0.0 { t + std::f64::consts::TAU } else { t }
}

fn triangle(c: &mut Ctx, input: &FigureInput) -> R<FigureResult> {
    let p = &input.points;
    if p.len() != 3 {
        return Err("A triangle has three corners".into());
    }
    // Side i joins corner i to i+1: AB is c, BC is a, CA is b.
    const SIDE_OF: [usize; 3] = [2, 0, 1];
    let mut side: [Option<Sym>; 3] = [None, None, None];
    let mut angle: [Option<Sym>; 3] = [None, None, None];
    let mut given_side = [false; 3];
    let mut given_angle = [false; 3];
    for i in 0..3 {
        if let Some(v) = c.given(input.sides.get(i).unwrap_or(&None), false, &format!("Side {}", side_name(i, 3)))? {
            side[SIDE_OF[i]] = Some(v);
            given_side[i] = true;
        }
        if let Some(v) = c.given(input.angles.get(i).unwrap_or(&None), true, &format!("Angle {}", corner(i)))? {
            angle[i] = Some(v);
            given_angle[i] = true;
        }
    }
    let count = given_side.iter().chain(&given_angle).filter(|g| **g).count();
    let sides_given = given_side.iter().filter(|g| **g).count();
    let parts_now = |c: &mut Ctx, side: &[Option<Sym>; 3], angle: &[Option<Sym>; 3]| -> R<(Vec<Part>, Vec<Part>)> {
        let mut sp = Vec::new();
        for i in 0..3 {
            sp.push(c.part(side_name(i, 3), if given_side[i] { "given" } else { "found" }, side[SIDE_OF[i]].as_ref())?);
        }
        let mut ap = Vec::new();
        for i in 0..3 {
            ap.push(c.part(corner(i), if given_angle[i] { "given" } else { "found" }, angle[i].as_ref())?);
        }
        Ok((sp, ap))
    };
    if count < 3 || sides_given == 0 {
        let (sides, angles) = parts_now(c, &side, &angle)?;
        let message = if count >= 3 {
            "Angles alone fix the shape but not the size: give a side".to_string()
        } else {
            let left = 3 - count;
            let what = if sides_given == 0 { " (at least one a side)" } else { "" };
            format!("Give {left} more side{} or angle{}{what}", if left == 1 { "" } else { "s" }, if left == 1 { "" } else { "s" })
        };
        return Ok(FigureResult { status: "missing", message, points: p.clone(), sides, angles, quantities: Vec::new(), radius: None });
    }
    // More than three: solve from three (sides first) and check the rest.
    let mut use_side = side.clone();
    let mut use_angle = angle.clone();
    let mut extra = Vec::new();
    if count > 3 {
        let mut kept = 0;
        for k in 0..3 {
            if use_side[k].is_some() {
                if kept < 3 { kept += 1 } else { extra.push((true, k, use_side[k].take().expect("some"))) }
            }
        }
        for k in 0..3 {
            if use_angle[k].is_some() {
                if kept < 3 { kept += 1 } else { extra.push((false, k, use_angle[k].take().expect("some"))) }
            }
        }
    }
    let mut steps = Vec::new();
    let solutions = c.b.triangles_from(use_side, use_angle, &mut steps).map_err(|e| e.message)?;
    if solutions.is_empty() {
        let (sides, angles) = parts_now(c, &side, &angle)?;
        return Ok(FigureResult { status: "conflict", message: "No triangle has these measurements".into(), points: p.clone(), sides, angles, quantities: Vec::new(), radius: None });
    }
    // Where two fit, the one closer to the drawing.
    let drawn = sketch_angles(p);
    let mut best = 0;
    let mut best_gap = f64::INFINITY;
    for (k, (_, ang)) in solutions.iter().enumerate() {
        let mut gap = 0.0;
        for i in 0..3 {
            gap += (c.f64_of(&ang[i])? - drawn[i]).abs();
        }
        if gap < best_gap {
            best_gap = gap;
            best = k;
        }
    }
    let (s, a) = solutions[best].clone();
    for (is_side, k, v) in &extra {
        let have = if *is_side { &s[*k] } else { &a[*k] };
        if (c.f64_of(have)? - c.f64_of(v)?).abs() > 1e-9 * c.f64_of(v)?.abs().max(1.0) {
            let (sides, angles) = parts_now(c, &side, &angle)?;
            return Ok(FigureResult { status: "conflict", message: "These measurements don't fit one triangle: remove one".into(), points: p.clone(), sides, angles, quantities: Vec::new(), radius: None });
        }
    }
    let side_full = [Some(s[0].clone()), Some(s[1].clone()), Some(s[2].clone())];
    let angle_full = [Some(a[0].clone()), Some(a[1].clone()), Some(a[2].clone())];
    let (sides, angles) = parts_now(c, &side_full, &angle_full)?;
    // To scale: A where drawn, AB along the drawn direction, C on the drawn side.
    let (cf, bf, af_deg) = (c.f64_of(&s[2])?, c.f64_of(&s[1])?, c.f64_of(&a[0])?);
    let (ux, uy) = unit(p[0], p[1]);
    let o = orientation(p);
    let base = uy.atan2(ux);
    let t = base + o * af_deg.to_radians();
    let pa = p[0];
    let pb = [pa[0] + cf * ux, pa[1] + cf * uy];
    let pc = [pa[0] + bf * t.cos(), pa[1] + bf * t.sin()];
    // Quantities, exactly.
    let (sa, sb, sc) = (s[0].clone(), s[1].clone(), s[2].clone());
    let sin = |c: &mut Ctx, x: &Sym| c.b.func(Func::Sin, x.clone()).map_err(|e| e.message);
    let perimeter = simp(add(vec![sa.clone(), sb.clone(), sc.clone()]));
    let half = simp(div(perimeter.clone(), n(2)).map_err(|e| e.message)?);
    // The area two ways, ½ab·sin C and Heron's √(s(s−a)(s−b)(s−c)): whichever comes out
    // exact and shorter (sin of an arccos rarely simplifies; a surd under Heron may not).
    let by_sine = simp(mul(vec![sym::q(1, 2), sa.clone(), sb.clone(), sin(c, &a[2])?]));
    let minus = |x: &Sym| add(vec![half.clone(), sym::neg(x.clone())]);
    let heron = simp(pow(simp(mul(vec![half.clone(), minus(&sa), minus(&sb), minus(&sc)])), sym::q(1, 2)).map_err(|e| e.message)?);
    let score = |s: &Sym| (!s.is_closed_form(), crate::latex::latex(s).len());
    let use_heron = score(&heron) < score(&by_sine);
    let area = if use_heron { heron } else { by_sine };
    // R = abc/(4A): no sine needed.
    let circum = simp(div(mul(vec![sa.clone(), sb.clone(), sc.clone()]), mul(vec![n(4), area.clone()])).map_err(|e| e.message)?);
    let inr = simp(div(area.clone(), half.clone()).map_err(|e| e.message)?);
    let height = simp(div(mul(vec![n(2), area.clone()]), sc.clone()).map_err(|e| e.message)?);
    let (la, lb, lc) = (c.show(&sa)?.0, c.show(&sb)?.0, c.show(&sc)?.0);
    let lcc = c.show(&a[2])?.0;
    let (larea, lper, lhalf) = (c.show(&area)?.0, c.show(&perimeter)?.0, c.show(&half)?.0);
    let mut solving = steps;
    solving.truncate(6);
    let (area_formula, area_step) = if use_heron {
        (
            "A=\\sqrt{s(s-a)(s-b)(s-c)}",
            step(format!("s=\\frac{{{lper}}}{{2}}={lhalf},\\quad A=\\sqrt{{{lhalf}\\left({lhalf}-{la}\\right)\\left({lhalf}-{lb}\\right)\\left({lhalf}-{lc}\\right)}}"), "Heron's formula, from the three sides and half the perimeter $s$"),
        )
    } else {
        ("A=\\frac{1}{2}ab\\sin C", step(format!("A=\\frac{{1}}{{2}}\\cdot {la}\\cdot {lb}\\cdot\\sin {lcc}^{{\\circ}}"), "Half of two sides times the sine of the angle between them"))
    };
    let quantities = vec![
        c.quantity("perimeter", "Perimeter", 1, &perimeter, "P=a+b+c", vec![step(format!("P={la}+{lb}+{lc}"), "Add the three sides")])?,
        c.quantity("area", "Area", 2, &area, area_formula, vec![area_step])?,
        c.quantity("height", "Height from C", 1, &height, "h_C=\\frac{2A}{c}", vec![step(format!("h_C=\\frac{{2\\cdot {larea}}}{{{lc}}}"), "The height onto AB: twice the area over the base")])?,
        c.quantity("circumradius", "Circumradius", 1, &circum, "R=\\frac{abc}{4A}", vec![step(format!("R=\\frac{{{la}\\cdot {lb}\\cdot {lc}}}{{4\\cdot {larea}}}"), "The radius of the circle through the three corners")])?,
        c.quantity("inradius", "Inradius", 1, &inr, "r=\\frac{A}{s}", vec![step(format!("s=\\frac{{{lper}}}{{2}}={lhalf},\\quad r=\\frac{{{larea}}}{{{lhalf}}}"), "The radius of the circle inside: the area over half the perimeter")])?,
    ];
    let mut quantities = quantities;
    // How the missing parts were found goes with every quantity that used them.
    if !solving.is_empty() {
        for q in &mut quantities {
            let mut s = solving.clone();
            s.append(&mut q.steps);
            q.steps = s;
        }
    }
    let message = if solutions.len() > 1 { "Two triangles fit these measurements; this is the one closer to your drawing".into() } else { String::new() };
    Ok(FigureResult { status: "solved", message, points: vec![pa, pb, pc], sides, angles, quantities, radius: None })
}

/// Any polygon: least squares from the sketch, for the corners that satisfy every given side
/// and angle.
fn polygon(c: &mut Ctx, input: &FigureInput) -> R<FigureResult> {
    let p = &input.points;
    let k = p.len();
    if k < 3 {
        return Err("A polygon needs at least three corners".into());
    }
    let mut lengths: Vec<Option<f64>> = Vec::new();
    let mut angles: Vec<Option<f64>> = Vec::new();
    let mut given_l = Vec::new();
    let mut given_a = Vec::new();
    for i in 0..k {
        let l = c.given(input.sides.get(i).unwrap_or(&None), false, &format!("Side {}", side_name(i, k)))?;
        given_l.push(l.clone());
        lengths.push(match &l { Some(v) => Some(c.f64_of(v)?), None => None });
        let a = c.given(input.angles.get(i).unwrap_or(&None), true, &format!("Angle {}", corner(i)))?;
        given_a.push(a.clone());
        angles.push(match &a { Some(v) => Some(c.f64_of(v)?.to_radians()), None => None });
    }
    let s = orientation(p);
    let unknown_parts = |c: &mut Ctx| -> R<(Vec<Part>, Vec<Part>)> {
        let mut sp = Vec::new();
        for i in 0..k {
            sp.push(c.part(side_name(i, k), "given", given_l[i].as_ref())?);
        }
        let mut ap = Vec::new();
        for i in 0..k {
            ap.push(c.part(corner(i), "given", given_a[i].as_ref())?);
        }
        Ok((sp, ap))
    };
    let constraints = lengths.iter().filter(|x| x.is_some()).count() + angles.iter().filter(|x| x.is_some()).count();
    let needed = 2 * k - 3;
    // Interior angles of a simple polygon add to (k − 2)·180°.
    let angle_sum: f64 = angles.iter().flatten().sum();
    let all_angles = angles.iter().all(|a| a.is_some());
    if all_angles && (angle_sum - (k as f64 - 2.0) * std::f64::consts::PI).abs() > 1e-9 {
        let (sides, angs) = unknown_parts(c)?;
        return Ok(FigureResult {
            status: "conflict",
            message: format!("The angles of a {k}-sided shape add up to {}°, but these add up to {}°", (k - 2) * 180, fmt(angle_sum.to_degrees())),
            points: p.clone(),
            sides,
            angles: angs,
            quantities: Vec::new(),
            radius: None,
        });
    }
    if lengths.iter().all(|l| l.is_none()) || constraints < needed {
        let (sides, angs) = unknown_parts(c)?;
        let left = needed.saturating_sub(constraints).max(1);
        let message = if lengths.iter().all(|l| l.is_none()) && constraints >= needed {
            "Angles alone fix the shape but not the size: give a side".to_string()
        } else {
            format!("Give {left} more side{} or angle{} (a {k}-sided shape needs {needed}{})", if left == 1 { "" } else { "s" }, if left == 1 { "" } else { "s" }, if lengths.iter().all(|l| l.is_none()) { ", at least one a side" } else { "" })
        };
        return Ok(FigureResult { status: "missing", message, points: p.clone(), sides, angles: angs, quantities: Vec::new(), radius: None });
    }
    // Start from the sketch scaled to the given sides.
    let sketch_len = |i: usize| ((p[(i + 1) % k][0] - p[i][0]).powi(2) + (p[(i + 1) % k][1] - p[i][1]).powi(2)).sqrt().max(1e-9);
    let ratios: Vec<f64> = (0..k).filter_map(|i| lengths[i].map(|l| l / sketch_len(i))).collect();
    let scale = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let origin = p[0];
    let mut x: Vec<f64> = p.iter().skip(1).flat_map(|q| [origin[0] + (q[0] - origin[0]) * scale, origin[1] + (q[1] - origin[1]) * scale]).collect();
    let dir = unit(p[0], p[1]);
    let typical = lengths.iter().flatten().sum::<f64>() / ratios.len() as f64;
    let point = |x: &[f64], i: usize| if i == 0 { origin } else { [x[2 * (i - 1)], x[2 * (i - 1) + 1]] };
    let residuals = |x: &[f64]| -> Vec<f64> {
        let mut r = Vec::new();
        for i in 0..k {
            if let Some(l) = lengths[i] {
                let (a, b) = (point(x, i), point(x, (i + 1) % k));
                r.push((((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() - l) / typical);
            }
            if let Some(t) = angles[i] {
                let got = interior(point(x, (i + k - 1) % k), point(x, i), point(x, (i + 1) % k), s);
                let mut d = got - t;
                while d > std::f64::consts::PI {
                    d -= std::f64::consts::TAU;
                }
                while d < -std::f64::consts::PI {
                    d += std::f64::consts::TAU;
                }
                r.push(d);
            }
        }
        // Keep AB along the drawn direction.
        let b = point(x, 1);
        r.push(((b[0] - origin[0]) * dir.1 - (b[1] - origin[1]) * dir.0) / typical);
        r
    };
    // Levenberg–Marquardt with a numeric Jacobian.
    let mut lambda = 1e-3;
    let mut res = residuals(&x);
    let norm = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
    for _ in 0..300 {
        let m = res.len();
        let nv = x.len();
        let mut jac = vec![vec![0.0; nv]; m];
        for j in 0..nv {
            let h = 1e-7 * typical.max(1e-9);
            let mut xp = x.clone();
            xp[j] += h;
            let rp = residuals(&xp);
            for i in 0..m {
                jac[i][j] = (rp[i] - res[i]) / h;
            }
        }
        // (JᵀJ + λ diag) δ = −Jᵀr
        let mut a = vec![vec![0.0; nv + 1]; nv];
        for r in 0..nv {
            for col in 0..nv {
                a[r][col] = (0..m).map(|i| jac[i][r] * jac[i][col]).sum();
            }
            a[r][r] *= 1.0 + lambda;
            a[r][r] += 1e-12;
            a[r][nv] = -(0..m).map(|i| jac[i][r] * res[i]).sum::<f64>();
        }
        let Some(delta) = gauss(a) else { break };
        let trial: Vec<f64> = x.iter().zip(&delta).map(|(v, d)| v + d).collect();
        let tr = residuals(&trial);
        if norm(&tr) < norm(&res) {
            x = trial;
            res = tr;
            lambda = (lambda * 0.3).max(1e-12);
        } else {
            lambda *= 10.0;
        }
        if norm(&res) < 1e-26 || lambda > 1e12 {
            break;
        }
    }
    let worst = res.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    if worst > 1e-7 {
        let (sides, angs) = unknown_parts(c)?;
        return Ok(FigureResult { status: "conflict", message: "These measurements don't fit together in one shape: check them or remove one".into(), points: p.clone(), sides, angles: angs, quantities: Vec::new(), radius: None });
    }
    let pts: Vec<[f64; 2]> = (0..k).map(|i| point(&x, i)).collect();
    let mut sides = Vec::new();
    let mut perim = 0.0;
    for i in 0..k {
        let (a, b) = (pts[i], pts[(i + 1) % k]);
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        perim += len;
        sides.push(match &given_l[i] {
            Some(v) => c.part(side_name(i, k), "given", Some(v))?,
            None => Part { name: side_name(i, k), status: "found", latex: Some(fmt(len)), text: Some(fmt(len)), value: Some(len) },
        });
    }
    let mut angs = Vec::new();
    for i in 0..k {
        let deg = interior(pts[(i + k - 1) % k], pts[i], pts[(i + 1) % k], s).to_degrees();
        angs.push(match &given_a[i] {
            Some(v) => c.part(corner(i), "given", Some(v))?,
            None => Part { name: corner(i), status: "found", latex: Some(fmt(deg)), text: Some(fmt(deg)), value: Some(deg) },
        });
    }
    let twice: f64 = (0..k).map(|i| pts[i][0] * pts[(i + 1) % k][1] - pts[(i + 1) % k][0] * pts[i][1]).sum();
    let area = twice.abs() / 2.0;
    let coords = pts.iter().enumerate().map(|(i, q)| format!("{}\\left({},{}\\right)", corner(i), fmt(q[0] - origin[0]), fmt(q[1] - origin[1]))).collect::<Vec<_>>().join(",\\ ");
    let found_note = step(coords, "The corners to scale (A at the origin): fitted so every given side and angle holds");
    let quantities = vec![
        Quantity {
            id: "perimeter",
            name: "Perimeter",
            dim: 1,
            latex: fmt(perim),
            text: fmt(perim),
            value: perim,
            formula: "P=\\text{sum of the sides}".into(),
            steps: vec![found_note.clone(), step(format!("P={}", sides.iter().map(|s| s.text.clone().unwrap_or_default()).collect::<Vec<_>>().join("+")), "Add the sides")],
        },
        Quantity {
            id: "area",
            name: "Area",
            dim: 2,
            latex: fmt(area),
            text: fmt(area),
            value: area,
            formula: "A=\\frac{1}{2}\\left|\\sum_i (x_iy_{i+1}-x_{i+1}y_i)\\right|".into(),
            steps: vec![found_note, step(format!("A=\\frac{{1}}{{2}}\\left|{}\\right|={}", fmt(twice), fmt(area)), "The shoelace formula over the corners in order")],
        },
    ];
    let message = if constraints > needed { "More measurements than needed: they all fit".into() } else { String::new() };
    Ok(FigureResult { status: "solved", message, points: pts, sides, angles: angs, quantities, radius: None })
}

/// Solves the augmented system (Gaussian elimination with pivoting).
fn gauss(mut a: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let nv = a.len();
    for col in 0..nv {
        let piv = (col..nv).max_by(|&i, &j| a[i][col].abs().partial_cmp(&a[j][col].abs()).unwrap_or(std::cmp::Ordering::Equal))?;
        if a[piv][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, piv);
        for r in col + 1..nv {
            let f = a[r][col] / a[col][col];
            for cc in col..=nv {
                a[r][cc] -= f * a[col][cc];
            }
        }
    }
    let mut x = vec![0.0; nv];
    for r in (0..nv).rev() {
        let s: f64 = (r + 1..nv).map(|cc| a[r][cc] * x[cc]).sum();
        x[r] = (a[r][nv] - s) / a[r][r];
    }
    Some(x)
}
