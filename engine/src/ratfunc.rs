//! Rational functions: cancelling common factors, combining into one fraction, partial
//! fractions and polynomial long division, with the working.
//!
//! In one variable everything is exact over ℚ (polynomial gcd, factoring by rational roots).
//! With several letters, common factors are found by dividing exactly by the factors of the
//! top and the bottom: (x² − y²)/(x − y) = x + y.

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational as Q;
use num_traits::{One, Signed, Zero};

use crate::latex::latex;
use crate::poly::{self, Poly};
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow};
use crate::{EngineError, Method, Step};

type R<T> = Result<T, EngineError>;

/// Highest degree handled for partial fractions and long division.
const MAX_DEGREE: usize = 24;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn expand(s: &Sym) -> R<Sym> {
    poly::expand(s)
}

/// Top and bottom of a product: ⅔·x²·e^{−x}/(x+1) → (2x², 3eˣ(x+1)).
pub fn fraction(s: &Sym) -> R<(Sym, Sym)> {
    let factors = match s {
        Sym::Mul(v) => v.clone(),
        other => vec![other.clone()],
    };
    let (mut top, mut bottom) = (Vec::new(), Vec::new());
    for f in factors {
        match &f {
            Sym::Num(q) if !q.is_integer() => {
                top.push(Sym::Num(Q::from_integer(q.numer().clone())));
                bottom.push(Sym::Num(Q::from_integer(q.denom().clone())));
            }
            Sym::Pow(b, e) if sym::split_coeff((**e).clone()).0.is_negative() => bottom.push(pow((**b).clone(), neg((**e).clone()))?),
            _ => top.push(f),
        }
    }
    Ok((mul(top), mul(bottom)))
}

/// Every atom the polynomial parts are built from (x, y, sin x, √x …).
fn atoms(parts: &[&Sym]) -> R<BTreeSet<Sym>> {
    let mut out = BTreeSet::new();
    for p in parts {
        for t in terms(&expand(p)?) {
            out.extend(poly::monomial(&t).1.into_keys());
        }
    }
    Ok(out)
}

fn terms(s: &Sym) -> Vec<Sym> {
    match s {
        Sym::Add(v) => v.clone(),
        z if z.is_zero() => Vec::new(),
        other => vec![other.clone()],
    }
}

/// s as a polynomial in `atom` with rational coefficients.
fn to_poly(s: &Sym, atom: &Sym) -> R<Option<Poly>> {
    let mut c: Vec<Q> = Vec::new();
    for t in terms(&expand(s)?) {
        let (k, m) = poly::monomial(&t);
        let deg = match m.len() {
            0 => 0,
            1 if m.contains_key(atom) => m[atom] as usize,
            _ => return Ok(None),
        };
        if deg > MAX_DEGREE * 4 {
            return Ok(None);
        }
        if c.len() <= deg {
            c.resize(deg + 1, Q::zero());
        }
        c[deg] += k;
    }
    Ok(Some(Poly(c).trim()))
}

// ---------- Several letters: sparse polynomials ----------

/// Exponent vector (one entry per atom) → coefficient. Keys compare lexicographically, so the
/// last key is the leading term.
#[derive(Debug, Clone, PartialEq)]
struct MPoly(BTreeMap<Vec<u32>, Q>);

impl MPoly {
    fn from_sym(s: &Sym, atoms: &[Sym]) -> R<Option<MPoly>> {
        let mut out = BTreeMap::new();
        for t in terms(&expand(s)?) {
            let (k, m) = poly::monomial(&t);
            let mut e = vec![0u32; atoms.len()];
            for (a, p) in m {
                let Some(i) = atoms.iter().position(|x| *x == a) else { return Ok(None) };
                e[i] = p;
            }
            *out.entry(e).or_insert_with(Q::zero) += k;
        }
        out.retain(|_, c| !c.is_zero());
        Ok(Some(MPoly(out)))
    }

