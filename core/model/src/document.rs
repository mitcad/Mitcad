// SPDX-License-Identifier: MIT
//! The parametric part document: the definition state (parameters, the
//! timeline with its marker, body names), the commands that change it, undo
//! and redo, and the results of the last recompute.
//!
//! Every command works on a copy of the definition state and replaces it
//! only when the command succeeds, so a rejected command changes nothing.
//! The replaced state goes onto the undo stack with a label; the state is
//! cheap to copy (features are shared until changed), and recompute reuses
//! cached results, so undo and redo are fast. After each command the
//! document recomputes and names new bodies (`Body1`, ...).
//!
//! Every state the document takes has a revision number, which undo and
//! redo take back with the state, so the document can tell whether it is
//! the state last saved (P8) without comparing design files.
//!
//! A command computes its new state before it takes it, so a computation
//! cancelled through the document's monitor (P7, [`Document::set_monitor`])
//! rejects the command like any other error: the state, the undo and redo
//! steps, the revision and the results stay as they were.

mod bodies;
// Browser and timeline (U3).
mod browser;
mod components;
mod copy;
mod datums;
mod point_patterns;
// Named views (U5).
mod views;
// The Origin folder, Isolate and favourite parameters (P9).
mod display;
// Timeline groups (P9).
mod groups;
// Analyses kept in the document (mitcad#41).
mod analyses;
// Appearances kept in the document (mitcad#46).
mod appearances;
// Render settings of the document (mitcad#47).
mod render_settings;
// Joints between occurrences (mitcad#55).
mod joints;
// Library components and configuration tables (mitcad#64).
mod library_parts;
pub use library_parts::{LibraryChange, LibraryPart, PartsListRow};
// Configurations: options, the cascade of rules, `configure` (mitcad#797).
pub(crate) mod configurations;
pub use configurations::ConfigurationsSet;
// Tables in configurations: rows generated from a table, options moved by
// a table edit (mitcad#806).
pub(crate) mod configuration_tables;
// Move Features into a Component (mitcad#171).
mod move_features;
pub use move_features::{MoveConflict, MoveGroup, MovePlan, MovedGroup, PlannedGroup};
pub use move_features::{MoveDestination, MovedDestination};
// What the importers say about the features they made (mitcad#170).
mod import_notes;
pub use import_notes::{status_message as noted_message, status_text as noted_status};
// Origin groups of the final unused sketch (mitcad#193).
mod autoselect;
pub use autoselect::AutoselectProfiles;
// Sheet-metal Flat Patterns kept in the document (mitcad#208).
mod flat_patterns;
// Design policy: per-component creation and link permissions (mitcad#220).
mod policy;
pub use policy::{PolicyItem, PolicyReport};
// Moving an occurrence to another parent (mitcad#219).
mod reparent;
pub use reparent::{Carried, ReparentPlan};
// Body edges and vertices in a sketch's plane (mitcad#256).
mod plane_geometry;
pub use plane_geometry::PlaneShape;
// Features added many at once (the IFC import, mitcad#273).
mod feature_batch;
pub use feature_batch::FeatureBatch;
// Extrude's automatic operation (mitcad#289).
mod auto_operation;
pub use auto_operation::AutoOperation;
// The failed features after a command, and whether it made them
// (mitcad#401); a feature added at a timeline position (mitcad#403).
mod command_outcome;
pub use command_outcome::TimelineBefore;
mod insert;
// Motion studies and contact sets (mitcad#454).
mod motion;
pub use joints::Driven;
pub(crate) use motion::joint_motions;
pub use motion::{JointValues, MotionOutcome};
// Motion links between joints (mitcad#466).
mod motion_links;
// Bodies moved into another component on the timeline (mitcad#465).
mod body_transfer;
pub use body_transfer::TransferDestination;
// Typed property sets (mitcad#496).
mod properties;
pub use properties::PropertyTypeChange;
// Reorient Design (mitcad#502).
mod reorient;
pub use reorient::ReorientReport;
// Mount frames of any part and placing a part by one (mitcad#525).
mod mounts;
pub use mounts::{
    FrameAt, GridPlace, Inserted, MountAt, MountFrame, MountInput, MountPlan, MountPreview,
    MountSource,
};
// Materials as objects, component default materials (mitcad#640).
mod materials;
pub use materials::{AppearanceChoice, DeletedMaterial, MaterialSource};
// Printer and process profiles, the Print property sets and what a body
// prints with (mitcad#640, part 3).
mod print_settings;
pub use print_settings::{EffectivePrint, PrintFrom, PrintTarget, print_key_name};
// The design's copies of library materials and profiles (mitcad#640, part
// 4).
mod print_library;
pub use print_library::LibraryUpdate;
// Importing a slicer's print settings (mitcad#640, part 5).
mod print_import;
pub use print_import::read_print_file;
// Materials and print profiles listed, edited and chosen where they live
// (mitcad#655).
mod library_items;
pub use library_items::{library_kind, writable_library};
// Annotations: dimensions and notes kept in the design (mitcad#702), grown
// from the measurement objects of mitcad#649.
mod annotations;
pub use annotations::normalized_color;
pub use annotations::{
    AUTO_COLOR, Annotation, AnnotationFormat, AnnotationPlane, Characteristic, DatumNeed,
    GeometricTolerance, MAX_DECIMALS, MaterialCondition, MeasureKind, MeasurePick, Measurement,
    NewMeasurement, OLD_DEFAULT_COLORS, TEXT_HEIGHTS, Tolerance, automatic_text_height,
    check_text_height, datum_letter,
};
pub(crate) use annotations::{
    check_format, check_frame, check_tolerance, read_old_defaults, settle_frames,
};
// The parameters exported to and imported from CSV (mitcad#666).
mod parameters_csv;
pub use parameters_csv::{CsvAction, CsvImport, CsvLimits, CsvRowOutcome};
// Purge volumes on filament changes: the table of the filament slots and
// the pairs set by hand (mitcad#705).
mod purge_volumes;
pub use purge_volumes::PurgeChange;
pub use purge_volumes::purge_defaults_json;
// Drawings kept in the root's and components' Drawings folders (mitcad#707).
mod drawings;
pub use drawings::deleted_views;
// A body's print bed face, which the 3D print export turns down onto the
// plate (mitcad#731).
mod print_bed;
pub use print_bed::PrintBedFace;
// Runtime caches kept beside the results, one per type (mitcad#777).
mod side_caches;
// Redefine Sketch Plane: a sketch moved to another plane and placed there
// (mitcad#787).
mod sketch_plane;
pub use sketch_plane::SketchPlaneChange;
// Image references: images placed on planes and faces to model from
// (mitcad#782).
mod image_references;
pub use image_references::{
    CalibrationPoint, EmbeddedImage, ImageEdit, ImageReference, ImageSize, ImageSource, ImageTarget,
};
// The user presets of an installed Bambu-family slicer, listed and
// imported (mitcad#830).
mod slicer_presets;
pub use slicer_presets::{PresetChoice, SameName};
// Appearance, material, suppression and visibility from expressions of
// parameters (mitcad#800).
mod bindings;
pub use bindings::{BindingKind, Bindings};
// Options of inserted components: library parts, linked designs and BUILD
// parts as expressions of the design's parameters (mitcad#801).
mod component_options;
pub use component_options::{ComponentOptions, OptionsChange, OptionsChanged};
// Tables of data that expressions look values up in (mitcad#667).
mod tables;
// A table exported to and imported from CSV (mitcad#804).
mod tables_csv;
pub use tables_csv::{TableCsvChange, TableCsvFailure, TableCsvImport};

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub use analyses::{Analysis, AnalysisDef, SectionAnalysis, SectionPlane};
pub use analyses::{EstimateCheck, OverhangCheck, PrintAnalysis, ThinWallCheck};
pub use analyses::{check_minimum_wall, check_overhang_angle, print_material};
pub use appearances::DeletedAppearance;
pub use bodies::{BodyAttributes, FaceAppearance};
pub use components::{InsertOptions, InstanceView};
pub use datums::{DatumView, DocResolver};
pub use display::{DisplayState, Isolated};
pub use groups::TimelineGroup;
pub use views::NamedView;

use crate::appearance::Appearance;
use crate::assembly::Assembly;
use crate::expr::ParamLimits;
use crate::expr::{LengthUnit, ParamSpec, Unit, value_to_expression, with_decimal_points};
use crate::features::sketch::LinkedReferences;
use crate::features::{
    CheckContext, FeatureDef, FeatureEntry, HelixConstruction, SketchDef, SketchPlane, ValueInput,
    slot_label,
};
use crate::ids::{BodyUid, ComponentUid, EntityUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLinkExt;
use crate::monitor::RecomputeMonitor;
use crate::parameters::{ParamId, ParameterError, Parameters, fits_slot, slot_unit, unit_for};
use crate::profile::{ProfileRegion, SketchFrame};
use crate::recompute::{
    Cache, FeatureStatus, FinalBodies, Recomputed, def_fingerprint, recompute, recompute_after,
};
use crate::render_settings::RenderSettings;
use crate::sketch::edit::{EditReport, SketchEdit};
use crate::store::{self, PersistReport, ResultStore};
use crate::topo::RegionKey;
use crate::topo::SegmentKey;
use mitcad_model_base::internal::parameters::expression_dims;

// Moved down to the base so that the sketch resolves its values as the
// document does (mitcad#780).
use mitcad_model_base::internal::values::resolve_value_as;
pub use mitcad_model_base::internal::values::split_named;
pub use mitcad_model_base::values::ModelError;

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

/// The definition state: everything a design file saves.
#[derive(Debug, Clone, PartialEq)]
pub struct DocState {
    pub parameters: Parameters,
    /// In timeline order.
    pub features: Vec<Arc<FeatureEntry>>,
    /// The number of features before the timeline marker; features after it
    /// are rolled back (not evaluated), and new features go in at it.
    pub marker: usize,
    /// Display names given to bodies, by the commands or the user.
    pub body_names: BTreeMap<BodyUid, String>,
    /// Visibility, material and appearance of bodies; only those that
    /// differ from the default are listed.
    pub body_attributes: BTreeMap<BodyUid, BodyAttributes>,
    /// The number of the next feature uid.
    pub(crate) next_uid: u64,
    /// Components, occurrences and the active component (F6).
    pub assembly: Assembly,
    /// Light bulbs of sketches and construction features set by the user
    /// (browser, U3) where they differ from the default (a sketch hidden
    /// while a feature uses it, construction geometry shown; see
    /// `document/browser.rs`).
    pub feature_visibility: BTreeMap<FeatureUid, bool>,
    /// Cameras saved by name (U5), in the order they were added.
    pub named_views: Vec<NamedView>,
    /// The Origin folder's light bulb and Isolate (P9).
    pub(crate) display: DisplayState,
    /// Favourite parameters (P9: Change Parameters' star).
    pub favorites: BTreeSet<ParamId>,
    /// Timeline groups (P9), in the order they were made.
    pub groups: Vec<TimelineGroup>,
    /// Analyses kept in the document (mitcad#41), in the order they were
    /// added.
    pub analyses: Vec<Analysis>,
    /// The document's own appearances (mitcad#46), in the order they were
    /// made; the library's are not listed.
    pub appearances: Vec<Appearance>,
    /// How the rendered view lights and shows the design (mitcad#47).
    pub render: RenderSettings,
    /// The configuration table (mitcad#64): sizes of the design.
    pub configurations: crate::configurations::Configurations,
    /// Tables of data that expressions look values up in (mitcad#667),
    /// in the order they were made. The parameters evaluate with the same
    /// tables: they change together (`document/tables.rs`).
    pub(crate) tables: crate::tables::Tables,
    /// A component made where modelled bodies are does not offer to move
    /// them into a component first (mitcad#171: "don't ask again").
    pub(crate) hybrid_assembly: bool,
    /// What an importer said about the features it made (mitcad#170): a
    /// fallback's base feature, an item that came in partly, a placeholder
    /// of a skipped item. Shown as the feature's warning until an edit of
    /// its definition builds.
    pub(crate) import_notes: BTreeMap<FeatureUid, String>,
    /// Flat Patterns of sheet-metal bodies (mitcad#208), by body.
    pub flat_patterns: BTreeMap<BodyUid, crate::sheet::flat::FlatPatternDef>,
    /// The distance (mm) within which the features' joins still fuse
    /// where they fail exactly ([`Kernel::join_within`], mitcad#298): set
    /// by an importer whose bodies match a file's only within its
    /// geometric tolerance; None (exact joins) for the model's own designs.
    pub(crate) join_tolerance: Option<f64>,
    /// The library parts BUILD placed, each kept once (mitcad#375).
    pub library_sources: crate::build::LibrarySources,
    /// What features remember of the pieces they split bodies into, by
    /// feature (mitcad#408, `features/pieces.rs`): taken from each
    /// recompute, saved with the features.
    pub(crate) split_pieces: crate::features::SplitPieces,
    /// Motion studies (mitcad#454), in the order they were added.
    pub(crate) motion_studies: Vec<crate::motion::MotionStudy>,
    /// Contact sets (mitcad#454), in the order they were added.
    pub(crate) contact_sets: Vec<crate::motion::ContactSet>,
    /// Own property types and the properties of components, occurrences
    /// and bodies (mitcad#496).
    pub(crate) properties: crate::properties::PropertyStore,
    /// The document's own materials (mitcad#640), in the order they were
    /// made; one with a built-in's id takes its place.
    pub materials: Vec<crate::material::Material>,
    /// Default materials of components (C0: the root's), by component.
    pub component_materials: BTreeMap<ComponentUid, String>,
    /// The design's printer and process profiles (mitcad#640, part 3), in
    /// the order they were made.
    pub print_profiles: Vec<crate::print_profile::PrintProfile>,
    /// Annotations (mitcad#702; the measurement objects of mitcad#649), in
    /// the order they were added.
    pub annotations: Vec<Annotation>,
    /// The height (mm) of the annotations' texts (mitcad#894); none for
    /// the automatic one.
    pub(crate) annotation_text_height: Option<f64>,
    /// Purge volumes set by hand for pairs of materials (mitcad#705), in
    /// the order they were set.
    pub(crate) purge_volumes: Vec<crate::print_purge::PurgeOverride>,
    /// Drawings of the root and the components (mitcad#707), in the order
    /// they were made.
    pub drawings: Vec<crate::drawing::def::Drawing>,
    /// Image references (mitcad#782), in the order they were inserted.
    pub(crate) image_references: Vec<ImageReference>,
    /// Suppression of features and visibility of occurrences from
    /// expressions (mitcad#800); those of bodies are in their attributes.
    pub bindings: Bindings,
    /// Where each feature is in `features` (mitcad#841): not part of the
    /// definition, and right whatever changed `features` (see
    /// `feature_index.rs`).
    pub(crate) feature_index: crate::feature_index::FeatureIndex,
}

impl Default for DocState {
    fn default() -> Self {
        Self {
            parameters: Parameters::default(),
            features: Vec::new(),
            marker: 0,
            body_names: BTreeMap::new(),
            body_attributes: BTreeMap::new(),
            next_uid: 1,
            assembly: Assembly::default(),
            feature_visibility: BTreeMap::new(),
            named_views: Vec::new(),
            display: DisplayState::default(),
            favorites: BTreeSet::new(),
            groups: Vec::new(),
            analyses: Vec::new(),
            appearances: Vec::new(),
            render: RenderSettings::default(),
            configurations: Default::default(),
            tables: Default::default(),
            hybrid_assembly: false,
            import_notes: BTreeMap::new(),
            flat_patterns: BTreeMap::new(),
            join_tolerance: None,
            library_sources: Default::default(),
            split_pieces: BTreeMap::new(),
            motion_studies: Vec::new(),
            contact_sets: Vec::new(),
            properties: Default::default(),
            materials: Vec::new(),
            component_materials: BTreeMap::new(),
            print_profiles: Vec::new(),
            annotations: Vec::new(),
            annotation_text_height: None,
            purge_volumes: Vec::new(),
            drawings: Vec::new(),
            image_references: Vec::new(),
            bindings: Bindings::default(),
            feature_index: Default::default(),
        }
    }
}

/// What [`Document::fill_component_copy`] copied: each occurrence and
/// feature of the source document with its copy.
#[derive(Debug, Clone, Default)]
pub struct ComponentCopy {
    pub occurrences: HashMap<OccurrenceUid, OccurrenceUid>,
    pub features: HashMap<FeatureUid, FeatureUid>,
}

/// A feature added by a command.
#[derive(Debug, Clone, PartialEq)]
pub struct Added {
    pub uid: FeatureUid,
    pub name: String,
    /// Dimension parameters the command created.
    pub parameters: Vec<String>,
}

/// A shape added to a sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeAdded {
    pub curves: Vec<EntityUid>,
    pub region: RegionKey,
    pub parameters: Vec<String>,
    /// Everything the edit created, and the sketch's status.
    pub report: EditReport,
}

impl DocState {
    pub fn position(&self, uid: FeatureUid) -> Option<usize> {
        self.feature_index.position(&self.features, uid)
    }

    pub fn entry(&self, uid: FeatureUid) -> Option<&FeatureEntry> {
        self.feature_index.find(&self.features, uid).map(|f| &**f)
    }

    fn require(&self, uid: FeatureUid) -> Result<usize, ModelError> {
        self.position(uid).ok_or(ModelError::NoSuchFeature(uid))
    }

