// SPDX-License-Identifier: MIT
//! Projection of model geometry into a sketch (Project): the
//! kernel lists the curves of an edge, a face's boundary or a vertex
//! ([`mitcad_model_base::Kernel::curves_of`]); they are projected along the sketch
//! normal onto the plane and become fixed reference entities. Lines stay
//! lines, circles in parallel planes stay circles, other circles and
//! ellipses become ellipses (or lines seen edge on), B-splines keep their
//! knots with projected poles (a planar one seen edge on is the line
//! between its extreme points, mitcad#389). Ends that meet share a point;
//! a curve or point the same as another of the source is projected once.
//! A linked projection is recorded ([`Projection`]) and its entities follow the
//! source at every evaluation, each the fresh curve of its kind most like
//! it (mitcad#232); an entity that finds none, and the entities of a lost
//! source, keep their last geometry, and the sketch warns about them. A
//! pairing that would tear an arc from the curves it shares points with
//! (the source changed shape) moves nothing: the whole projection keeps
//! its last geometry and warns (mitcad#249). Projections of earlier
//! versions and imports (no [`Projection::by_geometry`]) follow only a
//! source whose curves come back as they were listed, as there. A
//! projection with a [`Projection::chain`] is one curve, its edges joined
//! ([`join_curves`]); one with [`Projection::section`] is where the
//! sketch's plane cuts its face ([`mitcad_model_base::Kernel::section_curves`]), or the
//! points where the plane crosses its edge ([`mitcad_model_base::Kernel::section_points`],
//! mitcad#736).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mitcad_vec3::distance;

use super::geometry::dot as dot2;
use super::geometry::{Curve2, P2, add, angle_after, dist, norm, scale, sub};
use super::solve::{SolveError, Solved};
use super::{Entity, EntityKind, Projection, Ref};
use crate::datum::DatumPlane;
use crate::def::SketchDef;
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::Curve3;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::topo::TopoName;
use crate::transform::Transform;
use mitcad_vec3::{cross, dot};

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
                    // Seen edge on: the extreme points of the curve, its
                    // ends or where it turns along the line (exactly:
                    // sampled ones fell up to 1e-3 short of an arc's top,
                    // mitcad#955).
                    let u = [rotation.cos(), rotation.sin()];
                    let along = |t: &f64| dot2(sub(at(*t), c), u);
                    let (t0, t1) = (start.min(*end), start.max(*end));
                    let turn = dot2(ay, u).atan2(dot2(ax, u));
                    let mut ts = vec![t0, t1];
                    let first = ((t0 - turn) / std::f64::consts::PI).ceil() as i64;
                    let last = ((t1 - turn) / std::f64::consts::PI).floor() as i64;
                    for k in first..=last.min(first + 8) {
                        ts.push(turn + k as f64 * std::f64::consts::PI);
                    }
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
                            start: 0.0,
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
                let spline = Curve2::Nurbs(super::geometry::Nurbs {
                    degree: *degree as usize,
                    control: poles.iter().map(|p| to_sketch(frame, *p)).collect(),
                    weights: weights.clone(),
                    knots: knots.clone(),
                });
                // Seen edge on (a planar curve whose plane holds the view
                // direction, mitcad#389): the line between its extreme
                // points, as a conic's.
                out.push(Projected::Curve(edge_on(&spline).unwrap_or(spline)));
            }
            // A helix seen along its axis is its circle (mitcad#385); seen
            // otherwise it is no curve a sketch holds, and is left out.
            Curve3::Helix {
                center,
                axis,
                x_axis,
                radius,
                start,
                end,
                ..
            } => {
                let off = cross(*axis, frame.normal());
                if dot(off, off).sqrt() <= 1e-9 {
                    let circle = Curve3::Conic {
                        center: *center,
                        normal: *axis,
                        x_axis: *x_axis,
                        major: *radius,
                        minor: *radius,
                        start: *start,
                        end: *end,
                        closed: end - start >= std::f64::consts::TAU - 1e-12,
                    };
                    out.extend(project(&[circle], frame));
                }
            }
        }
    }
    out
}

