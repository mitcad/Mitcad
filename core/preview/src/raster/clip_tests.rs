// SPDX-License-Identifier: MIT
//! Sides that run far past the picture, or end just in front of the eye,
//! are drawn along their true lines and without gaps (mitcad#1606).

use std::f64::consts::SQRT_2;

use mitcad_vec3::{Vec3, add, scale};

use super::{ACCENT, Corner, Paint, Projector, Target, cut, draw_line, draw_triangle};
use crate::camera::{Camera, Projection};

const WIDTH: usize = 400;
const HEIGHT: usize = 300;
const EDGE: [u8; 3] = [255, 0, 0];

/// From -Y toward the origin, Z up: the camera's frame's right is +X,
/// its up +Z, its forward +Y; the near plane 0.1 mm in front of the eye.
fn projector(projection: Projection) -> Projector {
    let camera = Camera {
        eye: [0.0, -100.0, 0.0],
        target: [0.0; 3],
        up: [0.0, 0.0, 1.0],
        projection,
        height: 10.0,
        fov: 45.0,
    };
    Projector::new(&[], &camera, WIDTH, HEIGHT)
}

/// A point given in the camera's frame, in the design's coordinates.
fn world(p: &Projector, c: Vec3) -> Vec3 {
    add(
        add(add(p.eye, scale(p.right, c[0])), scale(p.up, c[1])),
        scale(p.forward, c[2]),
    )
}

/// A point's exact sample coordinates (the camera's frame), unclipped,
/// worked out here on their own.
fn exact(p: &Projector, c: Vec3) -> (f64, f64) {
    let w = if p.perspective { c[2] } else { 1.0 };
    (p.cx + p.focal * c[0] / w, p.cy - p.focal * c[1] / w)
}

/// A point (the camera's frame) just in front of the near plane, seen
/// `right` samples right of the picture's centre and `up` samples above
/// it.
fn just_in_front(p: &Projector, right: f64, up: f64) -> Vec3 {
    let z = p.near * 1.0001;
    [right * z / p.focal, up * z / p.focal, z]
}

/// Along the exact line from `a` (in the picture) toward `b`, every point
/// in the picture has a sample in `drawn` within one sample of its own,
/// and every sample in `drawn` lies on the line, within the brush's
/// `reach` and one sample. How many points were checked.
fn check_line(target: &Target, drawn: [u8; 4], a: (f64, f64), b: (f64, f64), reach: f64) -> usize {
    let is_drawn = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && (x as usize) < target.width
            && (y as usize) < target.height
            && target.color[y as usize * target.width + x as usize] == drawn
    };
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx.hypot(dy);
    let (ux, uy) = (dx / length, dy / length);
    let mut checked = 0;
    let mut along = 0.0;
    while along <= length {
        let (x, y) = (a.0 + ux * along, a.1 + uy * along);
        if x < 0.0 || y < 0.0 || x >= target.width as f64 || y >= target.height as f64 {
            break;
        }
        let (sx, sy) = (x.floor() as i64, y.floor() as i64);
        let found = (-1..=1).any(|oy| (-1..=1).any(|ox| is_drawn(sx + ox, sy + oy)));
        assert!(
            found,
            "nothing drawn near ({x:.2}, {y:.2}), {along:.2} samples along the line"
        );
        checked += 1;
        along += 0.25;
    }
    for y in 0..target.height {
        for x in 0..target.width {
            if target.color[y * target.width + x] != drawn {
                continue;
            }
            let (px, py) = (x as f64 + 0.5 - a.0, y as f64 + 0.5 - a.1);
            let off = (px * uy - py * ux).abs();
            assert!(
                off <= reach * SQRT_2 + 1.0,
                "sample ({x}, {y}) is {off:.2} samples off the line"
            );
        }
    }
    checked
}

/// The ticket's case: an edge from a point in the picture to a point just
/// in front of the near plane, far off the axis (about 1.6 million samples
/// from the picture's centre), is drawn along its exact line, whole.
#[test]
fn an_edge_to_just_in_front_of_the_near_plane_is_whole_and_on_its_line() {
    let p = projector(Projection::Perspective);
    let mut target = Target::new(WIDTH, HEIGHT, 1);
    let a = [5.0, -3.0, 100.0];
    let b = just_in_front(&p, 1.24e6, 1.05e6);
    assert!(draw_line(
        &mut target,
        &p,
        world(&p, a),
        world(&p, b),
        EDGE,
        1
    ));
    let rgba = [EDGE[0], EDGE[1], EDGE[2], 255];
    // About 238 samples of it are in the picture.
    let checked = check_line(&target, rgba, exact(&p, a), exact(&p, b), 0.5);
    assert!(checked > 800, "{checked}");
    // Drawn from the far end too, alike.
    let mut back = Target::new(WIDTH, HEIGHT, 1);
    assert!(draw_line(
        &mut back,
        &p,
        world(&p, b),
        world(&p, a),
        EDGE,
        1
    ));
    check_line(&back, rgba, exact(&p, a), exact(&p, b), 0.5);
}

