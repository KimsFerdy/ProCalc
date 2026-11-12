//! Evaluates an [`Expr`]: stays exact (rational) whenever the maths allows,
//! and falls back to arbitrary-precision floats otherwise.

use astro_float::BigFloat;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::ast::{BinOp, Const, Expr, Func};
use crate::complex::{self, C};
use crate::number::{self, Prec, RM, Value, int};
use crate::{AngleMode, EngineError};

/// Largest exact result we are willing to build, in bits (~300k decimal digits).
pub(crate) const MAX_EXACT_BITS: u64 = 1_000_000;
pub(crate) const MAX_FACTORIAL: i64 = 20_000;

pub struct Evaluator {
    pub prec: Prec,
    pub angle: AngleMode,
    /// User function calls made so far (see `defs::MAX_CALLS`).
    pub calls: usize,
    /// Values for variables: the integration variable during numeric integration, or the
    /// sample points when an antiderivative is checked.
    pub vars: std::collections::BTreeMap<String, Value>,
    /// Working collected while a line is built (derivatives, integrals…), for the Steps view.
    pub work: Vec<crate::Method>,
}

type R = Result<Value, EngineError>;

pub(crate) fn zeroth_root_err() -> EngineError {
    EngineError::new("The 0th root is undefined")
}

/// A complex result as a `Value`: real when the imaginary part is exactly zero.
pub(crate) fn from_c(c: C) -> Value {
    if c.im.is_zero() { Value::Real(c.re) } else { Value::Complex(c.re, c.im) }
}

fn is_complex(v: &Value) -> bool {
    matches!(v, Value::Complex(..))
}

impl Evaluator {
    fn cx(&mut self, v: &Value) -> C {
        match v {
            Value::Complex(re, im) => C::new(re.clone(), im.clone()),
            other => {
                let f = self.prec.float_of(other);
                complex::real(f, &self.prec)
            }
        }
    }

