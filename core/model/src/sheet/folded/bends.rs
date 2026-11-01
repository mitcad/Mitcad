// SPDX-License-Identifier: MIT
//! Tangent construction and material-side correspondence of native bends.

use mitcad_vec3::{add, distance, scale, sub};

use super::*;
use crate::features::sheet::{Allowance, Relief, ReliefKind};
use crate::features::sheet_folded::*;
use crate::sketch::geometry as g2;
use crate::transform::Transform;
use mitcad_vec3::dot;

pub(super) struct BendMetrics {
    pub(super) angle: f64,
    pub(super) radius: f64,
    pub(super) thickness: f64,
    pub(super) k: f64,
    pub(super) allowance: f64,
    pub(super) inner: f64,
    pub(super) outer: f64,
    /// How far the bend's outer surface reaches beyond its stationary
    /// tangent line, along the panel and against the wall's direction:
    /// `(r+t) sin(min(|angle|, 90 degrees))` (mitcad#242).
    pub(super) extent: f64,
}
impl BendMetrics {
    /// A bend of up to 180 degrees (a hem, mitcad#242). The virtual sharps
    /// (and so the inner and outer setbacks) are not defined at 180
    /// degrees; [`BendMetrics::setback`] refuses them there.
    pub(super) fn new(
        angle: f64,
        radius: Option<f64>,
        rules: &SheetRulesDef<f64>,
    ) -> Result<Self, String> {
        let radius = radius.unwrap_or(rules.bend_radius);
        if !angle.is_finite() || angle.abs() <= 1e-9 || angle.abs() > std::f64::consts::PI + 1e-12 {
            return Err("sheet bend angle must be nonzero and at most 180 degrees".into());
        }
        let angle = angle.clamp(-std::f64::consts::PI, std::f64::consts::PI);
        if !radius.is_finite() || radius <= TOLERANCE {
            return Err("sheet inside radius must be positive".into());
        }
        let Allowance::KFactor { k } = &rules.allowance;
        let k = *k;
        let setback = (angle.abs() / 2.0).tan();
        Ok(Self {
            angle,
            radius,
            thickness: rules.thickness,
            k,
            allowance: angle.abs() * (radius + k * rules.thickness),
            inner: radius * setback,
            outer: (radius + rules.thickness) * setback,
            extent: (radius + rules.thickness) * angle.abs().min(std::f64::consts::FRAC_PI_2).sin(),
        })
    }
    /// The inner or outer setback, refused where the virtual sharp does
    /// not exist (a bend of 180 degrees).
    pub(super) fn setback(&self, value: f64, what: &str) -> Result<f64, String> {
        if self.angle.abs() >= std::f64::consts::PI - 1e-9 || !value.is_finite() {
            return Err(format!(
                "a 180-degree bend has no {what} virtual sharp; measure it from the tangent or \
                 the outer tangent"
            ));
        }
        Ok(value)
    }
    fn offset(&self, position: SheetPosition) -> Result<f64, String> {
        Ok(match position {
            SheetPosition::Inside => -self.setback(self.outer, "outer")?,
            SheetPosition::Outside => -self.setback(self.inner, "inner")?,
            SheetPosition::BendLine => -self.allowance / 2.0,
            SheetPosition::Edge => 0.0,
            SheetPosition::SideTangent => -self.extent,
        })
    }
}

/// Axis follows the canonical boundary orientation e x m = material normal.
pub(super) struct PortFrame {
    pub(super) origin: [f64; 3],
    pub(super) m: [f64; 3],
    pub(super) e: [f64; 3],
    pub(super) n: [f64; 3],
    pub(super) to_operation: Isometry2,
    pub(super) width: f64,
}

pub(super) fn port_frame(state: &SheetState, reference: &BoundaryRef) -> Result<PortFrame, String> {
    let edge = boundary(state, reference)?;
    let (a, b) = line(edge)?;
    let patch = chart(state, reference.panel)?;
    let (frame, parity) = plane(patch)?;
    let along = g2::unit(g2::sub(b, a));
    let m = [along[1], -along[0]];
    let e = g2::scale(along, -parity.sign());
    let start = if parity == MaterialParity::Positive {
        b
    } else {
        a
    };
    Ok(PortFrame {
        origin: frame.point(start),
        m: add(scale(frame.x_axis, m[0]), scale(frame.y_axis, m[1])),
        e: add(scale(frame.x_axis, e[0]), scale(frame.y_axis, e[1])),
        n: scale(frame.normal(), parity.sign()),
        to_operation: Isometry2 {
            origin: [-g2::dot(start, m), -g2::dot(start, e)],
            x_axis: [m[0], e[0]],
            y_axis: [m[1], e[1]],
        },
        width: g2::dist(a, b),
    })
}

fn free(state: &SheetState, reference: &BoundaryRef) -> Result<(), String> {
    let edge = boundary(state, reference)?;
    if state.joins.iter().any(|p| {
        [&p.first, &p.second].iter().any(|s| {
            s.boundary.domain == SheetDomainRef::Panel(reference.panel)
                && s.boundary.piece == edge.piece
        })
    }) {
        return Err("sheet flange boundary is already joined to another material domain".into());
    }
    Ok(())
}

fn extent(patch: &SheetPatch, map: Isometry2) -> Result<([f64; 2], [f64; 2]), String> {
    let (a, b) = bounds(patch)?;
    let points = [a, [a[0], b[1]], b, [b[0], a[1]]].map(|p| map.point(p));
    Ok((
        std::array::from_fn(|i| points.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min)),
        std::array::from_fn(|i| {
            points
                .iter()
                .map(|p| p[i])
                .fold(f64::NEG_INFINITY, f64::max)
        }),
    ))
}

fn margin(min: [f64; 2], max: [f64; 2]) -> f64 {
    1.0 + 10.0 * ((max[0] - min[0]).abs() + (max[1] - min[1]).abs())
}

/// The boundary edges of a chart.
fn edges_of(patch: &SheetPatch) -> impl Iterator<Item = &MaterialEdge> {
    patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
}

/// A bend whose seam with a panel a flange's trim reaches: which end of
/// the trim's axial interval it lies at, and its developed width.
struct Crossed {
    bend: BendId,
    upper: bool,
    allowance: f64,
}

/// The bends seamed to `panel` along a boundary the trim rectangle `min`
/// to `max` (operation coordinates of `to_operation`) reaches beyond the
/// modelling tolerance.
fn crossed_bends(
    state: &SheetState,
    panel: PanelId,
    to_operation: Isometry2,
    min: [f64; 2],
    max: [f64; 2],
) -> Vec<Crossed> {
    let mut out: Vec<Crossed> = Vec::new();
    for port in &state.joins {
        for (here, there) in [(&port.first, &port.second), (&port.second, &port.first)] {
            if here.boundary.domain != SheetDomainRef::Panel(panel) {
                continue;
            }
            let SheetDomainRef::Bend(bend) = there.boundary.domain else {
                continue;
            };
            let Some((a, b)) = port_ends(state, here) else {
                continue;
            };
            let (a, b) = (to_operation.point(a), to_operation.point(b));
            let inside = |p: [f64; 2]| {
                p[0] > min[0] + TOLERANCE
                    && p[0] < max[0]
                    && p[1] > min[1] - TOLERANCE
                    && p[1] < max[1] + TOLERANCE
            };
            let reached = (0..=64).any(|k| {
                let t = f64::from(k) / 64.0;
                inside([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])])
            });
            let Some(record) = state.bends.iter().find(|r| r.id == bend) else {
                continue;
            };
            if reached && !out.iter().any(|c| c.bend == bend) {
                let middle = (a[1] + b[1]) / 2.0;
                out.push(Crossed {
                    bend,
                    upper: middle > (min[1] + max[1]) / 2.0,
                    allowance: record.allowance,
                });
            }
        }
    }
    out
}

pub(super) fn side(
    patch: &SheetPatch,
    piece: &CurvePieceRef,
    to_axial: Isometry2,
) -> Result<ShapePortSide, String> {
    let edges: Vec<_> = patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .filter(|e| &e.piece == piece)
        .collect();
    if edges.len() != 1 {
        return Err("tangent port must be one unique uninterrupted straight boundary".into());
    }
    port_side(patch, edges[0], to_axial)
}

/// Every stretch of a straight boundary curve of a chart (each its own
/// piece, [`number_stretches`]), in the order of their axial coordinates:
/// one for an uninterrupted boundary, several where holes or cut windows
/// cross the line (a fold across them, mitcad#288).
fn sides(
    patch: &SheetPatch,
    piece: &CurvePieceRef,
    to_axial: Isometry2,
) -> Result<Vec<ShapePortSide>, String> {
    let mut out = patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .filter(|e| e.piece.curve == piece.curve)
        .map(|e| port_side(patch, e, to_axial))
        .collect::<Result<Vec<_>, String>>()?;
    if out.is_empty() {
        return Err("tangent port must be one unique uninterrupted straight boundary".into());
    }
    out.sort_by(|a, b| coverage(a)[0].total_cmp(&coverage(b)[0]));
    Ok(out)
}

