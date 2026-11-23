// SPDX-License-Identifier: MIT
//! Profiles in parallel (mitcad#2201; `commands.md`, "BUILD: profile
//! assemblies", *Parallel placements*): `build_place` with `at.place`
//! `parallel` lays a profile along a target profile, their length axes
//! parallel and a long face of the new profile against the outer plane of
//! the target's side picked (above, below or beside it), so that it rests
//! across the slot opening.
//!
//! - The side is the one of the face or the side frame picked. **Across**
//!   (`at.across`) centres the new profile on the side (its middle between
//!   two or more slots, `side_<side>`, or its only slot or face frame), or
//!   puts its cross-section's centre on one slot: the slot frame picked, or
//!   the slot nearest `at.point`.
//! - **Rotate** turns the profile about its length axis in quarter turns;
//!   the face that then faces the target is the contact face. Unturned, the
//!   profile rests on its widest face (a 20 x 40 on a 40 mm face, as a
//!   member lies flat); turned 90 degrees on its next face (the 20 mm one).
//! - **Length**: left out (or `{"match": "target"}`) the target's own length
//!   parameter, so that the profile follows the target's length; typed, a
//!   value.
//! - **From** and **Offset** (`at.from`, `at.offset`) place it along the
//!   target as a part on a side is placed (mitcad#417): from end A its end A
//!   from the target's end A, from end B its end B from the target's end B
//!   (positive towards A), from the middle its middle from the target's.
//!   End A and offset 0 when left out: both ends flush when the lengths
//!   match. It may overhang the target's ends.
//!
//! The joint is a slider along the target's slot frame (or the side's
//! middle) as a part on a side has, flipped, between the new part's `end_a`
//! frame turned by a frame override to its contact face (origin on the
//! face at end A, z out of it, x along the part) and the target's frame; its
//! slide is bound to `from` as a side placement's is. So the profile
//! follows the target when it moves or turns, its end B and middle
//! references follow the target's length, and its record keeps `from` and
//! `parallel`. `build_edit` places it anew with what changed
//! ([`ParallelCrateOps::edit_parallel`]).

use std::f64::consts::FRAC_PI_2;

use serde::{Deserialize, Serialize};

use crate::geometry::{V3, dot, inverse, sub, unit, x_axis, z_axis};
use crate::parts::PartInfo;
use crate::place::{
    EditCrateOps, LengthPlan, NewPart, PlaceAt, PlaceInput, PlaceKind, SidePlan, SourceUse, Spot,
    Target, edit_joint, edit_part, offset_of, reach_along_x, refuse, shifted, side_offset,
    slide_of, slide_value_of, value_in,
};
use crate::prelude::*;
use crate::{EditInput, LengthInput, PartInput, SideFrom};
use mitcad_model::expr::Unit;
use mitcad_model::features::{FeatureDef, FrameOverride, GeomRef, ValueInput};
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::joints::{JointKind, Motion, SlideAxis};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ModelError};

/// How a profile placed in parallel stands across the target's side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Across {
    /// Centred on the side: on its middle between its slots, or on its only
    /// slot.
    #[default]
    Middle,
    /// Its cross-section's centre on one slot of the side.
    Slot,
}

impl Across {
    pub fn as_str(self) -> &'static str {
        match self {
            Across::Middle => "middle",
            Across::Slot => "slot",
        }
    }
}

/// The length `{"match": "target"}` names (`LengthInput::Match`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchLength {
    /// The target's length.
    #[default]
    Target,
}

/// The outward normal of the face a section rests on, turned `quarter`
/// quarter turns about the part's axis (+Z): unturned its widest face's
/// (−Y when the section is at least as wide along X as along Y, else −X).
pub(crate) fn contact_normal(section: &BoundingBox, quarter: i64) -> V3 {
    let (w, h) = (
        section.max[0] - section.min[0],
        section.max[1] - section.min[1],
    );
    let first = if w >= h - 1e-9 {
        [0.0, -1.0]
    } else {
        [-1.0, 0.0]
    };
    let (s, c) = match quarter.rem_euclid(4) {
        0 => (0.0, 1.0),
        1 => (1.0, 0.0),
        2 => (0.0, -1.0),
        _ => (-1.0, 0.0),
    };
    let x: f64 = c * first[0] - s * first[1];
    let y: f64 = s * first[0] + c * first[1];
    // No negative zero.
    [x + 0.0, y + 0.0, 0.0]
}

