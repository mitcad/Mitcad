// SPDX-License-Identifier: MIT
//! Development of the labelled panel/bend graph (mitcad#208).
//!
//! The traversal starts at a stationary panel and crosses the registered
//! tangent ports. A panel, or a bend whose configuration stays, moves
//! rigidly with the domain it is reached from. A bend whose configuration
//! changes is rebuilt from the material station of the side it is entered
//! from: its cylinder becomes the neutral strip `[0, BA] x axial` in the
//! tangent plane of that station (developed lengths are neutral-surface
//! lengths), or the strip becomes the bend's cylinder again. Charts keep
//! their (u, v) material coordinates, so every curve, port and identity
//! survives unchanged, and cuts made while a bend is open are folded back
//! with it. A second path to an already placed domain must agree; a cycle
//! that does not close is a non-developable configuration, never resolved
//! by dropping a seam.

use mitcad_vec3::{add, cross, distance, scale, sub};

use super::*;
use crate::features::sheet_unfold::{SheetRefoldDef, SheetUnfoldDef};
use crate::transform::Transform;
use mitcad_vec3::dot;
use std::collections::{BTreeMap, VecDeque};

use mitcad_vec3::Vec3 as V3;

/// Positions of one seam reached along two paths must agree this closely.
/// Well above chained rounding, well below any meaningful material offset.
const CLOSURE: f64 = 1.0e-6;
/// Direction agreement of the same seam frame along two paths.
const ANGULAR: f64 = 1.0e-9;

pub(crate) fn domain_label(domain: SheetDomainRef) -> String {
    match domain {
        SheetDomainRef::Panel(p) => format!("panel {} slot {}", p.feature, p.slot),
        SheetDomainRef::Bend(b) => format!("bend {} slot {}", b.feature, b.slot),
    }
}

/// A material station of a chart: the front-surface point at `(u, 0)`, the
/// unit direction of increasing `u`, the axial direction of `v`, and the
/// front-to-back material direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Station {
    pub origin: V3,
    pub x: V3,
    pub y: V3,
    pub f: V3,
}

impl Station {
    fn frame(&self) -> SketchFrame {
        SketchFrame {
            origin: self.origin,
            x_axis: self.x,
            y_axis: self.y,
        }
    }
    fn moved(&self, t: &Transform) -> Self {
        Self {
            origin: t.apply_point(self.origin),
            x: t.apply_vector(self.x),
            y: t.apply_vector(self.y),
            f: t.apply_vector(self.f),
        }
    }
    fn agrees(&self, other: &Self) -> bool {
        distance(self.origin, other.origin) <= CLOSURE
            && [(self.x, other.x), (self.y, other.y), (self.f, other.f)]
                .iter()
                .all(|(a, b)| distance(*a, *b) <= 1e-8)
    }
}

fn front_radius(c: &CylinderEmbedding, thickness: f64) -> f64 {
    if c.inner_is_front {
        c.inside_radius
    } else {
        c.inside_radius + thickness
    }
}

pub(crate) fn station(embedding: &SheetEmbedding, u: f64, thickness: f64) -> Station {
    match embedding {
        SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } => Station {
            origin: front_frame.point([u, 0.0]),
            x: front_frame.x_axis,
            y: front_frame.y_axis,
            f: scale(front_frame.normal(), front_to_back_sign.sign()),
        },
        SheetEmbedding::Cylinder(c) => {
            let phi = u / c.neutral_radius;
            let radial = add(
                scale(c.radial_start, phi.cos()),
                scale(c.angular_direction, phi.sin()),
            );
            let tangent = add(
                scale(c.radial_start, -phi.sin()),
                scale(c.angular_direction, phi.cos()),
            );
            Station {
                origin: add(c.centre, scale(radial, front_radius(c, thickness))),
                x: tangent,
                y: c.axis,
                f: if c.inner_is_front {
                    radial
                } else {
                    scale(radial, -1.0)
                },
            }
        }
    }
}

/// A chart point on the front surface (`depth` 0) or inside the material
/// (`depth` towards the back, `0..=thickness`).
pub(crate) fn chart_point(
    embedding: &SheetEmbedding,
    p: [f64; 2],
    depth: f64,
    thickness: f64,
) -> V3 {
    // On a cylinder, f is the signed radial direction: depth changes the radius.
    let s = station(embedding, p[0], thickness);
    add(add(s.origin, scale(s.y, p[1])), scale(s.f, depth))
}

