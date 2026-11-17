// SPDX-License-Identifier: MIT
//! The direction of gravity in the document (mitcad#1713, Part B;
//! `gravity.rs`): one undo step to set or clear it, and the direction a
//! planar face, a plane or an axis gives at the marker. Flex items hang
//! toward it (`document/flex_hang.rs`); one that hangs while it changes
//! settles again toward the new direction.

use super::{Document, ModelError, invalid};
use crate::features::GeomRef;
use crate::gravity::{Gravity, GravitySource, unit};
use crate::ids::ComponentUid;
use crate::kernel::Kernel;

type Vec3 = [f64; 3];

impl<K: Kernel> Document<K> {
    /// The design's direction of gravity, None without one.
    pub fn gravity(&self) -> Option<&Gravity> {
        self.state.gravity.as_ref()
    }

    /// The direction a source gives at the marker: against a planar face's
    /// or a plane's normal, along an axis; the other way with `reverse`.
    pub fn gravity_direction_of(&self, source: &GravitySource) -> Result<Vec3, ModelError> {
        let refused =
            || invalid("gravity takes its direction from a planar face, a plane or an axis");
        let plane = match &source.geometry {
            GeomRef::Origin(origin) => {
                use crate::datum::OriginDatum;
                match origin {
                    OriginDatum::Xy | OriginDatum::Xz | OriginDatum::Yz => true,
                    OriginDatum::X | OriginDatum::Y | OriginDatum::Z => false,
                    OriginDatum::Origin => return Err(refused()),
                }
            }
            GeomRef::Datum(uid) => match self.result.datums.get(uid).map(|d| &d.value) {
                Some(crate::datum::Datum::Plane(_)) => true,
                Some(crate::datum::Datum::Axis(_)) => false,
                _ => return Err(refused()),
            },
            GeomRef::Face { body, face } => {
                let shape = self.result.bodies.get(body).ok_or_else(|| {
                    invalid("the face gravity was to be taken from does not exist")
                })?;
                if self.kernel.face_plane(&shape.value, face).is_err() {
                    return Err(refused());
                }
                true
            }
            GeomRef::Edge { .. } | GeomRef::FixedAxis { .. } | GeomRef::SketchCurves { .. } => {
                false
            }
            GeomRef::FixedPlane { .. } => true,
            _ => return Err(refused()),
        };
        let frame = self
            .joint_frame(
                ComponentUid::ROOT,
                &source.occurrence.to_string(),
                &source.geometry,
                None,
            )
            .map_err(|e| {
                invalid(crate::notice::said(
                    "the direction of gravity cannot be taken from that",
                    e,
                ))
            })?;
        let z = std::array::from_fn(|r| frame.frame.linear[r][2]);
        let along = if plane != source.reverse {
            mitcad_vec3::scale(z, -1.0)
        } else {
            z
        };
        unit(along).ok_or_else(refused)
    }

    /// Sets the design's direction of gravity (None clears it); one undo
    /// step. A cable that hangs goes on hanging and settles again toward the
    /// new direction (the Let It Hang panel changes it while the item
    /// hangs); cleared, no direction is left and it is let go.
    pub fn set_gravity(&mut self, gravity: Option<Gravity>) -> Result<(), ModelError> {
        if let Some(g) = &gravity {
            g.check().map_err(invalid)?;
        }
        if self.state.gravity == gravity {
            return Ok(());
        }
        let label = if gravity.is_some() {
            "Set Gravity"
        } else {
            "Clear Gravity"
        };
        let hanging = self.flex_hang_item().is_some();
        self.apply(|state| {
            state.gravity = gravity;
            Ok((label.to_owned(), ()))
        })?;
        if hanging {
            self.hang_follow_gravity();
        }
        Ok(())
    }
}
