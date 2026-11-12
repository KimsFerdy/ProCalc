//! Limits: lim_{x→a} f, from one side or both, at a number or at ±∞.
//!
//! Substitution when f is continuous at a. Otherwise the limit comes from the shape of f and
//! the arithmetic of ∞ (∞ + c = ∞, c/∞ = 0, …); the indeterminate forms 0/0 and ∞/∞ go to
//! L'Hôpital's rule, and 0·∞, ∞ − ∞ and 1^∞ are rewritten as quotients first.

use crate::ast::{Const, Func};
use crate::calculus::{self, free_of};
use crate::latex::latex;
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow};
use crate::{AngleMode, EngineError, Step};

type R<T> = Result<T, EngineError>;

/// A limit's value: a number (or expression in other letters), or ±∞.
#[derive(Debug, Clone, PartialEq)]
pub enum Lim {
    Fin(Sym),
    Pos,
    Neg,
}

/// Where x goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Point {
    At(Sym),
    PosInf,
    NegInf,
}

impl Point {
    pub fn latex(&self) -> String {
        match self {
            Point::At(a) => latex(a),
            Point::PosInf => "\\infty".into(),
            Point::NegInf => "-\\infty".into(),
        }
    }
}

impl Lim {
    pub fn to_sym(&self) -> Sym {
        match self {
            Lim::Fin(s) => s.clone(),
            Lim::Pos => Sym::Const(Const::Inf),
            Lim::Neg => neg(Sym::Const(Const::Inf)),
        }
    }

    fn infinite(sign: i8) -> Lim {
        if sign > 0 { Lim::Pos } else { Lim::Neg }
    }

    fn sign_inf(&self) -> i8 {
        match self {
            Lim::Pos => 1,
            Lim::Neg => -1,
            Lim::Fin(_) => 0,
        }
    }
}

const MAX_DEPTH: u32 = 40;
const MAX_LHOPITAL: u32 = 8;

struct Ctx {
    x: String,
    point: Point,
    side: i8,
    steps: Vec<Step>,
    lhopital: u32,
}

impl Ctx {
    /// `\lim_{x\to a^{+}}`
    fn lim_latex(&self) -> String {
        let side = match self.side {
            1 => "^{+}",
            -1 => "^{-}",
            _ => "",
        };
        format!("\\lim_{{{}\\to {}{side}}}", self.x, self.point.latex())
    }
}

fn unknown() -> EngineError {
    EngineError::new("I can't work out this limit yet")
}

/// Bounded whatever x does: sin, cos, arctan, tanh, and sums and products of them.
fn bounded(s: &Sym) -> bool {
    match s {
        Sym::Func(Func::Sin | Func::Cos | Func::Atan | Func::Tanh, _) => true,
        Sym::Add(v) | Sym::Mul(v) => v.iter().all(|t| bounded(t) || !t.has_vars()),
        Sym::Pow(b, e) => bounded(b) && matches!(e.as_num(), Some(k) if k.is_integer() && *k > num_rational::BigRational::from_integer(0.into())),
        _ => false,
    }
}

/// f as numerator over denominator, when some factor has a negative power: x²·e^{−x} = x²/eˣ.
fn split_quotient(f: &Sym) -> R<Option<(Sym, Sym)>> {
    let factors = match f {
        Sym::Mul(v) => v.clone(),
        other => vec![other.clone()],
    };
    let (mut top, mut bottom) = (Vec::new(), Vec::new());
    for fac in factors {
        match &fac {
            Sym::Pow(b, e) if sym::split_coeff((**e).clone()).0 < num_rational::BigRational::from_integer(0.into()) => {
                bottom.push(pow((**b).clone(), neg((**e).clone()))?);
            }
            _ => top.push(fac),
        }
    }
    Ok(if bottom.is_empty() { None } else { Some((mul(top), mul(bottom))) })
}

