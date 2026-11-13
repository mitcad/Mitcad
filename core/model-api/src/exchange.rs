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
    OnBed, PlateFiles, ReadOptions, Refinement, Settle, StepSchema,
};
use mitcad_model::exchange::{ImportStructure, ImportedComponent, ImportedFile};
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
    /// A STEP assembly as components (the default) or one base feature
    /// (mitcad#1274).
    #[serde(default)]
    structure: ImportStructure,
    /// Find Repeated Parts on the imported base features in the same undo
    /// step (mitcad#1277).
    #[serde(default)]
    repeated_parts: bool,
    /// Into a new component named after the file, placed in the active
    /// component (true), or straight into the active component (false);
    /// by default true for a STEP, IGES or BRep file imported into a
    /// design that has content (mitcad#1629).
    #[serde(default)]
    wrap: Option<bool>,
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
    /// 3MF (mitcad#1050): the design's print plates instead of the
    /// bodies, on the print bed: every plate in one file (`one_file`) or a
    /// file per plate beside the path (`file_per_plate`).
    #[serde(default)]
    plates: Option<PlatesMode>,
    /// 3MF (mitcad#1050): one print plate (id or name) alone, in the path.
    #[serde(default)]
    plate: Option<String>,
    /// On the print bed (mitcad#1491): the clearance between two
    /// neighbouring pieces beyond their brims, mm (default 5).
    #[serde(default)]
    spacing: Option<f64>,
    /// USDZ (mitcad#1596): each placement in the colours of the filaments
    /// it prints with by height, in place of the appearances.
    #[serde(default)]
    print_colors: bool,
    /// USDZ (mitcad#1597): time-sampled placements, from poses given per
    /// occurrence or body, or from a motion study.
    #[serde(default)]
    animation: Option<AnimationInput>,
}

/// `export`'s `animation` (mitcad#1597; commands.md, *Animated USDZ*).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationInput {
    /// Frames per second.
    fps: f64,
    /// The first frame's time code (default 0); the others follow one
    /// apart.
    #[serde(default)]
    start: Option<f64>,
    /// Each frame's time code, increasing (instead of `start`).
    #[serde(default)]
    times: Option<Vec<f64>>,
    /// The coordinates of the tracks' poses: `parent` (default) or
    /// `design`.
    #[serde(default)]
    space: Option<mitcad_model::exchange::AnimationSpace>,
    /// Poses given: per occurrence or body, one transform per frame.
    #[serde(default)]
    tracks: Option<Vec<TrackInput>>,
    /// Or a motion study (name), sampled from step `from` (default 0) to
    /// `to` (default its last step) at `frames` frames (default one per
    /// step, both ends included).
    #[serde(default)]
    study: Option<String>,
    #[serde(default)]
    from: Option<f64>,
    #[serde(default)]
    to: Option<f64>,
    #[serde(default)]
    frames: Option<usize>,
}

/// A track of `animation`: an occurrence (path) or a body (uid or name)
/// and its poses.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackInput {
    #[serde(default)]
    occurrence: Option<String>,
    #[serde(default)]
    body: Option<String>,
    transforms: Vec<PoseInput>,
}

/// A pose: three or four rows of four numbers (the rotation's rows with
/// the translation last; a fourth row `[0, 0, 0, 1]`), or the same 12 or
/// 16 numbers row by row.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PoseInput {
    Rows(Vec<Vec<f64>>),
    Flat(Vec<f64>),
}

impl PoseInput {
    fn transform(&self) -> Result<Transform, String> {
        let rows: Vec<Vec<f64>> = match self {
            Self::Rows(rows) => rows.clone(),
            Self::Flat(numbers) if numbers.len() == 12 || numbers.len() == 16 => {
                numbers.chunks(4).map(<[f64]>::to_vec).collect()
            }
            Self::Flat(numbers) => {
                return Err(format!(
                    "a pose is three or four rows of four numbers, or 12 or 16 numbers, not {}",
                    numbers.len()
                ));
            }
        };
        if rows.iter().flatten().any(|v| !v.is_finite()) {
            return Err("the pose is not finite".to_owned());
        }
        mitcad_model::assembly::from_rows(&rows).map_err(|e| {
            let e = e.replace("an occurrence transform", "a pose");
            if e.contains("last row") {
                format!(
                    "{e} (a pose maps p to M·p: the translation is the last column; a matrix \
                     with the translation in its last row is transposed)"
                )
            } else {
                e
            }
        })
    }
}

