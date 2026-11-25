// SPDX-License-Identifier: MIT
//! Rib and web: thin walls grown from sketch curves and joined to the
//! bodies they reach. A rib grows from an open chain of curves in the
//! sketch plane, its thickness across the plane; a web grows from each
//! curve along the sketch normal, its thickness in the plane: lines, arcs,
//! ellipses, splines, conics, and closed curves as rings (mitcad#2364; the
//! kernel offsets them). Either runs a depth, or up to the next faces of
//! the bodies.
//!
//! Instead of sketch curves either takes the curve of an Intersection Curve
//! (mitcad#1417), `{"feature": "F5"}`, when it lies in one plane: that
//! plane is its sketch plane ([`intersection_curve::curve_plane`]).
//!
//! The sketch curves may be a 3D sketch's (mitcad#1550), in the same form:
//! its curves in its sketch plane, as its profiles are; one out of the
//! plane fails the rib or web, naming the sketch
//! ([`SketchCurves::in_plane`]).
//!
//! A rib's Extend Ends (`extend_ends`, mitcad#1746) continues the chain's
//! open ends along their end tangents to the faces of the bodies, so that
//! it fills up to a curved or slanted face; End Offset (`end_offset`) moves
//! where they stop along the tangent. The kernel does it
//! ([`RibSpec`]); an end it could not continue comes back as a note, shown
//! as the rib's warning. The panel turns it on for new ribs; a rib saved
//! without it keeps its ends as drawn. Its Direction ([`RibDirection`])
//! grows it along the sketch's x or y or a picked line instead of at right
//! angles to the chord.
//!
//! A web (mitcad#1968) joins walls whose curves meet end to end in mitred
//! corners and walls meeting at a node by their union there; its Extend
//! Ends continues each free end (one touching no other curve of the web)
//! along its curve to the bodies, or cuts it back to the wall it crosses,
//! as a rib's ends are cut back. Both rebuild their tool at each copy of a
//! pattern ([`Evaluate::placed_tool`]), so that To Next and Extend Ends are
//! solved again where the copy lands.
//!
//! A Draft Angle (`draft_angle`, mitcad#2102) leans both sides of the wall
//! so that it is thicker where it meets the body, for moulded parts: the
//! kernel builds it into the tool ([`RibSpec::draft_angle`]), so To Next,
//! Extend Ends and End Offset are solved on the drafted wall. Absent, the
//! sides are parallel, as ribs and webs made before it.
//!
//! A rib's Direction Normal to Curves (`"normal"`, mitcad#2398, by a depth
//! only) grows it from each point of its curves at right angles to them:
//! its far side is the chain offset by the depth in the sketch plane, as
//! the sketch's Offset offsets it (what vanishes left out, the corners
//! sharp or round by `corners`), computed here ([`normal_far`]) and given
//! to the kernel ([`RibSpec::far`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use mitcad_vec3::{distance, dot, scale, sub, unit};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::geom_ref::{GeomRef, Want};
use super::intersection_curve;
use super::path::{self, SketchCurves};
use super::pattern::none;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{Curve3, Kernel};
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::sketch::geometry::{Curve2, P2};
use crate::sketch::offset;
use crate::sketch::project::{Projected, project};
use crate::sweeps::{PathCurve, RibFarCurve, RibSpec, ThicknessLocation};
use crate::transform::Transform;

/// How far a rib or a web grows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RibExtent<P = ParamId> {
    /// Up to the faces of the bodies it reaches first, which must close it
    /// off.
    ToNext,
    Depth {
        depth: P,
    },
}

