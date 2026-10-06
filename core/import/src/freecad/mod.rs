// SPDX-License-Identifier: MIT
//! FreeCAD document import (`.FCStd`, read by `mitcad-freecad`): the
//! document's structure with the bodies FreeCAD stored (stage 1), then the
//! timeline: the sketches (stage 2, [`sketches`]) and the history of the
//! PartDesign Bodies and the Part workbench's results, each feature checked
//! against its stored shape (stage 3, [`history`]); not with `bodies_only`.
//!
//! - App::Part and Assembly → components, placed by their placements;
//!   nested ones are subcomponents.
//! - PartDesign::Body and the Part workbench's leaf objects (shape-bearing
//!   objects nothing consumes) → bodies: the history rebuilds those it
//!   replays (`history.rs`), the others come from
//!   the shapes stored in the file (each object's result as FreeCAD last
//!   computed it, in its container's coordinates). A component's stored
//!   bodies are one base feature, named after their labels; their faces
//!   are `<feature>:import(j)` in the shapes' face order, j counted over
//!   the feature's bodies. A Body with a placement of its own gets a
//!   component of its own with the Body's origin as the component's (its
//!   features' origin references stay the component's).
//! - App::Link → occurrences of the linked object's component (the linked
//!   object gets one when it is not a part), an array → one per element;
//!   links to other files read those files; a scaled link → a copy of the
//!   body. Assembly parts are links; a grounded joint grounds its part's
//!   occurrence; other joints are left out.
//! - Visibility (hidden bodies and occurrences) and colours (of the stored
//!   bodies).
//! - The origin and objects without geometry are left out with the
//!   reason; features inside a Body and consumed operands are part of
//!   their result (and of the history).
//! - Sketches come on the timeline, editable, with their constraints and
//!   dimensions ([`sketch`] translates them).
//! - Spreadsheets, VarSets, named constraints and the expressions bound to
//!   properties become parameters and expressions (stage 4, [`params`]).
//!
//! Placements are taken from the file, not computed from attachments. A
//! link shows its object with the link's placement instead of the object's
//! own (`LinkTransform` false), or on top of it (true); an array element's
//! placement comes between.

mod elements;
mod features;
mod history;
mod params;
mod part;
mod primitives;
mod reference;
pub mod report;
mod sketch;
mod sketches;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use mitcad_freecad::{Color, FcstdFile, Object, Placement, Value, version};
use mitcad_model::expr::LengthUnit;
use mitcad_model::{
    BaseInput, BodyKind, BodyUid, ComponentUid, Document, FeatureUid, ImportBody, Kernel,
    MassProperties, OccurrenceUid, Transform,
};

pub use report::{
    BodyReport, DimensionSource, ExpressionOutcome, ExpressionReport, FcstdReport, FeatureCheck,
    FeatureReport, ObjectOutcome, ObjectReport, ParameterReport, PlacedReport, ReferenceReport,
    ShapeCheck, SketchReport,
};

/// Import settings.
#[derive(Debug, Clone, Default)]
pub struct FcstdOptions {
    /// The bodies only, without the history (the sketches).
    pub bodies_only: bool,
    /// Merge the import into one undo step with this label.
    pub undo_label: Option<String>,
    /// A dump of the document by FreeCAD (`tools/freecad-export/dump.py`)
    /// to compare the import with ([`FcstdReport::reference`]).
    pub reference: Option<serde_json::Value>,
    /// Parameters to change after the import (name, expression), before
    /// the report measures the result: to compare with FreeCAD's own
    /// change of them (a dump of the changed document as `reference`).
    pub set_parameters: Vec<(String, String)>,
}

/// Reads a linked document by its path.
pub type Loader<'a> = dyn FnMut(&Path) -> Result<FcstdFile, String> + 'a;

/// Imports the `.FCStd` file at `path` into the document (normally a new
/// one); linked documents are read from the disk.
pub fn import_fcstd<K: Kernel>(
    doc: &mut Document<K>,
    path: &Path,
    options: &FcstdOptions,
) -> Result<FcstdReport, String> {
    let file = FcstdFile::open(path).map_err(|e| e.to_string())?;
    let mut loader = |p: &Path| FcstdFile::open(p).map_err(|e| e.to_string());
    import_fcstd_file(doc, file, path, &mut loader, options)
}

/// Imports an opened file; `path` is where it is (for its name and for
/// links to other files, which `loader` reads).
pub fn import_fcstd_file<K: Kernel>(
    doc: &mut Document<K>,
    file: FcstdFile,
    path: &Path,
    loader: &mut Loader<'_>,
    options: &FcstdOptions,
) -> Result<FcstdReport, String> {
    let depth = doc.undo_depth();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut importer = Importer {
        doc,
        loader,
        sources: vec![Source::new(file, path.to_path_buf())],
        report: FcstdReport {
            file: name,
            bodies_only: options.bodies_only,
            ..FcstdReport::default()
        },
        components: HashMap::new(),
        link_targets: HashSet::new(),
        planned: Vec::new(),
        placed: Vec::new(),
        roles: HashMap::new(),
        occurrences_of: HashMap::new(),
        deferred: Vec::new(),
        deferring: true,
        component_names: HashMap::new(),
        component_paths: HashMap::new(),
        stored: HashMap::new(),
        bodies_only: options.bodies_only,
        replay: history::Replay::default(),
        replayed_bodies: HashSet::new(),
        params: params::Params::default(),
        set_parameters: options.set_parameters.clone(),
        measured_sketches: Vec::new(),
    };
    let result = importer.run();
    let Importer {
        doc, mut report, ..
    } = importer;
    result?;
    if let Some(label) = &options.undo_label {
        doc.merge_undo(depth, label);
    }
    if let Some(dump) = &options.reference {
        report.reference = Some(reference::check(&report, dump));
    }
    Ok(report)
}

