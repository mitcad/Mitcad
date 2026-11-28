// SPDX-License-Identifier: MIT
//! Holes and cuts in a Lofted Flange's sheet made by ordinary features (a
//! hole, an extrusion cutting, mitcad#2560): the sheet follows them, so its
//! Flat Pattern shows them where they lie laid flat.
//!
//! - The cut's walls are the result's faces that are not the sheet's own
//!   (the lofted flange's, its flanges' and hems', mitcad#2575). Their
//!   edges on the front of the sheet (every sample on the transition's
//!   front, within 0.1 µm) are laid into the chart: on a flat facet
//!   exactly, by the facet's own map (a circle stays a circle), elsewhere
//!   through the rulings' triangles, as a spline through 65 laid points
//!   with their tangents. Edges laid on the chart's boundary already are
//!   earlier cuts' and are left as they are.
//! - The walls must run through the thickness at right angles to the
//!   sheet: every front edge moved the thickness along the material
//!   direction lies on one of the walls' edges on the back (within the
//!   import tolerance, 0.01 mm). A pocket, a slanted cut or material added
//!   is refused in words.
//! - The front edges join into closed loops (holes and cut-outs) and open
//!   chains whose ends lie on the chart's boundary (notches, cuts that
//!   reach the sheet's edge, mitcad#2575). An open chain takes the material
//!   on the side the result has none (a point beside its middle, half the
//!   thickness deep, classified in the result), closed along the chart's
//!   boundary on that side, from chain to chain. All of them are taken out
//!   of the chart in one sheet trim keeping the chart's other curves; a cut
//!   that would part the sheet in pieces is refused in words.
//! - The body stays the feature's result: the lofted flange's body is not
//!   built from its chart.
//!
//! A Sheet Cut on a lofted flange's sheet (mitcad#2575) is the same: its
//! profiles cut through the thickness at right angles to the flat facet
//! whose front or back their sketch lies on (developed), or swept along
//! the sketch's normal (straight), and the sheet follows the result.

use mitcad_vec3::{add, distance, dot, scale, sub};

use super::*;
use crate::features::sheet_cut::SheetCutDef;
use crate::kernel::{Curve3, KernelError};
use crate::sheet::BoundedCurve2Ext;

/// Why a lofted flange's sheet does not follow an ordinary feature.
pub(crate) enum Refused {
    /// The kernel cannot read the result's faces.
    Unsupported,
    Because(String),
}

impl From<String> for Refused {
    fn from(reason: String) -> Self {
        Self::Because(reason)
    }
}

impl From<&str> for Refused {
    fn from(reason: &str) -> Self {
        Self::Because(reason.into())
    }
}

fn kernel<T>(result: Result<T, KernelError>) -> Result<T, Refused> {
    result.map_err(|e| match e {
        KernelError::Unsupported(_) => Refused::Unsupported,
        other => Refused::Because(other.to_string()),
    })
}

/// A point this near the front or the back lies on it (mm).
const ON: f64 = 1e-4;
/// A wall meets the back this near where its front edge moved through the
/// thickness lies (mm; the imports' geometric tolerance, mitcad#224).
const SQUARE: f64 = 0.01;
/// The samples of an edge (and its spline's stations in the chart).
const SAMPLES: usize = 64;

/// An edge's points at `SAMPLES + 1` equal steps of its parameter, None
/// for a point or a helix.
fn sampled(curve: &Curve3) -> Option<Vec<V3>> {
    (0..=SAMPLES)
        .map(|i| point_at(curve, i as f64 / SAMPLES as f64))
        .collect()
}

/// The point of an edge at the share `s` of its parameter.
fn point_at(curve: &Curve3, s: f64) -> Option<V3> {
    use crate::features::path::{bspline_point, conic_point};
    Some(match curve {
        Curve3::Line { start, end } => add(*start, scale(sub(*end, *start), s)),
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            let (a, b) = if *closed {
                (0.0, std::f64::consts::TAU)
            } else {
                (*start, *end)
            };
            conic_point(*center, *normal, *x_axis, (*major, *minor), a + (b - a) * s)
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let p = *degree as usize;
            if p == 0 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
                return None;
            }
            let (t0, t1) = (knots[p], knots[poles.len()]);
            bspline_point(p, poles, weights, knots, t0 + (t1 - t0) * s)
        }
        Curve3::Point(_) | Curve3::Helix { .. } => return None,
    })
}

