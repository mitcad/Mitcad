// SPDX-License-Identifier: MIT
//! Placing and editing BUILD parts (`build_place`, `build_edit`): the
//! source a part is made from, where it goes (a profile's side or end, a
//! frame it mates with, the first part's plane, point or face), the joint
//! that places it, its bound length and its mitres. Everything is planned
//! against the document as it is, then made in one undo step; a refusal
//! changes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::geometry::{
    V3, add, angle_between, cross, dot, inverse, parts, perpendicular, scale, sub, unit, x_axis,
    z_axis,
};
use crate::parts::{
    PartInfo, as_input, default_value, source_option_values, source_state_with_options,
};
use crate::prelude::*;
use crate::{BuildMeta, BuildRecord, BuildRole, LibrarySourceDef, SourceId};
// Stored in the design file, so kept in the model (mitcad#777).
pub use mitcad_model::build::SideFrom;
use mitcad_model::expr::Unit;
use mitcad_model::features::bound_length::{
    BoundFrom, BoundLengthDef, BoundMode, BoundTo, RayDirection, ShiftAxis,
};
use mitcad_model::features::derived_part::{DerivedPartDef, EndCut, Ends, Tilt};
use mitcad_model::features::extrude::Operation;
use mitcad_model::features::{
    FeatureDef, FeatureEntry, FrameOverride, GeomRef, JointDef, JointOrigin, OccurrencePath,
    ValueInput,
};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
pub use mitcad_model::internal::build::value_in;
use mitcad_model::internal::features::evaluate_source;
use mitcad_model::joints::{JointKind, Motion, SlideAxis};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::library::LibraryRequest;
use mitcad_model::parameters::ParamId;
use mitcad_model::part_options::{Naming, PartValue, combination};
use mitcad_model::transform::Transform;
use mitcad_model::{DocState, Document, ModelError};

pub(crate) fn refuse(message: impl Into<String>) -> ModelError {
    ModelError::Invalid(message.into())
}

/// How near a typed length must end to another member's face or outer
/// side to meet it as a corner (mitcad#492), mm: half the 0.1 mm a length
/// typed from the one shown is rounded to.
const MEETS: f64 = 0.05;

// Inputs.

/// The part a command places: a library's component at a version, or a
/// source the design has.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartInput {
    #[serde(default)]
    pub library: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub component: Option<String>,
    #[serde(default)]
    pub config: Option<String>,
    #[serde(default)]
    pub source: Option<SourceId>,
    /// Values of its options (mitcad#801): expressions of the design's
    /// parameters, as the command's `parameters` (which win for a name in
    /// both).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, ValueInput>,
}

/// Whether a pick on a profile goes on its side or on its end.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    #[default]
    Auto,
    Side,
    End,
}

/// An end of a part (a hole at it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum EndFrom {
    #[default]
    #[serde(rename = "a")]
    A,
    #[serde(rename = "b")]
    B,
}

/// Where a part goes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceAt {
    /// A path from the BUILD host (uids or names); empty for its own
    /// geometry.
    #[serde(default)]
    pub occurrence: String,
    #[serde(default)]
    pub geometry: Option<GeomRef>,
    /// Where it was picked, in the BUILD host's coordinates.
    #[serde(default)]
    pub point: Option<V3>,
    #[serde(default)]
    pub place: PlaceKind,
    /// On a side: the offset from `from` (mitcad#417); on a turned end:
    /// the new part's end from the target's outer side.
    #[serde(default)]
    pub offset: Option<ValueInput>,
    /// What `offset` is measured from (end A when left out).
    #[serde(default)]
    pub from: Option<SideFrom>,
    /// On an end: the new part's axis, in the BUILD host's
    /// coordinates; when left out, a click on an end face decides.
    #[serde(default)]
    pub direction: Option<V3>,
    /// The app's least snap distance, mm (about 12 px at the click's
    /// depth, mitcad#417).
    #[serde(default)]
    pub snap: Option<f64>,
}

/// What a length is bound to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindInput {
    #[serde(default)]
    pub occurrence: String,
    pub geometry: GeomRef,
}

/// A length: a value, or bound to what the part's ray meets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LengthInput {
    Value(ValueInput),
    Bound {
        bind: Box<BindInput>,
        #[serde(default)]
        mode: BoundMode,
        #[serde(default)]
        add: Option<ValueInput>,
    },
}

/// Which ends a mitre cuts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MitreCut {
    #[default]
    New,
    Existing,
    Shared,
    /// No mitre (on a corner not at 90 degrees, where the default is
    /// `shared`).
    None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MitreInput {
    #[serde(default)]
    pub angle: Option<ValueInput>,
    #[serde(default)]
    pub cut: MitreCut,
}

/// The fields of `build_place` (and of `build_preview` for one).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceInput {
    pub part: PartInput,
    #[serde(default)]
    pub at: Option<PlaceAt>,
    #[serde(default)]
    pub rotate: Option<ValueInput>,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub length: Option<LengthInput>,
    #[serde(default)]
    pub mitre: Option<MitreInput>,
    /// On a side (mitcad#444): the lean along the host, an angle, positive
    /// towards its end B; 0 or left out: square.
    #[serde(default)]
    pub tilt: Option<ValueInput>,
    #[serde(default)]
    pub parameters: BTreeMap<String, ValueInput>,
    #[serde(default)]
    pub name: Option<String>,
    // Timber construction (mitcad#478).
    /// A member by two points: where it ends (`at.point` its start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<crate::timber::PointTo>,
    /// How a member by two points lies about its axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lie: Option<crate::timber::Lie>,
    /// The member repeated as a spacing set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<crate::spacing::SpacingInput>,
    /// The part's own grade (a timber member's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<String>,
}

/// An end's cut in `build_edit`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndCutInput {
    pub angle: ValueInput,
    #[serde(default)]
    pub tilt: Option<Tilt>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndsInput {
    /// null squares the end; left out keeps it.
    #[serde(default, deserialize_with = "some")]
    pub a: Option<Option<EndCutInput>>,
    #[serde(default, deserialize_with = "some")]
    pub b: Option<Option<EndCutInput>>,
}

fn some<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditAt {
    #[serde(default)]
    pub offset: Option<ValueInput>,
    #[serde(default)]
    pub from: Option<SideFrom>,
    /// A member by two points: its start (mitcad#478).
    #[serde(default)]
    pub point: Option<V3>,
}

/// The fields of `build_edit`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditInput {
    pub component: String,
    #[serde(default)]
    pub at: Option<EditAt>,
    #[serde(default)]
    pub rotate: Option<ValueInput>,
    #[serde(default)]
    pub flip: Option<bool>,
    #[serde(default)]
    pub length: Option<LengthInput>,
    #[serde(default)]
    pub ends: Option<EndsInput>,
    /// A part on a side (mitcad#444): its tilt along the host; 0 squares it.
    #[serde(default)]
    pub tilt: Option<ValueInput>,
    #[serde(default)]
    pub config: Option<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, ValueInput>,
    /// The part's own end machining (not a connector's): replaces what it
    /// had; an empty list takes it away.
    #[serde(default)]
    pub machining: Option<Vec<OwnMachining>>,
    // Timber construction (mitcad#478).
    /// The whole spacing set the part is in: its fields changed.
    #[serde(default)]
    pub spacing: Option<crate::spacing::SpacingInput>,
    /// The part's own grade; null clears it.
    #[serde(default, deserialize_with = "some")]
    pub grade: Option<Option<String>>,
    /// `refit`: the part's joinery cut anew from where the members are.
    #[serde(default)]
    pub cuts: Option<String>,
    /// A member by two points: its end and its lie again.
    #[serde(default)]
    pub to: Option<crate::timber::PointTo>,
    #[serde(default)]
    pub lie: Option<crate::timber::Lie>,
    // Timber frames (mitcad#479).
    /// The timber frame the part is in, laid out again with these fields
    /// over its definition's.
    #[serde(default)]
    pub frame: Option<crate::frames::FrameInput>,
}

/// A hole at one of a part's ends.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnMachining {
    pub end: EndFrom,
    pub hole: crate::HoleMeta,
}

/// What a placing command made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placed {
    pub component: Option<ComponentUid>,
    pub occurrence: Option<OccurrenceUid>,
    pub name: String,
    pub features: Vec<FeatureUid>,
    pub joint: Option<FeatureUid>,
    pub bound: Vec<FeatureUid>,
    pub adds: Vec<crate::BuildItem>,
    pub warnings: Vec<String>,
    /// `side`, `end`, `frame`, `geometry` or `grounded`.
    pub place: &'static str,
    /// A panel's fit (`build_panel`): false with the reason.
    pub fits: Option<(bool, String)>,
    /// A spacing set's members (mitcad#478), the first the answer's part.
    pub set: Vec<ComponentUid>,
}

// Planning.

/// The source a command uses: one of the design's, or one it adds.
#[derive(Debug, Clone)]
pub struct SourceUse {
    pub def: LibrarySourceDef,
    pub new: bool,
    pub meta: BuildMeta,
    pub config: Option<String>,
}

/// The part a pick is on.
#[derive(Debug, Clone)]
pub struct Target {
    pub path: Vec<OccurrenceUid>,
    pub info: PartInfo,
    /// From its coordinates into the BUILD host's.
    pub placement: Transform,
}

impl Target {
    pub(crate) fn part(&self) -> Result<FeatureUid, ModelError> {
        self.info
            .part
            .ok_or_else(|| refuse(format!("{} has no derived part", self.info.name)))
    }

    /// A frame in the BUILD host's coordinates.
    pub(crate) fn frame(&self, name: &str) -> Option<Transform> {
        Some(self.placement.after(self.info.mount(name)?))
    }

    pub(crate) fn origin(&self, name: &str) -> Result<JointOrigin, ModelError> {
        Ok(JointOrigin {
            occurrence: OccurrencePath(self.path.clone()),
            geometry: GeomRef::Mount {
                feature: self.part()?,
                name: name.to_owned(),
            },
            frame_override: None,
        })
    }
}

/// The ray of a bound length before the joint exists: its start, and
/// whether the joint's slide along the slot shifts it.
#[derive(Debug, Clone)]
pub(crate) struct RayPlan {
    pub from: JointOrigin,
    pub shift: [f64; 3],
    pub shift_by_joint: bool,
    /// Added to the joint's slide when it shifts the start (a tilted
    /// part's far end, mitcad#444).
    pub slide_extra: f64,
    pub direction: RayDirection,
    /// The start and direction now, in the BUILD host's coordinates (a
    /// frame's closing reads them, mitcad#485).
    pub start: V3,
    pub along: V3,
}

/// How the new part is placed.
#[derive(Debug, Clone)]
pub struct JointPlan {
    pub kind: JointKind,
    pub slide: SlideAxis,
    /// The new part's frame that mates.
    pub own: String,
    pub b: JointOrigin,
    /// b's frame in the BUILD host's coordinates, at the slide.
    pub frame_b: Transform,
    pub flip: bool,
    pub slide_value: Option<(Motion, ValueInput, f64)>,
    pub(crate) ray: Option<RayPlan>,
    /// On a side: how the part stands along the slot (mitcad#394).
    pub side: Option<SidePlan>,
    /// On an end: straight or turned, and the corner's mitre (mitcad#417).
    pub end: Option<EndPlan>,
    /// A tilted part on a side (mitcad#444): its end frame on the slot
    /// turned (joint side a) and its end cuts (end B, angle, tilt).
    pub own_override: Option<FrameOverride>,
    pub cuts: Vec<(bool, f64, Tilt)>,
    /// What a slide's offset is measured from (kept in the record).
    pub from: Option<SideFrom>,
    pub warnings: Vec<String>,
}

/// The new part as planning sees it: its frames and box in its own
/// coordinates, and its rotation about the axis.
pub(crate) struct NewPart<'a> {
    pub mounts: &'a BTreeMap<String, Transform>,
    pub bounds: Option<&'a BoundingBox>,
    pub rotate: f64,
    /// Its tilt on a side (mitcad#444), radians.
    pub tilt: f64,
}

/// How far a part reaches along the x axis of the frame its frame `own`
/// mates with (turned by `rotate`, flipped as joints flip): the least and
/// the most, 0 without a box.
pub(crate) fn reach_along_x(
    bounds: Option<&BoundingBox>,
    own: &Transform,
    rotate: f64,
    flip: bool,
) -> (f64, f64) {
    let Some(bounds) = bounds else {
        return (0.0, 0.0);
    };
    let to_frame =
        mitcad_model::internal::joints::alignment(rotate, 0.0, flip).after(&inverse(own));
    bounds
        .corners()
        .map(|c| to_frame.apply_point(c)[0])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
            (lo.min(x), hi.max(x))
        })
}

/// A value moved by `by` mm: a number added to, an expression with the
/// shift written after it.
fn shifted(value: &ValueInput, by: f64) -> ValueInput {
    match value {
        ValueInput::Number(n) => ValueInput::Number(n + by),
        ValueInput::Name(text) if by.abs() < 1e-9 => ValueInput::Name(text.clone()),
        ValueInput::Name(text) if by > 0.0 => ValueInput::Name(format!(
            "({text}) + {}",
            mitcad_model::internal::build::round(by)
        )),
        ValueInput::Name(text) => ValueInput::Name(format!(
            "({text}) - {}",
            mitcad_model::internal::build::round(-by)
        )),
    }
}

/// ` + k` or ` - k` after an expression; nothing for 0.
fn plus(k: f64) -> String {
    if k.abs() < 1e-9 {
        String::new()
    } else if k > 0.0 {
        format!(" + {}", mitcad_model::internal::build::round(k))
    } else {
        format!(" - {}", mitcad_model::internal::build::round(-k))
    }
}

/// Where a part's frame slides to along a profile (from end A) for an
/// offset from a reference (mitcad#417): its near face `distance` from end
/// A, its far face from end B, its middle from the profile's. `near` and
/// `far` are its reach from its frame, `length` the profile's.
pub(crate) fn slide_of(
    from: SideFrom,
    distance: f64,
    near: f64,
    far: f64,
    length: Option<f64>,
) -> Result<f64, ModelError> {
    let middle = (near + far) / 2.0;
    match (from, length) {
        (SideFrom::A, _) => Ok(distance - near),
        (SideFrom::B, Some(l)) => Ok(l - distance - far),
        (SideFrom::Middle, Some(l)) => Ok(l / 2.0 + distance - middle),
        (SideFrom::B, None) => Err(refuse("the part has no end B to measure from")),
        (SideFrom::Middle, None) => Err(refuse("the part has no length to find its middle")),
    }
}

/// The offset from a reference of a part whose frame is at `tx` along the
/// profile ([`slide_of`] the other way); 0 for B and the middle without a
/// length.
pub(crate) fn offset_of(from: SideFrom, tx: f64, near: f64, far: f64, length: Option<f64>) -> f64 {
    let middle = (near + far) / 2.0;
    let offset = match (from, length) {
        (SideFrom::A, _) => tx + near,
        (SideFrom::B, Some(l)) => l - (tx + far),
        (SideFrom::Middle, Some(l)) => tx + middle - l / 2.0,
        (_, None) => 0.0,
    };
    if offset.abs() < 1e-9 { 0.0 } else { offset }
}

/// The slide's value as the joint keeps it, bound to its reference: from
/// end A the offset less the part's reach behind its frame, from end B
/// and the middle an expression of the profile's length parameter `name`
/// (`d12 - 15`, `d12 / 2 + 35`), so that the part keeps its place from
/// that end or the middle when the profile's length changes.
pub(crate) fn slide_value_of(
    from: SideFrom,
    offset: &ValueInput,
    near: f64,
    far: f64,
    name: Option<&str>,
) -> Result<ValueInput, ModelError> {
    let middle = (near + far) / 2.0;
    let length = |what: &str| {
        name.ok_or_else(|| refuse(format!("the part has no length to measure {what}")))
    };
    Ok(match (from, offset) {
        (SideFrom::A, value) => shifted(value, -near),
        (SideFrom::B, ValueInput::Number(n)) => {
            ValueInput::Name(format!("{}{}", length("from end B")?, plus(-(n + far))))
        }
        (SideFrom::B, ValueInput::Name(text)) => ValueInput::Name(format!(
            "{} - ({text}){}",
            length("from end B")?,
            plus(-far)
        )),
        (SideFrom::Middle, ValueInput::Number(n)) => ValueInput::Name(format!(
            "{} / 2{}",
            length("from the middle")?,
            plus(n - middle)
        )),
        (SideFrom::Middle, ValueInput::Name(text)) => ValueInput::Name(format!(
            "{} / 2 + ({text}){}",
            length("from the middle")?,
            plus(-middle)
        )),
    })
}

/// A part on a side, along the slot: where it reaches from its joint's
/// slide, the length it stays within, and the snap a click took.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SidePlan {
    /// The part's reach along the slot's x from where its frame meets the
    /// slot: its near face is at `tx + near`, its far one at `tx + far`.
    pub near: f64,
    pub far: f64,
    /// The target's length along the slot (from end A), when it has one.
    pub length: Option<f64>,
    /// How near a snap point a click snaps, mm: one section width at the
    /// ends, a quarter of it at the middle and the grid points.
    pub snap: f64,
    /// The slot's grid (the system's), 0 when it has none.
    pub grid: f64,
    /// The snap a click took (`end_a`, `end_b`, `middle`, `grid`), or the
    /// clamp at an end (`clamped`); None for a click between them or a
    /// typed offset that fits.
    pub snapped: Option<&'static str>,
    /// The near face's distance from end A.
    pub offset: f64,
    /// The reference the placement is bound to and its offset from it
    /// (mitcad#417): the near face from end A, the far face from end B,
    /// the part's middle from the target's.
    pub from: SideFrom,
    pub from_offset: f64,
    /// Offsets are to the foot's long point (a tilted part, mitcad#444),
    /// not to its near or far face or its middle; `near` and `far` are
    /// still its foot's reach.
    pub long_point: bool,
}

