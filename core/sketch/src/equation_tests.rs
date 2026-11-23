// SPDX-License-Identifier: MIT
//! Equation curves (mitcad#2237): formulas placed as a fit point spline
//! within its tolerance, following the parameters they name, closed where
//! they return to their start, kept joined across an edit, and refused in
//! words.

use std::f64::consts::PI;

use super::*;
use crate::def::SketchDef;
use crate::expr::{ParamSpec, Unit};
use std::collections::BTreeSet;

use crate::geometry::{Curve2, P2, dist};
use crate::ids::EntityUid;
use crate::ids::FeatureUid;
use crate::parameters::Parameters;
use crate::values::ValueInput;
use crate::{EntityKind, Ref};

fn input(mode: EquationMode, formulas: &[(&str, &str)], start: &str, end: &str) -> EquationInput {
    let mut input = EquationInput {
        mode: Some(mode),
        start: Some(ValueInput::Name(start.to_owned())),
        end: Some(ValueInput::Name(end.to_owned())),
        ..EquationInput::default()
    };
    for (field, text) in formulas {
        let text = Some((*text).to_owned());
        match *field {
            "x" => input.x = text,
            "y" => input.y = text,
            _ => input.r = text,
        }
    }
    input
}

fn spline(def: &SketchDef, curve: EntityUid) -> (Vec<EntityUid>, bool) {
    match &def.entity(curve).expect("the curve").kind {
        EntityKind::FittedSpline { points, closed } => (points.clone(), *closed),
        other => panic!("not a fit point spline: {other:?}"),
    }
}

fn at(def: &SketchDef, point: EntityUid) -> P2 {
    match def.entity(point).expect("the point").kind {
        EntityKind::Point { at } => at,
        _ => panic!("not a point"),
    }
}

/// The solved curve of a finished sketch.
fn solved_curve(def: &SketchDef, params: &Parameters, curve: EntityUid) -> Curve2 {
    let solved = def
        .solve_with(&mut |id| params.value(id).ok_or_else(String::new), &|id| {
            params.name(id)
        })
        .expect("the sketch solves");
    solved.curves[&curve].clone()
}

fn user(
    params: &mut Parameters,
    name: &str,
    expression: &str,
    unit: Unit,
) -> crate::parameters::ParamId {
    params
        .add(ParamSpec::user(name, expression, unit), None)
        .expect("a parameter")
}

fn refusal(params: &mut Parameters, input: &EquationInput) -> String {
    let mut edit = SketchEdit::new(SketchDef::default(), params, FeatureUid(1), "Sketch1");
    edit.equation_curve(input)
        .expect_err("the curve is refused")
}

#[test]
fn explicit_parabola_within_its_tolerance() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&input(
            EquationMode::Explicit,
            &[("y", "x^2/20")],
            "-10",
            "10",
        ))
        .unwrap();
    assert!(!made.closed && !made.capped);
    assert!(made.points.len() >= 8 && made.points.len() <= MAX_POINTS);
    assert!(made.deviation <= DEFAULT_TOLERANCE, "{}", made.deviation);
    let (def, report) = edit.finish().unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    // Its points are on the parabola, fixed, from x = -10 to x = 10.
    for p in &made.points {
        let e = def.entity(*p).unwrap();
        assert!(e.fixed);
        let [x, y] = at(&def, *p);
        assert!((y - x * x / 20.0).abs() < 1e-12);
    }
    assert!((at(&def, made.points[0])[0] + 10.0).abs() < 1e-12);
    assert!((at(&def, *made.points.last().unwrap())[0] - 10.0).abs() < 1e-12);
    // The spline is within the tolerance of the formula everywhere.
    let curve = solved_curve(&def, &params, made.curve);
    for i in 0..=400 {
        let x = -10.0 + 20.0 * f64::from(i) / 400.0;
        let away = curve.closest([x, x * x / 20.0]).1;
        assert!(away <= DEFAULT_TOLERANCE, "x {x}: {away} mm away");
    }
    // The record keeps the formulas.
    assert_eq!(def.equations.len(), 1);
    let q = &def.equations[0];
    assert_eq!(
        (q.mode, q.y.as_str(), q.curve),
        (EquationMode::Explicit, "x^2/20", made.curve)
    );
    assert!(q.x.is_empty() && q.r.is_empty());
}

