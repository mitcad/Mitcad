// SPDX-License-Identifier: MIT
//! The allowance methods of sheet rules (mitcad#1939): every method gives a
//! fold an effective K-factor; a crease table picks its row; a kerf bend
//! keeps to its lattice's minimum radius; a fold's own allowance overrides
//! the rules'.

use std::f64::consts::FRAC_PI_2;

use super::*;

fn rules(allowance: serde_json::Value) -> SheetRulesDef<f64> {
    serde_json::from_value(serde_json::json!({
        "thickness": 2.0, "bend_radius": 0.5, "allowance": allowance,
        "reference_side": "front",
        "bend_relief": {"type": "round", "width": 1.0, "depth": 2.0},
        "corner_relief": {"type": "round", "width": 1.0, "depth": 2.0},
    }))
    .expect("rules")
}

/// The developed length of a 90 degree fold with the effective K.
fn developed(rules: &SheetRulesDef<f64>, angle: f64) -> f64 {
    let fold = rules.fold_allowance(angle, None).expect("allowance");
    angle.abs() * (fold.radius + fold.k * rules.thickness)
}

#[test]
fn every_method_gives_the_fold_its_measured_length() {
    let k = rules(serde_json::json!({"type": "k_factor", "k": 0.44}));
    assert!((developed(&k, FRAC_PI_2) - FRAC_PI_2 * (0.5 + 0.88)).abs() < 1e-12);
    let a = rules(serde_json::json!({"type": "bend_allowance", "allowance": 3.5}));
    assert!((developed(&a, FRAC_PI_2) - 3.5).abs() < 1e-12);
    // Other angles in proportion: the same neutral radius.
    assert!((developed(&a, FRAC_PI_2 / 2.0) - 1.75).abs() < 1e-12);
    // A deduction of 1.5: 2 (r + t) - 1.5 = 3.5.
    let d = rules(serde_json::json!({"type": "bend_deduction", "deduction": 1.5}));
    assert!((developed(&d, -FRAC_PI_2) - 3.5).abs() < 1e-12);
}

#[test]
fn a_nearly_sharp_crease_needs_more_than_k_one() {
    // A deduction close to 0: the allowance is almost twice the outside
    // setback, which no K in [0, 1] gives.
    let d = rules(serde_json::json!({"type": "bend_deduction", "deduction": 0.2}));
    let fold = d.fold_allowance(FRAC_PI_2, None).unwrap();
    assert!(fold.k > 1.0, "{}", fold.k);
    assert!((developed(&d, FRAC_PI_2) - 4.8).abs() < 1e-12);
    // A deduction larger than the fold develops is refused in words.
    let too_much = rules(serde_json::json!({"type": "bend_deduction", "deduction": 6.0}));
    let error = too_much.fold_allowance(FRAC_PI_2, None).unwrap_err();
    assert!(error.contains("must develop to more than 0 mm"), "{error}");
    assert!(too_much.validate().is_ok());
}

#[test]
fn the_crease_table_row_is_chosen_by_thickness_flute_direction_and_side() {
    let table = serde_json::json!({"type": "crease_table", "rows": [
        {"thickness": 4.0, "deduction": 3.0},
        {"thickness": 2.0, "flute": "C", "direction": "across", "deduction": 1.5},
        {"thickness": 2.0, "flute": "C", "direction": "along", "deduction": 1.0},
        {"thickness": 2.0, "side": "inner", "allowance": 4.0},
    ]});
    let mut r = rules(table);
    r.flute = Some("C".into());
    // No direction given: the rows of any direction, the nearest thickness,
    // the most specific: the outer fold takes no `inner` row, so the
    // row of thickness 4 is farther; none of 2 mm matches any direction
    // but the inner one.
    let error = r.fold_allowance(FRAC_PI_2, None);
    assert!(error.is_ok(), "{error:?}");
    assert!((developed(&r, FRAC_PI_2) - (5.0 - 3.0)).abs() < 1e-12);
    // A fold closing towards the front (negative angle) takes the inner row.
    assert!((developed(&r, -FRAC_PI_2) - 4.0).abs() < 1e-12);
    // A fold's own allowance picks the rules' rows in its direction.
    let across: Allowance<f64> =
        serde_json::from_value(serde_json::json!({"type": "crease_table", "direction": "across"}))
            .unwrap();
    let own = r.with_fold_allowance(Some(&across));
    assert!((developed(&own, FRAC_PI_2) - 3.5).abs() < 1e-12);
    let along: Allowance<f64> =
        serde_json::from_value(serde_json::json!({"type": "crease_table", "direction": "along"}))
            .unwrap();
    let own = r.with_fold_allowance(Some(&along));
    assert!((developed(&own, FRAC_PI_2) - 4.0).abs() < 1e-12);
    // A table with no row for the fold says so in words.
    let mut e = rules(serde_json::json!({"type": "crease_table", "rows": [
        {"thickness": 2.0, "flute": "E", "deduction": 1.0}]}));
    e.flute = Some("B".into());
    let error = e.fold_allowance(FRAC_PI_2, None).unwrap_err();
    assert!(
        error.starts_with("the crease table has no row for a fold of 2 mm board of flute B"),
        "{error}"
    );
}

