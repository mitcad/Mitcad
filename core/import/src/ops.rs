// SPDX-License-Identifier: MIT
//! Body and face operations of external dumps (their inputs are
//! fingerprints and feature references the stream decoder does not give):
//! combine, mirror, circular and rectangular patterns, shell, offset
//! faces, move, split body, hole, thread and replace face (`commands.md`,
//! F1, F2 and F4 import tables). Without those inputs a
//! combine is found with the history: the bodies the next state changed
//! are its target and tools; so are the faces a replace face replaced.

use mitcad_f3d::design::ir::{Geometry, Reference, TimelineItem, Vec3};
use mitcad_model::{BodyUid, Cylinder, FaceName, FeatureUid, Kernel, Plane};
use serde_json::{Map, Value, json};

use crate::geom::{self, mm, mm3};
use crate::history::Sig;
use crate::{Candidate, Importer, features, refs};

/// An item's detail as its JSON object (typed details written back).
fn detail_map(item: &TimelineItem) -> Map<String, Value> {
    item.detail
        .as_ref()
        .and_then(|d| serde_json::to_value(d).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn reference(v: &Value) -> Option<Reference> {
    Some(Reference::from_map(v.as_object()?.clone()))
}

fn references(v: Option<&Value>) -> Vec<Reference> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(reference).collect())
        .unwrap_or_default()
}

/// The dump's compute option as Mitcad's.
fn compute(option: Option<&Value>) -> &'static str {
    match option.and_then(Value::as_str) {
        Some(o) if o.starts_with("Identical") => "identical",
        Some(o) if o.starts_with("Optimized") => "optimized",
        _ => "adjust",
    }
}

/// A face a hole can start from: how far the removed piece's centre is
/// from half the depth below it, the face and the hole's point on it.
type StartFace = (f64, String, [f64; 3]);

/// The points of a hole feature on one face of a body.
type HolePoints = (BodyUid, String, Vec<[f64; 3]>);

/// A piece of material a history state removed: the body it came from,
/// its centre and its shape.
type Piece<S> = (BodyUid, [f64; 3], S);

/// Copies of features whose inputs the stream decoder does not give; the
/// quantities and the axis when it decodes them.
enum Copies {
    Circular {
        angle: Value,
        symmetric: bool,
        quantity: Option<i64>,
        axis: Option<Value>,
    },
    Rectangular {
        distance: Value,
        quantity: Option<i64>,
        quantity_two: Option<i64>,
        distance_two: Option<Value>,
    },
}