impl Builder<'_> {
    /// lim f as `x` goes to `point` (from the right when `side` is 1, the left when −1), with
    /// the working.
    pub(crate) fn limit(&mut self, f: &Sym, x: &str, point: Point, side: i8) -> R<(Lim, Vec<Step>)> {
        let mut c = Ctx { x: x.to_string(), point, side, steps: Vec::new(), lhopital: 0 };
        c.steps.push(Step { latex: format!("{}{}", c.lim_latex(), latex(f)), note: "The limit".into() });
        let l = self.lim(&mut c, f, 0)?;
        let shown = match &l {
            Lim::Fin(s) => latex(s),
            Lim::Pos => "\\infty".into(),
            Lim::Neg => "-\\infty".into(),
        };
        c.steps.push(Step { latex: format!("{}{}={shown}", c.lim_latex(), latex(f)), note: "The limit".into() });
        Ok((l, c.steps))
    }

    fn lim(&mut self, c: &mut Ctx, f: &Sym, depth: u32) -> R<Lim> {
        if depth > MAX_DEPTH {
            return Err(unknown());
        }
        if free_of(f, &c.x) {
            return Ok(Lim::Fin(f.clone()));
        }
        if let Point::At(a) = c.point.clone()
            && let Some(v) = self.continuous_value(c, f, &a)?
        {
            if depth == 0 {
                c.steps.push(Step { latex: latex(&v), note: format!("Substitute ${}={}$: the function is continuous there", c.x, latex(&a)) });
            }
            return Ok(Lim::Fin(v));
        }
        if let Some((top, bottom)) = split_quotient(f)? {
            return self.quotient(c, &top, &bottom, depth);
        }
        match f {
            Sym::Var(_) => Ok(match &c.point {
                Point::At(a) => Lim::Fin(a.clone()),
                Point::PosInf => Lim::Pos,
                Point::NegInf => Lim::Neg,
            }),
            Sym::Add(terms) => self.sum_limit(c, terms, depth),
            Sym::Mul(factors) => self.product_limit(c, factors, depth),
            Sym::Pow(b, e) => self.power_limit(c, b, e, depth),
            Sym::Func(g, u) => self.func_limit(c, *g, u, depth),
            Sym::Log(base, u) => {
                let q = div(sym::ln((**u).clone())?, sym::ln((**base).clone())?)?;
                self.lim(c, &q, depth + 1)
            }
            Sym::Factorial(u, _) => match self.lim(c, u, depth + 1)? {
                Lim::Pos => Ok(Lim::Pos),
                _ => Err(unknown()),
            },
            _ => Err(unknown()),
        }
    }

    /// f(a), when substituting works and gives a finite number.
    fn continuous_value(&mut self, c: &Ctx, f: &Sym, a: &Sym) -> R<Option<Sym>> {
        let Ok(v) = self.subst(f, &c.x, a) else { return Ok(None) };
        if v.has_vars() {
            return Ok(Some(v));
        }
        Ok(match self.numeric(&v) {
            Ok(crate::number::Value::Complex(..)) | Err(_) => None,
            Ok(val) => {
                let fl = self.ev.prec.float_of(&val);
                (!fl.is_nan() && !fl.is_inf()).then_some(v)
            }
        })
    }

    fn is_zero_lim(&mut self, s: &Sym) -> bool {
        s.is_zero() || (!s.has_vars() && self.sign_of(s).is_ok_and(|k| k == 0))
    }

    /// The sign of `s` just beside the point (both sides must agree when the limit is
    /// two-sided).
    fn sign_near(&mut self, c: &Ctx, s: &Sym) -> R<i8> {
        let sample = |b: &mut Self, at: Sym| -> R<i8> {
            let v = b.subst(s, &c.x, &at)?;
            if v.has_vars() {
                return Err(EngineError::new("The limit's sign depends on the other letters"));
            }
            b.sign_of(&v)
        };
        match &c.point {
            Point::PosInf => sample(self, n(100_000_000)),
            Point::NegInf => sample(self, n(-100_000_000)),
            Point::At(a) => {
                let delta = Sym::Num(num_rational::BigRational::new(1.into(), num_bigint::BigInt::from(10).pow(20)));
                let right = |b: &mut Self| sample(b, add(vec![a.clone(), delta.clone()]));
                let left = |b: &mut Self| sample(b, add(vec![a.clone(), neg(delta.clone())]));
                match c.side {
                    1 => right(self),
                    -1 => left(self),
                    _ => {
                        let (l, r) = (left(self)?, right(self)?);
                        if l != r {
                            return Err(EngineError::new(format!(
                                "The limit doesn't exist: it goes to {}∞ from the left and {}∞ from the right",
                                if l < 0 { "−" } else { "" },
                                if r < 0 { "−" } else { "" }
                            )));
                        }
                        Ok(r)
                    }
                }
            }
        }
    }

    fn quotient(&mut self, c: &mut Ctx, top: &Sym, bottom: &Sym, depth: u32) -> R<Lim> {
        let lb = self.lim(c, bottom, depth + 1)?;
        let lt = match self.lim(c, top, depth + 1) {
            Ok(l) => l,
            // sin x / x as x → ∞: bounded over unbounded.
            Err(_) if bounded(top) && lb != Lim::Fin(n(0)) && matches!(lb, Lim::Pos | Lim::Neg) => {
                c.steps.push(Step {
                    latex: format!("{}{}=0", c.lim_latex(), latex(&div(top.clone(), bottom.clone())?)),
                    note: "The top stays bounded while the bottom grows without limit".into(),
                });
                return Ok(Lim::Fin(n(0)));
            }
            Err(e) => return Err(e),
        };
        match (&lt, &lb) {
            (Lim::Fin(p), Lim::Fin(q)) => {
                let (pz, qz) = (self.is_zero_lim(p), self.is_zero_lim(q));
                match (pz, qz) {
                    (_, false) => Ok(Lim::Fin(div(p.clone(), q.clone())?)),
                    (true, true) => self.lhopital(c, top, bottom, "0", depth),
                    (false, true) => {
                        let s = self.sign_of(p)? * self.sign_near(c, bottom)?;
                        Ok(Lim::infinite(s))
                    }
                }
            }
            (Lim::Fin(_), _) => Ok(Lim::Fin(n(0))),
            (_, Lim::Fin(q)) => {
                let s = if self.is_zero_lim(q) { self.sign_near(c, bottom)? } else { self.sign_of(q)? };
                Ok(Lim::infinite(lt.sign_inf() * s))
            }
            _ => {
                // At ±∞, compare leading powers: (√(x²+x) + x) ~ 2x.
                let flip = c.point == Point::NegInf;
                if c.point != Point::PosInf && !flip {
                    return self.lhopital(c, top, bottom, "\\infty", depth);
                }
                match (leading(top, &c.x, flip), leading(bottom, &c.x, flip)) {
                    (Some((ct, pt)), Some((cb, pb))) => {
                        let shown = |coef: &Sym, p: &num_rational::BigRational| {
                            latex(&mul(vec![coef.clone(), sym::pow_or_keep(Sym::Var(c.x.clone()), Sym::Num(p.clone()))]))
                        };
                        c.steps.push(Step {
                            latex: format!("{}\\frac{{{}}}{{{}}}", c.lim_latex(), shown(&ct, &pt), shown(&cb, &pb)),
                            note: "Only the highest powers matter as $x$ grows: divide top and bottom by the highest power".into(),
                        });
                        let ratio = div(ct, cb)?;
                        Ok(if pt == pb {
                            Lim::Fin(ratio)
                        } else if pt < pb {
                            Lim::Fin(n(0))
                        } else {
                            // The coefficients already carry the sign of x → −∞ (x = −y).
                            Lim::infinite(self.sign_of(&ratio)?)
                        })
                    }
                    _ => self.lhopital(c, top, bottom, "\\infty", depth),
                }
            }
        }
    }

    fn lhopital(&mut self, c: &mut Ctx, top: &Sym, bottom: &Sym, form: &str, depth: u32) -> R<Lim> {
        if c.lhopital >= MAX_LHOPITAL {
            return Err(unknown());
        }
        c.lhopital += 1;
        let angle: AngleMode = self.ev.angle;
        let dt = calculus::diff(top, &c.x, angle)?;
        let db = calculus::diff(bottom, &c.x, angle)?;
        let next = div(dt.clone(), db.clone())?;
        c.steps.push(Step {
            latex: format!(
                "{l}\\frac{{{}}}{{{}}}={l}\\frac{{{}}}{{{}}}",
                latex(top),
                latex(bottom),
                latex(&dt),
                latex(&db),
                l = c.lim_latex()
            ),
            note: format!("The form $\\frac{{{form}}}{{{form}}}$: L'Hôpital's rule, differentiate the top and the bottom"),
        });
        self.lim(c, &next, depth + 1)
    }

    fn sum_limit(&mut self, c: &mut Ctx, terms: &[Sym], depth: u32) -> R<Lim> {
        let mut fin = Vec::new();
        let (mut pos, mut negs) = (false, false);
        for t in terms {
            match self.lim(c, t, depth + 1) {
                Ok(Lim::Fin(s)) => fin.push(s),
                Ok(Lim::Pos) => pos = true,
                Ok(Lim::Neg) => negs = true,
                // 1/x alone has no two-sided limit at 0, but 1/x − 1/sin x does.
                Err(e) => {
                    pos = true;
                    negs = true;
                    if common_denominator(terms)?.is_none() {
                        return Err(e);
                    }
                }
            }
        }
        match (pos, negs) {
            (false, false) => Ok(Lim::Fin(add(fin))),
            (true, false) => Ok(Lim::Pos),
            (false, true) => Ok(Lim::Neg),
            (true, true) => {
                // ∞ − ∞: √a − b = (a − b²)/(√a + b), or one fraction over a common denominator.
                if let [a, b] = terms
                    && let Some((top, bottom)) = conjugate(a, b)?
                {
                    c.steps.push(Step {
                        latex: format!("{}\\frac{{{}}}{{{}}}", c.lim_latex(), latex(&top), latex(&bottom)),
                        note: "The form $\\infty-\\infty$: multiply top and bottom by the conjugate".into(),
                    });
                    return self.quotient(c, &top, &bottom, depth + 1);
                }
                if let Some((top, bottom)) = common_denominator(terms)? {
                    c.steps.push(Step {
                        latex: format!("{}\\frac{{{}}}{{{}}}", c.lim_latex(), latex(&top), latex(&bottom)),
                        note: "The form $\\infty-\\infty$: write it as one fraction".into(),
                    });
                    return self.quotient(c, &top, &bottom, depth + 1);
                }
                Err(EngineError::new("This limit has the form ∞ − ∞, which I can't resolve yet"))
            }
        }
    }

    fn product_limit(&mut self, c: &mut Ctx, factors: &[Sym], depth: u32) -> R<Lim> {
        let mut lims = Vec::new();
        for f in factors {
            lims.push(self.lim(c, f, depth + 1)?);
        }
        let zero: Vec<bool> = lims.iter().map(|l| matches!(l, Lim::Fin(s) if s.is_zero())).collect();
        let infinite = lims.iter().any(|l| l.sign_inf() != 0);
        if !infinite {
            return Ok(Lim::Fin(mul(lims.into_iter().map(|l| l.to_sym()).collect())));
        }
        if zero.iter().any(|z| *z) {
            // 0·∞: one group moves to the bottom as 1/(…). Inverting a power of x or an
            // exponential keeps the derivatives simple: x ln x = ln x/(1/x), x·eˣ = x/e^{−x},
            // x ln(1 + 1/x) = ln(1 + 1/x)/(1/x).
            let simple = |f: &Sym| match f {
                Sym::Var(_) => true,
                Sym::Pow(b, e) => matches!(**b, Sym::Var(_)) && !e.has_vars() || **b == Sym::Const(Const::E),
                _ => false,
            };
            let zero_simple = factors.iter().zip(&zero).any(|(f, z)| *z && simple(f));
            let inf_simple = factors.iter().zip(&lims).any(|(f, l)| l.sign_inf() != 0 && simple(f));
            let invert_zero = zero_simple || !inf_simple;
            let (mut top, mut bottom) = (Vec::new(), Vec::new());
            for ((f, z), l) in factors.iter().zip(&zero).zip(&lims) {
                let invert = if invert_zero { *z } else { l.sign_inf() != 0 };
                if invert { bottom.push(pow(f.clone(), n(-1))?) } else { top.push(f.clone()) }
            }
            let (top, bottom) = (mul(top), mul(bottom));
            c.steps.push(Step {
                latex: format!("{}\\frac{{{}}}{{{}}}", c.lim_latex(), latex(&top), latex(&bottom)),
                note: "The form $0\\cdot\\infty$: rewrite it as a quotient".into(),
            });
            return self.quotient(c, &top, &bottom, depth + 1);
        }
        let mut sign = 1;
        for l in &lims {
            sign *= match l {
                Lim::Fin(s) => self.sign_of(s)?,
                inf => inf.sign_inf(),
            };
        }
        Ok(Lim::infinite(sign))
    }

    fn power_limit(&mut self, c: &mut Ctx, base: &Sym, e: &Sym, depth: u32) -> R<Lim> {
        if free_of(e, &c.x) {
            return Ok(match self.lim(c, base, depth + 1)? {
                Lim::Fin(l) => Lim::Fin(pow(l, e.clone())?),
                Lim::Pos => {
                    if e.is_zero() { Lim::Fin(n(1)) } else { Lim::Pos }
                }
                Lim::Neg => match e.as_num() {
                    Some(k) if k.is_integer() => Lim::infinite(if k.to_integer() % 2 == 0.into() { 1 } else { -1 }),
                    _ => return Err(EngineError::new("A fractional power of a negative number isn't real")),
                },
            });
        }
        if free_of(base, &c.x) {
            let one = self.sign_of(&add(vec![base.clone(), n(-1)]))?;
            return Ok(match (self.lim(c, e, depth + 1)?, one) {
                (Lim::Fin(l), _) => Lim::Fin(pow(base.clone(), l)?),
                (_, 0) => Lim::Fin(n(1)),
                (Lim::Pos, 1) | (Lim::Neg, -1) => Lim::Pos,
                _ => Lim::Fin(n(0)),
            });
        }
        // f^g = e^{g ln f}, which covers 1^∞, 0⁰ and ∞⁰.
        let inner = mul(vec![e.clone(), sym::ln(base.clone())?]);
        c.steps.push(Step {
            latex: format!("{}{}", c.lim_latex(), latex(&inner)),
            note: "Write $f^g$ as $e^{g\\ln f}$ and find the limit of the exponent".into(),
        });
        Ok(match self.lim(c, &inner, depth + 1)? {
            Lim::Fin(l) => Lim::Fin(pow(Sym::Const(Const::E), l)?),
            Lim::Pos => Lim::Pos,
            Lim::Neg => Lim::Fin(n(0)),
        })
    }

    fn func_limit(&mut self, c: &mut Ctx, g: Func, u: &Sym, depth: u32) -> R<Lim> {
        let degrees = self.ev.angle == AngleMode::Degrees;
        let quarter = |k: i64| {
            if degrees { n(90 * k) } else { mul(vec![sym::q(k, 2), Sym::Const(Const::Pi)]) }
        };
        let oscillates = || EngineError::new("The limit doesn't exist: the function keeps oscillating");
        match self.lim(c, u, depth + 1)? {
            Lim::Fin(l) => {
                if matches!(g, Func::Ln | Func::Log) && self.is_zero_lim(&l) {
                    return Ok(Lim::Neg);
                }
                if g == Func::Tan && self.func(g, l.clone()).is_err() {
                    let q = div(self.func(Func::Sin, u.clone())?, self.func(Func::Cos, u.clone())?)?;
                    return self.lim(c, &q, depth + 1);
                }
                Ok(Lim::Fin(self.func(g, l).map_err(|_| unknown())?))
            }
            Lim::Pos => match g {
                Func::Ln | Func::Log | Func::Sqrt | Func::Sinh | Func::Cosh | Func::Asinh | Func::Acosh | Func::Gamma | Func::Abs
                | Func::Floor | Func::Ceil | Func::Exp => Ok(Lim::Pos),
                Func::Atan => Ok(Lim::Fin(quarter(1))),
                Func::Tanh => Ok(Lim::Fin(n(1))),
                Func::Sin | Func::Cos | Func::Tan => Err(oscillates()),
                _ => Err(unknown()),
            },
            Lim::Neg => match g {
                Func::Sinh | Func::Asinh | Func::Floor | Func::Ceil => Ok(Lim::Neg),
                Func::Cosh | Func::Abs => Ok(Lim::Pos),
                Func::Exp => Ok(Lim::Fin(n(0))),
                Func::Atan => Ok(Lim::Fin(quarter(-1))),
                Func::Tanh => Ok(Lim::Fin(n(-1))),
                Func::Sin | Func::Cos | Func::Tan => Err(oscillates()),
                _ => Err(unknown()),
            },
        }
    }
}