/// The distance from a point to a polyline whose ends go on `reach`
/// further along its first and last segments (mitcad#2575: a wall of a
/// notch meets the transition's walls along its profiles, which the mitres
/// at the folds set off at the back, so its back edge ends a little short
/// of or past where its front edge's end lies through the thickness).
fn to_polyline(p: V3, points: &[V3], reach: f64) -> f64 {
    let mut best = f64::INFINITY;
    let last = points.len().saturating_sub(2);
    for (k, w) in points.windows(2).enumerate() {
        let d = sub(w[1], w[0]);
        let l2 = dot(d, d);
        let s = if l2 > 0.0 {
            let length = l2.sqrt();
            let lo = if k == 0 { -reach / length } else { 0.0 };
            let hi = if k == last { 1.0 + reach / length } else { 1.0 };
            (dot(sub(p, w[0]), d) / l2).clamp(lo, hi)
        } else {
            0.0
        };
        best = best.min(distance(add(w[0], scale(d, s)), p));
    }
    best
}

/// A front edge laid into the chart: exactly by a facet's map where every
/// point of it lies on that facet, otherwise a spline through its points
/// laid flat one by one.
fn charted(
    l: &LoftedEmbedding,
    curve: &Curve3,
    points: &[V3],
    slack: f64,
) -> Result<SegmentGeometry, String> {
    let lost = || "an edge of the cut could not be laid flat with the sheet".to_owned();
    let at = |p: V3| located(l, p, 0.0, slack).ok_or_else(lost);
    let middle = at(points[points.len() / 2])?;
    let frame = middle.frame;
    let mut one_facet = true;
    for p in points {
        let here = at(*p)?;
        let d = sub(*p, frame.origin);
        let by_frame = [dot(d, frame.x_axis), dot(d, frame.y_axis)];
        if (by_frame[0] - here.chart[0]).hypot(by_frame[1] - here.chart[1]) > 1e-7 {
            one_facet = false;
            break;
        }
    }
    if one_facet {
        return crate::sheet::rederive::planar_curve(curve, &frame);
    }
    let chart: Vec<[f64; 2]> = points
        .iter()
        .map(|p| at(*p).map(|a| a.chart))
        .collect::<Result<_, _>>()?;
    let h = 1e-5;
    let mut derivatives = Vec::with_capacity(chart.len());
    for i in 0..chart.len() {
        let s = i as f64 / SAMPLES as f64;
        let (s0, s1) = ((s - h).max(0.0), (s + h).min(1.0));
        let p0 = point_at(curve, s0).ok_or_else(lost)?;
        let p1 = point_at(curve, s1).ok_or_else(lost)?;
        let (c0, c1) = (at(p0)?.chart, at(p1)?.chart);
        derivatives.push([(c1[0] - c0[0]) / (s1 - s0), (c1[1] - c0[1]) / (s1 - s0)]);
    }
    Ok(super::geometry::chart_spline(&chart, &derivatives))
}

