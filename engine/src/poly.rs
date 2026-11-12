//! Expansion and factorisation.
//!
//! `expand` distributes products over sums. `factor` treats any expression as a polynomial
//! in its "atoms" (variables, or non-polynomial pieces like `sin x`) and:
//!
//! 1. pulls out the rational content and the common monomial (6x²+3x → 3x(2x+1));
//! 2. for a single atom, splits into square-free parts (Yun) and peels off every rational
//!    root (x²−1 → (x−1)(x+1)).
//!
//! Every factorisation is checked by expanding it back; on mismatch the input is returned.

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational as Q;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::EngineError;
use crate::sym::{SR, Sym, add, mul, n, pow};

/// Upper bound on terms produced by a single expansion.
const MAX_TERMS: usize = 5_000;
/// Upper bound on products formed in one distribution step, before like terms merge.
const MAX_PRODUCTS: usize = 100_000;
/// Largest degree we try to factor.
const MAX_FACTOR_DEGREE: usize = 200;
/// Integers above this aren't trial-factored for rational-root candidates.
const MAX_ROOT_CANDIDATE_INT: u64 = 1_000_000_000_000;

fn too_big() -> EngineError {
    EngineError::new(format!("Expansion is too large (more than {MAX_TERMS} terms)"))
}

// ---------- Expand ----------

fn terms_of(s: Sym) -> Vec<Sym> {
    match s {
        Sym::Add(t) => t,
        other => vec![other],
    }
}

/// Multiplies two sums term by term.
fn distribute(a: Vec<Sym>, b: &[Sym]) -> Result<Vec<Sym>, EngineError> {
    if a.len().saturating_mul(b.len()) > MAX_PRODUCTS {
        return Err(too_big());
    }
    let mut out = Vec::with_capacity(a.len() * b.len());
    for x in &a {
        for y in b {
            out.push(mul(vec![x.clone(), y.clone()]));
        }
    }
    let terms = terms_of(add(out));
    if terms.len() > MAX_TERMS {
        return Err(too_big());
    }
    Ok(terms)
}

/// Terms in (t terms)^k fully expanded, at most: C(k+t−1, t−1). Saturates past `MAX_TERMS`.
fn power_terms(t: usize, k: u32) -> usize {
    let mut c: u128 = 1;
    for i in 1..t as u128 {
        c = c * (k as u128 + i) / i;
        if c > MAX_TERMS as u128 {
            return MAX_TERMS + 1;
        }
    }
    c as usize
}

pub fn expand(s: &Sym) -> SR {
    Ok(match s {
        Sym::Num(_) | Sym::Const(_) | Sym::Var(_) | Sym::Unit(_) => s.clone(),
        Sym::Add(t) => add(t.iter().map(expand).collect::<Result<_, _>>()?),
        Sym::Mul(f) => {
            let mut acc = vec![n(1)];
            for factor in f {
                acc = distribute(acc, &terms_of(expand(factor)?))?;
            }
            add(acc)
        }
        Sym::Pow(b, e) => {
            let base = expand(b)?;
            match (e.as_num().and_then(|k| if k.is_integer() { k.to_u32() } else { None }), &base) {
                (Some(k), Sym::Add(t)) if k >= 2 => {
                    // Refuse up front rather than grinding through thousands of steps first.
                    if power_terms(t.len(), k) > MAX_TERMS {
                        return Err(too_big());
                    }
                    if let [a, b] = t.as_slice() {
                        // Binomial theorem: Σ C(k,j)·a^(k−j)·b^j, one term per step.
                        let mut c = BigInt::one();
                        let mut terms = Vec::with_capacity(k as usize + 1);
                        for j in 0..=k {
                            let coeff = Sym::Num(Q::from_integer(c.clone()));
                            terms.push(mul(vec![coeff, pow(a.clone(), n((k - j) as i64))?, pow(b.clone(), n(j as i64))?]));
                            c = c * BigInt::from(k - j) / BigInt::from(j + 1);
                        }
                        return Ok(add(terms));
                    }
                    let mut acc = vec![n(1)];
                    for _ in 0..k {
                        acc = distribute(acc, t)?;
                    }
                    add(acc)
                }
                _ => pow(base, expand(e)?)?,
            }
        }
        Sym::Func(f, x) => Sym::Func(*f, Box::new(expand(x)?)),
        Sym::Log(b, x) => Sym::Log(Box::new(expand(b)?), Box::new(expand(x)?)),
        Sym::Factorial(x, k) => Sym::Factorial(Box::new(expand(x)?), *k),
        Sym::Matrix(m) => Sym::Matrix(m.iter().map(|r| r.iter().map(expand).collect::<Result<_, _>>()).collect::<Result<_, _>>()?),
        Sym::Integral { .. } | Sym::Sum { .. } => s.clone(),
    })
}