/// The most frames a study is sampled at (mitcad#1597).
const MAX_STUDY_FRAMES: usize = mitcad_model::exchange::usdz_animation::MAX_FRAMES;

/// An `animation` as the model takes it: the poses given, or the study's
/// sampled without changing the document (mitcad#1597).
fn animation_of<K: Kernel>(
    doc: &mut Document<K>,
    input: &AnimationInput,
) -> Result<mitcad_model::exchange::UsdzAnimation, ApiError> {
    use mitcad_model::exchange::{
        AnimationPoses, AnimationTarget, AnimationTrack, UsdzAnimation, usdz_animation,
    };
    let err = |m: String| ApiError(format!("animation: {m}"));
    if !(input.fps.is_finite() && input.fps > 0.0) {
        return Err(err(format!(
            "fps must be a positive number of frames per second, not {}",
            input.fps
        )));
    }
    if input.start.is_some() && input.times.is_some() {
        return Err(err("give start or times, not both".to_owned()));
    }
    // The frames' time codes: given, or one apart from the start.
    let times_for = |frames: usize| -> Result<Vec<f64>, ApiError> {
        let times = match &input.times {
            Some(times) => {
                if times.len() != frames {
                    return Err(err(format!(
                        "{} times for {frames} frames: give one time code per frame",
                        times.len()
                    )));
                }
                times.clone()
            }
            None => {
                let start = input.start.unwrap_or(0.0);
                (0..frames).map(|k| start + k as f64).collect()
            }
        };
        usdz_animation::check_timing(input.fps, &times).map_err(err)?;
        Ok(times)
    };
    let poses = match (&input.tracks, &input.study) {
        (Some(_), Some(_)) => {
            return Err(err(
                "give tracks (poses per occurrence or body) or study, not both".to_owned(),
            ));
        }
        (None, None) => {
            return Err(err(
                "give tracks (poses per occurrence or body) or study (a motion study's name)"
                    .to_owned(),
            ));
        }
        (Some(tracks), None) => {
            if let Some(key) = [
                ("from", input.from.is_some()),
                ("to", input.to.is_some()),
                ("frames", input.frames.is_some()),
            ]
            .iter()
            .find_map(|(k, given)| given.then_some(*k))
            {
                return Err(err(format!(
                    "{key} is for a study; tracks have one pose per frame"
                )));
            }
            let first = tracks
                .first()
                .ok_or_else(|| err("tracks is empty: give at least one".to_owned()))?;
            let frames = first.transforms.len();
            let mut made = Vec::with_capacity(tracks.len());
            for (i, track) in tracks.iter().enumerate() {
                let target = match (&track.occurrence, &track.body) {
                    (Some(path), None) => AnimationTarget::Occurrence(
                        doc.assembly()
                            .find_path(path)
                            .ok_or_else(|| err(format!("there is no occurrence {path}")))?,
                    ),
                    (None, Some(body)) => {
                        AnimationTarget::Body(doc.body_by_uid_or_name(body).map_err(|e| err(e.0))?)
                    }
                    _ => {
                        return Err(err(format!(
                            "track {i} needs an occurrence (a path like Arm:1/Pin:1) or a body \
                             (a uid or name), one of them"
                        )));
                    }
                };
                let label = track
                    .occurrence
                    .as_deref()
                    .or(track.body.as_deref())
                    .unwrap_or_default();
                if track.transforms.len() != frames {
                    return Err(err(format!(
                        "{label} has {} poses, the first track {frames}: every track has one \
                         pose per frame",
                        track.transforms.len()
                    )));
                }
                let transforms = track
                    .transforms
                    .iter()
                    .enumerate()
                    .map(|(k, pose)| {
                        pose.transform()
                            .map_err(|e| err(format!("{label}, frame {k}: {e}")))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                made.push(AnimationTrack { target, transforms });
            }
            if frames == 0 {
                return Err(err("a track needs at least one pose".to_owned()));
            }
            let times = times_for(frames)?;
            UsdzAnimation {
                fps: input.fps,
                times,
                poses: AnimationPoses::Tracks {
                    space: input.space.unwrap_or_default(),
                    tracks: made,
                },
            }
        }
        (None, Some(study)) => {
            if input.space.is_some() {
                return Err(err(
                    "space is for tracks; a study's placements are its own".to_owned()
                ));
            }
            let steps = f64::from(doc.motion_study(study)?.steps);
            let from = input.from.unwrap_or(0.0);
            let to = input.to.unwrap_or(steps);
            if !(from.is_finite() && to.is_finite()) {
                return Err(err("from and to must be finite steps".to_owned()));
            }
            let frames = match input.frames {
                Some(frames) => frames,
                None => ((to - from).abs().round() as usize).saturating_add(1),
            };
            if frames == 0 || frames > MAX_STUDY_FRAMES {
                return Err(err(format!(
                    "frames must be 1 to {MAX_STUDY_FRAMES}, not {frames}"
                )));
            }
            let times = times_for(frames)?;
            let at: Vec<f64> = (0..frames)
                .map(|k| {
                    if frames == 1 {
                        from
                    } else {
                        from + (to - from) * k as f64 / (frames - 1) as f64
                    }
                })
                .collect();
            let placements = doc.study_placements(study, &at)?;
            UsdzAnimation {
                fps: input.fps,
                times,
                poses: AnimationPoses::Study {
                    name: study.clone(),
                    placements,
                },
            }
        }
    };
    Ok(poses)
}

/// `plates` (mitcad#1050).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PlatesMode {
    OneFile,
    FilePerPlate,
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
    /// The file's length unit (`mm`, `in`, …); the design's when left out
    /// (mitcad#1408).
    #[serde(default)]
    units: Option<String>,
    /// Also write the construction curves and center lines, on the layer
    /// `construction` (mitcad#1445); left out by default.
    #[serde(default)]
    construction: bool,
}

/// The length unit a DXF export writes (mitcad#1408): its `units`, or the
/// design's when it names none.
pub(super) fn dxf_unit<K: Kernel>(
    doc: &Document<K>,
    units: Option<&str>,
) -> Result<mitcad_model::expr::LengthUnit, ApiError> {
    let design = doc.parameters().context().default_length_unit;
    mitcad_model::exchange::dxf_export_unit(units, design).map_err(ApiError)
}

/// An export's warnings as a person reads them (mitcad#1727,
/// `docs/ui-conventions.md` "What a message says"): where a warning of
/// `warnings` is written for agents (the missing printer profile, naming
/// `import_print_profile`), `warning_messages` beside it gives every
/// warning in plain words, which the application shows; `warnings` keeps
/// the agents' texts.
fn with_shown_warnings(result: &mut Value) {
    use mitcad_model::print_files::{NO_PRINTER_PROFILE, NO_PRINTER_PROFILE_MESSAGE};
    let Some(warnings) = result.get("warnings").and_then(Value::as_array) else {
        return;
    };
    if !warnings
        .iter()
        .any(|w| w.as_str() == Some(NO_PRINTER_PROFILE))
    {
        return;
    }
    let shown: Vec<Value> = warnings
        .iter()
        .map(|w| match w.as_str() {
            Some(NO_PRINTER_PROFILE) => json!(NO_PRINTER_PROFILE_MESSAGE),
            _ => w.clone(),
        })
        .collect();
    result["warning_messages"] = Value::Array(shown);
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

/// The reply of an `import_file` that kept a STEP assembly (mitcad#1274):
/// the first base feature's `uid` and `name` and every body it and the
/// others made, as for one base feature, and the components.
fn components_json(components: &[ImportedComponent], warnings: &[String]) -> Value {
    let first = components.iter().find_map(|c| c.feature.as_ref());
    let bodies: Vec<Value> = components
        .iter()
        .filter_map(|c| c.feature.as_ref())
        .flat_map(|f| &f.bodies)
        .map(|b| json!({"uid": b.uid, "name": b.name, "kind": b.kind.as_str()}))
        .collect();
    let list: Vec<Value> = components
        .iter()
        .map(|c| {
            json!({
                "uid": c.component.to_string(),
                "name": c.name,
                "occurrences": c.occurrences.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "feature": c.feature.as_ref().map(|f| f.feature.uid.to_string()),
                "bodies": c.feature.as_ref().map_or(0, |f| f.bodies.len()),
            })
        })
        .collect();
    let mut out = json!({
        "uid": first.map(|f| f.feature.uid.to_string()),
        "name": first.map(|f| f.feature.name.clone()),
        "bodies": bodies,
        "structure": "components",
        "components": list,
        "occurrences": components.iter().map(|c| c.occurrences.len()).sum::<usize>(),
    });
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
    out
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait ExchangeOps<K: Kernel> {
    fn import_file_command(&mut self, command: ImportFileCommand) -> Result<Value, ApiError>;

    /// `&mut`: an animation from a motion study evaluates its steps with
    /// the document's cache (mitcad#1597); nothing else changes.
    fn export_command(&mut self, command: ExportCommand) -> Result<Value, ApiError>;

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
        let depth = self.undo_depth();
        // A file into a design with content: in a component of its own
        // named after the file (mitcad#1629).
        let wrap = command
            .wrap
            .unwrap_or_else(|| self.import_wraps_by_default(&command.path));
        let (imported, wrapper) = if wrap {
            let (imported, wrapper) = self.import_file_wrapped(
                &command.path,
                &options,
                command.name.as_deref(),
                command.structure,
            )?;
            (imported, Some(wrapper))
        } else {
            let imported = self.import_file_structured(
                &command.path,
                &options,
                command.name.as_deref(),
                command.structure,
            )?;
            (imported, None)
        };
        // The base features Find Repeated Parts runs on (mitcad#1277): the
        // one base feature, or each part's of the components.
        let bases: Vec<FeatureUid> = match &imported {
            ImportedFile::Base(imported) => vec![imported.feature.uid],
            ImportedFile::Components { components, .. } => components
                .iter()
                .filter_map(|c| c.feature.as_ref().map(|f| f.feature.uid))
                .collect(),
        };
        let mut out = match &imported {
            ImportedFile::Base(imported) => imported_json(imported),
            ImportedFile::Components {
                components,
                warnings,
            } => components_json(components, warnings),
        };
        if let Some(wrapper) = &wrapper {
            out["component"] = json!({
                "uid": wrapper.component.to_string(),
                "name": wrapper.name,
                "occurrence": wrapper.occurrence.to_string(),
            });
        }
        // Repeated parts (mitcad#1277): the copies among the imported
        // bodies made occurrences of one component, in the import's undo
        // step; a failure takes the import back.
        if command.repeated_parts {
            let label = self.undo_label().unwrap_or("Import").to_owned();
            let mut found = mitcad_model::RepeatedParts {
                parts: Vec::new(),
                kept: Vec::new(),
                combines: Vec::new(),
            };
            for base in bases {
                match self.find_repeated_parts(base, None) {
                    Ok(mut one) => {
                        found.parts.append(&mut one.parts);
                        found.kept.append(&mut one.kept);
                        found.combines.append(&mut one.combines);
                    }
                    Err(error) => {
                        self.merge_undo(depth, &label);
                        self.undo();
                        return Err(error.into());
                    }
                }
            }
            self.merge_undo(depth, &label);
            out["repeated_parts"] = super::components::repeated_parts_json(&found);
        }
        Ok(out)
    }

    fn export_command(&mut self, command: ExportCommand) -> Result<Value, ApiError> {
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
        // The print plates (mitcad#1050): on the print bed.
        let plates = match (command.plates, &command.plate) {
            (Some(_), Some(_)) => {
                return Err(ApiError(
                    "plates writes every plate, plate one of them: give one".to_owned(),
                ));
            }
            (Some(PlatesMode::OneFile), None) => Some(PlateFiles::OneFile),
            (Some(PlatesMode::FilePerPlate), None) => Some(PlateFiles::FilePerPlate),
            (None, Some(plate)) => Some(PlateFiles::Only(self.plate_index(plate)?)),
            (None, None) => None,
        };
        let on_bed = command.on_bed || plates.is_some();
        // The automatic orientation and the settling (mitcad#1109): on the
        // print bed only, each true unless given false; `center` also for a
        // slicer's 3MF, which is centred on the plate without `on_bed` too
        // (mitcad#1596).
        let centre_alone = dialect.is_some();
        let rules = [
            command.center.filter(|_| !centre_alone),
            command.drop,
            command.auto_orient,
        ];
        if !on_bed && (rules.iter().any(Option::is_some) || command.spacing.is_some()) {
            return Err(ApiError(
                "center, drop, auto_orient and spacing are for the bodies put on the print bed \
                 (on_bed)"
                    .to_owned(),
            ));
        }
        // The clearance between neighbouring pieces (mitcad#1491).
        if let Some(spacing) = command.spacing
            && !(spacing.is_finite() && (0.0..=1000.0).contains(&spacing))
        {
            return Err(ApiError(format!(
                "spacing is a clearance in mm from 0 to 1000, not {spacing}"
            )));
        }
        // An animation (mitcad#1597): USDZ only; a study's steps sampled
        // without changing the document.
        let animation = match &command.animation {
            Some(_) if format != ExportFormat::Usdz => {
                return Err(ApiError(
                    "animation is for USDZ export (format usdz)".to_owned(),
                ));
            }
            Some(input) => Some(animation_of(self, input)?),
            None => None,
        };
        let placement = ExportPlacement {
            coordinates: command.coordinates,
            within,
            on_bed,
            occurrences,
            whole,
            bed_faces,
            plan_only: command.plan,
            auto_orient: command.auto_orient.unwrap_or(true),
            settle: Settle {
                center: command.center.unwrap_or(true),
                drop: command.drop.unwrap_or(true),
            },
            plates,
            spacing: command.spacing,
            animation,
            print_colors: command.print_colors,
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
                // The filament changes and pauses at print heights written
                // (mitcad#1596).
                if !exported.layer_events.is_empty() {
                    result["filament_changes"] = exported
                        .layer_events
                        .iter()
                        .map(|e| {
                            let mut event = json!({"plate": e.plate, "height": e.height,
                                   "print_z": e.print_z, "layer": e.layer,
                                   "kind": e.kind.as_str(), "material": e.material});
                            // At a plane or face (mitcad#1643): the height
                            // worked out, its source and the body measured
                            // from.
                            if let Some(at) = &e.at {
                                let at = json!(at);
                                let face = at.get("face").is_some();
                                event["source"] = json!(if face { "face" } else { "plane" });
                                event["at"] = at;
                                event["body"] = json!(e.body);
                            } else {
                                event["source"] = json!("height");
                            }
                            event
                        })
                        .collect();
                }
            } else if !exported.warnings.is_empty() {
                // Filament changes without a slicer (mitcad#1596).
                result["warnings"] = json!(exported.warnings);
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
            // The frames and what moves (mitcad#1597).
            if let Some(animation) = &exported.animation {
                result["animation"] = json!({
                    "fps": animation.fps,
                    "start": animation.start,
                    "end": animation.end,
                    "frames": animation.frames,
                    "moved": animation.moved,
                });
            }
        }
        // How the bodies were put on the print bed (mitcad#731).
        if let Some(on_bed) = &exported.on_bed {
            result["bed"] = self.on_bed_json(on_bed);
            result["warnings"] = json!(exported.warnings);
        }
        // Each print plate as written, and the files (mitcad#1050).
        if plates.is_some() {
            let mut files: Vec<&str> = Vec::new();
            let written: Vec<Value> = exported
                .plates
                .iter()
                .map(|plate| {
                    if let Some(path) = plate.path.as_deref()
                        && !files.contains(&path)
                    {
                        files.push(path);
                    }
                    json!({
                        "id": plate.id,
                        "name": plate.name,
                        "path": plate.path,
                        "bodies": plate.bodies,
                        "pieces": plate.pieces,
                        "origin": plate.origin,
                        "bed": plate.on_bed.as_ref().map(|b| self.on_bed_json(b)),
                        "warnings": plate.warnings,
                    })
                })
                .collect();
            result["plates"] = json!(written);
            result["files"] = json!(files);
            result["warnings"] = json!(exported.warnings);
        }
        with_shown_warnings(&mut result);
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
        let unit = dxf_unit(self, command.units.as_deref())?;
        let entities =
            self.export_sketch_dxf(sketch, &command.path, unit, command.construction, &options)?;
        let mut result =
            json!({"path": command.path, "entities": entities, "units": unit.symbol()});
        // Whether construction geometry was written (mitcad#1445).
        result["construction"] = json!(command.construction);
        Ok(result)
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
        let unit = dxf_unit(self, command.units.as_deref())?;
        let options = FlatDxfOptions {
            unit,
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
            "unit": unit.symbol(),
            "units": unit.symbol(),
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
    /// The drawing unit (`mm`, `in`, …); the design's when left out
    /// (mitcad#1408). `unit` is its older name.
    #[serde(default, alias = "unit")]
    units: Option<String>,
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
