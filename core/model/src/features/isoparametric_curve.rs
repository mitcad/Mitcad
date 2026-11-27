// SPDX-License-Identifier: MIT
//! Isoparametric Curve (mitcad#2522): the curves of a face along which one
//! of its surface's two parameters keeps one value, as 3D curves that
//! follow the face when it changes. A u curve holds u and runs along v (on
//! a cylinder a line along its axis), a v curve holds v and runs along u
//! (on a cylinder a circle about its axis). The curve lies at a position,
//! a fraction (0 to 1) of the face's range of the held parameter; or
//! through the face's point nearest to a picked point; or there are
//! several, evenly spaced across the face (at 1 / (n + 1), 2 / (n + 1),
//! ... of the range). The geometry comes from the kernel
//! ([`Kernel::iso_curves`]): the surface's own iso curve, exact where it is
//! a line, a circle or a B-spline, cut to the face.
//!
//! The curves are the feature's [`FeatureOutput::curve`], as an
//! Intersection Curve's and a Helix Curve's are, their pieces `F5:curve1`,
//! `F5:curve2`, ... in order: one curve that is one piece is a path for the
//! features after it (`{"feature": "F5"}`); Project and Intersect take
//! every piece, and the view draws each curve.
//!
//! The face may be on a body of another component (mitcad#2554), linked
//! as a construction point's faces are (`links`, mitcad#1029): the body is
//! read in its component and moved into the feature's coordinates by the
//! occurrences' placements at the feature's point of the timeline. The
//! picked point is kept in the coordinates of the face's body, where the
//! view's click gives it, and moved by the same link, so the curve keeps
//! running through the clicked place when either occurrence moves.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Resolver, Want};
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::datum::Vec3;
use crate::ids::BodyUid;
use crate::kernel::{IsoParameter, IsoPlace, Kernel};
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::sweeps::PathCurve;

/// The most curves one feature makes evenly spaced.
pub const MAX_COUNT: f64 = 100.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IsoparametricCurveDef<P = ParamId> {
    /// The face of a body whose curves these are.
    pub face: GeomRef,
    /// The surface parameter the curves hold: `u` (the default) or `v`.
    #[serde(default, skip_serializing_if = "is_u")]
    pub direction: IsoParameter,
    /// Where the one curve lies: a fraction (0 to 1) of the face's range
    /// of the held parameter, 0 at its least.
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub position: Option<P>,
    /// A point in the coordinates of the face's body (the feature's
    /// component's, or with a link the linked component's): the one curve
    /// runs through the face's point nearest to it, wherever the face then
    /// lies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<Vec3>,
    /// How many curves, evenly spaced across the face: at 1 / (n + 1),
    /// 2 / (n + 1), ... n / (n + 1) of its range (a whole number, 1 to
    /// 100).
    #[serde(
        default = "super::pattern::none",
        skip_serializing_if = "Option::is_none"
    )]
    pub count: Option<P>,
    /// The face's body when it is another component's (mitcad#2554, as a
    /// construction point's `links`): read in its component and moved,
    /// with the point, into the feature's coordinates by the placements at
    /// its point of the timeline.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<BodyUid, OccurrenceLink>,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

fn is_u(direction: &IsoParameter) -> bool {
    *direction == IsoParameter::U
}

impl<P> IsoparametricCurveDef<P> {
    pub const TYPE: &'static str = "isoparametric_curve";
    pub const BASE_NAME: &'static str = "IsoparametricCurve";

    // Written by hand: the optional values map when they are there.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<IsoparametricCurveDef<Q>, E> {
        Ok(IsoparametricCurveDef {
            face: self.face.clone(),
            direction: self.direction,
            position: self
                .position
                .as_ref()
                .map(|p| f("position", p))
                .transpose()?,
            point: self.point,
            count: self.count.as_ref().map(|c| f("count", c)).transpose()?,
            links: self.links.clone(),
            values: PhantomData,
        })
    }
}

/// A position checked: a number from 0 to 1.
fn position(value: f64) -> Result<f64, String> {
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "the isoparametric curve's position must lie from 0 to 1 across the face, got {value}"
        ))
    }
}