/// An object of one of the documents read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Key {
    doc: usize,
    name: String,
}

/// What an object is to the import.
#[derive(Debug, Clone, PartialEq)]
enum Role {
    Component,
    Body,
    Link,
    /// A folder: its objects go where it is.
    Group,
    Included(String),
    Skipped(String),
}

/// A document read: the file, and its structure.
struct Source {
    file: FcstdFile,
    path: PathBuf,
    /// Each object's container: the object whose group (or origin, or
    /// array elements) holds it.
    container: HashMap<String, String>,
    /// The first shape-bearing object that consumes an object.
    consumer: HashMap<String, String>,
}

/// Link properties that do not consume what they refer to: containers'
/// lists, attachments, references for directions and limits, a link's
/// target, assembly joints' references.
const NOT_CONSUMING: &[&str] = &[
    "Group",
    "Origin",
    "OriginFeatures",
    "Tip",
    "LinkedObject",
    "ElementList",
    "ColoredElements",
    "LinkCopyOnChangeSource",
    "LinkCopyOnChangeGroup",
    "AttachmentSupport",
    "Support",
    "ExternalGeometry",
    "ReferenceAxis",
    "Direction",
    "DirLink",
    "AxisLink",
    "UpToFace",
    "UpToShape",
    "UpToFace2",
    "UpToShape2",
    "MirrorPlane",
    "NeutralPlane",
    "PullDirection",
    "ObjectToGround",
    "Reference1",
    "Reference2",
];

impl Source {
    fn new(file: FcstdFile, path: PathBuf) -> Self {
        let doc = &file.document;
        let mut container = HashMap::new();
        let mut consumer = HashMap::new();
        for o in &doc.objects {
            for property in ["Group", "Origin", "OriginFeatures", "ElementList"] {
                for link in o.links(property).iter().filter(|l| l.file.is_none()) {
                    container
                        .entry(link.object.clone())
                        .or_insert_with(|| o.name.clone());
                }
            }
            if o.shape().is_none() || is_sketch(&o.type_name) || is_construction(&o.type_name) {
                continue;
            }
            for (property, link) in o.references() {
                if link.file.is_none()
                    && link.object != o.name
                    && !NOT_CONSUMING.contains(&property)
                {
                    consumer
                        .entry(link.object.clone())
                        .or_insert_with(|| o.name.clone());
                }
            }
        }
        Self {
            file,
            path,
            container,
            consumer,
        }
    }

    fn object(&self, name: &str) -> Option<&Object> {
        self.file.document.object(name)
    }

    fn label(&self, name: &str) -> String {
        self.object(name)
            .map_or_else(|| name.to_owned(), |o| o.label().to_owned())
    }

    /// The Body an object is inside, through its containers.
    fn body_of(&self, name: &str) -> Option<&str> {
        let mut at = name;
        for _ in 0..16 {
            let parent = self.container.get(at)?;
            if self
                .object(parent)
                .is_some_and(|o| o.type_name == "PartDesign::Body")
            {
                return Some(parent);
            }
            at = parent;
        }
        None
    }

    fn role(&self, o: &Object) -> Role {
        let t = o.type_name.as_str();
        if is_component(t) {
            return Role::Component;
        }
        if t == "App::LinkElement" {
            let array = self.container.get(&o.name).map_or("?", String::as_str);
            return Role::Included(format!(
                "an element of the link array {}",
                self.label(array)
            ));
        }
        if t == "App::Link" || (o.has_extension("App::LinkExtension") && t != "App::LinkGroup") {
            return Role::Link;
        }
        if t == "App::LinkGroup" {
            return Role::Skipped("link groups are not imported yet".to_owned());
        }
        if let Some(body) = self.body_of(&o.name) {
            return Role::Included(format!("in the body {}", self.label(body)));
        }
        if is_construction(t) {
            return Role::Skipped("construction geometry (origin, datum)".to_owned());
        }
        if is_sketch(t) {
            return Role::Skipped("a sketch: sketches come with the history".to_owned());
        }
        if o.property("ObjectToGround").is_some() || o.property("JointType").is_some() {
            return Role::Skipped(
                "an assembly joint: Mitcad has none; the parts keep their placements".to_owned(),
            );
        }
        if is_group(o) {
            return Role::Group;
        }
        if o.shape().is_some() {
            if let Some(user) = self.consumer.get(&o.name) {
                return Role::Included(format!("used by {}", self.label(user)));
            }
            return Role::Body;
        }
        Role::Skipped("no geometry".to_owned())
    }

    /// The objects at the top of the document, in its order.
    fn roots(&self) -> Vec<String> {
        self.file
            .document
            .objects
            .iter()
            .filter(|o| !self.container.contains_key(&o.name))
            .map(|o| o.name.clone())
            .collect()
    }

    /// The objects a container holds, in its order.
    fn children(&self, name: &str) -> Vec<String> {
        let Some(o) = self.object(name) else {
            return Vec::new();
        };
        o.links("Group")
            .iter()
            .filter(|l| {
                l.file.is_none() && self.container.get(&l.object).map(String::as_str) == Some(name)
            })
            .map(|l| l.object.clone())
            .collect()
    }
}

fn is_component(t: &str) -> bool {
    matches!(t, "App::Part" | "Assembly::AssemblyObject")
}

fn is_sketch(t: &str) -> bool {
    t.starts_with("Sketcher::SketchObject")
}

fn is_construction(t: &str) -> bool {
    matches!(
        t,
        "App::Origin"
            | "App::Line"
            | "App::Plane"
            | "App::Point"
            | "App::LocalCoordinateSystem"
            | "App::DatumElement"
            | "PartDesign::Plane"
            | "PartDesign::Line"
            | "PartDesign::Point"
            | "PartDesign::CoordinateSystem"
            | "Part::DatumPlane"
            | "Part::DatumLine"
            | "Part::DatumPoint"
            | "Part::LocalCoordinateSystem"
    )
}

