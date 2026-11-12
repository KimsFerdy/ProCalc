//! Geometry: solving triangles, the measurements of common shapes, and coordinates, with the
//! working.
//!
//! Triangles: sides a, b, c opposite angles A, B, C. Any three parts with at least one side;
//! the ambiguous SSA case gives both triangles. Angles are in the angle mode (an explicit °
//! always means degrees).

use crate::ast::{Arg, Expr, Func, GeoFn};
use crate::latex::latex;
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow};
use crate::{AngleMode, EngineError, Method, Step};

type R<T> = Result<T, EngineError>;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn simp(s: Sym) -> Sym {
    sym::normalize_complex(crate::poly::expand(&s).unwrap_or(s))
}

fn sq(s: &Sym) -> R<Sym> {
    pow(s.clone(), n(2))
}

fn sqrt(s: Sym) -> R<Sym> {
    pow(s, sym::q(1, 2))
}

/// The answer: a list of named values (one list per solution), and the value to use when the
/// call sits inside a larger expression.
pub struct Geo {
    pub solutions: Vec<Vec<(String, Sym)>>,
    pub primary: Option<Sym>,
}

#[derive(Clone)]
struct Tri {
    side: [Option<Sym>; 3],
    angle: [Option<Sym>; 3],
}

const SIDES: [&str; 3] = ["a", "b", "c"];
const ANGLES: [&str; 3] = ["A", "B", "C"];

