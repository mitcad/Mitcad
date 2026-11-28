// SPDX-License-Identifier: MIT
//! Wood Joint (mitcad#2604; commands.md "Wood Joint"; the Build tab's
//! Joint command): a woodworking joint between two parts, built on
//! Mortise and Tenon's model ([`super::mortise_tenon`]); not an assembly
//! joint ([`super::joint`]):
//! a planar end face of the first part (`body`) meets the second part
//! (`other`); what grows out of the end face, or a loose piece set into
//! both, goes into the other part, which loses it grown by the clearance
//! (default 0.1 mm per side), as a mortise is the tenon grown by it.
//!
//! The end face's axes are Mortise and Tenon's: x along the face's longest
//! straight edge (its length), y across it (its thickness), z its outward
//! normal, into the other part. The kinds:
//!
//! - **Dado** (housing) and **Rabbet**: the whole end face grows `depth`
//!   into the other part, which loses a housing that much deep (a rabbet
//!   is a housing at the other part's end, open there). The end face must
//!   be a rectangle, as a panel's end is.
//! - **Tongue and Groove**: a tongue `thickness` thick along the whole
//!   length grows `depth` out of the end face, its middle `offset_thickness`
//!   from the face's; the other part loses the groove.
//! - **Bridle**: the same tongue, through the other part (to where it ends
//!   along the tongue's middle); the other part loses an open slot.
//! - **Box Joint**: the end face's length divided into `count` fingers of
//!   one width (by default the odd count nearest the length over the
//!   thickness, at least 3); the first part takes the fingers at even
//!   places from the start (with `flip`, at odd places) the whole
//!   thickness through the other part, which loses their slots, wider by
//!   the clearance along the length only (its own fingers are what stays
//!   between them).
//! - **Loose Tenon** and **Dowel**: `count` pieces (default 1; a loose
//!   tenon `width` x `thickness`, a dowel round of `diameter`) set `depth`
//!   into the other part and `body_depth` (default `depth`) back into the
//!   first one; both parts lose their holes, the pieces grown by the
//!   clearance on every side and past their ends, and each piece is a new
//!   body. One piece sits at the face's middle moved by `offset_width` and
//!   `offset_thickness`; several are spaced evenly along the length, the
//!   first one's side `start_offset` from the face's start, the last one's
//!   `end_offset` from its end, as Mortise and Tenon's tenons.
//!
//! Both parts keep their ids; a piece split off a part becomes a body of
//! the feature, as a cut's. Refused with the reason: a field the kind does
//! not use or a size it needs left out, an end face that is not planar (or
//! not a rectangle where the kind takes the whole face), a tongue thicker
//! than the face, pieces past the face's edges or overlapping, a joint
//! that does not reach the other part.
//!
//! Face names: what grows out of the end face, or the piece `k` (from 0,
//! from the start), has the curves `20 k + 1` on (`side(c1)` … `side(c4)`
//! counter-clockwise from the side along the length at -y; a dowel's
//! cylinder `side(c1)`), its hole or housing `20 k + 11` on.

use serde::{Deserialize, Serialize};

use mitcad_vec3::{dot, sub, unit_div};

use super::base::apply_tool;
use super::bearing_sizes::short_number;
use super::face_refs::{add_faces, check_faces, kernel_error};
use super::mapping::map_params;
use super::mortise_tenon::{
    EndFace, SLACK, THROUGH_PAST, curve, extrude, one_shape, rectangle, through_length,
};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{BooleanOp, Curve3, Kernel};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::topo::{FaceName, RegionKey, SegmentKey, TopoName};

/// The default clearance per side, mm.
pub const DEFAULT_CLEARANCE: f64 = 0.1;

/// The most loose tenons or dowels of one joint.
pub const MAX_PIECES: f64 = 100.0;

/// The most fingers of a box joint.
pub const MAX_FINGERS: f64 = 1000.0;

/// How far apart the curve ids of two pieces are.
const CURVE_STRIDE: u32 = 20;

/// How far a corner of the end face may lie off its rectangle, mm.
const RECTANGLE_SLACK: f64 = 1e-4;

