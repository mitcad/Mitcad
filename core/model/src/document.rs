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
//! the state last saved (P8) without comparing project files.
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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use analyses::{Analysis, AnalysisDef, SectionAnalysis, SectionPlane};
pub use appearances::DeletedAppearance;
pub use bodies::{BodyAttributes, FaceAppearance};
pub use components::{InsertOptions, InstanceView};
pub use datums::DatumView;
pub use display::{DisplayState, Isolated};
pub use groups::TimelineGroup;
pub use views::NamedView;

use crate::appearance::Appearance;
use crate::assembly::Assembly;
use crate::expr::{
    LengthUnit, ParamSpec, Unit, is_valid_name, value_to_expression, with_decimal_points,
};
use crate::features::{
    CheckContext, FeatureDef, FeatureEntry, HelixConstruction, SketchDef, SketchPlane, ValueInput,
    slot_label,
};
use crate::ids::{BodyUid, ComponentUid, EntityUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::monitor::{Cancelled, RecomputeMonitor};
use crate::parameters::{
    ParamId, ParameterError, Parameters, describe, expression_dims, fits_slot, slot_unit, unit_for,
};
use crate::profile::{ProfileRegion, SketchFrame};
use crate::recompute::{Cache, FeatureStatus, Recomputed, def_fingerprint, recompute};
use crate::render_settings::RenderSettings;
use crate::sketch::edit::{EditReport, SketchEdit};
use crate::store::{self, PersistReport, ResultStore};
use crate::topo::RegionKey;
use crate::topo::SegmentKey;

/// Rejected command; the document is left unchanged.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    NoSuchFeature(FeatureUid),
    /// A reference or a value the command cannot accept.
    Invalid(String),
    /// Deleting the feature would break the features that use it.
    Dependents {
        feature: String,
        dependents: Vec<String>,
    },
    Parameter(ParameterError),
    /// The computation was cancelled through the monitor (P7).
    Cancelled,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchFeature(uid) => write!(f, "feature {uid} does not exist"),
            Self::Invalid(message) => f.write_str(message),
            Self::Dependents {
                feature,
                dependents,
            } => write!(
                f,
                "{feature} is used by {}; delete them too, or change them first",
                dependents.join(", ")
            ),
            Self::Parameter(error) => error.fmt(f),
            Self::Cancelled => Cancelled.fmt(f),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<ParameterError> for ModelError {
    fn from(error: ParameterError) -> Self {
        Self::Parameter(error)
    }
}

impl From<Cancelled> for ModelError {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

fn invalid(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

/// The definition state: everything a project file saves.
#[derive(Debug, Clone, PartialEq)]
pub struct DocState {
    pub(crate) parameters: Parameters,
    /// In timeline order.
    pub(crate) features: Vec<Arc<FeatureEntry>>,
    /// The number of features before the timeline marker; features after it
    /// are rolled back (not evaluated), and new features go in at it.
    pub(crate) marker: usize,
    /// Display names given to bodies, by the commands or the user.
    pub(crate) body_names: BTreeMap<BodyUid, String>,
    /// Visibility, material and appearance of bodies; only those that
    /// differ from the default are listed.
    pub(crate) body_attributes: BTreeMap<BodyUid, BodyAttributes>,
    /// The number of the next feature uid.
    pub(crate) next_uid: u64,
    /// Components, occurrences and the active component (F6).
    pub(crate) assembly: Assembly,
    /// Light bulbs of sketches and construction features set by the user
    /// (browser, U3) where they differ from the default (a sketch hidden
    /// while a feature uses it, construction geometry shown; see
    /// `document/browser.rs`).
    pub(crate) feature_visibility: BTreeMap<FeatureUid, bool>,
    /// Cameras saved by name (U5), in the order they were added.
    pub(crate) named_views: Vec<NamedView>,
    /// The Origin folder's light bulb and Isolate (P9).
    pub(crate) display: DisplayState,
    /// Favourite parameters (P9: Change Parameters' star).
    pub(crate) favorites: BTreeSet<ParamId>,
    /// Timeline groups (P9), in the order they were made.
    pub(crate) groups: Vec<TimelineGroup>,
    /// Analyses kept in the document (mitcad#41), in the order they were
    /// added.
    pub(crate) analyses: Vec<Analysis>,
    /// The document's own appearances (mitcad#46), in the order they were
    /// made; the library's are not listed.
    pub(crate) appearances: Vec<Appearance>,
    /// How the rendered view lights and shows the design (mitcad#47).
    pub(crate) render: RenderSettings,
    /// The configuration table (mitcad#64): sizes of the design.
    pub(crate) configurations: crate::configurations::Configurations,
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
        }
    }
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
    pub(crate) fn position(&self, uid: FeatureUid) -> Option<usize> {
        self.features.iter().position(|f| f.uid == uid)
    }

    pub(crate) fn entry(&self, uid: FeatureUid) -> Option<&FeatureEntry> {
        self.features.iter().find(|f| f.uid == uid).map(|f| &**f)
    }

    fn require(&self, uid: FeatureUid) -> Result<usize, ModelError> {
        self.position(uid).ok_or(ModelError::NoSuchFeature(uid))
    }

    /// Checks a feature placed at `position` against the features before it.
    pub(crate) fn check_feature(
        &self,
        position: usize,
        entry: &FeatureEntry,
    ) -> Result<(), String> {
        let ctx = CheckContext {
            uid: entry.uid,
            earlier: &self.features[..position],
            all: &self.features,
        };
        entry.def.info().check(&ctx)?;
        check_components(&self.assembly, &self.features, entry)
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
        let params = &mut self.parameters;
        let mut resolved = def.map_params(&mut |slot, value| -> Result<ParamId, ModelError> {
            let comment = format!("{owner_name} {}", slot_label(slot));
            let (id, new) =
                resolve_value(params, slot, value, owner, &comment, old_slots.get(slot))?;
            created.extend(new);
            Ok(id)
        })?;
        // A growing helix without a construction keeps the one it had, else
        // it is Mitcad's (mitcad#83).
        if let FeatureDef::Helix(helix) = &mut resolved {
            let had = match old {
                Some(FeatureDef::Helix(old)) => old.construction,
                _ => None,
            };
            helix.settle_construction(had.unwrap_or(HelixConstruction::Mitcad));
        }
        Ok((resolved, created))
    }

    /// The features that refer to any of `of`, directly or through each
    /// other, in timeline order.
    pub(crate) fn dependents(&self, of: &BTreeSet<FeatureUid>) -> Vec<FeatureUid> {
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
    pub(crate) fn release_params(&mut self, owner: FeatureUid) {
        let used = used_params(&self.features);
        let exists = self.position(owner).is_some();
        self.parameters
            .release(owner, exists, &|id| used.contains(&id));
    }

    /// Refuses a unit for a parameter that a feature uses as another kind
    /// of value (an angle for a length).
    fn check_unit_change(&self, id: ParamId, unit: Unit) -> Result<(), ModelError> {
        for entry in &self.features {
            let mut wrong = None;
            let _ = entry.def.map_params(&mut |slot, p: &ParamId| {
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
        self.features
            .iter()
            .filter(|f| f.def.params().contains(&id))
            .map(|f| f.name.clone())
            .collect()
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
    pub(crate) fn add_feature_in(
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
                let made = self.assembly.create(None, Some(uid));
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
                self.assembly.remove(made);
            }
            _ => {}
        }
        Ok(())
    }

    /// Removes features with the parameters they owned, their bodies'
    /// names and attributes and the components they made (when nothing
    /// else is in them). Returns the removed features in timeline order.
    pub(crate) fn remove_features(&mut self, removed: &BTreeSet<FeatureUid>) -> Vec<FeatureUid> {
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
        self.release_components(removed);
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
            if self.features.iter().any(|f| f.component == c)
                || self.assembly.children(c).next().is_some()
            {
                if let Some(def) = self.assembly.component_mut(c) {
                    def.created_by = None;
                }
            } else {
                self.assembly.remove(c);
            }
        }
    }

    pub(crate) fn edit_feature(
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
    entry: &FeatureEntry,
) -> Result<(), String> {
    if !assembly.exists(entry.component) {
        return Err(format!("component {} does not exist", entry.component));
    }
    // Joints use geometry of the components their occurrences place
    // (mitcad#55).
    if let Some(result) = crate::joints::check_components(assembly, features, entry) {
        return result;
    }
    for uid in entry.def.info().references().features {
        let Some(other) = features.iter().find(|f| f.uid == uid) else {
            continue;
        };
        let placed = crate::joints::placed_geometry(&other.def);
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

/// Resolves one command value (see [`DocState::resolve`]): an existing
/// parameter by name, or a number or an expression for the slot's own
/// parameter (`old`, when the feature owns it) or a new dimension
/// parameter. Numbers are millimetres or radians; new parameters show
/// lengths in millimetres and angles in degrees (see [`slot_unit`]).
pub(crate) fn resolve_value(
    params: &mut Parameters,
    slot: &str,
    value: &ValueInput,
    owner: FeatureUid,
    comment: &str,
    old: Option<&ParamId>,
) -> Result<(ParamId, Option<String>), ModelError> {
    let unit = match slot_unit(slot) {
        Unit::MM => Unit::of_length(params.context().default_length_unit),
        unit => unit,
    };
    let own = old.copied().filter(|id| params.owner(*id) == Some(owner));
    let in_slot = |e: ParameterError| invalid(format!("{slot}: {e}"));
    let expression = match value {
        ValueInput::Number(number) => {
            if !number.is_finite() {
                return Err(invalid(format!(
                    "{slot}: the value must be a finite number, got {number}"
                )));
            }
            if let Some(id) = own {
                params.set_value(id, *number).map_err(in_slot)?;
                return Ok((id, None));
            }
            value_to_expression(*number, unit)
        }
        ValueInput::Name(text) => {
            if let Some(id) = params.find(text.trim()) {
                let param = params.get(id).expect("found");
                if !fits_slot(param.unit(), unit) {
                    return Err(invalid(format!(
                        "{slot}: parameter '{}' is {}, not {}",
                        param.name(),
                        describe(param.unit()),
                        describe(unit)
                    )));
                }
                return Ok((id, None));
            }
            if is_valid_name(text.trim()) {
                return Err(invalid(format!(
                    "{slot}: parameter '{}' does not exist",
                    text.trim()
                )));
            }
            if let Some(id) = own {
                params.set_expression(id, text).map_err(in_slot)?;
                return Ok((id, None));
            }
            text.clone()
        }
    };
    let id = params
        .add_dimension(&expression, unit, comment, Some(owner))
        .map_err(in_slot)?;
    Ok((id, Some(params.name(id))))
}

/// A feature in the timeline with its status after the last recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineItem<'a> {
    pub entry: &'a FeatureEntry,
    /// None until the document has been recomputed.
    pub status: Option<&'a FeatureStatus>,
    /// What a feature that succeeded built with caveats (P9).
    pub warnings: &'a [String],
}

impl TimelineItem<'_> {
    /// `ok`, `warning` (succeeded with warnings), `error`, `suppressed`,
    /// `rolled_back`, or `pending` before the first recompute.
    pub fn status_text(&self) -> &'static str {
        status_text(self.status, self.warnings)
    }

    /// The error of a failed feature, or its warnings.
    pub fn message(&self) -> Option<String> {
        status_message(self.status, self.warnings)
    }
}

/// A feature's status as the queries give it: `warning` for one that
/// succeeded with warnings.
pub(crate) fn status_text(status: Option<&FeatureStatus>, warnings: &[String]) -> &'static str {
    match status {
        Some(FeatureStatus::Ok) if !warnings.is_empty() => "warning",
        Some(status) => status.as_str(),
        None => "pending",
    }
}

/// A failed feature's error, or a feature's warnings joined.
pub(crate) fn status_message(
    status: Option<&FeatureStatus>,
    warnings: &[String],
) -> Option<String> {
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
    pub uid: FeatureUid,
    pub name: String,
    pub status: FeatureStatus,
    /// What the feature built with caveats (P9).
    pub warnings: Vec<String>,
    /// A pattern's elements by number, suppressed ones too (P9).
    pub elements: Vec<crate::transform::Transform>,
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
}

struct Preview<S> {
    uid: FeatureUid,
    state: DocState,
    result: Recomputed<S>,
}

/// The versions of each feature's result and of each body (P7d tests).
#[cfg(test)]
pub(crate) type ResultVersions = (Vec<(FeatureUid, Option<u128>)>, Vec<(BodyUid, u128)>);

/// An undo or redo step: the command's label and the state on the other
/// side of it, with that state's revision.
struct Step {
    label: String,
    state: DocState,
    revision: u64,
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
    cache: Cache<K::Shape>,
    result: Recomputed<K::Shape>,
    /// Recomputes so far, so callers can tell whether a command recomputed;
    /// a cancelled one is not counted.
    recomputes: u64,
    preview: Option<Preview<K::Shape>>,
    /// What loading the project file changed, such as renamed parameters.
    warnings: Vec<String>,
    /// Where recomputes report progress and find cancel requests (P7).
    monitor: Option<Arc<RecomputeMonitor>>,
    /// Reads library components (mitcad#64); the process's when None.
    resolver: Option<Arc<dyn crate::library::LinkResolver>>,
}

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
    pub(crate) fn from_state(kernel: K, state: DocState) -> Self {
        Self {
            kernel,
            state,
            revision: 0,
            next_revision: 1,
            saved_revision: None,
            undo: Vec::new(),
            redo: Vec::new(),
            cache: Cache::default(),
            result: Recomputed::default(),
            recomputes: 0,
            preview: None,
            warnings: Vec::new(),
            monitor: None,
            resolver: None,
        }
    }

    pub(crate) fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }

    /// What loading the project file changed (renamed parameters, ...).
    pub fn load_warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn kernel(&self) -> &K {
        &self.kernel
    }

    pub(crate) fn state(&self) -> &DocState {
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
        self.set_result(result);
        Ok(self.stats())
    }

    /// Evaluates a definition state with the document's cache, reporting
    /// to its monitor; nothing of the document but the cache changes.
    fn compute(&mut self, state: &DocState) -> Result<Recomputed<K::Shape>, ModelError> {
        let monitor = self.monitor.as_ref();
        Ok(recompute(&self.kernel, state, &mut self.cache, monitor)?)
    }

    /// Takes the results of a recompute of the state.
    fn set_result(&mut self, result: Recomputed<K::Shape>) {
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
    pub(crate) fn link_resolver(&self) -> Option<Arc<dyn crate::library::LinkResolver>> {
        self.resolver.clone().or_else(crate::library::link_resolver)
    }

    /// The number of recomputes so far.
    pub fn recompute_count(&self) -> u64 {
        self.recomputes
    }

    /// The versions of the last recompute's results and bodies (P7d tests).
    #[cfg(test)]
    pub(crate) fn versions(&self) -> ResultVersions {
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

    /// The memory the cached results take, estimated in bytes.
    pub fn memory_cache_bytes(&self) -> u64 {
        self.cache.bytes()
    }

    /// Drops the cached results that neither the document's results nor
    /// its preview hold; returns their number and bytes.
    pub fn clear_memory_cache(&mut self) -> (u64, u64) {
        self.cache.clear()
    }

    /// The cache and the last recompute, for diagnostics (`api/cache.rs`).
    pub(crate) fn cache(&self) -> &Cache<K::Shape> {
        &self.cache
    }

    pub(crate) fn last_result(&self) -> &Recomputed<K::Shape> {
        &self.result
    }

    /// Writes to the result store the results of the last recompute that
    /// are not there yet: the features that succeeded, whose evaluation
    /// took at least `min_time` and whose results the store takes (see
    /// `store.rs`). What a preview computed is not written.
    pub fn persist_results(&mut self, min_time: Duration) -> PersistReport {
        let start = Instant::now();
        let mut report = PersistReport::default();
        let Some(store) = self.cache.store.as_ref() else {
            return report;
        };
        let params = &self.state.parameters;
        for result in &self.result.results {
            let Some(output) = &result.output else {
                continue;
            };
            if output.persisted.get() || output.time < min_time || output.result.is_err() {
                continue;
            }
            let Some(entry) = self.state.entry(result.uid) else {
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
            match store.write(&self.kernel, entry, output, params) {
                Ok((bytes, shapes)) => {
                    output.persisted.set(true);
                    report.results += 1;
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
        self.result.result(uid).map(|r| &r.status)
    }

    /// The warnings of a feature that succeeded in the last recompute (P9:
    /// a fillet built 0.1 % smaller than asked).
    pub fn warnings(&self, uid: FeatureUid) -> &[String] {
        self.result.result(uid).map_or(&[], |r| r.warnings())
    }

    pub fn timeline(&self) -> Vec<TimelineItem<'_>> {
        self.features()
            .map(|entry| TimelineItem {
                entry,
                status: self.status(entry.uid),
                warnings: self.warnings(entry.uid),
            })
            .collect()
    }

    /// Bodies at the timeline marker, by id.
    /// Bodies at the timeline marker, by id, of every component (each in
    /// its component's coordinates; [`Document::instances`] places them).
    pub fn bodies(&self) -> Vec<BodyView<'_, K::Shape>> {
        let names = display_names(&self.state, &self.result);
        self.result
            .bodies
            .iter()
            .map(|(uid, shape)| BodyView {
                uid: *uid,
                name: names.get(uid).cloned().unwrap_or_else(|| uid.to_string()),
                shape: &shape.value,
            })
            .collect()
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

    /// An evaluated profile region with its sketch frame.
    pub fn profile_region(
        &self,
        sketch: FeatureUid,
        region: &RegionKey,
    ) -> Option<(SketchFrame, &ProfileRegion)> {
        let output = &self.result.sketches.get(&sketch)?.value;
        Some((output.frame, output.region(region)?))
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

    // Commands.

    /// Runs a command on a copy of the state; on success the copy becomes
    /// the state and the old one an undo step.
    pub(crate) fn apply<T>(
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
        let mut next = self.state.clone();
        let (label, value, recompute) = edit(&mut next)?;
        // Timeline groups stay runs whatever the edit did (P9).
        next.normalize_groups();
        // A sketch that got its first consumer hides, one that lost its
        // last shows (mitcad#7).
        next.follow_sketch_use(&self.state);
        let result = if recompute {
            Some(self.compute(&next)?)
        } else {
            None
        };
        self.commit(label, next);
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
        self.undo.push(Step {
            label,
            state: std::mem::replace(&mut self.state, next),
            revision: std::mem::replace(&mut self.revision, revision),
        });
        self.redo.clear();
        self.preview = None;
    }

    fn name_new_bodies(&mut self) {
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
        self.apply(|state| {
            let added = state.add_feature_in(def, name, component)?;
            Ok((format!("Add {}", added.name), added))
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
        self.apply(|state| {
            let created = state.edit_feature(uid, def)?;
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

    /// Suppressed features are skipped by recompute.
    pub fn set_suppressed(&mut self, uid: FeatureUid, suppressed: bool) -> Result<(), ModelError> {
        let position = self.state.require(uid)?;
        if self.state.features[position].suppressed == suppressed {
            return Ok(());
        }
        self.apply(|state| {
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
    /// document's length unit, degrees, or none.
    pub fn add_parameter_expression(
        &mut self,
        name: &str,
        expression: &str,
        unit: Option<Unit>,
        comment: &str,
    ) -> Result<(), ModelError> {
        self.apply(|state| {
            let unit = match unit {
                Some(unit) => unit,
                None => {
                    let dims = expression_dims(&state.parameters, expression)?;
                    unit_for(dims, &state.parameters.context())
                        .ok_or_else(|| invalid(format!("'{expression}' has no unit; give one")))?
                }
            };
            let spec = ParamSpec::user(name, expression, unit).with_comment(comment);
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
        let id = self.param_id(name)?;
        if self.state.parameters.value(id) == Some(value) {
            return Ok(());
        }
        self.apply(|state| {
            state.parameters.set_value(id, value)?;
            Ok((format!("Change {name}"), ()))
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
            let users = state.param_users(id);
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
        let bodies = self.bodies_before(position);
        // Only the sketch's component's bodies are in its coordinates.
        let component = self.state.features[position].component;
        let mut curves = Vec::new();
        for (uid, shape) in &bodies {
            if body.is_some_and(|b| b != *uid)
                || self.body_component(*uid).is_some_and(|c| c != component)
            {
                continue;
            }
            curves = self
                .kernel
                .curves_of(shape, source)
                .map_err(|e| invalid(format!("projection of {source}: {e}")))?;
            if !curves.is_empty() {
                break;
            }
        }
        if curves.is_empty() {
            return Err(invalid(format!(
                "nothing named {source} exists before the sketch"
            )));
        }
        let projected = crate::sketch::project::project(&curves, &frame);
        let source = source.clone();
        let ((), report) = self.edit_sketch(sketch, "Project to {sketch}", |edit| {
            let entities = crate::sketch::project::entities(&projected, &mut || edit.id());
            let refs = crate::sketch::project::refs(&entities);
            for entity in entities {
                edit.push(entity);
            }
            if linked {
                edit.def.projections.push(crate::sketch::Projection {
                    source,
                    body,
                    entities: refs,
                });
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
        }
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|step| step.label.as_str())
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
        // The state and its revision stay: only the steps in between go.
        self.undo.truncate(depth + 1);
        self.undo[depth].label = label.to_owned();
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

    /// Marks the definition state as the one in the project file, after
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
        let mut state = self.state.clone();
        let added = state.add_feature(def, name)?;
        self.preview_state(state, added.uid)
    }

    /// Evaluates an edited feature without committing it.
    pub fn preview_edit(
        &mut self,
        uid: FeatureUid,
        def: &FeatureDef<ValueInput>,
    ) -> Result<PreviewReport, ModelError> {
        self.check_profiles(def)?;
        let mut state = self.state.clone();
        state.edit_feature(uid, def)?;
        self.preview_state(state, uid)
    }

    /// Computes a previewed state; a cancelled preview (P7) leaves none.
    fn preview_state(
        &mut self,
        state: DocState,
        uid: FeatureUid,
    ) -> Result<PreviewReport, ModelError> {
        let result = match self.compute(&state) {
            Ok(result) => result,
            Err(error) => {
                self.preview = None;
                return Err(error);
            }
        };
        let names = display_names(&state, &result);
        let current = &self.result.bodies;
        let report = PreviewReport {
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
            tool: result
                .result(uid)
                .and_then(|r| r.output.as_ref())
                .is_some_and(|o| o.tool.is_some()),
            placements: self.moved_occurrences(&state, &result),
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
        preview
            .result
            .result(preview.uid)?
            .output
            .as_ref()?
            .tool
            .as_ref()
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
        for change in &output.changes {
            let crate::recompute::Change::Set(uid, _) = change else {
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
