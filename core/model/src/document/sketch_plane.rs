// SPDX-License-Identifier: MIT
//! Redefine Sketch Plane (mitcad#787): a sketch moved to another plane or
//! planar face, and placed there by an offset along the normal, a move in
//! the plane, a rotation about the normal and a flip (`placement.rs`). The
//! sketch keeps its geometry in its own 2D coordinates; its linked
//! projections and sections follow their sources from the new plane at the
//! next evaluation (or say why they cannot), and the features built on it
//! follow it. One undo step (`sketch.redefine_plane` in commands.md).

use std::collections::BTreeSet;

use super::{Document, ModelError, invalid};
use crate::expr::{EvalContext, Expr, Lookup, Quantity, with_decimal_points};
use crate::features::{FeatureDef, SketchDef, SketchPlane, ValueInput};
use crate::ids::{FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::recompute::FeatureStatus;
use crate::sketch::placement::SketchPlacement;

/// A change of a sketch's plane and of its place on it.
#[derive(Debug, Clone, Default)]
pub struct SketchPlaneChange {
    /// The new plane; None keeps the sketch's.
    pub plane: Option<SketchPlane>,
    /// Where the new plane was picked: the occurrence path from the root to
    /// its component, as `sketch.create` takes it (mitcad#100); another
    /// component's plane is linked.
    pub occurrence: Option<Vec<OccurrenceUid>>,
    /// Where the sketch's component is seen (its first occurrence path when
    /// None).
    pub context: Option<Vec<OccurrenceUid>>,
    /// The offset along the normal, the move along x and y and the rotation
    /// about the normal: None keeps a value, a plain zero (`0`, `"0 mm"`)
    /// takes it away.
    pub offset: Option<ValueInput>,
    pub x: Option<ValueInput>,
    pub y: Option<ValueInput>,
    pub angle: Option<ValueInput>,
    /// The sketch turned over; None keeps it.
    pub flip: Option<bool>,
}

/// What a redefine did.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchPlaneChanged {
    /// The undo step's label.
    pub label: String,
    /// Whether the plane is another one: the own frame of the old plane was
    /// dropped.
    pub plane_changed: bool,
    /// The parameters it made.
    pub created: Vec<String>,
}

/// No parameters: constant expressions only.
struct NoNames;

impl Lookup for NoNames {
    fn lookup(&self, _name: &str) -> Option<Quantity> {
        None
    }
}

/// Whether a value is a plain zero: a number, or an expression without
/// parameters, that is zero in any unit.
fn plain_zero(value: &ValueInput) -> bool {
    match value {
        ValueInput::Number(v) => *v == 0.0,
        ValueInput::Name(text) => Expr::parse(&with_decimal_points(text))
            .ok()
            .filter(Expr::is_constant)
            .and_then(|e| e.eval(&NoNames, &EvalContext::default()).ok())
            .is_some_and(|q| q.value == 0.0),
    }
}

/// The features a plane is made by.
fn plane_features(plane: &SketchPlane) -> BTreeSet<FeatureUid> {
    match plane {
        SketchPlane::Face { face, body } | SketchPlane::Developed { face, body, .. } => {
            let mut features = face.features();
            if let Some(body) = body {
                features.insert(body.feature);
            }
            features
        }
        SketchPlane::Construction(uid) => BTreeSet::from([*uid]),
        SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => BTreeSet::new(),
    }
}

