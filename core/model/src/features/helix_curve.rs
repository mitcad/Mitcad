// SPDX-License-Identifier: MIT
//! Helix Curve (mitcad#1777): a helical 3D curve about an axis, a path for
//! the features after it as an Intersection Curve's curve is
//! ([`FeatureOutput::curve`], `{"feature": "F5"}`; its pieces named
//! `F5:curve1`, ...). The axis is an origin or construction axis, a straight
//! or circular edge, a sketch line or a cylindrical or conical face; on such
//! a face the curve starts where the face starts along the axis (the way
//! it rises), and without a radius of its own takes the face's, so that it
//! lies on the face; on a cone it then also moves out or in as the cone
//! widens or narrows. A growth makes it a conical helix about any axis.
//!
//! A helix that keeps its radius is exact ([`Curve3::Helix`]); one that
//! grows comes from the geometry kernel ([`Kernel::helix_curve`]), a curve
//! on the cone its turns lie on.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Resolver, Want};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::datum::{SurfaceGeometry, Vec3};
use crate::kernel::{Curve3, HelixCurveSpec, Kernel};
use crate::parameters::ParamId;
use crate::sweeps::PathCurve;
use crate::topo::TopoName;
use mitcad_vec3::{add, cross, dot, scale, sub, unit};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelixCurveDef<P = ParamId> {
    /// What it turns about: an origin or construction axis, a straight or
    /// circular edge, a sketch line, circle or arc, a cylindrical or conical
    /// face (its axis), or a fixed axis.
    pub axis: GeomRef,
    /// The distance from the axis where it starts; none: the picked face's
    /// there (a cylindrical or conical face).
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub radius: Option<P>,
    /// The rise per turn along the axis.
    pub pitch: P,
    /// The turns (need not be whole).
    pub revolutions: P,
    /// Turns left-handed about the way it rises instead of right-handed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub left_handed: bool,
    /// Rises against the axis direction.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// Where it starts round the axis (an angle, right-handed about the way
    /// it rises), from the model X axis seen along the axis (Y where the
    /// axis lies along X); none: 0.
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub start_angle: Option<P>,
    /// How far it moves out from the axis per turn (a conical helix);
    /// negative narrows it. None: 0, or on a conical face whose radius it
    /// takes, the cone's.
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub growth: Option<P>,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> HelixCurveDef<P> {
    pub const TYPE: &'static str = "helix_curve";
    pub const BASE_NAME: &'static str = "HelixCurve";

    // Written by hand: the optional values map when they are there.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<HelixCurveDef<Q>, E> {
        Ok(HelixCurveDef {
            axis: self.axis.clone(),
            radius: self.radius.as_ref().map(|r| f("radius", r)).transpose()?,
            pitch: f("pitch", &self.pitch)?,
            revolutions: f("revolutions", &self.revolutions)?,
            left_handed: self.left_handed,
            flip: self.flip,
            start_angle: self
                .start_angle
                .as_ref()
                .map(|a| f("start_angle", a))
                .transpose()?,
            growth: self.growth.as_ref().map(|g| f("growth", g)).transpose()?,
            values: PhantomData,
        })
    }
}

fn positive(value: f64, what: &str) -> Result<f64, String> {
    if value > 0.0 && value.is_finite() {
        Ok(value)
    } else {
        Err(format!(
            "the helix curve's {what} must be greater than zero, got {value}"
        ))
    }
}

fn finite(value: f64, what: &str) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!(
            "the helix curve's {what} must be a finite number, got {value}"
        ))
    }
}

impl FeatureInfo for HelixCurveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.axis.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.axis.check(ctx, Want::Axis)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        positive(value(self.pitch), "pitch")?;
        positive(value(self.revolutions), "revolutions")?;
        if let Some(r) = self.radius {
            positive(value(r), "radius")?;
        }
        if let Some(a) = self.start_angle {
            finite(value(a), "start angle")?;
        }
        if let Some(g) = self.growth {
            finite(value(g), "growth")?;
        }
        Ok(())
    }
}

/// Where the helix turns: its axis' start and the way it rises, and the
/// round face it was picked on (a cylinder or a cone, its axis as the
/// kernel gives it).
struct Placement {
    start: Vec3,
    direction: Vec3,
    face: Option<SurfaceGeometry>,
}

impl HelixCurveDef {
    /// The axis, the start along it and the round face, if one was picked.
    fn placement<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Placement, String> {
        let sign = if self.flip { -1.0 } else { 1.0 };
        if let GeomRef::Face { body, face } = &self.axis {
            let surface = ctx.surface(&self.axis)?;
            let (origin, axis) = match &surface {
                SurfaceGeometry::Cylinder { origin, axis, .. }
                | SurfaceGeometry::Cone { origin, axis, .. } => (*origin, *axis),
                _ if self.radius.is_some() => return self.on_axis(ctx, sign),
                _ => {
                    return Err(crate::notice::placed(
                        "the face is neither cylindrical nor conical: give the helix curve a \
                         radius, or pick a cylindrical or conical face",
                        crate::notice::NoticePlace::face(face.to_string())
                            .in_body(&body.to_string()),
                        format!(
                            "face {face} is {}, not a cylinder or a cone",
                            surface.kind()
                        ),
                    ));
                }
            };
            let direction = scale(unit(axis).ok_or("the face's axis has no direction")?, sign);
            // Where the face starts along the way the helix rises: its
            // boundary's lowest point that way.
            let shape = Resolver::shape(&mut *ctx, *body)?;
            let found = super::renamed::face_for(&mut *ctx, &shape, face)?;
            let boundary = ctx
                .kernel
                .curves_of(&shape, &TopoName::Face(found))
                .map_err(|e| e.to_string())?;
            let pieces: Vec<PathCurve> = boundary
                .into_iter()
                .map(|curve| PathCurve {
                    name: String::new(),
                    curve,
                })
                .collect();
            let low = super::intersection_curve::curve_polyline(&pieces)
                .into_iter()
                .map(|p| dot(sub(p, origin), direction))
                .min_by(f64::total_cmp)
                .unwrap_or(0.0);
            return Ok(Placement {
                start: add(origin, scale(direction, low)),
                direction,
                face: Some(surface),
            });
        }
        self.on_axis(ctx, sign)
    }

