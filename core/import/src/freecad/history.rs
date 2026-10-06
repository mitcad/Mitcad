// SPDX-License-Identifier: MIT
//! The history (stage 3): PartDesign Bodies and the Part workbench's
//! objects replayed as Mitcad features on one timeline, each checked
//! against the shape FreeCAD stored for it.
//!
//! - What is replayed: every PartDesign Body the import made a body of
//!   (with the Bodies its booleans use), and every Part workbench result
//!   the import made a body of whose tree has objects Mitcad builds
//!   (primitives, extrusions, revolutions, booleans, fillets, chamfers,
//!   mirrorings), into the result's component. Other bodies stay the
//!   stored shapes of stage 1 (base features before the timeline).
//! - Order: the dependencies (links other than containers' lists; a Body
//!   after its features, a boolean after the Bodies it uses), ties in the
//!   document's order; every sketch of the document comes in its place.
//! - Check: after each feature, the Mitcad bodies of its Body (or of the
//!   Part object) together against the object's stored shape (volume,
//!   area, centre of mass, solids; [`TOLERANCE`]). The first definition
//!   that gives FreeCAD's shape is kept (a few are tried where FreeCAD's
//!   conventions are uncertain: directions, taper signs, chamfer sides).
//! - Fallback: a feature that cannot be translated, fails or does not give
//!   FreeCAD's shape becomes a base feature of the stored shape, replacing
//!   the Body's Mitcad body (`replaces`: it keeps its id), and the
//!   features after it continue on it. A feature whose result FreeCAD
//!   stored as its base's (it changes nothing) is skipped.
//! - A Body's `Tip` before its last feature: the features after the tip
//!   are replayed, then suppressed; FreeCAD's suppressed features (1.0) are
//!   replayed and suppressed.

use std::collections::{BTreeSet, HashMap, HashSet};

use mitcad_freecad::Object;
use mitcad_model::{
    BaseInput, BodyUid, ComponentUid, FeatureDef, FeatureUid, ImportBody, Kernel, Transform,
    ValueInput,
};
use serde_json::Value;

use super::report::{FeatureCheck, FeatureReport, ObjectOutcome};
use super::sketches::Placed;
use super::{Importer, NOT_CONSUMING, measure};

/// The largest distance (relative volume and area, centre over size) of a
/// replayed feature from FreeCAD's stored shape: the same OCCT definitions
/// built by another OCCT version.
pub(super) const TOLERANCE: f64 = 1e-6;

/// A sketch the import made: its Mitcad feature and the entities of
/// FreeCAD's geometry.
#[derive(Debug, Clone)]
pub(super) struct SketchMade {
    pub uid: FeatureUid,
    pub component: ComponentUid,
    /// The sketch's frame in its component.
    pub frame: mitcad_model::SketchFrame,
    /// FreeCAD geometry index → Mitcad curve entity id.
    pub curves: HashMap<i32, u32>,
    /// FreeCAD point geometry index → Mitcad point entity id.
    pub points: HashMap<i32, u32>,
    /// The H and V axis lines.
    pub axes: [Option<u32>; 2],
}

/// The Mitcad bodies holding an object's result, with their shapes right
/// after it (the names of their elements carry through later features).
#[derive(Debug, Clone)]
pub(super) struct Made<S> {
    pub bodies: Vec<(BodyUid, S)>,
}

/// What the replay knows.
pub(super) struct Replay<S> {
    /// The planned bodies the replay makes, by object.
    pub roots: HashMap<String, usize>,
    /// The objects to replay.
    pub replayed: HashSet<String>,
    /// Sketches referred to with their H or V axis.
    pub axis_uses: HashMap<String, [bool; 2]>,
    pub results: HashMap<String, Made<S>>,
    /// The Mitcad bodies of each FreeCAD Body now.
    pub bodies: HashMap<String, Vec<BodyUid>>,
    /// The Mitcad features made of each object, and the main one of those
    /// replayed (the one patterns copy).
    pub features: HashMap<String, Vec<FeatureUid>>,
    pub main: HashMap<String, FeatureUid>,
    pub sketches: HashMap<String, SketchMade>,
    pub datums: HashMap<String, FeatureUid>,
    /// How many more objects use each Part workbench operand.
    pub uses: HashMap<String, usize>,
    /// Objects replayed parametrically (not fallen back), which patterns
    /// and mirrors can copy.
    pub parametric: HashSet<String>,
}

impl<S> Default for Replay<S> {
    fn default() -> Self {
        Self {
            roots: HashMap::new(),
            replayed: HashSet::new(),
            axis_uses: HashMap::new(),
            results: HashMap::new(),
            bodies: HashMap::new(),
            features: HashMap::new(),
            main: HashMap::new(),
            sketches: HashMap::new(),
            datums: HashMap::new(),
            uses: HashMap::new(),
            parametric: HashSet::new(),
        }
    }
}