/// The sheet of a lofted flange's body re-derived after an ordinary
/// feature changed it from `before` to `shape` (see the module
/// documentation): its chart with the cuts taken out.
pub(crate) fn rederived<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    mut state: SheetState,
    thickness: f64,
    before: &K::Shape,
    shape: &K::Shape,
) -> Result<SheetState, Refused> {
    let index = state
        .charts
        .iter()
        .position(|p| matches!(p.embedding, SheetEmbedding::Lofted(_)))
        .ok_or("the sheet has no lofted flange's chart")?;
    let (domain, l) = match &state.charts[index] {
        SheetPatch {
            domain: domain @ SheetDomainRef::Panel(_),
            embedding: SheetEmbedding::Lofted(l),
            ..
        } => (*domain, l.clone()),
        _ => return Err("the lofted flange's chart is not a panel".into()),
    };
    let SheetDomainRef::Panel(panel) = domain else {
        return Err("the lofted flange's chart is not a panel".into());
    };
    let v0 = kernel(ctx.kernel.mass_properties(before))?.volume;
    let v1 = kernel(ctx.kernel.mass_properties(shape))?.volume;
    let size = v0.abs().max(1.0);
    if v1 > v0 + 1e-6 * size {
        return Err(
            "it adds material to the sheet: a lofted flange's sheet takes holes and \
                    cuts through its thickness"
                .into(),
        );
    }
    let slack = 2.0 * thickness + 1e-3;
    let faces = kernel(ctx.kernel.face_geometries(shape))?;
    // The sheet's own faces: the lofted flange's, and its flanges' and
    // hems' (mitcad#2575).
    let own: std::collections::BTreeSet<String> = std::iter::once(panel.feature)
        .chain(state.charts.iter().map(|p| match p.domain {
            SheetDomainRef::Panel(id) => id.feature,
            SheetDomainRef::Bend(id) => id.feature,
        }))
        .map(|f| format!("{f}:"))
        .collect();
    let mut front: Vec<(Curve3, Vec<V3>)> = Vec::new();
    let mut back: Vec<Vec<V3>> = Vec::new();
    for (names, _) in &faces {
        let Some(name) = names.first() else {
            continue;
        };
        // The cuts' walls are the faces that are not the sheet's own.
        if own.iter().any(|o| name.starts_with(o.as_str())) {
            continue;
        }
        let topo: crate::topo::TopoName = name
            .parse()
            .map_err(|_| "a face of the result has no usable name")?;
        for curve in kernel(ctx.kernel.curves_of(shape, &topo))? {
            let Some(points) = sampled(&curve) else {
                continue;
            };
            let on = |depth: f64| {
                points.iter().all(|p| {
                    located(&l, *p, depth, slack).is_some_and(|at| (at.depth - depth).abs() <= ON)
                })
            };
            if on(0.0) {
                front.push((curve, points));
            } else if on(thickness) {
                back.push(points);
            }
        }
    }
    // Laid into the chart; edges on its boundary already are earlier cuts'.
    let regions = state.charts[index].regions.clone();
    let mut laid: Vec<(SegmentGeometry, &[V3])> = Vec::new();
    for (curve, points) in &front {
        let geometry = charted(&l, curve, points, slack)?;
        if !on_boundary(&geometry, &regions)? {
            laid.push((geometry, points.as_slice()));
        }
    }
    if laid.is_empty() {
        if (v0 - v1).abs() <= 1e-6 * size {
            state.shape_generation.clear();
            return Ok(state);
        }
        return Err(
            "it changes the sheet other than by holes and cuts through its thickness \
                    from the front"
                .into(),
        );
    }
    // The walls square to the sheet: through the thickness from each front
    // edge to a back edge.
    for (_, points) in &laid {
        for p in points.iter() {
            let at = located(&l, *p, 0.0, slack).ok_or("a cut's edge lies off the sheet")?;
            let through = add(*p, scale(at.front_to_back, thickness));
            let off = back
                .iter()
                .map(|b| to_polyline(through, b, 2.0 * thickness))
                .fold(f64::INFINITY, f64::min);
            if off > SQUARE {
                return Err(
                    "a cut in a lofted flange's sheet runs through its thickness at right angles \
                     to it: this one is slanted, or does not go through"
                        .into(),
                );
            }
        }
    }
    let curves: Vec<SegmentGeometry> = laid.into_iter().map(|(g, _)| g).collect();
    let probe = Probe {
        l: &l,
        thickness,
        shape,
    };
    let tools = cut_tools(ctx, &probe, &regions, curves)?;
    let pieces = regions.len();
    super::super::trim(
        ctx,
        &mut state,
        &[domain],
        SheetTrimOperation::Difference,
        &tools,
        &[Isometry2::IDENTITY],
    )?;
    let left = state
        .charts
        .iter()
        .find(|p| p.domain == domain)
        .map_or(0, |p| p.regions.len());
    if left == 0 {
        return Err("it removes all of the lofted flange's sheet".into());
    }
    if left > pieces {
        return Err(
            "it parts the lofted flange's sheet in pieces: its sheet stays one piece".into(),
        );
    }
    state.shape_generation.clear();
    Ok(state)
}

