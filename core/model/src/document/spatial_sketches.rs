// SPDX-License-Identifier: MIT
//! 3D sketches in the document (mitcad#1521): starting one, reading what
//! its last evaluation gave, and editing it in one step.
//!
//! An edit ([`Document::edit_spatial_sketch`]) changes a copy of the whole
//! document state: the sketch's definition, the parameters its new driving
//! dimensions get and the ids it allocates. That copy is evaluated up to
//! and with the sketch, with its support and its attachments' sources
//! found where they are now, and solved from the edited positions; the
//! solved positions are stored as the start of the next solve. Only a
//! sketch that solves is taken, as one undo step; anything else leaves the
//! document as it was (the solver's restoring of one failed part of the
//! sketch alone would not do: a segment, its end point, its attachments,
//! its axis and its dimensions are one edit). A refusal carries the
//! failed solve's relations and entities ([`SpatialEditError::failure`]).
//!
//! A drag pulls points toward targets with temporary soft goals: the
//! relations, attachments and dimensions hold, and no relation is stored.
//! [`Document::preview_spatial_drag`] solves one without changing the
//! document; [`Document::drag_spatial_sketch`] takes it as an undo step.
//!
//! An upstream change that breaks the sketch (a lost source vertex, a
//! moved one that no longer agrees with a dimension) is no edit: the
//! recompute fails the feature, keeps its definition, and its output
//! ([`Document::spatial_sketch`]) has the failure and no geometry.

use std::fmt;
use std::sync::Arc;

use super::{Added, DocState, Document, ModelError, invalid};
use crate::FeatureStatus;
use crate::features::path::take_fillet_arcs;
use crate::features::spatial_sketch::{
    SpatialFailure, SpatialSketchDef, SpatialSketchOutput, SpatialSolved,
};
use crate::features::{FeatureDef, FeatureEntry, SketchPlane};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::Parameters;
use crate::sketch::spatial::SpatialTangentPull;
use crate::sketch::spatial_edit::{SpatialEdit, SpatialEditReport, SpatialGoals};

/// A refused edit of a 3D sketch: the error, and the failed solve's
/// relations and entities when the solve refused it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialEditError {
    pub error: Box<ModelError>,
    pub failure: Option<Box<SpatialFailure>>,
}

impl From<ModelError> for SpatialEditError {
    fn from(error: ModelError) -> Self {
        Self {
            error: Box::new(error),
            failure: None,
        }
    }
}

impl fmt::Display for SpatialEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for SpatialEditError {}

/// The refusal of `name`'s edit by its solve.
fn refused(name: &str, failure: SpatialFailure) -> SpatialEditError {
    SpatialEditError {
        error: Box::new(invalid(format!("{name}: {failure}"))),
        failure: Some(Box::new(failure)),
    }
}

