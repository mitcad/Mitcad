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
        if !angle.is_finite() || angle.abs() <= 1e-9 || angle.abs() > std::f64::consts::PI + 1e-12 {
            return Err("sheet bend angle must be nonzero and at most 180 degrees".into());
        }
        Self::sized(
            angle.clamp(-std::f64::consts::PI, std::f64::consts::PI),
            radius,
            rules,
        )
    }
    /// A flange's bend, which may turn past 180 degrees up to less than a
    /// full turn (a rolled, teardrop or rope hem, mitcad#1378): its
    /// allowance is the same arc of the neutral surface, `|angle| (r +
    /// k t)`, and its outer extent `r + t`.
    pub(super) fn past_half_turn(
        angle: f64,
        radius: Option<f64>,
        rules: &SheetRulesDef<f64>,
    ) -> Result<Self, String> {
        if !angle.is_finite() || angle.abs() <= 1e-9 || angle.abs() >= std::f64::consts::TAU - 1e-9
        {
            return Err("sheet flange bend angle must be nonzero and less than 360 degrees".into());
        }
        Self::sized(angle, radius, rules)
    }
    fn sized(angle: f64, radius: Option<f64>, rules: &SheetRulesDef<f64>) -> Result<Self, String> {
        let radius = radius.unwrap_or(rules.bend_radius);
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
    /// not exist (a bend of 180 degrees or more).
    pub(super) fn setback(&self, value: f64, what: &str) -> Result<f64, String> {
        if self.angle.abs() >= std::f64::consts::PI - 1e-9 || !value.is_finite() {
            return Err(format!(
                "a bend of 180 degrees or more has no {what} virtual sharp; measure it from the \
                 tangent or the outer tangent"
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

/// Seams between the stretches of two boundaries along one of `bend`'s
/// tangent lines (`which`: "stationary" or "moving"), in the order of their
/// axial coordinates: the stretches pair one to one, each pair covering the
/// same part of the line, one seam per pair. One stretch each where nothing
/// interrupts the line; several where holes or cut windows cross it (a fold
/// across them, mitcad#288). A seam covers the part of the line its material
/// covers, not necessarily the bend's whole axial interval: a fold across
/// edges oblique to its line ends its two tangent lines at different axial
/// positions, each seam over its own interval (mitcad#1424). A bend made from
/// a rectangle (a flange, a hem, a bend between two panels) has its sides
/// over the whole interval, so its partners must cover all of it. A refusal
/// names the bend and the boundary curve of its strip where the two sides
/// part (`strip`: the bend's chart).
fn pair_all(
    state: &mut SheetState,
    bend: BendId,
    which: &str,
    strip: &SheetPatch,
    first: Vec<ShapePortSide>,
    second: Vec<ShapePortSide>,
) -> Result<(), String> {
    let label = super::develop::domain_label(SheetDomainRef::Bend(bend));
    if first.len() != second.len() {
        return Err(format!(
            "{label}: the material along its {which} tangent line comes in {} stretches on one \
             side of the seam and {} on the other; the seams cannot pair up",
            first.len(),
            second.len()
        ));
    }
    for (a, b) in first.into_iter().zip(second) {
        let (ca, cb) = (coverage(&a), coverage(&b));
        if let Some((x, y)) = ca
            .into_iter()
            .zip(cb)
            .find(|(x, y)| (x - y).abs() > TOLERANCE)
        {
            // The bend's own side names the edge its strip ends on there.
            let (own, at) = if b.boundary.domain == SheetDomainRef::Bend(bend) {
                (&b, y)
            } else {
                (&a, x)
            };
            let edge = end_edge(strip, own, at).unwrap_or_else(|| "?".into());
            return Err(format!(
                "{label}: along its {which} tangent line one side of the seam ends at {x:.4} mm \
                 and the other at {y:.4} mm, where the bend meets boundary curve {edge}; the bend \
                 cannot follow that edge"
            ));
        }
        if ca[1] - ca[0] <= TOLERANCE {
            return Err(format!(
                "{label}: a stretch of its {which} tangent line at {:.4} mm has no length",
                ca[0]
            ));
        }
        push_port(state, ca, a, b);
    }
    Ok(())
}

/// The boundary curve of a bend's strip that meets the stretch `side` of
/// one of its tangent lines at the axial coordinate `at` (strip coordinates:
/// `v` axial): the edge the bend ends on there.
fn end_edge(strip: &SheetPatch, side: &ShapePortSide, at: f64) -> Option<String> {
    for l in strip.regions.iter().flat_map(|r| &r.loops) {
        let n = l.edges.len();
        for (i, e) in l.edges.iter().enumerate() {
            if e.piece != side.boundary.piece {
                continue;
            }
            let (a, b) = line(e).ok()?;
            // The loop runs from the previous edge into this one at `a`.
            let near = if (a[1] - at).abs() <= (b[1] - at).abs() {
                (i + n - 1) % n
            } else {
                (i + 1) % n
            };
            return Some(l.edges[near].piece.curve.to_string());
        }
    }
    None
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
    bend_strip(
        ctx,
        state,
        metrics,
        cylinder,
        stationary,
        Some(moving_side),
        width,
        strip_regions,
        slot,
    )
    .map(|_| ())
}

/// A bend as [`add_bend_strip`], whose moving tangent line may be left
/// free (`moving_side` none: a flange's bend with no wall after it, or one
/// followed at once by the next bend of a hem, mitcad#1378): the stretches
/// of that line (generated curve `41 + 100 slot`), for the next bend's
/// seam. Its record names that line as its moving boundary.
#[allow(clippy::too_many_arguments)]
fn bend_strip<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    state: &mut SheetState,
    metrics: &BendMetrics,
    cylinder: CylinderEmbedding,
    stationary: Vec<ShapePortSide>,
    moving_side: Option<Vec<ShapePortSide>>,
    width: f64,
    strip_regions: Option<Vec<MaterialRegion>>,
    slot: u32,
) -> Result<Vec<ShapePortSide>, String> {
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
    let moving = match &moving_side {
        Some(m) => m.first(),
        None => right.first(),
    };
    let (Some(first), Some(last)) = (stationary.first(), moving) else {
        return Err("tangent port must be one unique uninterrupted straight boundary".into());
    };
    let moving = last.boundary.clone();
    // The axial interval the strip's material covers: `[0, width]` unless a
    // fold's strip ends on edges oblique to its line or runs on to the
    // outline (mitcad#1424).
    let hull = left
        .iter()
        .chain(&right)
        .map(coverage)
        .fold([f64::INFINITY, f64::NEG_INFINITY], |h, c| {
            [h[0].min(c[0]), h[1].max(c[1])]
        });
    let axial_interval = if hull[0].abs() <= TOLERANCE && (hull[1] - width).abs() <= TOLERANCE {
        ParamSpan {
            first: 0.0,
            last: width,
        }
    } else {
        ParamSpan {
            first: hull[0],
            last: hull[1],
        }
    };
    state.bends.push(BendDomain {
        id,
        stationary: first.boundary.clone(),
        moving,
        folded_embedding: cylinder,
        signed_angle: metrics.angle,
        k_factor: metrics.k,
        radius: metrics.radius,
        allowance: metrics.allowance,
        axial_interval,
        rules: state.rules,
        rules_version: state.rules_version.clone(),
    });
    pair_all(state, id, "stationary", &strip, stationary, left)?;
    let free = match moving_side {
        Some(moving_side) => {
            pair_all(state, id, "moving", &strip, right, moving_side)?;
            Vec::new()
        }
        None => right,
    };
    state.charts.push(strip);
    Ok(free)
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

/// A flange's wall drawn by its profile sketch (mitcad#1648), read in the
/// sketch's own coordinates ([`flange_profile_frame`]): `x` along the bend
/// line from the middle of the boundary (against the boundary's canonical
/// axial direction), `y` along the wall from its root.
struct ProfileWall {
    /// The distances the bend leaves free at the boundary's start and end
    /// (as [`flange_span`] gives them; never negative).
    start: f64,
    end: f64,
    /// How far the wall passes the boundary's start and end beside the bend.
    overhang: (f64, f64),
    /// The wall's length: how far its outline reaches from the bend.
    length: f64,
    /// Half the boundary's length: the sketch's origin along it.
    half: f64,
    /// The outline in the sketch's coordinates, its side on the bend line
    /// the feature's generated curve 53 (the bend's moving port, as the
    /// rectangular wall's root), its other curves the sketch's own.
    region: SheetRegion,
}

impl ProfileWall {
    /// The wall in its chart, whose `x` runs along the wall from its root
    /// and whose `y` starts `start` along the boundary.
    fn chart(&self, start: f64) -> Result<MaterialRegion, String> {
        let region = MaterialRegion::from_legacy(&self.region)?;
        // Sketch (x, y) is chart (y, half - start - x): a quarter turn.
        let map = Isometry2 {
            origin: [0.0, self.half - start],
            x_axis: [0.0, -1.0],
            y_axis: [1.0, 0.0],
        };
        mapped_regions(&[region], map)
            .pop()
            .ok_or_else(|| "The flange's profile has no outline.".to_owned())
    }
}

/// The wall a flange's profile sketch draws (mitcad#1648): the one closed
/// region of the sketch with a straight side on the bend line (the
/// sketch's `x` axis) and its material above it. That side is the bend's
/// span, within the boundary; the rest of the outline may slant, curve or
/// pass the boundary's ends beside the bend. A region on both sides of the
/// bend line, none or more than one on it, or a side on the line past the
/// boundary's ends is refused in words.
fn profile_wall<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    d: &SheetFlangeDef<f64>,
    sketch: FeatureUid,
    edge: f64,
) -> Result<ProfileWall, String> {
    if d.offsets.is_some() || d.width.is_some() || d.hem.is_some() || d.corner.is_some() {
        return Err(
            "A flange with a profile makes one bend and the wall its profile draws; it takes no \
             offsets, width, hem or corner joint."
                .into(),
        );
    }
    let output = ctx.sketch(sketch)?;
    let on_line = |g: &SegmentGeometry| match g {
        SegmentGeometry::Line { start, end } => {
            start[1].abs() <= TOLERANCE
                && end[1].abs() <= TOLERANCE
                && (end[0] - start[0]).abs() > TOLERANCE
        }
        _ => false,
    };
    let mut resting = Vec::new();
    for region in &output.regions {
        let (min, max) = region.bounds();
        if min[1] < -TOLERANCE && max[1] > TOLERANCE {
            return Err(
                "The flange's profile crosses the bend line; the wall's outline must stay on the \
                 wall's side of it."
                    .into(),
            );
        }
        let on = region
            .loops
            .first()
            .is_some_and(|l| l.segments.iter().any(|s| on_line(&s.geometry)));
        if on && max[1] > TOLERANCE {
            resting.push(region);
        }
    }
    let region = match resting.as_slice() {
        [] => {
            return Err(
                "The flange's profile sketch has no closed outline with a side on the bend line."
                    .into(),
            );
        }
        [one] => *one,
        _ => {
            return Err(
                "More than one outline of the flange's profile sketch rests on the bend line; the \
                 wall is one closed outline."
                    .into(),
            );
        }
    };
    let outer = &region.loops[0];
    // The side on the bend line: one stretch, however many curves draw it.
    let mut stretches: Vec<[f64; 2]> = outer
        .segments
        .iter()
        .filter_map(|s| match &s.geometry {
            SegmentGeometry::Line { start, end } if on_line(&s.geometry) => {
                Some([start[0].min(end[0]), start[0].max(end[0])])
            }
            _ => None,
        })
        .collect();
    stretches.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut merged: Vec<[f64; 2]> = Vec::new();
    for stretch in stretches {
        let joins = merged
            .last()
            .is_some_and(|last| stretch[0] <= last[1] + TOLERANCE);
        if joins {
            let last = merged.last_mut().expect("a stretch before");
            last[1] = last[1].max(stretch[1]);
        } else {
            merged.push(stretch);
        }
    }
    let [[x0, x1]] = merged.as_slice() else {
        return Err(
            "The flange's profile meets the bend line in more than one place; its side on the \
             bend line is one straight line."
                .into(),
        );
    };
    let (x0, x1) = (*x0, *x1);
    let half = edge / 2.0;
    // Along the boundary from its start: s = half - x.
    let clean = |v: f64| if v.abs() <= TOLERANCE { 0.0 } else { v };
    let (start, end) = (clean(half - x1), clean(half + x0));
    if start < 0.0 || end < 0.0 {
        return Err(
            "The flange's profile runs along the bend line past the ends of its edge; the bend \
             covers only the edge, and only the wall above it may pass the edge's ends."
                .into(),
        );
    }
    let (min, max) = region.bounds();
    let overhang = (
        clean((max[0] - half).max(0.0)),
        clean((-min[0] - half).max(0.0)),
    );
    let uid = ctx.uid;
    let mut loops = Vec::with_capacity(region.loops.len());
    for (index, profile_loop) in region.loops.iter().enumerate() {
        let mut segments = Vec::with_capacity(profile_loop.segments.len());
        let mut root = false;
        for s in &profile_loop.segments {
            if index == 0 && on_line(&s.geometry) {
                // The curves on the bend line make one root edge, where the
                // first of them is in the loop.
                if !root {
                    segments.push(SheetSegment {
                        id: SheetCurveId::Generated {
                            feature: uid,
                            slot: 53,
                        },
                        geometry: SegmentGeometry::Line {
                            start: [x0, 0.0],
                            end: [x1, 0.0],
                        },
                    });
                    root = true;
                }
                continue;
            }
            segments.push(SheetSegment {
                id: SheetCurveId::Sketch {
                    sketch,
                    segment: s.key.clone(),
                },
                geometry: s.geometry.clone(),
            });
        }
        loops.push(SheetLoop { segments });
    }
    Ok(ProfileWall {
        start,
        end,
        overhang,
        length: max[1],
        half,
        region: SheetRegion { loops },
    })
}

/// The frame of a flange's profile sketch (mitcad#1648), and the length of
/// its boundary: on the wall's front surface where the flange's angle,
/// radius and position put it, the origin on the wall's root line (the
/// bend's moving tangent line) at the middle of the boundary, `x` along
/// that line against the boundary's canonical axial direction and `y`
/// along the wall away from the bend. The normal is the wall's chart
/// normal, so the sketch's coordinates map to the wall's chart by a
/// quarter turn ([`ProfileWall::chart`]).
pub(crate) fn flange_profile_frame(
    state: &SheetState,
    rules: &SheetRulesDef<f64>,
    d: &SheetFlangeDef<f64>,
) -> Result<(SketchFrame, f64), String> {
    let frame = port_frame(state, &d.boundary)?;
    let metrics = BendMetrics::past_half_turn(d.angle, d.radius, rules)?;
    let tangent = metrics.offset(d.position)?;
    let constructed = construction(&frame, tangent, 0.0, &metrics);
    let SheetEmbedding::Plane { front_frame, .. } = constructed.moving else {
        return Err("a flange's wall is not planar".into());
    };
    Ok((
        SketchFrame {
            origin: add(front_frame.origin, scale(frame.e, frame.width / 2.0)),
            x_axis: scale(frame.e, -1.0),
            y_axis: front_frame.x_axis,
        },
        frame.width,
    ))
}

/// Where a new profile sketch of a flange goes and the rectangle it starts
/// with (mitcad#1648): the frame of [`flange_profile_frame`], and the
/// corners of the wall the flange's length and width make now, in the
/// sketch's coordinates (`min` on the bend line). Overhangs past the
/// edge's ends are left to the sketch; a flange without a wall (a length
/// of 0) starts with a wall 10 mm long.
pub(crate) struct FlangeProfileLayout {
    pub(crate) frame: SketchFrame,
    pub(crate) min: [f64; 2],
    pub(crate) max: [f64; 2],
}

pub(crate) fn flange_profile_layout(
    state: &SheetState,
    rules: &SheetRulesDef<f64>,
    d: &SheetFlangeDef<f64>,
) -> Result<FlangeProfileLayout, String> {
    let (frame, edge) = flange_profile_frame(state, rules, d)?;
    let half = edge / 2.0;
    let (lo, hi) = if d.offsets.is_none() && d.width.is_none() {
        (0.0, 0.0)
    } else {
        flange_span(d, edge)?
    };
    let (start, end) = (lo.max(0.0), hi.max(0.0));
    let wall = flange_steps(d, rules)
        .ok()
        .and_then(|steps| steps.first().map(|s| s.wall))
        .filter(|w| *w > TOLERANCE)
        .unwrap_or(10.0);
    Ok(FlangeProfileLayout {
        frame,
        min: [end - half, 0.0],
        max: [half - start, wall],
    })
}

/// Warns when a flange's profile sketch no longer lies on the wall
/// (mitcad#1648): its plane was placed for an earlier angle, radius or
/// position, or moved. The wall still follows the profile's outline; the
/// Flange panel's Edit Profile puts the sketch back on the wall.
pub(crate) fn warn_moved_profile<K: Kernel>(
    d: &SheetFlangeDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<(), String> {
    let Some(sketch) = d.profile else {
        return Ok(());
    };
    // What the flange cannot read it says itself.
    let Ok((state, rules)) = read(ctx, d.body) else {
        return Ok(());
    };
    let Ok((expected, _)) = flange_profile_frame(&state, &rules, d) else {
        return Ok(());
    };
    let actual = ctx.sketch(sketch)?.frame;
    let on_wall = distance(actual.origin, expected.origin) <= 1e-6
        && distance(actual.x_axis, expected.x_axis) <= 1e-7
        && distance(actual.y_axis, expected.y_axis) <= 1e-7;
    if !on_wall {
        ctx.warn(
            "The flange's profile sketch no longer lies on its wall; Edit Profile in the Flange \
             panel puts it back there."
                .to_owned(),
        );
    }
    Ok(())
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

/// How far the panel's outline reaches past the ends of the selected
/// boundary between it and the setback line `tangent` (port frame
/// coordinates: `u` across the boundary, `v` axial over `0..width`), before
/// its start and after its end (mitcad#1576). From each end the outline is
/// followed while it runs straight inward until it crosses the setback line;
/// where a side meets the boundary at more than 90 degrees, its material
/// beside the bend's end (a sliver the cut would leave) reaches past the
/// end. Zero at an end whose outline runs square, narrows, or turns before
/// the line (an arc, a joined boundary, a side parallel to the boundary).
fn outline_overreach(
    state: &SheetState,
    reference: &BoundaryRef,
    frame: &PortFrame,
    tangent: f64,
) -> Result<(f64, f64), String> {
    let selected = boundary(state, reference)?;
    let patch = chart(state, reference.panel)?;
    let Some(edges) = patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .map(|l| &l.edges)
        .find(|l| l.iter().any(|e| std::ptr::eq(e, selected)))
    else {
        return Ok((0.0, 0.0));
    };
    let at = edges
        .iter()
        .position(|e| std::ptr::eq(e, selected))
        .expect("the selected edge is in its loop");
    let map = frame.to_operation;
    let joined = |e: &MaterialEdge| {
        state.joins.iter().any(|p| {
            [&p.first, &p.second]
                .iter()
                .any(|s| s.boundary.domain == patch.domain && s.boundary.piece == e.piece)
        })
    };
    let near = |p: [f64; 2], q: [f64; 2]| g2::dist(p, q) <= 1e3 * TOLERANCE;
    // The axial coordinates the outline reaches from the boundary's end at
    // `from` to its crossing of the setback line, or none.
    let walk = |from: [f64; 2]| -> Option<Vec<f64>> {
        let n = edges.len();
        let ends = |i: usize| {
            line(&edges[i])
                .ok()
                .map(|(a, b)| (map.point(a), map.point(b)))
        };
        let step = [1, n - 1]
            .into_iter()
            .find(|s| ends((at + s) % n).is_some_and(|(a, b)| near(a, from) || near(b, from)))?;
        let mut reached = Vec::new();
        let mut p = from;
        let mut i = (at + step) % n;
        for _ in 1..n {
            if joined(&edges[i]) {
                return None;
            }
            let (a, b) = ends(i)?;
            let q = if near(a, p) {
                b
            } else if near(b, p) {
                a
            } else {
                return None;
            };
            if q[0] >= p[0] - TOLERANCE {
                return None;
            }
            if q[0] <= tangent + TOLERANCE {
                let t = (tangent - p[0]) / (q[0] - p[0]);
                reached.push(p[1] + t * (q[1] - p[1]));
                return Some(reached);
            }
            reached.push(q[1]);
            p = q;
            i = (i + step) % n;
        }
        None
    };
    let before = walk([0.0, 0.0]).map_or(0.0, |v| v.into_iter().fold(0.0, |m: f64, x| m.max(-x)));
    let after = walk([0.0, frame.width]).map_or(0.0, |v| {
        v.into_iter().fold(0.0, |m: f64, x| m.max(x - frame.width))
    });
    Ok((before, after))
}

pub(crate) fn flange<K: Kernel>(
    d: &SheetFlangeDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    free(&state, &d.boundary)?;
    let frame = port_frame(&state, &d.boundary)?;
    // A wall drawn by a profile sketch (mitcad#1648): one bend, its span
    // and overhangs where the profile's outline puts them.
    let profile = d
        .profile
        .map(|sketch| profile_wall(ctx, d, sketch, frame.width))
        .transpose()?;
    let steps = match &profile {
        Some(wall) => {
            let metrics = BendMetrics::past_half_turn(d.angle, d.radius, &rules)?;
            vec![FlangeStep {
                angle: metrics.angle,
                radius: metrics.radius,
                wall: wall.length,
            }]
        }
        None => flange_steps(d, &rules)?,
    };
    let first = steps[0];
    let mut metrics = BendMetrics::past_half_turn(first.angle, Some(first.radius), &rules)?;
    // The bend covers the boundary between the flange's ends; a wall wider
    // than the boundary overhangs past them (mitcad#242).
    let (lo, hi) = match &profile {
        Some(wall) => (wall.start, wall.end),
        None => flange_span(d, frame.width)?,
    };
    let (start, end) = (lo.max(0.0), hi.max(0.0));
    let overhang = match &profile {
        Some(wall) => wall.overhang,
        None => (-lo.min(0.0), -hi.min(0.0)),
    };
    if (overhang.0 > 0.0 || overhang.1 > 0.0) && (steps.len() > 1 || first.wall <= 0.0) {
        return Err(
            "a sheet flange's width passes its boundary only with one bend and a wall".into(),
        );
    }
    let tangent = metrics.offset(d.position)?;
    // An end of the whole boundary (no offset, no overhang) follows the
    // panel's side on to the setback line at the side's own angle
    // (mitcad#1576): the cut takes the material beside the bend's end where
    // the side meets the boundary at more than 90 degrees, and the bend and
    // its wall span the panel's new boundary along the line.
    // (A profile's side on the bend line is the bend's whole span: its
    // ends stay where the profile puts them.)
    let whole = (
        profile.is_none() && start <= TOLERANCE && overhang.0 <= 0.0,
        profile.is_none() && end <= TOLERANCE && overhang.1 <= 0.0,
    );
    let reach = if d.position == SheetPosition::Edge {
        (0.0, 0.0)
    } else {
        outline_overreach(&state, &d.boundary, &frame, tangent)?
    };
    let reach = (
        if whole.0 { reach.0 } else { 0.0 },
        if whole.1 { reach.1 } else { 0.0 },
    );
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
        (start - reach.0, end - reach.1),
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
    // The new boundary's axial interval: a whole end where the panel's side
    // crosses the setback line, an offset end where the cut ends.
    let port = coverage(&side(
        chart(&state, d.boundary.panel)?,
        &tangent_piece,
        frame.to_operation,
    )?);
    let start = if whole.0 { port[0] } else { start };
    let end = if whole.1 { frame.width - port[1] } else { end };
    let width = frame.width - start - end;
    if width <= TOLERANCE {
        return Err("the flange's setback line leaves no material along its boundary".into());
    }
    let mut axial = frame.to_operation;
    axial.origin[1] -= start;
    let stationary = side(chart(&state, d.boundary.panel)?, &tangent_piece, axial)?;
    let mut constructed = construction(&frame, tangent, start, &metrics);
    let mut stationary = vec![stationary];
    // Each bend and the wall after it (a hem's several, mitcad#1378): bend
    // `k` is the feature's bend slot `k` (strip curves `40 + 100 k`), its
    // wall panel slot `k` (curves `50 + 100 k`, the root `53 + 100 k`, the
    // far end `51 + 100 k`). The next bend starts at the wall's far end, or
    // at the bend's moving tangent line where no wall follows it; the last
    // bend's line is left free where it has no wall.
    for (k, step) in steps.iter().enumerate() {
        let slot = k as u32;
        if k > 0 {
            metrics = BendMetrics::past_half_turn(step.angle, Some(step.radius), &rules)?;
            let previous = steps[k - 1].wall;
            let frame = wall_end_frame(&constructed.moving, previous, width)?;
            constructed = construction(&frame, 0.0, 0.0, &metrics);
        }
        if step.wall <= 0.0 {
            stationary = bend_strip(
                ctx,
                &mut state,
                &metrics,
                constructed.cylinder.clone(),
                stationary,
                None,
                width,
                None,
                slot,
            )?;
            continue;
        }
        let region = match (&profile, k) {
            (Some(wall), 0) => wall.chart(start)?,
            (None, 0) => wall_region(ctx.uid, step.wall, width, overhang, root)?,
            _ => rectangle(ctx.uid, 50 + 100 * slot, [0.0, 0.0], [step.wall, width])?,
        };
        let moving = SheetPatch {
            domain: SheetDomainRef::Panel(PanelId {
                feature: ctx.uid,
                slot,
            }),
            regions: vec![region],
            embedding: constructed.moving.clone(),
        };
        let moving_side = side(
            &moving,
            &generated(ctx.uid, 53 + 100 * slot),
            Isometry2::IDENTITY,
        )?;
        // The next bend starts at the wall's far end (a hem's); the last
        // wall's far end is free (a profile's outline has no such curve).
        let far = if k + 1 < steps.len() {
            vec![side(
                &moving,
                &generated(ctx.uid, 51 + 100 * slot),
                Isometry2::IDENTITY,
            )?]
        } else {
            Vec::new()
        };
        add_bend_strip(
            ctx,
            &mut state,
            &metrics,
            constructed.cylinder.clone(),
            stationary,
            vec![moving_side],
            width,
            None,
            slot,
        )?;
        state.charts.push(moving);
        stationary = far;
    }
    if d.corner == Some(SheetFlangeCorner::HalfOpen) {
        half_open_seam(ctx, &mut state, rules.thickness)?;
    }
    publish(ctx, state, &rules, None)
}

/// A half open corner (mitcad#1410): the flange's wall (its panel slot 0)
/// reaches `t / 2` over the end of another wall of the sheet, which is cut
/// back by [`HALF_OPEN_SEAM`] under it, as walls that touch over a face do
/// not make one sound body (mitcad#1409). The other wall is the planar
/// panel normal to the wall's length whose material lies under the wall's
/// last `t / 2` and whose end lies on the wall's surface; it is cut back
/// only where the wall lies over it (the trim's curve 60 to 63).
fn half_open_seam<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    thickness: f64,
) -> Result<(), String> {
    let wall = chart(
        state,
        PanelId {
            feature: ctx.uid,
            slot: 0,
        },
    )?;
    let (frame, parity) = plane(wall)?;
    let (min, max) = bounds(wall)?;
    let normal = frame.normal();
    // The wall's material across its thickness, along `normal` from its
    // front surface, and the part of its length past the open corner.
    let across = [
        0.0_f64.min(parity.sign() * thickness),
        0.0_f64.max(parity.sign() * thickness),
    ];
    let reach = [max[0] - thickness / 2.0, max[0]];
    let mut found = None;
    for patch in &state.charts {
        let SheetDomainRef::Panel(panel) = patch.domain else {
            continue;
        };
        if panel.feature == ctx.uid {
            continue;
        }
        let Ok((other, other_parity)) = plane(patch) else {
            continue;
        };
        if dot(other.normal(), frame.x_axis).abs() < 1.0 - 1e-9 {
            continue;
        }
        let Ok((a, b)) = bounds(patch) else {
            continue;
        };
        let offset = scale(other.normal(), other_parity.sign() * thickness);
        let corners: Vec<[f64; 3]> = [a, [a[0], b[1]], b, [b[0], a[1]]]
            .iter()
            .flat_map(|p| {
                let q = other.point(*p);
                [q, add(q, offset)]
            })
            .collect();
        let range = |axis: [f64; 3]| {
            corners
                .iter()
                .map(|p| dot(sub(*p, frame.origin), axis))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(v), hi.max(v))
                })
        };
        let (x, y, h) = (range(frame.x_axis), range(frame.y_axis), range(normal));
        if x.1.min(reach[1]) - x.0.max(reach[0]) <= TOLERANCE
            || y.1.min(max[1]) - y.0.max(min[1]) <= TOLERANCE
        {
            continue;
        }
        let below = (h.1 - across[0]).abs() <= TOLERANCE;
        if !below && (h.0 - across[1]).abs() > TOLERANCE {
            continue;
        }
        if found.is_some() {
            return Err(
                "a half open corner's wall lies over the ends of more than one wall".into(),
            );
        }
        found = Some((panel, other, below, y));
    }
    let Some((panel, other, below, y)) = found else {
        return Err(
            "a half open corner closes against another wall of the sheet, and no wall ends under \
             this flange's wall"
                .into(),
        );
    };
    // The other wall's chart in the operation coordinates: across the
    // flange's wall (along `normal`) and along its width.
    let origin = sub(other.origin, frame.origin);
    let to_operation = Isometry2 {
        origin: [dot(origin, normal), dot(origin, frame.y_axis)],
        x_axis: [dot(other.x_axis, normal), dot(other.x_axis, frame.y_axis)],
        y_axis: [dot(other.y_axis, normal), dot(other.y_axis, frame.y_axis)],
    };
    let h = if below {
        [across[0] - HALF_OPEN_SEAM, across[0] + thickness]
    } else {
        [across[1] - thickness, across[1] + HALF_OPEN_SEAM]
    };
    // Past the other wall's sides where the flange's wall reaches them.
    let lo = if y.0 >= min[1] - TOLERANCE {
        y.0 - 1.0
    } else {
        min[1]
    };
    let hi = if y.1 <= max[1] + TOLERANCE {
        y.1 + 1.0
    } else {
        max[1]
    };
    let tool = rectangle(ctx.uid, 60, [h[0], lo], [h[1], hi])?;
    trim(
        ctx,
        state,
        &[SheetDomainRef::Panel(panel)],
        SheetTrimOperation::Difference,
        &[tool],
        &[to_operation],
    )
}

/// One bend of a flange and the straight wall after it (0: none).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FlangeStep {
    /// The signed bend angle (radians; up to less than a full turn).
    pub(super) angle: f64,
    /// The inside radius.
    pub(super) radius: f64,
    /// The wall's straight length after the bend.
    pub(super) wall: f64,
}

/// The bends and walls of a flange: one bend and its wall (none where its
/// length leaves no straight part, a rolled hem's, mitcad#1378), or a
/// hem's bends worked out from its sizes ([`SheetHem`]).
pub(super) fn flange_steps(
    d: &SheetFlangeDef<f64>,
    rules: &SheetRulesDef<f64>,
) -> Result<Vec<FlangeStep>, String> {
    let metrics = BendMetrics::past_half_turn(d.angle, d.radius, rules)?;
    // A corner joint: the length is the closed section's inside dimension,
    // and the joint sets how far the wall reaches past it (mitcad#1410).
    let reach = match d.corner {
        None => 0.0,
        Some(corner) => {
            if d.hem.is_some() {
                return Err("a sheet flange makes a hem or a corner joint, not both".into());
            }
            if d.length.mode != SheetLengthMode::Inner {
                return Err(
                    "a corner joint's length is the inside dimension (length mode inner)".into(),
                );
            }
            match corner {
                SheetFlangeCorner::Open => 0.0,
                SheetFlangeCorner::HalfOpen => rules.thickness / 2.0,
            }
        }
    };
    if let Some(hem) = &d.hem {
        if d.length.mode != SheetLengthMode::OuterTangent {
            return Err(
                "a hem's length is measured from the plane touching its bend's outer surface \
                 (length mode outer_tangent)"
                    .into(),
            );
        }
        return hem_steps(
            hem,
            metrics.angle.signum(),
            metrics.radius,
            rules.thickness,
            d.length.value,
        );
    }
    let length = d.length.value
        - match d.length.mode {
            SheetLengthMode::Tangent => 0.0,
            SheetLengthMode::Inner => metrics.setback(metrics.inner, "inner")?,
            SheetLengthMode::Outer => metrics.setback(metrics.outer, "outer")?,
            SheetLengthMode::OuterTangent => metrics.extent,
        }
        + reach;
    if !length.is_finite() || length < -TOLERANCE {
        return Err("sheet flange length is shorter than its requested setback".into());
    }
    Ok(vec![FlangeStep {
        angle: metrics.angle,
        radius: metrics.radius,
        wall: if length <= TOLERANCE { 0.0 } else { length },
    }])
}

/// A hem's bends and walls (mitcad#1378; the forms' geometry:
/// [`SheetHem`]), folding to the side `sign`, of inside radius `r` on a
/// sheet of thickness `t`, `length` its length from the plane touching
/// the first bend's outer surface.
pub(super) fn hem_steps(
    hem: &SheetHem<f64>,
    sign: f64,
    r: f64,
    t: f64,
    length: f64,
) -> Result<Vec<FlangeStep>, String> {
    use std::f64::consts::{FRAC_PI_2, PI};
    let a = r + t;
    let step = |angle: f64, wall: f64| FlangeStep {
        angle: sign * angle,
        radius: r,
        wall: if wall <= TOLERANCE { 0.0 } else { wall },
    };
    let positive = |v: f64, what: &str| {
        if v.is_finite() && v >= 0.0 {
            Ok(v)
        } else {
            Err(format!("a hem's {what} must not be negative"))
        }
    };
    if !length.is_finite() {
        return Err("a hem's length is not a number".into());
    }
    match hem {
        SheetHem::Teardrop { gap } => {
            let g = positive(*gap, "gap")?;
            if r <= g / 2.0 + TOLERANCE {
                return Err("a teardrop hem's inside radius must be more than half its gap".into());
            }
            if length <= 2.0 * a + TOLERANCE {
                return Err(format!(
                    "a teardrop hem's length must be more than its bend's outer diameter {:.4} mm",
                    2.0 * a
                ));
            }
            // The bend turns `180 + alpha` degrees; its wall, `s` long,
            // ends with its inner corner `g` above the panel and its outer
            // corner at the length: `s sin(alpha) = r (1 + cos(alpha)) - g`
            // and `s cos(alpha) = length - a (1 + sin(alpha))`.
            let rise = |alpha: f64| r * (1.0 + alpha.cos()) - g;
            let run = |alpha: f64| length - a * (1.0 + alpha.sin());
            let f = |alpha: f64| rise(alpha) * alpha.cos() - run(alpha) * alpha.sin();
            let (mut lo, mut hi) = (0.0_f64, FRAC_PI_2);
            for _ in 0..200 {
                let mid = (lo + hi) / 2.0;
                if f(mid) > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let alpha = (lo + hi) / 2.0;
            let (rise, run) = (rise(alpha), run(alpha));
            if rise < -TOLERANCE || run < -TOLERANCE {
                return Err("a teardrop hem's wall does not reach back to its gap".into());
            }
            Ok(vec![step(PI + alpha, rise.max(0.0).hypot(run.max(0.0)))])
        }
        SheetHem::Rope { gap } => {
            let g = positive(*gap, "gap")?;
            if r <= g / 2.0 + TOLERANCE {
                return Err("a rope hem's inside radius must be more than half its gap".into());
            }
            // The second bend's centre lies `2 r + t` from the first's, at
            // the height that puts the wall `g` above the panel.
            let alpha = ((g + t) / (2.0 * r + t)).acos();
            let wall = length - a - (2.0 * r + t) * alpha.sin();
            if wall < -TOLERANCE {
                return Err(format!(
                    "a rope hem's length must reach past its second bend, {:.4} mm",
                    a + (2.0 * r + t) * alpha.sin()
                ));
            }
            Ok(vec![step(PI + alpha, 0.0), step(-alpha, wall)])
        }
        SheetHem::Double { gap, setback } => {
            let g = positive(*gap, "gap")?;
            let setback = positive(*setback, "setback")?;
            let over = length - 2.0 * a;
            let under = length - a - t - setback;
            if over < -TOLERANCE || under <= TOLERANCE {
                return Err(format!(
                    "a double hem's length must be at least {:.4} mm and leave a wall under its \
                     second fold",
                    (2.0 * a).max(a + t + setback)
                ));
            }
            Ok(vec![
                step(FRAC_PI_2, g + t),
                step(FRAC_PI_2, over),
                step(PI, under),
            ])
        }
    }
}

/// The port frame at the far end of a flange's wall `wall` long, whose
/// chart is `moving` (a bend's moving embedding: `u` along the wall from
/// the bend, `v` axial over `width`): the frame of the wall's far edge, as
/// [`port_frame`] gives a panel's boundary; at the bend's moving tangent
/// line itself where `wall` is 0.
fn wall_end_frame(moving: &SheetEmbedding, wall: f64, width: f64) -> Result<PortFrame, String> {
    let SheetEmbedding::Plane {
        front_frame,
        front_to_back_sign,
    } = moving
    else {
        return Err("a flange's wall is not planar".into());
    };
    Ok(PortFrame {
        origin: front_frame.point([wall, 0.0]),
        m: front_frame.x_axis,
        e: front_frame.y_axis,
        n: scale(front_frame.normal(), front_to_back_sign.sign()),
        to_operation: Isometry2 {
            origin: [-wall, 0.0],
            x_axis: [1.0, 0.0],
            y_axis: [0.0, 1.0],
        },
        width,
    })
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

/// The domains of the moving branch reached from `start`, or the path by
/// which it reaches the stationary panel `fixed` too: a second path, which
/// no rigid fold or bend can keep (mitcad#1442).
fn branch(
    state: &SheetState,
    start: SheetDomainRef,
    fixed: SheetDomainRef,
    skip_pair: bool,
) -> Result<BTreeSet<SheetDomainRef>, SecondPath> {
    let mut reached = BTreeSet::new();
    let mut parents = std::collections::BTreeMap::new();
    let mut pending = vec![(start, None)];
    while let Some((domain, parent)) = pending.pop() {
        if !reached.insert(domain) {
            continue;
        }
        if let Some(parent) = parent {
            parents.insert(domain, parent);
        }
        if domain == fixed {
            let mut path = vec![domain];
            let mut at = domain;
            while let Some(&before) = parents.get(&at) {
                path.push(before);
                at = before;
            }
            path.reverse();
            return Err(SecondPath(path));
        }
        for port in &state.joins {
            let a = port.first.boundary.domain;
            let b = port.second.boundary.domain;
            if skip_pair && ((a == start && b == fixed) || (b == start && a == fixed)) {
                continue;
            }
            if a == domain {
                pending.push((b, Some(domain)));
            } else if b == domain {
                pending.push((a, Some(domain)));
            }
        }
    }
    Ok(reached)
}

/// A second path from a moving branch to its stationary panel: its
/// domains, from the branch's start to the stationary panel (mitcad#1442).
#[derive(Debug, Clone, PartialEq, Eq)]
struct SecondPath(Vec<SheetDomainRef>);

impl SecondPath {
    /// The earlier features whose material on the path joins the two
    /// sides: those of its bends, or, with no bend on it, those of its
    /// panels besides the stationary one; never `own`, the feature being
    /// made. In the path's order, each once.
    fn features(&self, own: FeatureUid) -> Vec<FeatureUid> {
        let Some((_, joining)) = self.0.split_last() else {
            return Vec::new();
        };
        let mut bends = Vec::new();
        let mut panels = Vec::new();
        for domain in joining {
            let list = match domain {
                SheetDomainRef::Bend(_) => &mut bends,
                SheetDomainRef::Panel(_) => &mut panels,
            };
            let feature = domain.feature();
            if feature != own && !list.contains(&feature) {
                list.push(feature);
            }
        }
        if bends.is_empty() { panels } else { bends }
    }
}

/// The operation a second path refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bending {
    Fold,
    Bend,
}

/// "A", "A and B", "A, B and C".
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The refusal of a fold or bend whose moving side is also joined to its
/// stationary side another way (mitcad#1442), in the user's terms: the
/// features `joining` (by their names as the browser shows them, and
/// whether each is a fold) whose bends join both sides, and what to do
/// instead.
fn second_path_message(bending: Bending, joining: &[(String, bool)]) -> String {
    let names: Vec<String> = joining.iter().map(|(name, _)| name.clone()).collect();
    let all_folds = joining.iter().all(|(_, fold)| *fold);
    // The features by their names; one feature and several are whole
    // sentences of their own, so that a translation needs no verb forms put
    // in (mitcad#1284).
    let all = match names.as_slice() {
        [] => {
            return match bending {
                Bending::Fold => "Both sides of the fold line are also joined elsewhere, \
                     so the fold cannot turn one side and keep the other."
                    .to_owned(),
                Bending::Bend => "The moving panel is also joined to the stationary one \
                     elsewhere, so the bend cannot turn it alone."
                    .to_owned(),
            };
        }
        [only] => only.clone(),
        _ => listed(&names),
    };
    let one = names.len() == 1;
    match bending {
        Bending::Fold => {
            let instead = if !all_folds {
                if one {
                    format!("change {all} so that it does not join both sides")
                } else {
                    "change them so that they do not join both sides".to_owned()
                }
            } else if one {
                format!("make {all} with a separate line on each side")
            } else {
                "make each of them with a separate line on each side".to_owned()
            };
            if one {
                format!(
                    "This fold would also have to bend {all}, whose bend joins both sides of the \
                     fold line. Fold here before {all}, or {instead}."
                )
            } else {
                format!(
                    "This fold would also have to bend {all}, whose bends join both sides of the \
                     fold line. Fold here before them, or {instead}."
                )
            }
        }
        Bending::Bend => {
            if one {
                format!(
                    "This bend would also have to bend {all}, whose bend joins the moving panel to \
                     the stationary one too. Bend here before {all}, or choose panels that \
                     {all} does not join."
                )
            } else {
                format!(
                    "This bend would also have to bend {all}, whose bends join the moving panel to \
                     the stationary one too. Bend here before them, or choose panels that \
                     they do not join."
                )
            }
        }
    }
}

/// [`second_path_message`] for a second path found while `ctx` makes its
/// feature.
fn second_path_refusal<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    bending: Bending,
    path: &SecondPath,
) -> String {
    let joining: Vec<(String, bool)> = path
        .features(ctx.uid)
        .into_iter()
        .map(|uid| {
            let fold = ctx
                .env
                .feature(uid)
                .is_some_and(|f| matches!(f.def, crate::features::FeatureDef::SheetFold(_)));
            (ctx.feature_name(uid), fold)
        })
        .collect();
    second_path_message(bending, &joining)
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

/// How far past an end of a fold line its material is probed: well above
/// the modelling tolerance, well below any meaningful material size.
const END_PROBE: f64 = 1.0e-5;

/// How one end of a fold line meets the panel's material (mitcad#1424).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FoldEnd {
    /// The line ends inside material: a relief separates the bend there.
    Inside,
    /// The line reaches the outline at this end (on it or past it) and no
    /// material within the bend's reach crosses the normal through its end:
    /// the bend ends there, at the end of the line.
    Flush,
    /// The line reaches the outline, but material within the bend's reach
    /// crosses the normal through its end (the line ends on an edge oblique
    /// to it, or the moving side widens past it): the bend and the moving
    /// panel run on past the end, bounded by the outline (the oblique edges
    /// continued across the strip).
    Outline,
}

/// Classifies the end of a fold line at the axial coordinate `at` of the
/// operation frame (`u` across the line, `0` on it, the moving side
/// positive; `v` along it), `outward` the sign of `v` past that end, from
/// the material of `operation` (the panel's regions in that frame) on the
/// normal just past the end. `tangent` is where the bend starts across the
/// line: material past it is within the bend's reach.
fn fold_end(
    operation: &[MaterialRegion],
    at: f64,
    outward: f64,
    tangent: f64,
) -> Result<FoldEnd, String> {
    // Swap the axes, so that the chord of `x = v` gives the material
    // intervals of `u` on the normal at `v`.
    let swap = Isometry2 {
        origin: [0.0, 0.0],
        x_axis: [0.0, 1.0],
        y_axis: [1.0, 0.0],
    };
    let across = super::develop::chord(&mapped_regions(operation, swap), at + outward * END_PROBE)?;
    Ok(
        if across.iter().any(|c| c[0] < -TOLERANCE && c[1] > TOLERANCE) {
            FoldEnd::Inside
        } else if across.iter().any(|c| c[1] > tangent + TOLERANCE) {
            FoldEnd::Outline
        } else {
            FoldEnd::Flush
        },
    )
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
    // How each end of the line meets the material (mitcad#1424): a relief
    // where it ends inside material; where it reaches the outline but an
    // edge oblique to it carries material past its end within the bend's
    // reach, the bend and the moving panel run on and the outline bounds
    // them.
    let operation = mapped_regions(&old.regions, frame.to_operation);
    let ends = match (
        fold_end(&operation, 0.0, -1.0, tangent),
        fold_end(&operation, frame.width, 1.0, tangent),
    ) {
        (Ok(start), Ok(end)) => [start, end],
        // A boundary the probe cannot cross (a closed spline has no
        // bounded support): the panel's extent decides, as it did before.
        _ => [
            if min[1] < -TOLERANCE {
                FoldEnd::Inside
            } else {
                FoldEnd::Flush
            },
            if max[1] > frame.width + TOLERANCE {
                FoldEnd::Inside
            } else {
                FoldEnd::Flush
            },
        ],
    };
    let low = if ends[0] == FoldEnd::Outline {
        min[1].min(0.0) - pad
    } else {
        0.0
    };
    let high = if ends[1] == FoldEnd::Outline {
        max[1].max(frame.width) + pad
    } else {
        frame.width
    };
    if ends.contains(&FoldEnd::Inside) {
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
    let moving_tool = rectangle(ctx.uid, 50, [far_tangent, low], [outside, high])?;
    trim(
        ctx,
        state,
        &[moving_domain],
        SheetTrimOperation::Intersection,
        &[moving_tool],
        &[frame.to_operation],
    )?;
    let strip_tool = rectangle(ctx.uid, 40, [tangent, low], [far_tangent, high])?;
    trim(
        ctx,
        state,
        &[strip_domain],
        SheetTrimOperation::Intersection,
        &[strip_tool],
        &[frame.to_operation],
    )?;
    let mut removal = vec![rectangle(ctx.uid, 10, [tangent, low], [outside, high])?];
    if ends[0] == FoldEnd::Inside {
        removal.push(relief_tool(
            ctx.uid, 20, relief, tangent, 0.0, false, outside,
        )?);
    }
    if ends[1] == FoldEnd::Inside {
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
                && a[1] >= low - TOLERANCE
                && b[1] >= low - TOLERANCE
                && a[1] <= high + TOLERANCE
                && b[1] <= high + TOLERANCE
            {
                own.boundary.domain = moving_domain;
                moving_neighbors.push(other.boundary.domain);
            } else if a[0] > tangent + TOLERANCE || b[0] > tangent + TOLERANCE {
                return Err("fold strip crosses an existing tangent port".into());
            }
        }
    }
    // The moving panel's trim may give the boundaries it keeps other
    // parameters than the panel's (a line returned running the other way,
    // as after a join, mitcad#1450): the ports it took are bound to them
    // again, by position.
    let mut before = state.clone();
    if let Some(p) = before.charts.iter_mut().find(|p| p.domain == moving_domain) {
        p.regions = old.regions.clone();
    }
    let rebound: Vec<(usize, bool, ShapePortSide)> = state
        .joins
        .iter()
        .enumerate()
        .flat_map(|(i, port)| [(i, true, &port.first), (i, false, &port.second)])
        .filter(|(_, _, side)| side.boundary.domain == moving_domain)
        .filter_map(|(i, first, side)| Some((i, first, cut::rebind(&before, state, side)?)))
        .collect();
    for (i, first, side) in rebound {
        let port = &mut state.joins[i];
        *(if first {
            &mut port.first
        } else {
            &mut port.second
        }) = side;
    }
    let mut moved = BTreeSet::new();
    for neighbor in moving_neighbors {
        moved.extend(
            branch(state, neighbor, stationary_domain, false)
                .map_err(|path| second_path_refusal(ctx, Bending::Fold, &path))?,
        );
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
    let moved = branch(state, moving_domain, fixed, true)
        .map_err(|path| second_path_refusal(ctx, Bending::Bend, &path))?;
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
        // A second path through an earlier fold's bend: refused, naming
        // the fold whose bend joins both sides (mitcad#1442).
        let earlier = SheetDomainRef::Bend(BendId {
            feature: FeatureUid(5),
            slot: 0,
        });
        state.joins.push(link(domain(2), earlier, 2));
        state.joins.push(link(earlier, domain(0), 3));
        let path = branch(&state, domain(1), domain(0), true).unwrap_err();
        assert_eq!(
            path,
            SecondPath(vec![domain(1), domain(2), earlier, domain(0)])
        );
        assert_eq!(path.features(FeatureUid(7)), vec![FeatureUid(5)]);
        assert_eq!(
            second_path_message(Bending::Fold, &[("SheetFold1".to_owned(), true)]),
            "This fold would also have to bend SheetFold1, whose bend joins both sides of the \
             fold line. Fold here before SheetFold1, or make SheetFold1 with a separate line \
             on each side."
        );
        assert_eq!(
            second_path_message(Bending::Bend, &[("SheetFold1".to_owned(), true)]),
            "This bend would also have to bend SheetFold1, whose bend joins the moving panel \
             to the stationary one too. Bend here before SheetFold1, or choose panels that \
             SheetFold1 does not join."
        );
        assert_eq!(
            second_path_message(
                Bending::Fold,
                &[
                    ("SheetFold1".to_owned(), true),
                    ("SheetFlange2".to_owned(), false),
                    ("SheetFold3".to_owned(), true)
                ]
            ),
            "This fold would also have to bend SheetFold1, SheetFlange2 and SheetFold3, whose \
             bends join both sides of the fold line. Fold here before them, or change them so \
             that they do not join both sides."
        );
        // A path with no earlier bend on it names the earlier panels; with
        // none either, it still says what is wrong without the graph.
        let mut state = empty_state();
        state.joins = vec![
            link(domain(0), domain(1), 0),
            link(domain(1), domain(2), 1),
            link(domain(2), domain(0), 2),
        ];
        let path = branch(&state, domain(1), domain(0), true).unwrap_err();
        assert_eq!(path.features(FeatureUid(7)), vec![FeatureUid(2)]);
        assert!(path.features(FeatureUid(2)).is_empty());
        let message = second_path_message(Bending::Fold, &[]);
        assert!(message.contains("Both sides of the fold line are also joined elsewhere"));
        assert!(!message.contains("graph"));
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

    /// Rules of thickness 2.5, bend radius 2.5 and K-factor 0.44 (the
    /// reference hems' rule, mitcad#1378).
    fn hem_rules() -> SheetRulesDef<f64> {
        serde_json::from_value(serde_json::json!({
            "thickness": 2.5, "bend_radius": 2.5,
            "allowance": {"type": "k_factor", "k": 0.44}, "reference_side": "front",
            "bend_relief": {"type": "rectangular", "width": 2.5, "depth": 1.25},
            "corner_relief": {"type": "rectangular", "width": 2.5, "depth": 1.25}
        }))
        .expect("rules")
    }

    /// A flange of the given angle, length, radius and hem on a sketch
    /// boundary.
    fn hem_flange(
        angle: f64,
        mode: &str,
        length: f64,
        radius: f64,
        hem: serde_json::Value,
    ) -> SheetFlangeDef<f64> {
        let mut v = serde_json::json!({
            "body": "F3.b0",
            "boundary": {"panel": {"feature": "F3", "slot": 0},
                         "curve": {"type": "sketch", "sketch": "F2", "segment": "c2[c1,c3]"}},
            "angle": angle, "length": {"mode": mode, "value": length},
            "position": "side_tangent", "radius": radius,
        });
        if !hem.is_null() {
            v["hem"] = hem;
        }
        serde_json::from_value(v).expect("flange")
    }

    /// A flange's bend may turn past 180 degrees, short of a full turn
    /// (mitcad#1378): its flat length is the bend allowance `|angle| (r +
    /// k t)`, its outer extent `r + t`; its virtual sharps do not exist.
    /// Bends and folds keep the 180-degree limit.
    #[test]
    fn a_flange_bend_turns_past_half_a_turn_with_its_allowance() {
        use std::f64::consts::{PI, TAU};
        let rules = hem_rules();
        let m = BendMetrics::past_half_turn(1.5 * PI, Some(3.0), &rules).expect("270 degrees");
        assert!((m.allowance - 1.5 * PI * (3.0 + 0.44 * 2.5)).abs() < 1e-12);
        assert!((m.extent - 5.5).abs() < 1e-12);
        assert!(m.setback(m.outer, "outer").is_err());
        assert!(m.offset(SheetPosition::Inside).is_err());
        assert!((m.offset(SheetPosition::SideTangent).unwrap() + 5.5).abs() < 1e-12);
        let m = BendMetrics::past_half_turn(-1.5 * PI, Some(3.0), &rules).expect("either side");
        assert!((m.angle + 1.5 * PI).abs() < 1e-12);
        for refused in [0.0, TAU, -TAU, 7.0, f64::NAN] {
            assert!(BendMetrics::past_half_turn(refused, Some(3.0), &rules).is_err());
        }
        assert!(BendMetrics::new(1.5 * PI, Some(3.0), &rules).is_err());
        assert!(BendMetrics::new(PI, Some(3.0), &rules).is_ok());
        // A rolled hem: 270 degrees and no wall (its length 0 from the
        // tangent line); a length short of the outer extent is refused.
        let steps = flange_steps(
            &hem_flange(1.5 * PI, "tangent", 0.0, 3.0, serde_json::Value::Null),
            &rules,
        )
        .expect("a rolled hem");
        assert_eq!(
            steps,
            vec![FlangeStep {
                angle: 1.5 * PI,
                radius: 3.0,
                wall: 0.0
            }]
        );
        let flange = hem_flange(1.5 * PI, "outer_tangent", 5.0, 3.0, serde_json::Value::Null);
        assert!(flange_steps(&flange, &rules).is_err());
        let flange = hem_flange(
            1.5 * PI,
            "outer_tangent",
            15.5,
            3.0,
            serde_json::Value::Null,
        );
        let steps = flange_steps(&flange, &rules).expect("a wall of 10");
        assert!((steps[0].wall - 10.0).abs() < 1e-12);
    }

    /// The hem forms' bends from their sizes (mitcad#1378), on the
    /// reference hems' sheet (t 2.5): each form's defining distances hold,
    /// and the bodies' volumes these give are the stored ones (teardrop
    /// 20402.855, rope 20561.79, double 22784.292 mm3 on the 17500 mm3
    /// panel, 100 wide).
    #[test]
    fn hem_forms_give_their_bends_and_walls() {
        use std::f64::consts::{FRAC_PI_2, PI};
        let t = 2.5;
        let ring = |r: f64, angle: f64| angle.abs() / 2.0 * ((r + t) * (r + t) - r * r);
        let volume = |r: f64, steps: &[FlangeStep]| {
            17500.0 - (steps[0].radius + t) * t * 100.0
                + steps
                    .iter()
                    .map(|s| (ring(r, s.angle) + s.wall * t) * 100.0)
                    .sum::<f64>()
        };
        // Teardrop, radius 2, gap 1, length 10: the wall's inner end
        // corner 1 above the panel, its outer end corner 10 from the side.
        let (r, a) = (2.0, 4.5);
        let steps = hem_steps(&SheetHem::Teardrop { gap: 1.0 }, 1.0, r, t, 10.0).expect("teardrop");
        assert_eq!(steps.len(), 1);
        let alpha = steps[0].angle - PI;
        let s = steps[0].wall;
        assert!((alpha.to_degrees() - 46.906_427).abs() < 1e-5, "{alpha}");
        assert!((s - 3.240_561).abs() < 1e-5, "{s}");
        assert!((a + r * alpha.cos() - s * alpha.sin() - (t + 1.0)).abs() < 1e-9);
        assert!((a + a * alpha.sin() + s * alpha.cos() - 10.0).abs() < 1e-9);
        assert!((volume(r, &steps[..]) - 20402.855).abs() < 1e-3);
        // Rope, radius 2, gap 1, length 10: 237.42 degrees, at once -57.42,
        // then a wall of 0.0228 one gap above the panel.
        let steps = hem_steps(&SheetHem::Rope { gap: 1.0 }, 1.0, r, t, 10.0).expect("rope");
        let alpha = (3.5f64 / 6.5).acos();
        assert_eq!(steps.len(), 2);
        assert!((steps[0].angle - (PI + alpha)).abs() < 1e-12);
        assert_eq!(steps[0].wall, 0.0);
        assert!((steps[1].angle + alpha).abs() < 1e-12);
        assert!((steps[1].wall - (10.0 - a - 6.5 * alpha.sin())).abs() < 1e-12);
        assert!((steps[1].wall - 0.022_774).abs() < 1e-6);
        assert!((volume(r, &steps[..]) - 20561.79).abs() < 1e-2);
        // Double, radius 1 (half the gap), gap 2, setback 1, length 10:
        // 90, 4.5, 90, 3, 180, 3; to the other side every angle turns.
        let steps = hem_steps(
            &SheetHem::Double {
                gap: 2.0,
                setback: 1.0,
            },
            -1.0,
            1.0,
            t,
            10.0,
        )
        .expect("double");
        let angles: Vec<f64> = steps.iter().map(|s| s.angle).collect();
        let walls: Vec<f64> = steps.iter().map(|s| s.wall).collect();
        assert_eq!(angles, [-FRAC_PI_2, -FRAC_PI_2, -PI]);
        assert_eq!(walls, [4.5, 3.0, 3.0]);
        assert!((volume(1.0, &steps[..]) - 22784.292).abs() < 1e-3);
        // Refused: a radius not more than half the gap, a length too short.
        assert!(hem_steps(&SheetHem::Teardrop { gap: 4.0 }, 1.0, r, t, 10.0).is_err());
        assert!(hem_steps(&SheetHem::Rope { gap: 4.0 }, 1.0, r, t, 10.0).is_err());
        assert!(hem_steps(&SheetHem::Teardrop { gap: 1.0 }, 1.0, r, t, 9.0).is_err());
        assert!(hem_steps(&SheetHem::Rope { gap: 1.0 }, 1.0, r, t, 9.0).is_err());
        let short = SheetHem::Double {
            gap: 2.0,
            setback: 1.0,
        };
        assert!(hem_steps(&short, 1.0, 1.0, t, 6.0).is_err());
        assert!(hem_steps(&SheetHem::Teardrop { gap: -1.0 }, 1.0, r, t, 10.0).is_err());
        // Through the flange: the hem's length is from the outer tangent
        // plane only, its radius the flange's.
        let rules = hem_rules();
        let hem = serde_json::json!({"form": "teardrop", "gap": 1.0});
        let flange = hem_flange(PI, "outer_tangent", 10.0, 2.0, hem.clone());
        let steps = flange_steps(&flange, &rules).expect("a teardrop flange");
        assert!((steps[0].wall - 3.240_561).abs() < 1e-5);
        let flange = hem_flange(-PI, "tangent", 10.0, 2.0, hem);
        assert!(
            flange_steps(&flange, &rules)
                .unwrap_err()
                .contains("outer_tangent")
        );
    }

    /// A corner joint's wall (mitcad#1410): the length is the inside
    /// dimension (Inner Length); an open corner ends there, a half open one
    /// half the thickness further. Other length modes and a hem with it are
    /// refused.
    #[test]
    fn a_corner_joint_sets_the_wall_end_from_the_thickness() {
        use std::f64::consts::FRAC_PI_2;
        let rules = hem_rules();
        let corner = |mode: &str, corner: &str, hem: serde_json::Value| {
            let mut flange = hem_flange(FRAC_PI_2, mode, 20.0, 2.5, hem);
            flange.corner = serde_json::from_value(serde_json::json!(corner)).expect("corner");
            flange_steps(&flange, &rules)
        };
        // The inner setback of a 90-degree bend is its radius, 2.5.
        let open = corner("inner", "open", serde_json::Value::Null).expect("an open corner");
        assert!((open[0].wall - 17.5).abs() < 1e-12);
        let half =
            corner("inner", "half_open", serde_json::Value::Null).expect("a half open corner");
        assert!((half[0].wall - 18.75).abs() < 1e-12);
        assert!(
            corner("outer", "open", serde_json::Value::Null)
                .unwrap_err()
                .contains("length mode inner")
        );
        assert!(
            corner(
                "inner",
                "open",
                serde_json::json!({"form": "teardrop", "gap": 1.0})
            )
            .unwrap_err()
            .contains("not both")
        );
    }

    /// A stretch of a seam over `interval` of the bend's axis.
    fn stretch(domain: SheetDomainRef, slot: u32, interval: [f64; 2]) -> ShapePortSide {
        ShapePortSide {
            boundary: BoundaryPieceRef {
                domain,
                piece: generated(FeatureUid(7), slot),
            },
            span: ParamSpan {
                first: 0.0,
                last: 1.0,
            },
            axial: AxialMap {
                offset: interval[0],
                scale: interval[1] - interval[0],
            },
        }
    }

    #[test]
    fn seams_of_a_fold_across_oblique_edges_each_cover_their_own_interval() {
        // mitcad#1424: a strip whose ends follow two edges oblique to its
        // axis; its stationary side (u = 0) runs 13.1..96.1, its moving
        // side (u = 2) 13.3..96.8.
        let bend = BendId {
            feature: FeatureUid(7),
            slot: 0,
        };
        let own = SheetDomainRef::Bend(bend);
        let strip = SheetPatch {
            domain: own,
            regions: vec![
                polygon(
                    FeatureUid(7),
                    40,
                    &[[0.0, 13.1], [2.0, 13.3], [2.0, 96.8], [0.0, 96.1]],
                )
                .unwrap(),
            ],
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame::XY,
                front_to_back_sign: MaterialParity::Positive,
            },
        };
        let mut state = empty_state();
        let seam = |state: &mut SheetState, which, first, second| {
            pair_all(state, bend, which, &strip, vec![first], vec![second])
        };
        seam(
            &mut state,
            "stationary",
            stretch(domain(0), 13, [13.1, 96.1]),
            stretch(own, 43, [13.1, 96.1]),
        )
        .unwrap();
        seam(
            &mut state,
            "moving",
            stretch(own, 41, [13.3, 96.8]),
            stretch(domain(1), 53, [13.3, 96.8]),
        )
        .unwrap();
        let intervals: Vec<_> = state
            .joins
            .iter()
            .map(|p| [p.axial_interval.first, p.axial_interval.last])
            .collect();
        assert_eq!(intervals.len(), 2);
        for (found, wanted) in intervals.iter().zip([[13.1, 96.1], [13.3, 96.8]]) {
            assert!((found[0] - wanted[0]).abs() < 1e-9 && (found[1] - wanted[1]).abs() < 1e-9);
        }
        // The panel's material running past the edge the strip ends on is
        // refused, naming the bend and that edge (the strip's g40).
        let error = seam(
            &mut state,
            "stationary",
            stretch(domain(0), 13, [12.9, 96.1]),
            stretch(own, 43, [13.1, 96.1]),
        )
        .unwrap_err();
        assert!(error.contains("bend F7 slot 0"), "{error}");
        assert!(error.contains("stationary tangent line"), "{error}");
        assert!(error.contains("F7/g40"), "{error}");
        // A bend from a rectangle still needs partners over its whole
        // interval.
        let error = seam(
            &mut state,
            "moving",
            stretch(own, 41, [0.0, 20.0]),
            stretch(domain(1), 53, [0.0, 15.0]),
        )
        .unwrap_err();
        assert!(error.contains("bend F7 slot 0"), "{error}");
        let error = pair_all(
            &mut state,
            bend,
            "moving",
            &strip,
            vec![stretch(own, 41, [0.0, 8.0]), stretch(own, 41, [12.0, 20.0])],
            vec![stretch(domain(1), 53, [0.0, 20.0])],
        )
        .unwrap_err();
        assert!(error.contains("2 stretches"), "{error}");
        assert_eq!(state.joins.len(), 2);
    }

    #[test]
    fn fold_line_ends_are_classified_against_the_outline() {
        // mitcad#1424, in the fold's operation frame (u across the line,
        // the moving side positive; v along it).
        let region = |points: &[[f64; 2]]| vec![polygon(FeatureUid(3), 0, points).unwrap()];
        let ba = 1.8 * std::f64::consts::PI;
        let centred = -ba / 2.0;
        // The trapezoid (0,0), (60,0), (50,20), (10,20) and the line y = 10
        // from edge to edge, its wide side moving (u = 10 - y, v = x - 5):
        // material past both ends within the bend's reach.
        let wide = region(&[[10.0, -5.0], [10.0, 55.0], [-10.0, 45.0], [-10.0, 5.0]]);
        for tangent in [centred, 0.0] {
            assert_eq!(
                fold_end(&wide, 0.0, -1.0, tangent).unwrap(),
                FoldEnd::Outline
            );
            assert_eq!(
                fold_end(&wide, 50.0, 1.0, tangent).unwrap(),
                FoldEnd::Outline
            );
        }
        // Its narrow side moving (u = y - 10): a centred strip reaches the
        // wide stationary side past the ends, one starting at the line not.
        let narrow = region(&[[-10.0, -5.0], [-10.0, 55.0], [10.0, 45.0], [10.0, 5.0]]);
        assert_eq!(
            fold_end(&narrow, 0.0, -1.0, centred).unwrap(),
            FoldEnd::Outline
        );
        assert_eq!(
            fold_end(&narrow, 50.0, 1.0, centred).unwrap(),
            FoldEnd::Outline
        );
        assert_eq!(fold_end(&narrow, 0.0, -1.0, 0.0).unwrap(), FoldEnd::Flush);
        assert_eq!(fold_end(&narrow, 50.0, 1.0, 0.0).unwrap(), FoldEnd::Flush);
        // The line drawn past both edges (x from -5 to 65): nothing past
        // its ends.
        let past = region(&[[10.0, 5.0], [10.0, 65.0], [-10.0, 55.0], [-10.0, 15.0]]);
        assert_eq!(fold_end(&past, 0.0, -1.0, centred).unwrap(), FoldEnd::Flush);
        assert_eq!(fold_end(&past, 70.0, 1.0, centred).unwrap(), FoldEnd::Flush);
        // A line ending inside material takes a relief at both ends.
        let around = region(&[[-10.0, -10.0], [10.0, -10.0], [10.0, 60.0], [-10.0, 60.0]]);
        assert_eq!(
            fold_end(&around, 0.0, -1.0, centred).unwrap(),
            FoldEnd::Inside
        );
        assert_eq!(
            fold_end(&around, 50.0, 1.0, centred).unwrap(),
            FoldEnd::Inside
        );
        // Edges square to the line at its ends: the usual full-width fold.
        let square = region(&[[-10.0, 0.0], [10.0, 0.0], [10.0, 50.0], [-10.0, 50.0]]);
        assert_eq!(
            fold_end(&square, 0.0, -1.0, centred).unwrap(),
            FoldEnd::Flush
        );
        assert_eq!(
            fold_end(&square, 50.0, 1.0, centred).unwrap(),
            FoldEnd::Flush
        );
    }
}