/// A count checked: a whole number from 1 to [`MAX_COUNT`].
fn count(value: f64) -> Result<usize, String> {
    if value.is_finite()
        && (value - value.round()).abs() <= 1e-9
        && (1.0..=MAX_COUNT).contains(&value.round())
    {
        Ok(value.round() as usize)
    } else {
        Err(format!(
            "the isoparametric curves' count must be a whole number from 1 to {MAX_COUNT}, got {value}"
        ))
    }
}

impl FeatureInfo for IsoparametricCurveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.face.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let places = [
            self.position.is_some(),
            self.point.is_some(),
            self.count.is_some(),
        ]
        .iter()
        .filter(|given| **given)
        .count();
        if places != 1 {
            return Err(
                "an isoparametric curve needs one of a position, a point or a count".to_owned(),
            );
        }
        if let Some(point) = self.point
            && point.iter().any(|v| !v.is_finite())
        {
            return Err("the isoparametric curve's point must be finite".to_owned());
        }
        self.face.check(ctx, Want::Face)?;
        // Only the face's body is linked (mitcad#2554).
        let face_body = match &self.face {
            GeomRef::Face { body, .. } => Some(*body),
            _ => None,
        };
        if self.links.keys().any(|body| Some(*body) != face_body) {
            return Err(
                "the isoparametric curve links a body that is not the one of its face".to_owned(),
            );
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(p) = self.position {
            position(value(p))?;
        }
        if let Some(c) = self.count {
            count(value(c))?;
        }
        Ok(())
    }
}

impl IsoparametricCurveDef {
    /// Where the curves lie, from the values now.
    fn place<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<IsoPlace, String> {
        if let Some(point) = self.point {
            return Ok(IsoPlace::Through(point));
        }
        if let Some(c) = self.count {
            let n = count(ctx.param(c)?)?;
            return Ok(IsoPlace::Fractions(
                (1..=n).map(|i| i as f64 / (n + 1) as f64).collect(),
            ));
        }
        let p = self
            .position
            .ok_or("an isoparametric curve needs one of a position, a point or a count")?;
        Ok(IsoPlace::Fractions(vec![position(ctx.param(p)?)?]))
    }
}

impl<K: Kernel> Evaluate<K> for IsoparametricCurveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let GeomRef::Face { body, face } = &self.face else {
            return Err("an isoparametric curve needs a face of a body".to_owned());
        };
        // A face of another component's body (mitcad#2554): read there and
        // moved into the feature's coordinates.
        ctx.geom_links.clone_from(&self.links);
        let mut place = self.place(ctx)?;
        let shape = Resolver::shape(&mut *ctx, *body)?;
        // The picked point is in the body's coordinates: moved as the body
        // is, so the curve runs through the clicked place wherever the
        // occurrences put the face.
        if let (IsoPlace::Through(point), Some(link)) = (&mut place, self.links.get(body)) {
            let (_, transform) = ctx
                .linked(link)
                .map_err(|e| format!("a linked body: {e}"))?;
            *point = transform.apply_point(*point);
        }
        let found = super::renamed::face_for(&mut *ctx, &shape, face)?;
        let curves = ctx
            .kernel
            .iso_curves(&shape, &found, self.direction, &place)
            .map_err(|e| format!("isoparametric curve: {e}"))?;
        let uid = ctx.uid;
        let curve: Vec<PathCurve> = curves
            .into_iter()
            .flat_map(|curve| curve.pieces)
            .enumerate()
            .map(|(i, curve)| PathCurve {
                name: format!("{uid}:curve{}", i + 1),
                curve,
            })
            .collect();
        if curve.is_empty() {
            return Err("the isoparametric curve could not be made".to_owned());
        }
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
    fn counts_and_positions_are_checked() {
        assert_eq!(count(3.0), Ok(3));
        assert!(count(0.0).is_err());
        assert!(count(2.5).is_err());
        assert!(count(101.0).is_err());
        assert!(count(f64::NAN).is_err());
        assert_eq!(position(0.0), Ok(0.0));
        assert_eq!(position(1.0), Ok(1.0));
        assert!(position(-0.1).is_err());
        assert!(position(1.5).is_err());
    }
}
