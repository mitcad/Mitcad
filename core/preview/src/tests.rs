// SPDX-License-Identifier: MIT
use serde_json::json;

use crate::args::{Background, Fit, RenderArgs, Selection};
use crate::camera::{Aabb, Frame, Projection, SavedView, View, resolve_camera};
use crate::raster::{self, Mesh, Scene, Style};
use crate::{camera::Camera, png};

fn cube_box() -> Aabb {
    Aabb {
        min: [0.0, 0.0, 0.0],
        max: [20.0, 10.0, 5.0],
    }
}

fn framed(view: View, projection: Option<Projection>) -> Camera {
    let frame = Frame {
        bounds: Some(cube_box()),
        explicit: false,
    };
    resolve_camera(&view, projection, frame, 4.0 / 3.0, &[])
        .expect("a camera")
        .0
}

/// A box body as a display mesh: six faces, each its own four vertices.
pub(crate) fn box_mesh(min: [f64; 3], max: [f64; 3], color: [f64; 3]) -> Mesh {
    let mut mesh = Mesh {
        edge_color: raster::EDGE_COLOR,
        ..Mesh::default()
    };
    let p = |x: usize, y: usize, z: usize| {
        [
            if x == 0 { min[0] } else { max[0] },
            if y == 0 { min[1] } else { max[1] },
            if z == 0 { min[2] } else { max[2] },
        ]
    };
    // Each face: its corners counter-clockwise seen from outside, normal.
    let faces: [([[usize; 3]; 4], [f64; 3]); 6] = [
        (
            [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]],
            [0.0, 0.0, -1.0],
        ),
        (
            [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]],
            [0.0, 0.0, 1.0],
        ),
        (
            [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]],
            [0.0, -1.0, 0.0],
        ),
        (
            [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]],
            [0.0, 1.0, 0.0],
        ),
        (
            [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]],
            [-1.0, 0.0, 0.0],
        ),
        (
            [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]],
            [1.0, 0.0, 0.0],
        ),
    ];
    for (corners, normal) in faces {
        let first = mesh.vertices.len() as u32;
        for c in corners {
            mesh.vertices.push(p(c[0], c[1], c[2]));
            mesh.normals.push(normal);
        }
        mesh.triangles.push([first, first + 1, first + 2]);
        mesh.triangles.push([first, first + 2, first + 3]);
        mesh.face_triangles.push(2);
        mesh.face_colors.push(color);
    }
    mesh
}

fn scene() -> Scene {
    Scene {
        meshes: vec![box_mesh(
            [0.0, 0.0, 0.0],
            [20.0, 10.0, 5.0],
            raster::BODY_COLOR,
        )],
        ..Scene::default()
    }
}

fn picture(camera: &Camera, size: [u32; 2], background: Background) -> raster::Picture {
    let scene = scene();
    let prepared = raster::prepare(&scene);
    raster::render(
        &prepared,
        camera,
        size,
        &Style {
            background,
            edges: true,
        },
    )
}

#[test]
fn home_is_the_isometric_view_from_the_front_right() {
    let camera = framed(View::default(), None);
    let toward_eye = mitcad_vec3::sub(camera.eye, camera.target);
    assert!(toward_eye[0] > 0.0 && toward_eye[1] < 0.0 && toward_eye[2] > 0.0);
    assert!((toward_eye[0] + toward_eye[1]).abs() < 1e-9 * toward_eye[0].abs());
    assert_eq!(camera.projection, Projection::Perspective);
    assert_eq!(camera.fov, 45.0);
    // Up is upward and square to the line of sight.
    assert!(camera.up[2] > 0.0);
    assert!(mitcad_vec3::dot(camera.up, toward_eye).abs() < 1e-12);
}

#[test]
fn standard_views_look_along_their_axes() {
    let front = framed(View::Named("front".into()), Some(Projection::Orthographic));
    assert_eq!(front.direction(), [0.0, 1.0, 0.0]);
    assert_eq!(front.up, [0.0, 0.0, 1.0]);
    let top = framed(View::Named("Top".into()), None);
    assert_eq!(top.direction(), [0.0, 0.0, -1.0]);
    assert_eq!(top.up, [0.0, 1.0, 0.0]);
    let right = framed(View::Named("right".into()), None);
    assert_eq!(right.direction(), [-1.0, 0.0, 0.0]);
    // Azimuth 90 is from the right.
    let angles = framed(
        View::Angles {
            azimuth: 90.0,
            elevation: 0.0,
        },
        None,
    );
    let d = angles.direction();
    assert!((d[0] + 1.0).abs() < 1e-12 && d[1].abs() < 1e-12 && d[2].abs() < 1e-12);
}

