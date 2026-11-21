// SPDX-License-Identifier: MIT
//! Corner relief at the junction of two bends (mitcad#209; junctions without
//! a common panel and non-perpendicular rectangular reliefs mitcad#125).
//!
//! The junction is found in a development: a stationary panel stays fixed
//! while the two bends, and every bend on the seam paths from that panel to
//! them, are developed into its plane. The panel is the one both bends leave.
//! When they leave none in common (their panels meet through planar seams,
//! other panels or other bends), it is the panel next to the first bend on
//! the shortest seam path to the second, or the selected panel, which must
//! join one of the two bends. Each bend is then a strip between the tangent
//! line through which the development reaches it (`s = 0`, the line on the
//! common panel when there is one) and its far tangent line (`s = BA`). The
//! junction is where the two strips would cross: `J0` is the meeting point of
//! the two `s = 0` lines and `C` that of the two bend centrelines
//! (`s = BA/2`). `C` and the crossing zones do not depend on which side of a
//! strip is `s = 0`. The relief is removed from all material in the panel's
//! developed plane with the same operation as a sheet Cut, then the input
//! configuration returns:
//!
//! - `round`: a disc of diameter `width` centred at `C`;
//! - `rectangular`: the crossing of two bands of width `width` centred on the
//!   two centrelines (`|s1 - BA1/2| <= width/2`, `|s2 - BA2/2| <= width/2`):
//!   for perpendicular bends a square of side `width` with sides along both
//!   strips, otherwise the parallelogram with sides parallel to both bend
//!   lines and `width` across each strip (area `width^2 / sin(angle)`);
//! - `tear`: exactly the crossing zone `0 <= s1 <= BA1, 0 <= s2 <= BA2`, the
//!   minimum separation of the two bend zones; it needs no size.
//!
//! `depth` belongs to bend reliefs and is not used here. A relief that would
//! remove a whole domain or split the sheet is refused (the flange is too
//! short for that relief), as is one reaching a seam of a third, folded bend;
//! parallel bends have no junction.

use super::cut::developed_cut;
use super::develop::{bend_record, bend_side, domain_label, planar_map, reconfigure};
use super::*;
use crate::features::sheet::ReliefKind;
use crate::features::sheet_relief::SheetCornerReliefDef;
use std::collections::{BTreeMap, VecDeque};

type V2 = [f64; 2];

fn vector(map: Isometry2, v: V2) -> V2 {
    [
        v[0] * map.x_axis[0] + v[1] * map.y_axis[0],
        v[0] * map.x_axis[1] + v[1] * map.y_axis[1],
    ]
}

/// A developed bend in the panel's chart: its `s = 0` tangent line passes
/// through `origin`; `into` is the unit direction of increasing `s`,
/// `allowance` its developed width.
#[derive(Debug, Clone, Copy)]
struct Strip {
    origin: V2,
    into: V2,
    allowance: f64,
}

/// The point at offset `s` into the first strip and `t` into the second.
fn junction(a: &Strip, b: &Strip, s: f64, t: f64) -> Result<V2, String> {
    let det = a.into[0] * b.into[1] - a.into[1] * b.into[0];
    if det.abs() <= 1e-9 {
        return Err("the bends are parallel and have no corner junction".into());
    }
    // Solve into_a . p = ra, into_b . p = rb: the offsets measured from each
    // tangent line.
    let ra = s + a.origin[0] * a.into[0] + a.origin[1] * a.into[1];
    let rb = t + b.origin[0] * b.into[0] + b.origin[1] * b.into[1];
    Ok([
        (ra * b.into[1] - rb * a.into[1]) / det,
        (a.into[0] * rb - b.into[0] * ra) / det,
    ])
}

/// The corners of the rectangular relief of `width` centred at `centre`:
/// the crossing of the bands `|s1 - BA1/2| <= width/2` and
/// `|s2 - BA2/2| <= width/2`. For perpendicular strips this is the square
/// `centre + s*h*into_a + t*h*into_b`, `s, t = +-1`.
fn rectangular(a: &Strip, b: &Strip, centre: V2, width: f64) -> Vec<V2> {
    let h = width / 2.0;
    let d = a.into[0] * b.into[0] + a.into[1] * b.into[1];
    let sine2 = 1.0 - d * d;
    // p = centre + alpha*into_a + beta*into_b with into_a.(p - centre) = s*h
    // and into_b.(p - centre) = t*h.
    let at = |s: f64, t: f64| {
        let alpha = (s * h - d * t * h) / sine2;
        let beta = (t * h - d * s * h) / sine2;
        [
            centre[0] + alpha * a.into[0] + beta * b.into[0],
            centre[1] + alpha * a.into[1] + beta * b.into[1],
        ]
    };
    vec![at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]
}

