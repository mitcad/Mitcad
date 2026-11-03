// SPDX-License-Identifier: MIT
//! References resolved geometrically against the replayed bodies: the file's
//! B-rep fingerprints (bodies, faces, edges) as Mitcad names, planar faces
//! for sketch and construction planes, the edges a fillet or chamfer
//! consumed (from the history state after it), and the final comparison.

use mitcad_dump::{Fingerprint, Vec3};
use mitcad_model::analysis::CompareOptions;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::{BodyUid, ComponentUid, Document, EdgeName, FaceName, Kernel, VertexName};

use crate::geom::{self, mm, mm3};
use crate::history::{Sig, StoredBody};
use crate::report::BodyReport;
use std::sync::Arc;

// Planar faces for sketch and construction planes, and the planar faces and
// edges kept by body version, shared with the FreeCAD import
// (`mitcad-import-base`).
pub use mitcad_import_base::planes::KeptPlanes;
pub use mitcad_import_base::planes::planar_faces;
pub(crate) use mitcad_import_base::planes::{BodyEdge, edges_of, release_kept_edges};

/// The bodies a fingerprint's entity is looked for among: those of the
/// other document the file names it in (`_f3d.document`) where the import
/// has that document's components filled (mitcad#196), else the bodies at
/// the marker (outside those components).
pub(crate) fn bodies_for<'d, K: Kernel>(
    doc: &'d Document<K>,
    fp: &Fingerprint,
) -> Vec<mitcad_model::BodyView<'d, K::Shape>> {
    // The components of the occurrences the file's reference is in, where
    // the import knows them (a document placed more than once has a copy
    // per arrangement, and the reference is in one of them).
    if let Some(components) = fp
        .f3d
        .as_ref()
        .and_then(|f| f.object_id)
        .and_then(|o| doc.import_focus(o))
    {
        return crate::body_scope::filter(doc, doc.bodies_of_components(components));
    }
    match fp.f3d.as_ref().and_then(|f| f.document.as_deref()) {
        Some(guid) if !doc.import_scope(guid).is_empty() => {
            crate::body_scope::filter(doc, doc.scope_bodies(guid))
        }
        _ => crate::body_scope::bodies(doc),
    }
}

// The geometric tolerance of the replay's lookups (mitcad#224).

/// How far (mm) the replay's geometry may be from the file's for a match
/// the matchers take as exact: their first pass.
pub(crate) const EXACT: f64 = 1e-3;

// The default of `Options::geometric_tolerance` (mm), shared with the
// `.rvt` import (`mitcad-import-base`).
pub use mitcad_import_base::DEFAULT_GEOMETRIC_TOLERANCE;

std::thread_local! {
    /// The import's geometric tolerance on this thread
    /// ([`GeometricTolerance`]).
    static GEOMETRIC: std::cell::Cell<f64> =
        const { std::cell::Cell::new(DEFAULT_GEOMETRIC_TOLERANCE) };
    /// The largest deviation beyond [`EXACT`] a match took since it was last
    /// taken ([`take_deviation`]).
    static DEVIATION: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
}

/// The distance (mm) within which the replay's geometry is taken as the
/// file's where no exact match is (`Options::geometric_tolerance`, set for
/// the import's thread by [`GeometricTolerance`]): the second pass of the
/// lookups of decoded edges, faces and vertices ([`dressup_edge_match`],
/// [`resolve_edge_where`], [`resolve_face_where`], [`resolve_vertex`]) and
/// the face distance of the geometric check (`geometric.rs`). The replay
/// differs from the file by the kernels' noise: an earlier feature built
/// 1 µm otherwise moves every later edge on it as far.
pub(crate) fn geometric_tolerance() -> f64 {
    GEOMETRIC.with(std::cell::Cell::get)
}

/// The wider tolerance of the matchers' second pass; None when it is not
/// wider than [`EXACT`].
fn widened() -> Option<f64> {
    let tolerance = geometric_tolerance();
    (tolerance > EXACT).then_some(tolerance)
}

/// Notes the deviation of a match that needed more than [`EXACT`] (or of
/// a sketch's solved points, `features.rs`).
pub(crate) fn note_deviation(distance: f64) {
    DEVIATION.with(|d| d.set(Some(d.get().map_or(distance, |o| o.max(distance)))));
}

/// The largest deviation beyond [`EXACT`] the matches on this thread took
/// since the last call (mm), for the item's report; None when none did.
pub(crate) fn take_deviation() -> Option<f64> {
    DEVIATION.with(|d| d.take())
}

/// Sets the geometric tolerance for this thread until dropped (the import
/// and its workers; an import inside an import keeps the outer one's once
/// it ends).
pub(crate) struct GeometricTolerance(f64);

impl GeometricTolerance {
    pub(crate) fn set(tolerance: f64) -> Self {
        let tolerance = if tolerance.is_finite() && tolerance > 0.0 {
            tolerance
        } else {
            DEFAULT_GEOMETRIC_TOLERANCE
        };
        GeometricTolerance(GEOMETRIC.with(|g| g.replace(tolerance)))
    }
}

impl Drop for GeometricTolerance {
    fn drop(&mut self) {
        GEOMETRIC.with(|g| g.set(self.0));
    }
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
    for body in bodies_for(doc, fp) {
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
    let fits: Vec<BodyUid> = bodies_for(doc, fp)
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
        [] => {
            let found = resolve_body_by_boundary(doc, fp, &points);
            if found.is_none() && crate::tracing() {
                eprintln!(
                    "import: a body input ({} edge points) has no body with those edges or surfaces among {} bodies",
                    points.len(),
                    bodies_for(doc, fp).len()
                );
            }
            found
        }
        _ => {
            if crate::tracing() {
                eprintln!(
                    "import: a body input ({} edge points) fits {} bodies: {fits:?}",
                    points.len(),
                    fits.len()
                );
            }
            None
        }
    }
}

/// The only body whose faces pass through each of the points: a body a
/// replayed feature made (a sweep, a pipe) has the file's surfaces but not
/// necessarily its edges (seams and splits elsewhere).
fn resolve_body_by_boundary<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
    points: &[Vec3],
) -> Option<BodyUid> {
    let kernel = doc.kernel();
    let fits: Vec<BodyUid> = bodies_for(doc, fp)
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
pub(crate) fn resolve_cylinder_through<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
) -> Option<(BodyUid, FaceName)> {
    let points = [mm3(fp.start_point.flatten()?), mm3(fp.end_point.flatten()?)];
    let kernel = doc.kernel();
    const TOLERANCE: f64 = 1e-3;
    let mut best: Option<((bool, f64), BodyUid, FaceName)> = None;
    for body in bodies_for(doc, fp) {
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
pub(crate) fn resolve_cylinder_surface<K: Kernel>(
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
    for body in bodies_for(doc, fp) {
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
    resolve_face_where(doc, fp, &|_| true)
}

/// A planar face by its plane: a face of the replay in the plane of the
/// fingerprint's (`geometry`), one facing the same way before one facing
/// the other. For a plane definition that needs only the face's plane,
/// where the fingerprint's point lies on a part of the face that the
/// replay's bodies no longer have (the decoder found the face in an
/// earlier history state than the item's).
pub(crate) fn resolve_face_by_plane<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
) -> Option<(BodyUid, FaceName)> {
    let g = fp.geometry.as_ref()?;
    if g.geometry_type.as_deref() != Some("Plane") {
        return None;
    }
    let origin = mm3(g.origin?);
    let normal = geom::unit(g.normal?)?;
    let mut best: Option<(bool, BodyUid, FaceName)> = None;
    for (body, face, plane) in planar_faces(doc) {
        let parallel = geom::norm(geom::cross(plane.normal, normal)) < 1e-6;
        if !parallel || geom::dot(geom::sub(origin, plane.origin), normal).abs() > 1e-4 {
            continue;
        }
        let opposed = geom::dot(plane.normal, normal) < 0.0;
        if best.as_ref().is_none_or(|(o, _, _)| *o && !opposed) {
            best = Some((opposed, body, face));
        }
    }
    best.map(|(_, b, f)| (b, f))
}

// Faces passed over by their boxes (mitcad#119).

/// The boxes of a body's `count` faces ([`Kernel::face_boxes`]); None when
/// the kernel gives none (or not one per face): every face is measured.
fn face_boxes<K: Kernel>(kernel: &K, shape: &K::Shape, count: usize) -> Option<Vec<[Vec3; 2]>> {
    kernel.face_boxes(shape).ok().filter(|b| b.len() == count)
}

/// Whether face `k`'s box lies farther than `within` from `point`, so the
/// face does too: the lookups through a point measured the point's distance
/// from every face of a body (a body of 23 000 faces took a tenth of a
/// large design's replay).
fn beyond(boxes: &Option<Vec<[Vec3; 2]>>, k: usize, point: Vec3, within: f64) -> bool {
    let Some([low, high]) = boxes.as_ref().and_then(|b| b.get(k)) else {
        return false;
    };
    let gap = |i: usize| (low[i] - point[i]).max(point[i] - high[i]).max(0.0);
    (0..3).map(|i| gap(i).powi(2)).sum::<f64>().sqrt() > within
}

/// [`resolve_face`] among the bodies `keep` keeps (mitcad#161: those of
/// the component the decoder found the face in): through the point within
/// [`EXACT`], else within the [`geometric_tolerance`].
pub fn resolve_face_where<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
    keep: &dyn Fn(BodyUid) -> bool,
) -> Option<(BodyUid, FaceName)> {
    let point = mm3(fp.point_on_face?);
    let area = fp.area.map(|a| a * 100.0);
    let surface = fp
        .geometry
        .as_ref()
        .and_then(|g| g.geometry_type.as_deref())
        .map(surface_kind);
    let wide = widened().unwrap_or(EXACT);
    let kernel = doc.kernel();
    // The best within EXACT and the best within the wider tolerance:
    // (score, distance, body, face).
    let mut best: [Option<(f64, f64, BodyUid, FaceName)>; 2] = [None, None];
    for body in bodies_for(doc, fp) {
        if !keep(body.uid) {
            continue;
        }
        crate::tick();
        let Ok(faces) = kernel.faces(body.shape) else {
            continue;
        };
        let boxes = face_boxes(kernel, body.shape, faces.len());
        for (k, face) in faces.into_iter().enumerate() {
            if surface.is_some_and(|s| !same_surface_kind(s, &face.surface)) {
                continue;
            }
            if beyond(&boxes, k, point, wide) {
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
            if d > wide {
                continue;
            }
            let da = area.map_or(0.0, |a| (face.area - a).abs() / a.max(1e-9));
            let score = d + da;
            for (pass, within) in [EXACT, wide].into_iter().enumerate() {
                if d <= within && best[pass].as_ref().is_none_or(|(s, ..)| score < *s) {
                    best[pass] = Some((score, d, body.uid, name.clone()));
                }
            }
        }
    }
    let [exact, wider] = best;
    if let Some((_, _, b, f)) = exact {
        return Some((b, f));
    }
    let (_, d, b, f) = wider?;
    note_deviation(d);
    Some((b, f))
}

/// Whether the face [`resolve_face`] gives for a fingerprint decoded from
/// the file's own names (`_f3d`, with the point found in the history) is
/// the only face of the replay that fits it: no other face of the surface
/// type passes through the point and faces the same way (a face of
/// another body or occurrence lying on it, a copy). The caller names the
/// face found.
pub fn face_resolved_uniquely<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
    found: (BodyUid, &FaceName),
) -> bool {
    let (Some(f3d), Some(point)) = (fp.f3d.as_ref(), fp.point_on_face) else {
        return false;
    };
    if f3d.entities.as_ref().is_none_or(Vec::is_empty) {
        return false;
    }
    let point = mm3(point);
    let facing = fp.normal_at_point.and_then(geom::unit);
    let surface = fp
        .geometry
        .as_ref()
        .and_then(|g| g.geometry_type.as_deref())
        .map(surface_kind);
    let wide = widened().unwrap_or(EXACT);
    let kernel = doc.kernel();
    // The faces within EXACT, and within the wider tolerance (where none is
    // within EXACT, the face was found by the wider one).
    let mut fits: [Vec<(BodyUid, FaceName)>; 2] = [Vec::new(), Vec::new()];
    for body in bodies_for(doc, fp) {
        crate::tick();
        let Ok(faces) = kernel.faces(body.shape) else {
            return false;
        };
        let boxes = face_boxes(kernel, body.shape, faces.len());
        for (k, face) in faces.into_iter().enumerate() {
            if surface.is_some_and(|s| !same_surface_kind(s, &face.surface)) {
                continue;
            }
            if beyond(&boxes, k, point, wide) {
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
            if d > wide {
                continue;
            }
            if facing.is_some_and(|n| geom::dot(n, near.normal) < 0.0) {
                continue;
            }
            for (pass, within) in [EXACT, wide].into_iter().enumerate() {
                if d <= within && !fits[pass].contains(&(body.uid, name.clone())) {
                    fits[pass].push((body.uid, name.clone()));
                }
            }
        }
    }
    let fits = if fits[0].is_empty() {
        &fits[1]
    } else {
        &fits[0]
    };
    matches!(fits.as_slice(), [(b, f)] if *b == found.0 && f == found.1)
}

/// Whether a face of the replay's surface type `kind` can be a face of
/// the dump's type `wanted` ([`surface_kind`]): the same type, or any
/// free-form surface for a free-form one (mitcad#161: the file's spline
/// or ruled thread flank is the replay's swept surface).
fn same_surface_kind(wanted: &str, kind: &str) -> bool {
    const FREE_FORM: [&str; 6] = [
        "bspline",
        "bezier",
        "revolution",
        "extrusion",
        "offset",
        "other",
    ];
    wanted == kind || (FREE_FORM.contains(&wanted) && FREE_FORM.contains(&kind))
}

// Faces and edges the replay splits (mitcad#161).

/// What [`face_pieces`] reads of a body's faces and edges, kept for many
/// lookups on one body (mitcad#311: a design's appearances name hundreds
/// of faces of one body, and listing its faces and edges and reading their
/// names for each took most of the time of applying them). Valid while
/// the body's shape does not change.
pub struct PieceIndex {
    /// Each face's surface type and first name (none where it does not
    /// read).
    faces: Vec<(String, Option<FaceName>)>,
    boxes: Option<Vec<[Vec3; 2]>>,
    /// The edges whose names read, with their middles.
    middles: Vec<(EdgeName, Vec3)>,
}

impl PieceIndex {
    pub fn new<K: Kernel>(doc: &Document<K>, body: BodyUid) -> Option<Self> {
        let shape = crate::body_scope::shape(doc, body)?;
        let kernel = doc.kernel();
        let faces = kernel.faces(shape).ok()?;
        let boxes = face_boxes(kernel, shape, faces.len());
        let faces = faces
            .into_iter()
            .map(|f| {
                let name = f.names.first().and_then(|n| n.parse::<FaceName>().ok());
                (f.surface, name)
            })
            .collect();
        let middles = kernel
            .edge_middles(shape)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|m| Some((m.name.parse::<EdgeName>().ok()?, m.point)))
            .collect();
        Some(PieceIndex {
            faces,
            boxes,
            middles,
        })
    }
}

/// Every face of `body` that a face fingerprint covers: those of its
/// surface type through `point_on_face` or one of `_f3d.face_points`
/// (the replay may split a face of the file, a thread's flank one face
/// per turn), each name once; the first through `point_on_face`.
pub(crate) fn face_pieces<K: Kernel>(
    doc: &Document<K>,
    body: BodyUid,
    fp: &Fingerprint,
) -> Vec<FaceName> {
    match PieceIndex::new(doc, body) {
        Some(index) => face_pieces_in(doc, body, fp, &index),
        None => Vec::new(),
    }
}

/// [`face_pieces`] with the body's [`PieceIndex`].
pub fn face_pieces_in<K: Kernel>(
    doc: &Document<K>,
    body: BodyUid,
    fp: &Fingerprint,
    index: &PieceIndex,
) -> Vec<FaceName> {
    let Some(shape) = crate::body_scope::shape(doc, body) else {
        return Vec::new();
    };
    let surface = fp
        .geometry
        .as_ref()
        .and_then(|g| g.geometry_type.as_deref())
        .map(surface_kind);
    let points: Vec<Vec3> = fp
        .point_on_face
        .into_iter()
        .chain(
            fp.f3d
                .as_ref()
                .and_then(|f| f.face_points.clone())
                .into_iter()
                .flatten(),
        )
        .map(mm3)
        .collect();
    let kernel = doc.kernel();
    // (position of the first point on it, name)
    let mut found: Vec<(usize, FaceName)> = Vec::new();
    let boxes = &index.boxes;
    for (k, (kind, name)) in index.faces.iter().enumerate() {
        if surface.is_some_and(|s| !same_surface_kind(s, kind)) {
            continue;
        }
        if points.iter().all(|&p| beyond(boxes, k, p, 1e-3)) {
            continue;
        }
        let Some(name) = name else {
            continue;
        };
        if found.iter().any(|(_, n)| n == name) {
            continue;
        }
        crate::tick();
        // (A point beyond the face's box is farther from it.)
        let on = points.iter().position(|&p| {
            !beyond(boxes, k, p, 1e-3)
                && kernel
                    .face_point_normal(shape, name, p)
                    .is_ok_and(|near| geom::distance(near.point, p) <= 1e-3)
        });
        if let Some(k) = on {
            found.push((k, name.clone()));
        }
    }
    found.sort_by_key(|(k, _)| *k);
    let mut pieces: Vec<FaceName> = found.into_iter().map(|(_, n)| n).collect();
    // The pieces between them that no point falls on: other pieces of the
    // same face of the replay (its name but for the piece numbers) that go
    // on smoothly across an edge from one found (a split face's pieces
    // meet without a crease).
    let same_face =
        |a: &FaceName, b: &FaceName| a.feature == b.feature && a.role == b.role && a.key == b.key;
    let mut grew = !pieces.is_empty();
    while grew {
        grew = false;
        for (edge, point) in &index.middles {
            let [a, b] = edge.faces();
            let (inner, outer) = match (pieces.contains(a), pieces.contains(b)) {
                (true, false) => (a, b),
                (false, true) => (b, a),
                _ => continue,
            };
            if !pieces.iter().any(|p| same_face(p, outer)) {
                continue;
            }
            crate::tick();
            let normal = |f: &FaceName| kernel.face_point_normal(shape, f, *point).ok();
            if let (Some(i), Some(o)) = (normal(inner), normal(outer))
                && geom::dot(i.normal, o.normal) > SMOOTH
            {
                pieces.push(outer.clone());
                grew = true;
            }
        }
    }
    pieces
}

/// An edge of the file that the replay splits into several (a thread's
/// helical edge, one edge per turn in the replay), by its fingerprint: the
/// connected edges of one body that `keep` keeps from `start_point` to
/// `end_point`, each going on smoothly from the one before (the file's
/// edge is one curve), whose lengths add up to `length` and whose middle
/// (half way along them) is `mid_point`, in order; within 1e-3 mm (the
/// length within 1e-3 of it). None for a closed edge, or where no chain or
/// several fit.
pub(crate) fn resolve_edge_chain<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
    keep: &dyn Fn(BodyUid) -> bool,
) -> Option<(BodyUid, Vec<EdgeName>)> {
    let (start, end) = (mm3(fp.start_point.flatten()?), mm3(fp.end_point.flatten()?));
    let (mid, length) = (mm3(fp.mid_point?), mm(fp.length?));
    let tolerance = 1e-3 * length.max(1.0);
    let near = |a: Vec3, b: Vec3| geom::distance(a, b) <= 1e-3;
    let kernel = doc.kernel();
    let mut found: Vec<(BodyUid, Vec<EdgeName>)> = Vec::new();
    for body in bodies_for(doc, fp) {
        if !keep(body.uid) {
            continue;
        }
        crate::tick();
        let Ok(curves) = kernel.edge_geometries(body.shape) else {
            continue;
        };
        let lengths: std::collections::HashMap<String, f64> = kernel
            .edge_middles(body.shape)
            .unwrap_or_default()
            .into_iter()
            .map(|m| (m.name, m.length))
            .collect();
        // Each open edge: its name, ends, length and the unit tangents at
        // its ends along it.
        let edges: Vec<(EdgeName, [Vec3; 2], f64, [Vec3; 2])> = curves
            .into_iter()
            .filter_map(|(name, curve)| {
                let name = name?;
                let ends = match curve {
                    mitcad_model::datum::CurveGeometry::Line { start, end }
                    | mitcad_model::datum::CurveGeometry::Circle { start, end, .. }
                    | mitcad_model::datum::CurveGeometry::Other { start, end, .. } => [start, end],
                };
                let length = *lengths.get(&name)?;
                if near(ends[0], ends[1]) {
                    return None;
                }
                let name: EdgeName = name.parse().ok()?;
                let one = std::slice::from_ref(&name);
                let at = |f: f64| {
                    kernel.path_point(
                        body.shape,
                        one,
                        mitcad_model::datum::PathParameter::Fraction(f),
                    )
                };
                let (a, b) = (at(0.0).ok()?, at(1.0).ok()?);
                // The path runs from the edge's start where it starts there.
                let ([a, b], ends) = if near(a.point, ends[0]) {
                    ([a.tangent, b.tangent], ends)
                } else {
                    ([b.tangent, a.tangent], [ends[1], ends[0]])
                };
                Some((name, ends, length, [a, b]))
            })
            .collect();
        // Depth first from the edges at `start`, along edges that go on
        // smoothly end to end, as long as the lengths stay within `length`.
        // (chain, the free end, the tangent going on there, the length)
        let going = |i: usize, k: usize| -> (Vec3, Vec3) {
            let (_, ends, _, tangents) = &edges[i];
            if k == 0 {
                (ends[1], tangents[1])
            } else {
                (ends[0], geom::scale(tangents[0], -1.0))
            }
        };
        let coming = |i: usize, k: usize| -> Vec3 {
            let tangents = &edges[i].3;
            if k == 0 {
                tangents[0]
            } else {
                geom::scale(tangents[1], -1.0)
            }
        };
        let mut stack: Vec<(Vec<usize>, Vec3, Vec3, f64)> = edges
            .iter()
            .enumerate()
            .filter_map(|(i, (_, ends, l, _))| {
                let k = (0..2).find(|&k| near(ends[k], start))?;
                let (at, tangent) = going(i, k);
                Some((vec![i], at, tangent, *l))
            })
            .collect();
        let mut steps = 0;
        while let Some((chain, at, tangent, sum)) = stack.pop() {
            steps += 1;
            if steps > CHAIN_STEPS {
                break;
            }
            if sum > length + tolerance {
                continue;
            }
            if chain.len() > 1 && near(at, end) && (sum - length).abs() <= tolerance {
                let names: Vec<EdgeName> = chain.iter().map(|&i| edges[i].0.clone()).collect();
                let middle = kernel.path_point(
                    body.shape,
                    &names,
                    mitcad_model::datum::PathParameter::Fraction(0.5),
                );
                if crate::tracing() {
                    eprintln!(
                        "import: edge chain of {} edges in {}, middle {:?}",
                        names.len(),
                        body.uid,
                        middle.as_ref().map(|m| m.point)
                    );
                }
                if middle.is_ok_and(|m| near(m.point, mid)) {
                    found.push((body.uid, names));
                }
                continue;
            }
            for (i, (_, ends, l, _)) in edges.iter().enumerate() {
                if chain.contains(&i) {
                    continue;
                }
                let Some(k) = (0..2).find(|&k| near(ends[k], at)) else {
                    continue;
                };
                if geom::dot(coming(i, k), tangent) < SMOOTH {
                    continue;
                }
                let mut next = chain.clone();
                next.push(i);
                let (at, tangent) = going(i, k);
                stack.push((next, at, tangent, sum + l));
            }
        }
    }
    match found.len() {
        1 => found.pop(),
        _ => None,
    }
}

