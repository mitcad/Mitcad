// SPDX-License-Identifier: MIT
//! Ordered contour corners are trimmed through the material thickness.
//!
//! A corner beside a bend (mitcad#1765: a side of a wall or a face that
//! meets the bend's tangent line at another angle than 90 degrees) is a
//! corner of the developed outline: the bend is opened into the panel's
//! plane, the corner is the one between the panel's free side and the
//! bend's side there, and the round or bevel is cut from every chart in
//! that plane as Sheet Cut cuts; then the bend folds again. The bend's
//! side is trimmed with the panel's, and the flat pattern has the rounded
//! outline.

use super::cut::developed_cut;
use super::develop::{planar_map, reconfigure};
use super::*;
use crate::features::sheet_folded::{SheetCorner, SheetCornerChamferDef, SheetCornerRoundDef};
use crate::sketch::geometry::{add, cross, dist, dot, norm, scale, sub, unit};
use std::collections::BTreeMap;

/// How a corner is trimmed: a round of a radius, or a bevel of two
/// distances (along the incoming and along the outgoing boundary).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CornerSize {
    Round(f64),
    Chamfer([f64; 2]),
}

pub(crate) fn corner_round<K: Kernel>(
    d: &SheetCornerRoundDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    if !d.radius.is_finite() || d.radius <= TOLERANCE {
        return Err("sheet corner radius must be positive".into());
    }
    let (mut state, rules) = read(ctx, d.body)?;
    let corners = slots(&d.corners, &d.corner_slots, CornerSize::Round(d.radius));
    apply_all(ctx, &mut state, rules.thickness, corners)?;
    publish(ctx, state, &rules, None)
}

pub(crate) fn corner_chamfer<K: Kernel>(
    d: &SheetCornerChamferDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    if [d.distances.first, d.distances.second]
        .iter()
        .any(|v| !v.is_finite() || *v <= TOLERANCE)
    {
        return Err("sheet corner distances must be positive".into());
    }
    let (mut state, rules) = read(ctx, d.body)?;
    let size = CornerSize::Chamfer([d.distances.first, d.distances.second]);
    let corners = slots(&d.corners, &d.corner_slots, size);
    apply_all(ctx, &mut state, rules.thickness, corners)?;
    publish(ctx, state, &rules, None)
}

/// An ordinary Fillet or Chamfer on a valid sheet (mitcad#1765) whose edges
/// are all thickness edges at corners of its panels, one of them at least
/// beside a bend: the corners are rounded or bevelled as Corner Round and
/// Corner Chamfer do, in the developed outline, so that the body stays a
/// sheet whose flat pattern has them; the feature warns that it did so.
/// None where that does not apply: the ordinary feature and the
/// sheet-domain adapter then go on as before.
pub(crate) fn dressup_corners<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
    edges: &[(crate::topo::EdgeName, CornerSize)],
) -> Result<Option<FeatureOutput<K::Shape>>, String> {
    // (Without a read of the sheet for other bodies, as the adapter.)
    let valid = ctx
        .env
        .sheets
        .get(&body)
        .is_some_and(|s| s.value.status() == "valid");
    if edges.is_empty() || !valid {
        return Ok(None);
    }
    let Ok((mut state, rules)) = read(ctx, body) else {
        return Ok(None);
    };
    let mut corners = Vec::with_capacity(edges.len());
    let mut beside = false;
    for (index, (name, size)) in edges.iter().enumerate() {
        let Ok(curve) = ctx.kernel.edge_geometry(shape, name) else {
            return Ok(None);
        };
        let (start, end) = match curve {
            crate::datum::CurveGeometry::Line { start, end }
            | crate::datum::CurveGeometry::Other { start, end, .. } => (start, end),
            crate::datum::CurveGeometry::Circle { .. } => return Ok(None),
        };
        let Ok(found) = crate::sheet::pick::corner_at(&state, rules.thickness, start, end) else {
            return Ok(None);
        };
        let Ok(corner) = serde_json::from_value::<SheetCorner>(found["corner"].clone()) else {
            return Ok(None);
        };
        beside |= tangent_to_bend(&state, &corner);
        corners.push((corner, index as u32 * 4, *size));
    }
    if !beside {
        return Ok(None);
    }
    apply_all(ctx, &mut state, rules.thickness, corners)?;
    let output = publish(ctx, state, &rules, None)?;
    let name = ctx.feature_name(ctx.uid);
    ctx.warn(match edges[0].1 {
        CornerSize::Round(_) => format!(
            "{name} rounds a corner beside a bend: rounded through the thickness in the sheet's \
             flat outline, as Corner Round does"
        ),
        CornerSize::Chamfer(_) => format!(
            "{name} bevels a corner beside a bend: bevelled through the thickness in the sheet's \
             flat outline, as Corner Chamfer does"
        ),
    });
    Ok(Some(output))
}

