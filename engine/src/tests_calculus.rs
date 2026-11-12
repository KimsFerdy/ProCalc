//! Tests for derivatives, integrals and substitution.
//! Test names are referenced from FEATURES.md.

use crate::{AngleMode, Settings, evaluate, evaluate_with};

fn exact(s: &str) -> String {
    let a = evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"));
    a.exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn dec(s: &str) -> String {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}")).decimal.unwrap().text
}

fn fails(s: &str) -> bool {
    evaluate(s, &Settings::default()).is_err()
}

#[test]
fn derivative_rules() {
    assert_eq!(exact("\\frac{d}{dx}x^{2}"), "2x");
    assert_eq!(exact("\\frac{d}{dx}(x^{3}+2x)"), "3x^{2}+2");
    assert_eq!(exact("\\frac{d}{dx}x\\sin(x)"), "x\\cos\\left(x\\right)+\\sin\\left(x\\right)");
    assert_eq!(exact("\\frac{d}{dx}e^{2x}"), "2e^{2x}");
    assert_eq!(exact("\\frac{d}{dx}\\ln(x)"), "\\frac{1}{x}");
    assert_eq!(exact("\\frac{d}{dx}\\sqrt{x}"), "\\frac{1}{2\\sqrt{x}}");
    assert_eq!(exact("\\frac{d}{dx}x^{x}"), "x^{x}\\left(\\ln\\left(x\\right)+1\\right)");
    assert_eq!(exact("\\frac{d}{dx}\\arctan(x)"), "\\frac{1}{x^{2}+1}");
    assert_eq!(exact("\\frac{d}{dt}5t^{3}"), "15t^{2}");
    // MathLive writes the d upright: \mathrm{d}.
    assert_eq!(exact("\\frac{\\mathrm{d}}{\\mathrm{d}x}x^{2}"), "2x");
    // d/dx covers the rest of the product, not what comes after a +.
    assert_eq!(exact("\\frac{d}{dx}x^{2}+1"), "2x+1");
    // Other letters are constants.
    assert_eq!(exact("\\frac{d}{dx}ax^{2}"), "2ax");
}

#[test]
fn higher_derivatives_and_primes() {
    assert_eq!(exact("\\frac{d^{2}}{dx^{2}}x^{4}"), "12x^{2}");
    assert_eq!(exact("\\frac{d^{3}}{dx^{3}}\\sin(x)"), "-\\cos\\left(x\\right)");
    let defs = vec!["f(x):=x^{3}".to_string()];
    let at = |s: &str| evaluate_with(s, &defs, &Settings::default()).unwrap().exact_latex.unwrap();
    assert_eq!(at("f'(2)"), "12");
    assert_eq!(at("f''(x)"), "6x");
    assert_eq!(at("f^{\\prime}(1)"), "3");
}

#[test]
fn derivatives_at_defined_points() {
    // With x defined, d/dx is the slope there.
    let defs = vec!["x:=3".to_string()];
    let a = evaluate_with("\\frac{d}{dx}x^{2}", &defs, &Settings::default()).unwrap();
    assert_eq!(a.exact_latex.as_deref(), Some("6"));
    // Degree mode: d/dx sin x° = (π/180) cos x°.
    let deg = Settings { angle: AngleMode::Degrees, ..Settings::default() };
    let a = evaluate("\\frac{d}{dx}\\sin(x)", &deg).unwrap();
    assert_eq!(a.exact_latex.as_deref(), Some("\\frac{\\pi\\cos\\left(x\\right)}{180}"));
}

#[test]
fn indefinite_integrals() {
    assert_eq!(exact("\\int x\\,dx"), "\\frac{x^{2}}{2}+C");
    assert_eq!(exact("\\int\\sin(x)\\,dx"), "-\\cos\\left(x\\right)+C");
    assert_eq!(exact("\\int e^{x}\\,dx"), "e^{x}+C");
    assert_eq!(exact("\\int\\frac{1}{x}\\,dx"), "\\ln\\left(\\left|x\\right|\\right)+C");
    assert_eq!(exact("\\int 3x^{2}+1\\,dx"), "x^{3}+x+C");
    assert_eq!(exact("\\int dx"), "x+C");
    assert_eq!(exact("\\int\\frac{1}{1+x^{2}}\\,dx"), "\\arctan\\left(x\\right)+C");
    assert_eq!(exact("\\int\\frac{1}{\\sqrt{1-x^{2}}}\\,dx"), "\\arcsin\\left(x\\right)+C");
    assert_eq!(exact("\\int\\cos(3x)\\,dx"), "\\frac{\\sin\\left(3x\\right)}{3}+C");
    assert_eq!(exact("\\int\\mathrm{e}^{2x}\\,\\mathrm{d}x"), "\\frac{e^{2x}}{2}+C");
}

#[test]
fn integration_techniques() {
    // u-substitution
    assert_eq!(exact("\\int 2x\\cos(x^{2})\\,dx"), "\\sin\\left(x^{2}\\right)+C");
    assert_eq!(exact("\\int\\sin(x)\\cos(x)\\,dx"), "\\frac{\\sin^{2}\\left(x\\right)}{2}+C");
    assert_eq!(exact("\\int xe^{x^{2}}\\,dx"), "\\frac{e^{x^{2}}}{2}+C");
    // by parts
    assert_eq!(exact("\\int xe^{x}\\,dx"), "xe^{x}-e^{x}+C");
    assert_eq!(exact("\\int x\\cos(x)\\,dx"), "x\\sin\\left(x\\right)+\\cos\\left(x\\right)+C");
    assert_eq!(exact("\\int\\ln(x)\\,dx"), "x\\ln\\left(x\\right)-x+C");
    assert_eq!(exact("\\int x^{2}\\ln(x)\\,dx"), "\\frac{x^{3}\\ln\\left(x\\right)}{3}-\\frac{x^{3}}{9}+C");
    // expanding products
    assert_eq!(exact("\\int x(x+1)\\,dx"), "\\frac{x^{3}}{3}+\\frac{x^{2}}{2}+C");
    // partial fractions
    assert!(exact("\\int\\frac{1}{x^{2}-4}\\,dx").contains("\\ln"));
    // No elementary antiderivative: an honest error.
    assert!(fails("\\int e^{x^{2}}\\,dx"));
}