/// How many partial chains [`resolve_edge_chain`] looks at, at most, in a
/// body.
const CHAIN_STEPS: usize = 20_000;

/// The cosine of the angle between the tangents of two edges of a chain
/// at the vertex they share, at least (0.1 degrees).
const SMOOTH: f64 = 0.999_998_5;

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
pub(crate) fn edge_midpoints<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
) -> Vec<(EdgeName, Vec3, f64)> {
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
    resolve_edge_where(doc, fp, &|_| true)
}

/// [`resolve_edge`] among the bodies `keep` keeps (mitcad#161): the middle
/// within [`EXACT`] (the length within 1e-3 of it), else within the
/// [`geometric_tolerance`] (the length within it or 1e-3 of it).
pub(crate) fn resolve_edge_where<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
    keep: &dyn Fn(BodyUid) -> bool,
) -> Option<(BodyUid, EdgeName)> {
    let mid = mm3(fp.mid_point?);
    let length = fp.length.map(mm);
    let wide = widened().unwrap_or(EXACT);
    let kernel = doc.kernel();
    // The best within EXACT and within the wider tolerance: (score,
    // deviation, body, edge).
    let mut best: [Option<(f64, f64, BodyUid, EdgeName)>; 2] = [None, None];
    for body in bodies_for(doc, fp) {
        if !keep(body.uid) {
            continue;
        }
        crate::tick();
        for (name, at, len) in edge_midpoints(kernel, body.shape) {
            let d = geom::distance(at, mid);
            let dl = length.map_or(0.0, |l| (len - l).abs());
            for (pass, within) in [EXACT, wide].into_iter().enumerate() {
                if d > within || dl > (1e-3 * len).max(within) {
                    continue;
                }
                if best[pass].as_ref().is_none_or(|(s, ..)| d + dl < *s) {
                    best[pass] = Some((d + dl, d.max(dl), body.uid, name.clone()));
                }
            }
        }
    }
    let [exact, wider] = best;
    if let Some((_, _, b, e)) = exact {
        return Some((b, e));
    }
    let (_, d, b, e) = wider?;
    note_deviation(d);
    Some((b, e))
}

/// Whether a point lies on an analytic surface (mm); `None` for another
/// surface type.
fn on_surface(surface: &SurfaceGeometry, p: Vec3, tolerance: f64) -> Option<bool> {
    let radial = |origin: Vec3, axis: Vec3| {
        let d = geom::sub(p, origin);
        let along = geom::dot(d, axis);
        (geom::norm(geom::sub(d, geom::scale(axis, along))), along)
    };
    Some(match surface {
        SurfaceGeometry::Plane { origin, normal } => {
            geom::dot(geom::sub(p, *origin), *normal).abs() < tolerance
        }
        SurfaceGeometry::Cylinder {
            origin,
            axis,
            radius,
        } => (radial(*origin, *axis).0 - radius).abs() < tolerance,
        SurfaceGeometry::Cone {
            origin,
            axis,
            radius,
            half_angle,
        } => {
            let (rho, along) = radial(*origin, *axis);
            ((rho - (radius + along * half_angle.tan())) * half_angle.cos()).abs() < tolerance
        }
        SurfaceGeometry::Sphere { center, radius } => {
            (geom::norm(geom::sub(p, *center)) - radius).abs() < tolerance
        }
        SurfaceGeometry::Torus {
            center,
            axis,
            major_radius,
            minor_radius,
        } => {
            let (rho, along) = radial(*center, *axis);
            ((rho - major_radius).hypot(along) - minor_radius).abs() < tolerance
        }
        SurfaceGeometry::Other { .. } => return None,
    })
}

/// A vertex by its fingerprint (mitcad#186): the vertex at `point` of a
/// body, named by the faces that meet there. The faces through the point
/// are found by their surfaces, then the largest group of them that names
/// a vertex where the point is (a plane through the point may belong to a
/// face elsewhere on the body). Within [`EXACT`], else within the [`geometric_tolerance`].
pub(crate) fn resolve_vertex<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
) -> Option<(BodyUid, VertexName)> {
    if let Some((found, _)) = resolve_vertex_within(doc, fp, EXACT) {
        return Some(found);
    }
    let (found, d) = resolve_vertex_within(doc, fp, widened()?)?;
    note_deviation(d);
    Some(found)
}

/// [`resolve_vertex`] within `tolerance`, with the vertex's distance.
fn resolve_vertex_within<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
    tolerance: f64,
) -> Option<((BodyUid, VertexName), f64)> {
    /// The most vertex names tried per body.
    const TRIES: usize = 400;
    let target = mm3(fp.point?);
    let kernel = doc.kernel();
    for body in bodies_for(doc, fp) {
        crate::tick();
        if let Ok(Some(b)) = kernel.enclosing_box(body.shape)
            && (0..3).any(|i| target[i] < b.min[i] - tolerance || target[i] > b.max[i] + tolerance)
        {
            continue;
        }
        let Ok(faces) = kernel.faces(body.shape) else {
            continue;
        };
        let on: Vec<FaceName> = faces
            .iter()
            .filter_map(|f| {
                let name: FaceName = f.names.first()?.parse().ok()?;
                let surface = kernel.face_geometry(body.shape, &name).ok()?;
                on_surface(&surface, target, tolerance)?.then_some(name)
            })
            .collect();
        // All of them first, then the groups of three, then of two.
        let mut groups: Vec<Vec<FaceName>> = vec![on.clone()];
        for size in [3, 2] {
            if on.len() > size {
                for_each_group(&on, size, &mut |g| groups.push(g.to_vec()));
            }
        }
        for group in groups.into_iter().take(TRIES) {
            let Some(name) = VertexName::new(group) else {
                continue;
            };
            if let Ok(q) = kernel.vertex_point(body.shape, &name)
                && geom::distance(q, target) < tolerance
            {
                return Some(((body.uid, name), geom::distance(q, target)));
            }
        }
    }
    None
}

