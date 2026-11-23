// SPDX-License-Identifier: MIT
//! Members along an edge (mitcad#1814; `commands.md`, "BUILD: timber
//! construction", *Members along an edge*): `build_place` with `along`
//! lays a member along a straight edge on a face beside it, its side flush
//! with the edge, its underside on the face, moved by four offsets. The
//! model's `edge_frame` keeps the edge, the face and the offsets; the
//! member's length is bound from its `start` frame to its `end` frame's
//! plane and its joint is on `start`, so that the member follows the body
//! the edge is on.
//!
//! The edge may be a sketch line (mitcad#1965, Profile's *Along sketch
//! lines*, `lines.rs`): the member then follows the line, aligned to it as
//! `align` says (centered by default), turned by `rotate` in quarter turns
//! and run the other way by `flip` (the edge frame `reversed`).

use std::collections::BTreeMap;
use std::f64::consts::FRAC_PI_2;

use serde::{Deserialize, Serialize};

use crate::geometry::inverse;
use crate::place::{
    Plan, RayPlan, add_bound, add_feature, make_part, move_before, refuse, value_in,
};
use crate::prelude::*;
use crate::timber::Lie;
use crate::{BuildItem, PlaceInput, Placed};
use mitcad_model::expr::Unit;
use mitcad_model::features::bound_length::{BoundMode, BoundTo, RayDirection};
use mitcad_model::features::derived_part::EndCut;
use mitcad_model::features::edge_frame::{
    END, EdgeFrameDef, EdgeOffsets, EdgeRun, START, edge_run,
};
use mitcad_model::features::{
    FeatureDef, FrameOverride, GeomRef, JointDef, JointOrigin, OccurrencePath, ValueInput,
};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::joints::{JointKind, SlideAxis};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::transform::Transform;
use mitcad_model::{DocState, Document, ModelError};

/// Where a member along an edge goes (`build_place`'s `along`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlongEdge {
    /// A path from the BUILD host to the component the edge and the face
    /// are in (uids or names); empty for its own geometry.
    #[serde(default)]
    pub occurrence: String,
    /// A straight edge, or a sketch line (mitcad#1965).
    pub edge: GeomRef,
    /// The planar face beside it that the member lies on: it decides which
    /// side of the edge the member is on. None along a sketch line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<GeomRef>,
    /// Along a sketch line: how the section sits on the line (mitcad#1965);
    /// centered when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    /// Across the face, away from the edge: 0 flush, negative out over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset: Option<ValueInput>,
    /// Along the edge at its start: positive shortens, negative runs past.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ValueInput>,
    /// Along the edge at its end, likewise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ValueInput>,
    /// Off the face along its normal: 0 rests on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lift: Option<ValueInput>,
}

/// How a member's section sits on a sketch line (mitcad#1965): centered on
/// it, or with a side or a corner on it. Left and right look along the line
/// with the sketch's normal up; above is on the normal's side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Center,
    /// Centered across the line, resting on the sketch's plane.
    Above,
    /// Centered across the line, hanging under the sketch's plane.
    Below,
    LeftAbove,
    RightAbove,
    LeftBelow,
    RightBelow,
}

impl Align {
    pub const ALL: [Align; 7] = [
        Align::Center,
        Align::Above,
        Align::Below,
        Align::LeftAbove,
        Align::RightAbove,
        Align::LeftBelow,
        Align::RightBelow,
    ];

    /// Across the line (1 left, -1 right, 0 centered) and off the plane (1
    /// above, -1 below, 0 centered).
    fn sides(self) -> (i8, i8) {
        match self {
            Align::Center => (0, 0),
            Align::Above => (0, 1),
            Align::Below => (0, -1),
            Align::LeftAbove => (1, 1),
            Align::RightAbove => (-1, 1),
            Align::LeftBelow => (1, -1),
            Align::RightBelow => (-1, -1),
        }
    }

