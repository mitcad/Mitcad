// SPDX-License-Identifier: MIT
//! Sketches on geometry of other components, and linked curves as
//! projections (mitcad#225).
//!
//! A sketch may lie on a face of a body of another component, and its
//! linked curves follow edges of the model, its own component's or
//! another's. The model keeps both through occurrence links (mitcad#100,
//! `mitcad_model::links`): a sketch's `plane_link` and a projection's
//! `link`, each the occurrence path from the root to the geometry's
//! component and the one to the sketch's component.
//!
//! The file stores neither the face a sketch lies on nor the geometry a
//! linked curve follows in a form the decoder names (`core/import/docs/f3d-learning.md`,
//! *Sketch curve flags, spline kinds and sketches without a link*): both
//! are found in the replay by where they are. A face is the sketch's plane
//! when it is the only planar face of the other components' bodies, seen
//! through their occurrences, in the sketch's plane (looking along the
//! sketch's normal, else against it). A linked curve follows an edge when
//! that edge is the only one whose projection the curve lies on (the
//! edges in the sketch's plane first, as the file projects the face a
//! sketch lies on), its curve is the edge's only curve, and the curve is
//! where the edge projects (the snap of `snap.rs` put it there). A linked
//! line or spline left follows an edge of any kind, or a chain of edges
//! end to end (mitcad#389: a thread's helix the replay splits per turn),
//! when the projection of the pieces lies on it within the geometric
//! tolerance and putting it there keeps the sketch's regions ([`chains`],
//! [`resolve_chains`]). Every other linked curve stays fixed reference
//! geometry, and the sketch's import note says how many.

use std::collections::HashMap;

use mitcad_model::assembly::inverse;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::features::SketchPlane;
use mitcad_model::sketch::EntityKind;
use mitcad_model::sketch::geometry::{Curve2, Nurbs};
use mitcad_model::sketch::project::{Projected, entities, project, transform_curve};
use mitcad_model::sketch::regions::{RegionCurve, regions};
use mitcad_model::{
    BodyUid, ComponentUid, Curve3, EntityUid, FaceName, OccurrenceUid, SketchFrame, TopoName,
    Transform,
};
use serde_json::{Value, json};

use crate::sketch::SketchParts;
use crate::{Importer, geom, refs, snap};

/// How far (mm) a linked curve's points and radius may lie from where its
/// source projects for the curve to be that projection: the snap put them
/// there, so only rounding is left.
const EXACT: f64 = 1e-6;

/// How far (mm) a face or an edge may lie off the sketch's plane to be in
/// it; the tolerance the sketch's own faces are found with. (A local
/// constant until the import's geometric tolerance option, mitcad#224,
/// replaces it.)
pub(crate) const IN_PLANE: f64 = 1e-4;

/// An occurrence path as the model writes it (`O1/O4`, empty for the
/// root).
pub(crate) fn path_text(path: &[OccurrenceUid]) -> String {
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// An occurrence link (`plane_link`, a projection's `link`).
pub(crate) fn link(source: &[OccurrenceUid], target: &[OccurrenceUid]) -> Value {
    let mut out = json!({});
    if !source.is_empty() {
        out["source"] = json!(path_text(source));
    }
    if !target.is_empty() {
        out["target"] = json!(path_text(target));
    }
    out
}

/// A planar face of another component's body, seen through one of its
/// occurrences, in the sketch's component's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceCandidate {
    /// From the root to the body's component.
    pub path: Vec<OccurrenceUid>,
    pub body: BodyUid,
    pub face: FaceName,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

/// The face a sketch with `frame` lies on among `candidates` (planar faces
/// of other components): those in the sketch's plane, looking along its
/// normal where any does; the only one, else the one that holds more of
/// the sketch's points than any other (`points_on` counts them), else
/// None.
pub(crate) fn plane_face(
    frame: &SketchFrame,
    candidates: Vec<FaceCandidate>,
    points_on: &mut dyn FnMut(&FaceCandidate) -> usize,
) -> Option<FaceCandidate> {
    let normal = geom::normal(frame);
    let mut on: Vec<FaceCandidate> = candidates
        .into_iter()
        .filter(|c| geom::coplanar(frame, c.origin, c.normal, IN_PLANE))
        .collect();
    if on.iter().any(|c| geom::dot(c.normal, normal) > 0.0) {
        on.retain(|c| geom::dot(c.normal, normal) > 0.0);
    }
    let mut on = unique(on, |a, b| {
        a.path == b.path && a.body == b.body && a.face == b.face
    });
    if on.len() == 1 {
        return on.pop();
    }
    let mut scored: Vec<(usize, FaceCandidate)> =
        on.into_iter().map(|c| (points_on(&c), c)).collect();
    scored.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    let best = matches!(scored.as_slice(),
        [(best, _), rest @ ..] if *best > 0 && rest.iter().all(|(n, _)| n < best));
    best.then(|| scored.swap_remove(0).1)
}

/// `items` without repeats (by `same`), in their order.
fn unique<T>(items: Vec<T>, same: impl Fn(&T, &T) -> bool) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for item in items {
        if !out.iter().any(|o| same(o, &item)) {
            out.push(item);
        }
    }
    out
}

/// An edge of the replay a linked curve may follow.
#[derive(Debug, Clone)]
pub struct EdgeSource {
    pub body: BodyUid,
    /// The edge's name, as the kernel lists it (empty for an edge without
    /// one: no source, but the snap may still put a curve on it).
    pub edge: String,
    /// From the root to the body's component; None for the sketch's own
    /// component (no link).
    pub path: Option<Vec<OccurrenceUid>>,
    /// The edge's curve in the sketch's component's coordinates.
    pub curve: CurveGeometry,
}

/// Linked curves made projections.
#[derive(Debug, Default)]
pub struct Resolved {
    /// The sketch definition's `projections`.
    pub projections: Vec<Value>,
    /// Of them, those of other components (with a link).
    pub other: usize,
}

/// The import notes on a sketch's `linked` curves: those `resolved` as
/// projections where the sketch came in with them (`used`), and the rest
/// kept fixed, with the reason where their sources were looked for
/// (`looked`), how many the snap put on the model's edges and how many it
/// `left` where the file stored them (a snap would change the regions).
pub(crate) fn notes(
    linked: usize,
    resolved: &Resolved,
    used: bool,
    looked: bool,
    snapped: usize,
    left: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    let made = if used { resolved.projections.len() } else { 0 };
    if made > 0 {
        out.push(format!(
            "{made} linked curves as projections{}",
            if resolved.other > 0 {
                format!(" ({} of other components)", resolved.other)
            } else {
                String::new()
            }
        ));
    }
    let rest = linked.saturating_sub(made);
    if rest > 0 {
        let mut why = Vec::new();
        if looked && !used && !resolved.projections.is_empty() {
            why.push("the sketch does not solve with them as projections".to_owned());
        } else if looked {
            why.push("no single edge of the replay they follow".to_owned());
        }
        if snapped > 0 {
            why.push(format!("{snapped} on the model's edges"));
        }
        if left > 0 {
            why.push(format!(
                "{left} left where the file stored them: a snap onto the model's edge would change the sketch's regions"
            ));
        }
        out.push(format!(
            "{rest} linked curves kept as fixed reference geometry{}",
            if why.is_empty() {
                String::new()
            } else {
                format!(" ({})", why.join("; "))
            }
        ));
    }
    out
}

