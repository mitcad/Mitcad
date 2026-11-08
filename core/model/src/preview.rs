// SPDX-License-Identifier: MIT
//! Preview images of the design (mitcad#1486): `model_render`'s scene,
//! camera and picture. The bodies drawn are placed as the design places
//! them ([`Document::instances`]), with their appearances (the body's, its
//! single faces', else an imported colour, else the view's body colour);
//! the camera frames their bounding boxes. `mitcad-preview` draws them; it
//! knows no document. Nothing here changes the document.
//!
//! - `bodies`: the bodies the design shows (light bulbs, hidden
//!   occurrences, isolation), all, or those named; `hidden` leaves some out.
//! - Names: a body id (`F2.b0`), a feature id (`F7`: its bodies), a
//!   component id (`C1`: its bodies wherever placed), an occurrence path
//!   (`O1/O4`, or by names `Arm:1/Pin:2`: what it places), or a body's name.
//! - Each body is meshed once per picture, within a thousandth of its
//!   box's diagonal (0.01 to 0.5 mm) and 12 degrees, as the 3D view meshes
//!   it, on a copy of its shape.
//! - `colors`, `highlight`, `section` and `explode` (mitcad#1487):
//!   [`inspect`].
//! - `overlay` (mitcad#1488, `preview/overlay.rs`): what Print Analysis'
//!   functions find (or the bodies failed features read), drawn over the
//!   bodies, with its summary.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Instant;

use mitcad_preview::raster::{self, Mesh, Scene, Style};
pub use mitcad_preview::{
    Aabb, Background, Camera, Fit, Frame, Projection, RenderArgs, SavedView, Selection, View, png,
    resolve_camera,
};
// The inspection options and several views (mitcad#1487).
pub use mitcad_preview::{Inspect, views};

mod inspect;
use mitcad_vec3::{cross, dot, scale, unit};

use crate::document::{Document, InstanceView, ModelError};
use crate::exchange::{BodyKind, FaceMeshes, MeshTolerance};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

// The overlay's analyses (mitcad#1488).
mod overlay;

/// Encodings at most per picture for the byte budget, and the longer
/// side a smaller picture never goes below (`ui_screenshot`'s).
pub const BUDGET_ATTEMPTS: usize = 8;
pub const BUDGET_MIN_SIDE: u32 = 64;

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

/// The camera and the bodies of a picture.
#[derive(Debug, Clone)]
pub struct PreviewPlan {
    pub camera: Camera,
    pub warnings: Vec<String>,
    /// The placements drawn.
    pub instances: Vec<InstanceView>,
}

/// One encoded picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Triangles and edge segments drawn.
    pub triangles: usize,
    pub edges: usize,
}

/// The picture within the byte budget.
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    /// The full picture fits.
    Full,
    /// A smaller one does.
    Smaller(Encoded),
    /// None did: the smallest tried.
    TooLarge {
        bytes: usize,
        width: u32,
        height: u32,
    },
}

/// A preview picture.
#[derive(Debug, Clone)]
pub struct Preview {
    pub camera: Camera,
    pub warnings: Vec<String>,
    /// The bodies drawn, each once, in the order they were drawn.
    pub bodies: Vec<BodyUid>,
    /// The picture at the size asked for.
    pub full: Encoded,
    pub inline: Inline,
    /// Milliseconds: meshing (and the scene), drawing, encoding.
    pub mesh_ms: f64,
    pub raster_ms: f64,
    pub encode_ms: f64,
    /// The overlay's summary (mitcad#1488), when one is drawn.
    pub overlay: Option<serde_json::Value>,
    /// Milliseconds the overlay's analysis took (in `mesh_ms` too).
    pub overlay_ms: f64,
}

