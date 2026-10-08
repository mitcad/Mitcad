// SPDX-License-Identifier: MIT
//! Data exchange: base features from imported files and from bodies an
//! importer built, export of bodies (STEP, IGES, STL, OBJ, BRep, 3MF) and
//! of a sketch's geometry (DXF).
//!
//! File formats are the kernel's business ([`Kernel::read_file`],
//! [`Kernel::write_file`], which takes each body's placements, mitcad#19);
//! DXF is written by `mitcad-dxf` from the evaluated sketch, and 3MF by
//! `mitcad-3mf` and STL by the model (`stl`) from the kernel's triangle
//! meshes ([`Kernel::triangle_mesh`], mitcad#13, mitcad#17). Every format
//! has the bodies where the design shows them unless the components'
//! coordinates are asked for.

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

/// What a body is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// One or more closed solids.
    Solid,
    /// Faces or open shells with surfaces (a surface body).
    Sheet,
    /// Triangles without surfaces (from STL or OBJ).
    Mesh,
    Empty,
}

impl BodyKind {
    /// `solid`, `sheet`, `mesh` or `empty`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Sheet => "sheet",
            Self::Mesh => "mesh",
            Self::Empty => "empty",
        }
    }
}

/// A body read from a file or built by an importer.
#[derive(Debug, Clone)]
pub struct ImportBody<S> {
    pub name: Option<String>,
    /// sRGB components in [0, 1].
    pub color: Option<[f64; 3]>,
    pub shape: S,
}

/// Options of [`Kernel::read_file`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadOptions {
    /// The length of one file unit in millimetres for formats without
    /// units (STL, OBJ).
    pub unit_mm: f64,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self { unit_mm: 1.0 }
    }
}

/// A body to write, with its display name and colour, and where it goes
/// (mitcad#19): once per placement, or once as it is without placements.
#[derive(Debug, Clone, Copy)]
pub struct ExportBody<'a, S> {
    pub name: &'a str,
    pub color: Option<[f64; 3]>,
    pub shape: &'a S,
    pub placements: &'a [BodyPlacement],
}

/// Where [`Kernel::write_file`] puts a body: a rotation and a translation
/// from its component's coordinates to the design's (an occurrence's
/// placement), and the occurrences' path that places it (`Arm:1/Pin:2`;
/// empty for the root's bodies). STEP writes a body's placements as the
/// instances of one part in an assembly, the other formats as moved
/// copies.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyPlacement {
    pub transform: Transform,
    pub name: String,
}

/// File formats for bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Step,
    Iges,
    Stl,
    Obj,
    /// OCCT's native B-rep text format.
    Brep,
    /// 3D Manufacturing Format for slicers (mitcad#13): one object with a
    /// part per body.
    #[serde(rename = "3mf")]
    ThreeMf,
}

impl ExportFormat {
    /// The format of a file name's extension (`.step`/`.stp`, `.iges`/`.igs`,
    /// `.stl`, `.obj`, `.brep`/`.brp`, `.3mf`), ignoring case.
    pub fn of_path(path: &str) -> Option<Self> {
        let extension = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
        Some(match extension.as_str() {
            "step" | "stp" => Self::Step,
            "iges" | "igs" => Self::Iges,
            "stl" => Self::Stl,
            "obj" => Self::Obj,
            "brep" | "brp" => Self::Brep,
            "3mf" => Self::ThreeMf,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::Iges => "iges",
            Self::Stl => "stl",
            Self::Obj => "obj",
            Self::Brep => "brep",
            Self::ThreeMf => "3mf",
        }
    }

    /// Formats that write triangles, which mesh bodies can go to.
    pub fn takes_meshes(self) -> bool {
        matches!(self, Self::Stl | Self::Obj | Self::Brep | Self::ThreeMf)
    }
}

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
}

