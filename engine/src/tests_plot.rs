//! Tests for sampling sheet lines for the graph.
//! Test names are referenced from FEATURES.md.

use crate::{Curve, CurveKind, Settings, View, plot_sheet};

fn sheet(lines: &[&str], x_min: f64, x_max: f64, n: usize) -> Vec<Option<Curve>> {
    let lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let view = View { x_min, x_max, y_min: -10.0, y_max: 10.0, width: n as f64, height: 200.0 };
    plot_sheet(&lines, &Settings::default(), view)
}

fn ys(c: &Option<Curve>) -> &Vec<Option<f64>> {
    match &c.as_ref().expect("plottable").kind {
        CurveKind::Function { ys, .. } => ys,
        other => panic!("not a function: {other:?}"),
    }
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-12)
}

#[test]
fn plottable_lines_are_sampled() {
    let out = sheet(&["x^{2}", "f(t):=\\sin(t)", "y=2x+1", "a:=3", "2+2", "x+y+z", "x=5"], -1.0, 1.0, 5);
    // x² at -1, -0.5, 0, 0.5, 1
    let sq = ys(&out[0]);
    assert!(close(sq[0], 1.0) && close(sq[2], 0.0) && close(sq[3], 0.25));
    assert_eq!(out[1].as_ref().unwrap().label, "f\\left(t\\right)");
    assert!(close(ys(&out[1])[4], 1f64.sin()));
    assert_eq!(out[2].as_ref().unwrap().label, "y");
    assert!(close(ys(&out[2])[0], -1.0));
    // A constant, plain arithmetic, three letters and an equation in x alone: nothing to plot.
    assert!(out[3].is_none() && out[4].is_none() && out[5].is_none() && out[6].is_none());
}

#[test]
fn definitions_feed_later_curves() {
    let out = sheet(&["k:=2", "kx^{2}", "g(x):=x+1", "g(x)^{2}"], 0.0, 2.0, 3);
    assert!(close(ys(&out[1])[2], 8.0));
    assert!(close(ys(&out[3])[1], 4.0));
}

#[test]
fn undefined_points_are_gaps() {
    let out = sheet(&["\\frac{1}{x}", "\\sqrt{x}", "\\ln(x)"], -1.0, 1.0, 3);
    assert_eq!(ys(&out[0])[1], None);
    assert!(close(ys(&out[0])[2], 1.0));
    // √−1 and ln(−1) are complex: no point on the real graph.
    assert_eq!(ys(&out[1])[0], None);
    assert_eq!(ys(&out[2])[0], None);
}

#[test]
fn integrals_shade_and_derivatives_draw_tangents() {
    let out = sheet(&["\\int_{0}^{2}x^{2}\\,dx", "f(x):=x^{3}", "f'(1)", "\\left.\\frac{d}{dx}x^{2}\\right|_{x=3}"], -1.0, 1.0, 5);
    match &out[0].as_ref().unwrap().kind {
        CurveKind::Function { shade, ys } => {
            assert_eq!(*shade, Some((0.0, 2.0)));
            assert!(close(ys[0], 1.0));
        }
        other => panic!("{other:?}"),
    }
    match out[2].as_ref().unwrap().kind {
        CurveKind::Tangent { x0, y0, slope } => assert!(x0 == 1.0 && y0 == 1.0 && (slope - 3.0).abs() < 1e-12),
        ref other => panic!("{other:?}"),
    }
    match out[3].as_ref().unwrap().kind {
        CurveKind::Tangent { x0, y0, slope } => assert!(x0 == 3.0 && y0 == 9.0 && (slope - 6.0).abs() < 1e-12),
        ref other => panic!("{other:?}"),
    }
}

