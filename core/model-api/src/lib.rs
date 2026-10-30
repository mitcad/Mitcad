// SPDX-License-Identifier: MIT
//! The application interface of the Mitcad model (mitcad#777: a crate of
//! its own above `mitcad-model`): JSON commands, queries and previews (see
//! `commands.md` next to this file). The application and `mitcad-cli` use
//! only this and the shape handles, so a new command needs no change to the
//! C++ bridge. The document's api methods are extension traits of
//! [`Document`]; import [`DocumentApi`] (or the [`prelude`]) to call them.

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
mod formula_help;
// The caches of computed results (P7d).
mod cache;
// Joints between occurrences (mitcad#55).
mod joints;
// Configuration tables and library parts (mitcad#64).
mod libraries;
// Move Features into a Component (mitcad#171).
mod move_features;
// Design policy (mitcad#220).
mod policy;
// The class of each command and query: what external clients may run
// (mitcad#265).
pub mod classes;
// Tangent chains of edges and faces, for the hover (mitcad#290).
mod tangent_chains;
// The BUILD tab: profile assemblies (mitcad#375).
mod build;
// Motion studies and contact sets (mitcad#454).
mod motion;
// Drawings: views with dimensions and a hole table as PDF, SVG or DXF
// (mitcad#471).
mod drawing;
// Gears (mitcad#472).
mod gears;
// Typed property sets (mitcad#496).
mod properties;
// Reorient Design (mitcad#502).
mod reorient;
// Mount frames of any part and placing a part by one (mitcad#525).
mod mounts;
// Timing belt pulleys (mitcad#562).
mod pulleys;
// Printable bearings (mitcad#577).
mod bearings;
// Seats on shafts (mitcad#578).
mod seats;
// The enums of commands and queries with their names, and the table of
// command families (mitcad#585).
mod dispatch;
// Print Analysis: overhangs, thin walls and a print estimate (mitcad#620).
mod print;
// Materials as objects, component default materials and the print key
// map (mitcad#640).
mod materials;
// Printer and process profiles, print settings and the build volume
// (mitcad#640).
mod print_profiles;
// Measure's angles and distances with axes (mitcad#647).
mod measure_axes;
// Placement by picked geometry, Gear Pair and Belt Drive (mitcad#656).
mod drives;
// Measurement objects kept in the design (mitcad#649).
mod measurements;
// Annotations grown from them (mitcad#702).
mod annotations;
// Parameter limits: min, max, step and strict (mitcad#665).
mod limits;

// The parameters as CSV (mitcad#666).
mod parameters_csv;
// Drawings kept in the design: drawings, sheets and views (mitcad#707).
mod drawings;
// A two-sided extrusion's negative distance: refused in commands without
// signed distances (mitcad#693), taken with them (mitcad#765).
mod two_sides;
// Purge volumes on filament changes (mitcad#705).
mod purge_volumes;
// Dimensions, centre marks and notes on drawings' views (mitcad#707 phase 2).
mod drawing_annotations;
// Hole tables, parts lists and balloons on drawings' views (mitcad#707
// phase 3).
mod drawing_tables;
// Faces and edges with geometry, filters and pages (mitcad#714).
mod face_lists;
// Sheet templates and revisions of drawings (mitcad#707 phase 4).
mod drawing_templates;
// Copy, Cut and Paste of sketch geometry (mitcad#788).
mod sketch_clipboard;
// Redefine Sketch Plane (mitcad#787).
mod sketch_plane;
// Image references: images on planes and faces to model from (mitcad#782).
mod image_references;
// Configurations: options, rules, the cascade, `configure` and Check Rules
// (mitcad#797).
mod configurations;
// Options of inserted components (mitcad#801).
mod component_options;
// Tables of data that expressions look values up in (mitcad#667).
mod tables;
// A table as CSV: its export, the preview and the import (mitcad#804).
mod tables_csv;
// Cut sheets: flat parts at 1:1 written as DXF and SVG (mitcad#853).
mod cut_sheets;
// Clearance holes and head recesses by screw size (mitcad#903).
mod hole_sizes;
// Hole callouts on the model, their text from the hole's feature
// (mitcad#902).
mod hole_callouts;
// A Fillet's or Chamfer's picked edges kept with its result, drawn where
// they were (mitcad#957).
mod feature_inputs;
// Section Analysis worked out without the document, off the UI thread
// (mitcad#438).
pub use analysis::{PreparedClips, PreparedQuery};
// The public api traits (mitcad#777).
pub use analysis::AnalysisApi;
pub use drawing::DrawingApi;
pub use report::ReportApi;
pub use script::ScriptApi;
pub use sketch::SketchApi;

/// The document's api methods are extension traits (mitcad#777), so that
/// the api can live in a crate above the model: import the public ones
/// (`use mitcad_model_api::prelude::*`) to call `command`, `query`,
/// `preview` and the others on a [`Document`](mitcad_model::Document).
pub mod prelude {
    pub use super::DocumentApi;
    // The drawing views projected before (mitcad#777: the drawing crate's).
    pub(crate) use super::DocumentCrateOps;
    pub(crate) use super::DocumentLocal;
    pub(crate) use super::DocumentOps;
    pub use super::analysis::AnalysisApi;
    pub(crate) use super::analysis::AnalysisOps;
    pub(crate) use super::annotations::AnnotationsOps;
    pub(crate) use super::bearings::BearingsCrateOps;
    pub(crate) use super::bearings::BearingsOps;
    pub(crate) use super::build::BuildCrateOps;
    pub(crate) use super::cache::CacheOps;
    pub(crate) use super::components::ComponentsCrateOps;
    pub(crate) use super::configurations::ConfigurationsOps;
    pub(crate) use super::dispatch::DispatchOps;
    pub use super::drawing::DrawingApi;
    pub(crate) use super::drawing::DrawingOps;
    pub(crate) use super::drawing_annotations::DrawingAnnotationsOps;
    pub(crate) use super::drawing_tables::DrawingTablesOps;
    pub(crate) use super::drawing_templates::DrawingTemplatesOps;
    pub(crate) use super::drawings::DrawingsOps;
    pub(crate) use super::drives::DrivesOps;
    pub(crate) use super::evaluate::EvaluateOps;
    pub(crate) use super::exchange::ExchangeOps;
    pub(crate) use super::face_lists::FaceListsOps;
    pub(crate) use super::formula_help::FormulaHelpOps;
    pub(crate) use super::gears::GearsOps;
    pub(crate) use super::image_references::ImageReferencesCrateOps;
    pub(crate) use super::image_references::ImageReferencesOps;
    pub(crate) use super::joints::JointsCrateOps;
    pub(crate) use super::libraries::LibrariesCrateOps;
    pub(crate) use super::limits::LimitsOps;
    pub(crate) use super::materials::MaterialsCrateOps;
    pub(crate) use super::materials::MaterialsOps;
    pub(crate) use super::measurements::MeasurementsOps;
    pub(crate) use super::motion::MotionCrateOps;
    pub(crate) use super::mounts::MountsCrateOps;
    pub(crate) use super::move_features::MoveFeaturesCrateOps;
    pub(crate) use super::parameters_csv::ParametersCsvOps;
    pub(crate) use super::policy::PolicyCrateOps;
    pub(crate) use super::print::PrintOps;
    pub(crate) use super::print_profiles::PrintProfilesCrateOps;
    pub(crate) use super::print_profiles::PrintProfilesOps;
    pub(crate) use super::properties::PropertiesCrateOps;
    pub(crate) use super::pulleys::PulleysCrateOps;
    pub(crate) use super::pulleys::PulleysOps;
    pub(crate) use super::purge_volumes::PurgeVolumesCrateOps;
    pub(crate) use super::purge_volumes::PurgeVolumesOps;
    pub(crate) use super::reorient::ReorientCrateOps;
    pub use super::report::ReportApi;
    pub use super::script::ScriptApi;
    pub(crate) use super::seats::SeatsOps;
    pub use super::sketch::SketchApi;
    pub(crate) use super::sketch::SketchOps;
    pub(crate) use super::tangent_chains::TangentChainsOps;
    pub(crate) use super::threads::ThreadsOps;
    pub(crate) use super::two_sides::TwoSidesOps;
    pub(crate) use mitcad_model_drawing::DrawingCache;
    // Copy, Cut and Paste of sketch geometry (mitcad#788).
    pub(crate) use super::sketch_clipboard::SketchClipboardOps;
    // Tables of data that expressions look values up in (mitcad#667).
    pub(crate) use super::tables::TablesOps;
    // A table as CSV (mitcad#804).
    pub(crate) use super::tables_csv::TablesCsvOps;
    // Cut sheets (mitcad#853).
    pub(crate) use super::cut_sheets::CutSheetsOps;
    // Hole callouts on the model (mitcad#902).
    pub(crate) use super::hole_callouts::HoleCalloutsOps;
}

use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};

