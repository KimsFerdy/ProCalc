//! Tests for Σ and Π, limits, integrals with infinite bounds or singular ends, and Taylor
//! series. Test names are referenced from FEATURES.md.

use crate::{Settings, evaluate};

fn exact(s: &str) -> String {
    let a = evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"));
    a.exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn dec(s: &str) -> String {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}")).decimal.unwrap().text
}

fn fails(s: &str) -> String {
    match evaluate(s, &Settings::default()) {
        Ok(a) => panic!("{s} should fail, gave {:?}", a.exact_latex),
        Err(e) => e.message,
    }
}

#[test]
fn finite_sums_in_closed_form() {
    assert_eq!(exact("\\sum_{k=1}^{100}k"), "5\\,050");
    assert_eq!(exact("\\sum_{k=1}^{n}k"), "\\frac{n\\left(n+1\\right)}{2}");
    assert_eq!(exact("\\sum_{k=1}^{n}k^2"), "\\frac{n\\left(2n+1\\right)\\left(n+1\\right)}{6}");
    assert_eq!(exact("\\sum_{k=1}^{n}(2k-1)"), "n^{2}");
    assert_eq!(exact("\\sum_{k=0}^{n}3^k"), "\\frac{3^{n+1}-1}{2}");
    // A billion terms through Faulhaber's formula, not one by one.
    assert_eq!(dec("\\sum_{k=1}^{10^{9}}k"), "500000000500000000");
    assert_eq!(exact("\\sum_{k=1}^{10}\\frac{1}{k}"), "\\frac{7\\,381}{2\\,520}");
    assert_eq!(exact("\\sum_{k=5}^{1}k"), "0");
    // The body runs to the end, like an integral's.
    assert_eq!(exact("\\sum_{k=1}^{3}k+1"), "9");
}

#[test]
fn infinite_series() {
    assert_eq!(exact("\\sum_{k=0}^{\\infty}\\frac{1}{2^k}"), "2");
    assert_eq!(exact("\\sum_{k=1}^{\\infty}\\frac{1}{k^2}"), "\\frac{\\pi^{2}}{6}");
    assert_eq!(exact("\\sum_{k=1}^{\\infty}\\frac{1}{k^4}"), "\\frac{\\pi^{4}}{90}");
    assert_eq!(exact("\\sum_{k=2}^{\\infty}\\frac{1}{k^2}"), "\\frac{\\pi^{2}}{6}-1");
    // Numerically: alternating, fast, and power-law series.
    assert_eq!(dec("\\sum_{k=1}^{\\infty}\\frac{(-1)^{k+1}}{k}"), "0.69314718055994530941723212145818");
    assert_eq!(dec("\\sum_{k=0}^{\\infty}\\frac{1}{k!}"), "2.7182818284590452353602874713527");
    assert_eq!(dec("\\sum_{k=1}^{\\infty}\\frac{1}{k^3}"), "1.2020569031595942853997381615114");
    assert_eq!(dec("\\sum_{k=1}^{\\infty}\\frac{1}{k^2+1}"), "1.07667404746858117413405079475");
    assert_eq!(dec("\\sum_{k=1}^{\\infty}\\frac{k}{(k+1)^3}"), "0.44287716368863215107267700513458");
}

#[test]
fn divergent_and_unsummable_series() {
    assert!(fails("\\sum_{k=1}^{\\infty}\\frac{1}{k}").contains("diverges"));
    assert!(fails("\\sum_{k=1}^{\\infty}\\frac{1}{\\sqrt{k}}").contains("diverges"));
    assert!(fails("\\sum_{k=0}^{\\infty}2^k").contains("diverges"));
    assert_eq!(exact("\\sum_{k=1}^{\\infty}k"), "\\infty");
    // Extrapolation that can't be trusted is refused, not guessed (ζ(1.5) = 2.612…).
    assert!(fails("\\sum_{k=1}^{\\infty}\\frac{1}{k^{1.5}}").contains("slowly"));
    assert!(fails("\\sum_{k=1}^{10^{6}}\\frac{1}{k^2}").contains("too many terms"));
}

#[test]
fn products() {
    assert_eq!(exact("\\prod_{k=1}^{5}k"), "120");
    assert_eq!(exact("\\prod_{k=1}^{n}k"), "n!");
    assert_eq!(exact("\\prod_{k=1}^{10}(1+\\frac{1}{k})"), "11");
    assert!(fails("\\prod_{k=1}^{\\infty}k").contains("Infinite products"));
}

