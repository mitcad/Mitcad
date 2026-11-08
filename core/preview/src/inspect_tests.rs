// SPDX-License-Identifier: MIT
//! The inspection options and views (mitcad#1487): their arguments, a
//! section's caps and highlighted edges.

use serde_json::json;

use crate::args::{Background, RenderArgs};
use crate::camera::{Aabb, Camera, Frame, Projection, View, resolve_camera};
use crate::inspect::{EntityKind, Highlight, Section};
use crate::raster::{self, Inspection, Mesh, Scene, Style};
use crate::tests::box_mesh;
use crate::views;

const RED: [f64; 3] = [0.9, 0.1, 0.1];

/// A 20 mm cube at the origin, hollow: a 10 mm cavity in its middle (the
/// cavity's faces turned to face into it, out of the material).
fn hollow_cube() -> Mesh {
    let mut mesh = box_mesh([0.0; 3], [20.0; 3], raster::BODY_COLOR);
    let inner = box_mesh([5.0; 3], [15.0; 3], raster::BODY_COLOR);
    let first = mesh.vertices.len() as u32;
    mesh.vertices.extend(&inner.vertices);
    mesh.normals
        .extend(inner.normals.iter().map(|n| n.map(|c| -c)));
    mesh.triangles.extend(
        inner
            .triangles
            .iter()
            .map(|t| [t[0] + first, t[2] + first, t[1] + first]),
    );
    mesh.face_triangles.extend(&inner.face_triangles);
    mesh.face_colors.extend(&inner.face_colors);
    mesh
}

/// From the front (looking along +Y), orthographic, framed on the cube.
fn front() -> Camera {
    let frame = Frame {
        bounds: Some(Aabb {
            min: [0.0; 3],
            max: [20.0; 3],
        }),
        explicit: false,
    };
    resolve_camera(
        &View::Named("front".into()),
        Some(Projection::Orthographic),
        frame,
        1.0,
        &[],
    )
    .expect("a camera")
    .0
}

fn draw(mesh: Mesh, inspection: &Inspection) -> raster::Picture {
    let scene = Scene {
        meshes: vec![mesh],
        ..Scene::default()
    };
    let prepared = raster::prepare(&scene);
    raster::render_with(
        &prepared,
        &front(),
        [200, 200],
        &Style {
            background: Background::Color([255, 255, 255]),
            edges: true,
        },
        inspection,
    )
}

fn at(picture: &raster::Picture, x: usize, y: usize) -> [u8; 3] {
    let i = (y * picture.width as usize + x) * picture.channels;
    [
        picture.pixels[i],
        picture.pixels[i + 1],
        picture.pixels[i + 2],
    ]
}

fn reddish(p: [u8; 3]) -> bool {
    i32::from(p[0]) > i32::from(p[2]) + 60
}

/// Cut in the middle, the half toward the eye (at -Y) cut away.
fn middle() -> Inspection {
    Inspection {
        section: Some(Section {
            origin: [10.0, 10.0, 10.0],
            normal: [0.0, -1.0, 0.0],
        }),
        accent_lines: Vec::new(),
    }
}