/// `r` of the involute is a parameter: changing it moves the curve's
/// points before the next solve, the same number of them.
#[test]
fn involute_follows_its_parameter() {
    let mut params = Parameters::default();
    let r = user(&mut params, "r", "5 mm", Unit::MM);
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&input(
            EquationMode::Parametric,
            &[("x", "r*(cos(t)+t*sin(t))"), ("y", "r*(sin(t)-t*cos(t))")],
            "0",
            "2",
        ))
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    let involute = |r: f64, t: f64| [r * (t.cos() + t * t.sin()), r * (t.sin() - t * t.cos())];
    let end = at(&def, *made.points.last().unwrap());
    assert!(dist(end, involute(5.0, 2.0)) < 1e-9, "{end:?}");
    assert!(dist(at(&def, made.points[0]), [5.0, 0.0]) < 1e-9);
    assert_eq!(used_parameters(&def), BTreeSet::from(["r".to_owned()]));
    // r changes: the points follow, as many as before.
    params.set_expression(r, "8 mm").unwrap();
    let mut followed = def.clone();
    let warnings = follow_formulas(&mut followed, params.table());
    assert!(
        warnings.iter().all(|w| !w.contains("last shape")),
        "{warnings:?}"
    );
    let (points, closed) = spline(&followed, made.curve);
    assert_eq!((points.len(), closed), (made.points.len(), false));
    let count = points.len();
    for (i, p) in points.iter().enumerate() {
        let t = 2.0 * i as f64 / (count - 1) as f64;
        assert!(dist(at(&followed, *p), involute(8.0, t)) < 1e-9);
    }
    // An edit of the sketch places them so too.
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.add_point([50.0, 50.0]);
    let (def, _) = edit.finish().unwrap();
    let end = at(&def, *spline(&def, made.curve).0.last().unwrap());
    assert!(dist(end, involute(8.0, 2.0)) < 1e-9, "{end:?}");
    let curve = solved_curve(&def, &params, made.curve);
    let (_, e) = curve.ends().unwrap();
    assert!(dist(e, involute(8.0, 2.0)) < 1e-9);
}

#[test]
fn full_circle_is_closed_and_smooth() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&input(
            EquationMode::Parametric,
            &[("x", "10*cos(t)"), ("y", "10*sin(t)")],
            "0",
            "2*PI",
        ))
        .unwrap();
    assert!(made.closed);
    let (def, _) = edit.finish().unwrap();
    let (points, closed) = spline(&def, made.curve);
    assert!(closed);
    // The point at 2π is the one at 0: listed once.
    assert_eq!(points.len(), made.points.len());
    for p in &points {
        let [x, y] = at(&def, *p);
        assert!(((x * x + y * y).sqrt() - 10.0).abs() < 1e-12);
    }
    let curve = solved_curve(&def, &params, made.curve);
    assert!(curve.is_closed());
    let (lo, hi) = curve.domain();
    let [p0, d0, s0] = curve.eval(lo);
    let [p1, d1, s1] = curve.eval(hi);
    assert!(dist(p0, p1) < 1e-9);
    // Smooth where it closes: the same first and second derivatives.
    assert!(
        dist(d0, d1) < 1e-6 * (1.0 + d0[0].hypot(d0[1])),
        "{d0:?} {d1:?}"
    );
    assert!(
        dist(s0, s1) < 1e-6 * (1.0 + s0[0].hypot(s0[1])),
        "{s0:?} {s1:?}"
    );
    for i in 0..360 {
        let t = f64::from(i) * PI / 180.0;
        let away = curve.closest([10.0 * t.cos(), 10.0 * t.sin()]).1;
        assert!(away <= DEFAULT_TOLERANCE, "{i} deg: {away}");
    }
}

