// SPDX-License-Identifier: MIT
//! Sketches on cylindrical and conical faces (mitcad#1768): the face's
//! development found at every evaluation ([`development`]), the sketch's
//! curves as curves on the face ([`curve3`]), its profiles raised or sunk
//! along the face's normals ([`wrapped_regions`], Extrude and Emboss) and
//! its curves laid onto the face as Split Face's tool ([`curves_tool`]).
//!
//! The map itself is `mitcad_model::developed` (below the model, for the
//! sketch's output): a sketch point goes onto the face as an emboss's wrap
//! lays a sketch onto it, so that what the kernel wraps lands where the
//! sketch's curves are drawn.

use std::f64::consts::TAU;

use mitcad_model_base::segments::NamedSegment;
use mitcad_vec3::{add, dot, scale, unit};

use super::EvalContext;
use super::emboss::counted;
use super::sketch::{SketchOutput, SketchPlane, face_body};
use crate::developed::{DevelopedSurface, Development};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::{Axis, Curve3, EmbossWrapSpec, Kernel, WrappedCurvesSpec};
use crate::notice::{NoticePlace, about_place};
use crate::profile::ProfileRegion;
use crate::sketch::geometry::{Curve2, angle_after};
use crate::topo::TopoName;

/// The development of a sketch's curved face, from the bodies before the
/// sketch: the face's cylinder or cone, the sketch's origin at the face's
/// low end along its axis (counted towards +Z, else +Y, else +X, as an
/// emboss's Along Axis counts; a cone's at its other end when the low end
/// is its apex) at the plane's angle about the axis from the reference
/// direction across it (the model X axis, else Y), the sketch's y axis
/// along the counted axis and its x axis to the right as the face is seen
/// from outside the material.
pub(crate) fn development<K: Kernel>(
    plane: &SketchPlane,
    ctx: &mut EvalContext<'_, K>,
) -> Result<Development, String> {
    let SketchPlane::Developed { face, body, angle } = plane else {
        return Err("the sketch is not on a curved face".to_owned());
    };
    let shapes = match body {
        Some(body) => vec![(*body, ctx.body(*body)?)],
        None => ctx.bodies(),
    };
    let (_, shape, found) = face_body(ctx.kernel, face, shapes)?;
    if let Some(found) = &found {
        ctx.found_renamed("face", face, found, face.feature);
    }
    let name = found.as_ref().unwrap_or(&**face);
    let place = || NoticePlace::face(face.to_string());
    if let Ok(cylinder) = ctx.kernel.face_cylinder(&shape, name) {
        let along = unit(cylinder.axis.direction)
            .ok_or_else(|| about_place(place(), "the sketch's cylindrical face has no axis"))?;
        let up = counted(along);
        // Its low end along the counted axis.
        let start = if dot(up, along) > 0.0 {
            cylinder.axis.origin
        } else {
            add(cylinder.axis.origin, scale(along, cylinder.length))
        };
        return Development::new(
            DevelopedSurface::Cylinder {
                radius: cylinder.radius,
            },
            start,
            up,
            cylinder.internal,
            0.0,
            crate::datum::default_x_axis(up),
            *angle,
            up,
        )
        .ok_or_else(|| about_place(place(), "the sketch's cylindrical face has no size"));
    }
    if let Ok(cone) = ctx.kernel.face_cone(&shape, name) {
        let up = counted(cone.axis);
        // The face's extent along the cone's axis, from its edges.
        let curves = ctx
            .kernel
            .curves_of(&shape, &TopoName::Face(name.clone()))
            .map_err(|e| about_place(place(), &format!("the sketch's conical face: {e}")))?;
        let heights: Vec<f64> = curves
            .iter()
            .flat_map(super::path::samples)
            .map(|p| dot(mitcad_vec3::sub(p, cone.apex), cone.axis))
            .collect();
        let low = heights.iter().copied().fold(f64::INFINITY, f64::min);
        let high = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !(low.is_finite() && high.is_finite() && high > 0.0) {
            return Err(about_place(
                place(),
                "the sketch's conical face has no edges to measure it by",
            ));
        }
        let size = high.abs().max(1.0);
        // The end lowest along the counted axis, unless it is the apex.
        let (first, other) = if dot(up, cone.axis) > 0.0 {
            (low, high)
        } else {
            (high, low)
        };
        let start = if first > 1e-6 * size { first } else { other };
        return Development::new(
            DevelopedSurface::Cone {
                half_angle: cone.half_angle,
            },
            cone.apex,
            cone.axis,
            cone.internal,
            start,
            crate::datum::default_x_axis(up),
            *angle,
            up,
        )
        .ok_or_else(|| about_place(place(), "the sketch's conical face lies only at its apex"));
    }
    Err(about_place(
        place(),
        "the sketch's face is neither cylindrical nor conical",
    ))
}

