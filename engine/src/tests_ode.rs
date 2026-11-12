//! Tests for differential equations: the exact methods, initial conditions, numeric
//! solutions and their graphs. Test names are referenced from FEATURES.md.

use crate::{CurveKind, Settings, View, evaluate, plot_sheet};

fn exact(s: &str) -> String {
    let a = evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"));
    a.exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn fails(s: &str) -> String {
    match evaluate(s, &Settings::default()) {
        Ok(a) => panic!("{s} should fail, gave {:?}", a.exact_latex),
        Err(e) => e.message,
    }
}

#[test]
fn first_order_linear_by_integrating_factor() {
    assert_eq!(exact("y'=2y"), "y=Ce^{2x}");
    assert_eq!(exact("y'+y=x"), "y=Ce^{-x}+x-1");
    assert_eq!(exact("y'+2y=4"), "y=Ce^{-2x}+2");
    assert_eq!(exact("xy'+y=x^2"), "y=\\frac{x^{2}}{3}+\\frac{C}{x}");
    assert_eq!(exact("\\frac{dy}{dx}=xy"), "y=Ce^{\\frac{x^{2}}{2}}");
    assert_eq!(exact("\\frac{d}{dx}y=3y"), "y=Ce^{3x}");
    // t when the equation is in t; other letters are parameters.
    assert_eq!(exact("y'=ty"), "y=Ce^{\\frac{t^{2}}{2}}");
    assert_eq!(exact("y'=ky"), "y=Ce^{kx}");
    let a = evaluate("y'+y=x", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Differential equation");
    assert!(a.methods[0].steps.iter().any(|s| s.note.contains("integrating factor")));
}

#[test]
fn separable_equations() {
    assert_eq!(exact("y'=\\frac{x}{y}"), "y=-\\sqrt{x^{2}+2C}\\quad\\text{or}\\quad y=\\sqrt{x^{2}+2C}");
    assert_eq!(exact("y'=y^2"), "y=\\frac{1}{-C-x}");
    assert_eq!(exact("y'=e^{x+y}"), "y=-\\ln\\left(-C-e^{x}\\right)");
    assert_eq!(exact("y'=\\cos x"), "y=C+\\sin\\left(x\\right)");
}

#[test]
fn constant_coefficients() {
    assert_eq!(exact("y''+y=0"), "y=C_{1}\\cos\\left(x\\right)+C_{2}\\sin\\left(x\\right)");
    assert_eq!(exact("y''-3y'+2y=0"), "y=C_{1}e^{x}+C_{2}e^{2x}");
    // A repeated root brings an extra x.
    assert_eq!(exact("y''+2y'+y=0"), "y=C_{2}xe^{-x}+C_{1}e^{-x}");
    assert_eq!(exact("y''+\\omega^2y=0"), "y=C_{1}\\cos\\left(\\omega x\\right)+C_{2}\\sin\\left(\\omega x\\right)");
    assert_eq!(exact("\\frac{d^2y}{dt^2}+9y=0"), "y=C_{1}\\cos\\left(3t\\right)+C_{2}\\sin\\left(3t\\right)");
    // r⁴ + 4 = 0 as a quadratic in r²: roots ±1 ± i.
    assert_eq!(
        exact("y''''+4y=0"),
        "y=C_{1}e^{x}\\cos\\left(x\\right)+C_{2}e^{x}\\sin\\left(x\\right)+C_{3}e^{-x}\\cos\\left(x\\right)+C_{4}e^{-x}\\sin\\left(x\\right)"
    );
    assert_eq!(exact("y''''+5y''+4y=0"), "y=C_{1}\\cos\\left(x\\right)+C_{2}\\sin\\left(x\\right)+C_{3}\\cos\\left(2x\\right)+C_{4}\\sin\\left(2x\\right)");
    assert_eq!(
        exact("y'''-y=0"),
        "y=C_{2}e^{-\\frac{x}{2}}\\cos\\left(\\frac{\\sqrt{3}x}{2}\\right)+C_{3}e^{-\\frac{x}{2}}\\sin\\left(\\frac{\\sqrt{3}x}{2}\\right)+C_{1}e^{x}"
    );
}

#[test]
fn undetermined_coefficients() {
    assert_eq!(exact("y''+4y=\\sin x"), "y=C_{1}\\cos\\left(2x\\right)+C_{2}\\sin\\left(2x\\right)+\\frac{\\sin\\left(x\\right)}{3}");
    assert_eq!(exact("y''+y=x^2"), "y=C_{1}\\cos\\left(x\\right)+C_{2}\\sin\\left(x\\right)+x^{2}-2");
    // Resonance: eˣ already solves the homogeneous equation, so the guess gains an x.
    assert_eq!(exact("y''-y=e^{x}"), "y=C_{1}e^{x}+C_{2}e^{-x}+\\frac{xe^{x}}{2}");
}

#[test]
fn initial_conditions() {
    assert_eq!(exact("y'=2y, y(0)=3"), "y=3e^{2x}");
    assert_eq!(exact("y'+y=x, y(0)=1"), "y=x+2e^{-x}-1");
    assert_eq!(exact("y'=2y, y(1)=e"), "y=e^{2x-1}");
    assert_eq!(exact("y''+y=0, y(0)=0, y'(0)=1"), "y=\\sin\\left(x\\right)");
    assert_eq!(exact("y''+4y'+13y=0, y(0)=1, y'(0)=0"), "y=e^{-2x}\\cos\\left(3x\\right)+\\frac{2e^{-2x}\\sin\\left(3x\\right)}{3}");
    assert_eq!(exact("y''+\\omega^2y=0, y(0)=A, y'(0)=0"), "y=A\\cos\\left(\\omega x\\right)");
    // The condition picks the branch.
    assert_eq!(exact("y'=\\frac{x}{y}, y(0)=2"), "y=\\sqrt{x^{2}+4}");
    // Fewer conditions than the order leave a constant.
    assert_eq!(exact("y''+y=0, y(0)=1"), "y=C_{2}\\sin\\left(x\\right)+\\cos\\left(x\\right)");
    assert!(fails("y'=2y, y(0)=1, y'(0)=2").contains("first-order"));
    // y_0 is a parameter, not the unknown.
    assert_eq!(exact("y'=ky, y(0)=y_0"), "y=y_{0}e^{kx}");
}

#[test]
fn numeric_solutions() {
    let s = Settings::default();
    let a = evaluate("y'=\\sin(xy), y(0)=1", &s).unwrap();
    assert!(a.exact_latex.unwrap().starts_with("\\text{solved numerically: }y\\left(1\\right)\\approx 1.5340998"));
    // The pendulum.
    let a = evaluate("y''=-\\sin y, y(0)=1, y'(0)=0", &s).unwrap();
    assert!(a.exact_latex.unwrap().contains("0.600085"));
    assert!(fails("y'=x+y^2").contains("numerically"));
}

#[test]
fn solutions_on_the_graph() {
    let view = View { x_min: -2.0, x_max: 2.0, y_min: -5.0, y_max: 5.0, width: 200.0, height: 200.0 };
    let lines: Vec<String> = ["y'=2y, y(0)=1", "y'=2y", "y'=\\sin(xy), y(0)=1", "y'=\\frac{x}{y}, y(0)=2"].iter().map(|s| s.to_string()).collect();
    let curves = plot_sheet(&lines, &Settings::default(), view);
    // A particular solution is a function: y = e^{2x} at x = 0 is 1.
    match &curves[0].as_ref().unwrap().kind {
        CurveKind::Function { ys, .. } => assert!((ys[100].unwrap_or(0.0) - 1.0).abs() < 0.05),
        other => panic!("expected a function, got {other:?}"),
    }
    // One free constant: a family of members, separated by breaks.
    match &curves[1].as_ref().unwrap().kind {
        CurveKind::Path { points } => assert_eq!(points.iter().filter(|p| p.is_none()).count(), 5),
        other => panic!("expected a family, got {other:?}"),
    }
    // Numeric: a path through (0, 1).
    match &curves[2].as_ref().unwrap().kind {
        CurveKind::Path { points } => assert!(points.iter().flatten().any(|(x, y)| x.abs() < 1e-9 && (y - 1.0).abs() < 1e-9)),
        other => panic!("expected a path, got {other:?}"),
    }
    assert!(curves[3].is_some());
}