    fn to_sym(&self, atoms: &[Sym]) -> Sym {
        add(self
            .0
            .iter()
            .map(|(e, c)| {
                let mut f = vec![Sym::Num(c.clone())];
                f.extend(e.iter().zip(atoms).filter(|(p, _)| **p > 0).map(|(p, a)| poly::pow_int(a, *p)));
                mul(f)
            })
            .collect())
    }

    /// self / d when d divides it exactly.
    fn div_exact(&self, d: &MPoly) -> Option<MPoly> {
        let (ld, cd) = d.0.iter().next_back()?;
        let mut r = self.clone();
        let mut q = BTreeMap::new();
        let mut guard = 0;
        while let Some((lr, cr)) = r.0.iter().next_back().map(|(e, c)| (e.clone(), c.clone())) {
            guard += 1;
            if guard > 10_000 || lr.iter().zip(ld).any(|(a, b)| a < b) {
                return None;
            }
            let shift: Vec<u32> = lr.iter().zip(ld).map(|(a, b)| a - b).collect();
            let c = &cr / cd;
            for (e, k) in &d.0 {
                let key: Vec<u32> = e.iter().zip(&shift).map(|(a, b)| a + b).collect();
                let v = r.0.entry(key.clone()).or_insert_with(Q::zero);
                *v -= &c * k;
                if v.is_zero() {
                    r.0.remove(&key);
                }
            }
            q.insert(shift, c);
        }
        Some(MPoly(q))
    }
}

/// The factors of s worth trying as common factors: (x−y)²·x·3 → [x−y, x−y, x].
fn factor_list(s: &Sym) -> R<Vec<Sym>> {
    let f = poly::factor(s)?;
    let parts = match f {
        Sym::Mul(v) => v,
        other => vec![other],
    };
    let mut out = Vec::new();
    for p in parts {
        match &p {
            Sym::Num(_) => {}
            Sym::Pow(b, e) if e.as_num().is_some_and(|k| k.is_integer() && k.is_positive()) => {
                let k = e.as_num().and_then(|k| num_traits::ToPrimitive::to_usize(&k.to_integer())).unwrap_or(1).min(MAX_DEGREE);
                out.extend(std::iter::repeat_n((**b).clone(), k));
            }
            _ => out.push(p),
        }
    }
    Ok(out)
}

/// top/bottom with common factors cancelled, or None when nothing cancels. Also returns the
/// common factor that went, for the working.
fn cancel_pair(top: &Sym, bottom: &Sym) -> R<Option<(Sym, Sym)>> {
    if bottom.is_one() || top.is_zero() {
        return Ok(None);
    }
    let set = atoms(&[top, bottom])?;
    let list: Vec<Sym> = set.into_iter().collect();
    let (mut t, mut b, mut common) = (expand(top)?, expand(bottom)?, Vec::new());
    if list.len() == 1 {
        let atom = &list[0];
        let (Some(pt), Some(pb)) = (to_poly(&t, atom)?, to_poly(&b, atom)?) else { return Ok(None) };
        let g = Poly::gcd(&pt, &pb);
        if g.deg() >= 1 {
            common.push(g.to_sym(atom));
            t = pt.divmod(&g).0.to_sym(atom);
            b = pb.divmod(&g).0.to_sym(atom);
        }
    } else if !list.is_empty() {
        // Divide by each factor of the bottom that divides the top, then the other way round.
        for f in factor_list(&b)? {
            let (Some(mt), Some(mb), Some(mf)) = (MPoly::from_sym(&t, &list)?, MPoly::from_sym(&b, &list)?, MPoly::from_sym(&f, &list)?) else {
                continue;
            };
            if let (Some(qt), Some(qb)) = (mt.div_exact(&mf), mb.div_exact(&mf)) {
                t = qt.to_sym(&list);
                b = qb.to_sym(&list);
                common.push(f);
            }
        }
        if let (Some(mt), Some(mb)) = (MPoly::from_sym(&t, &list)?, MPoly::from_sym(&b, &list)?)
            && b.has_vars()
            && let Some(q) = mt.div_exact(&mb)
        {
            common.push(b.clone());
            t = q.to_sym(&list);
            b = n(1);
        }
    }
    // Numbers: (2x + 4)/6 = (x + 2)/3.
    let ct = numeric_content(&t)?;
    let cb = numeric_content(&b)?;
    let shared = gcd_q(&ct, &cb);
    if common.is_empty() && shared.is_one() {
        return Ok(None);
    }
    let t_prim = expand(&div(t, Sym::Num(ct.clone()))?)?;
    let b_prim = expand(&div(b, Sym::Num(cb.clone()))?)?;
    let result = mul(vec![Sym::Num(ct / cb), t_prim, pow(b_prim, n(-1))?]);
    let shown = if common.is_empty() { Sym::Num(shared) } else { mul(common) };
    Ok(Some((result, shown)))
}

