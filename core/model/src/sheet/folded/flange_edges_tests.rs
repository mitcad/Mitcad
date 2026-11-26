// SPDX-License-Identifier: MIT
//! A flange on several edges across a recorded corner gap (mitcad#2098):
//! the corner of a converted tray's rim, two walls apart across a rip, on
//! a synthetic sheet with no kernel.

use super::*;
use std::f64::consts::FRAC_PI_2;

fn uid(text: &str) -> FeatureUid {
    text.parse().unwrap()
}

/// Rules of thickness 1 and bend radius 1.
fn rules() -> SheetRulesDef<f64> {
    serde_json::from_value(serde_json::json!({
        "thickness": 1, "bend_radius": 1,
        "allowance": {"type": "k_factor", "k": 0.44}, "reference_side": "front",
        "bend_relief": {"type": "round", "width": 1, "depth": 2},
        "corner_relief": {"type": "round", "width": 1, "depth": 2}
    }))
    .expect("rules")
}

fn panel(slot: u32) -> PanelId {
    PanelId {
        feature: uid("F3"),
        slot,
    }
}

fn curve(panel: u32, side: u32) -> SheetCurveId {
    SheetCurveId::Generated {
        feature: uid("F3"),
        slot: 10 * panel + side,
    }
}

/// Panel `slot`, its chart the rectangle `x0..x1` by `0..20` on `frame`;
/// its sides 1 (bottom), 2, 3 (top) and 4 run counter-clockwise.
fn rectangle(slot: u32, frame: SketchFrame, (x0, x1): (f64, f64)) -> SheetPatch {
    let corners = [[x0, 0.0], [x1, 0.0], [x1, 20.0], [x0, 20.0]];
    let region = MaterialRegion::from_legacy(&SheetRegion {
        loops: vec![SheetLoop {
            segments: (0..4)
                .map(|i| SheetSegment {
                    id: curve(slot, i as u32 + 1),
                    geometry: SegmentGeometry::Line {
                        start: corners[i],
                        end: corners[(i + 1) % 4],
                    },
                })
                .collect(),
        }],
    })
    .unwrap();
    SheetPatch {
        domain: SheetDomainRef::Panel(panel(slot)),
        regions: vec![region],
        embedding: SheetEmbedding::Plane {
            front_frame: frame,
            front_to_back_sign: MaterialParity::Positive,
        },
    }
}

/// A converted tray's corner: wall 0 in the plane y = 0 (x from 0 to
/// 39), wall 1 in the plane x = 40 (y from 1 to 30), 20 high, cut apart
/// at the rip on the line x = 40, y = 0 with a gap of 2 (as Convert to
/// Sheet Metal records it); wall 2 in the plane y = 50, beside neither.
/// `gap`: whether the rip is recorded (a sheet from before mitcad#2098
/// has none).
fn tray_corner(gap: bool) -> SheetState {
    let frame = |origin: V3, x_axis: V3| SketchFrame {
        origin,
        x_axis,
        y_axis: [0.0, 0.0, 1.0],
    };
    SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: BodyUid::new(uid("F3"), 0),
        owner: crate::ids::ComponentUid::ROOT,
        shape_generation: String::new(),
        rules: uid("F1"),
        rules_version: String::new(),
        panels: Vec::new(),
        charts: vec![
            rectangle(0, frame([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]), (0.0, 39.0)),
            rectangle(1, frame([40.0, 0.0, 0.0], [0.0, 1.0, 0.0]), (1.0, 30.0)),
            rectangle(2, frame([0.0, 50.0, 0.0], [-1.0, 0.0, 0.0]), (-39.0, 0.0)),
        ],
        bends: Vec::new(),
        joins: Vec::new(),
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        gaps: if gap {
            vec![crate::sheet::SheetGap {
                panels: [panel(0), panel(1)],
                line: [[40.0, 0.0, 0.0], [40.0, 0.0, 20.0]],
                width: 2.0,
            }]
        } else {
            Vec::new()
        },
        invalidated: None,
        blockers: Vec::new(),
    }
}

/// The top edge (the rim) of panel `slot`.
fn rim(slot: u32) -> BoundaryRef {
    BoundaryRef {
        panel: panel(slot),
        curve: curve(slot, 3),
        piece: None,
        range: None,
    }
}

fn close(a: V3, b: V3) -> bool {
    norm(sub(a, b)) < 1e-9
}

#[test]
fn rim_edges_across_a_rip_are_one_corner_at_the_rips_corner() {
    let state = tray_corner(true);
    let found = edge_corners(&state, 1.0, &[rim(0), rim(1)]).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, [0, 1]);
    // Where the two edges' lines come closest: the rip's top corner.
    assert!(close(found[0].1, [40.0, 0.0, 20.0]), "{found:?}");
    // In the other order too.
    let found = edge_corners(&state, 1.0, &[rim(1), rim(0)]).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(close(found[0].1, [40.0, 0.0, 20.0]), "{found:?}");
}

