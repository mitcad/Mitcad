// SPDX-License-Identifier: MIT
//! `.solidcraft` designs with their history (mitcad#450), read by
//! `mitcad-solidcraft` (upgraded to one format first) and built again as
//! MitCad features through the model's commands, on the [`Kernel`] trait.
//!
//! The file stores definitions only (no bodies, apart from imported STEP
//! text and meshes), so every feature is evaluated by MitCad; there is no
//! stored body to fall back to. A feature that MitCad has no counterpart
//! for, or whose MitCad feature fails, is named in the report and holds its
//! place on the timeline as a suppressed placeholder with a note; the
//! timeline goes on after it.
//!
//! - Parameters ([`params`]): the file's user parameters, the sketch
//!   dimensions' and the feature inputs' named parameters keep their names
//!   and expressions (translated, `mitcad_solidcraft::expr`); a feature's
//!   input is its slot's parameter (`d3 = height`).
//! - Components and occurrences come first, with their placements; each
//!   feature goes into its component, whose coordinates it is in.
//! - Sketches ([`sketch`]): points, curves, constraints and dimensions,
//!   on the plane the file names ([`planes`]), in the file's sketch frame.
//! - Features ([`features`]): references to faces and edges are points on
//!   them in the file; they are found on the rebuilt bodies at those
//!   points ([`refs`]) and named by MitCad's topological names, which
//!   follow later edits. Bodies are named as the file names them.
//! - Joints ([`assembly`]), sheet metal ([`sheet`]).

mod assembly;
mod features;
mod imports;
mod names;
mod params;
mod planes;
mod refs;
pub mod report;
mod sheet;
mod sketch;
// Patch, Stitch and Thicken (mitcad#456).
mod surfaces;
// Surface modelling: surface bodies and their features (mitcad#455).
mod surface_modelling;
// Plastic part features and rules (mitcad#459).
mod plastic;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use mitcad_model::{
    BodyUid, ComponentUid, Document, FeatureDef, FeatureUid, Kernel, OccurrenceUid, Transform,
    ValueInput,
};
use mitcad_solidcraft::{Design, Feature, SolidcraftFile};
use serde_json::Value;

pub use report::{
    BodyReport, CheckReport, FeatureReport, JointReport, Outcome, ParameterReport, SketchReport,
    SolidcraftReport,
};

/// Import settings.
#[derive(Debug, Clone, Default)]
pub struct SolidcraftOptions {
    /// Merge the import into one undo step with this label.
    pub undo_label: Option<String>,
    /// Parameters to change after the import (the file's names), before
    /// the report measures the bodies.
    pub set_parameters: Vec<(String, String)>,
    /// Values to check the import against ([`check`]).
    pub expected: Option<Value>,
}

/// Imports the design at `path` into the document (normally a new one).
pub fn import_solidcraft<K: Kernel>(
    doc: &mut Document<K>,
    path: &Path,
    options: &SolidcraftOptions,
) -> Result<SolidcraftReport, String> {
    let file = SolidcraftFile::open(path)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    import_solidcraft_file(doc, file, &name, options)
}

/// Imports a design read; `name` is the file's name for the report.
pub fn import_solidcraft_file<K: Kernel>(
    doc: &mut Document<K>,
    file: SolidcraftFile,
    name: &str,
    options: &SolidcraftOptions,
) -> Result<SolidcraftReport, String> {
    let depth = doc.undo_depth();
    let report = SolidcraftReport {
        file: name.to_owned(),
        design: file.design.name.clone(),
        format: file.saved_format,
        upgrades: file.upgrades.clone(),
        container: file.container,
        units: file.design.units.clone(),
        ..SolidcraftReport::default()
    };
    let design = file.design;
    let mut importer = Importer {
        doc,
        design: &design,
        report,
        made: HashMap::new(),
        sketches: HashMap::new(),
        planes: HashMap::new(),
        components: HashMap::new(),
        occurrences: HashMap::new(),
        bodies: BTreeMap::new(),
        body_counter: 0,
        params: params::Params::new(&design),
        notes: Vec::new(),
        sheet_rule_features: HashMap::new(),
        sheet_bases: HashMap::new(),
        sheet_unfolds: HashMap::new(),
        plastic_rules: HashMap::new(),
    };
    importer.run(options)?;
    let Importer {
        doc,
        mut report,
        notes,
        ..
    } = importer;
    if let Some(e) = mitcad_import_base::scaffold::finish_import(
        doc,
        depth,
        notes,
        options.undo_label.as_deref(),
    ) {
        report.notes.push(format!("timeline notes: {e}"));
    }
    if let Some(expected) = &options.expected {
        report.check = Some(check(&report, expected));
    }
    Ok(report)
}