/// The inverse of [`chart_point`]: chart coordinates and depth of a model
/// point, or None when it is outside the chart's angular interval.
pub(crate) fn chart_coordinates(
    embedding: &SheetEmbedding,
    point: V3,
    thickness: f64,
) -> Option<([f64; 2], f64)> {
    match embedding {
        SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } => {
            let d = sub(point, front_frame.origin);
            Some((
                [dot(d, front_frame.x_axis), dot(d, front_frame.y_axis)],
                dot(d, front_frame.normal()) * front_to_back_sign.sign(),
            ))
        }
        SheetEmbedding::Cylinder(c) => {
            let d = sub(point, c.centre);
            let v = dot(d, c.axis);
            let w = sub(d, scale(c.axis, v));
            let radius = dot(w, w).sqrt();
            let phi = dot(w, c.angular_direction).atan2(dot(w, c.radial_start));
            // Past half a turn (a hem's bend, mitcad#1378) the angle goes on
            // beyond 180 degrees.
            let phi = if phi < -1e-9 && c.angle_extent > std::f64::consts::PI {
                phi + std::f64::consts::TAU
            } else {
                phi
            };
            if radius <= TOLERANCE || phi < -1e-9 || phi > c.angle_extent + 1e-9 {
                return None;
            }
            let depth = if c.inner_is_front {
                radius - c.inside_radius
            } else {
                c.inside_radius + thickness - radius
            };
            Some(([phi.max(0.0) * c.neutral_radius, v], depth))
        }
    }
}

/// The neutral strip of a bend developed in the tangent plane of `entry`,
/// the station at chart coordinate `u`.
fn developed(entry: &Station, u: f64) -> SheetEmbedding {
    let normal = cross(entry.x, entry.y);
    SheetEmbedding::Plane {
        front_frame: SketchFrame {
            origin: sub(entry.origin, scale(entry.x, u)),
            x_axis: entry.x,
            y_axis: entry.y,
        },
        front_to_back_sign: if dot(entry.f, normal) > 0.0 {
            MaterialParity::Positive
        } else {
            MaterialParity::Negative
        },
    }
}

/// The cylinder of a bend refolded from `entry`, the station at chart
/// coordinate `u`. Radii, angle and material-side parity are the bend's own;
/// only the placement follows the entry side.
fn refolded(
    entry: &Station,
    u: f64,
    shape: &CylinderEmbedding,
    thickness: f64,
) -> CylinderEmbedding {
    let phi = u / shape.neutral_radius;
    let radial = if shape.inner_is_front {
        entry.f
    } else {
        scale(entry.f, -1.0)
    };
    CylinderEmbedding {
        centre: sub(entry.origin, scale(radial, front_radius(shape, thickness))),
        axis: entry.y,
        radial_start: sub(scale(radial, phi.cos()), scale(entry.x, phi.sin())),
        angular_direction: add(scale(radial, phi.sin()), scale(entry.x, phi.cos())),
        ..shape.clone()
    }
}

fn boundary_edges<'a>(
    patch: &'a SheetPatch,
    side: &'a ShapePortSide,
) -> impl Iterator<Item = &'a MaterialEdge> {
    patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .filter(move |e| {
            e.piece == side.boundary.piece && {
                let (lo, hi) = (
                    e.curve.range.first.min(e.curve.range.last),
                    e.curve.range.first.max(e.curve.range.last),
                );
                side.span.first.min(side.span.last) >= lo - 1e-9
                    && side.span.first.max(side.span.last) <= hi + 1e-9
            }
        })
}

/// The tangent line of a bend chart that a port boundary lies on: 0 or BA.
pub(crate) fn bend_side(
    patch: &SheetPatch,
    side: &ShapePortSide,
    allowance: f64,
) -> Result<f64, String> {
    let mut found: Option<f64> = None;
    for edge in boundary_edges(patch, side) {
        if !matches!(edge.curve.geometry, SegmentGeometry::Line { .. }) {
            return Err(format!(
                "a seam of {} is not a straight tangent line",
                domain_label(patch.domain)
            ));
        }
        let (a, b) = edge.curve.ends()?;
        let u = if a[0].abs() <= CLOSURE && b[0].abs() <= CLOSURE {
            0.0
        } else if (a[0] - allowance).abs() <= CLOSURE && (b[0] - allowance).abs() <= CLOSURE {
            allowance
        } else {
            return Err(format!(
                "a seam of {} does not lie on one of its tangent lines",
                domain_label(patch.domain)
            ));
        };
        if found.is_some_and(|old| old != u) {
            return Err(format!(
                "a seam of {} has ambiguous tangent-line provenance",
                domain_label(patch.domain)
            ));
        }
        found = Some(u);
    }
    found.ok_or_else(|| {
        format!(
            "a registered seam of {} has no material boundary",
            domain_label(patch.domain)
        )
    })
}