/// A projected spline whose control points lie on one line (within 1e-9
/// of its size): the line between the curve's extreme points along it.
fn edge_on(spline: &Curve2) -> Option<Curve2> {
    let Curve2::Nurbs(n) = spline else {
        return None;
    };
    let first = *n.control.first()?;
    let far = n
        .control
        .iter()
        .copied()
        .max_by(|a, b| dist(*a, first).total_cmp(&dist(*b, first)))?;
    let size = dist(far, first);
    if size <= 1e-12 {
        return None;
    }
    let u = scale(sub(far, first), 1.0 / size);
    let off = |p: P2| super::geometry::cross(sub(p, first), u).abs();
    if n.control.iter().any(|p| off(*p) > 1e-9 * size) {
        return None;
    }
    // The extreme parameters along the line: sampled, then narrowed.
    let along = |t: f64| dot2(sub(spline.point(t), first), u);
    let (t0, t1) = spline.domain();
    let ts = spline.sample_params(t0, t1);
    let step = (t1 - t0) / (ts.len().max(2) - 1) as f64;
    let extreme = |sign: f64| {
        let best = ts
            .iter()
            .copied()
            .max_by(|a, b| (sign * along(*a)).total_cmp(&(sign * along(*b))))
            .unwrap_or(t0);
        let (mut lo, mut hi) = ((best - step).max(t0), (best + step).min(t1));
        for _ in 0..100 {
            let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
            if sign * along(a) >= sign * along(b) {
                hi = b;
            } else {
                lo = a;
            }
        }
        let t = (lo + hi) / 2.0;
        // (An end is exact where it is the extreme.)
        [t0, t1, t]
            .into_iter()
            .max_by(|a, b| (sign * along(*a)).total_cmp(&(sign * along(*b))))
            .unwrap_or(t)
    };
    let (a, b) = (spline.point(extreme(-1.0)), spline.point(extreme(1.0)));
    (dist(a, b) > 1e-12).then_some(Curve2::Line { a, b })
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
            Curve2::Circle { center, radius, .. } => EntityKind::Circle {
                center: point(&mut out, *center, false),
                radius: *radius,
                start: 0.0,
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

/// The ids a new projection's entities take until [`join_existing`] has
/// decided which of them are added ([`renumber`] gives them their own):
/// above any a sketch gives out, so that renumbering never meets an id in
/// use.
pub const FRESH: u32 = 1 << 30;

/// A new projection's entities joined to the projected geometry already in
/// the sketch ([`join_existing`]).
#[derive(Debug, Default)]
pub struct Joined {
    /// The entities to add, with the ids [`entities`] gave them.
    pub entities: Vec<Entity>,
    /// The new curves left out as the same as one in the sketch, with that
    /// curve: (left out, in the sketch).
    pub same: Vec<(EntityUid, EntityUid)>,
}

/// The entities of the sketch a new projection may join
/// ([`join_existing`]): for a linked one (`linked` with its body and link)
/// those of the linked projections of the same body and link, with the
/// points their curves end on; for an unlinked one the projected entities
/// in no projection. A static curve never shares a point a linked one
/// moves, and a linked one only points that follow the same body. A linked
/// projection of another sketch's geometry (`sketch`, mitcad#904) joins
/// only those of the same sketch, one of model geometry only those of
/// model geometry.
pub fn joinable(
    def: &SketchDef,
    linked: Option<(Option<BodyUid>, Option<&crate::occurrence::OccurrenceLink>)>,
    sketch: Option<crate::ids::FeatureUid>,
) -> BTreeSet<EntityUid> {
    let in_any: BTreeSet<EntityUid> = def
        .projections
        .iter()
        .flat_map(|p| p.entities.iter().map(|r| r.uid()))
        .collect();
    let mut out: BTreeSet<EntityUid> = match linked {
        Some((body, link)) => def
            .projections
            .iter()
            .filter(|p| {
                p.body == body
                    && p.link.as_ref() == link
                    && p.source.sketch().map(|e| e.sketch) == sketch
            })
            .flat_map(|p| p.entities.iter().map(|r| r.uid()))
            .collect(),
        None => def
            .entities
            .iter()
            .filter(|e| e.reference && e.fixed && e.axis.is_none() && !in_any.contains(&e.id))
            .map(|e| e.id)
            .collect(),
    };
    // The points their curves use (a clean-up may have put a curve's end on
    // another projection's point).
    let points: Vec<EntityUid> = def
        .entities
        .iter()
        .filter(|e| out.contains(&e.id))
        .flat_map(|e| e.kind.points())
        .filter(|p| linked.is_some() || !in_any.contains(p))
        .collect();
    out.extend(points);
    out
}

/// A new projection's entities (`fresh`, made by [`entities`] with ids
/// above [`FRESH`]) joined to the sketch's projected geometry (mitcad#741):
///
/// 1. Ends and loose points of its own closer than the tolerance share a
///    point (a model's edges meet within its tolerance).
/// 2. A curve the same as one of `joinable` (the same edge projected
///    again, a face projected after one of its edges) is left out, and the
///    new curves that end on it end on that curve's points.
/// 3. An end or a loose point where an end or a loose point of `joinable`
///    is, is that point.
///
/// A join that would make three curves of the outline's kind end at one
/// point is not made (a face beside a face projected before keeps its own
/// line on the edge they share, so that each outline stays a chain for
/// Chain Selection). For a linked projection (`linked`), whose points
/// another projection may move elsewhere when the model changes, a point
/// of an arc is never shared with another projection: a line takes either
/// place and stays a line, an arc would no longer fit its centre. Centres
/// and inner control points stay the curve's own. The tolerance is half of
/// what following the projection allows ([`SAME`] of the size of its
/// geometry), so that a joined point never tears it.
pub fn join_existing(
    def: &SketchDef,
    joinable: &BTreeSet<EntityUid>,
    fresh: Vec<Entity>,
    linked: bool,
) -> Joined {
    let index = super::EntityIndex::new(&def.entities);
    let fresh_points: BTreeMap<EntityUid, P2> = fresh
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some((e.id, at)),
            _ => None,
        })
        .collect();
    let mut size = 1.0_f64;
    for p in fresh_points.values() {
        size = size.max(norm(*p));
    }
    let tol = 0.5 * SAME * size;
    let ends_of = |e: &Entity| -> Vec<EntityUid> {
        e.kind.ends().map(|(s, t)| vec![s, t]).unwrap_or_default()
    };
    let bound = |e: &Entity| {
        matches!(
            e.kind,
            EntityKind::Arc { .. } | EntityKind::EllipticalArc { .. }
        )
    };
    let fresh_used: BTreeSet<EntityUid> = fresh.iter().flat_map(|e| e.kind.points()).collect();
    let fresh_ends: BTreeSet<EntityUid> = fresh.iter().flat_map(ends_of).collect();
    let fresh_bound: BTreeSet<EntityUid> = fresh
        .iter()
        .filter(|e| bound(e))
        .flat_map(|e| e.kind.points())
        .collect();
    let old_used: BTreeSet<EntityUid> = def.entities.iter().flat_map(|e| e.kind.points()).collect();
    let old_bound: BTreeSet<EntityUid> = def
        .entities
        .iter()
        .filter(|e| bound(e))
        .flat_map(|e| e.kind.points())
        .collect();
    let old_ends: BTreeSet<EntityUid> = def
        .entities
        .iter()
        .filter(|e| joinable.contains(&e.id))
        .flat_map(ends_of)
        .collect();
    // How many of the sketch's curves of the outline's kind end at a point.
    let old_degree = |p: EntityUid| {
        def.entities
            .iter()
            .filter(|e| !e.is_point() && !e.construction && ends_of(e).contains(&p))
            .count()
    };
    // Whether a new point may be the sketch's point `old`.
    let shareable = |new: EntityUid, old: EntityUid| {
        !linked || !(fresh_bound.contains(&new) || old_bound.contains(&old))
    };
    // The other end of each new curve: never the same point.
    let mut partners: BTreeMap<EntityUid, Vec<EntityUid>> = BTreeMap::new();
    for e in &fresh {
        if let Some((s, t)) = e.kind.ends() {
            partners.entry(s).or_default().push(t);
            partners.entry(t).or_default().push(s);
        }
    }
    let joins = |e: &Entity| fresh_ends.contains(&e.id) || !fresh_used.contains(&e.id);
    // 1. Its own ends that meet.
    let mut map: BTreeMap<EntityUid, EntityUid> = BTreeMap::new();
    let mut anchors: Vec<(EntityUid, P2)> = Vec::new();
    for e in &fresh {
        let EntityKind::Point { at } = e.kind else {
            continue;
        };
        if !joins(e) {
            continue;
        }
        let others = partners.get(&e.id).cloned().unwrap_or_default();
        let found = anchors
            .iter()
            .find(|(id, p)| {
                dist(*p, at) <= tol
                    && !others
                        .iter()
                        .any(|q| q == id || map.get(q).is_some_and(|m| m == id))
            })
            .map(|(id, _)| *id);
        match found {
            Some(id) => {
                map.insert(e.id, id);
            }
            None => anchors.push((e.id, at)),
        }
    }
    // The new curves (not left out) that end at a point, as joined so far.
    let ending =
        |p: EntityUid, map: &BTreeMap<EntityUid, EntityUid>, dropped: &BTreeSet<EntityUid>| {
            let target = resolve(map, p);
            fresh
                .iter()
                .filter(|c| {
                    !c.is_point()
                        && !dropped.contains(&c.id)
                        && ends_of(c).iter().any(|q| resolve(map, *q) == target)
                })
                .collect::<Vec<&Entity>>()
        };
    // 2. Curves the same as one in the sketch.
    let fresh_at = |id: EntityUid| fresh_points.get(&id).copied();
    let old_at = |id: EntityUid| index.point(id);
    let mut out = Joined::default();
    let mut dropped: BTreeSet<EntityUid> = BTreeSet::new();
    for e in fresh.iter().filter(|e| !e.is_point()) {
        let twin = def
            .entities
            .iter()
            .filter(|x| !x.is_point() && !x.construction && joinable.contains(&x.id))
            .filter_map(|x| {
                unlikeness(&e.kind, &fresh_at, &x.kind, &old_at).map(|(c, back)| (x, c, back))
            })
            .filter(|(_, c, _)| *c <= 2.0 * tol)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((x, _, back)) = twin else {
            continue;
        };
        let (fp, xp) = (e.kind.points(), x.kind.points());
        let n = fp.len();
        let pairs: Vec<(EntityUid, EntityUid)> = fp
            .iter()
            .enumerate()
            .map(|(k, p)| (*p, if back { xp[n - 1 - k] } else { xp[k] }))
            .collect();
        // The other new curves would end on the sketch's points instead.
        let mut gone = dropped.clone();
        gone.insert(e.id);
        let fits = pairs.iter().all(|(p, o)| {
            let on = ending(*p, &map, &gone);
            on.is_empty()
                || (old_degree(*o) + on.len() <= 2
                    && (!linked || (!old_bound.contains(o) && !on.iter().any(|c| bound(c)))))
        });
        if !fits {
            continue;
        }
        for (p, o) in pairs {
            map.insert(p, o);
        }
        dropped.insert(e.id);
        out.same.push((e.id, x.id));
    }
    // 3. Ends and loose points on the sketch's.
    let old_anchors: Vec<(EntityUid, P2)> = def
        .entities
        .iter()
        .filter(|e| {
            joinable.contains(&e.id) && (old_ends.contains(&e.id) || !old_used.contains(&e.id))
        })
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some((e.id, at)),
            _ => None,
        })
        .collect();
    for e in &fresh {
        let EntityKind::Point { at } = e.kind else {
            continue;
        };
        if !joins(e) || map.contains_key(&e.id) {
            continue;
        }
        let degree = ending(e.id, &map, &dropped).len();
        let others = partners.get(&e.id).cloned().unwrap_or_default();
        let found = old_anchors
            .iter()
            .find(|(id, p)| {
                dist(*p, at) <= tol
                    && shareable(e.id, *id)
                    && old_degree(*id) + degree <= 2
                    && !others.iter().any(|q| resolve(&map, *q) == *id)
            })
            .map(|(id, _)| *id);
        if let Some(id) = found {
            map.insert(e.id, id);
        }
    }
    for mut e in fresh {
        if dropped.contains(&e.id) || map.contains_key(&e.id) {
            continue;
        }
        for p in e.kind.points() {
            let to = resolve(&map, p);
            if to != p {
                e.kind.replace_point(p, to);
            }
        }
        out.entities.push(e);
    }
    // Points only left-out curves used.
    let used: BTreeSet<EntityUid> = out.entities.iter().flat_map(|e| e.kind.points()).collect();
    out.entities
        .retain(|e| !e.is_point() || used.contains(&e.id) || !fresh_used.contains(&e.id));
    out
}

