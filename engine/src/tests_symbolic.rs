//! Tests for the symbolic layer: simplification, surds, exact trig and printing.
//! Test names are referenced from FEATURES.md.

use crate::{AngleMode, Settings, evaluate, evaluate_numeric};

fn sym(s: &str) -> String {
    let a = evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"));
    a.exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn sym_deg(s: &str) -> String {
    let st = Settings { angle: AngleMode::Degrees, ..Settings::default() };
    evaluate(s, &st).unwrap_or_else(|e| panic!("{s}: {e}")).exact_latex.unwrap()
}

fn no_exact(s: &str) -> bool {
    evaluate(s, &Settings::default()).unwrap().exact_latex.is_none()
}

#[test]
fn like_terms_collect() {
    assert_eq!(sym("x+x"), "2x");
    assert_eq!(sym("3x-x"), "2x");
    assert_eq!(sym("x-x"), "0");
    assert_eq!(sym("2x+3y-x+y"), "x+4y");
    assert_eq!(sym("y+x"), sym("x+y"));
    assert_eq!(sym("x+1+x+2"), "2x+3");
    // Regression: a product starting with plain letters used to crash the printer.
    assert_eq!(sym("ab^{2}"), "ab^{2}");
    assert_eq!(sym("yx"), "xy");
}

#[test]
fn powers_combine() {
    assert_eq!(sym("x\\cdot x"), "x^{2}");
    assert_eq!(sym("x^{2}x^{3}"), "x^{5}");
    assert_eq!(sym("\\frac{x}{x}"), "1");
    assert_eq!(sym("\\frac{x^{5}}{x^{2}}"), "x^{3}");
    assert_eq!(sym("(x^{2})^{3}"), "x^{6}");
    assert_eq!(sym("(2x)^{2}"), "4x^{2}");
    assert_eq!(sym("x^{0}"), "1");
    assert_eq!(sym("0x"), "0");
}

#[test]
fn polynomial_order() {
    assert_eq!(sym("1+x+x^{2}"), "x^{2}+x+1");
    assert_eq!(sym("5-x^{3}"), "-x^{3}+5");
    assert_eq!(sym("\\pi+x+2"), "x+\\pi+2");
}

#[test]
fn fractions_print() {
    assert_eq!(sym("\\frac{1}{x}"), "\\frac{1}{x}");
    assert_eq!(sym("\\frac{x}{2}"), "\\frac{x}{2}");
    assert_eq!(sym("\\frac{3x}{4y}"), "\\frac{3x}{4y}");
    assert_eq!(sym("-\\frac{x}{2}"), "-\\frac{x}{2}");
    assert_eq!(sym("\\frac{1}{x^{2}}"), "\\frac{1}{x^{2}}");
}

#[test]
fn surds_simplify() {
    assert_eq!(sym("\\sqrt{8}"), "2\\sqrt{2}");
    assert_eq!(sym("\\sqrt{72}"), "6\\sqrt{2}");
    assert_eq!(sym("\\sqrt{12}+\\sqrt{3}"), "3\\sqrt{3}");
    assert_eq!(sym("\\sqrt{2}\\sqrt{3}"), "\\sqrt{6}");
    assert_eq!(sym("\\sqrt{2}\\sqrt{2}"), "2");
    assert_eq!(sym("\\sqrt{2}\\sqrt{8}"), "4");
    assert_eq!(sym("(\\sqrt{3})^{2}"), "3");
    assert_eq!(sym("(2\\sqrt{2})^{2}"), "8");
    assert_eq!(sym("\\sqrt[3]{16}"), "2\\sqrt[3]{2}");
    assert_eq!(sym("\\sqrt[4]{4}"), "\\sqrt{2}");
    assert_eq!(sym("\\sqrt[3]{-16}"), "-2\\sqrt[3]{2}");
}

#[test]
fn denominators_rationalise() {
    assert_eq!(sym("\\frac{1}{\\sqrt{2}}"), "\\frac{\\sqrt{2}}{2}");
    assert_eq!(sym("\\frac{6}{\\sqrt{3}}"), "2\\sqrt{3}");
    assert_eq!(sym("\\sqrt{\\frac{1}{2}}"), "\\frac{\\sqrt{2}}{2}");
    assert_eq!(sym("\\sqrt{\\frac{3}{4}}"), "\\frac{\\sqrt{3}}{2}");
}

#[test]
fn roots_of_variables() {
    assert_eq!(sym("\\sqrt{4x}"), "2\\sqrt{x}");
    assert_eq!(sym("\\sqrt{20x}+20x"), "20x+2\\sqrt{5x}");
    assert_eq!(sym("\\sqrt{x}\\sqrt{x}"), "x");
    assert_eq!(sym("(\\sqrt{x})^{2}"), "x");
}

#[test]
fn exact_trig_radians() {
    assert_eq!(sym("\\sin(\\frac{\\pi}{6})"), "\\frac{1}{2}");
    assert_eq!(sym("\\sin(\\frac{\\pi}{4})"), "\\frac{\\sqrt{2}}{2}");
    assert_eq!(sym("\\cos(\\frac{\\pi}{6})"), "\\frac{\\sqrt{3}}{2}");
    assert_eq!(sym("\\tan(\\frac{\\pi}{3})"), "\\sqrt{3}");
    assert_eq!(sym("\\tan(\\frac{\\pi}{6})"), "\\frac{\\sqrt{3}}{3}");
    assert_eq!(sym("\\cos(\\frac{2\\pi}{3})"), "-\\frac{1}{2}");
    assert_eq!(sym("\\sin(\\frac{7\\pi}{4})"), "-\\frac{\\sqrt{2}}{2}");
    assert_eq!(sym("\\sin(13\\pi)"), "0");
    assert!(evaluate("\\tan(\\frac{\\pi}{2})", &Settings::default()).is_err());
}

#[test]
fn exact_trig_degrees() {
    assert_eq!(sym_deg("\\sin(45)"), "\\frac{\\sqrt{2}}{2}");
    assert_eq!(sym_deg("\\cos(30)"), "\\frac{\\sqrt{3}}{2}");
    assert_eq!(sym_deg("\\sin(-30)"), "-\\frac{1}{2}");
    assert_eq!(sym_deg("\\sin(30^{\\circ})"), "\\frac{1}{2}");
    assert_eq!(sym("\\sin(45^{\\circ})"), "\\frac{\\sqrt{2}}{2}");
}

#[test]
fn exact_inverse_trig() {
    assert_eq!(sym("\\arcsin(\\frac{1}{2})"), "\\frac{\\pi}{6}");
    assert_eq!(sym("\\arccos(\\frac{\\sqrt{2}}{2})"), "\\frac{\\pi}{4}");
    assert_eq!(sym("\\arctan(1)"), "\\frac{\\pi}{4}");
    assert_eq!(sym("\\arctan(\\sqrt{3})"), "\\frac{\\pi}{3}");
    assert_eq!(sym("\\arcsin(-1)"), "-\\frac{\\pi}{2}");
    assert_eq!(sym("\\arccos(-\\frac{1}{2})"), "\\frac{2\\pi}{3}");
    assert_eq!(sym_deg("\\arccos(\\frac{\\sqrt{3}}{2})"), "30");
    // arcsin 2 is complex now: see tests_complex::functions_beyond_the_real_domain.
    assert!(no_exact("\\arcsin(2)"));
}

#[test]
fn logs_and_exp() {
    assert_eq!(sym("\\ln(e)"), "1");
    assert_eq!(sym("\\ln(e^{x})"), "x");
    assert_eq!(sym("e^{\\ln(x)}"), "x");
    assert_eq!(sym("\\log_{2}(2^{x})"), "x");
    assert_eq!(sym("\\log_{x}(x)"), "1");
    assert_eq!(sym("\\ln(x)"), "\\ln\\left(x\\right)");
    assert_eq!(sym("\\log(x)"), "\\log\\left(x\\right)");
    assert_eq!(sym("\\log_{3}(x)"), "\\log_{3}\\left(x\\right)");
    assert_eq!(sym("\\exp(2)"), "e^{2}");
}

#[test]
fn absolute_value() {
    assert_eq!(sym("\\left|-3x\\right|"), "3\\left|x\\right|");
    assert_eq!(sym("\\left|-\\pi\\right|"), "\\pi");
    assert_eq!(sym("\\left|-\\sqrt{2}\\right|"), "\\sqrt{2}");
}

#[test]
fn constants_stay_symbolic() {
    assert_eq!(sym("2\\pi"), "2\\pi");
    assert_eq!(sym("\\pi+\\pi"), "2\\pi");
    assert_eq!(sym("\\frac{\\pi}{2}"), "\\frac{\\pi}{2}");
    assert_eq!(sym("e\\cdot e"), "e^{2}");
    assert_eq!(sym("2\\sqrt{2}+\\pi"), "2\\sqrt{2}+\\pi");
}

#[test]
fn greek_and_subscripts() {
    assert_eq!(sym("\\theta+\\theta"), "2\\theta");
    assert_eq!(sym("x_1+x_1+x_2"), "2x_{1}+x_{2}");
    assert_eq!(sym("2\\omega t"), "2\\omega t");
    assert_eq!(sym("\\alpha\\beta"), "\\alpha\\beta");
}

#[test]
fn unevaluated_functions_hide_exact() {
    // "= sin(1)" would just repeat the input, so only the decimal is shown.
    assert!(no_exact("\\sin(1)"));
    assert!(no_exact("\\ln(2)"));
    assert!(no_exact("2^{100000000}"));
    // 32-bit builds (older phones) hold binary exponents only up to about 2^536,870,911, a
    // limit of the float library, so this one is reported as too large there.
    #[cfg(not(target_pointer_width = "32"))]
    assert!(no_exact("2^{1000000000}"));
    #[cfg(target_pointer_width = "32")]
    assert!(evaluate("2^{1000000000}", &Settings::default()).is_err());
}

#[test]
fn symbolic_results_have_no_decimal() {
    let a = evaluate("2x+1", &Settings::default()).unwrap();
    assert!(a.symbolic);
    assert!(a.decimal.is_none());
    let a = evaluate("x-x+1", &Settings::default()).unwrap();
    assert!(!a.symbolic);
    assert_eq!(a.decimal.unwrap().text, "1");
}

#[test]
fn symbolic_errors() {
    let s = Settings::default();
    assert!(evaluate("\\frac{x}{0}", &s).is_err());
    assert!(evaluate("\\ln(0)", &s).is_err());
    assert!(evaluate("\\log_{1}(x)", &s).is_err());
    assert!(evaluate("\\frac{x}{i-i}", &s).is_err());
}

/// The decimal from the symbolic path must match direct numeric evaluation digit for digit.
#[test]
fn symbolic_and_numeric_paths_agree() {
    let cases = [
        "\\sqrt{8}+\\sqrt{2}",
        "\\frac{1}{\\sqrt{2}}",
        "\\sqrt{2}\\sqrt{3}\\pi",
        "\\sin(\\frac{\\pi}{4})",
        "\\sin(2)+\\cos(3)",
        "\\ln(10)+\\log_{2}(10)",
        "e^{\\pi}",
        "\\sqrt[3]{-16}",
        "\\frac{22}{7}-\\pi",
        "(1+\\sqrt{5})^{2}",
        "\\arctan(\\sqrt{3})",
        "\\sqrt{\\frac{3}{4}}+\\frac{1}{3}",
        "2^{0.5}\\cdot3^{0.5}",
    ];
    for digits in [16, 32, 100] {
        for angle in [AngleMode::Radians, AngleMode::Degrees] {
            let st = Settings { digits, angle };
            for c in cases {
                let s = evaluate(c, &st).unwrap_or_else(|e| panic!("{c}: {e}")).decimal.unwrap();
                let n = evaluate_numeric(c, &st).unwrap_or_else(|e| panic!("{c}: {e}"));
                assert_eq!(s.text, n.text, "{c} at {digits} digits, {angle:?}");
            }
        }
    }
}

/// Hostile inputs must come back quickly with an answer or a clean error, never a panic.
#[test]
fn nasty_inputs_never_panic() {
    let cases = [
        "2^{2^{2^{2^{2}}}}",
        "2^{2^{2^{2^{2^{2}}}}}",
        "(\\sqrt{2})^{1000000}",
        "10!!",
        "(-1)!",
        "0.5!",
        "(x+1)^{100}",
        "x^{x^{x}}",
        "\\sqrt{\\sqrt{\\sqrt{\\sqrt{\\sqrt{2}}}}}",
        "\\sqrt[1000]{2}",
        "\\sqrt[0]{2}",
        "0^{0}",
        "0^{x}",
        "\\frac{0}{0}",
        "\\sin(10^{50})",
        "\\tan(10^{100}\\pi)",
        "\\ln(10^{-400})",
        "((((((((((x))))))))))",
        "\\frac{1}{3}^{\\frac{1}{3}}",
        "e^{1000000}",
        "99999999999999999999999^{\\frac{1}{2}}",
    ];
    for c in cases {
        let t = std::time::Instant::now();
        let r = std::panic::catch_unwind(|| evaluate(c, &Settings::default()));
        assert!(r.is_ok(), "{c} panicked");
        assert!(t.elapsed().as_secs_f64() < 3.0, "{c} took {:?}", t.elapsed());
    }
}

// ---------- Expand and factor ----------

#[test]
fn expand_products_and_powers() {
    assert_eq!(sym("expand((x+1)^{2})"), "x^{2}+2x+1");
    assert_eq!(sym("expand((x+1)^{3})"), "x^{3}+3x^{2}+3x+1");
    assert_eq!(sym("expand((a+b)(a-b))"), "a^{2}-b^{2}");
    assert_eq!(sym("expand(2(x+3))"), "2x+6");
    assert_eq!(sym("expand((x+y+z)^{2})"), "x^{2}+2xy+2xz+y^{2}+2yz+z^{2}");
    assert_eq!(sym("expand((\\sqrt{2}+1)^{2})"), "2\\sqrt{2}+3");
    assert_eq!(sym("\\operatorname{expand}\\left((x-1)(x+1)\\right)"), "x^{2}-1");
}

#[test]
fn expand_has_a_size_limit() {
    assert!(evaluate("expand((x+y+z)^{100})", &Settings::default()).is_err());
}

#[test]
fn factor_polynomials() {
    assert_eq!(sym("factor(x^2-1)"), "\\left(x-1\\right)\\left(x+1\\right)");
    assert_eq!(sym("factor(6x^2+3x)"), "3x\\left(2x+1\\right)");
    assert_eq!(sym("factor(x^2+2x+1)"), "\\left(x+1\\right)^{2}");
    assert_eq!(sym("factor(4x^2-12x+9)"), "\\left(2x-3\\right)^{2}");
    assert_eq!(sym("factor(2x^2-x-1)"), "\\left(x-1\\right)\\left(2x+1\\right)");
    assert_eq!(
        sym("factor(x^3-6x^2+11x-6)"),
        "\\left(x-3\\right)\\left(x-2\\right)\\left(x-1\\right)"
    );
    assert_eq!(sym("factor(x^4-1)"), "\\left(x-1\\right)\\left(x+1\\right)\\left(x^{2}+1\\right)");
    assert_eq!(sym("factor(-x^2+1)"), "-\\left(x-1\\right)\\left(x+1\\right)");
    assert_eq!(sym("factor(\\frac{1}{2}x^2-\\frac{1}{2})"), "\\frac{\\left(x-1\\right)\\left(x+1\\right)}{2}");
}

#[test]
fn factor_leaves_irreducibles_alone() {
    assert_eq!(sym("factor(x^2+1)"), "x^{2}+1");
    assert_eq!(sym("factor(x^2-2)"), "x^{2}-2");
    assert_eq!(sym("factor(x^3-2)"), "x^{3}-2");
}

#[test]
fn factor_several_variables() {
    assert_eq!(sym("factor(6x^2y+3xy)"), "3xy\\left(2x+1\\right)");
    assert_eq!(sym("factor(ax+ay)"), "a\\left(x+y\\right)");
}

#[test]
fn factor_treats_functions_as_variables() {
    assert_eq!(
        sym("factor(\\sin(x)^2-1)"),
        "\\left(\\sin\\left(x\\right)-1\\right)\\left(\\sin\\left(x\\right)+1\\right)"
    );
    assert_eq!(sym("\\sin(x)^2"), "\\sin^{2}\\left(x\\right)");
}

#[test]
fn factor_leaves_symbols_alone() {
    assert_eq!(sym("factor(x)"), "x");
}

/// Random products of linear factors: factoring their expansion must give a product
/// whose printed LaTeX parses back and expands to the same polynomial.
#[test]
fn factor_round_trips_through_latex() {
    let mut seed: u64 = 0x2545F4914F6CDD1D;
    let mut rand = |m: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % m
    };
    for _ in 0..300 {
        let k = 2 + rand(4) as usize; // 2..5 factors
        let factors: Vec<String> = (0..k)
            .map(|_| {
                let a = 1 + rand(4) as i64;
                let b = rand(13) as i64 - 6;
                format!("({a}x{b:+})")
            })
            .collect();
        let product = factors.concat();
        let expanded = sym(&format!("expand({product})"));
        let factored = sym(&format!("factor({product})"));
        assert!(factored.contains("\\left("), "{product}: not factored, got {factored}");
        let back = sym(&format!("expand({factored})"));
        assert_eq!(back, expanded, "{product} factored as {factored}");
    }
}

