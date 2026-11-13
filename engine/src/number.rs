//! Numeric values: exact rationals when possible, arbitrary-precision floats otherwise.

use astro_float::{BigFloat, Consts, Radix, RoundingMode};
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

pub const RM: RoundingMode = RoundingMode::ToEven;

#[derive(Debug, Clone)]
pub enum Value {
    Exact(BigRational),
    Real(BigFloat),
    /// Real and imaginary parts; the imaginary part is never zero (see `eval::from_c`).
    Complex(BigFloat, BigFloat),
}

/// Working precision plus the constants cache that astro-float needs.
pub struct Prec {
    pub bits: usize,
    pub digits: usize,
    pub cc: Consts,
}

impl Prec {
    pub fn new(digits: usize) -> Prec {
        Prec::with_bits(digits, Prec::base_bits(digits))
    }

    /// ~3.32 bits per decimal digit, plus guard bits so rounding noise stays out of sight.
    pub fn base_bits(digits: usize) -> usize {
        (digits as f64 * 3.322).ceil() as usize + 64
    }

    pub fn with_bits(digits: usize, bits: usize) -> Prec {
        Prec { bits, digits, cc: Consts::new().expect("astro-float constants cache") }
    }

    pub fn float_of(&mut self, v: &Value) -> BigFloat {
        match v {
            Value::Real(f) => f.clone(),
            // Callers handle complex values first; NaN turns a slip into an error, not a wrong answer.
            Value::Complex(..) => BigFloat::nan(None),
            Value::Exact(r) => {
                let n = self.parse_int(r.numer());
                if r.denom().is_one() {
                    n
                } else {
                    let d = self.parse_int(r.denom());
                    n.div(&d, self.bits, RM)
                }
            }
        }
    }

    fn parse_int(&mut self, i: &BigInt) -> BigFloat {
        // Small integers directly; the decimal round trip is slow in a tight loop (Σ 1/k²).
        if let Some(w) = i.magnitude().to_u64() {
            let f = BigFloat::from_u64(w, self.bits);
            return if i.is_negative() { f.neg() } else { f };
        }
        BigFloat::parse(&i.to_string(), Radix::Dec, self.bits, RM, &mut self.cc)
    }

    /// Decimal digits of a float: (negative, significant digits, exponent of the first digit).
    pub fn float_digits(&mut self, f: &BigFloat) -> Option<(bool, String, i64)> {
        let s = f.format(Radix::Dec, RM, &mut self.cc).ok()?;
        if f.is_zero() {
            return Some((false, "0".into(), 0));
        }
        let (neg, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.as_str()),
        };
        let (mant, exp) = s.split_once('e')?;
        let exp: i64 = exp.parse().ok()?;
        let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
        Some((neg, digits, exp))
    }

    /// Converts an integer-valued float to an exact integer when it is held exactly.
    pub fn float_to_int(&mut self, f: &BigFloat) -> Option<BigInt> {
        let (neg, digits, exp) = self.float_digits(f)?;
        if exp < 0 || exp as usize + 1 > self.bits * 3 / 10 {
            return if digits == "0" { Some(BigInt::zero()) } else { None };
        }
        let need = exp as usize + 1;
        let mut d = digits.trim_end_matches('0').to_string();
        if d.len() > need {
            return None;
        }
        while d.len() < need {
            d.push('0');
        }
        let i: BigInt = d.parse().ok()?;
        Some(if neg { -i } else { i })
    }
}

pub fn int(i: i64) -> BigRational {
    BigRational::from_integer(BigInt::from(i))
}

/// Parses a decimal literal such as "12.50" exactly.
pub fn parse_exact(s: &str) -> Option<BigRational> {
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    let digits = format!("{whole}{frac}");
    let n: BigInt = if digits.is_empty() { BigInt::zero() } else { digits.parse().ok()? };
    let d = BigInt::from(10u32).pow(frac.len() as u32);
    Some(BigRational::new(n, d))
}