/// A point joined to a point that joined another in turn: the last one.
fn resolve(map: &BTreeMap<EntityUid, EntityUid>, id: EntityUid) -> EntityUid {
    let mut id = id;
    while let Some(next) = map.get(&id) {
        id = *next;
    }
    id
}

/// Gives entities [`join_existing`] kept (with ids above [`FRESH`]) the
/// ids `ids`, in the order their ids were given out, so that they are
/// numbered as [`entities`] numbers them.
pub fn renumber(entities: &mut [Entity], ids: &[EntityUid]) {
    let mut order: Vec<EntityUid> = entities.iter().map(|e| e.id).collect();
    order.sort_unstable();
    let map: BTreeMap<EntityUid, EntityUid> = order.into_iter().zip(ids.iter().copied()).collect();
    for e in entities.iter_mut() {
        if let Some(id) = map.get(&e.id) {
            e.id = *id;
        }
        for p in e.kind.points() {
            if let Some(to) = map.get(&p) {
                e.kind.replace_point(p, *to);
            }
        }
    }
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

/// The curves one linked projection follows (mitcad#736): its source and
/// the faces its curve continues into, or none for curves that come in
/// unlinked (a section's pieces on faces without names). A curve or point
/// of another sketch is a source of its own (mitcad#904).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceCurves {
    pub source: Option<super::ProjectionSource>,
    pub chain: Vec<TopoName>,
    pub curves: Vec<Curve3>,
}

/// A whole body's section (Intersect of a body, mitcad#736) as the
/// sections of its faces: the pieces on one face are one projection of
/// that face's section, a B-spline joined across faces one of the first
/// face's section with the others as its chain (as a face's own section
/// joins it, [`join_curves`]), and pieces on faces without names one
/// group without a source. In the order the kernel lists the pieces.
pub fn section_groups(found: Vec<crate::kernel::SectionCurve>) -> Vec<SourceCurves> {
    let mut groups: Vec<SourceCurves> = Vec::new();
    for piece in found {
        let faces: Vec<TopoName> = piece.faces.into_iter().map(TopoName::Face).collect();
        let joined = matches!(piece.curve, Curve3::BSpline { .. }) && faces.len() > 1;
        let (source, chain): (Option<super::ProjectionSource>, Vec<TopoName>) =
            match faces.split_first() {
                Some((first, rest)) if joined => (Some(first.clone().into()), rest.to_vec()),
                Some((first, _)) => (Some(first.clone().into()), Vec::new()),
                None => (None, Vec::new()),
            };
        match groups
            .iter_mut()
            .find(|g| !joined && g.chain.is_empty() && g.source == source)
        {
            Some(group) => group.curves.push(piece.curve),
            None => groups.push(SourceCurves {
                source,
                chain,
                curves: vec![piece.curve],
            }),
        }
    }
    groups
}

/// How far apart (mm) the ends of two pieces of a chain may be and still
/// join ([`join_curves`]): the model's edges meet within their tolerance.
const JOIN: f64 = 1e-4;

/// A clamped, non-rational B-spline: degree, poles, knots.
type Spline = (u32, Vec<[f64; 3]>, Vec<f64>);

/// A piece as a clamped, non-rational B-spline of `degree` (a line raised
/// to it exactly); None for conics, rational and unclamped splines and
/// other degrees.
fn as_spline(curve: &Curve3, degree: u32) -> Option<Spline> {
    match curve {
        Curve3::Line { start, end } => {
            let p = degree.max(1) as usize;
            let poles = (0..=p)
                .map(|i| {
                    let t = i as f64 / p as f64;
                    std::array::from_fn(|k| start[k] + t * (end[k] - start[k]))
                })
                .collect();
            let knots = std::iter::repeat_n(0.0, p + 1)
                .chain(std::iter::repeat_n(1.0, p + 1))
                .collect();
            Some((degree.max(1), poles, knots))
        }
        Curve3::BSpline {
            degree: d,
            poles,
            weights,
            knots,
        } if *d == degree => {
            let p = degree as usize;
            let rational = weights.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-12);
            let clamped = knots.len() == poles.len() + p + 1
                && knots[..=p].iter().all(|k| *k == knots[0])
                && knots[knots.len() - p - 1..]
                    .iter()
                    .all(|k| *k == knots[knots.len() - 1]);
            (!rational && clamped).then(|| (degree, poles.clone(), knots.clone()))
        }
        _ => None,
    }
}

fn reversed((degree, poles, knots): Spline) -> Spline {
    let (first, last) = (knots[0], knots[knots.len() - 1]);
    (
        degree,
        poles.into_iter().rev().collect(),
        knots.iter().rev().map(|k| first + last - k).collect(),
    )
}

/// Curves joined end to end into one B-spline, exactly (continuous in
/// position where they meet; mitcad#389), in the order given, each turned
/// to start where the one before ends. Lines join B-splines of the degree
/// of the first B-spline; None when the pieces do not meet within
/// [`JOIN`], or one is a conic, rational or unclamped, or of another
/// degree. One curve is itself.
pub fn join_curves(curves: &[Curve3]) -> Option<Curve3> {
    if curves.len() == 1 {
        return Some(curves[0].clone());
    }
    let degree = curves
        .iter()
        .find_map(|c| match c {
            Curve3::BSpline { degree, .. } => Some(*degree),
            _ => None,
        })
        .unwrap_or(1);
    let pieces: Vec<Spline> = curves
        .iter()
        .map(|c| as_spline(c, degree))
        .collect::<Option<_>>()?;
    let ends = |s: &Spline| (s.1[0], s.1[s.1.len() - 1]);
    // The first piece runs towards the second.
    let mut joined = pieces[0].clone();
    let (a0, a1) = ends(&joined);
    let (b0, b1) = ends(&pieces[1]);
    if distance(a1, b0).min(distance(a1, b1)) > distance(a0, b0).min(distance(a0, b1)) {
        joined = reversed(joined);
    }
    let p = degree as usize;
    for piece in &pieces[1..] {
        let end = joined.1[joined.1.len() - 1];
        let (s0, s1) = ends(piece);
        let piece = if distance(end, s0) <= distance(end, s1) {
            piece.clone()
        } else {
            reversed(piece.clone())
        };
        if distance(end, piece.1[0]) > JOIN {
            return None;
        }
        let at = joined.2[joined.2.len() - 1];
        let shift = at - piece.2[0];
        // The joint is a knot of multiplicity p: drop one of the
        // joined end knots (p + 1 each) from each side.
        let mut knots: Vec<f64> = joined.2[..joined.2.len() - 1].to_vec();
        knots.extend(piece.2[p + 1..].iter().map(|k| k + shift));
        let mut poles = joined.1.clone();
        poles.extend_from_slice(&piece.1[1..]);
        joined = (degree, poles, knots);
    }
    Some(Curve3::BSpline {
        degree,
        poles: joined.1,
        weights: Vec::new(),
        knots: joined.2,
    })
}

