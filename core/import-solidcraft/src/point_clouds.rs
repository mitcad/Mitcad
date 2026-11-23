// SPDX-License-Identifier: MIT
//! Point clouds (formats 43, 45 and 51; mitcad#2240): each of the design's
//! clouds as a MitCad point cloud item (`mitcad_model::PointCloud`) in the
//! root: its name, the file it came from, its points (base64 of
//! little-endian f32 x, y, z from its `origin`, millimetres; a container's
//! `points_blob` is put back so by the reader), its colours (base64 of r,
//! g, b bytes), its placement (`transform`, a rigid motion as a 4 x 4
//! matrix whose last row is the translation) and its light bulb. What the
//! file keeps besides (how the points are coloured, filters, segments,
//! triangles and other properties, the point size, a file to reload from)
//! is MitCad's own and left out with one note.

use serde_json::Value;

use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;
use mitcad_model::{CloudData, Document};

/// A cloud of the design as the import reads it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileCloud {
    pub name: String,
    pub file: String,
    pub data: CloudData,
    pub placement: Transform,
    pub visible: bool,
}

fn decode(value: Option<&Value>, what: &str) -> Result<Vec<u8>, String> {
    match value.and_then(Value::as_str) {
        None | Some("") => Ok(Vec::new()),
        Some(text) => mitcad_model::internal::base64::decode(text)
            .map_err(|e| format!("its {what} are no base64: {e}")),
    }
}

