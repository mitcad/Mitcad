// SPDX-License-Identifier: MIT
//! Placements of the transform family (moves, copies, patterns, mirrors,
//! alignments, scales and primitives): affine maps of model space and the
//! kernel data these features exchange. Millimetres and radians.

use crate::ids::FeatureUid;
use crate::profile::SketchFrame;
use crate::topo::{FaceName, RoleKey};

/// An affine map of model space, `p' = linear · p + translation`: a rigid
/// motion, a mirror or a scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Row-major.
    pub linear: [[f64; 3]; 3],
    pub translation: [f64; 3],
}

/// Relative tolerance of the rigidity and identity checks.
const TOLERANCE: f64 = 1e-9;

impl Transform {
    pub const IDENTITY: Self = Self {
        linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };

    pub fn translation(offset: [f64; 3]) -> Self {
        Self {
            translation: offset,
            ..Self::IDENTITY
        }
    }

    /// Rotation by `angle` about the axis through `origin` along `direction`,
    /// counter-clockwise looking against the direction (right-hand rule).
    /// None for a zero direction.
    pub fn rotation(origin: [f64; 3], direction: [f64; 3], angle: f64) -> Option<Self> {
        let [x, y, z] = normalized(direction)?;
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        let linear = [
            [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
            [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
            [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
        ];
        Some(Self::about(origin, linear))
    }

    /// Reflection in the plane through `origin` with the normal. None for a
    /// zero normal.
    pub fn mirror(origin: [f64; 3], normal: [f64; 3]) -> Option<Self> {
        let n = normalized(normal)?;
        let linear = std::array::from_fn(|r| {
            std::array::from_fn(|c| f64::from(u8::from(r == c)) - 2.0 * n[r] * n[c])
        });
        Some(Self::about(origin, linear))
    }

    /// Scale about `center` by factors along the model axes.
    pub fn scale(center: [f64; 3], factors: [f64; 3]) -> Self {
        let linear =
            std::array::from_fn(|r| std::array::from_fn(|c| if r == c { factors[r] } else { 0.0 }));
        Self::about(center, linear)
    }

    /// The rigid motion that takes the frame `from` onto `to`: origin onto
    /// origin, axes onto axes. Both frames must be orthonormal.
    pub fn between_frames(from: &SketchFrame, to: &SketchFrame) -> Self {
        let a = [from.x_axis, from.y_axis, from.normal()];
        let b = [to.x_axis, to.y_axis, to.normal()];
        // linear = B · Aᵀ, columns of A and B being the axes.
        let linear = std::array::from_fn(|r| {
            std::array::from_fn(|c| (0..3).map(|k| b[k][r] * a[k][c]).sum())
        });
        let mut map = Self {
            linear,
            translation: [0.0; 3],
        };
        let moved = map.apply_point(from.origin);
        map.translation = sub(to.origin, moved);
        map
    }

    /// `linear` about a fixed point.
    fn about(point: [f64; 3], linear: [[f64; 3]; 3]) -> Self {
        let mut map = Self {
            linear,
            translation: [0.0; 3],
        };
        map.translation = sub(point, map.apply_vector(point));
        map
    }

    /// The map that applies `first`, then this one.
    pub fn after(&self, first: &Transform) -> Self {
        Self {
            linear: std::array::from_fn(|r| {
                std::array::from_fn(|c| {
                    (0..3).map(|k| self.linear[r][k] * first.linear[k][c]).sum()
                })
            }),
            translation: self.apply_point(first.translation),
        }
    }

    pub fn apply_point(&self, p: [f64; 3]) -> [f64; 3] {
        add(self.apply_vector(p), self.translation)
    }

    pub fn apply_vector(&self, v: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|r| dot(self.linear[r], v))
    }

    pub fn determinant(&self) -> f64 {
        let m = &self.linear;
        dot(m[0], cross(m[1], m[2]))
    }

    /// True for rotations combined with translations.
    pub fn is_rigid(&self) -> bool {
        self.is_orthogonal() && self.determinant() > 0.0
    }

    /// True for rigid motions and mirrors.
    pub fn is_orthogonal(&self) -> bool {
        (0..3).all(|r| {
            (0..3).all(|c| {
                let product: f64 = (0..3).map(|k| self.linear[k][r] * self.linear[k][c]).sum();
                (product - f64::from(u8::from(r == c))).abs() <= TOLERANCE
            })
        })
    }

    pub fn is_identity(&self) -> bool {
        let scale = self.translation.iter().fold(1.0_f64, |m, v| m.max(v.abs()));
        (0..3).all(|r| {
            (0..3).all(|c| (self.linear[r][c] - f64::from(u8::from(r == c))).abs() <= TOLERANCE)
        }) && self
            .translation
            .iter()
            .all(|v| v.abs() <= TOLERANCE * scale)
    }

    pub fn is_finite(&self) -> bool {
        self.linear.iter().flatten().all(|v| v.is_finite())
            && self.translation.iter().all(|v| v.is_finite())
    }

    /// A frame moved by a rigid motion.
    pub fn apply_frame(&self, frame: &SketchFrame) -> SketchFrame {
        SketchFrame {
            origin: self.apply_point(frame.origin),
            x_axis: self.apply_vector(frame.x_axis),
            y_axis: self.apply_vector(frame.y_axis),
        }
    }
}

/// Names the faces of a copy made by a pattern, mirror or copying move:
/// every face name `n` becomes `<feature>:inst<index>(n)`. Instance 0 is
/// the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instance {
    pub feature: FeatureUid,
    pub index: u32,
}

impl Instance {
    /// `F9:inst2`, the part before the original name.
    pub fn prefix(&self) -> String {
        format!("{}:inst{}", self.feature, self.index)
    }

