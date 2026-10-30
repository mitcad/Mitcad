// SPDX-License-Identifier: MIT
//! The FreeCAD import on small hand-made documents with the model's mock
//! kernel: a shape file `"<faces> <name>"` is a mock body with that many
//! faces in a 10 mm box from the origin.

use std::path::Path;

use mitcad_freecad::FcstdFile;
use mitcad_model::testing::MockKernel;
use mitcad_model::{BoundingBox, Document, FeatureDef, FeatureUid, Kernel, Transform};
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
    // A shape brought in as FreeCAD stored it: where FreeCAD's measures are
    // those of the kernel's fixed-point integration of it, which Mitcad's
    // refine, a difference is none of the import (mitcad#139); where they
    // are not (another volume or centre), it is. The mock kernel has no
    // such measures: none, no excuse.
    report.placed[0].volume = 1000.0;
    report.placed[0].area = 600.0;
    let dump = json!({"format": "mitcad-freecad-dump", "objects": [
        {"name": "Box", "shape": {"solids": 1, "volume": 1000.002, "area": 600.0001,
                                  "world_center": [0.0, 0.0, 1.0], "faces": 6}}]});
    assert!(report.placed[0].fixed.is_none());
    assert!(!reference::check(&report, &dump).pass);
    report.placed[0].fixed = Some(report::StoredMeasures {
        volume: 1000.002,
        area: 600.0001,
        center: [0.0, 0.0, 1.0],
    });
    for (volume, center, pass, excused) in [
        (1000.002, [0.0, 0.0, 1.0], true, 1),
        (1000.001, [0.0, 0.0, 1.0], false, 0),
        (1000.002, [0.0, 0.0, 1.001], false, 0),
    ] {
        let dump = json!({"format": "mitcad-freecad-dump", "objects": [
            {"name": "Box", "shape": {"solids": 1, "volume": volume, "area": 600.0001,
                                      "world_center": center, "faces": 6}}]});
        let r = reference::check(&report, &dump);
        assert!(r.pass == pass && r.not_compared.len() == excused, "{r:?}");
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
    def_json(doc, uid)
}

/// A feature id of the report.
fn feature_uid(uid: &str) -> FeatureUid {
    uid.parse()
        .unwrap_or_else(|_| panic!("not a feature id: {uid}"))
}

/// A feature's definition as JSON with its values by their parameters'
/// names: what the api's `feature` query answers as `def`, from the model
/// itself, so that these tests do without the api (mitcad#831).
fn def_json(doc: &Document<MockKernel>, uid: &str) -> serde_json::Value {
    let entry = doc
        .feature(feature_uid(uid))
        .unwrap_or_else(|| panic!("feature {uid} does not exist"));
    let params = doc.parameters();
    let def = entry
        .def
        .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
        .expect("names never fail");
    serde_json::to_value(&def).unwrap()
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

#[test]
fn a_hole_s_circles_and_wall_are_found_wherever_their_seams_are() {
    // (mitcad#835) FreeCAD's hole starts its circles on the sketch's x
    // axis, Mitcad's revolved hole on another side: the fillet on the
    // hole's edges (or its wall) must find them all the same.
    use mitcad_model::Curve3;
    use std::f64::consts::TAU;
    let circle = |z: f64, normal: [f64; 3], x_axis: [f64; 3], r: f64, end: f64| Curve3::Conic {
        center: [15.0, 10.0, z],
        normal,
        x_axis,
        major: r,
        minor: r,
        start: 0.0,
        end,
        closed: end >= TAU,
    };
    let line = |start: [f64; 3], end: [f64; 3]| Curve3::Line { start, end };
    let theirs = circle(0.0, [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], 4.5, TAU);
    let ours = circle(0.0, [0.0, 0.0, 1.0], [0.0, -1.0, 0.0], 4.5, TAU);
    assert!(elements::same_curves(
        std::slice::from_ref(&theirs),
        std::slice::from_ref(&ours)
    ));
    // Not another radius, height or a half circle.
    for other in [
        circle(0.0, [0.0, 0.0, 1.0], [0.0, -1.0, 0.0], 4.6, TAU),
        circle(1.0, [0.0, 0.0, 1.0], [0.0, -1.0, 0.0], 4.5, TAU),
        circle(0.0, [0.0, 0.0, 1.0], [0.0, -1.0, 0.0], 4.5, TAU / 2.0),
        circle(0.0, [1.0, 0.0, 0.0], [0.0, -1.0, 0.0], 4.5, TAU),
    ] {
        assert!(!elements::same_curves(
            std::slice::from_ref(&theirs),
            std::slice::from_ref(&other)
        ));
    }
    // The wall: two circles and a seam, at +x in FreeCAD's, at -y in
    // Mitcad's (in another order and the other way along).
    let wall_theirs = [
        circle(15.0, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 4.5, TAU),
        line([19.5, 10.0, 0.0], [19.5, 10.0, 15.0]),
        theirs.clone(),
    ];
    let wall_ours = [
        ours.clone(),
        circle(15.0, [0.0, 0.0, -1.0], [0.0, -1.0, 0.0], 4.5, TAU),
        line([15.0, 5.5, 15.0], [15.0, 5.5, 0.0]),
    ];
    assert!(elements::same_curves(&wall_theirs, &wall_ours));
    // Not a line across the wall, nor one off the circles.
    let mut across = wall_ours.clone();
    across[2] = line([15.0, 5.5, 15.0], [10.5, 10.0, 0.0]);
    assert!(!elements::same_curves(&wall_theirs, &across));
    let mut outside = wall_ours.clone();
    outside[2] = line([15.0, 5.0, 15.0], [15.0, 5.0, 0.0]);
    assert!(!elements::same_curves(&wall_theirs, &outside));
}

fn feature<'a>(report: &'a FcstdReport, name: &str) -> &'a FeatureReport {
    report
        .features
        .iter()
        .find(|f| f.object == name)
        .unwrap_or_else(|| panic!("no feature {name}: {:?}", report.features))
}