/// The way a rib grows in its sketch plane (mitcad#1746): at right angles
/// to its chain's chord (`"perpendicular"`, the default, written as
/// nothing), along the sketch's x or y axis (`"sketch_x"`, `"sketch_y"`),
/// or along a straight line, edge or axis in or parallel to the sketch
/// plane (`{"along": <reference>}`). `flip` turns it. Normal to Curves
/// (`"normal"`, mitcad#2398, by a depth only): from each point of the
/// curves at right angles to them, so that the far side is the chain
/// offset by the depth as the sketch's Offset offsets it ([`normal_far`],
/// its corners [`RibCorners`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RibDirection {
    #[default]
    Perpendicular,
    SketchX,
    SketchY,
    Along(GeomRef),
    Normal,
}

impl RibDirection {
    fn is_perpendicular(&self) -> bool {
        *self == Self::Perpendicular
    }

    /// The unit vector it grows along in model space before `flip`, in
    /// the plane of `frame`; none for Perpendicular to Chord, which the
    /// kernel takes from the chain, and Normal to Curves, which has a far
    /// side instead.
    fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        frame: &SketchFrame,
    ) -> Result<Option<[f64; 3]>, String> {
        let along = match self {
            Self::Perpendicular | Self::Normal => return Ok(None),
            Self::SketchX => frame.x_axis,
            Self::SketchY => frame.y_axis,
            Self::Along(reference) => {
                ctx.axis(reference)
                    .map_err(|e| format!("the rib's direction: {e}"))?
                    .direction
            }
        };
        let along = unit(along).ok_or_else(|| "the rib's direction has no length".to_owned())?;
        let normal = frame.normal();
        // In or parallel to the sketch plane: within 1e-6 rad.
        if dot(along, normal).abs() > 1e-6 {
            return Err(
                "the rib's direction must lie in or parallel to its sketch plane".to_owned(),
            );
        }
        let flat = sub(along, scale(normal, dot(along, normal)));
        Ok(Some(unit(flat).unwrap_or(along)))
    }
}

/// How the far side of a rib grown Normal to Curves (mitcad#2398) joins
/// where the chain turns away from it, as the sketch's Offset joins its
/// copies (its Corners): `"sharp"` (the default, written as nothing), the
/// offsets on either side run on to where they cross; `"round"`, an arc
/// about the chain's corner, its radius the depth. Where the chain turns
/// toward the far side the offsets meet where they cross either way. A
/// chain with an ellipse, an elliptical arc or a spline is joined round, as
/// Offset joins it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RibCorners {
    #[default]
    Sharp,
    Round,
}

fn is_sharp(corners: &RibCorners) -> bool {
    *corners == RibCorners::Sharp
}

/// Why a rib grown Normal to Curves needs a depth.
const NORMAL_NEEDS_DEPTH: &str = "a rib grown Normal to Curves runs a depth: its far side is its \
     curves offset by the depth; set Depth Type to Depth, or choose another Direction for To Next";

/// What a rib or a web grows from: curves of a sketch
/// (`{"sketch": "F6", "curves": ["c1"]}`), or the curve of an Intersection
/// Curve (mitcad#1417), `{"feature": "F5"}`, which must lie in one plane.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum RibCurves {
    Sketch(SketchCurves),
    Feature { feature: FeatureUid },
}

impl<'de> Deserialize<'de> for RibCurves {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = Value::deserialize(deserializer)?;
        match &value {
            Value::Object(map) if map.contains_key("feature") => {
                if map.len() != 1 {
                    return Err(D::Error::custom(
                        "an intersection curve's curves are {\"feature\": \"F5\"} alone",
                    ));
                }
                let feature = map["feature"]
                    .as_str()
                    .ok_or_else(|| D::Error::custom("\"feature\" is a feature's uid like \"F5\""))?
                    .parse::<FeatureUid>()
                    .map_err(D::Error::custom)?;
                Ok(Self::Feature { feature })
            }
            _ => serde_json::from_value(value)
                .map(Self::Sketch)
                .map_err(D::Error::custom),
        }
    }
}

impl From<SketchCurves> for RibCurves {
    fn from(curves: SketchCurves) -> Self {
        Self::Sketch(curves)
    }
}

