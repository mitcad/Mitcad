// SPDX-License-Identifier: MIT
//! Bodies coloured as a whole in STEP files (mitcad#2423), with the mock
//! kernel, in the colouring pattern of a reporter's circuit board export:
//! each part's solid carries its own colour (the board green, a
//! connector tan) and most of the connector's faces override it (grey),
//! the parts placed by sub-assemblies, one of them a part without
//! geometry. The view draws appearances only, so each body's colour from
//! the file becomes the body's appearance, flat and as components, in the
//! import's one undo step; the faces keep theirs (mitcad#2313).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::exchange::{
    FaceColor, ImportAssembly, ImportBody, ImportInstance, ImportPart, ImportStructure,
    ImportedFile, ReadOptions,
};
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::{MockKernel, MockShape};
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

/// The colours as the file gives them (sRGB), and their appearances' names.
const GREEN: [f64; 3] = [0.0, 0.501961, 0.0];
const TAN: [f64; 3] = [0.768627, 0.749004, 0.647044];
const GREY: [f64; 3] = [0.6, 0.6, 0.6];
const GREEN_NAME: &str = "Imported color 0 128 0";
const TAN_NAME: &str = "Imported color 196 191 165";
const GREY_NAME: &str = "Imported color 153 153 153";

const PATH: &str = "/models/board.step";

/// The board: green as a whole, no faces of their own colour.
fn board() -> ImportBody<MockShape> {
    ImportBody {
        name: Some("Board".to_owned()),
        color: Some(GREEN),
        shape: MockShape::imported("board", 6),
        face_colors: Vec::new(),
    }
}

/// The connector: tan as a whole, its faces 0 to 3 grey (most of them, as
/// in the reported file), faces 4 and 5 in the body's tan.
fn connector() -> ImportBody<MockShape> {
    ImportBody {
        name: Some("Connector".to_owned()),
        color: Some(TAN),
        shape: MockShape::imported("connector", 6),
        face_colors: (0..4).map(|face| FaceColor { face, color: GREY }).collect(),
    }
}

/// A part the file places without geometry (a curve set of one point).
fn wireframe() -> ImportBody<MockShape> {
    ImportBody {
        name: None,
        color: None,
        shape: MockShape::imported("wireframe", 0),
        face_colors: Vec::new(),
    }
}

/// Where the connector's sub-assembly places it: turned upright and moved
/// onto the board.
fn upright() -> Transform {
    Transform {
        linear: [[-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
        translation: [130.0, 127.8, 4.0],
    }
}

fn instance(part: usize, transform: Transform) -> ImportInstance {
    ImportInstance {
        part,
        name: None,
        transform,
    }
}

fn part(
    name: &str,
    bodies: Vec<ImportBody<MockShape>>,
    instances: Vec<ImportInstance>,
) -> ImportPart<MockShape> {
    ImportPart {
        name: Some(name.to_owned()),
        bodies,
        instances,
    }
}

/// The file as the kernel reads it: its assembly (the board's part; the
/// designator placing the connector's sub-assembly, which places the
/// connector and the wireframe part) and, flattened, its bodies.
fn design() -> Document<MockKernel> {
    let doc = Document::new(MockKernel::default());
    let assembly = ImportAssembly {
        parts: vec![
            part("Board", vec![board()], Vec::new()),
            part(
                "Designator",
                Vec::new(),
                vec![instance(2, Transform::IDENTITY)],
            ),
            part(
                "Connector assembly",
                Vec::new(),
                vec![instance(3, upright()), instance(4, upright())],
            ),
            part("Connector", vec![connector()], Vec::new()),
            part("Wireframe", vec![wireframe()], Vec::new()),
        ],
        roots: vec![
            instance(0, Transform::translation([0.0, 0.0, -0.41148])),
            instance(1, Transform::IDENTITY),
        ],
    };
    doc.kernel()
        .assemblies
        .borrow_mut()
        .insert(PATH.to_owned(), assembly);
    doc.kernel()
        .files
        .borrow_mut()
        .insert(PATH.to_owned(), vec![board(), connector(), wireframe()]);
    doc
}

/// Each body's appearance as the `bodies` query gives it, by body name:
/// what the view draws (the appearance's name and colour; none without).
fn looks(doc: &Document<MockKernel>) -> BTreeMap<String, Option<(String, [f64; 3])>> {
    let answer: Value =
        serde_json::from_str(&doc.query(&json!({"query": "bodies"}).to_string()).unwrap()).unwrap();
    answer
        .as_array()
        .expect("the bodies")
        .iter()
        .map(|body| {
            let look = body["appearance"].as_str().map(|id| {
                let appearance = doc.appearance(id).expect("the body's appearance");
                (appearance.name.clone(), appearance.display_color())
            });
            (body["name"].as_str().unwrap().to_owned(), look)
        })
        .collect()
}

/// The faces of a body with appearances of their own, by the
/// appearance's name.
fn painted_faces(doc: &Document<MockKernel>, body: BodyUid) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for face in doc.face_appearances(body) {
        let name = doc.appearance(&face.appearance).unwrap().name.clone();
        *out.entry(name).or_insert(0) += 1;
    }
    out
}

fn body_named(doc: &Document<MockKernel>, name: &str) -> BodyUid {
    doc.bodies()
        .into_iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("a body {name}"))
        .uid
}

