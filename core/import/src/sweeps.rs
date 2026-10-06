// SPDX-License-Identifier: MIT
//! Sweeps, pipes and lofts (F3) from external dumps (`commands.md`, the
//! F3 import table). The stream decoder gives only their parameters, not
//! their profiles and paths, so from an .f3d alone they fall back to the
//! stored bodies, as do coils, ribs and webs, which dumps give no inputs of.

use mitcad_f3d::design::ir::{PipeDetail, Reference, SweepDetail, TimelineItem};
use mitcad_model::{EdgeName, Kernel};
use serde_json::{Map, Value, json};

use crate::{Candidate, Importer, refs};

fn reference(v: &Value) -> Option<Reference> {
    Some(Reference::from_map(v.as_object()?.clone()))
}

fn is_zero(r: &Option<Reference>) -> bool {
    r.as_ref()
        .and_then(Reference::parameter)
        .and_then(|p| p.value)
        .is_none_or(|v| v == 0.0)
}

/// A value present and not empty (a guide rail, guide surfaces).
fn given(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Array(a)) => !a.is_empty(),
        Some(_) => true,
    }
}

impl<K: Kernel> Importer<'_, K> {
    /// A `Path` of the dump (its `PathEntity` items, or entity references) as a
    /// Mitcad path: curves of one sketch or edges of one body.
    fn path_of(&self, path: Option<&Value>, what: &str) -> Result<Value, String> {
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
                    let (b, edge) = refs::resolve_edge(self.doc, &fp)
                        .ok_or_else(|| format!("an edge of its {what} was not found"))?;
                    let b = b.to_string();
                    if *body.get_or_insert_with(|| b.clone()) != b {
                        return Err(format!("its {what} runs over several bodies"));
                    }
                    edges.push(edge.to_string());
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
        let shape = self
            .doc
            .bodies()
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

    pub(crate) fn sweep(&mut self, index: i64, d: &SweepDetail) -> Result<Vec<Candidate>, String> {
        if d.is_solid == Some(false) {
            return Err("surface sweeps are not supported".to_owned());
        }
        if given(d.other.get("guideSurfaces")) {
            return Err("sweeps along guide surfaces are not supported".to_owned());
        }
        let profiles = d.profile.clone().unwrap_or_default();
        if profiles.is_empty() {
            return Err("its profiles were not decoded".to_owned());
        }
        let path = self.path_of(d.path.as_ref(), "path")?;
        let (sketch, _) = self.profile_sketch(&profiles, index)?;
        let sets = self.region_sets(&profiles, sketch, None, None)?;
        let uid = self.sketches[&sketch].uid.to_string();
        let mut base = json!({"type": "sweep", "path": path,
                              "operation": Self::operation(d.operation.as_deref())});
        if d.orientation
            .as_deref()
            .is_some_and(|o| o.starts_with("Parallel"))
        {
            base["orientation"] = json!("parallel");
        }
        if given(d.guide_rail.as_ref()) {
            base["guide_rail"] = self.path_of(d.guide_rail.as_ref(), "guide rail")?;
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
        let extent = self.path_extent(&d.distance_one, Some(&d.distance_two));
        if extent["type"] != "full" {
            base["extent"] = extent;
        }
        if d.other.get("isDirectionFlipped").and_then(Value::as_bool) == Some(true) {
            base["flip"] = json!(true);
        }
        let participants = self.participants(&d.participant_bodies);
        if !participants.is_empty() {
            base["participants"] = json!(participants);
        }
        Ok(sets
            .iter()
            .map(|set| {
                let mut def = base.clone();
                def["profiles"] = set
                    .keys
                    .iter()
                    .map(|r| json!({"sketch": uid, "region": r}))
                    .collect();
                Candidate {
                    defs: vec![def],
                    note: None,
                    guess: set.guess,
                    predicted: None,
                }
            })
            .collect())
    }

    pub(crate) fn pipe(&mut self, d: &PipeDetail) -> Result<Vec<Candidate>, String> {
        let path = self.path_of(d.path.as_ref(), "path")?;
        let size = self
            .value(&d.section_size)
            .ok_or("its section size was not decoded")?;
        let mut def = json!({"type": "pipe", "path": path, "size": size,
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
        let mut candidates = Vec::new();
        if !is_zero(&d.distance_two) {
            let mut both = def.clone();
            both["extent"] = self.path_extent(&d.distance_one, Some(&d.distance_two));
            candidates.push(Candidate::new(both));
        }
        let extent = self.path_extent(&d.distance_one, None);
        if extent["type"] != "full" {
            def["extent"] = extent;
        }
        candidates.push(Candidate::new(def));
        Ok(candidates)
    }

    pub(crate) fn loft(&mut self, item: &TimelineItem) -> Result<Vec<Candidate>, String> {
        let d: Map<String, Value> = item
            .detail
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        if d.get("isSolid").and_then(Value::as_bool) == Some(false) {
            return Err("surface lofts are not supported".to_owned());
        }
        let mut items: Vec<&Value> = d
            .get("loftSections")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .ok_or("its sections were not decoded")?;
        items.sort_by_key(|s| s.get("index").and_then(Value::as_i64).unwrap_or(0));
        let index = item.index.unwrap_or(0);
        let mut sections = Vec::new();
        for (i, section) in items.iter().enumerate() {
            let at = |e: String| format!("section {}: {e}", i + 1);
            // A path of edges (.f3d designs take tangent and smooth conditions
            // only at such a section): the face they go round.
            let raw = section.get("entity");
            let entity = raw.and_then(reference);
            if matches!(raw, Some(Value::Array(_))) || matches!(entity, Some(Reference::Edge(_))) {
                sections.push(self.edge_loop_face(raw).map_err(at)?);
                continue;
            }
            let entity = entity.ok_or_else(|| at("its entity was not decoded".to_owned()))?;
            sections.push(match entity {
                Reference::Profile(p) => {
                    let profile = Reference::Profile(p);
                    let (sketch, _) = self
                        .profile_sketch(std::slice::from_ref(&profile), index)
                        .map_err(at)?;
                    let sets = self
                        .region_sets(std::slice::from_ref(&profile), sketch, None, None)
                        .map_err(at)?;
                    let region = sets
                        .iter()
                        .find(|s| s.keys.len() == 1)
                        .map(|s| s.keys[0].clone())
                        .ok_or_else(|| at("its region was not found".to_owned()))?;
                    json!({"type": "profile", "sketch": self.sketches[&sketch].uid.to_string(),
                           "region": region})
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
            });
        }
        let mut def = json!({"type": "loft", "sections": sections,
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
                converted.push(self.path_of(Some(path), "centre line or rail")?);
            }
            if centerline {
                def["centerline"] = converted.into_iter().next().ok_or("no centre line")?;
            } else {
                def["rails"] = json!(converted);
            }
        }
        if d.get("isClosed").and_then(Value::as_bool) == Some(true) {
            def["closed"] = json!(true);
        }
        let participants: Vec<String> = d
            .get("participantBodies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(reference)
            .filter_map(|r| match r {
                Reference::Body(fp) => refs::resolve_body(self.doc, &fp),
                _ => None,
            })
            .map(|b| b.to_string())
            .collect();
        if !participants.is_empty() {
            def["participants"] = json!(participants);
        }
        Ok(vec![Candidate::new(def)])
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