    /// Checks a feature placed at `position` against the features before it.
    pub fn check_feature(&self, position: usize, entry: &FeatureEntry) -> Result<(), String> {
        let ctx = CheckContext {
            uid: entry.uid,
            earlier: &self.features[..position],
            all: &self.features,
            index: &self.feature_index,
        };
        entry.def.info().check(&ctx)?;
        crate::joints::check_generated_references(&self.assembly, &ctx, &entry.def)?;
        if let FeatureDef::Extrude(extrude) = &entry.def
            && let Some(scope) = extrude.active_cut_scope()
        {
            scope.check_components(&self.assembly, &ctx, entry.component)?;
        }
        check_components(&self.assembly, &self.features, &self.feature_index, entry)
    }

    fn check_values(&self, def: &FeatureDef) -> Result<(), ModelError> {
        def.info()
            .check_values(&|id| self.parameters.value(id).unwrap_or(f64::NAN))
            .map_err(ModelError::Invalid)
    }

    fn new_uid(&mut self) -> FeatureUid {
        self.next_uid += 1;
        FeatureUid(self.next_uid - 1)
    }

    /// The next free default name, as in V0: `Extrude3` for the
    /// third extrude, or the first unused number after it.
    fn default_name(&self, base: &str) -> String {
        let count = self
            .features
            .iter()
            .filter(|f| f.def.base_name() == base)
            .count();
        (count + 1..)
            .map(|n| format!("{base}{n}"))
            .find(|name| self.features.iter().all(|f| &f.name != name))
            .expect("an unused name exists")
    }

    fn check_feature_name(&self, name: &str, uid: Option<FeatureUid>) -> Result<(), ModelError> {
        if name.trim().is_empty() {
            return Err(invalid("the feature name is empty"));
        }
        if self
            .features
            .iter()
            .any(|f| f.name == name && Some(f.uid) != uid)
        {
            return Err(invalid(format!("a feature is already named '{name}'")));
        }
        Ok(())
    }

    /// Converts command values to parameters. A name refers to a parameter;
    /// a number sets the parameter of the same slot in `old` if the feature
    /// owns it, or creates a dimension parameter owned by the feature.
    fn resolve(
        &mut self,
        def: &FeatureDef<ValueInput>,
        owner: FeatureUid,
        owner_name: &str,
        old: Option<&FeatureDef>,
    ) -> Result<(FeatureDef, Vec<String>), ModelError> {
        let mut old_slots = HashMap::new();
        if let Some(old) = old {
            let _ = old.map_params(&mut |slot, id: &ParamId| {
                old_slots.insert(slot.to_owned(), *id);
                Ok::<_, ()>(())
            });
        }
        let mut created = Vec::new();
        // A derived part's values take its source's units (mitcad#801).
        let part_units = crate::part_options::slot_units(self, def);
        let params = &mut self.parameters;
        let mut resolved = def.map_params(&mut |slot, value| -> Result<ParamId, ModelError> {
            if let ValueInput::Name(expression) = value {
                let expression =
                    split_named(expression).map_or(expression.as_str(), |(_, rest)| rest);
                created.extend(crate::print_fits::ensure(params, expression)?);
            }
            let comment = format!("{owner_name} {}", slot_label(slot));
            let (id, new) = resolve_value_as(
                params,
                slot,
                part_units.get(slot).copied(),
                value,
                owner,
                &comment,
                old_slots.get(slot),
            )?;
            created.extend(new);
            Ok(id)
        })?;
        // Driven dimensions get their reference parameters (mitcad#231).
        if let FeatureDef::Sketch(sketch) = &mut resolved {
            let new = mitcad_sketch::internal::settle_references(
                sketch,
                params,
                owner,
                owner_name,
                &|slot| old_slots.get(slot).copied(),
                None,
            )
            .map_err(invalid)?;
            created.extend(new);
        }
        // A bound length's parameter is its own reference (mitcad#375).
        if let FeatureDef::BoundLength(bound) = &resolved {
            crate::features::bound_length::settle_parameter(params, bound.parameter, owner)
                .map_err(invalid)?;
        }
        // A growing helix without a construction keeps the one it had, else
        // it is Mitcad's (mitcad#83).
        if let FeatureDef::Helix(helix) = &mut resolved {
            let had = match old {
                Some(FeatureDef::Helix(old)) => old.construction,
                _ => None,
            };
            helix.settle_construction(had.unwrap_or(HelixConstruction::Mitcad));
        }
        if let FeatureDef::SheetFace(face) = &mut resolved {
            face.settle(match old {
                Some(FeatureDef::SheetFace(old)) => Some(old),
                _ => None,
            });
        }
        crate::features::sheet_folded::settle(&mut resolved, old);
        Ok((resolved, created))
    }

    /// The features that refer to any of `of`, directly or through each
    /// other, in timeline order.
    pub fn dependents(&self, of: &BTreeSet<FeatureUid>) -> Vec<FeatureUid> {
        let mut affected = of.clone();
        let mut dependents = Vec::new();
        // A feature only refers to earlier ones, so one pass in order finds
        // all transitive dependents.
        for entry in &self.features {
            if affected.contains(&entry.uid) {
                continue;
            }
            if entry
                .def
                .references()
                .features
                .iter()
                .any(|f| affected.contains(f))
                || crate::joints::referenced_occurrences(&entry.def)
                    .iter()
                    .any(|uid| {
                        self.assembly
                            .occurrence(*uid)
                            .and_then(|o| o.pattern.as_ref())
                            .is_some_and(|p| affected.contains(&p.feature))
                    })
                || matches!(&entry.def, FeatureDef::Extrude(extrude) if extrude.active_cut_scope().is_some_and(|scope|
                    self.features.iter().take_while(|f| f.uid != entry.uid).any(|f| affected.contains(&f.uid)
                        && scope.depends_on(&self.assembly, f))))
            {
                affected.insert(entry.uid);
                dependents.push(entry.uid);
            }
        }
        dependents
    }

    fn names_of(&self, uids: &[FeatureUid]) -> Vec<String> {
        uids.iter()
            .map(|uid| {
                self.entry(*uid)
                    .map_or_else(|| uid.to_string(), |f| f.name.clone())
            })
            .collect()
    }

    /// Removes the parameters a feature owned that nothing uses any more
    /// (no feature and no other parameter's expression); used ones become
    /// user parameters when the feature is gone.
    pub fn release_params(&mut self, owner: FeatureUid) {
        let mut used = used_params(&self.features);
        // Those that bindings use stay too (mitcad#800).
        used.extend(self.binding_param_ids());
        let exists = self.position(owner).is_some();
        self.parameters
            .release(owner, exists, &|id| used.contains(&id));
        // A reference whose driven dimension is gone keeps its last value
        // as a user parameter (mitcad#231).
        if exists {
            let kept: BTreeSet<ParamId> = match self.entry(owner).map(|e| &e.def) {
                Some(FeatureDef::Sketch(def)) => {
                    def.dimensions.iter().filter_map(|d| d.reference).collect()
                }
                // A bound length keeps its own reference (mitcad#375).
                Some(FeatureDef::BoundLength(def)) => BTreeSet::from([def.parameter]),
                _ => BTreeSet::new(),
            };
            for id in self.parameters.owned_by(owner) {
                if self.parameters.is_reference(id) && !kept.contains(&id) {
                    self.parameters.set_owner(id, None);
                }
            }
        }
    }

    /// Refuses uses of reference parameters (mitcad#231) the timeline gives
    /// no value: a reference is known only after its sketch, so only the
    /// features after it may use it, directly or through other parameters'
    /// expressions.
    pub(crate) fn check_reference_order(&self) -> Result<(), ModelError> {
        // Each reference: its sketch's position and name, and its dimension.
        let mut refs: HashMap<ParamId, (usize, &str, crate::sketch::ConstraintUid)> =
            HashMap::new();
        for (position, entry) in self.features.iter().enumerate() {
            if let FeatureDef::Sketch(def) = &entry.def {
                for d in &def.dimensions {
                    if let Some(id) = d.reference {
                        refs.insert(id, (position, entry.name.as_str(), d.id));
                    }
                }
            }
        }
        if refs.is_empty() {
            return Ok(());
        }
        let table = self.parameters.table();
        // The references each parameter's value depends on, itself included.
        let mut memo: HashMap<ParamId, BTreeSet<ParamId>> = HashMap::new();
        let mut uses = |id: ParamId| -> BTreeSet<ParamId> {
            if let Some(found) = memo.get(&id) {
                return found.clone();
            }
            let mut found = BTreeSet::new();
            let mut seen = BTreeSet::from([id]);
            let mut todo = vec![id];
            while let Some(p) = todo.pop() {
                if refs.contains_key(&p) {
                    found.insert(p);
                }
                for d in table.get(p).map(|p| p.dependencies()).unwrap_or_default() {
                    if seen.insert(*d) {
                        todo.push(*d);
                    }
                }
            }
            memo.insert(id, found.clone());
            found
        };
        for (position, entry) in self.features.iter().enumerate() {
            // What the feature uses: a sketch's own references are not uses.
            let mut used = entry.def.params();
            if let FeatureDef::Sketch(def) = &entry.def {
                for r in def.dimensions.iter().filter_map(|d| d.reference) {
                    if let Some(i) = used.iter().position(|p| *p == r) {
                        used.swap_remove(i);
                    }
                }
            }
            for id in used {
                for r in uses(id) {
                    let (at, sketch, dimension) = refs[&r];
                    if position <= at {
                        let through = if id == r {
                            String::new()
                        } else {
                            format!(" (through parameter '{}')", self.parameters.name(id))
                        };
                        let user = if position == at {
                            format!("{sketch} itself")
                        } else {
                            format!("{}, before {sketch} in the timeline,", entry.name)
                        };
                        return Err(invalid(format!(
                            "'{}' is what the driven dimension {dimension} of {sketch} measures: \
                             {user} cannot use it{through}; only features after {sketch} can",
                            self.parameters.name(r)
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Takes the values driven dimensions measured in a recompute into
    /// their reference parameters (mitcad#231).
    pub(crate) fn take_measured(&mut self, measured: &[(ParamId, f64)]) {
        for (id, value) in measured {
            let _ = self.parameters.set_measured(*id, *value);
        }
    }

    /// Takes what the features a recompute evaluated remember of the
    /// pieces of their splits (mitcad#408); features it did not evaluate
    /// (suppressed, rolled back, failed) keep theirs.
    pub(crate) fn take_split_pieces(&mut self, made: &crate::features::SplitPieces) {
        for (feature, groups) in made {
            if groups.is_empty() {
                self.split_pieces.remove(feature);
            } else if self.split_pieces.get(feature) != Some(groups) {
                self.split_pieces.insert(*feature, groups.clone());
            }
        }
    }

    /// Gives every sketch's driven dimensions their reference parameters,
    /// as a design file read without them needs (mitcad#231).
    pub(crate) fn settle_references(&mut self) -> Result<(), ModelError> {
        for i in 0..self.features.len() {
            let entry = &self.features[i];
            let FeatureDef::Sketch(def) = &entry.def else {
                continue;
            };
            let mut def = def.clone();
            let (uid, name) = (entry.uid, entry.name.clone());
            mitcad_sketch::internal::settle_references(
                &mut def,
                &mut self.parameters,
                uid,
                &name,
                &|_| None,
                None,
            )
            .map_err(invalid)?;
            let FeatureDef::Sketch(old) = &self.features[i].def else {
                continue;
            };
            if *old != def {
                let mut entry = (*self.features[i]).clone();
                entry.def = FeatureDef::Sketch(def);
                self.features[i] = Arc::new(entry);
            }
        }
        Ok(())
    }

    /// Refuses a unit for a parameter that a feature uses as another kind
    /// of value (an angle for a length).
    fn check_unit_change(&self, id: ParamId, unit: Unit) -> Result<(), ModelError> {
        for entry in &self.features {
            let mut wrong = None;
            // A derived part's values have its source's units (mitcad#801).
            let free = matches!(entry.def, FeatureDef::DerivedPart(_));
            let _ = entry.def.map_params(&mut |slot, p: &ParamId| {
                if free && slot.starts_with("parameters.") {
                    return Ok::<_, ()>(());
                }
                if *p == id && wrong.is_none() && !fits_slot(unit, slot_unit(slot)) {
                    wrong = Some(slot_unit(slot));
                }
                Ok::<_, ()>(())
            });
            if let Some(expected) = wrong {
                return Err(ParameterError::UnitInUse {
                    name: self.parameters.name(id),
                    unit,
                    user: entry.name.clone(),
                    expected,
                }
                .into());
            }
        }
        Ok(())
    }

    /// The features that use a parameter, by name.
    fn param_users(&self, id: ParamId) -> Vec<String> {
        let mut users: Vec<String> = self
            .features
            .iter()
            .filter(|f| f.def.params().contains(&id))
            .map(|f| f.name.clone())
            .collect();
        // The bindings that use it (mitcad#800).
        users.extend(self.binding_users(&self.parameters.name(id)));
        users
    }

    /// Adds a feature to the active component.
    pub(crate) fn add_feature(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
    ) -> Result<Added, ModelError> {
        self.add_feature_in(def, name, self.assembly.active)
    }

    /// Adds a feature to a component at the marker; a new-component
    /// operation also makes its component, placed in that one.
    pub fn add_feature_in(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
        component: ComponentUid,
    ) -> Result<Added, ModelError> {
        if !self.assembly.exists(component) {
            return Err(invalid(format!("component {component} does not exist")));
        }
        let uid = self.new_uid();
        let name = match name {
            Some(name) => {
                self.check_feature_name(name, None)?;
                name.to_owned()
            }
            None => self.default_name(def.base_name()),
        };
        let (def, parameters) = self.resolve(def, uid, &name, None)?;
        let entry = FeatureEntry {
            uid,
            name: name.clone(),
            suppressed: false,
            component,
            def,
        };
        self.check_feature(self.marker, &entry)
            .map_err(ModelError::Invalid)?;
        self.check_values(&entry.def)?;
        self.insert_resolved(entry)?;
        Ok(Added {
            uid,
            name,
            parameters,
        })
    }

    /// Keeps the component a feature makes in step with its definition
    /// after an edit: a new-component operation makes one, another
    /// operation removes it unless something was added to it.
    fn sync_made_component(&mut self, uid: FeatureUid) -> Result<(), ModelError> {
        let entry = self.entry(uid).expect("exists").clone();
        let made = self.assembly.created_by(uid);
        match (made, entry.def.info().new_component()) {
            (None, true) => {
                let name = self.made_component_name(&entry.def);
                let made = self.assembly.create(name.as_deref(), Some(uid));
                self.assembly
                    .place(made, entry.component, crate::transform::Transform::IDENTITY);
            }
            (Some(made), false) => {
                if self.features.iter().any(|f| f.component == made)
                    || self.assembly.children(made).next().is_some()
                {
                    return Err(invalid(format!(
                        "{} has features or occurrences of its own; delete them first, or keep \
                         the new-component operation",
                        self.assembly.name(made)
                    )));
                }
                if self.has_drawings(made) {
                    return Err(invalid(format!(
                        "{} has drawings in its Drawings folder; delete them first, or keep the \
                         new-component operation",
                        self.assembly.name(made)
                    )));
                }
                self.assembly.remove(made);
            }
            _ => {}
        }
        Ok(())
    }

    /// Removes features with the parameters they owned, their bodies'
    /// names and attributes and the components they made (when nothing
    /// else is in them). Returns the removed features in timeline order.
    pub fn remove_features(&mut self, removed: &BTreeSet<FeatureUid>) -> Vec<FeatureUid> {
        let order: Vec<FeatureUid> = self
            .features
            .iter()
            .map(|f| f.uid)
            .filter(|f| removed.contains(f))
            .collect();
        let before_marker = self.features[..self.marker]
            .iter()
            .filter(|f| removed.contains(&f.uid))
            .count();
        self.features.retain(|f| !removed.contains(&f.uid));
        self.assembly.occurrences.retain(|o| {
            o.pattern
                .as_ref()
                .is_none_or(|p| !removed.contains(&p.feature))
        });
        self.marker -= before_marker;
        for owner in &order {
            self.release_params(*owner);
        }
        self.body_names
            .retain(|body, _| !removed.contains(&body.feature));
        self.body_attributes
            .retain(|body, _| !removed.contains(&body.feature));
        self.feature_visibility
            .retain(|uid, _| !removed.contains(uid));
        self.import_notes.retain(|uid, _| !removed.contains(uid));
        self.release_components(removed);
        self.drop_orphan_drawings();
        order
    }

    /// After features were deleted: the components they made go too when
    /// nothing else is in them, else they stay as plain components.
    fn release_components(&mut self, removed: &BTreeSet<FeatureUid>) {
        let made: Vec<ComponentUid> = self
            .assembly
            .components
            .iter()
            .filter(|c| c.created_by.is_some_and(|f| removed.contains(&f)))
            .map(|c| c.uid)
            .collect();
        for c in made {
            // One with features, occurrences or drawings of its own stays
            // as a plain component.
            if self.features.iter().any(|f| f.component == c)
                || self.assembly.children(c).next().is_some()
                || self.has_drawings(c)
            {
                if let Some(def) = self.assembly.component_mut(c) {
                    def.created_by = None;
                }
            } else {
                self.assembly.remove(c);
            }
        }
    }

    pub fn edit_feature(
        &mut self,
        uid: FeatureUid,
        def: &FeatureDef<ValueInput>,
    ) -> Result<Vec<String>, ModelError> {
        let position = self.require(uid)?;
        let old = self.features[position].clone();
        if old.def.type_name() != def.type_name() {
            return Err(invalid(format!(
                "cannot change {} ({}) into a {}",
                old.name,
                old.def.type_name(),
                def.type_name()
            )));
        }
        let (def, created) = self.resolve(def, uid, &old.name, Some(&old.def))?;
        let entry = FeatureEntry {
            def,
            ..(*old).clone()
        };
        self.check_feature(position, &entry)
            .map_err(ModelError::Invalid)?;
        self.check_values(&entry.def)?;
        self.features[position] = Arc::new(entry);
        self.sync_point_pattern(uid)?;
        // The features after it may refer to what the edit removed.
        for later in position + 1..self.features.len() {
            let entry = self.features[later].clone();
            self.check_feature(later, &entry)
                .map_err(|e| invalid(format!("{} would no longer work: {e}", entry.name)))?;
        }
        self.sync_made_component(uid)?;
        self.release_params(uid);
        Ok(created)
    }
}

/// Sketches and construction geometry are in their component's
/// coordinates, so a feature uses only those of its own component (F6);
/// a joint's origins use those of the components they are on.
pub(crate) fn check_components(
    assembly: &Assembly,
    features: &[Arc<FeatureEntry>],
    index: &crate::feature_index::FeatureIndex,
    entry: &FeatureEntry,
) -> Result<(), String> {
    if !assembly.exists(entry.component) {
        return Err(format!("component {} does not exist", entry.component));
    }
    if let FeatureDef::PatternAtPoints(def) = &entry.def {
        return def.check_components(assembly, features, entry);
    }
    // A move's points and axes may be picked in occurrences (mitcad#699).
    if let Some(spec) = entry.def.move_spec() {
        return spec.check_components(assembly, features, entry);
    }
    // Joints use geometry of the components their occurrences place
    // (mitcad#55).
    if let Some(result) = crate::joints::check_components(assembly, features, entry) {
        return result;
    }
    // A sketch's plane and projections may be geometry of the component a
    // link names (mitcad#100); a path that leads nowhere now is left to
    // recompute, which reports it.
    let linked = match &entry.def {
        FeatureDef::Sketch(sketch) => sketch.linked_references(),
        _ => Vec::new(),
    };
    for uid in entry.def.info().references().features {
        let Some(other) = index.find(features, uid) else {
            continue;
        };
        let placed = crate::joints::placed_geometry(&other.def);
        if let Some((_, link)) = linked.iter().find(|(u, _)| *u == uid) {
            if let Ok(component) = link.source_component(assembly)
                && placed
                && other.component != component
            {
                return Err(format!(
                    "{} is in {}, not in {}, where the link {} leads",
                    other.name,
                    assembly.name(other.component),
                    assembly.name(component),
                    link.source
                ));
            }
            continue;
        }
        if placed && other.component != entry.component {
            return Err(format!(
                "{} is in {}; a feature of {} cannot use it",
                other.name,
                assembly.name(other.component),
                assembly.name(entry.component)
            ));
        }
    }
    Ok(())
}

/// Every parameter the features use.
pub(crate) fn used_params(features: &[Arc<FeatureEntry>]) -> BTreeSet<ParamId> {
    features.iter().flat_map(|f| f.def.params()).collect()
}

/// The parameters an edit renamed, as (old name, new name): the same id
/// under a new name, where the old name is gone and the new one was free.
fn renamed_parameters(before: &Parameters, after: &Parameters) -> Vec<(String, String)> {
    after
        .table()
        .iter()
        .filter_map(|(id, p)| {
            let old = before.get(id)?.name();
            (old != p.name() && after.find(old).is_none() && before.find(p.name()).is_none())
                .then(|| (old.to_owned(), p.name().to_owned()))
        })
        .collect()
}

/// A feature in the timeline with its status after the last recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineItem<'a> {
    pub entry: &'a FeatureEntry,
    /// None until the document has been recomputed.
    pub status: Option<&'a FeatureStatus>,
    /// What a feature that succeeded built with caveats (P9).
    pub warnings: &'a [String],
    /// What the importer that made it said (mitcad#170).
    pub import_note: Option<&'a str>,
}

impl TimelineItem<'_> {
    /// `ok`, `warning` (succeeded with warnings or with an import note),
    /// `error`, `suppressed`, `rolled_back`, or `pending` before the first
    /// recompute.
    pub fn status_text(&self) -> &'static str {
        import_notes::status_text(self.status, self.warnings, self.import_note)
    }

    /// The error of a failed feature, or its import note and warnings.
    pub fn message(&self) -> Option<String> {
        import_notes::status_message(self.status, self.warnings, self.import_note)
    }
}

/// A feature's status as the queries give it: `warning` for one that
/// succeeded with warnings.
pub fn status_text(status: Option<&FeatureStatus>, warnings: &[String]) -> &'static str {
    match status {
        Some(FeatureStatus::Ok) if !warnings.is_empty() => "warning",
        Some(status) => status.as_str(),
        None => "pending",
    }
}

/// A failed feature's error, or a feature's warnings joined.
pub fn status_message(status: Option<&FeatureStatus>, warnings: &[String]) -> Option<String> {
    match status.and_then(FeatureStatus::error) {
        Some(error) => Some(error.to_owned()),
        None if !warnings.is_empty() => Some(warnings.join("; ")),
        None => None,
    }
}

/// A body at the timeline marker.
#[derive(Debug)]
pub struct BodyView<'a, S> {
    pub uid: BodyUid,
    pub name: String,
    /// The shape in its component's coordinates (F6: see
    /// [`Document::body_component`] and [`Document::instances`]).
    pub shape: &'a S,
}

/// A profile region of an evaluated sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileView {
    pub sketch: FeatureUid,
    pub region: RegionKey,
    /// An extrude uses it (the profile is then hidden).
    pub consumed: bool,
}

/// The first failed feature of a recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureError {
    pub uid: FeatureUid,
    pub name: String,
    pub message: String,
}

impl fmt::Display for FeatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}