/// The board green, the connector tan with its grey faces: three
/// appearances, one per colour.
fn check_colors(doc: &Document<MockKernel>) {
    assert_eq!(
        looks(doc),
        BTreeMap::from([
            ("Board".to_owned(), Some((GREEN_NAME.to_owned(), GREEN))),
            ("Connector".to_owned(), Some((TAN_NAME.to_owned(), TAN))),
        ])
    );
    assert_eq!(
        painted_faces(doc, body_named(doc, "Connector")),
        BTreeMap::from([(GREY_NAME.to_owned(), 4)])
    );
    assert!(painted_faces(doc, body_named(doc, "Board")).is_empty());
    let mut names: Vec<&str> = doc
        .custom_appearances()
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, [GREEN_NAME, GREY_NAME, TAN_NAME]);
}

#[test]
fn the_board_and_connector_keep_their_colours_as_components() {
    let mut doc = design();
    let imported = doc
        .import_file_structured(
            PATH,
            &ReadOptions::default(),
            None,
            ImportStructure::Components,
        )
        .unwrap();
    let ImportedFile::Components { warnings, .. } = imported else {
        panic!("the file's assembly as components");
    };
    // The part without geometry is left out, said twice: its body and the
    // part.
    assert_eq!(
        warnings,
        [
            "Wireframe: body 0 is empty: left out",
            "Wireframe is empty: left out"
        ]
    );
    check_colors(&doc);
    // The placements stay as the file has them: the board lowered by its
    // thickness, the connector upright on it.
    let placed = |name: &str| {
        let body = body_named(&doc, name);
        let found: Vec<Transform> = doc
            .instances()
            .into_iter()
            .filter(|i| i.body == body)
            .map(|i| i.transform)
            .collect();
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        found[0]
    };
    let close = |a: Transform, b: Transform| {
        a.linear
            .iter()
            .flatten()
            .zip(b.linear.iter().flatten())
            .chain(a.translation.iter().zip(&b.translation))
            .all(|(x, y)| (x - y).abs() < 1e-9)
    };
    let board = placed("Board");
    assert!(
        close(board, Transform::translation([0.0, 0.0, -0.41148])),
        "{board:?}"
    );
    let connector = placed("Connector");
    assert!(close(connector, upright()), "{connector:?}");
    // One undo step takes the components, the bodies and the appearances.
    assert_eq!(doc.undo_label(), Some("Import board.step"));
    assert_eq!(doc.undo_depth(), 1);
    assert!(doc.undo().is_some());
    assert!(doc.bodies().is_empty());
    assert!(doc.assembly().is_empty());
    assert!(doc.custom_appearances().is_empty());
}

#[test]
fn the_board_and_connector_keep_their_colours_as_one_base_feature() {
    let mut doc = design();
    let imported = doc
        .import_file_structured(PATH, &ReadOptions::default(), None, ImportStructure::Base)
        .unwrap();
    let ImportedFile::Base(imported) = imported else {
        panic!("one base feature");
    };
    assert_eq!(imported.warnings, ["body 2 is empty: left out"]);
    check_colors(&doc);
    assert_eq!(doc.undo_label(), Some("Import board.step"));
    assert_eq!(doc.undo_depth(), 1);
    assert!(doc.undo().is_some());
    assert!(doc.bodies().is_empty());
    assert!(doc.custom_appearances().is_empty());
}

#[test]
fn a_design_that_has_the_colours_uses_their_appearances_again() {
    let mut doc = design();
    doc.import_file_structured(PATH, &ReadOptions::default(), None, ImportStructure::Base)
        .unwrap();
    doc.import_file_wrapped(
        PATH,
        &ReadOptions::default(),
        None,
        ImportStructure::Components,
    )
    .unwrap();
    // Four bodies, still three appearances.
    assert_eq!(doc.bodies().len(), 4);
    assert_eq!(doc.custom_appearances().len(), 3);
    for body in doc.bodies() {
        let look = doc.body_attributes(body.uid).appearance;
        assert!(look.is_some(), "{} has no appearance", body.name);
    }
}

#[test]
fn bodies_without_a_colour_keep_their_material_s_look() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().files.borrow_mut().insert(
        PATH.to_owned(),
        vec![ImportBody {
            color: None,
            face_colors: Vec::new(),
            ..board()
        }],
    );
    doc.import_file(PATH, &ReadOptions::default(), None)
        .unwrap();
    assert_eq!(looks(&doc), BTreeMap::from([("Board".to_owned(), None)]));
    assert!(doc.custom_appearances().is_empty());
}

/// The `import_file` command's result counts the features the import
/// recomputed: the bodies' appearances are set with the features, not in
/// steps after them that recompute nothing (the count of a later step).
#[test]
fn the_import_s_result_counts_its_own_recompute() {
    let mut doc = Document::new(MockKernel::default());
    let flat = "/models/board-only.step";
    doc.kernel()
        .files
        .borrow_mut()
        .insert(flat.to_owned(), vec![board()]);
    let placed = "/models/boards.step";
    doc.kernel().assemblies.borrow_mut().insert(
        placed.to_owned(),
        ImportAssembly {
            parts: vec![part("Board", vec![board()], Vec::new())],
            roots: vec![
                instance(0, Transform::IDENTITY),
                instance(0, Transform::translation([100.0, 0.0, 0.0])),
            ],
        },
    );
    let run = |doc: &mut Document<MockKernel>, command: Value| -> Value {
        serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
    };
    let result = run(&mut doc, json!({"cmd": "import_file", "path": flat}));
    assert_eq!(result["recomputed"], 1, "{result}");
    assert!(doc.undo().is_some());
    let result = run(&mut doc, json!({"cmd": "import_file", "path": placed}));
    assert_eq!(result["structure"], "components", "{result}");
    // At least the part's base feature (its component's own placing may
    // add to the count).
    assert!(result["recomputed"].as_u64().unwrap_or(0) >= 1, "{result}");
    assert_eq!(
        looks(&doc),
        BTreeMap::from([("Board".to_owned(), Some((GREEN_NAME.to_owned(), GREEN)))])
    );
}