// ---------- Univariate polynomials over Q ----------

/// Coefficients from the constant term up: `c[0] + c[1]·t + c[2]·t² + …`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Poly(pub(crate) Vec<Q>);

impl Poly {
    pub(crate) fn trim(mut self) -> Poly {
        while self.0.last().is_some_and(Zero::is_zero) {
            self.0.pop();
        }
        self
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn deg(&self) -> usize {
        self.0.len().saturating_sub(1)
    }

    pub(crate) fn lead(&self) -> &Q {
        self.0.last().expect("non-zero polynomial")
    }

    pub(crate) fn scale(&self, k: &Q) -> Poly {
        Poly(self.0.iter().map(|c| c * k).collect()).trim()
    }

    pub(crate) fn monic(&self) -> Poly {
        self.scale(&self.lead().recip())
    }

    pub(crate) fn mul(&self, o: &Poly) -> Poly {
        if self.is_zero() || o.is_zero() {
            return Poly(vec![]);
        }
        let mut c = vec![Q::zero(); self.0.len() + o.0.len() - 1];
        for (i, a) in self.0.iter().enumerate() {
            for (j, b) in o.0.iter().enumerate() {
                c[i + j] += a * b;
            }
        }
        Poly(c).trim()
    }

    pub(crate) fn derivative(&self) -> Poly {
        Poly(self.0.iter().enumerate().skip(1).map(|(i, c)| c * Q::from_integer(i.into())).collect()).trim()
    }

    pub(crate) fn divmod(&self, d: &Poly) -> (Poly, Poly) {
        let mut r = self.0.clone();
        let dd = d.deg();
        if self.deg() < dd || self.is_zero() {
            return (Poly(vec![]), self.clone());
        }
        let mut q = vec![Q::zero(); self.deg() - dd + 1];
        let lead = d.lead().clone();
        for i in (0..q.len()).rev() {
            let c = &r[i + dd] / &lead;
            if !c.is_zero() {
                for (j, dc) in d.0.iter().enumerate() {
                    r[i + j] -= &c * dc;
                }
            }
            q[i] = c;
        }
        r.truncate(dd);
        (Poly(q).trim(), Poly(r).trim())
    }

    pub(crate) fn gcd(a: &Poly, b: &Poly) -> Poly {
        let (mut a, mut b) = (a.clone(), b.clone());
        while !b.is_zero() {
            let r = a.divmod(&b).1;
            a = b;
            b = r;
        }
        if a.is_zero() { a } else { a.monic() }
    }

    pub(crate) fn eval(&self, x: &Q) -> Q {
        self.0.iter().rev().fold(Q::zero(), |acc, c| acc * x + c)
    }

    /// Scales to integer coefficients with gcd 1 and a positive leading coefficient.
    pub(crate) fn primitive(&self) -> Poly {
        let lcm = self.0.iter().fold(BigInt::one(), |l, c| l.lcm(c.denom()));
        let ints: Vec<BigInt> = self.0.iter().map(|c| (c * Q::from_integer(lcm.clone())).to_integer()).collect();
        let g = ints.iter().fold(BigInt::zero(), |g, c| g.gcd(c));
        let sign = if self.lead().is_negative() { -BigInt::one() } else { BigInt::one() };
        let k = g * sign;
        Poly(ints.into_iter().map(|c| Q::from_integer(c / &k)).collect())
    }

    pub(crate) fn to_sym(&self, atom: &Sym) -> Sym {
        add(self
            .0
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_zero())
            .map(|(i, c)| mul(vec![Sym::Num(c.clone()), pow_int(atom, i as u32)]))
            .collect())
    }
}

pub(crate) fn pow_int(atom: &Sym, k: u32) -> Sym {
    match k {
        0 => n(1),
        1 => atom.clone(),
        _ => pow(atom.clone(), n(k as i64)).unwrap_or_else(|_| Sym::Pow(Box::new(atom.clone()), Box::new(n(k as i64)))),
    }
}

