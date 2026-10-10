// SPDX-License-Identifier: MIT
//! References resolved geometrically against the replayed bodies: the file's
//! B-rep fingerprints (bodies, faces, edges) as Mitcad names, planar faces
//! for sketch and construction planes, the edges a fillet or chamfer
//! consumed (from the history state after it), and the final comparison.

use mitcad_f3d::design::ir::{Fingerprint, Vec3};
use mitcad_model::analysis::CompareOptions;
use mitcad_model::{BodyUid, Document, EdgeName, FaceName, Kernel, Plane};

use crate::geom::{self, mm, mm3};
use crate::history::{Sig, StoredBody};
use crate::report::BodyReport;
use std::sync::Arc;

/// Every planar face of the bodies at the marker with its plane (outward
/// normal). Each body's are kept by its version ([`planes_of`]).
pub fn planar_faces<K: Kernel>(doc: &Document<K>) -> Vec<(BodyUid, FaceName, Plane)> {
    let kernel = doc.kernel();
    let mut out = Vec::new();
    for body in doc.bodies() {
        // Large bodies have thousands of faces, a kernel call each
        // (mitcad#82: the watchdog's progress).
        crate::tick();
        let planes = match doc.body_version(body.uid) {
            Some(version) => planes_of(version, || body_planes(kernel, body.shape)),
            None => Arc::new(body_planes(kernel, body.shape)),
        };
        out.extend(
            planes
                .iter()
                .map(|(name, plane)| (body.uid, name.clone(), *plane)),
        );
    }
    out
}

// Planar faces kept by the body's version (mitcad#103).

/// The planar faces of a body's shape with their planes.
fn body_planes<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<(FaceName, Plane)> {
    let Ok(faces) = kernel.faces(shape) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for face in faces.iter().filter(|f| f.surface == "plane") {
        let Some(name) = face.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
            continue;
        };
        crate::tick();
        if let Ok(plane) = kernel.face_plane(shape, &name) {
            out.push((name, plane));
        }
    }
    out
}

/// How many bodies' planar faces an import keeps ([`planes_of`]).
const PLANES_KEPT: usize = 64;

type Planes = Arc<Vec<(FaceName, Plane)>>;

std::thread_local! {
    /// The planar faces found by the import running on this thread, by
    /// body version, the latest last ([`KeptPlanes`]).
    static KEPT: std::cell::RefCell<Option<Vec<(u128, Planes)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Keeps the planar faces found on this thread until dropped
/// ([`crate::import_design`]): every sketch looks for its plane among the
/// faces of all bodies, and on bodies of thousands of faces that took
/// seconds a sketch while the bodies stayed the same (mitcad#103). A
/// version names one shape of the document (its feature, definition and
/// inputs).
pub(crate) struct KeptPlanes(bool);

impl KeptPlanes {
    pub fn new() -> Self {
        KeptPlanes(KEPT.with(|k| k.borrow_mut().replace(Vec::new()).is_none()))
    }
}

impl Drop for KeptPlanes {
    fn drop(&mut self) {
        // (An import inside an import keeps the outer one's.)
        if self.0 {
            KEPT.with(|k| k.borrow_mut().take());
        }
    }
}

/// The planar faces of the body whose shape has `version`: kept ones, or
/// found by `find` (and kept while [`KeptPlanes`] is).
pub(crate) fn planes_of(version: u128, find: impl FnOnce() -> Vec<(FaceName, Plane)>) -> Planes {
    let kept = KEPT.with(|k| {
        let mut k = k.borrow_mut();
        let kept = k.as_mut()?;
        let i = kept.iter().position(|(v, _)| *v == version)?;
        let entry = kept.remove(i);
        let planes = Arc::clone(&entry.1);
        kept.push(entry);
        Some(planes)
    });
    if let Some(planes) = kept {
        return planes;
    }
    let planes = Arc::new(find());
    KEPT.with(|k| {
        if let Some(kept) = k.borrow_mut().as_mut() {
            if kept.len() >= PLANES_KEPT {
                kept.remove(0);
            }
            kept.push((version, Arc::clone(&planes)));
        }
    });
    planes
}

/// A body by its fingerprint: volume, area and box (the replay should hold
/// the same body before the feature that refers to it).
pub fn resolve_body<K: Kernel>(doc: &Document<K>, fp: &Fingerprint) -> Option<BodyUid> {
    let Some(volume) = fp.volume.map(|v| v * 1000.0) else {
        return resolve_body_by_points(doc, fp);
    };
    let kernel = doc.kernel();
    // Among bodies of the same volume, the one of the same name.
    let mut best: Option<((bool, f64), BodyUid)> = None;
    for body in doc.bodies() {
        let Some(sig) = Sig::of(kernel, body.shape) else {
            continue;
        };
        let dv = (sig.volume - volume).abs() / volume.abs().max(1e-9);
        let named = fp.name.as_deref() == Some(body.name.as_str());
        let score = (!named, dv);
        if dv < 1e-3 && best.is_none_or(|(s, _)| score < s) {
            best = Some((score, body.uid));
        }
    }
    best.map(|(_, uid)| uid)
}

/// A body the stream decoder found by its names (no volume): the only body
/// with an edge through each of the points it gives (`_f3d.edge_points`,
/// middle points of some of the body's edges, cm).
fn resolve_body_by_points<K: Kernel>(doc: &Document<K>, fp: &Fingerprint) -> Option<BodyUid> {
    let points: Vec<Vec3> = fp
        .f3d
        .as_ref()?
        .edge_points
        .as_ref()?
        .iter()
        .map(|&p| mm3(p))
        .collect();
    if points.is_empty() {
        return None;
    }
    let kernel = doc.kernel();
    let fits: Vec<BodyUid> = doc
        .bodies()
        .iter()
        .filter(|body| {
            crate::tick();
            let middles = edge_midpoints(kernel, body.shape);
            points.iter().all(|p| {
                middles
                    .iter()
                    .any(|(_, m, _)| geom::distance(*m, *p) <= 1e-3)
            })
        })
        .map(|body| body.uid)
        .collect();
    match fits[..] {
        [uid] => Some(uid),
        [] => resolve_body_by_boundary(doc, &points),
        _ => None,
    }
}

/// The only body whose faces pass through each of the points: a body a
/// replayed feature made (a sweep, a pipe) has the file's surfaces but not
/// necessarily its edges (seams and splits elsewhere).
fn resolve_body_by_boundary<K: Kernel>(doc: &Document<K>, points: &[Vec3]) -> Option<BodyUid> {
    let kernel = doc.kernel();
    let fits: Vec<BodyUid> = doc
        .bodies()
        .iter()
        .filter(|body| {
            crate::tick();
            kernel
                .boundary_distances(body.shape, points)
                .is_ok_and(|d| d.iter().all(|&d| d <= 1e-3))
        })
        .map(|body| body.uid)
        .collect();
    match fits[..] {
        [uid] => Some(uid),
        _ => None,
    }
}

/// A cylindrical face by two points of its fingerprint (`start_point`,
/// `end_point`: an `.ipt` thread's ends), each on the cylinder's surface
/// or on its axis, within the face's extent along the axis: the face
/// with both on its surface first, then the narrowest.
pub fn resolve_cylinder_through<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
) -> Option<(BodyUid, FaceName)> {
    let points = [mm3(fp.start_point.flatten()?), mm3(fp.end_point.flatten()?)];
    let kernel = doc.kernel();
    const TOLERANCE: f64 = 1e-3;
    let mut best: Option<((bool, f64), BodyUid, FaceName)> = None;
    for body in doc.bodies() {
        crate::tick();
        let Ok(faces) = kernel.faces(body.shape) else {
            continue;
        };
        for face in faces.iter().filter(|f| f.surface == "cylinder") {
            let Some(name) = face.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
                continue;
            };
            crate::tick();
            let Ok(c) = kernel.face_cylinder(body.shape, &name) else {
                continue;
            };
            let Some(along) = geom::unit(c.axis.direction) else {
                continue;
            };
            let mut on_surface = true;
            let fits = points.iter().all(|&p| {
                let d = geom::sub(p, c.axis.origin);
                let t = geom::dot(d, along);
                let off = geom::norm(geom::sub(d, geom::scale(along, t)));
                let within = t >= -TOLERANCE && t <= c.length + TOLERANCE;
                let surface = (off - c.radius).abs() <= TOLERANCE;
                on_surface &= surface;
                within && (surface || off <= TOLERANCE)
            });
            if !fits {
                continue;
            }
            let key = (!on_surface, c.radius);
            if best.as_ref().is_none_or(|(k, _, _)| key < *k) {
                best = Some((key, body.uid, name));
            }
        }
    }
    best.map(|(_, b, f)| (b, f))
}