/// What the last recompute did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecomputeStats {
    /// Features evaluated (not taken from the cache), in timeline order.
    pub evaluated: Vec<FeatureUid>,
    /// The first feature that failed, in timeline order.
    pub error: Option<FeatureError>,
    /// How long each of `evaluated` took.
    pub times: Vec<Duration>,
    /// Features whose results came from the result store (P7d), in
    /// timeline order, and how long reading each took.
    pub restored: Vec<FeatureUid>,
    pub restore_times: Vec<Duration>,
}

/// The outcome of a previewed command.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewReport {
    pub sheet_states: serde_json::Value,
    pub uid: FeatureUid,
    pub name: String,
    pub status: FeatureStatus,
    /// What the feature built with caveats (P9).
    pub warnings: Vec<String>,
    /// A pattern's elements by number, suppressed ones too (P9).
    pub elements: Vec<crate::transform::Transform>,
    pub anchor: Option<crate::features::point_pattern::PatternAnchor>,
    /// Where a sweep's profile lies on its path (mitcad#795).
    pub sweep_place: Option<crate::features::sweep::SweepPlace>,
    /// What a move's motion was worked out from: a Rotate's axis, the
    /// points of Point to Point and Point to Position (mitcad#838).
    pub move_basis: Option<crate::features::moves::MotionBasis>,
    /// New shared-body occurrence instances, in world coordinates.
    pub instances: Vec<crate::assembly::Instance>,
    pub removed_instances: Vec<(Vec<OccurrenceUid>, BodyUid)>,
    pub cut_targets: Option<crate::features::extrude_scope::CutTargets>,
    /// Bodies at the marker after the command: id, name and whether the
    /// command changed or created it.
    pub bodies: Vec<(BodyUid, String, bool)>,
    /// Bodies the command removed.
    pub removed: Vec<BodyUid>,
    pub tool: bool,
    /// Occurrences the command places elsewhere in the design (joints,
    /// moves of occurrences; mitcad#55): each path from the root with its
    /// new placement in the design. An occurrence inside a moved one is
    /// listed too.
    pub placements: Vec<(Vec<OccurrenceUid>, crate::transform::Transform)>,
    /// Where the tool body (`tool`) is in the design (mitcad#636): the
    /// tool is in its feature's component's coordinates, so one entry per
    /// occurrence of that component (the root: one, unplaced), each path
    /// from the root, its placement in the design and whether every
    /// occurrence on the path is shown. Empty without a tool.
    pub tool_instances: Vec<(Vec<OccurrenceUid>, crate::transform::Transform, bool)>,
}

struct Preview<S> {
    uid: FeatureUid,
    state: DocState,
    result: Recomputed<S>,
}

/// The versions of each feature's result and of each body (P7d tests).
#[cfg(any(test, feature = "testing"))]
pub type ResultVersions = (Vec<(FeatureUid, Option<u128>)>, Vec<(BodyUid, u128)>);

/// An undo or redo step: the command's label and the state on the other
/// side of it, with that state's revision, and the library files the
/// command wrote (mitcad#655), which undo and redo write back.
#[derive(Clone)]
struct Step {
    label: String,
    state: DocState,
    revision: u64,
    files: Vec<library_items::FileWrite>,
}

/// Definition/history retained while an asynchronous frontend command is
/// being prepared. Geometry stays in the shared recompute cache rather
/// than being copied into the checkpoint.
struct CommandCheckpoint {
    token: String,
    state: DocState,
    revision: u64,
    saved_revision: Option<u64>,
    undo: Vec<Step>,
    redo: Vec<Step>,
}

/// Parametric part document: definition state, history and results.
pub struct Document<K: Kernel> {
    kernel: K,
    state: DocState,
    /// The revision of `state`: no other state of this document had it.
    revision: u64,
    /// The revision the next change gets.
    next_revision: u64,
    /// The revision last marked saved; None while the document matches no
    /// saved file.
    saved_revision: Option<u64>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    command_checkpoint: Option<CommandCheckpoint>,
    next_command_token: u64,
    cache: Cache<K::Shape>,
    result: Recomputed<K::Shape>,
    /// Recomputes so far, so callers can tell whether a command recomputed;
    /// a cancelled one is not counted.
    recomputes: u64,
    preview: Option<Preview<K::Shape>>,
    /// What loading the design file changed, such as renamed parameters.
    warnings: Vec<String>,
    /// Where recomputes report progress and find cancel requests (P7).
    monitor: Option<Arc<RecomputeMonitor>>,
    /// Reads library components (mitcad#64); the process's when None.
    resolver: Option<Arc<dyn crate::library::LinkResolver>>,
    /// The last loaded or saved design's folder. Runtime context only:
    /// copied appearances resolve relative image paths against it.
    source_directory: RwLock<Option<PathBuf>>,
    /// Flat Pattern evaluations (mitcad#208), separate from modelling results.
    flat_cache: flat_patterns::FlatCache<K::Shape>,
    /// Print Analysis meshes and results (mitcad#620), separate from
    /// modelling results.
    print_cache: crate::print_analysis::PrintCache,
    /// Runtime caches separate from modelling results, one per type
    /// ([`Document::side_cache`]): the drawing views projected before
    /// (mitcad#707), the annotations' picks measured before (mitcad#716).
    side_caches: side_caches::SideCaches,
    /// The components an import filled from other documents, by the
    /// document's name for them, with their sub-components
    /// ([`Document::set_import_scopes`]). Runtime context only.
    import_scopes: Vec<(String, BTreeSet<ComponentUid>)>,
    /// The components each entity of another document that a design names
    /// can be in, by the id of the reference ([`Document::set_import_focus`]).
    /// Runtime context only.
    import_focus: std::collections::HashMap<u64, Vec<ComponentUid>>,
    /// Changes are checked against the design policy (mitcad#220): on
    /// while a JSON command runs ([`Document::with_policy_checks`]).
    policy_checks: bool,
    /// Policy checks are off even in commands (an importer replaying a
    /// file's design, [`Document::without_policy_checks`]).
    policy_suspended: bool,
    /// The component BUILD places parts in while a timber frame drives the
    /// fasteners of its own component (mitcad#522). Runtime context only.
    build_host_override: Option<ComponentUid>,
    /// The last answers of the `sketch` query with what they were made
    /// from (mitcad#679), the newest first. Runtime context only.
    sketch_answers: std::sync::Mutex<Vec<SketchAnswer>>,
    /// The options the cascade of configuration rules moved in the command
    /// that made the revision kept with them (mitcad#799,
    /// [`Document::configuration_moves`]). Runtime context only.
    configuration_moves: (u64, Vec<crate::configurations::ConfigurationMove>),
}

/// A `sketch` query's answer with what it was made from (mitcad#679): the
/// sketch's definition (held, so that a changed one is another entry, see
/// [`Document::known_sketch_answer`]), its last evaluation's status and
/// version, its warnings and the parameters (names, expressions, values,
/// limits).
pub(crate) struct SketchAnswer {
    uid: FeatureUid,
    entry: Arc<FeatureEntry>,
    result: Option<(FeatureStatus, Option<crate::recompute::Version>)>,
    warnings: Vec<String>,
    params: Parameters,
    answer: String,
}

/// How many sketches' answers a document keeps.
const SKETCH_ANSWERS: usize = 4;

impl<K: Kernel> Document<K> {
    /// An empty document; it is not modified (there is nothing to save).
    pub fn new(kernel: K) -> Self {
        let mut document = Self::from_state(kernel, DocState::default());
        document.mark_saved();
        document
    }

    /// A document of a loaded definition; nothing is evaluated until
    /// [`Document::recompute`]. It is modified until
    /// [`Document::mark_saved`]: the definition need not come from the file
    /// it will be saved to (an imported design, recovered work).
    pub fn from_state(kernel: K, state: DocState) -> Self {
        Self {
            kernel,
            state,
            revision: 0,
            next_revision: 1,
            saved_revision: None,
            undo: Vec::new(),
            redo: Vec::new(),
            command_checkpoint: None,
            next_command_token: 1,
            cache: Cache::default(),
            result: Recomputed::default(),
            recomputes: 0,
            preview: None,
            warnings: Vec::new(),
            monitor: None,
            resolver: None,
            source_directory: RwLock::new(None),
            flat_cache: Default::default(),
            print_cache: Default::default(),
            side_caches: Default::default(),
            import_scopes: Vec::new(),
            import_focus: Default::default(),
            policy_checks: false,
            policy_suspended: false,
            build_host_override: None,
            sketch_answers: Default::default(),
            configuration_moves: (0, Vec::new()),
        }
    }

    /// The component BUILD places parts in instead of the active one's
    /// host while a timber frame drives its own fasteners (mitcad#522).
    pub fn build_host_override(&self) -> Option<ComponentUid> {
        self.build_host_override
    }

    /// Sets (or with None ends) the BUILD host override; returns the one
    /// before, to restore.
    pub fn set_build_host_override(&mut self, host: Option<ComponentUid>) -> Option<ComponentUid> {
        std::mem::replace(&mut self.build_host_override, host)
    }