/// The joint's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WoodJointKind {
    /// The whole end face set into a housing.
    Dado,
    /// The whole end face set into a housing at the other part's end.
    Rabbet,
    /// A tongue along the end face in a groove.
    TongueAndGroove,
    /// A tongue through an open slot.
    Bridle,
    /// Fingers the whole thickness through, alternating with the other
    /// part's.
    BoxJoint,
    /// Separate rectangular pieces in holes of both parts.
    LooseTenon,
    /// Separate round pieces in holes of both parts.
    Dowel,
}

/// A size field of the definition, as the kinds use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Depth,
    BodyDepth,
    Width,
    Thickness,
    Diameter,
    OffsetWidth,
    OffsetThickness,
    Count,
    StartOffset,
    EndOffset,
    Flip,
}

impl Field {
    /// The definition's key.
    fn key(self) -> &'static str {
        match self {
            Field::Depth => "depth",
            Field::BodyDepth => "body_depth",
            Field::Width => "width",
            Field::Thickness => "thickness",
            Field::Diameter => "diameter",
            Field::OffsetWidth => "offset_width",
            Field::OffsetThickness => "offset_thickness",
            Field::Count => "count",
            Field::StartOffset => "start_offset",
            Field::EndOffset => "end_offset",
            Field::Flip => "flip",
        }
    }

    /// The refusal of a kind that needs the field and has none.
    fn missing(self) -> &'static str {
        match self {
            Field::Depth => "the joint needs a depth",
            Field::Width => "the joint needs a width",
            Field::Thickness => "the joint needs a thickness",
            Field::Diameter => "the joint needs a diameter",
            _ => "the joint needs a size it was not given",
        }
    }
}

impl WoodJointKind {
    /// The fields the kind needs and those it may have besides.
    fn fields(self) -> (&'static [Field], &'static [Field]) {
        use Field::*;
        const SPACED: &[Field] = &[
            BodyDepth,
            OffsetWidth,
            OffsetThickness,
            Count,
            StartOffset,
            EndOffset,
        ];
        match self {
            WoodJointKind::Dado | WoodJointKind::Rabbet => (&[Depth], &[]),
            WoodJointKind::TongueAndGroove => (&[Depth, Thickness], &[OffsetThickness]),
            WoodJointKind::Bridle => (&[Thickness], &[OffsetThickness]),
            WoodJointKind::BoxJoint => (&[], &[Count, Flip]),
            WoodJointKind::LooseTenon => (&[Depth, Width, Thickness], SPACED),
            WoodJointKind::Dowel => (&[Depth, Diameter], SPACED),
        }
    }

    /// Whether the kind takes the end face's whole length (and so needs a
    /// rectangular face).
    fn whole_length(self) -> bool {
        matches!(
            self,
            WoodJointKind::Dado
                | WoodJointKind::Rabbet
                | WoodJointKind::TongueAndGroove
                | WoodJointKind::Bridle
                | WoodJointKind::BoxJoint
        )
    }
}

/// A joint between `body`, whose end face `face` meets `other`, and
/// `other` (see the module's documentation). Lengths in millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct WoodJointDef<P = ParamId> {
    pub kind: WoodJointKind,
    /// The part whose end face meets the other.
    pub body: BodyUid,
    /// Its planar end face.
    pub face: FaceName,
    /// The part the joint is cut into.
    pub other: BodyUid,
    /// Into the other part.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub depth: Option<P>,
    /// Of loose pieces: back into the first part; default `depth`.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub body_depth: Option<P>,
    /// A loose tenon's width, along the end face's length.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub width: Option<P>,
    /// A tongue's or a loose tenon's thickness, across the end face.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thickness: Option<P>,
    /// A dowel's diameter.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub diameter: Option<P>,
    /// One piece's middle from the face's along its length; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub offset_width: Option<P>,
    /// The middle from the face's across it; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub offset_thickness: Option<P>,
    /// Loose pieces (default 1) or a box joint's fingers (both parts').
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub count: Option<P>,
    /// Of several pieces: the first one's side from the face's start.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<P>,
    /// The last one's side from the face's end.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<P>,
    /// A box joint's end fingers are the other part's.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// Per side; default 0.1 mm.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<P>,
}

impl<P> WoodJointDef<P> {
    pub const TYPE: &'static str = "wood_joint";
    pub const BASE_NAME: &'static str = "WoodJoint";