fn is_group(o: &Object) -> bool {
    o.type_name.starts_with("App::DocumentObjectGroup")
        || o.type_name.starts_with("Assembly::")
            && o.shape().is_none()
            && !is_component(&o.type_name)
}

/// A FreeCAD placement as a Mitcad transform.
fn transform(p: &Placement) -> Transform {
    Transform {
        linear: p.matrix(),
        translation: p.position,
    }
}

/// A body to make: an object's stored shape, moved by `transform` when
/// given (into the object's own frame, or a scaled copy).
struct Planned {
    key: Key,
    /// The object of the imported document it is reported under (the
    /// object itself, or a link for a scaled copy).
    owner: Option<String>,
    component: ComponentUid,
    transform: Option<Transform>,
    name: String,
    color: Option<[f64; 3]>,
    visible: bool,
    /// Made (after the base features are added).
    body: Option<BodyUid>,
}

/// What a placed record measures.
#[derive(Clone)]
enum Measured {
    /// One planned body (an index into `planned`).
    Body(usize),
    /// Everything under an occurrence.
    Under,
}

/// Where an object of the imported document shows its geometry.
struct Placed {
    object: String,
    element: Option<usize>,
    path: Vec<OccurrenceUid>,
    what: Measured,
    visible: bool,
}

/// A link waiting for the structure to be made (links are placed after
/// it, so that the parts and bodies they show are where the document puts
/// them first).
struct Deferred {
    key: Key,
    component: ComponentUid,
    path: Vec<OccurrenceUid>,
    record: bool,
}

struct Importer<'a, 'l, K: Kernel> {
    doc: &'a mut Document<K>,
    loader: &'a mut Loader<'l>,
    sources: Vec<Source>,
    report: FcstdReport,
    /// Components made for parts and for objects shown by links (or with
    /// placements of their own).
    components: HashMap<Key, ComponentUid>,
    /// Objects of the imported document that links show.
    link_targets: HashSet<Key>,
    planned: Vec<Planned>,
    placed: Vec<Placed>,
    /// The role of each object of the imported document, as reported.
    roles: HashMap<String, (ObjectOutcome, Option<String>)>,
    /// The occurrences made for parts, links and placed bodies.
    occurrences_of: HashMap<Key, Vec<OccurrenceUid>>,
    deferred: Vec<Deferred>,
    deferring: bool,
    component_names: HashMap<ComponentUid, String>,
    /// The occurrence path where the document itself places each component
    /// first.
    component_paths: HashMap<ComponentUid, Vec<OccurrenceUid>>,
    /// Stored shapes read for sketches and the history, by object.
    stored: HashMap<String, Option<K::Shape>>,
    bodies_only: bool,
    /// The history's replay (stage 3).
    replay: history::Replay<K::Shape>,
    /// The bodies the replay made of planned bodies.
    replayed_bodies: HashSet<BodyUid>,
    /// The parameters made of FreeCAD's quantities (stage 4).
    params: params::Params,
    set_parameters: Vec<(String, String)>,
    /// The sketches made (report index, feature, placement), measured
    /// again after `set_parameters`.
    measured_sketches: Vec<(usize, FeatureUid, sketches::Placed)>,
}

/// How deep links of links and nested parts are followed.
const MAX_DEPTH: usize = 32;

impl<K: Kernel> Importer<'_, '_, K> {
    fn run(&mut self) -> Result<(), String> {
        self.describe();
        self.set_units();
        self.find_link_targets();
        // The structure, then the links into it.
        self.walk(0, None, ComponentUid::ROOT, &[], true, 0);
        self.deferring = false;
        let deferred = std::mem::take(&mut self.deferred);
        for d in deferred {
            self.place_link(&d.key, d.component, &d.path, d.record, 0);
        }
        self.ground();
        if !self.bodies_only {
            self.plan_replay();
        }
        self.make_bodies()?;
        if !self.bodies_only {
            self.import_parameters();
            self.import_history();
            let changes = std::mem::take(&mut self.set_parameters);
            self.set_parameters(&changes);
        }
        self.finish_report();
        if !self.bodies_only {
            self.finish_parameters();
        }
        Ok(())
    }

    fn source(&self, doc: usize) -> &Source {
        &self.sources[doc]
    }

    fn warn(&mut self, message: String) {
        self.report.warnings.push(message);
    }

    /// The report's header: version, objects by type, stale shapes, what
    /// could not be read.
    fn describe(&mut self) {
        let file = &self.sources[0].file;
        let doc = &file.document;
        self.report.label = doc.label().unwrap_or_default().to_owned();
        self.report.program_version = doc.program_version.clone();
        self.report.schema_version = doc.schema_version;
        for o in &doc.objects {
            *self
                .report
                .objects_by_type
                .entry(o.type_name.clone())
                .or_default() += 1;
        }
        let warnings = file.warnings.clone();
        self.report.warnings.extend(warnings);
    }

    /// The document's unit system (1.0 and later) as its length unit.
    fn set_units(&mut self) {
        let doc = &self.sources[0].file.document;
        let Some(Value::Enumeration(e)) = doc.property("UnitSystem").map(|p| &p.value) else {
            return;
        };
        let text = usize::try_from(e.index).ok().and_then(|i| {
            version::document_enum_values(&doc.version(), "UnitSystem")?
                .get(i)
                .cloned()
        });
        let Some(text) = text else {
            return;
        };
        self.report.units = Some(text.clone());
        // The first unit in parentheses: "Standard (mm, kg, s, °)".
        let first = text
            .split_once('(')
            .map(|(_, rest)| rest.split([',', ')', '-']).next().unwrap_or("").trim())
            .unwrap_or("");
        let unit = match first {
            "mm" => LengthUnit::Millimetre,
            "cm" => LengthUnit::Centimetre,
            "m" => LengthUnit::Metre,
            "in" => LengthUnit::Inch,
            "ft" => LengthUnit::Foot,
            _ => return,
        };
        if let Err(e) = self.doc.set_units(unit) {
            self.warn(format!("units {text}: {e}"));
        }
    }

