// SPDX-License-Identifier: MIT
//! glTF 2.0 files (`.glb`, `.gltf`) as mesh bodies and components
//! (mitcad#1826), read by `mitcad-gltf`:
//!
//! - A node with children is a component named after the node, placed by
//!   an occurrence with the node's placement; its mesh and its children go
//!   into it. Nodes whose subtrees are the same (the same names, meshes
//!   and placements, as a writer that repeats a component writes them) are
//!   one component with an occurrence each.
//! - A node with a mesh and no children is a mesh body in the component
//!   that holds it, its placement moved into the triangles; a mesh of
//!   several materials is a body per material.
//! - A node whose placement scales or mirrors is no component: its meshes
//!   and its children's come in moved into place in the component that
//!   holds it.
//! - A scene of one root node that holds the rest (and no mesh) is the
//!   design itself: its children come in straight into the component the
//!   file is imported into, its placement (such as a writer's turn of the
//!   axes) moved into theirs.
//! - Lengths are metres (×1000 to millimetres) and Y is up (turned to Z
//!   up, `mitcad_gltf::Y_UP_TO_Z_UP`).
//! - Each material a body uses becomes one document appearance: its base
//!   colour (sRGB), metalness, roughness, opacity of a blended one,
//!   emission, transmission, index of refraction, and its base colour,
//!   normal, occlusion and metallic-roughness images embedded as a texture
//!   (the last as the roughness map of its green channel and the
//!   metalness map of its blue one, the occlusion its red one;
//!   mitcad#1862), the roughness and metalness factors that scale those
//!   channels with their maps where they are not 1 (`TextureMap::factor`,
//!   mitcad#1874).
//! - A textured body keeps the file's texture coordinates of its base
//!   colour image (mitcad#1862, `Kernel::textured_triangle_body`), its
//!   corners welded only where their points and coordinates are the same,
//!   and its appearance projects the texture by them
//!   (`appearance::Projection::Uv`); one without them is projected as
//!   MitCad projects textures, and a warning says so.
//! - What the file has that is not taken (animations, skins, morph
//!   targets, points and lines, compressed meshes, other extensions,
//!   texture coordinates that are missing or cannot be read) is listed in
//!   the import's warnings in words.
//!
//! One undo step; nothing stays of a failed import.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use mitcad_gltf::{Gltf, IDENTITY, MM_PER_UNIT, Matrix, Y_UP_TO_Z_UP, multiply};
use serde_json::Value;

use super::{
    BaseInput, ImportBody, ImportWrapper, Imported, ImportedComponent, ImportedFile, file_name,
    invalid,
};
use crate::appearance::{
    self, AppearanceChange, Channel, ImageData, Projection, Texture, TextureMap,
};
use crate::document::{Document, ModelError};
use crate::ids::{BodyUid, ComponentUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// Whether the file at `path` is a glTF file by its extension (`.glb`,
/// `.gltf`, any case).
pub fn is_gltf_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("glb") || e.eq_ignore_ascii_case("gltf"))
}

/// A glTF file read, or the error in words (its detail to the log).
fn read(path: &str) -> Result<Gltf, ModelError> {
    mitcad_gltf::read_file(Path::new(path)).map_err(|e| {
        eprintln!("import: glTF: {path}: {}", e.detail);
        invalid(format!("{}: {}", file_name(path), e.words))
    })
}

/// The `gltf_file` query: the file as the reader reads it, each node with
/// its name, its path of names, its placements in MitCad's millimetres and
/// axes, its triangles and its materials' colours (sRGB), and what is not
/// read in words (`mitcad_gltf::Gltf::summary`). Changes nothing.
pub fn gltf_file_json(path: &str) -> Result<Value, ModelError> {
    Ok(read(path)?.summary())
}

/// The largest difference allowed between a placement's 3 x 3 part and the
/// rotation nearest to it for the placement to be a rotation and a
/// translation (the file keeps single precision numbers).
const RIGID_TOLERANCE: f64 = 1e-6;