/// How near (mm) a laid point lies to the chart's boundary to be on it.
const ON_EDGE: f64 = 1e-4;

/// Whether every point of a laid edge lies on the chart's boundary (an
/// earlier cut's wall).
fn on_boundary(geometry: &SegmentGeometry, regions: &[MaterialRegion]) -> Result<bool, String> {
    let curve = BoundedCurve2::from_geometry(geometry.clone())?;
    let supports: Vec<_> = regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .map(|e| e.curve.support())
        .collect::<Result<_, _>>()?;
    for k in 0..=8 {
        let t = curve.range.first + (curve.range.last - curve.range.first) * f64::from(k) / 8.0;
        let p = curve.point(t)?;
        if !supports.iter().any(|s| s.closest(p).1 <= ON_EDGE) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// What tells which side of a cut lost its material: the result, and the
/// chart's points in it.
struct Probe<'a, S> {
    l: &'a LoftedEmbedding,
    thickness: f64,
    shape: &'a S,
}

/// Laid edges joined end to end: their indices, each reversed or not, and
/// whether they close.
struct Joined {
    items: Vec<(usize, bool)>,
    closed: bool,
    start: [f64; 2],
    end: [f64; 2],
}

fn near2(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= ON_EDGE
}

/// The laid edges joined into closed loops and open chains.
fn joined(ends: &[([f64; 2], [f64; 2])]) -> Vec<Joined> {
    let mut used = vec![false; ends.len()];
    let mut out = Vec::new();
    for first in 0..ends.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut items = std::collections::VecDeque::from([(first, false)]);
        let (mut head, mut tail) = ends[first];
        let mut closed = near2(head, tail);
        // On from the tail, then back from the head.
        while !closed {
            let Some((k, reversed)) = (0..ends.len()).find_map(|k| {
                if used[k] {
                    None
                } else if near2(ends[k].0, tail) {
                    Some((k, false))
                } else if near2(ends[k].1, tail) {
                    Some((k, true))
                } else {
                    None
                }
            }) else {
                break;
            };
            used[k] = true;
            items.push_back((k, reversed));
            tail = if reversed { ends[k].0 } else { ends[k].1 };
            closed = near2(head, tail);
        }
        while !closed {
            let Some((k, reversed)) = (0..ends.len()).find_map(|k| {
                if used[k] {
                    None
                } else if near2(ends[k].1, head) {
                    Some((k, false))
                } else if near2(ends[k].0, head) {
                    Some((k, true))
                } else {
                    None
                }
            }) else {
                break;
            };
            used[k] = true;
            items.push_front((k, reversed));
            head = if reversed { ends[k].1 } else { ends[k].0 };
            closed = near2(head, tail);
        }
        out.push(Joined {
            items: items.into_iter().collect(),
            closed,
            start: head,
            end: tail,
        });
    }
    out
}

/// Where a chart point lies on the boundary: its region, loop, and place
/// along the loop (edge index plus the share of that edge, in the loop's
/// direction).
#[derive(Debug, Clone, Copy)]
struct Place {
    region: usize,
    lp: usize,
    at: f64,
    /// The chain's end there.
    point: [f64; 2],
}

/// The point of a loop at a place along it.
fn loop_point(lp: &MaterialLoop, at: f64) -> Result<[f64; 2], String> {
    let n = lp.edges.len();
    let whole = at.floor();
    let edge = &lp.edges[(whole as usize) % n];
    let (first, last) = (edge.curve.range.first, edge.curve.range.last);
    edge.curve.point(first + (at - whole) * (last - first))
}

