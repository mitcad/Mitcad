// SPDX-License-Identifier: MIT
//! Point clouds and Point Cloud Deviation (mitcad#2240,
//! `mitcad_model::PointCloud`; see `commands.md`, "Point clouds"): the
//! commands `insert_point_cloud`, `move_point_cloud`, `place_point_cloud`,
//! `set_point_cloud_visible`, `rename_point_cloud` and
//! `delete_point_cloud`, and the queries `point_clouds`,
//! `point_cloud_points` and `point_cloud_deviation`. The point cloud tools
//! (mitcad#2516, `mitcad_model::point_cloud_tools`; commands.md "Point
//! cloud tools"): `align_point_cloud` with its query
//! `point_cloud_alignment`, `filter_point_cloud` and
//! `reconstruct_point_cloud`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::point_cloud_files::DEFAULT_MAX_POINTS;
use mitcad_model::point_cloud_tools::filter::{
    ByRadius, CropBox, CropLasso, FilterOptions, Smooth, Statistical,
};
use mitcad_model::point_cloud_tools::matching::FitOptions;
use mitcad_model::point_cloud_tools::reconstruct::{self, ReconstructOptions};
use mitcad_model::topo::FaceName;
use mitcad_model::transform::Transform;
use mitcad_model::{
    AlignSettings, AlignStart, AlignTarget, CloudFilter, CloudInsert, CloudSource,
    DEFAULT_ALIGN_SAMPLES, DEFAULT_DEVIATION_TOLERANCE, DEFAULT_MAX_SAMPLES, DEFAULT_WORST,
    Document, PointCloudDeviation,
};

/// A turn about an axis, degrees, counter-clockwise looking against the
/// axis (right-hand rule).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rotation {
    /// A point of the axis; the component's origin when left out.
    #[serde(default)]
    origin: Option<[f64; 3]>,
    axis: [f64; 3],
    angle: f64,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum PointCloudCommand {
        // A file (`path`) or its bytes (`data`, base64, with `file_name`).
        "insert_point_cloud" => Insert {
            #[serde(default)]
            path: Option<String>,
            #[serde(default)]
            data: Option<String>,
            #[serde(default)]
            file_name: Option<String>,
            /// The next free `PointCloud<n>` when left out.
            #[serde(default)]
            name: Option<String>,
            /// The occurrence of the component it goes into, from the root
            /// (`O1/O4`); the active component's first occurrence when
            /// left out (the root while the root is active).
            #[serde(default)]
            occurrence: Option<OccurrencePath>,
            /// The file's length unit: `mm` (the default), `cm`, `m`, `in`
            /// or `ft`.
            #[serde(default)]
            unit: Option<String>,
            #[serde(default)]
            max_points: Option<usize>,
        },
        // Turned (`rotate`) then moved (`translate`, mm) in its component.
        "move_point_cloud" => Move {
            name: String,
            #[serde(default)]
            translate: Option<[f64; 3]>,
            #[serde(default)]
            rotate: Option<Rotation>,
        },
        // Into another occurrence's component (`occurrence`, "" for the
        // root) and/or back where it came in (`reset`).
        "place_point_cloud" => Place {
            name: String,
            #[serde(default)]
            occurrence: Option<OccurrencePath>,
            #[serde(default)]
            reset: bool,
        },
        "set_point_cloud_visible" => SetVisible {
            name: String,
            visible: bool,
        },
        "rename_point_cloud" => Rename {
            name: String,
            new_name: String,
        },
        "delete_point_cloud" => Delete {
            name: String,
        },
        // The point cloud tools (mitcad#2516): moved onto a body, a mesh
        // body or another cloud; filtered into a new cloud; rebuilt as a
        // mesh body.
        "align_point_cloud" => Align(Box<AlignFields>),
        "filter_point_cloud" => Filter(Box<FilterFields>),
        "reconstruct_point_cloud" => Reconstruct {
            name: String,
            #[serde(default = "default_detail")]
            detail: usize,
            #[serde(default = "default_normal_neighbours", rename = "neighbors")]
            neighbours: usize,
        },
    }
}

fn default_true() -> bool {
    true
}

fn default_detail() -> usize {
    reconstruct::DEFAULT_DETAIL
}

fn default_normal_neighbours() -> usize {
    reconstruct::DEFAULT_NEIGHBOURS
}