/// Without the near plane: an orthographic edge three million samples
/// long (far more than the stamps a line once had) is whole in the
/// picture.
#[test]
fn a_long_orthographic_edge_is_whole_and_on_its_line() {
    let p = projector(Projection::Orthographic);
    let mut target = Target::new(WIDTH, HEIGHT, 1);
    let a = [1.0, 0.5, 100.0];
    let b = [1.0 + 8.0e4, 0.5 + 6.0e4, 100.0];
    assert!(draw_line(
        &mut target,
        &p,
        world(&p, a),
        world(&p, b),
        EDGE,
        1
    ));
    let rgba = [EDGE[0], EDGE[1], EDGE[2], 255];
    // About 212 samples of it are in the picture.
    let checked = check_line(&target, rgba, exact(&p, a), exact(&p, b), 0.5);
    assert!(checked > 700, "{checked}");
    // Wholly past a side, or behind the eye: nothing drawn.
    let mut empty = Target::new(WIDTH, HEIGHT, 1);
    let left = [-1.0e3, 0.0, 100.0];
    assert!(!draw_line(
        &mut empty,
        &p,
        world(&p, left),
        world(&p, [-1.0e5, 3.0e4, 100.0]),
        EDGE,
        1
    ));
    let perspective = projector(Projection::Perspective);
    assert!(!draw_line(
        &mut empty,
        &perspective,
        world(&perspective, [1.0, 1.0, -5.0]),
        world(&perspective, [-1.0, 2.0, -50.0]),
        EDGE,
        1
    ));
    assert!(empty.color.iter().all(|c| c[3] == 0));
}

/// A highlighted edge (drawn over everything, two samples wide) to just in
/// front of the near plane is whole and on its line too.
#[test]
fn a_highlighted_edge_to_just_in_front_of_the_near_plane_is_whole() {
    let p = projector(Projection::Perspective);
    let mut target = Target::new(WIDTH, HEIGHT, 1);
    let a = [5.0, -3.0, 100.0];
    let b = just_in_front(&p, 1.24e6, 1.05e6);
    assert!(cut::draw_on_top(
        &mut target,
        &p,
        world(&p, a),
        world(&p, b),
        1
    ));
    let rgba = [ACCENT[0], ACCENT[1], ACCENT[2], 255];
    let checked = check_line(&target, rgba, exact(&p, a), exact(&p, b), 1.0);
    assert!(checked > 800, "{checked}");
}

/// A face with a corner just in front of the near plane, far off the
/// axis, fills its exact projected triangle: every sample one sample or
/// more inside it is filled, none one sample or more outside.
#[test]
fn a_face_to_just_in_front_of_the_near_plane_fills_its_exact_outline() {
    let p = projector(Projection::Perspective);
    let mut target = Target::new(WIDTH, HEIGHT, 1);
    let points = [
        [5.0, -3.0, 100.0],
        just_in_front(&p, 1.24e6, 1.05e6),
        [-20.0, -10.0, 100.0],
    ];
    let corners = points.map(|c| Corner { c, color: [0.5; 3] });
    draw_triangle(&mut target, &p, corners, Paint::Opaque);
    let screen = points.map(|c| exact(&p, c));
    let cross = |o: (f64, f64), u: (f64, f64), v: (f64, f64)| {
        (u.0 - o.0) * (v.1 - o.1) - (u.1 - o.1) * (v.0 - o.0)
    };
    let turn = cross(screen[0], screen[1], screen[2]).signum();
    // A point's distance inside the triangle (negative: outside), samples.
    let inside = |q: (f64, f64)| {
        [(0, 1), (1, 2), (2, 0)]
            .iter()
            .map(|&(i, j)| {
                let (u, v) = (screen[i], screen[j]);
                turn * cross(u, v, q) / (v.0 - u.0).hypot(v.1 - u.1)
            })
            .fold(f64::INFINITY, f64::min)
    };
    let (mut filled, mut empty) = (0usize, 0usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let depth = inside((x as f64 + 0.5, y as f64 + 0.5));
            let drawn = target.color[y * WIDTH + x][3] == 255;
            if depth >= 1.0 {
                assert!(
                    drawn,
                    "sample ({x}, {y}) inside by {depth:.2} is not filled"
                );
                filled += 1;
            } else if depth <= -1.0 {
                assert!(!drawn, "sample ({x}, {y}) outside by {depth:.2} is filled");
                empty += 1;
            }
        }
    }
    assert!(filled > 5000 && empty > 5000, "{filled} {empty}");
}