impl RibCurves {
    fn add_references(&self, references: &mut References) {
        match self {
            Self::Sketch(curves) => curves.add_references(references),
            Self::Feature { feature } => {
                references.features.insert(*feature);
            }
        }
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match self {
            // A 3D sketch's in its sketch plane too (mitcad#1550).
            Self::Sketch(curves) => curves.check_in_plane(ctx),
            Self::Feature { feature } => {
                let entry = ctx.feature(*feature)?;
                // A Helix Curve's too (mitcad#1777), refused at evaluation
                // as a curve off any one plane.
                if entry.def.makes_curve() {
                    Ok(())
                } else {
                    Err(format!(
                        "{} ({feature}) makes no curve: the curves are a sketch's or an \
                         intersection curve's",
                        entry.name
                    ))
                }
            }
        }
    }

    /// The plane the curves lie in and the curves in model space. An
    /// Intersection Curve's pieces are named `c1`, `c2`, ... in order along
    /// it, as a sketch's curves are, so that the faces grown from them are
    /// `side(c1)`, `wall1(c2)` and so on.
    fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<(SketchFrame, Vec<PathCurve>), String> {
        let feature = match self {
            Self::Sketch(curves) => return curves.in_plane(ctx),
            Self::Feature { feature } => *feature,
        };
        let curve = ctx.feature_curve(feature)?;
        // Its sketches' normals, which orient its plane.
        let sketches: Vec<FeatureUid> = match ctx.env.feature(feature).map(|e| &e.def) {
            // (Faces and bodies crossed have none, mitcad#1777.)
            Some(super::FeatureDef::IntersectionCurve(def)) => def.sketches(),
            _ => Vec::new(),
        };
        let mut normals = Vec::with_capacity(sketches.len());
        for sketch in sketches {
            normals.push(ctx.sketch(sketch)?.frame.normal());
        }
        let frame = intersection_curve::curve_plane(&curve, &normals)
            .map_err(|e| format!("{}: {e}", ctx.feature_name(feature)))?;
        let pieces = curve
            .iter()
            .enumerate()
            .map(|(i, piece)| PathCurve {
                name: EntityUid(i as u32 + 1).curve_name(),
                curve: piece.curve.clone(),
            })
            .collect();
        Ok((frame, pieces))
    }
}

