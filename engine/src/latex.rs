//! Prints canonical [`Sym`] expressions as textbook LaTeX.

use std::cmp::Reverse;

use num_rational::BigRational as Q;
use num_traits::{One, Signed, Zero};

use crate::ast::{Const, Func};
use crate::number;
use crate::sym::Sym;

pub fn latex(s: &Sym) -> String {
    match s {
        Sym::Num(v) => number::rational_latex(v),
        Sym::Const(Const::Pi) => "\\pi".into(),
        Sym::Const(Const::E) => "e".into(),
        Sym::Var(v) => v.clone(),
        Sym::Add(terms) => add(terms),
        Sym::Mul(_) | Sym::Pow(..) => product(s),
        Sym::Func(f, x) => func(*f, x),
        Sym::Log(b, x) => {
            if *b.as_ref() == Sym::Num(Q::from_integer(10.into())) {
                format!("\\log\\left({}\\right)", latex(x))
            } else {
                format!("\\log_{{{}}}\\left({}\\right)", latex(b), latex(x))
            }
        }
        Sym::Factorial(x, step) => {
            let bangs = "!".repeat(*step as usize);
            if is_atom(x) {
                format!("{}{bangs}", latex(x))
            } else {
                format!("\\left({}\\right){bangs}", latex(x))
            }
        }
    }
}

fn func(f: Func, x: &Sym) -> String {
    let inner = latex(x);
    match f {
        Func::Abs => format!("\\left|{inner}\\right|"),
        Func::Floor => format!("\\left\\lfloor {inner}\\right\\rfloor"),
        Func::Ceil => format!("\\left\\lceil {inner}\\right\\rceil"),
        _ => format!("{}\\left({inner}\\right)", f.latex()),
    }
}

fn is_atom(s: &Sym) -> bool {
    match s {
        Sym::Num(v) => v.is_integer() && !v.is_negative(),
        Sym::Const(_) | Sym::Var(_) => true,
        _ => false,
    }
}

/// Total degree in the variables, used to order polynomial terms (x² + x + 1).
fn degree(s: &Sym) -> Q {
    match s {
        Sym::Var(_) => Q::one(),
        Sym::Pow(b, e) => match e.as_num() {
            Some(e) => degree(b) * e,
            None if b.has_vars() => Q::one(),
            None => Q::zero(),
        },
        Sym::Mul(f) => f.iter().map(degree).sum(),
        Sym::Add(t) => t.iter().map(degree).max().unwrap_or_else(Q::zero),
        Sym::Func(_, x) | Sym::Factorial(x, _) if x.has_vars() => Q::one(),
        Sym::Log(_, x) if x.has_vars() => Q::one(),
        _ => Q::zero(),
    }
}

/// Splits off a negative sign: −3x → (true, 3x).
fn split_sign(t: &Sym) -> (bool, Sym) {
    match t {
        Sym::Num(v) if v.is_negative() => (true, Sym::Num(-v)),
        Sym::Mul(f) => match f.first() {
            Some(Sym::Num(c)) if c.is_negative() => {
                let c = -c;
                let mut rest = f[1..].to_vec();
                if !c.is_one() {
                    rest.insert(0, Sym::Num(c));
                }
                let body = if rest.len() == 1 { rest.pop().unwrap() } else { Sym::Mul(rest) };
                (true, body)
            }
            _ => (false, t.clone()),
        },
        _ => (false, t.clone()),
    }
}

fn add(terms: &[Sym]) -> String {
    let mut ordered: Vec<&Sym> = terms.iter().collect();
    // Variables first by descending degree, then surds, then π/e terms, then plain numbers.
    ordered.sort_by_key(|t| {
        let group = if t.has_vars() {
            0
        } else if matches!(t, Sym::Num(_)) {
            3
        } else if has_const(t) {
            2
        } else {
            1
        };
        (group, Reverse(degree(t)), latex(&split_sign(t).1.without_coeff()))
    });
    let mut out = String::new();
    for (i, t) in ordered.into_iter().enumerate() {
        let (negative, body) = split_sign(t);
        let s = latex(&body);
        match (i, negative) {
            (0, true) => out.push('-'),
            (0, false) => {}
            (_, true) => out.push('-'),
            (_, false) => out.push('+'),
        }
        out.push_str(&s);
    }
    out
}

/// A factor's place in a product: coefficient, roots, constants, variables, the rest.
fn factor_rank(s: &Sym) -> u8 {
    match s {
        Sym::Num(_) => 0,
        Sym::Pow(_, e) if root_index(e).is_some() => 1,
        Sym::Const(_) => 2,
        Sym::Var(_) => 3,
        Sym::Pow(b, _) if matches!(**b, Sym::Var(_)) => 3,
        _ => 4,
    }
}

/// 1/n exponents print as roots.
fn root_index(e: &Sym) -> Option<u64> {
    let v = e.as_num()?;
    if v.numer().is_one() && !v.denom().is_one() {
        num_traits::ToPrimitive::to_u64(v.denom())
    } else {
        None
    }
}