/// A cylindrical face by its surface alone (a fingerprint without a point
/// on it: an `.ipt` extrusion's extent): a face on the cylinder of the
/// fingerprint's axis (`origin`, `axis`) and `radius` (cm), the largest.
pub fn resolve_cylinder_surface<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
) -> Option<(BodyUid, FaceName)> {
    let g = fp
        .geometry
        .as_ref()
        .filter(|g| g.geometry_type.as_deref() == Some("Cylinder"))?;
    let (origin, axis, radius) = (mm3(g.origin?), geom::unit(g.axis?)?, mm(g.radius?));
    let kernel = doc.kernel();
    const TOLERANCE: f64 = 1e-3;
    let mut best: Option<(f64, BodyUid, FaceName)> = None;
    for body in doc.bodies() {
        crate::tick();
        let Ok(faces) = kernel.faces(body.shape) else {
            continue;
        };
        for face in faces.iter().filter(|f| f.surface == "cylinder") {
            let Some(name) = face.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
                continue;
            };
            let Ok(c) = kernel.face_cylinder(body.shape, &name) else {
                continue;
            };
            let Some(along) = geom::unit(c.axis.direction) else {
                continue;
            };
            let d = geom::sub(origin, c.axis.origin);
            let off = geom::norm(geom::sub(d, geom::scale(along, geom::dot(d, along))));
            if (c.radius - radius).abs() > TOLERANCE
                || geom::norm(geom::cross(along, axis)) > 1e-6
                || off > TOLERANCE
            {
                continue;
            }
            if best.as_ref().is_none_or(|(a, _, _)| face.area > *a) {
                best = Some((face.area, body.uid, name));
            }
        }
    }
    best.map(|(_, b, f)| (b, f))
}

/// A face by its fingerprint: the face with the same surface type through
/// `point_on_face`, preferring the area that is closest.
pub fn resolve_face<K: Kernel>(doc: &Document<K>, fp: &Fingerprint) -> Option<(BodyUid, FaceName)> {
    let point = mm3(fp.point_on_face?);
    let area = fp.area.map(|a| a * 100.0);
    let surface = fp
        .geometry
        .as_ref()
        .and_then(|g| g.geometry_type.as_deref())
        .map(surface_kind);
    let kernel = doc.kernel();
    let mut best: Option<(f64, BodyUid, FaceName)> = None;
    for body in doc.bodies() {
        crate::tick();
        let Ok(faces) = kernel.faces(body.shape) else {
            continue;
        };
        for face in faces {
            if surface.is_some_and(|s| s != face.surface) {
                continue;
            }
            let Some(name) = face.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
                continue;
            };
            crate::tick();
            let Ok(near) = kernel.face_point_normal(body.shape, &name, point) else {
                continue;
            };
            let d = geom::distance(near.point, point);
            if d > 1e-3 {
                continue;
            }
            let da = area.map_or(0.0, |a| (face.area - a).abs() / a.max(1e-9));
            let score = d + da;
            if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
                best = Some((score, body.uid, name));
            }
        }
    }
    best.map(|(_, b, f)| (b, f))
}

/// Mitcad's name of a surface type of the dump's geometry.
fn surface_kind(kind: &str) -> &'static str {
    match kind {
        "Plane" => "plane",
        "Cylinder" => "cylinder",
        "Cone" => "cone",
        "Sphere" => "sphere",
        "Torus" => "torus",
        _ => "bspline",
    }
}

/// The midpoints of a body's edges, by name.
pub fn edge_midpoints<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<(EdgeName, Vec3, f64)> {
    edge_middles(kernel, shape)
        .into_iter()
        .map(|(name, p, _, length)| (name, p, length))
        .collect()
}

/// The midpoints of a body's edges with the unit tangent there and the
/// length, by name.
fn edge_middles<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<(EdgeName, Vec3, Vec3, f64)> {
    let Ok(middles) = kernel.edge_middles(shape) else {
        return Vec::new();
    };
    middles
        .into_iter()
        .filter_map(|m| Some((m.name.parse().ok()?, m.point, m.tangent, m.length)))
        .collect()
}

/// An edge by its fingerprint: the edge through `mid_point` with the same
/// length.
pub fn resolve_edge<K: Kernel>(doc: &Document<K>, fp: &Fingerprint) -> Option<(BodyUid, EdgeName)> {
    let mid = mm3(fp.mid_point?);
    let length = fp.length.map(mm);
    let kernel = doc.kernel();
    let mut best: Option<(f64, BodyUid, EdgeName)> = None;
    for body in doc.bodies() {
        crate::tick();
        for (name, at, len) in edge_midpoints(kernel, body.shape) {
            let d = geom::distance(at, mid);
            let dl = length.map_or(0.0, |l| (len - l).abs());
            if d > 1e-3 || dl > 1e-3 * len.max(1.0) {
                continue;
            }
            if best.as_ref().is_none_or(|(s, _, _)| d + dl < *s) {
                best = Some((d + dl, body.uid, name));
            }
        }
    }
    best.map(|(_, b, e)| (b, e))
}