impl<K: Kernel> Importer<'_, K> {
    /// The volume the history shows the item at timeline `index` adding
    /// (negative: removing): its state against the one before.
    pub(crate) fn history_change(&mut self, index: i64) -> Option<f64> {
        let state = self.oracle.state_of_item(index)?;
        let kernel = self.doc.kernel();
        let volume = |s: Vec<Sig>| s.iter().map(|x| x.volume).sum::<f64>();
        let after = volume(self.oracle.try_sigs(kernel, state)?);
        let before = volume(self.oracle.try_sigs(kernel, state.checked_sub(1)?)?);
        Some(after - before)
    }

    /// How much volume the next history state has more than the replay.
    pub(crate) fn change_to_next(&mut self) -> Option<f64> {
        let next = self.oracle.next_index();
        let kernel = self.doc.kernel();
        let target: f64 = self
            .oracle
            .try_sigs(kernel, next)?
            .iter()
            .map(|s| s.volume)
            .sum();
        let now: f64 = self.current_sigs().iter().map(|s| s.volume).sum();
        Some(target - now)
    }

    /// Patterns without decoded inputs: copies of one of the last
    /// extrusions before the item, as many as the history's change asks
    /// for, about each origin axis (guesses the history checks).
    fn guessed_copies(
        &mut self,
        item: &TimelineItem,
        copies: Copies,
    ) -> Result<Vec<Candidate>, String> {
        let none = "no inputs decoded";
        if !self.oracle.enabled {
            return Err(format!("{none} (no history to guess them)"));
        }
        let index = item.index.ok_or(none)?;
        let change = self
            .change_to_next()
            .ok_or_else(|| format!("{none}, and its history state could not be rebuilt"))?;
        let mut recent: Vec<(i64, mitcad_model::FeatureUid)> = self
            .features
            .iter()
            .filter(|(i, _)| **i < index)
            .map(|(i, f)| (*i, *f))
            .collect();
        recent.sort_by_key(|r| std::cmp::Reverse(r.0));
        let mut inputs = Vec::new();
        for (i, uid) in recent {
            let extrusion = self
                .doc
                .feature(uid)
                .is_some_and(|f| matches!(f.def, mitcad_model::FeatureDef::Extrude(_)));
            if !extrusion {
                continue;
            }
            if let Some(added) = self.history_change(i).filter(|a| a.abs() > 1e-6) {
                inputs.push((uid, added));
            }
            if inputs.len() >= 3 {
                break;
            }
        }
        if inputs.is_empty() {
            return Err(format!("{none}, and no extrusion before it to copy"));
        }
        let mut out = Vec::new();
        for (uid, added) in inputs {
            let objects = json!({"type": "features", "features": [uid.to_string()]});
            // An extrusion up to an object is rebuilt at each copy by
            // Adjust (the default) and moved by Identical; by
            // distances both are the same.
            let to_object = self.doc.feature(uid).is_some_and(|f| match &f.def {
                mitcad_model::FeatureDef::Extrude(e) => {
                    use mitcad_model::features::{Extent, Side};
                    let side = |s: &Side| matches!(s, Side::ToObject { .. });
                    match &e.extent {
                        Extent::ToObject { .. } => true,
                        Extent::TwoSides { side1, side2 } => side(side1) || side(side2),
                        _ => false,
                    }
                }
                _ => false,
            });
            // The copies the change asks for (overlaps make it a guess),
            // unless the quantity is decoded.
            let estimate = 1.0 + change / added;
            let nearest = estimate.round().clamp(-1.0, 1e3) as i64;
            let decoded = match &copies {
                Copies::Circular { quantity, .. } | Copies::Rectangular { quantity, .. } => {
                    *quantity
                }
            };
            let mut quantities = match decoded {
                Some(q) => vec![q],
                None => vec![nearest, nearest + 1, nearest - 1],
            };
            quantities.retain(|q| (2..=1000).contains(q));
            // What `copies` more copies of the feature add (overlaps aside),
            // so that the likeliest feature and quantity come first. The
            // compute option is not decoded: Adjust, then for an extrusion
            // up to an object Identical (Adjust applies its copies in one
            // boolean operation too where that gives the same bodies).
            let candidate = |def: Value, copies: i64| {
                let one = |def: Value| Candidate {
                    defs: vec![def],
                    note: Some(format!(
                        "inputs not decoded: copies of {} checked against the history",
                        uid
                    )),
                    guess: true,
                    predicted: Some(added * copies as f64),
                };
                let mut out = Vec::new();
                if to_object {
                    let mut identical = def.clone();
                    identical["compute"] = json!("identical");
                    out.push(one(def));
                    out.push(one(identical));
                } else {
                    out.push(one(def));
                }
                out
            };
            match &copies {
                Copies::Circular {
                    angle,
                    symmetric,
                    axis,
                    ..
                } => {
                    let axes = match axis {
                        Some(a) => vec![a.clone()],
                        None => vec![json!("z"), json!("x"), json!("y")],
                    };
                    for q in &quantities {
                        for axis in &axes {
                            out.extend(candidate(
                                json!({"type": "circular_pattern",
                                "objects": objects, "axis": axis,
                                "quantity": q, "angle": angle, "symmetric": symmetric,
                                "compute": "adjust"}),
                                q - 1,
                            ));
                        }
                    }
                }
                Copies::Rectangular {
                    distance,
                    quantity_two,
                    distance_two,
                    ..
                } => {
                    // A second direction of one element changes nothing.
                    let two = quantity_two
                        .filter(|q| *q > 1)
                        .zip(distance_two.clone().filter(|d| d.as_f64() != Some(0.0)));
                    for q in &quantities {
                        for axis in ["x", "y", "z"] {
                            let mut def = json!({"type": "rectangular_pattern",
                                "objects": objects,
                                "direction1": {"axis": axis, "quantity": q,
                                               "distance": distance},
                                "distance_type": "spacing", "compute": "adjust"});
                            match &two {
                                Some((q2, d2)) => {
                                    // The second direction across the first.
                                    for other in ["x", "y", "z"].into_iter().filter(|o| *o != axis)
                                    {
                                        let mut d = def.clone();
                                        d["direction2"] =
                                            json!({"axis": other, "quantity": q2, "distance": d2});
                                        out.extend(candidate(d, q * q2 - 1));
                                    }
                                }
                                None => out.extend(candidate(def.take(), q - 1)),
                            }
                        }
                    }
                }
            }
        }
        if out.is_empty() {
            return Err(format!(
                "{none}, and no extrusion before it matches the history's change"
            ));
        }
        Ok(out)
    }

    /// A mirror of bodies without decoded inputs (guesses the history
    /// checks): the bodies of the item's component that the next state has
    /// twins of (the same volume and area) as new bodies; each body joined
    /// with its mirror image (a body the next state no longer has); each
    /// body as a new body. About the decoded plane, else each origin plane
    /// and the planes a body and its image in the next state are symmetric
    /// about (from their centres).
    fn mirrored_bodies(&mut self, plane: Option<Value>) -> Vec<Candidate> {
        if !self.oracle.enabled {
            return Vec::new();
        }
        let planes = match plane {
            Some(p) => vec![p],
            None => vec![json!("yz"), json!("xz"), json!("xy")],
        };
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, Sig)> = self
            .doc
            .bodies()
            .iter()
            .filter(|b| self.doc.body_component(b.uid) == Some(self.component))
            .filter_map(|b| Some((b.uid, Sig::of(kernel, b.shape)?)))
            .collect();
        let next = self.oracle.next_index();
        let target = self.oracle.try_sigs(kernel, next).unwrap_or_default();
        let current = self.current_sigs();
        let mut used = vec![false; current.len()];
        // The next state's bodies the replay does not have, and the
        // replay's bodies it no longer has.
        let new: Vec<Sig> = target
            .iter()
            .filter(
                |t| match (0..current.len()).find(|&j| !used[j] && current[j].same(t)) {
                    Some(j) => {
                        used[j] = true;
                        false
                    }
                    None => true,
                },
            )
            .copied()
            .collect();
        let gone: Vec<BodyUid> = bodies
            .iter()
            .filter(|(_, s)| !target.iter().any(|t| t.same(s)))
            .map(|(b, _)| *b)
            .collect();
        let rel = |x: f64, y: f64| (x - y).abs() / x.abs().max(y.abs()).max(1e-9);
        let twin = |a: &Sig, b: &Sig| rel(a.volume, b.volume) < 1e-4 && rel(a.area, b.area) < 1e-4;
        // The plane a body and its mirror image are symmetric about: half way
        // between their centres (a copy), or through the centre of the two
        // joined (a body twice as large).
        let between = |from: [f64; 3], to: [f64; 3]| -> Option<(Vec3, Vec3)> {
            let normal = geom::unit(geom::sub(to, from))?;
            Some((geom::scale(geom::add(from, to), 0.5), normal))
        };
        let mut derived: Vec<(Vec3, Vec3)> = Vec::new();
        for (_, s) in &bodies {
            for n in &new {
                let plane = if twin(n, s) {
                    between(s.center, n.center)
                } else if rel(n.volume, 2.0 * s.volume) < 1e-3 {
                    // The centre of the joined body is on the plane.
                    between(s.center, geom::sub(geom::scale(n.center, 2.0), s.center))
                } else {
                    None
                };
                if let Some(p) = plane
                    && !derived.iter().any(|q| {
                        geom::norm(geom::cross(p.1, q.1)) < 1e-6
                            && geom::dot(geom::sub(p.0, q.0), q.1).abs() < 1e-4
                    })
                {
                    derived.push(p);
                }
            }
        }
        let mut planes = planes;
        for (origin, normal) in derived.into_iter().take(3) {
            planes.push(self.plane_at(origin, normal));
        }
        let mut out = Vec::new();
        let push = |out: &mut Vec<Candidate>, objects: Vec<String>, combine: bool, volume: f64| {
            for plane in &planes {
                out.push(Candidate {
                    defs: vec![json!({"type": "mirror",
                        "objects": {"type": "bodies", "bodies": objects},
                        "plane": plane, "combine": combine, "compute": "adjust"})],
                    note: Some(format!(
                        "inputs not decoded: {} of {} checked against the history",
                        if combine { "a joined mirror" } else { "copies" },
                        objects.join(", ")
                    )),
                    guess: true,
                    predicted: Some(volume),
                });
            }
        };
        let twins: Vec<(BodyUid, Sig)> = bodies
            .iter()
            .filter(|(_, s)| new.iter().any(|n| twin(n, s)))
            .copied()
            .collect();
        if !twins.is_empty() && twins.len() <= new.len() {
            let volume = twins.iter().map(|(_, s)| s.volume).sum();
            push(
                &mut out,
                twins.iter().map(|(b, _)| b.to_string()).collect(),
                false,
                volume,
            );
        }
        for (body, sig) in bodies.iter().filter(|(b, _)| gone.contains(b)).take(8) {
            push(&mut out, vec![body.to_string()], true, sig.volume);
        }
        for (body, sig) in bodies.iter().take(8) {
            push(&mut out, vec![body.to_string()], false, sig.volume);
        }
        out
    }

    /// A mirror of features without decoded inputs: sets of up to four of
    /// the features before the item that Mitcad can mirror (they make a
    /// tool body), whose volume changes in the history add up to the next
    /// state's change, about the decoded plane, else each origin plane
    /// (guesses the history checks, the closest sums first).
    fn mirrored_features(&mut self, item: &TimelineItem, plane: Option<Value>) -> Vec<Candidate> {
        let (Some(index), true) = (item.index, self.oracle.enabled) else {
            return Vec::new();
        };
        let Some(change) = self.change_to_next().filter(|c| c.abs() > 1e-6) else {
            return Vec::new();
        };
        let mut recent: Vec<(i64, mitcad_model::FeatureUid)> = self
            .features
            .iter()
            .filter(|(i, _)| **i < index)
            .map(|(i, f)| (*i, *f))
            .collect();
        recent.sort_by_key(|r| std::cmp::Reverse(r.0));
        let mut inputs: Vec<(mitcad_model::FeatureUid, f64)> = Vec::new();
        for (i, uid) in recent {
            let mirrorable = self
                .doc
                .feature(uid)
                .is_some_and(|f| f.def.info().tool_use().is_some());
            if !mirrorable {
                continue;
            }
            if let Some(added) = self.history_change(i).filter(|a| a.abs() > 1e-6) {
                inputs.push((uid, added));
            }
            if inputs.len() >= 10 {
                break;
            }
        }
        // Subsets by how close their sum comes to the change.
        let mut sets: Vec<(f64, Vec<usize>)> = Vec::new();
        for k in 1..=inputs.len().min(4) {
            features::combinations(inputs.len(), k, 2000, &mut |picked: &[usize]| {
                let sum: f64 = picked.iter().map(|&p| inputs[p].1).sum();
                let off = (sum - change).abs() / change.abs();
                if off < 0.05 {
                    sets.push((off, picked.to_vec()));
                }
            });
        }
        sets.sort_by(|a, b| a.0.total_cmp(&b.0));
        let planes = match plane {
            Some(p) => vec![p],
            None => vec![json!("yz"), json!("xz"), json!("xy")],
        };
        let mut out = Vec::new();
        for (_, picked) in sets.into_iter().take(4) {
            let uids: Vec<String> = picked.iter().map(|&p| inputs[p].0.to_string()).collect();
            let sum: f64 = picked.iter().map(|&p| inputs[p].1).sum();
            for plane in &planes {
                out.push(Candidate {
                    defs: vec![json!({"type": "mirror",
                        "objects": {"type": "features", "features": uids},
                        "plane": plane, "compute": "adjust"})],
                    note: Some(format!(
                        "inputs not decoded: a mirror of {} checked against the history",
                        uids.join(", ")
                    )),
                    guess: true,
                    predicted: Some(sum),
                });
            }
        }
        out
    }

    /// A plane through `origin` with `normal`: a planar face of the
    /// replay's bodies that lies on it, else the plane itself, fixed.
    fn plane_at(&self, origin: Vec3, normal: Vec3) -> Value {
        let on = refs::planar_faces(self.doc).into_iter().find(|(_, _, p)| {
            geom::norm(geom::cross(p.normal, normal)) < 1e-6
                && geom::dot(geom::sub(p.origin, origin), normal).abs() < 1e-4
        });
        match on {
            Some((body, face, _)) => json!({"body": body.to_string(), "face": face.to_string()}),
            None => json!({"origin": origin, "normal": normal}),
        }
    }

    /// A value from a reflected parameter reference.
    fn value_of(&self, v: Option<&Value>) -> Option<Value> {
        let r = reference(v?)?;
        self.value(&Some(r))
    }

    fn body_of(&self, r: &Reference) -> Option<BodyUid> {
        match r {
            Reference::Body(fp) => refs::resolve_body(self.doc, fp),
            _ => None,
        }
    }

    /// Pattern and mirror objects: features, bodies or faces of one body.
    fn objects(&self, inputs: &[Reference]) -> Result<Value, String> {
        let mut features = Vec::new();
        let mut bodies = Vec::new();
        let mut faces: Vec<(BodyUid, String)> = Vec::new();
        for r in inputs {
            match r {
                Reference::Feature(f) => {
                    let uid = f
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))
                        .ok_or("a patterned feature was not imported")?;
                    features.push(uid.to_string());
                }
                Reference::Body(fp) => bodies.push(
                    refs::resolve_body(self.doc, fp)
                        .ok_or("a body was not found in the replay")?
                        .to_string(),
                ),
                Reference::Face(fp) => {
                    let (b, f) = refs::resolve_face(self.doc, fp)
                        .ok_or("a face was not found in the replay")?;
                    faces.push((b, f.to_string()));
                }
                _ => return Err("an input of an unsupported kind".to_owned()),
            }
        }
        match (features.is_empty(), bodies.is_empty(), faces.is_empty()) {
            (false, true, true) => Ok(json!({"type": "features", "features": features})),
            (true, false, true) => Ok(json!({"type": "bodies", "bodies": bodies})),
            (true, true, false) => {
                let body = faces[0].0;
                if faces.iter().any(|(b, _)| *b != body) {
                    return Err("faces of several bodies".to_owned());
                }
                let names: Vec<String> = faces.into_iter().map(|(_, f)| f).collect();
                Ok(json!({"type": "faces", "body": body.to_string(), "faces": names}))
            }
            _ => Err("no inputs, or inputs of several kinds".to_owned()),
        }
    }

    /// An axis reference (`GeomRef`): origin or construction axis, edge or
    /// face.
    fn axis_ref(&self, r: &Reference) -> Option<Value> {
        match r {
            Reference::ConstructionAxis(c) => match c.origin.clone().flatten().as_deref() {
                Some(o @ ("X" | "Y" | "Z")) => Some(json!(o.to_lowercase())),
                _ => {
                    if let Some(uid) = c
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))
                    {
                        return Some(json!(uid.to_string()));
                    }
                    fixed_axis(c.geometry.as_ref()?)
                }
            },
            Reference::Edge(fp) => {
                let (body, edge) = refs::resolve_edge(self.doc, fp)?;
                Some(json!({"body": body.to_string(), "edge": edge.to_string()}))
            }
            Reference::Face(fp) => {
                let (body, face) = refs::resolve_face(self.doc, fp)?;
                Some(json!({"body": body.to_string(), "face": face.to_string()}))
            }
            _ => None,
        }
    }

    /// A pattern axis: an axis reference, or a line of an imported sketch.
    fn pattern_axis(&self, r: &Reference) -> Option<Value> {
        if let Reference::SketchEntity(e) = r {
            let s = self.sketches.get(&e.sketch_timeline_index.flatten()?)?;
            let curve = s.ids.get(&e.id.clone().flatten()?)?;
            return Some(json!({"sketch": s.uid.to_string(), "curve": curve}));
        }
        self.axis_ref(r)
    }

    /// A plane reference (`GeomRef`): origin plane, planar face, or a fixed
    /// plane from a construction plane's geometry.
    fn plane_ref(&self, r: &Reference) -> Option<Value> {
        match r {
            Reference::ConstructionPlane(c) => match c.origin.clone().flatten().as_deref() {
                Some(o @ ("XY" | "XZ" | "YZ")) => Some(json!(o.to_lowercase())),
                _ => {
                    if let Some(uid) = c
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))
                    {
                        return Some(json!(uid.to_string()));
                    }
                    let g = c.geometry.as_ref()?;
                    let mut v = json!({"origin": mm3(g.origin?), "normal": geom::unit(g.normal?)?});
                    if let Some(x) = g.u_direction.and_then(geom::unit) {
                        v["x_axis"] = json!(x);
                    }
                    Some(v)
                }
            },
            Reference::Face(fp) => {
                let (body, face) = refs::resolve_face(self.doc, fp)?;
                Some(json!({"body": body.to_string(), "face": face.to_string()}))
            }
            _ => None,
        }
    }

    pub(crate) fn translate_op(
        &mut self,
        object_type: &str,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let other = detail_map(item);
        let get = |k: &str| other.get(k);
        let flag = |_: &TimelineItem, k: &str| other.get(k).and_then(Value::as_bool);
        let compute = |_: &TimelineItem| compute(other.get("patternComputeOption"));
        match object_type {
            "CombineFeature" => {
                let target = get("targetBody").and_then(reference);
                let tools = references(get("toolBodies"));
                if target.is_none() && tools.is_empty() {
                    return self.combine_by_history();
                }
                let target = target
                    .as_ref()
                    .and_then(|r| self.body_of(r))
                    .ok_or("the target body was not found")?;
                let tools: Vec<String> = tools
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a tool body was not found")?;
                let operation = match get("operation").and_then(Value::as_str) {
                    Some(o) if o.starts_with("Cut") => "cut",
                    Some(o) if o.starts_with("Intersect") => "intersect",
                    _ => "join",
                };
                // A new component's result is the target in the component
                // the import put the item in (the dump's components, F6).
                Ok(vec![Candidate::new(
                    json!({"type": "combine", "target": target.to_string(),
                    "tools": tools, "operation": operation,
                    "keep_tools": get("isKeepToolBodies").and_then(Value::as_bool).unwrap_or(false)}),
                )])
            }
            "MirrorFeature" | "CircularPatternFeature" | "RectangularPatternFeature"
                if references(get("inputEntities")).is_empty() =>
            {
                // Decoded counts are values, not parameters.
                let count = |key: &str| {
                    get(key)
                        .and_then(reference)
                        .and_then(|r| r.parameter()?.value)
                        .map(|v| v.round() as i64)
                };
                let mirror_plane = get("mirrorPlane")
                    .and_then(reference)
                    .and_then(|r| self.plane_ref(&r));
                let copies = match object_type {
                    "MirrorFeature" => {
                        // Mirrors of features, else of bodies.
                        let mut out = self.mirrored_features(item, mirror_plane.clone());
                        out.extend(self.mirrored_bodies(mirror_plane));
                        if out.is_empty() {
                            return Err(if self.oracle.enabled {
                                "no inputs decoded, and no features or bodies before it match \
                                 the history's change"
                                    .to_owned()
                            } else {
                                "no inputs decoded (no history to guess them)".to_owned()
                            });
                        }
                        return Ok(out);
                    }
                    "CircularPatternFeature" => Copies::Circular {
                        angle: self
                            .value_of(get("totalAngle"))
                            .unwrap_or(json!(std::f64::consts::TAU)),
                        symmetric: flag(item, "isSymmetric").unwrap_or(false),
                        quantity: count("quantity"),
                        axis: get("axis")
                            .and_then(reference)
                            .and_then(|r| self.pattern_axis(&r)),
                    },
                    _ => Copies::Rectangular {
                        distance: self
                            .value_of(get("distanceOne"))
                            .ok_or("no inputs, and the distance was not decoded")?,
                        quantity: count("quantityOne"),
                        quantity_two: count("quantityTwo"),
                        distance_two: self.value_of(get("distanceTwo")),
                    },
                };
                self.guessed_copies(item, copies)
            }
            "MirrorFeature" => {
                let objects = self.objects(&references(get("inputEntities")))?;
                let plane = get("mirrorPlane")
                    .and_then(reference)
                    .and_then(|r| self.plane_ref(&r))
                    .ok_or("the mirror plane was not decoded")?;
                Ok(vec![Candidate::new(
                    json!({"type": "mirror", "objects": objects,
                    "plane": plane, "combine": flag(item, "isCombine").unwrap_or(false),
                    "compute": compute(item)}),
                )])
            }
            "CircularPatternFeature" => {
                let objects = self.objects(&references(get("inputEntities")))?;
                let axis = get("axis")
                    .and_then(reference)
                    .and_then(|r| self.axis_ref(&r))
                    .ok_or("the pattern axis was not decoded")?;
                let quantity = self
                    .value_of(get("quantity"))
                    .ok_or("the quantity was not decoded")?;
                let angle = self
                    .value_of(get("totalAngle"))
                    .ok_or("the angle was not decoded")?;
                let mut def = json!({"type": "circular_pattern", "objects": objects, "axis": axis,
                    "quantity": quantity, "angle": angle,
                    "symmetric": flag(item, "isSymmetric").unwrap_or(false),
                    "compute": compute(item)});
                suppressed(&mut def, get("suppressedElementsIds"));
                Ok(vec![Candidate::new(def)])
            }
            "RectangularPatternFeature" => {
                let objects = self.objects(&references(get("inputEntities")))?;
                let direction = |entity: &str,
                                 vector: &str,
                                 quantity: &str,
                                 distance: &str,
                                 symmetric: &str| {
                    let axis = get(entity)
                        .and_then(reference)
                        .and_then(|r| self.axis_ref(&r))
                        .or_else(|| {
                            let d: [f64; 3] = serde_json::from_value(get(vector)?.clone()).ok()?;
                            Some(json!({"origin": [0.0, 0.0, 0.0], "direction": geom::unit(d)?}))
                        })?;
                    Some(
                        json!({"axis": axis, "quantity": self.value_of(get(quantity))?,
                                "distance": self.value_of(get(distance))?,
                                "symmetric": get(symmetric).and_then(Value::as_bool).unwrap_or(false)}),
                    )
                };
                let one = direction(
                    "directionOneEntity",
                    "directionOne",
                    "quantityOne",
                    "distanceOne",
                    "isSymmetricInDirectionOne",
                )
                .ok_or("direction one was not decoded")?;
                let mut def = json!({"type": "rectangular_pattern", "objects": objects,
                "direction1": one, "compute": compute(item),
                "distance_type": match get("patternDistanceType").and_then(Value::as_str) {
                    Some(t) if t.starts_with("Extent") => "extent",
                    _ => "spacing",
                }});
                if let Some(two) = direction(
                    "directionTwoEntity",
                    "directionTwo",
                    "quantityTwo",
                    "distanceTwo",
                    "isSymmetricInDirectionTwo",
                ) {
                    def["direction2"] = two;
                }
                suppressed(&mut def, get("suppressedElementsIds"));
                Ok(vec![Candidate::new(def)])
            }
            "ShellFeature" => {
                let inputs = references(get("inputEntities"));
                let mut faces = Vec::new();
                let mut body = None;
                for r in &inputs {
                    match r {
                        Reference::Face(fp) => {
                            let (b, f) = refs::resolve_face(self.doc, fp)
                                .ok_or("a shell face was not found")?;
                            body = Some(b);
                            faces.push(f.to_string());
                        }
                        Reference::Body(_) => body = self.body_of(r),
                        _ => {}
                    }
                }
                let body = body.ok_or("the shelled body was not found")?;
                let mut def = json!({"type": "shell", "body": body.to_string(), "faces": faces,
                    "tangent_chain": flag(item, "isTangentChain").unwrap_or(true),
                    "rounded": get("shellType").and_then(Value::as_str)
                        .is_some_and(|t| t.starts_with("Rounded"))});
                if let Some(v) = self.value_of(get("insideThickness")) {
                    def["inside"] = v;
                }
                if let Some(v) = self.value_of(get("outsideThickness")) {
                    def["outside"] = v;
                }
                Ok(vec![Candidate::new(def)])
            }
            "OffsetFacesFeature" => {
                let mut inputs = references(get("inputFaces"));
                if inputs.is_empty() {
                    inputs = references(get("faces"));
                }
                let distance = self
                    .value_of(get("distance"))
                    .ok_or("the distance was not decoded")?;
                let mut by_body: Vec<(BodyUid, Vec<String>)> = Vec::new();
                for r in &inputs {
                    let Reference::Face(fp) = r else { continue };
                    let (b, f) = refs::resolve_face(self.doc, fp).ok_or("a face was not found")?;
                    match by_body.iter_mut().find(|(x, _)| *x == b) {
                        Some((_, faces)) => faces.push(f.to_string()),
                        None => by_body.push((b, vec![f.to_string()])),
                    }
                }
                if by_body.is_empty() {
                    return Err("its faces were not decoded".to_owned());
                }
                let defs = by_body
                    .into_iter()
                    .map(|(b, faces)| {
                        json!({"type": "offset_face", "body": b.to_string(),
                                             "faces": faces, "distance": distance})
                    })
                    .collect();
                Ok(vec![Candidate {
                    defs,
                    note: None,
                    guess: false,
                    predicted: None,
                }])
            }
            "MoveFeature" => {
                let bodies: Vec<String> = references(get("inputEntities"))
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a moved body was not found (moves of faces are not supported)")?;
                let m: [[f64; 4]; 4] = get("transform")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .ok_or("the move's transform was not decoded")?;
                let matrix: Vec<[f64; 4]> = (0..3)
                    .map(|r| [m[r][0], m[r][1], m[r][2], mm(m[r][3])])
                    .collect();
                Ok(vec![
                    Candidate::new(json!({"type": "move", "bodies": bodies,
                    "transform": {"type": "free", "matrix": matrix}}))
                    .with_note("as a fixed transform"),
                ])
            }
            "SplitBodyFeature" => {
                let bodies: Vec<String> = references(get("splitBodies"))
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a split body was not found")?;
                let tool = match get("splittingTool").and_then(reference) {
                    Some(Reference::ConstructionPlane(c)) => {
                        match c.origin.clone().flatten().as_deref() {
                            Some(o @ ("XY" | "XZ" | "YZ")) => json!(o.to_lowercase()),
                            _ => {
                                let g = c
                                    .geometry
                                    .as_ref()
                                    .ok_or("the splitting plane was not decoded")?;
                                json!({"origin": mm3(g.origin.ok_or("no origin")?),
                                   "normal": geom::unit(g.normal.ok_or("no normal")?)})
                            }
                        }
                    }
                    Some(Reference::Face(fp)) => {
                        let (b, f) = refs::resolve_face(self.doc, &fp)
                            .ok_or("the splitting face was not found")?;
                        json!({"body": b.to_string(), "face": f.to_string()})
                    }
                    Some(r @ Reference::Body(_)) => {
                        let b = self.body_of(&r).ok_or("the splitting body was not found")?;
                        json!({"body": b.to_string()})
                    }
                    _ => return Err("the splitting tool was not decoded".to_owned()),
                };
                Ok(vec![Candidate::new(
                    json!({"type": "split_body", "bodies": bodies, "tool": tool,
                    "extend": flag(item, "isSplittingToolExtended").unwrap_or(true)}),
                )])
            }
            "HoleFeature" => self.hole(&other),
            "ThreadFeature" => self.thread(&other),
            "ReplaceFaceFeature" => self.replace_face(&other),
            other => Err(format!("{other} is not supported yet")),
        }
    }

    /// A hole at a point of a planar face (F1 import table); without a
    /// decoded position, where the history state shows material removed.
    fn hole(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let get = |k: &str| d.get(k);
        let position: Option<[f64; 3]> =
            get("position").and_then(|p| serde_json::from_value(p.clone()).ok());
        let Some(position) = position else {
            if !self.oracle.enabled {
                return Err("its position was not decoded".to_owned());
            }
            return self.hole_by_history(d);
        };
        let at = mm3(position);
        // The face: one named by the position definition, else the planar
        // face through the position.
        let named = get("holePositionDefinition")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|m| m.values())
            .filter_map(reference)
            .find_map(|r| match r {
                Reference::Face(fp) => refs::resolve_face(self.doc, &fp),
                _ => None,
            });
        let (body, face) = match named {
            Some(f) => f,
            None => refs::planar_faces(self.doc)
                .into_iter()
                .find(|(_, _, p)| geom::dot(geom::sub(at, p.origin), p.normal).abs() < 1e-4)
                .map(|(b, f, _)| (b, f))
                .ok_or("no planar face through its position")?,
        };
        let kind = get("holeType")
            .and_then(Value::as_str)
            .ok_or("its hole type was not decoded")?;
        Ok(vec![Candidate::new(self.hole_def(
            d,
            body,
            &face.to_string(),
            &[at],
            kind,
        )?)])
    }

    /// A hole from the history: each piece of material the item's state
    /// lost is a hole. Its axis is that of the piece's cylinder (the bore);
    /// the hole starts on the plane of a planar face across the axis at
    /// either end of the piece (the point need not lie on the face: a hole
    /// may start in a groove), its point where the axis meets the plane.
    /// Without a cylinder the piece's centre is projected onto the face it
    /// starts from. The hole type is not decoded either: simple, then the
    /// kinds whose sizes the item has; the depth, then through all.
    fn hole_by_history(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let depth = d
            .get("extentDefinition")
            .and_then(Value::as_object)
            .and_then(|e| e.get("distance"))
            .and_then(reference)
            .and_then(|r| r.parameter()?.value)
            .map(mm);
        let radius = d
            .get("holeDiameter")
            .and_then(reference)
            .and_then(|r| r.parameter()?.value)
            .map(|v| mm(v) / 2.0);
        let pieces = self.removed_pieces()?;
        let faces = refs::planar_faces(self.doc);
        let kernel = self.doc.kernel();
        // Per piece the faces it can start from, nearest to half the depth
        // first.
        let mut options: Vec<(BodyUid, Vec<StartFace>)> = Vec::new();
        for (body, center, piece) in pieces {
            if let Some(found) = axis_starts(kernel, &piece, radius, &faces, body) {
                options.push((body, found));
                continue;
            }
            let Some(shape) = self
                .doc
                .bodies()
                .iter()
                .find(|b| b.uid == body)
                .map(|b| b.shape.clone())
            else {
                continue;
            };
            let mut found: Vec<StartFace> = Vec::new();
            for (_, face, plane) in faces.iter().filter(|(b, _, _)| *b == body) {
                let h = geom::dot(geom::sub(center, plane.origin), plane.normal);
                if h >= -1e-6 {
                    continue;
                }
                let at = geom::sub(center, geom::scale(plane.normal, h));
                let eps = 1e-3;
                let probe = [
                    geom::sub(at, geom::scale(plane.normal, eps)),
                    geom::add(at, geom::scale(plane.normal, eps)),
                ];
                // Material just below the point, none above: the hole
                // starts on this face here (the body before the hole).
                match kernel.points_inside(&shape, &probe) {
                    Ok(v) if v == [true, false] => {}
                    _ => continue,
                }
                let score = depth.map_or(-h, |dd| (-h - dd / 2.0).abs());
                found.push((score, face.to_string(), at));
            }
            found.sort_by(|a, b| a.0.total_cmp(&b.0));
            if !found.is_empty() {
                options.push((body, found));
            }
        }
        if options.is_empty() {
            return Err("its position was not decoded, and no face was found where the history removes material".to_owned());
        }
        // The best face of each piece, then the second best (a hole through
        // a plate fits both of its sides).
        let mut placements: Vec<Vec<HolePoints>> = Vec::new();
        for rank in 0..2 {
            let mut groups: Vec<HolePoints> = Vec::new();
            for (body, found) in &options {
                let (_, face, at) = found.get(rank).unwrap_or(&found[0]);
                match groups.iter_mut().find(|(b, f, _)| b == body && f == face) {
                    Some(g) => g.2.push(*at),
                    None => groups.push((*body, face.clone(), vec![*at])),
                }
            }
            if !placements.contains(&groups) {
                placements.push(groups);
            }
        }
        let mut kinds = vec!["Simple"];
        if d.contains_key("counterboreDiameter") && d.contains_key("counterboreDepth") {
            kinds.push("Counterbore");
        }
        if d.contains_key("countersinkDiameter") && d.contains_key("countersinkAngle") {
            kinds.push("Countersink");
        }
        let mut candidates = Vec::new();
        // The extent type is not decoded: the depth, else through all.
        for through in [false, true] {
            for groups in &placements {
                for kind in &kinds {
                    let mut defs = Vec::new();
                    for (body, face, points) in groups {
                        let mut def = self.hole_def(d, *body, face, points, kind)?;
                        if through {
                            def["extent"] = json!({"type": "through_all"});
                        }
                        defs.push(def);
                    }
                    candidates.push(Candidate {
                        defs,
                        note: Some("placed where the history removes material".to_owned()),
                        guess: true,
                        predicted: None,
                    });
                }
            }
        }
        Ok(candidates)
    }

    /// The material the next history state removes from the replay's
    /// bodies: each piece with the body it came from, its centre and its
    /// shape.
    fn removed_pieces(&mut self) -> Result<Vec<Piece<K::Shape>>, String> {
        let next = self.oracle.next_index();
        let kernel = self.doc.kernel();
        let state: Vec<(crate::StoredBody<K::Shape>, Sig)> =
            self.oracle.state(kernel, next)?.to_vec();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.uid, b.shape.clone(), Sig::of(kernel, b.shape)))
            .collect();
        let mut used = vec![false; state.len()];
        let mut changed = Vec::new();
        for (uid, shape, sig) in &bodies {
            let Some(sig) = sig else { continue };
            match (0..state.len()).find(|&j| !used[j] && state[j].1.same(sig)) {
                Some(j) => used[j] = true,
                None => changed.push((*uid, shape, *sig)),
            }
        }
        let mut out = Vec::new();
        for (uid, shape, sig) in changed {
            let Some(j) = (0..state.len()).filter(|&j| !used[j]).min_by(|&a, &b| {
                let d = |j: usize| geom::distance(state[j].1.center, sig.center);
                d(a).total_cmp(&d(b))
            }) else {
                continue;
            };
            used[j] = true;
            let cut = kernel
                .boolean(mitcad_model::BooleanOp::Cut, &[shape], &state[j].0.shape)
                .map_err(|e| e.to_string())?;
            let pieces: Vec<(f64, [f64; 3], K::Shape)> = cut
                .pieces
                .iter()
                .filter_map(|p| {
                    let m = kernel.mass_properties(&p.shape).ok()?;
                    Some((m.volume, m.center, p.shape.clone()))
                })
                .collect();
            // Slivers along the faces both share are no holes.
            let largest = pieces.iter().map(|p| p.0).fold(0.0, f64::max);
            out.extend(
                pieces
                    .into_iter()
                    .filter(|(v, _, _)| *v > 1e-3 * largest && *v > 1e-9)
                    .map(|(_, c, s)| (uid, c, s)),
            );
        }
        if out.is_empty() {
            return Err("the history shows no material removed".to_owned());
        }
        Ok(out)
    }

    /// A hole definition of the item's sizes at points of a face.
    fn hole_def(
        &self,
        d: &Map<String, Value>,
        body: BodyUid,
        face: &str,
        points: &[[f64; 3]],
        kind: &str,
    ) -> Result<Value, String> {
        let get = |k: &str| d.get(k);
        let mut def = json!({"type": "hole",
            "placement": {"type": "face", "body": body.to_string(), "face": face, "points": points},
            "diameter": self.value_of(get("holeDiameter")).ok_or("no diameter")?});
        match Some(kind) {
            Some(t) if t.starts_with("Counterbore") => {
                def["kind"] = json!({"type": "counterbore",
                    "diameter": self.value_of(get("counterboreDiameter")).ok_or("no counterbore diameter")?,
                    "depth": self.value_of(get("counterboreDepth")).ok_or("no counterbore depth")?});
            }
            Some(t) if t.starts_with("Countersink") => {
                def["kind"] = json!({"type": "countersink",
                    "diameter": self.value_of(get("countersinkDiameter")).ok_or("no countersink diameter")?,
                    "angle": self.value_of(get("countersinkAngle")).ok_or("no countersink angle")?});
            }
            Some(t) if t.starts_with("Simple") => {}
            _ => return Err(format!("hole type {kind} is not supported")),
        }
        let tip = get("tipAngle").and_then(reference);
        let flat = tip
            .as_ref()
            .and_then(Reference::parameter)
            .and_then(|p| p.value)
            .is_some_and(|a| (a - std::f64::consts::PI).abs() < 1e-9);
        if flat {
            def["flat"] = json!(true);
        } else if let Some(v) = self.value(&tip) {
            def["tip_angle"] = v;
        }
        let extent = get("extentDefinition").and_then(Value::as_object);
        def["extent"] = match extent.and_then(|e| e.get("_type")).and_then(Value::as_str) {
            Some("DistanceExtentDefinition") => json!({"type": "distance",
                "depth": self.value_of(extent.and_then(|e| e.get("distance"))).ok_or("no depth")?}),
            Some("ThroughAllExtentDefinition") => json!({"type": "through_all"}),
            _ => return Err("its extent was not decoded".to_owned()),
        };
        if get("isDefaultDirection").and_then(Value::as_bool) == Some(false) {
            def["flip"] = json!(true);
        }
        let participants: Vec<String> = references(get("participantBodies"))
            .iter()
            .filter_map(|r| self.body_of(r))
            .map(|b| b.to_string())
            .collect();
        if !participants.is_empty() {
            def["participants"] = json!(participants);
        }
        if get("holeTapType")
            .and_then(Value::as_str)
            .is_some_and(|t| t.starts_with("Tapped"))
        {
            let info = get("tappedHoleInfo").and_then(Value::as_object);
            let thread = get("thread").and_then(Value::as_object);
            let mut spec = thread_spec(info, thread)?;
            if let Some(t) = thread {
                spec["modeled"] =
                    json!(t.get("isModeled").and_then(Value::as_bool).unwrap_or(false));
                if t.get("isFullLength").and_then(Value::as_bool) == Some(false) {
                    if let Some(l) = self.value_of(t.get("threadLength")) {
                        spec["length"] = l;
                    }
                    if let Some(o) = self.value_of(t.get("threadOffset")) {
                        spec["offset"] = o;
                    }
                }
            }
            def["thread"] = spec;
        }
        Ok(def)
    }

    /// Threads on cylindrical faces (F1 import table).
    fn thread(&self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let mut faces = references(d.get("inputCylindricalFaces"));
        if faces.is_empty() {
            faces.extend(d.get("inputCylindricalFace").and_then(reference));
        }
        let faces: Vec<Value> = faces
            .iter()
            .map(|r| match r {
                Reference::Face(fp) => refs::resolve_face(self.doc, fp)
                    .map(|(b, f)| json!({"body": b.to_string(), "face": f.to_string()})),
                _ => None,
            })
            .collect::<Option<_>>()
            .ok_or("a threaded face was not found")?;
        if faces.is_empty() {
            return Err("its faces were not decoded".to_owned());
        }
        let info = d.get("threadInfo").and_then(Value::as_object);
        let mut def = json!({"type": "thread", "faces": faces, "thread": thread_spec(info, Some(d))?,
            "modeled": d.get("isModeled").and_then(Value::as_bool).unwrap_or(false)});
        if d.get("isFullLength").and_then(Value::as_bool) == Some(false) {
            def["length"] = self
                .value_of(d.get("threadLength"))
                .ok_or("no thread length")?;
            if let Some(o) = self.value_of(d.get("threadOffset")) {
                def["offset"] = o;
            }
            def["location"] = json!(match d.get("threadLocation").and_then(Value::as_str) {
                Some(l) if l.starts_with("Low") => "low_end",
                _ => "high_end",
            });
        }
        Ok(vec![Candidate::new(def)])
    }

    /// A combine the dump does not describe (the stream decoder): the
    /// bodies the next history states changed are its target and tools.
    fn combine_by_history(&mut self) -> Result<Vec<Candidate>, String> {
        if !self.oracle.enabled {
            return Err("its bodies were not decoded (no history to find them)".to_owned());
        }
        let kernel = self.doc.kernel();
        let current: Vec<(BodyUid, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.uid, Sig::of(kernel, b.shape)))
            .collect();
        let mut candidates = Vec::new();
        for q in self.lookahead(3) {
            let Ok(state) = self.oracle.state(kernel, q) else {
                continue;
            };
            let mut used = vec![false; state.len()];
            let mut changed = Vec::new();
            for (uid, sig) in &current {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None => changed.push(*uid),
                }
            }
            if changed.is_empty() {
                continue;
            }
            let others: Vec<BodyUid> = current
                .iter()
                .map(|(u, _)| *u)
                .filter(|u| !changed.contains(u))
                .collect();
            let mut push = |target: BodyUid, tools: Vec<BodyUid>, keep: bool| {
                for operation in ["join", "cut", "intersect"] {
                    let def = json!({"type": "combine", "target": target.to_string(),
                        "tools": tools.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "operation": operation, "keep_tools": keep});
                    if !candidates.iter().any(|c: &Candidate| c.defs[0] == def) {
                        let mut c = Candidate::new(def);
                        c.guess = true;
                        c.note = Some("target and tools found with the history".to_owned());
                        candidates.push(c);
                    }
                }
            };
            // Tools removed: the changed bodies are the target and tools.
            if changed.len() >= 2 {
                for &target in &changed {
                    let tools = changed.iter().copied().filter(|b| *b != target).collect();
                    push(target, tools, false);
                }
            }
            // Tools kept: one changed body, any other body as the tool.
            if changed.len() == 1 {
                for &tool in &others {
                    push(changed[0], vec![tool], true);
                }
            }
        }
        if candidates.is_empty() {
            return Err("no body changes in the next history states".to_owned());
        }
        Ok(candidates)
    }

    /// A replace face target (`targetFaces`): a construction plane, a face
    /// (as a fixed plane when it is planar and not in the replay), or the
    /// faces of one surface body (the body when they are all of it).
    fn replace_target(&self, targets: &[Reference]) -> Result<Value, String> {
        if let [r @ Reference::ConstructionPlane(_)] = targets {
            return self
                .plane_ref(r)
                .ok_or_else(|| "the target plane was not decoded".to_owned());
        }
        if let [Reference::Body(fp)] = targets {
            let body = refs::resolve_sheet(self.doc, fp).ok_or("the target body was not found")?;
            return Ok(json!({"body": body.to_string()}));
        }
        let mut faces: Vec<(BodyUid, String)> = Vec::new();
        for r in targets {
            let Reference::Face(fp) = r else {
                return Err("a target of an unsupported kind".to_owned());
            };
            match refs::resolve_face(self.doc, fp) {
                Some((b, f)) => faces.push((b, f.to_string())),
                None => {
                    // A planar face of a body the replay does not have.
                    let g = fp.geometry.as_ref().filter(|g| {
                        targets.len() == 1 && g.geometry_type.as_deref() == Some("Plane")
                    });
                    let plane = g.and_then(|g| Some((g.origin?, geom::unit(g.normal?)?)));
                    return match plane {
                        Some((origin, normal)) => {
                            Ok(json!({"origin": mm3(origin), "normal": normal}))
                        }
                        None => Err("the target face was not found in the replay".to_owned()),
                    };
                }
            }
        }
        match faces.as_slice() {
            [] => Err("its target was not decoded".to_owned()),
            [(b, f)] => Ok(json!({"body": b.to_string(), "face": f})),
            [(b, _), ..] => {
                if faces.iter().any(|(x, _)| x != b) {
                    return Err("target faces of several bodies".to_owned());
                }
                let shape = self
                    .doc
                    .body_shape(*b)
                    .ok_or("the target body was not found")?;
                let all = self.doc.kernel().face_count(shape).ok();
                if all != Some(faces.len()) {
                    return Err("target faces that are part of a body".to_owned());
                }
                Ok(json!({"body": b.to_string()}))
            }
        }
    }

    /// A replace face (P5). A dump does not name the faces replaced
    /// (`sourceFaces` is read when a dump has it):
    /// the history gives them, the faces of the body a next state changed
    /// that it no longer has. They are listed in full without the tangent
    /// chain, since Mitcad's chain also takes faces tangent to them that
    /// stay (a slot's sides at its round end).
    fn replace_face(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let get = |k: &str| d.get(k);
        let targets = references(get("targetFaces"));
        if targets.is_empty() {
            return Err("its target was not decoded".to_owned());
        }
        let target = self.replace_target(&targets)?;
        let chain = get("isTangentChain")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut sources = references(get("sourceFaces"));
        sources.extend(references(get("inputFaces")));
        if !sources.is_empty() {
            let mut body = None;
            let mut faces = Vec::new();
            for r in &sources {
                let Reference::Face(fp) = r else { continue };
                let (b, f) = refs::resolve_face(self.doc, fp).ok_or("a face was not found")?;
                if body.is_some_and(|x| x != b) {
                    return Err("faces of several bodies".to_owned());
                }
                body = Some(b);
                faces.push(f.to_string());
            }
            let body = body.ok_or("its faces were not decoded")?;
            return Ok(vec![Candidate::new(
                json!({"type": "replace_face", "body": body.to_string(), "faces": faces,
                       "target": target, "tangent_chain": chain}),
            )]);
        }
        if !self.oracle.enabled {
            return Err("its source faces were not decoded (no history to find them)".to_owned());
        }
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.uid, b.shape.clone(), Sig::of(kernel, b.shape)))
            .collect();
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut last_error = None;
        for q in self.lookahead(3) {
            let kernel = self.doc.kernel();
            let state: Vec<(crate::StoredBody<K::Shape>, Sig)> = match self.oracle.state(kernel, q)
            {
                Ok(s) => s.to_vec(),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            // The replay's bodies without an equal in the state, against
            // the state's without one in the replay.
            let mut used = vec![false; state.len()];
            let mut changed = Vec::new();
            for (uid, shape, sig) in &bodies {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None if sig.is_some() => changed.push((*uid, shape)),
                    None => {}
                }
            }
            let after: Vec<&K::Shape> = state
                .iter()
                .zip(&used)
                .filter(|(_, u)| !**u)
                .map(|((b, _), _)| &b.shape)
                .collect();
            for (uid, shape) in changed {
                let lost = match refs::lost_faces(kernel, shape, &after) {
                    Ok(l) => l,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
                if lost.is_empty() {
                    continue;
                }
                let faces: Vec<String> = lost.iter().map(ToString::to_string).collect();
                let def = json!({"type": "replace_face", "body": uid.to_string(), "faces": faces,
                                 "target": target, "tangent_chain": false});
                if !candidates.iter().any(|c| c.defs[0] == def) {
                    let mut c = Candidate::new(def);
                    c.guess = true;
                    c.note = Some(format!(
                        "the faces replaced found with the history (tangent chain {})",
                        if chain { "on in the file" } else { "off" }
                    ));
                    candidates.push(c);
                }
            }
        }
        if candidates.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                "its source faces were not decoded, and no next history state lost faces".to_owned()
            }));
        }
        Ok(candidates)
    }
}