fn ccw(mut points: Vec<V2>) -> Vec<V2> {
    let area: f64 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(p, q)| p[0] * q[1] - q[0] * p[1])
        .sum();
    if area < 0.0 {
        points.reverse();
    }
    points
}

/// The panels joined to `bend` through registered seams.
fn neighbours(state: &SheetState, bend: BendId) -> BTreeSet<PanelId> {
    let domain = SheetDomainRef::Bend(bend);
    state
        .joins
        .iter()
        .filter_map(|p| {
            if p.first.boundary.domain == domain {
                Some(p.second.boundary.domain)
            } else if p.second.boundary.domain == domain {
                Some(p.first.boundary.domain)
            } else {
                None
            }
        })
        .filter_map(|d| match d {
            SheetDomainRef::Panel(p) => Some(p),
            SheetDomainRef::Bend(_) => None,
        })
        .collect()
}

/// Breadth-first search over the registered seams from `start`, the seams in
/// their stored order: every reached domain with the domain it was first
/// reached from.
fn reach(
    state: &SheetState,
    start: SheetDomainRef,
) -> BTreeMap<SheetDomainRef, Option<SheetDomainRef>> {
    let mut parents = BTreeMap::from([(start, None)]);
    let mut queue = VecDeque::from([start]);
    while let Some(domain) = queue.pop_front() {
        for port in &state.joins {
            for (own, other) in [(&port.first, &port.second), (&port.second, &port.first)] {
                let next = other.boundary.domain;
                if own.boundary.domain == domain && !parents.contains_key(&next) {
                    parents.insert(next, Some(domain));
                    queue.push_back(next);
                }
            }
        }
    }
    parents
}

/// The seam path from the start of `parents` to `to`, both included.
fn path(
    parents: &BTreeMap<SheetDomainRef, Option<SheetDomainRef>>,
    to: SheetDomainRef,
) -> Option<Vec<SheetDomainRef>> {
    let mut path = vec![to];
    let mut at = to;
    while let Some(parent) = *parents.get(&at)? {
        path.push(parent);
        at = parent;
    }
    path.reverse();
    Some(path)
}

/// The stationary panel of the development (see the module notes).
fn stationary_panel(
    state: &SheetState,
    first: BendId,
    second: BendId,
    selected: Option<PanelId>,
) -> Result<PanelId, String> {
    let label = |b: BendId| domain_label(SheetDomainRef::Bend(b));
    let (near_first, near_second) = (neighbours(state, first), neighbours(state, second));
    let common: Vec<PanelId> = near_first.intersection(&near_second).copied().collect();
    if !common.is_empty() {
        return match selected {
            Some(p) if common.contains(&p) => Ok(p),
            Some(p) => Err(format!(
                "{} does not join both {} and {}",
                domain_label(SheetDomainRef::Panel(p)),
                label(first),
                label(second)
            )),
            None => match common.as_slice() {
                [p] => Ok(*p),
                _ => Err(format!(
                    "{} and {} leave several common panels; select one",
                    label(first),
                    label(second)
                )),
            },
        };
    }
    // No common panel: the junction is taken in the development of the seam
    // path between the two bends.
    if let Some(p) = selected {
        if near_first.contains(&p) || near_second.contains(&p) {
            return Ok(p);
        }
        return Err(format!(
            "{} joins neither {} nor {}",
            domain_label(SheetDomainRef::Panel(p)),
            label(first),
            label(second)
        ));
    }
    let parents = reach(state, SheetDomainRef::Bend(first));
    let route = path(&parents, SheetDomainRef::Bend(second)).ok_or_else(|| {
        format!(
            "{} and {} are not connected through the sheet's bends and seams",
            label(first),
            label(second)
        )
    })?;
    route
        .iter()
        .find_map(|d| match d {
            SheetDomainRef::Panel(p) => Some(*p),
            SheetDomainRef::Bend(_) => None,
        })
        .ok_or_else(|| {
            format!(
                "{} and {} are joined by no panel",
                label(first),
                label(second)
            )
        })
}