/// A point of a port in the current configuration, for closure checks.
fn port_point(patch: &SheetPatch, side: &ShapePortSide, thickness: f64) -> Result<V3, String> {
    let edge = boundary_edges(patch, side).next().ok_or_else(|| {
        format!(
            "a registered seam of {} has no material boundary",
            domain_label(patch.domain)
        )
    })?;
    Ok(chart_point(
        &patch.embedding,
        edge.curve.point(side.span.first)?,
        0.0,
        thickness,
    ))
}

fn agree(a: &Transform, b: &Transform, at: V3) -> bool {
    [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .iter()
        .all(|offset| {
            let p = add(at, *offset);
            distance(a.apply_point(p), b.apply_point(p)) <= CLOSURE
        })
}

/// The bend's record, checked against the rules the sheet is evaluated
/// with: a stale allowance, radius or parity is never developed.
pub(crate) fn bend_record(
    state: &SheetState,
    id: BendId,
    thickness: f64,
) -> Result<&BendDomain, String> {
    let label = domain_label(SheetDomainRef::Bend(id));
    let bend = state
        .bends
        .iter()
        .find(|b| b.id == id)
        .ok_or_else(|| format!("{label} is not a bend of this sheet"))?;
    let c = &bend.folded_embedding;
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    if !near(c.inside_radius, bend.radius)
        || !near(c.neutral_radius, bend.radius + bend.k_factor * thickness)
        || !near(bend.allowance, c.neutral_radius * c.angle_extent)
        || !near(c.angle_extent, bend.signed_angle.abs())
        || c.inner_is_front != (bend.signed_angle < 0.0)
        || bend.rules != state.rules
        || bend.rules_version != state.rules_version
    {
        return Err(format!(
            "{label} does not match the sheet's current rules; its development is stale"
        ));
    }
    Ok(bend)
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
enum Placement {
    Rigid(Transform),
    Changed {
        entry: f64,
        entry_t: Transform,
        exit_t: Transform,
        embedding: SheetEmbedding,
    },
}

fn changed(
    patch: &SheetPatch,
    record: &BendDomain,
    open: bool,
    side: f64,
    carried: Transform,
    thickness: f64,
) -> Result<Placement, String> {
    let entry = station(&patch.embedding, side, thickness).moved(&carried);
    let embedding = if open {
        developed(&entry, side)
    } else {
        SheetEmbedding::Cylinder(refolded(&entry, side, &record.folded_embedding, thickness))
    };
    let label = domain_label(patch.domain);
    if !station(&embedding, side, thickness).agrees(&entry) {
        return Err(format!(
            "{label} lost its entry seam while changing configuration"
        ));
    }
    let other = record.allowance - side;
    let before = station(&patch.embedding, other, thickness);
    let after = station(&embedding, other, thickness);
    let exit_t = Transform::between_frames(&before.frame(), &after.frame());
    if !before.moved(&exit_t).agrees(&after) {
        return Err(format!(
            "{label} has inconsistent material-side parity between its tangent lines"
        ));
    }
    Ok(Placement::Changed {
        entry: side,
        entry_t: carried,
        exit_t,
        embedding,
    })
}

/// Changes the listed bends to developed (`true`) or folded (`false`) while
/// `stationary` stays fixed. Every other domain is carried rigidly.
pub(crate) fn reconfigure(
    state: &mut SheetState,
    thickness: f64,
    stationary: PanelId,
    targets: &BTreeMap<BendId, bool>,
) -> Result<(), String> {
    let fixed = SheetDomainRef::Panel(stationary);
    let index: BTreeMap<SheetDomainRef, usize> = state
        .charts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.domain, i))
        .collect();
    let fixed_patch = index
        .get(&fixed)
        .map(|i| &state.charts[*i])
        .ok_or_else(|| {
            format!(
                "stationary {} is not a material domain of this sheet",
                domain_label(fixed)
            )
        })?;
    if !matches!(fixed_patch.embedding, SheetEmbedding::Plane { .. }) {
        return Err(format!("stationary {} is not planar", domain_label(fixed)));
    }
    for bend in &state.bends {
        bend_record(state, bend.id, thickness)?;
    }
    let mut changing: BTreeMap<BendId, (BendDomain, bool)> = BTreeMap::new();
    for (id, open) in targets {
        let record = bend_record(state, *id, thickness)?.clone();
        let domain = SheetDomainRef::Bend(*id);
        let patch = index
            .get(&domain)
            .map(|i| &state.charts[*i])
            .ok_or_else(|| format!("{} has no material chart", domain_label(domain)))?;
        if matches!(patch.embedding, SheetEmbedding::Plane { .. }) != *open {
            changing.insert(*id, (record, *open));
        }
    }
    let mut placements: BTreeMap<SheetDomainRef, Placement> = BTreeMap::new();
    placements.insert(fixed, Placement::Rigid(Transform::IDENTITY));
    let mut queue = VecDeque::from([fixed]);
    while let Some(domain) = queue.pop_front() {
        let patch = &state.charts[index[&domain]];
        for port in &state.joins {
            for (own, other) in [(&port.first, &port.second), (&port.second, &port.first)] {
                if own.boundary.domain != domain {
                    continue;
                }
                let carried = match &placements[&domain] {
                    Placement::Rigid(t) => *t,
                    Placement::Changed {
                        entry,
                        entry_t,
                        exit_t,
                        ..
                    } => {
                        let SheetDomainRef::Bend(id) = domain else {
                            unreachable!("only bends change configuration")
                        };
                        let side = bend_side(patch, own, changing[&id].0.allowance)?;
                        if (side - entry).abs() <= CLOSURE {
                            *entry_t
                        } else {
                            *exit_t
                        }
                    }
                };
                let next = other.boundary.domain;
                let next_patch = &state.charts[*index.get(&next).ok_or_else(|| {
                    format!("a registered seam refers to missing {}", domain_label(next))
                })?];
                let existing = match (next, placements.get(&next)) {
                    (_, None) => None,
                    (_, Some(Placement::Rigid(t))) => Some(*t),
                    (
                        SheetDomainRef::Bend(id),
                        Some(Placement::Changed {
                            entry,
                            entry_t,
                            exit_t,
                            ..
                        }),
                    ) => {
                        let side = bend_side(next_patch, other, changing[&id].0.allowance)?;
                        Some(if (side - entry).abs() <= CLOSURE {
                            *entry_t
                        } else {
                            *exit_t
                        })
                    }
                    (SheetDomainRef::Panel(_), Some(Placement::Changed { .. })) => {
                        unreachable!("panels are always rigid")
                    }
                };
                if let Some(existing) = existing {
                    if !agree(&existing, &carried, port_point(patch, own, thickness)?) {
                        return Err(format!(
                            "non-developable cycle: {} and {} do not close in this configuration",
                            domain_label(domain),
                            domain_label(next)
                        ));
                    }
                    continue;
                }
                let placement = match next {
                    SheetDomainRef::Bend(id) if changing.contains_key(&id) => {
                        let (record, open) = &changing[&id];
                        let side = bend_side(next_patch, other, record.allowance)?;
                        changed(next_patch, record, *open, side, carried, thickness)?
                    }
                    _ => Placement::Rigid(carried),
                };
                placements.insert(next, placement);
                queue.push_back(next);
            }
        }
    }
    if let Some(missing) = state
        .charts
        .iter()
        .find(|p| !placements.contains_key(&p.domain))
    {
        return Err(format!(
            "{} is not connected to the stationary panel through registered seams",
            domain_label(missing.domain)
        ));
    }
    for patch in &mut state.charts {
        patch.embedding = match &placements[&patch.domain] {
            Placement::Rigid(t) => patch.embedding.transformed(t),
            Placement::Changed { embedding, .. } => embedding.clone(),
        };
    }
    // The stored cylinder of an open bend is the one refolding would build
    // while holding its u=0 side; a folded bend's is its chart.
    for bend in &mut state.bends {
        let domain = SheetDomainRef::Bend(bend.id);
        let patch = &state.charts[index[&domain]];
        bend.folded_embedding = match &patch.embedding {
            SheetEmbedding::Cylinder(c) => c.clone(),
            plane => refolded(
                &station(plane, 0.0, thickness),
                0.0,
                &bend.folded_embedding,
                thickness,
            ),
        };
    }
    Ok(())
}