/// Products, quotients and powers: splits into numerator / denominator and prints
/// `\frac{..}{..}` when anything lands below the line.
fn product(s: &Sym) -> String {
    let factors: Vec<Sym> = match s {
        Sym::Mul(f) => f.clone(),
        other => vec![other.clone()],
    };
    let mut negative = false;
    let mut top: Vec<Sym> = Vec::new();
    let mut bottom: Vec<Sym> = Vec::new();
    for f in factors {
        match f {
            Sym::Num(c) => {
                if c.is_negative() {
                    negative = true;
                }
                let c = c.abs();
                if !c.numer().is_one() {
                    top.push(Sym::Num(Q::from_integer(c.numer().clone())));
                }
                if !c.denom().is_one() {
                    bottom.push(Sym::Num(Q::from_integer(c.denom().clone())));
                }
            }
            Sym::Pow(b, e) if e.as_num().is_some_and(|v| v.is_negative()) => {
                let pos = -e.as_num().unwrap();
                if pos.is_one() {
                    bottom.push(*b);
                } else {
                    bottom.push(Sym::Pow(b, Box::new(Sym::Num(pos))));
                }
            }
            other => top.push(other),
        }
    }
    let sign = if negative { "-" } else { "" };
    if bottom.is_empty() {
        format!("{sign}{}", juxtapose(top))
    } else {
        let num = if top.is_empty() { "1".to_string() } else { juxtapose(top) };
        format!("{sign}\\frac{{{num}}}{{{}}}", juxtapose(bottom))
    }
}

/// Writes factors side by side: 2√3πx², merging same-index roots (√5·√x → √(5x)).
fn juxtapose(mut factors: Vec<Sym>) -> String {
    // Bracketed factors in textbook order: (x−1)(x+1), (x−1)(2x+1).
    factors.sort_by_key(|f| (factor_rank(f), degree(f), constant_term(f), latex(f)));
    // Merge roots of equal index into one radical.
    let mut roots: Vec<(u64, Vec<Sym>)> = Vec::new();
    let mut others: Vec<Sym> = Vec::new();
    for f in factors {
        if let Sym::Pow(b, e) = &f
            && let Some(k) = root_index(e) {
                match roots.iter_mut().find(|(i, _)| *i == k) {
                    Some((_, v)) => v.push((**b).clone()),
                    None => roots.push((k, vec![(**b).clone()])),
                }
                continue;
            }
        others.push(f);
    }
    let mut parts: Vec<String> = Vec::new();
    let mut rest = others.into_iter().peekable();
    // Coefficient first, then the radicals, then everything else.
    if let Some(Sym::Num(_)) = rest.peek() {
        parts.push(latex(&rest.next().unwrap()));
    }
    for (k, bases) in roots {
        let inner = juxtapose(bases);
        parts.push(if k == 2 { format!("\\sqrt{{{inner}}}") } else { format!("\\sqrt[{k}]{{{inner}}}") });
    }
    for f in rest {
        parts.push(factor(&f));
    }
    let mut out = String::new();
    for p in parts {
        // 2·3^x: a digit right after another factor needs an explicit dot.
        if !out.is_empty() && p.starts_with(|c: char| c.is_ascii_digit()) {
            out.push_str("\\cdot ");
        } else if ends_with_command(&out) && p.starts_with(|c: char| c.is_ascii_alphabetic()) {
            // `\omega` then `t` must not fuse into `\omegat`.
            out.push(' ');
        }
        out.push_str(&p);
    }
    out
}

/// The plain-number term of a sum (or of a power of a sum), else 0.
fn constant_term(s: &Sym) -> Q {
    match s {
        Sym::Add(t) => t.iter().find_map(|t| t.as_num().cloned()).unwrap_or_else(Q::zero),
        Sym::Pow(b, _) => constant_term(b),
        _ => Q::zero(),
    }
}

fn has_const(s: &Sym) -> bool {
    match s {
        Sym::Const(_) => true,
        Sym::Num(_) | Sym::Var(_) => false,
        Sym::Add(v) | Sym::Mul(v) => v.iter().any(has_const),
        Sym::Pow(a, b) | Sym::Log(a, b) => has_const(a) || has_const(b),
        Sym::Func(_, a) | Sym::Factorial(a, _) => has_const(a),
    }
}

fn ends_with_command(s: &str) -> bool {
    let letters = s.bytes().rev().take_while(u8::is_ascii_alphabetic).count();
    letters > 0 && s.len().checked_sub(letters + 1).is_some_and(|i| s.as_bytes()[i] == b'\\')
}

/// One factor inside a product, parenthesised when needed.
fn factor(f: &Sym) -> String {
    match f {
        Sym::Add(_) => format!("\\left({}\\right)", latex(f)),
        Sym::Pow(b, e) => power(b, e),
        other => latex(other),
    }
}

fn power(b: &Sym, e: &Sym) -> String {
    if let Some(v) = e.as_num() {
        if v.is_negative() {
            return product(&Sym::Pow(Box::new(b.clone()), Box::new(e.clone())));
        }
        if let Some(k) = root_index(e) {
            let inner = latex(b);
            return if k == 2 { format!("\\sqrt{{{inner}}}") } else { format!("\\sqrt[{k}]{{{inner}}}") };
        }
    }
    // sin²(x): a positive integer power of a named function goes on the name.
    if let (Sym::Func(f, x), Some(k)) = (b, e.as_num())
        && k.is_integer() && k.is_positive() && !matches!(f, Func::Abs | Func::Floor | Func::Ceil) {
            return format!("{}^{{{}}}\\left({}\\right)", f.latex(), latex(e), latex(x));
        }
    let base = match b {
        Sym::Num(v) if v.is_integer() && !v.is_negative() => latex(b),
        Sym::Const(_) | Sym::Var(_) => latex(b),
        _ => format!("\\left({}\\right)", latex(b)),
    };
    format!("{base}^{{{}}}", latex(e))
}
