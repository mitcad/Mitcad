// SPDX-License-Identifier: MIT
//! End caps (mitcad#2243) with the mock kernel: `build_end_cap` closes a
//! 40 x 40 steel tube's ends with plates of its section, outward past the
//! end or inward with the tube cut back, set in or chamfered; a round
//! tube's cap is a disc, an angle's the rectangle round it; a cap follows
//! its member's size, is listed as a plate in the cut list and the parts
//! list, and the refusals are in words. The mock kernel's bodies are
//! boxes: a test gives the tube's edges the circles of its corner rounds
//! (as the geometry kernel reads them off a real tube), and the plates'
//! areas are the model's own outline's.

use std::f64::consts::PI;
use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::datum::CurveGeometry;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, ValueInput};
use mitcad_model_tests_support::test_library::*;

/// Caps ends; returns the answer or the refusal.
fn cap(doc: &mut Document<MockKernel>, fields: Value) -> Result<Value, String> {
    let mut fields = fields;
    fields["cmd"] = json!("build_end_cap");
    command(doc, fields)
}

/// Gives the member's body circular edges along its axis, as the
/// geometry kernel reads them off a real section: centres in the part's
/// x and y, all of radius `r`.
fn circles(doc: &Document<MockKernel>, member: &Value, centres: &[[f64; 2]], r: f64) {
    let part = derived(doc, component(member));
    let shape = doc.body_shape(BodyUid::new(part, 0)).unwrap().clone();
    let names: Vec<String> = doc
        .kernel()
        .edges(&shape)
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .filter(|n| {
            !["end_a", "end_b", "start(", "end("]
                .iter()
                .any(|end| n.contains(end))
        })
        .collect();
    assert!(names.len() >= centres.len(), "{names:?}");
    let mut curves = doc.kernel().edge_curves.borrow_mut();
    for (name, [x, y]) in names.iter().zip(centres) {
        let center = [*x, *y, 0.0];
        let start = [x + r, *y, 0.0];
        curves.insert(
            name.clone(),
            CurveGeometry::Circle {
                center,
                normal: [0.0, 0.0, 1.0],
                radius: r,
                start,
                end: start,
            },
        );
    }
}

/// A 40 x 40 tube's outer corner rounds of 6 mm (EN 10219's 2 T of a 3 mm
/// wall).
const CORNERS: [[f64; 2]; 4] = [[14.0, 14.0], [-14.0, 14.0], [-14.0, -14.0], [14.0, -14.0]];

fn cut_list(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "build_cut_list"}))
}

fn plates(doc: &Document<MockKernel>) -> Vec<Value> {
    cut_list(doc)["plates"].as_array().unwrap().clone()
}

/// The cut list's length of a member.
fn cut_length(doc: &Document<MockKernel>, member: &Value) -> f64 {
    let list = cut_list(doc);
    list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["components"]
                .as_array()
                .unwrap()
                .contains(&member["component"])
        })
        .and_then(|r| r["length"].as_f64())
        .unwrap_or_else(|| panic!("no row of {}: {list}", member["component"]))
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

/// The parts list's designation of a component.
fn designation(doc: &Document<MockKernel>, c: ComponentUid) -> String {
    doc.parts_list()
        .into_iter()
        .find(|r| r.component == c)
        .map(|r| r.designation)
        .unwrap()
}

