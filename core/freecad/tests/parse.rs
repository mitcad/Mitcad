// SPDX-License-Identifier: MIT
//! The reader on small hand-made documents (tests/data), packed into
//! archives in memory: objects, properties of every kind, states, links,
//! placements, lists kept in files, view data and enumerations.

use mitcad_freecad::{Color, Document, FcstdError, FcstdFile, GuiDocument, Value, Version};
use mitcad_zip::{Method, Writer};

const DOCUMENT_0_21: &str = include_str!("data/document_0_21.xml");
const DOCUMENT_1_0: &str = include_str!("data/document_1_0.xml");
const GUI: &str = include_str!("data/gui_document.xml");

fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = Writer::new();
    for (name, data) in files {
        writer.add(name, data, Method::Deflate).unwrap();
    }
    writer.finish().unwrap()
}

fn placements(list: &[([f64; 3], [f64; 4])]) -> Vec<u8> {
    let mut data = (list.len() as u32).to_le_bytes().to_vec();
    for (p, q) in list {
        for v in p.iter().chain(q) {
            data.extend(v.to_le_bytes());
        }
    }
    data
}

#[test]
fn reads_objects_states_and_document_properties() {
    let doc = Document::parse(DOCUMENT_0_21).unwrap();
    assert_eq!(doc.schema_version, 4);
    assert_eq!(doc.version(), Version::parse("0.21R33771"));
    assert_eq!(doc.label(), Some("sample & test"));
    assert!(!doc.string_hasher);
    let names: Vec<&str> = doc.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(
        names,
        ["Box", "Body", "Pad", "Part", "Cylinder", "Link", "Array"]
    );
    assert_eq!(doc.position("Pad"), Some(2));

    let pad = doc.object("Pad").unwrap();
    assert_eq!(pad.type_name, "PartDesign::Pad");
    assert_eq!(pad.id, Some(14));
    assert!(pad.state.touched && pad.state.is_stale());
    let cylinder = doc.object("Cylinder").unwrap();
    assert!(cylinder.state.invalid);
    assert_eq!(
        cylinder.state.error.as_deref(),
        Some("Cylinder: radius is zero")
    );
    // An empty shape file name is no shape.
    assert_eq!(cylinder.shape_file(), None);
    assert!(!doc.object("Box").unwrap().state.is_stale());

    let body = doc.object("Body").unwrap();
    assert_eq!(body.dependencies, ["Pad"]);
    assert!(body.has_extension("App::OriginGroupExtension"));
    assert_eq!(body.links("Group")[0].object, "Pad");
    assert_eq!(body.link("Tip").unwrap().object, "Pad");
    assert!(body.link("BaseFeature").is_none());
    assert_eq!(body.visibility(), Some(false));
    let transient = body.property("_GroupTouched").unwrap();
    assert!(transient.transient && transient.value == Value::None);
    match &doc.property("Material").unwrap().value {
        Value::Map(items) => assert_eq!(items, &[("Density".to_owned(), "7.85".to_owned())]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn reads_values_of_every_kind() {
    let doc = Document::parse(DOCUMENT_0_21).unwrap();
    let r#box = doc.object("Box").unwrap();
    assert_eq!(r#box.label(), "Red box");
    assert_eq!(r#box.f64("Height"), Some(30.0));
    assert_eq!(r#box.shape_file(), Some("PartShape.brp"));
    let p = r#box.placement().unwrap();
    assert_eq!(p.position, [1.0, 2.0, 3.0]);
    let moved = p.apply([1.0, 0.0, 0.0]);
    assert!((moved[0] - 1.0).abs() < 1e-12 && (moved[1] - 3.0).abs() < 1e-12);
    // Enumerations: a custom list in the file, others from the tables.
    assert_eq!(
        doc.enum_text(r#box, "AttacherEngine").as_deref(),
        Some("Engine Plane")
    );
    assert_eq!(doc.enum_text(r#box, "MapMode").as_deref(), Some("FlatFace"));
    let pad = doc.object("Pad").unwrap();
    assert_eq!(doc.enum_text(pad, "Type").as_deref(), Some("UpToLast"));
    match pad.value("ExpressionEngine").unwrap() {
        Value::Expressions(e) => {
            assert_eq!(e[0].path, "Length");
            assert_eq!(e[0].expression, "Spreadsheet.depth * 2");
        }
        other => panic!("{other:?}"),
    }
    let profile = pad.link("Profile").unwrap();
    assert_eq!(
        (profile.object.as_str(), profile.subs[0].name.as_str()),
        ("Sketch", "Face1")
    );

    // A sub-list groups consecutive entries of one object.
    let support = doc.object("Cylinder").unwrap().links("Support");
    assert_eq!(support.len(), 2);
    assert_eq!(support[0].object, "Box");
    let subs: Vec<&str> = support[0].subs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(subs, ["Face1", "Face3"]);
    assert_eq!(support[1].subs[0].name, "Edge2");

    let link = doc.object("Link").unwrap();
    let target = link.link("LinkedObject").unwrap();
    assert_eq!(
        (target.object.as_str(), target.file.as_deref()),
        ("Body", None)
    );
    assert_eq!(link.bool("LinkTransform"), Some(false));
    assert_eq!(
        link.value("ScaleVector"),
        Some(&Value::Vector([1.0, 1.0, 2.0]))
    );
    let references: Vec<(&str, &str)> = link
        .references()
        .map(|(p, l)| (p, l.object.as_str()))
        .collect();
    assert_eq!(references, [("LinkedObject", "Body")]);

    let array = doc.object("Array").unwrap();
    let other = array.link("LinkedObject").unwrap();
    assert_eq!(other.file.as_deref(), Some("other part.FCStd"));
    // The first element is the last character.
    assert_eq!(
        array.value("VisibilityList"),
        Some(&Value::BoolList(vec![true, true, false]))
    );
    assert_eq!(
        array.value("PlacementList"),
        Some(&Value::File {
            element: "PlacementList".to_owned(),
            file: "PlacementList".to_owned()
        })
    );
}

#[test]
fn reads_the_forms_of_version_1_0() {
    let doc = Document::parse(DOCUMENT_1_0).unwrap();
    assert!(doc.string_hasher);
    assert!(doc.version().at_least(1, 0));
    match &doc.property("UnitSystem").unwrap().value {
        Value::Enumeration(e) => assert_eq!(e.index, 2),
        other => panic!("{other:?}"),
    }
    let r#box = doc.object("Box").unwrap();
    assert!(r#box.state.frozen && !r#box.state.is_stale());
    let shape = r#box.shape().unwrap();
    assert_eq!(shape.file.as_deref(), Some("Box.Shape.brp"));
    assert_eq!(shape.element_map.as_deref(), Some("0.4"));
    let comment = r#box.property("Comment").unwrap();
    assert_eq!(comment.group.as_deref(), Some("Notes"));
    assert_eq!(comment.doc.as_deref(), Some("Added by hand"));

    let fillet = doc.object("Fillet").unwrap();
    let base = fillet.link("Base").unwrap();
    assert_eq!(base.subs.len(), 2);
    assert_eq!(base.subs[0].name, "Edge3");
    assert!(base.subs[0].mapped.as_deref().unwrap().contains("XTR"));
    assert_eq!(base.subs[1].mapped, None);
    assert_eq!(
        fillet.value("Values"),
        Some(&Value::FloatList(vec![1.5, 2.5]))
    );
    assert_eq!(
        fillet.value("Names"),
        Some(&Value::StringList(vec!["a".to_owned(), "b".to_owned()]))
    );
    assert_eq!(
        fillet.value("Indices"),
        Some(&Value::IntegerList(vec![4, -1]))
    );
    // What is not interpreted is kept.
    match fillet.value("Geometry").unwrap() {
        Value::Xml(element) => {
            assert_eq!(element.name, "GeometryList");
            let geometry = element.elements("Geometry").next().unwrap();
            assert_eq!(geometry.attribute("type"), Some("Part::GeomLineSegment"));
            assert_eq!(geometry.children[0].attribute("EndX"), Some("1"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn opens_archives_with_shapes_lists_and_view_data() {
    let mut appearance = 1u32.to_le_bytes().to_vec();
    for packed in [0x5555_5500u32, 0x00cc_0000, 0x8888_8800, 0] {
        appearance.extend(packed.to_le_bytes());
    }
    appearance.extend(0.9f32.to_le_bytes());
    appearance.extend(0.0f32.to_le_bytes());
    let mut diffuse = 2u32.to_le_bytes().to_vec();
    diffuse.extend(0xff00_0000u32.to_le_bytes());
    diffuse.extend(0x0000_ff00u32.to_le_bytes());
    let elements = placements(&[
        ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        ([10.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        ([20.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]),
    ]);
    let zip = archive(&[
        ("Document.xml", DOCUMENT_0_21.as_bytes()),
        ("GuiDocument.xml", GUI.as_bytes()),
        (
            "PartShape.brp",
            b"CASCADE Topology V1, (c) Matra-Datavision\n",
        ),
        ("PartShape1.brp", b""),
        ("PlacementList", &elements),
        ("ScaleList", &0u32.to_le_bytes()),
        ("DiffuseColor", &diffuse),
        ("ShapeAppearance", &appearance),
    ]);
    let file = FcstdFile::from_bytes(zip).unwrap();
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    let doc = &file.document;
    match doc.object("Array").unwrap().value("PlacementList").unwrap() {
        Value::PlacementList(list) => {
            assert_eq!(list.len(), 3);
            assert_eq!(list[2].position, [20.0, 0.0, 0.0]);
            assert_eq!(list[2].rotation, [0.0, 0.0, 1.0, 0.0]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        doc.object("Array").unwrap().value("ScaleList"),
        Some(&Value::VectorList(Vec::new()))
    );
    assert!(
        file.shape_data("Box")
            .unwrap()
            .unwrap()
            .starts_with(b"CASCADE")
    );
    // An empty shape file, and no shape at all.
    assert_eq!(file.shape_data("Body").unwrap(), None);
    assert_eq!(file.shape_data("Pad").unwrap(), None);
    assert!(file.has_entry("ScaleList"));

    let r#box = doc.object("Box").unwrap();
    let body = doc.object("Body").unwrap();
    assert!(file.visible(r#box));
    assert!(!file.visible(body));
    // No view: the object's own visibility, else shown.
    assert!(file.visible(doc.object("Pad").unwrap()));
    let view = file.view("Box").unwrap();
    assert_eq!(view.shape_color().unwrap().rgb8(), [255, 0, 0]);
    let faces: Vec<[u8; 3]> = view.face_colors().iter().map(Color::rgb8).collect();
    assert_eq!(faces, [[255, 0, 0], [0, 0, 255]]);
    assert_eq!(
        file.view("Body").unwrap().shape_color().unwrap().rgb8(),
        [0, 204, 0]
    );
    match file.view("Part").unwrap().value("LineMaterial").unwrap() {
        Value::Material(m) => assert_eq!(m.diffuse.rgb8(), [25, 25, 25]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn reports_missing_list_files_and_bad_view_data() {
    let zip = archive(&[
        ("Document.xml", DOCUMENT_0_21.as_bytes()),
        ("GuiDocument.xml", b"<Document><ViewProviderData>"),
    ]);
    let file = FcstdFile::from_bytes(zip).unwrap();
    assert!(file.gui.is_none());
    assert!(file.warnings.iter().any(|w| w.contains("GuiDocument.xml")));
    assert!(
        file.warnings
            .iter()
            .any(|w| w.contains("Array: PlacementList"))
    );
}

#[test]
fn refuses_what_is_not_a_document() {
    assert!(matches!(
        FcstdFile::from_bytes(archive(&[("a.txt", b"x")])),
        Err(FcstdError::NoDocument)
    ));
    assert!(matches!(
        FcstdFile::from_bytes(b"not a zip".to_vec()),
        Err(FcstdError::Zip(_))
    ));
    assert!(matches!(
        FcstdFile::from_bytes(archive(&[("Document.xml", b"<Document")])),
        Err(FcstdError::Xml { .. })
    ));
    // Entities are not expanded: a document type declaration is refused.
    let dtd = r#"<?xml version="1.0"?>
<!DOCTYPE Document [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;">]>
<Document SchemaVersion="4"><Properties/></Document>"#;
    assert!(Document::parse(dtd).is_err());
    assert!(Document::parse("<Other/>").is_err());
}

#[test]
fn values_that_do_not_read_are_kept_with_a_warning() {
    let xml = r#"<Document SchemaVersion="4" ProgramVersion="1.0R1">
      <Objects><Object type="Part::Box" name="Box"/></Objects>
      <ObjectData><Object name="Box"><Properties>
        <Property name="Length" type="App::PropertyLength"><Float value="ten"/></Property>
        <Property name="ShapeMaterial" type="Materials::PropertyMaterial"><PropertyMaterial uuid="1"/></Property>
      </Properties></Object></ObjectData></Document>"#;
    let doc = Document::parse(xml).unwrap();
    assert_eq!(doc.warnings.len(), 1, "{:?}", doc.warnings);
    assert!(doc.warnings[0].starts_with("object Box: property Length"));
    let r#box = doc.object("Box").unwrap();
    assert!(matches!(r#box.value("Length"), Some(Value::Xml(_))));
    assert!(matches!(r#box.value("ShapeMaterial"), Some(Value::Xml(_))));
}

#[test]
fn reads_spreadsheet_cells_and_expressions() {
    use mitcad_freecad::expression::Expr;
    use mitcad_freecad::spreadsheet::{Content, Sheet};
    let xml = r#"<Document SchemaVersion="4" ProgramVersion="1.0R1">
      <Objects><Object type="Spreadsheet::Sheet" name="Sheet"/><Object type="PartDesign::Pad" name="Pad"/></Objects>
      <ObjectData>
        <Object name="Sheet"><Properties>
          <Property name="cells" type="Spreadsheet::PropertySheet"><Cells Count="4">
            <Cell address="A1" content="&apos;Width" />
            <Cell address="B1" content="=30 mm" alias="Width" />
            <Cell address="B2" content="12" />
            <Cell address="B3" content="=Width &gt; 20 mm ? 5 mm : 7 mm" />
          </Cells></Property>
        </Properties></Object>
        <Object name="Pad"><Properties>
          <Property name="ExpressionEngine" type="App::PropertyExpressionEngine">
            <ExpressionEngine count="1">
              <Expression path="Length" expression="&lt;&lt;Sheet&gt;&gt;.Width * 2"/>
            </ExpressionEngine>
          </Property>
        </Properties></Object>
      </ObjectData></Document>"#;
    let doc = Document::parse(xml).unwrap();
    let sheet = Sheet::of(doc.object("Sheet").unwrap());
    assert_eq!(sheet.cells.len(), 4);
    assert_eq!(sheet.cells[0].content(), Content::Text("Width"));
    assert_eq!(sheet.named("Width").unwrap().address, "B1");
    assert_eq!(sheet.named("B2").unwrap().content(), Content::Number("12"));
    let Content::Expression(e) = sheet.cell("B3").unwrap().content() else {
        panic!("an expression");
    };
    assert!(matches!(Expr::parse(e).unwrap(), Expr::Conditional(..)));
    let Some(Value::Expressions(list)) = doc.object("Pad").unwrap().value("ExpressionEngine")
    else {
        panic!("expressions");
    };
    let refs = Expr::parse(&list[0].expression).unwrap();
    assert_eq!(refs.references()[0].label.as_deref(), Some("Sheet"));
}

#[test]
fn view_documents_parse_on_their_own() {
    let gui = GuiDocument::parse(GUI).unwrap();
    assert_eq!(gui.providers.len(), 3);
    assert_eq!(gui.get("Body").unwrap().visible(), Some(false));
    assert!(gui.get("Nothing").is_none());
}
