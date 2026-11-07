// SPDX-License-Identifier: MIT
//! Graph development on hand-built charts with independently calculated
//! placements (mitcad#208). Real constructed solids are covered by the CLI
//! sheet fixtures; these tests pin the traversal and its refusals.

use super::*;
use crate::ids::ComponentUid;
use std::f64::consts::{FRAC_PI_2, PI};

const T: f64 = 2.0;
const R: f64 = 3.0;
const K: f64 = 0.3;
const W: f64 = 20.0;

fn near(a: V3, b: V3) -> bool {
    distance(a, b) < 1e-9
}

fn panel(feature: u64) -> PanelId {
    PanelId {
        feature: FeatureUid(feature),
        slot: 0,
    }
}

fn bend_id(feature: u64) -> BendId {
    BendId {
        feature: FeatureUid(feature),
        slot: 0,
    }
}

fn patch(domain: SheetDomainRef, u0: f64, u1: f64, embedding: SheetEmbedding) -> SheetPatch {
    SheetPatch {
        domain,
        regions: vec![rectangle(domain.feature(), 10 * domain.slot(), [u0, 0.0], [u1, W]).unwrap()],
        embedding,
    }
}

/// Edge 1 is the side `u = max`, edge 3 the side `u = min`.
fn port_side(patch: &SheetPatch, index: usize) -> ShapePortSide {
    let edge = &patch.regions[0].loops[0].edges[index];
    let (a, b) = edge.curve.ends().unwrap();
    ShapePortSide {
        boundary: BoundaryPieceRef {
            domain: patch.domain,
            piece: edge.piece.clone(),
        },
        span: edge.curve.range,
        axial: AxialMap {
            offset: a[1],
            scale: b[1] - a[1],
        },
    }
}

fn plane(origin: V3, x_axis: V3, y_axis: V3) -> SheetEmbedding {
    SheetEmbedding::Plane {
        front_frame: SketchFrame {
            origin,
            x_axis,
            y_axis,
        },
        front_to_back_sign: MaterialParity::Negative,
    }
}

fn cylinder(
    centre: V3,
    radial_start: V3,
    angular_direction: V3,
    radius: f64,
    inner_is_front: bool,
) -> CylinderEmbedding {
    CylinderEmbedding {
        centre,
        axis: [0.0, -1.0, 0.0],
        radial_start,
        angular_direction,
        inside_radius: radius,
        neutral_radius: radius + K * T,
        angle_extent: FRAC_PI_2,
        inner_is_front,
    }
}

fn record(feature: u64, c: &CylinderEmbedding, sign: f64) -> BendDomain {
    let side = BoundaryPieceRef {
        domain: SheetDomainRef::Panel(panel(feature)),
        piece: generated(FeatureUid(feature), 0),
    };
    BendDomain {
        id: bend_id(feature),
        stationary: side.clone(),
        moving: side,
        folded_embedding: c.clone(),
        signed_angle: sign * FRAC_PI_2,
        k_factor: K,
        radius: c.inside_radius,
        allowance: c.neutral_radius * FRAC_PI_2,
        axial_interval: ParamSpan {
            first: 0.0,
            last: W,
        },
        rules: FeatureUid(1),
        rules_version: String::new(),
    }
}

fn state(charts: Vec<SheetPatch>, bends: Vec<BendDomain>) -> SheetState {
    let joins = charts
        .windows(2)
        .enumerate()
        .map(|(i, pair)| ShapePort {
            token: i as u32,
            axial_interval: ParamSpan {
                first: 0.0,
                last: W,
            },
            first: port_side(&pair[0], 1),
            second: port_side(&pair[1], 3),
        })
        .collect();
    SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: BodyUid::new(FeatureUid(3), 0),
        owner: ComponentUid::ROOT,
        shape_generation: String::new(),
        rules: FeatureUid(1),
        rules_version: String::new(),
        panels: Vec::new(),
        charts,
        bends,
        joins,
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
    }
}