use self::dispatch::named_enum;
use self::prelude::*;
use mitcad_model::expr::{DEFAULT_DECIMALS, Unit, value_to_expression};
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::internal::document::{noted_message, noted_status};
use mitcad_model::kernel::Kernel;
use mitcad_model::profile::{ProfileRegion, SketchFrame};
use mitcad_model::{AnalysisDef, Document, ModelError, NamedView, PreviewReport};

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

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    enum Command {
        "begin_command" => BeginCheckpoint,
        "restore_command" => RestoreCheckpoint {
            token: String,
        },
        "discard_command" => DiscardCheckpoint {
            token: String,
        },
        "add_feature" => AddFeature {
            def: FeatureDef<ValueInput>,
            #[serde(default)]
            name: Option<String>,
            /// The component (uid or name); the active one when left out.
            #[serde(default)]
            component: Option<String>,
            /// Inserted just after this feature instead of at the marker
            /// (mitcad#403).
            #[serde(default)]
            after: Option<FeatureUid>,
            /// Inserted at this timeline position (the number of features
            /// before it) instead of at the marker (mitcad#403).
            #[serde(default)]
            index: Option<usize>,
        },
        "edit_feature" => EditFeature {
            uid: FeatureUid,
            def: FeatureDef<ValueInput>,
        },
        "delete_feature" => DeleteFeature {
            uid: FeatureUid,
            #[serde(default)]
            dependents: bool,
        },
        "suppress_feature" => SuppressFeature {
            uid: FeatureUid,
            /// True when left out (and no `expression` is given).
            #[serde(default)]
            suppressed: Option<bool>,
            /// A condition the suppression follows instead (mitcad#800);
            /// "" removes it.
            #[serde(default)]
            expression: Option<String>,
        },
        "reorder_feature" => ReorderFeature {
            uid: FeatureUid,
            index: usize,
        },
        "rename_feature" => RenameFeature {
            uid: FeatureUid,
            name: String,
        },
        "rename_body" => RenameBody {
            uid: BodyUid,
            name: String,
        },
        "set_marker" => SetMarker {
            position: usize,
            #[serde(default)]
            temporary: bool,
        },
        "add_parameter" => AddParameter {
            name: String,
            #[serde(default)]
            value: Option<ValueInput>,
            #[serde(default)]
            expression: Option<String>,
            #[serde(default)]
            unit: Option<String>,
            #[serde(default)]
            comment: String,
            // Limits (mitcad#665, `limits.rs`).
            #[serde(default, deserialize_with = "limits::present")]
            min: Option<Value>,
            #[serde(default, deserialize_with = "limits::present")]
            max: Option<Value>,
            #[serde(default, deserialize_with = "limits::present")]
            step: Option<Value>,
            #[serde(default)]
            strict: Option<bool>,
            // Allowed values (mitcad#798): a list, null or [] removes it.
            #[serde(default, deserialize_with = "limits::present")]
            values: Option<Value>,
        },
        "set_parameter" => SetParameter {
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
            // Limits (mitcad#665, `limits.rs`): null or "" removes one.
            #[serde(default, deserialize_with = "limits::present")]
            min: Option<Value>,
            #[serde(default, deserialize_with = "limits::present")]
            max: Option<Value>,
            #[serde(default, deserialize_with = "limits::present")]
            step: Option<Value>,
            #[serde(default)]
            strict: Option<bool>,
            // Allowed values (mitcad#798): a list, null or [] removes it.
            #[serde(default, deserialize_with = "limits::present")]
            values: Option<Value>,
        },
        "set_units" => SetUnits {
            length: String,
        },
        "rename_parameter" => RenameParameter {
            name: String,
            new_name: String,
        },
        "delete_parameter" => DeleteParameter {
            name: String,
        },
        "undo" => Undo,
        "redo" => Redo,
        "recompute" => Recompute,
        // Body attributes (F5).
        "set_body_visible" => SetBodyVisible {
            uid: BodyUid,
            #[serde(default)]
            visible: Option<bool>,
            /// A condition the visibility follows instead (mitcad#800);
            /// "" removes it.
            #[serde(default)]
            expression: Option<String>,
        },
        "set_body_material" => SetBodyMaterial {
            uid: BodyUid,
            #[serde(default)]
            material: Option<String>,
            /// `replace` or `keep` the body's appearance; left out: replace
            /// unless it was set by hand (mitcad#640).
            #[serde(default)]
            appearance: Option<String>,
            /// `base`, `user` or `project`: the material is that library's
            /// item, copied in (or the design's copy reused) in the same
            /// undo step (mitcad#655).
            #[serde(default)]
            library: Option<String>,
            /// The project's folder, when the design is in one.
            #[serde(default)]
            project: Option<String>,
            /// An expression the material follows instead (mitcad#800);
            /// "" removes it.
            #[serde(default)]
            expression: Option<String>,
        },
        "set_body_appearance" => SetBodyAppearance {
            uid: BodyUid,
            #[serde(default)]
            appearance: Option<String>,
            /// An expression the appearance follows instead (mitcad#800);
            /// "" removes it.
            #[serde(default)]
            expression: Option<String>,
        },
        // Appearances of single faces (mitcad#53).
        "set_face_appearance" => SetFaceAppearance {
            uid: BodyUid,
            faces: Vec<String>,
            #[serde(default)]
            appearance: Option<String>,
            /// An expression the faces' appearance follows instead
            /// (mitcad#800); "" removes it.
            #[serde(default)]
            expression: Option<String>,
        },
        "clear_face_appearances" => ClearFaceAppearances {
            uid: BodyUid,
        },
        "import_file" => ImportFile(exchange::ImportFileCommand),
        "export" => Export(exchange::ExportCommand),
        "export_sketch" => ExportSketch(exchange::ExportSketchCommand),
        // Sketch mode (U2): one undo step for an interactive operation made of
        // several commands (a tool's curves and constraints, a drag).
        "merge_undo" => MergeUndo {
            depth: usize,
            label: String,
        },
        // Browser and timeline (U3): the light bulb of sketches and
        // construction features.
        "set_feature_visible" => SetFeatureVisible {
            uid: FeatureUid,
            visible: bool,
        },
        // Named views (U5).
        "add_named_view" => AddNamedView {
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
        "delete_named_view" => DeleteNamedView {
            name: String,
        },
        "rename_named_view" => RenameNamedView {
            name: String,
            new_name: String,
        },
        // The Origin folder and Isolate saved in the document (P9).
        "set_origin_visible" => SetOriginVisible {
            visible: bool,
        },
        "set_isolation" => SetIsolation {
            #[serde(default)]
            items: Vec<mitcad_model::Isolated>,
        },
        // Timeline groups (P9).
        "group_features" => GroupFeatures {
            features: Vec<FeatureUid>,
            #[serde(default)]
            name: Option<String>,
        },
        "ungroup" => Ungroup {
            name: String,
        },
        "rename_group" => RenameGroup {
            name: String,
            new_name: String,
        },
        // The saved state (P8): the design file was written or read.
        "mark_saved" => MarkSaved,
        // The caches of computed results (P7d, `cache.rs`).
        "clear_cache" => ClearCache,
        // Analyses kept in the document (mitcad#41, `document/analyses.rs`).
        "add_analysis" => AddAnalysis {
            def: AnalysisDef,
            /// The next free `Section<n>` when left out.
            #[serde(default)]
            name: Option<String>,
        },
        "edit_analysis" => EditAnalysis {
            name: String,
            def: AnalysisDef,
            /// Shows (hiding the others) or hides it; as it was when left out.
            #[serde(default)]
            visible: Option<bool>,
        },
        "set_analysis_visible" => SetAnalysisVisible {
            name: String,
            visible: bool,
        },
        "rename_analysis" => RenameAnalysis {
            name: String,
            new_name: String,
        },
        "delete_analysis" => DeleteAnalysis {
            name: String,
        },
        // Appearances kept in the document (mitcad#46, `document/appearances.rs`).
        "create_appearance" => CreateAppearance(mitcad_model::appearance::AppearanceChange),
        "edit_appearance" => EditAppearance(mitcad_model::appearance::AppearanceChange),
        "delete_appearance" => DeleteAppearance {
            id: String,
        },
        // Render settings of the document (mitcad#47).
        "set_render_settings" => SetRenderSettings(mitcad_model::render_settings::RenderSettingsChange),
        "reset_render_settings" => ResetRenderSettings,
        // Lights of the render settings (mitcad#54): the light's fields.
        "add_render_light" => AddRenderLight(serde_json::Map<String, Value>),
        "edit_render_light" => EditRenderLight(serde_json::Map<String, Value>),
        "delete_render_light" => DeleteRenderLight {
            id: String,
        },
        // Sheet-metal Flat Patterns (mitcad#208, `document/flat_patterns.rs`).
        "set_flat_pattern" => SetFlatPattern {
            body: BodyUid,
            #[serde(default)]
            stationary: Option<mitcad_model::sheet::PanelId>,
            #[serde(default)]
            x_axis: Option<mitcad_model::sheet::flat::FlatXAxis>,
            #[serde(default)]
            reference_side: Option<mitcad_model::sheet::SheetReferenceSide>,
            /// Show the developed solid in place of the body (mitcad#210).
            #[serde(default)]
            shown: Option<bool>,
        },
        "remove_flat_pattern" => RemoveFlatPattern {
            body: BodyUid,
        },
        // The Flat Pattern as a DXF drawing (mitcad#210, `exchange.rs`).
        "export_flat_pattern_dxf" => ExportFlatPatternDxf(exchange::ExportFlatPatternCommand),
        // Drawings: views with dimensions and a hole table (mitcad#471, `drawing.rs`).
        "export_drawing" => ExportDrawing(drawing::ExportDrawingCommand),
        // A body's print bed face (mitcad#731, `document/print_bed.rs`).
        "set_print_bed_face" => SetPrintBedFace {
            uid: BodyUid,
            /// A face name; null or left out takes it away.
            #[serde(default)]
            face: Option<String>,
        },
    }
}