fn default_iterations() -> usize {
    FitOptions::default().max_iterations
}

fn default_align_samples() -> usize {
    DEFAULT_ALIGN_SAMPLES
}

fn default_outlier_neighbours() -> usize {
    16
}

fn default_ratio() -> f64 {
    2.0
}

fn default_radius_neighbours() -> usize {
    4
}

fn default_passes() -> usize {
    1
}

/// A picked point pair: a point on the cloud and its place on the target,
/// in the design's coordinates.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairField {
    cloud: [f64; 3],
    target: [f64; 3],
}

/// A box: its lowest and highest corner, in the design's coordinates.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BoxField {
    min: [f64; 3],
    max: [f64; 3],
    /// Keeps the points inside (a crop's; the default) or outside.
    #[serde(default = "default_true")]
    inside: bool,
}

/// The fields of `align_point_cloud` and of the `point_cloud_alignment`
/// query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AlignFields {
    /// The cloud aligned.
    name: String,
    /// The target: a body (`body`, with some of its faces in `faces`), or
    /// another cloud (`target_cloud`).
    #[serde(default)]
    body: Option<BodyUid>,
    #[serde(default)]
    faces: Vec<FaceName>,
    #[serde(default)]
    target_cloud: Option<String>,
    /// `placed` (the default) or `shape`; `pairs` with `pairs`.
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    pairs: Vec<PairField>,
    #[serde(default = "default_true")]
    best_fit: bool,
    #[serde(default)]
    region: Option<BoxField>,
    #[serde(default)]
    max_distance: Option<f64>,
    #[serde(default = "default_iterations")]
    max_iterations: usize,
    #[serde(default = "default_align_samples")]
    max_samples: usize,
}

/// Statistical outliers' settings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StatisticalField {
    #[serde(default = "default_outlier_neighbours", rename = "neighbors")]
    neighbours: usize,
    #[serde(default = "default_ratio")]
    ratio: f64,
}

/// Outliers by radius.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RadiusField {
    radius: f64,
    #[serde(default = "default_radius_neighbours", rename = "neighbors")]
    neighbours: usize,
}

/// Noise smoothing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SmoothField {
    #[serde(default = "default_outlier_neighbours", rename = "neighbors")]
    neighbours: usize,
    #[serde(default = "default_passes")]
    passes: usize,
}

/// A lasso: a polygon on a plane facing the viewer (`normal`), in the
/// design's coordinates.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LassoField {
    normal: [f64; 3],
    points: Vec<[f64; 3]>,
    #[serde(default = "default_true")]
    inside: bool,
}

/// The fields of `filter_point_cloud`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FilterFields {
    name: String,
    #[serde(default)]
    new_name: Option<String>,
    #[serde(default)]
    keep_shown: bool,
    #[serde(default)]
    statistical: Option<StatisticalField>,
    #[serde(default)]
    radius: Option<RadiusField>,
    #[serde(default)]
    smooth: Option<SmoothField>,
    #[serde(default)]
    crop_box: Option<BoxField>,
    #[serde(default)]
    lasso: Option<LassoField>,
}

/// An alignment's settings of its fields; refused in words for a target
/// that is not one or a start that does not fit the pairs.
fn align_settings(fields: &AlignFields) -> Result<AlignSettings, ApiError> {
    let target = match (&fields.body, &fields.target_cloud) {
        (Some(body), None) => AlignTarget::Body {
            body: *body,
            faces: fields.faces.clone(),
        },
        (None, Some(cloud)) => {
            if !fields.faces.is_empty() {
                return Err(ApiError(
                    "faces are a body's: give them with `body`, not with `target_cloud`".to_owned(),
                ));
            }
            AlignTarget::Cloud(cloud.clone())
        }
        _ => {
            return Err(ApiError(
                "give what the cloud is aligned to: a `body` or a `target_cloud`, one of them"
                    .to_owned(),
            ));
        }
    };
    let start = match (fields.start.as_deref(), fields.pairs.is_empty()) {
        (None | Some("pairs"), false) => AlignStart::Pairs(
            fields
                .pairs
                .iter()
                .map(|pair| (pair.cloud, pair.target))
                .collect(),
        ),
        (None | Some("placed"), true) => AlignStart::Placed,
        (Some("shape"), true) => AlignStart::Shape,
        (Some("pairs"), true) => {
            return Err(ApiError(
                "a start by point pairs needs the `pairs`".to_owned(),
            ));
        }
        (Some("placed" | "shape"), false) => {
            return Err(ApiError(
                "point pairs give the start: leave `start` out or make it `pairs`".to_owned(),
            ));
        }
        (Some(other), _) => {
            return Err(ApiError(format!(
                "the start must be placed, shape or pairs, got {other}"
            )));
        }
    };
    Ok(AlignSettings {
        start,
        best_fit: fields.best_fit,
        region: fields.region.as_ref().map(|r| (r.min, r.max)),
        max_distance: fields.max_distance,
        max_iterations: fields.max_iterations,
        max_samples: fields.max_samples,
        ..AlignSettings::new(target)
    })
}