fn cap_component(answer: &Value, i: usize) -> ComponentUid {
    answer["caps"][i]["component"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Where a point of a cap's own coordinates is in the design.
fn cap_point(doc: &Document<MockKernel>, answer: &Value, i: usize, p: [f64; 3]) -> [f64; 3] {
    let occurrence = answer["caps"][i]["occurrence"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    placed(doc, occurrence).apply_point(p)
}

#[test]
fn a_square_tube_capped_outward_gets_plates_of_its_section_with_its_corner_round() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 1000.0);
    circles(&doc, &tube, &CORNERS, 6.0);
    let answer = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"},
                        {"component": tube["component"], "end": "b"}],
               "thickness": 5}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(answer["count"], 2, "{answer}");
    assert_eq!(answer["caps"][0]["end"], "a");
    assert_eq!(answer["caps"][1]["end"], "b");
    let name = answer["caps"][0]["name"].as_str().unwrap();
    assert!(name.ends_with("End Cap"), "{answer}");
    assert_eq!(answer["caps"][0]["designation"], "Plate 40 x 40 x 5");
    // One undo step.
    assert_eq!(doc.undo_label(), Some("End Caps x2"));
    // The plates past the ends: end A's down from z = 0, end B's up from
    // z = 1000.
    let at = |i: usize, z: f64| cap_point(&doc, &answer, i, [0.0, 0.0, z]);
    assert!(close(at(0, 0.0), [0.0, 0.0, 0.0]), "{:?}", at(0, 0.0));
    assert!(close(at(0, 5.0), [0.0, 0.0, -5.0]), "{:?}", at(0, 5.0));
    assert!(close(at(1, 0.0), [0.0, 0.0, 1000.0]), "{:?}", at(1, 0.0));
    assert!(close(at(1, 5.0), [0.0, 0.0, 1005.0]), "{:?}", at(1, 5.0));
    // The cut list: the tube at its length, the two plates in one row of
    // 40 x 40 x 5 with the tube's 6 mm corners, the tube's steel.
    assert!((cut_length(&doc, &tube) - 1000.0).abs() < 1e-9);
    let plates = plates(&doc);
    assert_eq!(plates.len(), 1, "{plates:?}");
    let plate = &plates[0];
    assert_eq!(plate["quantity"], 2);
    assert_eq!(plate["designation"], "Plate 40 x 40 x 5");
    assert!((number(&plate["width"]) - 40.0).abs() < 1e-9);
    assert!((number(&plate["height"]) - 40.0).abs() < 1e-9);
    assert!((number(&plate["thickness"]) - 5.0).abs() < 1e-9);
    let area = 1600.0 - (4.0 - PI) * 36.0;
    let volume = number(&plate["area"]) * number(&plate["thickness"]);
    assert!((volume - 5.0 * area).abs() / (5.0 * area) < 1e-6, "{plate}");
    // 7850 kg/m3.
    let mass = number(&plate["mass"]);
    assert!((mass - 7850.0 * 5.0 * area / 1e9).abs() < 1e-9, "{plate}");
    // Listed as plates in the parts list too, not among the other parts
    // of the cut list.
    assert_eq!(
        designation(&doc, cap_component(&answer, 0)),
        "Plate 40 x 40 x 5"
    );
    let other = cut_list(&doc)["other"].as_array().unwrap().clone();
    assert!(
        !other
            .iter()
            .any(|o| o["component"] == answer["caps"][0]["component"]),
        "{other:?}"
    );
    // The tube's record keeps its caps: they go with it.
    let removed = command(
        &mut doc,
        json!({"cmd": "build_remove", "component": tube["component"]}),
    )
    .unwrap();
    let gone = removed["removed"]["components"].as_array().unwrap();
    assert_eq!(gone.len(), 3, "{removed}");
    all_ok(&doc);
}