/// An edge gone after a fillet or chamfer.
#[derive(Debug, Clone)]
pub struct GoneEdge {
    pub name: EdgeName,
    /// How far its midpoint is from the faces after.
    pub distance: f64,
    /// The angle of the wedge between its faces on the side a rounding
    /// ball sits (π − the angle between the outward normals), radians;
    /// None when the normals are not known.
    pub wedge: Option<f64>,
    /// Its midpoint and length.
    pub mid: Vec3,
    pub length: f64,
    /// The cross section of its faces at the midpoint, when they meet at
    /// an angle.
    pub across: Option<Across>,
}

/// Two faces across an edge at a point: per face (in the order of the
/// edge's name) the unit direction into the face, perpendicular to the
/// edge, and its outward normal; and whether the material between them
/// is convex (less than half a turn: a rounding cuts it away) or concave
/// (a rounding fills the corner).
#[derive(Debug, Clone, Copy)]
pub struct Across {
    pub into: [Vec3; 2],
    pub normal: [Vec3; 2],
    pub convex: bool,
}

/// The edges of `shape` that are gone in `after`: their midpoints are no
/// longer on any face of `after` (a fillet or chamfer cut or filled
/// them). Seams (a face against itself) are left out: they are no
/// fillet's input.
pub fn consumed_edges<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    after: &[&K::Shape],
) -> Result<Vec<GoneEdge>, String> {
    let mut edges = edge_middles(kernel, shape);
    edges.retain(|(name, _, _, _)| {
        let [a, b] = name.faces();
        a != b
    });
    if edges.is_empty() {
        return Ok(Vec::new());
    }
    let points: Vec<Vec3> = edges.iter().map(|(_, p, _, _)| *p).collect();
    let nearest = nearest_boundary(kernel, after, &points)?;
    // Each edge with its tangent and its faces' normals at the midpoint.
    type Frame = Option<(Vec3, [Vec3; 2])>;
    let mut gone: Vec<(GoneEdge, Frame)> = edges
        .into_iter()
        .zip(nearest)
        .filter(|((_, _, _, len), d)| *d > 1e-4 * len.max(1.0) && d.is_finite())
        .map(|((name, mid, tangent, length), distance)| {
            crate::tick();
            let [a, b] = name.faces();
            let normal = |face| {
                kernel
                    .face_point_normal(shape, face, mid)
                    .ok()
                    .and_then(|p| geom::unit(p.normal))
            };
            let normals = normal(a).zip(normal(b));
            let wedge = normals
                .map(|(n1, n2)| std::f64::consts::PI - geom::dot(n1, n2).clamp(-1.0, 1.0).acos());
            let frame = normals.and_then(|(n1, n2)| Some((geom::unit(tangent)?, [n1, n2])));
            let edge = GoneEdge {
                name,
                distance,
                wedge,
                mid,
                length,
                across: None,
            };
            (edge, frame)
        })
        .collect();
    // Convex or concave: a point just off the edge above the first face
    // and below the second is outside convex material, inside concave.
    let probes: Vec<(usize, Vec3)> = gone
        .iter()
        .enumerate()
        .filter_map(|(i, (edge, frame))| {
            let (_, [n1, n2]) = (*frame)?;
            let eps = (0.1 * edge.length).min(0.01);
            Some((i, geom::add(edge.mid, geom::scale(geom::sub(n1, n2), eps))))
        })
        .collect();
    if !probes.is_empty() {
        // Which side of the nearer of the edge's two faces the point is
        // on; the solid classifier only where that does not tell (it casts
        // rays through every face, about 0.1 s a point on stored bodies of
        // splines, and fillets ask about the same edges state after state).
        let mut inside: Vec<Option<bool>> = probes
            .iter()
            .map(|(i, p)| {
                let (edge, frame) = &gone[*i];
                let (_, [n1, n2]) = (*frame)?;
                if geom::norm(geom::sub(n1, n2)) < 1e-6 {
                    return None;
                }
                crate::tick();
                let side = |face| {
                    let near = kernel.face_point_normal(shape, face, *p).ok()?;
                    let normal = geom::unit(near.normal)?;
                    let off = geom::sub(*p, near.point);
                    Some((geom::norm(off), geom::dot(off, normal)))
                };
                let [a, b] = edge.name.faces();
                let ((da, sa), (db, sb)) = (side(a)?, side(b)?);
                let s = if da <= db { sa } else { sb };
                (s.abs() > 1e-9).then_some(s < 0.0)
            })
            .collect();
        let unknown: Vec<usize> = (0..probes.len()).filter(|&k| inside[k].is_none()).collect();
        if !unknown.is_empty() {
            let points: Vec<Vec3> = unknown.iter().map(|&k| probes[k].1).collect();
            let classified = kernel
                .points_inside(shape, &points)
                .map_err(|e| e.to_string())?;
            for (k, c) in unknown.into_iter().zip(classified) {
                inside[k] = Some(c);
            }
        }
        for ((i, _), inside) in probes.into_iter().zip(inside) {
            let inside = inside.unwrap_or(false);
            let (edge, frame) = &mut gone[i];
            let Some((t, normal)) = *frame else {
                continue;
            };
            let convex = !inside;
            // Into each face: perpendicular to the edge in the face, away
            // from the other face's side when convex, towards it when not.
            let into = |own: Vec3, other: Vec3| -> Option<Vec3> {
                let u = geom::unit(geom::cross(t, own))?;
                let side = geom::dot(u, other);
                if side.abs() < 1e-6 {
                    return None;
                }
                Some(if (side < 0.0) == convex {
                    u
                } else {
                    geom::scale(u, -1.0)
                })
            };
            if let (Some(u1), Some(u2)) = (into(normal[0], normal[1]), into(normal[1], normal[0])) {
                edge.across = Some(Across {
                    into: [u1, u2],
                    normal,
                    convex,
                });
            }
        }
    }
    Ok(gone.into_iter().map(|(edge, _)| edge).collect())
}

/// How far a point inside a face may be from the next state's faces and
/// still count as on them, mm.
const ON_FACE: f64 = 1e-3;

/// The faces of `shape` that are gone in `after`: none of the points
/// inside them is on a face of `after` any longer, as for the faces a
/// replace face replaced (the neighbours that stay keep some of theirs).
/// Faces too thin to have points stay.
pub fn lost_faces<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    after: &[&K::Shape],
) -> Result<Vec<FaceName>, String> {
    lost_faces_by(kernel, shape, after, 1.0)
}

/// The faces of `shape` of which at least `share` of the points inside
/// are on no face of `after` ([`lost_faces`] with every point): a shell's
/// removed face keeps the points near its edges on the wall's end.
pub fn lost_faces_by<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    after: &[&K::Shape],
    share: f64,
) -> Result<Vec<FaceName>, String> {
    let faces = kernel.face_points(shape).map_err(|e| e.to_string())?;
    let points: Vec<Vec3> = faces
        .iter()
        .flat_map(|f| f.points.iter().copied())
        .collect();
    if points.is_empty() || after.is_empty() {
        return Ok(Vec::new());
    }
    let nearest = nearest_boundary(kernel, after, &points)?;
    let mut lost = Vec::new();
    let mut next = 0;
    for face in &faces {
        let own = &nearest[next..next + face.points.len()];
        next += face.points.len();
        if !own.is_empty()
            && own.iter().filter(|d| **d > ON_FACE).count() as f64 >= share * own.len() as f64
            && let Ok(name) = face.name.parse::<FaceName>()
        {
            lost.push(name);
        }
    }
    Ok(lost)
}