    /// The alignment an edge frame keeps (`as_str`), when it is one.
    pub fn parse(text: &str) -> Option<Align> {
        Align::ALL.into_iter().find(|a| a.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Align::Center => "center",
            Align::Above => "above",
            Align::Below => "below",
            Align::LeftAbove => "left_above",
            Align::RightAbove => "right_above",
            Align::LeftBelow => "left_below",
            Align::RightBelow => "right_below",
        }
    }
}

/// Whether an edge reference is a sketch line (mitcad#1965).
pub fn is_line(edge: &GeomRef) -> bool {
    matches!(edge, GeomRef::SketchCurves { .. })
}

/// The shift of the `start` frame (along its x across the line and its y
/// up) that puts a member's section, turned by `angle` about the line
/// (quarter turns), where `align` says (mitcad#1965). The section is the
/// member's box in its own coordinates.
pub(crate) fn line_shift(section: Option<&BoundingBox>, angle: f64, align: Align) -> [f64; 3] {
    let Some(b) = section else {
        return [0.0; 3];
    };
    let (s, c) = angle.sin_cos();
    let (mut x0, mut x1, mut y0, mut y1) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for (px, py) in [
        (b.min[0], b.min[1]),
        (b.max[0], b.min[1]),
        (b.min[0], b.max[1]),
        (b.max[0], b.max[1]),
    ] {
        let (x, y) = (c * px - s * py, s * px + c * py);
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    let pick = |side: i8, low: f64, high: f64| match side {
        0 => -(low + high) / 2.0,
        1 => -low,
        _ => -high,
    };
    let (across, up) = align.sides();
    let round = |v: f64| (v * 1e9).round() / 1e9;
    [round(pick(across, x0, x1)), round(pick(up, y0, y1)), 0.0]
}

/// How a member along an edge is turned and shifted on its `start` frame:
/// along a body's edge by its lie (`lie_on_edge`), along a sketch line by
/// its rotation and alignment (`line_shift`).
pub(crate) fn turn_and_shift(
    section: Option<&BoundingBox>,
    lie: Lie,
    line: Option<(f64, Align)>,
) -> (f64, [f64; 3]) {
    match line {
        Some((angle, align)) => (angle, line_shift(section, angle, align)),
        None => lie_on_edge(section, lie),
    }
}

/// The joint's turn about the edge and the shift of the `start` frame
/// along its axes (across the face, up from it) that put a member's side
/// flush with the edge and its underside on the face, lying as `lie` says
/// (as members by two points lie): its section's smaller dimension up
/// when flat, the larger one on edge. The section is the member's box in
/// its own coordinates (its width along X, thickness along Y).
pub(crate) fn lie_on_edge(section: Option<&BoundingBox>, lie: Lie) -> (f64, [f64; 3]) {
    let Some(b) = section else {
        return (0.0, [0.0; 3]);
    };
    let (w, t) = (b.max[0] - b.min[0], b.max[1] - b.min[1]);
    let y_up = match lie {
        Lie::Flat => w >= t,
        Lie::Edge => w < t,
    };
    if y_up {
        // X across the face, Y up.
        (0.0, [-b.min[0], -b.min[1], 0.0])
    } else {
        // Turned a quarter about the edge: X up, Y across the face the
        // other way.
        (FRAC_PI_2, [b.max[1], -b.min[0], 0.0])
    }
}

/// Where a member along an edge goes in the BUILD host: its end A frame
/// on the edge frame's `start` (in the host's coordinates) as its joint
/// puts it.
pub(crate) fn along_placement(
    start: &Transform,
    mounts: &BTreeMap<String, Transform>,
    section: Option<&BoundingBox>,
    lie: Lie,
    line: Option<(f64, Align)>,
) -> Transform {
    let (angle, shift) = turn_and_shift(section, lie, line);
    let own = mounts.get("end_a").copied().unwrap_or(Transform::IDENTITY);
    start
        .after(&Transform::translation(shift))
        .after(&mitcad_model::internal::joints::alignment(angle, 0.0, true))
        .after(&inverse(&own))
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait AlongCrateOps<K: Kernel> {
    /// The edge frame `along` gives, now: the path to the edge's component
    /// and the frames in the BUILD host's coordinates; `reversed` runs a
    /// sketch line's the other way (mitcad#1965).
    fn plan_along(
        &self,
        along: &AlongEdge,
        reversed: bool,
    ) -> Result<(Vec<OccurrenceUid>, EdgeRun), ModelError>;

    /// Places a member along an edge (`build_place` with `along`).
    fn build_place_along(
        &mut self,
        input: &PlaceInput,
        along: &AlongEdge,
    ) -> Result<Placed, ModelError>;
}

impl<K: Kernel> AlongCrateOps<K> for Document<K> {
    fn plan_along(
        &self,
        along: &AlongEdge,
        reversed: bool,
    ) -> Result<(Vec<OccurrenceUid>, EdgeRun), ModelError> {
        let path = self.path_from_host(&along.occurrence)?;
        let value = |v: &Option<ValueInput>| match v {
            Some(v) => value_in(self, v, Unit::MM),
            None => Ok(0.0),
        };
        let offsets = EdgeOffsets {
            inset: value(&along.inset)?,
            start: value(&along.start)?,
            end: value(&along.end)?,
            lift: value(&along.lift)?,
            reversed: reversed && is_line(&along.edge),
        };
        let local = edge_run(
            &mut self.resolver(),
            &along.edge,
            along.face.as_ref(),
            &offsets,
        )
        .map_err(refuse)?;
        let placement = self.path_transform(&path);
        Ok((
            path,
            EdgeRun {
                start: placement.after(&local.start),
                end: placement.after(&local.end),
                ..local
            },
        ))
    }

    fn build_place_along(
        &mut self,
        input: &PlaceInput,
        along: &AlongEdge,
    ) -> Result<Placed, ModelError> {
        let plan = self.plan_place_between(input, None)?;
        let (path, run) = self.plan_along(along, input.flip)?;
        let active = self.build_host();
        self.state().editable(active)?;
        let label = format!(
            "Place {}",
            if plan.designation.is_empty() {
                plan.source.def.label()
            } else {
                plan.designation.clone()
            }
        );
        let lie = input.lie.unwrap_or_default();
        let line = line_turn(input, along, self)?;
        self.apply(|state| {
            let placed = make_along(
                state, &plan, along, &path, run.length, lie, line, input.flip, active,
            )?;
            Ok((label, placed))
        })
    }
}

/// A member along a sketch line's turn about it (its rotation, radians, in
/// quarter turns) and its alignment (mitcad#1965); None along a body's
/// edge.
pub(crate) fn line_turn<K: Kernel>(
    input: &PlaceInput,
    along: &AlongEdge,
    doc: &Document<K>,
) -> Result<Option<(f64, Align)>, ModelError> {
    if !is_line(&along.edge) {
        return Ok(None);
    }
    let rotate = match &input.rotate {
        Some(value) => value_in(doc, value, Unit::DEG)?,
        None => 0.0,
    };
    let quarter = rotate / FRAC_PI_2;
    if (quarter - quarter.round()).abs() > 1e-6 {
        return Err(refuse("the rotation is 0, 90, 180 or 270 degrees"));
    }
    Ok(Some((
        quarter.round().rem_euclid(4.0) * FRAC_PI_2,
        along.align.unwrap_or_default(),
    )))
}

/// Refuses the fields a member along an edge does not take: the edge and
/// its offsets give its place and its length.
pub(crate) fn check_along(input: &PlaceInput) -> Result<(), ModelError> {
    if let Some(along) = &input.along {
        if is_line(&along.edge) {
            if along.face.is_some() {
                return Err(refuse(
                    "a member along a sketch line lies on the sketch's plane: leave out `face`",
                ));
            }
            if input.lie.is_some() {
                return Err(refuse(
                    "a member along a sketch line is turned by Rotate: leave out `lie`",
                ));
            }
        } else if along.align.is_some() {
            return Err(refuse(
                "a member along a body's edge is flush with it: `align` is for sketch lines",
            ));
        }
    }
    let refused = if input.to.is_some() {
        Some("to")
    } else if input.length.is_some() {
        Some("length")
    } else if input.spacing.is_some() {
        Some("spacing")
    } else if input.mitre.is_some() {
        Some("mitre")
    } else if input.tilt.is_some() {
        Some("tilt")
    } else if input.at.as_ref().is_some_and(|a| a.geometry.is_some()) {
        Some("at")
    } else {
        None
    };
    match refused {
        Some(field) => Err(refuse(format!(
            "a member along an edge takes its place and length from the edge and its offsets: \
             leave out `{field}`"
        ))),
        None => Ok(()),
    }
}

/// Makes a member along an edge as `plan` says (the part where it goes),
/// then its edge frame before its derived part, its length bound from the
/// frame `start` to the plane of `end`, and its rigid joint on `start`;
/// the edge frame goes into the record's `adds`, so that it goes with the
/// member.
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_along(
    state: &mut DocState,
    plan: &Plan,
    along: &AlongEdge,
    path: &[OccurrenceUid],
    length: f64,
    lie: Lie,
    line: Option<(f64, Align)>,
    flip: bool,
    active: ComponentUid,
) -> Result<Placed, ModelError> {
    let mut placed = make_part(state, plan, active)?;
    let (Some(component), Some(occurrence)) = (placed.component, placed.occurrence) else {
        return Err(refuse("the member was not made"));
    };
    let part: FeatureUid = state
        .assembly
        .component(component)
        .and_then(|c| c.build.as_ref())
        .and_then(|b| b.part)
        .ok_or_else(|| refuse("the member has no derived part"))?;
    let length_name = plan
        .source
        .meta
        .length
        .as_ref()
        .map(|l| l.parameter.clone())
        .ok_or_else(|| refuse("the part is not cut to length"))?;
    // The edge frame, before the derived part.
    let def = EdgeFrameDef {
        occurrence: OccurrencePath(path.to_vec()),
        edge: along.edge.clone(),
        face: along.face.clone(),
        inset: along.inset.clone(),
        start: along.start.clone(),
        end: along.end.clone(),
        lift: along.lift.clone(),
        reversed: flip && line.is_some(),
        align: line.map(|(_, align)| align.as_str().to_owned()),
        start_meets: None,
        end_meets: None,
        start_angle: None,
        end_angle: None,
    };
    let frames = add_feature(state, FeatureDef::EdgeFrame(def), active)?;
    move_before(state, frames, part);
    let mount = |name: &str| GeomRef::Mount {
        feature: frames,
        name: name.to_owned(),
    };
    // Its length from `start` to the plane of `end`.
    let ray = RayPlan {
        from: JointOrigin {
            occurrence: OccurrencePath::default(),
            geometry: mount(START),
            frame_override: None,
        },
        shift: [0.0; 3],
        shift_by_joint: false,
        slide_extra: 0.0,
        direction: RayDirection::default(),
        start: plan.placement.translation,
        along: plan.placement.apply_vector([0.0, 0.0, 1.0]),
    };
    let to = BoundTo {
        occurrence: OccurrencePath::default(),
        geometry: mount(END),
    };
    let bound = add_bound(
        state,
        active,
        part,
        &length_name,
        &ray,
        None,
        &to,
        BoundMode::Face,
        None,
        length,
    )?;
    // Held by its joint, no longer grounded.
    if let Some(o) = state.assembly.occurrence_mut(occurrence) {
        o.grounded = false;
    }
    let (angle, shift) = turn_and_shift(plan.bounds.as_ref(), lie, line);
    let joint_def = JointDef {
        kind: JointKind::Rigid,
        slide_axis: SlideAxis::Z,
        a: JointOrigin {
            occurrence: OccurrencePath(vec![occurrence]),
            geometry: GeomRef::Mount {
                feature: part,
                name: "end_a".to_owned(),
            },
            frame_override: None,
        },
        b: JointOrigin {
            occurrence: OccurrencePath::default(),
            geometry: mount(START),
            frame_override: Some(FrameOverride {
                shift: Some(shift),
                ..FrameOverride::default()
            }),
        },
        offset: None,
        angle: (angle.abs() > 1e-12).then_some(ValueInput::Number(angle)),
        flip: true,
        limits: BTreeMap::new(),
        position: BTreeMap::new(),
        start: BTreeMap::new(),
    };
    let joint = add_feature(state, FeatureDef::Joint(joint_def), active)?;
    if let Some(record) = state
        .assembly
        .component_mut(component)
        .and_then(|c| c.build.as_mut())
    {
        record.joint = Some(joint);
        record.bound = vec![bound];
        record.adds.push(BuildItem::Feature(frames));
    }
    placed.features.insert(0, bound);
    placed.features.insert(0, frames);
    placed.features.push(joint);
    placed.joint = Some(joint);
    placed.bound = vec![bound];
    placed.adds = vec![BuildItem::Feature(frames)];
    Ok(placed)
}

// Editing (mitcad#1814, point 7): a placed member's edge, face, offsets,
// lie and size.

/// What `build_edit`'s `along` changes of a member along an edge: each
/// field left out keeps what it has.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlongEdit {
    /// A path from the BUILD host to the edge's and the face's component.
    #[serde(default)]
    pub occurrence: Option<String>,
    #[serde(default)]
    pub edge: Option<GeomRef>,
    #[serde(default)]
    pub face: Option<GeomRef>,
    #[serde(default)]
    pub inset: Option<ValueInput>,
    #[serde(default)]
    pub start: Option<ValueInput>,
    #[serde(default)]
    pub end: Option<ValueInput>,
    #[serde(default)]
    pub lift: Option<ValueInput>,
    /// Along a sketch line: how the section sits on it (mitcad#1965).
    #[serde(default)]
    pub align: Option<Align>,
}

/// A member placed along an edge: its edge frame, its joint and how it
/// lies now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlongPart {
    pub frames: FeatureUid,
    pub joint: FeatureUid,
    pub lie: Lie,
    /// Along a sketch line (mitcad#1965): how its section sits on the line,
    /// its turn about it (radians) and whether it runs from the line's end.
    pub line: Option<AlongLine>,
}