/// What a feature of the file became.
#[derive(Debug, Clone, Default)]
pub(crate) struct Made {
    /// The MitCad features, the last being the one others refer to.
    pub features: Vec<FeatureUid>,
}

impl Made {
    pub fn last(&self) -> Option<FeatureUid> {
        self.features.last().copied()
    }
}

pub(crate) struct Importer<'d, 'f, K: Kernel> {
    pub doc: &'d mut Document<K>,
    pub design: &'f Design,
    pub report: SolidcraftReport,
    /// By the file's feature id.
    pub made: HashMap<u64, Made>,
    /// The sketches made, by the file's feature id.
    pub sketches: HashMap<u64, sketch::SketchMade>,
    /// The world frames of construction planes made, by the file's id.
    pub planes: HashMap<u64, planes::Frame>,
    pub components: HashMap<u64, ComponentUid>,
    pub occurrences: HashMap<u64, OccurrenceUid>,
    /// The file's body names and the MitCad bodies they are.
    pub bodies: BTreeMap<String, BodyUid>,
    /// SolidCraft's counter of `Body<n>` names.
    pub body_counter: usize,
    pub params: params::Params,
    /// Notes on the timeline (mitcad#170).
    pub notes: Vec<(FeatureUid, String)>,
    /// Sheet rules made, by the file's rule name and side.
    pub sheet_rule_features: HashMap<(String, bool), FeatureUid>,
    /// Sheet bodies and the Face that made them.
    pub sheet_bases: HashMap<BodyUid, FeatureUid>,
    /// The last Unfold of each sheet body.
    pub sheet_unfolds: HashMap<BodyUid, FeatureUid>,
    /// Plastic rules made, by the file's rule name (mitcad#459).
    pub plastic_rules: HashMap<String, FeatureUid>,
}

/// The result of mapping one feature.
pub(crate) struct Mapped {
    pub outcome: Outcome,
    pub notes: Vec<String>,
}

impl Mapped {
    pub fn parametric() -> Self {
        Self {
            outcome: Outcome::Parametric,
            notes: Vec::new(),
        }
    }

    pub fn partial(notes: Vec<String>) -> Self {
        Self {
            outcome: if notes.is_empty() {
                Outcome::Parametric
            } else {
                Outcome::Partial
            },
            notes,
        }
    }
}

/// Why a feature did not come in: no counterpart in MitCad (`unmapped`),
/// or it failed.
pub(crate) enum NotMapped {
    Unmapped(String),
    Failed(String),
}

impl From<String> for NotMapped {
    fn from(e: String) -> Self {
        NotMapped::Failed(e)
    }
}

impl From<&str> for NotMapped {
    fn from(e: &str) -> Self {
        NotMapped::Failed(e.to_owned())
    }
}

pub(crate) type MapResult = Result<Mapped, NotMapped>;

impl<K: Kernel> Importer<'_, '_, K> {
    fn run(&mut self, options: &SolidcraftOptions) -> Result<(), String> {
        self.make_components()?;
        let active = self.design.active_len();
        let features: Vec<Feature> = self.design.features.clone();
        let mut marker = None;
        for (index, f) in features.iter().enumerate() {
            if index == active {
                marker = Some(self.doc.features().count());
            }
            self.feature(f, index >= active);
        }
        self.params.make_rest(self.doc, &mut self.report);
        assembly::joints(self);
        self.visibility();
        // Point clouds (format 43): references only, which MitCad keeps none of.
        let clouds = self.design.point_clouds.len();
        if clouds > 0 {
            self.report.notes.push(format!(
                "{clouds} point cloud{} (scanned reference points): not mapped",
                if clouds > 1 { "s" } else { "" }
            ));
        }
        if let Some(position) = marker
            && let Err(e) = self.doc.set_marker(position)
        {
            self.report.notes.push(format!("timeline marker: {e}"));
        }
        for (name, expression) in &options.set_parameters {
            match self.params.set(self.doc, name, expression) {
                Ok(()) => self.report.changed.push((name.clone(), expression.clone())),
                Err(e) => return Err(format!("set {name}: {e}")),
            }
        }
        self.doc.recompute();
        self.measure();
        Ok(())
    }

