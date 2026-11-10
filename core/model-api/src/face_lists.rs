// SPDX-License-Identifier: MIT
//! The `faces` and `edges` queries (mitcad#714): each face's names, surface
//! type and area, each edge's name, curve type and length; with
//! `"geometry": true` also each item's geometry and `bbox`, and with a
//! filter, `offset` or `limit` the matching items in a page with their
//! total count, for bodies of hundreds of faces. See commands.md "Faces
//! and edges with geometry".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry, Vec3};
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::{BoundingBox, Kernel, KernelError};

/// A face's normal within this angle of a `normal` filter's (radians) when
/// the query gives no `normal_tolerance`.
const NORMAL_TOLERANCE: f64 = 1e-6;
/// A circular edge matches a `radius` filter within this many mm.
pub(super) const RADIUS_TOLERANCE: f64 = 1e-4;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FacesQuery {
    body: BodyUid,
    /// Each face's surface geometry and box.
    #[serde(default)]
    geometry: bool,
    /// Only faces of this surface type (`plane`, `cylinder`, ...).
    #[serde(default)]
    surface: Option<String>,
    /// Only faces of at least (at most) this area, mm².
    #[serde(default)]
    min_area: Option<f64>,
    #[serde(default)]
    max_area: Option<f64>,
    /// Only planar faces whose outward normal is this direction within
    /// `normal_tolerance` (radians).
    #[serde(default)]
    normal: Option<Vec3>,
    #[serde(default)]
    normal_tolerance: Option<f64>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EdgesQuery {
    body: BodyUid,
    /// Each edge's curve geometry and box.
    #[serde(default)]
    geometry: bool,
    /// Only edges of this curve type (`line`, `circle`, ...).
    #[serde(default)]
    curve: Option<String>,
    /// Only edges of at least (at most) this length, mm.
    #[serde(default)]
    min_length: Option<f64>,
    #[serde(default)]
    max_length: Option<f64>,
    /// Only circular edges of this radius, mm, within
    /// [`RADIUS_TOLERANCE`] (mitcad#1587).
    #[serde(default)]
    radius: Option<f64>,
    /// Only edges between faces of these surface types, in any order
    /// (`["cylinder", "sphere"]`); one type: at least one face of it
    /// (mitcad#1587).
    #[serde(default)]
    between: Option<Vec<String>>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

impl FacesQuery {
    /// A filter or a page: the answer is an object with the total.
    fn paged(&self) -> bool {
        self.surface.is_some()
            || self.min_area.is_some()
            || self.max_area.is_some()
            || self.normal.is_some()
            || self.offset.is_some()
            || self.limit.is_some()
    }
}

impl EdgesQuery {
    fn paged(&self) -> bool {
        self.curve.is_some()
            || self.radius.is_some()
            || self.between.is_some()
            || self.min_length.is_some()
            || self.max_length.is_some()
            || self.offset.is_some()
            || self.limit.is_some()
    }
}

fn kernel_error(e: KernelError) -> ApiError {
    ApiError(e.to_string())
}

/// + 0.0 turns -0.0 into 0.0.
fn tidy(v: Vec3) -> Vec3 {
    v.map(|x| x + 0.0)
}

fn bbox_json(found: Option<BoundingBox>) -> Value {
    match found {
        Some(b) => json!({"min": tidy(b.min), "max": tidy(b.max)}),
        None => Value::Null,
    }
}

/// The page of `matched` that `offset` and `limit` choose.
fn page(matched: &[usize], offset: Option<usize>, limit: Option<usize>) -> &[usize] {
    let start = offset.unwrap_or(0).min(matched.len());
    let end = limit.map_or(matched.len(), |n| {
        start.saturating_add(n).min(matched.len())
    });
    &matched[start..end]
}

/// The answer: the items as an array, or with a filter or a page an object
/// with the total count of matching items and the offset.
fn answer(paged: bool, key: &str, items: Vec<Value>, total: usize, offset: Option<usize>) -> Value {
    if !paged {
        return Value::Array(items);
    }
    let mut value = json!({"total": total, "offset": offset.unwrap_or(0)});
    value[key] = Value::Array(items);
    value
}

fn check_range(what: &str, min: Option<f64>, max: Option<f64>) -> Result<(), ApiError> {
    for value in [min, max].into_iter().flatten() {
        if !value.is_finite() {
            return Err(ApiError(format!(
                "min_{what} and max_{what} must be finite numbers"
            )));
        }
    }
    Ok(())
}

/// A surface's fields in a face's entry: a plane's `origin` (its point
/// nearest to the model origin) and outward `normal`; a cylinder's or
/// cone's `origin` (the axis point nearest to the face's centre), `axis`
/// and `radius` (a cone's at `origin`, with `half_angle`); a sphere's
/// `center` and `radius`; a torus's `center`, `axis`, `major_radius` and
/// `minor_radius`. Nothing for other surfaces.
fn add_surface_json(value: &mut Value, surface: &SurfaceGeometry) {
    match surface {
        SurfaceGeometry::Plane { origin, normal } => {
            value["origin"] = json!(tidy(*origin));
            value["normal"] = json!(tidy(*normal));
        }
        SurfaceGeometry::Cylinder {
            origin,
            axis,
            radius,
        } => {
            value["origin"] = json!(tidy(*origin));
            value["axis"] = json!(tidy(*axis));
            value["radius"] = json!(radius);
        }
        SurfaceGeometry::Cone {
            origin,
            axis,
            radius,
            half_angle,
        } => {
            value["origin"] = json!(tidy(*origin));
            value["axis"] = json!(tidy(*axis));
            value["radius"] = json!(radius);
            value["half_angle"] = json!(half_angle);
        }
        SurfaceGeometry::Sphere { center, radius } => {
            value["center"] = json!(tidy(*center));
            value["radius"] = json!(radius);
        }
        SurfaceGeometry::Torus {
            center,
            axis,
            major_radius,
            minor_radius,
        } => {
            value["center"] = json!(tidy(*center));
            value["axis"] = json!(tidy(*axis));
            value["major_radius"] = json!(major_radius);
            value["minor_radius"] = json!(minor_radius);
        }
        SurfaceGeometry::Other { .. } => {}
    }
}

/// A curve's fields in an edge's entry: its `start` and `end`, and a
/// circle's or arc's `center`, `radius` and `normal` (the arc runs
/// counter-clockwise about it from `start` to `end`).
fn add_curve_json(value: &mut Value, curve: &CurveGeometry) {
    match curve {
        CurveGeometry::Line { start, end } | CurveGeometry::Other { start, end, .. } => {
            value["start"] = json!(tidy(*start));
            value["end"] = json!(tidy(*end));
        }
        CurveGeometry::Circle {
            center,
            normal,
            radius,
            start,
            end,
        } => {
            value["center"] = json!(tidy(*center));
            value["radius"] = json!(radius);
            value["normal"] = json!(tidy(*normal));
            value["start"] = json!(tidy(*start));
            value["end"] = json!(tidy(*end));
        }
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait FaceListsOps<K: Kernel> {
    fn faces_json(&self, query: &FacesQuery) -> Result<Value, ApiError>;

    fn edges_json(&self, query: &EdgesQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> FaceListsOps<K> for Document<K> {
    fn faces_json(&self, query: &FacesQuery) -> Result<Value, ApiError> {
        check_range("area", query.min_area, query.max_area)?;
        let normal = match query.normal {
            Some(n) => Some(
                mitcad_vec3::unit(n)
                    .filter(|u| u.iter().all(|x| x.is_finite()))
                    .ok_or_else(|| ApiError("normal must be a nonzero direction".to_owned()))?,
            ),
            None => None,
        };
        let tolerance = query.normal_tolerance.unwrap_or(NORMAL_TOLERANCE);
        if !(tolerance.is_finite() && tolerance >= 0.0) {
            return Err(ApiError(
                "normal_tolerance must be a nonnegative angle in radians".to_owned(),
            ));
        }
        let shape = self.require_body(query.body)?;
        let kernel = self.kernel();
        let faces = kernel.faces(shape).map_err(kernel_error)?;
        let surfaces: Vec<SurfaceGeometry> = if query.geometry || normal.is_some() {
            let surfaces = kernel.face_geometries(shape).map_err(kernel_error)?;
            if surfaces.len() != faces.len() {
                return Err(ApiError(format!(
                    "the kernel described {} of the body's {} faces",
                    surfaces.len(),
                    faces.len()
                )));
            }
            surfaces.into_iter().map(|(_, surface)| surface).collect()
        } else {
            Vec::new()
        };
        let least = tolerance.cos();
        let matched: Vec<usize> = (0..faces.len())
            .filter(|&i| {
                let face = &faces[i];
                query.surface.as_ref().is_none_or(|s| face.surface == *s)
                    && query.min_area.is_none_or(|m| face.area >= m)
                    && query.max_area.is_none_or(|m| face.area <= m)
                    && normal.is_none_or(|n| match &surfaces[i] {
                        SurfaceGeometry::Plane { normal: own, .. } => mitcad_vec3::unit(*own)
                            .is_some_and(|own| mitcad_vec3::dot(own, n) >= least),
                        _ => false,
                    })
            })
            .collect();
        let chosen = page(&matched, query.offset, query.limit);
        let boxes = if query.geometry {
            let boxes = kernel
                .face_bounding_boxes(shape, chosen)
                .map_err(kernel_error)?;
            Some(boxes)
        } else {
            None
        };
        let paged = query.paged();
        let items = chosen
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                let face = &faces[i];
                let mut value =
                    json!({"names": face.names, "surface": face.surface, "area": face.area});
                if paged {
                    value["index"] = json!(i);
                }
                if let Some(boxes) = &boxes {
                    add_surface_json(&mut value, &surfaces[i]);
                    value["bbox"] = bbox_json(boxes.get(k).copied().flatten());
                }
                value
            })
            .collect();
        Ok(answer(paged, "faces", items, matched.len(), query.offset))
    }

    fn edges_json(&self, query: &EdgesQuery) -> Result<Value, ApiError> {
        check_range("length", query.min_length, query.max_length)?;
        let shape = self.require_body(query.body)?;
        let kernel = self.kernel();
        let edges = kernel.edges(shape).map_err(kernel_error)?;
        if let Some(radius) = query.radius
            && !(radius.is_finite() && radius > 0.0)
        {
            return Err(ApiError("radius must be a positive length".to_owned()));
        }
        if let Some(types) = &query.between
            && !(1..=2).contains(&types.len())
        {
            return Err(ApiError(
                "between takes one or two surface types, e.g. [\"cylinder\", \"sphere\"]"
                    .to_owned(),
            ));
        }
        let curves = if query.geometry || query.radius.is_some() {
            let curves = kernel.edge_geometries(shape).map_err(kernel_error)?;
            if curves.len() != edges.len() {
                return Err(ApiError(format!(
                    "the kernel described {} of the body's {} edges",
                    curves.len(),
                    edges.len()
                )));
            }
            curves
        } else {
            Vec::new()
        };
        // Each face name's surface type, for `between` (mitcad#1587).
        let surfaces: std::collections::HashMap<String, String> = if query.between.is_some() {
            kernel
                .faces(shape)
                .map_err(kernel_error)?
                .into_iter()
                .flat_map(|face| {
                    let surface = face.surface;
                    face.names
                        .into_iter()
                        .map(move |name| (name, surface.clone()))
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };
        let surface_of = |name: &str| {
            surfaces.get(name).or_else(|| {
                // A face named with a piece number (`#k`) where the edge
                // names the whole face, or the other way round.
                let base = name.split_once('#').map_or(name, |(base, _)| base);
                surfaces
                    .iter()
                    .find(|(face, _)| {
                        face.split_once('#').map_or(face.as_str(), |(b, _)| b) == base
                    })
                    .map(|(_, surface)| surface)
            })
        };
        let between = |name: Option<&String>| {
            let Some(types) = &query.between else {
                return true;
            };
            let Some((a, b)) = name.and_then(|name| mitcad_model::sheet::pick::edge_faces(name))
            else {
                return false;
            };
            let (a, b) = (surface_of(&a), surface_of(&b));
            match types.as_slice() {
                [one] => a == Some(one) || b == Some(one),
                [x, y] => (a == Some(x) && b == Some(y)) || (a == Some(y) && b == Some(x)),
                _ => false,
            }
        };
        let matched: Vec<usize> = (0..edges.len())
            .filter(|&i| {
                let edge = &edges[i];
                query.curve.as_ref().is_none_or(|c| edge.curve == *c)
                    && query.min_length.is_none_or(|m| edge.length >= m)
                    && query.max_length.is_none_or(|m| edge.length <= m)
                    && query.radius.is_none_or(|r| match &curves[i].1 {
                        CurveGeometry::Circle { radius, .. } => {
                            (radius - r).abs() <= RADIUS_TOLERANCE
                        }
                        _ => false,
                    })
                    && between(edge.name.as_ref())
            })
            .collect();
        let chosen = page(&matched, query.offset, query.limit);
        let geometry = if query.geometry {
            let boxes = kernel
                .edge_bounding_boxes(shape, chosen)
                .map_err(kernel_error)?;
            Some((curves, boxes))
        } else {
            None
        };
        let paged = query.paged();
        let items = chosen
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                let edge = &edges[i];
                let mut value =
                    json!({"name": edge.name, "curve": edge.curve, "length": edge.length});
                if paged {
                    value["index"] = json!(i);
                }
                if let Some((curves, boxes)) = &geometry {
                    add_curve_json(&mut value, &curves[i].1);
                    value["bbox"] = bbox_json(boxes.get(k).copied().flatten());
                }
                value
            })
            .collect();
        Ok(answer(paged, "edges", items, matched.len(), query.offset))
    }
}
