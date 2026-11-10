// SPDX-License-Identifier: MIT
//! Timeline items as MitCad feature definitions (`commands.md`; the IR
//! field mappings of the F1, F2 and F4 import tables).
//!
//! Each item gives candidates, best first. What the dump leaves open (the
//! stream decoder knows how many profiles an extrude uses but not which,
//! the direction may be uncertain, fillet edges are not decoded) becomes
//! several candidates, and the ASM history picks the one whose result
//! the file has; without a history only the first, which is not a guess,
//! is used.

use std::collections::{BTreeSet, HashMap};

use mitcad_dump::{
    ChamferDetail, CoilDetail, Definition, Detail, EdgeSet, EmbossDetail, ExtrudeDetail,
    ExtrudeF3d, FilletDetail, Fingerprint, Geometry, Reference, RevolveDetail, SketchDetail,
    TimelineItem,
};
use mitcad_model::assembly::inverse;
use mitcad_model::datum::Datum;
use mitcad_model::features::{SketchOutput, SketchPlane};
use mitcad_model::sketch::regions::Region;
use mitcad_model::sketch::solve::SolverItem;
use mitcad_model::{
    BodyUid, ComponentUid, FaceName, FeatureUid, Kernel, SketchFrame, TopoName, Transform,
};
use serde_json::{Value, json};

use crate::geom::{self, mm, mm3};
use crate::history::{Sig, StoredBody};
use crate::refs::Section;
use crate::report::{InputProvenance, Outcome};
use crate::scaffold::CandidateCore;
use crate::sketch::{self, inactive, position_error};
use crate::snap;
use crate::{Candidate, Guesses, Importer, SketchInfo, linked, refs};

/// Items whose candidates depend on the bodies before them (edges found
/// against the replay, faces extruded), so a fallback before them may make
/// them work.
pub(crate) fn depends_on_bodies(item: &TimelineItem) -> bool {
    match item.object_type() {
        Some(
            "FilletFeature" | "ChamferFeature" | "CombineFeature" | "ReplaceFaceFeature"
            | "DeleteFaceFeature" | "MoveFaceFeature" | "DraftFeature",
        ) => true,
        // Paths, rails or sections of edges (mitcad#34).
        Some("SweepFeature" | "PipeFeature" | "LoftFeature") => crate::sweeps::uses_edges(item),
        // An extrusion of a face (no profile decoded).
        Some("ExtrudeFeature") => match &item.detail {
            Some(Detail::Extrude(d)) => profiles_of(&d.profile, &d.other).is_empty(),
            _ => false,
        },
        _ => false,
    }
}

/// The largest error of solved sketch points accepted, mm.
const SKETCH_TOLERANCE: f64 = 1e-4;
/// How far (mm) the points of reference curves may move where the sketch
/// is solved with them free (`sketch::position_error_slack`).
const REFERENCE_SLACK: f64 = 0.05;
/// How far (turns) a coil's retry shifts the seams of the helix's pieces
/// (`piece_shift`, mitcad#235): on a coil whose cut gave the file's state
/// only with its seams moved, shifts of 1/12 to 1/4 of a turn worked.
const COIL_PIECE_SHIFT: f64 = 1.0 / 6.0;

fn param(r: &Option<Reference>) -> Option<&mitcad_dump::ParameterRef> {
    r.as_ref().and_then(Reference::parameter)
}

/// Whether every entity of a datum's definition is one the file names
/// outright ([`is_direct`]); there is at least one, and no path (a path's
/// edges are guessed in order and direction).
fn direct_entities(def: &Definition) -> bool {
    // (A tangent plane without the plane its angle is measured from has it
    // guessed.)
    if def.definition_type.as_deref() == Some("ConstructionPlaneTangentDefinition")
        && def.planar_entity.is_none()
    {
        return false;
    }
    let mut entities: Vec<Reference> = def.planar_entity.iter().cloned().collect();
    entities.extend(
        def.other
            .values()
            .filter_map(Value::as_object)
            .filter(|m| m.contains_key("kind"))
            .map(|m| Reference::from_map(m.clone())),
    );
    !entities.is_empty()
        && !def.other.contains_key("pathEntities")
        && entities.iter().all(is_direct)
}

/// Whether a datum's entity is one the file names outright (an origin or
/// construction datum of the timeline; a face, edge or vertex found in the
/// history; a sketch entity), not one known only by its geometry
/// (mitcad#186).
fn is_direct(r: &Reference) -> bool {
    let named = |c: &mitcad_dump::ConstructionRef| {
        c.origin.clone().flatten().is_some() || c.timeline_index.flatten().is_some()
    };
    match r {
        Reference::ConstructionPlane(c)
        | Reference::ConstructionAxis(c)
        | Reference::ConstructionPoint(c) => named(c),
        Reference::Face(fp) => fp.point_on_face.is_some(),
        Reference::Edge(fp) => fp.mid_point.is_some(),
        Reference::Vertex(fp) => fp.point.is_some(),
        Reference::SketchEntity(_) => true,
        // The plane of the profile's sketch.
        Reference::Profile(p) => p.sketch_timeline_index.flatten().is_some(),
        _ => false,
    }
}

/// Why items of a type that changes bodies are not translated
/// (mitcad#43); the history's states stand in for them.
fn not_translated(object_type: &str) -> Option<&'static str> {
    Some(match object_type {
        "FlangeFeature" => "a sheet metal flange: not translated (MitCad has no sheet metal)",
        "PCBFeature" => "an electronics board: not translated",
        "MirrorComponent" => {
            "mirrors a component into a new one: not translated (the stored bodies stand in)"
        }
        "ComponentFromBodies" => {
            "moves bodies into a new component: not translated (the stored bodies stand in)"
        }
        // (mitcad#641) The derive follows the other design, which the file
        // does not hold, and none of this design's parameters.
        "ExternalDerived" => {
            "bodies derived from another design, which the file does not hold: static in the \
             source too, its stored bodies stand in"
        }
        _ => return None,
    })
}

/// An extrusion's extent to try: (extent, flip, guess, length swept in mm
/// when known).
type ExtentChoice = (Value, bool, bool, Option<f64>);