/// A surface body (or any body) by its fingerprint: the body of that name,
/// else the one of the same area (a surface body has no volume to match).
pub fn resolve_sheet<K: Kernel>(doc: &Document<K>, fp: &Fingerprint) -> Option<BodyUid> {
    if let Some(uid) = resolve_body(doc, fp) {
        return Some(uid);
    }
    let kernel = doc.kernel();
    let bodies = doc.bodies();
    if let Some(name) = fp.name.as_deref()
        && let Some(b) = bodies.iter().find(|b| b.name == name)
    {
        return Some(b.uid);
    }
    let area = fp.area? * 100.0;
    bodies
        .iter()
        .filter_map(|b| {
            crate::tick();
            let m = kernel.mass_properties(b.shape).ok()?;
            let da = (m.area - area).abs() / area.abs().max(1e-9);
            (da < 1e-3).then_some((da, b.uid))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, uid)| uid)
}

/// The distance of each point from the nearest boundary of the shapes.
pub fn nearest_boundary<K: Kernel>(
    kernel: &K,
    shapes: &[&K::Shape],
    points: &[Vec3],
) -> Result<Vec<f64>, String> {
    let mut nearest = vec![f64::INFINITY; points.len()];
    // The bodies whose boxes hold the most points first, and each body
    // only for the points not on a face yet: most points asked about are
    // on one body's faces, and measuring a point far from a body is what
    // takes long (a stored thread's thousand edges against another body:
    // 45 s).
    let slack = 1e-3;
    let mut order: Vec<(usize, usize)> = shapes
        .iter()
        .enumerate()
        .map(|(k, body)| {
            crate::tick();
            let held = match kernel.bounding_box(body) {
                Ok(Some(b)) => points
                    .iter()
                    .filter(|p| {
                        (0..3).all(|i| p[i] >= b.min[i] - slack && p[i] <= b.max[i] + slack)
                    })
                    .count(),
                _ => 0,
            };
            (k, held)
        })
        .collect();
    order.sort_by_key(|&(k, held)| (std::cmp::Reverse(held), k));
    for (k, _) in order {
        let open: Vec<usize> = (0..points.len()).filter(|&i| nearest[i] > 0.0).collect();
        if open.is_empty() {
            break;
        }
        let asked: Vec<Vec3> = open.iter().map(|&i| points[i]).collect();
        crate::tick();
        let d = kernel
            .boundary_distances(shapes[k], &asked)
            .map_err(|e| e.to_string())?;
        for (&i, d) in open.iter().zip(d) {
            nearest[i] = nearest[i].min(d);
        }
    }
    Ok(nearest)
}

/// The cross section of a fillet or chamfer of one edge set.
#[derive(Debug, Clone, Copy)]
pub enum Section {
    /// A rounding of this radius.
    Fillet(f64),
    /// A bevel this far along the edge's first and second face.
    Chamfer(f64, f64),
    /// A bevel this far along the face it is measured on and at this
    /// angle from it (radians), measured on the second face when `true`.
    ChamferAngle(f64, f64, bool),
}

impl Section {
    /// Its size: the radius, the larger distance.
    pub fn size(&self) -> f64 {
        match *self {
            Self::Fillet(r) => r,
            Self::Chamfer(a, b) => a.max(b),
            Self::ChamferAngle(d, _, _) => d,
        }
    }

    /// How far the midpoint of an edge with this wedge angle lies from the
    /// dressup's face.
    pub fn depth(&self, wedge: f64) -> f64 {
        match *self {
            Self::Fillet(r) => fillet_distance(r, wedge),
            Self::Chamfer(a, b) => chamfer_distance(a, b, wedge),
            Self::ChamferAngle(d, angle, _) => {
                let third = std::f64::consts::PI - wedge - angle;
                let other = d * angle.sin() / third.sin().max(1e-6);
                chamfer_distance(d, other, wedge)
            }
        }
    }
}

/// Where a fillet or chamfer of `section` at a gone edge meets the edge's
/// two faces and the middle of its cross section, in the body `shape`
/// had before it (the contact points projected onto the faces). A state
/// whose faces pass through all three made that dressup at that edge.
pub fn section_points<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    gone: &GoneEdge,
    section: Section,
) -> Option<[Vec3; 3]> {
    let across = gone.across?;
    let wedge = gone.wedge?;
    let m = gone.mid;
    let faces = gone.name.faces();
    // The point of face k `s` from the edge, with the normal there.
    let on_face = |k: usize, s: f64| -> (Vec3, Vec3) {
        let p = geom::add(m, geom::scale(across.into[k], s));
        match kernel.face_point_normal(shape, &faces[k], p) {
            Ok(near) => (
                near.point,
                geom::unit(near.normal).unwrap_or(across.normal[k]),
            ),
            Err(_) => (p, across.normal[k]),
        }
    };
    match section {
        Section::Fillet(r) => {
            let setback = r / (wedge / 2.0).tan().max(1e-6);
            // The ball's centre from each contact point: into the material
            // of a convex edge, into the empty corner of a concave one.
            let sign = if across.convex { -1.0 } else { 1.0 };
            let (a, na) = on_face(0, setback);
            let (b, nb) = on_face(1, setback);
            let ca = geom::add(a, geom::scale(na, sign * r));
            let cb = geom::add(b, geom::scale(nb, sign * r));
            let c = geom::scale(geom::add(ca, cb), 0.5);
            let middle = geom::add(c, geom::scale(geom::unit(geom::sub(m, c))?, r));
            Some([a, b, middle])
        }
        Section::Chamfer(d1, d2) => {
            let (a, _) = on_face(0, d1);
            let (b, _) = on_face(1, d2);
            Some([a, b, geom::scale(geom::add(a, b), 0.5)])
        }
        Section::ChamferAngle(d, angle, second) => {
            // The triangle of the edge and the two contact points: the
            // angle `angle` at the measured face's point, `wedge` at the
            // edge.
            let third = std::f64::consts::PI - wedge - angle;
            if third <= 1e-6 {
                return None;
            }
            let other = d * angle.sin() / third.sin();
            let (d1, d2) = if second { (other, d) } else { (d, other) };
            let (a, _) = on_face(0, d1);
            let (b, _) = on_face(1, d2);
            Some([a, b, geom::scale(geom::add(a, b), 0.5)])
        }
    }
}

/// How far the midpoint of an edge with this wedge angle lies from a
/// rounding of radius `r`: r (1 / sin(α/2) − 1).
pub fn fillet_distance(r: f64, wedge: f64) -> f64 {
    r * (1.0 / (wedge / 2.0).sin().max(1e-6) - 1.0)
}