/// The error for a value given together with the expression that would
/// replace it (mitcad#800).
fn both(field: &str) -> ApiError {
    ApiError(format!(
        "`{field}` and `expression` are both given: give the value, or the expression it \
         follows"
    ))
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
        /// Choose the operation of a new extrusion (mitcad#289): the
        /// report gets `auto_operation`.
        #[serde(default)]
        auto_operation: bool,
    },
    EditFeature {
        uid: FeatureUid,
        def: FeatureDef<ValueInput>,
        /// Suggest an extrusion operation from the bodies before the edit.
        #[serde(default)]
        auto_operation: bool,
    },
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
    enum Query {
        "document" => Document,
        "timeline" => Timeline,
        // Value inputs of command panels (U1, `evaluate.rs`).
        "evaluate" => Evaluate(evaluate::EvaluateQuery),
        "formula_help" => FormulaHelp(formula_help::FormulaHelpQuery),
        "parameters" => Parameters,
        "printed_fit_parameters" => PrintedFitParameters,
        "bodies" => Bodies {
            #[serde(default)]
            properties: bool,
            // Only the volume of the properties (W1: the application's log of
            // what a command changed, without listing faces and edges).
            #[serde(default)]
            volumes: bool,
            // Each body's shape version (mitcad#689: MCP's command results
            // list the bodies a command changed).
            #[serde(default)]
            versions: bool,
            // Each body's bounding box alone (mitcad#714: MCP's command
            // results tell where the bodies a command changed are).
            #[serde(default)]
            bbox: bool,
        },
        "profiles" => Profiles {
            // A hash of each region's geometry and sketch frame (W1: the
            // application keeps the faces it built while it stays the same).
            #[serde(default)]
            hashes: bool,
        },
        "autoselect_profiles" => AutoselectProfiles,
        "sheet_state" => SheetState {
            #[serde(default)]
            body: Option<BodyUid>,
        },
        "sheet_rules" => SheetRules {
            #[serde(default)]
            feature: Option<FeatureUid>,
        },
        "feature" => Feature {
            uid: FeatureUid,
        },
        // With geometry, filters and pages (mitcad#714, `face_lists.rs`).
        "faces" => Faces(face_lists::FacesQuery),
        "edges" => Edges(face_lists::EdgesQuery),
        "report" => Report,
        // Construction geometry and analysis (F5, `analysis.rs`).
        "datums" => Datums(analysis::DatumsQuery),
        "datum" => Datum(analysis::DatumQuery),
        "properties" => Properties(analysis::PropertiesQuery),
        "measure" => Measure(analysis::MeasureQuery),
        "interference" => Interference(analysis::InterferenceQuery),
        "section" => Section(analysis::SectionQuery),
        "section_plane" => SectionPlane(mitcad_model::SectionAnalysis),
        "compare_step" => CompareStep(analysis::CompareStepQuery),
        "threads" => Threads {
            /// Only cosmetic threads, the ones a view draws (mitcad#878).
            #[serde(default)]
            cosmetic: bool,
        },
        // The general sketch (S1, `sketch.rs`).
        "sketch" => Sketch {
            uid: FeatureUid,
        },
        // Components and occurrences (F6, `components.rs`).
        "components" => Components,
        "instances" => Instances(components::InstancesQuery),
        // Browser and timeline (U3).
        "dependents" => Dependents {
            uid: FeatureUid,
        },
        "can_reorder" => CanReorder {
            uid: FeatureUid,
            index: usize,
        },
        // Named views (U5).
        "named_views" => NamedViews,
        // Recompute speed (P10).
        "recompute_times" => RecomputeTimes,
        // Small UI gaps and metric defaults (P9, P11).
        "thread_sizes" => ThreadSizes {
            #[serde(default)]
            standard: Option<mitcad_model::features::ThreadStandard>,
        },
        "dxf_info" => DxfInfo {
            path: String,
        },
        // The caches of computed results (P7d, `cache.rs`).
        "cache" => Cache {
            /// Also what the result store on disk holds (reads its files).
            #[serde(default)]
            disk: bool,
        },
        // Saving a version (P12d): what changed since the saved state.
        "changes_since_saved" => ChangesSinceSaved,
        // Analyses kept in the document (mitcad#41).
        "analyses" => Analyses,
        // Appearances: the library's and the document's (mitcad#46).
        "appearances" => Appearances,
        // An appearance's embedded texture image (mitcad#53), of the base
        // colour or another map of its set (mitcad#313).
        "appearance_image" => AppearanceImage {
            id: String,
            #[serde(default)]
            map: Option<String>,
        },
        // Render settings of the document (mitcad#47).
        "render_settings" => RenderSettings,
        // Joints between occurrences (mitcad#55, `joints.rs`).
        "joints" => Joints,
        "joint_dof" => JointDof(joints::JointDofQuery),
        "joint_drag" => JointDrag(joints::JointDragQuery),
        // The frame a joint origin gives (mitcad#55 phase 3: the panels show
        // where an origin snaps).
        "joint_frame" => JointFrame(joints::JointFrameQuery),
        // Configuration tables and library parts (mitcad#64).
        "configurations" => Configurations,
        "library_parts" => LibraryParts,
        "parts_list" => PartsList {
            /// Columns of property values: `Key` or `Set/Key` (mitcad#496).
            #[serde(default)]
            properties: Vec<String>,
        },
        // A row of assembly constraints before it is added: its references' kinds,
        // the types they fit and what they measure (mitcad#148).
        "constraint_row" => ConstraintRow(Box<joints::ConstraintRowQuery>),
        // Move Features into a Component (mitcad#171): what a move would do,
        // and what a definition refers to.
        "move_to_component" => MoveToComponent(move_features::MoveSelection),
        "feature_references" => FeatureReferences(Box<move_features::ReferencesQuery>),
        // The region a profile reference means now (mitcad#199): the
        // application's selection inputs tell by it whether a click is on a
        // selected region, whatever key the selection keeps for it.
        "profile" => Profile {
            sketch: FeatureUid,
            region: mitcad_model::topo::RegionKey,
        },
        // Sheet-metal Flat Patterns (mitcad#208): one body's, or all of them.
        "flat_pattern" => FlatPattern {
            #[serde(default)]
            body: Option<BodyUid>,
        },
        // The two-way flat correspondence of a body point or flat position.
        "flat_point" => FlatPoint {
            body: BodyUid,
            #[serde(default)]
            flat: Option<[f64; 2]>,
            #[serde(default)]
            model: Option<[f64; 3]>,
        },
        // The command UI of sheet metal (mitcad#210, `document/flat_patterns.rs`):
        // a picked face or edge as a semantic reference, the seam two panels
        // share, and the sheet bodies with their rules and Flat Patterns.
        "sheet_reference" => SheetReference {
            body: BodyUid,
            #[serde(default)]
            face: Option<String>,
            #[serde(default)]
            edge: Option<String>,
            #[serde(default)]
            at: Option<[f64; 3]>,
        },
        "sheet_seam" => SheetSeam {
            body: BodyUid,
            stationary: mitcad_model::sheet::PanelId,
            moving: mitcad_model::sheet::PanelId,
        },
        "sheet_bodies" => SheetBodies,
        // Design policy (mitcad#220, `policy.rs`): one owner's settings and
        // violations, every owner's, and whose geometry sketches may attach to.
        "policy" => Policy(policy::PolicyQuery),
        "policies" => Policies,
        "sketch_supports" => SketchSupports(policy::SketchSupportsQuery),
        // Moving an occurrence to another parent (mitcad#219, `components.rs`).
        "reparent_occurrence" => ReparentOccurrence(components::ReparentQuery),
        // Body edges and vertices in a sketch's plane (mitcad#256, `sketch.rs`).
        "plane_geometry" => PlaneGeometry {
            sketch: FeatureUid,
            // Also those out of the plane, projected (mitcad#866).
            #[serde(default)]
            out_of_plane: bool,
        },
        // Tangent chains of edges and faces (mitcad#290, `tangent_chains.rs`).
        "tangent_chains" => TangentChains(tangent_chains::TangentChainsQuery),
        // A thread's default end chamfer (mitcad#304, `threads.rs`).
        "thread_chamfer" => ThreadChamfer(threads::ThreadChamferQuery),
        // The BUILD tab (mitcad#375, `build.rs`): a preview of build_place's,
        // build_panel's or build_connect's fields, the parts, the junctions
        // and the cut list.
        "build_preview" => BuildPreview(serde_json::Map<String, Value>),
        "build_parts" => BuildParts,
        "build_junctions" => BuildJunctions(build::JunctionsQuery),
        "build_cut_list" => BuildCutList(build::CutListQuery),
        // The component BUILD places parts in (mitcad#394, `build.rs`).
        "build_host" => BuildHost,
        // The library parts the design uses, with their counts, for the part
        // picker (mitcad#418, `libraries.rs`).
        "library_usage" => LibraryUsage,
        // Motion studies and contact sets (mitcad#454, `motion.rs`).
        "motion_studies" => MotionStudies,
        "study_values" => StudyValues(motion::StudyValuesQuery),
        "contact_sets" => ContactSets,
        "contact_check" => ContactCheck,
        // Motion links between joints (mitcad#466, `joints.rs`).
        "motion_link_offset" => MotionLinkOffset(Box<joints::MotionLinkOffsetQuery>),
        // Bodies moved into another component (mitcad#465, `move_features.rs`).
        "transfer_bodies" => TransferBodies(move_features::TransferQuery),
        // Gears: derived values (mitcad#472, `gears.rs`).
        "gear_values" => GearValues(gears::GearValuesQuery),
        "gear_link" => GearLink(gears::GearLinkQuery),
        // An SVG drawing for Insert SVG (mitcad#473, `sketch/svg.rs`).
        "svg_info" => SvgInfo {
            path: String,
        },
        // Typed property sets (mitcad#496, `properties.rs`).
        "property_sets" => PropertySets(properties::PropertySetsQuery),
        "property_types" => PropertyTypes,
        "property_search" => PropertySearch(properties::PropertySearchQuery),
        // Mount frames and placing a part by one (mitcad#525, `mounts.rs`).
        "mount_frames" => MountFrames(mounts::MountFramesQuery),
        "mount_preview" => MountPreview(Box<mounts::MountPreviewQuery>),
        // Pulleys: derived values and belts (mitcad#562, `pulleys.rs`).
        "pulley_values" => PulleyValues(pulleys::PulleyValuesQuery),
        // Printable bearings: derived values and sizes (mitcad#577, `bearings.rs`).
        "bearing_values" => BearingValues(bearings::BearingValuesQuery),
        "bearing_sizes" => BearingSizes(bearings::BearingSizesQuery),
        // Seats on shafts (mitcad#578, `seats.rs`).
        "seat_frame" => SeatFrame(seats::SeatFrameQuery),
        // Print Analysis (mitcad#620, `print.rs`).
        "print_overhangs" => PrintOverhangs(print::OverhangsQuery),
        "print_thickness" => PrintThickness(print::ThicknessQuery),
        "print_estimate" => PrintEstimate(print::EstimateQuery),
        // Materials (mitcad#640, `materials.rs`).
        "materials" => Materials,
        "body_material" => BodyMaterial(materials::BodyMaterialQuery),
        "material_conflicts" => MaterialConflicts(materials::MaterialConflictsQuery),
        "print_keys" => PrintKeys(materials::PrintKeysQuery),
        // Print profiles and settings (mitcad#640, `print_profiles.rs`).
        "print_profiles" => PrintProfiles,
        "print_settings" => PrintSettings(print_profiles::PrintSettingsQuery),
        "build_volume" => BuildVolume,
        "print_library" => PrintLibrary(print_profiles::PrintLibraryQuery),
        // Placement by picked geometry, Gear Pair and Belt Drive (mitcad#656,
        // `drives.rs`).
        "place_frame" => PlaceFrame(Box<drives::PlaceFrameQuery>),
        "gear_pair" => GearPair(Box<drives::GearPairQuery>),
        "belt_drive" => BeltDrive(Box<drives::BeltDriveQuery>),
        "belt_check" => BeltCheck(drives::BeltCheckQuery),
        // Choosable materials and profiles where they live (mitcad#655).
        "library_items" => LibraryItems(print_profiles::LibraryItemsQuery),
        // Measurement objects (mitcad#649, `measurements.rs`).
        "measurements" => Measurements {
            // Hidden annotations measured too (mitcad#716).
            #[serde(default)]
            measure_hidden: bool,
        },
        // Annotations (mitcad#702, `annotations.rs`): the same list.
        "annotations" => Annotations {
            #[serde(default)]
            measure_hidden: bool,
        },
        // The characteristics of feature control frames and their ISO 1101
        // rules (mitcad#702 phase 2), for an editor of frames.
        "geometric_characteristics" => GeometricCharacteristics,
        // The letter a new datum label takes (mitcad#702 phase 2).
        "next_datum_letter" => NextDatumLetter,
        // The parameters as CSV (mitcad#666, `parameters_csv.rs`).
        "parameters_csv" => ParametersCsv,
        "parameters_csv_preview" => ParametersCsvPreview(parameters_csv::ParametersCsvQuery),
        // Purge volumes on filament changes (mitcad#705, `purge_volumes.rs`).
        "purge_volumes" => PurgeVolumes(purge_volumes::PurgeVolumesQuery),
        "purge_defaults" => PurgeDefaults(purge_volumes::PurgeDefaultsQuery),
        // Drawings kept in the design (mitcad#707, `drawings.rs`).
        "drawings" => Drawings,
        "drawing" => Drawing(drawings::DrawingQuery),
        // What a click on a drawing's view picks (mitcad#707 phase 2).
        "drawing_pick" => DrawingPick(drawing_annotations::PickQuery),
        // The sheet templates a drawing can choose (mitcad#707 phase 4).
        "drawing_templates" => DrawingTemplates(drawing_templates::TemplatesQuery),
        // Copy of sketch geometry for a paste (mitcad#788, `sketch_clipboard.rs`).
        "sketch_fragment" => SketchFragment(sketch_clipboard::FragmentQuery),
        // Image references (mitcad#782, `image_references.rs`).
        "image_references" => ImageReferences,
        "image_reference_image" => ImageReferenceImage {
            name: String,
        },
        "image_reference_outline" => ImageReferenceOutline(Box<image_references::OutlineQuery>),
        // Tables of data that expressions look values up in (mitcad#667,
        // `tables.rs`).
        "tables" => Tables(tables::TablesQuery),
        "image_info" => ImageInfo {
            path: String,
        },
        // An installed slicer's user presets (mitcad#830, `print_profiles.rs`).
        "slicer_presets" => SlicerPresets(print_profiles::SlicerPresetsQuery),
        // Configurations: the choosers' report and Check Rules (mitcad#797,
        // `configurations.rs`).
        "configure" => Configure(configurations::ConfigureQuery),
        "check_rules" => CheckRules(configurations::CheckRulesQuery),
        // A table as CSV (mitcad#804, `tables_csv.rs`).
        "table_csv" => TableCsv(tables_csv::TableCsvQuery),
        "table_csv_preview" => TableCsvPreview(tables_csv::TableCsvPreviewQuery),
        // A cut sheet's parts and their outlines (mitcad#853, `cut_sheets.rs`).
        "cut_sheet" => CutSheet(cut_sheets::CutSheetQuery),
        // The height of the annotations' texts (mitcad#894, `annotations.rs`).
        "annotation_text_height" => AnnotationTextHeight,
        // Clearance holes and head recesses by screw size (mitcad#903,
        // `hole_sizes.rs`).
        "hole_sizes" => HoleSizes(hole_sizes::HoleSizesQuery),
        // A Fillet's or Chamfer's picked edges kept with its result
        // (mitcad#957, `feature_inputs.rs`).
        "feature_inputs" => FeatureInputs(feature_inputs::FeatureInputsQuery),
    }
}