/// The rotation and translation of a placement (millimetres), the rotation
/// made exact; None when it scales, shears or mirrors.
fn rigid(m: &Matrix) -> Option<Transform> {
    let linear: [[f64; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| m[r][c]));
    let det = |a: &[[f64; 3]; 3]| {
        a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
            - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
            + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
    };
    let first = det(&linear);
    if first.is_nan() || first <= 0.0 || !m.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    // The nearest rotation (the polar decomposition's), by Newton's
    // iteration R <- (R + R^-T) / 2.
    let mut r = linear;
    for _ in 0..8 {
        let d = det(&r);
        if d.abs() < 1e-12 {
            return None;
        }
        // R^-T is the cofactor matrix over the determinant.
        let cofactor: [[f64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                let (i1, i2) = ((i + 1) % 3, (i + 2) % 3);
                let (j1, j2) = ((j + 1) % 3, (j + 2) % 3);
                r[i1][j1] * r[i2][j2] - r[i1][j2] * r[i2][j1]
            })
        });
        r = std::array::from_fn(|i| std::array::from_fn(|j| (r[i][j] + cofactor[i][j] / d) / 2.0));
    }
    let off = (0..3)
        .flat_map(|i| (0..3).map(move |j| (i, j)))
        .map(|(i, j)| (linear[i][j] - r[i][j]).abs())
        .fold(0.0, f64::max);
    if off > RIGID_TOLERANCE {
        return None;
    }
    let t = Transform {
        linear: r,
        translation: [m[0][3], m[1][3], m[2][3]],
    };
    t.is_rigid().then_some(t)
}

/// A glTF placement in millimetres (its translation scaled).
fn in_mm(m: &Matrix) -> Matrix {
    let mut out = *m;
    for row in out.iter_mut().take(3) {
        row[3] *= MM_PER_UNIT;
    }
    out
}

/// A body planned: its name, its material and its triangles in its
/// component's coordinates (mm), with the texture coordinates of its
/// material's image when the file gives them (one per position).
struct PlannedBody {
    name: String,
    material: Option<usize>,
    positions: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
    uvs: Option<Vec<[f64; 2]>>,
}

/// Whether material `m` has a base colour image that was read: its
/// appearance then has a texture.
fn textured(gltf: &Gltf, m: usize) -> bool {
    gltf.materials[m]
        .base_color_image
        .and_then(|i| gltf.images.get(i))
        .is_some_and(|i| i.bytes.is_some())
}

/// What goes into a component (or into the one imported into): bodies and
/// components placed, and a key of all that, by which equal subtrees are
/// found.
#[derive(Default)]
struct Container {
    bodies: Vec<PlannedBody>,
    children: Vec<(usize, Transform)>,
    key: String,
}

impl Container {
    fn is_empty(&self) -> bool {
        self.bodies.is_empty() && self.children.is_empty()
    }
}

struct PlannedComponent {
    name: String,
    content: Container,
}

/// The file's nodes as components and bodies, before anything is made.
struct Plan<'a> {
    gltf: &'a Gltf,
    components: Vec<PlannedComponent>,
    by_key: HashMap<String, usize>,
    seen: Vec<bool>,
    /// Nodes that scale or mirror, whose meshes came in moved into place.
    baked: usize,
    deep: bool,
}

fn matrix_key(m: &Matrix) -> String {
    m.iter()
        .take(3)
        .flatten()
        .map(|v| format!("{v:.9e}"))
        .collect::<Vec<_>>()
        .join(",")
}