/// A plane moved by a rigid transform.
pub fn section_plane(t: &Transform, plane: &DatumPlane) -> DatumPlane {
    DatumPlane {
        origin: t.apply_point(plane.origin),
        x_axis: t.apply_vector(plane.x_axis),
        y_axis: t.apply_vector(plane.y_axis),
    }
}

/// A model curve moved by a rigid transform.
pub fn transform_curve(t: &Transform, curve: &Curve3) -> Curve3 {
    match curve {
        Curve3::Point(p) => Curve3::Point(t.apply_point(*p)),
        Curve3::Line { start, end } => Curve3::Line {
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => Curve3::Conic {
            center: t.apply_point(*center),
            normal: t.apply_vector(*normal),
            x_axis: t.apply_vector(*x_axis),
            major: *major,
            minor: *minor,
            start: *start,
            end: *end,
            closed: *closed,
        },
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => Curve3::BSpline {
            degree: *degree,
            poles: poles.iter().map(|p| t.apply_point(*p)).collect(),
            weights: weights.clone(),
            knots: knots.clone(),
        },
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => Curve3::Helix {
            center: t.apply_point(*center),
            axis: t.apply_vector(*axis),
            x_axis: t.apply_vector(*x_axis),
            radius: *radius,
            pitch: *pitch,
            start: *start,
            end: *end,
        },
    }
}

/// How far from a plane (mm) a point of a curve may be and still lie on
/// it, for [`plane_crossings`].
const ON_PLANE: f64 = 1e-7;

/// Where a model curve crosses the plane through `origin` with the unit
/// `normal` (mitcad#998: the section of another sketch's curve, as
/// `Kernel::section_points` gives an edge's): a point where it passes
/// through or touches the plane, in the curve's order; a curve lying in
/// the plane gives its ends (none for a closed one); a point on the plane
/// itself. Lines and conics exactly; splines and helices by their sign
/// changes refined by bisection (a touch between samples is missed).
pub fn plane_crossings(curve: &Curve3, origin: [f64; 3], normal: [f64; 3]) -> Vec<[f64; 3]> {
    let side = |p: [f64; 3]| dot(mitcad_vec3::sub(p, origin), normal);
    let on = |d: f64| d.abs() <= ON_PLANE;
    let mut out: Vec<[f64; 3]> = Vec::new();
    match curve {
        Curve3::Point(p) => {
            if on(side(*p)) {
                out.push(*p);
            }
        }
        Curve3::Line { start, end } => {
            let (a, b) = (side(*start), side(*end));
            if on(a) {
                out.push(*start);
            }
            if !on(a) && !on(b) && (a < 0.0) != (b < 0.0) {
                out.push(mitcad_vec3::add_scaled(
                    *start,
                    mitcad_vec3::sub(*end, *start),
                    a / (a - b),
                ));
            }
            if on(b) {
                out.push(*end);
            }
        }
        Curve3::Conic {
            center,
            normal: axis,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            // side(t) = c + p cos t + q sin t = c + r cos(t − φ).
            let y_axis = cross(*axis, *x_axis);
            let c = side(*center);
            let p = major * dot(*x_axis, normal);
            let q = minor * dot(y_axis, normal);
            let r = p.hypot(q);
            let at = |t: f64| {
                mitcad_model_base::curves::conic_point(*center, *axis, *x_axis, (*major, *minor), t)
            };
            let tau = std::f64::consts::TAU;
            if r <= ON_PLANE {
                // Parallel to the plane: lying in it, its ends.
                if on(c) && !closed {
                    out.push(at(*start));
                    out.push(at(*end));
                }
            } else if c.abs() <= r + ON_PLANE {
                let phi = q.atan2(p);
                let a = (-c / r).clamp(-1.0, 1.0).acos();
                let mut ts: Vec<f64> = Vec::new();
                for t in [phi + a, phi - a] {
                    // Its parameter from the start, within one turn.
                    let from = (t - start).rem_euclid(tau);
                    // At the start, from below.
                    let from = if tau - from <= 1e-9 { 0.0 } else { from };
                    if *closed || from <= end - start + 1e-9 {
                        ts.push(start + from);
                    }
                }
                ts.sort_by(f64::total_cmp);
                out.extend(ts.into_iter().map(at));
            }
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let d = *degree as usize;
            if poles.len() <= d || knots.len() != poles.len() + d + 1 {
                return Vec::new();
            }
            let at = |t: f64| mitcad_model_base::curves::bspline_point(d, poles, weights, knots, t);
            // Samples: each knot span cut in 16.
            let (t0, t1) = (knots[d], knots[poles.len()]);
            let mut ts: Vec<f64> = Vec::new();
            for w in knots[d..=poles.len()].windows(2) {
                if w[1] > w[0] {
                    ts.extend((0..16).map(|k| w[0] + (w[1] - w[0]) * f64::from(k) / 16.0));
                }
            }
            ts.push(t1);
            if t1 > t0 {
                out = sampled_crossings(&ts, &at, &side);
            }
        }
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => {
            let at = |t: f64| {
                mitcad_model_base::curves::helix_point(
                    *center,
                    *axis,
                    *x_axis,
                    (*radius, *pitch),
                    t,
                )
            };
            // Samples: 64 a turn.
            let steps = (((end - start) / std::f64::consts::TAU) * 64.0)
                .ceil()
                .max(1.0);
            let n = steps.min(1e6) as usize;
            let ts: Vec<f64> = (0..=n)
                .map(|k| start + (end - start) * k as f64 / n as f64)
                .collect();
            out = sampled_crossings(&ts, &at, &side);
        }
    }
    // A crossing at a point found twice (a tangent, an end) once.
    let mut kept: Vec<[f64; 3]> = Vec::new();
    for p in out {
        if !kept.iter().any(|k| distance(*k, p) <= ON_PLANE) {
            kept.push(p);
        }
    }
    kept
}

/// The crossings of a curve with a plane from samples of its parameter
/// (`ts`, rising): samples on the plane, and a point found by bisection
/// between two samples on either side of it. A curve whose samples all lie
/// on the plane gives its ends.
fn sampled_crossings(
    ts: &[f64],
    at: &dyn Fn(f64) -> [f64; 3],
    side: &dyn Fn([f64; 3]) -> f64,
) -> Vec<[f64; 3]> {
    let values: Vec<f64> = ts.iter().map(|t| side(at(*t))).collect();
    let on = |d: f64| d.abs() <= ON_PLANE;
    if values.iter().all(|d| on(*d)) {
        return match (ts.first(), ts.last()) {
            (Some(a), Some(b)) => vec![at(*a), at(*b)],
            _ => Vec::new(),
        };
    }
    let mut out = Vec::new();
    for (i, t) in ts.iter().enumerate() {
        if on(values[i]) {
            out.push(at(*t));
            continue;
        }
        let Some(&next) = values.get(i + 1) else {
            continue;
        };
        if on(next) || (values[i] < 0.0) == (next < 0.0) {
            continue;
        }
        let (mut lo, mut hi) = (*t, ts[i + 1]);
        let low_side = values[i] < 0.0;
        for _ in 0..100 {
            let mid = 0.5 * (lo + hi);
            if (side(at(mid)) < 0.0) == low_side {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        out.push(at(0.5 * (lo + hi)));
    }
    out
}

/// How a linked projection followed its source at an evaluation, when
/// anything of it did not (mitcad#232).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectionFollow {
    /// The projection's index in the definition's `projections`.
    pub projection: usize,
    /// Its curves and loose points that found nothing like them in the
    /// source: they keep their last geometry.
    pub unfollowed: Vec<EntityUid>,
    /// The source's curves and points no entity of the projection follows
    /// (a projection of some of a face's edges, an edge the face gained).
    pub unmatched: usize,
    /// The source has no curves (it is gone): everything kept.
    pub lost: bool,
    /// Its curves no longer match the source's (mitcad#249): following by
    /// geometry would tear its arcs from the curves they share points with,
    /// or a projection that follows by position finds the source's curves
    /// changed in kind, order or joints. Everything kept.
    pub kept: bool,
    /// Its curves that, where they are now, match none of the source's
    /// current curves (all of them for a lost source): stale curves Clean
    /// Up Stale Projections tidies (mitcad#252).
    pub stale: Vec<EntityUid>,
}

impl ProjectionFollow {
    /// The evaluation's warning about it; None when its entities all
    /// followed (source curves nothing follows are no warning: a
    /// projection may hold only some of its source's curves).
    pub fn warning(&self, projection: &Projection) -> Option<String> {
        if self.lost {
            return Some(format!(
                "the projection of {} does not follow it: it is gone, and its entities keep \
                 their last geometry",
                projection.source
            ));
        }
        if self.kept {
            return Some(format!(
                "the projection of {} does not follow it: its curves no longer match the \
                 source's, and they keep their last geometry",
                projection.source
            ));
        }
        if self.unfollowed.is_empty() {
            return None;
        }
        let names: Vec<String> = self
            .unfollowed
            .iter()
            .map(|id| entity_name(projection, *id))
            .collect();
        let one = names.len() == 1;
        Some(format!(
            "the projection of {} does not follow it with {}: nothing like {} is in it now, \
             so {} {} last geometry",
            projection.source,
            names.join(", "),
            if one { "it" } else { "them" },
            if one { "it keeps" } else { "they keep" },
            if one { "its" } else { "their" },
        ))
    }
}

/// `p7` or `c9`, as the projection lists it.
fn entity_name(projection: &Projection, id: EntityUid) -> String {
    projection
        .entities
        .iter()
        .find(|r| r.uid() == id)
        .map_or_else(|| format!("c{}", id.0), ToString::to_string)
}

/// What [`follow_links`] did.
#[derive(Debug, Clone, Default)]
pub struct Followed {
    /// The definition with linked projections moved to their sources'
    /// current geometry, or None when nothing moved.
    pub def: Option<SketchDef>,
    /// The projections that did not follow all the way.
    pub reports: Vec<ProjectionFollow>,
}

/// The definition with linked projections moved to their sources' current
/// geometry (None when there are none or nothing moved), and what of them
/// could not follow. Each entity follows the fresh curve of its kind most
/// like it ([`pair`]); one that finds none keeps its last geometry.
pub fn follow_links(
    def: &SketchDef,
    frame: &SketchFrame,
    source_curves: &mut dyn FnMut(&Projection) -> Result<Vec<Curve3>, String>,
) -> Result<Followed, String> {
    let mut out = Followed::default();
    if def.projections.is_empty() {
        return Ok(out);
    }
    let mut updated = def.clone();
    let mut changed = false;
    let index = super::EntityIndex::new(&def.entities);
    let old_at = |id: EntityUid| index.point(id);
    // Where each entity is in the list (the first of an id, as
    // `SketchDef::entity` finds it): `updated` keeps the order. Looked up
    // instead of searched for each projection's entities (mitcad#889: a
    // sketch of 5 000 projections searched its 10 000 entities 30 000
    // times).
    let at = positions(&def.entities);
    // Fresh ids above the sketch's: the old ids replace them one at a
    // time, so no old id may also be a fresh one still to replace.
    let top = def.entities.iter().map(|e| e.id.0).max().unwrap_or(0);
    for (k, projection) in def.projections.iter().enumerate() {
        let curves = source_curves(projection)?;
        if curves.is_empty() {
            // A lost projection keeps its last geometry.
            out.reports.push(ProjectionFollow {
                projection: k,
                lost: true,
                stale: linked_entities_at(def, &at, projection)
                    .iter()
                    .filter(|e| !e.is_point())
                    .map(|e| e.id)
                    .collect(),
                ..ProjectionFollow::default()
            });
            continue;
        }
        let mut counter = top;
        let mut fresh_id = || {
            counter += 1;
            EntityUid(counter)
        };
        let old: Vec<Entity> = linked_entities_at(def, &at, projection);
        if !projection.by_geometry {
            // A projection of an earlier version or an import follows as
            // there: only a source whose curves come back as they were
            // listed, of the same kinds and joined the same way, entity by
            // entity (mitcad#249).
            let fresh = entities(&project(&curves, frame), &mut fresh_id);
            if shape_of(&fresh) != shape_of(&old) {
                out.reports.push(ProjectionFollow {
                    projection: k,
                    kept: true,
                    stale: stale_curves(&updated, projection, &fresh),
                    ..ProjectionFollow::default()
                });
                continue;
            }
            for (new, old) in fresh.iter().zip(&old) {
                if let Some(target) = at.get(&old.id).map(|&i| &mut updated.entities[i]) {
                    let mut kind = new.kind.clone();
                    // Keep the old point ids.
                    for (from, to) in new.kind.points().iter().zip(old.kind.points()) {
                        kind.replace_point(*from, to);
                    }
                    // Where a circle starts is no geometry of its source.
                    if let (
                        EntityKind::Circle { start, .. },
                        EntityKind::Circle {
                            start: old_start, ..
                        },
                    ) = (&mut kind, &old.kind)
                    {
                        *start = *old_start;
                    }
                    if target.kind != kind {
                        target.kind = kind;
                        changed = true;
                    }
                }
            }
            continue;
        }
        let fresh = entities(&distinct(project(&curves, frame)), &mut fresh_id);
        let pairing = pair(&old, &old_at, &fresh);
        if pairing.kept {
            // A pairing that tears the projection's joints moves nothing
            // (mitcad#249).
            out.reports.push(ProjectionFollow {
                projection: k,
                kept: true,
                stale: stale_curves(&updated, projection, &fresh),
                ..ProjectionFollow::default()
            });
            continue;
        }
        for (id, kind) in pairing.kinds {
            if let Some(target) = at.get(&id).map(|&i| &mut updated.entities[i])
                && target.kind != kind
            {
                target.kind = kind;
                changed = true;
            }
        }
        if !pairing.unfollowed.is_empty() || pairing.unmatched > 0 {
            // Where its entities are now: those that followed match.
            let stale = if pairing.unfollowed.is_empty() {
                Vec::new()
            } else {
                stale_curves(&updated, projection, &fresh)
            };
            out.reports.push(ProjectionFollow {
                projection: k,
                unfollowed: pairing.unfollowed,
                unmatched: pairing.unmatched,
                stale,
                ..ProjectionFollow::default()
            });
        }
    }
    out.def = changed.then_some(updated);
    Ok(out)
}

/// How far apart, relative to the size of the geometry, two curves or
/// points may be and still be the same.
pub(crate) const SAME: f64 = 1e-7;

/// How far apart, relative to the size of the geometry, a projected curve
/// and a curve of its source may be and still match: looser than [`SAME`],
/// as an imported projection's numbers come from the file.
const MATCH: f64 = 1e-6;

/// The curves of a projection that match none of `fresh` (entities made
/// from its source's current curves), where `def` has them (mitcad#252).
fn stale_curves(def: &SketchDef, projection: &Projection, fresh: &[Entity]) -> Vec<EntityUid> {
    let index = super::EntityIndex::new(&def.entities);
    let at = |id: EntityUid| index.point(id);
    let fresh_points: BTreeMap<EntityUid, P2> = fresh
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some((e.id, at)),
            _ => None,
        })
        .collect();
    let fresh_at = |id: EntityUid| fresh_points.get(&id).copied();
    let old = linked_entities(def, projection);
    let mut size = 1.0_f64;
    for p in fresh_points.values() {
        size = size.max(norm(*p));
    }
    for e in &old {
        if let EntityKind::Point { at } = e.kind {
            size = size.max(norm(at));
        }
    }
    let tol = MATCH * size;
    old.iter()
        .filter(|e| !e.is_point())
        .filter(|e| {
            !fresh.iter().any(|f| {
                !f.is_point()
                    && unlikeness(&e.kind, &at, &f.kind, &fresh_at).is_some_and(|(c, _)| c <= tol)
            })
        })
        .map(|e| e.id)
        .collect()
}

