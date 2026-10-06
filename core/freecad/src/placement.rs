// SPDX-License-Identifier: MIT
//! FreeCAD's placements: a rotation (a unit quaternion) then a translation,
//! in millimetres. `App::PropertyPlacement` writes the position `Px Py Pz`,
//! the quaternion `Q0 Q1 Q2 Q3` (x, y, z, w) and the same rotation as an
//! axis and an angle (`Ox Oy Oz`, `A`); the quaternion is read.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Placement {
    pub position: [f64; 3],
    /// Unit quaternion (x, y, z, w).
    pub rotation: [f64; 4],
}

impl Default for Placement {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Placement {
    pub const IDENTITY: Self = Self {
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };

    /// The quaternion is normalized (FreeCAD writes 16 digits); a zero one
    /// is no rotation.
    pub fn from_quaternion(position: [f64; 3], q: [f64; 4]) -> Self {
        let norm = q.iter().map(|c| c * c).sum::<f64>().sqrt();
        let rotation = if norm > 0.0 && norm.is_finite() {
            q.map(|c| c / norm)
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };
        Self { position, rotation }
    }

    pub fn translation(position: [f64; 3]) -> Self {
        Self {
            position,
            ..Self::IDENTITY
        }
    }

    /// A rotation of `angle` radians about `axis` (right-hand rule) through
    /// the origin, then the translation.
    pub fn from_axis_angle(position: [f64; 3], axis: [f64; 3], angle: f64) -> Self {
        let norm = axis.iter().map(|c| c * c).sum::<f64>().sqrt();
        if norm == 0.0 {
            return Self::translation(position);
        }
        let (s, c) = (angle / 2.0).sin_cos();
        let a = axis.map(|v| v / norm * s);
        Self::from_quaternion(position, [a[0], a[1], a[2], c])
    }

    /// The rotation as a matrix, row-major.
    pub fn matrix(&self) -> [[f64; 3]; 3] {
        let [x, y, z, w] = self.rotation;
        [
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - z * w),
                2.0 * (x * z + y * w),
            ],
            [
                2.0 * (x * y + z * w),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - x * w),
            ],
            [
                2.0 * (x * z - y * w),
                2.0 * (y * z + x * w),
                1.0 - 2.0 * (x * x + y * y),
            ],
        ]
    }

    /// The transform as three rows of four numbers (rotation and
    /// translation).
    pub fn rows(&self) -> [[f64; 4]; 3] {
        let m = self.matrix();
        std::array::from_fn(|r| [m[r][0], m[r][1], m[r][2], self.position[r]])
    }

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let m = self.matrix();
        std::array::from_fn(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + self.position[r])
    }

    /// `self` after `other`: a point is placed by `other`, then by `self`
    /// (FreeCAD's `self.multiply(other)`).
    pub fn multiply(&self, other: &Placement) -> Placement {
        let [ax, ay, az, aw] = self.rotation;
        let [bx, by, bz, bw] = other.rotation;
        let rotation = [
            aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
            aw * bw - ax * bx - ay * by - az * bz,
        ];
        Placement::from_quaternion(self.apply(other.position), rotation)
    }

    pub fn inverse(&self) -> Placement {
        let [x, y, z, w] = self.rotation;
        let rotated = Placement {
            position: [0.0; 3],
            rotation: [-x, -y, -z, w],
        };
        let p = rotated.apply(self.position);
        Placement {
            position: p.map(|c| -c),
            rotation: rotated.rotation,
        }
    }

    /// No rotation (within `tolerance` in the quaternion) and no
    /// translation (within `tolerance` millimetres).
    pub fn is_identity(&self, tolerance: f64) -> bool {
        let [x, y, z, _] = self.rotation;
        self.position.iter().all(|c| c.abs() <= tolerance)
            && [x, y, z].iter().all(|c| c.abs() <= tolerance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-12)
    }

    #[test]
    fn quarter_turns_and_compositions() {
        // 90 degrees about Z, as FreeCAD writes it, then moved.
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let p = Placement::from_quaternion([1.0, 2.0, 3.0], [0.0, 0.0, s, s]);
        assert!(close(p.apply([1.0, 0.0, 0.0]), [1.0, 3.0, 3.0]));
        assert!(close(
            p.inverse().apply(p.apply([4.0, 5.0, 6.0])),
            [4.0, 5.0, 6.0]
        ));
        // About X by 90 degrees, then the turn about Z.
        let x = Placement::from_axis_angle([0.0; 3], [1.0, 0.0, 0.0], std::f64::consts::FRAC_PI_2);
        let both = p.multiply(&x);
        assert!(close(
            both.apply([0.0, 1.0, 0.0]),
            p.apply(x.apply([0.0, 1.0, 0.0]))
        ));
        assert!(close(both.apply([0.0, 1.0, 0.0]), [1.0, 2.0, 4.0]));
        assert!(Placement::IDENTITY.is_identity(0.0));
        assert!(!p.is_identity(1e-9));
        assert_eq!(
            Placement::from_quaternion([0.0; 3], [0.0; 4]),
            Placement::IDENTITY
        );
    }
}
