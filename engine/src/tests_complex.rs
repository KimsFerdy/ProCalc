//! Tests for complex numbers: i, exact a + bi arithmetic, and complex function values.
//! Test names are referenced from FEATURES.md.

use crate::{AngleMode, Settings, evaluate, evaluate_numeric, parse};

fn exact(s: &str) -> String {
    let a = evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"));
    a.exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn dec(s: &str) -> String {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}")).decimal.unwrap().text
}

fn dec16(s: &str) -> String {
    let st = Settings { digits: 16, ..Settings::default() };
    evaluate(s, &st).unwrap_or_else(|e| panic!("{s}: {e}")).decimal.unwrap().text
}

#[test]
fn i_squared_is_minus_one() {
    assert_eq!(exact("i^{2}"), "-1");
    assert_eq!(exact("i\\cdot i"), "-1");
    assert_eq!(exact("i^{3}"), "-i");
    assert_eq!(exact("i^{4}"), "1");
    assert_eq!(exact("i^{-1}"), "-i");
    assert_eq!(exact("i^{101}"), "i");
    // MathLive's \imaginaryI and \imaginaryJ both mean i.
    assert_eq!(exact("\\imaginaryI^{2}"), "-1");
    assert_eq!(exact("\\imaginaryJ+i"), "2i");
}

#[test]
fn exact_complex_arithmetic() {
    assert_eq!(exact("(1+2i)(3-i)"), "5+5i");
    assert_eq!(exact("(1+i)^{2}"), "2i");
    assert_eq!(exact("(1+i)^{8}"), "16");
    assert_eq!(exact("\\frac{1}{1+i}"), "\\frac{1}{2}-\\frac{i}{2}");
    assert_eq!(dec("\\frac{1}{1+i}"), "0.5-0.5i");
    assert!(evaluate("\\frac{1}{1+i}", &Settings::default()).unwrap().decimal.unwrap().is_exact);
    assert_eq!(exact("\\frac{2+3i}{1-i}"), "-\\frac{1}{2}+\\frac{5i}{2}");
    assert_eq!(exact("\\frac{1}{1+\\sqrt{2}i}"), "\\frac{1}{3}-\\frac{\\sqrt{2}i}{3}");
    assert_eq!(exact("3+4i-3"), "4i");
    assert_eq!(exact("2\\pi i"), "2\\pi i");
}

#[test]
fn roots_of_negative_numbers() {
    assert_eq!(exact("\\sqrt{-4}"), "2i");
    assert_eq!(exact("\\sqrt{-2}"), "\\sqrt{2}i");
    assert_eq!(exact("(-8)^{0.5}"), "2\\sqrt{2}i");
    assert_eq!(exact("\\sqrt[4]{-16}"), "\\sqrt{2}+\\sqrt{2}i");
    assert_eq!(exact("\\sqrt{-4}\\sqrt{-9}"), "-6");
    // Odd roots of negative numbers stay real: the calculator convention.
    assert_eq!(exact("\\sqrt[3]{-8}"), "-2");
    assert_eq!(dec16("\\sqrt{-2}"), "1.414213562373095i");
}

#[test]
fn euler_and_logs() {
    assert_eq!(exact("e^{i\\pi}"), "-1");
    assert_eq!(exact("e^{i\\pi}+1"), "0");
    assert_eq!(exact("e^{\\frac{i\\pi}{2}}"), "i");
    assert_eq!(exact("\\ln(-1)"), "\\pi i");
    assert_eq!(exact("\\ln(i)"), "\\frac{\\pi i}{2}");
    assert_eq!(dec16("\\ln(-2)"), "0.6931471805599453+3.141592653589793i");
    assert_eq!(dec("e^{i}"), "0.54030230586813971740093660744298+0.8414709848078965066525023216303i");
    // i^i is real: e^(−π/2).
    assert_eq!(dec16("i^{i}"), "0.2078795763507619");
}

#[test]
fn modulus() {
    assert_eq!(exact("\\left|3+4i\\right|"), "5");
    assert_eq!(exact("\\left|i\\right|"), "1");
    assert_eq!(exact("\\left|1+i\\right|"), "\\sqrt{2}");
    assert_eq!(exact("\\left|\\frac{1}{1+i}\\right|"), "\\frac{\\sqrt{2}}{2}");
}