/// A body as one triangle mesh ([`Kernel::triangle_mesh`]), millimetres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriangleMesh {
    pub vertices: Vec<[f64; 3]>,
    /// Indices into `vertices`, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
}

/// What [`Document::export_bodies`] wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exported {
    /// The bodies written, in order.
    pub bodies: Vec<BodyUid>,
    /// Bodies left out and why (3MF of all bodies: sheet and empty bodies).
    pub skipped: Vec<(BodyUid, String)>,
    /// 3MF: the mesh of each written body, in `bodies` order.
    pub meshes: Vec<MeshSummary>,
}

/// A body's mesh in a 3MF file: its triangles and the volume they enclose
/// (mm³), which differs from the body's by the refinement's deviation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshSummary {
    pub triangles: usize,
    pub volume: f64,
}

/// STEP application protocol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepSchema {
    /// AP214 (automotive design), read by nearly every program.
    #[default]
    Ap214,
    /// AP242 (managed model based 3D engineering).
    Ap242,
}

/// Length unit of a STEP or IGES file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    #[default]
    #[serde(rename = "mm")]
    Millimeter,
    #[serde(rename = "cm")]
    Centimeter,
    #[serde(rename = "m")]
    Meter,
    #[serde(rename = "in")]
    Inch,
    #[serde(rename = "ft")]
    Foot,
}

/// Mesh refinement of STL and OBJ export: a preset or the
/// tolerances themselves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Refinement {
    Preset(RefinementPreset),
    Custom(MeshTolerance),
}