/// Exact k-th root of a non-negative integer, if it is a perfect power.
pub fn exact_root_int(n: &BigInt, k: u32) -> Option<BigInt> {
    let r = n.nth_root(k);
    if r.pow(k) == *n { Some(r) } else { None }
}

/// Exact k-th root of a rational (odd k allows negatives).
pub fn exact_root(r: &BigRational, k: u32) -> Option<BigRational> {
    if r.is_negative() {
        if k.is_multiple_of(2) {
            return None;
        }
        return exact_root(&-r, k).map(|x| -x);
    }
    let n = exact_root_int(r.numer(), k)?;
    let d = exact_root_int(r.denom(), k)?;
    Some(BigRational::new(n, d))
}

/// Base-10 exponent k when r = 10^k exactly.
pub fn exact_log10(r: &BigRational) -> Option<i64> {
    let ten = BigInt::from(10);
    let (mut big, neg) = if r.denom().is_one() {
        (r.numer().clone(), false)
    } else if r.numer().is_one() {
        (r.denom().clone(), true)
    } else {
        return None;
    };
    let mut k = 0i64;
    while big > BigInt::one() {
        let (q, rem) = big.div_rem(&ten);
        if !rem.is_zero() {
            return None;
        }
        big = q;
        k += 1;
    }
    if big.is_one() { Some(if neg { -k } else { k }) } else { None }
}

pub fn bit_len(r: &BigRational) -> u64 {
    r.numer().bits().max(r.denom().bits())
}

pub fn to_i64(r: &BigRational) -> Option<i64> {
    if r.is_integer() { r.numer().to_i64() } else { None }
}

// ---------- Formatting ----------

/// Rounded decimal digits of an exact rational: (negative, digits, exponent, exactly representable).
fn rational_digits(r: &BigRational, sig: usize) -> (bool, String, i64, bool) {
    if r.is_zero() {
        return (false, "0".into(), 0, true);
    }
    let neg = r.is_negative();
    let num = r.numer().abs();
    let den = r.denom().clone();
    // Find e with 10^e <= |r| < 10^(e+1).
    let mut e = num.to_string().len() as i64 - den.to_string().len() as i64;
    let ten = BigInt::from(10);
    let scaled_ge = |e: i64, n: &BigInt, d: &BigInt| -> bool {
        // |r| >= 10^e ?
        if e >= 0 { n >= &(d * ten.pow(e as u32)) } else { &(n * ten.pow((-e) as u32)) >= d }
    };
    while !scaled_ge(e, &num, &den) {
        e -= 1;
    }
    while scaled_ge(e + 1, &num, &den) {
        e += 1;
    }
    // Digits = round(|r| * 10^(sig-1-e))
    let shift = sig as i64 - 1 - e;
    let (n, d) = if shift >= 0 {
        (num * ten.pow(shift as u32), den)
    } else {
        (num, den * ten.pow((-shift) as u32))
    };
    let (q, rem) = n.div_rem(&d);
    let exact = rem.is_zero();
    let twice = &rem * 2u32;
    let q = if twice > d || (twice == d && q.is_odd()) { q + 1u32 } else { q };
    let mut digits = q.to_string();
    if digits.len() > sig {
        // Rounding carried into a new digit, e.g. 9.99 -> 10.0
        digits.truncate(sig);
        e += 1;
    }
    (neg, digits, e, exact)
}

pub struct Decimal {
    pub text: String,
    pub latex: String,
    /// True when the shown digits are the whole, exact value.
    pub exact: bool,
}

pub fn format_value(v: &Value, prec: &mut Prec) -> Option<Decimal> {
    let sig = prec.digits;
    let (neg, digits, exp, exact) = match v {
        Value::Exact(r) => rational_digits(r, sig),
        Value::Real(f) => float_parts(f, prec)?,
        Value::Complex(re, im) => return format_complex(re, im, prec),
    };
    Some(render_decimal(neg, &digits, exp, exact))
}

