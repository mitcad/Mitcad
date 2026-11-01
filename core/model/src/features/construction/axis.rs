// SPDX-License-Identifier: MIT
//! Construction axes (also those of imported .f3d designs). An axis is a
//! line with a direction; the direction sets the sense of rotations about
//! it (revolve, circular pattern). Mitcad's directions (K68): through a
//! cylinder, cone or torus: the surface's axis; two planes: n1 × n2; two
//! points: from the first to the second; an edge: along the edge's curve;
//! perpendicular or normal to a face at a point: the outward normal.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::{GeomRef, Want, distinct, finite};
use crate::datum::{Datum, DatumAxis, SurfaceGeometry, Vec3};
use crate::features::mapping::map_params;
use crate::features::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use mitcad_vec3::unit;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructionAxisDef<P = ParamId> {
    pub definition: AxisDefinition,
    /// Reverses the direction, e.g. to match an imported design whose stored
    /// axis points the other way (K68).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// Bodies of other components that the definition's faces, edges and
    /// vertices are on (mitcad#1029, as a construction plane's `links`):
    /// each is read in its component and moved into the axis's coordinates
    /// by the placements at its point of the timeline.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<BodyUid, OccurrenceLink>,
    /// Construction planes, axes and points (mitcad#1035) and sketches
    /// (mitcad#1039) of other components that the definition names, as a
    /// construction plane's `datum_links`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub datum_links: BTreeMap<FeatureUid, OccurrenceLink>,
    /// Axes have no values; the definition is generic like the others.
    #[serde(skip)]
    pub values: PhantomData<P>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AxisDefinition {
    /// Axis Through Cylinder/Cone/Torus.
    CircularFace { face: GeomRef },
    /// Axis Perpendicular at Point: normal to a face, through the face
    /// point nearest to the point.
    PerpendicularAtPoint { face: GeomRef, point: GeomRef },
    /// Axis Through Two Planes: their intersection.
    TwoPlanes { plane1: GeomRef, plane2: GeomRef },
    /// Axis Through Two Points.
    TwoPoints { point1: GeomRef, point2: GeomRef },
    /// Axis Through Edge: a straight edge (or another axis).
    Edge { edge: GeomRef },
    /// Axis Perpendicular to Face at Point: through the point, along the
    /// face normal at the face point nearest to it.
    NormalToFaceAtPoint { face: GeomRef, point: GeomRef },
    /// A fixed axis (the non-parametric `setByLine` of .f3d designs, the importer's
    /// fallback); the direction is normalized.
    Fixed { origin: Vec3, direction: Vec3 },
}

impl<P> ConstructionAxisDef<P> {
    pub const TYPE: &'static str = "construction_axis";
    pub const BASE_NAME: &'static str = "Axis";

    pub fn new(definition: AxisDefinition) -> Self {
        Self {
            definition,
            flip: false,
            links: BTreeMap::new(),
            datum_links: BTreeMap::new(),
            values: PhantomData,
        }
    }

    map_params! {
        ConstructionAxisDef {
            definition: clone,
            flip: copy,
            links: clone,
            datum_links: clone,
            values: phantom,
        }
    }

    fn inputs(&self) -> Vec<(&GeomRef, Want)> {
        use AxisDefinition as D;
        match &self.definition {
            D::CircularFace { face } => vec![(face, Want::Face)],
            D::PerpendicularAtPoint { face, point } => {
                vec![(face, Want::Face), (point, Want::Point)]
            }
            D::NormalToFaceAtPoint { face, point } => {
                vec![(face, Want::FaceOrPlane), (point, Want::Point)]
            }
            D::TwoPlanes { plane1, plane2 } => vec![(plane1, Want::Plane), (plane2, Want::Plane)],
            D::TwoPoints { point1, point2 } => vec![(point1, Want::Point), (point2, Want::Point)],
            D::Edge { edge } => vec![(edge, Want::Line)],
            D::Fixed { .. } => Vec::new(),
        }
    }
}

impl FeatureInfo for ConstructionAxisDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for (reference, _) in self.inputs() {
            reference.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let inputs = self.inputs();
        for (reference, want) in &inputs {
            reference.check(ctx, *want)?;
        }
        distinct(&inputs.iter().map(|(r, _)| *r).collect::<Vec<_>>())?;
        if let AxisDefinition::Fixed { origin, direction } = &self.definition {
            fixed(*origin, *direction)?;
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ConstructionAxisDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        use AxisDefinition as D;
        ctx.geom_links.clone_from(&self.links);
        ctx.datum_links.clone_from(&self.datum_links);
        let axis = match &self.definition {
            D::CircularFace { face } => match ctx.surface(face)? {
                SurfaceGeometry::Cylinder { origin, axis, .. }
                | SurfaceGeometry::Cone { origin, axis, .. } => DatumAxis {
                    origin,
                    direction: axis,
                },
                SurfaceGeometry::Torus { center, axis, .. } => DatumAxis {
                    origin: center,
                    direction: axis,
                },
                other => {
                    return Err(format!(
                        "{face} is not a cylinder, cone or torus ({})",
                        other.kind()
                    ));
                }
            },
            D::PerpendicularAtPoint { face, point } => {
                let point = ctx.point(point)?;
                let on = ctx.face_point(face, point)?;
                DatumAxis {
                    origin: on.point,
                    direction: on.normal,
                }
            }
            D::NormalToFaceAtPoint { face, point } => {
                let point = ctx.point(point)?;
                let on = ctx.face_point(face, point)?;
                DatumAxis {
                    origin: point,
                    direction: on.normal,
                }
            }
            D::TwoPlanes { plane1, plane2 } => {
                let (first, second) = (ctx.plane(plane1)?, ctx.plane(plane2)?);
                first
                    .intersection(&second)
                    .ok_or("the two planes are parallel")?
            }
            D::TwoPoints { point1, point2 } => {
                DatumAxis::through(ctx.point(point1)?, ctx.point(point2)?)
                    .ok_or("the two points are the same point")?
            }
            D::Edge { edge } => ctx.axis(edge)?,
            D::Fixed { origin, direction } => fixed(*origin, *direction)?,
        };
        let axis = if self.flip {
            DatumAxis {
                direction: axis.direction.map(|v| -v),
                ..axis
            }
        } else {
            axis
        };
        Ok(FeatureOutput {
            datum: Some(finite(Datum::Axis(axis))?),
            ..FeatureOutput::default()
        })
    }
}

fn fixed(origin: Vec3, direction: Vec3) -> Result<DatumAxis, String> {
    if !origin.iter().chain(&direction).all(|v| v.is_finite()) {
        return Err("the axis's numbers must be finite".to_owned());
    }
    Ok(DatumAxis {
        origin,
        direction: unit(direction).ok_or("the axis's direction is zero")?,
    })
}
