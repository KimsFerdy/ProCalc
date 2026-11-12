//! Tests for definitions (`x := 5`, `f(x) := x^2`) carried from line to line.
//! Test names are referenced from FEATURES.md.

use crate::{Answer, Settings, evaluate, evaluate_sheet, evaluate_with};

/// Evaluates the last line after the earlier ones, notebook-style.
fn run(lines: &[&str]) -> Answer {
    let (last, before) = lines.split_last().unwrap();
    let defs: Vec<String> = before.iter().map(|s| s.to_string()).collect();
    evaluate_with(last, &defs, &Settings::default()).unwrap_or_else(|e| panic!("{lines:?}: {e}"))
}

fn exact(lines: &[&str]) -> String {
    run(lines).exact_latex.unwrap_or_else(|| panic!("{lines:?} has no exact form"))
}

fn fails(lines: &[&str]) -> bool {
    let (last, before) = lines.split_last().unwrap();
    let defs: Vec<String> = before.iter().map(|s| s.to_string()).collect();
    evaluate_with(last, &defs, &Settings::default()).is_err()
}

#[test]
fn variables_carry_to_later_lines() {
    assert_eq!(exact(&["x := 5", "2x+1"]), "11");
    // MathLive writes := as \coloneq.
    assert_eq!(exact(&["x\\coloneq 5", "x^{2}"]), "25");
    assert_eq!(exact(&["v_{max}:=3", "\\theta:=\\frac{1}{2}", "v_{max}\\theta"]), "\\frac{3}{2}");
    // Values are fixed when defined: later changes to y don't reach x.
    assert_eq!(exact(&["y:=2", "x:=3y", "y:=10", "x"]), "6");
    // Redefining from the old value.
    assert_eq!(exact(&["x:=5", "x:=x+1", "x"]), "6");
    // Undefined letters stay symbolic.
    assert_eq!(exact(&["x:=2", "x+y"]), "y+2");
    assert!(run(&["x:=2", "x+y"]).symbolic);
}

#[test]
fn definitions_report_what_they_define() {
    let a = evaluate("x := \\frac{1}{4}", &Settings::default()).unwrap();
    assert_eq!(a.defines.as_deref(), Some("x"));
    assert_eq!(a.exact_latex.as_deref(), Some("\\frac{1}{4}"));
    assert_eq!(a.decimal.unwrap().text, "0.25");
    let f = evaluate("f(x) := x\\cdot x", &Settings::default()).unwrap();
    assert_eq!(f.defines.as_deref(), Some("f"));
    assert_eq!(f.exact_latex.as_deref(), Some("x^{2}"));
    assert!(evaluate("2+2", &Settings::default()).unwrap().defines.is_none());
    let g = evaluate("area(w,h):=wh", &Settings::default()).unwrap();
    assert_eq!(g.defines.as_deref(), Some("\\operatorname{area}"));
}

#[test]
fn functions_can_be_called() {
    assert_eq!(exact(&["f(x):=x^{2}", "f(3)"]), "9");
    assert_eq!(exact(&["f(x):=x^{2}", "f(3)+f(4)"]), "25");
    assert_eq!(exact(&["f(x):=x^{2}", "f(y)"]), "y^{2}");
    assert_eq!(exact(&["f(x):=x^{2}", "f\\left(\\sqrt{2}\\right)"]), "2");
    assert_eq!(exact(&["f(x):=x^{2}", "2f(3)"]), "18");
    assert_eq!(exact(&["area(w,h):=wh", "area(3,4)"]), "12");
    assert_eq!(exact(&["\\operatorname{area}(w,h)\\coloneq wh", "\\operatorname{area}(3,4)"]), "12");
    assert_eq!(exact(&["\\phi(t):=2t", "\\phi(5)"]), "10");
    // Functions built from functions.
    assert_eq!(exact(&["f(x):=x+1", "g(x):=f(f(x))", "g(1)"]), "3");
    // Without a defined f, f(3) is still f·3.
    assert_eq!(exact(&["f(3)"]), "3f");
}

#[test]
fn function_scope_rules() {
    // Parameters shadow variables of the same name.
    assert_eq!(exact(&["x:=5", "f(x):=x+1", "f(2)"]), "3");
    assert_eq!(exact(&["x:=5", "f(x):=x+1", "x"]), "5");
    // A body sees the definitions made before it, not later ones.
    assert_eq!(exact(&["a:=2", "g(x):=ax", "a:=3", "g(1)"]), "2");
    assert_eq!(exact(&["g(x):=ax", "a:=3", "g(1)"]), "a");
}

#[test]
fn bad_definitions_are_errors() {
    for line in [
        "sin:=3", "e:=1", "\\pi:=3", "i:=2", "expand(x):=x", "xy:=3", "f(x,x):=1", "f(e):=1",
        "x:=", ":=3", "x:=1:=2", "x:=y=5", "f(2):=1", "x+1:=2", "f_{1}(x):=x", "x:=\\frac{1}{0}",
    ] {
        assert!(fails(&[line]), "{line} should fail");
    }
    assert!(fails(&["f(x):=x", "f(1,2)"]));
    // = is an equation now; mixing it with := is the error.
    assert!(evaluate("x:=y=5", &Settings::default()).unwrap_err().message.contains(":="));
}

#[test]
fn broken_earlier_lines_are_skipped() {
    assert_eq!(exact(&["x:=\\frac{1}{0}", "x:=4", "nonsense+", "x"]), "4");
    // Plain expressions among the earlier lines are ignored too.
    assert_eq!(exact(&["x:=4", "x+100", "x"]), "4");
}

#[test]
fn sheets_evaluate_in_one_pass() {
    let lines: Vec<String> = ["x:=5", "f(t):=t^{2}", "f(x)+1", "nonsense+", "x:=2", "f(x)"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let out = evaluate_sheet(&lines, &Settings::default());
    let shown: Vec<String> = out
        .iter()
        .map(|r| match r {
            Ok(a) => a.exact_latex.clone().unwrap_or_default(),
            Err(_) => "error".into(),
        })
        .collect();
    assert_eq!(shown, ["5", "t^{2}", "26", "error", "2", "4"]);
    // The same lines one at a time agree with the sheet.
    assert_eq!(exact(&["x:=5", "f(t):=t^{2}", "f(x)+1"]), "26");
}

#[test]
fn deeply_nested_functions_stop_quickly() {
    // Each level calls the one below ten times: g(1) makes 10⁴ calls, h would make 10⁵+.
    let mut lines: Vec<String> = vec!["a(x):=x+1".into()];
    let names = ['a', 'b', 'c', 'd', 'g', 'h'];
    for w in names.windows(2) {
        let calls = vec![format!("{}(x)", w[0]); 10].join("+");
        lines.push(format!("{}(x):={calls}", w[1]));
    }
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let start = std::time::Instant::now();
    let (h_def, up_to_g) = refs.split_last().unwrap();
    let mut with_g_call = up_to_g.to_vec();
    with_g_call.push("g(1)");
    assert_eq!(exact(&with_g_call), "20\\,000");
    assert!(fails(&[up_to_g, &[*h_def]].concat()));
    assert!(start.elapsed().as_secs() < 10, "took {:?}", start.elapsed());
}