/// ∞ and −∞ as points; anything else with ∞ in it is an error.
pub fn point_of(s: &Sym) -> R<Point> {
    let inf = Sym::Const(Const::Inf);
    Ok(if *s == inf {
        Point::PosInf
    } else if *s == neg(inf) {
        Point::NegInf
    } else if contains_inf(s) {
        return Err(EngineError::new("An infinite value can only stand on its own (∞ plus or times anything else isn't a number)"));
    } else {
        Point::At(s.clone())
    })
}

/// True when sin, cos or tan of something with x in it appears.
fn oscillates(s: &Sym, x: &str) -> bool {
    match s {
        Sym::Func(Func::Sin | Func::Cos | Func::Tan, a) => !free_of(a, x),
        Sym::Func(_, a) | Sym::Factorial(a, _) => oscillates(a, x),
        Sym::Add(v) | Sym::Mul(v) => v.iter().any(|t| oscillates(t, x)),
        Sym::Pow(a, b) | Sym::Log(a, b) => oscillates(a, x) || oscillates(b, x),
        _ => false,
    }
}

/// True when ∞ appears outside the bounds of a numeric sum or integral.
pub fn contains_inf(s: &Sym) -> bool {
    match s {
        Sym::Const(c) => *c == Const::Inf,
        Sym::Add(v) | Sym::Mul(v) => v.iter().any(contains_inf),
        Sym::Pow(a, b) | Sym::Log(a, b) => contains_inf(a) || contains_inf(b),
        Sym::Func(_, a) | Sym::Factorial(a, _) => contains_inf(a),
        _ => false,
    }
}