    pub fn eval(&mut self, e: &Expr) -> R {
        match e {
            Expr::Number(s) => number::parse_exact(s)
                .map(Value::Exact)
                .ok_or_else(|| EngineError::new(format!("Bad number '{s}'"))),
            Expr::Const(Const::Pi) => Ok(Value::Real(self.prec.cc.pi(self.prec.bits, RM))),
            Expr::Const(Const::E) => Ok(Value::Real(self.prec.cc.e(self.prec.bits, RM))),
            Expr::Const(Const::I) => {
                let p = self.prec.bits;
                Ok(Value::Complex(BigFloat::from_word(0, p), BigFloat::from_word(1, p)))
            }
            Expr::Var(v) => self.vars.get(v).cloned().ok_or_else(|| EngineError::new(format!("{v} has no value"))),
            Expr::Const(Const::Inf) => Err(EngineError::new("Infinity isn't a number you can calculate with")),
            // a × b between numbers is plain multiplication.
            Expr::MatCall(crate::ast::MatFn::Times, args) if args.len() == 2 => {
                let (a, b) = (self.eval(&args[0])?, self.eval(&args[1])?);
                self.binary(BinOp::Mul, a, b)
            }
            Expr::Matrix(_) | Expr::MatCall(..) => Err(EngineError::new("Matrices need the symbolic engine")),
            Expr::StatCall(..) | Expr::Optimize(..) | Expr::Geometry(..) => Err(EngineError::new("This needs the symbolic engine")),
            Expr::OdeFn { name, .. } => Err(EngineError::new(format!("{name} is the unknown of a differential equation: write one, like {name}' = 2{name}"))),
            Expr::Integral { body, var, bounds: Some((lo, hi)) } => {
                // ∞ bounds map the range onto [0, 1).
                let range = match (infinity_sign(lo), infinity_sign(hi)) {
                    (None, None) => {
                        let a = self.eval(lo)?;
                        let b = self.eval(hi)?;
                        return crate::calculus::quadrature(body, var, &a, &b, self);
                    }
                    (None, Some(1)) => crate::calculus::Range::Above(self.eval(lo)?),
                    (Some(-1), None) => crate::calculus::Range::Below(self.eval(hi)?),
                    (Some(-1), Some(1)) => crate::calculus::Range::Whole,
                    _ => return Err(EngineError::new("Put the smaller bound at the bottom of an infinite integral")),
                };
                crate::calculus::quadrature_infinite(body, var, range, self)
            }
            Expr::Sum { body, var, lo, hi, product: false } => crate::sums::eval_sum(body, var, lo, hi, self),
            Expr::Sum { product: true, .. } | Expr::Limit { .. } | Expr::Taylor { .. } | Expr::PolyDiv { .. } => {
                Err(EngineError::new("This needs the symbolic engine"))
            }
            // The symbolic layer does these before anything reaches here.
            Expr::Integral { bounds: None, .. }
            | Expr::Deriv { .. }
            | Expr::UserDeriv { .. }
            | Expr::Subst(..)
            | Expr::BarEval(..) => Err(EngineError::new("This needs the symbolic engine")),
            // The symbolic layer expands user calls before anything reaches here.
            Expr::UserCall(name, _) => Err(EngineError::new(format!("{name} isn't defined"))),
            // Number functions need whole numbers, which the symbolic layer works out exactly.
            Expr::NumCall(f, _) => Err(EngineError::new(format!("{} needs exact numbers", f.name()))),
            // Units are taken off by `units::resolve` before numeric evaluation; one that
            // reaches here sits somewhere units can't go, like floor(2.5 m).
            Expr::Unit(_) | Expr::Convert(..) => Err(EngineError::new("Units can't be used here")),
            Expr::Neg(x) => Ok(match self.eval(x)? {
                Value::Exact(r) => Value::Exact(-r),
                Value::Real(f) => Value::Real(f.neg()),
                Value::Complex(re, im) => Value::Complex(re.neg(), im.neg()),
            }),
            Expr::Binary(op, a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                self.binary(*op, a, b)
            }
            // e^y straight through exp: faster than the general power, and as accurate.
            Expr::Pow(a, b) if **a == Expr::Const(Const::E) => {
                let b = self.eval(b)?;
                self.call(Func::Exp, b)
            }
            Expr::Pow(a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                self.pow(a, b)
            }
            Expr::Root(n, x) => {
                let n = self.eval(n)?;
                let x = self.eval(x)?;
                self.root(n, x)
            }
            Expr::LogBase(b, x) => {
                let b = self.eval(b)?;
                let x = self.eval(x)?;
                self.log_base(b, x)
            }
            Expr::Factorial(x, step) => {
                let x = self.eval(x)?;
                self.factorial(x, *step)
            }
            // Rewriting never changes the value.
            Expr::Op(_, x) => self.eval(x),
            Expr::Percent(x) => {
                let x = self.eval(x)?;
                self.binary(BinOp::Div, x, Value::Exact(int(100)))
            }
            Expr::Call(f, x) => {
                let trig = matches!(f, Func::Sin | Func::Cos | Func::Tan);
                // `sin 30°` means degrees whatever the angle mode is.
                let (arg, angle) = match as_explicit_degrees(x) {
                    Some(inner) if trig => (inner, AngleMode::Degrees),
                    _ => (&**x, self.angle),
                };
                let x = self.eval(arg)?;
                if trig && angle == AngleMode::Degrees
                    && let Value::Exact(d) = &x
                        && let Some(v) = exact_trig(*f, d)? {
                            return Ok(v);
                        }
                let saved = std::mem::replace(&mut self.angle, angle);
                let out = self.call(*f, x);
                self.angle = saved;
                out
            }
        }
    }

    fn f(&mut self, v: &Value) -> BigFloat {
        self.prec.float_of(v)
    }