#[test]
fn an_inward_cap_cuts_the_tube_back_and_outward_gives_it_its_length_again() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 1000.0);
    circles(&doc, &tube, &CORNERS, 6.0);
    let answer = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": "5 mm", "direction": "inward"}),
    )
    .unwrap();
    all_ok(&doc);
    // The tube is 5 mm shorter in the cut list; the plate fills its last
    // 5 mm, so the frame's length stays.
    assert!((cut_length(&doc, &tube) - 995.0).abs() < 1e-9);
    assert!(close(
        cap_point(&doc, &answer, 0, [0.0, 0.0, -5.0]),
        [0.0, 0.0, 995.0]
    ));
    let parts = query(&doc, json!({"query": "build_parts"}));
    let member = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["component"] == tube["component"])
        .unwrap();
    assert!(
        (number(&member["cut_back"]["b"]) - 5.0).abs() < 1e-9,
        "{member}"
    );
    assert_eq!(member["end_caps"]["b"], answer["caps"][0]["component"]);
    // A thicker cap cuts it back further: one value.
    let again = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 8, "direction": "inward"}),
    )
    .unwrap();
    assert_eq!(again["caps"][0]["new"], false, "{again}");
    assert_eq!(
        again["caps"][0]["component"],
        answer["caps"][0]["component"]
    );
    all_ok(&doc);
    assert!((cut_length(&doc, &tube) - 992.0).abs() < 1e-9);
    // Outward again: the same cap, the tube's whole length.
    cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 5, "direction": "outward"}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((cut_length(&doc, &tube) - 1000.0).abs() < 1e-9);
    assert_eq!(plates(&doc)[0]["quantity"], 1);
    // Inward, then the cap removed: the tube gets its length back.
    cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "b"}],
               "thickness": 5, "direction": "inward"}),
    )
    .unwrap();
    assert!((cut_length(&doc, &tube) - 995.0).abs() < 1e-9);
    command(
        &mut doc,
        json!({"cmd": "build_remove", "component": answer["caps"][0]["component"]}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((cut_length(&doc, &tube) - 1000.0).abs() < 1e-9);
    assert!(plates(&doc).is_empty());
    let parts = query(&doc, json!({"query": "build_parts"}));
    let member = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["component"] == tube["component"])
        .unwrap();
    assert_eq!(member["adds"], json!([]), "{member}");
}