/// A bend developed in the fixed chart, measured from its tangent line on
/// `from`, the domain through which the development reaches it.
fn strip(
    state: &SheetState,
    bend: BendId,
    from: SheetDomainRef,
    fixed: &SheetEmbedding,
    thickness: f64,
) -> Result<Strip, String> {
    let record = bend_record(state, bend, thickness)?;
    let domain = SheetDomainRef::Bend(bend);
    let patch = state
        .charts
        .iter()
        .find(|p| p.domain == domain)
        .ok_or_else(|| format!("{} has no material chart", domain_label(domain)))?;
    let map = planar_map(&patch.embedding, fixed)?;
    let mut side: Option<f64> = None;
    for port in &state.joins {
        let own = if port.first.boundary.domain == domain && port.second.boundary.domain == from {
            &port.first
        } else if port.second.boundary.domain == domain && port.first.boundary.domain == from {
            &port.second
        } else {
            continue;
        };
        let u = bend_side(patch, own, record.allowance)?;
        if side.is_some_and(|old| (old - u).abs() > TOLERANCE) {
            return Err(format!(
                "{} joins {} along both tangent lines",
                domain_label(domain),
                domain_label(from)
            ));
        }
        side = Some(u);
    }
    let u = side.ok_or_else(|| {
        format!(
            "{} does not join {}",
            domain_label(domain),
            domain_label(from)
        )
    })?;
    let into = vector(map, [if u <= TOLERANCE { 1.0 } else { -1.0 }, 0.0]);
    Ok(Strip {
        origin: map.point([u, 0.0]),
        into,
        allowance: record.allowance,
    })
}