    map_params! {
        WoodJointDef {
            kind: copy,
            body: copy,
            face: clone,
            other: copy,
            depth: option,
            body_depth: option,
            width: option,
            thickness: option,
            diameter: option,
            offset_width: option,
            offset_thickness: option,
            count: option,
            start_offset: option,
            end_offset: option,
            flip: copy,
            clearance: option,
        }
    }

    /// Whether the definition has the field.
    fn has(&self, field: Field) -> bool {
        match field {
            Field::Depth => self.depth.is_some(),
            Field::BodyDepth => self.body_depth.is_some(),
            Field::Width => self.width.is_some(),
            Field::Thickness => self.thickness.is_some(),
            Field::Diameter => self.diameter.is_some(),
            Field::OffsetWidth => self.offset_width.is_some(),
            Field::OffsetThickness => self.offset_thickness.is_some(),
            Field::Count => self.count.is_some(),
            Field::StartOffset => self.start_offset.is_some(),
            Field::EndOffset => self.end_offset.is_some(),
            Field::Flip => self.flip,
        }
    }

    /// Refuses a size the kind needs left out and one it does not use.
    fn check_fields(&self) -> Result<(), String> {
        let (needed, optional) = self.kind.fields();
        if let Some(field) = needed.iter().find(|f| !self.has(**f)) {
            return Err(field.missing().to_owned());
        }
        const ALL: [Field; 11] = [
            Field::Depth,
            Field::BodyDepth,
            Field::Width,
            Field::Thickness,
            Field::Diameter,
            Field::OffsetWidth,
            Field::OffsetThickness,
            Field::Count,
            Field::StartOffset,
            Field::EndOffset,
            Field::Flip,
        ];
        if let Some(field) = ALL
            .iter()
            .find(|f| self.has(**f) && !needed.contains(f) && !optional.contains(f))
        {
            return Err(format!(
                "this kind of joint does not use the field {}",
                field.key()
            ));
        }
        Ok(())
    }
}

/// The definition's values, mm (defaults filled in).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Values {
    kind: WoodJointKind,
    depth: f64,
    body_depth: f64,
    width: f64,
    thickness: f64,
    diameter: f64,
    offset: [f64; 2],
    /// None: the kind's default.
    count: Option<f64>,
    ends: [f64; 2],
    flip: bool,
    clearance: f64,
}

impl Values {
    fn new(
        def: &WoodJointDef,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    ) -> Result<Self, String> {
        let mut optional = |id: Option<ParamId>| -> Result<Option<f64>, String> {
            id.map(&mut *value).transpose()
        };
        let depth = optional(def.depth)?;
        let values = Values {
            kind: def.kind,
            depth: depth.unwrap_or(0.0),
            body_depth: optional(def.body_depth)?.or(depth).unwrap_or(0.0),
            width: optional(def.width)?.unwrap_or(0.0),
            thickness: optional(def.thickness)?.unwrap_or(0.0),
            diameter: optional(def.diameter)?.unwrap_or(0.0),
            offset: [
                optional(def.offset_width)?.unwrap_or(0.0),
                optional(def.offset_thickness)?.unwrap_or(0.0),
            ],
            count: optional(def.count)?,
            ends: [
                optional(def.start_offset)?.unwrap_or(0.0),
                optional(def.end_offset)?.unwrap_or(0.0),
            ],
            flip: def.flip,
            clearance: optional(def.clearance)?.unwrap_or(DEFAULT_CLEARANCE),
        };
        values.check(def)?;
        Ok(values)
    }