    /// The objects of the imported document that its links show: they get
    /// components of their own, placed where the document has them too.
    fn find_link_targets(&mut self) {
        let names: Vec<String> = self.sources[0]
            .file
            .document
            .objects
            .iter()
            .filter(|o| self.source(0).role(o) == Role::Link)
            .map(|o| o.name.clone())
            .collect();
        for name in names {
            let mut at = Key { doc: 0, name };
            for _ in 0..MAX_DEPTH {
                let Some(link) = self
                    .source(at.doc)
                    .object(&at.name)
                    .and_then(|o| o.link("LinkedObject"))
                else {
                    break;
                };
                if link.file.is_some() {
                    break;
                }
                let next = Key {
                    doc: at.doc,
                    name: link.object.clone(),
                };
                let is_link = self
                    .source(next.doc)
                    .object(&next.name)
                    .is_some_and(|o| self.source(next.doc).role(o) == Role::Link);
                if !is_link {
                    self.link_targets.insert(next);
                    break;
                }
                at = next;
            }
        }
    }

    /// Records what an object of the imported document became.
    fn note(&mut self, key: &Key, outcome: ObjectOutcome, note: Option<String>) {
        if key.doc == 0 {
            self.roles
                .entry(key.name.clone())
                .or_insert((outcome, note));
        }
    }

    /// Places the objects of a container (the document's top when None)
    /// into `component`, reached by `path`. `record`: the places are the
    /// imported document's own (not those of a link's copy).
    fn walk(
        &mut self,
        doc: usize,
        container: Option<&str>,
        component: ComponentUid,
        path: &[OccurrenceUid],
        record: bool,
        depth: usize,
    ) {
        if depth > MAX_DEPTH {
            self.warn("the parts are nested too deeply; the deepest are left out".to_owned());
            return;
        }
        let children = match container {
            None => self.source(doc).roots(),
            Some(c) => self.source(doc).children(c),
        };
        for name in children {
            self.place(doc, &name, component, path, record, depth);
        }
    }

    fn place(
        &mut self,
        doc: usize,
        name: &str,
        component: ComponentUid,
        path: &[OccurrenceUid],
        record: bool,
        depth: usize,
    ) {
        let key = Key {
            doc,
            name: name.to_owned(),
        };
        let Some(o) = self.source(doc).object(name) else {
            return;
        };
        let role = self.source(doc).role(o);
        let visible = self.source(doc).file.visible(o);
        let placement = o.placement().unwrap_or_default();
        let own = role == Role::Body
            && (self.link_targets.contains(&key)
                || o.type_name == "PartDesign::Body" && !placement.is_identity(1e-12));
        match role {
            Role::Component => {
                self.note(&key, ObjectOutcome::Component, None);
                self.place_component(&key, component, &placement, path, visible, record, depth);
            }
            Role::Body if own => {
                self.note(&key, ObjectOutcome::Body, None);
                self.place_component(&key, component, &placement, path, visible, record, depth);
            }
            Role::Body => {
                self.note(&key, ObjectOutcome::Body, None);
                let index = self.plan(&key, component, None, visible);
                if record {
                    self.placed.push(Placed {
                        object: key.name.clone(),
                        element: None,
                        path: path.to_vec(),
                        what: Measured::Body(index),
                        visible,
                    });
                }
            }
            Role::Link => {
                self.note(&key, ObjectOutcome::Occurrence, None);
                if self.deferring {
                    self.deferred.push(Deferred {
                        key,
                        component,
                        path: path.to_vec(),
                        record,
                    });
                } else {
                    self.place_link(&key, component, path, record, depth);
                }
            }
            Role::Group => {
                self.note(&key, ObjectOutcome::Included, Some("a folder".to_owned()));
                self.walk(doc, Some(name), component, path, record, depth + 1);
            }
            Role::Included(why) => self.note(&key, ObjectOutcome::Included, Some(why)),
            Role::Skipped(why) => self.note(&key, ObjectOutcome::Skipped, Some(why)),
        }
    }

    /// Places a part, or a body with a component of its own, in `parent`
    /// at `placement`, making its component the first time.
    #[allow(clippy::too_many_arguments)]
    fn place_component(
        &mut self,
        key: &Key,
        parent: ComponentUid,
        placement: &Placement,
        path: &[OccurrenceUid],
        visible: bool,
        record: bool,
        depth: usize,
    ) -> Option<OccurrenceUid> {
        let occurrence = self.occurrence(key, parent, placement, path, record, depth)?;
        if !visible && let Err(e) = self.doc.set_occurrence_visible(occurrence, false) {
            self.warn(format!("{}: {e}", key.name));
        }
        self.occurrences_of
            .entry(key.clone())
            .or_default()
            .push(occurrence);
        if record {
            let mut path = path.to_vec();
            path.push(occurrence);
            if let Some(&component) = self.components.get(key) {
                self.component_paths
                    .entry(component)
                    .or_insert_with(|| path.clone());
            }
            self.placed.push(Placed {
                object: key.name.clone(),
                element: None,
                path,
                what: Measured::Under,
                visible,
            });
        }
        Some(occurrence)
    }