macro_rules! thin_feature {
    ($(#[$doc:meta])* $name:ident, $type:literal, $base:literal, $web:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name<P = ParamId> {
            pub curves: RibCurves,
            pub thickness: P,
            #[serde(default, skip_serializing_if = "is_symmetric")]
            pub thickness_location: ThicknessLocation,
            pub extent: RibExtent<P>,
            /// Grows the other way.
            #[serde(default, skip_serializing_if = "is_false")]
            pub flip: bool,
            /// The bodies it reaches and joins; empty for all.
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            pub participants: Vec<BodyUid>,
            /// The open ends continued along their end tangents to the
            /// faces of the bodies (a rib's mitcad#1746, a web's free ends
            /// mitcad#1968); absent: as drawn, as ribs and webs made before
            /// it.
            #[serde(default, skip_serializing_if = "is_false")]
            pub extend_ends: bool,
            /// How much further each continued end goes along its tangent,
            /// into the face it meets (negative: short of it); absent: 0.
            #[serde(default = "none", skip_serializing_if = "Option::is_none")]
            pub end_offset: Option<P>,
            /// The way a rib grows in its sketch plane (mitcad#1746);
            /// absent: at right angles to its chain's chord.
            #[serde(default, skip_serializing_if = "RibDirection::is_perpendicular")]
            pub direction: RibDirection,
            /// Leans the sides so that the wall is thicker where it meets
            /// the body (mitcad#2102), radians; absent: 0, no draft.
            #[serde(default = "none", skip_serializing_if = "Option::is_none")]
            pub draft_angle: Option<P>,
            /// How the far side of a rib grown Normal to Curves joins at
            /// the corners the chain turns away from it at (mitcad#2398);
            /// absent: sharp.
            #[serde(default, skip_serializing_if = "is_sharp")]
            pub corners: RibCorners,
        }

        impl<P> $name<P> {
            pub const TYPE: &'static str = $type;
            pub const BASE_NAME: &'static str = $base;

            // Written by hand: the extent maps by its kind (`extent.depth`).
            pub fn map_params<Q, E>(
                &self,
                f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
            ) -> Result<$name<Q>, E> {
                Ok($name {
                    curves: self.curves.clone(),
                    thickness: f("thickness", &self.thickness)?,
                    thickness_location: self.thickness_location,
                    extent: match &self.extent {
                        RibExtent::ToNext => RibExtent::ToNext,
                        RibExtent::Depth { depth } => RibExtent::Depth {
                            depth: f("extent.depth", depth)?,
                        },
                    },
                    flip: self.flip,
                    participants: self.participants.clone(),
                    extend_ends: self.extend_ends,
                    end_offset: self
                        .end_offset
                        .as_ref()
                        .map(|v| f("end_offset", v))
                        .transpose()?,
                    direction: self.direction.clone(),
                    draft_angle: self
                        .draft_angle
                        .as_ref()
                        .map(|v| f("draft_angle", v))
                        .transpose()?,
                    corners: self.corners,
                })
            }

            fn view(&self) -> Thin<'_, P> {
                Thin {
                    web: $web,
                    curves: &self.curves,
                    thickness: &self.thickness,
                    location: self.thickness_location,
                    extent: &self.extent,
                    flip: self.flip,
                    participants: &self.participants,
                    extend_ends: self.extend_ends,
                    end_offset: self.end_offset.as_ref(),
                    direction: &self.direction,
                    draft_angle: self.draft_angle.as_ref(),
                    corners: self.corners,
                }
            }
        }

        impl FeatureInfo for $name {
            fn references(&self) -> References {
                self.view().references()
            }

            fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
                self.view().check(ctx)
            }

            fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
                self.view().check_values(value)
            }

            fn creates_bodies(&self) -> bool {
                // A rib that reaches no body to join becomes one.
                true
            }

            fn tool_use(&self) -> Option<ToolUse> {
                Some(ToolUse {
                    operation: Operation::Join,
                    participants: self.participants.clone(),
                })
            }
        }

        impl<K: Kernel> Evaluate<K> for $name {
            fn evaluate(
                &self,
                ctx: &mut EvalContext<'_, K>,
            ) -> Result<FeatureOutput<K::Shape>, String> {
                self.view().evaluate(ctx)
            }

            fn placed_tool(
                &self,
                ctx: &mut EvalContext<'_, K>,
                feature: FeatureUid,
                placement: &Transform,
            ) -> Option<Result<K::Shape, String>> {
                self.view().placed_tool(ctx, feature, placement)
            }
        }
    };
}

thin_feature!(
    /// A rib from one open chain of sketch curves.
    RibDef,
    "rib",
    "Rib",
    false
);
thin_feature!(
    /// A web from sketch curves of any kind (mitcad#2364; a closed one a
    /// ring), one wall per curve, joined where the curves meet
    /// (mitcad#1968).
    WebDef,
    "web",
    "Web",
    true
);

fn is_symmetric(location: &ThicknessLocation) -> bool {
    *location == ThicknessLocation::Symmetric
}

/// The fields of a rib or a web.
struct Thin<'a, P> {
    web: bool,
    curves: &'a RibCurves,
    thickness: &'a P,
    location: ThicknessLocation,
    extent: &'a RibExtent<P>,
    flip: bool,
    participants: &'a [BodyUid],
    extend_ends: bool,
    end_offset: Option<&'a P>,
    direction: &'a RibDirection,
    draft_angle: Option<&'a P>,
    corners: RibCorners,
}