impl Builder<'_> {
    /// An angle in the angle mode's units; an explicit ° is degrees either way.
    pub(crate) fn angle_value(&mut self, e: &Expr) -> R<Sym> {
        if self.ev.angle == AngleMode::Degrees
            && let Some(inner) = crate::eval::as_explicit_degrees(e)
        {
            return self.build(inner);
        }
        self.build(e)
    }

    fn half_turn(&self) -> Sym {
        match self.ev.angle {
            AngleMode::Radians => Sym::Const(crate::ast::Const::Pi),
            AngleMode::Degrees => n(180),
        }
    }

    fn positive(&mut self, s: &Sym, what: &str) -> R<()> {
        if !s.has_vars() && self.sign_of(s)? <= 0 {
            return Err(EngineError::new(format!("{what} must be positive")));
        }
        Ok(())
    }

    /// Named arguments (r=2) and positional ones, in that order of the names.
    fn shape_args(&mut self, args: &[Arg], names: &[&str]) -> R<Vec<Option<Sym>>> {
        let mut out = vec![None; names.len()];
        let mut next = 0;
        for a in args {
            match a {
                Arg::Eq(Expr::Var(k), v) => {
                    let i = names.iter().position(|nm| nm == k).ok_or_else(|| EngineError::new(format!("This takes {}, not {k}", names.join(", "))))?;
                    out[i] = Some(self.build(v)?);
                }
                Arg::Expr(e) => {
                    while next < names.len() && out[next].is_some() {
                        next += 1;
                    }
                    if next >= names.len() {
                        return Err(EngineError::new(format!("Too many values: this takes {}", names.join(", "))));
                    }
                    out[next] = Some(self.build(e)?);
                    next += 1;
                }
                _ => return Err(EngineError::new(format!("Give the measurements as {}", names.iter().map(|nm| format!("{nm}=…")).collect::<Vec<_>>().join(", ")))),
            }
        }
        for (v, nm) in out.iter().zip(names) {
            if let Some(v) = v {
                self.positive(v, nm)?;
            }
        }
        Ok(out)
    }

    fn points(&mut self, args: &[Arg], what: &str) -> R<Vec<Vec<Sym>>> {
        let mut out: Vec<Vec<Sym>> = Vec::new();
        for a in args {
            let Arg::Expr(e) = a else { return Err(EngineError::new(format!("{what} takes points like [1, 2]"))) };
            match self.build(e)? {
                Sym::Matrix(m) => out.push(m.into_iter().flatten().collect()),
                _ => return Err(EngineError::new(format!("{what} takes points like [1, 2]"))),
            }
        }
        if out.windows(2).any(|w| w[0].len() != w[1].len()) {
            return Err(EngineError::new("The points need the same number of coordinates"));
        }
        Ok(out)
    }

    /// `triangle(…)`, `circle(r=2)`, `distance(P, Q)`, …
    pub(crate) fn geometry(&mut self, f: GeoFn, args: &[Arg]) -> R<Geo> {
        let mut steps = Vec::new();
        let pi = || Sym::Const(crate::ast::Const::Pi);
        let one = |pairs: Vec<(&str, Sym)>, primary: usize| -> Geo {
            let p = pairs.get(primary).map(|(_, v)| v.clone());
            Geo { solutions: vec![pairs.into_iter().map(|(k, v)| (k.to_string(), simp(v))).collect()], primary: p.map(simp) }
        };
        let need = |v: &Option<Sym>, what: &str| v.clone().ok_or_else(|| EngineError::new(format!("Give {what}")));
        let geo = match f {
            GeoFn::Triangle => return self.triangle(args),
            GeoFn::Circle => {
                let v = self.shape_args(args, &["r", "d"])?;
                let r = match (&v[0], &v[1]) {
                    (Some(r), _) => r.clone(),
                    (None, Some(d)) => div(d.clone(), n(2))?,
                    _ => return Err(EngineError::new("Give the radius r or the diameter d")),
                };
                steps.push(step("A=\\pi r^2,\\quad C=2\\pi r", "Area and circumference"));
                one(vec![("A", mul(vec![pi(), sq(&r)?])), ("C", mul(vec![n(2), pi(), r.clone()])), ("r", r.clone()), ("d", mul(vec![n(2), r]))], 0)
            }
            GeoFn::Square => {
                let v = self.shape_args(args, &["s"])?;
                let s = need(&v[0], "the side s")?;
                one(vec![("A", sq(&s)?), ("P", mul(vec![n(4), s.clone()])), ("d", mul(vec![s, sqrt(n(2))?]))], 0)
            }
            GeoFn::Rectangle => {
                let v = self.shape_args(args, &["w", "h"])?;
                let (w, h) = (need(&v[0], "the width w")?, need(&v[1], "the height h")?);
                steps.push(step("A=wh,\\quad P=2(w+h),\\quad d=\\sqrt{w^2+h^2}", "Area, perimeter and diagonal"));
                one(vec![("A", mul(vec![w.clone(), h.clone()])), ("P", mul(vec![n(2), add(vec![w.clone(), h.clone()])])), ("d", sqrt(add(vec![sq(&w)?, sq(&h)?]))?)], 0)
            }
            GeoFn::Trapezoid => {
                let v = self.shape_args(args, &["a", "b", "h"])?;
                let (a, b, h) = (need(&v[0], "the parallel sides a and b")?, need(&v[1], "the parallel sides a and b")?, need(&v[2], "the height h")?);
                steps.push(step("A=\\frac{(a+b)h}{2}", "Area"));
                one(vec![("A", div(mul(vec![add(vec![a, b]), h]), n(2))?)], 0)
            }
            GeoFn::Sphere => {
                let v = self.shape_args(args, &["r"])?;
                let r = need(&v[0], "the radius r")?;
                steps.push(step("V=\\frac{4}{3}\\pi r^3,\\quad S=4\\pi r^2", "Volume and surface area"));
                one(vec![("V", mul(vec![sym::q(4, 3), pi(), pow(r.clone(), n(3))?])), ("S", mul(vec![n(4), pi(), sq(&r)?]))], 0)
            }
            GeoFn::Cylinder => {
                let v = self.shape_args(args, &["r", "h"])?;
                let (r, h) = (need(&v[0], "the radius r")?, need(&v[1], "the height h")?);
                steps.push(step("V=\\pi r^2h,\\quad S=2\\pi r(r+h)", "Volume and surface area"));
                one(vec![("V", mul(vec![pi(), sq(&r)?, h.clone()])), ("S", mul(vec![n(2), pi(), r.clone(), add(vec![r, h])]))], 0)
            }
            GeoFn::Cone => {
                let v = self.shape_args(args, &["r", "h"])?;
                let (r, h) = (need(&v[0], "the radius r")?, need(&v[1], "the height h")?);
                let l = sqrt(add(vec![sq(&r)?, sq(&h)?]))?;
                steps.push(step("V=\\frac{1}{3}\\pi r^2h,\\quad l=\\sqrt{r^2+h^2},\\quad S=\\pi r(r+l)", "Volume, slant height and surface area"));
                one(vec![("V", mul(vec![sym::q(1, 3), pi(), sq(&r)?, h])), ("l", l.clone()), ("S", mul(vec![pi(), r.clone(), add(vec![r, l])]))], 0)
            }
            GeoFn::Cube => {
                let v = self.shape_args(args, &["s"])?;
                let s = need(&v[0], "the side s")?;
                one(vec![("V", pow(s.clone(), n(3))?), ("S", mul(vec![n(6), sq(&s)?])), ("d", mul(vec![s, sqrt(n(3))?]))], 0)
            }
            GeoFn::Box => {
                let v = self.shape_args(args, &["l", "w", "h"])?;
                let (l, w, h) = (need(&v[0], "l, w and h")?, need(&v[1], "l, w and h")?, need(&v[2], "l, w and h")?);
                steps.push(step("V=lwh,\\quad S=2(lw+wh+hl),\\quad d=\\sqrt{l^2+w^2+h^2}", "Volume, surface area and diagonal"));
                let s = mul(vec![n(2), add(vec![mul(vec![l.clone(), w.clone()]), mul(vec![w.clone(), h.clone()]), mul(vec![h.clone(), l.clone()])])]);
                one(vec![("V", mul(vec![l.clone(), w.clone(), h.clone()])), ("S", s), ("d", sqrt(add(vec![sq(&l)?, sq(&w)?, sq(&h)?]))?)], 0)
            }
            GeoFn::Distance | GeoFn::Midpoint | GeoFn::Slope | GeoFn::Line => {
                let pts = self.points(args, "This")?;
                let [p, q] = pts.as_slice() else { return Err(EngineError::new("This takes two points, like ([1, 2], [4, 6])")) };
                let diffs: Vec<Sym> = p.iter().zip(q).map(|(a, b)| add(vec![b.clone(), neg(a.clone())])).collect();
                match f {
                    GeoFn::Distance => {
                        let d = sqrt(add(diffs.iter().map(sq).collect::<R<Vec<_>>>()?))?;
                        steps.push(step(format!("\\sqrt{{{}}}={}", diffs.iter().map(|d| format!("\\left({}\\right)^2", latex(&simp(d.clone())))).collect::<Vec<_>>().join("+"), latex(&simp(d.clone()))), "The square root of the sum of the squared differences"));
                        one(vec![("d", d)], 0)
                    }
                    GeoFn::Midpoint => {
                        let m: Vec<Vec<Sym>> = p.iter().zip(q).map(|(a, b)| vec![simp(div(add(vec![a.clone(), b.clone()]), n(2)).unwrap_or_else(|_| n(0)))]).collect();
                        steps.push(step(crate::matrix::matrix_latex(&m), "The average of each coordinate"));
                        one(vec![("M", Sym::Matrix(m))], 0)
                    }
                    _ => {
                        if p.len() != 2 {
                            return Err(EngineError::new("Slopes and lines are for points in the plane, [x, y]"));
                        }
                        if simp(diffs[0].clone()).is_zero() {
                            if f == GeoFn::Slope {
                                return Err(EngineError::new("The line is vertical: its slope is undefined"));
                            }
                            steps.push(step(format!("x={}", latex(&p[0])), "The same x: a vertical line"));
                            Geo { solutions: vec![vec![("x".into(), p[0].clone())]], primary: None }
                        } else {
                            let m = simp(div(diffs[1].clone(), diffs[0].clone())?);
                            steps.push(step(format!("m=\\frac{{{}}}{{{}}}={}", latex(&simp(diffs[1].clone())), latex(&simp(diffs[0].clone())), latex(&m)), "Rise over run"));
                            if f == GeoFn::Slope {
                                one(vec![("m", m)], 0)
                            } else {
                                let b = simp(add(vec![p[1].clone(), neg(mul(vec![m.clone(), p[0].clone()]))]));
                                let line = add(vec![mul(vec![m.clone(), Sym::Var("x".into())]), b.clone()]);
                                steps.push(step(format!("y={}", latex(&line)), "$y-y_1=m(x-x_1)$"));
                                Geo { solutions: vec![vec![("y".into(), line), ("m".into(), m), ("b".into(), b)]], primary: None }
                            }
                        }
                    }
                }
            }
            GeoFn::Angle => {
                let pts = self.points(args, "angle")?;
                // angle(u, v) between vectors, or angle(A, B, C) at B.
                let (u, v) = match pts.as_slice() {
                    [u, v] => (u.clone(), v.clone()),
                    [a, b, c] => (
                        a.iter().zip(b).map(|(x, y)| add(vec![x.clone(), neg(y.clone())])).collect(),
                        c.iter().zip(b).map(|(x, y)| add(vec![x.clone(), neg(y.clone())])).collect(),
                    ),
                    _ => return Err(EngineError::new("angle takes two vectors, or three points (the angle at the middle one)")),
                };
                let dot = simp(add(u.iter().zip(&v).map(|(a, b)| mul(vec![a.clone(), b.clone()])).collect()));
                let nu = sqrt(simp(add(u.iter().map(sq).collect::<R<Vec<_>>>()?)))?;
                let nv = sqrt(simp(add(v.iter().map(sq).collect::<R<Vec<_>>>()?)))?;
                if nu.is_zero() || nv.is_zero() {
                    return Err(EngineError::new("A zero vector has no direction"));
                }
                let cos = simp(div(dot.clone(), mul(vec![nu.clone(), nv.clone()]))?);
                let theta = self.func(Func::Acos, cos.clone())?;
                steps.push(step(format!("\\cos\\theta=\\frac{{{}}}{{{}\\cdot {}}}={}", latex(&dot), latex(&nu), latex(&nv), latex(&cos)), "$\\cos\\theta=\\frac{u\\cdot v}{|u||v|}$"));
                one(vec![("\\theta", theta)], 0)
            }
            GeoFn::Polygon => {
                let pts = self.points(args, "polygon")?;
                if pts.len() < 3 || pts.iter().any(|p| p.len() != 2) {
                    return Err(EngineError::new("polygon takes 3 or more points in the plane, in order around it"));
                }
                let k = pts.len();
                let cross = add((0..k).map(|i| {
                    let (p, q) = (&pts[i], &pts[(i + 1) % k]);
                    add(vec![mul(vec![p[0].clone(), q[1].clone()]), neg(mul(vec![q[0].clone(), p[1].clone()]))])
                }).collect());
                let area = simp(div(cross.clone(), n(2))?);
                let area = if !area.has_vars() && self.sign_of(&area)? < 0 { simp(neg(area)) } else { area };
                let perimeter = add((0..k).map(|i| {
                    let (p, q) = (&pts[i], &pts[(i + 1) % k]);
                    sqrt(add(vec![sq(&add(vec![q[0].clone(), neg(p[0].clone())])).unwrap_or_else(|_| n(0)), sq(&add(vec![q[1].clone(), neg(p[1].clone())])).unwrap_or_else(|_| n(0))])).unwrap_or_else(|_| n(0))
                }).collect());
                steps.push(step(format!("\\frac{{1}}{{2}}\\left|{}\\right|={}", latex(&simp(cross)), latex(&area)), "The shoelace formula: $\\frac{1}{2}|\\sum (x_iy_{i+1}-x_{i+1}y_i)|$"));
                one(vec![("A", area), ("P", perimeter)], 0)
            }
        };
        if !steps.is_empty() {
            self.ev.work.push(Method { name: "Geometry".into(), steps });
        }
        Ok(geo)
    }

    fn triangle(&mut self, args: &[Arg]) -> R<Geo> {
        let mut t = Tri { side: [None, None, None], angle: [None, None, None] };
        for a in args {
            let Arg::Eq(Expr::Var(k), v) = a else {
                return Err(EngineError::new("Name the parts: triangle(a=3, b=4, C=60°), with sides a, b, c opposite angles A, B, C"));
            };
            if let Some(i) = SIDES.iter().position(|s| s == k) {
                let v = self.build(v)?;
                self.positive(&v, "A side")?;
                t.side[i] = Some(v);
            } else if let Some(i) = ANGLES.iter().position(|s| s == k) {
                let v = self.angle_value(v)?;
                self.positive(&v, "An angle")?;
                t.angle[i] = Some(v);
            } else {
                return Err(EngineError::new(format!("{k} isn't a part of a triangle: use a, b, c and A, B, C")));
            }
        }
        let known = t.side.iter().chain(&t.angle).filter(|x| x.is_some()).count();
        if known != 3 || t.side.iter().all(Option::is_none) {
            return Err(EngineError::new("Give exactly three parts, at least one of them a side"));
        }
        let mut steps = Vec::new();
        let solved = self.solve_triangle(t, &mut steps, 0)?;
        if solved.is_empty() {
            self.ev.work.push(Method { name: "Triangle".into(), steps });
            return Err(EngineError::new("No triangle has those measurements"));
        }
        let mut solutions = Vec::new();
        for s in &solved {
            let (a, b, c) = (s.side[0].clone().unwrap_or(n(0)), s.side[1].clone().unwrap_or(n(0)), s.side[2].clone().unwrap_or(n(0)));
            let big_c = s.angle[2].clone().unwrap_or(n(0));
            let area = simp(mul(vec![sym::q(1, 2), a.clone(), b.clone(), self.func(Func::Sin, big_c)?]));
            let mut list: Vec<(String, Sym)> = Vec::new();
            for (name, v) in SIDES.iter().zip(&s.side).chain(ANGLES.iter().zip(&s.angle)) {
                list.push((name.to_string(), simp(v.clone().unwrap_or(n(0)))));
            }
            list.push(("\\text{area}".into(), area));
            list.push(("\\text{perimeter}".into(), simp(add(vec![a, b, c]))));
            solutions.push(list);
        }
        if solved.len() == 2 {
            steps.push(step("", "Two triangles fit (the ambiguous case): both are listed"));
        }
        self.ev.work.push(Method { name: "Triangle".into(), steps });
        Ok(Geo { solutions, primary: None })
    }

    /// Every triangle with these known parts (sides a, b, c opposite angles A, B, C), as
    /// (sides, angles): none, one, or two in the ambiguous case.
    pub(crate) fn triangles_from(&mut self, side: [Option<Sym>; 3], angle: [Option<Sym>; 3], steps: &mut Vec<Step>) -> R<Vec<([Sym; 3], [Sym; 3])>> {
        let solved = self.solve_triangle(Tri { side, angle }, steps, 0)?;
        Ok(solved
            .into_iter()
            .filter_map(|t| {
                let s: Option<Vec<Sym>> = t.side.iter().cloned().collect();
                let a: Option<Vec<Sym>> = t.angle.iter().cloned().collect();
                Some((s?.try_into().ok()?, a?.try_into().ok()?))
            })
            .collect())
    }

    /// Fills in the missing parts, branching for the ambiguous case.
    fn solve_triangle(&mut self, mut t: Tri, steps: &mut Vec<Step>, depth: u32) -> R<Vec<Tri>> {
        if depth > 12 {
            return Ok(Vec::new());
        }
        let half = self.half_turn();
        loop {
            if t.side.iter().all(Option::is_some) && t.angle.iter().all(Option::is_some) {
                return Ok(vec![t]);
            }
            // Two angles give the third.
            let known: Vec<usize> = (0..3).filter(|&i| t.angle[i].is_some()).collect();
            if known.len() == 2 {
                let missing = (0..3).find(|i| !known.contains(i)).expect("one missing");
                let v = simp(add(vec![half.clone(), neg(t.angle[known[0]].clone().expect("known")), neg(t.angle[known[1]].clone().expect("known"))]));
                if !v.has_vars() && self.sign_of(&v)? <= 0 {
                    steps.push(step(format!("{}={}", ANGLES[missing], latex(&v)), "The angles add up to more than a half turn"));
                    return Ok(Vec::new());
                }
                steps.push(step(format!("{}={}", ANGLES[missing], latex(&v)), "The angles of a triangle add up to a half turn"));
                t.angle[missing] = Some(v);
                continue;
            }
            // Three sides: each angle by the law of cosines.
            if t.side.iter().all(Option::is_some) {
                let s: Vec<Sym> = t.side.iter().map(|x| x.clone().expect("all sides")).collect();
                for i in 0..3 {
                    if t.angle[i].is_some() {
                        continue;
                    }
                    let (j, k) = ((i + 1) % 3, (i + 2) % 3);
                    let cos = simp(div(add(vec![sq(&s[j])?, sq(&s[k])?, neg(sq(&s[i])?)]), mul(vec![n(2), s[j].clone(), s[k].clone()]))?);
                    if !cos.has_vars() {
                        let c = self.sign_of(&add(vec![cos.clone(), n(-1)]))?;
                        let d = self.sign_of(&add(vec![cos.clone(), n(1)]))?;
                        if c >= 0 || d <= 0 {
                            steps.push(step(format!("\\cos {}={}", ANGLES[i], latex(&cos)), "The sides break the triangle inequality"));
                            return Ok(Vec::new());
                        }
                    }
                    let ang = self.func(Func::Acos, cos.clone())?;
                    steps.push(step(format!("\\cos {}=\\frac{{{}^2+{}^2-{}^2}}{{2\\cdot {}\\cdot {}}}={}\\ \\Rightarrow\\ {}={}", ANGLES[i], SIDES[j], SIDES[k], SIDES[i], SIDES[j], SIDES[k], latex(&cos), ANGLES[i], latex(&ang)), "The law of cosines"));
                    t.angle[i] = Some(ang);
                }
                continue;
            }
            // Two sides and the angle between them: the third side by the law of cosines.
            let mut progressed = false;
            for i in 0..3 {
                let (j, k) = ((i + 1) % 3, (i + 2) % 3);
                if t.side[i].is_none() && t.angle[i].is_some() && t.side[j].is_some() && t.side[k].is_some() {
                    let (sj, sk, ai) = (t.side[j].clone().expect("known"), t.side[k].clone().expect("known"), t.angle[i].clone().expect("known"));
                    let cos = self.func(Func::Cos, ai)?;
                    let v = simp(sqrt(simp(add(vec![sq(&sj)?, sq(&sk)?, mul(vec![n(-2), sj, sk, cos])])))?);
                    steps.push(step(format!("{}^2={}^2+{}^2-2{}{}\\cos {}\\ \\Rightarrow\\ {}={}", SIDES[i], SIDES[j], SIDES[k], SIDES[j], SIDES[k], ANGLES[i], SIDES[i], latex(&v)), "The law of cosines"));
                    t.side[i] = Some(v);
                    progressed = true;
                    break;
                }
            }
            if progressed {
                continue;
            }
            // A side with its opposite angle: the law of sines for the rest.
            let Some(p) = (0..3).find(|&i| t.side[i].is_some() && t.angle[i].is_some()) else { return Ok(Vec::new()) };
            let ratio = div(t.side[p].clone().expect("known"), self.func(Func::Sin, t.angle[p].clone().expect("known"))?)?;
            // A known angle gives its side.
            if let Some(i) = (0..3).find(|&i| t.side[i].is_none() && t.angle[i].is_some()) {
                let v = simp(mul(vec![ratio.clone(), self.func(Func::Sin, t.angle[i].clone().expect("known"))?]));
                steps.push(step(format!("\\frac{{{}}}{{\\sin {}}}=\\frac{{{}}}{{\\sin {}}}\\ \\Rightarrow\\ {}={}", SIDES[i], ANGLES[i], SIDES[p], ANGLES[p], SIDES[i], latex(&v)), "The law of sines"));
                t.side[i] = Some(v);
                continue;
            }
            // A known side gives sin of its angle: maybe two triangles.
            let Some(i) = (0..3).find(|&i| t.side[i].is_some() && t.angle[i].is_none()) else { return Ok(Vec::new()) };
            let sin = simp(div(t.side[i].clone().expect("known"), ratio)?);
            if !sin.has_vars() && self.sign_of(&add(vec![sin.clone(), n(-1)]))? > 0 {
                steps.push(step(format!("\\sin {}={}>1", ANGLES[i], latex(&sin)), "No angle has a sine above 1: no triangle"));
                return Ok(Vec::new());
            }
            let first = self.func(Func::Asin, sin.clone())?;
            steps.push(step(format!("\\sin {}=\\frac{{{}\\sin {}}}{{{}}}={}\\ \\Rightarrow\\ {}={}", ANGLES[i], SIDES[i], ANGLES[p], SIDES[p], latex(&sin), ANGLES[i], latex(&first)), "The law of sines"));
            let second = simp(add(vec![half.clone(), neg(first.clone())]));
            let mut out = Vec::new();
            let mut t1 = t.clone();
            t1.angle[i] = Some(first.clone());
            out.extend(self.solve_triangle(t1, steps, depth + 1)?);
            // The obtuse twin, when it still leaves room for the third angle.
            let room = simp(add(vec![half.clone(), neg(second.clone()), neg(t.angle[p].clone().expect("known"))]));
            if second != first && !room.has_vars() && self.sign_of(&room)? > 0 {
                let mut t2 = t.clone();
                t2.angle[i] = Some(second.clone());
                steps.push(step(format!("{}={}", ANGLES[i], latex(&second)), "The other angle with the same sine also fits"));
                out.extend(self.solve_triangle(t2, steps, depth + 1)?);
            }
            return Ok(out);
        }
    }
}
