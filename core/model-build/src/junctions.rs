// SPDX-License-Identifier: MIT
//! Where profiles meet (`build_junctions`): from the derived parts' frames
//! and boxes at the timeline marker. Two profiles meet when their boxes
//! touch or overlap within the model's tolerance; then an end on a side
//! face is `T`, or `L` within the section's size of that side's end; two
//! ends at one point are `L` (axes at an angle) or `inline` (on one axis);
//! two sides crossing are `cross`.

use crate::BuildRole;
use crate::System;
use crate::geometry::{
    V3, add, angle_between, cross as cross3, dot, in_box, inverse, moved_box, norm, parts,
    perpendicular, scale, sub, unit, z_axis,
};
use crate::parts::PartInfo;
use crate::prelude::*;
use mitcad_model::Document;
use mitcad_model::ids::{ComponentUid, OccurrenceUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::transform::Transform;

/// Distances within this meet, mm.
const TOLERANCE: f64 = 1e-3;

/// A cut end's plane in a part's coordinates (mitcad#521): a point on it
/// and its outward normal, as the derived part cuts it.
pub(crate) type CutPlane = (V3, V3);

/// Whether a point (in a part's coordinates) is in its material: in its
/// box and on the inner side of its end cuts, within the tolerance.
pub(crate) fn holds(bounds: &BoundingBox, cuts: &[Option<CutPlane>; 2], local: V3) -> bool {
    in_box(local, bounds, TOLERANCE)
        && cuts
            .iter()
            .flatten()
            .all(|(p, n)| dot(sub(local, *p), *n) <= TOLERANCE)
}

/// The corners of a part's material (in its coordinates): its box clipped
/// by its end cuts, each face of the box clipped by the cut planes.
pub(crate) fn material_corners(bounds: &BoundingBox, cuts: &[Option<CutPlane>; 2]) -> Vec<V3> {
    let c: Vec<V3> = bounds.corners().collect();
    // The faces by their corners in turn (bit 0: x, bit 1: y, bit 2: z).
    const FACES: [[usize; 4]; 6] = [
        [0, 2, 6, 4],
        [1, 3, 7, 5],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [0, 1, 3, 2],
        [4, 5, 7, 6],
    ];
    let mut out = Vec::new();
    for face in FACES {
        let mut polygon: Vec<V3> = face.iter().map(|i| c[*i]).collect();
        for (p, n) in cuts.iter().flatten() {
            polygon = clip_polygon(&polygon, *p, *n);
        }
        out.extend(polygon);
    }
    out
}

/// What of a polygon lies on the inner side of a plane (Sutherland and
/// Hodgman).
fn clip_polygon(polygon: &[V3], point: V3, normal: V3) -> Vec<V3> {
    let side = |v: V3| dot(sub(v, point), normal);
    let mut out = Vec::new();
    for (i, a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        let (sa, sb) = (side(*a), side(b));
        if sa <= 0.0 {
            out.push(*a);
        }
        if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
            out.push(add(*a, scale(sub(b, *a), sa / (sa - sb))));
        }
    }
    out
}