    /// An occurrence of the object's component in `parent` (reached by
    /// `path`); the component is made and filled the first time, a part's
    /// objects recorded as the document's own when `record`.
    fn occurrence(
        &mut self,
        key: &Key,
        parent: ComponentUid,
        placement: &Placement,
        path: &[OccurrenceUid],
        record: bool,
        depth: usize,
    ) -> Option<OccurrenceUid> {
        let t = transform(placement);
        if let Some(&component) = self.components.get(key) {
            return match self.doc.add_occurrence(component, parent, t) {
                Ok(o) => {
                    self.report.occurrences += 1;
                    Some(o)
                }
                Err(e) => {
                    self.warn(format!("{}: {e}", self.source(key.doc).label(&key.name)));
                    None
                }
            };
        }
        let label = self.source(key.doc).label(&key.name);
        let (component, occurrence) = match self.doc.add_component(Some(&label), parent, t) {
            Ok(made) => made,
            Err(e) => {
                self.warn(format!("{label}: {e}"));
                return None;
            }
        };
        self.report.components += 1;
        self.report.occurrences += 1;
        self.components.insert(key.clone(), component);
        let name = self.doc.assembly().name(component).to_owned();
        self.component_names.insert(component, name);
        let o = self.source(key.doc).object(&key.name)?;
        if is_component(&o.type_name) {
            // Its objects, in its coordinates.
            let mut inside = path.to_vec();
            inside.push(occurrence);
            self.walk(
                key.doc,
                Some(&key.name),
                component,
                &inside,
                record,
                depth + 1,
            );
        } else {
            // A body in its own frame: the stored shape without the
            // object's placement; the occurrence shows or hides it.
            let own = transform(&placement_of(o).inverse());
            self.plan(key, component, Some(own), true);
        }
        Some(occurrence)
    }

    /// A body of an object's stored shape to make in `component`.
    fn plan(
        &mut self,
        key: &Key,
        component: ComponentUid,
        transform: Option<Transform>,
        visible: bool,
    ) -> usize {
        let source = self.source(key.doc);
        let o = source.object(&key.name);
        let color = o.and_then(|o| body_color(&source.file, o));
        self.planned.push(Planned {
            key: key.clone(),
            owner: (key.doc == 0).then(|| key.name.clone()),
            component,
            transform,
            name: source.label(&key.name),
            color,
            visible,
            body: None,
        });
        self.planned.len() - 1
    }

    /// Reads a linked document (once), relative to the linking one.
    fn load(&mut self, from: usize, file: &str) -> Option<usize> {
        let base = self.sources[from]
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let path = base.join(file);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if let Some(i) = self
            .sources
            .iter()
            .position(|s| s.path.canonicalize().unwrap_or_else(|_| s.path.clone()) == canonical)
        {
            return Some(i);
        }
        if self.report.missing_files.iter().any(|m| m == file) {
            return None;
        }
        match (self.loader)(&path) {
            Ok(opened) => {
                self.report.files.push(file.to_owned());
                let warnings = opened.warnings.clone();
                for w in warnings {
                    self.warn(format!("{file}: {w}"));
                }
                self.sources.push(Source::new(opened, path));
                Some(self.sources.len() - 1)
            }
            Err(e) => {
                self.report.missing_files.push(file.to_owned());
                self.warn(format!("{file}: {e}"));
                None
            }
        }
    }

    /// What a link shows: the object at the end of a chain of links, and
    /// the placement of that object's frame in the link's container.
    fn resolve(&mut self, key: &Key, depth: usize) -> Result<(Key, Placement), String> {
        if depth > MAX_DEPTH {
            return Err("links of links too deep".to_owned());
        }
        let o = self
            .source(key.doc)
            .object(&key.name)
            .ok_or_else(|| format!("no object {}", key.name))?;
        let own = placement_of(o);
        let relative = o.bool("LinkTransform").unwrap_or(false);
        let link = o
            .link("LinkedObject")
            .cloned()
            .ok_or("the link has no linked object")?;
        if !link.subs.is_empty() {
            return Err(format!(
                "links to elements of objects are not imported yet ({})",
                link.subs[0].name
            ));
        }
        let doc = match &link.file {
            Some(file) => self
                .load(key.doc, file)
                .ok_or_else(|| format!("the linked file {file} was not found"))?,
            None => key.doc,
        };
        let target = Key {
            doc,
            name: link.object.clone(),
        };
        let t = self
            .source(doc)
            .object(&target.name)
            .ok_or_else(|| format!("no object {} to link to", target.name))?;
        let target_placement = placement_of(t);
        let (end, frame) = if self.source(doc).role(t) == Role::Link {
            self.resolve(&target, depth + 1)?
        } else {
            (target.clone(), target_placement)
        };
        // Its own placement replaces the target's, or goes on top of it.
        let inner = if relative {
            frame
        } else {
            target_placement.inverse().multiply(&frame)
        };
        Ok((end, own.multiply(&inner)))
    }

