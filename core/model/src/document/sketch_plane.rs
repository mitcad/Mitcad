// SPDX-License-Identifier: MIT
//! Redefine Sketch Plane (mitcad#787): a sketch moved to another plane or
//! planar face, and placed there by an offset along the normal, a move in
//! the plane, a rotation about the normal and a flip (`placement.rs`). The
//! sketch keeps its geometry in its own 2D coordinates; its linked
//! projections and sections follow their sources from the new plane at the
//! next evaluation (or say why they cannot), and the features built on it
//! follow it. One undo step (`sketch.redefine_plane` in commands.md).

use std::collections::BTreeSet;
use std::sync::Arc;

use super::{DocState, Document, ModelError, invalid};
use crate::expr::{EvalContext, Expr, Lookup, Quantity, with_decimal_points};
use crate::features::{FeatureDef, SketchDef, SketchPlane, ValueInput};
use crate::ids::{FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::recompute::{FeatureStatus, Recomputed};
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
        SketchPlane::Face { face, body, .. } | SketchPlane::Developed { face, body, .. } => {
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
            // The same face is the same plane, its placed frame (mitcad#2409)
            // given or not.
            if !plane.same_plane(&old.plane) || link != old.plane_link {
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

/// The frame a sketch on a planar face is to keep from where it was placed
/// (mitcad#2409) after an edit that leaves it none: `before` is the sketch
/// before the edit (None for a new one), `plane` its plane's frame as the
/// edit's recompute found it. A sketch placed on a face by the edit, new or
/// moved to another face, keeps the frame it has now, so nothing moves
/// when it is placed; one whose placed frame an edit left out on the same
/// face keeps the one it had. A sketch that had none on the same face (a
/// design saved before mitcad#2409) stays without one: its frame keeps the
/// default rule, so nothing moves in it. A planar sketch and a 3D sketch
/// (its support, mitcad#2465) are alike here: a sketch converted from one
/// kind to the other keeps its uid and its plane, so it is the same sketch
/// on the same face.
fn frame_to_keep(
    before: Option<Placing<'_>>,
    now: Placing<'_>,
    plane: Option<&SketchFrame>,
) -> Option<SketchFrame> {
    let (SketchPlane::Face { placed: None, .. }, link) = now else {
        return None;
    };
    match before {
        Some((old, old_link)) if old.same_plane(now.0) && old_link == link => old.placed().copied(),
        _ => plane.copied(),
    }
}

/// A sketch's plane and the link to the component whose plane it is (None
/// for its own): a planar sketch's plane, a 3D sketch's support
/// (mitcad#2465).
type Placing<'a> = (&'a SketchPlane, Option<&'a OccurrenceLink>);

/// The plane of a planar or 3D sketch; None for any other feature.
fn placing(def: &FeatureDef) -> Option<Placing<'_>> {
    match def {
        FeatureDef::Sketch(def) => Some((&def.plane, def.plane_link.as_ref())),
        FeatureDef::SpatialSketch(def) => Some((&def.support, def.planar.plane_link.as_ref())),
        _ => None,
    }
}

/// The plane of a planar or 3D sketch, to change.
fn placing_mut(def: &mut FeatureDef) -> Option<&mut SketchPlane> {
    match def {
        FeatureDef::Sketch(def) => Some(&mut def.plane),
        FeatureDef::SpatialSketch(def) => Some(&mut def.support),
        _ => None,
    }
}

/// The frame of the plane of sketch `uid` as `result` found it: the planar
/// output's plane (a 3D sketch has one when it solved). A 3D sketch whose
/// sketch plane gave no profiles (it did not solve, or its curves in the
/// plane did not make regions) takes its frame when it is the support's:
/// without an own frame or a placement on the support (mitcad#2465).
fn found_plane<S>(
    def: &FeatureDef,
    uid: FeatureUid,
    result: &Recomputed<S>,
) -> Option<SketchFrame> {
    if let Some(output) = result.sketches.get(&uid) {
        return Some(output.value.plane);
    }
    let FeatureDef::SpatialSketch(def) = def else {
        return None;
    };
    if def.frame.is_some() || def.planar.placement.is_some() {
        return None;
    }
    result.result(uid)?.output.as_ref()?.spatial.as_ref()?.frame
}

impl DocState {
    /// Gives the sketches an edit placed on planar faces the frames they
    /// have now (mitcad#2409, [`frame_to_keep`]), in the edit's step: their
    /// frames stay as the recompute made them, and a later edit of an
    /// earlier feature that tilts the face takes them with it. 3D sketches
    /// on faces keep theirs the same way (mitcad#2465).
    pub(crate) fn keep_face_sketch_frames<S>(&mut self, before: &DocState, result: &Recomputed<S>) {
        for i in 0..self.features.len() {
            let entry = &self.features[i];
            let Some(now) = placing(&entry.def) else {
                continue;
            };
            if !matches!(now.0, SketchPlane::Face { placed: None, .. }) {
                continue;
            }
            let old = before.entry(entry.uid).and_then(|old| placing(&old.def));
            let plane = found_plane(&entry.def, entry.uid, result);
            let Some(frame) = frame_to_keep(old, now, plane.as_ref()) else {
                continue;
            };
            let mut kept = (**entry).clone();
            if let Some(SketchPlane::Face { placed, .. }) = placing_mut(&mut kept.def) {
                *placed = Some(Box::new(frame));
            }
            self.features[i] = Arc::new(kept);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{frame_to_keep, placing, plain_zero};
    use crate::features::{FeatureDef, SketchDef, SketchPlane, SpatialSketchDef, ValueInput};
    use crate::profile::SketchFrame;

    fn side(placed: Option<SketchFrame>) -> SketchPlane {
        SketchPlane::Face {
            face: Box::new("F2:side(c2[c1,c3])".parse().unwrap()),
            body: None,
            placed: placed.map(Box::new),
        }
    }

    /// [`frame_to_keep`] of two sketch definitions.
    fn keep(
        before: Option<&FeatureDef>,
        now: &FeatureDef,
        plane: Option<&SketchFrame>,
    ) -> Option<SketchFrame> {
        frame_to_keep(
            before.map(|b| placing(b).unwrap()),
            placing(now).unwrap(),
            plane,
        )
    }

    #[test]
    fn a_3d_sketch_placed_on_a_face_keeps_its_frame() {
        // mitcad#2465: a 3D sketch's support as a planar sketch's plane.
        let spatial = |placed: Option<SketchFrame>| {
            FeatureDef::SpatialSketch(SpatialSketchDef::new(side(placed)))
        };
        let planar = |placed: Option<SketchFrame>| {
            FeatureDef::Sketch(SketchDef {
                plane: side(placed),
                ..SketchDef::default()
            })
        };
        let now = SketchFrame {
            origin: [60.0, 0.0, 0.0],
            x_axis: [0.0, -1.0, 0.0],
            y_axis: [0.0, 0.0, -1.0],
        };
        let kept = SketchFrame {
            origin: [59.8, 5.2, 0.0],
            ..now
        };
        // Created on the face: the frame it has now.
        assert_eq!(keep(None, &spatial(None), Some(&now)), Some(now));
        // Its support moved onto the face from an origin plane or another
        // face: the frame it has now.
        let on_xy = FeatureDef::SpatialSketch(SpatialSketchDef::new(SketchPlane::Xy));
        assert_eq!(keep(Some(&on_xy), &spatial(None), Some(&now)), Some(now));
        let other = FeatureDef::SpatialSketch(SpatialSketchDef::new(SketchPlane::on_face(
            "F2:end(r{c1})".parse().unwrap(),
            None,
        )));
        assert_eq!(keep(Some(&other), &spatial(None), Some(&now)), Some(now));
        // An edit that left the placed frame out keeps the one it had.
        assert_eq!(
            keep(Some(&spatial(Some(kept))), &spatial(None), Some(&now)),
            Some(kept)
        );
        // A 3D sketch saved before: none, the default rule, also after an
        // edit of it.
        assert_eq!(keep(Some(&spatial(None)), &spatial(None), Some(&now)), None);
        // Converted from a planar sketch on the same face (the same uid):
        // the planar sketch's placed frame or none, as it had.
        assert_eq!(keep(Some(&planar(None)), &spatial(None), Some(&now)), None);
        assert_eq!(
            keep(Some(&planar(Some(kept))), &spatial(None), Some(&now)),
            Some(kept)
        );
        // And back to a planar sketch.
        assert_eq!(keep(Some(&spatial(None)), &planar(None), Some(&now)), None);
        // One kept already, a support that is no face, not evaluated.
        assert_eq!(keep(None, &spatial(Some(kept)), Some(&now)), None);
        assert_eq!(keep(None, &on_xy, Some(&now)), None);
        assert_eq!(keep(None, &spatial(None), None), None);
        // Another component's face where it was its own: placed again.
        let mut linked = SpatialSketchDef::new(side(None));
        linked.planar.plane_link = Some(crate::links::OccurrenceLink::default());
        assert_eq!(
            keep(
                Some(&spatial(Some(kept))),
                &FeatureDef::SpatialSketch(linked),
                Some(&now)
            ),
            Some(now)
        );
    }

    #[test]
    fn a_sketch_placed_on_a_face_keeps_its_frame() {
        let face = |placed: Option<SketchFrame>| {
            FeatureDef::Sketch(SketchDef {
                plane: side(placed),
                ..SketchDef::default()
            })
        };
        let other = FeatureDef::Sketch(SketchDef {
            plane: SketchPlane::on_face("F2:end(r{c1})".parse().unwrap(), None),
            ..SketchDef::default()
        });
        let now = SketchFrame {
            origin: [60.0, 0.0, 0.0],
            x_axis: [0.0, -1.0, 0.0],
            y_axis: [0.0, 0.0, -1.0],
        };
        let kept = SketchFrame {
            origin: [59.8, 5.2, 0.0],
            ..now
        };
        // New, or moved onto the face: the frame it has now.
        assert_eq!(keep(None, &face(None), Some(&now)), Some(now));
        assert_eq!(keep(Some(&other), &face(None), Some(&now)), Some(now));
        assert_eq!(
            keep(Some(&face(Some(kept))), &face(None), Some(&now)),
            Some(kept),
            "an edit that left the placed frame out keeps the one it had"
        );
        // A design saved before mitcad#2409: none, the default rule.
        assert_eq!(keep(Some(&face(None)), &face(None), Some(&now)), None);
        // One kept already, a plane that is no face, a sketch not evaluated.
        assert_eq!(keep(None, &face(Some(kept)), Some(&now)), None);
        assert_eq!(
            keep(None, &FeatureDef::Sketch(SketchDef::default()), Some(&now)),
            None
        );
        assert_eq!(keep(None, &face(None), None), None);
        // Another component's face where it was its own: placed again.
        let linked = FeatureDef::Sketch(SketchDef {
            plane: side(None),
            plane_link: Some(crate::links::OccurrenceLink::default()),
            ..SketchDef::default()
        });
        assert_eq!(
            keep(Some(&face(Some(kept))), &linked, Some(&now)),
            Some(now)
        );
    }

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
