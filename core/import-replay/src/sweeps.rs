// SPDX-License-Identifier: MIT
//! Sweeps, pipes and lofts (F3): from external dumps (`commands.md`, the
//! F3 import table) and from the stream decoder (mitcad#34), which gives
//! their paths, rails, sections and conditions but not every choice: a
//! profile's region (only its sketch), a sweep's direction, orientation
//! and scaling, a pipe's section type and wall. Those are tried in turn
//! and the history picks (the dump's readings first). Coils, ribs and webs,
//! which dumps give no inputs of, fall back to the stored bodies.

use mitcad_dump::{PipeDetail, Reference, SweepDetail, TimelineItem};
use mitcad_model::{EdgeName, FaceName, FeatureUid};
// Inputs on other components' bodies (mitcad#161).
use mitcad_dump::Fingerprint;
use mitcad_model::{BodyUid, ComponentUid};
use serde_json::{Map, Value, json};

use crate::features::profiles_of;
use crate::scaffold::CandidateCore;
use crate::{Candidate, Importer, refs};

/// At most this many choices of loft sections' regions are tried.
const SECTION_CHOICES: usize = 12;

/// Choices of one option per section, those of the first options first
/// (by the sum of the options' positions), at most `limit`.
fn choices(options: &[Vec<Value>], limit: usize) -> Vec<Vec<Value>> {
    let mut out: Vec<(usize, Vec<Value>)> = vec![(0, Vec::new())];
    for section in options {
        let mut next = Vec::new();
        for (rank, chosen) in &out {
            for (i, option) in section.iter().enumerate() {
                let mut c = chosen.clone();
                c.push(option.clone());
                next.push((rank + i, c));
            }
        }
        // A stable sort keeps the earlier sections' order among equals.
        next.sort_by_key(|(rank, _)| *rank);
        next.truncate(limit);
        out = next;
    }
    out.into_iter().map(|(_, c)| c).collect()
}

/// The candidates with the operation's other readings ([`Importer::
/// operation_readings`]) after them: the `k`-th reading of the `i`-th
/// candidate by the rank `i + k`.
fn with_operations(
    candidates: Vec<Candidate>,
    readings: &[(String, Vec<String>)],
) -> Vec<Candidate> {
    let mut ranked = Vec::new();
    for (k, (operation, participants)) in readings.iter().enumerate() {
        for (i, c) in candidates.iter().enumerate() {
            let mut c = c.clone();
            if k > 0 {
                for def in &mut c.defs {
                    def["operation"] = json!(operation);
                    match def.as_object_mut() {
                        Some(m) if participants.is_empty() => {
                            m.remove("participants");
                        }
                        _ => def["participants"] = json!(participants),
                    }
                }
                c.guess = true;
                c.input_provenance = crate::InputProvenance::Inferred;
            }
            ranked.push((i + k, c));
        }
    }
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, c)| c).collect()
}

/// Whether a sweep's, pipe's or loft's inputs name edges of the bodies
/// (paths, rails or sections of edges), which a fallback before it can
/// change.
pub(crate) fn uses_edges(item: &TimelineItem) -> bool {
    fn edges(v: &Value) -> bool {
        match v {
            Value::Object(m) => {
                m.get("kind").and_then(Value::as_str) == Some("edge") || m.values().any(edges)
            }
            Value::Array(a) => a.iter().any(edges),
            _ => false,
        }
    }
    item.detail
        .as_ref()
        .and_then(|d| serde_json::to_value(d).ok())
        .is_some_and(|v| edges(&v))
}

fn reference(v: &Value) -> Option<Reference> {
    Some(Reference::from_map(v.as_object()?.clone()))
}

fn is_zero(r: &Option<Reference>) -> bool {
    r.as_ref()
        .and_then(Reference::parameter)
        .and_then(|p| p.value)
        .is_none_or(|v| v == 0.0)
}

