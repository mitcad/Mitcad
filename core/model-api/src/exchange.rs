// SPDX-License-Identifier: MIT
//! Commands for data exchange: `import_file`, `export` and `export_sketch`
//! (see `commands.md`).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::Document;
use mitcad_model::exchange::{
    BedChoice, Coordinates, ExportFormat, ExportOptions, ExportPlacement, Imported, LengthUnit,
    OnBed, ReadOptions, Refinement, Settle, StepSchema,
};
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImportFileCommand {
    path: String,
    /// The base feature's name.
    #[serde(default)]
    name: Option<String>,
    /// STL and OBJ: millimetres per file unit.
    #[serde(default)]
    unit_mm: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportCommand {
    path: String,
    /// Bodies at the marker by uid or name; all when left out.
    #[serde(default)]
    bodies: Vec<String>,
    /// By the file extension when left out.
    #[serde(default)]
    format: Option<ExportFormat>,
    #[serde(default)]
    schema: StepSchema,
    #[serde(default)]
    unit: LengthUnit,
    #[serde(default)]
    refinement: Refinement,
    #[serde(default)]
    ascii: bool,
    /// Colours of bodies by uid or name (sRGB in [0, 1]), over imported
    /// ones: what the application shows (mitcad#13).
    #[serde(default)]
    colors: BTreeMap<String, [f64; 3]>,
    /// `design` (the default) or `component` (mitcad#17, mitcad#19).
    #[serde(default)]
    coordinates: Option<Coordinates>,
    /// Only the placements by this occurrence (`O1/O4` or `Arm:1/Pin:2`).
    #[serde(default)]
    occurrence: Option<String>,
    /// USDZ: the design file's folder, which relative texture paths are
    /// relative to (mitcad#123).
    #[serde(default)]
    design_folder: Option<String>,
    /// 3MF: the slicer whose settings go with the bodies (`bambu_studio`,
    /// `orcaslicer`, `qidi_studio`, `prusaslicer`, `cura`; mitcad#640).
    #[serde(default)]
    dialect: Option<String>,
    /// 3MF and STL: the bodies on the print bed by their print bed faces,
    /// centred on the plate (mitcad#731).
    #[serde(default)]
    on_bed: bool,
    /// 3MF (mitcad#856): per body (uid or name), the occurrences whose
    /// placements are written (`O1/O4` or `Arm:1/Pin:2`, `""` the design
    /// itself; an empty list all of them).
    #[serde(default)]
    occurrences: BTreeMap<String, Vec<String>>,
    /// 3MF on the print bed (mitcad#856): occurrences sent whole, each
    /// one object as assembled.
    #[serde(default)]
    whole: Vec<String>,
    /// On the print bed (mitcad#857): the print bed face that turns the
    /// piece it is in, `{"body", "occurrence"}`, or a list of them (one
    /// per piece).
    #[serde(default)]
    bed_face: Option<BedFaces>,
    /// On the print bed (mitcad#857): only how the bodies would lie, with
    /// each piece's `choices`; nothing is written.
    #[serde(default)]
    plan: bool,
    /// On the print bed (mitcad#1109): each piece's box centred over the
    /// plate (default true).
    #[serde(default)]
    center: Option<bool>,
    /// On the print bed (mitcad#1109): each piece's lowest point on the bed
    /// (default true).
    #[serde(default)]
    drop: Option<bool>,
    /// On the print bed (mitcad#1109): a piece without a usable print bed
    /// face turned onto the face the automatic orientation finds (default
    /// true).
    #[serde(default)]
    auto_orient: Option<bool>,
}

/// A print bed face chosen (mitcad#857): a body (uid or name) and the
/// occurrence of its placement (`O1/O4` or `Arm:1/Pin:2`, `""` the design
/// itself).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BedFaceChoice {
    body: String,
    #[serde(default)]
    occurrence: String,
}