    /// One feature of the file: mapped, or a placeholder.
    fn feature(&mut self, f: &Feature, rolled_back: bool) {
        let component = self.component_of(f.component);
        let before = self.doc.features().count();
        let result = features::map(self, f, component);
        let after: Vec<FeatureUid> = self.doc.features().skip(before).map(|e| e.uid).collect();
        let mut entry = FeatureReport {
            id: f.id,
            name: f.name.clone(),
            kind: f.kind.clone(),
            outcome: Outcome::Parametric,
            features: Vec::new(),
            suppressed: f.suppressed,
            rolled_back,
            notes: Vec::new(),
        };
        match result {
            Ok(mapped) => {
                entry.outcome = mapped.outcome;
                entry.notes = mapped.notes;
                if f.suppressed
                    && let Some(&last) = after.last()
                {
                    for uid in &after {
                        let _ = self.doc.set_suppressed(*uid, true);
                    }
                    let _ = last;
                }
                if entry.outcome == Outcome::Partial
                    && let Some(&last) = after.last()
                {
                    self.notes.push((last, entry.notes.join("; ")));
                }
                self.made.insert(
                    f.id,
                    Made {
                        features: after.clone(),
                    },
                );
                entry.features = after.iter().map(ToString::to_string).collect();
            }
            Err(not) => {
                // Whatever a failed mapping added is taken back.
                let added = self.doc.features().count() - before;
                for _ in 0..added {
                    self.doc.undo();
                }
                let (outcome, note) = match not {
                    NotMapped::Unmapped(why) => {
                        self.count_unmapped(&f.kind_title());
                        (Outcome::Unmapped, why)
                    }
                    NotMapped::Failed(why) => (Outcome::Failed, why),
                };
                entry.outcome = outcome;
                entry.notes.push(note.clone());
                let text = match outcome {
                    Outcome::Unmapped => format!(
                        "{} ({}) has no MitCad counterpart: {note}",
                        f.name,
                        f.kind_title()
                    ),
                    _ => format!("{} ({}) did not come in: {note}", f.name, f.kind_title()),
                };
                match self.doc.add_import_placeholder(
                    Some(&f.name),
                    &f.kind_title(),
                    &text,
                    Some(component),
                ) {
                    Ok(added) => {
                        entry.features.push(added.uid.to_string());
                        self.made.insert(
                            f.id,
                            Made {
                                features: vec![added.uid],
                            },
                        );
                    }
                    Err(e) => entry.notes.push(format!("placeholder: {e}")),
                }
            }
        }
        self.report.features.push(entry);
    }

    fn count_unmapped(&mut self, kind: &str) {
        match self.report.unmapped.iter_mut().find(|(k, _)| k == kind) {
            Some((_, n)) => *n += 1,
            None => self.report.unmapped.push((kind.to_owned(), 1)),
        }
    }

    /// The MitCad component of the file's component id.
    pub fn component_of(&self, id: u64) -> ComponentUid {
        self.components
            .get(&id)
            .copied()
            .unwrap_or(ComponentUid::ROOT)
    }

    /// Adds a definition (JSON) named `name` in `component`; Err (taken
    /// back) when it does not evaluate.
    pub fn add(
        &mut self,
        def: &Value,
        name: Option<&str>,
        component: ComponentUid,
    ) -> Result<FeatureUid, String> {
        let typed: FeatureDef<ValueInput> =
            serde_json::from_value(def.clone()).map_err(|e| format!("definition: {e}"))?;
        let name = name.map(str::trim).filter(|n| !n.is_empty());
        let before = self.body_versions();
        let added = match self.doc.add_feature_to(&typed, name, Some(component)) {
            Ok(added) => added,
            Err(e) if name.is_some() && e.to_string().contains("named") => self
                .doc
                .add_feature_to(&typed, None, Some(component))
                .map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        };
        if let Some(error) = self.doc.status(added.uid).and_then(|s| s.error()) {
            let error = error.to_owned();
            self.doc.undo();
            return Err(error);
        }
        self.params.adopt(self.doc, &added);
        self.track_bodies(&before, added.uid);
        Ok(added.uid)
    }