/// How far the midpoint of an edge with this wedge angle lies from a
/// chamfer `d1` and `d2` along its faces.
pub fn chamfer_distance(d1: f64, d2: f64, wedge: f64) -> f64 {
    let base = (d1 * d1 + d2 * d2 - 2.0 * d1 * d2 * wedge.cos()).sqrt();
    d1 * d2 * wedge.sin() / base.max(1e-12)
}

/// How long the geometric comparisons of the final bodies may take, s.
const COMPARE_SECONDS: f64 = 60.0;

/// How long the comparison of one body may take, s.
const BODY_COMPARE_SECONDS: f64 = 10.0;

/// Bodies whose sampled surfaces lie within this of each other (mm) are
/// not compared by boolean differences (mitcad#69): of nearly coincident
/// bodies those rarely finish in their time, and when they do they can
/// take the bodies as apart; the deviation says enough. (The stored
/// geometry's own approximations, spline fits, lie within it.)
const BOOLEANS_ABOVE: f64 = 0.01;

/// The file's stored bodies against the replay: each matched to the Mitcad
/// body with the same signature, else the nearest one of similar volume.
/// `tick` is called between its kernel calls (the import's progress, for
/// its watchdog).
pub fn compare_bodies<K: Kernel>(
    doc: &Document<K>,
    finals: &[(StoredBody<K::Shape>, Sig)],
    compare: bool,
    tick: &dyn Fn(),
) -> Vec<BodyReport> {
    let kernel = doc.kernel();
    let bodies: Vec<_> = doc
        .bodies()
        .into_iter()
        .filter_map(|b| {
            Some((
                b.uid,
                b.name.clone(),
                b.shape.clone(),
                Sig::of(kernel, b.shape)?,
            ))
        })
        .collect();
    let mut used = vec![false; bodies.len()];
    let mut out = Vec::new();
    // Exact matches first, so a near one does not take another's body.
    let mut pairs: Vec<Option<usize>> = finals
        .iter()
        .map(|(_, sig)| {
            let i = (0..bodies.len()).find(|&i| !used[i] && bodies[i].3.matches(sig))?;
            used[i] = true;
            Some(i)
        })
        .collect();
    for (k, (_, sig)) in finals.iter().enumerate() {
        if pairs[k].is_some() {
            continue;
        }
        let near = (0..bodies.len())
            .filter(|&i| !used[i])
            .filter(|&i| {
                let r = bodies[i].3.volume / sig.volume;
                (0.5..2.0).contains(&r)
            })
            .min_by(|&a, &b| {
                let d = |i: usize| geom::distance(bodies[i].3.center, sig.center);
                d(a).total_cmp(&d(b))
            });
        if let Some(i) = near {
            used[i] = true;
            pairs[k] = Some(i);
        }
    }
    // The boolean differences are slow on large bodies: the first bodies
    // of moderate size only, for a minute at most, the others by volume.
    let mut compared = 0;
    let started = std::time::Instant::now();
    for ((body, sig), pair) in finals.iter().zip(pairs) {
        tick();
        let file = body.name.clone().unwrap_or_else(|| body.source.clone());
        let mut report = BodyReport {
            file,
            file_volume: sig.volume,
            mitcad: None,
            mitcad_volume: None,
            volume_difference: None,
            max_deviation: None,
            relative_difference: None,
        };
        if let Some(i) = pair {
            let (uid, name, shape, mine) = &bodies[i];
            report.mitcad = Some(format!("{uid} {name}"));
            report.mitcad_volume = Some(mine.volume);
            report.volume_difference = Some((mine.volume - sig.volume) / sig.volume);
            let small = |s: &K::Shape| kernel.faces(s).is_ok_and(|f| f.len() <= 300);
            // A body with the same measures (to 1e-4) is not compared surface
            // by surface: the boolean differences of coincident shapes can
            // take minutes (a fallback's body is the file's own), and of nearly
            // coincident ones may never return (a corpus design: 192
            // against 209 faces, the volumes 7e-5 apart).
            let identical = mine.distance(sig) <= 1e-4;
            let in_time = started.elapsed().as_secs_f64() < COMPARE_SECONDS;
            if compare
                && !identical
                && in_time
                && compared < 25
                && small(shape)
                && small(&body.shape)
            {
                compared += 1;
                // Each for ten seconds at most, within what is left of the
                // minute: one boolean of nearly coincident bodies alone can
                // take many minutes. The deviations come first (about a
                // second); the booleans get the rest of the time.
                let left = COMPARE_SECONDS - started.elapsed().as_secs_f64();
                let options = CompareOptions {
                    samples: 600,
                    seconds: Some(left.clamp(1.0, BODY_COMPARE_SECONDS)),
                    booleans_above: BOOLEANS_ABOVE,
                    ..CompareOptions::default()
                };
                if crate::tracing() {
                    eprintln!(
                        "import: comparing {} ({} faces) with {} {} ({} faces), volumes {:.6} / {:.6}",
                        report.file, sig.faces, uid, name, mine.faces, sig.volume, mine.volume
                    );
                }
                tick();
                let clock = std::time::Instant::now();
                if let Ok(c) = kernel.compare_shapes(&[shape], &[&body.shape], &options) {
                    // (No samples measured in time: no deviation.)
                    if c.a_to_b.samples + c.b_to_a.samples > 0 {
                        report.max_deviation = Some(c.max_deviation);
                    }
                    report.relative_difference = c.relative_difference;
                }
                if crate::tracing() {
                    eprintln!("import: compared in {:.2} s", clock.elapsed().as_secs_f64());
                }
            }
        }
        out.push(report);
    }
    out
}

// Fillet and chamfer edges found by their names (mitcad#96): the file's
// edge in the replay where its middle point does not tell it.

/// The replay's edges of every body: body, name, middle, unit tangent
/// there and length ([`dressup_edge_match`]).
pub struct ReplayEdges(Vec<(BodyUid, EdgeName, Vec3, Vec3, f64)>);

impl ReplayEdges {
    pub fn of<K: Kernel>(doc: &Document<K>) -> Self {
        let kernel = doc.kernel();
        let mut out = Vec::new();
        for body in doc.bodies() {
            crate::tick();
            for (name, mid, tangent, length) in edge_middles(kernel, body.shape) {
                out.push((body.uid, name, mid, tangent, length));
            }
        }
        ReplayEdges(out)
    }
}

/// A replay edge found for a file's edge, with the file edge's own
/// direction at the replay edge's middle where the file gives it
/// (`_f3d.direction`).
#[derive(Debug, Clone)]
pub struct FoundEdge {
    pub body: BodyUid,
    pub name: EdgeName,
    pub mid: Vec3,
    pub direction: Option<Vec3>,
}

/// Why a decoded dressup edge cannot be used by name (mitcad#106).
/// Decoder failures are separate from edges found in ASM but absent or
/// ambiguous in the replay; none of these justifies choosing an edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DressupEdgeFailure {
    DecoderUnresolved,
    MissingFingerprint,
    MissingInReplay,
    AmbiguousInReplay(Vec<(BodyUid, EdgeName)>),
}

