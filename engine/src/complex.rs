//! Complex arithmetic on pairs of arbitrary-precision floats, for the numeric layer.
//!
//! Principal values throughout, with Mathematica's branch cuts: asin 2 = π/2 − 1.317i,
//! acosh ½ = 1.047i, atanh 2 = 0.549 + 1.571i.

use astro_float::BigFloat;

use crate::EngineError;
use crate::number::{Prec, RM};

#[derive(Debug, Clone)]
pub struct C {
    pub re: BigFloat,
    pub im: BigFloat,
}

type CR = Result<C, EngineError>;

impl C {
    pub fn new(re: BigFloat, im: BigFloat) -> C {
        C { re, im }
    }

    pub fn is_zero(&self) -> bool {
        self.re.is_zero() && self.im.is_zero()
    }

    /// i·z
    fn times_i(&self) -> C {
        C::new(self.im.neg(), self.re.clone())
    }

    /// −i·z
    fn times_neg_i(&self) -> C {
        C::new(self.im.clone(), self.re.neg())
    }
}

fn zero(pr: &Prec) -> BigFloat {
    BigFloat::from_word(0, pr.bits)
}

fn one(pr: &Prec) -> BigFloat {
    BigFloat::from_word(1, pr.bits)
}

pub fn real(x: BigFloat, pr: &Prec) -> C {
    C::new(x, zero(pr))
}

fn log_zero() -> EngineError {
    EngineError::new("Logarithm of zero is undefined")
}

pub fn add(a: &C, b: &C, pr: &Prec) -> C {
    C::new(a.re.add(&b.re, pr.bits, RM), a.im.add(&b.im, pr.bits, RM))
}

pub fn sub(a: &C, b: &C, pr: &Prec) -> C {
    C::new(a.re.sub(&b.re, pr.bits, RM), a.im.sub(&b.im, pr.bits, RM))
}

pub fn mul(a: &C, b: &C, pr: &Prec) -> C {
    let p = pr.bits;
    let re = a.re.mul(&b.re, p, RM).sub(&a.im.mul(&b.im, p, RM), p, RM);
    let im = a.re.mul(&b.im, p, RM).add(&a.im.mul(&b.re, p, RM), p, RM);
    C::new(re, im)
}

pub fn div(a: &C, b: &C, pr: &Prec) -> CR {
    let p = pr.bits;
    let den = b.re.mul(&b.re, p, RM).add(&b.im.mul(&b.im, p, RM), p, RM);
    if den.is_zero() {
        return Err(EngineError::new("Division by zero"));
    }
    let re = a.re.mul(&b.re, p, RM).add(&a.im.mul(&b.im, p, RM), p, RM);
    let im = a.im.mul(&b.re, p, RM).sub(&a.re.mul(&b.im, p, RM), p, RM);
    Ok(C::new(re.div(&den, p, RM), im.div(&den, p, RM)))
}

fn scale(a: &C, k: &BigFloat, pr: &Prec) -> C {
    C::new(a.re.mul(k, pr.bits, RM), a.im.mul(k, pr.bits, RM))
}

/// |z|
pub fn abs(z: &C, pr: &Prec) -> BigFloat {
    let p = pr.bits;
    z.re.mul(&z.re, p, RM).add(&z.im.mul(&z.im, p, RM), p, RM).sqrt(p, RM)
}

/// The angle of z in (−π, π].
pub fn arg(z: &C, pr: &mut Prec) -> BigFloat {
    let p = pr.bits;
    let pi = pr.cc.pi(p, RM);
    if z.re.is_zero() {
        let half_pi = pi.div(&BigFloat::from_word(2, p), p, RM);
        return if z.im.is_zero() {
            zero(pr)
        } else if z.im.is_positive() {
            half_pi
        } else {
            half_pi.neg()
        };
    }
    let t = z.im.div(&z.re, p, RM).atan(p, RM, &mut pr.cc);
    if z.re.is_positive() {
        t
    } else if z.im.is_negative() && !z.im.is_zero() {
        t.sub(&pi, p, RM)
    } else {
        t.add(&pi, p, RM)
    }
}

pub fn exp(z: &C, pr: &mut Prec) -> C {
    let p = pr.bits;
    let cc = &mut pr.cc;
    let m = z.re.exp(p, RM, cc);
    C::new(m.mul(&z.im.cos(p, RM, cc), p, RM), m.mul(&z.im.sin(p, RM, cc), p, RM))
}

pub fn ln(z: &C, pr: &mut Prec) -> CR {
    if z.is_zero() {
        return Err(log_zero());
    }
    let m = abs(z, pr).ln(pr.bits, RM, &mut pr.cc);
    Ok(C::new(m, arg(z, pr)))
}

pub fn sqrt(z: &C, pr: &mut Prec) -> C {
    if z.is_zero() {
        return z.clone();
    }
    let p = pr.bits;
    let two = BigFloat::from_word(2, p);
    let m = abs(z, pr);
    // Computed so neither part comes from a cancelling subtraction.
    if !z.re.is_negative() || z.re.is_zero() {
        let t = m.add(&z.re, p, RM).div(&two, p, RM).sqrt(p, RM);
        C::new(t.clone(), z.im.div(&t.mul(&two, p, RM), p, RM))
    } else {
        let t = m.sub(&z.re, p, RM).div(&two, p, RM).sqrt(p, RM);
        let re = z.im.abs().div(&t.mul(&two, p, RM), p, RM);
        C::new(re, if z.im.is_negative() && !z.im.is_zero() { t.neg() } else { t })
    }
}