    /// Adds a hidden helper (a construction plane a sketch or a feature is
    /// placed on).
    pub fn add_helper(
        &mut self,
        def: &Value,
        component: ComponentUid,
    ) -> Result<FeatureUid, String> {
        let uid = self.add(def, None, component)?;
        let _ = self.doc.set_feature_visible(uid, false);
        Ok(uid)
    }

    /// The file's names for the bodies a MitCad feature made, and the
    /// names of bodies it removed taken out of the map.
    fn track_bodies(&mut self, before: &[(BodyUid, Option<u128>)], feature: FeatureUid) {
        let now: Vec<BodyUid> = self.doc.bodies().iter().map(|b| b.uid).collect();
        self.bodies.retain(|_, uid| now.contains(uid));
        let new: Vec<BodyUid> = now
            .iter()
            .filter(|b| !before.iter().any(|(u, _)| u == *b))
            .copied()
            .collect();
        let Some(f) = self.current_file_feature(feature) else {
            return;
        };
        // Pieces a cut or a split leaves of a body: the first keeps its
        // name, the others are `<name> (1)`, `<name> (2)`, … (SolidCraft's
        // `eval.rs`, Split).
        let splits = matches!(
            f.def.get("operation").and_then(Value::as_str),
            Some("cut" | "intersect")
        ) || matches!(f.kind.as_str(), "split" | "hole" | "remove");
        let changed: Vec<BodyUid> = before
            .iter()
            .filter(|(u, v)| now.contains(u) && self.doc.body_version(*u) != *v)
            .map(|(u, _)| *u)
            .collect();
        let parent = if splits {
            changed.iter().find_map(|u| {
                self.bodies
                    .iter()
                    .find(|(_, b)| *b == u)
                    .map(|(n, b)| (n.clone(), *b))
            })
        } else {
            None
        };
        let mut pieces = Vec::new();
        for (k, uid) in new.into_iter().enumerate() {
            let name = match &parent {
                Some((base, _)) => {
                    let mut n = (1..)
                        .map(|i| format!("{base} ({i})"))
                        .find(|n| !self.bodies.contains_key(n))
                        .unwrap_or_default();
                    while self.bodies.contains_key(&n) {
                        n.push('\'');
                    }
                    n
                }
                None => self.new_body_name(&f, k),
            };
            self.bodies.insert(name.clone(), uid);
            pieces.push((name, uid));
        }
        if let Some((base, uid)) = parent
            && !pieces.is_empty()
        {
            pieces.insert(0, (base, uid));
            self.order_pieces(&f, &mut pieces);
        }
        // Through names of their own, so that no two meet on the way.
        for (k, (name, uid)) in pieces.iter().enumerate() {
            if self.doc.body_name(*uid) != *name {
                let _ = self.doc.rename_body(*uid, &format!("{name} (piece {k})"));
            }
        }
        for (name, uid) in &pieces {
            if self.doc.body_name(*uid) != *name {
                let _ = self.doc.rename_body(*uid, name);
            }
        }
    }