/// The isometry from the chart of `from` to the chart coordinates of `to`;
/// both planar, coplanar and with the same front-to-back direction.
pub(crate) fn planar_map(from: &SheetEmbedding, to: &SheetEmbedding) -> Result<Isometry2, String> {
    let (
        SheetEmbedding::Plane {
            front_frame: a,
            front_to_back_sign: sa,
        },
        SheetEmbedding::Plane {
            front_frame: b,
            front_to_back_sign: sb,
        },
    ) = (from, to)
    else {
        return Err("a developed material chart is not planar".into());
    };
    let fa = scale(a.normal(), sa.sign());
    let fb = scale(b.normal(), sb.sign());
    if dot(fa, fb) < 1.0 - ANGULAR || dot(sub(a.origin, b.origin), fb).abs() > CLOSURE {
        return Err("developed material charts are not coplanar".into());
    }
    let d = sub(a.origin, b.origin);
    Ok(Isometry2 {
        origin: [dot(d, b.x_axis), dot(d, b.y_axis)],
        x_axis: [dot(a.x_axis, b.x_axis), dot(a.x_axis, b.y_axis)],
        y_axis: [dot(a.y_axis, b.x_axis), dot(a.y_axis, b.y_axis)],
    })
}

/// `first`, then `second`.
pub(crate) fn then(first: Isometry2, second: Isometry2) -> Isometry2 {
    let vector = |v: [f64; 2]| {
        [
            v[0] * second.x_axis[0] + v[1] * second.y_axis[0],
            v[0] * second.x_axis[1] + v[1] * second.y_axis[1],
        ]
    };
    Isometry2 {
        origin: second.point(first.origin),
        x_axis: vector(first.x_axis),
        y_axis: vector(first.y_axis),
    }
}