/// The override of the new part's `end_a` frame (`own`, in its coordinates)
/// that puts it on the contact face: its origin on the face, centred on the
/// section, at end A; z out of the face; x along the part (+Z).
pub(crate) fn contact_override(
    section: &BoundingBox,
    own: &Transform,
    quarter: i64,
) -> FrameOverride {
    let n = contact_normal(section, quarter);
    let centre = [
        (section.min[0] + section.max[0]) / 2.0,
        (section.min[1] + section.max[1]) / 2.0,
    ];
    let half = if n[0].abs() > 0.5 {
        (section.max[0] - section.min[0]) / 2.0
    } else {
        (section.max[1] - section.min[1]) / 2.0
    };
    let round = |v: f64| (v * 1e9).round() / 1e9 + 0.0;
    FrameOverride {
        origin: Some([
            round(centre[0] + n[0] * half),
            round(centre[1] + n[1] * half),
            round(own.translation[2]),
        ]),
        z_axis: Some(n),
        x_axis: Some(crate::geometry::Z),
        shift: None,
        relative: None,
    }
}

/// The quarter turn a contact override's z gives (0 to 3), by the section.
fn quarter_of(section: &BoundingBox, z: V3) -> Option<i64> {
    let z = unit(z)?;
    (0..4).find(|k| dot(contact_normal(section, *k), z) > 1.0 - 1e-6)
}

/// The quarter turns of a rotation (radians).
fn quarters(rotate: f64) -> i64 {
    // A rotation is a few quarter turns: the cast cannot overflow.
    #[allow(clippy::cast_possible_truncation)]
    let k = (rotate / FRAC_PI_2).round() as i64;
    k.rem_euclid(4)
}

/// A value negated: a number's sign, an expression in brackets.
fn negated(value: &ValueInput) -> ValueInput {
    match value {
        ValueInput::Number(n) => ValueInput::Number(-n + 0.0),
        ValueInput::Name(text) => ValueInput::Name(format!("-({text})")),
    }
}

/// The side frames of a target on the side (the plane facing the way) of
/// `frame`: its slots there, or its face frame.
fn side_frames(target: &PartInfo, frame: &Transform) -> Vec<(String, Transform)> {
    let normal = z_axis(frame);
    target
        .sides()
        .into_iter()
        .filter(|(_, t)| {
            dot(z_axis(t), normal) > 1.0 - 1e-6
                && dot(sub(t.translation, frame.translation), normal).abs() < 1e-6
        })
        .map(|(n, t)| (n.clone(), *t))
        .collect()
}

/// The middle of the side of `frame` between its slots (mitcad#2002), when
/// it has one.
fn side_middle(target: &PartInfo, frame: &Transform) -> Option<String> {
    let normal = z_axis(frame);
    target
        .middles
        .iter()
        .find(|(_, t)| {
            dot(z_axis(t), normal) > 1.0 - 1e-6
                && dot(sub(t.translation, frame.translation), normal).abs() < 1e-6
        })
        .map(|(n, _)| n.clone())
}

/// The distance of a point from a frame's line along its x (a slot's).
fn off_line(t: &Transform, p: V3) -> f64 {
    let [o, x, _, _] = crate::geometry::parts(t);
    let d = sub(p, o);
    let along = dot(d, x);
    crate::geometry::norm(sub(d, crate::geometry::scale(x, along)))
}

/// The name of the length parameter of the target `at` names (the profile
/// a parallel placement lies along).
pub(crate) fn target_length_name<K: Kernel>(
    doc: &Document<K>,
    at: &PlaceAt,
) -> Result<String, ModelError> {
    let path = doc.path_from_host(&at.occurrence)?;
    let target = doc.target(&path).ok_or_else(|| {
        refuse("a profile placed in parallel lies along another profile: pick its long side")
    })?;
    let (id, _) = target.info.length.ok_or_else(|| {
        refuse(format!(
            "{} has no length: a profile placed in parallel lies along a profile",
            target.info.name
        ))
    })?;
    Ok(doc.parameters().name(id))
}