#[test]
fn without_a_recorded_gap_or_across_panels_that_are_no_neighbours_edges_are_refused() {
    // A sheet from before the gaps were recorded: the ends are 1.4 mm
    // apart, which is never read as a gap.
    let refusal = edge_corners(&tray_corner(false), 1.0, &[rim(0), rim(1)]).unwrap_err();
    assert!(
        refusal.contains("must be connected at their ends"),
        "{refusal}"
    );
    // Panels 0 and 2 have no gap between them; 1 and 2 neither.
    let state = tray_corner(true);
    for edges in [[rim(0), rim(2)], [rim(1), rim(2)]] {
        let refusal = edge_corners(&state, 1.0, &edges).unwrap_err();
        assert!(
            refusal.contains("must be connected at their ends"),
            "{refusal}"
        );
        assert!(!refusal.contains("F3"), "{refusal}");
    }
    // The gap's panels, but ends at different places along it: the bottom
    // of one wall and the rim of the other do not meet.
    let bottom = BoundaryRef {
        curve: curve(0, 1),
        ..rim(0)
    };
    assert!(edge_corners(&state, 1.0, &[bottom, rim(1)]).is_err());
}

#[test]
fn walls_across_a_rip_are_mitred_on_the_rips_bisecting_plane() {
    let state = tray_corner(true);
    let rules = rules();
    for position in [
        SheetPosition::BendLine,
        SheetPosition::Inside,
        SheetPosition::Outside,
    ] {
        for angle in [FRAC_PI_2, -FRAC_PI_2] {
            let metrics = BendMetrics::past_half_turn(angle, None, &rules).unwrap();
            let tangent = metrics.offset(position).unwrap();
            let geometry = [rim(0), rim(1)]
                .map(|edge| EdgeGeometry::new(&state, &edge, tangent, &metrics).unwrap());
            let ends = [rim(0), rim(1)].map(|edge| EdgeEnds::of(&state, &edge).unwrap());
            let found = corners(&state, 1.0, &ends).unwrap();
            assert_eq!(found.len(), 1);
            let c = &found[0];
            assert!(c.gap);
            let n = {
                let d = sub(geometry[0].away(c.ends[0]), geometry[1].away(c.ends[1]));
                scale(d, 1.0 / norm(d))
            };
            // The bisecting plane holds the rip's line: the walls are
            // mitred as at a touching corner, symmetric about the rip.
            assert!(dot(n, [0.0, 0.0, 1.0]).abs() < 1e-12);
            let point = mitre_point(&geometry[0], &geometry[1], corner_point(&ends, c));
            assert!(
                dot(n, sub(point, [40.0, 0.0, 7.0])).abs() < 1e-9,
                "{position:?} {angle}: {point:?}"
            );
            // The gap a flange records there: in the same plane, along the
            // walls (here lips in one plane: level, along the diagonal).
            let gap = corner_gap(uid("F5"), &geometry, &ends, c, (0.5, 20.0)).unwrap();
            assert_eq!(
                gap.panels,
                [0, 1].map(|slot| PanelId {
                    feature: uid("F5"),
                    slot
                })
            );
            let along = sub(gap.line[1], gap.line[0]);
            assert!((norm(along) - 20.0).abs() < 1e-9);
            assert!(dot(n, along).abs() < 1e-9);
            assert!(dot(n, sub(gap.line[0], [40.0, 0.0, 7.0])).abs() < 1e-9);
            assert!(dot(along, geometry[0].wall_x) > 0.0);
            assert_eq!(gap.width, 0.5);
        }
    }
}

#[test]
fn a_sheet_saved_without_gaps_still_loads_and_gaps_are_kept() {
    let without = tray_corner(false);
    let text = serde_json::to_value(&without).unwrap();
    assert!(text.get("gaps").is_none(), "{text}");
    let read: SheetState = serde_json::from_value(text).unwrap();
    assert!(read.gaps.is_empty());
    let with = tray_corner(true);
    let read: SheetState = serde_json::from_value(serde_json::to_value(&with).unwrap()).unwrap();
    assert_eq!(read.gaps, with.gaps);
    // Moved with the sheet.
    let moved = with.transformed(&crate::transform::Transform::translation([0.0, 0.0, 5.0]));
    assert!(close(moved.gaps[0].line[1], [40.0, 0.0, 25.0]));
}