pub(crate) fn inverse(map: Isometry2) -> Isometry2 {
    // The linear part is orthogonal: its inverse is its transpose.
    let x_axis = [map.x_axis[0], map.y_axis[0]];
    let y_axis = [map.x_axis[1], map.y_axis[1]];
    let o = map.origin;
    Isometry2 {
        origin: [
            -(o[0] * x_axis[0] + o[1] * y_axis[0]),
            -(o[0] * x_axis[1] + o[1] * y_axis[1]),
        ],
        x_axis,
        y_axis,
    }
}

fn edge_area(curve: &BoundedCurve2) -> Result<f64, String> {
    let (t0, t1) = (curve.range.first, curve.range.last);
    Ok(match &curve.geometry {
        SegmentGeometry::Line { .. } => {
            let (a, b) = curve.ends()?;
            (a[0] * b[1] - b[0] * a[1]) / 2.0
        }
        SegmentGeometry::Circle { center, radius, .. }
        | SegmentGeometry::Arc { center, radius, .. } => {
            let (c, r) = (*center, *radius);
            0.5 * (r * c[0] * (t1.sin() - t0.sin()) - r * c[1] * (t1.cos() - t0.cos())
                + r * r * (t1 - t0))
        }
        _ => {
            // A measure, not a construction: dense samples of the exact support.
            let support = curve.support()?;
            let n = 2048;
            let points: Vec<_> = (0..=n)
                .map(|i| support.point(t0 + (t1 - t0) * f64::from(i) / f64::from(n)))
                .collect();
            points
                .windows(2)
                .map(|w| (w[0][0] * w[1][1] - w[1][0] * w[0][1]) / 2.0)
                .sum()
        }
    })
}

/// The signed area of a loop: positive when it runs counter-clockwise.
pub(crate) fn loop_area(l: &MaterialLoop) -> Result<f64, String> {
    let mut signed = 0.0;
    for e in &l.edges {
        signed += edge_area(&e.curve)?;
    }
    Ok(signed)
}

/// Exterior counter-clockwise, holes clockwise.
pub(crate) fn orient(region: &mut MaterialRegion) -> Result<(), String> {
    for (i, l) in region.loops.iter_mut().enumerate() {
        if (loop_area(l)? > 0.0) != (i == 0) {
            l.edges.reverse();
            for e in &mut l.edges {
                e.curve.range = e.curve.range.reversed();
            }
        }
    }
    Ok(())
}