    fn binary(&mut self, op: BinOp, a: Value, b: Value) -> R {
        let p = self.prec.bits;
        if let (Value::Exact(x), Value::Exact(y)) = (&a, &b) {
            return Ok(Value::Exact(match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => {
                    if y.is_zero() {
                        return Err(EngineError::new("Division by zero"));
                    }
                    x / y
                }
            }));
        }
        if is_complex(&a) || is_complex(&b) {
            let (x, y) = (self.cx(&a), self.cx(&b));
            let pr = &self.prec;
            return Ok(from_c(match op {
                BinOp::Add => complex::add(&x, &y, pr),
                BinOp::Sub => complex::sub(&x, &y, pr),
                BinOp::Mul => complex::mul(&x, &y, pr),
                BinOp::Div => complex::div(&x, &y, pr)?,
            }));
        }
        let (x, y) = (self.f(&a), self.f(&b));
        Ok(Value::Real(match op {
            BinOp::Add => x.add(&y, p, RM),
            BinOp::Sub => x.sub(&y, p, RM),
            BinOp::Mul => x.mul(&y, p, RM),
            BinOp::Div => {
                if y.is_zero() {
                    return Err(EngineError::new("Division by zero"));
                }
                x.div(&y, p, RM)
            }
        }))
    }

    fn pow(&mut self, a: Value, b: Value) -> R {
        if let (Value::Exact(base), Value::Exact(exp)) = (&a, &b)
            && let Some(v) = exact_pow(base, exp)? {
                return Ok(Value::Exact(v));
            }
        if is_complex(&a) || is_complex(&b) {
            return self.complex_pow(a, b);
        }
        let x = self.f(&a);
        let y = self.f(&b);
        if x.is_zero() {
            if number::is_negative_float(&y) {
                return Err(EngineError::new("Division by zero (0 to a negative power)"));
            }
            return Ok(Value::Real(x));
        }
        if number::is_negative_float(&x) {
            // Negative base: real for integer exponents, or rational ones with an odd denominator
            // ((−8)^⅓ = −2, the calculator convention). Anything else is complex.
            let sign = match &b {
                Value::Exact(r) if r.denom().is_odd_int() => {
                    if r.numer().is_odd_int() { -1 } else { 1 }
                }
                _ => return self.complex_pow(a, b),
            };
            let mag = self.real_pow(&x.abs(), &y, &b);
            return Ok(Value::Real(if sign < 0 { mag.neg() } else { mag }));
        }
        Ok(Value::Real(self.real_pow(&x, &y, &b)))
    }

    /// xʸ for x > 0. Exponent ½ goes through sqrt. Everything else uses pow without
    /// last-bit rounding: astro-float's correctly rounded pow can loop forever when the
    /// true result is exactly representable (a float 4.0 to the 0.5), and the precision
    /// check in `lib.rs` already guards the digits we show.
    fn real_pow(&mut self, x: &BigFloat, y: &BigFloat, exact_y: &Value) -> BigFloat {
        let p = self.prec.bits;
        if let Value::Exact(r) = exact_y {
            if *r == BigRational::new(1.into(), 2.into()) {
                return x.sqrt(p, RM);
            }
            // Small whole powers by repeated multiplication (x² is just x·x).
            if r.is_integer()
                && let Some(k) = r.numer().to_i64().filter(|k| (1..=64).contains(&k.abs()))
            {
                let pos = x.powi(k.unsigned_abs() as usize, p, RM);
                return if k < 0 { pos.reciprocal(p, RM) } else { pos };
            }
        }
        x.pow(y, p, astro_float::RoundingMode::None, &mut self.prec.cc)
    }

    fn complex_pow(&mut self, a: Value, b: Value) -> R {
        let z = self.cx(&a);
        // Integer powers by repeated squaring stay accurate: i² is exactly −1.
        if let Value::Exact(r) = &b
            && r.is_integer()
            && let Some(k) = r.numer().to_i64().filter(|k| k.unsigned_abs() <= 4096)
        {
            return Ok(from_c(complex::powi(&z, k, &self.prec)?));
        }
        let w = self.cx(&b);
        Ok(from_c(complex::pow(&z, &w, &mut self.prec)?))
    }

    fn root(&mut self, n: Value, x: Value) -> R {
        let n = match n {
            Value::Exact(r) if r.is_zero() => return Err(zeroth_root_err()),
            Value::Exact(r) if r.is_integer() && r.is_positive() => r,
            _ => {
                let inv = self.binary(BinOp::Div, Value::Exact(int(1)), n)?;
                return self.pow(x, inv);
            }
        };
        let inv = Value::Exact(BigRational::one() / n);
        self.pow(x, inv)
    }

    pub(crate) fn factorial(&mut self, x: Value, step: u32) -> R {
        let n = match &x {
            Value::Exact(r) => number::to_i64(r),
            Value::Real(f) => self.prec.float_to_int(f).and_then(|i| i.to_i64()),
            Value::Complex(..) => None,
        };
        // (−1)!! = 1 by convention, which keeps n!! = n·(n−2)!! true for n = 1.
        let lowest = if step == 2 { -1 } else { 0 };
        match n {
            Some(n) if (lowest..=MAX_FACTORIAL).contains(&n) => {
                let mut acc = BigInt::one();
                let mut k = n;
                while k > 1 {
                    acc *= k;
                    k -= step as i64;
                }
                Ok(Value::Exact(BigRational::from_integer(acc)))
            }
            Some(n) if n > MAX_FACTORIAL => {
                Err(EngineError::new(format!("Factorial is limited to n ≤ {MAX_FACTORIAL} for now")))
            }
            Some(_) => Err(EngineError::new("Factorial is undefined for negative whole numbers")),
            // x! = Γ(x+1) between the whole numbers.
            None if step == 1 && !is_complex(&x) => {
                let x1 = self.binary(BinOp::Add, x, Value::Exact(BigRational::one()))?;
                self.call(Func::Gamma, x1)
            }
            None => Err(EngineError::new("Factorial needs a real number (and n!! a whole one)")),
        }
    }

    fn log_base(&mut self, b: Value, x: Value) -> R {
        if let (Value::Exact(b), Value::Exact(x)) = (&b, &x)
            && let Some(k) = exact_log(b, x) {
                return Ok(Value::Exact(k));
            }
        let lx = self.call(Func::Ln, x)?;
        let lb = self.call(Func::Ln, b)?;
        if let Value::Real(f) = &lb
            && f.is_zero() {
                return Err(EngineError::new("Logarithm base can't be 1"));
            }
        self.binary(BinOp::Div, lx, lb)
    }

    fn angle_to_rad(&mut self, x: BigFloat) -> BigFloat {
        match self.angle {
            AngleMode::Radians => x,
            AngleMode::Degrees => {
                let p = self.prec.bits;
                let pi = self.prec.cc.pi(p, RM);
                x.mul(&pi, p, RM).div(&BigFloat::from_word(180, p), p, RM)
            }
        }
    }

    fn rad_to_angle(&mut self, x: BigFloat) -> BigFloat {
        match self.angle {
            AngleMode::Radians => x,
            AngleMode::Degrees => {
                let p = self.prec.bits;
                let pi = self.prec.cc.pi(p, RM);
                x.mul(&BigFloat::from_word(180, p), p, RM).div(&pi, p, RM)
            }
        }
    }

    /// Multiplies a complex angle by π/180 (`to_rad`) or 180/π in degree mode.
    fn convert_angle(&mut self, z: C, to_rad: bool) -> C {
        if self.angle == AngleMode::Radians {
            return z;
        }
        let p = self.prec.bits;
        let pi = self.prec.cc.pi(p, RM);
        let k180 = BigFloat::from_word(180, p);
        let k = if to_rad { pi.div(&k180, p, RM) } else { k180.div(&pi, p, RM) };
        complex::scale_by(&z, &k, &self.prec)
    }

    fn complex_call(&mut self, f: Func, x: Value) -> R {
        let z = self.cx(&x);
        let out = match f {
            Func::Abs => return Ok(Value::Real(complex::abs(&z, &self.prec))),
            Func::Floor | Func::Ceil => {
                return Err(EngineError::new("floor and ceil need a real number"));
            }
            Func::Gamma => return Err(EngineError::new("Γ of a complex number isn't supported yet")),
            Func::Erf | Func::Probit => return Err(EngineError::new(format!("{} of a complex number isn't supported", f.latex()))),
            Func::Re => return Ok(Value::Real(z.re)),
            Func::Im => return Ok(Value::Real(z.im)),
            Func::Conj => return Ok(from_c(C::new(z.re, z.im.neg()))),
            Func::Arg => {
                let a = complex::arg(&z, &mut self.prec);
                return Ok(Value::Real(self.rad_to_angle(a)));
            }
            Func::Sqrt => complex::sqrt(&z, &mut self.prec),
            Func::Exp => complex::exp(&z, &mut self.prec),
            Func::Ln => complex::ln(&z, &mut self.prec)?,
            Func::Log => {
                let l = complex::ln(&z, &mut self.prec)?;
                let p = self.prec.bits;
                let ln10 = BigFloat::from_word(10, p).ln(p, RM, &mut self.prec.cc);
                complex::scale_by(&l, &ln10.reciprocal(p, RM), &self.prec)
            }
            Func::Sin | Func::Cos | Func::Tan => {
                let r = self.convert_angle(z, true);
                let pr = &mut self.prec;
                match f {
                    Func::Sin => complex::sin(&r, pr),
                    Func::Cos => complex::cos(&r, pr),
                    _ => complex::tan(&r, pr)?,
                }
            }
            Func::Asin | Func::Acos | Func::Atan => {
                let pr = &mut self.prec;
                let r = match f {
                    Func::Asin => complex::asin(&z, pr)?,
                    Func::Acos => complex::acos(&z, pr)?,
                    _ => complex::atan(&z, pr)?,
                };
                self.convert_angle(r, false)
            }
            Func::Sinh => complex::sinh(&z, &mut self.prec),
            Func::Cosh => complex::cosh(&z, &mut self.prec),
            Func::Tanh => complex::tanh(&z, &mut self.prec)?,
            Func::Asinh => complex::asinh(&z, &mut self.prec)?,
            Func::Acosh => complex::acosh(&z, &mut self.prec)?,
            Func::Atanh => complex::atanh(&z, &mut self.prec)?,
        };
        if out.re.is_nan() || out.im.is_nan() {
            return Err(EngineError::new("Calculation failed (result is undefined)"));
        }
        Ok(from_c(out))
    }

    pub(crate) fn call(&mut self, f: Func, x: Value) -> R {
        if is_complex(&x) {
            return self.complex_call(f, x);
        }
        let p = self.prec.bits;
        // A real number is its own real part and conjugate, at angle 0 or π.
        match f {
            Func::Re | Func::Conj => return Ok(x),
            Func::Im => return Ok(Value::Exact(BigRational::zero())),
            Func::Arg => {
                let negative = match &x {
                    Value::Exact(r) => r.is_negative(),
                    v => number::is_negative_float(&self.prec.float_of(v)),
                };
                if !negative {
                    return Ok(Value::Exact(BigRational::zero()));
                }
                return Ok(match self.angle {
                    AngleMode::Degrees => Value::Exact(int(180)),
                    AngleMode::Radians => Value::Real(self.prec.cc.pi(p, RM)),
                });
            }
            _ => {}
        }
        // Exact cases first.
        if let Value::Exact(r) = &x {
            match f {
                Func::Abs => return Ok(Value::Exact(r.abs())),
                Func::Floor => return Ok(Value::Exact(r.floor())),
                Func::Ceil => return Ok(Value::Exact(r.ceil())),
                // Γ(n) = (n−1)!
                Func::Gamma if r.is_integer() && r.is_positive() => {
                    return self.factorial(Value::Exact(r - BigRational::one()), 1);
                }
                Func::Sqrt => {
                    if let Some(s) = number::exact_root(r, 2) {
                        return Ok(Value::Exact(s));
                    }
                }
                Func::Log => {
                    if let Some(k) = number::exact_log10(r) {
                        return Ok(Value::Exact(int(k)));
                    }
                }
                Func::Ln if r.is_one() => return Ok(Value::Exact(BigRational::zero())),
                Func::Exp if r.is_zero() => return Ok(Value::Exact(BigRational::one())),
                Func::Sin | Func::Tan | Func::Asin | Func::Atan | Func::Sinh | Func::Tanh
                | Func::Asinh | Func::Atanh
                    if r.is_zero() =>
                {
                    return Ok(Value::Exact(BigRational::zero()));
                }
                Func::Cos | Func::Cosh if r.is_zero() => return Ok(Value::Exact(BigRational::one())),
                Func::Acosh if r.is_one() => return Ok(Value::Exact(BigRational::zero())),
                Func::Asin | Func::Acos | Func::Atan if self.angle == AngleMode::Degrees => {
                    if let Some(d) = exact_inverse_trig_degrees(f, r) {
                        return Ok(Value::Exact(d));
                    }
                }
                _ => {}
            }
        }

        let v = self.f(&x);
        let one = BigFloat::from_word(1, p);
        // Real arguments outside the real domain have complex results: √−4 = 2i, asin 2, ln −1.
        let outside = match f {
            Func::Sqrt | Func::Ln | Func::Log => number::is_negative_float(&v),
            Func::Asin | Func::Acos | Func::Atanh => v.abs().cmp(&one).is_some_and(|c| c > 0),
            Func::Acosh => v.cmp(&one).is_some_and(|c| c < 0),
            _ => false,
        };
        if outside {
            return self.complex_call(f, x);
        }
        let cc = &mut self.prec.cc;
        let out = match f {
            Func::Abs => v.abs(),
            Func::Re | Func::Im | Func::Arg | Func::Conj => unreachable!("handled before the exact cases"),
            Func::Floor => {
                let fl = v.floor();
                return Ok(self.prec.float_to_int(&fl).map_or(Value::Real(fl), |i| {
                    Value::Exact(BigRational::from_integer(i))
                }));
            }
            Func::Ceil => {
                let c = v.ceil();
                return Ok(self.prec.float_to_int(&c).map_or(Value::Real(c), |i| {
                    Value::Exact(BigRational::from_integer(i))
                }));
            }
            Func::Sqrt => v.sqrt(p, RM),
            Func::Gamma => crate::gamma::gamma(&v, &mut self.prec)?,
            Func::Erf => crate::special::erf(&v, &mut self.prec),
            Func::Probit => crate::special::probit(&v, &mut self.prec)?,
            Func::Exp => v.exp(p, RM, cc),
            Func::Ln | Func::Log => {
                if v.is_zero() {
                    return Err(EngineError::new("Logarithm of zero is undefined"));
                }
                if f == Func::Ln { v.ln(p, RM, cc) } else { v.log10(p, RM, cc) }
            }
            Func::Sin | Func::Cos | Func::Tan => {
                let rad = self.angle_to_rad(v.clone());
                let cc = &mut self.prec.cc;
                let r = match f {
                    Func::Sin => rad.sin(p, RM, cc),
                    Func::Cos => rad.cos(p, RM, cc),
                    _ => {
                        let c = rad.cos(p, RM, cc);
                        if self.is_noise(&c, &rad) {
                            return Err(EngineError::new("tan is undefined here (cos = 0)"));
                        }
                        rad.sin(p, RM, &mut self.prec.cc).div(&c, p, RM)
                    }
                };
                return Ok(self.flush_noise(r, &rad));
            }
            Func::Asin | Func::Acos => {
                let r = if f == Func::Asin { v.asin(p, RM, cc) } else { v.acos(p, RM, cc) };
                Value::Real(self.rad_to_angle(r)).into_float()
            }
            Func::Atan => {
                let r = v.atan(p, RM, cc);
                Value::Real(self.rad_to_angle(r)).into_float()
            }
            Func::Sinh => v.sinh(p, RM, cc),
            Func::Cosh => v.cosh(p, RM, cc),
            Func::Tanh => v.tanh(p, RM, cc),
            Func::Asinh => v.asinh(p, RM, cc),
            Func::Acosh => v.acosh(p, RM, cc),
            Func::Atanh => {
                if v.abs().cmp(&one).is_some_and(|c| c >= 0) {
                    return Err(EngineError::new("artanh is infinite at ±1"));
                }
                v.atanh(p, RM, cc)
            }
        };
        if out.is_nan() {
            return Err(EngineError::new("Calculation failed (result is undefined)"));
        }
        Ok(Value::Real(out))
    }

    /// True when `r` is smaller than the rounding noise we expect for argument `arg`.
    fn is_noise(&self, r: &BigFloat, arg: &BigFloat) -> bool {
        if r.is_zero() {
            return true;
        }
        // An exact argument (like 10^50) carries no rounding error to cancel.
        if arg.as_raw_parts().is_some_and(|parts| !parts.4) {
            return false;
        }
        let p = self.prec.bits as i64;
        let er = r.exponent().map(|e| e as i64).unwrap_or(i64::MIN);
        let ea = arg.exponent().map(|e| e as i64).unwrap_or(0).max(1);
        // Only meaningful when the argument itself isn't tiny.
        ea > -(p / 2) && er < ea - (p - 32)
    }

    /// sin(π) comes out as ~1e-60 in floating point; show it as the 0 it really is.
    /// It stays a `Real` so the precision check in `lib.rs` still compares it across precisions.
    fn flush_noise(&self, r: BigFloat, arg: &BigFloat) -> Value {
        if self.is_noise(&r, arg) {
            Value::Real(BigFloat::from_word(0, self.prec.bits))
        } else {
            Value::Real(r)
        }
    }
}