/// The box of a sketch's point positions (sketch mm); None without points.
pub(crate) fn points_box(parts: &SketchParts) -> Option<([f64; 2], [f64; 2])> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for (_, p) in &parts.positions {
        for k in 0..2 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    (low[0].is_finite() && high[0].is_finite()).then_some((low, high))
}

/// Whether an edge lies in the sketch's plane.
fn in_plane(curve: &CurveGeometry, frame: &SketchFrame) -> bool {
    let n = geom::normal(frame);
    let on = |p: [f64; 3]| geom::dot(geom::sub(p, frame.origin), n).abs() <= IN_PLANE;
    match curve {
        CurveGeometry::Line { start, end } => on(*start) && on(*end),
        CurveGeometry::Circle { center, normal, .. } => {
            on(*center) && geom::norm(geom::cross(*normal, n)) < 1e-6
        }
        CurveGeometry::Other { .. } => false,
    }
}

/// The point ids of a sketch entity (JSON) in the order of
/// [`EntityKind::points`] for its type; None for other types.
fn point_keys(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "line" => &["start", "end"],
        "circle" => &["center"],
        "arc" => &["center", "start", "end"],
        "ellipse" => &["center", "major"],
        "elliptical_arc" => &["center", "major", "start", "end"],
        _ => return None,
    })
}

/// The position of point `id` among `entities`.
fn point_at(entities: &[Value], id: &str) -> Option<[f64; 2]> {
    let e = entities.iter().find(|e| e["id"] == id)?;
    let at = e["at"].as_array()?;
    Some([at.first()?.as_f64()?, at.get(1)?.as_f64()?])
}

/// The entity ids of a projection of `curves` (the source's, in the
/// sketch's component's coordinates) that the linked curve `entities[i]`
/// already is, in the order the model makes the projection's entities;
/// None unless the curve and its points are where the projection puts
/// them. A line the other way round is turned (`swap`).
fn as_projection(
    entities_json: &[Value],
    i: usize,
    curves: &[Curve3],
    frame: &SketchFrame,
) -> Option<(Vec<String>, bool)> {
    let e = &entities_json[i];
    let kind = e["type"].as_str()?;
    let keys = point_keys(kind)?;
    let mut counter = 0;
    let fresh = entities(&project(curves, frame), &mut || {
        counter += 1;
        EntityUid(counter)
    });
    let curve = fresh.last()?;
    // One curve and its own points.
    if fresh.len() != curve.kind.points().len() + 1 || curve.kind.type_name() != kind {
        return None;
    }
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) <= EXACT;
    let radius_ok = match &curve.kind {
        EntityKind::Circle { radius, .. } => e["radius"]
            .as_f64()
            .is_some_and(|r| (r - radius).abs() <= EXACT),
        EntityKind::Ellipse { minor_radius, .. }
        | EntityKind::EllipticalArc { minor_radius, .. } => e["minor_radius"]
            .as_f64()
            .is_some_and(|r| (r - minor_radius).abs() <= EXACT),
        _ => true,
    };
    if !radius_ok {
        return None;
    }
    let fresh_at = |id: EntityUid| {
        fresh.iter().find_map(|f| match f.kind {
            EntityKind::Point { at } if f.id == id => Some(at),
            _ => None,
        })
    };
    let points = curve.kind.points();
    let try_keys = |keys: &[&str]| -> Option<Vec<String>> {
        let mut map: Vec<(EntityUid, String)> = Vec::new();
        for (p, key) in points.iter().zip(keys) {
            let id = e[*key].as_str()?;
            if !near(fresh_at(*p)?, point_at(entities_json, id)?) {
                return None;
            }
            map.push((*p, id.to_owned()));
        }
        let mut refs = Vec::new();
        for f in &fresh {
            if f.id == curve.id {
                refs.push(e["id"].as_str()?.to_owned());
            } else {
                refs.push(map.iter().find(|(p, _)| *p == f.id)?.1.clone());
            }
        }
        Some(refs)
    };
    if let Some(refs) = try_keys(keys) {
        return Some((refs, false));
    }
    // A line the projection draws the other way round.
    if kind == "line" {
        let refs = try_keys(&["end", "start"])?;
        return Some((refs, true));
    }
    None
}

/// What the projection rule decides for a linked curve (`entity`, an
/// index into the sketch's entities): the distinct edges (indices into the
/// edge sources) whose projection it lies on, those in the sketch's plane
/// only where there are any; the curve follows the edge when there is one.
/// The snap takes the first of them too (`snap.rs`), so that the two agree
/// on the edge (mitcad#227).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub entity: usize,
    pub candidates: Vec<usize>,
}

impl Choice {
    /// The edge the curve follows, when the candidates name one.
    pub fn chosen(&self) -> Option<usize> {
        match self.candidates[..] {
            [k] => Some(k),
            _ => None,
        }
    }
}

/// The projection rule's choices for the linked curves of `parts` among
/// `edges` (in the space of the sketch's `frame`): curves no edge projects
/// onto are left out.
pub(crate) fn choices(
    parts: &SketchParts,
    frame: &SketchFrame,
    edges: &[EdgeSource],
) -> Vec<Choice> {
    let geometry: Vec<CurveGeometry> = edges.iter().map(|e| e.curve.clone()).collect();
    snap::sources(parts, frame, &geometry)
        .into_iter()
        .map(|(entity, found)| {
            let planar: Vec<usize> = found
                .iter()
                .copied()
                .filter(|k| in_plane(&edges[*k].curve, frame))
                .collect();
            let candidates = unique(if planar.is_empty() { found } else { planar }, |a, b| {
                let (a, b) = (&edges[*a], &edges[*b]);
                a.body == b.body && a.edge == b.edge && a.path == b.path
            });
            Choice { entity, candidates }
        })
        .collect()
}

/// Where a curve follows the coincident copies of one edge in several
/// bodies of the sketch's own component (a body and its copy in place,
/// mitcad#1175), the copy in `body`, the body whose face the sketch lies
/// on: each projects the same curve. The others stay as they are.
pub(crate) fn prefer_own_body(choices: &mut [Choice], edges: &[EdgeSource], body: BodyUid) {
    for choice in choices {
        if choice.candidates.len() < 2 {
            continue;
        }
        let first = &edges[choice.candidates[0]];
        let copies = choice.candidates.iter().all(|&k| {
            let e = &edges[k];
            e.path.is_none() && same_curve(&e.curve, &first.curve)
        });
        if !copies {
            continue;
        }
        let own: Vec<usize> = choice
            .candidates
            .iter()
            .copied()
            .filter(|&k| edges[k].body == body)
            .collect();
        if own.len() == 1 {
            choice.candidates = own;
        }
    }
}

/// Whether two edges' curves are the same within the geometric tolerance.
fn same_curve(a: &CurveGeometry, b: &CurveGeometry) -> bool {
    let tol = refs::geometric_tolerance();
    let near = |p: &[f64; 3], q: &[f64; 3]| geom::distance(*p, *q) <= tol;
    match (a, b) {
        (CurveGeometry::Line { start, end }, CurveGeometry::Line { start: s2, end: e2 }) => {
            near(start, s2) && near(end, e2)
        }
        (
            CurveGeometry::Circle {
                center,
                radius,
                start,
                end,
                ..
            },
            CurveGeometry::Circle {
                center: c2,
                radius: r2,
                start: s2,
                end: e2,
                ..
            },
        ) => near(center, c2) && (radius - r2).abs() <= tol && near(start, s2) && near(end, e2),
        (
            CurveGeometry::Other { kind, start, end },
            CurveGeometry::Other {
                kind: k2,
                start: s2,
                end: e2,
            },
        ) => kind == k2 && near(start, s2) && near(end, e2),
        _ => false,
    }
}

