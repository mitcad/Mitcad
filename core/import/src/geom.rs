// SPDX-License-Identifier: MIT
//! Small vector helpers and the file's internal units: the IR is in
//! centimetres and radians, Mitcad in millimetres and radians.

use mitcad_f3d::design::ir::{Mat4, Vec3};
use mitcad_model::SketchFrame;
use mitcad_model::features::FrameDef;

/// Millimetres per centimetre.
pub const MM_PER_CM: f64 = 10.0;

pub fn mm(cm: f64) -> f64 {
    cm * MM_PER_CM
}

pub fn mm3(v: Vec3) -> Vec3 {
    v.map(mm)
}

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn scale(a: Vec3, s: f64) -> Vec3 {
    a.map(|v| v * s)
}

pub fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

pub fn distance(a: Vec3, b: Vec3) -> f64 {
    norm(sub(a, b))
}

/// The unit vector; None for a (nearly) zero vector.
pub fn unit(a: Vec3) -> Option<Vec3> {
    let n = norm(a);
    (n > 1e-12).then(|| scale(a, 1.0 / n))
}

/// A sketch's placement in model space: the dump's `model_frame`
/// (`sketch_to_model` columns), in millimetres. The axes are made unit
/// vectors at right angles (the stored ones are, up to rounding).
pub fn frame_of(m: &Mat4) -> Option<SketchFrame> {
    let col = |c: usize| [m[0][c], m[1][c], m[2][c]];
    let x = unit(col(0))?;
    let y0 = col(1);
    // Gram-Schmidt against x for rounding errors.
    let y = unit(sub(y0, scale(x, dot(x, y0))))?;
    Some(SketchFrame {
        origin: mm3(col(3)),
        x_axis: x,
        y_axis: y,
    })
}

/// `frame` expressed in `plane`'s frame (x, y, normal coordinates), as a
/// sketch definition's `frame`; None when it is the plane's own frame.
pub fn relative_frame(plane: &SketchFrame, frame: &SketchFrame) -> Option<FrameDef> {
    let n = plane.normal();
    let local = |v: Vec3| [dot(v, plane.x_axis), dot(v, plane.y_axis), dot(v, n)];
    let def = FrameDef {
        origin: local(sub(frame.origin, plane.origin)),
        x_axis: local(frame.x_axis),
        y_axis: local(frame.y_axis),
    };
    let same = norm(def.origin) < 1e-9
        && distance(def.x_axis, [1.0, 0.0, 0.0]) < 1e-12
        && distance(def.y_axis, [0.0, 1.0, 0.0]) < 1e-12;
    (!same).then_some(def)
}

/// The frame's normal (x × y).
pub fn normal(frame: &SketchFrame) -> Vec3 {
    cross(frame.x_axis, frame.y_axis)
}

/// Sketch coordinates of a model point.
pub fn to_sketch(frame: &SketchFrame, p: Vec3) -> [f64; 2] {
    let d = sub(p, frame.origin);
    [dot(d, frame.x_axis), dot(d, frame.y_axis)]
}

/// Whether two frames lie in the same plane (normals parallel either way,
/// origins on it) within `tolerance` millimetres.
pub fn coplanar(a: &SketchFrame, b_origin: Vec3, b_normal: Vec3, tolerance: f64) -> bool {
    let n = normal(a);
    let parallel = norm(cross(n, b_normal)) < 1e-6;
    parallel && dot(sub(b_origin, a.origin), n).abs() < tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_relative_to_planes() {
        let xy = SketchFrame::XY;
        assert_eq!(relative_frame(&xy, &xy), None);
        // A sketch on the XY plane turned a quarter and moved by 1 cm in x.
        let m = [
            [0.0, -1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let f = frame_of(&m).unwrap();
        assert_eq!(f.origin, [10.0, 0.0, 0.0]);
        let def = relative_frame(&xy, &f).unwrap();
        assert_eq!(def.origin, [10.0, 0.0, 0.0]);
        assert_eq!(def.x_axis, [0.0, 1.0, 0.0]);
        assert_eq!(def.y_axis, [-1.0, 0.0, 0.0]);
        assert_eq!(def.apply(&xy), f);
        assert_eq!(to_sketch(&f, [10.0, 2.0, 0.0]), [2.0, 0.0]);
        assert!(coplanar(&f, [3.0, 4.0, 0.0], [0.0, 0.0, -1.0], 1e-9));
        assert!(!coplanar(&f, [3.0, 4.0, 0.1], [0.0, 0.0, 1.0], 1e-3));
    }
}
