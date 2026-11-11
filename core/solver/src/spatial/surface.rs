// SPDX-License-Identifier: MIT
//! Constant surfaces of the spatial system (mitcad#1549): the faces a
//! point of a 3D sketch lies on, mapped into the sketch's local frame by
//! the caller. Analytic surfaces are solved in closed form: the relation's
//! one row is the signed distance from the point to the surface. A face of
//! another kind (a B-spline surface, a surface of revolution, an offset
//! surface) is given as the plane that touches it at the face's point
//! nearest to the point (the kernel's closest-point projection); the caller
//! projects the solved point again and replaces that plane until the point
//! lies on the face ([`super::SpatialSystem::set_surface`]).

use std::f64::consts::FRAC_PI_2;

use crate::scalar::{Scalar, V3};

/// A surface that does not move while the spatial system solves, in the
/// sketch's local frame. Directions are unit vectors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpatialSurface {
    /// The plane through `origin` with the unit `normal`.
    Plane { origin: [f64; 3], normal: [f64; 3] },
    /// The cylinder of `radius` about the line through `origin` along the
    /// unit `axis`.
    Cylinder {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    /// The cone about the line through `origin` along the unit `axis`
    /// whose radius is `radius` at `origin` and grows along `axis` by
    /// `tan(half_angle)` (`half_angle` strictly between `-pi/2` and
    /// `pi/2`, not zero).
    Cone {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        half_angle: f64,
    },
    /// The sphere of `radius` around `center`.
    Sphere { center: [f64; 3], radius: f64 },
    /// The torus around `center` about the unit `axis`: the circle of
    /// `major` radius in the plane through `center` across `axis`, swept by
    /// a circle of `minor` radius.
    Torus {
        center: [f64; 3],
        axis: [f64; 3],
        major: f64,
        minor: f64,
    },
}

fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn unit_ok(v: [f64; 3]) -> bool {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n - 1.0).abs() < 1e-9
}

/// Keeps the derivative of a length finite where it is zero (a point on a
/// cylinder's axis): `sqrt(q.q + EPS2)` differs from `|q|` by far less
/// than the acceptance tolerance anywhere a surface is solved.
const EPS2: f64 = 1e-24;

fn length<T: Scalar>(q: V3<T>) -> T {
    (q.dot(q) + T::cst(EPS2)).sqrt()
}