    /// Places a link's occurrences (one per array element).
    fn place_link(
        &mut self,
        key: &Key,
        component: ComponentUid,
        path: &[OccurrenceUid],
        record: bool,
        depth: usize,
    ) {
        let label = self.source(key.doc).label(&key.name);
        let (target, frame) = match self.resolve(key, 0) {
            Ok(resolved) => resolved,
            Err(e) => {
                self.skip_link(key, &label, e);
                return;
            }
        };
        let Some(o) = self.source(key.doc).object(&key.name) else {
            return;
        };
        let own = placement_of(o);
        let visible = self.source(key.doc).file.visible(o);
        let elements = elements(&self.sources[key.doc], o);
        let scale = link_scale(o);
        // A part, or an object with a stored shape (a body, a feature).
        let (shows_part, shows_body) = self
            .source(target.doc)
            .object(&target.name)
            .map_or((false, false), |t| {
                (is_component(&t.type_name), t.shape().is_some())
            });
        if !shows_part && !shows_body {
            self.skip_link(
                key,
                &label,
                format!(
                    "it links {}, which has no geometry to place",
                    self.source(target.doc).label(&target.name)
                ),
            );
            return;
        }
        let mut scaled_copies = 0;
        // The frame without the link's own placement, for elements.
        let inner = own.inverse().multiply(&frame);
        let count = elements.as_ref().map_or(1, Vec::len);
        for i in 0..count {
            let (element, element_placement, element_visible, element_scale) = match &elements {
                Some(list) => (Some(i), list[i].0, list[i].1, list[i].2),
                None => (None, Placement::IDENTITY, true, [1.0; 3]),
            };
            let at = own.multiply(&element_placement).multiply(&inner);
            let factors = std::array::from_fn::<f64, 3, _>(|k| scale[k] * element_scale[k]);
            let shown = visible && element_visible;
            if factors.iter().any(|f| (f - 1.0).abs() > 1e-12) {
                if !shows_body {
                    self.warn(format!(
                        "{label}: a scaled link of a part is not imported (Mitcad's placements are rigid)"
                    ));
                    continue;
                }
                // A scaled copy of the body: its stored shape back in its
                // own frame, scaled there, then placed.
                let t = self
                    .source(target.doc)
                    .object(&target.name)
                    .map(placement_of);
                let local = transform(&t.unwrap_or_default().inverse());
                let scaled = Transform {
                    linear: [
                        [factors[0], 0.0, 0.0],
                        [0.0, factors[1], 0.0],
                        [0.0, 0.0, factors[2]],
                    ],
                    translation: [0.0; 3],
                };
                let placed = transform(&own.multiply(&element_placement))
                    .after(&scaled)
                    .after(&transform(&inner))
                    .after(&local);
                let index = self.plan(&target, component, Some(placed), shown);
                self.planned[index].name = label.clone();
                self.planned[index].owner = (key.doc == 0).then(|| key.name.clone());
                scaled_copies += 1;
                if record {
                    self.placed.push(Placed {
                        object: key.name.clone(),
                        element,
                        path: path.to_vec(),
                        what: Measured::Body(index),
                        visible: shown,
                    });
                }
                continue;
            }
            let Some(occurrence) = self.occurrence(&target, component, &at, path, false, depth + 1)
            else {
                continue;
            };
            if !shown && let Err(e) = self.doc.set_occurrence_visible(occurrence, false) {
                self.warn(format!("{label}: {e}"));
            }
            self.occurrences_of
                .entry(key.clone())
                .or_default()
                .push(occurrence);
            if record {
                let mut path = path.to_vec();
                path.push(occurrence);
                self.placed.push(Placed {
                    object: key.name.clone(),
                    element,
                    path,
                    what: Measured::Under,
                    visible: shown,
                });
            }
        }
        if scaled_copies > 0 && key.doc == 0 {
            self.roles.insert(
                key.name.clone(),
                (
                    ObjectOutcome::Body,
                    Some(
                        "a scaled link: a scaled copy of the body (Mitcad's placements are rigid)"
                            .to_owned(),
                    ),
                ),
            );
        }
    }

    fn skip_link(&mut self, key: &Key, label: &str, why: String) {
        if key.doc == 0 {
            self.roles
                .insert(key.name.clone(), (ObjectOutcome::Skipped, Some(why)));
        } else {
            self.warn(format!("{label}: {why}"));
        }
    }

    /// Grounded joints ground their parts' occurrences.
    fn ground(&mut self) {
        let joints: Vec<(String, String)> = self.sources[0]
            .file
            .document
            .objects
            .iter()
            .filter_map(|o| Some((o.name.clone(), o.link("ObjectToGround")?.object.clone())))
            .collect();
        for (joint, grounded) in joints {
            let key = Key {
                doc: 0,
                name: grounded,
            };
            let occurrences = self.occurrences_of.get(&key).cloned().unwrap_or_default();
            if occurrences.is_empty() {
                self.warn(format!("{joint}: nothing placed to ground"));
            }
            for o in occurrences {
                if let Err(e) = self.doc.set_occurrence_grounded(o, true) {
                    self.warn(format!("{joint}: {e}"));
                }
            }
            if let Some(entry) = self.roles.get_mut(&joint) {
                entry.1 = Some(format!(
                    "a grounded joint: {} is grounded",
                    self.sources[0].label(&key.name)
                ));
            }
        }
    }