fn float_parts(f: &BigFloat, prec: &mut Prec) -> Option<(bool, String, i64, bool)> {
    let (neg, d, e) = prec.float_digits(f)?;
    let (d, e) = round_digits(&d, e, prec.digits);
    Some((neg, d, e, false))
}

fn format_complex(re: &BigFloat, im: &BigFloat, prec: &mut Prec) -> Option<Decimal> {
    // A part far below the digits shown is rounding noise (e^{iπ} = −1 + 10⁻⁶⁰i). The cutoff
    // depends on the digits shown, not the working precision, so runs at different
    // precisions agree.
    let cutoff = (prec.digits as f64 * 3.322) as i64 + 14;
    let exp = |f: &BigFloat| f.exponent().map(|e| e as i64).filter(|_| !f.is_zero());
    let (er, ei) = (exp(re), exp(im));
    let keep_re = match (er, ei) {
        (Some(a), Some(b)) => a >= b - cutoff,
        (a, _) => a.is_some(),
    };
    let keep_im = match (er, ei) {
        (Some(a), Some(b)) => b >= a - cutoff,
        (_, b) => b.is_some(),
    };
    if !keep_im {
        return format_value(&Value::Real(if keep_re { re.clone() } else { BigFloat::from_word(0, 64) }), prec);
    }
    let real_part = if keep_re {
        let (neg, d, e, x) = float_parts(re, prec)?;
        Some(render_decimal(neg, &d, e, x))
    } else {
        None
    };
    let (im_neg, d, e, x) = float_parts(im, prec)?;
    Some(join_complex(real_part, im_neg, render_decimal(false, &d, e, x)))
}

/// a + bi with exact rational parts.
pub fn format_complex_exact(re: &BigRational, im: &BigRational, sig: usize) -> Decimal {
    let part = |r: &BigRational| {
        let (neg, d, e, x) = rational_digits(r, sig);
        (neg, render_decimal(false, &d, e, x))
    };
    let (im_neg, im_dec) = part(im);
    let real_part = (!re.is_zero()).then(|| {
        let (neg, d, e, x) = rational_digits(re, sig);
        render_decimal(neg, &d, e, x)
    });
    if im.is_zero() {
        return real_part.unwrap_or_else(|| render_decimal(false, "0", 0, true));
    }
    join_complex(real_part, im_neg, im_dec)
}

/// Joins a real part and the magnitude of an imaginary part: 3−2i, 2i, −i.
fn join_complex(re: Option<Decimal>, im_neg: bool, im: Decimal) -> Decimal {
    let sign = if im_neg { "-" } else if re.is_some() { "+" } else { "" };
    let (coef_text, coef_latex) = if im.text == "1" { ("", "") } else { (im.text.as_str(), im.latex.as_str()) };
    let (re_text, re_latex, re_exact) = match &re {
        Some(d) => (d.text.as_str(), d.latex.as_str(), d.exact),
        None => ("", "", true),
    };
    Decimal {
        text: format!("{re_text}{sign}{coef_text}i"),
        latex: format!("{re_latex}{sign}{coef_latex}i"),
        exact: re_exact && im.exact,
    }
}

/// Rounds a digit string (first digit at 10^exp) to `sig` significant digits.
fn round_digits(d: &str, exp: i64, sig: usize) -> (String, i64) {
    if d.len() <= sig {
        return (d.to_string(), exp);
    }
    let keep: BigInt = d[..sig].parse().unwrap();
    let rest = &d[sig..];
    let first = rest.as_bytes()[0] - b'0';
    let tail_nonzero = rest[1..].bytes().any(|b| b != b'0');
    let up = first > 5 || (first == 5 && (tail_nonzero || keep.is_odd()));
    let keep = if up { keep + 1u32 } else { keep };
    let mut s = keep.to_string();
    let mut exp = exp;
    if s.len() > sig {
        s.truncate(sig);
        exp += 1;
    }
    (s, exp)
}

