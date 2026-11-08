// SPDX-License-Identifier: MIT
//! `model_render`'s overlay (mitcad#1488): the argument read and refused,
//! and the triangles drawn flat in their color over the faces they lie on,
//! never through the faces in front of them.

use serde_json::json;

use crate::args::{Background, RenderArgs};
use crate::camera::{Aabb, Camera, Frame, Projection, View, resolve_camera};
use crate::overlay::{self, Overlay, OverlayKind, OverlayMesh};
use crate::raster::{self, Mesh, Scene, Style};

const SIZE: [u32; 2] = [200, 100];
const RED: [u8; 3] = [200, 30, 40];

/// The box from `min` to `max`: its faces' corners counter-clockwise seen
/// from outside, and their outward normals.
fn faces(min: [f64; 3], max: [f64; 3]) -> Vec<([[f64; 3]; 4], [f64; 3])> {
    let p = |x: usize, y: usize, z: usize| {
        [
            if x == 0 { min[0] } else { max[0] },
            if y == 0 { min[1] } else { max[1] },
            if z == 0 { min[2] } else { max[2] },
        ]
    };
    vec![
        (
            [p(0, 0, 0), p(0, 1, 0), p(1, 1, 0), p(1, 0, 0)],
            [0.0, 0.0, -1.0],
        ),
        (
            [p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1)],
            [0.0, 0.0, 1.0],
        ),
        (
            [p(0, 0, 0), p(1, 0, 0), p(1, 0, 1), p(0, 0, 1)],
            [0.0, -1.0, 0.0],
        ),
        (
            [p(0, 1, 0), p(0, 1, 1), p(1, 1, 1), p(1, 1, 0)],
            [0.0, 1.0, 0.0],
        ),
        (
            [p(0, 0, 0), p(0, 0, 1), p(0, 1, 1), p(0, 1, 0)],
            [-1.0, 0.0, 0.0],
        ),
        (
            [p(1, 0, 0), p(1, 1, 0), p(1, 1, 1), p(1, 0, 1)],
            [1.0, 0.0, 0.0],
        ),
    ]
}

fn box_mesh(min: [f64; 3], max: [f64; 3]) -> Mesh {
    let mut mesh = Mesh {
        edge_color: raster::EDGE_COLOR,
        ..Mesh::default()
    };
    for (corners, normal) in faces(min, max) {
        let first = mesh.vertices.len() as u32;
        for c in corners {
            mesh.vertices.push(c);
            mesh.normals.push(normal);
        }
        mesh.triangles.push([first, first + 1, first + 2]);
        mesh.triangles.push([first, first + 2, first + 3]);
        mesh.face_triangles.push(2);
        mesh.face_colors.push(raster::BODY_COLOR);
    }
    mesh
}

/// The two triangles of a quad, as an overlay holds them.
fn quad_triangles(c: [[f64; 3]; 4]) -> Vec<[[f64; 3]; 3]> {
    vec![[c[0], c[1], c[2]], [c[0], c[2], c[3]]]
}

const MIN: [f64; 3] = [0.0, 0.0, 0.0];
const MAX: [f64; 3] = [20.0, 10.0, 5.0];

/// From the front (looking along +Y), orthographic.
fn front() -> Camera {
    let frame = Frame {
        bounds: Some(Aabb { min: MIN, max: MAX }),
        explicit: false,
    };
    let aspect = f64::from(SIZE[0]) / f64::from(SIZE[1]);
    resolve_camera(
        &View::Named("front".into()),
        Some(Projection::Orthographic),
        frame,
        aspect,
        &[],
    )
    .expect("a camera")
    .0
}

/// The box with these overlays, from the front: the color of the pixel
/// where the point `(x, z)` of the front face is.
fn drawn(overlays: Vec<OverlayMesh>, points: &[(f64, f64)], edges: bool) -> Vec<[u8; 3]> {
    let camera = front();
    let scene = Scene {
        meshes: vec![box_mesh(MIN, MAX)],
        overlays,
    };
    let prepared = raster::prepare(&scene);
    let picture = raster::render(
        &prepared,
        &camera,
        SIZE,
        &Style {
            background: Background::Theme,
            edges,
        },
    );
    let scale = f64::from(SIZE[1]) / camera.height;
    points
        .iter()
        .map(|&(x, z)| {
            let column =
                (f64::from(SIZE[0]) / 2.0 + (x - camera.target[0]) * scale).floor() as usize;
            let row = (f64::from(SIZE[1]) / 2.0 - (z - camera.target[2]) * scale).floor() as usize;
            let i = (row * SIZE[0] as usize + column) * 3;
            [
                picture.pixels[i],
                picture.pixels[i + 1],
                picture.pixels[i + 2],
            ]
        })
        .collect()
}