impl Value {
    fn into_float(self) -> BigFloat {
        match self {
            Value::Real(f) => f,
            Value::Exact(_) | Value::Complex(..) => unreachable!("only used on real values"),
        }
    }
}

trait OddInt {
    fn is_odd_int(&self) -> bool;
}

impl OddInt for BigInt {
    fn is_odd_int(&self) -> bool {
        num_integer::Integer::is_odd(self)
    }
}

/// `x * (π/180)` written with an explicit degree sign: returns `x`.
/// 1 for ∞, −1 for −∞ (as `−1·∞` or `−∞`), None for anything else.
pub(crate) fn infinity_sign(e: &Expr) -> Option<i8> {
    match e {
        Expr::Const(Const::Inf) => Some(1),
        Expr::Neg(x) => infinity_sign(x).map(|s| -s),
        Expr::Binary(BinOp::Mul, k, x) if matches!(&**k, Expr::Neg(one) if **one == Expr::Number("1".into())) => infinity_sign(x).map(|s| -s),
        _ => None,
    }
}

pub(crate) fn as_explicit_degrees(e: &Expr) -> Option<&Expr> {
    if let Expr::Binary(BinOp::Mul, x, factor) = e
        && let Expr::Binary(BinOp::Div, pi, d) = &**factor
            && **pi == Expr::Const(Const::Pi) && **d == Expr::Number("180".into()) {
                return Some(x);
            }
    None
}