/// Gives the stretches of the straight boundary `piece` of a domain's chart
/// (a fold line across holes or cut windows, mitcad#288) distinct piece
/// numbers in the order of their axial coordinates, as a developed cut
/// numbers its fragments: each seam then names its own stretch. The
/// lineage records of the stretches follow, keeping the new piece as an
/// alias.
fn number_stretches(
    state: &mut SheetState,
    domain: SheetDomainRef,
    piece: &CurvePieceRef,
    to_axial: Isometry2,
) -> Result<(), String> {
    let patch = state
        .charts
        .iter_mut()
        .find(|p| p.domain == domain)
        .ok_or("a fold's material domain is unavailable")?;
    let mut stretches = Vec::new();
    for (r, region) in patch.regions.iter().enumerate() {
        for (l, lp) in region.loops.iter().enumerate() {
            for (e, edge) in lp.edges.iter().enumerate() {
                if &edge.piece == piece {
                    let (a, b) = line(edge)?;
                    let middle = (to_axial.point(a)[1] + to_axial.point(b)[1]) / 2.0;
                    stretches.push((middle, r, l, e));
                }
            }
        }
    }
    if stretches.len() < 2 {
        return Ok(());
    }
    stretches.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (k, (_, r, l, e)) in stretches.into_iter().enumerate() {
        let edge = &mut patch.regions[r].loops[l].edges[e];
        let generation = CurveLineage::generation(&edge.curve);
        let renamed = CurvePieceRef {
            curve: piece.curve.clone(),
            piece: k as u32,
        };
        edge.piece = renamed.clone();
        for record in state.lineage.iter_mut().filter(|record| {
            record.boundary.domain == domain
                && &record.boundary.piece == piece
                && record.curve_generation == generation
        }) {
            record.boundary.piece = renamed.clone();
            if !record.aliases.contains(&renamed) {
                record.aliases.push(renamed.clone());
                record
                    .aliases
                    .sort_by_key(|p| serde_json::to_string(p).expect("curve identity"));
            }
        }
    }
    Ok(())
}

/// The axial interval a port side covers, lower end first.
fn coverage(s: &ShapePortSide) -> [f64; 2] {
    let a = s.axial.offset + s.axial.scale * s.span.first;
    let b = s.axial.offset + s.axial.scale * s.span.last;
    [a.min(b), a.max(b)]
}

fn port_side(
    patch: &SheetPatch,
    edge: &MaterialEdge,
    to_axial: Isometry2,
) -> Result<ShapePortSide, String> {
    let (a, b) = line(edge)?;
    let first = to_axial.point(a)[1];
    let last = to_axial.point(b)[1];
    let range = edge.curve.range;
    let derivative = (last - first) / (range.last - range.first);
    Ok(ShapePortSide {
        boundary: BoundaryPieceRef {
            domain: patch.domain,
            piece: edge.piece.clone(),
        },
        span: range,
        axial: AxialMap {
            offset: first - derivative * range.first,
            scale: derivative,
        },
    })
}

fn pair(
    state: &mut SheetState,
    first: ShapePortSide,
    second: ShapePortSide,
    width: f64,
) -> Result<(), String> {
    for values in [coverage(&first), coverage(&second)] {
        if values[0].abs() > TOLERANCE || (values[1] - width).abs() > TOLERANCE {
            return Err("tangent ports do not cover the same full axial interval".into());
        }
    }
    push_port(state, [0.0, width], first, second);
    Ok(())
}

fn push_port(
    state: &mut SheetState,
    interval: [f64; 2],
    first: ShapePortSide,
    second: ShapePortSide,
) {
    let token = state
        .joins
        .iter()
        .map(|p| p.token)
        .max()
        .map_or(0, |v| v + 1);
    state.joins.push(ShapePort {
        token,
        axial_interval: ParamSpan {
            first: interval[0],
            last: interval[1],
        },
        first,
        second,
    });
}

/// Seams between the stretches of two boundaries along one line: a single
/// stretch each must cover the whole axial interval `[0, width]` (as
/// [`pair`]); stretches interrupted by holes or cut windows (a fold across
/// them, mitcad#288) pair one to one, each pair covering the same part of
/// it, one seam per stretch.
fn pair_all(
    state: &mut SheetState,
    first: Vec<ShapePortSide>,
    second: Vec<ShapePortSide>,
    width: f64,
) -> Result<(), String> {
    if first.len() == 1 && second.len() == 1 {
        let (Some(a), Some(b)) = (first.into_iter().next(), second.into_iter().next()) else {
            unreachable!()
        };
        return pair(state, a, b, width);
    }
    if first.len() != second.len() {
        return Err("the stretches of a bend's seam do not pair up".into());
    }
    for (a, b) in first.into_iter().zip(second) {
        let (ca, cb) = (coverage(&a), coverage(&b));
        if (ca[0] - cb[0]).abs() > TOLERANCE
            || (ca[1] - cb[1]).abs() > TOLERANCE
            || ca[0] < -TOLERANCE
            || ca[1] > width + TOLERANCE
            || ca[1] - ca[0] <= TOLERANCE
        {
            return Err(
                "the stretches of a bend's seam do not cover the same axial intervals".into(),
            );
        }
        push_port(state, ca, a, b);
    }
    Ok(())
}

pub(super) struct Construction {
    pub(super) cylinder: CylinderEmbedding,
    pub(super) moving: SheetEmbedding,
}
pub(super) fn construction(
    frame: &PortFrame,
    tangent: f64,
    axial_start: f64,
    metrics: &BendMetrics,
) -> Construction {
    let p = add(
        add(frame.origin, scale(frame.m, tangent)),
        scale(frame.e, axial_start),
    );
    let sign = metrics.angle.signum();
    let centre = add(
        p,
        scale(
            frame.n,
            if sign > 0.0 {
                metrics.radius + metrics.thickness
            } else {
                -metrics.radius
            },
        ),
    );
    let a = scale(frame.n, -sign);
    let theta = metrics.angle.abs();
    let radial = add(scale(a, theta.cos()), scale(frame.m, theta.sin()));
    let front_radius = if sign > 0.0 {
        metrics.radius + metrics.thickness
    } else {
        metrics.radius
    };
    let tangent_direction = add(
        scale(frame.m, theta.cos()),
        scale(frame.n, sign * theta.sin()),
    );
    Construction {
        cylinder: CylinderEmbedding {
            centre,
            axis: frame.e,
            radial_start: a,
            angular_direction: frame.m,
            inside_radius: metrics.radius,
            neutral_radius: metrics.radius + metrics.k * metrics.thickness,
            angle_extent: theta,
            inner_is_front: sign < 0.0,
        },
        moving: SheetEmbedding::Plane {
            front_frame: SketchFrame {
                origin: add(centre, scale(radial, front_radius)),
                x_axis: tangent_direction,
                y_axis: frame.e,
            },
            front_to_back_sign: MaterialParity::Negative,
        },
    }
}

