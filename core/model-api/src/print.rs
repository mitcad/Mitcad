// SPDX-License-Identifier: MIT
//! Print Analysis queries (mitcad#620): `print_overhangs`,
//! `print_thickness` and `print_estimate` on bodies at the timeline
//! marker, worked out in Rust (`mitcad_model::print_analysis`) on the meshes and
//! measures the kernel already gives (`Kernel::triangle_mesh`,
//! `Kernel::physical_properties`). See `commands.md`, "Print analysis".
//!
//! A body that is gone, a bed face that cannot be found or a mesh the
//! kernel cannot make answers `{"error": ...}` (the application shows the
//! analysis red with the reason); settings out of range are refused as
//! other queries' inputs are. Run inside a model job, the overhangs and the
//! walls stop when the job is cancelled ("the computation was cancelled").
//! The meshes and results are kept by the body's shape version, its
//! placement, the mesh tolerance and the settings, so asking again
//! computes nothing.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::analysis;
use mitcad_model::datum::Vec3;
use mitcad_model::exchange::{MeshTolerance, Refinement, TriangleMesh};
use mitcad_model::features::geom_ref::GeomRef;
use mitcad_model::ids::BodyUid;
use mitcad_model::internal::document::{check_minimum_wall, check_overhang_angle, print_material};
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::kernel::Kernel;
use mitcad_model::print_analysis::{self, Bed, EstimateSettings};
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ModelError};

fn default_angle() -> f64 {
    print_analysis::DEFAULT_ANGLE
}

fn default_minimum() -> f64 {
    print_analysis::DEFAULT_MINIMUM
}

fn default_line_width() -> f64 {
    print_analysis::DEFAULT_LINE_WIDTH
}

fn default_walls() -> u32 {
    print_analysis::DEFAULT_WALLS
}

fn default_layer_height() -> f64 {
    print_analysis::DEFAULT_LAYER_HEIGHT
}

fn default_top_bottom_layers() -> u32 {
    print_analysis::DEFAULT_TOP_BOTTOM_LAYERS
}

fn default_infill() -> f64 {
    print_analysis::DEFAULT_INFILL
}

fn default_filament_diameter() -> f64 {
    print_analysis::DEFAULT_FILAMENT_DIAMETER
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OverhangsQuery {
    /// Empty for all bodies at the marker.
    #[serde(default)]
    bodies: Vec<BodyUid>,
    /// Towards the bed (any length); the design's −Z when neither this
    /// nor `bed_face` is given.
    #[serde(default)]
    down: Option<Vec3>,
    /// A planar face lying on the bed: its outward normal is down.
    #[serde(default)]
    bed_face: Option<GeomRef>,
    /// Degrees from the vertical.
    #[serde(default = "default_angle")]
    angle: f64,
    /// The mesh's deviation, mm; the 3MF export's default when left out.
    #[serde(default)]
    deviation: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThicknessQuery {
    #[serde(default)]
    bodies: Vec<BodyUid>,
    /// The thinnest wall a printer makes, mm.
    #[serde(default = "default_minimum")]
    minimum: f64,
    #[serde(default)]
    deviation: Option<f64>,
    /// The thin triangles' corners per body; false answers the summary
    /// only (mitcad#689: their count stays in `thin_triangles`).
    #[serde(default = "default_triangles")]
    triangles: bool,
    /// At most this many triangles per body; the rest are left out and
    /// the body says `truncated`.
    #[serde(default)]
    max_triangles: Option<usize>,
}

fn default_triangles() -> bool {
    true
}

/// The fields of a print analysis' `estimate` with the bodies; `enabled`
/// is taken and ignored, so that a definition's settings can be passed as
/// they are.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EstimateQuery {
    #[serde(default)]
    bodies: Vec<BodyUid>,
    #[serde(default)]
    #[allow(dead_code)]
    enabled: Option<bool>,
    /// A material id or name; each body's own material when left out or
    /// null, PLA for a body without one.
    #[serde(default)]
    material: Option<String>,
    #[serde(default = "default_line_width")]
    line_width: f64,
    #[serde(default = "default_walls")]
    walls: u32,
    #[serde(default = "default_layer_height")]
    layer_height: f64,
    #[serde(default = "default_top_bottom_layers")]
    top_bottom_layers: u32,
    #[serde(default = "default_infill")]
    infill: f64,
    #[serde(default = "default_filament_diameter")]
    filament_diameter: f64,
    #[serde(default)]
    price_per_kg: Option<f64>,
}

impl EstimateQuery {
    fn settings(&self) -> EstimateSettings {
        EstimateSettings {
            line_width: self.line_width,
            walls: self.walls,
            layer_height: self.layer_height,
            top_bottom_layers: self.top_bottom_layers,
            infill: self.infill,
            filament_diameter: self.filament_diameter,
            price_per_kg: self.price_per_kg,
        }
    }
}

/// A body to analyse: its shape, the identity of the shape and where it
/// is printed from.
struct PrintBody<'a, S> {
    uid: BodyUid,
    name: String,
    shape: &'a S,
    version: u128,
    placement: Transform,
}