impl Thin<'_, ParamId> {
    fn what(&self) -> &'static str {
        if self.web { "web" } else { "rib" }
    }

    fn references(&self) -> References {
        let mut references = References::default();
        self.curves.add_references(&mut references);
        for body in self.participants {
            references.body(*body);
        }
        if let RibDirection::Along(reference) = self.direction {
            reference.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.web && !self.direction.is_perpendicular() {
            return Err("a web grows along the sketch normal: Direction is a rib's".to_owned());
        }
        // Normal to Curves (mitcad#2398): by a depth, its Corners its own.
        if *self.direction == RibDirection::Normal && matches!(self.extent, RibExtent::ToNext) {
            return Err(NORMAL_NEEDS_DEPTH.to_owned());
        }
        if !is_sharp(&self.corners) && *self.direction != RibDirection::Normal {
            return Err(
                "Corners joins the far side of a rib grown Normal to Curves: set its direction \
                 to \"normal\", or leave corners out"
                    .to_owned(),
            );
        }
        if let RibDirection::Along(reference) = self.direction {
            reference
                .check(ctx, Want::Line)
                .map_err(|e| format!("the rib's direction: {e}"))?;
        }
        check_participants(ctx, Operation::Join, self.participants)?;
        self.curves.check(ctx)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |id: ParamId, what: &str| {
            let v = value(id);
            if v > 0.0 && v.is_finite() {
                Ok(())
            } else {
                Err(format!(
                    "the {}'s {what} must be greater than zero, got {v}",
                    self.what()
                ))
            }
        };
        positive(*self.thickness, "thickness")?;
        if let RibExtent::Depth { depth } = self.extent {
            positive(*depth, "depth")?;
        }
        if let Some(offset) = self.end_offset {
            let v = value(*offset);
            if !v.is_finite() {
                return Err(format!(
                    "the {}'s end offset must be a number, got {v}",
                    self.what()
                ));
            }
        }
        if let Some(angle) = self.draft_angle {
            let v = value(*angle);
            // Less than a right angle either way: at 90 degrees the sides
            // would lie along the way the wall grows.
            if !(v.is_finite() && v.abs() < std::f64::consts::FRAC_PI_2) {
                return Err(format!(
                    "the {}'s draft angle must lie between -90 and 90 degrees, got {v} rad",
                    self.what()
                ));
            }
        }
        Ok(())
    }

    fn evaluate<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let values = self.values(ctx)?;
        let participants = participant_bodies(ctx, Operation::Join, self.participants)?;
        let bodies = participants.iter().map(|(_, s)| s.clone()).collect();
        let uid = ctx.uid;
        let tool = self.tool(ctx, uid, &Transform::IDENTITY, &values, bodies)?;
        // An end that could not be continued to the bodies (mitcad#1746).
        ctx.warn_notes(&tool);
        apply_operation(ctx, Operation::Join, participants, tool, self.what())
    }

    /// The tool rebuilt at a copy of a pattern (mitcad#1968): its curves
    /// and sketch plane moved by `placement`, faces named after `feature`,
    /// To Next and Extend Ends solved again against the bodies at the
    /// pattern, so that a copy where the room is narrower or lower is cut
    /// back to the walls and reaches the faces there. A depth without
    /// Extend Ends depends on no body: the pattern moves the tool instead.
    fn placed_tool<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        placement: &Transform,
    ) -> Option<Result<K::Shape, String>> {
        if matches!(self.extent, RibExtent::Depth { .. }) && !self.extend_ends {
            return None;
        }
        let values = match self.values(ctx) {
            Ok(values) => values,
            Err(e) => return Some(Err(e)),
        };
        let bodies = if self.participants.is_empty() {
            match participant_bodies(ctx, Operation::Join, self.participants) {
                Ok(p) => p.into_iter().map(|(_, s)| s).collect(),
                Err(e) => return Some(Err(e)),
            }
        } else {
            // A participant merged into another is not there at the
            // pattern; the body it went into, another participant, stands
            // for it (as an extrusion's, mitcad#406).
            let mut present = Vec::with_capacity(self.participants.len());
            let mut missing = None;
            for uid in self.participants {
                match ctx.body(*uid) {
                    Ok(shape) => present.push(shape),
                    Err(e) => {
                        missing.get_or_insert(e);
                    }
                }
            }
            match missing {
                Some(e) if present.is_empty() => return Some(Err(e)),
                _ => present,
            }
        };
        Some(self.tool(ctx, feature, placement, &values, bodies))
    }

    /// The thickness, the depth (none: To Next) and the end offset, checked.
    fn values<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Values, String> {
        let thickness = ctx.param(*self.thickness)?;
        let depth = match self.extent {
            RibExtent::ToNext => None,
            RibExtent::Depth { depth } => Some(ctx.param(*depth)?),
        };
        let end_offset = match self.end_offset {
            Some(id) => ctx.param(*id)?,
            None => 0.0,
        };
        let draft_angle = match self.draft_angle {
            Some(id) => ctx.param(*id)?,
            None => 0.0,
        };
        self.check_values(&|id| {
            if id == *self.thickness {
                thickness
            } else if self.end_offset == Some(&id) {
                end_offset
            } else if self.draft_angle == Some(&id) {
                draft_angle
            } else {
                depth.unwrap_or(f64::NAN)
            }
        })?;
        Ok(Values {
            thickness,
            depth,
            end_offset,
            draft_angle,
        })
    }

    /// The kernel's tool from the curves moved by the rigid `placement`
    /// (the identity for the feature itself), its faces named after
    /// `feature`, reaching `bodies`.
    fn tool<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        placement: &Transform,
        values: &Values,
        bodies: Vec<K::Shape>,
    ) -> Result<K::Shape, String> {
        let (frame, curves) = self.curves.resolve(ctx)?;
        let direction = self.direction.resolve(ctx, &frame)?;
        let (frame, curves, direction) = if placement.is_identity() {
            (frame, curves, direction)
        } else {
            (
                placement.apply_frame(&frame),
                curves
                    .into_iter()
                    .map(|c| PathCurve {
                        curve: moved_curve(placement, &c.curve),
                        name: c.name,
                    })
                    .collect(),
                direction.map(|d| placement.apply_vector(d)),
            )
        };
        let chains = if self.web {
            // Every kind of curve makes a wall, a closed one a ring
            // (mitcad#2364); only a point is no curve.
            for curve in &curves {
                if matches!(curve.curve, Curve3::Point(_)) {
                    return Err(format!(
                        "a point is not a curve; a web needs curves ({})",
                        curve.name
                    ));
                }
            }
            curves.into_iter().map(|c| vec![c]).collect()
        } else {
            let chain = path::chain(curves)?;
            if path::is_closed(&chain) {
                return Err("a rib needs an open chain of curves".to_owned());
            }
            vec![chain]
        };
        // Normal to Curves (mitcad#2398): the far side, the chain offset by
        // the depth where it lies now (a pattern's copy too).
        let far = match (self.direction, values.depth) {
            (RibDirection::Normal, Some(depth)) if !self.web => normal_far(
                &frame,
                &chains[0],
                depth,
                self.flip,
                self.corners,
                values.draft_angle != 0.0,
            )?,
            (RibDirection::Normal, _) => return Err(NORMAL_NEEDS_DEPTH.to_owned()),
            _ => Vec::new(),
        };
        let spec = RibSpec {
            feature,
            frame,
            web: self.web,
            chains: &chains,
            thickness: values.thickness,
            location: self.location,
            depth: values.depth,
            flip: self.flip,
            bodies,
            extend_ends: self.extend_ends,
            end_offset: values.end_offset,
            direction,
            draft_angle: values.draft_angle,
            far: &far,
        };
        ctx.kernel.rib(&spec).map_err(|e| e.to_string())
    }
}