/// Calls `f` with each group of `size` of `items`, in order.
fn for_each_group<T: Clone>(items: &[T], size: usize, f: &mut dyn FnMut(&[T])) {
    fn go<T: Clone>(
        items: &[T],
        size: usize,
        from: usize,
        chosen: &mut Vec<T>,
        f: &mut dyn FnMut(&[T]),
    ) {
        if chosen.len() == size {
            f(chosen);
            return;
        }
        for i in from..items.len() {
            chosen.push(items[i].clone());
            go(items, size, i + 1, chosen, f);
            chosen.pop();
        }
    }
    go(items, size, 0, &mut Vec::new(), f);
}

/// An edge of a datum's definition (mitcad#186): as [`resolve_edge`], else
/// as a fillet's edge is found ([`dressup_edge_match`]: a closed edge on
/// its circle where the replay's seam is elsewhere), when exactly one
/// replay edge fits.
pub(crate) fn resolve_datum_edge<K: Kernel>(
    doc: &Document<K>,
    fp: &Fingerprint,
) -> Option<(BodyUid, EdgeName)> {
    resolve_edge(doc, fp).or_else(|| {
        let found = dressup_edge_match(&ReplayEdges::of(doc), fp).ok()?;
        // (A sliver's other edge is no datum's.)
        match found.iter().filter(|e| !e.sliver_twin).collect::<Vec<_>>()[..] {
            [found] => Some((found.body, found.name.clone())),
            _ => None,
        }
    })
}

/// An edge gone after a fillet or chamfer.
#[derive(Debug, Clone)]
pub(crate) struct GoneEdge {
    pub(crate) name: EdgeName,
    /// How far its midpoint is from the faces after.
    pub(crate) distance: f64,
    /// The angle of the wedge between its faces on the side a rounding
    /// ball sits (π − the angle between the outward normals), radians;
    /// None when the normals are not known.
    pub(crate) wedge: Option<f64>,
    /// Its midpoint and length.
    pub(crate) mid: Vec3,
    pub(crate) length: f64,
    /// The cross section of its faces at the midpoint, when they meet at
    /// an angle.
    pub(crate) across: Option<Across>,
}

/// Two faces across an edge at a point: per face (in the order of the
/// edge's name) the unit direction into the face, perpendicular to the
/// edge, and its outward normal; and whether the material between them
/// is convex (less than half a turn: a rounding cuts it away) or concave
/// (a rounding fills the corner).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Across {
    pub(crate) into: [Vec3; 2],
    pub(crate) normal: [Vec3; 2],
    pub(crate) convex: bool,
}

/// Faces whose normals at an edge are within this angle (radians) meet
/// tangentially there: the edge is no fillet's or chamfer's input.
pub(crate) const SMOOTH_WEDGE: f64 = 1e-3;

/// How far (mm) an edge's midpoint must be from the next state's faces to
/// count as gone, for an edge of this length.
fn gone_at(length: f64) -> f64 {
    1e-4 * length.max(1.0)
}

/// Two measured distances closer than this (mm) are not told apart.
const NOISE: f64 = 1e-6;

