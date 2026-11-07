// SPDX-License-Identifier: MIT
//! Data exchange: base features from imported files and from bodies an
//! importer built, export of bodies (STEP, IGES, STL, OBJ, BRep, 3MF,
//! USDZ) and of a sketch's geometry (DXF).
//!
//! File formats are the kernel's business ([`Kernel::read_file`],
//! [`Kernel::write_file`], which takes each body's placements, mitcad#19);
//! DXF is written by `mitcad-dxf` from the evaluated sketch, and 3MF by
//! `mitcad-3mf` and STL by the model (`stl`) from the kernel's triangle
//! meshes ([`Kernel::triangle_mesh`], mitcad#13, mitcad#17); USDZ by
//! `mitcad-usdz` from the kernel's face meshes ([`Kernel::face_meshes`],
//! `usdz`, mitcad#123). Every format has the bodies where the design shows
//! them unless the components' coordinates are asked for.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::document::{Added, BodyView, Document, InstanceView, ModelError};
use crate::features::{BaseBody, BaseDef, Brep, FeatureDef, Operation, ValueInput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::profile::SegmentGeometry;
use crate::transform::Transform;

mod stl;
pub(crate) mod usdz;
/// The triangles of an STL file (binary or text), for tests (mitcad#777).
#[cfg(any(test, feature = "testing"))]
pub use stl::read as read_stl;

// The bodies on the print bed by their print bed faces (mitcad#731).
mod bed;
pub use bed::{BedChoice, BedObject, DEFAULT_PLATE, OnBed, Settle};

// The design's print plates in 3MF files (mitcad#1050).
mod plates;
pub use plates::{ExportedPlate, PlateFiles};
// The automatic orientation on the print bed (mitcad#1109).
pub mod orient;

// Meshes with a longest edge (mitcad#734, `MeshTolerance::max_edge`).
mod edges;

// STEP assemblies as components and occurrences (mitcad#1274).
mod assembly_import;
pub use assembly_import::{ImportStructure, ImportedComponent, ImportedFile, file_has_assembly};

// The data the kernel reads and writes, below the model (mitcad#777).
pub use mitcad_model_base::exchange::{
    BodyKind, BodyPlacement, ExportBody, ExportFormat, ExportOptions, FaceMeshes, FileRead,
    ImportAssembly, ImportBody, ImportInstance, ImportPart, LengthUnit, MeshTolerance, ReadOptions,
    Refinement, RefinementPreset, StepSchema, TriangleMesh,
};

/// Where exported bodies are (mitcad#17, mitcad#19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coordinates {
    /// Where the design shows them: placed by their components'
    /// occurrences, a body placed twice written twice (the default).
    Design,
    /// Each body once, in its own component's coordinates.
    Component,
}

/// Which placements of the bodies [`Document::export_bodies`] writes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportPlacement {
    /// The design's coordinates when `None`.
    pub coordinates: Option<Coordinates>,
    /// Only the placements by this occurrence (a path from the root) and
    /// the occurrences in it; in the design's coordinates.
    pub within: Option<Vec<OccurrenceUid>>,
    /// 3MF and STL: the bodies on the print bed by their print bed faces
    /// (mitcad#731, `bed.rs`).
    pub on_bed: bool,
    /// 3MF (mitcad#856): per body, the occurrences (paths from the root)
    /// whose placements are written, all of them for an empty list; a body
    /// not listed writes its placements as the design shows them. On the
    /// print bed, two or more placements picked of bodies with usable
    /// print bed faces are each an object of its own (`bed.rs`).
    pub occurrences: BTreeMap<BodyUid, Vec<Vec<OccurrenceUid>>>,
    /// 3MF on the print bed (mitcad#856): occurrences sent whole, the
    /// placements in each one object, as they are assembled.
    pub whole: Vec<Vec<OccurrenceUid>>,
    /// On the print bed (mitcad#857): print bed faces chosen, each a body
    /// and the path of the placement whose face it is (empty for the design
    /// itself): the piece it is in turns onto it, whatever the rule says
    /// (`bed.rs`); one per piece.
    pub bed_faces: Vec<(BodyUid, Vec<OccurrenceUid>)>,
    /// On the print bed (mitcad#857): only how the bodies would lie, with
    /// the faces each piece kept as assembled could lie on; no file is
    /// written and nothing meshed but the faces the automatic orientation
    /// looks at (mitcad#1109; the transforms are the turns alone).
    pub plan_only: bool,
    /// On the print bed (mitcad#1109): a piece without a usable print bed
    /// face is turned onto the face the automatic orientation finds
    /// (`orient.rs`).
    pub auto_orient: bool,
    /// On the print bed (mitcad#1109): each piece's box centred over the
    /// plate and its lowest point on the bed, also without a bed face and
    /// kept as assembled (both by default).
    pub settle: Settle,
    /// 3MF on the print bed (mitcad#1050): the design's print plates
    /// instead of the bodies chosen, in one file, a file per plate or one
    /// plate (`plates.rs`).
    pub plates: Option<PlateFiles>,
}

/// A 3MF model before its slicer's settings (mitcad#1050,
/// [`Document::build_3mf`]): its parts and objects, each part's effective
/// settings (with a slicer's) and what was written.
pub(crate) struct Built3mf {
    pub model: mitcad_3mf::Model,
    pub settings: Vec<crate::document::EffectivePrint>,
    pub exported: Exported,
}

/// The print plate a 3MF model is built on (mitcad#1050): the plate whose
/// settings its bodies take (none: each body its own plate's) and the
/// move of the placements onto the plate's place in the file, mm.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ThreeMfPlate<'a> {
    pub plate: Option<&'a crate::print_plates::PrintPlate>,
    pub shift: [f64; 2],
}

/// What [`Document::export_bodies`] wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exported {
    /// The bodies written, in order.
    pub bodies: Vec<BodyUid>,
    /// Bodies left out and why (3MF of all bodies: sheet and empty bodies;
    /// USDZ of all bodies: hidden and empty bodies).
    pub skipped: Vec<(BodyUid, String)>,
    /// 3MF: the mesh of each written body, in `bodies` order.
    pub meshes: Vec<MeshSummary>,
    /// USDZ: the materials written (appearance ids, `color rrggbb`,
    /// `default look`).
    pub materials: Vec<String>,
    /// USDZ: what was written otherwise than the design has it (a texture
    /// whose image cannot be read); 3MF with a slicer's settings: what the
    /// slicer does not take as the design has it (mitcad#640).
    pub warnings: Vec<String>,
    /// 3MF with a slicer's settings (mitcad#640): the filament slot of each
    /// written part (0 the first), in `parts` order.
    pub slots: Vec<usize>,
    /// 3MF with a slicer's settings (mitcad#1052): each written part's
    /// material (its id) and the number of its own values written, in
    /// `parts` order.
    pub printed: Vec<(String, usize)>,
    /// 3MF: the parts written, each its body and name (mitcad#1061). A
    /// body is one part, named after it; with a slicer's settings, a body
    /// whose written placements print otherwise is one part per group of
    /// placements that print alike, named after the body and the group's
    /// occurrences (`Body2 (Box:2)`).
    pub parts: Vec<(BodyUid, String)>,
    /// 3MF and STL on the print bed (mitcad#731): how the bodies were put
    /// there.
    pub on_bed: Option<OnBed>,
    /// 3MF of the print plates (mitcad#1050): each plate as written, in
    /// the design's order.
    pub plates: Vec<ExportedPlate>,
}

/// A body's mesh in a 3MF file: its triangles and the volume they enclose
/// (mm³), which differs from the body's by the refinement's deviation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshSummary {
    pub triangles: usize,
    pub volume: f64,
}