    /// The pieces of a split body named so that the picks later features
    /// make on them by name lie on them: SolidCraft orders the pieces by
    /// its kernel (or their faces' names), which the import cannot repeat,
    /// but the file's later references tell which piece is which.
    fn order_pieces(&mut self, f: &Feature, pieces: &mut [(String, BodyUid)]) {
        let names: Vec<String> = pieces.iter().map(|(n, _)| n.clone()).collect();
        let picks = self.later_picks(f, &names);
        if picks.is_empty() || pieces.len() > 6 {
            return;
        }
        let kernel = self.doc.kernel();
        let boxes: Vec<Option<mitcad_model::BoundingBox>> = pieces
            .iter()
            .map(|(_, u)| {
                self.doc
                    .body_shape(*u)
                    .and_then(|s| kernel.bounding_box(s).ok().flatten())
            })
            .collect();
        let inside = |b: &Option<mitcad_model::BoundingBox>, p: [f64; 3]| {
            b.is_some_and(|b| {
                (0..3).all(|i| p[i] >= b.min[i] - refs::REACH && p[i] <= b.max[i] + refs::REACH)
            })
        };
        let n = pieces.len();
        let mut order: Vec<usize> = (0..n).collect();
        let mut best = (0usize, order.clone());
        loop {
            let score = picks
                .iter()
                .filter(|(name, p)| {
                    names
                        .iter()
                        .position(|x| x == name)
                        .is_some_and(|k| inside(&boxes[order[k]], *p))
                })
                .count();
            if score > best.0 {
                best = (score, order.clone());
            }
            if !next_permutation(&mut order) {
                break;
            }
        }
        let uids: Vec<BodyUid> = pieces.iter().map(|(_, u)| *u).collect();
        for (k, (name, uid)) in pieces.iter_mut().enumerate() {
            *uid = uids[best.1[k]];
            self.bodies.insert(name.clone(), *uid);
        }
    }

    /// The points later features of the file pick on bodies they name.
    fn later_picks(&self, f: &Feature, names: &[String]) -> Vec<(String, [f64; 3])> {
        let mut out = Vec::new();
        let Some(index) = self.design.features.iter().position(|x| x.id == f.id) else {
            return out;
        };
        for later in &self.design.features[index + 1..] {
            for key in ["body", "target"] {
                let Some(name) = later.def.get(key).and_then(Value::as_str) else {
                    continue;
                };
                if !names.iter().any(|n| n == name) {
                    continue;
                }
                for points in ["edges", "faces", "position", "face", "point", "bends"] {
                    collect_points(later.def.get(points), &mut |p| {
                        out.push((name.to_owned(), p))
                    });
                }
            }
        }
        out
    }

    /// The bodies and their versions now.
    pub(crate) fn body_versions(&self) -> Vec<(BodyUid, Option<u128>)> {
        self.doc
            .bodies()
            .iter()
            .map(|b| (b.uid, self.doc.body_version(b.uid)))
            .collect()
    }

    /// SolidCraft's name of a feature's k-th new body (`eval.rs`, `new_name`).
    fn new_body_name(&mut self, f: &Feature, k: usize) -> String {
        // A bounding solid's box: its stored name or `Bounding`, made
        // unique, the `Body<n>` counter untouched (`eval.rs`, BoundingSolid).
        if f.kind == "bounding_solid" {
            return self.unique_body_name(f.body_names.first().map_or("Bounding", String::as_str));
        }
        // A mesh import's bodies: the stored name, else the mesh's own, made
        // unique, the `Body<n>` counter untouched (`eval.rs`, MeshImport's
        // `copy_body_name`).
        if f.kind == "mesh_import" {
            let mesh = f
                .def
                .pointer(&format!("/meshes/{k}/name"))
                .and_then(Value::as_str)
                .unwrap_or("Body");
            let base = f
                .body_names
                .get(k)
                .map(|n| n.trim())
                .filter(|n| !n.is_empty())
                .unwrap_or(mesh);
            return self.unique_body_name(base);
        }
        if let Some(n) = f.body_names.get(k)
            && !n.is_empty()
            && !self.bodies.contains_key(n)
        {
            return n.clone();
        }
        if !f.shell_piece_order
            && k >= f.body_names.len()
            && let Some(base) = f
                .body_names
                .first()
                .map(|n| n.trim())
                .filter(|n| !n.is_empty())
        {
            let n = format!("{base} ({})", k + 1);
            return if self.bodies.contains_key(&n) {
                self.unique_body_name(base)
            } else {
                n
            };
        }
        loop {
            self.body_counter += 1;
            let n = format!("Body{}", self.body_counter);
            if !self.bodies.contains_key(&n) {
                return n;
            }
        }
    }

    fn unique_body_name(&self, base: &str) -> String {
        let base = if base.trim().is_empty() {
            "Body"
        } else {
            base.trim()
        };
        if !self.bodies.contains_key(base) {
            return base.to_owned();
        }
        (2..)
            .map(|i| format!("{base} ({i})"))
            .find(|n| !self.bodies.contains_key(n))
            .unwrap_or_else(|| base.to_owned())
    }