/// A part on an end (mitcad#417): what the click chose, the new part's
/// axis, the four turns square to the target, a corner's offset and the
/// mitre the placement uses.
#[derive(Debug, Clone)]
pub struct EndPlan {
    /// `straight` or `edge` (the click), `given` (`at.direction`).
    pub zone: &'static str,
    /// In the BUILD host's coordinates.
    pub direction: V3,
    /// +x, +y, -x, -y of the target's end frame, in the host's coordinates.
    pub turns: [V3; 4],
    /// A corner's offset (None when straight).
    pub offset: Option<f64>,
    pub mitre: Option<MitreInput>,
}

impl SidePlan {
    /// The snap points along the slot: (kind, the near face's distance
    /// from end A).
    pub fn snaps(&self) -> Vec<(&'static str, f64)> {
        let Some(length) = self.length else {
            return Vec::new();
        };
        let width = self.far - self.near;
        let mut out = vec![("end_a", 0.0)];
        if length - width > 1e-9 {
            out.push(("end_b", length - width));
            out.push(("middle", (length - width) / 2.0));
        }
        if self.grid > 1e-9 {
            let mut d = self.grid;
            while d < length - width - 1e-9 && out.len() < 1000 {
                if out.iter().all(|(_, o)| (o - d).abs() > 1e-9) {
                    out.push(("grid", d));
                }
                d += self.grid;
            }
        }
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out
    }
}

#[derive(Debug, Clone)]
pub enum Spot {
    Grounded(Transform),
    Joint(Box<JointPlan>),
}

#[derive(Debug, Clone)]
pub enum LengthPlan {
    None,
    Value(ValueInput, f64),
    Bound {
        to: Box<BoundTo>,
        mode: BoundMode,
        add: Option<ValueInput>,
        /// The validated geometric length used for planning and commit.
        first: f64,
        /// Stock length in the selected source row, before measurement.
        source_default: f64,
    },
}

/// Everything a `build_place` makes, worked out first.
#[derive(Debug, Clone)]
pub struct Plan {
    pub source: SourceUse,
    pub values: BTreeMap<String, ValueInput>,
    pub length: LengthPlan,
    pub spot: Spot,
    pub own_ends: Ends<ValueInput>,
    /// The target's derived part with its end cut (`existing`, `shared`).
    pub target_ends: Option<(FeatureUid, bool, EndCut<ValueInput>)>,
    pub rotate: f64,
    /// The tilt on a side (mitcad#444), radians; 0 square.
    pub tilt: f64,
    pub name: Option<String>,
    pub designation: String,
    pub place: &'static str,
    pub warnings: Vec<String>,
    /// The new part's frames and box in its coordinates, at its length.
    pub mounts: BTreeMap<String, Transform>,
    pub bounds: Option<BoundingBox>,
    /// Where the new part goes in the BUILD host's coordinates.
    pub placement: Transform,
    pub target: Option<Target>,
    /// The part's own grade (mitcad#478).
    pub grade: Option<String>,
    /// A member by two points: its placement, to place it anew.
    pub points: Option<serde_json::Value>,
    /// The corner its bound length closes (mitcad#485).
    pub closing: Option<Closing>,
    /// A member by two points: what its points are on, its ends cut back
    /// to the members they are in (mitcad#509).
    pub between: Option<crate::timber::Between>,
}