/// A base feature to add from shapes (see [`Document::add_base_feature`]).
#[derive(Debug, Clone)]
pub struct BaseInput<S> {
    /// The feature's name; `Base<n>` when None.
    pub name: Option<String>,
    /// Where the bodies come from, e.g. a file name.
    pub source: Option<String>,
    pub bodies: Vec<ImportBody<S>>,
    pub operation: Operation,
    /// For join, cut and intersect; empty for all bodies.
    pub participants: Vec<BodyUid>,
    /// New bodies only: the bodies they replace, body i taking the id and
    /// name of `replaces[i]` (see [`crate::features::BaseDef::replaces`]).
    pub replaces: Vec<BodyUid>,
    /// The component it goes into (F6); the active component when None.
    pub component: Option<ComponentUid>,
    /// The control cages of the bodies that have one (an imported Form
    /// body, mitcad#1365): `cages[i]` is body i's; bodies past the end of
    /// the list have none.
    pub cages: Vec<Option<Box<crate::features::ControlCage>>>,
}

impl<S> BaseInput<S> {
    /// New bodies from the shapes.
    pub fn new(bodies: Vec<ImportBody<S>>) -> Self {
        Self {
            name: None,
            source: None,
            bodies,
            operation: Operation::NewBody,
            participants: Vec::new(),
            replaces: Vec::new(),
            component: None,
            cages: Vec::new(),
        }
    }
}

/// A body a base feature created.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedBody {
    pub uid: BodyUid,
    pub name: String,
    pub kind: BodyKind,
}

/// The result of an import: the base feature and the bodies it created.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub feature: Added,
    pub bodies: Vec<ImportedBody>,
    /// What the import left out ([`Document::import_file`]: empty bodies).
    pub warnings: Vec<String>,
}

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

fn kernel_error(e: crate::kernel::KernelError) -> ModelError {
    invalid(e.to_string())
}

/// Whether two placements of a body print alike (mitcad#1052): the same
/// material, values and slicers' own keys, wherever each comes from.
fn same_print(a: &crate::document::EffectivePrint, b: &crate::document::EffectivePrint) -> bool {
    fn plain<V, F>(map: &BTreeMap<String, (V, F)>) -> impl Iterator<Item = (&String, &V)> {
        map.iter().map(|(k, (v, _))| (k, v))
    }
    a.material == b.material
        && plain(&a.values).eq(plain(&b.values))
        && plain(&a.filament).eq(plain(&b.filament))
        && a.slicer.len() == b.slicer.len()
        && a.slicer
            .iter()
            .zip(&b.slicer)
            .all(|((da, ka), (db, kb))| da == db && plain(ka).eq(plain(kb)))
}

/// The unit of a 3MF file's model.
fn three_mf(unit: LengthUnit) -> mitcad_3mf::Unit {
    match unit {
        LengthUnit::Millimeter => mitcad_3mf::Unit::Millimeter,
        LengthUnit::Centimeter => mitcad_3mf::Unit::Centimeter,
        LengthUnit::Meter => mitcad_3mf::Unit::Meter,
        LengthUnit::Inch => mitcad_3mf::Unit::Inch,
        LengthUnit::Foot => mitcad_3mf::Unit::Foot,
    }
}

/// Refuses a file unit that is not a positive length.
pub(crate) fn check_unit(options: &ReadOptions) -> Result<(), ModelError> {
    if options.unit_mm > 0.0 && options.unit_mm.is_finite() {
        Ok(())
    } else {
        Err(invalid(format!(
            "the file unit must be greater than zero, got {} mm",
            options.unit_mm
        )))
    }
}

/// The file name without directories, for the `source` of a base feature.
pub(crate) fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_owned()
}

/// `name`, or `name (2)`, `name (3)`, ... the first one not taken.
pub(crate) fn unique_name(name: &str, taken: &mut BTreeSet<String>) -> String {
    let name = name.trim();
    let unique = std::iter::once(name.to_owned())
        .chain((2..).map(|n| format!("{name} ({n})")))
        .find(|n| !taken.contains(n))
        .expect("an unused name exists");
    taken.insert(unique.clone());
    unique
}

impl<K: Kernel> Document<K> {
    /// Shapes as the bodies of a base feature: B-rep data with their names
    /// and colours.
    pub(crate) fn base_bodies(
        &self,
        bodies: &[ImportBody<K::Shape>],
    ) -> Result<Vec<BaseBody>, ModelError> {
        let mut out = Vec::with_capacity(bodies.len());
        for (i, body) in bodies.iter().enumerate() {
            let fail = |e: crate::kernel::KernelError| invalid(format!("body {i}: {e}"));
            let kind = self.kernel().body_kind(&body.shape).map_err(fail)?;
            if kind == BodyKind::Empty {
                return Err(invalid(format!("body {i} is empty")));
            }
            let data = self.kernel().brep_data(&body.shape).map_err(fail)?;
            out.push(BaseBody {
                name: body.name.clone().filter(|n| !n.trim().is_empty()),
                color: body.color,
                mesh: kind == BodyKind::Mesh,
                brep: Brep::new(data),
                number: None,
                first_face: None,
                cage: None,
            });
        }
        Ok(out)
    }

    /// Adds a base feature holding the shapes as bodies, at the marker:
    /// new bodies named after the shapes (unnamed ones get `Body<n>`), or a
    /// join, cut or intersect with participant bodies. This is how an
    /// importer adds bodies it built itself, such as the .f3d importer's
    /// bodies without history and its fallback for features it cannot
    /// replay. The shapes are stored as B-rep data in the feature.
    pub fn add_base_feature(&mut self, base: BaseInput<K::Shape>) -> Result<Imported, ModelError> {
        let label = match &base.name {
            Some(name) => format!("Add {name}"),
            None => "Add Base Feature".to_owned(),
        };
        let mut imported = self.add_base_features(&label, vec![base])?;
        Ok(imported.remove(0))
    }

    /// Adds several base features as one undo step named `label`.
    pub fn add_base_features(
        &mut self,
        label: &str,
        features: Vec<BaseInput<K::Shape>>,
    ) -> Result<Vec<Imported>, ModelError> {
        if features.is_empty() {
            return Err(invalid("nothing to import"));
        }
        let mut defs = Vec::with_capacity(features.len());
        // Each body's data with the shape it was written from: the
        // evaluation adopts the shape rather than reading the data back
        // (mitcad#244).
        let mut written_from = Vec::new();
        for base in &features {
            let component = base.component.unwrap_or(self.active_component());
            self.state().editable(component)?;
            let mut bodies = self.base_bodies(&base.bodies)?;
            // The bodies' control cages (mitcad#1365).
            for (body, cage) in bodies.iter_mut().zip(&base.cages) {
                body.cage.clone_from(cage);
            }
            for (body, input) in bodies.iter().zip(&base.bodies) {
                written_from.push((body.brep.clone(), input.shape.clone()));
            }
            let def = FeatureDef::<ValueInput>::Base(BaseDef {
                operation: base.operation,
                participants: base.participants.clone(),
                source: base.source.clone(),
                replaces: base.replaces.clone(),
                ..BaseDef::new(bodies)
            });
            defs.push((def, base.name.clone(), component));
        }
        let mut taken: BTreeSet<String> = self.bodies().into_iter().map(|b| b.name).collect();
        let added = self.with_written_from(written_from, |doc| {
            doc.apply(|state| {
                taken.extend(state.body_names.values().cloned());
                let mut added = Vec::with_capacity(defs.len());
                for (def, name, component) in &defs {
                    let feature = state.add_feature_in(def, name.as_deref(), *component)?;
                    if let FeatureDef::Base(base) = def
                        && base.operation == Operation::NewBody
                    {
                        // Bodies that replace others keep those bodies' names.
                        let replacing = base.replaces.len();
                        for (i, body) in (0..).zip(&base.bodies).skip(replacing) {
                            if let Some(name) = &body.name {
                                let name = unique_name(name, &mut taken);
                                state.body_names.insert(BodyUid::new(feature.uid, i), name);
                            }
                        }
                    }
                    added.push(feature);
                }
                Ok((label.to_owned(), added))
            })
        })?;
        let all = self.bodies();
        let mut result = Vec::with_capacity(added.len());
        for (feature, base) in added.into_iter().zip(&features) {
            let mut bodies = Vec::new();
            let mine = |uid: BodyUid| uid.feature == feature.uid || base.replaces.contains(&uid);
            for body in all.iter().filter(|b| mine(b.uid)) {
                bodies.push(ImportedBody {
                    uid: body.uid,
                    name: body.name.clone(),
                    kind: self.kernel().body_kind(body.shape).map_err(kernel_error)?,
                });
            }
            result.push(Imported {
                feature,
                bodies,
                warnings: Vec::new(),
            });
        }
        Ok(result)
    }