/// A filter's settings of its fields.
fn filter_settings(fields: &FilterFields) -> CloudFilter {
    CloudFilter {
        options: FilterOptions {
            crop_box: fields.crop_box.as_ref().map(|b| CropBox {
                min: b.min,
                max: b.max,
                keep_inside: b.inside,
            }),
            lasso: fields.lasso.as_ref().map(|l| CropLasso {
                normal: l.normal,
                points: l.points.clone(),
                keep_inside: l.inside,
            }),
            statistical: fields.statistical.as_ref().map(|s| Statistical {
                neighbours: s.neighbours,
                ratio: s.ratio,
            }),
            radius: fields.radius.as_ref().map(|r| ByRadius {
                radius: r.radius,
                neighbours: r.neighbours,
            }),
            smooth: fields.smooth.as_ref().map(|s| Smooth {
                neighbours: s.neighbours,
                passes: s.passes,
            }),
        },
        name: fields.new_name.clone(),
        keep_shown: fields.keep_shown,
    }
}

/// The `point_cloud_alignment` query: the motion and the remaining
/// deviation, the cloud left where it is.
pub(crate) fn alignment_json<K: Kernel>(
    doc: &Document<K>,
    fields: &AlignFields,
) -> Result<Value, ApiError> {
    let settings = align_settings(fields)?;
    Ok(doc.point_cloud_alignment(&fields.name, &settings)?.json())
}

/// Millimetres per unit, by its name.
fn unit_scale(unit: Option<&str>) -> Result<f64, ApiError> {
    let Some(unit) = unit else {
        return Ok(1.0);
    };
    Ok(match unit.trim().to_ascii_lowercase().as_str() {
        "mm" => 1.0,
        "cm" => 10.0,
        "m" => 1000.0,
        "in" => 25.4,
        "ft" => 304.8,
        other => {
            return Err(ApiError(format!(
                "the point cloud's unit must be mm, cm, m, in or ft, got {other}"
            )));
        }
    })
}

/// The file of an insert: read from `path`, or decoded from `data`.
fn source(
    path: Option<String>,
    data: Option<String>,
    file_name: Option<String>,
) -> Result<CloudSource, ApiError> {
    let bytes = match (&path, &data) {
        (Some(_), Some(_)) => {
            return Err(ApiError(
                "give the point cloud's `path` or its `data`, not both".to_owned(),
            ));
        }
        (None, None) => {
            return Err(ApiError(
                "give the point cloud's `path` or its `data`".to_owned(),
            ));
        }
        (Some(path), None) => {
            std::fs::read(path).map_err(|e| ApiError(format!("cannot read {path}: {e}")))?
        }
        (None, Some(data)) => mitcad_model::internal::base64::decode(data)
            .map_err(|e| ApiError(format!("the point cloud's data is no base64: {e}")))?,
    };
    let own = path
        .as_deref()
        .map(std::path::Path::new)
        .and_then(std::path::Path::file_name)
        .map(|n| n.to_string_lossy().into_owned());
    let file_name = file_name.or(own).unwrap_or_default();
    Ok(CloudSource { bytes, file_name })
}

pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: PointCloudCommand,
) -> Result<Value, ApiError> {
    Ok(match command {
        PointCloudCommand::Insert {
            path,
            data,
            file_name,
            name,
            occurrence,
            unit,
            max_points,
        } => {
            let source = source(path, data, file_name)?;
            let insert = CloudInsert {
                name,
                occurrence,
                scale: unit_scale(unit.as_deref())?,
                max_points: max_points.unwrap_or(DEFAULT_MAX_POINTS),
            };
            let (name, warnings) = doc.insert_point_cloud(source, insert)?;
            let cloud = doc
                .point_clouds()
                .iter()
                .find(|c| c.name == name)
                .ok_or_else(|| ApiError(format!("the point cloud {name} was not kept")))?;
            json!({
                "name": name,
                "count": cloud.points.len(),
                "file_points": cloud.file_points,
                "warnings": warnings,
            })
        }
        PointCloudCommand::Move {
            name,
            translate,
            rotate,
        } => {
            let mut motion = Transform::IDENTITY;
            if let Some(rotate) = rotate {
                if !rotate.angle.is_finite() {
                    return Err(ApiError("the turn's angle must be finite".to_owned()));
                }
                motion = Transform::rotation(
                    rotate.origin.unwrap_or([0.0; 3]),
                    rotate.axis,
                    rotate.angle.to_radians(),
                )
                .ok_or_else(|| ApiError("the turn's axis has no direction".to_owned()))?;
            }
            if let Some(offset) = translate {
                motion = Transform::translation(offset).after(&motion);
            }
            doc.move_point_cloud(&name, motion)?;
            json!({})
        }
        PointCloudCommand::Place {
            name,
            occurrence,
            reset,
        } => {
            doc.place_point_cloud(&name, occurrence.map(Some), reset)?;
            json!({})
        }
        PointCloudCommand::SetVisible { name, visible } => {
            doc.set_point_cloud_visible(&name, visible)?;
            json!({})
        }
        PointCloudCommand::Rename { name, new_name } => {
            doc.rename_point_cloud(&name, &new_name)?;
            json!({})
        }
        PointCloudCommand::Delete { name } => {
            doc.delete_point_cloud(&name)?;
            json!({})
        }
        PointCloudCommand::Align(fields) => {
            let settings = align_settings(&fields)?;
            doc.align_point_cloud(&fields.name, &settings)?.json()
        }
        PointCloudCommand::Filter(fields) => {
            let settings = filter_settings(&fields);
            doc.filter_point_cloud(&fields.name, &settings)?.json()
        }
        PointCloudCommand::Reconstruct {
            name,
            detail,
            neighbours,
        } => {
            let options = ReconstructOptions { detail, neighbours };
            doc.reconstruct_point_cloud(&name, &options)?.json()
        }
    })
}

fn default_tolerance() -> f64 {
    DEFAULT_DEVIATION_TOLERANCE
}

fn default_max_samples() -> usize {
    DEFAULT_MAX_SAMPLES
}

fn default_worst() -> usize {
    DEFAULT_WORST
}

/// The `point_cloud_deviation` query: a deviation's fields, how many worst
/// points to list and whether each sampled point's distance is wanted.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeviationQuery {
    cloud: String,
    body: BodyUid,
    #[serde(default)]
    faces: Vec<FaceName>,
    #[serde(default = "default_tolerance")]
    tolerance: f64,
    #[serde(default = "default_max_samples")]
    max_samples: usize,
    #[serde(default)]
    signed: Option<bool>,
    #[serde(default = "default_worst")]
    worst: usize,
    #[serde(default)]
    distances: bool,
}

/// The `point_clouds` query.
pub(crate) fn clouds_json<K: Kernel>(doc: &Document<K>) -> Value {
    doc.point_clouds_json()
}

/// The `point_cloud_points` query.
pub(crate) fn points_json<K: Kernel>(doc: &Document<K>, name: &str) -> Result<Value, ApiError> {
    Ok(doc.point_cloud_points_json(name)?)
}

/// The `point_cloud_deviation` query: the statistics, the worst points and,
/// when asked, each sampled point's distance; the cloud and body named.
pub(crate) fn deviation_json<K: Kernel>(
    doc: &Document<K>,
    query: DeviationQuery,
) -> Result<Value, ApiError> {
    let def = PointCloudDeviation {
        cloud: query.cloud,
        body: query.body,
        faces: query.faces,
        tolerance: query.tolerance,
        max_samples: query.max_samples,
        signed: query.signed,
        limit: None,
    };
    let deviation = doc.point_cloud_deviation(&def, query.worst)?;
    let mut value = deviation.json(query.distances);
    value["cloud"] = json!(def.cloud);
    value["body"] = json!(def.body);
    value["body_name"] = json!(doc.body_name(def.body));
    Ok(value)
}
