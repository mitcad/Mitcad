// SPDX-License-Identifier: MIT
//! USDZ files (Universal Scene Description packages, mitcad#123): a design's
//! bodies as meshes with physically based materials for AR viewers and
//! other programs that take USD.
//!
//! A USDZ package is a zip archive whose files are stored uncompressed,
//! each starting at a multiple of 64 bytes, so that readers map them
//! directly; its first file is the scene's layer. [`write`] writes a
//! [`Scene`] as one binary layer (`.usdc`, [`usdc`]; the format AR viewers
//! require of a package's layer) and the textures' images:
//!
//! - the root prim (the layer's default prim) is an `Xform` named after
//!   the scene, turned a quarter about X (`xformOp:rotateX` -90) so that
//!   Mitcad's Z-up millimetres stand Y-up as the layer declares
//!   (`upAxis` "Y"; `metersPerUnit` 0.001: the points stay millimetres);
//! - each [`Node`] (a component's occurrence) is an `Xform` with its
//!   placement (`xformOp:transform`), its meshes and its child occurrences:
//!   the design's component hierarchy;
//! - each [`Mesh`] is a `Mesh` of triangles (`subdivisionScheme` none, so
//!   it is drawn as it is) with its normals, its extent, texture
//!   coordinates (`primvars:st`) when its material has a texture, a
//!   `material:binding` and a `GeomSubset` per group of triangles with a
//!   material of their own (`materialBind` family);
//! - each [`Material`] is a `Material` under `<root>/Materials` with a
//!   `UsdPreviewSurface` shader (base colour, metallic, roughness, opacity,
//!   IOR, clear coat, emission), and for a texture a `UsdUVTexture` reading
//!   the image from the package (sRGB, repeated) at the mesh's `st`.
//!
//! A scene with an [`Animation`] (mitcad#1597) moves rigid parts along
//! time: the layer declares the time codes of the first and last samples
//! (`startTimeCode`, `endTimeCode`) and the rate (`timeCodesPerSecond`
//! and `framesPerSecond`, one time code per frame), and each node or mesh
//! with `samples` has its `xformOp:transform` sampled at each time code
//! (`timeSamples`; the static placement stays its `default`, for readers
//! without time). A moved mesh gets its own `xformOp:transform` in its
//! node's coordinates. The meshes and materials are written once: only
//! the placements change. Readers interpolate between samples linearly,
//! matrix element by matrix element (USD's linear interpolation), which is
//! exact at the samples.
//!
//! [`read::read_package`] reads a package back and checks what the formats
//! require of it (tests use it on what [`write`] wrote).

pub mod compress;
pub mod read;
pub mod sdf;
mod stage;
pub mod usdc;

use std::fmt;

pub use read::{Layer, Package, read_layer, read_package};
pub use stage::identifier;

/// The alignment of a package's files.
pub const ALIGNMENT: usize = 64;

/// An affine placement, `p' = linear * p + translation` (row-major).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub linear: [[f64; 3]; 3],
    pub translation: [f64; 3],
}

impl Placement {
    pub const IDENTITY: Self = Self {
        linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };

    /// USD's matrix of the placement: row vectors (`p' = p * M`), so the
    /// transpose of `linear` with the translation in the last row.
    pub fn to_usd(&self) -> [[f64; 4]; 4] {
        let m = &self.linear;
        let t = &self.translation;
        [
            [m[0][0], m[1][0], m[2][0], 0.0],
            [m[0][1], m[1][1], m[2][1], 0.0],
            [m[0][2], m[1][2], m[2][2], 0.0],
            [t[0], t[1], t[2], 1.0],
        ]
    }

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|r| {
            self.linear[r][0] * p[0]
                + self.linear[r][1] * p[1]
                + self.linear[r][2] * p[2]
                + self.translation[r]
        })
    }

    /// Every number finite.
    pub fn is_finite(&self) -> bool {
        self.linear.iter().flatten().all(|v| v.is_finite())
            && self.translation.iter().all(|v| v.is_finite())
    }
}

/// The time of an animated scene (mitcad#1597): the rate and the time
/// codes of the samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// Time codes (frames) per second; positive.
    pub fps: f64,
    /// The time codes of the samples, increasing: the first is the layer's
    /// start, the last its end.
    pub times: Vec<f64>,
}

