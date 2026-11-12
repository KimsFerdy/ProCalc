//! Tests for equations and systems: solutions and the worked methods.
//! Test names are referenced from FEATURES.md.

use crate::{Answer, AngleMode, Settings, evaluate};

fn calc(s: &str) -> Answer {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn exact(s: &str) -> String {
    calc(s).exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn dec(s: &str) -> String {
    calc(s).decimal.unwrap_or_else(|| panic!("{s} has no decimal")).text
}

fn method_names(s: &str) -> Vec<String> {
    calc(s).methods.into_iter().map(|m| m.name).collect()
}

#[test]
fn linear_and_quadratic_equations() {
    assert_eq!(exact("2x+3=7"), "x=2");
    assert_eq!(exact("3(x-1)=2x+4"), "x=7");
    assert_eq!(exact("\\frac{x}{2}+\\frac{x}{3}=5"), "x=6");
    assert_eq!(exact("x^{2}=2"), "x=-\\sqrt{2}\\quad\\text{or}\\quad x=\\sqrt{2}");
    assert_eq!(exact("x^{2}-5x+6=0"), "x=2\\quad\\text{or}\\quad x=3");
    assert_eq!(exact("x^{2}+2x+1=0"), "x=-1");
    // Negative discriminant: complex roots.
    assert_eq!(exact("x^{2}+1=0"), "x=-i\\quad\\text{or}\\quad x=i");
    assert_eq!(exact("x^{2}+2x+5=0"), "x=-1-2i\\quad\\text{or}\\quad x=-1+2i");
    // Letters as parameters.
    assert_eq!(exact("ax+b=0"), "x=-\\frac{b}{a}");
    assert_eq!(exact("y=2x+1"), "y=2x+1");
}

#[test]
fn quadratic_methods_to_choose_from() {
    assert_eq!(method_names("x^{2}-5x+6=0"), ["Quadratic formula", "Completing the square", "Factoring"]);
    // Irrational roots: no factoring over the rationals.
    assert_eq!(method_names("x^{2}-2x-1=0"), ["Quadratic formula", "Completing the square"]);
    let a = calc("x^{2}-5x+6=0");
    let formula = &a.methods[0].steps;
    assert!(formula.iter().any(|s| s.latex.contains("D=b^{2}-4ac=1")), "{formula:?}");
}

#[test]
fn higher_degree_and_rational_equations() {
    assert_eq!(exact("x^{3}-6x^{2}+11x-6=0"), "x=1\\quad\\text{or}\\quad x=2\\quad\\text{or}\\quad x=3");
    assert_eq!(exact("x^{3}=2"), "x=\\sqrt[3]{2}");
    assert_eq!(exact("x^{4}=16"), "x=-2\\quad\\text{or}\\quad x=2");
    assert_eq!(dec("x^{5}-x-1=0"), "x = 1.1673039782614186842560458998548");
    assert_eq!(exact("\\frac{1}{x}+1=2"), "x=1");
    // x = 1 makes a denominator zero, so it is excluded.
    assert_eq!(exact("\\frac{x}{x-1}=\\frac{1}{x-1}"), "\\text{no solution}");
}

#[test]
fn isolating_the_unknown() {
    assert_eq!(exact("e^{x}=5"), "x=\\ln\\left(5\\right)");
    assert_eq!(exact("2^{x}=8"), "x=3");
    assert_eq!(exact("\\ln(x)=2"), "x=e^{2}");
    assert_eq!(exact("\\sqrt{x}=3"), "x=9");
    assert_eq!(exact("\\sqrt{x}=-3"), "\\text{no solution}");
    assert_eq!(exact("|x-3|=2"), "x=1\\quad\\text{or}\\quad x=5");
    assert_eq!(
        exact("\\sin(x)=\\frac{1}{2}"),
        "x=2\\pi k+\\frac{\\pi}{6}\\quad\\text{or}\\quad x=2\\pi k+\\frac{5\\pi}{6},\\quad k\\in\\mathbb{Z}"
    );
    let deg = Settings { angle: AngleMode::Degrees, ..Settings::default() };
    let a = evaluate("\\cos(x)=0", &deg).unwrap();
    assert!(a.exact_latex.unwrap().contains("360k"));
    assert_eq!(exact("\\sin(x)=2"), "\\text{no solution}");
}

#[test]
fn radical_equations_drop_extraneous_roots() {
    // Squaring gives x = 0 or 3; 0 fails √1 = −1, so only 3 stays, with the check shown.
    assert_eq!(exact("\\sqrt{x+1}=x-1"), "x=3");
    assert_eq!(exact("\\sqrt{2x+3}=x"), "x=3");
    let a = calc("\\sqrt{x}=x-2");
    assert_eq!(a.exact_latex.as_deref(), Some("x=4"));
    assert!(a.methods[0].steps.iter().any(|s| s.note.contains("squaring added it")));
}

#[test]
fn numeric_roots_when_nothing_exact_applies() {
    assert_eq!(dec("\\cos(x)=x"), "x = 0.73908513321516064165531208767387");
    // Many roots: the ones nearest 0, with a note.
    let a = exact("x\\cos(x)=1");
    assert!(a.contains("nearest 0"), "{a}");
    assert_eq!(method_names("\\cos(x)=x"), ["Numeric search"]);
}

#[test]
fn identities_and_contradictions() {
    assert_eq!(exact("x+1=x+1"), "\\text{true for every x}");
    assert_eq!(exact("x+1=x+2"), "\\text{no solution}");
    assert_eq!(exact("2+2=4"), "\\text{true}");
    assert_eq!(exact("2+2=5"), "\\text{false}");
    assert_eq!(exact("0.1+0.2=0.3"), "\\text{true}");
}

#[test]
fn linear_systems() {
    assert_eq!(exact("x+y=3,x-y=1"), "x=2,\\quad y=1");
    assert_eq!(exact("x+y+z=6;2x-y+z=3;x+2y-z=2"), "x=1,\\quad y=2,\\quad z=3");
    // MathLive's cases block.
    assert_eq!(exact("\\begin{cases}2x+3y=7\\\\x-y=1\\end{cases}"), "x=2,\\quad y=1");
    assert_eq!(exact("x+y=1,x+y=2"), "\\text{no solution}");
    assert_eq!(exact("x+y=1,2x+2y=2"), "x=-y+1,\\quad y\\text{ free}");
    assert_eq!(exact("\\frac{x}{2}+y=1,x-\\frac{y}{3}=\\frac{1}{2}"), "x=\\frac{5}{7},\\quad y=\\frac{9}{14}");
    // More letters than equations: x and y are the unknowns, a is a parameter.
    let a = calc("ax+y=1,x+ay=1");
    assert!(a.symbolic || a.decimal.is_none());
    let shown = a.exact_latex.unwrap();
    assert!(shown.starts_with("x=") && shown.contains("y="), "{shown}");
}

#[test]
fn ten_unknowns() {
    // x₁ + x₂ = 3, x₂ + x₃ = 5, …, x₉ + x₁₀ = 19, x₁₀ = 10 with solution xₖ = k.
    // (Closing the cycle with x₁₀ + x₁ = 11 instead would make the matrix singular.)
    let mut eqs: Vec<String> = (1..=9).map(|k| format!("x_{{{k}}}+x_{{{}}}={}", k + 1, 2 * k + 1)).collect();
    eqs.push("x_{10}=10".into());
    let a = calc(&eqs.join(","));
    let shown = a.exact_latex.unwrap();
    for k in 1..=10 {
        assert!(shown.contains(&format!("x_{{{k}}}={k}")), "{shown}");
    }
    assert_eq!(a.methods.len(), 5);
    let eleven: Vec<String> = (1..=11).map(|k| format!("x_{{{k}}}=1")).collect();
    assert!(evaluate(&eleven.join(","), &Settings::default()).is_err());
}

#[test]
fn system_methods_to_choose_from() {
    let names = method_names("2x+y=5,x-y=1");
    assert_eq!(names, ["Substitution", "Elimination", "Gauss–Jordan", "Cramer's rule", "Inverse matrix"]);
    let a = calc("2x+y=5,x-y=1");
    let cramer = a.methods.iter().find(|m| m.name == "Cramer's rule").unwrap();
    assert!(cramer.steps.iter().any(|s| s.latex.contains("D=") && s.latex.ends_with("=-3")), "{:?}", cramer.steps);
    assert!(cramer.steps.iter().any(|s| s.latex.contains("x=\\frac{D_{x}}{D}=2")));
    let inverse = a.methods.iter().find(|m| m.name == "Inverse matrix").unwrap();
    assert!(inverse.steps.iter().any(|s| s.latex.starts_with("A^{-1}=")));
    // Cramer explains itself when it can't be used.
    let dep = calc("x+y=1,2x+2y=2");
    let cramer = dep.methods.iter().find(|m| m.name == "Cramer's rule").unwrap();
    assert!(cramer.steps.iter().any(|s| s.note.contains("D=0")));
}

#[test]
fn nonlinear_systems() {
    assert_eq!(exact("x+y=5,xy=6"), "\\left(x,\\ y\\right)=\\left(2,\\ 3\\right)\\quad\\text{or}\\quad \\left(3,\\ 2\\right)");
    assert_eq!(exact("x^{2}+y^{2}=25,x-y=1"), "\\left(x,\\ y\\right)=\\left(-3,\\ -4\\right)\\quad\\text{or}\\quad \\left(4,\\ 3\\right)");
    assert_eq!(method_names("x+y=5,xy=6"), ["Substitution"]);
}

#[test]
fn equation_errors() {
    for bad in ["x=", "=3", "x=1=2", "x+y=3,5", "x:=y=5", "x^{2}+y^{2}=1,x^{2}-y^{2}=0,xy=3,\\sin(x)\\sin(y)=1"] {
        assert!(evaluate(bad, &Settings::default()).is_err(), "{bad} should fail");
    }
}