/// `bed_face`: one, or one per piece.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BedFaces {
    One(BedFaceChoice),
    Many(Vec<BedFaceChoice>),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportSketchCommand {
    /// Uid or name.
    sketch: String,
    path: String,
    /// `r2000` (default) or `r12`.
    #[serde(default)]
    version: Option<String>,
}

pub(super) fn imported_json(imported: &Imported) -> Value {
    let bodies: Vec<Value> = imported
        .bodies
        .iter()
        .map(|b| json!({"uid": b.uid, "name": b.name, "kind": b.kind.as_str()}))
        .collect();
    let mut out =
        json!({"uid": imported.feature.uid, "name": imported.feature.name, "bodies": bodies});
    if !imported.warnings.is_empty() {
        out["warnings"] = json!(imported.warnings);
    }
    out
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait ExchangeOps<K: Kernel> {
    fn import_file_command(&mut self, command: ImportFileCommand) -> Result<Value, ApiError>;

    fn export_command(&self, command: ExportCommand) -> Result<Value, ApiError>;

    fn export_sketch_command(&self, command: ExportSketchCommand) -> Result<Value, ApiError>;

    /// A body at the marker by its uid or display name.
    fn body_by_uid_or_name(&self, body: &str) -> Result<BodyUid, ApiError>;

    fn export_flat_pattern_command(
        &self,
        command: ExportFlatPatternCommand,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> ExchangeOps<K> for Document<K> {
    fn import_file_command(&mut self, command: ImportFileCommand) -> Result<Value, ApiError> {
        let options = ReadOptions {
            unit_mm: command.unit_mm.unwrap_or(1.0),
        };
        let imported = self.import_file(&command.path, &options, command.name.as_deref())?;
        Ok(imported_json(&imported))
    }

    fn export_command(&self, command: ExportCommand) -> Result<Value, ApiError> {
        let format = match command.format {
            Some(format) => format,
            None => ExportFormat::of_path(&command.path).ok_or_else(|| {
                ApiError(format!(
                    "{}: unknown file type; use .step, .iges, .stl, .obj, .brep, .3mf or .usdz, or give \"format\"",
                    command.path
                ))
            })?,
        };
        let dialect = match command.dialect.as_deref() {
            None => None,
            Some(id) => {
                if format != ExportFormat::ThreeMf {
                    return Err(ApiError(
                        "a slicer's settings go into 3MF files only".to_owned(),
                    ));
                }
                Some(mitcad_model::print_keys::Dialect::from_id(id).ok_or_else(|| {
                    ApiError(format!(
                        "{id} is no slicer: bambu_studio, orcaslicer, qidi_studio, prusaslicer or cura"
                    ))
                })?)
            }
        };
        let options = ExportOptions {
            format,
            step_schema: command.schema,
            unit: command.unit,
            refinement: command.refinement,
            ascii: command.ascii,
            design_folder: command.design_folder.map(std::path::PathBuf::from),
            dialect,
        };
        let bodies = command
            .bodies
            .iter()
            .map(|body| self.body_by_uid_or_name(body))
            .collect::<Result<Vec<_>, _>>()?;
        let mut colors = BTreeMap::new();
        for (body, color) in &command.colors {
            if !color.iter().all(|c| (0.0..=1.0).contains(c)) {
                return Err(ApiError(format!(
                    "the colour of {body} must have components from 0 to 1"
                )));
            }
            colors.insert(self.body_by_uid_or_name(body)?, *color);
        }
        let within = match &command.occurrence {
            Some(path) => Some(
                self.assembly()
                    .find_path(path)
                    .ok_or_else(|| ApiError(format!("there is no occurrence {path}")))?,
            ),
            None => None,
        };
        let find = |path: &str| {
            self.assembly()
                .find_path(path)
                .ok_or_else(|| ApiError(format!("there is no occurrence {path}")))
        };
        let mut occurrences = BTreeMap::new();
        for (body, paths) in &command.occurrences {
            let uid = self.body_by_uid_or_name(body)?;
            let paths = paths
                .iter()
                .map(|p| find(p.as_str()))
                .collect::<Result<Vec<_>, _>>()?;
            occurrences.insert(uid, paths);
        }
        let whole = command
            .whole
            .iter()
            .map(|p| find(p.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        // The print bed faces chosen (mitcad#857).
        let chosen: Vec<&BedFaceChoice> = match &command.bed_face {
            None => Vec::new(),
            Some(BedFaces::One(one)) => vec![one],
            Some(BedFaces::Many(many)) => many.iter().collect(),
        };
        let mut bed_faces = Vec::with_capacity(chosen.len());
        for choice in chosen {
            let uid = self.body_by_uid_or_name(&choice.body)?;
            bed_faces.push((uid, find(choice.occurrence.as_str())?));
        }
        // The automatic orientation and the settling (mitcad#1109): on the
        // print bed only, each true unless given false.
        let rules = [command.center, command.drop, command.auto_orient];
        if !command.on_bed && rules.iter().any(Option::is_some) {
            return Err(ApiError(
                "center, drop and auto_orient are for the bodies put on the print bed (on_bed)"
                    .to_owned(),
            ));
        }
        let placement = ExportPlacement {
            coordinates: command.coordinates,
            within,
            on_bed: command.on_bed,
            occurrences,
            whole,
            bed_faces,
            plan_only: command.plan,
            auto_orient: command.auto_orient.unwrap_or(true),
            settle: Settle {
                center: command.center.unwrap_or(true),
                drop: command.drop.unwrap_or(true),
            },
        };
        let exported = self.export_bodies(&command.path, &bodies, &options, &colors, &placement)?;
        let mut result =
            json!({"path": command.path, "format": format.as_str(), "bodies": exported.bodies});
        if format == ExportFormat::ThreeMf {
            // Each body's mesh, and the bodies left out (mitcad#13).
            result["meshes"] = exported
                .bodies
                .iter()
                .zip(&exported.meshes)
                .map(|(uid, mesh)| {
                    json!({"body": uid, "name": self.body_name(*uid),
                           "triangles": mesh.triangles, "volume": mesh.volume})
                })
                .collect();
            result["skipped"] = exported
                .skipped
                .iter()
                .map(|(uid, why)| json!({"body": uid, "name": self.body_name(*uid), "reason": why}))
                .collect();
            // The slicer's settings (mitcad#640): each part's filament slot
            // (a body split by its placements' settings is several parts,
            // mitcad#1061) and what the slicer does not take as the design
            // has it.
            if let Some(dialect) = dialect {
                result["dialect"] = json!(dialect.id());
                result["filaments"] = json!(
                    exported
                        .slots
                        .iter()
                        .map(|slot| slot + 1)
                        .collect::<Vec<_>>()
                );
                result["warnings"] = json!(exported.warnings);
                // Each part's body, name, material and own values written
                // (mitcad#1052, mitcad#1061).
                result["printed"] = exported
                    .parts
                    .iter()
                    .zip(&exported.printed)
                    .map(|((uid, name), (material, own))| {
                        json!({"body": uid, "name": name,
                               "material": material, "own_values": own})
                    })
                    .collect();
            }
        }
        if format == ExportFormat::Usdz {
            // The materials, the bodies left out and what was written
            // otherwise (mitcad#123).
            result["materials"] = json!(exported.materials);
            result["skipped"] = exported
                .skipped
                .iter()
                .map(|(uid, why)| json!({"body": uid, "name": self.body_name(*uid), "reason": why}))
                .collect();
            result["warnings"] = json!(exported.warnings);
        }
        // How the bodies were put on the print bed (mitcad#731).
        if let Some(on_bed) = &exported.on_bed {
            result["bed"] = self.on_bed_json(on_bed);
            result["warnings"] = json!(exported.warnings);
        }
        Ok(result)
    }

    fn export_sketch_command(&self, command: ExportSketchCommand) -> Result<Value, ApiError> {
        let extension = std::path::Path::new(&command.path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if extension.as_deref() != Some("dxf") {
            return Err(ApiError(format!(
                "{}: sketches are exported to .dxf files",
                command.path
            )));
        }
        let version = match command.version.as_deref() {
            None | Some("r2000") => mitcad_dxf::DxfVersion::R2000,
            Some("r12") => mitcad_dxf::DxfVersion::R12,
            Some(other) => {
                return Err(ApiError(format!(
                    "unknown DXF version “{other}” (expected r2000 or r12)"
                )));
            }
        };
        let options = mitcad_dxf::WriteOptions {
            version,
            ..mitcad_dxf::WriteOptions::default()
        };
        // The sketch of a name it shares with other features (its extrude,
        // mitcad#1107); an unknown uid is reported by the export.
        let is_sketch = |def: &FeatureDef| matches!(def, FeatureDef::Sketch(_));
        let sketch = match self.feature_by_text(&command.sketch, Some(&is_sketch))? {
            Some(entry) => entry.uid,
            None => command
                .sketch
                .trim()
                .parse::<FeatureUid>()
                .map_err(|_| ApiError(format!("no feature is named {}", command.sketch)))?,
        };
        let entities = self.export_sketch_dxf(sketch, &command.path, &options)?;
        Ok(json!({"path": command.path, "entities": entities}))
    }

    fn body_by_uid_or_name(&self, body: &str) -> Result<BodyUid, ApiError> {
        if let Ok(uid) = body.parse::<BodyUid>() {
            return Ok(uid);
        }
        self.bodies()
            .into_iter()
            .find(|b| b.name == body)
            .map(|b| b.uid)
            .ok_or_else(|| ApiError(format!("no body at the timeline marker is named {body}")))
    }

    fn export_flat_pattern_command(
        &self,
        command: ExportFlatPatternCommand,
    ) -> Result<Value, ApiError> {
        use mitcad_model::sheet::flat_dxf::{self, FlatDxfOptions};
        let extension = std::path::Path::new(&command.path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if extension.as_deref() != Some("dxf") {
            return Err(ApiError(format!(
                "{}: Flat Patterns are exported to .dxf files",
                command.path
            )));
        }
        let version = match command.version.as_deref() {
            None | Some("r2000") => mitcad_dxf::DxfVersion::R2000,
            Some("r12") => mitcad_dxf::DxfVersion::R12,
            Some(other) => {
                return Err(ApiError(format!(
                    "unknown DXF version “{other}” (expected r2000 or r12)"
                )));
            }
        };
        let write = mitcad_dxf::WriteOptions {
            version,
            ..mitcad_dxf::WriteOptions::default()
        };
        let body = self.body_by_uid_or_name(&command.body)?;
        let options = FlatDxfOptions {
            unit: command.unit,
            tangents: command.tangents,
        };
        let (def, drawing) = self.export_flat_pattern_dxf(
            body,
            &command.path,
            command.reference_side,
            &options,
            &write,
        )?;
        let count = |layer: &str| drawing.entities.iter().filter(|e| e.layer == layer).count();
        let layers: Vec<Value> = drawing
            .layers
            .iter()
            .map(|l| json!({"name": l.name, "entities": count(&l.name)}))
            .collect();
        let bounds = drawing
            .bounds()
            .map(|(min, max)| json!({"min": [min.x, min.y], "max": [max.x, max.y]}));
        Ok(json!({
            "path": command.path,
            "body": body,
            "unit": command.unit,
            "stationary": def.stationary,
            "reference_side": def.reference_side,
            "entities": drawing.entities.len(),
            "layers": layers,
            "bends": {"up": count(flat_dxf::BEND_UP), "down": count(flat_dxf::BEND_DOWN)},
            "bounds": bounds,
        }))
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait ExchangeLocal<K: Kernel> {
    /// The `bed` of an export's result: the body and face on the bed, the
    /// transform after the design's placements (rows of the turn with the
    /// move last), the plate's centre, the bodies kept as assembled and
    /// the bed faces that cannot be used.
    fn on_bed_json(&self, on_bed: &OnBed) -> Value;
}

impl<K: Kernel> ExchangeLocal<K> for Document<K> {
    fn on_bed_json(&self, on_bed: &OnBed) -> Value {
        let rows_of = |transform: Option<Transform>| {
            transform.map(|t| {
                let row = |r: usize| {
                    let [x, y, z] = t.linear[r];
                    [x, y, z, t.translation[r]]
                };
                [row(0), row(1), row(2)]
            })
        };
        let rows = rows_of(on_bed.transform);
        // The bed faces a piece kept as assembled could lie on, one per
        // placement (mitcad#857): the occurrence by uids, as `bed_face`
        // takes it, and by names.
        let choices_of = |choices: &[BedChoice]| -> Vec<Value> {
            choices
                .iter()
                .map(|choice| {
                    let path = choice.path.as_deref().unwrap_or(&[]);
                    let uids: Vec<String> = path.iter().map(ToString::to_string).collect();
                    json!({
                        "body": choice.body,
                        "name": self.body_name(choice.body),
                        "face": choice.face,
                        "occurrence": uids.join("/"),
                        "occurrence_name": self.assembly().path_name(path),
                    })
                })
                .collect()
        };
        // The occurrence of the placement whose face lies on the bed
        // (mitcad#1109): by uids, as `bed_face` takes it, and by names;
        // null without a face.
        let occurrence_of = |path: &Option<Vec<mitcad_model::ids::OccurrenceUid>>| {
            path.as_ref().map(|path| {
                let uids: Vec<String> = path.iter().map(ToString::to_string).collect();
                (uids.join("/"), self.assembly().path_name(path))
            })
        };
        // The file's other objects, each on the bed by itself (mitcad#856).
        let objects: Vec<Value> = on_bed
            .objects
            .iter()
            .map(|object| {
                let at = occurrence_of(&object.occurrence);
                json!({
                    "name": object.name,
                    "body": object.face.as_ref().map(|(uid, _)| *uid),
                    "face": object.face.as_ref().map(|(_, face)| face),
                    "auto": object.auto,
                    "occurrence": at.as_ref().map(|(uids, _)| uids),
                    "occurrence_name": at.as_ref().map(|(_, name)| name),
                    "transform": rows_of(object.transform),
                    "kept": object.kept,
                    "choices": choices_of(&object.choices),
                })
            })
            .collect();
        let at = occurrence_of(&on_bed.occurrence);
        let problems: Vec<Value> = on_bed
            .problems
            .iter()
            .map(|(uid, why)| json!({"body": uid, "name": self.body_name(*uid), "reason": why}))
            .collect();
        json!({
            "body": on_bed.face.as_ref().map(|(uid, _)| *uid),
            "name": on_bed.face.as_ref().map(|(uid, _)| self.body_name(*uid)),
            "face": on_bed.face.as_ref().map(|(_, face)| face),
            "auto": on_bed.auto,
            "occurrence": at.as_ref().map(|(uids, _)| uids),
            "occurrence_name": at.as_ref().map(|(_, name)| name),
            "transform": rows,
            "center": on_bed.settle.center,
            "drop": on_bed.settle.drop,
            "plate": on_bed.plate,
            "kept": on_bed.kept,
            "problems": problems,
            "objects": objects,
            "on_faces": on_bed.on_faces,
            "choices": choices_of(&on_bed.choices),
        })
    }
}

/// `export_flat_pattern_dxf` (mitcad#210): a body's Flat Pattern as a DXF
/// drawing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportFlatPatternCommand {
    /// Uid or name of the sheet-metal body.
    body: String,
    path: String,
    /// The drawing unit (`mm` by default).
    #[serde(default)]
    unit: LengthUnit,
    /// `r2000` (default) or `r12`.
    #[serde(default)]
    version: Option<String>,
    /// Also the bends' tangent lines, on `BEND_TANGENT`.
    #[serde(default)]
    tangents: bool,
    /// The side the drawing looks at; the Flat Pattern's when left out.
    #[serde(default)]
    reference_side: Option<mitcad_model::sheet::SheetReferenceSide>,
}