impl DressupEdgeFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DecoderUnresolved => "decoder_unresolved",
            Self::MissingFingerprint => "missing_fingerprint",
            Self::MissingInReplay => "missing_in_replay",
            Self::AmbiguousInReplay(_) => "ambiguous_in_replay",
        }
    }
}

impl std::fmt::Display for DressupEdgeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code())?;
        if let Self::AmbiguousInReplay(edges) = self {
            for (i, (body, name)) in edges.iter().take(MAX_REPORTED_EDGE_MATCHES).enumerate() {
                write!(f, "{} {body}/{name}", if i == 0 { ":" } else { "," })?;
            }
            if edges.len() > MAX_REPORTED_EDGE_MATCHES {
                write!(f, ", ... ({} candidates total)", edges.len())?;
            }
        }
        Ok(())
    }
}

/// Keep production failure notes bounded independently of the full
/// structured ambiguity retained for learning and diagnostics.
const MAX_REPORTED_EDGE_MATCHES: usize = 16;

/// How far a replay edge's middle may be from the file's open edge's
/// (mm): the file gives B-spline and ellipse edges' middles at the middle
/// of their parameter range, 0.001 to 0.05 mm from the replay's.
const MID_OFF: f64 = 0.05;
/// How far a point may be from the file's circle (mm).
const ON_CURVE: f64 = 1e-3;
/// Midpoint-plus-length scores within this many mm are tied. This is
/// much smaller than the matching tolerances; ties cannot establish
/// which of two equally fitting edges of one body the input names.
const SCORE_TIE: f64 = 1e-9;

/// A file edge's curve as far as its fingerprint tells: a segment, a
/// circle (closed or an arc, with the plane's normal where known) or
/// neither.
enum FileCurve {
    Segment(Vec3, Vec3),
    Circle {
        center: Vec3,
        radius: f64,
        normal: Option<Vec3>,
    },
    Other,
}

impl FileCurve {
    fn of(a: Vec3, m: Vec3, b: Vec3, length: f64, closed: bool, direction: Option<Vec3>) -> Self {
        if closed {
            let center = geom::scale(geom::add(a, m), 0.5);
            let radius = geom::distance(a, m) / 2.0;
            if (std::f64::consts::TAU * radius - length).abs() > 1e-3 * length.max(1.0) {
                return FileCurve::Other;
            }
            let normal = direction.and_then(|d| geom::unit(geom::cross(geom::sub(m, center), d)));
            return FileCurve::Circle {
                center,
                radius,
                normal,
            };
        }
        let chord = geom::distance(a, b);
        if (chord - length).abs() <= 1e-6 * length.max(1.0) {
            return FileCurve::Segment(a, b);
        }
        // The circle through the three points.
        let (u, v) = (geom::sub(a, m), geom::sub(b, m));
        let n = geom::cross(u, v);
        let nn = geom::dot(n, n);
        if nn < 1e-18 {
            return FileCurve::Other;
        }
        let w = geom::add(
            geom::scale(geom::cross(v, n), geom::dot(u, u)),
            geom::scale(geom::cross(n, u), geom::dot(v, v)),
        );
        let center = geom::add(m, geom::scale(w, 0.5 / nn));
        let radius = geom::distance(center, m);
        let normal = geom::unit(n);
        // An arc of that circle as long as the edge.
        let angle = |p: Vec3, q: Vec3| {
            let (p, q) = (geom::sub(p, center), geom::sub(q, center));
            geom::norm(geom::cross(p, q)).atan2(geom::dot(p, q))
        };
        let arc = radius * (angle(a, m) + angle(m, b));
        if (arc - length).abs() > 1e-3 * length.max(1.0) {
            return FileCurve::Other;
        }
        FileCurve::Circle {
            center,
            radius,
            normal,
        }
    }

    /// Whether a replay edge's middle lies on the curve.
    fn holds(&self, p: Vec3, tangent: Vec3) -> bool {
        match *self {
            FileCurve::Segment(a, b) => {
                let ab = geom::sub(b, a);
                let t = geom::dot(geom::sub(p, a), ab) / geom::dot(ab, ab).max(1e-18);
                (0.0..=1.0).contains(&t)
                    && geom::distance(p, geom::add(a, geom::scale(ab, t))) <= ON_CURVE
            }
            FileCurve::Circle {
                center,
                radius,
                normal,
            } => {
                let r = geom::sub(p, center);
                // The replay edge's own plane when the file's is not known.
                let n = normal.or_else(|| geom::unit(geom::cross(r, tangent)));
                (geom::norm(r) - radius).abs() <= ON_CURVE
                    && n.is_some_and(|n| geom::dot(r, n).abs() <= ON_CURVE)
                    && geom::dot(r, tangent).abs() <= 1e-3 * radius.max(1.0)
            }
            FileCurve::Other => false,
        }
    }

    /// The file edge's direction at a point of it, from its direction at
    /// its middle `m` (a circle's turns about its normal).
    fn direction_at(&self, m: Vec3, d: Vec3, p: Vec3) -> Vec3 {
        match *self {
            FileCurve::Circle {
                center,
                normal: Some(n),
                ..
            } => {
                let axis = geom::cross(geom::sub(m, center), d);
                let n = if geom::dot(axis, n) < 0.0 {
                    geom::scale(n, -1.0)
                } else {
                    n
                };
                geom::unit(geom::cross(n, geom::sub(p, center))).unwrap_or(d)
            }
            _ => d,
        }
    }
}