/// The occurrence path as text (`O1/O4`; empty for the root).
fn path_text(path: &[OccurrenceUid]) -> String {
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

fn occurrence_path(text: &str) -> Option<Vec<OccurrenceUid>> {
    text.split('/')
        .map(|part| OccurrenceUid::from_str(part).ok())
        .collect()
}

impl<K: Kernel> Document<K> {
    /// Which placements an id names (see the module documentation).
    fn named_instances(
        &self,
        id: &str,
        instances: &[InstanceView],
    ) -> Result<Vec<bool>, ModelError> {
        let id = id.trim();
        let mask: Vec<bool> = if let Ok(body) = BodyUid::from_str(id) {
            instances.iter().map(|i| i.body == body).collect()
        } else if let Ok(feature) = FeatureUid::from_str(id) {
            instances
                .iter()
                .map(|i| i.body.feature == feature)
                .collect()
        } else if let Ok(component) = ComponentUid::from_str(id) {
            instances.iter().map(|i| i.component == component).collect()
        } else if let Some(path) = occurrence_path(id) {
            instances
                .iter()
                .map(|i| i.path.starts_with(&path))
                .collect()
        } else {
            let below = format!("{id}/");
            instances
                .iter()
                .map(|i| {
                    i.name == id
                        || (!i.path_name.is_empty()
                            && (i.path_name == id || i.path_name.starts_with(&below)))
                })
                .collect()
        };
        if mask.iter().any(|&m| m) {
            Ok(mask)
        } else {
            Err(invalid(format!(
                "{id} names no body placed in the design (a body, feature or component id such as F2.b0, F2 or C1, an occurrence path such as O1/O4, or a body's name)"
            )))
        }
    }

    /// The placements `ids` name together.
    fn named_mask(
        &self,
        ids: &[String],
        instances: &[InstanceView],
    ) -> Result<Vec<bool>, ModelError> {
        let mut mask = vec![false; instances.len()];
        for id in ids {
            for (m, hit) in mask.iter_mut().zip(self.named_instances(id, instances)?) {
                *m |= hit;
            }
        }
        Ok(mask)
    }

    /// Every placement of a body at the marker.
    fn preview_instances(&self) -> Vec<InstanceView> {
        let bodies: std::collections::BTreeSet<BodyUid> =
            self.bodies().iter().map(|b| b.uid).collect();
        self.instances()
            .into_iter()
            .filter(|i| bodies.contains(&i.body))
            .collect()
    }

    /// The bounding box of placements, in the design's coordinates.
    fn instances_box(&self, instances: &[&InstanceView]) -> Option<Aabb> {
        let mut found: Option<Aabb> = None;
        for instance in instances {
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let Ok(Some(b)) = self.kernel().bounding_box(shape) else {
                continue;
            };
            let placed = Aabb::of_points(b.corners().map(|c| instance.transform.apply_point(c)));
            found = match (found, placed) {
                (Some(f), Some(p)) => Some(f.union(p)),
                (f, p) => f.or(p),
            };
        }
        found
    }

    /// `model_render`'s camera and bodies, without drawing: what every
    /// quality frames (the ray traced picture of mitcad#1489 too).
    pub fn preview_camera(&self, args: &RenderArgs) -> Result<PreviewPlan, ModelError> {
        let all = self.preview_instances();
        let isolation = self.isolation();
        let isolated = |i: &InstanceView| {
            isolation.is_empty()
                || isolation.iter().any(|item| match item.body {
                    Some(body) => i.body == body && path_text(&i.path) == item.occurrence,
                    None => occurrence_path(&item.occurrence)
                        .is_some_and(|path| i.path.starts_with(&path)),
                })
        };
        let mut drawn: Vec<bool> = match &args.bodies {
            Selection::Visible => all.iter().map(|i| i.visible && isolated(i)).collect(),
            Selection::All => vec![true; all.len()],
            Selection::Items(ids) => self.named_mask(ids, &all)?,
        };
        if !args.hidden.is_empty() {
            let hidden = self.named_mask(&args.hidden, &all)?;
            for (d, h) in drawn.iter_mut().zip(hidden) {
                *d &= !h;
            }
        }
        let mut instances: Vec<InstanceView> = all
            .iter()
            .zip(&drawn)
            .filter(|(_, d)| **d)
            .map(|(i, _)| i.clone())
            .collect();
        // Moved apart (mitcad#1487): fit frames them where they are drawn.
        let mut all = all;
        if args.inspect.explode != 0.0 {
            let offsets = self.explode_offsets(&instances, args.inspect.explode);
            let mut moved = offsets.into_iter();
            for (instance, d) in all.iter_mut().zip(&drawn) {
                if *d && let Some(offset) = moved.next() {
                    inspect::moved(instance, offset);
                }
            }
            instances = all
                .iter()
                .zip(&drawn)
                .filter(|(_, d)| **d)
                .map(|(i, _)| i.clone())
                .collect();
        }
        let mut warnings = Vec::new();
        if instances.is_empty() {
            warnings.push("no body is drawn".to_owned());
        }
        let (bounds, explicit) = match &args.fit {
            Fit::Default => (
                self.instances_box(&instances.iter().collect::<Vec<_>>()),
                false,
            ),
            Fit::All => (
                self.instances_box(&instances.iter().collect::<Vec<_>>()),
                true,
            ),
            Fit::Items(ids) => {
                let mask = self.named_mask(ids, &all)?;
                let named: Vec<&InstanceView> = all
                    .iter()
                    .zip(mask)
                    .filter(|(_, m)| *m)
                    .map(|(i, _)| i)
                    .collect();
                (self.instances_box(&named), true)
            }
            Fit::Box(b) => (Some(*b), true),
        };
        let saved: Vec<SavedView> = self
            .named_views()
            .iter()
            .map(|v| SavedView {
                name: v.name.clone(),
                eye: v.eye,
                target: v.target,
                up: v.up,
                perspective: v.perspective,
                height: v.height,
            })
            .collect();
        let (camera, more) = resolve_camera(
            &args.view,
            args.projection,
            Frame { bounds, explicit },
            args.aspect(),
            &saved,
        )
        .map_err(invalid)?;
        warnings.extend(more);
        Ok(PreviewPlan {
            camera,
            warnings,
            instances,
        })
    }

    /// The scene of the placements: each body meshed once.
    fn preview_scene(
        &self,
        instances: &[InstanceView],
        looks: &inspect::Looks,
        warnings: &mut Vec<String>,
    ) -> (Scene, Vec<BodyUid>) {
        struct Meshed {
            faces: FaceMeshes,
            kind: BodyKind,
            colors: Vec<[f64; 3]>,
            see_through: bool,
        }
        let mut meshed: BTreeMap<BodyUid, Option<Meshed>> = BTreeMap::new();
        let mut scene = Scene::default();
        let mut bodies = Vec::new();
        for (index, instance) in instances.iter().enumerate() {
            let entry = meshed.entry(instance.body).or_insert_with(|| {
                self.mesh_body(instance.body, warnings)
                    .map(|(faces, kind, colors, see_through)| Meshed {
                        faces,
                        kind,
                        colors,
                        see_through,
                    })
            });
            let Some(body) = entry else {
                continue;
            };
            if !bodies.contains(&instance.body) {
                bodies.push(instance.body);
            }
            let transparent = instance.transparent || body.see_through;
            let base = body.colors.first().copied().unwrap_or(raster::BODY_COLOR);
            // colors and highlight (mitcad#1487); a cut face has the body's
            // colour.
            let look = looks.instances.get(index).cloned().unwrap_or_default();
            scene.meshes.push(placed_mesh(
                &body.faces,
                &instance.transform,
                Mesh {
                    face_colors: look.face_colors(&body.colors),
                    cap_color: Some(look.color.unwrap_or(base)),
                    edge_color: if transparent {
                        base.map(|c| (c * 0.6 * 255.0).round() as u8)
                    } else {
                        raster::EDGE_COLOR
                    },
                    transparent,
                    double_sided: body.kind == BodyKind::Sheet,
                    faceted: body.kind == BodyKind::Mesh,
                    ..Mesh::default()
                },
            ));
        }
        (scene, bodies)
    }

    /// A body's display mesh and each face's colour; None (with a
    /// warning) when it cannot be meshed or is empty.
    fn mesh_body(
        &self,
        uid: BodyUid,
        warnings: &mut Vec<String>,
    ) -> Option<(FaceMeshes, BodyKind, Vec<[f64; 3]>, bool)> {
        let shape = self.body_shape(uid)?;
        let name = self.body_name(uid);
        let kind = match self.kernel().body_kind(shape) {
            Ok(BodyKind::Empty) => return None,
            Ok(kind) => kind,
            Err(e) => {
                warnings.push(format!("{name} ({uid}) is not drawn: {e}"));
                return None;
            }
        };
        let diagonal = match self.kernel().bounding_box(shape) {
            Ok(Some(b)) => mitcad_vec3::norm(mitcad_vec3::sub(b.max, b.min)),
            _ => 0.0,
        };
        let tolerance = MeshTolerance {
            deviation: display_deviation(diagonal),
            angle: 12f64.to_radians(),
            face_triangles: 20_000,
            max_edge: 0.0,
        };
        let faces = match self.kernel().face_meshes(shape, &tolerance) {
            Ok(faces) => faces,
            Err(e) => {
                warnings.push(format!("{name} ({uid}) is not drawn: {e}"));
                return None;
            }
        };
        let attributes = self.body_attributes(uid);
        let look = attributes
            .appearance
            .as_deref()
            .and_then(|id| self.appearance(id));
        let base = match look {
            Some(appearance) => appearance.display_color(),
            None => self.imported_color(uid).unwrap_or(raster::BODY_COLOR),
        };
        let see_through = look.is_some_and(|a| a.opacity < 1.0 || a.transmission > 0.5);
        let count = faces.face_triangles.len().max(1);
        let mut colors = vec![base; count];
        for (id, indices) in self.face_appearance_indices(uid) {
            if let Some(appearance) = self.appearance(&id) {
                for f in indices {
                    if let Some(color) = colors.get_mut(f) {
                        *color = appearance.display_color();
                    }
                }
            }
        }
        Some((faces, kind, colors, see_through))
    }

    /// `model_render`'s picture at `args.size`, and within `max_bytes`
    /// (the PNG's bytes) when given: drawn again smaller with the same
    /// camera, at most [`BUDGET_ATTEMPTS`] encodings in all, never below
    /// [`BUDGET_MIN_SIDE`] pixels on the longer side.
    pub fn render_preview(
        &self,
        args: &RenderArgs,
        max_bytes: Option<u64>,
    ) -> Result<Preview, ModelError> {
        let started = Instant::now();
        let plan = self.preview_camera(args)?;
        let mut warnings = plan.warnings;
        let looks = self.preview_looks(&plan.instances, &args.inspect, &mut warnings);
        let (mut scene, bodies) = self.preview_scene(&plan.instances, &looks, &mut warnings);
        // What an analysis finds, over the bodies drawn (mitcad#1488).
        let overlay_started = Instant::now();
        let overlay = match &args.overlay {
            Some(overlay) => {
                let drawn: Vec<&InstanceView> = plan
                    .instances
                    .iter()
                    .filter(|i| bodies.contains(&i.body))
                    .collect();
                let (meshes, summary) =
                    self.preview_overlay(overlay, &bodies, &drawn, &mut warnings)?;
                scene.overlays = meshes;
                Some(summary)
            }
            None => None,
        };
        let overlay_ms = elapsed(overlay_started);
        let prepared = raster::prepare(&scene);
        let inspection = raster::Inspection {
            section: args.inspect.section,
            accent_lines: looks.lines,
        };
        let mesh_ms = elapsed(started);
        let style = Style {
            background: args.background,
            edges: args.edges,
        };
        let (mut raster_ms, mut encode_ms) = (0.0, 0.0);
        let mut draw = |size: [u32; 2]| {
            let start = Instant::now();
            let picture = raster::render_with(&prepared, &plan.camera, size, &style, &inspection);
            raster_ms += elapsed(start);
            let start = Instant::now();
            let png = png::encode(
                picture.width,
                picture.height,
                picture.channels,
                &picture.pixels,
            );
            encode_ms += elapsed(start);
            Encoded {
                png,
                width: picture.width,
                height: picture.height,
                triangles: picture.triangles,
                edges: picture.edges,
            }
        };
        let full = draw(args.size);
        let inline = match max_bytes {
            Some(budget) if full.png.len() as u64 > budget => {
                let mut last = (full.png.len(), full.width, full.height);
                let mut found = None;
                for _ in 1..BUDGET_ATTEMPTS {
                    let (bytes, width, height) = last;
                    let longest = width.max(height);
                    if longest <= BUDGET_MIN_SIDE {
                        break;
                    }
                    // Bytes go with the area: the side by the root of the
                    // ratio, with a margin.
                    let factor = ((budget as f64 / bytes as f64).sqrt() * 0.9).min(0.9);
                    let mut factor = factor;
                    if f64::from(longest) * factor < f64::from(BUDGET_MIN_SIDE) {
                        factor = f64::from(BUDGET_MIN_SIDE) / f64::from(longest);
                    }
                    let side = |s: u32| ((f64::from(s) * factor).round() as u32).max(1);
                    let encoded = draw([side(width), side(height)]);
                    last = (encoded.png.len(), encoded.width, encoded.height);
                    if encoded.png.len() as u64 <= budget {
                        found = Some(encoded);
                        break;
                    }
                }
                match found {
                    Some(encoded) => Inline::Smaller(encoded),
                    None => Inline::TooLarge {
                        bytes: last.0,
                        width: last.1,
                        height: last.2,
                    },
                }
            }
            _ => Inline::Full,
        };
        Ok(Preview {
            camera: plan.camera,
            warnings,
            bodies,
            full,
            inline,
            mesh_ms,
            raster_ms,
            encode_ms,
            overlay,
            overlay_ms,
        })
    }
}

/// The deviation (mm) a body is meshed within for a picture: a thousandth
/// of its box's diagonal, 0.01 to 0.5 mm.
fn display_deviation(diagonal: f64) -> f64 {
    (diagonal * 1e-3).clamp(0.01, 0.5)
}

fn elapsed(start: Instant) -> f64 {
    (start.elapsed().as_secs_f64() * 1e6).round() / 1e3
}

/// A body's faces placed by `transform`, into `mesh` (its colours and
/// flags set): normals turned with the placement, the triangles' turn
/// kept counter-clockwise from outside where the placement mirrors.
fn placed_mesh(faces: &FaceMeshes, transform: &Transform, mut mesh: Mesh) -> Mesh {
    let m = transform.linear;
    let determinant = transform.determinant();
    // The inverse transpose up to a positive factor: the cofactors.
    let cofactors = [cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1])];
    let sign = if determinant < 0.0 { -1.0 } else { 1.0 };
    let rigid_identity = *transform == Transform::IDENTITY;
    mesh.vertices = if rigid_identity {
        faces.vertices.clone()
    } else {
        faces
            .vertices
            .iter()
            .map(|&v| transform.apply_point(v))
            .collect()
    };
    mesh.normals = if rigid_identity {
        faces.normals.clone()
    } else {
        faces
            .normals
            .iter()
            .map(|&n| {
                let turned = scale(cofactors.map(|c| dot(c, n)), sign);
                unit(turned).unwrap_or(n)
            })
            .collect()
    };
    mesh.triangles = if determinant < 0.0 {
        faces.triangles.iter().map(|&[a, b, c]| [a, c, b]).collect()
    } else {
        faces.triangles.clone()
    };
    mesh.face_triangles = faces.face_triangles.clone();
    mesh
}
