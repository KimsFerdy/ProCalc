//! The error function and the normal quantile Φ⁻¹, at any precision.
//!
//! erf by its Maclaurin series, with guard bits for the cancellation between its large
//! alternating terms (about x²·log₂e bits); Φ⁻¹ by Newton's method on Φ from a
//! double-precision start (Acklam's rational approximation).

use astro_float::BigFloat;

use crate::EngineError;
use crate::number::{self, Prec, RM};

/// erf(x) for a real x.
pub fn erf(x: &BigFloat, prec: &mut Prec) -> BigFloat {
    let p = prec.bits;
    if x.is_zero() {
        return BigFloat::from_word(0, p);
    }
    let xf = number::float_to_f64(x).unwrap_or(f64::INFINITY);
    let negative = xf < 0.0;
    let one = BigFloat::from_word(1, p);
    // Past here 1 − erf(x) < 2^{−4p}: ±1 to every digit.
    if xf * xf > 4.0 * p as f64 || xf.is_infinite() {
        return if negative { one.neg() } else { one };
    }
    let guard = (xf * xf * 1.45) as usize + 32;
    let q = p + guard;
    let xq = x.clone().set_precision_inline(q);
    // Σ (−1)ⁿ x^{2n+1} / (n!(2n+1))
    let x2 = xq.mul(&xq, q, RM);
    let mut power = xq.clone(); // (−1)ⁿ x^{2n+1}/n!
    let mut sum = xq.clone();
    let tiny = BigFloat::from_word(1, q).div(&BigFloat::from_word(2, q).powi(q + 8, q, RM), q, RM);
    let mut k = 1u64;
    loop {
        power = power.mul(&x2, q, RM).div(&BigFloat::from_word(k, q), q, RM).neg();
        let term = power.div(&BigFloat::from_word(2 * k + 1, q), q, RM);
        sum = sum.add(&term, q, RM);
        if term.abs().cmp(&sum.abs().mul(&tiny, q, RM)).is_some_and(|c| c < 0) || k > 200_000 {
            break;
        }
        k += 1;
    }
    let pi = prec.cc.pi(q, RM);
    let two_over_root_pi = BigFloat::from_word(2, q).div(&pi.sqrt(q, RM), q, RM);
    let mut r = sum.mul(&two_over_root_pi, q, RM);
    r.set_precision(p, RM).ok();
    r
}

trait InlinePrecision {
    fn set_precision_inline(self, p: usize) -> Self;
}

impl InlinePrecision for BigFloat {
    fn set_precision_inline(mut self, p: usize) -> Self {
        self.set_precision(p, RM).ok();
        self
    }
}

/// Φ(z) = ½(1 + erf(z/√2)), the standard normal CDF.
fn phi(z: &BigFloat, prec: &mut Prec) -> BigFloat {
    let p = prec.bits;
    let root2 = BigFloat::from_word(2, p).sqrt(p, RM);
    let e = erf(&z.div(&root2, p, RM), prec);
    BigFloat::from_word(1, p).add(&e, p, RM).div(&BigFloat::from_word(2, p), p, RM)
}

/// Acklam's approximation of Φ⁻¹ (relative error about 1e−9).
fn probit_f64(p: f64) -> f64 {
    const A: [f64; 6] = [-3.969683028665376e1, 2.209460984245205e2, -2.759285104469687e2, 1.38357751867269e2, -3.066479806614716e1, 2.506628277459239];
    const B: [f64; 5] = [-5.447609879822406e1, 1.615858368580409e2, -1.556989798598866e2, 6.680131188771972e1, -1.328068155288572e1];
    const C: [f64; 6] = [-7.784894002430293e-3, -3.223964580411365e-1, -2.400758277161838, -2.549732539343734, 4.374664141464968, 2.938163982698783];
    const D: [f64; 4] = [7.784695709041462e-3, 3.224671290700398e-1, 2.445134137142996, 3.754408661907416];
    let low = 0.02425;
    let tail = |q: f64| (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    if p < low {
        tail((-2.0 * p.ln()).sqrt())
    } else if p > 1.0 - low {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

/// Φ⁻¹(p) for 0 < p < 1: Newton's method z ← z − (Φ(z) − p)/φ(z) at the working precision.
pub fn probit(pv: &BigFloat, prec: &mut Prec) -> Result<BigFloat, EngineError> {
    let p = prec.bits;
    let pf = number::float_to_f64(pv).ok_or_else(|| EngineError::new("Φ⁻¹ needs a probability"))?;
    let zero = BigFloat::from_word(0, p);
    let one = BigFloat::from_word(1, p);
    if pv.cmp(&zero).is_none_or(|c| c <= 0) || pv.cmp(&one).is_none_or(|c| c >= 0) {
        return Err(EngineError::new("The inverse normal needs a probability strictly between 0 and 1"));
    }
    let start = probit_f64(pf.clamp(1e-300, 1.0 - 1e-16));
    let mut z = BigFloat::from_f64(if start.is_finite() { start } else { 0.0 }, p);
    let two_pi = prec.cc.pi(p, RM).mul(&BigFloat::from_word(2, p), p, RM);
    let norm = BigFloat::from_word(1, p).div(&two_pi.sqrt(p, RM), p, RM);
    let tiny = BigFloat::from_word(1, p).div(&BigFloat::from_word(2, p).powi(p - 8, p, RM), p, RM);
    for _ in 0..200 {
        let f = phi(&z, prec).sub(pv, p, RM);
        let half = BigFloat::from_word(2, p);
        let density = norm.mul(&z.mul(&z, p, RM).div(&half, p, RM).neg().exp(p, RM, &mut prec.cc), p, RM);
        if density.is_zero() {
            break;
        }
        let change = f.div(&density, p, RM);
        z = z.sub(&change, p, RM);
        if change.abs().cmp(&z.abs().add(&one, p, RM).mul(&tiny, p, RM)).is_some_and(|c| c <= 0) {
            break;
        }
    }
    Ok(z)
}