/// A value present and not empty (a guide rail, guide surfaces). A text is
/// a collection an external dump could not convert (its proxy's text),
/// taken as empty: the history tells whether it was.
fn given(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null | Value::String(_)) => false,
        Some(Value::Array(a)) => !a.is_empty(),
        Some(_) => true,
    }
}

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// A `Path` of the dump (its `PathEntity` items, or entity references) as a
    /// Mitcad path: curves of one sketch or edges of one body.
    pub(crate) fn path_of(&self, path: Option<&Value>, what: &str) -> Result<Value, String> {
        let items: Vec<&Value> = match path {
            Some(Value::Array(items)) => items.iter().collect(),
            Some(v @ Value::Object(_)) => vec![v],
            _ => return Err(format!("its {what} was not decoded")),
        };
        let mut sketch: Option<String> = None;
        let mut curves = Vec::new();
        let mut body: Option<String> = None;
        let mut edges = Vec::new();
        for item in items {
            // A PathEntity's entity, or the reference itself.
            let entity = item.get("entity").unwrap_or(item);
            match reference(entity) {
                Some(Reference::SketchEntity(e)) => {
                    let info = e
                        .sketch_timeline_index
                        .flatten()
                        .and_then(|i| self.sketches.get(&i))
                        .ok_or_else(|| format!("the sketch of its {what} was not imported"))?;
                    let id =
                        e.id.clone()
                            .flatten()
                            .ok_or_else(|| format!("a curve of its {what} has no id"))?;
                    let curve = info
                        .ids
                        .get(&id)
                        .ok_or_else(|| format!("{what} curve {id} was left out of its sketch"))?;
                    let uid = info.uid.to_string();
                    if *sketch.get_or_insert_with(|| uid.clone()) != uid {
                        return Err(format!("its {what} runs through several sketches"));
                    }
                    curves.push(curve.clone());
                }
                Some(Reference::Edge(fp)) => {
                    let (b, found) = self
                        .edge_input(&fp)
                        .ok_or_else(|| format!("an edge of its {what} was not found"))?;
                    let b = b.to_string();
                    if *body.get_or_insert_with(|| b.clone()) != b {
                        return Err(format!("its {what} runs over several bodies"));
                    }
                    edges.extend(found.iter().map(ToString::to_string));
                }
                _ => return Err(format!("its {what} is not sketch curves or edges")),
            }
        }
        match (sketch, body) {
            (Some(s), None) => Ok(json!({"sketch": s, "curves": curves})),
            (None, Some(b)) => Ok(json!({"body": b, "edges": edges})),
            (None, None) => Err(format!("its {what} is empty")),
            _ => Err(format!("its {what} mixes sketch curves and edges")),
        }
    }

    /// A loft section of a body's edges (a `Path` of `BRepEdge`s, or
    /// one edge) as the face they go round: the face each of them borders
    /// that has no other edges. The faces across the edges are then the
    /// ones a tangent or smooth condition continues.
    fn edge_loop_face(&self, path: Option<&Value>) -> Result<Value, String> {
        let items: Vec<&Value> = match path {
            Some(Value::Array(items)) => items.iter().collect(),
            Some(v @ Value::Object(_)) => vec![v],
            _ => return Err("its edges were not decoded".to_owned()),
        };
        let mut body = None;
        let mut edges: Vec<EdgeName> = Vec::new();
        for item in items {
            let entity = item.get("entity").unwrap_or(item);
            let Some(Reference::Edge(fp)) = reference(entity) else {
                return Err("this kind of section is not supported".to_owned());
            };
            let (b, edge) = refs::resolve_edge(self.doc, &fp)
                .ok_or_else(|| "an edge of it was not found".to_owned())?;
            if *body.get_or_insert(b) != b {
                return Err("its edges are on several bodies".to_owned());
            }
            if !edges.contains(&edge) {
                edges.push(edge);
            }
        }
        let body = body.ok_or_else(|| "it has no edges".to_owned())?;
        let shape = crate::body_scope::bodies(self.doc)
            .into_iter()
            .find(|b| b.uid == body)
            .map(|b| b.shape)
            .ok_or_else(|| "its body was not found".to_owned())?;
        let all: Vec<EdgeName> = refs::edge_midpoints(self.doc.kernel(), shape)
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        let face = edges[0]
            .faces()
            .iter()
            .find(|face| {
                edges.iter().all(|e| e.faces().contains(face))
                    && all.iter().filter(|e| e.faces().contains(face)).count() == edges.len()
            })
            .ok_or_else(|| "its edges do not go round one face".to_owned())?;
        Ok(json!({"type": "face", "body": body.to_string(), "face": face.to_string()}))
    }

    /// A sweep's guide surfaces (mitcad#157) as `guide_surfaces`: each
    /// face found by its fingerprint (a point on it, its surface type and
    /// area), as other face inputs are.
    fn guide_surfaces(&self, faces: Option<&Value>) -> Result<Value, String> {
        let items: Vec<&Value> = match faces {
            Some(Value::Array(items)) => items.iter().collect(),
            Some(v @ Value::Object(_)) => vec![v],
            _ => return Err("its guide surfaces were not decoded".to_owned()),
        };
        let mut out = Vec::new();
        for item in items {
            let Some(Reference::Face(fp)) = reference(item) else {
                return Err("its guide surfaces were not decoded".to_owned());
            };
            if fp.point_on_face.is_none() {
                return Err("its guide surfaces' geometry was not decoded".to_owned());
            }
            let keep = self.input_bodies(&fp);
            let Some((body, face)) = refs::resolve_face_where(self.doc, &fp, &keep) else {
                self.trace_nearest_faces(&fp, &keep);
                return Err("a guide surface was not found".to_owned());
            };
            // Every piece of it where the replay splits it (mitcad#161).
            let mut pieces = refs::face_pieces(self.doc, body, &fp);
            if !pieces.contains(&face) {
                pieces.insert(0, face);
            }
            for face in pieces {
                let face = json!({"body": body.to_string(), "face": face.to_string()});
                if !out.contains(&face) {
                    out.push(face);
                }
            }
        }
        Ok(Value::Array(out))
    }

    // Inputs on other components' bodies (mitcad#161).

    /// The bodies a fingerprint's entity is looked for among: those of the
    /// component the decoder found it in when it gives one
    /// (`_f3d.component`, another component's history), else all.
    fn input_bodies(&self, fp: &Fingerprint) -> impl Fn(BodyUid) -> bool + '_ {
        let component = fp
            .f3d
            .as_ref()
            .and_then(|f| f.component)
            .map(|c| self.components.of_object(c));
        move |b| match component {
            Some(c) => c.is_some() && self.doc.body_component(b) == c,
            None => true,
        }
    }

    /// The face a fingerprint names, among the bodies of the component the
    /// decoder found it in where it gives one, else among those of the
    /// item's own component (a feature works on its component's bodies; the
    /// faces of a split's pieces and of the balls that cut them lie on each
    /// other), else among all.
    pub(crate) fn face_input(&self, fp: &Fingerprint) -> Option<(BodyUid, FaceName)> {
        let keep = self.input_bodies(fp);
        let own = self.component;
        let given = fp.f3d.as_ref().is_some_and(|f| f.component.is_some());
        let found = given
            .then(|| refs::resolve_face_where(self.doc, fp, &keep))
            .flatten()
            .or_else(|| {
                refs::resolve_face_where(self.doc, fp, &|b| self.doc.body_component(b) == Some(own))
            })
            .or_else(|| refs::resolve_face(self.doc, fp));
        if let Some((body, _)) = &found {
            self.hint_path(fp, *body);
        }
        found
    }

    /// Traces how far the faces of the bodies `keep` keeps are from a face
    /// fingerprint's point, when the face is not found (`MITCAD_IMPORT_TRACE`).
    pub(crate) fn trace_nearest_faces(&self, fp: &Fingerprint, keep: &dyn Fn(BodyUid) -> bool) {
        if !crate::tracing() {
            return;
        }
        let points = fp.point_on_face.into_iter().chain(
            fp.f3d
                .as_ref()
                .and_then(|f| f.face_points.clone())
                .into_iter()
                .flatten(),
        );
        let kernel = self.doc.kernel();
        for point in points.map(crate::geom::mm3) {
            let mut nearest: Option<(f64, String)> = None;
            for body in crate::body_scope::bodies(self.doc)
                .iter()
                .filter(|b| keep(b.uid))
            {
                for face in kernel.faces(body.shape).unwrap_or_default() {
                    let Some(name) = face.names.first().and_then(|n| n.parse().ok()) else {
                        continue;
                    };
                    if let Ok(near) = kernel.face_point_normal(body.shape, &name, point) {
                        let d = crate::geom::distance(near.point, point);
                        if nearest.as_ref().is_none_or(|(n, _)| d < *n) {
                            nearest = Some((d, format!("{} {name} ({})", body.uid, face.surface)));
                        }
                    }
                }
            }
            if let Some((d, face)) = nearest {
                eprintln!(
                    "import: [{:.2}] guide surface: {face} is {d:.6} mm from {point:?}",
                    crate::trace_clock(),
                );
            }
        }
    }

    /// An edge input: the edge its fingerprint names, or the edges the
    /// replay split it into ([`refs::resolve_edge_chain`]), in order.
    fn edge_input(&self, fp: &Fingerprint) -> Option<(BodyUid, Vec<EdgeName>)> {
        let keep = self.input_bodies(fp);
        if let Some((body, edge)) = refs::resolve_edge_where(self.doc, fp, &keep) {
            self.hint_path(fp, body);
            return Some((body, vec![edge]));
        }
        let chain = refs::resolve_edge_chain(self.doc, fp, &keep);
        if let Some((body, _)) = &chain {
            self.hint_path(fp, *body);
        }
        if chain.is_none() && crate::tracing() {
            self.trace_edges_near(fp, &keep);
        }
        chain
    }

    /// Traces the edges of the bodies `keep` keeps with an end near an edge
    /// fingerprint's ends, when the edge is not found (`MITCAD_IMPORT_TRACE`).
    pub(crate) fn trace_edges_near(&self, fp: &Fingerprint, keep: &dyn Fn(BodyUid) -> bool) {
        let ends: Vec<[f64; 3]> = [fp.start_point.flatten(), fp.end_point.flatten()]
            .into_iter()
            .flatten()
            .map(crate::geom::mm3)
            .collect();
        eprintln!(
            "import: edge input: ends {ends:?}, middle {:?}, length {:?}",
            fp.mid_point.map(crate::geom::mm3),
            fp.length.map(crate::geom::mm)
        );
        let kernel = self.doc.kernel();
        for body in crate::body_scope::bodies(self.doc)
            .iter()
            .filter(|b| keep(b.uid))
        {
            let middles = kernel.edge_middles(body.shape).unwrap_or_default();
            for (name, curve) in kernel.edge_geometries(body.shape).unwrap_or_default() {
                let (a, b) = match &curve {
                    mitcad_model::datum::CurveGeometry::Line { start, end }
                    | mitcad_model::datum::CurveGeometry::Circle { start, end, .. }
                    | mitcad_model::datum::CurveGeometry::Other { start, end, .. } => {
                        (*start, *end)
                    }
                };
                let gap = ends
                    .iter()
                    .flat_map(|e| [crate::geom::distance(*e, a), crate::geom::distance(*e, b)])
                    .fold(f64::INFINITY, f64::min);
                if gap < 0.5 {
                    let length = name
                        .as_ref()
                        .and_then(|n| middles.iter().find(|m| &m.name == n))
                        .map(|m| m.length);
                    eprintln!(
                        "import: edge input: {} {name:?} ({}) {a:?} -> {b:?}, length {length:?}, {gap:.6} mm from an end",
                        body.uid,
                        curve.kind()
                    );
                }
            }
        }
    }

    /// The links of the bodies a sweep's path and guide surfaces are on
    /// that belong to another component than `own` (the profile sketch's,
    /// where the sweep goes): the occurrence paths to the body's component
    /// and to `own`, each placed once, as a combine's `tool_links`.
    fn body_links(&self, own: ComponentUid, def: &Value) -> Result<Map<String, Value>, String> {
        let bodies = def["path"]["body"].as_str().into_iter().chain(
            def["guide_surfaces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f["body"].as_str()),
        );
        self.links_of(own, bodies, "its path or guide surfaces are")
    }

    /// The links of the bodies an extrusion's objects are or are faces of
    /// that belong to another component than `own` (the profile sketch's,
    /// where the extrusion goes), as [`Importer::body_links`].
    pub(crate) fn object_links(
        &self,
        own: ComponentUid,
        def: &Value,
    ) -> Result<Map<String, Value>, String> {
        let objects = [
            &def["start"]["object"],
            &def["extent"]["object"],
            &def["extent"]["side1"]["object"],
            &def["extent"]["side2"]["object"],
        ];
        let bodies = objects.into_iter().filter_map(|o| o["body"].as_str());
        self.links_of(own, bodies, "its objects are")
    }

    /// The links of the bodies a construction plane's definition names faces,
    /// edges and vertices of that belong to another component than `own`
    /// (the plane's), as [`Importer::body_links`] (mitcad#196).
    pub(crate) fn plane_links(
        &self,
        own: ComponentUid,
        def: &Value,
    ) -> Result<Map<String, Value>, String> {
        fn bodies<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
            match v {
                Value::Object(m) => {
                    for (key, value) in m {
                        match (key.as_str(), value) {
                            ("body", Value::String(s)) => out.push(s),
                            _ => bodies(value, out),
                        }
                    }
                }
                Value::Array(a) => a.iter().for_each(|x| bodies(x, out)),
                _ => {}
            }
        }
        let mut names = Vec::new();
        bodies(&def["definition"], &mut names);
        self.links_of(own, names.into_iter(), "its faces are")
    }

    /// The links of `bodies` of other components than `own`: the occurrence
    /// paths to the body's component, placed once, and to `own`, placed once or
    /// else (an occurrence pattern) at its first occurrence, the original that
    /// the file's features are made in.
    fn links_of<'a>(
        &self,
        own: ComponentUid,
        bodies: impl Iterator<Item = &'a str>,
        what: &str,
    ) -> Result<Map<String, Value>, String> {
        let assembly = self.doc.assembly();
        let only_path = |c, first: bool| match assembly.paths_to(c).as_slice() {
            [path] => Ok(path
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/")),
            paths if first && !paths.is_empty() => Ok(paths
                .iter()
                .min()
                .into_iter()
                .flatten()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/")),
            _ => Err(format!(
                "{what} on another component, and {} is not placed once",
                assembly.name(c)
            )),
        };
        let mut links = Map::new();
        for body in bodies {
            let Some((uid, c)) = body
                .parse::<BodyUid>()
                .ok()
                .and_then(|uid| Some((uid, self.doc.body_component(uid)?)))
            else {
                continue;
            };
            if c != own && !links.contains_key(body) {
                // The occurrence the file's reference to the body is in,
                // where a component is placed more than once.
                let source = match self.hinted_path(uid) {
                    Some(path) => path
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("/"),
                    None => only_path(c, false)?,
                };
                links.insert(
                    body.to_owned(),
                    json!({"source": source, "target": only_path(own, true)?}),
                );
            }
        }
        Ok(links)
    }

    /// A unitless value (a fraction, a weight): the imported parameter's
    /// name, or its number.
    fn unitless(&self, r: &Option<Reference>) -> Option<Value> {
        let p = r.as_ref().and_then(Reference::parameter)?;
        if let Some(name) = p.name.as_deref().and_then(|n| self.params.get(n)) {
            return Some(json!(name));
        }
        p.value.map(|v| json!(v))
    }

    /// The part of the path: all of it unless a fraction is not 1.
    fn path_extent(&self, one: &Option<Reference>, two: Option<&Option<Reference>>) -> Value {
        let whole = |r: &Option<Reference>| {
            r.as_ref()
                .and_then(Reference::parameter)
                .and_then(|p| p.value)
                .is_none_or(|v| (v - 1.0).abs() < 1e-12)
        };
        if whole(one) && two.is_none_or(whole) {
            return json!({"type": "full"});
        }
        let mut extent = json!({"type": "partial",
                                "fraction": self.unitless(one).unwrap_or(json!(1.0))});
        if let Some(fraction) = two.and_then(|r| self.unitless(r)) {
            extent["fraction2"] = fraction;
        }
        extent
    }

    /// The operation and participants to try, the dump's first. Without
    /// participants (the stream decoder gives none) a join or cut is also
    /// tried on the bodies the history changed, and a join as a new body
    /// (a join that meets none of its participants makes one).
    fn operation_readings(
        &mut self,
        index: i64,
        operation: &str,
        bodies: &Option<Vec<Reference>>,
        sketch: Option<FeatureUid>,
    ) -> Vec<(String, Vec<String>)> {
        let decoded = self.participants(bodies);
        let mut out: Vec<(String, Vec<String>)> = match sketch {
            Some(s) => self
                .participant_options(index, bodies, operation, s)
                .into_iter()
                .map(|p| (operation.to_owned(), p))
                .collect(),
            None => vec![(operation.to_owned(), decoded.clone())],
        };
        if operation == "join" && decoded.is_empty() {
            out.push(("new_body".to_owned(), Vec::new()));
        }
        out
    }

    pub(crate) fn sweep(&mut self, index: i64, d: &SweepDetail) -> Result<Vec<Candidate>, String> {
        // Guide surfaces (mitcad#157): the faces as found by their
        // fingerprints. The stream decoder may also give a guide rail
        // then (a slot the sweep keeps from another type): the surfaces
        // decide the type.
        let surfaces = given(d.other.get("guideSurfaces"));
        let guide_surfaces = if surfaces {
            Some(self.guide_surfaces(d.other.get("guideSurfaces"))?)
        } else {
            None
        };
        let profiles = profiles_of(&d.profile, &d.other);
        if profiles.is_empty() {
            return Err("its profiles were not decoded".to_owned());
        }
        let path = self.path_of(d.path.as_ref(), "path")?;
        let (sketch, _) = self.profile_sketch(&profiles, index)?;
        // The decoded profiles' regions first, and without probing the
        // history's next states for others (mitcad#181: a probe of a
        // thread's states took 12–30 s on every try of the item).
        let decoded = self.mapped_set(&d.other, sketch).is_some();
        let mut sets = self.region_sets_probing(&profiles, sketch, None, None, !decoded)?;
        self.mapped_first(&mut sets, &d.other, sketch);
        let uid = self.sketches[&sketch].uid.to_string();
        let mut base = json!({"type": "sweep", "path": path,
                              "operation": Self::operation(d.operation.as_deref())});
        if d.orientation
            .as_deref()
            .is_some_and(|o| o.starts_with("Parallel"))
        {
            base["orientation"] = json!("parallel");
        }
        let rail = given(d.guide_rail.as_ref()) && !surfaces;
        if let Some(faces) = guide_surfaces {
            if base.get("orientation").is_some() {
                return Err(
                    "guide surfaces with the parallel orientation are not supported".to_owned(),
                );
            }
            base["guide_surfaces"] = faces;
        } else if rail {
            base["guide_rail"] = self.path_of(d.guide_rail.as_ref(), "guide rail")?;
            // The rail's point where it crosses the plane square to the
            // path (mitcad#469).
            base["rail_point"] = json!("plane");
            match d.profile_scaling.as_deref() {
                Some(s) if s.contains("Stretch") => base["profile_scaling"] = json!("stretch"),
                Some(s) if s.contains("NoScaling") => base["profile_scaling"] = json!("none"),
                _ => {}
            }
            if d.other.get("extent").and_then(Value::as_str) == Some("FullExtentsSweepExtentType") {
                return Err("sweeps to the guide rail's full extent are not supported".to_owned());
            }
        } else {
            if !is_zero(&d.twist_angle) {
                base["twist_angle"] = self.value(&d.twist_angle).ok_or("no twist angle")?;
            }
            if !is_zero(&d.taper_angle) {
                base["taper_angle"] = self.value(&d.taper_angle).ok_or("no taper angle")?;
            }
        }
        // A path or guide surfaces on another component's body (mitcad#161):
        // the sweep goes where its profile is and links the body.
        if let Some(own) = self
            .doc
            .feature(self.sketches[&sketch].uid)
            .map(|f| f.component)
        {
            let links = self.body_links(own, &base)?;
            if !links.is_empty() {
                base["body_links"] = Value::Object(links);
            }
        }
        // The stream decoder gives neither the direction nor the orientation
        // nor the scaling, and a second fraction 0 where a dump has none (a
        // profile at the path's end): the readings to try, the dump's first.
        let flip = d.other.get("isDirectionFlipped").and_then(Value::as_bool);
        let mut extents = vec![self.path_extent(&d.distance_one, Some(&d.distance_two))];
        if flip.is_none() && is_zero(&d.distance_two) && d.distance_two.is_some() {
            extents.insert(0, self.path_extent(&d.distance_one, None));
        }
        let mut variants: Vec<Value> = Vec::new();
        for extent in extents {
            let mut def = base.clone();
            if extent["type"] != "full" {
                def["extent"] = extent;
            }
            // The dump's direction, then the other one. With a rail the
            // file's flag reads the other way (the reference models' rail
            // sweep, flipped, is Mitcad's sweep without `flip`). Without a
            // partial extent or a rail the direction changes nothing.
            let first = if rail {
                !flip.unwrap_or(true)
            } else {
                flip.unwrap_or(false)
            };
            let both = rail || def.get("extent").is_some();
            for f in [first, !first].into_iter().take(if both { 2 } else { 1 }) {
                let mut def = def.clone();
                if f {
                    def["flip"] = json!(true);
                }
                variants.push(def);
            }
        }
        let mut more = Vec::new();
        if rail && d.profile_scaling.is_none() {
            for scaling in ["stretch", "none"] {
                more.extend(variants.iter().map(|v| {
                    let mut v = v.clone();
                    v["profile_scaling"] = json!(scaling);
                    v
                }));
            }
        } else if !rail && !surfaces && d.orientation.is_none() && flip.is_none() {
            more.extend(variants.iter().map(|v| {
                let mut v = v.clone();
                v["orientation"] = json!("parallel");
                v
            }));
        }
        variants.extend(more);
        let participants = self.participants(&d.participant_bodies);
        if !participants.is_empty() {
            for v in &mut variants {
                v["participants"] = json!(participants);
            }
        }
        // The likeliest regions with the likeliest readings first: by the
        // sum of their positions.
        let mut candidates = Vec::new();
        for (k, variant) in variants.iter().enumerate() {
            for (s, set) in sets.iter().enumerate() {
                let mut def = variant.clone();
                def["profiles"] = set
                    .keys
                    .iter()
                    .map(|r| json!({"sketch": uid, "region": r}))
                    .collect();
                let whole_path =
                    d.orientation.is_some() && !rail && !surfaces && def.get("extent").is_none();
                let candidate = Candidate {
                    core: CandidateCore::new(def),
                    note: None,
                    guess: set.guess || k > 0,
                    predicted: None,
                    input_provenance: if set.guess || k > 0 {
                        crate::InputProvenance::Inferred
                    } else if whole_path && set.exact {
                        // Orientation (mitcad#25), twist, taper, regions
                        // (the decoded loops select them exactly) and the
                        // whole path are decoded; a partial extent's
                        // direction is not.
                        crate::InputProvenance::Read
                    } else {
                        crate::InputProvenance::Unknown
                    },
                    first: false,
                };
                candidates.push((k + s, candidate));
            }
        }
        candidates.sort_by_key(|(rank, _)| *rank);
        let operation = Self::operation(d.operation.as_deref());
        let sketch_uid = self.sketches[&sketch].uid;
        let readings =
            self.operation_readings(index, operation, &d.participant_bodies, Some(sketch_uid));
        let candidates =
            with_operations(candidates.into_iter().map(|(_, c)| c).collect(), &readings);
        // A surface sweep of regions (mitcad#455).
        Ok(if d.is_solid == Some(false) {
            crate::features::surface_candidates(candidates)
        } else {
            candidates
        })
    }

    pub(crate) fn pipe(&mut self, index: i64, d: &PipeDetail) -> Result<Vec<Candidate>, String> {
        let path = self.path_of(d.path.as_ref(), "path")?;
        let size = self
            .value(&d.section_size)
            .ok_or("its section size was not decoded")?;
        let mut def = json!({"type": "pipe", "path": path.clone(), "size": size,
                             "operation": Self::operation(d.operation.as_deref())});
        match d.section_type.as_deref() {
            Some(s) if s.starts_with("Square") => def["section"] = json!("square"),
            Some(s) if s.starts_with("Triangular") => def["section"] = json!("triangular"),
            _ => {}
        }
        if d.is_hollow == Some(true) {
            def["thickness"] = self
                .value(&d.section_thickness)
                .ok_or("its wall thickness was not decoded")?;
        }
        let participants = self.participants(&d.participant_bodies);
        if !participants.is_empty() {
            def["participants"] = json!(participants);
        }
        // The second fraction applies to closed paths only: with it first
        // when it is not zero, then without.
        let mut defs = Vec::new();
        if !is_zero(&d.distance_two) {
            let mut both = def.clone();
            both["extent"] = self.path_extent(&d.distance_one, Some(&d.distance_two));
            defs.push(both);
        }
        let extent = self.path_extent(&d.distance_one, None);
        if extent["type"] != "full" {
            def["extent"] = extent;
        }
        defs.push(def);
        // The stream decoder gives neither the section's type nor whether
        // it is hollow: a solid circle first, then the others.
        if d.section_type.is_none() && d.is_hollow.is_none() {
            let thickness = self.value(&d.section_thickness);
            let mut all = Vec::new();
            for section in ["circular", "square", "triangular"] {
                for hollow in [false, true] {
                    for def in &defs {
                        let mut def = def.clone();
                        if section != "circular" {
                            def["section"] = json!(section);
                        }
                        match (&thickness, hollow) {
                            (Some(t), true) => def["thickness"] = t.clone(),
                            (None, true) => continue,
                            _ => {}
                        }
                        all.push(def);
                    }
                }
            }
            defs = all;
        }
        let candidates = defs
            .into_iter()
            .enumerate()
            .map(|(k, def)| Candidate {
                guess: k > 0 && d.section_type.is_none(),
                input_provenance: if k > 0 && d.section_type.is_none() {
                    crate::InputProvenance::Inferred
                } else if k == 0 && d.section_type.is_some() && d.is_hollow.is_some() {
                    // The path, section, size, wall and extents are decoded
                    // (mitcad#25).
                    crate::InputProvenance::Read
                } else {
                    crate::InputProvenance::Unknown
                },
                ..Candidate::new(def)
            })
            .collect();
        // The component's bodies: those of the path's sketch.
        let sketch = path
            .get("sketch")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok());
        let operation = Self::operation(d.operation.as_deref());
        let readings = self.operation_readings(index, operation, &d.participant_bodies, sketch);
        Ok(with_operations(candidates, &readings))
    }

    pub(crate) fn loft(&mut self, item: &TimelineItem) -> Result<Vec<Candidate>, String> {
        let d: Map<String, Value> = item
            .detail
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let surface = d.get("isSolid").and_then(Value::as_bool) == Some(false);
        let mut items: Vec<&Value> = d
            .get("loftSections")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .ok_or("its sections were not decoded")?;
        items.sort_by_key(|s| s.get("index").and_then(Value::as_i64).unwrap_or(0));
        let index = item.index.unwrap_or(0);
        // Each section's options: one, or the regions a profile without its
        // area (the stream decoder gives only its sketch) may be.
        let mut sections: Vec<Vec<Value>> = Vec::new();
        for (i, section) in items.iter().enumerate() {
            let at = |e: String| format!("section {}: {e}", i + 1);
            // A path of edges (.f3d designs take tangent and smooth conditions
            // only at such a section): the face they go round.
            let raw = section.get("entity");
            let entity = raw.and_then(reference);
            if matches!(raw, Some(Value::Array(_))) || matches!(entity, Some(Reference::Edge(_))) {
                sections.push(vec![self.edge_loop_face(raw).map_err(at)?]);
                continue;
            }
            let entity = entity.ok_or_else(|| at("its entity was not decoded".to_owned()))?;
            sections.push(vec![match entity {
                Reference::Profile(p) => {
                    let measured = p.area.is_some();
                    let profile = Reference::Profile(p);
                    let (sketch, _) = self
                        .profile_sketch(std::slice::from_ref(&profile), index)
                        .map_err(at)?;
                    let sets = self
                        .region_sets(std::slice::from_ref(&profile), sketch, None, None)
                        .map_err(at)?;
                    let mut regions: Vec<String> = Vec::new();
                    for s in sets.iter().filter(|s| s.keys.len() == 1) {
                        if !regions.contains(&s.keys[0]) {
                            regions.push(s.keys[0].clone());
                        }
                    }
                    if regions.is_empty() {
                        return Err(at("its region was not found".to_owned()));
                    }
                    if measured {
                        regions.truncate(1);
                    }
                    let uid = self.sketches[&sketch].uid.to_string();
                    sections.push(
                        regions
                            .iter()
                            .map(|r| json!({"type": "profile", "sketch": uid, "region": r}))
                            .collect(),
                    );
                    continue;
                }
                Reference::Face(fp) => {
                    let (body, face) = refs::resolve_face(self.doc, &fp)
                        .ok_or_else(|| at("its face was not found".to_owned()))?;
                    json!({"type": "face", "body": body.to_string(), "face": face.to_string()})
                }
                Reference::SketchEntity(e) => {
                    let info = e
                        .sketch_timeline_index
                        .flatten()
                        .and_then(|i| self.sketches.get(&i))
                        .ok_or_else(|| at("its sketch was not imported".to_owned()))?;
                    let id = e.id.clone().flatten().unwrap_or_default();
                    let point = info
                        .ids
                        .get(&id)
                        .ok_or_else(|| at(format!("sketch point {id} was not imported")))?;
                    json!({"type": "point", "point": {"sketch": info.uid.to_string(), "point": point}})
                }
                Reference::ConstructionPoint(c) => {
                    let point = match c.origin.clone().flatten() {
                        Some(_) => json!("origin"),
                        None => {
                            let uid = c
                                .timeline_index
                                .flatten()
                                .and_then(|i| self.features.get(&i))
                                .ok_or_else(|| at("its point was not imported".to_owned()))?;
                            json!(uid.to_string())
                        }
                    };
                    json!({"type": "point", "point": point})
                }
                _ => return Err(at("this kind of section is not supported".to_owned())),
            }]);
        }
        let mut def = json!({"type": "loft",
            "operation": Self::operation(d.get("operation").and_then(Value::as_str))});
        for (key, section) in [
            ("start_condition", items.first()),
            ("end_condition", items.last()),
        ] {
            if let Some(condition) = section.and_then(|s| self.end_condition(s.get("endCondition")))
            {
                def[key] = condition;
            }
        }
        // The centre line or rails: a collection of paths.
        if let Some(guides) = d.get("centerLineOrRails").filter(|v| given(Some(v))) {
            let paths: Vec<&Value> = match guides {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            let centerline = d
                .get("centerLineOrRails.isCenterLine")
                .and_then(Value::as_bool)
                == Some(true);
            let mut converted = Vec::new();
            for path in paths {
                converted.push(self.path_of(Some(path), "center line or rail")?);
            }
            if centerline {
                def["centerline"] = converted.into_iter().next().ok_or("no center line")?;
            } else {
                def["rails"] = json!(converted);
            }
        }
        if d.get("isClosed").and_then(Value::as_bool) == Some(true) {
            def["closed"] = json!(true);
        }
        if surface {
            def["surface"] = json!(true);
        }
        let bodies: Option<Vec<Reference>> = d
            .get("participantBodies")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(reference).collect());
        let participants = self.participants(&bodies);
        if !participants.is_empty() {
            def["participants"] = json!(participants);
        }
        // The options (closed, solid, rails, centre line, conditions) are
        // decoded (mitcad#25); the first choice of the sections is read where
        // each has one region only.
        let unique = sections.iter().all(|s| s.len() == 1);
        let decoded = d.contains_key("isClosed") && unique;
        let candidates = choices(&sections, SECTION_CHOICES)
            .into_iter()
            .enumerate()
            .map(|(k, chosen)| {
                let mut def = def.clone();
                def["sections"] = Value::Array(chosen);
                Candidate {
                    guess: k > 0,
                    input_provenance: if k > 0 {
                        crate::InputProvenance::Inferred
                    } else if decoded {
                        crate::InputProvenance::Read
                    } else {
                        crate::InputProvenance::Unknown
                    },
                    ..Candidate::new(def)
                }
            })
            .collect();
        // The component's bodies: those of the first profile's sketch.
        let sketch = sections
            .iter()
            .flatten()
            .find(|s| s["type"] == "profile")
            .and_then(|s| s["sketch"].as_str())
            .and_then(|s| s.parse().ok());
        let operation = Self::operation(d.get("operation").and_then(Value::as_str));
        let readings = self.operation_readings(index, operation, &bodies, sketch);
        Ok(with_operations(candidates, &readings))
    }

    /// A loft section's end condition; none for a free one.
    fn end_condition(&self, condition: Option<&Value>) -> Option<Value> {
        let condition = condition?.as_object()?;
        // The dump's type names are `Loft…EndCondition` (`LoftTangentEndCondition`).
        let kind = condition.get("_type").and_then(Value::as_str)?;
        let kind = kind.strip_prefix("Loft").unwrap_or(kind);
        let param = |key: &str| condition.get(key).and_then(reference);
        let weight = || self.unitless(&param("weight")).unwrap_or(json!(1.0));
        Some(match kind {
            k if k.starts_with("Free") => return None,
            k if k.starts_with("PointSharp") => json!({"type": "point_sharp"}),
            k if k.starts_with("PointTangent") => {
                json!({"type": "point_tangent", "weight": weight()})
            }
            k if k.starts_with("Tangent") => json!({"type": "tangent", "weight": weight()}),
            k if k.starts_with("Smooth") => json!({"type": "smooth", "weight": weight()}),
            k if k.starts_with("Direction") => json!({"type": "direction",
                "angle": self.value(&param("angle")).unwrap_or(json!(0.0)), "weight": weight()}),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_take_the_first_options_first() {
        let options = vec![
            vec![json!("a0"), json!("a1"), json!("a2")],
            vec![json!("b0")],
            vec![json!("c0"), json!("c1")],
        ];
        let all = choices(&options, 10);
        assert_eq!(all.len(), 6);
        assert_eq!(all[0], vec![json!("a0"), json!("b0"), json!("c0")]);
        // Then a second option of one section.
        assert_eq!(all[1], vec![json!("a0"), json!("b0"), json!("c1")]);
        assert_eq!(all[2], vec![json!("a1"), json!("b0"), json!("c0")]);
        assert_eq!(choices(&options, 2).len(), 2);
    }
}