fn relief_tool(
    feature: FeatureUid,
    slot: u32,
    relief: &Relief<f64>,
    tangent: f64,
    end: f64,
    upper: bool,
    outside: f64,
) -> Result<MaterialRegion, String> {
    if [relief.width, relief.depth]
        .iter()
        .any(|v| !v.is_finite() || *v <= TOLERANCE)
    {
        return Err("sheet endpoint relief requires positive width and depth".into());
    }
    let (v0, v1) = if upper {
        (end, end + relief.width)
    } else {
        (end - relief.width, end)
    };
    let back = tangent - relief.depth;
    match relief.kind {
        ReliefKind::Rectangular | ReliefKind::Tear => {
            rectangle(feature, slot, [back, v0], [outside, v1])
        }
        ReliefKind::Round => {
            let radius = relief.width / 2.0;
            if relief.depth < radius + TOLERANCE {
                return Err("round sheet relief depth must include its semicircular tip".into());
            }
            let centre = [back + radius, (v0 + v1) / 2.0];
            region(
                feature,
                slot,
                vec![
                    SegmentGeometry::Line {
                        start: [outside, v0],
                        end: [outside, v1],
                    },
                    SegmentGeometry::Line {
                        start: [outside, v1],
                        end: [centre[0], v1],
                    },
                    SegmentGeometry::Arc {
                        center: centre,
                        radius,
                        start_angle: std::f64::consts::FRAC_PI_2,
                        end_angle: 3.0 * std::f64::consts::FRAC_PI_2,
                    },
                    SegmentGeometry::Line {
                        start: [centre[0], v0],
                        end: [outside, v0],
                    },
                ],
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_strip<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    state: &mut SheetState,
    metrics: &BendMetrics,
    construction: Construction,
    moving: SheetPatch,
    stationary: Vec<ShapePortSide>,
    moving_side: Vec<ShapePortSide>,
    width: f64,
    strip_regions: Option<Vec<MaterialRegion>>,
) -> Result<(), String> {
    add_bend_strip(
        ctx,
        state,
        metrics,
        construction.cylinder,
        stationary,
        moving_side,
        width,
        strip_regions,
        0,
    )?;
    state.charts.push(moving);
    Ok(())
}

/// A bend of the feature (its `slot`) on `cylinder` between two port
/// sides: its record, its strip (generated curves `40 + 100 slot` to `43 +
/// 100 slot`) and its two seams (each one seam per stretch where holes or
/// cut windows interrupt the bend's lines, mitcad#288).
#[allow(clippy::too_many_arguments)]
pub(super) fn add_bend_strip<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    state: &mut SheetState,
    metrics: &BendMetrics,
    cylinder: CylinderEmbedding,
    stationary: Vec<ShapePortSide>,
    moving_side: Vec<ShapePortSide>,
    width: f64,
    strip_regions: Option<Vec<MaterialRegion>>,
    slot: u32,
) -> Result<(), String> {
    let id = BendId {
        feature: ctx.uid,
        slot,
    };
    let curves = 40 + 100 * slot;
    let strip = SheetPatch {
        domain: SheetDomainRef::Bend(id),
        regions: strip_regions.unwrap_or(vec![rectangle(
            ctx.uid,
            curves,
            [0.0, 0.0],
            [metrics.allowance, width],
        )?]),
        embedding: SheetEmbedding::Cylinder(cylinder.clone()),
    };
    let left = sides(&strip, &generated(ctx.uid, curves + 3), Isometry2::IDENTITY)?;
    let right = sides(&strip, &generated(ctx.uid, curves + 1), Isometry2::IDENTITY)?;
    let (Some(first), Some(last)) = (stationary.first(), moving_side.first()) else {
        return Err("tangent port must be one unique uninterrupted straight boundary".into());
    };
    state.bends.push(BendDomain {
        id,
        stationary: first.boundary.clone(),
        moving: last.boundary.clone(),
        folded_embedding: cylinder,
        signed_angle: metrics.angle,
        k_factor: metrics.k,
        radius: metrics.radius,
        allowance: metrics.allowance,
        axial_interval: ParamSpan {
            first: 0.0,
            last: width,
        },
        rules: state.rules,
        rules_version: state.rules_version.clone(),
    });
    pair_all(state, stationary, left, width)?;
    pair_all(state, right, moving_side, width)?;
    state.charts.push(strip);
    Ok(())
}

/// The distances the flange leaves free at the start and the end of the
/// selected boundary (along the canonical axial direction): its `offsets`,
/// or those its `width` gives on a boundary of length `edge` (mitcad#242).
/// A negative distance is an overhang: the wall passes that end of the
/// boundary (only a width measured from the middle gives one).
fn flange_span(d: &SheetFlangeDef<f64>, edge: f64) -> Result<(f64, f64), String> {
    let (start, end) = match (&d.offsets, &d.width) {
        (Some(_), Some(_)) => {
            return Err("a sheet flange takes offsets or a width, not both".into());
        }
        (Some(o), None) => {
            if [o.start, o.end].iter().any(|v| !v.is_finite() || *v < 0.0) {
                return Err("sheet flange offsets must not be negative".into());
            }
            (o.start, o.end)
        }
        (None, None) => (0.0, 0.0),
        (None, Some(SheetFlangeWidth::Middle { before, after, .. })) => {
            if !(before + after).is_finite() || before + after <= TOLERANCE {
                return Err("a sheet flange width must be positive".into());
            }
            (edge / 2.0 - before, edge / 2.0 - after)
        }
    };
    if !start.is_finite() || !end.is_finite() || start.max(0.0) + end.max(0.0) >= edge - TOLERANCE {
        return Err(format!(
            "sheet flange offsets or width leave no positive axial width on its {edge:.4} mm \
             boundary"
        ));
    }
    // Overhangs within the modelling tolerance are none.
    let clean = |v: f64| if v.abs() <= TOLERANCE { 0.0 } else { v };
    Ok((clean(start), clean(end)))
}

/// A flange's wall in its own chart: `length` along the wall from its
/// tangent line, the bend's axial width `width`, and the overhangs past
/// the boundary's start and end, each cut back at the root by `root`. The
/// root edge along the bend is always `g53` (the bend's moving port).
fn wall_region(
    feature: FeatureUid,
    length: f64,
    width: f64,
    overhang: (f64, f64),
    root: f64,
) -> Result<MaterialRegion, String> {
    let (before, after) = overhang;
    if before <= 0.0 && after <= 0.0 {
        return rectangle(feature, 50, [0.0, 0.0], [length, width]);
    }
    if root >= length - TOLERANCE {
        return Err("the flange's wall is shorter than the relief cut back beside its bend".into());
    }
    let line = |slot: u32, start: [f64; 2], end: [f64; 2]| SheetSegment {
        id: SheetCurveId::Generated { feature, slot },
        geometry: SegmentGeometry::Line { start, end },
    };
    let mut segments = Vec::new();
    // (A root of 0: the overhang's root edge goes on from the bend's port.)
    let flush = root <= TOLERANCE;
    let root = if flush { 0.0 } else { root };
    if before > 0.0 {
        if !flush {
            segments.push(line(54, [0.0, 0.0], [root, 0.0]));
        }
        segments.push(line(55, [root, 0.0], [root, -before]));
        segments.push(line(50, [root, -before], [length, -before]));
    } else {
        segments.push(line(50, [0.0, 0.0], [length, 0.0]));
    }
    let top = width + after.max(0.0);
    segments.push(line(51, [length, -before.max(0.0)], [length, top]));
    if after > 0.0 {
        segments.push(line(52, [length, top], [root, top]));
        segments.push(line(56, [root, top], [root, width]));
        if !flush {
            segments.push(line(57, [root, width], [0.0, width]));
        }
    } else {
        segments.push(line(52, [length, top], [0.0, width]));
    }
    segments.push(line(53, [0.0, width], [0.0, 0.0]));
    MaterialRegion::from_legacy(&SheetRegion {
        loops: vec![SheetLoop { segments }],
    })
}

/// Cuts a panel back from its `reference` boundary to the line `tangent`
/// of the boundary's port frame (negative: into the material), between the
/// axial distances `start` and `width - end`, with the relief tools
/// `reliefs` gives for the far side of the cut. Bends already on a
/// neighbouring boundary whose seam the cut reaches (flanges on adjacent
/// edges, a flange on the side edge of a wall) are cut too: the cut goes
/// on across the seam through each such bend, developed, so that the bend
/// ends where the panel now ends (a rectangular corner relief of the cut's
/// depth), mitcad#211. The piece of the new boundary (the feature's
/// generated curve 13), or the boundary itself where the cut removes
/// nothing (`at_edge`).
#[allow(clippy::too_many_arguments)]
fn cut_back<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    thickness: f64,
    reference: &BoundaryRef,
    frame: &PortFrame,
    tangent: f64,
    (start, end): (f64, f64),
    reliefs: impl FnOnce(f64) -> Result<Vec<MaterialRegion>, String>,
    at_edge: bool,
    beyond: Option<f64>,
    what: &str,
) -> Result<CurvePieceRef, String> {
    let panel = reference.panel;
    let (min, max) = extent(chart(state, panel)?, frame.to_operation)?;
    let far = beyond.unwrap_or_else(|| max[0] + margin(min, max));
    let mut tools = vec![rectangle(
        ctx.uid,
        10,
        [tangent, start],
        [far, frame.width - end],
    )?];
    tools.extend(reliefs(far)?);
    let crossed = crossed_bends(
        state,
        panel,
        frame.to_operation,
        [tangent, start],
        [far, frame.width - end],
    );
    if crossed.is_empty() {
        trim(
            ctx,
            state,
            &[SheetDomainRef::Panel(panel)],
            SheetTrimOperation::Difference,
            &tools,
            &[frame.to_operation],
        )?;
    } else {
        let lower = crossed.iter().filter(|c| !c.upper).map(|c| c.allowance);
        let upper = crossed.iter().filter(|c| c.upper).map(|c| c.allowance);
        let below = lower.fold(0.0, f64::max);
        let above = upper.fold(0.0, f64::max);
        tools[0] = rectangle(
            ctx.uid,
            10,
            [tangent, start - below],
            [far, frame.width - end + above],
        )?;
        let to_chart = super::develop::inverse(frame.to_operation);
        let chart_tools = mapped_regions(&tools, to_chart);
        let bends: Vec<BendId> = crossed.iter().map(|c| c.bend).collect();
        let outcome =
            super::cut::developed_cut(ctx, state, thickness, panel, &bends, &chart_tools, what)?;
        *state = outcome.state;
    }
    // (Across a bend, the cut's line has a piece on each domain.)
    Ok(
        match edges_of(chart(state, panel)?).find(|e| e.piece.curve == generated(ctx.uid, 13).curve)
        {
            Some(e) => e.piece.clone(),
            None if at_edge => boundary(state, reference)?.piece.clone(),
            None => generated(ctx.uid, 13),
        },
    )
}

pub(crate) fn flange<K: Kernel>(
    d: &SheetFlangeDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    free(&state, &d.boundary)?;
    let metrics = BendMetrics::new(d.angle, d.radius, &rules)?;
    let length = d.length.value
        - match d.length.mode {
            SheetLengthMode::Tangent => 0.0,
            SheetLengthMode::Inner => metrics.setback(metrics.inner, "inner")?,
            SheetLengthMode::Outer => metrics.setback(metrics.outer, "outer")?,
            SheetLengthMode::OuterTangent => metrics.extent,
        };
    if !length.is_finite() || length <= TOLERANCE {
        return Err("sheet flange length is shorter than its requested setback".into());
    }
    let frame = port_frame(&state, &d.boundary)?;
    // The bend covers the boundary between the flange's ends; a wall wider
    // than the boundary overhangs past them (mitcad#242).
    let (lo, hi) = flange_span(d, frame.width)?;
    let (start, end) = (lo.max(0.0), hi.max(0.0));
    let overhang = (-lo.min(0.0), -hi.min(0.0));
    let width = frame.width - start - end;
    let tangent = metrics.offset(d.position)?;
    let relief = d.relief.as_ref().unwrap_or(&rules.bend_relief);
    // Where the overhangs start along the wall: cut back by the relief
    // depth, or by the width's own `root` (mitcad#242).
    let root = match &d.width {
        Some(SheetFlangeWidth::Middle { root: Some(r), .. }) => {
            if !r.is_finite() || *r < 0.0 {
                return Err("a sheet flange's overhang root must not be negative".into());
            }
            *r
        }
        _ => relief.depth,
    };
    let uid = ctx.uid;
    // (At the edge the trim removes nothing: the boundary is the port.)
    let tangent_piece = cut_back(
        ctx,
        &mut state,
        rules.thickness,
        &d.boundary,
        &frame,
        tangent,
        (start, end),
        |far| {
            let mut tools = Vec::new();
            if start > TOLERANCE {
                tools.push(relief_tool(uid, 20, relief, tangent, start, false, far)?);
            }
            if end > TOLERANCE {
                tools.push(relief_tool(
                    uid,
                    30,
                    relief,
                    tangent,
                    frame.width - end,
                    true,
                    far,
                )?);
            }
            Ok(tools)
        },
        d.position == SheetPosition::Edge,
        None,
        "the flange's trim",
    )?;
    let mut axial = frame.to_operation;
    axial.origin[1] -= start;
    let stationary = side(chart(&state, d.boundary.panel)?, &tangent_piece, axial)?;
    let constructed = construction(&frame, tangent, start, &metrics);
    let moving = SheetPatch {
        domain: SheetDomainRef::Panel(PanelId {
            feature: ctx.uid,
            slot: 0,
        }),
        regions: vec![wall_region(ctx.uid, length, width, overhang, root)?],
        embedding: constructed.moving.clone(),
    };
    let moving_side = side(&moving, &generated(ctx.uid, 53), Isometry2::IDENTITY)?;
    add_strip(
        ctx,
        &mut state,
        &metrics,
        constructed,
        moving,
        vec![stationary],
        vec![moving_side],
        width,
        None,
    )?;
    publish(ctx, state, &rules, None)
}

pub(crate) fn bend<K: Kernel>(
    d: &SheetBendDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    // A bend moves material the straight cuts it keeps were made through.
    state.overlay_refusal("sheet Bend")?;
    if d.resolved_mode() == BendPortMode::Double {
        return double_bend(d, ctx, state, rules);
    }
    let metrics = BendMetrics::new(d.angle, d.radius, &rules)?;
    let moving_body = d.moving_body.unwrap_or(d.body);
    if moving_body == d.body {
        if d.resolved_mode() != BendPortMode::VirtualSharp {
            return Err("same-body sheet bend requires a virtual-sharp seam".into());
        }
        virtual_bend(ctx, &mut state, &rules, &metrics, &d.stationary, &d.moving)?;
        publish(ctx, state, &rules, None)
    } else {
        let (moving, moving_rules) = read(ctx, moving_body)?;
        if d.resolved_mode() != BendPortMode::TangentPorts {
            return Err("separate sheet bodies require tangent-port mode".into());
        }
        if moving.owner != state.owner || moving.rules != state.rules || moving_rules != rules {
            return Err("sheet Bend bodies must have the same owner and rule reference".into());
        }
        positioned_bend(
            ctx,
            &mut state,
            moving,
            &rules,
            &metrics,
            &d.stationary,
            &d.moving,
        )?;
        publish(ctx, state, &rules, Some(moving_body))
    }
}

pub(crate) fn fold<K: Kernel>(
    d: &SheetFoldDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    state.overlay_refusal("sheet Fold")?;
    let metrics = BendMetrics::new(d.angle, d.radius, &rules)?;
    let sketch = ctx.sketch(d.line.sketch)?;
    let crate::sketch::geometry::Curve2::Line { a, b } =
        crate::features::sketch::curve_of(&sketch.solved, d.line.curve)
            .ok_or("fold sketch line is unavailable")?
    else {
        return Err("sheet Fold requires a straight sketch line".into());
    };
    let patch = chart(&state, d.panel)?.clone();
    let (frame, parity) = plane(&patch)?;
    let world_a = sketch.frame.point(*a);
    let world_b = sketch.frame.point(*b);
    // On the front or the back surface of the panel
    // (mitcad#280: the line lies across the thickness at the same place).
    let material = scale(frame.normal(), parity.sign());
    let depth = |p: [f64; 3]| dot(sub(p, frame.origin), material);
    let (da, db) = (depth(world_a), depth(world_b));
    if (da - db).abs() > TOLERANCE
        || (da.abs() > TOLERANCE && (da - rules.thickness).abs() > TOLERANCE)
    {
        return Err("fold line is not on a surface of the selected panel".into());
    }
    let project = |p: [f64; 3]| {
        let delta = sub(p, frame.origin);
        [dot(delta, frame.x_axis), dot(delta, frame.y_axis)]
    };
    let a = project(world_a);
    let b = project(world_b);
    let along = g2::unit(g2::sub(b, a));
    let m = if d.stationary_side == StationarySide::Left {
        [along[1], -along[0]]
    } else {
        [-along[1], along[0]]
    };
    let e = g2::scale(g2::perp(m), -parity.sign());
    let start = if g2::dot(g2::sub(b, a), e) > 0.0 {
        a
    } else {
        b
    };
    let port = PortFrame {
        origin: frame.point(start),
        m: add(scale(frame.x_axis, m[0]), scale(frame.y_axis, m[1])),
        e: add(scale(frame.x_axis, e[0]), scale(frame.y_axis, e[1])),
        n: scale(frame.normal(), parity.sign()),
        to_operation: Isometry2 {
            origin: [-g2::dot(start, m), -g2::dot(start, e)],
            x_axis: [m[0], e[0]],
            y_axis: [m[1], e[1]],
        },
        width: g2::dist(a, b),
    };
    fold_panel(
        ctx,
        &mut state,
        &rules,
        &metrics,
        d.panel,
        &port,
        d.position,
        d.relief.as_ref().unwrap_or(&rules.bend_relief),
    )?;
    publish(ctx, state, &rules, None)
}

// Implementations below preserve semantic chart identity while moving only
// the graph branch on the moving side of the selected seam.

fn branch(
    state: &SheetState,
    start: SheetDomainRef,
    fixed: SheetDomainRef,
    skip_pair: bool,
) -> Result<BTreeSet<SheetDomainRef>, String> {
    let mut reached = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(domain) = pending.pop() {
        if !reached.insert(domain) {
            continue;
        }
        if domain == fixed {
            return Err("sheet graph has a second path to the stationary panel".into());
        }
        for port in &state.joins {
            let a = port.first.boundary.domain;
            let b = port.second.boundary.domain;
            if skip_pair && ((a == start && b == fixed) || (b == start && a == fixed)) {
                continue;
            }
            if a == domain {
                pending.push(b);
            } else if b == domain {
                pending.push(a);
            }
        }
    }
    Ok(reached)
}

fn move_branch(state: &mut SheetState, domains: &BTreeSet<SheetDomainRef>, transform: &Transform) {
    for p in &mut state.charts {
        if domains.contains(&p.domain) {
            p.embedding = p.embedding.transformed(transform);
        }
    }
    for b in &mut state.bends {
        if domains.contains(&SheetDomainRef::Bend(b.id))
            && let SheetEmbedding::Cylinder(c) =
                SheetEmbedding::Cylinder(b.folded_embedding.clone()).transformed(transform)
        {
            b.folded_embedding = c;
        }
    }
}

fn moving_transform(
    frame: &PortFrame,
    tangent: f64,
    axial: f64,
    construction: &Construction,
) -> Transform {
    let source = SketchFrame {
        origin: add(
            add(frame.origin, scale(frame.m, tangent)),
            scale(frame.e, axial),
        ),
        x_axis: frame.m,
        y_axis: frame.e,
    };
    let SheetEmbedding::Plane {
        front_frame: target,
        ..
    } = &construction.moving
    else {
        unreachable!()
    };
    Transform::between_frames(&source, target)
}

#[allow(clippy::too_many_arguments)]
fn fold_panel<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    rules: &SheetRulesDef<f64>,
    metrics: &BendMetrics,
    panel: PanelId,
    frame: &PortFrame,
    position: SheetPosition,
    relief: &Relief<f64>,
) -> Result<(), String> {
    let old = chart(state, panel)?.clone();
    let stationary_domain = old.domain;
    let moving_domain = SheetDomainRef::Panel(PanelId {
        feature: ctx.uid,
        slot: 0,
    });
    let strip_domain = SheetDomainRef::Bend(BendId {
        feature: ctx.uid,
        slot: 0,
    });
    let tangent = metrics.offset(position)?;
    let far_tangent = tangent + metrics.allowance;
    let (min, max) = extent(&old, frame.to_operation)?;
    let pad = margin(min, max);
    let outside = max[0] + pad;
    if min[1] < -TOLERANCE || max[1] > frame.width + TOLERANCE {
        if !relief.depth.is_finite() || relief.depth <= TOLERANCE {
            return Err("sheet endpoint relief must extend behind the tangent".into());
        }
        if !relief.width.is_finite() || relief.width <= TOLERANCE {
            return Err("sheet endpoint relief requires a positive explicit clearance".into());
        }
        if relief.kind == ReliefKind::Round && relief.depth < relief.width / 2.0 + TOLERANCE {
            return Err("round sheet relief depth must include its semicircular tip".into());
        }
    }
    let mut moving = old.clone();
    moving.domain = moving_domain;
    let mut strip = old.clone();
    strip.domain = strip_domain;
    copy_lineage(
        &mut state.lineage,
        stationary_domain,
        moving_domain,
        &old.regions,
    );
    copy_lineage(
        &mut state.lineage,
        stationary_domain,
        strip_domain,
        &old.regions,
    );
    state.charts.extend([moving, strip]);
    let moving_tool = rectangle(ctx.uid, 50, [far_tangent, 0.0], [outside, frame.width])?;
    trim(
        ctx,
        state,
        &[moving_domain],
        SheetTrimOperation::Intersection,
        &[moving_tool],
        &[frame.to_operation],
    )?;
    let strip_tool = rectangle(ctx.uid, 40, [tangent, 0.0], [far_tangent, frame.width])?;
    trim(
        ctx,
        state,
        &[strip_domain],
        SheetTrimOperation::Intersection,
        &[strip_tool],
        &[frame.to_operation],
    )?;
    let mut removal = vec![rectangle(
        ctx.uid,
        10,
        [tangent, 0.0],
        [outside, frame.width],
    )?];
    if min[1] < -TOLERANCE {
        removal.push(relief_tool(
            ctx.uid, 20, relief, tangent, 0.0, false, outside,
        )?);
    }
    if max[1] > frame.width + TOLERANCE {
        removal.push(relief_tool(
            ctx.uid,
            30,
            relief,
            tangent,
            frame.width,
            true,
            outside,
        )?);
    }
    trim(
        ctx,
        state,
        &[stationary_domain],
        SheetTrimOperation::Difference,
        &removal,
        &[frame.to_operation],
    )?;
    for (domain, slot) in [
        (stationary_domain, 13),
        (moving_domain, 53),
        (strip_domain, 41),
        (strip_domain, 43),
    ] {
        number_stretches(state, domain, &generated(ctx.uid, slot), frame.to_operation)?;
    }
    let moving = state
        .charts
        .iter()
        .find(|p| p.domain == moving_domain)
        .expect("moving domain")
        .clone();
    let strip = state
        .charts
        .iter()
        .find(|p| p.domain == strip_domain)
        .expect("bend domain")
        .clone();
    let mut map = frame.to_operation;
    map.origin[0] -= tangent;
    let strip_regions = mapped_regions(&strip.regions, map);
    remap_lineage(&mut state.lineage, strip_domain, &strip.regions, map);
    // A line across holes or cut windows leaves the panels' new boundaries
    // in stretches: one seam each (mitcad#288).
    let stationary = sides(
        chart(state, panel)?,
        &generated(ctx.uid, 13),
        frame.to_operation,
    )?;
    let moving_side = sides(&moving, &generated(ctx.uid, 53), frame.to_operation)?;
    let constructed = construction(frame, tangent, 0.0, metrics);
    let transform = moving_transform(frame, far_tangent, 0.0, &constructed);
    // Classify existing ports using their original material boundaries. A
    // port crossing the new strip cannot be silently assigned to one branch.
    let mut moving_neighbors = Vec::new();
    for port in &mut state.joins {
        for (first, second) in [(&mut port.first, &mut port.second)] {
            let (own, other) = if first.boundary.domain == stationary_domain {
                (first, second)
            } else if second.boundary.domain == stationary_domain {
                (second, first)
            } else {
                continue;
            };
            let edges: Vec<_> = old
                .regions
                .iter()
                .flat_map(|r| &r.loops)
                .flat_map(|l| &l.edges)
                .filter(|e| e.piece == own.boundary.piece)
                .collect();
            if edges.len() != 1 {
                return Err("existing sheet port has ambiguous boundary provenance".into());
            }
            let (a, b) = line(edges[0])?;
            let a = frame.to_operation.point(a);
            let b = frame.to_operation.point(b);
            if a[0] >= far_tangent - TOLERANCE
                && b[0] >= far_tangent - TOLERANCE
                && a[1] >= -TOLERANCE
                && b[1] >= -TOLERANCE
                && a[1] <= frame.width + TOLERANCE
                && b[1] <= frame.width + TOLERANCE
            {
                own.boundary.domain = moving_domain;
                moving_neighbors.push(other.boundary.domain);
            } else if a[0] > tangent + TOLERANCE || b[0] > tangent + TOLERANCE {
                return Err("fold strip crosses an existing tangent port".into());
            }
        }
    }
    let mut moved = BTreeSet::new();
    for neighbor in moving_neighbors {
        moved.extend(branch(state, neighbor, stationary_domain, false)?);
    }
    moved.insert(moving_domain);
    move_branch(state, &moved, &transform);
    for bend in &mut state.bends {
        for boundary in [&mut bend.stationary, &mut bend.moving] {
            if boundary.domain == stationary_domain
                && moving
                    .regions
                    .iter()
                    .flat_map(|r| &r.loops)
                    .flat_map(|l| &l.edges)
                    .any(|e| e.piece == boundary.piece)
            {
                boundary.domain = moving_domain;
            }
        }
    }
    let moving = state
        .charts
        .iter()
        .find(|p| p.domain == moving_domain)
        .expect("moved panel")
        .clone();
    state
        .charts
        .retain(|p| p.domain != moving_domain && p.domain != strip_domain);
    add_strip(
        ctx,
        state,
        metrics,
        constructed,
        moving,
        stationary,
        moving_side,
        frame.width,
        Some(strip_regions),
    )?;
    let _ = rules;
    Ok(())
}