fn place_on(regions: &[MaterialRegion], p: [f64; 2]) -> Result<Option<Place>, String> {
    let mut best: Option<(f64, Place)> = None;
    for (r, region) in regions.iter().enumerate() {
        for (li, lp) in region.loops.iter().enumerate() {
            for (ei, edge) in lp.edges.iter().enumerate() {
                let support = edge.curve.support()?;
                let (mut t, d) = support.closest(p);
                if d > ON_EDGE || best.as_ref().is_some_and(|(b, _)| d >= *b) {
                    continue;
                }
                let (first, last) = (edge.curve.range.first, edge.curve.range.last);
                let (lo, hi) = (first.min(last), first.max(last));
                if support.is_closed() {
                    let tau = std::f64::consts::TAU;
                    while t < lo - 1e-9 {
                        t += tau;
                    }
                    while t > hi + 1e-9 {
                        t -= tau;
                    }
                }
                if t < lo - 1e-9 || t > hi + 1e-9 {
                    continue;
                }
                let share = ((t - first) / (last - first)).clamp(0.0, 1.0);
                best = Some((
                    d,
                    Place {
                        region: r,
                        lp: li,
                        at: ei as f64 + share,
                        point: p,
                    },
                ));
            }
        }
    }
    Ok(best.map(|(_, p)| p))
}

/// The loop's boundary from place `from` on to place `to`, in its
/// direction, as pieces of its edges.
fn along(lp: &MaterialLoop, from: f64, to: f64) -> Result<Vec<SegmentGeometry>, String> {
    let n = lp.edges.len();
    let length = (to - from).rem_euclid(n as f64);
    let end = from + length;
    let mut out = Vec::new();
    let mut s = from;
    let mut steps = 0;
    while s < end - 1e-12 && steps <= n + 1 {
        steps += 1;
        let whole = s.floor();
        let next = (whole + 1.0).min(end);
        let (f0, f1) = (s - whole, next - whole);
        s = next;
        if f1 - f0 <= 1e-9 {
            continue;
        }
        let edge = &lp.edges[(whole as usize) % n];
        let (first, last) = (edge.curve.range.first, edge.curve.range.last);
        let piece = BoundedCurve2 {
            geometry: edge.curve.geometry.clone(),
            range: ParamSpan {
                first: first + f0 * (last - first),
                last: first + f1 * (last - first),
            },
        };
        out.push(piece.geometry_piece()?);
    }
    Ok(out)
}