#[test]
fn functions_beyond_the_real_domain() {
    assert_eq!(dec16("\\arcsin(2)"), "1.570796326794897-1.316957896924817i");
    assert_eq!(dec16("\\arccos(2)"), "1.316957896924817i");
    assert_eq!(dec16("\\operatorname{arcosh}(0.5)"), "1.047197551196598i");
    assert_eq!(dec16("\\operatorname{artanh}(2)"), "0.5493061443340548+1.570796326794897i");
    assert_eq!(dec16("\\log(-100)"), "2+1.364376353841841i");
    // Round trips through complex values come back real.
    assert_eq!(dec("\\sin(\\arcsin(2))"), "2");
    assert_eq!(dec("\\cos(i)^{2}+\\sin(i)^{2}"), "1");
    assert_eq!(dec("\\exp(\\ln(3+4i))"), "3+4i");
    // Degree mode converts complex angles too.
    let deg = Settings { angle: AngleMode::Degrees, digits: 16 };
    assert_eq!(evaluate("\\arcsin(2)", &deg).unwrap().decimal.unwrap().text, "90-75.45612929021689i");
}

#[test]
fn symbolic_and_numeric_paths_agree_on_complex() {
    for s in ["\\frac{1}{1+i}", "(1+2i)^{3}", "\\sqrt{-2}", "e^{i}", "\\sqrt[4]{-16}", "\\frac{\\sqrt{2}}{3-i}", "i^{i}"] {
        let sym = evaluate(s, &Settings::default()).unwrap().decimal.unwrap().text;
        let num = evaluate_numeric(s, &Settings::default()).unwrap().text;
        assert_eq!(sym, num, "{s}");
    }
}

#[test]
fn i_with_a_subscript_is_a_variable() {
    assert_eq!(exact("i_{1}+i_{1}"), "2i_{1}");
    assert_eq!(exact("e_{1}+i"), "e_{1}+i");
    assert!(parse("i_{max}").is_ok());
}