/// How a member along a sketch line sits on it (mitcad#1965).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlongLine {
    pub align: Align,
    pub rotate: f64,
    pub reversed: bool,
}

/// The alignment whose shift is the joint's (mitcad#1965), the nearest one.
fn align_of(section: Option<&BoundingBox>, angle: f64, shift: [f64; 3]) -> Align {
    Align::ALL
        .into_iter()
        .min_by(|a, b| {
            let d = |align: Align| {
                let s = line_shift(section, angle, align);
                (s[0] - shift[0]).abs() + (s[1] - shift[1]).abs()
            };
            d(*a).total_cmp(&d(*b))
        })
        .unwrap_or_default()
}

/// How a member lies by its joint's turn about the edge and its section
/// (as [`lie_on_edge`] turns it): unturned, its section's X runs across
/// the face.
fn lie_of(angle: f64, section: Option<&BoundingBox>) -> Lie {
    let wide = section.is_none_or(|b| b.max[0] - b.min[0] >= b.max[1] - b.min[1]);
    if (angle.abs() < 1e-6) == wide {
        Lie::Flat
    } else {
        Lie::Edge
    }
}

/// The member along an edge a BUILD component is, when it is one: its
/// joint is on an edge frame's `start`.
pub fn along_of<K: Kernel>(doc: &Document<K>, component: ComponentUid) -> Option<AlongPart> {
    let info = doc.part_info(component)?;
    let joint = info.record.joint?;
    let FeatureDef::Joint(j) = &doc.feature(joint)?.def else {
        return None;
    };
    let GeomRef::Mount { feature, name } = &j.b.geometry else {
        return None;
    };
    let FeatureDef::EdgeFrame(frames) = &doc.feature(*feature)?.def else {
        return None;
    };
    if name != START {
        return None;
    }
    let angle = j
        .angle
        .and_then(|id| doc.parameters().value(id))
        .unwrap_or(0.0);
    let section = doc.part_bounds(component);
    let line = is_line(&frames.edge).then(|| AlongLine {
        // As placed, else by the joint's shift (an older frame).
        align: frames
            .align
            .as_deref()
            .and_then(Align::parse)
            .unwrap_or_else(|| {
                align_of(
                    section.as_ref(),
                    angle,
                    j.b.frame_override.and_then(|o| o.shift).unwrap_or([0.0; 3]),
                )
            }),
        rotate: angle,
        reversed: frames.reversed,
    });
    Some(AlongPart {
        frames: *feature,
        joint,
        lie: lie_of(angle, section.as_ref()),
        line,
    })
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait AlongEditCrateOps<K: Kernel> {
    /// `build_edit` of a member along an edge with `along`, `lie` or
    /// `config` (and with them `parameters` and `ends`): its edge frame,
    /// its row and values, and its joint's turn and shift for the lie and
    /// the section, in one undo step. None for another part or edit.
    fn edit_along(
        &mut self,
        component: ComponentUid,
        input: &crate::EditInput,
    ) -> Result<Option<Vec<String>>, ModelError>;
}

impl<K: Kernel> AlongEditCrateOps<K> for Document<K> {
    fn edit_along(
        &mut self,
        component: ComponentUid,
        input: &crate::EditInput,
    ) -> Result<Option<Vec<String>>, ModelError> {
        let Some(along) = along_of(self, component) else {
            return Ok(None);
        };
        // Along a sketch line (mitcad#1965): turned by Rotate and run the
        // other way by Flip, as it was placed.
        let on_line = along.line.is_some();
        let turned = on_line && (input.rotate.is_some() || input.flip.is_some());
        if input.along.is_none() && input.lie.is_none() && input.config.is_none() && !turned {
            return Ok(None);
        }
        if on_line && input.lie.is_some() {
            return Err(refuse(
                "a member along a sketch line is turned by Rotate: edit `rotate`, not `lie`",
            ));
        }
        let other = [
            ("length", input.length.is_some()),
            ("rotate", input.rotate.is_some() && !on_line),
            ("flip", input.flip.is_some() && !on_line),
            ("at", input.at.is_some()),
            ("tilt", input.tilt.is_some()),
            ("machining", input.machining.is_some()),
            ("cuts", input.cuts.is_some()),
            ("to", input.to.is_some()),
            ("grade", input.grade.is_some()),
        ];
        if let Some((field, _)) = other.iter().find(|(_, given)| *given) {
            return Err(refuse(format!(
                "a member along an edge takes its place and length from the edge and its offsets: \
                 edit `{field}` apart from `along`, `lie` and `config`"
            )));
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
        if let Some(config) = &input.config
            && let Some(def) = &info.source
            && def
                .design
                .state()
                .map_err(refuse)?
                .configurations
                .row(config)
                .is_none()
        {
            return Err(refuse(format!(
                "{} has no configuration {config}",
                def.label()
            )));
        }
        // The edge frame as it is, with the changes.
        let frames_entry = self
            .feature(along.frames)
            .ok_or(ModelError::NoSuchFeature(along.frames))?;
        let FeatureDef::EdgeFrame(mut frames) =
            crate::parts::as_input(&frames_entry.def, self.parameters())
        else {
            return Err(refuse("the member's edge frame is gone"));
        };
        let mut changed = Vec::new();
        if let Some(edit) = &input.along {
            if let Some(text) = &edit.occurrence {
                frames.occurrence = OccurrencePath(self.path_from_host(text)?);
            }
            if let Some(edge) = &edit.edge {
                frames.edge = edge.clone();
            }
            if let Some(face) = &edit.face {
                frames.face = Some(face.clone());
            }
            for (slot, value) in [
                (&mut frames.inset, &edit.inset),
                (&mut frames.start, &edit.start),
                (&mut frames.end, &edit.end),
                (&mut frames.lift, &edit.lift),
            ] {
                if let Some(value) = value {
                    *slot = Some(value.clone());
                }
            }
            changed.push("along".to_owned());
        }
        let lie = input.lie.unwrap_or(along.lie);
        if input.lie.is_some() {
            changed.push("lie".to_owned());
        }
        // Along a sketch line: its alignment, turn and direction.
        let line = along.line.map(|now| {
            let align = input
                .along
                .as_ref()
                .and_then(|e| e.align)
                .unwrap_or(now.align);
            let reversed = input.flip.unwrap_or(now.reversed);
            (align, reversed, now.rotate)
        });
        if let Some((_, reversed, _)) = line
            && reversed != frames.reversed
        {
            frames.reversed = reversed;
            changed.push("flip".to_owned());
        }
        if let Some((align, _, _)) = line {
            frames.align = Some(align.as_str().to_owned());
        }
        if input.rotate.is_some() && on_line {
            changed.push("rotate".to_owned());
        }
        let edit_frames = input.along.is_some() || changed.iter().any(|c| c == "flip");
        let config = input.config.clone().or_else(|| info.config());
        if input.config.is_some() {
            changed.push("config".to_owned());
        }
        // The member as it would be placed so: the edge, the face and the
        // offsets checked, its section that of the row.
        let place = PlaceInput {
            part: crate::PartInput {
                source: Some(source),
                config,
                ..crate::PartInput::default()
            },
            along: Some(AlongEdge {
                occurrence: frames.occurrence.to_string(),
                edge: frames.edge.clone(),
                face: frames.face.clone(),
                align: line.map(|(align, _, _)| align),
                inset: frames.inset.clone(),
                start: frames.start.clone(),
                end: frames.end.clone(),
                lift: frames.lift.clone(),
            }),
            lie: (!on_line).then_some(lie),
            rotate: match (&input.rotate, line) {
                (Some(rotate), _) if on_line => Some(rotate.clone()),
                (_, Some((_, _, now))) => Some(ValueInput::Number(now)),
                _ => None,
            },
            flip: frames.reversed,
            ..PlaceInput::default()
        };
        let plan = self.plan_place(&place)?;
        let turn = match &place.along {
            Some(a) => line_turn(&place, a, self)?,
            None => None,
        };
        let (angle, shift) = turn_and_shift(plan.bounds.as_ref(), lie, turn);
        let joint_entry = self
            .feature(along.joint)
            .ok_or(ModelError::NoSuchFeature(along.joint))?;
        let FeatureDef::Joint(mut joint) =
            crate::parts::as_input(&joint_entry.def, self.parameters())
        else {
            return Err(refuse("the member's joint is gone"));
        };
        joint.angle = (angle.abs() > 1e-12).then_some(ValueInput::Number(angle));
        joint.b.frame_override = Some(FrameOverride {
            shift: Some(shift),
            ..FrameOverride::default()
        });
        let parameters = input.parameters.clone();
        let ends = input.ends.clone();
        if !parameters.is_empty() {
            changed.push("parameters".to_owned());
        }
        if ends.is_some() {
            changed.push("ends".to_owned());
        }
        let new_config = input.config.clone();
        let label = format!("Edit {}", info.name);
        let joint_uid = along.joint;
        let frames_uid = along.frames;
        self.apply(|state| {
            if edit_frames {
                state.edit_feature(frames_uid, &FeatureDef::EdgeFrame(frames))?;
            }
            if new_config.is_some() || !parameters.is_empty() || ends.is_some() {
                crate::place::edit_part(state, part, |def| {
                    if let Some(row) = new_config {
                        def.config = Some(row);
                    }
                    for (name, value) in parameters {
                        def.parameters.insert(name, value);
                    }
                    if let Some(ends) = ends {
                        let cut = |c: crate::EndCutInput| EndCut {
                            angle: c.angle,
                            tilt: c.tilt,
                        };
                        if let Some(a) = ends.a {
                            def.ends.a = a.map(cut);
                        }
                        if let Some(b) = ends.b {
                            def.ends.b = b.map(cut);
                        }
                    }
                })?;
            }
            state.edit_feature(joint_uid, &FeatureDef::Joint(joint))?;
            Ok((label, changed))
        })
        .map(Some)
    }
}