#[test]
fn a_framed_box_is_in_the_picture() {
    for projection in [Projection::Perspective, Projection::Orthographic] {
        let camera = framed(View::default(), Some(projection));
        let forward = camera.direction();
        let right = mitcad_vec3::cross(forward, camera.up);
        let half = (camera.fov.to_radians() / 2.0).tan();
        for c in cube_box().corners() {
            let q = mitcad_vec3::sub(c, camera.eye);
            let (x, y, z) = (
                mitcad_vec3::dot(q, right),
                mitcad_vec3::dot(q, camera.up),
                mitcad_vec3::dot(q, forward),
            );
            assert!(z > 0.0, "{projection:?}: the corner is in front of the eye");
            let (sx, sy) = match projection {
                Projection::Perspective => (x / (z * half), y / (z * half)),
                Projection::Orthographic => (x / (camera.height / 2.0), y / (camera.height / 2.0)),
            };
            assert!(sy.abs() <= 0.86, "{projection:?}: {sy}");
            assert!(sx.abs() <= 0.86 * 4.0 / 3.0, "{projection:?}: {sx}");
        }
    }
}

#[test]
fn an_explicit_camera_is_kept_and_the_answer_reproduces_it() {
    let camera = framed(View::default(), None);
    let answer = camera.to_json();
    let args = RenderArgs::from_json(&json!({"document": "D1", "view": answer})).unwrap();
    let frame = Frame {
        bounds: Some(Aabb {
            min: [-500.0; 3],
            max: [500.0; 3],
        }),
        explicit: false,
    };
    let (again, warnings) =
        resolve_camera(&args.view, args.projection, frame, args.aspect(), &[]).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(again, camera);
    // Orthographic too, with its height.
    let ortho = framed(View::Named("iso".into()), Some(Projection::Orthographic));
    let args = RenderArgs::from_json(&json!({"view": ortho.to_json()})).unwrap();
    let (again, _) = resolve_camera(&args.view, args.projection, frame, 4.0 / 3.0, &[]).unwrap();
    assert_eq!(again, ortho);
}

#[test]
fn saved_views_and_home() {
    let saved = vec![SavedView {
        name: "Home".into(),
        eye: [0.0, -100.0, 0.0],
        target: [0.0; 3],
        up: [0.0, 0.0, 1.0],
        perspective: false,
        height: 40.0,
    }];
    let frame = Frame {
        bounds: Some(cube_box()),
        explicit: false,
    };
    let (home, _) = resolve_camera(&View::default(), None, frame, 1.0, &saved).unwrap();
    assert_eq!(home.eye, [0.0, -100.0, 0.0]);
    assert_eq!(home.projection, Projection::Orthographic);
    assert_eq!(home.height, 40.0);
    // By its name in any case; fit given explicitly frames it.
    let explicit = Frame {
        explicit: true,
        ..frame
    };
    let (fitted, _) =
        resolve_camera(&View::Named("home".into()), None, explicit, 1.0, &saved).unwrap();
    assert_eq!(fitted.direction(), [0.0, 1.0, 0.0]);
    assert_ne!(fitted.height, 40.0);
    let error =
        resolve_camera(&View::Named("Detail".into()), None, frame, 1.0, &saved).unwrap_err();
    assert!(
        error.contains("no view Detail") && error.contains("Home"),
        "{error}"
    );
}

#[test]
fn nothing_to_frame_shows_the_origin_with_a_warning() {
    let frame = Frame {
        bounds: None,
        explicit: false,
    };
    let (camera, warnings) = resolve_camera(&View::default(), None, frame, 1.0, &[]).unwrap();
    assert_eq!(camera.target, [0.0; 3]);
    assert!((camera.height - 100.0).abs() < 1e-9, "{}", camera.height);
    assert_eq!(warnings.len(), 1);
}

#[test]
fn arguments_are_read_and_refused() {
    let args = RenderArgs::from_json(&json!({
        "document": "D1",
        "view": {"azimuth": 30, "elevation": 20},
        "projection": "orthographic",
        "fit": ["F2.b0", "C1"],
        "bodies": "all",
        "hidden": ["F7"],
        "size": [320, 200],
        "edges": false,
        "background": "#102030",
        "quality": "fast"
    }))
    .unwrap();
    assert_eq!(
        args.view,
        View::Angles {
            azimuth: 30.0,
            elevation: 20.0
        }
    );
    assert_eq!(args.projection, Some(Projection::Orthographic));
    assert_eq!(args.fit, Fit::Items(vec!["F2.b0".into(), "C1".into()]));
    assert_eq!(args.bodies, Selection::All);
    assert_eq!(args.hidden, vec!["F7".to_owned()]);
    assert_eq!(args.size, [320, 200]);
    assert!(!args.edges);
    assert_eq!(args.background, Background::Color([0x10, 0x20, 0x30]));
    let fit = RenderArgs::from_json(&json!({"fit": {"min": [0, 0, 0], "max": [1, 2, 3]}})).unwrap();
    assert_eq!(
        fit.fit,
        Fit::Box(Aabb {
            min: [0.0; 3],
            max: [1.0, 2.0, 3.0]
        })
    );
    for bad in [
        json!({"size": [8, 8]}),
        json!({"view": 3}),
        json!({"view": {"eye": [0, 0], "target": [0, 0, 0]}}),
        json!({"fit": {"min": [1, 0, 0], "max": [0, 0, 0]}}),
        json!({"background": "#zzzzzz"}),
        json!({"bodies": []}),
        json!({"projection": "fisheye"}),
    ] {
        assert!(RenderArgs::from_json(&bad).is_err(), "{bad}");
    }
}

