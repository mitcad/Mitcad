// SPDX-License-Identifier: MIT
//! Projection of model geometry into a sketch (Project): the
//! kernel lists the curves of an edge, a face's boundary or a vertex
//! ([`crate::Kernel::curves_of`]); they are projected along the sketch
//! normal onto the plane and become fixed reference entities. Lines stay
//! lines, circles in parallel planes stay circles, other circles and
//! ellipses become ellipses (or lines seen edge on), B-splines keep their
//! knots with projected poles. Ends that meet share a point. A linked
//! projection is recorded ([`Projection`]) and its entities follow the
//! source at every evaluation; when the source changes kind or count, the
//! entities keep their last geometry (lost projections are kept).

use std::collections::BTreeMap;

use super::geometry::{Curve2, P2, add, angle_after, dist, norm, scale, sub};
use super::{Entity, EntityKind, Projection, Ref};
use crate::features::EvalContext;
use crate::features::sketch::SketchDef;
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{Curve3, Kernel};
use crate::profile::{SketchFrame, cross, dot};
use crate::topo::TopoName;

/// A projected primitive in sketch coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum Projected {
    Point(P2),
    Curve(Curve2),
}

fn to_sketch(frame: &SketchFrame, p: [f64; 3]) -> P2 {
    let d: [f64; 3] = std::array::from_fn(|i| p[i] - frame.origin[i]);
    [dot(d, frame.x_axis), dot(d, frame.y_axis)]
}

fn direction(frame: &SketchFrame, v: [f64; 3]) -> P2 {
    [dot(v, frame.x_axis), dot(v, frame.y_axis)]
}

/// The principal axes of the ellipse `M (cos t, sin t)` for `M = [a b]`:
/// semi-axes and the major axis direction.
fn principal_axes(a: P2, b: P2) -> (f64, f64, f64) {
    // M M^T = [[p, q], [q, r]].
    let p = a[0] * a[0] + b[0] * b[0];
    let q = a[0] * a[1] + b[0] * b[1];
    let r = a[1] * a[1] + b[1] * b[1];
    let mean = (p + r) / 2.0;
    let spread = (((p - r) / 2.0).powi(2) + q * q).sqrt();
    let major = (mean + spread).max(0.0).sqrt();
    let minor = (mean - spread).max(0.0).sqrt();
    let rotation = 0.5 * (2.0 * q).atan2(p - r);
    (major, minor, rotation)
}

