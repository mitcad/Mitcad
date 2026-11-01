// SPDX-License-Identifier: MIT
//! The automatic orientation on the print bed (mitcad#1109,
//! `exchange/orient.rs`) on face meshes built here: a lying tube with a
//! flat square flange lies on the flange, not on one of the flange's
//! narrow sides nor on the tube's end; a block on its largest face; a cube
//! keeps its bottom (−Z down) among faces alike; a piece without planar
//! faces has no orientation.

use std::f64::consts::TAU;

use mitcad_model::exchange::FaceMeshes;
use mitcad_model::exchange::orient::best_bed_face;

type V = [f64; 3];

const TOL: f64 = 1e-6;

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: V) -> V {
    let length = dot(a, a).sqrt();
    [a[0] / length, a[1] / length, a[2] / length]
}

/// Face meshes made face by face, each triangle turned counter-clockwise
/// seen from the side its outward normal points to.
#[derive(Default)]
struct Builder {
    mesh: FaceMeshes,
    planar: Vec<bool>,
}

impl Builder {
    /// A face of `triangles`, outward along `outward` at each triangle's
    /// centroid.
    fn face(&mut self, triangles: &[[V; 3]], outward: impl Fn(V) -> V, planar: bool) {
        for &[a, b, c] in triangles {
            let centroid = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let out = unit(outward(centroid));
            let corners = if dot(cross(sub(b, a), sub(c, a)), out) >= 0.0 {
                [a, b, c]
            } else {
                [a, c, b]
            };
            let first = self.mesh.vertices.len() as u32;
            for p in corners {
                self.mesh.vertices.push(p);
                self.mesh.normals.push(out);
            }
            self.mesh.triangles.push([first, first + 1, first + 2]);
        }
        self.mesh.face_triangles.push(triangles.len() as u32);
        self.planar.push(planar);
    }

    /// A planar rectangle of four corners in turn.
    fn rectangle(&mut self, corners: [V; 4], normal: V) {
        let [a, b, c, d] = corners;
        self.face(&[[a, b, c], [a, c, d]], |_| normal, true);
    }
}

/// Points around the X axis at `x`: the angles of [`SEGMENTS`] steps,
/// which include the diagonals (32 is a multiple of 8).
const SEGMENTS: usize = 32;

fn angle(k: usize) -> f64 {
    TAU * (k % SEGMENTS) as f64 / SEGMENTS as f64
}

fn on_circle(x: f64, radius: f64, k: usize) -> V {
    [x, radius * angle(k).cos(), radius * angle(k).sin()]
}

/// The point of the square of half side `half` around the X axis at `x`
/// in the direction of step `k`: its corners at the diagonals.
fn on_square(x: f64, half: f64, k: usize) -> V {
    let (sin, cos) = angle(k).sin_cos();
    let reach = half / cos.abs().max(sin.abs());
    [x, reach * cos, reach * sin]
}

/// The planar ring at `x` between two outlines around the X axis.
fn ring(inner: impl Fn(usize) -> V, outer: impl Fn(usize) -> V) -> Vec<[V; 3]> {
    (0..SEGMENTS)
        .flat_map(|k| {
            [
                [inner(k), inner(k + 1), outer(k + 1)],
                [inner(k), outer(k + 1), outer(k)],
            ]
        })
        .collect()
}

/// A cylinder of `radius` around the X axis from `x0` to `x1`.
fn cylinder(x0: f64, x1: f64, radius: f64) -> Vec<[V; 3]> {
    (0..SEGMENTS)
        .flat_map(|k| {
            let (a, b) = (on_circle(x0, radius, k), on_circle(x0, radius, k + 1));
            let (c, d) = (on_circle(x1, radius, k + 1), on_circle(x1, radius, k));
            [[a, b, c], [a, c, d]]
        })
        .collect()
}

