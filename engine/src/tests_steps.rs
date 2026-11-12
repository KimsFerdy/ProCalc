//! Tests for the worked steps of derivatives, integrals and substitution.
//! Test names are referenced from FEATURES.md.

use crate::{Answer, Settings, evaluate, evaluate_with};

fn calc(s: &str) -> Answer {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"))
}

/// All step notes and maths of the first method, joined, for searching.
fn working(s: &str) -> (String, String) {
    let a = calc(s);
    let m = a.methods.first().unwrap_or_else(|| panic!("{s} has no working"));
    let notes = m.steps.iter().map(|st| st.note.clone()).collect::<Vec<_>>().join(" | ");
    let maths = m.steps.iter().map(|st| st.latex.clone()).collect::<Vec<_>>().join(" | ");
    (notes, maths)
}

#[test]
fn derivative_steps_name_each_rule() {
    let (notes, _) = working("\\frac{d}{dx}x\\sin(x)");
    assert!(notes.contains("Product rule") && notes.contains("Derivative of sin"), "{notes}");
    let (notes, _) = working("\\frac{d}{dx}\\sin(x^{2})");
    assert!(notes.contains("Chain rule") && notes.contains("Power rule"), "{notes}");
    let (notes, _) = working("\\frac{d}{dx}(3x^{2}+5x-2)");
    assert!(notes.contains("Sum rule") && notes.contains("Constant multiple rule"), "{notes}");
    let (notes, _) = working("\\frac{d}{dx}e^{3x}");
    assert!(notes.contains("Exponential rule"), "{notes}");
    assert_eq!(calc("\\frac{d}{dx}\\frac{x^{2}}{x+1}").methods[0].name, "Derivative");
}

#[test]
fn quotient_rule_steps_and_tidy_answer() {
    let (notes, _) = working("\\frac{d}{dx}\\frac{x^{2}}{x+1}");
    assert!(notes.contains("Quotient rule"), "{notes}");
    assert_eq!(calc("\\frac{d}{dx}\\frac{x^{2}}{x+1}").exact_latex.as_deref(), Some("\\frac{x^{2}+2x}{\\left(x+1\\right)^{2}}"));
}

#[test]
fn higher_derivatives_and_points_show_each_round() {
    let (notes, _) = working("\\frac{d^{2}}{dx^{2}}x^{3}");
    assert!(notes.contains("First derivative") && notes.contains("Second derivative"), "{notes}");
    let defs = vec!["x:=2".to_string()];
    let a = evaluate_with("\\frac{d}{dx}x^{3}", &defs, &Settings::default()).unwrap();
    let last = a.methods[0].steps.last().unwrap();
    assert_eq!(last.latex, "3x^{2}=12");
    let defs = vec!["f(t):=t^{3}".to_string()];
    let a = evaluate_with("f'(2)", &defs, &Settings::default()).unwrap();
    assert!(a.methods[0].steps.iter().any(|s| s.latex.starts_with("f'\\left(2\\right)=12")), "{:?}", a.methods[0].steps);
}

#[test]
fn integral_steps_show_the_technique() {
    let (notes, maths) = working("\\int xe^{x}\\,dx");
    assert!(notes.contains("Integration by parts"), "{notes}");
    assert!(maths.contains("u=x") && maths.ends_with("+C"), "{maths}");
    let (notes, maths) = working("\\int 2x\\cos(x^{2})\\,dx");
    assert!(notes.contains("Substitute") && notes.contains("Put back"), "{notes}");
    assert!(maths.contains("u=x^{2}"), "{maths}");
    let (notes, _) = working("\\int(3x^{2}+1)\\,dx");
    assert!(notes.contains("term by term") && notes.contains("Power rule"), "{notes}");
    let (notes, _) = working("\\int x(x+1)\\,dx");
    assert!(notes.contains("Multiply out"), "{notes}");
    let (notes, _) = working("\\int\\frac{1}{1+x^{2}}\\,dx");
    assert!(notes.contains("arctan"), "{notes}");
}

#[test]
fn definite_and_numeric_integral_steps() {
    let (_, maths) = working("\\int_{0}^{\\pi}\\sin(x)\\,dx");
    assert!(maths.contains("\\left[-\\cos\\left(x\\right)\\right]_{0}^{\\pi}=\\left(1\\right)-\\left(-1\\right)=2"), "{maths}");
    let a = calc("\\int_{0}^{1}e^{-x^{2}}\\,dx");
    assert_eq!(a.methods[0].name, "Numeric integral");
}

#[test]
fn substitution_and_mixed_lines() {
    let (notes, maths) = working("(2x+1)|_{x=3}");
    assert!(notes.contains("Replace $x$ with $3$"), "{notes}");
    assert!(maths.ends_with("7"), "{maths}");
    // A line with two calculus pieces gets a method for each.
    let a = calc("\\frac{d}{dx}x^{2}+\\int x\\,dx");
    let names: Vec<_> = a.methods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Derivative", "Integral"]);
    // Plain arithmetic has no working to show.
    assert!(calc("2+2").methods.is_empty());
}
