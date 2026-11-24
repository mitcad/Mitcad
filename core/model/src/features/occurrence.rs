// SPDX-License-Identifier: MIT
//! Timeline features of components and occurrences (F6):
//!
//! - `component_from_bodies`: Component from Bodies. The bodies
//!   move into a new component placed where they are (the identity in the
//!   feature's component), keeping their ids, shapes and names; later
//!   features that work on them belong to that component.
//! - `move_occurrence`: the Move feature with occurrences: occurrences
//!   placed in the feature's component move by a rigid motion in its
//!   coordinates (the motions of `move`, [`MoveSpec`]).
//! - `capture_position`: Capture Position (a snapshot): the
//!   occurrences are put at the given placements.
//!
//! A grounded occurrence cannot be moved: the feature fails (recompute
//! checks it, `recompute.rs`).

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::mapping::map_params;
use super::moves::{MoveSpec, check_bodies};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, OccurrencePath,
    PlacementChange, References,
};
use crate::assembly::{from_rows, matrix_rows};
use crate::ids::{BodyUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentFromBodiesDef<P = ParamId> {
    pub bodies: Vec<BodyUid>,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

impl<P> ComponentFromBodiesDef<P> {
    pub const TYPE: &'static str = "component_from_bodies";
    pub const BASE_NAME: &'static str = "ComponentFromBodies";

    pub fn new(bodies: Vec<BodyUid>) -> Self {
        Self {
            bodies,
            marker: PhantomData,
        }
    }

    map_params! {
        ComponentFromBodiesDef {
            bodies: clone,
            marker: phantom,
        }
    }
}

impl FeatureInfo for ComponentFromBodiesDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_bodies(ctx, &self.bodies)
    }

    fn new_component(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for ComponentFromBodiesDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut changes = Vec::with_capacity(self.bodies.len());
        for body in &self.bodies {
            // Set again: the component the feature made takes them.
            changes.push(BodyChange::Set(*body, ctx.body(*body)?));
        }
        Ok(FeatureOutput {
            sheets: self
                .bodies
                .iter()
                .map(|b| {
                    (
                        *b,
                        crate::sheet::SheetChange::Transform(Transform::IDENTITY),
                    )
                })
                .collect(),
            changes,
            ..FeatureOutput::default()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveOccurrenceDef<P = ParamId> {
    pub occurrences: Vec<OccurrenceUid>,
    pub transform: MoveSpec<P>,
    /// The occurrence of the feature's component (the parent of the moved
    /// occurrences) they were picked through, its absolute path from the
    /// root (`"O7"`), when that component is used more than once
    /// (mitcad#2307): a point or axis taken into the feature's component
    /// goes through it when none of the motion's picked paths passes
    /// through an occurrence of that component ([`MoveSpec::resolve`]), so
    /// that a part picked through the second occurrence of its parent
    /// lands on a root-level target there. A path that does not end in an
    /// occurrence of the feature's component (moved since) is not used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_occurrence: Option<OccurrencePath>,
}

impl<P> MoveOccurrenceDef<P> {
    /// A move of occurrences without a recorded parent occurrence.
    pub fn new(occurrences: Vec<OccurrenceUid>, transform: MoveSpec<P>) -> Self {
        Self {
            occurrences,
            transform,
            parent_occurrence: None,
        }
    }
}

impl<P> MoveOccurrenceDef<P> {
    pub const TYPE: &'static str = "move_occurrence";
    /// Named like moves of bodies.
    pub const BASE_NAME: &'static str = "Move";

    map_params! {
        MoveOccurrenceDef {
            occurrences: clone,
            transform: map,
            parent_occurrence: clone,
        }
    }
}

fn check_occurrences(occurrences: &[OccurrenceUid]) -> Result<(), String> {
    if occurrences.is_empty() {
        return Err("no occurrences selected".to_owned());
    }
    for (i, o) in occurrences.iter().enumerate() {
        if occurrences[..i].contains(o) {
            return Err(format!("occurrence {o} is listed more than once"));
        }
    }
    Ok(())
}

impl FeatureInfo for MoveOccurrenceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.transform.add_references(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_occurrences(&self.occurrences)?;
        self.transform.check(ctx)
    }
}

impl<K: Kernel> Evaluate<K> for MoveOccurrenceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let transform = self
            .transform
            .resolve(ctx, self.parent_occurrence.as_ref())?;
        if !transform.is_rigid() {
            return Err("an occurrence moves only by a rotation and a translation".to_owned());
        }
        Ok(FeatureOutput {
            placements: self
                .occurrences
                .iter()
                .map(|o| PlacementChange::Move(*o, transform))
                .collect(),
            ..FeatureOutput::default()
        })
    }
}

/// An occurrence's captured placement: rows of the rotation and the
/// translation in its parent's coordinates, `[[r00, r01, r02, tx], …]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub occurrence: OccurrenceUid,
    #[serde(with = "rows")]
    pub transform: Transform,
}

/// Transforms as rows of numbers (3 or 4 rows of 4).
pub(crate) mod rows {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::assembly::{from_rows, matrix_rows};
    use crate::transform::Transform;

    pub fn serialize<S: Serializer>(t: &Transform, s: S) -> Result<S::Ok, S::Error> {
        matrix_rows(t).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Transform, D::Error> {
        let rows = Vec::<Vec<f64>>::deserialize(d)?;
        from_rows(&rows).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturePositionDef<P = ParamId> {
    pub positions: Vec<Position>,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

impl<P> CapturePositionDef<P> {
    pub const TYPE: &'static str = "capture_position";
    pub const BASE_NAME: &'static str = "Position";

    pub fn new(positions: Vec<Position>) -> Self {
        Self {
            positions,
            marker: PhantomData,
        }
    }

    map_params! {
        CapturePositionDef {
            positions: clone,
            marker: phantom,
        }
    }
}

impl FeatureInfo for CapturePositionDef {
    fn references(&self) -> References {
        References::default()
    }

    fn check(&self, _ctx: &CheckContext<'_>) -> Result<(), String> {
        let occurrences: Vec<OccurrenceUid> = self.positions.iter().map(|p| p.occurrence).collect();
        check_occurrences(&occurrences)?;
        for p in &self.positions {
            // Rows read back as they were written.
            let rows: Vec<Vec<f64>> = matrix_rows(&p.transform).map(Vec::from).to_vec();
            from_rows(&rows).map_err(|e| format!("{}: {e}", p.occurrence))?;
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for CapturePositionDef {
    fn evaluate(&self, _ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        Ok(FeatureOutput {
            placements: self
                .positions
                .iter()
                .map(|p| PlacementChange::Set(p.occurrence, p.transform))
                .collect(),
            ..FeatureOutput::default()
        })
    }
}