    fn check(&self, def: &WoodJointDef) -> Result<(), String> {
        let positive = |what: &str, v: f64| -> Result<(), String> {
            if v > 0.0 && v.is_finite() {
                Ok(())
            } else {
                Err(format!("the {what} must be greater than zero, got {v}"))
            }
        };
        if def.depth.is_some() {
            positive("joint's depth", self.depth)?;
        }
        if def.body_depth.is_some() {
            positive("joint's depth into the end face's part", self.body_depth)?;
        }
        if def.width.is_some() {
            positive("joint's width", self.width)?;
        }
        if def.thickness.is_some() {
            positive("joint's thickness", self.thickness)?;
        }
        if def.diameter.is_some() {
            positive("dowel's diameter", self.diameter)?;
        }
        for offset in self.offset {
            if !offset.is_finite() {
                return Err(format!("the joint's offset must be a length, got {offset}"));
            }
        }
        for (what, offset) in ["start", "end"].into_iter().zip(self.ends) {
            if offset < 0.0 || !offset.is_finite() {
                return Err(format!(
                    "the {what} offset must not be negative, got {offset}"
                ));
            }
        }
        if self.clearance < 0.0 || !self.clearance.is_finite() {
            return Err(format!(
                "the clearance must not be negative, got {}",
                self.clearance
            ));
        }
        if let Some(count) = self.count {
            let (min, max) = if self.kind == WoodJointKind::BoxJoint {
                (2.0, MAX_FINGERS)
            } else {
                (1.0, MAX_PIECES)
            };
            if !count.is_finite()
                || (count - count.round()).abs() > 1e-9
                || !(min..=max).contains(&count.round())
            {
                return Err(mitcad_model_base::internal::parameters::count_error(
                    format!(
                        "the joint's count must be a whole number from {min} to {max}, got {count}"
                    ),
                    count,
                ));
            }
        }
        Ok(())
    }

    fn count(&self) -> Option<usize> {
        self.count.map(|c| c.round() as usize)
    }
}

/// What the joint puts where, in the end face's frame.
#[derive(Debug, Clone, PartialEq)]
struct Plan {
    /// The middles of the pieces (what grows out of the face, or the loose
    /// pieces), from the face's origin along its x and y.
    centres: Vec<[f64; 2]>,
    /// A piece's size along x and y; a round piece's both its diameter.
    size: [f64; 2],
    round: bool,
    /// How much larger the holes are than the pieces on each side along x
    /// and y, and past their ends.
    grow: [f64; 2],
    tip: f64,
    /// Into the other part; None: through it.
    depth: Option<f64>,
    /// Loose pieces: how far back into the first part.
    back: Option<f64>,
}

impl Plan {
    fn new(values: &Values, face: &EndFace, rectangular: bool) -> Result<Self, String> {
        let kind = values.kind;
        if kind.whole_length() && !rectangular {
            return Err("this kind of joint needs a rectangular end face".to_owned());
        }
        let length = face.max[0] - face.min[0];
        let across = face.max[1] - face.min[1];
        let middle = [
            (face.min[0] + face.max[0]) / 2.0,
            (face.min[1] + face.max[1]) / 2.0,
        ];
        let c = values.clearance;
        let tongue = |thickness: f64| -> Result<Vec<[f64; 2]>, String> {
            let centre = [middle[0], middle[1] + values.offset[1]];
            if thickness > across + SLACK {
                return Err(format!(
                    "the tongue ({} mm) is thicker than the end face ({} mm)",
                    short_number(thickness),
                    short_number(across)
                ));
            }
            check_inside(face, &[centre], [length, thickness])?;
            Ok(vec![centre])
        };
        let plan = |centres, size, grow, depth, back| Plan {
            centres,
            size,
            round: kind == WoodJointKind::Dowel,
            grow,
            tip: c,
            depth,
            back,
        };
        Ok(match kind {
            WoodJointKind::Dado | WoodJointKind::Rabbet => plan(
                vec![middle],
                [length, across],
                [c, c],
                Some(values.depth),
                None,
            ),
            WoodJointKind::TongueAndGroove => plan(
                tongue(values.thickness)?,
                [length, values.thickness],
                [c, c],
                Some(values.depth),
                None,
            ),
            WoodJointKind::Bridle => plan(
                tongue(values.thickness)?,
                [length, values.thickness],
                [c, c],
                None,
                None,
            ),
            WoodJointKind::BoxJoint => {
                let count = match values.count() {
                    Some(count) => count,
                    None => default_fingers(length, across),
                };
                let width = length / count as f64;
                if width <= 2.0 * c + SLACK {
                    return Err(format!(
                        "the box joint's fingers ({} mm) must be wider than twice the \
                         clearance",
                        short_number(width)
                    ));
                }
                let centres = (0..count)
                    .filter(|i| (i % 2 == 0) != values.flip)
                    .map(|i| [face.min[0] + (i as f64 + 0.5) * width, middle[1]])
                    .collect();
                plan(centres, [width, across], [c, 0.0], None, None)
            }
            WoodJointKind::LooseTenon | WoodJointKind::Dowel => {
                let size = if kind == WoodJointKind::Dowel {
                    [values.diameter, values.diameter]
                } else {
                    [values.width, values.thickness]
                };
                let centres = spaced(values, face, middle, size)?;
                plan(
                    centres,
                    size,
                    [c, c],
                    Some(values.depth),
                    Some(values.body_depth),
                )
            }
        })
    }
}

