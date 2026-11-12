//! The gamma function at any precision.
//!
//! Γ(x) for real x: reflection Γ(x)Γ(1−x) = π/sin(πx) below ½; above, shift x up to z so
//! Stirling's series for ln Γ(z) converges past the working precision, then divide by
//! x(x+1)…(z−1). The series' Bernoulli numbers come from tangent numbers, whole numbers that
//! a short recurrence builds without any fractions, kept between calls.

use std::sync::Mutex;

use astro_float::BigFloat;
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};

use crate::EngineError;
use crate::number::{self, Prec, RM};

/// Tangent numbers T₁, T₂, … (1, 2, 16, 272, …), extended as needed.
static TANGENT: Mutex<Vec<BigInt>> = Mutex::new(Vec::new());

/// T₁..=Tₙ. Brent and Harvey's in-place recurrence: O(n²) small multiplications.
fn tangent_numbers(n: usize) -> Vec<BigInt> {
    let mut cache = TANGENT.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() < n {
        let n = n.max(2 * cache.len());
        let mut t = vec![BigInt::zero(); n + 1];
        t[1] = BigInt::one();
        for k in 2..=n {
            t[k] = &t[k - 1] * (k - 1);
        }
        for k in 2..=n {
            for j in k..=n {
                t[j] = &t[j - 1] * (j - k) + &t[j] * (j - k + 2);
            }
        }
        *cache = t.split_off(1);
    }
    cache[..n].to_vec()
}

/// A whole number as a float, keeping only the bits the precision needs.
fn big_to_float(n: &BigInt, p: usize) -> BigFloat {
    let bits = n.bits() as usize;
    let shift = bits.saturating_sub(p + 64);
    let words = (n.abs() >> shift).to_u64_digits().1;
    // from_words reads the words as a fraction 0.m, scaled by 2^e.
    let e = (words.len() * 64 + shift) as astro_float::Exponent;
    let sign = if n.is_negative() { astro_float::Sign::Neg } else { astro_float::Sign::Pos };
    BigFloat::from_words(&words, sign, e)
}

fn pole() -> EngineError {
    EngineError::new("Γ is undefined at 0 and the negative whole numbers")
}

/// Γ(x) for a real x.
pub fn gamma(x: &BigFloat, prec: &mut Prec) -> Result<BigFloat, EngineError> {
    let p = prec.bits;
    let xf = number::float_to_f64(x).ok_or_else(|| EngineError::new("Γ needs a real number"))?;
    if xf <= 0.0 && x.fract().is_zero() {
        return Err(pole());
    }
    if xf < 0.5 {
        // Γ(x) = π / (sin(πx) Γ(1−x))
        let pi = prec.cc.pi(p, RM);
        let one = BigFloat::from_word(1, p);
        let other = gamma(&one.sub(x, p, RM), prec)?;
        let s = pi.mul(x, p, RM).sin(p, RM, &mut prec.cc);
        if s.is_zero() {
            return Err(pole());
        }
        return Ok(pi.div(&s.mul(&other, p, RM), p, RM));
    }
    // A larger z needs fewer series terms (the costly part) for more shift steps (cheap).
    let target = 0.6 * p as f64 + 20.0;
    let shift = (target - xf).ceil().max(0.0) as usize;
    let mut z = x.clone();
    let mut product = BigFloat::from_word(1, p);
    for _ in 0..shift {
        product = product.mul(&z, p, RM);
        z = z.add(&BigFloat::from_word(1, p), p, RM);
    }
    // Terms needed: |B₂ₖ| / (2k(2k−1) z^(2k−1)) ≈ 2 (2k)! / (2πz)^(2k), in bits.
    let log2_2piz = (2.0 * std::f64::consts::PI * xf.max(target)).log2();
    let mut log2_fact = 0.0;
    let mut count = 1;
    loop {
        log2_fact += ((2 * count - 1) as f64).log2() + ((2 * count) as f64).log2();
        if log2_fact - 2.0 * count as f64 * log2_2piz + 1.0 < -(p as f64 + 16.0) || count >= 20_000 {
            break;
        }
        count += 1;
    }
    let tangent = tangent_numbers(count);
    // ln Γ(z) = (z − ½) ln z − z + ½ ln 2π + Σ B₂ₖ / (2k(2k−1) z^(2k−1)),
    // with B₂ₖ / (2k(2k−1)) = (−1)^(k−1) Tₖ / ((2k−1) 4ᵏ (4ᵏ−1)).
    let cc = &mut prec.cc;
    let half = BigFloat::from_word(1, p).div(&BigFloat::from_word(2, p), p, RM);
    let ln_z = z.ln(p, RM, cc);
    let two_pi = cc.pi(p, RM).mul(&BigFloat::from_word(2, p), p, RM);
    let mut sum = z.sub(&half, p, RM).mul(&ln_z, p, RM).sub(&z, p, RM).add(&half.mul(&two_pi.ln(p, RM, cc), p, RM), p, RM);
    let z2 = z.mul(&z, p, RM);
    let mut zpow = z.clone(); // z^(2k−1)
    let mut four_k = BigInt::one();
    for (i, t) in tangent.iter().enumerate() {
        let k = i + 1;
        four_k <<= 2;
        let denom = BigInt::from(2 * k - 1) * &four_k * (&four_k - 1u32);
        let term = big_to_float(t, p).div(&big_to_float(&denom, p).mul(&zpow, p, RM), p, RM);
        sum = if k % 2 == 1 { sum.add(&term, p, RM) } else { sum.sub(&term, p, RM) };
        zpow = zpow.mul(&z2, p, RM);
    }
    let g = sum.exp(p, RM, &mut prec.cc);
    Ok(g.div(&product, p, RM))
}