/// Whether a side of the corner is its panel's tangent line to a bend.
fn tangent_to_bend(state: &SheetState, c: &SheetCorner) -> bool {
    let own = SheetDomainRef::Panel(c.panel);
    [
        (&c.incoming, c.incoming_piece),
        (&c.outgoing, c.outgoing_piece),
    ]
    .into_iter()
    .any(|(curve, piece)| {
        let reference = BoundaryRef {
            panel: c.panel,
            curve: curve.clone(),
            piece,
            range: None,
        };
        let Ok(edge) = boundary(state, &reference) else {
            return false;
        };
        state.joins.iter().any(|port| {
            [(&port.first, &port.second), (&port.second, &port.first)]
                .iter()
                .any(|(side, other)| {
                    side.boundary.domain == own
                        && side.boundary.piece == edge.piece
                        && matches!(other.boundary.domain, SheetDomainRef::Bend(_))
                })
        })
    })
}

/// Each corner with the first slot of its tool's curves and its size.
fn slots(
    corners: &[SheetCorner],
    settled: &[crate::features::sheet_folded::CornerSlot],
    size: CornerSize,
) -> Vec<(SheetCorner, u32, CornerSize)> {
    corners
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let slot = settled
                .iter()
                .find(|s| s.corner == *c)
                .map_or(i as u32, |s| s.slot);
            (c.clone(), slot * 4, size)
        })
        .collect()
}

/// A corner's tool, in its panel's chart, and how it trims.
struct CornerTool {
    panel: PanelId,
    tool: MaterialRegion,
    /// A convex corner removes material, a concave one adds it.
    convex: bool,
    /// The bend beside the corner, opened while the tool cuts
    /// (mitcad#1765).
    bend: Option<BendId>,
}

/// The side of the bend beside a corner (mitcad#1765), when `edge`, a
/// boundary of `panel` that ends or starts at `vertex`, is the panel's
/// tangent line to a bend: the bend, the unit direction of the bend's side
/// from the vertex in the panel's chart with the bend developed into its
/// plane, and how far the developed outline runs straight along it (the
/// bend's side, and the side of the panel beyond it where that goes on in
/// line). None for a free boundary.
fn bend_beside(
    state: &SheetState,
    thickness: f64,
    panel: PanelId,
    edge: &MaterialEdge,
    vertex: [f64; 2],
) -> Result<Option<(BendId, [f64; 2], f64)>, String> {
    let own = SheetDomainRef::Panel(panel);
    let mut bend = None;
    for port in &state.joins {
        for (side, other) in [(&port.first, &port.second), (&port.second, &port.first)] {
            if side.boundary.domain != own || side.boundary.piece != edge.piece {
                continue;
            }
            // A seam to a coplanar panel, or a bend's tangent line that
            // stops short of the corner, is not trimmed with it.
            let reaches = port_ends(state, side)
                .is_some_and(|(p, q)| dist(p, vertex) <= 1e-6 || dist(q, vertex) <= 1e-6);
            match other.boundary.domain {
                SheetDomainRef::Bend(b) if reaches && bend.is_none_or(|old| old == b) => {
                    bend = Some(b);
                }
                _ => return Err("sheet corner touches a tangent port".into()),
            }
        }
    }
    let Some(bend) = bend else {
        return Ok(None);
    };
    let mut work = state.clone();
    reconfigure(&mut work, thickness, panel, &BTreeMap::from([(bend, true)]))?;
    let fixed = chart(&work, panel)?.embedding.clone();
    let (j0, j1) = line(edge)?;
    let along = unit(sub(j1, j0));
    // Every straight boundary of the charts in the developed plane, in the
    // panel's chart, and the bend's side at the vertex.
    let mut lines = Vec::new();
    let mut side = None;
    for patch in &work.charts {
        let Ok(map) = planar_map(&patch.embedding, &fixed) else {
            continue;
        };
        for e in mapped_regions(&patch.regions, map)
            .iter()
            .flat_map(|r| &r.loops)
            .flat_map(|l| &l.edges)
        {
            if !matches!(e.curve.geometry, SegmentGeometry::Line { .. }) {
                continue;
            }
            let (p, q) = e.curve.ends()?;
            if dist(p, q) <= TOLERANCE {
                continue;
            }
            lines.push((p, q));
            if patch.domain != SheetDomainRef::Bend(bend) {
                continue;
            }
            for (at, far) in [(p, q), (q, p)] {
                let d = unit(sub(far, at));
                if dist(at, vertex) <= 1e-6 && cross(d, along).abs() > 1e-9 {
                    side = Some(d);
                }
            }
        }
    }
    let direction = side.ok_or(
        "The corner beside the bend cannot be rounded or chamfered: the bend's side there is not \
         straight.",
    )?;
    // The straight stretch from the vertex: the boundaries in line with the
    // bend's side, joined end to end.
    let mut spans: Vec<[f64; 2]> = lines
        .iter()
        .filter(|(p, q)| {
            [p, q]
                .iter()
                .all(|x| cross(direction, sub(**x, vertex)).abs() <= 1e-6)
        })
        .map(|(p, q)| {
            let (s, t) = (
                dot(sub(*p, vertex), direction),
                dot(sub(*q, vertex), direction),
            );
            [s.min(t), s.max(t)]
        })
        .collect();
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut reach = 0.0_f64;
    for [s, t] in spans {
        if s <= reach + 1e-6 && t > reach {
            reach = t;
        }
    }
    Ok(Some((bend, direction, reach)))
}