impl Builder<'_> {
    /// `\lim_{x\to a} body`.
    pub(crate) fn limit_expr(&mut self, body: &crate::ast::Expr, x: &str, to: &crate::ast::Expr, side: i8) -> R<Sym> {
        let f = self.build_free(body, &[x.to_string()])?;
        let point = point_of(&self.build(to)?)?;
        if side != 0 && !matches!(point, Point::At(_)) {
            return Err(EngineError::new("Only a limit at a number has sides"));
        }
        let (l, steps) = self.limit(&f, x, point, side)?;
        self.ev.work.push(crate::Method { name: "Limit".into(), steps });
        Ok(l.to_sym())
    }

    /// F at an end of the interval: substituted, or the one-sided limit from inside (`side`)
    /// where F isn't defined (x ln x − x at 0). An infinite limit means divergence.
    pub(crate) fn endpoint(&mut self, big_f: &Sym, x: &str, at: &Sym, side: i8) -> R<Sym> {
        if let Some(v) = self.continuous_value(&Ctx { x: x.to_string(), point: Point::At(at.clone()), side, steps: Vec::new(), lhopital: 0 }, big_f, at)? {
            return Ok(v);
        }
        match self.limit(big_f, x, Point::At(at.clone()), side)?.0 {
            Lim::Fin(v) => Ok(v),
            _ => Err(EngineError::new("This integral diverges: the antiderivative is infinite at an end")),
        }
    }

    /// ∫ with an infinite bound: lim F(b) − F(a) when there is an antiderivative, else numeric.
    pub(crate) fn improper_integral(&mut self, f: Sym, x: &str, a: Sym, b: Sym, antiderivative: Option<Sym>, mut steps: Vec<Step>) -> R<Sym> {
        let (pa, pb) = (point_of(&a)?, point_of(&b)?);
        let shown = format!("\\int_{{{}}}^{{{}}}{}\\,\\mathrm{{d}}{x}", pa.latex(), pb.latex(), latex(&f));
        if let Some(big_f) = antiderivative {
            steps.push(Step { latex: format!("F({x})={}", latex(&big_f)), note: "An antiderivative".into() });
            let end = |b: &mut Self, p: &Point, steps: &mut Vec<Step>| -> R<Lim> {
                match p {
                    Point::At(v) => Ok(Lim::Fin(b.subst(&big_f, x, v)?)),
                    inf => {
                        let (l, _) = b.limit(&big_f, x, inf.clone(), 0)?;
                        steps.push(Step {
                            latex: format!("\\lim_{{{x}\\to {}}}F({x})={}", inf.latex(), latex(&l.to_sym())),
                            note: "The infinite end is a limit".into(),
                        });
                        Ok(l)
                    }
                }
            };
            let fb = end(self, &pb, &mut steps)?;
            let fa = end(self, &pa, &mut steps)?;
            let result = match (fb, fa) {
                (Lim::Fin(u), Lim::Fin(l)) => add(vec![u, neg(l)]),
                (u, l) => {
                    let s = u.sign_inf() - l.sign_inf();
                    steps.push(Step { latex: format!("{shown}={}", if s > 0 { "\\infty" } else { "-\\infty" }), note: "It grows without limit: the integral diverges".into() });
                    self.ev.work.push(crate::Method { name: "Integral".into(), steps });
                    return Err(EngineError::new("This integral diverges"));
                }
            };
            steps.push(Step { latex: format!("{shown}={}", latex(&result)), note: "Evaluate $F(b)-F(a)$ with the limits".into() });
            self.ev.work.push(crate::Method { name: "Integral".into(), steps });
            return Ok(result);
        }
        if f.has_vars_except(x) || a.has_vars() || b.has_vars() {
            return Err(EngineError::new("I can't find an antiderivative for this yet, and it has letters, so it can't be done numerically"));
        }
        // sin x / x over [0, ∞) never settles numerically.
        if oscillates(&f, x) {
            return Err(EngineError::new("I can't do oscillating integrals over an infinite range yet"));
        }
        self.ev.work.push(crate::Method {
            name: "Numeric integral".into(),
            steps: vec![Step {
                latex: shown,
                note: "No antiderivative from the rules: map the infinite range onto $[0,1)$ with $x=\\frac{u}{1-u}$ and use tanh-sinh quadrature".into(),
            }],
        });
        Ok(Sym::Integral { body: Box::new(f), var: x.to_string(), lo: Box::new(a), hi: Box::new(b) })
    }

    /// Taylor coefficients c₀ … c_n of f about a: f⁽ᵏ⁾(a)/k! by differentiating and
    /// substituting; for a quotient that is 0/0 at a (sin x / x), the top's series divided by
    /// the bottom's; otherwise each f⁽ᵏ⁾(a) as a limit.
    fn taylor_coeffs(&mut self, f: &Sym, x: &str, a: &Sym, order: usize, steps: &mut Vec<Step>, depth: u32) -> R<Vec<Sym>> {
        let blows_up = || EngineError::new(format!("The function has no Taylor series at {}: it blows up there", latex(a)));
        if depth > 4 {
            return Err(blows_up());
        }
        let angle = self.ev.angle;
        let show = depth == 0;
        // 1. Differentiate and substitute.
        let mut d = f.clone();
        let mut factorial = num_bigint::BigInt::from(1);
        let mut coeffs = Vec::new();
        let mut direct = true;
        let mut local = Vec::new();
        for k in 0..=order {
            if k > 0 {
                d = calculus::diff(&d, x, angle)?;
                factorial *= k;
            }
            let value = match self.subst(&d, x, a) {
                Ok(v) if v.has_vars() || self.numeric(&v).is_ok() => v,
                _ => {
                    direct = false;
                    break;
                }
            };
            let primes = match k {
                0 => String::new(),
                1 => "'".into(),
                2 => "''".into(),
                k => format!("^{{({k})}}"),
            };
            if k <= 6 {
                local.push(Step {
                    latex: format!("f{primes}({x})={},\\quad f{primes}({})={}", latex(&d), latex(a), latex(&value)),
                    note: if k == 0 { "The function at the point".into() } else { "Differentiate, then substitute".into() },
                });
            }
            coeffs.push(div(value, Sym::Num(num_rational::BigRational::from_integer(factorial.clone())))?);
        }
        if direct {
            if show {
                steps.extend(local);
            }
            return Ok(coeffs);
        }
        // 2. A quotient: divide the series, after the zeros at a cancel.
        if let Some((top, bottom)) = split_quotient(f)? {
            const EXTRA: usize = 4;
            let nt = self.taylor_coeffs(&top, x, a, order + EXTRA, steps, depth + 1)?;
            let nb = self.taylor_coeffs(&bottom, x, a, order + EXTRA, steps, depth + 1)?;
            let m = nb.iter().position(|c| !c.is_zero()).ok_or_else(blows_up)?;
            if m > EXTRA || nt.iter().take(m).any(|c| !c.is_zero()) {
                return Err(blows_up());
            }
            let (t, b) = (&nt[m..], &nb[m..]);
            let mut q: Vec<Sym> = Vec::new();
            for k in 0..=order {
                let mut acc = vec![t[k].clone()];
                for j in 1..=k {
                    acc.push(neg(mul(vec![b[j].clone(), q[k - j].clone()])));
                }
                q.push(crate::poly::expand(&div(add(acc), b[0].clone())?)?);
            }
            if show {
                let series = |c: &[Sym]| {
                    let s = add(c.iter().take(order + 1).enumerate().map(|(k, c)| mul(vec![c.clone(), sym::pow_or_keep(Sym::Var(x.into()), n(k as i64))])).collect());
                    latex(&s)
                };
                steps.push(Step {
                    latex: format!("\\frac{{{}+\\cdots}}{{{}+\\cdots}}", series(&nt), series(&nb)),
                    note: "Substituting gives $\\frac{0}{0}$: expand the top and the bottom, then divide the series".into(),
                });
            }
            return Ok(q);
        }
        // 3. Each coefficient as a limit.
        let mut d = f.clone();
        let mut factorial = num_bigint::BigInt::from(1);
        let mut coeffs = Vec::new();
        for k in 0..=order {
            if k > 0 {
                d = calculus::diff(&d, x, angle)?;
                factorial *= k;
            }
            let value = match self.subst(&d, x, a) {
                Ok(v) if v.has_vars() || self.numeric(&v).is_ok() => v,
                _ => match self.limit(&d, x, Point::At(a.clone()), 0).map_err(|_| blows_up())?.0 {
                    Lim::Fin(v) => v,
                    _ => return Err(blows_up()),
                },
            };
            coeffs.push(div(value, Sym::Num(num_rational::BigRational::from_integer(factorial.clone())))?);
        }
        if show {
            steps.push(Step { latex: String::new(), note: "Where substituting gives $\\frac{0}{0}$, each derivative's value is a limit".into() });
        }
        Ok(coeffs)
    }

    /// `taylor(f, a, n)`: Σ f⁽ᵏ⁾(a)/k! (x − a)ᵏ for k ≤ n.
    pub(crate) fn taylor(&mut self, body: &crate::ast::Expr, var: Option<&str>, at: &crate::ast::Expr, order: u32) -> R<Sym> {
        let probe = self.build(body)?;
        let mut vars = std::collections::BTreeSet::new();
        calculus::free_vars(&probe, &mut vars);
        let x = match var {
            Some(v) => v.to_string(),
            None if vars.len() == 1 => vars.pop_first().expect("one variable"),
            None if vars.is_empty() || vars.contains("x") => "x".to_string(),
            None => return Err(EngineError::new("Say which variable: taylor(f, x, a, n)")),
        };
        let f = self.build_free(body, std::slice::from_ref(&x))?;
        let a = self.build(at)?;
        if contains_inf(&a) {
            return Err(EngineError::new("A Taylor series is about a number, not ∞"));
        }
        let mut steps = Vec::new();
        let coeffs = self.taylor_coeffs(&f, &x, &a, order as usize, &mut steps, 0)?;
        let shift = sym::normalize_complex(add(vec![Sym::Var(x.clone()), neg(a.clone())]));
        let poly = add(coeffs.into_iter().enumerate().map(|(k, c)| mul(vec![c, sym::pow_or_keep(shift.clone(), n(k as i64))])).collect());
        steps.push(Step {
            latex: format!("\\sum_{{k=0}}^{{{order}}}\\frac{{f^{{(k)}}({})}}{{k!}}{}^{{k}}={}", latex(&a), if a.is_zero() { x.clone() } else { format!("\\left({}\\right)", latex(&shift)) }, latex(&poly)),
            note: "Taylor's formula".into(),
        });
        self.ev.work.push(crate::Method { name: "Taylor series".into(), steps });
        Ok(poly)
    }
}