/// For the snap: each linked curve with the edge it is to be put on first,
/// the first of the projection rule's candidates.
pub(crate) fn preferred(choices: &[Choice]) -> Vec<(usize, usize)> {
    choices
        .iter()
        .filter_map(|c| Some((c.entity, *c.candidates.first()?)))
        .collect()
}

/// The linked curves of `parts` that follow an edge of `edges` (in the
/// space of the sketch's `frame`), as projections (see the module
/// documentation), by the projection rule's `choices`. `target` is the
/// occurrence path the sketch's component is seen at; `curves_of` gives
/// an edge's curves as the model's projection reads them
/// (`Kernel::curves_of`, moved into the sketch's component's
/// coordinates), None where the name does not resolve. Lines the
/// projection draws the other way round are turned in `parts`.
pub(crate) fn resolve(
    parts: &mut SketchParts,
    frame: &SketchFrame,
    edges: &[EdgeSource],
    choices: &[Choice],
    target: &[OccurrenceUid],
    curves_of: &mut dyn FnMut(&EdgeSource) -> Option<Vec<Curve3>>,
) -> Resolved {
    let mut out = Resolved::default();
    for choice in choices {
        let i = choice.entity;
        let Some(k) = choice.chosen() else {
            if crate::tracing() {
                eprintln!(
                    "import:   linked {} follows {} edges: kept fixed",
                    parts.entities[i]["id"],
                    choice.candidates.len()
                );
            }
            continue;
        };
        let source = &edges[k];
        let Some(curves) = curves_of(source) else {
            continue;
        };
        let Some((refs, swap)) = as_projection(&parts.entities, i, &curves, frame) else {
            if crate::tracing() {
                eprintln!(
                    "import:   linked {} is not where {} projects: kept fixed",
                    parts.entities[i]["id"], source.edge
                );
            }
            continue;
        };
        if swap {
            let e = &mut parts.entities[i];
            let (start, end) = (e["start"].clone(), e["end"].clone());
            e["start"] = end;
            e["end"] = start;
        }
        let mut projection = json!({"source": source.edge, "body": source.body.to_string(),
                                    "entities": refs});
        if let Some(path) = &source.path {
            projection["link"] = link(path, target);
            out.other += 1;
        }
        out.projections.push(projection);
    }
    out
}

/// A linked curve of the sketch as a curve of the sketch's plane (sketch
/// mm): a line, or a spline from its control points; None for other kinds.
fn file_curve(entities: &[Value], e: &Value) -> Option<Curve2> {
    match e["type"].as_str()? {
        "line" => Some(Curve2::Line {
            a: point_at(entities, e["start"].as_str()?)?,
            b: point_at(entities, e["end"].as_str()?)?,
        }),
        "spline" => {
            let control = e["control"]
                .as_array()?
                .iter()
                .map(|id| point_at(entities, id.as_str()?))
                .collect::<Option<Vec<_>>>()?;
            let degree = e["degree"].as_u64()? as usize;
            let knots: Vec<f64> = serde_json::from_value(e["knots"].clone()).ok()?;
            let weights: Vec<f64> =
                serde_json::from_value(e["weights"].clone()).unwrap_or_default();
            (knots.len() == control.len() + degree + 1).then_some(Curve2::Nurbs(Nurbs {
                degree,
                control,
                weights,
                knots,
            }))
        }
        _ => None,
    }
}

/// How far (mm) the points of `piece` lie from `curve` at most, sampled.
fn off_curve(piece: &Curve2, curve: &Curve2) -> f64 {
    let (t0, t1) = piece.domain();
    piece
        .sample(t0, t1)
        .into_iter()
        .map(|p| curve.closest(p).1)
        .fold(0.0, f64::max)
}

fn same_point(a: [f64; 2], b: [f64; 2], tolerance: f64) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= tolerance
}

/// A linked curve that is the projection of a chain of edges (mitcad#389):
/// the entity, the edges (indices into the edge sources) in order along
/// it, and their curves (as `curves_of` gives them) joined into one.
#[derive(Debug, Clone)]
pub struct Chain {
    pub entity: usize,
    pub edges: Vec<usize>,
    pub curve: Curve3,
}

/// The linked lines and splines of `parts` (other than `skip`, those made
/// projections already) that are the projection of one edge, or of
/// several edges end to end (a thread's helix the replay splits into a
/// piece per turn), of one body seen through one occurrence path: every
/// sampled point of each edge's projection within `tolerance` of the
/// curve, the pieces following one another from the curve's start to its
/// end, one way only, and their projection joined of the curve's kind.
/// `curves_of` gives an edge's curves in the sketch's component's
/// coordinates (one curve, or it is no piece).
pub(crate) fn chains(
    parts: &SketchParts,
    frame: &SketchFrame,
    edges: &[EdgeSource],
    skip: &[usize],
    tolerance: f64,
    curves_of: &mut dyn FnMut(&EdgeSource) -> Option<Vec<Curve3>>,
) -> Vec<Chain> {
    let mut out = Vec::new();
    // Each edge's projection, read once, for the curves that need them.
    let mut projected: HashMap<usize, Option<(Curve3, Curve2)>> = HashMap::new();
    for (i, e) in parts.entities.iter().enumerate() {
        if e["reference"] != json!(true) || skip.contains(&i) {
            continue;
        }
        let Some(curve) = file_curve(&parts.entities, e) else {
            continue;
        };
        let (t0, t1) = curve.domain();
        let (low, high) = curve.bounds();
        let (start, end) = (curve.point(t0), curve.point(t1));
        // The pieces on the curve, each turned to run along it.
        let mut pieces: Vec<(usize, f64, [f64; 2], [f64; 2])> = Vec::new();
        for (k, source) in edges.iter().enumerate() {
            if source.edge.is_empty() {
                continue;
            }
            let entry = projected.entry(k).or_insert_with(|| {
                let curves = curves_of(source)?;
                let [c] = curves.as_slice() else {
                    return None;
                };
                match project(std::slice::from_ref(c), frame).pop()? {
                    Projected::Curve(p) => Some((c.clone(), p)),
                    Projected::Point(_) => None,
                }
            });
            let Some((_, piece)) = entry else {
                continue;
            };
            let Some((a, b)) = piece.ends() else {
                continue;
            };
            let (lo, hi) = piece.bounds();
            if (0..2).any(|n| hi[n] < low[n] - tolerance || lo[n] > high[n] + tolerance)
                || off_curve(piece, &curve) > tolerance
            {
                continue;
            }
            let (ta, tb) = (curve.closest(a).0, curve.closest(b).0);
            if ta <= tb {
                pieces.push((k, ta, a, b));
            } else {
                pieces.push((k, tb, b, a));
            }
        }
        let same_edge = |x: usize, y: usize| {
            let (x, y) = (&edges[x], &edges[y]);
            x.body == y.body && x.edge == y.edge && x.path == y.path
        };
        // From the curve's start to its end, one piece after another.
        let mut chain: Vec<usize> = Vec::new();
        let mut at = start;
        let mut done = false;
        for _ in 0..pieces.len() {
            let next: Vec<&(usize, f64, [f64; 2], [f64; 2])> = pieces
                .iter()
                .filter(|p| same_point(p.2, at, tolerance) && !chain.contains(&p.0))
                .collect();
            let Some(first) = next.first() else {
                break;
            };
            if next.iter().any(|p| !same_edge(p.0, first.0)) {
                chain.clear();
                break;
            }
            if let Some(last) = chain.last()
                && (edges[*last].body != edges[first.0].body
                    || edges[*last].path != edges[first.0].path)
            {
                chain.clear();
                break;
            }
            chain.push(first.0);
            at = first.3;
            if same_point(at, end, tolerance) {
                done = true;
                break;
            }
        }
        if !done || chain.is_empty() {
            continue;
        }
        let curves: Vec<Curve3> = chain
            .iter()
            .filter_map(|k| {
                projected
                    .get(k)
                    .and_then(|p| p.as_ref())
                    .map(|p| p.0.clone())
            })
            .collect();
        let Some(joined) = mitcad_model::sketch::project::join_curves(&curves) else {
            continue;
        };
        // Of the curve's kind, and all of it on the joined curve.
        let Some(Projected::Curve(whole)) = project(std::slice::from_ref(&joined), frame).pop()
        else {
            continue;
        };
        let kind_ok = matches!(
            (&whole, e["type"].as_str()),
            (Curve2::Line { .. }, Some("line")) | (Curve2::Nurbs(_), Some("spline"))
        );
        if !kind_ok || off_curve(&curve, &whole) > tolerance {
            if crate::tracing() {
                eprintln!(
                    "import:   linked {} on {} edges: not a projection of its kind",
                    e["id"],
                    chain.len()
                );
            }
            continue;
        }
        if crate::tracing() {
            eprintln!(
                "import:   linked {} on {} edges, {:.2e} mm off",
                e["id"],
                chain.len(),
                off_curve(&curve, &whole).max(off_curve(&whole, &curve))
            );
        }
        out.push(Chain {
            entity: i,
            edges: chain,
            curve: joined,
        });
    }
    out
}