/// A corner's tool in its panel's chart and whether it removes material
/// (a convex corner) or adds it, from the sheet before the feature: every
/// corner of a feature is found on its input, so that an earlier corner's
/// trim (which splits and renumbers the pieces of the curves) does not
/// change what a later one names (mitcad#280). A corner beside a bend is
/// one of the developed outline (see the module notes).
fn corner_tool(
    feature: FeatureUid,
    state: &SheetState,
    thickness: f64,
    c: &SheetCorner,
    slot: u32,
    size: CornerSize,
) -> Result<CornerTool, String> {
    let incoming = BoundaryRef {
        panel: c.panel,
        curve: c.incoming.clone(),
        piece: c.incoming_piece,
        range: None,
    };
    let outgoing = BoundaryRef {
        panel: c.panel,
        curve: c.outgoing.clone(),
        piece: c.outgoing_piece,
        range: None,
    };
    let a = boundary(state, &incoming)?.clone();
    let b = boundary(state, &outgoing)?.clone();
    let (a0, vertex) = line(&a)?;
    let (v1, b1) = line(&b)?;
    if dist(vertex, v1) > TOLERANCE {
        return Err(
            "sheet corner references must be ordered incoming and outgoing at one vertex".into(),
        );
    }
    let beside = [
        bend_beside(state, thickness, c.panel, &a, vertex)?,
        bend_beside(state, thickness, c.panel, &b, vertex)?,
    ];
    // The two sides from the vertex: the panel's own boundaries, or the
    // side of the bend beside it in the developed outline.
    let mut va = unit(sub(a0, vertex));
    let mut vb = unit(sub(b1, vertex));
    let mut lengths = [norm(sub(a0, vertex)), norm(sub(b1, vertex))];
    let bend = match beside {
        [Some(_), Some(_)] => {
            return Err("The corner lies between two bends; use Corner Relief there.".into());
        }
        [Some((bend, direction, reach)), None] => {
            va = direction;
            lengths[0] = reach;
            Some(bend)
        }
        [None, Some((bend, direction, reach))] => {
            vb = direction;
            lengths[1] = reach;
            Some(bend)
        }
        [None, None] => None,
    };
    let convex = cross(scale(va, -1.0), vb) > 0.0;
    if bend.is_some() && !convex {
        return Err(
            "An inside corner beside a bend cannot be rounded or chamfered: that would add \
             material across the bend."
                .into(),
        );
    }
    let beta = dot(va, vb).clamp(-1.0, 1.0).acos();
    if beta <= 1e-9 || std::f64::consts::PI - beta <= 1e-9 {
        return Err("sheet corner is degenerate".into());
    }
    let mut amounts = match size {
        CornerSize::Round(r) => {
            let tangent = r / (beta / 2.0).tan();
            [tangent, tangent]
        }
        CornerSize::Chamfer(distances) => distances,
    };
    // A corner may take a whole adjacent boundary, to its far vertex
    // (mitcad#242: a chamfer as long as the side of a joined face); within
    // the modelling tolerance it ends exactly there.
    // A round whose one side is shorter than its tangent length and ends
    // on an arc: the round takes that side whole and is tangent to the arc
    // beyond it (mitcad#280, a round next to an earlier round). Not beside
    // a bend: the bend's side is straight.
    if let (CornerSize::Round(r), None) = (size, bend) {
        let past = if amounts[1] > lengths[1] + TOLERANCE && amounts[0] <= lengths[0] + TOLERANCE {
            let next = neighbour(state, c.panel, &b, true)?;
            Some(round_past(
                feature,
                slot,
                r,
                (a0, vertex),
                b1,
                &next,
                convex,
            )?)
        } else if amounts[0] > lengths[0] + TOLERANCE && amounts[1] <= lengths[1] + TOLERANCE {
            let previous = neighbour(state, c.panel, &a, false)?;
            Some(round_past(
                feature,
                slot,
                r,
                (b1, vertex),
                a0,
                &previous,
                convex,
            )?)
        } else {
            None
        };
        if let Some((tool, convex)) = past {
            return Ok(CornerTool {
                panel: c.panel,
                tool,
                convex,
                bend,
            });
        }
    }
    for (side, (amount, length)) in amounts.iter_mut().zip(lengths).enumerate() {
        if *amount > length + TOLERANCE {
            if beside[side].is_some() {
                return Err(
                    "The corner beside the bend is larger than the bend's straight side allows; \
                     make it smaller."
                        .into(),
                );
            }
            return Err("sheet corner exceeds its adjacent material boundaries".into());
        }
        if (*amount - length).abs() <= 1e-9 * length.max(1.0) {
            *amount = length;
        }
    }
    let pa = add(vertex, scale(va, amounts[0]));
    let pb = add(vertex, scale(vb, amounts[1]));
    let mut curves = vec![SegmentGeometry::Line {
        start: vertex,
        end: pa,
    }];
    if let CornerSize::Round(r) = size {
        let centre = add(vertex, scale(unit(add(va, vb)), r / (beta / 2.0).sin()));
        let start = (pa[1] - centre[1]).atan2(pa[0] - centre[0]);
        let end = (pb[1] - centre[1]).atan2(pb[0] - centre[0]);
        let (start_angle, end_angle) = if cross(sub(pa, centre), sub(pb, centre)) > 0.0 {
            (start, end)
        } else {
            (end, start)
        };
        curves.push(SegmentGeometry::Arc {
            center: centre,
            radius: r,
            start_angle,
            end_angle,
        });
    } else {
        curves.push(SegmentGeometry::Line { start: pa, end: pb });
    }
    curves.push(SegmentGeometry::Line {
        start: pb,
        end: vertex,
    });
    Ok(CornerTool {
        panel: c.panel,
        tool: region(feature, slot, curves)?,
        convex,
        bend,
    })
}