impl Plan<'_> {
    /// The node's mesh as bodies (one per material) moved by `placement`
    /// (millimetres) into `into`.
    fn bodies(&self, node: usize, placement: &Matrix, into: &mut Container) {
        let Some(mesh) = self.gltf.nodes[node].mesh else {
            return;
        };
        let primitives = &self.gltf.meshes[mesh].primitives;
        let mut materials: Vec<Option<usize>> = Vec::new();
        for p in primitives {
            if !materials.contains(&p.material) {
                materials.push(p.material);
            }
        }
        let name = self.gltf.node_name(node);
        let mirror = {
            let l = placement;
            l[0][0] * (l[1][1] * l[2][2] - l[1][2] * l[2][1])
                - l[0][1] * (l[1][0] * l[2][2] - l[1][2] * l[2][0])
                + l[0][2] * (l[1][0] * l[2][1] - l[1][1] * l[2][0])
                < 0.0
        };
        for (k, material) in materials.iter().enumerate() {
            let mut positions = Vec::new();
            let mut triangles = Vec::new();
            let wanted = *material;
            let mine = move || primitives.iter().filter(move |p| p.material == wanted);
            // A textured body keeps the texture coordinates when each of
            // its primitives has them (mitcad#1862).
            let keep_uvs =
                material.is_some_and(|m| textured(self.gltf, m)) && mine().all(|p| p.uvs.is_some());
            let mut uvs: Vec<[f64; 2]> = Vec::new();
            // Corners at the same point are one vertex: a writer splits
            // them where texture coordinates or normals differ. Kept
            // texture coordinates keep such corners apart where theirs
            // differ (a seam of the image).
            let mut welded: HashMap<[u64; 5], u32> = HashMap::new();
            for p in mine() {
                let corner: Vec<u32> = p
                    .positions
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        let q = q.map(|v| v * MM_PER_UNIT);
                        let point: [f64; 3] = std::array::from_fn(|r| {
                            placement[r][0] * q[0]
                                + placement[r][1] * q[1]
                                + placement[r][2] * q[2]
                                + placement[r][3]
                        });
                        let uv = if keep_uvs {
                            p.uvs.as_ref().and_then(|u| u.get(i)).copied()
                        } else {
                            None
                        };
                        let [x, y, z] = point.map(|v| (v + 0.0).to_bits());
                        let [u, v] = uv.map_or([0, 0], |uv| uv.map(|v| (v + 0.0).to_bits()));
                        *welded.entry([x, y, z, u, v]).or_insert_with(|| {
                            positions.push(point);
                            if keep_uvs {
                                uvs.push(uv.unwrap_or_default());
                            }
                            (positions.len() - 1) as u32
                        })
                    })
                    .collect();
                for t in &p.triangles {
                    let [a, b, c] = t.map(|i| corner[i as usize]);
                    if a != b && b != c && a != c {
                        triangles.push(if mirror { [a, c, b] } else { [a, b, c] });
                    }
                }
            }
            if triangles.is_empty() {
                continue;
            }
            let body_name = if materials.len() > 1 {
                let which = material
                    .and_then(|m| self.gltf.materials[m].name.clone())
                    .unwrap_or_else(|| (k + 1).to_string());
                format!("{name} ({which})")
            } else {
                name.clone()
            };
            into.key.push_str(&format!(
                "b[{body_name}|{mesh}|{material:?}|{}]",
                matrix_key(placement)
            ));
            into.bodies.push(PlannedBody {
                name: body_name,
                material: *material,
                positions,
                triangles,
                uvs: keep_uvs.then_some(uvs),
            });
        }
    }

    /// Node `node` placed by `local` (millimetres, in what holds it) into
    /// `into`: a component, its meshes, or (when the placement is no
    /// rotation and translation) its meshes moved into place.
    fn place(&mut self, node: usize, local: Matrix, into: &mut Container, depth: usize) {
        if node >= self.seen.len() || self.seen[node] {
            return;
        }
        if depth > mitcad_gltf::MAX_DEPTH {
            self.deep = true;
            return;
        }
        self.seen[node] = true;
        let children = self.gltf.nodes[node].children.clone();
        if children.is_empty() {
            self.bodies(node, &local, into);
            return;
        }
        let Some(placement) = rigid(&local) else {
            // Scaled or mirrored: no occurrence can place it.
            self.baked += 1;
            self.bodies(node, &local, into);
            for c in children {
                let child = multiply(&local, &in_mm(&self.gltf.nodes[c].matrix));
                self.place(c, child, into, depth + 1);
            }
            return;
        };
        let mut content = Container::default();
        self.bodies(node, &IDENTITY, &mut content);
        for c in children {
            let child = in_mm(&self.gltf.nodes[c].matrix);
            self.place(c, child, &mut content, depth + 1);
        }
        if content.is_empty() {
            return;
        }
        let name = self.gltf.node_name(node);
        let key = format!("{name}|{}", content.key);
        let index = match self.by_key.get(&key) {
            Some(known) => *known,
            None => {
                self.components.push(PlannedComponent { name, content });
                let index = self.components.len() - 1;
                self.by_key.insert(key, index);
                index
            }
        };
        into.key
            .push_str(&format!("c[{index}|{}]", matrix_key(&local)));
        into.children.push((index, placement));
    }
}

/// The plan of a file: what goes straight into the component imported
/// into, the components, how many nodes were moved into place (they scale
/// or mirror) and whether nodes were nested too deeply.
struct Planned {
    top: Container,
    components: Vec<PlannedComponent>,
    baked: usize,
    deep: bool,
}