/// The rational content of a polynomial in any letters.
fn numeric_content(s: &Sym) -> R<Q> {
    let ts = terms(&expand(s)?);
    if ts.is_empty() {
        return Ok(Q::one());
    }
    let cs: Vec<Q> = ts.iter().map(|t| poly::monomial(t).0).collect();
    let num = cs.iter().fold(BigInt::zero(), |g, c| g.gcd(c.numer()));
    let den = cs.iter().fold(BigInt::one(), |l, c| l.lcm(c.denom()));
    Ok(Q::new(num, den))
}

fn gcd_q(a: &Q, b: &Q) -> Q {
    Q::new(a.numer().gcd(b.numer()), a.denom().lcm(b.denom()))
}

impl Builder<'_> {
    /// Cancels common factors in each fraction of an answer, recording the working; the
    /// answer is unchanged when nothing cancels.
    pub(crate) fn tidy_fractions(&mut self, s: Sym) -> R<Sym> {
        let parts = match &s {
            Sym::Add(v) => v.clone(),
            other => vec![other.clone()],
        };
        let mut steps = Vec::new();
        let mut out = Vec::new();
        let mut changed = false;
        for p in parts {
            let (top, bottom) = fraction(&p)?;
            match cancel_pair(&top, &bottom)? {
                Some((r, common)) => {
                    steps.push(step(
                        format!("\\frac{{{}}}{{{}}}={}", latex(&poly::factor(&top)?), latex(&poly::factor(&bottom)?), latex(&r)),
                        format!("Cancel the common factor ${}$", latex(&common)),
                    ));
                    out.push(r);
                    changed = true;
                }
                None => out.push(p),
            }
        }
        if !changed {
            return Ok(s);
        }
        self.ev.work.push(Method { name: "Simplify".into(), steps });
        Ok(add(out))
    }

    /// `simplify(e)`: one fraction with no common factors.
    pub(crate) fn simplify_fraction(&mut self, s: &Sym) -> R<Sym> {
        let mut steps = Vec::new();
        let (top, bottom) = self.together_parts(s, &mut steps)?;
        let result = match cancel_pair(&top, &bottom)? {
            Some((r, common)) => {
                steps.push(step(
                    format!("\\frac{{{}}}{{{}}}={}", latex(&poly::factor(&top)?), latex(&poly::factor(&bottom)?), latex(&r)),
                    format!("Factor the top and the bottom, and cancel ${}$", latex(&common)),
                ));
                r
            }
            None => {
                let r = div(top, bottom)?;
                if steps.is_empty() {
                    steps.push(step(latex(&r), "Nothing cancels"));
                }
                r
            }
        };
        self.ev.work.push(Method { name: "Simplify".into(), steps });
        Ok(result)
    }

    /// The terms of a sum over a common denominator: (top, bottom).
    fn together_parts(&mut self, s: &Sym, steps: &mut Vec<Step>) -> R<(Sym, Sym)> {
        let parts: Vec<(Sym, Sym)> = terms(s).iter().map(fraction).collect::<R<_>>()?;
        if parts.len() <= 1 {
            return Ok(parts.into_iter().next().unwrap_or((n(0), n(1))));
        }
        // Each distinct denominator once; repeated ones aren't multiplied twice.
        let mut dens: Vec<Sym> = Vec::new();
        for (_, d) in &parts {
            if !d.is_one() && !dens.contains(d) {
                dens.push(d.clone());
            }
        }
        if dens.is_empty() {
            return Ok((s.clone(), n(1)));
        }
        let bottom = mul(dens.clone());
        let mut tops = Vec::new();
        for (t, d) in &parts {
            let others: Vec<Sym> = dens.iter().filter(|x| *x != d).cloned().collect();
            tops.push(mul(vec![t.clone(), mul(others)]));
        }
        let top = expand(&add(tops))?;
        steps.push(step(
            format!("\\frac{{{}}}{{{}}}", latex(&top), latex(&bottom)),
            "Put everything over a common denominator and add the tops",
        ));
        Ok((top, bottom))
    }

    /// `apart(e)`: partial fractions in the one variable of e.
    pub(crate) fn apart(&mut self, s: &Sym) -> R<Sym> {
        let mut steps = Vec::new();
        let (top, bottom) = self.together_parts(s, &mut steps)?;
        let set = atoms(&[&top, &bottom])?;
        let x = match set.into_iter().collect::<Vec<_>>().as_slice() {
            [v @ Sym::Var(_)] => v.clone(),
            [] => return Ok(s.clone()),
            _ => return Err(EngineError::new("Partial fractions need a fraction of polynomials in one variable")),
        };
        let (Some(mut pt), Some(mut pb)) = (to_poly(&top, &x)?, to_poly(&bottom, &x)?) else {
            return Err(EngineError::new("Partial fractions need a fraction of polynomials"));
        };
        if pb.deg() == 0 {
            return Ok(s.clone());
        }
        if pb.deg() > MAX_DEGREE {
            return Err(EngineError::new(format!("The bottom's degree is over {MAX_DEGREE}")));
        }
        // Cancel first: (x² − 1)/(x − 1) has nothing to split.
        let g = Poly::gcd(&pt, &pb);
        if g.deg() >= 1 {
            pt = pt.divmod(&g).0;
            pb = pb.divmod(&g).0;
            steps.push(step(format!("\\frac{{{}}}{{{}}}", latex(&pt.to_sym(&x)), latex(&pb.to_sym(&x))), format!("Cancel ${}$", latex(&g.to_sym(&x)))));
        }
        let mut pieces = Vec::new();
        let (q, r) = pt.divmod(&pb);
        if !q.is_zero() {
            steps.push(step(
                format!("{}+\\frac{{{}}}{{{}}}", latex(&q.to_sym(&x)), latex(&r.to_sym(&x)), latex(&pb.to_sym(&x))),
                "The top's degree isn't lower than the bottom's: divide first",
            ));
            pieces.push(q.to_sym(&x));
        }
        if r.is_zero() {
            let result = add(pieces);
            self.ev.work.push(Method { name: "Partial fractions".into(), steps });
            return Ok(result);
        }
        let factors = poly::factor_univariate(&pb);
        let shown_bottom = mul(factors.iter().map(|(f, m)| poly::pow_int(&f.to_sym(&x), *m)).collect());
        // pb = lead · Π fᵢ^mᵢ
        let product = factors.iter().fold(Poly(vec![Q::one()]), |acc, (f, m)| (0..*m).fold(acc, |a, _| a.mul(f)));
        let scale = pb.lead() / product.lead();
        steps.push(step(
            format!("{}={}", latex(&pb.to_sym(&x)), latex(&mul(vec![Sym::Num(scale.clone()), shown_bottom]))),
            "Factor the bottom",
        ));
        // Unknowns: a numerator of degree < deg f for each power f^j, j ≤ m.
        let mut slots: Vec<(Poly, u32, usize)> = Vec::new(); // (factor, power, degree)
        for (f, m) in &factors {
            for j in 1..=*m {
                slots.push((f.clone(), j, f.deg()));
            }
        }
        let size = pb.deg();
        let names: Vec<String> = (0..size)
            .map(|i| if i < 8 { ((b'A' + i as u8) as char).to_string() } else { format!("A_{{{}}}", i + 1) })
            .collect();
        let mut template = Vec::new();
        let mut columns: Vec<Poly> = Vec::new();
        let mut k = 0;
        for (f, j, d) in &slots {
            let fj = (0..*j).fold(Poly(vec![Q::one()]), |a, _| a.mul(f));
            let cofactor = pb.divmod(&fj).0;
            let mut num = Vec::new();
            for t in (0..*d).rev() {
                let mut basis = vec![Q::zero(); t + 1];
                basis[t] = Q::one();
                columns.push(Poly(basis).trim().mul(&cofactor));
                let var = Sym::Var(names[k].clone());
                num.push(mul(vec![var, poly::pow_int(&x, t as u32)]));
                k += 1;
            }
            template.push(format!("\\frac{{{}}}{{{}}}", latex(&add(num)), latex(&poly::pow_int(&f.to_sym(&x), *j))));
        }
        steps.push(step(
            format!("\\frac{{{}}}{{{}}}={}", latex(&r.to_sym(&x)), latex(&pb.to_sym(&x)), template.join("+")),
            "One fraction for each factor and each power of it, with a top of lower degree",
        ));
        // r = Σ unknownᵢ · columnᵢ: match coefficients of x⁰ … x^{size−1}.
        let mut matrix: Vec<Vec<Q>> = (0..size)
            .map(|row| {
                let mut v: Vec<Q> = columns.iter().map(|c| c.0.get(row).cloned().unwrap_or_else(Q::zero)).collect();
                v.push(r.0.get(row).cloned().unwrap_or_else(Q::zero));
                v
            })
            .collect();
        let values = solve_linear(&mut matrix).ok_or_else(|| EngineError::new("Couldn't solve for the partial fractions"))?;
        steps.push(step(
            values.iter().zip(&names).map(|(v, nm)| format!("{nm}={}", latex(&Sym::Num(v.clone())))).collect::<Vec<_>>().join(",\\ "),
            format!("Multiply both sides by ${}$ and match the coefficients of each power of ${}$", latex(&pb.to_sym(&x)), latex(&x)),
        ));
        let mut k = 0;
        for (f, j, d) in &slots {
            let coeffs: Vec<Q> = (0..*d).rev().map(|_| {
                let v = values[k].clone();
                k += 1;
                v
            }).collect::<Vec<_>>().into_iter().rev().collect();
            let numerator = Poly(coeffs).trim();
            if numerator.is_zero() {
                continue;
            }
            pieces.push(mul(vec![numerator.to_sym(&x), pow(f.to_sym(&x), n(-(*j as i64)))?]));
        }
        let result = add(pieces);
        steps.push(step(latex(&result), "The partial fractions"));
        self.ev.work.push(Method { name: "Partial fractions".into(), steps });
        Ok(result)
    }

    /// Polynomial (or whole-number) long division: (quotient, remainder).
    pub(crate) fn long_division(&mut self, a: &Sym, d: &Sym) -> R<(Sym, Sym)> {
        if d.is_zero() {
            return Err(EngineError::new("Division by zero"));
        }
        let mut steps = Vec::new();
        // Whole numbers: 17 = 3·5 + 2.
        if let (Some(p), Some(q)) = (a.as_num(), d.as_num()) {
            if !p.is_integer() || !q.is_integer() {
                return Err(EngineError::new("Long division needs whole numbers or polynomials"));
            }
            let (p, q) = (p.to_integer(), q.to_integer());
            let quo = p.div_floor(&q);
            let rem = &p - &quo * &q;
            let (qs, rs) = (Sym::Num(Q::from_integer(quo)), Sym::Num(Q::from_integer(rem)));
            steps.push(step(format!("{}={}\\cdot {}+{}", latex(a), latex(&qs), latex(d), latex(&rs)), "How many times it goes in, and what's left"));
            self.ev.work.push(Method { name: "Long division".into(), steps });
            return Ok((qs, rs));
        }
        let set = atoms(&[a, d])?;
        let x = match set.into_iter().collect::<Vec<_>>().as_slice() {
            [v] => v.clone(),
            _ => return Err(EngineError::new("Long division needs polynomials in one variable")),
        };
        let (Some(pa), Some(pd)) = (to_poly(a, &x)?, to_poly(d, &x)?) else {
            return Err(EngineError::new("Long division needs polynomials"));
        };
        if pa.deg() > MAX_DEGREE * 2 {
            return Err(EngineError::new("That polynomial's degree is too high to divide here"));
        }
        // Step by step: lead term over lead term, multiply back, subtract.
        let mut rest = pa.clone();
        let mut quotient = Poly(vec![]);
        let lead_d = pd.lead().clone();
        let mut shown = 0;
        while !rest.is_zero() && rest.deg() >= pd.deg() {
            let shift = rest.deg() - pd.deg();
            let c = rest.lead() / &lead_d;
            let mut t = vec![Q::zero(); shift + 1];
            t[shift] = c;
            let term = Poly(t);
            let back = term.mul(&pd);
            let next = rest.trim_sub(&back);
            if shown < 8 {
                steps.push(step(
                    format!(
                        "{}\\div {}={},\\quad {}-{}={}",
                        latex(&leading_term(&rest, &x)),
                        latex(&leading_term(&pd, &x)),
                        latex(&term.to_sym(&x)),
                        wrap(&rest.to_sym(&x)),
                        wrap(&back.to_sym(&x)),
                        latex(&next.to_sym(&x))
                    ),
                    "Divide the leading terms, multiply back, subtract",
                ));
                shown += 1;
            }
            quotient = quotient.trim_sub(&term.scale(&-Q::one()));
            rest = next;
        }
        let (qs, rs) = (quotient.to_sym(&x), rest.to_sym(&x));
        steps.push(step(
            format!("{}=\\left({}\\right)\\left({}\\right)+{}", latex(a), latex(d), latex(&qs), latex(&rs)),
            "The quotient and the remainder (its degree is below the divisor's)",
        ));
        self.ev.work.push(Method { name: "Long division".into(), steps });
        Ok((qs, rs))
    }
}

