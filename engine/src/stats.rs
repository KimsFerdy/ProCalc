//! Statistics of a list, and probability distributions, with the working.
//!
//! Exact wherever the data are: the mean of fractions is a fraction, a standard deviation a
//! surd. Quartiles follow the TI method (medians of the halves, the median itself left out
//! when n is odd); percentiles interpolate linearly (Excel's PERCENTILE.INC).

use num_rational::BigRational as Q;
use num_traits::{Signed, ToPrimitive, Zero};

use crate::ast::{Const, Func, StatFn};
use crate::latex::latex;
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow};
use crate::{EngineError, Method, Step};

type R<T> = Result<T, EngineError>;

/// Values shown in the working before "…".
const SHOWN: usize = 10;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

fn simp(s: Sym) -> Sym {
    sym::normalize_complex(crate::poly::expand(&s).unwrap_or(s))
}

fn list_latex(v: &[Sym]) -> String {
    let mut parts: Vec<String> = v.iter().take(SHOWN).map(latex).collect();
    if v.len() > SHOWN {
        parts.push("\\ldots".into());
    }
    parts.join(",\\ ")
}

fn sum_latex(v: &[Sym]) -> String {
    let mut out = String::new();
    for (i, x) in v.iter().take(SHOWN).enumerate() {
        let s = latex(x);
        if i > 0 && !s.starts_with('-') {
            out.push('+');
        }
        out.push_str(&s);
    }
    if v.len() > SHOWN {
        out.push_str("+\\cdots");
    }
    out
}

/// Whole number argument.
fn whole(s: &Sym, what: &str) -> R<i64> {
    s.as_num().filter(|q| q.is_integer()).and_then(|q| q.to_integer().to_i64()).ok_or_else(|| EngineError::new(format!("{what} must be a whole number")))
}

