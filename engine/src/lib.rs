//! ProCalc's maths engine.
//!
//! Input is LaTeX as produced by MathLive (plain text like `sqrt(2)*pi` works too).
//! Results stay exact (fractions) whenever possible and fall back to
//! arbitrary-precision floating point with a user-chosen number of digits.

pub(crate) mod ast;
mod eval;
mod latex;
mod number;
mod parser;
mod poly;
mod sym;
#[cfg(test)]
mod tests_symbolic;

use serde::{Deserialize, Serialize};

pub use parser::parse;

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
}

pub const MIN_DIGITS: usize = 1;
pub const MAX_DIGITS: usize = 1000;

fn evaluator(settings: &Settings) -> eval::Evaluator {
    let digits = settings.digits.clamp(MIN_DIGITS, MAX_DIGITS);
    eval::Evaluator { prec: number::Prec::new(digits), angle: settings.angle }
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
        let mut ev = eval::Evaluator { prec: number::Prec::with_bits(digits, bits), angle: settings.angle };
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
    let expr = parser::parse(input)?;
    let mut ev = evaluator(settings);
    let s = sym::Builder { ev: &mut ev }.build(&expr)?;
    if s.has_vars() {
        return Ok(Answer { exact_latex: Some(latex::latex(&s)), decimal: None, symbolic: true });
    }
    // A plain fraction is exact at any precision; everything else is checked across precisions.
    let dec = match &s {
        sym::Sym::Num(r) => decimal(&number::Value::Exact(r.clone()), &mut ev)?,
        _ => reliable_decimal(&s.to_expr(), settings)?,
    };
    Ok(Answer {
        exact_latex: s.is_closed_form().then(|| latex::latex(&s)),
        decimal: Some(dec),
        symbolic: false,
    })
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
        assert!(evaluate("\\sqrt{-1}", &s).is_err());
        assert!(evaluate("\\ln(0)", &s).is_err());
        assert!(evaluate("2+", &s).is_err());
        assert!(evaluate("", &s).is_err());
        assert!(evaluate("(-8)^{0.5}", &s).is_err());
    }
}