fn virtual_bend<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    _rules: &SheetRulesDef<f64>,
    metrics: &BendMetrics,
    stationary_ref: &BoundaryRef,
    moving_ref: &BoundaryRef,
) -> Result<(), String> {
    if stationary_ref.panel == moving_ref.panel {
        return Err("sheet Bend needs two distinct labeled panel domains".into());
    }
    let stationary_old = chart(state, stationary_ref.panel)?.clone();
    let moving_old = chart(state, moving_ref.panel)?.clone();
    let frame = port_frame(state, stationary_ref)?;
    let moving_edge = boundary(state, moving_ref)?.clone();
    let (a, b) = line(&moving_edge)?;
    let (moving_frame, moving_parity) = plane(&moving_old)?;
    let end = add(frame.origin, scale(frame.e, frame.width));
    let a = moving_frame.point(a);
    let b = moving_frame.point(b);
    if !((distance(a, frame.origin) <= TOLERANCE && distance(b, end) <= TOLERANCE)
        || (distance(b, frame.origin) <= TOLERANCE && distance(a, end) <= TOLERANCE))
        || dot(scale(moving_frame.normal(), moving_parity.sign()), frame.n) < 1.0 - 1e-9
    {
        return Err("virtual-sharp Bend requires a full shared coplanar material seam".into());
    }
    let fixed = stationary_old.domain;
    let moving_domain = moving_old.domain;
    if state
        .joins
        .iter()
        .filter(|p| {
            (p.first.boundary.domain == fixed && p.second.boundary.domain == moving_domain)
                || (p.second.boundary.domain == fixed && p.first.boundary.domain == moving_domain)
        })
        .count()
        > 1
    {
        return Err("sheet graph has multiple seams between the selected panels".into());
    }
    let moved = branch(state, moving_domain, fixed, true)?;
    state.joins.retain(|p| {
        !((p.first.boundary.domain == fixed && p.second.boundary.domain == moving_domain)
            || (p.second.boundary.domain == fixed && p.first.boundary.domain == moving_domain))
    });
    let moving_map = Isometry2 {
        origin: [
            dot(sub(moving_frame.origin, frame.origin), frame.m),
            dot(sub(moving_frame.origin, frame.origin), frame.e),
        ],
        x_axis: [
            dot(moving_frame.x_axis, frame.m),
            dot(moving_frame.x_axis, frame.e),
        ],
        y_axis: [
            dot(moving_frame.y_axis, frame.m),
            dot(moving_frame.y_axis, frame.e),
        ],
    };
    let tangent = -metrics.allowance / 2.0;
    let far_tangent = metrics.allowance / 2.0;
    let (smin, smax) = extent(&stationary_old, frame.to_operation)?;
    let (mmin, mmax) = extent(&moving_old, moving_map)?;
    let pad = margin(smin, smax) + margin(mmin, mmax);
    let far = smax[0].max(mmax[0]) + pad;
    let strip_domain = SheetDomainRef::Bend(BendId {
        feature: ctx.uid,
        slot: 0,
    });
    let mut half = stationary_old.clone();
    half.domain = strip_domain;
    state.charts.push(half);
    copy_lineage(
        &mut state.lineage,
        fixed,
        strip_domain,
        &stationary_old.regions,
    );
    let strip_mask = rectangle(ctx.uid, 40, [tangent, 0.0], [far_tangent, frame.width])?;
    trim(
        ctx,
        state,
        &[strip_domain],
        SheetTrimOperation::Intersection,
        std::slice::from_ref(&strip_mask),
        &[frame.to_operation],
    )?;
    let mut first = state
        .charts
        .iter()
        .find(|p| p.domain == strip_domain)
        .expect("strip")
        .clone();
    let mut smap = frame.to_operation;
    smap.origin[0] -= tangent;
    remap_lineage(&mut state.lineage, strip_domain, &first.regions, smap);
    first.regions = mapped_regions(&first.regions, smap);
    state.charts.retain(|p| p.domain != strip_domain);
    let mut half = moving_old.clone();
    half.domain = strip_domain;
    state.charts.push(half);
    copy_lineage(
        &mut state.lineage,
        moving_domain,
        strip_domain,
        &moving_old.regions,
    );
    trim(
        ctx,
        state,
        &[strip_domain],
        SheetTrimOperation::Intersection,
        &[strip_mask],
        &[moving_map],
    )?;
    let half = state
        .charts
        .iter()
        .find(|p| p.domain == strip_domain)
        .expect("strip")
        .clone();
    let mut mmap = moving_map;
    mmap.origin[0] -= tangent;
    let tools = mapped_regions(&half.regions, mmap);
    remap_lineage(&mut state.lineage, strip_domain, &half.regions, mmap);
    state.charts.retain(|p| p.domain != strip_domain);
    state.charts.push(first);
    trim_with_lineage(
        ctx,
        state,
        &[strip_domain],
        SheetTrimOperation::Union,
        &tools,
        &[Isometry2::IDENTITY],
        Some(strip_domain),
    )?;
    let strip_regions = state
        .charts
        .iter()
        .find(|p| p.domain == strip_domain)
        .expect("strip")
        .regions
        .clone();
    state.charts.retain(|p| p.domain != strip_domain);
    trim(
        ctx,
        state,
        &[fixed],
        SheetTrimOperation::Difference,
        &[rectangle(
            ctx.uid,
            10,
            [tangent, -pad],
            [far, frame.width + pad],
        )?],
        &[frame.to_operation],
    )?;
    trim(
        ctx,
        state,
        &[moving_domain],
        SheetTrimOperation::Intersection,
        &[rectangle(
            ctx.uid,
            50,
            [far_tangent, -pad],
            [far, frame.width + pad],
        )?],
        &[moving_map],
    )?;
    let stationary = side(
        chart(state, stationary_ref.panel)?,
        &generated(ctx.uid, 13),
        frame.to_operation,
    )?;
    let moving_side = side(
        chart(state, moving_ref.panel)?,
        &generated(ctx.uid, 53),
        moving_map,
    )?;
    let constructed = construction(&frame, tangent, 0.0, metrics);
    let transform = moving_transform(&frame, far_tangent, 0.0, &constructed);
    move_branch(state, &moved, &transform);
    let moving = chart(state, moving_ref.panel)?.clone();
    state.charts.retain(|p| p.domain != moving_domain);
    add_strip(
        ctx,
        state,
        metrics,
        constructed,
        moving,
        vec![stationary],
        vec![moving_side],
        frame.width,
        Some(strip_regions),
    )
}