/// The scene's nodes as components and bodies (see the module).
fn plan(gltf: &Gltf) -> Planned {
    let mut plan = Plan {
        gltf,
        components: Vec::new(),
        by_key: HashMap::new(),
        seen: vec![false; gltf.nodes.len()],
        baked: 0,
        deep: false,
    };
    let mut top = Container::default();
    let single = match gltf.roots.as_slice() {
        [root] => Some(*root).filter(|r| {
            let n = &gltf.nodes[*r];
            n.mesh.is_none() && !n.children.is_empty()
        }),
        _ => None,
    };
    if let Some(root) = single {
        // The design itself: its placement moved into its children's.
        plan.seen[root] = true;
        let above = multiply(&Y_UP_TO_Z_UP, &in_mm(&gltf.nodes[root].matrix));
        for c in gltf.nodes[root].children.clone() {
            let local = multiply(&above, &in_mm(&gltf.nodes[c].matrix));
            plan.place(c, local, &mut top, 1);
        }
    } else {
        for r in gltf.roots.clone() {
            let local = multiply(&Y_UP_TO_Z_UP, &in_mm(&gltf.nodes[r].matrix));
            plan.place(r, local, &mut top, 0);
        }
    }
    Planned {
        top,
        components: plan.components,
        baked: plan.baked,
        deep: plan.deep,
    }
}

/// Bodies made for a component (or the one imported into), each with its
/// material, and the components it places.
struct Content<S> {
    bodies: Vec<ImportBody<S>>,
    materials: Vec<Option<usize>>,
    children: Vec<(usize, Transform)>,
}

/// The plan's bodies made: those of the component imported into, and each
/// planned component's with its name; and the materials whose bodies kept
/// their texture coordinates, whose appearances project by them.
struct Parts<S> {
    top: Content<S>,
    components: Vec<(String, Content<S>)>,
    uv_materials: BTreeSet<usize>,
}

/// What applying the plan made.
struct Made {
    /// Each planned component's: its uid and its entry in `imported`.
    components: Vec<Option<(ComponentUid, usize)>>,
    imported: Vec<ImportedComponent>,
    /// The entry of `imported` of each component made, in plan order.
    planned: Vec<usize>,
}

impl<K: Kernel> Document<K> {
    /// Imports a glTF file (see the module): with `wrap` into a new
    /// component named after the file. One undo step (`Import <file>`).
    pub(crate) fn import_gltf(
        &mut self,
        path: &str,
        name: Option<&str>,
        wrap: bool,
    ) -> Result<(ImportedFile, Option<ImportWrapper>), ModelError> {
        let gltf = read(path)?;
        let source = file_name(path);
        for note in &gltf.notes {
            eprintln!("import: glTF: {source}: {} ({})", note.words, note.detail);
        }
        let mut warnings: Vec<String> = gltf.notes.iter().map(|n| n.words.clone()).collect();

        let Planned {
            top,
            components,
            baked,
            deep,
        } = plan(&gltf);
        if baked > 0 {
            warnings.push(
                "nodes that scale or mirror what they hold are not components: their meshes came in \
                 moved into place"
                    .to_owned(),
            );
        }
        if deep {
            warnings.push("nodes nested more deeply than MitCad follows are left out".to_owned());
        }
        let bodies = top.bodies.len()
            + components
                .iter()
                .map(|c| c.content.bodies.len())
                .sum::<usize>();
        if bodies == 0 {
            let why = if warnings.is_empty() {
                String::new()
            } else {
                format!(": {}", warnings.join("; "))
            };
            return Err(invalid(format!("{source} has no triangles to import{why}")));
        }

        // The bodies' shapes, made before anything changes; textured ones
        // with their texture coordinates (mitcad#1862).
        let kernel = self.kernel();
        let mut uv_materials = BTreeSet::new();
        let make = |planned: Vec<PlannedBody>,
                    children: Vec<(usize, Transform)>,
                    warnings: &mut Vec<String>,
                    uv_materials: &mut BTreeSet<usize>|
         -> Content<K::Shape> {
            let mut out = Vec::with_capacity(planned.len());
            let mut materials = Vec::with_capacity(planned.len());
            for b in planned {
                let made = match &b.uvs {
                    Some(uvs) => kernel
                        .textured_triangle_body(&b.positions, &b.triangles, uvs)
                        .inspect(|_| uv_materials.extend(b.material)),
                    None => kernel.triangle_body(&b.positions, &b.triangles),
                };
                match made {
                    Ok(shape) => {
                        let color = b.material.map(|m| {
                            let c = gltf.materials[m].base_color;
                            [c[0], c[1], c[2]].map(mitcad_gltf::srgb)
                        });
                        out.push(ImportBody {
                            name: Some(b.name),
                            color,
                            shape,
                        });
                        materials.push(b.material);
                    }
                    Err(e) => {
                        eprintln!("import: glTF: {source}: {}: {e}", b.name);
                        warnings.push(format!("{} cannot be made a mesh body: left out", b.name));
                    }
                }
            }
            Content {
                bodies: out,
                materials,
                children,
            }
        };
        let top = make(top.bodies, top.children, &mut warnings, &mut uv_materials);
        let components = components
            .into_iter()
            .map(|c| {
                let content = make(
                    c.content.bodies,
                    c.content.children,
                    &mut warnings,
                    &mut uv_materials,
                );
                (c.name, content)
            })
            .collect();
        let parts = Parts {
            top,
            components,
            uv_materials,
        };

        let label = format!("Import {source}");
        let depth = self.undo_depth();
        let made = self.apply_gltf(path, name, wrap, &gltf, parts);
        match made {
            Ok((file, wrapper, more)) => {
                self.merge_undo(depth, &label);
                warnings.extend(more);
                let file = match file {
                    ImportedFile::Base(mut imported) => {
                        imported.warnings = warnings;
                        ImportedFile::Base(imported)
                    }
                    ImportedFile::Components {
                        components, loose, ..
                    } => ImportedFile::Components {
                        components,
                        warnings,
                        loose,
                    },
                };
                Ok((file, wrapper))
            }
            Err(e) => {
                self.undo_back_to(depth);
                Err(e)
            }
        }
    }