/// A polar spiral `r = a + b*t`: the range in the design's angle unit
/// (degrees), `t` in radians inside the formula.
#[test]
fn polar_spiral() {
    let mut params = Parameters::default();
    user(&mut params, "a", "2 mm", Unit::MM);
    user(&mut params, "b", "1 mm", Unit::MM);
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&input(EquationMode::Polar, &[("r", "a + b*t")], "0", "720"))
        .unwrap();
    assert!(!made.closed && !made.capped);
    let (def, report) = edit.finish().unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let count = made.points.len();
    for (i, p) in made.points.iter().enumerate() {
        let t = 4.0 * PI * i as f64 / (count - 1) as f64;
        let r = 2.0 + t;
        assert!(dist(at(&def, *p), [r * t.cos(), r * t.sin()]) < 1e-9);
    }
    let curve = solved_curve(&def, &params, made.curve);
    for i in 0..=720 {
        let t = f64::from(i) * PI / 180.0;
        let r = 2.0 + t;
        let away = curve.closest([r * t.cos(), r * t.sin()]).1;
        assert!(away <= DEFAULT_TOLERANCE, "{i} deg: {away}");
    }
}

/// A line from the curve's end stays on it when the formulas change: the
/// end point keeps its id.
#[test]
fn joined_line_stays_joined_after_an_edit() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&input(
            EquationMode::Explicit,
            &[("y", "x^2/20")],
            "0",
            "10",
        ))
        .unwrap();
    let end = *made.points.last().unwrap();
    let far = edit.add_point([20.0, 0.0]);
    let line = edit.add_curve(EntityKind::Line {
        start: end,
        end: far,
        centerline: false,
    });
    let (def, _) = edit.finish().unwrap();
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let edited = edit
        .edit_equation_curve(
            made.curve,
            &EquationInput {
                y: Some("x^3/100 + 2".to_owned()),
                tolerance: Some(0.0001),
                ..EquationInput::default()
            },
        )
        .unwrap();
    assert_eq!(edited.points[0], made.points[0]);
    assert_eq!(*edited.points.last().unwrap(), end);
    assert!(edited.points.len() > made.points.len());
    let (def, _) = edit.finish().unwrap();
    let q = &def.equations[0];
    assert_eq!(
        (q.y.as_str(), q.tolerance, q.start.as_str()),
        ("x^3/100 + 2", 0.0001, "0")
    );
    match def.entity(line).unwrap().kind {
        EntityKind::Line { start, .. } => assert_eq!(start, end),
        _ => panic!("the line"),
    }
    let solved = def
        .solve_with(&mut |id| params.value(id).ok_or_else(String::new), &|id| {
            params.name(id)
        })
        .unwrap();
    let (_, curve_end) = solved.curves[&made.curve].ends().unwrap();
    let (line_start, _) = solved.curves[&line].ends().unwrap();
    assert!(dist(curve_end, [10.0, 12.0]) < 1e-9, "{curve_end:?}");
    assert!(dist(line_start, curve_end) < 1e-9);
    // Fewer points again: those it no longer needs go.
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    let fewer = edit
        .edit_equation_curve(
            made.curve,
            &EquationInput {
                tolerance: Some(1.0),
                ..EquationInput::default()
            },
        )
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    assert_eq!(*fewer.points.last().unwrap(), end);
    let points = def.entities.iter().filter(|e| e.is_point()).count();
    assert_eq!(points, fewer.points.len() + 1);
    // Deleting the curve takes its formulas.
    let mut edit = SketchEdit::new(def, &mut params, FeatureUid(1), "Sketch1");
    edit.remove(&[crate::modify::Item::Entity(Ref::Curve(made.curve))])
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    assert!(def.equations.is_empty());
}

