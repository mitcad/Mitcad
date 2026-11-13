// SPDX-License-Identifier: MIT
//! Base features from shapes the caller built, and the bodies of .f3d
//! files without history, for C++ callers (`mitcad-cli`, the application);
//! the mesh bodies of the .f3d files come with them, with their materials
//! and appearances (mitcad#1376). Rust callers, such as the .f3d import, use
//! [`mitcad_model::Document::add_base_feature`] with shapes from
//! [`crate::kernel::exchange::ffi::f3d_bodies`] directly.

use cxx::SharedPtr;
use mitcad_model::Kernel as _;
use mitcad_model::features::Operation;
use mitcad_model::{BaseInput, BodyUid, ImportBody, Imported};
use mitcad_model_api::ApiError;
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
        let input = BaseInput {
            name: options.name,
            source: options.source,
            operation: options.operation,
            participants: options.participants,
            ..BaseInput::new(bodies)
        };
        // An import into the active component, as a command is: checked
        // against its design policy (mitcad#220).
        let imported = self
            .0
            .with_policy_checks(|document| document.add_base_feature(input))?;
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
        // The mesh bodies (mitcad#1376): a base feature each after the
        // B-rep bodies, placed as the timeline import places them.
        let documents = crate::f3d_import::file_meshes(path).map_err(ApiError)?;
        let mut meshes_found = Vec::new();
        let mut meshes_not_read = Vec::new();
        // Each mesh base feature's document and mesh body record.
        let mut mesh_records = Vec::new();
        for (d, document) in documents.iter().enumerate() {
            for mesh in &document.meshes {
                let place = if document.document.is_empty() {
                    mesh.source.clone()
                } else {
                    format!("{}: {}", document.document, mesh.source)
                };
                let built = mesh.mesh.as_ref().map_err(String::clone).and_then(|m| {
                    let shape = self
                        .0
                        .kernel()
                        .triangle_body(&m.points, &m.triangles)
                        .map_err(|e| e.to_string())?;
                    Ok((shape, m.triangles.len()))
                });
                let (shape, triangles) = match built {
                    Ok(built) => built,
                    Err(e) => {
                        meshes_not_read.push(json!({"source": place, "error": e}));
                        continue;
                    }
                };
                meshes_found.push(json!({"source": place, "triangles": triangles}));
                mesh_records.push((d, mesh.id));
                inputs.push(BaseInput {
                    source: Some(format!("{file}: {place}")),
                    ..BaseInput::new(vec![ImportBody {
                        name: mesh.name.clone(),
                        color: None,
                        shape,
                    }])
                });
            }
        }
        if inputs.is_empty() {
            return Err(ApiError(format!(
                "{file} has no bodies to import ({} could not be built)",
                skipped.len() + meshes_not_read.len()
            )));
        }
        let label = format!("Import {file}");
        let depth = self.0.undo_depth();
        let imported = self.0.add_base_features(&label, inputs)?;
        let (breps, meshes) = imported.split_at(found.len().min(imported.len()));
        for (report, feature) in found.iter_mut().zip(breps) {
            report["feature"] = json!(feature.feature.uid);
            report["feature_name"] = json!(feature.feature.name);
            report["bodies"] = imported_json(feature)["bodies"].clone();
        }
        // The mesh bodies' materials and appearances, part of the import's
        // undo step.
        let mut by_document: Vec<Vec<(u64, BodyUid)>> = vec![Vec::new(); documents.len()];
        for (&(d, record), feature) in mesh_records.iter().zip(meshes) {
            if let (Some(record), Some(body)) = (record, feature.bodies.first()) {
                by_document[d].push((record, body.uid));
            }
        }
        let mut gaps = Vec::new();
        for (document, bodies) in documents.iter().zip(&by_document) {
            if let Some(data) = &document.appearance
                && !bodies.is_empty()
            {
                let looks = mitcad_import::appearances::mesh_appearances(&mut self.0, data, bodies);
                gaps.extend(looks.gaps);
            }
        }
        self.0.merge_undo(depth, &label);
        for (report, feature) in meshes_found.iter_mut().zip(meshes) {
            report["feature"] = json!(feature.feature.uid);
            report["feature_name"] = json!(feature.feature.name);
            report["bodies"] = imported_json(feature)["bodies"].clone();
            if let Some(body) = feature.bodies.first() {
                let attributes = self.0.body_attributes(body.uid);
                report["appearance"] = json!(attributes.appearance);
                report["material"] = json!(attributes.material);
            }
        }
        meshes_found.extend(meshes_not_read);
        let mut report = json!({
            "file": file, "imported": found, "skipped": skipped, "meshes": meshes_found
        });
        if !gaps.is_empty() {
            report["mesh_gaps"] = json!(gaps);
        }
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
    // The mesh bodies (mitcad#1376).
    let meshes = report["meshes"].as_array().map_or(&[][..], Vec::as_slice);
    if !meshes.is_empty() {
        let read = meshes.iter().filter(|m| m["error"].is_null()).count();
        text += &format!(
            "Imported {read} mesh bodies ({} not read):\n",
            meshes.len() - read
        );
    }
    for mesh in meshes {
        let source = mesh["source"].as_str().unwrap_or_default();
        if let Some(error) = mesh["error"].as_str() {
            text += &format!("  not read: {source}: {error}\n");
            continue;
        }
        let body = &mesh["bodies"][0];
        let look = [
            ("appearance", &mesh["appearance"]),
            ("material", &mesh["material"]),
        ]
        .iter()
        .filter_map(|(what, v)| Some(format!(", {what} {}", v.as_str()?)))
        .collect::<String>();
        text += &format!(
            "  {} {} ({}): mesh body of {} triangles{look} from {source}\n",
            mesh["feature"].as_str().unwrap_or_default(),
            body["name"].as_str().unwrap_or_default(),
            body["uid"].as_str().unwrap_or_default(),
            mesh["triangles"]
        );
    }
    if let Some(gaps) = report["mesh_gaps"].as_array() {
        for gap in gaps {
            text += &format!("  {}\n", gap.as_str().unwrap_or_default());
        }
    }
    text
}