    /// Makes what the plan says: the components and their occurrences,
    /// a base feature per component with bodies (and one for the bodies
    /// of the component imported into), and the materials' appearances.
    /// The warnings it adds come back with it.
    fn apply_gltf(
        &mut self,
        path: &str,
        name: Option<&str>,
        wrap: bool,
        gltf: &Gltf,
        parts: Parts<K::Shape>,
    ) -> Result<(ImportedFile, Option<ImportWrapper>, Vec<String>), ModelError> {
        let source = file_name(path);
        let label = format!("Import {source}");
        let wrapper = if wrap {
            Some(self.add_import_wrapper(path)?)
        } else {
            None
        };
        let parent = wrapper
            .as_ref()
            .map_or_else(|| self.active_component(), |w| w.component);
        let Parts {
            top,
            components,
            uv_materials,
        } = parts;
        let mut warnings = Vec::new();

        // The components, each made once and placed by every occurrence.
        let mut made = Made {
            components: vec![None; components.len()],
            imported: Vec::new(),
            planned: Vec::new(),
        };
        let names: Vec<String> = components.iter().map(|(n, _)| n.clone()).collect();
        let children: Vec<Vec<(usize, Transform)>> =
            components.iter().map(|(_, c)| c.children.clone()).collect();
        let mut stack: Vec<(usize, ComponentUid, Transform)> = top
            .children
            .iter()
            .rev()
            .map(|(k, t)| (*k, parent, *t))
            .collect();
        while let Some((k, holder, placement)) = stack.pop() {
            if let Some((component, entry)) = made.components[k] {
                match self.add_occurrence(component, holder, placement) {
                    Ok(occurrence) => made.imported[entry].occurrences.push(occurrence),
                    Err(e) => warnings.push(format!("{}: {e}", names[k])),
                }
                continue;
            }
            let (component, occurrence) =
                self.add_component(Some(names[k].as_str()), holder, placement)?;
            made.components[k] = Some((component, made.imported.len()));
            made.planned.push(k);
            made.imported.push(ImportedComponent {
                component,
                name: self.assembly().name(component),
                occurrences: vec![occurrence],
                feature: None,
            });
            for (child, t) in children[k].iter().rev() {
                stack.push((*child, component, *t));
            }
        }

        // The base features: the components' bodies and those placed
        // straight into the component imported into.
        let mut inputs = Vec::new();
        let mut owners: Vec<Option<usize>> = Vec::new();
        let mut materials_of: Vec<Vec<Option<usize>>> = Vec::new();
        let mut contents: Vec<Option<Content<K::Shape>>> =
            components.into_iter().map(|(_, c)| Some(c)).collect();
        for (entry, &k) in made.planned.iter().enumerate() {
            let Some(content) = contents[k].take() else {
                continue;
            };
            if content.bodies.is_empty() {
                continue;
            }
            inputs.push(BaseInput {
                source: Some(source.clone()),
                component: Some(made.imported[entry].component),
                ..BaseInput::new(content.bodies)
            });
            owners.push(Some(entry));
            materials_of.push(content.materials);
        }
        let structured = !made.imported.is_empty();
        if !top.bodies.is_empty() {
            inputs.push(BaseInput {
                name: name.map(str::to_owned),
                source: Some(source.clone()),
                component: Some(parent),
                ..BaseInput::new(top.bodies)
            });
            owners.push(None);
            materials_of.push(top.materials);
        }
        let added = if inputs.is_empty() {
            Vec::new()
        } else {
            self.add_base_features(&label, inputs)?
        };

        // The materials as appearances, each made once.
        let mut taken: BTreeSet<String> = appearance::library()
            .iter()
            .chain(self.custom_appearances())
            .map(|a| a.name.clone())
            .collect();
        let mut ids: HashMap<usize, String> = HashMap::new();
        for (imported, materials) in added.iter().zip(&materials_of) {
            for (i, material) in materials.iter().enumerate() {
                let Some(m) = *material else { continue };
                let id = match ids.get(&m) {
                    Some(id) => id.clone(),
                    None => {
                        let uv = uv_materials.contains(&m);
                        let id =
                            self.create_appearance(&gltf_appearance(gltf, m, uv, &mut taken))?;
                        ids.insert(m, id.clone());
                        if unheld_maps(gltf, m) {
                            let words = "images of a material without a base colour image are not \
                                         read: the material keeps its values";
                            if !warnings.iter().any(|w| w == words) {
                                warnings.push(words.to_owned());
                            }
                        }
                        id
                    }
                };
                let uid = BodyUid::new(imported.feature.uid, i as u32);
                self.set_body_appearance(uid, Some(id.as_str()))?;
            }
        }

        let mut loose = None;
        for (imported, owner) in added.into_iter().zip(owners) {
            match owner {
                Some(entry) => made.imported[entry].feature = Some(imported),
                None => loose = Some(imported),
            }
        }
        let file = if structured {
            ImportedFile::Components {
                components: made.imported,
                warnings: Vec::new(),
                loose,
            }
        } else {
            let imported: Imported = loose.ok_or_else(|| invalid("the import made no bodies"))?;
            ImportedFile::Base(imported)
        };
        Ok((file, wrapper, warnings))
    }
}

