// SPDX-License-Identifier: MIT
//! The FreeCAD import on small hand-made documents with the model's mock
//! kernel: a shape file `"<faces> <name>"` is a mock body with that many
//! faces in a 10 mm box from the origin.

use std::path::Path;

use mitcad_freecad::FcstdFile;
use mitcad_model::testing::MockKernel;
use mitcad_model::{BoundingBox, Document, Kernel, Transform};
use mitcad_zip::{Method, Writer};
use serde_json::json;

use super::*;

/// `<Property>` elements.
fn prop(name: &str, kind: &str, value: &str) -> String {
    format!("<Property name=\"{name}\" type=\"{kind}\">{value}</Property>")
}

fn placement(p: [f64; 3], axis: [f64; 3], degrees: f64) -> String {
    let q = Placement::from_axis_angle(p, axis, degrees.to_radians()).rotation;
    prop(
        "Placement",
        "App::PropertyPlacement",
        &format!(
            "<PropertyPlacement Px=\"{}\" Py=\"{}\" Pz=\"{}\" Q0=\"{}\" Q1=\"{}\" Q2=\"{}\" Q3=\"{}\" A=\"0\" Ox=\"0\" Oy=\"0\" Oz=\"1\"/>",
            p[0], p[1], p[2], q[0], q[1], q[2], q[3]
        ),
    )
}

fn shape(file: &str) -> String {
    prop(
        "Shape",
        "Part::PropertyPartShape",
        &format!("<Part file=\"{file}\"/>"),
    )
}

fn label(text: &str) -> String {
    prop(
        "Label",
        "App::PropertyString",
        &format!("<String value=\"{text}\"/>"),
    )
}

fn links(name: &str, kind: &str, targets: &[&str]) -> String {
    let items: String = targets
        .iter()
        .map(|t| format!("<Link value=\"{t}\"/>"))
        .collect();
    prop(
        name,
        kind,
        &format!("<LinkList count=\"{}\">{items}</LinkList>", targets.len()),
    )
}

fn link(name: &str, target: &str) -> String {
    prop(
        name,
        "App::PropertyLink",
        &format!("<Link value=\"{target}\"/>"),
    )
}

fn xlink(file: &str, target: &str) -> String {
    prop(
        "LinkedObject",
        "App::PropertyXLink",
        &format!("<XLink file=\"{file}\" stamp=\"\" name=\"{target}\"/>"),
    )
}

fn flag(name: &str, value: bool) -> String {
    prop(
        name,
        "App::PropertyBool",
        &format!("<Bool value=\"{value}\"/>"),
    )
}

/// A Document.xml of objects: (type, name, properties).
fn document(objects: &[(&str, &str, String)]) -> String {
    let list: String = objects
        .iter()
        .map(|(t, n, _)| format!("<Object type=\"{t}\" name=\"{n}\"/>"))
        .collect();
    let data: String = objects
        .iter()
        .map(|(t, n, p)| {
            let extension = if *t == "App::Link" {
                "<Extensions Count=\"1\"><Extension type=\"App::LinkExtension\" name=\"LinkExtension\"/></Extensions>"
            } else {
                ""
            };
            format!("<Object name=\"{n}\">{extension}<Properties>{p}</Properties></Object>")
        })
        .collect();
    format!(
        "<Document SchemaVersion=\"4\" ProgramVersion=\"1.0R39319 (Git)\" FileVersion=\"1\">\
         <Properties>{}</Properties><Objects>{list}</Objects><ObjectData>{data}</ObjectData></Document>",
        label("test")
    )
}

fn fcstd(xml: &str, gui: Option<&str>, files: &[(&str, &[u8])]) -> FcstdFile {
    let mut writer = Writer::new();
    writer
        .add("Document.xml", xml.as_bytes(), Method::Deflate)
        .unwrap();
    if let Some(gui) = gui {
        writer
            .add("GuiDocument.xml", gui.as_bytes(), Method::Deflate)
            .unwrap();
    }
    for (name, data) in files {
        writer.add(name, data, Method::Stored).unwrap();
    }
    FcstdFile::from_bytes(writer.finish().unwrap()).unwrap()
}

fn import(
    file: FcstdFile,
    others: Vec<(&'static str, FcstdFile)>,
) -> (Document<MockKernel>, FcstdReport) {
    import_with(file, others, MockKernel::default(), true)
}

fn import_with(
    file: FcstdFile,
    others: Vec<(&'static str, FcstdFile)>,
    kernel: MockKernel,
    bodies_only: bool,
) -> (Document<MockKernel>, FcstdReport) {
    let mut doc = Document::new(kernel);
    let mut others: Vec<(&str, Option<FcstdFile>)> =
        others.into_iter().map(|(n, f)| (n, Some(f))).collect();
    let mut loader = |p: &Path| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        others
            .iter_mut()
            .find(|(n, _)| *n == name)
            .and_then(|(_, f)| f.take())
            .ok_or_else(|| format!("{name}: not found"))
    };
    let options = FcstdOptions {
        bodies_only,
        undo_label: Some("Import test.FCStd".to_owned()),
        ..FcstdOptions::default()
    };
    let report = import_fcstd_file(
        &mut doc,
        file,
        Path::new("/models/test.FCStd"),
        &mut loader,
        &options,
    )
    .unwrap();
    (doc, report)
}

fn item<'a>(report: &'a FcstdReport, name: &str) -> &'a ObjectReport {
    report
        .items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no item {name}"))
}

/// The world box centres of a body's instances.
fn centres(doc: &Document<MockKernel>, name: &str) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = doc
        .instances()
        .iter()
        .filter(|i| i.name == name)
        .map(|i| {
            let BoundingBox { min, max } = doc
                .kernel()
                .bounding_box(doc.body_shape(i.body).unwrap())
                .unwrap()
                .unwrap();
            let c = std::array::from_fn(|k| (min[k] + max[k]) / 2.0);
            i.transform.apply_point(c).map(|v| (v * 1e6).round() / 1e6)
        })
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

#[test]
fn bodies_parts_and_what_is_left_out() {
    let xml = document(&[
        (
            "Part::Box",
            "Box",
            format!("{}{}", label("Red box"), shape("Box.brp")),
        ),
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}{}",
                shape("Body.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad"]),
                link("Tip", "Pad"),
                flag("Visibility", false)
            ),
        ),
        ("Sketcher::SketchObject", "Sketch", shape("Sketch.brp")),
        ("PartDesign::Pad", "Pad", shape("Pad.brp")),
        ("Sketcher::SketchObject", "Loose", shape("Sketch.brp")),
        (
            "App::Part",
            "Part",
            format!(
                "{}{}",
                links("Group", "App::PropertyLinkList", &["Cylinder"]),
                placement([0.0, 100.0, 0.0], [0.0, 0.0, 1.0], 90.0)
            ),
        ),
        ("Part::Cylinder", "Cylinder", shape("Cylinder.brp")),
        ("Part::Box", "A", shape("Box.brp")),
        ("Part::Box", "B", shape("Box.brp")),
        (
            "Part::Cut",
            "Cut",
            format!(
                "{}{}{}",
                shape("Cut.brp"),
                link("Base", "A"),
                link("Tool", "B")
            ),
        ),
        ("Part::Line", "Line", shape("Line.brp")),
        ("Spreadsheet::Sheet", "Sheet", String::new()),
        ("Part::Feature", "Broken", shape("Missing.brp")),
    ]);
    let gui = r#"<Document SchemaVersion="1"><ViewProviderData Count="2">
        <ViewProvider name="Box"><Properties><Property name="ShapeColor" type="App::PropertyColor"><PropertyColor value="4278190080"/></Property></Properties></ViewProvider>
        <ViewProvider name="Cut"><Properties><Property name="ShapeColor" type="App::PropertyColor"><PropertyColor value="3435973632"/></Property></Properties></ViewProvider>
        </ViewProviderData></Document>"#;
    let file = fcstd(
        &xml,
        Some(gui),
        &[
            ("Box.brp", b"6 box"),
            ("Body.brp", b"7 body"),
            ("Sketch.brp", b"0 wire"),
            ("Pad.brp", b"7 pad"),
            ("Cylinder.brp", b"3 cylinder"),
            ("Cut.brp", b"9 cut"),
            ("Line.brp", b"0 line"),
        ],
    );
    let (doc, report) = import(file, Vec::new());
    let outcome = |name: &str| item(&report, name).outcome;
    assert_eq!(outcome("Box"), ObjectOutcome::Body);
    assert_eq!(outcome("Body"), ObjectOutcome::Body);
    assert_eq!(outcome("Pad"), ObjectOutcome::Included);
    assert_eq!(outcome("Sketch"), ObjectOutcome::Included);
    assert_eq!(outcome("Loose"), ObjectOutcome::Skipped);
    assert_eq!(outcome("Part"), ObjectOutcome::Component);
    assert_eq!(outcome("Cylinder"), ObjectOutcome::Body);
    assert_eq!(outcome("A"), ObjectOutcome::Included);
    assert!(
        item(&report, "A")
            .note
            .as_deref()
            .unwrap()
            .contains("used by Cut")
    );
    assert_eq!(outcome("Cut"), ObjectOutcome::Body);
    assert_eq!(outcome("Line"), ObjectOutcome::Skipped);
    assert!(
        item(&report, "Line")
            .note
            .as_deref()
            .unwrap()
            .starts_with("no faces")
    );
    assert_eq!(outcome("Sheet"), ObjectOutcome::Skipped);
    assert!(
        item(&report, "Broken")
            .note
            .as_deref()
            .unwrap()
            .contains("could not be read")
    );
    assert_eq!(item(&report, "Cylinder").component.as_deref(), Some("Part"));

    // One base feature per component, bodies named after the labels.
    let names: Vec<String> = doc.bodies().into_iter().map(|b| b.name).collect();
    assert_eq!(names, ["Red box", "Body", "Cut", "Cylinder"]);
    let bases = doc
        .features()
        .filter(|f| matches!(f.def, mitcad_model::FeatureDef::Base(_)))
        .count();
    assert_eq!(bases, 2);
    assert_eq!(report.components, 1);
    // The part's placement moves its cylinder: (5, 5, 5) turned about Z.
    assert_eq!(centres(&doc, "Cylinder"), [[-5.0, 105.0, 5.0]]);
    // The hidden body, the colours (FreeCAD's default grey is not taken).
    let body = item(&report, "Body").bodies[0].parse().unwrap();
    assert!(!doc.body_attributes(body).visible);
    let colors: Vec<Option<[f64; 3]>> = report.bodies.iter().map(|b| b.color).collect();
    assert_eq!(colors, [Some([1.0, 0.0, 0.0]), None, None, None]);
    // One undo step.
    assert_eq!(doc.undo_depth(), 1);
    // The bodies' places and the part's.
    let placed: Vec<&str> = report.placed.iter().map(|p| p.object.as_str()).collect();
    assert_eq!(placed, ["Box", "Body", "Cylinder", "Part", "Cut"]);
}

