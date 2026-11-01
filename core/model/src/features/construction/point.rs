// SPDX-License-Identifier: MIT
//! Construction points (also those of imported .f3d designs): vertices,
//! intersections and centres. Lines are extended to intersect; lines that
//! pass each other without meeting are an error (what imported designs
//! expect there is experiment K69).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{GeomRef, PathRef, Want, along_slot, distinct, finite};
use crate::datum::{
    CurveGeometry, Datum, DatumAxis, DatumPlane, DatumPoint, PathParameter, SurfaceGeometry, Vec3,
};
use crate::features::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use mitcad_model_base::internal::datum::{ANGULAR, LINEAR};
use mitcad_vec3::{add, norm, scale, sub};
use mitcad_vec3::{cross, dot};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructionPointDef<P = ParamId> {
    pub definition: PointDefinition<P>,
    /// Bodies of other components that the definition's faces, edges and
    /// vertices are on (mitcad#1029, as a construction plane's `links`):
    /// each is read in its component and moved into the point's
    /// coordinates by the placements at its point of the timeline.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<BodyUid, OccurrenceLink>,
    /// Construction planes, axes and points (mitcad#1035) and sketches
    /// (mitcad#1039) of other components that the definition names, as a
    /// construction plane's `datum_links`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub datum_links: BTreeMap<FeatureUid, OccurrenceLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PointDefinition<P = ParamId> {
    /// Point at Vertex (or at another point).
    Point { point: GeomRef },
    /// Point Through Two Edges: where two lines meet.
    TwoEdges { line1: GeomRef, line2: GeomRef },
    /// Point Through Three Planes.
    ThreePlanes {
        plane1: GeomRef,
        plane2: GeomRef,
        plane3: GeomRef,
    },
    /// Point at Center of Circle/Sphere/Torus: a circular edge, or a
    /// spherical or toroidal face.
    Center { entity: GeomRef },
    /// Point at Edge and Plane: where a line meets a plane.
    EdgeAndPlane { line: GeomRef, plane: GeomRef },
    /// Point Along Path: at a fraction of the path's length, or a length
    /// (mm) from its start with `physical`.
    AlongPath {
        path: PathRef,
        distance: P,
        #[serde(default, skip_serializing_if = "is_false")]
        physical: bool,
    },
    /// A fixed point (the non-parametric points of .f3d designs, the importer's
    /// fallback).
    Fixed { point: Vec3 },
}

impl<P> ConstructionPointDef<P> {
    pub const TYPE: &'static str = "construction_point";
    pub const BASE_NAME: &'static str = "Point";

    pub fn new(definition: PointDefinition<P>) -> Self {
        Self {
            definition,
            links: BTreeMap::new(),
            datum_links: BTreeMap::new(),
        }
    }

    // Written by hand: the definition maps by its kind (`definition.distance`,
    // `definition.fraction`).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ConstructionPointDef<Q>, E> {
        use PointDefinition as D;
        let definition = match &self.definition {
            D::Point { point } => D::Point {
                point: point.clone(),
            },
            D::TwoEdges { line1, line2 } => D::TwoEdges {
                line1: line1.clone(),
                line2: line2.clone(),
            },
            D::ThreePlanes {
                plane1,
                plane2,
                plane3,
            } => D::ThreePlanes {
                plane1: plane1.clone(),
                plane2: plane2.clone(),
                plane3: plane3.clone(),
            },
            D::Center { entity } => D::Center {
                entity: entity.clone(),
            },
            D::EdgeAndPlane { line, plane } => D::EdgeAndPlane {
                line: line.clone(),
                plane: plane.clone(),
            },
            D::AlongPath {
                path,
                distance,
                physical,
            } => D::AlongPath {
                path: path.clone(),
                // A fraction of the path is unitless (mitcad#662).
                distance: f(along_slot(*physical), distance)?,
                physical: *physical,
            },
            D::Fixed { point } => D::Fixed { point: *point },
        };
        Ok(ConstructionPointDef {
            definition,
            links: self.links.clone(),
            datum_links: self.datum_links.clone(),
        })
    }

    fn inputs(&self) -> Vec<(&GeomRef, Want)> {
        use PointDefinition as D;
        match &self.definition {
            D::Point { point } => vec![(point, Want::Point)],
            D::TwoEdges { line1, line2 } => vec![(line1, Want::Line), (line2, Want::Line)],
            D::ThreePlanes {
                plane1,
                plane2,
                plane3,
            } => vec![
                (plane1, Want::Plane),
                (plane2, Want::Plane),
                (plane3, Want::Plane),
            ],
            D::Center { entity } => vec![(entity, Want::EdgeOrFace)],
            D::EdgeAndPlane { line, plane } => vec![(line, Want::Line), (plane, Want::Plane)],
            D::AlongPath { .. } | D::Fixed { .. } => Vec::new(),
        }
    }
}

