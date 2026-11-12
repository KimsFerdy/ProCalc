//! Number theory: gcd, lcm, mod, binomial coefficients, permutations, primes, prime
//! factorisation, divisors and Euler's totient, with the working where there is some.
//!
//! Primality is Miller–Rabin (deterministic for n < 3.3·10²⁴ with these bases, and a
//! probable-prime test beyond); factoring is trial division then Pollard's rho (Brent).

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational as Q;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::ast::NumFn;
use crate::latex::latex;
use crate::sym::Sym;
use crate::{EngineError, Method, Step};

type R<T> = Result<T, EngineError>;

/// Largest k in C(n, k) or P(n, k) built exactly.
const MAX_CHOOSE: u64 = 200_000;
/// Trial division goes up to this before Pollard's rho takes over.
const TRIAL_LIMIT: u64 = 100_000;
/// Most divisors listed.
const MAX_DIVISORS: usize = 10_000;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn big(i: i64) -> BigInt {
    BigInt::from(i)
}

fn show(n: &BigInt) -> String {
    crate::number::rational_latex(&Q::from_integer(n.clone()))
}

/// A whole-number argument.
pub fn integer(f: NumFn, s: &Sym) -> R<BigInt> {
    match s {
        Sym::Num(q) if q.is_integer() => Ok(q.to_integer()),
        Sym::Num(_) => Err(EngineError::new(format!("{} needs whole numbers", f.name()))),
        _ => Err(EngineError::new(format!("{} needs numbers, not letters", f.name()))),
    }
}

/// The value of a number function, adding its working to `work`.
pub fn apply(f: NumFn, args: &[Sym], work: &mut Vec<Method>) -> R<Sym> {
    let arity = |want: usize| -> R<()> {
        if args.len() == want {
            Ok(())
        } else {
            Err(EngineError::new(format!("{} takes {want} numbers", f.name())))
        }
    };
    let n = |k: usize| integer(f, &args[k]);
    Ok(Sym::Num(Q::from_integer(match f {
        NumFn::Gcd | NumFn::Lcm => {
            if args.is_empty() {
                return Err(EngineError::new(format!("{} needs numbers", f.name())));
            }
            let nums = args.iter().map(|a| integer(f, a)).collect::<R<Vec<_>>>()?;
            let (value, steps) = if f == NumFn::Gcd { gcd_steps(&nums) } else { lcm_steps(&nums) };
            let name = if f == NumFn::Gcd { "Euclid's algorithm" } else { "Through the gcd" };
            work.push(Method { name: name.into(), steps });
            value
        }
        NumFn::Mod => {
            arity(2)?;
            let (Sym::Num(a), Sym::Num(b)) = (&args[0], &args[1]) else {
                return Err(EngineError::new("mod needs numbers, not letters"));
            };
            if b.is_zero() {
                return Err(EngineError::new("mod 0 is undefined"));
            }
            // a mod b = a − b·⌊a/b⌋, always between 0 and b.
            let r = a - b * (a / b).floor();
            work.push(Method {
                name: "Remainder".into(),
                steps: vec![step(
                    format!("{}={}\\cdot {}+{}", latex(&args[0]), latex(&Sym::Num(b.clone())), latex(&Sym::Num((a / b).floor())), latex(&Sym::Num(r.clone()))),
                    "Divide and keep the remainder",
                )],
            });
            return Ok(Sym::Num(r));
        }
        NumFn::Binom => {
            arity(2)?;
            let (nn, k) = (n(0)?, n(1)?);
            choose(&nn, &k)?
        }
        NumFn::Perm => {
            arity(2)?;
            let (nn, k) = (n(0)?, n(1)?);
            if nn.is_negative() || k.is_negative() {
                return Err(EngineError::new("nPr needs whole numbers ≥ 0"));
            }
            if k > nn {
                BigInt::zero()
            } else {
                let k = k.to_u64().filter(|k| *k <= MAX_CHOOSE).ok_or_else(too_big)?;
                (0..k).fold(BigInt::one(), |acc, i| acc * (&nn - big(i as i64)))
            }
        }
        NumFn::Totient => {
            arity(1)?;
            let nn = n(0)?;
            if !nn.is_positive() {
                return Err(EngineError::new("totient needs a whole number ≥ 1"));
            }
            // φ(n) = n·∏(1 − 1/p) over the primes p dividing n.
            factorize(&nn)?.iter().fold(nn.clone(), |acc, (p, _)| acc / p * (p - 1))
        }
        NumFn::IsPrime | NumFn::Divisors | NumFn::Roots => {
            return Err(EngineError::new(format!("{} gives its own kind of answer: put it on a line by itself", f.name())));
        }
    })))
}