    /// The feature of the file being mapped (set by [`features::map`]).
    fn current_file_feature(&self, _feature: FeatureUid) -> Option<Feature> {
        features::CURRENT.with(|c| c.borrow().clone())
    }

    /// A body of the file by name.
    pub fn body(&self, name: &str) -> Result<BodyUid, String> {
        self.bodies
            .get(name)
            .copied()
            .ok_or_else(|| format!("body `{name}` does not exist here"))
    }

    /// The bodies of a component now.
    pub fn component_bodies(&self, component: ComponentUid) -> Vec<BodyUid> {
        self.doc
            .bodies()
            .iter()
            .filter(|b| self.doc.body_component(b.uid) == Some(component))
            .map(|b| b.uid)
            .collect()
    }

    /// Components (in tree order) and their occurrences.
    fn make_components(&mut self) -> Result<(), String> {
        let mut left: Vec<_> = self.design.components.clone();
        self.report.components = left.len();
        while !left.is_empty() {
            let ready: Vec<_> = left
                .iter()
                .filter(|c| c.parent == 0 || self.components.contains_key(&c.parent))
                .cloned()
                .collect();
            if ready.is_empty() {
                self.report
                    .notes
                    .push("components whose parents form a cycle are left out".to_owned());
                break;
            }
            left.retain(|c| !ready.contains(c));
            for c in ready {
                let parent = self.component_of(c.parent);
                let occurrences: Vec<_> = self
                    .design
                    .occurrences
                    .iter()
                    .filter(|o| o.component == c.id)
                    .cloned()
                    .collect();
                let first = occurrences.first();
                let transform = first.map_or(Transform::IDENTITY, |o| transform(&o.transform));
                let in_parent = first.map_or(parent, |o| self.component_of(o.parent));
                let (made, occurrence) = self
                    .doc
                    .add_component(Some(&c.name), in_parent, transform)
                    .map_err(|e| format!("component {}: {e}", c.name))?;
                self.components.insert(c.id, made);
                if let Some(o) = first {
                    self.occurrences.insert(o.id, occurrence);
                }
                for o in occurrences.iter().skip(1) {
                    let parent = self.component_of(o.parent);
                    match self.doc.add_occurrence(made, parent, transform_of(o)) {
                        Ok(uid) => {
                            self.occurrences.insert(o.id, uid);
                        }
                        Err(e) => self
                            .report
                            .notes
                            .push(format!("occurrence {}: {e}", o.name)),
                    }
                }
            }
        }
        for o in &self.design.occurrences {
            if o.grounded
                && let Some(uid) = self.occurrences.get(&o.id)
                && let Err(e) = self.doc.set_occurrence_grounded(*uid, true)
            {
                self.report.notes.push(format!("ground {}: {e}", o.name));
            }
        }
        self.report.occurrences = self.occurrences.len();
        Ok(())
    }

    /// Hidden bodies and sketches as the file saved them; sketches a
    /// feature uses are hidden unless the file shows them.
    fn visibility(&mut self) {
        let v = self.design.visibility.clone();
        for name in &v.hidden_bodies {
            if let Some(uid) = self.bodies.get(name) {
                let _ = self.doc.set_body_visible(*uid, false);
            }
        }
        let used: Vec<u64> = self
            .design
            .features
            .iter()
            .filter_map(|f| f.def.get("sketch").and_then(Value::as_u64))
            .collect();
        for (id, made) in &self.sketches {
            let hidden = v.hidden_sketches.contains(id)
                || (used.contains(id) && !v.shown_sketches.contains(id));
            if hidden {
                let _ = self.doc.set_feature_visible(made.uid, false);
            }
        }
    }