fn deg(s: &str) -> crate::Answer {
    let st = Settings { angle: AngleMode::Degrees, ..Settings::default() };
    evaluate(s, &st).unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn real_and_imaginary_parts_and_conjugate() {
    assert_eq!(exact("\\operatorname{Re}(3+4i)"), "3");
    assert_eq!(exact("\\Re(2-i)"), "2");
    assert_eq!(exact("Im(3+4i)"), "4");
    assert_eq!(exact("\\operatorname{Im}(\\frac{1}{1+i})"), "-\\frac{1}{2}");
    assert_eq!(exact("\\operatorname{Re}(e^{i\\pi/3})"), "\\frac{1}{2}");
    assert_eq!(exact("conj(3+4i)"), "3-4i");
    assert_eq!(exact("\\overline{3+4i}"), "3-4i");
    assert_eq!(exact("(3+4i)\\overline{3+4i}"), "25");
    assert_eq!(exact("\\operatorname{Re}(5)"), "5");
    assert_eq!(exact("\\operatorname{Im}(5)"), "0");
    // Numerically too: e^i has no exact parts.
    assert_eq!(dec("\\operatorname{Re}(e^{i})"), "0.54030230586813971740093660744298");
    // With letters they stay as written; typed Re without a bracket is still R·e.
    assert_eq!(exact("\\operatorname{Im}(x)"), "\\operatorname{Im}\\left(x\\right)");
    assert_eq!(exact("\\overline{x}"), "\\overline{x}");
    assert_eq!(exact("Re"), "eR");
}

#[test]
fn argument_of_a_complex_number() {
    assert_eq!(exact("\\arg(1+i)"), "\\frac{\\pi}{4}");
    assert_eq!(exact("\\arg(-1+\\sqrt{3}i)"), "\\frac{2\\pi}{3}");
    assert_eq!(exact("\\arg(-1-\\sqrt{3}i)"), "-\\frac{2\\pi}{3}");
    assert_eq!(exact("\\arg(-2i)"), "-\\frac{\\pi}{2}");
    assert_eq!(exact("\\arg(-1)"), "\\pi");
    assert_eq!(exact("\\arg(0)"), "0");
    assert_eq!(dec("\\arg(3+4i)"), "0.92729521800161223242851246292243");
    assert_eq!(dec("\\arg(e^{2i})"), "2");
    assert_eq!(deg("\\arg(-1)").exact_latex.unwrap(), "180");
    assert_eq!(deg("\\arg(1+i)").exact_latex.unwrap(), "45");
}

#[test]
fn polar_form() {
    assert_eq!(exact("polar(1+i)"), "\\sqrt{2}e^{i\\frac{\\pi}{4}}");
    assert_eq!(exact("polar(i)"), "e^{i\\frac{\\pi}{2}}");
    assert_eq!(exact("polar(-2)"), "2e^{i\\pi}");
    assert_eq!(exact("polar(5)"), "5");
    assert_eq!(exact("polar(-1-\\sqrt{3}i)"), "2e^{-i\\frac{2\\pi}{3}}");
    assert_eq!(exact("polar(-3+4i)"), "5e^{i\\left(\\pi-\\arctan\\left(\\frac{4}{3}\\right)\\right)}");
    assert_eq!(dec("polar(3+4i)"), "5·e^(0.92729521800161223242851246292243i)");
    let a = deg("polar(1+i)");
    assert_eq!(a.exact_latex.unwrap(), "\\sqrt{2}\\angle 45^{\\circ}");
    assert_eq!(deg("polar(3+4i)").decimal.unwrap().text, "5∠53.130102354155978703144387440907°");
    let a = evaluate("polar(1+i)", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Polar form");
    assert!(evaluate("polar(x)", &Settings::default()).is_err());
}

#[test]
fn angle_notation() {
    assert_eq!(exact("2\\angle\\frac{\\pi}{3}"), "1+\\sqrt{3}i");
    assert_eq!(exact("2\\angle60°"), "1+\\sqrt{3}i");
    assert_eq!(deg("2\\angle60").exact_latex.unwrap(), "1+\\sqrt{3}i");
    assert_eq!(exact("\\sqrt{2}\\angle\\frac{\\pi}{4}+1"), "2+i");
}

#[test]
fn all_nth_roots() {
    assert_eq!(exact("roots(8,3)"), "z_{0}=2,\\quad z_{1}=-1+\\sqrt{3}i,\\quad z_{2}=-1-\\sqrt{3}i");
    assert_eq!(exact("roots(1,4)"), "z_{0}=1,\\quad z_{1}=i,\\quad z_{2}=-1,\\quad z_{3}=-i");
    assert_eq!(
        exact("roots(-16,4)"),
        "z_{0}=\\sqrt{2}+\\sqrt{2}i,\\quad z_{1}=-\\sqrt{2}+\\sqrt{2}i,\\quad z_{2}=-\\sqrt{2}-\\sqrt{2}i,\\quad z_{3}=\\sqrt{2}-\\sqrt{2}i"
    );
    assert_eq!(exact("roots(i,3)"), "z_{0}=\\frac{\\sqrt{3}}{2}+\\frac{i}{2},\\quad z_{1}=-\\frac{\\sqrt{3}}{2}+\\frac{i}{2},\\quad z_{2}=-i");
    // Square roots exactly, with no angle needed.
    assert_eq!(exact("roots(3+4i,2)"), "z_{0}=2+i,\\quad z_{1}=-2-i");
    // Angles off the table: decimals.
    assert!(dec("roots(1,5)").starts_with("z0 = 1, z1 = 0.30901699437494742410229341718282+0.95105651629515357211643933337938i"));
    assert_eq!(exact("roots(0,3)"), "0");
    for bad in ["roots(8,0)", "roots(8,1.5)", "roots(x,2)", "roots(8)"] {
        assert!(evaluate(bad, &Settings::default()).is_err(), "{bad} should fail");
    }
    let a = evaluate("roots(8,3)", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "De Moivre's theorem");
}

#[test]
fn equations_with_only_complex_roots() {
    let s = |e: &str| evaluate(e, &Settings::default()).unwrap().exact_latex.unwrap();
    assert_eq!(
        s("z^{4}=-16"),
        "z=\\sqrt{2}+\\sqrt{2}i\\quad\\text{or}\\quad z=-\\sqrt{2}+\\sqrt{2}i\\quad\\text{or}\\quad z=-\\sqrt{2}-\\sqrt{2}i\\quad\\text{or}\\quad z=\\sqrt{2}-\\sqrt{2}i"
    );
    // A real root is still the only one listed.
    assert_eq!(s("x^{3}=8"), "x=2");
    // Complex numbers aren't ordered, so a condition like x > 0 rules them out.
    assert_eq!(s("x^{4}=-16, x>0"), "\\text{no solution}");
    assert_eq!(s("x^{2}=-4, x>0"), "\\text{no solution}");
}

#[test]
fn complex_errors_and_hostile_input() {
    let s = Settings::default();
    for bad in ["\\frac{1}{i-i}", "\\ln(0)", "i!", "\\operatorname{floor}(i)", "0^{i}", "\\operatorname{artanh}(1)"] {
        assert!(evaluate(bad, &s).is_err(), "{bad} should fail");
    }
    let start = std::time::Instant::now();
    for hostile in ["(1+i)^{1000}", "(1+i)^{1000000}", "e^{10^{50}i}", "i^{10^{100}}", "\\sqrt{-10^{400}}"] {
        let _ = evaluate(hostile, &s);
    }
    assert!(start.elapsed().as_secs() < 20, "took {:?}", start.elapsed());
}