#[test]
fn a_box_is_shaded_edged_and_drawn_alike_twice() {
    let camera = framed(View::default(), None);
    let one = picture(&camera, [160, 120], Background::Theme);
    let two = picture(&camera, [160, 120], Background::Theme);
    assert_eq!(one, two);
    assert_eq!((one.width, one.height, one.channels), (160, 120, 3));
    // The middle shows the body, the corner the background's top.
    let at = |x: usize, y: usize| {
        let i = (y * 160 + x) * 3;
        [one.pixels[i], one.pixels[i + 1], one.pixels[i + 2]]
    };
    assert_eq!(at(0, 0), raster::THEME_TOP);
    assert_ne!(at(80, 60), at(0, 60));
    // Three faces seen, lit differently; twelve edges, nine of them seen.
    assert_eq!(one.triangles, 6);
    assert!(one.edges >= 9, "{}", one.edges);
    assert!(one.pixels.chunks(3).any(|p| p == raster::EDGE_COLOR));
    // The PNG holds those pixels.
    let encoded = png::encode(one.width, one.height, one.channels, &one.pixels);
    assert_eq!(png::header(&encoded), Some((160, 120, 2)));
    let (w, h, channels, pixels) = png::decode(&encoded).unwrap();
    assert_eq!((w, h, channels), (160, 120, 3));
    assert_eq!(pixels, one.pixels);
    // Another view, another picture.
    let other = picture(
        &framed(View::Named("top".into()), None),
        [160, 120],
        Background::Theme,
    );
    assert_ne!(other.pixels, one.pixels);
}

#[test]
fn hidden_edges_stay_hidden() {
    // From the front, orthographic: the back face's edges lie behind the
    // front face; without the front face nothing of them may show inside.
    let camera = framed(View::Named("front".into()), Some(Projection::Orthographic));
    let mut scene = scene();
    // Edges in a colour of their own.
    scene.meshes[0].edge_color = [255, 0, 0];
    let prepared = raster::prepare(&scene);
    let picture = raster::render(
        &prepared,
        &camera,
        [200, 100],
        &Style {
            background: Background::Theme,
            edges: true,
        },
    );
    let red = |x: usize, y: usize| {
        let i = (y * 200 + x) * 3;
        picture.pixels[i] > 200 && picture.pixels[i + 1] < 80
    };
    // The middle of the front face has no edge; its outline has.
    assert!(!red(100, 50));
    assert!((0..200).any(|x| red(x, 50)));
}

#[test]
fn a_transparent_background_keeps_alpha() {
    let camera = framed(View::default(), None);
    let picture = picture(&camera, [64, 48], Background::Transparent);
    assert_eq!(picture.channels, 4);
    assert_eq!(&picture.pixels[0..4], &[0, 0, 0, 0]);
    let middle = ((24 * 64) + 32) * 4;
    assert_eq!(picture.pixels[middle + 3], 255);
    let encoded = png::encode(64, 48, 4, &picture.pixels);
    assert_eq!(png::header(&encoded), Some((64, 48, 6)));
}

#[test]
fn a_camera_inside_the_box_draws_without_failing() {
    let camera = Camera {
        eye: [10.0, 5.0, 2.5],
        target: [10.0, 50.0, 2.5],
        up: [0.0, 0.0, 1.0],
        projection: Projection::Perspective,
        height: 10.0,
        fov: 45.0,
    };
    let picture = picture(&camera, [32, 32], Background::Theme);
    assert_eq!(picture.pixels.len(), 32 * 32 * 3);
}

/// A cylinder's side as one face whose seam has two copies (the last
/// column at 2 pi, off by rounding), with a pole-like sliver: its edges
/// are the two circles only, and its folds are inside.
#[test]
fn seams_and_slivers_are_no_edges() {
    let n = 24usize;
    let mut mesh = Mesh {
        edge_color: raster::EDGE_COLOR,
        ..Mesh::default()
    };
    for k in 0..=n {
        let angle = std::f64::consts::TAU * k as f64 / n as f64;
        let (s, c) = angle.sin_cos();
        for z in [0.0, 10.0] {
            mesh.vertices.push([5.0 * c, 5.0 * s, z]);
            mesh.normals.push([c, s, 0.0]);
        }
    }
    for k in 0..n as u32 {
        let (a, b, c, d) = (2 * k, 2 * k + 2, 2 * k + 3, 2 * k + 1);
        mesh.triangles.push([a, b, c]);
        mesh.triangles.push([a, c, d]);
    }
    // A sliver with two corners at one point.
    mesh.triangles.push([1, 3, 3]);
    mesh.face_triangles.push(mesh.triangles.len() as u32);
    mesh.face_colors.push(raster::BODY_COLOR);
    let edges = raster::mesh_edges(&mesh);
    assert_eq!(edges.segments.len(), 2 * n);
    for [a, b] in &edges.segments {
        assert_eq!(a[2], b[2], "a circle's segment, not one along the side");
    }
    // Every side along the cylinder (the seam's too) is a fold.
    assert_eq!(edges.folds.len(), 2 * n);
}
