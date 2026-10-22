//! Evaluates an [`Expr`]: stays exact (rational) whenever the maths allows,
//! and falls back to arbitrary-precision floats otherwise.

use astro_float::BigFloat;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::ast::{BinOp, Const, Expr, Func};
use crate::number::{self, Prec, RM, Value, int};
use crate::{AngleMode, EngineError};

/// Largest exact result we are willing to build, in bits (~300k decimal digits).
pub(crate) const MAX_EXACT_BITS: u64 = 1_000_000;
pub(crate) const MAX_FACTORIAL: i64 = 20_000;

pub struct Evaluator {
    pub prec: Prec,
    pub angle: AngleMode,
}

type R = Result<Value, EngineError>;

pub(crate) fn zeroth_root_err() -> EngineError {
    EngineError::new("The 0th root is undefined")
}

pub(crate) fn complex_err() -> EngineError {
    EngineError::new("The result is a complex number (complex mode is coming soon)")
}

impl Evaluator {
    pub fn eval(&mut self, e: &Expr) -> R {
        match e {
            Expr::Number(s) => number::parse_exact(s)
                .map(Value::Exact)
                .ok_or_else(|| EngineError::new(format!("Bad number '{s}'"))),
            Expr::Const(Const::Pi) => Ok(Value::Real(self.prec.cc.pi(self.prec.bits, RM))),
            Expr::Const(Const::E) => Ok(Value::Real(self.prec.cc.e(self.prec.bits, RM))),
            Expr::Var(v) => Err(EngineError::new(format!("{v} has no value"))),
            Expr::Neg(x) => Ok(match self.eval(x)? {
                Value::Exact(r) => Value::Exact(-r),
                Value::Real(f) => Value::Real(f.neg()),
            }),
            Expr::Binary(op, a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                self.binary(*op, a, b)
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
        let p = self.prec.bits;
        let x = self.f(&a);
        let y = self.f(&b);
        if x.is_zero() {
            if number::is_negative_float(&y) {
                return Err(EngineError::new("Division by zero (0 to a negative power)"));
            }
            return Ok(Value::Real(x));
        }
        if number::is_negative_float(&x) {
            // Negative base: fine for integer exponents, or rational ones with an odd denominator.
            let sign = match &b {
                Value::Exact(r) if r.denom().is_odd_int() => {
                    if r.numer().is_odd_int() { -1 } else { 1 }
                }
                _ => return Err(complex_err()),
            };
            let mag = x.abs().pow(&y, p, RM, &mut self.prec.cc);
            return Ok(Value::Real(if sign < 0 { mag.neg() } else { mag }));
        }
        Ok(Value::Real(x.pow(&y, p, RM, &mut self.prec.cc)))
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
            _ => Err(EngineError::new("Factorial needs a whole number ≥ 0 (gamma function is coming)")),
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

    pub(crate) fn call(&mut self, f: Func, x: Value) -> R {
        let p = self.prec.bits;
        // Exact cases first.
        if let Value::Exact(r) = &x {
            match f {
                Func::Abs => return Ok(Value::Exact(r.abs())),
                Func::Floor => return Ok(Value::Exact(r.floor())),
                Func::Ceil => return Ok(Value::Exact(r.ceil())),
                Func::Sqrt => {
                    if r.is_negative() {
                        return Err(complex_err());
                    }
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
        let cc = &mut self.prec.cc;
        let out = match f {
            Func::Abs => v.abs(),
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
            Func::Sqrt => {
                if number::is_negative_float(&v) {
                    return Err(complex_err());
                }
                v.sqrt(p, RM)
            }
            Func::Exp => v.exp(p, RM, cc),
            Func::Ln | Func::Log => {
                if v.is_zero() || number::is_negative_float(&v) {
                    return Err(if v.is_zero() {
                        EngineError::new("Logarithm of zero is undefined")
                    } else {
                        complex_err()
                    });
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
                if v.abs().cmp(&one).is_some_and(|c| c > 0) {
                    return Err(complex_err());
                }
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
            Func::Acosh => {
                if v.cmp(&one).is_some_and(|c| c < 0) {
                    return Err(complex_err());
                }
                v.acosh(p, RM, cc)
            }
            Func::Atanh => {
                if v.abs().cmp(&one).is_some_and(|c| c >= 0) {
                    return Err(EngineError::new("artanh needs |x| < 1"));
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
            Value::Exact(_) => unreachable!("only used on real values"),
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