impl<K: Kernel> Document<K> {
    /// Moves sketch `uid` to another plane and places it there
    /// ([`SketchPlaneChange`]), one undo step. Refused, the document
    /// unchanged, for a plane made by a feature at or after the sketch in
    /// the timeline (the reason names it) and for a plane the sketch does
    /// not find where it is. A new plane drops the own frame the sketch
    /// had on the old one; the placement's values are the sketch's own
    /// parameters.
    pub fn redefine_sketch_plane(
        &mut self,
        uid: FeatureUid,
        change: &SketchPlaneChange,
    ) -> Result<SketchPlaneChanged, ModelError> {
        let position = self.state.require(uid)?;
        let entry = self.state.features[position].clone();
        let FeatureDef::Sketch(old) = &entry.def else {
            return Err(invalid(format!("{} ({uid}) is not a sketch", entry.name)));
        };
        self.state.editable(entry.component)?;
        // The definition as a command gives it: the parameters by name, so
        // that they stay.
        let params = &self.state.parameters;
        let mut def: SketchDef<ValueInput> = old
            .map_params(&mut |_, id: &ParamId| Ok::<_, ()>(ValueInput::Name(params.name(*id))))
            .expect("names never fail");
        let name = entry.name.clone();
        let mut plane_changed = false;
        if let Some(plane) = &change.plane {
            let link = match &change.occurrence {
                Some(path) => crate::links::link_for(
                    &self.state.assembly,
                    entry.component,
                    path,
                    change.context.as_deref(),
                )
                .map_err(|e| invalid(format!("{name} cannot use this plane: {e}")))?,
                None => None,
            };
            if *plane != old.plane || link != old.plane_link {
                plane_changed = true;
                // A plane of the sketch's own component comes before it
                // (another component's is found where its link shows it).
                if link.is_none() {
                    self.check_plane_order(position, &entry.name, plane)?;
                }
                def.plane = plane.clone();
                def.plane_link = link;
                // The own frame belonged to the old plane.
                def.frame = None;
                // A curved face's sketch lies in its developed coordinates
                // (mitcad#1768): the old plane's placement goes too.
                if plane.is_developed() {
                    def.placement = None;
                }
            }
        }
        let mut placement: SketchPlacement<ValueInput> =
            def.placement.take().map(|p| *p).unwrap_or_default();
        let given = [&change.offset, &change.x, &change.y, &change.angle];
        let slots = [
            &mut placement.offset,
            &mut placement.x,
            &mut placement.y,
            &mut placement.angle,
        ];
        for (value, slot) in given.into_iter().zip(slots) {
            if let Some(value) = value {
                *slot = (!plain_zero(value)).then(|| value.clone());
            }
        }
        if let Some(flip) = change.flip {
            placement.flip = flip;
        }
        def.placement = (!placement.is_empty()).then(|| Box::new(placement));
        if def.plane.is_developed() && def.placement.is_some() {
            return Err(invalid(format!(
                "{name} lies on a curved face: it takes no offset, move, rotation or flip"
            )));
        }
        let label = format!("Redefine Plane of {name}");
        let feature = FeatureDef::Sketch(def);
        let step = label.clone();
        // A sketch that failed before (it does not solve) may still move.
        let failed_before = matches!(self.status(uid), Some(FeatureStatus::Failed(_)));
        let created = self.apply_checked(
            |state| {
                let created = state.edit_feature(uid, &feature)?;
                Ok((step, created))
            },
            // A sketch that does not find its new plane is no change.
            |_, result| match result.result(uid).map(|r| &r.status) {
                Some(FeatureStatus::Failed(error)) if !failed_before => Err(invalid(format!(
                    "{name} cannot use this plane or face: {error}"
                ))),
                _ => Ok(()),
            },
        )?;
        Ok(SketchPlaneChanged {
            label,
            plane_changed,
            created,
        })
    }

    /// Refuses a plane made by a feature at `position` (the sketch's) or
    /// after it: the sketch is evaluated before it.
    fn check_plane_order(
        &self,
        position: usize,
        sketch: &str,
        plane: &SketchPlane,
    ) -> Result<(), ModelError> {
        for feature in plane_features(plane) {
            let Some(at) = self.state.position(feature) else {
                return Err(invalid(format!(
                    "{sketch} cannot use this plane: feature {feature} does not exist"
                )));
            };
            if at >= position {
                let later = &self.state.features[at].name;
                return Err(invalid(format!(
                    "{sketch} cannot lie on a plane or face of {later}: {later} comes after \
                     {sketch} in the timeline, and a sketch's plane must be made before it"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::plain_zero;
    use crate::features::ValueInput;

    #[test]
    fn plain_zeros_are_constant_zero_values() {
        let name = |text: &str| ValueInput::Name(text.to_owned());
        assert!(plain_zero(&ValueInput::Number(0.0)));
        assert!(plain_zero(&name("0")));
        assert!(plain_zero(&name("0 mm")));
        assert!(plain_zero(&name("0 deg")));
        assert!(plain_zero(&name("2 mm - 2 mm")));
        assert!(!plain_zero(&ValueInput::Number(1.0)));
        assert!(!plain_zero(&name("5 mm")));
        assert!(!plain_zero(&name("d1")));
        assert!(!plain_zero(&name("d1 * 0")));
    }
}