/// A sketch's plane: the definition's `plane` and `plane_link` (a face of
/// another component, mitcad#225), the plane's frame and import notes.
type SketchPlaneChoice = (Value, Option<Value>, SketchFrame, Vec<String>);

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// A parameter reference as a value: the imported parameter's name, or
    /// its value (mm or rad).
    pub fn value(&self, r: &Option<Reference>) -> Option<Value> {
        let p = param(r)?;
        if let Some(name) = p.name.as_deref().and_then(|n| self.params.get(n)) {
            return Some(json!(name));
        }
        let v = p.value?;
        // Angles are radians in both; counts (unit "") have no unit.
        let raw = p
            .unit
            .as_deref()
            .is_some_and(|u| u.is_empty() || u.contains("deg") || u.contains("rad"));
        Some(json!(if raw { v } else { mm(v) }))
    }

    pub fn value_or(&self, r: &Option<Reference>, what: &str) -> Result<Value, String> {
        self.value(r).ok_or_else(|| format!("no {what}"))
    }

    /// Whether a parameter's value is zero (an unused taper).
    fn is_zero(r: &Option<Reference>) -> bool {
        param(r).and_then(|p| p.value).is_none_or(|v| v == 0.0)
    }

    // Sketches.

    /// Imports a sketch; `copy` imports it again into the item's component,
    /// moved from its own component's coordinates by the transform (see
    /// [`Importer::copy_sketch`]).
    pub(crate) fn import_sketch(
        &mut self,
        index: i64,
        item: &TimelineItem,
        copy: Option<&Transform>,
    ) -> Result<(FeatureUid, Outcome, String), String> {
        let Some(Detail::Sketch(detail)) = &item.detail else {
            return Err("no sketch data".to_owned());
        };
        let frame = sketch_frame(detail).ok_or("the sketch's placement was not decoded")?;
        let frame = copy.map_or(frame, |t| t.apply_frame(&frame));
        let mut parts = sketch::translate(detail, &self.params);
        // The texts that follow the file's text parameters (mitcad#664).
        let doc_params = self.doc.parameters();
        parts.text_values = sketch::text_values(&parts, self.params.texts_of(index), &|name| {
            doc_params
                .find(name)
                .and_then(|id| doc_params.text(id))
                .map(str::to_owned)
        });
        let name = item.name();
        let start = self.doc.undo_depth();
        let owned = self.components.is_owned(index);
        let (plane, plane_link, plane_frame, mut notes) =
            self.sketch_plane(detail, &frame, name, copy, owned, &parts.positions)?;
        // The curves on the edges of the face the sketch lies on within the
        // file's tolerance, exactly on them (a point two of them share where
        // their curves meet); then the other linked curves on the geometry
        // they follow (the stored numbers are rounded, the replay's edges
        // are exact).
        let carriers = self.face_carriers(&plane, &frame);
        let on_edges = sketch::edges::snap_to_edges(
            &mut parts.entities,
            &mut parts.positions,
            &carriers,
            sketch::edges::EDGE_TOLERANCE,
        );
        if crate::tracing() && !on_edges.is_empty() {
            eprintln!("import: {name:?}: curves put on the edges of its face: {on_edges:?}");
        }
        if !on_edges.is_empty() {
            notes.push(format!(
                "{} curves within {} mm of the edges of the face it lies on put on them",
                on_edges.len(),
                sketch::edges::EDGE_TOLERANCE
            ));
        }
        // The edges the linked curves may follow (mitcad#225): the sketch's
        // own component's and, through their occurrences, the other
        // components'. A copy keeps its own component's, as fixed geometry.
        let target = if parts.linked > 0 && copy.is_none() {
            self.sketch_target()
        } else {
            None
        };
        let sources = match &target {
            Some(t) => self.edge_sources(t, &frame, linked::points_box(&parts)),
            None => Vec::new(),
        };
        // The projection rule's choice of edge per curve, made once: the
        // snap puts the curve on that edge's projection and the projection
        // follows the same edge (mitcad#227).
        let mut choices = if target.is_some() {
            linked::choices(&parts, &frame, &sources)
        } else {
            Vec::new()
        };
        // Copies in place of the body under the sketch: its own edge.
        if let Some(body) = plane
            .get("body")
            .and_then(Value::as_str)
            .and_then(|b| b.parse::<BodyUid>().ok())
        {
            linked::prefer_own_body(&mut choices, &sources, body);
        }
        let snapped = if parts.linked > 0 {
            let own = owned && target.is_some();
            let edges = if own {
                sources.iter().map(|e| e.curve.clone()).collect()
            } else {
                let component = (copy.is_some() || owned).then_some(self.component);
                self.replay_edges(component)
            };
            let preferred = if own {
                linked::preferred(&choices)
            } else {
                Vec::new()
            };
            let kept = sketch::edges::points_of(&parts.entities, &on_edges);
            snap::snap_linked_keeping(&mut parts, &frame, &edges, &kept, &preferred)
        } else {
            snap::Snapped::default()
        };
        // Those that follow one edge as projections.
        let mut resolved = match &target {
            Some(t) => linked::resolve(&mut parts, &frame, &sources, &choices, t, &mut |e| {
                self.edge_curves(e, t)
            }),
            None => linked::Resolved::default(),
        };
        // The others that are the projection of an edge of any kind, or of
        // several edges end to end (mitcad#389).
        if let Some(t) = &target
            && parts.linked > resolved.projections.len()
        {
            let made: Vec<&str> = resolved
                .projections
                .iter()
                .flat_map(|p| p["entities"].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .collect();
            let skip: Vec<usize> = parts
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| e["id"].as_str().is_some_and(|id| made.contains(&id)))
                .map(|(i, _)| i)
                .collect();
            let found = linked::chains(
                &parts,
                &frame,
                &sources,
                &skip,
                refs::geometric_tolerance(),
                &mut |e| self.edge_curves(e, t),
            );
            if crate::tracing() {
                for c in &found {
                    eprintln!(
                        "import:   linked {} follows {} edges: {:?}",
                        parts.entities[c.entity]["id"],
                        c.edges.len(),
                        c.edges
                            .iter()
                            .map(|k| &sources[*k].edge)
                            .collect::<Vec<_>>()
                    );
                }
            }
            linked::resolve_chains(&mut parts, &frame, &sources, &found, t, &mut resolved);
        }
        if crate::tracing() && parts.linked > 0 {
            eprintln!(
                "import: {name:?} in {} (owned {owned}, seen at {target:?}): {} linked curves, {} edges they may follow, {} projections ({} of other components)",
                self.component,
                parts.linked,
                sources.len(),
                resolved.projections.len(),
                resolved.other
            );
        }
        let frame_def = geom::relative_frame(&plane_frame, &frame);
        let made_params = self.offset_params(&mut parts);
        // Every constraint and dimension the file has, active (level 0); the
        // fewest that keep the sketch off the file's geometry inactive and
        // everything else active (level 1, mitcad#721); all of them
        // inactive, without the patterns and offsets (level 2).
        let all_items = inactive::item_ids(&parts.constraints, &parts.dimensions);
        // The curves the file keeps as reference geometry (linked to the model)
        // are locked in place; where their positions do not meet exactly (the
        // ends two of them share lie off one of them) no solution keeps them
        // all: they are tried free too, each level.
        let unlocked_entities: Vec<Value> = parts
            .entities
            .iter()
            .cloned()
            .map(|mut e| {
                if e["reference"] == true
                    && let Some(map) = e.as_object_mut()
                {
                    map.remove("fixed");
                }
                e
            })
            .collect();
        // Free, but with the points that hold the sketch's own curves (a
        // linked arc's end an own circle passes through) where the file has
        // them, tried first: the reference curves' inconsistency then stays
        // in their other points instead of moving the own curves.
        let pinned =
            sketch::pinned_reference_points(&parts.entities, &parts.constraints, &parts.dimensions);
        let pinned_entities: Vec<Value> = unlocked_entities
            .iter()
            .cloned()
            .map(|mut e| {
                if e["type"] == "point" && e["id"].as_str().is_some_and(|id| pinned.contains(id)) {
                    e["fixed"] = json!(true);
                }
                e
            })
            .collect();
        let has_reference = parts
            .entities
            .iter()
            .any(|e| e["reference"] == true && e["fixed"] == true);
        let has_derived = parts.offsets.iter().any(|o| o["derived"] == true);
        let reference_points =
            sketch::reference_points(&parts.entities, &parts.constraints, &parts.dimensions);
        let mut last = String::new();
        // Level 1 searches one variant only (mitcad#723): the first whose
        // sketch solves with everything inactive, where the import before
        // inactive items solved without constraints. A variant that does not
        // costs one sketch add; the search up to about 66.
        let mut searched = false;
        for level in 0..3 {
            // Reference curves locked (with the linked ones that follow an
            // edge as projections, then without), then free with the points
            // that hold the own curves pinned, then free; offsets MitCad
            // derives from their chains (round corners), then without those.
            for (unlocked, pin, keep_derived, project) in [
                (false, false, true, true),
                (false, false, true, false),
                (true, true, true, false),
                (true, false, true, false),
                (false, false, false, true),
                (false, false, false, false),
                (true, true, false, false),
                (true, false, false, false),
            ] {
                if unlocked && !has_reference
                    || pin && pinned.is_empty()
                    || !keep_derived && !has_derived
                    || project && resolved.projections.is_empty()
                {
                    continue;
                }
                let entities = if pin {
                    &pinned_entities
                } else if unlocked {
                    &unlocked_entities
                } else {
                    &parts.entities
                };
                let check = SketchCheck {
                    unlocked,
                    positions: &parts.positions,
                    reference_points: &reference_points,
                    entities,
                };
                // The sketch with the constraints and dimensions of
                // `off` inactive; patterns and offsets named there are
                // left out.
                let def_with = |off: &BTreeSet<String>| -> Value {
                    let left = |v: &Value| !v["id"].as_str().is_some_and(|id| off.contains(id));
                    let mut def = json!({"type": "sketch", "plane": plane, "entities": entities,
                                     "constraints": inactive::mark(&parts.constraints, off),
                                     "dimensions": inactive::mark(&parts.dimensions, off),
                                     "texts": parts.texts});
                    if let Some(link) = &plane_link {
                        def["plane_link"] = link.clone();
                    }
                    // Texts that follow the file's text parameters
                    // (mitcad#664).
                    if !parts.text_values.is_empty() {
                        def["text_values"] = json!(parts.text_values);
                    }
                    if project {
                        def["projections"] = json!(resolved.projections);
                    }
                    // Patterns and offsets go with the constraints.
                    if level < 2 {
                        def["patterns"] = parts
                            .patterns
                            .iter()
                            .filter(|v| left(v))
                            .cloned()
                            .collect::<Vec<_>>()
                            .into();
                        def["offsets"] = parts
                            .offsets
                            .iter()
                            .filter(|v| left(v) && (keep_derived || v["derived"] != true))
                            .cloned()
                            .collect::<Vec<_>>()
                            .into();
                    }
                    if let Some(f) = &frame_def {
                        def["frame"] = serde_json::to_value(f).expect("serializes");
                    }
                    def
                };
                let mut off: BTreeSet<String> = match level {
                    0 => BTreeSet::new(),
                    1 => {
                        // Everything inactive first: whether the file's
                        // positions solve at all, and what of the constraints
                        // and dimensions they do not meet. Then the fewest of
                        // them inactive.
                        let everything: BTreeSet<String> = all_items.iter().cloned().collect();
                        if everything.is_empty() || searched {
                            continue;
                        }
                        let Some(unmet) = self.sketch_attempt(&def_with(&everything), name, &check)
                        else {
                            continue;
                        };
                        searched = true;
                        if crate::tracing() {
                            eprintln!("import:   {name:?}: not met in the file: {unmet:?}");
                        }
                        inactive::fewest(&unmet, &all_items, &mut |set| {
                            self.sketch_attempt(&def_with(set), name, &check).is_some()
                        })
                    }
                    _ => all_items.iter().cloned().collect(),
                };
                // The level as the file has it; then, where the file's own
                // positions break some of its constraints (a sketch that was
                // not solved again after the geometry it is linked to
                // changed), with those inactive.
                for round in 0..2 {
                    let def = def_with(&off);
                    let depth = self.doc.undo_depth();
                    match self.add_sketch_def(&def, name) {
                        Ok(uid) => {
                            let output = self.doc.sketch_output(uid);
                            let (error, within) = check.within(output, &def);
                            if !within {
                                last = format!("the solved sketch moved by {error:.3e} mm");
                                if crate::tracing() {
                                    eprintln!(
                                        "import: [{:.2}] {name:?}: sketch level {level}: {last}",
                                        crate::trace_clock()
                                    );
                                    trace_worst(&def, &parts.positions, output);
                                }
                                // What the file's positions do not keep.
                                let violated: Vec<String> = output
                                    .map(|o| {
                                        o.status()
                                            .stored_violations
                                            .iter()
                                            .filter_map(|v| match v {
                                                SolverItem::Constraint(k) => Some(k.to_string()),
                                                SolverItem::Fixed(_) => None,
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                self.undo_to(depth);
                                // (Pinned, nothing is made inactive: where
                                // the pinned points do not solve, the free
                                // curves are tried as before.)
                                if level == 0 && round == 0 && !violated.is_empty() && !pin {
                                    if crate::tracing() {
                                        eprintln!(
                                            "import:   not kept by the file's positions: {}",
                                            violated.join(", ")
                                        );
                                    }
                                    off = violated.into_iter().collect();
                                    continue;
                                }
                                break;
                            }
                            self.sketches.insert(
                                index,
                                SketchInfo {
                                    uid,
                                    ids: parts.ids.clone(),
                                    frame,
                                },
                            );
                            self.adopt(index, uid);
                            if !made_params.is_empty() {
                                let _ = self.doc.adopt_parameters(uid, &made_params);
                            }
                            if level == 2 {
                                notes.push(
                                    "no subset of its constraints and dimensions solves to the \
                                     file's geometry: all of them inactive, patterns and offsets \
                                     left out"
                                        .to_owned(),
                                );
                            } else if let Some(note) =
                                inactive::summary(&parts.constraints, &parts.dimensions, &off)
                            {
                                notes.push(note);
                            }
                            if error > SKETCH_TOLERANCE {
                                notes.push(format!(
                                    "solved within the geometric tolerance: points up to \
                                     {error:.1e} mm from the file's"
                                ));
                                if error > refs::EXACT {
                                    refs::note_deviation(error);
                                }
                            }
                            if unlocked {
                                notes.push(
                                    "reference curves not locked: their ends do not meet exactly"
                                        .to_owned(),
                                );
                            }
                            if !keep_derived {
                                notes.push(
                                    "left out: an offset whose corners MitCad's offset does not make \
                                     as the file's"
                                        .to_owned(),
                                );
                            }
                            // Patterns and offsets the file's own positions
                            // do not keep cannot be inactive: left out.
                            let groups = parts
                                .patterns
                                .iter()
                                .chain(&parts.offsets)
                                .filter(|v| v["id"].as_str().is_some_and(|id| off.contains(id)))
                                .count();
                            if level < 2 && groups > 0 {
                                notes.push(format!(
                                    "left out: {groups} patterns and offsets the file's own \
                                     positions do not keep"
                                ));
                            }
                            notes.extend(linked::notes(
                                parts.linked,
                                &resolved,
                                project,
                                target.is_some(),
                                snapped.moved,
                                snapped.left,
                            ));
                            if !parts.dropped.is_empty() {
                                notes.push(format!("left out: {}", summarize(&parts.dropped)));
                            }
                            let partial = level > 0
                                || !keep_derived
                                || !off.is_empty()
                                || !parts.dropped.is_empty();
                            let outcome = if partial {
                                Outcome::Partial
                            } else {
                                Outcome::Parametric
                            };
                            return Ok((uid, outcome, notes.join("; ")));
                        }
                        Err(e) => {
                            if crate::tracing() {
                                eprintln!(
                                    "import: [{:.2}] {name:?}: sketch level {level}: {e}",
                                    crate::trace_clock()
                                );
                            }
                            last = e;
                            break;
                        }
                    }
                }
            }
        }
        // Not the plane made for it either.
        self.undo_to(start);
        Err(last)
    }

    /// Adds a sketch definition: without its driven dimensions' references
    /// where the model refuses a file's parameter as one (a feature before
    /// the sketch uses it; the dimensions get their own, mitcad#250), and
    /// without its texts' text parameters where the model refuses one (the
    /// texts keep the file's strings, mitcad#664).
    fn add_sketch_def(&mut self, def: &Value, name: Option<&str>) -> Result<FeatureUid, String> {
        let added = match self.add(def, name) {
            Err(_) if sketch::has_references(def) => {
                self.add(&sketch::without_references(def), name)
            }
            other => other,
        };
        match added {
            Err(_) if def.get("text_values").is_some() => {
                let plain = sketch::without_text_values(def);
                match self.add(&plain, name) {
                    Err(_) if sketch::has_references(&plain) => {
                        self.add(&sketch::without_references(&plain), name)
                    }
                    other => other,
                }
            }
            other => other,
        }
    }

    /// Adds a sketch definition, checks that it solves to the file's
    /// geometry and takes it back (mitcad#721): Some with the inactive
    /// constraints and dimensions its solution does not meet when it does,
    /// None when it does not.
    fn sketch_attempt(
        &mut self,
        def: &Value,
        name: Option<&str>,
        check: &SketchCheck<'_>,
    ) -> Option<Vec<String>> {
        let depth = self.doc.undo_depth();
        let result = self.add_sketch_def(def, name).ok().and_then(|uid| {
            let output = self.doc.sketch_output(uid);
            check.within(output, def).1.then(|| {
                output
                    .map(|o| {
                        o.status()
                            .unmet_inactive
                            .iter()
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default()
            })
        });
        self.undo_to(depth);
        result
    }

    /// The curves of the edges of the replay's bodies (of a component's
    /// bodies when given), in model space.
    fn replay_edges(
        &self,
        component: Option<mitcad_model::ComponentUid>,
    ) -> Vec<mitcad_model::datum::CurveGeometry> {
        let kernel = self.doc.kernel();
        let mut out = Vec::new();
        for body in crate::body_scope::bodies(self.doc) {
            if component.is_some_and(|c| self.doc.body_component(body.uid) != Some(c)) {
                continue;
            }
            crate::tick();
            if let Ok(curves) = kernel.edge_geometries(body.shape) {
                out.extend(curves.into_iter().map(|(_, curve)| curve));
            }
        }
        out
    }

    /// Creates the parameters a sketch's offsets need (the magnitude of a
    /// negative distance parameter, shared by the offset's dimensions and
    /// record) and puts their names in place of the placeholders. Returns
    /// their names, for the sketch to adopt.
    fn offset_params(&mut self, parts: &mut sketch::SketchParts) -> Vec<String> {
        let mut made = Vec::new();
        for (placeholder, suggested, expression) in std::mem::take(&mut parts.new_params) {
            let name = (0..)
                .map(|i| {
                    if i == 0 {
                        suggested.clone()
                    } else {
                        format!("{suggested}_{i}")
                    }
                })
                .find(|n| self.doc.parameters().find(n).is_none())
                .expect("a free name");
            let comment = format!("offset distance, {expression}");
            let text = match self
                .doc
                .add_parameter_expression(&name, &expression, None, &comment)
            {
                Ok(()) => {
                    made.push(name.clone());
                    name
                }
                // Each value its own expression.
                Err(_) => expression,
            };
            for v in parts
                .dimensions
                .iter_mut()
                .chain(parts.offsets.iter_mut())
                .chain(parts.patterns.iter_mut())
            {
                replace_text(v, &placeholder, &text);
            }
        }
        made
    }

    /// The plane of a sketch as the definition's `plane`, with that plane's
    /// frame: an origin plane, an imported construction plane, a planar
    /// face of the replay, or a fixed construction plane made for it. A
    /// copy in another component's coordinates takes planes of that
    /// component only: one of its faces, else a fixed plane (its origin
    /// planes when it is placed as the sketch's own component). So does a
    /// sketch whose component the decoder gives (`owned`, mitcad#37): the
    /// faces of other components are in their own coordinates.
    /// The curves of the edges of the face a sketch lies on (`plane`, a
    /// face of a body of the item's component), in the sketch's
    /// coordinates (`frame`): lines, and circles in the sketch's plane.
    fn face_carriers(&self, plane: &Value, frame: &SketchFrame) -> Vec<sketch::edges::Carrier> {
        use sketch::edges::Carrier;
        let (Some(face), Some(body)) = (
            plane["face"]
                .as_str()
                .and_then(|f| f.parse::<FaceName>().ok()),
            plane["body"]
                .as_str()
                .and_then(|b| b.parse::<BodyUid>().ok()),
        ) else {
            return Vec::new();
        };
        if self.doc.body_component(body) != Some(self.component) {
            return Vec::new();
        }
        let Some(shape) = crate::body_scope::shape(self.doc, body) else {
            return Vec::new();
        };
        let Ok(curves) = self
            .doc
            .kernel()
            .curves_of(shape, &mitcad_model::TopoName::Face(face))
        else {
            return Vec::new();
        };
        let normal = geom::normal(frame);
        let to2 = |p: [f64; 3]| {
            let d = geom::sub(p, frame.origin);
            [geom::dot(d, frame.x_axis), geom::dot(d, frame.y_axis)]
        };
        let mut out = Vec::new();
        for c in curves {
            match c {
                mitcad_model::Curve3::Line { start, end } => {
                    let (a, b) = (to2(start), to2(end));
                    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
                    if length > 1e-9 {
                        out.push(Carrier::Line {
                            point: a,
                            direction: [(b[0] - a[0]) / length, (b[1] - a[1]) / length],
                        });
                    }
                }
                mitcad_model::Curve3::Conic {
                    center,
                    normal: n,
                    major,
                    minor,
                    ..
                } if (major - minor).abs() <= 1e-12 * major.max(1.0)
                    && geom::dot(n, normal).abs() > 1.0 - 1e-12 =>
                {
                    out.push(Carrier::Circle {
                        center: to2(center),
                        radius: major,
                    });
                }
                _ => {}
            }
        }
        out
    }

    fn sketch_plane(
        &mut self,
        detail: &SketchDetail,
        frame: &SketchFrame,
        name: Option<&str>,
        copy: Option<&Transform>,
        owned: bool,
        points: &[(u32, [f64; 2])],
    ) -> Result<SketchPlaneChoice, String> {
        let moved = copy.is_some_and(|t| !t.is_identity());
        let component = (copy.is_some() || owned).then_some(self.component);
        if let Some(Reference::ConstructionPlane(c)) = &detail.reference_plane
            && !moved
        {
            let origin = c.origin.clone().flatten();
            let plane = match origin.as_deref() {
                Some("XY") => Some((SketchPlane::Xy, "xy")),
                Some("XZ") => Some((SketchPlane::Xz, "xz")),
                Some("YZ") => Some((SketchPlane::Yz, "yz")),
                _ => None,
            };
            if let Some((plane, text)) = plane {
                let f = plane.origin_frame().expect("an origin plane");
                return Ok((json!(text), None, f, Vec::new()));
            }
            if let Some(uid) = c
                .timeline_index
                .flatten()
                .and_then(|i| self.features.get(&i))
                .copied()
                && let Some(Datum::Plane(p)) = self.doc.datum(uid)
                && component.is_none_or(|c| self.doc.feature(uid).map(|f| f.component) == Some(c))
            {
                return Ok((json!(uid.to_string()), None, p.frame(), Vec::new()));
            }
        }
        // A planar face of the replay in the sketch's plane.
        let normal = geom::normal(frame);
        let mut faces: Vec<_> = refs::planar_faces(self.doc)
            .into_iter()
            .filter(|(b, _, p)| {
                geom::coplanar(frame, p.origin, p.normal, 1e-4)
                    && component.is_none_or(|c| self.doc.body_component(*b) == Some(c))
            })
            .collect();
        // A sketch on a face looks along the face's outward normal.
        faces.sort_by_key(|(_, _, p)| geom::dot(p.normal, normal) < 0.0);
        if let Some((body, face, p)) = faces.first() {
            let f = SketchPlane::face_frame(p.origin, p.normal);
            return Ok((
                json!({"face": face.to_string(), "body": body.to_string()}),
                None,
                f,
                Vec::new(),
            ));
        }
        // A planar face of another component, through its occurrence
        // (mitcad#225).
        if owned
            && copy.is_none()
            && let Some((plane, link, f, note)) = self.linked_face_plane(frame, points)
        {
            return Ok((plane, Some(link), f, vec![note]));
        }
        // A fixed construction plane in the sketch's place.
        let def = json!({"type": "construction_plane", "definition": {"type": "fixed",
            "origin": frame.origin, "x_axis": frame.x_axis, "y_axis": frame.y_axis}});
        let plane_name = name.map(|n| format!("{n} plane"));
        let uid = self.add(&def, plane_name.as_deref())?;
        let f = match self.doc.datum(uid) {
            Some(Datum::Plane(p)) => p.frame(),
            _ => *frame,
        };
        Ok((
            json!(uid.to_string()),
            None,
            f,
            vec![format!("on a fixed plane {uid} (its face was not found)")],
        ))
    }

    // Construction geometry.

    /// Construction geometry: parametric where the dump tells what it is
    /// measured from, else fixed in place (notes starting "a fixed").
    pub(crate) fn translate_datum(
        &mut self,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let detail = match &item.detail {
            Some(Detail::ConstructionPlane(d)) => d,
            Some(Detail::ConstructionPoint(p)) => return self.point_datum(item, p),
            Some(Detail::ConstructionAxis(a)) => return self.axis_datum(item, a),
            _ => {
                return self.fixed_datum(item).map(|c| vec![c]);
            }
        };
        let geometry = detail.geometry.clone().unwrap_or_default();
        let (Some(origin), Some(normal)) = (geometry.origin, geometry.normal) else {
            return Err("the plane's geometry was not decoded".to_owned());
        };
        let origin = mm3(origin);
        let normal = geom::unit(normal).ok_or("a plane without a normal")?;
        let mut candidates = Vec::new();
        if let Some(def) = &detail.definition
            && def.definition_type.as_deref() == Some("ConstructionPlaneOffsetDefinition")
            && let Some(offset) = self.value(&def.offset)
        {
            let d = param(&def.offset)
                .and_then(|p| p.value)
                .map(mm)
                .unwrap_or(0.0);
            for base in self.offset_bases(def, origin, normal, d) {
                candidates.push(Candidate::new(json!({"type": "construction_plane",
                    "definition": {"type": "offset", "plane": base, "distance": offset}})));
            }
        }
        // Mid planes and planes at an angle (the `.ipt` import's): each
        // pairing of the planes and lines they may be measured from, an
        // angle either way round; `datum_item` keeps the one in the
        // plane's place.
        let entity = |key: &str| {
            detail
                .definition
                .as_ref()
                .and_then(|d| d.other.get(key))
                .and_then(Value::as_object)
                .map(|m| Reference::from_map(m.clone()))
        };
        match detail
            .definition
            .as_ref()
            .and_then(|d| d.definition_type.as_deref())
        {
            Some("ConstructionPlaneMidplaneDefinition") => {
                let one = self.plane_options(entity("planarEntityOne").as_ref());
                let two = self.plane_options(entity("planarEntityTwo").as_ref());
                for a in &one {
                    for b in &two {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "midplane", "plane1": a, "plane2": b}})));
                    }
                }
            }
            Some("ConstructionPlaneAtAngleDefinition") => {
                let def = detail.definition.as_ref().expect("matched");
                let mut planes = self.plane_options(def.planar_entity.as_ref());
                // Without a plane in the file's inputs (mitcad#178): the
                // origin planes, each checked against the file's plane.
                if def.planar_entity.is_none() {
                    planes.extend(["xy", "xz", "yz"].map(|p| json!(p)));
                }
                let linear = entity("linearEntity");
                // A cylindrical or conical face as its line: a plane
                // tangent to it, turned about its axis (mitcad#178).
                let tangent = match &linear {
                    Some(Reference::Face(fp)) => fp
                        .geometry
                        .as_ref()
                        .and_then(|g| g.geometry_type.as_deref())
                        .is_some_and(|t| matches!(t, "Cylinder" | "Cone")),
                    _ => false,
                };
                let lines = self.axis_options(linear.as_ref());
                if let Some(angle) = self.value(&def.angle) {
                    let negated = match &angle {
                        Value::String(s) => json!(format!("-({s})")),
                        v => json!(-v.as_f64().unwrap_or(0.0)),
                    };
                    let mut angles = vec![angle.clone(), negated.clone()];
                    // A tangent plane touches the face on either side.
                    if tangent {
                        for a in [&angle, &negated] {
                            angles.push(match a {
                                Value::String(s) => json!(format!("({s}) + 180 deg")),
                                v => json!(v.as_f64().unwrap_or(0.0) + std::f64::consts::PI),
                            });
                        }
                    }
                    for line in &lines {
                        for plane in &planes {
                            for a in &angles {
                                let d = if tangent {
                                    json!({"type": "tangent", "face": line, "angle": a,
                                           "plane": plane})
                                } else {
                                    json!({"type": "angle", "line": line, "angle": a,
                                           "plane": plane})
                                };
                                candidates.push(Candidate::new(
                                    json!({"type": "construction_plane", "definition": d}),
                                ));
                            }
                        }
                    }
                }
            }
            // Normal to a path at a fraction of its length from the start
            // of its first edge (mitcad#178): the edges in the file's order
            // and the other way round.
            Some("ConstructionPlaneDistanceOnPathDefinition") => {
                let def = detail.definition.as_ref().expect("matched");
                let entities: Vec<Reference> = def
                    .other
                    .get("pathEntities")
                    .and_then(Value::as_array)
                    .map(|v| {
                        v.iter()
                            .filter_map(|e| e.as_object().map(|m| Reference::from_map(m.clone())))
                            .collect()
                    })
                    .unwrap_or_default();
                // A length along the path where the parameter has a unit,
                // else a fraction of it.
                let physical = param(&def.distance)
                    .and_then(|p| p.unit.clone())
                    .is_some_and(|u| !u.is_empty());
                let mut paths = Vec::new();
                if let [e @ Reference::SketchEntity(_)] = &entities[..]
                    && let Some(curve) = self.pattern_axis(e)
                {
                    paths.push((curve, false));
                }
                // Several curves of one sketch, joined end to end: in the
                // file's order, which the path starts from.
                if entities.len() > 1
                    && let Some(path) = self.sketch_curves_path(&entities)
                {
                    paths.push((path, false));
                }
                let edges: Option<Vec<(String, String)>> = entities
                    .iter()
                    .map(|e| match e {
                        Reference::Edge(fp) => refs::resolve_edge(self.doc, fp)
                            .map(|(b, e)| (b.to_string(), e.to_string())),
                        _ => None,
                    })
                    .collect();
                if let Some(edges) = edges
                    && let Some(body) = edges.first().map(|e| e.0.clone())
                    && edges.iter().all(|e| e.0 == body)
                {
                    let names: Vec<&String> = edges.iter().map(|e| &e.1).collect();
                    let reversed: Vec<&String> = names.iter().rev().copied().collect();
                    paths.push((json!({"body": body, "edges": names}), false));
                    paths.push((json!({"body": body, "edges": reversed}), true));
                }
                if let Some(f) = self.value(&def.distance) {
                    // From the other end too (the path's direction is
                    // MitCad's own); kept only where the plane is the file's.
                    let other = match (&f, physical) {
                        (_, true) => None,
                        (Value::String(s), false) => Some(json!(format!("1 - ({s})"))),
                        (v, false) => Some(json!(1.0 - v.as_f64().unwrap_or(0.0))),
                    };
                    for (path, reversed) in &paths {
                        let ds: Vec<&Value> = if *reversed {
                            other.iter().collect()
                        } else {
                            std::iter::once(&f).chain(other.iter()).collect()
                        };
                        for d in ds {
                            candidates.push(Candidate::new(json!({"type": "construction_plane",
                                "definition": {"type": "along_path", "path": path,
                                               "distance": d, "physical": physical}})));
                        }
                    }
                }
            }
            // Planes through points and lines (the `.ipt` import's): each
            // pairing of what they may be, the lines either way round.
            Some("ConstructionPlaneThreePointsDefinition") => {
                let p = |k: &str| self.point_options(entity(k).as_ref());
                let (a, b, c) = (
                    p("pointEntityOne"),
                    p("pointEntityTwo"),
                    p("pointEntityThree"),
                );
                if crate::tracing() && (a.is_empty() || b.is_empty() || c.is_empty()) {
                    eprintln!(
                        "import: a plane through three points: {} {} {} candidate points of the \
                         three entities",
                        a.len(),
                        b.len(),
                        c.len()
                    );
                }
                for x in &a {
                    for y in &b {
                        for z in &c {
                            candidates.push(Candidate::new(json!({"type": "construction_plane",
                                "definition": {"type": "three_points", "point1": x,
                                               "point2": y, "point3": z}})));
                        }
                    }
                }
            }
            Some("ConstructionPlaneTwoEdgesDefinition") => {
                let one = self.axis_options(entity("linearEntityOne").as_ref());
                let two = self.axis_options(entity("linearEntityTwo").as_ref());
                for (a, b) in one.iter().flat_map(|a| two.iter().map(move |b| (a, b))) {
                    for (l1, l2) in [(a, b), (b, a)] {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "two_edges", "line1": l1, "line2": l2}})));
                    }
                }
            }
            Some("ConstructionPlaneLineAndPointDefinition") => {
                let lines = self.axis_options(entity("linearEntity").as_ref());
                let points = self.point_options(entity("pointEntity").as_ref());
                for line in &lines {
                    for point in &points {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "edge_and_point", "line": line,
                                           "point": point}})));
                    }
                }
            }
            Some("ConstructionPlaneNormalToLineDefinition") => {
                // Normal to the line where it is nearest to the point: the
                // line as a straight path about that place.
                let line = match entity("linearEntity") {
                    Some(Reference::ConstructionAxis(c)) => {
                        match c.origin.clone().flatten().as_deref() {
                            Some("X") => Some(([0.0; 3], [1.0, 0.0, 0.0])),
                            Some("Y") => Some(([0.0; 3], [0.0, 1.0, 0.0])),
                            Some("Z") => Some(([0.0; 3], [0.0, 0.0, 1.0])),
                            _ => c
                                .geometry
                                .as_ref()
                                .and_then(|g| Some((mm3(g.origin?), geom::unit(g.direction?)?))),
                        }
                    }
                    _ => None,
                };
                if let Some((o, v)) = line {
                    let at = geom::add(o, geom::scale(v, geom::dot(geom::sub(origin, o), v)));
                    let path = json!({"start": geom::sub(at, geom::scale(v, 100.0)),
                                      "end": geom::add(at, geom::scale(v, 100.0))});
                    for point in self.point_options(entity("pointEntity").as_ref()) {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "normal_at_point", "path": path,
                                           "point": point}})));
                    }
                }
            }
            // The plane tangent to a face at a vertex or point (mitcad#186).
            Some("ConstructionPlaneTangentAtPointDefinition") => {
                let faces = self.axis_options(entity("tangentFace").as_ref());
                let points = self.point_options(entity("pointEntity").as_ref());
                for face in &faces {
                    for point in &points {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "tangent_at_point", "face": face,
                                           "point": point}})));
                    }
                }
            }
            // Parallel to a plane, through a vertex or point (mitcad#186).
            Some("ConstructionPlaneOffsetThroughPointDefinition") => {
                let def = detail.definition.as_ref().expect("matched");
                let planes = self.plane_options(def.planar_entity.as_ref());
                let points = self.point_options(entity("point").as_ref());
                for plane in &planes {
                    for point in &points {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "offset_through_point", "plane": plane,
                                           "point": point}})));
                    }
                }
            }
            // A tangent plane with the plane its angle is measured from
            // stored (mitcad#186); the angle as stored first, then the
            // other ways round a cylinder it can touch.
            Some("ConstructionPlaneTangentDefinition") => {
                let def = detail.definition.as_ref().expect("matched");
                let faces = self.axis_options(entity("tangentFace").as_ref());
                let mut planes = self.plane_options(def.planar_entity.as_ref());
                // Without a plane in the file's inputs: the origin planes,
                // each checked against the file's plane.
                if def.planar_entity.is_none() {
                    planes.extend(["xy", "xz", "yz"].map(|p| json!(p)));
                }
                if let Some(angle) = self.value(&def.angle) {
                    let negated = match &angle {
                        Value::String(s) => json!(format!("-({s})")),
                        v => json!(-v.as_f64().unwrap_or(0.0)),
                    };
                    let mut angles = vec![angle.clone(), negated.clone()];
                    for a in [&angle, &negated] {
                        angles.push(match a {
                            Value::String(s) => json!(format!("({s}) + 180 deg")),
                            v => json!(v.as_f64().unwrap_or(0.0) + std::f64::consts::PI),
                        });
                    }
                    for face in &faces {
                        for plane in &planes {
                            for a in &angles {
                                candidates.push(Candidate::new(
                                    json!({"type": "construction_plane",
                                    "definition": {"type": "tangent", "face": face, "angle": a,
                                                   "plane": plane}}),
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        // What was read (mitcad#186): the first reading of a definition
        // whose entities the file names; the others are guesses.
        let direct = detail.definition.as_ref().is_some_and(direct_entities);
        for (k, c) in candidates.iter_mut().enumerate() {
            c.input_provenance = if direct && k == 0 {
                InputProvenance::Read
            } else {
                InputProvenance::Inferred
            };
        }
        let x = geometry
            .u_direction
            .and_then(geom::unit)
            .unwrap_or_else(|| default_x(normal));
        let y = geom::cross(normal, x);
        candidates.push(
            Candidate::new(
                json!({"type": "construction_plane", "definition": {"type": "fixed",
                "origin": origin, "x_axis": x, "y_axis": y}}),
            )
            .with_note("a fixed plane"),
        );
        Ok(candidates)
    }

    /// Curves of one imported sketch as a path (`{"sketch", "curves"}`),
    /// in the order given; None when one is not a curve of an imported
    /// sketch or they are of several sketches.
    fn sketch_curves_path(&self, entities: &[Reference]) -> Option<Value> {
        let mut sketch = None;
        let mut curves = Vec::new();
        for e in entities {
            let Reference::SketchEntity(e) = e else {
                return None;
            };
            let s = self.sketches.get(&e.sketch_timeline_index.flatten()?)?;
            if sketch.is_some_and(|u| u != s.uid) {
                return None;
            }
            sketch = Some(s.uid);
            curves.push(s.ids.get(&e.id.clone().flatten()?)?.clone());
        }
        Some(json!({"sketch": sketch?.to_string(), "curves": curves}))
    }

    /// A plane reference as the planes it may be: an origin plane, an
    /// imported construction plane, a face it names, or by its geometry
    /// the origin planes and planar faces in it, else that plane fixed.
    pub fn plane_options(&self, r: Option<&Reference>) -> Vec<Value> {
        let mut out = Vec::new();
        let Some(r) = r else {
            return out;
        };
        if let Reference::ConstructionPlane(c) = r
            && c.origin.clone().flatten().is_none()
            && c.timeline_index.flatten().is_none()
            && let Some(g) = &c.geometry
            && let (Some(o), Some(n)) = (g.origin, g.normal.and_then(geom::unit))
        {
            let o = mm3(o);
            let on = |p: [f64; 3], m: [f64; 3]| {
                geom::norm(geom::cross(n, m)) < 1e-9 && geom::dot(geom::sub(p, o), n).abs() < 1e-4
            };
            for (text, axis) in [("xy", 2), ("xz", 1), ("yz", 0)] {
                let mut m = [0.0; 3];
                m[axis] = 1.0;
                if on([0.0; 3], m) {
                    out.push(json!(text));
                }
            }
            for (body, face, p) in refs::planar_faces(self.doc) {
                if on(p.origin, p.normal) {
                    out.push(json!({"body": body.to_string(), "face": face.to_string()}));
                }
            }
        }
        out.extend(self.plane_ref(r));
        out
    }

    /// The edge, sketch line or face of a datum's definition (mitcad#186):
    /// [`Self::axis_options`], with a closed edge found on its circle where
    /// the replay's seam is elsewhere ([`refs::resolve_datum_edge`]).
    fn entity_options(&self, r: Option<&Reference>) -> Vec<Value> {
        match r {
            Some(Reference::Edge(fp)) => refs::resolve_datum_edge(self.doc, fp)
                .map(|(body, edge)| json!({"body": body.to_string(), "edge": edge.to_string()}))
                .into_iter()
                .collect(),
            r => self.axis_options(r),
        }
    }

    /// A line reference as the axes it may be: an origin axis, an imported
    /// construction axis, by its geometry an origin axis on it, else that
    /// axis fixed.
    fn axis_options(&self, r: Option<&Reference>) -> Vec<Value> {
        let mut out = Vec::new();
        let Some(Reference::ConstructionAxis(c)) = r else {
            // An edge, a sketch line, or a cylindrical face's axis
            // (mitcad#178).
            return r.and_then(|r| self.pattern_axis(r)).into_iter().collect();
        };
        match c.origin.clone().flatten().as_deref() {
            Some(o @ ("X" | "Y" | "Z")) => return vec![json!(o.to_lowercase())],
            _ => {
                if let Some(uid) = c
                    .timeline_index
                    .flatten()
                    .and_then(|i| self.features.get(&i))
                {
                    return vec![json!(uid.to_string())];
                }
            }
        }
        let Some(g) = &c.geometry else {
            return out;
        };
        let (Some(o), Some(v)) = (g.origin, g.direction.and_then(geom::unit)) else {
            return out;
        };
        let o = mm3(o);
        let on = |p: [f64; 3]| {
            let d = geom::sub(p, o);
            geom::norm(geom::sub(d, geom::scale(v, geom::dot(d, v)))) < 1e-4
        };
        for (text, axis) in [("x", 0), ("y", 1), ("z", 2)] {
            let mut m = [0.0; 3];
            m[axis] = 1.0;
            if on([0.0; 3]) && geom::norm(geom::cross(v, m)) < 1e-9 {
                out.push(json!(text));
            }
        }
        out.push(json!({"origin": o, "direction": v}));
        out
    }

    /// A point reference as the points it may be: the origin, an imported
    /// construction point, else that point fixed.
    fn point_options(&self, r: Option<&Reference>) -> Vec<Value> {
        // A vertex (mitcad#186), named by the faces that meet at it.
        if let Some(Reference::Vertex(fp)) = r {
            let found = refs::resolve_vertex(self.doc, fp);
            if found.is_none() && crate::tracing() {
                eprintln!(
                    "import: a vertex input at {:?} mm is not in the replay",
                    fp.point.map(geom::mm3)
                );
                for body in crate::body_scope::bodies(self.doc) {
                    eprintln!(
                        "import:   body {} box {:?}",
                        body.uid,
                        self.doc.kernel().bounding_box(body.shape).ok().flatten()
                    );
                    // The edges with an end near it.
                    let target = fp.point.map(geom::mm3);
                    for (name, curve) in self
                        .doc
                        .kernel()
                        .edge_geometries(body.shape)
                        .unwrap_or_default()
                    {
                        let (a, b) = match &curve {
                            mitcad_model::datum::CurveGeometry::Line { start, end }
                            | mitcad_model::datum::CurveGeometry::Circle { start, end, .. }
                            | mitcad_model::datum::CurveGeometry::Other { start, end, .. } => {
                                (*start, *end)
                            }
                        };
                        if let Some(t) = target
                            && geom::distance(t, a).min(geom::distance(t, b)) < 2.0
                        {
                            eprintln!("import:     edge {name:?} {} {a:?} -> {b:?}", curve.kind());
                        }
                    }
                }
            }
            return found
                .map(|(body, vertex)| json!({"body": body.to_string(), "vertex": vertex.to_string()}))
                .into_iter()
                .collect();
        }
        // A sketch point (mitcad#186).
        if let Some(Reference::SketchEntity(e)) = r {
            return (|| {
                if e.object_type.as_deref() != Some("SketchPoint") {
                    return None;
                }
                let s = self.sketches.get(&e.sketch_timeline_index.flatten()?)?;
                let point = s.ids.get(&e.id.clone().flatten()?)?;
                Some(json!({"sketch": s.uid.to_string(), "point": point}))
            })()
            .into_iter()
            .collect();
        }
        let Some(Reference::ConstructionPoint(c)) = r else {
            return Vec::new();
        };
        if c.origin.clone().flatten().is_some() {
            return vec![json!("origin")];
        }
        if let Some(uid) = c
            .timeline_index
            .flatten()
            .and_then(|i| self.features.get(&i))
        {
            return vec![json!(uid.to_string())];
        }
        c.geometry
            .as_ref()
            .and_then(|g| g.origin)
            .map(|p| vec![json!({"point": mm3(p)})])
            .unwrap_or_default()
    }

    /// The planes an offset plane may be measured from: its planar entity
    /// when the dump has it, else an origin plane or a planar face at
    /// `distance` behind it.
    fn offset_bases(
        &self,
        def: &Definition,
        origin: [f64; 3],
        normal: [f64; 3],
        distance: f64,
    ) -> Vec<Value> {
        let mut out = Vec::new();
        match &def.planar_entity {
            Some(Reference::ConstructionPlane(c)) => {
                match c.origin.clone().flatten().as_deref() {
                    Some(o @ ("XY" | "XZ" | "YZ")) => out.push(json!(o.to_lowercase())),
                    _ => {
                        if let Some(uid) = c
                            .timeline_index
                            .flatten()
                            .and_then(|i| self.features.get(&i))
                        {
                            out.push(json!(uid.to_string()));
                        } else if let Some(g) = &c.geometry
                            && let (Some(o), Some(n)) = (g.origin, g.normal.and_then(geom::unit))
                        {
                            // A plane off the timeline by its geometry (an
                            // `.ipt` part's work plane on a face): an
                            // origin plane or a planar face in it.
                            // With the offset along its normal giving the
                            // plane's place.
                            let o = mm3(o);
                            let close = |a: f64, b: f64| (a - b).abs() < 1e-6 * (1.0 + a.abs());
                            let on = |p: [f64; 3], m: [f64; 3]| {
                                geom::norm(geom::cross(n, m)) < 1e-9
                                    && geom::dot(geom::sub(p, o), n).abs() < 1e-4
                                    && close(geom::dot(geom::sub(origin, p), m), distance)
                            };
                            for (text, axis) in [("xy", 2), ("xz", 1), ("yz", 0)] {
                                let mut m = [0.0; 3];
                                m[axis] = 1.0;
                                if on([0.0; 3], m) {
                                    out.push(json!(text));
                                }
                            }
                            for (body, face, p) in refs::planar_faces(self.doc) {
                                if on(p.origin, p.normal) {
                                    out.push(json!({"body": body.to_string(),
                                                    "face": face.to_string()}));
                                }
                            }
                        }
                    }
                }
                return out;
            }
            Some(Reference::Face(fp)) => {
                if let Some((body, face)) = refs::resolve_face(self.doc, fp) {
                    out.push(json!({"body": body.to_string(), "face": face.to_string()}));
                }
                // Then the guesses below (mitcad#178): the face the
                // decoder names may have no geometry, or not be where the
                // replay has it; the plane the file stores tells.
            }
            _ => {}
        }
        // The base is `distance` back along its own normal.
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6 * (1.0 + a.abs());
        for (text, axis) in [("xy", 2), ("xz", 1), ("yz", 0)] {
            let mut n = [0.0; 3];
            n[axis] = 1.0;
            if geom::norm(geom::cross(normal, n)) < 1e-9 && close(origin[axis], distance) {
                out.push(json!(text));
            }
        }
        for (body, face, p) in refs::planar_faces(self.doc) {
            if geom::norm(geom::cross(normal, p.normal)) < 1e-9
                && close(geom::dot(geom::sub(origin, p.origin), p.normal), distance)
            {
                out.push(json!({"body": body.to_string(), "face": face.to_string()}));
            }
        }
        out
    }

    /// A construction point (mitcad#186): the point at the vertex or point,
    /// at the centre of the circle, or where the planes meet, as its
    /// definition says; else fixed where it is.
    fn point_datum(
        &mut self,
        item: &TimelineItem,
        detail: &mitcad_dump::ConstructionPointDetail,
    ) -> Result<Vec<Candidate>, String> {
        let mut candidates = Vec::new();
        if let Some(def) = &detail.definition {
            let entity = |key: &str| {
                def.other
                    .get(key)
                    .and_then(Value::as_object)
                    .map(|m| Reference::from_map(m.clone()))
            };
            let mut definitions: Vec<Value> = Vec::new();
            match def.definition_type.as_deref() {
                Some("ConstructionPointPointDefinition") => {
                    for point in self.point_options(entity("pointEntity").as_ref()) {
                        definitions.push(json!({"type": "point", "point": point}));
                    }
                }
                Some("ConstructionPointCenterDefinition") => {
                    // A circular edge or a spherical or toroidal face.
                    for e in self.entity_options(entity("circularEntity").as_ref()) {
                        definitions.push(json!({"type": "center", "entity": e}));
                    }
                }
                Some("ConstructionPointThreePlanesDefinition") => {
                    let one = self.plane_options(entity("planeOne").as_ref());
                    let two = self.plane_options(entity("planeTwo").as_ref());
                    let three = self.plane_options(entity("planeThree").as_ref());
                    for a in &one {
                        for b in &two {
                            for c in &three {
                                definitions.push(json!({"type": "three_planes", "plane1": a,
                                                        "plane2": b, "plane3": c}));
                            }
                        }
                    }
                }
                _ => {}
            }
            let direct = direct_entities(def);
            for (k, d) in definitions.into_iter().enumerate() {
                let mut c = Candidate::new(json!({"type": "construction_point", "definition": d}));
                c.input_provenance = if direct && k == 0 {
                    InputProvenance::Read
                } else {
                    InputProvenance::Inferred
                };
                candidates.push(c);
            }
        }
        if let Ok(fixed) = self.fixed_datum(item) {
            candidates.push(fixed);
        }
        if candidates.is_empty() {
            return Err("no geometry for the datum".to_owned());
        }
        Ok(candidates)
    }

    /// A construction axis (mitcad#186): through two points, along an edge
    /// or sketch line, through a round face, or where two planes meet, as
    /// its definition says, the direction as MitCad has it and then
    /// reversed (the one in the file's direction is kept); else fixed.
    fn axis_datum(
        &mut self,
        item: &TimelineItem,
        detail: &mitcad_dump::ConstructionAxisDetail,
    ) -> Result<Vec<Candidate>, String> {
        let mut candidates = Vec::new();
        if let Some(def) = &detail.definition {
            let entity = |key: &str| {
                def.other
                    .get(key)
                    .and_then(Value::as_object)
                    .map(|m| Reference::from_map(m.clone()))
            };
            let mut definitions: Vec<Value> = Vec::new();
            match def.definition_type.as_deref() {
                Some("ConstructionAxisEdgeDefinition") => {
                    for e in self.entity_options(entity("edgeEntity").as_ref()) {
                        definitions.push(json!({"type": "edge", "edge": e}));
                    }
                }
                Some("ConstructionAxisCircularFaceDefinition") => {
                    for f in self.entity_options(entity("circularFace").as_ref()) {
                        definitions.push(json!({"type": "circular_face", "face": f}));
                    }
                }
                // Normal to a face at a point (mitcad#186): through the point.
                Some("ConstructionAxisNormalToFaceAtPointDefinition") => {
                    // A face, or a plane: a profile is the plane of its
                    // sketch.
                    let face = entity("face");
                    let faces = match &face {
                        Some(Reference::Profile(_) | Reference::ConstructionPlane(_)) => {
                            self.plane_options(face.as_ref())
                        }
                        _ => self.entity_options(face.as_ref()),
                    };
                    let points = self.point_options(entity("pointEntity").as_ref());
                    for f in &faces {
                        for p in &points {
                            definitions.push(json!({"type": "normal_to_face_at_point",
                                                    "face": f, "point": p}));
                        }
                    }
                }
                // Perpendicular to a face where the axis' own origin is
                // (the file stores that point, no other input).
                Some("ConstructionAxisPerpendicularAtPointDefinition") => {
                    if let Some(at) = def
                        .other
                        .get("point")
                        .and_then(|v| serde_json::from_value::<[f64; 3]>(v.clone()).ok())
                    {
                        for f in self.entity_options(entity("face").as_ref()) {
                            definitions.push(json!({"type": "perpendicular_at_point",
                                "face": f, "point": {"point": mm3(at)}}));
                        }
                    }
                }
                Some("ConstructionAxisTwoPointDefinition") => {
                    let one = self.point_options(entity("pointEntityOne").as_ref());
                    let two = self.point_options(entity("pointEntityTwo").as_ref());
                    for a in &one {
                        for b in &two {
                            definitions.push(json!({"type": "two_points", "point1": a,
                                                    "point2": b}));
                        }
                    }
                }
                Some("ConstructionAxisTwoPlaneDefinition") => {
                    let one = self.plane_options(entity("planarEntityOne").as_ref());
                    let two = self.plane_options(entity("planarEntityTwo").as_ref());
                    for a in &one {
                        for b in &two {
                            definitions.push(json!({"type": "two_planes", "plane1": a,
                                                    "plane2": b}));
                        }
                    }
                }
                _ => {}
            }
            let direct = direct_entities(def);
            for (k, d) in definitions.into_iter().enumerate() {
                for flip in [false, true] {
                    let mut c = Candidate::new(json!({"type": "construction_axis",
                        "definition": d, "flip": flip}));
                    c.input_provenance = if direct && k == 0 && !flip {
                        InputProvenance::Read
                    } else {
                        InputProvenance::Inferred
                    };
                    candidates.push(c);
                }
            }
        }
        if let Ok(fixed) = self.fixed_datum(item) {
            candidates.push(fixed);
        }
        if candidates.is_empty() {
            return Err("no geometry for the datum".to_owned());
        }
        Ok(candidates)
    }

    fn fixed_datum(&self, item: &TimelineItem) -> Result<Candidate, String> {
        // The dump's geometry: a point's three numbers, an axis' line.
        let g = match &item.detail {
            Some(Detail::ConstructionPoint(p)) => json!(p.geometry.ok_or("no point geometry")?),
            Some(Detail::ConstructionAxis(a)) => {
                let g = a.geometry.as_ref().ok_or("no axis geometry")?;
                json!({"origin": g.origin, "direction": g.direction})
            }
            Some(Detail::Other(map)) => map
                .get("geometry")
                .cloned()
                .ok_or("no geometry for the datum")?,
            _ => return Err("no geometry for the datum".to_owned()),
        };
        let g = &g;
        let v =
            |key: &str| -> Option<[f64; 3]> { serde_json::from_value(g.get(key)?.clone()).ok() };
        match item.object_type() {
            Some("ConstructionAxis") => {
                let origin = v("origin").ok_or("no axis origin")?;
                let direction = v("direction")
                    .and_then(geom::unit)
                    .ok_or("no axis direction")?;
                Ok(
                    Candidate::new(json!({"type": "construction_axis", "definition":
                    {"type": "fixed", "origin": mm3(origin), "direction": direction}}))
                    .with_note("a fixed axis"),
                )
            }
            Some("ConstructionPoint") => {
                let p: [f64; 3] = serde_json::from_value(g.clone()).map_err(|e| e.to_string())?;
                Ok(
                    Candidate::new(json!({"type": "construction_point", "definition":
                    {"type": "fixed", "point": mm3(p)}}))
                    .with_note("a fixed point"),
                )
            }
            _ => Err("not a datum".to_owned()),
        }
    }

    // Features.

    pub(crate) fn translate(
        &mut self,
        index: i64,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let translated = self.translate_item(index, item);
        // The learning dump keeps them (mitcad#96), outside the item's time.
        self.learn_offered_paused(index, item, &translated);
        translated
    }

    fn translate_item(
        &mut self,
        index: i64,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let Some(object_type) = item.object_type() else {
            return Err("unknown timeline item (type not decoded)".to_owned());
        };
        // Sheet-metal items the decoder read (mitcad#211).
        if let Some(Detail::Other(map)) = &item.detail
            && let Some(sheet) = map.get("sheet").and_then(Value::as_object)
        {
            let families = self.families;
            return families.sheet_item(self, index, item, sheet);
        }
        if let Some(Detail::Other(map)) = &item.detail
            && let Some(e) = map.get("_sheet_error").and_then(Value::as_str)
        {
            return Err(format!("a sheet-metal item not decoded: {e}"));
        }
        if let Some(reason) = not_translated(object_type) {
            return Err(reason.to_owned());
        }
        match (&item.detail, object_type) {
            (Some(Detail::Extrude(d)), _) => self.extrude(index, item, d),
            (Some(Detail::Revolve(d)), _) => self.revolve(index, d),
            (Some(Detail::Fillet(d)), _) => {
                let c = self.fillet(d)?;
                Ok(self.families.dressups_on_sheets(self, c, true))
            }
            (Some(Detail::Chamfer(d)), _) => {
                let c = self.chamfer(d)?;
                Ok(self.families.dressups_on_sheets(self, c, false))
            }
            // Sweeps, pipes and lofts (F3).
            (Some(Detail::Sweep(d)), _) => self.sweep(index, d),
            (Some(Detail::Pipe(d)), _) => self.pipe(index, d),
            (Some(_), "LoftFeature") => self.loft(item),
            // Coils of sketch profiles (the `.ipt` import's).
            (Some(Detail::Coil(d)), _) if d.other.contains_key("profile") => {
                self.coil_helix(index, d)
            }
            // Primitives and coils placed on a plane (mitcad#25).
            (Some(Detail::Primitive(d)), t) => {
                let families = self.families;
                families.primitive(self, index, t, d)
            }
            (Some(Detail::Coil(d)), _) => {
                let families = self.families;
                families.coil_primitive(self, index, d)
            }
            // Emboss (mitcad#458).
            (Some(Detail::Emboss(d)), _) => self.emboss(index, d),
            // Drafts (mitcad#1363).
            (Some(Detail::Draft(d)), _) => self.draft(d),
            (_, "BaseFeature") => Err("a base feature: the file's bodies as they are".to_owned()),
            // Bodies of other documents (mitcad#324).
            (_, "DerivedContext") => self.derive(index, item),
            (
                _,
                t @ ("CombineFeature"
                | "MirrorFeature"
                | "CircularPatternFeature"
                | "RectangularPatternFeature"
                | "PathPatternFeature"
                | "ShellFeature"
                | "OffsetFacesFeature"
                | "MoveFeature"
                | "SplitBodyFeature"
                | "SplitFaceFeature"
                | "HoleFeature"
                | "ThreadFeature"
                | "ReplaceFaceFeature"
                | "RemoveFeature"
                | "DeleteFaceFeature"
                | "CopyPasteBodies"
                | "MoveFaceFeature"
                | "AlignFeature"),
            ) => self.translate_op(t, item),
            (None, t) => Err(format!("{t}: no details decoded")),
            (_, t) => Err(format!("{t} is not supported yet")),
        }
    }

    /// The curves a thin extrusion follows (mitcad#463) and whether they
    /// are exact: the sketch curves of its profile when it names them (an
    /// open path in a dump), else, when its sketch has no regions, the
    /// sketch's curves that are not construction geometry; None otherwise.
    pub(crate) fn wall_curves(
        &self,
        profiles: &[Reference],
        sketch: i64,
    ) -> Option<(Vec<String>, bool)> {
        let info = self.sketches.get(&sketch)?;
        let named: Option<Vec<String>> = profiles
            .iter()
            .map(|p| match path_curve(p) {
                Some((s, id)) if s == sketch => info.ids.get(&id).cloned(),
                _ => None,
            })
            .collect();
        if let Some(named) = named.filter(|n| !n.is_empty()) {
            return Some((named, true));
        }
        self.sketch_wall_curves(sketch).map(|c| (c, false))
    }

    /// A sketch's curves that are not construction geometry, when it has
    /// no regions.
    fn sketch_wall_curves(&self, sketch: i64) -> Option<Vec<String>> {
        let uid = self.sketches.get(&sketch)?.uid;
        if !self.doc.sketch_output(uid)?.region_info.is_empty() {
            return None;
        }
        let mitcad_model::FeatureDef::Sketch(def) = &self.doc.feature(uid)?.def else {
            return None;
        };
        let curves: Vec<String> = def
            .entities
            .iter()
            .filter(|e| !e.construction && !e.is_point())
            .map(|e| e.id.curve_name())
            .collect();
        (!curves.is_empty()).then_some(curves)
    }

    /// The sketch of profile references, or of the last sketch before the
    /// item when the dump does not name it.
    pub fn profile_sketch(
        &self,
        profiles: &[Reference],
        index: i64,
    ) -> Result<(i64, bool), String> {
        let named = profiles.iter().find_map(|p| match p {
            Reference::Profile(p) => p.sketch_timeline_index.flatten(),
            // Sketch curves of an open path (a thin extrusion's, mitcad#463).
            _ => path_curve(p).map(|(sketch, _)| sketch),
        });
        if let Some(i) = named {
            return if self.sketches.contains_key(&i) {
                Ok((i, false))
            } else {
                Err(format!("its sketch (item {i}) was not imported"))
            };
        }
        self.sketches
            .keys()
            .filter(|k| **k < index)
            .max()
            .map(|k| (*k, true))
            .ok_or_else(|| "no sketch before it".to_owned())
    }

    /// Region sets to try: matched by area and centroid when the dump has
    /// them (a region, each of several alike as a guess, or else a union of
    /// regions first), else every set of as many regions as the item used,
    /// largest first, with their total area (mm²).
    pub fn region_sets(
        &mut self,
        profiles: &[Reference],
        sketch: i64,
        depth: Option<f64>,
        target_area: Option<f64>,
    ) -> Result<Vec<RegionSet>, String> {
        self.region_sets_probing(profiles, sketch, depth, target_area, true)
    }

    /// [`Importer::region_sets`], with the regions the history's next states
    /// change probed only when `probe` (the probes cost seconds on large
    /// bodies, mitcad#181: not where the decoded profiles give the regions).
    pub(crate) fn region_sets_probing(
        &mut self,
        profiles: &[Reference],
        sketch: i64,
        depth: Option<f64>,
        target_area: Option<f64>,
        probe: bool,
    ) -> Result<Vec<RegionSet>, String> {
        let info = self.sketches[&sketch].clone();
        let output = self
            .doc
            .sketch_output(info.uid)
            .ok_or("its sketch did not evaluate")?;
        let regions: Vec<Region> = output.region_info.clone();
        let frame = output.frame;
        let regions = &regions[..];
        if regions.is_empty() {
            return Err("its sketch has no profiles".to_owned());
        }
        let key = |r: &Region| r.profile.key.to_string();
        // By area and centroid (external dumps' profiles).
        let measured: Vec<_> = profiles
            .iter()
            .filter_map(|p| match p {
                Reference::Profile(p) => Some((p.area?, p.centroid?)),
                _ => None,
            })
            .collect();
        let mut first: Vec<RegionSet> = Vec::new();
        if !measured.is_empty() && measured.len() == profiles.len() {
            // Each profile's regions of its area and centroid: several
            // when the sketch has regions alike (rings of one area about
            // one centre), and then each choice of regions apart from
            // the other profiles' is a guess. Without one,
            // a union of regions of that area and centroid (a line that
            // bounds another feature's profile splits this one in
            // MitCad's sketch), tried before the sets found without the
            // measures.
            let mut order: Vec<&Region> = regions.iter().collect();
            order.sort_by(|a, b| b.area.total_cmp(&a.area));
            let areas: Vec<f64> = order.iter().map(|r| r.area).collect();
            let mut unions = false;
            let found: Vec<Vec<Vec<&Region>>> = measured
                .iter()
                .map(|&(area, centroid)| {
                    let area = area * 100.0;
                    let c = [mm(centroid[0]), mm(centroid[1])];
                    let tol = 1e-3 * area.sqrt().max(1.0);
                    let fits = |set: &[&Region]| {
                        let a: f64 = set.iter().map(|r| r.area).sum();
                        let m = set.iter().fold([0.0, 0.0], |m, r| {
                            [m[0] + r.centroid[0] * r.area, m[1] + r.centroid[1] * r.area]
                        });
                        a > 0.0
                            && (a - area).abs() <= 1e-3 * area.abs().max(1e-6)
                            && (m[0] / a - c[0]).hypot(m[1] / a - c[1]) <= tol
                    };
                    let single: Vec<Vec<&Region>> = regions
                        .iter()
                        .filter(|r| fits(&[*r]))
                        .map(|r| vec![r])
                        .collect();
                    if !single.is_empty() {
                        return single;
                    }
                    unions = true;
                    // (The search may give a set more than once.)
                    let mut sets = area_subsets(&areas, area, 12);
                    sets.sort();
                    sets.dedup();
                    sets.into_iter()
                        .map(|s| s.into_iter().map(|i| order[i]).collect::<Vec<_>>())
                        .filter(|s| s.len() > 1 && fits(s))
                        .collect()
                })
                .collect();
            if crate::tracing() {
                eprintln!(
                    "import:   measured profiles {measured:?}: {:?} choices{}",
                    found.iter().map(Vec::len).collect::<Vec<_>>(),
                    if unions { " (unions)" } else { "" }
                );
                for options in found
                    .iter()
                    .filter(|f| f.len() > 1 || f.iter().any(|g| g.len() > 1))
                {
                    for g in options.iter().take(4) {
                        let parts: Vec<String> = g
                            .iter()
                            .map(|r| format!("{:.3}@{:.3?}", r.area, r.centroid))
                            .collect();
                        eprintln!("import:     {}", parts.join(" + "));
                    }
                }
            }
            if found.iter().all(|f| !f.is_empty()) {
                let mut chosen: Vec<Vec<&Region>> = vec![Vec::new()];
                for options in &found {
                    let mut next = Vec::new();
                    for c in &chosen {
                        for group in options {
                            let apart =
                                !group.iter().any(|r| c.iter().any(|d| std::ptr::eq(*d, *r)));
                            if next.len() < 32 && apart {
                                let mut c = c.clone();
                                c.extend(group.iter().copied());
                                next.push(c);
                            }
                        }
                    }
                    chosen = next;
                }
                // One way to take the regions apart is no guess.
                let alike = chosen.len() > 1;
                let sets: Vec<RegionSet> = chosen
                    .into_iter()
                    .map(|c| RegionSet {
                        keys: c.iter().map(|r| key(r)).collect(),
                        guess: alike,
                        area: c.iter().map(|r| r.area).sum(),
                        parts: c.iter().map(|r| (r.centroid, r.area)).collect(),
                        probe: None,
                        measured: unions && !alike,
                        exact: false,
                    })
                    .collect();
                // Unions come before the sets found without the measures.
                if unions {
                    first = sets;
                } else if !sets.is_empty() {
                    return Ok(sets);
                }
            }
        }
        let n = profiles.len().max(1);
        // Construction curves are not decoded, so the file's profiles may be
        // unions of MitCad's regions: the regions whose material the next
        // history states change, probed in both directions, come first.
        let mut probed = if probe {
            self.probed_sets(info.uid, frame, regions, depth)
        } else {
            Vec::new()
        };
        let mut order: Vec<&Region> = regions.iter().collect();
        order.sort_by(|a, b| b.area.total_cmp(&a.area));
        // The sets of any number of regions whose area is the one the
        // history's change asks for (the file's profiles may be unions of
        // MitCad's regions where construction lines split them, so the
        // decoded count is no limit).
        if let Some(target) = target_area {
            let areas: Vec<f64> = order.iter().map(|r| r.area).collect();
            for chosen in area_subsets(&areas, target, 6) {
                let set = RegionSet {
                    keys: chosen.iter().map(|&i| key(order[i])).collect(),
                    guess: true,
                    area: chosen.iter().map(|&i| order[i].area).sum(),
                    parts: chosen
                        .iter()
                        .map(|&i| (order[i].centroid, order[i].area))
                        .collect(),
                    probe: None,
                    measured: false,
                    exact: false,
                };
                if !probed.iter().any(|p| same_keys(&p.keys, &set.keys)) {
                    probed.push(set);
                }
            }
        }
        let mut all = RegionSet {
            keys: order.iter().map(|r| key(r)).collect(),
            guess: false,
            area: order.iter().map(|r| r.area).sum(),
            parts: order.iter().map(|r| (r.centroid, r.area)).collect(),
            probe: None,
            measured: false,
            exact: false,
        };
        // (A probe that finds every region keeps what it found.)
        all.probe = probed
            .iter()
            .find(|p| same_keys(&p.keys, &all.keys))
            .and_then(|p| p.probe);
        // (A union a probe also found keeps what the probe found.)
        let with_first = |mut sets: Vec<RegionSet>| {
            let mut first = first.clone();
            for f in &mut first {
                if let Some(s) = sets.iter().find(|s| same_keys(&s.keys, &f.keys)) {
                    f.probe = f.probe.or(s.probe);
                }
            }
            sets.retain(|s| !first.iter().any(|f| same_keys(&f.keys, &s.keys)));
            first.into_iter().chain(sets).collect::<Vec<_>>()
        };
        if n >= regions.len() {
            probed.retain(|p| !same_keys(&p.keys, &all.keys));
            probed.insert(0, all);
            return Ok(with_first(probed));
        }
        let mut sets: Vec<RegionSet> = Vec::new();
        combinations(order.len(), n, 2000, &mut |c| {
            sets.push(RegionSet {
                keys: c.iter().map(|&i| key(order[i])).collect(),
                guess: !sets.is_empty(),
                area: c.iter().map(|&i| order[i].area).sum(),
                parts: c
                    .iter()
                    .map(|&i| (order[i].centroid, order[i].area))
                    .collect(),
                probe: None,
                measured: false,
                exact: false,
            });
        });
        sets.push(RegionSet { guess: true, ..all });
        sets.retain(|s| !probed.iter().any(|p| same_keys(&p.keys, &s.keys)));
        probed.extend(sets);
        Ok(with_first(probed))
    }

    /// The regions the decoded profile loops select (`_f3d_profile_loops`
    /// of the item's detail, mitcad#96), mapped by
    /// [`crate::profiles::mapped_regions`] with the file's geometry of the
    /// sketch's curves.
    pub fn mapped_set(
        &self,
        other: &serde_json::Map<String, Value>,
        sketch: i64,
    ) -> Option<RegionSet> {
        let Some(loops) = other.get("_f3d_profile_loops") else {
            return self.text_set(other.get("_f3d_text_profiles")?, sketch);
        };
        let info = self.sketches.get(&sketch)?;
        let output = self.doc.sketch_output(info.uid)?;
        let regions = &output.region_info;
        let detail = self
            .dump
            .timeline_items()
            .iter()
            .enumerate()
            .find(|(p, it)| it.index.unwrap_or(*p as i64) == sketch)
            .and_then(|(_, it)| match &it.detail {
                Some(Detail::Sketch(s)) => Some(s),
                _ => None,
            })?;
        let curves: HashMap<String, Geometry> = detail
            .curves
            .iter()
            .flatten()
            .filter_map(|c| Some((c.id.clone()?, c.geometry.clone()?)))
            .collect();
        let back: HashMap<&str, &str> = info
            .ids
            .iter()
            .map(|(file, ours)| (ours.as_str(), file.as_str()))
            .collect();
        let views: Vec<crate::profiles::RegionView> = regions
            .iter()
            .zip(interior_points(regions))
            .map(|(r, inside)| crate::profiles::RegionView {
                inside,
                outer: r.profile.loops.first().and_then(|l| {
                    l.segments
                        .iter()
                        .map(|s| {
                            back.get(s.key.curve.to_string().as_str())
                                .map(|f| f.to_string())
                        })
                        .collect()
                }),
            })
            .collect();
        let construction: std::collections::HashSet<String> = detail
            .curves
            .iter()
            .flatten()
            .filter(|c| c.is_construction == Some(true) || c.is_center_line == Some(true))
            .filter_map(|c| c.id.clone())
            .collect();
        let (chosen, exact) = crate::profiles::mapped_with(loops, &curves, &construction, &views)?;
        if crate::tracing() {
            eprintln!(
                "import: profile loops select {} of {} regions, exact={exact}, areas {:?}",
                chosen.len(),
                regions.len(),
                chosen.iter().map(|&i| regions[i].area).collect::<Vec<_>>()
            );
        }
        Some(RegionSet {
            keys: chosen
                .iter()
                .map(|&i| regions[i].profile.key.to_string())
                .collect(),
            guess: false,
            area: chosen.iter().map(|&i| regions[i].area).sum(),
            parts: chosen
                .iter()
                .map(|&i| (regions[i].centroid, regions[i].area))
                .collect(),
            probe: None,
            measured: false,
            exact,
        })
    }

    /// The letters of the texts an extrusion's text profiles name
    /// (`_f3d_text_profiles`: `texts`, the file's text ids; `profiles`, how
    /// many profiles the item selects; `only`, all of them the texts'),
    /// mitcad#1177: the file's text profiles have no loops of sketch
    /// curves, one profile a letter (the drill index's 59 profiles are the
    /// 59 letters of its text). The regions bounded by one glyph contour
    /// that is a letter's outline (not a counter); exact when the item
    /// selects only the texts' profiles and as many as there are letters.
    fn text_set(&self, named: &Value, sketch: i64) -> Option<RegionSet> {
        let info = self.sketches.get(&sketch)?;
        let output = self.doc.sketch_output(info.uid)?;
        let texts: Vec<u32> = named["texts"]
            .as_array()?
            .iter()
            .filter_map(|t| info.ids.get(t.as_str()?))
            .filter_map(|ours| ours.strip_prefix('t')?.parse().ok())
            .collect();
        if texts.is_empty() {
            return None;
        }
        let letters: std::collections::BTreeSet<mitcad_model::topo::CurveId> = output
            .texts
            .iter()
            .filter(|t| texts.contains(&t.id.0))
            .flat_map(mitcad_model::sketch::text::letter_contours)
            .collect();
        let regions = &output.region_info;
        let chosen: Vec<usize> = regions
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                let mut outer = r.profile.key.segments().map(|s| &s.curve);
                matches!((outer.next(), outer.next()), (Some(c), None) if letters.contains(c))
            })
            .map(|(i, _)| i)
            .collect();
        if chosen.is_empty() {
            return None;
        }
        let exact = named["only"] == true
            && named["profiles"].as_u64() == Some(chosen.len() as u64)
            && chosen.len() == letters.len();
        if crate::tracing() {
            eprintln!(
                "import: text profiles select {} letters of {} regions, exact={exact}",
                chosen.len(),
                regions.len(),
            );
        }
        Some(RegionSet {
            keys: chosen
                .iter()
                .map(|&i| regions[i].profile.key.to_string())
                .collect(),
            guess: false,
            area: chosen.iter().map(|&i| regions[i].area).sum(),
            parts: chosen
                .iter()
                .map(|&i| (regions[i].centroid, regions[i].area))
                .collect(),
            probe: None,
            measured: false,
            exact,
        })
    }

    /// Puts the decoded profiles' set ([`Importer::mapped_set`]) first in
    /// `sets` (with what a probe found for the same regions); whether there
    /// is one.
    pub(crate) fn mapped_first(
        &self,
        sets: &mut Vec<RegionSet>,
        other: &serde_json::Map<String, Value>,
        sketch: i64,
    ) -> bool {
        let Some(mut mapped) = self.mapped_set(other, sketch) else {
            return false;
        };
        if let Some(k) = sets.iter().position(|s| same_keys(&s.keys, &mapped.keys)) {
            mapped.probe = sets.remove(k).probe;
        }
        sets.insert(0, mapped);
        true
    }

    /// The regions whose material the history's next states change: a
    /// point inside each region, `depth` off the sketch plane on either
    /// side, is in a different number of bodies after than before.
    fn probed_sets(
        &mut self,
        sketch: FeatureUid,
        frame: SketchFrame,
        regions: &[Region],
        depth: Option<f64>,
    ) -> Vec<RegionSet> {
        // Hundreds of regions (text, patterns) are not worth probing.
        if !self.oracle.enabled || regions.is_empty() || regions.len() > 300 {
            return Vec::new();
        }
        let _ = sketch;
        let inner = interior_points(regions);
        let normal = geom::normal(&frame);
        let depth = depth.unwrap_or(0.05);
        let kernel = self.doc.kernel();
        // Whether each point is inside a body: where a segment from it along
        // the sketch's normal out of the body's box crosses the body's
        // faces (mitcad#161: classifying the points took a minute on a
        // stored thread of 720 spline faces, as for thread angles,
        // mitcad#68), else the points classified.
        let inside_of = |shape: &K::Shape, points: &[[f64; 3]]| -> Option<Vec<bool>> {
            // (A box that holds the body is enough to reach out of it.)
            let crossed = kernel.enclosing_box(shape).ok().flatten().and_then(|b| {
                let reach = geom::distance(b.min, b.max) + 1.0;
                let segments: Vec<([f64; 3], [f64; 3])> = points
                    .iter()
                    .map(|&p| (p, geom::add(p, geom::scale(normal, reach))))
                    .collect();
                // All at once where the kernel can (each face set up once,
                // mitcad#181), else one by one.
                match kernel.segments_crossings(shape, &segments) {
                    Ok(all) => Some(
                        all.iter()
                            .map(|crossings| crate::ops::inside_at(crossings, 0.0))
                            .collect(),
                    ),
                    Err(_) => segments
                        .iter()
                        .map(|&(p, far)| {
                            let crossings = kernel.segment_crossings(shape, p, far).ok()?;
                            Some(crate::ops::inside_at(&crossings, 0.0))
                        })
                        .collect::<Option<Vec<bool>>>(),
                }
            });
            crossed.or_else(|| kernel.points_inside(shape, points).ok())
        };
        let count = |shapes: &[&K::Shape], points: &[[f64; 3]]| -> Option<Vec<usize>> {
            let mut counts = vec![0; points.len()];
            for shape in shapes {
                let inside = inside_of(shape, points)?;
                for (c, i) in counts.iter_mut().zip(inside) {
                    *c += usize::from(i);
                }
            }
            Some(counts)
        };
        let current: Vec<(K::Shape, Option<Sig>)> = crate::body_scope::bodies(self.doc)
            .iter()
            .map(|b| (b.shape.clone(), Sig::of(kernel, b.shape)))
            .collect();
        let now: f64 = current
            .iter()
            .filter_map(|(_, s)| s.map(|s| s.volume))
            .sum();
        let volume = |state: &[(StoredBody<K::Shape>, Sig)]| -> f64 {
            state.iter().map(|(_, s)| s.volume).sum()
        };
        let mut sets: Vec<RegionSet> = Vec::new();
        for q in self.lookahead(3) {
            let Ok(state) = self.oracle.state(kernel, q) else {
                continue;
            };
            // Bodies in both sets count the same before and after: only
            // the ones that differ are probed.
            let mut used = vec![false; state.len()];
            let mut before: Vec<&K::Shape> = Vec::new();
            for (shape, sig) in &current {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None => before.push(shape),
                }
            }
            let after: Vec<&K::Shape> = state
                .iter()
                .zip(&used)
                .filter(|(_, u)| !**u)
                .map(|((b, _), _)| &b.shape)
                .collect();
            if before.is_empty() && after.is_empty() {
                continue;
            }
            for side in [1.0, -1.0] {
                let points: Vec<[f64; 3]> = inner
                    .iter()
                    .map(|p| {
                        let at = p.unwrap_or([1e9, 1e9]);
                        geom::add(frame.point(at), geom::scale(normal, side * depth))
                    })
                    .collect();
                let (Some(b), Some(a)) = (count(&before, &points), count(&after, &points)) else {
                    return sets;
                };
                if crate::tracing() {
                    for (k, r) in regions.iter().enumerate() {
                        eprintln!(
                            "import:   probe state {q} side {side}: {} area {:.3} at {:?}: {} -> {}",
                            r.profile.key, r.area, inner[k], b[k], a[k]
                        );
                    }
                }
                let chosen: Vec<&Region> = regions
                    .iter()
                    .zip(&inner)
                    .zip(b.iter().zip(&a))
                    .filter(|((_, p), (b, a))| p.is_some() && b != a)
                    .map(|((r, _), _)| r)
                    .collect();
                if chosen.is_empty() {
                    continue;
                }
                let set = RegionSet {
                    keys: chosen.iter().map(|r| r.profile.key.to_string()).collect(),
                    guess: true,
                    area: chosen.iter().map(|r| r.area).sum(),
                    parts: chosen.iter().map(|r| (r.centroid, r.area)).collect(),
                    probe: Some((side, volume(state) - now)),
                    measured: false,
                    exact: false,
                };
                if !sets.iter().any(|s| s.keys == set.keys) {
                    sets.push(set);
                }
            }
        }
        sets
    }

    /// The item's operation. A new component's body is a new body of the
    /// component the history puts the item in: the import makes the file's
    /// components from the dump (F6, `components.rs`).
    pub fn operation(op: Option<&str>) -> &'static str {
        match op {
            Some(o) if o.starts_with("Join") => "join",
            Some(o) if o.starts_with("Cut") => "cut",
            Some(o) if o.starts_with("Intersect") => "intersect",
            _ => "new_body",
        }
    }

    /// The bodies a join, cut or intersection works on, to try in turn:
    /// the dump's (none: all), then, when the dump names none, those of the
    /// sketch's component that the item's history state (or the next one)
    /// changed, when that is not all of them (the stream decoder gives no
    /// participants, and a cut in the file may have left bodies in its way
    /// alone). All bodies come first, so that features keep no
    /// participants they do not need (a pattern of the feature later
    /// would combine with them).
    ///
    /// The stream decoder gives an extrusion's participants (mitcad#96):
    /// when every one is found and they are not all the solids of the
    /// sketch's component, they come first, then all bodies (unless the
    /// history's state changed only bodies they name) and the
    /// history's guess. The guess stays even where the file names every
    /// body: a join there changes only the bodies its material overlaps,
    /// while MitCad's also takes in those it only touches.
    pub fn participant_options(
        &mut self,
        index: i64,
        bodies: &Option<Vec<Reference>>,
        operation: &str,
        sketch: FeatureUid,
    ) -> Vec<Vec<String>> {
        let component = self.doc.feature(sketch).map(|f| f.component);
        self.participant_options_in(index, bodies, operation, component)
    }

    /// [`Importer::participant_options`] for the bodies of `component`.
    pub fn participant_options_in(
        &mut self,
        index: i64,
        bodies: &Option<Vec<Reference>>,
        operation: &str,
        component: Option<ComponentUid>,
    ) -> Vec<Vec<String>> {
        let decoded = self.participants(bodies);
        if !decoded.is_empty() && operation != "new_body" {
            let all = self
                .current_bodies()
                .into_iter()
                .filter(|(b, _)| self.doc.body_component(*b) == component)
                .all(|(b, _)| decoded.contains(&b.to_string()));
            let changed = self.changed_bodies(index, &[], operation, component);
            // The state changed only bodies the file names (mitcad#181): on
            // all bodies the extrusion gives those the same as on the named
            // ones, and the others must stay, so it is not tried (on bodies
            // of 14 000 faces it took 30 s a definition).
            let named_only = changed
                .as_ref()
                .is_some_and(|c| c.iter().all(|b| decoded.contains(b)));
            let mut options = Vec::new();
            if !all {
                options.push(decoded);
            }
            if !named_only || all {
                options.push(Vec::new());
            }
            if let Some(changed) = changed
                && !options.contains(&changed)
            {
                options.push(changed);
            }
            return options;
        }
        let mut options = vec![decoded.clone()];
        if let Some(changed) = self.changed_bodies(index, &decoded, operation, component) {
            // Bodies a join touches but the file does not take in (one of
            // the bodies that changed may differ from the history's only
            // by the replay's own small errors): each of them alone after.
            let alone: Vec<Vec<String>> = if changed.len() > 1 {
                changed.iter().take(4).map(|b| vec![b.clone()]).collect()
            } else {
                Vec::new()
            };
            options.push(changed);
            options.extend(alone);
        }
        options
    }

    /// See [`Importer::participant_options`].
    fn changed_bodies(
        &mut self,
        index: i64,
        decoded: &[String],
        operation: &str,
        component: Option<ComponentUid>,
    ) -> Option<Vec<String>> {
        if !decoded.is_empty() || operation == "new_body" || !self.oracle.enabled {
            return None;
        }
        let state = self
            .oracle
            .item_state(index)
            .unwrap_or_else(|| self.oracle.next_index());
        let kernel = self.doc.kernel();
        let after = self.oracle.state(kernel, state).ok()?;
        let after: Vec<Sig> = after.iter().map(|(_, s)| *s).collect();
        let current: Vec<(BodyUid, Sig)> = self
            .current_bodies()
            .into_iter()
            .filter(|(b, _)| self.doc.body_component(*b) == component)
            .collect();
        let unchanged_by = |same: &dyn Fn(&Sig, &Sig) -> bool| {
            let mut used = vec![false; after.len()];
            let mut changed = Vec::new();
            for (uid, sig) in &current {
                match (0..after.len()).find(|&j| !used[j] && same(&after[j], sig)) {
                    Some(j) => used[j] = true,
                    None => changed.push(uid.to_string()),
                }
            }
            changed
        };
        let mut changed = unchanged_by(&|a, b| a.same(b));
        if changed.len() == current.len() {
            // The replay can give a body other faces than the file's (its
            // faces split otherwise, mitcad#60): one the same within
            // `EXACT` apart from its faces is taken as unchanged too.
            changed = unchanged_by(&|a, b| a.distance(b) <= crate::history::EXACT);
        }
        (!changed.is_empty() && changed.len() < current.len()).then_some(changed)
    }

    /// The decoded participants, once each; none unless every one is
    /// found (a part of them would leave the others out).
    pub fn participants(&self, bodies: &Option<Vec<Reference>>) -> Vec<String> {
        bodies
            .as_deref()
            .and_then(|b| self.bodies_of_refs(b))
            .unwrap_or_default()
            .into_iter()
            .map(|b: BodyUid| b.to_string())
            .collect()
    }

    /// An extent object: an origin or construction plane, a face, a body,
    /// or a point (mitcad#1329: a vertex, a sketch point or a construction
    /// point, the plane through it).
    fn extent_object(&self, entity: &Option<Reference>, def: &Definition) -> Option<Value> {
        match entity.as_ref()? {
            r @ (Reference::Vertex(_)
            | Reference::SketchEntity(_)
            | Reference::ConstructionPoint(_)) => {
                let point = self.point_options(Some(r)).into_iter().next()?;
                Some(json!({"type": "point", "point": point}))
            }
            Reference::ConstructionPlane(c) => match c.origin.clone().flatten().as_deref() {
                Some(o @ ("XY" | "XZ" | "YZ")) => {
                    Some(json!({"type": "plane", "plane": o.to_lowercase()}))
                }
                _ => {
                    let uid = c
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))?;
                    Some(json!({"type": "plane", "plane": uid.to_string()}))
                }
            },
            Reference::Face(fp) => {
                let (body, face) = refs::resolve_face(self.doc, fp).or_else(|| {
                    fp.point_on_face
                        .is_none()
                        .then(|| refs::resolve_cylinder_surface(self.doc, fp))
                        .flatten()
                })?;
                Some(
                    json!({"type": "face", "body": body.to_string(), "face": face.to_string(),
                            "chained": def.is_chained == Some(true)}),
                )
            }
            Reference::Body(fp) => {
                let body = self.body_ref(fp)?;
                Some(json!({"type": "body", "body": body.to_string(),
                            "through": def.is_minimum_solution == Some(false)}))
            }
            _ => None,
        }
    }

    /// A taper that is not zero, as a definition's value.
    fn taper_of(&self, taper: &Option<Reference>) -> Option<Value> {
        (!Self::is_zero(taper)).then(|| self.value(taper)).flatten()
    }

    /// One side's extent; Ok(None) when it is not decoded.
    fn side(
        &self,
        def: &Definition,
        taper: &Option<Reference>,
    ) -> Result<Option<(Value, Option<bool>)>, String> {
        let taper = (!Self::is_zero(taper)).then(|| self.value(taper)).flatten();
        let mut v = match def.definition_type.as_deref() {
            Some("DistanceExtentDefinition") => {
                json!({"type": "distance", "distance": self.value_or(&def.distance, "distance")?})
            }
            Some("ToEntityExtentDefinition") => {
                let object = self
                    .extent_object(&def.entity, def)
                    .ok_or("its extent's object was not found")?;
                let mut v = json!({"type": "to_object", "object": object});
                if !Self::is_zero(&def.offset) {
                    v["offset"] = self.value_or(&def.offset, "offset")?;
                }
                v
            }
            Some("ThroughAllExtentDefinition") => {
                let flip = def.is_positive_direction.map(|p| !p);
                let mut v = json!({"type": "through_all"});
                if let Some(t) = taper {
                    v["taper"] = t;
                }
                return Ok(Some((v, flip)));
            }
            Some(other) => return Err(format!("{other} is not supported")),
            None => return Ok(None),
        };
        if let Some(t) = taper {
            v["taper"] = t;
        }
        Ok(Some((v, None)))
    }

    fn extrude(
        &mut self,
        index: i64,
        item: &TimelineItem,
        d: &ExtrudeDetail,
    ) -> Result<Vec<Candidate>, String> {
        // A surface extrusion (mitcad#455): the extrusion's walls without
        // caps, a new surface body, of the regions or of the curves.
        let surface = d.is_solid == Some(false);
        // Text profiles whose texts the file no longer has (mitcad#332): the
        // history keeps the extrusion's result, but no sketch has its
        // profile, so no region of the sketch can stand in for it.
        if let Some(texts) = d
            .other
            .get("_f3d_missing_texts")
            .and_then(Value::as_array)
            .filter(|t| !t.is_empty())
        {
            let sketch = profiles_of(&d.profile, &d.other)
                .into_iter()
                .find_map(|p| match p {
                    Reference::Profile(p) => p.sketch.flatten(),
                    _ => None,
                });
            let deleted = sketch
                .map(|s| format!(" (deleted from {s})"))
                .unwrap_or_default();
            // One and several as whole messages (mitcad#1284).
            return Err(if texts.len() == 1 {
                format!("its profile is a sketch text the file no longer has{deleted}")
            } else {
                format!(
                    "its profile is {} sketch texts the file no longer has{deleted}",
                    texts.len()
                )
            });
        }
        // The faces of bodies the profile selects (mitcad#169), and the
        // profiles of its sketch.
        let (face_refs, profiles): (Vec<Reference>, Vec<Reference>) =
            profiles_of(&d.profile, &d.other)
                .into_iter()
                .partition(|p| matches!(p, Reference::Face(_)));
        let participants = d
            .participant_bodies
            .as_deref()
            .and_then(|b| self.bodies_of_refs(b))
            .unwrap_or_default();
        let mut profile_faces = self.profile_faces(&face_refs, &participants);
        // The faces are read when each is named by the file's own names
        // (the decoder found it in the history) and is the only face of
        // the replay that fits.
        let mut faces_read = !face_refs.is_empty()
            && profile_faces.len() == face_refs.len()
            && profile_faces
                .iter()
                .zip(&face_refs)
                .all(|((body, face, _), r)| {
                    matches!(r, Reference::Face(fp)
                        if refs::face_resolved_uniquely(self.doc, fp, (*body, face)))
                });
        // A part of an item in another component whose faces the item's
        // own operation took away: as the item's operation found them
        // (mitcad#1347).
        let mut made = None;
        if profile_faces.is_empty()
            && !face_refs.is_empty()
            && profiles.is_empty()
            && let Some((faces, read, sketch)) =
                self.part_face_sketch(index, item, &face_refs, &participants)
        {
            profile_faces = faces;
            faces_read = read;
            made = Some(sketch);
        }
        // No profile of a sketch: its faces as decoded, else perhaps a face
        // of a body (tried first), else the last sketch before it.
        let faces = if profiles.is_empty() {
            let faces =
                self.face_profiles(index, item, d, &profile_faces, faces_read, made.as_ref());
            self.with_decoded_participants(index, item, d, faces)
        } else {
            Vec::new()
        };
        let (sketch, last_sketch) = match self.profile_sketch(&profiles, index) {
            Ok(s) => s,
            Err(_) if !faces.is_empty() => {
                // Face-only profiles have no deferred sketch-region set.
                if let Some(g) = self.guesses.as_mut() {
                    g.done = true;
                }
                return Ok(faces);
            }
            Err(e) => return Err(e),
        };
        // Regions are probed halfway along a distance extent.
        let probe = d
            .extent_one
            .as_ref()
            .or(d.symmetric_extent.as_ref())
            .filter(|e| e.definition_type.as_deref() != Some("ThroughAllExtentDefinition"))
            .and_then(|e| param(&e.distance))
            .and_then(|p| p.value)
            .map(|v| mm(v).abs() / 2.0)
            .filter(|v| *v > 1e-6);
        let length = |def: &Definition| {
            param(&def.distance)
                .and_then(|p| p.value)
                .map(|v| mm(v).abs())
        };
        let kind = d.extent_type.as_deref().unwrap_or("");
        let one = d.extent_one.clone().unwrap_or_default();
        let raw = item.f3d.as_ref().and_then(|f| f.extrude.as_ref());
        // Each side's extent kind in the stream (mitcad#154): 1 a distance,
        // 2 up to an object, 4 through all.
        let raw_sides: Option<[u32; 2]> = raw
            .and_then(|r| r.sides.as_deref())
            .and_then(|s| <[u32; 2]>::try_from(s).ok());
        // The stream's extent codes: 1 one side, 2 two sides, 3 symmetric;
        // for one side 2 a distance, 1 up to an object, 0 through all.
        let raw_extent = raw.map(|r| (r.extent_a, r.extent_b));
        let symmetric = kind.starts_with("Symmetric")
            || d.symmetric_extent.is_some()
            || raw_extent.is_some_and(|(a, _)| a == Some(3));
        // The profile area the history's change asks for: the change over
        // the length swept by a distance extent (a prism's volume).
        let target_area = if self.oracle.enabled && d.is_thin_extrude != Some(true) {
            let swept = if symmetric {
                let s = d.symmetric_extent.as_ref().unwrap_or(&one);
                length(s).map(|l| {
                    if s.is_full_length.or(raw.and_then(|r| r.full_length)) == Some(true) {
                        l
                    } else {
                        2.0 * l
                    }
                })
            } else if kind.starts_with("TwoSides") || d.has_two_extents == Some(true) {
                let two = d.extent_two.clone().unwrap_or_default();
                length(&one).zip(length(&two)).map(|(a, b)| a + b)
            } else if one.definition_type.as_deref() == Some("DistanceExtentDefinition") {
                length(&one)
            } else {
                None
            };
            let change = match self.oracle.item_state(index) {
                Some(_) => self.history_change(index),
                None => self.change_to_next(),
            };
            // (Less the faces the profile selects besides the sketch's.)
            let faces: f64 = profile_faces.iter().map(|(_, _, a)| a).sum();
            swept
                .filter(|l| *l > 1e-6)
                .zip(change)
                .map(|(l, c)| c.abs() / l - faces)
                .filter(|a| *a > 1e-9)
        } else {
            None
        };
        // The faces the profile selects besides the sketch's regions
        // (mitcad#169): projected into the sketch, whose regions on them
        // the extrusion takes too.
        let face_keys = (!face_refs.is_empty() && profile_faces.len() == face_refs.len())
            .then(|| self.faces_into_sketch(index, sketch, &profile_faces))
            .flatten();
        if !face_refs.is_empty() && face_keys.is_none() && crate::tracing() {
            eprintln!(
                "import: {}: the faces its profile selects were not found or not projected",
                crate::item_name(item)
            );
        }
        // Checked against the history one part at a time (mitcad#181): the
        // decoded profiles' definitions first on their own, without the
        // other sets of regions (probing the next states for them took
        // 10–30 s an extrusion on large bodies, while the decoded profiles
        // gave the state for most), the others when those gave none.
        let decoded_part = match self.guesses.as_mut() {
            Some(g) if !g.named => {
                g.named = true;
                true
            }
            _ => false,
        };
        let decoded_set = (decoded_part && face_refs.is_empty() && faces.is_empty())
            .then(|| self.mapped_set(&d.other, sketch))
            .flatten();
        let decoded_only = decoded_set.is_some();
        // A thin extrusion along sketch curves (mitcad#463): those its
        // profile names (an open path), else, when its sketch has no regions,
        // the sketch's curves that are not construction geometry, as a guess
        // the history checks.
        // A surface extrusion's curves likewise (mitcad#455).
        let wall_curves = ((d.is_thin_extrude == Some(true) || surface) && decoded_set.is_none())
            .then(|| self.wall_curves(&profiles, sketch))
            .flatten();
        let (sets, mapped) = match decoded_set {
            Some(set) => (vec![set], true),
            None if wall_curves.is_some() => (vec![RegionSet::default()], false),
            None => {
                let mut sets = self.region_sets(&profiles, sketch, probe, target_area)?;
                // The regions the decoded profile loops select first
                // (mitcad#96).
                let mapped = self.mapped_first(&mut sets, &d.other, sketch);
                (sets, mapped)
            }
        };
        let sketch_uid = self.sketches[&sketch].uid.to_string();
        let operation = Self::operation(
            d.operation
                .as_deref()
                .or(raw.and_then(|r| r.operation.clone().flatten()).as_deref()),
        );
        let participant_options = self.participant_options(
            index,
            &d.participant_bodies,
            operation,
            self.sketches[&sketch].uid,
        );
        let decoded_flip = self.decoded_flip(raw, self.sketches[&sketch].uid);
        // A join whose tool touches no body makes a new body (the .f3d rule):
        // when the history's state has more bodies, that too.
        let join_as_new = operation == "join" && self.oracle.enabled && {
            let state = self
                .oracle
                .item_state(index)
                .unwrap_or_else(|| self.oracle.next_index());
            let kernel = self.doc.kernel();
            let after = self.oracle.try_sigs(kernel, state).map_or(0, |s| s.len());
            after > self.current_sigs().len()
        };

        // Extents: (extent, flip, guess, length swept in mm when known).
        let mut extents: Vec<ExtentChoice> = Vec::new();
        if symmetric
            && d.symmetric_extent
                .as_ref()
                .unwrap_or(&one)
                .distance
                .is_none()
        {
            // Symmetric through all (no distance).
            extents.push((
                json!({"type": "through_all", "both_sides": true}),
                false,
                false,
                None,
            ));
        } else if symmetric {
            let s = d.symmetric_extent.as_ref().unwrap_or(&one);
            // Without the stream decoder's length: a half each way first.
            let fulls = match (s.is_full_length, raw.and_then(|r| r.full_length)) {
                // An API dump says it.
                (Some(full), None) => vec![(full, false)],
                // The stream decoder's (mitcad#96) first, the other after it.
                (Some(full), Some(_)) | (None, Some(full)) => {
                    vec![(full, false), (!full, true)]
                }
                (None, None) => vec![(false, false), (true, true)],
            };
            for (full, guess) in fulls {
                let mut v = json!({"type": "symmetric", "distance": self.value_or(&s.distance, "distance")?,
                                   "full_length": full});
                let taper = s.taper_angle.clone().or(d.taper_angle_one.clone());
                if !Self::is_zero(&taper) {
                    v["taper"] = self.value_or(&taper, "taper")?;
                }
                let swept = length(s).map(|l| if full { l } else { 2.0 * l });
                extents.push((v, false, guess, swept));
            }
        } else if kind.starts_with("TwoSides") || d.has_two_extents == Some(true) {
            let two = d.extent_two.clone().unwrap_or_default();
            // A side without a distance or an object in the stream is
            // through all (its code there is 4).
            let through_all = |def: &Definition, taper: &Option<Reference>| {
                let mut v = json!({"type": "through_all"});
                if !Self::is_zero(taper)
                    && let Some(t) = self.value(taper)
                {
                    v["taper"] = t;
                }
                (raw.is_some() && def.definition_type.is_none()).then_some((v, None))
            };
            // A side up to a face the replay does not have as it is goes
            // up to its plane (mitcad#104).
            let side = |def: &Definition, taper: &Option<Reference>| {
                if def.definition_type.as_deref() == Some("ToEntityExtentDefinition")
                    && self.extent_object(&def.entity, def).is_none()
                    && let Some(mut v) = self.decoded_plane(def)?
                {
                    if !Self::is_zero(taper)
                        && let Some(t) = self.value(taper)
                    {
                        v["taper"] = t;
                    }
                    return Ok(Some((v, None)));
                }
                self.side(def, taper)
            };
            let (s1, side_one_flip) = match side(&one, &d.taper_angle_one)? {
                Some(s) => s,
                None => through_all(&one, &d.taper_angle_one).ok_or("side one not decoded")?,
            };
            let (s2, _) = match side(&two, &d.taper_angle_two)? {
                Some(s) => s,
                None => through_all(&two, &d.taper_angle_two).ok_or("side two not decoded")?,
            };
            let swept = length(&one).zip(length(&two)).map(|(a, b)| a + b);
            // Side one goes along the decoded direction; up to an object,
            // towards the side of the sketch the object lies on.
            // Two distances go along the sketch's normal, each by its
            // parameter's sign; the stored vector tells no side there
            // (mitcad#186).
            let flip = if s1["type"] == "to_object" {
                self.to_object_flip(&one, raw, sketch).or(decoded_flip)
            } else if s1["type"] == "distance" && s2["type"] == "distance" {
                Some(false)
            } else if s1["type"] == "through_all" {
                // Side one through all goes the way its flag says (as a
                // one-sided extent does); side two the other way.
                side_one_flip.or(decoded_flip)
            } else {
                decoded_flip
            }
            .unwrap_or(false);
            let v = json!({"type": "two_sides", "side1": s1, "side2": s2});
            extents.push((v.clone(), flip, false, swept));
            extents.push((v, !flip, true, swept));
        } else if one.definition_type.as_deref() == Some("ToEntityExtentDefinition")
            && self.extent_object(&one.entity, &one).is_none()
        {
            // Up to a planar face the decoder named but the replay does not
            // have as it is (mitcad#96): its plane first.
            let plane = self.decoded_plane(&one)?;
            if let Some(v) = &plane {
                let flip = self
                    .to_object_flip(&one, raw, sketch)
                    .or(decoded_flip)
                    .unwrap_or(false);
                extents.push((v.clone(), flip, false, None));
                extents.push((v.clone(), !flip, true, None));
            }
            // Up to an object the decoder does not name: planar faces
            // parallel to the sketch, nearest first, and through all.
            if !self.oracle.enabled {
                if plane.is_none() {
                    return Err("its extent's object was not decoded".to_owned());
                }
            } else {
                extents.extend(self.to_object_guesses(sketch, index, &one, None)?);
            }
            // With the extrusion's taper (an `.ipt` extrusion up to a face
            // keeps one).
            if let Some(t) = self.taper_of(&d.taper_angle_one) {
                for e in &mut extents {
                    e.0["taper"] = t.clone();
                }
            }
        } else {
            match self.side(&one, &d.taper_angle_one)? {
                Some((v, flip)) => {
                    let flip = if v["type"] == "to_object" {
                        flip.or(self.to_object_flip(&one, raw, sketch))
                    } else {
                        flip
                    };
                    // A one-sided distance goes along the sketch's normal by
                    // its parameter's sign: the reference models store the
                    // same vector for a negative direction, a negative
                    // distance and a positive one, and of the corpus'
                    // settled extrusions with a stored vector against the
                    // normal 8 of 9 went along it (mitcad#186).
                    // A side through all goes the way its flag byte says
                    // (0 along the normal, 1 against it; the API reference
                    // dumps agree on all 78 through-all extents), whatever
                    // the history would accept; without the byte the
                    // stored vector's side is a guess.
                    let through_all = v["type"] == "through_all";
                    let through_all_read = through_all && flip.is_some();
                    let flip = if v["type"] == "distance" && raw_sides == Some([1, 0]) {
                        false
                    } else {
                        flip.or(decoded_flip).unwrap_or(false)
                    };
                    let swept = length(&one);
                    extents.push((v.clone(), flip, through_all && !through_all_read, swept));
                    if decoded_flip.is_some() || flip || through_all {
                        extents.push((v.clone(), !flip, true, swept));
                    }
                    if through_all {
                        let mut both = v.clone();
                        both["both_sides"] = json!(true);
                        extents.push((both, false, true, None));
                    }
                    // The decoder cannot tell a symmetric extent (code 3).
                    if d.extent_type.is_none() && v["type"] == "distance" {
                        let mut s = v.clone();
                        s["type"] = json!("symmetric");
                        extents.push((s, false, true, swept.map(|l| 2.0 * l)));
                    }
                    // The object the decoder named (mitcad#96) first, then
                    // a planar face's whole plane (a face of the replay
                    // bounded otherwise, or another piece of the plane),
                    // then the faces the history may settle.
                    if v["type"] == "to_object" && self.oracle.enabled {
                        let from = extents.len();
                        // Up to a body, the other solution too (its first
                        // face or its far side): an extrusion that fills a
                        // pocket of its participant up to the face of the body
                        // above it stores the far side's flag.
                        if v["object"]["type"] == "body" {
                            let mut other = v.clone();
                            other["object"]["through"] = json!(v["object"]["through"] != true);
                            extents.push((other.clone(), flip, true, None));
                            extents.push((other, !flip, true, None));
                        }
                        if let Ok(Some(p)) = self.decoded_plane(&one) {
                            extents.push((p.clone(), flip, true, None));
                            extents.push((p, !flip, true, None));
                        }
                        if let Ok(guesses) =
                            self.to_object_guesses(sketch, index, &one, Some(&v["object"]))
                        {
                            extents.extend(guesses);
                        }
                        if let Some(t) = self.taper_of(&d.taper_angle_one) {
                            for e in &mut extents[from..] {
                                e.0["taper"] = t.clone();
                            }
                        }
                    }
                }
                None => {
                    // Not a distance (the decoder leaves the extent out):
                    // through all is the likeliest, and certain with the
                    // stream's extent code for it, or a one-sided input
                    // slot through all of bodies (mitcad#96), and a decoded
                    // direction.
                    let slot = raw.is_some_and(|r| {
                        r.extent_a == Some(1) && r.slot_roles.iter().flatten().any(|&x| x == 5)
                    });
                    let through_all = raw_extent == Some((Some(1), Some(0))) || slot;
                    let flip = decoded_flip.unwrap_or(false);
                    for (flip, guess) in [
                        (flip, !(through_all && decoded_flip.is_some())),
                        (!flip, true),
                    ] {
                        extents.push((json!({"type": "through_all"}), flip, guess, None));
                    }
                    extents.push((
                        json!({"type": "through_all", "both_sides": true}),
                        false,
                        true,
                        None,
                    ));
                }
            }
        }
        let sign = match operation {
            "cut" => Some(-1.0),
            "intersect" => None,
            _ => Some(1.0),
        };
        let start = self.extrude_start(d)?;
        let thin = if d.is_thin_extrude == Some(true) {
            // The file's side one lies inside the profile (towards its
            // material), side two outside it (the reference models' thin
            // variants, mitcad#186); the model's side one is the one away
            // from the material, so the sides swap.
            let location = |l: &Option<String>| match l.as_deref() {
                Some(l) if l.contains("Center") => "center",
                Some(l) if l.contains("Side2") => "side1",
                _ => "side2",
            };
            let mut t = json!({"location": location(&d.thin_extrude_wall_location_one),
                               "thickness": self.value_or(&d.thin_extrude_wall_thickness_one, "wall thickness")?});
            if d.thin_extrude_wall_thickness_two.is_some() {
                t["side2"] = json!({"location": location(&d.thin_extrude_wall_location_two),
                    "thickness": self.value_or(&d.thin_extrude_wall_thickness_two, "wall thickness")?});
            }
            Some(t)
        } else {
            None
        };
        // The side of the sketch plane a one-sided extent goes to (±1 along
        // its normal); None for both sides.
        let distance_sign = param(&one.distance)
            .and_then(|p| p.value)
            .map_or(1.0, |v| if v < 0.0 { -1.0 } else { 1.0 });
        let side_of = |extent: &Value, flip: bool| -> Option<f64> {
            let along = if flip { -1.0 } else { 1.0 };
            match extent["type"].as_str() {
                Some("distance") => Some(along * distance_sign),
                Some("to_object") => Some(along),
                Some("through_all") if extent["both_sides"] != true => Some(along),
                _ => None,
            }
        };
        // What the stream's bytes give (mitcad#186): a candidate is `read`
        // when its profile selection is exact, and its operation, start,
        // thin-wall block, bodies and extent are each read from the stream
        // (an extent by its codes and parameters, a direction only where a
        // byte decides it).
        let full_length_read = d
            .symmetric_extent
            .as_ref()
            .unwrap_or(&one)
            .is_full_length
            .or(raw.and_then(|r| r.full_length))
            .is_some();
        // A side up to an object: side one by its code; side two where the
        // stream keeps the block of its object (the nearest-solution byte),
        // for the stream's side codes name the first side only after an
        // object.
        let second_object_read = d
            .extent_two
            .as_ref()
            .is_some_and(|e| e.is_minimum_solution.is_some());
        let side_read = |side: &Value, code: u32, second: bool| match side["type"].as_str() {
            Some("distance") => code == 1,
            Some("through_all") => code == 4,
            Some("to_object") if second => code == 2 || (code == 0 && second_object_read),
            Some("to_object") => code == 2,
            _ => false,
        };
        let extent_read = |extent: &Value, flip: bool| -> bool {
            match (extent["type"].as_str(), raw_sides) {
                (Some("distance"), Some([1, 0])) => !flip,
                (Some("symmetric"), Some([1, 0] | [1, 1])) => full_length_read,
                (Some("through_all"), Some([4, 4])) => extent["both_sides"] == true && !flip,
                // One side through all: the flag byte decides the side, 0
                // along the sketch's normal, 1 against it, in every writer.
                (Some("through_all"), Some([4, 0])) => {
                    extent["both_sides"] != true
                        && raw
                            .and_then(|r| r.flag)
                            .filter(|f| *f <= 1)
                            .is_some_and(|f| flip == (f == 1))
                }
                (Some("two_sides"), Some([a, b])) => {
                    // Two distances go along the sketch's normal; a side up
                    // to an object towards the object's side.
                    let direction = if extent["side1"]["type"] == "to_object" {
                        self.to_object_flip(&one, raw, sketch) == Some(flip)
                    } else if extent["side1"]["type"] == "through_all" {
                        // Side one through all: the flag byte decides the
                        // direction, as for one side.
                        raw.and_then(|r| r.flag)
                            .filter(|f| *f <= 1)
                            .is_some_and(|f| flip == (f == 1))
                    } else {
                        !flip
                    };
                    direction
                        && side_read(&extent["side1"], a, false)
                        && side_read(&extent["side2"], b, true)
                }
                // Up to an object: the object the stream names, on the side
                // of the sketch's plane it lies on (or the flag byte).
                (Some("to_object"), Some([2, 0])) => {
                    self.to_object_flip(&one, raw, sketch) == Some(flip)
                }
                _ => false,
            }
        };
        let start_read = match (raw.and_then(|r| r.start), &start) {
            (Some(0), None) => true,
            (Some(1), Some(s)) => s["type"] == "offset",
            (Some(2), Some(s)) => s["type"] == "object",
            _ => false,
        };
        let thin_read = match raw.and_then(|r| r.thin) {
            Some(false) => d.is_thin_extrude != Some(true),
            // A two-sided thin extrusion's second wall (mitcad#25).
            Some(true) => {
                thin.is_some()
                    && (raw_sides.is_some_and(|s| s[1] == 0)
                        || (d.thin_extrude_wall_location_two.is_some()
                            && d.thin_extrude_wall_thickness_two.is_some()))
            }
            None => false,
        };
        // (The sketch's regions on the faces make up the faces' area, which
        // `face_keys` holds.)
        let read_inputs = ((face_refs.is_empty() && face_keys.is_none())
            || (faces_read && face_keys.is_some()))
            && !last_sketch
            && raw.is_some_and(|r| matches!(r.operation_code, Some(1..=4)))
            && start_read
            && thin_read
            && (operation == "new_body" || !self.participants(&d.participant_bodies).is_empty());
        if crate::tracing() {
            eprintln!(
                "import: {} (item {}): extrude read: faces={} faces_read={faces_read} face_keys={} last_sketch={} op_code={:?} start_read={} thin_read={} participants={} raw_sides={:?} extents_read={:?} mapped={} sets={:?}",
                crate::item_name(item),
                index,
                face_refs.is_empty(),
                face_keys.is_none(),
                !last_sketch,
                raw.and_then(|r| r.operation_code),
                start_read,
                thin_read,
                operation == "new_body" || !self.participants(&d.participant_bodies).is_empty(),
                raw_sides,
                extents
                    .iter()
                    .map(|(e, f, _, _)| extent_read(e, *f))
                    .collect::<Vec<_>>(),
                mapped,
                sets.iter().map(|s| (s.exact, s.guess)).collect::<Vec<_>>()
            );
        }
        let sketch_component = self
            .doc
            .feature(self.sketches[&sketch].uid)
            .map(|f| f.component);
        let mut candidates = Vec::new();
        // Profile sets outside, extents inside: the likeliest extent of
        // each set is tried before the next set.
        for (k, set) in sets.iter().enumerate() {
            let decoded = mapped && k == 0;
            for (extent, flip, extent_guess, swept) in &extents {
                let profiles: Vec<Value> = set
                    .keys
                    .iter()
                    .map(|r| json!({"sketch": sketch_uid, "region": r}))
                    .collect();
                let mut def = json!({"type": "extrude", "profiles": profiles, "extent": extent,
                                     "flip": flip, "operation": operation});
                if let Some(s) = &start {
                    def["start"] = s.clone();
                }
                if let Some(t) = &thin {
                    def["thin"] = t.clone();
                }
                if let Some((curves, _)) = &wall_curves {
                    def.as_object_mut().map(|o| o.remove("profiles"));
                    let curves = json!({"sketch": sketch_uid, "curves": curves});
                    if surface {
                        def["curves"] = curves;
                    } else {
                        def["thin"]["curves"] = curves;
                    }
                }
                if surface {
                    def["surface"] = json!(true);
                }
                // Objects on other components' bodies (mitcad#104): the
                // extrusion goes where its sketch is and links them.
                if let Some(own) = sketch_component {
                    let links = self.object_links(own, &def)?;
                    if !links.is_empty() {
                        def["object_links"] = Value::Object(links);
                    }
                }
                let mut c = Candidate::new(def);
                // Without tapers, a prism's volume (overlaps aside); regions
                // a history state changes on the extrusion's side, the
                // state's change (a cut deeper than the body removes less
                // than its prism), so that they are tried first.
                c.predicted = match set.probe {
                    Some((side, change)) if side_of(extent, *flip).is_none_or(|s| s == side) => {
                        Some(change)
                    }
                    _ => sign.zip(*swept).map(|(s, l)| s * l * set.area),
                };
                let mut notes = Vec::new();
                if last_sketch {
                    notes.push("the sketch is the last one before it".to_owned());
                }
                if set.guess || *extent_guess {
                    c.guess = true;
                }
                if last_sketch || set.guess || *extent_guess {
                    c.input_provenance = InputProvenance::Inferred;
                } else if read_inputs && decoded && set.exact && extent_read(extent, *flip) {
                    c.input_provenance = InputProvenance::Read;
                }
                c.first = (decoded && !*extent_guess) || set.measured;
                // A surface changes no volume (mitcad#455).
                if surface {
                    c.predicted = None;
                }
                if let Some((_, exact)) = &wall_curves {
                    c.predicted = None;
                    if !exact {
                        c.guess = true;
                        c.input_provenance = InputProvenance::Inferred;
                        notes.push("the walls follow the sketch's curves".to_owned());
                    }
                }
                if decoded && crate::tracing() && c.input_provenance == InputProvenance::Unknown {
                    eprintln!(
                        "import: extrusion not read: faces {} keys {} last sketch {last_sketch} \
                         operation {:?} start {start_read} thin {thin_read} participants {} \
                         exact {} extent {} sides {raw_sides:?}",
                        face_refs.is_empty(),
                        face_keys.is_none(),
                        raw.and_then(|r| r.operation_code),
                        operation == "new_body"
                            || !self.participants(&d.participant_bodies).is_empty(),
                        set.exact,
                        extent_read(extent, *flip)
                    );
                }
                if decoded {
                    notes.push("the profiles as decoded".to_owned());
                } else if sets.len() > 1 {
                    notes.push(format!("profiles {}", set.keys.join(" + ")));
                }
                if !notes.is_empty() {
                    c.note = Some(notes.join("; "));
                }
                candidates.extend(with_participants(&c, &participant_options));
                // Which side of an open chain the file's side one is: the
                // other one is tried next (the history settles it).
                if wall_curves.is_some() {
                    let other = |l: &Value| match l.as_str() {
                        Some("side1") => Some(json!("side2")),
                        Some("side2") => Some(json!("side1")),
                        _ => None,
                    };
                    let mut o = c.clone();
                    let mut changed = false;
                    if let Some(l) = other(&o.defs[0]["thin"]["location"]) {
                        o.defs[0]["thin"]["location"] = l;
                        changed = true;
                    }
                    if let Some(l) = other(&o.defs[0]["thin"]["side2"]["location"]) {
                        o.defs[0]["thin"]["side2"]["location"] = l;
                        changed = true;
                    }
                    if changed {
                        o.first = false;
                        o.guess = true;
                        o.input_provenance = InputProvenance::Inferred;
                        candidates.extend(with_participants(&o, &participant_options));
                    }
                }
                if join_as_new {
                    let mut n = c.clone();
                    n.defs[0]["operation"] = json!("new_body");
                    n.guess = true;
                    n.input_provenance = InputProvenance::Inferred;
                    n.first = false;
                    notes.push("a join that touches no body: a new body".to_owned());
                    n.note = Some(notes.join("; "));
                    candidates.push(n);
                }
            }
        }
        if let Some(keys) = &face_keys {
            for c in &mut candidates {
                with_face_regions(c, &sketch_uid, keys);
            }
        }
        let mut all = faces;
        all.extend(candidates);
        let key = |c: &Candidate| c.defs.iter().map(Value::to_string).collect::<Vec<_>>();
        if decoded_only {
            // The decoded profiles' definitions that come first among all
            // (`Candidate::first`); the others after.
            all.retain(|c| c.first);
            if all.is_empty() {
                return self.extrude(index, item, d);
            }
            // (Without the probes, which settle the change a region set
            // makes, the prism's volume is no estimate to rank them by or
            // leave them out for: they keep their order.)
            for c in &mut all {
                c.predicted = None;
            }
            if let Some(g) = self.guesses.as_mut() {
                g.seen = all.iter().map(key).collect();
            }
            return Ok(all);
        }
        if let Some(g) = self.guesses.as_mut() {
            g.done = true;
            all.retain(|c| !g.seen.contains(&key(c)));
        }
        Ok(all)
    }

    /// The faces of bodies an extrusion's profile selects (mitcad#169),
    /// each with its area (mm²), as the replay has them; none unless every
    /// one is found.
    fn profile_faces(
        &self,
        faces: &[Reference],
        participants: &[BodyUid],
    ) -> Vec<(BodyUid, FaceName, f64)> {
        let kernel = self.doc.kernel();
        let mut out = Vec::new();
        for f in faces {
            let Reference::Face(fp) = f else {
                return Vec::new();
            };
            // A face of the bodies the extrusion works on first: where
            // another body has a face through the same point and plane (a
            // body the extrusion's own lies against), the faces' order
            // chose either, and half of a cut's ten profile faces went to
            // the other body (mitcad#325).
            let resolved = refs::resolve_face_where(self.doc, fp, &|b| participants.contains(&b))
                .or_else(|| refs::resolve_face(self.doc, fp));
            let found = resolved.and_then(|(body, face)| {
                let shape = crate::body_scope::bodies(self.doc)
                    .into_iter()
                    .find(|b| b.uid == body)?
                    .shape;
                let area = kernel
                    .faces(shape)
                    .ok()?
                    .into_iter()
                    .find(|x| {
                        x.surface == "plane"
                            && x.names
                                .first()
                                .and_then(|n| n.parse::<FaceName>().ok())
                                .as_ref()
                                == Some(&face)
                    })?
                    .area;
                Some((body, face, area))
            });
            match found {
                Some(f) => {
                    if crate::tracing() {
                        eprintln!(
                            "import: the profile selects {} of {} ({:.4} mm²)",
                            f.1, f.0, f.2
                        );
                    }
                    out.push(f);
                }
                None => {
                    if crate::tracing() {
                        eprintln!(
                            "import: a face the profile selects is not a plane of the replay"
                        );
                    }
                    return Vec::new();
                }
            }
        }
        out
    }

    /// The faces of bodies an extrusion's profile selects besides its
    /// sketch's regions (mitcad#169), projected into that sketch (linked; a
    /// face of another component through its occurrence, placed once), as
    /// the file's extrusion takes them: the keys of the sketch's regions on
    /// the faces. Only into a sketch no other feature uses yet (the
    /// projections split its regions, so earlier features' region keys
    /// would change); None (nothing changed) when one does, a projection
    /// fails, or the regions on the faces do not make up their area.
    fn faces_into_sketch(
        &mut self,
        index: i64,
        sketch: i64,
        faces: &[(BodyUid, FaceName, f64)],
    ) -> Option<Vec<String>> {
        let uid = self.sketches.get(&sketch)?.uid;
        if let Some(user) = self
            .doc
            .features()
            .find(|f| f.uid != uid && f.def.references().features.contains(&uid))
        {
            if crate::tracing() {
                eprintln!(
                    "import: the faces cannot be projected into the sketch: {} uses it",
                    user.uid
                );
            }
            return None;
        }
        let depth = self.doc.undo_depth();
        let projected = self.project_faces(uid, faces);
        if projected.is_none() && crate::tracing() {
            eprintln!("import: projecting the profile's faces into the sketch failed");
        }
        let keys = projected.and_then(|()| self.regions_on_faces(index, uid, faces));
        if keys.is_none() && projected.is_some() && crate::tracing() {
            eprintln!("import: the sketch's regions on the faces do not make up their area");
        }
        if keys.is_none() {
            self.undo_to(depth);
        }
        keys
    }

    /// Projects the boundaries of `faces` into the sketch `uid` (linked; a
    /// face of another component through its occurrence, placed once).
    fn project_faces(&mut self, uid: FeatureUid, faces: &[(BodyUid, FaceName, f64)]) -> Option<()> {
        let component = self.doc.feature(uid)?.component;
        for (body, face, _) in faces {
            let owner = self.doc.body_component(*body)?;
            let path = if owner == component {
                None
            } else {
                match self.doc.assembly().paths_to(owner).as_slice() {
                    [path] => Some(path.clone()),
                    _ => return None,
                }
            };
            self.doc
                .project_from(
                    uid,
                    &TopoName::Face(face.clone()),
                    Some(*body),
                    true,
                    path.as_deref(),
                    None,
                )
                .ok()?;
        }
        Some(())
    }

    /// The keys of the regions of the sketch `uid` that lie on `faces` (a
    /// point inside each region on one of the faces, moved into the face's
    /// component by the placements at the item `index`); None unless they
    /// make up the faces' area (within 1e-3).
    fn regions_on_faces(
        &self,
        index: i64,
        uid: FeatureUid,
        faces: &[(BodyUid, FaceName, f64)],
    ) -> Option<Vec<String>> {
        let output = self.doc.sketch_output(uid)?;
        let component = self.doc.feature(uid)?.component;
        let kernel = self.doc.kernel();
        let mut on_faces = Vec::new();
        for (body, face, _) in faces {
            let shape = crate::body_scope::bodies(self.doc)
                .into_iter()
                .find(|b| b.uid == *body)?
                .shape;
            let owner = self.doc.body_component(*body)?;
            let into = if owner == component {
                Transform::IDENTITY
            } else {
                inverse(&self.placement_at(owner, index)?)
                    .after(&self.placement_at(component, index)?)
            };
            on_faces.push((shape, face, into));
        }
        let want: f64 = faces.iter().map(|(_, _, a)| a).sum();
        let mut keys = Vec::new();
        let mut area = 0.0;
        for (region, inside) in output
            .region_info
            .iter()
            .zip(interior_points(&output.region_info))
        {
            // A sliver of a region, left where the file's sketch lines
            // nearly meet, is no part of the face (and makes the
            // extruded tool degenerate).
            if region.area < 1e-6 * want {
                continue;
            }
            let Some(inside) = inside else { continue };
            let at = output.frame.point(inside);
            let on = on_faces.iter().any(|(shape, face, into)| {
                let p = into.apply_point(at);
                kernel
                    .face_point_normal(*shape, face, p)
                    .is_ok_and(|near| geom::distance(near.point, p) < 1e-4)
            });
            if on {
                keys.push(region.profile.key.to_string());
                area += region.area;
            }
        }

        let made_up = !keys.is_empty() && (area - want).abs() <= 1e-3 * want;
        if !made_up && crate::tracing() {
            eprintln!(
                "import: the sketch's regions on the faces make up {area:.4} mm² of their {want:.4} ({} regions of {})",
                keys.len(),
                output.region_info.len()
            );
        }
        made_up.then_some(keys)
    }

    /// Extents up to an object the history may settle (an object the
    /// decoder does not name, or that did not give the state): planar faces
    /// parallel to the sketch, nearest first, and through all both ways;
    /// all guesses. `decoded` (the object the decoder named) is left out.
    fn to_object_guesses(
        &self,
        sketch: i64,
        index: i64,
        one: &Definition,
        decoded: Option<&Value>,
    ) -> Result<Vec<ExtentChoice>, String> {
        let mut extents = Vec::new();
        let frame = self
            .doc
            .sketch_output(self.sketches[&sketch].uid)
            .map(|o| o.frame)
            .ok_or("its sketch did not evaluate")?;
        let normal = geom::normal(&frame);
        // Faces of the sketch's component as they are; those of other
        // components (the file's assembly context: a cut up to another
        // part) as fixed planes where their occurrences place them in
        // the sketch's component, which the extrusion can work in, at
        // this point of the timeline (mitcad#86).
        let own = self
            .doc
            .feature(self.sketches[&sketch].uid)
            .map(|f| f.component);
        let into_own = |c: ComponentUid| {
            let own = own?;
            Some(inverse(&self.placement_at(own, index)?).after(&self.placement_at(c, index)?))
        };
        let mut faces: Vec<(f64, Value)> = refs::planar_faces(self.doc)
            .into_iter()
            .filter_map(|(body, face, p)| {
                let c = self.doc.body_component(body)?;
                if Some(c) == own {
                    let object = json!({"type": "face", "body": body.to_string(),
                                            "face": face.to_string()});
                    return Some((p.origin, p.normal, object));
                }
                let t = into_own(c)?;
                let (origin, n) = (t.apply_point(p.origin), t.apply_vector(p.normal));
                let object = json!({"type": "plane", "plane": {"origin": origin, "normal": n}});
                Some((origin, n, object))
            })
            .filter(|(_, n, _)| geom::norm(geom::cross(*n, normal)) < 1e-9)
            .map(|(origin, _, object)| (geom::dot(geom::sub(origin, frame.origin), normal), object))
            .filter(|(s, _)| s.abs() > 1e-6)
            .collect();
        faces.sort_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
        faces.truncate(24);
        let offset = (!Self::is_zero(&one.offset))
            .then(|| self.value(&one.offset))
            .flatten();
        for (s, object) in faces {
            let mut v = json!({"type": "to_object", "object": object});
            if let Some(o) = &offset {
                v["offset"] = o.clone();
            }
            if decoded.is_some_and(|o| Self::same_object(o, &v["object"])) {
                continue;
            }
            extents.push((v, s < 0.0, true, Some(s.abs())));
        }
        // Up to the first face of a body of the component it reaches (an
        // object that is not a planar face parallel to the sketch: the next
        // face, a curved one).
        let bodies: Vec<BodyUid> = crate::body_scope::bodies(self.doc)
            .iter()
            .map(|b| b.uid)
            .collect();
        for body in bodies {
            if self.doc.body_component(body) != own {
                continue;
            }
            let mut v = json!({"type": "to_object",
                               "object": {"type": "body", "body": body.to_string(), "through": false}});
            if let Some(o) = &offset {
                v["offset"] = o.clone();
            }
            if decoded.is_some_and(|o| o["type"] == "body" && o["body"] == v["object"]["body"]) {
                continue;
            }
            for flip in [false, true] {
                extents.push((v.clone(), flip, true, None));
            }
        }
        for flip in [false, true] {
            extents.push((json!({"type": "through_all"}), flip, true, None));
        }
        Ok(extents)
    }

    /// Whether an extent up to the object the decoder named goes against
    /// the sketch's normal (mitcad#96): the side of the sketch's plane the
    /// object's point lies on (a face's, or a plane's origin), else the
    /// stream's flag (1: against the normal; on every extrusion up to an
    /// object of the corpus' older writers, where the stored direction is
    /// the sketch's normal).
    fn to_object_flip(
        &self,
        one: &Definition,
        raw: Option<&ExtrudeF3d>,
        sketch: i64,
    ) -> Option<bool> {
        let point = match one.entity.as_ref()? {
            Reference::Face(fp) => fp.point_on_face,
            Reference::ConstructionPlane(c) => c.geometry.as_ref().and_then(|g| g.origin),
            // A point the extent ends at (mitcad#1329).
            Reference::Vertex(fp) => fp.point,
            Reference::ConstructionPoint(c) => c.geometry.as_ref().and_then(|g| g.origin),
            _ => None,
        };
        let frame = self
            .doc
            .sketch_output(self.sketches[&sketch].uid)
            .map(|o| o.frame);
        if let (Some(p), Some(frame)) = (point, frame) {
            let s = geom::dot(geom::sub(geom::mm3(p), frame.origin), geom::normal(&frame));
            if s.abs() > 1e-6 {
                return Some(s < 0.0);
            }
        }
        let to_object = raw?
            .slot_roles
            .iter()
            .flatten()
            .any(|r| matches!(r, 17 | 18));
        raw?.flag.filter(|_| to_object).map(|f| f == 1)
    }

    /// The plane of the planar face an extent up to an object names
    /// (mitcad#96), as an extent up to that plane (mm), with the extent's
    /// offset.
    fn decoded_plane(&self, one: &Definition) -> Result<Option<Value>, String> {
        let Some(Reference::Face(fp)) = one.entity.as_ref() else {
            return Ok(None);
        };
        let Some(g) = fp
            .geometry
            .as_ref()
            .filter(|g| g.geometry_type.as_deref() == Some("Plane"))
        else {
            return Ok(None);
        };
        let (Some(origin), Some(normal)) = (g.origin, g.normal) else {
            return Ok(None);
        };
        let mut v = json!({"type": "to_object",
                           "object": {"type": "plane",
                                      "plane": {"origin": geom::mm3(origin), "normal": normal}}});
        if !Self::is_zero(&one.offset) {
            v["offset"] = self.value_or(&one.offset, "offset")?;
        }
        Ok(Some(v))
    }

    /// Whether two extent objects name the same face (a decoded one carries
    /// `chained`).
    fn same_object(a: &Value, b: &Value) -> bool {
        a["type"] == b["type"] && a["body"] == b["body"] && a["face"] == b["face"]
    }

    /// Whether the extrusion goes against its sketch's normal: by the
    /// decoded direction vector when there is one (it is stored in the
    /// component's coordinates, so it tells for sketches on any plane),
    /// else by the sign the stream decoder found (axis-aligned directions).
    fn decoded_flip(&self, raw: Option<&ExtrudeF3d>, sketch: FeatureUid) -> Option<bool> {
        let raw = raw?;
        // One side through all: the stored vector points away from the
        // extrusion (every such extrusion the history settled went the
        // other way).
        let away = raw.extent_a == Some(1) && raw.extent_b == Some(0);
        let normal = self
            .doc
            .sketch_output(sketch)
            .map(|o| geom::normal(&o.frame));
        if let (Some(v), Some(n)) = (raw.direction_vector, normal) {
            let along = geom::dot(v, n);
            // A stored vector oblique to the sketch's normal tells no side
            // (mitcad#104): every such extrusion the history settled went
            // along the normal (36 with distance extents, in sketches on
            // tilted planes), against the vector's side in 8 of them.
            if !away && along.abs() > 1e-6 && along.abs() < 1.0 - 1e-6 {
                return Some(false);
            }
            if along.abs() > 1e-6 {
                return Some((along < 0.0) != away);
            }
        }
        raw.direction.flatten().map(|v| (v < 0.0) != away)
    }

    /// The extrusions of faces ([`Importer::face_profiles`]) each with the
    /// participants the file names first, then the history's, as the
    /// extrusions of sketches have them
    /// ([`Importer::participant_options`]): a join that takes in every body it
    /// touches merges bodies the file's join leaves apart.
    fn with_decoded_participants(
        &mut self,
        index: i64,
        item: &TimelineItem,
        d: &ExtrudeDetail,
        faces: Vec<Candidate>,
    ) -> Vec<Candidate> {
        if faces.is_empty() {
            return faces;
        }
        let raw = item.f3d.as_ref().and_then(|f| f.extrude.as_ref());
        let operation = Self::operation(
            d.operation
                .as_deref()
                .or(raw.and_then(|r| r.operation.clone().flatten()).as_deref()),
        );
        let options = self.participant_options_in(
            index,
            &d.participant_bodies,
            operation,
            Some(self.component),
        );
        faces
            .into_iter()
            .flat_map(|c| {
                options
                    .iter()
                    .enumerate()
                    .map(|(i, participants)| {
                        let mut c = c.clone();
                        if i > 0 {
                            c.input_provenance = InputProvenance::Inferred;
                        }
                        if !participants.is_empty() {
                            for def in &mut c.defs {
                                if def["type"] == "extrude" {
                                    def["participants"] = json!(participants);
                                }
                            }
                        }
                        c
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// An extrusion's start as the model's `start`: an offset from the
    /// profile's plane, or an object (a face or plane) and an offset from
    /// it; `None` on the profile's plane.
    fn extrude_start(&self, d: &ExtrudeDetail) -> Result<Option<Value>, String> {
        Ok(match d.start_extent.as_ref() {
            Some(s) if s.definition_type.as_deref() == Some("OffsetStartDefinition") => {
                Some(json!({"type": "offset", "offset": self.value_or(&s.offset, "start offset")?}))
            }
            Some(s) if s.definition_type.as_deref() == Some("FromEntityStartDefinition") => {
                let object = self
                    .extent_object(&s.entity, s)
                    .ok_or("the start object was not found")?;
                let mut v = json!({"type": "object", "object": object});
                if !Self::is_zero(&s.offset) {
                    v["offset"] = self.value_or(&s.offset, "start offset")?;
                }
                Some(v)
            }
            _ => None,
        })
    }

    /// Extrusions of planar faces of bodies (.f3d designs extrude faces
    /// too): the faces the decoder found in the profile (`decoded`,
    /// mitcad#169), all together, else the faces of the item's component
    /// whose area times the distance is the volume the history state adds
    /// or removes, each on its own. Each face is a sketch on the face with
    /// its boundary projected (linked, so it follows the face) and its
    /// region extruded, outwards first for a new body or a join, inwards
    /// first for a cut, and for a new body the side where the state's new
    /// body is. Distance extents only. The faces found by their areas are
    /// guesses the history checks. `made`: the decoded faces' sketch made
    /// already ([`Importer::part_face_sketch`]).
    fn face_profiles(
        &mut self,
        index: i64,
        item: &TimelineItem,
        d: &ExtrudeDetail,
        decoded: &[(BodyUid, FaceName, f64)],
        faces_read: bool,
        made: Option<&FaceSketch>,
    ) -> Vec<Candidate> {
        if !self.oracle.enabled && decoded.is_empty() {
            return Vec::new();
        }
        let Some(one) = d.extent_one.as_ref() else {
            return Vec::new();
        };
        if d.extent_type
            .as_deref()
            .is_some_and(|k| !k.starts_with("OneSide"))
        {
            return Vec::new();
        }
        let raw = item.f3d.as_ref().and_then(|f| f.extrude.as_ref());
        let operation = Self::operation(
            d.operation
                .as_deref()
                .or(raw.and_then(|r| r.operation.clone().flatten()).as_deref()),
        );
        if operation == "intersect" {
            return Vec::new();
        }
        // The decoded faces go up to an object or through all too (their
        // extent as a sketch's regions take it, the side read where the
        // file gives it); the faces guessed by their prisms' volumes take a
        // distance only.
        if one.definition_type.as_deref() != Some("DistanceExtentDefinition") {
            return self.face_profiles_without_distance(index, d, raw, operation, decoded, made);
        }
        // A distance, its length known.
        let (length, extent_value): (Option<f64>, Value) = {
            let (Some(length), Ok(distance)) = (
                param(&one.distance)
                    .and_then(|p| p.value)
                    .map(|v| mm(v).abs()),
                self.value_or(&one.distance, "distance"),
            ) else {
                return Vec::new();
            };
            (Some(length), distance)
        };
        if length.is_some_and(|l| l < 1e-9) {
            return Vec::new();
        }
        // The history's change and the new body's centre (for the side of
        // a new body), where the history has them.
        let mut change = None;
        let mut new_body = None;
        if self.oracle.enabled {
            let state = self
                .oracle
                .item_state(index)
                .unwrap_or_else(|| self.oracle.next_index());
            if let Some(target) = self.oracle.try_sigs(self.doc.kernel(), state) {
                let current = self.current_sigs();
                let c = target.iter().map(|s| s.volume).sum::<f64>()
                    - current.iter().map(|s| s.volume).sum::<f64>();
                if c.abs() >= 1e-9 && (operation == "cut") == (c < 0.0) {
                    change = Some(c);
                    let mut used = vec![false; current.len()];
                    new_body = target
                        .iter()
                        .filter(|t| {
                            match (0..current.len()).find(|&j| !used[j] && current[j].same(t)) {
                                Some(j) => {
                                    used[j] = true;
                                    false
                                }
                                None => true,
                            }
                        })
                        .find(|t| ((t.volume - c.abs()) / c.abs()).abs() < 0.02)
                        .map(|t| t.center);
                }
            }
        }
        let sign = if operation == "cut" { -1.0 } else { 1.0 };
        // The extrusion's taper goes with the faces too; a tapered
        // extrusion is no prism, so its volume is not the area times the
        // length.
        let taper = self.taper_of(&d.taper_angle_one);
        let mut extent = if length.is_some() {
            json!({"type": "distance", "distance": extent_value})
        } else {
            extent_value.clone()
        };
        if length.is_some()
            && let Some(t) = &taper
        {
            extent["taper"] = t.clone();
        }
        let predicted = |area: f64| length.filter(|_| taper.is_none()).map(|l| sign * area * l);
        // Where the extrusion starts (mitcad#641: a face's prism that
        // starts on another face).
        let Ok(start) = self.extrude_start(d) else {
            return Vec::new();
        };
        let with_start = |mut def: Value| {
            if let Some(s) = &start {
                def["start"] = s.clone();
            }
            def
        };
        // A one-sided distance goes along the sketch's normal where the
        // stream's extent kinds say so (as for the sketch's regions).
        let distance_side_read = raw
            .and_then(|r| r.sides.as_deref())
            .and_then(|s| <[u32; 2]>::try_from(s).ok())
            == Some([1, 0]);
        // An extent up to an object goes towards the side of the profile's
        // plane the object lies on (as for the sketch's regions).
        let to_object_sides = raw
            .and_then(|r| r.sides.as_deref())
            .and_then(|s| <[u32; 2]>::try_from(s).ok())
            == Some([2, 0]);
        let object_point = match one.entity.as_ref() {
            Some(Reference::Face(fp)) if length.is_none() => fp.point_on_face.map(geom::mm3),
            _ => None,
        };
        let mut out = Vec::new();
        // The faces as decoded, all in one extrusion (mitcad#169).
        let sketch = match made {
            Some(m) => Some(m.clone()),
            None => (!decoded.is_empty())
                .then(|| self.face_sketch(index, decoded))
                .flatten(),
        };
        if let Some((sketch, keys, centre, normal)) = sketch {
            let area: f64 = decoded.iter().map(|(_, _, a)| a).sum();
            let mut flips = if operation == "cut" {
                [true, false]
            } else {
                [false, true]
            };
            if let (Some(c), Some(length)) = (new_body, length) {
                let at = |flip: bool| {
                    let s = if flip { -0.5 } else { 0.5 };
                    geom::distance(geom::add(centre, geom::scale(normal, s * length)), c)
                };
                if at(true) < at(false) {
                    flips = [true, false];
                }
            }
            // The bodies it joins or cuts as decoded first.
            let options = self.participant_options_in(
                index,
                &d.participant_bodies,
                operation,
                Some(self.component),
            );
            let side_read = |flip: bool| {
                faces_read
                    && ((distance_side_read && !flip)
                        || (to_object_sides
                            && object_point.is_some_and(|p| {
                                let s = geom::dot(geom::sub(p, centre), normal);
                                s.abs() > 1e-6 && (s < 0.0) == flip
                            })))
            };
            // One face by a distance from it: the face itself as the
            // profile (`profile_face`, mitcad#641), which follows the face
            // when the model changes as the file's face profile does; a
            // sketch of its boundary follows only while the face's edges
            // come back as they were.
            if let [(body, face, area)] = decoded
                && start.is_none()
                && length.is_some()
                && self.doc.body_component(*body) == Some(self.component)
            {
                out.extend(flips.into_iter().flat_map(|flip| {
                    let c = Candidate {
                        core: CandidateCore::of(vec![json!({"type": "extrude",
                            "profile_face": {"body": body.to_string(), "face": face.to_string()},
                            "extent": extent.clone(), "flip": flip, "operation": operation})]),
                        note: Some("the face of its profile as decoded".to_owned()),
                        guess: false,
                        predicted: predicted(*area),
                        input_provenance: if side_read(flip) {
                            InputProvenance::Read
                        } else {
                            InputProvenance::Inferred
                        },
                        first: true,
                    };
                    with_participants(&c, &options)
                }));
            }
            out.extend(flips.into_iter().flat_map(|flip| {
                let defs = on_face_sketch(&sketch, |on| {
                    let profiles: Vec<Value> = keys
                        .iter()
                        .map(|key| json!({"sketch": on, "region": key}))
                        .collect();
                    with_start(json!({"type": "extrude", "profiles": profiles,
                           "extent": extent.clone(),
                           "flip": flip, "operation": operation}))
                });
                let c = Candidate {
                    core: CandidateCore::of(defs),
                    note: Some("the faces of its profile as decoded".to_owned()),
                    guess: false,
                    predicted: predicted(area),
                    // The faces, the distance and the taper are decoded; the
                    // side is read from the stream's sides, else chosen by
                    // the history.
                    input_provenance: if side_read(flip) {
                        InputProvenance::Read
                    } else {
                        InputProvenance::Inferred
                    },
                    first: true,
                };
                with_participants(&c, &options)
            }));
        }
        let Some(change) = change else {
            return out;
        };
        // The faces found by their prisms' volumes are for untapered
        // extrusions by a distance.
        let Some(length) = length.filter(|_| taper.is_none()) else {
            return out;
        };
        let kernel = self.doc.kernel();
        // Faces by how close their prism comes to the change.
        let mut faces: Vec<(f64, BodyUid, FaceName, f64)> = Vec::new();
        for body in crate::body_scope::bodies(self.doc) {
            if self.doc.body_component(body.uid) != Some(self.component) {
                continue;
            }
            // (A kernel call per body, mitcad#82.)
            crate::tick();
            let Ok(list) = kernel.faces(body.shape) else {
                continue;
            };
            for f in list.iter().filter(|f| f.surface == "plane") {
                let Some(name) = f.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
                    continue;
                };
                let off = (f.area * length - change.abs()).abs() / change.abs();
                if off < 0.02 {
                    faces.push((off, body.uid, name, f.area));
                }
            }
        }
        faces.sort_by(|a, b| a.0.total_cmp(&b.0));
        faces.truncate(6);
        for (_, body, face, area) in faces {
            let Some((sketch, keys, centre, normal)) =
                self.face_sketch(index, &[(body, face.clone(), area)])
            else {
                continue;
            };
            let profiles: Vec<Value> = keys
                .iter()
                .map(|key| json!({"sketch": "$0", "region": key}))
                .collect();
            // Outwards (along the face's normal) first, except for a cut.
            let mut flips = if operation == "cut" {
                [true, false]
            } else {
                [false, true]
            };
            if let Some(c) = new_body {
                let at = |flip: bool| {
                    let s = if flip { -0.5 } else { 0.5 };
                    geom::distance(geom::add(centre, geom::scale(normal, s * length)), c)
                };
                if at(true) < at(false) {
                    flips = [true, false];
                }
            }
            for flip in flips {
                out.push(Candidate {
                    core: CandidateCore::of(vec![
                        sketch.clone(),
                        with_start(json!({"type": "extrude", "profiles": profiles,
                               "extent": {"type": "distance", "distance": extent_value},
                               "flip": flip, "operation": operation})),
                    ]),
                    note: Some(format!(
                        "profile not decoded: face {face} of {body} checked against the history"
                    )),
                    guess: true,
                    predicted: Some(sign * area * length),
                    input_provenance: InputProvenance::Inferred,
                    first: false,
                });
            }
        }
        out
    }

    /// The faces an extrusion's profile selects (as decoded, mitcad#169)
    /// through all or up to an object: one sketch on them as for a
    /// distance, the extent as the file's (up to a planar face the replay
    /// does not have as it is, its plane), with the bodies it joins or cuts
    /// as decoded first. The side read first: through all, its direction
    /// flag (against the faces' normal when negative), up to an object, the
    /// side of the faces' plane the object lies on; the other side after it.
    fn face_profiles_without_distance(
        &mut self,
        index: i64,
        d: &ExtrudeDetail,
        raw: Option<&ExtrudeF3d>,
        operation: &str,
        decoded: &[(BodyUid, FaceName, f64)],
        made: Option<&FaceSketch>,
    ) -> Vec<Candidate> {
        let Some(one) = d.extent_one.as_ref() else {
            return Vec::new();
        };
        if decoded.is_empty() {
            return Vec::new();
        }
        let extent = match self.side(one, &d.taper_angle_one) {
            Ok(Some((v, flip))) => Some((v, flip)),
            Ok(None) => None,
            Err(_) => self.decoded_plane(one).ok().flatten().map(|mut v| {
                if let Some(t) = self.taper_of(&d.taper_angle_one) {
                    v["taper"] = t;
                }
                (v, None)
            }),
        };
        let Some((extent, flag_flip)) = extent else {
            return Vec::new();
        };
        let made = match made {
            Some(m) => Some(m.clone()),
            None => self.face_sketch(index, decoded),
        };
        let Some((sketch, keys, centre, normal)) = made else {
            return Vec::new();
        };
        let read_flip = match extent["type"].as_str() {
            Some("through_all") => flag_flip,
            Some("to_object") => {
                let point = match one.entity.as_ref() {
                    Some(Reference::Face(fp)) => fp.point_on_face,
                    Some(Reference::ConstructionPlane(c)) => {
                        c.geometry.as_ref().and_then(|g| g.origin)
                    }
                    _ => None,
                };
                point
                    .map(|p| geom::dot(geom::sub(geom::mm3(p), centre), normal))
                    .filter(|s| s.abs() > 1e-6)
                    .map(|s| s < 0.0)
                    .or_else(|| {
                        let to_object = raw?
                            .slot_roles
                            .iter()
                            .flatten()
                            .any(|r| matches!(r, 17 | 18));
                        raw?.flag.filter(|_| to_object).map(|f| f == 1)
                    })
            }
            _ => None,
        };
        let first = read_flip.unwrap_or(operation == "cut");
        let options = self.participant_options_in(
            index,
            &d.participant_bodies,
            operation,
            Some(self.component),
        );
        let mut out = Vec::new();
        for flip in [first, !first] {
            let defs = on_face_sketch(&sketch, |on| {
                let profiles: Vec<Value> = keys
                    .iter()
                    .map(|key| json!({"sketch": on, "region": key}))
                    .collect();
                let mut def = json!({"type": "extrude", "profiles": profiles, "extent": extent,
                                     "flip": flip, "operation": operation});
                if let Ok(links) = self.object_links(self.component, &def)
                    && !links.is_empty()
                {
                    def["object_links"] = Value::Object(links);
                }
                def
            });
            let read = read_flip == Some(flip);
            let c = Candidate {
                core: CandidateCore::of(defs),
                note: Some("the faces of its profile as decoded".to_owned()),
                guess: !read,
                predicted: None,
                input_provenance: if read {
                    InputProvenance::Read
                } else {
                    InputProvenance::Inferred
                },
                first: true,
            };
            out.extend(with_participants(&c, &options));
        }
        out
    }

    /// A sketch on the first of `faces` (planar faces of bodies of the
    /// item's component) with the faces' boundaries projected (linked), as
    /// a definition, with the keys of its regions on the faces, the first
    /// one's centre and the sketch's normal; None when the regions on the
    /// faces do not make up their area (a face not in the first one's
    /// plane, say).
    fn face_sketch(
        &mut self,
        index: i64,
        faces: &[(BodyUid, FaceName, f64)],
    ) -> Option<FaceSketch> {
        let depth = self.doc.undo_depth();
        let sketch = self
            .make_face_sketch(index, faces)
            .and_then(|(uid, keys, centre, normal)| {
                let params = self.doc.parameters();
                let def = self
                    .doc
                    .feature(uid)?
                    .def
                    .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                    .ok()?;
                Some((serde_json::to_value(&def).ok()?, keys, centre, normal))
            });
        self.undo_to(depth);
        sketch
    }

    /// The sketch of [`Importer::face_sketch`] added at the marker: its
    /// uid, the keys of its regions on the faces, the first one's centre
    /// and the sketch's normal. A face of another component's body is the
    /// sketch's plane through that component's occurrence (`plane_link`,
    /// one occurrence only), as its projections are. None (perhaps with a
    /// feature added) when it does not come out.
    fn make_face_sketch(
        &mut self,
        index: i64,
        faces: &[(BodyUid, FaceName, f64)],
    ) -> Option<MadeFaceSketch> {
        let (body, face, _) = faces.first()?;
        let mut plane = json!({"type": "sketch", "plane": {"face": face.to_string(), "body": body.to_string()}});
        if let Some(owner) = self
            .doc
            .body_component(*body)
            .filter(|c| *c != self.component)
        {
            let paths = self.doc.assembly().paths_to(owner);
            let [path] = paths.as_slice() else {
                return None;
            };
            plane["plane_link"] = linked::link(path, &self.sketch_target()?);
        }
        let uid = self.add(&plane, None).ok()?;
        self.project_faces(uid, faces)?;
        let keys = self.regions_on_faces(index, uid, faces)?;
        let output = self.doc.sketch_output(uid)?;
        let first = output
            .region_info
            .iter()
            .find(|r| r.profile.key.to_string() == keys[0])?;
        let centre = output.frame.point(first.centroid);
        let normal = geom::normal(&output.frame);
        Some((uid, keys, centre, normal))
    }

    /// The profile faces of a part of an item in another component
    /// (`_f3d.part_of`, mitcad#169) that the replay no longer has: faces of
    /// a body the item's own operation took away (a cut from a face of the
    /// item's own body that goes on into another component's body,
    /// mitcad#1347). The file makes such a feature's tool once, before any
    /// of its operations, so the part takes its profile where the item's
    /// operation stands: with the timeline marker before the item's last
    /// feature, the faces as the profile names them, whether they are read
    /// (each the only face that fits), and a sketch on them made there. The
    /// sketch stays in the timeline, before the item's operation, as one of
    /// the part's features; the part's extrusion of its regions comes after
    /// the item's. None where the faces are not there either.
    fn part_face_sketch(
        &mut self,
        index: i64,
        item: &TimelineItem,
        face_refs: &[Reference],
        participants: &[BodyUid],
    ) -> Option<PartFaceSketch> {
        let of = item.f3d.as_ref()?.part_of?;
        // (Made already, when the part is tried again.)
        if let Some(made) = self.part_sketches.get(&index).filter(|m| {
            m.2.0
                .as_str()
                .and_then(|u| u.parse::<FeatureUid>().ok())
                .is_some_and(|u| self.doc.feature(u).is_some())
        }) {
            return Some(made.clone());
        }
        if self.doc.marker() != self.doc.features().count() {
            return None;
        }
        let master: Vec<FeatureUid> = self
            .report
            .items
            .iter()
            .find(|r| r.index == of)?
            .features
            .iter()
            .filter_map(|f| f.parse().ok())
            .collect();
        let at = self
            .doc
            .features()
            .enumerate()
            .filter(|(_, f)| master.contains(&f.uid))
            .map(|(i, _)| i)
            .max()?;
        let depth = self.doc.undo_depth();
        if self.doc.set_marker(at).is_err() {
            self.undo_to(depth);
            return None;
        }
        let made = self.part_face_sketch_here(index, face_refs, participants);
        let end = self.doc.features().count();
        let Some(made) = made.filter(|_| self.doc.set_marker(end).is_ok()) else {
            self.undo_to(depth);
            return None;
        };
        if crate::tracing() {
            eprintln!(
                "import: item {index}: its profile taken before item {of}'s operation (faces {}), sketched as {}",
                made.0
                    .iter()
                    .map(|(b, f, _)| format!("{f} of {b}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                made.2.0
            );
        }
        if let (Some(r), Some(uid)) = (
            self.report.items.iter_mut().find(|r| r.index == index),
            made.2.0.as_str(),
        ) {
            r.features.push(uid.to_owned());
        }
        self.part_sketches.insert(index, made.clone());
        Some(made)
    }

    /// [`Importer::part_face_sketch`] at the marker as it is.
    fn part_face_sketch_here(
        &mut self,
        index: i64,
        face_refs: &[Reference],
        participants: &[BodyUid],
    ) -> Option<PartFaceSketch> {
        let faces = self.profile_faces(face_refs, participants);
        if faces.is_empty() {
            return None;
        }
        let read = faces.iter().zip(face_refs).all(|((body, face, _), r)| {
            matches!(r, Reference::Face(fp)
                    if refs::face_resolved_uniquely(self.doc, fp, (*body, face)))
        });
        let (uid, keys, centre, normal) = self.make_face_sketch(index, &faces)?;
        Some((faces, read, (json!(uid.to_string()), keys, centre, normal)))
    }

    fn revolve(&mut self, index: i64, d: &RevolveDetail) -> Result<Vec<Candidate>, String> {
        // A surface revolution of regions (mitcad#455).
        let surface = d.is_solid == Some(false);
        let profiles = profiles_of(&d.profile, &d.other);
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        let mut sets = self.region_sets(&profiles, sketch, None, None)?;
        // The regions the decoded profile loops select first (mitcad#96).
        let mapped = self.mapped_first(&mut sets, &d.other, sketch);
        let info = self.sketches[&sketch].clone();
        let axes = self.revolve_axes(&d.axis, &info)?;
        let operation = Self::operation(d.operation.as_deref());
        let participant_options =
            self.participant_options(index, &d.participant_bodies, operation, info.uid);
        let ext = d
            .extent_definition
            .clone()
            .or(d.extent_one.clone())
            .unwrap_or_default();
        let mut extents: Vec<(Value, bool)> = Vec::new();
        // The extent is read when the stream says whether it is symmetric
        // (two angles, or symmetric) and, for one angle, no direction is
        // needed: a full turn.
        let extent_read;
        match ext.definition_type.as_deref() {
            Some("AngleExtentDefinition") | None if ext.angle.is_some() => {
                let angle = self.value_or(&ext.angle, "angle")?;
                if ext.is_symmetric == Some(true) {
                    extents.push((json!({"type": "symmetric", "angle": angle}), false));
                    extent_read = true;
                } else {
                    let full = param(&ext.angle)
                        .and_then(|p| p.value)
                        .is_some_and(|a| (a.abs() - std::f64::consts::TAU).abs() < 1e-9);
                    extents.push((json!({"type": "angle", "angle": angle}), false));
                    extent_read = full && ext.is_symmetric == Some(false);
                    if !full {
                        // The direction is not decoded.
                        if let Some(name) = angle.as_str() {
                            extents.push((
                                json!({"type": "angle", "angle": format!("-{name}")}),
                                true,
                            ));
                        } else if let Some(v) = angle.as_f64() {
                            extents.push((json!({"type": "angle", "angle": -v}), true));
                        }
                        if ext.is_symmetric.is_none() {
                            extents.push((json!({"type": "symmetric", "angle": angle}), true));
                        }
                    }
                }
            }
            Some(_) if ext.angle_one.is_some() => {
                extents.push((
                    json!({"type": "two_sides",
                    "angle1": self.value_or(&ext.angle_one, "angle")?,
                    "angle2": self.value_or(&ext.angle_two, "angle")?}),
                    false,
                ));
                extent_read = d.is_solid.is_some();
            }
            Some(other) => return Err(format!("{other} is not supported")),
            None => return Err("its extent was not decoded".to_owned()),
        }
        // The swept angle of each extent, for volume estimates.
        let angle_of = |e: &Value| -> Option<f64> {
            let value = |v: &Value| match v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => {
                    let name = s.trim_start_matches('-');
                    let id = self.doc.parameters().find(name)?;
                    self.doc.parameters().get(id).map(|p| p.value())
                }
                _ => None,
            };
            match e["type"].as_str()? {
                "angle" => value(&e["angle"]),
                "symmetric" => value(&e["angle"]).map(|a| 2.0 * a),
                "two_sides" => Some(value(&e["angle1"])? + value(&e["angle2"])?),
                "full" => Some(std::f64::consts::TAU),
                _ => None,
            }
        };
        let sign = match operation {
            "cut" => Some(-1.0),
            "intersect" => None,
            _ => Some(1.0),
        };
        if crate::tracing() {
            eprintln!(
                "import: item {index}: revolve read: last_sketch={last_sketch} extent_read={extent_read} solid={:?} axis={} participants={} mapped={mapped} sets={:?}",
                d.is_solid,
                d.axis.is_some(),
                operation == "new_body" || !self.participants(&d.participant_bodies).is_empty(),
                sets.iter().map(|s| (s.exact, s.guess)).collect::<Vec<_>>()
            );
        }
        let mut candidates = Vec::new();
        for (k, set) in sets.iter().enumerate() {
            for (axis, axis_guess) in &axes {
                let line = self.axis_line(axis, &info);
                for (extent, extent_guess) in &extents {
                    let profiles: Vec<Value> = set
                        .keys
                        .iter()
                        .map(|r| json!({"sketch": info.uid.to_string(), "region": r}))
                        .collect();
                    let mut def = json!({"type": "revolve", "profiles": profiles, "axis": axis,
                                         "extent": extent, "operation": operation});
                    if d.is_project_axis == Some(true) {
                        def["project_axis"] = json!(true);
                    }
                    let mut c = Candidate::new(def);
                    c.predicted = line
                        .zip(angle_of(extent))
                        .zip(sign)
                        .map(|(((a, d), angle), s)| s * set.turned(a, d, angle));
                    c.guess = set.guess || *axis_guess || *extent_guess;
                    if last_sketch || c.guess {
                        c.input_provenance = InputProvenance::Inferred;
                    } else if mapped
                        && k == 0
                        && set.exact
                        && extent_read
                        && d.is_solid.is_some()
                        && d.axis.is_some()
                        && (operation == "new_body"
                            || !self.participants(&d.participant_bodies).is_empty())
                    {
                        // Profiles, axis (by the stream's own references),
                        // operation, bodies and angles are all read.
                        c.input_provenance = InputProvenance::Read;
                    }
                    // Profiles, axis and extent as decoded (mitcad#96).
                    c.first = mapped && k == 0 && !*axis_guess && !*extent_guess;
                    if last_sketch {
                        c.note = Some("the sketch is the last one before it".to_owned());
                    } else if mapped && k == 0 {
                        c.note = Some("the profiles as decoded".to_owned());
                    }
                    candidates.extend(with_participants(&c, &participant_options));
                }
            }
        }
        Ok(if surface {
            surface_candidates(candidates)
        } else {
            candidates
        })
    }

    /// An emboss (mitcad#458): MitCad's emboss of the decoded profiles on
    /// the face the file names, sunk where the file's depth is negative.
    /// Profiles moved or turned on the face (the item's horizontal and
    /// vertical distances and rotation) have no counterpart.
    fn emboss(&mut self, index: i64, d: &EmbossDetail) -> Result<Vec<Candidate>, String> {
        for (what, r) in [
            ("moved across the face", &d.horizontal_distance),
            ("moved along the face", &d.vertical_distance),
            ("turned on the face", &d.rotation_angle),
        ] {
            if param(r)
                .and_then(|p| p.value)
                .is_some_and(|v| v.abs() > 1e-12)
            {
                return Err(format!(
                    "its profiles are {what}, which MitCad's emboss has no option for"
                ));
            }
        }
        let value = param(&d.depth)
            .and_then(|p| p.value)
            .ok_or("its depth was not decoded")?;
        if value == 0.0 {
            return Err("its depth is zero".to_owned());
        }
        let sink = value < 0.0;
        let depth = match self.value_or(&d.depth, "depth")? {
            Value::String(s) if sink => json!(format!("-({s})")),
            Value::Number(n) if sink => json!(-n.as_f64().unwrap_or_default()),
            v => v,
        };
        let Some([Reference::Face(fp)]) = d.input_faces.as_deref() else {
            return Err("its face was not decoded".to_owned());
        };
        let (body, face) = self.face_input(fp).ok_or("its face was not found")?;
        let profiles = d.profile.clone().unwrap_or_default();
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        let mut sets = self.region_sets(&profiles, sketch, None, None)?;
        // The regions the decoded profile loops select first.
        let mapped = self.mapped_first(&mut sets, &d.other, sketch);
        let info = self.sketches[&sketch].clone();
        let mut candidates = Vec::new();
        for (k, set) in sets.iter().enumerate() {
            let profiles: Vec<Value> = set
                .keys
                .iter()
                .map(|r| json!({"sketch": info.uid.to_string(), "region": r}))
                .collect();
            let mut def = json!({"type": "emboss", "profiles": profiles,
                "face": {"body": body.to_string(), "face": face.to_string()}, "depth": depth});
            if sink {
                def["mode"] = json!("sink");
            }
            let mut c = Candidate::new(def);
            c.guess = set.guess;
            c.first = mapped && k == 0;
            if last_sketch || c.guess {
                c.input_provenance = InputProvenance::Inferred;
            } else if mapped && k == 0 && set.exact {
                // Profiles, face and depth are all read.
                c.input_provenance = InputProvenance::Read;
            }
            if last_sketch {
                c.note = Some("the sketch is the last one before it".to_owned());
            } else if mapped && k == 0 {
                c.note = Some("the profiles as decoded".to_owned());
            }
            candidates.push(c);
        }
        Ok(candidates)
    }

    /// A coil that sweeps profiles of a sketch along a helix about an axis
    /// (the `.ipt` import's coils: `profile` and `axis` in the detail),
    /// as MitCad's helix: the pitch and the turns (a height divided by the
    /// pitch), right-handed and along the axis first (against it where
    /// the detail's `flip` says so), then the other hand and direction as
    /// guesses.
    fn coil_helix(&mut self, index: i64, d: &CoilDetail) -> Result<Vec<Candidate>, String> {
        let profiles: Vec<Reference> = d
            .other
            .get("profile")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_object().map(|m| Reference::from_map(m.clone())))
            .collect();
        if profiles.is_empty() {
            return Err("its profiles were not decoded".to_owned());
        }
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        let sets = self.region_sets(&profiles, sketch, None, None)?;
        let info = self.sketches[&sketch].clone();
        let axis = d
            .other
            .get("axis")
            .and_then(Value::as_object)
            .map(|m| Reference::from_map(m.clone()));
        let axes = self.revolve_axes(&axis, &info)?;
        let operation = Self::operation(d.operation.as_deref());
        // The bodies the file names (mitcad#235): without them all bodies
        // came first, and on a body of hundreds of faces that try alone
        // ran out of memory.
        let participant_options =
            self.participant_options(index, &d.participant_bodies, operation, info.uid);
        // A quotient of two parameters: by their names, else their values.
        let ratio = |a: &Option<Reference>, b: &Option<Reference>| -> Option<Value> {
            match (self.value(a)?, self.value(b)?) {
                (Value::String(x), Value::String(y)) => Some(json!(format!("{x} / {y}"))),
                _ => {
                    let (x, y) = (param(a)?.value?, param(b)?.value?);
                    (y != 0.0).then(|| json!(x / y))
                }
            }
        };
        let (pitch, revolutions) = match d.coil_type.as_deref() {
            Some("PitchAndRevolutionCoilType") => (
                self.value_or(&d.pitch, "pitch")?,
                self.value_or(&d.revolutions, "revolutions")?,
            ),
            Some("PitchAndHeightCoilType") => (
                self.value_or(&d.pitch, "pitch")?,
                ratio(&d.height, &d.pitch).ok_or("no height or pitch")?,
            ),
            Some("RevolutionAndHeightCoilType") => (
                ratio(&d.height, &d.revolutions).ok_or("no height or turns")?,
                self.value_or(&d.revolutions, "revolutions")?,
            ),
            other => return Err(format!("coil type {other:?} is not supported")),
        };
        // A taper (mitcad#235): the coil on a cone of that half angle, the
        // profiles moving out (or in) by the pitch times its tangent for
        // every pitch they rise; which way is not decoded, out first.
        let growths: Vec<Option<Value>> = if Self::is_zero(&d.angle) {
            vec![None]
        } else {
            let angle = self.value_or(&d.angle, "taper")?;
            let tangent = param(&d.angle).and_then(|p| p.value).map(f64::tan);
            [1.0, -1.0]
                .into_iter()
                .map(|sign: f64| match (&pitch, &angle) {
                    (Value::String(p), Value::String(a)) => Some(json!(format!(
                        "{}{p} * tan({a})",
                        if sign < 0.0 { "-" } else { "" }
                    ))),
                    _ => Some(json!(sign * pitch.as_f64()? * tangent?)),
                })
                .collect()
        };
        if growths.iter().any(Option::is_none) && growths.len() > 1 {
            return Err("the taper's growth could not be given".to_owned());
        }
        // Flat ends (mitcad#235): their transition and flat angles. MitCad's
        // turns are those at the full pitch; the file's may count the ends
        // too (not decoded): as given first, then less the ends' angles.
        let end_of = |key: &str| -> Option<Value> {
            let e = d.other.get(key)?.as_object()?;
            let r = |k: &str| {
                e.get(k)
                    .and_then(Value::as_object)
                    .map(|m| Reference::from_map(m.clone()))
            };
            Some(
                json!({"transition_angle": self.value(&r("transition"))?, "flat_angle": self.value(&r("flat"))?}),
            )
        };
        let (start_end, end_end) = (end_of("startEnd"), end_of("endEnd"));
        let mut turn_options = vec![revolutions.clone()];
        if start_end.is_some() || end_end.is_some() {
            let angles: f64 = ["startEnd", "endEnd"]
                .iter()
                .filter_map(|k| d.other.get(*k))
                .flat_map(|e| ["transition", "flat"].map(|k| e[k]["value"].as_f64()))
                .map(|v| v.unwrap_or(0.0))
                .sum();
            let given = match &revolutions {
                Value::String(_) => None,
                v => v.as_f64(),
            }
            .or_else(|| {
                (d.coil_type.as_deref() == Some("PitchAndRevolutionCoilType"))
                    .then(|| param(&d.revolutions).and_then(|p| p.value))
                    .flatten()
            });
            if let Some(n) = given {
                let fewer = n - angles / std::f64::consts::TAU;
                if fewer > 0.0 {
                    turn_options.push(json!(fewer));
                }
            }
        }
        // The direction along the axis the decoder read (`flip`) first.
        let reversed = d.other.get("flip").and_then(Value::as_bool) == Some(true);
        let mut candidates = Vec::new();
        for set in &sets {
            for (axis, axis_guess) in &axes {
                for (g, growth) in growths.iter().enumerate() {
                    for (n, turns) in turn_options.iter().enumerate() {
                        for (k, (left_handed, flip)) in [
                            (false, reversed),
                            (true, reversed),
                            (false, !reversed),
                            (true, !reversed),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let profiles: Vec<Value> = set
                                .keys
                                .iter()
                                .map(|r| json!({"sketch": info.uid.to_string(), "region": r}))
                                .collect();
                            let mut def = json!({"type": "helix", "profiles": profiles, "axis": axis,
                        "pitch": pitch, "revolutions": turns,
                        "left_handed": left_handed, "flip": flip, "operation": operation});
                            if let Some(growth) = growth {
                                def["growth"] = growth.clone();
                                def["construction"] = json!("mitcad");
                            }
                            if let Some(e) = &start_end {
                                def["start"] = e.clone();
                            }
                            if let Some(e) = &end_end {
                                def["end"] = e.clone();
                            }
                            let mut c = Candidate::new(def);
                            c.guess = set.guess || *axis_guess || k > 0 || g > 0 || n > 0;
                            if last_sketch || c.guess {
                                c.input_provenance = InputProvenance::Inferred;
                            }
                            if last_sketch {
                                c.note = Some("the sketch is the last one before it".to_owned());
                            }
                            candidates.extend(with_participants(&c, &participant_options));
                        }
                    }
                }
            }
        }
        // The coil as decoded once more, right after it, with the seams of
        // the helix's pieces shifted (mitcad#235): OCCT's cut or join can
        // fail or leave part of the coil uncut where those seams meet the
        // seams of the body's faces about the axis. A coil that gives the
        // file's state as built first never gets to it. (Only the decoded
        // definitions, not the guesses, so that the guesses keep their
        // places among the candidates tried; the retry itself is a guess,
        // tried after the others where guesses come last.)
        let mut with_shifted = Vec::with_capacity(candidates.len());
        for c in candidates {
            let shifted = (!c.guess).then(|| {
                let mut s = c.clone();
                s.guess = true;
                s.input_provenance = InputProvenance::Inferred;
                for def in s.defs.iter_mut().filter(|d| d["type"] == "helix") {
                    def["piece_shift"] = json!(COIL_PIECE_SHIFT);
                }
                let shift = "the seams of the helix's pieces shifted by a sixth of a turn";
                s.note = Some(match s.note.take() {
                    Some(n) => format!("{n}; {shift}"),
                    None => shift.to_owned(),
                });
                s
            });
            with_shifted.push(c);
            with_shifted.extend(shifted);
        }
        Ok(with_shifted)
    }

    /// A revolve axis as a line of the sketch plane (a point and a unit
    /// direction, sketch mm): a sketch line, or an origin axis that lies
    /// in the plane.
    fn axis_line(&self, axis: &Value, sketch: &SketchInfo) -> Option<([f64; 2], [f64; 2])> {
        let output = self.doc.sketch_output(sketch.uid)?;
        let frame = output.frame;
        let unit2 = |d: [f64; 2]| {
            let n = d[0].hypot(d[1]);
            (n > 1e-12).then(|| [d[0] / n, d[1] / n])
        };
        match axis {
            Value::String(o) => {
                let dir = match o.as_str() {
                    "x" => [1.0, 0.0, 0.0],
                    "y" => [0.0, 1.0, 0.0],
                    "z" => [0.0, 0.0, 1.0],
                    _ => return None,
                };
                let normal = geom::normal(&frame);
                let in_plane = geom::dot(dir, normal).abs() < 1e-9
                    && geom::dot(geom::sub([0.0; 3], frame.origin), normal).abs() < 1e-6;
                if !in_plane {
                    return None;
                }
                let a = geom::to_sketch(&frame, [0.0; 3]);
                let d = unit2([geom::dot(dir, frame.x_axis), geom::dot(dir, frame.y_axis)])?;
                Some((a, d))
            }
            Value::Object(m) => {
                let curve = m.get("curve")?.as_str()?.strip_prefix('c')?.parse().ok()?;
                // A line of another sketch (mitcad#96): where it lies in
                // this sketch's plane.
                let own = sketch.uid.to_string();
                let other = match m.get("sketch").and_then(Value::as_str) {
                    Some(uid) if uid != own => Some(self.doc.sketch_output(uid.parse().ok()?)?),
                    _ => None,
                };
                let of = other.as_ref().unwrap_or(&output);
                let (a, b) = match of.solved.curves.get(&mitcad_model::EntityUid(curve))? {
                    mitcad_model::sketch::geometry::Curve2::Line { a, b } => (*a, *b),
                    _ => return None,
                };
                let (a, b) = match &other {
                    Some(o) => {
                        let (p, q) = (o.frame.point(a), o.frame.point(b));
                        let normal = geom::normal(&frame);
                        let off = |x| geom::dot(geom::sub(x, frame.origin), normal).abs();
                        if off(p) > 1e-6 || off(q) > 1e-6 {
                            return None;
                        }
                        (geom::to_sketch(&frame, p), geom::to_sketch(&frame, q))
                    }
                    None => (a, b),
                };
                Some((a, unit2([b[0] - a[0], b[1] - a[1]])?))
            }
            _ => None,
        }
    }

    /// The revolve axis: an origin or construction axis, a sketch line or
    /// an edge; without one decoded, the sketch's lines (centre lines
    /// first) and the origin axes, all guesses.
    fn revolve_axes(
        &self,
        axis: &Option<Reference>,
        sketch: &SketchInfo,
    ) -> Result<Vec<(Value, bool)>, String> {
        match axis {
            Some(Reference::ConstructionAxis(c)) => {
                if let Some(o) = c.origin.clone().flatten() {
                    return match o.as_str() {
                        "X" | "Y" | "Z" => Ok(vec![(json!(o.to_lowercase()), false)]),
                        other => Err(format!("axis {other}")),
                    };
                }
                // An axis off the timeline by its geometry (an `.ipt`
                // part's work axis, as its planes take them): fixed where
                // it is, then the guesses.
                if c.timeline_index.flatten().is_none()
                    && let Some((o, v)) = c
                        .geometry
                        .as_ref()
                        .and_then(|g| Some((mm3(g.origin?), geom::unit(g.direction?)?)))
                {
                    let fixed = json!({"origin": o, "direction": v});
                    let mut axes = vec![(fixed, false)];
                    axes.extend(self.revolve_axes(&None, sketch)?);
                    return Ok(axes);
                }
                let uid = c
                    .timeline_index
                    .flatten()
                    .and_then(|i| self.features.get(&i))
                    .ok_or("its construction axis was not imported")?;
                Ok(vec![(json!(uid.to_string()), false)])
            }
            Some(Reference::SketchEntity(e)) => {
                let s = e
                    .sketch_timeline_index
                    .flatten()
                    .and_then(|i| self.sketches.get(&i));
                let s = s.ok_or("the axis' sketch was not imported")?;
                let id = e.id.clone().flatten().ok_or("the axis line has no id")?;
                let curve = s.ids.get(&id).ok_or("the axis line was left out")?;
                // Read from the axis input's curve ids (mitcad#96): the
                // guesses stay after it.
                let decoded = json!({"sketch": s.uid.to_string(), "curve": curve});
                let mut axes = vec![(decoded.clone(), false)];
                axes.extend(
                    self.revolve_axes(&None, sketch)?
                        .into_iter()
                        .filter(|(a, _)| *a != decoded),
                );
                Ok(axes)
            }
            // A cylindrical or conical face (the API reference dumps' six
            // revolutions about a face): its axis where the face was found,
            // fixed there, the guesses after it.
            Some(Reference::Face(fp)) => {
                let (o, v) = fp
                    .geometry
                    .as_ref()
                    .filter(|g| matches!(g.geometry_type.as_deref(), Some("Cylinder" | "Cone")))
                    .and_then(|g| Some((mm3(g.origin?), geom::unit(g.axis?)?)))
                    .ok_or("the axis face was not found or is not round")?;
                let fixed = json!({"origin": o, "direction": v});
                let mut axes = vec![(fixed, false)];
                axes.extend(self.revolve_axes(&None, sketch)?);
                Ok(axes)
            }
            Some(Reference::Edge(fp)) => {
                let (body, edge) =
                    refs::resolve_edge(self.doc, fp).ok_or("the axis edge was not found")?;
                Ok(vec![(
                    json!({"body": body.to_string(), "edge": edge.to_string()}),
                    false,
                )])
            }
            Some(_) => Err("the axis is not supported".to_owned()),
            None => {
                let mut axes = Vec::new();
                if let Some(def) = self.doc.feature(sketch.uid)
                    && let mitcad_model::FeatureDef::Sketch(s) = &def.def
                {
                    let lines: Vec<_> = s
                        .entities
                        .iter()
                        .filter(|e| matches!(e.kind, mitcad_model::sketch::EntityKind::Line { .. }))
                        .collect();
                    let centre = |e: &&mitcad_model::sketch::Entity| match e.kind {
                        mitcad_model::sketch::EntityKind::Line { centerline, .. } => !centerline,
                        _ => true,
                    };
                    let mut lines = lines;
                    lines.sort_by_key(centre);
                    for e in lines.iter().take(12) {
                        axes.push((json!({"sketch": sketch.uid.to_string(), "curve": format!("c{}", e.id.0)}), true));
                    }
                }
                for a in ["x", "y", "z"] {
                    axes.push((json!(a), true));
                }
                Ok(axes)
            }
        }
    }

    // Fillets and chamfers.

    pub fn fillet(&mut self, d: &FilletDetail) -> Result<Vec<Candidate>, String> {
        let sets = d.edge_sets.clone().unwrap_or_default();
        if sets.is_empty() {
            return Err("no edge sets decoded".to_owned());
        }
        let curvature = |s: &EdgeSet| {
            d.is_g2 == Some(true)
                || s.continuity
                    .as_deref()
                    .is_some_and(|c| c.starts_with("Curvature"))
        };
        let value = |r: &Option<Reference>| param(r).and_then(|p| p.value).map(mm);
        // Per set: its size, the options next to it, and the cross section
        // the history's edges are matched with (a variable one by its mean
        // radius, an asymmetric one by its contact distances).
        let mut sizes = Vec::new();
        let mut options = Vec::new();
        let mut sections: Vec<Vec<Section>> = Vec::new();
        for s in &sets {
            let mut extra = serde_json::Map::new();
            let size = match s.edge_set_type.as_deref() {
                Some("ChordLengthFilletEdgeSet") => {
                    sections.push(vec![Section::Fillet(value(&s.radius).unwrap_or(1.0))]);
                    json!({"type": "chord_length", "length": self.value_or(&s.other_ref("chordLength"), "chord length")?})
                }
                Some("VariableRadiusFilletEdgeSet") => {
                    let mut size = json!({"type": "variable",
                        "start": self.value_or(&s.start_radius, "start radius")?,
                        "end": self.value_or(&s.end_radius, "end radius")?});
                    let radii = s.mid_radii.clone().unwrap_or_default();
                    // Positions as parameters, or as plain numbers.
                    let positions: Vec<Option<f64>> = match &s.mid_positions {
                        Some(refs) => refs
                            .iter()
                            .map(|r| r.parameter().and_then(|p| p.value))
                            .collect(),
                        None => s
                            .other
                            .get("midPositions")
                            .and_then(Value::as_array)
                            .map(|a| a.iter().map(Value::as_f64).collect())
                            .unwrap_or_default(),
                    };
                    if radii.len() != positions.len() {
                        return Err("the mid radii and their positions do not pair up".to_owned());
                    }
                    let mut mid = Vec::new();
                    let mut all = vec![value(&s.start_radius), value(&s.end_radius)];
                    for (radius, position) in radii.iter().zip(&positions) {
                        let radius = Some(radius.clone());
                        let position = position.ok_or("a mid radius position was not decoded")?;
                        mid.push(json!({"position": position,
                            "radius": self.value_or(&radius, "mid radius")?}));
                        all.push(value(&radius));
                    }
                    let known: Vec<f64> = all.into_iter().flatten().collect();
                    let mean = match known.as_slice() {
                        [] => value(&s.radius).unwrap_or(1.0),
                        known => known.iter().sum::<f64>() / known.len() as f64,
                    };
                    if !mid.is_empty() {
                        size["mid"] = Value::Array(mid);
                    }
                    sections.push(vec![Section::Fillet(mean)]);
                    size
                }
                Some(t) if t.contains("Asymmetric") => {
                    let (a, b) = (
                        value(&s.offset_one).unwrap_or(0.1),
                        value(&s.offset_two).unwrap_or(0.1),
                    );
                    sections.push(vec![Section::Chamfer(a, b), Section::Chamfer(b, a)]);
                    json!({"type": "asymmetric",
                        "distance1": self.value_or(&s.offset_one, "first offset")?,
                        "distance2": self.value_or(&s.offset_two, "second offset")?,
                        "flip": s.is_flipped == Some(true)})
                }
                Some(t) if !t.starts_with("ConstantRadius") => {
                    return Err(format!("{t} is not supported"));
                }
                _ => {
                    sections.push(vec![Section::Fillet(value(&s.radius).unwrap_or(1.0))]);
                    json!({"type": "constant", "radius": self.value_or(&s.radius, "radius")?})
                }
            };
            if curvature(s) {
                extra.insert("continuity".to_owned(), json!("curvature"));
                if s.tangency_weight.is_some() {
                    extra.insert(
                        "tangency_weight".to_owned(),
                        self.value_or(&s.tangency_weight, "tangency weight")?,
                    );
                }
            }
            sizes.push(size);
            options.push(extra);
        }
        let rolling = d.is_rolling_ball_corner != Some(false);
        let tangent = |s: &EdgeSet| s.is_tangent_chain.or(d.is_tangent_chain) != Some(false);
        let make = |body: &str, edges: &[SetEdges]| -> Value {
            let sets: Vec<Value> = edges
                .iter()
                .filter(|(_, e)| !e.is_empty())
                .map(|(i, e)| {
                    let mut set =
                        json!({"edges": e, "size": sizes[*i], "tangent_chain": tangent(&sets[*i])});
                    for (key, value) in &options[*i] {
                        set[key] = value.clone();
                    }
                    set
                })
                .collect();
            json!({"type": "fillet", "body": body, "sets": sets, "rolling_ball_corners": rolling})
        };
        let mut candidates =
            self.dressup(&sets, &sections, &make, read_constant_fillet_inputs(d))?;
        // Asymmetric: which face takes which offset is not settled (K88),
        // the other reading next to each definition (as for chamfers).
        if sizes.iter().any(|s| s["type"] == "asymmetric") {
            candidates = candidates
                .into_iter()
                .flat_map(|c| {
                    let mut flipped = c.clone();
                    for def in &mut flipped.defs {
                        for set in def["sets"].as_array_mut().into_iter().flatten() {
                            if set["size"]["type"] == "asymmetric" {
                                let f = set["size"]["flip"].as_bool().unwrap_or(false);
                                set["size"]["flip"] = json!(!f);
                            }
                        }
                    }
                    flipped.guess = true;
                    flipped.input_provenance = InputProvenance::Inferred;
                    [c, flipped]
                })
                .collect();
        }
        Ok(candidates)
    }

    pub fn chamfer(&mut self, d: &ChamferDetail) -> Result<Vec<Candidate>, String> {
        let mut sets = d.edge_sets.clone().unwrap_or_default();
        if sets.is_empty() && d.edges.is_some() {
            // Retired form: one set of `edges`.
            sets.push(EdgeSet {
                edges: d.edges.clone(),
                ..EdgeSet::default()
            });
        }
        if sets.is_empty() {
            return Err("no edge sets decoded".to_owned());
        }
        let corner = match d.corner_type.as_deref() {
            None => "chamfer",
            Some(c) if c.starts_with("Chamfer") => "chamfer",
            Some(c) if c.starts_with("Miter") => "miter",
            Some(c) if c.starts_with("Blend") => "blend",
            Some(c) => return Err(format!("chamfer corners of type {c} are not supported")),
        };
        let legacy = d.chamfer_type.as_deref().unwrap_or("");
        let mut sizes = Vec::new();
        let mut sections = Vec::new();
        for s in &sets {
            let kind = s.edge_set_type.as_deref().unwrap_or(legacy);
            // The cross section, either way round where it is not decoded.
            let length = |r: &Option<Reference>| mm(param(r).and_then(|p| p.value).unwrap_or(0.1));
            let (size, section) = if kind.contains("TwoDistances") || s.distance_one.is_some() {
                let d1 = self.value_or(&s.distance_one, "first distance")?;
                let d2 = self.value_or(&s.distance_two, "second distance")?;
                let (a, b) = (length(&s.distance_one), length(&s.distance_two));
                (
                    json!({"type": "two_distances", "distance1": d1, "distance2": d2}),
                    vec![Section::Chamfer(a, b), Section::Chamfer(b, a)],
                )
            } else if kind.contains("DistanceAndAngle") || s.angle.is_some() {
                let dist = self.value_or(&s.distance, "distance")?;
                let a = length(&s.distance);
                let angle = param(&s.angle)
                    .and_then(|p| p.value)
                    .unwrap_or(std::f64::consts::FRAC_PI_4);
                (
                    json!({"type": "distance_angle", "distance": dist,
                        "angle": self.value_or(&s.angle, "angle")?}),
                    vec![
                        Section::ChamferAngle(a, angle, false),
                        Section::ChamferAngle(a, angle, true),
                    ],
                )
            } else {
                let dist = self.value_or(&s.distance, "distance")?;
                let a = length(&s.distance);
                (
                    json!({"type": "equal_distance", "distance": dist}),
                    vec![Section::Chamfer(a, a)],
                )
            };
            sizes.push((
                size,
                s.is_flipped == Some(true),
                s.is_tangent_chain != Some(false),
            ));
            sections.push(section);
        }
        let make = |body: &str, edges: &[SetEdges]| -> Value {
            let sets: Vec<Value> = edges
                .iter()
                .filter(|(_, e)| !e.is_empty())
                .map(|(i, e)| {
                    let (size, flip, tangent) = &sizes[*i];
                    json!({"edges": e, "size": size, "flip": flip, "tangent_chain": tangent})
                })
                .collect();
            json!({"type": "chamfer", "body": body, "sets": sets, "corner": corner})
        };
        let mut candidates = self.dressup(&sets, &sections, &make, read_chamfer_inputs(d))?;
        // Two distances: which face takes which is not decoded. Each
        // definition with the other reading next to it: the wrong one can
        // come within the tolerance, and the right one is tried before
        // other edges. So too a distance and angle whose face is not
        // decoded (no `isFlipped`, as in the `.ipt` import): the distance
        // on either face of the edge.
        let undecided = |(i, (s, _, _)): (usize, &(Value, bool, bool))| {
            s["type"] == "two_distances"
                || (s["type"] == "distance_angle" && sets[i].is_flipped.is_none())
        };
        if sizes.iter().enumerate().any(undecided) {
            candidates = candidates
                .into_iter()
                .flat_map(|c| {
                    let mut flipped = c.clone();
                    for def in &mut flipped.defs {
                        for set in def["sets"].as_array_mut().into_iter().flatten() {
                            let f = set["flip"].as_bool().unwrap_or(false);
                            set["flip"] = json!(!f);
                        }
                    }
                    flipped.guess = true;
                    flipped.input_provenance = InputProvenance::Inferred;
                    [c, flipped]
                })
                .collect();
        }
        Ok(candidates)
    }

    /// Fillet or chamfer candidates: edges from the fingerprints (external
    /// dumps), else the edges the history's next states no longer have;
    /// with [`Importer::guesses`] set, the next part of them (the edges
    /// found by their names, then one state's guesses at a time).
    fn dressup(
        &mut self,
        sets: &[EdgeSet],
        sections: &[Vec<Section>],
        make: &dyn Fn(&str, &[SetEdges]) -> Value,
        inputs_read: bool,
    ) -> Result<Vec<Candidate>, String> {
        let mut lazy = self.guesses.take();
        let result = self.dressup_part(sets, sections, make, inputs_read, lazy.as_mut());
        self.guesses = lazy;
        result
    }

    /// [`Importer::dressup`]: all candidates, or with `lazy` the next part.
    fn dressup_part(
        &mut self,
        sets: &[EdgeSet],
        sections: &[Vec<Section>],
        make: &dyn Fn(&str, &[SetEdges]) -> Value,
        inputs_read: bool,
        mut lazy: Option<&mut Guesses>,
    ) -> Result<Vec<Candidate>, String> {
        if lazy.as_ref().is_some_and(|g| g.done) {
            return Ok(Vec::new());
        }
        let named_first = lazy.as_ref().is_none_or(|g| !g.named);
        if let Some(g) = lazy.as_deref_mut() {
            g.named = true;
        }
        let fingerprints: Vec<Vec<&Fingerprint>> = sets
            .iter()
            .map(|s| {
                s.edges
                    .iter()
                    .flatten()
                    .filter_map(|r| match r {
                        Reference::Edge(fp) => Some(&**fp),
                        // A face the decoder did not turn into its edges
                        // (mitcad#96): not found.
                        Reference::Face(fp) if fp.f3d.is_some() => Some(&**fp),
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        // Edges the stream decoder found by their names (`_f3d`) come first,
        // then what the history suggests; an external dump's edges are
        // taken as they are.
        let mut first = Vec::new();
        let mut named_error = None;
        if named_first && fingerprints.iter().any(|f| !f.is_empty()) {
            let named = fingerprints.iter().flatten().any(|fp| fp.f3d.is_some());
            let result = self.dressup_by_fingerprints(&fingerprints, sections, make, inputs_read);
            if !named && let Some(g) = lazy.as_deref_mut() {
                g.done = true;
            }
            match result {
                Ok((c, _)) if !named => return Ok(vec![c]),
                Err(e) if !named => return Err(e),
                Ok((mut c, twins)) => {
                    c.note = Some(BY_NAME.to_owned());
                    let chains = self.by_chains(&c);
                    first.push(c);
                    first.extend(chains);
                    // With the other edges of the sliver faces beside them
                    // (mitcad#224), after the edges alone.
                    if let Some(mut t) = twins {
                        t.note = Some(format!(
                            "{BY_NAME}, with the other edges of the slivers beside them"
                        ));
                        first.push(t);
                    }
                }
                Err(e) => {
                    if crate::tracing() {
                        eprintln!("import: edges by their names: {e}");
                    }
                    // (A suppressed item's placeholder says it, mitcad#180.)
                    self.suppressed.named_error = Some(e.clone());
                    named_error = Some(e);
                    // Edges the file names that its history no longer has
                    // are no input of the file's operation: the other edges
                    // come first (the history checks them).
                    if let Some((subset, lost)) = without_stale_edges(&fingerprints)
                        && let Ok((mut c, _)) =
                            self.dressup_by_fingerprints(&subset, sections, make, false)
                    {
                        let plural = if lost == 1 { "" } else { "s" };
                        let dropped = format!("{lost} edge{plural}");
                        c.note = Some(format!(
                            "{BY_NAME}; {dropped} the file no longer has left out"
                        ));
                        c.input_provenance = InputProvenance::Inferred;
                        let chains = self.by_chains(&c);
                        // The lost references in their place as the edges
                        // that join the others end to end along a face all
                        // of them share (mitcad#343).
                        let joined = self.with_joining_edges(&c, lost).map(|mut j| {
                            j.note = Some(format!(
                                "{BY_NAME}; {dropped} the file no longer has taken as the edges \
                                 joining the others along their common face"
                            ));
                            j
                        });
                        first.push(c);
                        first.extend(chains);
                        first.extend(joined);
                    }
                }
            }
        }
        if !self.oracle.enabled {
            if let Some(g) = lazy.as_deref_mut() {
                g.done = true;
            }
            if !first.is_empty() {
                return Ok(first);
            }
            return Err(match named_error {
                Some(e) => format!("{e} (no history to find them)"),
                None => "its edges were not decoded (no history to find them)".to_owned(),
            });
        }
        // (The edges found by their names are not tried twice.)
        let mut seen: BTreeSet<Vec<String>> = first.iter().map(|c| dressup_key(&c.defs)).collect();
        // One part at a time: the edges found by their names before the
        // states' guesses.
        let mut skip = 0;
        if let Some(g) = lazy.as_deref_mut() {
            if !first.is_empty() {
                g.seen = seen;
                g.given = true;
                return Ok(first);
            }
            seen = std::mem::take(&mut g.seen);
            skip = g.states;
        }
        // The edges the next states lost. (Bodies kept within the tolerance
        // as their stored ones, mitcad#285.)
        let stored = self.stored_counterparts();
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = crate::body_scope::bodies(self.doc)
            .iter()
            .map(|b| {
                let sig = stored.get(&b.uid).copied();
                (
                    b.uid,
                    b.shape.clone(),
                    sig.or_else(|| Sig::of(kernel, b.shape)),
                )
            })
            .collect();
        // The file's state before the item, which the replay matches
        // (mitcad#440): an edge as far off it as off the next state was not
        // taken away by the item.
        let before_state: Vec<(StoredBody<K::Shape>, Sig)> = match self.oracle.cursor {
            Some(c) => self
                .oracle
                .state(kernel, c)
                .map(|s| s.to_vec())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        // The least depth of the item's dressups at an edge of a wedge.
        let least_depth = |wedge: f64| {
            sections
                .iter()
                .flatten()
                .map(|s| s.depth(wedge))
                .fold(f64::INFINITY, f64::min)
        };
        let mut candidates = Vec::new();
        let mut last_error = None;
        // (Past the item's own state too: below, a later state is looked
        // at only while the item's own lost no edges.)
        let from = self.oracle.next_index();
        let states = from..self.oracle.count().min(from + 4);
        let count = states.len();
        for q in states.skip(skip) {
            // One state's guesses at a time: the next ones when these give
            // no state.
            if let Some(g) = lazy.as_deref_mut() {
                if !candidates.is_empty() {
                    break;
                }
                // Past the item's own state, once that lost edges: a later
                // state's guesses add only edges the item's state did not
                // lose, or the same ones again (mitcad#181; seconds a state
                // on large bodies).
                if g.own_gone && g.own.is_some_and(|own| q > own) {
                    if crate::tracing() {
                        eprintln!(
                            "import: state {q}: not looked at, the item's own state lost its edges"
                        );
                    }
                    g.states = count;
                    break;
                }
                g.states += 1;
            }
            let kernel = self.doc.kernel();
            let state: Vec<(StoredBody<K::Shape>, Sig)> = match self.oracle.state(kernel, q) {
                Ok(s) => s.to_vec(),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            // Only bodies the state changed: the replay's without an equal
            // in the state, against the state's without one in the replay.
            let mut used = vec![false; state.len()];
            let mut changed = Vec::new();
            for (uid, shape, sig) in &bodies {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None => changed.push((*uid, shape)),
                }
            }
            let after: Vec<&K::Shape> = state
                .iter()
                .zip(&used)
                .filter(|(_, u)| !**u)
                .map(|((b, _), _)| &b.shape)
                .collect();
            if after.is_empty() {
                continue;
            }
            // The state before's bodies, but those the replay and this
            // state both have as they are.
            let mut kept = vec![false; before_state.len()];
            for (j, _) in used.iter().enumerate().filter(|(_, u)| **u) {
                if let Some(i) = (0..before_state.len())
                    .find(|&i| !kept[i] && before_state[i].1.same(&state[j].1))
                {
                    kept[i] = true;
                }
            }
            let before: Vec<&K::Shape> = before_state
                .iter()
                .zip(&kept)
                .filter(|(_, k)| !**k)
                .map(|((b, _), _)| &b.shape)
                .collect();
            // Per body, each gone edge with the set it fits best: whose
            // dressup the state has there (where it meets the edge's faces
            // and its middle lie on the state's faces), else whose size
            // leaves the distance the edge's midpoint has from the state.
            let mut found: Vec<(BodyUid, Vec<SetGone>)> = Vec::new();
            for (uid, shape) in changed {
                let clock = std::time::Instant::now();
                let consumed =
                    match refs::consumed_edges(kernel, shape, &after, &before, &least_depth) {
                        Ok(c) => c,
                        Err(e) => {
                            last_error = Some(e);
                            continue;
                        }
                    };
                if consumed.is_empty() {
                    continue;
                }
                let fits = match section_fits(kernel, shape, &consumed, sections, &after) {
                    Ok(f) => f,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
                let edges: Vec<SetGone> = consumed
                    .iter()
                    .zip(fits)
                    .map(|(gone, fit)| {
                        let wedge = gone.wedge.unwrap_or(std::f64::consts::FRAC_PI_2);
                        let off = |i: usize| {
                            let e = sections[i][0].depth(wedge);
                            (gone.distance - e).abs() / e.max(1e-9)
                        };
                        let set = (0..sets.len())
                            .min_by(|&a, &b| {
                                (fit[a] > FIT)
                                    .cmp(&(fit[b] > FIT))
                                    .then(off(a).total_cmp(&off(b)))
                            })
                            .unwrap_or(0);
                        SetGone {
                            set,
                            name: gone.name.to_string(),
                            distance: gone.distance,
                            off: off(set),
                            fit: fit[set],
                            fits: fit[set] <= FIT,
                            smooth: gone
                                .wedge
                                .is_some_and(|w| w > std::f64::consts::PI - refs::SMOOTH_WEDGE),
                            concave: gone.across.is_some_and(|a| !a.convex),
                        }
                    })
                    .collect();
                if crate::tracing() {
                    eprintln!(
                        "import: state {q} {uid}: {} gone edges in {:.2} s",
                        consumed.len(),
                        clock.elapsed().as_secs_f64()
                    );
                    for (gone, e) in consumed.iter().zip(&edges) {
                        eprintln!(
                            "import: state {q} {uid}: gone {} length {:.3} distance {:.4} wedge {:.1} {} set {} off {:.3} fit {:.4}",
                            e.name,
                            gone.length,
                            gone.distance,
                            gone.wedge.unwrap_or(f64::NAN).to_degrees(),
                            gone.across.map_or("?", |a| if a.convex {
                                "convex"
                            } else {
                                "concave"
                            }),
                            e.set,
                            e.off,
                            e.fit
                        );
                    }
                }
                found.push((uid, edges));
            }
            if let Some(g) = lazy.as_deref_mut()
                && g.own == Some(q)
                && found.iter().any(|(_, e)| e.iter().any(|e| !e.smooth))
            {
                g.own_gone = true;
            }
            // The edges whose dressup the state has (at the distance its
            // size leaves at their corner angle, and with its faces through
            // the points where it meets theirs): at once, then one tangent
            // chain after the other in the dump's order of the sets and edges
            // (OCCT fails some contours together that build one by one).
            // Then those edges with the gone edges that continue them
            // tangentially (a rounding that ends where its edge runs on
            // tangentially takes the next edge's middle away too, and the
            // kernel follows the chain); the edges whose dressup the state
            // has whatever their distance (beside a narrow wedge another
            // face of the state can be nearer than the rounding), with their
            // continuations. Then the looser guesses: those at that
            // distance (edges next to the rounded ones can be cut away too);
            // every gone edge; per set the largest group at one distance.
            // Edges between faces that meet tangentially are no dressup's.
            // First of all, those edges with the concave ones whose dressup
            // the state has at any distance: a rounding of a concave edge
            // whose face beside it is a strip narrower than the ball rolls
            // on the strip's far edge and leaves the edge much nearer the
            // state than the ball's depth (a fin's side strips under its
            // top's chamfer, mitcad#1388); the same set as the next where
            // there are none.
            for variant in [7, 0, 1, 2, 3, 4, 5, 6] {
                if variant == 7
                    && !found.iter().any(|(_, edges)| {
                        edges
                            .iter()
                            .any(|e| !e.smooth && e.fits && e.concave && e.off > 0.1)
                    })
                {
                    continue;
                }
                let mut defs = Vec::new();
                for (uid, edges) in &found {
                    let mut by_set: Vec<SetEdges> =
                        (0..sets.len()).map(|i| (i, Vec::new())).collect();
                    let shape = bodies.iter().find(|(b, _, _)| b == uid).map(|(_, s, _)| s);
                    for (i, list) in by_set.iter_mut() {
                        let i = *i;
                        let mine: Vec<&SetGone> =
                            edges.iter().filter(|e| e.set == i && !e.smooth).collect();
                        let chosen: Vec<&SetGone> = match variant {
                            0..=2 => mine
                                .iter()
                                .copied()
                                .filter(|e| e.fits && e.off <= 0.1)
                                .collect(),
                            3 => mine.iter().copied().filter(|e| e.fits).collect(),
                            4 => mine.iter().copied().filter(|e| e.off <= 0.1).collect(),
                            5 => mine.clone(),
                            7 => mine
                                .iter()
                                .copied()
                                .filter(|e| e.fits && (e.off <= 0.1 || e.concave))
                                .collect(),
                            _ => largest_group(&mine),
                        };
                        list.extend(chosen.into_iter().map(|e| e.name.clone()));
                        if matches!(variant, 2 | 3)
                            && !list.is_empty()
                            && let Some(shape) = shape
                        {
                            let more = tangent_continuations(kernel, shape, list, &mine);
                            list.extend(more);
                        }
                    }
                    if variant == 1 {
                        for (i, list) in &by_set {
                            let chains = match shape {
                                Some(shape) => tangent_chains(kernel, shape, list),
                                None => vec![list.clone()],
                            };
                            for chain in chains {
                                defs.push(make(&uid.to_string(), &[(*i, chain)]));
                            }
                        }
                    } else if by_set.iter().any(|(_, l)| !l.is_empty()) {
                        defs.push(make(&uid.to_string(), &by_set));
                    }
                }
                // One chain is no new candidate.
                if variant == 1 && defs.len() < 2 {
                    continue;
                }
                if !defs.is_empty() && seen.insert(dressup_key(&defs)) {
                    let note = match variant {
                        0 | 7 => format!("edges whose dressup ASM history state {q} has"),
                        1 => format!(
                            "edges whose dressup ASM history state {q} has, one tangent chain at a time"
                        ),
                        _ => format!("edges found against ASM history state {q}"),
                    };
                    candidates.push(Candidate {
                        core: CandidateCore::of(defs),
                        note: Some(note),
                        guess: true,
                        predicted: None,
                        input_provenance: InputProvenance::Inferred,
                        first: false,
                    });
                }
            }
        }
        if let Some(g) = lazy {
            g.done = g.states >= count;
            g.seen = seen;
            if g.given {
                return Ok(candidates);
            }
            g.given = !candidates.is_empty();
        }
        first.extend(candidates);
        if first.is_empty() {
            return Err(match named_error {
                Some(e) => format!(
                    "{e}; {}",
                    last_error.unwrap_or_else(|| {
                        "no edge of the replay is gone in the next history states".to_owned()
                    })
                ),
                None => last_error.unwrap_or_else(|| {
                    "its edges were not decoded, and no edge of the replay is gone in the next history states"
                        .to_owned()
                }),
            });
        }
        Ok(first)
    }

    /// The dressup of edges given by fingerprints (every set's), by body.
    /// The stream decoder's edges (`_f3d`) as [`refs::dressup_edge_match`]
    /// finds them (a two-distance chamfer's sides decoded, mitcad#96:
    /// [`Importer::decoded_sides`]); an external dump's by their middles.
    /// With the other edges of sliver faces beside edges found
    /// ([`refs::FoundEdge::sliver_twin`], mitcad#224), the same with them
    /// too as a second definition.
    fn dressup_by_fingerprints(
        &self,
        fingerprints: &[Vec<&Fingerprint>],
        sections: &[Vec<Section>],
        make: &dyn Fn(&str, &[SetEdges]) -> Value,
        inputs_read: bool,
    ) -> Result<(Candidate, Option<Candidate>), String> {
        let replay = fingerprints
            .iter()
            .flatten()
            .any(|fp| fp.f3d.is_some())
            .then(|| refs::ReplayEdges::of(self.doc));
        // The edges found for each input, by set.
        let mut found_by_set: Vec<(usize, Vec<refs::FoundEdge>)> = Vec::new();
        // The inputs the replay lacks (their place in `found_by_set`, the
        // input and the error): parts of a replay edge, looked for together
        // below (mitcad#1437).
        let mut missing: Vec<(usize, &Fingerprint, String)> = Vec::new();
        for (i, fps) in fingerprints.iter().enumerate() {
            for (j, fp) in fps.iter().enumerate() {
                let edges = match &replay {
                    Some(replay) if fp.f3d.is_some() => {
                        let error = |e: refs::DressupEdgeFailure| {
                            let context = fp
                                .f3d
                                .as_ref()
                                .and_then(|f| f.found.as_deref())
                                .map(|found| format!(" (decoder: {found})"))
                                .unwrap_or_default();
                            format!("edge set {i}, input {j}: {e}{context}")
                        };
                        match self.match_dressup_edge(replay, fp) {
                            Ok(edges) => edges,
                            Err(e @ refs::DressupEdgeFailure::MissingInReplay) => {
                                missing.push((found_by_set.len(), *fp, error(e)));
                                Vec::new()
                            }
                            Err(e) => return Err(error(e)),
                        }
                    }
                    _ => refs::resolve_edge(self.doc, fp)
                        .map(|(body, name)| {
                            vec![refs::FoundEdge {
                                body,
                                name,
                                mid: [0.0; 3],
                                direction: None,
                                chosen_from_multiple: false,
                                sliver_twin: false,
                            }]
                        })
                        .ok_or("an edge was not found in the replay")?,
                };
                if crate::tracing() {
                    let names: Vec<String> = edges.iter().map(|e| e.name.to_string()).collect();
                    eprintln!(
                        "import: edge set {i}, input {j}: middle {:.3?} length {:.3?}: {names:?}",
                        fp.mid_point.map(crate::geom::mm3),
                        fp.length.map(crate::geom::mm)
                    );
                }
                found_by_set.push((i, edges));
            }
        }
        // The inputs that are parts of one replay edge, by set: the edge
        // with the first of them (mitcad#1437).
        if let (Some(replay), false) = (&replay, missing.is_empty()) {
            let mut found_parts: Vec<usize> = Vec::new();
            for i in 0..fingerprints.len() {
                let (at, fps): (Vec<usize>, Vec<&Fingerprint>) = missing
                    .iter()
                    .filter(|(at, _, _)| found_by_set[*at].0 == i)
                    .map(|(at, fp, _)| (*at, *fp))
                    .unzip();
                if fps.len() < 2 {
                    continue;
                }
                for (parts, edge) in self.match_dressup_edge_parts(replay, &fps) {
                    // (The others have no edge of their own.)
                    found_by_set[at[parts[0]]].1.push(edge);
                    found_parts.extend(parts.iter().map(|&k| at[k]));
                }
            }
            missing.retain(|(at, _, _)| !found_parts.contains(at));
        }
        if let Some((_, _, e)) = missing.into_iter().next() {
            return Err(e);
        }
        let alone = self.dressup_of_found(
            fingerprints.len(),
            &found_by_set,
            false,
            sections,
            make,
            inputs_read,
        );
        let twins = found_by_set
            .iter()
            .any(|(_, edges)| edges.iter().any(|e| e.sliver_twin))
            .then(|| {
                self.dressup_of_found(
                    fingerprints.len(),
                    &found_by_set,
                    true,
                    sections,
                    make,
                    inputs_read,
                )
            });
        Ok((alone, twins))
    }

    /// [`Importer::dressup_by_fingerprints`]'s definition of the edges found
    /// (for each input its set and edges), with the sliver faces' twins or
    /// without them.
    fn dressup_of_found(
        &self,
        sets: usize,
        found_by_set: &[(usize, Vec<refs::FoundEdge>)],
        twins: bool,
        sections: &[Vec<Section>],
        make: &dyn Fn(&str, &[SetEdges]) -> Value,
        inputs_read: bool,
    ) -> Candidate {
        let mut by_body: Vec<(BodyUid, Vec<SetEdges>)> = Vec::new();
        let mut found_edges: Vec<refs::FoundEdge> = Vec::new();
        let mut chosen = false;
        for (i, edges) in found_by_set {
            // With a sliver's twin the pair is the choice (its flag).
            chosen |= match edges.iter().find(|e| e.sliver_twin) {
                Some(twin) if twins => twin.chosen_from_multiple,
                _ => edges
                    .iter()
                    .any(|e| !e.sliver_twin && e.chosen_from_multiple),
            };
            for e in edges.iter().filter(|e| twins || !e.sliver_twin) {
                let entry = match by_body.iter_mut().position(|(b, _)| *b == e.body) {
                    Some(k) => &mut by_body[k].1,
                    None => {
                        let sets = (0..sets).map(|i| (i, Vec::new())).collect();
                        by_body.push((e.body, sets));
                        &mut by_body.last_mut().expect("pushed").1
                    }
                };
                let name = e.name.to_string();
                if !entry[*i].1.contains(&name) {
                    entry[*i].1.push(name);
                }
                found_edges.push(e.clone());
            }
        }
        let defs: Vec<Value> = by_body
            .iter()
            .map(|(body, edges)| {
                let mut def = make(&body.to_string(), edges);
                self.decoded_sides(&mut def, *body, edges, sections, &found_edges);
                def
            })
            .collect();
        // The sides of two distances come from the edges' directions.
        let sides_read = !defs.iter().any(|def: &Value| {
            def["sets"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|s| s["size"]["type"] == "two_distances")
        }) || found_edges.iter().all(|e| e.direction.is_some());
        let input_provenance = if chosen {
            InputProvenance::Inferred
        } else if inputs_read && sides_read && !found_edges.is_empty() {
            InputProvenance::Read
        } else {
            InputProvenance::Unknown
        };
        Candidate {
            core: CandidateCore::of(defs),
            note: None,
            guess: false,
            predicted: None,
            input_provenance,
            first: false,
        }
    }

    // Edges found by their names (mitcad#96).

    /// A two-distance chamfer's sides in a definition made of `edges`
    /// (mitcad#96): the file's first distance is taken to lie on the face
    /// on the left of the edge's own direction in the file (the face whose
    /// coedge runs along the edge), MitCad's on the first face of the edge's
    /// name unless `flip` ([`refs::first_face_on_left`]). A distance and
    /// angle lie on that face too where the file's flag (`isFlipped`, the
    /// set's `flip` as decoded) is off and on the other face where it is on
    /// (mitcad#343: the replay's names order an edge's faces either way, so
    /// that the flag taken against the name's first face put the chamfer on
    /// the wrong face of every other replay). A tangent chain
    /// takes the reading of most of its edges (the kernel carries the side
    /// of the chain's first edge along it); a set whose chains differ is
    /// split. Sets of equal distances and edges whose side is not known
    /// keep the set's `flip`. The other reading stays a guess after it.
    fn decoded_sides(
        &self,
        def: &mut Value,
        body: BodyUid,
        edges: &[SetEdges],
        sections: &[Vec<Section>],
        found: &[refs::FoundEdge],
    ) {
        if def["type"] != "chamfer" || found.iter().all(|e| e.direction.is_none()) {
            return;
        }
        let Some(shape) = crate::body_scope::bodies(self.doc)
            .iter()
            .find(|b| b.uid == body)
            .map(|b| b.shape.clone())
        else {
            return;
        };
        let kernel = self.doc.kernel();
        let Some(sets) = def["sets"].as_array() else {
            return;
        };
        // The definition's sets are the non-empty ones, in order.
        let index: Vec<usize> = edges
            .iter()
            .filter(|(_, e)| !e.is_empty())
            .map(|(i, _)| *i)
            .collect();
        let mut out = Vec::new();
        for (k, set) in sets.iter().enumerate() {
            let unequal = index
                .get(k)
                .and_then(|&i| sections.get(i))
                .and_then(|s| s.first())
                .is_some_and(|s| match *s {
                    Section::Chamfer(a, b) => (a - b).abs() > 1e-9 * a.max(b),
                    _ => false,
                });
            // A distance and angle: the file's flag says which side of the
            // edge the distance is measured on (mitcad#343).
            let angled = set["size"]["type"] == "distance_angle";
            if !angled && (set["size"]["type"] != "two_distances" || !unequal) {
                out.push(set.clone());
                continue;
            }
            let flip = set["flip"].as_bool().unwrap_or(false);
            let names: Vec<String> = set["edges"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect();
            let mut groups: Vec<(bool, Vec<String>)> = Vec::new();
            for chain in tangent_chains(kernel, &shape, &names) {
                // (Each edge's reading; the kernel takes the side of the
                // chain's first edge in its own order, so the most edges'.)
                let flips: Vec<bool> = chain
                    .iter()
                    .filter_map(|name| {
                        let e = found
                            .iter()
                            .find(|e| e.body == body && e.name.to_string() == *name)?;
                        refs::first_face_on_left(kernel, &shape, e)
                            .map(|left| if angled { left == flip } else { !left })
                    })
                    .collect();
                if crate::tracing() {
                    eprintln!(
                        "import: [{:.2}] {} set {k}: the file's flag {flip}, by the edges' sides \
                         {flips:?}",
                        crate::trace_clock(),
                        set["size"]["type"]
                    );
                }
                let flipped = flips.iter().filter(|&&f| f).count();
                let f = match flipped * 2 {
                    _ if flips.is_empty() => flip,
                    n if n == flips.len() => flips[0],
                    n => n > flips.len(),
                };
                match groups.last_mut() {
                    Some((g, list)) if *g == f => list.extend(chain),
                    _ => groups.push((f, chain)),
                }
            }
            for (f, list) in groups {
                let mut s = set.clone();
                s["edges"] = json!(list);
                s["flip"] = json!(f);
                out.push(s);
            }
        }
        def["sets"] = Value::Array(out);
    }

    /// The edges found by their names one tangent chain at a time (a
    /// feature per chain and set: the kernel builds some chains only one by
    /// one, mitcad#96), when that makes more than one feature; a guess
    /// after the edges at once.
    fn by_chains(&self, c: &Candidate) -> Option<Candidate> {
        let kernel = self.doc.kernel();
        let mut defs = Vec::new();
        for def in &c.defs {
            let body = def["body"].as_str()?;
            let shape = crate::body_scope::bodies(self.doc)
                .iter()
                .find(|b| b.uid.to_string() == body)?
                .shape
                .clone();
            for set in def["sets"].as_array()? {
                let edges: Vec<String> = set["edges"]
                    .as_array()?
                    .iter()
                    .filter_map(|e| e.as_str().map(str::to_owned))
                    .collect();
                for chain in tangent_chains(kernel, &shape, &edges) {
                    let mut one = set.clone();
                    one["edges"] = json!(chain);
                    let mut d = def.clone();
                    d["sets"] = json!([one]);
                    defs.push(d);
                }
            }
        }
        (defs.len() > c.defs.len()).then(|| Candidate {
            core: CandidateCore::of(defs),
            note: Some(format!("{BY_NAME}, one tangent chain at a time")),
            guess: true,
            predicted: None,
            input_provenance: c.input_provenance,
            first: false,
        })
    }

    /// Candidate `c` (the edges found by their names, the `lost` ones the
    /// file no longer has left out) with the edges that join each set's
    /// edges end to end along a face they all share ([`joining_edges`]), at
    /// most `lost` of them in all: where the lost references named the
    /// pieces of one chain between the edges found (a rounding along a
    /// torus's edges with a plane and with narrow faces between, whose
    /// references to the narrow faces' edges are lost, mitcad#343). None
    /// when no set has such edges.
    fn with_joining_edges(&self, c: &Candidate, lost: usize) -> Option<Candidate> {
        let kernel = self.doc.kernel();
        let mut defs = c.defs.clone();
        let mut added = 0;
        for def in &mut defs {
            let body = def["body"].as_str()?.to_owned();
            let shape = crate::body_scope::bodies(self.doc)
                .iter()
                .find(|b| b.uid.to_string() == body)?
                .shape
                .clone();
            let ends = edge_ends(kernel, &shape);
            for set in def["sets"].as_array_mut()? {
                let kept: Vec<String> = set["edges"]
                    .as_array()?
                    .iter()
                    .filter_map(|e| e.as_str().map(str::to_owned))
                    .collect();
                let more = joining_edges(&kept, &ends, lost - added, 1e-6);
                added += more.len();
                if let Some(edges) = set["edges"].as_array_mut() {
                    edges.extend(more.into_iter().map(Value::String));
                }
            }
        }
        (added > 0).then_some(Candidate {
            core: CandidateCore::of(defs),
            note: None,
            guess: true,
            predicted: None,
            input_provenance: InputProvenance::Inferred,
            first: false,
        })
    }
}

/// Every edge of `shape` by name with its two end points (the edges whose
/// ends the kernel does not give left out).
fn edge_ends<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<(String, [[f64; 3]; 2])> {
    use mitcad_model::EdgeName;
    use mitcad_model::datum::PathParameter;
    let Ok(edges) = kernel.edges(shape) else {
        return Vec::new();
    };
    edges
        .into_iter()
        .filter_map(|e| {
            let text = e.name?;
            let name: EdgeName = text.parse().ok()?;
            let at = |f: f64| {
                kernel
                    .path_point(
                        shape,
                        std::slice::from_ref(&name),
                        PathParameter::Fraction(f),
                    )
                    .ok()
                    .map(|p| p.point)
            };
            Some((text, [at(0.0)?, at(1.0)?]))
        })
        .collect()
}

/// The edges of `ends` (every edge of a body by name with its end points)
/// that join the edges `kept` end to end along a face all of them share:
/// edges of that face, not kept, from an end of one kept edge to an end of
/// another (within `tol`). Of the faces the kept edges share, the one face
/// with such edges, at most `limit` of them; none where no face or more
/// than one has them, or where one has more than `limit`.
fn joining_edges(
    kept: &[String],
    ends: &[(String, [[f64; 3]; 2])],
    limit: usize,
    tol: f64,
) -> Vec<String> {
    use mitcad_model::EdgeName;
    let faces_of = |name: &str| -> Option<[String; 2]> {
        let e: EdgeName = name.parse().ok()?;
        Some([e.faces()[0].to_string(), e.faces()[1].to_string()])
    };
    if kept.len() < 2 || limit == 0 {
        return Vec::new();
    }
    // The faces all kept edges share.
    let Some(first) = faces_of(&kept[0]) else {
        return Vec::new();
    };
    let mut common: Vec<String> = first.to_vec();
    common.dedup();
    for k in &kept[1..] {
        let Some(f) = faces_of(k) else {
            return Vec::new();
        };
        common.retain(|c| f.contains(c));
    }
    // Which kept edge an end point is an end of.
    let kept_ends: Vec<(usize, [f64; 3])> = kept
        .iter()
        .enumerate()
        .filter_map(|(i, k)| ends.iter().find(|(n, _)| n == k).map(|(_, p)| (i, p)))
        .flat_map(|(i, p)| p.iter().map(move |q| (i, *q)))
        .collect();
    let kept_at = |p: [f64; 3]| -> Vec<usize> {
        kept_ends
            .iter()
            .filter(|(_, q)| geom::distance(p, *q) <= tol)
            .map(|(i, _)| *i)
            .collect()
    };
    let mut found: Vec<Vec<String>> = Vec::new();
    for face in &common {
        let joins: Vec<String> = ends
            .iter()
            .filter(|(name, [a, b])| {
                !kept.contains(name)
                    && faces_of(name).is_some_and(|f| f.contains(face))
                    && geom::distance(*a, *b) > tol
                    && {
                        let (at_a, at_b) = (kept_at(*a), kept_at(*b));
                        at_a.iter().any(|i| at_b.iter().any(|j| i != j))
                    }
            })
            .map(|(name, _)| name.clone())
            .collect();
        if !joins.is_empty() {
            found.push(joins);
        }
    }
    match found.as_slice() {
        [one] if one.len() <= limit => one.clone(),
        _ => Vec::new(),
    }
}

/// The note of a fillet's or chamfer's edges found by their names.
const BY_NAME: &str = "edges found by their names in the file";

/// A deliberately narrow fillet path whose modelling inputs are all
/// decoded fields. The native option decoder supplies the set's tangent
/// flag and continuity and the feature's corner flag; their absence must
/// not acquire read provenance from the translator's defaults. A G1
/// constant fillet does not use tangency weight, side orientation or a
/// variable-radius start vertex. Replay resolution is checked separately.
fn read_constant_fillet_inputs(d: &FilletDetail) -> bool {
    if d.is_rolling_ball_corner != Some(true) || d.is_g2 == Some(true) {
        return false;
    }
    d.edge_sets.as_ref().is_some_and(|sets| {
        !sets.is_empty()
            && sets.iter().all(|s| {
                s.edge_set_type.as_deref() == Some("ConstantRadiusFilletEdgeSet")
                    && param(&s.radius).is_some_and(|p| {
                        p.unit.as_deref() == Some("mm")
                            && p.value.is_some_and(|v| v.is_finite() && v > 0.0)
                    })
                    && s.is_tangent_chain.is_some()
                    && s.continuity.as_deref() == Some("TangentSurfaceContinuityType")
                    && named_edges_read(&s.edges)
            })
    })
}

/// Whether every input of an equal-distance or two-distance chamfer is a
/// decoded field (mitcad#186): the corner type, each set's size
/// parameters, tangent-chain option (the set record's own byte) and edges
/// named by the stream. A two-distance chamfer's sides come from the
/// edges' directions in the file, so that the other reading stays a guess
/// after it; a distance-and-angle chamfer's angle side is not decoded.
/// Replay resolution is checked separately (the edges found, not chosen
/// from several).
fn read_chamfer_inputs(d: &ChamferDetail) -> bool {
    let two = match d.chamfer_type.as_deref() {
        Some("EqualDistanceChamferType") => false,
        Some("TwoDistancesChamferType") => true,
        _ => return false,
    };
    let sized = |r: &Option<Reference>| {
        param(r).is_some_and(|p| {
            p.unit.as_deref().is_some_and(|u| !u.is_empty())
                && p.value.is_some_and(|v| v.is_finite() && v > 0.0)
        })
    };
    d.corner_type.is_some()
        && d.edge_sets.as_ref().is_some_and(|sets| {
            !sets.is_empty()
                && sets.iter().all(|s| {
                    let sizes = if two {
                        sized(&s.distance_one) && sized(&s.distance_two)
                    } else {
                        sized(&s.distance)
                    };
                    sizes && s.is_tangent_chain.is_some() && named_edges_read(&s.edges)
                })
        })
}

/// The edges of a dressup's sets without those the file names but its
/// history no longer has: both faces of the edge are there with no edge
/// between them, or a face of it is gone. The file's own operation takes
/// the other edges only (the API reference dumps list just the edges that
/// resolve for most items whose edges the decoder left unresolved for
/// these reasons; the history checks the rest, which a decoder's own
/// failure to find a face does not hold), and how many were left out; None
/// when there are none of that kind or none is left.
fn without_stale_edges<'a>(
    sets: &[Vec<&'a Fingerprint>],
) -> Option<(Vec<Vec<&'a Fingerprint>>, usize)> {
    let stale = |fp: &Fingerprint| {
        fp.mid_point.is_none()
            && fp.f3d.as_ref().is_some_and(|f| {
                f.found.as_deref().is_some_and(|t| {
                    t.starts_with("no edge between the faces named")
                        || t.starts_with("a face of the edge is not in the history")
                })
            })
    };
    let dropped = sets.iter().flatten().filter(|fp| stale(fp)).count();
    let kept: Vec<Vec<&Fingerprint>> = sets
        .iter()
        .map(|s| s.iter().copied().filter(|fp| !stale(fp)).collect())
        .collect();
    (dropped > 0 && kept.iter().any(|s| !s.is_empty())).then_some((kept, dropped))
}

/// Whether every edge of a set is one the stream decoder named (its
/// recipe and entities) with its middle and length. An edge of a face the
/// set selects (a `bounded_face` recipe, which the decoder turned into
/// the face's edges) is named by the face's recipe: the set input says it
/// selects the face's edges (`edge_groups`, `set_selects` 16), so that
/// every one of them is read.
fn named_edges_read(edges: &Option<Vec<Reference>>) -> bool {
    edges.as_ref().is_some_and(|edges| {
        !edges.is_empty()
            && edges.iter().all(|r| match r {
                Reference::Edge(fp) => {
                    fp.mid_point
                        .is_some_and(|p| p.iter().all(|v| v.is_finite()))
                        && fp.length.is_some_and(|v| v.is_finite() && v > 0.0)
                        && fp.f3d.as_ref().is_some_and(|f| {
                            f.object_id.is_some()
                                && (f.recipe.as_deref() == Some("edge")
                                    || (f.recipe.as_deref() == Some("bounded_face")
                                        && f.found
                                            .as_deref()
                                            .is_some_and(|t| t.ends_with("(an edge of a face)"))))
                                && f.entities.as_ref().is_some_and(|e| !e.is_empty())
                        })
                }
                _ => false,
            })
    })
}

/// A candidate once with each set of participants (none: all bodies).
/// A sketch made on a face for an extrusion of faces (mitcad#169): its
/// definition, the keys of its regions on the faces, the first region's
/// centre and the sketch's normal.
/// A sketch already in the timeline has its uid (a string) for its
/// definition ([`Importer::part_face_sketch`]).
type FaceSketch = (Value, Vec<String>, [f64; 3], [f64; 3]);

/// The profile of a part of an item in another component taken where the
/// item's own operation stands ([`Importer::part_face_sketch`],
/// mitcad#1347): the faces, whether they are read, and the sketch made on
/// them there.
pub type PartFaceSketch = (
    Vec<(BodyUid, FaceName, f64)>,
    bool,
    (Value, Vec<String>, [f64; 3], [f64; 3]),
);

/// A face sketch added to the timeline ([`Importer::make_face_sketch`]): its
/// uid, the keys of its regions on the faces, the first one's centre and
/// the sketch's normal.
type MadeFaceSketch = (FeatureUid, Vec<String>, [f64; 3], [f64; 3]);

/// The definitions of an extrusion of regions of a face sketch
/// ([`FaceSketch`]), `extrude` given the sketch to name in its profiles:
/// the sketch's definition and the extrusion on it (`$0`), or the
/// extrusion alone on a sketch already in the timeline.
fn on_face_sketch(sketch: &Value, extrude: impl FnOnce(&str) -> Value) -> Vec<Value> {
    match sketch.as_str() {
        Some(uid) => vec![extrude(uid)],
        None => vec![sketch.clone(), extrude("$0")],
    }
}

/// An extrusion candidate that takes the regions `keys` of its sketch
/// `sketch` too (the faces its profile selects, mitcad#169), those it
/// does not take already.
fn with_face_regions(c: &mut Candidate, sketch: &str, keys: &[String]) {
    if let Some(Value::Array(profiles)) = c.defs.last_mut().map(|d| &mut d["profiles"]) {
        for key in keys {
            let p = json!({"sketch": sketch, "region": key});
            if !profiles.contains(&p) {
                profiles.push(p);
            }
        }
    }
}

/// The candidates of a surface item (`isSolid` false, mitcad#455): each
/// definition a surface (`surface`), a new body without participants that
/// changes no volume.
pub(crate) fn surface_candidates(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    for c in &mut candidates {
        for def in &mut c.defs {
            def["surface"] = json!(true);
            if def["operation"] != "new_component" {
                def["operation"] = json!("new_body");
            }
            if let Some(map) = def.as_object_mut() {
                map.remove("participants");
            }
        }
        c.predicted = None;
    }
    let mut kept: Vec<Candidate> = Vec::new();
    for c in candidates {
        if !kept.iter().any(|k| k.defs == c.defs) {
            kept.push(c);
        }
    }
    kept
}

pub fn with_participants(c: &Candidate, options: &[Vec<String>]) -> Vec<Candidate> {
    options
        .iter()
        .enumerate()
        .map(|(i, participants)| {
            let mut c = c.clone();
            // The first option retains the decoded selection (or the
            // existing all-body default). Later options replace it with
            // all bodies or the bodies changed by the stored history.
            if i > 0 {
                c.input_provenance = InputProvenance::Inferred;
            }
            if !participants.is_empty() {
                // (Not on a sketch the candidate makes for faces it
                // extrudes.)
                for def in c.defs.iter_mut().filter(|d| d["type"] != "sketch") {
                    def["participants"] = json!(participants);
                }
            }
            c
        })
        .collect()
}

/// An edge set's index with the names of its edges.
type SetEdges = (usize, Vec<String>);

/// A gone edge and the edge set it fits best.
#[derive(Debug, Clone)]
struct SetGone {
    set: usize,
    name: String,
    /// How far its midpoint is from the next state.
    distance: f64,
    /// How far that is off the set's expected distance (relative).
    off: f64,
    /// How far the set's dressup there is off the next state's faces
    /// (relative to its size, beyond [`FIT_ABSOLUTE`]).
    fit: f64,
    /// Whether the next state has the set's dressup there.
    fits: bool,
    /// Whether its faces meet tangentially (no dressup's input).
    smooth: bool,
    /// Whether its faces meet in a concave edge.
    concave: bool,
}

/// How far (relative to its size) the points of a dressup at an edge may
/// be off the next state's faces, beyond [`FIT_ABSOLUTE`] mm: the contact
/// points are where the faces are tangent to it, and the middle of a
/// rounding between curved faces is only near the plane formula's.
const FIT: f64 = 0.05;
const FIT_ABSOLUTE: f64 = 1e-3;

/// Per gone edge and edge set, how far the points of the set's dressup at
/// the edge (any of its cross sections) are off the faces of `after`:
/// the largest of their distances beyond [`FIT_ABSOLUTE`], relative to the
/// size; infinite where the edge's faces give no cross section.
fn section_fits<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    gone: &[refs::GoneEdge],
    sections: &[Vec<Section>],
    after: &[&K::Shape],
) -> Result<Vec<Vec<f64>>, String> {
    let mut points = Vec::new();
    let mut owners = Vec::new();
    for (k, edge) in gone.iter().enumerate() {
        for (i, list) in sections.iter().enumerate() {
            for section in list {
                if let Some(p) = refs::section_points(kernel, shape, edge, *section) {
                    owners.push((k, i, section.size()));
                    points.extend(p);
                }
            }
        }
    }
    let mut fits = vec![vec![f64::INFINITY; sections.len()]; gone.len()];
    if points.is_empty() {
        return Ok(fits);
    }
    let distances = refs::nearest_boundary(kernel, after, &points)?;
    for (n, (k, i, size)) in owners.into_iter().enumerate() {
        let worst = distances[3 * n..3 * n + 3]
            .iter()
            .fold(0.0_f64, |a, &d| a.max(d));
        let fit = (worst - FIT_ABSOLUTE).max(0.0) / size.max(1e-9);
        fits[k][i] = fits[k][i].min(fit);
    }
    Ok(fits)
}

/// Edges (by name) in tangent chains, as a fillet follows them: edges
/// that share an end where their tangents are parallel, in the order of
/// their first edges. Ends within the import's geometric tolerance are one
/// (mitcad#1343): two edges of a vertex end on their curves within the
/// vertex's tolerance, and an extrusion's end face whose loop joins the
/// sketch's curves with the edges of the face it lies on (mitcad#1138)
/// has its curves' ends 1 to 2 µm apart there, which split a rounding of
/// 44 edges round a ring into 9 features that no longer built.
fn tangent_chains<K: Kernel>(kernel: &K, shape: &K::Shape, names: &[String]) -> Vec<Vec<String>> {
    use mitcad_model::EdgeName;
    use mitcad_model::datum::PathParameter;
    // Each edge's two ends: the point and the tangent there.
    type Ends = Option<[([f64; 3], [f64; 3]); 2]>;
    let ends: Vec<Ends> = names
        .iter()
        .map(|n| {
            let name: EdgeName = n.parse().ok()?;
            let at = |f: f64| {
                kernel
                    .path_point(
                        shape,
                        std::slice::from_ref(&name),
                        PathParameter::Fraction(f),
                    )
                    .ok()
                    .map(|p| (p.point, p.tangent))
            };
            Some([at(0.0)?, at(1.0)?])
        })
        .collect();
    let within = refs::geometric_tolerance().max(1e-6);
    let mut chain: Vec<usize> = (0..names.len()).collect();
    fn root(chain: &mut [usize], mut i: usize) -> usize {
        while chain[i] != i {
            chain[i] = chain[chain[i]];
            i = chain[i];
        }
        i
    }
    for a in 0..names.len() {
        for b in a + 1..names.len() {
            let (Some(ea), Some(eb)) = (&ends[a], &ends[b]) else {
                continue;
            };
            let joined = ea.iter().any(|(pa, ta)| {
                eb.iter().any(|(pb, tb)| {
                    let parallel = geom::unit(*ta)
                        .zip(geom::unit(*tb))
                        .is_some_and(|(u, v)| geom::norm(geom::cross(u, v)) < 1e-3);
                    geom::distance(*pa, *pb) < within && parallel
                })
            });
            if joined {
                let (ra, rb) = (root(&mut chain, a), root(&mut chain, b));
                chain[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    let mut out: Vec<(usize, Vec<String>)> = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let r = root(&mut chain, i);
        match out.iter_mut().find(|(k, _)| *k == r) {
            Some((_, list)) => list.push(name.clone()),
            None => out.push((r, vec![name.clone()])),
        }
    }
    out.into_iter().map(|(_, list)| list).collect()
}

/// The edges of `pool` (gone edges, by name) not in `chosen` that lie in a
/// tangent chain with one of `chosen`, in the pool's order.
fn tangent_continuations<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    chosen: &[String],
    pool: &[&SetGone],
) -> Vec<String> {
    let mut names = chosen.to_vec();
    names.extend(
        pool.iter()
            .filter(|e| !chosen.contains(&e.name))
            .map(|e| e.name.clone()),
    );
    if names.len() == chosen.len() {
        return Vec::new();
    }
    tangent_chains(kernel, shape, &names)
        .into_iter()
        .filter(|chain| chain.iter().any(|n| chosen.contains(n)))
        .flatten()
        .filter(|n| !chosen.contains(n))
        .collect()
}

/// What tells fillet or chamfer definitions apart: each with its sets'
/// edges sorted.
fn dressup_key(defs: &[Value]) -> Vec<String> {
    defs.iter()
        .map(|d| {
            let mut d = d.clone();
            for set in d["sets"].as_array_mut().into_iter().flatten() {
                if let Some(edges) = set["edges"].as_array_mut() {
                    edges.sort_by_key(Value::to_string);
                }
            }
            d.to_string()
        })
        .collect()
}

/// What the decoder says of an edge it found no edge for when the faces the
/// file names are not in the history state the item was made from, or no edge
/// lies between them: the file's own reference does not resolve there.
const LOST_REFERENCES: [&str; 2] = [
    "a face of the edge is not in the history state",
    "no edge between the faces named",
];

/// How many of a fillet's or chamfer's edge references do not resolve in the
/// file's own history (the faces they name are not in the state the item
/// was made from, or no edge lies between them): `(lost, all)`. The
/// application that wrote the file reports such references as lost and keeps
/// the geometry the item had before (its health warning says the edge
/// references are lost and the cached geometry is used): the items with such
/// references are those the API reference dumps report so (*What stays open
/// (decoder)* in `docs/f3d-learning.md`). None when the item is not a fillet or chamfer
/// or has none.
pub(crate) fn lost_edge_references(detail: &Detail) -> Option<(usize, usize)> {
    let sets: Vec<&EdgeSet> = match detail {
        Detail::Fillet(d) => d.edge_sets.iter().flatten().collect(),
        Detail::Chamfer(d) => d.edge_sets.iter().flatten().collect(),
        _ => return None,
    };
    let (mut lost, mut all) = (0, 0);
    for edge in sets.iter().flat_map(|s| s.edges.iter().flatten()) {
        let Reference::Edge(fp) = edge else { continue };
        all += 1;
        let found = fp.f3d.as_ref().and_then(|f| f.found.as_deref());
        if found.is_some_and(|f| LOST_REFERENCES.contains(&f)) {
            lost += 1;
        }
    }
    (lost > 0).then_some((lost, all))
}

/// How many curves an extrusion's or revolution's profile names when its
/// sketch no longer has any of them ([`crate::profiles::gone_curves`]):
/// the sketch was edited after the item was made, and its regions are not
/// the profile (mitcad#379: one circle the profile named was deleted, and
/// the sketch's one region left is the projected circle round it).
pub(crate) fn profile_curves_gone(detail: &Detail) -> Option<usize> {
    let other = match detail {
        Detail::Extrude(d) => &d.other,
        Detail::Revolve(d) => &d.other,
        _ => return None,
    };
    crate::profiles::gone_curves(other.get("_f3d_profile_loops")?)
}

/// The largest group of edges at one distance (within 3 %) from the state
/// after a rounding.
fn largest_group<'a>(edges: &[&'a SetGone]) -> Vec<&'a SetGone> {
    let mut best: Vec<&SetGone> = Vec::new();
    for e in edges {
        let d = e.distance;
        let group: Vec<&SetGone> = edges
            .iter()
            .copied()
            .filter(|x| (x.distance - d).abs() <= 0.03 * d.max(1e-9))
            .collect();
        if group.len() > best.len() {
            best = group;
        }
    }
    best
}

/// Up to `k` sets of regions (indices into `areas`, which is sorted,
/// largest first) of any size whose total area is within 5 % of `target`
/// (the regions' areas come from sampled outlines), nearest first. A
/// depth-first search with a bounded number of steps.
fn area_subsets(areas: &[f64], target: f64, k: usize) -> Vec<Vec<usize>> {
    struct Search {
        areas: Vec<f64>,
        /// The area of the regions from i on.
        rest: Vec<f64>,
        target: f64,
        tolerance: f64,
        steps: usize,
        best: Vec<(f64, Vec<usize>)>,
        k: usize,
    }
    impl Search {
        fn visit(&mut self, i: usize, sum: f64, chosen: &mut Vec<usize>) {
            if self.steps == 0 {
                return;
            }
            self.steps -= 1;
            let off = (sum - self.target).abs();
            if !chosen.is_empty() && off <= self.tolerance {
                let worst = self.best.last().map(|(o, _)| *o);
                if self.best.len() < self.k || worst.is_some_and(|w| off < w) {
                    self.best.push((off, chosen.clone()));
                    self.best.sort_by(|a, b| a.0.total_cmp(&b.0));
                    self.best.truncate(self.k);
                }
            }
            if i >= self.areas.len()
                || sum + self.rest[i] < self.target - self.tolerance
                || sum > self.target + self.tolerance
            {
                return;
            }
            // With region i, then without it.
            chosen.push(i);
            self.visit(i + 1, sum + self.areas[i], chosen);
            chosen.pop();
            self.visit(i + 1, sum, chosen);
        }
    }
    if target.is_nan() || target <= 0.0 || areas.is_empty() {
        return Vec::new();
    }
    let mut rest = vec![0.0; areas.len() + 1];
    for i in (0..areas.len()).rev() {
        rest[i] = rest[i + 1] + areas[i];
    }
    let mut search = Search {
        areas: areas.to_vec(),
        rest,
        target,
        tolerance: 0.05 * target,
        steps: 200_000,
        best: Vec::new(),
        k,
    };
    search.visit(0, 0.0, &mut Vec::new());
    search.best.into_iter().map(|(_, c)| c).collect()
}

/// A feature's profile references: a list, or one profile (external dumps
/// may write `ExtrudeFeature.profile` as a single reference).
pub(crate) fn profiles_of(
    list: &Option<Vec<Reference>>,
    other: &serde_json::Map<String, Value>,
) -> Vec<Reference> {
    if let Some(list) = list {
        return list.clone();
    }
    match other.get("profile") {
        Some(Value::Object(map)) => vec![Reference::from_map(map.clone())],
        _ => Vec::new(),
    }
}

/// The sketch (timeline index) and the file's id of the sketch curve a
/// path entity of a profile names (an open path, mitcad#463).
fn path_curve(reference: &Reference) -> Option<(i64, String)> {
    let Reference::Other(map) = reference else {
        return None;
    };
    if map.get("_type").and_then(Value::as_str) != Some("PathEntity") {
        return None;
    }
    let entity = map.get("entity")?;
    if entity.get("kind").and_then(Value::as_str) != Some("sketch_entity") {
        return None;
    }
    Some((
        entity.get("sketch_timeline_index")?.as_i64()?,
        entity.get("id")?.as_str()?.to_owned(),
    ))
}

/// Whether two lists of region keys name the same regions.
fn same_keys(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && {
        let mut a: Vec<&String> = a.iter().collect();
        let mut b: Vec<&String> = b.iter().collect();
        a.sort();
        b.sort();
        a == b
    }
}

/// Profile regions of one sketch to try together.
#[derive(Debug, Clone, Default)]
pub struct RegionSet {
    pub keys: Vec<String>,
    pub guess: bool,
    /// Their total area, mm².
    area: f64,
    /// Each region's centroid (sketch mm) and area.
    parts: Vec<([f64; 2], f64)>,
    /// Regions found by probing a history state: the side of the sketch
    /// plane (±1 along its normal) where the state changes their material,
    /// and the state's volume change (mm³).
    probe: Option<(f64, f64)>,
    /// The one set the dump's measured profiles make up with a union of
    /// regions: tried before the sets found without the measures.
    measured: bool,
    /// The decoded profile loops selected these regions exactly
    /// ([`crate::profiles::mapped`], mitcad#186): a set of any other origin
    /// is not read.
    pub(crate) exact: bool,
}

impl RegionSet {
    /// The volume of a turn by `angle` about a line of the sketch plane
    /// (Pappus), mm³; the line through `a` along the unit `d`.
    fn turned(&self, a: [f64; 2], d: [f64; 2], angle: f64) -> f64 {
        self.parts
            .iter()
            .map(|(c, area)| {
                let r = ((c[0] - a[0]) * d[1] - (c[1] - a[1]) * d[0]).abs();
                area * r * angle.abs()
            })
            .sum()
    }
}

/// Replaces every string `from` in a JSON value by `to`.
fn replace_text(v: &mut Value, from: &str, to: &str) {
    match v {
        Value::String(s) if s == from => *s = to.to_owned(),
        Value::Array(a) => a.iter_mut().for_each(|x| replace_text(x, from, to)),
        Value::Object(m) => m.values_mut().for_each(|x| replace_text(x, from, to)),
        _ => {}
    }
}

/// What an imported sketch's solve is checked against: the file's
/// positions (points of reference curves within [`REFERENCE_SLACK`] when
/// they are free) and, within the geometric tolerance, the regions the
/// file's numbers bound with `entities`.
struct SketchCheck<'a> {
    unlocked: bool,
    positions: &'a [(u32, [f64; 2])],
    reference_points: &'a std::collections::HashSet<u32>,
    entities: &'a [Value],
}

impl SketchCheck<'_> {
    /// How far a solve of `def` is from the file's positions, mm, and
    /// whether it is the file's sketch: within [`SKETCH_TOLERANCE`], or
    /// within the import's geometric tolerance where the regions its active
    /// constraints bound stay the file's (mitcad#224, mitcad#396: the linked
    /// curves the replay's edges give and the file's rounded numbers differ
    /// by micrometres). Linked curves the file keeps inconsistent among
    /// themselves (an arc's end off its circle by a few hundredths of a
    /// millimetre) can only be held together by moving their points a
    /// little.
    fn within(&self, output: Option<&SketchOutput>, def: &Value) -> (f64, bool) {
        let error = output.map_or(f64::INFINITY, |o| {
            if self.unlocked {
                sketch::position_error_slack(
                    self.positions,
                    &o.solved.points,
                    self.reference_points,
                    REFERENCE_SLACK,
                )
            } else {
                position_error(self.positions, &o.solved.points)
            }
        });
        let constraints: Vec<Value> = def["constraints"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter(|v| inactive::is_active(v))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let within = error <= SKETCH_TOLERANCE
            || error <= refs::geometric_tolerance()
                && output.is_some_and(|o| {
                    linked::regions_kept(self.entities, &constraints, &o.solved.points)
                });
        (error, within)
    }
}

/// Traces the point of an imported sketch's solve that moved most from the
/// file's position and what holds it.
fn trace_worst(def: &Value, positions: &[(u32, [f64; 2])], output: Option<&SketchOutput>) {
    let Some(o) = output else {
        return;
    };
    let worst = positions
        .iter()
        .filter_map(|(n, at)| {
            let s = o.solved.points.get(&mitcad_model::EntityUid(*n))?;
            Some((*n, *at, *s, (s[0] - at[0]).hypot(s[1] - at[1])))
        })
        .max_by(|a, b| a.3.total_cmp(&b.3));
    let Some((n, at, s, d)) = worst else {
        return;
    };
    eprintln!("import:   moved most: p{n} {at:?} -> {s:?} ({d:.3e})");
    let key = format!("\"p{n}\"");
    let items = def["constraints"]
        .as_array()
        .into_iter()
        .chain(def["dimensions"].as_array())
        .flatten();
    for v in items {
        if v.to_string().contains(&key) {
            eprintln!("import:     {v}");
        }
    }
}

/// The sketch's frame: `model_frame` (sketch → model), else `origin`,
/// `xDirection`, `yDirection`.
fn sketch_frame(detail: &SketchDetail) -> Option<SketchFrame> {
    let frame = if let Some(m) = detail.model_frame.as_ref().and_then(|f| f.sketch_to_model) {
        geom::frame_of(&m)?
    } else {
        let (o, x, y) = (detail.origin?, detail.x_direction?, detail.y_direction?);
        let m = [
            [x[0], y[0], 0.0, o[0]],
            [x[1], y[1], 0.0, o[1]],
            [x[2], y[2], 0.0, o[2]],
            [0.0, 0.0, 0.0, 1.0],
        ];
        geom::frame_of(&m)?
    };
    // The sketch lies where its points do (mitcad#169).
    let height = mm(plane_height(detail));
    if height == 0.0 {
        return Some(frame);
    }
    Some(SketchFrame {
        origin: geom::add(frame.origin, geom::scale(geom::normal(&frame), height)),
        ..frame
    })
}

/// The height (cm, along the normal of its stored frame) at which most of
/// a sketch's points lie, where that is not the frame's plane: a sketch
/// on a face can keep its points at the face's height in its own
/// coordinates while its frame lies in a parallel plane through the
/// origin (a sketch on a body's top face 1.6 mm up, whose extrusion of 10
/// mm starts from the face: the file's body reaches 11.6 mm). The points
/// are taken in the sketch's plane (their x and y), so the frame moves to
/// their height. 0 when more than half of the points do not share one.
fn plane_height(detail: &SketchDetail) -> f64 {
    const SAME: f64 = 1e-7;
    let mut heights: Vec<f64> = detail
        .points
        .iter()
        .flatten()
        .filter_map(|p| p.xyz.map(|at| at[2]))
        .collect();
    heights.sort_by(f64::total_cmp);
    let (mut best, mut count, mut start) = (0.0, 0, 0);
    for i in 0..heights.len() {
        while heights[i] - heights[start] > SAME {
            start += 1;
        }
        if i + 1 - start > count {
            count = i + 1 - start;
            best = heights[start];
        }
    }
    if 2 * count > heights.len() && best.abs() > SAME {
        best
    } else {
        0.0
    }
}

/// The model X axis, or Y when the normal is along X.
fn default_x(normal: [f64; 3]) -> [f64; 3] {
    let x = if normal[0].abs() > 1.0 - 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    geom::unit(geom::sub(x, geom::scale(normal, geom::dot(x, normal)))).unwrap_or(x)
}

/// Whether a point is inside a polygon (even-odd rule).
fn in_polygon(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// The box of a polygon: (low corner, high corner).
fn bounds(polygon: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in polygon {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

/// Where a polygon's edges cross the line at height `y`, sorted.
fn crossings(polygon: &[[f64; 2]], y: f64, out: &mut Vec<f64>) {
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > y) != (b[1] > y) {
            out.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
        }
    }
}

/// A point inside each region: inside its outer outline but not in one of
/// its holes; the centroid when it is, else the middle of the widest
/// stretch of a horizontal scan line inside it; None when the region is
/// too thin to find one.
pub(crate) fn interior_points(regions: &[Region]) -> Vec<Option<[f64; 2]>> {
    regions
        .iter()
        .map(|r| {
            let (lo, hi) = bounds(&r.outline);
            let own = |p: [f64; 2]| {
                in_polygon(&r.outline, p) && !r.holes.iter().any(|h| in_polygon(h, p))
            };
            if own(r.centroid) {
                return Some(r.centroid);
            }
            // The widest stretch of a scan line inside it.
            let lines = 64;
            let mut best: Option<(f64, [f64; 2])> = None;
            let mut xs = Vec::new();
            for i in 0..lines {
                let y = lo[1] + (i as f64 + 0.5) / lines as f64 * (hi[1] - lo[1]);
                xs.clear();
                crossings(&r.outline, y, &mut xs);
                for h in &r.holes {
                    crossings(h, y, &mut xs);
                }
                xs.sort_by(f64::total_cmp);
                for w in xs.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    if b - a <= best.map_or(1e-9, |(width, _)| width) {
                        continue;
                    }
                    let p = [(a + b) / 2.0, y];
                    if own(p) {
                        best = Some((b - a, p));
                    }
                }
            }
            best.map(|(_, p)| p)
        })
        .collect()
}

/// Calls `f` with each set of `k` of `n` indices, in lexicographic order,
/// at most `limit` times.
pub(crate) fn combinations(n: usize, k: usize, limit: usize, f: &mut dyn FnMut(&[usize])) {
    if k == 0 || k > n {
        return;
    }
    let mut c: Vec<usize> = (0..k).collect();
    let mut count = 0;
    loop {
        f(&c);
        count += 1;
        if count >= limit {
            return;
        }
        // Next combination.
        let mut i = k;
        loop {
            if i == 0 {
                return;
            }
            i -= 1;
            if c[i] < n - k + i {
                break;
            }
            if i == 0 {
                return;
            }
        }
        if c[i] >= n - k + i {
            return;
        }
        c[i] += 1;
        for j in i + 1..k {
            c[j] = c[j - 1] + 1;
        }
    }
}

/// The dropped items, counted by reason.
fn summarize(dropped: &[String]) -> String {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for d in dropped {
        let reason = d.split_once(": ").map_or(d.as_str(), |(_, r)| r).to_owned();
        *counts.entry(reason).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(r, n)| format!("{n} × {r}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extra fields of an edge set the IR does not model.
trait OtherRef {
    fn other_ref(&self, key: &str) -> Option<Reference>;
}

impl OtherRef for EdgeSet {
    fn other_ref(&self, key: &str) -> Option<Reference> {
        let v = self.other.get(key)?.as_object()?.clone();
        Some(Reference::from_map(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_alternatives_report_inference_without_changing_replay_policy() {
        let mut c = Candidate::new(json!({"type": "extrude", "operation": "cut"}));
        c.input_provenance = InputProvenance::Read;
        c.first = true;
        c.predicted = Some(-20.0);
        let selected = vec!["F1.b0".to_owned()];
        for options in [
            vec![selected.clone(), Vec::new()],
            vec![Vec::new(), selected.clone()],
        ] {
            let variants = with_participants(&c, &options);
            assert_eq!(variants[0].input_provenance, InputProvenance::Read);
            assert_eq!(variants[1].input_provenance, InputProvenance::Inferred);
            for (variant, participants) in variants.iter().zip(&options) {
                assert!(!variant.guess);
                assert!(variant.first);
                assert_eq!(variant.predicted, c.predicted);
                if participants.is_empty() {
                    assert_eq!(variant.defs, c.defs);
                } else {
                    assert_eq!(variant.defs[0]["participants"], json!(participants));
                }
            }
        }
        c.input_provenance = InputProvenance::Inferred;
        assert_eq!(
            with_participants(&c, &[selected])[0].input_provenance,
            InputProvenance::Inferred
        );
    }

    /// An edge between faces `a` and `b` of base feature F1 (with its
    /// index) from `p` to `q`.
    fn edge(
        a: u32,
        b: u32,
        index: Option<u32>,
        p: [f64; 3],
        q: [f64; 3],
    ) -> (String, [[f64; 3]; 2]) {
        let mut name = format!("E{{F1:import({a})|F1:import({b})}}");
        if let Some(k) = index {
            name.push_str(&format!("#{k}"));
        }
        (name, [p, q])
    }

    #[test]
    fn edges_joining_the_found_ones_along_their_common_face() {
        // A torus (31) meeting a plane (34) in three pieces, two narrow
        // faces (32, 35) between them, each meeting the torus in one edge
        // from the end of one piece to the start of the next and the plane
        // in two edges away from them.
        let ends = vec![
            edge(31, 34, Some(0), [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            edge(31, 34, Some(1), [2.0, 0.0, 0.0], [3.0, 0.0, 0.0]),
            edge(31, 34, Some(2), [4.0, 0.0, 0.0], [5.0, 0.0, 0.0]),
            edge(31, 32, None, [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]),
            edge(31, 35, None, [3.0, 0.0, 0.0], [4.0, 0.0, 0.0]),
            edge(32, 34, Some(0), [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]),
            edge(32, 34, Some(1), [2.0, 0.0, 0.0], [2.0, 1.0, 0.0]),
            edge(34, 35, Some(0), [3.0, 0.0, 0.0], [3.0, 1.0, 0.0]),
            edge(34, 35, Some(1), [4.0, 0.0, 0.0], [4.0, 1.0, 0.0]),
        ];
        let kept: Vec<String> = ends[..3].iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(
            joining_edges(&kept, &ends, 2, 1e-6),
            vec![ends[3].0.clone(), ends[4].0.clone()]
        );
        // More than the references lost, or none lost: none.
        assert!(joining_edges(&kept, &ends, 1, 1e-6).is_empty());
        assert!(joining_edges(&kept, &ends, 0, 1e-6).is_empty());
        // One edge alone joins nothing.
        assert!(joining_edges(&kept[..1], &ends, 2, 1e-6).is_empty());
        // The face 1 edge from the start of the first piece to the end of
        // the last joins two found edges along the torus too: three, more
        // than lost.
        let mut with_far = ends.clone();
        with_far.push(edge(1, 31, None, [0.0, 0.0, 0.0], [5.0, 0.0, 0.0]));
        assert!(joining_edges(&kept, &with_far, 2, 1e-6).is_empty());
        // Joining edges along both shared faces: which face is not known.
        let mut both = ends[..5].to_vec();
        both.push(edge(34, 40, None, [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]));
        assert!(joining_edges(&kept, &both, 4, 1e-6).is_empty());
    }

    #[test]
    fn combinations_in_order() {
        let mut out = Vec::new();
        combinations(4, 2, 100, &mut |c| out.push(c.to_vec()));
        assert_eq!(
            out,
            vec![
                vec![0, 1],
                vec![0, 2],
                vec![0, 3],
                vec![1, 2],
                vec![1, 3],
                vec![2, 3]
            ]
        );
        let mut n = 0;
        combinations(10, 3, 5, &mut |_| n += 1);
        assert_eq!(n, 5);
        let mut one = Vec::new();
        combinations(3, 3, 10, &mut |c| one.push(c.to_vec()));
        assert_eq!(one, vec![vec![0, 1, 2]]);
    }

    #[test]
    fn region_sets_of_any_size_by_area() {
        // A corpus design's Extrude1: two of five regions make the area
        // the history's new body asks for.
        let areas = [215.189, 123.729, 28.540, 4.811, 4.811];
        let found = area_subsets(&areas, 1016.756 / 3.0, 6);
        assert_eq!(found[0], vec![0, 1]);
        assert!(found.iter().all(|s| {
            let a: f64 = s.iter().map(|&i| areas[i]).sum();
            (a - 338.92).abs() <= 0.05 * 338.92
        }));
        assert!(area_subsets(&areas, 1000.0, 6).is_empty());
        assert!(area_subsets(&[], 10.0, 6).is_empty());
    }

    use mitcad_model::KernelError;

    /// Straight edges along x by name, for [`tangent_chains`]: each name's
    /// start and end.
    struct Lines(Vec<(String, [f64; 3], [f64; 3])>);

    impl Kernel for Lines {
        type Shape = ();

        fn extrude(&self, _: &mitcad_model::ExtrudeSpec<'_>) -> Result<(), KernelError> {
            Err(KernelError::Unsupported("extrude"))
        }

        fn solids(&self, _: &()) -> Result<Vec<()>, KernelError> {
            Ok(vec![()])
        }

        fn boolean(
            &self,
            _: mitcad_model::BooleanOp,
            _: &[&()],
            _: &(),
        ) -> Result<mitcad_model::BooleanOutput<()>, KernelError> {
            Err(KernelError::Unsupported("boolean"))
        }

        fn fillet(
            &self,
            _: mitcad_model::FeatureUid,
            _: &(),
            _: &[mitcad_model::FilletSet<'_>],
            _: bool,
        ) -> Result<(), KernelError> {
            Err(KernelError::Unsupported("fillet"))
        }

        fn count_edges(&self, _: &(), _: &mitcad_model::EdgeName) -> usize {
            0
        }

        fn path_point(
            &self,
            _: &(),
            edges: &[mitcad_model::EdgeName],
            at: mitcad_model::datum::PathParameter,
        ) -> Result<mitcad_model::datum::PathPoint, KernelError> {
            let mitcad_model::datum::PathParameter::Fraction(f) = at else {
                return Err(KernelError::Unsupported("paths"));
            };
            let name = edges[0].to_string();
            let (_, a, b) = self
                .0
                .iter()
                .find(|(n, _, _)| *n == name)
                .ok_or(KernelError::Unsupported("paths"))?;
            Ok(mitcad_model::datum::PathPoint {
                point: [
                    a[0] + f * (b[0] - a[0]),
                    a[1] + f * (b[1] - a[1]),
                    a[2] + f * (b[2] - a[2]),
                ],
                tangent: [1.0, 0.0, 0.0],
            })
        }
    }

    /// Edges whose ends lie 1 to 2 µm apart, as an extrusion's end face
    /// gives them where its loop joins the sketch's curves with the edges of
    /// the face it lies on, are one tangent chain (mitcad#1343: a rounding
    /// of 44 edges round a ring was split into 9 features, which no longer
    /// built); edges farther apart than the import's geometric tolerance are
    /// not.
    #[test]
    fn tangent_chains_join_ends_within_the_geometric_tolerance() {
        let name = |k: usize| format!("E{{F1:import({k})|F2:end(r{{c1}})}}");
        let kernel = Lines(vec![
            (name(1), [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            (name(2), [1.0 + 1.7e-6, 0.0, 0.0], [2.0, 0.0, 0.0]),
            (name(3), [2.0 + 0.5e-6, 0.0, 0.0], [3.0, 0.0, 0.0]),
            (name(4), [3.5, 0.0, 0.0], [4.0, 0.0, 0.0]),
        ]);
        let names: Vec<String> = (1..=4).map(name).collect();
        let chains = tangent_chains(&kernel, &(), &names);
        assert_eq!(chains, vec![names[..3].to_vec(), vec![names[3].clone()]]);
        let _tight = refs::GeometricTolerance::set(1e-7);
        assert_eq!(tangent_chains(&kernel, &(), &names).len(), 3);
    }
}
