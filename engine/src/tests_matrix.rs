//! Tests for matrices and vectors: arithmetic, determinants, inverses, row reduction,
//! eigenvalues and vector products. Test names are referenced from FEATURES.md.

use crate::{Settings, View, evaluate, evaluate_sheet, plot_sheet};

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

const A: &str = "\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}";

#[test]
fn matrix_input_and_arithmetic() {
    assert_eq!(exact(A), "\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}");
    assert_eq!(exact("\\begin{bmatrix}1&2\\\\3&4\\end{bmatrix}"), "\\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}");
    assert_eq!(exact(&format!("{A}+\\begin{{pmatrix}}1&0\\\\0&1\\end{{pmatrix}}")), "\\begin{pmatrix}2&2\\\\3&5\\end{pmatrix}");
    assert_eq!(exact(&format!("2{A}")), "\\begin{pmatrix}2&4\\\\6&8\\end{pmatrix}");
    assert_eq!(exact(&format!("{A}\\begin{{pmatrix}}5\\\\6\\end{{pmatrix}}")), "\\begin{pmatrix}17\\\\39\\end{pmatrix}");
    // Products don't commute.
    let b = "\\begin{pmatrix}0&1\\\\1&0\\end{pmatrix}";
    assert_eq!(exact(&format!("{A}{b}")), "\\begin{pmatrix}2&1\\\\4&3\\end{pmatrix}");
    assert_eq!(exact(&format!("{b}{A}")), "\\begin{pmatrix}3&4\\\\1&2\\end{pmatrix}");
    assert_eq!(exact(&format!("{A}^{{T}}")), "\\begin{pmatrix}1&3\\\\2&4\\end{pmatrix}");
    assert_eq!(exact("\\begin{pmatrix}1&1\\\\1&0\\end{pmatrix}^{10}"), "\\begin{pmatrix}89&55\\\\55&34\\end{pmatrix}");
    assert!(fails("\\begin{pmatrix}1&2\\end{pmatrix}+\\begin{pmatrix}1\\\\2\\end{pmatrix}").contains("Can't add"));
    assert!(fails("\\begin{pmatrix}1&2\\\\3\\end{pmatrix}").contains("same number"));
    assert!(fails(&format!("\\sin({A})")).contains("isn't supported"));
    // The decimal answer is a matrix too.
    let a = evaluate("\\begin{pmatrix}\\sqrt{2}&1\\\\0&\\pi\\end{pmatrix}", &Settings::default()).unwrap();
    assert_eq!(a.decimal.unwrap().text, "[[1.4142135623730950488016887242097, 1], [0, 3.1415926535897932384626433832795]]");
}