/// A sketch curve on the face: a line along a cylinder's axis or a cone's
/// generator, a circle about the axis (a cylinder's line around it, a
/// cone's arc about its apex), a helix (a cylinder's slanted line); other
/// curves through their points (`Development::curve`).
pub(crate) fn curve3(development: &Development, curve: &Curve2) -> Curve3 {
    let exact = match curve {
        Curve2::Line { a, b } => return development.line(*a, *b),
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => development.arc_about_apex(*center, *radius, *start, angle_after(*start, *end)),
        Curve2::Circle {
            center,
            radius,
            start,
        } => development.arc_about_apex(*center, *radius, *start, start + TAU),
        _ => None,
    };
    exact.unwrap_or_else(|| {
        let (t0, t1) = curve.domain();
        development.curve(&|t| curve.point(t), t0, t1)
    })
}

/// The length of a sketch curve on the face.
#[cfg(test)]
pub(crate) fn length_on_face(development: &Development, curve: &Curve2) -> f64 {
    let (t0, t1) = curve.domain();
    development.length_on_face(&|t| curve.point(t), t0, t1)
}

/// The tool of a developed sketch's profiles raised out of the material by
/// `depth` along the face's normals or sunk into it, the walls tilted by
/// `taper` (radians, as an emboss's): the kernel's wrap of the regions
/// onto the face (`Kernel::wrapped_emboss`, faces named as an emboss's).
pub(crate) fn wrapped_regions<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    development: &Development,
    regions: &[ProfileRegion],
    depth: f64,
    raise: bool,
    taper: f64,
) -> Result<K::Shape, String> {
    let outwards = if development.internal { -1.0 } else { 1.0 };
    let offset = if raise { outwards } else { -outwards } * depth;
    let radius = development.radius();
    let half_angle = development.half_angle();
    if radius + offset * half_angle.cos() <= radius * 1e-6 {
        return Err(format!(
            "the depth {depth} mm reaches the axis of the sketch's face"
        ));
    }
    let spec = EmbossWrapSpec {
        feature: ctx.uid,
        frame: development.frame,
        regions,
        axis: Axis {
            origin: development.origin,
            direction: development.axis,
        },
        radius,
        to_radius: radius + offset,
        half_angle,
        taper,
    };
    ctx.kernel.wrapped_emboss(&spec).map_err(|e| e.to_string())
}

/// Split Face's tool of a developed sketch's curves (`curves`, or every
/// curve but construction curves when empty): each laid onto the face and
/// ruled a little along its normals both ways (`Kernel::wrapped_curves`),
/// a surface body that crosses the face along the curves.
pub(crate) fn curves_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    sketch: FeatureUid,
    development: &Development,
    output: &SketchOutput,
    curves: &[EntityUid],
) -> Result<K::Shape, String> {
    let name = ctx.feature_name(sketch);
    for id in curves {
        if !output.solved.curves.contains_key(id) {
            return Err(format!("{name} has no curve {}", id.curve_name()));
        }
    }
    let construction = super::path::construction_curves(ctx, sketch);
    let named: Vec<NamedSegment> = output
        .solved
        .curves
        .iter()
        .filter(|(id, _)| {
            if curves.is_empty() {
                !construction.contains(*id)
            } else {
                curves.contains(*id)
            }
        })
        .map(|(id, curve)| NamedSegment {
            name: id.curve_name(),
            geometry: curve.whole(),
        })
        .collect();
    if named.is_empty() {
        return Err(format!("{name} has none of the curves"));
    }
    let radius = development.radius();
    let width = (0.1 * radius).min(0.5);
    let spec = WrappedCurvesSpec {
        feature: ctx.uid,
        frame: development.frame,
        curves: &named,
        axis: Axis {
            origin: development.origin,
            direction: development.axis,
        },
        radius,
        half_angle: development.half_angle(),
        inner: -width,
        outer: width,
    };
    ctx.kernel.wrapped_curves(&spec).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rod() -> Development {
        Development::new(
            DevelopedSurface::Cylinder { radius: 10.0 },
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            false,
            0.0,
            [1.0, 0.0, 0.0],
            0.0,
            [0.0, 0.0, 1.0],
        )
        .unwrap()
    }

    #[test]
    fn sketch_lines_and_arcs_go_onto_the_face() {
        let d = rod();
        let around = Curve2::Line {
            a: [0.0, 5.0],
            b: [10.0 * std::f64::consts::PI, 5.0],
        };
        // Half a turn around the rod: a half circle.
        assert!(matches!(curve3(&d, &around), Curve3::Conic { .. }));
        assert!((length_on_face(&d, &around) - 10.0 * std::f64::consts::PI).abs() < 1e-6);
        // A circle in the sketch keeps its length on a cylinder.
        let circle = Curve2::Circle {
            center: [0.0, 5.0],
            radius: 3.0,
            start: 0.0,
        };
        assert!(matches!(curve3(&d, &circle), Curve3::BSpline { .. }));
        assert!((length_on_face(&d, &circle) - 6.0 * std::f64::consts::PI).abs() < 1e-6);
    }
}