/// Texture coordinates of a mesh.
#[derive(Debug, Clone, PartialEq)]
pub enum TexCoords {
    /// One per vertex.
    Vertex(Vec<[f32; 2]>),
    /// One per corner of each triangle, in the triangles' order.
    FaceVarying(Vec<[f32; 2]>),
}

/// Triangles of a mesh with a material of their own.
#[derive(Debug, Clone, PartialEq)]
pub struct Subset {
    pub name: String,
    /// Indices into the mesh's triangles.
    pub triangles: Vec<u32>,
    /// Index into [`Scene::materials`].
    pub material: usize,
}

/// A triangle mesh in its node's coordinates (millimetres).
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub name: String,
    pub points: Vec<[f64; 3]>,
    /// A unit normal per point, pointing out of the body.
    pub normals: Vec<[f64; 3]>,
    /// Counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    pub tex_coords: Option<TexCoords>,
    /// Drawn from both sides (a surface body).
    pub double_sided: bool,
    /// Index into [`Scene::materials`].
    pub material: usize,
    pub subsets: Vec<Subset>,
    /// With the scene's [`Animation`] (mitcad#1597): the mesh's placement
    /// in its node at each time code; none when it does not move in its
    /// node.
    pub samples: Vec<Placement>,
}

/// A physically based material (`UsdPreviewSurface`); colours linear.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub name: String,
    pub diffuse_color: [f64; 3],
    pub metallic: f64,
    pub roughness: f64,
    pub opacity: f64,
    pub ior: f64,
    pub clearcoat: f64,
    pub clearcoat_roughness: f64,
    pub emissive_color: [f64; 3],
    /// Index into [`Scene::images`]: the diffuse colour's texture.
    pub texture: Option<usize>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            diffuse_color: [0.18; 3],
            metallic: 0.0,
            roughness: 0.5,
            opacity: 1.0,
            ior: 1.5,
            clearcoat: 0.0,
            clearcoat_roughness: 0.01,
            emissive_color: [0.0; 3],
            texture: None,
        }
    }
}

/// An image file of the package (PNG or JPEG).
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Its name in the package, `textures/wood.png`.
    pub path: String,
    pub data: Vec<u8>,
}

/// An `Xform`: a placement in its parent's coordinates, meshes and child
/// nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub placement: Placement,
    pub meshes: Vec<Mesh>,
    pub children: Vec<Node>,
    /// With the scene's [`Animation`] (mitcad#1597): the placement at each
    /// time code; none when the node stays at `placement`.
    pub samples: Vec<Placement>,
}

impl Node {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            placement: Placement::IDENTITY,
            meshes: Vec::new(),
            children: Vec::new(),
            samples: Vec::new(),
        }
    }
}

/// What a USDZ file holds: the root node (its placement is not written:
/// the root only turns Z up into Y up), the materials and their images.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub root: Node,
    pub materials: Vec<Material>,
    pub images: Vec<Image>,
    /// The layer's `doc`: the writing application.
    pub documentation: String,
    /// Time-sampled placements (mitcad#1597); none for a still scene.
    pub animation: Option<Animation>,
}

/// A USDZ file could not be written or read, or breaks the format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The scene's layer as specs (see the module documentation).
pub fn specs(scene: &Scene) -> Result<Vec<sdf::Spec>, Error> {
    stage::specs(scene)
}

/// The USDZ package of a scene: the layer `<root>.usdc` first, then the
/// images, every file stored and 64-byte aligned.
pub fn write(scene: &Scene) -> Result<Vec<u8>, Error> {
    let specs = specs(scene)?;
    let layer = usdc::write(&specs);
    let mut zip = mitcad_zip::Writer::new();
    let failed = |e: mitcad_zip::ZipError| Error(e.to_string());
    let name = format!("{}.usdc", stage::identifier(&scene.root.name));
    zip.add_aligned(&name, &layer, ALIGNMENT).map_err(failed)?;
    for image in &scene.images {
        zip.add_aligned(&image.path, &image.data, ALIGNMENT)
            .map_err(failed)?;
    }
    zip.finish().map_err(failed)
}

#[cfg(test)]
mod tests;
