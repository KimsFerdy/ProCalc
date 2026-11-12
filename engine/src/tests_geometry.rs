//! Tests for optimisation and geometry. Test names are referenced from FEATURES.md.

use crate::{AngleMode, Settings, evaluate};

fn exact(s: &str) -> String {
    let a = evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"));
    a.exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn deg_text(s: &str) -> String {
    let st = Settings { angle: AngleMode::Degrees, digits: 6 };
    evaluate(s, &st).unwrap_or_else(|e| panic!("{s}: {e}")).decimal.unwrap().text
}

fn fails(s: &str) -> String {
    match evaluate(s, &Settings::default()) {
        Ok(a) => panic!("{s} should fail, gave {:?}", a.exact_latex),
        Err(e) => e.message,
    }
}

#[test]
fn optimise_one_variable() {
    assert_eq!(exact("minimize(x^2-4x+7)"), "\\min\\ f=3\\ \\text{at}\\ x=2");
    assert_eq!(exact("maximize(-x^2+6x)"), "\\max\\ f=9\\ \\text{at}\\ x=3");
    assert_eq!(exact("minimize(x e^{x})"), "\\min\\ f=-\\frac{1}{e}\\ \\text{at}\\ x=-1");
    assert_eq!(exact("extrema(x^3-3x)"), "\\text{max}\\ f=2\\ \\text{at}\\ x=-1,\\quad \\text{min}\\ f=-2\\ \\text{at}\\ x=1");
    // f″ = 0: the sign of f′ decides.
    assert_eq!(exact("extrema(x^4)"), "\\text{min}\\ f=0\\ \\text{at}\\ x=0");
    assert_eq!(exact("extrema(x^3)"), "\\text{inflection}\\ f=0\\ \\text{at}\\ x=0");
    assert!(fails("maximize(x^2)").contains("no maximum"));
    assert!(fails("maximize(\\sin x)").contains("give an interval"));
    // Inside a larger expression: the value.
    assert_eq!(exact("2\\cdot minimize(x^2-4x+7)"), "6");
    let a = evaluate("minimize(x^2-4x+7)", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Optimisation");
}

#[test]
fn optimise_on_an_interval() {
    assert_eq!(exact("minimize(x^3-3x, -3, 3)"), "\\min\\ f=-18\\ \\text{at}\\ x=-3\\quad\\left(-3\\le x\\le 3\\right)");
    assert_eq!(exact("maximize(x^3-3x, -3 \\le x \\le 3)"), "\\max\\ f=18\\ \\text{at}\\ x=3\\quad\\left(-3\\le x\\le 3\\right)");
    assert_eq!(exact("maximize(\\sin x, 0 \\le x \\le 2\\pi)"), "\\max\\ f=1\\ \\text{at}\\ x=\\frac{\\pi}{2}\\quad\\left(0\\le x\\le 2\\pi\\right)");
    assert_eq!(exact("minimize(x+\\frac{1}{x}, 0.1 \\le x \\le 10)"), "\\min\\ f=2\\ \\text{at}\\ x=1\\quad\\left(\\frac{1}{10}\\le x\\le 10\\right)");
}

#[test]
fn optimise_several_variables() {
    assert_eq!(exact("minimize(x^2+y^2-2x+4y)"), "\\min\\ f=-5\\ \\text{at}\\ \\left(x,y\\right)=\\left(1,-2\\right)\\quad\\left(\\text{local}\\right)");
    assert_eq!(
        exact("extrema(x^3-3xy+y^3)"),
        "\\text{saddle}\\ f=0\\ \\text{at}\\ \\left(x,y\\right)=\\left(0,0\\right),\\quad \\text{min}\\ f=-1\\ \\text{at}\\ \\left(x,y\\right)=\\left(1,1\\right)"
    );
    assert_eq!(exact("extrema(x^2-y^2)"), "\\text{saddle}\\ f=0\\ \\text{at}\\ \\left(x,y\\right)=\\left(0,0\\right)");
}

#[test]
fn optimise_with_constraints() {
    assert_eq!(exact("maximize(xy, x+y=10)"), "\\max\\ f=25\\ \\text{at}\\ \\left(x,y\\right)=\\left(5,5\\right)\\quad\\left(x+y=10\\right)");
    assert_eq!(exact("minimize(x^2+y^2, x+y=1)"), "\\min\\ f=\\frac{1}{2}\\ \\text{at}\\ \\left(x,y\\right)=\\left(\\frac{1}{2},\\frac{1}{2}\\right)\\quad\\left(x+y=1\\right)");
    assert_eq!(
        exact("maximize(x+y, x^2+y^2=1)"),
        "\\max\\ f=\\sqrt{2}\\ \\text{at}\\ \\left(x,y\\right)=\\left(\\frac{\\sqrt{2}}{2},\\frac{\\sqrt{2}}{2}\\right)\\quad\\left(x^{2}+y^{2}=1\\right)"
    );
    assert_eq!(
        exact("minimize(x^2+y^2+z^2, x+2y+3z=14)"),
        "\\min\\ f=14\\ \\text{at}\\ \\left(x,y,z\\right)=\\left(1,2,3\\right)\\quad\\left(x+2y+3z=14\\right)"
    );
}

#[test]
fn triangles() {
    assert_eq!(deg_text("triangle(a=3, b=4, c=5)"), "a = 3, b = 4, c = 5, A = 36.8699, B = 53.1301, C = 90, area = 6, perimeter = 12");
    assert_eq!(
        exact("triangle(a=5, A=30°, B=60°)"),
        "a=5,\\quad b=5\\sqrt{3},\\quad c=10,\\quad A=\\frac{\\pi}{6},\\quad B=\\frac{\\pi}{3},\\quad C=\\frac{\\pi}{2},\\quad \\text{area}=\\frac{25\\sqrt{3}}{2},\\quad \\text{perimeter}=5\\sqrt{3}+15"
    );
    // SAS
    assert!(exact("triangle(a=7, b=9, C=\\frac{\\pi}{3})").contains("c=\\sqrt{67}"));
    // The ambiguous case: two triangles.
    assert_eq!(
        deg_text("triangle(a=6, b=8, A=30)"),
        "a = 6, b = 8, c = 11.4003, A = 30, B = 41.8103, C = 108.19, area = 22.8007, perimeter = 25.4003  or  a = 6, b = 8, c = 2.45607, A = 30, B = 138.19, C = 11.8103, area = 4.91213, perimeter = 16.4561"
    );
    assert!(fails("triangle(a=2, b=8, A=30°)").contains("No triangle"));
    assert!(fails("triangle(a=1, b=2, c=5)").contains("No triangle"));
    assert!(fails("triangle(A=30°, B=60°, C=90°)").contains("at least one of them a side"));
}

#[test]
fn shapes() {
    assert_eq!(exact("circle(r=2)"), "A=4\\pi,\\quad C=4\\pi,\\quad r=2,\\quad d=4");
    assert_eq!(exact("circle(d=10)"), "A=25\\pi,\\quad C=10\\pi,\\quad r=5,\\quad d=10");
    assert_eq!(exact("rectangle(w=3, h=4)"), "A=12,\\quad P=14,\\quad d=5");
    assert_eq!(exact("sphere(r=3)"), "V=36\\pi,\\quad S=36\\pi");
    assert_eq!(exact("cone(r=3, h=4)"), "V=12\\pi,\\quad l=5,\\quad S=24\\pi");
    assert_eq!(exact("cuboid(l=2, w=3, h=6)"), "V=36,\\quad S=72,\\quad d=7");
    assert_eq!(exact("square(x)"), "A=x^{2},\\quad P=4x,\\quad d=\\sqrt{2}x");
    // The main measurement inside a larger expression.
    assert_eq!(exact("2\\cdot circle(r=1)"), "2\\pi");
    assert!(fails("circle(r=-1)").contains("positive"));
}

#[test]
fn coordinates() {
    assert_eq!(exact("distance([1,2],[4,6])"), "d=5");
    assert_eq!(exact("distance([0,0,0],[1,2,2])"), "d=3");
    assert_eq!(exact("midpoint([1,2],[3,8])"), "M=\\begin{pmatrix}2\\\\5\\end{pmatrix}");
    assert_eq!(exact("slope([1,2],[3,8])"), "m=3");
    assert_eq!(exact("line([1,2],[3,8])"), "y=3x-1,\\quad m=3,\\quad b=-1");
    assert_eq!(exact("line([1,2],[1,5])"), "x=1");
    assert_eq!(exact("angle([1,0],[1,1])"), "\\theta=\\frac{\\pi}{4}");
    assert_eq!(exact("angle([0,0],[1,0],[1,1])"), "\\theta=\\frac{\\pi}{2}");
    assert_eq!(exact("polygon([0,0],[4,0],[4,3])"), "A=6,\\quad P=12");
    // A user's own function named like a shape still wins.
    let lines: Vec<String> = ["square(x) := x^2", "square(3)"].iter().map(|s| s.to_string()).collect();
    let out = crate::evaluate_sheet(&lines, &Settings::default());
    assert_eq!(out[1].as_ref().unwrap().exact_latex.as_deref(), Some("9"));
}
