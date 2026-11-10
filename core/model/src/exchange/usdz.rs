// SPDX-License-Identifier: MIT
//! USDZ export (mitcad#123): the bodies as shaded meshes in the design's
//! component hierarchy, with their appearances as materials, for AR viewers
//! and other programs that read USD. The model builds a
//! [`mitcad_usdz::Scene`] from the kernel's face meshes
//! ([`Kernel::face_meshes`]) and `mitcad-usdz` writes it.
//!
//! - Each placement of a body is a mesh under the `Xform`s of the
//!   occurrences that place it (each with its placement in its parent), so
//!   the file keeps the components; a body of the root is a mesh of the
//!   root. In the components' coordinates each body is one mesh of the root.
//! - Writing all bodies leaves out hidden ones: hidden bodies, and bodies
//!   only hidden occurrences place; a placement by a hidden occurrence is
//!   left out. Listed bodies are written where they are shown, or by all
//!   their occurrences when none is shown, as the other formats.
//! - A body's appearance (else its colour from `colors` or its import,
//!   else the default look) is its material; faces with appearances of
//!   their own are subsets with theirs. A texture's image goes into the
//!   package and the meshes get texture coordinates from its projection,
//!   as the rendered view projects it.
//! - With an animation (mitcad#1597, `usdz_animation`), the occurrences'
//!   `Xform`s and the bodies' meshes it moves get time-sampled placements;
//!   the meshes are written once.

use std::collections::BTreeMap;
use std::path::Path;

use mitcad_usdz::{Image, Material, Mesh, Node, Placement, Scene, Subset, TexCoords};
use mitcad_vec3::{cross, dot, sub};

