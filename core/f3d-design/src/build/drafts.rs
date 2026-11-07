// SPDX-License-Identifier: MIT
//! Drafts (mitcad#1363) as the dump IR's `DraftFeature` detail
//! (`core/import/SCHEMA.md`). The groups and scalars follow the cadmpeg
//! format description (*Draft*: two angle scalars at local ordinals 0
//! and 1, the drafted-face group of role `0x10`, the neutral-plane or
//! parting groups of role `0x21`), which our input slots carry as their
//! roles' high words (16 and 33); checked on a controlled model and the
//! corpus' draft *(seen)*:
//!
//! - the **drafted faces**: the face inputs of the slots of role 16;
//! - one slot of role 33 of **face inputs**: the neutral plane is that
//!   face (a planar face of the body before the draft);
//! - one slot of role 33 with **one entity input**: the neutral plane is
//!   that origin or construction plane (the description's WorkPlane form);
//! - **two slots of role 33**, one of one entity input and one of face
//!   inputs: a parting-line draft, its pull direction the plane's normal
//!   and its parting tool the faces;
//! - the **angles**: the parameters of the roles `Angle1` (the draft
//!   angle, radians, signed) and `Angle2` (the opposite side's, zero on
//!   both drafts read), held by the item's two parameter holders;
//! - the **tangent chain**: the second of the two bytes nine bytes after
//!   the root part (`u32 3 | u32 1 | u8 0 | u8 chain`): 0 on the controlled
//!   model, made without the tangent chain, 1 on the corpus' draft, made
//!   with the default. The first byte is 0 on both; the description names
//!   no material-side bit (the pull direction is the angle's sign with the
//!   neutral plane's orientation).

use std::collections::BTreeMap;

use super::super::classes::{ENTITY_REF, FACE_REF};
use super::super::decode::{self, ParameterEntry, TimelineEntry};
use super::super::ir::*;
use super::{Builder, face_input, paramref};

/// The draft's class.
pub const DRAFT: &str = "F4163A07-9E71-4C46-BD92-ACE5B5B40BED";

/// The slot role of the drafted faces (the description's `0x10` group).
pub const DRAFTED_FACES: u32 = 16;

/// The slot role of the neutral plane and of a parting line's groups (the
/// description's `0x21` groups).
pub const NEUTRAL: u32 = 33;

/// What a slot of role [`NEUTRAL`] selects.
enum Neutral {
    Faces(Vec<Reference>),
    Plane(Reference),
}

impl Builder<'_> {
    /// A draft's detail (see the module's documentation). Groups that do
    /// not decode leave their keys out: the import names what is missing.
    pub(super) fn draft(
        &self,
        it: &TimelineEntry,
        rl: &BTreeMap<&str, Vec<&ParameterEntry>>,
    ) -> Detail {
        let seg = self.seg;
        let pr = |role: &str| rl.get(role).and_then(|v| v.first()).map(|p| paramref(p));
        let slots = decode::extent_slots(seg, it.id);
        let faces_of = |inputs: &[u64]| -> Option<Vec<Reference>> {
            if inputs.is_empty() {
                return None;
            }
            inputs
                .iter()
                .map(|&i| {
                    if seg.guid_of(i) == Some(FACE_REF) {
                        face_input(seg, i)
                    } else {
                        None
                    }
                })
                .collect()
        };
        let mut x = DraftDetail::default();
        // The drafted faces of every slot of their role, in order; none if
        // one does not decode.
        let drafted: Vec<&decode::ExtentSlot> =
            slots.iter().filter(|s| s.role == DRAFTED_FACES).collect();
        if !drafted.is_empty() {
            x.input_faces = drafted
                .iter()
                .map(|s| faces_of(&s.inputs))
                .collect::<Option<Vec<Vec<Reference>>>>()
                .map(|v| v.into_iter().flatten().collect());
        }
        let neutral: Vec<Option<Neutral>> = slots
            .iter()
            .filter(|s| s.role == NEUTRAL)
            .map(|s| match s.inputs.as_slice() {
                [e] if seg.guid_of(*e) == Some(ENTITY_REF) => {
                    self.plane_ref(*e).map(Neutral::Plane)
                }
                inputs => faces_of(inputs).map(Neutral::Faces),
            })
            .collect();
        match neutral.as_slice() {
            [Some(Neutral::Faces(f))] if f.len() == 1 => x.plane = Some(f[0].clone()),
            [Some(Neutral::Faces(f))] => x.neutral_faces = Some(f.clone()),
            [Some(Neutral::Plane(p))] => x.plane = Some(p.clone()),
            [Some(Neutral::Plane(p)), Some(Neutral::Faces(f))]
            | [Some(Neutral::Faces(f)), Some(Neutral::Plane(p))] => {
                x.pull_plane = Some(p.clone());
                x.parting_tool = Some(f.clone());
            }
            _ => {}
        }
        if let Some(angle) = pr("Angle1") {
            x.draft_definition = Some(Definition {
                definition_type: Some("AngleExtentDefinition".to_owned()),
                angle: Some(angle),
                ..Definition::default()
            });
        }
        x.angle_two = pr("Angle2");
        let d = seg.data_of(it.id);
        x.is_tangent_chain = seg.root_part(d).and_then(|r| {
            match (d.get(r.end + 8).copied(), d.get(r.end + 9).copied()) {
                (Some(0 | 1), Some(c @ (0 | 1))) => Some(c == 1),
                _ => None,
            }
        });
        Detail::Draft(Box::new(x))
    }
}
