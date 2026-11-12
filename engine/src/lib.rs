//! ProCalc's maths engine.
//!
//! Input is LaTeX as produced by MathLive (plain text like `sqrt(2)*pi` works too).
//! Results stay exact (fractions) whenever possible and fall back to
//! arbitrary-precision floating point with a user-chosen number of digits.

pub(crate) mod ast;
mod calculus;
mod complex;
mod defs;
mod eval;
mod figure;
mod gamma;
mod geometry;
mod latex;
mod limits;
mod matrix;
mod number;
mod numtheory;
mod ode;
mod optimize;
mod parser;
mod plot;
mod poly;
mod ratfunc;
mod solve;
mod special;
mod stats;
mod sums;
mod sym;
mod units;
#[cfg(test)]
mod tests_calculus;
#[cfg(test)]
mod tests_complex;
#[cfg(test)]
mod tests_defs;
#[cfg(test)]
mod tests_figure;
#[cfg(test)]
mod tests_geometry;
#[cfg(test)]
mod tests_inequalities;
#[cfg(test)]
mod tests_matrix;
#[cfg(test)]
mod tests_numtheory;
#[cfg(test)]
mod tests_ode;
#[cfg(test)]
mod tests_plot;
#[cfg(test)]
mod tests_series;
#[cfg(test)]
mod tests_solve;
#[cfg(test)]
mod tests_stats;
#[cfg(test)]
mod tests_steps;
#[cfg(test)]
mod tests_symbolic;
#[cfg(test)]
mod tests_units;

use serde::{Deserialize, Serialize};

