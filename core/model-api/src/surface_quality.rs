// SPDX-License-Identifier: MIT
//! The `surface_analysis` query (mitcad#2517): a Zebra's stripes, a
//! Curvature Map's values, an Isocurve Analysis' curves or an Environment
//! Map's mesh of the bodies and faces a definition names, worked out in Rust
//! (`mitcad_model::surface_analysis`) on what the kernel gives of the faces
//! (`Kernel::surface_samples`, `Kernel::face_isocurves`). See
//! `commands.md`, "Surface quality analyses".
//!
//! As Draft Analysis' query (`print.rs`): the bodies are where Print
//! Analysis places them, a body or reference that is gone answers
//! `{"error": ...}`, the results are kept by the body's shape, placement,
//! the mesh tolerance and the settings, and a cancelled model job stops it.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::print::{PrintBody, PrintLocal, named, print_tolerance, result_key};
use mitcad_model::exchange::MeshTolerance;
use mitcad_model::kernel::Kernel;
use mitcad_model::surface_analysis::{
    self, CurvatureKind, automatic_limit, curvature_values, placed, placed_curve,
};
use mitcad_model::surface_quality::SurfaceSamples;
use mitcad_model::topo::FaceName;
use mitcad_model::{AnalysisDef, Document, SurfaceTarget};

fn yes() -> bool {
    true
}

/// `surface_analysis`: a surface analysis' definition as the document
/// keeps it, the mesh's deviation and whether the geometry is wanted.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SurfaceQuery {
    /// The definition with its `type` (`zebra`, `curvature`, `isocurve`,
    /// `environment`); what the `analyses` query adds to a kept one
    /// (`name`, `visible`, `error`, `stripe_direction`) is taken and left.
    analysis: Value,
    #[serde(default)]
    deviation: Option<f64>,
    /// Each body's triangles, values or curves; false answers the numbers
    /// only.
    #[serde(default = "yes")]
    geometry: bool,
}

/// The definition of a query, without what the `analyses` query adds.
fn definition(analysis: &Value) -> Result<AnalysisDef, ApiError> {
    let mut def = analysis.clone();
    if let Some(fields) = def.as_object_mut() {
        for key in ["name", "visible", "error", "stripe_direction"] {
            fields.remove(key);
        }
    }
    let def: AnalysisDef =
        serde_json::from_value(def).map_err(|e| ApiError(format!("the surface analysis: {e}")))?;
    if def.surface_inputs().is_none() {
        return Err(ApiError(
            "the surface analysis must be a zebra, curvature, isocurve or environment analysis"
                .to_owned(),
        ));
    }
    Ok(def)
}

/// Points or vectors as one flat list of numbers.
fn flat3(points: &[[f64; 3]]) -> Vec<f64> {
    points.iter().flatten().copied().collect()
}

/// The document's method of the surface quality analyses, as an extension
/// trait (mitcad#777).
pub(super) trait SurfaceOps<K: Kernel> {
    /// `surface_analysis`: per body the analysis' geometry and numbers,
    /// with the totals (`commands.md`, "Surface quality analyses").
    fn surface_analysis_json(&self, query: &SurfaceQuery) -> Result<Value, ApiError>;
}

/// The bodies of the targets as Print Analysis takes them, with their
/// faces.
type Bodies<'a, S> = Vec<(PrintBody<'a, S>, Vec<FaceName>)>;

/// The bodies of the targets, or why one is not there.
fn surface_bodies<'a, K: Kernel>(
    doc: &'a Document<K>,
    targets: &[SurfaceTarget],
) -> Result<Bodies<'a, K::Shape>, String> {
    let uids: Vec<_> = targets.iter().map(|t| t.body).collect();
    let bodies = doc.print_bodies(&uids)?;
    Ok(bodies
        .into_iter()
        .zip(targets)
        .map(|(body, target)| (body, target.faces.clone()))
        .collect())
}

/// A body's samples where it is placed, or why the kernel gave none.
fn placed_samples<K: Kernel>(
    doc: &Document<K>,
    body: &PrintBody<'_, K::Shape>,
    faces: &[FaceName],
    tolerance: &MeshTolerance,
) -> Result<SurfaceSamples, String> {
    let samples = doc
        .kernel()
        .surface_samples(body.shape, faces, tolerance)
        .map_err(|e| format!("{}: {e}", body.name))?;
    Ok(placed(&samples, &body.placement))
}