#[test]
fn refusals_in_words() {
    let mut params = Parameters::default();
    let explicit =
        |y: &str, start: &str, end: &str| input(EquationMode::Explicit, &[("y", y)], start, end);
    let message = refusal(&mut params, &explicit("x^", "0", "1"));
    assert!(message.contains("the formula y does not read"), "{message}");
    let message = refusal(&mut params, &explicit("x * k", "0", "1"));
    assert!(message.contains("k is not a parameter"), "{message}");
    let message = refusal(&mut params, &explicit("sqrt(x)", "-1", "1"));
    assert!(
        message.contains("cannot be evaluated at x = -1"),
        "{message}"
    );
    let message = refusal(&mut params, &explicit("x", "5", "5"));
    assert!(message.contains("empty or reversed"), "{message}");
    let message = refusal(&mut params, &explicit("x", "5", "1"));
    assert!(message.contains("empty or reversed"), "{message}");
    let message = refusal(&mut params, &explicit("", "0", "1"));
    assert!(message.contains("needs the formula y"), "{message}");
    let message = refusal(
        &mut params,
        &EquationInput {
            tolerance: Some(0.0),
            ..explicit("x", "0", "1")
        },
    );
    assert!(message.contains("tolerance"), "{message}");
    let message = refusal(
        &mut params,
        &input(EquationMode::Parametric, &[("x", "t")], "0", "1"),
    );
    assert!(message.contains("needs the formula y"), "{message}");
    // A figure eight crosses itself: refused as a profile curve, a warning
    // as construction geometry.
    let eight = input(
        EquationMode::Parametric,
        &[("x", "10*sin(t)"), ("y", "10*sin(t)*cos(t)")],
        "0.1",
        "0.1 + 2*PI",
    );
    let message = refusal(&mut params, &eight);
    assert!(message.contains("crosses itself"), "{message}");
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&EquationInput {
            construction: Some(true),
            ..eight
        })
        .unwrap();
    assert!(made.closed);
    assert!(
        edit.report
            .warnings
            .iter()
            .any(|w| w.contains("crosses itself")),
        "{:?}",
        edit.report.warnings
    );
    assert!(edit.def.entity(made.curve).unwrap().construction);
    // An edit of a curve that has no formulas.
    let line = {
        let a = edit.add_point([0.0, 0.0]);
        let b = edit.add_point([1.0, 0.0]);
        edit.add_curve(EntityKind::Line {
            start: a,
            end: b,
            centerline: false,
        })
    };
    let message = edit
        .edit_equation_curve(line, &EquationInput::default())
        .unwrap_err();
    assert!(message.contains("not an equation curve"), "{message}");
}

/// A formula that fails after a parameter change keeps the curve's last
/// shape with a warning, and a rename rewrites the formulas.
#[test]
fn renames_and_failing_formulas() {
    let mut params = Parameters::default();
    let r = user(&mut params, "r", "5 mm", Unit::MM);
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&input(
            EquationMode::Polar,
            &[("r", "r + t")],
            "0",
            "r / 1 mm * 10",
        ))
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    let mut q = def.equations[0].clone();
    assert!(q.rename_parameter("r", "radius"));
    assert_eq!(
        (q.r.as_str(), q.end.as_str()),
        ("radius + t", "radius / 1 mm * 10")
    );
    // The variable is not renamed inside the formulas.
    let mut p = def.equations[0].clone();
    assert!(!p.rename_parameter("t", "u"));
    // r = 0 makes the range empty: the curve keeps its last shape.
    params.set_expression(r, "0 mm").unwrap();
    let mut followed = def.clone();
    let warnings = follow_formulas(&mut followed, params.table());
    assert!(
        warnings.iter().any(|w| w.contains("keeps its last shape")),
        "{warnings:?}"
    );
    assert_eq!(followed.entities, def.entities);
    assert_eq!(followed.equations.len(), 1);
    let _ = made;
}