    /// Reads the planned bodies' shapes and adds a base feature per
    /// component; hides the hidden bodies.
    fn make_bodies(&mut self) -> Result<(), String> {
        let mut order: Vec<ComponentUid> = Vec::new();
        // The bodies of each component, with their indices into `planned`.
        type Bodies<S> = Vec<(usize, ImportBody<S>)>;
        let mut inputs: BTreeMap<ComponentUid, Bodies<K::Shape>> = BTreeMap::new();
        // The history makes these.
        let replayed: HashSet<usize> = self.replay.roots.values().copied().collect();
        for index in 0..self.planned.len() {
            if replayed.contains(&index) {
                continue;
            }
            let shape = match self.read_shape(index) {
                Ok(shape) => shape,
                Err(why) => {
                    let key = self.planned[index].key.clone();
                    if key.doc == 0 {
                        self.roles
                            .insert(key.name.clone(), (ObjectOutcome::Skipped, Some(why)));
                    } else {
                        let label = self.source(key.doc).label(&key.name);
                        self.warn(format!("{label}: {why}"));
                    }
                    continue;
                }
            };
            let p = &self.planned[index];
            if !order.contains(&p.component) {
                order.push(p.component);
            }
            inputs.entry(p.component).or_default().push((
                index,
                ImportBody {
                    name: Some(p.name.clone()),
                    color: p.color,
                    shape,
                },
            ));
        }
        if order.is_empty() {
            return Ok(());
        }
        let source = self.report.file.clone();
        let mut made: Vec<Vec<usize>> = Vec::new();
        let mut bases = Vec::new();
        let mut taken: HashSet<String> = self.doc.features().map(|f| f.name.clone()).collect();
        for component in &order {
            let bodies = inputs.remove(component).unwrap_or_default();
            made.push(bodies.iter().map(|(i, _)| *i).collect());
            // Feature names are unique: the component's name tells them
            // apart.
            let base = match self.component_names.get(component) {
                Some(c) => format!("FreeCAD bodies of {c}"),
                None => "FreeCAD bodies".to_owned(),
            };
            let name = std::iter::once(base.clone())
                .chain((2..).map(|n| format!("{base} ({n})")))
                .find(|n| !taken.contains(n))
                .expect("a free name");
            taken.insert(name.clone());
            bases.push(BaseInput {
                name: Some(name),
                source: Some(source.clone()),
                component: Some(*component),
                ..BaseInput::new(bodies.into_iter().map(|(_, b)| b).collect())
            });
        }
        let imported = self
            .doc
            .add_base_features(&format!("Import {source}"), bases)
            .map_err(|e| e.to_string())?;
        for (feature, indices) in imported.iter().zip(made) {
            for (i, index) in indices.into_iter().enumerate() {
                self.planned[index].body = Some(BodyUid::new(feature.feature.uid, i as u32));
            }
        }
        for p in &self.planned {
            if let Some(body) = p.body
                && !p.visible
                && let Err(e) = self.doc.set_body_visible(body, false)
            {
                self.report.warnings.push(format!("{}: {e}", p.name));
            }
        }
        Ok(())
    }

    /// A planned body's shape: the stored B-rep, moved when asked.
    fn read_shape(&self, index: usize) -> Result<K::Shape, String> {
        let p = &self.planned[index];
        let source = self.source(p.key.doc);
        let data = source
            .file
            .shape_data(&p.key.name)
            .map_err(|e| format!("its stored shape could not be read: {e}"))?
            .ok_or("empty: no stored shape")?;
        let kernel = self.doc.kernel();
        let (shape, _) = kernel
            .import_brep(FeatureUid(0), &data, 0)
            .map_err(|e| format!("its stored shape could not be read: {e}"))?;
        match kernel.body_kind(&shape) {
            Ok(BodyKind::Empty) => return Err("no faces (a wire, an edge or points)".to_owned()),
            Ok(_) => {}
            Err(e) => return Err(format!("its stored shape could not be read: {e}")),
        }
        match &p.transform {
            Some(t) => kernel
                .transform_shape(&shape, t, None)
                .map_err(|e| format!("its shape could not be moved: {e}")),
            None => Ok(shape),
        }
    }

    /// The report's items, bodies and places.
    fn finish_report(&mut self) {
        // Sketches that changed with the parameters: measured again.
        if self.params.changed {
            for (index, uid, place) in self.measured_sketches.clone() {
                let mut report = std::mem::take(&mut self.report.sketches[index]);
                self.measure_sketch(uid, &place, &mut report);
                self.report.sketches[index] = report;
            }
        }
        let mut bodies_of: HashMap<String, Vec<String>> = HashMap::new();
        let mut component_of: HashMap<String, String> = HashMap::new();
        for p in &self.planned {
            let Some(uid) = p.body else {
                continue;
            };
            let kernel = self.doc.kernel();
            let shape = self.doc.body_shape(uid);
            let measured = shape.and_then(|s| measure(kernel, s));
            let kind = shape
                .and_then(|s| kernel.body_kind(s).ok())
                .map_or("?", |k| k.as_str());
            let component =
                (!p.component.is_root()).then(|| self.component_names[&p.component].clone());
            if let Some(owner) = &p.owner {
                bodies_of
                    .entry(owner.clone())
                    .or_default()
                    .push(uid.to_string());
                if let Some(c) = &component {
                    component_of.insert(owner.clone(), c.clone());
                }
            }
            self.report.bodies.push(BodyReport {
                object: p.owner.clone().unwrap_or_else(|| p.key.name.clone()),
                file: (p.key.doc != 0).then(|| {
                    self.sources[p.key.doc]
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                }),
                uid: uid.to_string(),
                name: self.doc.body_name(uid),
                component,
                kind: kind.to_owned(),
                volume: measured.map_or(0.0, |m| m.volume),
                area: measured.map_or(0.0, |m| m.area),
                center: measured.map_or([0.0; 3], |m| m.center),
                color: p.color,
                visible: p.visible,
            });
        }
        for (key, components) in &self.components {
            if key.doc == 0 {
                component_of
                    .entry(key.name.clone())
                    .or_insert_with(|| self.component_names[components].clone());
            }
        }
        let source = &self.sources[0];
        let doc = &source.file.document;
        for (index, o) in doc.objects.iter().enumerate() {
            // Objects the walk did not reach: inside bodies, arrays and
            // origins, or below what was left out.
            let (outcome, note) = self
                .roles
                .get(&o.name)
                .cloned()
                .unwrap_or_else(|| match source.role(o) {
                    Role::Included(why) => (ObjectOutcome::Included, Some(why)),
                    Role::Skipped(why) => (ObjectOutcome::Skipped, Some(why)),
                    _ => (
                        ObjectOutcome::Skipped,
                        Some("not reached from the document's top".to_owned()),
                    ),
                });
            let stale = o.state.is_stale() && outcome == ObjectOutcome::Body;
            if o.state.is_stale() {
                self.report.stale.push(o.name.clone());
            }
            self.report.items.push(ObjectReport {
                index,
                name: o.name.clone(),
                label: o.label().to_owned(),
                object_type: o.type_name.clone(),
                outcome,
                note,
                bodies: bodies_of.remove(&o.name).unwrap_or_default(),
                component: component_of.remove(&o.name),
                stale,
            });
        }
        self.report.placed = self.measure_places();
    }