fn too_big() -> EngineError {
    EngineError::new(format!("That's too large to work out exactly (k is limited to {MAX_CHOOSE})"))
}

/// C(n, k), exactly. Zero when k is out of range.
pub fn choose(n: &BigInt, k: &BigInt) -> R<BigInt> {
    if n.is_negative() || k.is_negative() {
        return Err(EngineError::new("nCr needs whole numbers ≥ 0"));
    }
    if k > n {
        return Ok(BigInt::zero());
    }
    let k = k.min(&(n - k)).clone();
    let k = k.to_u64().filter(|k| *k <= MAX_CHOOSE).ok_or_else(too_big)?;
    let mut acc = BigInt::one();
    for i in 0..k {
        acc = acc * (n - big(i as i64)) / big(i as i64 + 1);
    }
    Ok(acc)
}

fn gcd_steps(nums: &[BigInt]) -> (BigInt, Vec<Step>) {
    let mut steps = Vec::new();
    let mut g = nums[0].abs();
    for b in &nums[1..] {
        let (mut x, mut y) = (g.clone().max(b.abs()), g.clone().min(b.abs()));
        if nums.len() > 2 {
            steps.push(step(format!("\\gcd\\left({},{}\\right)", show(&g), show(&b.abs())), "Two at a time"));
        }
        while !y.is_zero() {
            let (q, r) = x.div_rem(&y);
            steps.push(step(format!("{}={}\\cdot {}+{}", show(&x), show(&q), show(&y), show(&r)), "Divide; the remainder takes the smaller number's place"));
            x = y;
            y = r;
        }
        g = x;
    }
    steps.push(step(format!("\\gcd={}", show(&g)), "The last non-zero remainder"));
    (g, steps)
}

fn lcm_steps(nums: &[BigInt]) -> (BigInt, Vec<Step>) {
    let mut steps = Vec::new();
    let mut l = nums[0].abs();
    for b in &nums[1..] {
        let g = l.gcd(b);
        let next = if g.is_zero() { BigInt::zero() } else { (&l * b.abs()) / &g };
        steps.push(step(
            format!("\\operatorname{{lcm}}\\left({},{}\\right)=\\frac{{{}\\cdot {}}}{{{}}}={}", show(&l), show(&b.abs()), show(&l), show(&b.abs()), show(&g), show(&next)),
            "$\\operatorname{lcm}(a,b)=\\frac{ab}{\\gcd(a,b)}$",
        ));
        l = next;
    }
    (l, steps)
}

// ---------- Primes ----------

const SMALL_PRIMES: [u64; 16] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53];