/// The projected primitives without repeats: a curve or point the same as
/// an earlier one (a seam edge a face lists twice, edges seen edge on that
/// project onto one line) is left out (mitcad#232).
pub fn distinct(projected: Vec<Projected>) -> Vec<Projected> {
    let mut size = 1.0_f64;
    for p in &projected {
        if let Projected::Curve(c) = p {
            let (lo, hi) = c.bounds();
            size = size.max(norm(lo)).max(norm(hi));
        }
    }
    let tol = SAME * size;
    let mut out: Vec<Projected> = Vec::new();
    for p in projected {
        if !out.iter().any(|o| same_projected(o, &p, tol)) {
            out.push(p);
        }
    }
    out
}

fn same_projected(a: &Projected, b: &Projected, tol: f64) -> bool {
    match (a, b) {
        (Projected::Point(p), Projected::Point(q)) => dist(*p, *q) <= tol,
        (Projected::Curve(a), Projected::Curve(b)) => same_curve(a, b, tol),
        _ => false,
    }
}

/// Whether two curves are the same within `tol`: of one kind, on the same
/// place, a line or a spline either way round (a curve projected twice,
/// mitcad#232 and mitcad#741).
#[doc(hidden)]
pub fn same_curve(a: &Curve2, b: &Curve2, tol: f64) -> bool {
    let near = |p: P2, q: P2| dist(p, q) <= tol;
    let close = |x: f64, y: f64| (x - y).abs() <= tol;
    let ends = |a: &Curve2, b: &Curve2| {
        a.ends()
            .zip(b.ends())
            .is_some_and(|(x, y)| near(x.0, y.0) && near(x.1, y.1))
    };
    let middle = |c: &Curve2| {
        let (t0, t1) = c.domain();
        c.point((t0 + t1) / 2.0)
    };
    match (a, b) {
        (Curve2::Line { a: a0, b: a1 }, Curve2::Line { a: b0, b: b1 }) => {
            (near(*a0, *b0) && near(*a1, *b1)) || (near(*a0, *b1) && near(*a1, *b0))
        }
        (
            Curve2::Circle {
                center: c0,
                radius: r0,
                ..
            },
            Curve2::Circle {
                center: c1,
                radius: r1,
                ..
            },
        ) => near(*c0, *c1) && close(*r0, *r1),
        (Curve2::Arc { .. }, Curve2::Arc { .. })
        | (Curve2::EllipticalArc { .. }, Curve2::EllipticalArc { .. }) => {
            ends(a, b) && near(middle(a), middle(b))
        }
        (
            Curve2::Ellipse {
                center: c0,
                major: m0,
                minor: n0,
                rotation: t0,
            },
            Curve2::Ellipse {
                center: c1,
                major: m1,
                minor: n1,
                rotation: t1,
            },
        ) => {
            // The major axis either way round.
            let turn = (t0 - t1).rem_euclid(std::f64::consts::PI);
            let turn = turn.min(std::f64::consts::PI - turn);
            near(*c0, *c1) && close(*m0, *m1) && close(*n0, *n1) && turn * m0.max(1.0) <= tol
        }
        (Curve2::Nurbs(x), Curve2::Nurbs(y)) => {
            x.degree == y.degree
                && x.control.len() == y.control.len()
                && x.weights.len() == y.weights.len()
                && x.knots.len() == y.knots.len()
                && (x.control.iter().zip(&y.control).all(|(p, q)| near(*p, *q))
                    || x.control
                        .iter()
                        .zip(y.control.iter().rev())
                        .all(|(p, q)| near(*p, *q)))
        }
        _ => false,
    }
}