    /// The bodies after the import: volume, area and world box per placed
    /// body.
    fn measure(&mut self) {
        let names: HashMap<BodyUid, String> =
            self.bodies.iter().map(|(n, u)| (*u, n.clone())).collect();
        let kernel = self.doc.kernel();
        let mut out = Vec::new();
        for instance in self.doc.instances() {
            let Some(shape) = self.doc.body_shape(instance.body) else {
                continue;
            };
            let placed = if instance.transform == Transform::IDENTITY {
                None
            } else {
                kernel
                    .transform_shape(shape, &instance.transform, None)
                    .ok()
            };
            let shape = placed.as_ref().unwrap_or(shape);
            let mass = kernel.mass_properties(shape).ok();
            let bbox = kernel.bounding_box(shape).ok().flatten();
            out.push(BodyReport {
                name: names
                    .get(&instance.body)
                    .cloned()
                    .unwrap_or_else(|| instance.name.clone()),
                body: instance.body.to_string(),
                path: instance.path_name.clone(),
                volume_mm3: mass.map_or(0.0, |m| m.volume),
                area_mm2: mass.map_or(0.0, |m| m.area),
                bbox_min: bbox.map_or([0.0; 3], |b| b.min),
                bbox_max: bbox.map_or([0.0; 3], |b| b.max),
            });
        }
        self.report.bodies = out;
    }
}

/// A SolidCraft transform as MitCad's (`m[j][i]` is row i, column j).
pub(crate) fn transform(m: &mitcad_solidcraft::document::Mat) -> Transform {
    Transform {
        linear: std::array::from_fn(|i| std::array::from_fn(|j| m[j][i])),
        translation: [m[3][0], m[3][1], m[3][2]],
    }
}

fn transform_of(o: &mitcad_solidcraft::Occurrence) -> Transform {
    transform(&o.transform)
}