    pub(crate) fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }

    pub(crate) fn source_directory(&self) -> Option<PathBuf> {
        self.source_directory
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) fn set_source_directory(&self, directory: &Path) {
        *self
            .source_directory
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(directory.to_path_buf());
    }

    /// What loading the design file changed (renamed parameters, ...).
    pub fn load_warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn kernel(&self) -> &K {
        &self.kernel
    }

    pub fn state(&self) -> &DocState {
        &self.state
    }

    pub fn parameters(&self) -> &Parameters {
        &self.state.parameters
    }

    /// Features in timeline order.
    pub fn features(&self) -> impl Iterator<Item = &FeatureEntry> {
        self.state.features.iter().map(|f| &**f)
    }

    pub fn feature(&self, uid: FeatureUid) -> Option<&FeatureEntry> {
        self.state.entry(uid)
    }

    /// The number of features before the timeline marker.
    pub fn marker(&self) -> usize {
        self.state.marker
    }

    // Recompute and results.

    /// Evaluates the timeline, reusing cached results. A cancelled
    /// recompute (see [`Document::try_recompute`]) keeps the last results.
    pub fn recompute(&mut self) -> RecomputeStats {
        self.try_recompute().unwrap_or_else(|_| self.stats())
    }

    /// Evaluates the timeline; fails only when the monitor cancels it, and
    /// then the last results stay.
    pub fn try_recompute(&mut self) -> Result<RecomputeStats, ModelError> {
        let monitor = self.monitor.as_ref();
        let result = recompute(&self.kernel, &self.state, &mut self.cache, monitor)?;
        self.state.take_measured(&result.measured);
        self.set_result(result);
        Ok(self.stats())
    }

    /// Evaluates a definition state with the document's cache, reporting
    /// to its monitor; nothing of the document but the cache changes.
    fn compute(&mut self, state: &DocState) -> Result<Recomputed<K::Shape>, ModelError> {
        let monitor = self.monitor.as_ref();
        Ok(recompute(&self.kernel, state, &mut self.cache, monitor)?)
    }

    /// Like [`Document::compute`] for a state a command made from the
    /// document's: the features at the start of its timeline that the
    /// document's last results evaluated, and that nothing changed for,
    /// are taken from those results instead of being gone through again
    /// (mitcad#841: an import appends its features in batches to a
    /// timeline of thousands, `recompute::recompute_after`). The result is
    /// the same.
    fn compute_after(&mut self, state: &DocState) -> Result<Recomputed<K::Shape>, ModelError> {
        let monitor = self.monitor.as_ref();
        let before = Some(&self.result);
        Ok(recompute_after(
            &self.kernel,
            state,
            &mut self.cache,
            monitor,
            before,
        )?)
    }

    /// Takes the results of a recompute of the state.
    fn set_result(&mut self, result: Recomputed<K::Shape>) {
        // The pieces of splits as this recompute made them (mitcad#408).
        self.state.take_split_pieces(&result.split_pieces);
        self.result = result;
        self.recomputes += 1;
        // Results only the last ones held may go now (a memory budget).
        self.cache.trim();
    }

    /// Recomputes from now on report their progress to `monitor` and stop
    /// when it is cancelled (P7); None detaches it. A cancelled
    /// computation rejects the command, undo, redo or preview that ran it,
    /// which then changes nothing but the cache (see the module comment).
    pub fn set_monitor(&mut self, monitor: Option<Arc<RecomputeMonitor>>) {
        self.monitor = monitor;
    }

    pub fn monitor(&self) -> Option<&Arc<RecomputeMonitor>> {
        self.monitor.as_ref()
    }

    /// Reads this document's library components with `resolver` instead
    /// of the process's ([`crate::library::set_link_resolver`]).
    pub fn set_link_resolver(&mut self, resolver: Option<Arc<dyn crate::library::LinkResolver>>) {
        self.resolver = resolver;
    }

    /// The resolver of library components: the document's, else the
    /// process's.
    pub fn link_resolver(&self) -> Option<Arc<dyn crate::library::LinkResolver>> {
        self.resolver.clone().or_else(crate::library::link_resolver)
    }

    /// The number of recomputes so far.
    pub fn recompute_count(&self) -> u64 {
        self.recomputes
    }

    /// The versions of the last recompute's results and bodies (P7d tests).
    #[cfg(any(test, feature = "testing"))]
    pub fn versions(&self) -> ResultVersions {
        let outputs = self
            .result
            .results
            .iter()
            .map(|r| (r.uid, r.output.as_ref().map(|o| o.version)))
            .collect();
        let bodies = self
            .result
            .bodies
            .iter()
            .map(|(uid, body)| (*uid, body.version))
            .collect();
        (outputs, bodies)
    }

    /// What the last recompute did.
    pub fn stats(&self) -> RecomputeStats {
        let error = self.result.results.iter().find_map(|r| {
            r.status.error().map(|message| FeatureError {
                uid: r.uid,
                name: self
                    .state
                    .entry(r.uid)
                    .map_or_else(|| r.uid.to_string(), |f| f.name.clone()),
                message: message.to_owned(),
            })
        });
        RecomputeStats {
            evaluated: self.result.evaluated.clone(),
            error,
            times: self.result.times.clone(),
            restored: self.result.restored.clone(),
            restore_times: self.result.restore_times.clone(),
        }
    }

    // The result store (P7d).

    /// Recomputes from now on take results from the store when the memory
    /// cache has none, and [`Document::persist_results`] writes there;
    /// None: no store (the default).
    pub fn set_result_store(&mut self, store: Option<ResultStore>) {
        self.cache.store = store;
    }

    pub fn result_store(&self) -> Option<&ResultStore> {
        self.cache.store.as_ref()
    }

    /// The most memory the cached results may take, estimated in bytes;
    /// None: no limit (the default). Beyond it the results used longest ago
    /// go, except those of the document's results and its preview; they
    /// can still come from the result store.
    pub fn set_memory_budget(&mut self, bytes: Option<u64>) {
        self.cache.set_budget(bytes);
    }

    pub fn memory_budget(&self) -> Option<u64> {
        self.cache.budget()
    }

    /// Recomputes from now on evaluate the features of independent
    /// components ahead on up to `workers` threads, each with a fork of
    /// the kernel, and take their results in timeline order (mitcad#842,
    /// `recompute/ahead.rs`): the results, the cache and the progress are
    /// those of one thread. 1 evaluates every feature on the recompute's
    /// own thread, as before; 0 counts as 1. The default is 1, or the
    /// environment variable `MITCAD_RECOMPUTE_WORKERS`.
    pub fn set_recompute_workers(&mut self, workers: usize) {
        self.cache.workers = workers.max(1);
    }

    pub fn recompute_workers(&self) -> usize {
        self.cache.workers
    }

    /// The next recompute that goes through the timeline from its start
    /// (opening a design) puts each shown body into `feed` as soon as no
    /// later feature can change it, and closes the feed when it ends
    /// (mitcad#881, `recompute/final_bodies.rs`): another thread meshes
    /// them for display meanwhile. A recompute that goes on after earlier
    /// results, or a preview's, closes it at once; a feed given before
    /// replaces nothing but is closed.
    pub fn feed_final_bodies(&mut self, feed: FinalBodies<K::Shape>) {
        if let Some(earlier) = self.cache.final_bodies.replace(feed) {
            earlier.close();
        }
    }

    /// The memory the cached results take, estimated in bytes.
    pub fn memory_cache_bytes(&self) -> u64 {
        self.cache.bytes()
    }

    /// Drops the cached results that neither the document's results nor
    /// its preview hold; returns their number and bytes.
    pub fn clear_memory_cache(&mut self) -> (u64, u64) {
        self.cache.clear()
    }

    // Definitions evaluated in parallel (the .f3d import, mitcad#95).

    /// A copy of the document for another thread, which evaluates features
    /// on the same results at the same time ([`Kernel::fork`]; None when
    /// the kernel has no fork): the definition, the cached results and the
    /// last recompute, shared, without undo history, result store or
    /// monitor. Features added to the copy get the uids they would get
    /// here, and their results the same versions, so
    /// [`Document::adopt_results`] can take them over.
    pub fn fork(&self) -> Option<Document<K>> {
        let kernel = self.kernel.fork()?;
        Some(Self {
            kernel,
            state: self.state.clone(),
            revision: self.revision,
            next_revision: self.next_revision,
            saved_revision: self.saved_revision,
            undo: Vec::new(),
            redo: Vec::new(),
            command_checkpoint: None,
            next_command_token: 1,
            cache: self.cache.fork(),
            result: self.result.fork(),
            recomputes: self.recomputes,
            preview: None,
            warnings: Vec::new(),
            monitor: None,
            resolver: self.resolver.clone(),
            source_directory: RwLock::new(self.source_directory()),
            flat_cache: Default::default(),
            print_cache: Default::default(),
            side_caches: Default::default(),
            import_scopes: self.import_scopes.clone(),
            import_focus: self.import_focus.clone(),
            policy_checks: false,
            policy_suspended: self.policy_suspended,
            build_host_override: None,
            sketch_answers: Default::default(),
            configuration_moves: (0, Vec::new()),
        })
    }

    /// Takes over the cached results of the features `uids` that a fork of
    /// this document evaluated ([`Document::fork`]): adding the same
    /// definitions here then reuses them instead of evaluating them again.
    /// Returns how many were taken.
    pub fn adopt_results(&mut self, fork: &Document<K>, uids: &[FeatureUid]) -> usize {
        self.cache.adopt(&fork.cache, uids)
    }

    /// The cache and the last recompute, for diagnostics (`model-api/src/cache.rs`).
    pub fn cache(&self) -> &Cache<K::Shape> {
        &self.cache
    }

    /// Runs `apply` with base features' B-rep data known as written from
    /// the shapes beside it, which their evaluations then adopt
    /// (mitcad#244, [`crate::kernel::Kernel::adopt_shape`]); forgotten
    /// after it, so later evaluations read the data.
    pub(crate) fn with_written_from<T>(
        &mut self,
        written_from: Vec<(crate::features::Brep, K::Shape)>,
        apply: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.cache.written_from = written_from;
        let result = apply(self);
        self.cache.written_from = Vec::new();
        result
    }

    pub fn last_result(&self) -> &Recomputed<K::Shape> {
        &self.result
    }

    /// The Print Analysis meshes and results kept (mitcad#620).
    pub fn print_cache(&self) -> &crate::print_analysis::PrintCache {
        &self.print_cache
    }

    /// The document's runtime cache of type `T`, made empty on first use
    /// (mitcad#777). Side caches change no state and are not kept by a
    /// fork; a layer above the model keeps its own caches here, such as
    /// the annotations' measured picks (mitcad#716).
    pub fn side_cache<T: std::any::Any + Send + Sync + Default>(&self) -> Arc<T> {
        self.side_caches.get()
    }

    /// Writes to the result store the results of the last recompute that
    /// are not there yet: the features that succeeded, whose evaluation
    /// took at least `min_time`, or less with shapes small for its time
    /// (quick results, mitcad#882), and whose results the store takes (see
    /// `store.rs`). What a preview computed is not written.
    pub fn persist_results(&mut self, min_time: Duration) -> PersistReport {
        match self.cache.store.as_ref() {
            Some(store) => self.write_results(store, Some(min_time)),
            None => PersistReport::default(),
        }
    }

    /// Writes every result of the last recompute that a store takes to
    /// `store`, however quick its evaluation was, and leaves them marked
    /// as not written to the document's own store: the import process
    /// hands the import's evaluation over to the application this way
    /// (mitcad#413), which reads the imported design with that store and
    /// so restores the results instead of evaluating every feature again.
    /// What a preview computed is not written.
    pub fn hand_off_results(&self, store: &ResultStore) -> PersistReport {
        self.write_results(store, None)
    }

    /// Writes the results of the last recompute that succeeded and that
    /// the store takes: with `min_time`, those not written yet whose
    /// evaluation took at least that long or that are worth storing
    /// although quicker ([`ResultStore::worth_storing`]), marked written;
    /// without, all.
    fn write_results(&self, store: &ResultStore, min_time: Option<Duration>) -> PersistReport {
        let start = Instant::now();
        let mut report = PersistReport::default();
        let params = &self.state.parameters;
        // (By uid at once: a search per result was quadratic.)
        let entries: HashMap<FeatureUid, &FeatureEntry> =
            self.state.features.iter().map(|f| (f.uid, &**f)).collect();
        for result in &self.result.results {
            let Some(output) = &result.output else {
                continue;
            };
            if output.result.is_err() || (min_time.is_some() && output.persisted.get()) {
                continue;
            }
            let Some(entry) = entries.get(&result.uid).copied() else {
                continue;
            };
            let stored_form =
                output.sketch.is_none() && output.datum.is_none() && output.placements.is_empty();
            // A parameter renamed since the evaluation: its version holds
            // the old name, and the file would be found by neither.
            if !stored_form
                || !store::storable(&entry.def)
                || def_fingerprint(&entry.def, params) != output.def_fp
            {
                continue;
            }
            // A quick result, when restoring it is cheaper than evaluating
            // it again (mitcad#882).
            let quick = min_time.is_some_and(|min| output.time < min);
            if quick && !store.worth_storing(output.time, store::shape_memory(&self.kernel, output))
            {
                continue;
            }
            match store.write(&self.kernel, entry, output, params) {
                Ok((bytes, shapes)) => {
                    if min_time.is_some() {
                        output.persisted.set(true);
                    }
                    report.results += 1;
                    report.quick += usize::from(quick);
                    report.shapes += shapes;
                    report.bytes += bytes;
                }
                Err(error) => {
                    report.failed += 1;
                    report.errors.push(error);
                }
            }
        }
        report.time = start.elapsed();
        report
    }

    pub fn status(&self, uid: FeatureUid) -> Option<&FeatureStatus> {
        self.feature_result(uid).map(|r| &r.status)
    }

    /// The last recompute's result of a feature. The results are in
    /// timeline order, one per feature, so it is found where the feature is
    /// in the timeline (mitcad#841: an importer asks for the status of
    /// every feature it adds), else searched for.
    pub(crate) fn feature_result(
        &self,
        uid: FeatureUid,
    ) -> Option<&crate::recompute::FeatureResult<K::Shape>> {
        if let Some(i) = self.state.position(uid)
            && let Some(result) = self.result.results.get(i)
            && result.uid == uid
        {
            return Some(result);
        }
        self.result.result(uid)
    }

    /// The warnings of a feature that succeeded in the last recompute (P9:
    /// a fillet built 0.1 % smaller than asked).
    pub fn warnings(&self, uid: FeatureUid) -> &[String] {
        self.feature_result(uid).map_or(&[], |r| r.warnings())
    }

    pub fn timeline(&self) -> Vec<TimelineItem<'_>> {
        self.features()
            .map(|entry| TimelineItem {
                entry,
                status: self.status(entry.uid),
                warnings: self.warnings(entry.uid),
                import_note: self.import_note(entry.uid),
            })
            .collect()
    }

    /// Bodies at the timeline marker, by id.
    /// Bodies at the timeline marker, by id, of every component (each in
    /// its component's coordinates; [`Document::instances`] places them).
    pub fn bodies(&self) -> Vec<BodyView<'_, K::Shape>> {
        self.bodies_where(|component| {
            !self
                .import_scopes
                .iter()
                .any(|(_, components)| components.contains(&component))
        })
    }

    /// The bodies of the components `keep` takes, with the components they
    /// are in.
    fn bodies_where(&self, keep: impl Fn(ComponentUid) -> bool) -> Vec<BodyView<'_, K::Shape>> {
        let names = display_names(&self.state, &self.result);
        self.result
            .bodies
            .iter()
            .filter(|(uid, _)| {
                self.result
                    .owners
                    .get(uid)
                    .is_none_or(|component| keep(*component))
            })
            .map(|(uid, shape)| BodyView {
                uid: *uid,
                name: names.get(uid).cloned().unwrap_or_else(|| uid.to_string()),
                shape: &shape.value,
            })
            .collect()
    }

    /// Sets the components that stand for other documents while an import
    /// replays a design: `scopes` are each document's name for them (the
    /// `.f3d` import: its GUID) and the components it filled from it.
    /// Their sub-components belong to them. While set, [`Document::bodies`]
    /// leaves out the bodies of the components of all scopes (the import
    /// matches the design's own history, which holds none of them), and
    /// [`Document::scope_bodies`] gives those of one. An empty list clears
    /// them. Runtime context of an import (mitcad#196): not part of the
    /// design, not undone.
    pub fn set_import_scopes(&mut self, scopes: Vec<(String, Vec<ComponentUid>)>) {
        let assembly = &self.state.assembly;
        self.import_scopes = scopes
            .into_iter()
            .map(|(name, roots)| {
                let mut all: BTreeSet<ComponentUid> = BTreeSet::new();
                let mut open = roots;
                while let Some(c) = open.pop() {
                    if all.insert(c) {
                        open.extend(assembly.children(c).map(|o| o.component));
                    }
                }
                (name, all)
            })
            .collect();
    }

    /// Sets the components each entity of another document can be in, by
    /// the id of the reference that names it (the `.f3d` import: the
    /// occurrences the reference's context transform says, an occurrence
    /// and those inside it that sit at the same placement, mitcad#196), for
    /// the lookups that find the entities among [`Document::scope_bodies`]:
    /// a document placed several times has a copy per arrangement and the
    /// entity is in one of them. An empty map clears it. Runtime context of
    /// an import: not part of the design, not undone.
    pub fn set_import_focus(&mut self, focus: std::collections::HashMap<u64, Vec<ComponentUid>>) {
        self.import_focus = focus;
    }

    /// The components an entity of another document can be in, when the
    /// import knows them ([`Document::set_import_focus`]).
    pub fn import_focus(&self, reference: u64) -> Option<&[ComponentUid]> {
        self.import_focus.get(&reference).map(Vec::as_slice)
    }

    /// Whether a component belongs to an import scope
    /// ([`Document::set_import_scopes`]): a component filled from another
    /// document or one inside it.
    pub fn in_import_scope(&self, component: ComponentUid) -> bool {
        self.import_scopes
            .iter()
            .any(|(_, components)| components.contains(&component))
    }

    /// The components of an import scope ([`Document::set_import_scopes`]).
    pub fn import_scope(&self, name: &str) -> Vec<ComponentUid> {
        self.import_scopes
            .iter()
            .filter(|(n, _)| n == name)
            .flat_map(|(_, c)| c.iter().copied())
            .collect()
    }

    /// The bodies of the components of one import scope
    /// ([`Document::set_import_scopes`]).
    pub fn scope_bodies(&self, name: &str) -> Vec<BodyView<'_, K::Shape>> {
        let scope: BTreeSet<ComponentUid> = self.import_scope(name).into_iter().collect();
        self.bodies_where(|component| scope.contains(&component))
    }

    /// The bodies of some components with their names (an import's lookups
    /// of the entities in a component of another document,
    /// [`Document::set_import_focus`]).
    pub fn bodies_of_components(&self, components: &[ComponentUid]) -> Vec<BodyView<'_, K::Shape>> {
        self.bodies_where(|c| components.contains(&c))
    }

    /// The component a body at the marker belongs to.
    pub fn body_component(&self, uid: BodyUid) -> Option<ComponentUid> {
        self.result.owners.get(&uid).copied()
    }

    pub fn body_shape(&self, uid: BodyUid) -> Option<&K::Shape> {
        self.result.bodies.get(&uid).map(|b| &b.value)
    }

    /// The identity of a body's shape at the marker (its version, P7d):
    /// the same as long as the shape is, e.g. to keep what was measured
    /// on it.
    pub fn body_version(&self, uid: BodyUid) -> Option<u128> {
        self.result.bodies.get(&uid).map(|b| b.version)
    }

    /// The display name of a body (stored, or the default it would get).
    pub fn body_name(&self, uid: BodyUid) -> String {
        display_names(&self.state, &self.result)
            .remove(&uid)
            .unwrap_or_else(|| uid.to_string())
    }

    /// Profile regions of the evaluated sketches before the marker.
    pub fn profiles(&self) -> Vec<ProfileView> {
        let consumed: BTreeSet<(FeatureUid, &RegionKey)> = self
            .state
            .features
            .iter()
            .flat_map(|f| {
                f.def
                    .info()
                    .profiles()
                    .iter()
                    .map(|p| (p.sketch, &p.region))
            })
            .collect();
        let mut profiles = Vec::new();
        for entry in &self.state.features {
            let Some(output) = self.result.sketches.get(&entry.uid) else {
                continue;
            };
            for region in &output.value.regions {
                profiles.push(ProfileView {
                    sketch: entry.uid,
                    region: region.key.clone(),
                    consumed: consumed.contains(&(entry.uid, &region.key)),
                });
            }
        }
        profiles
    }

    /// An evaluated profile region with its sketch frame: the one a
    /// reference with the key means, under its current key (the best match
    /// when the key changed with the sketch, as features find it; see
    /// `SketchOutput::resolved`), so that the application highlights what a
    /// feature extrudes (mitcad#199).
    pub fn profile_region(
        &self,
        sketch: FeatureUid,
        region: &RegionKey,
    ) -> Option<(SketchFrame, &ProfileRegion)> {
        let output = &self.result.sketches.get(&sketch)?.value;
        Some((output.frame, output.resolved(region)?))
    }

    /// All regions a profile reference means now ([`profile_region`]'s
    /// main one first, then the other pieces of a region a sketch change
    /// split, mitcad#407), with the sketch frame and whether the key is
    /// the region's own: what the application highlights for the
    /// reference.
    ///
    /// [`profile_region`]: Self::profile_region
    pub fn profile_pieces(
        &self,
        sketch: FeatureUid,
        region: &RegionKey,
    ) -> Option<(SketchFrame, Vec<&ProfileRegion>, bool)> {
        let output = &self.result.sketches.get(&sketch)?.value;
        let found = output.found(region)?;
        Some((
            output.frame,
            found.regions.iter().map(|r| &r.profile).collect(),
            found.exact,
        ))
    }

    /// The output of an evaluated sketch: frame, solved geometry, status
    /// and regions (None when it failed or is not evaluated).
    pub fn sketch_output(&self, sketch: FeatureUid) -> Option<&crate::features::SketchOutput> {
        self.result.sketches.get(&sketch).map(|o| &*o.value)
    }

    /// A sketch's definition as it last evaluated, when that differs from
    /// `def`, its stored one: linked projections where their sources were
    /// (the stored definition keeps them where the last edit left them).
    pub fn followed_sketch(&self, sketch: FeatureUid, def: &SketchDef) -> Option<SketchDef> {
        let output = self.sketch_output(sketch)?;
        crate::sketch::project::with_moved(def, &output.moved)
    }

    /// The solve of a sketch's last evaluation when it is the solve of its
    /// definition with the parameters' values now (mitcad#679): the
    /// evaluation succeeded, was made from this very definition (the one
    /// the cache keeps with it, else by fingerprint) and read the values
    /// the parameters have now. The
    /// `sketch` query then shows it instead of solving again; the solve
    /// is the same, as the same definition and values always solve the
    /// same. None for a sketch with projections (its evaluation solves
    /// what its links follow, the query the definition as it evaluated),
    /// and when anything differs: the caller solves.
    pub fn evaluated_solve(&self, sketch: FeatureUid) -> Option<Arc<crate::sketch::solve::Solved>> {
        let entry = self.feature(sketch)?;
        let FeatureDef::Sketch(def) = &entry.def else {
            return None;
        };
        if !def.projections.is_empty() {
            return None;
        }
        let result = self.feature_result(sketch)?;
        if !result.status.is_ok() {
            return None;
        }
        let output = result.output.as_ref()?;
        let solved = &output.sketch.as_ref()?.value.solved;
        let params = &self.state.parameters;
        let same_values = output.reads.iter().all(|read| match read {
            // A number's bits, a text's hash (mitcad#664).
            crate::recompute::Read::Param(id, bits) => params.read_bits(*id) == *bits,
            _ => true,
        });
        // The definition it was made from, as the cache keeps it (a rename
        // changes no definition), else by its fingerprint.
        let same_def = match self.cache.definition_of(sketch, output) {
            Some(made_from) => *made_from == entry.def,
            None => crate::recompute::def_fingerprint(&entry.def, params) == output.def_fp,
        };
        (same_values && same_def).then(|| solved.clone())
    }

    /// What a `sketch` query's answer is made from now: the sketch's entry,
    /// its last evaluation's status and version, and its warnings.
    #[allow(clippy::type_complexity)]
    fn sketch_answer_inputs(
        &self,
        sketch: FeatureUid,
    ) -> Option<(
        &Arc<FeatureEntry>,
        Option<(&FeatureStatus, Option<crate::recompute::Version>)>,
        &[String],
    )> {
        let entry = self
            .state
            .feature_index
            .find(&self.state.features, sketch)?;
        let result = self
            .feature_result(sketch)
            .map(|r| (&r.status, r.output.as_ref().map(|o| o.version)));
        Some((entry, result, self.warnings(sketch)))
    }

    /// The `sketch` query's last answer for `sketch` while nothing it is
    /// made from changed (mitcad#679): the same definition (the entry
    /// itself: an edit makes a new one, as the answer holds the old), the
    /// same evaluation, warnings and parameters. The application asks it
    /// several times per refresh, and a large sketch's answer takes long.
    pub fn known_sketch_answer(&self, sketch: FeatureUid) -> Option<String> {
        let (entry, result, warnings) = self.sketch_answer_inputs(sketch)?;
        let answers = self.sketch_answers.lock().ok()?;
        answers
            .iter()
            .find(|a| {
                a.uid == sketch
                    && Arc::ptr_eq(&a.entry, entry)
                    && a.result.as_ref().map(|(s, v)| (s, *v)) == result
                    && a.warnings == warnings
                    && a.params == self.state.parameters
            })
            .map(|a| a.answer.clone())
    }

    /// Keeps a `sketch` query's answer for [`Self::known_sketch_answer`].
    pub fn keep_sketch_answer(&self, sketch: FeatureUid, answer: &str) {
        let Some((entry, result, warnings)) = self.sketch_answer_inputs(sketch) else {
            return;
        };
        let kept = SketchAnswer {
            uid: sketch,
            entry: entry.clone(),
            result: result.map(|(s, v)| (s.clone(), v)),
            warnings: warnings.to_vec(),
            params: self.state.parameters.clone(),
            answer: answer.to_owned(),
        };
        if let Ok(mut answers) = self.sketch_answers.lock() {
            answers.retain(|a| a.uid != sketch);
            answers.insert(0, kept);
            answers.truncate(SKETCH_ANSWERS);
        }
    }

    // Commands.

    /// Runs a command on a copy of the state; on success the copy becomes
    /// the state and the old one an undo step.
    pub fn apply<T>(
        &mut self,
        edit: impl FnOnce(&mut DocState) -> Result<(String, T), ModelError>,
    ) -> Result<T, ModelError> {
        self.apply_with(|state| edit(state).map(|(label, value)| (label, value, true)))
    }

    /// Like [`Document::apply`]; the edit also tells whether anything
    /// evaluation reads changed. Parameter edits that change no value (a
    /// rename, a comment, an expression with the same value) do not
    /// recompute.
    ///
    /// The new state is computed before it is taken, so a cancelled
    /// computation (P7) rejects the command and leaves the document as it
    /// was; what was evaluated before the cancel stays in the cache.
    fn apply_with<T>(
        &mut self,
        edit: impl FnOnce(&mut DocState) -> Result<(String, T, bool), ModelError>,
    ) -> Result<T, ModelError> {
        self.apply_inner(edit, |_, _| Ok(()))
    }

    /// Like [`Document::apply`]; `check` sees the result of the new state
    /// before it is taken and can still refuse it (mitcad#219: a hierarchy
    /// move must not make a feature fail).
    pub(crate) fn apply_checked<T>(
        &mut self,
        edit: impl FnOnce(&mut DocState) -> Result<(String, T), ModelError>,
        check: impl FnOnce(&DocState, &Recomputed<K::Shape>) -> Result<(), ModelError>,
    ) -> Result<T, ModelError> {
        self.apply_inner(
            |state| edit(state).map(|(label, value)| (label, value, true)),
            check,
        )
    }

    fn apply_inner<T>(
        &mut self,
        edit: impl FnOnce(&mut DocState) -> Result<(String, T, bool), ModelError>,
        check: impl FnOnce(&DocState, &Recomputed<K::Shape>) -> Result<(), ModelError>,
    ) -> Result<T, ModelError> {
        let mut next = self.state.clone();
        let (label, value, mut recompute) = edit(&mut next)?;
        // A library source goes with its last derived part (mitcad#375).
        next.prune_library_sources(&self.state);
        // Configurations follow renames, also those a value slot's
        // `name = expression` made (mitcad#231).
        if next.parameters != self.state.parameters {
            for (old, new) in renamed_parameters(&self.state.parameters, &next.parameters) {
                next.configurations
                    .rename_parameter(&old, &new)
                    .map_err(invalid)?;
            }
        }
        // Nothing that breaks the design policy is added (mitcad#220).
        self.check_policy(&next)?;
        // Driven dimensions' references are used only after their sketches
        // (mitcad#231).
        next.check_reference_order()?;
        // Parameters can disappear with their owning features or
        // components, or change units or dependencies. Keep every saved
        // configuration applicable before committing any such edit.
        // A table edit too (mitcad#667): rows look values up in them.
        if (next.parameters != self.state.parameters
            || next.configurations != self.state.configurations
            || next.tables != self.state.tables)
            && let Some(problem) = next.configurations.problems(&next.parameters).first()
        {
            return Err(invalid(format!("configurations: {problem}")));
        }
        // The cascade of the configuration's rules and closed table
        // (mitcad#798, mitcad#799): options that are no longer allowed
        // move, in this undo step; a rule the change breaks refuses it.
        let moves = if next.parameters != self.state.parameters
            || next.configurations != self.state.configurations
        {
            configurations::enforce(&self.state, &mut next)?
        } else {
            Vec::new()
        };
        recompute |= !moves.is_empty();
        // Bindings follow renames, and their values follow the parameters
        // (mitcad#800), after the cascade so that they settle on the options it
        // moved: a feature whose suppression changed recomputes.
        if next.parameters != self.state.parameters {
            for (old, new) in renamed_parameters(&self.state.parameters, &next.parameters) {
                next.rename_in_bindings(&old, &new);
            }
        }
        recompute |= next.settle_bindings();
        // Timeline groups stay runs whatever the edit did (P9).
        next.normalize_groups();
        // Studies and contact sets lose what deleted joints and
        // occurrences leave behind (mitcad#454).
        next.prune_motion();
        // A sketch that got its first consumer hides, one that lost its
        // last shows (mitcad#7).
        next.follow_sketch_use(&self.state);
        let result = if recompute {
            Some(self.compute_after(&next)?)
        } else {
            None
        };
        if let Some(result) = &result {
            check(&next, result)?;
            next.take_measured(&result.measured);
        }
        // An edited definition that builds clears its import note
        // (mitcad#170).
        if let Some(result) = &result {
            next.settle_import_notes(&self.state, result);
        }
        self.commit(label, next);
        self.configuration_moves = (self.revision, moves);
        if let Some(result) = result {
            self.set_result(result);
            self.name_new_bodies();
        }
        Ok(value)
    }

    /// Makes `next` the state, with a new revision, and the state it
    /// replaces an undo step named `label`.
    fn commit(&mut self, label: String, next: DocState) {
        let revision = self.new_revision();
        let mut before = std::mem::replace(&mut self.state, next);
        // (Found again if the step comes back, mitcad#841.)
        before.feature_index.clear();
        self.undo.push(Step {
            label,
            state: before,
            revision: std::mem::replace(&mut self.revision, revision),
            files: Vec::new(),
        });
        self.redo.clear();
        self.preview = None;
    }

    fn name_new_bodies(&mut self) {
        // (Only when a body has no name yet: the names of every body were
        // copied and numbered after each command, also when the importer
        // that made it named all its bodies, mitcad#841.)
        let names = &self.state.body_names;
        let unnamed = self.result.results.iter().any(|feature| {
            feature.output.as_ref().is_some_and(|output| {
                // (By the heads: restored shapes stay packed, mitcad#858.)
                output.changes.heads().iter().any(|change| {
                    matches!(change, crate::recompute::ChangeHead::Set(uid, _)
                        if !names.contains_key(uid))
                })
            })
        });
        if !unnamed {
            return;
        }
        for (uid, name) in display_names(&self.state, &self.result) {
            self.state.body_names.entry(uid).or_insert(name);
        }
    }

    /// Refuses profile regions the evaluated sketch has nothing like.
    /// Region keys depend on the solved geometry, so this checks against
    /// the last recompute; a key that changed with the sketch still finds
    /// the region with the most curves in common.
    fn check_profiles(&self, def: &FeatureDef<ValueInput>) -> Result<(), ModelError> {
        let FeatureDef::Extrude(extrude) = def else {
            return Ok(());
        };
        for p in &extrude.profiles {
            let Some(output) = self.result.sketches.get(&p.sketch) else {
                continue;
            };
            if output.value.resolve_region(&p.region).is_none() {
                let name = self
                    .state
                    .entry(p.sketch)
                    .map_or_else(|| p.sketch.to_string(), |f| f.name.clone());
                return Err(invalid(format!("{name} has no profile {}", p.region)));
            }
        }
        Ok(())
    }

    /// Adds a feature at the timeline marker, to the active component.
    /// Numbers in the definition become dimension parameters owned by the
    /// feature.
    pub fn add_feature(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
    ) -> Result<Added, ModelError> {
        self.add_feature_to(def, name, None)
    }

    /// Adds a feature to a component (the active one when None).
    pub fn add_feature_to(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
        component: Option<ComponentUid>,
    ) -> Result<Added, ModelError> {
        let component = component.unwrap_or(self.state.assembly.active);
        self.state.editable(component)?;
        self.check_profiles(def)?;
        // A new component of a component that takes none goes beside it
        // (mitcad#292).
        let beside = self.component_parent_beside(component);
        self.apply(|state| {
            let added = state.add_feature_in(def, name, component)?;
            state.place_made_beside(added.uid, None, beside);
            Ok((format!("Add {}", added.name), added))
        })
    }

    /// Adds two features to a component (the active one when None) in one
    /// undo step named `label`, computed once; the second's definition may
    /// name the first. When either is refused nothing of the pair stays and
    /// the undo and redo steps stay as they were (mitcad#758). Made for
    /// gear pairs and belt drives: only the first's profiles are checked.
    pub fn add_feature_pair(
        &mut self,
        label: &str,
        first: &FeatureDef<ValueInput>,
        second: impl FnOnce(FeatureUid) -> FeatureDef<ValueInput>,
        names: [Option<&str>; 2],
        component: Option<ComponentUid>,
    ) -> Result<(Added, Added), ModelError> {
        let component = component.unwrap_or(self.state.assembly.active);
        self.state.editable(component)?;
        self.check_profiles(first)?;
        let beside = self.component_parent_beside(component);
        self.apply(|state| {
            let a = state.add_feature_in(first, names[0], component)?;
            state.place_made_beside(a.uid, None, beside);
            let b = state.add_feature_in(&second(a.uid), names[1], component)?;
            state.place_made_beside(b.uid, None, beside);
            Ok((label.to_owned(), (a, b)))
        })
    }

    /// Replaces a feature's definition; the features after it recompute.
    /// Returns the parameters it created.
    pub fn edit_feature(
        &mut self,
        uid: FeatureUid,
        def: &FeatureDef<ValueInput>,
    ) -> Result<Vec<String>, ModelError> {
        if let Some(entry) = self.state.entry(uid) {
            self.state.editable(entry.component)?;
        }
        self.check_profiles(def)?;
        let (made, beside) = self.made_and_beside(uid);
        self.apply(|state| {
            let created = state.edit_feature(uid, def)?;
            state.place_made_beside(uid, made, beside);
            let name = state.entry(uid).expect("edited").name.clone();
            Ok((format!("Edit {name}"), created))
        })
    }

    /// Deletes a feature and the parameters it owned. Features that use it
    /// make the command fail, unless `with_dependents` deletes them too.
    /// Returns the deleted features in timeline order.
    pub fn delete_feature(
        &mut self,
        uid: FeatureUid,
        with_dependents: bool,
    ) -> Result<Vec<FeatureUid>, ModelError> {
        if let Some(entry) = self.state.entry(uid) {
            self.state.editable(entry.component)?;
        }
        self.apply(|state| {
            let position = state.require(uid)?;
            let name = state.features[position].name.clone();
            let dependents = state.dependents(&BTreeSet::from([uid]));
            if !dependents.is_empty() && !with_dependents {
                return Err(ModelError::Dependents {
                    feature: name,
                    dependents: state.names_of(&dependents),
                });
            }
            let removed: BTreeSet<FeatureUid> = dependents.iter().copied().chain([uid]).collect();
            let order = state.remove_features(&removed);
            Ok((format!("Delete {name}"), order))
        })
    }

    /// Suppressed features are skipped by recompute. A condition the
    /// suppression followed (mitcad#800) is removed in the same undo step.
    pub fn set_suppressed(&mut self, uid: FeatureUid, suppressed: bool) -> Result<(), ModelError> {
        let position = self.state.require(uid)?;
        if self.state.features[position].suppressed == suppressed
            && !self.state.bindings.suppressed_when.contains_key(&uid)
        {
            return Ok(());
        }
        self.apply(|state| {
            state.bindings.suppressed_when.remove(&uid);
            let entry = Arc::make_mut(&mut state.features[position]);
            entry.suppressed = suppressed;
            let verb = if suppressed { "Suppress" } else { "Unsuppress" };
            Ok((format!("{verb} {}", entry.name), ()))
        })
    }

    /// Moves a feature to a new timeline index. Every feature must still
    /// come after the features it refers to.
    pub fn move_feature(&mut self, uid: FeatureUid, index: usize) -> Result<(), ModelError> {
        self.apply(|state| {
            let moved = state.moved(uid, index)?;
            let name = state.entry(uid).expect("moved").name.clone();
            *state = moved;
            Ok((format!("Move {name}"), ()))
        })
    }

    /// Moves the timeline marker: features from `position` on are rolled
    /// back. `position` is the number of features before the marker.
    pub fn set_marker(&mut self, position: usize) -> Result<(), ModelError> {
        if position > self.state.features.len() {
            return Err(invalid(format!(
                "marker position {position} is past the end of the timeline ({})",
                self.state.features.len()
            )));
        }
        if position == self.state.marker {
            return Ok(());
        }
        self.apply(|state| {
            state.marker = position;
            Ok(("Move Timeline Marker".to_owned(), ()))
        })
    }

    /// Moves the marker for a command's preview without adding a history
    /// step, clearing redo or changing the saved revision. The caller must
    /// restore the original marker before committing or saving a command.
    /// A cancelled computation leaves the state and its results unchanged.
    pub fn set_temporary_marker(&mut self, position: usize) -> Result<(), ModelError> {
        if position > self.state.features.len() {
            return Err(invalid(format!(
                "marker position {position} is past the end of the timeline ({})",
                self.state.features.len()
            )));
        }
        if position == self.state.marker {
            return Ok(());
        }
        let mut next = self.state.clone();
        next.marker = position;
        let result = self.compute(&next)?;
        self.state = next;
        self.preview = None;
        self.set_result(result);
        Ok(())
    }

    pub fn rename_feature(&mut self, uid: FeatureUid, name: &str) -> Result<(), ModelError> {
        self.apply(|state| {
            let position = state.require(uid)?;
            state.check_feature_name(name, Some(uid))?;
            let entry = Arc::make_mut(&mut state.features[position]);
            let label = format!("Rename {} to {name}", entry.name);
            entry.name = name.to_owned();
            Ok((label, ()))
        })
    }

    pub fn rename_body(&mut self, uid: BodyUid, name: &str) -> Result<(), ModelError> {
        let names = display_names(&self.state, &self.result);
        self.apply(|state| {
            if state.position(uid.feature).is_none() {
                return Err(invalid(format!("body {uid} does not exist")));
            }
            if name.trim().is_empty() {
                return Err(invalid("the body name is empty"));
            }
            if names.iter().any(|(other, n)| n == name && *other != uid) {
                return Err(invalid(format!("a body is already named '{name}'")));
            }
            state.body_names.insert(uid, name.to_owned());
            Ok((format!("Rename Body to {name}"), ()))
        })
    }

    /// Adds a user parameter with a value in millimetres.
    pub fn add_parameter(
        &mut self,
        name: &str,
        value: f64,
        comment: &str,
    ) -> Result<(), ModelError> {
        if !value.is_finite() {
            return Err(ParameterError::NotFinite {
                name: name.to_owned(),
                value,
            }
            .into());
        }
        let unit = Unit::of_length(self.state.parameters.context().default_length_unit);
        self.add_parameter_expression(name, &value_to_expression(value, unit), Some(unit), comment)
    }

    /// Adds a user parameter with an expression, such as `d1 * 2 + 5 mm`.
    /// Without a unit, it gets one for the expression's value: the
    /// document's length unit, degrees, none, or `Text` for a text
    /// (`'6qt'`, mitcad#664).
    pub fn add_parameter_expression(
        &mut self,
        name: &str,
        expression: &str,
        unit: Option<Unit>,
        comment: &str,
    ) -> Result<(), ModelError> {
        self.add_parameter_with_limits(name, expression, unit, comment, ParamLimits::default())
    }

    /// [`Document::add_parameter_expression`] with limits (mitcad#665):
    /// the parameter's value is its expression's result clamped to them,
    /// or, strict, refused outside them.
    pub fn add_parameter_with_limits(
        &mut self,
        name: &str,
        expression: &str,
        unit: Option<Unit>,
        comment: &str,
        limits: ParamLimits,
    ) -> Result<(), ModelError> {
        self.apply(|state| {
            let unit = match unit {
                Some(unit) => unit,
                // A text expression makes a text parameter (mitcad#664).
                None if mitcad_model_base::internal::parameters::expression_is_text(
                    &state.parameters,
                    expression,
                ) =>
                {
                    Unit::TEXT
                }
                None => {
                    let dims = expression_dims(&state.parameters, expression)?;
                    unit_for(dims, &state.parameters.context())
                        .ok_or_else(|| invalid(format!("'{expression}' has no unit; give one")))?
                }
            };
            let spec = ParamSpec::user(name, expression, unit)
                .with_comment(comment)
                .with_limits(limits);
            state.parameters.add(spec, None)?;
            Ok((format!("Add Parameter {name}"), ()))
        })
    }

    fn param_id(&self, name: &str) -> Result<ParamId, ModelError> {
        Ok(self
            .state
            .parameters
            .find(name)
            .ok_or_else(|| ParameterError::Unknown(name.to_owned()))?)
    }

    /// Sets a parameter to a value in millimetres or radians, written in
    /// its unit; the features that use it recompute. An unchanged value is
    /// no command at all.
    pub fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), ModelError> {
        self.set_parameter_value(name, value)?;
        Ok(())
    }

    /// Like [`Document::set_parameter`]; returns the parameters whose
    /// values changed, taken from the table's change set so that the text
    /// parameters that use it are among them (mitcad#758).
    pub fn set_parameter_value(
        &mut self,
        name: &str,
        value: f64,
    ) -> Result<Vec<String>, ModelError> {
        let id = self.param_id(name)?;
        // A limited value is the same only when its result was too.
        let unchanged = self
            .state
            .parameters
            .get(id)
            .is_some_and(|p| p.value() == value && p.unclamped() == value);
        if unchanged {
            return Ok(Vec::new());
        }
        self.apply(|state| {
            let changed = state.parameters.set_value(id, value)?;
            let names = changed
                .iter()
                .map(|id| state.parameters.name(*id))
                .collect::<Vec<_>>();
            Ok((format!("Change {name}"), names))
        })
    }

    /// Changes a parameter's expression (and its unit, if given). Returns
    /// the parameters whose values changed; when none did, nothing is
    /// recomputed. The same expression is no command at all.
    pub fn set_parameter_expression(
        &mut self,
        name: &str,
        expression: &str,
        unit: Option<Unit>,
    ) -> Result<Vec<String>, ModelError> {
        let id = self.param_id(name)?;
        let param = self.state.parameters.get(id).expect("found");
        let unit = unit.unwrap_or(param.unit());
        // Kept with decimal points: `1,5` is the same as `1.5`.
        if param.expression() == with_decimal_points(expression) && param.unit() == unit {
            return Ok(Vec::new());
        }
        if unit != param.unit() {
            self.state.check_unit_change(id, unit)?;
        }
        let changed = self.apply_with(|state| {
            let changed = state.parameters.update(id, expression, unit)?;
            let names = changed
                .iter()
                .map(|id| state.parameters.name(*id))
                .collect::<Vec<_>>();
            let recompute = !changed.is_empty();
            Ok((format!("Change {name}"), names, recompute))
        })?;
        Ok(changed)
    }

    /// Changes a parameter's limits (mitcad#665) and, when given, its
    /// expression and unit in the same undo step, so a new expression is
    /// checked against the new limits. Returns the parameters whose values
    /// changed; the same limits and expression are no command at all.
    pub fn set_parameter_limits(
        &mut self,
        name: &str,
        limits: ParamLimits,
        expression: Option<&str>,
        unit: Option<Unit>,
    ) -> Result<Vec<String>, ModelError> {
        let id = self.param_id(name)?;
        let param = self.state.parameters.get(id).expect("found");
        let unit = unit.unwrap_or(param.unit());
        let expression = expression.map_or_else(
            || param.expression().to_owned(),
            |e| with_decimal_points(e).into_owned(),
        );
        if param.limits() == &limits && param.expression() == expression && param.unit() == unit {
            return Ok(Vec::new());
        }
        if unit != param.unit() {
            self.state.check_unit_change(id, unit)?;
        }
        let label = if param.limits() == &limits {
            format!("Change {name}")
        } else {
            format!("Change Limits of {name}")
        };
        self.apply_with(|state| {
            let changed = state
                .parameters
                .redefine(id, &expression, unit, Some(limits))?;
            let names = changed
                .iter()
                .map(|id| state.parameters.name(*id))
                .collect::<Vec<_>>();
            let recompute = !changed.is_empty();
            Ok((label, names, recompute))
        })
    }

    pub fn set_parameter_comment(&mut self, name: &str, comment: &str) -> Result<(), ModelError> {
        let id = self.param_id(name)?;
        self.apply_with(|state| {
            state.parameters.set_comment(id, comment)?;
            Ok((format!("Change Comment of {name}"), (), false))
        })
    }

    /// Renames a parameter; expressions that use it are rewritten and
    /// features follow it (they refer to it by id).
    pub fn rename_parameter(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let id = self.param_id(name)?;
        self.apply_with(|state| {
            // Configurations follow in `apply_inner`.
            state.parameters.rename(id, new_name)?;
            Ok((format!("Rename {name} to {new_name}"), (), false))
        })
    }

    /// Makes user parameters model parameters of a feature, which then owns
    /// them as if it had made them (an importer recreates the file's model
    /// parameters by name before the feature that owns them). Names that
    /// do not exist or already have an owner are left alone; returns the
    /// names adopted. No recompute: values do not change.
    pub fn adopt_parameters(
        &mut self,
        owner: FeatureUid,
        names: &[String],
    ) -> Result<Vec<String>, ModelError> {
        let feature = self.state.require(owner)?;
        let ids: Vec<ParamId> = names
            .iter()
            .filter_map(|name| self.state.parameters.find(name))
            .filter(|id| self.state.parameters.owner(*id).is_none())
            .collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let label = format!("Adopt Parameters to {}", self.state.features[feature].name);
        self.apply_with(|state| {
            for id in &ids {
                state.parameters.set_owner(*id, Some(owner));
            }
            let names = ids.iter().map(|id| state.parameters.name(*id)).collect();
            Ok((label, names, false))
        })
    }

    /// Deletes a parameter that no feature and no other parameter uses.
    pub fn delete_parameter(&mut self, name: &str) -> Result<(), ModelError> {
        let id = self.param_id(name)?;
        self.apply_with(|state| {
            let mut users = state.param_users(id);
            // The rules that use it (mitcad#799); an option and a row's
            // parameter are refused by the table's check ("configurations:
            // ... does not exist").
            users.extend(
                state
                    .configurations
                    .rules_using(name)
                    .into_iter()
                    .map(|rule| format!("the rule {rule}")),
            );
            if !users.is_empty() {
                return Err(ParameterError::InUse {
                    name: name.to_owned(),
                    users,
                }
                .into());
            }
            state.parameters.remove(id)?;
            state.favorites.remove(&id);
            Ok((format!("Delete Parameter {name}"), (), false))
        })
    }

    /// Changes the document's default length unit; bare numbers in
    /// expressions are read in it (the document units).
    pub fn set_units(&mut self, length: LengthUnit) -> Result<Vec<String>, ModelError> {
        let mut context = self.state.parameters.context();
        if context.default_length_unit == length {
            return Ok(Vec::new());
        }
        context.default_length_unit = length;
        self.apply_with(|state| {
            let changed = state.parameters.set_context(context)?;
            let names = changed
                .iter()
                .map(|id| state.parameters.name(*id))
                .collect::<Vec<_>>();
            let recompute = !changed.is_empty();
            Ok((format!("Change Units to {length}"), names, recompute))
        })
    }

    /// Starts an empty sketch at the marker.
    pub fn add_sketch(&mut self, plane: SketchPlane) -> Result<Added, ModelError> {
        let def = FeatureDef::Sketch(SketchDef::new(plane));
        self.add_feature(&def, None)
    }

    /// Edits a sketch (see [`SketchEdit`]): `edit` changes a copy of its
    /// definition; the sketch is then solved from the edited positions and
    /// the solution stored. `what` names the undo step (`Add Line` gives
    /// `Add Line to Sketch1`). A sketch that no longer solves rejects the
    /// edit.
    pub fn edit_sketch<T>(
        &mut self,
        sketch: FeatureUid,
        what: &str,
        edit: impl FnOnce(&mut SketchEdit<'_>) -> Result<T, String>,
    ) -> Result<(T, EditReport), ModelError> {
        if let Some(entry) = self.state.entry(sketch) {
            self.state.editable(entry.component)?;
        }
        // Edits start from the sketch as it evaluated: linked projections
        // where their sources are now (mitcad#40).
        let moved = self.sketch_output(sketch).map(|o| o.moved.clone());
        self.apply(|state| {
            let position = state.require(sketch)?;
            let entry = state.features[position].clone();
            let FeatureDef::Sketch(def) = &entry.def else {
                return Err(invalid(format!(
                    "{} ({sketch}) is not a sketch",
                    entry.name
                )));
            };
            let def = moved
                .and_then(|m| crate::sketch::project::with_moved(def, &m))
                .unwrap_or_else(|| def.clone());
            let (value, def, report) = {
                let mut editor = SketchEdit::new(def, &mut state.parameters, sketch, &entry.name);
                let value = edit(&mut editor).map_err(invalid)?;
                let (def, report) = editor.finish().map_err(invalid)?;
                (value, def, report)
            };
            let edited = FeatureEntry {
                def: FeatureDef::Sketch(def),
                ..(*entry).clone()
            };
            state
                .check_feature(position, &edited)
                .map_err(ModelError::Invalid)?;
            state.features[position] = Arc::new(edited);
            for later in position + 1..state.features.len() {
                let entry = state.features[later].clone();
                state
                    .check_feature(later, &entry)
                    .map_err(|e| invalid(format!("{} would no longer work: {e}", entry.name)))?;
            }
            state.release_params(sketch);
            let label = if what.contains("{sketch}") {
                what.replace("{sketch}", &entry.name)
            } else {
                format!("{what} in {}", entry.name)
            };
            Ok((label, (value, report)))
        })
    }

    /// The bodies before a timeline position, from the last recompute.
    fn bodies_before(&self, position: usize) -> BTreeMap<BodyUid, K::Shape> {
        let mut bodies = BTreeMap::new();
        for result in self.result.results.iter().take(position) {
            let Some(output) = &result.output else {
                continue;
            };
            for change in &output.changes {
                match change {
                    crate::recompute::Change::Set(uid, shape) => {
                        bodies.insert(*uid, shape.value.clone());
                    }
                    crate::recompute::Change::Remove(uid) => {
                        bodies.remove(uid);
                    }
                }
            }
        }
        bodies
    }

    /// A body as it was before the feature `uid` (its input), from the
    /// last recompute; none when the body was not there yet.
    pub fn body_before(&self, uid: FeatureUid, body: BodyUid) -> Option<&K::Shape> {
        let position = self.state.position(uid)?;
        self.result
            .results
            .iter()
            .take(position)
            .rev()
            .filter_map(|result| result.output.as_deref())
            .find_map(|output| {
                output.changes.iter().rev().find_map(|change| match change {
                    crate::recompute::Change::Set(uid, shape) if *uid == body => {
                        Some(Some(&shape.value))
                    }
                    crate::recompute::Change::Remove(uid) if *uid == body => Some(None),
                    _ => None,
                })
            })
            .flatten()
    }

    /// The cylinder of a tapped hole's wall on a body whose modelled thread
    /// has its crest off it (leaving no piece of it, or only pieces outside
    /// the thread's part), as the hole measured it before the thread
    /// (mitcad#436, mitcad#628); none for any other wall.
    pub fn threaded_wall(
        &self,
        uid: FeatureUid,
        body: BodyUid,
        wall: &crate::topo::FaceName,
    ) -> Option<&crate::kernel::Cylinder> {
        let output = self.feature_result(uid)?.output.as_deref()?;
        output
            .threaded_walls
            .iter()
            .find(|w| w.body == body && &w.wall == wall)
            .map(|w| &w.cylinder)
    }

    /// What a feature picked, where it was before it (a Fillet's or a
    /// Chamfer's edges as polylines in its component's coordinates,
    /// mitcad#957), as its last build kept it: no geometry is computed
    /// here. None for a feature that kept none: not built (a document
    /// opened without building), failed, suppressed, rolled back, or of a
    /// kind without such inputs.
    pub fn feature_inputs(&self, uid: FeatureUid) -> Option<&crate::features::FeatureInputs> {
        let output = self.feature_result(uid)?.output.as_deref()?;
        if output.result.is_err() {
            return None;
        }
        output.inputs.as_ref()
    }

    /// Projects model geometry into a sketch (Project): the curves
    /// of an edge, a face's boundary or a vertex of the bodies before the
    /// sketch, as fixed reference entities. A linked projection follows
    /// its source when the model changes.
    pub fn project_into_sketch(
        &mut self,
        sketch: FeatureUid,
        source: &crate::topo::TopoName,
        body: Option<BodyUid>,
        linked: bool,
    ) -> Result<EditReport, ModelError> {
        self.project_from(sketch, source, body, linked, None, None)
    }

    /// [`Document::project_into_sketch`] of geometry picked where an
    /// occurrence path from the root (`occurrence`) places it, for the
    /// sketch's component seen at `context` (mitcad#100): geometry of
    /// another component is moved into the sketch's coordinates by the
    /// placements at the marker, and a linked projection keeps the
    /// [`crate::links::OccurrenceLink`] to follow it.
    pub fn project_from(
        &mut self,
        sketch: FeatureUid,
        source: &crate::topo::TopoName,
        body: Option<BodyUid>,
        linked: bool,
        occurrence: Option<&[OccurrenceUid]>,
        context: Option<&[OccurrenceUid]>,
    ) -> Result<EditReport, ModelError> {
        self.project_or_section(
            sketch,
            Some(source),
            body,
            linked,
            occurrence,
            context,
            false,
        )
    }

    /// Section curves (mitcad#389): where the sketch's plane cuts a face
    /// (every face of a name without `#k`), as [`Document::project_from`]
    /// adds a projection; a linked section is recomputed from the body
    /// when the model changes ([`crate::sketch::Projection::section`]).
    /// Intersect (mitcad#736) also takes an edge, whose crossing points
    /// with the plane come in as points, and a whole body (`source` None,
    /// `body` given): the section of each face the plane cuts is a
    /// projection of its own, all in one edit.
    pub fn section_from(
        &mut self,
        sketch: FeatureUid,
        source: Option<&crate::topo::TopoName>,
        body: Option<BodyUid>,
        linked: bool,
        occurrence: Option<&[OccurrenceUid]>,
        context: Option<&[OccurrenceUid]>,
    ) -> Result<EditReport, ModelError> {
        self.project_or_section(sketch, source, body, linked, occurrence, context, true)
    }

    /// Projects curves and points of another sketch into a sketch
    /// (mitcad#904): `entities` of the sketch `source` (lines, arcs,
    /// circles, ellipses, splines and points, construction and projected
    /// ones too), or every curve of it when None (a whole sketch picked).
    /// The source sketch comes before `sketch` in the timeline; a later one
    /// is refused, so no sketch follows one that follows it. Its geometry
    /// is projected along the sketch's normal as body edges are: an exact
    /// copy on the same plane, the same shapes on a parallel one, ellipses
    /// and shorter lines on an oblique one. Linked, each entity is a
    /// projection of its own ([`crate::sketch::SketchEntityRef`]) that
    /// follows it when the source sketch changes, and keeps its last
    /// geometry with a warning once the entity is gone; unlinked, they are
    /// fixed copies. Another component's sketch comes through the link of
    /// `occurrence` (seen at `context`), as its bodies' edges do.
    pub fn project_sketch_geometry(
        &mut self,
        sketch: FeatureUid,
        source: FeatureUid,
        entities: Option<&[crate::sketch::Ref]>,
        linked: bool,
        occurrence: Option<&[OccurrenceUid]>,
        context: Option<&[OccurrenceUid]>,
    ) -> Result<EditReport, ModelError> {
        use crate::sketch::project::SourceCurves;
        use crate::sketch::{ProjectionSource, SketchEntityRef};
        let source_name = match entities {
            Some([one]) => SketchEntityRef {
                sketch: source,
                entity: *one,
            }
            .to_string(),
            _ => source.to_string(),
        };
        let fail = |e: String| invalid(format!("projection of {source_name}: {e}"));
        let target = self.projection_target(sketch, occurrence, context, &fail)?;
        if source == sketch {
            return Err(fail(
                "a sketch does not project its own geometry; pick curves and points of \
                 another sketch"
                    .to_owned(),
            ));
        }
        let at = self.state.require(source)?;
        let entry = &self.state.features[at];
        let target_entry = &self.state.features[target.position];
        if !matches!(entry.def, FeatureDef::Sketch(_)) {
            return Err(fail(format!("{} ({source}) is not a sketch", entry.name)));
        }
        if at > target.position {
            return Err(fail(format!(
                "{} ({source}) comes after {} ({sketch}) in the timeline; only the curves and \
                 points of a sketch before it can be projected",
                entry.name, target_entry.name
            )));
        }
        let assembly = &self.state.assembly;
        if entry.component != target.from {
            return Err(fail(format!(
                "{} is a sketch of {}, not of {}; pick it where an occurrence of {} shows it",
                entry.name,
                assembly.name(entry.component),
                assembly.name(target.from),
                assembly.name(entry.component)
            )));
        }
        let output = self.sketch_output(source).ok_or_else(|| {
            fail(format!(
                "{} ({source}) does not evaluate; its geometry cannot be projected",
                entry.name
            ))
        })?;
        let picked: Vec<crate::sketch::Ref> = match entities {
            Some(list) => list.to_vec(),
            None => output
                .solved
                .curves
                .keys()
                .map(|id| crate::sketch::Ref::Curve(*id))
                .collect(),
        };
        if picked.is_empty() {
            return Err(fail(format!("{} has no curves to project", entry.name)));
        }
        let mut groups = Vec::with_capacity(picked.len());
        for entity in picked {
            let curve = crate::features::sketch::sketch_entity_curve(output, entity)
                .ok_or_else(|| fail(format!("{} has no {entity}", entry.name)))?;
            groups.push(SourceCurves {
                source: Some(ProjectionSource::Sketch(SketchEntityRef {
                    sketch: source,
                    entity,
                })),
                chain: Vec::new(),
                curves: vec![curve],
            });
        }
        let ProjectionTarget {
            frame,
            link,
            to_sketch,
            ..
        } = target;
        self.push_projections(
            sketch,
            &frame,
            groups,
            to_sketch.as_ref(),
            Projecting {
                body: None,
                link,
                linked,
                section: false,
                label: "Project to {sketch}",
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn project_or_section(
        &mut self,
        sketch: FeatureUid,
        source: Option<&crate::topo::TopoName>,
        body: Option<BodyUid>,
        linked: bool,
        occurrence: Option<&[OccurrenceUid]>,
        context: Option<&[OccurrenceUid]>,
        section: bool,
    ) -> Result<EditReport, ModelError> {
        use crate::sketch::project::SourceCurves;
        use crate::topo::TopoName;
        let what = if section { "section" } else { "projection" };
        // What the messages call the source: a whole body by its uid.
        let source_name = match (source, body) {
            (Some(source), _) => source.to_string(),
            (None, Some(body)) if section => body.to_string(),
            _ => {
                return Err(invalid(
                    "a projection needs a source; a section needs a source or a body",
                ));
            }
        };
        let target = self.projection_target(sketch, occurrence, context, &|e| {
            invalid(format!("{what} of {source_name}: {e}"))
        })?;
        let ProjectionTarget {
            position,
            frame,
            from,
            link,
            to_sketch,
        } = target;
        let bodies = self.bodies_before(position);
        let assembly = &self.state.assembly;
        if let Some(b) = body
            && let Some(owner) = self.body_component(b)
            && owner != from
        {
            return Err(invalid(format!(
                "{what} of {source_name}: {b} is a body of {}, not of {}; pick it where an \
                 occurrence of {} shows it",
                assembly.name(owner),
                assembly.name(from),
                assembly.name(owner)
            )));
        }
        // A section's plane where the source is: the sketch's moved back
        // by the link.
        let plane = match (section, source) {
            (false, _) => None,
            (true, None | Some(TopoName::Face(_) | TopoName::Edge(_))) => {
                let plane = crate::datum::DatumPlane {
                    origin: frame.origin,
                    x_axis: frame.x_axis,
                    y_axis: frame.y_axis,
                };
                Some(match &to_sketch {
                    Some(t) => {
                        crate::sketch::project::section_plane(&crate::assembly::inverse(t), &plane)
                    }
                    None => plane,
                })
            }
            (true, Some(_)) => {
                return Err(invalid(format!(
                    "section of {source_name}: a section's source is a face, an edge or a body"
                )));
            }
        };
        let one = |curves: Vec<crate::kernel::Curve3>| {
            vec![SourceCurves {
                source: source.cloned().map(crate::sketch::ProjectionSource::Topo),
                chain: Vec::new(),
                curves,
            }]
        };
        let error =
            |e: crate::kernel::KernelError| invalid(format!("{what} of {source_name}: {e}"));
        let mut groups: Vec<SourceCurves> = Vec::new();
        for (uid, shape) in &bodies {
            if body.is_some_and(|b| b != *uid)
                || self.body_component(*uid).is_some_and(|c| c != from)
            {
                continue;
            }
            groups = match (&plane, source) {
                // A whole body: each face's section (mitcad#736).
                (Some(plane), None) => crate::sketch::project::section_groups(
                    self.kernel
                        .section_curves(shape, None, plane)
                        .map_err(error)?,
                ),
                (Some(plane), Some(TopoName::Face(face))) => one(self
                    .kernel
                    .section_curves(shape, Some(face), plane)
                    .map_err(error)?
                    .into_iter()
                    .map(|c| c.curve)
                    .collect()),
                // Where the plane crosses an edge: points (mitcad#736).
                (Some(plane), Some(TopoName::Edge(edge))) => one(self
                    .kernel
                    .section_points(shape, edge, plane)
                    .map_err(error)?
                    .into_iter()
                    .map(crate::kernel::Curve3::Point)
                    .collect()),
                (_, Some(source)) => one(self.kernel.curves_of(shape, source).map_err(error)?),
                (None, None) => Vec::new(),
            };
            groups.retain(|g| !g.curves.is_empty());
            if !groups.is_empty() {
                break;
            }
        }
        if groups.is_empty() {
            return Err(invalid(match (section, source) {
                (true, None) => {
                    format!("section of {source_name}: the sketch's plane does not cut the body")
                }
                (true, Some(_)) => format!(
                    "section of {source_name}: the sketch's plane does not cut it (or nothing \
                     named so is before the sketch)"
                ),
                (false, _) => format!("nothing named {source_name} exists before the sketch"),
            }));
        }
        let label = if section {
            "Section Curves to {sketch}"
        } else {
            "Project to {sketch}"
        };
        self.push_projections(
            sketch,
            &frame,
            groups,
            to_sketch.as_ref(),
            Projecting {
                body,
                link,
                linked,
                section,
                label,
            },
        )
    }

    /// Where a projection into `sketch` goes and where its source is
    /// (Project and Section Curves, mitcad#100, mitcad#904): the sketch's
    /// place in the timeline and its frame, and the component the source
    /// geometry is in, with the link and the transform into the sketch's
    /// coordinates when `occurrence` places it in another component.
    /// `error` words what goes wrong with the link.
    fn projection_target(
        &self,
        sketch: FeatureUid,
        occurrence: Option<&[OccurrenceUid]>,
        context: Option<&[OccurrenceUid]>,
        error: &dyn Fn(String) -> ModelError,
    ) -> Result<ProjectionTarget, ModelError> {
        let position = self.state.require(sketch)?;
        let frame = match (
            self.sketch_output(sketch),
            &self.state.features[position].def,
        ) {
            (Some(output), _) => output.frame,
            (None, FeatureDef::Sketch(def)) => def
                .plane
                .origin_frame()
                .map(|plane| def.place(&plane))
                .ok_or_else(|| invalid("the sketch has no plane yet (it does not evaluate)"))?,
            (None, _) => return Err(invalid(format!("{sketch} is not a sketch"))),
        };
        // The sketch's component's geometry is in its coordinates; another
        // component's is moved there through a link.
        let component = self.state.features[position].component;
        let assembly = &self.state.assembly;
        let link = match occurrence {
            Some(path) => {
                crate::links::link_for(assembly, component, path, context).map_err(error)?
            }
            None => None,
        };
        let (from, to_sketch) = match &link {
            Some(link) => {
                let placements = &self.result.placements;
                let mut placement = |o: OccurrenceUid| {
                    placements.get(&o).copied().unwrap_or_else(|| {
                        assembly
                            .occurrence(o)
                            .map_or(crate::transform::Transform::IDENTITY, |o| o.transform)
                    })
                };
                let from = link.source_component(assembly);
                let to_sketch = link.transform(assembly, component, &mut placement);
                (from.map_err(error)?, Some(to_sketch.map_err(error)?))
            }
            None => (component, None),
        };
        Ok(ProjectionTarget {
            position,
            frame,
            from,
            link,
            to_sketch,
        })
    }

    /// Adds projected curves to a sketch in one edit (`what.label`): each
    /// group's curves, in the sketch's component's coordinates (moved
    /// there by `to_sketch`), projected onto its plane (`frame`), joined to
    /// the projected geometry already there, and for a linked projection
    /// its record. Project and Section Curves of model geometry, and
    /// Project of other sketches' curves and points (mitcad#904).
    fn push_projections(
        &mut self,
        sketch: FeatureUid,
        frame: &crate::profile::SketchFrame,
        groups: Vec<crate::sketch::project::SourceCurves>,
        to_sketch: Option<&crate::transform::Transform>,
        what: Projecting<'_>,
    ) -> Result<EditReport, ModelError> {
        let Projecting {
            body,
            link,
            linked,
            section,
            label,
        } = what;
        // Joined only to the projections of the same sketch's geometry, or
        // of model geometry (mitcad#904).
        let source_sketch = groups
            .iter()
            .find_map(|g| g.source.as_ref().and_then(|s| s.sketch()).map(|e| e.sketch));
        let projected: Vec<_> = groups
            .iter()
            .map(|group| {
                let curves: Vec<crate::kernel::Curve3> = match to_sketch {
                    Some(t) => group
                        .curves
                        .iter()
                        .map(|c| crate::sketch::project::transform_curve(t, c))
                        .collect(),
                    None => group.curves.clone(),
                };
                crate::sketch::project::distinct(crate::sketch::project::project(&curves, frame))
            })
            .collect();
        let ((), report) = self.edit_sketch(sketch, label, |edit| {
            use crate::sketch::project;
            for (group, projected) in groups.into_iter().zip(&projected) {
                // Joined to the projected geometry already there (mitcad#741):
                // an edge projected again, or a face after one of its edges,
                // adds no second curve, and ends that meet share a point.
                let joinable = project::joinable(
                    &edit.def,
                    linked.then_some((body, link.as_ref())),
                    source_sketch,
                );
                let mut next = project::FRESH;
                let fresh = project::entities(projected, &mut || {
                    next += 1;
                    EntityUid(next)
                });
                let joined = project::join_existing(&edit.def, &joinable, fresh, linked);
                let mut entities = joined.entities;
                let ids = edit.ids(entities.len());
                project::renumber(&mut entities, &ids);
                for (_, kept) in &joined.same {
                    edit.report.warnings.push(format!(
                        "c{} is in the sketch already: not projected again",
                        kept.0
                    ));
                }
                let refs = project::refs(&entities);
                for entity in entities {
                    edit.push(entity);
                }
                // Pieces on faces without names come in unlinked (mitcad#736).
                if linked
                    && !refs.is_empty()
                    && let Some(source) = group.source
                {
                    edit.def.projections.push(crate::sketch::Projection {
                        source,
                        body,
                        entities: refs,
                        link: link.clone(),
                        by_geometry: true,
                        section,
                        chain: group.chain,
                    });
                }
            }
            Ok(())
        })?;
        Ok(report)
    }

    /// Adds a rectangle to a sketch, as V0 did: four lines with new curve
    /// ids, the corner points (the first fixed), horizontal and vertical
    /// constraints and width and height dimensions (new parameters for
    /// numbers).
    pub fn add_rectangle(
        &mut self,
        sketch: FeatureUid,
        corner: [f64; 2],
        width: &ValueInput,
        height: &ValueInput,
    ) -> Result<ShapeAdded, ModelError> {
        let (curves, report) = self.edit_sketch(sketch, "Add Rectangle to {sketch}", |edit| {
            edit.dimensioned_rectangle(corner, width, height)
        })?;
        Ok(self.shape_added(sketch, curves, report))
    }

    /// Adds a circle with a fixed centre, dimensioned by its diameter.
    pub fn add_circle(
        &mut self,
        sketch: FeatureUid,
        center: [f64; 2],
        diameter: &ValueInput,
    ) -> Result<ShapeAdded, ModelError> {
        let (curve, report) = self.edit_sketch(sketch, "Add Circle to {sketch}", |edit| {
            edit.dimensioned_circle(center, diameter)
        })?;
        Ok(self.shape_added(sketch, vec![curve], report))
    }

    /// The region bounded by exactly these curves, or the key they would
    /// have alone.
    fn shape_added(
        &self,
        sketch: FeatureUid,
        curves: Vec<EntityUid>,
        report: EditReport,
    ) -> ShapeAdded {
        let region = self
            .result
            .sketches
            .get(&sketch)
            .and_then(|output| {
                output.value.regions.iter().find(|r| {
                    let mut used: Vec<EntityUid> =
                        r.key.segments().filter_map(|s| s.curve.entity()).collect();
                    used.sort_unstable();
                    used.dedup();
                    used == curves
                })
            })
            .map(|r| r.key.clone())
            .unwrap_or_else(|| {
                let n = curves.len();
                let segments: Vec<SegmentKey> = if n == 1 {
                    vec![SegmentKey::closed(curves[0])]
                } else {
                    (0..n)
                        .map(|i| {
                            SegmentKey::between(
                                curves[i],
                                curves[(i + n - 1) % n],
                                curves[(i + 1) % n],
                            )
                        })
                        .collect()
                };
                RegionKey::new(segments).expect("a shape has segments")
            });
        ShapeAdded {
            curves,
            region,
            parameters: report.parameters.clone(),
            report,
        }
    }

    // Undo and redo.

    /// Retains the definition and exact undo/redo history before a
    /// frontend command. Only one checkpoint may exist per document;
    /// callers discard it after accepting or restoring the command.
    pub fn begin_command(&mut self) -> Result<String, ModelError> {
        if self.command_checkpoint.is_some() {
            return Err(invalid("a command checkpoint is already active"));
        }
        let token = format!("command:{}", self.next_command_token);
        self.next_command_token = self
            .next_command_token
            .checked_add(1)
            .ok_or_else(|| invalid("command checkpoint tokens are exhausted"))?;
        self.command_checkpoint = Some(CommandCheckpoint {
            token: token.clone(),
            state: self.state.clone(),
            revision: self.revision,
            saved_revision: self.saved_revision,
            undo: self.undo.clone(),
            redo: self.redo.clone(),
        });
        Ok(token)
    }

    /// Restores a checkpoint only after computing its scene succeeds.
    /// Cancellation leaves the current definition, history and checkpoint
    /// intact. The token remains valid for failed-commit retries, and
    /// revision numbers allocated since it began are never reused.
    pub fn restore_command(&mut self, token: &str) -> Result<(), ModelError> {
        let checkpoint = self
            .command_checkpoint
            .as_ref()
            .filter(|checkpoint| checkpoint.token == token)
            .ok_or_else(|| invalid("the command checkpoint is not active"))?;
        let state = checkpoint.state.clone();
        let result = self.compute(&state)?;
        let checkpoint = self
            .command_checkpoint
            .as_ref()
            .expect("retained checkpoint");
        self.state = state;
        self.revision = checkpoint.revision;
        self.saved_revision = checkpoint.saved_revision;
        self.undo = checkpoint.undo.clone();
        self.redo = checkpoint.redo.clone();
        self.preview = None;
        self.set_result(result);
        Ok(())
    }

    /// Releases a matching checkpoint without changing the document or
    /// its history. A stale token cannot affect a newer command.
    pub fn discard_command(&mut self, token: &str) -> Result<(), ModelError> {
        if !self
            .command_checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.token == token)
        {
            return Err(invalid("the command checkpoint is not active"));
        }
        self.command_checkpoint = None;
        Ok(())
    }

    /// Restores the state before the last command; returns its label. A
    /// cancelled undo (see [`Document::try_undo`]) gives None.
    pub fn undo(&mut self) -> Option<String> {
        self.try_undo().unwrap_or(None)
    }

    pub fn redo(&mut self) -> Option<String> {
        self.try_redo().unwrap_or(None)
    }

    /// Undo that fails when the monitor cancels it (P7); the steps then
    /// stay where they were. Ok(None) when there is nothing to undo.
    pub fn try_undo(&mut self) -> Result<Option<String>, ModelError> {
        self.step(true)
    }

    pub fn try_redo(&mut self) -> Result<Option<String>, ModelError> {
        self.step(false)
    }

    /// Takes the last undo (or redo) step: computes its state, then makes
    /// it the state and the current state a step on the other stack.
    fn step(&mut self, back: bool) -> Result<Option<String>, ModelError> {
        let from = if back { &mut self.undo } else { &mut self.redo };
        let Some(step) = from.pop() else {
            return Ok(None);
        };
        let result = match self.compute(&step.state) {
            Ok(result) => result,
            Err(error) => {
                let from = if back { &mut self.undo } else { &mut self.redo };
                from.push(step);
                return Err(error);
            }
        };
        // The library files the step's command wrote go back too
        // (mitcad#655); a file that cannot be written keeps the step.
        if let Err(error) = library_items::write_files(&step.files, back) {
            let from = if back { &mut self.undo } else { &mut self.redo };
            from.push(step);
            return Err(error);
        }
        let label = step.label.clone();
        let current = self.restore(step);
        if back {
            self.redo.push(current);
        } else {
            self.undo.push(current);
        }
        self.set_result(result);
        Ok(Some(label))
    }

    /// Makes a step's state the state again, with its revision; returns the
    /// step back to the state it replaced.
    fn restore(&mut self, step: Step) -> Step {
        self.preview = None;
        Step {
            label: step.label,
            state: std::mem::replace(&mut self.state, step.state),
            revision: std::mem::replace(&mut self.revision, step.revision),
            files: step.files,
        }
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|step| step.label.as_str())
    }

    /// The definitions a history command would restore, for file-read
    /// auditing before an external client asks to recompute that state.
    pub fn history_state(&self, back: bool) -> Option<&DocState> {
        (if back { &self.undo } else { &self.redo })
            .last()
            .map(|step| &step.state)
    }

    /// The number of steps that can be undone.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Merges the undo steps after the first `depth` into one step named
    /// `label`, so that a command made of many (an import) undoes at once.
    pub fn merge_undo(&mut self, depth: usize, label: &str) {
        if self.undo.len() <= depth {
            return;
        }
        // The state and its revision stay: only the steps in between go,
        // the library files they wrote joining the first (mitcad#655).
        let files: Vec<library_items::FileWrite> = self.undo[depth..]
            .iter_mut()
            .flat_map(|step| std::mem::take(&mut step.files))
            .collect();
        self.undo.truncate(depth + 1);
        self.undo[depth].label = label.to_owned();
        self.undo[depth].files = files;
        self.redo.clear();
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|step| step.label.as_str())
    }

    // Revisions and the saved state (P8).

    /// The revision of the definition state. A command gives the state a
    /// new one; undo and redo take back the revision of the state they
    /// restore, so the same number means the same definition.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The definition state is not the one last marked saved (or the
    /// document was never saved).
    pub fn is_modified(&self) -> bool {
        self.saved_revision != Some(self.revision)
    }

    /// Marks the definition state as the one in the design file, after
    /// saving or opening it; no undo step.
    pub fn mark_saved(&mut self) {
        self.saved_revision = Some(self.revision);
    }

    /// The steps between the state last marked saved and this one, oldest
    /// first (P12d: the message of a version): the labels of the undo steps
    /// made since, or `Undo <label>` for the steps undone since, when the
    /// saved state is on the undo or redo stack; steps that changed only the
    /// display state (per user, not versioned) are left out. None when the
    /// saved state is on neither (never saved, or its step is gone); empty
    /// when nothing but the display state changed.
    pub fn changes_since_saved(&self) -> Option<Vec<String>> {
        let saved = self.saved_revision?;
        if saved == self.revision {
            return Some(Vec::new());
        }
        // An undo step holds the state before its command; a redo step the
        // state after its command, undone since (the first undone is the
        // lowest). Each step's other side is the next step's, or this one.
        let (steps, undone) = match self.undo.iter().rposition(|step| step.revision == saved) {
            Some(at) => (&self.undo[at..], false),
            None => {
                let at = self.redo.iter().rposition(|step| step.revision == saved)?;
                (&self.redo[at..], true)
            }
        };
        let after = steps
            .iter()
            .skip(1)
            .map(|step| &step.state)
            .chain(std::iter::once(&self.state));
        Some(
            steps
                .iter()
                .zip(after)
                .filter(|(step, after)| !display_only(&step.state, after))
                .map(|(step, _)| {
                    if undone {
                        format!("Undo {}", step.label)
                    } else {
                        step.label.clone()
                    }
                })
                .collect(),
        )
    }

    /// A revision number no state of this document has had.
    fn new_revision(&mut self) -> u64 {
        self.next_revision += 1;
        self.next_revision - 1
    }

    /// The state changed without an undo step (linked components updated
    /// on open).
    fn state_changed(&mut self) {
        self.revision = self.new_revision();
        self.preview = None;
    }

    // Preview.

    /// Evaluates an added feature without committing it. Its results stay
    /// in the cache, so committing the same command next does not
    /// evaluate it again.
    pub fn preview_add(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
    ) -> Result<PreviewReport, ModelError> {
        self.check_profiles(def)?;
        let beside = self.component_parent_beside(self.state.assembly.active);
        let mut state = self.state.clone();
        let added = state.add_feature(def, name)?;
        state.place_made_beside(added.uid, None, beside);
        self.preview_state(state, added.uid)
    }

    /// Evaluates an edited feature without committing it.
    pub fn preview_edit(
        &mut self,
        uid: FeatureUid,
        def: &FeatureDef<ValueInput>,
    ) -> Result<PreviewReport, ModelError> {
        self.check_profiles(def)?;
        let (made, beside) = self.made_and_beside(uid);
        let mut state = self.state.clone();
        state.edit_feature(uid, def)?;
        state.place_made_beside(uid, made, beside);
        // An editor shows the model as the feature finds it, the marker
        // just before the feature: the edit is computed there as a feature
        // added at the marker is, the later ones staying rolled back
        // (mitcad#309).
        if let Some(position) = state.features.iter().position(|f| f.uid == uid) {
            state.marker = state.marker.max(position + 1);
        }
        self.preview_state(state, uid)
    }

    /// Computes a previewed state; a cancelled preview (P7) leaves none.
    fn preview_state(
        &mut self,
        state: DocState,
        uid: FeatureUid,
    ) -> Result<PreviewReport, ModelError> {
        // A preview of what the policy refuses says so at once (mitcad#220).
        if let Err(error) = self.check_policy(&state) {
            self.preview = None;
            return Err(error);
        }
        // The feature's preview-only tool too (mitcad#712).
        self.cache.preview = Some(uid);
        let computed = self.compute(&state);
        self.cache.preview = None;
        let result = match computed {
            Ok(result) => result,
            Err(error) => {
                self.preview = None;
                return Err(error);
            }
        };
        let names = display_names(&state, &result);
        let cut_targets = result
            .result(uid)
            .and_then(|r| r.output.as_ref())
            .and_then(|o| o.cut_targets.clone())
            .map(|mut targets| {
                for row in &mut targets.rows {
                    for body in &mut row.bodies {
                        body.name = names
                            .get(&body.body)
                            .cloned()
                            .unwrap_or_else(|| body.body.to_string());
                    }
                }
                targets
            });
        let current = &self.result.bodies;
        let assembly = state.assembly.at_placements(&result.placements);
        let mut preview_instances = assembly.instances(
            &|o| {
                result
                    .placements
                    .get(&o)
                    .copied()
                    .unwrap_or(crate::transform::Transform::IDENTITY)
            },
            &|c| {
                result
                    .owners
                    .iter()
                    .filter(|(_, owner)| **owner == c)
                    .map(|(b, _)| *b)
                    .collect()
            },
        );
        for instance in &mut preview_instances {
            instance.visible &= state
                .body_attributes
                .get(&instance.body)
                .is_none_or(|a| a.visible);
        }
        let old_instances: BTreeSet<_> = self
            .instances()
            .into_iter()
            .map(|i| (i.path, i.body))
            .collect();
        let new_instances: BTreeSet<_> = preview_instances
            .iter()
            .map(|i| (i.path.clone(), i.body))
            .collect();
        let tool = result
            .result(uid)
            .and_then(|r| r.output.as_ref())
            .is_some_and(|o| o.tool.is_some())
            || result.preview_tool.is_some();
        // The tool's component in each of its occurrences (mitcad#636).
        let tool_component = state.entry(uid).map(|f| f.component).filter(|_| tool);
        let tool_instances = match tool_component {
            Some(component) => assembly
                .instances(
                    &|o| {
                        result
                            .placements
                            .get(&o)
                            .copied()
                            .unwrap_or(crate::transform::Transform::IDENTITY)
                    },
                    &|c| {
                        if c == component {
                            vec![BodyUid::new(uid, 0)]
                        } else {
                            Vec::new()
                        }
                    },
                )
                .into_iter()
                .map(|i| (i.path, i.transform, i.visible))
                .collect(),
            None => Vec::new(),
        };
        let report = PreviewReport {
            sheet_states: crate::sheet::state_query(&result, None),
            uid,
            name: state.entry(uid).map(|f| f.name.clone()).unwrap_or_default(),
            status: result
                .result(uid)
                .map_or(FeatureStatus::RolledBack, |r| r.status.clone()),
            warnings: result
                .result(uid)
                .map(|r| r.warnings().to_vec())
                .unwrap_or_default(),
            elements: result
                .result(uid)
                .and_then(|r| r.output.as_ref())
                .map(|o| o.elements.clone())
                .unwrap_or_default(),
            anchor: result
                .result(uid)
                .and_then(|r| r.output.as_ref())
                .and_then(|o| o.pattern_anchor.clone()),
            sweep_place: result
                .result(uid)
                .and_then(|r| r.output.as_ref())
                .and_then(|o| o.sweep_place),
            move_basis: result
                .result(uid)
                .and_then(|r| r.output.as_ref())
                .and_then(|o| o.move_basis),
            instances: preview_instances
                .into_iter()
                .filter(|i| !old_instances.contains(&(i.path.clone(), i.body)))
                .collect(),
            removed_instances: old_instances.difference(&new_instances).cloned().collect(),
            cut_targets,
            bodies: result
                .bodies
                .iter()
                .map(|(body, shape)| {
                    let changed = current
                        .get(body)
                        .is_none_or(|old| old.version != shape.version);
                    let name = names.get(body).cloned().unwrap_or_else(|| body.to_string());
                    (*body, name, changed)
                })
                .collect(),
            removed: current
                .keys()
                .filter(|body| !result.bodies.contains_key(body))
                .copied()
                .collect(),
            tool,
            placements: self.moved_occurrences(&state, &result),
            tool_instances,
        };
        self.preview = Some(Preview { uid, state, result });
        Ok(report)
    }

    /// The occurrences a previewed state places elsewhere in the design
    /// than the document does now: each path from the root with its new
    /// placement in the design. Occurrences the preview adds are left out.
    fn moved_occurrences(
        &self,
        state: &DocState,
        result: &Recomputed<K::Shape>,
    ) -> Vec<(Vec<OccurrenceUid>, crate::transform::Transform)> {
        use crate::transform::Transform;
        struct Walk<'a, K: Kernel> {
            doc: &'a Document<K>,
            state: &'a DocState,
            placements: &'a BTreeMap<OccurrenceUid, Transform>,
            path: Vec<OccurrenceUid>,
            out: Vec<(Vec<OccurrenceUid>, Transform)>,
        }
        impl<K: Kernel> Walk<'_, K> {
            /// `now` and `before`: where the parent component is placed in
            /// the preview and in the document (none when it is new).
            fn visit(&mut self, parent: ComponentUid, now: Transform, before: Option<Transform>) {
                if self.path.len() > 64 {
                    return;
                }
                let children: Vec<_> = self.state.assembly.children(parent).cloned().collect();
                for o in children {
                    if o.pattern.is_some() && !self.placements.contains_key(&o.uid) {
                        continue;
                    }
                    let placed = self.placements.get(&o.uid).copied().unwrap_or(o.transform);
                    let world = now.after(&placed);
                    let old = before.and_then(|b| {
                        let current = self.doc.state.assembly.occurrence(o.uid)?;
                        (current.parent == parent).then(|| {
                            b.after(&self.doc.placement(o.uid).unwrap_or(current.transform))
                        })
                    });
                    self.path.push(o.uid);
                    if old.is_some_and(|old| !same_placement(&old, &world)) {
                        self.out.push((self.path.clone(), world));
                    }
                    self.visit(o.component, world, old);
                    self.path.pop();
                }
            }
        }
        let mut walk = Walk {
            doc: self,
            state,
            placements: &result.placements,
            path: Vec::new(),
            out: Vec::new(),
        };
        walk.visit(
            ComponentUid::ROOT,
            Transform::IDENTITY,
            Some(Transform::IDENTITY),
        );
        walk.out
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
    }

    /// A body after the previewed command.
    pub fn preview_body(&self, uid: BodyUid) -> Option<&K::Shape> {
        self.preview
            .as_ref()?
            .result
            .bodies
            .get(&uid)
            .map(|b| &b.value)
    }

    /// The tool body of the previewed feature (an extrusion before its
    /// boolean).
    pub fn preview_tool(&self) -> Option<&K::Shape> {
        let preview = self.preview.as_ref()?;
        // A split's or trim's extended tool, made for the preview only
        // (mitcad#712), else the feature's own.
        preview.result.preview_tool.as_ref().or_else(|| {
            preview
                .result
                .result(preview.uid)?
                .output
                .as_ref()?
                .tool
                .as_ref()
        })
    }

    /// The previewed definition state, for queries.
    pub fn preview_feature(&self) -> Option<&FeatureEntry> {
        let preview = self.preview.as_ref()?;
        preview.state.entry(preview.uid)
    }
}