    /// The name of the copy of a face.
    pub fn face(&self, original: FaceName) -> FaceName {
        FaceName::new(
            self.feature,
            &format!("inst{}", self.index),
            Some(RoleKey::Face(Box::new(original))),
        )
    }
}

/// A primitive solid on `frame`: the box's first corner, the base centre
/// of a cylinder, the centre of a sphere or a torus, with the frame normal
/// as the height or axis direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimitiveShape {
    Box {
        length: f64,
        width: f64,
        height: f64,
    },
    Cylinder {
        radius: f64,
        height: f64,
    },
    Sphere {
        radius: f64,
    },
    Torus {
        major_radius: f64,
        minor_radius: f64,
    },
}

/// Input of `Kernel::primitive`; faces are named after `feature`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimitiveSpec {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub shape: PrimitiveShape,
}

pub(crate) fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

pub(crate) fn scaled(v: [f64; 3], factor: f64) -> [f64; 3] {
    v.map(|x| x * factor)
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn length(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

/// The unit vector, None for a zero or non-finite one.
pub(crate) fn normalized(v: [f64; 3]) -> Option<[f64; 3]> {
    let l = length(v);
    (l > 1e-12 && l.is_finite()).then(|| scaled(v, 1.0 / l))
}

/// A unit vector at right angles to the unit vector `v`.
pub(crate) fn perpendicular(v: [f64; 3]) -> [f64; 3] {
    let other = if v[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    normalized(cross(v, other)).expect("not parallel")
}

/// The rotation about `pivot` that turns the unit vector `from` onto `to`,
/// about `fallback` (a unit vector at right angles to `from`) when they are
/// opposite.
pub(crate) fn rotation_between(
    pivot: [f64; 3],
    from: [f64; 3],
    to: [f64; 3],
    fallback: [f64; 3],
) -> Transform {
    let axis = cross(from, to);
    let angle = length(axis).atan2(dot(from, to));
    if angle.abs() <= 1e-12 {
        return Transform::IDENTITY;
    }
    let axis = if length(axis) <= 1e-12 {
        fallback
    } else {
        axis
    };
    Transform::rotation(pivot, axis, angle).expect("a non-zero axis")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn rotations_follow_the_right_hand_rule() {
        let quarter =
            Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2).unwrap();
        assert!(near(quarter.apply_point([1.0, 0.0, 5.0]), [0.0, 1.0, 5.0]));
        assert!(quarter.is_rigid());
        let about =
            Transform::rotation([10.0, 0.0, 0.0], [0.0, 0.0, 2.0], std::f64::consts::PI).unwrap();
        assert!(near(about.apply_point([20.0, 0.0, 0.0]), [0.0, 0.0, 0.0]));
        assert!(Transform::rotation([0.0; 3], [0.0; 3], 1.0).is_none());
    }

    #[test]
    fn mirrors_and_scales() {
        let mirror = Transform::mirror([30.0, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        assert!(near(
            mirror.apply_point([15.0, 20.0, 3.0]),
            [45.0, 20.0, 3.0]
        ));
        assert!(mirror.is_orthogonal() && !mirror.is_rigid());
        assert!((mirror.determinant() + 1.0).abs() < 1e-12);
        let scale = Transform::scale([1.0, 1.0, 1.0], [2.0, 1.0, 0.5]);
        assert!(near(scale.apply_point([3.0, 3.0, 3.0]), [5.0, 3.0, 2.0]));
        assert!(!scale.is_orthogonal());
    }

    #[test]
    fn composition_and_frames() {
        let shift = Transform::translation([5.0, 0.0, 0.0]);
        let turn =
            Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2).unwrap();
        // Shift first, then turn.
        let both = turn.after(&shift);
        assert!(near(both.apply_point([0.0; 3]), [0.0, 5.0, 0.0]));
        assert!(
            Transform::IDENTITY
                .after(&Transform::IDENTITY)
                .is_identity()
        );

        let to = SketchFrame {
            origin: [1.0, 2.0, 3.0],
            x_axis: [0.0, 1.0, 0.0],
            y_axis: [0.0, 0.0, 1.0],
        };
        let map = Transform::between_frames(&SketchFrame::XY, &to);
        assert!(map.is_rigid());
        assert!(near(map.apply_point([1.0, 0.0, 0.0]), [1.0, 3.0, 3.0]));
        let moved = map.apply_frame(&SketchFrame::XY);
        assert!(near(moved.origin, to.origin) && near(moved.normal(), to.normal()));
    }

    #[test]
    fn rotation_between_vectors() {
        let r = rotation_between([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        assert!(near(r.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
        let flip = rotation_between([0.0; 3], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
        assert!(near(flip.apply_vector([0.0, 0.0, 1.0]), [0.0, 0.0, -1.0]));
        assert!(near(flip.apply_vector([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]));
        assert!(
            near(perpendicular([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]) || {
                let p = perpendicular([0.0, 0.0, 1.0]);
                dot(p, [0.0, 0.0, 1.0]).abs() < 1e-12 && (length(p) - 1.0).abs() < 1e-12
            }
        );
    }

    #[test]
    fn instances_wrap_face_names() {
        let original: FaceName = "F2:side(c1[c4,c2])".parse().unwrap();
        let instance = Instance {
            feature: FeatureUid(9),
            index: 2,
        };
        let name = instance.face(original);
        assert_eq!(name.to_string(), "F9:inst2(F2:side(c1[c4,c2]))");
        assert_eq!(instance.prefix(), "F9:inst2");
        let split: FaceName = "F9:inst1(F2:end(r{c5})#1)".parse().unwrap();
        assert_eq!(split.to_string(), "F9:inst1(F2:end(r{c5})#1)");
        assert!(split.features().contains(&FeatureUid(2)));
    }
}
