// SPDX-License-Identifier: MIT
//! The application interface: JSON commands, queries and previews (see
//! `commands.md` next to this file). The application and `mitcad-cli` use
//! only this and the shape handles, so a new command needs no change to the
//! C++ bridge.

mod analysis;
// Components and occurrences (F6).
mod components;
mod exchange;
mod report;
mod script;
// Profile features (F1).
mod threads;
// The general sketch (S1).
mod sketch;
// Value inputs of command panels (U1).
mod evaluate;
// The caches of computed results (P7d).
mod cache;
// Joints between occurrences (mitcad#55).
mod joints;
// Configuration tables and library parts (mitcad#64).
mod libraries;

use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::document::{
    AnalysisDef, Document, ModelError, NamedView, PreviewReport, status_message, status_text,
};
use crate::expr::{DEFAULT_DECIMALS, Unit, value_to_expression};
use crate::features::{FeatureDef, ValueInput};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::profile::{ProfileRegion, SketchFrame};

/// A rejected command or query; the document is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(pub String);

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ApiError {}

impl From<ModelError> for ApiError {
    fn from(error: ModelError) -> Self {
        Self(error.to_string())
    }
}

fn parse<T: for<'de> Deserialize<'de>>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|e| ApiError(format!("invalid {what}: {e}")))
}

fn read_json(json: &str) -> Result<Value, ApiError> {
    serde_json::from_str(json).map_err(|e| ApiError(format!("not valid JSON: {e}")))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    AddFeature {
        def: FeatureDef<ValueInput>,
        #[serde(default)]
        name: Option<String>,
        /// The component (uid or name); the active one when left out.
        #[serde(default)]
        component: Option<String>,
    },
    EditFeature {
        uid: FeatureUid,
        def: FeatureDef<ValueInput>,
    },
    DeleteFeature {
        uid: FeatureUid,
        #[serde(default)]
        dependents: bool,
    },
    SuppressFeature {
        uid: FeatureUid,
        #[serde(default = "yes")]
        suppressed: bool,
    },
    ReorderFeature {
        uid: FeatureUid,
        index: usize,
    },
    RenameFeature {
        uid: FeatureUid,
        name: String,
    },
    RenameBody {
        uid: BodyUid,
        name: String,
    },
    SetMarker {
        position: usize,
    },
    AddParameter {
        name: String,
        #[serde(default)]
        value: Option<ValueInput>,
        #[serde(default)]
        expression: Option<String>,
        #[serde(default)]
        unit: Option<String>,
        #[serde(default)]
        comment: String,
    },
    SetParameter {
        name: String,
        #[serde(default)]
        value: Option<ValueInput>,
        #[serde(default)]
        expression: Option<String>,
        #[serde(default)]
        unit: Option<String>,
        #[serde(default)]
        comment: Option<String>,
        /// A favourite (P9: Change Parameters' star).
        #[serde(default)]
        favorite: Option<bool>,
    },
    SetUnits {
        length: String,
    },
    RenameParameter {
        name: String,
        new_name: String,
    },
    DeleteParameter {
        name: String,
    },
    Undo,
    Redo,
    Recompute,
    // Body attributes (F5).
    SetBodyVisible {
        uid: BodyUid,
        visible: bool,
    },
    SetBodyMaterial {
        uid: BodyUid,
        #[serde(default)]
        material: Option<String>,
    },
    SetBodyAppearance {
        uid: BodyUid,
        #[serde(default)]
        appearance: Option<String>,
    },
    // Appearances of single faces (mitcad#53).
    SetFaceAppearance {
        uid: BodyUid,
        faces: Vec<String>,
        #[serde(default)]
        appearance: Option<String>,
    },
    ClearFaceAppearances {
        uid: BodyUid,
    },
    ImportFile(exchange::ImportFileCommand),
    Export(exchange::ExportCommand),
    ExportSketch(exchange::ExportSketchCommand),
    // Sketch mode (U2): one undo step for an interactive operation made of
    // several commands (a tool's curves and constraints, a drag).
    MergeUndo {
        depth: usize,
        label: String,
    },
    // Browser and timeline (U3): the light bulb of sketches and
    // construction features.
    SetFeatureVisible {
        uid: FeatureUid,
        visible: bool,
    },
    // Named views (U5).
    AddNamedView {
        /// The next free `NamedView<n>` when left out.
        #[serde(default)]
        name: String,
        eye: [f64; 3],
        target: [f64; 3],
        up: [f64; 3],
        #[serde(default)]
        perspective: bool,
        height: f64,
        /// Replace a view of the same name instead of refusing.
        #[serde(default)]
        replace: bool,
    },
    DeleteNamedView {
        name: String,
    },
    RenameNamedView {
        name: String,
        new_name: String,
    },
    // The Origin folder and Isolate saved in the document (P9).
    SetOriginVisible {
        visible: bool,
    },
    SetIsolation {
        #[serde(default)]
        items: Vec<crate::document::Isolated>,
    },
    // Timeline groups (P9).
    GroupFeatures {
        features: Vec<FeatureUid>,
        #[serde(default)]
        name: Option<String>,
    },
    Ungroup {
        name: String,
    },
    RenameGroup {
        name: String,
        new_name: String,
    },
    // The saved state (P8): the project file was written or read.
    MarkSaved,
    // The caches of computed results (P7d, `cache.rs`).
    ClearCache,
    // Analyses kept in the document (mitcad#41, `document/analyses.rs`).
    AddAnalysis {
        def: AnalysisDef,
        /// The next free `Section<n>` when left out.
        #[serde(default)]
        name: Option<String>,
    },
    EditAnalysis {
        name: String,
        def: AnalysisDef,
        /// Shows (hiding the others) or hides it; as it was when left out.
        #[serde(default)]
        visible: Option<bool>,
    },
    SetAnalysisVisible {
        name: String,
        visible: bool,
    },
    RenameAnalysis {
        name: String,
        new_name: String,
    },
    DeleteAnalysis {
        name: String,
    },
    // Appearances kept in the document (mitcad#46, `document/appearances.rs`).
    CreateAppearance(crate::appearance::AppearanceChange),
    EditAppearance(crate::appearance::AppearanceChange),
    DeleteAppearance {
        id: String,
    },
    // Render settings of the document (mitcad#47).
    SetRenderSettings(crate::render_settings::RenderSettingsChange),
    ResetRenderSettings,
    // Lights of the render settings (mitcad#54): the light's fields.
    AddRenderLight(serde_json::Map<String, Value>),
    EditRenderLight(serde_json::Map<String, Value>),
    DeleteRenderLight {
        id: String,
    },
}