#[test]
fn file_form_round_trip() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    edit.equation_curve(&input(EquationMode::Polar, &[("r", "10 + t")], "0", "90"))
        .unwrap();
    let (def, _) = edit.finish().unwrap();
    // The file form names parameters (none here).
    let named: SketchDef<String> = def
        .map_params(&mut |_, id| Ok::<String, String>(params.name(*id)))
        .unwrap();
    let text = serde_json::to_string(&named).unwrap();
    assert!(text.contains(r#""equations":[{"curve":"c"#), "{text}");
    assert!(
        text.contains(r#""mode":"polar","r":"10 + t","start":"0","end":"90","tolerance":0.01"#),
        "{text}"
    );
    let back: SketchDef<String> = serde_json::from_str(&text).unwrap();
    assert_eq!(back, named);
    assert_eq!(back.equations, def.equations);
    back.check_definition().unwrap();
    // A record whose curve is no fit point spline is refused.
    let mut wrong = def.clone();
    wrong.equations[0].curve = def.entities[0].id;
    let message = wrong.check_definition().unwrap_err();
    assert!(message.contains("not a fit point spline"), "{message}");
}

/// Numbers of the range are millimetres and radians, as commands give
/// values, kept as expressions in the design's units.
#[test]
fn numbers_of_the_range() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&EquationInput {
            mode: Some(EquationMode::Polar),
            r: Some("10".to_owned()),
            start: Some(ValueInput::Number(0.0)),
            end: Some(ValueInput::Number(PI)),
            ..EquationInput::default()
        })
        .unwrap();
    assert_eq!(
        (
            edit.def.equations[0].start.as_str(),
            edit.def.equations[0].end.as_str()
        ),
        ("0 deg", "180 deg")
    );
    // A half circle of radius 10 mm: a plain number is in millimetres.
    let end = at(&edit.def, *made.points.last().unwrap());
    assert!(dist(end, [-10.0, 0.0]) < 1e-9, "{end:?}");
    edit.edit_equation_curve(
        made.curve,
        &EquationInput {
            mode: Some(EquationMode::Explicit),
            y: Some("0".to_owned()),
            start: Some(ValueInput::Number(-10.0)),
            end: Some(ValueInput::Number(5.0)),
            ..EquationInput::default()
        },
    )
    .unwrap();
    let q = &edit.def.equations[0];
    assert_eq!(
        (q.start.as_str(), q.end.as_str(), q.r.as_str(), q.y.as_str()),
        ("-10 mm", "5 mm", "", "0")
    );
}