/// An entity's geometry to compare with another of its kind: its points'
/// positions and its sizes; None for a kind projections do not make or a
/// point that is missing.
fn anchors(kind: &EntityKind, at: &dyn Fn(EntityUid) -> Option<P2>) -> Option<(Vec<P2>, Vec<f64>)> {
    let points = |ids: &[EntityUid]| ids.iter().map(|id| at(*id)).collect::<Option<Vec<P2>>>();
    Some(match kind {
        EntityKind::Point { at } => (vec![*at], Vec::new()),
        EntityKind::Circle { center, radius, .. } => (points(&[*center])?, vec![*radius]),
        EntityKind::Ellipse { minor_radius, .. }
        | EntityKind::EllipticalArc { minor_radius, .. } => {
            (points(&kind.points())?, vec![*minor_radius])
        }
        EntityKind::Line { .. } | EntityKind::Arc { .. } => (points(&kind.points())?, Vec::new()),
        EntityKind::Spline {
            degree,
            control,
            weights,
            knots,
        } => {
            let mut sizes = vec![f64::from(*degree), knots.len() as f64];
            sizes.extend(weights);
            (points(control)?, sizes)
        }
        EntityKind::FittedSpline { .. } => return None,
    })
}

/// How unlike two entities' geometry is (the distances of their points
/// plus the differences of their sizes), and whether the second runs the
/// other way round (lines and splines); None when they are not of one kind
/// and form.
pub(crate) fn unlikeness(
    a: &EntityKind,
    a_at: &dyn Fn(EntityUid) -> Option<P2>,
    b: &EntityKind,
    b_at: &dyn Fn(EntityUid) -> Option<P2>,
) -> Option<(f64, bool)> {
    if a.type_name() != b.type_name() {
        return None;
    }
    let (pa, sa) = anchors(a, a_at)?;
    let (pb, sb) = anchors(b, b_at)?;
    if pa.len() != pb.len() || sa.len() != sb.len() {
        return None;
    }
    let sizes: f64 = sa.iter().zip(&sb).map(|(x, y)| (x - y).abs()).sum();
    let ahead: f64 = pa.iter().zip(&pb).map(|(p, q)| dist(*p, *q)).sum();
    if matches!(a, EntityKind::Line { .. } | EntityKind::Spline { .. }) {
        let back: f64 = pa
            .iter()
            .zip(pb.iter().rev())
            .map(|(p, q)| dist(*p, *q))
            .sum();
        if back < ahead {
            return Some((back + sizes, true));
        }
    }
    Some((ahead + sizes, false))
}