impl Builder<'_> {
    /// The data: one list ([1,2,3] or a defined list), or the numbers themselves.
    fn data(&mut self, name: &str, args: &[Sym]) -> R<Vec<Sym>> {
        let out = match args {
            [Sym::Matrix(m)] => m.iter().flatten().cloned().collect(),
            _ if args.iter().any(|a| matches!(a, Sym::Matrix(_))) => {
                return Err(EngineError::new(format!("{name} takes one list, like {name}([1, 2, 3]), or the numbers themselves")));
            }
            _ => args.to_vec(),
        };
        if out.is_empty() {
            return Err(EngineError::new(format!("{name} needs at least one value")));
        }
        Ok(out)
    }

    /// The values in increasing order (they must be numbers).
    fn sorted(&mut self, name: &str, mut v: Vec<Sym>) -> R<Vec<Sym>> {
        let mut keyed = Vec::new();
        for x in v.drain(..) {
            if x.has_vars() {
                return Err(EngineError::new(format!("{name} needs numbers to put in order, not letters")));
            }
            let val = self.numeric(&x)?;
            let f = crate::number::float_to_f64(&self.ev.prec.float_of(&val)).ok_or_else(|| EngineError::new(format!("{name} needs real numbers")))?;
            keyed.push((f, x));
        }
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(keyed.into_iter().map(|(_, x)| x).collect())
    }

    fn mean_of(&mut self, v: &[Sym]) -> R<Sym> {
        Ok(simp(div(add(v.to_vec()), n(v.len() as i64))?))
    }

    fn median_of(&mut self, s: &[Sym]) -> R<Sym> {
        let k = s.len();
        Ok(if k % 2 == 1 { s[k / 2].clone() } else { simp(div(add(vec![s[k / 2 - 1].clone(), s[k / 2].clone()]), n(2))?) })
    }

    /// Σ(x − x̄)²
    fn squares(&mut self, v: &[Sym], m: &Sym) -> R<Sym> {
        Ok(simp(add(v.iter().map(|x| pow(add(vec![x.clone(), neg(m.clone())]), n(2))).collect::<R<Vec<_>>>()?)))
    }

    fn two_lists(&mut self, name: &str, args: &[Sym]) -> R<(Vec<Sym>, Vec<Sym>)> {
        let [Sym::Matrix(a), Sym::Matrix(b)] = args else {
            return Err(EngineError::new(format!("{name} takes two lists of the same length, like {name}([1,2,3], [2,4,7])")));
        };
        let (a, b): (Vec<Sym>, Vec<Sym>) = (a.iter().flatten().cloned().collect(), b.iter().flatten().cloned().collect());
        if a.len() != b.len() || a.len() < 2 {
            return Err(EngineError::new(format!("{name} needs two lists of the same length, at least 2 each")));
        }
        Ok((a, b))
    }

    /// Sxy = Σ(x − x̄)(y − ȳ), with the means.
    fn cross_sum(&mut self, xs: &[Sym], ys: &[Sym]) -> R<(Sym, Sym, Sym)> {
        let (mx, my) = (self.mean_of(xs)?, self.mean_of(ys)?);
        let s = simp(add(xs.iter().zip(ys).map(|(x, y)| mul(vec![add(vec![x.clone(), neg(mx.clone())]), add(vec![y.clone(), neg(my.clone())])])).collect()));
        Ok((s, mx, my))
    }

    /// A statistic or a distribution's value.
    pub(crate) fn stat_call(&mut self, f: StatFn, name: &str, args: Vec<Sym>) -> R<Sym> {
        let mut steps = Vec::new();
        let value = match f {
            StatFn::Mean => {
                let v = self.data(name, &args)?;
                let m = self.mean_of(&v)?;
                steps.push(step(format!("\\bar{{x}}=\\frac{{{}}}{{{}}}={}", sum_latex(&v), v.len(), latex(&m)), "Add the values and divide by how many there are"));
                m
            }
            StatFn::Total => {
                let v = self.data(name, &args)?;
                simp(add(v))
            }
            StatFn::Count => n(self.data(name, &args)?.len() as i64),
            StatFn::Median => {
                let v = self.data(name, &args)?;
                let s = self.sorted(name, v)?;
                let m = self.median_of(&s)?;
                let note = if s.len() % 2 == 1 { "The middle value in order" } else { "The mean of the two middle values in order" };
                steps.push(step(format!("{}\\ \\Rightarrow\\ {}", list_latex(&s), latex(&m)), note));
                m
            }
            StatFn::Mode => {
                let v = self.data(name, &args)?;
                let mut counts: Vec<(Sym, usize)> = Vec::new();
                for x in v.iter().map(|x| simp(x.clone())) {
                    match counts.iter_mut().find(|(y, _)| *y == x) {
                        Some(e) => e.1 += 1,
                        None => counts.push((x, 1)),
                    }
                }
                let top = counts.iter().map(|(_, c)| *c).max().unwrap_or(0);
                if top == 1 && v.len() > 1 {
                    return Err(EngineError::new("Every value appears once, so there is no mode"));
                }
                let modes: Vec<Sym> = counts.into_iter().filter(|(_, c)| *c == top).map(|(x, _)| x).collect();
                let modes = self.sorted(name, modes)?;
                steps.push(step(format!("{}\\ (\\times{top})", list_latex(&modes)), "The value that appears most often"));
                if modes.len() == 1 { modes[0].clone() } else { Sym::Matrix(modes.into_iter().map(|x| vec![x]).collect()) }
            }
            StatFn::Var | StatFn::PVar | StatFn::Sd | StatFn::PSd => {
                let v = self.data(name, &args)?;
                let sample = matches!(f, StatFn::Var | StatFn::Sd);
                if sample && v.len() < 2 {
                    return Err(EngineError::new("A sample variance needs at least 2 values"));
                }
                let m = self.mean_of(&v)?;
                let ss = self.squares(&v, &m)?;
                let d = if sample { v.len() - 1 } else { v.len() };
                let var = simp(div(ss.clone(), n(d as i64))?);
                steps.push(step(format!("\\bar{{x}}={}", latex(&m)), "The mean"));
                steps.push(step(format!("\\sum(x-\\bar{{x}})^2={}", latex(&ss)), "Add up the squared distances from the mean"));
                let symbol = if sample { "s^2" } else { "\\sigma^2" };
                let why = if sample { "Divide by $n-1$ (a sample)" } else { "Divide by $n$ (the whole population)" };
                steps.push(step(format!("{symbol}=\\frac{{{}}}{{{d}}}={}", latex(&ss), latex(&var)), why));
                if matches!(f, StatFn::Var | StatFn::PVar) {
                    var
                } else {
                    let sd = pow(var.clone(), sym::q(1, 2))?;
                    steps.push(step(format!("{}=\\sqrt{{{}}}={}", if sample { "s" } else { "\\sigma" }, latex(&var), latex(&sd)), "The square root of the variance"));
                    sd
                }
            }
            StatFn::Min | StatFn::Max | StatFn::Range => {
                let v = self.data(name, &args)?;
                let s = self.sorted(name, v)?;
                let (lo, hi) = (s[0].clone(), s[s.len() - 1].clone());
                match f {
                    StatFn::Min => lo,
                    StatFn::Max => hi,
                    _ => {
                        let r = simp(add(vec![hi.clone(), neg(lo.clone())]));
                        steps.push(step(format!("{}-{}={}", latex(&hi), wrap(&lo), latex(&r)), "Largest minus smallest"));
                        r
                    }
                }
            }
            StatFn::Quartile | StatFn::Iqr => {
                let (list, which) = match (f, args.as_slice()) {
                    (StatFn::Quartile, [l, k]) => (std::slice::from_ref(l), whole(k, "The quartile")?),
                    (StatFn::Quartile, _) => return Err(EngineError::new("Write quartile(L, 1) or quartile(L, 3)")),
                    _ => (args.as_slice(), 0),
                };
                let v = self.data(name, list)?;
                let s = self.sorted(name, v)?;
                let k = s.len();
                let (lower, upper) = (&s[..k / 2], &s[k.div_ceil(2)..]);
                let (q1, q3) = if k == 1 { (s[0].clone(), s[0].clone()) } else { (self.median_of(lower)?, self.median_of(upper)?) };
                steps.push(step(format!("Q_1={},\\quad Q_3={}", latex(&q1), latex(&q3)), "The medians of the lower and upper halves (the median itself left out when $n$ is odd)"));
                match which {
                    1 => q1,
                    3 => q3,
                    0 => {
                        let r = simp(add(vec![q3.clone(), neg(q1.clone())]));
                        steps.push(step(format!("Q_3-Q_1={}", latex(&r)), "The interquartile range"));
                        r
                    }
                    2 => self.median_of(&s)?,
                    _ => return Err(EngineError::new("A quartile is 1, 2 or 3")),
                }
            }
            StatFn::Percentile => {
                let [l, p] = args.as_slice() else { return Err(EngineError::new("Write percentile(L, 90)")) };
                let v = self.data(name, std::slice::from_ref(l))?;
                let s = self.sorted(name, v)?;
                let pq = p.as_num().cloned().filter(|q| !q.is_negative() && *q <= Q::from_integer(100.into())).ok_or_else(|| EngineError::new("A percentile is a number from 0 to 100"))?;
                // rank = p/100·(n − 1), between two neighbours.
                let rank = &pq / Q::from_integer(100.into()) * Q::from_integer(((s.len() - 1) as i64).into());
                let i = rank.floor().to_integer().to_usize().unwrap_or(0).min(s.len() - 1);
                let frac = &rank - rank.floor();
                let value = if frac.is_zero() || i + 1 >= s.len() {
                    s[i].clone()
                } else {
                    simp(add(vec![s[i].clone(), mul(vec![Sym::Num(frac), add(vec![s[i + 1].clone(), neg(s[i].clone())])])]))
                };
                steps.push(step(format!("\\text{{rank}}={}\\ \\Rightarrow\\ {}", latex(&Sym::Num(rank)), latex(&value)), "Position $\\frac{p}{100}(n-1)$ in the ordered list, between neighbours if it isn't whole"));
                value
            }
            StatFn::Corr | StatFn::Cov => {
                let (xs, ys) = self.two_lists(name, &args)?;
                let (sxy, _, _) = self.cross_sum(&xs, &ys)?;
                if f == StatFn::Cov {
                    let c = simp(div(sxy.clone(), n(xs.len() as i64 - 1))?);
                    steps.push(step(format!("\\frac{{{}}}{{{}}}={}", latex(&sxy), xs.len() - 1, latex(&c)), "$\\frac{\\sum(x-\\bar{x})(y-\\bar{y})}{n-1}$"));
                    c
                } else {
                    let mx = self.mean_of(&xs)?;
                    let my = self.mean_of(&ys)?;
                    let sxx = self.squares(&xs, &mx)?;
                    let syy = self.squares(&ys, &my)?;
                    if sxx.is_zero() || syy.is_zero() {
                        return Err(EngineError::new("A list with every value the same has no correlation"));
                    }
                    let r = simp(div(sxy.clone(), pow(mul(vec![sxx.clone(), syy.clone()]), sym::q(1, 2))?)?);
                    steps.push(step(format!("r=\\frac{{{}}}{{\\sqrt{{{}\\cdot {}}}}}={}", latex(&sxy), latex(&sxx), latex(&syy), latex(&r)), "$r=\\frac{S_{xy}}{\\sqrt{S_{xx}S_{yy}}}$"));
                    r
                }
            }
            StatFn::LinReg | StatFn::Summary => return Err(EngineError::new(format!("{name} gives several results: put it on a line by itself"))),
            _ => self.distribution(f, name, &args, &mut steps)?,
        };
        if !steps.is_empty() {
            self.ev.work.push(Method { name: title(f).into(), steps });
        }
        Ok(value)
    }

    fn distribution(&mut self, f: StatFn, name: &str, args: &[Sym], steps: &mut Vec<Step>) -> R<Sym> {
        if args.iter().any(|a| matches!(a, Sym::Matrix(_))) {
            return Err(EngineError::new(format!("{name} takes numbers, not lists")));
        }
        let arity = |lo: usize, hi: usize, usage: &str| -> R<()> {
            if (lo..=hi).contains(&args.len()) { Ok(()) } else { Err(EngineError::new(format!("Write {usage}"))) }
        };
        let probability = |b: &mut Self, p: &Sym| -> R<()> {
            if !p.has_vars() {
                let below = b.sign_of(p)? < 0;
                let above = b.sign_of(&add(vec![p.clone(), n(-1)]))? > 0;
                if below || above {
                    return Err(EngineError::new("A probability must be between 0 and 1"));
                }
            }
            Ok(())
        };
        let e = || Sym::Const(Const::E);
        Ok(match f {
            StatFn::BinomPdf | StatFn::BinomCdf => {
                arity(3, 3, &format!("{name}(n, p, k)"))?;
                let trials = whole(&args[0], "n")?;
                let p = args[1].clone();
                probability(self, &p)?;
                let k = whole(&args[2], "k")?;
                if trials < 0 {
                    return Err(EngineError::new("n must be 0 or more"));
                }
                let term = |i: i64| -> R<Sym> {
                    if i < 0 || i > trials {
                        return Ok(n(0));
                    }
                    let c = crate::numtheory::choose(&trials.into(), &i.into())?;
                    Ok(mul(vec![Sym::Num(Q::from_integer(c)), pow(p.clone(), n(i))?, pow(add(vec![n(1), neg(p.clone())]), n(trials - i))?]))
                };
                if f == StatFn::BinomPdf {
                    // With a letter for p, 10p²(1 − p)³ reads better than its expansion.
                    let raw = term(k)?;
                    let v = if raw.has_vars() { raw } else { simp(raw) };
                    steps.push(step(
                        format!("\\binom{{{trials}}}{{{k}}}{}^{{{k}}}\\left(1-{}\\right)^{{{}}}={}", wrap(&p), latex(&p), trials - k, latex(&v)),
                        "$P(X=k)=\\binom{n}{k}p^k(1-p)^{n-k}$",
                    ));
                    v
                } else {
                    let top = k.min(trials);
                    let v = simp(add((0..=top.max(-1)).map(term).collect::<R<Vec<_>>>()?));
                    steps.push(step(format!("\\sum_{{i=0}}^{{{k}}}\\binom{{{trials}}}{{i}}{}^{{i}}\\left(1-{}\\right)^{{{trials}-i}}={}", wrap(&p), latex(&p), latex(&v)), "$P(X\\le k)$: add the probabilities from 0 to $k$"));
                    v
                }
            }
            StatFn::PoissonPdf | StatFn::PoissonCdf => {
                arity(2, 2, &format!("{name}(λ, k)"))?;
                let lambda = args[0].clone();
                let k = whole(&args[1], "k")?;
                if k < 0 {
                    return Ok(n(0));
                }
                let fact = |i: i64| Sym::Num(Q::from_integer((1..=i).fold(num_bigint::BigInt::from(1), |a, j| a * j)));
                let term = |i: i64| -> R<Sym> { div(pow(lambda.clone(), n(i))?, fact(i)) };
                let lead = pow(e(), neg(lambda.clone()))?;
                if f == StatFn::PoissonPdf {
                    let v = simp(mul(vec![lead, term(k)?]));
                    steps.push(step(format!("\\frac{{e^{{-{}}}{}^{{{k}}}}}{{{k}!}}={}", wrap(&lambda), wrap(&lambda), latex(&v)), "$P(X=k)=\\frac{e^{-\\lambda}\\lambda^k}{k!}$"));
                    v
                } else {
                    let v = simp(mul(vec![lead, add((0..=k).map(term).collect::<R<Vec<_>>>()?)]));
                    steps.push(step(format!("e^{{-{}}}\\sum_{{i=0}}^{{{k}}}\\frac{{{}^{{i}}}}{{i!}}={}", wrap(&lambda), wrap(&lambda), latex(&v)), "$P(X\\le k)$: add the probabilities from 0 to $k$"));
                    v
                }
            }
            StatFn::GeomPdf | StatFn::GeomCdf => {
                arity(2, 2, &format!("{name}(p, k)"))?;
                let p = args[0].clone();
                probability(self, &p)?;
                let k = whole(&args[1], "k")?;
                if k < 1 {
                    return Ok(n(0));
                }
                let miss = add(vec![n(1), neg(p.clone())]);
                if f == StatFn::GeomPdf {
                    let v = simp(mul(vec![pow(miss, n(k - 1))?, p.clone()]));
                    steps.push(step(format!("\\left(1-{}\\right)^{{{}}}{}={}", latex(&p), k - 1, wrap(&p), latex(&v)), "First success on trial $k$: $(1-p)^{k-1}p$"));
                    v
                } else {
                    let v = simp(add(vec![n(1), neg(pow(miss, n(k))?)]));
                    steps.push(step(format!("1-\\left(1-{}\\right)^{{{k}}}={}", latex(&p), latex(&v)), "A success within $k$ trials: $1-(1-p)^k$"));
                    v
                }
            }
            StatFn::NormalPdf => {
                arity(1, 3, &format!("{name}(x, μ, σ)"))?;
                let (x, mu, sigma) = self.normal_args(args, 1)?;
                let z = div(add(vec![x, neg(mu)]), sigma.clone())?;
                let v = simp(div(pow(e(), mul(vec![sym::q(-1, 2), pow(z, n(2))?]))?, mul(vec![sigma, pow(mul(vec![n(2), Sym::Const(Const::Pi)]), sym::q(1, 2))?]))?);
                steps.push(step(latex(&v), "$\\frac{1}{\\sigma\\sqrt{2\\pi}}e^{-\\frac{(x-\\mu)^2}{2\\sigma^2}}$"));
                v
            }
            StatFn::NormalCdf => {
                arity(2, 4, &format!("{name}(a, b, μ, σ)"))?;
                let (a, b) = (args[0].clone(), args[1].clone());
                let (_, mu, sigma) = self.normal_args(&args[1..], 1)?;
                let za = simp(div(add(vec![a, neg(mu.clone())]), sigma.clone())?);
                let zb = simp(div(add(vec![b, neg(mu)]), sigma)?);
                // Φ(b) − Φ(a) = ½(erf(z_b/√2) − erf(z_a/√2))
                let root2 = pow(n(2), sym::q(-1, 2))?;
                let ea = self.func(Func::Erf, simp(mul(vec![za.clone(), root2.clone()])))?;
                let eb = self.func(Func::Erf, simp(mul(vec![zb.clone(), root2])))?;
                let v = simp(mul(vec![sym::q(1, 2), add(vec![eb, neg(ea)])]));
                steps.push(step(format!("z_a={},\\quad z_b={}", latex(&za), latex(&zb)), "Standardise: $z=\\frac{x-\\mu}{\\sigma}$"));
                steps.push(step(format!("\\Phi({})-\\Phi({})={}", latex(&zb), latex(&za), latex(&v)), "$\\Phi(z)=\\frac{1}{2}\\left(1+\\operatorname{erf}\\frac{z}{\\sqrt{2}}\\right)$"));
                v
            }
            StatFn::InvNorm => {
                arity(1, 3, &format!("{name}(p, μ, σ)"))?;
                let p = args[0].clone();
                probability(self, &p)?;
                let (_, mu, sigma) = self.normal_args(args, 1)?;
                let z = self.func(Func::Probit, p.clone())?;
                let v = simp(add(vec![mu, mul(vec![sigma, z.clone()])]));
                steps.push(step(format!("\\mu+\\sigma\\,\\Phi^{{-1}}({})={}", latex(&p), latex(&v)), "The value with probability $p$ below it (Newton's method on $\\Phi$)"));
                v
            }
            _ => unreachable!("handled in stat_call"),
        })
    }

    /// (x, μ, σ) with μ = 0 and σ = 1 when left out; args start with `skip` other values.
    fn normal_args(&mut self, args: &[Sym], skip: usize) -> R<(Sym, Sym, Sym)> {
        let x = args.first().cloned().unwrap_or_else(|| n(0));
        let mu = args.get(skip).cloned().unwrap_or_else(|| n(0));
        let sigma = args.get(skip + 1).cloned().unwrap_or_else(|| n(1));
        if !sigma.has_vars() && self.sign_of(&sigma)? <= 0 {
            return Err(EngineError::new("σ must be positive"));
        }
        Ok((x, mu, sigma))
    }

    /// `linreg(xs, ys)`: (a, b, r) for y = a + bx.
    pub(crate) fn linreg(&mut self, args: &[Sym]) -> R<(Sym, Sym, Sym, Vec<Step>)> {
        let (xs, ys) = self.two_lists("linreg", args)?;
        let (sxy, mx, my) = self.cross_sum(&xs, &ys)?;
        let sxx = self.squares(&xs, &mx)?;
        let syy = self.squares(&ys, &my)?;
        if sxx.is_zero() {
            return Err(EngineError::new("All the x values are the same: no line fits"));
        }
        let b = simp(div(sxy.clone(), sxx.clone())?);
        let a = simp(add(vec![my.clone(), neg(mul(vec![b.clone(), mx.clone()]))]));
        let r = if syy.is_zero() { n(1) } else { simp(div(sxy.clone(), pow(mul(vec![sxx.clone(), syy.clone()]), sym::q(1, 2))?)?) };
        let steps = vec![
            step(format!("\\bar{{x}}={},\\quad \\bar{{y}}={}", latex(&mx), latex(&my)), "The means"),
            step(format!("S_{{xx}}={},\\quad S_{{xy}}={},\\quad S_{{yy}}={}", latex(&sxx), latex(&sxy), latex(&syy)), "Sums of products of the distances from the means"),
            step(format!("b=\\frac{{S_{{xy}}}}{{S_{{xx}}}}={},\\quad a=\\bar{{y}}-b\\bar{{x}}={}", latex(&b), latex(&a)), "The least-squares slope and intercept"),
            step(format!("r=\\frac{{S_{{xy}}}}{{\\sqrt{{S_{{xx}}S_{{yy}}}}}}={}", latex(&r)), "The correlation"),
        ];
        Ok((a, b, r, steps))
    }

    /// `stats(L)`: the usual summary, as (name, value) pairs.
    pub(crate) fn summary(&mut self, args: &[Sym]) -> R<Vec<(&'static str, Sym)>> {
        let v = self.data("stats", args)?;
        let s = self.sorted("stats", v.clone())?;
        let k = s.len();
        let m = self.mean_of(&v)?;
        let ss = self.squares(&v, &m)?;
        let mut out = vec![("n", n(k as i64)), ("\\bar{x}", m.clone()), ("\\sum x", simp(add(v.clone())))];
        if k >= 2 {
            out.push(("s", pow(simp(div(ss.clone(), n(k as i64 - 1))?), sym::q(1, 2))?));
        }
        out.push(("\\sigma", pow(simp(div(ss, n(k as i64))?), sym::q(1, 2))?));
        let (q1, q3) = if k == 1 { (s[0].clone(), s[0].clone()) } else { (self.median_of(&s[..k / 2])?, self.median_of(&s[k.div_ceil(2)..])?) };
        out.push(("\\min", s[0].clone()));
        out.push(("Q_1", q1));
        out.push(("\\text{med}", self.median_of(&s)?));
        out.push(("Q_3", q3));
        out.push(("\\max", s[k - 1].clone()));
        Ok(out)
    }
}

fn title(f: StatFn) -> &'static str {
    match f {
        StatFn::Mean => "Mean",
        StatFn::Median => "Median",
        StatFn::Mode => "Mode",
        StatFn::Var | StatFn::PVar => "Variance",
        StatFn::Sd | StatFn::PSd => "Standard deviation",
        StatFn::Range => "Range",
        StatFn::Quartile | StatFn::Iqr => "Quartiles",
        StatFn::Percentile => "Percentile",
        StatFn::Corr => "Correlation",
        StatFn::Cov => "Covariance",
        StatFn::BinomPdf | StatFn::BinomCdf => "Binomial distribution",
        StatFn::PoissonPdf | StatFn::PoissonCdf => "Poisson distribution",
        StatFn::GeomPdf | StatFn::GeomCdf => "Geometric distribution",
        StatFn::NormalPdf | StatFn::NormalCdf | StatFn::InvNorm => "Normal distribution",
        _ => "Statistics",
    }
}

fn wrap(s: &Sym) -> String {
    match s {
        Sym::Num(v) if v.is_integer() && !v.is_negative() => latex(s),
        Sym::Var(_) | Sym::Const(_) => latex(s),
        _ => format!("\\left({}\\right)", latex(s)),
    }
}