/// Puts `def` as the definition of the 3D sketch at `position` of `state`,
/// checked against the features before it, and the features after it
/// against it.
fn place(state: &mut DocState, position: usize, def: SpatialSketchDef) -> Result<(), ModelError> {
    let entry = state.features[position].clone();
    let edited = FeatureEntry {
        def: FeatureDef::SpatialSketch(def),
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
    Ok(())
}

/// The definition with the solved positions (normals, radii, tangency
/// senses) as its stored ones.
fn store(def: &mut SpatialSketchDef, solved: &SpatialSolved) {
    def.store_solved(solved);
}

/// `def` dragged toward `goals` (points, and end tangents of fit-point
/// splines) with the targets and parameters of an evaluation (`output`).
fn dragged(
    name: &str,
    def: &SpatialSketchDef,
    params: &Parameters,
    output: &SpatialSketchOutput,
    goals: &SpatialGoals,
) -> Result<SpatialSolved, SpatialEditError> {
    if let Some(failure) = &output.failure {
        return Err(refused(name, failure.clone()));
    }
    let mut value = |id| {
        params
            .value(id)
            .ok_or_else(|| format!("parameter {} has no value", params.name(id)))
    };
    def.build_with(&mut value, &output.inputs())
        .and_then(|mut model| model.drag_with(&goals.points, &goals.tangents))
        .map_err(|failure| refused(name, failure))
}

impl<K: Kernel> Document<K> {
    /// Starts an empty 3D sketch on `support` (an origin plane, a
    /// construction plane or a planar face) at the marker, in the active
    /// component.
    pub fn add_spatial_sketch(&mut self, support: SketchPlane) -> Result<Added, ModelError> {
        self.add_feature(
            &FeatureDef::SpatialSketch(SpatialSketchDef::new(support)),
            None,
        )
    }

    /// A 3D sketch's stored definition.
    pub fn spatial_sketch_def(&self, uid: FeatureUid) -> Option<&SpatialSketchDef> {
        match &self.state.entry(uid)?.def {
            FeatureDef::SpatialSketch(def) => Some(def),
            _ => None,
        }
    }

    /// What the last recompute gave of a 3D sketch: its frame, its solved
    /// points and lines and their status, or why it failed (then without
    /// geometry). None when it was not evaluated (suppressed, rolled back).
    pub fn spatial_sketch(&self, uid: FeatureUid) -> Option<&SpatialSketchOutput> {
        self.feature_result(uid)?
            .output
            .as_deref()?
            .spatial
            .as_deref()
    }

    /// The position and entry of a 3D sketch that may be edited.
    fn spatial_entry(&self, uid: FeatureUid) -> Result<(usize, Arc<FeatureEntry>), ModelError> {
        let position = self.state.require(uid)?;
        let entry = self.state.features[position].clone();
        self.state.editable(entry.component)?;
        if !matches!(entry.def, FeatureDef::SpatialSketch(_)) {
            // The name for a person, the uid for agents (mitcad#1644).
            return Err(invalid(crate::notice::said(
                format!("{} is not a 3D sketch", entry.name),
                format!("{} ({uid}) is not a 3D sketch", entry.name),
            )));
        }
        Ok((position, entry))
    }

    /// Evaluates `state` up to and with the 3D sketch at `position` (a
    /// suppressed one as if it were not): what it gave. The document
    /// changes in nothing but its cache.
    pub(super) fn probe_spatial(
        &mut self,
        state: &DocState,
        position: usize,
    ) -> Result<Arc<SpatialSketchOutput>, SpatialEditError> {
        let mut probe = state.clone();
        probe.marker = position + 1;
        let entry = probe.features[position].clone();
        if entry.suppressed {
            probe.features[position] = Arc::new(FeatureEntry {
                suppressed: false,
                ..(*entry).clone()
            });
        }
        let result = self.compute_after(&probe)?;
        let found = result
            .results
            .get(position)
            .filter(|r| r.uid == entry.uid)
            .or_else(|| result.result(entry.uid));
        if let Some(output) = found
            .and_then(|r| r.output.as_ref())
            .and_then(|o| o.spatial.clone())
        {
            return Ok(output);
        }
        let message = match found.map(|r| &r.status) {
            Some(FeatureStatus::Failed(message)) => format!("{}: {message}", entry.name),
            _ => format!("{} was not evaluated", entry.name),
        };
        Err(invalid(message).into())
    }

    /// Takes `def` as the definition of the 3D sketch at `position` of
    /// `next`, pulled toward `goals` first when there are any, solved with
    /// its support and sources, and stores the solved positions; refused
    /// when it does not solve.
    pub(super) fn settle_spatial(
        &mut self,
        next: &mut DocState,
        position: usize,
        mut def: SpatialSketchDef,
        goals: &SpatialGoals,
    ) -> Result<SpatialSolved, SpatialEditError> {
        let name = next.features[position].name.clone();
        place(next, position, def.clone())?;
        if !goals.is_empty() {
            let output = self.probe_spatial(next, position)?;
            let solved = dragged(&name, &def, &next.parameters, &output, goals)?;
            store(&mut def, &solved);
            place(next, position, def.clone())?;
        }
        let output = self.probe_spatial(next, position)?;
        if let Some(failure) = &output.failure {
            return Err(refused(&name, failure.clone()));
        }
        let solved = output
            .solved
            .clone()
            .ok_or_else(|| invalid(format!("{name} did not solve")))?;
        store(&mut def, &solved);
        place(next, position, def)?;
        Ok(solved)
    }

    /// Edits a 3D sketch (see [`SpatialEdit`]): `edit` changes a copy of
    /// its definition and of the parameters; the sketch is then solved
    /// with its support and sources where they are now, and the solved
    /// positions stored. The edit is one undo step, `what` naming it (`Add
    /// Line` gives `Add Line in 3DSketch1`); one that does not solve
    /// changes nothing (definition, parameters, ids, history) and says why.
    pub fn edit_spatial_sketch<T>(
        &mut self,
        sketch: FeatureUid,
        what: &str,
        edit: impl FnOnce(&mut SpatialEdit<'_>) -> Result<T, String>,
    ) -> Result<(T, SpatialEditReport), SpatialEditError> {
        let (position, entry) = self.spatial_entry(sketch)?;
        let FeatureDef::SpatialSketch(def) = &entry.def else {
            unreachable!("spatial_entry gives 3D sketches");
        };
        let mut next = self.state.clone();
        let (value, def, mut report, goals) = {
            let mut editor =
                SpatialEdit::new(def.clone(), &mut next.parameters, sketch, &entry.name);
            let value = edit(&mut editor).map_err(invalid)?;
            let (def, report, goals) = editor.finish().map_err(invalid)?;
            (value, def, report, goals)
        };
        // A path that lists both lines of a fillet takes its arc in the same
        // step (mitcad#1548).
        if !report.rounded.is_empty() {
            for entry in &mut next.features {
                if matches!(entry.def, FeatureDef::Pipe(_) | FeatureDef::Sweep(_)) {
                    let mut def = entry.def.clone();
                    if take_fillet_arcs(&mut def, sketch, &report.rounded) {
                        Arc::make_mut(entry).def = def;
                    }
                }
            }
        }
        let solved = self.settle_spatial(&mut next, position, def, &goals)?;
        report.status = solved.status;
        next.release_params(sketch);
        let label = format!("{what} in {}", entry.name);
        self.apply_checked(
            move |state| {
                *state = next;
                Ok((label, (value, report)))
            },
            move |_, result| match result.result(sketch).map(|r| &r.status) {
                Some(FeatureStatus::Failed(message)) => Err(invalid(message.clone())),
                _ => Ok(()),
            },
        )
        .map_err(SpatialEditError::from)
    }

    /// Pulls points of a 3D sketch toward local targets as far as its
    /// relations allow (temporary soft goals) and takes where they went as
    /// one undo step; no relation is added.
    pub fn drag_spatial_sketch(
        &mut self,
        sketch: FeatureUid,
        goals: &[(EntityUid, [f64; 3])],
    ) -> Result<SpatialEditReport, SpatialEditError> {
        self.drag_spatial_sketch_with(sketch, goals, &[])
    }

    /// [`Document::drag_spatial_sketch`] with end tangents of fit-point
    /// splines pulled too (mitcad#1526: a tangent handle's drag); the
    /// tangents reached are stored with the spline.
    pub fn drag_spatial_sketch_with(
        &mut self,
        sketch: FeatureUid,
        goals: &[(EntityUid, [f64; 3])],
        tangents: &[SpatialTangentPull],
    ) -> Result<SpatialEditReport, SpatialEditError> {
        self.edit_spatial_sketch(sketch, "Drag", |edit| {
            edit.drag(goals)?;
            for pull in tangents {
                edit.drag_tangent(pull.curve, pull.end, pull.tangent)?;
            }
            Ok(())
        })
        .map(|((), report)| report)
    }

    /// Where a drag of a 3D sketch would take its points (local
    /// coordinates), without changing the document: a cancelled drag leaves
    /// no trace.
    pub fn preview_spatial_drag(
        &mut self,
        sketch: FeatureUid,
        goals: &[(EntityUid, [f64; 3])],
    ) -> Result<SpatialSolved, SpatialEditError> {
        self.preview_spatial_drag_with(sketch, goals, &[])
    }

    /// [`Document::preview_spatial_drag`] with end tangents pulled too.
    pub fn preview_spatial_drag_with(
        &mut self,
        sketch: FeatureUid,
        goals: &[(EntityUid, [f64; 3])],
        tangents: &[SpatialTangentPull],
    ) -> Result<SpatialSolved, SpatialEditError> {
        let (position, entry) = self.spatial_entry(sketch)?;
        let FeatureDef::SpatialSketch(def) = &entry.def else {
            unreachable!("spatial_entry gives 3D sketches");
        };
        for pull in tangents {
            def.tangent_handle(pull.curve, pull.end)
                .map_err(|e| invalid(format!("{}: {e}", entry.name)))?;
        }
        let state = self.state.clone();
        let output = self.probe_spatial(&state, position)?;
        let goals = SpatialGoals {
            points: goals.to_vec(),
            tangents: tangents.to_vec(),
        };
        dragged(&entry.name, def, &state.parameters, &output, &goals)
    }
}