/// Whether two placements are the same within rounding.
fn same_placement(a: &crate::transform::Transform, b: &crate::transform::Transform) -> bool {
    let scale = a
        .translation
        .iter()
        .chain(&b.translation)
        .fold(1.0_f64, |m, v| m.max(v.abs()));
    (0..3).all(|r| (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() <= 1e-9))
        && (0..3).all(|i| (a.translation[i] - b.translation[i]).abs() <= 1e-9 * scale)
}

/// Whether two definition states differ in the display state only (the
/// Origin folder, Isolate), which a project does not version (P12d).
fn display_only(a: &DocState, b: &DocState) -> bool {
    a.display != b.display
        && DocState {
            display: b.display.clone(),
            ..a.clone()
        } == *b
}

/// Display names of every body the timeline creates: the stored name, or
/// the next free `Body<n>` in the order the bodies first appear.
fn display_names<S>(state: &DocState, result: &Recomputed<S>) -> BTreeMap<BodyUid, String> {
    let mut names = state.body_names.clone();
    let mut used: BTreeSet<String> = names.values().cloned().collect();
    let mut next = 1;
    for feature in &result.results {
        let Some(output) = &feature.output else {
            continue;
        };
        for change in output.changes.heads() {
            let crate::recompute::ChangeHead::Set(uid, _) = change else {
                continue;
            };
            if names.contains_key(uid) {
                continue;
            }
            while used.contains(&format!("Body{next}")) {
                next += 1;
            }
            let name = format!("Body{next}");
            used.insert(name.clone());
            names.insert(*uid, name);
        }
    }
    names
}

/// How `Document::push_projections` adds its curves: the source's body
/// (model geometry), its link to another component, whether the
/// projection is linked or a section, and the edit's undo label.
struct Projecting<'a> {
    body: Option<BodyUid>,
    link: Option<crate::links::OccurrenceLink>,
    linked: bool,
    section: bool,
    label: &'a str,
}

/// Where a projection goes and where its source is
/// (`Document::projection_target`).
struct ProjectionTarget {
    /// The sketch's place in the timeline.
    position: usize,
    frame: SketchFrame,
    /// The component the source geometry is in.
    from: ComponentUid,
    link: Option<crate::links::OccurrenceLink>,
    /// From the source's component's coordinates into the sketch's.
    to_sketch: Option<crate::transform::Transform>,
}