/// Exact sin/cos/tan at rational multiples of 90°, plus 30°/45°/60°-family values that are rational.
fn exact_trig(f: Func, deg: &BigRational) -> Result<Option<Value>, EngineError> {
    if !deg.is_integer() {
        return Ok(None);
    }
    let d = deg.numer() % BigInt::from(360);
    let d = ((d.to_i64().unwrap() % 360) + 360) % 360;
    let half = BigRational::new(1.into(), 2.into());
    let sin = |d: i64| -> Option<BigRational> {
        Some(match d {
            0 | 180 => BigRational::zero(),
            90 => int(1),
            270 => int(-1),
            30 | 150 => half.clone(),
            210 | 330 => -half.clone(),
            _ => return None,
        })
    };
    let v = match f {
        Func::Sin => sin(d),
        Func::Cos => sin((d + 90) % 360),
        Func::Tan => match d {
            90 | 270 => return Err(EngineError::new("tan is undefined at 90° + k·180°")),
            0 | 180 => Some(BigRational::zero()),
            45 | 225 => Some(int(1)),
            135 | 315 => Some(int(-1)),
            _ => None,
        },
        _ => None,
    };
    Ok(v.map(Value::Exact))
}

fn exact_inverse_trig_degrees(f: Func, r: &BigRational) -> Option<BigRational> {
    let half = BigRational::new(1.into(), 2.into());
    let one = int(1);
    let d = match f {
        Func::Asin => {
            if *r == one { 90 } else if *r == -one.clone() { -90 }
            else if *r == half { 30 } else if *r == -half.clone() { -30 } else { return None }
        }
        Func::Acos => {
            if *r == one { 0 } else if *r == -one.clone() { 180 }
            else if *r == half { 60 } else if *r == -half.clone() { 120 }
            else if r.is_zero() { 90 } else { return None }
        }
        Func::Atan => {
            if *r == one { 45 } else if *r == -one { -45 } else { return None }
        }
        _ => return None,
    };
    Some(int(d))
}