    /// Imports a STEP, IGES, BRep, STL or OBJ file as a base feature: one
    /// body per part or solid (STEP, IGES, BRep) or object (OBJ), a mesh
    /// body for STL. Empty bodies (a part without geometry) are left out
    /// with a warning; a file of only empty bodies is refused.
    pub fn import_file(
        &mut self,
        path: &str,
        options: &ReadOptions,
        name: Option<&str>,
    ) -> Result<Imported, ModelError> {
        check_unit(options)?;
        let bodies = self
            .kernel()
            .read_file(path, options)
            .map_err(kernel_error)?;
        self.import_read_bodies(path, bodies, name)
    }

    /// The bodies read from the file at `path` as one base feature
    /// ([`Document::import_file`]).
    pub(crate) fn import_read_bodies(
        &mut self,
        path: &str,
        bodies: Vec<ImportBody<K::Shape>>,
        name: Option<&str>,
    ) -> Result<Imported, ModelError> {
        if bodies.is_empty() {
            return Err(invalid(format!("{path} has no bodies")));
        }
        // Here, unlike in base_bodies, an empty body is no error: nothing
        // refers to the file's bodies by index yet.
        let count = bodies.len();
        let mut kept = Vec::with_capacity(count);
        let mut warnings = Vec::new();
        for (i, body) in bodies.into_iter().enumerate() {
            let kind = self
                .kernel()
                .body_kind(&body.shape)
                .map_err(|e| invalid(format!("body {i}: {e}")))?;
            if kind == BodyKind::Empty {
                let named = match body.name.as_deref().map(str::trim) {
                    Some(name) if !name.is_empty() => format!("body {i} ({name})"),
                    _ => format!("body {i}"),
                };
                warnings.push(format!("{named} is empty: left out"));
            } else {
                kept.push(body);
            }
        }
        if kept.is_empty() {
            return Err(invalid(format!(
                "{path} has only empty bodies ({count}): nothing to import"
            )));
        }
        let source = file_name(path);
        let base = BaseInput {
            name: name.map(str::to_owned),
            source: Some(source.clone()),
            ..BaseInput::new(kept)
        };
        let mut imported = self
            .add_base_features(&format!("Import {source}"), vec![base])?
            .remove(0);
        imported.warnings = warnings;
        Ok(imported)
    }