#[test]
fn an_inset_and_a_chamfer_shape_the_plate() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 1000.0);
    circles(&doc, &tube, &CORNERS, 6.0);
    // Set in 1 mm: 38 x 38, the rounds 5 mm.
    cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "inset": 1}),
    )
    .unwrap();
    all_ok(&doc);
    let plate = &plates(&doc)[0];
    assert_eq!(plate["designation"], "Plate 38 x 38 x 5");
    let area = 38.0 * 38.0 - (4.0 - PI) * 25.0;
    assert!(
        (number(&plate["area"]) - area).abs() / area < 1e-9,
        "{plate}"
    );
    // A ratio of the thickness: 0.2 x 5 = 1 mm, the same plate.
    cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "ratio": 0.2}),
    )
    .unwrap();
    assert_eq!(plates(&doc)[0]["designation"], "Plate 38 x 38 x 5");
    // A chamfer of 3 mm is less than the 6 mm corners: refused in words.
    let refused = cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "chamfer": 3}),
    )
    .unwrap_err();
    assert!(
        refused.contains("less than the corners' round (6 mm)"),
        "{refused}"
    );
    // 6 mm cuts the rounds away: 1600 - 2 x 36.
    cap(
        &mut doc,
        json!({"ends": [{"component": tube["component"], "end": "a"}],
               "thickness": 5, "chamfer": 6}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((number(&plates(&doc)[0]["area"]) - (1600.0 - 72.0)).abs() < 1e-9);
    // A tube with sharp corners takes a 3 mm chamfer.
    let sharp = first(&mut doc, "tube40", [200.0, 0.0, 0.0], 500.0);
    cap(
        &mut doc,
        json!({"ends": [{"component": sharp["component"], "end": "b"}],
               "thickness": 5, "chamfer": 3}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        plates(&doc)
            .iter()
            .any(|p| (number(&p["area"]) - (1600.0 - 18.0)).abs() < 1e-9),
        "{:?}",
        plates(&doc)
    );
}

#[test]
fn a_round_tube_s_cap_is_a_disc_and_an_angle_s_the_rectangle_round_it() {
    let mut doc = design();
    // A round tube: its outer circle of 20 mm and its bore of 17 mm.
    let round = first(&mut doc, "tube40", [0.0; 3], 600.0);
    circles(&doc, &round, &[[0.0, 0.0], [0.0, 0.0]], 20.0);
    cap(
        &mut doc,
        json!({"ends": [{"component": round["component"], "end": "b"}], "thickness": 4}),
    )
    .unwrap();
    all_ok(&doc);
    let disc = &plates(&doc)[0];
    assert_eq!(disc["designation"], "Plate D40 x 4");
    assert_eq!(disc["disc"], true);
    assert!((number(&disc["area"]) - PI * 400.0).abs() < 1e-9, "{disc}");
    // A round section has no corners to chamfer.
    let refused = cap(
        &mut doc,
        json!({"ends": [{"component": round["component"], "end": "b"}],
               "thickness": 4, "chamfer": 2}),
    )
    .unwrap_err();
    assert!(refused.contains("no corners to chamfer"), "{refused}");
    // An angle 40 x 40 x 5 with its heel at (-20, -20): its root round
    // (7 mm) and toe rounds lie inside the rectangle round it, whose
    // corners stay sharp.
    let angle = first(&mut doc, "tube40", [200.0, 0.0, 0.0], 600.0);
    circles(&doc, &angle, &[[-8.0, -8.0]], 7.0);
    cap(
        &mut doc,
        json!({"ends": [{"component": angle["component"], "end": "a"}], "thickness": 4}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        plates(&doc)
            .iter()
            .any(|p| p["designation"] == "Plate 40 x 40 x 4"
                && (number(&p["area"]) - 1600.0).abs() < 1e-9),
        "{:?}",
        plates(&doc)
    );
}

/// A bar of a typed width (`W` x 10 mm, mitcad#1328): an open table whose
/// option `W` sets the section's width.
fn typed_bar() -> Document<MockKernel> {
    let mut source = Document::new(MockKernel::default());
    for (name, expression) in [("W", "20 mm"), ("L", "1000 mm")] {
        let mut add = json!({"cmd": "add_parameter", "name": name, "expression": expression});
        if name == "W" {
            add["min"] = json!("10 mm");
            add["max"] = json!("100 mm");
        }
        command(&mut source, add).unwrap();
    }
    let sketch = source.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = source
        .add_rectangle(
            sketch,
            [0.0, 0.0],
            &ValueInput::Name("W".into()),
            &ValueInput::Number(10.0),
        )
        .unwrap()
        .region;
    let solid = source
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    fixed_frame(
        &mut source,
        "end_a",
        [0.0; 3],
        [0.0, 0.0, -1.0],
        [1.0, 0.0, 0.0],
    );
    source
        .add_feature(
            &def(
                json!({"type": "joint_origin", "geometry": {"body": format!("{solid}.b0"),
                        "face": format!("{solid}:end({region})")}}),
            ),
            Some("mount:end_b"),
        )
        .unwrap();
    command(
        &mut source,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["W"], "closed": false, "default": "20",
            "rows": [{"name": "20", "values": {"W": "20 mm"}}]}}),
    )
    .unwrap();
    source.recompute();
    all_ok(&source);
    let mut build = profile_build("I", 5.0, 20.0, &[]);
    build["material"] = json!({"density": 7850});
    let mut library = TestLibrary::default();
    library.parts.insert(
        ("typed-bar".into(), V1.into()),
        Part {
            text: source.to_json(),
            build: Some(build),
            part_number: String::new(),
            name: "Typed bar".into(),
            designation: "Bar {W}x10 L={L}".into(),
        },
    );
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Arc::new(library)));
    doc
}