#[test]
fn other_letters_and_double_integrals() {
    // Letters other than the variable are constants (this used to be rejected by the check).
    assert_eq!(exact("\\int ax\\,dx"), "\\frac{ax^{2}}{2}+C");
    assert_eq!(exact("\\int y\\,dx"), "xy+C");
    assert_eq!(exact("\\int_{0}^{1}xy\\,dx"), "\\frac{y}{2}");
    assert_eq!(exact("\\int\\int xy\\,dx\\,dy"), "\\frac{x^{2}y^{2}}{4}+C");
    assert_eq!(exact("\\int_{0}^{1}\\int_{0}^{2}xy\\,dx\\,dy"), "1");
}

#[test]
fn definite_integrals() {
    assert_eq!(exact("\\int_{0}^{1}x^{2}\\,dx"), "\\frac{1}{3}");
    assert_eq!(exact("\\int_{0}^{\\pi}\\sin(x)\\,dx"), "2");
    assert_eq!(exact("\\int_{1}^{e}\\frac{1}{x}\\,dx"), "1");
    assert_eq!(exact("\\int_{0}^{1}x^{1.5}\\,dx"), "\\frac{2}{5}");
    assert_eq!(exact("\\int_{0}^{a}x\\,dx"), "\\frac{a^{2}}{2}");
    // The integrand blowing up inside the interval is caught (not "−2").
    assert!(fails("\\int_{-1}^{1}\\frac{1}{x^{2}}\\,dx"));
}

#[test]
fn numeric_integrals() {
    // No closed form, so tanh-sinh quadrature to every digit shown.
    assert_eq!(dec("\\int_{0}^{1}e^{-x^{2}}\\,dx"), "0.74682413281242702539946743613185");
    // A quarter circle of radius 2 (also exercises a float pow that is exactly 2).
    assert_eq!(dec("\\int_{0}^{2}\\sqrt{4-x^{2}}\\,dx"), "3.1415926535897932384626433832795");
    let st = Settings { digits: 16, ..Settings::default() };
    let a = evaluate("\\int_{0}^{1}\\sqrt{x}\\sin(x)\\,dx", &st).unwrap();
    assert_eq!(a.decimal.unwrap().text, "0.3642219320321324");
    assert!(fails("\\int_{0}^{1}e^{-x^{2}y}\\,dx"));
}

#[test]
fn substitution_bars() {
    assert_eq!(exact("(2x+1)|_{x=3}"), "7");
    assert_eq!(exact("\\left.x^{2}\\right|_{x=5}"), "25");
    assert_eq!(exact("(x+y)|_{x=1,y=2}"), "3");
    assert_eq!(exact("\\sin(x)|_{x=\\frac{\\pi}{6}}"), "\\frac{1}{2}");
    assert_eq!(exact("\\left.x^{3}\\right|_{0}^{2}"), "8");
    assert_eq!(exact("\\left.\\frac{d}{dx}x^{3}\\right|_{x=2}"), "12");
    // Substitution wins over a definition of the same letter.
    let defs = vec!["x:=10".to_string()];
    let a = evaluate_with("x^{2}|_{x=3}", &defs, &Settings::default()).unwrap();
    assert_eq!(a.exact_latex.as_deref(), Some("9"));
}

#[test]
fn typed_symbols_and_mathlive_templates() {
    // The ∫ character itself, and superscript digits.
    assert_eq!(exact("∫ x dx"), "\\frac{x^{2}}{2}+C");
    assert_eq!(exact("∫x²dx"), "\\frac{x^{3}}{3}+C");
    assert_eq!(dec("∫_0^1 e^(-x^2) dx"), "0.74682413281242702539946743613185");
    // MathLive's int shortcut leaves empty bound boxes: no bounds.
    assert_eq!(exact("\\int_{\\placeholder{}}^{\\placeholder{}}x\\,dx"), "\\frac{x^{2}}{2}+C");
    assert_eq!(exact("\\int_{}^{}x\\,dx"), "\\frac{x^{2}}{2}+C");
    // MathLive's upright differential d.
    assert_eq!(exact("\\int x\\,\\differentialD x"), "\\frac{x^{2}}{2}+C");
    assert_eq!(exact("x²+1"), "x^{2}+1");
    // Typed d/dx, as in the app's tips.
    assert_eq!(exact("d/dx x sin x"), "x\\cos\\left(x\\right)+\\sin\\left(x\\right)");
    assert_eq!(exact("d/dx x^2 + 1"), "2x+1");
}

#[test]
fn calculus_errors() {
    for bad in [
        "\\frac{d}{dx}",
        "\\int x",
        "\\int_{0}x\\,dx",
        "\\frac{d}{dx}x!",
        "x=",
        "(x+y)|_{0}^{1}",
        "\\int x\\,\\mathrm{m}\\,dx",
    ] {
        assert!(fails(bad), "{bad} should fail");
    }
}