#[test]
fn a_section_caps_a_solid_and_shows_a_cavity() {
    let capped = |mut mesh: Mesh| {
        mesh.cap_color = Some(RED);
        mesh
    };
    // Whole: the front face, in the body's colour.
    let whole = draw(capped(hollow_cube()), &Inspection::default());
    assert!(!reddish(at(&whole, 100, 100)));
    assert_eq!(
        whole,
        raster::render(
            &raster::prepare(&Scene {
                meshes: vec![capped(hollow_cube())],
                ..Scene::default()
            }),
            &front(),
            [200, 200],
            &Style {
                background: Background::Color([255, 255, 255]),
                edges: true
            },
        )
    );
    // A solid cut: its cap everywhere inside the outline.
    let solid = draw(
        capped(box_mesh([0.0; 3], [20.0; 3], raster::BODY_COLOR)),
        &middle(),
    );
    for (x, y) in [(100, 100), (40, 40), (160, 150)] {
        assert!(
            reddish(at(&solid, x, y)),
            "{x}, {y}: {:?}",
            at(&solid, x, y)
        );
    }
    // Hatched: some pixels darker than the cap's base.
    let darkest = (60..140).map(|x| at(&solid, x, 100)[0]).min().unwrap();
    let lightest = (60..140).map(|x| at(&solid, x, 100)[0]).max().unwrap();
    assert!(lightest > darkest + 30, "{darkest} {lightest}");
    // Hollow: the cavity's far wall in the middle, the cap around it.
    let hollow = draw(capped(hollow_cube()), &middle());
    assert!(
        !reddish(at(&hollow, 100, 100)),
        "{:?}",
        at(&hollow, 100, 100)
    );
    assert!(reddish(at(&hollow, 30, 100)), "{:?}", at(&hollow, 30, 100));
    assert!(
        reddish(at(&hollow, 100, 170)),
        "{:?}",
        at(&hollow, 100, 170)
    );
    // The same arguments, the same pixels.
    assert_eq!(hollow, draw(capped(hollow_cube()), &middle()));
    // Everything cut away: the background only.
    let gone = draw(
        capped(hollow_cube()),
        &Inspection {
            section: Some(Section {
                origin: [0.0, -1.0, 0.0],
                normal: [0.0, 1.0, 0.0],
            }),
            accent_lines: Vec::new(),
        },
    );
    assert!(gone.pixels.iter().all(|&c| c == 255));
}

#[test]
fn a_cut_sheet_and_a_transparent_body_are_not_capped() {
    for (sheet, transparent) in [(true, false), (false, true)] {
        let mut mesh = box_mesh([0.0; 3], [20.0; 3], raster::BODY_COLOR);
        mesh.cap_color = Some(RED);
        mesh.double_sided = sheet;
        mesh.transparent = transparent;
        let cut = draw(mesh, &middle());
        assert!(!reddish(at(&cut, 100, 100)), "{sheet} {transparent}");
    }
}

#[test]
fn highlighted_edges_are_drawn_over_everything() {
    // The back face's bottom edge, behind the box from the front.
    let line = vec![[0.0, 20.0, 10.0], [20.0, 20.0, 10.0]];
    let picture = draw(
        box_mesh([0.0; 3], [20.0; 3], raster::BODY_COLOR),
        &Inspection {
            section: None,
            accent_lines: vec![line],
        },
    );
    assert_eq!(at(&picture, 100, 100), raster::ACCENT);
    assert_ne!(at(&picture, 100, 60), raster::ACCENT);
}