/// Projects model curves onto the sketch plane.
pub fn project(curves: &[Curve3], frame: &SketchFrame) -> Vec<Projected> {
    let mut out = Vec::new();
    for curve in curves {
        match curve {
            Curve3::Point(p) => out.push(Projected::Point(to_sketch(frame, *p))),
            Curve3::Line { start, end } => {
                let (a, b) = (to_sketch(frame, *start), to_sketch(frame, *end));
                if dist(a, b) <= 1e-9 * (1.0 + norm(a)) {
                    out.push(Projected::Point(a));
                } else {
                    out.push(Projected::Curve(Curve2::Line { a, b }));
                }
            }
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
                let y_axis = cross(*normal, *x_axis);
                let c = to_sketch(frame, *center);
                let ax = scale(direction(frame, *x_axis), *major);
                let ay = scale(direction(frame, y_axis), *minor);
                let at = |t: f64| add(c, add(scale(ax, t.cos()), scale(ay, t.sin())));
                let (big, small, rotation) = principal_axes(ax, ay);
                let size = big.max(1e-300);
                if small <= 1e-9 * size {
                    // Seen edge on: the extreme points of the curve.
                    let ts: Vec<f64> = (0..=64)
                        .map(|i| start + (end - start) * i as f64 / 64.0)
                        .collect();
                    let u = [rotation.cos(), rotation.sin()];
                    let along = |t: &f64| dot2(sub(at(*t), c), u);
                    let lo = ts
                        .iter()
                        .copied()
                        .min_by(|a, b| along(a).total_cmp(&along(b)));
                    let hi = ts
                        .iter()
                        .copied()
                        .max_by(|a, b| along(a).total_cmp(&along(b)));
                    if let (Some(lo), Some(hi)) = (lo, hi) {
                        out.push(Projected::Curve(Curve2::Line {
                            a: at(lo),
                            b: at(hi),
                        }));
                    }
                    continue;
                }
                let round = (big - small).abs() <= 1e-9 * size;
                let ellipse = Curve2::Ellipse {
                    center: c,
                    major: big,
                    minor: small,
                    rotation,
                };
                let param = |p: P2| -> f64 {
                    if round {
                        let v = sub(p, c);
                        v[1].atan2(v[0])
                    } else {
                        ellipse.closest(p).0
                    }
                };
                if *closed {
                    out.push(Projected::Curve(if round {
                        Curve2::Circle {
                            center: c,
                            radius: big,
                        }
                    } else {
                        ellipse
                    }));
                    continue;
                }
                let (p0, p1, pm) = (at(*start), at(*end), at((start + end) / 2.0));
                let (mut t0, mut t1) = (param(p0), param(p1));
                let tm = param(pm);
                // Counter-clockwise from t0 through the middle to t1, or the
                // other way round.
                if angle_after(t0, tm) > angle_after(t0, t1) {
                    std::mem::swap(&mut t0, &mut t1);
                }
                let t1 = angle_after(t0, t1);
                out.push(Projected::Curve(if round {
                    Curve2::Arc {
                        center: c,
                        radius: big,
                        start: t0,
                        end: t1,
                    }
                } else {
                    Curve2::EllipticalArc {
                        center: c,
                        major: big,
                        minor: small,
                        rotation,
                        start: t0,
                        end: t1,
                    }
                }));
            }
            Curve3::BSpline {
                degree,
                poles,
                weights,
                knots,
            } => {
                out.push(Projected::Curve(Curve2::Nurbs(super::geometry::Nurbs {
                    degree: *degree as usize,
                    control: poles.iter().map(|p| to_sketch(frame, *p)).collect(),
                    weights: weights.clone(),
                    knots: knots.clone(),
                })));
            }
        }
    }
    out
}