/// The material area of a region: its exterior less its holes.
pub(crate) fn region_area(region: &MaterialRegion) -> Result<f64, String> {
    let mut total = 0.0;
    for (i, l) in region.loops.iter().enumerate() {
        let signed = loop_area(l)?.abs();
        total += if i == 0 { signed } else { -signed };
    }
    Ok(total)
}

/// The material intervals of the chart line `u = const`, as `[v0, v1]`.
pub(crate) fn chord(regions: &[MaterialRegion], u: f64) -> Result<Vec<[f64; 2]>, String> {
    let mut crossings = Vec::new();
    for edge in regions.iter().flat_map(|r| &r.loops).flat_map(|l| &l.edges) {
        let support = edge.curve.support()?;
        let (t0, t1) = (edge.curve.range.first, edge.curve.range.last);
        let n = if matches!(edge.curve.geometry, SegmentGeometry::Line { .. }) {
            1
        } else {
            256
        };
        let at = |i: u32| t0 + (t1 - t0) * f64::from(i) / f64::from(n);
        for i in 0..n {
            let (mut a, mut b) = (at(i), at(i + 1));
            let (pa, pb) = (support.point(a), support.point(b));
            // Half-open, so a crossing through a vertex counts once.
            if !((pa[0] < u && u <= pb[0]) || (pb[0] < u && u <= pa[0])) {
                continue;
            }
            let v = if n == 1 {
                let s = (u - pa[0]) / (pb[0] - pa[0]);
                pa[1] + s * (pb[1] - pa[1])
            } else {
                let rising = pa[0] < pb[0];
                for _ in 0..60 {
                    let m = (a + b) / 2.0;
                    if (support.point(m)[0] < u) == rising {
                        a = m;
                    } else {
                        b = m;
                    }
                }
                support.point((a + b) / 2.0)[1]
            };
            crossings.push(v);
        }
    }
    crossings.sort_by(f64::total_cmp);
    Ok(crossings
        .chunks(2)
        .filter(|c| c.len() == 2 && c[1] - c[0] > CLOSURE)
        .map(|c| [c[0], c[1]])
        .collect())
}

fn region_bounds(regions: &[MaterialRegion]) -> Result<([f64; 2], [f64; 2]), String> {
    bounds(&SheetPatch {
        domain: SheetDomainRef::Panel(PanelId {
            feature: FeatureUid(0),
            slot: 0,
        }),
        regions: regions.to_vec(),
        embedding: SheetEmbedding::Plane {
            front_frame: SketchFrame::XY,
            front_to_back_sign: MaterialParity::Positive,
        },
    })
}