    /// Writes bodies at the marker to a file; all bodies when `bodies` is
    /// empty. Bodies keep their display names, and `colors` or else the
    /// colours of imported bodies. Mesh bodies go only to STL, OBJ, BRep,
    /// 3MF and USDZ files. Every format has the bodies where the design
    /// shows them (mitcad#17, mitcad#19), unless `placement` asks for the components'
    /// coordinates; with an occurrence, only its placements (all bodies:
    /// those it places). A 3MF file holds the solids and meshes (of all
    /// bodies, sheet and empty bodies are left out) as the parts of one
    /// object, each placed where its component's occurrences place it; a
    /// STEP file of moved bodies is an assembly of their parts; a USDZ file
    /// keeps the occurrences as a hierarchy and the bodies' appearances (of
    /// all bodies, hidden ones are left out; `usdz`).
    pub fn export_bodies(
        &self,
        path: &str,
        bodies: &[BodyUid],
        options: &ExportOptions,
        colors: &BTreeMap<BodyUid, [f64; 3]>,
        placement: &ExportPlacement,
    ) -> Result<Exported, ModelError> {
        let format = options.format;
        // The design's print plates (mitcad#1050, `plates.rs`).
        if let Some(files) = placement.plates {
            return self.export_plates(path, bodies, options, colors, placement, files);
        }
        let design = placement.coordinates != Some(Coordinates::Component);
        let within = placement.within.as_deref();
        if within.is_some() && !design {
            return Err(invalid(
                "the placements of an occurrence are in the design's coordinates",
            ));
        }
        if placement.on_bed {
            if !matches!(
                format,
                ExportFormat::ThreeMf | ExportFormat::Stl | ExportFormat::Obj
            ) {
                return Err(invalid(
                    "only 3MF, STL and OBJ files put the bodies on the print bed",
                ));
            }
            if !design {
                return Err(invalid(
                    "bodies are put on the print bed from the design's coordinates",
                ));
            }
        } else if !placement.bed_faces.is_empty() || placement.plan_only {
            // mitcad#857.
            return Err(invalid(
                "a print bed face chosen, and the plan, are for the bodies put on the print bed (on_bed)",
            ));
        }
        // The occurrences picked, the occurrences sent whole (mitcad#856).
        if !placement.occurrences.is_empty() || !placement.whole.is_empty() {
            if format != ExportFormat::ThreeMf {
                return Err(invalid(
                    "only 3MF files take the occurrences to write and those sent whole",
                ));
            }
            if !design {
                return Err(invalid(
                    "the occurrences to write are in the design's coordinates",
                ));
            }
        }
        if placement.whole.iter().any(Vec::is_empty) {
            return Err(invalid(
                "an occurrence sent whole is an occurrence, not the design",
            ));
        }
        let all = self.bodies();
        let mut chosen: Vec<_> = if bodies.is_empty() {
            all.iter().collect()
        } else {
            let mut chosen = Vec::new();
            for (i, uid) in bodies.iter().enumerate() {
                if bodies[..i].contains(uid) {
                    return Err(invalid(format!("body {uid} is listed more than once")));
                }
                chosen.push(all.iter().find(|b| b.uid == *uid).ok_or_else(|| {
                    invalid(format!("body {uid} does not exist at the timeline marker"))
                })?);
            }
            chosen
        };
        // Where the design shows the bodies: the instances of their
        // components (within the occurrence).
        let instances = if design { self.instances() } else { Vec::new() };
        let inside = |i: &InstanceView| within.is_none_or(|w| i.path.starts_with(w));
        let placed = |body: BodyUid| instances.iter().any(|i| i.body == body && inside(i));
        if let Some(occurrence) = within {
            if bodies.is_empty() {
                chosen.retain(|b| placed(b.uid));
            } else if let Some(body) = chosen.iter().find(|b| !placed(b.uid)) {
                return Err(invalid(format!(
                    "{} is not placed in {}",
                    body.name,
                    self.assembly().path_name(occurrence)
                )));
            }
        }
        if chosen.is_empty() {
            return Err(invalid("there are no bodies to export"));
        }
        // Each body's occurrences picked place it (mitcad#856).
        for (body, paths) in &placement.occurrences {
            let Some(view) = chosen.iter().find(|b| b.uid == *body) else {
                return Err(invalid(format!(
                    "{} is not among the bodies written",
                    self.body_name(*body)
                )));
            };
            for path in paths {
                if !instances
                    .iter()
                    .any(|i| i.body == *body && i.path == *path && inside(i))
                {
                    let name = self.assembly().path_name(path);
                    return Err(invalid(if name.is_empty() {
                        format!("{} is not placed by the design itself", view.name)
                    } else {
                        format!("{} is not placed by {name}", view.name)
                    }));
                }
            }
        }
        let tolerance = options.refinement.tolerance();
        if !(tolerance.deviation > 0.0 && tolerance.angle > 0.0) {
            return Err(invalid(
                "the mesh deviation and angle must be greater than zero",
            ));
        }
        if !(tolerance.max_edge >= 0.0 && tolerance.max_edge.is_finite()) {
            return Err(invalid(
                "the maximum edge length must be greater than zero (or 0 for none)",
            ));
        }
        let color = |uid: BodyUid| {
            colors
                .get(&uid)
                .copied()
                .or_else(|| self.imported_color(uid))
        };
        // A body's placements: by the occurrences picked (mitcad#856), else
        // by the shown occurrences of its component, by all of them when
        // none is shown; once in component coordinates.
        let instances_of = |body: BodyUid| {
            let mine: Vec<&InstanceView> = instances
                .iter()
                .filter(|i| i.body == body && inside(i))
                .collect();
            let picked = placement.occurrences.get(&body).filter(|p| !p.is_empty());
            let shown: Vec<&InstanceView> = mine
                .iter()
                .copied()
                .filter(|i| picked.map_or(i.visible, |paths| paths.contains(&i.path)))
                .collect();
            if shown.is_empty() && picked.is_none() {
                mine
            } else {
                shown
            }
        };
        let placed_at = |body: BodyUid| {
            let mut out: Vec<BodyPlacement> = instances_of(body)
                .iter()
                .map(|i| BodyPlacement {
                    transform: i.transform,
                    name: i.path_name.clone(),
                })
                .collect();
            if out.is_empty() {
                out.push(BodyPlacement {
                    transform: Transform::IDENTITY,
                    name: String::new(),
                });
            }
            out
        };
        let placements = |body: BodyUid| -> Vec<Transform> {
            placed_at(body).into_iter().map(|p| p.transform).collect()
        };
        // The occurrence paths of the placements, None for a body not
        // placed (mitcad#856).
        let paths = |body: BodyUid| -> Vec<Option<Vec<OccurrenceUid>>> {
            let found: Vec<Option<Vec<OccurrenceUid>>> = instances_of(body)
                .iter()
                .map(|i| Some(i.path.clone()))
                .collect();
            if found.is_empty() { vec![None] } else { found }
        };
        // The print bed faces chosen (mitcad#857): of a body written, with
        // a face that can lie on the bed, at a placement written.
        for (body, path) in &placement.bed_faces {
            let Some(view) = chosen.iter().find(|b| b.uid == *body) else {
                return Err(invalid(format!(
                    "{} is not among the bodies written",
                    self.body_name(*body)
                )));
            };
            match self.print_bed_face(*body).map(|face| face.plane) {
                None => {
                    return Err(invalid(format!("{} has no print bed face", view.name)));
                }
                // Gone, not planar, or of a body that is not printed.
                Some(Err(why)) => return Err(invalid(format!("{}: {why}", view.name))),
                Some(Ok(_)) => {}
            }
            if !paths(*body)
                .iter()
                .any(|p| p.as_deref().unwrap_or(&[]) == path.as_slice())
            {
                let name = self.assembly().path_name(path);
                return Err(invalid(if name.is_empty() {
                    format!(
                        "{} is not written as the design itself places it",
                        view.name
                    )
                } else {
                    format!("{} is not written where {name} places it", view.name)
                }));
            }
        }
        // On the print bed (mitcad#731): the bodies' bed faces, and the turn
        // that takes the one face down (or the automatic orientation's,
        // mitcad#1109); for 3MF, each object's (below).
        let rules = bed::BedRules {
            chosen: &placement.bed_faces,
            auto_orient: placement.auto_orient,
            settle: placement.settle,
        };
        let mut on_bed = if placement.on_bed && format != ExportFormat::ThreeMf {
            let uids: Vec<BodyUid> = chosen.iter().map(|b| b.uid).collect();
            Some(self.bed_plan(&uids, &placements, &paths, &rules)?)
        } else {
            None
        };
        // Only the plan (mitcad#857).
        if placement.plan_only && format != ExportFormat::ThreeMf {
            let mut exported = Exported {
                bodies: chosen.iter().map(|b| b.uid).collect(),
                ..Exported::default()
            };
            self.report_bed(&mut exported, on_bed);
            return Ok(exported);
        }
        if format == ExportFormat::ThreeMf {
            // On the print bed as objects (mitcad#856), a face chosen
            // turning the one it is in (mitcad#857).
            let mut pieces = if placement.on_bed {
                let uids: Vec<BodyUid> = chosen.iter().map(|b| b.uid).collect();
                Some(self.bed_pieces(
                    &uids,
                    &placements,
                    &paths,
                    &placement.occurrences,
                    &placement.whole,
                    &rules,
                )?)
            } else {
                None
            };
            // Only the plan (mitcad#857): the pieces' turns, unwritten.
            if placement.plan_only {
                let mut exported = Exported {
                    bodies: chosen.iter().map(|b| b.uid).collect(),
                    ..Exported::default()
                };
                let merged = pieces.map(|p| self.merged_bed(p, placement.settle));
                self.report_bed(&mut exported, merged);
                return Ok(exported);
            }
            let mut exported = self.export_3mf(
                path,
                &chosen,
                bodies.is_empty(),
                &tolerance,
                options.unit,
                color,
                placements,
                options.dialect.map(|d| {
                    (
                        d,
                        &paths as &dyn Fn(BodyUid) -> Vec<Option<Vec<OccurrenceUid>>>,
                    )
                }),
                pieces.as_mut(),
            )?;
            if let Some(pieces) = pieces {
                on_bed = Some(self.merged_bed(pieces, placement.settle));
            }
            self.report_bed(&mut exported, on_bed);
            return Ok(exported);
        }
        if format == ExportFormat::Stl {
            let mut exported = self.export_stl(
                path,
                &chosen,
                &tolerance,
                options,
                placements,
                on_bed.as_mut(),
            )?;
            self.report_bed(&mut exported, on_bed);
            return Ok(exported);
        }
        if format == ExportFormat::Obj && (on_bed.is_some() || tolerance.max_edge > 0.0) {
            // The model's meshes (mitcad#734): split to the longest edge,
            // and placed on the print bed; the kernel writes them.
            let placed: Vec<Vec<BodyPlacement>> = chosen
                .iter()
                .map(|b| if design { placed_at(b.uid) } else { Vec::new() })
                .collect();
            let mut exported = self.export_obj_meshes(
                path,
                &chosen,
                &placed,
                &tolerance,
                options,
                color,
                on_bed.as_mut(),
            )?;
            self.report_bed(&mut exported, on_bed);
            return Ok(exported);
        }
        if format == ExportFormat::Usdz {
            // The placements with their occurrence paths (mitcad#123).
            let inside: Vec<InstanceView> =
                instances.iter().filter(|i| inside(i)).cloned().collect();
            return self.export_usdz(
                path,
                &chosen,
                bodies.is_empty(),
                &tolerance,
                color,
                design.then_some(inside.as_slice()),
                within.map_or(0, <[OccurrenceUid]>::len),
                options.design_folder.as_deref(),
            );
        }
        // The kernel's formats (mitcad#19): each body with its placements in
        // the design's coordinates, without any in the components'.
        let placed: Vec<Vec<BodyPlacement>> = chosen
            .iter()
            .map(|b| if design { placed_at(b.uid) } else { Vec::new() })
            .collect();
        let mut out = Vec::with_capacity(chosen.len());
        for (body, placements) in chosen.iter().zip(&placed) {
            let kind = self.kernel().body_kind(body.shape).map_err(kernel_error)?;
            if kind == BodyKind::Mesh && !options.format.takes_meshes() {
                return Err(invalid(format!(
                    "{} is a mesh body, which cannot be written to {} (only to STL, OBJ, BRep or 3MF)",
                    body.name,
                    options.format.as_str().to_uppercase()
                )));
            }
            if kind == BodyKind::Empty {
                return Err(invalid(format!("{} is empty", body.name)));
            }
            out.push(ExportBody {
                name: &body.name,
                color: color(body.uid),
                shape: body.shape,
                placements,
            });
        }
        self.kernel()
            .write_file(path, &out, options)
            .map_err(kernel_error)?;
        Ok(Exported {
            bodies: chosen.iter().map(|b| b.uid).collect(),
            ..Exported::default()
        })
    }