/// How a projection's entities follow fresh ones made from its source.
#[derive(Debug, Default)]
struct Pairing {
    /// The new kinds of the old entities that follow, points included.
    kinds: Vec<(EntityUid, EntityKind)>,
    unfollowed: Vec<EntityUid>,
    unmatched: usize,
    /// The pairs would tear points the projection's curves share: nothing
    /// follows (mitcad#249).
    kept: bool,
}

/// A pair of an old and a fresh entity (indices), how unlike they are and
/// whether the fresh one runs the other way round.
type Pair = (usize, usize, f64, bool);

/// Pairs a projection's entities (`old`, their points' positions from
/// `old_at`) with `fresh` ones made from its source's current curves: each
/// with the fresh one of its kind most like it, in the order of the lists
/// when that is as good (the kernel lists a source's curves the same way
/// each time) and nearest first otherwise, so that a projection of some of
/// a source's curves, or of curves listed in another order or from another
/// start, follows. An entity the same as another of the projection (a
/// curve projected twice) follows the same fresh one.
fn pair(old: &[Entity], old_at: &dyn Fn(EntityUid) -> Option<P2>, fresh: &[Entity]) -> Pairing {
    let fresh_points: BTreeMap<EntityUid, P2> = fresh
        .iter()
        .filter_map(|e| match e.kind {
            EntityKind::Point { at } => Some((e.id, at)),
            _ => None,
        })
        .collect();
    let fresh_at = |id: EntityUid| fresh_points.get(&id).copied();
    // Curves, then the points no curve of the list uses.
    let items = |list: &[Entity]| -> Vec<Entity> {
        let used: std::collections::BTreeSet<EntityUid> =
            list.iter().flat_map(|e| e.kind.points()).collect();
        let curves = list.iter().filter(|e| !e.is_point());
        let loose = list
            .iter()
            .filter(|e| e.is_point() && !used.contains(&e.id));
        curves.chain(loose).cloned().collect()
    };
    let (olds, news) = (items(old), items(fresh));
    let cost = |i: usize, j: usize| unlikeness(&olds[i].kind, old_at, &news[j].kind, &fresh_at);
    // In the order of the lists.
    let in_order: Option<Vec<Pair>> = (olds.len() == news.len()
        && shape_of(old) == shape_of(fresh))
    .then(|| {
        (0..olds.len())
            .map(|i| cost(i, i).map(|(c, back)| (i, i, c, back)))
            .collect::<Option<Vec<_>>>()
    })
    .flatten();
    // Nearest first.
    let mut candidates: Vec<Pair> = Vec::new();
    for i in 0..olds.len() {
        for j in 0..news.len() {
            if let Some((c, back)) = cost(i, j) {
                candidates.push((i, j, c, back));
            }
        }
    }
    candidates.sort_by(|a, b| a.2.total_cmp(&b.2));
    let (mut old_taken, mut new_taken) = (vec![false; olds.len()], vec![false; news.len()]);
    let mut nearest: Vec<Pair> = Vec::new();
    for (i, j, c, back) in candidates {
        if !old_taken[i] && !new_taken[j] {
            old_taken[i] = true;
            new_taken[j] = true;
            nearest.push((i, j, c, back));
        }
    }
    let total = |pairs: &[Pair]| pairs.iter().map(|p| p.2).sum::<f64>();
    let mut pairs = match in_order {
        Some(in_order)
            if in_order.len() > nearest.len()
                || total(&in_order) <= total(&nearest) * (1.0 + 1e-9) + 1e-12 =>
        {
            in_order
        }
        _ => nearest,
    };
    // What counts as the same, by the size of the projection's geometry.
    let mut size = 1.0_f64;
    for e in &olds {
        for p in anchors(&e.kind, old_at).map(|a| a.0).unwrap_or_default() {
            size = size.max(norm(p));
        }
    }
    // Repeats of a paired entity follow its fresh one.
    let paired: Vec<(usize, usize)> = pairs.iter().map(|p| (p.0, p.1)).collect();
    for i in 0..olds.len() {
        if paired.iter().any(|p| p.0 == i) {
            continue;
        }
        let twin = paired.iter().find(|(k, _)| {
            unlikeness(&olds[i].kind, old_at, &olds[*k].kind, old_at)
                .is_some_and(|(c, _)| c <= SAME * size)
        });
        if let Some(&(_, j)) = twin
            && let Some((c, back)) = cost(i, j)
        {
            pairs.push((i, j, c, back));
        }
    }
    let mut out = Pairing::default();
    let mut moved: BTreeMap<EntityUid, P2> = BTreeMap::new();
    // A point of an arc the pairs would put in two places tears the
    // projection's joints: the arc's centre and ends would no longer fit
    // (mitcad#249). Lines take either place and stay lines.
    let tol = SAME * size;
    let bound = |kind: &EntityKind| {
        matches!(
            kind,
            EntityKind::Arc { .. } | EntityKind::EllipticalArc { .. }
        )
    };
    let arc_points: std::collections::BTreeSet<EntityUid> = olds
        .iter()
        .filter(|e| bound(&e.kind))
        .flat_map(|e| e.kind.points())
        .collect();
    let mut torn = false;
    for &(i, j, _, back) in &pairs {
        let (o, f) = (&olds[i], &news[j]);
        if let (EntityKind::Point { .. }, EntityKind::Point { at }) = (&o.kind, &f.kind) {
            moved.entry(o.id).or_insert(*at);
            continue;
        }
        let (op, fp) = (o.kind.points(), f.kind.points());
        let n = op.len();
        for (k, id) in op.iter().enumerate() {
            let from = if back { fp[n - 1 - k] } else { fp[k] };
            if let Some(at) = fresh_at(from) {
                let to = *moved.entry(*id).or_insert(at);
                torn |= arc_points.contains(id) && dist(to, at) > tol;
            }
        }
        let kind = match (&o.kind, &f.kind) {
            // A line is its points (and the user's centre line flag).
            (EntityKind::Line { .. }, _) => o.kind.clone(),
            (
                EntityKind::Spline { .. },
                EntityKind::Spline {
                    degree,
                    weights,
                    knots,
                    ..
                },
            ) if back => {
                let (lo, hi) = (
                    knots.first().copied().unwrap_or(0.0),
                    knots.last().copied().unwrap_or(1.0),
                );
                EntityKind::Spline {
                    degree: *degree,
                    control: op.clone(),
                    weights: weights.iter().rev().copied().collect(),
                    knots: knots.iter().rev().map(|k| lo + hi - k).collect(),
                }
            }
            _ => {
                let mut kind = f.kind.clone();
                // Keep the old point ids.
                for (from, to) in fp.iter().zip(&op) {
                    kind.replace_point(*from, *to);
                }
                // Where a circle starts (its seam, an import's) is no
                // geometry of its source: kept.
                if let (
                    EntityKind::Circle { start, .. },
                    EntityKind::Circle {
                        start: old_start, ..
                    },
                ) = (&mut kind, &o.kind)
                {
                    *start = *old_start;
                }
                kind
            }
        };
        out.kinds.push((o.id, kind));
    }
    out.unfollowed = (0..olds.len())
        .filter(|i| !pairs.iter().any(|p| p.0 == *i))
        .map(|i| olds[i].id)
        .collect();
    // An arc that keeps its geometry while a point of it moves with
    // another curve tears too.
    torn |= olds
        .iter()
        .filter(|e| bound(&e.kind) && out.unfollowed.contains(&e.id))
        .flat_map(|e| e.kind.points())
        .any(|id| {
            moved
                .get(&id)
                .zip(old_at(id))
                .is_some_and(|(to, from)| dist(*to, from) > tol)
        });
    if torn {
        return Pairing {
            kept: true,
            ..Pairing::default()
        };
    }
    for (id, at) in moved {
        out.kinds.push((id, EntityKind::Point { at }));
    }
    out.unmatched = (0..news.len())
        .filter(|j| !pairs.iter().any(|p| p.1 == *j))
        .count();
    out
}