fn dot2(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Entities for projected primitives: fixed reference geometry, with
/// coinciding ends sharing a point. `next` gives fresh ids.
pub fn entities(projected: &[Projected], next: &mut dyn FnMut() -> EntityUid) -> Vec<Entity> {
    let mut out: Vec<Entity> = Vec::new();
    let mut points: Vec<(P2, EntityUid)> = Vec::new();
    let mut size = 1.0_f64;
    for p in projected {
        if let Projected::Curve(c) = p {
            let (lo, hi) = c.bounds();
            size = size.max(dist(lo, hi));
        }
    }
    let tol = 1e-9 * size;
    // Points are made through `point`, curve ids through `next`.
    let ids = std::cell::RefCell::new(next);
    let mut point = |out: &mut Vec<Entity>, at: P2, shared: bool| -> EntityUid {
        if shared && let Some((_, id)) = points.iter().find(|(p, _)| dist(*p, at) <= tol) {
            return *id;
        }
        let id = (ids.borrow_mut())();
        let mut e = Entity::point(id, at);
        e.fixed = true;
        e.reference = true;
        out.push(e);
        if shared {
            points.push((at, id));
        }
        id
    };
    for p in projected {
        let curve = match p {
            Projected::Point(at) => {
                point(&mut out, *at, true);
                continue;
            }
            Projected::Curve(c) => c,
        };
        let id = (ids.borrow_mut())();
        let kind = match curve {
            Curve2::Line { a, b } => EntityKind::Line {
                start: point(&mut out, *a, true),
                end: point(&mut out, *b, true),
                centerline: false,
            },
            Curve2::Circle { center, radius } => EntityKind::Circle {
                center: point(&mut out, *center, false),
                radius: *radius,
            },
            Curve2::Arc { center, .. } => {
                let (s, e) = curve.ends().expect("an arc has ends");
                EntityKind::Arc {
                    center: point(&mut out, *center, false),
                    start: point(&mut out, s, true),
                    end: point(&mut out, e, true),
                }
            }
            Curve2::Ellipse {
                center,
                major,
                minor,
                rotation,
            } => EntityKind::Ellipse {
                center: point(&mut out, *center, false),
                major: point(
                    &mut out,
                    add(*center, scale([rotation.cos(), rotation.sin()], *major)),
                    false,
                ),
                minor_radius: *minor,
            },
            Curve2::EllipticalArc {
                center,
                major,
                minor,
                rotation,
                ..
            } => {
                let (s, e) = curve.ends().expect("an arc has ends");
                EntityKind::EllipticalArc {
                    center: point(&mut out, *center, false),
                    major: point(
                        &mut out,
                        add(*center, scale([rotation.cos(), rotation.sin()], *major)),
                        false,
                    ),
                    minor_radius: *minor,
                    start: point(&mut out, s, true),
                    end: point(&mut out, e, true),
                }
            }
            Curve2::Nurbs(n) => {
                let last = n.control.len() - 1;
                let control = n
                    .control
                    .iter()
                    .enumerate()
                    .map(|(i, c)| point(&mut out, *c, i == 0 || i == last))
                    .collect();
                EntityKind::Spline {
                    degree: n.degree as u32,
                    control,
                    weights: n.weights.clone(),
                    knots: n.knots.clone(),
                }
            }
        };
        let mut e = Entity::new(id, kind);
        e.fixed = true;
        e.reference = true;
        out.push(e);
    }
    out
}

/// The kind of each entity, to tell whether new geometry fits old ids.
fn shape_of(entities: &[Entity]) -> Vec<(&'static str, Vec<usize>)> {
    let position: BTreeMap<EntityUid, usize> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id, i))
        .collect();
    entities
        .iter()
        .map(|e| {
            (
                e.kind.type_name(),
                e.kind
                    .points()
                    .iter()
                    .map(|p| position.get(p).copied().unwrap_or(usize::MAX))
                    .collect(),
            )
        })
        .collect()
}