/// The convex hull of points in a plane, counter-clockwise.
fn hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if points.len() < 3 {
        return points;
    }
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower: Vec<[f64; 2]> = Vec::new();
    for p in &points {
        while lower.len() >= 2 && turn(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for p in points.iter().rev() {
        while upper.len() >= 2 && turn(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The area two convex polygons (counter-clockwise) have in common.
fn common_area(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    if a.len() < 3 || b.len() < 3 {
        return 0.0;
    }
    let mut out: Vec<[f64; 2]> = a.to_vec();
    for (i, p) in b.iter().enumerate() {
        let q = b[(i + 1) % b.len()];
        // Inside: left of the edge p -> q.
        let side = |v: [f64; 2]| (q[0] - p[0]) * (v[1] - p[1]) - (q[1] - p[1]) * (v[0] - p[0]);
        let input = std::mem::take(&mut out);
        for (k, s) in input.iter().enumerate() {
            let e = input[(k + 1) % input.len()];
            let (ss, se) = (side(*s), side(e));
            if ss >= 0.0 {
                out.push(*s);
            }
            if (ss > 0.0 && se < 0.0) || (ss < 0.0 && se > 0.0) {
                let t = ss / (ss - se);
                out.push([s[0] + (e[0] - s[0]) * t, s[1] + (e[1] - s[1]) * t]);
            }
        }
        if out.len() < 3 {
            return 0.0;
        }
    }
    (0..out.len())
        .map(|i| {
            let (s, e) = (out[i], out[(i + 1) % out.len()]);
            s[0] * e[1] - e[0] * s[1]
        })
        .sum::<f64>()
        / 2.0
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait JunctionCutsBuild<K: Kernel> {
    /// How many of a junction's members (`build_junctions`' id) a shaft
    /// passes through, and how many it has (mitcad#521): the shaft from
    /// `from` to `to` along the unit `axis` from `entry` (in the BUILD
    /// host), its points every half millimetre (from half a millimetre in)
    /// in a member's material.
    fn shaft_members(
        &self,
        junction: &str,
        entry: V3,
        axis: V3,
        from: f64,
        to: f64,
    ) -> (usize, usize);
}

impl<K: Kernel> JunctionCutsBuild<K> for Document<K> {
    fn shaft_members(
        &self,
        junction: &str,
        entry: V3,
        axis: V3,
        from: f64,
        to: f64,
    ) -> (usize, usize) {
        let members: Vec<ComponentUid> = junction
            .split('|')
            .filter_map(|side| side.split(':').next()?.parse().ok())
            .collect();
        let (from, to) = (from.max(0.0) + 0.5, to - 0.5);
        let steps = ((to - from) / 0.5).ceil().max(1.0) as usize;
        let met = members
            .iter()
            .filter(|c| {
                let Some(m) = self.member(**c) else {
                    return false;
                };
                let length = m
                    .info
                    .length
                    .map_or(m.bounds.max[2] - m.bounds.min[2], |(_, l)| l);
                let cuts = self.cut_planes(&m.info, length);
                let to_local = inverse(&m.placement);
                (0..=steps).any(|i| {
                    let s = from + (to - from) * i as f64 / steps as f64;
                    holds(
                        &m.bounds,
                        &cuts,
                        to_local.apply_point(add(entry, scale(axis, s))),
                    )
                })
            })
            .count();
        (met, members.len())
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait JunctionCutsCrateOps<K: Kernel> {
    /// A part's end cuts as planes in its coordinates (mitcad#521), as the
    /// derived part cuts them: from the section its metadata gives (a
    /// timber member's, centred on its axis) and its length; none for a
    /// square end or a part of no such section.
    fn cut_planes(&self, info: &PartInfo, length: f64) -> [Option<CutPlane>; 2];
}

impl<K: Kernel> JunctionCutsCrateOps<K> for Document<K> {
    fn cut_planes(&self, info: &PartInfo, length: f64) -> [Option<CutPlane>; 2] {
        let (Some([w, t]), Some(ends)) = (info.meta.section_size(), info.ends()) else {
            return [None, None];
        };
        let section = BoundingBox {
            min: [-w / 2.0, -t / 2.0, 0.0],
            max: [w / 2.0, t / 2.0, length],
        };
        let params = self.parameters();
        [(&ends.a, false), (&ends.b, true)].map(|(cut, b)| {
            let cut = cut.as_ref()?;
            let angle = params.value(cut.angle)?;
            if angle.abs() <= 1e-12 {
                return None;
            }
            mitcad_model::internal::features::end_plane(
                Some(&section),
                b,
                length,
                angle,
                cut.tilt.unwrap_or_default(),
            )
        })
    }
}

/// The frame of a cut end's face, in the part's coordinates: where its axis
/// meets the cut plane, z the plane's outward normal.
pub(crate) fn cut_face_frame((p, n): CutPlane) -> Option<Transform> {
    if n[2].abs() < 1e-9 {
        return None;
    }
    let z = p[2] + (p[0] * n[0] + p[1] * n[1]) / n[2];
    let origin = [0.0, 0.0, z];
    let x = unit(sub([1.0, 0.0, 0.0], scale(n, n[0]))).unwrap_or_else(|| perpendicular(n));
    Some(crate::geometry::frame(origin, x, n))
}

/// One part of a junction.
#[derive(Debug, Clone, PartialEq)]
pub struct JunctionSide {
    pub component: ComponentUid,
    /// The path from the BUILD host.
    pub occurrence: Vec<OccurrenceUid>,
    /// `end_a`, `end_b`, a slot frame, or `side` (a closed side).
    pub at: String,
    /// That frame in the design's coordinates.
    pub frame: Transform,
}

/// Where two profiles meet.
#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    pub id: String,
    /// `L`, `T`, `cross` or `inline`.
    pub kind: &'static str,
    /// The part whose end meets (for `T` and `L`), and the other.
    pub a: JunctionSide,
    pub b: JunctionSide,
    /// Between the axes, radians.
    pub angle: f64,
    pub systems: [Option<System>; 2],
    /// The free room in the inner corner: along a, along b and across.
    pub space: [f64; 3],
    /// Connectors already there.
    pub connectors: Vec<ComponentUid>,
    /// b's axis from the junction into b (design coordinates): at a corner
    /// on b's side towards b's farther end, the frame's inside
    /// (mitcad#1115).
    pub b_axis: V3,
}

/// A profile as the search sees it.
struct Placed {
    info: PartInfo,
    path: Vec<OccurrenceUid>,
    world: Transform,
    bounds: BoundingBox,
    world_box: BoundingBox,
    length: f64,
    /// Its end cuts (mitcad#521): its material is its box on their inner side.
    cuts: [Option<CutPlane>; 2],
}

impl Placed {
    fn frame(&self, name: &str) -> Option<Transform> {
        Some(self.world.after(self.info.mount(name)?))
    }

    /// The axis from end A to end B in the design.
    fn axis(&self) -> V3 {
        self.world.apply_vector([0.0, 0.0, 1.0])
    }

    fn side(&self, at: String, frame: Transform) -> JunctionSide {
        JunctionSide {
            component: self.info.component,
            occurrence: self.path.clone(),
            at,
            frame,
        }
    }

    /// Whether the end (`end_a`, `end_b`) has a cut.
    fn is_cut(&self, end: &str) -> bool {
        self.info
            .def
            .as_ref()
            .is_some_and(|d| d.ends.end(end == "end_b").is_some())
    }

    /// Whether a point (its coordinates) is in its material.
    fn holds(&self, local: V3) -> bool {
        holds(&self.bounds, &self.cuts, local)
    }

    /// Its material's corners in the design.
    fn material(&self) -> Vec<V3> {
        material_corners(&self.bounds, &self.cuts)
            .into_iter()
            .map(|c| self.world.apply_point(c))
            .collect()
    }

    /// A cut end's face (`end_a`, `end_b`) in the design: where its axis
    /// meets the cut, z out of it (mitcad#521); None for a square end.
    fn cut_face(&self, end: &str) -> Option<Transform> {
        let plane = self.cuts[usize::from(end == "end_b")]?;
        Some(self.world.after(&cut_face_frame(plane)?))
    }

    /// The section's size across the axis.
    fn section(&self) -> f64 {
        let b = &self.bounds;
        (b.max[0] - b.min[0]).max(b.max[1] - b.min[1])
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait JunctionsBuild<K: Kernel> {
    /// The junctions of the design's profiles (of `only` when given).
    fn build_junctions(&self, only: Option<&[ComponentUid]>) -> Vec<Junction>;
}

impl<K: Kernel> JunctionsBuild<K> for Document<K> {
    fn build_junctions(&self, only: Option<&[ComponentUid]>) -> Vec<Junction> {
        let active = self.host_placement();
        let placed: Vec<Placed> = self
            .build_parts_info()
            .into_iter()
            .filter(|i| i.meta.role == BuildRole::Profile || i.is_cut_to_length())
            .filter(|i| i.meta.role != BuildRole::Panel)
            .filter_map(|info| {
                let path = self.placed_in_host(info.component)?;
                let world = active.after(&self.path_transform(&path));
                let bounds = self.part_bounds(info.component)?;
                let length = info
                    .length
                    .map_or(bounds.max[2] - bounds.min[2], |(_, l)| l);
                let cuts = self.cut_planes(&info, length);
                Some(Placed {
                    world_box: moved_box(&bounds, &world),
                    info,
                    path,
                    world,
                    bounds,
                    length,
                    cuts,
                })
            })
            .collect();
        let connectors: Vec<(ComponentUid, String)> = self
            .assembly()
            .components
            .iter()
            .filter_map(|c| Some((c.uid, c.build.as_ref()?.junction.clone()?)))
            .collect();
        let mut out = Vec::new();
        for (i, p) in placed.iter().enumerate() {
            for q in &placed[i + 1..] {
                if let Some(only) = only
                    && !only.contains(&p.info.component)
                    && !only.contains(&q.info.component)
                {
                    continue;
                }
                if !p.world_box.overlaps(&q.world_box, TOLERANCE) {
                    continue;
                }
                // The newer part is a when both could be.
                let (p, q) = if p.info.component > q.info.component {
                    (p, q)
                } else {
                    (q, p)
                };
                // An end on the other part (either way round) before sides
                // crossing: a square corner lying on an end face is an L, not
                // a cross (mitcad#427).
                if let Some(mut junction) =
                    meet(p, q).or_else(|| meet(q, p)).or_else(|| cross(p, q))
                {
                    junction.connectors = connectors
                        .iter()
                        .filter(|(_, id)| *id == junction.id)
                        .map(|(c, _)| *c)
                        .collect();
                    out.push(junction);
                }
            }
        }
        out
    }
}

/// How `p` meets `q`: an end of p on q's end or side.
fn meet(p: &Placed, q: &Placed) -> Option<Junction> {
    let ends = ["end_a", "end_b"];
    // Two ends at one point, or two cut ends each in the other's box (a
    // shared mitre: the outer corners meet, the end frames do not,
    // mitcad#443).
    let to_p = inverse(&p.world);
    let to_q = inverse(&q.world);
    for e in ends {
        let Some(pe) = p.frame(e) else { continue };
        for f in ends {
            let Some(qf) = q.frame(f) else { continue };
            let mitred = p.is_cut(e)
                && q.is_cut(f)
                && in_box(to_q.apply_point(pe.translation), &q.bounds, TOLERANCE)
                && in_box(to_p.apply_point(qf.translation), &p.bounds, TOLERANCE)
                && one_cut(p, e, q, f);
            if mitred || norm(sub(pe.translation, qf.translation)) <= TOLERANCE {
                let angle = angle_between(scale(z_axis(&pe), -1.0), z_axis(&qf));
                let kind = if dot(z_axis(&pe), z_axis(&qf)) < -1.0 + 1e-6 {
                    "inline"
                } else {
                    "L"
                };
                return Some(junction(
                    kind,
                    p.side(e.to_owned(), pe),
                    q.side(f.to_owned(), qf),
                    angle,
                    p,
                    q,
                    scale(z_axis(&qf), -1.0),
                ));
            }
        }
    }
    // An end of p on a side of q: its end frame, else a cut end's face
    // (mitcad#521: a plumb cut rafter's end frame lies beyond its cut, the
    // cut face's middle on the ridge board's side), in q's material.
    let mounts = ends.into_iter().filter_map(|e| Some((e, p.frame(e)?)));
    let faces = ends.into_iter().filter_map(|e| Some((e, p.cut_face(e)?)));
    for (e, pe) in mounts.chain(faces) {
        let point = to_q.apply_point(pe.translation);
        let out_of_p = to_q.apply_vector(z_axis(&pe));
        if !q.holds(point) {
            continue;
        }
        // The side face it stands on: the slot frame whose plane holds the
        // point and faces it, else a side of the box.
        let mut at = None;
        for (name, slot) in q.info.sides() {
            let [o, _, _, z] = parts(slot);
            if dot(sub(point, o), z).abs() <= TOLERANCE && dot(out_of_p, z) < -1.0 + 1e-6 {
                at = Some((name.clone(), q.world.after(slot)));
                break;
            }
        }
        let at = match at {
            Some(at) => at,
            None => {
                let b = &q.bounds;
                let on_side = (0..2).any(|k| {
                    (point[k] - b.min[k]).abs() <= TOLERANCE
                        || (point[k] - b.max[k]).abs() <= TOLERANCE
                });
                if !on_side || out_of_p[2].abs() > 1e-6 {
                    continue;
                }
                (
                    "side".to_owned(),
                    q.world.after(&Transform::translation(point)),
                )
            }
        };
        // Within the section's size of q's end: a corner.
        let from_end = point[2].min(q.length - point[2]);
        let kind = if from_end <= q.section() + TOLERANCE {
            "L"
        } else {
            "T"
        };
        let angle = angle_between(scale(z_axis(&pe), -1.0), q.axis());
        // Into q at a corner: towards its farther end, where the frame's
        // inner corner is; q's own axis would point out of the frame at a
        // corner near its end B (mitcad#1115).
        let into_q = if kind == "L" && point[2] > q.length - point[2] {
            scale(q.axis(), -1.0)
        } else {
            q.axis()
        };
        let mut j = junction(
            kind,
            p.side(e.to_owned(), pe),
            q.side(at.0, at.1),
            angle,
            p,
            q,
            into_q,
        );
        // Along b: the room beside a, to b's farther end.
        j.space[1] = point[2].max(q.length - point[2]);
        return Some(j);
    }
    None
}

/// Whether two cut ends share one cut (a shared mitre, mitcad#443): their
/// cut planes are one plane, the cuts facing each other. Two rafters' plumb
/// cuts on each side of a ridge board are not (mitcad#521). True when a
/// plane is not known (a part whose metadata gives no section).
fn one_cut(p: &Placed, e: &str, q: &Placed, f: &str) -> bool {
    let (Some((pp, pn)), Some((qp, qn))) = (
        p.cuts[usize::from(e == "end_b")],
        q.cuts[usize::from(f == "end_b")],
    ) else {
        return true;
    };
    let (pn, qn) = (p.world.apply_vector(pn), q.world.apply_vector(qn));
    let (po, qo) = (p.world.apply_point(pp), q.world.apply_point(qp));
    dot(pn, qn) < -1.0 + 1e-6 && dot(sub(qo, po), pn).abs() <= TOLERANCE
}

/// Whether two members' material meets where they would cross: along
/// `n` (square to both axes) their extents touch or overlap, and across it
/// their outlines have an area in common (mitcad#521: the boxes of two
/// rafters' plumb cut ends overlap above a ridge board where neither has
/// material).
fn materials_meet(p: &Placed, q: &Placed, n: V3) -> bool {
    let (pm, qm) = (p.material(), q.material());
    let range = |m: &[V3]| {
        m.iter()
            .map(|c| dot(*c, n))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            })
    };
    let ((p0, p1), (q0, q1)) = (range(&pm), range(&qm));
    if p0 > q1 + TOLERANCE || q0 > p1 + TOLERANCE {
        return false;
    }
    let u = perpendicular(n);
    let v = cross3(n, u);
    let flat = |m: &[V3]| hull(m.iter().map(|c| [dot(*c, u), dot(*c, v)]).collect());
    common_area(&flat(&pm), &flat(&qm)) > TOLERANCE
}

/// Two sides of `p` (the newer) and `q` crossing.
fn cross(p: &Placed, q: &Placed) -> Option<Junction> {
    if p.info.component > q.info.component {
        let pa = p.axis();
        let qa = q.axis();
        if dot(pa, qa).abs() < 1.0 - 1e-6
            && unit(cross3(pa, qa)).is_some_and(|n| materials_meet(p, q, n))
        {
            let towards = |from: &Placed, to: &Placed| -> (String, Transform) {
                let centre = |b: &BoundingBox| -> V3 {
                    std::array::from_fn(|i| (b.min[i] + b.max[i]) / 2.0)
                };
                let target = centre(&to.world_box);
                from.info
                    .sides()
                    .into_iter()
                    .map(|(n, t)| (n.clone(), from.world.after(t)))
                    .max_by(|x, y| {
                        let d = |t: &Transform| dot(z_axis(t), sub(target, t.translation));
                        d(&x.1).total_cmp(&d(&y.1))
                    })
                    .unwrap_or_else(|| ("side".to_owned(), from.world))
            };
            let (pa_name, pa_frame) = towards(p, q);
            let (qa_name, qa_frame) = towards(q, p);
            return Some(junction(
                "cross",
                p.side(pa_name, pa_frame),
                q.side(qa_name, qa_frame),
                angle_between(pa, qa),
                p,
                q,
                qa,
            ));
        }
    }
    None
}

fn junction(
    kind: &'static str,
    a: JunctionSide,
    b: JunctionSide,
    angle: f64,
    p: &Placed,
    q: &Placed,
    b_axis: V3,
) -> Junction {
    let id = format!("{}:{}|{}:{}", a.component, a.at, b.component, b.at);
    Junction {
        id,
        kind,
        a,
        b,
        angle,
        systems: [p.info.meta.system.clone(), q.info.meta.system.clone()],
        space: [p.length, q.length, p.section().min(q.section())],
        connectors: Vec::new(),
        b_axis,
    }
}