/// Where the bend of a flange's edge ends at a corner the rules' corner
/// relief opens (mitcad#2411): the edge y = 0 of a face reaching to
/// x = 40 at the corner, Inside, t = 1, r = 1 (its tangent line at y = 2,
/// its wall at y = 0..1 from z = 2 up), the next edge's tangent line at
/// x = 38 (J0 = (38, 2)), the corner's bisecting plane x + y = 40.
#[test]
fn a_bend_at_an_opened_corner_runs_to_the_other_tangent_line_as_its_wall_covers_it() {
    let bend = |angle: f64| CylinderEmbedding {
        centre: [0.0, 2.0, 2.0],
        axis: [1.0, 0.0, 0.0],
        radial_start: [0.0, 0.0, -1.0],
        angular_direction: [0.0, -1.0, 0.0],
        inside_radius: 1.0,
        neutral_radius: 1.44,
        angle_extent: angle,
        inner_is_front: false,
    };
    let upper = |limit: Limit| match limit {
        Limit::Upper(v) => v,
        _ => panic!("not an upper limit"),
    };
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let n = [-half, -half, 0.0];
    let mitre = |gap: f64| Keep {
        normal: n,
        offset: dot(n, [39.5, 0.5, 0.0]) + gap / 2.0,
    };
    let tangent = Keep {
        normal: [-1.0, 0.0, 0.0],
        offset: -38.0,
    };
    let c = bend(FRAC_PI_2);
    // Gap 1: the bisecting plane ends the bend g/sqrt2 short of J0, its
    // wall's root reaches x = 39 - g/sqrt2; opened, the bend runs to J0.
    let open = Opened {
        mitre: mitre(1.0),
        tangent,
    };
    assert!((upper(bend_limit(&c, 1.0, &open.mitre)) - (38.0 - half)).abs() < 1e-12);
    assert!((upper(root_limit(&c, 1.0, &open.mitre)) - (39.0 - half)).abs() < 1e-12);
    let reached = upper(opened_limit(&c, 1.0, &open, &[open.mitre]));
    assert!((reached - 38.0).abs() < 1e-12, "{reached}");
    // Gap 3: the wall's root ends short of J0, and so does the bend.
    let open = Opened {
        mitre: mitre(3.0),
        tangent,
    };
    let reached = upper(opened_limit(&c, 1.0, &open, &[open.mitre]));
    assert!((reached - (39.0 - 3.0 * half)).abs() < 1e-12, "{reached}");
    // Past a half turn the bend keeps the bisecting plane's limit.
    let past = bend(1.2 * std::f64::consts::PI);
    let kept = upper(opened_limit(&past, 1.0, &open, &[open.mitre]));
    assert!((kept - upper(bend_limit(&past, 1.0, &open.mitre))).abs() < 1e-12);
}

/// A tray's long wall and an end wall beside the end of its top edge
/// (mitcad#2445): wall 0 of [`tray_corner`] (in y = -1..0, x from 0 to
/// 39, 20 high) and wall 1 in the slab x = `at`..`at + 1`, from y = 2 to
/// 30, as high.
fn lip_tray(at: f64) -> SheetState {
    let mut state = tray_corner(false);
    state.charts.truncate(1);
    state.charts.push(rectangle(
        1,
        SketchFrame {
            origin: [at, 0.0, 0.0],
            x_axis: [0.0, 1.0, 0.0],
            y_axis: [0.0, 0.0, 1.0],
        },
        (2.0, 30.0),
    ));
    state
}

#[test]
fn a_lip_keeps_the_gap_short_of_a_wall_beside_its_end() {
    let rules = rules();
    // A lip of 9 on the long wall's top edge, Bend Line, folded either
    // way: its wall level at the top, 1 to 10 off the long wall on its
    // side.
    let keeps = |state: &SheetState| -> Vec<Vec<Keep>> {
        [FRAC_PI_2, -FRAC_PI_2]
            .map(|angle| {
                let metrics = BendMetrics::past_half_turn(angle, None, &rules).unwrap();
                let tangent = metrics.offset(SheetPosition::BendLine).unwrap();
                let edge = EdgeGeometry::new(state, &rim(0), tangent, &metrics).unwrap();
                [End::Start, End::End]
                    .into_iter()
                    .flat_map(|end| {
                        walls_beside(state, uid("F9"), panel(0), 1.0, &edge, end, 1.0, 9.0)
                    })
                    .map(|(keep, _, _)| keep)
                    .collect()
            })
            .into_iter()
            .collect()
    };
    // The end wall stands 0.5 to 1.5 past the edge's end at x = 0: folded
    // over it the lip would pass into its top; it keeps the gap short of
    // its surface x = 1.5 (x >= 2.5). Folded the other way, it is clear.
    let found = keeps(&lip_tray(0.5));
    assert_eq!(
        found.iter().filter(|k| !k.is_empty()).count(),
        1,
        "{found:?}"
    );
    let all: Vec<&Keep> = found.iter().flatten().collect();
    assert_eq!(all.len(), 1, "{found:?}");
    assert!(norm(sub(all[0].normal, [1.0, 0.0, 0.0])) < 1e-9, "{all:?}");
    assert!((all[0].offset - 2.5).abs() < 1e-9, "{all:?}");
    // An end wall already clear of the edge's end (x = -1.5..-0.5, 0.5
    // short of it), or across from neither end: no keep, the lip as it is.
    assert!(keeps(&lip_tray(-1.5)).iter().all(Vec::is_empty));
    assert!(keeps(&lip_tray(60.0)).iter().all(Vec::is_empty));
}
