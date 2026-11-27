// SPDX-License-Identifier: MIT
//! Holes and cuts in a Lofted Flange's sheet made by ordinary features (a
//! hole, an extrusion cutting, mitcad#2560): the sheet follows them, so its
//! Flat Pattern shows them where they lie laid flat.
//!
//! - The cut's walls are the result's faces that are not the lofted
//!   flange's own. Their edges on the front of the sheet (every sample on
//!   the transition's front, within 0.1 µm) are laid into the chart: on a
//!   flat facet exactly, by the facet's own map (a circle stays a circle),
//!   elsewhere through the rulings' triangles, as a spline through 65 laid
//!   points with their tangents.
//! - The walls must run through the thickness at right angles to the
//!   sheet: every front edge moved the thickness along the material
//!   direction lies on one of the walls' edges on the back (within the
//!   import tolerance, 0.01 mm). A pocket, a slanted cut or material added
//!   is refused in words.
//! - The front edges join into closed loops, each taken out of the chart
//!   (a sheet trim keeping the chart's other curves). A cut that reaches
//!   the sheet's edge is not taken yet.
//! - The body stays the feature's result: the lofted flange's body is not
//!   built from its chart.

use mitcad_vec3::{add, distance, dot, scale, sub};

use super::*;
use crate::kernel::{Curve3, KernelError};

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

/// The distance from a point to a polyline.
fn to_polyline(p: V3, points: &[V3]) -> f64 {
    let mut best = f64::INFINITY;
    for w in points.windows(2) {
        let d = sub(w[1], w[0]);
        let l2 = dot(d, d);
        let s = if l2 > 0.0 {
            (dot(sub(p, w[0]), d) / l2).clamp(0.0, 1.0)
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
    let own = format!("{}:", panel.feature);
    let mut front: Vec<(Curve3, Vec<V3>)> = Vec::new();
    let mut back: Vec<Vec<V3>> = Vec::new();
    for (names, _) in &faces {
        let Some(name) = names.first() else {
            continue;
        };
        // The lofted flange's own faces; the cuts' walls are the others.
        if name.starts_with(&own) {
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
    if front.is_empty() {
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
    for (_, points) in &front {
        for p in points {
            let at = located(&l, *p, 0.0, slack).ok_or("a cut's edge lies off the sheet")?;
            let through = add(*p, scale(at.front_to_back, thickness));
            let off = back
                .iter()
                .map(|b| to_polyline(through, b))
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
    let curves = front
        .iter()
        .map(|(curve, points)| charted(&l, curve, points, slack))
        .collect::<Result<Vec<_>, _>>()?;
    let mut slot = 0;
    let tools = crate::sheet::rederive::regions(ctx.uid, &mut slot, curves).map_err(|_| {
        "a cut that reaches the edge of a lofted flange's sheet is not taken yet: its sheet \
         takes holes and cut-outs inside its edges"
            .to_owned()
    })?;
    super::super::trim(
        ctx,
        &mut state,
        &[domain],
        SheetTrimOperation::Difference,
        &tools,
        &[Isometry2::IDENTITY],
    )?;
    state.shape_generation.clear();
    Ok(state)
}
