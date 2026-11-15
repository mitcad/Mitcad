// SPDX-License-Identifier: MIT
//! Motion links between joints (mitcad#466): a `motion_link` feature
//! couples one joint's free motion (`b`, the follower) to another's (`a`,
//! the driver) in the feature's component, so that the joint solver moves
//! them together (see `crate::motion_links` for what recompute does):
//!
//! - `ratio`: `b = ratio · a + offset`;
//! - `gear`: two gears on turns, `ra` on a's and `rb` on b's (pitch radii,
//!   or any two sizes in their proportion such as tooth counts): b turns
//!   `ra / rb` as far as a, the other way, or the same way for an internal
//!   (ring) gear;
//! - `rack_pinion`: a pinion of `radius` on a turn and a rack on a slide:
//!   the rack travels the radius times the turn (either may be `a`);
//! - `screw`: a turn and a slide (a cylindrical joint's own two, or a
//!   revolute joint and a slider): the slide advances `lead` per turn;
//! - `cam`: a cam turning on a revolute joint (`a`) and its follower on a
//!   slider or a revolute joint (`b`): the follower rides the cam's
//!   profile (a planar face or a sketch profile on the cam's part), a
//!   knife edge, a roller or a flat face.
//!
//! A size is a value (a parameter, an expression or a number) or the
//! radius of a round face, circular edge or sketch circle picked on an
//! occurrence (a joint origin), found again on every recompute. `reverse`
//! turns the follower the other way (a cam's follower rides it from the
//! other side).

use serde::{Deserialize, Serialize};

use super::joint::JointOrigin;
use super::pattern::none;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::{FeatureUid, OccurrenceUid};
use crate::joints::Motion;
use crate::kernel::Kernel;
use crate::parameters::ParamId;

/// A joint's free motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedMotion {
    pub joint: FeatureUid,
    pub motion: Motion,
    /// A joint of a flexible occurrence's component, as that occurrence
    /// moves it (mitcad#1793): the occurrence, placed in the link's
    /// component. None: a joint of the link's own component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrenceUid>,
}

/// A size of a coupling: a value, or the radius of round geometry picked
/// on an occurrence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LinkSize<P> {
    Round(Box<JointOrigin>),
    Value(P),
}

impl<P> LinkSize<P> {
    fn map<Q, E>(
        &self,
        slot: &str,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<LinkSize<Q>, E> {
        Ok(match self {
            Self::Round(o) => LinkSize::Round(o.clone()),
            Self::Value(v) => LinkSize::Value(f(slot, v)?),
        })
    }

    fn origin(&self) -> Option<&JointOrigin> {
        match self {
            Self::Round(o) => Some(o),
            Self::Value(_) => None,
        }
    }

    fn origin_mut(&mut self) -> Option<&mut JointOrigin> {
        match self {
            Self::Round(o) => Some(o),
            Self::Value(_) => None,
        }
    }
}

/// What touches a cam.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Follower<P> {
    /// A point (a knife edge seen along the cam's axis).
    Knife,
    /// A roller of a radius about the follower's point.
    Roller { radius: LinkSize<P> },
    /// A flat face square to a sliding follower's line.
    Flat,
}

fn knife<P>() -> Follower<P> {
    Follower::Knife
}

/// What a link's follower follows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Coupling<P> {
    Ratio {
        ratio: P,
    },
    Gear {
        ra: LinkSize<P>,
        rb: LinkSize<P>,
        #[serde(default, skip_serializing_if = "is_false")]
        internal: bool,
    },
    RackPinion {
        radius: LinkSize<P>,
    },
    Screw {
        lead: LinkSize<P>,
    },
    Cam {
        /// A planar face of the cam (its outer boundary) or curves of a
        /// sketch on the cam's part (the profile they bound), on the side
        /// of joint `a` that turns.
        profile: Box<JointOrigin>,
        #[serde(default = "knife")]
        follower: Follower<P>,
        /// The follower's tip (the knife's point or the roller's centre)
        /// on the follower; a turning follower needs it. Without it a
        /// sliding follower's tip is its joint frame's origin.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tip: Option<Box<JointOrigin>>,
    },
}