/// A definition to try: one or more Mitcad features (later ones may
/// refer to earlier ones as `$0`, `$1`), with notes for the report.
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub defs: Vec<Value>,
    /// Per definition, what its name adds to the object's label (the main
    /// one's is empty: it is named after the label).
    pub suffixes: Vec<&'static str>,
    pub notes: Vec<String>,
    /// Kept as `partial`: something of FreeCAD's definition is left out.
    pub partial: bool,
}

impl Candidate {
    pub fn new(def: Value) -> Self {
        Self {
            defs: vec![def],
            suffixes: vec![""],
            notes: Vec::new(),
            partial: false,
        }
    }

    /// Several definitions with their name suffixes.
    pub fn several(defs: Vec<(Value, &'static str)>) -> Self {
        let (defs, suffixes) = defs.into_iter().unzip();
        Self {
            defs,
            suffixes,
            notes: Vec::new(),
            partial: false,
        }
    }

    /// Adds a definition after the others.
    pub fn then(mut self, def: Value, suffix: &'static str) -> Self {
        self.defs.push(def);
        self.suffixes.push(suffix);
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn partial(mut self, note: impl Into<String>) -> Self {
        self.partial = true;
        self.notes.push(note.into());
        self
    }

    /// The main definition: the one named after the label.
    fn main(&self) -> usize {
        self.suffixes
            .iter()
            .position(|s| s.is_empty())
            .unwrap_or(self.defs.len().saturating_sub(1))
    }

    /// The Mitcad type to report: `given`, else the main definition's.
    fn mitcad_type(&self, given: &str) -> String {
        if !given.is_empty() {
            return given.to_owned();
        }
        self.defs
            .get(self.main())
            .and_then(|d| d["type"].as_str())
            .unwrap_or_default()
            .to_owned()
    }
}

/// How a replayed object compares with its stored shape.
pub(super) enum Check {
    /// Measured: within the tolerance or not.
    Measured(FeatureCheck),
    /// Not measurable (no stored shape, a stale one, a kernel without
    /// measures): taken as it is.
    Unchecked(String),
}

impl Check {
    fn pass(&self) -> bool {
        match self {
            Check::Measured(c) => c.pass,
            Check::Unchecked(_) => true,
        }
    }
}

/// Where an object's result goes and what it works on.
pub(super) struct Target {
    pub place: Placed,
    /// The FreeCAD Body it is in (PartDesign features).
    pub body: Option<String>,
    /// The Mitcad bodies it works on.
    pub current: Vec<BodyUid>,
}

pub(super) fn is_datum(t: &str) -> bool {
    matches!(
        t,
        "PartDesign::Plane"
            | "PartDesign::Line"
            | "PartDesign::Point"
            | "PartDesign::CoordinateSystem"
            | "Part::DatumPlane"
            | "Part::DatumLine"
            | "Part::DatumPoint"
            | "Part::LocalCoordinateSystem"
    )
}

/// A PartDesign feature of a Body's chain of solids (not a datum, a binder
/// or a boolean's group).
pub(super) fn is_solid_feature(t: &str) -> bool {
    t.starts_with("PartDesign::")
        && !is_datum(t)
        && !matches!(
            t,
            "PartDesign::Body" | "PartDesign::ShapeBinder" | "PartDesign::SubShapeBinder"
        )
}

/// The Part workbench objects the replay builds.
pub(super) fn is_part_op(t: &str) -> bool {
    matches!(
        t,
        "Part::Box"
            | "Part::Cylinder"
            | "Part::Sphere"
            | "Part::Torus"
            | "Part::Cone"
            | "Part::Prism"
            | "Part::Wedge"
            | "Part::Ellipsoid"
            | "Part::Extrusion"
            | "Part::Revolution"
            | "Part::Cut"
            | "Part::Fuse"
            | "Part::MultiFuse"
            | "Part::Common"
            | "Part::MultiCommon"
            | "Part::Fillet"
            | "Part::Chamfer"
            | "Part::Mirroring"
    )
}

/// Whether a Part workbench result is worth replaying: a primitive with its
/// sizes, an extrusion or revolution of a sketch into a solid, an
/// operation on other objects. Others keep their stored shapes.
fn replayable_root(o: &Object, doc: &mitcad_freecad::Document) -> bool {
    let needs: &[&str] = match o.type_name.as_str() {
        "Part::Extrusion" | "Part::Revolution" => {
            let profile = o.link("Base").or_else(|| o.link("Source"));
            return o.bool("Solid") == Some(true)
                && profile
                    .and_then(|l| doc.object(&l.object))
                    .is_some_and(|p| super::is_sketch(&p.type_name));
        }
        t => t
            .strip_prefix("Part::")
            .and_then(super::primitives::part_kind)
            .map_or(&[], super::primitives::primitive_sizes),
    };
    needs.iter().all(|p| o.f64(p).is_some())
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// Decides what the history replays: the roots (planned bodies), and
    /// every object of their trees.
    pub(super) fn plan_replay(&mut self) {
        let source = &self.sources[0];
        let doc = &source.file.document;
        let mut roots = HashMap::new();
        for (index, p) in self.planned.iter().enumerate() {
            if p.key.doc != 0 || p.owner.as_deref() != Some(p.key.name.as_str()) {
                continue;
            }
            let Some(o) = doc.object(&p.key.name) else {
                continue;
            };
            let replay = if o.type_name == "PartDesign::Body" {
                o.links("Group").iter().any(|l| {
                    doc.object(&l.object)
                        .is_some_and(|f| is_solid_feature(&f.type_name) && f.shape().is_some())
                })
            } else {
                // A Part result in its own frame (a link's target) keeps
                // its stored shape.
                p.transform.is_none() && is_part_op(&o.type_name) && replayable_root(o, doc)
            };
            if replay {
                roots.insert(p.key.name.clone(), index);
            }
        }
        // The trees.
        let mut replayed = HashSet::new();
        let mut stack: Vec<String> = roots.keys().cloned().collect();
        while let Some(name) = stack.pop() {
            let Some(o) = doc.object(&name) else {
                continue;
            };
            if !replayed.insert(name.clone()) {
                continue;
            }
            let t = o.type_name.as_str();
            if t == "PartDesign::Body" || t == "PartDesign::Boolean" {
                stack.extend(
                    o.links("Group")
                        .iter()
                        .filter(|l| l.file.is_none())
                        .map(|l| l.object.clone()),
                );
            } else if !t.starts_with("PartDesign::") && !super::is_sketch(t) {
                // A Part object's operands.
                for (property, link) in o.references() {
                    if link.file.is_none()
                        && !NOT_CONSUMING.contains(&property)
                        && doc.object(&link.object).is_some_and(|x| {
                            x.shape().is_some() && !super::is_construction(&x.type_name)
                        })
                    {
                        stack.push(link.object.clone());
                    }
                }
            }
        }
        // A MultiTransform's transformations are part of it, not features
        // of their own.
        for o in &doc.objects {
            if o.type_name == "PartDesign::MultiTransform" {
                for link in o.links("Transformations") {
                    replayed.remove(&link.object);
                }
            }
        }
        // Operands' uses.
        let mut uses: HashMap<String, usize> = HashMap::new();
        for name in &replayed {
            let Some(o) = doc.object(name) else { continue };
            if o.type_name.starts_with("PartDesign::") || super::is_sketch(&o.type_name) {
                continue;
            }
            let mut seen = BTreeSet::new();
            for (property, link) in o.references() {
                if !NOT_CONSUMING.contains(&property)
                    && replayed.contains(&link.object)
                    && seen.insert(link.object.clone())
                {
                    *uses.entry(link.object.clone()).or_default() += 1;
                }
            }
        }
        // Sketches whose axes something uses.
        let mut axis_uses: HashMap<String, [bool; 2]> = HashMap::new();
        for o in &doc.objects {
            for (_, link) in o.references() {
                for sub in &link.subs {
                    let which = match super::elements::sub_name(sub) {
                        "H_Axis" => 0,
                        "V_Axis" => 1,
                        _ => continue,
                    };
                    axis_uses.entry(link.object.clone()).or_default()[which] = true;
                }
            }
        }
        self.replay.roots = roots;
        self.replay.replayed = replayed;
        self.replay.uses = uses;
        self.replay.axis_uses = axis_uses;
    }

    /// The objects of the timeline in the order of their dependencies.
    fn timeline(&self) -> Vec<String> {
        let doc = &self.sources[0].file.document;
        let wanted: HashSet<&str> = doc
            .objects
            .iter()
            .filter(|o| super::is_sketch(&o.type_name) || self.replay.replayed.contains(&o.name))
            .map(|o| o.name.as_str())
            .collect();
        let mut needs: HashMap<&str, BTreeSet<&str>> = HashMap::new();
        let mut users: HashMap<&str, Vec<&str>> = HashMap::new();
        for o in doc
            .objects
            .iter()
            .filter(|o| wanted.contains(o.name.as_str()))
        {
            let group = matches!(
                o.type_name.as_str(),
                "PartDesign::Body" | "PartDesign::Boolean"
            );
            let mut deps = BTreeSet::new();
            for (property, link) in o.references() {
                let skip = match property {
                    "Group" => !group,
                    "Origin" | "OriginFeatures" | "Tip" | "ElementList" | "LinkedObject"
                    | "_Body" => true,
                    _ => false,
                };
                if skip || link.file.is_some() || link.object == o.name {
                    continue;
                }
                if let Some(&dep) = wanted.get(link.object.as_str()) {
                    deps.insert(dep);
                }
            }
            for dep in &deps {
                users.entry(dep).or_default().push(o.name.as_str());
            }
            needs.insert(o.name.as_str(), deps);
        }
        // Kahn's order, ties by the document's order.
        let position = |name: &str| doc.position(name).unwrap_or(usize::MAX);
        let mut ready: BTreeSet<(usize, &str)> = needs
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(name, _)| (position(name), *name))
            .collect();
        let mut left: HashMap<&str, usize> = needs.iter().map(|(n, d)| (*n, d.len())).collect();
        let mut order = Vec::new();
        while let Some(first) = ready.iter().next().copied() {
            ready.remove(&first);
            order.push(first.1.to_owned());
            for user in users.get(first.1).into_iter().flatten() {
                let count = left.get_mut(user).expect("an object of the timeline");
                *count -= 1;
                if *count == 0 {
                    ready.insert((position(user), user));
                }
            }
        }
        // Cycles: the rest in the document's order.
        let done: HashSet<String> = order.iter().cloned().collect();
        let mut rest: Vec<&str> = needs
            .keys()
            .copied()
            .filter(|n| !done.contains(*n))
            .collect();
        rest.sort_by_key(|n| position(n));
        order.extend(rest.into_iter().map(str::to_owned));
        order
    }

    /// The timeline: sketches, datums and the features of the replayed
    /// bodies, then the bodies' names, visibility and tips.
    pub(super) fn import_history(&mut self) {
        let order = self.timeline();
        for name in order {
            let Some(o) = self.sources[0].object(&name).cloned() else {
                continue;
            };
            let t = o.type_name.as_str();
            if super::is_sketch(t) {
                self.sketch_item(&name);
            } else if t == "PartDesign::Body" {
                self.finish_body(&o);
            } else if is_datum(t) {
                self.datum_item(&o);
            } else if is_solid_feature(t) {
                self.feature_item(&o);
            } else if t.starts_with("PartDesign::") {
                let body = self.sources[0].body_of(&o.name).map(str::to_owned);
                self.feature_skipped(
                    &o,
                    body,
                    "a binder: Mitcad has none (sketches take its geometry as fixed geometry)",
                );
            } else {
                self.part_item(&o);
            }
        }
        self.finish_roots();
    }

    // Shared steps.

    /// Adds a candidate's definitions as features named after `label` and
    /// their suffixes (a definition's `$<i>` is the i-th one added), Err
    /// when one does not evaluate (then none stays).
    pub(super) fn add_defs(
        &mut self,
        candidate: &Candidate,
        label: &str,
        component: ComponentUid,
    ) -> Result<Vec<FeatureUid>, String> {
        let depth = self.doc.undo_depth();
        let mut uids: Vec<FeatureUid> = Vec::new();
        for (i, def) in candidate.defs.iter().enumerate() {
            let mut def = def.clone();
            self.substitute_regions(&mut def, &uids);
            substitute(&mut def, &uids);
            let suffix = candidate.suffixes.get(i).copied().unwrap_or_default();
            let name = format!("{label}{suffix}");
            match self.add_named(&def, &name, component) {
                Ok(uid) => {
                    // Construction geometry and sketches made on the way
                    // are hidden.
                    if !suffix.is_empty()
                        && def["type"]
                            .as_str()
                            .is_some_and(|t| t.starts_with("construction_") || t == "sketch")
                    {
                        let _ = self.doc.set_feature_visible(uid, false);
                    }
                    uids.push(uid);
                }
                Err(e) => {
                    self.undo_to(depth);
                    return Err(e);
                }
            }
        }
        Ok(uids)
    }

    /// Replaces `"$<i>:region"` by the one region of the sketch added
    /// i-th.
    fn substitute_regions(&self, def: &mut Value, uids: &[FeatureUid]) {
        match def {
            Value::String(s) if s.starts_with('$') && s.ends_with(":region") => {
                if let Ok(i) = s[1..s.len() - ":region".len()].parse::<usize>()
                    && let Some(uid) = uids.get(i)
                    && let Some(output) = self.doc.sketch_output(*uid)
                    && let [region] = output.regions.as_slice()
                {
                    *s = region.key.to_string();
                }
            }
            Value::Array(items) => items
                .iter_mut()
                .for_each(|v| self.substitute_regions(v, uids)),
            Value::Object(map) => map
                .values_mut()
                .for_each(|v| self.substitute_regions(v, uids)),
            _ => {}
        }
    }

    /// Adds a definition named `label` (Mitcad's default name when it is
    /// taken); Err when it does not evaluate.
    pub(super) fn add_named(
        &mut self,
        def: &Value,
        label: &str,
        component: ComponentUid,
    ) -> Result<FeatureUid, String> {
        let def: FeatureDef<ValueInput> =
            serde_json::from_value(def.clone()).map_err(|e| format!("definition: {e}"))?;
        let name = Some(label).filter(|l| !l.trim().is_empty());
        let added = match self.doc.add_feature_to(&def, name, Some(component)) {
            Ok(added) => added,
            Err(e) if name.is_some() && e.to_string().contains("already named") => self
                .doc
                .add_feature_to(&def, None, Some(component))
                .map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        };
        match self.doc.status(added.uid).and_then(|s| s.error()) {
            None => Ok(added.uid),
            Some(error) => {
                let error = error.to_owned();
                self.doc.undo();
                Err(error)
            }
        }
    }

    /// The bodies of a component now.
    pub(super) fn component_bodies(&self, component: ComponentUid) -> Vec<BodyUid> {
        self.doc
            .bodies()
            .iter()
            .filter(|b| self.doc.body_component(b.uid) == Some(component))
            .map(|b| b.uid)
            .collect()
    }

    /// An object's stored shape in its component's coordinates.
    pub(super) fn stored_in(&mut self, object: &str, transform: &Transform) -> Option<K::Shape> {
        let shape = self.stored_shape(object)?;
        if transform.is_identity() {
            return Some(shape);
        }
        self.doc
            .kernel()
            .transform_shape(&shape, transform, None)
            .ok()
    }

    /// Mitcad's bodies against an object's stored shape.
    pub(super) fn check(&mut self, object: &Object, bodies: &[BodyUid], place: &Placed) -> Check {
        if object.state.is_stale() {
            return Check::Unchecked("FreeCAD had not recomputed it: not compared".to_owned());
        }
        let Some(stored) = self.stored_in(&object.name, &place.transform) else {
            return Check::Unchecked("no stored shape to compare with".to_owned());
        };
        let kernel = self.doc.kernel();
        let Some(theirs) = measure(kernel, &stored) else {
            return Check::Unchecked("the geometry kernel does not measure shapes".to_owned());
        };
        let their_solids = kernel.solids(&stored).map_or(0, |s| s.len());
        // Solids by volume; shapes without (sheets) by area.
        let sheets = theirs.volume.abs() <= 1e-9;
        let weight = |m: &mitcad_model::MassProperties| if sheets { m.area } else { m.volume };
        let (mut volume, mut area, mut moment, mut total, mut solids) =
            (0.0, 0.0, [0.0; 3], 0.0, 0);
        for body in bodies {
            let Some(shape) = self.doc.body_shape(*body) else {
                continue;
            };
            let Some(m) = measure(kernel, shape) else {
                return Check::Unchecked("the geometry kernel does not measure shapes".to_owned());
            };
            solids += kernel.solids(shape).map_or(1, |s| s.len());
            volume += m.volume;
            area += m.area;
            total += weight(&m);
            for (k, c) in moment.iter_mut().enumerate() {
                *c += weight(&m) * m.center[k];
            }
        }
        let center = if total.abs() > 0.0 {
            moment.map(|c| c / total)
        } else {
            [0.0; 3]
        };
        let rel = |a: f64, b: f64| (a - b).abs() / a.abs().max(b.abs()).max(1e-9);
        let size = if sheets {
            theirs.area.abs().sqrt().max(1e-3)
        } else {
            theirs.volume.abs().cbrt().max(1e-3)
        };
        let off = (0..3)
            .map(|k| (center[k] - theirs.center[k]).powi(2))
            .sum::<f64>()
            .sqrt();
        let distance = if sheets {
            0.0
        } else {
            rel(volume, theirs.volume.abs())
        }
        .max(rel(area, theirs.area))
        .max(off / size);
        Check::Measured(FeatureCheck {
            volume,
            area,
            center,
            solids,
            freecad_volume: theirs.volume.abs(),
            freecad_area: theirs.area,
            freecad_center: theirs.center,
            freecad_solids: their_solids,
            distance,
            pass: distance <= TOLERANCE && solids == their_solids,
        })
    }

    /// Whether an object's stored shape is what the bodies are now (it
    /// changes nothing).
    fn unchanged(&mut self, object: &Object, bodies: &[BodyUid], place: &Placed) -> bool {
        !bodies.is_empty()
            && matches!(self.check(object, bodies, place), Check::Measured(c) if c.pass)
    }

    /// A base feature of an object's stored shape in place of `replaces`
    /// (the first keeps its id; the others go): its uid and the body.
    pub(super) fn fallback(
        &mut self,
        object: &Object,
        place: &Placed,
        replaces: &[BodyUid],
    ) -> Result<(FeatureUid, BodyUid), String> {
        let shape = self
            .stored_in(&object.name, &place.transform)
            .ok_or("no stored shape to fall back on")?;
        let label = object.label().to_owned();
        let base = BaseInput {
            name: Some(label.clone()),
            source: Some(self.report.file.clone()),
            replaces: replaces.to_vec(),
            component: Some(place.component),
            ..BaseInput::new(vec![ImportBody {
                name: None,
                color: None,
                shape,
            }])
        };
        let imported = match self.doc.add_base_feature(base.clone()) {
            Ok(i) => i,
            Err(e) if e.to_string().contains("already named") => self
                .doc
                .add_base_feature(BaseInput { name: None, ..base })
                .map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        };
        let uid = imported.feature.uid;
        if let Some(e) = self.doc.status(uid).and_then(|s| s.error()) {
            let e = e.to_owned();
            self.doc.undo();
            return Err(e);
        }
        let body = replaces
            .first()
            .copied()
            .unwrap_or_else(|| BodyUid::new(uid, 0));
        Ok((uid, body))
    }

    /// Records the bodies holding an object's result, as they are now.
    pub(super) fn record(&mut self, name: &str, bodies: &[BodyUid]) {
        let made = bodies
            .iter()
            .filter_map(|b| Some((*b, self.doc.body_shape(*b)?.clone())))
            .collect();
        self.replay
            .results
            .insert(name.to_owned(), Made { bodies: made });
    }

    /// Replays an object: its candidates in turn, each checked; else the
    /// fallback. Returns the report and the bodies holding the result (the
    /// target's that are left, and new ones). An empty `mitcad_type` is
    /// the main definition's.
    pub(super) fn replay_object(
        &mut self,
        object: &Object,
        target: &Target,
        candidates: Result<Vec<Candidate>, String>,
        mitcad_type: &str,
    ) -> (FeatureReport, Vec<BodyUid>) {
        let mut report = FeatureReport {
            object: object.name.clone(),
            label: object.label().to_owned(),
            object_type: object.type_name.clone(),
            body: target.body.clone(),
            ..FeatureReport::default()
        };
        let component = target.place.component;
        let suppressed = object.bool("Suppressed") == Some(true);
        let mut last = String::new();
        let mut first: Option<String> = None;
        let mut closest: Option<FeatureCheck> = None;
        match candidates {
            Ok(candidates) => {
                for (tried, candidate) in candidates.iter().enumerate() {
                    let before: BTreeSet<BodyUid> =
                        self.component_bodies(component).into_iter().collect();
                    let depth = self.doc.undo_depth();
                    let uids = match self.add_defs(candidate, object.label(), component) {
                        Ok(uids) => uids,
                        Err(e) => {
                            last = e;
                            first.get_or_insert_with(|| last.clone());
                            continue;
                        }
                    };
                    let bodies =
                        result_bodies(&target.current, &before, &self.component_bodies(component));
                    if suppressed {
                        // FreeCAD's result is its base's: the definition
                        // only needs to build.
                        for uid in &uids {
                            let _ = self.doc.set_suppressed(*uid, true);
                        }
                        report.outcome = ObjectOutcome::Partial;
                        report.notes.push(
                            "suppressed in FreeCAD: replayed and suppressed, not compared"
                                .to_owned(),
                        );
                        report.notes.extend(candidate.notes.iter().cloned());
                        report.features = uids.iter().map(ToString::to_string).collect();
                        report.mitcad = candidate.mitcad_type(mitcad_type);
                        let current = target.current.clone();
                        self.adopt_properties(&object.name, &uids);
                        self.replay.features.insert(object.name.clone(), uids);
                        return (report, current);
                    }
                    let check = self.check(object, &bodies, &target.place);
                    if check.pass() {
                        report.outcome = if candidate.partial {
                            ObjectOutcome::Partial
                        } else {
                            ObjectOutcome::Parametric
                        };
                        report.notes.extend(candidate.notes.iter().cloned());
                        if tried > 0 {
                            report.notes.push(format!(
                                "reading {} of {} of FreeCAD's definition",
                                tried + 1,
                                candidates.len()
                            ));
                        }
                        match check {
                            Check::Measured(c) => report.check = Some(c),
                            Check::Unchecked(why) => report.notes.push(why),
                        }
                        report.features = uids.iter().map(ToString::to_string).collect();
                        report.mitcad = candidate.mitcad_type(mitcad_type);
                        if let Some(main) = uids.get(candidate.main()) {
                            self.replay.main.insert(object.name.clone(), *main);
                        }
                        self.adopt_properties(&object.name, &uids);
                        self.replay.features.insert(object.name.clone(), uids);
                        self.replay.parametric.insert(object.name.clone());
                        self.record(&object.name, &bodies);
                        return (report, bodies);
                    }
                    if let Check::Measured(c) = check {
                        last = format!(
                            "its result is off FreeCAD's by {:.1e} ({} solids against {})",
                            c.distance, c.solids, c.freecad_solids
                        );
                        if closest.as_ref().is_none_or(|b| c.distance < b.distance) {
                            closest = Some(c);
                        }
                    }
                    self.undo_to(depth);
                    first.get_or_insert_with(|| last.clone());
                }
                if candidates.is_empty() {
                    last = "nothing to try".to_owned();
                }
            }
            Err(why) => last = why,
        }
        // The first reading's failure, and the last's when it differs.
        if let Some(first) = first.filter(|f| *f != last) {
            last = format!("{first}; the last reading: {last}");
        }
        // Not replayed: unchanged, suppressed, or the stored shape.
        if suppressed {
            report.outcome = ObjectOutcome::Skipped;
            report
                .notes
                .push(format!("suppressed in FreeCAD; not replayed: {last}"));
            return (report, target.current.clone());
        }
        if self.unchanged(object, &target.current, &target.place) {
            report.outcome = ObjectOutcome::Skipped;
            report.notes.push(format!(
                "FreeCAD's result is the shape before it (it changes nothing); not replayed: {last}"
            ));
            let current = target.current.clone();
            self.record(&object.name, &current);
            return (report, current);
        }
        report.notes.push(format!("not replayed: {last}"));
        if let Some(c) = closest {
            report.check = Some(c);
        }
        match self.fallback(object, &target.place, &target.current) {
            Ok((uid, body)) => {
                report.outcome = ObjectOutcome::Fallback;
                report.features = vec![uid.to_string()];
                report.mitcad = "base".to_owned();
                self.replay.features.insert(object.name.clone(), vec![uid]);
                self.record(&object.name, &[body]);
                (report, vec![body])
            }
            Err(e) => {
                report.outcome = ObjectOutcome::Skipped;
                report.notes.push(format!("no fallback: {e}"));
                (report, target.current.clone())
            }
        }
    }

    /// A PartDesign feature of a replayed Body.
    fn feature_item(&mut self, object: &Object) {
        let source = &self.sources[0];
        let body = source.body_of(&object.name).map(str::to_owned);
        let Some(place) = self.place_of(&object.name, false) else {
            self.feature_skipped(object, body, "its container was not imported");
            return;
        };
        let current = body
            .as_ref()
            .and_then(|b| self.replay.bodies.get(b).cloned())
            .unwrap_or_default();
        let target = Target {
            place,
            body: body.clone(),
            current,
        };
        let (candidates, mitcad) = self.feature_candidates(object, &target);
        let (report, bodies) = self.replay_object(object, &target, candidates, mitcad);
        if let Some(b) = &body {
            self.replay.bodies.insert(b.clone(), bodies);
        }
        self.push_feature(object, report);
    }

    pub(super) fn feature_skipped(&mut self, object: &Object, body: Option<String>, why: &str) {
        let report = FeatureReport {
            object: object.name.clone(),
            label: object.label().to_owned(),
            object_type: object.type_name.clone(),
            body,
            outcome: ObjectOutcome::Skipped,
            notes: vec![why.to_owned()],
            ..FeatureReport::default()
        };
        self.push_feature(object, report);
    }

    pub(super) fn push_feature(&mut self, object: &Object, report: FeatureReport) {
        let note = match report.outcome {
            ObjectOutcome::Parametric | ObjectOutcome::Partial => Some(format!(
                "{} ({})",
                if report.mitcad.is_empty() {
                    "a feature"
                } else {
                    report.mitcad.as_str()
                },
                report.features.join(", ")
            )),
            ObjectOutcome::Fallback => Some(format!(
                "a base feature of its stored shape ({}): {}",
                report.features.join(", "),
                report.notes.last().cloned().unwrap_or_default()
            )),
            _ => report.notes.first().cloned(),
        };
        // A root keeps its outcome (a body); its features' notes say what
        // they became.
        if !self.replay.roots.contains_key(&object.name) {
            self.roles
                .insert(object.name.clone(), (report.outcome, note));
        }
        self.report.features.push(report);
    }

    /// The end of a Body's features: the features after its tip are
    /// suppressed.
    fn finish_body(&mut self, body: &Object) {
        let doc = &self.sources[0].file.document;
        let Some(tip) = body.link("Tip").map(|l| l.object.clone()) else {
            return;
        };
        let solids: Vec<String> = body
            .links("Group")
            .iter()
            .filter(|l| {
                doc.object(&l.object)
                    .is_some_and(|f| is_solid_feature(&f.type_name) && f.shape().is_some())
            })
            .map(|l| l.object.clone())
            .collect();
        let Some(at) = solids.iter().position(|f| *f == tip) else {
            return;
        };
        let after: Vec<String> = solids[at + 1..].to_vec();
        for name in after {
            for uid in self.replay.features.get(&name).cloned().unwrap_or_default() {
                if let Err(e) = self.doc.set_suppressed(uid, true) {
                    self.warn(format!("{name}: {e}"));
                }
            }
            if let Some(f) = self.report.features.iter_mut().find(|f| f.object == name) {
                f.notes
                    .push("after its Body's tip: suppressed (rolled back)".to_owned());
            }
        }
    }

    /// The replayed roots' bodies: the planned bodies, named, shown or
    /// hidden as FreeCAD has them.
    fn finish_roots(&mut self) {
        let mut roots: Vec<(String, usize)> = self
            .replay
            .roots
            .iter()
            .map(|(n, i)| (n.clone(), *i))
            .collect();
        roots.sort_by_key(|(_, i)| *i);
        let mut colored = Vec::new();
        for (name, index) in roots {
            let bodies = match self.replay.bodies.get(&name) {
                Some(b) => b.clone(),
                None => self
                    .replay
                    .results
                    .get(&name)
                    .map(|m| m.bodies.iter().map(|(b, _)| *b).collect())
                    .unwrap_or_default(),
            };
            let bodies: Vec<BodyUid> = bodies
                .into_iter()
                .filter(|b| self.doc.body_shape(*b).is_some())
                .collect();
            let Some(&first) = bodies.first() else {
                let why = "the replay left no body".to_owned();
                self.warn(format!("{name}: {why}"));
                self.roles
                    .insert(name.clone(), (ObjectOutcome::Skipped, Some(why)));
                continue;
            };
            if bodies.len() > 1 {
                self.warn(format!(
                    "{name}: the replay made {} bodies; the first stands for it",
                    bodies.len()
                ));
            }
            let planned = &self.planned[index];
            let (label, visible, color) = (planned.name.clone(), planned.visible, planned.color);
            self.planned[index].body = Some(first);
            if self.doc.body_name(first) != label
                && let Err(e) = self.doc.rename_body(first, &label)
            {
                self.warn(format!("{label}: {e}"));
            }
            if !visible && let Err(e) = self.doc.set_body_visible(first, false) {
                self.warn(format!("{label}: {e}"));
            }
            if color.is_some() {
                colored.push(label);
            }
            self.replayed_bodies.insert(first);
        }
        if !colored.is_empty() {
            self.warn(format!(
                "the colours of {} are not kept (bodies the history makes have Mitcad's look)",
                colored.join(", ")
            ));
        }
    }

    /// A datum: a construction feature.
    fn datum_item(&mut self, object: &Object) {
        let body = self.sources[0].body_of(&object.name).map(str::to_owned);
        let Some(place) = self.place_of(&object.name, false) else {
            self.feature_skipped(object, body, "its container was not imported");
            return;
        };
        let readings = match self.datum_def(object, &place) {
            Ok(d) => d,
            Err(why) => {
                self.feature_skipped(object, body, &why);
                return;
            }
        };
        let mut report = FeatureReport {
            object: object.name.clone(),
            label: object.label().to_owned(),
            object_type: object.type_name.clone(),
            body,
            ..FeatureReport::default()
        };
        let mut last = String::new();
        let mut taken = false;
        for (i, reading) in readings.iter().enumerate() {
            let added = match &reading.check {
                Some(frame) => self
                    .added_plane(&reading.def, frame, place.component, object.label())
                    .map(|(uid, _)| uid),
                None => self.add_named(&reading.def, object.label(), place.component),
            };
            match added {
                Ok(uid) => {
                    let _ = self.doc.set_feature_visible(uid, false);
                    report.outcome = ObjectOutcome::Parametric;
                    report.features = vec![uid.to_string()];
                    report.mitcad = reading.def["type"].as_str().unwrap_or_default().to_owned();
                    report.notes = reading.notes.clone();
                    self.replay.datums.insert(object.name.clone(), uid);
                    taken = true;
                    // The readings tried before it carry nothing now.
                    for earlier in &readings[..i] {
                        for path in &earlier.carries {
                            if !reading.carries.contains(path) {
                                self.left_out(object, path, &format!("its plane: {last}"));
                            }
                        }
                    }
                    break;
                }
                Err(e) => last = e,
            }
        }
        if !taken {
            report.outcome = ObjectOutcome::Skipped;
            report.notes.push(format!("not imported: {last}"));
        }
        self.push_feature(object, report);
    }

    /// The enumeration's value as text.
    pub(super) fn enum_text(&self, object: &Object, property: &str) -> Option<String> {
        self.sources[0].file.document.enum_text(object, property)
    }
}

/// The bodies of a result: those it worked on that still exist, and the
/// new ones.
fn result_bodies(
    current: &[BodyUid],
    before: &BTreeSet<BodyUid>,
    after: &[BodyUid],
) -> Vec<BodyUid> {
    let mut out: Vec<BodyUid> = current
        .iter()
        .copied()
        .filter(|b| after.contains(b))
        .collect();
    out.extend(after.iter().copied().filter(|b| !before.contains(b)));
    out
}

/// Replaces `"$<i>"` strings in a definition by the uids of the features
/// added before it, also as body prefixes (`"$0.b0"`).
fn substitute(def: &mut Value, uids: &[FeatureUid]) {
    match def {
        Value::String(s) if s.starts_with('$') => {
            let (index, rest) = s[1..].split_at(
                s[1..]
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(s.len() - 1),
            );
            if let Ok(i) = index.parse::<usize>()
                && let Some(uid) = uids.get(i)
            {
                *s = format!("{uid}{rest}");
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| substitute(v, uids)),
        Value::Object(map) => map.values_mut().for_each(|v| substitute(v, uids)),
        _ => {}
    }
}