/// A body's result, from the cache or worked out by `work` and kept; the
/// inner error is why it could not be worked out.
fn surface_result<K: Kernel>(
    doc: &Document<K>,
    tag: &str,
    body: &PrintBody<'_, K::Shape>,
    faces: &[FaceName],
    tolerance: &MeshTolerance,
    settings: &Value,
    work: impl FnOnce() -> Result<Value, String>,
) -> Result<Result<Arc<Value>, String>, ApiError> {
    let mut settings = settings.clone();
    settings["faces"] = json!(faces.iter().map(ToString::to_string).collect::<Vec<_>>());
    let key = result_key(tag, body, tolerance, &settings);
    if let Some(value) = doc.print_cache().result(key) {
        return Ok(Ok(value));
    }
    doc.print_stop()?;
    let value = match work() {
        Ok(value) => Arc::new(value),
        Err(error) => return Ok(Err(error)),
    };
    doc.print_stop()?;
    doc.print_cache().keep_result(key, Arc::clone(&value));
    Ok(Ok(value))
}

/// A per-body answer with its uid and name, its geometry left out when it
/// is not wanted.
fn body_answer<S>(value: &Value, body: &PrintBody<'_, S>, geometry: bool, keys: &[&str]) -> Value {
    let mut value = named(value, body);
    if !geometry && let Some(fields) = value.as_object_mut() {
        for key in keys {
            fields.remove(*key);
        }
    }
    value
}

/// The smaller and the larger of the bodies' smallest and largest values,
/// with where they are.
#[derive(Default)]
struct Extremes {
    min: Option<(f64, Value)>,
    max: Option<(f64, Value)>,
}

impl Extremes {
    fn take(&mut self, body: &Value) {
        if let (Some(v), Some(at)) = (body["min"].as_f64(), body.get("min_at"))
            && self.min.as_ref().is_none_or(|(m, _)| v < *m)
        {
            self.min = Some((v, at.clone()));
        }
        if let (Some(v), Some(at)) = (body["max"].as_f64(), body.get("max_at"))
            && self.max.as_ref().is_none_or(|(m, _)| v > *m)
        {
            self.max = Some((v, at.clone()));
        }
    }

    fn write(&self, answer: &mut Value) {
        if let Some((v, at)) = &self.min {
            answer["min"] = json!(v);
            answer["min_at"] = at.clone();
        }
        if let Some((v, at)) = &self.max {
            answer["max"] = json!(v);
            answer["max_at"] = at.clone();
        }
    }
}