    /// The axis alone: the helix starts at its origin.
    fn on_axis<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        sign: f64,
    ) -> Result<Placement, String> {
        let line = ctx.axis(&self.axis)?;
        let direction = scale(
            unit(line.direction).ok_or("the helix curve's axis has no direction")?,
            sign,
        );
        Ok(Placement {
            start: line.origin,
            direction,
            face: None,
        })
    }
}

/// `v` (at right angles to the unit vector `k`) turned by `angle` about
/// `k`, right-handed.
fn turned(v: Vec3, k: Vec3, angle: f64) -> Vec3 {
    add(scale(v, angle.cos()), scale(cross(k, v), angle.sin()))
}

impl<K: Kernel> Evaluate<K> for HelixCurveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let pitch = positive(ctx.param(self.pitch)?, "pitch")?;
        let revolutions = positive(ctx.param(self.revolutions)?, "revolutions")?;
        let start_angle = match self.start_angle {
            Some(a) => finite(ctx.param(a)?, "start angle")?,
            None => 0.0,
        };
        let given_growth = match self.growth {
            Some(g) => Some(finite(ctx.param(g)?, "growth")?),
            None => None,
        };
        let given_radius = match self.radius {
            Some(r) => Some(positive(ctx.param(r)?, "radius")?),
            None => None,
        };
        let Placement {
            start,
            direction,
            face,
        } = self.placement(ctx)?;
        // The radius and the growth the face gives: a cylinder's radius; a
        // cone's radius where the helix starts, and its widening per pitch
        // risen.
        let (face_radius, face_growth) = match face {
            Some(SurfaceGeometry::Cylinder { radius, .. }) => (Some(radius), 0.0),
            Some(SurfaceGeometry::Cone {
                origin,
                axis,
                radius,
                half_angle,
            }) => {
                let along = unit(axis).unwrap_or(direction);
                let at = radius + dot(sub(start, origin), along) * half_angle.tan();
                (Some(at), pitch * dot(direction, along) * half_angle.tan())
            }
            _ => (None, 0.0),
        };
        let radius = match (given_radius, face_radius) {
            (Some(r), _) => r,
            (None, Some(r)) if r > 1e-9 => r,
            (None, Some(_)) => {
                return Err(
                    "the cone's radius is zero where the helix curve starts: give it a radius"
                        .to_owned(),
                );
            }
            (None, None) => {
                return Err(
                    "the helix curve needs a radius, or a cylindrical or conical face to take \
                     it from"
                        .to_owned(),
                );
            }
        };
        let growth = match given_growth {
            Some(g) => g,
            None if given_radius.is_none() => face_growth,
            None => 0.0,
        };
        if growth < 0.0 && radius + growth * revolutions <= 1e-9 {
            return Err(format!(
                "the helix curve narrows to its axis before its last turn: a radius of {radius} \
                 mm narrowing {} mm per turn reaches the axis after {:.3} turns",
                -growth,
                radius / -growth
            ));
        }
        let x_axis = turned(
            mitcad_model_base::internal::datum::default_x_axis(direction),
            direction,
            start_angle,
        );
        let pieces: Vec<Curve3> = if growth == 0.0 {
            let (axis, rise) = if self.left_handed {
                (scale(direction, -1.0), -pitch)
            } else {
                (direction, pitch)
            };
            vec![Curve3::Helix {
                center: start,
                axis,
                x_axis,
                radius,
                pitch: rise,
                start: 0.0,
                end: std::f64::consts::TAU * revolutions,
            }]
        } else {
            ctx.kernel
                .helix_curve(&HelixCurveSpec {
                    origin: start,
                    direction,
                    x_axis,
                    radius,
                    pitch,
                    revolutions,
                    left_handed: self.left_handed,
                    growth,
                })
                .map_err(|e| format!("helix curve: {e}"))?
        };
        if pieces.is_empty() {
            return Err("the helix curve could not be made".to_owned());
        }
        let uid = ctx.uid;
        let curve = pieces
            .into_iter()
            .enumerate()
            .map(|(i, curve)| PathCurve {
                name: format!("{uid}:curve{}", i + 1),
                curve,
            })
            .collect();
        Ok(FeatureOutput {
            curve: Some(curve),
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_turns_right_handed_about_its_axis() {
        let v = turned(
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_2,
        );
        assert!(mitcad_vec3::distance(v, [0.0, 1.0, 0.0]) < 1e-12, "{v:?}");
        let v = turned(
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            std::f64::consts::FRAC_PI_2,
        );
        assert!(mitcad_vec3::distance(v, [0.0, -1.0, 0.0]) < 1e-12, "{v:?}");
    }
}
