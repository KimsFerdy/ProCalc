//! Tests for inequalities and for conditions on equations.
//! Test names are referenced from FEATURES.md.

use crate::{Settings, evaluate};

fn exact(s: &str) -> String {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}")).exact_latex.unwrap()
}

#[test]
fn one_letter_inequalities_by_sign_chart() {
    assert_eq!(exact("x^{2}<4"), "-2<x<2");
    assert_eq!(exact("x^{2}\\ge 9"), "x\\le -3\\quad\\text{or}\\quad x\\ge 3");
    assert_eq!(exact("2x+1>7"), "x>3");
    assert_eq!(exact("(x-1)(x-3)\\le 0"), "1\\le x\\le 3");
    assert_eq!(exact("x^{3}-x>0"), "-1<x<0\\quad\\text{or}\\quad x>1");
    assert_eq!(exact("x^{2}+1>0"), "\\text{every real }x");
    assert_eq!(exact("x^{2}<0"), "\\text{no solution}");
    // Typed and MathLive spellings.
    assert_eq!(exact("x\\lt 2"), "x<2");
    assert_eq!(exact("x ≥ 1"), "x\\ge 1");
}

#[test]
fn denominators_and_domains_split_the_line() {
    assert_eq!(exact("\\frac{1}{x}>2"), "0<x<\\frac{1}{2}");
    assert_eq!(exact("\\sqrt{x}<3"), "0\\le x<9");
    assert_eq!(exact("\\ln(x)>1"), "x>e");
    assert_eq!(exact("\\sqrt{x-2}\\ge 1"), "x\\ge 3");
}

#[test]
fn chains_regions_and_truth() {
    assert_eq!(exact("1<2x+1\\le 7"), "0<x\\le 3");
    assert_eq!(exact("2<3"), "\\text{true}");
    assert_eq!(exact("3<2"), "\\text{false}");
    assert!(exact("y>x^{2}").contains("region"));
    let a = evaluate("x^{2}<4", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Sign chart");
}

#[test]
fn conditions_pick_out_solutions() {
    assert_eq!(exact("\\sin(x)=\\frac{1}{2},0\\le x\\le 2\\pi"), "x=\\frac{\\pi}{6}\\quad\\text{or}\\quad x=\\frac{5\\pi}{6}");
    assert_eq!(exact("\\cos(x)=0,-\\pi\\le x\\le\\pi"), "x=-\\frac{\\pi}{2}\\quad\\text{or}\\quad x=\\frac{\\pi}{2}");
    assert_eq!(exact("x^{2}=4,x>0"), "x=2");
    assert_eq!(exact("x^{2}=4,x>5"), "\\text{no solution}");
    // Four solutions of sin 2x = 0 in [0, 3π/2]: 0, π/2, π, 3π/2.
    let four = exact("\\sin(2x)=0,0\\le x\\le\\frac{3\\pi}{2}");
    assert_eq!(four.matches("x=").count(), 4, "{four}");
    assert!(evaluate("x+y=1,x>0", &Settings::default()).is_ok());
}