/// A variable with a unit (mitcad#2264): a length or an angle inside the
/// formulas and in the range, following its parameters, kept in the
/// record and refused where the mode cannot take it.
#[test]
fn a_variable_with_a_unit() {
    let mut params = Parameters::default();
    let a = user(&mut params, "a", "2 mm", Unit::MM);
    let length = |formulas: &[(&str, &str)], start: &str, end: &str| EquationInput {
        variable_unit: Some(VariableUnit::Length),
        ..input(EquationMode::Parametric, formulas, start, end)
    };
    // t a length: t^2 / (10 mm) is a length, and a plain t would refuse it.
    let lifted = length(&[("x", "t"), ("y", "a + t^2 / (10 mm)")], "0 mm", "10 mm");
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit.equation_curve(&lifted).unwrap();
    assert!(!made.closed && !made.capped);
    assert!(made.deviation <= DEFAULT_TOLERANCE, "{}", made.deviation);
    let (def, report) = edit.finish().unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let lift = |a: f64, t: f64| [t, a + t * t / 10.0];
    for p in &made.points {
        let [x, y] = at(&def, *p);
        assert!(dist([x, y], lift(2.0, x)) < 1e-12, "{x} {y}");
    }
    assert!(dist(at(&def, made.points[0]), [0.0, 2.0]) < 1e-12);
    let end = at(&def, *made.points.last().unwrap());
    assert!(dist(end, [10.0, 12.0]) < 1e-12, "{end:?}");
    let curve = solved_curve(&def, &params, made.curve);
    for i in 0..=200 {
        let t = 10.0 * f64::from(i) / 200.0;
        let away = curve.closest(lift(2.0, t)).1;
        assert!(away <= DEFAULT_TOLERANCE, "t {t}: {away} mm away");
    }
    let q = &def.equations[0];
    assert_eq!(
        (q.variable_unit, q.start.as_str(), q.end.as_str()),
        (VariableUnit::Length, "0 mm", "10 mm")
    );
    // The record names the unit; a plain one leaves it out.
    let text = serde_json::to_string(q).unwrap();
    assert!(text.contains(r#""variable_unit":"length""#), "{text}");
    let back: EquationCurve = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, q);
    let mut plain = q.clone();
    plain.variable_unit = VariableUnit::Plain;
    let text = serde_json::to_string(&plain).unwrap();
    assert!(!text.contains("variable_unit"), "{text}");
    // a changes: the points follow, as many as before.
    params.set_expression(a, "5 mm").unwrap();
    let mut followed = def.clone();
    let warnings = follow_formulas(&mut followed, params.table());
    assert!(warnings.is_empty(), "{warnings:?}");
    let (points, _) = spline(&followed, made.curve);
    assert_eq!(points.len(), made.points.len());
    for p in &points {
        let [x, y] = at(&followed, *p);
        assert!(dist([x, y], lift(5.0, x)) < 1e-12, "{x} {y}");
    }
    params.set_expression(a, "2 mm").unwrap();
    // The same formulas with a plain t give no length.
    let message = refusal(
        &mut params,
        &EquationInput {
            variable_unit: None,
            ..lifted.clone()
        },
    );
    assert!(message.contains("start"), "{message}");
    let message = refusal(
        &mut params,
        &input(
            EquationMode::Parametric,
            &[("x", "t"), ("y", "t^2 / (10 mm)")],
            "0",
            "10",
        ),
    );
    assert!(message.contains("with units"), "{message}");
    // A formula failing says where, with the unit.
    let message = refusal(
        &mut params,
        &length(&[("x", "t"), ("y", "(1 mm) * (1 mm) / t")], "0 mm", "1 mm"),
    );
    assert!(message.contains("at t = 0 mm"), "{message}");
    // Numbers of the range are millimetres.
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    edit.equation_curve(&EquationInput {
        start: Some(ValueInput::Number(-5.0)),
        end: Some(ValueInput::Number(5.0)),
        ..length(&[("x", "t"), ("y", "t / 2")], "", "")
    })
    .unwrap();
    let q = &edit.def.equations[0];
    assert_eq!((q.start.as_str(), q.end.as_str()), ("-5 mm", "5 mm"));
    // An explicit curve's x a length.
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(1), "Sketch1");
    let made = edit
        .equation_curve(&EquationInput {
            variable_unit: Some(VariableUnit::Length),
            ..input(
                EquationMode::Explicit,
                &[("y", "x^2 / (20 mm)")],
                "-10 mm",
                "10 mm",
            )
        })
        .unwrap();
    for p in &made.points {
        let [x, y] = at(&edit.def, *p);
        assert!((y - x * x / 20.0).abs() < 1e-12);
    }
    // t an angle: a quarter circle, the range in degrees, numbers radians.
    let made = edit
        .equation_curve(&EquationInput {
            variable_unit: Some(VariableUnit::Angle),
            start: Some(ValueInput::Number(0.0)),
            end: Some(ValueInput::Number(PI / 2.0)),
            ..input(
                EquationMode::Parametric,
                &[("x", "10 mm * cos(t)"), ("y", "10 mm * sin(t)")],
                "",
                "",
            )
        })
        .unwrap();
    let q = edit.def.equations.last().unwrap();
    assert_eq!((q.start.as_str(), q.end.as_str()), ("0 deg", "90 deg"));
    let end = at(&edit.def, *made.points.last().unwrap());
    assert!(dist(end, [0.0, 10.0]) < 1e-9, "{end:?}");
    // A polar curve's t an angle as it is.
    let made = edit
        .equation_curve(&EquationInput {
            variable_unit: Some(VariableUnit::Angle),
            ..input(EquationMode::Polar, &[("r", "10")], "0 deg", "180 deg")
        })
        .unwrap();
    let end = at(&edit.def, *made.points.last().unwrap());
    assert!(dist(end, [-10.0, 0.0]) < 1e-9, "{end:?}");
    // Refused where the mode cannot take it.
    let message = refusal(
        &mut params,
        &EquationInput {
            variable_unit: Some(VariableUnit::Angle),
            ..input(EquationMode::Explicit, &[("y", "x")], "0", "1")
        },
    );
    assert!(message.contains("not an angle"), "{message}");
    let message = refusal(
        &mut params,
        &EquationInput {
            variable_unit: Some(VariableUnit::Length),
            ..input(EquationMode::Polar, &[("r", "10")], "0", "1")
        },
    );
    assert!(message.contains("not a length"), "{message}");
}
