// SPDX-License-Identifier: MIT
//! Base features from shapes the caller built, and the bodies of .f3d
//! files without history, for C++ callers (`mitcad-cli`, the application).
//! Rust callers, such as the .f3d import, use
//! [`mitcad_model::Document::add_base_feature`] with shapes from
//! [`crate::kernel::exchange::ffi::f3d_bodies`] directly.

use cxx::SharedPtr;
use mitcad_model::api::ApiError;
use mitcad_model::features::Operation;
use mitcad_model::{BaseInput, BodyUid, ImportBody, Imported};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Document;
use crate::kernel::Shape;
use crate::kernel::exchange::ffi::f3d_bodies;
use crate::kernel::shape::ffi::{ShapeList, new_shape_list};

/// The JSON of `add_base_feature`; every field is optional.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaseOptions {
    /// The feature's name.
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default = "new_body")]
    operation: Operation,
    #[serde(default)]
    participants: Vec<BodyUid>,
    /// Names and colours of the shapes, by index.
    #[serde(default)]
    bodies: Vec<BodyOptions>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyOptions {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    color: Option<[f64; 3]>,
}

/// The JSON of `import_f3d`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct F3dOptions {
    /// Also the bodies of `.smbh` blobs (with ASM history, often copies).
    #[serde(default)]
    history: bool,
    /// Also bodies that only own faces saved at the top level (assemblies).
    #[serde(default)]
    owners: bool,
    /// A readable summary instead of JSON.
    #[serde(default)]
    text: bool,
}

fn new_body() -> Operation {
    Operation::NewBody
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str, what: &str) -> Result<T, ApiError> {
    let json = if json.trim().is_empty() { "{}" } else { json };
    serde_json::from_str(json).map_err(|e| ApiError(format!("invalid {what}: {e}")))
}

fn imported_json(imported: &Imported) -> Value {
    let bodies: Vec<Value> = imported
        .bodies
        .iter()
        .map(|b| json!({"uid": b.uid, "name": b.name, "kind": b.kind.as_str()}))
        .collect();
    json!({"uid": imported.feature.uid, "name": imported.feature.name, "bodies": bodies})
}

/// The file name without directories.
fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

impl Document {
    pub(crate) fn add_base_feature(
        &mut self,
        shapes: &ShapeList,
        json: &str,
    ) -> Result<String, ApiError> {
        let options: BaseOptions = parse(json, "base feature options")?;
        if options.bodies.len() > shapes.size() {
            return Err(ApiError(format!(
                "{} body options for {} shapes",
                options.bodies.len(),
                shapes.size()
            )));
        }
        let mut bodies = Vec::with_capacity(shapes.size());
        for i in 0..shapes.size() {
            let shape: SharedPtr<Shape> = shapes.at(i);
            if shape.is_null() {
                return Err(ApiError(format!("shape {i} is null")));
            }
            let body = options.bodies.get(i);
            bodies.push(ImportBody {
                name: body.and_then(|b| b.name.clone()),
                color: body.and_then(|b| b.color),
                shape,
            });
        }
        let imported = self.0.add_base_feature(BaseInput {
            name: options.name,
            source: options.source,
            operation: options.operation,
            participants: options.participants,
            ..BaseInput::new(bodies)
        })?;
        Ok(imported_json(&imported).to_string())
    }

    pub(crate) fn import_f3d(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        // A job does not cancel it (Document::without_job).
        self.without_job(|document| document.import_f3d_bodies(path, json))
    }

    fn import_f3d_bodies(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        let options: F3dOptions = parse(json, "import options")?;
        let mut list = new_shape_list();
        let bodies = f3d_bodies(path, options.history, options.owners, list.pin_mut())
            .map_err(|e| ApiError(e.what().to_owned()))?;
        let file = file_name(path);
        let mut inputs = Vec::new();
        let mut found = Vec::new();
        let mut skipped = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            let place = if body.document.is_empty() {
                format!("{}#{}", body.blob, body.record)
            } else {
                format!("{}: {}#{}", body.document, body.blob, body.record)
            };
            if !body.built {
                skipped.push(json!({"source": place, "error": body.error}));
                continue;
            }
            found.push(json!({
                "source": place, "solid": body.solid, "valid": body.valid, "volume": body.volume,
                "area": body.area, "faces": body.faces, "issues": body.issues
            }));
            inputs.push(BaseInput {
                source: Some(format!("{file}: {place}")),
                ..BaseInput::new(vec![ImportBody {
                    name: None,
                    color: None,
                    shape: list.at(i),
                }])
            });
        }
        if inputs.is_empty() {
            return Err(ApiError(format!(
                "{file} has no bodies to import ({} could not be built)",
                skipped.len()
            )));
        }
        let imported = self
            .0
            .add_base_features(&format!("Import {file}"), inputs)?;
        for (report, feature) in found.iter_mut().zip(&imported) {
            report["feature"] = json!(feature.feature.uid);
            report["bodies"] = imported_json(feature)["bodies"].clone();
        }
        let report = json!({"file": file, "imported": found, "skipped": skipped});
        Ok(if options.text {
            summary(&report)
        } else {
            report.to_string()
        })
    }
}

/// The import report of `import_f3d` as text.
fn summary(report: &Value) -> String {
    let mut text = String::new();
    let imported = report["imported"].as_array().map_or(&[][..], Vec::as_slice);
    let skipped = report["skipped"].as_array().map_or(&[][..], Vec::as_slice);
    let count = |key: &str| imported.iter().filter(|b| b[key] == true).count();
    text += &format!(
        "Imported {} bodies of {} without history ({} solids, {} valid; {} not built):\n",
        imported.len(),
        report["file"].as_str().unwrap_or_default(),
        count("solid"),
        count("valid"),
        skipped.len()
    );
    for body in imported {
        let name = &body["bodies"][0];
        text += &format!(
            "  {} {} ({}): {} {}, volume {:.3} mm3, {} faces{} from {}\n",
            body["feature"].as_str().unwrap_or_default(),
            name["name"].as_str().unwrap_or_default(),
            name["uid"].as_str().unwrap_or_default(),
            if body["valid"] == true {
                "valid"
            } else {
                "INVALID"
            },
            if body["solid"] == true {
                "solid"
            } else {
                "sheet"
            },
            body["volume"].as_f64().unwrap_or_default(),
            body["faces"],
            match body["issues"].as_u64() {
                Some(0) | None => String::new(),
                Some(n) => format!(", {n} issues"),
            },
            body["source"].as_str().unwrap_or_default()
        );
    }
    for body in skipped {
        text += &format!(
            "  not built: {}: {}\n",
            body["source"].as_str().unwrap_or_default(),
            body["error"].as_str().unwrap_or_default()
        );
    }
    text
}