/// The cut-out regions of the laid edges: closed loops as holes and
/// cut-outs, open chains closed along the chart's boundary on the side the
/// result has no material (see the module documentation).
fn cut_tools<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    probe: &Probe<'_, K::Shape>,
    regions: &[MaterialRegion],
    curves: Vec<SegmentGeometry>,
) -> Result<Vec<MaterialRegion>, Refused> {
    let bounded: Vec<BoundedCurve2> = curves
        .iter()
        .map(|g| BoundedCurve2::from_geometry(g.clone()))
        .collect::<Result<_, _>>()?;
    let ends: Vec<([f64; 2], [f64; 2])> =
        bounded.iter().map(|c| c.ends()).collect::<Result<_, _>>()?;
    let mut slot = 0;
    let mut closed = Vec::new();
    // Open chains: their pieces, ends' places, removed material on the left.
    let mut open: Vec<(Vec<SegmentGeometry>, Place, Place)> = Vec::new();
    for chain in joined(&ends) {
        if chain.closed {
            closed.extend(chain.items.iter().map(|(k, _)| curves[*k].clone()));
            continue;
        }
        let unclosed = || {
            Refused::from(
                "a cut's edge on the sheet's front neither closes nor reaches the sheet's edge",
            )
        };
        let start = place_on(regions, chain.start)?.ok_or_else(unclosed)?;
        let end = place_on(regions, chain.end)?.ok_or_else(unclosed)?;
        // Beside the middle of the chain: which side kept its material.
        let (k, reversed) = chain.items[chain.items.len() / 2];
        let c = &bounded[k];
        let (first, last) = (c.range.first, c.range.last);
        let middle = 0.5 * (first + last);
        let h = 1e-3 * (last - first);
        let m = c.point(middle)?;
        let (a, b) = (c.point(middle - h)?, c.point(middle + h)?);
        let mut d = [b[0] - a[0], b[1] - a[1]];
        if reversed {
            d = [-d[0], -d[1]];
        }
        let length = d[0].hypot(d[1]);
        if length <= 1e-15 {
            return Err("a cut's edge on the sheet's front has no length".into());
        }
        let left = [-d[1] / length, d[0] / length];
        let step = (0.25 * probe.thickness).min(0.01);
        let beside = |s: f64| {
            super::chart_point(
                probe.l,
                [m[0] + s * step * left[0], m[1] + s * step * left[1]],
                probe.thickness / 2.0,
            )
        };
        let inside = kernel(
            ctx.kernel
                .points_inside(probe.shape, &[beside(1.0), beside(-1.0)]),
        )?;
        let pieces: Vec<SegmentGeometry> = chain
            .items
            .iter()
            .map(|(k, _)| curves[*k].clone())
            .collect();
        match inside.as_slice() {
            // Removed on the left: as the chain runs.
            [false, true] => open.push((pieces, start, end)),
            // Removed on the right: the chain the other way.
            [true, false] => open.push((pieces, end, start)),
            _ => {
                return Err(
                    "which side of a cut that reaches the sheet's edge was taken away could not \
                     be told"
                        .into(),
                );
            }
        }
    }
    let mut tools = if closed.is_empty() {
        Vec::new()
    } else {
        crate::sheet::rederive::regions(ctx.uid, &mut slot, closed)?
    };
    // Each removed region: a chain, the boundary on from its end (the
    // material lies left of the boundary, so the removed part does) to the
    // next chain's start, and so on round.
    let mut used = vec![false; open.len()];
    for first in 0..open.len() {
        if used[first] {
            continue;
        }
        let mut outline: Vec<SegmentGeometry> = Vec::new();
        let mut current = first;
        loop {
            used[current] = true;
            let (pieces, _, end) = &open[current];
            outline.extend(pieces.iter().cloned());
            let lp = &regions[end.region].loops[end.lp];
            let n = lp.edges.len() as f64;
            let ahead = |p: &Place| (p.at - end.at).rem_euclid(n);
            // The nearest chain start ahead on the same loop, with no
            // chain's end before it.
            let same = |p: &Place| p.region == end.region && p.lp == end.lp;
            let next = (0..open.len())
                .filter(|&j| same(&open[j].1))
                .min_by(|&i, &j| ahead(&open[i].1).total_cmp(&ahead(&open[j].1)))
                .ok_or("a cut that reaches the sheet's edge does not close along it")?;
            let crossed = (0..open.len()).any(|j| {
                j != current
                    && same(&open[j].2)
                    && ahead(&open[j].2) > 1e-9
                    && ahead(&open[j].2) < ahead(&open[next].1)
            });
            if crossed || (used[next] && next != first) {
                return Err(
                    "cuts that reach the sheet's edge cross each other: make them one cut".into(),
                );
            }
            // The chain's ends lie on the boundary within ON_EDGE: joined
            // to it exactly.
            let start = open[next].1;
            let joint = |a: [f64; 2], b: [f64; 2]| {
                ((a[0] - b[0]).hypot(a[1] - b[1]) > 1e-9)
                    .then_some(SegmentGeometry::Line { start: a, end: b })
            };
            outline.extend(joint(end.point, loop_point(lp, end.at)?));
            outline.extend(along(lp, end.at, start.at)?);
            outline.extend(joint(loop_point(lp, start.at)?, start.point));
            if next == first {
                break;
            }
            current = next;
        }
        tools.extend(crate::sheet::rederive::regions(
            ctx.uid, &mut slot, outline,
        )?);
    }
    Ok(tools)
}

#[cfg(test)]
#[path = "lofted_cut_tests.rs"]
mod tests;

/// A refusal of the lofted flange's sheet in words.
pub(crate) fn refusal(r: Refused) -> String {
    match r {
        Refused::Unsupported => "the geometry kernel cannot read the cut's faces".into(),
        Refused::Because(reason) => reason,
    }
}