/// Square-free decomposition (Yun): f = c · Π aᵢ^i with each aᵢ square-free and coprime.
fn square_free(f: &Poly) -> Vec<(Poly, u32)> {
    let mut out = Vec::new();
    let fp = f.derivative();
    let a = Poly::gcd(f, &fp);
    if a.deg() == 0 {
        return vec![(f.monic(), 1)];
    }
    let mut b = f.divmod(&a).0;
    let mut c = fp.divmod(&a).0;
    let mut d = c.trim_sub(&b.derivative());
    let mut i = 1;
    loop {
        let g = Poly::gcd(&b, &d);
        if g.deg() > 0 {
            out.push((g.clone(), i));
        }
        b = b.divmod(&g).0;
        if b.deg() == 0 {
            break;
        }
        c = d.divmod(&g).0;
        d = c.trim_sub(&b.derivative());
        i += 1;
        if i > MAX_FACTOR_DEGREE as u32 {
            break;
        }
    }
    out
}

impl Poly {
    pub(crate) fn trim_sub(&self, o: &Poly) -> Poly {
        let len = self.0.len().max(o.0.len());
        let mut c = vec![Q::zero(); len];
        for (i, v) in self.0.iter().enumerate() {
            c[i] += v;
        }
        for (i, v) in o.0.iter().enumerate() {
            c[i] -= v;
        }
        Poly(c).trim()
    }
}

fn divisors(n: &BigInt) -> Option<Vec<BigInt>> {
    let n = n.abs().to_u64().filter(|v| *v <= MAX_ROOT_CANDIDATE_INT)?;
    let mut small = Vec::new();
    let mut large = Vec::new();
    let mut d = 1u64;
    while d * d <= n {
        if n % d == 0 {
            small.push(BigInt::from(d));
            if d * d != n {
                large.push(BigInt::from(n / d));
            }
        }
        d += 1;
    }
    small.extend(large.into_iter().rev());
    Some(small)
}

/// Splits a square-free integer polynomial into linear factors (q·t − p) for each rational
/// root, plus whatever is left.
fn rational_roots(f: Poly) -> (Vec<Poly>, Poly) {
    let mut f = f.primitive();
    let mut linear = Vec::new();
    loop {
        if f.deg() < 1 {
            break;
        }
        if f.0[0].is_zero() {
            linear.push(Poly(vec![Q::zero(), Q::one()]));
            f = f.divmod(&Poly(vec![Q::zero(), Q::one()])).0.primitive();
            continue;
        }
        let (Some(ps), Some(qs)) = (divisors(&f.0[0].to_integer()), divisors(&f.lead().to_integer())) else {
            break;
        };
        let mut found = None;
        'search: for q in &qs {
            for p in &ps {
                for sign in [1, -1] {
                    let r = Q::new(p * sign, q.clone());
                    if f.eval(&r).is_zero() {
                        found = Some(r);
                        break 'search;
                    }
                }
            }
        }
        let Some(r) = found else { break };
        // (q·t − p) with q > 0
        let lin = Poly(vec![-Q::from_integer(r.numer().clone()), Q::from_integer(r.denom().clone())]);
        f = f.divmod(&lin).0.primitive();
        linear.push(lin);
    }
    (linear, f)
}

/// Factors of a univariate polynomial, as (primitive factor, multiplicity).
pub(crate) fn factor_univariate(f: &Poly) -> Vec<(Poly, u32)> {
    let mut out: Vec<(Poly, u32)> = Vec::new();
    for (part, m) in square_free(f) {
        let (lins, rest) = rational_roots(part);
        for l in lins {
            out.push((l, m));
        }
        if rest.deg() >= 1 {
            out.push((rest.primitive(), m));
        }
    }
    out
}

// ---------- Factor ----------

/// A term as rational coefficient × product of atoms to positive integer powers.
pub(crate) type Monomial = (Q, BTreeMap<Sym, u32>);

pub(crate) fn monomial(term: &Sym) -> Monomial {
    let factors = match term {
        Sym::Mul(f) => f.clone(),
        other => vec![other.clone()],
    };
    let mut c = Q::one();
    let mut m = BTreeMap::new();
    for f in factors {
        match f {
            Sym::Num(v) => c *= v,
            Sym::Pow(b, e) if e.as_num().is_some_and(|k| k.is_integer() && k.is_positive()) => {
                let k = e.as_num().and_then(|k| k.to_u32()).unwrap_or(1);
                *m.entry(*b).or_insert(0) += k;
            }
            other => *m.entry(other).or_insert(0) += 1,
        }
    }
    (c, m)
}