pub(crate) fn corner_relief<K: Kernel>(
    d: &SheetCornerReliefDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (state, rules) = read(ctx, d.body)?;
    let relief = d.relief.clone().unwrap_or(rules.corner_relief.clone());
    let thickness = rules.thickness;
    let [first, second] = d.bends[..] else {
        return Err("a corner relief names exactly two bends".into());
    };
    if relief.kind != ReliefKind::Tear && (!relief.width.is_finite() || relief.width <= TOLERANCE) {
        return Err("a corner relief requires a positive width".into());
    }
    for bend in [first, second] {
        bend_record(&state, bend, thickness)?;
    }
    let panel = stationary_panel(&state, first, second, d.panel)?;
    // The bends developed: both, and every bend on the seam paths from the
    // panel to them (none when they leave it directly).
    let parents = reach(&state, SheetDomainRef::Panel(panel));
    let mut open: Vec<BendId> = Vec::new();
    let mut entries = Vec::new();
    for bend in [first, second] {
        let route = path(&parents, SheetDomainRef::Bend(bend)).ok_or_else(|| {
            format!(
                "{} is not connected to {} through the sheet's bends and seams",
                domain_label(SheetDomainRef::Bend(bend)),
                domain_label(SheetDomainRef::Panel(panel))
            )
        })?;
        for d in &route {
            if let SheetDomainRef::Bend(b) = d
                && !open.contains(b)
            {
                open.push(*b);
            }
        }
        entries.push(route[route.len() - 2]);
    }
    // The junction is found with those bends developed from the panel.
    let mut work = state.clone();
    let targets: BTreeMap<BendId, bool> = open.iter().map(|b| (*b, true)).collect();
    reconfigure(&mut work, thickness, panel, &targets)?;
    let fixed = chart(&work, panel)?.embedding.clone();
    let a = strip(&work, first, entries[0], &fixed, thickness)?;
    let b = strip(&work, second, entries[1], &fixed, thickness)?;
    let centre = junction(&a, &b, a.allowance / 2.0, b.allowance / 2.0)?;
    let tool = match relief.kind {
        ReliefKind::Round => region(
            ctx.uid,
            0,
            vec![SegmentGeometry::Circle {
                center: centre,
                radius: relief.width / 2.0,
                start: 0.0,
            }],
        )?,
        ReliefKind::Rectangular => {
            polygon(ctx.uid, 0, &ccw(rectangular(&a, &b, centre, relief.width)))?
        }
        ReliefKind::Tear => polygon(
            ctx.uid,
            0,
            &ccw(vec![
                junction(&a, &b, 0.0, 0.0)?,
                junction(&a, &b, a.allowance, 0.0)?,
                junction(&a, &b, a.allowance, b.allowance)?,
                junction(&a, &b, 0.0, b.allowance)?,
            ]),
        )?,
    };
    let outcome = developed_cut(
        ctx,
        &state,
        thickness,
        panel,
        &open,
        &[tool],
        "the corner relief",
    )?;
    if let Some(domain) = outcome.removed.first() {
        return Err(format!(
            "the corner relief removes all material of {}; the flange is too short for it",
            domain_label(*domain)
        ));
    }
    if !outcome.used_tools {
        ctx.warn("the corner relief removes no material at the junction");
    }
    publish_pieces(ctx, outcome.state, &rules, Some("the corner relief"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(p: V2, q: V2) -> bool {
        (p[0] - q[0]).abs() < 1e-12 && (p[1] - q[1]).abs() < 1e-12
    }

    #[test]
    fn junction_is_measured_from_both_tangent_lines() {
        let ba = 1.2 * std::f64::consts::PI;
        // Bend 1 leaves the panel at x = 37 towards +x, bend 2 at y = 27
        // towards +y: the tangent lines meet at (37, 27), the centrelines at
        // (37 + BA/2, 27 + BA/2).
        let a = Strip {
            origin: [37.0, -5.0],
            into: [1.0, 0.0],
            allowance: ba,
        };
        let b = Strip {
            origin: [4.0, 27.0],
            into: [0.0, 1.0],
            allowance: ba,
        };
        assert!(near(junction(&a, &b, 0.0, 0.0).unwrap(), [37.0, 27.0]));
        assert!(near(
            junction(&a, &b, ba / 2.0, ba / 2.0).unwrap(),
            [37.0 + ba / 2.0, 27.0 + ba / 2.0]
        ));
        // Measured into each strip, whichever way the strip runs.
        let c = Strip {
            origin: [0.0, 27.0],
            into: [0.0, -1.0],
            allowance: ba,
        };
        assert!(near(junction(&a, &c, 1.0, 2.0).unwrap(), [38.0, 25.0]));
        let parallel = Strip {
            origin: [50.0, 0.0],
            into: [-1.0, 0.0],
            allowance: ba,
        };
        assert!(junction(&a, &parallel, 0.0, 0.0).is_err());
        // Outlines are counter-clockwise whatever the bends' handedness.
        let square = ccw(vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        assert_eq!(square[1], [1.0, 1.0]);
    }

    #[test]
    fn the_centre_and_crossing_zone_do_not_depend_on_the_strip_side() {
        // The same strip measured from its other tangent line.
        let ba = 3.0;
        let a = Strip {
            origin: [10.0, 0.0],
            into: [1.0, 0.0],
            allowance: ba,
        };
        let a_far = Strip {
            origin: [10.0 + ba, 7.0],
            into: [-1.0, 0.0],
            allowance: ba,
        };
        let b = Strip {
            origin: [0.0, 5.0],
            into: [0.6, 0.8],
            allowance: 2.0,
        };
        let c = junction(&a, &b, ba / 2.0, 1.0).unwrap();
        assert!(near(junction(&a_far, &b, ba / 2.0, 1.0).unwrap(), c));
        let zone = |s: &Strip| -> Vec<V2> {
            let mut corners: Vec<V2> = [(0.0, 0.0), (ba, 0.0), (ba, 2.0), (0.0, 2.0)]
                .iter()
                .map(|(p, q)| junction(s, &b, *p, *q).unwrap())
                .collect();
            corners.sort_by(|p, q| p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1])));
            corners
        };
        for (p, q) in zone(&a).iter().zip(zone(&a_far)) {
            assert!((p[0] - q[0]).abs() < 1e-12 && (p[1] - q[1]).abs() < 1e-12);
        }
    }

    #[test]
    fn rectangular_relief_is_the_crossing_of_two_bands() {
        let a = Strip {
            origin: [0.0, 0.0],
            into: [1.0, 0.0],
            allowance: 4.0,
        };
        // Perpendicular: the square of side `width` along both strips.
        let b = Strip {
            origin: [0.0, 0.0],
            into: [0.0, 1.0],
            allowance: 4.0,
        };
        let centre = junction(&a, &b, 2.0, 2.0).unwrap();
        let square = rectangular(&a, &b, centre, 2.0);
        assert_eq!(square, vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]]);
        // At 60 degrees between the strips' directions: a parallelogram whose
        // sides are parallel to both bend lines, `width` across each strip,
        // of area width^2 / sin(60 degrees).
        let (sin, cos) = (3f64.sqrt() / 2.0, 0.5);
        let b = Strip {
            origin: [0.0, 0.0],
            into: [cos, sin],
            allowance: 4.0,
        };
        let centre = junction(&a, &b, 2.0, 2.0).unwrap();
        let corners = ccw(rectangular(&a, &b, centre, 2.0));
        let area: f64 = corners
            .iter()
            .zip(corners.iter().cycle().skip(1))
            .map(|(p, q)| (p[0] * q[1] - q[0] * p[1]) / 2.0)
            .sum();
        assert!((area - 4.0 / sin).abs() < 1e-12, "{area}");
        for p in &corners {
            let s1 = p[0];
            let s2 = p[0] * cos + p[1] * sin;
            assert!(((s1 - 2.0).abs() - 1.0).abs() < 1e-12, "{p:?}");
            assert!(((s2 - 2.0).abs() - 1.0).abs() < 1e-12, "{p:?}");
        }
    }

    fn port(token: u32, a: SheetDomainRef, b: SheetDomainRef) -> ShapePort {
        let side = |domain| ShapePortSide {
            boundary: BoundaryPieceRef {
                domain,
                piece: generated(FeatureUid(99), token),
            },
            span: ParamSpan {
                first: 0.0,
                last: 1.0,
            },
            axial: AxialMap {
                offset: 0.0,
                scale: 1.0,
            },
        };
        ShapePort {
            token,
            axial_interval: ParamSpan {
                first: 0.0,
                last: 1.0,
            },
            first: side(a),
            second: side(b),
        }
    }

    #[test]
    fn the_panel_of_a_junction_without_a_common_panel_is_on_the_seam_path() {
        let panel = |f: u64, slot: u32| PanelId {
            feature: FeatureUid(f),
            slot,
        };
        let bend = |f: u64| BendId {
            feature: FeatureUid(f),
            slot: 0,
        };
        let (p_left, p_right, p_mid) = (panel(3, 0), panel(3, 1), panel(3, 2));
        let (b1, b2, b3) = (bend(4), bend(5), bend(6));
        let (w1, w2) = (panel(4, 0), panel(5, 0));
        // Wall 1 -- b1 -- right panel == left panel -- b2 -- wall 2, the two
        // base panels joined by a planar seam; b3 joins the right panel and
        // a third panel that neither b1 nor b2 joins.
        let state = SheetState {
            schema: 2,
            algorithm: ALGORITHM_VERSION,
            body: BodyUid::new(FeatureUid(3), 0),
            owner: crate::ids::ComponentUid::ROOT,
            shape_generation: String::new(),
            rules: FeatureUid(1),
            rules_version: String::new(),
            panels: Vec::new(),
            charts: Vec::new(),
            bends: Vec::new(),
            joins: vec![
                port(0, SheetDomainRef::Panel(p_right), SheetDomainRef::Bend(b1)),
                port(1, SheetDomainRef::Bend(b1), SheetDomainRef::Panel(w1)),
                port(
                    2,
                    SheetDomainRef::Panel(p_left),
                    SheetDomainRef::Panel(p_right),
                ),
                port(3, SheetDomainRef::Panel(p_left), SheetDomainRef::Bend(b2)),
                port(4, SheetDomainRef::Bend(b2), SheetDomainRef::Panel(w2)),
                port(5, SheetDomainRef::Panel(p_right), SheetDomainRef::Bend(b3)),
                port(6, SheetDomainRef::Bend(b3), SheetDomainRef::Panel(p_mid)),
            ],
            opened: Vec::new(),
            lineage: Vec::new(),
            overlays: Vec::new(),
            gaps: Vec::new(),
            invalidated: None,
            blockers: Vec::new(),
        };
        // The panel next to the first bend on the path to the second.
        assert_eq!(stationary_panel(&state, b1, b2, None), Ok(p_right));
        assert_eq!(stationary_panel(&state, b2, b1, None), Ok(p_left));
        // A selected panel must join one of them.
        assert_eq!(stationary_panel(&state, b1, b2, Some(w2)), Ok(w2));
        assert_eq!(stationary_panel(&state, b1, b2, Some(w1)), Ok(w1));
        let refused = stationary_panel(&state, b1, b2, Some(p_mid)).unwrap_err();
        assert!(refused.contains("joins neither"), "{refused}");
        // With a common panel the selection keeps its earlier rule.
        assert_eq!(stationary_panel(&state, b1, b3, None), Ok(p_right));
        let refused = stationary_panel(&state, b1, b3, Some(w1)).unwrap_err();
        assert!(refused.contains("does not join both"), "{refused}");
        // The paths developed from the right panel: b1 directly, b2 through
        // the planar seam.
        let parents = reach(&state, SheetDomainRef::Panel(p_right));
        assert_eq!(
            path(&parents, SheetDomainRef::Bend(b2)).unwrap(),
            vec![
                SheetDomainRef::Panel(p_right),
                SheetDomainRef::Panel(p_left),
                SheetDomainRef::Bend(b2)
            ]
        );
        // From wall 2 the path to b1 crosses b2 as well.
        let parents = reach(&state, SheetDomainRef::Panel(w2));
        assert_eq!(
            path(&parents, SheetDomainRef::Bend(b1)).unwrap(),
            vec![
                SheetDomainRef::Panel(w2),
                SheetDomainRef::Bend(b2),
                SheetDomainRef::Panel(p_left),
                SheetDomainRef::Panel(p_right),
                SheetDomainRef::Bend(b1)
            ]
        );
    }
}