/// The odd count of a box joint's fingers nearest to the length over the
/// thickness (the larger on a tie), at least 3.
fn default_fingers(length: f64, thickness: f64) -> usize {
    if thickness <= 0.0 || !thickness.is_finite() {
        return 3;
    }
    let ratio = length / thickness;
    let k = ((ratio - 1.0) / 2.0 + 0.5).floor().max(1.0);
    2 * k as usize + 1
}

/// The middles of loose pieces of `size`: one at the face's middle moved
/// by the offsets, or several spaced along the face's length between the
/// start and end offsets.
fn spaced(
    values: &Values,
    face: &EndFace,
    middle: [f64; 2],
    size: [f64; 2],
) -> Result<Vec<[f64; 2]>, String> {
    let n = values.count().unwrap_or(1);
    let v = middle[1] + values.offset[1];
    let centres = if n <= 1 {
        vec![[middle[0] + values.offset[0], v]]
    } else {
        let a = size[0];
        let [start, end] = values.ends;
        let first = face.min[0] + start + a / 2.0;
        let last = face.max[0] - end - a / 2.0;
        if last < first - SLACK {
            return Err(format!(
                "the start and end offsets ({} and {} mm) leave less than a piece's {} mm of \
                 the end face ({} mm)",
                short_number(start),
                short_number(end),
                short_number(a),
                short_number(face.max[0] - face.min[0])
            ));
        }
        let pitch = (last - first).max(0.0) / (n - 1) as f64;
        let hole = a + 2.0 * values.clearance;
        if pitch < hole + SLACK {
            return Err(format!(
                "the holes of the {n} pieces overlap: {} mm along the face each with the \
                 clearance, their middles {} mm apart",
                short_number(hole),
                short_number(pitch)
            ));
        }
        (0..n).map(|k| [first + k as f64 * pitch, v]).collect()
    };
    if size[1] > face.max[1] - face.min[1] + SLACK {
        return Err(format!(
            "the joint's pieces ({} mm) are thicker than the end face ({} mm)",
            short_number(size[1]),
            short_number(face.max[1] - face.min[1])
        ));
    }
    check_inside(face, &centres, size)?;
    Ok(centres)
}

/// Refuses pieces of `size` at `centres` that reach past the end face.
fn check_inside(face: &EndFace, centres: &[[f64; 2]], size: [f64; 2]) -> Result<(), String> {
    for centre in centres {
        for i in 0..2 {
            let low = centre[i] - size[i] / 2.0 - face.min[i];
            let high = face.max[i] - centre[i] - size[i] / 2.0;
            if low < -SLACK || high < -SLACK {
                return Err(format!(
                    "the joint's offset takes it past the end face's edge (by {} mm)",
                    short_number(-low.min(high))
                ));
            }
        }
    }
    Ok(())
}

/// Whether the end face is the rectangle of its extent: only straight
/// edges, each on the rectangle's border.
fn rectangular(face: &EndFace, curves: &[Curve3]) -> bool {
    let edge_on_border = |start: [f64; 3], end: [f64; 3]| {
        let d = [sub(start, face.origin), sub(end, face.origin)];
        let at = |p: [f64; 3], i: usize| dot(p, if i == 0 { face.x } else { face.y });
        // Both ends on one side of the rectangle.
        (0..2).any(|i| {
            [face.min[i], face.max[i]].into_iter().any(|side| {
                (at(d[0], i) - side).abs() <= RECTANGLE_SLACK
                    && (at(d[1], i) - side).abs() <= RECTANGLE_SLACK
            })
        })
    };
    !curves.is_empty()
        && curves.iter().all(|curve| match curve {
            Curve3::Line { start, end } => edge_on_border(*start, *end),
            _ => false,
        })
}