/// The contract's L: base x=[-30,0], z=[-2,0]; flange x=[3,5], z=[3,18].
fn l_sheet() -> SheetState {
    let c = cylinder([0.0, W, R], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], R, false);
    let ba = c.neutral_radius * FRAC_PI_2;
    state(
        vec![
            patch(
                SheetDomainRef::Panel(panel(3)),
                -30.0,
                0.0,
                plane([0.0, W, -T], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
            ),
            patch(
                SheetDomainRef::Bend(bend_id(4)),
                0.0,
                ba,
                SheetEmbedding::Cylinder(c.clone()),
            ),
            patch(
                SheetDomainRef::Panel(panel(4)),
                0.0,
                15.0,
                plane([R + T, W, R], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
            ),
        ],
        vec![record(4, &c, 1.0)],
    )
}

/// An opposite-sign Z: B1 (radius 3, outer front) then B2 (radius 4, inner front).
fn z_sheet() -> SheetState {
    let c1 = cylinder([0.0, W, R], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], R, false);
    let middle = [R + T, W, R];
    let r2 = 4.0;
    let centre2 = [R + T + r2, W, R + 15.0];
    let c2 = cylinder(centre2, [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], r2, true);
    let (a1, a2) = (c1.neutral_radius * FRAC_PI_2, c2.neutral_radius * FRAC_PI_2);
    state(
        vec![
            patch(
                SheetDomainRef::Panel(panel(3)),
                -30.0,
                0.0,
                plane([0.0, W, -T], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
            ),
            patch(
                SheetDomainRef::Bend(bend_id(4)),
                0.0,
                a1,
                SheetEmbedding::Cylinder(c1.clone()),
            ),
            patch(
                SheetDomainRef::Panel(panel(4)),
                0.0,
                15.0,
                plane(middle, [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
            ),
            patch(
                SheetDomainRef::Bend(bend_id(5)),
                0.0,
                a2,
                SheetEmbedding::Cylinder(c2.clone()),
            ),
            patch(
                SheetDomainRef::Panel(panel(5)),
                0.0,
                12.0,
                plane(
                    [centre2[0], W, centre2[2] + r2],
                    [1.0, 0.0, 0.0],
                    [0.0, -1.0, 0.0],
                ),
            ),
        ],
        vec![record(4, &c1, 1.0), record(5, &c2, -1.0)],
    )
}

fn embedding(state: &SheetState, domain: SheetDomainRef) -> &SheetEmbedding {
    &state
        .charts
        .iter()
        .find(|p| p.domain == domain)
        .unwrap()
        .embedding
}

fn open(bends: &[BendId], value: bool) -> BTreeMap<BendId, bool> {
    bends.iter().map(|b| (*b, value)).collect()
}

fn same_embeddings(a: &SheetState, b: &SheetState) {
    for (x, y) in a.charts.iter().zip(&b.charts) {
        assert_eq!(x.domain, y.domain);
        assert_eq!(x.regions, y.regions, "charts keep their material curves");
        match (&x.embedding, &y.embedding) {
            (
                SheetEmbedding::Plane {
                    front_frame: f,
                    front_to_back_sign: s,
                },
                SheetEmbedding::Plane {
                    front_frame: g,
                    front_to_back_sign: t,
                },
            ) => {
                assert!(near(f.origin, g.origin) && near(f.x_axis, g.x_axis));
                assert!(near(f.y_axis, g.y_axis));
                assert_eq!(s, t);
            }
            (SheetEmbedding::Cylinder(c), SheetEmbedding::Cylinder(d)) => {
                assert!(near(c.centre, d.centre) && near(c.axis, d.axis));
                assert!(near(c.radial_start, d.radial_start));
                assert!(near(c.angular_direction, d.angular_direction));
                assert_eq!(c.inner_is_front, d.inner_is_front);
                assert!((c.inside_radius - d.inside_radius).abs() < 1e-12);
            }
            other => panic!("configuration differs: {other:?}"),
        }
    }
}

#[test]
fn stationary_base_develops_the_neutral_strip_and_refolds_exactly() {
    let folded = l_sheet();
    let mut flat = folded.clone();
    reconfigure(&mut flat, T, panel(3), &open(&[bend_id(4)], true)).unwrap();
    let ba = 1.8 * PI;
    // The base stays; the strip continues its front surface.
    same_embeddings(
        &SheetState {
            charts: vec![flat.charts[0].clone()],
            ..flat.clone()
        },
        &SheetState {
            charts: vec![folded.charts[0].clone()],
            ..folded.clone()
        },
    );
    let strip = embedding(&flat, SheetDomainRef::Bend(bend_id(4)));
    assert!(near(chart_point(strip, [0.0, 0.0], 0.0, T), [0.0, W, -T]));
    assert!(near(chart_point(strip, [ba, W], T, T), [ba, 0.0, 0.0]));
    // The flange lies flat after the allowance: total length 45 + 1.8 pi.
    let flange = embedding(&flat, SheetDomainRef::Panel(panel(4)));
    assert!(near(
        chart_point(flange, [15.0, W], 0.0, T),
        [15.0 + ba, 0.0, -T]
    ));
    assert!(near(chart_point(flange, [0.0, 0.0], T, T), [ba, W, 0.0]));
    // Inverse correspondence across the developed strip.
    let (uv, depth) = chart_coordinates(strip, [ba / 2.0, 7.0, -1.0], T).unwrap();
    assert!((uv[0] - ba / 2.0).abs() < 1e-12 && (uv[1] - (W - 7.0)).abs() < 1e-12);
    assert!((depth - 1.0).abs() < 1e-12);
    // Refolding from the developed charts restores the folded configuration.
    let mut refolded = flat.clone();
    reconfigure(&mut refolded, T, panel(3), &open(&[bend_id(4)], false)).unwrap();
    same_embeddings(&refolded, &folded);
    assert_eq!(refolded.bends, folded.bends);
}

#[test]
fn folded_points_map_to_their_developed_positions() {
    let folded = l_sheet();
    let mut flat = folded.clone();
    reconfigure(&mut flat, T, panel(3), &open(&[bend_id(4)], true)).unwrap();
    let bend = SheetDomainRef::Bend(bend_id(4));
    // The middle of the bend: angle pi/4 on the neutral radius 3.6.
    let rn = R + K * T;
    let folded_point = [rn * (PI / 4.0).sin(), W - 5.0, R - rn * (PI / 4.0).cos()];
    let (uv, depth) = chart_coordinates(embedding(&folded, bend), folded_point, T).unwrap();
    assert!((uv[0] - rn * PI / 4.0).abs() < 1e-12 && (uv[1] - 5.0).abs() < 1e-12);
    // The outer (front) radius is 5: the neutral surface is 1.4 deep.
    assert!((depth - (R + T - rn)).abs() < 1e-12);
    let developed = chart_point(embedding(&flat, bend), uv, depth, T);
    assert!(near(developed, [rn * PI / 4.0, W - 5.0, -T + depth]));
}

#[test]
fn the_other_stationary_panel_keeps_the_flange_and_moves_the_base() {
    let folded = l_sheet();
    let mut flat = folded.clone();
    reconfigure(&mut flat, T, panel(4), &open(&[bend_id(4)], true)).unwrap();
    let ba = 1.8 * PI;
    let flange = embedding(&flat, SheetDomainRef::Panel(panel(4)));
    assert_eq!(flange, embedding(&folded, SheetDomainRef::Panel(panel(4))));
    let base = embedding(&flat, SheetDomainRef::Panel(panel(3)));
    // The developed sheet continues the flange's front plane x = 5 downwards.
    assert!(near(
        chart_point(base, [0.0, 0.0], 0.0, T),
        [R + T, W, R - ba]
    ));
    assert!(near(
        chart_point(base, [-30.0, W], T, T),
        [R, 0.0, R - ba - 30.0]
    ));
    let mut refolded = flat.clone();
    reconfigure(&mut refolded, T, panel(4), &open(&[bend_id(4)], false)).unwrap();
    same_embeddings(&refolded, &folded);
}

#[test]
fn partial_opening_carries_the_other_bend_with_its_radius_sign_and_parity() {
    let folded = z_sheet();
    let mut partial = folded.clone();
    reconfigure(&mut partial, T, panel(3), &open(&[bend_id(4)], true)).unwrap();
    let a1 = (R + K * T) * FRAC_PI_2;
    // The middle panel now lies flat after B1.
    let middle = embedding(&partial, SheetDomainRef::Panel(panel(4)));
    assert!(near(chart_point(middle, [0.0, 0.0], 0.0, T), [a1, W, -T]));
    let SheetEmbedding::Cylinder(b2) = embedding(&partial, SheetDomainRef::Bend(bend_id(5))) else {
        panic!("B2 stays folded");
    };
    let SheetEmbedding::Cylinder(before) = embedding(&folded, SheetDomainRef::Bend(bend_id(5)))
    else {
        panic!("B2 folded");
    };
    assert!(b2.inner_is_front && before.inner_is_front);
    assert_eq!(b2.inside_radius, before.inside_radius);
    assert_eq!(b2.neutral_radius, before.neutral_radius);
    // Rotated by -pi/2 about y, then moved with the middle panel: the axis
    // stays, the centre lies at the middle panel's end, below its front.
    assert!(near(b2.axis, before.axis));
    assert!(near(b2.centre, [a1 + 15.0, W, -T - 4.0]));
    assert!(near(b2.radial_start, [0.0, 0.0, 1.0]));
    // B1 is the only bend recorded as developed by the graph change.
    let record = partial.bends.iter().find(|b| b.id == bend_id(5)).unwrap();
    assert_eq!(&record.folded_embedding, b2);
    let mut refolded = partial.clone();
    reconfigure(&mut refolded, T, panel(3), &open(&[bend_id(4)], false)).unwrap();
    same_embeddings(&refolded, &folded);
    // Opening B2 as well develops the whole Z into one plane.
    let mut flat = partial.clone();
    reconfigure(&mut flat, T, panel(3), &open(&[bend_id(5)], true)).unwrap();
    let a2 = (4.0 + K * T) * FRAC_PI_2;
    let end = embedding(&flat, SheetDomainRef::Panel(panel(5)));
    assert!(near(
        chart_point(end, [12.0, 0.0], 0.0, T),
        [a1 + 15.0 + a2 + 12.0, W, -T]
    ));
    for patch in &flat.charts {
        planar_map(
            &patch.embedding,
            embedding(&flat, SheetDomainRef::Panel(panel(3))),
        )
        .expect("the developed Z is one plane with one front side");
    }
}

#[test]
fn inconsistent_cycles_disconnected_domains_and_stale_bends_fail_explicitly() {
    // A second seam from the base to the flange cannot close once the bend
    // between them is developed.
    let mut cycle = l_sheet();
    let base = cycle.charts[0].clone();
    let flange = cycle.charts[2].clone();
    cycle.joins.push(ShapePort {
        token: 9,
        axial_interval: ParamSpan {
            first: 0.0,
            last: W,
        },
        first: port_side(&base, 3),
        second: port_side(&flange, 1),
    });
    let error = reconfigure(&mut cycle, T, panel(3), &open(&[bend_id(4)], true)).unwrap_err();
    assert!(error.contains("non-developable cycle"), "{error}");

    let mut cut = l_sheet();
    cut.joins.pop();
    let error = reconfigure(&mut cut, T, panel(3), &open(&[bend_id(4)], true)).unwrap_err();
    assert!(error.contains("not connected"), "{error}");

    let mut stale = l_sheet();
    stale.bends[0].k_factor = 0.4;
    let error = reconfigure(&mut stale, T, panel(3), &open(&[bend_id(4)], true)).unwrap_err();
    assert!(error.contains("stale"), "{error}");
    let error = reconfigure(&mut l_sheet(), 3.0, panel(3), &open(&[bend_id(4)], true)).unwrap_err();
    assert!(
        error.contains("stale"),
        "a changed thickness is not developed"
    );

    let error = reconfigure(&mut l_sheet(), T, panel(9), &open(&[bend_id(4)], true)).unwrap_err();
    assert!(error.contains("stationary"), "{error}");
    let error = reconfigure(
        &mut l_sheet(),
        T,
        PanelId {
            feature: FeatureUid(4),
            slot: 0,
        },
        &open(&[bend_id(7)], true),
    )
    .unwrap_err();
    assert!(error.contains("not a bend"), "{error}");
}

#[test]
fn developed_measures_follow_exact_curves() {
    let square = rectangle(FeatureUid(2), 0, [0.0, 0.0], [10.0, 4.0]).unwrap();
    assert!((region_area(&square).unwrap() - 40.0).abs() < 1e-12);
    assert_eq!(
        chord(std::slice::from_ref(&square), 3.0).unwrap(),
        vec![[0.0, 4.0]]
    );
    let disc = region(
        FeatureUid(2),
        0,
        vec![SegmentGeometry::Circle {
            center: [1.0, 2.0],
            radius: 3.0,
            start: 0.0,
        }],
    )
    .unwrap();
    assert!((region_area(&disc).unwrap() - 9.0 * PI).abs() < 1e-12);
    let c = chord(std::slice::from_ref(&disc), 1.0).unwrap();
    assert!((c[0][0] + 1.0).abs() < 1e-9 && (c[0][1] - 5.0).abs() < 1e-9);
    let map = Isometry2 {
        origin: [3.0, -2.0],
        x_axis: [0.0, 1.0],
        y_axis: [-1.0, 0.0],
    };
    let back = then(map, inverse(map));
    assert!(
        back.point([5.0, 7.0])
            .iter()
            .zip([5.0, 7.0])
            .all(|(a, b)| (a - b).abs() < 1e-12)
    );
}

#[test]
fn pieces_keep_their_own_ports_bends_and_open_state() {
    // mitcad#209: a cut that disconnects material publishes each solid with
    // the domains of its regions.
    let l = l_sheet();
    let m = |domain, region| RegionMembership { domain, region };
    let base = SheetDomainRef::Panel(panel(3));
    let strip = SheetDomainRef::Bend(bend_id(4));
    let wall = SheetDomainRef::Panel(panel(4));
    let all: BTreeSet<_> = [m(base, 0), m(strip, 0), m(wall, 0)].into();
    assert_eq!(partition(&l, &all).unwrap(), l);
    // A piece boundary across a registered seam is an inconsistent result.
    let error = partition(&l, &[m(base, 0), m(strip, 0)].into()).unwrap_err();
    assert!(error.contains("separate solids"), "{error}");
    // The strip cut away completely: base and wall come apart; the open
    // state of a bend goes with the piece that keeps the bend.
    let mut cut = l.clone();
    cut.charts.retain(|p| p.domain != strip);
    cut.joins.clear();
    let mut z = z_sheet();
    z.opened.push(OpenedBend {
        bend: bend_id(5),
        by: FeatureUid(9),
        stationary: Some(panel(4)),
    });
    let base_piece = partition(&cut, &[m(base, 0)].into()).unwrap();
    assert_eq!(base_piece.charts.len(), 1);
    assert!(base_piece.joins.is_empty() && base_piece.bends.is_empty());
    let wall_piece = partition(&cut, &[m(wall, 0)].into()).unwrap();
    assert_eq!(wall_piece.charts[0].domain, wall);
    // In the Z, the second bend and its far panel keep their port.
    let far = SheetDomainRef::Panel(panel(5));
    let second = SheetDomainRef::Bend(bend_id(5));
    z.charts.retain(|p| p.domain != strip);
    z.joins
        .retain(|p| p.first.boundary.domain != strip && p.second.boundary.domain != strip);
    let piece = partition(&z, &[m(wall, 0), m(second, 0), m(far, 0)].into()).unwrap();
    assert_eq!(piece.joins.len(), 2);
    assert_eq!(piece.bends.len(), 1);
    assert_eq!(piece.bends[0].id, bend_id(5));
    assert_eq!(piece.opened.len(), 1);
    let rest = partition(&z, &[m(base, 0)].into()).unwrap();
    assert!(rest.opened.is_empty() && rest.bends.is_empty());
}

#[test]
fn the_first_operation_that_took_the_sheet_meaning_away_stays_named() {
    use crate::sheet::{SheetChange, SheetStates, publish};
    let body = BodyUid::new(FeatureUid(3), 0);
    let mut states = SheetStates::new();
    let set = SheetChange::Set(l_sheet());
    publish(
        &mut states,
        body,
        ComponentUid::ROOT,
        1,
        FeatureUid(5),
        Some(&set),
    );
    assert!(states[&body].value.invalidated.is_none());
    // An ordinary operation without a sheet adapter, then an explicit one.
    publish(
        &mut states,
        body,
        ComponentUid::ROOT,
        2,
        FeatureUid(6),
        None,
    );
    let later = SheetChange::Invalidate("a later cut".into());
    publish(
        &mut states,
        body,
        ComponentUid::ROOT,
        3,
        FeatureUid(7),
        Some(&later),
    );
    let first = states[&body].value.invalidated.clone().unwrap();
    assert_eq!(first.feature, FeatureUid(6));
    assert!(first.message.contains("sheet-domain adapter"), "{first:?}");
}

/// A bend past half a turn (a hem's, mitcad#1378): its points beyond 180
/// degrees have chart coordinates, and points past its extent none.
#[test]
fn chart_coordinates_go_on_past_half_a_turn() {
    let mut c = cylinder([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], R, false);
    c.angle_extent = 1.5 * PI;
    let embedding = SheetEmbedding::Cylinder(c.clone());
    for f in [0.25, 0.75, 1.0, 1.25, 1.45] {
        let u = f * PI * c.neutral_radius;
        let p = chart_point(&embedding, [u, 7.0], 0.5, T);
        let (uv, depth) = chart_coordinates(&embedding, p, T).expect("on the bend");
        assert!(
            (uv[0] - u).abs() < 1e-9 && (uv[1] - 7.0).abs() < 1e-9 && (depth - 0.5).abs() < 1e-9,
            "{f}: {uv:?} {depth}"
        );
    }
    let beyond = chart_point(&embedding, [1.75 * PI * c.neutral_radius, 0.0], 0.0, T);
    assert!(chart_coordinates(&embedding, beyond, T).is_none());
}