#[test]
fn determinants() {
    assert_eq!(exact(&format!("\\det{A}")), "-2");
    assert_eq!(exact(&format!("\\left|{A}\\right|")), "-2");
    assert_eq!(exact("\\begin{vmatrix}2&0&1\\\\1&3&2\\\\1&1&4\\end{vmatrix}"), "18");
    assert_eq!(exact("det(\\begin{pmatrix}1&2&3&4\\\\5&6&7&8\\\\2&6&4&8\\\\3&1&1&2\\end{pmatrix})"), "72");
    assert_eq!(exact("\\det\\begin{pmatrix}x&1\\\\1&x\\end{pmatrix}"), "x^{2}-1");
    // And it can be solved for.
    assert_eq!(exact("\\det\\begin{pmatrix}x&1\\\\1&x\\end{pmatrix}=0"), "x=-1\\quad\\text{or}\\quad x=1");
    let a = evaluate("\\det\\begin{pmatrix}2&0&1\\\\1&3&2\\\\1&1&4\\end{pmatrix}", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Determinant");
    assert!(fails("\\det\\begin{pmatrix}1&2\\end{pmatrix}").contains("square"));
}

#[test]
fn inverses() {
    assert_eq!(exact(&format!("{A}^{{-1}}")), "\\begin{pmatrix}-2&1\\\\\\frac{3}{2}&-\\frac{1}{2}\\end{pmatrix}");
    assert_eq!(exact("inv(\\begin{pmatrix}2&1&1\\\\1&3&2\\\\1&0&0\\end{pmatrix})"), "\\begin{pmatrix}0&0&1\\\\-2&1&3\\\\3&-1&-5\\end{pmatrix}");
    assert_eq!(exact(&format!("{A}{A}^{{-1}}")), "\\begin{pmatrix}1&0\\\\0&1\\end{pmatrix}");
    assert_eq!(
        exact("\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}^{-1}"),
        "\\begin{pmatrix}\\frac{d}{ad-bc}&-\\frac{b}{ad-bc}\\\\-\\frac{c}{ad-bc}&\\frac{a}{ad-bc}\\end{pmatrix}"
    );
    assert!(fails("\\begin{pmatrix}1&2\\\\2&4\\end{pmatrix}^{-1}").contains("no inverse"));
    let a = evaluate("inv(\\begin{pmatrix}2&1&1\\\\1&3&2\\\\1&0&0\\end{pmatrix})", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Inverse");
    // Row operations read naturally, with + for a negative multiplier.
    assert!(a.methods[0].steps.iter().all(|s| !s.note.contains("--")));
}

#[test]
fn row_reduction_rank_and_trace() {
    let m = "\\begin{pmatrix}1&2&3\\\\4&5&6\\\\7&8&9\\end{pmatrix}";
    assert_eq!(exact(&format!("rref({m})")), "\\begin{pmatrix}1&0&-1\\\\0&1&2\\\\0&0&0\\end{pmatrix}");
    assert_eq!(exact(&format!("rank({m})")), "2");
    assert_eq!(exact(&format!("trace({A})")), "5");
    assert_eq!(exact("identity(3)"), "\\begin{pmatrix}1&0&0\\\\0&1&0\\\\0&0&1\\end{pmatrix}");
}

#[test]
fn eigenvalues_and_eigenvectors() {
    let s = "\\begin{pmatrix}2&1\\\\1&2\\end{pmatrix}";
    assert_eq!(exact(&format!("eigenvalues({s})")), "\\lambda_{1}=1,\\quad \\lambda_{2}=3");
    assert_eq!(
        exact(&format!("eigenvectors({s})")),
        "\\lambda_{1}=1:\\ \\begin{pmatrix}-1\\\\1\\end{pmatrix},\\quad \\lambda_{2}=3:\\ \\begin{pmatrix}1\\\\1\\end{pmatrix}"
    );
    // A rotation: complex eigenvalues.
    assert_eq!(exact("eigenvalues(\\begin{pmatrix}0&-1\\\\1&0\\end{pmatrix})"), "\\lambda_{1}=-i,\\quad \\lambda_{2}=i");
    assert_eq!(
        exact("eigenvectors(\\begin{pmatrix}2&0&0\\\\0&3&4\\\\0&4&9\\end{pmatrix})"),
        "\\lambda_{1}=1:\\ \\begin{pmatrix}0\\\\-2\\\\1\\end{pmatrix},\\quad \\lambda_{2}=2:\\ \\begin{pmatrix}1\\\\0\\\\0\\end{pmatrix},\\quad \\lambda_{3}=11:\\ \\begin{pmatrix}0\\\\1\\\\2\\end{pmatrix}"
    );
    let a = evaluate(&format!("eigenvalues({A})"), &Settings::default()).unwrap();
    assert_eq!(a.decimal.unwrap().text, "λ1 = -0.37228132326901432992530573410946, λ2 = 5.3722813232690143299253057341095");
    assert!(fails(&format!("1+eigenvalues({A})")).contains("by itself"));
}

#[test]
fn vectors() {
    assert_eq!(exact("[1,2,3]\\cdot[4,5,6]"), "32");
    assert_eq!(exact("dot([1,2,3],[4,5,6])"), "32");
    assert_eq!(exact("[1,0,0]\\times[0,1,0]"), "\\begin{pmatrix}0\\\\0\\\\1\\end{pmatrix}");
    assert_eq!(exact("cross([1,2,3],[4,5,6])"), "\\begin{pmatrix}-3\\\\6\\\\-3\\end{pmatrix}");
    assert_eq!(exact("|[3,4]|"), "5");
    assert_eq!(exact("norm([1,1,1])"), "\\sqrt{3}");
    assert_eq!(exact("2[1,2]+[0,1]"), "\\begin{pmatrix}2\\\\5\\end{pmatrix}");
    // × between numbers is still multiplication.
    assert_eq!(exact("2\\times3"), "6");
    assert!(fails("cross([1,2],[3,4])").contains("3 entries"));
}

#[test]
fn matrices_on_the_paper() {
    let lines: Vec<String> = [
        "A := \\begin{pmatrix}1&2\\\\3&4\\end{pmatrix}",
        "B := \\begin{pmatrix}0&1\\\\1&0\\end{pmatrix}",
        "AB",
        "A^{-1}",
        "\\det(A)",
        "\\frac{d}{dx}\\begin{pmatrix}x^2&\\sin x\\\\1&e^x\\end{pmatrix}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = evaluate_sheet(&lines, &Settings::default());
    let shown: Vec<String> = out.iter().map(|r| r.as_ref().unwrap().exact_latex.clone().unwrap()).collect();
    assert_eq!(shown[2], "\\begin{pmatrix}2&1\\\\4&3\\end{pmatrix}");
    assert_eq!(shown[3], "\\begin{pmatrix}-2&1\\\\\\frac{3}{2}&-\\frac{1}{2}\\end{pmatrix}");
    assert_eq!(shown[4], "-2");
    assert_eq!(shown[5], "\\begin{pmatrix}2x&\\cos\\left(x\\right)\\\\0&e^{x}\\end{pmatrix}");
    // The graph leaves matrices alone.
    let view = View { x_min: -5.0, x_max: 5.0, y_min: -5.0, y_max: 5.0, width: 100.0, height: 100.0 };
    let curves = plot_sheet(&lines, &Settings::default(), view);
    assert!(curves.iter().all(Option::is_none));
}