fn positioned_bend<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    mut moving: SheetState,
    _rules: &SheetRulesDef<f64>,
    metrics: &BendMetrics,
    stationary_ref: &BoundaryRef,
    moving_ref: &BoundaryRef,
) -> Result<(), String> {
    free(state, stationary_ref)?;
    free(&moving, moving_ref)?;
    let frame = port_frame(state, stationary_ref)?;
    let constructed = construction(&frame, 0.0, 0.0, metrics);
    let moving_patch = chart(&moving, moving_ref.panel)?.clone();
    let (moving_frame, parity) = plane(&moving_patch)?;
    let edge = boundary(&moving, moving_ref)?.clone();
    let (a, b) = line(&edge)?;
    let a = moving_frame.point(a);
    let b = moving_frame.point(b);
    let SheetEmbedding::Plane {
        front_frame: expected,
        front_to_back_sign,
    } = &constructed.moving
    else {
        unreachable!()
    };
    let end = add(expected.origin, scale(frame.e, frame.width));
    if !((distance(a, expected.origin) <= TOLERANCE && distance(b, end) <= TOLERANCE)
        || (distance(b, expected.origin) <= TOLERANCE && distance(a, end) <= TOLERANCE))
        || dot(
            scale(moving_frame.normal(), parity.sign()),
            scale(expected.normal(), front_to_back_sign.sign()),
        ) < 1.0 - 1e-9
    {
        return Err("positioned sheet tangent ports do not match the requested radius, angle and material sides".into());
    }
    let stationary = side(
        chart(state, stationary_ref.panel)?,
        &boundary(state, stationary_ref)?.piece,
        frame.to_operation,
    )?;
    let moving_axial = Isometry2 {
        origin: [0.0, dot(sub(moving_frame.origin, expected.origin), frame.e)],
        x_axis: [0.0, dot(moving_frame.x_axis, frame.e)],
        y_axis: [0.0, dot(moving_frame.y_axis, frame.e)],
    };
    let moving_side = side(&moving_patch, &edge.piece, moving_axial)?;
    for port in &mut moving.joins {
        port.token = state
            .joins
            .iter()
            .map(|p| p.token)
            .max()
            .map_or(0, |v| v + 1);
        state.joins.push(port.clone());
    }
    state.bends.extend(moving.bends);
    state.opened.extend(moving.opened);
    state.lineage.extend(moving.lineage);
    state.charts.extend(
        moving
            .charts
            .into_iter()
            .filter(|p| p.domain != moving_patch.domain),
    );
    add_strip(
        ctx,
        state,
        metrics,
        constructed,
        moving_patch,
        vec![stationary],
        vec![moving_side],
        frame.width,
        None,
    )
}