    /// A body's triangle mesh for a mesh file: the kernel's within the
    /// tolerance, its edges split to `max_edge` (mitcad#734). An empty
    /// body, or one without triangles, is an error.
    fn export_mesh(
        &self,
        body: &BodyView<'_, K::Shape>,
        tolerance: &MeshTolerance,
    ) -> Result<TriangleMesh, ModelError> {
        let kind = self.kernel().body_kind(body.shape).map_err(kernel_error)?;
        if kind == BodyKind::Empty {
            return Err(invalid(format!("{} is empty", body.name)));
        }
        let mesh = self
            .kernel()
            .triangle_mesh(body.shape, tolerance)
            .map_err(|e| invalid(format!("{}: {e}", body.name)))?;
        if mesh.triangles.is_empty() {
            return Err(invalid(format!("{} has no triangles", body.name)));
        }
        edges::split_long_edges(mesh, tolerance.max_edge, edges::MOST_TRIANGLES).map_err(|e| {
            invalid(format!(
                "{}: the maximum edge length {} mm would make more than {} triangles",
                body.name, tolerance.max_edge, e.most
            ))
        })
    }

    /// An STL file of the bodies (mitcad#17): their triangle meshes as one
    /// solid, each body once per placement (in the design's coordinates,
    /// or once in its component's); turned onto the print bed face, the
    /// lowest point on the bed and centred on the plate, with `bed`'s turn
    /// (mitcad#731); text with `options.ascii`, in `options.unit`
    /// (mitcad#734).
    fn export_stl(
        &self,
        path: &str,
        chosen: &[&BodyView<'_, K::Shape>],
        tolerance: &MeshTolerance,
        options: &ExportOptions,
        placements: impl Fn(BodyUid) -> Vec<Transform>,
        on_bed: Option<&mut OnBed>,
    ) -> Result<Exported, ModelError> {
        let mut all = TriangleMesh::default();
        for body in chosen {
            let mesh = self.export_mesh(body, tolerance)?;
            for placement in placements(body.uid) {
                // The first vertex's index; every index must fit in a u32.
                let first = u32::try_from(all.vertices.len() + mesh.vertices.len())
                    .map(|_| all.vertices.len() as u32)
                    .map_err(|_| invalid("the bodies have too many vertices for one STL file"))?;
                all.vertices
                    .extend(mesh.vertices.iter().map(|v| placement.apply_point(*v)));
                // A mirroring placement would turn the triangles inside out.
                let mirrored = placement.determinant() < 0.0;
                all.triangles.extend(mesh.triangles.iter().map(|t| {
                    let [a, b, c] = t.map(|i| first + i);
                    if mirrored { [a, c, b] } else { [a, b, c] }
                }));
            }
        }
        // On the print bed (mitcad#731): a turn keeps the triangles facing
        // out; settled without one too (mitcad#1109).
        if let Some(on_bed) = on_bed
            && let Some(turn) = bed::turn_to_settle(on_bed.transform, on_bed.settle)
        {
            let moved = bed::settled(
                &turn,
                all.vertices.iter().copied(),
                on_bed.plate,
                on_bed.settle,
            );
            for vertex in &mut all.vertices {
                *vertex = moved.apply_point(*vertex);
            }
            on_bed.transform = Some(moved);
        }
        // In the file's unit (mitcad#734).
        let scale = 1.0 / options.unit.millimeters();
        if scale != 1.0 {
            for vertex in &mut all.vertices {
                *vertex = vertex.map(|v| v * scale);
            }
        }
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Mitcad");
        std::fs::write(path, stl::write(&all, name, options.ascii))
            .map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        Ok(Exported {
            bodies: chosen.iter().map(|b| b.uid).collect(),
            ..Exported::default()
        })
    }

    /// An OBJ file of the model's meshes (mitcad#734): each body's mesh
    /// split to the longest edge, made a mesh body
    /// ([`Kernel::triangle_body`]) and written by the kernel at its
    /// `placed` placements, which `on_bed`'s turn moves onto the print bed
    /// face, the lowest point on the bed and centred on the plate, as STL
    /// does (mitcad#731). The kernel scales to `options.unit`.
    #[allow(clippy::too_many_arguments)]
    fn export_obj_meshes(
        &self,
        path: &str,
        chosen: &[&BodyView<'_, K::Shape>],
        placed: &[Vec<BodyPlacement>],
        tolerance: &MeshTolerance,
        options: &ExportOptions,
        color: impl Fn(BodyUid) -> Option<[f64; 3]>,
        on_bed: Option<&mut OnBed>,
    ) -> Result<Exported, ModelError> {
        let mut meshes = Vec::with_capacity(chosen.len());
        for body in chosen {
            let mesh = self.export_mesh(body, tolerance)?;
            let shape = self
                .kernel()
                .triangle_body(&mesh.vertices, &mesh.triangles)
                .map_err(|e| invalid(format!("{}: {e}", body.name)))?;
            meshes.push((mesh, shape));
        }
        let mut placed = placed.to_vec();
        // On the print bed (mitcad#731): every placement moved alike, as
        // the bodies are one piece; settled without a turn too
        // (mitcad#1109).
        if let Some(on_bed) = on_bed
            && let Some(turn) = bed::turn_to_settle(on_bed.transform, on_bed.settle)
        {
            let mut points = Vec::new();
            for ((mesh, _), placements) in meshes.iter().zip(&placed) {
                if placements.is_empty() {
                    points.extend(mesh.vertices.iter().copied());
                }
                for placement in placements {
                    points.extend(
                        mesh.vertices
                            .iter()
                            .map(|v| placement.transform.apply_point(*v)),
                    );
                }
            }
            let moved = bed::settled(&turn, points.into_iter(), on_bed.plate, on_bed.settle);
            for placements in &mut placed {
                if placements.is_empty() {
                    placements.push(BodyPlacement {
                        transform: Transform::IDENTITY,
                        name: String::new(),
                    });
                }
                for placement in placements.iter_mut() {
                    placement.transform = moved.after(&placement.transform);
                }
            }
            on_bed.transform = Some(moved);
        }
        let out: Vec<ExportBody<'_, K::Shape>> = chosen
            .iter()
            .zip(&meshes)
            .zip(&placed)
            .map(|((body, (_, shape)), placements)| ExportBody {
                name: &body.name,
                color: color(body.uid),
                shape,
                placements,
            })
            .collect();
        self.kernel()
            .write_file(path, &out, options)
            .map_err(kernel_error)?;
        Ok(Exported {
            bodies: chosen.iter().map(|b| b.uid).collect(),
            ..Exported::default()
        })
    }

    /// A 3MF file of the bodies (mitcad#13): each body's triangle mesh a
    /// part, placed by `placements` (the visible occurrences of its
    /// component, all of them when none is visible), named after the body,
    /// in its colour. Sheet and empty bodies are an error when chosen, and
    /// left out when all bodies are written (`all`). With `print` (a
    /// slicer, and the occurrence paths of a body's placements, in
    /// `placements` order), each body's effective print settings go with
    /// it in the slicer's dialect (mitcad#640, `crate::print_files`); a
    /// body whose placements print otherwise is one part per group of
    /// placements that print alike, each with its group's settings
    /// (mitcad#1061). On the print bed, each
    /// of the `pieces`' placements turns with its piece's turn onto its
    /// print bed face, the lowest point on the bed and the middle over the
    /// plate's centre (mitcad#731), the pieces turned set side by side
    /// along X (mitcad#856); each piece is an object of the build when
    /// there are several. The meshes stay as the kernel made them. The
    /// model is in `unit`, its meshes and placements scaled to it
    /// (mitcad#734).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    fn export_3mf(
        &self,
        path: &str,
        chosen: &[&BodyView<'_, K::Shape>],
        all: bool,
        tolerance: &MeshTolerance,
        unit: LengthUnit,
        color: impl Fn(BodyUid) -> Option<[f64; 3]>,
        placements: impl Fn(BodyUid) -> Vec<Transform>,
        print: Option<(
            crate::print_keys::Dialect,
            &dyn Fn(BodyUid) -> Vec<Option<Vec<OccurrenceUid>>>,
        )>,
        pieces: Option<&mut Vec<bed::Piece>>,
    ) -> Result<Exported, ModelError> {
        let model_name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Mitcad")
            .to_owned();
        let dialect = print.as_ref().map(|(d, _)| *d);
        let built = self.build_3mf(
            model_name,
            chosen,
            all,
            tolerance,
            unit,
            color,
            placements,
            print,
            pieces,
            ThreeMfPlate::default(),
        )?;
        self.write_3mf(path, built, dialect, &[])
    }

    /// The model of a 3MF file of the bodies as [`Document::export_3mf`]
    /// writes it, without the slicer's settings: its parts and objects,
    /// each part's effective settings (with `print`) and what is written.
    /// `plate` gives the print plate whose settings the bodies take
    /// (mitcad#1050; none: each body its own plate's) and the move of the
    /// placements onto that plate's place in the file (before the unit's
    /// scale).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn build_3mf(
        &self,
        model_name: String,
        chosen: &[&BodyView<'_, K::Shape>],
        all: bool,
        tolerance: &MeshTolerance,
        unit: LengthUnit,
        color: impl Fn(BodyUid) -> Option<[f64; 3]>,
        placements: impl Fn(BodyUid) -> Vec<Transform>,
        print: Option<(
            crate::print_keys::Dialect,
            &dyn Fn(BodyUid) -> Vec<Option<Vec<OccurrenceUid>>>,
        )>,
        pieces: Option<&mut Vec<bed::Piece>>,
        plate: ThreeMfPlate<'_>,
    ) -> Result<Built3mf, ModelError> {
        let settings_of = |body: BodyUid, path: Option<Vec<OccurrenceUid>>| {
            let target = crate::document::PrintTarget::Body(body, path);
            match plate.plate {
                Some(on) => self.effective_print_on(&target, Some(on)),
                None => self.effective_print(&target),
            }
        };
        let mut exported = Exported::default();
        let mut parts = Vec::with_capacity(chosen.len());
        // With a slicer's settings, each part's effective ones (mitcad#640,
        // mitcad#1061), in `parts` order.
        let mut settings: Vec<crate::document::EffectivePrint> = Vec::new();
        // Where each body's placements went: per placement, its part and
        // its index in that part's placements (sheet and empty bodies have
        // none).
        let mut part_of: Vec<Vec<(usize, usize)>> = Vec::with_capacity(chosen.len());
        for body in chosen {
            part_of.push(Vec::new());
            let kind = self.kernel().body_kind(body.shape).map_err(kernel_error)?;
            if matches!(kind, BodyKind::Sheet | BodyKind::Empty) {
                let why = format!("a {} body", kind.as_str());
                if !all {
                    return Err(invalid(format!(
                        "{} is {why}; 3MF takes solids and mesh bodies",
                        body.name
                    )));
                }
                exported.skipped.push((body.uid, why));
                continue;
            }
            let mesh = self.export_mesh(body, tolerance)?;
            let placed: Vec<mitcad_3mf::Placement> = placements(body.uid)
                .iter()
                .map(|t| mitcad_3mf::Placement {
                    linear: t.linear,
                    translation: t.translation,
                })
                .collect();
            let mut mesh = mitcad_3mf::Mesh {
                vertices: mesh.vertices,
                triangles: mesh.triangles,
            };
            exported.bodies.push(body.uid);
            exported.meshes.push(MeshSummary {
                triangles: mesh.triangles.len(),
                volume: mesh.volume(),
            });
            // The placements grouped by the settings they print with
            // (mitcad#1061): a material or a value of an occurrence or of a
            // component up a placement's path makes it print otherwise than
            // another, so each group is a part of its own, with its own
            // filament slot and values; without a slicer's settings, all of
            // them are one part.
            let paths = print
                .as_ref()
                .map(|(_, paths_of)| paths_of(body.uid))
                .unwrap_or_default();
            let mut groups: Vec<(Option<crate::document::EffectivePrint>, Vec<usize>)> = Vec::new();
            for (k, _) in placed.iter().enumerate() {
                let effective = print
                    .as_ref()
                    .map(|_| settings_of(body.uid, paths.get(k).cloned().flatten()));
                let alike = groups
                    .iter_mut()
                    .find(|(group, _)| match (group, &effective) {
                        (Some(a), Some(b)) => same_print(a, b),
                        _ => true,
                    });
                match alike {
                    Some((_, members)) => members.push(k),
                    None => groups.push((effective, vec![k])),
                }
            }
            if groups.is_empty() {
                groups.push((
                    print
                        .as_ref()
                        .map(|_| settings_of(body.uid, paths.first().cloned().flatten())),
                    Vec::new(),
                ));
            }
            let split = groups.len() > 1;
            let count = groups.len();
            let mut here = vec![(0, 0); placed.len()];
            for (g, (effective, members)) in groups.into_iter().enumerate() {
                let name = if split {
                    // The group's occurrences, the first three named.
                    let names: Vec<String> = members
                        .iter()
                        .filter_map(|&k| paths.get(k).cloned().flatten())
                        .map(|path| self.assembly().path_name(&path))
                        .filter(|name| !name.is_empty())
                        .collect();
                    match names.len() {
                        0 => body.name.clone(),
                        1..=3 => format!("{} ({})", body.name, names.join(", ")),
                        n => format!(
                            "{} ({} and {} more)",
                            body.name,
                            names[..3].join(", "),
                            n - 3
                        ),
                    }
                } else {
                    body.name.clone()
                };
                for (i, &k) in members.iter().enumerate() {
                    here[k] = (parts.len(), i);
                }
                if let Some(effective) = effective {
                    settings.push(effective);
                }
                exported.parts.push((body.uid, name.clone()));
                parts.push(mitcad_3mf::Part {
                    name,
                    color: color(body.uid),
                    // The last group takes the mesh, the others a copy.
                    mesh: if g + 1 == count {
                        std::mem::take(&mut mesh)
                    } else {
                        mesh.clone()
                    },
                    placements: members.iter().map(|&k| placed[k]).collect(),
                    metadata: Vec::new(),
                });
            }
            if let Some(last) = part_of.last_mut() {
                *last = here;
            }
        }
        if parts.is_empty() {
            return Err(invalid("there are no solid or mesh bodies to export"));
        }
        // On the print bed (mitcad#731): each piece's placements moved
        // alike by its turn, as its bodies are one object; the pieces
        // turned side by side, and several pieces the build's objects
        // (mitcad#856).
        let mut objects = Vec::new();
        if let Some(pieces) = pieces {
            // Each piece's placements in the file: (part, placement).
            let placed: Vec<Vec<(usize, usize)>> = pieces
                .iter()
                .map(|piece| {
                    piece
                        .members
                        .iter()
                        .filter_map(|&(b, k)| part_of.get(b).and_then(|p| p.get(k)).copied())
                        .collect()
                })
                .collect();
            let points = |members: &[(usize, usize)]| -> Vec<[f64; 3]> {
                members
                    .iter()
                    .flat_map(|&(p, k)| {
                        let placement = parts[p].placements[k];
                        parts[p]
                            .mesh
                            .vertices
                            .iter()
                            .map(move |v| placement.apply(*v))
                    })
                    .collect()
            };
            let mut moved: Vec<Option<Transform>> = pieces
                .iter()
                .zip(&placed)
                .map(|(piece, members)| {
                    let settle = piece.plan.settle;
                    bed::turn_to_settle(piece.plan.transform, settle).map(|turn| {
                        bed::settled(&turn, points(members).into_iter(), piece.plan.plate, settle)
                    })
                })
                .collect();
            // Side by side along X, the row centred on the plate (only when
            // the pieces are centred, mitcad#1109); a row wider than the bed
            // wraps into rows along Y, the rows centred on the plate too
            // (mitcad#1316), and pieces that still do not fit are named in
            // a warning.
            let turned: Vec<usize> = moved
                .iter()
                .zip(&placed)
                .enumerate()
                .filter(|(_, (t, members))| t.is_some() && !members.is_empty())
                .map(|(i, _)| i)
                .collect();
            // The x range of a piece's placements moved by `t`.
            let x_span = |t: Transform, members: &[(usize, usize)]| -> (f64, f64) {
                points(members)
                    .into_iter()
                    .map(|p| t.apply_point(p)[0])
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                        (lo.min(x), hi.max(x))
                    })
            };
            let centred = pieces.iter().all(|piece| piece.plan.settle.center);
            if turned.len() > 1 && centred {
                // Each piece's box in X and Y as it lies now.
                let spans: Vec<[(f64, f64); 2]> = turned
                    .iter()
                    .map(|&i| {
                        let t = moved[i].unwrap_or(Transform::IDENTITY);
                        let empty = (f64::INFINITY, f64::NEG_INFINITY);
                        points(&placed[i])
                            .into_iter()
                            .map(|p| t.apply_point(p))
                            .fold([empty, empty], |[x, y], p| {
                                [
                                    (x.0.min(p[0]), x.1.max(p[0])),
                                    (y.0.min(p[1]), y.1.max(p[1])),
                                ]
                            })
                    })
                    .collect();
                let bed_size = self.plate_size();
                let rows = bed::wrap_rows(&spans, bed_size[0]);
                let centre = pieces[turned[0]].plan.plate;
                let width = |row: &[usize]| -> f64 {
                    row.iter()
                        .map(|&k| spans[k][0].1 - spans[k][0].0)
                        .sum::<f64>()
                        + bed::GAP * (row.len() - 1) as f64
                };
                let depth = |row: &[usize]| -> f64 {
                    row.iter()
                        .map(|&k| spans[k][1].1 - spans[k][1].0)
                        .fold(0.0, f64::max)
                };
                let total_depth = rows.iter().map(|row| depth(row)).sum::<f64>()
                    + bed::GAP * (rows.len() - 1) as f64;
                let mut row_start = centre[1] - total_depth / 2.0;
                for row in &rows {
                    let mut cursor = centre[0] - width(row) / 2.0;
                    let middle = row_start + depth(row) / 2.0;
                    for &k in row {
                        let ((lo, hi), (y_lo, y_hi)) = (spans[k][0], spans[k][1]);
                        // One row stays where the pieces lie in Y.
                        let dy = if rows.len() > 1 {
                            middle - (y_lo + y_hi) / 2.0
                        } else {
                            0.0
                        };
                        if let Some(t) = moved[turned[k]].as_mut() {
                            *t = Transform::translation([cursor - lo, dy, 0.0]).after(t);
                        }
                        cursor += hi - lo + bed::GAP;
                    }
                    row_start += depth(row) + bed::GAP;
                }
                let widest = rows.iter().map(|row| width(row)).fold(0.0, f64::max);
                if widest > bed_size[0] + bed::FIT || total_depth > bed_size[1] + bed::FIT {
                    exported.warnings.push(format!(
                        "the {} pieces do not fit on the {:.0} x {:.0} mm bed (they take {:.0} x \
                         {:.0} mm): arrange them in the slicer, or put some on another plate",
                        turned.len(),
                        bed_size[0],
                        bed_size[1],
                        widest,
                        total_depth
                    ));
                }
            }
            // Not centred, the further copies of a print plate's item
            // (mitcad#1311) each beside the one before it along X, from the
            // first copy's place: the copies would otherwise lie in one
            // place. The other pieces keep theirs.
            if !centred {
                for (i, piece) in pieces.iter().enumerate().skip(1) {
                    if !piece.copy || placed[i].is_empty() || placed[i - 1].is_empty() {
                        continue;
                    }
                    let before: Transform = moved[i - 1].unwrap_or(Transform::IDENTITY);
                    let here: Transform = moved[i].unwrap_or(Transform::IDENTITY);
                    let (_, end) = x_span(before, &placed[i - 1]);
                    let (start, _) = x_span(here, &placed[i]);
                    if end.is_finite() && start.is_finite() {
                        moved[i] = Some(
                            Transform::translation([end + bed::GAP - start, 0.0, 0.0]).after(&here),
                        );
                    }
                }
            }
            for ((piece, members), t) in pieces.iter_mut().zip(&placed).zip(&moved) {
                if let Some(t) = t {
                    let outer = mitcad_3mf::Placement {
                        linear: t.linear,
                        translation: t.translation,
                    };
                    for &(p, k) in members {
                        let placement = &mut parts[p].placements[k];
                        *placement = outer.after(placement);
                    }
                    piece.plan.transform = Some(*t);
                }
            }
            for (piece, members) in pieces.iter().zip(placed) {
                if !members.is_empty() {
                    objects.push(mitcad_3mf::BuildObject {
                        name: piece.name.clone().unwrap_or_else(|| model_name.clone()),
                        components: members,
                    });
                }
            }
            // One object is the model's own.
            if objects.len() < 2 {
                objects.clear();
            }
        }
        // On a print plate's place in a file of several plates
        // (mitcad#1050): every placement moved alike.
        if plate.shift != [0.0, 0.0] {
            for part in &mut parts {
                for placement in &mut part.placements {
                    placement.translation[0] += plate.shift[0];
                    placement.translation[1] += plate.shift[1];
                }
            }
        }
        // In the file's unit (mitcad#734): the meshes and the placements'
        // moves scaled, the turns as they are.
        let scale = 1.0 / unit.millimeters();
        if scale != 1.0 {
            for part in &mut parts {
                for vertex in &mut part.mesh.vertices {
                    *vertex = vertex.map(|v| v * scale);
                }
                for placement in &mut part.placements {
                    placement.translation = placement.translation.map(|v| v * scale);
                }
            }
        }
        let model = mitcad_3mf::Model {
            name: model_name,
            parts,
            objects,
            unit: three_mf(unit),
            application: format!("MitCad {}", env!("CARGO_PKG_VERSION")),
            ..mitcad_3mf::Model::default()
        };
        Ok(Built3mf {
            model,
            settings,
            exported,
        })
    }