/// The edge after (`after`) or before `edge` in its contour of `panel`.
fn neighbour(
    state: &SheetState,
    panel: PanelId,
    edge: &MaterialEdge,
    after: bool,
) -> Result<MaterialEdge, String> {
    let patch = chart(state, panel)?;
    for contour in patch.regions.iter().flat_map(|r| &r.loops) {
        if let Some(i) = contour.edges.iter().position(|e| e == edge) {
            let n = contour.edges.len();
            let j = if after { (i + 1) % n } else { (i + n - 1) % n };
            return Ok(contour.edges[j].clone());
        }
    }
    Err("sheet corner's boundary is not in its panel's contour".into())
}

/// The angle of a point about a centre.
fn angle_of(centre: [f64; 2], p: [f64; 2]) -> f64 {
    (p[1] - centre[1]).atan2(p[0] - centre[0])
}

/// The arc of a circle from `p` to `q` that turns less than half a turn.
fn minor_arc(centre: [f64; 2], radius: f64, p: [f64; 2], q: [f64; 2]) -> SegmentGeometry {
    let (a, b) = (angle_of(centre, p), angle_of(centre, q));
    let ccw = (b - a).rem_euclid(std::f64::consts::TAU);
    let (start_angle, end_angle) = if ccw <= std::f64::consts::PI {
        (a, a + ccw)
    } else {
        (b, b + (a - b).rem_euclid(std::f64::consts::TAU))
    };
    SegmentGeometry::Arc {
        center: centre,
        radius,
        start_angle,
        end_angle,
    }
}