/// A thread size from the dump's ThreadInfo (and the thread feature's flags).
fn thread_spec(
    info: Option<&Map<String, Value>>,
    feature: Option<&Map<String, Value>>,
) -> Result<Value, String> {
    let info = info.ok_or("its thread was not decoded")?;
    let standard = match info.get("threadType").and_then(Value::as_str) {
        Some(t) if t.contains("Metric") => "iso_metric",
        Some(t) if t.contains("Unified") => "unified",
        Some(t) => return Err(format!("thread type {t} is not supported")),
        None => "iso_metric",
    };
    let designation = info
        .get("threadDesignation")
        .and_then(Value::as_str)
        .ok_or("no thread designation")?;
    let mut spec = json!({"standard": standard, "designation": designation});
    if let Some(class) = info.get("threadClass").and_then(Value::as_str) {
        spec["class"] = json!(class);
    }
    let right = feature
        .and_then(|f| f.get("isRightHanded"))
        .or_else(|| info.get("isRightHanded"))
        .and_then(Value::as_bool);
    if let Some(right) = right {
        spec["right_handed"] = json!(right);
    }
    Ok(spec)
}

fn suppressed(def: &mut Value, ids: Option<&Value>) {
    let ids: Vec<u64> = ids
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    if !ids.is_empty() {
        def["suppressed_elements"] = json!(ids);
    }
}