/// z^n for an integer n, by repeated squaring (no logarithm, so no rounding of the angle).
pub fn powi(z: &C, n: i64, pr: &Prec) -> CR {
    let mut acc = real(one(pr), pr);
    let mut base = z.clone();
    let mut k = n.unsigned_abs();
    while k > 0 {
        if k & 1 == 1 {
            acc = mul(&acc, &base, pr);
        }
        base = mul(&base, &base, pr);
        k >>= 1;
    }
    if n < 0 { div(&real(one(pr), pr), &acc, pr) } else { Ok(acc) }
}

/// Principal value of z^w = exp(w·ln z).
pub fn pow(z: &C, w: &C, pr: &mut Prec) -> CR {
    if z.is_zero() {
        if w.re.is_positive() && !w.re.is_zero() {
            return Ok(z.clone());
        }
        return Err(EngineError::new("Division by zero (0 to a non-positive power)"));
    }
    let l = ln(z, pr)?;
    Ok(exp(&mul(w, &l, pr), pr))
}

pub fn sin(z: &C, pr: &mut Prec) -> C {
    let p = pr.bits;
    let cc = &mut pr.cc;
    let re = z.re.sin(p, RM, cc).mul(&z.im.cosh(p, RM, cc), p, RM);
    let im = z.re.cos(p, RM, cc).mul(&z.im.sinh(p, RM, cc), p, RM);
    C::new(re, im)
}

pub fn cos(z: &C, pr: &mut Prec) -> C {
    let p = pr.bits;
    let cc = &mut pr.cc;
    let re = z.re.cos(p, RM, cc).mul(&z.im.cosh(p, RM, cc), p, RM);
    let im = z.re.sin(p, RM, cc).mul(&z.im.sinh(p, RM, cc), p, RM).neg();
    C::new(re, im)
}

pub fn tan(z: &C, pr: &mut Prec) -> CR {
    let s = sin(z, pr);
    let c = cos(z, pr);
    div(&s, &c, pr)
}

pub fn sinh(z: &C, pr: &mut Prec) -> C {
    // sinh z = −i·sin(iz)
    sin(&z.times_i(), pr).times_neg_i()
}

pub fn cosh(z: &C, pr: &mut Prec) -> C {
    // cosh z = cos(iz)
    cos(&z.times_i(), pr)
}

pub fn tanh(z: &C, pr: &mut Prec) -> CR {
    let s = sinh(z, pr);
    let c = cosh(z, pr);
    div(&s, &c, pr)
}

/// asin z = −i·ln(iz + √(1 − z²))
pub fn asin(z: &C, pr: &mut Prec) -> CR {
    let one = real(one(pr), pr);
    let root = sqrt(&sub(&one, &mul(z, z, pr), pr), pr);
    Ok(ln(&add(&z.times_i(), &root, pr), pr)?.times_neg_i())
}

/// acos z = π/2 − asin z
pub fn acos(z: &C, pr: &mut Prec) -> CR {
    let p = pr.bits;
    let half_pi = pr.cc.pi(p, RM).div(&BigFloat::from_word(2, p), p, RM);
    let a = asin(z, pr)?;
    Ok(sub(&real(half_pi, pr), &a, pr))
}

/// atan z = −(i/2)·ln((1 + iz)/(1 − iz))
pub fn atan(z: &C, pr: &mut Prec) -> CR {
    let one = real(one(pr), pr);
    let iz = z.times_i();
    let den = sub(&one, &iz, pr);
    if den.is_zero() {
        return Err(EngineError::new("arctan is undefined at ±i"));
    }
    let q = div(&add(&one, &iz, pr), &den, pr)?;
    if q.is_zero() {
        return Err(EngineError::new("arctan is undefined at ±i"));
    }
    let half = BigFloat::from_word(1, pr.bits).div(&BigFloat::from_word(2, pr.bits), pr.bits, RM);
    Ok(scale(&ln(&q, pr)?.times_neg_i(), &half, pr))
}

/// asinh z = ln(z + √(z² + 1))
pub fn asinh(z: &C, pr: &mut Prec) -> CR {
    let one = real(one(pr), pr);
    let root = sqrt(&add(&mul(z, z, pr), &one, pr), pr);
    ln(&add(z, &root, pr), pr)
}

/// acosh z = ln(z + √(z + 1)·√(z − 1))
pub fn acosh(z: &C, pr: &mut Prec) -> CR {
    let one = real(one(pr), pr);
    let a = sqrt(&add(z, &one, pr), pr);
    let b = sqrt(&sub(z, &one, pr), pr);
    ln(&add(z, &mul(&a, &b, pr), pr), pr)
}

/// atanh z = ½·ln((1 + z)/(1 − z))
pub fn atanh(z: &C, pr: &mut Prec) -> CR {
    let one = real(one(pr), pr);
    let den = sub(&one, z, pr);
    let num = add(&one, z, pr);
    if den.is_zero() || num.is_zero() {
        return Err(EngineError::new("artanh is infinite at ±1"));
    }
    let half = BigFloat::from_word(1, pr.bits).div(&BigFloat::from_word(2, pr.bits), pr.bits, RM);
    Ok(scale(&ln(&div(&num, &den, pr)?, pr)?, &half, pr))
}

pub fn scale_by(z: &C, k: &BigFloat, pr: &Prec) -> C {
    scale(z, k, pr)
}
