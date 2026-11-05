// SPDX-License-Identifier: MIT
//! The data of data exchange that the geometry kernel reads and writes:
//! bodies read from files, bodies to write with their placements, file
//! formats and their options, and triangle meshes (mitcad#777: below the
//! model, next to the [`Kernel`] trait).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(doc)]
use crate::kernel::Kernel;
use crate::transform::Transform;

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

/// What [`Kernel::read_file_structure`] read (mitcad#1274): a file's
/// assembly structure, or its bodies as [`Kernel::read_file`] reads them.
#[derive(Debug, Clone)]
pub enum FileRead<S> {
    Bodies(Vec<ImportBody<S>>),
    Assembly(ImportAssembly<S>),
}

/// The assembly structure of a file (mitcad#1274): each distinct part and
/// assembly once, and the instances that place them.
#[derive(Debug, Clone)]
pub struct ImportAssembly<S> {
    pub parts: Vec<ImportPart<S>>,
    /// The instances placed in the design.
    pub roots: Vec<ImportInstance>,
}

/// A part (bodies in its own coordinates) or an assembly (instances of
/// other parts and assemblies) of a file, with its product name.
#[derive(Debug, Clone)]
pub struct ImportPart<S> {
    pub name: Option<String>,
    pub bodies: Vec<ImportBody<S>>,
    pub instances: Vec<ImportInstance>,
}

/// An instance of [`ImportAssembly::parts`]`[part]`: a rigid placement in
/// its parent, and its name in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInstance {
    pub part: usize,
    pub name: Option<String>,
    pub transform: Transform,
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
    /// Universal Scene Description package for AR viewers and other
    /// programs (mitcad#123): the bodies as shaded meshes in the component
    /// hierarchy, with their appearances.
    Usdz,
}

impl ExportFormat {
    /// The format of a file name's extension (`.step`/`.stp`, `.iges`/`.igs`,
    /// `.stl`, `.obj`, `.brep`/`.brp`, `.3mf`, `.usdz`), ignoring case.
    pub fn of_path(path: &str) -> Option<Self> {
        let extension = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
        Some(match extension.as_str() {
            "step" | "stp" => Self::Step,
            "iges" | "igs" => Self::Iges,
            "stl" => Self::Stl,
            "obj" => Self::Obj,
            "brep" | "brp" => Self::Brep,
            "3mf" => Self::ThreeMf,
            "usdz" => Self::Usdz,
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
            Self::Usdz => "usdz",
        }
    }

    /// Formats that write triangles, which mesh bodies can go to.
    pub fn takes_meshes(self) -> bool {
        matches!(
            self,
            Self::Stl | Self::Obj | Self::Brep | Self::ThreeMf | Self::Usdz
        )
    }
}

/// A body as one triangle mesh ([`Kernel::triangle_mesh`]), millimetres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriangleMesh {
    pub vertices: Vec<[f64; 3]>,
    /// Indices into `vertices`, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
}

/// A body's faces as triangles for display ([`Kernel::face_meshes`],
/// mitcad#123), millimetres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FaceMeshes {
    pub vertices: Vec<[f64; 3]>,
    /// A unit normal per vertex, out of the body.
    pub normals: Vec<[f64; 3]>,
    /// Counter-clockwise seen from outside, face by face.
    pub triangles: Vec<[u32; 3]>,
    /// Each face's number of triangles, in the order [`Kernel::faces`]
    /// lists the faces.
    pub face_triangles: Vec<u32>,
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

/// Length unit of a STEP, IGES, STL, OBJ or 3MF file (the mesh formats
/// since mitcad#734).
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

impl LengthUnit {
    /// The length of one unit in millimetres.
    pub fn millimeters(self) -> f64 {
        match self {
            Self::Millimeter => 1.0,
            Self::Centimeter => 10.0,
            Self::Meter => 1000.0,
            Self::Inch => 25.4,
            Self::Foot => 304.8,
        }
    }
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

/// The presets bound each doubly curved face's triangles (mitcad#398): a
/// face that would get more at the preset's deviation, which only faces
/// larger than about 15 cm can, meshes with a larger deviation that
/// grows with its size, so that a dome of metres exports in about the
/// triangles of one of centimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefinementPreset {
    /// 0.1 mm surface deviation, 30 degrees between facet normals, about
    /// 5 000 triangles a face at most.
    Low,
    /// 0.03 mm, 15 degrees, 20 000 triangles.
    Medium,
    /// 0.01 mm, 8 degrees, 50 000 triangles.
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshTolerance {
    /// Largest distance between the mesh and the surface, millimetres.
    pub deviation: f64,
    /// Largest angle between neighbouring facet normals, radians.
    pub angle: f64,
    /// About how many triangles a doubly curved face gets at most; such a
    /// face that would get more meshes with a larger deviation (mitcad#398,
    /// `geometry/io` `MeshOptions::face_triangles`). 0 (the default of a
    /// tolerance given as numbers): `deviation` everywhere.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub face_triangles: u64,
    /// The longest edge a triangle of an exported mesh may have, mm
    /// (mitcad#734): STL, OBJ and 3MF split longer edges at their middles
    /// after the kernel's meshing (`edges.rs`), which keeps the mesh's shape
    /// and adds triangles only. 0 (the default, and the presets'): no limit.
    /// The kernel's meshing does not use it.
    #[serde(default, skip_serializing_if = "is_no_length")]
    pub max_edge: f64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn is_no_length(length: &f64) -> bool {
    *length == 0.0
}

impl Refinement {
    /// The tolerances: (surface deviation in mm, normal angle in radians,
    /// triangles a face at most).
    pub fn tolerance(self) -> MeshTolerance {
        let (deviation, degrees, face_triangles) = match self {
            Self::Custom(tolerance) => return tolerance,
            Self::Preset(RefinementPreset::Low) => (0.1, 30.0_f64, 5_000),
            Self::Preset(RefinementPreset::Medium) => (0.03, 15.0, 20_000),
            Self::Preset(RefinementPreset::High) => (0.01, 8.0, 50_000),
        };
        MeshTolerance {
            deviation,
            angle: degrees.to_radians(),
            face_triangles,
            max_edge: 0.0,
        }
    }
}

/// Options of [`Kernel::write_file`]; each format uses its own.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub step_schema: StepSchema,
    /// STEP, IGES, STL, OBJ and 3MF (the kernel's OBJ writer scales to it).
    pub unit: LengthUnit,
    /// STL, OBJ, 3MF and USDZ.
    pub refinement: Refinement,
    /// STL: text instead of binary.
    pub ascii: bool,
    /// USDZ: the design file's folder, which relative texture paths are
    /// relative to.
    pub design_folder: Option<PathBuf>,
    /// 3MF: the slicer whose settings go with the bodies (mitcad#640):
    /// each body's effective printer, process and filament in its dialect.
    pub dialect: Option<crate::print_keys::Dialect>,
}

impl ExportOptions {
    pub fn new(format: ExportFormat) -> Self {
        Self {
            format,
            step_schema: StepSchema::default(),
            unit: LengthUnit::default(),
            refinement: Refinement::default(),
            ascii: false,
            design_folder: None,
            dialect: None,
        }
    }
}