/// The leading term c·x^p of an algebraic expression as x → ∞ (as x → −∞ when `flip`,
/// with x = −y), or None when it can't be told, as when leading terms cancel.
fn leading(s: &Sym, x: &str, flip: bool) -> Option<(Sym, num_rational::BigRational)> {
    use num_rational::BigRational as Q;
    use num_traits::Zero;
    if free_of(s, x) {
        return Some((s.clone(), Q::zero()));
    }
    match s {
        Sym::Var(v) if v == x => Some((n(if flip { -1 } else { 1 }), Q::from_integer(1.into()))),
        Sym::Mul(v) => {
            let mut c = Vec::new();
            let mut p = Q::zero();
            for f in v {
                let (cf, pf) = leading(f, x, flip)?;
                c.push(cf);
                p += pf;
            }
            Some((mul(c), p))
        }
        Sym::Add(v) => {
            let parts: Vec<(Sym, Q)> = v.iter().map(|t| leading(t, x, flip)).collect::<Option<_>>()?;
            let top = parts.iter().map(|(_, p)| p.clone()).max()?;
            let c = add(parts.into_iter().filter(|(_, p)| *p == top).map(|(c, _)| c).collect());
            if c.is_zero() || c.has_vars() && !c.has_vars_except(x) {
                return None;
            }
            Some((c, top))
        }
        Sym::Pow(b, e) => {
            let k = e.as_num()?.clone();
            let (cb, pb) = leading(b, x, flip)?;
            // A fractional power needs a positive leading coefficient.
            if !k.is_integer() && !matches!(cb.as_num(), Some(v) if *v > Q::zero()) {
                return None;
            }
            Some((pow(cb, Sym::Num(k.clone())).ok()?, pb * k))
        }
        _ => None,
    }
}

