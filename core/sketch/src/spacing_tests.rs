// SPDX-License-Identifier: MIT
//! Equal spacing (mitcad#221): its definition in files, the anchors it
//! takes, the order along a direction and the solve.

use super::solve::{DimensionInput, solve};
use super::spacing::{AnchorPoints, anchor_points, check, sorted_along};
use super::*;

fn p(n: u32) -> Ref {
    Ref::Point(EntityUid(n))
}

fn c(n: u32) -> Ref {
    Ref::Curve(EntityUid(n))
}

/// p1 (0, 0); line c2 from p3 (8, -4) to p4 (12, 4); circle c5 around p6
/// (26, 3); ellipse c7 around p8 with p9 its major end; line c10 from p11
/// to p12, a direction.
fn entities() -> Vec<Entity> {
    vec![
        Entity::point(EntityUid(1), [0.0, 0.0]),
        Entity::new(
            EntityUid(2),
            EntityKind::Line {
                start: EntityUid(3),
                end: EntityUid(4),
                centerline: false,
            },
        ),
        Entity::point(EntityUid(3), [8.0, -4.0]),
        Entity::point(EntityUid(4), [12.0, 4.0]),
        Entity::new(
            EntityUid(5),
            EntityKind::Circle {
                center: EntityUid(6),
                radius: 2.0,
                start: 0.0,
            },
        ),
        Entity::point(EntityUid(6), [26.0, 3.0]),
        Entity::new(
            EntityUid(7),
            EntityKind::Ellipse {
                center: EntityUid(8),
                major: EntityUid(9),
                minor_radius: 1.0,
            },
        ),
        Entity::point(EntityUid(8), [40.0, 0.0]),
        Entity::point(EntityUid(9), [43.0, 0.0]),
        Entity::new(
            EntityUid(10),
            EntityKind::Line {
                start: EntityUid(11),
                end: EntityUid(12),
                centerline: false,
            },
        ),
        Entity::point(EntityUid(11), [0.0, -20.0]),
        Entity::point(EntityUid(12), [3.0, -16.0]),
    ]
}

#[test]
fn the_constraint_and_its_dimension_read_and_write_their_file_form() {
    let text = r#"{"id": "k3", "type": "equal_spacing", "anchors": ["p1", "c2", "c5"],
                   "mode": "along", "direction": "c10"}"#;
    let k: Constraint = serde_json::from_str(text).unwrap();
    assert_eq!(
        k.kind,
        ConstraintKind::EqualSpacing {
            anchors: vec![p(1), c(2), c(5)],
            mode: SpacingMode::Along,
            direction: Some(EntityUid(10)),
        }
    );
    assert_eq!(k.kind.refs(), vec![p(1), c(2), c(5), c(10)]);
    assert_eq!(k.kind.type_name(), "equal_spacing");
    let back = serde_json::to_value(&k).unwrap();
    assert_eq!(
        back,
        serde_json::json!({"id": "k3", "type": "equal_spacing", "anchors": ["p1", "c2", "c5"],
                           "mode": "along", "direction": "c10"})
    );
    // Without a direction the field is left out.
    let free: Constraint = serde_json::from_str(
        r#"{"id": "k4", "type": "equal_spacing", "anchors": ["p1", "c2", "c5"],
            "mode": "point_to_point"}"#,
    )
    .unwrap();
    assert!(
        serde_json::to_string(&free)
            .unwrap()
            .contains("\"mode\":\"point_to_point\"")
    );
    assert!(!serde_json::to_string(&free).unwrap().contains("direction"));
    let d: Dimension<String> = serde_json::from_str(
        r#"{"id": "k6", "type": "spacing", "constraint": "k3", "value": "d2"}"#,
    )
    .unwrap();
    assert_eq!(
        d.kind,
        DimensionKind::Spacing {
            constraint: ConstraintUid(3)
        }
    );
    assert_eq!(d.slot(), "dimensions[k6].spacing");
    assert!(d.kind.refs().is_empty());
    assert_eq!(d.kind.range(), ValueRange::Positive);
}