#[test]
fn inspection_arguments_are_read_and_refused() {
    let args = RenderArgs::from_json(&json!({
        "colors": {"F2.b0": "#a52a2a", "C1": "#3060C0"},
        "highlight": ["F2.b0", {"body": "F2.b0", "face": "F2:end(r{c5})"},
                      {"kind": "edge", "owner": "F2.b0", "name": "E{a|b}"}, {"body": "Body1"}],
        "section": {"origin": [1, 2, 3], "normal": [0, 0, 2]},
        "explode": 1.5
    }))
    .unwrap();
    let inspect = &args.inspect;
    assert_eq!(
        inspect.colors,
        [
            ("C1".to_owned(), [0x30, 0x60, 0xc0]),
            ("F2.b0".to_owned(), [0xa5, 0x2a, 0x2a])
        ]
    );
    assert_eq!(inspect.highlight[0], Highlight::Text("F2.b0".into()));
    assert_eq!(
        inspect.highlight[1],
        Highlight::Entity {
            kind: EntityKind::Face,
            body: Some("F2.b0".into()),
            occurrence: None,
            name: "F2:end(r{c5})".into()
        }
    );
    assert!(
        matches!(&inspect.highlight[2], Highlight::Entity { kind: EntityKind::Edge, name, .. } if name == "E{a|b}")
    );
    assert!(matches!(
        &inspect.highlight[3],
        Highlight::Entity {
            kind: EntityKind::Body,
            ..
        }
    ));
    assert_eq!(inspect.section.unwrap().normal, [0.0, 0.0, 1.0]);
    assert_eq!(inspect.explode, 1.5);
    assert_eq!(
        inspect.given(),
        ["colors", "highlight", "section", "explode"]
    );
    assert!(
        RenderArgs::from_json(&json!({}))
            .unwrap()
            .inspect
            .given()
            .is_empty()
    );
    for bad in [
        json!({"colors": {"F2.b0": "red"}}),
        json!({"colors": ["F2.b0"]}),
        json!({"highlight": []}),
        json!({"highlight": [3]}),
        json!({"highlight": [{"face": "F2:end(r{c5})", "edge": "E{a|b}"}]}),
        json!({"highlight": [{"kind": "vertex", "owner": "F2.b0", "name": "V1"}]}),
        json!({"section": {"origin": [0, 0, 0], "normal": [0, 0, 0]}}),
        json!({"section": {"origin": [0, 0], "normal": [0, 0, 1]}}),
        json!({"section": {"origin": [0, 0, 0], "normal": [0, 0, 1], "flip": true}}),
        json!({"explode": -0.5}),
        json!({"explode": 11}),
    ] {
        assert!(RenderArgs::from_json(&bad).is_err(), "{bad}");
    }
}

#[test]
fn views_merge_into_single_calls() {
    let arguments = json!({
        "document": "D1", "size": [64, 48], "edges": false,
        "views": ["iso", {"view": "top", "size": [32, 32], "section": {"origin": [0, 0, 1], "normal": [0, 0, 1]}},
                  {"eye": [1, 2, 3], "target": [0, 0, 0], "up": [0, 0, 1]}]
    });
    let each = views::view_arguments(&arguments).unwrap().unwrap();
    assert_eq!(each.len(), 3);
    assert_eq!(
        each[0],
        json!({"document": "D1", "size": [64, 48], "edges": false, "view": "iso"})
    );
    assert_eq!(each[1]["size"], json!([32, 32]));
    assert_eq!(each[1]["view"], "top");
    assert!(each[1].get("views").is_none());
    assert_eq!(each[2]["view"]["eye"], json!([1, 2, 3]));
    assert!(
        views::view_arguments(&json!({"view": "iso"}))
            .unwrap()
            .is_none()
    );
    for bad in [
        json!({"views": []}),
        json!({"views": vec!["iso"; 9]}),
        json!({"views": ["iso"], "path": "a.png"}),
        json!({"views": [{"view": "top", "colors": {}}]}),
        json!({"views": [3]}),
    ] {
        assert!(views::view_arguments(&bad).is_err(), "{bad}");
    }
}

#[test]
fn a_section_cuts_an_overlay_too() {
    // An overlay over the cube's front face (y = 0, its normal -Y): from the
    // front it shows, cut away with the front half it does not.
    let green = [0, 200, 0];
    let overlay = crate::overlay::OverlayMesh {
        triangles: vec![
            [[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [20.0, 0.0, 20.0]],
            [[0.0, 0.0, 0.0], [20.0, 0.0, 20.0], [0.0, 0.0, 20.0]],
        ],
        color: green,
        lift: 0.05,
    };
    let picture = |inspection: &Inspection| {
        let scene = Scene {
            meshes: vec![box_mesh([0.0; 3], [20.0; 3], raster::BODY_COLOR)],
            overlays: vec![overlay.clone()],
        };
        raster::render_with(
            &raster::prepare(&scene),
            &front(),
            [200, 200],
            &Style {
                background: Background::Color([255, 255, 255]),
                edges: false,
            },
            inspection,
        )
    };
    assert_eq!(at(&picture(&Inspection::default()), 100, 100), green);
    assert_ne!(at(&picture(&middle()), 100, 100), green);
}
