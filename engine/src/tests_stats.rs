//! Tests for statistics of lists and probability distributions. Test names are referenced
//! from FEATURES.md.

use crate::{Settings, evaluate, evaluate_sheet};

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

const DATA: &str = "[2,4,4,4,5,5,7,9]";

#[test]
fn centre_and_spread() {
    assert_eq!(exact(&format!("mean({DATA})")), "5");
    assert_eq!(exact("mean(1,2,3,4)"), "\\frac{5}{2}");
    assert_eq!(exact("median([3,1,4,1,5,9,2,6])"), "\\frac{7}{2}");
    assert_eq!(exact("median([3,1,4,1,5])"), "3");
    assert_eq!(exact("mode([1,2,2,3,3,3])"), "3");
    // A tie gives every mode.
    assert_eq!(exact("mode([1,2,2,3,3])"), "\\begin{pmatrix}2\\\\3\\end{pmatrix}");
    assert!(fails("mode([1,2,3])").contains("no mode"));
    // Sample (n − 1) and population (n), exactly.
    assert_eq!(exact(&format!("var({DATA})")), "\\frac{32}{7}");
    assert_eq!(exact(&format!("pvar({DATA})")), "4");
    assert_eq!(exact(&format!("sd({DATA})")), "\\frac{4\\sqrt{14}}{7}");
    assert_eq!(exact(&format!("psd({DATA})")), "2");
    assert_eq!(exact("range([3,1,4,1,5])"), "4");
    assert_eq!(exact("max(3,1,4)"), "4");
    assert_eq!(exact("sum([1,2,3])"), "6");
    assert_eq!(exact("count([1,2,3])"), "3");
    assert_eq!(exact("mean([a,b])"), "\\frac{a}{2}+\\frac{b}{2}");
    let a = evaluate(&format!("sd({DATA})"), &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Standard deviation");
}

#[test]
fn quartiles_and_percentiles() {
    assert_eq!(exact("quartile([1,2,3,4,5,6,7,8,9],1)"), "\\frac{5}{2}");
    assert_eq!(exact("quartile([1,2,3,4,5,6,7,8,9],3)"), "\\frac{15}{2}");
    assert_eq!(exact("iqr([1,2,3,4,5,6,7,8])"), "4");
    assert_eq!(exact("percentile([1,2,3,4,5],90)"), "\\frac{23}{5}");
    assert!(fails("percentile([1,2,3],120)").contains("0 to 100"));
}

#[test]
fn correlation_and_regression() {
    assert_eq!(exact("corr([1,2,3,4],[2,4,5,9])"), "\\frac{11\\sqrt{130}}{130}");
    assert_eq!(exact("cov([1,2,3,4],[2,4,5,9])"), "\\frac{11}{3}");
    assert_eq!(
        exact("linreg([1,2,3,4],[2,4,5,9])"),
        "y=\\frac{11x}{5}-\\frac{1}{2},\\quad a=-\\frac{1}{2},\\quad b=\\frac{11}{5},\\quad r=\\frac{11\\sqrt{130}}{130},\\quad r^{2}=\\frac{121}{130}"
    );
    let a = evaluate("linreg([1,2,3,4],[2,4,5,9])", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Linear regression");
    assert!(fails("corr([1,2,3],[1,2])").contains("same length"));
}

#[test]
fn summary_and_named_lists() {
    assert_eq!(
        exact(&format!("stats({DATA})")),
        "n=8,\\quad \\bar{x}=5,\\quad \\sum x=40,\\quad s=\\frac{4\\sqrt{14}}{7},\\quad \\sigma=2,\\quad \\min=2,\\quad Q_1=4,\\quad \\text{med}=\\frac{9}{2},\\quad Q_3=6,\\quad \\max=9"
    );
    let lines: Vec<String> = ["L := [3,1,4,1,5,9,2,6]", "mean(L)", "median(L)"].iter().map(|s| s.to_string()).collect();
    let out = evaluate_sheet(&lines, &Settings::default());
    assert_eq!(out[1].as_ref().unwrap().exact_latex.as_deref(), Some("\\frac{31}{8}"));
    assert_eq!(out[2].as_ref().unwrap().exact_latex.as_deref(), Some("\\frac{7}{2}"));
}

#[test]
fn discrete_distributions() {
    assert_eq!(exact("binompdf(10,0.5,3)"), "\\frac{15}{128}");
    assert_eq!(exact("binomcdf(10,\\frac{1}{2},3)"), "\\frac{11}{64}");
    assert_eq!(exact("binompdf(5,p,2)"), "10p^{2}\\left(-p+1\\right)^{3}");
    assert_eq!(exact("poissonpdf(2,3)"), "\\frac{4}{3e^{2}}");
    assert_eq!(exact("poissoncdf(2,3)"), "\\frac{19}{3e^{2}}");
    assert_eq!(exact("geompdf(\\frac{1}{6},3)"), "\\frac{25}{216}");
    assert_eq!(exact("geomcdf(\\frac{1}{6},3)"), "\\frac{91}{216}");
    assert!(fails("binompdf(10,2,3)").contains("between 0 and 1"));
}

#[test]
fn normal_distribution_and_erf() {
    assert_eq!(dec("\\operatorname{erf}(1)"), "0.84270079294971486934122063508261");
    assert_eq!(exact("\\frac{d}{dx}\\operatorname{erf}(x)"), "\\frac{2e^{-x^{2}}}{\\sqrt{\\pi}}");
    assert_eq!(dec("normalpdf(0)"), "0.39894228040143267793994605993438");
    assert_eq!(dec("normalcdf(-1,1)"), "0.68268949213708589717046509126408");
    assert_eq!(dec("normalcdf(-1.96,1.96)"), "0.95000420970355913172683146191833");
    assert_eq!(dec("normalcdf(-10^{99},1.645)"), "0.95001509446087863472188079765139");
    assert_eq!(dec("invnorm(0.975)"), "1.9599639845400542355245944305206");
    assert_eq!(exact("invnorm(0.5)"), "0");
    assert_eq!(dec("invnorm(0.95,100,15)"), "124.67280440427209072295773361987");
    assert!(fails("invnorm(1.5)").contains("between 0 and 1"));
    assert!(fails("normalpdf(0,0,-1)").contains("positive"));
}
