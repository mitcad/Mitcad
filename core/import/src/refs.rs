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

/// Every planar face of the bodies at the marker with its plane (outward
/// normal).
pub fn planar_faces<K: Kernel>(doc: &Document<K>) -> Vec<(BodyUid, FaceName, Plane)> {
    let kernel = doc.kernel();
    let mut out = Vec::new();
    for body in doc.bodies() {
        let Ok(faces) = kernel.faces(body.shape) else {
            continue;
        };
        for face in faces.iter().filter(|f| f.surface == "plane") {
            let Some(name) = face.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
                continue;
            };
            if let Ok(plane) = kernel.face_plane(body.shape, &name) {
                out.push((body.uid, name, plane));
            }
        }
    }
    out
}

/// A body by its fingerprint: volume, area and box (the replay should hold
/// the same body before the feature that refers to it).
pub fn resolve_body<K: Kernel>(doc: &Document<K>, fp: &Fingerprint) -> Option<BodyUid> {
    let volume = fp.volume? * 1000.0;
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
        let points: Vec<Vec3> = probes.iter().map(|(_, p)| *p).collect();
        let inside = kernel
            .points_inside(shape, &points)
            .map_err(|e| e.to_string())?;
        for ((i, _), inside) in probes.into_iter().zip(inside) {
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
            && own.iter().all(|d| *d > ON_FACE)
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
    for body in shapes {
        let d = kernel
            .boundary_distances(body, points)
            .map_err(|e| e.to_string())?;
        for (n, d) in nearest.iter_mut().zip(d) {
            *n = n.min(d);
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

/// The file's stored bodies against the replay: each matched to the Mitcad
/// body with the same signature, else the nearest one of similar volume.
/// `tick` is called before each geometric comparison (the import's
/// progress, for its watchdog).
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
                let options = CompareOptions {
                    samples: 600,
                    ..CompareOptions::default()
                };
                if crate::tracing() {
                    eprintln!(
                        "import: comparing {} ({} faces) with {} {} ({} faces), volumes {:.6} / {:.6}",
                        report.file, sig.faces, uid, name, mine.faces, sig.volume, mine.volume
                    );
                }
                tick();
                if let Ok(c) = kernel.compare_shapes(&[shape], &[&body.shape], &options) {
                    report.max_deviation = Some(c.max_deviation);
                    report.relative_difference = c.relative_difference;
                }
            }
        }
        out.push(report);
    }
    out
}