#[test]
fn placed_bodies_and_links_get_components() {
    let xml = document(&[
        (
            "PartDesign::Body",
            "Bracket",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                placement([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 0.0),
                flag("Visibility", false)
            ),
        ),
        (
            "App::Link",
            "Plain",
            format!(
                "{}{}{}",
                xlink("", "Bracket"),
                placement([100.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0),
                flag("LinkTransform", false)
            ),
        ),
        (
            "App::Link",
            "OnTop",
            format!(
                "{}{}{}",
                xlink("", "Bracket"),
                placement([0.0, 100.0, 0.0], [0.0, 0.0, 1.0], 0.0),
                flag("LinkTransform", true)
            ),
        ),
        (
            "App::Link",
            "Relay",
            format!(
                "{}{}",
                xlink("", "Plain"),
                placement([0.0, 0.0, 50.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        (
            "App::Link",
            "Bolt",
            format!(
                "{}{}",
                xlink("bolt.FCStd", "Screw"),
                placement([-100.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        ("App::Link", "Lost", xlink("lost.FCStd", "Screw")),
    ]);
    let bolt = fcstd(
        &document(&[(
            "Part::Cylinder",
            "Screw",
            format!("{}{}", label("Screw"), shape("Screw.brp")),
        )]),
        None,
        &[("Screw.brp", b"3 screw")],
    );
    let file = fcstd(&xml, None, &[("Body.brp", b"7 body")]);
    let (doc, report) = import(file, vec![("bolt.FCStd", bolt)]);
    // The body: a component of its own with its origin; hidden there
    // (its occurrence), shown through the links: in place of the body's
    // placement, on top of it, and a link's link in place of the link's.
    // (The mock's bodies are a 10 mm box from their component's origin.)
    assert_eq!(report.components, 2);
    assert_eq!(
        item(&report, "Bracket").component.as_deref(),
        Some("Bracket")
    );
    assert_eq!(
        centres(&doc, "Bracket"),
        [
            [5.0, 5.0, 15.0],
            [5.0, 5.0, 55.0],
            [5.0, 105.0, 15.0],
            [105.0, 5.0, 5.0]
        ]
    );
    let hidden: Vec<bool> = doc.instances().iter().map(|i| i.visible).collect();
    assert_eq!(hidden.iter().filter(|v| !**v).count(), 1);
    // The other file's object.
    assert_eq!(centres(&doc, "Screw"), [[-95.0, 5.0, 5.0]]);
    assert_eq!(report.files, ["bolt.FCStd"]);
    assert_eq!(report.missing_files, ["lost.FCStd"]);
    assert_eq!(item(&report, "Lost").outcome, ObjectOutcome::Skipped);
    assert_eq!(item(&report, "Bolt").outcome, ObjectOutcome::Occurrence);
    assert_eq!(
        report
            .bodies
            .iter()
            .find(|b| b.object == "Screw")
            .unwrap()
            .file
            .as_deref(),
        Some("bolt.FCStd")
    );
}

#[test]
fn link_arrays_scales_and_grounded_assembly_parts() {
    let mut list = 2u32.to_le_bytes().to_vec();
    for v in [
        0.0f64, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 20.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ] {
        list.extend(v.to_le_bytes());
    }
    let xml = document(&[
        (
            "App::Part",
            "Cell",
            format!(
                "{}{}",
                links("Group", "App::PropertyLinkList", &["Pin"]),
                placement([0.0, 30.0, 0.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        ("Part::Box", "Pin", shape("Pin.brp")),
        (
            "App::Link",
            "Pair",
            format!(
                "{}{}{}{}",
                xlink("", "Cell"),
                prop(
                    "ElementCount",
                    "App::PropertyIntegerConstraint",
                    "<Integer value=\"2\"/>"
                ),
                prop(
                    "PlacementList",
                    "App::PropertyPlacementList",
                    "<PlacementList file=\"PlacementList\"/>"
                ),
                prop(
                    "VisibilityList",
                    "App::PropertyBoolList",
                    "<BoolList value=\"01\"/>"
                )
            ),
        ),
        (
            "App::Link",
            "Big",
            format!(
                "{}{}",
                xlink("", "Pin"),
                prop(
                    "ScaleVector",
                    "App::PropertyVector",
                    "<PropertyVector valueX=\"2\" valueY=\"2\" valueZ=\"2\"/>"
                )
            ),
        ),
        (
            "Assembly::AssemblyObject",
            "Assembly",
            links("Group", "App::PropertyLinkList", &["Joints", "Fixed"]),
        ),
        (
            "Assembly::JointGroup",
            "Joints",
            links("Group", "App::PropertyLinkList", &["GroundedJoint"]),
        ),
        (
            "App::FeaturePython",
            "GroundedJoint",
            link("ObjectToGround", "Fixed"),
        ),
        ("App::Link", "Fixed", xlink("", "Pin")),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[("Pin.brp", b"6 pin"), ("PlacementList", &list)],
    );
    let (doc, report) = import(file, Vec::new());
    // The part in place and in two elements (the part's placement replaced
    // by the link's, the second moved by 20 in X and hidden); the pin, which
    // links show, a component of its own, also in the assembly, grounded.
    assert_eq!(
        centres(&doc, "Pin"),
        [
            [5.0, 5.0, 5.0],
            [5.0, 5.0, 5.0],
            [5.0, 35.0, 5.0],
            [25.0, 5.0, 5.0]
        ]
    );
    assert_eq!(report.components, 3);
    let hidden = doc.instances().iter().filter(|i| !i.visible).count();
    assert_eq!(hidden, 1);
    let grounded = doc
        .assembly()
        .occurrences
        .iter()
        .filter(|o| o.grounded)
        .count();
    assert_eq!(grounded, 1);
    assert_eq!(item(&report, "Assembly").outcome, ObjectOutcome::Component);
    assert_eq!(
        item(&report, "GroundedJoint").outcome,
        ObjectOutcome::Skipped
    );
    assert!(
        item(&report, "GroundedJoint")
            .note
            .as_deref()
            .unwrap()
            .contains("grounded")
    );
    // The scaled link: a (scaled) copy of the pin's body.
    assert_eq!(item(&report, "Big").outcome, ObjectOutcome::Body);
    assert_eq!(centres(&doc, "Big").len(), 1);
    let elements: Vec<Option<usize>> = report
        .placed
        .iter()
        .filter(|p| p.object == "Pair")
        .map(|p| p.element)
        .collect();
    assert_eq!(elements, [Some(0), Some(1)]);
}

#[test]
fn reference_dumps_are_compared() {
    let xml = document(&[("Part::Box", "Box", shape("Box.brp"))]);
    let file = fcstd(&xml, None, &[("Box.brp", b"6 box")]);
    let (_, mut report) = import(file, Vec::new());
    // The mock kernel does not measure: no volume, area or centre.
    let dump = json!({"format": "mitcad-freecad-dump", "objects": [
        {"name": "Box", "shape": {"solids": 1, "volume": 0.0, "area": 0.0,
                                  "world_center": [0.0, 0.0, 0.0], "faces": 6}}]});
    let r = reference::check(&report, &dump);
    assert!(r.pass, "{:?}", r.differences);
    assert_eq!(r.checked, 3);
    report.placed[0].center = [0.0, 0.0, 1.0];
    let r = reference::check(&report, &dump);
    assert!(!r.pass && r.differences[0].contains("centre"));
    assert!(!reference::check(&report, &json!({})).pass);
    // FreeCAD's own measures are no reference for an invalid shape, nor
    // when its mesh of the shape disagrees with them: a difference there is
    // not one of the import; agreeing measures still count.
    for (shape, excused) in [
        (
            json!({"solids": 1, "valid": false, "volume": -5.0, "area": 1.0}),
            1,
        ),
        (
            json!({"solids": 1, "volume": 5.0, "mesh_volume": 9.0, "area": 1.0, "mesh_area": 1.0}),
            1,
        ),
        (
            json!({"solids": 1, "valid": false, "volume": 0.0, "area": 0.0,
                   "world_center": [0.0, 0.0, 1.0]}),
            0,
        ),
    ] {
        let dump = json!({"format": "mitcad-freecad-dump",
                          "objects": [{"name": "Box", "shape": shape}]});
        let r = reference::check(&report, &dump);
        assert!(
            r.pass && r.checked == 3 && r.not_compared.len() == excused,
            "{r:?}"
        );
    }
    // A transform as the importer makes them.
    assert_eq!(
        transform(&Placement::translation([1.0, 2.0, 3.0])),
        Transform::translation([1.0, 2.0, 3.0])
    );
}

// Sketches (stage 2).

/// A `Geometry` property of the geometries' XML.
fn geometry(items: &[String]) -> String {
    prop(
        "Geometry",
        "Part::PropertyGeometryList",
        &format!(
            "<GeometryList count=\"{}\">{}</GeometryList>",
            items.len(),
            items.concat()
        ),
    )
}

fn segment(a: [f64; 2], b: [f64; 2], construction: bool) -> String {
    format!(
        "<Geometry type=\"Part::GeomLineSegment\"><LineSegment StartX=\"{}\" StartY=\"{}\" StartZ=\"0\" EndX=\"{}\" EndY=\"{}\" EndZ=\"0\"/><Construction value=\"{}\"/></Geometry>",
        a[0],
        a[1],
        b[0],
        b[1],
        u8::from(construction)
    )
}

fn round(center: [f64; 2], radius: f64) -> String {
    format!(
        "<Geometry type=\"Part::GeomCircle\"><Circle CenterX=\"{}\" CenterY=\"{}\" CenterZ=\"0\" NormalX=\"0\" NormalY=\"0\" NormalZ=\"1\" AngleXU=\"0\" Radius=\"{radius}\"/><Construction value=\"0\"/></Geometry>",
        center[0], center[1]
    )
}

/// A constraint: type, value, references (geo, pos), name and whether it
/// drives.
fn constrain(kind: u8, value: f64, refs: &[(i32, u8)], name: &str, driving: bool) -> String {
    let r = |i: usize| refs.get(i).copied().unwrap_or((-2000, 0));
    format!(
        "<Constrain Name=\"{name}\" Type=\"{kind}\" Value=\"{value}\" First=\"{}\" FirstPos=\"{}\" Second=\"{}\" SecondPos=\"{}\" Third=\"{}\" ThirdPos=\"{}\" IsDriving=\"{}\" IsActive=\"1\"/>",
        r(0).0,
        r(0).1,
        r(1).0,
        r(1).1,
        r(2).0,
        r(2).1,
        u8::from(driving)
    )
}

fn constraints(items: &[String]) -> String {
    prop(
        "Constraints",
        "Sketcher::PropertyConstraintList",
        &format!(
            "<ConstraintList count=\"{}\">{}</ConstraintList>",
            items.len(),
            items.concat()
        ),
    )
}

/// The Mitcad sketch made of a FreeCAD object.
fn sketch_def(doc: &Document<MockKernel>, report: &FcstdReport, object: &str) -> serde_json::Value {
    let s = report
        .sketches
        .iter()
        .find(|s| s.object == object)
        .unwrap_or_else(|| panic!("no sketch {object}"));
    let uid = s
        .feature
        .as_deref()
        .unwrap_or_else(|| panic!("{object} not imported: {:?}", s.notes));
    let answer = doc
        .query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
        .unwrap();
    serde_json::from_str::<serde_json::Value>(&answer).unwrap()["def"].clone()
}

#[test]
fn sketches_come_after_the_bodies_editable() {
    let profile = format!(
        "{}{}{}",
        geometry(&[
            segment([0.0, 0.0], [40.0, 0.0], false),
            segment([40.0, 0.0], [40.0, 20.0], false),
            round([10.0, 10.0], 3.0),
            segment([0.0, 0.0], [40.0, 20.0], true),
        ]),
        constraints(&[
            constrain(1, 0.0, &[(0, 1), (-1, 1)], "", true),
            constrain(1, 0.0, &[(0, 2), (1, 1)], "", true),
            constrain(1, 0.0, &[(3, 1), (0, 1)], "", true),
            constrain(1, 0.0, &[(3, 2), (1, 2)], "", true),
            constrain(2, 0.0, &[(0, 0)], "", true),
            constrain(3, 0.0, &[(1, 0)], "", true),
            constrain(6, 40.0, &[(0, 0)], "width", true),
            constrain(8, 20.0, &[(1, 0)], "", false),
            constrain(7, -10.0, &[(2, 3), (0, 1)], "", true),
            constrain(11, 3.0, &[(2, 0)], "", true),
        ]),
        flag("Visibility", false)
    );
    // At the document's top, turned a quarter about X: on Mitcad's XZ
    // plane with a frame of its own (FreeCAD's y is +Z, Mitcad's −Z).
    let side = format!(
        "{}{}{}",
        placement([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 90.0),
        geometry(&[segment([0.0, 0.0], [5.0, 5.0], false)]),
        constraints(&[constrain(16, 1.0, &[(0, 2), (0, 1), (0, 0)], "", true)])
    );
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}",
                shape("Body.brp"),
                links("Group", "App::PropertyLinkList", &["Profile"])
            ),
        ),
        ("Sketcher::SketchObject", "Profile", profile),
        ("Sketcher::SketchObject", "Side", side),
    ]);
    let file = || fcstd(&xml, None, &[("Body.brp", b"6 body")]);
    let (doc, report) = import_with(file(), Vec::new(), MockKernel::default(), false);
    assert!(!report.bodies_only);
    // The base feature first, then the sketches.
    let types: Vec<&str> = doc
        .features()
        .map(|f| match f.def {
            mitcad_model::FeatureDef::Base(_) => "base",
            mitcad_model::FeatureDef::Sketch(_) => "sketch",
            _ => "other",
        })
        .collect();
    assert_eq!(types, ["base", "sketch", "sketch"]);
    let s = &report.sketches[0];
    assert_eq!(s.outcome, ObjectOutcome::Parametric, "{s:?}");
    assert_eq!(item(&report, "Profile").outcome, ObjectOutcome::Parametric);
    assert!(s.error < 1e-9);
    let def = sketch_def(&doc, &report, "Profile");
    assert_eq!(def["plane"], "xy");
    assert!(def.get("frame").is_none());
    // Points joined by the coincidences (the root point fixed), the
    // construction line kept as one.
    let entities = def["entities"].as_array().unwrap();
    let points = entities.iter().filter(|e| e["type"] == "point").count();
    assert_eq!(points, 4);
    assert!(
        entities
            .iter()
            .any(|e| e["type"] == "point" && e["fixed"] == true && e["at"] == json!([0.0, 0.0]))
    );
    assert_eq!(
        entities
            .iter()
            .filter(|e| e["construction"] == true)
            .count(),
        1
    );
    let kinds: Vec<&str> = def["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["horizontal", "vertical"]);
    // The dimensions: the named length; the reference distance driven;
    // the negative horizontal distance turned round.
    let dims = def["dimensions"].as_array().unwrap();
    assert_eq!(dims.len(), 4);
    assert_eq!(dims[0]["type"], "length");
    assert_eq!(dims[1]["type"], "vertical_distance");
    assert_eq!(dims[1]["driven"], true);
    assert_eq!(dims[2]["type"], "horizontal_distance");
    assert_eq!(s.dimension_sources[0].name, "width");
    assert_eq!(s.dimension_sources[1].mitcad, 10.0);
    // FreeCAD's visibility.
    let uid = s.feature.as_deref().unwrap().parse().unwrap();
    assert_eq!(doc.feature_visible(uid), Some(false));

    // Snell's law left out: partial; on XZ with a frame.
    let side = &report.sketches[1];
    assert_eq!(side.outcome, ObjectOutcome::Partial);
    assert!(side.dropped[0].contains("SnellsLaw"), "{:?}", side.dropped);
    let def = sketch_def(&doc, &report, "Side");
    assert_eq!(def["plane"], "xz");
    let y: Vec<f64> = def["frame"]["y_axis"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v.as_f64().unwrap() * 1e9).round() / 1e9)
        .collect();
    assert_eq!(y, [0.0, -1.0, 0.0]);
    // The whole import is one undo step.
    assert_eq!(doc.undo_depth(), 1);

    // The bodies only.
    let (doc, report) = import_with(file(), Vec::new(), MockKernel::default(), true);
    assert!(report.sketches.is_empty());
    assert_eq!(doc.features().count(), 1);
}

#[test]
fn external_geometry_links_to_the_imported_bodies() {
    use mitcad_model::Curve3;
    let kernel = MockKernel::default();
    // Every edge of the mock's bodies is this line.
    kernel.curves.borrow_mut().push(Curve3::Line {
        start: [0.0, 0.0, 10.0],
        end: [40.0, 0.0, 10.0],
    });
    let external = prop(
        "ExternalGeometry",
        "App::PropertyLinkSubList",
        "<LinkSubList count=\"1\"><Link obj=\"Pad\" sub=\"Edge1\"/></LinkSubList>",
    );
    let axis = |x: f64, y: f64| {
        format!(
            "<Geometry type=\"Part::GeomLineSegment\"><LineSegment StartX=\"0\" StartY=\"0\" StartZ=\"0\" EndX=\"{x}\" EndY=\"{y}\" EndZ=\"0\"/><Construction value=\"1\"/></Geometry>"
        )
    };
    let copy = prop(
        "ExternalGeo",
        "Part::PropertyGeometryList",
        &format!(
            "<GeometryList count=\"3\">{}{}{}</GeometryList>",
            axis(1.0, 0.0),
            axis(0.0, 1.0),
            segment([0.0, 0.0], [40.0, 0.0], true)
        ),
    );
    let on_top = format!(
        "{}{}{}{}{}",
        placement([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 0.0),
        external,
        copy,
        geometry(&[segment([0.0, 0.0], [20.0, 0.0], false)]),
        constraints(&[
            constrain(1, 0.0, &[(0, 1), (-3, 1)], "", true),
            constrain(13, 0.0, &[(0, 2), (-3, 0)], "", true),
        ])
    );
    let xml = document(&[
        (
            "PartDesign::Body",
            "Block",
            format!(
                "{}{}{}",
                shape("Block.brp"),
                links("Group", "App::PropertyLinkList", &["Pad", "OnTop"]),
                link("Tip", "Pad")
            ),
        ),
        ("PartDesign::Pad", "Pad", shape("Pad.brp")),
        ("Sketcher::SketchObject", "OnTop", on_top),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[("Block.brp", b"6 block"), ("Pad.brp", b"6 pad")],
    );
    let (doc, report) = import_with(file, Vec::new(), kernel, false);
    let s = &report.sketches[0];
    assert_eq!(s.outcome, ObjectOutcome::Parametric, "{s:?}");
    let def = sketch_def(&doc, &report, "OnTop");
    // On XY, 10 mm up; the pad's edge projected, linked to the body's
    // edge by its name, as construction geometry; the line's start on its
    // start point.
    assert_eq!(def["plane"], "xy");
    assert_eq!(def["frame"]["origin"], json!([0.0, 0.0, 10.0]));
    let projection = &def["projections"][0];
    assert_eq!(projection["body"], "F1.b0");
    assert_eq!(projection["source"], "E{F1:import(0)|F1:import(1)}");
    let entities = def["entities"].as_array().unwrap();
    let projected = entities
        .iter()
        .find(|e| e["type"] == "line" && e["reference"] == true)
        .unwrap();
    assert_eq!(projected["construction"], true);
    let line = entities
        .iter()
        .find(|e| e["type"] == "line" && e["reference"].is_null())
        .unwrap();
    assert_eq!(line["start"], projected["start"]);
    assert_eq!(
        def["constraints"][0],
        json!({"id": "k1", "type": "coincident", "point": line["end"], "entity": projected["id"]})
    );
}

// The history (stage 3). The mock kernel does not measure shapes, so each
// feature's first definition that builds is taken unchecked.

fn float(name: &str, value: f64) -> String {
    prop(
        name,
        "App::PropertyLength",
        &format!("<Float value=\"{value}\"/>"),
    )
}

fn enumeration(name: &str, index: i64) -> String {
    prop(
        name,
        "App::PropertyEnumeration",
        &format!("<Integer value=\"{index}\"/>"),
    )
}

fn link_sub(name: &str, target: &str, subs: &[&str]) -> String {
    let items: String = subs
        .iter()
        .map(|s| format!("<Sub value=\"{s}\"/>"))
        .collect();
    prop(
        name,
        "App::PropertyLinkSub",
        &format!(
            "<LinkSub value=\"{target}\" count=\"{}\">{items}</LinkSub>",
            subs.len()
        ),
    )
}

/// A closed rectangle sketch from (x, y), its corners joined.
fn rectangle_sketch(x: f64, y: f64, w: f64, h: f64) -> String {
    let p = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
    format!(
        "{}{}",
        geometry(&[
            segment(p[0], p[1], false),
            segment(p[1], p[2], false),
            segment(p[2], p[3], false),
            segment(p[3], p[0], false),
        ]),
        constraints(&[
            constrain(1, 0.0, &[(0, 2), (1, 1)], "", true),
            constrain(1, 0.0, &[(1, 2), (2, 1)], "", true),
            constrain(1, 0.0, &[(2, 2), (3, 1)], "", true),
            constrain(1, 0.0, &[(3, 2), (0, 1)], "", true),
        ])
    )
}

#[test]
fn references_are_read_by_their_index_names() {
    use mitcad_freecad::SubName;
    let sub = |name: &str, mapped: Option<&str>| SubName {
        name: name.to_owned(),
        mapped: mapped.map(str::to_owned),
    };
    // 1.0 writes a sketch's construction line as "Axis" shadowing "Axis0";
    // a topological name stays a hint.
    assert_eq!(elements::sub_name(&sub("Axis", Some("Axis0"))), "Axis0");
    assert_eq!(
        elements::sub_name(&sub("Edge4", Some(";g4;SKT.Edge4"))),
        "Edge4"
    );
    assert_eq!(elements::sub_name(&sub("V_Axis", None)), "V_Axis");
    assert_eq!(
        elements::parse_element("Face12"),
        Some((mitcad_model::ElementKind::Face, 11))
    );
    assert_eq!(elements::parse_element("Edge0"), None);
    assert_eq!(elements::parse_element("Axis1"), None);
}

fn feature<'a>(report: &'a FcstdReport, name: &str) -> &'a FeatureReport {
    report
        .features
        .iter()
        .find(|f| f.object == name)
        .unwrap_or_else(|| panic!("no feature {name}: {:?}", report.features))
}

fn def_of(doc: &Document<MockKernel>, uid: &str) -> serde_json::Value {
    let answer = doc
        .query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
        .unwrap();
    serde_json::from_str::<serde_json::Value>(&answer).unwrap()["def"].clone()
}

#[test]
fn a_body_s_features_are_replayed_in_dependency_order() {
    // The pad comes before its sketch in the file; a feature Mitcad does
    // not translate (a Python feature) falls back to its stored shape, replacing
    // the Body's body, and the second pad joins that; the tip is the
    // second pad, so nothing is rolled back.
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}{}",
                label("Plate"),
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Pad", "Sketch", "Gear", "Sketch2", "Pad2"]
                ),
                link("Tip", "Pad2")
            ),
        ),
        (
            "PartDesign::Pad",
            "Pad",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                float("Length", 10.0),
                enumeration("Type", 0),
                shape("Pad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch",
            rectangle_sketch(0.0, 0.0, 40.0, 30.0),
        ),
        (
            "PartDesign::FeaturePython",
            "Gear",
            format!("{}{}", link("BaseFeature", "Pad"), shape("Gear.brp")),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch2",
            rectangle_sketch(5.0, 5.0, 10.0, 10.0),
        ),
        (
            "PartDesign::Pad",
            "Pad2",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Sketch2", &[]),
                link("BaseFeature", "Gear"),
                float("Length", 5.0),
                shape("Pad2.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"6 body"),
            ("Pad.brp", b"6 pad"),
            ("Gear.brp", b"8 gear"),
            ("Pad2.brp", b"10 pad2"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let order: Vec<&str> = report.features.iter().map(|f| f.object.as_str()).collect();
    assert_eq!(order, ["Pad", "Gear", "Pad2"]);
    let pad = feature(&report, "Pad");
    assert_eq!(pad.outcome, ObjectOutcome::Parametric, "{pad:?}");
    assert_eq!(pad.mitcad, "extrude");
    assert!(
        pad.notes.iter().any(|n| n.contains("does not measure")),
        "{pad:?}"
    );
    // The sketch (F1) before the pad (F2), a new body.
    let def = def_of(&doc, &pad.features[0]);
    assert_eq!(def["type"], "extrude");
    assert_eq!(def["profiles"][0]["sketch"], "F1");
    assert_eq!(def["operation"], "new_body");
    assert_eq!(def["extent"]["type"], "distance", "{def}");
    let length = def["extent"]["distance"].as_str().unwrap();
    let parameters = doc.parameters();
    assert_eq!(
        parameters.find(length).and_then(|id| parameters.value(id)),
        Some(10.0)
    );
    let gear = feature(&report, "Gear");
    assert_eq!(gear.outcome, ObjectOutcome::Fallback, "{gear:?}");
    assert!(gear.notes[0].contains("not translated"), "{gear:?}");
    let base = def_of(&doc, &gear.features[0]);
    assert_eq!(base["replaces"], json!(["F2.b0"]));
    let pad2 = feature(&report, "Pad2");
    let def = def_of(&doc, &pad2.features[0]);
    assert_eq!(def["operation"], "join");
    assert_eq!(def["participants"], json!(["F2.b0"]));
    // One body, the Body's, named after it; the report's items.
    assert_eq!(doc.bodies().len(), 1);
    assert_eq!(item(&report, "Body").outcome, ObjectOutcome::Body);
    assert_eq!(item(&report, "Body").bodies, ["F2.b0"]);
    assert_eq!(doc.body_name("F2.b0".parse().unwrap()), "Plate");
    assert_eq!(item(&report, "Gear").outcome, ObjectOutcome::Fallback);
    assert_eq!(item(&report, "Pad2").outcome, ObjectOutcome::Parametric);
    // Two sketches and two pads.
    assert!(
        report
            .text()
            .contains("4 parametric, 0 partial, 1 fallback"),
        "{}",
        report.text()
    );
}

#[test]
fn features_after_the_tip_are_suppressed_and_part_trees_replayed() {
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad", "Pad2"]),
                link("Tip", "Pad")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch",
            rectangle_sketch(0.0, 0.0, 40.0, 30.0),
        ),
        (
            "PartDesign::Pad",
            "Pad",
            format!(
                "{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                float("Length", 10.0),
                shape("Pad.brp")
            ),
        ),
        (
            "PartDesign::Pad",
            "Pad2",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                link("BaseFeature", "Pad"),
                float("Length", 20.0),
                shape("Pad2.brp")
            ),
        ),
        // A box cut by a cylinder, and by it again: the first cut works on
        // a copy of the cylinder.
        (
            "Part::Box",
            "Box",
            format!(
                "{}{}{}{}",
                float("Length", 10.0),
                float("Width", 10.0),
                float("Height", 10.0),
                shape("Box.brp")
            ),
        ),
        (
            "Part::Cylinder",
            "Tool",
            format!(
                "{}{}{}",
                float("Radius", 2.0),
                float("Height", 20.0),
                shape("Tool.brp")
            ),
        ),
        (
            "Part::Cut",
            "Cut",
            format!(
                "{}{}{}{}",
                label("Holed"),
                link("Base", "Box"),
                link("Tool", "Tool"),
                shape("Cut.brp")
            ),
        ),
        (
            "Part::Cut",
            "Cut2",
            format!(
                "{}{}{}",
                link("Base", "Cut"),
                link("Tool", "Tool"),
                shape("Cut2.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"6 body"),
            ("Pad.brp", b"6 pad"),
            ("Pad2.brp", b"6 pad2"),
            ("Box.brp", b"6 box"),
            ("Tool.brp", b"3 tool"),
            ("Cut.brp", b"7 cut"),
            ("Cut2.brp", b"7 cut2"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let pad2 = feature(&report, "Pad2");
    assert!(
        pad2.notes
            .iter()
            .any(|n| n.contains("after its Body's tip")),
        "{pad2:?}"
    );
    let uid: FeatureUid = pad2.features[0].parse().unwrap();
    assert!(doc.feature(uid).unwrap().suppressed);
    // The Part tree: the box, the cylinder, a copy of it for the first
    // cut, the cuts; the second cut is the body, the box's.
    for name in ["Box", "Tool", "Cut", "Cut2"] {
        assert_eq!(
            feature(&report, name).outcome,
            ObjectOutcome::Parametric,
            "{name}: {:?}",
            feature(&report, name)
        );
    }
    let cut = def_of(&doc, &feature(&report, "Cut").features[0]);
    assert_eq!(cut["type"], "combine");
    assert_eq!(cut["operation"], "cut");
    let copy = cut["tools"][0].as_str().unwrap().to_owned();
    let tool = feature(&report, "Tool").features[0].clone();
    assert_ne!(copy, format!("{tool}.b0"), "the first cut uses a copy");
    let cut2 = def_of(&doc, &feature(&report, "Cut2").features[0]);
    assert_eq!(cut2["target"], cut["target"]);
    assert_eq!(cut2["tools"], json!([format!("{tool}.b0")]));
    assert_eq!(item(&report, "Cut2").outcome, ObjectOutcome::Body);
    assert_eq!(item(&report, "Cut").outcome, ObjectOutcome::Parametric);
    let names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    assert!(names.contains(&"Cut2".to_owned()), "{names:?}");
    assert_eq!(doc.bodies().len(), 2, "{names:?}");
}

fn integer(name: &str, value: i64) -> String {
    prop(
        name,
        "App::PropertyInteger",
        &format!("<Integer value=\"{value}\"/>"),
    )
}

/// The definitions of the Mitcad features made of an object, by type.
fn defs_of(doc: &Document<MockKernel>, report: &FcstdReport, name: &str) -> Vec<serde_json::Value> {
    feature(report, name)
        .features
        .iter()
        .map(|uid| def_of(doc, uid))
        .collect()
}

#[test]
fn helices_primitives_holes_and_multi_transforms_become_features() {
    // A conical helix about its sketch's V axis; a skewed prism and a wedge; a
    // plate with a tapered, counterdrilled hole, and a pocket patterned by
    // a MultiTransform of two linear patterns.
    let xml = document(&[
        (
            "PartDesign::Body",
            "Spring",
            format!(
                "{}{}{}",
                shape("Spring.brp"),
                links("Group", "App::PropertyLinkList", &["Profile", "Helix"]),
                link("Tip", "Helix")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Profile",
            geometry(&[round([8.0, 0.0], 1.0)]),
        ),
        (
            "PartDesign::AdditiveHelix",
            "Helix",
            format!(
                "{}{}{}{}{}{}{}",
                link_sub("Profile", "Profile", &[]),
                link_sub("ReferenceAxis", "Profile", &["V_Axis"]),
                float("Pitch", 5.0),
                float("Height", 20.0),
                float("Angle", 10.0),
                enumeration("Mode", 0),
                shape("Helix.brp")
            ),
        ),
        (
            "PartDesign::Body",
            "Shapes",
            format!(
                "{}{}{}",
                shape("Shapes.brp"),
                links("Group", "App::PropertyLinkList", &["Prism", "Wedge"]),
                link("Tip", "Wedge")
            ),
        ),
        (
            "PartDesign::AdditivePrism",
            "Prism",
            format!(
                "{}{}{}{}{}{}",
                integer("Polygon", 6),
                float("Circumradius", 4.0),
                float("Height", 10.0),
                prop("FirstAngle", "App::PropertyAngle", "<Float value=\"10\"/>"),
                placement([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0),
                shape("Prism.brp")
            ),
        ),
        (
            "PartDesign::AdditiveWedge",
            "Wedge",
            format!(
                "{}{}{}{}{}{}{}{}{}{}{}{}{}",
                float("Xmin", 0.0),
                float("Xmax", 10.0),
                float("Ymin", 0.0),
                float("Ymax", 10.0),
                float("Zmin", 0.0),
                float("Zmax", 10.0),
                float("X2min", 2.0),
                float("X2max", 8.0),
                float("Z2min", 2.0),
                float("Z2max", 8.0),
                link("BaseFeature", "Prism"),
                placement([20.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0),
                shape("Wedge.brp")
            ),
        ),
        (
            "PartDesign::Body",
            "Plate",
            format!(
                "{}{}{}",
                shape("Plate.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &[
                        "PlateSketch",
                        "PlatePad",
                        "HoleSketch",
                        "Hole",
                        "Holes",
                        "AlongX",
                        "AlongY",
                        "Turned",
                        "Around"
                    ]
                ),
                link("Tip", "Turned")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "PlateSketch",
            rectangle_sketch(0.0, 0.0, 60.0, 30.0),
        ),
        (
            "PartDesign::Pad",
            "PlatePad",
            format!(
                "{}{}{}",
                link_sub("Profile", "PlateSketch", &[]),
                float("Length", 15.0),
                shape("PlatePad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "HoleSketch",
            geometry(&[round([8.0, 8.0], 2.0)]),
        ),
        (
            "PartDesign::Hole",
            "Hole",
            format!(
                "{}{}{}{}{}{}{}{}{}{}{}{}{}",
                link_sub("Profile", "HoleSketch", &[]),
                link("BaseFeature", "PlatePad"),
                float("Diameter", 6.0),
                float("Depth", 10.0),
                enumeration("DepthType", 0),
                enumeration("DrillPoint", 0),
                enumeration("HoleCutType", 3),
                float("HoleCutDiameter", 10.0),
                float("HoleCutDepth", 3.0),
                prop(
                    "HoleCutCountersinkAngle",
                    "App::PropertyAngle",
                    "<Float value=\"90\"/>"
                ),
                flag("Tapered", true),
                prop(
                    "TaperedAngle",
                    "App::PropertyAngle",
                    "<Float value=\"85\"/>"
                ),
                shape("Hole.brp")
            ),
        ),
        (
            "PartDesign::MultiTransform",
            "Holes",
            format!(
                "{}{}{}{}",
                links("Originals", "App::PropertyLinkList", &["Hole"]),
                links(
                    "Transformations",
                    "App::PropertyLinkList",
                    &["AlongX", "AlongY"]
                ),
                link("BaseFeature", "Hole"),
                shape("Holes.brp")
            ),
        ),
        (
            "PartDesign::LinearPattern",
            "AlongX",
            format!(
                "{}{}{}",
                link_sub("Direction", "PlateSketch", &["H_Axis"]),
                float("Length", 40.0),
                integer("Occurrences", 3)
            ),
        ),
        (
            "PartDesign::LinearPattern",
            "AlongY",
            format!(
                "{}{}{}",
                link_sub("Direction", "PlateSketch", &["V_Axis"]),
                float("Length", 10.0),
                integer("Occurrences", 2)
            ),
        ),
        // A linear pattern, then a polar one: a pattern of the pattern.
        (
            "PartDesign::MultiTransform",
            "Turned",
            format!(
                "{}{}{}{}",
                links("Originals", "App::PropertyLinkList", &["Hole"]),
                links(
                    "Transformations",
                    "App::PropertyLinkList",
                    &["AlongX", "Around"]
                ),
                link("BaseFeature", "Holes"),
                shape("Turned.brp")
            ),
        ),
        (
            "PartDesign::PolarPattern",
            "Around",
            format!(
                "{}{}{}",
                link_sub("Axis", "PlateSketch", &["V_Axis"]),
                float("Angle", 360.0),
                integer("Occurrences", 4)
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Spring.brp", b"3 spring"),
            ("Helix.brp", b"3 helix"),
            ("Shapes.brp", b"6 shapes"),
            ("Prism.brp", b"8 prism"),
            ("Wedge.brp", b"6 wedge"),
            ("Plate.brp", b"6 plate"),
            ("PlatePad.brp", b"6 pad"),
            ("Hole.brp", b"9 hole"),
            ("Holes.brp", b"20 holes"),
            ("Turned.brp", b"40 turned"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    // The helix: the profile turned about the sketch's V axis, 4 turns of 5,
    // widening at 10 degrees.
    let helix = feature(&report, "Helix");
    assert_eq!(helix.outcome, ObjectOutcome::Parametric, "{helix:?}");
    assert_eq!(helix.mitcad, "helix");
    let def = &defs_of(&doc, &report, "Helix")[0];
    assert_eq!(def["type"], "helix");
    assert_eq!(def["axis"]["sketch"], def["profiles"][0]["sketch"], "{def}");
    let parameters = doc.parameters();
    let value = |v: &serde_json::Value| {
        v.as_f64().unwrap_or_else(|| {
            let name = v.as_str().unwrap();
            parameters
                .find(name)
                .and_then(|id| parameters.value(id))
                .unwrap()
        })
    };
    assert_eq!(value(&def["pitch"]), 5.0);
    assert_eq!(value(&def["revolutions"]), 4.0);
    assert_eq!(def["operation"], "new_body");
    // Conical: a growth of the pitch times the tangent of the angle per
    // turn, built as FreeCAD builds it (mitcad#83).
    assert!((value(&def["growth"]) - 5.0 * 10f64.to_radians().tan()).abs() < 1e-9);
    assert_eq!(def["construction"], "freecad");
    // The skewed prism: its hexagon swept along the skew without turning.
    let prism = feature(&report, "Prism");
    assert_eq!(prism.mitcad, "sweep", "{prism:?}");
    let defs = defs_of(&doc, &report, "Prism");
    let types: Vec<&str> = defs.iter().map(|d| d["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["construction_plane", "sketch", "sweep"]);
    assert_eq!(defs[2]["orientation"], "parallel");
    let end = defs[2]["path"]["end"].as_array().unwrap();
    assert!((end[0].as_f64().unwrap() - 10.0 * 10f64.to_radians().tan()).abs() < 1e-9);
    // The wedge: a ruled loft between its rectangles, joined to the prism.
    let defs = defs_of(&doc, &report, "Wedge");
    let loft = defs.last().unwrap();
    assert_eq!(loft["type"], "loft");
    assert_eq!(loft["ruled"], true);
    assert_eq!(loft["operation"], "join");
    assert_eq!(loft["sections"].as_array().unwrap().len(), 2);
    // The hole: counterdrilled, its wall at 5 degrees to the axis.
    let def = &defs_of(&doc, &report, "Hole")[0];
    assert_eq!(def["kind"]["type"], "counterdrill", "{def}");
    assert!((value(&def["taper"]) - 5f64.to_radians()).abs() < 1e-12);
    assert_eq!(def["flat"], true);
    // The MultiTransform: one rectangular pattern of two directions; its
    // transformations are no features of their own.
    let holes = feature(&report, "Holes");
    assert_eq!(holes.outcome, ObjectOutcome::Parametric, "{holes:?}");
    let def = &defs_of(&doc, &report, "Holes")[0];
    assert_eq!(def["type"], "rectangular_pattern");
    assert_eq!(value(&def["direction1"]["quantity"]), 3.0);
    assert_eq!(value(&def["direction2"]["quantity"]), 2.0);
    assert_eq!(value(&def["direction2"]["distance"]).abs(), 10.0);
    assert!(report.features.iter().all(|f| f.object != "AlongX"));
    assert_eq!(item(&report, "AlongX").outcome, ObjectOutcome::Included);
    // The linear pattern, then the polar one: a circular pattern of the
    // rectangular pattern (a pattern of a pattern).
    let turned = feature(&report, "Turned");
    assert_eq!(turned.outcome, ObjectOutcome::Parametric, "{turned:?}");
    assert_eq!(turned.mitcad, "circular_pattern");
    let defs = defs_of(&doc, &report, "Turned");
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[0]["type"], "rectangular_pattern");
    let hole = &feature(&report, "Hole").features[0];
    assert_eq!(defs[0]["objects"]["features"], json!([hole]));
    assert_eq!(defs[1]["type"], "circular_pattern");
    assert_eq!(defs[1]["objects"]["features"], json!([turned.features[0]]));
    assert_eq!(value(&defs[1]["quantity"]), 4.0);
}

#[test]
fn counts_round_and_cone_sizes_follow_their_expressions() {
    // A pattern of Count / 2 bosses (FreeCAD rounds 2.5 to 3) and a cone
    // whose radius a cell drives (its section's dimension).
    let xml = document(&[
        (
            "Spreadsheet::Sheet",
            "Spreadsheet",
            cells(&[("B1", "5", "Count"), ("B2", "=6 mm", "Radius")]),
        ),
        (
            "PartDesign::Body",
            "Plate",
            format!(
                "{}{}{}",
                shape("Plate.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["PlateSketch", "PlatePad", "BossSketch", "Boss", "Bosses"]
                ),
                link("Tip", "Bosses")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "PlateSketch",
            rectangle_sketch(0.0, 0.0, 60.0, 30.0),
        ),
        (
            "PartDesign::Pad",
            "PlatePad",
            format!(
                "{}{}{}",
                link_sub("Profile", "PlateSketch", &[]),
                float("Length", 5.0),
                shape("PlatePad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "BossSketch",
            geometry(&[round([6.0, 10.0], 2.0)]),
        ),
        (
            "PartDesign::Pad",
            "Boss",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "BossSketch", &[]),
                link("BaseFeature", "PlatePad"),
                float("Length", 8.0),
                shape("Boss.brp")
            ),
        ),
        (
            "PartDesign::LinearPattern",
            "Bosses",
            format!(
                "{}{}{}{}{}{}",
                links("Originals", "App::PropertyLinkList", &["Boss"]),
                link_sub("Direction", "PlateSketch", &["H_Axis"]),
                float("Length", 40.0),
                prop(
                    "Occurrences",
                    "App::PropertyIntegerConstraint",
                    "<Integer value=\"3\"/>"
                ),
                expressions(&[("Occurrences", "Spreadsheet.Count / 2")]),
                shape("Bosses.brp")
            ),
        ),
        (
            "PartDesign::Body",
            "Coned",
            format!(
                "{}{}{}",
                shape("Coned.brp"),
                links("Group", "App::PropertyLinkList", &["ConeAdd"]),
                link("Tip", "ConeAdd")
            ),
        ),
        (
            "PartDesign::AdditiveCone",
            "ConeAdd",
            format!(
                "{}{}{}{}{}{}",
                float("Radius1", 6.0),
                float("Radius2", 2.0),
                float("Height", 9.0),
                expressions(&[("Radius1", "Spreadsheet.Radius")]),
                placement([0.0, 40.0, 0.0], [0.0, 0.0, 1.0], 0.0),
                shape("ConeAdd.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Plate.brp", b"6 plate"),
            ("PlatePad.brp", b"6 pad"),
            ("Boss.brp", b"9 boss"),
            ("Bosses.brp", b"15 bosses"),
            ("Coned.brp", b"3 cone"),
            ("ConeAdd.brp", b"3 cone"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let row = expression_row(&report, "Bosses", "Occurrences");
    assert_eq!(row.outcome, ExpressionOutcome::Expression, "{row:?}");
    assert_eq!(row.mitcad.as_deref(), Some("round(Count / 2)"));
    let def = &defs_of(&doc, &report, "Bosses")[0];
    let quantity = def["direction1"]["quantity"].as_str().unwrap();
    let id = doc.parameters().find(quantity).unwrap();
    assert_eq!(
        doc.parameters().get(id).unwrap().expression(),
        "round(Count / 2)"
    );
    assert_eq!(doc.parameters().value(id), Some(3.0));
    // The cone's section: its first radius the parameter, the others
    // numbers, as dimensions of its sides.
    let row = expression_row(&report, "ConeAdd", "Radius1");
    assert_eq!(row.outcome, ExpressionOutcome::Expression, "{row:?}");
    let section = &defs_of(&doc, &report, "ConeAdd")[1];
    assert_eq!(section["type"], "sketch");
    let values: Vec<&serde_json::Value> = section["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| &d["value"])
        .collect();
    assert_eq!(values[0], "Radius", "{section}");
    assert_eq!(values.len(), 3, "{section}");
}

// Parameters and expressions (stage 4).

/// Text in an XML attribute.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// An `ExpressionEngine` of (path, expression).
fn expressions(items: &[(&str, &str)]) -> String {
    let list: String = items
        .iter()
        .map(|(p, e)| {
            format!(
                "<Expression path=\"{}\" expression=\"{}\"/>",
                escape(p),
                escape(e)
            )
        })
        .collect();
    prop(
        "ExpressionEngine",
        "App::PropertyExpressionEngine",
        &format!(
            "<ExpressionEngine count=\"{}\">{list}</ExpressionEngine>",
            items.len()
        ),
    )
}

/// A spreadsheet's cells: (address, content, alias).
fn cells(items: &[(&str, &str, &str)]) -> String {
    let list: String = items
        .iter()
        .map(|(a, c, alias)| {
            let alias = if alias.is_empty() {
                String::new()
            } else {
                format!(" alias=\"{alias}\"")
            };
            format!("<Cell address=\"{a}\" content=\"{}\"{alias}/>", escape(c))
        })
        .collect();
    prop(
        "cells",
        "Spreadsheet::PropertySheet",
        &format!("<Cells Count=\"{}\">{list}</Cells>", items.len()),
    )
}

/// A VarSet's property (a dynamic one: it has a group).
fn variable(name: &str, kind: &str, value: &str) -> String {
    format!(
        "<Property name=\"{name}\" type=\"App::Property{kind}\" group=\"Base\" doc=\"\" attr=\"0\" ro=\"0\" hide=\"0\">{value}</Property>"
    )
}

fn parameter<'a>(report: &'a FcstdReport, name: &str) -> &'a ParameterReport {
    report
        .parameters
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no parameter {name}: {:?}", report.parameters))
}

fn expression_row<'a>(report: &'a FcstdReport, object: &str, path: &str) -> &'a ExpressionReport {
    report
        .expressions
        .iter()
        .find(|e| e.object == object && e.path == path)
        .unwrap_or_else(|| panic!("no expression {object}.{path}: {:?}", report.expressions))
}

/// A document of a spreadsheet, a VarSet, a sketch with a named and a
/// bound dimension, and two pads: one bound to a cell, one to the first's
/// length.
fn expression_document() -> FcstdFile {
    let sketch = {
        let p = [[0.0, 0.0], [40.0, 0.0], [40.0, 20.0], [0.0, 20.0]];
        format!(
            "{}{}{}",
            geometry(&[
                segment(p[0], p[1], false),
                segment(p[1], p[2], false),
                segment(p[2], p[3], false),
                segment(p[3], p[0], false),
            ]),
            constraints(&[
                constrain(1, 0.0, &[(0, 2), (1, 1)], "", true),
                constrain(1, 0.0, &[(1, 2), (2, 1)], "", true),
                constrain(1, 0.0, &[(2, 2), (3, 1)], "", true),
                constrain(1, 0.0, &[(3, 2), (0, 1)], "", true),
                constrain(1, 0.0, &[(0, 1), (-1, 1)], "", true),
                constrain(2, 0.0, &[(0, 0)], "", true),
                constrain(2, 0.0, &[(2, 0)], "", true),
                constrain(3, 0.0, &[(1, 0)], "", true),
                constrain(3, 0.0, &[(3, 0)], "", true),
                constrain(7, 40.0, &[(0, 1), (0, 2)], "width", true),
                constrain(8, 20.0, &[(1, 1), (1, 2)], "", true),
            ]),
            expressions(&[
                (".Constraints.width", "Spreadsheet.Width"),
                ("Constraints[10]", "<<Sheet>>.Half"),
            ])
        )
    };
    let xml = document(&[
        (
            "Spreadsheet::Sheet",
            "Spreadsheet",
            format!(
                "{}{}",
                label("Sheet"),
                cells(&[
                    ("A1", "'Width", ""),
                    ("B1", "=40 mm", "Width"),
                    ("B2", "=Width / 2", "Half"),
                    ("B3", "=12", ""),
                    ("B4", "=hypot(3 mm; 4 mm) * 2", "Diag"),
                    ("B5", "=10 kg", "Mass"),
                ])
            ),
        ),
        (
            "App::VarSet",
            "VarSet",
            format!(
                "{}{}{}",
                variable("Depth", "Length", "<Float value=\"7\"/>"),
                variable("Twice", "Length", "<Float value=\"14\"/>"),
                expressions(&[("Twice", "Depth * 2")])
            ),
        ),
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad", "Pad2"]),
                link("Tip", "Pad2")
            ),
        ),
        ("Sketcher::SketchObject", "Sketch", sketch),
        (
            "PartDesign::Pad",
            "Pad",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                float("Length", 12.0),
                expressions(&[("Length", "Spreadsheet.B3")]),
                shape("Pad.brp")
            ),
        ),
        (
            "PartDesign::Pad",
            "Pad2",
            format!(
                "{}{}{}{}{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                link("BaseFeature", "Pad"),
                float("Length", 6.0),
                float("Offset", 0.0),
                float("Length2", 15.0),
                expressions(&[
                    ("Length", "Pad.Length / 2"),
                    ("Offset", "create(<<vector>>; 1; 2; 3).x"),
                    ("Length2", "VarSet.Twice + 1 mm"),
                ]),
                shape("Pad2.brp")
            ),
        ),
    ]);
    fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"6 body"),
            ("Pad.brp", b"6 pad"),
            ("Pad2.brp", b"6 pad2"),
        ],
    )
}

#[test]
fn spreadsheets_varsets_and_constraints_become_parameters() {
    let (doc, report) = import_with(
        expression_document(),
        Vec::new(),
        MockKernel::default(),
        false,
    );
    let expression = |name: &str| parameter(&report, name).expression.clone();
    // Aliases, a cell named by its address (after the sheet's label), a
    // VarSet's properties.
    assert_eq!(expression("Width"), "40 mm");
    assert_eq!(expression("Half"), "Width / 2");
    assert_eq!(expression("Sheet_B3"), "12");
    assert_eq!(expression("Diag"), "sqrt((3 mm)^2 + (4 mm)^2) * 2");
    assert_eq!(parameter(&report, "Diag").value, Some(10.0));
    assert_eq!(expression("Depth"), "7 mm");
    assert_eq!(expression("Twice"), "Depth * 2");
    assert_eq!(
        parameter(&report, "Twice").outcome,
        ExpressionOutcome::Parameter
    );
    // A cell of another kind of quantity: none.
    assert_eq!(
        parameter(&report, "Mass").outcome,
        ExpressionOutcome::Skipped
    );
    // The named constraint is a parameter of its own, the sketch's.
    assert_eq!(expression("width"), "Width");
    let width = doc.parameters().find("width").unwrap();
    let sketch_uid: FeatureUid = report.sketches[0]
        .feature
        .as_deref()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(doc.parameters().owner(width), Some(sketch_uid));
    let def = sketch_def(&doc, &report, "Sketch");
    let dims = def["dimensions"].as_array().unwrap();
    assert_eq!(dims[0]["value"], "width");
    // The unnamed bound constraint: its expression (a parameter by name is
    // that parameter).
    assert_eq!(dims[1]["value"], "Half");
    assert_eq!(
        expression_row(&report, "Sketch", "Constraints[10]").outcome,
        ExpressionOutcome::Expression
    );
    // The pad's length (a cell without a unit gets FreeCAD's millimetres)
    // is a parameter, since the second pad refers to it.
    assert_eq!(expression("Pad_Length"), "Sheet_B3 * 1 mm");
    let pad = def_of(&doc, &feature(&report, "Pad").features[0]);
    assert_eq!(pad["extent"]["distance"], "Pad_Length");
    let pad_uid: FeatureUid = feature(&report, "Pad").features[0].parse().unwrap();
    let id = doc.parameters().find("Pad_Length").unwrap();
    assert_eq!(doc.parameters().owner(id), Some(pad_uid));
    let pad2 = def_of(&doc, &feature(&report, "Pad2").features[0]);
    let length = pad2["extent"]["distance"].as_str().unwrap();
    let id = doc.parameters().find(length).unwrap();
    assert_eq!(
        doc.parameters().get(id).unwrap().expression(),
        "Pad_Length / 2"
    );
    // What could not be translated keeps FreeCAD's value and says why;
    // what the import does not carry over is unused.
    let offset = expression_row(&report, "Pad2", "Offset");
    assert_eq!(offset.outcome, ExpressionOutcome::Value, "{offset:?}");
    assert!(
        offset.note.as_deref().unwrap().contains("not read"),
        "{offset:?}"
    );
    assert_eq!(
        expression_row(&report, "Pad2", "Length2").outcome,
        ExpressionOutcome::Unused
    );
    let text = report.text();
    assert!(
        text.contains("parameter width (Sketch constraint width): parameter Width"),
        "{text}"
    );
    // One undo step still.
    assert_eq!(doc.undo_depth(), 1);
}

#[test]
fn functions_units_and_precedence_translate() {
    // (alias, FreeCAD's formula, Mitcad's text, value in mm, rad or as is)
    let pi = std::f64::consts::PI;
    let cases: &[(&str, &str, &str, f64)] = &[
        ("A", "=2", "2", 2.0),
        ("B", "=3", "3", 3.0),
        ("SignFirst", "=-2 ^ 2", "(-2) ^ 2", 4.0),
        ("LeftPowers", "=2 ^ 3 ^ 2", "(2 ^ 3) ^ 2", 64.0),
        ("Modulo", "=mod(7; 3)", "7 % 3", 1.0),
        (
            "Truncated",
            "=trunc(-2.5)",
            "if(-2.5 < 0; ceil(-2.5); floor(-2.5))",
            -2.0,
        ),
        ("Logs", "=log(e) + log10(100)", "ln(E) + log(100)", 3.0),
        ("Sum", "=sum(A:B)", "", 0.0),
        ("Range", "=sum(A1:A2) * 1 mm", "(A + B) * 1 mm", 5.0),
        ("Mean", "=average(A1:A2)", "(A + B) / 2", 2.5),
        (
            "Chosen",
            "=A > 1 ? 10 mm : 20 mm",
            "if(A > 1; 10 mm; 20 mm)",
            10.0,
        ),
        ("Imperial", "=1 in + 2 ft", "1 in + 2 ft", 25.4 + 609.6),
        ("Decimetre", "=0.5 dm", "5 cm", 50.0),
        ("Turn", "=100 gon", "100 grad", pi / 2.0),
        ("Minutes", "=90 ′", "1.5 deg", 1.5f64.to_radians()),
        (
            "Cath",
            "=cath(5 mm; 3 mm)",
            "sqrt((5 mm)^2 - (3 mm)^2)",
            4.0,
        ),
        (
            "Cube",
            "=cbrt(-8)",
            "if(-8 < 0; -abs(-8)^(1 / 3); abs(-8)^(1 / 3))",
            -2.0,
        ),
        (
            "Biggest",
            "=max(1 mm; 3 mm; 2 mm)",
            "max(1 mm; 3 mm; 2 mm)",
            3.0,
        ),
    ];
    let mut items: Vec<(String, String, String)> = Vec::new();
    for (i, (alias, formula, _, _)) in cases.iter().enumerate() {
        items.push((
            format!("A{}", i + 1),
            (*formula).to_owned(),
            (*alias).to_owned(),
        ));
    }
    let refs: Vec<(&str, &str, &str)> = items
        .iter()
        .map(|(a, f, n)| (a.as_str(), f.as_str(), n.as_str()))
        .collect();
    let xml = document(&[("Spreadsheet::Sheet", "Sheet", cells(&refs))]);
    let (_, report) = import_with(
        fcstd(&xml, None, &[]),
        Vec::new(),
        MockKernel::default(),
        false,
    );
    for (alias, _, text, value) in cases {
        let p = parameter(&report, alias);
        if text.is_empty() {
            // Not FreeCAD's syntax: no parameter, and why.
            assert_eq!(p.outcome, ExpressionOutcome::Skipped, "{p:?}");
            assert!(p.note.as_deref().unwrap().contains("not read"), "{p:?}");
            continue;
        }
        assert_eq!(p.expression, *text, "{alias}");
        let got = p.value.unwrap();
        assert!(
            (got - value).abs() <= 1e-12 * value.abs().max(1.0),
            "{alias}: {got} against {value}"
        );
    }
}

/// The expression document imported, then parameters changed.
fn import_changed(changes: &[(&str, &str)]) -> (Document<MockKernel>, FcstdReport) {
    let mut doc = Document::new(MockKernel::default());
    let mut loader =
        |p: &Path| -> Result<FcstdFile, String> { Err(format!("{}: not found", p.display())) };
    let options = FcstdOptions {
        set_parameters: changes
            .iter()
            .map(|(n, e)| ((*n).to_owned(), (*e).to_owned()))
            .collect(),
        ..FcstdOptions::default()
    };
    let report = import_fcstd_file(
        &mut doc,
        expression_document(),
        Path::new("/models/test.FCStd"),
        &mut loader,
        &options,
    )
    .unwrap();
    (doc, report)
}

#[test]
fn parameters_changed_after_the_import() {
    let (doc, report) = import_changed(&[("Width", "50 mm")]);
    assert_eq!(parameter(&report, "Width").value, Some(50.0));
    assert_eq!(parameter(&report, "Half").value, Some(25.0));
    let width = doc.parameters().find("width").unwrap();
    assert_eq!(doc.parameters().value(width), Some(50.0));
    // A change that does not apply is a warning.
    let (_, report) = import_changed(&[("Nothing", "1 mm")]);
    assert!(
        report.warnings.iter().any(|w| w.starts_with("Nothing")),
        "{:?}",
        report.warnings
    );
}