/// Plans a profile placed in parallel on a target's side (see the module):
/// the side's frame across, the contact face and the slide from `from`.
/// `matched`: its length is the target's own parameter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_parallel<K: Kernel>(
    doc: &Document<K>,
    source: &SourceUse,
    target: &Target,
    geometry: &GeomRef,
    at: &PlaceAt,
    new: &NewPart<'_>,
    length: &LengthPlan,
    matched: bool,
) -> Result<Spot, ModelError> {
    let label = source.def.label();
    if source.meta.length.is_none() {
        return Err(refuse(format!(
            "{label} is not cut to length: only a profile lies along another in parallel"
        )));
    }
    if matches!(length, LengthPlan::Bound { .. }) {
        return Err(refuse(
            "a profile placed in parallel takes the target's length or a typed one: its length cannot be bound",
        ));
    }
    let (length_id, target_length) = target.info.length.ok_or_else(|| {
        refuse(format!(
            "{} has no length: a profile placed in parallel lies along a profile",
            target.info.name
        ))
    })?;
    let target_name = doc.parameters().name(length_id);
    let target_part = target.part()?;
    let point_local = at.point.map(|p| inverse(&target.placement).apply_point(p));
    // The side picked: a side frame, or a face on it.
    let picked = match geometry {
        GeomRef::Mount { feature, name } if *feature == target_part => {
            if !target.info.is_side(name) {
                return Err(refuse(format!(
                    "{name} of {} is no side: pick a long side of it",
                    target.info.name
                )));
            }
            name.clone()
        }
        GeomRef::Mount { .. } => {
            return Err(refuse(format!(
                "{geometry} is not a frame of {}",
                target.info.name
            )));
        }
        GeomRef::Face { body, face } => {
            if face.feature == target_part && matches!(face.role.as_str(), "end_a" | "end_b") {
                return Err(refuse(format!(
                    "a profile placed in parallel lies on a long side of {}, not on its end",
                    target.info.name
                )));
            }
            doc.pick_slot(target, Some(&(*body, face.clone())), point_local, false)?
        }
        _ => {
            return Err(refuse(format!(
                "a profile placed in parallel lies on a long side of {}: pick one of its side faces",
                target.info.name
            )));
        }
    };
    // It rests on the side's outer plane, in no slot: no system to fit.
    let picked_frame = *target
        .info
        .mount(&picked)
        .ok_or_else(|| refuse(format!("{} has no frame {picked}", target.info.name)))?;
    if dot(x_axis(&picked_frame), crate::geometry::Z) < 1.0 - 1e-6 {
        return Err(refuse(format!(
            "the side {picked} of {} does not run along its length",
            target.info.name
        )));
    }
    // Across: centred on the side, or on one of its slots.
    let on_side = side_frames(&target.info, &picked_frame);
    let several = on_side.len() > 1;
    let middle = side_middle(&target.info, &picked_frame);
    let (frame, across) = match at.across.unwrap_or_default() {
        Across::Middle => match (&middle, several) {
            (Some(m), _) => (m.clone(), Some(Across::Middle)),
            (None, false) => (picked.clone(), None),
            // Slots that do not lie in one plane have no middle.
            (None, true) => (picked.clone(), Some(Across::Slot)),
        },
        Across::Slot if target.info.is_side_middle(&picked) => {
            let mut slots = on_side.clone();
            if let Some(p) = point_local {
                slots.sort_by(|a, b| off_line(&a.1, p).total_cmp(&off_line(&b.1, p)));
            }
            let first = slots
                .first()
                .map(|(n, _)| n.clone())
                .ok_or_else(|| refuse(format!("{} has no slot on that side", target.info.name)))?;
            (first, Some(Across::Slot))
        }
        Across::Slot => (picked.clone(), several.then_some(Across::Slot)),
    };
    let frame_local = *target
        .info
        .mount(&frame)
        .ok_or_else(|| refuse(format!("{} has no frame {frame}", target.info.name)))?;
    // The contact face, turned by Rotate.
    let section = new
        .bounds
        .ok_or_else(|| refuse(format!("{label} has no box to rest on the side")))?;
    let own = new.mounts.get("end_a").copied().ok_or_else(|| {
        refuse(format!(
            "{label} has no frame end_a to lie along the side from"
        ))
    })?;
    let contact = contact_override(section, &own, quarters(new.rotate));
    let turned = mitcad_model::internal::joints::apply_override(&own, &contact).map_err(refuse)?;
    // Mating frames face each other: the contact face against the side.
    let flip = true;
    let (near, far) = reach_along_x(Some(section), &turned, 0.0, flip);
    // Along the target, from its reference.
    let from = at.from.unwrap_or_default();
    if from == SideFrom::Gap {
        return Err(refuse(
            "a profile placed in parallel is measured from end A, end B or the middle, not Middle of Gap",
        ));
    }
    let offset_value = at.offset.clone().unwrap_or(ValueInput::Number(0.0));
    let offset = value_in(doc, &offset_value, Unit::MM)?;
    let value = if matched {
        // Its length is the target's: from end B its end B is the target's
        // less the offset, from the middle its middle the target's plus it.
        match from {
            SideFrom::B => shifted(&negated(&offset_value), -near),
            _ => shifted(&offset_value, -near),
        }
    } else {
        slide_value_of(from, &offset_value, near, far, Some(&target_name))?
    };
    let tx = slide_of(from, offset, near, far, Some(target_length))?;
    let frame_b = target
        .placement
        .after(&frame_local)
        .after(&Transform::translation([tx, 0.0, 0.0]));
    let side = SidePlan {
        near,
        far,
        length: Some(target_length),
        snap: 0.0,
        grid: 0.0,
        snapped: None,
        offset: tx + near,
        from,
        from_offset: offset,
        long_point: false,
        gaps: Vec::new(),
        gap: None,
        lines: Vec::new(),
        line: None,
        across: across.map(Across::as_str),
    };
    Ok(Spot::Joint(Box::new(crate::place::JointPlan {
        kind: JointKind::Slider,
        slide: SlideAxis::X,
        own: "end_a".to_owned(),
        b: target.origin(&frame)?,
        frame_b,
        flip,
        slide_value: Some((Motion::Tx, value, tx)),
        ray: None,
        side: Some(side),
        end: None,
        own_override: Some(contact),
        cuts: Vec::new(),
        from: Some(from),
        warnings: Vec::new(),
    })))
}