#[test]
fn a_fold_may_override_the_rules_with_any_method() {
    let k = rules(serde_json::json!({"type": "k_factor", "k": 0.44}));
    let own: Allowance<f64> =
        serde_json::from_value(serde_json::json!({"type": "bend_allowance", "allowance": 3.0}))
            .unwrap();
    assert!((developed(&k.with_fold_allowance(Some(&own)), FRAC_PI_2) - 3.0).abs() < 1e-12);
    assert_eq!(k.with_fold_allowance(None), k);
}

#[test]
fn a_kerf_bend_keeps_to_its_lattice() {
    let mut r = rules(serde_json::json!({"type": "kerf_bend", "slit_length": 20.0,
                                        "gap": 3.0, "pitch": 1.5}));
    // No slit width and no kerf: refused.
    assert!(
        r.fold_allowance(FRAC_PI_2, None)
            .unwrap_err()
            .contains("slit width")
    );
    r.kerf = Some(0.2);
    // r >= t p / (2 w) - t / 2 = 2 1.5 / 0.4 - 1 = 6.5.
    let fold = r.fold_allowance(FRAC_PI_2, None).unwrap();
    assert_eq!(fold.k, 0.5);
    assert!((fold.radius - 6.5).abs() < 1e-12, "{}", fold.radius);
    let lattice = fold.lattice.unwrap();
    assert_eq!(lattice.slit_width, 0.2);
    assert!(r.fold_allowance(FRAC_PI_2, Some(10.0)).is_ok());
    let error = r.fold_allowance(FRAC_PI_2, Some(5.0)).unwrap_err();
    assert!(
        error.contains("smaller than its slit lattice bends to (6.5 mm)"),
        "{error}"
    );
    // The rows lie across the developed strip, the slits along the axis
    // inside the material, every other row shifted by half a period.
    let rows = lattice.rows(10.0);
    assert_eq!(rows.len(), 6);
    assert!((rows[0] - 1.25).abs() < 1e-12);
    let even = lattice.row(0, &[[0.0, 50.0]]);
    assert_eq!(even, vec![[3.0, 23.0], [26.0, 46.0]]);
    let odd = lattice.row(1, &[[0.0, 50.0]]);
    assert_eq!(odd, vec![[3.0, 11.5], [14.5, 34.5], [37.5, 47.0]]);
}

#[test]
fn the_rules_check_their_crease_table_and_kerf() {
    let empty = rules(serde_json::json!({"type": "crease_table"}));
    assert!(empty.validate().unwrap_err().contains("no rows"));
    let both = rules(serde_json::json!({"type": "crease_table", "rows": [
        {"thickness": 2.0, "allowance": 3.0, "deduction": 1.0}]}));
    assert!(both.validate().unwrap_err().contains("not both"));
    let mut kerf = rules(serde_json::json!({"type": "k_factor", "k": 0.5}));
    kerf.kerf = Some(-0.1);
    assert!(kerf.validate().is_err());
    kerf.kerf = Some(0.0);
    assert!(kerf.validate().is_ok());
}