/// The material direction of the flat facet whose front (along it) or
/// back (against it) holds a sketch's plane; None where no facet's does.
fn facet_side(l: &LoftedEmbedding, frame: &SketchFrame, thickness: f64) -> Option<V3> {
    let n = frame.normal();
    for pair in l.rulings.windows(2) {
        let (r, s) = (&pair[0], &pair[1]);
        let [r0, r1, s0, s1] = super::pair_at(l, r, s, 0.0);
        for (m, c) in [
            ([r0, r1, s0], [r.chart[0], r.chart[1], s.chart[0]]),
            ([r1, s1, s0], [r.chart[1], s.chart[1], s.chart[0]]),
        ] {
            let Some((_, normal)) = super::triangle_frame(m, c, r.front_to_back) else {
                continue;
            };
            if dot(n, normal).abs() < 1.0 - 1e-9 {
                continue;
            }
            let offset = dot(sub(frame.origin, m[0]), normal);
            if offset.abs() <= 1e-6 {
                return Some(normal);
            }
            if (offset - thickness).abs() <= 1e-6 {
                return Some(scale(normal, -1.0));
            }
        }
    }
    None
}

/// A Sheet Cut on a lofted flange's sheet (mitcad#2575), developed: the
/// profiles of each sketch lying on the front or the back of a flat facet
/// cut through the thickness at right angles to it, and the sheet
/// re-derived from the result as for an ordinary feature's cut ([`rederived`]):
/// holes and cut-outs, and notches that reach the sheet's edge.
pub(crate) fn sheet_cut<K: Kernel>(
    d: &SheetCutDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (state, rules) = super::super::read_any(ctx, d.body)?;
    let panel = super::lofted_panel(&state).ok_or("the sheet has no lofted flange's chart")?;
    if d.profiles.is_empty() {
        return Err("no profiles selected".into());
    }
    if d.stationary.is_some_and(|s| s != panel) {
        return Err(
            "a sheet cut on a lofted flange's sheet cuts its transition, which stays fixed: cuts \
             in its flanges are not taken yet"
                .into(),
        );
    }
    if !d.open_bends.is_empty() {
        return Err(
            "a sheet cut on a lofted flange's sheet opens no bends: its profiles lie on a flat \
             facet"
                .into(),
        );
    }
    let l = match &super::super::chart(&state, panel)?.embedding {
        SheetEmbedding::Lofted(l) => l.clone(),
        _ => return Err("the sheet has no lofted flange's chart".into()),
    };
    let t = rules.thickness;
    let current = ctx.body(d.body)?;
    let mut sketches: Vec<crate::ids::FeatureUid> = d.profiles.iter().map(|p| p.sketch).collect();
    sketches.sort();
    sketches.dedup();
    let mut shape = current.clone();
    let mut touched = false;
    for uid in sketches {
        let sketch = ctx.sketch(uid)?;
        let frame = sketch.frame;
        let direction = facet_side(&l, &frame, t).ok_or(
            "the cut's sketch does not lie on the front or the back of a flat facet of the lofted \
             flange",
        )?;
        let mut regions = Vec::new();
        for profile in d.profiles.iter().filter(|p| p.sketch == uid) {
            regions.extend(super::super::cut::found_profiles(
                ctx,
                &sketch,
                uid,
                &profile.region,
            )?);
        }
        // Through the thickness, a little past both surfaces.
        let over = (0.01 * t).min(0.01);
        let tool = ctx
            .kernel
            .extrude(&crate::kernel::ExtrudeSpec {
                feature: ctx.uid,
                frame,
                regions: &regions,
                direction,
                start: -over,
                end: t + over,
            })
            .map_err(|e| e.to_string())?;
        let cut = ctx
            .kernel
            .boolean(crate::kernel::BooleanOp::Cut, &[&shape], &tool)
            .map_err(|e| e.to_string())?;
        if !cut.touched.first().copied().unwrap_or(false) {
            continue;
        }
        let [piece] = cut.pieces.as_slice() else {
            return Err(
                "the cut parts the lofted flange's sheet in pieces: its sheet stays one piece"
                    .into(),
            );
        };
        shape = piece.shape.clone();
        touched = true;
    }
    let mut output = FeatureOutput::default();
    if !touched {
        ctx.warn("the cut profiles do not reach the sheet material");
        super::super::finish(&mut output, state, &rules, current)?;
        return Ok(output);
    }
    let state = rederived(ctx, state, t, &current, &shape).map_err(refusal)?;
    super::super::finish(&mut output, state, &rules, shape)?;
    Ok(output)
}