/// The regions the sketch's curves (not construction ones) bound, where
/// the entities place them with the points the file holds on a curve
/// (`coincident` with a curve) on it and those it holds on a point at that
/// point, as the solver puts them: a change
/// that keeps them keeps the profiles the features after the sketch name.
/// Each region's key with its curves' directions left out (a line turned
/// to run as its edge bounds the same region).
fn region_keys(entities: &[Value], constraints: &[Value]) -> Vec<String> {
    let mut held: Vec<Value> = entities.to_vec();
    for c in constraints {
        let (Some("coincident"), Some(point), Some(on)) = (
            c["type"].as_str(),
            c["point"].as_str(),
            c["entity"].as_str(),
        ) else {
            continue;
        };
        // A point coincident with a point is where that one is, as the
        // solver puts it (a corner on a projected curve's end that the
        // replay's edge gives micrometres off the file's).
        if let Some(target) = held
            .iter()
            .find(|e| e["id"] == on && e["type"] == "point")
            .map(|e| e["at"].clone())
            .filter(|at| at.is_array())
        {
            if let Some(p) = held
                .iter_mut()
                .find(|e| e["id"] == point && e["type"] == "point")
            {
                p["at"] = target;
            }
            continue;
        }
        let Some(curve) = held
            .iter()
            .find(|e| e["id"] == on && e["type"] != "point")
            .and_then(|e| entity_curve(&held, e))
        else {
            continue;
        };
        if let Some(p) = held
            .iter_mut()
            .find(|e| e["id"] == point && e["type"] == "point")
            && let Some(at) = p["at"]
                .as_array()
                .and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]))
        {
            p["at"] = json!(curve.point(curve.closest(at).0));
        }
    }
    let curves: Vec<RegionCurve> = held
        .iter()
        .filter(|e| e["construction"] != json!(true) && e["type"] != "point")
        .filter_map(|e| {
            let curve = entity_curve(&held, e)?;
            let n: u32 = e["id"].as_str()?.get(1..)?.parse().ok()?;
            Some(RegionCurve::new(EntityUid(n), curve))
        })
        .collect();
    let mut keys: Vec<String> = regions(&curves)
        .into_iter()
        .map(|r| unoriented(&r.profile.key.to_string()))
        .collect();
    keys.sort();
    keys
}

/// Whether the sketch solved bounds the regions the file's numbers do
/// ([`region_keys`]): `solved` the points where the solver put them
/// (mitcad#396; a solve that moves points within the import's geometric
/// tolerance is the file's sketch only where its regions stay).
pub(crate) fn regions_kept(
    entities: &[Value],
    constraints: &[Value],
    solved: &std::collections::BTreeMap<EntityUid, [f64; 2]>,
) -> bool {
    let moved: Vec<Value> = entities
        .iter()
        .map(|e| {
            let mut e = e.clone();
            if e["type"] == "point"
                && let Some(at) = e["id"]
                    .as_str()
                    .and_then(|id| id.strip_prefix('p')?.parse::<u32>().ok())
                    .and_then(|n| solved.get(&EntityUid(n)))
            {
                e["at"] = json!(at);
            }
            e
        })
        .collect();
    region_keys(entities, constraints) == region_keys(&moved, constraints)
}

/// A sketch curve entity as a curve of the plane.
fn entity_curve(entities: &[Value], e: &Value) -> Option<Curve2> {
    let at = |id: &Value| id.as_str().and_then(|id| point_at(entities, id));
    match e["type"].as_str()? {
        "spline" => file_curve(entities, e),
        _ => snap::curve_of(e, &at, None),
    }
}

/// A region key with each segment's two neighbours in order, and the
/// segments sorted: `r{c3[c1,c2],c1[c3,c2]}` and `r{c1[c2,c3],c3[c1,c2]}`
/// are the same.
fn unoriented(key: &str) -> String {
    let inner = key
        .strip_prefix("r{")
        .and_then(|k| k.strip_suffix('}'))
        .unwrap_or(key);
    let mut segments: Vec<String> = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (i, ch) in inner.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                segments.push(inner[start..i].to_owned());
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(inner[start..].to_owned());
    let mut segments: Vec<String> = segments
        .into_iter()
        .map(|s| match (s.find('['), s.find(']')) {
            (Some(open), Some(close)) if close > open => {
                let mut ends: Vec<&str> = s[open + 1..close].split(',').collect();
                ends.sort_unstable();
                format!("{}[{}]{}", &s[..open], ends.join(","), &s[close + 1..])
            }
            _ => s,
        })
        .collect();
    segments.sort();
    format!("r{{{}}}", segments.join(","))
}