    /// The world measures of every recorded place.
    fn measure_places(&self) -> Vec<PlacedReport> {
        let kernel = self.doc.kernel();
        let instances = self.doc.instances();
        let assembly = self.doc.assembly();
        let mut out = Vec::new();
        for place in &self.placed {
            let pieces: Vec<(BodyUid, Transform)> = match &place.what {
                Measured::Body(index) => match self.planned[*index].body {
                    Some(body) => vec![(body, self.doc.path_transform(&place.path))],
                    None => continue,
                },
                Measured::Under => instances
                    .iter()
                    .filter(|i| i.path.starts_with(&place.path))
                    .map(|i| (i.body, i.transform))
                    .collect(),
            };
            let mut solid = (0.0, [0.0; 3]);
            let mut sheet = (0.0, [0.0; 3]);
            let mut area = 0.0;
            let replayed = pieces
                .iter()
                .any(|(body, _)| self.replayed_bodies.contains(body));
            for (body, t) in &pieces {
                let Some(m) = self.doc.body_shape(*body).and_then(|s| measure(kernel, s)) else {
                    continue;
                };
                let c = t.apply_point(m.center);
                area += m.area;
                let (weight, sum) = if m.volume > 0.0 {
                    (m.volume, &mut solid)
                } else {
                    (m.area, &mut sheet)
                };
                sum.0 += weight;
                for (s, c) in sum.1.iter_mut().zip(c) {
                    *s += weight * c;
                }
            }
            let (weight, sum) = if solid.0 > 0.0 { solid } else { sheet };
            // A replayed body: the stored shape measured too.
            let stored = match &place.what {
                Measured::Body(index) if replayed => self
                    .read_shape(*index)
                    .ok()
                    .and_then(|s| measure(kernel, &s))
                    .map(|m| report::StoredMeasures {
                        volume: m.volume.abs(),
                        area: m.area,
                        center: self.doc.path_transform(&place.path).apply_point(m.center),
                    }),
                _ => None,
            };
            out.push(PlacedReport {
                object: place.object.clone(),
                element: place.element,
                path: assembly.path_name(&place.path),
                bodies: pieces.len(),
                volume: solid.0,
                area,
                center: if weight > 0.0 {
                    sum.map(|s| s / weight)
                } else {
                    [0.0; 3]
                },
                visible: place.visible,
                replayed,
                stored,
            });
        }
        out
    }
}

/// A body's volume (of its closed solids), area and centre of mass (of the
/// volume, of the surface for a body without volume).
fn measure<K: Kernel>(kernel: &K, shape: &K::Shape) -> Option<MassProperties> {
    match kernel.physical_properties(shape, 1.0) {
        Ok(p) => Some(MassProperties {
            volume: p.volume,
            area: p.area,
            center: p.center_of_mass,
        }),
        Err(_) => kernel.mass_properties(shape).ok(),
    }
}

/// An object's placement (identity when it has none).
fn placement_of(o: &Object) -> Placement {
    o.placement().unwrap_or_default()
}

/// A link array's elements: placement, visibility and scale of each; None
/// for a plain link. Element objects (`ShowElement`) carry their own
/// placements; without them the link's `PlacementList` does.
fn elements(source: &Source, o: &Object) -> Option<Vec<(Placement, bool, [f64; 3])>> {
    let count = usize::try_from(o.i64("ElementCount").unwrap_or(0)).unwrap_or(0);
    if count == 0 {
        return None;
    }
    let element_objects = o.links("ElementList");
    let list = match o.value("PlacementList") {
        Some(Value::PlacementList(list)) => list.clone(),
        _ => Vec::new(),
    };
    let visibility = match o.value("VisibilityList") {
        Some(Value::BoolList(v)) => v.clone(),
        _ => Vec::new(),
    };
    let scales = match o.value("ScaleList") {
        Some(Value::VectorList(v)) => v.clone(),
        _ => Vec::new(),
    };
    Some(
        (0..count)
            .map(|i| {
                let placement = element_objects
                    .get(i)
                    .and_then(|e| source.object(&e.object))
                    .and_then(Object::placement)
                    .or_else(|| list.get(i).copied())
                    .unwrap_or_default();
                let visible = visibility.get(i).copied().unwrap_or(true);
                let scale = scales.get(i).copied().unwrap_or([1.0; 3]);
                (placement, visible, scale)
            })
            .collect(),
    )
}

/// A link's scale (`ScaleVector`, else `Scale`).
fn link_scale(o: &Object) -> [f64; 3] {
    match o.value("ScaleVector") {
        Some(Value::Vector(v)) => *v,
        _ => [o.f64("Scale").unwrap_or(1.0); 3],
    }
}

/// FreeCAD's default shape colours (0.21's grey, 1.0's default
/// appearance): not imported, so that the bodies keep Mitcad's look.
const DEFAULT_COLORS: [[u8; 3]; 2] = [[204, 204, 204], [204, 204, 230]];

/// The colour of an object's body: the view's shape colour, unless it is
/// FreeCAD's default or the faces have colours of their own.
fn body_color(file: &FcstdFile, o: &Object) -> Option<[f64; 3]> {
    let view = file.view(&o.name)?;
    let faces = view.face_colors();
    let first = faces.first().map(Color::rgb8);
    if faces.len() > 1 && faces.iter().any(|c| Some(c.rgb8()) != first) {
        return None;
    }
    let color = view.shape_color()?;
    (!DEFAULT_COLORS.contains(&color.rgb8())).then(|| color.rgb())
}
