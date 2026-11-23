// SPDX-License-Identifier: MIT
//! Point clouds and Point Cloud Deviation (mitcad#2240,
//! `mitcad_model::PointCloud`; see `commands.md`, "Point clouds"): the
//! commands `insert_point_cloud`, `move_point_cloud`, `place_point_cloud`,
//! `set_point_cloud_visible`, `rename_point_cloud` and
//! `delete_point_cloud`, and the queries `point_clouds`,
//! `point_cloud_points` and `point_cloud_deviation`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::point_cloud_files::DEFAULT_MAX_POINTS;
use mitcad_model::topo::FaceName;
use mitcad_model::transform::Transform;
use mitcad_model::{
    CloudInsert, CloudSource, DEFAULT_DEVIATION_TOLERANCE, DEFAULT_MAX_SAMPLES, DEFAULT_WORST,
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
    }
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