/// Exact rational power when the result is rational and not absurdly large.
pub(crate) fn exact_pow(base: &BigRational, exp: &BigRational) -> Result<Option<BigRational>, EngineError> {
    if base.is_zero() {
        if exp.is_negative() {
            return Err(EngineError::new("Division by zero (0 to a negative power)"));
        }
        return Ok(Some(if exp.is_zero() { BigRational::one() } else { BigRational::zero() }));
    }
    let Some(q) = exp.denom().to_u32().filter(|q| *q <= 1024) else { return Ok(None) };
    let Some(p) = exp.numer().to_i64() else { return Ok(None) };
    let rooted = if q == 1 {
        base.clone()
    } else {
        match number::exact_root(base, q) {
            Some(r) => r,
            None => return Ok(None),
        }
    };
    let size = number::bit_len(&rooted).saturating_mul(p.unsigned_abs());
    if size > MAX_EXACT_BITS {
        return Ok(None);
    }
    let powered = num_traits::pow(rooted, p.unsigned_abs() as usize);
    Ok(Some(if p < 0 { powered.recip() } else { powered }))
}

/// log_b(x) = k exactly when b^k = x for a small rational k (integer k, or 1/k).
pub(crate) fn exact_log(b: &BigRational, x: &BigRational) -> Option<BigRational> {
    if !b.is_positive() || b.is_one() || !x.is_positive() {
        return None;
    }
    if x.is_one() {
        return Some(BigRational::zero());
    }
    let bits = number::bit_len(x).max(number::bit_len(b));
    for k in 1..=bits.min(4096) as i64 {
        let bk = num_traits::pow(b.clone(), k as usize);
        if bk == *x {
            return Some(int(k));
        }
        if bk.recip() == *x {
            return Some(int(-k));
        }
        if number::bit_len(&bk) > bits + 1 {
            break;
        }
    }
    // x^k = b  =>  log_b x = 1/k
    for k in 2..=64u32 {
        if num_traits::pow(x.clone(), k as usize) == *b {
            return Some(BigRational::new(1.into(), k.into()));
        }
    }
    None
}