/// A point of the sketch plane of `frame` in its coordinates.
fn flat_point(frame: &SketchFrame, p: [f64; 3]) -> P2 {
    let d = sub(p, frame.origin);
    [dot(d, frame.x_axis), dot(d, frame.y_axis)]
}

/// The far side of a rib grown Normal to Curves (mitcad#2398): its chain
/// offset by `depth` in the sketch plane of `frame`, to the chain's left
/// looking along it with the sketch normal up (to its right with `flip`),
/// as the sketch's Offset offsets a chain ([`offset::compute`]): what
/// vanishes at the depth is left out, and where the chain turns toward the
/// far side the offsets on either side meet where they cross; where it
/// turns away from it they run on to where they cross (`corners` sharp,
/// for a chain of lines and arcs) or an arc about the corner joins them.
/// The pieces run the way the chain runs, from the far point across from
/// its start to the one across from its end.
///
/// With a draft (`drafted`) each side of the rib is the ruled face between
/// a curve and its offset (and a cone about a round corner), so each curve
/// must keep its offset (none vanishes), each arc's offset must run its
/// whole length (it meets its neighbours smoothly or by a round corner),
/// and the chain is of lines and arcs; anything else fails in words.
fn normal_far(
    frame: &SketchFrame,
    chain: &[PathCurve],
    depth: f64,
    flip: bool,
    corners: RibCorners,
    drafted: bool,
) -> Result<Vec<RibFarCurve>, String> {
    // The curves in the sketch plane, each with whether it runs the way
    // the chain does (an arc is counter-clockwise in the plane).
    let mut sources: Vec<(Curve2, bool)> = Vec::with_capacity(chain.len());
    for piece in chain {
        let flat = match project(std::slice::from_ref(&piece.curve), frame).pop() {
            Some(Projected::Curve(curve)) => curve,
            _ => {
                return Err(format!(
                    "{} cannot be offset in the rib's sketch plane for Normal to Curves",
                    piece.name
                ));
            }
        };
        let (start, _) = path::curve_ends(&piece.curve)
            .ok_or_else(|| "a rib needs an open chain of curves".to_owned())?;
        let start = flat_point(frame, start);
        let (lo, hi) = flat.domain();
        let gap = |t: f64| {
            let p = flat.eval(t)[0];
            (p[0] - start[0]).hypot(p[1] - start[1])
        };
        let forward = gap(lo) <= gap(hi);
        sources.push((flat, forward));
    }
    let name = |i: usize| chain[i].name.clone();
    // Offset's distance is to the right of the chain; the rib grows to its
    // left unless flipped.
    let d = if flip { depth } else { -depth };
    // Sharp corners as Offset makes them: lines and arcs only.
    let exact = sources
        .iter()
        .all(|(c, _)| matches!(c, Curve2::Line { .. } | Curve2::Arc { .. }));
    let computed = if corners == RibCorners::Sharp && exact {
        offset::compute_sharp(&sources, d, false, &name)
    } else {
        offset::compute(&sources, d, false, None, &name)
    }
    .map_err(|e| format!("the rib's curves offset by its depth of {depth} mm: {e}"))?;
    let m = computed.made.len();
    let vanished = || {
        let names: Vec<String> = computed.dropped.iter().map(|i| name(*i)).collect();
        names.join(", ")
    };
    // One open outline from the chain's start to its end.
    let whole = m > 0
        && !computed.closed
        && (0..m).all(|k| computed.next[k] == if k + 1 < m { Some(k + 1) } else { None });
    if !whole {
        return Err(if computed.dropped.is_empty() {
            format!(
                "the rib's curves offset by its depth of {depth} mm fall apart; give the rib a \
                 smaller depth"
            )
        } else {
            format!(
                "the rib's curves offset by its depth of {depth} mm fall apart where {} vanish; \
                 give the rib a smaller depth",
                vanished()
            )
        });
    }
    if drafted {
        if !computed.dropped.is_empty() {
            return Err(format!(
                "with a draft angle, a rib grown Normal to Curves needs every curve's offset: \
                 {} vanish at its depth of {depth} mm; give it a smaller depth or no draft",
                vanished()
            ));
        }
        for spans in &computed.spans {
            let [(i, t0, t1)] = spans.as_slice() else {
                return Err(
                    "with a draft angle, a rib grown Normal to Curves needs one offset for each \
                     curve; give it no draft"
                        .to_owned(),
                );
            };
            match &sources[*i].0 {
                Curve2::Line { .. } => {}
                arc @ Curve2::Arc { .. } => {
                    let (lo, hi) = arc.domain();
                    let tolerance = 1e-9 * (1.0 + hi.abs().max(lo.abs()));
                    if (t0.min(*t1) - lo).abs() > tolerance || (t0.max(*t1) - hi).abs() > tolerance
                    {
                        return Err(format!(
                            "with a draft angle, a rib grown Normal to Curves needs each arc's \
                             offset to run the arc's whole length: the offset of {} is cut or \
                             runs on at a corner; set Corners to Round where it runs on, or \
                             give it no draft",
                            name(*i)
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "with a draft angle, a rib grown Normal to Curves takes lines and arcs: \
                         {} is neither; give it no draft",
                        name(*i)
                    ));
                }
            }
        }
    }
    // A piece in model space running from `from` the way the chain runs.
    let piece = |made: &offset::Made, from: P2, near: usize, corner: bool| {
        let curve = path::curve3(frame, &offset::made_curve(made));
        let curve = match path::curve_ends(&curve) {
            Some((s, e)) => {
                let at = frame.point(from);
                if distance(e, at) < distance(s, at) {
                    path::reversed(curve)
                } else {
                    curve
                }
            }
            None => curve,
        };
        RibFarCurve {
            curve: PathCurve {
                name: chain[near].name.clone(),
                curve,
            },
            near,
            corner,
        }
    };
    let mut far = Vec::with_capacity(m + computed.corners.len());
    for k in 0..m {
        let spans = &computed.spans[k];
        far.push(piece(
            &computed.made[k],
            computed.ends[k].0,
            spans[0].0,
            false,
        ));
        if let Some((_, arc)) = computed.corners.iter().find(|(j, _)| *j == k) {
            let near = spans.last().map_or(spans[0].0, |s| s.0);
            far.push(piece(arc, computed.ends[k].1, near, true));
        }
    }
    Ok(far)
}

/// The values of a rib or a web.
struct Values {
    thickness: f64,
    depth: Option<f64>,
    end_offset: f64,
    draft_angle: f64,
}

/// A curve moved by a rigid motion.
fn moved_curve(t: &Transform, curve: &Curve3) -> Curve3 {
    match curve {
        Curve3::Point(p) => Curve3::Point(t.apply_point(*p)),
        Curve3::Line { start, end } => Curve3::Line {
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => Curve3::Conic {
            center: t.apply_point(*center),
            normal: t.apply_vector(*normal),
            x_axis: t.apply_vector(*x_axis),
            major: *major,
            minor: *minor,
            start: *start,
            end: *end,
            closed: *closed,
        },
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => Curve3::BSpline {
            degree: *degree,
            poles: poles.iter().map(|p| t.apply_point(*p)).collect(),
            weights: weights.clone(),
            knots: knots.clone(),
        },
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => Curve3::Helix {
            center: t.apply_point(*center),
            axis: t.apply_vector(*axis),
            x_axis: t.apply_vector(*x_axis),
            radius: *radius,
            pitch: *pitch,
            start: *start,
            end: *end,
        },
    }
}