/// The exact length the committed bound feature will measure. The
/// planner uses the same target geometry calculation, before any edit.
fn measured_bound_length<K: Kernel>(
    doc: &Document<K>,
    plan: &Plan,
) -> Result<Option<f64>, ModelError> {
    let LengthPlan::Bound { to, mode, add, .. } = &plan.length else {
        return Ok(None);
    };
    let (start, along) = match &plan.spot {
        Spot::Grounded(at) => (at.translation, z_axis(at)),
        Spot::Joint(joint) => joint
            .ray
            .as_ref()
            .map(|ray| (ray.start, ray.along))
            .ok_or_else(|| refuse("the length cannot be bound here"))?,
    };
    let along = unit(along).ok_or_else(|| refuse("the direction is zero"))?;
    let component = mitcad_model::internal::joints::path_component(
        doc.assembly(),
        doc.build_host(),
        &to.occurrence.0,
    )
    .map_err(refuse)?;
    let target_owner = match &to.geometry {
        GeomRef::Face { body, .. } => Some(doc.body_component(*body).unwrap_or(ComponentUid::ROOT)),
        GeomRef::Mount { feature, .. } => doc.feature(*feature).map(|entry| entry.component),
        _ => None,
    };
    if target_owner.is_some_and(|owner| owner != component) {
        return Err(refuse("the bound target is not in the selected occurrence"));
    }
    let bodies = if matches!(to.geometry, GeomRef::Mount { .. }) {
        doc.bodies_of_components(&[component])
            .into_iter()
            .map(|b| b.uid)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let placement = doc.path_transform(&to.occurrence.0);
    let add = add
        .as_ref()
        .map_or(Ok(0.0), |v| value_in(doc, v, Unit::MM))?;
    mitcad_model::internal::features::measure_bound_length(
        &mut doc.resolver(),
        start,
        along,
        &to.geometry,
        placement,
        &bodies,
        *mode,
        add,
    )
    .map(Some)
    .map_err(refuse)
}

/// A corner a bound length closes (mitcad#485): the far end of a part
/// placed on an end meets the member its length is bound to end to end.
/// With the start mitred both ends get a shared mitre (`cut`, the other
/// member's end cut; the length runs through to its outer side, the long
/// point); with the start square the far end is a square butt on its face.
#[derive(Debug, Clone)]
pub struct Closing {
    pub component: ComponentUid,
    pub part: FeatureUid,
    /// The other member's end the corner is at: end B, else end A.
    pub end_b: bool,
    pub cut: Option<EndCut<ValueInput>>,
}

/// The bounds of shapes.
pub fn bounds_of<K: Kernel>(kernel: &K, shapes: &[K::Shape]) -> Option<BoundingBox> {
    let mut all: Option<BoundingBox> = None;
    for shape in shapes {
        let b = kernel.bounding_box(shape).ok()??;
        all = Some(match all {
            Some(a) => a.union(&b),
            None => b,
        });
    }
    all
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait PlaceBuild<K: Kernel> {
    /// The source a part input names: the design's, else read through the
    /// library resolver (a source the design has at that version is used
    /// again). Refused for a part without `build`.
    fn source_use(&self, part: &PartInput) -> Result<SourceUse, ModelError>;

    /// Places a part (`build_place`).
    fn build_place(&mut self, input: &PlaceInput) -> Result<Placed, ModelError>;

    /// The `build_preview` of a placing: what `build_place` would do, the
    /// document left as it is.
    fn preview_place(&self, input: &PlaceInput) -> Result<Plan, ModelError>;
}

impl<K: Kernel> PlaceBuild<K> for Document<K> {
    fn source_use(&self, part: &PartInput) -> Result<SourceUse, ModelError> {
        let state = self.state();
        let (def, new) =
            match part.source {
                Some(id) => (
                    state.library_sources.get(id).cloned().ok_or_else(|| {
                        refuse(format!("library source {id} is not in the design"))
                    })?,
                    false,
                ),
                None => {
                    let (Some(library), Some(component)) = (&part.library, &part.component) else {
                        return Err(refuse(
                            "the part is a library's component (library, url, rev, component) or a \
                         source of the design (source)",
                        ));
                    };
                    let rev = part.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
                    let resolver = self.link_resolver().ok_or_else(|| {
                        refuse(
                            "no component libraries are set up here (libraries are read by the \
                            application and mitcad-cli)",
                        )
                    })?;
                    let read = resolver
                        .library_source(&LibraryRequest {
                            library: library.clone(),
                            url: part.url.clone(),
                            rev,
                            component: component.clone(),
                        })
                        .map_err(refuse)?;
                    match state
                        .library_sources
                        .find(&read.library, &read.component, &read.rev)
                    {
                        Some(id) => (
                            state.library_sources.get(id).cloned().expect("found"),
                            false,
                        ),
                        None => (
                            LibrarySourceDef::from_library(state.library_sources.next_id(), &read)
                                .map_err(refuse)?,
                            true,
                        ),
                    }
                }
            };
        let Some(build) = &def.build else {
            return Err(refuse(format!(
                "{} has no build: it is no part of the BUILD tab",
                def.label()
            )));
        };
        let meta = BuildMeta::read_row(build, part.config.as_deref())
            .map_err(|e| refuse(format!("{}: {e}", def.label())))?;
        let design = def
            .design
            .state()
            .map_err(|e| refuse(format!("{}: {e}", def.label())))?;
        if let Some(config) = &part.config
            && design.configurations.row(config).is_none()
        {
            return Err(refuse(format!(
                "{} has no configuration {config}",
                def.label()
            )));
        }
        Ok(SourceUse {
            def,
            new,
            meta,
            config: part.config.clone(),
        })
    }

    fn build_place(&mut self, input: &PlaceInput) -> Result<Placed, ModelError> {
        if input.spacing.is_some() {
            return self.build_place_set(input);
        }
        let plan = self.plan_place_between(input, None)?;
        // In the BUILD host: the active component, or the nearest one up
        // that takes components (mitcad#394).
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
        self.apply(|state| {
            let placed = make_part(state, &plan, active)?;
            Ok((label, placed))
        })
    }

    fn preview_place(&self, input: &PlaceInput) -> Result<Plan, ModelError> {
        self.plan_place_between(input, None)
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait PlaceCrateOps<K: Kernel> {
    /// An occurrence path from the BUILD host ([`Document::build_host`]).
    fn path_from_host(&self, text: &str) -> Result<Vec<OccurrenceUid>, ModelError>;

    /// The BUILD part a pick names, with its placement.
    fn target(&self, path: &[OccurrenceUid]) -> Option<Target>;

    /// Plans a `build_place`.
    fn plan_place(&self, input: &PlaceInput) -> Result<Plan, ModelError>;

    /// Rebuilds a bound plan once using its measured target length.
    fn plan_place_with_bound_length(
        &self,
        input: &PlaceInput,
        measured: Option<f64>,
    ) -> Result<Plan, ModelError>;

    /// Whether a part's far end meets `other` end to end, as a corner
    /// (mitcad#485): the ray from `start` along `along` (the part's axis,
    /// in the host's coordinates) runs through `other` (not along one of its
    /// faces), not along its axis, within
    /// its section's size of one of its ends (as `build_junctions` finds an
    /// `L`), and `other` is not the part the start is on (`from`). Whether
    /// the corner is at its end B, its axis away from the corner, and how
    /// far along the ray its box begins and ends (its face, its outer
    /// side).
    fn closing_corner(
        &self,
        other: &Target,
        start: V3,
        along: V3,
        from: Option<&Target>,
    ) -> Option<(bool, V3, (f64, f64))>;

    /// The member a typed length's far end meets as a corner (mitcad#492):
    /// of the members cut to length its axis meets ([`Self::closing_corner`]),
    /// the nearest one whose box the far end, `length` from `start` along
    /// `along`, is in (within [`MEETS`]).
    fn typed_corner(
        &self,
        length: f64,
        start: V3,
        along: V3,
        from: Option<&Target>,
    ) -> Option<(Target, bool, V3, (f64, f64))>;

    /// Plans a part on a BUILD part: on a side's slot, on an end, or on a
    /// frame it mates with.
    #[allow(clippy::too_many_arguments)]
    fn plan_on_part(
        &self,
        source: &SourceUse,
        target: &Target,
        geometry: &GeomRef,
        at: &PlaceAt,
        touch: &str,
        new: &NewPart<'_>,
        length: &LengthPlan,
        mitre: Option<&MitreInput>,
    ) -> Result<(Spot, &'static str), ModelError>;

    /// Refuses a part whose systems do not fit the target, or a frame that
    /// does not accept it.
    fn check_fits(
        &self,
        source: &SourceUse,
        target: &Target,
        frame: &str,
    ) -> Result<(), ModelError>;

    /// The end frame of a target nearest to a point (its own coordinates).
    fn nearest_end(&self, target: &Target, point: Option<V3>) -> Result<String, ModelError>;

    /// The slot of a picked side face nearest to the point: of the slots
    /// on the face's plane when the face is planar (none: a closed side).
    fn pick_slot(
        &self,
        target: &Target,
        face: Option<&(mitcad_model::ids::BodyUid, mitcad_model::topo::FaceName)>,
        point: Option<V3>,
    ) -> Result<String, ModelError>;

    /// Plans a part on a slot of a profile's side (mitcad#394): its near
    /// face at the offset from end A (its far face from end B), else where
    /// the click was, snapped flush with an end within one section width
    /// of it, or to the middle or a grid point, and kept within the
    /// target's length.
    #[allow(clippy::too_many_arguments)]
    fn plan_side(
        &self,
        source: &SourceUse,
        target: &Target,
        slot: &str,
        at: &PlaceAt,
        touch: &str,
        new: &NewPart<'_>,
        length: &LengthPlan,
    ) -> Result<(Spot, &'static str), ModelError>;

    /// The slide along a slot of a part on a side (`position.tx`, the
    /// distance of the part's frame from end A), bound to its reference
    /// (mitcad#417): `at.offset` from `at.from`, else the click, snapped,
    /// from the reference it snapped to (the nearer end between snaps, or
    /// `at.from` when given); kept within the target's length (a typed
    /// offset that would overhang goes flush with that end, with a warning
    /// that `what` names).
    #[allow(clippy::too_many_arguments)]
    fn side_value(
        &self,
        at: &PlaceAt,
        slot_local: &Transform,
        point_local: Option<V3>,
        target_length: Option<(ParamId, f64)>,
        side: &mut SidePlan,
        warnings: &mut Vec<String>,
        what: &str,
    ) -> Result<(ValueInput, f64), ModelError>;

    /// The offset along a rail a part that moves on it slides by
    /// (`at.offset` from end A, end B or the middle, else the pick's
    /// distance along it), between the ends.
    fn slide_value(
        &self,
        at: &PlaceAt,
        frame_local: &Transform,
        point_local: Option<V3>,
        target_length: Option<(ParamId, f64)>,
    ) -> Result<(ValueInput, f64), ModelError>;

    /// Plans a part whose frame mates with a frame of the target (a
    /// carriage on a rail, a cap on an end, a plate on a frame): the joint
    /// the part's `mate` says, else rigid.
    #[allow(clippy::too_many_arguments)]
    fn plan_mate(
        &self,
        source: &SourceUse,
        target: &Target,
        on: &str,
        own: &str,
        mate: Option<&crate::LinearMate>,
        at: &PlaceAt,
        mounts: &BTreeMap<String, Transform>,
    ) -> Result<Spot, ModelError>;

    /// Plans the first part: on a plane, a point or a face (of the BUILD host,
    /// or of an occurrence that is no BUILD part), at the picked point, along
    /// `at.direction` when given; `lie` (and the part's box) turns a member
    /// about it, flat or on edge on the plane (mitcad#485). A member lying
    /// on it (`at.direction` along the plane) rests on it, on its normal's
    /// side, as turned by `rotate` (mitcad#492).
    #[allow(clippy::too_many_arguments)]
    fn plan_on_geometry(
        &self,
        source: &SourceUse,
        path: &[OccurrenceUid],
        geometry: &GeomRef,
        at: &PlaceAt,
        touch: &str,
        mounts: &BTreeMap<String, Transform>,
        lie: Option<crate::Lie>,
        bounds: Option<&BoundingBox>,
        rotate: f64,
    ) -> Result<Spot, ModelError>;
}

impl<K: Kernel> PlaceCrateOps<K> for Document<K> {
    fn path_from_host(&self, text: &str) -> Result<Vec<OccurrenceUid>, ModelError> {
        let a = self.assembly();
        let host = self.build_host();
        mitcad_model::internal::joints::find_path_from(a, host, text)
            .ok_or_else(|| refuse(format!("no occurrence {text} in {}", a.name(host))))
    }

    fn target(&self, path: &[OccurrenceUid]) -> Option<Target> {
        let component = mitcad_model::internal::joints::path_component(
            self.assembly(),
            self.build_host(),
            path,
        )
        .ok()?;
        if path.is_empty() {
            return None;
        }
        Some(Target {
            path: path.to_vec(),
            info: self.part_info(component)?,
            placement: self.path_transform(path),
        })
    }

    fn plan_place(&self, input: &PlaceInput) -> Result<Plan, ModelError> {
        self.plan_place_with_bound_length(input, None)
    }

    fn plan_place_with_bound_length(
        &self,
        input: &PlaceInput,
        measured: Option<f64>,
    ) -> Result<Plan, ModelError> {
        let source = self.source_use(&input.part)?;
        let label = source.def.label();
        match source.meta.role {
            BuildRole::Panel => {
                return Err(refuse(format!(
                    "{label} is a panel: it is placed in a frame with build_panel"
                )));
            }
            BuildRole::Connector => {
                return Err(refuse(format!(
                    "{label} is a connector: it is placed at a junction with build_connect"
                )));
            }
            BuildRole::Fastener => {
                return Err(refuse(format!(
                    "{label} is a fastener: it comes with its connector"
                )));
            }
            _ => {}
        }
        let mut warnings = Vec::new();
        // Values of the source's parameters, the length first; the part's
        // options (mitcad#801) under the command's own.
        let mut values = input.part.parameters.clone();
        values.extend(input.parameters.clone());
        let config = source.config.as_deref();
        // A member by two points: their distance unless a length is given
        // (mitcad#478).
        let points_length = match (&input.to, &input.length) {
            (Some(to), None) => {
                let start =
                    input.at.as_ref().and_then(|a| a.point).ok_or_else(|| {
                        refuse("a member by two points needs its start (at.point)")
                    })?;
                let distance = crate::geometry::norm(sub(to.point, start));
                Some(LengthInput::Value(ValueInput::Number(
                    (distance * 1000.0).round() / 1000.0,
                )))
            }
            _ => None,
        };
        let input_length = points_length.as_ref().or(input.length.as_ref());
        let mut length = match (&source.meta.length, input_length) {
            (None, None) => LengthPlan::None,
            (None, Some(_)) => {
                return Err(refuse(format!("{label} is not cut to length")));
            }
            (Some(meta), input_length) => {
                let default =
                    default_value(&source.def, config, &meta.parameter).ok_or_else(|| {
                        refuse(format!(
                            "{label}: its length parameter {} does not exist",
                            meta.parameter
                        ))
                    })?;
                match input_length {
                    None => LengthPlan::Value(ValueInput::Number(default), default),
                    Some(LengthInput::Value(value)) => {
                        let length = value_in(self, value, Unit::MM)?;
                        if let Some(problem) = meta.outside(length) {
                            return Err(refuse(problem));
                        }
                        LengthPlan::Value(value.clone(), length)
                    }
                    Some(LengthInput::Bound { bind, mode, add }) => LengthPlan::Bound {
                        to: Box::new(BoundTo {
                            occurrence: OccurrencePath(self.path_from_host(&bind.occurrence)?),
                            geometry: bind.geometry.clone(),
                        }),
                        mode: *mode,
                        add: add.clone(),
                        first: measured.unwrap_or(default),
                        source_default: default,
                    },
                }
            }
        };
        let length_value = match &length {
            LengthPlan::Value(_, v) => Some(*v),
            LengthPlan::Bound { first, .. } => Some(*first),
            LengthPlan::None => None,
        };
        if let (Some(meta), LengthPlan::Value(value, _)) = (&source.meta.length, &length) {
            values.insert(meta.parameter.clone(), value.clone());
        }
        let mut planning_values = values.clone();
        if let (Some(meta), LengthPlan::Bound { .. }, Some(v)) =
            (&source.meta.length, &length, length_value)
        {
            planning_values.insert(meta.parameter.clone(), ValueInput::Number(v));
        }
        // The new part's frames and box.
        let mut chosen = source_option_values(self, &source.def, &planning_values)?;
        if let (Some(meta), LengthPlan::Bound { .. }, Some(_)) =
            (&source.meta.length, &length, measured)
        {
            // The planned number stands for a geometry-bound length; the
            // committed derived part cannot move that binding either.
            chosen.pinned.insert(meta.parameter.clone());
        }
        let configured = source_state_with_options(&source.def, config, &chosen)
            .map_err(|e| refuse(format!("{label}: {e}")))?;
        // Keep repaired constants as component-option edits do. Bound
        // expressions stay untouched because the cascade cannot move them.
        for moved in &configured.moves {
            values.insert(
                moved.option.clone(),
                ValueInput::Name(moved.to_expression.clone()),
            );
        }
        warnings.extend(
            configured
                .moves
                .iter()
                .map(|m| format!("{label}: {}", m.text)),
        );
        let normalized = chosen.normalized(&configured).map_err(refuse)?;
        let state = &configured.state;
        if let (Some(meta), LengthPlan::Value(expression, current)) =
            (&source.meta.length, &mut length)
        {
            let id = state
                .parameters
                .find(&meta.parameter)
                .ok_or_else(|| refuse("the length parameter is missing"))?;
            *current = state
                .parameters
                .value(id)
                .filter(|v| v.is_finite())
                .ok_or_else(|| refuse("the configured length is not finite"))?;
            if let Some(moved) = configured.moves.iter().find(|m| m.option == meta.parameter) {
                *expression = ValueInput::Name(moved.to_expression.clone());
            }
        }
        let evaluated =
            evaluate_source(self.kernel(), state).map_err(|e| refuse(format!("{label}: {e}")))?;
        let bounds = bounds_of(self.kernel(), &evaluated.bodies);
        let mounts = evaluated.mounts;
        let rotate = match &input.rotate {
            Some(value) => value_in(self, value, Unit::DEG)?,
            None => 0.0,
        };
        let quarter = rotate / FRAC_PI_2;
        if (quarter - quarter.round()).abs() > 1e-6 {
            return Err(refuse("the rotation is 0, 90, 180 or 270 degrees"));
        }
        // A tilt along the host (mitcad#444), only on a side.
        let tilt = match &input.tilt {
            Some(value) => value_in(self, value, Unit::DEG)?,
            None => 0.0,
        };
        crate::tilt::check_tilt(tilt).map_err(refuse)?;
        let touch = if input.flip { "end_b" } else { "end_a" };
        let at = input.at.clone().unwrap_or_default();
        let mut target = None;
        let new = NewPart {
            mounts: &mounts,
            bounds: bounds.as_ref(),
            rotate,
            tilt,
        };
        let points = match &input.to {
            Some(to) => Some(self.plan_points(
                &source,
                &at,
                to,
                input.lie.unwrap_or_default(),
                input.flip,
                rotate,
                &mounts,
                bounds.as_ref(),
            )?),
            None => None,
        };
        // A member by two points is turned by its axes, not the joint.
        let rotate = if points.is_some() { 0.0 } else { rotate };
        let (spot, place) = match (points, &at.geometry) {
            (Some(points), _) => (points.spot, "points"),
            (None, None) => {
                // At the point (the origin), along `at.direction` (+Z).
                let point = at.point.unwrap_or([0.0; 3]);
                let grounded = match at.direction.and_then(unit) {
                    Some(d) => {
                        let hint = match input.lie {
                            // Flat or on edge on the host's XY (mitcad#485).
                            Some(lie) => {
                                crate::timber::lie_axes(
                                    d,
                                    bounds.as_ref(),
                                    crate::geometry::Z,
                                    lie,
                                    0.0,
                                )
                                .0
                            }
                            None => [crate::geometry::X, crate::geometry::Y, crate::geometry::Z]
                                .into_iter()
                                .find(|a| dot(*a, d).abs() < 1.0 - 1e-6)
                                .unwrap_or(crate::geometry::X),
                        };
                        let frame = crate::geometry::frame(point, hint, d);
                        // Lying on the XY plane it rests on it, not centred
                        // on it (mitcad#492).
                        let up = crate::geometry::Z;
                        match bounds.as_ref() {
                            Some(b) if dot(d, up).abs() < 1e-6 => {
                                Transform::translation(scale(up, rest_lift(b, &frame, point, up)))
                                    .after(&frame)
                            }
                            _ => frame,
                        }
                    }
                    None => Transform::translation(point),
                };
                (Spot::Grounded(grounded), "grounded")
            }
            (None, Some(geometry)) => {
                let path = self.path_from_host(&at.occurrence)?;
                match self.target(&path) {
                    Some(t) => {
                        let (spot, place) = self.plan_on_part(
                            &source,
                            &t,
                            geometry,
                            &at,
                            touch,
                            &new,
                            &length,
                            input.mitre.as_ref(),
                        )?;
                        target = Some(t);
                        (spot, place)
                    }
                    None => (
                        self.plan_on_geometry(
                            &source,
                            &path,
                            geometry,
                            &at,
                            touch,
                            &mounts,
                            input.lie,
                            bounds.as_ref(),
                            rotate,
                        )?,
                        "geometry",
                    ),
                }
            }
        };
        if let Spot::Joint(j) = &spot {
            warnings.extend(j.warnings.iter().cloned());
        }
        if tilt.abs() > 1e-12 && place != "side" {
            return Err(refuse(
                "only a part on a side tilts (on an end its direction turns it)",
            ));
        }
        // Where the new part goes.
        let placement = match &spot {
            Spot::Grounded(t) => *t,
            Spot::Joint(j) => {
                let mut own = mounts.get(&j.own).copied().unwrap_or(Transform::IDENTITY);
                if let Some(o) = &j.own_override {
                    own =
                        mitcad_model::internal::joints::apply_override(&own, o).map_err(refuse)?;
                }
                j.frame_b
                    .after(&mitcad_model::internal::joints::alignment(
                        rotate, 0.0, j.flip,
                    ))
                    .after(&inverse(&own))
            }
        };
        // Mitres: on an end the one its plan resolved (a corner's default,
        // mitcad#417), elsewhere the command's.
        let mut own_ends = Ends { a: None, b: None };
        let mut target_ends = None;
        // A tilted part's cuts (mitcad#444): its foot, and its far end
        // with a bound length.
        if let Spot::Joint(j) = &spot {
            for (b, angle, cut_tilt) in &j.cuts {
                let cut = Some(EndCut {
                    angle: ValueInput::Number(*angle),
                    tilt: Some(*cut_tilt),
                });
                if *b {
                    own_ends.b = cut;
                } else {
                    own_ends.a = cut;
                }
            }
        }
        let mitre = match &spot {
            Spot::Joint(j) if j.end.is_some() => j.end.as_ref().and_then(|e| e.mitre.clone()),
            _ => input.mitre.clone().filter(|m| m.cut != MitreCut::None),
        };
        if let Some(mitre) = &mitre {
            let Some(target) = &target else {
                return Err(refuse("a mitre needs a profile's side or end to meet"));
            };
            if tilt.abs() > 1e-12 {
                return Err(refuse(
                    "a tilted part's foot is cut by its tilt: it takes no mitre",
                ));
            }
            if source.meta.length.is_none() {
                return Err(refuse(format!(
                    "{label} is not cut to length: it has no mitres"
                )));
            }
            let Spot::Joint(joint) = &spot else {
                return Err(refuse("a mitre needs a profile's side or end to meet"));
            };
            let own_end_b = joint.own == "end_b";
            if joint.own != "end_a" && joint.own != "end_b" {
                return Err(refuse("a mitre is cut at the end that meets the profile"));
            }
            // The new part's lean and the target's, in their coordinates.
            let to_new = |v: V3| inverse(&placement).apply_vector(v);
            let angle = |what: &str| -> Result<(ValueInput, f64), ModelError> {
                let value = mitre
                    .angle
                    .clone()
                    .ok_or_else(|| refuse(format!("the {what} mitre needs its angle")))?;
                let radians = value_in(self, &value, Unit::DEG)?;
                Ok((value, radians))
            };
            match place {
                "side" => {
                    if mitre.cut != MitreCut::New {
                        return Err(refuse(
                            "on a side only the new part's end is mitred (cut \"new\")",
                        ));
                    }
                    let (value, _) = angle("new")?;
                    let tilt = Tilt::towards(to_new(x_axis(&joint.frame_b)));
                    let cut = EndCut { angle: value, tilt };
                    if own_end_b {
                        own_ends.b = Some(cut);
                    } else {
                        own_ends.a = Some(cut);
                    }
                }
                "end" => {
                    let end_name = target_end_name(&joint.b);
                    let end_frame = target.frame(&end_name).ok_or_else(|| {
                        refuse(format!("{} has no frame {end_name}", target.info.name))
                    })?;
                    let u = z_axis(&end_frame);
                    let v = z_axis(&joint.frame_b);
                    let target_lean = inverse(&target.placement).apply_vector(v);
                    let own_lean = to_new(crate::geometry::scale(u, -1.0));
                    let target_part = target.part()?;
                    let target_b = end_name == "end_b";
                    let (own_angle, target_angle) = match mitre.cut {
                        MitreCut::New => (Some(angle("new")?.0), None),
                        MitreCut::Existing => (None, Some(angle("existing")?.0)),
                        MitreCut::None => (None, None),
                        MitreCut::Shared => {
                            let half = angle_between(u, v) / 2.0;
                            if half < 1e-9 {
                                warnings.push(
                                    "the parts are on one axis: a shared mitre cuts nothing"
                                        .to_owned(),
                                );
                                (None, None)
                            } else {
                                (
                                    Some(ValueInput::Number(half)),
                                    Some(ValueInput::Number(half)),
                                )
                            }
                        }
                    };
                    if let Some(value) = own_angle {
                        let cut = EndCut {
                            angle: value,
                            tilt: Tilt::towards(own_lean),
                        };
                        if own_end_b {
                            own_ends.b = Some(cut);
                        } else {
                            own_ends.a = Some(cut);
                        }
                    }
                    if let Some(value) = target_angle {
                        if !target.info.is_cut_to_length() {
                            return Err(refuse(format!(
                                "{} is not cut to length: it has no mitres",
                                target.info.name
                            )));
                        }
                        target_ends = Some((
                            target_part,
                            target_b,
                            EndCut {
                                angle: value,
                                tilt: Tilt::towards(target_lean),
                            },
                        ));
                    }
                }
                _ => return Err(refuse("a mitre needs a profile's side or end to meet")),
            }
        }
        // Closing a frame (mitcad#485): a part on an end whose far end meets
        // a member end to end, as a corner: its length bound to that
        // member, or typed to end on its face or its outer side (mitcad#492,
        // within `MEETS`). A mitred start gets a shared mitre there too,
        // cutting both (the length through to the member's outer side, the
        // long point); a square start a square butt on its face, never into
        // it. A typed length that ends inside the member gets a warning.
        // A member by two points (mitcad#492) has a square start: its far end
        // a square butt on the face, else the warning.
        let mut closing = None;
        let start_ray = match (&spot, place) {
            (Spot::Joint(joint), "end") => Some((
                joint.own == "end_b",
                match &joint.ray {
                    Some(ray) => (ray.start, ray.along),
                    None => (joint.frame_b.translation, z_axis(&joint.frame_b)),
                },
            )),
            // From end A along the part's axis (the points as `flip` ordered
            // them).
            (_, "points") => {
                let end_a = mounts.get("end_a").map_or([0.0; 3], |t| t.translation);
                Some((
                    false,
                    (
                        placement.apply_point(end_a),
                        placement.apply_vector(crate::geometry::Z),
                    ),
                ))
            }
            _ => None,
        };
        if let Some((start_b, (start, along))) = start_ray
            && source.meta.length.is_some()
            && tilt.abs() <= 1e-12
        {
            let mitred = if start_b {
                own_ends.b.is_some()
            } else {
                own_ends.a.is_some()
            };
            // Whether a mitred start takes a shared mitre at the corner.
            let shares = |other: &Target, away: V3| {
                mitred && angle_between(along, away) / 2.0 > 1e-9 && other.info.is_cut_to_length()
            };
            let mut corner = None;
            match &mut length {
                // A member by two points keeps the bound mode it was given.
                LengthPlan::Bound { to, mode, .. } if place == "end" => {
                    if let Some(other) = self.target(&to.occurrence.0)
                        && let Some((end_b, away, _)) =
                            self.closing_corner(&other, start, along, target.as_ref())
                    {
                        let through = shares(&other, away);
                        *mode = if through {
                            BoundMode::Through
                        } else {
                            BoundMode::Face
                        };
                        corner = Some((other, end_b, away, through));
                    }
                }
                LengthPlan::Value(_, l) => {
                    if let Some((other, end_b, away, (near, far))) =
                        self.typed_corner(*l, start, along, target.as_ref())
                    {
                        let through = shares(&other, away) && (*l - far).abs() <= MEETS;
                        if through || (*l - near).abs() <= MEETS {
                            corner = Some((other, end_b, away, through));
                        } else {
                            warnings.push(format!(
                                "its far end ends inside {}: a length of {} mm meets its face, {} mm its outer side",
                                other.info.name,
                                mitcad_model::internal::build::round(near),
                                mitcad_model::internal::build::round(far)
                            ));
                        }
                    }
                }
                LengthPlan::Bound { .. } | LengthPlan::None => {}
            }
            if let Some((other, end_b, away, through)) = corner {
                let half = angle_between(along, away) / 2.0;
                let cut = if through {
                    // As at a corner's start: each end leans towards the
                    // other's axis away from the corner.
                    let far = Some(EndCut {
                        angle: ValueInput::Number(half),
                        tilt: Tilt::towards(inverse(&placement).apply_vector(away)),
                    });
                    if start_b {
                        own_ends.a = far;
                    } else {
                        own_ends.b = far;
                    }
                    Some(EndCut {
                        angle: ValueInput::Number(half),
                        tilt: Tilt::towards(
                            inverse(&other.placement).apply_vector(scale(along, -1.0)),
                        ),
                    })
                } else {
                    None
                };
                closing = Some(Closing {
                    component: other.info.component,
                    part: other.part()?,
                    end_b,
                    cut,
                });
            }
        }
        let mut shown = BTreeMap::new();
        for (name, value) in &normalized {
            if let PartValue::Number(number) = value {
                shown.insert(name.clone(), *number);
            }
        }
        // A member by two points keeps how it was placed (mitcad#478).
        let source_id = source.def.id;
        let points = input.to.as_ref().map(|_| {
            let mut base = input.clone();
            base.spacing = None;
            base.part = PartInput {
                source: Some(source_id),
                config: base.part.config.clone(),
                ..PartInput::default()
            };
            serde_json::to_value(&base).unwrap_or_default()
        });
        let designation = combination(
            state,
            config,
            &Naming {
                pattern: &source.def.library.designation,
                standard: &source.def.standard,
                name: &label,
                fallback: "{name} {config}",
                rows: None,
                chosen: "",
            },
            &shown,
        )
        .designation;
        let plan = Plan {
            source,
            values,
            length,
            spot,
            own_ends,
            target_ends,
            rotate,
            tilt,
            name: input.name.clone(),
            designation,
            place,
            warnings,
            mounts,
            bounds,
            placement,
            target,
            grade: input.grade.clone().filter(|g| !g.trim().is_empty()),
            points,
            closing,
            between: None,
        };
        if let Some(length) = measured_bound_length(self, &plan)? {
            if let Some(previous) = measured {
                if (length - previous).abs() > 1e-9 {
                    return Err(refuse("the bound length changes its own placement"));
                }
            } else {
                return self.plan_place_with_bound_length(input, Some(length));
            }
        }
        Ok(plan)
    }

    fn closing_corner(
        &self,
        other: &Target,
        start: V3,
        along: V3,
        from: Option<&Target>,
    ) -> Option<(bool, V3, (f64, f64))> {
        if from.is_some_and(|t| t.info.component == other.info.component) {
            return None;
        }
        let ea = other.frame("end_a")?.translation;
        let eb = other.frame("end_b")?.translation;
        let length = crate::geometry::norm(sub(eb, ea));
        let axis = unit(sub(eb, ea))?;
        if dot(axis, along).abs() > 1.0 - 1e-6 {
            return None;
        }
        let bounds = self.part_bounds(other.info.component)?;
        let (near, far) = crate::geometry::box_along(&bounds, &other.placement, start, along);
        if near <= 1e-6 {
            return None;
        }
        // Through it, not along one of its faces (a board on a stud's face).
        let middle =
            inverse(&other.placement).apply_point(add(start, scale(along, (near + far) / 2.0)));
        if !(0..3).all(|i| middle[i] > bounds.min[i] + 1e-3 && middle[i] < bounds.max[i] - 1e-3) {
            return None;
        }
        let at = dot(sub(add(start, scale(along, near)), ea), axis);
        let tolerance = 1e-3;
        if at < -tolerance || at > length + tolerance {
            return None;
        }
        let end_b = length - at < at;
        let from_end = if end_b { length - at } else { at };
        let section = (bounds.max[0] - bounds.min[0]).max(bounds.max[1] - bounds.min[1]);
        if from_end > section + tolerance {
            return None;
        }
        let away = if end_b { scale(axis, -1.0) } else { axis };
        Some((end_b, away, (near, far)))
    }

    fn typed_corner(
        &self,
        length: f64,
        start: V3,
        along: V3,
        from: Option<&Target>,
    ) -> Option<(Target, bool, V3, (f64, f64))> {
        self.build_parts_info()
            .into_iter()
            .filter(|info| info.is_cut_to_length())
            .filter_map(|info| {
                let path = self.placed_in_host(info.component)?;
                let other = Target {
                    placement: self.path_transform(&path),
                    path,
                    info,
                };
                let (end_b, away, (near, far)) = self.closing_corner(&other, start, along, from)?;
                (length >= near - MEETS && length <= far + MEETS).then_some((
                    other,
                    end_b,
                    away,
                    (near, far),
                ))
            })
            .min_by(|a, b| a.3.0.total_cmp(&b.3.0))
    }

    fn plan_on_part(
        &self,
        source: &SourceUse,
        target: &Target,
        geometry: &GeomRef,
        at: &PlaceAt,
        touch: &str,
        new: &NewPart<'_>,
        length: &LengthPlan,
        mitre: Option<&MitreInput>,
    ) -> Result<(Spot, &'static str), ModelError> {
        let label = source.def.label();
        let mounts = new.mounts;
        let target_part = target.part()?;
        // A click on an end face: its zone turns the new part (mitcad#417).
        let on_end_face = matches!(geometry, GeomRef::Face { face, .. }
            if face.feature == target_part && matches!(face.role.as_str(), "end_a" | "end_b"));
        let point_local = at.point.map(|p| inverse(&target.placement).apply_point(p));
        // What was picked: a frame or a face.
        enum Pick {
            End(String),
            Slot(String),
            Frame(String),
            Face(Option<(mitcad_model::ids::BodyUid, mitcad_model::topo::FaceName)>),
        }
        let pick = match geometry {
            GeomRef::Mount { feature, name } => {
                if *feature != target_part {
                    return Err(refuse(format!(
                        "{geometry} is not a frame of {}",
                        target.info.name
                    )));
                }
                match target.info.frame_kind(name).as_str() {
                    "end" => Pick::End(name.clone()),
                    "slot" => Pick::Slot(name.clone()),
                    "face" if target.info.is_side(name) => Pick::Slot(name.clone()),
                    _ => Pick::Frame(name.clone()),
                }
            }
            GeomRef::Face { body, face } => {
                if face.feature == target_part && matches!(face.role.as_str(), "end_a" | "end_b") {
                    Pick::End(face.role.clone())
                } else {
                    Pick::Face(Some((*body, face.clone())))
                }
            }
            _ => Pick::Face(None),
        };
        let pick = match (at.place, pick) {
            (PlaceKind::Side, Pick::End(_)) => Pick::Face(None),
            (PlaceKind::End, Pick::Slot(_) | Pick::Face(_)) => {
                Pick::End(self.nearest_end(target, point_local)?)
            }
            (_, pick) => pick,
        };
        // A part that moves on another's frame (a carriage on its rail). One
        // that slides in a slot goes on a profile's side as a profile does
        // (mitcad#448): on the slot of the side picked, not on the first
        // slot frame the profile lists.
        let mate = source.meta.linear.as_ref().and_then(|l| l.mate.clone());
        let in_slot = mate.as_ref().is_some_and(|m| m.on == "slot")
            && target.info.meta.role == BuildRole::Profile
            && matches!(pick, Pick::Slot(_) | Pick::Face(_));
        if let Some(mate) = mate.as_ref().filter(|_| !in_slot) {
            let on = match &pick {
                Pick::Frame(name) | Pick::Slot(name) | Pick::End(name)
                    if target.info.frame_kind(name) == mate.on =>
                {
                    Some(name.clone())
                }
                _ => target
                    .info
                    .mounts
                    .keys()
                    .find(|name| target.info.frame_kind(name) == mate.on)
                    .cloned(),
            };
            if let Some(on) = on {
                return self
                    .plan_mate(source, target, &on, &mate.frame, Some(mate), at, mounts)
                    .map(|spot| (spot, "frame"));
            }
        }
        match pick {
            Pick::End(end) => {
                if source.meta.role != BuildRole::Profile
                    && source.meta.role != BuildRole::Cap
                    && source.meta.length.is_none()
                {
                    return Err(refuse(format!(
                        "{label} cannot go on an end ({} is no profile)",
                        source.meta.role
                    )));
                }
                self.check_fits(source, target, &end)?;
                let own = if source.meta.length.is_some() {
                    touch.to_owned()
                } else {
                    own_frame(&source.meta, mounts, &["end", "face"]).ok_or_else(|| {
                        refuse(format!("{label} has no frame that goes on an end"))
                    })?
                };
                let end_frame_local = *target
                    .info
                    .mount(&end)
                    .ok_or_else(|| refuse(format!("{} has no frame {end}", target.info.name)))?;
                let [e, ex, ey, u] = parts(&end_frame_local);
                let to_host = |v: V3| target.placement.apply_vector(v);
                let cut_to_length = source.meta.length.is_some();
                let target_box = self.part_bounds(target.info.component);
                let least = at.snap.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(0.0);
                // The new part's axis (mitcad#417): `at.direction`, else the
                // zone of the end face the click was in.
                let (zone, bend) = match at.direction.and_then(unit) {
                    Some(d) => {
                        let d = unit(inverse(&target.placement).apply_vector(d)).unwrap_or(d);
                        ("given", (dot(d, u) < 1.0 - 1e-9).then_some(d))
                    }
                    None => match (point_local, &target_box) {
                        (Some(p), Some(b)) if on_end_face && cut_to_length => {
                            match end_zone(p, e, ex, ey, b, least) {
                                Some(d) => ("edge", Some(d)),
                                None => ("straight", None),
                            }
                        }
                        _ => ("straight", None),
                    },
                };
                if bend.is_some_and(|d| dot(d, u) < -1.0 + 1e-6) {
                    return Err(refuse(
                        "the direction runs back along the part it meets: turn it to a side",
                    ));
                }
                // A corner's offset and mitre: a square corner (90 degrees)
                // lies on the end face without a mitre; another angle is
                // shared at offset 0 unless the command says otherwise; none
                // at another offset (owner 18.10.2026).
                let corner = bend.is_some();
                let offset = match (corner, &at.offset) {
                    (true, Some(value)) => value_in(self, value, Unit::MM)?,
                    _ => 0.0,
                };
                let mitre = match mitre {
                    Some(m) if m.cut == MitreCut::None => None,
                    Some(m) => {
                        if corner && m.cut == MitreCut::Shared && offset.abs() > 1e-9 {
                            return Err(refuse(
                                "a shared mitre needs offset 0: the outer corners meet only there",
                            ));
                        }
                        Some(m.clone())
                    }
                    None if corner
                        && bend.is_some_and(|d| dot(d, u).abs() > 1e-6)
                        && offset.abs() <= 1e-9
                        && cut_to_length
                        && target.info.is_cut_to_length() =>
                    {
                        Some(MitreInput {
                            angle: None,
                            cut: MitreCut::Shared,
                        })
                    }
                    None => None,
                };
                let shared = mitre.as_ref().is_some_and(|m| m.cut == MitreCut::Shared);
                let mut b = target.origin(&end)?;
                let mut local = end_frame_local;
                // A member on a member's end (mitcad#485) lies as the target
                // does: its thickness axis parallel to the target's where the
                // direction allows (`rotate` turns it further).
                let own_local = mounts.get(&own).copied().unwrap_or(Transform::IDENTITY);
                let same_lie = |z: V3, hint: V3| match (new.bounds, &target_box) {
                    (Some(own_box), Some(target_box))
                        if source.meta.is_timber() && target.info.is_timber() =>
                    {
                        same_lie_x(&end_frame_local, z, hint, &own_local, own_box, target_box)
                    }
                    _ => None,
                };
                // The corner's shift from the end frame's origin, in the
                // target's coordinates.
                let mut shift = [0.0; 3];
                if bend.is_none()
                    && let Some(x) = same_lie(u, ex)
                {
                    let o = FrameOverride {
                        origin: None,
                        z_axis: None,
                        x_axis: Some(x),
                        shift: None,
                    };
                    local = mitcad_model::internal::joints::apply_override(&end_frame_local, &o)
                        .map_err(refuse)?;
                    b.frame_override = Some(o);
                }
                if let Some(d) = bend {
                    // The end frame's first axis that is not along the new
                    // direction keeps the turned frame's roll.
                    let hint = [ex, ey, u]
                        .into_iter()
                        .find(|a| dot(*a, d).abs() < 1.0 - 1e-6)
                        .unwrap_or(ex);
                    let hint = same_lie(d, hint).unwrap_or(hint);
                    let mut o = FrameOverride {
                        origin: None,
                        z_axis: Some(d),
                        x_axis: Some(hint),
                        shift: None,
                    };
                    let turned =
                        mitcad_model::internal::joints::apply_override(&end_frame_local, &o)
                            .map_err(refuse)?;
                    // Aligned (mitcad#417): in the target's plane, on its end
                    // face flush with its outer side, or mitred with the
                    // outer corners meeting.
                    if let (Some(own_box), Some(target_box)) = (new.bounds, &target_box) {
                        let to_target = turned
                            .after(&mitcad_model::internal::joints::alignment(
                                new.rotate, 0.0, true,
                            ))
                            .after(&inverse(&own_local));
                        shift =
                            corner_shift(e, u, d, target_box, own_box, &to_target, offset, shared);
                        o.shift = Some(inverse(&turned).apply_vector(shift));
                    }
                    local = mitcad_model::internal::joints::apply_override(&end_frame_local, &o)
                        .map_err(refuse)?;
                    b.frame_override = Some(o);
                }
                let frame_b = target.placement.after(&local);
                // Mating frames face each other: z against z.
                let flip = true;
                let ray = match length {
                    LengthPlan::Bound { .. } => {
                        let direction = match bend {
                            Some(d) => {
                                RayDirection::Vector(inverse(&end_frame_local).apply_vector(d))
                            }
                            None => RayDirection::default(),
                        };
                        Some(RayPlan {
                            from: target.origin(&end)?,
                            shift: inverse(&end_frame_local).apply_vector(shift),
                            shift_by_joint: false,
                            slide_extra: 0.0,
                            direction,
                            start: frame_b.translation,
                            along: z_axis(&frame_b),
                        })
                    }
                    _ => None,
                };
                let plan = EndPlan {
                    zone,
                    direction: to_host(bend.unwrap_or(u)),
                    turns: [ex, ey, scale(ex, -1.0), scale(ey, -1.0)].map(to_host),
                    offset: corner.then_some(offset),
                    mitre,
                };
                Ok((
                    Spot::Joint(Box::new(JointPlan {
                        kind: JointKind::Rigid,
                        slide: SlideAxis::Z,
                        own,
                        b,
                        frame_b,
                        flip,
                        slide_value: None,
                        ray,
                        side: None,
                        end: Some(plan),
                        own_override: None,
                        cuts: Vec::new(),
                        from: None,
                        warnings: Vec::new(),
                    })),
                    "end",
                ))
            }
            Pick::Frame(name) => self
                .plan_mate(source, target, &name, "", None, at, mounts)
                .map(|spot| (spot, "frame")),
            Pick::Slot(slot) => self.plan_side(source, target, &slot, at, touch, new, length),
            Pick::Face(face) => {
                let slot = self.pick_slot(target, face.as_ref(), point_local)?;
                self.plan_side(source, target, &slot, at, touch, new, length)
            }
        }
    }

    fn check_fits(
        &self,
        source: &SourceUse,
        target: &Target,
        frame: &str,
    ) -> Result<(), ModelError> {
        let label = source.def.label();
        if target.info.is_timber() && !source.meta.systems.is_empty() {
            return Err(refuse(format!(
                "a member has no slots: {label} goes in a slot"
            )));
        }
        if !source.meta.fits(target.info.meta.system.as_ref()) {
            let fits: Vec<String> = source.meta.systems.iter().map(|s| s.label()).collect();
            return Err(refuse(format!(
                "the systems do not fit: {label} fits {}, {} is {}",
                fits.join(", "),
                target.info.name,
                target
                    .info
                    .meta
                    .system
                    .as_ref()
                    .map_or_else(|| "of no system".to_owned(), |s| s.label())
            )));
        }
        if let Some(meta) = target.info.meta.frame(frame)
            && !meta.accepts(source.meta.role)
        {
            return Err(refuse(format!(
                "frame {frame} of {} does not take a {}",
                target.info.name, source.meta.role
            )));
        }
        Ok(())
    }

    fn nearest_end(&self, target: &Target, point: Option<V3>) -> Result<String, ModelError> {
        let mut ends: Vec<(&String, &Transform)> = target
            .info
            .mounts
            .iter()
            .filter(|(n, _)| target.info.frame_kind(n) == "end")
            .collect();
        if ends.is_empty() {
            return Err(refuse(format!("{} has no end frames", target.info.name)));
        }
        if let Some(p) = point {
            ends.sort_by(|a, b| {
                let d = |t: &Transform| crate::geometry::norm(sub(t.translation, p));
                d(a.1).total_cmp(&d(b.1))
            });
        }
        Ok(ends[0].0.clone())
    }

    fn pick_slot(
        &self,
        target: &Target,
        face: Option<&(mitcad_model::ids::BodyUid, mitcad_model::topo::FaceName)>,
        point: Option<V3>,
    ) -> Result<String, ModelError> {
        let mut slots = target.info.slots();
        // A member's face frames where it has no slots (mitcad#478).
        let faces: Vec<(&String, &Transform)> = target
            .info
            .sides()
            .into_iter()
            .filter(|(n, _)| target.info.frame_kind(n) == "face")
            .collect();
        if let Some((body, face)) = face
            && let Some(shape) = self.body_shape(*body)
            && let Ok(mitcad_model::datum::SurfaceGeometry::Plane { normal, .. }) =
                self.kernel().face_geometry(shape, face)
            && let Some(n) = unit(normal)
        {
            slots.retain(|(_, t)| dot(z_axis(t), n) > 1.0 - 1e-6);
            if slots.is_empty() {
                slots = faces.clone();
                slots.retain(|(_, t)| dot(z_axis(t), n) > 1.0 - 1e-6);
            }
            if slots.is_empty() {
                return Err(refuse(format!(
                    "the side has no slot frame: face {face} of {} is a closed side",
                    target.info.name
                )));
            }
        }
        if slots.is_empty() {
            slots = faces;
        }
        if slots.is_empty() {
            return Err(refuse(format!(
                "{} has no slot frames to place on",
                target.info.name
            )));
        }
        if let Some(p) = point {
            let distance = |t: &Transform| {
                let [o, x, _, _] = parts(t);
                let along = dot(sub(p, o), x);
                crate::geometry::norm(sub(sub(p, o), crate::geometry::scale(x, along)))
            };
            slots.sort_by(|a, b| distance(a.1).total_cmp(&distance(b.1)));
        }
        Ok(slots[0].0.clone())
    }

    fn plan_side(
        &self,
        source: &SourceUse,
        target: &Target,
        slot: &str,
        at: &PlaceAt,
        touch: &str,
        new: &NewPart<'_>,
        length: &LengthPlan,
    ) -> Result<(Spot, &'static str), ModelError> {
        let label = source.def.label();
        let mounts = new.mounts;
        self.check_fits(source, target, slot)?;
        let slot_local = *target
            .info
            .mount(slot)
            .ok_or_else(|| refuse(format!("{} has no frame {slot}", target.info.name)))?;
        // A profile stands on the slot with its end; another part with its
        // frame that mounts on a slot: the one its slot mate names (a
        // slider, mitcad#448), else its first slot, face or hole frame.
        let mounting = source
            .meta
            .linear
            .as_ref()
            .and_then(|l| l.mate.as_ref())
            .filter(|m| m.on == "slot" && mounts.contains_key(&m.frame))
            .map(|m| m.frame.clone())
            .or_else(|| own_frame(&source.meta, mounts, &["slot", "face", "hole"]));
        let own = match mounting {
            Some(own) if source.meta.role != BuildRole::Profile => own,
            _ if source.meta.length.is_some() && mounts.contains_key(touch) => touch.to_owned(),
            Some(own) => own,
            None => {
                return Err(refuse(format!(
                    "{label} has no frame that mounts on a slot"
                )));
            }
        };
        // Mating frames face each other: z against z.
        let flip = true;
        // How far the new part reaches along the slot from its frame.
        let mut own_local = mounts.get(&own).copied().unwrap_or(Transform::IDENTITY);
        // A member standing on a member's side lies as the target does
        // (mitcad#492): its own frame turned a quarter about its axis when
        // only that puts its section's axes parallel to the target's
        // (`rotate` turns it further).
        let mut own_turn = None;
        if new.tilt.abs() <= 1e-12
            && own == touch
            && source.meta.is_timber()
            && target.info.is_timber()
            && let (Some(own_box), Some(target_box)) =
                (new.bounds, self.part_bounds(target.info.component))
            && let Some(o) = side_lie(&slot_local, &own_local, own_box, &target_box)
        {
            own_local =
                mitcad_model::internal::joints::apply_override(&own_local, &o).map_err(refuse)?;
            own_turn = Some(o);
        }
        // Tilted along the host (mitcad#444): its foot cut to lie on the
        // side, its long point where the slide puts it.
        let length_now = match length {
            LengthPlan::Value(_, v) => Some(*v),
            LengthPlan::Bound { first, .. } => Some(*first),
            LengthPlan::None => None,
        };
        let tilted = if new.tilt.abs() > 1e-12 {
            let (Some(section), Some(length_now), true) = (new.bounds, length_now, own == touch)
            else {
                return Err(refuse(format!(
                    "{label} does not stand on the slot with an end: it cannot tilt"
                )));
            };
            let plan = crate::tilt::tilt_plan(
                section,
                &own_local,
                new.rotate,
                own == "end_b",
                length_now,
                new.tilt,
            )
            .ok_or_else(|| refuse(format!("{label} cannot tilt along the slot")))?;
            Some((plan, *section, length_now))
        } else {
            None
        };
        let (near, far) = match &tilted {
            Some((plan, ..)) => plan.reach,
            None => reach_along_x(new.bounds, &own_local, new.rotate, flip),
        };
        let section = match (new.bounds, source.meta.length.is_some()) {
            (Some(b), true) => (b.max[0] - b.min[0]).max(b.max[1] - b.min[1]),
            _ => f64::INFINITY,
        };
        let mut side = SidePlan {
            near,
            far,
            length: target.info.length.map(|(_, l)| l),
            snap: (far - near).min(section).max(0.0),
            grid: target
                .info
                .meta
                .system
                .as_ref()
                .and_then(|s| s.grid)
                .filter(|g| *g > 0.0)
                .unwrap_or(0.0),
            snapped: None,
            offset: 0.0,
            from: SideFrom::A,
            from_offset: 0.0,
            long_point: tilted.is_some(),
        };
        let point_local = at.point.map(|p| inverse(&target.placement).apply_point(p));
        let mut warnings = Vec::new();
        let (value, tx) = self.side_value(
            at,
            &slot_local,
            point_local,
            target.info.length,
            &mut side,
            &mut warnings,
            &format!("{label} stays on {}", target.info.name),
        )?;
        let frame_b = target
            .placement
            .after(&slot_local)
            .after(&Transform::translation([tx, 0.0, 0.0]));
        // A tilted part's foot cut, and with a bound length its far end
        // flush with the plane the length ends on, measured to its long
        // point (mitcad#444).
        let mut cuts = Vec::new();
        let mut far_end = None;
        if let Some((plan, section, length_now)) = &tilted {
            cuts.push((plan.foot_b, plan.foot.0, plan.foot.1));
            if let LengthPlan::Bound { to, .. } = length {
                let normal = self
                    .bound_normal(to)
                    .map(|n| inverse(&target.placement.after(&slot_local)).apply_vector(n))
                    .ok_or_else(|| refuse("the length's target is no plane"))?;
                let end = crate::tilt::far_end(plan, section, *length_now, normal)
                    .map_err(|e| refuse(format!("{label}: {e}")))?;
                if let Some((angle, cut_tilt)) = end.cut {
                    cuts.push((!plan.foot_b, angle, cut_tilt));
                }
                far_end = Some(end);
            }
        }
        let ray = match length {
            LengthPlan::Bound { .. } if own == touch => Some(match &far_end {
                Some(end) => RayPlan {
                    from: target.origin(slot)?,
                    shift: [tx + end.start[0], end.start[1], end.start[2]],
                    shift_by_joint: true,
                    slide_extra: end.start[0],
                    direction: RayDirection::Vector(end.along),
                    start: frame_b.apply_point(end.start),
                    along: frame_b.apply_vector(end.along),
                },
                None => RayPlan {
                    from: target.origin(slot)?,
                    shift: [tx, 0.0, 0.0],
                    shift_by_joint: true,
                    slide_extra: 0.0,
                    direction: RayDirection::default(),
                    start: frame_b.translation,
                    along: z_axis(&frame_b),
                },
            }),
            LengthPlan::Bound { .. } => {
                return Err(refuse(format!(
                    "{label} stands on the slot with no end: its length cannot be bound"
                )));
            }
            _ => None,
        };
        Ok((
            Spot::Joint(Box::new(JointPlan {
                kind: JointKind::Slider,
                slide: SlideAxis::X,
                own,
                b: target.origin(slot)?,
                frame_b,
                flip,
                slide_value: Some((Motion::Tx, value, tx)),
                ray,
                from: Some(side.from),
                side: Some(side),
                end: None,
                own_override: tilted.as_ref().map(|(plan, ..)| plan.own).or(own_turn),
                cuts,
                warnings,
            })),
            "side",
        ))
    }

    fn side_value(
        &self,
        at: &PlaceAt,
        slot_local: &Transform,
        point_local: Option<V3>,
        target_length: Option<(ParamId, f64)>,
        side: &mut SidePlan,
        warnings: &mut Vec<String>,
        what: &str,
    ) -> Result<(ValueInput, f64), ModelError> {
        let (near, far) = (side.near, side.far);
        // What the offsets are to: the near and far faces, or a tilted
        // part's long point (mitcad#444).
        let (rn, rf) = if side.long_point {
            (0.0, 0.0)
        } else {
            (near, far)
        };
        let length = side.length;
        let along = |p: V3| dot(sub(p, slot_local.translation), x_axis(slot_local));
        let (mut from, mut offset, tx) = match &at.offset {
            Some(offset) => {
                let from = at.from.unwrap_or_default();
                let distance = value_in(self, offset, Unit::MM)?;
                let tx = slide_of(from, distance, rn, rf, length)?;
                (from, offset.clone(), tx)
            }
            None => {
                // The part centred on the click, then snapped; the snap
                // distances at least the app's (`at.snap`).
                let least = at.snap.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(0.0);
                let middle = (near + far) / 2.0;
                let mut tx = point_local.map_or(-near, along) - middle;
                if let Some(l) = length {
                    let face = tx + near;
                    let ends = side.snap.max(least);
                    let fine = (side.snap / 4.0).max(least);
                    if face < ends {
                        tx = -near;
                        side.snapped = Some("end_a");
                    } else if l - (tx + far) < ends {
                        tx = l - far;
                        side.snapped = Some("end_b");
                    } else if (tx + middle - l / 2.0).abs() < fine {
                        tx = l / 2.0 - middle;
                        side.snapped = Some("middle");
                    } else if side.grid > 0.0 {
                        let grid = (face / side.grid).round() * side.grid;
                        let within = (side.snap / 4.0)
                            .min(side.grid / 4.0)
                            .max(least)
                            .min(side.grid / 2.0);
                        if (face - grid).abs() < within {
                            tx = grid - near;
                            side.snapped = Some("grid");
                        }
                    }
                }
                let from = match (at.from, side.snapped, length) {
                    (Some(from), _, _) => from,
                    (None, Some("end_a"), _) | (None, _, None) => SideFrom::A,
                    (None, Some("end_b"), _) => SideFrom::B,
                    (None, Some("middle"), _) => SideFrom::Middle,
                    (None, _, Some(l)) if l - (tx + far) < tx + near => SideFrom::B,
                    (None, _, Some(_)) => SideFrom::A,
                };
                let distance = offset_of(from, tx, rn, rf, length);
                // A click is no finer than a thousandth of a millimetre.
                let distance = (distance * 1000.0).round() / 1000.0;
                let tx = slide_of(from, distance, rn, rf, length)?;
                (from, ValueInput::Number(distance), tx)
            }
        };
        // Never past an end of the target (a part longer than the target
        // stays flush with end A).
        let kept = match length {
            Some(l) => tx.min(l - far).max(-near),
            None => tx,
        };
        if (kept - tx).abs() > 1e-9 {
            let flush = if (kept + near).abs() <= 1e-9 {
                SideFrom::A
            } else {
                SideFrom::B
            };
            if at.offset.is_some() {
                warnings.push(format!(
                    "{what}: flush with end {} instead",
                    if flush == SideFrom::A { "A" } else { "B" }
                ));
            }
            if side.snapped.is_none() {
                side.snapped = Some("clamped");
            }
            from = flush;
            // 0 for the faces; a tilted part's long point is in from them.
            offset = ValueInput::Number(offset_of(flush, kept, rn, rf, length));
        }
        let name = target_length.map(|(id, _)| self.parameters().name(id));
        let value = slide_value_of(from, &offset, rn, rf, name.as_deref())?;
        side.offset = kept + near;
        side.from = from;
        side.from_offset = offset_of(from, kept, rn, rf, length);
        Ok((value, kept))
    }

    fn slide_value(
        &self,
        at: &PlaceAt,
        frame_local: &Transform,
        point_local: Option<V3>,
        target_length: Option<(ParamId, f64)>,
    ) -> Result<(ValueInput, f64), ModelError> {
        let along = |p: V3| dot(sub(p, frame_local.translation), x_axis(frame_local));
        let clamp = |v: f64| match target_length {
            Some((_, l)) => v.clamp(0.0, l),
            None => v,
        };
        Ok(match &at.offset {
            Some(offset) => {
                let from = at.from.unwrap_or_default();
                let distance = value_in(self, offset, Unit::MM)?;
                let length = target_length.map(|(_, l)| l);
                let name = target_length.map(|(id, _)| self.parameters().name(id));
                (
                    slide_value_of(from, offset, 0.0, 0.0, name.as_deref())?,
                    slide_of(from, distance, 0.0, 0.0, length)?,
                )
            }
            None => {
                let v = clamp(point_local.map_or(0.0, along));
                (ValueInput::Number(v), v)
            }
        })
    }

    fn plan_mate(
        &self,
        source: &SourceUse,
        target: &Target,
        on: &str,
        own: &str,
        mate: Option<&crate::LinearMate>,
        at: &PlaceAt,
        mounts: &BTreeMap<String, Transform>,
    ) -> Result<Spot, ModelError> {
        let label = source.def.label();
        self.check_fits(source, target, on)?;
        // A board against a board (mitcad#477): its groove on the other's
        // tongue, its tongue in the other's groove, when the profiles match.
        let kind = target.info.frame_kind(on);
        if let Some(pair) = crate::profiled::mating_edge(&kind) {
            crate::profiled::check_match(
                &source.meta,
                &label,
                &target.info.meta,
                &target.info.name,
            )
            .map_err(refuse)?;
            let own = own_frame(&source.meta, mounts, &[pair]).ok_or_else(|| {
                refuse(format!(
                    "{label} has no {pair}: it does not fit {}'s {kind}",
                    target.info.name
                ))
            })?;
            return Ok(Spot::Joint(Box::new(JointPlan {
                kind: JointKind::Rigid,
                slide: SlideAxis::Z,
                own,
                b: target.origin(on)?,
                frame_b: target.placement.after(
                    target
                        .info
                        .mount(on)
                        .ok_or_else(|| refuse(format!("{} has no frame {on}", target.info.name)))?,
                ),
                flip: true,
                slide_value: None,
                ray: None,
                side: None,
                end: None,
                own_override: None,
                cuts: Vec::new(),
                from: None,
                warnings: Vec::new(),
            })));
        }
        let own = if own.is_empty() || !mounts.contains_key(own) {
            own_frame(&source.meta, mounts, &[kind.as_str(), "face", "slot"])
                .or_else(|| mounts.keys().next().cloned())
                .ok_or_else(|| refuse(format!("{label} has no frames")))?
        } else {
            own.to_owned()
        };
        let (kind, slide) = match mate {
            Some(mate) => {
                let kind = serde_json::from_value::<JointKind>(serde_json::json!(mate.joint))
                    .unwrap_or(JointKind::Slider);
                // A slide axis only on sliders (a rigid shaft support's
                // mate names none).
                let slide = match kind {
                    JointKind::Slider => {
                        serde_json::from_value::<SlideAxis>(serde_json::json!(mate.axis))
                            .unwrap_or(SlideAxis::X)
                    }
                    _ => SlideAxis::Z,
                };
                (kind, slide)
            }
            None => (JointKind::Rigid, SlideAxis::Z),
        };
        let on_local = *target
            .info
            .mount(on)
            .ok_or_else(|| refuse(format!("{} has no frame {on}", target.info.name)))?;
        let motion = match kind {
            JointKind::Slider => Some(slide.motion()),
            JointKind::Cylindrical => Some(Motion::Tz),
            _ => None,
        };
        let point_local = at.point.map(|p| inverse(&target.placement).apply_point(p));
        let slide_value = match motion {
            Some(Motion::Tx) => {
                let (value, v) =
                    self.slide_value(at, &on_local, point_local, target.info.length)?;
                Some((Motion::Tx, value, v))
            }
            Some(m) => {
                let v = match &at.offset {
                    Some(offset) => (offset.clone(), value_in(self, offset, Unit::MM)?),
                    None => (ValueInput::Number(0.0), 0.0),
                };
                Some((m, v.0, v.1))
            }
            None => None,
        };
        let shift = match &slide_value {
            Some((m, _, v)) => {
                let axis = match m {
                    Motion::Tx => [*v, 0.0, 0.0],
                    Motion::Ty => [0.0, *v, 0.0],
                    _ => [0.0, 0.0, *v],
                };
                Transform::translation(axis)
            }
            None => Transform::IDENTITY,
        };
        // Mating frames face each other: z against z.
        let flip = true;
        Ok(Spot::Joint(Box::new(JointPlan {
            kind,
            slide,
            own,
            b: target.origin(on)?,
            frame_b: target.placement.after(&on_local).after(&shift),
            flip,
            slide_value,
            ray: None,
            side: None,
            end: None,
            own_override: None,
            cuts: Vec::new(),
            from: matches!(motion, Some(Motion::Tx)).then(|| match at.offset {
                Some(_) => at.from.unwrap_or_default(),
                None => SideFrom::A,
            }),
            warnings: Vec::new(),
        })))
    }

    fn plan_on_geometry(
        &self,
        source: &SourceUse,
        path: &[OccurrenceUid],
        geometry: &GeomRef,
        at: &PlaceAt,
        touch: &str,
        mounts: &BTreeMap<String, Transform>,
        lie: Option<crate::Lie>,
        bounds: Option<&BoundingBox>,
        rotate: f64,
    ) -> Result<Spot, ModelError> {
        let label = source.def.label();
        let local = mitcad_model::internal::joints::origin_frame(&mut self.resolver(), geometry)
            .map_err(|e| refuse(format!("{geometry}: {e}")))?;
        let placement = self.path_transform(path);
        let point_local = at.point.map(|p| inverse(&placement).apply_point(p));
        let own = if source.meta.length.is_some() {
            touch.to_owned()
        } else {
            own_frame(&source.meta, mounts, &["end", "face", "slot"])
                .or_else(|| mounts.keys().next().cloned())
                .ok_or_else(|| refuse(format!("{label} has no frames")))?
        };
        // The axis the part runs along (`at.direction`, in the host's
        // coordinates, mitcad#394): the frame's z turned to it, unless it is
        // the frame's own; its first axis not along it keeps the roll.
        let turn = at
            .direction
            .and_then(unit)
            .map(|d| unit(inverse(&placement).apply_vector(d)).unwrap_or(d))
            .filter(|d| dot(*d, z_axis(&local)) < 1.0 - 1e-9);
        let hint = turn.map(|d| {
            let [_, ex, ey, ez] = parts(&local);
            match lie {
                Some(lie) => crate::timber::lie_axes(d, bounds, ez, lie, 0.0).0,
                None => [ex, ey, ez]
                    .into_iter()
                    .find(|a| dot(*a, d).abs() < 1.0 - 1e-6)
                    .unwrap_or(ex),
            }
        });
        let mut override_frame =
            (point_local.is_some() || turn.is_some()).then_some(FrameOverride {
                origin: point_local,
                z_axis: turn,
                x_axis: hint,
                shift: None,
            });
        let direction = match turn {
            Some(d) => RayDirection::Vector(inverse(&local).apply_vector(d)),
            None => RayDirection::default(),
        };
        let mut at_point = local;
        if let Some(o) = &override_frame {
            at_point = mitcad_model::internal::joints::apply_override(&local, o).map_err(refuse)?;
        }
        // Lying on the plane or face it rests on it, on its normal's side
        // (mitcad#492): lifted along the normal by how far its box, as the
        // joint places it, reaches below.
        let up = z_axis(&local);
        let lift = match (bounds, turn, override_frame.as_mut()) {
            (Some(b), Some(d), Some(o)) if dot(d, up).abs() < 1e-6 => {
                let own_local = mounts.get(&own).copied().unwrap_or(Transform::IDENTITY);
                let part = at_point
                    .after(&mitcad_model::internal::joints::alignment(
                        rotate, 0.0, true,
                    ))
                    .after(&inverse(&own_local));
                let lift = rest_lift(b, &part, at_point.translation, up);
                o.origin = Some(add(at_point.translation, scale(up, lift)));
                at_point =
                    mitcad_model::internal::joints::apply_override(&local, o).map_err(refuse)?;
                lift
            }
            _ => 0.0,
        };
        let shift = point_local.map_or([0.0; 3], |p| inverse(&local).apply_point(p));
        let frame_b = placement.after(&at_point);
        Ok(Spot::Joint(Box::new(JointPlan {
            kind: JointKind::Rigid,
            slide: SlideAxis::Z,
            own,
            b: JointOrigin {
                occurrence: OccurrencePath(path.to_vec()),
                geometry: geometry.clone(),
                frame_override: override_frame,
            },
            frame_b,
            flip: true,
            slide_value: None,
            ray: Some(RayPlan {
                from: JointOrigin {
                    occurrence: OccurrencePath(path.to_vec()),
                    geometry: geometry.clone(),
                    frame_override: None,
                },
                shift: [shift[0], shift[1], lift],
                shift_by_joint: false,
                slide_extra: 0.0,
                direction,
                start: frame_b.translation,
                along: z_axis(&frame_b),
            }),
            side: None,
            end: None,
            own_override: None,
            cuts: Vec::new(),
            from: None,
            warnings: Vec::new(),
        })))
    }
}

/// How far a part lying on a plane moves along the plane's normal `up` to
/// rest on it (mitcad#492): its box (moved by `placement`) with its lowest
/// point on the plane through `origin`, on the normal's side.
fn rest_lift(bounds: &BoundingBox, placement: &Transform, origin: V3, up: V3) -> f64 {
    -crate::geometry::box_along(bounds, placement, origin, up).0
}

/// The least and the most of points along a direction.
fn extent(points: &[V3], along: V3) -> (f64, f64) {
    points
        .iter()
        .map(|p| dot(*p, along))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

/// The turn a click at `p` on a profile's end face asks for (mitcad#417),
/// in the target's coordinates: none in the inner half of the face both
/// ways (at least `least` across, as far as the face allows), else 90°
/// towards the edge of the band it is in (the nearest edge in a corner),
/// along ±x or ±y of the end frame (origin `e`, axes `ex`, `ey`). The face
/// is the target's box across the end frame's z.
pub(crate) fn end_zone(
    p: V3,
    e: V3,
    ex: V3,
    ey: V3,
    target: &BoundingBox,
    least: f64,
) -> Option<V3> {
    let corners: Vec<V3> = target.corners().map(|c| sub(c, e)).collect();
    let q = sub(p, e);
    // Across an axis: the click from the face's middle, the face's half
    // width, and the inner zone's half width.
    let across = |axis: V3| {
        let (lo, hi) = extent(&corners, axis);
        let half = (hi - lo) / 2.0;
        let inner = (half / 2.0).max((least / 2.0).min(half));
        (dot(q, axis) - (lo + hi) / 2.0, half, inner)
    };
    let (dx, hx, ix) = across(ex);
    let (dy, hy, iy) = across(ey);
    let along_x = match (dx.abs() >= ix, dy.abs() >= iy) {
        (false, false) => return None,
        (true, false) => true,
        (false, true) => false,
        (true, true) => hx - dx.abs() <= hy - dy.abs(),
    };
    Some(if along_x {
        scale(ex, if dx < 0.0 { -1.0 } else { 1.0 })
    } else {
        scale(ey, if dy < 0.0 { -1.0 } else { 1.0 })
    })
}

/// Where a corner moves the new part's end frame from the target's end
/// frame origin `e` (mitcad#417), in the target's coordinates. `u` is the
/// end frame's z (out of the target), `d` the new part's axis; `to_target`
/// takes the new part's coordinates into the target's with its end frame
/// at `e` turned to `d`. The new part lies in the target's plane (the plane
/// of `u` and `d`), centred on the target's section across it, and:
///
/// - without a shared mitre on the target's end face, its end `offset`
///   from the target's outer side (the side away from the turn) towards
///   the turn: at 0 flush with it;
/// - with a shared mitre, the outer corners meet (mitcad#443): the new
///   part's outer side (away from the corner's inside) runs through the
///   target's outer edge at its end, and its end lies on that edge, so the
///   two overlap fully and both are cut on the bisector through the shared
///   outer corner, each at its long point; the outer lengths are the
///   lengths.
#[allow(clippy::too_many_arguments)]
pub fn corner_shift(
    e: V3,
    u: V3,
    d: V3,
    target: &BoundingBox,
    new: &BoundingBox,
    to_target: &Transform,
    offset: f64,
    shared: bool,
) -> V3 {
    // Towards the turn across the target, across the new part away from
    // the corner's inside, and across both.
    let s = unit(sub(d, scale(u, dot(d, u)))).unwrap_or(d);
    let t = unit(sub(u, scale(d, dot(u, d)))).unwrap_or(u);
    let n = unit(cross(u, d)).unwrap_or_else(|| perpendicular(u));
    let target_corners: Vec<V3> = target.corners().map(|c| sub(c, e)).collect();
    let new_corners: Vec<V3> = new
        .corners()
        .map(|c| sub(to_target.apply_point(c), e))
        .collect();
    // The target's outer side from its axis, and the centring across.
    let outer = extent(&target_corners, scale(s, -1.0)).1;
    let (tl, th) = extent(&target_corners, n);
    let (nl, nh) = extent(&new_corners, n);
    let centred = scale(n, (tl + th) / 2.0 - (nl + nh) / 2.0);
    if shared {
        // The target's long point, its outer edge at its end, is the new
        // part's: its end frame that far in from its outer side. Both cut
        // planes (across `u + d`) run through that corner.
        let height = extent(&new_corners, t).1;
        let corner = scale(s, -outer);
        let start = sub(corner, scale(t, height));
        return add(start, centred);
    }
    // On the end face: lifted by what reaches below it, then along its
    // axis until its end is `offset` from the outer side.
    let lift = scale(u, extent(&new_corners, scale(u, -1.0)).1);
    let least = extent(&new_corners, s).0 + dot(lift, s);
    let along = (offset - outer - least) / dot(d, s);
    add(add(lift, scale(d, along)), centred)
}

/// The axis of a section a part is thinnest along, in its coordinates (X
/// or Y of its box); None for a square section.
fn thickness_axis(section: &BoundingBox) -> Option<V3> {
    let (w, t) = (
        section.max[0] - section.min[0],
        section.max[1] - section.min[1],
    );
    if (w - t).abs() <= 1e-6 {
        None
    } else if w < t {
        Some(crate::geometry::X)
    } else {
        Some(crate::geometry::Y)
    }
}

/// The x axis of the frame a member goes on at a member's end (mitcad#485),
/// in the target's coordinates: `frame` (the end frame) turned to `z` (the
/// new part's axis) with `hint` as its x, or with `hint` turned a quarter
/// about `z` when only that puts the new part's thickness axis (its own
/// frame `own` mated flipped, not rotated) parallel to the target's. None
/// when `hint` does already, or neither can (a section is square, or the
/// target's thickness axis runs along `z`).
pub(crate) fn same_lie_x(
    frame: &Transform,
    z: V3,
    hint: V3,
    own: &Transform,
    new: &BoundingBox,
    target: &BoundingBox,
) -> Option<V3> {
    let (thin_new, thin_target) = (thickness_axis(new)?, thickness_axis(target)?);
    let parallel = |x: V3| {
        let o = FrameOverride {
            origin: None,
            z_axis: Some(z),
            x_axis: Some(x),
            shift: None,
        };
        mitcad_model::internal::joints::apply_override(frame, &o).is_ok_and(|turned| {
            let to_target = turned
                .after(&mitcad_model::internal::joints::alignment(0.0, 0.0, true))
                .after(&inverse(own));
            dot(to_target.apply_vector(thin_new), thin_target).abs() > 1.0 - 1e-6
        })
    };
    if parallel(hint) {
        return None;
    }
    let quarter = cross(z, hint);
    (unit(quarter).is_some() && parallel(quarter)).then_some(quarter)
}

/// The override of the own frame `own` (an end) of a member standing on a
/// member's side (mitcad#492), mated flipped and not rotated with the side
/// frame `slot` (in the target's coordinates): its x turned a quarter about
/// its axis when only that puts the new member's thickness axis parallel
/// to the target's, or, where its axis takes the target's thickness axis
/// (on the target's wide face), its width axis parallel to the target's,
/// so that same-size members have flush sides. None when it lies so
/// already, or neither can (a section is square).
pub(crate) fn side_lie(
    slot: &Transform,
    own: &Transform,
    new: &BoundingBox,
    target: &BoundingBox,
) -> Option<FrameOverride> {
    let (thin_new, thin_target) = (thickness_axis(new)?, thickness_axis(target)?);
    let wide = |thin: V3| {
        if thin[0].abs() > 0.5 {
            crate::geometry::Y
        } else {
            crate::geometry::X
        }
    };
    let lies = |own: &Transform| {
        let to_target = slot
            .after(&mitcad_model::internal::joints::alignment(0.0, 0.0, true))
            .after(&inverse(own));
        let parallel = |a: V3, b: V3| dot(to_target.apply_vector(a), b).abs() > 1.0 - 1e-6;
        (
            parallel(thin_new, thin_target),
            parallel(wide(thin_new), wide(thin_target)),
        )
    };
    let o = FrameOverride {
        origin: None,
        z_axis: None,
        x_axis: Some(parts(own)[2]),
        shift: None,
    };
    let turned = mitcad_model::internal::joints::apply_override(own, &o).ok()?;
    let (now, quarter) = (lies(own), lies(&turned));
    let turn = if now.0 || quarter.0 {
        quarter.0 && !now.0
    } else {
        quarter.1 && !now.1
    };
    turn.then_some(o)
}

/// The name of the target's end a joint's b side names.
fn target_end_name(b: &JointOrigin) -> String {
    match &b.geometry {
        GeomRef::Mount { name, .. } => name.clone(),
        _ => "end_b".to_owned(),
    }
}

/// The first frame of the part of one of the kinds (in that order).
pub(crate) fn own_frame(
    meta: &BuildMeta,
    mounts: &BTreeMap<String, Transform>,
    kinds: &[&str],
) -> Option<String> {
    for kind in kinds {
        if let Some(frame) = meta
            .frames
            .iter()
            .find(|f| f.kind == *kind && mounts.contains_key(&f.name))
        {
            return Some(frame.name.clone());
        }
    }
    None
}

/// Adds a feature and returns its uid.
pub(crate) fn add_feature(
    state: &mut DocState,
    def: FeatureDef<ValueInput>,
    component: ComponentUid,
) -> Result<FeatureUid, ModelError> {
    Ok(state.add_feature_in(&def, None, component)?.uid)
}

/// Moves a feature to just before another one.
pub(crate) fn move_before(state: &mut DocState, uid: FeatureUid, before: FeatureUid) {
    let Some(from) = state.position(uid) else {
        return;
    };
    let entry = state.features.remove(from);
    let to = state.position(before).unwrap_or(state.features.len());
    state.features.insert(to, entry);
}

/// The parameter a joint slides by.
fn joint_slide_param(state: &DocState, joint: FeatureUid, motion: Motion) -> Option<ParamId> {
    match &state.entry(joint)?.def {
        FeatureDef::Joint(j) => j.position.get(&motion).copied(),
        _ => None,
    }
}

/// Edits a derived part's definition in place: its values and ends.
pub(crate) fn edit_part(
    state: &mut DocState,
    part: FeatureUid,
    edit: impl FnOnce(&mut DerivedPartDef<ValueInput>),
) -> Result<(), ModelError> {
    let entry = state
        .entry(part)
        .ok_or(ModelError::NoSuchFeature(part))?
        .clone();
    let FeatureDef::DerivedPart(_) = &entry.def else {
        return Err(refuse(format!("{} is no derived part", entry.name)));
    };
    let FeatureDef::DerivedPart(mut def) = as_input(&entry.def, &state.parameters) else {
        unreachable!("a derived part stays one")
    };
    edit(&mut def);
    state.edit_feature(part, &FeatureDef::DerivedPart(def))?;
    Ok(())
}

/// Sets a derived part's length to a parameter (a bound length's), the
/// one it had going when nothing else uses it.
pub(crate) fn set_part_length(
    state: &mut DocState,
    part: FeatureUid,
    name: &str,
    id: ParamId,
) -> Result<(), ModelError> {
    let position = state
        .position(part)
        .ok_or(ModelError::NoSuchFeature(part))?;
    let mut entry: FeatureEntry = (*state.features[position]).clone();
    let FeatureDef::DerivedPart(def) = &mut entry.def else {
        return Err(refuse(format!("{} is no derived part", entry.name)));
    };
    def.parameters.insert(name.to_owned(), id);
    state.features[position] = Arc::new(entry);
    state.release_params(part);
    Ok(())
}

/// Adds a bound length for a part before its derived part; returns it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_bound(
    state: &mut DocState,
    component: ComponentUid,
    part: FeatureUid,
    length_name: &str,
    ray: &RayPlan,
    shift_x: Option<ParamId>,
    to: &BoundTo,
    mode: BoundMode,
    add: Option<ValueInput>,
    first: f64,
) -> Result<FeatureUid, ModelError> {
    let mut shift = BTreeMap::new();
    match shift_x {
        Some(id) => {
            let slide = state.parameters.name(id);
            shift.insert(
                ShiftAxis::X,
                ValueInput::Name(format!("{slide}{}", plus(ray.slide_extra))),
            );
        }
        None if ray.shift[0].abs() > 1e-12 => {
            shift.insert(ShiftAxis::X, ValueInput::Number(ray.shift[0]));
        }
        None => {}
    }
    if ray.shift[1].abs() > 1e-12 {
        shift.insert(ShiftAxis::Y, ValueInput::Number(ray.shift[1]));
    }
    if ray.shift[2].abs() > 1e-12 {
        shift.insert(ShiftAxis::Z, ValueInput::Number(ray.shift[2]));
    }
    let def = BoundLengthDef {
        parameter: ValueInput::Number(first),
        from: BoundFrom {
            occurrence: ray.from.occurrence.clone(),
            geometry: ray.from.geometry.clone(),
            shift,
        },
        direction: ray.direction,
        to: to.clone(),
        mode,
        add,
    };
    let bound = add_feature(state, FeatureDef::BoundLength(def), component)?;
    move_before(state, bound, part);
    let parameter = match &state.entry(bound).expect("added").def {
        FeatureDef::BoundLength(b) => b.parameter,
        _ => unreachable!("a bound length"),
    };
    set_part_length(state, part, length_name, parameter)?;
    let position = state.position(bound).expect("added");
    let entry = state.features[position].clone();
    state.check_feature(position, &entry).map_err(refuse)?;
    Ok(bound)
}

/// Makes what a plan says, in `state`: the source, the component, its
/// occurrence, the derived part, the joint, the bound length, the mitres
/// and the record.
pub(crate) fn make_part(
    state: &mut DocState,
    plan: &Plan,
    active: ComponentUid,
) -> Result<Placed, ModelError> {
    let source_id = plan.source.def.id;
    if plan.source.new && state.library_sources.get(source_id).is_none() {
        state.library_sources.0.push(plan.source.def.clone());
    }
    let wanted = plan.name.clone().unwrap_or_else(|| {
        if plan.designation.is_empty() {
            plan.source.def.label()
        } else {
            plan.designation.clone()
        }
    });
    let name = match &plan.name {
        Some(name) => {
            state.check_component_name(name, ComponentUid(u32::MAX))?;
            name.clone()
        }
        None => state.free_component_name(&wanted),
    };
    let made = state.assembly.create(Some(&name), None);
    let (transform, grounded) = match &plan.spot {
        Spot::Grounded(t) => (*t, true),
        Spot::Joint(_) => (Transform::IDENTITY, false),
    };
    let occurrence = state.assembly.place(made, active, transform);
    if grounded && let Some(o) = state.assembly.occurrence_mut(occurrence) {
        o.grounded = true;
    }
    let mut features = Vec::new();
    let part_def = DerivedPartDef {
        source: source_id,
        config: plan.source.config.clone(),
        parameters: plan.values.clone(),
        ends: plan.own_ends.clone(),
        operation: Operation::NewBody,
        cuts: Vec::new(),
    };
    let part = add_feature(state, FeatureDef::DerivedPart(part_def), made)?;
    crate::parts::inherit_appearance(state, part, &plan.source.def);
    features.push(part);
    let (joint, bound) = attach_part(state, plan, active, occurrence, part, None)?;
    if let Some(j) = joint {
        features.push(j);
    }
    for b in bound.iter().rev() {
        features.insert(0, *b);
    }
    let record = BuildRecord {
        role: plan.source.meta.role,
        source: Some(source_id),
        part: Some(part),
        joint,
        bound: bound.clone(),
        added_by: None,
        adds: Vec::new(),
        junction: None,
        from: match &plan.spot {
            Spot::Joint(j) => j.from,
            Spot::Grounded(_) => None,
        },
        tilt: (plan.tilt.abs() > 1e-12).then_some(plan.tilt),
        grade: plan.grade.clone(),
        set: None,
        spacing: None,
        points: plan.points.clone(),
        frame: None,
        framing: None,
        member: None,
    };
    state.assembly.component_mut(made).expect("made").build = Some(record);
    Ok(Placed {
        component: Some(made),
        occurrence: Some(occurrence),
        name,
        features,
        joint,
        bound,
        adds: Vec::new(),
        warnings: plan.warnings.clone(),
        place: plan.place,
        fits: None,
        set: Vec::new(),
    })
}

/// Places a made part as a plan says: its joint, its bound length (before
/// its derived part) and the target's mitre; returns the joint and the
/// bound lengths. A spacing set lays its members out again with it
/// (mitcad#478).
pub(crate) fn attach_part(
    state: &mut DocState,
    plan: &Plan,
    active: ComponentUid,
    occurrence: OccurrenceUid,
    part: FeatureUid,
    reuse: Option<FeatureUid>,
) -> Result<(Option<FeatureUid>, Vec<FeatureUid>), ModelError> {
    let mut features: Vec<FeatureUid> = Vec::new();
    let mut joint = None;
    let mut slide_param = None;
    if let Spot::Joint(plan_joint) = &plan.spot {
        let mut position = BTreeMap::new();
        if let Some((motion, value, _)) = &plan_joint.slide_value {
            position.insert(*motion, value.clone());
        }
        let def = JointDef {
            kind: plan_joint.kind,
            slide_axis: plan_joint.slide,
            a: JointOrigin {
                occurrence: OccurrencePath(vec![occurrence]),
                geometry: GeomRef::Mount {
                    feature: part,
                    name: plan_joint.own.clone(),
                },
                frame_override: plan_joint.own_override,
            },
            b: plan_joint.b.clone(),
            offset: None,
            angle: (plan.rotate.abs() > 1e-12).then_some(ValueInput::Number(plan.rotate)),
            flip: plan_joint.flip,
            limits: BTreeMap::new(),
            position,
        };
        let uid = match reuse {
            Some(old) if matches!(state.entry(old).map(|e| &e.def), Some(FeatureDef::Joint(j)) if j.kind == plan_joint.kind) =>
            {
                state.edit_feature(old, &FeatureDef::Joint(def))?;
                old
            }
            _ => {
                if let Some(old) = reuse.filter(|o| state.position(*o).is_some()) {
                    state.remove_features(&BTreeSet::from([old]));
                }
                add_feature(state, FeatureDef::Joint(def), active)?
            }
        };
        features.push(uid);
        joint = Some(uid);
        slide_param = plan_joint
            .slide_value
            .as_ref()
            .and_then(|(motion, _, _)| joint_slide_param(state, uid, *motion));
    }
    let mut bound = Vec::new();
    if let LengthPlan::Bound {
        to,
        mode,
        add,
        first,
        ..
    } = &plan.length
    {
        let ray = match &plan.spot {
            Spot::Joint(j) => j.ray.clone(),
            Spot::Grounded(t) => Some(RayPlan {
                from: JointOrigin {
                    occurrence: OccurrencePath::default(),
                    geometry: GeomRef::Origin(mitcad_model::datum::OriginDatum::Xy),
                    frame_override: None,
                },
                shift: t.translation,
                shift_by_joint: false,
                slide_extra: 0.0,
                direction: if dot(z_axis(t), crate::geometry::Z) > 1.0 - 1e-9 {
                    RayDirection::default()
                } else {
                    RayDirection::Vector(z_axis(t))
                },
                start: t.translation,
                along: z_axis(t),
            }),
        }
        .ok_or_else(|| refuse("the length cannot be bound here"))?;
        let length_name = plan
            .source
            .meta
            .length
            .as_ref()
            .map(|l| l.parameter.clone())
            .ok_or_else(|| refuse("the part is not cut to length"))?;
        let shift_x = if ray.shift_by_joint {
            slide_param
        } else {
            None
        };
        let uid = add_bound(
            state,
            active,
            part,
            &length_name,
            &ray,
            shift_x,
            to,
            *mode,
            add.clone(),
            *first,
        )?;
        features.insert(0, uid);
        bound.push(uid);
    }
    if let Some((target_part, b, cut)) = &plan.target_ends {
        let cut = cut.clone();
        edit_part(state, *target_part, |def| {
            if *b {
                def.ends.b = Some(cut);
            } else {
                def.ends.a = Some(cut);
            }
        })?;
    }
    // The other end of a corner the bound length closes (mitcad#485).
    if let Some(closing) = &plan.closing
        && let Some(cut) = &closing.cut
    {
        let cut = cut.clone();
        edit_part(state, closing.part, |def| {
            if closing.end_b {
                def.ends.b = Some(cut);
            } else {
                def.ends.a = Some(cut);
            }
        })?;
    }
    if matches!(plan.spot, Spot::Grounded(_))
        && let Some(old) = reuse.filter(|o| state.position(*o).is_some())
    {
        state.remove_features(&BTreeSet::from([old]));
    }
    let _ = features;
    Ok((joint, bound))
}

// Editing.

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait EditBuild<K: Kernel> {
    /// Edits a BUILD part (`build_edit`); returns what changed.
    fn build_edit(&mut self, input: &EditInput) -> Result<Vec<String>, ModelError>;

    /// A component by uid or name.
    fn component_by_ref(&self, text: &str) -> Result<ComponentUid, ModelError>;
}

impl<K: Kernel> EditBuild<K> for Document<K> {
    fn build_edit(&mut self, input: &EditInput) -> Result<Vec<String>, ModelError> {
        let component = self.component_by_ref(&input.component)?;
        // A timber frame is laid out again as one (mitcad#479).
        if let Some(frame) = &input.frame {
            return self
                .edit_frame(component, frame, input.config.as_ref())
                .map(|edited| edited.changed);
        }
        // A spacing set is edited as one (mitcad#478).
        if let Some(spacing) = &input.spacing {
            return self.edit_set(component, spacing, input.config.as_ref());
        }
        // A member by two points placed anew (mitcad#478).
        if input.to.is_some()
            || input.lie.is_some()
            || input.at.as_ref().is_some_and(|a| a.point.is_some())
        {
            return self.edit_points(component, input);
        }
        let info = self
            .part_info(component)
            .ok_or_else(|| refuse(format!("{} is no BUILD part", input.component)))?;
        let part = info
            .part
            .ok_or_else(|| refuse(format!("{} has no derived part", info.name)))?;
        let length_name = info.meta.length.as_ref().map(|l| l.parameter.clone());
        let host = self.build_host();
        // Checks that need the document as it is.
        let mut bind_plan = None;
        if let Some(LengthInput::Bound { bind, mode, add }) = &input.length {
            let length_name = length_name
                .clone()
                .ok_or_else(|| refuse(format!("{} is not cut to length", info.name)))?;
            let to_path = self.path_from_host(&bind.occurrence)?;
            let to_component = mitcad_model::internal::joints::path_component(
                self.assembly(),
                self.build_host(),
                &to_path,
            )
            .map_err(refuse)?;
            self.check_bind(component, to_component)?;
            let ray = self.part_ray(&info)?;
            bind_plan = Some((
                length_name,
                ray,
                BoundTo {
                    occurrence: OccurrencePath(to_path),
                    geometry: bind.geometry.clone(),
                },
                *mode,
                add.clone(),
            ));
        }
        if let Some(LengthInput::Value(value)) = &input.length {
            let length = value_in(self, value, Unit::MM)?;
            if let Some(problem) = info.meta.length.as_ref().and_then(|l| l.outside(length)) {
                return Err(refuse(problem));
            }
        }
        if let Some(config) = &input.config
            && let Some(source) = &info.source
            && source
                .design
                .state()
                .map_err(refuse)?
                .configurations
                .row(config)
                .is_none()
        {
            return Err(refuse(format!(
                "{} has no configuration {config}",
                source.label()
            )));
        }
        // The part's own end machining.
        let machining = match &input.machining {
            Some(list) => {
                let mut defs = Vec::new();
                for m in list {
                    let frame = if m.end == EndFrom::B {
                        "end_b"
                    } else {
                        "end_a"
                    };
                    let local = info
                        .mount(frame)
                        .ok_or_else(|| refuse(format!("{} has no frame {frame}", info.name)))?;
                    let def = crate::connect::hole_def(&info, frame, local.translation, &m.hole)
                        .ok_or_else(|| refuse("a hole needs its diameter"))?;
                    let def: FeatureDef<ValueInput> = serde_json::from_value(def)
                        .map_err(|e| refuse(format!("the hole: {e}")))?;
                    defs.push(def);
                }
                Some(defs)
            }
            None => None,
        };
        // Its joinery cut anew (mitcad#478).
        let refit = match input.cuts.as_deref() {
            Some("refit") => Some(self.refit_plan(component)?),
            Some(other) => {
                return Err(refuse(format!("cuts is \"refit\", not \"{other}\"")));
            }
            None => None,
        };
        let joint = info.record.joint;
        let label = format!("Edit {}", info.name);
        let rotate = match &input.rotate {
            Some(value) => {
                let angle = value_in(self, value, Unit::DEG)?;
                let quarter = angle / FRAC_PI_2;
                if (quarter - quarter.round()).abs() > 1e-6 {
                    return Err(refuse("the rotation is 0, 90, 180 or 270 degrees"));
                }
                Some(angle)
            }
            None => None,
        };
        // The offset along a side or a rail, bound to its reference
        // (mitcad#417): a new From alone keeps the part where it is.
        let offset = match &input.at {
            Some(EditAt { offset, from, .. }) if offset.is_some() || from.is_some() => {
                let joint = joint.ok_or_else(|| refuse(format!("{} has no joint", info.name)))?;
                let Some(FeatureDef::Joint(j)) = self.feature(joint).map(|e| &e.def) else {
                    return Err(refuse(format!("{} has no joint", info.name)));
                };
                let motion = match j.kind {
                    JointKind::Slider => j.slide_axis.motion(),
                    JointKind::Cylindrical => Motion::Tz,
                    _ => {
                        return Err(refuse(format!(
                            "{} is not on a side or a rail: it has no offset to edit",
                            info.name
                        )));
                    }
                };
                let (length, target_name) = self.joint_target_length(joint);
                // On a side the offset is to the part's near face (from end
                // A), its far face (from end B) or its middle, as
                // `build_place` measures it.
                let (near, far) = self.side_reach(&info, joint, rotate);
                let from_now = info.record.from.unwrap_or_default();
                let new_from = from.unwrap_or(from_now);
                let value = match offset {
                    Some(value) => value.clone(),
                    None => {
                        let tx = j
                            .position
                            .get(&motion)
                            .and_then(|id| self.parameters().value(*id))
                            .unwrap_or(0.0);
                        let distance = offset_of(new_from, tx, near, far, length);
                        ValueInput::Number((distance * 1000.0).round() / 1000.0)
                    }
                };
                let value = slide_value_of(new_from, &value, near, far, target_name.as_deref())?;
                Some((motion, value, new_from, from.is_some()))
            }
            _ => None,
        };
        // A tilt on a side (mitcad#444): the one asked for, or the part's
        // own again through a turn, a flip or a new length.
        let tilt = match &input.tilt {
            Some(value) => {
                let tilt = value_in(self, value, Unit::DEG)?;
                crate::tilt::check_tilt(tilt).map_err(refuse)?;
                Some(tilt)
            }
            None if crate::tilt::is_tilted(info.record.tilt)
                && (rotate.is_some() || input.flip.is_some() || input.length.is_some()) =>
            {
                info.record.tilt
            }
            None => None,
        };
        let retilt = match tilt {
            Some(tilt) => {
                Some(self.plan_retilt(&info, tilt, rotate, input.flip, input.length.as_ref())?)
            }
            None => None,
        };
        let rest = self.plan_rest(&info, rotate, input.flip);
        let side_turn = self.plan_side_lie(&info, input.flip);
        self.apply(|state| {
            let mut changed = Vec::new();
            if let Some((motion, value, from, from_given)) = offset {
                let joint = joint.expect("checked");
                edit_joint(state, joint, |j| {
                    j.position.insert(motion, value);
                })?;
                if let Some(record) = state
                    .assembly
                    .component_mut(component)
                    .and_then(|c| c.build.as_mut())
                {
                    record.from = Some(from);
                }
                changed.push("offset".to_owned());
                if from_given {
                    changed.push("from".to_owned());
                }
            }
            if let Some(angle) = rotate {
                let joint = joint.ok_or_else(|| refuse(format!("{} has no joint", info.name)))?;
                edit_joint(state, joint, |j| {
                    j.angle = Some(ValueInput::Number(angle));
                })?;
                changed.push("rotate".to_owned());
            }
            if let Some(flip) = input.flip {
                let joint = joint.ok_or_else(|| refuse(format!("{} has no joint", info.name)))?;
                let mut swapped = false;
                edit_joint(state, joint, |j| {
                    if let GeomRef::Mount { name, .. } = &mut j.a.geometry {
                        let wanted = if flip { "end_b" } else { "end_a" };
                        let current = name.as_str();
                        if (current == "end_a" || current == "end_b") && current != wanted {
                            *name = wanted.to_owned();
                            swapped = true;
                        }
                    }
                })?;
                if swapped {
                    // The cut goes with the end that meets.
                    edit_part(state, part, |def| {
                        std::mem::swap(&mut def.ends.a, &mut def.ends.b);
                    })?;
                    // On a member's side it lies as that member does from
                    // its other end too (mitcad#492).
                    if let Some(turn) = side_turn {
                        edit_joint(state, joint, |j| j.a.frame_override = turn)?;
                    }
                    changed.push("flip".to_owned());
                }
            }
            // Turned or flipped lying on a plane: resting on it again
            // (mitcad#492), a bound length's ray from the lifted axis.
            if let (Some((origin, lift)), Some(joint)) = (rest, joint) {
                edit_joint(state, joint, |j| {
                    if let Some(o) = j.b.frame_override.as_mut() {
                        o.origin = Some(origin);
                    }
                })?;
                for bound in current_bounds(state, component) {
                    edit_bound(state, bound, |b| {
                        if lift.abs() > 1e-12 {
                            b.from.shift.insert(ShiftAxis::Z, ValueInput::Number(lift));
                        } else {
                            b.from.shift.remove(&ShiftAxis::Z);
                        }
                    })?;
                }
            }
            match &input.length {
                Some(LengthInput::Value(value)) => {
                    let name = length_name
                        .clone()
                        .ok_or_else(|| refuse(format!("{} is not cut to length", info.name)))?;
                    for bound in current_bounds(state, component) {
                        detach(state, component, bound, part)?;
                        changed.push("detached".to_owned());
                    }
                    let value = value.clone();
                    edit_part(state, part, |def| {
                        def.parameters.insert(name, value);
                    })?;
                    changed.push("length".to_owned());
                }
                Some(LengthInput::Bound { .. }) => {
                    let (name, ray, to, mode, add) = bind_plan.take().expect("planned");
                    let current = current_bounds(state, component);
                    if let Some(bound) = current.first() {
                        edit_bound(state, *bound, |b| {
                            b.to = to;
                            b.mode = mode;
                            if let Some(add) = add {
                                b.add = Some(add);
                            }
                        })?;
                    } else {
                        let first = info.length.map_or(1.0, |(_, v)| v);
                        let shift_x = if ray.shift_by_joint {
                            joint.and_then(|j| {
                                joint_slide_param(state, j, Motion::Tx)
                                    .or_else(|| joint_slide_param(state, j, Motion::Tz))
                            })
                        } else {
                            None
                        };
                        // In the BUILD host, whose paths `to` and the
                        // joint use.
                        let uid = add_bound(
                            state, host, part, &name, &ray, shift_x, &to, mode, add, first,
                        )?;
                        if let Some(record) = state
                            .assembly
                            .component_mut(component)
                            .and_then(|c| c.build.as_mut())
                        {
                            record.bound.push(uid);
                        }
                    }
                    changed.push("bound".to_owned());
                }
                None => {}
            }
            if let Some(r) = &retilt {
                let joint = joint.ok_or_else(|| refuse(format!("{} has no joint", info.name)))?;
                edit_joint(state, joint, |j| j.a.frame_override = r.own)?;
                let [a, b] = r.ends.clone();
                edit_part(state, part, |def| {
                    def.ends.a = a;
                    def.ends.b = b;
                })?;
                // Its bound length measured from the far end's long side
                // along its axis.
                let slide =
                    joint_slide_param(state, joint, Motion::Tx).map(|id| state.parameters.name(id));
                let (start, along) = r.ray;
                for bound in current_bounds(state, component) {
                    edit_bound(state, bound, |b| {
                        if let Some(slide) = &slide {
                            b.from.shift.insert(
                                ShiftAxis::X,
                                ValueInput::Name(format!("{slide}{}", plus(start[0]))),
                            );
                        }
                        for (axis, value) in [(ShiftAxis::Y, start[1]), (ShiftAxis::Z, start[2])] {
                            if value.abs() > 1e-12 {
                                b.from.shift.insert(axis, ValueInput::Number(value));
                            } else {
                                b.from.shift.remove(&axis);
                            }
                        }
                        b.direction = if dot(along, crate::geometry::Z) > 1.0 - 1e-12 {
                            RayDirection::default()
                        } else {
                            RayDirection::Vector(along)
                        };
                    })?;
                }
                if let Some(record) = state
                    .assembly
                    .component_mut(component)
                    .and_then(|c| c.build.as_mut())
                {
                    record.tilt = (r.tilt.abs() > 1e-12).then_some(r.tilt);
                }
                changed.push("tilt".to_owned());
            }
            if let Some(ends) = &input.ends {
                let ends = ends.clone();
                edit_part(state, part, |def| {
                    let cut = |c: Option<EndCutInput>| {
                        c.map(|c| EndCut {
                            angle: c.angle,
                            tilt: c.tilt,
                        })
                    };
                    if let Some(a) = ends.a {
                        def.ends.a = cut(a);
                    }
                    if let Some(b) = ends.b {
                        def.ends.b = cut(b);
                    }
                })?;
                changed.push("ends".to_owned());
            }
            if let Some(config) = &input.config {
                let config = config.clone();
                edit_part(state, part, |def| def.config = Some(config))?;
                changed.push("config".to_owned());
            }
            if !input.parameters.is_empty() {
                let parameters = input.parameters.clone();
                edit_part(state, part, |def| {
                    for (name, value) in parameters {
                        def.parameters.insert(name, value);
                    }
                })?;
                changed.push("parameters".to_owned());
            }
            if let Some(defs) = machining {
                let own: BTreeSet<FeatureUid> = state
                    .assembly
                    .component(component)
                    .and_then(|c| c.build.as_ref())
                    .map(|r| {
                        r.adds
                            .iter()
                            .filter_map(|i| match i {
                                crate::BuildItem::Feature(f) => Some(*f),
                                crate::BuildItem::Component(_) => None,
                            })
                            .filter(|f| state.entry(*f).is_some_and(|e| e.component == component))
                            .collect()
                    })
                    .unwrap_or_default();
                if !own.is_empty() {
                    let mut all = own.clone();
                    all.extend(state.dependents(&own));
                    state.remove_features(&all);
                }
                let mut added = Vec::new();
                for def in defs {
                    added.push(crate::BuildItem::Feature(add_feature(
                        state, def, component,
                    )?));
                }
                if let Some(record) = state
                    .assembly
                    .component_mut(component)
                    .and_then(|c| c.build.as_mut())
                {
                    record.adds.retain(|i| match i {
                        crate::BuildItem::Feature(f) => !own.contains(f),
                        crate::BuildItem::Component(_) => true,
                    });
                    record.adds.extend(added);
                }
                changed.push("machining".to_owned());
            }
            if let Some(parts) = &refit {
                for (p, cuts) in parts {
                    let cuts = cuts.clone();
                    edit_part(state, *p, |def| def.cuts = cuts)?;
                }
                changed.push("cuts".to_owned());
            }
            if let Some(grade) = &input.grade {
                if let Some(record) = state
                    .assembly
                    .component_mut(component)
                    .and_then(|c| c.build.as_mut())
                {
                    record.grade = grade.clone().filter(|g| !g.trim().is_empty());
                }
                changed.push("grade".to_owned());
            }
            if changed.is_empty() {
                return Err(refuse("nothing to change"));
            }
            Ok((label, changed))
        })
    }

    fn component_by_ref(&self, text: &str) -> Result<ComponentUid, ModelError> {
        let a = self.assembly();
        if let Ok(uid) = text.parse::<ComponentUid>()
            && a.exists(uid)
        {
            return Ok(uid);
        }
        a.components
            .iter()
            .find(|c| c.name == text)
            .map(|c| c.uid)
            .ok_or_else(|| refuse(format!("no component {text}")))
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait EditCrateOps<K: Kernel> {
    /// Where a part lying on a plane or a face (placed on geometry that is
    /// no BUILD part, its axis along the plane) goes when an edit turns it
    /// (`rotate`) or flips it, to rest on the plane again (mitcad#492): its
    /// joint's override origin, in the plane's component's coordinates,
    /// and its lift off the plane. None for any other part or edit.
    fn plan_rest(
        &self,
        info: &PartInfo,
        rotate: Option<f64>,
        flip: Option<bool>,
    ) -> Option<(V3, f64)>;

    /// The own frame override of a timber member standing on a timber
    /// member's side (mitcad#492) when a Flip edit makes its other end the
    /// one that meets: [`side_lie`] for that end, as a fresh placement
    /// gives it (None inside: no turn). None for any other part or edit,
    /// and for a tilted part, whose override is its tilt's.
    fn plan_side_lie(&self, info: &PartInfo, flip: Option<bool>) -> Option<Option<FrameOverride>>;

    /// The length parameter of the part a joint's b side is on.
    fn joint_target_length(&self, joint: FeatureUid) -> (Option<f64>, Option<String>);

    /// How far a part on a profile's side reaches along the slot from its
    /// joint's slide ([`reach_along_x`]), at the rotation `rotate` (else
    /// its joint's); (0, 0) for a part placed otherwise (on an end, on a
    /// rail).
    fn side_reach(&self, info: &PartInfo, joint: FeatureUid, rotate: Option<f64>) -> (f64, f64);

    /// The ray a placed part's length is measured along: from the frame its
    /// joint places it on.
    fn part_ray(&self, info: &PartInfo) -> Result<RayPlan, ModelError>;

    /// Refuses a bound length that would make a cycle (the target's length
    /// bound, through others, to this part) or that ends on a part placed
    /// after this one.
    fn check_bind(&self, part: ComponentUid, target: ComponentUid) -> Result<(), ModelError>;

    /// The component a bound length's ray ends on.
    fn bound_target(&self, bound: FeatureUid) -> Option<ComponentUid>;
}

impl<K: Kernel> EditCrateOps<K> for Document<K> {
    fn plan_rest(
        &self,
        info: &PartInfo,
        rotate: Option<f64>,
        flip: Option<bool>,
    ) -> Option<(V3, f64)> {
        if rotate.is_none() && flip.is_none() {
            return None;
        }
        let Some(FeatureDef::Joint(j)) = self.feature(info.record.joint?).map(|e| &e.def) else {
            return None;
        };
        let o = j.b.frame_override?;
        let (Some(origin), Some(z)) = (o.origin, o.z_axis) else {
            return None;
        };
        if j.kind != JointKind::Rigid || self.target(&j.b.occurrence.0).is_some() {
            return None;
        }
        let GeomRef::Mount { name: own_name, .. } = &j.a.geometry else {
            return None;
        };
        let local =
            mitcad_model::internal::joints::origin_frame(&mut self.resolver(), &j.b.geometry)
                .ok()?;
        let up = z_axis(&local);
        if dot(unit(z)?, up).abs() >= 1e-6 {
            return None;
        }
        let own_name = match (flip, own_name.as_str()) {
            (Some(true), "end_a") => "end_b",
            (Some(false), "end_b") => "end_a",
            (_, name) => name,
        };
        let mut own = *info.mount(own_name)?;
        if let Some(a) = &j.a.frame_override {
            own = mitcad_model::internal::joints::apply_override(&own, a).ok()?;
        }
        let angle = rotate
            .or_else(|| j.angle.and_then(|id| self.parameters().value(id)))
            .unwrap_or(0.0);
        let bounds = self.part_bounds(info.component)?;
        let on_plane = sub(origin, scale(up, dot(sub(origin, local.translation), up)));
        let at_point = mitcad_model::internal::joints::apply_override(
            &local,
            &FrameOverride {
                origin: Some(on_plane),
                ..o
            },
        )
        .ok()?;
        let part = at_point
            .after(&mitcad_model::internal::joints::alignment(
                angle, 0.0, j.flip,
            ))
            .after(&inverse(&own));
        let lift = rest_lift(&bounds, &part, on_plane, up);
        Some((add(on_plane, scale(up, lift)), lift))
    }

    fn plan_side_lie(&self, info: &PartInfo, flip: Option<bool>) -> Option<Option<FrameOverride>> {
        let flip = flip?;
        if crate::tilt::is_tilted(info.record.tilt) || !info.is_timber() {
            return None;
        }
        let Some(FeatureDef::Joint(j)) = self.feature(info.record.joint?).map(|e| &e.def) else {
            return None;
        };
        let (GeomRef::Mount { name: own, .. }, GeomRef::Mount { name: slot, .. }) =
            (&j.a.geometry, &j.b.geometry)
        else {
            return None;
        };
        let wanted = if flip { "end_b" } else { "end_a" };
        if j.kind != JointKind::Slider
            || !matches!(own.as_str(), "end_a" | "end_b")
            || own == wanted
        {
            return None;
        }
        let target = self.target(&j.b.occurrence.0)?;
        if !target.info.is_timber() || !target.info.is_side(slot) {
            return None;
        }
        let slot_local = target.info.mount(slot)?;
        let own_local = info.mount(wanted)?;
        let own_box = self.part_bounds(info.component)?;
        let target_box = self.part_bounds(target.info.component)?;
        Some(side_lie(slot_local, own_local, &own_box, &target_box))
    }

    fn joint_target_length(&self, joint: FeatureUid) -> (Option<f64>, Option<String>) {
        let Some(FeatureDef::Joint(j)) = self.feature(joint).map(|e| &e.def) else {
            return (None, None);
        };
        let Ok(component) = mitcad_model::internal::joints::path_component(
            self.assembly(),
            self.feature(joint)
                .map_or(ComponentUid::ROOT, |e| e.component),
            &j.b.occurrence.0,
        ) else {
            return (None, None);
        };
        match self.part_info(component).and_then(|i| i.length) {
            Some((id, v)) => (Some(v), Some(self.parameters().name(id))),
            None => (None, None),
        }
    }

    fn side_reach(&self, info: &PartInfo, joint: FeatureUid, rotate: Option<f64>) -> (f64, f64) {
        // A tilted part's offsets are to its foot's long point, where its
        // slide is (mitcad#444).
        if crate::tilt::is_tilted(info.record.tilt) {
            return (0.0, 0.0);
        }
        let Some(entry) = self.feature(joint) else {
            return (0.0, 0.0);
        };
        let FeatureDef::Joint(j) = &entry.def else {
            return (0.0, 0.0);
        };
        let (GeomRef::Mount { name: own, .. }, GeomRef::Mount { name: on, .. }) =
            (&j.a.geometry, &j.b.geometry)
        else {
            return (0.0, 0.0);
        };
        let on_slot = mitcad_model::internal::joints::path_component(
            self.assembly(),
            entry.component,
            &j.b.occurrence.0,
        )
        .ok()
        .and_then(|c| self.part_info(c))
        .is_some_and(|t| t.is_side(on));
        if j.kind != JointKind::Slider || !on_slot {
            return (0.0, 0.0);
        }
        let mut own = info.mount(own).copied().unwrap_or(Transform::IDENTITY);
        // Turned as it lies on the side (mitcad#492).
        if let Some(o) = &j.a.frame_override
            && let Ok(turned) = mitcad_model::internal::joints::apply_override(&own, o)
        {
            own = turned;
        }
        let angle = rotate
            .or_else(|| j.angle.and_then(|id| self.parameters().value(id)))
            .unwrap_or(0.0);
        reach_along_x(
            self.part_bounds(info.component).as_ref(),
            &own,
            angle,
            j.flip,
        )
    }

    fn part_ray(&self, info: &PartInfo) -> Result<RayPlan, ModelError> {
        let joint = info
            .record
            .joint
            .ok_or_else(|| refuse(format!("{} has no joint to measure from", info.name)))?;
        let Some(FeatureDef::Joint(j)) = self.feature(joint).map(|e| &e.def) else {
            return Err(refuse(format!(
                "{} has no joint to measure from",
                info.name
            )));
        };
        if !matches!(&j.a.geometry, GeomRef::Mount { name, .. } if name == "end_a" || name == "end_b")
        {
            return Err(refuse(format!(
                "{} is not placed with an end: its length cannot be bound",
                info.name
            )));
        }
        let mut from = j.b.clone();
        // The frame the joint turns and moves (a corner, mitcad#417): the
        // ray starts where it is, along its z, in the frame's axes.
        let mut shift = [0.0; 3];
        let direction = match from.frame_override.take() {
            Some(o) => {
                let local = mitcad_model::internal::joints::origin_frame(
                    &mut self.resolver(),
                    &from.geometry,
                )
                .map_err(refuse)?;
                let turned =
                    mitcad_model::internal::joints::apply_override(&local, &o).map_err(refuse)?;
                if o.shift.is_some() {
                    shift =
                        inverse(&local).apply_vector(sub(turned.translation, local.translation));
                }
                match o.z_axis {
                    Some(_) => RayDirection::Vector(inverse(&local).apply_vector(z_axis(&turned))),
                    None => RayDirection::default(),
                }
            }
            None => RayDirection::default(),
        };
        Ok(RayPlan {
            from,
            shift,
            shift_by_joint: j.kind == JointKind::Slider,
            slide_extra: 0.0,
            direction,
            start: [0.0; 3],
            along: crate::geometry::Z,
        })
    }

    fn check_bind(&self, part: ComponentUid, target: ComponentUid) -> Result<(), ModelError> {
        if part == target {
            return Err(refuse("a part's length cannot be bound to itself"));
        }
        let names = |c: ComponentUid| self.assembly().name(c);
        // Is the target's length bound to the part, through others?
        let mut seen = BTreeSet::new();
        let mut todo = vec![target];
        while let Some(c) = todo.pop() {
            if !seen.insert(c) {
                continue;
            }
            for bound in self
                .part_info(c)
                .map(|i| i.record.bound)
                .unwrap_or_default()
            {
                if let Some(next) = self.bound_target(bound) {
                    if next == part {
                        return Err(refuse(format!(
                            "the lengths would be bound to each other: {} is bound to {}, so {} \
                             cannot be bound to it (a cycle)",
                            names(target),
                            names(part),
                            names(part)
                        )));
                    }
                    todo.push(next);
                }
            }
        }
        let position = |uid: Option<FeatureUid>| uid.and_then(|u| self.state().position(u));
        let info = self.part_info(part);
        let mine = position(info.as_ref().and_then(|i| i.part));
        let theirs = self
            .part_info(target)
            .map(|i| position(i.record.joint).or(position(i.part)));
        if let (Some(mine), Some(Some(theirs))) = (mine, theirs)
            && theirs > mine
        {
            return Err(refuse(format!(
                "{} is placed after {} in the timeline: a length is bound only to parts placed \
                 before it",
                names(target),
                names(part)
            )));
        }
        Ok(())
    }

    fn bound_target(&self, bound: FeatureUid) -> Option<ComponentUid> {
        let entry = self.feature(bound)?;
        let FeatureDef::BoundLength(b) = &entry.def else {
            return None;
        };
        mitcad_model::internal::joints::path_component(
            self.assembly(),
            entry.component,
            &b.to.occurrence.0,
        )
        .ok()
    }
}

/// The bound lengths a part's record lists that still exist.
pub(crate) fn current_bounds(state: &DocState, component: ComponentUid) -> Vec<FeatureUid> {
    state
        .assembly
        .component(component)
        .and_then(|c| c.build.as_ref())
        .map(|r| {
            r.bound
                .iter()
                .copied()
                .filter(|b| state.position(*b).is_some())
                .collect()
        })
        .unwrap_or_default()
}

/// Detaches a bound length: its reference becomes a dimension of the part
/// with the same name and its last value, and the feature goes.
pub(crate) fn detach(
    state: &mut DocState,
    component: ComponentUid,
    bound: FeatureUid,
    part: FeatureUid,
) -> Result<(), ModelError> {
    let Some(FeatureDef::BoundLength(b)) = state.entry(bound).map(|e| e.def.clone()) else {
        return Ok(());
    };
    let owner = state.entry(part).map(|_| part);
    state.parameters.set_owner(b.parameter, owner);
    state.remove_features(&BTreeSet::from([bound]));
    if let Some(record) = state
        .assembly
        .component_mut(component)
        .and_then(|c| c.build.as_mut())
    {
        record.bound.retain(|u| *u != bound);
    }
    Ok(())
}

/// Edits a joint's definition in command form.
pub(crate) fn edit_joint(
    state: &mut DocState,
    joint: FeatureUid,
    edit: impl FnOnce(&mut JointDef<ValueInput>),
) -> Result<(), ModelError> {
    let entry = state
        .entry(joint)
        .ok_or(ModelError::NoSuchFeature(joint))?
        .clone();
    let FeatureDef::Joint(mut def) = as_input(&entry.def, &state.parameters) else {
        return Err(refuse(format!("{} is no joint", entry.name)));
    };
    edit(&mut def);
    state.edit_feature(joint, &FeatureDef::Joint(def))?;
    Ok(())
}

/// Edits a bound length's definition in command form (its parameter kept).
pub(crate) fn edit_bound(
    state: &mut DocState,
    bound: FeatureUid,
    edit: impl FnOnce(&mut BoundLengthDef<ValueInput>),
) -> Result<(), ModelError> {
    let entry = state
        .entry(bound)
        .ok_or(ModelError::NoSuchFeature(bound))?
        .clone();
    let FeatureDef::BoundLength(mut def) = as_input(&entry.def, &state.parameters) else {
        return Err(refuse(format!("{} is no bound length", entry.name)));
    };
    edit(&mut def);
    state.edit_feature(bound, &FeatureDef::BoundLength(def))?;
    Ok(())
}