/// A cloud entry of the file, or why it cannot be read.
pub(crate) fn read_cloud(value: &Value, index: usize) -> Result<FileCloud, String> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map_or_else(|| format!("Point cloud {}", index + 1), str::to_owned);
    let origin: [f64; 3] = match value.get("origin").and_then(Value::as_array) {
        Some(o) if o.len() == 3 => {
            let o: Vec<f64> = o.iter().filter_map(Value::as_f64).collect();
            if o.len() != 3 {
                return Err("its origin is not three numbers".to_owned());
            }
            [o[0], o[1], o[2]]
        }
        None => [0.0; 3],
        Some(_) => return Err("its origin is not three numbers".to_owned()),
    };
    let points = decode(value.get("points"), "points")?;
    if points.is_empty() || !points.len().is_multiple_of(12) {
        return Err("its points are not whole x, y, z triples".to_owned());
    }
    let points: Vec<[f64; 3]> = points
        .as_chunks::<12>()
        .0
        .iter()
        .map(|p| {
            std::array::from_fn(|k| {
                let c = f32::from_le_bytes([p[4 * k], p[4 * k + 1], p[4 * k + 2], p[4 * k + 3]]);
                origin[k] + f64::from(c)
            })
        })
        .collect();
    let colors = decode(value.get("colors"), "colours")?;
    let colors = (!colors.is_empty() && colors.len() == points.len() * 3)
        .then(|| colors.as_chunks::<3>().0.to_vec());
    let placement = match value.get("transform").and_then(Value::as_array) {
        None => Transform::IDENTITY,
        Some(rows) => {
            let m: Vec<Vec<f64>> = rows
                .iter()
                .map(|r| {
                    r.as_array()
                        .map(|r| r.iter().filter_map(Value::as_f64).collect())
                        .unwrap_or_default()
                })
                .collect();
            if m.len() != 4 || m.iter().any(|r| r.len() != 4) {
                return Err("its placement is not a 4 x 4 matrix".to_owned());
            }
            // The file's rows are its axes' images: p' = Σ p_r m[r] + m[3].
            let placement = Transform {
                linear: std::array::from_fn(|r| std::array::from_fn(|c| m[c][r])),
                translation: [m[3][0], m[3][1], m[3][2]],
            };
            if !placement.is_finite() || !placement.is_rigid() {
                return Err("its placement is not a rigid motion".to_owned());
            }
            placement
        }
    };
    let file_points = value
        .get("source_points")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n >= points.len())
        .unwrap_or(points.len());
    Ok(FileCloud {
        name,
        file: value
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        data: CloudData {
            points,
            colors,
            file_points,
        },
        placement,
        visible: value
            .get("visible")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

/// Whether a cloud entry keeps anything MitCad's point clouds do not: a
/// colouring other than the file's colours, filters, segments, triangles
/// or other properties.
fn has_extras(value: &Value) -> bool {
    let set = |key: &str| value.get(key).is_some_and(|v| !v.is_null());
    value
        .get("color_by")
        .and_then(Value::as_str)
        .is_some_and(|c| c != "file")
        || value
            .get("filters")
            .and_then(Value::as_array)
            .is_some_and(|f| !f.is_empty())
        || set("complex")
}

/// The design's clouds as point cloud items; notes in words for those that
/// cannot be read and for what MitCad keeps none of.
pub(crate) fn import_clouds<K: Kernel>(
    doc: &mut Document<K>,
    clouds: &[Value],
    notes: &mut Vec<String>,
) {
    let mut extras = 0usize;
    for (index, value) in clouds.iter().enumerate() {
        extras += usize::from(has_extras(value));
        let made = read_cloud(value, index).and_then(|cloud| {
            let name = doc.free_point_cloud_name(&cloud.name);
            doc.add_point_cloud(
                Some(&name),
                &cloud.file,
                cloud.data,
                None,
                cloud.placement,
                cloud.visible,
            )
            .map_err(|e| e.to_string())
        });
        if let Err(why) = made {
            notes.push(format!("a point cloud cannot be read: left out ({why})"));
        }
    }
    if extras > 0 {
        // One and several as whole messages (mitcad#1284).
        notes.push(if extras == 1 {
            "a point cloud's colouring, filters and segments: not mapped (MitCad keeps the points and their colours)".to_owned()
        } else {
            format!(
                "{extras} point clouds' colouring, filters and segments: not mapped (MitCad keeps the points and their colours)"
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base64(bytes: &[u8]) -> String {
        mitcad_model::internal::base64::encode(bytes)
    }

    fn points(values: &[[f32; 3]]) -> String {
        let bytes: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        base64(&bytes)
    }

    #[test]
    fn a_cloud_entry_reads_its_points_colours_and_placement() {
        let value = json!({
            "id": 7, "name": "Scan", "file": "scan.ply",
            "origin": [1000.0, 0.0, 0.0],
            "points": points(&[[1.0, 2.0, 3.0], [-1.0, 0.0, 0.5]]),
            "colors": base64(&[255, 0, 0, 0, 0, 255]),
            "source_points": 4,
            // A quarter turn about z, then 10 up.
            "transform": [[0.0, 1.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 10.0, 1.0]],
            "visible": false,
        });
        let cloud = read_cloud(&value, 0).unwrap();
        assert_eq!(cloud.name, "Scan");
        assert_eq!(cloud.file, "scan.ply");
        assert_eq!(
            cloud.data.points,
            vec![[1001.0, 2.0, 3.0], [999.0, 0.0, 0.5]]
        );
        assert_eq!(cloud.data.colors, Some(vec![[255, 0, 0], [0, 0, 255]]));
        assert_eq!(cloud.data.file_points, 4);
        assert!(!cloud.visible);
        // x onto y.
        let moved = cloud.placement.apply_point([1.0, 0.0, 0.0]);
        assert!((moved[0]).abs() < 1e-12 && (moved[1] - 1.0).abs() < 1e-12);
        assert!((moved[2] - 10.0).abs() < 1e-12);
    }

    #[test]
    fn a_damaged_entry_says_why() {
        let error = read_cloud(&json!({"name": "A", "points": base64(&[0; 5])}), 0).unwrap_err();
        assert!(error.contains("whole x, y, z"), "{error}");
        let error = read_cloud(
            &json!({"points": points(&[[0.0; 3]]), "transform": [[2.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]}),
            2,
        )
        .unwrap_err();
        assert!(error.contains("rigid"), "{error}");
        assert_eq!(
            read_cloud(&json!({"points": points(&[[0.0; 3]])}), 2)
                .unwrap()
                .name,
            "Point cloud 3"
        );
    }
}