#[test]
fn a_cap_follows_its_member_s_size() {
    let mut doc = typed_bar();
    let bar = place(
        &mut doc,
        json!({"part": part("typed-bar"), "at": {"point": [0, 0, 0]}, "length": 500}),
    );
    let answer = cap(
        &mut doc,
        json!({"ends": [{"component": bar["component"], "end": "a"}], "thickness": 3}),
    )
    .unwrap();
    all_ok(&doc);
    let c = cap_component(&answer, 0);
    assert_eq!(designation(&doc, c), "Plate 20 x 10 x 3");
    // The bar 30 wide: its cap too, in the same step's recompute.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": bar["component"],
               "parameters": {"W": "30 mm"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(designation(&doc, c), "Plate 30 x 10 x 3");
    // And its length: end B's cap moves with the end.
    let b = cap(
        &mut doc,
        json!({"ends": [{"component": bar["component"], "end": "b"}], "thickness": 3}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": bar["component"], "length": 700}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        (cap_point(&doc, &b, 0, [0.0; 3])[2] - 700.0).abs() < 1e-9,
        "{:?}",
        cap_point(&doc, &b, 0, [0.0; 3])
    );
}

#[test]
fn refusals_are_in_words() {
    let mut doc = design();
    let tube = first(&mut doc, "tube40", [0.0; 3], 1000.0);
    circles(&doc, &tube, &CORNERS, 6.0);
    let refused = |doc: &mut Document<MockKernel>, fields: Value| cap(doc, fields).unwrap_err();
    let at = |end: &str| json!([{"component": tube["component"], "end": end}]);
    // Nothing to cap, no thickness, an inset that leaves nothing, an
    // inset given twice.
    assert!(refused(&mut doc, json!({"ends": [], "thickness": 5})).contains("pick the ends"));
    assert!(
        refused(&mut doc, json!({"ends": at("a"), "thickness": 0}))
            .contains("thickness must be greater than zero")
    );
    let deep = refused(
        &mut doc,
        json!({"ends": at("a"), "thickness": 5, "inset": 20}),
    );
    assert!(
        deep.contains("leaves nothing of the 40 x 40 mm section"),
        "{deep}"
    );
    assert!(
        refused(
            &mut doc,
            json!({"ends": at("a"), "thickness": 5, "inset": 1, "ratio": 0.2})
        )
        .contains("a length or a ratio, not both")
    );
    // An inward cap thicker than the tube is long.
    let short = first(&mut doc, "tube40", [300.0, 0.0, 0.0], 20.0);
    let long = refused(
        &mut doc,
        json!({"ends": [{"component": short["component"], "end": "a"}],
               "thickness": 25, "direction": "inward"}),
    );
    assert!(long.contains("leaves nothing of it"), "{long}");
    // A mitered end gets no cap.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": tube["component"],
               "ends": {"a": {"angle": "45 deg"}}}),
    )
    .unwrap();
    let mitered = refused(&mut doc, json!({"ends": at("a"), "thickness": 5}));
    assert!(mitered.contains("is mitered"), "{mitered}");
    // A capped end is not mitered.
    cap(&mut doc, json!({"ends": at("b"), "thickness": 5})).unwrap();
    let edit = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": tube["component"],
               "ends": {"b": {"angle": "30 deg"}}}),
    )
    .unwrap_err();
    assert!(edit.contains("has an end cap"), "{edit}");
    // A part placed on an end: the end is not free.
    let tp = derived(&doc, component(&short));
    place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": short["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")}},
               "length": 100}),
    );
    let taken = refused(
        &mut doc,
        json!({"ends": [{"component": short["component"], "end": "b"}], "thickness": 5}),
    );
    assert!(taken.contains("an end cap needs a free end"), "{taken}");
    // The preview query says the same, with the member it is about.
    let preview = query(
        &doc,
        json!({"query": "build_end_caps",
               "ends": [{"component": short["component"], "end": "b"},
                        {"component": tube["component"], "end": "b"}],
               "thickness": 5}),
    );
    assert_eq!(
        preview["problems"][0]["component"], short["component"],
        "{preview}"
    );
    assert_eq!(
        preview["plates"][0]["component"], tube["component"],
        "{preview}"
    );
    assert_eq!(preview["caps"].as_array().unwrap().len(), 1, "{preview}");
    // The plate's outline at the end, in the design: z = 1000.
    let outline = preview["plates"][0]["outline"].as_array().unwrap();
    assert!(outline.len() > 4, "{preview}");
    assert!(
        outline
            .iter()
            .all(|p| (number(&p[2]) - 1000.0).abs() < 1e-9),
        "{preview}"
    );
}