/// The report against expected values: SolidCraft's evaluation of the
/// design as `solidcraft-cli eval` prints it (`body_count`, `total.volume_mm3`,
/// `bodies[].volume_mm3`/`bbox`, `timeline`, `params`, `sketches`), or a
/// hand-written subset of it; volumes within 0.1 % (`volume_tolerance`,
/// relative), boxes within 0.01 mm (`bbox_tolerance`).
pub fn check(report: &SolidcraftReport, expected: &Value) -> CheckReport {
    // SolidCraft's evaluation measures every placed body (its builds before
    // components measured the root's only: `"scope": "root"`).
    let all = expected.get("scope").and_then(Value::as_str) != Some("root");
    let bodies: Vec<&BodyReport> = report
        .bodies
        .iter()
        .filter(|b| all || b.path.is_empty())
        .collect();
    let mut failures = Vec::new();
    let mut checks = 0;
    let vtol = expected
        .get("volume_tolerance")
        .and_then(Value::as_f64)
        .unwrap_or(1e-3);
    let btol = expected
        .get("bbox_tolerance")
        .and_then(Value::as_f64)
        .unwrap_or(0.01);
    if let Some(n) = expected.get("body_count").and_then(Value::as_u64) {
        checks += 1;
        if bodies.len() as u64 != n {
            failures.push(format!("{} bodies, expected {n}", bodies.len()));
        }
    }
    let total: f64 = bodies.iter().map(|b| b.volume_mm3).sum();
    let volume = expected
        .pointer("/total/volume_mm3")
        .or_else(|| expected.get("volume_mm3"))
        .and_then(Value::as_f64);
    if let Some(v) = volume {
        checks += 1;
        if (total - v).abs() > vtol * v.abs().max(1e-9) {
            failures.push(format!("volume {total:.4} mm3, expected {v:.4}"));
        }
    }
    let close = |a: &[f64; 3], b: &Value| -> bool {
        (0..3).all(|i| {
            b.get(i)
                .and_then(Value::as_f64)
                .is_some_and(|x| (a[i] - x).abs() <= btol)
        })
    };
    if let Some(bbox) = expected.get("bbox") {
        checks += 1;
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for b in &bodies {
            for i in 0..3 {
                min[i] = min[i].min(b.bbox_min[i]);
                max[i] = max[i].max(b.bbox_max[i]);
            }
        }
        if !close(&min, &bbox["min"]) || !close(&max, &bbox["max"]) {
            failures.push(format!(
                "box {min:?} .. {max:?}, expected {} .. {}",
                bbox["min"], bbox["max"]
            ));
        }
    }
    for b in expected
        .get("bodies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = b.get("name").and_then(Value::as_str) else {
            continue;
        };
        checks += 1;
        let Some(ours) = bodies.iter().find(|x| x.name == name) else {
            failures.push(format!("no body {name}"));
            continue;
        };
        if let Some(v) = b.get("volume_mm3").and_then(Value::as_f64)
            && (ours.volume_mm3 - v).abs() > vtol * v.abs().max(1e-9)
        {
            failures.push(format!(
                "{name}: volume {:.4} mm3, expected {v:.4}",
                ours.volume_mm3
            ));
        }
        // A surface body's area (mitcad#455), within the volume's tolerance.
        if let Some(a) = b.get("area_mm2").and_then(Value::as_f64)
            && (ours.area_mm2 - a).abs() > vtol * a.abs().max(1e-9)
        {
            failures.push(format!(
                "{name}: area {:.4} mm2, expected {a:.4}",
                ours.area_mm2
            ));
        }
        if let Some(bbox) = b.get("bbox")
            && (!close(&ours.bbox_min, &bbox["min"]) || !close(&ours.bbox_max, &bbox["max"]))
        {
            failures.push(format!(
                "{name}: box {:?} .. {:?}, expected {} .. {}",
                ours.bbox_min, ours.bbox_max, bbox["min"], bbox["max"]
            ));
        }
    }
    if let Some(timeline) = expected.get("timeline").and_then(Value::as_array) {
        checks += 1;
        let names: Vec<&str> = timeline
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        let ours: Vec<&str> = report.features.iter().map(|f| f.name.as_str()).collect();
        if names != ours {
            failures.push(format!("timeline {ours:?}, expected {names:?}"));
        }
    }
    for p in expected
        .get("params")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = p.get("name").and_then(Value::as_str) else {
            continue;
        };
        checks += 1;
        if !report
            .parameters
            .iter()
            .any(|q| q.name == name && q.error.is_none())
        {
            failures.push(format!("parameter {name} did not come in"));
        }
    }
    for s in expected
        .get("sketches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = s.get("name").and_then(Value::as_str) else {
            continue;
        };
        checks += 1;
        let Some(ours) = report.sketches.iter().find(|x| x.name == name) else {
            failures.push(format!("no sketch {name}"));
            continue;
        };
        for (key, have) in [
            ("curves", ours.curves),
            ("constraints", ours.constraints_mapped),
            ("dimensions", ours.dimensions_mapped),
        ] {
            if let Some(n) = s.get(key).and_then(Value::as_u64)
                && n as usize != have
            {
                failures.push(format!("sketch {name}: {have} {key}, expected {n}"));
            }
        }
    }
    // Motion links that came in parametric (mitcad#466).
    if let Some(n) = expected.get("links").and_then(Value::as_u64) {
        checks += 1;
        let ours = report
            .links
            .iter()
            .filter(|l| l.outcome == Outcome::Parametric)
            .count() as u64;
        if ours != n {
            failures.push(format!("{ours} motion links came in, expected {n}"));
        }
    }
    if let Some(min) = expected.get("parametric").and_then(Value::as_u64) {
        checks += 1;
        let n = report.count(Outcome::Parametric) as u64;
        if n < min {
            failures.push(format!("{n} parametric features, expected at least {min}"));
        }
    }
    CheckReport {
        passed: failures.is_empty(),
        checks,
        failures,
    }
}

/// The next permutation in lexical order; false after the last.
fn next_permutation(v: &mut [usize]) -> bool {
    let Some(i) = (1..v.len()).rev().find(|&i| v[i - 1] < v[i]) else {
        return false;
    };
    let j = (i..v.len()).rev().find(|&j| v[j] > v[i - 1]).unwrap_or(i);
    v.swap(i - 1, j);
    v[i..].reverse();
    true
}

/// Every point (`[x, y, z]`) in a JSON value: a point, a list of them, or
/// edges with their own values (`{"point": [x, y, z]}`).
fn collect_points(v: Option<&Value>, out: &mut dyn FnMut([f64; 3])) {
    match v {
        Some(Value::Array(a)) if a.len() == 3 && a.iter().all(Value::is_number) => {
            out([
                a[0].as_f64().unwrap_or(0.0),
                a[1].as_f64().unwrap_or(0.0),
                a[2].as_f64().unwrap_or(0.0),
            ]);
        }
        Some(Value::Array(a)) => a.iter().for_each(|x| collect_points(Some(x), out)),
        Some(Value::Object(o)) => collect_points(o.get("point"), out),
        _ => {}
    }
}