use super::usdz_animation::{MeshPlaces, UsdzAnimation};
use super::{BodyKind, Exported, FaceMeshes, MeshTolerance, invalid, kernel_error};
use crate::appearance::{Appearance, Projection, Texture, image_format};
use crate::document::{BodyView, Document, InstanceView, ModelError, PrintColors};
use crate::ids::{BodyUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::print_changes::{Bands, band_mesh};
use crate::transform::Transform;

/// An sRGB component in [0, 1] as linear light.
pub fn linear(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn placement(t: &Transform) -> Placement {
    Placement {
        linear: t.linear,
        translation: t.translation,
    }
}

/// The materials of the file, one per appearance (or colour), and their
/// images.
#[derive(Default)]
struct Materials {
    materials: Vec<Material>,
    /// The appearance or colour each material is made from.
    keys: BTreeMap<String, usize>,
    /// Texture parameters of each material with an image.
    textures: BTreeMap<usize, Texture>,
    images: Vec<Image>,
    warnings: Vec<String>,
}

impl Materials {
    /// The material of an appearance, made at its first use; `key` names it
    /// among the file's materials.
    fn of(&mut self, key: &str, appearance: &Appearance, folder: Option<&Path>) -> usize {
        if let Some(&index) = self.keys.get(key) {
            return index;
        }
        let index = self.materials.len();
        let mut texture = None;
        if let Some(t) = &appearance.texture {
            match texture_image(t, folder) {
                Ok((data, extension)) => {
                    texture = Some(self.images.len());
                    let name = mitcad_usdz::identifier(key);
                    let path = (1..)
                        .map(|n| match n {
                            1 => format!("textures/{name}.{extension}"),
                            n => format!("textures/{name}_{n}.{extension}"),
                        })
                        .find(|p| self.images.iter().all(|i| i.path != *p))
                        .expect("a free name exists");
                    self.images.push(Image { path, data });
                    self.textures.insert(index, t.clone());
                }
                Err(why) => self.warnings.push(format!(
                    "{}: {why}; its base colour is written",
                    if appearance.name.is_empty() {
                        key
                    } else {
                        &appearance.name
                    }
                )),
            }
        }
        // UsdPreviewSurface has no transmission: light going through makes
        // the surface see-through, clear glass faintly visible.
        let opacity = appearance.opacity * (1.0 - 0.9 * appearance.transmission);
        self.materials.push(Material {
            name: key.to_owned(),
            diffuse_color: appearance.base_color.map(linear),
            metallic: appearance.metalness,
            roughness: appearance.roughness,
            opacity,
            ior: appearance.ior,
            clearcoat: appearance.coat,
            clearcoat_roughness: appearance.coat_roughness,
            emissive_color: appearance
                .emission_color
                .map(|c| linear(c) * appearance.emission),
            texture,
        });
        self.keys.insert(key.to_owned(), index);
        index
    }
}

/// A texture's image file: the embedded one, or the file at its path
/// (relative to `folder`, the design's); its bytes and extension.
fn texture_image(
    texture: &Texture,
    folder: Option<&Path>,
) -> Result<(Vec<u8>, &'static str), String> {
    let bytes = match &texture.data {
        Some(data) => data.bytes()?,
        None => {
            let path = Path::new(&texture.path);
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                match folder {
                    Some(folder) => folder.join(path),
                    None => {
                        return Err(format!(
                            "the texture image {} is relative to the design's folder, which is not known",
                            texture.path
                        ));
                    }
                }
            };
            std::fs::read(&path)
                .map_err(|e| format!("the texture image {} cannot be read ({e})", path.display()))?
        }
    };
    match image_format(&bytes) {
        Some("png") => Ok((bytes, "png")),
        Some(_) => Ok((bytes, "jpg")),
        None => Err(format!(
            "the texture image {} is no PNG or JPEG file",
            texture.path
        )),
    }
}

/// Texture coordinates of a point on a surface facing `normal`, as the
/// rendered view projects the texture (`docs/rendering.md`, Materials):
/// planar takes (x, y); box takes (y, z) where the surface faces X most,
/// (x, z) where it faces Y, (x, y) where it faces Z; turned back by the
/// rotation and divided by the size of one repeat.
fn project(texture: &Texture, p: [f64; 3], normal: [f64; 3]) -> [f32; 2] {
    let [x, y, z] = normal.map(f64::abs);
    let (a, b) = match texture.projection {
        Projection::Planar => (p[0], p[1]),
        Projection::Box if x >= y.max(z) => (p[1], p[2]),
        Projection::Box if y >= z => (p[0], p[2]),
        Projection::Box => (p[0], p[1]),
    };
    let (sin, cos) = texture.rotation.sin_cos();
    let u = (a * cos + b * sin) / texture.size[0];
    let v = (-a * sin + b * cos) / texture.size[1];
    [u as f32, v as f32]
}

impl<K: Kernel> Document<K> {
    /// A USDZ file of the bodies (see the module documentation).
    /// `instances` are the placements in the design's coordinates (within
    /// the occurrence asked for), none for the components' coordinates;
    /// `within_depth` the length of that occurrence's path (0 without one);
    /// `all` when all bodies are written; `animation` the time-sampled
    /// placements (mitcad#1597), checked here against what is written.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn export_usdz(
        &self,
        path: &str,
        chosen: &[&BodyView<'_, K::Shape>],
        all: bool,
        tolerance: &MeshTolerance,
        color: impl Fn(BodyUid) -> Option<[f64; 3]>,
        instances: Option<&[InstanceView]>,
        within_depth: usize,
        folder: Option<&Path>,
        animation: Option<&UsdzAnimation>,
        print_colors: bool,
    ) -> Result<Exported, ModelError> {
        let mut exported = Exported::default();
        let mut materials = Materials::default();
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Mitcad");
        let mut root = Node::new(name);
        // Nodes by occurrence path, made as placements need them.
        let mut nodes: BTreeMap<Vec<OccurrenceUid>, Node> = BTreeMap::new();
        // Where each body's meshes go (mitcad#1597): the node's path and
        // the mesh's index in it.
        let mut mesh_places = MeshPlaces::new();
        for body in chosen {
            let attributes = self.body_attributes(body.uid);
            if all && !attributes.visible {
                exported.skipped.push((body.uid, "hidden".to_owned()));
                continue;
            }
            let kind = self.kernel().body_kind(body.shape).map_err(kernel_error)?;
            if kind == BodyKind::Empty {
                if all {
                    exported
                        .skipped
                        .push((body.uid, "an empty body".to_owned()));
                    continue;
                }
                return Err(invalid(format!("{} is empty", body.name)));
            }
            // Where the body goes: occurrence paths (empty: the root).
            let paths: Vec<Vec<OccurrenceUid>> = match instances {
                None => vec![Vec::new()],
                Some(instances) => {
                    let mine: Vec<&InstanceView> =
                        instances.iter().filter(|i| i.body == body.uid).collect();
                    // Shown: the body and the occurrences on its path, those
                    // below the occurrence asked for (which is written even
                    // when it is hidden).
                    let assembly = self.assembly();
                    let shown_below = |i: &InstanceView| {
                        attributes.visible
                            && i.path
                                .iter()
                                .skip(within_depth)
                                .all(|o| assembly.occurrence(*o).is_some_and(|o| o.visible))
                    };
                    let shown: Vec<&InstanceView> = mine
                        .iter()
                        .copied()
                        .filter(|i| {
                            if within_depth == 0 {
                                i.visible
                            } else {
                                shown_below(i)
                            }
                        })
                        .collect();
                    if all && shown.is_empty() && !mine.is_empty() {
                        exported
                            .skipped
                            .push((body.uid, "in hidden occurrences only".to_owned()));
                        continue;
                    }
                    let placed = if shown.is_empty() { mine } else { shown };
                    if placed.is_empty() {
                        vec![Vec::new()]
                    } else {
                        placed.iter().map(|i| i.path.clone()).collect()
                    }
                }
            };
            let faces = self
                .kernel()
                .face_meshes(body.shape, tolerance)
                .map_err(|e| invalid(super::body_failure(&body.name, body.uid, &e)))?;
            if faces.triangles.is_empty() {
                return Err(invalid(format!("{} has no triangles", body.name)));
            }
            // Print colours by height (mitcad#1596): a mesh per placement,
            // cut where its filament changes.
            if print_colors {
                let places = mesh_places.entry(body.uid).or_default();
                for occurrences in paths {
                    let transform = instances
                        .and_then(|all| {
                            all.iter()
                                .find(|i| i.body == body.uid && i.path == occurrences)
                        })
                        .map_or(Transform::IDENTITY, |i| i.transform);
                    let at = instances.map(|_| occurrences.as_slice());
                    let colors = self.print_colors(body.uid, at, &transform);
                    for note in &colors.notes {
                        let note = format!("{}: {note}", body.name);
                        if !exported.warnings.contains(&note) {
                            exported.warnings.push(note);
                        }
                    }
                    let mesh = print_color_mesh(
                        &body.name,
                        &faces,
                        &transform,
                        &colors,
                        kind == BodyKind::Sheet,
                        &mut materials,
                    );
                    // Where it goes, for the animation's samples
                    // (mitcad#1597).
                    let node = if occurrences.is_empty() {
                        &mut root
                    } else {
                        nodes
                            .entry(occurrences.clone())
                            .or_insert_with(|| Node::new(""))
                    };
                    places.push((occurrences, node.meshes.len()));
                    node.meshes.push(mesh);
                }
                exported.bodies.push(body.uid);
                continue;
            }
            // The body's material, and those of faces of their own.
            let fallback = || {
                let mut look = Appearance::default();
                match color(body.uid) {
                    Some(c) => {
                        look.base_color = c;
                        let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                        // Keys that no appearance id has (a space).
                        (format!("color {r:02x}{g:02x}{b:02x}"), look)
                    }
                    None => ("default look".to_owned(), look),
                }
            };
            let (key, look) = match attributes
                .appearance
                .as_deref()
                .and_then(|id| self.appearance(id))
            {
                Some(appearance) => (appearance.id.clone(), appearance.clone()),
                None => fallback(),
            };
            let material = materials.of(&key, &look, folder);
            // The first triangle of each face.
            let mut starts = Vec::with_capacity(faces.face_triangles.len() + 1);
            let mut total = 0u32;
            for count in &faces.face_triangles {
                starts.push(total);
                total += count;
            }
            starts.push(total);
            let mut triangle_material = vec![material; faces.triangles.len()];
            let mut subsets = Vec::new();
            for (id, indices) in self.face_appearance_indices(body.uid) {
                let Some(appearance) = self.appearance(&id) else {
                    continue;
                };
                let face_material = materials.of(&id, appearance, folder);
                let triangles: Vec<u32> = indices
                    .iter()
                    .filter(|&&f| f + 1 < starts.len())
                    .flat_map(|&f| starts[f]..starts[f + 1])
                    .collect();
                if triangles.is_empty() {
                    continue;
                }
                for &t in &triangles {
                    triangle_material[t as usize] = face_material;
                }
                subsets.push(Subset {
                    name: format!("Faces_{id}"),
                    triangles,
                    material: face_material,
                });
            }
            // Texture coordinates when a material of the mesh has an image:
            // per corner, each triangle by its own material's projection and
            // its facet's normal.
            let tex_coords = triangle_material
                .iter()
                .any(|m| materials.textures.contains_key(m))
                .then(|| {
                    let mut st = Vec::with_capacity(faces.triangles.len() * 3);
                    for (t, m) in faces.triangles.iter().zip(&triangle_material) {
                        let [a, b, c] = t.map(|i| faces.vertices[i as usize]);
                        let normal = cross(sub(b, a), sub(c, a));
                        for p in [a, b, c] {
                            st.push(match materials.textures.get(m) {
                                Some(texture) => project(texture, p, normal),
                                None => [0.0, 0.0],
                            });
                        }
                    }
                    TexCoords::FaceVarying(st)
                });
            let mesh = Mesh {
                name: body.name.clone(),
                points: faces.vertices,
                normals: faces.normals,
                triangles: faces.triangles,
                tex_coords,
                double_sided: kind == BodyKind::Sheet,
                material,
                subsets,
                samples: Vec::new(),
            };
            let places = mesh_places.entry(body.uid).or_default();
            for occurrences in paths {
                let node = if occurrences.is_empty() {
                    &mut root
                } else {
                    nodes
                        .entry(occurrences.clone())
                        .or_insert_with(|| Node::new(""))
                };
                places.push((occurrences, node.meshes.len()));
                node.meshes.push(mesh.clone());
            }
            exported.bodies.push(body.uid);
        }
        if exported.bodies.is_empty() {
            return Err(invalid("there are no shown bodies to export"));
        }
        // The occurrences' nodes, nested: each path's node under its
        // parent path's, deepest first.
        let mut paths: Vec<Vec<OccurrenceUid>> = nodes.keys().cloned().collect();
        for path in paths.clone() {
            for depth in 1..path.len() {
                let parent = path[..depth].to_vec();
                if !nodes.contains_key(&parent) {
                    nodes.insert(parent.clone(), Node::new(""));
                    paths.push(parent);
                }
            }
        }
        // The animation's samples (mitcad#1597), with every node in place.
        if let Some(animation) = animation {
            exported.animation =
                Some(self.usdz_samples(animation, &mut root, &mut nodes, &mesh_places)?);
        }
        // Deepest first, so that a node has its children when it goes into
        // its parent; siblings in the order the occurrences were made.
        paths.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        for path in paths {
            let mut node = nodes.remove(&path).expect("every path has a node");
            let occurrence = *path.last().expect("not the root");
            node.name = self.assembly().occurrence_name(occurrence);
            node.placement = placement(&self.placement(occurrence).unwrap_or(Transform::IDENTITY));
            if path.len() == 1 {
                root.children.push(node);
            } else {
                nodes
                    .get_mut(&path[..path.len() - 1])
                    .expect("the parent's node exists")
                    .children
                    .push(node);
            }
        }
        let Materials {
            materials: made,
            images,
            warnings,
            ..
        } = materials;
        let names = made.iter().map(|m| m.name.clone()).collect();
        let scene = Scene {
            root,
            materials: made,
            images,
            documentation: format!("MitCad {}", env!("CARGO_PKG_VERSION")),
            animation: animation.map(|a| mitcad_usdz::Animation {
                fps: a.fps,
                times: a.times.clone(),
            }),
        };
        let data = mitcad_usdz::write(&scene).map_err(|e| invalid(e.to_string()))?;
        std::fs::write(path, data).map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        exported.materials = names;
        exported.warnings.extend(warnings);
        Ok(exported)
    }
}

/// A placement's mesh in its print colours (mitcad#1596): the body's
/// triangles cut at the layer boundaries of its filament changes, measured
/// along its print direction from its lowest point
/// ([`crate::print_changes::band_mesh`], as `model_render`'s print colours
/// cut them), the first band in the mesh's material and each other band a
/// subset with its filament's colour.
fn print_color_mesh(
    name: &str,
    faces: &FaceMeshes,
    transform: &Transform,
    colors: &PrintColors,
    double_sided: bool,
    materials: &mut Materials,
) -> Mesh {
    let height = |p: [f64; 3]| dot(transform.apply_point(p), colors.up);
    let (low, high) = faces
        .vertices
        .iter()
        .map(|&p| height(p))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), h| {
            (lo.min(h), hi.max(h))
        });
    let bands = Bands::new(colors.start, &colors.changes, low, high);
    let banded = band_mesh(
        &faces.vertices,
        &faces.normals,
        &faces.triangles,
        height,
        &bands.cuts,
    );
    let ids: Vec<usize> = bands
        .colors
        .iter()
        .map(|&c| {
            let look = Appearance {
                base_color: c,
                ..Appearance::default()
            };
            let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
            // A key no appearance id has (a space).
            materials.of(&format!("print color {r:02x}{g:02x}{b:02x}"), &look, None)
        })
        .collect();
    let subsets = (1..ids.len())
        .filter_map(|b| {
            let triangles: Vec<u32> = banded
                .band
                .iter()
                .enumerate()
                .filter(|(_, band)| **band == b)
                .map(|(t, _)| t as u32)
                .collect();
            (!triangles.is_empty()).then(|| Subset {
                name: format!("PrintColor_{b}"),
                triangles,
                material: ids[b],
            })
        })
        .collect();
    Mesh {
        name: name.to_owned(),
        points: banded.vertices,
        normals: banded.normals,
        triangles: banded.triangles,
        tex_coords: None,
        double_sided,
        material: ids[0],
        subsets,
        samples: Vec::new(),
    }
}
