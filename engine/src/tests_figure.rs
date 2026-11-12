//! Tests for the geometry workspace's figure solver. Test names are referenced from
//! FEATURES.md.

use crate::figure::{CircleGiven, FigureInput, FigureResult, solve_figure};
use crate::Settings;

fn fig(kind: &str, points: &[[f64; 2]], sides: &[&str], angles: &[&str]) -> FigureResult {
    let opt = |v: &[&str]| v.iter().map(|s| if s.is_empty() { None } else { Some(s.to_string()) }).collect();
    let input = FigureInput { kind: kind.into(), points: points.to_vec(), sides: opt(sides), angles: opt(angles), circle: None };
    solve_figure(&input, &Settings::default())
}

fn quantity(r: &FigureResult, id: &str) -> (String, f64) {
    let q = r.quantities.iter().find(|q| q.id == id).unwrap_or_else(|| panic!("no {id} in {:?}", r.quantities.iter().map(|q| q.id).collect::<Vec<_>>()));
    (q.latex.clone(), q.value)
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

const SKETCH_TRI: [[f64; 2]; 3] = [[0.0, 0.0], [4.0, 0.0], [4.2, 2.5]];
const SKETCH_QUAD: [[f64; 2]; 4] = [[0.0, 0.0], [5.0, 0.2], [5.3, 3.8], [0.1, 4.1]];

#[test]
fn triangles_solve_exactly_and_to_scale() {
    // AB = 3, BC = 4, CA = 5: a right angle at B.
    let r = fig("triangle", &SKETCH_TRI, &["3", "4", "5"], &["", "", ""]);
    assert_eq!(r.status, "solved", "{}", r.message);
    assert_eq!(r.angles[1].latex.as_deref(), Some("90"));
    assert_eq!(r.angles[1].status, "found");
    assert_eq!(quantity(&r, "area").0, "6");
    assert_eq!(quantity(&r, "perimeter").0, "12");
    assert_eq!(quantity(&r, "circumradius").0, "\\frac{5}{2}");
    assert_eq!(quantity(&r, "inradius").0, "1");
    // To scale, with A where it was drawn and C on the drawn side.
    assert!((dist(r.points[0], r.points[1]) - 3.0).abs() < 1e-9);
    assert!((dist(r.points[1], r.points[2]) - 4.0).abs() < 1e-9);
    assert!(r.points[2][1] > 0.0);
    // A side and two angles.
    let r = fig("triangle", &SKETCH_TRI, &["10", "", ""], &["60", "60", ""]);
    assert_eq!(r.status, "solved");
    assert_eq!(quantity(&r, "area").0, "25\\sqrt{3}");
}

#[test]
fn what_is_missing_or_wrong() {
    let r = fig("triangle", &SKETCH_TRI, &["3", "", ""], &["", "", ""]);
    assert_eq!(r.status, "missing");
    assert_eq!(r.message, "Give 2 more sides or angles");
    let r = fig("triangle", &SKETCH_TRI, &["", "", ""], &["30", "60", "90"]);
    assert_eq!(r.status, "missing");
    assert!(r.message.contains("give a side"));
    let r = fig("triangle", &SKETCH_TRI, &["1", "2", "5"], &["", "", ""]);
    assert_eq!(r.status, "conflict");
    // A fourth value that disagrees.
    let r = fig("triangle", &SKETCH_TRI, &["3", "4", "5"], &["", "80", ""]);
    assert_eq!(r.status, "conflict");
    let r = fig("triangle", &SKETCH_TRI, &["x", "", ""], &["", "", ""]);
    assert_eq!(r.status, "error");
    assert!(r.message.contains("needs a number"));
}

#[test]
fn rectangles_squares_and_circles() {
    let r = fig("rectangle", &SKETCH_QUAD, &["20", "10", "", ""], &[]);
    assert_eq!(r.status, "solved");
    assert_eq!(quantity(&r, "area").0, "200");
    assert_eq!(quantity(&r, "perimeter").0, "60");
    assert_eq!(quantity(&r, "diagonal").0, "10\\sqrt{5}");
    assert!((dist(r.points[0], r.points[1]) - 20.0).abs() < 1e-9);
    assert_eq!(r.angles[0].status, "fixed");
    let r = fig("rectangle", &SKETCH_QUAD, &["20", "", "", ""], &[]);
    assert_eq!(r.status, "missing");
    let r = fig("rectangle", &SKETCH_QUAD, &["20", "", "21", ""], &[]);
    assert_eq!(r.status, "error");
    let r = fig("square", &SKETCH_QUAD, &["", "4", "", ""], &[]);
    assert_eq!(quantity(&r, "area").0, "16");
    let circle = |which: &str, value: &str| {
        let input = FigureInput { kind: "circle".into(), points: vec![[0.0, 0.0], [1.0, 0.0]], sides: vec![], angles: vec![], circle: Some(CircleGiven { which: which.into(), value: value.into() }) };
        solve_figure(&input, &Settings::default())
    };
    let r = circle("r", "2");
    assert_eq!(quantity(&r, "area").0, "4\\pi");
    assert_eq!(quantity(&r, "circumference").0, "4\\pi");
    let r = circle("A", "\\pi");
    assert_eq!(quantity(&r, "radius").0, "1");
    assert_eq!(r.radius, Some(1.0));
}

#[test]
fn any_polygon_adjusts_to_its_measurements() {
    // A plot: 20, 15, 20, 15 with a right angle at A (2·4 − 3 = 5 measurements).
    let r = fig("polygon", &SKETCH_QUAD, &["20", "15", "20", "15"], &["90", "", "", ""]);
    assert_eq!(r.status, "solved", "{}", r.message);
    assert!((quantity(&r, "area").1 - 300.0).abs() < 1e-6);
    assert!((quantity(&r, "perimeter").1 - 70.0).abs() < 1e-6);
    assert!((r.angles[2].value.unwrap() - 90.0).abs() < 1e-6);
    // One short.
    let r = fig("polygon", &SKETCH_QUAD, &["20", "15", "20", "15"], &["", "", "", ""]);
    assert_eq!(r.status, "missing");
    assert!(r.message.starts_with("Give 1 more side or angle"));
    // Angles that can't belong to a quadrilateral.
    let r = fig("polygon", &SKETCH_QUAD, &["20", "", "", ""], &["90", "90", "90", "80"]);
    assert_eq!(r.status, "conflict");
    assert!(r.message.contains("360"));
    // An irregular plot keeps its drawn shape where two would fit.
    let r = fig("polygon", &SKETCH_QUAD, &["30", "22", "25", "18"], &["85", "", "", ""]);
    assert_eq!(r.status, "solved", "{}", r.message);
    assert!((dist(r.points[0], r.points[1]) - 30.0).abs() < 1e-6);
    assert!(r.points.iter().all(|p| p[1] >= -1e-9));
}