/// √a − b as (a − b²)/(√a + b) (either term may be the square root).
fn conjugate(a: &Sym, b: &Sym) -> R<Option<(Sym, Sym)>> {
    let is_root = |s: &Sym| {
        let (_, rest) = sym::split_coeff(s.clone());
        matches!(&rest, Sym::Pow(_, e) if e.as_num().is_some_and(|k| *k.denom() == 2.into()))
    };
    if !is_root(a) && !is_root(b) {
        return Ok(None);
    }
    let sq = |s: &Sym| pow(s.clone(), n(2));
    let top = crate::poly::expand(&add(vec![sq(a)?, neg(sq(b)?)]))?;
    let bottom = add(vec![a.clone(), neg(b.clone())]);
    Ok(Some((top, bottom)))
}

/// Σ nᵢ/dᵢ as one fraction, when some term has a denominator.
fn common_denominator(terms: &[Sym]) -> R<Option<(Sym, Sym)>> {
    let mut parts = Vec::new();
    for t in terms {
        parts.push(split_quotient(t)?.unwrap_or((t.clone(), n(1))));
    }
    if parts.iter().all(|(_, d)| d.is_one()) {
        return Ok(None);
    }
    let bottom = mul(parts.iter().map(|(_, d)| d.clone()).collect());
    let top = add(
        (0..parts.len())
            .map(|i| {
                let mut f = vec![parts[i].0.clone()];
                f.extend(parts.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, (_, d))| d.clone()));
                mul(f)
            })
            .collect(),
    );
    Ok(Some((crate::poly::expand(&top)?, bottom)))
}