    /// Writes a model [`Document::build_3mf`] made: with a slicer's
    /// `dialect`, each part's effective settings in its dialect
    /// (mitcad#640, mitcad#1061: of its group of placements), on the print
    /// `plates` the file holds (mitcad#1050; none: one plate of every
    /// object).
    pub(crate) fn write_3mf(
        &self,
        path: &str,
        built: Built3mf,
        dialect: Option<crate::print_keys::Dialect>,
        plates: &[crate::print_files::ExportPlate<'_>],
    ) -> Result<Exported, ModelError> {
        let Built3mf {
            mut model,
            settings,
            mut exported,
        } = built;
        // The slicer's settings (mitcad#640): each part's effective ones
        // (mitcad#1061: of its group of placements).
        if let Some(dialect) = dialect {
            let parts: Vec<crate::print_files::ExportPart<'_>> = model
                .parts
                .iter()
                .zip(&settings)
                .map(|(part, settings)| crate::print_files::ExportPart {
                    name: &part.name,
                    settings,
                    triangles: part.mesh.triangles.len(),
                    placements: part.placements.len(),
                })
                .collect();
            // The build's objects and their parts (mitcad#856).
            let objects: Vec<crate::print_files::ExportObject<'_>> = model
                .objects
                .iter()
                .map(|object| {
                    let mut used: Vec<usize> = object.components.iter().map(|c| c.0).collect();
                    used.sort_unstable();
                    used.dedup();
                    crate::print_files::ExportObject {
                        name: &object.name,
                        parts: used,
                    }
                })
                .collect();
            // With the purge volumes set by hand (mitcad#705), on the
            // plates (mitcad#1050).
            let written = crate::print_files::export_plates(
                dialect,
                &model.name,
                &parts,
                &objects,
                &self.state().purge_volumes,
                plates,
            );
            if let Some(application) = written.application {
                model.application = application;
            }
            model.metadata = written.metadata;
            model.namespaces = written.namespaces;
            model.files = written.files;
            for (part, metadata) in model.parts.iter_mut().zip(written.part_metadata) {
                part.metadata = metadata;
            }
            exported.slots = written.slots;
            exported.printed = settings
                .iter()
                .map(|s| s.material.clone())
                .zip(written.own_values)
                .collect();
            exported.warnings.extend(written.warnings);
        }
        let data = mitcad_3mf::write(&model).map_err(|e| invalid(e.to_string()))?;
        std::fs::write(path, data).map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        Ok(exported)
    }

    /// The colour a base feature gave a new body.
    fn imported_color(&self, body: BodyUid) -> Option<[f64; 3]> {
        match &self.feature(body.feature)?.def {
            FeatureDef::Base(base) if base.operation == Operation::NewBody => {
                base.body_numbered(body.index)?.color
            }
            _ => None,
        }
    }

    /// Writes the geometry of an evaluated sketch to a DXF file, in sketch
    /// coordinates and in `unit` (mitcad#1408: the design's unless the
    /// export names another). Returns the number of entities.
    pub fn export_sketch_dxf(
        &self,
        sketch: FeatureUid,
        path: &str,
        unit: crate::expr::LengthUnit,
        options: &mitcad_dxf::WriteOptions,
    ) -> Result<usize, ModelError> {
        let entry = self
            .feature(sketch)
            .ok_or(ModelError::NoSuchFeature(sketch))?;
        if !matches!(entry.def, FeatureDef::Sketch(_)) {
            return Err(invalid(format!(
                "{} ({sketch}) is not a sketch",
                entry.name
            )));
        }
        if !self.status(sketch).is_some_and(|s| s.is_ok()) {
            return Err(invalid(format!(
                "{} has no result to export (it failed, is suppressed or is after the marker)",
                entry.name
            )));
        }
        let FeatureDef::Sketch(def) = &entry.def else {
            unreachable!("checked above");
        };
        let output = self
            .sketch_output(sketch)
            .ok_or_else(|| invalid(format!("{} has no result to export", entry.name)))?;
        // Every curve of the sketch once, in its solved geometry;
        // construction geometry on its own layer; the lines that stand for
        // the sketch's axes (mitcad#500) are not drawn.
        let curves: Vec<(&str, String, SegmentGeometry)> = def
            .entities
            .iter()
            .filter(|e| e.axis.is_none())
            .filter_map(|e| {
                let curve = output.solved.curves.get(&e.id)?;
                let layer = if e.construction { "construction" } else { "0" };
                Some((layer, e.as_ref().to_string(), curve.whole()))
            })
            .collect();
        let drawing = sketch_drawing(curves.iter().map(|(l, n, g)| (*l, n.as_str(), g)))
            .map_err(|e| invalid(format!("{}: {e}", entry.name)))?;
        let drawing = dxf_in_unit(drawing, unit);
        mitcad_dxf::write_file(&drawing, path, options)
            .map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        Ok(drawing.entities.len())
    }
}

// --- The unit of a DXF export (mitcad#1408) ----------------------------------

/// The DXF drawing unit (`$INSUNITS`) of a length unit: every unit a
/// design can be in has one.
pub fn dxf_units(unit: crate::expr::LengthUnit) -> mitcad_dxf::Units {
    use crate::expr::LengthUnit;
    use mitcad_dxf::Units;
    match unit {
        LengthUnit::Micrometre => Units::Microns,
        LengthUnit::Millimetre => Units::Millimeters,
        LengthUnit::Centimetre => Units::Centimeters,
        LengthUnit::Metre => Units::Meters,
        LengthUnit::Kilometre => Units::Kilometers,
        LengthUnit::Inch => Units::Inches,
        LengthUnit::Foot => Units::Feet,
        LengthUnit::Yard => Units::Yards,
        LengthUnit::Mile => Units::Miles,
        LengthUnit::Mil => Units::Mils,
    }
}

/// A drawing made in millimetres, written in `unit` instead: every
/// coordinate, radius and text height divided by the unit's length in
/// millimetres, and the drawing's `$INSUNITS` (and so `$MEASUREMENT`) the
/// unit's. A millimetre drawing stays as it is.
pub fn dxf_in_unit(
    drawing: mitcad_dxf::Drawing,
    unit: crate::expr::LengthUnit,
) -> mitcad_dxf::Drawing {
    let millimetres = unit.millimetres();
    let mut drawing = if millimetres == 1.0 {
        drawing
    } else {
        let scale = 1.0 / millimetres;
        drawing.transformed(&mitcad_dxf::Affine::scale(scale, scale))
    };
    drawing.units = dxf_units(unit);
    drawing
}

/// The `units` of a DXF export: the unit it names (`mm`, `in`, … as
/// `set_units` reads them), or `default` (the design's) when it names none.
pub fn dxf_export_unit(
    units: Option<&str>,
    default: crate::expr::LengthUnit,
) -> Result<crate::expr::LengthUnit, String> {
    match units {
        None => Ok(default),
        Some(text) => crate::file::parse_length_unit(text.trim()),
    }
}

/// The DXF drawing of a sketch's solved curves: each curve (its layer,
/// its name for messages and its geometry) once, in sketch coordinates and
/// millimetres.
///
/// This is the one place that maps sketch geometry to DXF.
#[doc(hidden)]
pub fn sketch_drawing<'a>(
    curves: impl IntoIterator<Item = (&'a str, &'a str, &'a SegmentGeometry)>,
) -> Result<mitcad_dxf::Drawing, String> {
    use mitcad_dxf::{Drawing, Geometry, Point2, Spline, Units};

    let point = |p: [f64; 2]| Point2::new(p[0], p[1]);
    let mut drawing = Drawing::new(Units::Millimeters);
    {
        for (layer, name, segment_geometry) in curves {
            let geometry = match segment_geometry {
                SegmentGeometry::Line { start, end } => Geometry::Line {
                    start: point(*start),
                    end: point(*end),
                },
                SegmentGeometry::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                } => Geometry::arc(point(*center), *radius, *start_angle, *end_angle),
                SegmentGeometry::Circle { center, radius, .. } => Geometry::Circle {
                    center: point(*center),
                    radius: *radius,
                },
                SegmentGeometry::Ellipse {
                    center,
                    major_radius,
                    minor_radius,
                    rotation,
                } => ellipse(
                    point(*center),
                    *major_radius,
                    *minor_radius,
                    *rotation,
                    None,
                ),
                SegmentGeometry::EllipseArc {
                    center,
                    major_radius,
                    minor_radius,
                    rotation,
                    start_angle,
                    end_angle,
                } => ellipse(
                    point(*center),
                    *major_radius,
                    *minor_radius,
                    *rotation,
                    Some((*start_angle, *end_angle)),
                ),
                SegmentGeometry::BSpline {
                    degree,
                    poles,
                    weights,
                    knots,
                    multiplicities,
                    periodic,
                } => {
                    if *periodic {
                        return Err(format!(
                            "{name}: periodic splines cannot be written to DXF yet"
                        ));
                    }
                    Geometry::Spline(Spline {
                        degree: *degree,
                        control_points: poles.iter().map(|p| point(*p)).collect(),
                        knots: knots
                            .iter()
                            .zip(multiplicities)
                            .flat_map(|(k, m)| std::iter::repeat_n(*k, *m as usize))
                            .collect(),
                        weights: weights.clone(),
                        ..Spline::default()
                    })
                }
            };
            drawing.push(layer, geometry);
        }
    }
    Ok(drawing)
}

/// An ellipse, or the arc between two parameter angles counter-clockwise,
/// with the first axis `rotation` from the x axis. DXF needs the major axis
/// to be the longer one, so a longer second axis turns the frame a quarter
/// and the parameters back by as much.
fn ellipse(
    center: mitcad_dxf::Point2,
    first: f64,
    second: f64,
    rotation: f64,
    arc: Option<(f64, f64)>,
) -> mitcad_dxf::Geometry {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let turn = if second > first { FRAC_PI_2 } else { 0.0 };
    let (major, minor) = (first.max(second), first.min(second));
    let (start_param, end_param) = match arc {
        None => (0.0, TAU),
        Some((start, end)) => {
            let span = if end <= start {
                end + TAU - start
            } else {
                end - start
            };
            let start = (start - turn).rem_euclid(TAU);
            (start, start + span)
        }
    };
    mitcad_dxf::Geometry::Ellipse {
        center,
        major_axis: mitcad_dxf::Point2::polar(rotation + turn) * major,
        ratio: minor / major,
        start_param,
        end_param,
    }
}