fn monomial_sym(c: &Q, m: &BTreeMap<Sym, u32>) -> Sym {
    let mut f = vec![Sym::Num(c.clone())];
    f.extend(m.iter().map(|(a, k)| pow_int(a, *k)));
    mul(f)
}

pub fn factor(s: &Sym) -> SR {
    let e = expand(s)?;
    let Sym::Add(terms) = &e else { return Ok(e) };
    let monos: Vec<Monomial> = terms.iter().map(monomial).collect();

    // Rational content: gcd of numerators over lcm of denominators.
    let num_gcd = monos.iter().fold(BigInt::zero(), |g, (c, _)| g.gcd(c.numer()));
    let den_lcm = monos.iter().fold(BigInt::one(), |l, (c, _)| l.lcm(c.denom()));
    let mut content = Q::new(num_gcd, den_lcm);

    // Common monomial: lowest power of each atom present in every term.
    let mut common: BTreeMap<Sym, u32> = monos[0].1.clone();
    for (_, m) in &monos[1..] {
        common.retain(|a, k| match m.get(a) {
            Some(j) => {
                *k = (*k).min(*j);
                true
            }
            None => false,
        });
    }

    let mut atoms: Vec<Sym> = monos.iter().flat_map(|(_, m)| m.keys().cloned()).collect();
    atoms.sort();
    atoms.dedup();

    // What's left after dividing out content and the common monomial.
    let reduced: Vec<Monomial> = monos
        .iter()
        .map(|(c, m)| {
            let mut m = m.clone();
            for (a, k) in &common {
                let left = m[a] - k;
                if left == 0 {
                    m.remove(a);
                } else {
                    m.insert(a.clone(), left);
                }
            }
            (c / &content, m)
        })
        .collect();

    let mut factors = vec![monomial_sym(&Q::one(), &common)];
    if atoms.len() == 1 {
        let atom = &atoms[0];
        let max_deg = reduced.iter().map(|(_, m)| m.get(atom).copied().unwrap_or(0)).max().unwrap_or(0) as usize;
        if max_deg <= MAX_FACTOR_DEGREE {
            let mut coeffs = vec![Q::zero(); max_deg + 1];
            for (c, m) in &reduced {
                coeffs[m.get(atom).copied().unwrap_or(0) as usize] += c;
            }
            let p = Poly(coeffs).trim();
            let parts = factor_univariate(&p);
            // Leading coefficients absorb into the content so the product matches exactly.
            let product = parts
                .iter()
                .fold(Poly(vec![Q::one()]), |acc, (f, m)| (0..*m).fold(acc, |a, _| a.mul(f)));
            content *= p.lead() / product.lead();
            for (f, m) in &parts {
                factors.push(pow_int(&f.to_sym(atom), *m));
            }
        } else {
            factors.push(add(reduced.iter().map(|(c, m)| monomial_sym(c, m)).collect()));
        }
    } else {
        let rest = add(reduced.iter().map(|(c, m)| monomial_sym(c, m)).collect());
        factors.push(rest);
    }
    factors.insert(0, Sym::Num(content));
    let result = mul(factors);

    // Safety net: the factorisation must expand back to exactly the same thing.
    if expand(&result)? == e { Ok(result) } else { Ok(e) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(c: &[i64]) -> Poly {
        Poly(c.iter().map(|v| Q::from_integer((*v).into())).collect()).trim()
    }

    #[test]
    fn poly_division() {
        // (x² − 1) / (x − 1) = x + 1
        let (q, r) = p(&[-1, 0, 1]).divmod(&p(&[-1, 1]));
        assert_eq!(q, p(&[1, 1]));
        assert!(r.is_zero());
    }

    #[test]
    fn poly_gcd() {
        // gcd((x−1)(x+2), (x−1)(x+3)) = x − 1
        let a = p(&[-1, 1]).mul(&p(&[2, 1]));
        let b = p(&[-1, 1]).mul(&p(&[3, 1]));
        assert_eq!(Poly::gcd(&a, &b), p(&[-1, 1]));
    }

    #[test]
    fn yun_square_free() {
        // (x+1)²(x−2) splits into (x−2)¹ and (x+1)²
        let f = p(&[1, 1]).mul(&p(&[1, 1])).mul(&p(&[-2, 1]));
        let parts = square_free(&f);
        assert!(parts.contains(&(p(&[-2, 1]), 1)));
        assert!(parts.contains(&(p(&[1, 1]), 2)));
    }
}