#[test]
fn implicit_parametric_and_polar_curves() {
    let out = sheet(&["x^{2}+y^{2}=1", "x=\\cos(t),y=\\sin(t)", "r=2", "xy=1"], -2.0, 2.0, 80);
    match &out[0].as_ref().unwrap().kind {
        CurveKind::Implicit { nx, ny, values, .. } => {
            assert_eq!(values.len(), (nx + 1) * (ny + 1));
            // Corner (x, y) = (−2, −10): 4 + 100 − 1 > 0.
            assert!(close(values[0], 103.0));
        }
        other => panic!("{other:?}"),
    }
    match &out[1].as_ref().unwrap().kind {
        CurveKind::Path { points } => {
            let (x, y) = points[0].unwrap();
            assert!((x - 1.0).abs() < 1e-12 && y.abs() < 1e-12);
            // Once round: it ends where it started.
            let (x, y) = points.last().unwrap().unwrap();
            assert!((x - 1.0).abs() < 1e-9 && y.abs() < 1e-9);
        }
        other => panic!("{other:?}"),
    }
    match &out[2].as_ref().unwrap().kind {
        CurveKind::Path { points } => {
            let quarter = points[points.len() / 4].unwrap();
            assert!(quarter.0.abs() < 1e-9 && (quarter.1 - 2.0).abs() < 1e-9);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(out[3].as_ref().unwrap().kind, CurveKind::Implicit { .. }));
}

#[test]
fn inequalities_ranges_and_surfaces() {
    let out = sheet(
        &[
            "y>x^{2}",
            "x=\\cos(t),y=\\sin(t),0\\le t\\le\\pi",
            "y=x^{2},-1\\le x\\le 1",
            "z=x^{2}-y^{2}",
            "x^{2}+y^{2}\\le 4,y>0",
            "r=\\theta,0\\le\\theta\\le 4\\pi",
        ],
        -2.0,
        2.0,
        5,
    );
    match &out[0].as_ref().unwrap().kind {
        CurveKind::Implicit { region, values, nx, .. } => {
            assert_eq!(*region, Some(true));
            // Corner (−2, −10): y − x² = −14 < 0, outside.
            assert!(values[0].is_some_and(|v| v < 0.0));
            // Top row, middle column (0, 10): inside.
            let top_mid = values[values.len() - 1 - nx / 2];
            assert!(top_mid.is_some_and(|v| v > 0.0));
        }
        other => panic!("{other:?}"),
    }
    // Half a turn: from (1, 0) to (−1, 0).
    match &out[1].as_ref().unwrap().kind {
        CurveKind::Path { points } => {
            let (x, y) = points.last().unwrap().unwrap();
            assert!((x + 1.0).abs() < 1e-9 && y.abs() < 1e-9);
        }
        other => panic!("{other:?}"),
    }
    // Only between −1 and 1 (samples at −2, −1, 0, 1, 2).
    let restricted = ys(&out[2]);
    assert!(restricted[0].is_none() && restricted[4].is_none() && close(restricted[1], 1.0));
    match &out[3].as_ref().unwrap().kind {
        CurveKind::Surface { zs, x, y, axes, .. } => {
            assert_eq!((*x, *y), ((-5.0, 5.0), (-5.0, 5.0)));
            assert_eq!(axes.2, "z");
            // Corner (−5, −5): 25 − 25 = 0.
            assert!(close(zs[0], 0.0));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(out[4].as_ref().unwrap().kind, CurveKind::Implicit { region: Some(false), .. }));
    match &out[5].as_ref().unwrap().kind {
        // r = θ ends at θ = 4π: (4π, 0).
        CurveKind::Path { points } => assert!((points.last().unwrap().unwrap().0 - 4.0 * std::f64::consts::PI).abs() < 1e-9),
        other => panic!("{other:?}"),
    }
}

#[test]
fn sampling_is_fast_enough_to_pan() {
    let start = std::time::Instant::now();
    let out = sheet(&["\\sin(x)", "x^{3}-2x", "e^{-x^{2}}", "\\tan(x)", "x^{2}+y^{2}=25"], -10.0, 10.0, 1000);
    assert!(out.iter().all(Option::is_some));
    assert!(start.elapsed().as_millis() < 2500, "took {:?}", start.elapsed());
}