fn yes() -> bool {
    true
}

/// A parameter value in a command: a number (millimetres or radians) or
/// an expression text.
enum Expression {
    Number(f64),
    Text(String),
}

/// The `value` (number or expression) or `expression` of a command.
fn expression_of(
    value: Option<ValueInput>,
    expression: Option<String>,
) -> Result<Expression, ApiError> {
    match (value, expression) {
        (Some(ValueInput::Number(number)), None) => Ok(Expression::Number(number)),
        (Some(ValueInput::Name(text)), None) | (None, Some(text)) => Ok(Expression::Text(text)),
        (Some(_), Some(_)) => Err(ApiError(
            "give either \"value\" or \"expression\", not both".to_owned(),
        )),
        (None, None) => Err(ApiError(
            "\"value\" or \"expression\" is missing".to_owned(),
        )),
    }
}

fn parse_unit(text: &str) -> Result<Unit, ApiError> {
    Unit::parse(text).map_err(|e| ApiError(format!("unit '{text}': {e}")))
}

/// A command that a preview can evaluate.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
enum PreviewCommand {
    AddFeature {
        def: FeatureDef<ValueInput>,
        #[serde(default)]
        name: Option<String>,
    },
    EditFeature {
        uid: FeatureUid,
        def: FeatureDef<ValueInput>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
enum Query {
    Document,
    Timeline,
    // Value inputs of command panels (U1, `evaluate.rs`).
    Evaluate(evaluate::EvaluateQuery),
    Parameters,
    Bodies {
        #[serde(default)]
        properties: bool,
        // Only the volume of the properties (W1: the application's log of
        // what a command changed, without listing faces and edges).
        #[serde(default)]
        volumes: bool,
    },
    Profiles {
        // A hash of each region's geometry and sketch frame (W1: the
        // application keeps the faces it built while it stays the same).
        #[serde(default)]
        hashes: bool,
    },
    Feature {
        uid: FeatureUid,
    },
    Faces {
        body: BodyUid,
    },
    Edges {
        body: BodyUid,
    },
    Report,
    // Construction geometry and analysis (F5, `analysis.rs`).
    Datums(analysis::DatumsQuery),
    Datum(analysis::DatumQuery),
    Properties(analysis::PropertiesQuery),
    Measure(analysis::MeasureQuery),
    Interference(analysis::InterferenceQuery),
    Section(analysis::SectionQuery),
    CompareStep(analysis::CompareStepQuery),
    Threads,
    // The general sketch (S1, `sketch.rs`).
    Sketch {
        uid: FeatureUid,
    },
    // Components and occurrences (F6, `components.rs`).
    Components,
    Instances(components::InstancesQuery),
    // Browser and timeline (U3).
    Dependents {
        uid: FeatureUid,
    },
    CanReorder {
        uid: FeatureUid,
        index: usize,
    },
    // Named views (U5).
    NamedViews,
    // Recompute speed (P10).
    RecomputeTimes,
    // Small UI gaps and metric defaults (P9, P11).
    ThreadSizes {
        #[serde(default)]
        standard: Option<crate::features::ThreadStandard>,
    },
    DxfInfo {
        path: String,
    },
    // The caches of computed results (P7d, `cache.rs`).
    Cache {
        /// Also what the result store on disk holds (reads its files).
        #[serde(default)]
        disk: bool,
    },
    // Saving a version (P12d): what changed since the saved state.
    ChangesSinceSaved,
    // Analyses kept in the document (mitcad#41).
    Analyses,
    // Appearances: the library's and the document's (mitcad#46).
    Appearances,
    // An appearance's embedded texture image (mitcad#53).
    AppearanceImage {
        id: String,
    },
    // Render settings of the document (mitcad#47).
    RenderSettings,
    // Joints between occurrences (mitcad#55, `joints.rs`).
    Joints,
    JointDof(joints::JointDofQuery),
    JointDrag(joints::JointDragQuery),
    // The frame a joint origin gives (mitcad#55 phase 3: the panels show
    // where an origin snaps).
    JointFrame(joints::JointFrameQuery),
    // Configuration tables and library parts (mitcad#64).
    Configurations,
    LibraryParts,
    PartsList,
}

fn names(names: &[String]) -> Value {
    json!(names)
}

impl<K: Kernel> Document<K> {
    /// Runs a JSON command, or an array of them in order (each its own undo
    /// step, stopping at the first failure). Returns the result as JSON.
    pub fn command(&mut self, json: &str) -> Result<String, ApiError> {
        let value = read_json(json)?;
        let result = match value {
            Value::Array(commands) => {
                let mut results = Vec::with_capacity(commands.len());
                for (i, command) in commands.into_iter().enumerate() {
                    results.push(
                        self.run_command(command)
                            .map_err(|e| ApiError(format!("commands[{i}]: {e}")))?,
                    );
                }
                Value::Array(results)
            }
            command => self.run_command(command)?,
        };
        Ok(result.to_string())
    }

    pub(crate) fn run_command(&mut self, value: Value) -> Result<Value, ApiError> {
        let recomputes = self.recompute_count();
        let is_sketch = value
            .get("cmd")
            .and_then(Value::as_str)
            .is_some_and(|cmd| cmd.starts_with("sketch."));
        let is_component = value
            .get("cmd")
            .and_then(Value::as_str)
            .is_some_and(|cmd| components::COMMANDS.contains(&cmd));
        // Joints between occurrences (mitcad#55).
        let is_joint = value
            .get("cmd")
            .and_then(Value::as_str)
            .is_some_and(|cmd| joints::COMMANDS.contains(&cmd));
        // Configuration tables and library parts (mitcad#64).
        let is_library = value
            .get("cmd")
            .and_then(Value::as_str)
            .is_some_and(|cmd| libraries::COMMANDS.contains(&cmd));
        let mut result = if is_sketch {
            let command = sketch::parse_sketch_command(value)?;
            self.run_sketch_command(command)?
        } else if is_component {
            self.run_component_command(parse(value, "command")?)?
        } else if is_joint {
            self.run_joint_command(parse(value, "command")?)?
        } else if is_library {
            self.run_library_command(parse(value, "command")?)?
        } else {
            self.run_document_command(parse(value, "command")?)?
        };
        let stats = self.stats();
        let recomputed = self.recompute_count() != recomputes;
        result["recomputed"] = json!(if recomputed { stats.evaluated.len() } else { 0 });
        // Results read from the result store (P7d), when there were any.
        if recomputed && !stats.restored.is_empty() {
            result["restored"] = json!(stats.restored.len());
        }
        result["error"] = json!(stats.error.map(|e| e.to_string()));
        Ok(result)
    }

    fn run_document_command(&mut self, command: Command) -> Result<Value, ApiError> {
        Ok(match command {
            Command::AddFeature {
                def,
                name,
                component,
            } => {
                let component = component
                    .as_deref()
                    .map(|c| self.component_ref(c))
                    .transpose()?;
                let added = self.add_feature_to(&def, name.as_deref(), component)?;
                json!({"uid": added.uid, "name": added.name, "parameters": names(&added.parameters)})
            }
            Command::EditFeature { uid, def } => {
                let created = self.edit_feature(uid, &def)?;
                json!({"parameters": names(&created)})
            }
            Command::DeleteFeature { uid, dependents } => {
                let deleted = self.delete_feature(uid, dependents)?;
                json!({"deleted": deleted})
            }
            Command::SuppressFeature { uid, suppressed } => {
                self.set_suppressed(uid, suppressed)?;
                json!({})
            }
            Command::ReorderFeature { uid, index } => {
                self.move_feature(uid, index)?;
                json!({})
            }
            Command::RenameFeature { uid, name } => {
                self.rename_feature(uid, &name)?;
                json!({})
            }
            Command::RenameBody { uid, name } => {
                self.rename_body(uid, &name)?;
                json!({})
            }
            Command::SetMarker { position } => {
                self.set_marker(position)?;
                json!({})
            }
            Command::AddParameter {
                name,
                value,
                expression,
                unit,
                comment,
            } => {
                let unit = unit.as_deref().map(parse_unit).transpose()?;
                match expression_of(value, expression)? {
                    Expression::Number(value) => {
                        let unit = unit.unwrap_or(Unit::of_length(
                            self.parameters().context().default_length_unit,
                        ));
                        let text = value_to_expression(value, unit);
                        self.add_parameter_expression(&name, &text, Some(unit), &comment)?;
                    }
                    Expression::Text(text) => {
                        self.add_parameter_expression(&name, &text, unit, &comment)?;
                    }
                }
                json!({})
            }
            Command::SetParameter {
                name,
                value,
                expression,
                unit,
                comment,
                favorite,
            } => {
                let unit = unit.as_deref().map(parse_unit).transpose()?;
                let mut changed = Vec::new();
                if value.is_some() || expression.is_some() || unit.is_some() {
                    changed = match expression_of(value, expression) {
                        Ok(Expression::Number(value)) if unit.is_none() => {
                            let before: Vec<f64> =
                                self.parameters().iter().map(|p| p.value()).collect();
                            self.set_parameter(&name, value)?;
                            self.parameters()
                                .iter()
                                .zip(before)
                                .filter(|(p, old)| p.value() != *old)
                                .map(|(p, _)| p.name().to_owned())
                                .collect()
                        }
                        Ok(Expression::Number(value)) => {
                            let unit = unit.expect("checked");
                            let text = value_to_expression(value, unit);
                            self.set_parameter_expression(&name, &text, Some(unit))?
                        }
                        Ok(Expression::Text(text)) => {
                            self.set_parameter_expression(&name, &text, unit)?
                        }
                        // Only a new unit: the expression stays.
                        Err(_) => {
                            let expression = self
                                .parameters()
                                .find(&name)
                                .and_then(|id| self.parameters().get(id))
                                .map(|p| p.expression().to_owned())
                                .ok_or_else(|| ApiError(format!("unknown parameter '{name}'")))?;
                            self.set_parameter_expression(&name, &expression, unit)?
                        }
                    };
                }
                if let Some(comment) = comment {
                    self.set_parameter_comment(&name, &comment)?;
                }
                if let Some(favorite) = favorite {
                    self.set_parameter_favorite(&name, favorite)?;
                }
                json!({"changed": changed})
            }
            Command::SetUnits { length } => {
                let unit = parse_unit(&length)?;
                let length = match (unit.length(), unit.angle()) {
                    (Some((length, 1)), None) => length,
                    _ => return Err(ApiError(format!("'{length}' is not a length unit"))),
                };
                let changed = self.set_units(length)?;
                json!({"changed": changed})
            }
            Command::RenameParameter { name, new_name } => {
                self.rename_parameter(&name, &new_name)?;
                json!({})
            }
            Command::DeleteParameter { name } => {
                self.delete_parameter(&name)?;
                json!({})
            }
            // A cancelled computation (P7) rejects these too.
            Command::Undo => json!({"label": self.try_undo()?}),
            Command::Redo => json!({"label": self.try_redo()?}),
            Command::Recompute => {
                self.try_recompute()?;
                json!({})
            }
            Command::SetBodyVisible { uid, visible } => {
                self.set_body_visible(uid, visible)?;
                json!({})
            }
            Command::SetBodyMaterial { uid, material } => {
                self.set_body_material(uid, material.as_deref())?;
                json!({})
            }
            Command::SetBodyAppearance { uid, appearance } => {
                self.set_body_appearance(uid, appearance.as_deref())?;
                json!({})
            }
            Command::SetFaceAppearance {
                uid,
                faces,
                appearance,
            } => {
                self.set_face_appearance(uid, &faces, appearance.as_deref())?;
                json!({})
            }
            Command::ClearFaceAppearances { uid } => {
                self.clear_face_appearances(uid)?;
                json!({})
            }
            Command::ImportFile(command) => self.import_file_command(command)?,
            Command::Export(command) => self.export_command(command)?,
            Command::ExportSketch(command) => self.export_sketch_command(command)?,
            Command::MergeUndo { depth, label } => {
                if depth > self.undo_depth() {
                    return Err(ApiError(format!(
                        "undo depth {depth} is past the last step ({})",
                        self.undo_depth()
                    )));
                }
                self.merge_undo(depth, &label);
                json!({"undo_depth": self.undo_depth()})
            }
            Command::SetFeatureVisible { uid, visible } => {
                self.set_feature_visible(uid, visible)?;
                json!({})
            }
            // Named views (U5).
            Command::AddNamedView {
                name,
                eye,
                target,
                up,
                perspective,
                height,
                replace,
            } => {
                let view = NamedView {
                    name,
                    eye,
                    target,
                    up,
                    perspective,
                    height,
                };
                json!({"name": self.add_named_view(view, replace)?})
            }
            Command::DeleteNamedView { name } => {
                self.delete_named_view(&name)?;
                json!({})
            }
            Command::RenameNamedView { name, new_name } => {
                self.rename_named_view(&name, &new_name)?;
                json!({})
            }
            // The Origin folder and Isolate (P9).
            Command::SetOriginVisible { visible } => {
                self.set_origin_visible(visible)?;
                json!({})
            }
            Command::SetIsolation { items } => {
                self.set_isolation(items)?;
                json!({})
            }
            // Timeline groups (P9).
            Command::GroupFeatures { features, name } => {
                json!({"name": self.group_features(&features, name.as_deref())?})
            }
            Command::Ungroup { name } => {
                self.ungroup(&name)?;
                json!({})
            }
            Command::RenameGroup { name, new_name } => {
                self.rename_group(&name, &new_name)?;
                json!({})
            }
            // The saved state (P8).
            Command::MarkSaved => {
                self.mark_saved();
                json!({"revision": self.revision()})
            }
            // The caches (P7d).
            Command::ClearCache => {
                let (results, bytes) = self.clear_memory_cache();
                json!({"results": results, "bytes": bytes})
            }
            // Analyses kept in the document (mitcad#41).
            Command::AddAnalysis { def, name } => {
                json!({"name": self.add_analysis(def, name.as_deref())?})
            }
            Command::EditAnalysis { name, def, visible } => {
                self.edit_analysis(&name, def, visible)?;
                json!({})
            }
            Command::SetAnalysisVisible { name, visible } => {
                self.set_analysis_visible(&name, visible)?;
                json!({})
            }
            Command::RenameAnalysis { name, new_name } => {
                self.rename_analysis(&name, &new_name)?;
                json!({})
            }
            Command::DeleteAnalysis { name } => {
                self.delete_analysis(&name)?;
                json!({})
            }
            // Appearances kept in the document (mitcad#46).
            Command::CreateAppearance(change) => {
                json!({"id": self.create_appearance(&change)?})
            }
            Command::EditAppearance(change) => {
                let id = change.id.clone().ok_or_else(|| {
                    ApiError("edit_appearance needs the appearance's id".to_owned())
                })?;
                self.edit_appearance(&id, &change)?;
                json!({})
            }
            Command::DeleteAppearance { id } => {
                let deleted = self.delete_appearance(&id)?;
                let faces: Vec<Value> = deleted
                    .faces
                    .iter()
                    .map(|(body, face)| json!({"body": body, "face": face}))
                    .collect();
                json!({"bodies": deleted.bodies, "faces": faces})
            }
            // Render settings of the document (mitcad#47).
            Command::SetRenderSettings(change) => {
                json!({"changed": self.set_render_settings(&change)?})
            }
            Command::ResetRenderSettings => {
                json!({"changed": self.reset_render_settings()?})
            }
            Command::AddRenderLight(fields) => {
                json!({"id": self.add_render_light(&fields)?})
            }
            Command::EditRenderLight(mut fields) => {
                let Some(Value::String(id)) = fields.remove("id") else {
                    return Err(ApiError(
                        "edit_render_light needs the light's id".to_owned(),
                    ));
                };
                json!({"changed": self.edit_render_light(&id, &fields)?})
            }
            Command::DeleteRenderLight { id } => {
                self.delete_render_light(&id)?;
                json!({})
            }
        })
    }

    /// Evaluates an `add_feature` or `edit_feature` command without
    /// committing it; the shapes are then available as preview handles.
    pub fn preview(&mut self, json: &str) -> Result<String, ApiError> {
        let command: PreviewCommand = parse(read_json(json)?, "preview command")?;
        let report = match command {
            PreviewCommand::AddFeature { def, name } => self.preview_add(&def, name.as_deref())?,
            PreviewCommand::EditFeature { uid, def } => self.preview_edit(uid, &def)?,
        };
        let mut value = preview_json(&report);
        if let Some(datum) = self.preview_datum() {
            value["datum"] = analysis::datum_json(&datum);
        }
        Ok(value.to_string())
    }

    /// Answers a JSON query.
    pub fn query(&self, json: &str) -> Result<String, ApiError> {
        let query: Query = parse(read_json(json)?, "query")?;
        let result = match query {
            Query::Document => {
                let context = self.parameters().context();
                let mut value = json!({
                    "features": self.features().count(),
                    "marker": self.marker(),
                    "bodies": self.bodies().len(),
                    "undo": self.undo_label(),
                    "redo": self.redo_label(),
                    "units": {"length": context.default_length_unit.symbol(),
                              "angle": context.default_angle_unit.symbol()},
                    "warnings": self.load_warnings(),
                    "undo_depth": self.undo_depth(),
                    // The Origin folder and Isolate (P9); only isolated
                    // items that are there at the marker.
                    "display": {"origin": self.display().origin, "isolated": self.isolation()},
                    // The saved state (P8).
                    "revision": self.revision(),
                    "modified": self.is_modified(),
                });
                let assembly = self.assembly();
                if !assembly.is_empty() {
                    value["components"] = json!(assembly.components.len());
                    value["occurrences"] = json!(assembly.occurrences.len());
                    value["active_component"] = json!(assembly.active);
                }
                value
            }
            Query::Timeline => self.timeline_json(),
            // Value inputs of command panels (U1, `evaluate.rs`).
            Query::Evaluate(query) => self.evaluate_json(&query)?,
            Query::Parameters => {
                let params = self.parameters();
                Value::Array(
                    params
                        .iter()
                        .map(|p| {
                            let dependencies: Vec<String> =
                                p.dependencies().iter().map(|id| params.name(*id)).collect();
                            json!({"name": p.name(), "expression": p.expression(),
                                   "unit": p.unit().to_string(), "value": p.value(),
                                   "text": p.formatted_value(DEFAULT_DECIMALS),
                                   "comment": p.comment(), "kind": p.kind().as_str(),
                                   "owner": params.owner(p.id()),
                                   "dependencies": dependencies,
                                   "favorite": self.is_favorite_parameter(p.name())})
                        })
                        .collect(),
                )
            }
            Query::Bodies {
                properties,
                volumes,
            } => self.bodies_json(properties, volumes)?,
            Query::Profiles { hashes } => Value::Array(
                self.profiles()
                    .into_iter()
                    .map(|p| {
                        let sketch_name = self.feature(p.sketch).map(|f| f.name.clone());
                        let mut value = json!({"sketch": p.sketch, "sketch_name": sketch_name,
                                               "region": p.region, "consumed": p.consumed});
                        if hashes {
                            let hash = self
                                .profile_region(p.sketch, &p.region)
                                .map(|(frame, region)| profile_hash(&frame, region));
                            value["hash"] = json!(hash);
                        }
                        value
                    })
                    .collect(),
            ),
            Query::Feature { uid } => {
                let entry = self
                    .feature(uid)
                    .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
                let params = self.parameters();
                let def = entry
                    .def
                    .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                    .expect("names never fail");
                let status = self.status(uid);
                let warnings = self.warnings(uid);
                let mut value = json!({"uid": uid, "name": entry.name, "type": entry.def.type_name(),
                       "suppressed": entry.suppressed,
                       "status": status.map(|s| status_text(Some(s), warnings)),
                       "error": status_message(status, warnings), "def": def});
                if !warnings.is_empty() {
                    value["warnings"] = json!(warnings);
                }
                if !entry.component.is_root() {
                    value["component"] = json!(entry.component);
                }
                if let Some(visible) = self.feature_shown(uid) {
                    value["visible"] = json!(visible);
                    value["visible_set"] = json!(self.feature_visible(uid).is_some());
                }
                value
            }
            Query::Faces { body } => {
                let shape = self.require_body(body)?;
                let faces = self
                    .kernel()
                    .faces(shape)
                    .map_err(|e| ApiError(e.to_string()))?;
                Value::Array(
                    faces
                        .into_iter()
                        .map(|f| json!({"names": f.names, "surface": f.surface, "area": f.area}))
                        .collect(),
                )
            }
            Query::Edges { body } => {
                let shape = self.require_body(body)?;
                let edges = self
                    .kernel()
                    .edges(shape)
                    .map_err(|e| ApiError(e.to_string()))?;
                Value::Array(
                    edges
                        .into_iter()
                        .map(|e| json!({"name": e.name, "curve": e.curve, "length": e.length}))
                        .collect(),
                )
            }
            Query::Report => {
                let mut value = json!({
                    "timeline": self.timeline_json(),
                    "bodies": self.bodies_json(true, false)?,
                });
                if !self.assembly().is_empty() {
                    value["components"] = self.components_json();
                    value["instances"] =
                        self.instances_json(&components::InstancesQuery::everything())?;
                }
                value
            }
            Query::Datums(query) => self.datums_json(&query),
            Query::Datum(query) => self.datum_json(&query)?,
            Query::Properties(query) => self.properties_json(&query)?,
            Query::Measure(query) => self.measure_json(&query)?,
            Query::Interference(query) => self.interference_json(&query)?,
            Query::Section(query) => self.section_json(&query)?,
            Query::CompareStep(query) => self.compare_step_json(&query)?,
            Query::Threads => self.threads_json(),
            Query::Sketch { uid } => self.sketch_json(uid)?,
            Query::Components => self.components_json(),
            Query::Instances(query) => self.instances_json(&query)?,
            // Browser and timeline (U3).
            Query::Dependents { uid } => {
                let dependents: Vec<Value> = self
                    .feature_dependents(uid)?
                    .into_iter()
                    .map(|d| {
                        let name = self.feature(d).map(|f| f.name.clone());
                        json!({"uid": d, "name": name})
                    })
                    .collect();
                json!({"dependents": dependents})
            }
            Query::CanReorder { uid, index } => match self.check_reorder(uid, index) {
                Ok(()) => json!({"ok": true}),
                Err(error) => json!({"ok": false, "error": error.to_string()}),
            },
            // Named views (U5).
            Query::NamedViews => json!(self.named_views()),
            // Recompute speed (P10).
            Query::RecomputeTimes => self.recompute_times_json(),
            // Small UI gaps and metric defaults (P9, P11).
            Query::ThreadSizes { standard } => threads::thread_sizes_json(standard),
            Query::DxfInfo { path } => sketch::dxf_info_json(&path)?,
            // The caches (P7d).
            Query::Cache { disk } => self.cache_json(disk),
            // Saving a version (P12d).
            Query::ChangesSinceSaved => {
                let steps = self.changes_since_saved();
                json!({"known": steps.is_some(), "steps": steps.unwrap_or_default()})
            }
            // Analyses kept in the document (mitcad#41).
            Query::Analyses => self.analyses_json(),
            // Appearances (mitcad#46).
            Query::Appearances => self.appearances_json(),
            Query::AppearanceImage { id } => self.appearance_image_json(&id)?,
            // Render settings (mitcad#47).
            Query::RenderSettings => self.render_settings_json(),
            // Joints between occurrences (mitcad#55).
            Query::Joints => self.joints_json(),
            Query::JointDof(query) => self.joint_dof_json(&query)?,
            Query::JointDrag(query) => self.joint_drag_json(&query)?,
            Query::JointFrame(query) => self.joint_frame_json(&query)?,
            // Configuration tables and library parts (mitcad#64).
            Query::Configurations => self.configurations_json(),
            Query::LibraryParts => self.library_parts_json(),
            Query::PartsList => self.parts_list_json(),
        };
        Ok(result.to_string())
    }

    /// The features the last recompute evaluated with how long each took
    /// (`recompute_times`).
    pub(super) fn recompute_times_json(&self) -> Value {
        let stats = self.stats();
        let features: Vec<Value> = stats
            .evaluated
            .iter()
            .zip(&stats.times)
            .map(|(uid, time)| {
                let entry = self.feature(*uid);
                json!({"uid": uid, "name": entry.map(|f| &f.name),
                       "type": entry.map(|f| f.def.type_name()),
                       "ms": time.as_secs_f64() * 1000.0})
            })
            .collect();
        let total: f64 = stats.times.iter().map(|t| t.as_secs_f64() * 1000.0).sum();
        json!({"features": features, "ms": total})
    }

    fn require_body(&self, uid: BodyUid) -> Result<&K::Shape, ApiError> {
        self.body_shape(uid)
            .ok_or_else(|| ApiError(format!("body {uid} does not exist at the timeline marker")))
    }

    fn timeline_json(&self) -> Value {
        let used = self.state().used_sketches();
        let features: Vec<Value> = self
            .timeline()
            .into_iter()
            .map(|item| {
                // A feature that succeeded with warnings is `warning`, its
                // warnings in `error` (P9: yellow in the timeline).
                let mut value = json!({
                    "uid": item.entry.uid,
                    "name": item.entry.name,
                    "type": item.entry.def.type_name(),
                    "suppressed": item.entry.suppressed,
                    "status": item.status_text(),
                    "error": item.message(),
                });
                if !item.entry.component.is_root() {
                    value["component"] = json!(item.entry.component);
                }
                // A sketch's or construction feature's light bulb, and
                // whether it was set or follows the default (mitcad#7).
                if let Some(visible) = self.state().feature_shown(item.entry, &used) {
                    value["visible"] = json!(visible);
                    value["visible_set"] = json!(self.feature_visible(item.entry.uid).is_some());
                }
                // A sketch's degrees of freedom (P9: the browser marks a
                // fully constrained one).
                if let Some(sketch) = self.sketch_output(item.entry.uid) {
                    value["dof"] = json!(sketch.solved.status.dof);
                }
                value
            })
            .collect();
        let mut value = json!({"marker": self.marker(), "features": features});
        // Timeline groups (P9), when there are any.
        if !self.timeline_groups().is_empty() {
            value["groups"] = json!(self.timeline_groups());
        }
        value
    }

    fn bodies_json(&self, properties: bool, volumes: bool) -> Result<Value, ApiError> {
        let mut bodies = Vec::new();
        for body in self.bodies() {
            let mut value = json!({"uid": body.uid, "name": body.name});
            if let Some(component) = self.body_component(body.uid).filter(|c| !c.is_root()) {
                value["component"] = json!(component);
            }
            self.add_body_attributes(body.uid, &mut value);
            if volumes && !properties {
                let mass = self
                    .kernel()
                    .mass_properties(body.shape)
                    .map_err(|e| ApiError(format!("{}: {e}", body.name)))?;
                value["volume"] = json!(mass.volume);
            }
            if properties {
                let kernel = self.kernel();
                let error = |e: crate::kernel::KernelError| ApiError(format!("{}: {e}", body.name));
                let mass = kernel.mass_properties(body.shape).map_err(error)?;
                value["volume"] = json!(mass.volume);
                value["area"] = json!(mass.area);
                value["center"] = json!(mass.center);
                if let Some(bounds) = kernel.bounding_box(body.shape).map_err(error)? {
                    value["bbox"] = json!({"min": bounds.min, "max": bounds.max});
                }
                value["faces"] = json!(kernel.faces(body.shape).map_err(error)?.len());
                value["edges"] = json!(kernel.edges(body.shape).map_err(error)?.len());
                if let Ok(kind) = kernel.body_kind(body.shape) {
                    value["kind"] = json!(kind.as_str());
                }
            }
            bodies.push(value);
        }
        Ok(Value::Array(bodies))
    }
}

/// A hash of a profile region's geometry and sketch frame (the `profiles`
/// query's `hash`): the same while the face built from them would be. The
/// numbers are hashed by their bits (P9: formatting them through `Debug`
/// took 90-100 ms for the 594 profiles of a large corpus design).
fn profile_hash(frame: &SketchFrame, region: &ProfileRegion) -> String {
    use crate::profile::SegmentGeometry as G;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    fn numbers(hasher: &mut DefaultHasher, values: &[f64]) {
        for value in values {
            value.to_bits().hash(hasher);
        }
    }
    let mut hasher = DefaultHasher::new();
    numbers(&mut hasher, &frame.origin);
    numbers(&mut hasher, &frame.x_axis);
    numbers(&mut hasher, &frame.y_axis);
    region.key.hash(&mut hasher);
    region.loops.len().hash(&mut hasher);
    for profile_loop in &region.loops {
        profile_loop.segments.len().hash(&mut hasher);
        for segment in &profile_loop.segments {
            segment.key.hash(&mut hasher);
            std::mem::discriminant(&segment.geometry).hash(&mut hasher);
            match &segment.geometry {
                G::Line { start, end } => {
                    numbers(&mut hasher, &[start[0], start[1], end[0], end[1]])
                }
                G::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                } => numbers(
                    &mut hasher,
                    &[center[0], center[1], *radius, *start_angle, *end_angle],
                ),
                G::Circle { center, radius } => {
                    numbers(&mut hasher, &[center[0], center[1], *radius]);
                }
                G::Ellipse {
                    center,
                    major_radius,
                    minor_radius,
                    rotation,
                } => numbers(
                    &mut hasher,
                    &[
                        center[0],
                        center[1],
                        *major_radius,
                        *minor_radius,
                        *rotation,
                    ],
                ),
                G::EllipseArc {
                    center,
                    major_radius,
                    minor_radius,
                    rotation,
                    start_angle,
                    end_angle,
                } => numbers(
                    &mut hasher,
                    &[
                        center[0],
                        center[1],
                        *major_radius,
                        *minor_radius,
                        *rotation,
                        *start_angle,
                        *end_angle,
                    ],
                ),
                G::BSpline {
                    degree,
                    poles,
                    weights,
                    knots,
                    multiplicities,
                    periodic,
                } => {
                    degree.hash(&mut hasher);
                    poles.len().hash(&mut hasher);
                    for pole in poles {
                        numbers(&mut hasher, pole);
                    }
                    weights.len().hash(&mut hasher);
                    numbers(&mut hasher, weights);
                    knots.len().hash(&mut hasher);
                    numbers(&mut hasher, knots);
                    multiplicities.hash(&mut hasher);
                    periodic.hash(&mut hasher);
                }
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

fn preview_json(report: &PreviewReport) -> Value {
    let bodies: Vec<Value> = report
        .bodies
        .iter()
        .map(|(uid, name, changed)| json!({"uid": uid, "name": name, "changed": changed}))
        .collect();
    let mut value = json!({
        "uid": report.uid,
        "name": report.name,
        "status": report.status.as_str(),
        "error": report.status.error(),
        "warnings": report.warnings,
        "bodies": bodies,
        "removed": report.removed,
        "tool": report.tool,
    });
    // A pattern's elements (P9): rows of each element's transform.
    if !report.elements.is_empty() {
        let elements: Vec<_> = report
            .elements
            .iter()
            .map(crate::assembly::matrix_rows)
            .collect();
        value["elements"] = json!(elements);
    }
    // Occurrences placed elsewhere (joints, moves of occurrences; mitcad#55):
    // paths of uids from the root and their placements in the design.
    if !report.placements.is_empty() {
        let placements: Vec<Value> = report
            .placements
            .iter()
            .map(|(path, t)| {
                let path: Vec<String> = path.iter().map(ToString::to_string).collect();
                json!({"path": path.join("/"), "transform": crate::assembly::matrix4(t)})
            })
            .collect();
        value["placements"] = json!(placements);
    }
    value
}

#[cfg(test)]
mod browser_tests;
#[cfg(test)]
mod sketch_tests;
#[cfg(test)]
mod tests;
// Named views and DXF insert (U5, U6).
#[cfg(test)]
mod view_file_tests;
// Sketch patterns, offsets and text (P3, P4).
#[cfg(test)]
mod sketch_p34_tests;
// Small UI gaps and metric defaults (P9, P11).
#[cfg(test)]
mod metric_defaults_tests;
#[cfg(test)]
mod ui_gaps_tests;
// Analyses kept in the document (mitcad#41).
#[cfg(test)]
mod analysis_tests;
// Appearances kept in the document (mitcad#46).
#[cfg(test)]
mod appearance_tests;
// Render settings of the document (mitcad#47).
#[cfg(test)]
mod render_settings_tests;