impl SpatialSurface {
    /// Checks the surface: finite values, unit directions, positive radii
    /// (a cone's may be zero at its origin), a cone's half angle between
    /// nothing and a right angle.
    pub fn check(&self) -> Result<(), &'static str> {
        use SpatialSurface::*;
        let ok = match *self {
            Plane { origin, normal } => finite(&origin) && unit_ok(normal),
            Cylinder {
                origin,
                axis,
                radius,
            } => finite(&origin) && unit_ok(axis) && radius.is_finite() && radius > 0.0,
            Cone {
                origin,
                axis,
                radius,
                half_angle,
            } => {
                finite(&origin)
                    && unit_ok(axis)
                    && radius.is_finite()
                    && radius >= 0.0
                    && half_angle.is_finite()
                    && half_angle != 0.0
                    && half_angle.abs() < FRAC_PI_2
            }
            Sphere { center, radius } => finite(&center) && radius.is_finite() && radius > 0.0,
            Torus {
                center,
                axis,
                major,
                minor,
            } => {
                finite(&center)
                    && unit_ok(axis)
                    && major.is_finite()
                    && minor.is_finite()
                    && major > 0.0
                    && minor > 0.0
            }
        };
        if ok {
            Ok(())
        } else {
            Err(
                "a surface needs finite values, unit directions, positive radii and a cone's \
                 half angle between nothing and a right angle",
            )
        }
    }

    /// `plane`, `cylinder`, `cone`, `sphere` or `torus`.
    pub fn kind(&self) -> &'static str {
        match self {
            SpatialSurface::Plane { .. } => "plane",
            SpatialSurface::Cylinder { .. } => "cylinder",
            SpatialSurface::Cone { .. } => "cone",
            SpatialSurface::Sphere { .. } => "sphere",
            SpatialSurface::Torus { .. } => "torus",
        }
    }

    /// The signed distance from `p` to the surface (millimetres): the
    /// relation's row. Positive on the side the plane's normal points to,
    /// and outside a cylinder, cone, sphere or torus.
    pub(crate) fn distance<T: Scalar>(&self, p: V3<T>) -> T {
        use SpatialSurface::*;
        match *self {
            Plane { origin, normal } => V3::cst(normal).dot(p.sub(V3::cst(origin))),
            Cylinder {
                origin,
                axis,
                radius,
            } => {
                let w = p.sub(V3::cst(origin));
                let a = V3::cst(axis);
                let across = w.sub(a.mul(w.dot(a)));
                length(across) - T::cst(radius)
            }
            Cone {
                origin,
                axis,
                radius,
                half_angle,
            } => {
                // In the half plane through the axis: the generator line
                // `rho = radius + h tan(angle)`, its distance from (h, rho).
                let w = p.sub(V3::cst(origin));
                let a = V3::cst(axis);
                let h = w.dot(a);
                let rho = length(w.sub(a.mul(h)));
                (rho - T::cst(radius) - h.mulf(half_angle.tan())).mulf(half_angle.cos())
            }
            Sphere { center, radius } => length(p.sub(V3::cst(center))) - T::cst(radius),
            Torus {
                center,
                axis,
                major,
                minor,
            } => {
                let w = p.sub(V3::cst(center));
                let a = V3::cst(axis);
                let h = w.dot(a);
                let rho = length(w.sub(a.mul(h))) - T::cst(major);
                (rho * rho + h * h + T::cst(EPS2)).sqrt() - T::cst(minor)
            }
        }
    }

    /// The signed distance from `p` to the surface.
    pub fn distance_to(&self, p: [f64; 3]) -> f64 {
        self.distance(V3::<f64>::cst(p)).val()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_of_the_analytic_surfaces() {
        let z = [0.0, 0.0, 1.0];
        let plane = SpatialSurface::Plane {
            origin: [0.0, 0.0, 5.0],
            normal: z,
        };
        assert!((plane.distance_to([3.0, 4.0, 8.0]) - 3.0).abs() < 1e-12);
        let cylinder = SpatialSurface::Cylinder {
            origin: [0.0; 3],
            axis: z,
            radius: 10.0,
        };
        assert!(cylinder.distance_to([6.0, 8.0, 40.0]).abs() < 1e-12);
        assert!((cylinder.distance_to([0.0, 5.0, -3.0]) + 5.0).abs() < 1e-12);
        let sphere = SpatialSurface::Sphere {
            center: [1.0, 1.0, 1.0],
            radius: 2.0,
        };
        assert!((sphere.distance_to([1.0, 1.0, 4.0]) - 1.0).abs() < 1e-12);
        // 45 degrees: radius 10 at the origin, 20 at h = 10.
        let cone = SpatialSurface::Cone {
            origin: [0.0; 3],
            axis: z,
            radius: 10.0,
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        assert!(cone.distance_to([20.0, 0.0, 10.0]).abs() < 1e-12);
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!((cone.distance_to([21.0, 0.0, 10.0]) - s).abs() < 1e-12);
        let torus = SpatialSurface::Torus {
            center: [0.0; 3],
            axis: z,
            major: 20.0,
            minor: 5.0,
        };
        assert!(torus.distance_to([20.0, 0.0, 5.0]).abs() < 1e-12);
        assert!((torus.distance_to([0.0, 27.0, 0.0]) - 2.0).abs() < 1e-12);
        for surface in [plane, cylinder, sphere, cone, torus] {
            assert_eq!(surface.check(), Ok(()), "{surface:?}");
        }
        let flat = SpatialSurface::Cone {
            origin: [0.0; 3],
            axis: z,
            radius: 1.0,
            half_angle: 0.0,
        };
        assert!(flat.check().is_err());
        let skew = SpatialSurface::Plane {
            origin: [0.0; 3],
            normal: [0.0, 0.0, 2.0],
        };
        assert!(skew.check().is_err());
    }
}