/// Rejects material of distinct domains occupying the same place in one
/// plane: developed panels, strips, holes and reliefs. Common seams are not
/// overlaps. Folded configurations are checked by the kernel's collision
/// test of the constructed solid.
pub(crate) fn check_overlaps<K: Kernel>(kernel: &K, state: &SheetState) -> Result<(), String> {
    let planar: Vec<&SheetPatch> = state
        .charts
        .iter()
        .filter(|p| matches!(p.embedding, SheetEmbedding::Plane { .. }))
        .collect();
    let mut groups: Vec<Vec<&SheetPatch>> = Vec::new();
    for patch in planar {
        match groups
            .iter_mut()
            .find(|g| planar_map(&patch.embedding, &g[0].embedding).is_ok())
        {
            Some(group) => group.push(patch),
            None => groups.push(vec![patch]),
        }
    }
    for group in groups.iter().filter(|g| g.len() > 1) {
        let mut mapped = Vec::new();
        for patch in group {
            let regions = mapped_regions(
                &patch.regions,
                planar_map(&patch.embedding, &group[0].embedding)?,
            );
            let (min, max) = region_bounds(&regions)?;
            let mut area = 0.0;
            for region in &regions {
                area += region_area(region)?;
            }
            mapped.push((patch.domain, regions, min, max, area));
        }
        for (i, a) in mapped.iter().enumerate() {
            for b in &mapped[i + 1..] {
                let overlap = |k: usize| a.3[k].min(b.3[k]) - a.2[k].max(b.2[k]);
                if overlap(0) <= CLOSURE || overlap(1) <= CLOSURE {
                    continue;
                }
                let result = kernel
                    .sheet_trim_domain(&SheetTrimSpec {
                        operation: SheetTrimOperation::Intersection,
                        domains: &[TrimDomain {
                            domain: a.0,
                            regions: a.1.clone(),
                            to_operation: Isometry2::IDENTITY,
                        }],
                        tools: &b.1,
                        seams: &[],
                        tolerance: TOLERANCE,
                    })
                    .map_err(|e| e.to_string())?;
                let mut common = 0.0;
                for region in result.domains.iter().flat_map(|d| &d.regions) {
                    let material = MaterialRegion {
                        loops: region
                            .loops
                            .iter()
                            .map(|l| MaterialLoop {
                                edges: l
                                    .edges
                                    .iter()
                                    .map(|e| MaterialEdge {
                                        piece: generated(FeatureUid(0), e.token),
                                        curve: e.curve.clone(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    };
                    common += region_area(&material)?;
                }
                if common > 1e-6_f64.max(1e-9 * a.4.min(b.4)) {
                    return Err(format!(
                        "sheet self-overlap: {} and {} occupy the same material area ({common:.6} mm2)",
                        domain_label(a.0),
                        domain_label(b.0)
                    ));
                }
            }
        }
    }
    Ok(())
}

fn targets_of(bends: &[BendId], open: bool) -> BTreeMap<BendId, bool> {
    bends.iter().map(|b| (*b, open)).collect()
}

pub(crate) fn unfold<K: Kernel>(
    d: &SheetUnfoldDef,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    state.overlay_refusal("sheet Unfold")?;
    if d.bends.is_empty() {
        return Err("sheet Unfold needs an explicit nonempty set of bends".into());
    }
    for bend in &d.bends {
        let label = domain_label(SheetDomainRef::Bend(*bend));
        bend_record(&state, *bend, rules.thickness)?;
        if let Some(open) = state.opened.iter().find(|o| o.bend == *bend) {
            return Err(format!("{label} is already open by {}", open.by));
        }
        let patch = state
            .charts
            .iter()
            .find(|p| p.domain == SheetDomainRef::Bend(*bend))
            .ok_or_else(|| format!("{label} has no material chart"))?;
        if !matches!(patch.embedding, SheetEmbedding::Cylinder(_)) {
            return Err(format!("{label} is not folded"));
        }
    }
    reconfigure(
        &mut state,
        rules.thickness,
        d.stationary,
        &targets_of(&d.bends, true),
    )?;
    state.opened.extend(d.bends.iter().map(|bend| OpenedBend {
        bend: *bend,
        by: ctx.uid,
        stationary: Some(d.stationary),
    }));
    check_overlaps(ctx.kernel, &state)?;
    publish(ctx, state, &rules, None)
}

pub(crate) fn refold<K: Kernel>(
    d: &SheetRefoldDef,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    state.overlay_refusal("sheet Refold")?;
    let owned: Vec<OpenedBend> = state
        .opened
        .iter()
        .filter(|o| o.by == d.unfold)
        .cloned()
        .collect();
    let bends: Vec<BendId> = if d.bends.is_empty() {
        owned.iter().map(|o| o.bend).collect()
    } else {
        for bend in &d.bends {
            let label = domain_label(SheetDomainRef::Bend(*bend));
            if !owned.iter().any(|o| o.bend == *bend) {
                return Err(match state.opened.iter().find(|o| o.bend == *bend) {
                    Some(other) => format!(
                        "{label} is open by {}, not by {}; Refold cannot close it",
                        other.by, d.unfold
                    ),
                    None => format!("{label} is not open; Refold cannot close it again"),
                });
            }
        }
        d.bends.clone()
    };
    if bends.is_empty() {
        return Err(format!(
            "sheet Unfold {} has no open bends of this sheet to refold",
            d.unfold
        ));
    }
    let mut stationary = None;
    for o in owned.iter().filter(|o| bends.contains(&o.bend)) {
        let panel = o.stationary.ok_or_else(|| {
            format!(
                "the stationary panel of {} is not recorded",
                domain_label(SheetDomainRef::Bend(o.bend))
            )
        })?;
        if stationary.is_some_and(|s| s != panel) {
            return Err("the refolded bends were opened against different panels".into());
        }
        stationary = Some(panel);
    }
    let stationary = stationary.expect("nonempty");
    reconfigure(
        &mut state,
        rules.thickness,
        stationary,
        &targets_of(&bends, false),
    )?;
    state.opened.retain(|o| !bends.contains(&o.bend));
    check_overlaps(ctx.kernel, &state)?;
    publish(ctx, state, &rules, None)
}

#[cfg(test)]
#[path = "develop_tests.rs"]
mod tests;