impl<K: Kernel> SurfaceOps<K> for Document<K> {
    fn surface_analysis_json(&self, query: &SurfaceQuery) -> Result<Value, ApiError> {
        let def = definition(&query.analysis)?;
        let tolerance = print_tolerance(query.deviation)?;
        let (bodies, faces) = def.surface_inputs().expect("checked in definition");
        let targets = match self.surface_targets(bodies, faces) {
            Ok(targets) => targets,
            Err(error) => return Ok(json!({"error": error})),
        };
        let bodies = match surface_bodies(self, &targets) {
            Ok(bodies) => bodies,
            Err(error) => return Ok(json!({"error": error})),
        };
        match &def {
            AnalysisDef::Zebra(zebra) => {
                if let Err(error) = mitcad_model::check_zebra_settings(zebra.stripes, zebra.width) {
                    return Err(ApiError(error));
                }
                let direction = match self.zebra_direction(zebra) {
                    Ok(direction) => direction,
                    Err(error) => return Ok(json!({"error": error})),
                };
                let settings =
                    json!({"direction": direction, "stripes": zebra.stripes, "width": zebra.width});
                let mut results = Vec::with_capacity(bodies.len());
                let (mut dark, mut light, mut dark_count, mut light_count) = (0.0, 0.0, 0, 0);
                for (body, faces) in &bodies {
                    let value = surface_result(
                        self,
                        "zebra-analysis-v1",
                        body,
                        faces,
                        &tolerance,
                        &settings,
                        || {
                            let samples = placed_samples(self, body, faces, &tolerance)?;
                            let stripes = surface_analysis::zebra(
                                &samples,
                                direction,
                                zebra.stripes,
                                zebra.width,
                            );
                            Ok(json!({
                                "triangles": {"dark": stripes.dark, "light": stripes.light},
                                "dark_triangles": stripes.dark.len(),
                                "light_triangles": stripes.light.len(),
                                "dark_area": stripes.dark_area,
                                "light_area": stripes.light_area,
                                "faces": samples.face_triangles.len(),
                            }))
                        },
                    )?;
                    let value = match value {
                        Ok(value) => value,
                        Err(error) => return Ok(json!({"error": error})),
                    };
                    dark += value["dark_area"].as_f64().unwrap_or_default();
                    light += value["light_area"].as_f64().unwrap_or_default();
                    dark_count += value["dark_triangles"].as_u64().unwrap_or_default();
                    light_count += value["light_triangles"].as_u64().unwrap_or_default();
                    results.push(body_answer(&value, body, query.geometry, &["triangles"]));
                }
                Ok(json!({
                    "type": "zebra",
                    "direction": direction,
                    "stripes": zebra.stripes,
                    "width": zebra.width,
                    "bodies": results,
                    "dark_area": dark,
                    "light_area": light,
                    "dark_triangles": dark_count,
                    "light_triangles": light_count,
                }))
            }
            AnalysisDef::Curvature(curvature) => {
                mitcad_model::check_limit(curvature.limit).map_err(ApiError)?;
                let kind = curvature.kind;
                let settings = json!({"kind": kind.name()});
                let mut results = Vec::with_capacity(bodies.len());
                let mut extremes = Extremes::default();
                let mut triangles = 0;
                for (body, faces) in &bodies {
                    let value = surface_result(
                        self,
                        "curvature-map-v1",
                        body,
                        faces,
                        &tolerance,
                        &settings,
                        || {
                            let samples = placed_samples(self, body, faces, &tolerance)?;
                            Ok(curvature_json(&samples, kind))
                        },
                    )?;
                    let value = match value {
                        Ok(value) => value,
                        Err(error) => return Ok(json!({"error": error})),
                    };
                    extremes.take(&value);
                    triangles += value["triangles"].as_u64().unwrap_or_default();
                    results.push(body_answer(&value, body, query.geometry, &["mesh"]));
                }
                let limit = curvature.limit.unwrap_or_else(|| {
                    automatic_limit(
                        extremes.min.as_ref().map(|m| m.0),
                        extremes.max.as_ref().map(|m| m.0),
                    )
                });
                let mut answer = json!({
                    "type": "curvature",
                    "kind": kind.name(),
                    "unit": kind.unit(),
                    "limit": limit,
                    "automatic": curvature.limit.is_none(),
                    "bodies": results,
                    "triangles": triangles,
                });
                extremes.write(&mut answer);
                Ok(answer)
            }
            AnalysisDef::Isocurve(isocurve) => {
                mitcad_model::check_isocurve_count(isocurve.count).map_err(ApiError)?;
                mitcad_model::check_limit(isocurve.limit).map_err(ApiError)?;
                let settings = json!({"count": isocurve.count});
                let mut results = Vec::with_capacity(bodies.len());
                let mut extremes = Extremes::default();
                let (mut curves, mut points) = (0, 0);
                for (body, faces) in &bodies {
                    let value = surface_result(
                        self,
                        "isocurve-analysis-v1",
                        body,
                        faces,
                        &tolerance,
                        &settings,
                        || {
                            let found = self
                                .kernel()
                                .face_isocurves(
                                    body.shape,
                                    faces,
                                    isocurve.count,
                                    tolerance.deviation,
                                )
                                .map_err(|e| format!("{}: {e}", body.name))?;
                            Ok(isocurves_json(&found, body))
                        },
                    )?;
                    let value = match value {
                        Ok(value) => value,
                        Err(error) => return Ok(json!({"error": error})),
                    };
                    extremes.take(&value);
                    curves += value["count"].as_u64().unwrap_or_default();
                    points += value["points"].as_u64().unwrap_or_default();
                    results.push(body_answer(&value, body, query.geometry, &["curves"]));
                }
                let mut answer = json!({
                    "type": "isocurve",
                    "count": isocurve.count,
                    "curvature": isocurve.curvature,
                    "bodies": results,
                    "curves": curves,
                    "points": points,
                });
                extremes.write(&mut answer);
                if isocurve.curvature {
                    answer["limit"] = json!(isocurve.limit.unwrap_or_else(|| {
                        automatic_limit(
                            extremes.min.as_ref().map(|m| m.0),
                            extremes.max.as_ref().map(|m| m.0),
                        )
                    }));
                    answer["automatic"] = json!(isocurve.limit.is_none());
                }
                Ok(answer)
            }
            AnalysisDef::Environment(environment) => {
                let settings = json!({"environment": true});
                let mut results = Vec::with_capacity(bodies.len());
                let mut triangles = 0;
                for (body, faces) in &bodies {
                    let value = surface_result(
                        self,
                        "environment-map-v1",
                        body,
                        faces,
                        &tolerance,
                        &settings,
                        || {
                            let samples = placed_samples(self, body, faces, &tolerance)?;
                            Ok(json!({
                                "mesh": {
                                    "positions": flat3(&samples.vertices),
                                    "normals": flat3(&samples.normals),
                                    "indices": samples.triangles.iter().flatten().collect::<Vec<_>>(),
                                },
                                "triangles": samples.triangles.len(),
                                "faces": samples.face_triangles.len(),
                            }))
                        },
                    )?;
                    let value = match value {
                        Ok(value) => value,
                        Err(error) => return Ok(json!({"error": error})),
                    };
                    triangles += value["triangles"].as_u64().unwrap_or_default();
                    results.push(body_answer(&value, body, query.geometry, &["mesh"]));
                }
                Ok(json!({
                    "type": "environment",
                    "image": environment.image,
                    "bodies": results,
                    "triangles": triangles,
                }))
            }
            _ => Err(ApiError("not a surface analysis".to_owned())),
        }
    }
}