/// A circle of `diameter` at `centre`, the curve `c<first>`.
fn circle(first: u32, diameter: f64, centre: [f64; 2]) -> ProfileRegion {
    let segment = ProfileSegment {
        key: SegmentKey::closed(curve(first)),
        geometry: SegmentGeometry::Circle {
            center: centre,
            radius: diameter / 2.0,
            start: 0.0,
        },
    };
    let key = RegionKey::new([segment.key.clone()]).expect("a loop has segments");
    ProfileRegion {
        key,
        loops: vec![ProfileLoop {
            segments: vec![segment],
        }],
    }
}

impl Plan {
    /// Piece `k`'s profile grown by `grow` along x and y, its curves from
    /// `c<first>` on.
    fn region(&self, k: usize, first: u32, grow: [f64; 2]) -> ProfileRegion {
        let first = first + CURVE_STRIDE * k as u32;
        let centre = self.centres[k];
        if self.round {
            circle(first, self.size[0] + 2.0 * grow[0], centre)
        } else {
            rectangle(
                first,
                self.size[0] + 2.0 * grow[0],
                self.size[1] + 2.0 * grow[1],
                0.0,
                centre,
            )
        }
    }
}

impl FeatureInfo for WoodJointDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references.body(self.other);
        add_faces(&mut references, std::iter::once(&self.face));
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, std::slice::from_ref(&self.face), "end face")?;
        ctx.body(self.other)?;
        if self.other == self.body {
            return Err("the joint needs two different parts".to_owned());
        }
        self.check_fields()
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        Values::new(self, &mut |id| Ok(value(id))).map(|_| ())
    }

    fn creates_bodies(&self) -> bool {
        // Loose pieces, and pieces of a part that the joint splits.
        true
    }
}