#[test]
fn anchors_are_points_centres_and_line_midpoints_only() {
    let e = entities();
    let index = EntityIndex::new(&e);
    assert_eq!(
        anchor_points(&index, p(1)),
        Some(AnchorPoints::Point(EntityUid(1)))
    );
    assert_eq!(
        anchor_points(&index, c(2)),
        Some(AnchorPoints::Mid(EntityUid(3), EntityUid(4)))
    );
    assert_eq!(
        anchor_points(&index, c(5)),
        Some(AnchorPoints::Point(EntityUid(6)))
    );
    assert_eq!(anchor_points(&index, c(7)), None, "no center of an ellipse");
    assert_eq!(anchor_points(&index, c(1)), None, "p1 is no curve");
    let what = "k3";
    let mode = SpacingMode::Horizontal;
    assert!(check(&index, what, &[p(1), c(2), c(5)], mode, None).is_ok());
    let err = |anchors: &[Ref], mode, direction| {
        check(&index, what, anchors, mode, direction).unwrap_err()
    };
    assert!(err(&[p(1), c(2)], mode, None).contains("at least three"));
    assert!(err(&[p(1), c(2), c(7)], mode, None).contains("is a ellipse"));
    assert!(err(&[p(1), c(2), p(1)], mode, None).contains("listed twice"));
    assert!(err(&[p(1), c(5), p(6)], mode, None).contains("c5 and p6 stand for the same point"));
    assert!(err(&[p(1), c(2), c(5)], SpacingMode::Along, None).contains("needs a direction"));
    assert!(
        err(&[p(1), c(2), c(5)], SpacingMode::Along, Some(EntityUid(5))).contains("not a line")
    );
    assert!(
        err(
            &[p(1), c(2), c(10)],
            SpacingMode::Along,
            Some(EntityUid(10))
        )
        .contains("both an anchor and the direction")
    );
    assert!(err(&[p(1), c(2), c(5)], mode, Some(EntityUid(10))).contains("only an along"));
    assert!(
        check(
            &index,
            what,
            &[p(1), c(2), c(5)],
            SpacingMode::Along,
            Some(EntityUid(10))
        )
        .is_ok()
    );
}

#[test]
fn sorting_along_a_direction_names_ties() {
    assert_eq!(
        sorted_along(&[(p(1), 20.0), (c(2), -5.0), (c(5), 7.5)]),
        Ok(vec![c(2), c(5), p(1)])
    );
    // Two at the same place: named in the order given.
    assert_eq!(
        sorted_along(&[(c(5), 7.5), (p(1), 20.0), (c(2), 7.5 + 1e-9)]),
        Err((c(5), c(2)))
    );
}

#[test]
fn a_spacing_dimension_drives_the_mixed_anchors_without_aligning_them() {
    let mut e = entities();
    e[0].fixed = true; // p1
    let constraints = [Constraint::new(
        ConstraintUid(1),
        ConstraintKind::EqualSpacing {
            anchors: vec![p(1), c(2), c(5)],
            mode: SpacingMode::Horizontal,
            direction: None,
        },
    )];
    let spacing = DimensionKind::Spacing {
        constraint: ConstraintUid(1),
    };
    let dims = [DimensionInput {
        id: ConstraintUid(2),
        kind: &spacing,
        value: Some(15.0),
    }];
    let solved = solve(&e, &constraints, &dims).unwrap();
    let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let line_mid = mid(solved.points[&EntityUid(3)], solved.points[&EntityUid(4)]);
    assert!((line_mid[0] - 15.0).abs() < 1e-9, "{line_mid:?}");
    assert!((solved.points[&EntityUid(6)][0] - 30.0).abs() < 1e-9);
    // Not aligned: the y coordinates stay.
    assert!((solved.points[&EntityUid(6)][1] - 3.0).abs() < 1e-9);
    assert!(line_mid[1].abs() < 1e-9);
    // A driven spacing measures the first interval.
    let driven = [DimensionInput {
        id: ConstraintUid(2),
        kind: &spacing,
        value: None,
    }];
    let solved = solve(&e, &constraints, &driven).unwrap();
    let measured = solved.status.driven[&ConstraintUid(2)];
    let line_mid = mid(solved.points[&EntityUid(3)], solved.points[&EntityUid(4)]);
    assert!(
        (measured - line_mid[0]).abs() < 1e-9,
        "{measured} vs {line_mid:?}"
    );
}