impl Default for Refinement {
    fn default() -> Self {
        Self::Preset(RefinementPreset::Medium)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefinementPreset {
    /// 0.1 mm surface deviation, 30 degrees between facet normals.
    Low,
    /// 0.03 mm, 15 degrees.
    Medium,
    /// 0.01 mm, 8 degrees.
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshTolerance {
    /// Largest distance between the mesh and the surface, millimetres.
    pub deviation: f64,
    /// Largest angle between neighbouring facet normals, radians.
    pub angle: f64,
}

impl Refinement {
    /// The tolerances: (surface deviation in mm, normal angle in radians).
    pub fn tolerance(self) -> MeshTolerance {
        let (deviation, degrees) = match self {
            Self::Custom(tolerance) => return tolerance,
            Self::Preset(RefinementPreset::Low) => (0.1, 30.0_f64),
            Self::Preset(RefinementPreset::Medium) => (0.03, 15.0),
            Self::Preset(RefinementPreset::High) => (0.01, 8.0),
        };
        MeshTolerance {
            deviation,
            angle: degrees.to_radians(),
        }
    }
}

/// Options of [`Kernel::write_file`]; each format uses its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub step_schema: StepSchema,
    /// STEP and IGES.
    pub unit: LengthUnit,
    /// STL and OBJ.
    pub refinement: Refinement,
    /// STL: text instead of binary.
    pub ascii: bool,
}

impl ExportOptions {
    pub fn new(format: ExportFormat) -> Self {
        Self {
            format,
            step_schema: StepSchema::default(),
            unit: LengthUnit::default(),
            refinement: Refinement::default(),
            ascii: false,
        }
    }
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
}

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

fn kernel_error(e: crate::kernel::KernelError) -> ModelError {
    invalid(e.to_string())
}

/// The file name without directories, for the `source` of a base feature.
fn file_name(path: &str) -> String {
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
        for base in &features {
            let component = base.component.unwrap_or(self.active_component());
            self.state().editable(component)?;
            let bodies = self.base_bodies(&base.bodies)?;
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
        let added = self.apply(|state| {
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
            result.push(Imported { feature, bodies });
        }
        Ok(result)
    }

    /// Imports a STEP, IGES, BRep, STL or OBJ file as a base feature: one
    /// body per part or solid (STEP, IGES, BRep) or object (OBJ), a mesh
    /// body for STL.
    pub fn import_file(
        &mut self,
        path: &str,
        options: &ReadOptions,
        name: Option<&str>,
    ) -> Result<Imported, ModelError> {
        if !(options.unit_mm > 0.0 && options.unit_mm.is_finite()) {
            return Err(invalid(format!(
                "the file unit must be greater than zero, got {} mm",
                options.unit_mm
            )));
        }
        let bodies = self
            .kernel()
            .read_file(path, options)
            .map_err(kernel_error)?;
        if bodies.is_empty() {
            return Err(invalid(format!("{path} has no bodies")));
        }
        let source = file_name(path);
        let base = BaseInput {
            name: name.map(str::to_owned),
            source: Some(source.clone()),
            ..BaseInput::new(bodies)
        };
        let mut imported = self.add_base_features(&format!("Import {source}"), vec![base])?;
        Ok(imported.remove(0))
    }

    /// Writes bodies at the marker to a file; all bodies when `bodies` is
    /// empty. Bodies keep their display names, and `colors` or else the
    /// colours of imported bodies. Mesh bodies go only to STL, OBJ, BRep and
    /// 3MF files. Every format has the bodies where the design shows them
    /// (mitcad#17, mitcad#19), unless `placement` asks for the components'
    /// coordinates; with an occurrence, only its placements (all bodies:
    /// those it places). A 3MF file holds the solids and meshes (of all
    /// bodies, sheet and empty bodies are left out) as the parts of one
    /// object, each placed where its component's occurrences place it; a
    /// STEP file of moved bodies is an assembly of their parts.
    pub fn export_bodies(
        &self,
        path: &str,
        bodies: &[BodyUid],
        options: &ExportOptions,
        colors: &BTreeMap<BodyUid, [f64; 3]>,
        placement: &ExportPlacement,
    ) -> Result<Exported, ModelError> {
        let format = options.format;
        let design = placement.coordinates != Some(Coordinates::Component);
        let within = placement.within.as_deref();
        if within.is_some() && !design {
            return Err(invalid(
                "the placements of an occurrence are in the design's coordinates",
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
        let tolerance = options.refinement.tolerance();
        if !(tolerance.deviation > 0.0 && tolerance.angle > 0.0) {
            return Err(invalid(
                "the mesh deviation and angle must be greater than zero",
            ));
        }
        let color = |uid: BodyUid| {
            colors
                .get(&uid)
                .copied()
                .or_else(|| self.imported_color(uid))
        };
        // A body's placements: by the shown occurrences of its component,
        // by all of them when none is shown; once in component coordinates.
        let placed_at = |body: BodyUid| {
            let mine: Vec<&InstanceView> = instances
                .iter()
                .filter(|i| i.body == body && inside(i))
                .collect();
            let shown: Vec<&InstanceView> = mine.iter().copied().filter(|i| i.visible).collect();
            let mut out: Vec<BodyPlacement> = if shown.is_empty() { mine } else { shown }
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
        if format == ExportFormat::ThreeMf {
            return self.export_3mf(
                path,
                &chosen,
                bodies.is_empty(),
                &tolerance,
                color,
                placements,
            );
        }
        if format == ExportFormat::Stl {
            return self.export_stl(path, &chosen, &tolerance, options.ascii, placements);
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

    /// An STL file of the bodies (mitcad#17): their triangle meshes as one
    /// solid, each body once per placement (in the design's coordinates,
    /// or once in its component's).
    fn export_stl(
        &self,
        path: &str,
        chosen: &[&BodyView<'_, K::Shape>],
        tolerance: &MeshTolerance,
        ascii: bool,
        placements: impl Fn(BodyUid) -> Vec<Transform>,
    ) -> Result<Exported, ModelError> {
        let mut all = TriangleMesh::default();
        for body in chosen {
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
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Mitcad");
        std::fs::write(path, stl::write(&all, name, ascii))
            .map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        Ok(Exported {
            bodies: chosen.iter().map(|b| b.uid).collect(),
            ..Exported::default()
        })
    }

    /// A 3MF file of the bodies (mitcad#13): each body's triangle mesh a
    /// part, placed by `placements` (the visible occurrences of its
    /// component, all of them when none is visible), named after the body,
    /// in its colour. Sheet and empty bodies are an error when chosen, and
    /// left out when all bodies are written (`all`).
    fn export_3mf(
        &self,
        path: &str,
        chosen: &[&BodyView<'_, K::Shape>],
        all: bool,
        tolerance: &MeshTolerance,
        color: impl Fn(BodyUid) -> Option<[f64; 3]>,
        placements: impl Fn(BodyUid) -> Vec<Transform>,
    ) -> Result<Exported, ModelError> {
        let mut exported = Exported::default();
        let mut parts = Vec::with_capacity(chosen.len());
        for body in chosen {
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
            let mesh = self
                .kernel()
                .triangle_mesh(body.shape, tolerance)
                .map_err(|e| invalid(format!("{}: {e}", body.name)))?;
            if mesh.triangles.is_empty() {
                return Err(invalid(format!("{} has no triangles", body.name)));
            }
            let placements = placements(body.uid)
                .iter()
                .map(|t| mitcad_3mf::Placement {
                    linear: t.linear,
                    translation: t.translation,
                })
                .collect();
            let mesh = mitcad_3mf::Mesh {
                vertices: mesh.vertices,
                triangles: mesh.triangles,
            };
            exported.bodies.push(body.uid);
            exported.meshes.push(MeshSummary {
                triangles: mesh.triangles.len(),
                volume: mesh.volume(),
            });
            parts.push(mitcad_3mf::Part {
                name: body.name.clone(),
                color: color(body.uid),
                mesh,
                placements,
            });
        }
        if parts.is_empty() {
            return Err(invalid("there are no solid or mesh bodies to export"));
        }
        let model = mitcad_3mf::Model {
            name: Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Mitcad")
                .to_owned(),
            parts,
            application: format!("Mitcad {}", env!("CARGO_PKG_VERSION")),
        };
        let data = mitcad_3mf::write(&model).map_err(|e| invalid(e.to_string()))?;
        std::fs::write(path, data).map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        Ok(exported)
    }

    /// The colour a base feature gave a new body.
    fn imported_color(&self, body: BodyUid) -> Option<[f64; 3]> {
        match &self.feature(body.feature)?.def {
            FeatureDef::Base(base) if base.operation == Operation::NewBody => {
                base.bodies.get(body.index as usize)?.color
            }
            _ => None,
        }
    }

    /// Writes the geometry of an evaluated sketch to a DXF file, in sketch
    /// coordinates and millimetres. Returns the number of entities.
    pub fn export_sketch_dxf(
        &self,
        sketch: FeatureUid,
        path: &str,
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
        // construction geometry on its own layer.
        let curves: Vec<(&str, String, SegmentGeometry)> = def
            .entities
            .iter()
            .filter_map(|e| {
                let curve = output.solved.curves.get(&e.id)?;
                let layer = if e.construction { "construction" } else { "0" };
                Some((layer, e.as_ref().to_string(), curve.whole()))
            })
            .collect();
        let drawing = sketch_drawing(curves.iter().map(|(l, n, g)| (*l, n.as_str(), g)))
            .map_err(|e| invalid(format!("{}: {e}", entry.name)))?;
        mitcad_dxf::write_file(&drawing, path, options)
            .map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        Ok(drawing.entities.len())
    }
}

/// The DXF drawing of a sketch's solved curves: each curve (its layer,
/// its name for messages and its geometry) once, in sketch coordinates and
/// millimetres.
///
/// This is the one place that maps sketch geometry to DXF.
pub(crate) fn sketch_drawing<'a>(
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
                SegmentGeometry::Circle { center, radius } => Geometry::Circle {
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

#[cfg(test)]
#[path = "exchange_tests.rs"]
mod tests;