fn render_decimal(neg: bool, digits: &str, exp: i64, exact: bool) -> Decimal {
    let digits = {
        let t = digits.trim_end_matches('0');
        if t.is_empty() { "0" } else { t }
    };
    let sign = if neg { "-" } else { "" };
    let n = digits.len() as i64;
    // Plain notation for "human" magnitudes, scientific beyond that.
    let (text, latex) = if digits == "0" {
        ("0".to_string(), "0".to_string())
    } else if (-6..21).contains(&exp) {
        let s = if exp < 0 {
            format!("0.{}{}", "0".repeat((-exp - 1) as usize), digits)
        } else if exp + 1 >= n {
            format!("{}{}", digits, "0".repeat((exp + 1 - n) as usize))
        } else {
            let (a, b) = digits.split_at((exp + 1) as usize);
            format!("{a}.{b}")
        };
        (format!("{sign}{s}"), format!("{sign}{}", group_latex(&s)))
    } else {
        let mant = if n == 1 { digits.to_string() } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        (format!("{sign}{mant}e{exp}"), format!("{sign}{}\\times10^{{{exp}}}", group_latex(&mant)))
    };
    Decimal { text, latex, exact }
}

/// Thin-space digit grouping in LaTeX: 1234567.891 -> 1\,234\,567.891
fn group_latex(s: &str) -> String {
    let (int_part, frac) = s.split_once('.').map(|(a, b)| (a, Some(b))).unwrap_or((s, None));
    let mut out = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            out.push_str("\\,");
        }
        out.push(c);
    }
    if let Some(f) = frac {
        out.push('.');
        for (i, c) in f.chars().enumerate() {
            if i > 0 && i % 3 == 0 {
                out.push_str("\\,");
            }
            out.push(c);
        }
    }
    out
}

pub fn rational_latex(r: &BigRational) -> String {
    let sign = if r.is_negative() { "-" } else { "" };
    let n = r.numer().abs().to_string();
    if r.denom().is_one() {
        format!("{sign}{}", group_latex(&n))
    } else {
        format!("{sign}\\frac{{{}}}{{{}}}", group_latex(&n), group_latex(&r.denom().to_string()))
    }
}

/// The nearest f64, from the top 64 bits of the mantissa (value = 0.m · 2ᵉ).
/// For plotting, where 16 digits are plenty. The mantissa's words are 64 bits on 64-bit chips
/// and 32 bits on 32-bit ones (older phones), so the top 64 bits are one word or two.
pub fn float_to_f64(f: &BigFloat) -> Option<f64> {
    if f.is_zero() {
        return Some(0.0);
    }
    let (words, _, sign, exp, _) = f.as_raw_parts()?;
    let ws = astro_float::WORD_BIT_SIZE as i32;
    let take = (64 / ws).max(1) as usize;
    let mut m = 0.0f64;
    for (i, w) in words.iter().rev().take(take).enumerate() {
        m += *w as f64 * 2f64.powi(-ws * (i as i32 + 1));
    }
    let v = m * 2f64.powi(exp);
    Some(if sign == astro_float::Sign::Neg { -v } else { v })
}

pub fn is_negative_float(f: &BigFloat) -> bool {
    f.is_negative() && !f.is_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_rounding() {
        let r = BigRational::new(2.into(), 3.into());
        let (_, d, e, exact) = rational_digits(&r, 5);
        assert_eq!((d.as_str(), e, exact), ("66667", -1, false));
        let r = parse_exact("999.96").unwrap();
        let (_, d, e, _) = rational_digits(&r, 4);
        assert_eq!((d.as_str(), e), ("1000", 3));
    }

    #[test]
    fn grouping() {
        assert_eq!(group_latex("1234567.8912"), "1\\,234\\,567.891\\,2");
    }
}