fn leading_term(p: &Poly, x: &Sym) -> Sym {
    mul(vec![Sym::Num(p.lead().clone()), poly::pow_int(x, p.deg() as u32)])
}

fn wrap(s: &Sym) -> String {
    if matches!(s, Sym::Add(_)) { format!("\\left({}\\right)", latex(s)) } else { latex(s) }
}

/// Solves the augmented square system in place (Gauss–Jordan over ℚ).
fn solve_linear(m: &mut [Vec<Q>]) -> Option<Vec<Q>> {
    let size = m.len();
    for col in 0..size {
        let pivot = (col..size).find(|&r| !m[r][col].is_zero())?;
        m.swap(col, pivot);
        let p = m[col][col].clone();
        for v in m[col].iter_mut() {
            *v = &*v / &p;
        }
        for r in 0..size {
            if r != col && !m[r][col].is_zero() {
                let f = m[r][col].clone();
                let row = m[col].clone();
                for (v, w) in m[r].iter_mut().zip(row) {
                    *v -= &f * w;
                }
            }
        }
    }
    Some(m.iter().map(|row| row[size].clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_division_in_several_letters() {
        let (x, y) = (Sym::Var("x".into()), Sym::Var("y".into()));
        let atoms = vec![x.clone(), y.clone()];
        // (x² − y²)/(x − y) = x + y
        let top = add(vec![pow(x.clone(), n(2)).unwrap(), neg(pow(y.clone(), n(2)).unwrap())]);
        let bottom = add(vec![x.clone(), neg(y.clone())]);
        let q = MPoly::from_sym(&top, &atoms).unwrap().unwrap().div_exact(&MPoly::from_sym(&bottom, &atoms).unwrap().unwrap()).unwrap();
        assert_eq!(q.to_sym(&atoms), add(vec![x.clone(), y.clone()]));
        // x² + y² isn't divisible by x − y.
        let sum = add(vec![pow(x, n(2)).unwrap(), pow(y, n(2)).unwrap()]);
        assert!(MPoly::from_sym(&sum, &atoms).unwrap().unwrap().div_exact(&MPoly::from_sym(&bottom, &atoms).unwrap().unwrap()).is_none());
    }
}