#[test]
fn fractions_with_letters_cancel() {
    assert_eq!(sym("\\frac{x^2-1}{x-1}"), "x+1");
    assert_eq!(sym("\\frac{x^3+1}{x+1}"), "x^{2}-x+1");
    assert_eq!(sym("\\frac{x^2-1}{x^2+2x+1}"), "\\frac{x-1}{x+1}");
    assert_eq!(sym("\\frac{x}{x^2+x}"), "\\frac{1}{x+1}");
    assert_eq!(sym("\\frac{2x+4}{6}"), "\\frac{x+2}{3}");
    // Several letters: exact division by the factors.
    assert_eq!(sym("\\frac{x^2-y^2}{x-y}"), "x+y");
    assert_eq!(sym("\\frac{x^2y+xy^2}{xy}"), "x+y");
    assert_eq!(sym("\\frac{ax+a}{x+1}"), "a");
    // Nothing in common: left as written.
    assert_eq!(sym("\\frac{x^3}{x-1}"), "\\frac{x^{3}}{x-1}");
    assert_eq!(sym("\\frac{1}{x}+\\frac{1}{x+1}"), "\\frac{1}{x+1}+\\frac{1}{x}");
    // A plain number below doesn't count as a common factor.
    assert_eq!(sym("taylor(\\cos x, \\pi, 2)"), "\\frac{\\left(x-\\pi\\right)^{2}}{2}-1+\\cdots");
    let a = evaluate("\\frac{x^2-1}{x-1}", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Simplify");
}

#[test]
fn simplify_combines_fractions() {
    assert_eq!(sym("simplify(\\frac{1}{x}+\\frac{1}{x+1})"), "\\frac{2x+1}{x\\left(x+1\\right)}");
    assert_eq!(sym("simplify(\\frac{1}{x-1}-\\frac{1}{x+1})"), "\\frac{2}{\\left(x-1\\right)\\left(x+1\\right)}");
    assert_eq!(sym("simplify(\\frac{x}{x^2-1}+\\frac{1}{x+1})"), "\\frac{2x-1}{x^{2}-1}");
}

#[test]
fn partial_fractions() {
    assert_eq!(sym("apart(\\frac{1}{x^2-1})"), "\\frac{1}{2\\left(x-1\\right)}-\\frac{1}{2\\left(x+1\\right)}");
    assert_eq!(sym("apart(\\frac{2x+3}{x^2+3x+2})"), "\\frac{1}{x+1}+\\frac{1}{x+2}");
    // Repeated and quadratic factors.
    assert_eq!(sym("apart(\\frac{1}{x(x+1)^2})"), "\\frac{1}{x}-\\frac{1}{x+1}-\\frac{1}{\\left(x+1\\right)^{2}}");
    assert_eq!(sym("apart(\\frac{1}{x^3+x})"), "\\frac{1}{x}-\\frac{x}{x^{2}+1}");
    // Divide first when the top's degree is high.
    assert_eq!(sym("apart(\\frac{x^3}{x^2-1})"), "x+\\frac{1}{2\\left(x+1\\right)}+\\frac{1}{2\\left(x-1\\right)}");
    assert!(evaluate("apart(\\frac{1}{xy})", &Settings::default()).is_err());
    let a = evaluate("apart(\\frac{1}{x^2-1})", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Partial fractions");
}

#[test]
fn polynomial_long_division() {
    let s = Settings::default();
    let a = evaluate("divide(x^3+1, x-1)", &s).unwrap();
    assert_eq!(a.exact_latex.unwrap(), "x^{2}+x+1+\\frac{2}{x-1}");
    assert_eq!(a.decimal.unwrap().text, "quotient x^{2}+x+1, remainder 2");
    assert_eq!(a.methods[0].name, "Long division");
    assert_eq!(sym("quotient(x^3+1, x-1)"), "x^{2}+x+1");
    assert_eq!(sym("remainder(x^4-1, x^2+1)"), "0");
    assert_eq!(evaluate("divide(17, 5)", &s).unwrap().exact_latex.unwrap(), "17=3\\cdot 5+2");
    assert!(evaluate("divide(x^2+y, x)", &s).is_err());
    assert!(evaluate("divide(x, 0)", &s).is_err());
}