/// Whether material `m` has images of other maps that read (normal,
/// occlusion, metalness and roughness) but no base colour image, without
/// which an appearance holds no texture set.
fn unheld_maps(gltf: &Gltf, m: usize) -> bool {
    let material = &gltf.materials[m];
    let read = |i: Option<usize>| {
        i.and_then(|i| gltf.images.get(i))
            .is_some_and(|i| i.bytes.is_some())
    };
    !textured(gltf, m)
        && (read(material.normal_image)
            || read(material.occlusion_image)
            || read(material.metallic_roughness_image))
}

/// The appearance of material `m`: its colours as sRGB, its images
/// embedded (the metallic-roughness image as the roughness map of its
/// green channel and the metalness map of its blue one, the occlusion
/// image's red channel as the occlusion; mitcad#1862); projected by the
/// bodies' texture coordinates with `uv`; named after it (made unique
/// among `taken`).
fn gltf_appearance(
    gltf: &Gltf,
    m: usize,
    uv: bool,
    taken: &mut BTreeSet<String>,
) -> AppearanceChange {
    let material = &gltf.materials[m];
    let base = material
        .name
        .clone()
        .unwrap_or_else(|| format!("glTF material {}", m + 1));
    let name = super::unique_name(&base, taken);
    let srgb = |c: [f64; 3]| c.map(mitcad_gltf::srgb);
    let [r, g, b, alpha] = material.base_color;
    let mut change = AppearanceChange {
        name: Some(name),
        base_color: Some(srgb([r, g, b])),
        metalness: Some(material.metallic),
        roughness: Some(material.roughness),
        opacity: material.blend.then_some(alpha.clamp(0.0, 1.0)),
        transmission: (material.transmission > 0.0).then_some(material.transmission),
        ior: material.ior.map(|i| i.clamp(1.0, 5.0)),
        ..AppearanceChange::default()
    };
    let peak = material.emissive.iter().copied().fold(0.0, f64::max);
    if peak > 0.0 {
        change.emission = Some(peak.min(1000.0));
        change.emission_color = Some(srgb(material.emissive.map(|v| v / peak)));
    }
    // Each image once in memory, however many maps show it (an image of
    // occlusion, roughness and metalness is one).
    let mut embedded: HashMap<usize, ImageData> = HashMap::new();
    let mut image = |i: Option<usize>| {
        let i = i?;
        let read = gltf.images.get(i)?;
        let bytes = read.bytes.as_ref()?;
        let data = embedded
            .entry(i)
            .or_insert_with(|| ImageData::from_bytes(bytes))
            .clone();
        Some((read.file_name.clone(), data))
    };
    // A set needs its base colour's image.
    if let Some((file, data)) = image(material.base_color_image) {
        let mut map = |i: Option<usize>, channel: Option<Channel>, factor: Option<f64>| {
            image(i).map(|(file, data)| TextureMap {
                path: file,
                data: Some(data),
                channel,
                factor,
                ..TextureMap::default()
            })
        };
        // glTF multiplies the image's channels by the factors (mitcad#1874):
        // each kept with its map where it is not 1.
        let factor = |f: f64| {
            let f = if f.is_finite() {
                f.clamp(0.0, 1.0)
            } else {
                1.0
            };
            (f != 1.0).then_some(f)
        };
        let packed = material.metallic_roughness_image;
        let normal = map(material.normal_image, None, None);
        let occlusion = map(material.occlusion_image, Some(Channel::Red), None);
        let roughness = map(packed, Some(Channel::Green), factor(material.roughness));
        let metalness = map(packed, Some(Channel::Blue), factor(material.metallic));
        change.texture = Some(Some(Texture {
            path: file,
            data: Some(data),
            projection: if uv { Projection::Uv } else { Projection::Box },
            normal,
            occlusion,
            roughness,
            metalness,
            ..Texture::default()
        }));
    }
    change
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_of_single_precision_are_rigid_and_scales_are_not() {
        let turn = 0.6_f32 as f64;
        let (s, c) = (turn.sin(), turn.cos());
        // A turn about z in single precision, 100 mm along x.
        let m: Matrix = [
            [c as f32 as f64, -s as f32 as f64, 0.0, 100.0],
            [s as f32 as f64, c as f32 as f64, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let t = rigid(&m).expect("a rotation");
        assert!(t.is_rigid());
        assert!((t.linear[0][0] - c).abs() < 1e-6);
        assert_eq!(t.translation, [100.0, 0.0, 0.0]);
        let mut scaled = IDENTITY;
        scaled[0][0] = 2.0;
        assert!(rigid(&scaled).is_none());
        let mut mirrored = IDENTITY;
        mirrored[2][2] = -1.0;
        assert!(rigid(&mirrored).is_none());
        assert!(rigid(&Y_UP_TO_Z_UP).is_some());
    }

    fn bounds(body: &PlannedBody) -> ([f64; 3], [f64; 3]) {
        let mut low = [f64::MAX; 3];
        let mut high = [f64::MIN; 3];
        for p in &body.positions {
            for (k, v) in p.iter().enumerate() {
                low[k] = low[k].min(*v);
                high[k] = high[k].max(*v);
            }
        }
        (low, high)
    }

    // glTF stores positions as 32-bit floats: a millimetre value read back
    // is off by up to about 1e-7 of itself, within the tests' 1e-6 mm.
    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    #[test]
    fn the_plan_places_bodies_in_millimetres_with_z_up() {
        use mitcad_gltf::testfile::{self, Form};
        let gltf =
            mitcad_gltf::read(&testfile::write(&testfile::assembly(), &Form::Glb).0, None).unwrap();
        let planned = plan(&gltf);
        // The two brackets are one component; the scaled node none.
        assert_eq!(planned.components.len(), 1);
        assert_eq!(planned.components[0].name, "Bracket");
        assert_eq!(planned.components[0].content.bodies.len(), 1);
        assert_eq!(planned.baked, 1);
        let names: Vec<&str> = planned.top.bodies.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Label", "Pin"]);
        let placements: Vec<[f64; 3]> = planned
            .top
            .children
            .iter()
            .map(|(_, t)| t.translation)
            .collect();
        assert!(near(placements[0], [100.0, 0.0, 0.0]) && near(placements[1], [300.0, 0.0, 0.0]));
        // The plate in its component as the file has it (20 x 10 x 5 mm),
        // its 8 corners welded, 12 triangles.
        let plate = &planned.components[0].content.bodies[0];
        assert!(
            near(bounds(plate).1, [20.0, 10.0, 5.0]),
            "{:?}",
            bounds(plate)
        );
        assert_eq!((plate.positions.len(), plate.triangles.len()), (8, 12));
        // The label 50 mm toward the viewer (MitCad's -y), 1 mm thick.
        let label = &planned.top.bodies[0];
        assert!(
            near(bounds(label).0, [0.0, -51.0, 0.0]),
            "{:?}",
            bounds(label)
        );
        assert!(
            near(bounds(label).1, [10.0, -50.0, 10.0]),
            "{:?}",
            bounds(label)
        );
        // The pin twice the plate's size, turned Z up.
        let pin = &planned.top.bodies[1];
        assert!(near(bounds(pin).0, [0.0, -10.0, 0.0]), "{:?}", bounds(pin));
        assert!(near(bounds(pin).1, [40.0, 0.0, 20.0]), "{:?}", bounds(pin));
        assert_eq!(pin.material, Some(0));
        assert_eq!(label.material, Some(1));
    }

    #[test]
    fn a_mirroring_node_keeps_its_triangles_facing_out() {
        use mitcad_gltf::testfile::{self, Form, TestNode};
        let mut scene = testfile::assembly();
        scene.nodes = vec![TestNode {
            scale: Some([-1.0, 1.0, 1.0]),
            ..TestNode::named("Mirrored", Some(0), &[])
        }];
        scene.roots = vec![0];
        let gltf = mitcad_gltf::read(&testfile::write(&scene, &Form::Glb).0, None).unwrap();
        let planned = plan(&gltf);
        let body = &planned.top.bodies[0];
        // The box's outward volume stays positive.
        let volume: f64 = body
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| body.positions[i as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0
            })
            .sum();
        // 32-bit positions: the volume within 1e-6 of itself.
        assert!((volume - 1000.0).abs() < 1e-3, "{volume}");
    }

    #[test]
    fn texture_coordinates_keep_the_corners_of_a_seam_apart() {
        use mitcad_gltf::testfile::{self, Form};
        let gltf = mitcad_gltf::read(
            &testfile::write(&testfile::textured_cube(), &Form::Glb).0,
            None,
        )
        .unwrap();
        let planned = plan(&gltf);
        let cube = &planned.top.bodies[0];
        // Corners are one vertex where their points and coordinates are the
        // same, apart where the coordinates differ (MitCad's: v up the
        // image): the file's 24 corners hold 20 such pairs (the origin, for
        // one, is (0, 1) in glTF's on two sides that meet there), more than
        // the cube's 8 points.
        let read = &gltf.meshes[0].primitives[0];
        let mut pairs: Vec<([u64; 3], [u64; 2])> = read
            .positions
            .iter()
            .zip(read.uvs.as_ref().unwrap())
            .map(|(p, uv)| (p.map(f64::to_bits), uv.map(f64::to_bits)))
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        assert_eq!(pairs.len(), 20);
        let uvs = cube.uvs.as_ref().expect("the texture coordinates");
        assert_eq!((cube.positions.len(), uvs.len()), (20, 20));
        assert_eq!(cube.triangles.len(), 12);
        assert_eq!(&uvs[..4], &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        // Without an image the corners are welded by their points alone.
        let mut plain = testfile::textured_cube();
        plain.materials[0].image = None;
        let gltf = mitcad_gltf::read(&testfile::write(&plain, &Form::Glb).0, None).unwrap();
        let cube = &plan(&gltf).top.bodies[0];
        assert!(cube.uvs.is_none());
        assert_eq!(cube.positions.len(), 8);
    }

    #[test]
    fn gltf_files_are_known_by_their_extension() {
        assert!(is_gltf_path("/models/robot.glb"));
        assert!(is_gltf_path("C:\\models\\robot.GLTF"));
        assert!(!is_gltf_path("/models/robot.obj"));
    }
}