/// A body's Curvature Map: its mesh with a value per vertex (null where
/// there is none), its smallest and largest value and where they are.
fn curvature_json(samples: &SurfaceSamples, kind: CurvatureKind) -> Value {
    let values = curvature_values(samples, kind);
    let mut value = json!({
        "mesh": {
            "positions": flat3(&samples.vertices),
            "indices": samples.triangles.iter().flatten().collect::<Vec<_>>(),
            "values": values.values,
        },
        "triangles": samples.triangles.len(),
        "faces": samples.face_triangles.len(),
    });
    if let Some((min, at)) = values.min {
        value["min"] = json!(min);
        value["min_at"] = json!(at);
    }
    if let Some((max, at)) = values.max {
        value["max"] = json!(max);
        value["max_at"] = json!(at);
    }
    value
}

/// A body's iso curves where it is placed: each its face (the index among
/// those asked), along `u` or `v`, where (0 to 1), its points and normals
/// (flat) and its normal curvature at each point (null where there is
/// none); with their count, their points' and the smallest and largest
/// curvature and where they are.
fn isocurves_json<S>(
    curves: &[mitcad_model::surface_quality::IsoCurve],
    body: &PrintBody<'_, S>,
) -> Value {
    // The smallest and the largest curvature with where it is.
    type Extreme = Option<(f64, [f64; 3])>;
    let mut extremes: (Extreme, Extreme) = (None, None);
    let mut points = 0usize;
    let list: Vec<Value> = curves
        .iter()
        .map(|curve| {
            let curve = placed_curve(curve, &body.placement);
            points += curve.points.len();
            let curvatures: Vec<Option<f64>> = curve
                .curvatures
                .iter()
                .map(|k| k.is_finite().then_some(if *k == 0.0 { 0.0 } else { *k }))
                .collect();
            for (k, p) in curvatures.iter().zip(&curve.points) {
                let Some(k) = *k else { continue };
                if extremes.0.is_none_or(|(m, _)| k < m) {
                    extremes.0 = Some((k, *p));
                }
                if extremes.1.is_none_or(|(m, _)| k > m) {
                    extremes.1 = Some((k, *p));
                }
            }
            json!({
                "face": curve.face,
                "along": if curve.along_u { "u" } else { "v" },
                "at": curve.at,
                "points": flat3(&curve.points),
                "normals": flat3(&curve.normals),
                "curvatures": curvatures,
            })
        })
        .collect();
    let mut value = json!({
        "count": list.len(),
        "points": points,
        "curves": list,
    });
    if let Some((min, at)) = extremes.0 {
        value["min"] = json!(min);
        value["min_at"] = json!(at);
    }
    if let Some((max, at)) = extremes.1 {
        value["max"] = json!(max);
        value["max_at"] = json!(at);
    }
    value
}