fn names(names: &[String]) -> Value {
    json!(names)
}

/// The JSON api of a document: commands, previews and queries
/// (`commands.md`). An extension trait (mitcad#777), so that the api can
/// live in a crate above the model; import it (or the [`prelude`]) to call
/// these methods on a [`Document`].
pub trait DocumentApi<K: Kernel> {
    /// Runs a JSON command, or an array of them in order (each its own undo
    /// step, stopping at the first failure). Returns the result as JSON.
    fn command(&mut self, json: &str) -> Result<String, ApiError>;

    /// Evaluates an `add_feature` or `edit_feature` command without
    /// committing it; the shapes are then available as preview handles.
    fn preview(&mut self, json: &str) -> Result<String, ApiError>;

    /// Answers a JSON query.
    fn query(&self, json: &str) -> Result<String, ApiError>;
}

impl<K: Kernel> DocumentApi<K> for Document<K> {
    fn command(&mut self, json: &str) -> Result<String, ApiError> {
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

    fn preview(&mut self, json: &str) -> Result<String, ApiError> {
        self.with_policy_checks(|doc| doc.checked_preview(json))
    }

    fn query(&self, json: &str) -> Result<String, ApiError> {
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
                // A hybrid assembly allowed without asking (mitcad#171).
                if self.hybrid_assembly() {
                    value["hybrid_assembly"] = json!(true);
                }
                // The parameters a limit changed (mitcad#665).
                let limited = limits::limited_parameters(self.parameters());
                if !limited.is_empty() {
                    value["limited_parameters"] = json!(limited);
                }
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
            Query::FormulaHelp(query) => self.formula_help_json(&query)?,
            Query::PrintedFitParameters => mitcad_model::internal::print_fits::json(),
            Query::Parameters => {
                let params = self.parameters();
                // The features' own limits of the parameters' slots (mitcad#665).
                let intrinsic = self.intrinsic_limits();
                Value::Array(
                    params
                        .iter()
                        .map(|p| {
                            let dependencies: Vec<String> =
                                p.dependencies().iter().map(|id| params.name(*id)).collect();
                            let mut value = json!({"name": p.name(), "expression": p.expression(),
                                   "unit": p.unit().to_string(), "value": p.value(),
                                   "text": p.formatted_value(DEFAULT_DECIMALS),
                                   "comment": p.comment(), "kind": p.kind().as_str(),
                                   "owner": params.owner(p.id()),
                                   "dependencies": dependencies,
                                   "favorite": self.is_favorite_parameter(p.name())});
                            // The table columns it looks values up in
                            // (mitcad#667): `Screws.dk`.
                            value["table_dependencies"] = json!(tables::table_dependencies(p));
                            if let Some(context) = p.expression_context() {
                                value["expression_units"] = json!({
                                    "length": context.default_length_unit.symbol(),
                                    "angle": context.default_angle_unit.symbol(),
                                });
                            }
                            limits::add_fields(&mut value, p, intrinsic.get(&p.id()));
                            // Configurations (mitcad#797): an option, and the
                            // rules that use it.
                            let table = self.configurations();
                            if table.is_option(p.name()) {
                                value["option"] = json!(true);
                            }
                            let rules = table.rules_using(p.name());
                            if !rules.is_empty() {
                                value["rules"] = json!(rules);
                            }
                            value
                        })
                        .collect(),
                )
            }
            Query::Bodies {
                properties,
                volumes,
                versions,
                bbox,
            } => {
                let mut bodies = self.bodies_json(properties, volumes)?;
                if versions {
                    self.add_body_versions(&mut bodies);
                }
                if bbox && !properties {
                    self.add_body_boxes(&mut bodies);
                }
                bodies
            }
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
            Query::AutoselectProfiles => json!(self.autoselect_profiles()),
            Query::SheetState { body } => self.sheet_state_json(body),
            Query::SheetRules { feature } => self.sheet_rules_json(feature),
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
                let note = self.import_note(uid);
                let mut value = json!({"uid": uid, "name": entry.name, "type": entry.def.type_name(),
                       "suppressed": entry.suppressed,
                       "status": status.map(|s| noted_status(Some(s), warnings, note)),
                       "error": noted_message(status, warnings, note), "def": def});
                if !warnings.is_empty() {
                    value["warnings"] = json!(warnings);
                }
                // What the importer said about it (mitcad#170).
                if let Some(note) = note {
                    value["import"] = json!(note);
                }
                if !entry.component.is_root() {
                    value["component"] = json!(entry.component);
                }
                if let Some(visible) = self.feature_shown(uid) {
                    value["visible"] = json!(visible);
                    value["visible_set"] = json!(self.feature_visible(uid).is_some());
                }
                self.add_suppressed_when(uid, &mut value);
                value
            }
            Query::Faces(query) => self.faces_json(&query)?,
            Query::Edges(query) => self.edges_json(&query)?,
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
            Query::SectionPlane(query) => {
                let plane = self.section_plane(&query).map_err(ApiError)?;
                let occurrence = self
                    .analysis_plane_occurrence(&query.plane, query.occurrence.as_ref())
                    .map_err(ApiError)?;
                let mut answer = json!({"origin": plane.origin, "normal": plane.normal,
                    "occurrence": occurrence});
                // The axis turned about (mitcad#823), in design coordinates.
                if let (Some(reference), Some(axis)) =
                    (&query.axis, self.section_axis(&query).map_err(ApiError)?)
                {
                    let axis_occurrence = self
                        .analysis_axis_occurrence(reference, query.axis_occurrence.as_ref())
                        .map_err(ApiError)?;
                    answer["axis"] = json!({"origin": axis.origin, "direction": axis.direction});
                    answer["axis_occurrence"] = json!(axis_occurrence);
                }
                answer
            }
            Query::CompareStep(query) => self.compare_step_json(&query)?,
            Query::Threads { cosmetic } => self.threads_json_of(cosmetic),
            // Answered as before while nothing it is made from changed
            // (mitcad#679).
            Query::Sketch { uid } => return self.sketch_answer(uid),
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
            Query::AppearanceImage { id, map } => {
                self.appearance_image_json(&id, map.as_deref())?
            }
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
            Query::PartsList { properties } => self.parts_list_with(&properties),
            // Assembly constraints (mitcad#148).
            Query::ConstraintRow(query) => self.constraint_row_json(&query)?,
            // Move Features into a Component (mitcad#171).
            Query::MoveToComponent(selection) => self.move_plan_json(&selection)?,
            Query::FeatureReferences(query) => self.feature_references_json(&query),
            Query::Profile { sketch, region } => match self.profile_pieces(sketch, &region) {
                Some((_, found, exact)) => json!({"sketch": sketch, "region": found[0].key,
                                                  "exact": exact,
                                                  "regions": found.iter().map(|r| &r.key)
                                                      .collect::<Vec<_>>()}),
                None => Value::Null,
            },
            // Sheet-metal Flat Patterns (mitcad#208).
            Query::FlatPattern { body } => self.flat_pattern_json(body),
            Query::FlatPoint { body, flat, model } => {
                self.flat_point_json(body, flat, model).map_err(ApiError)?
            }
            Query::SheetReference {
                body,
                face,
                edge,
                at,
            } => self
                .sheet_reference_json(body, face.as_deref(), edge.as_deref(), at)
                .map_err(ApiError)?,
            Query::SheetSeam {
                body,
                stationary,
                moving,
            } => self
                .sheet_seam_json(body, stationary, moving)
                .map_err(ApiError)?,
            Query::SheetBodies => self.sheet_bodies_json(),
            Query::Policy(query) => self.policy_query_json(&query)?,
            Query::Policies => self.policies_json()?,
            Query::SketchSupports(query) => self.sketch_supports_json(&query)?,
            // Moving an occurrence to another parent (mitcad#219).
            Query::ReparentOccurrence(query) => self.reparent_json(&query)?,
            // Body edges and vertices in a sketch's plane (mitcad#256).
            Query::PlaneGeometry {
                sketch,
                out_of_plane,
            } => self.plane_geometry_json(sketch, out_of_plane)?,
            // Tangent chains of edges and faces (mitcad#290).
            Query::TangentChains(query) => self.tangent_chains_json(&query)?,
            // A thread's default end chamfer (mitcad#304).
            Query::ThreadChamfer(query) => threads::thread_chamfer_json(&query)?,
            // The BUILD tab (mitcad#375).
            Query::BuildPreview(fields) => self.build_preview_json(fields)?,
            Query::BuildParts => self.build_parts_json(),
            Query::BuildJunctions(query) => self.build_junctions_json(&query)?,
            Query::BuildCutList(query) => self.build_cut_list_json(&query)?,
            Query::BuildHost => self.host_json(),
            // The library parts the design uses (mitcad#418).
            Query::LibraryUsage => self.library_usage_json(),
            // Motion studies and contact sets (mitcad#454).
            Query::MotionStudies => self.motion_studies_json(),
            Query::StudyValues(query) => self.study_values_json(&query)?,
            Query::ContactSets => self.contact_sets_json(),
            Query::ContactCheck => self.contact_check_json()?,
            Query::MotionLinkOffset(query) => self.motion_link_offset_json(*query)?,
            // Bodies moved into another component (mitcad#465).
            Query::TransferBodies(query) => self.transfer_plan_json(&query)?,
            // Gears (mitcad#472).
            Query::GearValues(query) => self.gear_values_json(&query)?,
            Query::GearLink(query) => self.gear_link_json(&query),
            // An SVG drawing (mitcad#473).
            Query::SvgInfo { path } => sketch::svg_info_json(&path)?,
            // Typed property sets (mitcad#496).
            Query::PropertySets(query) => self.property_sets_json(&query)?,
            Query::PropertyTypes => self.property_types_json(),
            Query::PropertySearch(query) => self.property_search_query_json(&query)?,
            // Mount frames (mitcad#525).
            Query::MountFrames(query) => self.mount_frames_json(&query)?,
            Query::MountPreview(query) => self.mount_preview_json(*query)?,
            // Pulleys (mitcad#562).
            Query::PulleyValues(query) => self.pulley_values_json(&query)?,
            // Printable bearings (mitcad#577).
            Query::BearingValues(query) => self.bearing_values_json(&query)?,
            Query::BearingSizes(query) => self.bearing_sizes_json(&query)?,
            // Seats on shafts (mitcad#578).
            Query::SeatFrame(query) => self.seat_frame_json(&query)?,
            // Print Analysis (mitcad#620).
            Query::PrintOverhangs(query) => self.print_overhangs_json(&query)?,
            Query::PrintThickness(query) => self.print_thickness_json(&query)?,
            Query::PrintEstimate(query) => self.print_estimate_json(&query)?,
            // Materials (mitcad#640).
            Query::Materials => self.materials_json(),
            Query::BodyMaterial(query) => self.body_material_query(&query)?,
            Query::MaterialConflicts(query) => self.material_conflicts_json(&query)?,
            Query::PrintKeys(query) => materials::print_keys_json(&query)?,
            // Print profiles and settings (mitcad#640).
            Query::PrintProfiles => self.print_profiles_json(),
            Query::PrintSettings(query) => self.print_settings_query(&query)?,
            Query::BuildVolume => self.build_volume_json(),
            Query::PrintLibrary(query) => self.print_library_query(&query),
            // Placement by picked geometry, Gear Pair and Belt Drive (mitcad#656).
            Query::PlaceFrame(query) => self.place_frame_json(&query)?,
            Query::GearPair(query) => self.gear_pair_json(&query)?,
            Query::BeltDrive(query) => self.belt_drive_json(&query)?,
            Query::BeltCheck(query) => self.belt_check_json(&query)?,
            // Choosable materials and profiles where they live (mitcad#655).
            Query::LibraryItems(query) => self.library_items_query(&query)?,
            // Measurement objects (mitcad#649).
            Query::Measurements { measure_hidden } => self.annotations_json(measure_hidden),
            // Annotations (mitcad#702).
            Query::Annotations { measure_hidden } => self.annotations_json(measure_hidden),
            Query::GeometricCharacteristics => annotations::geometric_characteristics_json(),
            Query::NextDatumLetter => {
                json!({"letter": self.next_datum_letter().map_err(ApiError)?})
            }
            // The parameters as CSV (mitcad#666).
            Query::ParametersCsv => self.parameters_csv_json(),
            Query::ParametersCsvPreview(query) => self.parameters_csv_preview_json(&query)?,
            // Purge volumes on filament changes (mitcad#705).
            Query::PurgeVolumes(query) => self.purge_volumes_query(&query)?,
            Query::PurgeDefaults(query) => purge_volumes::purge_defaults_query(&query)?,
            // Drawings kept in the design (mitcad#707).
            Query::Drawings => self.drawings_json(),
            Query::Drawing(query) => self.drawing_json(&query)?,
            Query::DrawingPick(query) => self.drawing_pick_json(&query)?,
            Query::DrawingTemplates(query) => self.drawing_templates_json(&query),
            // Copy of sketch geometry for a paste (mitcad#788).
            Query::SketchFragment(query) => self.sketch_fragment_json(&query)?,
            // Image references (mitcad#782).
            Query::ImageReferences => self.image_references_json(),
            Query::ImageReferenceImage { name } => self.image_reference_image_json(&name)?,
            Query::ImageReferenceOutline(query) => self.image_outline_json(&query)?,
            Query::ImageInfo { path } => image_references::image_info_json(&path)?,
            // An installed slicer's user presets (mitcad#830).
            Query::SlicerPresets(query) => self.slicer_presets_query(&query)?,
            // Configurations (mitcad#797).
            Query::Configure(query) => self.configure_json(&query)?,
            Query::CheckRules(query) => self.check_rules_json(&query),
            // Tables of data that expressions look values up in (mitcad#667).
            Query::Tables(query) => self.tables_json(&query)?,
            // A table as CSV (mitcad#804).
            Query::TableCsv(query) => self.table_csv_json(&query)?,
            Query::TableCsvPreview(query) => self.table_csv_preview_json(&query)?,
            // Cut sheets (mitcad#853).
            Query::CutSheet(query) => self.cut_sheet_json(&query)?,
            // The height of the annotations' texts (mitcad#894).
            Query::AnnotationTextHeight => self.annotation_text_height_json(),
            // Clearance holes and head recesses by screw size (mitcad#903).
            Query::HoleSizes(query) => hole_sizes::hole_sizes_json(&query)?,
            // A Fillet's or Chamfer's picked edges kept with its result
            // (mitcad#957).
            Query::FeatureInputs(query) => feature_inputs::feature_inputs_json(self, &query)?,
        };
        Ok(result.to_string())
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait DocumentCrateOps<K: Kernel> {
    /// Runs one command with its changes checked against the design
    /// policy (mitcad#220).
    fn run_command(&mut self, value: Value) -> Result<Value, ApiError>;
}

impl<K: Kernel> DocumentCrateOps<K> for Document<K> {
    fn run_command(&mut self, value: Value) -> Result<Value, ApiError> {
        self.with_policy_checks(|doc| doc.run_checked_command(value))
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above the
/// model.
pub(crate) trait DocumentOps<K: Kernel> {
    /// The features the last recompute evaluated with how long each took
    /// (`recompute_times`).
    fn recompute_times_json(&self) -> Value;
}

impl<K: Kernel> DocumentOps<K> for Document<K> {
    fn recompute_times_json(&self) -> Value {
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
}

/// The document's methods used only in the api's modules, as an extension
/// trait (mitcad#777): the api can then live in a crate above the model.
trait DocumentLocal<K: Kernel> {
    fn run_checked_command(&mut self, value: Value) -> Result<Value, ApiError>;

    fn run_document_command(&mut self, command: Command) -> Result<Value, ApiError>;

    fn checked_preview(&mut self, json: &str) -> Result<String, ApiError>;

    fn require_body(&self, uid: BodyUid) -> Result<&K::Shape, ApiError>;

    fn timeline_json(&self) -> Value;

    /// A feature's suppression condition (mitcad#800), the parameters it
    /// uses and why it does not hold, as the `timeline` and `feature`
    /// queries give them.
    fn add_suppressed_when(&self, uid: FeatureUid, value: &mut Value);

    fn bodies_json(&self, properties: bool, volumes: bool) -> Result<Value, ApiError>;

    /// Adds each body's shape version (`version`, hexadecimal text): it
    /// changes whenever the body's shape does (mitcad#689).
    fn add_body_versions(&self, bodies: &mut Value);

    /// Adds each body's `bbox` {`min`, `max`} as `properties` gives it,
    /// without the other properties (mitcad#714); none for an empty body
    /// or one the kernel cannot measure (MCP's snapshots must not fail on
    /// it).
    fn add_body_boxes(&self, bodies: &mut Value);

    fn add_face_with_rules_command(
        &mut self,
        rules: Value,
        command: &Value,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> DocumentLocal<K> for Document<K> {
    fn run_checked_command(&mut self, value: Value) -> Result<Value, ApiError> {
        let recomputes = self.recompute_count();
        // What failed before, and the feature the command names (mitcad#401).
        let before = self.timeline_before();
        let target = command_target(&value);
        // The family that runs the command (`dispatch.rs`); the document's
        // own commands take every other name.
        let families = Self::command_families();
        let family = value
            .get("cmd")
            .and_then(Value::as_str)
            .and_then(|cmd| families.iter().find(|family| family.runs(cmd)));
        let mut result = if let Some(rules) = inline_face_rules(&value) {
            self.add_face_with_rules_command(rules, &value)?
        } else if let Some(family) = family {
            (family.run)(self, value)?
        } else {
            self.run_document_command(parse(value, "command")?)?
        };
        let stats = self.stats();
        let recomputed = self.recompute_count() != recomputes;
        result["recomputed"] = json!(if recomputed { stats.evaluated.len() } else { 0 });
        // Results read from the result store (P7d), when there were any.
        if recomputed && !stats.restored.is_empty() {
            result["restored"] = json!(stats.restored.len());
            // Of them, those whose bodies' shapes stayed packed: states a
            // later result replaced before anything read them (mitcad#858).
            let packed = self
                .last_result()
                .results
                .iter()
                .filter(|r| stats.restored.contains(&r.uid))
                .filter_map(|r| r.output.as_ref())
                .filter(|o| !o.changes.is_empty() && !o.changes.is_unpacked())
                .count();
            result["restored_packed"] = json!(packed);
        }
        result["error"] = json!(stats.error.map(|e| e.to_string()));
        // Every failed feature, and whether this command made it
        // (mitcad#401), when any failed.
        let failed = self.failed_features(&before, target);
        if !failed.is_empty() {
            result["failed_features"] = json!(failed);
        }
        Ok(result)
    }

    fn run_document_command(&mut self, command: Command) -> Result<Value, ApiError> {
        Ok(match command {
            Command::BeginCheckpoint => json!({"token": self.begin_command()?}),
            Command::RestoreCheckpoint { token } => {
                self.restore_command(&token)?;
                json!({})
            }
            Command::DiscardCheckpoint { token } => {
                self.discard_command(&token)?;
                json!({})
            }
            Command::AddFeature {
                def,
                name,
                component,
                after,
                index,
            } => {
                let component = component
                    .as_deref()
                    .map(|c| self.component_ref(c))
                    .transpose()?;
                let requested = component.unwrap_or(self.active_component());
                // At a timeline position (mitcad#403).
                let position = match (after, index) {
                    (Some(_), Some(_)) => {
                        return Err(ApiError(
                            "add_feature takes either after or index, not both".to_owned(),
                        ));
                    }
                    (Some(after), None) => Some(
                        self.features()
                            .position(|f| f.uid == after)
                            .map(|i| i + 1)
                            .ok_or_else(|| ApiError(format!("feature {after} does not exist")))?,
                    ),
                    (None, index) => index,
                };
                self.check_two_side_distances(&def, None)?;
                let added = match position {
                    Some(position) => {
                        self.add_feature_at(&def, name.as_deref(), component, position)?
                    }
                    None => self.add_feature_to(&def, name.as_deref(), component)?,
                };
                let mut reply = json!({"uid": added.uid, "name": added.name,
                                       "parameters": names(&added.parameters)});
                // What it built with caveats, such as an angle of more than
                // one turn (mitcad#714).
                let warnings = self.warnings(added.uid);
                if !warnings.is_empty() {
                    reply["warnings"] = json!(warnings);
                }
                // A new component placed beside a part (mitcad#292).
                if let Some(made) = self.assembly().created_by(added.uid) {
                    let what = format!("{}'s new component", added.name);
                    self.note_placed_beside(&mut reply, requested, made, &what);
                }
                reply
            }
            Command::EditFeature { uid, def } => {
                let before = self.assembly().created_by(uid);
                self.check_two_side_distances(&def, Some(uid))?;
                let created = self.edit_feature(uid, &def)?;
                let mut reply = json!({"parameters": names(&created)});
                let warnings = self.warnings(uid);
                if !warnings.is_empty() {
                    reply["warnings"] = json!(warnings);
                }
                // A new component the edit made, placed beside a part (mitcad#292).
                if let (None, Some(made), Some(entry)) =
                    (before, self.assembly().created_by(uid), self.feature(uid))
                {
                    let what = format!("{}'s new component", entry.name);
                    self.note_placed_beside(&mut reply, entry.component, made, &what);
                }
                reply
            }
            Command::DeleteFeature { uid, dependents } => {
                let deleted = self.delete_feature(uid, dependents)?;
                json!({"deleted": deleted})
            }
            Command::SuppressFeature {
                uid,
                suppressed,
                expression,
            } => {
                match (suppressed, expression) {
                    (Some(_), Some(_)) => return Err(both("suppressed")),
                    (_, Some(expression)) => self.bind_suppressed(uid, &expression)?,
                    (suppressed, None) => self.set_suppressed(uid, suppressed.unwrap_or(true))?,
                }
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
            Command::SetMarker {
                position,
                temporary,
            } => {
                if temporary {
                    self.set_temporary_marker(position)?;
                } else {
                    self.set_marker(position)?;
                }
                json!({})
            }
            Command::AddParameter {
                name,
                value,
                expression,
                unit,
                comment,
                min,
                max,
                step,
                strict,
                values,
            } => {
                let unit = unit.as_deref().map(parse_unit).transpose()?;
                let limits = limits::LimitInput {
                    min,
                    max,
                    step,
                    strict,
                    values,
                }
                .apply_to(mitcad_model::expr::ParamLimits::default())?;
                match expression_of(value, expression)? {
                    Expression::Number(value) => {
                        let unit = unit.unwrap_or(Unit::of_length(
                            self.parameters().context().default_length_unit,
                        ));
                        let text = value_to_expression(value, unit);
                        self.add_parameter_with_limits(&name, &text, Some(unit), &comment, limits)?;
                    }
                    Expression::Text(text) => {
                        self.add_parameter_with_limits(&name, &text, unit, &comment, limits)?;
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
                min,
                max,
                step,
                strict,
                values,
            } => {
                let unit = unit.as_deref().map(parse_unit).transpose()?;
                let mut changed = Vec::new();
                let limits = limits::LimitInput {
                    min,
                    max,
                    step,
                    strict,
                    values,
                };
                // The cascade of configuration rules runs on a value change
                // (mitcad#799); a comment or a favourite moves nothing.
                let edits_value =
                    limits.is_given() || value.is_some() || expression.is_some() || unit.is_some();
                if limits.is_given() {
                    // With the expression and unit in one step (mitcad#665).
                    let current = self
                        .parameters()
                        .find(&name)
                        .and_then(|id| self.parameters().get(id))
                        .map(|p| p.limits().clone())
                        .ok_or_else(|| ApiError(format!("unknown parameter '{name}'")))?;
                    let limits = limits.apply_to(current)?;
                    let expression = match (value, expression) {
                        (None, None) => None,
                        (value, expression) => Some(match expression_of(value, expression)? {
                            Expression::Number(value) => {
                                let id = self.parameters().find(&name).expect("found above");
                                let unit = unit.unwrap_or_else(|| {
                                    self.parameters().get(id).expect("found").unit()
                                });
                                value_to_expression(value, unit)
                            }
                            Expression::Text(text) => text,
                        }),
                    };
                    changed =
                        self.set_parameter_limits(&name, limits, expression.as_deref(), unit)?;
                } else if value.is_some() || expression.is_some() || unit.is_some() {
                    changed = if value.is_none() && expression.is_none() {
                        // Only a new unit: the expression stays.
                        let expression = self
                            .parameters()
                            .find(&name)
                            .and_then(|id| self.parameters().get(id))
                            .map(|p| p.expression().to_owned())
                            .ok_or_else(|| ApiError(format!("unknown parameter '{name}'")))?;
                        self.set_parameter_expression(&name, &expression, unit)?
                    } else {
                        match expression_of(value, expression)? {
                            // From the table's change set: a text parameter's
                            // value is always 0 (mitcad#758).
                            Expression::Number(value) if unit.is_none() => {
                                self.set_parameter_value(&name, value)?
                            }
                            Expression::Number(value) => {
                                let unit = unit.expect("checked");
                                let text = value_to_expression(value, unit);
                                self.set_parameter_expression(&name, &text, Some(unit))?
                            }
                            Expression::Text(text) => {
                                self.set_parameter_expression(&name, &text, unit)?
                            }
                        }
                    };
                }
                let moves = if edits_value {
                    self.configuration_moves().to_vec()
                } else {
                    Vec::new()
                };
                if let Some(comment) = comment {
                    self.set_parameter_comment(&name, &comment)?;
                }
                if let Some(favorite) = favorite {
                    self.set_parameter_favorite(&name, favorite)?;
                }
                let mut answer = json!({"changed": changed});
                if !moves.is_empty() {
                    // The moved options changed too.
                    for m in &moves {
                        if !changed.contains(&m.option) {
                            changed.push(m.option.clone());
                        }
                    }
                    answer = json!({
                        "changed": changed,
                        "moves": configurations::moves_json(&moves),
                    });
                }
                answer
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
            Command::SetBodyVisible {
                uid,
                visible,
                expression,
            } => {
                match (visible, expression) {
                    (Some(_), Some(_)) => return Err(both("visible")),
                    (None, None) => {
                        return Err(ApiError(
                            "set_body_visible needs `visible` or `expression`".to_owned(),
                        ));
                    }
                    (Some(visible), None) => self.set_body_visible(uid, visible)?,
                    (None, Some(expression)) => self.bind_body_visible(uid, &expression)?,
                }
                json!({})
            }
            Command::SetBodyMaterial {
                expression: Some(expression),
                uid,
                material,
                appearance,
                library,
                project,
            } => {
                if material.is_some()
                    || appearance.is_some()
                    || library.is_some()
                    || project.is_some()
                {
                    return Err(ApiError(
                        "an `expression` is given without `material`, `appearance`, `library` \
                         and `project`"
                            .to_owned(),
                    ));
                }
                self.bind_body_material(uid, &expression)?;
                json!({})
            }
            Command::SetBodyMaterial {
                expression: None,
                uid,
                material,
                appearance,
                library,
                project,
            } => {
                let choice = match appearance.as_deref() {
                    Some(text) => mitcad_model::AppearanceChoice::parse(text)?,
                    None => mitcad_model::AppearanceChoice::Auto,
                };
                match (library.as_deref(), material) {
                    (Some(library), Some(material)) => {
                        let library = mitcad_model::internal::document::library_kind(library)?;
                        let libraries =
                            self.print_libraries(project.as_deref().map(std::path::Path::new));
                        self.as_one_step(|doc| {
                            let id = doc.library_item_in_design(
                                &libraries,
                                library,
                                mitcad_model::print_library::ItemKind::Material,
                                &material,
                            )?;
                            doc.set_body_material_with(uid, Some(id.as_str()), choice)
                        })?;
                    }
                    (Some(_), None) => {
                        return Err(ApiError(
                            "a library is given with the material it has".to_owned(),
                        ));
                    }
                    (None, material) => {
                        self.set_body_material_with(uid, material.as_deref(), choice)?
                    }
                }
                json!({})
            }
            Command::SetBodyAppearance {
                uid,
                appearance,
                expression,
            } => {
                match (appearance, expression) {
                    (Some(_), Some(_)) => return Err(both("appearance")),
                    (_, Some(expression)) => self.bind_body_appearance(uid, &expression)?,
                    (appearance, None) => self.set_body_appearance(uid, appearance.as_deref())?,
                }
                json!({})
            }
            Command::SetFaceAppearance {
                uid,
                faces,
                appearance,
                expression,
            } => {
                match (appearance, expression) {
                    (Some(_), Some(_)) => return Err(both("appearance")),
                    (_, Some(expression)) => self.bind_face_appearance(uid, &faces, &expression)?,
                    (appearance, None) => {
                        self.set_face_appearance(uid, &faces, appearance.as_deref())?
                    }
                }
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
            // Sheet-metal Flat Patterns (mitcad#208).
            Command::SetFlatPattern {
                body,
                stationary,
                x_axis,
                reference_side,
                shown,
            } => {
                self.set_flat_pattern(body, stationary, x_axis, reference_side, shown)?;
                json!({})
            }
            Command::RemoveFlatPattern { body } => {
                self.remove_flat_pattern(body)?;
                json!({})
            }
            Command::ExportFlatPatternDxf(command) => self.export_flat_pattern_command(command)?,
            Command::ExportDrawing(command) => self.export_drawing_command(command)?,
            Command::SetPrintBedFace { uid, face } => {
                self.set_print_bed_face(uid, face.as_deref())?;
                json!({})
            }
        })
    }

    fn checked_preview(&mut self, json: &str) -> Result<String, ApiError> {
        let value = read_json(json)?;
        // Motions: a study's playback, a driven joint (mitcad#454).
        if let Some((report, outcome)) = self.motion_preview(&value)? {
            let mut value = preview_json(&report);
            if let (Value::Object(out), Value::Object(more)) = (&mut value, outcome) {
                out.extend(more);
            }
            return Ok(value.to_string());
        }
        // Extrude's automatic operation (mitcad#289).
        let mut auto = None;
        let report = if let Some(rules) = inline_face_rules(&value) {
            let (rules, face, name) = face_with_rules(rules, &value)?;
            self.preview_face_with_rules(&rules, &face, name.as_deref())?
        } else {
            let command = parse::<PreviewCommand>(value, "preview command")?;
            // As the command would (mitcad#693).
            match &command {
                PreviewCommand::AddFeature { def, .. } => {
                    self.check_two_side_distances(def, None)?;
                }
                PreviewCommand::EditFeature { uid, def, .. } => {
                    self.check_two_side_distances(def, Some(*uid))?;
                }
            }
            match command {
                PreviewCommand::AddFeature {
                    def,
                    name,
                    auto_operation: false,
                } => self.preview_add(&def, name.as_deref())?,
                PreviewCommand::AddFeature { def, name, .. } => {
                    let (report, choice) = self.preview_add_auto(&def, name.as_deref())?;
                    auto = choice;
                    report
                }
                PreviewCommand::EditFeature {
                    uid,
                    def,
                    auto_operation: false,
                } => self.preview_edit(uid, &def)?,
                PreviewCommand::EditFeature { uid, def, .. } => {
                    let (report, choice) = self.preview_edit_auto(uid, &def)?;
                    auto = choice;
                    report
                }
            }
        };
        let mut value = preview_json(&report);
        if let Some(datum) = self.preview_datum() {
            value["datum"] = analysis::datum_json(&datum);
        }
        if let Some(choice) = auto {
            value["auto_operation"] =
                json!({"operation": choice.operation, "objects": choice.objects});
        }
        Ok(value.to_string())
    }

    fn require_body(&self, uid: BodyUid) -> Result<&K::Shape, ApiError> {
        self.body_shape(uid)
            .ok_or_else(|| ApiError(format!("body {uid} does not exist at the timeline marker")))
    }

    fn add_suppressed_when(&self, uid: FeatureUid, value: &mut Value) {
        let Some(text) = self.suppressed_when(uid) else {
            return;
        };
        value["suppressed_when"] = json!(text);
        value["dependencies"] = json!(mitcad_model::DocState::expression_parameters([text]));
        if let Some(warning) = self.state().feature_binding_warning(uid) {
            value["binding_warning"] = json!(warning);
        }
    }

    fn timeline_json(&self) -> Value {
        let used = self.state().used_sketches();
        let sketches = self.state().sketches_used_by();
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
                // What the importer said about it (mitcad#170): also in
                // `error`, first.
                if let Some(note) = item.import_note {
                    value["import"] = json!(note);
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
                // The sketches it uses: the browser lists it under each
                // (mitcad#177).
                if let Some(uses) = sketches.get(&item.entry.uid) {
                    value["sketches"] = json!(uses);
                }
                self.add_suppressed_when(item.entry.uid, &mut value);
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
            // The app's inputs of surface operations leave mesh bodies out
            // (mitcad#399).
            if mitcad_model::internal::features::is_mesh(self.kernel(), body.shape) {
                value["mesh"] = json!(true);
            }
            if volumes && !properties {
                let mass = self
                    .kernel()
                    .mass_properties(body.shape)
                    .map_err(|e| ApiError(format!("{}: {e}", body.name)))?;
                value["volume"] = json!(mass.volume);
            }
            if properties {
                let kernel = self.kernel();
                let error =
                    |e: mitcad_model::kernel::KernelError| ApiError(format!("{}: {e}", body.name));
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

    fn add_body_versions(&self, bodies: &mut Value) {
        for body in bodies.as_array_mut().into_iter().flatten() {
            let version = body["uid"]
                .as_str()
                .and_then(|uid| uid.parse::<BodyUid>().ok())
                .and_then(|uid| self.body_version(uid));
            if let Some(version) = version {
                body["version"] = json!(format!("{version:032x}"));
            }
        }
    }

    fn add_body_boxes(&self, bodies: &mut Value) {
        for body in bodies.as_array_mut().into_iter().flatten() {
            let bounds = body["uid"]
                .as_str()
                .and_then(|uid| uid.parse::<BodyUid>().ok())
                .and_then(|uid| self.body_shape(uid))
                .and_then(|shape| self.kernel().bounding_box(shape).ok().flatten());
            if let Some(bounds) = bounds {
                body["bbox"] = json!({"min": bounds.min, "max": bounds.max});
            }
        }
    }

    fn add_face_with_rules_command(
        &mut self,
        rules: Value,
        command: &Value,
    ) -> Result<Value, ApiError> {
        for key in command.as_object().into_iter().flat_map(|o| o.keys()) {
            if !matches!(key.as_str(), "cmd" | "def" | "name" | "component") {
                return Err(ApiError(format!("invalid command: unknown field `{key}`")));
            }
        }
        let (rules, face, name) = face_with_rules(rules, command)?;
        let component = command
            .get("component")
            .and_then(Value::as_str)
            .map(|c| self.component_ref(c))
            .transpose()?;
        let added = self.add_face_with_rules(&rules, &face, name.as_deref(), component)?;
        let rules = self
            .features()
            .take_while(|f| f.uid != added.uid)
            .last()
            .map(|f| f.uid);
        Ok(json!({"uid": added.uid, "name": added.name, "rules": rules,
                  "parameters": names(&added.parameters)}))
    }
}

/// A hash of a profile region's geometry and sketch frame (the `profiles`
/// query's `hash`): the same while the face built from them would be. The
/// numbers are hashed by their bits (P9: formatting them through `Debug`
/// took 90-100 ms for the 594 profiles of a large corpus design).
fn profile_hash(frame: &SketchFrame, region: &ProfileRegion) -> String {
    use mitcad_model::profile::SegmentGeometry as G;
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
                G::Circle { center, radius, .. } => {
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
    if let Some(anchor) = &report.anchor {
        value["anchor"] = json!(anchor);
    }
    // Where a sweep's profile lies on its path (mitcad#795).
    if let Some(place) = &report.sweep_place {
        value["sweep_path"] = json!(place);
    }
    // What a move's motion was worked out from (mitcad#838).
    if let Some(basis) = &report.move_basis {
        value["motion"] = json!(basis);
    }
    if !report.instances.is_empty() {
        value["instances"] = json!(
            report
                .instances
                .iter()
                .map(|i| {
                    let path = i
                        .path
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("/");
                    json!({"body": i.body, "occurrence": path,
                "transform": mitcad_model::assembly::matrix4(&i.transform), "visible": i.visible})
                })
                .collect::<Vec<_>>()
        );
    }
    if !report.removed_instances.is_empty() {
        value["removed_instances"] = json!(report.removed_instances.iter().map(|(path, body)| {
            json!({"body":body,"occurrence":path.iter().map(ToString::to_string).collect::<Vec<_>>().join("/")})
        }).collect::<Vec<_>>());
    }
    if let Some(targets) = &report.cut_targets {
        value["cut_targets"] = json!(targets);
    }
    value["sheet_states"] = report.sheet_states.clone();
    if !report.elements.is_empty() {
        let elements: Vec<_> = report
            .elements
            .iter()
            .map(mitcad_model::assembly::matrix_rows)
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
                json!({"path": path.join("/"), "transform": mitcad_model::assembly::matrix4(t)})
            })
            .collect();
        value["placements"] = json!(placements);
    }
    // Where the tool is in the design: its component's occurrences
    // (mitcad#636).
    if !report.tool_instances.is_empty() {
        let instances: Vec<Value> = report
            .tool_instances
            .iter()
            .map(|(path, t, visible)| {
                let path: Vec<String> = path.iter().map(ToString::to_string).collect();
                json!({"occurrence": path.join("/"), "transform": mitcad_model::assembly::matrix4(t),
                       "visible": visible})
            })
            .collect();
        value["tool_instances"] = json!(instances);
    }
    value
}

/// The feature a command edits by name (mitcad#401): `edit_feature`,
/// `suppress_feature` and `reorder_feature` its `uid`, a sketch command
/// its `sketch`. An edit that changes only the values of its dimensions
/// leaves the definition as it was, so it is not found by comparing them.
fn command_target(value: &Value) -> Option<FeatureUid> {
    let cmd = value.get("cmd")?.as_str()?;
    let field = match cmd {
        "edit_feature" | "suppress_feature" | "reorder_feature" => "uid",
        _ if cmd.starts_with("sketch.") => "sketch",
        _ => return None,
    };
    value.get(field)?.as_str()?.parse().ok()
}

// A Face with new rules of its own (mitcad#210): `add_feature` (and its
// preview) of a `sheet_face` whose `rules` is the rules' definition adds a
// `sheet_rules` feature just before the Face, in one undo step.

/// The rules' definition of such a command, or `None` for any other.
fn inline_face_rules(value: &Value) -> Option<Value> {
    if value.get("cmd").and_then(Value::as_str) != Some("add_feature") {
        return None;
    }
    let def = value.get("def")?;
    if !matches!(
        def.get("type").and_then(Value::as_str),
        Some("sheet_face" | "sheet_contour" | "sheet_convert")
    ) {
        return None;
    }
    let mut rules = def.get("rules")?.as_object()?.clone();
    rules.entry("type").or_insert_with(|| json!("sheet_rules"));
    Some(Value::Object(rules))
}

type FaceFor = Box<dyn Fn(FeatureUid) -> Result<FeatureDef<ValueInput>, mitcad_model::ModelError>>;

/// The rules' definition, the Face's for the rules' uid, and the Face's name.
fn face_with_rules(
    rules: Value,
    command: &Value,
) -> Result<(FeatureDef<ValueInput>, FaceFor, Option<String>), ApiError> {
    let rules: FeatureDef<ValueInput> = parse(rules, "sheet rules")?;
    if !matches!(rules, FeatureDef::SheetRules(_)) {
        return Err(ApiError("a Face's own rules must be sheet_rules".into()));
    }
    let face = command["def"].clone();
    // The Face itself must be valid apart from its rules.
    let mut probe = face.clone();
    probe["rules"] = json!("F1");
    parse::<FeatureDef<ValueInput>>(probe, "feature definition")?;
    let name = command
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let make: FaceFor = Box::new(move |uid| {
        let mut def = face.clone();
        def["rules"] = json!(uid);
        serde_json::from_value(def).map_err(|e| {
            mitcad_model::ModelError::Invalid(format!("invalid feature definition: {e}"))
        })
    });
    Ok((rules, make, name))
}
