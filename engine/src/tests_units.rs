//! Tests for physical units: arithmetic, dimension checks, conversion and temperatures.
//! Test names are referenced from FEATURES.md.

use crate::{Answer, Settings, evaluate, evaluate_with};

fn calc(s: &str) -> Answer {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn exact(s: &str) -> String {
    calc(s).exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn dec(s: &str) -> String {
    calc(s).decimal.unwrap_or_else(|| panic!("{s} has no decimal")).text
}

fn error(s: &str) -> String {
    match evaluate(s, &Settings::default()) {
        Ok(a) => panic!("{s} should fail, gave {a:?}"),
        Err(e) => e.message,
    }
}

#[test]
fn unit_arithmetic() {
    assert_eq!(exact("5\\,\\mathrm{kg}+3\\,\\mathrm{kg}"), "8\\,\\mathrm{kg}");
    assert_eq!(exact("2\\,\\mathrm{m}\\cdot3\\,\\mathrm{m}"), "6\\,\\mathrm{m}^{2}");
    assert_eq!(exact("\\frac{100\\,\\mathrm{km}}{2\\,\\mathrm{h}}"), "50\\,\\mathrm{km}/\\mathrm{h}");
    assert_eq!(exact("\\sqrt{9\\,\\mathrm{m}^{2}}"), "3\\,\\mathrm{m}");
    assert_eq!(exact("\\left|-5\\,\\mathrm{m}\\right|"), "5\\,\\mathrm{m}");
    assert_eq!(dec("9.81\\,\\mathrm{m/s^2}"), "9.81 m/s^2");
    // Units that cancel leave a plain number.
    assert_eq!(exact("\\frac{6\\,\\mathrm{m}}{2\\,\\mathrm{m}}"), "3");
    assert_eq!(exact("\\frac{1\\,\\mathrm{L}}{1\\,\\mathrm{cm}^{3}}"), "1\\,000");
    // Upright function names and e are not units.
    assert_eq!(exact("\\mathrm{e}^{2}"), "e^{2}");
    assert!(calc("\\mathrm{sin}(1)").decimal.is_some());
}

#[test]
fn mixed_units_use_the_largest() {
    assert_eq!(exact("1\\,\\mathrm{km}+300\\,\\mathrm{m}"), "\\frac{13}{10}\\,\\mathrm{km}");
    assert_eq!(dec("1\\,\\mathrm{km}+300\\,\\mathrm{m}"), "1.3 km");
    assert_eq!(dec("1\\,\\mathrm{h}+30\\,\\mathrm{min}"), "1.5 h");
    assert_eq!(dec("1\\,\\mathrm{km}\\cdot1\\,\\mathrm{m}"), "0.001 km^2");
}

#[test]
fn conversions() {
    assert_eq!(dec("60\\,\\mathrm{mph}\\to\\mathrm{km/h}"), "96.56064 km/h");
    // After the arrow, plain letters spell units.
    assert_eq!(dec("60\\,\\mathrm{mph}\\to km/h"), "96.56064 km/h");
    assert_eq!(dec("1\\,\\mathrm{h}\\to s"), "3600 s");
    assert_eq!(dec("1\\,\\mathrm{ft}\\to\\mathrm{in}"), "12 in");
    assert_eq!(dec("1\\,\\mathrm{mi}\\to km"), "1.609344 km");
    assert_eq!(dec("2\\,\\mathrm{kg}\\cdot9.81\\,\\mathrm{m/s^2}\\to N"), "19.62 N");
    assert_eq!(dec("1\\,\\mathrm{kWh}\\to MJ"), "3.6 MJ");
    assert_eq!(dec("1\\,\\mathrm{atm}\\to kPa"), "101.325 kPa");
    assert_eq!(dec("5\\,\\mathrm{N}\\,\\mathrm{m}\\to J"), "5 J");
}

#[test]
fn prefixes_and_spellings() {
    assert_eq!(dec("1\\,\\mathrm{km}\\to mm"), "1000000 mm");
    assert_eq!(dec("1\\,\\mathrm{\\mu m}\\to nm"), "1000 nm");
    assert_eq!(dec("1\\,\\mathrm{um}\\to nm"), "1000 nm");
    assert_eq!(dec("2\\,\\mathrm{k\\Omega}\\to\\Omega"), "2000 Ω");
    assert_eq!(exact("2\\,\\mathrm{k\\Omega}"), "2\\,\\mathrm{k}\\Omega");
    assert_eq!(dec("1\\,\\mathrm{dam}\\to m"), "10 m");
    assert_eq!(dec("500\\,\\mathrm{mL}\\to L"), "0.5 L");
    assert_eq!(dec("1\\,\\mathrm{min}\\to s"), "60 s");
    // Typing "m" in the app gives \text{m}; a group of units splits up.
    assert_eq!(exact("3\\,\\text{m}"), "3\\,\\mathrm{m}");
    assert_eq!(dec("5\\,\\text{N m}\\to J"), "5 J");
    assert_eq!(dec("2\\,\\mathrm{kWh}\\to\\text{MJ}"), "7.2 MJ");
}

#[test]
fn temperatures() {
    assert_eq!(dec("20°C\\to°F"), "68 °F");
    assert_eq!(exact("20°C\\to°F"), "68\\,{}^{\\circ}\\mathrm{F}");
    assert_eq!(dec("20^{\\circ}\\mathrm{C}\\to\\mathrm{K}"), "293.15 K");
    assert_eq!(dec("-40°C\\to°F"), "-40 °F");
    assert_eq!(dec("300\\,\\mathrm{K}\\to°F"), "80.33 °F");
    assert_eq!(dec("98.6\\degree F\\to^{\\circ}C"), "37 °C");
    assert_eq!(dec("10°C+5°C"), "15 °C");
    // In compound units a degree is a difference: 1 °C = 1 K.
    assert_eq!(dec("4.18\\,\\mathrm{J/(g\\,°C)}\\to\\mathrm{J/(kg\\,K)}"), "4180 J/(K·kg)");
    assert!(error("10°C+5\\,\\mathrm{K}").contains("ambiguous"));
    // A degree sign without C or F is still an angle.
    assert_eq!(exact("\\sin(30°)"), "\\frac{1}{2}");
}

#[test]
fn units_with_definitions_and_variables() {
    let defs = vec!["m_{1}:=5\\,\\mathrm{kg}".to_string(), "a:=9.81\\,\\mathrm{m/s^2}".to_string()];
    let f = evaluate_with("m_{1}a\\to N", &defs, &Settings::default()).unwrap();
    assert_eq!(f.decimal.unwrap().text, "49.05 N");
    let d = evaluate("v:=3\\,\\mathrm{m/s}", &Settings::default()).unwrap();
    assert_eq!(d.defines.as_deref(), Some("v"));
    assert_eq!(d.exact_latex.as_deref(), Some("3\\,\\mathrm{m}/\\mathrm{s}"));
    let a = calc("x\\,\\mathrm{m}+2\\,\\mathrm{m}");
    assert!(a.symbolic);
    assert_eq!(a.exact_latex.as_deref(), Some("\\left(x+2\\right)\\,\\mathrm{m}"));
}

#[test]
fn unit_errors() {
    let e = error("5\\,\\mathrm{kg}+3\\,\\mathrm{m}");
    assert!(e.contains("kg") && e.contains("m"), "{e}");
    assert!(error("1\\,\\mathrm{m}\\to s").contains("convert"));
    assert!(error("5\\to\\mathrm{m}").contains("plain number"));
    assert!(error("5+3\\,\\mathrm{m}").contains("plain number"));
    assert!(error("\\sin(5\\,\\mathrm{m})").contains("sin"));
    assert!(error("2^{\\mathrm{m}}").contains("exponent"));
    assert!(error("\\mathrm{m}^{x}").contains("number"));
    assert!(error("5\\,\\mathrm{m}\\to2\\,\\mathrm{m}").contains("units only"));
    assert!(error("1\\,\\mathrm{m}\\to xyz").contains("Unknown unit"));
    assert!(evaluate("\\to\\mathrm{m}", &Settings::default()).is_err());
    assert!(evaluate("\\operatorname{floor}(2.5\\,\\mathrm{m})", &Settings::default()).is_err());
}