fn def_of(doc: &Document<MockKernel>, uid: &str) -> serde_json::Value {
    def_json(doc, uid)
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
    // A scripted feature: no parametric counterpart (mitcad#539), its
    // type named.
    assert_eq!(gear.fallback_group, Some(FallbackGroup::NoCounterpart));
    assert!(
        gear.notes[0].contains("no parametric counterpart: a scripted object")
            && gear.notes[0].contains("PartDesign::FeaturePython"),
        "{gear:?}"
    );
    let base = def_of(&doc, &gear.features[0]);
    assert_eq!(base["replaces"], json!(["F2.b0"]));
    // The timeline says what the report says (mitcad#170).
    let note = doc
        .import_note(gear.features[0].parse().unwrap())
        .unwrap_or_default();
    assert_eq!(
        note,
        format!("Stands in for {} ({})", gear.label, gear.notes.join("; ")),
        "{gear:?}"
    );
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
        // `Sum` is a function name in any case (mitcad#663): the alias gets
        // its sheet's name before it, as a taken name does.
        let p = if *alias == "Sum" {
            let p = parameter(&report, "Sheet_Sum");
            assert!(report.parameters.iter().all(|p| p.name != "Sum"));
            assert!(p.describe.ends_with("(Sum)"), "{p:?}");
            p
        } else {
            parameter(&report, alias)
        };
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

#[test]
fn a_suppressed_feature_becomes_a_placeholder() {
    // mitcad#170: the suppressed feature is not replayed; a suppressed
    // placeholder named after it stands at its place with the report's
    // note, and the pad after it continues on the pad before it.
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
                    &["Sketch", "Pad", "Gear", "Sketch2", "Pad2"]
                ),
                link("Tip", "Pad2")
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
                "{}{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                float("Length", 10.0),
                enumeration("Type", 0),
                shape("Pad.brp")
            ),
        ),
        (
            "PartDesign::FeaturePython",
            "Gear",
            format!(
                "{}{}{}",
                link("BaseFeature", "Pad"),
                flag("Suppressed", true),
                shape("Gear.brp")
            ),
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
            ("Gear.brp", b"6 pad"),
            ("Pad2.brp", b"10 pad2"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let gear = feature(&report, "Gear");
    assert_eq!(gear.outcome, ObjectOutcome::Skipped, "{gear:?}");
    assert!(
        gear.notes[0].starts_with("suppressed in FreeCAD"),
        "{gear:?}"
    );
    let placeholder = doc
        .features()
        .find(|f| f.name == "Gear")
        .unwrap_or_else(|| panic!("no placeholder: {:?}", doc.features().collect::<Vec<_>>()));
    assert_eq!(placeholder.def.type_name(), "placeholder");
    assert!(placeholder.suppressed);
    assert_eq!(
        doc.import_note(placeholder.uid),
        Some(mitcad_import_base::notes::sentence(&gear.notes[0]).as_str())
    );
    // Between the pads.
    let names: Vec<&str> = doc.features().map(|f| f.name.as_str()).collect();
    let at = |name: &str| names.iter().position(|n| *n == name).unwrap();
    let pad = feature(&report, "Pad");
    let pad2 = feature(&report, "Pad2");
    let named = |uid: &str| doc.feature(uid.parse().unwrap()).unwrap().name.clone();
    assert!(at(&named(&pad.features[0])) < at("Gear"), "{names:?}");
    assert!(at("Gear") < at(&named(&pad2.features[0])), "{names:?}");
}

/// A named reference constraint's parameter (`height`) becomes its driven
/// dimension's reference (mitcad#250), owned by the sketch with the value
/// the sketch measures, instead of a user parameter beside a reference of
/// the dimension's own; the named driving length keeps its parameter.
#[test]
fn a_reference_constraints_parameter_is_its_dimensions_reference() {
    use mitcad_model::expr::ParamKind;
    let profile = format!(
        "{}{}",
        geometry(&[
            segment([0.0, 0.0], [40.0, 0.0], false),
            segment([40.0, 0.0], [40.0, 20.0], false),
        ]),
        constraints(&[
            constrain(1, 0.0, &[(0, 1), (-1, 1)], "", true),
            constrain(1, 0.0, &[(0, 2), (1, 1)], "", true),
            constrain(2, 0.0, &[(0, 0)], "", true),
            constrain(3, 0.0, &[(1, 0)], "", true),
            constrain(6, 40.0, &[(0, 0)], "width", true),
            constrain(8, 20.0, &[(1, 0)], "height", false),
        ])
    );
    let xml = document(&[("Sketcher::SketchObject", "Profile", profile)]);
    let (doc, report) = import_with(
        fcstd(&xml, None, &[]),
        Vec::new(),
        MockKernel::default(),
        false,
    );
    let s = &report.sketches[0];
    assert!(s.feature.is_some(), "{s:?}");
    assert!(s.dropped.is_empty(), "{s:?}");
    let def = sketch_def(&doc, &report, "Profile");
    let dims = def["dimensions"].as_array().unwrap();
    let driven = dims.iter().find(|d| d["driven"] == true).unwrap();
    assert_eq!(driven["reference"], "height", "{def}");
    let params = doc.parameters();
    let height = params.find("height").unwrap();
    assert_eq!(params.get(height).unwrap().kind(), ParamKind::Reference);
    assert_eq!(params.owner(height).map(|o| o.to_string()), s.feature);
    assert!((params.value(height).unwrap() - 20.0).abs() < 1e-9);
    let references = params
        .iter()
        .filter(|p| p.kind() == ParamKind::Reference)
        .count();
    assert_eq!(references, 1);
    let width = params.find("width").unwrap();
    assert_eq!(params.get(width).unwrap().kind(), ParamKind::Model);
}

// Part extrusions of faces and into faces, and the fallbacks' groups
// (mitcad#539).

fn vector(name: &str, v: [f64; 3]) -> String {
    prop(
        name,
        "App::PropertyVector",
        &format!(
            "<PropertyVector valueX=\"{}\" valueY=\"{}\" valueZ=\"{}\"/>",
            v[0], v[1], v[2]
        ),
    )
}

/// A definition's value: a number, or its parameter's value.
fn number(doc: &Document<MockKernel>, v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or_else(|| {
        let name = v.as_str().unwrap_or_else(|| panic!("no value: {v}"));
        let parameters = doc.parameters();
        parameters
            .find(name)
            .and_then(|id| parameters.value(id))
            .unwrap_or_else(|| panic!("no parameter {name}"))
    })
}

/// An open polyline sketch of two lines.
fn open_sketch() -> String {
    format!(
        "{}{}",
        geometry(&[
            segment([0.0, 0.0], [10.0, 0.0], false),
            segment([10.0, 0.0], [15.0, 8.0], false),
            segment([0.0, 0.0], [0.0, 5.0], true),
        ]),
        constraints(&[constrain(1, 0.0, &[(0, 2), (1, 1)], "", true)])
    )
}

/// A Part::Extrusion of `base` (DirMode by index: 0 Custom, 2 Normal).
fn part_extrusion(base: &str, solid: bool, mode: i64, dir: [f64; 3], extra: &str) -> String {
    format!(
        "{}{}{}{}{}{}",
        link("Base", base),
        flag("Solid", solid),
        enumeration("DirMode", mode),
        vector("Dir", dir),
        extra,
        shape(&format!("{base}Extrusion.brp"))
    )
}

#[test]
fn part_extrusions_of_faces_and_into_faces_become_features() {
    let face_maker = prop(
        "FaceMakerClass",
        "App::PropertyString",
        "<String value=\"Part::FaceMakerBullseye\"/>",
    );
    let xml = document(&[
        // An open polyline extruded into faces (Solid off) 15 mm up.
        ("Sketcher::SketchObject", "Open", open_sketch()),
        (
            "Part::Extrusion",
            "Wall",
            part_extrusion("Open", false, 0, [0.0, 0.0, 15.0], ""),
        ),
        // A face of a square sketch extruded 8 mm down and 3 up.
        (
            "Sketcher::SketchObject",
            "Square",
            rectangle_sketch(0.0, 0.0, 10.0, 10.0),
        ),
        (
            "Part::Face",
            "Face",
            format!(
                "{}{}{}",
                links("Sources", "App::PropertyLinkList", &["Square"]),
                face_maker,
                shape("Face.brp")
            ),
        ),
        (
            "Part::Extrusion",
            "Slab",
            part_extrusion(
                "Face",
                false,
                0,
                [0.0, 0.0, -1.0],
                &format!("{}{}", float("LengthFwd", 8.0), float("LengthRev", 3.0)),
            ),
        ),
        // A scripted object's face (no counterpart) extruded 5 mm
        // symmetric about it along its normal.
        ("Part::Part2DObjectPython", "Bolt", shape("Bolt.brp")),
        (
            "Part::Extrusion",
            "Bore",
            part_extrusion(
                "Bolt",
                true,
                2,
                [0.0, 0.0, 1.0],
                &format!("{}{}", float("LengthFwd", 5.0), flag("Symmetric", true)),
            ),
        ),
        // A tapered extrusion of a face (mitcad#541), and a face made by a
        // face maker Mitcad has not.
        (
            "Sketcher::SketchObject",
            "Other",
            rectangle_sketch(20.0, 0.0, 10.0, 10.0),
        ),
        (
            "Part::Face",
            "Face2",
            format!(
                "{}{}",
                links("Sources", "App::PropertyLinkList", &["Other"]),
                shape("Face2.brp")
            ),
        ),
        (
            "Part::Extrusion",
            "Tapered",
            part_extrusion(
                "Face2",
                true,
                2,
                [0.0, 0.0, 1.0],
                &format!("{}{}", float("LengthFwd", 5.0), float("TaperAngle", 5.0)),
            ),
        ),
        // Nested wires (a square with a circle in it): the simple face
        // maker's overlapping faces, one filling each wire (mitcad#548).
        (
            "Sketcher::SketchObject",
            "Nested",
            format!(
                "{}{}",
                geometry(&[
                    segment([40.0, 0.0], [50.0, 0.0], false),
                    segment([50.0, 0.0], [50.0, 10.0], false),
                    segment([50.0, 10.0], [40.0, 10.0], false),
                    segment([40.0, 10.0], [40.0, 0.0], false),
                    round([45.0, 5.0], 2.0),
                ]),
                constraints(&[
                    constrain(1, 0.0, &[(0, 2), (1, 1)], "", true),
                    constrain(1, 0.0, &[(1, 2), (2, 1)], "", true),
                    constrain(1, 0.0, &[(2, 2), (3, 1)], "", true),
                    constrain(1, 0.0, &[(3, 2), (0, 1)], "", true),
                ])
            ),
        ),
        (
            "Part::Face",
            "Simple",
            format!(
                "{}{}{}",
                links("Sources", "App::PropertyLinkList", &["Nested"]),
                prop(
                    "FaceMakerClass",
                    "App::PropertyString",
                    "<String value=\"Part::FaceMakerSimple\"/>",
                ),
                shape("Simple.brp")
            ),
        ),
        // A plane primitive Mitcad has no translation of, extruded: the
        // plane falls back (missing), its face extruded.
        ("Part::Plane", "Sheet", shape("Sheet.brp")),
        (
            "Part::Extrusion",
            "Sheeted",
            part_extrusion("Sheet", true, 2, [0.0, 0.0, 1.0], &float("LengthFwd", 2.0)),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("OpenExtrusion.brp", b"2 wall,sheet"),
            ("Face.brp", b"1 face,sheet"),
            ("FaceExtrusion.brp", b"6 slab"),
            ("Bolt.brp", b"1 bolt,sheet"),
            ("BoltExtrusion.brp", b"3 bore"),
            ("Face2.brp", b"1 face2,sheet"),
            ("Face2Extrusion.brp", b"6 tapered"),
            ("Simple.brp", b"1 simple,sheet"),
            ("Sheet.brp", b"1 sheet,sheet"),
            ("SheetExtrusion.brp", b"6 sheeted"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    // Into faces: a surface extrusion of the sketch's two lines (not its
    // construction line), the direction's length.
    let wall = feature(&report, "Wall");
    assert_eq!(wall.outcome, ObjectOutcome::Parametric, "{wall:?}");
    let def = def_of(&doc, &wall.features[0]);
    assert_eq!(def["type"], "extrude", "{def}");
    assert_eq!(def["surface"], true, "{def}");
    assert_eq!(
        def["curves"]["curves"].as_array().unwrap().len(),
        2,
        "{def}"
    );
    assert_eq!(def["extent"]["type"], "distance", "{def}");
    assert_eq!(number(&doc, &def["extent"]["distance"]), 15.0);
    // The face: a patch of the square's region.
    let face = feature(&report, "Face");
    assert_eq!(face.outcome, ObjectOutcome::Parametric, "{face:?}");
    assert_eq!(face.mitcad, "patch");
    let patch = face.features[0].clone();
    // Its extrusion: the patch's face, both lengths, then the patch's body
    // removed.
    let slab = feature(&report, "Slab");
    assert_eq!(slab.outcome, ObjectOutcome::Parametric, "{slab:?}");
    let defs = defs_of(&doc, &report, "Slab");
    assert_eq!(defs.len(), 2, "{defs:?}");
    assert_eq!(defs[0]["type"], "extrude");
    assert_eq!(defs[0]["profile_face"]["body"], format!("{patch}.b0"));
    assert!(
        defs[0]["profile_face"]["face"]
            .as_str()
            .unwrap()
            .starts_with(&format!("{patch}:patch(")),
        "{defs:?}"
    );
    let extent = &defs[0]["extent"];
    assert_eq!(extent["type"], "two_sides", "{extent}");
    assert_eq!(number(&doc, &extent["side1"]["distance"]), 8.0);
    assert_eq!(number(&doc, &extent["side2"]["distance"]), 3.0);
    assert_eq!(defs[1]["type"], "base");
    assert_eq!(defs[1]["replaces"], json!([format!("{patch}.b0")]));
    // The scripted object falls back without a counterpart; its face is
    // extruded symmetric.
    let bolt = feature(&report, "Bolt");
    assert_eq!(bolt.outcome, ObjectOutcome::Fallback, "{bolt:?}");
    assert_eq!(bolt.fallback_group, Some(FallbackGroup::NoCounterpart));
    assert!(
        bolt.notes.iter().any(|n| n.contains("scripted object")),
        "{bolt:?}"
    );
    let base = bolt.features[0].clone();
    let bore = feature(&report, "Bore");
    assert_eq!(bore.outcome, ObjectOutcome::Parametric, "{bore:?}");
    let defs = defs_of(&doc, &report, "Bore");
    assert_eq!(defs[0]["profile_face"]["body"], format!("{base}.b0"));
    assert_eq!(
        defs[0]["profile_face"]["face"],
        format!("{base}:import(0)"),
        "{defs:?}"
    );
    let extent = &defs[0]["extent"];
    assert_eq!(extent["type"], "symmetric", "{extent}");
    assert_eq!(extent["full_length"], true, "{extent}");
    assert_eq!(number(&doc, &extent["distance"]), 5.0);
    // A tapered face (mitcad#541): its taper on the extrusion; a face of
    // the simple face maker of nested wires: a face filling each wire
    // (mitcad#548); a plane primitive: missing in Mitcad.
    let tapered = feature(&report, "Tapered");
    assert_eq!(tapered.outcome, ObjectOutcome::Parametric, "{tapered:?}");
    let defs = defs_of(&doc, &report, "Tapered");
    assert_eq!(defs[0]["extent"]["type"], "distance", "{defs:?}");
    assert!((number(&doc, &defs[0]["extent"]["taper"]).abs() - 5f64.to_radians()).abs() < 1e-12);
    let simple = feature(&report, "Simple");
    assert_eq!(simple.outcome, ObjectOutcome::Parametric, "{simple:?}");
    let sheet = feature(&report, "Sheet");
    assert_eq!(sheet.outcome, ObjectOutcome::Fallback, "{sheet:?}");
    assert_eq!(sheet.fallback_group, Some(FallbackGroup::Missing));
    let sheeted = feature(&report, "Sheeted");
    assert_eq!(sheeted.outcome, ObjectOutcome::Parametric, "{sheeted:?}");
    assert_eq!(report.fallback_groups(), [1, 1, 0]);
    let text = report.text();
    assert!(
        text.contains(
            "fallbacks by group: 1 missing in Mitcad, 1 without a parametric counterpart, 0 other"
        ),
        "{text}"
    );
    assert!(
        text.contains("Part::Part2DObjectPython (no parametric counterpart)"),
        "{text}"
    );
    // The faces' bodies are gone with their extrusions.
    let names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    for gone in ["Face", "Bolt"] {
        assert!(!names.contains(&gone.to_owned()), "{names:?}");
    }
}

/// mitcad#541: the faces of a base of two bodies along a direction off
/// their normal, and an object of edges only (no faces) that only an
/// extrusion uses.
#[test]
fn part_extrusions_of_several_bodies_along_a_direction_and_of_edges() {
    let xml = document(&[
        (
            "Sketcher::SketchObject",
            "Left",
            rectangle_sketch(0.0, 0.0, 10.0, 10.0),
        ),
        (
            "Sketcher::SketchObject",
            "Right",
            rectangle_sketch(20.0, 0.0, 10.0, 10.0),
        ),
        (
            "Part::Face",
            "Faces",
            format!(
                "{}{}",
                links("Sources", "App::PropertyLinkList", &["Left", "Right"]),
                shape("Faces.brp")
            ),
        ),
        (
            "Part::Extrusion",
            "Pair",
            part_extrusion("Faces", true, 0, [1.0, 0.0, 1.0], &float("LengthFwd", 4.0)),
        ),
        ("Part::Circle", "Ring", shape("Ring.brp")),
        (
            "Part::Extrusion",
            "Tube",
            part_extrusion("Ring", true, 2, [0.0, 0.0, 1.0], &float("LengthFwd", 4.0)),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Faces.brp", b"2 faces,sheet"),
            ("FacesExtrusion.brp", b"12 pair"),
            ("Ring.brp", b"0 ring"),
            ("RingExtrusion.brp", b"3 tube"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    // Two patches; each face extruded along the direction, both bodies
    // removed.
    let faces = feature(&report, "Faces");
    assert_eq!(faces.outcome, ObjectOutcome::Parametric, "{faces:?}");
    assert_eq!(faces.features.len(), 2, "{faces:?}");
    let pair = feature(&report, "Pair");
    assert_eq!(pair.outcome, ObjectOutcome::Parametric, "{pair:?}");
    let defs = defs_of(&doc, &report, "Pair");
    assert_eq!(defs.len(), 3, "{defs:?}");
    for def in &defs[..2] {
        assert_eq!(def["type"], "extrude", "{def}");
        assert_eq!(def["direction"], json!([1.0, 0.0, 1.0]), "{def}");
        assert_eq!(number(&doc, &def["extent"]["distance"]), 4.0);
    }
    assert_ne!(
        defs[0]["profile_face"]["body"],
        defs[1]["profile_face"]["body"]
    );
    assert_eq!(defs[2]["type"], "base");
    assert_eq!(
        defs[2]["replaces"].as_array().map(Vec::len),
        Some(2),
        "{defs:?}"
    );
    // FreeCAD's one shape of two solids: both bodies stand for the object,
    // in its item and its place (the second numbered).
    assert_eq!(
        item(&report, "Pair").bodies.len(),
        2,
        "{:?}",
        item(&report, "Pair")
    );
    let place = report
        .placed
        .iter()
        .find(|p| p.object == "Pair")
        .expect("Pair's place");
    assert_eq!(place.bodies, 2);
    let names: Vec<String> = doc.bodies().iter().map(|b| b.name.clone()).collect();
    assert!(
        names.contains(&"Pair".to_owned()) && names.contains(&"Pair (2)".to_owned()),
        "{names:?}"
    );
    // The circle has no faces: part of the extrusion's sketch, no body
    // (the mock kernel gives its shape no edges, so the extrusion falls
    // back to its stored shape).
    let ring = feature(&report, "Ring");
    assert_eq!(ring.outcome, ObjectOutcome::Included, "{ring:?}");
    let tube = feature(&report, "Tube");
    assert_eq!(tube.outcome, ObjectOutcome::Fallback, "{tube:?}");
    assert!(
        tube.notes
            .iter()
            .any(|n| n.contains("neither faces nor edges")),
        "{tube:?}"
    );
}

/// mitcad#547: FreeCAD's simple face maker makes the faces Mitcad's
/// patches do where no wire lies in another; of nested wires (mitcad#548)
/// its overlapping faces, one filling each wire.
#[test]
fn simple_face_maker_faces_of_wires_apart() {
    let simple = prop(
        "FaceMakerClass",
        "App::PropertyString",
        "<String value=\"Part::FaceMakerSimple\"/>",
    );
    // Two squares apart; a square with a circle inside it.
    let square = |x: f64| {
        let p = [[x, 0.0], [x + 8.0, 0.0], [x + 8.0, 8.0], [x, 8.0]];
        (0..4)
            .map(|i| segment(p[i], p[(i + 1) % 4], false))
            .collect::<Vec<_>>()
    };
    let corners = |first: i32| -> Vec<String> {
        (0..4)
            .map(|i| {
                constrain(
                    1,
                    0.0,
                    &[(first + i, 2), (first + (i + 1) % 4, 1)],
                    "",
                    true,
                )
            })
            .collect()
    };
    let apart = format!(
        "{}{}",
        geometry(&[square(0.0), square(20.0)].concat()),
        constraints(&[corners(0), corners(4)].concat())
    );
    let mut nested_geometry = square(0.0);
    nested_geometry.push(round([4.0, 4.0], 2.0));
    let nested = format!("{}{}", geometry(&nested_geometry), constraints(&corners(0)));
    let xml = document(&[
        ("Sketcher::SketchObject", "Apart", apart),
        ("Sketcher::SketchObject", "Nested", nested),
        (
            "Part::Face",
            "TwoFaces",
            format!(
                "{}{}{}",
                links("Sources", "App::PropertyLinkList", &["Apart"]),
                simple,
                shape("TwoFaces.brp")
            ),
        ),
        (
            "Part::Face",
            "Overlapping",
            format!(
                "{}{}{}",
                links("Sources", "App::PropertyLinkList", &["Nested"]),
                prop(
                    "FaceMakerClass",
                    "App::PropertyString",
                    "<String value=\"Part::FaceMakerSimple\"/>",
                ),
                shape("Overlapping.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("TwoFaces.brp", b"2 two,sheet"),
            ("Overlapping.brp", b"2 overlapping,sheet"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let two = feature(&report, "TwoFaces");
    assert_eq!(two.outcome, ObjectOutcome::Parametric, "{two:?}");
    let defs = defs_of(&doc, &report, "TwoFaces");
    assert_eq!(defs[0]["type"], "patch", "{defs:?}");
    assert_eq!(
        defs[0]["profiles"].as_array().map(Vec::len),
        Some(2),
        "{defs:?}"
    );
    // Nested (mitcad#548): the circle's region is its wire's face; the
    // square's region (a ring) is patched, its outer opening patched (the
    // square's face, over the circle's) and the ring's patch removed.
    let overlapping = feature(&report, "Overlapping");
    assert_eq!(
        overlapping.outcome,
        ObjectOutcome::Parametric,
        "{overlapping:?}"
    );
    let defs = defs_of(&doc, &report, "Overlapping");
    let types: Vec<&str> = defs.iter().filter_map(|d| d["type"].as_str()).collect();
    assert_eq!(
        types.iter().filter(|t| **t == "patch").count(),
        3,
        "{defs:?}"
    );
    assert_eq!(
        types.iter().filter(|t| **t == "base").count(),
        1,
        "{defs:?}"
    );
    assert!(
        defs.iter().any(|d| d.get("boundary").is_some()),
        "the outer wire's opening patched: {defs:?}"
    );
    let ring = defs
        .iter()
        .position(|d| d.get("boundary").is_some())
        .and_then(|i| i.checked_sub(1))
        .map(|i| report_feature_uid(&report, "Overlapping", i))
        .expect("the ring's patch");
    let removed = defs.iter().find(|d| d["type"] == "base").expect("removed");
    assert_eq!(
        removed["replaces"],
        json!([format!("{ring}.b0")]),
        "{defs:?}"
    );
}

/// The uid of the `i`-th Mitcad feature made of an object.
fn report_feature_uid(report: &FcstdReport, name: &str, i: usize) -> String {
    feature(report, name).features[i].clone()
}

#[test]
fn a_definition_that_does_not_build_falls_back_as_other() {
    let xml = document(&[
        (
            "Sketcher::SketchObject",
            "Square",
            rectangle_sketch(0.0, 0.0, 10.0, 10.0),
        ),
        (
            "Part::Extrusion",
            "Post",
            part_extrusion("Square", true, 2, [0.0, 0.0, 1.0], &float("LengthFwd", 5.0)),
        ),
    ]);
    let file = fcstd(&xml, None, &[("SquareExtrusion.brp", b"6 post")]);
    let kernel = MockKernel::default();
    kernel.fail.borrow_mut().insert("extrude");
    let (_, report) = import_with(file, Vec::new(), kernel, false);
    let post = feature(&report, "Post");
    assert_eq!(post.outcome, ObjectOutcome::Fallback, "{post:?}");
    assert_eq!(post.fallback_group, Some(FallbackGroup::Other));
    assert_eq!(report.fallback_groups(), [0, 0, 1]);
}

/// A MultiTransform of a Body: its transformations after it in the list.
fn multi(
    name: &str,
    originals: &[&str],
    steps: &[&str],
    base: &str,
    whole: bool,
) -> (&'static str, String, String) {
    (
        "PartDesign::MultiTransform",
        name.to_owned(),
        format!(
            "{}{}{}{}",
            links("Originals", "App::PropertyLinkList", originals),
            links("Transformations", "App::PropertyLinkList", steps),
            link("BaseFeature", base),
            if whole {
                enumeration("TransformMode", 1)
            } else {
                String::new()
            }
        ),
    )
}

fn linear(name: &str, axis: &str, length: f64, occurrences: i64) -> (&'static str, String, String) {
    (
        "PartDesign::LinearPattern",
        name.to_owned(),
        format!(
            "{}{}{}",
            link_sub("Direction", "PlateSketch", &[axis]),
            float("Length", length),
            integer("Occurrences", occurrences)
        ),
    )
}

fn polar(name: &str, occurrences: i64) -> (&'static str, String, String) {
    (
        "PartDesign::PolarPattern",
        name.to_owned(),
        format!(
            "{}{}{}",
            link_sub("Axis", "PlateSketch", &["N_Axis"]),
            float("Angle", 360.0),
            integer("Occurrences", occurrences)
        ),
    )
}

fn scaled(name: &str, factor: f64, occurrences: i64) -> (&'static str, String, String) {
    (
        "PartDesign::Scaled",
        name.to_owned(),
        format!(
            "{}{}",
            prop(
                "Factor",
                "App::PropertyFloat",
                &format!("<Float value=\"{factor}\"/>")
            ),
            integer("Occurrences", occurrences)
        ),
    )
}

#[test]
fn multi_transforms_of_the_whole_body_and_scaled_ones_become_features() {
    // mitcad#539. The features have no stored shapes: with a mock kernel
    // that measures (scaled copies need centres of mass) each is taken
    // unchecked.
    let mut objects: Vec<(&str, String, String)> = vec![
        (
            "PartDesign::Body",
            "Plate".to_owned(),
            format!(
                "{}{}{}",
                shape("Plate.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &[
                        "PlateSketch",
                        "PlatePad",
                        "BossSketch",
                        "Boss",
                        "Whole",
                        "WholeX",
                        "WholeTurn",
                        "First",
                        "FirstScaled",
                        "FirstX",
                        "Runs",
                        "RunsX",
                        "RunsScaled",
                        "After",
                        "AfterTurn",
                        "AfterY",
                        "AfterScaled",
                        "Many",
                        "ManyTurn",
                        "ManyY",
                        "ManyScaled",
                        "WholeGrown",
                        "WholeGrownX",
                        "WholeGrownScaled",
                        "WholeTwo",
                        "WholeTwoX",
                        "WholeTwoY",
                        "WholeTwoScaled",
                        "WholeRuns",
                        "WholeRunsX",
                        "WholeRunsScaled",
                    ]
                ),
                link("Tip", "WholeRuns")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "PlateSketch".to_owned(),
            rectangle_sketch(0.0, 0.0, 60.0, 30.0),
        ),
        (
            "PartDesign::Pad",
            "PlatePad".to_owned(),
            format!(
                "{}{}",
                link_sub("Profile", "PlateSketch", &[]),
                float("Length", 5.0)
            ),
        ),
        (
            "Sketcher::SketchObject",
            "BossSketch".to_owned(),
            geometry(&[round([8.0, 8.0], 2.0)]),
        ),
        (
            "PartDesign::Pad",
            "Boss".to_owned(),
            format!(
                "{}{}{}",
                link_sub("Profile", "BossSketch", &[]),
                float("Length", 4.0),
                link("BaseFeature", "PlatePad")
            ),
        ),
    ];
    // The whole body along x, then turned half round: patterns of the body
    // joining their copies.
    objects.push(multi("Whole", &[], &["WholeX", "WholeTurn"], "Boss", true));
    objects.push(linear("WholeX", "H_Axis", 60.0, 2));
    objects.push(polar("WholeTurn", 2));
    // Scaled first: a scale pattern, then a pattern of it.
    objects.push(multi(
        "First",
        &["Boss"],
        &["FirstScaled", "FirstX"],
        "Whole",
        false,
    ));
    objects.push(scaled("FirstScaled", 1.5, 3));
    objects.push(linear("FirstX", "H_Axis", 20.0, 2));
    // Two runs of four copies.
    objects.push(multi(
        "Runs",
        &["Boss"],
        &["RunsX", "RunsScaled"],
        "First",
        false,
    ));
    objects.push(linear("RunsX", "H_Axis", 30.0, 4));
    objects.push(scaled("RunsScaled", 2.0, 2));
    // After two patterns, scaling the second one's copies.
    objects.push(multi(
        "After",
        &["Boss"],
        &["AfterTurn", "AfterY", "AfterScaled"],
        "Runs",
        false,
    ));
    objects.push(polar("AfterTurn", 4));
    objects.push(linear("AfterY", "V_Axis", 10.0, 2));
    objects.push(scaled("AfterScaled", 2.0, 2));
    // More runs than the last pattern has copies: not translated.
    objects.push(multi(
        "Many",
        &["Boss"],
        &["ManyTurn", "ManyY", "ManyScaled"],
        "After",
        false,
    ));
    objects.push(polar("ManyTurn", 2));
    objects.push(linear("ManyY", "V_Axis", 10.0, 2));
    objects.push(scaled("ManyScaled", 2.0, 4));
    // The whole body along x, the copy 1.25 times the size about the Body's
    // origin carried with it (mitcad#542).
    objects.push(multi(
        "WholeGrown",
        &[],
        &["WholeGrownX", "WholeGrownScaled"],
        "Many",
        true,
    ));
    objects.push(linear("WholeGrownX", "H_Axis", 60.0, 2));
    objects.push(scaled("WholeGrownScaled", 1.25, 2));
    // The whole body along x, then along y, then each of the four copies
    // scaled on its own (mitcad#547): a pattern of a pattern of the body.
    objects.push(multi(
        "WholeTwo",
        &[],
        &["WholeTwoX", "WholeTwoY", "WholeTwoScaled"],
        "WholeGrown",
        true,
    ));
    objects.push(linear("WholeTwoX", "H_Axis", 60.0, 2));
    objects.push(linear("WholeTwoY", "V_Axis", 30.0, 2));
    objects.push(scaled("WholeTwoScaled", 1.2, 4));
    // The whole body 4 along x, scaled in two runs: FreeCAD 1.0 leaves the
    // first run's copy out, which the pattern of the body skips.
    objects.push(multi(
        "WholeRuns",
        &[],
        &["WholeRunsX", "WholeRunsScaled"],
        "WholeTwo",
        true,
    ));
    objects.push(linear("WholeRunsX", "H_Axis", 90.0, 4));
    objects.push(scaled("WholeRunsScaled", 1.2, 2));
    // A transformation of the last one stored a shape: a Body is replayed
    // when a feature of it did (the transformations themselves are not
    // replayed, so nothing is compared with it).
    if let Some(many) = objects.iter_mut().find(|(_, n, _)| n == "ManyScaled") {
        many.2.push_str(&shape("Many.brp"));
    }
    let borrowed: Vec<(&str, &str, String)> = objects
        .iter()
        .map(|(t, n, p)| (*t, n.as_str(), p.clone()))
        .collect();
    let xml = document(&borrowed);
    let file = fcstd(
        &xml,
        None,
        &[("Plate.brp", b"6 plate"), ("Many.brp", b"6 many")],
    );
    let kernel = MockKernel::default();
    kernel.measures.set(true);
    let (doc, report) = import_with(file, Vec::new(), kernel, false);
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
    let boss = &feature(&report, "Boss").features[0];
    // The whole body: each transformation a pattern of the body, joined.
    let whole = feature(&report, "Whole");
    assert_eq!(whole.outcome, ObjectOutcome::Parametric, "{whole:?}");
    let defs = defs_of(&doc, &report, "Whole");
    let types: Vec<&str> = defs.iter().map(|d| d["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["rectangular_pattern", "circular_pattern"]);
    // The first a pattern of the body, the second a pattern of it (the
    // body at every product of the elements, mitcad#547), both joining.
    assert_eq!(defs[0]["objects"]["type"], "bodies", "{}", defs[0]);
    assert_eq!(
        defs[1]["objects"]["features"],
        json!([whole.features[0]]),
        "{}",
        defs[1]
    );
    for def in &defs {
        assert_eq!(def["combine"], true, "{def}");
    }
    assert_eq!(item(&report, "WholeX").outcome, ObjectOutcome::Included);
    // Scaled first: a scale pattern of the boss, then a pattern of it.
    let first = feature(&report, "First");
    assert_eq!(first.outcome, ObjectOutcome::Parametric, "{first:?}");
    let defs = defs_of(&doc, &report, "First");
    assert_eq!(defs[0]["type"], "scale_pattern");
    assert_eq!(defs[0]["objects"]["features"], json!([boss]));
    assert_eq!(value(&defs[0]["quantity"]), 3.0);
    assert_eq!(value(&defs[0]["scale"]), 1.5);
    assert_eq!(defs[1]["type"], "rectangular_pattern");
    assert_eq!(defs[1]["objects"]["features"], json!([first.features[0]]));
    // Two runs of the pattern's four copies.
    let runs = feature(&report, "Runs");
    assert_eq!(runs.outcome, ObjectOutcome::Parametric, "{runs:?}");
    let def = &defs_of(&doc, &report, "Runs")[0];
    assert_eq!(def["type"], "rectangular_pattern");
    assert_eq!(value(&def["scale"]), 2.0);
    assert_eq!(value(&def["scale_groups"]), 2.0);
    // A 1.0 document: first the reading without the first run's copy,
    // which FreeCAD 1.0 and 1.1 leave out (mitcad#542).
    assert_eq!(def["skipped_copies"], json!([1]), "{def}");
    // After a polar and a linear pattern: the linear one scales its
    // copies (of the polar one's too), one run per element.
    let after = feature(&report, "After");
    assert_eq!(after.outcome, ObjectOutcome::Parametric, "{after:?}");
    let defs = defs_of(&doc, &report, "After");
    assert_eq!(defs[0]["type"], "circular_pattern");
    assert!(defs[0].get("scale").is_none(), "{}", defs[0]);
    assert_eq!(defs[1]["type"], "rectangular_pattern");
    assert_eq!(value(&defs[1]["scale"]), 2.0);
    assert!(defs[1].get("scale_groups").is_none(), "{}", defs[1]);
    // The polar pattern's copies are the first run, factor 1: FreeCAD 1.0
    // leaves them out, the linear pattern still copies them.
    assert_eq!(defs[0]["skipped_copies"], json!([1, 2, 3]), "{}", defs[0]);
    assert!(defs[1].get("skipped_copies").is_none(), "{}", defs[1]);
    // Four runs of the second pattern's two copies (mitcad#542): both
    // patterns scale their copies, the runs counted over all four.
    let many = feature(&report, "Many");
    assert_eq!(many.outcome, ObjectOutcome::Parametric, "{many:?}");
    let defs = defs_of(&doc, &report, "Many");
    for def in &defs {
        assert_eq!(value(&def["scale"]), 2.0, "{def}");
        assert_eq!(value(&def["scale_groups"]), 4.0, "{def}");
        assert_eq!(value(&def["scale_copies"]), 4.0, "{def}");
        assert!(def.get("skipped_copies").is_none(), "{def}");
    }
    // The whole body scaled: a pattern of the body with its scale about the
    // Body's origin.
    let grown = feature(&report, "WholeGrown");
    assert_eq!(grown.outcome, ObjectOutcome::Parametric, "{grown:?}");
    let def = &defs_of(&doc, &report, "WholeGrown")[0];
    assert_eq!(def["type"], "rectangular_pattern");
    assert_eq!(def["combine"], true, "{def}");
    assert_eq!(value(&def["scale"]), 1.25);
    // A fixed point, as the model writes it back: its point alone.
    let point = def["scale_center"]["point"].as_array().unwrap();
    assert_eq!(point.len(), 3, "{def}");
    assert!(point.iter().all(|c| c.as_f64() == Some(0.0)), "{def}");
    // After two transformations: both patterns scale their copies of the
    // body, the runs counted over all four, about the Body's origin.
    let two = feature(&report, "WholeTwo");
    assert_eq!(two.outcome, ObjectOutcome::Parametric, "{two:?}");
    let defs = defs_of(&doc, &report, "WholeTwo");
    assert_eq!(defs[0]["objects"]["type"], "bodies", "{}", defs[0]);
    assert_eq!(defs[1]["objects"]["features"], json!([two.features[0]]));
    for def in &defs {
        assert_eq!(def["combine"], true, "{def}");
        assert_eq!(value(&def["scale_groups"]), 4.0, "{def}");
        assert_eq!(value(&def["scale_copies"]), 4.0, "{def}");
        assert!(def["scale_center"]["point"].is_array(), "{def}");
    }
    // Two runs over four copies of the body: a 1.0 document's first reading
    // skips the first run's copy.
    let runs = feature(&report, "WholeRuns");
    assert_eq!(runs.outcome, ObjectOutcome::Parametric, "{runs:?}");
    let def = &defs_of(&doc, &report, "WholeRuns")[0];
    assert_eq!(def["objects"]["type"], "bodies", "{def}");
    assert_eq!(def["skipped_copies"], json!([1]), "{def}");
}

#[test]
fn pipe_modes_slanted_tapers_and_turns_to_the_last_face_become_features() {
    // (mitcad#539) Pipes in the Binormal, Auxiliary and Frenet modes; a pad
    // tapered along a custom direction; a groove through all and a
    // revolution up to the last face. FreeCAD 1.0's enumerations: Mode
    // Standard, Fixed, Frenet, Auxiliary, Binormal; the groove's Type
    // Angle, ThroughAll, …; the revolution's Angle, UpToLast, ….
    let pipe = |name: &str, base: Option<&str>, mode: i64, extra: String| {
        (
            "PartDesign::AdditivePipe",
            name.to_owned(),
            format!(
                "{}{}{}{}{}{}",
                link_sub("Profile", "Ring", &[]),
                link_sub("Spine", "Spine", &[]),
                enumeration("Mode", mode),
                base.map(|b| link("BaseFeature", b)).unwrap_or_default(),
                extra,
                shape(&format!("{name}.brp"))
            ),
        )
    };
    let upright = placement([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 90.0);
    let banded = pipe("Banded", None, 4, vector("Binormal", [0.0, 1.0, 1.0]));
    let guided = pipe(
        "Guided",
        Some("Banded"),
        3,
        format!(
            "{}{}",
            link_sub("AuxillerySpine", "Rail", &[]),
            flag("AuxilleryCurvelinear", false)
        ),
    );
    let frenet = pipe("Following", Some("Guided"), 2, String::new());
    let objects = [
        (
            "PartDesign::Body",
            "Piped".to_owned(),
            format!(
                "{}{}{}",
                shape("Piped.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Ring", "Spine", "Rail", "Banded", "Guided", "Following"]
                ),
                link("Tip", "Following")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Ring".to_owned(),
            geometry(&[round([0.0, 0.0], 2.0)]),
        ),
        (
            "Sketcher::SketchObject",
            "Spine".to_owned(),
            format!(
                "{}{upright}",
                geometry(&[segment([0.0, 0.0], [0.0, 30.0], false)])
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Rail".to_owned(),
            format!(
                "{}{upright}",
                geometry(&[segment([6.0, 0.0], [-6.0, 30.0], false)])
            ),
        ),
        (banded.0, banded.1, banded.2),
        (guided.0, guided.1, guided.2),
        (frenet.0, frenet.1, frenet.2),
        (
            "PartDesign::Body",
            "Slanted".to_owned(),
            format!(
                "{}{}{}",
                shape("Slanted.brp"),
                links("Group", "App::PropertyLinkList", &["SlantSketch", "Slant"]),
                link("Tip", "Slant")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "SlantSketch".to_owned(),
            rectangle_sketch(40.0, 0.0, 20.0, 10.0),
        ),
        (
            "PartDesign::Pad",
            "Slant".to_owned(),
            format!(
                "{}{}{}{}{}{}",
                link_sub("Profile", "SlantSketch", &[]),
                float("Length", 10.0),
                flag("UseCustomVector", true),
                vector("Direction", [1.0, 0.0, 2.0]),
                prop("TaperAngle", "App::PropertyAngle", "<Float value=\"5\"/>"),
                shape("Slant.brp")
            ),
        ),
        (
            "PartDesign::Body",
            "Turned".to_owned(),
            format!(
                "{}{}{}",
                shape("Turned.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &[
                        "Block",
                        "BlockPad",
                        "GrooveSketch",
                        "Groove",
                        "CollarSketch",
                        "Collar"
                    ]
                ),
                link("Tip", "Collar")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Block".to_owned(),
            rectangle_sketch(0.0, 0.0, 20.0, 10.0),
        ),
        (
            "PartDesign::Pad",
            "BlockPad".to_owned(),
            format!(
                "{}{}{}",
                link_sub("Profile", "Block", &[]),
                float("Length", 10.0),
                shape("BlockPad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "GrooveSketch".to_owned(),
            rectangle_sketch(5.0, 2.0, 2.0, 2.0),
        ),
        (
            "PartDesign::Groove",
            "Groove".to_owned(),
            format!(
                "{}{}{}{}{}{}",
                link_sub("Profile", "GrooveSketch", &[]),
                link_sub("ReferenceAxis", "GrooveSketch", &["V_Axis"]),
                link("BaseFeature", "BlockPad"),
                prop("Angle", "App::PropertyAngle", "<Float value=\"90\"/>"),
                enumeration("Type", 1),
                shape("Groove.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "CollarSketch".to_owned(),
            rectangle_sketch(12.0, 0.0, 2.0, 2.0),
        ),
        (
            "PartDesign::Revolution",
            "Collar".to_owned(),
            format!(
                "{}{}{}{}{}",
                link_sub("Profile", "CollarSketch", &[]),
                link_sub("ReferenceAxis", "CollarSketch", &["V_Axis"]),
                link("BaseFeature", "Groove"),
                enumeration("Type", 1),
                shape("Collar.brp")
            ),
        ),
    ];
    let objects: Vec<(&str, &str, String)> = objects
        .iter()
        .map(|(t, n, p)| (*t, n.as_str(), p.clone()))
        .collect();
    let xml = document(&objects);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Piped.brp", b"3 piped"),
            ("Banded.brp", b"3 banded"),
            ("Guided.brp", b"3 guided"),
            ("Following.brp", b"3 following"),
            ("Slanted.brp", b"6 slanted"),
            ("Slant.brp", b"6 slant"),
            ("Turned.brp", b"6 turned"),
            ("BlockPad.brp", b"6 block"),
            ("Groove.brp", b"10 groove"),
            ("Collar.brp", b"14 collar"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    for name in ["Banded", "Guided", "Following", "Slant", "Groove", "Collar"] {
        assert_eq!(
            feature(&report, name).outcome,
            ObjectOutcome::Parametric,
            "{name}: {:?}",
            feature(&report, name)
        );
    }
    // Binormal: a constant binormal frame (the pipe has no placement of
    // its own here, so the binormal is FreeCAD's).
    let def = &defs_of(&doc, &report, "Banded")[0];
    assert_eq!(def["type"], "sweep", "{def}");
    assert_eq!(
        def["frame"],
        json!({"type": "constant_binormal", "binormal": [0.0, 1.0, 1.0]}),
        "{def}"
    );
    assert!(def.get("binormal").is_none(), "{def}");
    // Auxiliary: the rail's curve, its point where it crosses the plane
    // square to the spine.
    let def = &defs_of(&doc, &report, "Guided")[0];
    assert_eq!(def["frame"]["type"], "auxiliary", "{def}");
    assert!(def["frame"]["curve"]["sketch"].is_string(), "{def}");
    assert_ne!(
        def["frame"]["curve"]["sketch"], def["path"]["sketch"],
        "{def}"
    );
    assert!(def["frame"].get("curvilinear").is_none(), "{def}");
    assert_eq!(def["operation"], "join");
    // Frenet: the Frenet frame first.
    let def = &defs_of(&doc, &report, "Following")[0];
    assert_eq!(def["frame"], json!({"type": "frenet"}), "{def}");
    // The slanted pad: an extrusion along the unit direction, tapered.
    let def = &defs_of(&doc, &report, "Slant")[0];
    assert_eq!(def["type"], "extrude", "{def}");
    let direction = def["direction"].as_array().unwrap();
    assert!(
        (direction[0].as_f64().unwrap() - 1.0 / 5f64.sqrt()).abs() < 1e-12,
        "{def}"
    );
    assert!(
        (direction[2].as_f64().unwrap() - 2.0 / 5f64.sqrt()).abs() < 1e-12,
        "{def}"
    );
    assert_eq!(def["extent"]["type"], "distance", "{def}");
    assert!(def["extent"].get("taper").is_some(), "{def}");
    // The groove through all: a full turn, cut.
    let def = &defs_of(&doc, &report, "Groove")[0];
    assert_eq!(def["extent"], json!({"type": "full"}), "{def}");
    assert_eq!(def["operation"], "cut");
    // Up to the last face: through the Body's body.
    let def = &defs_of(&doc, &report, "Collar")[0];
    assert_eq!(def["extent"]["type"], "to_object", "{def}");
    assert_eq!(def["extent"]["object"]["type"], "body", "{def}");
    assert_eq!(def["extent"]["object"]["through"], true, "{def}");
}

// Part fillets of two radii, face profiles and chosen sketch edges
// (mitcad#539).

/// A parameter's value by the name a definition gives it.
fn value_of(doc: &Document<MockKernel>, name: &serde_json::Value) -> Option<f64> {
    let parameters = doc.parameters();
    parameters
        .find(name.as_str()?)
        .and_then(|id| parameters.value(id))
}

#[test]
fn part_fillets_of_two_radii_start_where_freecad_s_edge_starts() {
    use mitcad_model::Curve3;
    let kernel = MockKernel::default();
    // Every edge of the mock's bodies is this line, from (0, 0, 10); its
    // first vertex is the second the mock lists.
    kernel.curves.borrow_mut().push(Curve3::Line {
        start: [0.0, 0.0, 10.0],
        end: [40.0, 0.0, 10.0],
    });
    let start = "V{F1:import(0)|F1:import(1)|F1:import(2)}";
    kernel.vertices.borrow_mut().extend([
        (
            "V{F1:import(0)|F1:import(1)|F1:import(5)}".to_owned(),
            [40.0, 0.0, 10.0],
        ),
        (start.to_owned(), [0.0, 0.0, 10.0]),
    ]);
    // Edge 1 radius 2, edge 2 from 1 to 3, edge 3 radii 2 and 2 + 1e-9
    // (one radius for OCCT).
    let mut edges = 3u32.to_le_bytes().to_vec();
    for (edge, r1, r2) in [(1u32, 2.0f64, 2.0f64), (2, 1.0, 3.0), (3, 2.0, 2.0 + 1e-9)] {
        edges.extend(edge.to_le_bytes());
        edges.extend(r1.to_le_bytes());
        edges.extend(r2.to_le_bytes());
    }
    let xml = document(&[
        (
            "Part::Box",
            "Box",
            format!(
                "{}{}{}{}",
                float("Length", 40.0),
                float("Width", 20.0),
                float("Height", 10.0),
                shape("Box.brp")
            ),
        ),
        (
            "Part::Fillet",
            "Rounded",
            format!(
                "{}{}{}",
                link("Base", "Box"),
                prop(
                    "Edges",
                    "Part::PropertyFilletEdges",
                    "<FilletEdges file=\"Edges.bin\"/>"
                ),
                shape("Rounded.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Box.brp", b"6 box"),
            ("Rounded.brp", b"9 rounded"),
            ("Edges.bin", edges.as_slice()),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), kernel, false);
    let fillet = feature(&report, "Rounded");
    assert_eq!(fillet.outcome, ObjectOutcome::Parametric, "{fillet:?}");
    assert_eq!(fillet.mitcad, "fillet");
    assert!(
        fillet.notes.iter().any(|n| n.contains("variable radius")),
        "{fillet:?}"
    );
    let def = def_of(&doc, &fillet.features[0]);
    let sets = def["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 2, "{def}");
    // Edges 1 and 3 one constant radius.
    assert_eq!(sets[0]["size"]["type"], "constant", "{def}");
    assert_eq!(sets[0]["edges"].as_array().unwrap().len(), 2, "{def}");
    let radius = value_of(&doc, &sets[0]["size"]["radius"]).unwrap();
    assert!((radius - 2.0).abs() < 1e-7, "{radius}");
    // Edge 2 from 1 at its start vertex to 3.
    let size = &sets[1]["size"];
    assert_eq!(size["type"], "variable", "{def}");
    assert_eq!(size["start_vertex"], start, "{def}");
    assert_eq!(value_of(&doc, &size["start"]), Some(1.0), "{def}");
    assert_eq!(value_of(&doc, &size["end"]), Some(3.0), "{def}");
    assert_eq!(sets[1]["edges"].as_array().unwrap().len(), 1, "{def}");
}

#[test]
fn face_profiles_extrude_the_face_and_ignored_sketch_edges_take_the_sketch() {
    // Pad2 pads the top of Pad (its Face2: the mock's end cap); Pad3 names
    // an edge of its sketch without AllowMultiFace, which FreeCAD ignores.
    let xml = document(&[
        (
            "PartDesign::Body",
            "Block",
            format!(
                "{}{}{}",
                shape("Block.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch", "Pad", "Pad2", "Sketch2", "Pad3"]
                ),
                link("Tip", "Pad3")
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
                link_sub("Profile", "Pad", &["Face2"]),
                link("BaseFeature", "Pad"),
                float("Length", 5.0),
                shape("Pad2.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch2",
            rectangle_sketch(5.0, 5.0, 10.0, 10.0),
        ),
        (
            "PartDesign::Pad",
            "Pad3",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Sketch2", &["Edge1"]),
                link("BaseFeature", "Pad2"),
                float("Length", 2.0),
                shape("Pad3.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Block.brp", b"10 block"),
            ("Pad.brp", b"6 pad"),
            ("Pad2.brp", b"10 pad2"),
            ("Pad3.brp", b"14 pad3"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    for name in ["Pad", "Pad2", "Pad3"] {
        let f = feature(&report, name);
        assert_eq!(f.outcome, ObjectOutcome::Parametric, "{f:?}");
        assert_eq!(f.mitcad, "extrude", "{f:?}");
    }
    let pad = def_of(&doc, &feature(&report, "Pad").features[0]);
    let sketch = pad["profiles"][0]["sketch"].as_str().unwrap().to_owned();
    let pad2 = def_of(&doc, &feature(&report, "Pad2").features[0]);
    // The pad's body and its top, swept out of it, joined.
    let face = &pad2["profile_face"];
    assert!(pad2["profiles"].is_null(), "{pad2}");
    assert_eq!(face["body"], pad2["participants"][0], "{pad2}");
    assert!(face["face"].as_str().unwrap().contains(":end("), "{pad2}");
    // Along the outward normal: the stored definition leaves out a false
    // `flip`.
    assert!(pad2["flip"].is_null(), "{pad2}");
    assert_eq!(pad2["operation"], "join", "{pad2}");
    assert_eq!(value_of(&doc, &pad2["extent"]["distance"]), Some(5.0));
    // Pad3: the whole of its sketch.
    let pad3 = def_of(&doc, &feature(&report, "Pad3").features[0]);
    assert_eq!(pad3["profiles"].as_array().unwrap().len(), 1, "{pad3}");
    assert_ne!(pad3["profiles"][0]["sketch"], sketch.as_str(), "{pad3}");
    assert!(pad3["profile_face"].is_null(), "{pad3}");
}

#[test]
fn profiles_of_a_whole_sketch_with_an_empty_sub_element_replay() {
    // (mitcad#708) FreeCAD 1.x links a feature to its whole sketch by one
    // empty sub-element (`count="1"`, `<Sub value=""/>`), with
    // AllowMultiFace on and the sketch's normal as ReferenceAxis; older
    // files by none (`count="0"`, Pocket2). Each takes the whole sketch.
    let sketch_feature = |sketch: &str, subs: &[&str], direction: [f64; 3]| {
        format!(
            "{}{}{}{}",
            link_sub("Profile", sketch, subs),
            flag("AllowMultiFace", true),
            link_sub("ReferenceAxis", sketch, &["N_Axis"]),
            vector("Direction", direction)
        )
    };
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch", "Pad", "Sketch2", "Pocket", "Sketch3", "Pocket2"]
                ),
                link("Tip", "Pocket2")
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
                "{}{}{}{}",
                sketch_feature("Sketch", &[""], [0.0, 0.0, 1.0]),
                float("Length", 8.5),
                enumeration("Type", 0),
                shape("Pad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch2",
            rectangle_sketch(5.0, 5.0, 10.0, 10.0),
        ),
        (
            "PartDesign::Pocket",
            "Pocket",
            format!(
                "{}{}{}{}",
                sketch_feature("Sketch2", &[""], [0.0, 0.0, -1.0]),
                link("BaseFeature", "Pad"),
                enumeration("Type", 1),
                shape("Pocket.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch3",
            rectangle_sketch(20.0, 5.0, 10.0, 10.0),
        ),
        (
            "PartDesign::Pocket",
            "Pocket2",
            format!(
                "{}{}{}{}{}",
                link_sub("Profile", "Sketch3", &[]),
                flag("AllowMultiFace", true),
                link("BaseFeature", "Pocket"),
                float("Length", 2.0),
                shape("Pocket2.brp")
            ),
        ),
    ])
    .replace("1.0R39319", "1.2R1");
    let file = fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"14 body"),
            ("Pad.brp", b"6 pad"),
            ("Pocket.brp", b"10 pocket"),
            ("Pocket2.brp", b"14 pocket2"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    for name in ["Pad", "Pocket", "Pocket2"] {
        let f = feature(&report, name);
        assert_eq!(f.outcome, ObjectOutcome::Parametric, "{f:?}");
        assert_eq!(f.mitcad, "extrude", "{f:?}");
    }
    let def = |name: &str| def_of(&doc, &feature(&report, name).features[0]);
    let (pad, pocket, pocket2) = (def("Pad"), def("Pocket"), def("Pocket2"));
    // Each the one region of its whole sketch.
    let mut sketches = Vec::new();
    for d in [&pad, &pocket, &pocket2] {
        assert_eq!(d["profiles"].as_array().unwrap().len(), 1, "{d}");
        assert!(d["profile_face"].is_null(), "{d}");
        sketches.push(d["profiles"][0]["sketch"].clone());
    }
    assert!(
        sketches[0] != sketches[1] && sketches[1] != sketches[2] && sketches[0] != sketches[2],
        "{sketches:?}"
    );
    assert_eq!(pad["operation"], "new_body", "{pad}");
    assert_eq!(value_of(&doc, &pad["extent"]["distance"]), Some(8.5));
    // The pockets cut (their direction is the importer's, checked by the corpus).
    assert_eq!(pocket["operation"], "cut", "{pocket}");
    assert_eq!(pocket["extent"]["type"], "through_all", "{pocket}");
    assert_eq!(pocket2["operation"], "cut", "{pocket2}");
    assert_eq!(value_of(&doc, &pocket2["extent"]["distance"]), Some(2.0));
    assert!(
        report
            .text()
            .contains("6 parametric, 0 partial, 0 fallback"),
        "{}",
        report.text()
    );
}

#[test]
fn part_sweeps_framed_sections_face_pipes_and_several_faces_become_features() {
    // (mitcad#543) A Part sweep in the Frenet mode; a pipe of the block's
    // top along a sketch line; a pipe through two sections in the Frenet
    // mode (FreeCAD 1.0's Mode: Standard, Fixed, Frenet, …; Transformation:
    // Constant, Multisection, …); a pad of two of the block's faces
    // (AllowMultiFace).
    let upright = placement([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 90.0);
    let xml = document(&[
        (
            "PartDesign::Body",
            "Block",
            format!(
                "{}{}{}",
                shape("Block.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &[
                        "Sketch", "Pad", "Line", "Tube", "Ring", "Ring2", "Sections", "Faces"
                    ]
                ),
                link("Tip", "Faces")
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
            "Sketcher::SketchObject",
            "Line",
            format!(
                "{}{upright}",
                geometry(&[segment([20.0, 10.0], [20.0, 40.0], false)])
            ),
        ),
        (
            "PartDesign::AdditivePipe",
            "Tube",
            format!(
                "{}{}{}{}{}",
                link_sub("Profile", "Pad", &["Face2"]),
                link_sub("Spine", "Line", &[]),
                enumeration("Mode", 0),
                link("BaseFeature", "Pad"),
                shape("Tube.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Ring",
            geometry(&[round([60.0, 0.0], 2.0)]),
        ),
        (
            "Sketcher::SketchObject",
            "Ring2",
            format!(
                "{}{}",
                geometry(&[round([60.0, 0.0], 3.0)]),
                placement([0.0, 0.0, 30.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        (
            "PartDesign::AdditivePipe",
            "Sections",
            format!(
                "{}{}{}{}{}{}{}",
                link_sub("Profile", "Ring", &[]),
                links("Sections", "App::PropertyLinkList", &["Ring2"]),
                enumeration("Transformation", 1),
                link_sub("Spine", "Line", &[]),
                enumeration("Mode", 2),
                link("BaseFeature", "Tube"),
                shape("Sections.brp")
            ),
        ),
        (
            "PartDesign::Pad",
            "Faces",
            format!(
                "{}{}{}{}{}",
                link_sub("Profile", "Pad", &["Face1", "Face2"]),
                flag("AllowMultiFace", true),
                link("BaseFeature", "Sections"),
                float("Length", 2.0),
                shape("Faces.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Hoop",
            geometry(&[round([0.0, 100.0], 2.0)]),
        ),
        (
            "Sketcher::SketchObject",
            "Track",
            format!(
                "{}{upright}",
                geometry(&[segment([0.0, 0.0], [0.0, 30.0], false)])
            ),
        ),
        (
            "Part::Sweep",
            "Swept",
            format!(
                "{}{}{}{}{}",
                links("Sections", "App::PropertyLinkList", &["Hoop"]),
                // The whole sketch: a sketch's Edge<n> is found by its
                // stored shape, which these hand-made sketches have not.
                link_sub("Spine", "Track", &[]),
                flag("Solid", true),
                flag("Frenet", true),
                shape("Swept.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Block.brp", b"10 block"),
            ("Pad.brp", b"6 pad"),
            ("Tube.brp", b"10 tube"),
            ("Sections.brp", b"12 sections"),
            ("Faces.brp", b"14 faces"),
            ("Swept.brp", b"3 swept"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    for name in ["Tube", "Sections", "Faces", "Swept"] {
        let f = feature(&report, name);
        assert_eq!(f.outcome, ObjectOutcome::Parametric, "{name}: {f:?}");
    }
    // The pipe of the block's top: a sweep of that face, joined.
    let tube = def_of(&doc, &feature(&report, "Tube").features[0]);
    assert_eq!(tube["type"], "sweep", "{tube}");
    assert!(tube["profiles"].is_null(), "{tube}");
    assert!(
        tube["profile_face"]["face"]
            .as_str()
            .unwrap()
            .contains(":end("),
        "{tube}"
    );
    assert_eq!(tube["operation"], "join", "{tube}");
    // Two sections in the Frenet mode: a loft along the line, framed.
    let sections = def_of(&doc, &feature(&report, "Sections").features[0]);
    assert_eq!(sections["type"], "loft", "{sections}");
    assert_eq!(sections["frame"], json!({"type": "frenet"}), "{sections}");
    assert_eq!(sections["sections"].as_array().unwrap().len(), 2);
    // Two faces: an extrusion each, the second named apart.
    let faces = defs_of(&doc, &report, "Faces");
    assert_eq!(faces.len(), 2, "{faces:?}");
    assert_ne!(faces[0]["profile_face"], faces[1]["profile_face"]);
    assert!(faces.iter().all(|d| d["type"] == "extrude"), "{faces:?}");
    // The Part sweep: a new body, in the Frenet frame first.
    let swept = def_of(&doc, &feature(&report, "Swept").features[0]);
    assert_eq!(swept["type"], "sweep", "{swept}");
    assert_eq!(swept["frame"], json!({"type": "frenet"}), "{swept}");
    assert_eq!(swept["operation"], "new_body", "{swept}");
    assert!(swept.get("surface").is_none(), "{swept}");
}

#[test]
fn fixed_pipes_part_helix_sweeps_and_many_faces_become_features() {
    // (mitcad#547) A pipe through two sections in the Fixed mode: a loft
    // along the spine, parallel. A Part sweep of a sketch along a Part
    // helix (left-handed, moved): the helix part of the sweep (no body of
    // its own), the sweep's path a fixed helix from its sizes.
    let upright = placement([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 90.0);
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Ring", "Ring2", "Line", "Pipe"]
                ),
                link("Tip", "Pipe")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Ring",
            format!(
                "{}{}",
                geometry(&[round([20.0, 0.0], 2.0)]),
                placement([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Ring2",
            format!(
                "{}{}",
                geometry(&[round([20.0, 0.0], 3.0)]),
                placement([0.0, 0.0, 40.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Line",
            format!(
                "{}{upright}",
                geometry(&[segment([20.0, 10.0], [20.0, 40.0], false)])
            ),
        ),
        (
            "PartDesign::AdditivePipe",
            "Pipe",
            format!(
                "{}{}{}{}{}{}",
                link_sub("Profile", "Ring", &[]),
                links("Sections", "App::PropertyLinkList", &["Ring2"]),
                enumeration("Transformation", 1),
                link_sub("Spine", "Line", &[]),
                enumeration("Mode", 1),
                shape("Pipe.brp")
            ),
        ),
        (
            "Part::Helix",
            "Coil",
            format!(
                "{}{}{}{}{}{}{}",
                float("Pitch", 5.0),
                float("Height", 20.0),
                float("Radius", 10.0),
                float("Angle", 0.0),
                enumeration("LocalCoord", 1),
                placement([5.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0),
                shape("Coil.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Wire",
            format!("{}{upright}", geometry(&[round([15.0, 0.0], 1.0)])),
        ),
        (
            "Part::Sweep",
            "Spring",
            format!(
                "{}{}{}{}{}",
                links("Sections", "App::PropertyLinkList", &["Wire"]),
                link_sub("Spine", "Coil", &[]),
                flag("Solid", true),
                flag("Frenet", true),
                shape("Spring.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"3 body"),
            ("Pipe.brp", b"3 pipe"),
            ("Coil.brp", b"0 coil"),
            ("Spring.brp", b"3 spring"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let pipe = feature(&report, "Pipe");
    assert_eq!(pipe.outcome, ObjectOutcome::Parametric, "{pipe:?}");
    // The sweep along the helix: partial, the helix's sizes are not
    // carried over (a fixed helix path).
    let swept = feature(&report, "Spring");
    assert_eq!(swept.outcome, ObjectOutcome::Partial, "{swept:?}");
    assert!(
        swept
            .notes
            .iter()
            .any(|n| n.contains("Coil's helix is a fixed helix path")),
        "{swept:?}"
    );
    // The Fixed pipe: a loft along the line whose sections only move.
    let pipe = def_of(&doc, &feature(&report, "Pipe").features[0]);
    assert_eq!(pipe["type"], "loft", "{pipe}");
    assert_eq!(pipe["orientation"], "parallel", "{pipe}");
    assert!(pipe.get("frame").is_none(), "{pipe}");
    // The helix: part of the sweep.
    let coil = feature(&report, "Coil");
    assert_eq!(coil.outcome, ObjectOutcome::Included, "{coil:?}");
    // The sweep along a fixed helix: four turns clockwise, rising.
    let spring = def_of(&doc, &feature(&report, "Spring").features[0]);
    assert_eq!(spring["type"], "sweep", "{spring}");
    assert_eq!(spring["frame"], json!({"type": "frenet"}), "{spring}");
    let helix = &spring["path"]["helix"];
    assert_eq!(helix["center"], json!([5.0, 0.0, 0.0]), "{spring}");
    assert_eq!(helix["axis"], json!([0.0, 0.0, 1.0]), "{spring}");
    assert_eq!(helix["radius"], 10.0, "{spring}");
    assert_eq!(helix["pitch"], -5.0, "{spring}");
    let end = helix["end_angle"].as_f64().unwrap();
    assert!((end + 8.0 * std::f64::consts::PI).abs() < 1e-9, "{spring}");
    // More faces than the eight fixed names (mitcad#547): numbered on.
    assert_eq!(profiles::face_suffix(0), "");
    assert_eq!(profiles::face_suffix(11), " (face 12)");
}

#[test]
fn fixed_curves_and_the_wires_of_sweep_sections() {
    // (mitcad#548) Curves in no plane as a path of fixed curves: a line and
    // a spline; a helix among them has none.
    use mitcad_model::Curve3;
    let line = |a: [f64; 3], b: [f64; 3]| Curve3::Line { start: a, end: b };
    let spline = Curve3::BSpline {
        degree: 2,
        poles: vec![[10.0, 0.0, 0.0], [15.0, 0.0, 0.0], [15.0, 5.0, 5.0]],
        weights: Vec::new(),
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    };
    let path =
        fixed_paths::fixed_curves(&[line([0.0; 3], [10.0, 0.0, 0.0]), spline.clone()]).unwrap();
    let curves = path["fixed_curves"].as_array().unwrap();
    assert_eq!(curves.len(), 2, "{path}");
    assert_eq!(curves[0]["type"], "line", "{path}");
    assert_eq!(curves[1]["type"], "spline", "{path}");
    assert_eq!(curves[1]["degree"], 2, "{path}");
    assert!(curves[1].get("weights").is_none(), "{path}");
    let helix = Curve3::Helix {
        center: [0.0; 3],
        axis: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        radius: 5.0,
        pitch: 2.0,
        start: 0.0,
        end: 1.0,
    };
    assert!(fixed_paths::fixed_curves(&[spline, helix]).is_err());
    // A square is one closed wire, three of its sides one open wire, a
    // circle beside it another closed one.
    let square = [
        line([0.0; 3], [10.0, 0.0, 0.0]),
        line([10.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
        line([10.0, 10.0, 0.0], [0.0, 10.0, 0.0]),
        line([0.0, 10.0, 0.0], [0.0; 3]),
    ];
    assert_eq!(sweep_inputs::wires_of(&square), [true]);
    assert_eq!(sweep_inputs::wires_of(&square[..3]), [false]);
    let mut two = square.to_vec();
    two.push(Curve3::Conic {
        center: [30.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: 3.0,
        minor: 3.0,
        start: 0.0,
        end: std::f64::consts::TAU,
        closed: true,
    });
    assert_eq!(sweep_inputs::wires_of(&two), [true, true]);
}

// Inactive constraints and dimensions (mitcad#721).

/// A 40 x 20 mm rectangle from the root point, held by horizontal and
/// vertical constraints and its two lengths, with its bottom also parallel
/// to its side: a constraint FreeCAD's solution does not meet. It comes in
/// inactive and the report names it; the other constraints stay and the
/// lengths keep driving.
#[test]
fn a_constraint_freecad_s_solution_does_not_meet_comes_in_inactive() {
    let p = [[0.0, 0.0], [40.0, 0.0], [40.0, 20.0], [0.0, 20.0]];
    let profile = format!(
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
            constrain(1, 0.0, &[(0, 1), (-1, 1)], "", true),
            constrain(2, 0.0, &[(0, 0)], "", true),
            constrain(3, 0.0, &[(1, 0)], "", true),
            constrain(2, 0.0, &[(2, 0)], "", true),
            constrain(3, 0.0, &[(3, 0)], "", true),
            constrain(6, 40.0, &[(0, 0)], "width", true),
            constrain(6, 20.0, &[(1, 0)], "height", true),
            // The bottom parallel to the side.
            constrain(4, 0.0, &[(0, 0), (1, 0)], "", true),
        ])
    );
    let xml = document(&[("Sketcher::SketchObject", "Profile", profile)]);
    let (doc, report) = import_with(
        fcstd(&xml, None, &[]),
        Vec::new(),
        MockKernel::default(),
        false,
    );
    let s = &report.sketches[0];
    assert_eq!(s.outcome, ObjectOutcome::Partial, "{s:?}");
    assert!(s.error < 1e-9, "{s:?}");
    assert!(
        s.notes
            .iter()
            .any(|n| n.starts_with("1 constraint inactive, not met in the file: parallel k")),
        "{:?}",
        s.notes
    );
    let def = sketch_def(&doc, &report, "Profile");
    let constraints = def["constraints"].as_array().unwrap();
    let inactive: Vec<&serde_json::Value> = constraints
        .iter()
        .filter(|c| c["active"] == false)
        .collect();
    assert_eq!(inactive.len(), 1, "{def}");
    assert_eq!(inactive[0]["type"], "parallel", "{def}");
    assert_eq!(constraints.len(), 5, "{def}");
    // The lengths are active and driving.
    let dims = def["dimensions"].as_array().unwrap();
    assert_eq!(dims.len(), 2, "{def}");
    for d in dims {
        assert!(
            d.get("active").is_none() && d.get("driven").is_none(),
            "{d}"
        );
    }
    // It solves without an error and fully constrained: the sketch's own
    // solve with the document's values, which the api's `sketch` query
    // reports as its `error` and `dof`.
    let uid = s.feature.as_deref().unwrap();
    let FeatureDef::Sketch(sketch) = &doc.feature(feature_uid(uid)).unwrap().def else {
        panic!("{uid} is not a sketch");
    };
    let params = doc.parameters();
    let solved = sketch
        .solve_with(
            &mut |id| {
                params
                    .value(id)
                    .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
            },
            &|id| params.name(id),
        )
        .unwrap_or_else(|e| panic!("{uid} does not solve: {}", e.message));
    assert_eq!(solved.status.dof, 0, "{uid}");
}

// Colours and fillet edges that follow sketch edits (mitcad#793).

/// A view provider of `GuiDocument.xml` with these properties.
fn view(name: &str, properties: &[String]) -> String {
    format!(
        "<ViewProvider name=\"{name}\"><Properties>{}</Properties></ViewProvider>",
        properties.concat()
    )
}

fn gui_document(views: &[String]) -> String {
    format!(
        "<Document SchemaVersion=\"1\"><ViewProviderData Count=\"{}\">{}</ViewProviderData></Document>",
        views.len(),
        views.concat()
    )
}

fn shape_color(packed: u32) -> String {
    prop(
        "ShapeColor",
        "App::PropertyColor",
        &format!("<PropertyColor value=\"{packed}\"/>"),
    )
}

/// 0.21's colours per face (packed `r << 24 | g << 16 | b << 8 | a`), as
/// FreeCAD writes them to a file of the archive.
fn color_file(colors: &[u32]) -> Vec<u8> {
    let mut data = (colors.len() as u32).to_le_bytes().to_vec();
    for c in colors {
        data.extend(c.to_le_bytes());
    }
    data
}

/// 1.0's shape appearance: one opaque material per diffuse colour.
fn material_file(colors: &[u32]) -> Vec<u8> {
    let mut data = (colors.len() as u32).to_le_bytes().to_vec();
    for c in colors {
        for packed in [0x3333_3300u32, *c, 0, 0] {
            data.extend(packed.to_le_bytes());
        }
        data.extend(0.9f32.to_le_bytes());
        data.extend(0.0f32.to_le_bytes());
    }
    data
}

const RED: u32 = 0xFF00_0000;
const GREEN: u32 = 0x00FF_0000;
const BLUE: u32 = 0x0000_FF00;
/// FreeCAD 0.21's default grey.
const GREY: u32 = 0xCCCC_CC00;

/// The name of a body's face at an index of its shape.
fn face_at(doc: &Document<MockKernel>, body: mitcad_model::BodyUid, index: usize) -> String {
    doc.kernel()
        .indexed_element(
            doc.body_shape(body).unwrap(),
            mitcad_model::ElementKind::Face,
            index,
        )
        .unwrap()
        .unwrap()
        .name
        .unwrap()
}

#[test]
fn body_and_face_colours_become_appearances() {
    // A stored red box, a stored box of FreeCAD's grey, a Body rebuilt by
    // the history with 1.0's appearance per face (blue, one face red) and
    // a Body whose last feature falls back with 0.21's colours (green, one
    // face blue). The same colour is one appearance.
    let xml = document(&[
        (
            "Part::Box",
            "Box",
            format!("{}{}", label("Red box"), shape("Box.brp")),
        ),
        ("Part::Box", "Plain", shape("Box.brp")),
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}{}",
                label("Plate"),
                shape("Body.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad"]),
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
                "{}{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                float("Length", 10.0),
                enumeration("Type", 0),
                shape("Pad.brp")
            ),
        ),
        (
            "PartDesign::Body",
            "Body2",
            format!(
                "{}{}{}{}",
                label("Gearbox"),
                shape("Body2.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch2", "Pad2", "Gear"]
                ),
                link("Tip", "Gear")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch2",
            rectangle_sketch(100.0, 0.0, 20.0, 20.0),
        ),
        (
            "PartDesign::Pad",
            "Pad2",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Sketch2", &[]),
                float("Length", 10.0),
                enumeration("Type", 0),
                shape("Pad2.brp")
            ),
        ),
        (
            "PartDesign::FeaturePython",
            "Gear",
            format!("{}{}", link("BaseFeature", "Pad2"), shape("Gear.brp")),
        ),
    ]);
    let gui = gui_document(&[
        view("Box", &[shape_color(RED)]),
        view("Plain", &[shape_color(GREY)]),
        view(
            "Body",
            &[prop(
                "ShapeAppearance",
                "App::PropertyMaterialList",
                "<MaterialList file=\"BodyLook.bin\"/>",
            )],
        ),
        view(
            "Body2",
            &[
                shape_color(GREEN),
                prop(
                    "DiffuseColor",
                    "App::PropertyColorList",
                    "<ColorList file=\"Body2Colors.bin\"/>",
                ),
            ],
        ),
    ]);
    let look = material_file(&[BLUE, RED, BLUE, BLUE, BLUE, BLUE]);
    let mut faces2 = vec![GREEN; 8];
    faces2[5] = BLUE;
    let colors2 = color_file(&faces2);
    let file = || {
        fcstd(
            &xml,
            Some(&gui),
            &[
                ("Box.brp", b"6 box"),
                ("Body.brp", b"6 body"),
                ("Pad.brp", b"6 pad"),
                ("Body2.brp", b"8 body2"),
                ("Pad2.brp", b"6 pad2"),
                ("Gear.brp", b"8 gear"),
                ("BodyLook.bin", look.as_slice()),
                ("Body2Colors.bin", colors2.as_slice()),
            ],
        )
    };
    for bodies_only in [false, true] {
        let (doc, report) = import_with(file(), Vec::new(), MockKernel::default(), bodies_only);
        let id = |name: &str| {
            doc.custom_appearances()
                .iter()
                .find(|a| a.name == name)
                .map(|a| a.id.clone())
                .unwrap_or_else(|| panic!("no appearance {name}: {:?}", doc.custom_appearances()))
        };
        let (red, green, blue) = (
            id("FreeCAD colour 255 0 0"),
            id("FreeCAD colour 0 255 0"),
            id("FreeCAD colour 0 0 255"),
        );
        // One appearance per colour, none of FreeCAD's grey.
        assert_eq!(report.appearances, 3, "{:?}", doc.custom_appearances());
        assert_eq!(doc.custom_appearances().len(), 3);
        let red_look = doc.appearance(&red).unwrap();
        assert_eq!(red_look.base_color, [1.0, 0.0, 0.0]);
        assert!(
            !report.warnings.iter().any(|w| w.contains("colour")),
            "{:?}",
            report.warnings
        );
        let body_of = |object: &str| -> mitcad_model::BodyUid {
            item(&report, object).bodies[0].parse().unwrap()
        };
        let attributes = |object: &str| doc.body_attributes(body_of(object));
        assert_eq!(attributes("Box").appearance.as_deref(), Some(red.as_str()));
        assert!(attributes("Box").face_appearances.is_empty());
        assert_eq!(attributes("Plain").appearance, None);
        // The Body: rebuilt by the history (or stored, bodies only), blue,
        // its second face red.
        if !bodies_only {
            assert_eq!(
                feature(&report, "Pad").outcome,
                ObjectOutcome::Parametric,
                "{:?}",
                report.features
            );
            assert_eq!(feature(&report, "Gear").outcome, ObjectOutcome::Fallback);
        }
        let plate = attributes("Body");
        assert_eq!(plate.appearance.as_deref(), Some(blue.as_str()));
        let face = face_at(&doc, body_of("Body"), 1);
        assert_eq!(
            plate.face_appearances,
            [(face, red.clone())].into_iter().collect(),
            "bodies only: {bodies_only}"
        );
        // The Body ending in a fallback: green, its sixth face blue.
        let gearbox = attributes("Body2");
        assert_eq!(gearbox.appearance.as_deref(), Some(green.as_str()));
        let face = face_at(&doc, body_of("Body2"), 5);
        assert_eq!(
            gearbox.face_appearances,
            [(face, blue.clone())].into_iter().collect(),
            "bodies only: {bodies_only}"
        );
        // The report says what each body got.
        let row = report.bodies.iter().find(|b| b.object == "Body").unwrap();
        assert_eq!(row.color, Some([0.0, 0.0, 1.0]));
        assert_eq!(row.appearance.as_deref(), Some(blue.as_str()));
        assert_eq!(row.face_appearances, 1);
        let row = report.bodies.iter().find(|b| b.object == "Plain").unwrap();
        assert_eq!((row.color, row.appearance.as_deref()), (None, None));
        assert!(report.text().contains("1 faces of their own colour"));
        // Still one undo step.
        assert_eq!(doc.undo_depth(), 1);
    }
}

#[test]
fn fillets_of_all_edges_and_merged_edges_follow_a_sketch_edit() {
    use mitcad_model::{Curve3, EdgeName};
    // A Body: a sketch whose bottom side is two lines along one line (FreeCAD
    // refines the pad's faces into one; Mitcad keeps two), with a named
    // DistanceX on the second; a pad; a fillet of the pad's merged top
    // edge along that side (its Edge3); a fillet of every edge.
    let sketch = format!(
        "{}{}",
        geometry(&[
            segment([0.0, 0.0], [20.0, 0.0], false),
            segment([20.0, 0.0], [40.0, 0.0], false),
            segment([40.0, 0.0], [40.0, 30.0], false),
            segment([40.0, 30.0], [0.0, 30.0], false),
            segment([0.0, 30.0], [0.0, 0.0], false),
        ]),
        constraints(&[
            constrain(1, 0.0, &[(0, 2), (1, 1)], "", true),
            constrain(1, 0.0, &[(1, 2), (2, 1)], "", true),
            constrain(1, 0.0, &[(2, 2), (3, 1)], "", true),
            constrain(1, 0.0, &[(3, 2), (4, 1)], "", true),
            constrain(1, 0.0, &[(4, 2), (0, 1)], "", true),
            constrain(7, 20.0, &[(1, 0)], "Width", true),
        ])
    );
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch", "Pad", "Round", "RoundAll"]
                ),
                link("Tip", "RoundAll")
            ),
        ),
        ("Sketcher::SketchObject", "Sketch", sketch),
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
            "PartDesign::Fillet",
            "Round",
            format!(
                "{}{}{}",
                link_sub("Base", "Pad", &["Edge3"]),
                float("Radius", 2.0),
                shape("Round.brp")
            ),
        ),
        (
            "PartDesign::Fillet",
            "RoundAll",
            format!(
                "{}{}{}{}",
                link_sub("Base", "Round", &["Edge1"]),
                flag("UseAllEdges", true),
                float("Radius", 1.0),
                shape("RoundAll.brp")
            ),
        ),
    ]);
    let file = || {
        fcstd(
            &xml,
            None,
            &[
                ("Body.brp", b"30 body"),
                ("Pad.brp", b"7 pad"),
                ("Round.brp", b"9 round"),
                ("RoundAll.brp", b"30 all"),
            ],
        )
    };

    // A first import gives Mitcad's names: the pad's top face and the side
    // faces of the two bottom lines.
    let (doc, report) = import_with(file(), Vec::new(), MockKernel::default(), false);
    let pad = feature(&report, "Pad").features[0].clone();
    let def = sketch_def(&doc, &report, "Sketch");
    let entities = def["entities"].as_array().unwrap();
    let at = |id: &serde_json::Value| -> [f64; 2] {
        let e = entities.iter().find(|e| e["id"] == *id).unwrap();
        [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
    };
    let mut bottom: Vec<(f64, String)> = entities
        .iter()
        .filter(|e| e["type"] == "line")
        .filter(|e| at(&e["start"])[1].abs() < 1e-6 && at(&e["end"])[1].abs() < 1e-6)
        .map(|e| {
            (
                at(&e["start"])[0] + at(&e["end"])[0],
                e["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    bottom.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(bottom.len(), 2, "{def}");
    let body: mitcad_model::BodyUid = item(&report, "Body").bodies[0].parse().unwrap();
    let faces: Vec<String> = doc
        .body_shape(body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect();
    let top = faces
        .iter()
        .find(|f| f.starts_with(&format!("{pad}:end(")))
        .cloned()
        .unwrap_or_else(|| panic!("no top face in {faces:?}"));
    let edge = |curve: &str| {
        let prefix = format!("{pad}:side({curve}");
        let side = faces
            .iter()
            .find(|f| f.starts_with(&format!("{prefix}[")) || **f == format!("{prefix})"))
            .unwrap_or_else(|| panic!("no side face {prefix} in {faces:?}"));
        EdgeName::new(side.parse().unwrap(), top.parse().unwrap()).to_string()
    };
    let (edge_a, edge_b) = (edge(&bottom[0].1), edge(&bottom[1].1));
    // FreeCAD's Edge3 of the pad's stored shape.
    let (stored, _) = MockKernel::default()
        .import_brep(FeatureUid(0), b"7 pad", 0)
        .unwrap();
    let (x, y) = &stored.edges[2];
    let stored_edge = EdgeName::new(x.clone(), y.clone()).to_string();

    // Geometry: FreeCAD's edge runs along the whole side, Mitcad's two
    // edges each along a half; every other edge is elsewhere.
    let kernel = MockKernel::default();
    kernel.curves.borrow_mut().push(Curve3::Line {
        start: [500.0, 500.0, 500.0],
        end: [501.0, 500.0, 500.0],
    });
    let line = |from: f64, to: f64| Curve3::Line {
        start: [from, 0.0, 10.0],
        end: [to, 0.0, 10.0],
    };
    kernel.named_curves.borrow_mut().extend([
        (stored_edge, line(0.0, 40.0)),
        (edge_a.clone(), line(0.0, 20.0)),
        (edge_b.clone(), line(20.0, 40.0)),
    ]);
    let (mut doc, report) = import_with(file(), Vec::new(), kernel, false);

    // The merged edge: both of Mitcad's edges, by the faces they lie
    // between.
    let round = feature(&report, "Round");
    assert_eq!(round.outcome, ObjectOutcome::Parametric, "{round:?}");
    let def = def_of(&doc, &round.features[0]);
    let mut edges: Vec<&str> = def["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("{def}"))
        .iter()
        .map(|e| e.as_str().unwrap())
        .collect();
    edges.sort_unstable();
    let mut expected = [edge_a.as_str(), edge_b.as_str()];
    expected.sort_unstable();
    assert_eq!(edges, expected, "{def}");
    // Every edge: a whole-body set, not a list.
    let all = feature(&report, "RoundAll");
    assert_eq!(all.outcome, ObjectOutcome::Parametric, "{all:?}");
    let def = def_of(&doc, &all.features[0]);
    assert_eq!(def["sets"][0]["all_edges"], true, "{def}");
    assert!(def["sets"][0].get("edges").is_none(), "{def}");
    assert!(def.get("edges").is_none(), "{def}");

    // A sketch dimension changed afterwards: both fillets recompute on
    // the same edges with no error.
    assert!(
        report.parameters.iter().any(|p| p.name == "Width"),
        "{:?}",
        report.parameters
    );
    doc.set_parameter_expression("Width", "25 mm", None)
        .unwrap();
    let width = doc.parameters().find("Width").unwrap();
    assert_eq!(doc.parameters().value(width), Some(25.0));
    let round_uid: FeatureUid = round.features[0].parse().unwrap();
    for f in [round, all] {
        let uid: FeatureUid = f.features[0].parse().unwrap();
        assert!(
            doc.status(uid).and_then(|s| s.error()).is_none(),
            "{}: {:?}",
            f.label,
            doc.status(uid)
        );
    }
    let body: mitcad_model::BodyUid = item(&report, "Body").bodies[0].parse().unwrap();
    let shape = doc.body_shape(body).unwrap();
    let rounded: Vec<String> = shape
        .faces
        .iter()
        .map(ToString::to_string)
        .filter(|f| f.starts_with(&format!("{round_uid}:fillet(")))
        .collect();
    assert_eq!(rounded.len(), 2, "{rounded:?}");
    for edge in [&edge_a, &edge_b] {
        assert!(
            rounded.contains(&format!("{round_uid}:fillet({edge})")),
            "{edge}: {rounded:?}"
        );
    }
    assert!(
        shape.history.starts_with("fillet(fillet("),
        "{}",
        shape.history
    );
}

/// A sketch point (`Part::GeomPoint`).
fn sketch_point(at: [f64; 2], construction: bool) -> String {
    format!(
        "<Geometry type=\"Part::GeomPoint\"><GeomPoint X=\"{}\" Y=\"{}\" Z=\"0\"/><Construction value=\"{}\"/></Geometry>",
        at[0],
        at[1],
        u8::from(construction)
    )
}

#[test]
fn a_hole_drills_each_sketch_point_once() {
    // (mitcad#835) A through-all hole from a sketch of points, linked by
    // one empty sub-element as FreeCAD 1.1 links it: two coincident points
    // (one Mitcad point), a construction point (not in FreeCAD's sketch
    // shape, so not drilled) and a point of its own.
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch", "Pad", "Points", "Hole"]
                ),
                link("Tip", "Hole")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch",
            rectangle_sketch(0.0, 0.0, 50.0, 30.0),
        ),
        (
            "PartDesign::Pad",
            "Pad",
            format!(
                "{}{}{}",
                link_sub("Profile", "Sketch", &[""]),
                float("Length", 15.0),
                shape("Pad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Points",
            format!(
                "{}{}",
                geometry(&[
                    sketch_point([15.0, 10.0], false),
                    sketch_point([15.0, 10.0], false),
                    sketch_point([25.0, 20.0], true),
                    sketch_point([35.0, 10.0], false),
                ]),
                constraints(&[constrain(1, 0.0, &[(1, 1), (0, 1)], "", true)])
            ),
        ),
        (
            "PartDesign::Hole",
            "Hole",
            format!(
                "{}{}{}{}{}{}{}",
                link_sub("Profile", "Points", &[""]),
                link("BaseFeature", "Pad"),
                float("Diameter", 9.0),
                enumeration("DepthType", 1),
                enumeration("DrillPoint", 1),
                enumeration("HoleCutType", 0),
                shape("Hole.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"9 body"),
            ("Pad.brp", b"6 pad"),
            ("Hole.brp", b"9 hole"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let hole = feature(&report, "Hole");
    assert_eq!(hole.outcome, ObjectOutcome::Parametric, "{hole:?}");
    let def = &defs_of(&doc, &report, "Hole")[0];
    assert_eq!(def["type"], "hole", "{def}");
    assert_eq!(def["extent"]["type"], "through_all", "{def}");
    // Simple (the default kind, not stored), with an angled point.
    assert!(
        def["kind"].is_null() && !def["tip_angle"].is_null(),
        "{def}"
    );
    // One hole at each distinct, drawn point, in the sketch's order.
    let sketch = sketch_def(&doc, &report, "Points");
    let at = |id: &serde_json::Value| {
        sketch["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == *id)
            .map(|e| e["at"].clone())
            .unwrap_or_else(|| panic!("no entity {id} in {sketch}"))
    };
    let points: Vec<serde_json::Value> = def["placement"]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(at)
        .collect();
    assert_eq!(points, [json!([15.0, 10.0]), json!([35.0, 10.0])], "{def}");
}

// Binders and external geometry on sketches (mitcad#952).

/// An `App::PropertyXLink` with a sub-element path.
fn xlink_sub(name: &str, target: &str, sub: &str) -> String {
    prop(
        name,
        "App::PropertyXLink",
        &format!("<XLink file=\"\" stamp=\"\" name=\"{target}\" sub=\"{sub}\"/>"),
    )
}

/// An `App::PropertyXLinkSubList` of (object, sub-element path).
fn xlink_list(name: &str, targets: &[(&str, &str)]) -> String {
    let items: String = targets
        .iter()
        .map(|(t, sub)| format!("<XLink file=\"\" stamp=\"\" name=\"{t}\" sub=\"{sub}\"/>"))
        .collect();
    prop(
        name,
        "App::PropertyXLinkSubList",
        &format!(
            "<XLinkSubList count=\"{}\">{items}</XLinkSubList>",
            targets.len()
        ),
    )
}

/// A sketch's `ExternalGeometry`: (object, element) each.
fn external_links(links: &[(&str, &str)]) -> String {
    let items: String = links
        .iter()
        .map(|(o, sub)| format!("<Link obj=\"{o}\" sub=\"{sub}\"/>"))
        .collect();
    prop(
        "ExternalGeometry",
        "App::PropertyLinkSubList",
        &format!(
            "<LinkSubList count=\"{}\">{items}</LinkSubList>",
            links.len()
        ),
    )
}

/// A sketch's copy of its external geometry (1.0): the H and V axes, then
/// one geometry per link.
fn external_copy(items: &[String]) -> String {
    let axis = |x: f64, y: f64| {
        format!(
            "<Geometry type=\"Part::GeomLineSegment\"><LineSegment StartX=\"0\" StartY=\"0\" StartZ=\"0\" EndX=\"{x}\" EndY=\"{y}\" EndZ=\"0\"/><Construction value=\"1\"/></Geometry>"
        )
    };
    prop(
        "ExternalGeo",
        "Part::PropertyGeometryList",
        &format!(
            "<GeometryList count=\"{}\">{}{}{}</GeometryList>",
            items.len() + 2,
            axis(1.0, 0.0),
            axis(0.0, 1.0),
            items.concat()
        ),
    )
}

/// The name of a mock stored shape's edge (`"<faces> <name>"` data).
fn stored_edge_name(data: &[u8], index: usize) -> String {
    let (stored, _) = MockKernel::default()
        .import_brep(FeatureUid(0), data, 0)
        .unwrap();
    let (a, b) = &stored.edges[index];
    mitcad_model::EdgeName::new(a.clone(), b.clone()).to_string()
}

/// A Body: a 40 × 30 rectangle sketch on XY (with its stored shape), a
/// pad, a binder of the sketch whose `Context` names the Body (as FreeCAD
/// writes it), and a sketch 10 mm up whose external geometry is the
/// binder's first edge (the rectangle's bottom line), the sketch's second
/// edge (its right side) and the sketch's first vertex (its far corner);
/// a line of its own starts on the binder's edge and ends on it.
fn binder_document() -> FcstdFile {
    let side = format!(
        "{}{}{}{}{}",
        placement([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 0.0),
        external_links(&[
            ("Binder", "Edge1"),
            ("Sketch", "Edge2"),
            ("Sketch", "Vertex1")
        ]),
        external_copy(&[
            segment([0.0, 0.0], [40.0, 0.0], true),
            segment([40.0, 0.0], [40.0, 30.0], true),
            sketch_point([40.0, 30.0], true),
        ]),
        geometry(&[segment([0.0, 0.0], [20.0, 0.0], false)]),
        constraints(&[
            constrain(1, 0.0, &[(0, 1), (-3, 1)], "", true),
            constrain(13, 0.0, &[(0, 2), (-3, 0)], "", true),
        ])
    );
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch", "Pad", "Binder", "Side"]
                ),
                link("Tip", "Pad")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch",
            format!(
                "{}{}",
                rectangle_sketch(0.0, 0.0, 40.0, 30.0),
                shape("Sketch.brp")
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
            "PartDesign::SubShapeBinder",
            "Binder",
            format!(
                "{}{}{}",
                xlink_sub("Context", "Body", "Binder."),
                xlink_list("Support", &[("Sketch", "")]),
                shape("Binder.brp")
            ),
        ),
        ("Sketcher::SketchObject", "Side", side),
    ]);
    fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"6 body"),
            ("Pad.brp", b"6 pad"),
            ("Sketch.brp", b"4 sketch"),
            ("Binder.brp", b"4 binder"),
        ],
    )
}

/// The mock kernel of [`binder_document`]: the stored shapes' first edge
/// is the rectangle's bottom line, the second its right side (the sketch's
/// and the binder's alike), the first vertex its far corner.
fn binder_kernel() -> MockKernel {
    use mitcad_model::Curve3;
    let kernel = MockKernel::default();
    kernel.named_curves.borrow_mut().extend([
        (
            stored_edge_name(b"4 binder", 0),
            Curve3::Line {
                start: [0.0, 0.0, 0.0],
                end: [40.0, 0.0, 0.0],
            },
        ),
        (
            stored_edge_name(b"4 binder", 1),
            Curve3::Line {
                start: [40.0, 0.0, 0.0],
                end: [40.0, 30.0, 0.0],
            },
        ),
    ]);
    kernel
        .vertices
        .borrow_mut()
        .push(("corner".to_owned(), [40.0, 30.0, 0.0]));
    kernel
}

#[test]
fn a_binder_s_context_does_not_consume_its_body() {
    // (mitcad#952) FreeCAD writes a binder's `Context` as the Body it is
    // in: the Body stays a body with its features, not "used by" the
    // binder; the binder itself is skipped.
    let (doc, report) = import_with(binder_document(), Vec::new(), binder_kernel(), false);
    let body = item(&report, "Body");
    assert_eq!(body.outcome, ObjectOutcome::Body, "{body:?}");
    assert_eq!(body.bodies.len(), 1, "{body:?}");
    assert!(
        !body.note.as_deref().unwrap_or_default().contains("used by"),
        "{body:?}"
    );
    assert_eq!(doc.bodies().len(), 1);
    let pad = feature(&report, "Pad");
    assert_eq!(pad.outcome, ObjectOutcome::Parametric, "{pad:?}");
    assert_eq!(def_of(&doc, &pad.features[0])["type"], "extrude");
    let binder = feature(&report, "Binder");
    assert_eq!(binder.outcome, ObjectOutcome::Skipped, "{binder:?}");
    assert!(binder.notes[0].contains("a binder"), "{binder:?}");
    // The sketch on the binder after the sketch it binds.
    let order: Vec<&str> = report.sketches.iter().map(|s| s.object.as_str()).collect();
    assert_eq!(order, ["Sketch", "Side"]);
}

#[test]
fn external_geometry_of_a_binder_and_a_sketch_projects_the_sketch_s_entities() {
    // (mitcad#952, mitcad#904) The binder's edge is the rectangle's bottom
    // line: a linked projection of the sketch's curve; the sketch's own
    // edge and vertex likewise of its curve and point; none fixed in place.
    let (doc, report) = import_with(binder_document(), Vec::new(), binder_kernel(), false);
    let side = report.sketches.iter().find(|s| s.object == "Side").unwrap();
    assert_eq!(side.outcome, ObjectOutcome::Parametric, "{side:?}");
    assert!(side.dropped.is_empty(), "{side:?}");
    let sketch = report
        .sketches
        .iter()
        .find(|s| s.object == "Sketch")
        .unwrap();
    let sketch_uid = sketch.feature.clone().unwrap();
    let source = sketch_def(&doc, &report, "Sketch");
    let entities = source["entities"].as_array().unwrap();
    let at = |id: &serde_json::Value| -> [f64; 2] {
        let e = entities.iter().find(|e| e["id"] == *id).unwrap();
        [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
    };
    let near = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-9;
    let line = |a: [f64; 2], b: [f64; 2]| {
        entities
            .iter()
            .find(|e| {
                e["type"] == "line"
                    && (near(at(&e["start"]), a) && near(at(&e["end"]), b)
                        || near(at(&e["start"]), b) && near(at(&e["end"]), a))
            })
            .unwrap_or_else(|| panic!("no line {a:?}-{b:?} in {source}"))["id"]
            .clone()
    };
    let bottom = line([0.0, 0.0], [40.0, 0.0]);
    let right = line([40.0, 0.0], [40.0, 30.0]);
    let corner = entities
        .iter()
        .find(|e| e["type"] == "point" && near(at(&e["id"]), [40.0, 30.0]))
        .unwrap_or_else(|| panic!("no corner in {source}"))["id"]
        .clone();
    let def = sketch_def(&doc, &report, "Side");
    let projections = def["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 3, "{def}");
    assert_eq!(
        projections[0]["source"],
        json!({"sketch": sketch_uid, "curve": bottom}),
        "{def}"
    );
    assert_eq!(
        projections[1]["source"],
        json!({"sketch": sketch_uid, "curve": right}),
        "{def}"
    );
    assert_eq!(
        projections[2]["source"],
        json!({"sketch": sketch_uid, "point": corner}),
        "{def}"
    );
    assert!(projections.iter().all(|p| p.get("body").is_none()), "{def}");
    // The constraints on the binder's edge are on the projected line.
    let own = def["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "line" && e["reference"].is_null())
        .unwrap();
    let first = projections[0]["entities"].as_array().unwrap();
    assert!(first.contains(&own["start"]), "{def}");
    // The sketch evaluates with them.
    let uid = feature_uid(side.feature.as_deref().unwrap());
    assert!(
        doc.status(uid).and_then(|s| s.error()).is_none(),
        "{:?}",
        doc.status(uid)
    );
}

#[test]
fn external_geometry_on_a_pad_s_edge_follows_the_pad_s_length() {
    use mitcad_model::{Curve3, EdgeName};
    // (mitcad#952) A sketch on XZ (FreeCAD's sketch y is +Z) projects the
    // pad's top edge above the profile's bottom line (FreeCAD's Edge3 of
    // the pad): linked to Mitcad's edge, it moves up when the pad gets
    // longer.
    let side = format!(
        "{}{}{}{}{}",
        placement([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 90.0),
        external_links(&[("Pad", "Edge3")]),
        external_copy(&[segment([0.0, 10.0], [40.0, 10.0], true)]),
        geometry(&[segment([0.0, 10.0], [20.0, 10.0], false)]),
        constraints(&[
            constrain(1, 0.0, &[(0, 1), (-3, 1)], "", true),
            constrain(13, 0.0, &[(0, 2), (-3, 0)], "", true),
        ])
    );
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad", "Side"]),
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
                "{}{}{}{}",
                link_sub("Profile", "Sketch", &[]),
                float("Length", 10.0),
                enumeration("Type", 0),
                shape("Pad.brp")
            ),
        ),
        ("Sketcher::SketchObject", "Side", side),
    ]);
    let file = || {
        fcstd(
            &xml,
            None,
            &[("Body.brp", b"6 body"), ("Pad.brp", b"6 pad")],
        )
    };

    // A first import gives Mitcad's names: the pad's top face and the side
    // face of the profile's bottom line, the edge between them.
    let (doc, report) = import_with(file(), Vec::new(), MockKernel::default(), false);
    let pad = feature(&report, "Pad").features[0].clone();
    let def = sketch_def(&doc, &report, "Sketch");
    let entities = def["entities"].as_array().unwrap();
    let at = |id: &serde_json::Value| -> [f64; 2] {
        let e = entities.iter().find(|e| e["id"] == *id).unwrap();
        [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
    };
    let bottom = entities
        .iter()
        .find(|e| {
            e["type"] == "line" && at(&e["start"])[1].abs() < 1e-6 && at(&e["end"])[1].abs() < 1e-6
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let body: mitcad_model::BodyUid = item(&report, "Body").bodies[0].parse().unwrap();
    let faces: Vec<String> = doc
        .body_shape(body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect();
    let top = faces
        .iter()
        .find(|f| f.starts_with(&format!("{pad}:end(")))
        .unwrap_or_else(|| panic!("no top face in {faces:?}"));
    let prefix = format!("{pad}:side({bottom}");
    let side_face = faces
        .iter()
        .find(|f| f.starts_with(&format!("{prefix}[")) || **f == format!("{prefix})"))
        .unwrap_or_else(|| panic!("no side face {prefix} in {faces:?}"));
    let edge = EdgeName::new(side_face.parse().unwrap(), top.parse().unwrap()).to_string();

    // Geometry: FreeCAD's Edge3 and Mitcad's edge are the top edge above
    // the bottom line; every other edge is elsewhere.
    let kernel = MockKernel::default();
    kernel.curves.borrow_mut().push(Curve3::Line {
        start: [500.0, 500.0, 500.0],
        end: [501.0, 500.0, 500.0],
    });
    let top_edge = |z: f64| Curve3::Line {
        start: [0.0, 0.0, z],
        end: [40.0, 0.0, z],
    };
    kernel.named_curves.borrow_mut().extend([
        (stored_edge_name(b"6 pad", 2), top_edge(10.0)),
        (edge.clone(), top_edge(10.0)),
    ]);
    let (mut doc, report) = import_with(file(), Vec::new(), kernel, false);
    let s = report.sketches.iter().find(|s| s.object == "Side").unwrap();
    assert_eq!(s.outcome, ObjectOutcome::Parametric, "{s:?}");
    assert!(s.dropped.is_empty(), "{s:?}");
    let def = sketch_def(&doc, &report, "Side");
    let projection = def["projections"][0].clone();
    assert_eq!(projection["source"], edge, "{def}");
    assert_eq!(projection["body"], body.to_string(), "{def}");

    // The pad 15 mm long: its top edge (as the kernel then gives it) and
    // the projection with it.
    let pad_def = def_of(&doc, &feature(&report, "Pad").features[0]);
    let length = pad_def["extent"]["distance"].as_str().unwrap().to_owned();
    doc.kernel()
        .named_curves
        .borrow_mut()
        .insert(edge.clone(), top_edge(15.0));
    doc.set_parameter_expression(&length, "15 mm", None)
        .unwrap();
    let uid = feature_uid(s.feature.as_deref().unwrap());
    assert!(
        doc.status(uid).and_then(|st| st.error()).is_none(),
        "{:?}",
        doc.status(uid)
    );
    let output = doc.sketch_output(uid).unwrap();
    let projected: u32 = projection["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|e| e.as_str()?.strip_prefix('c')?.parse().ok())
        .unwrap();
    let Some(mitcad_model::sketch::geometry::Curve2::Line { a, b }) = output
        .solved
        .curves
        .get(&mitcad_model::EntityUid(projected))
    else {
        panic!("no projected line c{projected}: {def}");
    };
    for p in [a, b] {
        let z = output.frame.point(*p)[2];
        assert!((z - 15.0).abs() < 1e-9, "{z}: {def}");
    }
}

#[test]
fn external_geometry_on_another_body_s_sketch_is_linked_through_its_occurrence() {
    use mitcad_model::Curve3;
    use std::f64::consts::TAU;
    // (mitcad#952) A Body 50 mm up (a component of its own) has a sketch
    // whose external geometry is the outer of two concentric circles of a
    // sketch in another Body (at the document's top): a linked projection
    // of that circle (not of the inner one), seen through the occurrence.
    let upper = format!(
        "{}{}{}",
        external_links(&[("Sketch", "Edge2")]),
        external_copy(&[round([0.0, 0.0], 20.0)]),
        geometry(&[segment([20.0, 0.0], [30.0, 0.0], false)])
    );
    let xml = document(&[
        (
            "PartDesign::Body",
            "Base",
            format!(
                "{}{}{}",
                shape("Base.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad"]),
                link("Tip", "Pad")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch",
            format!(
                "{}{}",
                geometry(&[round([0.0, 0.0], 10.0), round([0.0, 0.0], 20.0)]),
                shape("Sketch.brp")
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
            "PartDesign::Body",
            "Top",
            format!(
                "{}{}{}",
                shape("Top.brp"),
                links("Group", "App::PropertyLinkList", &["Upper"]),
                placement([0.0, 0.0, 50.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        ("Sketcher::SketchObject", "Upper", upper),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Base.brp", b"6 base"),
            ("Pad.brp", b"6 pad"),
            ("Top.brp", b"6 top"),
            ("Sketch.brp", b"3 sketch"),
        ],
    );
    // The sketch's stored edges: its inner circle, then its outer one.
    let circle = |radius: f64| Curve3::Conic {
        center: [0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: radius,
        minor: radius,
        start: 0.0,
        end: TAU,
        closed: true,
    };
    let kernel = MockKernel::default();
    kernel.named_curves.borrow_mut().extend([
        (stored_edge_name(b"3 sketch", 0), circle(10.0)),
        (stored_edge_name(b"3 sketch", 1), circle(20.0)),
    ]);
    let (doc, report) = import_with(file, Vec::new(), kernel, false);
    let s = report
        .sketches
        .iter()
        .find(|s| s.object == "Upper")
        .unwrap();
    assert_eq!(s.outcome, ObjectOutcome::Parametric, "{s:?}");
    assert!(s.dropped.is_empty(), "{s:?}");
    assert!(s.component.is_some(), "{s:?}");
    let source = report
        .sketches
        .iter()
        .find(|s| s.object == "Sketch")
        .unwrap();
    let source_uid = source.feature.clone().unwrap();
    let solved = doc.sketch_output(feature_uid(&source_uid)).unwrap();
    let outer = solved
        .solved
        .radii
        .iter()
        .find(|(_, r)| (**r - 20.0).abs() < 1e-9)
        .map(|(id, _)| id.0)
        .unwrap();
    let def = sketch_def(&doc, &report, "Upper");
    let projection = &def["projections"][0];
    assert_eq!(
        projection["source"],
        json!({"sketch": source_uid, "curve": format!("c{outer}")}),
        "{def}"
    );
    // Seen from the root's sketch through the Body's occurrence.
    assert!(projection["link"]["target"].is_string(), "{def}");
    assert!(projection["link"].get("source").is_none(), "{def}");
    let uid = feature_uid(s.feature.as_deref().unwrap());
    assert!(
        doc.status(uid).and_then(|st| st.error()).is_none(),
        "{:?}",
        doc.status(uid)
    );
}

// A reported assembly's leftovers (mitcad#955).

/// An arc of a circle from `start` to `end` (radians, counter-clockwise).
fn arc(center: [f64; 2], radius: f64, start: f64, end: f64) -> String {
    format!(
        "<Geometry type=\"Part::GeomArcOfCircle\"><ArcOfCircle CenterX=\"{}\" CenterY=\"{}\" CenterZ=\"0\" NormalX=\"0\" NormalY=\"0\" NormalZ=\"1\" AngleXU=\"0\" Radius=\"{radius}\" StartAngle=\"{start}\" EndAngle=\"{end}\"/><Construction value=\"0\"/></Geometry>",
        center[0], center[1]
    )
}

#[test]
fn distances_between_circles_arcs_lines_and_points_are_dimensions() {
    use std::f64::consts::PI;
    // A circle of radius 5 at the origin; a circle of radius 3 outside it
    // (a gap of 12); an arc of radius 2 inside it (a gap of 2 to the
    // outer circle); a line 5 below it; a point 3 above it; a line across
    // it 4 from its centre (1 inside); a point inside it (2 from it).
    let profile = format!(
        "{}{}",
        geometry(&[
            round([0.0, 0.0], 5.0),
            round([20.0, 0.0], 3.0),
            arc([1.0, 0.0], 2.0, 0.0, PI),
            segment([-10.0, -10.0], [10.0, -10.0], false),
            sketch_point([0.0, 8.0], false),
            segment([-10.0, 4.0], [10.0, 4.0], false),
            sketch_point([3.0, 0.0], false),
        ]),
        constraints(&[
            constrain(6, 12.0, &[(0, 0), (1, 0)], "gap", true),
            constrain(6, 2.0, &[(0, 0), (2, 0)], "", true),
            constrain(6, 5.0, &[(3, 0), (0, 0)], "", true),
            constrain(6, 3.0, &[(4, 1), (0, 0)], "", true),
            constrain(6, 1.0, &[(0, 0), (5, 0)], "", true),
            constrain(6, 2.0, &[(0, 0), (6, 1)], "", false),
        ])
    );
    let xml = document(&[("Sketcher::SketchObject", "Profile", profile)]);
    let (doc, report) = import_with(
        fcstd(&xml, None, &[]),
        Vec::new(),
        MockKernel::default(),
        false,
    );
    let s = &report.sketches[0];
    assert_eq!(s.outcome, ObjectOutcome::Parametric, "{s:?}");
    assert!(s.dropped.is_empty(), "{s:?}");
    assert!(s.error < 1e-9, "{s:?}");
    let def = sketch_def(&doc, &report, "Profile");
    let dims: Vec<&str> = def["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        dims,
        [
            "concentric_distance",
            "concentric_distance",
            "tangent_distance",
            "concentric_distance",
            "concentric_distance",
            "concentric_distance",
        ],
        "{def}"
    );
    // The reference distance stays driven, the named one keeps its name.
    assert_eq!(def["dimensions"][5]["driven"], true, "{def}");
    assert_eq!(s.dimension_sources[0].name, "gap");
    // Each helper is a construction circle: one about the first circle
    // touching the second, one about the arc touching the circle from
    // inside, one through each point, one touching the line across.
    let entities = def["entities"].as_array().unwrap();
    let helpers = entities
        .iter()
        .filter(|e| e["type"] == "circle" && e["construction"] == true)
        .count();
    assert_eq!(helpers, 5, "{def}");
    let kinds: Vec<&str> = def["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "tangent").count(),
        3,
        "{def}"
    );
    assert_eq!(
        kinds.iter().filter(|k| **k == "coincident").count(),
        2,
        "{def}"
    );
    // It solves where FreeCAD left it.
    let uid = s.feature.as_deref().unwrap();
    let FeatureDef::Sketch(sketch) = &doc.feature(feature_uid(uid)).unwrap().def else {
        panic!("{uid} is not a sketch");
    };
    let params = doc.parameters();
    sketch
        .solve_with(
            &mut |id| {
                params
                    .value(id)
                    .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
            },
            &|id| params.name(id),
        )
        .unwrap_or_else(|e| panic!("{uid} does not solve: {}", e.message));

    // A distance the geometry does not hold either way is left out with
    // both readings.
    let profile = format!(
        "{}{}",
        geometry(&[round([0.0, 0.0], 5.0), round([20.0, 0.0], 3.0)]),
        constraints(&[constrain(6, 7.0, &[(0, 0), (1, 0)], "", true)])
    );
    let xml = document(&[("Sketcher::SketchObject", "Profile", profile)]);
    let (_, report) = import_with(
        fcstd(&xml, None, &[]),
        Vec::new(),
        MockKernel::default(),
        false,
    );
    let s = &report.sketches[0];
    assert_eq!(s.outcome, ObjectOutcome::Partial, "{s:?}");
    assert!(
        s.dropped[0].contains("12.000000 outside the circle"),
        "{:?}",
        s.dropped
    );
}

#[test]
fn a_revolution_over_a_full_turn_and_a_symmetric_pocket_up_to_a_face() {
    // A block; a revolution of 100° and 280° (FreeCAD turns the profile
    // back by 280° and revolves by 380°, which OCCT takes as 20°: from
    // 80° to 100°); a pocket of a slot at z = 5 up to a sketch's plane at
    // z = 8, symmetric (FreeCAD mirrors that side: 3 mm down too).
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &[
                        "Block",
                        "BlockPad",
                        "RingSketch",
                        "Ring",
                        "Slot",
                        "Level",
                        "Cut"
                    ]
                ),
                link("Tip", "Cut")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Block",
            rectangle_sketch(0.0, 0.0, 20.0, 10.0),
        ),
        (
            "PartDesign::Pad",
            "BlockPad",
            format!(
                "{}{}{}",
                link_sub("Profile", "Block", &[]),
                float("Length", 10.0),
                shape("BlockPad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "RingSketch",
            rectangle_sketch(12.0, 0.0, 2.0, 2.0),
        ),
        (
            "PartDesign::Revolution",
            "Ring",
            format!(
                "{}{}{}{}{}{}{}",
                link_sub("Profile", "RingSketch", &[]),
                link_sub("ReferenceAxis", "RingSketch", &["V_Axis"]),
                link("BaseFeature", "BlockPad"),
                prop("Angle", "App::PropertyAngle", "<Float value=\"100\"/>"),
                prop("Angle2", "App::PropertyAngle", "<Float value=\"280\"/>"),
                enumeration("Type", 4),
                shape("Ring.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Slot",
            format!(
                "{}{}",
                placement([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], 0.0),
                rectangle_sketch(5.0, 2.0, 4.0, 4.0)
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Level",
            format!(
                "{}{}",
                placement([0.0, 0.0, 8.0], [0.0, 0.0, 1.0], 0.0),
                geometry(&[segment([0.0, 0.0], [5.0, 0.0], false)])
            ),
        ),
        (
            "PartDesign::Pocket",
            "Cut",
            format!(
                "{}{}{}{}{}{}",
                link_sub("Profile", "Slot", &[]),
                link("BaseFeature", "Ring"),
                flag("Midplane", true),
                enumeration("Type", 3),
                link_sub("UpToFace", "Level", &[]),
                shape("Cut.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"14 body"),
            ("BlockPad.brp", b"6 block"),
            ("Ring.brp", b"10 ring"),
            ("Cut.brp", b"14 cut"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    for name in ["Ring", "Cut"] {
        assert_eq!(
            feature(&report, name).outcome,
            ObjectOutcome::Parametric,
            "{name}: {:?}",
            feature(&report, name)
        );
    }
    // The revolution from 80° to 100°: side two ends 80° past the profile.
    let def = &defs_of(&doc, &report, "Ring")[0];
    assert_eq!(def["extent"]["type"], "two_sides", "{def}");
    let angle = |key: &str| {
        let v = &def["extent"][key];
        v.as_f64().or_else(|| value_of(&doc, v)).unwrap()
    };
    assert!(
        (angle("angle1") - 100f64.to_radians()).abs() < 1e-12,
        "{def}"
    );
    assert!(
        (angle("angle2") + 80f64.to_radians()).abs() < 1e-12,
        "{def}"
    );
    // The symmetric pocket: up to the plane, and as far the other way.
    let def = &defs_of(&doc, &report, "Cut")[0];
    let extent = &def["extent"];
    assert_eq!(extent["type"], "two_sides", "{def}");
    assert_eq!(extent["side1"]["type"], "to_object", "{def}");
    assert_eq!(extent["side1"]["object"]["type"], "plane", "{def}");
    assert_eq!(extent["side2"]["type"], "distance", "{def}");
    let side2 = &extent["side2"]["distance"];
    assert!(
        (side2.as_f64().or_else(|| value_of(&doc, side2)).unwrap() - 3.0).abs() < 1e-9,
        "{def}"
    );
}

#[test]
fn an_extent_up_to_another_component_s_face_is_read_through_its_occurrence() {
    // A Body 50 mm up (a component of its own) pads its sketch down to the
    // top of another Body's pad (at the document's top): the face is read
    // in its component through the two occurrences.
    let xml = document(&[
        (
            "PartDesign::Body",
            "Base",
            format!(
                "{}{}{}",
                shape("Base.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad"]),
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
            "PartDesign::Body",
            "Top",
            format!(
                "{}{}{}{}",
                shape("Top.brp"),
                links("Group", "App::PropertyLinkList", &["Upper", "Reach"]),
                link("Tip", "Reach"),
                placement([0.0, 0.0, 50.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Upper",
            rectangle_sketch(5.0, 5.0, 10.0, 10.0),
        ),
        (
            "PartDesign::Pad",
            "Reach",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Upper", &[]),
                enumeration("Type", 3),
                link_sub("UpToFace", "Pad", &["Face2"]),
                shape("Reach.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Base.brp", b"6 base"),
            ("Pad.brp", b"6 pad"),
            ("Top.brp", b"6 top"),
            ("Reach.brp", b"6 reach"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let reach = feature(&report, "Reach");
    assert_eq!(reach.outcome, ObjectOutcome::Parametric, "{reach:?}");
    let def = &defs_of(&doc, &report, "Reach")[0];
    let object = &def["extent"]["object"];
    assert_eq!(object["type"], "face", "{def}");
    let body = object["body"].as_str().unwrap();
    // The pad's body, of the other component, linked.
    let link = &def["object_links"][body];
    assert!(link.is_object(), "{def}");
    assert!(link["target"].is_string(), "{def}");
    assert!(link.get("source").is_none(), "{def}");
}

#[test]
fn binders_of_another_body_s_sketches_are_profiles_and_hole_sketches() {
    // A Body at the document's top with a slot sketch and a sketch of a
    // circle; a Body 50 mm up (a component of its own) pockets through
    // a binder of the slot sketch and drills at a binder of the circle's
    // sketch (FreeCAD's binders of a whole sketch of another Body, its
    // path `Base` with `Slot.`). A sketch at the top is attached to an
    // origin's plane named as the origin's element (FreeCAD 1.x).
    let xml = document(&[
        (
            "PartDesign::Body",
            "Base",
            format!(
                "{}{}{}",
                shape("Base.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch", "Pad", "Slot", "Spots"]
                ),
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
            "Sketcher::SketchObject",
            "Slot",
            rectangle_sketch(5.0, 5.0, 10.0, 10.0),
        ),
        (
            "Sketcher::SketchObject",
            "Spots",
            geometry(&[round([30.0, 20.0], 2.0)]),
        ),
        (
            "PartDesign::Body",
            "Top",
            format!(
                "{}{}{}{}",
                shape("Top.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Block", "TopPad", "SlotBinder", "Cut", "SpotBinder", "Bore"]
                ),
                link("Tip", "Bore"),
                placement([0.0, 0.0, 50.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Block",
            rectangle_sketch(0.0, 0.0, 40.0, 30.0),
        ),
        (
            "PartDesign::Pad",
            "TopPad",
            format!(
                "{}{}{}",
                link_sub("Profile", "Block", &[]),
                float("Length", 5.0),
                shape("TopPad.brp")
            ),
        ),
        (
            "PartDesign::SubShapeBinder",
            "SlotBinder",
            format!(
                "{}{}{}",
                xlink_sub("Context", "Top", "SlotBinder."),
                xlink_list("Support", &[("Base", "Slot.")]),
                shape("SlotBinder.brp")
            ),
        ),
        (
            "PartDesign::Pocket",
            "Cut",
            format!(
                "{}{}{}{}",
                link_sub("Profile", "SlotBinder", &[""]),
                link("BaseFeature", "TopPad"),
                enumeration("Type", 1),
                shape("Cut.brp")
            ),
        ),
        (
            "PartDesign::SubShapeBinder",
            "SpotBinder",
            format!(
                "{}{}{}",
                xlink_sub("Context", "Top", "SpotBinder."),
                xlink_list("Support", &[("Base", "Spots.")]),
                shape("SpotBinder.brp")
            ),
        ),
        (
            "PartDesign::Hole",
            "Bore",
            format!(
                "{}{}{}{}{}{}{}",
                link_sub("Profile", "SpotBinder", &[""]),
                link("BaseFeature", "Cut"),
                float("Diameter", 3.0),
                enumeration("DepthType", 1),
                enumeration("DrillPoint", 1),
                enumeration("HoleCutType", 0),
                shape("Bore.brp")
            ),
        ),
        (
            "App::Origin",
            "Origin",
            links("OriginFeatures", "App::PropertyLinkList", &["XY_Plane"]),
        ),
        (
            "App::Plane",
            "XY_Plane",
            prop(
                "Role",
                "App::PropertyString",
                "<String value=\"XY_Plane\"/>",
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Attached",
            format!(
                "{}{}{}",
                prop(
                    "AttachmentSupport",
                    "App::PropertyLinkSubList",
                    "<LinkSubList count=\"1\"><Link obj=\"Origin\" sub=\"XY_Plane\"/></LinkSubList>"
                ),
                enumeration("MapMode", 5),
                geometry(&[segment([0.0, 0.0], [5.0, 0.0], false)])
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Base.brp", b"6 base"),
            ("Pad.brp", b"6 pad"),
            ("Top.brp", b"14 top"),
            ("TopPad.brp", b"6 toppad"),
            ("SlotBinder.brp", b"1 slot,sheet"),
            ("Cut.brp", b"10 cut"),
            ("SpotBinder.brp", b"1 spot,sheet"),
            ("Bore.brp", b"14 bore"),
        ],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    for name in ["Cut", "Bore"] {
        assert_eq!(
            feature(&report, name).outcome,
            ObjectOutcome::Parametric,
            "{name}: {:?}",
            feature(&report, name)
        );
    }
    let slot = report.sketches.iter().find(|s| s.object == "Slot").unwrap();
    let slot_uid = slot.feature.clone().unwrap();
    // The pocket's profile: a sketch of the binder's in the upper Body's
    // component, each of the slot's lines a projection of it seen through
    // the two occurrences.
    let cut = &defs_of(&doc, &report, "Cut")[0];
    let sketch = cut["profiles"][0]["sketch"].as_str().unwrap().to_owned();
    assert_ne!(sketch, slot_uid, "{cut}");
    let binder_sketch = def_json(&doc, &sketch);
    let projections = binder_sketch["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 4, "{binder_sketch}");
    for p in projections {
        assert_eq!(p["source"]["sketch"], slot_uid, "{binder_sketch}");
        assert!(p["source"]["curve"].is_string(), "{binder_sketch}");
        assert!(p["link"]["target"].is_string(), "{binder_sketch}");
    }
    let sketch_uid = feature_uid(&sketch);
    assert!(
        doc.status(sketch_uid).and_then(|s| s.error()).is_none(),
        "{:?}",
        doc.status(sketch_uid)
    );
    // Hidden: the pocket uses it.
    assert_eq!(doc.feature_shown(sketch_uid), Some(false));
    // The hole at the projection of the circle.
    let bore = &defs_of(&doc, &report, "Bore")[0];
    let placement = &bore["placement"];
    assert_ne!(placement["sketch"], json!(slot_uid), "{bore}");
    assert_eq!(placement["points"].as_array().unwrap().len(), 1, "{bore}");
    // On the origin plane, not only parallel to it.
    let attached = report
        .sketches
        .iter()
        .find(|s| s.object == "Attached")
        .unwrap();
    assert_eq!(attached.plane, "on xy", "{attached:?}");
    assert!(
        !attached
            .notes
            .iter()
            .any(|n| n.contains("its support is not imported")),
        "{attached:?}"
    );
}

#[test]
fn external_geometry_whose_link_is_missing_is_fixed_from_its_copy() {
    // FreeCAD keeps a copy of external geometry whose source is gone (no
    // link in `ExternalGeometry`): fixed in place from the copy, and the
    // constraint on it kept.
    let profile = format!(
        "{}{}{}",
        external_copy(&[segment([0.0, 0.0], [40.0, 0.0], true)]),
        geometry(&[segment([10.0, 0.0], [10.0, 20.0], false)]),
        constraints(&[constrain(13, 0.0, &[(0, 1), (-3, 0)], "", true)])
    );
    let xml = document(&[("Sketcher::SketchObject", "Profile", profile)]);
    let (doc, report) = import_with(
        fcstd(&xml, None, &[]),
        Vec::new(),
        MockKernel::default(),
        false,
    );
    let s = &report.sketches[0];
    assert_eq!(s.outcome, ObjectOutcome::Partial, "{s:?}");
    assert_eq!(
        s.dropped,
        ["external geometry 0: fixed in place (its link is missing in the file)"],
        "{s:?}"
    );
    let def = sketch_def(&doc, &report, "Profile");
    let kinds: Vec<&str> = def["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["coincident"], "{def}");
}

// Binders of another Body's sketch (mitcad#963).

#[test]
fn a_pad_on_a_binder_of_another_body_s_sketch_pads_a_linked_projecting_sketch() {
    // Two Bodies, each shown by a link (so each has a component of its
    // own). The Slider's pad profiles a binder of the Frame's outline (a
    // 40 × 30 rectangle whose bottom line's DistanceX is the named
    // constraint Width), a sketch is attached to the binder's face, and a
    // hole drills a binder of the Frame's sketch of one circle.
    let corners = [[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]];
    let outline = format!(
        "{}{}",
        geometry(&[
            segment(corners[0], corners[1], false),
            segment(corners[1], corners[2], false),
            segment(corners[2], corners[3], false),
            segment(corners[3], corners[0], false),
        ]),
        constraints(&[
            constrain(1, 0.0, &[(0, 2), (1, 1)], "", true),
            constrain(1, 0.0, &[(1, 2), (2, 1)], "", true),
            constrain(1, 0.0, &[(2, 2), (3, 1)], "", true),
            constrain(1, 0.0, &[(3, 2), (0, 1)], "", true),
            constrain(7, 40.0, &[(0, 0)], "Width", true),
        ])
    );
    let binder = |body: &str, name: &str, bound: &str| {
        let path = format!("{bound}.");
        format!(
            "{}{}{}",
            xlink_sub("Context", body, &format!("{name}.")),
            xlink_list("Support", &[("Frame", path.as_str())]),
            shape(&format!("{name}.brp"))
        )
    };
    let mark = format!(
        "{}{}{}",
        geometry(&[segment([105.0, -95.0], [115.0, -95.0], false)]),
        enumeration("MapMode", 5),
        prop(
            "AttachmentSupport",
            "App::PropertyLinkSubList",
            "<LinkSubList count=\"1\"><Link obj=\"Bound\" sub=\"Face1\"/></LinkSubList>",
        )
    );
    let objects = [
        (
            "PartDesign::Body",
            "Frame".to_owned(),
            format!(
                "{}{}{}",
                shape("Frame.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Outline", "FramePad", "Holes"]
                ),
                link("Tip", "FramePad")
            ),
        ),
        ("Sketcher::SketchObject", "Outline".to_owned(), outline),
        (
            "PartDesign::Pad",
            "FramePad".to_owned(),
            format!(
                "{}{}{}",
                link_sub("Profile", "Outline", &[]),
                float("Length", 10.0),
                shape("FramePad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Holes".to_owned(),
            geometry(&[round([10.0, 10.0], 3.0)]),
        ),
        (
            "PartDesign::Body",
            "Slider".to_owned(),
            format!(
                "{}{}{}",
                shape("Slider.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Bound", "SliderPad", "Mark", "Drill", "Hole"]
                ),
                link("Tip", "Hole")
            ),
        ),
        (
            "PartDesign::SubShapeBinder",
            "Bound".to_owned(),
            binder("Slider", "Bound", "Outline"),
        ),
        (
            "PartDesign::Pad",
            "SliderPad".to_owned(),
            format!(
                "{}{}{}{}",
                link_sub("Profile", "Bound", &[""]),
                float("Length", 5.0),
                enumeration("Type", 0),
                shape("SliderPad.brp")
            ),
        ),
        ("Sketcher::SketchObject", "Mark".to_owned(), mark),
        (
            "PartDesign::SubShapeBinder",
            "Drill".to_owned(),
            binder("Slider", "Drill", "Holes"),
        ),
        (
            "PartDesign::Hole",
            "Hole".to_owned(),
            format!(
                "{}{}{}{}{}{}{}",
                link_sub("Profile", "Drill", &[""]),
                link("BaseFeature", "SliderPad"),
                float("Diameter", 6.0),
                enumeration("DepthType", 1),
                enumeration("DrillPoint", 1),
                enumeration("HoleCutType", 0),
                shape("Hole.brp")
            ),
        ),
        (
            "App::Link",
            "FrameLink".to_owned(),
            format!(
                "{}{}",
                xlink("", "Frame"),
                placement([100.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
        (
            "App::Link",
            "SliderLink".to_owned(),
            format!(
                "{}{}",
                xlink("", "Slider"),
                placement([0.0, 100.0, 0.0], [0.0, 0.0, 1.0], 0.0)
            ),
        ),
    ];
    let objects: Vec<(&str, &str, String)> = objects
        .iter()
        .map(|(t, n, p)| (*t, n.as_str(), p.clone()))
        .collect();
    let xml = document(&objects);
    let file = fcstd(
        &xml,
        None,
        &[
            ("Frame.brp", b"6 frame"),
            ("FramePad.brp", b"6 frame pad"),
            ("Slider.brp", b"7 slider"),
            ("Bound.brp", b"1 bound"),
            ("SliderPad.brp", b"6 slider pad"),
            ("Drill.brp", b"1 drill"),
            ("Hole.brp", b"7 hole"),
        ],
    );
    let (mut doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    assert_eq!(item(&report, "Frame").component.as_deref(), Some("Frame"));
    assert_eq!(item(&report, "Slider").component.as_deref(), Some("Slider"));
    let outline_uid = report
        .sketches
        .iter()
        .find(|s| s.object == "Outline")
        .and_then(|s| s.feature.clone())
        .unwrap();

    // The pad profiles a sketch of the binder's in the Slider's component.
    let pad = feature(&report, "SliderPad");
    assert_eq!(pad.outcome, ObjectOutcome::Parametric, "{pad:?}");
    let pad_def = &defs_of(&doc, &report, "SliderPad")[0];
    let sketch_uid = pad_def["profiles"][0]["sketch"]
        .as_str()
        .unwrap_or_else(|| panic!("{pad_def}"))
        .to_owned();
    assert_ne!(sketch_uid, outline_uid, "{pad_def}");
    let sketch = feature_uid(&sketch_uid);
    let slider_component = doc.feature(sketch).unwrap().component;
    assert_eq!(doc.assembly().name(slider_component), "Slider");
    // On the Frame's XY plane itself (not the Slider's plane parallel to
    // it), seen through the two occurrences.
    let def = def_json(&doc, &sketch_uid);
    assert_eq!(def["plane"], "xy", "{def}");
    let plane_link = def["plane_link"].clone();
    let (source, target) = (
        plane_link["source"].as_str().unwrap_or_default(),
        plane_link["target"].as_str().unwrap_or_default(),
    );
    assert!(
        !source.is_empty() && !target.is_empty() && source != target,
        "{def}"
    );
    // Each of the outline's four lines a linked projection, not
    // construction.
    let projections = def["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 4, "{def}");
    for p in projections {
        assert_eq!(p["source"]["sketch"], outline_uid, "{def}");
        assert!(p["source"]["curve"].is_string(), "{def}");
        assert_eq!(p["link"], plane_link, "{def}");
    }
    let lines: Vec<&serde_json::Value> = def["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line")
        .collect();
    assert_eq!(lines.len(), 4, "{def}");
    assert!(lines.iter().all(|e| e["construction"] != true), "{def}");

    // The sketch on the binder's face lies on the same plane.
    let s = report.sketches.iter().find(|s| s.object == "Mark").unwrap();
    assert_eq!(s.plane, "on Outline's plane", "{s:?}");
    assert!(
        s.notes.iter().any(|n| n.contains("which Bound binds")),
        "{s:?}"
    );
    let mark = sketch_def(&doc, &report, "Mark");
    assert_eq!(mark["plane"], "xy", "{mark}");
    assert_eq!(mark["plane_link"], plane_link, "{mark}");

    // The hole drills the projected circle's centre of the sketch made for
    // the other binder, which also lies on the Frame's XY plane.
    let hole = feature(&report, "Hole");
    assert_eq!(hole.outcome, ObjectOutcome::Parametric, "{hole:?}");
    let hole_def = &defs_of(&doc, &report, "Hole")[0];
    let drill_uid = hole_def["placement"]["sketch"]
        .as_str()
        .unwrap_or_else(|| panic!("{hole_def}"))
        .to_owned();
    assert_ne!(drill_uid, sketch_uid, "{hole_def}");
    let drilled = hole_def["placement"]["points"].as_array().unwrap();
    assert_eq!(drilled.len(), 1, "{hole_def}");
    let drill_def = def_json(&doc, &drill_uid);
    assert_eq!(drill_def["plane"], "xy", "{drill_def}");
    assert_eq!(drill_def["plane_link"], plane_link, "{drill_def}");
    assert!(
        drill_def["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == drilled[0] && e["type"] == "circle"),
        "{drill_def}"
    );

    // The outline 50 mm wide: the binder's sketch follows it, and the pad
    // with it.
    let width = |doc: &Document<MockKernel>| -> f64 {
        let output = doc.sketch_output(sketch).unwrap();
        let xs: Vec<f64> = output
            .solved
            .curves
            .values()
            .flat_map(|c| match c {
                mitcad_model::sketch::geometry::Curve2::Line { a, b } => vec![a[0], b[0]],
                _ => Vec::new(),
            })
            .collect();
        xs.iter().copied().fold(f64::MIN, f64::max) - xs.iter().copied().fold(f64::MAX, f64::min)
    };
    assert!((width(&doc) - 40.0).abs() < 1e-6, "{}", width(&doc));
    doc.set_parameter_expression("Width", "50 mm", None)
        .unwrap();
    for uid in [sketch, feature_uid(&pad.features[0])] {
        assert!(
            doc.status(uid).and_then(|st| st.error()).is_none(),
            "{:?}",
            doc.status(uid)
        );
    }
    assert!((width(&doc) - 50.0).abs() < 1e-6, "{}", width(&doc));
}

// A reported assembly's last two fallbacks (mitcad#964).

/// An enumeration with the file's own list of values.
fn custom_enum(name: &str, index: usize, values: &[&str]) -> String {
    let items: String = values
        .iter()
        .map(|v| format!("<Enum value=\"{v}\"/>"))
        .collect();
    prop(
        name,
        "App::PropertyEnumeration",
        &format!(
            "<Integer value=\"{index}\" CustomEnum=\"true\"/><CustomEnumList count=\"{}\">{items}</CustomEnumList>",
            values.len()
        ),
    )
}

#[test]
fn a_modelled_thread_is_cut_at_every_centre_the_hole_drills() {
    use mitcad_model::Curve3;
    use std::f64::consts::TAU;
    // A FreeCAD 1.1 hole with a modelled M3 thread drills its sketch's own
    // circle and the circle of its defining external geometry (another
    // sketch's circle, part of the sketch's shape): the thread's groove
    // is cut about both centres, as FreeCAD threads both holes (before,
    // only about the sketch's own circle).
    let holes = format!(
        "{}{}{}{}",
        placement([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 0.0),
        external_links(&[("Marks", "Edge1")]),
        external_copy(&[round([40.0, 15.0], 1.5).replace(
            "</Geometry>",
            "<GeoExtensions count=\"1\"><GeoExtension type=\"Sketcher::ExternalGeometryExtension\" \
             Ref=\"Marks.Edge1\" Flags=\"1\"/></GeoExtensions></Geometry>",
        )]),
        geometry(&[round([10.0, 15.0], 1.5)])
    );
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links(
                    "Group",
                    "App::PropertyLinkList",
                    &["Sketch", "Pad", "Marks", "Holes", "Hole"]
                ),
                link("Tip", "Hole")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Sketch",
            rectangle_sketch(0.0, 0.0, 50.0, 30.0),
        ),
        (
            "PartDesign::Pad",
            "Pad",
            format!(
                "{}{}{}",
                link_sub("Profile", "Sketch", &[""]),
                float("Length", 10.0),
                shape("Pad.brp")
            ),
        ),
        (
            "Sketcher::SketchObject",
            "Marks",
            format!(
                "{}{}{}",
                placement([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], 0.0),
                geometry(&[round([40.0, 15.0], 1.5)]),
                shape("Marks.brp")
            ),
        ),
        ("Sketcher::SketchObject", "Holes", holes),
        (
            "PartDesign::Hole",
            "Hole",
            format!(
                "{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
                link_sub("Profile", "Holes", &[""]),
                link("BaseFeature", "Pad"),
                float("Diameter", 2.5),
                float("Depth", 5.0),
                float("ThreadDepth", 5.0),
                enumeration("DepthType", 0),
                enumeration("DrillPoint", 0),
                enumeration("HoleCutType", 0),
                flag("Threaded", true),
                flag("ModelThread", true),
                custom_enum("ThreadType", 1, &["None", "ISOMetricProfile"]),
                custom_enum("ThreadSize", 1, &["M2.5", "M3x0.5", "M4"]),
                custom_enum("ThreadClass", 0, &["6H"]),
                shape("Hole.brp")
            ),
        ),
    ])
    .replace("1.0R39319", "1.1R1");
    let file = fcstd(
        &xml,
        None,
        &[
            ("Body.brp", b"9 body"),
            ("Pad.brp", b"6 pad"),
            ("Marks.brp", b"1 marks"),
            ("Hole.brp", b"9 hole"),
        ],
    );
    let kernel = MockKernel::default();
    kernel.named_curves.borrow_mut().insert(
        stored_edge_name(b"1 marks", 0),
        Curve3::Conic {
            center: [40.0, 15.0, 10.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 1.5,
            minor: 1.5,
            start: 0.0,
            end: TAU,
            closed: true,
        },
    );
    let (doc, report) = import_with(file, Vec::new(), kernel, false);
    let hole = feature(&report, "Hole");
    assert_eq!(hole.outcome, ObjectOutcome::Parametric, "{hole:?}");
    let defs = defs_of(&doc, &report, "Hole");
    // The hole drills both circles.
    let drill = defs.iter().find(|d| d["type"] == "hole").unwrap();
    assert_eq!(
        drill["placement"]["points"].as_array().unwrap().len(),
        2,
        "{drill}"
    );
    // A groove about each of the two centres.
    let mut centres: Vec<[f64; 2]> = defs
        .iter()
        .filter(|d| d["type"] == "helix")
        .map(|d| {
            let o = &d["axis"]["origin"];
            [o[0].as_f64().unwrap(), o[1].as_f64().unwrap()]
        })
        .collect();
    centres.sort_by(|a, b| a[0].total_cmp(&b[0]));
    assert_eq!(centres.len(), 2, "{defs:?}");
    for (got, want) in centres.iter().zip([[10.0, 15.0], [40.0, 15.0]]) {
        assert!(
            (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6,
            "{centres:?}"
        );
    }
}

#[test]
fn a_reversed_pad_along_its_sketch_s_normal_axis_pads_the_other_way() {
    // FreeCAD keeps a Pad's Direction along its reference axis (here the
    // sketch's normal) before Reversed turns it: Reversed with Direction
    // along the normal pads against the normal, and that is the first
    // reading (before, the pad went along the normal first).
    let xml = document(&[
        (
            "PartDesign::Body",
            "Body",
            format!(
                "{}{}{}",
                shape("Body.brp"),
                links("Group", "App::PropertyLinkList", &["Sketch", "Pad"]),
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
                "{}{}{}{}{}{}",
                link_sub("Profile", "Sketch", &[""]),
                link_sub("ReferenceAxis", "Sketch", &["N_Axis"]),
                vector("Direction", [0.0, 0.0, 1.0]),
                flag("Reversed", true),
                float("Length", 2.1),
                shape("Pad.brp")
            ),
        ),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[("Body.brp", b"6 body"), ("Pad.brp", b"6 pad")],
    );
    let (doc, report) = import_with(file, Vec::new(), MockKernel::default(), false);
    let pad = feature(&report, "Pad");
    assert_eq!(pad.outcome, ObjectOutcome::Parametric, "{pad:?}");
    assert!(!pad.notes.iter().any(|n| n.contains("reading")), "{pad:?}");
    let def = &defs_of(&doc, &report, "Pad")[0];
    assert_eq!(def["flip"], true, "{def}");
}

#[test]
fn a_group_listing_an_object_twice_places_it_once() {
    let group = |targets: &[&str]| links("Group", "App::PropertyLinkList", targets);
    let xml = document(&[
        (
            "App::DocumentObjectGroup",
            "Folder",
            group(&["Part", "Box", "Part", "Box"]),
        ),
        ("App::Part", "Part", group(&["Cylinder", "Cylinder"])),
        ("Part::Cylinder", "Cylinder", shape("Cylinder.brp")),
        ("Part::Box", "Box", shape("Box.brp")),
    ]);
    let file = fcstd(
        &xml,
        None,
        &[("Cylinder.brp", b"3 cylinder"), ("Box.brp", b"6 box")],
    );
    let (doc, report) = import(file, Vec::new());
    // One component with one occurrence (the importer's
    // `occurrences_of[Part]` has one), one instance of each body.
    assert_eq!(report.components, 1);
    assert_eq!(report.occurrences, 1);
    assert_eq!(centres(&doc, "Cylinder").len(), 1);
    assert_eq!(centres(&doc, "Box").len(), 1);
}

#[test]
fn a_chain_of_groups_listing_the_next_many_times_is_walked_once() {
    // Twenty folders, each listing the next ten times, a body in the last:
    // walked as listed, 10^20 placements.
    let count = 20;
    let mut objects: Vec<(&str, String, String)> = (0..count)
        .map(|i| {
            let next = if i + 1 < count {
                format!("Folder{}", i + 1)
            } else {
                "Box".to_owned()
            };
            let targets = [next.as_str(); 10];
            (
                "App::DocumentObjectGroup",
                format!("Folder{i}"),
                links("Group", "App::PropertyLinkList", &targets),
            )
        })
        .collect();
    objects.push(("Part::Box", "Box".to_owned(), shape("Box.brp")));
    let list: Vec<(&str, &str, String)> = objects
        .iter()
        .map(|(t, n, p)| (*t, n.as_str(), p.clone()))
        .collect();
    let file = fcstd(&document(&list), None, &[("Box.brp", b"6 box")]);
    let (doc, report) = import(file, Vec::new());
    assert_eq!(centres(&doc, "Box").len(), 1);
    assert_eq!(report.occurrences, 0);
    assert_eq!(item(&report, "Folder19").outcome, ObjectOutcome::Included);
}