/// A tube lying along X (outer radius 10, bore 6, to x = 100) with a
/// square flange 40 x 40, 5 thick, at x = 0 to 5: the flange's outer face
/// (x = 0, facing -X, with the bore through it) is face 0.
fn flanged_tube() -> Builder {
    let mut b = Builder::default();
    let radial = |p: V| [0.0, p[1], p[2]];
    let inward = |p: V| [0.0, -p[1], -p[2]];
    // 0: the flange's outer face, the bore through it.
    b.face(
        &ring(|k| on_circle(0.0, 6.0, k), |k| on_square(0.0, 20.0, k)),
        |_| [-1.0, 0.0, 0.0],
        true,
    );
    // 1: the flange's inner face around the tube.
    b.face(
        &ring(|k| on_circle(5.0, 10.0, k), |k| on_square(5.0, 20.0, k)),
        |_| [1.0, 0.0, 0.0],
        true,
    );
    // 2-5: the flange's narrow sides.
    let h = 20.0;
    b.rectangle(
        [[0.0, -h, -h], [5.0, -h, -h], [5.0, h, -h], [0.0, h, -h]],
        [0.0, 0.0, -1.0],
    );
    b.rectangle(
        [[0.0, -h, h], [0.0, h, h], [5.0, h, h], [5.0, -h, h]],
        [0.0, 0.0, 1.0],
    );
    b.rectangle(
        [[0.0, -h, -h], [0.0, -h, h], [5.0, -h, h], [5.0, -h, -h]],
        [0.0, -1.0, 0.0],
    );
    b.rectangle(
        [[0.0, h, -h], [5.0, h, -h], [5.0, h, h], [0.0, h, h]],
        [0.0, 1.0, 0.0],
    );
    // 6: the tube's outside.
    b.face(&cylinder(5.0, 100.0, 10.0), radial, false);
    // 7: the tube's end, a ring.
    b.face(
        &ring(|k| on_circle(100.0, 6.0, k), |k| on_circle(100.0, 10.0, k)),
        |_| [1.0, 0.0, 0.0],
        true,
    );
    // 8: the bore, through flange and tube.
    b.face(&cylinder(0.0, 100.0, 6.0), inward, false);
    b
}

/// A block from the origin to `size`, its faces in turn -Z, +Z, -Y, +Y,
/// -X, +X.
fn block(size: V) -> Builder {
    let mut b = Builder::default();
    let [x, y, z] = size;
    b.rectangle(
        [[0.0, 0.0, 0.0], [0.0, y, 0.0], [x, y, 0.0], [x, 0.0, 0.0]],
        [0.0, 0.0, -1.0],
    );
    b.rectangle(
        [[0.0, 0.0, z], [x, 0.0, z], [x, y, z], [0.0, y, z]],
        [0.0, 0.0, 1.0],
    );
    b.rectangle(
        [[0.0, 0.0, 0.0], [x, 0.0, 0.0], [x, 0.0, z], [0.0, 0.0, z]],
        [0.0, -1.0, 0.0],
    );
    b.rectangle(
        [[0.0, y, 0.0], [0.0, y, z], [x, y, z], [x, y, 0.0]],
        [0.0, 1.0, 0.0],
    );
    b.rectangle(
        [[0.0, 0.0, 0.0], [0.0, 0.0, z], [0.0, y, z], [0.0, y, 0.0]],
        [-1.0, 0.0, 0.0],
    );
    b.rectangle(
        [[x, 0.0, 0.0], [x, y, 0.0], [x, y, z], [x, 0.0, z]],
        [1.0, 0.0, 0.0],
    );
    b
}

fn near_vector(a: V, b: V) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= TOL)
}

#[test]
fn a_lying_tube_with_a_flange_lies_on_the_flange() {
    let tube = flanged_tube();
    let best = best_bed_face(&tube.mesh, &tube.planar).expect("an orientation");
    assert_eq!(best.face, 0, "{best:?}");
    assert!(near_vector(best.normal, [-1.0, 0.0, 0.0]), "{best:?}");
    // Standing on the flange nothing overhangs; the flange's outer face
    // (the square less the bore) is on the bed, the tube 100 high.
    assert!(best.overhang.abs() <= TOL, "{best:?}");
    assert!(best.contact > 1400.0 && best.contact < 1600.0, "{best:?}");
    assert!((best.height - 100.0).abs() <= TOL, "{best:?}");
    // Without the flange's outer face as a candidate the tube's end wins
    // over the flange's narrow sides: still standing, not lying.
    let mut planar = tube.planar.clone();
    planar[0] = false;
    let next = best_bed_face(&tube.mesh, &planar).expect("an orientation");
    assert_eq!(next.face, 7, "{next:?}");
}

#[test]
fn a_block_lies_on_its_largest_face() {
    // 20 x 10 x 40: the 20 x 40 faces facing -Y and +Y are the largest;
    // alike, the one listed first.
    let b = block([20.0, 10.0, 40.0]);
    let best = best_bed_face(&b.mesh, &b.planar).expect("an orientation");
    assert_eq!(best.face, 2, "{best:?}");
    assert!(near_vector(best.normal, [0.0, -1.0, 0.0]), "{best:?}");
    assert!((best.contact - 800.0).abs() <= TOL, "{best:?}");
    assert!((best.height - 10.0).abs() <= TOL, "{best:?}");
}

#[test]
fn a_cube_keeps_its_bottom_and_curved_faces_are_no_candidates() {
    // Every face alike: the one facing down in the design.
    let cube = block([20.0, 20.0, 20.0]);
    let best = best_bed_face(&cube.mesh, &cube.planar).expect("an orientation");
    assert_eq!(best.face, 0, "{best:?}");
    // No planar face: no orientation (the design's, −Z down).
    let mut curved = cube.planar.clone();
    curved.fill(false);
    assert_eq!(best_bed_face(&cube.mesh, &curved), None);
    assert_eq!(best_bed_face(&FaceMeshes::default(), &[]), None);
}