impl FeatureInfo for ConstructionPointDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for (reference, _) in self.inputs() {
            reference.add_to(&mut references);
        }
        if let PointDefinition::AlongPath { path, .. } = &self.definition {
            path.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let inputs = self.inputs();
        for (reference, want) in &inputs {
            reference.check(ctx, *want)?;
        }
        distinct(&inputs.iter().map(|(r, _)| *r).collect::<Vec<_>>())?;
        if let PointDefinition::AlongPath { path, .. } = &self.definition {
            path.check(ctx)?;
        }
        if let PointDefinition::Fixed { point } = &self.definition
            && !point.iter().all(|v| v.is_finite())
        {
            return Err("the point's coordinates must be finite".to_owned());
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ConstructionPointDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        use PointDefinition as D;
        ctx.geom_links.clone_from(&self.links);
        ctx.datum_links.clone_from(&self.datum_links);
        let point = match &self.definition {
            D::Point { point } => ctx.point(point)?,
            D::TwoEdges { line1, line2 } => meet(&ctx.axis(line1)?, &ctx.axis(line2)?)?,
            D::ThreePlanes {
                plane1,
                plane2,
                plane3,
            } => common_point(
                &ctx.plane(plane1)?,
                &ctx.plane(plane2)?,
                &ctx.plane(plane3)?,
            )?,
            D::Center { entity } => match entity {
                GeomRef::Edge { .. } => match ctx.curve(entity)? {
                    CurveGeometry::Circle { center, .. } => center,
                    other => {
                        return Err(format!(
                            "{entity} is not a circle or an arc ({})",
                            other.kind()
                        ));
                    }
                },
                _ => match ctx.surface(entity)? {
                    SurfaceGeometry::Sphere { center, .. }
                    | SurfaceGeometry::Torus { center, .. } => center,
                    other => {
                        return Err(format!(
                            "{entity} is not a sphere or a torus ({})",
                            other.kind()
                        ));
                    }
                },
            },
            D::EdgeAndPlane { line, plane } => crossing(&ctx.axis(line)?, &ctx.plane(plane)?)?,
            D::AlongPath {
                path,
                distance,
                physical,
            } => {
                let value = ctx.param(*distance)?;
                let at = if *physical {
                    PathParameter::Length(value)
                } else {
                    PathParameter::Fraction(value)
                };
                ctx.path_point(path, at)?.point
            }
            D::Fixed { point } => *point,
        };
        Ok(FeatureOutput {
            datum: Some(finite(Datum::Point(DatumPoint { point }))?),
            ..FeatureOutput::default()
        })
    }
}

/// Where two lines meet, extended as far as needed.
pub(crate) fn meet(first: &DatumAxis, second: &DatumAxis) -> Result<Vec3, String> {
    let (d1, d2) = (first.direction, second.direction);
    let b = dot(d1, d2);
    let denominator = 1.0 - b * b;
    if denominator.sqrt() < ANGULAR {
        return Err("the two lines are parallel".to_owned());
    }
    let w = sub(first.origin, second.origin);
    let (d, e) = (dot(d1, w), dot(d2, w));
    let p = add(first.origin, scale(d1, (b * e - d) / denominator));
    let q = add(second.origin, scale(d2, (e - b * d) / denominator));
    let apart = norm(sub(p, q));
    if apart > LINEAR {
        return Err(format!(
            "the two lines do not meet (they pass {apart:.6} mm apart)"
        ));
    }
    Ok(scale(add(p, q), 0.5))
}

/// The point the three planes share.
pub(crate) fn common_point(a: &DatumPlane, b: &DatumPlane, c: &DatumPlane) -> Result<Vec3, String> {
    let (n1, n2, n3) = (a.normal(), b.normal(), c.normal());
    let determinant = dot(n1, cross(n2, n3));
    if determinant.abs() < ANGULAR {
        return Err("the three planes do not meet in one point".to_owned());
    }
    let (h1, h2, h3) = (dot(n1, a.origin), dot(n2, b.origin), dot(n3, c.origin));
    let sum = add(
        add(scale(cross(n2, n3), h1), scale(cross(n3, n1), h2)),
        scale(cross(n1, n2), h3),
    );
    Ok(scale(sum, 1.0 / determinant))
}

/// Where a line meets a plane, extended as far as needed.
pub(crate) fn crossing(line: &DatumAxis, plane: &DatumPlane) -> Result<Vec3, String> {
    let normal = plane.normal();
    let along = dot(line.direction, normal);
    if along.abs() < ANGULAR {
        return Err("the line is parallel to the plane".to_owned());
    }
    let t = dot(sub(plane.origin, line.origin), normal) / along;
    Ok(add(line.origin, scale(line.direction, t)))
}