/// A fillet's or chamfer's edge given by its fingerprint, in the replay
/// (mitcad#96). As [`resolve_edge`] (middle within 1e-3 mm, same length),
/// else:
/// - a closed edge: the edge of its length on its circle (the file's
///   middle is half way along the edge's parameter, opposite its seam;
///   the replay's seam is elsewhere), when only one is;
/// - an open edge whose middle is up to [`MID_OFF`] off: the nearest of
///   its length;
/// - two edges on its segment or circle whose lengths add up to its (the
///   replay splits it where a sketch had a point), when only one pair is.
///
/// Edges of any body. Tied midpoint scores within one body are refused;
/// as [`resolve_edge`], coincident copies in different bodies keep the
/// first body of the best match. Recipe tails do not decide ties until
/// their semantics have been proved against the learning dump.
pub fn dressup_edge_match(
    edges: &ReplayEdges,
    fp: &Fingerprint,
) -> Result<Vec<FoundEdge>, DressupEdgeFailure> {
    let (Some(m), Some(length)) = (fp.mid_point, fp.length) else {
        return Err(if fp.f3d.is_some() {
            DressupEdgeFailure::DecoderUnresolved
        } else {
            DressupEdgeFailure::MissingFingerprint
        });
    };
    let (m, length) = (mm3(m), mm(length));
    let a = fp.start_point.flatten().map(mm3);
    let b = fp.end_point.flatten().map(mm3);
    let direction = fp
        .f3d
        .as_ref()
        .and_then(|f| f.direction)
        .and_then(geom::unit);
    let same_length = |len: f64| (len - length).abs() <= 1e-3 * len.max(1.0);
    let found = |k: usize, direction: Option<Vec3>| {
        let (body, name, mid, _, _) = &edges.0[k];
        FoundEdge {
            body: *body,
            name: name.clone(),
            mid: *mid,
            direction,
        }
    };
    let ambiguous = |indices: Vec<usize>| {
        DressupEdgeFailure::AmbiguousInReplay(
            indices
                .into_iter()
                .map(|k| (edges.0[k].0, edges.0[k].1.clone()))
                .collect(),
        )
    };
    // A kernel may expose an edge's same name more than once. Such
    // aliases do not represent different selectable replay edges.
    let unique = |indices: Vec<usize>| {
        let mut seen = std::collections::HashSet::new();
        indices
            .into_iter()
            .filter(|&k| seen.insert((edges.0[k].0, edges.0[k].1.clone())))
            .collect::<Vec<_>>()
    };
    let nearest = |within: f64| -> Result<Option<usize>, DressupEdgeFailure> {
        let candidates: Vec<usize> = (0..edges.0.len())
            .filter(|&k| {
                let (_, _, p, _, len) = &edges.0[k];
                same_length(*len) && geom::distance(*p, m) <= within
            })
            .collect();
        let score = |k: usize| {
            let (_, _, p, _, len) = &edges.0[k];
            geom::distance(*p, m) + (len - length).abs()
        };
        let Some(&best) = candidates
            .iter()
            .min_by(|&&i, &&j| score(i).total_cmp(&score(j)))
        else {
            return Ok(None);
        };
        let tied = unique(
            candidates
                .into_iter()
                .filter(|&k| {
                    edges.0[k].0 == edges.0[best].0 && (score(k) - score(best)).abs() <= SCORE_TIE
                })
                .collect(),
        );
        if tied.len() > 1 {
            Err(ambiguous(tied))
        } else {
            Ok(Some(best))
        }
    };
    if let Some(k) = nearest(1e-3)? {
        return Ok(vec![found(k, direction)]);
    }
    let (Some(a), Some(b)) = (a, b) else {
        return Err(DressupEdgeFailure::MissingInReplay);
    };
    let closed = geom::distance(a, b) <= 1e-6 * length.max(1.0);
    let curve = FileCurve::of(a, m, b, length, closed, direction);
    let on = |k: usize| {
        let (_, _, p, t, _) = &edges.0[k];
        curve.holds(*p, *t)
    };
    let along = |k: usize| direction.map(|d| curve.direction_at(m, d, edges.0[k].2));
    if closed {
        let fits = unique(
            (0..edges.0.len())
                .filter(|&k| same_length(edges.0[k].4) && on(k))
                .collect(),
        );
        if let [k] = fits[..] {
            return Ok(vec![found(k, along(k))]);
        } else if fits.len() > 1 {
            return Err(ambiguous(fits));
        }
    } else if let Some(k) = nearest(MID_OFF)? {
        return Ok(vec![found(k, direction)]);
    }
    // Two pieces of one body.
    let pieces = unique(
        (0..edges.0.len())
            .filter(|&k| edges.0[k].4 < length && on(k))
            .collect(),
    );
    let mut pairs = Vec::new();
    for (x, &i) in pieces.iter().enumerate() {
        for &j in &pieces[x + 1..] {
            let (bi, bj) = (&edges.0[i].0, &edges.0[j].0);
            if bi == bj && (edges.0[i].4 + edges.0[j].4 - length).abs() <= 1e-3 * length.max(1.0) {
                pairs.push((i, j));
            }
        }
    }
    match pairs[..] {
        [(i, j)] => Ok(vec![found(i, along(i)), found(j, along(j))]),
        [] => Err(DressupEdgeFailure::MissingInReplay),
        _ => {
            let mut indices: Vec<usize> = pairs.into_iter().flat_map(|(i, j)| [i, j]).collect();
            indices.sort_unstable();
            indices.dedup();
            Err(ambiguous(indices))
        }
    }
}