/// A sheet of planar panels with its front and back swapped (mitcad#280):
/// each chart's frame moves to the other surface and its material runs the
/// other way, so its material coordinates and boundaries stay. A sheet
/// with bends is refused (their inner sides would change too).
fn relabelled(mut s: SheetState, thickness: f64) -> Result<SheetState, String> {
    if !s.bends.is_empty() {
        return Err(
            "the moving body's front surface lies on the other side, and its bends cannot take \
             the stationary body's labels"
                .into(),
        );
    }
    for chart in &mut s.charts {
        let SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } = &mut chart.embedding
        else {
            return Err("the moving body has a domain that is not planar".into());
        };
        let n = scale(front_frame.normal(), front_to_back_sign.sign() * thickness);
        front_frame.origin = add(front_frame.origin, n);
        *front_to_back_sign = match front_to_back_sign {
            MaterialParity::Positive => MaterialParity::Negative,
            MaterialParity::Negative => MaterialParity::Positive,
        };
    }
    Ok(s)
}

/// How far (mm) the panels and boundaries a double bend joins may be from
/// where its geometry needs them: placements read from stored geometry
/// agree to rounding, far below it.
const PLACED: f64 = 1e-6;

/// A double bend (mitcad#280): two opposite bends of `|angle|` each (at
/// most 90 degrees) with the inside radius `r` and a straight panel between
/// them (a jog) join two parallel panels of separate bodies whose selected
/// straight boundaries face each other and cover the same axial interval,
/// their material on the same side (front surface to front surface). The
/// first bend starts on the fixed boundary (`fixed`, default the
/// stationary one), whose panel stays whole; with `h` the distance between
/// the two panels' planes and `rm = r + t/2` the bends' middle radius, the
/// straight between the bends is `L = (h - 2 rm (1 - cos a)) / sin a` long
/// and the second bend ends `X = 2 rm sin a + L cos a` from the fixed
/// boundary, where the other panel is cut back to (through the bends
/// seamed to it along the cut, developed, as a flange's trim). The moving
/// body is consumed. Between panels of one body the jog would close a loop
/// of material, which is refused.
fn double_bend<K: Kernel>(
    d: &SheetBendDef<f64>,
    ctx: &mut EvalContext<'_, K>,
    mut state: SheetState,
    rules: SheetRulesDef<f64>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let moving_body = d.moving_body.unwrap_or(d.body);
    if moving_body == d.body {
        return Err(
            "a double bend joins two separate sheet bodies: between panels of one body it \
             would close a loop of material, which the sheet model does not develop"
                .into(),
        );
    }
    let (mut moving, moving_rules) = read(ctx, moving_body)?;
    moving.overlay_refusal("sheet Bend")?;
    if moving.owner != state.owner || moving.rules != state.rules || moving_rules != rules {
        return Err("sheet Bend bodies must have the same owner and rule reference".into());
    }
    free(&state, &d.stationary)?;
    free(&moving, &d.moving)?;
    // The joined sheet has one front and back: a moving body whose front
    // surface lies on the other side takes the stationary body's labels.
    let normal = |s: &SheetState, r: &BoundaryRef| -> Result<[f64; 3], String> {
        let (frame, parity) = plane(chart(s, r.panel)?)?;
        Ok(scale(frame.normal(), parity.sign()))
    };
    if dot(normal(&state, &d.stationary)?, normal(&moving, &d.moving)?) < 0.0 {
        moving = relabelled(moving, rules.thickness)?;
    }
    let theta = d.angle.abs();
    if !theta.is_finite() || theta <= 1e-9 || theta > std::f64::consts::FRAC_PI_2 + 1e-12 {
        return Err("a double bend's angle must be more than 0 and at most 90 degrees".into());
    }
    let theta = theta.min(std::f64::consts::FRAC_PI_2);
    let radius = d.radius.unwrap_or(rules.bend_radius);
    let thickness = rules.thickness;
    // The fixed boundary (the first bend starts on it) and the one whose
    // panel is cut back.
    let fixed_moving = d.fixed == Some(DoubleBendFixed::Moving);
    let (fixed_ref, cut_ref) = if fixed_moving {
        (&d.moving, &d.stationary)
    } else {
        (&d.stationary, &d.moving)
    };
    let (f, c, fixed_side) = {
        let (fs, cs) = if fixed_moving {
            (&moving, &state)
        } else {
            (&state, &moving)
        };
        let f = port_frame(fs, fixed_ref)?;
        let fixed_side = side(
            chart(fs, fixed_ref.panel)?,
            &boundary(fs, fixed_ref)?.piece,
            f.to_operation,
        )?;
        (f, port_frame(cs, cut_ref)?, fixed_side)
    };
    let facing = dot(f.n, c.n);
    if facing.abs() < 1.0 - 1e-9 {
        return Err("a double bend joins parallel panels".into());
    }
    if facing < 0.0 {
        return Err(
            "the two panels' material sides are opposite: a double bend joins front surface to \
             front surface"
                .into(),
        );
    }
    if dot(f.m, c.m) > -1.0 + 1e-9 {
        return Err("the boundaries of a double bend must face each other".into());
    }
    let offset = sub(c.origin, f.origin);
    let h = dot(offset, f.n);
    if h.abs() <= PLACED {
        return Err("the panels of a double bend lie in one plane".into());
    }
    // The cut boundary's axial interval in the fixed boundary's terms.
    let a0 = dot(offset, f.e);
    let a1 = a0 + c.width * dot(c.e, f.e);
    if a0.min(a1).abs() > PLACED || (a0.max(a1) - f.width).abs() > PLACED {
        return Err(format!(
            "the boundaries of a double bend must cover the same axial interval ({:.4} to \
             {:.4} mm against 0 to {:.4} mm)",
            a0.min(a1),
            a0.max(a1),
            f.width
        ));
    }
    let middle = radius + thickness / 2.0;
    let rise = 2.0 * middle * (1.0 - theta.cos());
    let straight = (h.abs() - rise) / theta.sin();
    if straight <= PLACED {
        return Err(format!(
            "the panels are {:.4} mm apart: two bends of {:.4} degrees with the inside radius \
             {radius:.4} mm rise {rise:.4} mm without a straight between them",
            h.abs(),
            theta.to_degrees()
        ));
    }
    let run = 2.0 * middle * theta.sin() + straight * theta.cos();
    let depth = run - dot(offset, f.m);
    if depth < -PLACED {
        return Err(format!(
            "the second bend would end {:.4} mm short of the panel it joins",
            -depth
        ));
    }
    let piece = {
        let cut_state = if fixed_moving {
            &mut state
        } else {
            &mut moving
        };
        cut_back(
            ctx,
            cut_state,
            thickness,
            cut_ref,
            &c,
            -depth.max(0.0),
            (0.0, 0.0),
            |_| Ok(Vec::new()),
            depth <= PLACED,
            // (Only the material up to its boundary: past it, in the
            // developed plane of the bends the cut crosses, lie other
            // panels, a relief notch away.)
            Some(1e-4),
            "the double bend's cut",
        )?
    };
    // The jog: the first bend from the fixed boundary towards the other
    // panel's plane, the straight, the second bend back.
    let sign = h.signum();
    let first = BendMetrics::new(sign * theta, d.radius, &rules)?;
    let built = construction(&f, 0.0, 0.0, &first);
    let SheetEmbedding::Plane {
        front_frame: jog_frame,
        front_to_back_sign: jog_parity,
    } = built.moving.clone()
    else {
        unreachable!()
    };
    let width = f.width;
    let jog = SheetPatch {
        domain: SheetDomainRef::Panel(PanelId {
            feature: ctx.uid,
            slot: 0,
        }),
        regions: vec![rectangle(ctx.uid, 50, [0.0, 0.0], [straight, width])?],
        embedding: built.moving,
    };
    let jog_root = side(&jog, &generated(ctx.uid, 53), Isometry2::IDENTITY)?;
    let jog_end = side(&jog, &generated(ctx.uid, 51), Isometry2::IDENTITY)?;
    let at_end = PortFrame {
        origin: jog_frame.point([straight, 0.0]),
        m: jog_frame.x_axis,
        e: jog_frame.y_axis,
        n: scale(jog_frame.normal(), jog_parity.sign()),
        to_operation: Isometry2 {
            origin: [-straight, 0.0],
            x_axis: [1.0, 0.0],
            y_axis: [0.0, 1.0],
        },
        width,
    };
    let second = BendMetrics::new(-sign * theta, d.radius, &rules)?;
    let back = construction(&at_end, 0.0, 0.0, &second);
    // The second bend must end on the cut panel's new boundary.
    let cut_state = if fixed_moving { &state } else { &moving };
    let cut_patch = chart(cut_state, cut_ref.panel)?;
    let (cut_plane, cut_parity) = plane(cut_patch)?;
    let SheetEmbedding::Plane {
        front_frame: landed,
        front_to_back_sign: landed_parity,
    } = &back.moving
    else {
        unreachable!()
    };
    let cut_n = scale(cut_plane.normal(), cut_parity.sign());
    if dot(scale(landed.normal(), landed_parity.sign()), cut_n) < 1.0 - 1e-9
        || dot(sub(landed.origin, cut_plane.origin), cut_n).abs() > PLACED
        || (dot(sub(landed.origin, f.origin), f.m) - run).abs() > PLACED
    {
        return Err("the double bend does not end on the panel it joins".into());
    }
    let cut_axial = Isometry2 {
        origin: [0.0, dot(sub(cut_plane.origin, landed.origin), f.e)],
        x_axis: [0.0, dot(cut_plane.x_axis, f.e)],
        y_axis: [0.0, dot(cut_plane.y_axis, f.e)],
    };
    let cut_side = side(cut_patch, &piece, cut_axial)?;
    // The moving body's material joins the sheet.
    for port in std::mem::take(&mut moving.joins) {
        let token = state
            .joins
            .iter()
            .map(|p| p.token)
            .max()
            .map_or(0, |v| v + 1);
        state.joins.push(ShapePort { token, ..port });
    }
    state.bends.extend(moving.bends);
    state.opened.extend(moving.opened);
    state.lineage.extend(moving.lineage);
    state.charts.extend(moving.charts);
    add_bend_strip(
        ctx,
        &mut state,
        &first,
        built.cylinder,
        vec![fixed_side],
        vec![jog_root],
        width,
        None,
        0,
    )?;
    state.charts.push(jog);
    add_bend_strip(
        ctx,
        &mut state,
        &second,
        back.cylinder,
        vec![jog_end],
        vec![cut_side],
        width,
        None,
        1,
    )?;
    publish(ctx, state, &rules, Some(moving_body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ComponentUid;

    fn empty_state() -> SheetState {
        SheetState {
            schema: 2,
            algorithm: ALGORITHM_VERSION,
            body: BodyUid::new(FeatureUid(2), 0),
            owner: ComponentUid::ROOT,
            shape_generation: String::new(),
            rules: FeatureUid(1),
            rules_version: String::new(),
            panels: Vec::new(),
            charts: Vec::new(),
            bends: Vec::new(),
            joins: Vec::new(),
            opened: Vec::new(),
            lineage: Vec::new(),
            overlays: Vec::new(),
            invalidated: None,
            blockers: Vec::new(),
        }
    }
    fn domain(slot: u32) -> SheetDomainRef {
        SheetDomainRef::Panel(PanelId {
            feature: FeatureUid(2),
            slot,
        })
    }
    fn link(a: SheetDomainRef, b: SheetDomainRef, token: u32) -> ShapePort {
        let side = |domain| ShapePortSide {
            boundary: BoundaryPieceRef {
                domain,
                piece: generated(FeatureUid(2), token),
            },
            span: ParamSpan {
                first: 0.0,
                last: 1.0,
            },
            axial: AxialMap {
                offset: 0.0,
                scale: 20.0,
            },
        };
        ShapePort {
            token,
            axial_interval: ParamSpan {
                first: 0.0,
                last: 20.0,
            },
            first: side(a),
            second: side(b),
        }
    }

    #[test]
    fn moving_branch_preserves_connected_bends_but_rejects_an_alternate_stationary_path() {
        let mut state = empty_state();
        state.joins = vec![link(domain(0), domain(1), 0), link(domain(1), domain(2), 1)];
        assert_eq!(
            branch(&state, domain(1), domain(0), true).unwrap(),
            BTreeSet::from([domain(1), domain(2)])
        );
        state.joins.push(link(domain(2), domain(0), 2));
        assert!(
            branch(&state, domain(1), domain(0), true)
                .unwrap_err()
                .contains("second path")
        );
    }

    #[test]
    fn back_reference_l_keeps_entered_material_side_for_both_angle_signs() {
        let frame = PortFrame {
            origin: [0.0, 20.0, -2.0],
            m: [1.0, 0.0, 0.0],
            e: [0.0, -1.0, 0.0],
            n: [0.0, 0.0, 1.0],
            to_operation: Isometry2::IDENTITY,
            width: 20.0,
        };
        for (sign, centre, endpoint, direction, inner_is_front) in [
            (
                1.0,
                [0.0, 20.0, 3.0],
                [5.0, 20.0, 3.0],
                [0.0, 0.0, 1.0],
                false,
            ),
            (
                -1.0,
                [0.0, 20.0, -5.0],
                [3.0, 20.0, -5.0],
                [0.0, 0.0, -1.0],
                true,
            ),
        ] {
            let metrics = BendMetrics {
                angle: sign * std::f64::consts::FRAC_PI_2,
                radius: 3.0,
                thickness: 2.0,
                k: 0.3,
                allowance: 1.8 * std::f64::consts::PI,
                inner: 3.0,
                outer: 5.0,
                extent: 5.0,
            };
            let result = construction(&frame, 0.0, 0.0, &metrics);
            assert_eq!(result.cylinder.centre, centre);
            assert_eq!(result.cylinder.inner_is_front, inner_is_front);
            assert!((result.cylinder.neutral_radius - 3.6).abs() < 1e-12);
            let SheetEmbedding::Plane {
                front_frame,
                front_to_back_sign,
            } = result.moving
            else {
                panic!("moving plane");
            };
            assert!(distance(front_frame.origin, endpoint) < 1e-12);
            assert!(distance(front_frame.x_axis, direction) < 1e-12);
            assert_eq!(front_to_back_sign, MaterialParity::Negative);
        }
    }

    #[test]
    fn tear_uses_exact_entered_gap_and_round_depth_includes_its_tip() {
        let tear = relief_tool(
            FeatureUid(8),
            100,
            &Relief {
                kind: ReliefKind::Tear,
                width: 2.0,
                depth: 3.0,
            },
            -5.0,
            3.0,
            false,
            40.0,
        )
        .unwrap();
        let p = SheetPatch {
            domain: domain(0),
            regions: vec![tear],
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame::XY,
                front_to_back_sign: MaterialParity::Positive,
            },
        };
        assert_eq!(bounds(&p).unwrap(), ([-8.0, 1.0], [40.0, 3.0]));
        let round = relief_tool(
            FeatureUid(8),
            200,
            &Relief {
                kind: ReliefKind::Round,
                width: 2.0,
                depth: 3.0,
            },
            -5.0,
            3.0,
            false,
            40.0,
        )
        .unwrap();
        assert!(round.loops[0].edges.iter().any(|e|matches!(e.curve.geometry,SegmentGeometry::Arc{center:[x,y],radius,..} if (x+7.0).abs()<1e-12&&(y-2.0).abs()<1e-12&&(radius-1.0).abs()<1e-12)));
        assert!(
            relief_tool(
                FeatureUid(8),
                200,
                &Relief {
                    kind: ReliefKind::Round,
                    width: 4.0,
                    depth: 1.0
                },
                -5.0,
                3.0,
                false,
                40.0
            )
            .is_err()
        );
    }

    #[test]
    fn face_registers_named_coplanar_adjacency_without_cap_topology() {
        let patches: Vec<_> = (0..2)
            .map(|slot| SheetPatch {
                domain: domain(slot),
                regions: vec![
                    rectangle(
                        FeatureUid(10 + u64::from(slot)),
                        0,
                        [10.0 * f64::from(slot), 0.0],
                        [10.0 * f64::from(slot + 1), 20.0],
                    )
                    .unwrap(),
                ],
                embedding: SheetEmbedding::Plane {
                    front_frame: SketchFrame::XY,
                    front_to_back_sign: MaterialParity::Positive,
                },
            })
            .collect();
        let joins = crate::sheet::planar_joins(&patches).unwrap();
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].first.boundary.domain, domain(0));
        assert_eq!(joins[0].second.boundary.domain, domain(1));
        assert_eq!(
            joins[0].axial_interval,
            ParamSpan {
                first: 0.0,
                last: 20.0
            }
        );
        assert_ne!(
            joins[0].first.boundary.piece.curve,
            joins[0].second.boundary.piece.curve
        );
    }

    #[test]
    fn coincident_alias_cannot_escape_its_proven_fragment_with_an_explicit_range() {
        let mut state = empty_state();
        let piece = generated(FeatureUid(2), 10);
        let alias = generated(FeatureUid(3), 20);
        let first = MaterialEdge {
            piece: piece.clone(),
            curve: BoundedCurve2 {
                geometry: SegmentGeometry::Line {
                    start: [0.0, 0.0],
                    end: [1.0, 0.0],
                },
                range: ParamSpan {
                    first: 0.0,
                    last: 1.0,
                },
            },
        };
        let second = MaterialEdge {
            piece: piece.clone(),
            curve: BoundedCurve2 {
                geometry: SegmentGeometry::Line {
                    start: [0.0, 1.0],
                    end: [1.0, 1.0],
                },
                range: ParamSpan {
                    first: 2.0,
                    last: 3.0,
                },
            },
        };
        state.lineage.push(CurveLineage {
            boundary: BoundaryPieceRef {
                domain: domain(0),
                piece,
            },
            curve_generation: CurveLineage::generation(&first.curve),
            aliases: vec![alias.clone()],
            sources: vec![SourceSpan {
                source: TrimSource::Tool {
                    piece: alias.clone(),
                },
                span: first.curve.range,
            }],
        });
        state.charts.push(SheetPatch {
            domain: domain(0),
            regions: vec![MaterialRegion {
                loops: vec![MaterialLoop {
                    edges: vec![first.clone(), second.clone()],
                }],
            }],
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame::XY,
                front_to_back_sign: MaterialParity::Positive,
            },
        });
        let mut reference = BoundaryRef {
            panel: PanelId {
                feature: FeatureUid(2),
                slot: 0,
            },
            curve: alias.curve,
            piece: Some(0),
            range: Some(first.curve.range),
        };
        assert_eq!(boundary(&state, &reference).unwrap(), &first);
        reference.range = Some(second.curve.range);
        assert!(boundary(&state, &reference).is_err());
    }

    #[test]
    fn chart_remapping_and_successive_trims_preserve_only_proven_aliases() {
        let mut state = empty_state();
        let piece = generated(FeatureUid(2), 10);
        let mut alias = generated(FeatureUid(3), 20);
        alias.piece = 7;
        let edge = MaterialEdge {
            piece: piece.clone(),
            curve: BoundedCurve2 {
                geometry: SegmentGeometry::Line {
                    start: [2.0, 3.0],
                    end: [8.0, 3.0],
                },
                range: ParamSpan {
                    first: 0.0,
                    last: 1.0,
                },
            },
        };
        let regions = vec![MaterialRegion {
            loops: vec![MaterialLoop {
                edges: vec![edge.clone()],
            }],
        }];
        let sources = vec![SourceSpan {
            source: TrimSource::Tool {
                piece: alias.clone(),
            },
            span: ParamSpan {
                first: 4.0,
                last: 8.0,
            },
        }];
        state.lineage.push(CurveLineage {
            boundary: BoundaryPieceRef {
                domain: domain(0),
                piece: piece.clone(),
            },
            curve_generation: CurveLineage::generation(&edge.curve),
            aliases: vec![alias.clone()],
            sources: sources.clone(),
        });
        copy_lineage(&mut state.lineage, domain(0), domain(1), &regions);
        let map = Isometry2 {
            origin: [10.0, -5.0],
            x_axis: [-1.0, 0.0],
            y_axis: [0.0, 1.0],
        };
        remap_lineage(&mut state.lineage, domain(1), &regions, map);
        let mapped = mapped_regions(&regions, map);
        let mapped_edge = mapped[0].loops[0].edges[0].clone();
        assert_eq!(mapped_edge.curve.range, edge.curve.range.reversed());
        assert!(state.lineage[0].applies_to(domain(0), &edge));
        assert!(state.lineage[1].applies_to(domain(1), &mapped_edge));
        assert!(!state.lineage[1].applies_to(domain(1), &edge));
        assert_eq!(
            state.lineage[1].sources, sources,
            "chart remapping must not invent an ancestral parameter mapping"
        );
        state.charts.push(SheetPatch {
            domain: domain(1),
            regions: mapped.clone(),
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame::XY,
                front_to_back_sign: MaterialParity::Positive,
            },
        });
        let reference = BoundaryRef {
            panel: PanelId {
                feature: FeatureUid(2),
                slot: 1,
            },
            curve: alias.curve.clone(),
            piece: Some(7),
            range: Some(mapped_edge.curve.range),
        };
        assert_eq!(boundary(&state, &reference).unwrap(), &mapped_edge);
        let inherited = inherited_aliases(
            &state.lineage,
            domain(1),
            &mapped,
            &piece,
            ParamSpan {
                first: 0.2,
                last: 0.8,
            },
        )
        .unwrap();
        assert!(inherited.contains(&alias));
        assert!(inherited.contains(&piece));
        assert!(
            inherited_aliases(
                &state.lineage,
                domain(1),
                &mapped,
                &piece,
                ParamSpan {
                    first: 2.0,
                    last: 3.0
                }
            )
            .is_err()
        );
    }
}