#[test]
fn overlay_arguments_are_read_and_refused() {
    assert_eq!(RenderArgs::from_json(&json!({})).unwrap().overlay, None);
    let args = RenderArgs::from_json(&json!({"overlay": "overhang"})).unwrap();
    assert_eq!(
        args.overlay,
        Some(Overlay {
            kind: OverlayKind::Overhang {
                angle: None,
                up: None
            },
            color: overlay::OVERHANG_COLOR,
        })
    );
    let args = RenderArgs::from_json(&json!({"overlay": {"type": "overhang", "angle": 60,
        "up": [0, 1, 1], "color": "#00FF00"}}))
    .unwrap();
    assert_eq!(
        args.overlay,
        Some(Overlay {
            kind: OverlayKind::Overhang {
                angle: Some(60.0),
                up: Some([0.0, 1.0, 1.0])
            },
            color: [0, 255, 0],
        })
    );
    let args =
        RenderArgs::from_json(&json!({"overlay": {"type": "thickness", "minimum": 1.2}})).unwrap();
    assert_eq!(
        args.overlay.unwrap().kind,
        OverlayKind::Thickness { minimum: Some(1.2) }
    );
    let args = RenderArgs::from_json(&json!({"overlay": "draft"})).unwrap();
    let draft = args.overlay.unwrap();
    assert_eq!(
        draft.kind,
        OverlayKind::Draft {
            pull: [0.0, 0.0, 1.0],
            angle: overlay::DEFAULT_DRAFT_ANGLE
        }
    );
    assert_eq!(draft.name(), "draft");
    assert_eq!(overlay::color_text(draft.color), "#d98a00");
    let args = RenderArgs::from_json(&json!({"overlay": {"type": "failed"}})).unwrap();
    assert_eq!(args.overlay.unwrap().kind, OverlayKind::Failed);
    for bad in [
        json!({"overlay": "curvature"}),
        json!({"overlay": 3}),
        json!({"overlay": {"angle": 50}}),
        json!({"overlay": {"type": "thickness", "angle": 50}}),
        json!({"overlay": {"type": "overhang", "up": [0, 0, 0]}}),
        json!({"overlay": {"type": "overhang", "angle": "steep"}}),
        json!({"overlay": {"type": "draft", "angle": 90}}),
        json!({"overlay": {"type": "draft", "pull": [1, 0]}}),
        json!({"overlay": {"type": "failed", "color": "red"}}),
    ] {
        assert!(RenderArgs::from_json(&bad).is_err(), "{bad}");
    }
}

#[test]
fn an_overlay_is_drawn_flat_over_its_faces_only() {
    let points = [(5.0, 2.5), (15.0, 2.5)];
    let plain = drawn(Vec::new(), &points, false);
    assert_ne!(plain[0], RED);
    // The front face's left half overlaid: its pixels take the overlay's
    // color exactly (unlit), the right half keeps the body's.
    let front_face = faces(MIN, MAX)[2].0;
    let left = [
        front_face[0],
        [10.0, 0.0, 0.0],
        [10.0, 0.0, 5.0],
        front_face[3],
    ];
    let overlaid = OverlayMesh {
        triangles: quad_triangles(left),
        color: RED,
        lift: 0.05,
    };
    let seen = drawn(vec![overlaid.clone()], &points, false);
    assert_eq!(seen[0], RED);
    assert_eq!(seen[1], plain[1]);
    assert_eq!(drawn(vec![overlaid.clone()], &points, false), seen);
    // With the edges: the same inside the face.
    let edged = drawn(vec![overlaid], &points, true);
    assert_eq!(edged[0], RED);
    // The back face overlaid: hidden behind the front face.
    let back = OverlayMesh {
        triangles: quad_triangles(faces(MIN, MAX)[3].0),
        color: RED,
        lift: 0.05,
    };
    assert_eq!(drawn(vec![back], &points, false), plain);
}