#[test]
fn limits_by_substitution_and_lhopital() {
    assert_eq!(exact("\\lim_{x\\to2}x^2+1"), "5");
    assert_eq!(exact("\\lim_{x\\to0}\\frac{\\sin x}{x}"), "1");
    assert_eq!(exact("\\lim_{x\\to1}\\frac{x^2-1}{x-1}"), "2");
    assert_eq!(exact("\\lim_{x\\to0}\\frac{1-\\cos x}{x^2}"), "\\frac{1}{2}");
    assert_eq!(exact("\\lim_{x\\to0}\\frac{\\tan x-x}{x^3}"), "\\frac{1}{3}");
    assert_eq!(exact("\\lim_{x\\to0}\\frac{\\sin(ax)}{x}"), "a");
    // Typed text works too.
    assert_eq!(exact("lim_{x->0} sin(x)/x"), "1");
    assert_eq!(exact("sum_{k=1}^{n} k"), "\\frac{n\\left(n+1\\right)}{2}");
    assert_eq!(exact("sum_{k=1}^{infty} 1/k^2"), "\\frac{\\pi^{2}}{6}");
    let a = evaluate("\\lim_{x\\to0}\\frac{\\sin x}{x}", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Limit");
    assert!(a.methods[0].steps.iter().any(|s| s.note.contains("L'Hôpital")));
}

#[test]
fn limits_at_infinity() {
    assert_eq!(exact("\\lim_{x\\to\\infty}\\frac{2x^2+1}{x^2-3}"), "2");
    assert_eq!(exact("\\lim_{x\\to\\infty}(1+\\frac{1}{x})^{x}"), "e");
    assert_eq!(exact("\\lim_{x\\to\\infty}\\sqrt{x^2+x}-x"), "\\frac{1}{2}");
    assert_eq!(exact("\\lim_{x\\to\\infty}\\frac{x^{3}}{e^{x}}"), "0");
    assert_eq!(exact("\\lim_{x\\to\\infty}\\frac{\\sin x}{x}"), "0");
    assert_eq!(exact("\\lim_{x\\to\\infty}\\arctan x"), "\\frac{\\pi}{2}");
    assert_eq!(exact("\\lim_{x\\to-\\infty}xe^{x}"), "0");
    assert_eq!(exact("\\lim_{x\\to-\\infty}\\frac{x^3}{x^2+1}"), "-\\infty");
    assert!(fails("\\lim_{x\\to\\infty}\\sin x").contains("oscillating"));
}

#[test]
fn indeterminate_forms_and_sides() {
    assert_eq!(exact("\\lim_{x\\to0^{+}}x\\ln x"), "0");
    assert_eq!(exact("\\lim_{x\\to0}x^{x}"), "1");
    assert_eq!(exact("\\lim_{x\\to0}\\frac{1}{x}-\\frac{1}{\\sin x}"), "0");
    assert_eq!(exact("\\lim_{x\\to0}\\frac{1}{x^2}"), "\\infty");
    assert_eq!(exact("\\lim_{x\\to0^-}\\frac{1}{x}"), "-\\infty");
    assert!(fails("\\lim_{x\\to0}\\frac{1}{x}").contains("doesn't exist"));
    // ∞ is a value only on its own.
    assert!(fails("1+\\lim_{x\\to0}\\frac{1}{x^2}").contains("infinite"));
    assert!(fails("\\infty+1").contains("Infinity"));
}

#[test]
fn improper_integrals() {
    assert_eq!(exact("\\int_{1}^{\\infty}\\frac{1}{x^2}\\,dx"), "1");
    assert_eq!(exact("\\int_{0}^{\\infty}e^{-x}\\,dx"), "1");
    assert_eq!(exact("\\int_{-\\infty}^{\\infty}\\frac{1}{1+x^2}\\,dx"), "\\pi");
    assert_eq!(dec("\\int_{-\\infty}^{\\infty}e^{-x^2}\\,dx"), "1.7724538509055160272981674833411");
    assert_eq!(dec("\\int_{0}^{\\infty}\\frac{1}{(1+x)\\sqrt{x}}\\,dx"), "3.1415926535897932384626433832795");
    assert!(fails("\\int_{1}^{\\infty}\\frac{1}{x}\\,dx").contains("diverges"));
    assert!(fails("\\int_{0}^{\\infty}\\frac{\\sin x}{x}\\,dx").contains("oscillating"));
    // Singular ends of a finite range.
    assert_eq!(exact("\\int_{0}^{1}\\ln x\\,dx"), "-1");
    assert_eq!(exact("\\int_{0}^{1}\\frac{1}{\\sqrt{x}}\\,dx"), "2");
    assert_eq!(dec("\\int_{0}^{1}\\frac{1}{\\sqrt{x}+x}\\,dx"), "1.3862943611198906188344642429164");
    assert!(fails("\\int_{0}^{1}\\frac{1}{x}\\,dx").contains("diverges"));
}

#[test]
fn taylor_series() {
    assert_eq!(exact("taylor(e^{x})"), "\\frac{x^{5}}{120}+\\frac{x^{4}}{24}+\\frac{x^{3}}{6}+\\frac{x^{2}}{2}+x+1+\\cdots");
    assert_eq!(exact("taylor(\\sin x, 0, 7)"), "-\\frac{x^{7}}{5\\,040}+\\frac{x^{5}}{120}-\\frac{x^{3}}{6}+x+\\cdots");
    assert_eq!(exact("taylor(\\ln(x), 1, 3)"), "\\frac{\\left(x-1\\right)^{3}}{3}-\\frac{\\left(x-1\\right)^{2}}{2}+x-1+\\cdots");
    assert_eq!(exact("taylor(\\tan x,0,7)"), "\\frac{17x^{7}}{315}+\\frac{2x^{5}}{15}+\\frac{x^{3}}{3}+x+\\cdots");
    assert_eq!(exact("taylor(e^{t}, t, 0, 3)"), "\\frac{t^{3}}{6}+\\frac{t^{2}}{2}+t+1+\\cdots");
    // 0/0 at the point: the top's series over the bottom's.
    assert_eq!(exact("taylor(\\frac{\\sin x}{x}, 0, 6)"), "-\\frac{x^{6}}{5\\,040}+\\frac{x^{4}}{120}-\\frac{x^{2}}{6}+1+\\cdots");
    assert_eq!(exact("taylor(\\frac{x}{e^{x}-1}, 0, 6)"), "\\frac{x^{6}}{30\\,240}-\\frac{x^{4}}{720}+\\frac{x^{2}}{12}-\\frac{x}{2}+1+\\cdots");
    assert!(fails("taylor(\\frac{1}{x})").contains("blows up"));
    let a = evaluate("taylor(e^{x})", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Taylor series");
}