impl<K: Kernel> Evaluate<K> for WoodJointDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let values = Values::new(self, &mut |id| ctx.param(id))?;
        let first = ctx.surface_body(self.body)?;
        let other = ctx.surface_body(self.other)?;
        let face = ctx.face_on(self.body, &first, &self.face)?;
        let plane = ctx.kernel.face_plane(&first, &face).map_err(|_| {
            crate::notice::placed(
                "the joint's end face must be planar",
                crate::notice::NoticePlace::face(face.to_string()).in_body(&self.body.to_string()),
                format!("the joint's end face {face} must be planar"),
            )
        })?;
        let z = unit_div(plane.normal).ok_or("the joint's end face has no normal")?;
        let curves = ctx
            .kernel
            .curves_of(&first, &TopoName::Face(face.clone()))
            .map_err(kernel_error)?;
        let end = EndFace::new(plane.origin, z, &curves, false)?;
        let plan = Plan::new(&values, &end, rectangular(&end, &curves))?;
        let frame = SketchFrame {
            origin: end.origin,
            x_axis: end.x,
            y_axis: end.y,
        };
        let not_reaching = || "the joint does not reach the other part".to_owned();
        let mut lengths = Vec::with_capacity(plan.centres.len());
        for centre in &plan.centres {
            lengths.push(match plan.depth {
                Some(depth) => depth,
                None => through_length(ctx.kernel, &other, frame.point(*centre), z)
                    .map_err(kernel_error)?
                    .ok_or_else(not_reaching)?,
            });
        }
        // Pieces one after another of one length are extruded together.
        let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
        for (k, &length) in lengths.iter().enumerate() {
            let same = groups
                .last()
                .is_some_and(|(first, _)| (first - length).abs() <= SLACK);
            if !same {
                groups.push((length, Vec::new()));
            }
            if let Some((_, members)) = groups.last_mut() {
                members.push(k);
            }
        }
        // The holes first (a joint that reaches nothing is refused as
        // such): from the back of loose pieces' holes, or the end face, to
        // past the pieces' ends.
        let back = plan.back.map(|back| back + plan.tip).unwrap_or(0.0);
        let mut holes = Vec::with_capacity(groups.len());
        for (length, members) in &groups {
            let mut distance = back + length + plan.tip;
            if plan.depth.is_none() {
                distance += THROUGH_PAST;
            }
            let regions: Vec<ProfileRegion> = members
                .iter()
                .map(|k| plan.region(*k, 11, plan.grow))
                .collect();
            holes.push(extrude(ctx, frame, &regions, z, -back, distance)?);
        }
        let holes = one_shape(ctx.kernel, holes)?;
        // (apply_tool's words for a tool that touches none of its bodies.)
        let missed = "the joint does not cut into any participant body";
        let mut changes = apply_tool(ctx, BooleanOp::Cut, &[(self.other, other)], &holes, "joint")
            .map_err(|e| if e == missed { not_reaching() } else { e })?;
        if let Some(back) = plan.back {
            changes.extend(
                apply_tool(ctx, BooleanOp::Cut, &[(self.body, first)], &holes, "joint").map_err(
                    |e| {
                        if e == missed {
                            "the joint's holes do not go into the end face's part".to_owned()
                        } else {
                            e
                        }
                    },
                )?,
            );
            // Each loose piece is a body of its own.
            for (k, length) in lengths.iter().enumerate() {
                let region = plan.region(k, 1, [0.0, 0.0]);
                let piece = extrude(ctx, frame, &[region], z, -back, back + length)?;
                changes.push(BodyChange::Set(ctx.new_body(), piece));
            }
        } else {
            // What grows out of the end face starts a little inside its
            // part, so that it overlaps the part instead of touching it.
            let mut grown = Vec::with_capacity(groups.len());
            for (length, members) in &groups {
                let delta = (length * 0.05).clamp(1e-3, 0.5);
                let regions: Vec<ProfileRegion> = members
                    .iter()
                    .map(|k| plan.region(*k, 1, [0.0, 0.0]))
                    .collect();
                grown.push(extrude(ctx, frame, &regions, z, -delta, length + delta)?);
            }
            let grown = one_shape(ctx.kernel, grown)?;
            changes.extend(apply_tool(
                ctx,
                BooleanOp::Join,
                &[(self.body, first)],
                &grown,
                "joint",
            )?);
        }
        Ok(FeatureOutput {
            changes,
            tool: Some(holes),
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start: [f64; 3], end: [f64; 3]) -> Curve3 {
        Curve3::Line { start, end }
    }

    /// A 20 x 60 end face at x = 0 (y 0 to 20, z 0 to 60), facing +X: its
    /// length along z, its thickness along -y.
    fn face_curves() -> Vec<Curve3> {
        vec![
            line([0.0, 0.0, 0.0], [0.0, 20.0, 0.0]),
            line([0.0, 20.0, 0.0], [0.0, 20.0, 60.0]),
            line([0.0, 20.0, 60.0], [0.0, 0.0, 60.0]),
            line([0.0, 0.0, 60.0], [0.0, 0.0, 0.0]),
        ]
    }

    fn end_face(curves: &[Curve3]) -> EndFace {
        EndFace::new([0.0, 10.0, 30.0], [1.0, 0.0, 0.0], curves, false).unwrap()
    }

    fn values(kind: WoodJointKind) -> Values {
        Values {
            kind,
            depth: 6.0,
            body_depth: 6.0,
            width: 0.0,
            thickness: 0.0,
            diameter: 0.0,
            offset: [0.0, 0.0],
            count: None,
            ends: [0.0, 0.0],
            flip: false,
            clearance: 0.1,
        }
    }

    fn plan(values: &Values) -> Result<Plan, String> {
        let curves = face_curves();
        let face = end_face(&curves);
        Plan::new(values, &face, rectangular(&face, &curves))
    }

    #[test]
    fn a_dado_takes_the_whole_face_and_needs_a_rectangle() {
        let p = plan(&values(WoodJointKind::Dado)).unwrap();
        assert_eq!(p.centres, vec![[0.0, 0.0]]);
        assert_eq!(p.size, [60.0, 20.0]);
        assert_eq!((p.depth, p.back), (Some(6.0), None));
        // A face with a notch: a corner off the rectangle.
        let mut curves = face_curves();
        curves[1] = line([0.0, 20.0, 0.0], [0.0, 15.0, 60.0]);
        let face = end_face(&curves);
        assert!(!rectangular(&face, &curves));
        assert_eq!(
            Plan::new(&values(WoodJointKind::Dado), &face, false).unwrap_err(),
            "this kind of joint needs a rectangular end face"
        );
    }

    #[test]
    fn a_tongue_runs_the_whole_length_and_fits_the_thickness() {
        let mut v = values(WoodJointKind::TongueAndGroove);
        v.thickness = 8.0;
        v.offset = [0.0, 3.0];
        let p = plan(&v).unwrap();
        assert_eq!(p.centres, vec![[0.0, 3.0]]);
        assert_eq!(p.size, [60.0, 8.0]);
        v.thickness = 24.0;
        assert_eq!(
            plan(&v).unwrap_err(),
            "the tongue (24 mm) is thicker than the end face (20 mm)"
        );
        v.thickness = 8.0;
        v.offset = [0.0, 7.0];
        assert_eq!(
            plan(&v).unwrap_err(),
            "the joint's offset takes it past the end face's edge (by 1 mm)"
        );
        let mut bridle = values(WoodJointKind::Bridle);
        bridle.thickness = 8.0;
        assert_eq!(plan(&bridle).unwrap().depth, None);
    }

    #[test]
    fn a_box_joint_alternates_fingers_from_the_start() {
        // 60 long over 20 thick: 3 fingers of 20, the first part's at the
        // ends.
        let p = plan(&values(WoodJointKind::BoxJoint)).unwrap();
        assert_eq!(p.centres, vec![[-20.0, 0.0], [20.0, 0.0]]);
        assert_eq!((p.size, p.grow), ([20.0, 20.0], [0.1, 0.0]));
        assert_eq!(p.depth, None);
        let mut v = values(WoodJointKind::BoxJoint);
        v.count = Some(6.0);
        v.flip = true;
        let p = plan(&v).unwrap();
        assert_eq!(p.centres, vec![[-15.0, 0.0], [5.0, 0.0], [25.0, 0.0]]);
        v.count = Some(300.0);
        assert_eq!(
            plan(&v).unwrap_err(),
            "the box joint's fingers (0.2 mm) must be wider than twice the clearance"
        );
        assert_eq!(default_fingers(60.0, 20.0), 3);
        assert_eq!(default_fingers(300.0, 18.0), 17);
        assert_eq!(default_fingers(40.0, 20.0), 3);
        assert_eq!(default_fingers(100.0, 20.0), 5);
    }

    #[test]
    fn loose_pieces_are_spaced_or_offset() {
        let mut v = values(WoodJointKind::Dowel);
        v.diameter = 8.0;
        v.count = Some(3.0);
        v.ends = [6.0, 6.0];
        let p = plan(&v).unwrap();
        // From z = 6 to 54: middles at z = 10, 30 and 50.
        assert_eq!(p.centres, vec![[-20.0, 0.0], [0.0, 0.0], [20.0, 0.0]]);
        assert!(p.round);
        assert_eq!(p.back, Some(6.0));
        v.count = Some(8.0);
        assert_eq!(
            plan(&v).unwrap_err(),
            "the holes of the 8 pieces overlap: 8.2 mm along the face each with the \
             clearance, their middles 5.714 mm apart"
        );
        let mut tenon = values(WoodJointKind::LooseTenon);
        tenon.width = 30.0;
        tenon.thickness = 8.0;
        tenon.offset = [10.0, -2.0];
        let p = plan(&tenon).unwrap();
        assert_eq!(p.centres, vec![[10.0, -2.0]]);
        assert!(!p.round);
        tenon.offset = [20.0, 0.0];
        assert_eq!(
            plan(&tenon).unwrap_err(),
            "the joint's offset takes it past the end face's edge (by 5 mm)"
        );
    }

    #[test]
    fn fields_are_checked_against_the_kind() {
        let def = |kind: &str| -> WoodJointDef<f64> {
            serde_json::from_value(serde_json::json!({
                "kind": kind, "body": "F1.b0", "face": "F1:side1", "other": "F2.b0",
                "depth": 6
            }))
            .unwrap()
        };
        assert!(def("dado").check_fields().is_ok());
        assert_eq!(
            def("tongue_and_groove").check_fields().unwrap_err(),
            "the joint needs a thickness"
        );
        assert_eq!(
            def("box_joint").check_fields().unwrap_err(),
            "this kind of joint does not use the field depth"
        );
        let mut dowel = def("dowel");
        assert_eq!(
            dowel.check_fields().unwrap_err(),
            "the joint needs a diameter"
        );
        dowel.diameter = Some(8.0);
        dowel.flip = true;
        assert_eq!(
            dowel.check_fields().unwrap_err(),
            "this kind of joint does not use the field flip"
        );
    }
}