/// The edges of `shape` that are gone in `after`: their midpoints are no
/// longer on any face of `after` (a fillet or chamfer cut or filled
/// them). Seams (a face against itself) are left out: they are no
/// fillet's input.
///
/// With `before`, the file's bodies before the item (mitcad#440), an edge
/// is gone only where its midpoint is farther from `after` than from
/// `before` by as much: an edge as far off the state before (the replay
/// split the faces there otherwise, or its body deviates) was not taken
/// away by the item. And an edge between faces so nearly tangent that the
/// item's dressup there would lie within that distance of it (`depth`:
/// the least of the item's dressups at the edge's wedge) is gone where its
/// midpoint is farther from `after` than from `before` by half that depth
/// at least; such edges are looked for on the faces of the edges found
/// gone (a dressup the state cannot show alone is not looked for).
pub(crate) fn consumed_edges<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    after: &[&K::Shape],
    before: &[&K::Shape],
    depth: &dyn Fn(f64) -> f64,
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
    let mut picked: Vec<usize> = (0..edges.len())
        .filter(|&k| nearest[k] > gone_at(edges[k].3) && nearest[k].is_finite())
        .collect();
    // Each edge's wedge, and its tangent and its faces' normals at the
    // midpoint.
    type Frame = Option<(Vec3, [Vec3; 2])>;
    let frame_of = |k: usize| -> (Option<f64>, Frame) {
        crate::tick();
        let (name, mid, tangent, _) = &edges[k];
        let [a, b] = name.faces();
        let normal = |face| {
            kernel
                .face_point_normal(shape, face, *mid)
                .ok()
                .and_then(|p| geom::unit(p.normal))
        };
        let normals = normal(a).zip(normal(b));
        let wedge = normals
            .map(|(n1, n2)| std::f64::consts::PI - geom::dot(n1, n2).clamp(-1.0, 1.0).acos());
        let frame = normals.and_then(|(n1, n2)| Some((geom::unit(*tangent)?, [n1, n2])));
        (wedge, frame)
    };
    let mut frames: std::collections::HashMap<usize, (Option<f64>, Frame)> =
        std::collections::HashMap::new();
    // How far each midpoint moved off the faces with the item; none where
    // the state before cannot be measured (the edges are then judged by the
    // state after alone).
    let moved = |ks: &[usize]| -> Option<Vec<f64>> {
        let mids: Vec<Vec3> = ks.iter().map(|&k| edges[k].1).collect();
        match nearest_boundary(kernel, before, &mids) {
            Ok(was) => Some(ks.iter().zip(was).map(|(&k, w)| nearest[k] - w).collect()),
            Err(e) => {
                if crate::tracing() {
                    eprintln!("import: the state before the item not measured: {e}");
                }
                None
            }
        }
    };
    if !before.is_empty()
        && !picked.is_empty()
        && let Some(off) = moved(&picked)
    {
        let kept: Vec<usize> = picked
            .iter()
            .zip(&off)
            .filter(|&(&k, m)| *m > gone_at(edges[k].3))
            .map(|(&k, _)| k)
            .collect();
        if crate::tracing() {
            for (&k, m) in picked.iter().zip(&off) {
                if !kept.contains(&k) {
                    eprintln!(
                        "import: not gone with the item: {} ({:.4} off the next state, {:.4} more than off the state before)",
                        edges[k].0, nearest[k], m
                    );
                }
            }
        }
        picked = kept;
        // Nearly tangent edges on the faces of those gone.
        let faces: Vec<&FaceName> = picked.iter().flat_map(|&k| edges[k].0.faces()).collect();
        let near: Vec<usize> = (0..edges.len())
            .filter(|&k| {
                !picked.contains(&k)
                    && nearest[k] > NOISE
                    && nearest[k] <= gone_at(edges[k].3)
                    && edges[k].0.faces().iter().any(|f| faces.contains(&f))
            })
            .collect();
        let mut shallow = Vec::new();
        for k in near {
            let (wedge, frame) = frame_of(k);
            if let Some(w) = wedge
                && w < std::f64::consts::PI - SMOOTH_WEDGE
                && depth(w) <= gone_at(edges[k].3)
                && depth(w) > 2.0 * NOISE
            {
                frames.insert(k, (wedge, frame));
                shallow.push(k);
            }
        }
        if !shallow.is_empty()
            && let Some(off) = moved(&shallow)
        {
            for (&k, m) in shallow.iter().zip(&off) {
                let d = frames[&k].0.map_or(f64::INFINITY, depth);
                let gone = *m >= 0.5 * d && *m > NOISE;
                if crate::tracing() {
                    eprintln!(
                        "import: nearly tangent edge {}: {:.2e} off the next state, {:.2e} more than off the state before, its dressup {:.2e} deep: {}",
                        edges[k].0,
                        nearest[k],
                        m,
                        d,
                        if gone { "gone" } else { "not gone" }
                    );
                }
                if gone {
                    picked.push(k);
                }
            }
            picked.sort_unstable();
        }
    }
    let mut gone: Vec<(GoneEdge, Frame)> = picked
        .into_iter()
        .map(|k| {
            let (wedge, frame) = frames.remove(&k).unwrap_or_else(|| frame_of(k));
            let (name, mid, _, length) = edges[k].clone();
            let edge = GoneEdge {
                name,
                distance: nearest[k],
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
pub(crate) fn lost_faces<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    after: &[&K::Shape],
) -> Result<Vec<FaceName>, String> {
    lost_faces_by(kernel, shape, after, 1.0)
}

/// The faces of `shape` of which at least `share` of the points inside
/// are on no face of `after` ([`lost_faces`] with every point): a shell's
/// removed face keeps the points near its edges on the wall's end.
pub(crate) fn lost_faces_by<K: Kernel>(
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
pub(crate) fn resolve_sheet<K: Kernel>(doc: &Document<K>, fp: &Fingerprint) -> Option<BodyUid> {
    if let Some(uid) = resolve_body(doc, fp) {
        return Some(uid);
    }
    let kernel = doc.kernel();
    let bodies = bodies_for(doc, fp);
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
pub(crate) fn nearest_boundary<K: Kernel>(
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
            let held = match kernel.enclosing_box(body) {
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
pub(crate) enum Section {
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
    pub(crate) fn size(&self) -> f64 {
        match *self {
            Self::Fillet(r) => r,
            Self::Chamfer(a, b) => a.max(b),
            Self::ChamferAngle(d, _, _) => d,
        }
    }

    /// How far the midpoint of an edge with this wedge angle lies from the
    /// dressup's face.
    pub(crate) fn depth(&self, wedge: f64) -> f64 {
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
pub(crate) fn section_points<K: Kernel>(
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
pub(crate) fn fillet_distance(r: f64, wedge: f64) -> f64 {
    r * (1.0 / (wedge / 2.0).sin().max(1e-6) - 1.0)
}

/// How far the midpoint of an edge with this wedge angle lies from a
/// chamfer `d1` and `d2` along its faces.
pub(crate) fn chamfer_distance(d1: f64, d2: f64, wedge: f64) -> f64 {
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
/// body with the same signature (in its component, `components[k]` for
/// `finals[k]`, first: two components may hold bodies alike), else the
/// nearest one of similar volume. `tick` is called between its kernel calls
/// (the import's progress, for its watchdog).
pub(crate) fn compare_bodies<K: Kernel>(
    doc: &Document<K>,
    finals: &[(StoredBody<K::Shape>, Sig)],
    components: &[ComponentUid],
    compare: bool,
    tick: &dyn Fn(),
) -> Vec<BodyReport> {
    let kernel = doc.kernel();
    let bodies: Vec<_> = crate::body_scope::bodies(doc)
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
    let body_components: Vec<Option<ComponentUid>> = bodies
        .iter()
        .map(|(uid, ..)| doc.body_component(*uid))
        .collect();
    let mut used = vec![false; bodies.len()];
    let mut out = Vec::new();
    // Exact matches first, so a near one does not take another's body;
    // those in the same component before those in another.
    let mut pairs: Vec<Option<usize>> = vec![None; finals.len()];
    for same_component in [true, false] {
        for (k, (_, sig)) in finals.iter().enumerate() {
            if pairs[k].is_some() {
                continue;
            }
            let found = (0..bodies.len()).find(|&i| {
                !used[i]
                    && (!same_component || body_components[i] == components.get(k).copied())
                    && bodies[i].3.matches(sig)
            });
            if let Some(i) = found {
                used[i] = true;
                pairs[k] = Some(i);
            }
        }
    }
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
    // (On the import's clock, mitcad#874.)
    let started = crate::now();
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
            file_faces: (sig.faces > 0).then_some(sig.faces),
            mitcad_faces: None,
            source: body.source.clone(),
            uid: None,
        };
        if let Some(i) = pair {
            let (uid, name, shape, mine) = &bodies[i];
            report.mitcad = Some(format!("{uid} {name}"));
            report.uid = Some(*uid);
            report.mitcad_volume = Some(mine.volume);
            report.mitcad_faces = (mine.faces > 0).then_some(mine.faces);
            report.volume_difference = Some((mine.volume - sig.volume) / sig.volume);
            let small = |s: &K::Shape| kernel.faces(s).is_ok_and(|f| f.len() <= 300);
            // A body with the same measures (to 1e-4) is not compared surface
            // by surface: the boolean differences of coincident shapes can
            // take minutes (a fallback's body is the file's own), and of nearly
            // coincident ones may never return (a corpus design: 192
            // against 209 faces, the volumes 7e-5 apart).
            let identical = mine.distance(sig) <= 1e-4;
            let in_time = crate::since(started).as_secs_f64() < COMPARE_SECONDS;
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
                let left = COMPARE_SECONDS - crate::since(started).as_secs_f64();
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
/// there and length, and in a second list (of the same length, or empty)
/// the points where the edge starts and ends ([`dressup_edge_match`]).
pub(crate) struct ReplayEdges(
    Vec<(BodyUid, EdgeName, Vec3, Vec3, f64)>,
    Vec<Option<[Vec3; 2]>>,
);

/// A body's named edges with their middles, tangents, lengths and ends
/// ([`ReplayEdges::of`]).
fn body_edges<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<BodyEdge> {
    let curves: std::collections::HashMap<String, [Vec3; 2]> = kernel
        .edge_geometries(shape)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, curve)| {
            let ends = match curve {
                mitcad_model::datum::CurveGeometry::Line { start, end }
                | mitcad_model::datum::CurveGeometry::Circle { start, end, .. }
                | mitcad_model::datum::CurveGeometry::Other { start, end, .. } => [start, end],
            };
            Some((name?, ends))
        })
        .collect();
    edge_middles(kernel, shape)
        .into_iter()
        .map(|(name, mid, tangent, length)| {
            let ends = curves.get(&name.to_string()).copied();
            (name, mid, tangent, length, ends)
        })
        .collect()
}

impl ReplayEdges {
    pub(crate) fn of<K: Kernel>(doc: &Document<K>) -> Self {
        let kernel = doc.kernel();
        let mut out = Vec::new();
        let mut ends = Vec::new();
        for body in crate::body_scope::bodies(doc) {
            crate::tick();
            // Kept by the body's version (mitcad#119).
            let edges = match doc.body_version(body.uid) {
                Some(version) => edges_of(version, || body_edges(kernel, body.shape)),
                None => Arc::new(body_edges(kernel, body.shape)),
            };
            for (name, mid, tangent, length, end) in edges.iter() {
                ends.push(*end);
                out.push((body.uid, name.clone(), *mid, *tangent, *length));
            }
        }
        ReplayEdges(out, ends)
    }

    /// The edges of the given bodies only.
    pub(crate) fn only(&self, bodies: &[BodyUid]) -> ReplayEdges {
        let keep: Vec<usize> = (0..self.0.len())
            .filter(|&k| bodies.contains(&self.0[k].0))
            .collect();
        ReplayEdges(
            keep.iter().map(|&k| self.0[k].clone()).collect(),
            keep.iter()
                .map(|&k| self.1.get(k).copied().flatten())
                .collect(),
        )
    }

    /// Where the edge `k` starts and ends, when known.
    fn ends(&self, k: usize) -> Option<[Vec3; 2]> {
        self.1.get(k).copied().flatten()
    }
}

/// A replay edge found for a file's edge, with the file edge's own
/// direction at the replay edge's middle where the file gives it
/// (`_f3d.direction`).
#[derive(Debug, Clone)]
pub(crate) struct FoundEdge {
    pub(crate) body: BodyUid,
    pub(crate) name: EdgeName,
    pub(crate) mid: Vec3,
    pub(crate) direction: Option<Vec3>,
    /// The matcher selected one of several distinct replay edges within
    /// the matching tolerance. False alone does not establish that all
    /// other inputs of the consuming feature were read.
    pub(crate) chosen_from_multiple: bool,
    /// The other edge of a sliver face beside the edge found
    /// ([`with_sliver_twin`], mitcad#224): the import tries the edges
    /// without it first.
    pub(crate) sliver_twin: bool,
}

/// Why a decoded dressup edge cannot be used by name (mitcad#106).
/// Decoder failures are separate from edges found in ASM but absent or
/// ambiguous in the replay; none of these justifies choosing an edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DressupEdgeFailure {
    DecoderUnresolved,
    MissingFingerprint,
    MissingInReplay,
    AmbiguousInReplay(Vec<(BodyUid, EdgeName)>),
}

impl DressupEdgeFailure {
    pub(crate) fn code(&self) -> &'static str {
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
/// Midpoint-plus-length scores within this many mm are tied. This is
/// much smaller than the matching tolerances; ties cannot establish
/// which of two equally fitting edges of one body the input names.
const SCORE_TIE: f64 = 1e-9;
/// How much longer or shorter than the file's an edge with the file's ends
/// may be (of its length): blends and splines are approximations.
const LENGTH_OFF: f64 = 0.02;

/// A file edge's curve as far as its fingerprint tells: a segment, a
/// circle (closed or an arc, with the plane's normal where known), a closed
/// ellipse or neither.
enum FileCurve {
    Segment(Vec3, Vec3),
    Circle {
        center: Vec3,
        radius: f64,
        normal: Option<Vec3>,
    },
    /// A closed ellipse: its centre, the unit directions of its two axes
    /// (`u` from the centre to the file's middle point, `v` the file's
    /// direction there) and the half lengths along them. The file starts a
    /// closed edge at one end of an axis and puts its middle at the other
    /// end: the axis between them is `u`, and the direction at the middle
    /// is along the other axis.
    Ellipse {
        center: Vec3,
        u: Vec3,
        v: Vec3,
        p: f64,
        q: f64,
    },
    Other,
}

/// The perimeter of an ellipse with the half axes `p` and `q`: the
/// trapezoid rule over the full period, which converges exponentially.
fn ellipse_perimeter(p: f64, q: f64) -> f64 {
    const N: usize = 512;
    let step = std::f64::consts::TAU / N as f64;
    (0..N)
        .map(|k| {
            let t = step * k as f64;
            (p * t.sin()).hypot(q * t.cos())
        })
        .sum::<f64>()
        * step
}

/// The half axis `q` of an ellipse of perimeter `length` whose other half
/// axis is `p`; None when no ellipse has them (a perimeter under the
/// rectangle round the axis `p`'s ellipse at its thinnest).
fn ellipse_other_axis(p: f64, length: f64) -> Option<f64> {
    if p <= 1e-9 || length <= 4.0 * p {
        return None;
    }
    // The perimeter grows with q, from 4p (q 0) past the length at
    // q = length / 4 (the perimeter is at least 4 q).
    let (mut low, mut high) = (0.0, length / 4.0);
    for _ in 0..80 {
        let mid = 0.5 * (low + high);
        if ellipse_perimeter(p, mid) < length {
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(0.5 * (low + high))
}

impl FileCurve {
    fn of(a: Vec3, m: Vec3, b: Vec3, length: f64, closed: bool, direction: Option<Vec3>) -> Self {
        if closed {
            let center = geom::scale(geom::add(a, m), 0.5);
            let radius = geom::distance(a, m) / 2.0;
            if (std::f64::consts::TAU * radius - length).abs() > 1e-3 * length.max(1.0) {
                let ellipse = direction.and_then(|d| {
                    let u = geom::unit(geom::sub(m, center))?;
                    // (The direction at the middle is across the axis.)
                    let v = geom::unit(geom::sub(d, geom::scale(u, geom::dot(d, u))))?;
                    let q = ellipse_other_axis(radius, length)?;
                    Some(FileCurve::Ellipse {
                        center,
                        u,
                        v,
                        p: radius,
                        q,
                    })
                });
                return ellipse.unwrap_or(FileCurve::Other);
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

    /// How far a replay edge's middle (with its tangent there) lies from
    /// the curve, mm; None where it does not run along it (off the
    /// segment's ends, across the circle or ellipse) or the curve is not
    /// known.
    fn offset(&self, p: Vec3, tangent: Vec3) -> Option<f64> {
        match *self {
            FileCurve::Segment(a, b) => {
                let ab = geom::sub(b, a);
                let t = geom::dot(geom::sub(p, a), ab) / geom::dot(ab, ab).max(1e-18);
                (0.0..=1.0)
                    .contains(&t)
                    .then(|| geom::distance(p, geom::add(a, geom::scale(ab, t))))
            }
            FileCurve::Circle {
                center,
                radius,
                normal,
            } => {
                let r = geom::sub(p, center);
                // The replay edge's own plane when the file's is not known.
                let n = normal.or_else(|| geom::unit(geom::cross(r, tangent)))?;
                (geom::dot(r, tangent).abs() <= 1e-3 * radius.max(1.0))
                    .then(|| (geom::norm(r) - radius).abs().max(geom::dot(r, n).abs()))
            }
            FileCurve::Ellipse {
                center,
                u,
                v,
                p: a,
                q: b,
            } => {
                let r = geom::sub(p, center);
                let (x, y, z) = (
                    geom::dot(r, u),
                    geom::dot(r, v),
                    geom::dot(r, geom::cross(u, v)),
                );
                // The distance to the ellipse to first order: the way the
                // point's scaled radius differs from 1, as long as the
                // thinner half axis.
                let k = (x / a).hypot(y / b);
                let off = (k - 1.0).abs() * a.min(b);
                // The tangent is across the gradient there.
                let gradient = geom::add(geom::scale(u, x / (a * a)), geom::scale(v, y / (b * b)));
                geom::unit(gradient)
                    .is_some_and(|g| geom::dot(g, tangent).abs() <= 1e-3)
                    .then(|| z.abs().max(off))
            }
            FileCurve::Other => None,
        }
    }

    /// Where a point lies across a closed curve: its signed offsets from a
    /// circle (radially, along the normal) or an ellipse (outward, out of
    /// its plane); None for another curve. Two edges of a sliver face lie
    /// apart by it; two coincident edges do not.
    fn across(&self, p: Vec3) -> Option<[f64; 2]> {
        match *self {
            FileCurve::Circle {
                center,
                radius,
                normal: Some(n),
            } => {
                let r = geom::sub(p, center);
                let along = geom::dot(r, n);
                let flat = geom::sub(r, geom::scale(n, along));
                Some([geom::norm(flat) - radius, along])
            }
            FileCurve::Ellipse {
                center,
                u,
                v,
                p: a,
                q: b,
            } => {
                let r = geom::sub(p, center);
                let k = (geom::dot(r, u) / a).hypot(geom::dot(r, v) / b);
                Some([(k - 1.0) * a.min(b), geom::dot(r, geom::cross(u, v))])
            }
            _ => None,
        }
    }

    /// Whether a replay edge's middle lies on the curve within `tolerance`.
    fn holds(&self, p: Vec3, tangent: Vec3, tolerance: f64) -> bool {
        self.offset(p, tangent).is_some_and(|d| d <= tolerance)
    }

    /// How far along the file's open edge (a to b through m) a point on
    /// it is, mm; none for a closed or unknown curve.
    fn param(&self, a: Vec3, m: Vec3, p: Vec3) -> Option<f64> {
        match *self {
            FileCurve::Segment(from, to) => {
                let u = geom::unit(geom::sub(to, from))?;
                Some(geom::dot(geom::sub(p, from), u))
            }
            FileCurve::Circle { center, radius, .. } if geom::distance(a, m) > 1e-6 => {
                let (x, y) = (geom::sub(a, center), geom::sub(p, center));
                let axis = geom::unit(geom::cross(x, geom::sub(m, center)))?;
                let mut angle = geom::dot(geom::cross(x, y), axis).atan2(geom::dot(x, y));
                if angle < -1e-9 {
                    angle += std::f64::consts::TAU;
                }
                Some(radius * angle)
            }
            _ => None,
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
            // The ellipse runs round its axes the way its direction at the
            // middle says: the tangent turns with the point.
            FileCurve::Ellipse {
                center,
                u,
                v,
                p: a,
                q: b,
            } => {
                let r = geom::sub(p, center);
                let (x, y) = (geom::dot(r, u), geom::dot(r, v));
                let tangent = geom::add(geom::scale(u, -y / (b * b)), geom::scale(v, x / (a * a)));
                geom::unit(tangent).unwrap_or(d)
            }
            _ => d,
        }
    }
}

/// [`dressup_edge_match_in`]; an edge the replay lacks is traced with the
/// replay's edges near it (`MITCAD_IMPORT_TRACE`).
pub(crate) fn dressup_edge_match(
    edges: &ReplayEdges,
    fp: &Fingerprint,
) -> Result<Vec<FoundEdge>, DressupEdgeFailure> {
    let result = dressup_edge_match_in(edges, fp);
    if crate::tracing()
        && matches!(result, Err(DressupEdgeFailure::MissingInReplay))
        && let (Some(m), Some(length)) = (fp.mid_point, fp.length)
    {
        let (m, length) = (mm3(m), mm(length));
        eprintln!(
            "import: edge not in the replay: middle {m:.3?} length {length:.3} start {:.3?} end {:.3?}",
            fp.start_point.flatten().map(mm3),
            fp.end_point.flatten().map(mm3)
        );
        for (k, (body, name, mid, _, len)) in edges.0.iter().enumerate() {
            if geom::distance(*mid, m) <= length.max(5.0) {
                eprintln!(
                    "import:   near: {body}/{name} middle {mid:.3?} length {len:.3} ends {:.3?}",
                    edges.ends(k)
                );
            }
        }
    }
    result
}

/// A fillet's or chamfer's edge given by its fingerprint, in the replay
/// (mitcad#96). As [`resolve_edge`] (middle within 1e-3 mm, same length),
/// else:
/// - a closed edge: the edge of its length on its circle (the file's
///   middle is half way along the edge's parameter, opposite its seam;
///   the replay's seam is elsewhere), when only one is;
/// - an open edge whose middle is up to [`MID_OFF`] off: the nearest of
///   its length;
/// - two or more edges of one body on its segment or circle whose lengths
///   add up to its, following each other along an open edge (the
///   replay splits it where a sketch had a point), when only one set is.
///
/// Edges of any body. Tied midpoint scores within one body are refused;
/// as [`resolve_edge`], coincident copies in different bodies keep the
/// first body of the best match. A recipe's tail is used by the decoder
/// (`_f3d.end_faces`), not here: it tells mirror-image edges apart there.
///
/// Where nothing is within [`EXACT`] (the middle, the points on the file's
/// curve, the ends), the same is looked for within the
/// [`geometric_tolerance`] (mitcad#224); the match keeps its uniqueness
/// rules, and its deviation is noted for the item's report.
fn dressup_edge_match_in(
    edges: &ReplayEdges,
    fp: &Fingerprint,
) -> Result<Vec<FoundEdge>, DressupEdgeFailure> {
    let found = match dressup_edge_match_within(edges, fp, EXACT) {
        Err(DressupEdgeFailure::MissingInReplay) => {
            // (mitcad#224) The replay's edge may be a few micrometres off
            // the file's: an earlier feature built that much otherwise.
            let Some(tolerance) = widened() else {
                return Err(DressupEdgeFailure::MissingInReplay);
            };
            let (found, deviation) = dressup_edge_match_within(edges, fp, tolerance)?;
            note_deviation(deviation);
            found
        }
        other => other.map(|(found, _)| found)?,
    };
    Ok(with_sliver_twin(edges, fp, found))
}

/// How far apart (mm) across the file's curve two edges of a sliver face
/// lie at least; closer, they are coincident copies.
const APART: f64 = 1e-7;

/// Whether two points on a closed curve lie apart across it ([`APART`]).
fn apart(curve: &FileCurve, p: Vec3, q: Vec3) -> bool {
    match (curve.across(p), curve.across(q)) {
        (Some(a), Some(b)) => (a[0] - b[0]).hypot(a[1] - b[1]) >= APART,
        _ => false,
    }
}

/// The edges found, with the other edge of a sliver face beside a single
/// edge found (mitcad#224): a dressup that takes a face away exactly is
/// built 1e-3 smaller (`geometry/src/dressup.cpp`), which leaves a strip
/// that narrow where the file's two faces meet in one edge. The strip's
/// two edges both lie on the file's curve within the
/// [`geometric_tolerance`], have its length and share the strip: the file's
/// edge is both of them. One such twin at most; none where the curve is
/// not known (a spline).
fn with_sliver_twin(
    edges: &ReplayEdges,
    fp: &Fingerprint,
    found: Vec<FoundEdge>,
) -> Vec<FoundEdge> {
    let ([one], Some(tolerance)) = (&found[..], widened()) else {
        return found;
    };
    let (Some(m), Some(length), Some(a), Some(b)) = (
        fp.mid_point.map(mm3),
        fp.length.map(mm),
        fp.start_point.flatten().map(mm3),
        fp.end_point.flatten().map(mm3),
    ) else {
        return found;
    };
    let closed = geom::distance(a, b) <= 1e-6 * length.max(1.0);
    let direction = fp
        .f3d
        .as_ref()
        .and_then(|f| f.direction)
        .and_then(geom::unit);
    let curve = FileCurve::of(a, m, b, length, closed, direction);
    let shares_a_face = |name: &EdgeName| {
        let [p, q] = name.faces();
        one.name.faces().iter().any(|f| f == p || f == q)
    };
    let twins: Vec<(usize, f64)> = (0..edges.0.len())
        .filter_map(|k| {
            let (body, name, p, t, len) = &edges.0[k];
            if *body != one.body || *name == one.name || !shares_a_face(name) {
                return None;
            }
            if (len - length).abs() > (1e-3 * len).max(tolerance) {
                return None;
            }
            let off = curve.offset(*p, *t).filter(|d| *d <= tolerance)?;
            // Beside the one found, not on it (a coincident copy); an open
            // edge runs along it.
            let beside = if closed {
                apart(&curve, *p, one.mid)
            } else {
                (APART..=2.0 * tolerance).contains(&geom::distance(*p, one.mid))
            };
            beside.then_some((k, off))
        })
        .collect();
    let mut names: Vec<&EdgeName> = twins.iter().map(|&(k, _)| &edges.0[k].1).collect();
    names.dedup();
    let [_] = names[..] else {
        return found;
    };
    let (k, off) = twins[0];
    if off > EXACT {
        note_deviation(off);
    }
    // The edge was chosen from several where others than the two fit the
    // file's: with both taken, the twin says whether that is still a
    // choice (the import's definition with the twins takes its flag).
    let mut fitting: Vec<(BodyUid, &EdgeName)> = (0..edges.0.len())
        .filter(|&k| {
            let (_, _, p, t, len) = &edges.0[k];
            (len - length).abs() <= (1e-3 * len).max(tolerance)
                && (geom::distance(*p, m) <= tolerance
                    || curve.offset(*p, *t).is_some_and(|d| d <= tolerance))
        })
        .map(|k| (edges.0[k].0, &edges.0[k].1))
        .collect();
    fitting.sort_by_key(|(b, n)| (b.to_string(), n.to_string()));
    fitting.dedup();
    let chosen = one.chosen_from_multiple && fitting.len() > 2;
    let (body, name, mid, _, _) = &edges.0[k];
    let twin = FoundEdge {
        body: *body,
        name: name.clone(),
        mid: *mid,
        direction: direction.map(|d| curve.direction_at(m, d, *mid)),
        chosen_from_multiple: chosen,
        sliver_twin: true,
    };
    let mut found = found;
    found.push(twin);
    found
}

/// [`dressup_edge_match_in`] with the distances that are tolerances (the
/// middle of an edge found by its middle and length, a point on the file's
/// curve, the ends) within `tolerance`; with the largest such distance of
/// the edges found.
fn dressup_edge_match_within(
    edges: &ReplayEdges,
    fp: &Fingerprint,
    tolerance: f64,
) -> Result<(Vec<FoundEdge>, f64), DressupEdgeFailure> {
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
    // (Within 1e-3 of the length, or the tolerance on a short edge.)
    let length_off = |len: f64| (1e-3 * len).max(tolerance);
    let same_length = |len: f64| (len - length).abs() <= length_off(len);
    let found = |k: usize, direction: Option<Vec3>, chosen_from_multiple: bool| {
        let (body, name, mid, _, _) = &edges.0[k];
        FoundEdge {
            body: *body,
            name: name.clone(),
            mid: *mid,
            direction,
            chosen_from_multiple,
            sliver_twin: false,
        }
    };
    // How far an edge found by its middle is off the file's.
    let middle_off = |k: usize| {
        let (_, _, p, _, len) = &edges.0[k];
        geom::distance(*p, m).max((len - length).abs())
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
    // `longer`: the length's tolerance relative to it (at least the
    // tolerance).
    let nearest = |within: f64, longer: f64| -> Result<Option<(usize, bool)>, DressupEdgeFailure> {
        let candidates: Vec<usize> = (0..edges.0.len())
            .filter(|&k| {
                let (_, _, p, _, len) = &edges.0[k];
                (len - length).abs() <= (longer * len.max(1.0)).max(tolerance)
                    && geom::distance(*p, m) <= within
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
        // Include other bodies and non-tied scores: choosing the first
        // coincident copy or the best of several plausible edges is an
        // inferred selection. Repeated aliases count as one edge. This
        // metadata does not change the existing selection or tie policy.
        let chosen_from_multiple = candidates
            .iter()
            .any(|&k| edges.0[k].0 != edges.0[best].0 || edges.0[k].1 != edges.0[best].1);
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
            Ok(Some((best, chosen_from_multiple)))
        }
    };
    if let Some((k, chosen)) = nearest(tolerance, 1e-3)? {
        return Ok((vec![found(k, direction, chosen)], middle_off(k)));
    }
    let (Some(a), Some(b)) = (a, b) else {
        return Err(DressupEdgeFailure::MissingInReplay);
    };
    let closed = geom::distance(a, b) <= 1e-6 * length.max(1.0);
    let curve = FileCurve::of(a, m, b, length, closed, direction);
    let on = |k: usize| {
        let (_, _, p, t, _) = &edges.0[k];
        curve.holds(*p, *t, tolerance)
    };
    let curve_off = |set: &[usize]| {
        set.iter()
            .filter_map(|&k| curve.offset(edges.0[k].2, edges.0[k].3))
            .fold(0.0, f64::max)
    };
    let along = |k: usize| direction.map(|d| curve.direction_at(m, d, edges.0[k].2));
    if closed {
        let fits = unique(
            (0..edges.0.len())
                .filter(|&k| same_length(edges.0[k].4) && on(k))
                .collect(),
        );
        if let [k] = fits[..] {
            return Ok((vec![found(k, along(k), false)], curve_off(&fits)));
        } else if let [i, j] = fits[..]
            && edges.0[i].0 == edges.0[j].0
            && apart(&curve, edges.0[i].2, edges.0[j].2)
            && edges.0[i]
                .1
                .faces()
                .iter()
                .any(|f| edges.0[j].1.faces().contains(f))
        {
            // The two edges of a sliver face where the file has one edge
            // ([`with_sliver_twin`]).
            return Ok((
                vec![found(i, along(i), false), found(j, along(j), false)],
                curve_off(&fits),
            ));
        } else if fits.len() > 1 {
            return Err(ambiguous(fits));
        }
    } else if let Some((k, chosen)) = nearest(MID_OFF, 1e-3)? {
        return Ok((vec![found(k, direction, chosen)], middle_off(k)));
    }
    // The edge's ends and length: a blend's or spline's middle is where its
    // parameter range is half, which is elsewhere on the replay's curve
    // (up to a tenth of its length), and its length the approximation's
    // (up to LENGTH_OFF).
    if !closed {
        // (The middle right to 0.01 mm, the length a little off.)
        if let Some((k, chosen)) = nearest(1e-2_f64.max(tolerance), LENGTH_OFF)? {
            return Ok((vec![found(k, direction, chosen)], middle_off(k)));
        }
        let near = |p: Vec3, q: Vec3| geom::distance(p, q) <= tolerance;
        let by_ends = unique(
            (0..edges.0.len())
                .filter(|&k| {
                    edges.ends(k).is_some_and(|[s, e]| {
                        (near(s, a) && near(e, b)) || (near(s, b) && near(e, a))
                    }) && (edges.0[k].4 - length).abs() <= LENGTH_OFF * length
                })
                .collect(),
        );
        let ends_off = |k: usize| {
            edges.ends(k).map_or(0.0, |[s, e]| {
                let (ab, ba) = (
                    geom::distance(s, a).max(geom::distance(e, b)),
                    geom::distance(s, b).max(geom::distance(e, a)),
                );
                ab.min(ba)
            })
        };
        match by_ends[..] {
            [] => {}
            [k] => return Ok((vec![found(k, direction, false)], ends_off(k))),
            _ if by_ends
                .iter()
                .all(|&k| geom::distance(edges.0[k].2, edges.0[by_ends[0]].2) <= 1e-3) =>
            {
                let k = by_ends[0];
                return Ok((vec![found(k, direction, true)], ends_off(k)));
            }
            _ => return Err(ambiguous(by_ends)),
        }
    }
    // Pieces of one body that make up the edge.
    let pieces = unique(
        (0..edges.0.len())
            .filter(|&k| edges.0[k].4 < length && on(k))
            .collect(),
    );
    let mut bodies: Vec<BodyUid> = Vec::new();
    for &k in &pieces {
        if !bodies.contains(&edges.0[k].0) {
            bodies.push(edges.0[k].0);
        }
    }
    let mut sets: Vec<Vec<usize>> = Vec::new();
    for body in bodies {
        let group: Vec<Piece> = pieces
            .iter()
            .filter(|&&k| edges.0[k].0 == body)
            .map(|&k| Piece {
                index: k,
                length: edges.0[k].4,
                at: curve.param(a, m, edges.0[k].2),
            })
            .collect();
        sets.extend(piece_sets(&group, length, closed, tolerance));
    }
    match &sets[..] {
        [set] => Ok((
            set.iter().map(|&k| found(k, along(k), false)).collect(),
            curve_off(set),
        )),
        [] if !closed && !matches!(curve, FileCurve::Segment(..)) => {
            // An open edge that is not a line (an elliptical arc, a
            // spline; a circle through three of an ellipse's points fits
            // its length but not its pieces): pieces that follow each
            // other by their ends from its start to its end.
            let chains = chains_by_ends(edges, a, b, m, length, tolerance);
            match &chains[..] {
                [] => Err(DressupEdgeFailure::MissingInReplay),
                [chain] => {
                    // How far its ends are from the file's.
                    let end_off = |p: Vec3| {
                        chain
                            .iter()
                            .filter_map(|&k| edges.ends(k))
                            .flatten()
                            .map(|q| geom::distance(p, q))
                            .fold(f64::INFINITY, f64::min)
                    };
                    Ok((
                        chain.iter().map(|&k| found(k, direction, false)).collect(),
                        end_off(a).max(end_off(b)),
                    ))
                }
                _ => {
                    let mut indices: Vec<usize> = chains.iter().flatten().copied().collect();
                    indices.sort_unstable();
                    indices.dedup();
                    Err(ambiguous(indices))
                }
            }
        }
        [] => Err(DressupEdgeFailure::MissingInReplay),
        _ => {
            let mut indices: Vec<usize> = sets.iter().flatten().copied().collect();
            indices.sort_unstable();
            indices.dedup();
            Err(ambiguous(indices))
        }
    }
}

/// Most steps of the search in [`chains_by_ends`].
const ENDS_CHAIN_STEPS: usize = 5_000;

/// Which edges [`chains_by_ends`] takes on from a piece: those on a face
/// that every piece so far lies on, those sharing a face with the last
/// piece, or any.
#[derive(Clone, Copy)]
enum Along {
    OneFace,
    Faces,
    Any,
}

/// How far the file's middle may be from the chain of pieces (of the edge's
/// length): the chain's middles and ends only approximate the curve.
const CHAIN_MID_OFF: f64 = 0.02;

/// The distance from `p` to the circular arc from `a` through `m` to `b`
/// (the segment `a`-`b` when the three points are in line).
fn arc_distance(p: Vec3, a: Vec3, m: Vec3, b: Vec3) -> f64 {
    let (ab, am) = (geom::sub(b, a), geom::sub(m, a));
    let n = geom::cross(am, ab);
    let n2 = geom::dot(n, n);
    if n2 <= 1e-18 * geom::dot(ab, ab).max(1e-18).powi(2) {
        return segment_distance(p, a, b);
    }
    // The centre: the point of the plane equally far from a, m and b.
    let center = geom::add(
        a,
        geom::scale(
            geom::add(
                geom::scale(geom::cross(n, am), geom::dot(ab, ab)),
                geom::scale(geom::cross(n, ab), -geom::dot(am, am)),
            ),
            1.0 / (2.0 * n2),
        ),
    );
    let radius = geom::distance(center, a);
    let Some(axis) = geom::unit(n) else {
        return segment_distance(p, a, b);
    };
    let angle = |x: Vec3| {
        let (u, v) = (geom::sub(a, center), geom::sub(x, center));
        geom::dot(axis, geom::cross(u, v)).atan2(geom::dot(u, v))
    };
    // The arc runs the way that passes m: from a to b either by the
    // positive angle or the negative one.
    let tau = std::f64::consts::TAU;
    let wrap = |t: f64| t.rem_euclid(tau);
    let (to_m, to_b) = (wrap(angle(m)), wrap(angle(b)));
    let positive = to_m <= to_b;
    let w = geom::sub(p, center);
    let out = geom::dot(w, axis);
    let flat = geom::sub(w, geom::scale(axis, out));
    let at = wrap(angle(p));
    let inside = if positive { at <= to_b } else { at >= to_b };
    if inside {
        (geom::norm(flat) - radius).hypot(out)
    } else {
        geom::distance(p, a).min(geom::distance(p, b))
    }
}

/// The distance from `p` to the segment `a`-`b`.
fn segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f64 {
    let ab = geom::sub(b, a);
    let t = (geom::dot(geom::sub(p, a), ab) / geom::dot(ab, ab).max(1e-18)).clamp(0.0, 1.0);
    geom::distance(p, geom::add(a, geom::scale(ab, t)))
}

/// A piece in a chain: the replay edge and the points where it is entered
/// and left.
type Link = (usize, Vec3, Vec3);

/// The chains of two or more edges of one body of the replay that follow
/// each other by their ends from `a` to `b` and whose lengths add up to
/// `length` (the pieces of one curve the replay splits where the file has
/// one edge), as far as their ends and middles lie near the file's middle
/// `m`; ends within `end_off` of each other. At most two are returned (a
/// second one makes the first ambiguous).
fn chains_by_ends(
    edges: &ReplayEdges,
    a: Vec3,
    b: Vec3,
    m: Vec3,
    length: f64,
    end_off: f64,
) -> Vec<Vec<usize>> {
    let tol = (1e-3 * length.max(1.0)).max(end_off);
    let short: Vec<usize> = (0..edges.0.len())
        .filter(|&k| edges.ends(k).is_some() && edges.0[k].4 < length - tol)
        .collect();
    struct Walk<'a> {
        edges: &'a ReplayEdges,
        short: &'a [usize],
        b: Vec3,
        length: f64,
        tol: f64,
        end_off: f64,
        steps: usize,
        along: Along,
        chain: Vec<Link>,
        out: Vec<Vec<Link>>,
    }
    impl Walk<'_> {
        /// Whether the chain may go on with edge `k`. The pieces of a curve
        /// the replay splits run along the faces on either side of it:
        /// first only edges on a face every piece so far lies on, then
        /// edges sharing a face with the last piece, then any (a helix of
        /// 520 mm split per turn: every chain of the body's shorter edges
        /// from its start, searched at once, spent the steps before it was
        /// found).
        fn takes(&self, k: usize) -> bool {
            let faces_of = |i: usize| self.edges.0[i].1.faces();
            match (self.along, self.chain.last()) {
                (Along::Any, _) | (_, None) => true,
                (Along::OneFace, Some(_)) => faces_of(k)
                    .iter()
                    .any(|f| self.chain.iter().all(|c| faces_of(c.0).contains(f))),
                (Along::Faces, Some(last)) => {
                    let before = faces_of(last.0);
                    faces_of(k).iter().any(|f| before.contains(f))
                }
            }
        }

        fn follow(&mut self, from: Vec3, sum: f64) {
            if self.steps == 0 || self.out.len() > 1 {
                return;
            }
            self.steps -= 1;
            let end_off = self.end_off;
            let near = |p: Vec3, q: Vec3| geom::distance(p, q) <= end_off;
            for &k in self.short {
                if self.chain.iter().any(|c| c.0 == k) {
                    continue;
                }
                if let Some(first) = self.chain.first()
                    && self.edges.0[first.0].0 != self.edges.0[k].0
                {
                    continue;
                }
                let Some([s, e]) = self.edges.ends(k) else {
                    continue;
                };
                let (start, end) = if near(s, from) {
                    (s, e)
                } else if near(e, from) {
                    (e, s)
                } else {
                    continue;
                };
                if !self.takes(k) {
                    continue;
                }
                let total = sum + self.edges.0[k].4;
                // (The rest of the curve from there to b is no shorter than
                // the straight line.)
                if total > self.length + self.tol
                    || near(start, end)
                    || geom::distance(end, self.b) > self.length - total + self.tol
                {
                    continue;
                }
                self.chain.push((k, start, end));
                if near(end, self.b) && (total - self.length).abs() <= self.tol {
                    if self.chain.len() >= 2 {
                        self.out.push(self.chain.clone());
                    }
                } else {
                    self.follow(end, total);
                }
                self.chain.pop();
            }
        }
    }
    let mut walk = Walk {
        edges,
        short: &short,
        b,
        length,
        tol,
        end_off,
        steps: ENDS_CHAIN_STEPS,
        along: Along::OneFace,
        chain: Vec::new(),
        out: Vec::new(),
    };
    for along in [Along::OneFace, Along::Faces, Along::Any] {
        walk.steps = ENDS_CHAIN_STEPS;
        walk.along = along;
        walk.follow(a, 0.0);
        if !walk.out.is_empty() {
            break;
        }
    }
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for chain in walk.out {
        // The file's middle lies on the curve, which the chain's ends and
        // middles follow.
        let mut points = vec![a];
        for &(k, _, end) in &chain {
            points.push(edges.0[k].2);
            points.push(end);
        }
        let off = points
            .windows(2)
            .map(|w| segment_distance(m, w[0], w[1]))
            .fold(f64::INFINITY, f64::min);
        // A piece that is a long arc of an ellipse or a blend curves away
        // from the polyline through its ends and middle: the arc through
        // the same three points follows it much closer.
        let arc_off = chain
            .iter()
            .map(|&(k, start, end)| arc_distance(m, start, edges.0[k].2, end))
            .fold(f64::INFINITY, f64::min);
        if off.min(arc_off) > CHAIN_MID_OFF * length {
            continue;
        }
        let mut names: Vec<usize> = Vec::new();
        for (k, _, _) in chain {
            // (Aliases of one name are one edge.)
            if !names
                .iter()
                .any(|&o| edges.0[o].0 == edges.0[k].0 && edges.0[o].1 == edges.0[k].1)
            {
                names.push(k);
            }
        }
        if !chains.iter().any(|c| {
            c.len() == names.len()
                && c.iter()
                    .all(|&o| names.iter().any(|&n| edges.0[n].1 == edges.0[o].1))
        }) {
            chains.push(names);
        }
    }
    chains
}

/// A replay edge that may be a piece of a file's edge: its index, length
/// and where its middle is along the file's edge (mm from its start; none
/// on a closed edge).
struct Piece {
    index: usize,
    length: f64,
    at: Option<f64>,
}

/// Most pieces of one closed edge looked at together; a longer group is
/// taken as a whole or not at all.
const MAX_PIECES: usize = 20;

/// Most steps of the search for the chains of pieces of an open edge (a
/// long run of edges on one line can make it long).
const PIECE_STEPS: usize = 20_000;

/// The sets of two or more pieces (of one body) that make up an edge of
/// `length`: on an open edge those that follow each other from its start
/// to its end, on a closed one those whose lengths add up to its. At most
/// two are returned (a second one makes the first ambiguous). Lengths add up
/// within 1e-3 of the length or `tolerance`.
fn piece_sets(pieces: &[Piece], length: f64, closed: bool, tolerance: f64) -> Vec<Vec<usize>> {
    let tol = (1e-3 * length.max(1.0)).max(tolerance);
    let mut sets: Vec<Vec<usize>> = Vec::new();
    if pieces.len() < 2 {
        return sets;
    }
    if closed && pieces.len() > MAX_PIECES {
        let total: f64 = pieces.iter().map(|p| p.length).sum();
        if (total - length).abs() <= tol {
            sets.push(pieces.iter().map(|p| p.index).collect());
        }
        return sets;
    }
    if closed {
        fn add(
            pieces: &[Piece],
            from: usize,
            sum: f64,
            length: f64,
            tol: f64,
            chosen: &mut Vec<usize>,
            out: &mut Vec<Vec<usize>>,
        ) {
            if chosen.len() >= 2 && (sum - length).abs() <= tol {
                out.push(chosen.clone());
                return;
            }
            for k in from..pieces.len() {
                if out.len() > 1 {
                    return;
                }
                if sum + pieces[k].length <= length + tol {
                    chosen.push(pieces[k].index);
                    add(
                        pieces,
                        k + 1,
                        sum + pieces[k].length,
                        length,
                        tol,
                        chosen,
                        out,
                    );
                    chosen.pop();
                }
            }
        }
        add(pieces, 0, 0.0, length, tol, &mut Vec::new(), &mut sets);
    } else {
        let mut walk = Chains {
            pieces,
            length,
            tol,
            chain: Vec::new(),
            used: Vec::new(),
            budget: PIECE_STEPS,
            out: Vec::new(),
        };
        walk.follow(0.0);
        sets = walk.out;
        sets.retain(|s| s.len() >= 2);
    }
    sets.truncate(2);
    sets
}

/// The search for the chains of pieces that continue from the start of an
/// open edge of `length` to its end.
struct Chains<'a> {
    pieces: &'a [Piece],
    length: f64,
    tol: f64,
    chain: Vec<usize>,
    used: Vec<usize>,
    budget: usize,
    out: Vec<Vec<usize>>,
}

impl Chains<'_> {
    fn follow(&mut self, at: f64) {
        if (at - self.length).abs() <= self.tol {
            self.out.push(self.chain.clone());
            return;
        }
        for (k, piece) in self.pieces.iter().enumerate() {
            if self.out.len() > 1 || self.budget == 0 {
                return;
            }
            self.budget -= 1;
            let Some(middle) = piece.at else { continue };
            if self.used.contains(&k)
                || piece.length <= self.tol
                || at + piece.length > self.length + self.tol
                || (middle - piece.length / 2.0 - at).abs() > self.tol
            {
                continue;
            }
            self.chain.push(piece.index);
            self.used.push(k);
            self.follow(at + piece.length);
            self.used.pop();
            self.chain.pop();
        }
    }
}

/// Whether the first face of a replay edge's name is the face on the
/// left of the file edge's direction at the edge's middle (seen against
/// the face's outward normal): a point just off the edge that way, in the
/// face's tangent plane, lies on that face and not on the other. `None`
/// when the faces do not tell.
pub(crate) fn first_face_on_left<K: Kernel>(
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
    use mitcad_dump::FingerprintF3d;

    /// A face is passed over only when its box lies farther than the
    /// tolerance from the point (mitcad#119); without boxes none is.
    #[test]
    fn faces_beyond_their_boxes_are_passed_over() {
        let boxes = Some(vec![[[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]]);
        assert!(!beyond(&boxes, 0, [0.5, 0.5, 0.5], 1e-3));
        assert!(!beyond(&boxes, 0, [1.0005, 0.5, 0.5], 1e-3));
        assert!(beyond(&boxes, 0, [1.002, 0.5, 0.5], 1e-3));
        assert!(beyond(&boxes, 0, [1.0008, 1.0008, 1.0008], 1e-3));
        assert!(!beyond(&None, 0, [9.0, 9.0, 9.0], 1e-3));
        assert!(!beyond(&boxes, 1, [9.0, 9.0, 9.0], 1e-3));
    }

    fn edges(list: &[(&str, &str, Vec3, Vec3, f64)]) -> ReplayEdges {
        ReplayEdges(
            list.iter()
                .map(|(b, e, p, t, l)| (b.parse().unwrap(), e.parse().unwrap(), *p, *t, *l))
                .collect(),
            Vec::new(),
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
        assert!(matched[0].chosen_from_multiple);
    }

    #[test]
    fn repeated_aliases_of_the_same_edge_are_one_match() {
        let f = fp([0.0; 3], 10.0, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], None);
        let edge = ("F1.b0", "E{F1:a|F1:b}", [0.0; 3], [1.0, 0.0, 0.0], 10.0);
        let matched = dressup_edge_match(&edges(&[edge, edge]), &f).unwrap();
        assert_eq!(matched.len(), 1);
        assert!(!matched[0].chosen_from_multiple);
    }

    #[test]
    fn best_midpoint_score_keeps_its_choice_but_reports_inference() {
        let f = fp([0.0; 3], 10.0, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], None);
        for off in [0.0001, 0.02] {
            let mut list = [
                (
                    "F1.b0",
                    "E{F1:a|F1:b}",
                    [off * 1.5, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    10.0,
                ),
                (
                    "F1.b0",
                    "E{F1:a|F1:c}",
                    [off, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    10.0,
                ),
            ];
            for _ in 0..2 {
                let matched = dressup_edge_match(&edges(&list), &f).unwrap();
                assert_eq!(matched[0].name.to_string(), "E{F1:a|F1:c}");
                assert!(matched[0].chosen_from_multiple);
                list.reverse();
            }
            let matched = dressup_edge_match(&edges(&list[1..]), &f).unwrap();
            assert_eq!(matched[0].name.to_string(), "E{F1:a|F1:c}");
            assert!(!matched[0].chosen_from_multiple);
        }
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
    fn a_curved_edge_is_found_by_its_ends_and_length() {
        let arc = [1.0, 0.0, 0.0];
        let (a, b) = ([0.0, 0.0, 0.0], [3.0, 3.0, 0.0]);
        // The file's edge: a curve 4.4 long whose middle is not where the
        // replay's is (0.4 mm off: a blend's parameter middle), and whose
        // length is 1 % off.
        let f = fp([1.0, 2.0, 0.0], 4.4, a, b, None);
        let mut replay = edges(&[
            ("F1.b0", "E{F1:a|F1:b}", [1.2, 1.7, 0.0], arc, 4.356),
            ("F1.b0", "E{F1:a|F1:c}", [1.0, 2.0, 0.5], arc, 9.0),
        ]);
        // Without the ends nothing says it is the edge.
        assert_eq!(
            dressup_edge_match(&replay, &f).unwrap_err(),
            DressupEdgeFailure::MissingInReplay
        );
        replay.1 = vec![Some([b, a]), Some([[0.0; 3], [3.0, 3.0, 5.0]])];
        let found = dressup_edge_match(&replay, &f).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.to_string(), "E{F1:a|F1:b}");
        assert!(!found[0].chosen_from_multiple);
        // The middle right to 0.01 mm and the length 1 % off is it, too,
        // without the ends.
        let near = edges(&[("F1.b0", "E{F1:a|F1:e}", [1.005, 2.0, 0.0], arc, 4.356)]);
        assert_eq!(dressup_edge_match(&near, &f).unwrap().len(), 1);
        let far = edges(&[("F1.b0", "E{F1:a|F1:e}", [1.05, 2.0, 0.0], arc, 4.356)]);
        assert!(dressup_edge_match(&far, &f).is_err());
        // Another length (a different edge between the same points) is not it.
        let f = fp([1.0, 2.0, 0.0], 5.0, a, b, None);
        assert!(dressup_edge_match(&replay, &f).is_err());
        // Two edges between the ends of the same length: not told apart.
        let f = fp([1.0, 2.0, 0.0], 4.4, a, b, None);
        replay.0.push(replay.0[0].clone());
        replay.0[2].1 = "E{F1:a|F1:d}".parse().unwrap();
        replay.0[2].2 = [2.0, 1.0, 0.0];
        replay.1.push(Some([a, b]));
        assert!(matches!(
            dressup_edge_match(&replay, &f),
            Err(DressupEdgeFailure::AmbiguousInReplay(e)) if e.len() == 2
        ));
    }

    #[test]
    fn edges_split_in_several_pieces() {
        use std::f64::consts::{PI, TAU};
        let line = [1.0, 0.0, 0.0];
        // A line from 0 to 4.5 split in three, listed out of order, on
        // body 0; body 1 has the same pieces.
        let replay = edges(&[
            ("F1.b0", "E{F1:a|F1:c}", [2.25, 0.0, 0.0], line, 1.5),
            ("F1.b0", "E{F1:a|F1:b}", [0.75, 0.0, 0.0], line, 1.5),
            ("F1.b0", "E{F1:a|F1:d}", [3.75, 0.0, 0.0], line, 1.5),
            ("F2.b0", "E{F2:a|F2:b}", [0.75, 0.0, 0.0], line, 1.5),
            ("F2.b0", "E{F2:a|F2:c}", [2.25, 0.0, 0.0], line, 1.5),
            ("F2.b0", "E{F2:a|F2:d}", [3.75, 0.0, 0.0], line, 1.5),
        ]);
        let f = fp([2.25, 0.0, 0.0], 4.5, [0.0; 3], [4.5, 0.0, 0.0], None);
        let found = dressup_edge_match(&replay, &f);
        assert!(
            matches!(&found, Err(DressupEdgeFailure::AmbiguousInReplay(e)) if e.len() == 6),
            "{found:?}"
        );
        // Without the other body's pieces they are the three.
        let found = dressup_edge_match(
            &edges(&[
                ("F1.b0", "E{F1:a|F1:c}", [2.25, 0.0, 0.0], line, 1.5),
                ("F1.b0", "E{F1:a|F1:b}", [0.75, 0.0, 0.0], line, 1.5),
                ("F1.b0", "E{F1:a|F1:d}", [3.75, 0.0, 0.0], line, 1.5),
            ]),
            &f,
        )
        .unwrap();
        assert_eq!(found.len(), 3);
        // Pieces with a gap are not the edge.
        let gap = edges(&[
            ("F1.b0", "E{F1:a|F1:b}", [0.75, 0.0, 0.0], line, 1.5),
            ("F1.b0", "E{F1:a|F1:d}", [3.75, 0.0, 0.0], line, 1.5),
            ("F1.b0", "E{F1:a|F1:e}", [2.25, 0.0, 0.0], line, 0.5),
        ]);
        assert_eq!(
            dressup_edge_match(&gap, &f).unwrap_err(),
            DressupEdgeFailure::MissingInReplay
        );
        // A circle in three arcs, the file's seam elsewhere.
        let r = 5.0;
        let arc = |name: &str, a: f64| {
            let mid = [r * a.cos(), r * a.sin(), 0.0];
            (
                "F1.b0",
                name.to_owned(),
                mid,
                [-a.sin(), a.cos(), 0.0],
                TAU * r / 3.0,
            )
        };
        let list = [
            arc("E{F1:a|F1:b}", PI / 3.0),
            arc("E{F1:a|F1:c}", PI),
            arc("E{F1:a|F1:d}", 5.0 * PI / 3.0),
        ];
        let replay = ReplayEdges(
            list.iter()
                .map(|(b, e, p, t, l)| (b.parse().unwrap(), e.parse().unwrap(), *p, *t, *l))
                .collect(),
            Vec::new(),
        );
        let f = fp(
            [-r, 0.0, 0.0],
            TAU * r,
            [r, 0.0, 0.0],
            [r, 0.0, 0.0],
            Some([0.0, -1.0, 0.0]),
        );
        assert_eq!(dressup_edge_match(&replay, &f).unwrap().len(), 3);
        // The same circle cut at its quarter points: an open arc of half
        // of it in two pieces, found along the arc from its start.
        let arc = |name: &str, a: f64, l: f64| {
            let mid = [r * a.cos(), r * a.sin(), 0.0];
            ("F1.b0", name.to_owned(), mid, [-a.sin(), a.cos(), 0.0], l)
        };
        let list = [
            arc("E{F1:a|F1:b}", PI / 8.0, PI * r / 4.0),
            arc("E{F1:a|F1:c}", 3.0 * PI / 8.0, PI * r / 4.0),
        ];
        let replay = ReplayEdges(
            list.iter()
                .map(|(b, e, p, t, l)| (b.parse().unwrap(), e.parse().unwrap(), *p, *t, *l))
                .collect(),
            Vec::new(),
        );
        let f = fp(
            [r * (PI / 4.0).cos(), r * (PI / 4.0).sin(), 0.0],
            PI * r / 2.0,
            [r, 0.0, 0.0],
            [0.0, r, 0.0],
            None,
        );
        assert_eq!(dressup_edge_match(&replay, &f).unwrap().len(), 2);
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
        assert!(!found[0].chosen_from_multiple);
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
        assert!(found.iter().all(|e| !e.chosen_from_multiple));
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
        assert!(!found[0].chosen_from_multiple);
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

    #[test]
    fn the_axis_of_an_ellipse_follows_from_its_perimeter() {
        for (p, q) in [(8.0, 5.0), (5.0, 8.0), (20.0, 1.0), (3.0, 2.9)] {
            let found = ellipse_other_axis(p, ellipse_perimeter(p, q)).unwrap();
            assert!((found - q).abs() < 1e-9, "{p} {q} {found}");
        }
        // The perimeter of a circle, and none shorter than the rectangle.
        assert!((ellipse_perimeter(4.0, 4.0) - std::f64::consts::TAU * 4.0).abs() < 1e-12);
        assert!(ellipse_other_axis(8.0, 30.0).is_none());
    }

    #[test]
    fn closed_ellipses_whole_or_in_pieces() {
        let (p, q) = (8.0, 5.0);
        let length = ellipse_perimeter(p, q);
        // The file's ellipse about the origin in the plane z = 0 starts at
        // -x (an end of its major axis), has its middle at +x and runs
        // counter-clockwise (+y there).
        let file = |q: f64| {
            fp(
                [p, 0.0, 0.0],
                ellipse_perimeter(p, q),
                [-p, 0.0, 0.0],
                [-p, 0.0, 0.0],
                Some([0.0, 1.0, 0.0]),
            )
        };
        // The replay holds it whole with its seam elsewhere (middle at +y).
        let whole = edges(&[
            (
                "F1.b0",
                "E{F1:a|F1:b}",
                [0.0, q, 0.0],
                [-1.0, 0.0, 0.0],
                length,
            ),
            // A circle of the same length is not it.
            (
                "F1.b0",
                "E{F1:a|F1:c}",
                [0.0, length / std::f64::consts::TAU, 0.0],
                [-1.0, 0.0, 0.0],
                length,
            ),
        ]);
        let found = dressup_edge_match(&whole, &file(q)).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.to_string(), "E{F1:a|F1:b}");
        // Its direction at the replay's middle (+y): -x.
        let d = found[0].direction.unwrap();
        assert!(geom::distance(d, [-1.0, 0.0, 0.0]) < 1e-9, "{d:?}");
        // The replay splits it at the ends of the major axis.
        let halves = edges(&[
            (
                "F1.b0",
                "E{F1:a|F1:b}#0",
                [0.0, q, 0.0],
                [-1.0, 0.0, 0.0],
                length / 2.0,
            ),
            (
                "F1.b0",
                "E{F1:a|F1:b}#1",
                [0.0, -q, 0.0],
                [1.0, 0.0, 0.0],
                length / 2.0,
            ),
            // Another body's half on another ellipse.
            (
                "F1.b0",
                "E{F1:a|F1:d}",
                [0.0, q + 1.0, 0.0],
                [-1.0, 0.0, 0.0],
                length / 2.0,
            ),
        ]);
        let found = dressup_edge_match(&halves, &file(q)).unwrap();
        assert_eq!(found.len(), 2);
        let mut names: Vec<String> = found.iter().map(|e| e.name.to_string()).collect();
        names.sort();
        assert_eq!(names, ["E{F1:a|F1:b}#0", "E{F1:a|F1:b}#1"]);
        // Directions: counter-clockwise: -x at +y, +x at -y.
        let up = found.iter().find(|e| e.mid[1] > 0.0).unwrap();
        assert!(geom::distance(up.direction.unwrap(), [-1.0, 0.0, 0.0]) < 1e-9);
        let down = found.iter().find(|e| e.mid[1] < 0.0).unwrap();
        assert!(geom::distance(down.direction.unwrap(), [1.0, 0.0, 0.0]) < 1e-9);
        // An ellipse with the other axes is not the file's.
        assert!(dressup_edge_match(&whole, &file(q + 1.0)).is_err());
        // Without the file's direction the plane is not known: not found.
        let mut blind = file(q);
        blind.f3d = None;
        assert!(dressup_edge_match(&whole, &blind).is_err());
    }

    /// A point of the ellipse with the half axes 8 and 5 about the origin
    /// in the plane z = 0 at the parameter `t`, and the length of its arc
    /// between two parameters.
    fn on_ellipse(t: f64) -> Vec3 {
        [8.0 * t.cos(), 5.0 * t.sin(), 0.0]
    }

    fn arc_length(from: f64, to: f64) -> f64 {
        let n = 20_000;
        let step = (to - from) / n as f64;
        (0..n)
            .map(|k| {
                let t = from + step * (k as f64 + 0.5);
                (8.0 * t.sin()).hypot(5.0 * t.cos()) * step
            })
            .sum()
    }

    #[test]
    fn an_elliptical_arc_the_replay_splits_is_found_by_the_ends_of_its_pieces() {
        let (t0, ts, t1) = (0.3, 1.0, 1.9);
        // The replay's arcs: the file's edge split at `ts`, an arc from
        // another place, and the rest of the ellipse.
        let piece = |name: &str, from: f64, to: f64| {
            let mid = on_ellipse(0.5 * (from + to));
            (
                "F1.b0".parse::<BodyUid>().unwrap(),
                name.parse::<EdgeName>().unwrap(),
                mid,
                [0.0, 0.0, 1.0],
                arc_length(from, to),
                Some([on_ellipse(from), on_ellipse(to)]),
            )
        };
        let list = [
            piece("E{F1:a|F1:b}#0", t0, ts),
            piece("E{F1:a|F1:b}#1", ts, t1),
            piece("E{F1:a|F1:c}", t1, 3.0),
        ];
        let replay = ReplayEdges(
            list.iter()
                .map(|(b, n, m, t, l, _)| (*b, n.clone(), *m, *t, *l))
                .collect(),
            list.iter().map(|e| e.5).collect(),
        );
        // The file's edge (the middle at the middle of its parameter
        // range, a little off the middle of the replay's pieces).
        let file = |length: f64| {
            fp(
                on_ellipse(0.5 * (t0 + t1)),
                length,
                on_ellipse(t0),
                on_ellipse(t1),
                None,
            )
        };
        let found = dressup_edge_match(&replay, &file(arc_length(t0, t1))).unwrap();
        let mut names: Vec<String> = found.iter().map(|e| e.name.to_string()).collect();
        names.sort();
        assert_eq!(names, ["E{F1:a|F1:b}#0", "E{F1:a|F1:b}#1"]);
        assert!(found.iter().all(|e| !e.chosen_from_multiple));
        // Not the file's edge: another length, or a middle elsewhere.
        assert!(dressup_edge_match(&replay, &file(arc_length(t0, t1) + 1.0)).is_err());
        let mut elsewhere = file(arc_length(t0, t1));
        elsewhere.mid_point = Some([0.0, 0.0, 3.0]);
        assert!(dressup_edge_match(&replay, &elsewhere).is_err());
    }

    #[test]
    fn a_long_elliptical_piece_is_followed_by_its_arc_not_its_polyline() {
        // The file's middle lies on the second, long piece of the split
        // edge; the polyline through the piece's ends and middle is
        // further from it than 2 % of the edge, the arc through the
        // same three points is not.
        let (t0, ts, t1) = (0.0, 0.3, 2.7);
        let length = arc_length(t0, t1);
        let m = on_ellipse(0.5 * (t0 + t1));
        let (from, mid, to) = (on_ellipse(ts), on_ellipse(0.5 * (ts + t1)), on_ellipse(t1));
        let polyline = segment_distance(m, from, mid).min(segment_distance(m, mid, to));
        assert!(polyline > CHAIN_MID_OFF * length, "{polyline}");
        let arc = arc_distance(m, from, mid, to);
        assert!(arc < CHAIN_MID_OFF * length, "{arc}");
        // The arc ends where the piece does, and a point beyond them is
        // measured from the nearer end.
        assert!(arc_distance(on_ellipse(0.0), from, mid, to) > 1.0);
        assert!(arc_distance(from, from, mid, to) < 1e-9);
        // Three points in a line: the segment.
        assert!(
            (arc_distance([1.0, 1.0, 0.0], [0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]) - 1.0).abs()
                < 1e-12
        );
        // The whole chain is found.
        let piece = |name: &str, a: f64, b: f64| {
            (
                "F1.b0".parse::<BodyUid>().unwrap(),
                name.parse::<EdgeName>().unwrap(),
                on_ellipse(0.5 * (a + b)),
                [0.0, 0.0, 1.0],
                arc_length(a, b),
                Some([on_ellipse(a), on_ellipse(b)]),
            )
        };
        let list = [
            piece("E{F1:a|F1:b}#0", t0, ts),
            piece("E{F1:a|F1:b}#1", ts, t1),
        ];
        let replay = ReplayEdges(
            list.iter()
                .map(|(b, n, m, t, l, _)| (*b, n.clone(), *m, *t, *l))
                .collect(),
            list.iter().map(|e| e.5).collect(),
        );
        let file = fp(m, length, on_ellipse(t0), on_ellipse(t1), None);
        assert_eq!(dressup_edge_match(&replay, &file).unwrap().len(), 2);
    }

    #[test]
    fn a_long_helix_split_per_turn_is_found_along_its_faces() {
        // A helix of radius 28 rising 2 a turn, from 0.4 rad on over three
        // turns: one edge in the file, four in the replay, which splits its
        // thread's flank per turn (each piece between the crest and the
        // flank's turn). Beside its start a grid of 1 mm edges of other
        // faces, listed first: searched by their ends alone, the chains of
        // grid edges from the start ran out of steps before the helix's.
        let (r, c) = (28.0, 2.0 / std::f64::consts::TAU);
        let at = |t: f64| [r * t.cos(), r * t.sin(), c * t];
        let per = r.hypot(c);
        let (t0, t1) = (0.4, 3.0 * std::f64::consts::TAU + 0.4);
        // (Body, name, middle, tangent, length, ends.)
        type Entry = (BodyUid, EdgeName, Vec3, Vec3, f64, Option<[Vec3; 2]>);
        let mut list: Vec<Entry> = Vec::new();
        let body: BodyUid = "F1.b0".parse().unwrap();
        let a = at(t0);
        for i in 0..16 {
            for j in 0..16 {
                let p = geom::add(a, [0.0, i as f64, j as f64]);
                for (k, q) in [geom::add(p, [0.0, 1.0, 0.0]), geom::add(p, [0.0, 0.0, 1.0])]
                    .into_iter()
                    .enumerate()
                {
                    let name = format!("E{{F1:g{i}x{j}x{k}|F1:h{i}x{j}x{k}}}");
                    list.push((
                        body,
                        name.parse().unwrap(),
                        geom::scale(geom::add(p, q), 0.5),
                        [0.0, 0.0, 1.0],
                        1.0,
                        Some([p, q]),
                    ));
                }
            }
        }
        let turn = std::f64::consts::TAU;
        let cuts = [t0, turn, 2.0 * turn, 3.0 * turn, t1];
        for (k, w) in cuts.windows(2).enumerate() {
            let name = format!("E{{F1:crest|F1:flank{k}}}");
            list.push((
                body,
                name.parse().unwrap(),
                at(0.5 * (w[0] + w[1])),
                [0.0, 0.0, 1.0],
                per * (w[1] - w[0]),
                Some([at(w[0]), at(w[1])]),
            ));
        }
        let replay = ReplayEdges(
            list.iter()
                .map(|(b, n, m, t, l, _)| (*b, n.clone(), *m, *t, *l))
                .collect(),
            list.iter().map(|e| e.5).collect(),
        );
        let file = fp(at(0.5 * (t0 + t1)), per * (t1 - t0), a, at(t1), None);
        let found = dressup_edge_match(&replay, &file).unwrap();
        let mut names: Vec<String> = found.iter().map(|e| e.name.to_string()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "E{F1:crest|F1:flank0}",
                "E{F1:crest|F1:flank1}",
                "E{F1:crest|F1:flank2}",
                "E{F1:crest|F1:flank3}"
            ]
        );
    }

    #[test]
    fn the_edges_of_given_bodies_only() {
        let f = fp([0.0; 3], 10.0, [-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], None);
        let replay = edges(&[
            ("F1.b0", "E{F1:a|F1:b}", [0.0; 3], [1.0, 0.0, 0.0], 10.0),
            ("F2.b0", "E{F2:a|F2:b}", [0.0; 3], [1.0, 0.0, 0.0], 10.0),
        ]);
        let second: BodyUid = "F2.b0".parse().unwrap();
        let only = replay.only(&[second]);
        let matched = dressup_edge_match(&only, &f).unwrap();
        assert_eq!(matched[0].body, second);
        // One body's edge in one place is no choice among several.
        assert!(!matched[0].chosen_from_multiple);
        let third: BodyUid = "F3.b0".parse().unwrap();
        assert!(dressup_edge_match(&replay.only(&[third]), &f).is_err());
    }

    // The geometric tolerance (mitcad#224).

    #[test]
    fn a_circle_a_micrometre_off_is_found_within_the_geometric_tolerance() {
        use std::f64::consts::TAU;
        // The file's closed circle r = 23.5 at z = 33.100; the replay's at
        // z = 33.0989 (an earlier chamfer built 1.1 µm otherwise).
        let r = 23.5;
        let circle = TAU * r;
        let replay = edges(&[(
            "F1.b0",
            "E{F1:side|F1:top}",
            [0.0, r, 33.0989],
            [-1.0, 0.0, 0.0],
            circle,
        )]);
        let f = fp(
            [-r, 0.0, 33.1],
            circle,
            [r, 0.0, 33.1],
            [r, 0.0, 33.1],
            Some([0.0, -1.0, 0.0]),
        );
        // Only exact matches: the edge is not in the replay.
        {
            let _exact = GeometricTolerance::set(EXACT);
            assert_eq!(
                dressup_edge_match(&replay, &f).unwrap_err(),
                DressupEdgeFailure::MissingInReplay
            );
            assert_eq!(take_deviation(), None);
        }
        // The default 0.01 mm: found, unique (read), its deviation noted.
        assert_eq!(geometric_tolerance(), DEFAULT_GEOMETRIC_TOLERANCE);
        take_deviation();
        let matched = dressup_edge_match(&replay, &f).unwrap();
        assert_eq!(matched.len(), 1);
        assert!(!matched[0].chosen_from_multiple);
        let deviation = take_deviation().unwrap();
        assert!((deviation - 0.0011).abs() < 1e-6, "{deviation}");
        // An exact match notes nothing.
        let exact = edges(&[(
            "F1.b0",
            "E{F1:side|F1:top}",
            [0.0, r, 33.1],
            [-1.0, 0.0, 0.0],
            circle,
        )]);
        assert!(dressup_edge_match(&exact, &f).is_ok());
        assert_eq!(take_deviation(), None);
        // Beyond the tolerance it stays missing.
        let far = edges(&[(
            "F1.b0",
            "E{F1:side|F1:top}",
            [0.0, r, 33.08],
            [-1.0, 0.0, 0.0],
            circle,
        )]);
        assert_eq!(
            dressup_edge_match(&far, &f).unwrap_err(),
            DressupEdgeFailure::MissingInReplay
        );
    }

    #[test]
    fn open_edges_found_by_their_ends_within_the_geometric_tolerance() {
        // A curved edge whose middle the replay has elsewhere (a blend's
        // parameter middle) and whose length is 1 % off: found by its ends,
        // which are 4 µm off the file's.
        let f = fp(
            [0.0, 1.0, 0.0],
            10.5,
            [-5.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            None,
        );
        let replay = |ends: &[([f64; 3], [f64; 3])]| {
            ReplayEdges(
                ends.iter()
                    .enumerate()
                    .map(|(k, _)| {
                        (
                            "F1.b0".parse().unwrap(),
                            format!("E{{F1:a|F1:e{k}}}").parse().unwrap(),
                            [0.0, 0.9 - 0.5 * k as f64, 0.0],
                            [1.0, 0.0, 0.0],
                            10.6,
                        )
                    })
                    .collect(),
                ends.iter().map(|&(s, e)| Some([s, e])).collect(),
            )
        };
        let one = replay(&[([-5.0, 0.004, 0.0], [5.0, 0.004, 0.0])]);
        {
            let _exact = GeometricTolerance::set(EXACT);
            assert_eq!(
                dressup_edge_match(&one, &f).unwrap_err(),
                DressupEdgeFailure::MissingInReplay
            );
        }
        take_deviation();
        let matched = dressup_edge_match(&one, &f).unwrap();
        assert_eq!(matched.len(), 1);
        assert!(!matched[0].chosen_from_multiple);
        assert!((take_deviation().unwrap() - 0.004).abs() < 1e-9);
        // Two edges with such ends in different places: ambiguous, as
        // within the exact tolerance.
        let two = replay(&[
            ([-5.0, 0.004, 0.0], [5.0, 0.004, 0.0]),
            ([-5.0, -0.004, 0.0], [5.0, -0.004, 0.0]),
        ]);
        assert!(matches!(
            dressup_edge_match(&two, &f),
            Err(DressupEdgeFailure::AmbiguousInReplay(e)) if e.len() == 2
        ));
    }

    #[test]
    fn the_geometric_tolerance_is_set_per_thread_and_restored() {
        assert_eq!(geometric_tolerance(), DEFAULT_GEOMETRIC_TOLERANCE);
        {
            let _outer = GeometricTolerance::set(0.05);
            assert_eq!(geometric_tolerance(), 0.05);
            {
                // Not a tolerance: the default.
                let _bad = GeometricTolerance::set(-1.0);
                assert_eq!(geometric_tolerance(), DEFAULT_GEOMETRIC_TOLERANCE);
            }
            assert_eq!(geometric_tolerance(), 0.05);
            // Another thread has its own.
            let other = std::thread::spawn(geometric_tolerance).join().unwrap();
            assert_eq!(other, DEFAULT_GEOMETRIC_TOLERANCE);
        }
        assert_eq!(geometric_tolerance(), DEFAULT_GEOMETRIC_TOLERANCE);
    }

    #[test]
    fn both_edges_of_a_sliver_face_are_the_files_one_edge() {
        use std::f64::consts::TAU;
        // Two chamfers that meet in the file's circle r = 28 at z = 14.6:
        // the replay builds one 1e-3 smaller, which leaves a strip of the
        // cylinder 2 µm high between them.
        let r = 28.0;
        let circle = TAU * r;
        let f = fp(
            [r, 0.0, 14.6],
            circle,
            [-r, 0.0, 14.6],
            [-r, 0.0, 14.6],
            Some([0.0, 1.0, 0.0]),
        );
        let replay = |z: [f64; 2], faces: [&str; 2]| {
            edges(&[
                ("F1.b0", faces[0], [0.0, r, z[0]], [-1.0, 0.0, 0.0], circle),
                ("F1.b0", faces[1], [0.0, r, z[1]], [-1.0, 0.0, 0.0], circle),
            ])
        };
        let strip = ["E{F1:side|F2:chamfer}", "E{F1:side|F3:chamfer}"];
        // One edge within 1e-3, its twin 1.2 µm off: both.
        let found = dressup_edge_match(&replay([14.5995, 14.6012], strip), &f).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|e| !e.chosen_from_multiple));
        // (The import tries the edge alone first.)
        assert!(!found[0].sliver_twin && found[1].sliver_twin);
        // Both within 1e-3: both, not ambiguous.
        let found = dressup_edge_match(&replay([14.5995, 14.6005], strip), &f).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|e| !e.sliver_twin));
        // Only the exact tolerance: the one edge.
        {
            let _exact = GeometricTolerance::set(EXACT);
            let found = dressup_edge_match(&replay([14.5995, 14.6012], strip), &f).unwrap();
            assert_eq!(found.len(), 1);
        }
        // Edges that share no face are no strip: one edge, or ambiguous.
        let apart = ["E{F1:side|F2:chamfer}", "E{F4:side|F3:chamfer}"];
        let found = dressup_edge_match(&replay([14.5995, 14.6012], apart), &f).unwrap();
        assert_eq!(found.len(), 1);
        assert!(matches!(
            dressup_edge_match(&replay([14.5995, 14.6005], apart), &f),
            Err(DressupEdgeFailure::AmbiguousInReplay(_))
        ));
    }
}

/// The edges a dressup took away, against the file's states after and
/// before the item (mitcad#440).
#[cfg(test)]
mod gone_edge_tests {
    use super::*;
    use mitcad_model::KernelError;
    use mitcad_model::testing::MockShape;

    /// The angle between the faces of a nearly tangent edge, radians.
    const THETA: f64 = 0.03;

    /// A body of planar faces (a point on each and its outward normal, by
    /// name) whose edges are given by their faces, middles, tangents and
    /// lengths, with the distances of those middles from the file's states
    /// (each state a shape whose history is its name).
    struct Planes {
        faces: Vec<(&'static str, Vec3, Vec3)>,
        edges: Vec<(&'static str, &'static str, Vec3, Vec3, f64)>,
        distances: Vec<(&'static str, Vec3, f64)>,
    }

    impl Kernel for Planes {
        type Shape = MockShape;

        fn extrude(&self, _: &mitcad_model::ExtrudeSpec<'_>) -> Result<MockShape, KernelError> {
            Err(KernelError::Unsupported("extrude"))
        }

        fn solids(&self, shape: &MockShape) -> Result<Vec<MockShape>, KernelError> {
            Ok(vec![shape.clone()])
        }

        fn boolean(
            &self,
            _: mitcad_model::BooleanOp,
            _: &[&MockShape],
            _: &MockShape,
        ) -> Result<mitcad_model::BooleanOutput<MockShape>, KernelError> {
            Err(KernelError::Unsupported("boolean"))
        }

        fn fillet(
            &self,
            _: mitcad_model::FeatureUid,
            _: &MockShape,
            _: &[mitcad_model::FilletSet<'_>],
            _: bool,
        ) -> Result<MockShape, KernelError> {
            Err(KernelError::Unsupported("fillet"))
        }

        fn count_edges(&self, _: &MockShape, _: &EdgeName) -> usize {
            0
        }

        fn edge_middles(
            &self,
            _: &MockShape,
        ) -> Result<Vec<mitcad_model::EdgeMiddle>, KernelError> {
            Ok(self
                .edges
                .iter()
                .map(|&(a, b, point, tangent, length)| mitcad_model::EdgeMiddle {
                    name: edge(a, b).to_string(),
                    point,
                    tangent,
                    length,
                })
                .collect())
        }

        fn face_point_normal(
            &self,
            _: &MockShape,
            face: &FaceName,
            near: Vec3,
        ) -> Result<mitcad_model::datum::SurfacePoint, KernelError> {
            let &(_, origin, normal) = self
                .faces
                .iter()
                .find(|(name, _, _)| face.to_string() == *name)
                .ok_or(KernelError::Unsupported("face normals"))?;
            let off = geom::dot(geom::sub(near, origin), normal);
            Ok(mitcad_model::datum::SurfacePoint {
                point: geom::sub(near, geom::scale(normal, off)),
                normal,
            })
        }

        fn boundary_distances(
            &self,
            shape: &MockShape,
            points: &[Vec3],
        ) -> Result<Vec<f64>, KernelError> {
            Ok(points
                .iter()
                .map(|p| {
                    self.distances
                        .iter()
                        .find(|(state, q, _)| *state == shape.history && q == p)
                        .map_or(1.0, |&(_, _, d)| d)
                })
                .collect())
        }
    }

    fn edge(a: &str, b: &str) -> EdgeName {
        EdgeName::new(a.parse().expect("face"), b.parse().expect("face"))
    }

    fn state(name: &str) -> MockShape {
        MockShape {
            history: name.to_owned(),
            faces: Vec::new(),
            edges: Vec::new(),
            bounds: None,
            parts: Vec::new(),
        }
    }

    #[test]
    fn edges_gone_with_the_item_and_a_nearly_tangent_one() {
        let (s, c) = (THETA.sin(), THETA.cos());
        let kernel = Planes {
            faces: vec![
                ("F1:import(0)", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
                ("F1:import(1)", [10.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
                // Nearly tangent to the first, beyond x = -10.
                ("F1:import(2)", [-10.0, 0.0, 0.0], [-s, 0.0, c]),
                ("F1:import(3)", [0.0, 30.0, 0.0], [0.0, 1.0, 0.0]),
                ("F1:import(4)", [0.0, 30.0, 0.0], [0.0, 0.0, 1.0]),
                // Nearly tangent to the second, below z = -5.
                ("F1:import(5)", [10.0, 0.0, -5.0], [c, 0.0, -s]),
                ("F1:import(6)", [0.0, -30.0, 0.0], [0.0, 0.0, 1.0]),
                ("F1:import(7)", [0.0, -30.0, 0.0], [s, 0.0, c]),
            ],
            edges: vec![
                // The rounded edge (a quarter turn).
                (
                    "F1:import(0)",
                    "F1:import(1)",
                    [10.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    10.0,
                ),
                // Rounded too, its blend 0.015 wide: the item's state lies
                // 5.6e-5 from its middle.
                (
                    "F1:import(0)",
                    "F1:import(2)",
                    [-10.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    3.0,
                ),
                // An edge the replay built otherwise than the file, which
                // lacks it before the item as after it.
                (
                    "F1:import(3)",
                    "F1:import(4)",
                    [0.0, 30.0, 0.0],
                    [1.0, 0.0, 0.0],
                    4.0,
                ),
                // Nearly tangent on a face of the rounded edge, left.
                (
                    "F1:import(1)",
                    "F1:import(5)",
                    [10.0, 0.0, -5.0],
                    [0.0, 1.0, 0.0],
                    3.0,
                ),
                // Nearly tangent on faces of no edge gone: not looked at.
                (
                    "F1:import(6)",
                    "F1:import(7)",
                    [0.0, -30.0, 0.0],
                    [0.0, 1.0, 0.0],
                    3.0,
                ),
            ],
            // The replay lies 2 µm off the file's state before.
            distances: vec![
                ("after", [10.0, 0.0, 0.0], 0.5 * (2f64.sqrt() - 1.0)),
                ("before", [10.0, 0.0, 0.0], 2e-6),
                ("after", [-10.0, 0.0, 0.0], 5.8e-5),
                ("before", [-10.0, 0.0, 0.0], 2e-6),
                ("after", [0.0, 30.0, 0.0], 0.004),
                ("before", [0.0, 30.0, 0.0], 0.004),
                ("after", [10.0, 0.0, -5.0], 2e-6),
                ("before", [10.0, 0.0, -5.0], 2e-6),
                ("after", [0.0, -30.0, 0.0], 5.8e-5),
                ("before", [0.0, -30.0, 0.0], 2e-6),
            ],
        };
        let (replay, after, before) = (state("replay"), state("after"), state("before"));
        let depth = |wedge: f64| Section::Fillet(0.5).depth(wedge);
        let names = |gone: Vec<GoneEdge>| -> Vec<String> {
            gone.iter().map(|e| e.name.to_string()).collect()
        };
        let gone = consumed_edges(&kernel, &replay, &[&after], &[&before], &depth).expect("gone");
        let near = gone
            .iter()
            .find(|e| e.name == edge("F1:import(0)", "F1:import(2)"))
            .and_then(|e| e.wedge)
            .expect("the nearly tangent edge");
        assert!((near - (std::f64::consts::PI - THETA)).abs() < 1e-9);
        assert_eq!(
            names(gone),
            [
                edge("F1:import(0)", "F1:import(1)").to_string(),
                edge("F1:import(0)", "F1:import(2)").to_string()
            ]
        );
        // Without the state before: by the distances from the state after
        // alone.
        let gone = consumed_edges(&kernel, &replay, &[&after], &[], &depth).expect("gone");
        assert_eq!(
            names(gone),
            [
                edge("F1:import(0)", "F1:import(1)").to_string(),
                edge("F1:import(3)", "F1:import(4)").to_string()
            ]
        );
    }
}