/// The model curves of a projection's source.
fn source_curves<K: Kernel>(
    source: &TopoName,
    body: Option<BodyUid>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<Vec<Curve3>, String> {
    let shapes = match body {
        Some(body) => vec![(body, ctx.body(body)?)],
        None => ctx.bodies(),
    };
    for (_, shape) in shapes {
        let curves = ctx
            .kernel
            .curves_of(&shape, source)
            .map_err(|e| format!("projection of {source}: {e}"))?;
        if !curves.is_empty() {
            return Ok(curves);
        }
    }
    Ok(Vec::new())
}

/// The definition with linked projections moved to their sources' current
/// geometry, or None when there are none or nothing moved.
pub fn follow_links<K: Kernel>(
    def: &SketchDef,
    frame: &SketchFrame,
    ctx: &mut EvalContext<'_, K>,
) -> Result<Option<SketchDef>, String> {
    if def.projections.is_empty() {
        return Ok(None);
    }
    let mut updated = def.clone();
    let mut changed = false;
    for projection in &def.projections {
        let curves = source_curves(&projection.source, projection.body, ctx)?;
        if curves.is_empty() {
            // A lost projection keeps its last geometry.
            continue;
        }
        // Fresh ids above the sketch's: the old ids replace them one at a
        // time, so no old id may also be a fresh one still to replace.
        let mut counter = def.entities.iter().map(|e| e.id.0).max().unwrap_or(0);
        let fresh = entities(&project(&curves, frame), &mut || {
            counter += 1;
            EntityUid(counter)
        });
        let old: Vec<Entity> = linked_entities(def, projection);
        if shape_of(&fresh) != shape_of(&old) {
            continue;
        }
        for (new, old) in fresh.iter().zip(&old) {
            if let Some(target) = updated.entity_mut(old.id) {
                let mut kind = new.kind.clone();
                // Keep the old point ids.
                for (from, to) in new.kind.points().iter().zip(old.kind.points()) {
                    kind.replace_point(*from, to);
                }
                if target.kind != kind {
                    target.kind = kind;
                    changed = true;
                }
            }
        }
    }
    Ok(changed.then_some(updated))
}

/// The entities of a projection with the points they use, in order:
/// each curve after its points, as [`entities`] makes them.
pub fn linked_entities(def: &SketchDef, projection: &Projection) -> Vec<Entity> {
    let mut out: Vec<Entity> = Vec::new();
    for r in &projection.entities {
        let Some(e) = def.entity(r.uid()) else {
            continue;
        };
        if !out.iter().any(|o| o.id == e.id) {
            out.push(e.clone());
        }
    }
    out
}

/// The refs of entities, points included, for a projection record.
pub fn refs(entities: &[Entity]) -> Vec<Ref> {
    entities.iter().map(Entity::as_ref).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_xz() -> SketchFrame {
        SketchFrame {
            origin: [0.0; 3],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 0.0, -1.0],
        }
    }

    #[test]
    fn conics_project_to_circles_ellipses_or_lines() {
        // A circle in the XY plane seen from XY: a circle.
        let circle = Curve3::Conic {
            center: [1.0, 2.0, 5.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 3.0,
            minor: 3.0,
            start: 0.0,
            end: std::f64::consts::TAU,
            closed: true,
        };
        let out = project(std::slice::from_ref(&circle), &SketchFrame::XY);
        assert_eq!(
            out,
            vec![Projected::Curve(Curve2::Circle {
                center: [1.0, 2.0],
                radius: 3.0
            })]
        );
        // Seen edge on from XZ: a line across its diameter.
        let out = project(std::slice::from_ref(&circle), &frame_xz());
        let Projected::Curve(Curve2::Line { a, b }) = &out[0] else {
            panic!("{out:?}");
        };
        assert!((dist(*a, *b) - 6.0).abs() < 1e-9);
        // Tilted 60 degrees: an ellipse with half the minor radius.
        let tilted = Curve3::Conic {
            center: [1.0, 2.0, 5.0],
            normal: [0.0, (60f64).to_radians().sin(), (60f64).to_radians().cos()],
            x_axis: [1.0, 0.0, 0.0],
            major: 3.0,
            minor: 3.0,
            start: 0.0,
            end: std::f64::consts::TAU,
            closed: true,
        };
        let Projected::Curve(Curve2::Ellipse { major, minor, .. }) =
            &project(&[tilted], &SketchFrame::XY)[0]
        else {
            panic!("an ellipse");
        };
        assert!((major - 3.0).abs() < 1e-9 && (minor - 1.5).abs() < 1e-9);
    }

    #[test]
    fn projected_ends_share_points() {
        let square: Vec<Curve3> = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
            .windows(2)
            .map(|w| Curve3::Line {
                start: [w[0][0], w[0][1], 3.0],
                end: [w[1][0], w[1][1], 3.0],
            })
            .chain([Curve3::Line {
                start: [0.0, 10.0, 3.0],
                end: [0.0, 0.0, 3.0],
            }])
            .collect();
        let mut n = 0;
        let entities = entities(&project(&square, &SketchFrame::XY), &mut || {
            n += 1;
            EntityUid(n)
        });
        assert_eq!(entities.iter().filter(|e| e.is_point()).count(), 4);
        assert_eq!(entities.iter().filter(|e| !e.is_point()).count(), 4);
        assert!(entities.iter().all(|e| e.fixed && e.reference));
    }
}