/// Whether the first face of a replay edge's name is the face on the
/// left of the file edge's direction at the edge's middle (seen against
/// the face's outward normal): a point just off the edge that way, in the
/// face's tangent plane, lies on that face and not on the other. `None`
/// when the faces do not tell.
pub fn first_face_on_left<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    edge: &FoundEdge,
) -> Option<bool> {
    let t = edge.direction?;
    let [a, b] = edge.name.faces();
    let eps = 1e-2;
    let left = |face: &FaceName| -> Option<bool> {
        let n = geom::unit(kernel.face_point_normal(shape, face, edge.mid).ok()?.normal)?;
        let u = geom::unit(geom::cross(n, t))?;
        let q = geom::add(edge.mid, geom::scale(u, eps));
        let near = kernel.face_point_normal(shape, face, q).ok()?;
        Some(geom::distance(near.point, q) < eps / 2.0)
    };
    match (left(a)?, left(b)?) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod dressup_tests {
    use super::*;
    use mitcad_f3d::design::ir::FingerprintF3d;

    fn edges(list: &[(&str, &str, Vec3, Vec3, f64)]) -> ReplayEdges {
        ReplayEdges(
            list.iter()
                .map(|(b, e, p, t, l)| (b.parse().unwrap(), e.parse().unwrap(), *p, *t, *l))
                .collect(),
        )
    }

    /// A fingerprint in cm from mm values.
    fn fp(mid: Vec3, length: f64, start: Vec3, end: Vec3, direction: Option<Vec3>) -> Fingerprint {
        let cm = |p: Vec3| p.map(|x| x / 10.0);
        Fingerprint {
            mid_point: Some(cm(mid)),
            length: Some(length / 10.0),
            start_point: Some(Some(cm(start))),
            end_point: Some(Some(cm(end))),
            f3d: Some(FingerprintF3d {
                direction,
                ..FingerprintF3d::default()
            }),
            ..Fingerprint::default()
        }
    }

    #[test]
    fn tied_midpoint_scores_do_not_choose_an_edge_of_one_body() {
        let f = fp([0.0; 3], 10.0, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], None);
        for off in [0.0001, 0.02] {
            // Exercise both the tight and relaxed midpoint tiers. A tiny
            // numeric difference does not make these equally likely edges
            // distinguishable, and reversing their order changes nothing.
            let mut list = [
                (
                    "F1.b0",
                    "E{F1:a|F1:b}",
                    [off, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    10.0,
                ),
                (
                    "F1.b0",
                    "E{F1:a|F1:c}",
                    [-off - SCORE_TIE / 2.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    10.0,
                ),
            ];
            for _ in 0..2 {
                let error = dressup_edge_match(&edges(&list), &f).unwrap_err();
                assert!(matches!(&error, DressupEdgeFailure::AmbiguousInReplay(e) if e.len() == 2));
                assert_eq!(error.code(), "ambiguous_in_replay");
                list.reverse();
            }
            list[1].2 = [off + SCORE_TIE * 2.0, 0.0, 0.0];
            assert!(dressup_edge_match(&edges(&list), &f).is_ok());
        }
    }

    #[test]
    fn coincident_copies_keep_the_first_body() {
        let f = fp([0.0; 3], 10.0, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], None);
        let replay = edges(&[
            ("F1.b0", "E{F1:a|F1:b}", [0.0; 3], [1.0, 0.0, 0.0], 10.0),
            ("F2.b0", "E{F2:a|F2:b}", [0.0; 3], [1.0, 0.0, 0.0], 10.0),
        ]);
        let matched = dressup_edge_match(&replay, &f).unwrap();
        assert_eq!(matched[0].body.to_string(), "F1.b0");
    }

    #[test]
    fn repeated_aliases_of_the_same_edge_are_one_match() {
        let f = fp([0.0; 3], 10.0, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], None);
        let edge = ("F1.b0", "E{F1:a|F1:b}", [0.0; 3], [1.0, 0.0, 0.0], 10.0);
        let matched = dressup_edge_match(&edges(&[edge, edge]), &f).unwrap();
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn failures_separate_decoder_and_replay() {
        let replay = edges(&[]);
        let mut f = Fingerprint::default();
        assert_eq!(
            dressup_edge_match(&replay, &f).unwrap_err(),
            DressupEdgeFailure::MissingFingerprint
        );
        f.f3d = Some(FingerprintF3d::default());
        assert_eq!(
            dressup_edge_match(&replay, &f).unwrap_err(),
            DressupEdgeFailure::DecoderUnresolved
        );
        f.mid_point = Some([0.0; 3]);
        f.length = Some(1.0);
        assert_eq!(
            dressup_edge_match(&replay, &f).unwrap_err(),
            DressupEdgeFailure::MissingInReplay
        );
    }

    #[test]
    fn ambiguity_notes_bound_names_and_keep_the_total_count() {
        let candidates = (0..MAX_REPORTED_EDGE_MATCHES + 3)
            .map(|i| {
                (
                    format!("F1.b{i}").parse().unwrap(),
                    "E{F1:a|F1:b}".parse().unwrap(),
                )
            })
            .collect();
        let error = DressupEdgeFailure::AmbiguousInReplay(candidates);
        let note = error.to_string();
        assert!(note.contains("F1.b15/"));
        assert!(!note.contains("F1.b16/"));
        assert!(note.contains("19 candidates total"));
        assert!(matches!(error, DressupEdgeFailure::AmbiguousInReplay(e) if e.len() == 19));
    }

    #[test]
    fn ambiguous_closed_and_split_edges_are_classified() {
        let circle = std::f64::consts::TAU * 5.0;
        let replay = edges(&[
            (
                "F1.b0",
                "E{F1:a|F1:b}",
                [0.0, 5.0, 0.0],
                [-1.0, 0.0, 0.0],
                circle,
            ),
            (
                "F1.b0",
                "E{F1:a|F1:c}",
                [0.0, -5.0, 0.0],
                [1.0, 0.0, 0.0],
                circle,
            ),
        ]);
        let f = fp(
            [-5.0, 0.0, 0.0],
            circle,
            [5.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            None,
        );
        assert!(matches!(
            dressup_edge_match(&replay, &f),
            Err(DressupEdgeFailure::AmbiguousInReplay(_))
        ));
        let replay = edges(&[
            (
                "F1.b0",
                "E{F1:a|F1:b}",
                [2.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                4.0,
            ),
            (
                "F1.b0",
                "E{F1:a|F1:c}",
                [7.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                6.0,
            ),
            (
                "F1.b0",
                "E{F1:a|F1:d}",
                [7.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                6.0,
            ),
        ]);
        let f = fp([5.0, 0.0, 0.0], 10.0, [0.0; 3], [10.0, 0.0, 0.0], None);
        assert!(matches!(
            dressup_edge_match(&replay, &f),
            Err(DressupEdgeFailure::AmbiguousInReplay(_))
        ));
    }

    #[test]
    fn closed_open_and_split_edges() {
        use std::f64::consts::{PI, TAU};
        let r = 5.0;
        let circle = TAU * r;
        let replay = edges(&[
            // A full circle of radius 5 about z at z = 0 whose middle is
            // at +y (its seam elsewhere than the file's).
            (
                "F1.b0",
                "E{F1:side|F1:end}",
                [0.0, r, 0.0],
                [-1.0, 0.0, 0.0],
                circle,
            ),
            // The same circle at z = 3.
            (
                "F1.b0",
                "E{F1:side|F1:start}",
                [0.0, r, 3.0],
                [-1.0, 0.0, 0.0],
                circle,
            ),
            // A line from (20, 0, 0) to (30, 0, 0) split at x = 24.
            (
                "F1.b0",
                "E{F1:a|F1:b}",
                [22.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                4.0,
            ),
            (
                "F1.b0",
                "E{F1:a|F1:c}",
                [27.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                6.0,
            ),
            // A spline-like edge whose middle is 0.02 mm off the file's.
            (
                "F1.b0",
                "E{F1:d|F1:e}",
                [40.02, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                7.5,
            ),
        ]);
        // The file's circle at z = 0 starts at +x, its middle at -x; it
        // runs counter-clockwise about z.
        let f = fp(
            [-r, 0.0, 0.0],
            circle,
            [r, 0.0, 0.0],
            [r, 0.0, 0.0],
            Some([0.0, -1.0, 0.0]),
        );
        let found = dressup_edge_match(&replay, &f).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.to_string(), "E{F1:end|F1:side}");
        // Its direction at the replay's middle (+y): -x.
        let d = found[0].direction.unwrap();
        assert!(geom::distance(d, [-1.0, 0.0, 0.0]) < 1e-9, "{d:?}");
        // The segment split in two.
        let f = fp(
            [25.0, 0.0, 0.0],
            10.0,
            [20.0, 0.0, 0.0],
            [30.0, 0.0, 0.0],
            None,
        );
        let found = dressup_edge_match(&replay, &f).unwrap();
        assert_eq!(found.len(), 2);
        // An open edge 0.02 mm off, of the same length.
        let f = fp(
            [40.0, 0.0, 0.0],
            7.5,
            [38.0, -2.0, 0.0],
            [38.0, 2.0, 0.0],
            None,
        );
        let found = dressup_edge_match(&replay, &f).unwrap();
        assert_eq!(found[0].name.to_string(), "E{F1:d|F1:e}");
        // A circle the replay does not have.
        let f = fp(
            [-r, 0.0, 9.0],
            circle,
            [r, 0.0, 9.0],
            [r, 0.0, 9.0],
            Some([0.0, -1.0, 0.0]),
        );
        assert!(dressup_edge_match(&replay, &f).is_err());
        // A half circle (an open arc) is matched by its middle only.
        let f = fp([0.0, r, 0.0], PI * r, [r, 0.0, 0.0], [-r, 0.0, 0.0], None);
        assert!(dressup_edge_match(&replay, &f).is_err());
    }
}