/// Miller–Rabin with the first 16 primes as bases: certain below 3.3·10²⁴, and wrong for a
/// larger composite with probability below 4⁻¹⁶.
pub fn is_prime(n: &BigInt) -> bool {
    if *n < big(2) {
        return false;
    }
    for p in SMALL_PRIMES {
        let p = big(p as i64);
        if *n == p {
            return true;
        }
        if (n % &p).is_zero() {
            return false;
        }
    }
    let one = BigInt::one();
    let n1 = n - &one;
    let mut d = n1.clone();
    let mut s = 0u32;
    while d.is_even() {
        d >>= 1;
        s += 1;
    }
    'bases: for a in SMALL_PRIMES {
        let mut x = big(a as i64).modpow(&d, n);
        if x == one || x == n1 {
            continue;
        }
        for _ in 1..s {
            x = x.modpow(&big(2), n);
            if x == n1 {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// A non-trivial factor of a composite n, by Pollard's rho with Brent's cycle detection.
fn pollard(n: &BigInt) -> Option<BigInt> {
    if n.is_even() {
        return Some(big(2));
    }
    for c in 1..20i64 {
        let c = big(c);
        let f = |x: &BigInt| (x * x + &c) % n;
        let (mut y, mut r, mut q) = (big(2), 1u64, BigInt::one());
        let mut g = BigInt::one();
        let mut x = y.clone();
        let mut ys = y.clone();
        let mut iterations = 0u64;
        while g.is_one() {
            x = y.clone();
            for _ in 0..r {
                y = f(&y);
            }
            let mut k = 0;
            while k < r && g.is_one() {
                ys = y.clone();
                for _ in 0..r.min(128).min(r - k) {
                    y = f(&y);
                    q = (q * (&x - &y).abs()) % n;
                }
                g = q.gcd(n);
                k += 128;
            }
            r *= 2;
            iterations += r;
            if iterations > 2_000_000 {
                break;
            }
        }
        if g == *n {
            // The batch overshot: step one at a time.
            loop {
                ys = f(&ys);
                g = (&x - &ys).abs().gcd(n);
                if !g.is_one() {
                    break;
                }
            }
        }
        if !g.is_one() && g != *n {
            return Some(g);
        }
    }
    None
}

/// Prime factors with their powers, smallest first. |n| ≥ 2.
pub fn factorize(n: &BigInt) -> R<Vec<(BigInt, u32)>> {
    let mut rest = n.abs();
    let mut out: Vec<(BigInt, u32)> = Vec::new();
    let push = |p: BigInt, out: &mut Vec<(BigInt, u32)>| match out.iter_mut().find(|(q, _)| *q == p) {
        Some(e) => e.1 += 1,
        None => out.push((p, 1)),
    };
    let mut d = 2u64;
    while d <= TRIAL_LIMIT && big(d as i64) * big(d as i64) <= rest {
        let db = big(d as i64);
        while (&rest % &db).is_zero() {
            rest /= &db;
            push(db.clone(), &mut out);
        }
        d += if d == 2 { 1 } else { 2 };
    }
    let mut stack = vec![rest];
    while let Some(m) = stack.pop() {
        if m.is_one() {
            continue;
        }
        if is_prime(&m) {
            push(m, &mut out);
            continue;
        }
        // A perfect square of a large prime slips past rho's batches; catch it directly.
        let r = m.sqrt();
        if &r * &r == m {
            stack.push(r.clone());
            stack.push(r);
            continue;
        }
        let f = pollard(&m).ok_or_else(|| EngineError::new("This number is too hard to factor here"))?;
        stack.push(&m / &f);
        stack.push(f);
    }
    out.sort();
    Ok(out)
}

/// `2^{3}\cdot 3^{2}\cdot 5`
pub fn factors_latex(fs: &[(BigInt, u32)]) -> String {
    fs.iter()
        .map(|(p, k)| if *k == 1 { show(p) } else { format!("{}^{{{k}}}", show(p)) })
        .collect::<Vec<_>>()
        .join("\\cdot ")
}

/// The factorisation of n with its working: (LaTeX, steps).
pub fn factor_with_steps(n: &BigInt) -> R<(String, Vec<Step>)> {
    if n.abs() < big(2) {
        return Ok((show(n), vec![step(show(n), "0 and ±1 have no prime factors")]));
    }
    let fs = factorize(n)?;
    let mut steps = Vec::new();
    let mut rest = n.abs();
    for (p, k) in &fs {
        let before = rest.clone();
        for _ in 0..*k {
            rest /= p;
        }
        let times = match k {
            1 => "once".to_string(),
            2 => "twice".to_string(),
            k => format!("{k} times"),
        };
        steps.push(step(format!("{}\\div {}^{{{k}}}={}", show(&before), show(p), show(&rest)), format!("Divide by the prime {} {times}", show(p))));
    }
    let sign = if n.is_negative() { "-" } else { "" };
    let shown = format!("{sign}{}", factors_latex(&fs));
    steps.push(step(format!("{}={shown}", show(n)), "Collect the primes"));
    Ok((shown, steps))
}

/// All positive divisors, in order.
pub fn divisors(n: &BigInt) -> R<Vec<BigInt>> {
    if n.is_zero() {
        return Err(EngineError::new("Every number divides 0"));
    }
    if n.abs().is_one() {
        return Ok(vec![BigInt::one()]);
    }
    let fs = factorize(n)?;
    let count: u64 = fs.iter().map(|(_, k)| *k as u64 + 1).product();
    if count > MAX_DIVISORS as u64 {
        return Err(EngineError::new(format!("{n} has {count} divisors: too many to list")));
    }
    let mut ds = vec![BigInt::one()];
    for (p, k) in &fs {
        let mut next = Vec::new();
        for d in &ds {
            let mut m = d.clone();
            for _ in 0..=*k {
                next.push(m.clone());
                m *= p;
            }
        }
        ds = next;
    }
    ds.sort();
    Ok(ds)
}