impl<P> Coupling<P> {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Ratio { .. } => "ratio",
            Self::Gear { .. } => "gear",
            Self::RackPinion { .. } => "rack_pinion",
            Self::Screw { .. } => "screw",
            Self::Cam { .. } => "cam",
        }
    }

    /// The sizes, by their slot names.
    pub fn sizes(&self) -> Vec<(&'static str, &LinkSize<P>)> {
        match self {
            Self::Ratio { .. } => Vec::new(),
            Self::Gear { ra, rb, .. } => vec![("coupling.ra", ra), ("coupling.rb", rb)],
            Self::RackPinion { radius } => vec![("coupling.radius", radius)],
            Self::Screw { lead } => vec![("coupling.lead", lead)],
            Self::Cam { follower, .. } => match follower {
                Follower::Roller { radius } => vec![("coupling.follower.radius", radius)],
                _ => Vec::new(),
            },
        }
    }

    /// Every joint origin it names (round sizes, a cam's profile and tip).
    pub fn origins(&self) -> Vec<&JointOrigin> {
        let mut out: Vec<&JointOrigin> = self
            .sizes()
            .into_iter()
            .filter_map(|(_, s)| s.origin())
            .collect();
        if let Self::Cam { profile, tip, .. } = self {
            out.push(profile);
            out.extend(tip.as_deref());
        }
        out
    }

    pub fn origins_mut(&mut self) -> Vec<&mut JointOrigin> {
        match self {
            Self::Ratio { .. } => Vec::new(),
            Self::Gear { ra, rb, .. } => {
                ra.origin_mut().into_iter().chain(rb.origin_mut()).collect()
            }
            Self::RackPinion { radius } => radius.origin_mut().into_iter().collect(),
            Self::Screw { lead } => lead.origin_mut().into_iter().collect(),
            Self::Cam {
                profile,
                follower,
                tip,
            } => {
                let mut out: Vec<&mut JointOrigin> = vec![&mut **profile];
                if let Follower::Roller { radius } = follower {
                    out.extend(radius.origin_mut());
                }
                out.extend(tip.as_deref_mut());
                out
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionLinkDef<P = ParamId> {
    /// The driver.
    pub a: LinkedMotion,
    /// The follower.
    pub b: LinkedMotion,
    pub coupling: Coupling<P>,
    /// The follower the other way (a cam's: riding it from the other side).
    #[serde(default, skip_serializing_if = "is_false")]
    pub reverse: bool,
    /// Added to the follower's value (mm or an angle, as b's motion):
    /// where it is when the driver is at zero. A cam's follower touches
    /// the profile without it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
}

impl<P> MotionLinkDef<P> {
    pub const TYPE: &'static str = "motion_link";
    pub const BASE_NAME: &'static str = "MotionLink";

    // Written by hand: the coupling maps by kind, the offset's slot by the motion.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<MotionLinkDef<Q>, E> {
        let coupling = match &self.coupling {
            Coupling::Ratio { ratio } => Coupling::Ratio {
                ratio: f("coupling.ratio", ratio)?,
            },
            Coupling::Gear { ra, rb, internal } => Coupling::Gear {
                ra: ra.map("coupling.ra", f)?,
                rb: rb.map("coupling.rb", f)?,
                internal: *internal,
            },
            Coupling::RackPinion { radius } => Coupling::RackPinion {
                radius: radius.map("coupling.radius", f)?,
            },
            Coupling::Screw { lead } => Coupling::Screw {
                lead: lead.map("coupling.lead", f)?,
            },
            Coupling::Cam {
                profile,
                follower,
                tip,
            } => Coupling::Cam {
                profile: profile.clone(),
                follower: match follower {
                    Follower::Knife => Follower::Knife,
                    Follower::Flat => Follower::Flat,
                    Follower::Roller { radius } => Follower::Roller {
                        radius: radius.map("coupling.follower.radius", f)?,
                    },
                },
                tip: tip.clone(),
            },
        };
        let slot = if self.b.motion.is_rotation() {
            "offset_angle"
        } else {
            "offset"
        };
        Ok(MotionLinkDef {
            a: self.a,
            b: self.b,
            coupling,
            reverse: self.reverse,
            offset: self.offset.as_ref().map(|v| f(slot, v)).transpose()?,
        })
    }
}

/// A joint's kind and free motions, by its feature.
fn joint_motions(
    ctx: &CheckContext<'_>,
    m: &LinkedMotion,
) -> Result<(String, Vec<Motion>, bool), String> {
    let entry = ctx.feature(m.joint)?;
    let (kind, motions) = match &entry.def {
        FeatureDef::Joint(j) => (j.kind, j.kind.motions(j.slide_axis)),
        FeatureDef::AsBuiltJoint(j) => (j.kind, j.kind.motions(j.slide_axis)),
        _ => return Err(format!("{} is not a joint", entry.name)),
    };
    if !motions.contains(&m.motion) {
        return Err(format!(
            "{} has no free motion {} (it has {})",
            entry.name,
            m.motion,
            if motions.is_empty() {
                "none".to_owned()
            } else {
                motions
                    .iter()
                    .map(|m| m.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
    }
    let revolute = kind == crate::joints::JointKind::Revolute;
    Ok((entry.name.clone(), motions, revolute))
}

impl FeatureInfo for MotionLinkDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.features.insert(self.a.joint);
        references.features.insert(self.b.joint);
        for origin in self.coupling.origins() {
            origin.add_references(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.a == self.b {
            return Err("a motion link couples two different motions".to_owned());
        }
        let (a_name, _, a_revolute) = joint_motions(ctx, &self.a).map_err(|e| format!("a: {e}"))?;
        let (b_name, b_motions, _) = joint_motions(ctx, &self.b).map_err(|e| format!("b: {e}"))?;
        let (turn_a, turn_b) = (self.a.motion.is_rotation(), self.b.motion.is_rotation());
        match &self.coupling {
            Coupling::Ratio { .. } => {}
            Coupling::Gear { .. } if !(turn_a && turn_b) => {
                return Err(format!(
                    "gears couple two turns ({a_name} {} and {b_name} {})",
                    self.a.motion, self.b.motion
                ));
            }
            Coupling::RackPinion { .. } | Coupling::Screw { .. } if turn_a == turn_b => {
                return Err(format!(
                    "a {} couples a turn and a slide ({a_name} {} and {b_name} {})",
                    if matches!(self.coupling, Coupling::Screw { .. }) {
                        "screw"
                    } else {
                        "rack and pinion"
                    },
                    self.a.motion,
                    self.b.motion
                ));
            }
            Coupling::Cam { follower, .. } => {
                if !a_revolute {
                    return Err(format!(
                        "a cam turns on a revolute joint ({a_name} is not one)"
                    ));
                }
                let sliding = b_motions.len() == 1 && !turn_b;
                let turning = b_motions == [Motion::Rz];
                if !(sliding || turning) {
                    return Err(format!(
                        "a cam's follower slides on a slider or swings on a revolute joint ({b_name} is neither)"
                    ));
                }
                if turning && matches!(follower, Follower::Flat) {
                    return Err("a flat-faced follower slides (on a slider)".to_owned());
                }
            }
            _ => {}
        }
        for (slot, size) in self.coupling.sizes() {
            if let LinkSize::Round(origin) = size {
                origin.check(ctx).map_err(|e| format!("{slot}: {e}"))?;
            }
        }
        if let Coupling::Cam { profile, tip, .. } = &self.coupling {
            use super::geom_ref::{GeomRef, Want};
            match &profile.geometry {
                GeomRef::Face { .. } => profile.geometry.check(ctx, Want::Face),
                GeomRef::SketchCurves { sketch, .. } => ctx.sketch(*sketch).map(|_| ()),
                other => Err(format!(
                    "{other} is no cam profile (a planar face or curves of a sketch)"
                )),
            }
            .map_err(|e| format!("profile: {e}"))?;
            if let Some(tip) = tip {
                tip.check(ctx).map_err(|e| format!("tip: {e}"))?;
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_sizes(self, &mut |id| Ok(value(id)))
    }
}

/// Ratios finite and not zero, sizes positive.
fn check_sizes(
    def: &MotionLinkDef,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
) -> Result<(), String> {
    if let Coupling::Ratio { ratio } = &def.coupling {
        let r = value(*ratio)?;
        if !r.is_finite() || r == 0.0 {
            return Err(format!(
                "the ratio must be a number other than zero, not {r}"
            ));
        }
    }
    for (slot, size) in def.coupling.sizes() {
        if let LinkSize::Value(id) = size {
            let v = value(*id)?;
            if !(v.is_finite() && v > 0.0) {
                let what = slot.rsplit('.').next().unwrap_or(slot);
                return Err(format!("{what} must be greater than zero, not {v}"));
            }
        }
    }
    if let Some(id) = def.offset {
        value(id)?;
    }
    Ok(())
}

impl<K: Kernel> Evaluate<K> for MotionLinkDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        check_sizes(self, &mut |id| ctx.param(id))?;
        Ok(FeatureOutput::default())
    }
}