pub use figure::{FigureInput, FigureResult, solve_figure};
pub use parser::parse;
pub use plot::{Curve, CurveKind, View, plot_sheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AngleMode {
    #[default]
    Radians,
    Degrees,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Significant digits shown in decimal results.
    pub digits: usize,
    pub angle: AngleMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { digits: 32, angle: AngleMode::Radians }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct EngineError {
    pub message: String,
    /// Byte offset into the input, when the error points at a spot.
    pub position: Option<usize>,
}

impl EngineError {
    pub fn new(message: impl Into<String>) -> Self {
        EngineError { message: message.into(), position: None }
    }

    pub fn at(message: impl Into<String>, position: usize) -> Self {
        EngineError { message: message.into(), position: Some(position) }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DecimalAnswer {
    /// Decimal result as LaTeX, with digit grouping.
    pub latex: String,
    /// Decimal result as plain text, for copying.
    pub text: String,
    /// True when the decimal shown is the complete value (not rounded).
    pub is_exact: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    /// Exact or simplified symbolic result as LaTeX (`\frac{1}{3}`, `2\sqrt{2}`, `3x+1`).
    /// Absent when the closed form would only repeat the input (e.g. `\sin(1)`).
    pub exact_latex: Option<String>,
    /// Decimal value; absent when the result still contains variables.
    pub decimal: Option<DecimalAnswer>,
    /// True when the result contains variables.
    pub symbolic: bool,
    /// For a definition (`x := 5`, `f(x) := x^2`): the name defined, as LaTeX.
    /// The caller passes such lines back to [`evaluate_with`] so later lines can use them.
    pub defines: Option<String>,
    /// Worked solutions, one per method (Substitution, Cramer's rule…), for the Steps view.
    /// Empty when there is no working to show.
    pub methods: Vec<Method>,
}

/// One way of working out an answer, as a list of steps.
#[derive(Debug, Clone, Serialize)]
pub struct Method {
    pub name: String,
    pub steps: Vec<Step>,
}

/// One line of working: maths (LaTeX) with a short explanation.
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub latex: String,
    pub note: String,
}

pub const MIN_DIGITS: usize = 1;
pub const MAX_DIGITS: usize = 1000;

pub(crate) fn evaluator(settings: &Settings) -> eval::Evaluator {
    let digits = settings.digits.clamp(MIN_DIGITS, MAX_DIGITS);
    eval::Evaluator { prec: number::Prec::new(digits), angle: settings.angle, calls: 0, vars: Default::default(), work: Vec::new() }
}

/// Evaluates at rising precision until two runs agree on every digit shown.
///
/// Some calculations lose digits along the way (sin(10^50 + π) needs ~170 extra bits just
/// to reduce the angle), so a single fixed precision can't promise correct output.
/// Agreement between independent precisions is the guarantee.
fn reliable_decimal(expr: &ast::Expr, settings: &Settings) -> Result<DecimalAnswer, EngineError> {
    let digits = settings.digits.clamp(MIN_DIGITS, MAX_DIGITS);
    let base = number::Prec::base_bits(digits);
    let max_bits = (base * 16).max(20_000);
    let mut bits = base;
    let mut previous: Option<DecimalAnswer> = None;
    while bits <= max_bits {
        let mut ev = eval::Evaluator { prec: number::Prec::with_bits(digits, bits), angle: settings.angle, calls: 0, vars: Default::default(), work: Vec::new() };
        let value = ev.eval(expr)?;
        let d = decimal(&value, &mut ev)?;
        if previous.as_ref().is_some_and(|p| p.text == d.text) {
            return Ok(d);
        }
        previous = Some(d);
        bits = bits * 2 + 64;
    }
    Err(EngineError::new(
        "Couldn't reach the requested accuracy: this calculation loses too many digits",
    ))
}

fn decimal(value: &number::Value, ev: &mut eval::Evaluator) -> Result<DecimalAnswer, EngineError> {
    let d = number::format_value(value, &mut ev.prec)
        .ok_or_else(|| EngineError::new("The result is too large or too small to show"))?;
    Ok(DecimalAnswer { latex: d.latex, text: d.text, is_exact: d.exact })
}

pub fn evaluate(input: &str, settings: &Settings) -> Result<Answer, EngineError> {
    evaluate_with(input, &[], settings)
}

/// Evaluates `input` after replaying the earlier definition lines in `defs`, oldest first.
/// Lines in `defs` that aren't definitions or no longer evaluate are ignored.
pub fn evaluate_with(input: &str, defs: &[String], settings: &Settings) -> Result<Answer, EngineError> {
    let mut ev = evaluator(settings);
    let mut env = defs::Env::from_lines(defs, &mut ev);
    evaluate_in(&mut env, input, &mut ev, settings)
}

/// Evaluates every line of a sheet (the Paper) in order. Each definition that works is
/// available to the lines after it, so a whole sheet costs one pass instead of one replay
/// per line.
pub fn evaluate_sheet(lines: &[String], settings: &Settings) -> Vec<Result<Answer, EngineError>> {
    let mut ev = evaluator(settings);
    let mut env = defs::Env::default();
    lines.iter().map(|line| evaluate_in(&mut env, line, &mut ev, settings)).collect()
}

/// Evaluates one line against `env`, adding it to `env` if it is a definition.
fn evaluate_in(
    env: &mut defs::Env,
    input: &str,
    ev: &mut eval::Evaluator,
    settings: &Settings,
) -> Result<Answer, EngineError> {
    ev.work.clear();
    let mut answer = evaluate_line(env, input, ev, settings)?;
    // Working gathered while building (derivatives, integrals, substitutions).
    if answer.methods.is_empty() {
        answer.methods = std::mem::take(&mut ev.work);
    }
    Ok(answer)
}

fn evaluate_line(
    env: &mut defs::Env,
    input: &str,
    ev: &mut eval::Evaluator,
    settings: &Settings,
) -> Result<Answer, EngineError> {
    ev.calls = 0;
    let mut indefinite = false;
    let mut series = false;
    let (s, defines) = match parser::parse_statement(input, &env.func_names())? {
        ast::Stmt::Expr(expr) => {
            if let Some(answer) = number_answer(&expr, env, ev)? {
                return Ok(answer);
            }
            if let Some(answer) = complex_answer(&expr, env, ev, settings)? {
                return Ok(answer);
            }
            indefinite = matches!(expr, ast::Expr::Integral { bounds: None, .. });
            series = matches!(expr, ast::Expr::Taylor { .. });
            let mut b = sym::Builder { ev: &mut *ev, env };
            // divide(p, d) on its own line: the quotient and the remainder by name.
            if let ast::Expr::PolyDiv { top, bottom, part: ast::DivPart::Both } = &expr {
                let (a, d) = (b.build(top)?, b.build(bottom)?);
                let (q, r) = b.long_division(&a, &d)?;
                let value = sym::add(vec![q.clone(), sym::div(r.clone(), d.clone())?]);
                let text = format!("quotient {}, remainder {}", latex::latex(&q), latex::latex(&r));
                // 17 = 3·5 + 2 for whole numbers; x² + x + 1 + 2/(x − 1) for polynomials.
                let shown = if value.has_vars() {
                    latex::latex(&value)
                } else {
                    format!("{}={}\\cdot {}+{}", latex::latex(&a), latex::latex(&q), latex::latex(&d), latex::latex(&r))
                };
                return Ok(Answer {
                    exact_latex: Some(shown),
                    decimal: Some(DecimalAnswer {
                        latex: format!("\\text{{quotient }}{},\\ \\text{{remainder }}{}", latex::latex(&q), latex::latex(&r)),
                        text,
                        is_exact: true,
                    }),
                    symbolic: value.has_vars(),
                    defines: None,
                    methods: Vec::new(),
                });
            }
            if let ast::Expr::Optimize(goal, args) = &expr {
                let opt = b.optimize(*goal, args)?;
                return optimum_answer(&opt, &mut b, settings);
            }
            if let ast::Expr::Geometry(f, args) = &expr {
                let g = b.geometry(*f, args)?;
                return geometry_answer(&g, &mut b, settings);
            }
            if let ast::Expr::StatCall(f @ (ast::StatFn::LinReg | ast::StatFn::Summary), _, args) = &expr {
                return stat_answer(*f, args, &mut b, settings);
            }
            if let ast::Expr::MatCall(f @ (ast::MatFn::Eigenvalues | ast::MatFn::Eigenvectors), args) = &expr {
                return eigen_answer(*f, args, &mut b, settings);
            }
            let s = sym::normalize_complex(b.build(&expr)?);
            // (x² − 1)/(x − 1) answers x + 1: common factors cancel when there are any.
            let s = if s.has_vars() && !matches!(s, sym::Sym::Matrix(_)) { b.tidy_fractions(s)? } else { s };
            (s, None)
        }
        ast::Stmt::Define { name, params, body } => {
            let shown_name = ast::name_latex(&name);
            (env.define(name, params, body, ev)?, Some(shown_name))
        }
        ast::Stmt::Equations(eqs, conds) => {
            let mut b = sym::Builder { ev: &mut *ev, env };
            // y' = 2y, y(0) = 1: a differential equation.
            if eqs.iter().any(|(l, r)| ode::mentions_ode(l) || ode::mentions_ode(r)) {
                if !conds.is_empty() {
                    return Err(EngineError::new("Conditions like x > 0 don't apply to differential equations yet"));
                }
                let sol = b.solve_ode(&eqs)?;
                let latex = sol.latex();
                let symbolic = !matches!(sol.result, ode::OdeResult::Numeric { .. });
                return Ok(Answer {
                    exact_latex: Some(latex.clone()),
                    decimal: (!symbolic).then(|| DecimalAnswer { latex: latex.clone(), text: latex, is_exact: false }),
                    symbolic,
                    defines: None,
                    methods: Vec::new(),
                });
            }
            let mut sides = Vec::new();
            for (l, r) in &eqs {
                let l = sym::normalize_complex(b.build(l)?);
                sides.push((l, sym::normalize_complex(b.build(r)?)));
            }
            let conds = build_chains(&mut b, &conds)?;
            let sol = solve::solve(&mut b, sides, conds)?;
            return solution_answer(sol, settings, ev);
        }
        ast::Stmt::Inequalities(chains) => {
            let mut b = sym::Builder { ev: &mut *ev, env };
            let chains = build_chains(&mut b, &chains)?;
            let sol = solve::inequalities(&mut b, chains)?;
            return solution_answer(sol, settings, ev);
        }
    };
    if let sym::Sym::Matrix(m) = &s {
        return matrix_answer(m, settings, ev, defines);
    }
    // A limit (or a divergent sum) can be ±∞, but only on its own.
    if limits::contains_inf(&s) {
        let sign = match limits::point_of(&s)? {
            limits::Point::PosInf => "",
            limits::Point::NegInf => "-",
            limits::Point::At(_) => unreachable!("contains ∞"),
        };
        let latex = format!("{sign}\\infty");
        let text = format!("{}∞", if sign.is_empty() { "" } else { "−" });
        return Ok(Answer {
            exact_latex: Some(latex.clone()),
            decimal: Some(DecimalAnswer { latex, text, is_exact: true }),
            symbolic: false,
            defines,
            methods: Vec::new(),
        });
    }
    if !units::contains_unit(&s) {
        let mut answer = value_answer(&s, settings, ev, defines)?;
        // An indefinite integral is a family of functions: show the constant.
        if indefinite {
            answer.exact_latex = answer.exact_latex.map(|e| format!("{e}+C"));
        }
        // A Taylor polynomial is the start of a series.
        if series {
            answer.exact_latex = answer.exact_latex.map(|e| format!("{e}+\\cdots"));
        }
        return Ok(answer);
    }
    // Units come off first; the value is shown like any other, with the units after it.
    let q = units::resolve(&s)?;
    let mut answer = value_answer(&q.value, settings, ev, defines)?;
    if q.units.is_empty() {
        return Ok(answer);
    }
    let (unit_latex, unit_text) = units::display(&q.units);
    let sum = matches!(q.value, sym::Sym::Add(_));
    let with_unit = |x: String| {
        if sum { format!("\\left({x}\\right)\\,{unit_latex}") } else { format!("{x}\\,{unit_latex}") }
    };
    answer.exact_latex = answer.exact_latex.map(with_unit);
    if let Some(d) = &mut answer.decimal {
        d.latex = with_unit(std::mem::take(&mut d.latex));
        d.text = if sum { format!("({}) {unit_text}", d.text) } else { format!("{} {unit_text}", d.text) };
    }
    Ok(answer)
}

/// Lines whose answer isn't a value: `factor(360)` = 2³·3²·5, `isprime(91)`, `divisors(12)`.
fn number_answer(expr: &ast::Expr, env: &defs::Env, ev: &mut eval::Evaluator) -> Result<Option<Answer>, EngineError> {
    use ast::{Expr, NumFn, SymOp};
    let (f, arg) = match expr {
        Expr::Op(SymOp::Factor, x) => (None, &**x),
        Expr::NumCall(f @ (NumFn::IsPrime | NumFn::Divisors), args) if args.len() == 1 => (Some(*f), &args[0]),
        Expr::NumCall(f @ (NumFn::IsPrime | NumFn::Divisors), _) => {
            return Err(EngineError::new(format!("{} takes one number", f.name())));
        }
        _ => return Ok(None),
    };
    let s = sym::Builder { ev: &mut *ev, env }.build(arg)?;
    let n = match (&s, f) {
        (sym::Sym::Num(r), _) if r.is_integer() => r.to_integer(),
        // factor of anything else is algebra.
        (_, None) => {
            ev.work.clear();
            return Ok(None);
        }
        (_, Some(f)) => return numtheory::integer(f, &s).map(|_| None),
    };
    let shown = |latex: String, text: String, methods: Vec<Method>| Answer {
        exact_latex: Some(latex.clone()),
        decimal: Some(DecimalAnswer { latex, text, is_exact: true }),
        symbolic: false,
        defines: None,
        methods,
    };
    let int_text = |i: &num_bigint::BigInt| i.to_string();
    // `2^{3}\cdot 1\,000\,003` → `2^3·1000003`
    let plain = |latex: &str| latex.replace("\\cdot ", "·").replace("\\,", "").replace(['{', '}'], "");
    Ok(Some(match f {
        None => {
            let (latex, steps) = numtheory::factor_with_steps(&n)?;
            let text = plain(&latex);
            shown(latex, text, vec![Method { name: "Prime factorisation".into(), steps }])
        }
        Some(NumFn::IsPrime) => {
            if numtheory::is_prime(&n) {
                shown("\\text{prime}".into(), "prime".into(), Vec::new())
            } else if n < num_bigint::BigInt::from(2) {
                shown("\\text{not prime}".into(), "not prime".into(), vec![Method {
                    name: "Definition".into(),
                    steps: vec![Step { latex: format!("{n}<2"), note: "Primes are whole numbers above 1 with no divisors but 1 and themselves".into() }],
                }])
            } else {
                let (latex, steps) = numtheory::factor_with_steps(&n)?;
                let text = plain(&latex);
                shown(
                    format!("\\text{{not prime: }}{n}={latex}"),
                    format!("not prime: {n} = {text}"),
                    vec![Method { name: "Prime factorisation".into(), steps }],
                )
            }
        }
        Some(_) => {
            let ds = numtheory::divisors(&n)?;
            let latex = ds.iter().map(int_text).collect::<Vec<_>>().join(",\\ ");
            let text = ds.iter().map(int_text).collect::<Vec<_>>().join(", ");
            let mut steps = Vec::new();
            if n.magnitude() > &num_bigint::BigUint::from(1u8) {
                let fs = numtheory::factorize(&n)?;
                let count = fs.iter().map(|(_, k)| (k + 1).to_string()).collect::<Vec<_>>().join("\\cdot ");
                steps.push(Step { latex: format!("{}={}", n.magnitude(), numtheory::factors_latex(&fs)), note: "Factor into primes".into() });
                steps.push(Step {
                    latex: format!("{count}={}", ds.len()),
                    note: "Each divisor takes every prime to a power from 0 up to its power here: one more choice than the power, for each prime".into(),
                });
            }
            shown(latex, text, if steps.is_empty() { Vec::new() } else { vec![Method { name: "From the primes".into(), steps }] })
        }
    }))
}

/// A matrix: exact entries, and a matrix of decimals when some entry isn't a plain fraction.
fn matrix_answer(m: &[Vec<sym::Sym>], settings: &Settings, ev: &mut eval::Evaluator, defines: Option<String>) -> Result<Answer, EngineError> {
    let exact = matrix::matrix_latex(&m.to_vec());
    let symbolic = m.iter().flatten().any(|e| e.has_vars());
    let decimal = if symbolic {
        None
    } else {
        let mut rows_latex = Vec::new();
        let mut rows_text = Vec::new();
        for row in m {
            let (mut l, mut t) = (Vec::new(), Vec::new());
            for e in row {
                let d = value_answer(e, settings, ev, None)?.decimal.ok_or_else(|| EngineError::new("A matrix entry has no value"))?;
                l.push(d.latex);
                t.push(d.text);
            }
            rows_latex.push(l.join("&"));
            rows_text.push(format!("[{}]", t.join(", ")));
        }
        let all_exact = m.iter().flatten().all(|e| e.as_num().is_some());
        Some(DecimalAnswer {
            latex: format!("\\begin{{pmatrix}}{}\\end{{pmatrix}}", rows_latex.join("\\\\")),
            text: format!("[{}]", rows_text.join(", ")),
            is_exact: all_exact,
        })
    };
    Ok(Answer { exact_latex: Some(exact), decimal, symbolic, defines, methods: Vec::new() })
}

/// `eigenvalues(A)` and `eigenvectors(A)` as lists: λ₁ = …, with a basis vector for each
/// exact eigenvalue.
fn eigen_answer(f: ast::MatFn, args: &[ast::Expr], b: &mut sym::Builder, settings: &Settings) -> Result<Answer, EngineError> {
    use solve::Root;
    let [arg] = args else { return Err(EngineError::new(format!("{} takes one matrix", f.name()))) };
    let sym::Sym::Matrix(m) = b.build(arg)? else { return Err(EngineError::new(format!("{} needs a square matrix", f.name()))) };
    let (roots, mut steps) = b.eigenvalues(&m)?;
    if roots.is_empty() {
        return Err(EngineError::new("Couldn't find the eigenvalues"));
    }
    let mut exact_parts = Vec::new();
    let mut dec_parts = Vec::new();
    let mut text_parts = Vec::new();
    for (i, r) in roots.iter().enumerate() {
        let name = format!("\\lambda_{{{}}}", i + 1);
        let (value_latex, value_dec, value_text, exact) = match r {
            Root::Exact(s) => {
                let a = value_answer(s, settings, b.ev, None)?;
                let d = a.decimal.clone();
                (a.exact_latex.unwrap_or_else(|| latex::latex(s)), d.as_ref().map(|d| d.latex.clone()).unwrap_or_default(), d.map(|d| d.text).unwrap_or_else(|| latex::latex(s)), Some(s.clone()))
            }
            Root::Approx(v) => {
                let d = decimal(v, b.ev)?;
                (d.latex.clone(), d.latex, d.text, None)
            }
        };
        if f == ast::MatFn::Eigenvectors {
            let vectors = match &exact {
                Some(l) => b.eigenvectors_for(&m, l)?,
                None => Vec::new(),
            };
            let shown = if vectors.is_empty() {
                "\\text{(numeric eigenvalue)}".to_string()
            } else {
                vectors.iter().map(matrix::matrix_latex).collect::<Vec<_>>().join(",\\ ")
            };
            steps.push(Step { latex: format!("{name}={value_latex}:\\quad {shown}"), note: "Solve $(A-\\lambda I)v=0$ by row reduction".into() });
            exact_parts.push(format!("{name}={value_latex}:\\ {shown}"));
            dec_parts.push(format!("{name}={value_dec}:\\ {shown}"));
            text_parts.push(format!("λ{} = {value_text}", i + 1));
        } else {
            exact_parts.push(format!("{name}={value_latex}"));
            dec_parts.push(format!("{name}={value_dec}"));
            text_parts.push(format!("λ{} = {value_text}", i + 1));
        }
    }
    let name = if f == ast::MatFn::Eigenvalues { "Eigenvalues" } else { "Eigenvectors" };
    Ok(Answer {
        exact_latex: Some(exact_parts.join(",\\quad ")),
        decimal: Some(DecimalAnswer { latex: dec_parts.join(",\\quad "), text: text_parts.join(", "), is_exact: false }),
        symbolic: false,
        defines: None,
        methods: vec![Method { name: name.into(), steps }],
    })
}

/// A value as (exact or decimal LaTeX, decimal LaTeX, text): decimals when it came from a
/// numeric root.
fn shown_value(s: &sym::Sym, approx: bool, settings: &Settings, ev: &mut eval::Evaluator) -> Result<(String, String, String), EngineError> {
    if s.has_vars() {
        let l = latex::latex(s);
        return Ok((l.clone(), l.clone(), l));
    }
    let a = value_answer(s, settings, ev, None)?;
    let d = a.decimal.ok_or_else(|| EngineError::new("A result has no value"))?;
    let exact = if approx { d.latex.clone() } else { a.exact_latex.unwrap_or_else(|| d.latex.clone()) };
    Ok((exact, d.latex, d.text))
}

/// `minimize(…)`: min f = v at x = p; `extrema(…)`: each critical point with its kind.
fn optimum_answer(opt: &optimize::Optimum, b: &mut sym::Builder, settings: &Settings) -> Result<Answer, EngineError> {
    use optimize::Kind;
    let (mut exact, mut dec, mut text) = (Vec::new(), Vec::new(), Vec::new());
    for e in &opt.found {
        let (ve, vd, vt) = shown_value(&e.value, e.approx, settings, b.ev)?;
        let mut pe = Vec::new();
        let mut pt = Vec::new();
        for (v, x) in &e.point {
            let (xe, _, xt) = shown_value(x, e.approx, settings, b.ev)?;
            pe.push((v.clone(), xe));
            pt.push(format!("{v} = {xt}"));
        }
        let point = match pe.as_slice() {
            [(v, x)] => format!("{v}={x}"),
            _ => format!(
                "\\left({}\\right)=\\left({}\\right)",
                pe.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>().join(","),
                pe.iter().map(|(_, x)| x.clone()).collect::<Vec<_>>().join(",")
            ),
        };
        let (label, word) = match (opt.goal, e.kind) {
            (ast::Goal::Min, _) => ("\\min", "min"),
            (ast::Goal::Max, _) => ("\\max", "max"),
            (_, Kind::Min) => ("\\text{min}", "min"),
            (_, Kind::Max) => ("\\text{max}", "max"),
            (_, Kind::Saddle) => ("\\text{saddle}", "saddle"),
            (_, Kind::Inflection) => ("\\text{inflection}", "inflection"),
            (_, Kind::End) => ("\\text{end}", "end"),
            (_, Kind::Unclassified) => ("\\text{critical}", "critical"),
        };
        let eq = if e.approx { "\\approx " } else { "=" };
        exact.push(format!("{label}\\ f{eq}{ve}\\ \\text{{at}}\\ {point}"));
        dec.push(format!("{label}\\ f={vd}\\ \\text{{at}}\\ {point}"));
        text.push(format!("{word} f = {vt} at {}", pt.join(", ")));
    }
    let suffix = opt.note.as_ref().map(|nt| format!("\\quad\\left({nt}\\right)")).unwrap_or_default();
    let join = if opt.goal == ast::Goal::All { ",\\quad " } else { "\\quad\\text{and}\\quad " };
    Ok(Answer {
        exact_latex: Some(format!("{}{suffix}", exact.join(join))),
        decimal: Some(DecimalAnswer { latex: format!("{}{suffix}", dec.join(join)), text: text.join("; "), is_exact: false }),
        symbolic: false,
        defines: None,
        methods: Vec::new(),
    })
}

/// A shape's or triangle's measurements as a list (each triangle when two fit).
fn geometry_answer(g: &geometry::Geo, b: &mut sym::Builder, settings: &Settings) -> Result<Answer, EngineError> {
    let mut exact_rows = Vec::new();
    let mut dec_rows = Vec::new();
    let mut text_rows = Vec::new();
    for sol in &g.solutions {
        let (mut e, mut d, mut t) = (Vec::new(), Vec::new(), Vec::new());
        for (k, v) in sol {
            if let sym::Sym::Matrix(m) = v {
                let l = matrix::matrix_latex(m);
                e.push(format!("{k}={l}"));
                d.push(format!("{k}={l}"));
                t.push(format!("{k} = {l}"));
                continue;
            }
            let (ve, vd, vt) = shown_value(v, false, settings, b.ev)?;
            e.push(format!("{k}={ve}"));
            d.push(format!("{k}={vd}"));
            t.push(format!("{} = {vt}", k.replace("\\text{", "").replace(['\\', '{', '}'], "")));
        }
        exact_rows.push(e.join(",\\quad "));
        dec_rows.push(d.join(",\\quad "));
        text_rows.push(t.join(", "));
    }
    let join = "\\qquad\\text{or}\\qquad ";
    Ok(Answer {
        exact_latex: Some(exact_rows.join(join)),
        decimal: Some(DecimalAnswer { latex: dec_rows.join(join), text: text_rows.join("  or  "), is_exact: false }),
        symbolic: false,
        defines: None,
        methods: Vec::new(),
    })
}

/// `linreg(xs, ys)`: y = a + bx with r and r²; `stats(L)`: the summary, as a list.
fn stat_answer(f: ast::StatFn, args: &[ast::Expr], b: &mut sym::Builder, settings: &Settings) -> Result<Answer, EngineError> {
    let args = args.iter().map(|a| b.build(a)).collect::<Result<Vec<_>, _>>()?;
    let shown = |b: &mut sym::Builder, s: &sym::Sym| -> Result<(String, String, String), EngineError> {
        let a = value_answer(s, settings, b.ev, None)?;
        let d = a.decimal.ok_or_else(|| EngineError::new("A result has no value"))?;
        Ok((a.exact_latex.unwrap_or_else(|| d.latex.clone()), d.latex, d.text))
    };
    let (pairs, name, steps): (Vec<(String, sym::Sym)>, &str, Vec<Step>) = if f == ast::StatFn::LinReg {
        let (a, slope, r, steps) = b.linreg(&args)?;
        let r2 = sym::normalize_complex(poly::expand(&sym::pow(r.clone(), sym::n(2))?)?);
        let line = sym::add(vec![a.clone(), sym::mul(vec![slope.clone(), sym::Sym::Var("x".into())])]);
        let mut pairs = vec![("y".to_string(), line), ("a".into(), a), ("b".into(), slope), ("r".into(), r)];
        pairs.push(("r^{2}".into(), r2));
        (pairs, "Linear regression", steps)
    } else {
        let pairs = b.summary(&args)?.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        (pairs, "Summary statistics", Vec::new())
    };
    let (mut exact, mut dec, mut text) = (Vec::new(), Vec::new(), Vec::new());
    for (k, v) in &pairs {
        if v.has_vars() {
            // The regression line itself.
            exact.push(format!("{k}={}", latex::latex(v)));
            dec.push(format!("{k}={}", latex::latex(v)));
            text.push(format!("{} = {}", k.replace(['\\', '{', '}'], ""), latex::latex(v)));
            continue;
        }
        let (e, d, t) = shown(b, v)?;
        exact.push(format!("{k}={e}"));
        dec.push(format!("{k}={d}"));
        text.push(format!("{} = {t}", k.replace("\\bar{x}", "mean").replace("\\sum x", "sum").replace("\\text{med}", "median").replace(['\\', '{', '}'], "")));
    }
    Ok(Answer {
        exact_latex: Some(exact.join(",\\quad ")),
        decimal: Some(DecimalAnswer { latex: dec.join(",\\quad "), text: text.join(", "), is_exact: false }),
        symbolic: false,
        defines: None,
        methods: if steps.is_empty() { Vec::new() } else { vec![Method { name: name.into(), steps }] },
    })
}

/// Most roots `roots(z, n)` lists.
const MAX_ROOTS: i64 = 100;

/// `polar(z)` = r·e^{iθ} (r∠θ° in degree mode), and `roots(z, n)`: all n-th roots of z.
fn complex_answer(
    expr: &ast::Expr,
    env: &defs::Env,
    ev: &mut eval::Evaluator,
    settings: &Settings,
) -> Result<Option<Answer>, EngineError> {
    use ast::{Expr, NumFn, SymOp};
    use sym::Sym;
    let (z, count) = match expr {
        Expr::Op(SymOp::Polar, x) => (&**x, None),
        Expr::NumCall(NumFn::Roots, args) if args.len() == 2 => (&args[0], Some(&args[1])),
        Expr::NumCall(NumFn::Roots, _) => return Err(EngineError::new("roots takes a number and how many roots: roots(8, 3)")),
        _ => return Ok(None),
    };
    let mut b = sym::Builder { ev: &mut *ev, env };
    let z = sym::normalize_complex(b.build(z)?);
    if z.has_vars() || units::contains_unit(&z) {
        return Err(EngineError::new("This needs a number, not letters or units"));
    }
    let (re, im) = sym::rect(&z).ok_or_else(|| EngineError::new("Couldn't split this into real and imaginary parts"))?;
    let r = b.func(ast::Func::Abs, z.clone())?;
    let theta = b.arg_of(re.clone(), im.clone())?;
    let degrees = b.ev.angle == AngleMode::Degrees;
    let lx = |s: &Sym| latex::latex(s);
    // r·e^{iθ}, or r∠θ° in degree mode. `t` is θ's LaTeX; `sum` when it needs brackets after i.
    let polar_latex = |r: &str, t: &str, one: bool, sum: bool| {
        let exponent = match t.strip_prefix('-') {
            _ if sum => format!("i\\left({t}\\right)"),
            Some(rest) => format!("-i{rest}"),
            None => format!("i{t}"),
        };
        match (degrees, one) {
            (true, _) => format!("{r}\\angle {t}^{{\\circ}}"),
            (false, true) => format!("e^{{{exponent}}}"),
            (false, false) => format!("{r}e^{{{exponent}}}"),
        }
    };
    let polar_steps = |r: &Sym, theta: &Sym| {
        let wrap = |s: &Sym| if matches!(s, Sym::Num(v) if *v >= num_rational::BigRational::from_integer(0.into())) { lx(s) } else { format!("\\left({}\\right)", lx(s)) };
        vec![
            Step { latex: format!("z={}", lx(&z)), note: format!("Real part ${}$, imaginary part ${}$", lx(&re), lx(&im)) },
            Step {
                latex: format!("r=\\sqrt{{{}^{{2}}+{}^{{2}}}}={}", wrap(&re), wrap(&im), lx(r)),
                note: "The distance from 0: $r=\\sqrt{a^2+b^2}$".into(),
            },
            Step {
                latex: format!("\\theta={}", lx(theta)),
                note: "The angle from the positive real axis: $\\arctan\\frac{b}{a}$, plus or minus half a turn when $a<0$".into(),
            },
        ]
    };

    let Some(count) = count else {
        let (rd, td) = (value_answer(&r, settings, b.ev, None)?, value_answer(&theta, settings, b.ev, None)?);
        let (rd, td) = (rd.decimal.expect("a number"), td.decimal.expect("a number"));
        // π − arctan(4/3) is exact, but in degrees 126.87° says more.
        let keep = !degrees || theta.is_closed_form();
        let theta_latex = match &theta {
            _ if !keep => td.latex.clone(),
            // π − arctan(4/3) reads better than −arctan(4/3) + π.
            Sym::Add(ts) => {
                let (neg, pos): (Vec<String>, Vec<String>) = ts.iter().map(lx).partition(|t| t.starts_with('-'));
                let mut out = String::new();
                for t in pos.iter().chain(&neg) {
                    if !out.is_empty() && !t.starts_with('-') {
                        out.push('+');
                    }
                    out.push_str(t);
                }
                out
            }
            _ => lx(&theta),
        };
        let sum = keep && matches!(theta, Sym::Add(_));
        let exact = if theta.is_zero() { lx(&r) } else { polar_latex(&lx(&r), &theta_latex, r.is_one(), sum) };
        let dec_latex = polar_latex(&rd.latex, &td.latex, false, false);
        let dec_text = if degrees { format!("{}∠{}°", rd.text, td.text) } else { format!("{}·e^({}i)", rd.text, td.text) };
        let mut steps = polar_steps(&r, &theta);
        steps.push(Step { latex: format!("z={exact}"), note: "Put them together: $z=re^{i\\theta}$".into() });
        return Ok(Some(Answer {
            exact_latex: Some(exact),
            decimal: Some(DecimalAnswer { latex: dec_latex, text: dec_text, is_exact: false }),
            symbolic: false,
            defines: None,
            methods: vec![Method { name: "Polar form".into(), steps }],
        }));
    };

    let k = b.build(count)?;
    let k = k.as_num().filter(|k| k.is_integer()).and_then(number::to_i64).filter(|k| (1..=MAX_ROOTS).contains(k));
    let k = k.ok_or_else(|| EngineError::new(format!("roots needs a whole number of roots from 1 to {MAX_ROOTS}")))?;
    if z.is_zero() {
        return Ok(Some(value_answer(&Sym::Num(number::int(0)), settings, b.ev, None)?));
    }
    let roots = b.nth_roots(&z, k)?;
    let mut shown = Vec::new();
    let mut texts = Vec::new();
    let mut decimals = Vec::new();
    for root in &roots {
        let a = value_answer(root, settings, b.ev, None)?;
        let d = a.decimal.expect("a number");
        shown.push(a.exact_latex.unwrap_or_else(|| d.latex.clone()));
        decimals.push(d.latex);
        texts.push(d.text);
    }
    let named = |j: usize, s: &str| format!("z_{{{j}}}={s}");
    let list = |items: &[String]| items.iter().enumerate().map(|(j, s)| named(j, s)).collect::<Vec<_>>().join(",\\quad ");
    let (exact, dec_latex) = (list(&shown), list(&decimals));
    let text = texts.iter().enumerate().map(|(j, s)| format!("z{j} = {s}")).collect::<Vec<_>>().join(", ");
    let mut steps = polar_steps(&r, &theta);
    let turn = if degrees { "360^{\\circ}" } else { "2\\pi" };
    steps.push(Step {
        latex: format!("z_{{k}}=\\sqrt[{k}]{{r}}\\left(\\cos\\frac{{\\theta+{turn} k}}{{{k}}}+i\\sin\\frac{{\\theta+{turn} k}}{{{k}}}\\right)"),
        note: format!(
            "De Moivre: take the {} root of the size and divide the angle by {k}, adding a whole turn for each next root, $k=0,\\dots,{}$",
            match (k, k % 10, k % 100) {
                (2, ..) => "square".to_string(),
                (3, ..) => "cube".to_string(),
                (_, _, 11..=13) => format!("{k}th"),
                (_, 1, _) => format!("{k}st"),
                (_, 2, _) => format!("{k}nd"),
                (_, 3, _) => format!("{k}rd"),
                _ => format!("{k}th"),
            },
            k - 1
        ),
    });
    for (j, s) in shown.iter().enumerate() {
        steps.push(Step { latex: named(j, s), note: format!("$k={j}$") });
    }
    Ok(Some(Answer {
        exact_latex: Some(exact),
        decimal: Some(DecimalAnswer { latex: dec_latex, text, is_exact: false }),
        symbolic: false,
        defines: None,
        methods: vec![Method { name: "De Moivre's theorem".into(), steps }],
    }))
}

/// The answer for a unit-free value: exact form, decimal, or a symbolic result.
fn value_answer(
    s: &sym::Sym,
    settings: &Settings,
    ev: &mut eval::Evaluator,
    defines: Option<String>,
) -> Result<Answer, EngineError> {
    if s.has_vars() {
        return Ok(Answer {
            exact_latex: Some(latex::latex(s)),
            decimal: None,
            symbolic: true,
            defines,
            methods: Vec::new(),
        });
    }
    // A plain fraction (or a + bi with fractions a, b) is exact at any precision; everything
    // else is checked across precisions.
    let dec = match (s, sym::rational_parts(s)) {
        (sym::Sym::Num(r), _) => decimal(&number::Value::Exact(r.clone()), ev)?,
        (_, Some((re, im))) => {
            let d = number::format_complex_exact(&re, &im, ev.prec.digits);
            DecimalAnswer { latex: d.latex, text: d.text, is_exact: d.exact }
        }
        _ => reliable_decimal(&s.to_expr(), settings)?,
    };
    Ok(Answer {
        exact_latex: s.is_closed_form().then(|| latex::latex(s)),
        decimal: Some(dec),
        symbolic: false,
        defines,
        methods: Vec::new(),
    })
}

fn build_chains(b: &mut sym::Builder, chains: &[ast::Chain]) -> Result<Vec<solve::SymChain>, EngineError> {
    chains
        .iter()
        .map(|c| {
            let parts = c.parts.iter().map(|p| b.build(p).map(sym::normalize_complex)).collect::<Result<_, _>>()?;
            Ok(solve::SymChain { parts, rels: c.rels.clone() })
        })
        .collect()
}

/// The answer for an equation or system: the solutions, their decimals, and the working.
fn solution_answer(sol: solve::Solution, settings: &Settings, ev: &mut eval::Evaluator) -> Result<Answer, EngineError> {
    use solve::{Outcome, Root};
    let vars: Vec<String> = sol.vars.iter().map(|v| ast::name_latex(v)).collect();
    let words = |latex: &str, text: &str| -> (Option<String>, Option<DecimalAnswer>) {
        (Some(format!("\\text{{{latex}}}")), Some(DecimalAnswer { latex: format!("\\text{{{latex}}}"), text: text.into(), is_exact: true }))
    };
    let (exact_latex, decimal) = match sol.outcome {
        Outcome::Text { latex, text, decimal } => {
            let dec = match decimal {
                Some((l, t)) => DecimalAnswer { latex: l, text: t, is_exact: false },
                None => DecimalAnswer { latex: latex.clone(), text, is_exact: true },
            };
            (Some(latex), Some(dec))
        }
        Outcome::True => words("true", "true"),
        Outcome::False => words("false", "false"),
        Outcome::NoSolution => words("no solution", "no solution"),
        Outcome::AllValues => {
            let l = format!("true for every {}", sol.vars.join(", "));
            words(&l, &l)
        }
        Outcome::Sets(sets) => {
            // Each root as (exact LaTeX if any, decimal LaTeX and text if numeric).
            let mut exact_sets = Vec::new();
            let mut dec_sets = Vec::new();
            let mut all_exact = true;
            let mut all_numeric = true;
            let mut decimals_exact = true;
            for set in &sets {
                let mut ex = Vec::new();
                let mut de = Vec::new();
                for root in set {
                    match root {
                        Root::Exact(s) => {
                            ex.push(latex::latex(s));
                            if s.has_vars() {
                                all_numeric = false;
                            } else {
                                let a = value_answer(s, settings, ev, None)?;
                                let d = a.decimal.expect("numeric roots have a decimal");
                                decimals_exact &= d.is_exact;
                                de.push((d.latex, d.text));
                            }
                        }
                        Root::Approx(v) => {
                            all_exact = false;
                            decimals_exact = false;
                            let d = decimal(v, ev)?;
                            ex.push(d.latex.clone());
                            de.push((d.latex, d.text));
                        }
                    }
                }
                exact_sets.push(ex);
                dec_sets.push(de);
            }
            let rel = if all_exact { "=" } else { "\\approx " };
            let join_sets = |sets: Vec<Vec<String>>, rel: &str, or: &str, sep: &str| -> String {
                if vars.len() == 1 {
                    sets.iter().map(|s| format!("{}{rel}{}", vars[0], s[0])).collect::<Vec<_>>().join(or)
                } else if sets.len() == 1 {
                    vars.iter().zip(&sets[0]).map(|(v, s)| format!("{v}{rel}{s}")).collect::<Vec<_>>().join(sep)
                } else {
                    let tuple = |xs: &[String]| format!("\\left({}\\right)", xs.join(",\\ "));
                    let head = tuple(&vars);
                    format!("{head}{rel}{}", sets.iter().map(|s| tuple(s)).collect::<Vec<_>>().join(or))
                }
            };
            let mut exact = join_sets(exact_sets, rel, "\\quad\\text{or}\\quad ", ",\\quad ");
            if let Some(note) = &sol.note {
                exact.push_str(&format!(",\\quad {note}"));
            }
            let decimal = if all_numeric && !sets.is_empty() {
                let d_latex: Vec<Vec<String>> = dec_sets.iter().map(|s| s.iter().map(|d| d.0.clone()).collect()).collect();
                let d_text: Vec<String> = dec_sets
                    .iter()
                    .map(|s| sol.vars.iter().zip(s).map(|(v, d)| format!("{v} = {}", d.1)).collect::<Vec<_>>().join(", "))
                    .collect();
                let rel = if decimals_exact { "=" } else { "\\approx " };
                Some(DecimalAnswer {
                    latex: join_sets(d_latex, rel, "\\quad\\text{or}\\quad ", ",\\quad "),
                    text: d_text.join(" or "),
                    is_exact: decimals_exact,
                })
            } else {
                None
            };
            (Some(exact), decimal)
        }
    };
    Ok(Answer { exact_latex, decimal, symbolic: false, defines: None, methods: sol.methods })
}

/// Purely numeric evaluation of the input, skipping the symbolic layer.
/// Used to cross-check the symbolic path.
#[cfg(test)]
fn evaluate_numeric(input: &str, settings: &Settings) -> Result<DecimalAnswer, EngineError> {
    reliable_decimal(&parser::parse(input)?, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(s: &str) -> Answer {
        evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    fn deg(s: &str) -> Answer {
        evaluate(s, &Settings { angle: AngleMode::Degrees, ..Settings::default() }).unwrap()
    }

    fn dec(s: &str) -> String {
        calc(s).decimal.unwrap().text
    }

    fn exact(s: &str) -> String {
        calc(s).exact_latex.unwrap_or_else(|| panic!("{s} should be exact"))
    }

    #[test]
    fn floating_point_traps_are_exact() {
        assert_eq!(exact("0.1+0.2"), "\\frac{3}{10}");
        assert_eq!(dec("0.1+0.2"), "0.3");
        assert!(calc("0.1+0.2").decimal.unwrap().is_exact);
        assert_eq!(exact("\\frac{1}{3}+\\frac{1}{6}"), "\\frac{1}{2}");
        assert_eq!(dec("1-0.9"), "0.1");
    }

    #[test]
    fn rounding_of_repeating_decimals() {
        assert_eq!(dec("\\frac{2}{3}"), "0.66666666666666666666666666666667");
        assert!(!calc("\\frac{2}{3}").decimal.unwrap().is_exact);
    }

    #[test]
    fn constants_to_32_digits() {
        assert_eq!(dec("\\pi"), "3.1415926535897932384626433832795");
        assert_eq!(dec("\\exponentialE"), "2.7182818284590452353602874713527");
        assert_eq!(dec("\\sqrt{2}"), "1.4142135623730950488016887242097");
    }

    #[test]
    fn exact_roots_and_powers() {
        assert_eq!(exact("\\sqrt{144}"), "12");
        assert_eq!(exact("\\sqrt[3]{-27}"), "-3");
        assert_eq!(exact("8^{\\frac{2}{3}}"), "4");
        assert_eq!(exact("2^{-3}"), "\\frac{1}{8}");
        assert_eq!(exact("\\sqrt{\\frac{9}{4}}"), "\\frac{3}{2}");
        assert_eq!(exact("2^{100}"), "1\\,267\\,650\\,600\\,228\\,229\\,401\\,496\\,703\\,205\\,376");
        assert_eq!(dec("(-8)^{\\frac{1}{3}}"), "-2");
    }

    #[test]
    fn double_factorial() {
        assert_eq!(exact("10!!"), "3\\,840");
        assert_eq!(exact("9!!"), "945");
        assert_eq!(exact("0!!"), "1");
        assert_eq!(exact("(-1)!!"), "1");
        assert_eq!(exact("(5!)!"), exact("120!"));
    }

    /// Reference values from `bc -l` at scale=300.
    #[test]
    fn huge_angles_stay_accurate() {
        assert_eq!(dec("\\sin(10^{50})"), "-0.78967249342931008271028953991741");
        assert_eq!(dec("\\sin(10^{50}+\\pi)"), "0.78967249342931008271028953991741");
        let s = Settings { digits: 100, ..Default::default() };
        assert_eq!(
            evaluate("\\cos(10^{100})", &s).unwrap().decimal.unwrap().text,
            "-0.9280819050746553434561946437769559281831820764390503933251142095425212220307967012613811770051118718"
        );
    }

    #[test]
    fn factorial_is_exact() {
        assert_eq!(exact("5!"), "120");
        // The exact field carries every digit; the decimal line switches to scientific.
        assert_eq!(exact("25!"), "15\\,511\\,210\\,043\\,330\\,985\\,984\\,000\\,000");
        assert_eq!(dec("25!"), "1.5511210043330985984e25");
        assert!(calc("25!").decimal.unwrap().is_exact);
    }

    #[test]
    fn trig_radians() {
        assert_eq!(dec("\\sin\\left(\\pi\\right)"), "0");
        assert_eq!(dec("\\cos(\\pi)"), "-1");
        assert_eq!(dec("\\sin\\left(\\frac{\\pi}{6}\\right)"), "0.5");
        assert_eq!(dec("\\sin(1)"), "0.8414709848078965066525023216303");
    }

    #[test]
    fn trig_degrees() {
        assert_eq!(deg("\\sin(30)").exact_latex.as_deref(), Some("\\frac{1}{2}"));
        assert_eq!(deg("\\cos(180)").exact_latex.as_deref(), Some("-1"));
        assert_eq!(deg("\\tan(45)").exact_latex.as_deref(), Some("1"));
        assert_eq!(deg("\\sin^{-1}(0.5)").exact_latex.as_deref(), Some("30"));
        assert_eq!(deg("\\sin(1)").decimal.unwrap().text, "0.017452406437283512819418978516316");
        // Explicit degree sign works in radian mode too.
        assert_eq!(exact("\\sin(30^{\\circ})"), "\\frac{1}{2}");
        assert!(evaluate("\\tan(90)", &Settings { angle: AngleMode::Degrees, ..Default::default() }).is_err());
    }

    #[test]
    fn logs() {
        assert_eq!(exact("\\log(1000)"), "3");
        assert_eq!(exact("\\log_{2}(8)"), "3");
        assert_eq!(exact("\\log_{2}\\left(\\frac{1}{8}\\right)"), "-3");
        assert_eq!(exact("\\log_{8}(2)"), "\\frac{1}{3}");
        assert_eq!(dec("\\ln(2)"), "0.69314718055994530941723212145818");
        assert_eq!(dec("\\ln(\\exponentialE^{2})"), "2");
    }

    #[test]
    fn precedence() {
        assert_eq!(exact("-2^{2}"), "-4");
        assert_eq!(exact("2+3\\cdot4"), "14");
        assert_eq!(exact("2(3+4)"), "14");
        assert_eq!(exact("12\\div4\\times3"), "9");
        assert_eq!(exact("50\\%"), "\\frac{1}{2}");
        assert_eq!(exact("\\left|-7\\right|"), "7");
    }

    #[test]
    fn high_precision() {
        let s = Settings { digits: 100, ..Default::default() };
        let a = evaluate("\\pi", &s).unwrap();
        assert_eq!(
            a.decimal.unwrap().text,
            "3.141592653589793238462643383279502884197169399375105820974944592307816406286208998628034825342117068"
        );
    }

    #[test]
    fn scientific_notation() {
        assert_eq!(dec("10^{40}"), "1e40");
        assert_eq!(dec("10^{20}"), "100000000000000000000");
        assert_eq!(calc("10^{40}").decimal.unwrap().latex, "1\\times10^{40}");
        assert_eq!(dec("\\frac{1}{10^{9}}"), "1e-9");
        assert_eq!(dec("6.02214076\\times10^{23}"), "6.02214076e23");
    }

    #[test]
    fn errors() {
        let s = Settings::default();
        assert!(evaluate("1/0", &s).is_err());
        assert!(evaluate("\\frac{1}{1+i-i-1}", &s).is_err());
        assert!(evaluate("\\ln(0)", &s).is_err());
        assert!(evaluate("2+", &s).is_err());
        assert!(evaluate("", &s).is_err());
        assert!(evaluate("0^{-1}", &s).is_err());
    }
}