/// A round of radius `r` at the corner `vertex` between the line from
/// `far` (the side long enough) and a short side ending at `short_end`,
/// beyond which the contour continues on the arc `next`: the round's circle
/// is tangent to the long side's line and to the arc's circle, and the
/// tool is the material between the contour (the long side's end, the
/// short side, the arc up to the tangent point) and the round. Of the
/// circles tangent to both, the one touching the long side within it and
/// the arc within its extent, on the short side's side of the long line,
/// must be the only one.
fn round_past(
    feature: FeatureUid,
    slot: u32,
    r: f64,
    (far, vertex): ([f64; 2], [f64; 2]),
    short_end: [f64; 2],
    next: &MaterialEdge,
    convex: bool,
) -> Result<(MaterialRegion, bool), String> {
    let (centre, radius) = match next.curve.geometry {
        SegmentGeometry::Circle { center, radius, .. }
        | SegmentGeometry::Arc { center, radius, .. } => (center, radius),
        _ => return Err("sheet corner exceeds its adjacent material boundaries".into()),
    };
    let u = unit(sub(vertex, far));
    let length = norm(sub(vertex, far));
    // The normal of the long line towards the short side.
    let mut n = [-u[1], u[0]];
    if dot(n, sub(short_end, vertex)) < 0.0 {
        n = scale(n, -1.0);
    }
    let w = sub(add(vertex, scale(n, r)), centre);
    let on_arc = |p: [f64; 2]| {
        let (lo, hi) = (
            next.curve.range.first.min(next.curve.range.last),
            next.curve.range.first.max(next.curve.range.last),
        );
        // (Sampled: within half a step's chord of a sample.)
        let reach = radius * (hi - lo) / 4096.0 + 1e-6;
        (0..=2048).any(|k| {
            let t = lo + (hi - lo) * f64::from(k) / 2048.0;
            next.curve.point(t).is_ok_and(|q| dist(p, q) <= reach)
        })
    };
    let mut found = Vec::new();
    for d in [radius + r, (radius - r).abs()] {
        let b = dot(w, u);
        let disc = b * b - (dot(w, w) - d * d);
        if disc < 0.0 {
            continue;
        }
        for s in [-b - disc.sqrt(), -b + disc.sqrt()] {
            // On the long side, at or before the corner.
            if s > TOLERANCE || s < -length - TOLERANCE {
                continue;
            }
            let p = add(add(vertex, scale(u, s)), scale(n, r));
            let to = unit(sub(p, centre));
            let touch = add(centre, scale(to, radius));
            if (norm(sub(p, centre)) - d).abs() > 1e-9 * d.max(1.0) || !on_arc(touch) {
                continue;
            }
            found.push((p, add(vertex, scale(u, s)), touch));
        }
    }
    let [(p, t1, t2)] = found.as_slice() else {
        return Err(format!(
            "sheet corner exceeds its adjacent material boundaries ({} rounds tangent to the \
             arc beyond its short side)",
            found.len()
        ));
    };
    let curves = vec![
        SegmentGeometry::Line {
            start: *t1,
            end: vertex,
        },
        SegmentGeometry::Line {
            start: vertex,
            end: short_end,
        },
        minor_arc(centre, radius, short_end, *t2),
        minor_arc(*p, r, *t2, *t1),
    ];
    Ok((region(feature, slot, curves)?, convex))
}

/// The corners' tools found on the input (`corner_tool`), then each trims
/// its panel, or, beside a bend, every chart of its panel's plane with the
/// bend opened (as Sheet Cut cuts, mitcad#1765).
fn apply_all<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    thickness: f64,
    corners: Vec<(SheetCorner, u32, CornerSize)>,
) -> Result<(), String> {
    let tools = corners
        .iter()
        .map(|(c, slot, size)| corner_tool(ctx.uid, state, thickness, c, *slot, *size))
        .collect::<Result<Vec<_>, String>>()?;
    for t in tools {
        let Some(bend) = t.bend else {
            trim(
                ctx,
                state,
                &[SheetDomainRef::Panel(t.panel)],
                if t.convex {
                    SheetTrimOperation::Difference
                } else {
                    SheetTrimOperation::Union
                },
                &[t.tool],
                &[Isometry2::IDENTITY],
            )?;
            continue;
        };
        let outcome = developed_cut(
            ctx,
            state,
            thickness,
            t.panel,
            &[bend],
            &[t.tool],
            "the corner",
        )?;
        if !outcome.removed.is_empty() {
            return Err(
                "The corner beside the bend would remove a whole wall or bend of the sheet; make \
                 it smaller."
                    .into(),
            );
        }
        *state = outcome.state;
    }
    Ok(())
}