/// The faces a hole along the bore of a removed `piece` of `body` can start
/// from: the planes of the body's planar faces across the bore's axis at
/// either end of the piece, each with the point where the axis meets it.
/// The bore is the piece's cylinder nearest the hole's radius (the longest
/// without one). None when the piece has no cylinder or no such plane.
fn axis_starts<K: Kernel>(
    kernel: &K,
    piece: &K::Shape,
    radius: Option<f64>,
    faces: &[(BodyUid, FaceName, Plane)],
    body: BodyUid,
) -> Option<Vec<StartFace>> {
    // The piece's faces are named to be asked about.
    let data = kernel.brep_data(piece).ok()?;
    let (named, _) = kernel.import_brep(FeatureUid(0), &data, 0).ok()?;
    let mut bore: Option<(f64, Cylinder)> = None;
    for face in kernel.faces(&named).ok()? {
        let Some(name) = face.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
            continue;
        };
        if face.surface != "cylinder" {
            continue;
        }
        let Ok(c) = kernel.face_cylinder(&named, &name) else {
            continue;
        };
        let score = radius.map_or(-c.length, |r| (c.radius - r).abs());
        if bore.is_none_or(|(s, _)| score < s) {
            bore = Some((score, c));
        }
    }
    let (_, bore) = bore?;
    let along = geom::unit(bore.axis.direction)?;
    let origin = bore.axis.origin;
    // The piece's extent along the axis (exact for an axis along the
    // model's axes).
    let bounds = kernel.bounding_box(piece).ok()??;
    let reach: Vec<f64> = bounds
        .corners()
        .map(|p| geom::dot(geom::sub(p, origin), along))
        .collect();
    let low = reach.iter().copied().fold(f64::INFINITY, f64::min);
    let high = reach.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut found: Vec<StartFace> = Vec::new();
    for (_, face, plane) in faces.iter().filter(|(b, _, _)| *b == body) {
        let slope = geom::dot(plane.normal, along);
        if slope.abs() < 1.0 - 1e-6 {
            continue;
        }
        // The hole runs against the face's normal, into the material: the
        // plane lies at the end of the piece the normal points to.
        let at = geom::dot(geom::sub(plane.origin, origin), along);
        let end = if slope > 0.0 { high } else { low };
        if (at - end).abs() > 1e-3 {
            continue;
        }
        let point = geom::add(origin, geom::scale(along, at));
        let same_plane = found
            .iter()
            .any(|(_, _, p)| geom::distance(*p, point) < 1e-6);
        if !same_plane {
            found.push(((at - end).abs(), face.to_string(), point));
        }
    }
    (!found.is_empty()).then_some(found)
}

/// A fixed axis from a construction axis' line geometry.
fn fixed_axis(g: &Geometry) -> Option<Value> {
    let origin = g.origin.or(g.start_point)?;
    let direction = g
        .direction
        .or_else(|| Some(geom::sub(g.end_point?, g.start_point?)))?;
    Some(json!({"origin": mm3(origin), "direction": geom::unit(direction)?}))
}
