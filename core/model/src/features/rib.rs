// SPDX-License-Identifier: MIT
//! Rib and web: thin walls grown from open sketch curves and joined to the
//! bodies they reach. A rib grows from a chain of curves in the sketch
//! plane, its thickness across the plane; a web grows from each curve along
//! the sketch normal, its thickness in the plane. Either runs a depth, or
//! up to the next faces of the bodies.
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

use serde::{Deserialize, Serialize};
use serde_json::Value;

use mitcad_vec3::{dot, scale, sub, unit};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::geom_ref::{GeomRef, Want};
use super::intersection_curve;
use super::path::{self, SketchCurves};
use super::pattern::none;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse, is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::sweeps::{PathCurve, RibSpec, ThicknessLocation};

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
/// plane (`{"along": <reference>}`). `flip` turns it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RibDirection {
    #[default]
    Perpendicular,
    SketchX,
    SketchY,
    Along(GeomRef),
}

impl RibDirection {
    fn is_perpendicular(&self) -> bool {
        *self == Self::Perpendicular
    }

    /// The unit vector it grows along in model space before `flip`, in
    /// the plane of `frame`; none for Perpendicular to Chord, which the
    /// kernel takes from the chain.
    fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        frame: &SketchFrame,
    ) -> Result<Option<[f64; 3]>, String> {
        let along = match self {
            Self::Perpendicular => return Ok(None),
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
            /// A rib's open ends continued along their end tangents to the
            /// faces of the bodies (mitcad#1746); absent: as drawn, as
            /// ribs made before it.
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
    /// A web from sketch curves (lines and arcs), one wall per curve.
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
        if self.web && (self.extend_ends || self.end_offset.is_some()) {
            return Err(
                "a web's curves end where they are drawn: Extend Ends and End Offset are a rib's"
                    .to_owned(),
            );
        }
        if self.web && !self.direction.is_perpendicular() {
            return Err("a web grows along the sketch normal: Direction is a rib's".to_owned());
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
        Ok(())
    }

    fn evaluate<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let thickness = ctx.param(*self.thickness)?;
        let depth = match self.extent {
            RibExtent::ToNext => None,
            RibExtent::Depth { depth } => Some(ctx.param(*depth)?),
        };
        let end_offset = match self.end_offset {
            Some(id) => ctx.param(*id)?,
            None => 0.0,
        };
        self.check_values(&|id| {
            if id == *self.thickness {
                thickness
            } else if self.end_offset == Some(&id) {
                end_offset
            } else {
                depth.unwrap_or(f64::NAN)
            }
        })?;
        let participants = participant_bodies(ctx, Operation::Join, self.participants)?;
        let (frame, curves) = self.curves.resolve(ctx)?;
        let direction = self.direction.resolve(ctx, &frame)?;
        let chains = if self.web {
            for curve in &curves {
                if path::curve_ends(&curve.curve).is_none() {
                    return Err(format!("{} is closed; a web needs open curves", curve.name));
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
        let spec = RibSpec {
            feature: ctx.uid,
            frame,
            web: self.web,
            chains: &chains,
            thickness,
            location: self.location,
            depth,
            flip: self.flip,
            bodies: participants.iter().map(|(_, s)| s.clone()).collect(),
            extend_ends: self.extend_ends,
            end_offset,
            direction,
        };
        let tool = ctx.kernel.rib(&spec).map_err(|e| e.to_string())?;
        // An end that could not be continued to the bodies (mitcad#1746).
        ctx.warn_notes(&tool);
        apply_operation(ctx, Operation::Join, participants, tool, self.what())
    }
}