/// A profile placed in parallel as it is now (mitcad#2201).
#[derive(Debug, Clone, PartialEq)]
pub struct ParallelPart {
    pub joint: FeatureUid,
    /// The target profile.
    pub target: ComponentUid,
    /// The target's frame it lies on: a slot, a face frame or a side's
    /// middle.
    pub frame: String,
    /// Centred on the side or on a slot; None on a side with one frame.
    pub across: Option<Across>,
    /// Its turn about its axis, radians (quarter turns).
    pub rotate: f64,
    /// Its length is the target's.
    pub matched: bool,
    pub from: SideFrom,
}

/// The profile placed in parallel a BUILD component is, when it is one.
pub fn parallel_of<K: Kernel>(doc: &Document<K>, component: ComponentUid) -> Option<ParallelPart> {
    let info = doc.part_info(component)?;
    if !info.record.parallel {
        return None;
    }
    let joint = info.record.joint?;
    let entry = doc.feature(joint)?;
    let FeatureDef::Joint(j) = &entry.def else {
        return None;
    };
    let GeomRef::Mount { name: frame, .. } = &j.b.geometry else {
        return None;
    };
    let target = mitcad_model::internal::joints::path_component(
        doc.assembly(),
        entry.component,
        &j.b.occurrence.0,
    )
    .ok()?;
    let target_info = doc.part_info(target)?;
    let quarter = match (
        j.a.frame_override.and_then(|o| o.z_axis),
        doc.part_bounds(component),
    ) {
        (Some(z), Some(section)) => quarter_of(&section, z).unwrap_or(0),
        _ => 0,
    };
    let matched = match (info.length, target_info.length) {
        (Some((own, _)), Some((other, _))) => own == other,
        _ => false,
    };
    let across = if target_info.is_side_middle(frame) {
        Some(Across::Middle)
    } else {
        let t = target_info.mount(frame)?;
        (side_frames(&target_info, t).len() > 1).then_some(Across::Slot)
    };
    // As placed: its quarter turn as a number of radians.
    #[allow(clippy::cast_precision_loss)]
    let rotate = quarter as f64 * FRAC_PI_2;
    Some(ParallelPart {
        joint,
        target,
        frame: frame.clone(),
        across,
        rotate,
        matched,
        from: info.record.from.unwrap_or_default(),
    })
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait ParallelCrateOps<K: Kernel> {
    /// `build_edit` of a profile placed in parallel with `at` (`offset`,
    /// `from`, `across`), `rotate`, `length` (a value or `{"match":
    /// "target"}`), `config` or `parameters`: placed anew on the same side
    /// with what changed, in one undo step. None for another part, or for
    /// an edit of none of these (its ends, its machining). Flip and tilt
    /// are refused.
    fn edit_parallel(
        &mut self,
        component: ComponentUid,
        input: &EditInput,
    ) -> Result<Option<Vec<String>>, ModelError>;
}

impl<K: Kernel> ParallelCrateOps<K> for Document<K> {
    fn edit_parallel(
        &mut self,
        component: ComponentUid,
        input: &EditInput,
    ) -> Result<Option<Vec<String>>, ModelError> {
        let Some(now) = parallel_of(self, component) else {
            return Ok(None);
        };
        let refused = [
            ("flip", input.flip.is_some()),
            ("tilt", input.tilt.is_some()),
            ("to", input.to.is_some()),
            ("lie", input.lie.is_some()),
            (
                "at.point",
                input.at.as_ref().is_some_and(|a| a.point.is_some()),
            ),
        ];
        if let Some((field, _)) = refused.iter().find(|(_, given)| *given) {
            return Err(refuse(format!(
                "a profile placed in parallel lies along its target from its end A: it takes no `{field}`"
            )));
        }
        let handled = input.at.is_some()
            || input.rotate.is_some()
            || input.length.is_some()
            || input.config.is_some()
            || !input.parameters.is_empty();
        if !handled {
            return Ok(None);
        }
        let apart = [
            ("ends", input.ends.is_some()),
            ("machining", input.machining.is_some()),
            ("cuts", input.cuts.is_some()),
            ("grade", input.grade.is_some()),
            ("limits", input.limits.is_some()),
        ];
        if let Some((field, _)) = apart.iter().find(|(_, given)| *given) {
            return Err(refuse(format!(
                "edit `{field}` of a profile placed in parallel in a build_edit of its own"
            )));
        }
        if matches!(input.length, Some(LengthInput::Bound { .. })) {
            return Err(refuse(
                "a profile placed in parallel takes the target's length or a typed one: its length cannot be bound",
            ));
        }
        let info = self
            .part_info(component)
            .ok_or_else(|| refuse(format!("{} is no BUILD part", input.component)))?;
        let part = info
            .part
            .ok_or_else(|| refuse(format!("{} has no derived part", info.name)))?;
        let source = info
            .record
            .source
            .ok_or_else(|| refuse(format!("{} has no library source", info.name)))?;
        let length_name = info
            .meta
            .length
            .as_ref()
            .map(|l| l.parameter.clone())
            .ok_or_else(|| refuse(format!("{} is not cut to length", info.name)))?;
        let target_info = self
            .part_info(now.target)
            .ok_or_else(|| refuse(format!("{}: the profile it lies along is gone", info.name)))?;
        let target_length = target_info.length.map(|(_, l)| l);
        let joint_entry = self
            .feature(now.joint)
            .ok_or(ModelError::NoSuchFeature(now.joint))?;
        let FeatureDef::Joint(j) = &joint_entry.def else {
            return Err(refuse(format!("{}: its joint is gone", info.name)));
        };
        let path = j.b.occurrence.to_string();
        let target_part = target_info
            .part
            .ok_or_else(|| refuse(format!("{} has no derived part", target_info.name)))?;
        let tx = j
            .position
            .get(&Motion::Tx)
            .and_then(|id| self.parameters().value(*id))
            .unwrap_or(0.0);
        let (near, far) = self.side_reach(&info, now.joint, None);
        let mut changed = Vec::new();
        // From and Offset: a new From alone keeps the part where it is.
        let edit_at = input.at.clone().unwrap_or_default();
        let from = edit_at.from.unwrap_or(now.from);
        if from != now.from {
            changed.push("from".to_owned());
        }
        let offset = match (&edit_at.offset, edit_at.from) {
            (Some(value), _) => {
                changed.push("offset".to_owned());
                value.clone()
            }
            (None, Some(f)) if f != now.from => {
                let kept = offset_of(f, tx, near, far, target_length);
                changed.push("offset".to_owned());
                ValueInput::Number((kept * 1000.0).round() / 1000.0 + 0.0)
            }
            _ => ValueInput::Number(side_offset(self, component).unwrap_or(0.0)),
        };
        let across = edit_at.across.or(now.across);
        if edit_at.across.is_some() && edit_at.across != now.across {
            changed.push("across".to_owned());
        }
        let rotate = match &input.rotate {
            Some(value) => {
                let angle = value_in(self, value, Unit::DEG)?;
                let quarter = angle / FRAC_PI_2;
                if (quarter - quarter.round()).abs() > 1e-6 {
                    return Err(refuse("the rotation is 0, 90, 180 or 270 degrees"));
                }
                changed.push("rotate".to_owned());
                angle
            }
            None => now.rotate,
        };
        // The length: the target's, typed, or as it is.
        let length_now = info.length.map(|(_, l)| l);
        let (length, write_length) = match &input.length {
            Some(LengthInput::Match { .. }) => (
                Some(LengthInput::Match {
                    what: MatchLength::Target,
                }),
                true,
            ),
            Some(value @ LengthInput::Value(_)) => (Some(value.clone()), true),
            _ if now.matched => (
                Some(LengthInput::Match {
                    what: MatchLength::Target,
                }),
                false,
            ),
            _ => (
                length_now.map(|l| LengthInput::Value(ValueInput::Number(l))),
                false,
            ),
        };
        if write_length {
            changed.push("length".to_owned());
        }
        let config = input.config.clone().or_else(|| info.config());
        if input.config.is_some() {
            changed.push("config".to_owned());
        }
        if !input.parameters.is_empty() {
            changed.push("parameters".to_owned());
        }
        // The part's other values as they are, the edit's over them.
        let mut parameters = std::collections::BTreeMap::new();
        if let Some(FeatureDef::DerivedPart(def)) = self
            .feature(part)
            .map(|e| crate::parts::as_input(&e.def, self.parameters()))
        {
            parameters = def.parameters;
        }
        parameters.remove(&length_name);
        parameters.extend(input.parameters.clone());
        // Placed anew so, on the frame of the same side.
        let place = PlaceInput {
            part: PartInput {
                source: Some(source),
                config: config.clone(),
                ..PartInput::default()
            },
            at: Some(PlaceAt {
                occurrence: path,
                geometry: Some(GeomRef::Mount {
                    feature: target_part,
                    name: now.frame.clone(),
                }),
                place: PlaceKind::Parallel,
                offset: Some(offset),
                from: Some(from),
                across,
                ..PlaceAt::default()
            }),
            rotate: Some(ValueInput::Number(rotate)),
            length,
            parameters,
            ..PlaceInput::default()
        };
        let plan = self.plan_place(&place)?;
        let Spot::Joint(planned) = &plan.spot else {
            return Err(refuse(format!("{} is no longer on a side", info.name)));
        };
        let planned = (**planned).clone();
        let slide = planned
            .slide_value
            .clone()
            .map(|(_, value, _)| value)
            .ok_or_else(|| refuse("the slide along the side is missing"))?;
        let new_length = plan.values.get(&length_name).cloned();
        let new_config = input.config.clone();
        let edited = input.parameters.clone();
        let label = format!("Edit {}", info.name);
        let joint = now.joint;
        self.apply(|state| {
            edit_joint(state, joint, |def| {
                def.b = planned.b.clone();
                def.a.frame_override = planned.own_override;
                def.angle = None;
                def.position.clear();
                def.position.insert(Motion::Tx, slide);
            })?;
            if write_length || new_config.is_some() || !edited.is_empty() {
                edit_part(state, part, |def| {
                    if let Some(row) = new_config {
                        def.config = Some(row);
                    }
                    for (name, value) in edited {
                        def.parameters.insert(name, value);
                    }
                    if write_length && let Some(value) = new_length {
                        def.parameters.insert(length_name, value);
                    }
                })?;
            }
            if let Some(record) = state
                .assembly
                .component_mut(component)
                .and_then(|c| c.build.as_mut())
            {
                record.from = Some(from);
            }
            Ok((label, changed))
        })
        .map(Some)
    }
}