/// The linked curves that are projections of `found` chains of edges
/// (mitcad#389) put where the projection of the joined edges is, with the
/// projections recording them (a chain of several edges as the first
/// edge's projection with the rest in its `chain`). A line keeps its
/// points; a spline its end points, its other control points made anew
/// (the file's are dropped). `target` is the occurrence path the sketch's
/// component is seen at.
pub(crate) fn resolve_chains(
    parts: &mut SketchParts,
    frame: &SketchFrame,
    edges: &[EdgeSource],
    found: &[Chain],
    target: &[OccurrenceUid],
    out: &mut Resolved,
) {
    let first_new = parts
        .entities
        .iter()
        .filter_map(|e| e["id"].as_str()?.get(1..)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    // (By id: a spline's replaced control points leave the entities.)
    let found: Vec<(Value, &Chain)> = found
        .iter()
        .map(|c| (parts.entities[c.entity]["id"].clone(), c))
        .collect();
    let before = region_keys(&parts.entities, &parts.constraints);
    let saved = (parts.entities.clone(), parts.positions.clone());
    // All of them at once (curves the file's numbers join where the
    // replay's edges meet: each alone would open the loop), else one at a
    // time.
    let mut next = first_new;
    let made: Vec<(Value, bool)> = found
        .iter()
        .filter_map(|(id, chain)| apply_chain(parts, frame, edges, id, chain, target, &mut next))
        .collect();
    let after = region_keys(&parts.entities, &parts.constraints);
    if crate::tracing() && !made.is_empty() && after != before {
        eprintln!(
            "import:   linked curves on chains of edges: the regions {:?} would be {:?}",
            before
                .iter()
                .filter(|k| !after.contains(k))
                .collect::<Vec<_>>(),
            after
                .iter()
                .filter(|k| !before.contains(k))
                .collect::<Vec<_>>()
        );
    }
    if !made.is_empty() && after == before {
        for (projection, other) in made {
            out.other += usize::from(other);
            out.projections.push(projection);
        }
        return;
    }
    (parts.entities, parts.positions) = saved;
    let mut next = first_new;
    for (id, chain) in &found {
        let saved = (parts.entities.clone(), parts.positions.clone());
        let Some((projection, other)) =
            apply_chain(parts, frame, edges, id, chain, target, &mut next)
        else {
            continue;
        };
        // Where the replay's curve would change the regions the file's
        // numbers make (as a snap must not, mitcad#227), the curve stays.
        if region_keys(&parts.entities, &parts.constraints) != before {
            if crate::tracing() {
                eprintln!(
                    "import:   linked {id} on {} edges: kept, the regions would change",
                    chain.edges.len()
                );
            }
            (parts.entities, parts.positions) = saved;
            continue;
        }
        out.other += usize::from(other);
        out.projections.push(projection);
    }
}

/// One linked curve of [`resolve_chains`] put where its chain projects:
/// its projection and whether it is of another component; None, `parts`
/// as it was, where it cannot be.
fn apply_chain(
    parts: &mut SketchParts,
    frame: &SketchFrame,
    edges: &[EdgeSource],
    id: &Value,
    chain: &Chain,
    target: &[OccurrenceUid],
    next: &mut u32,
) -> Option<(Value, bool)> {
    let i = parts.entities.iter().position(|x| x["id"] == *id)?;
    let saved = (parts.entities.clone(), parts.positions.clone());
    let mut counter = 0;
    let fresh = entities(
        &project(std::slice::from_ref(&chain.curve), frame),
        &mut || {
            counter += 1;
            EntityUid(counter)
        },
    );
    let curve = fresh.last()?;
    let at = |id: EntityUid| {
        fresh.iter().find_map(|f| match f.kind {
            EntityKind::Point { at } if f.id == id => Some(at),
            _ => None,
        })
    };
    let points = curve.kind.points();
    let e = parts.entities[i].clone();
    // The file's points the fresh ones become, by position: a line's
    // ends, a spline's end control points; the rest are new.
    let ends: Vec<String> = match e["type"].as_str() {
        Some("line") => vec![
            e["start"].as_str().unwrap_or_default().to_owned(),
            e["end"].as_str().unwrap_or_default().to_owned(),
        ],
        _ => {
            let control = e["control"].as_array().cloned().unwrap_or_default();
            vec![
                control
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                control
                    .last()
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ]
        }
    };
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return None;
    };
    // (The projection may run the other way round.)
    let swap = match (
        at(*first),
        point_at(&parts.entities, &ends[0]),
        point_at(&parts.entities, &ends[1]),
    ) {
        (Some(a), Some(s), Some(t)) => {
            (a[0] - t[0]).hypot(a[1] - t[1]) < (a[0] - s[0]).hypot(a[1] - s[1])
        }
        _ => false,
    };
    let mut ids: HashMap<EntityUid, String> = HashMap::new();
    ids.insert(*first, ends[usize::from(swap)].clone());
    ids.insert(*last, ends[1 - usize::from(swap)].clone());
    ids.insert(curve.id, e["id"].as_str().unwrap_or_default().to_owned());
    // A spline's other control points, new; the file's go.
    if e["type"] == "spline" {
        let old: Vec<String> = e["control"]
            .as_array()
            .map(|c| {
                c.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .filter(|id| !ends.contains(id))
                    .collect()
            })
            .unwrap_or_default();
        let used_elsewhere = |id: &str| {
            parts.entities.iter().enumerate().any(|(k, x)| {
                k != i && x["id"] != id && x.to_string().contains(&format!("\"{id}\""))
            }) || parts
                .constraints
                .iter()
                .chain(&parts.dimensions)
                .any(|c| c.to_string().contains(&format!("\"{id}\"")))
        };
        if old.iter().any(|id| used_elsewhere(id)) {
            return None;
        }
        parts.entities.retain(|x| {
            !x["id"]
                .as_str()
                .is_some_and(|id| old.iter().any(|o| o == id))
        });
        let old_numbers: Vec<u32> = old
            .iter()
            .filter_map(|id| id.get(1..)?.parse().ok())
            .collect();
        parts.positions.retain(|(n, _)| !old_numbers.contains(n));
        for p in &points[1..points.len() - 1] {
            *next += 1;
            let next = *next;
            ids.insert(*p, format!("p{next}"));
            if let Some(xy) = at(*p) {
                parts.entities.push(
                    json!({"id": format!("p{next}"), "type": "point", "at": xy, "fixed": true}),
                );
                parts.positions.push((next, xy));
            }
        }
    }
    let Some(i) = parts.entities.iter().position(|x| x["id"] == e["id"]) else {
        (parts.entities, parts.positions) = saved;
        return None;
    };
    // The curve and its points where the projection puts them.
    for p in &points {
        let (Some(id), Some(xy)) = (ids.get(p), at(*p)) else {
            continue;
        };
        if let Some(x) = parts.entities.iter_mut().find(|x| x["id"] == id.as_str()) {
            x["at"] = json!(xy);
        }
        if let Some(n) = id.get(1..).and_then(|n| n.parse::<u32>().ok())
            && let Some(pos) = parts.positions.iter_mut().find(|(m, _)| *m == n)
        {
            pos.1 = xy;
        }
    }
    let entity = &mut parts.entities[i];
    match &curve.kind {
        EntityKind::Line { .. } => {
            entity["start"] = json!(ids[first]);
            entity["end"] = json!(ids[last]);
        }
        EntityKind::Spline {
            degree,
            control,
            weights,
            knots,
        } => {
            entity["degree"] = json!(degree);
            entity["control"] = json!(control.iter().map(|p| ids[p].clone()).collect::<Vec<_>>());
            entity["knots"] = json!(knots);
            if weights.is_empty() {
                if let Some(m) = entity.as_object_mut() {
                    m.remove("weights");
                }
            } else {
                entity["weights"] = json!(weights);
            }
        }
        _ => {
            (parts.entities, parts.positions) = saved;
            return None;
        }
    }
    let refs: Vec<String> = fresh
        .iter()
        .filter_map(|f| ids.get(&f.id).cloned())
        .collect();
    let source = &edges[chain.edges[0]];
    let mut projection = json!({"source": source.edge, "body": source.body.to_string(),
                                "entities": refs});
    if chain.edges.len() > 1 {
        projection["chain"] = json!(
            chain.edges[1..]
                .iter()
                .map(|k| edges[*k].edge.clone())
                .collect::<Vec<_>>()
        );
    }
    if let Some(path) = &source.path {
        projection["link"] = link(path, target);
    }
    Some((projection, source.path.is_some()))
}