/// The mesh tolerance: the 3MF export's default (the mesh a slicer gets),
/// with the deviation given.
fn print_tolerance(deviation: Option<f64>) -> Result<MeshTolerance, ApiError> {
    let preset = Refinement::default().tolerance();
    match deviation {
        None => Ok(preset),
        Some(d) if d > 0.0 && d.is_finite() => Ok(MeshTolerance {
            deviation: d,
            ..preset
        }),
        Some(d) => Err(ApiError(format!(
            "the deviation must be greater than zero, got {d}"
        ))),
    }
}

/// The cache key of a body's result: what it is of, the body's shape and
/// placement, the mesh tolerance and the settings (shared with
/// `model_render`'s overlay, mitcad#1488).
fn result_key<S>(
    tag: &str,
    body: &PrintBody<'_, S>,
    tolerance: &MeshTolerance,
    settings: &Value,
) -> u128 {
    print_analysis::result_key(
        tag,
        body.uid,
        body.version,
        &body.placement,
        tolerance,
        settings,
    )
}

/// Sets the body's uid and name (the name may change without the shape).
fn named<S>(value: &Value, body: &PrintBody<'_, S>) -> Value {
    let mut value = value.clone();
    value["body"] = json!(body.uid);
    value["name"] = json!(body.name);
    value
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait PrintOps<K: Kernel> {
    /// `print_overhangs`: per body the area, the overhang area, its share
    /// and the overhanging triangles in design coordinates.
    fn print_overhangs_json(&self, query: &OverhangsQuery) -> Result<Value, ApiError>;

    /// `print_thickness`: per body the thinnest wall and where it is, the
    /// triangles of walls under the minimum (whole, or the thin pieces of a
    /// triangle thin in part, mitcad#767) in design coordinates and how
    /// many regions they make.
    fn print_thickness_json(&self, query: &ThicknessQuery) -> Result<Value, ApiError>;

    /// `print_estimate`: per body and in total the volume, area, shell,
    /// infill and filament, the filament's mass and length and its cost.
    fn print_estimate_json(&self, query: &EstimateQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> PrintOps<K> for Document<K> {
    fn print_overhangs_json(&self, query: &OverhangsQuery) -> Result<Value, ApiError> {
        check_overhang_angle(query.angle).map_err(ApiError)?;
        let tolerance = print_tolerance(query.deviation)?;
        if query.down.is_some() && query.bed_face.is_some() {
            return Err(ApiError(
                "give the print direction either as down or as a bed face".to_owned(),
            ));
        }
        let given = match query.down {
            Some(down) => Some(mitcad_vec3::unit(down).ok_or_else(|| {
                ApiError("the down direction must be a nonzero vector".to_owned())
            })?),
            None => None,
        };
        let bodies = match self.print_bodies(&query.bodies) {
            Ok(bodies) => bodies,
            Err(error) => return Ok(json!({"error": error})),
        };
        let (down, bed) = match &query.bed_face {
            Some(face) => match self.print_bed(face) {
                Ok((down, point)) => (down, Bed::Through(point)),
                Err(error) => return Ok(json!({"error": error})),
            },
            None => (given.unwrap_or([0.0, 0.0, -1.0]), Bed::Lowest),
        };
        let bed_point = match bed {
            Bed::Lowest => Value::Null,
            Bed::Through(point) => json!(point),
        };
        let settings = json!({"down": down, "bed": bed_point, "angle": query.angle});
        // Faces within the mesh's deviation of the bed lie on it.
        let on_bed = tolerance.deviation.max(1e-6);
        let mut results = Vec::with_capacity(bodies.len());
        let (mut area, mut overhang_area) = (0.0, 0.0);
        for body in &bodies {
            self.print_stop()?;
            let key = result_key("print-overhangs-v1", body, &tolerance, &settings);
            let value = match self.print_cache().result(key) {
                Some(value) => value,
                None => {
                    let mesh = match self.print_mesh(body, &tolerance) {
                        Ok(mesh) => mesh,
                        Err(error) => return Ok(json!({"error": error})),
                    };
                    self.print_stop()?;
                    let mesh = print_analysis::placed(&mesh, &body.placement);
                    let found = print_analysis::overhangs(&mesh, down, query.angle, bed, on_bed);
                    let triangles: Vec<[f64; 9]> = found
                        .triangles
                        .iter()
                        .map(|t| print_analysis::flat(&mesh, *t))
                        .collect();
                    let value = Arc::new(json!({
                        "area": found.area,
                        "overhang_area": found.overhang_area,
                        "share": found.share(),
                        "triangles": triangles,
                    }));
                    self.print_cache().keep_result(key, Arc::clone(&value));
                    value
                }
            };
            area += value["area"].as_f64().unwrap_or_default();
            overhang_area += value["overhang_area"].as_f64().unwrap_or_default();
            results.push(named(&value, body));
        }
        let share = if area > 0.0 {
            overhang_area / area
        } else {
            0.0
        };
        Ok(json!({
            "down": down,
            "angle": query.angle,
            "bodies": results,
            "overhang_area": overhang_area,
            "area": area,
            "share": share,
        }))
    }

    fn print_thickness_json(&self, query: &ThicknessQuery) -> Result<Value, ApiError> {
        check_minimum_wall(query.minimum).map_err(ApiError)?;
        let tolerance = print_tolerance(query.deviation)?;
        let bodies = match self.print_bodies(&query.bodies) {
            Ok(bodies) => bodies,
            Err(error) => return Ok(json!({"error": error})),
        };
        let settings = json!({"minimum": query.minimum});
        let monitor = self.monitor().cloned();
        let cancelled = move || monitor.as_ref().is_some_and(|m| m.is_cancelled());
        let mut results = Vec::with_capacity(bodies.len());
        let mut thinnest: Option<(f64, Value)> = None;
        let mut regions = 0;
        let mut thin_triangles = 0;
        let mut truncated = false;
        for body in &bodies {
            self.print_stop()?;
            let key = result_key(print_analysis::THICKNESS_TAG, body, &tolerance, &settings);
            let value = match self.print_cache().result(key) {
                Some(value) => value,
                None => {
                    let mesh = match self.print_mesh(body, &tolerance) {
                        Ok(mesh) => mesh,
                        Err(error) => return Ok(json!({"error": error})),
                    };
                    self.print_stop()?;
                    let mesh = print_analysis::placed(&mesh, &body.placement);
                    let found = print_analysis::thickness(&mesh, query.minimum, &cancelled)
                        .map_err(|_| ApiError::from(ModelError::Cancelled))?;
                    // Where the walls are thin: whole triangles, or the thin
                    // pieces of those thin in part (mitcad#767).
                    let value = Arc::new(print_analysis::thickness_value(&found));
                    self.print_cache().keep_result(key, Arc::clone(&value));
                    value
                }
            };
            if let Some(wall) = value["thinnest"].as_f64()
                && thinnest.as_ref().is_none_or(|(best, _)| wall < *best)
            {
                thinnest = Some((wall, value["at"].clone()));
            }
            regions += value["regions"].as_u64().unwrap_or_default();
            // The summary, or the triangles up to the cap (mitcad#689).
            let mut result = named(&value, body);
            let count = value["triangles"].as_array().map_or(0, Vec::len);
            thin_triangles += count;
            result["thin_triangles"] = json!(count);
            if !query.triangles {
                if let Some(fields) = result.as_object_mut() {
                    fields.remove("triangles");
                }
            } else if let Some(cap) = query.max_triangles
                && count > cap
                && let Some(list) = result["triangles"].as_array_mut()
            {
                list.truncate(cap);
                result["truncated"] = json!(true);
                truncated = true;
            }
            results.push(result);
        }
        let (wall, at) = match thinnest {
            Some((wall, at)) => (json!(wall), at),
            None => (Value::Null, Value::Null),
        };
        let mut answer = json!({
            "minimum": query.minimum,
            "bodies": results,
            "thinnest": wall,
            "at": at,
            "regions": regions,
            "thin_triangles": thin_triangles,
        });
        if truncated {
            answer["truncated"] = json!(true);
        }
        Ok(answer)
    }

    fn print_estimate_json(&self, query: &EstimateQuery) -> Result<Value, ApiError> {
        let settings = query.settings();
        settings.check().map_err(ApiError)?;
        let chosen = match &query.material {
            Some(material) => Some(print_material(material).map_err(ApiError)?),
            None => None,
        };
        let bodies = match self.print_bodies(&query.bodies) {
            Ok(bodies) => bodies,
            Err(error) => return Ok(json!({"error": error})),
        };
        let mut results = Vec::with_capacity(bodies.len());
        let mut total = print_analysis::Estimate::default();
        let mut materials: Vec<Made> = Vec::new();
        for body in &bodies {
            self.print_stop()?;
            // The body's own material or its component's default
            // (mitcad#640), else PLA.
            let material = match chosen {
                Some(m) => Made::of_library(m),
                None => match self.body_material(body.uid, None) {
                    (_, mitcad_model::MaterialSource::Default) => Made::of_library(
                        analysis::material(print_analysis::DEFAULT_MATERIAL)
                            .expect("PLA is a material of the library"),
                    ),
                    (m, _) => Made {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        density: m.density,
                    },
                },
            };
            let (volume, area) = match self.print_measures(body) {
                Ok(measures) => measures,
                Err(error) => return Ok(json!({"error": error})),
            };
            let estimate = print_analysis::estimate(volume, area, material.density, &settings);
            total.volume += estimate.volume;
            total.area += estimate.area;
            total.shell_volume += estimate.shell_volume;
            total.infill_volume += estimate.infill_volume;
            total.filament_volume += estimate.filament_volume;
            total.mass += estimate.mass;
            total.length += estimate.length;
            total.cost = match (total.cost, estimate.cost) {
                (Some(a), Some(b)) => Some(a + b),
                (None, b) => b,
                (a, None) => a,
            };
            let mut value = estimate_json(&estimate, &material);
            value["body"] = json!(body.uid);
            value["name"] = json!(body.name);
            results.push(value);
            if !materials.iter().any(|m| m.id == material.id) {
                materials.push(material);
            }
        }
        if results.is_empty() {
            total.cost = settings.price_per_kg.map(|_| 0.0);
        }
        // One material, or the names of several with the density of the
        // filament they make together.
        let (name, id, density) = match materials.as_slice() {
            [one] => (one.name.clone(), json!(one.id), one.density),
            [] => {
                let pla = analysis::material(print_analysis::DEFAULT_MATERIAL)
                    .expect("PLA is a material of the library");
                (pla.name.to_owned(), json!(pla.id), pla.density)
            }
            several => {
                let names: Vec<&str> = several.iter().map(|m| m.name.as_str()).collect();
                let density = if total.filament_volume > 0.0 {
                    total.mass / total.filament_volume * 1000.0
                } else {
                    0.0
                };
                (names.join(", "), Value::Null, density)
            }
        };
        let mut value = json!({
            "volume": total.volume,
            "area": total.area,
            "shell_volume": total.shell_volume,
            "infill_volume": total.infill_volume,
            "filament_volume": total.filament_volume,
            "mass": total.mass,
            "length": total.length,
            "cost": total.cost,
            "material": name,
            "material_id": id,
            "density": density,
        });
        value["bodies"] = json!(results);
        Ok(value)
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait PrintLocal<K: Kernel> {
    /// The bodies (all at the marker when none are named), or why one is
    /// not there.
    fn print_bodies(&self, uids: &[BodyUid]) -> Result<Vec<PrintBody<'_, K::Shape>>, String>;

    /// Stops a query that runs in a cancelled model job.
    fn print_stop(&self) -> Result<(), ApiError>;

    /// The body's triangle mesh in its own coordinates, kept by its shape.
    fn print_mesh(
        &self,
        body: &PrintBody<'_, K::Shape>,
        tolerance: &MeshTolerance,
    ) -> Result<Arc<TriangleMesh>, String>;

    /// The body's volume and area (`properties`), kept by its shape.
    fn print_measures(&self, body: &PrintBody<'_, K::Shape>) -> Result<(f64, f64), String>;
}

impl<K: Kernel> PrintLocal<K> for Document<K> {
    fn print_bodies(&self, uids: &[BodyUid]) -> Result<Vec<PrintBody<'_, K::Shape>>, String> {
        let uids: Vec<BodyUid> = if uids.is_empty() {
            self.bodies().iter().map(|b| b.uid).collect()
        } else {
            uids.to_vec()
        };
        let placements = self.print_placements(&uids);
        uids.iter()
            .zip(placements)
            .map(|(uid, placement)| {
                let shape = self
                    .body_shape(*uid)
                    .ok_or_else(|| format!("body {uid} does not exist at the timeline marker"))?;
                Ok(PrintBody {
                    uid: *uid,
                    name: self.body_name(*uid),
                    shape,
                    version: self.body_version(*uid).unwrap_or_default(),
                    placement,
                })
            })
            .collect()
    }

    fn print_stop(&self) -> Result<(), ApiError> {
        if self.monitor().is_some_and(|m| m.is_cancelled()) {
            Err(ModelError::Cancelled.into())
        } else {
            Ok(())
        }
    }

    fn print_mesh(
        &self,
        body: &PrintBody<'_, K::Shape>,
        tolerance: &MeshTolerance,
    ) -> Result<Arc<TriangleMesh>, String> {
        let key = print_analysis::mesh_key(body.uid, body.version, tolerance);
        if let Some(mesh) = self.print_cache().mesh(key) {
            return Ok(mesh);
        }
        let mesh = self
            .kernel()
            .triangle_mesh(body.shape, tolerance)
            .map_err(|e| format!("{}: {e}", body.name))?;
        let mesh = Arc::new(mesh);
        self.print_cache().keep_mesh(key, Arc::clone(&mesh));
        Ok(mesh)
    }

    fn print_measures(&self, body: &PrintBody<'_, K::Shape>) -> Result<(f64, f64), String> {
        let mut fp = Fingerprint::new("print-measures-v1");
        fp.str(&body.uid.to_string()).u128(body.version);
        let key = fp.finish();
        if let Some(value) = self.print_cache().result(key)
            && let (Some(volume), Some(area)) = (value["volume"].as_f64(), value["area"].as_f64())
        {
            return Ok((volume, area));
        }
        // Volume and area do not depend on the density.
        let properties = self
            .kernel()
            .physical_properties(body.shape, 1.0)
            .map_err(|e| format!("{}: {e}", body.name))?;
        let value = json!({"volume": properties.volume, "area": properties.area});
        self.print_cache().keep_result(key, Arc::new(value));
        Ok((properties.volume, properties.area))
    }
}

/// The material an estimate is for: a built-in's or the document's.
struct Made {
    id: String,
    name: String,
    /// g/cm³.
    density: f64,
}

impl Made {
    fn of_library(material: &analysis::Material) -> Self {
        Self {
            id: material.id.to_owned(),
            name: material.name.to_owned(),
            density: material.density,
        }
    }
}

fn estimate_json(estimate: &print_analysis::Estimate, material: &Made) -> Value {
    json!({
        "volume": estimate.volume,
        "area": estimate.area,
        "shell_volume": estimate.shell_volume,
        "infill_volume": estimate.infill_volume,
        "filament_volume": estimate.filament_volume,
        "mass": estimate.mass,
        "length": estimate.length,
        "cost": estimate.cost,
        "material": material.name,
        "material_id": material.id,
        "density": material.density,
    })
}