/// Solves a definition whose linked projections [`follow_links`] moved
/// (`followed`) as if they were dragged there from where `def` has them:
/// in steps, each starting from the last one's solution, so that geometry
/// constrained to them keeps its side, as with a dimension change (the
/// solver's continuation). One solve when they moved little or a step
/// does not solve.
pub fn solve_followed(
    def: &SketchDef,
    followed: &SketchDef,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    name: &dyn Fn(ParamId) -> String,
) -> Result<Solved, SolveError> {
    let steps = follow_steps(def, followed);
    if steps > 1
        && let Some(solved) = solve_in_steps(def, followed, steps, value, name)
    {
        return Ok(solved);
    }
    followed.solve_with(value, name)
}

/// The steps of [`solve_followed`]: one for each tenth of the sketch's
/// size the farthest point moves, at most 16.
fn follow_steps(def: &SketchDef, followed: &SketchDef) -> usize {
    let at = |e: &Entity| match e.kind {
        EntityKind::Point { at } => Some(at),
        _ => None,
    };
    let (mut low, mut high) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in def.entities.iter().chain(&followed.entities).filter_map(at) {
        for i in 0..2 {
            low[i] = low[i].min(p[i]);
            high[i] = high[i].max(p[i]);
        }
    }
    let size = dist(low, high);
    let farthest = def
        .entities
        .iter()
        .zip(&followed.entities)
        .filter_map(|(old, new)| Some(dist(at(old)?, at(new)?)))
        .fold(0.0, f64::max);
    if !(size.is_finite() && size > 0.0) {
        return 1;
    }
    ((farthest / (0.1 * size)).ceil() as usize).clamp(1, 16)
}

fn solve_in_steps(
    def: &SketchDef,
    followed: &SketchDef,
    steps: usize,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    name: &dyn Fn(ParamId) -> String,
) -> Option<Solved> {
    // From the definition solved where it was.
    let mut current = def.clone();
    current
        .solve_with(value, name)
        .ok()?
        .store(&mut current.entities);
    for k in 1..=steps {
        let t = k as f64 / steps as f64;
        for ((entity, old), new) in current
            .entities
            .iter_mut()
            .zip(&def.entities)
            .zip(&followed.entities)
        {
            if old.kind != new.kind {
                entity.kind = between(&old.kind, &new.kind, t);
            }
        }
        let solved = current.solve_with(value, name).ok()?;
        if k == steps {
            return Some(solved);
        }
        solved.store(&mut current.entities);
    }
    None
}

/// An entity's kind the part `t` of the way from `old` to `new`: points
/// and radii in between, the rest as `new`.
fn between(old: &EntityKind, new: &EntityKind, t: f64) -> EntityKind {
    let mix = |a: f64, b: f64| if t >= 1.0 { b } else { a + (b - a) * t };
    let mut kind = new.clone();
    match (old, &mut kind) {
        (EntityKind::Point { at: a }, EntityKind::Point { at: b }) => {
            *b = [mix(a[0], b[0]), mix(a[1], b[1])];
        }
        (EntityKind::Circle { radius: a, .. }, EntityKind::Circle { radius: b, .. })
        | (
            EntityKind::Ellipse {
                minor_radius: a, ..
            },
            EntityKind::Ellipse {
                minor_radius: b, ..
            },
        )
        | (
            EntityKind::EllipticalArc {
                minor_radius: a, ..
            },
            EntityKind::EllipticalArc {
                minor_radius: b, ..
            },
        ) => *b = mix(*a, *b),
        _ => {}
    }
    kind
}

/// The entities of `solution` (the definition as it evaluated, solved)
/// that differ from `def`'s: a sketch output keeps them.
pub fn moved_entities(def: &SketchDef, solution: &SketchDef) -> Vec<Entity> {
    solution
        .entities
        .iter()
        .zip(&def.entities)
        .filter(|(new, old)| new.id == old.id && new.kind != old.kind)
        .map(|(new, _)| new.clone())
        .collect()
}

/// The definition with the entities the last evaluation moved (a sketch
/// output's `moved`) in place, or None when none applies. The stored
/// definition keeps linked projections, and what is constrained to them,
/// where the last edit left them; this is the sketch as it evaluated,
/// which queries show and edits start from. Moved entities the definition
/// no longer has as the same kind of entity on the same points, and fixed
/// ones outside linked projections, are left out.
pub fn with_moved(def: &SketchDef, moved: &[Entity]) -> Option<SketchDef> {
    let mut out: Option<SketchDef> = None;
    for entity in moved {
        let Some(old) = def.entity(entity.id) else {
            continue;
        };
        let linked = || {
            def.projections
                .iter()
                .any(|p| p.entities.iter().any(|r| r.uid() == entity.id))
        };
        if old.kind == entity.kind
            || old.kind.type_name() != entity.kind.type_name()
            || old.kind.points() != entity.kind.points()
            || (old.fixed && !linked())
        {
            continue;
        }
        if let Some(target) = out.get_or_insert_with(|| def.clone()).entity_mut(entity.id) {
            target.kind = entity.kind.clone();
        }
    }
    out
}

/// The entities of a projection with the points they use, in order:
/// each curve after its points, as [`entities`] makes them.
pub fn linked_entities(def: &SketchDef, projection: &Projection) -> Vec<Entity> {
    linked_entities_at(def, &positions(&def.entities), projection)
}

/// Where each entity id is first in `entities`, as [`SketchDef::entity`]
/// finds it.
fn positions(entities: &[Entity]) -> HashMap<EntityUid, usize> {
    let mut at = HashMap::with_capacity(entities.len());
    for (i, e) in entities.iter().enumerate() {
        at.entry(e.id).or_insert(i);
    }
    at
}

/// [`linked_entities`] with the entities' [`positions`] in `def`.
fn linked_entities_at(
    def: &SketchDef,
    at: &HashMap<EntityUid, usize>,
    projection: &Projection,
) -> Vec<Entity> {
    let mut out: Vec<Entity> = Vec::new();
    for r in &projection.entities {
        let Some(e) = at.get(&r.uid()).map(|&i| &def.entities[i]) else {
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
                radius: 3.0,
                start: 0.0
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

    #[test]
    fn an_arc_seen_edge_on_reaches_its_exact_extreme() {
        // (mitcad#955) An arc in the XY plane from -50 to 70 degrees, seen
        // from XZ: the line from its far end to its exact top at 0 degrees,
        // not to the nearest of sampled points.
        let arc = Curve3::Conic {
            center: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 25.0,
            minor: 25.0,
            start: (-50f64).to_radians(),
            end: 70f64.to_radians(),
            closed: false,
        };
        let out = project(std::slice::from_ref(&arc), &frame_xz());
        let Projected::Curve(Curve2::Line { a, b }) = &out[0] else {
            panic!("{out:?}");
        };
        let (lo, hi) = (a[0].min(b[0]), a[0].max(b[0]));
        assert!((hi - 25.0).abs() < 1e-12, "{out:?}");
        assert!(
            (lo - 25.0 * 70f64.to_radians().cos()).abs() < 1e-12,
            "{out:?}"
        );
    }
}