/// A curve of an edge moved by a rigid transform (lines and circles; the
/// rest is no source [`snap::sources`] matches).
fn moved(t: &Transform, curve: &CurveGeometry) -> Option<CurveGeometry> {
    Some(match curve {
        CurveGeometry::Line { start, end } => CurveGeometry::Line {
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        CurveGeometry::Circle {
            center,
            normal,
            radius,
            start,
            end,
        } => CurveGeometry::Circle {
            center: t.apply_point(*center),
            normal: t.apply_vector(*normal),
            radius: *radius,
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        CurveGeometry::Other { .. } => return None,
    })
}

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// The occurrence path the item's component is seen at: the first of
    /// its paths (the original the file's features are made in, as
    /// [`Importer::original_placement_at`]); None where it is placed
    /// nowhere.
    pub(crate) fn sketch_target(&self) -> Option<Vec<OccurrenceUid>> {
        self.doc
            .assembly()
            .paths_to(self.component)
            .into_iter()
            .min()
    }

    /// From the coordinates of the occurrence path `path`'s component into
    /// the item's component's, seen at `target`, by the placements at the
    /// marker (those the model moves linked geometry by).
    fn to_target(&self, path: &[OccurrenceUid], target: &[OccurrenceUid]) -> Transform {
        inverse(&self.doc.path_transform(target)).after(&self.doc.path_transform(path))
    }

    /// The planar face of another component's body, seen through one of
    /// its occurrences, that a sketch of the item's component with `frame`
    /// lies on (see the module documentation): the definition's `plane`
    /// and `plane_link`, the face's frame in the sketch's component and
    /// an import note.
    pub(crate) fn linked_face_plane(
        &self,
        frame: &SketchFrame,
        points: &[(u32, [f64; 2])],
    ) -> Option<(Value, Value, SketchFrame, String)> {
        let target = self.sketch_target()?;
        let into = inverse(&self.doc.path_transform(&target));
        let mut planes: HashMap<BodyUid, Vec<(FaceName, mitcad_model::Plane)>> = HashMap::new();
        for (body, face, plane) in refs::planar_faces(self.doc) {
            planes.entry(body).or_default().push((face, plane));
        }
        let mut candidates = Vec::new();
        let sources = self.derive_sources();
        for instance in self.doc.instances() {
            if instance.component == self.component
                || instance.path.iter().any(|o| sources.contains(o))
            {
                continue;
            }
            let Some(faces) = planes.get(&instance.body) else {
                continue;
            };
            let t = into.after(&instance.transform);
            for (face, plane) in faces {
                let (origin, normal) = (t.apply_point(plane.origin), t.apply_vector(plane.normal));
                if geom::coplanar(frame, origin, normal, IN_PLANE) {
                    candidates.push(FaceCandidate {
                        path: instance.path.clone(),
                        body: instance.body,
                        face: face.clone(),
                        origin,
                        normal,
                    });
                }
            }
        }
        if crate::tracing() && !candidates.is_empty() {
            let n = geom::normal(frame);
            eprintln!(
                "import:   {} faces of other components in the sketch's plane",
                candidates.len()
            );
            for c in candidates.iter().filter(|c| geom::dot(c.normal, n) > 0.0) {
                eprintln!(
                    "import:     along its normal: {} {} at {}",
                    c.body,
                    c.face,
                    self.doc.assembly().path_name(&c.path)
                );
            }
        }
        // The sketch's points (at most 256 of them) in a candidate's body.
        let step = points.len().div_ceil(256).max(1);
        let kernel = self.doc.kernel();
        let mut points_on = |c: &FaceCandidate| -> usize {
            let Some(shape) = crate::body_scope::shape(self.doc, c.body) else {
                return 0;
            };
            let back = inverse(&into.after(&self.doc.path_transform(&c.path)));
            points
                .iter()
                .step_by(step)
                .filter(|(_, p)| {
                    let at = back.apply_point(frame.point(*p));
                    crate::tick();
                    kernel
                        .face_point_normal(shape, &c.face, at)
                        .is_ok_and(|near| geom::distance(near.point, at) <= IN_PLANE)
                })
                .count()
        };
        let c = plane_face(frame, candidates, &mut points_on)?;
        let f = SketchPlane::face_frame(c.origin, c.normal);
        let note = format!(
            "on a face of {} (its occurrence {})",
            self.doc.assembly().name(
                self.doc
                    .body_component(c.body)
                    .unwrap_or(ComponentUid::ROOT)
            ),
            self.doc.assembly().path_name(&c.path)
        );
        Some((
            json!({"face": c.face.to_string(), "body": c.body.to_string()}),
            link(&c.path, &target),
            f,
            note,
        ))
    }

    /// The edges a linked curve of a sketch of the item's component seen at
    /// `target` may follow: its own component's, and those of the other
    /// components' bodies through each of their occurrences, moved into
    /// its coordinates; bodies whose box does not come near `near` (the
    /// box of the sketch's points in `frame`, mm) are left out.
    pub(crate) fn edge_sources(
        &self,
        target: &[OccurrenceUid],
        frame: &SketchFrame,
        near: Option<([f64; 2], [f64; 2])>,
    ) -> Vec<EdgeSource> {
        let kernel = self.doc.kernel();
        let into = inverse(&self.doc.path_transform(target));
        let mut out = Vec::new();
        let mut edges_of: HashMap<BodyUid, Vec<(Option<String>, CurveGeometry)>> = HashMap::new();
        let mut read = |body: BodyUid| -> Option<Vec<(Option<String>, CurveGeometry)>> {
            if let Some(edges) = edges_of.get(&body) {
                return Some(edges.clone());
            }
            crate::tick();
            let edges = kernel
                .edge_geometries(crate::body_scope::shape(self.doc, body)?)
                .ok()?;
            edges_of.insert(body, edges.clone());
            Some(edges)
        };
        for body in self.doc.component_bodies(self.component) {
            // (Unnamed edges too: the snap puts curves on them.)
            for (name, curve) in read(body).unwrap_or_default() {
                out.push(EdgeSource {
                    body,
                    edge: name.unwrap_or_default(),
                    path: None,
                    curve,
                });
            }
        }
        let comes_near = |shape: &K::Shape, t: &Transform| -> bool {
            let Some((low, high)) = near else {
                return true;
            };
            let Ok(Some(b)) = kernel.bounding_box(shape) else {
                return true;
            };
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for i in 0..8 {
                let corner =
                    std::array::from_fn(|k| if i >> k & 1 == 0 { b.min[k] } else { b.max[k] });
                let p = geom::to_sketch(frame, t.apply_point(corner));
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            let margin = 1.0;
            (0..2).all(|k| hi[k] >= low[k] - margin && lo[k] <= high[k] + margin)
        };
        // (Not through derives' hidden sources, mitcad#324.)
        let sources = self.derive_sources();
        for instance in self.doc.instances() {
            if instance.component == self.component
                || instance.path.iter().any(|o| sources.contains(o))
            {
                continue;
            }
            let t = into.after(&instance.transform);
            let Some(shape) = crate::body_scope::shape(self.doc, instance.body) else {
                continue;
            };
            if !comes_near(shape, &t) {
                continue;
            }
            for (name, curve) in read(instance.body).unwrap_or_default() {
                if let Some(curve) = moved(&t, &curve) {
                    out.push(EdgeSource {
                        body: instance.body,
                        edge: name.unwrap_or_default(),
                        path: Some(instance.path.clone()),
                        curve,
                    });
                }
            }
        }
        out
    }

    /// The curves of an edge as the model's projection reads them, in the
    /// item's component's coordinates seen at `target`: one curve, else
    /// None.
    pub(crate) fn edge_curves(
        &self,
        source: &EdgeSource,
        target: &[OccurrenceUid],
    ) -> Option<Vec<Curve3>> {
        let name: TopoName = source.edge.parse().ok()?;
        let shape = crate::body_scope::shape(self.doc, source.body)?;
        let curves = self.doc.kernel().curves_of(shape, &name).ok()?;
        if curves.len() != 1 {
            return None;
        }
        Some(match &source.path {
            Some(path) => {
                let t = self.to_target(path, target);
                curves.iter().map(|c| transform_curve(&t, c)).collect()
            }
            None => curves,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(n: u64) -> BodyUid {
        format!("F{n}.b0").parse().unwrap()
    }

    fn face(name: &str) -> FaceName {
        name.parse().unwrap()
    }

    fn candidate(path: &[u32], z: f64, nz: f64) -> FaceCandidate {
        FaceCandidate {
            path: path.iter().map(|o| OccurrenceUid(*o)).collect(),
            body: body(2),
            face: face(if nz > 0.0 { "F2:end" } else { "F2:start" }),
            origin: [0.0, 0.0, z],
            normal: [0.0, 0.0, nz],
        }
    }

    fn at(z: f64) -> SketchFrame {
        SketchFrame {
            origin: [0.0, 0.0, z],
            ..SketchFrame::XY
        }
    }

    #[test]
    fn the_one_face_in_the_plane_looking_along_the_sketch() {
        let mut none = |_: &FaceCandidate| 0;
        // The board's top face (looking up) and a part's bottom face on it
        // (looking down): the top one.
        let up = candidate(&[1], 1.6, 1.0);
        let down = candidate(&[2], 1.6, -1.0);
        let below = candidate(&[1], 0.0, -1.0);
        assert_eq!(
            plane_face(
                &at(1.6),
                vec![down.clone(), up.clone(), below.clone()],
                &mut none
            ),
            Some(up.clone())
        );
        // Only one against the normal: that one.
        assert_eq!(
            plane_face(&at(0.0), vec![up.clone(), below.clone()], &mut none),
            Some(below)
        );
        // Two faces of different occurrences in the plane: the one holding
        // more of the sketch's points, none when neither holds more.
        let pad = candidate(&[3], 1.6, 1.0);
        let both = vec![up.clone(), pad.clone()];
        let mut board_points = |c: &FaceCandidate| if c.path == up.path { 40 } else { 2 };
        assert_eq!(
            plane_face(&at(1.6), both.clone(), &mut board_points),
            Some(up.clone())
        );
        let mut same = |_: &FaceCandidate| 3;
        assert_eq!(plane_face(&at(1.6), both.clone(), &mut same), None);
        assert_eq!(plane_face(&at(1.6), both, &mut none), None);
        assert_eq!(plane_face(&at(5.0), vec![up], &mut none), None);
    }

    #[test]
    fn links_name_both_paths() {
        assert_eq!(
            link(&[OccurrenceUid(1), OccurrenceUid(4)], &[]),
            json!({"source": "O1/O4"})
        );
        assert_eq!(
            link(&[OccurrenceUid(2)], &[OccurrenceUid(3)]),
            json!({"source": "O2", "target": "O3"})
        );
    }

    fn parts(entities: Vec<Value>) -> SketchParts {
        SketchParts {
            entities,
            linked: 3,
            ..SketchParts::default()
        }
    }

    fn line_edge(
        body_n: u64,
        name: &str,
        a: [f64; 3],
        b: [f64; 3],
        path: Option<Vec<u32>>,
    ) -> EdgeSource {
        EdgeSource {
            body: body(body_n),
            edge: name.to_owned(),
            path: path.map(|p| p.into_iter().map(OccurrenceUid).collect()),
            curve: CurveGeometry::Line { start: a, end: b },
        }
    }

    fn curve_of(edge: &EdgeSource) -> Option<Vec<Curve3>> {
        match &edge.curve {
            CurveGeometry::Line { start, end } => Some(vec![Curve3::Line {
                start: *start,
                end: *end,
            }]),
            CurveGeometry::Circle {
                center,
                normal,
                radius,
                ..
            } => Some(vec![Curve3::Conic {
                center: *center,
                normal: *normal,
                x_axis: [1.0, 0.0, 0.0],
                major: *radius,
                minor: *radius,
                start: 0.0,
                end: std::f64::consts::TAU,
                closed: true,
            }]),
            CurveGeometry::Other { .. } => None,
        }
    }

    #[test]
    fn linked_curves_on_one_edge_each_become_projections() {
        let point = |id: &str, x: f64, y: f64| json!({"id": id, "type": "point", "at": [x, y], "fixed": true, "reference": true});
        let mut p = parts(vec![
            point("p1", 0.0, 0.0),
            point("p2", 10.0, 0.0),
            point("p3", 10.0, 5.0),
            point("p4", 20.0, 20.0),
            json!({"id": "c5", "type": "line", "start": "p1", "end": "p2",
                   "fixed": true, "reference": true}),
            // Drawn from (10, 5) to (10, 0); the edge runs up.
            json!({"id": "c6", "type": "line", "start": "p3", "end": "p2",
                   "fixed": true, "reference": true}),
            json!({"id": "c7", "type": "circle", "center": "p4", "radius": 3.0,
                   "fixed": true, "reference": true}),
            // Two edges of other bodies project onto it: kept fixed.
            point("p8", 0.0, 30.0),
            point("p9", 5.0, 30.0),
            json!({"id": "c10", "type": "line", "start": "p8", "end": "p9",
                   "fixed": true, "reference": true}),
        ]);
        let frame = at(1.6);
        let edges = vec![
            // The sketch's own face's edge, and one under it (a wall's
            // bottom edge) that projects onto the same line.
            line_edge(1, "E1", [0.0, 0.0, 1.6], [10.0, 0.0, 1.6], None),
            line_edge(1, "E2", [0.0, 0.0, 0.0], [10.0, 0.0, 0.0], None),
            // Another component's edge, running up.
            line_edge(3, "E3", [10.0, 0.0, 1.6], [10.0, 5.0, 1.6], Some(vec![2])),
            EdgeSource {
                body: body(3),
                edge: "E4".to_owned(),
                path: Some(vec![OccurrenceUid(2)]),
                curve: CurveGeometry::Circle {
                    center: [20.0, 20.0, 1.6],
                    normal: [0.0, 0.0, 1.0],
                    radius: 3.0,
                    start: [23.0, 20.0, 1.6],
                    end: [23.0, 20.0, 1.6],
                },
            },
            line_edge(4, "E5", [0.0, 30.0, 0.0], [5.0, 30.0, 0.0], None),
            line_edge(5, "E6", [0.0, 30.0, 9.0], [5.0, 30.0, 9.0], None),
        ];
        let c = choices(&p, &frame, &edges);
        let r = resolve(&mut p, &frame, &edges, &c, &[], &mut curve_of);
        assert_eq!(r.projections.len(), 3, "{:?}", r.projections);
        assert_eq!(r.other, 2);
        assert_eq!(
            r.projections[0],
            json!({"source": "E1", "body": "F1.b0", "entities": ["p1", "p2", "c5"]})
        );
        // The line is turned to run as its edge.
        assert_eq!(
            r.projections[1],
            json!({"source": "E3", "body": "F3.b0", "entities": ["p2", "p3", "c6"],
                   "link": {"source": "O2"}})
        );
        assert_eq!(
            (&p.entities[5]["start"], &p.entities[5]["end"]),
            (&json!("p2"), &json!("p3"))
        );
        assert_eq!(
            r.projections[2],
            json!({"source": "E4", "body": "F3.b0", "entities": ["p4", "c7"],
                   "link": {"source": "O2"}})
        );
    }

    #[test]
    fn a_curve_off_its_edge_stays_fixed() {
        let mut p = parts(vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 0.001], "reference": true}),
            json!({"id": "p2", "type": "point", "at": [10.0, 0.0], "reference": true}),
            json!({"id": "c3", "type": "line", "start": "p1", "end": "p2", "reference": true}),
        ]);
        let edges = vec![line_edge(1, "E1", [0.0, 0.0, 0.0], [10.0, 0.0, 0.0], None)];
        // Within the snap's reach but not on the projection: not snapped,
        // so no projection.
        let c = choices(&p, &SketchFrame::XY, &edges);
        let r = resolve(&mut p, &SketchFrame::XY, &edges, &c, &[], &mut curve_of);
        assert!(r.projections.is_empty());
    }

    #[test]
    fn the_note_says_what_became_projections_and_why_the_rest_did_not() {
        let resolved = Resolved {
            projections: vec![json!({}); 3],
            other: 2,
        };
        assert_eq!(
            notes(5, &resolved, true, true, 4, 0),
            [
                "3 linked curves as projections (2 of other components)",
                "2 linked curves kept as fixed reference geometry (no single edge of the replay \
                 they follow; 4 on the model's edges)"
            ]
        );
        assert_eq!(
            notes(5, &resolved, true, true, 3, 1),
            [
                "3 linked curves as projections (2 of other components)",
                "2 linked curves kept as fixed reference geometry (no single edge of the replay \
                 they follow; 3 on the model's edges; 1 left where the file stored them: a snap \
                 onto the model's edge would change the sketch's regions)"
            ]
        );
        assert_eq!(
            notes(5, &resolved, false, true, 0, 0),
            [
                "5 linked curves kept as fixed reference geometry (the sketch does not solve with \
              them as projections)"
            ]
        );
        // A copy in another component: not looked for.
        assert_eq!(
            notes(2, &Resolved::default(), false, false, 0, 0),
            ["2 linked curves kept as fixed reference geometry"]
        );
    }

    /// A linked spline that is the projection of two edges end to end
    /// (a helix the replay splits per turn, mitcad#389): one projection of
    /// the first edge with the second in its chain, the spline where the
    /// joined edges project, its inner control points new.
    #[test]
    fn a_linked_spline_on_a_chain_of_edges_becomes_one_projection() {
        let point = |id: &str, x: f64, y: f64| json!({"id": id, "type": "point", "at": [x, y], "fixed": true});
        let mut p = parts(vec![
            point("p1", 0.0, 0.0),
            point("p2", 1.0, 1.000002),
            point("p3", 2.0, 0.0),
            point("p4", 3.0, -0.999998),
            point("p5", 4.0, 0.0),
            json!({"id": "c6", "type": "spline", "degree": 2,
                   "control": ["p1", "p2", "p3", "p4", "p5"],
                   "knots": [0, 0, 0, 1, 1, 2, 2, 2], "fixed": true, "reference": true}),
        ]);
        let quadratic = |poles: [[f64; 3]; 3]| Curve3::BSpline {
            degree: 2,
            poles: poles.to_vec(),
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        };
        let other = |name: &str| EdgeSource {
            body: body(1),
            edge: name.to_owned(),
            path: None,
            curve: CurveGeometry::Other {
                kind: "bspline".to_owned(),
                start: [0.0; 3],
                end: [0.0; 3],
            },
        };
        let edges = vec![other("E1"), other("E2"), other("E3")];
        let mut curves_of = |e: &EdgeSource| -> Option<Vec<Curve3>> {
            Some(vec![match e.edge.as_str() {
                "E1" => quadratic([[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [2.0, 0.0, 0.0]]),
                // (Listed the other way round.)
                "E2" => quadratic([[4.0, 0.0, 0.0], [3.0, -1.0, 0.0], [2.0, 0.0, 0.0]]),
                _ => quadratic([[0.0, 5.0, 0.0], [1.0, 6.0, 0.0], [2.0, 5.0, 0.0]]),
            }])
        };
        let found = chains(&p, &SketchFrame::XY, &edges, &[], 0.01, &mut curves_of);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].edges, vec![0, 1]);
        let mut r = Resolved::default();
        resolve_chains(&mut p, &SketchFrame::XY, &edges, &found, &[], &mut r);
        assert_eq!(
            r.projections,
            vec![json!({"source": "E1", "chain": ["E2"], "body": "F1.b0",
                        "entities": ["p1", "p7", "p8", "p9", "p5", "c6"]})]
        );
        let spline = p.entities.iter().find(|e| e["id"] == "c6").unwrap();
        assert_eq!(spline["control"], json!(["p1", "p7", "p8", "p9", "p5"]));
        assert_eq!(point_at(&p.entities, "p7"), Some([1.0, 1.0]));
        assert_eq!(point_at(&p.entities, "p9"), Some([3.0, -1.0]));
        assert!(point_at(&p.entities, "p2").is_none());
    }

    #[test]
    fn region_keys_are_compared_without_the_curves_directions() {
        assert_eq!(
            unoriented("r{c12[c13,c14],c13[c14,c12],c14[c13,c12]}"),
            unoriented("r{c13[c12,c14],c12[c13,c14],c14[c13,c12]}")
        );
        assert_ne!(
            unoriented("r{c12[c13,c14],c13[c14,c12]}"),
            unoriented("r{c12[c13+c15,c14],c13[c14,c12]}")
        );
        assert_eq!(unoriented("r{c3}"), "r{c3}");
    }

    /// A corner the file holds on a linked curve's end, which the replay's
    /// edge gives 0.3 µm off (round 11 of mitcad#343): the solve moving the
    /// corner there keeps the regions the file's numbers bound with the
    /// corner held on that end.
    #[test]
    fn a_corner_held_on_a_point_counts_as_there_in_the_file_s_regions() {
        let end = [1.0, 3.0e-4];
        let entities = vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0]}),
            json!({"id": "p2", "type": "point", "at": [1.0, 0.0]}),
            json!({"id": "p3", "type": "point", "at": [1.0, 1.0]}),
            json!({"id": "p4", "type": "point", "at": [0.0, 1.0]}),
            json!({"id": "p5", "type": "point", "at": end}),
            json!({"id": "c7", "type": "line", "start": "p1", "end": "p2"}),
            json!({"id": "c8", "type": "line", "start": "p2", "end": "p3"}),
            json!({"id": "c9", "type": "line", "start": "p3", "end": "p4"}),
            json!({"id": "c10", "type": "line", "start": "p4", "end": "p1"}),
            json!({"id": "c11", "type": "line", "start": "p5", "end": "p4", "reference": true}),
        ];
        let constraints =
            vec![json!({"id": "k0", "type": "coincident", "point": "p2", "entity": "p5"})];
        let solved = std::collections::BTreeMap::from([(EntityUid(2), end)]);
        assert!(regions_kept(&entities, &constraints, &solved));
        // Without the constraint the solved corner meets the linked line,
        // the file's does not: the regions differ.
        assert!(!regions_kept(&entities, &[], &solved));
    }
}
