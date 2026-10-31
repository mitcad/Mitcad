// SPDX-License-Identifier: MIT
use super::*;

/// A cube of side `size` with its lower corner at `at`, its triangles
/// facing outwards.
fn cube(at: [f64; 3], size: f64) -> Mesh {
    let vertices = (0..8)
        .map(|i| {
            [
                at[0] + size * f64::from(i & 1),
                at[1] + size * f64::from((i >> 1) & 1),
                at[2] + size * f64::from((i >> 2) & 1),
            ]
        })
        .collect();
    let quads = [
        [0, 2, 3, 1], // bottom (-z)
        [4, 5, 7, 6], // top
        [0, 1, 5, 4], // -y
        [2, 6, 7, 3], // +y
        [0, 4, 6, 2], // -x
        [1, 3, 7, 5], // +x
    ];
    let triangles = quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect();
    Mesh {
        vertices,
        triangles,
    }
}

fn part(name: &str, mesh: Mesh, color: Option<[f64; 3]>, placements: Vec<Placement>) -> Part {
    Part {
        name: name.to_owned(),
        color,
        mesh,
        placements,
        metadata: Vec::new(),
    }
}

fn model_xml(data: &[u8]) -> String {
    let archive = mitcad_zip::Archive::new(data.to_vec()).unwrap();
    String::from_utf8(archive.read_by_name("3D/3dmodel.model").unwrap()).unwrap()
}

#[test]
fn mesh_volume_closedness_and_placement() {
    let mesh = cube([1.0, 2.0, 3.0], 2.0);
    assert!((mesh.volume() - 8.0).abs() < 1e-12);
    assert_eq!(mesh.check_closed(), Ok(()));
    let mut open = mesh.clone();
    open.triangles.pop();
    assert!(
        open.check_closed()
            .unwrap_err()
            .contains("not shared by exactly two")
    );
    let mut turned = mesh.clone();
    turned.triangles[0].swap(1, 2);
    assert!(turned.check_closed().is_err());
    let mut degenerate = mesh.clone();
    degenerate.triangles[0] = [0, 0, 1];
    assert!(
        degenerate
            .check_closed()
            .unwrap_err()
            .contains("uses a vertex twice")
    );
    // A mirror keeps the triangles facing outwards.
    let mirror = Placement {
        linear: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };
    let mirrored = mesh.placed(&mirror);
    assert!((mirrored.volume() - 8.0).abs() < 1e-12);
    assert_eq!(mirrored.vertices[0], [-1.0, 2.0, 3.0]);
}

#[test]
fn placements_in_3mf_form() {
    // A quarter turn about z, then 10 along x: (1, 0, 0) goes to (10, 1, 0).
    let turn = Placement {
        linear: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [10.0, 0.0, 0.0],
    };
    assert_eq!(turn.apply([1.0, 0.0, 0.0]), [10.0, 1.0, 0.0]);
    let m = turn.to_3mf();
    // 3MF: p' = p * M with p a row vector and the translation last.
    let p = [1.0, 0.0, 0.0];
    let mapped: [f64; 3] =
        std::array::from_fn(|c| p[0] * m[c] + p[1] * m[3 + c] + p[2] * m[6 + c] + m[9 + c]);
    assert_eq!(mapped, [10.0, 1.0, 0.0]);
    assert_eq!(Placement::from_3mf(m), turn);
    // (1, 0, 0) -> (10, 1, 0) -> (9, 10, 0).
    let twice = turn.after(&turn);
    assert_eq!(twice.apply([1.0, 0.0, 0.0]), [9.0, 10.0, 0.0]);
}

#[test]
fn numbers_have_six_decimals_at_most_in_millimetres() {
    use crate::Unit;
    use crate::write::number_for_tests;
    let number = |value: f64| number_for_tests(value, Unit::Millimeter.decimals());
    assert_eq!(number(12.5), "12.5");
    assert_eq!(number(-0.000125), "-0.000125");
    assert_eq!(number(1.0 / 3.0), "0.333333");
    assert_eq!(number(-0.0000004), "0");
    assert_eq!(number(100.0), "100");
    assert_eq!(number(1e-7), "0");
    // Coarser units keep a nanometre (mitcad#734): 20 mm in inches.
    assert_eq!(
        number_for_tests(20.0 / 25.4, Unit::Inch.decimals()),
        "0.78740157"
    );
}

#[test]
fn writes_one_object_of_parts_and_reads_it_back() {
    let shift = Placement {
        translation: [30.0, 0.0, 5.0],
        ..Placement::IDENTITY
    };
    let model = Model {
        name: "Bracket & <plate>".to_owned(),
        application: "Mitcad 0.0.1".to_owned(),
        parts: vec![
            part(
                "Body1",
                cube([0.0; 3], 10.0),
                Some([1.0, 0.5, 0.0]),
                vec![Placement::IDENTITY],
            ),
            part(
                "Pin \"A\"",
                cube([0.0; 3], 2.0),
                None,
                vec![shift, Placement::IDENTITY],
            ),
            part(
                "Body3",
                cube([20.0, 0.0, 0.0], 5.0),
                Some([0.2, 0.4, 0.8]),
                vec![Placement::IDENTITY],
            ),
        ],
        ..Model::default()
    };
    let data = write(&model).unwrap();
    assert_eq!(&data[..2], b"PK");
    let xml = model_xml(&data);
    assert!(xml.contains("<model unit=\"millimeter\""));
    assert!(xml.contains(&format!("xmlns=\"{CORE_NAMESPACE}\"")));
    assert!(xml.contains("<metadata name=\"Application\">Mitcad 0.0.1</metadata>"));
    assert!(xml.contains("<base name=\"Body1\" displaycolor=\"#FF8000FF\"/>"));
    assert!(xml.contains("<base name=\"Body3\" displaycolor=\"#3366CCFF\"/>"));
    assert!(xml.contains("<object id=\"2\" type=\"model\" name=\"Body1\" pid=\"1\" pindex=\"0\">"));
    assert!(xml.contains("<object id=\"3\" type=\"model\" name=\"Pin &quot;A&quot;\">"));
    assert!(xml.contains("<object id=\"4\" type=\"model\" name=\"Body3\" pid=\"1\" pindex=\"1\">"));
    assert!(xml.contains("<object id=\"5\" type=\"model\" name=\"Bracket &amp; &lt;plate&gt;\">"));
    assert!(xml.contains("<component objectid=\"3\" transform=\"1 0 0 0 1 0 0 0 1 30 0 5\"/>"));
    assert!(xml.contains("<component objectid=\"2\"/>"));
    // One build item: the object of components.
    assert_eq!(xml.matches("<item ").count(), 1);
    assert!(xml.contains("<item objectid=\"5\"/>"));

    let back = read(&data).unwrap();
    assert_eq!(back.name, "Bracket & <plate>");
    assert_eq!(back.parts.len(), 3);
    assert_eq!(back.parts[0].name, "Body1");
    assert_eq!(back.parts[0].color, Some([1.0, 128.0 / 255.0, 0.0]));
    assert_eq!(back.parts[1].name, "Pin \"A\"");
    assert_eq!(back.parts[1].color, None);
    assert_eq!(back.parts[1].placements, vec![shift, Placement::IDENTITY]);
    for (written, read) in model.parts.iter().zip(&back.parts) {
        assert_eq!(read.mesh, written.mesh);
        assert!((read.mesh.volume() - written.mesh.volume()).abs() < 1e-9);
    }
}

#[test]
fn rejects_what_the_core_specification_does_not_allow() {
    let good = Model {
        name: "Part".to_owned(),
        application: String::new(),
        parts: vec![part(
            "Body1",
            cube([0.0; 3], 1.0),
            None,
            vec![Placement::IDENTITY],
        )],
        ..Model::default()
    };
    let data = write(&good).unwrap();
    assert!(read(&data).is_ok());
    let xml = model_xml(&data);
    let package = |model: &str| {
        let mut zip = mitcad_zip::Writer::new();
        let archive = mitcad_zip::Archive::new(data.clone()).unwrap();
        for name in ["[Content_Types].xml", "_rels/.rels"] {
            zip.add(
                name,
                &archive.read_by_name(name).unwrap(),
                mitcad_zip::Method::Stored,
            )
            .unwrap();
        }
        zip.add(
            "3D/3dmodel.model",
            model.as_bytes(),
            mitcad_zip::Method::Stored,
        )
        .unwrap();
        zip.finish().unwrap()
    };
    let fails = |model: String, text: &str| {
        let error = read(&package(&model)).unwrap_err().to_string();
        assert!(error.contains(text), "{error} (expected: {text})");
    };
    // A triangle turned round, one left out, an index out of range.
    fails(
        xml.replacen(
            "v1=\"0\" v2=\"2\" v3=\"3\"",
            "v1=\"0\" v2=\"3\" v3=\"2\"",
            1,
        ),
        "opposite directions",
    );
    fails(
        xml.replacen("<triangle v1=\"0\" v2=\"2\" v3=\"3\"/>", "", 1),
        "opposite directions",
    );
    fails(
        xml.replacen(
            "v1=\"0\" v2=\"2\" v3=\"3\"",
            "v1=\"0\" v2=\"2\" v3=\"9\"",
            1,
        ),
        "beyond the 8 vertices",
    );
    // References, units, extensions, the namespace.
    fails(
        xml.replace("<item objectid=\"3\"/>", "<item objectid=\"7\"/>"),
        "object 7, which is not defined",
    );
    fails(
        xml.replace("unit=\"millimeter\"", "unit=\"furlong\""),
        "unknown unit",
    );
    fails(
        xml.replace("<model ", "<model requiredextensions=\"p\" "),
        "requires extensions",
    );
    fails(
        xml.replace(CORE_NAMESPACE, "http://example.invalid/3mf"),
        "not a 3MF core <model>",
    );
    fails(xml.replace("<item objectid=\"3\"/>", ""), "no items");
    // Every triangle turned round: closed, but facing inwards.
    let inside_out = Model {
        parts: vec![part(
            "Body1",
            Mesh {
                triangles: cube([0.0; 3], 1.0)
                    .triangles
                    .iter()
                    .map(|&[a, b, c]| [a, c, b])
                    .collect(),
                ..cube([0.0; 3], 1.0)
            },
            None,
            vec![Placement::IDENTITY],
        )],
        ..good.clone()
    };
    let error = read(&write(&inside_out).unwrap()).unwrap_err().to_string();
    assert!(error.contains("face inwards"), "{error}");
    // Inches are read in millimetres.
    let inches = read(&package(
        &xml.replace("unit=\"millimeter\"", "unit=\"inch\""),
    ))
    .unwrap();
    assert!((inches.parts[0].mesh.volume() - 25.4f64.powi(3)).abs() < 1e-6);
    // Nothing to write.
    assert!(write(&Model::default()).is_err());
}

#[test]
fn settings_go_with_the_model_and_read_back() {
    let mut body = part(
        "Body1",
        cube([0.0; 3], 10.0),
        None,
        vec![Placement::IDENTITY],
    );
    body.metadata = vec![("cura:extruder_nr".to_owned(), "1".to_owned())];
    let model = Model {
        name: "Settings".to_owned(),
        application: "BambuStudio-02.00.00.00".to_owned(),
        parts: vec![body],
        metadata: vec![("BambuStudio:3mfVersion".to_owned(), "1".to_owned())],
        namespaces: vec![
            (
                "BambuStudio".to_owned(),
                "http://schemas.bambulab.com/package/2021".to_owned(),
            ),
            (
                "cura".to_owned(),
                "http://software.ultimaker.com/xml/cura/3mf/2015/10".to_owned(),
            ),
        ],
        files: vec![(
            "Metadata/project_settings.config".to_owned(),
            b"{\"layer_height\": \"0.2\"}".to_vec(),
        )],
        unit: Unit::Millimeter,
        objects: Vec::new(),
    };
    let data = write(&model).unwrap();
    let xml = model_xml(&data);
    assert!(xml.contains("xmlns:BambuStudio=\"http://schemas.bambulab.com/package/2021\""));
    assert!(xml.contains("<metadata name=\"BambuStudio:3mfVersion\">1</metadata>"));
    assert!(xml.contains(
        "<metadata name=\"cura:extruder_nr\" preserve=\"true\" type=\"xs:string\">1</metadata>"
    ));
    // The geometry reads as before.
    assert_eq!(read(&data).unwrap().parts.len(), 1);
    // The settings read back without it.
    let package = read_package(&data).unwrap();
    assert_eq!(
        package.metadata("Application"),
        Some("BambuStudio-02.00.00.00")
    );
    assert_eq!(package.metadata("BambuStudio:3mfVersion"), Some("1"));
    assert_eq!(
        package.file("Metadata/project_settings.config"),
        Some(&b"{\"layer_height\": \"0.2\"}"[..])
    );
    let body = package.objects.iter().find(|o| o.name == "Body1").unwrap();
    assert_eq!(
        body.metadata,
        vec![("cura:extruder_nr".to_owned(), "1".to_owned())]
    );
    // Other files have a content type, and paths outside the package are
    // refused.
    let types = String::from_utf8(
        mitcad_zip::Archive::new(data.clone())
            .unwrap()
            .read_by_name("[Content_Types].xml")
            .unwrap(),
    )
    .unwrap();
    assert!(types.contains("<Default Extension=\"config\""));
    let outside = Model {
        files: vec![("../escape.config".to_owned(), Vec::new())],
        ..model.clone()
    };
    assert!(write(&outside).is_err());
}

#[test]
fn a_model_in_another_unit_reads_back_in_millimetres() {
    // A cube of 1 inch, moved by 2 inches (mitcad#734).
    let model = Model {
        name: "Inches".to_owned(),
        parts: vec![part(
            "Body1",
            cube([0.0; 3], 1.0),
            None,
            vec![Placement {
                translation: [2.0, 0.0, 0.0],
                ..Placement::IDENTITY
            }],
        )],
        unit: Unit::Inch,
        ..Model::default()
    };
    let data = write(&model).unwrap();
    assert!(model_xml(&data).contains("<model unit=\"inch\""));
    let read = read(&data).unwrap();
    assert_eq!(read.unit, Unit::Millimeter);
    assert!((read.parts[0].mesh.volume() - 25.4f64.powi(3)).abs() < 1e-6);
    assert!((read.parts[0].placements[0].translation[0] - 50.8).abs() < 1e-9);
    for (unit, name) in [
        (Unit::Micron, "micron"),
        (Unit::Millimeter, "millimeter"),
        (Unit::Centimeter, "centimeter"),
        (Unit::Meter, "meter"),
        (Unit::Foot, "foot"),
    ] {
        assert_eq!(unit.as_str(), name);
    }
}

#[test]
fn several_objects_are_build_items_of_their_own_and_read_back() {
    // A nut placed three times (mitcad#856): two objects of one placement
    // each, a third of the last placement and the washer's.
    let at = |x: f64| Placement {
        translation: [x, 0.0, 0.0],
        ..Placement::IDENTITY
    };
    let model = Model {
        name: "Nuts".to_owned(),
        parts: vec![
            part(
                "Nut",
                cube([0.0; 3], 5.0),
                None,
                vec![at(0.0), at(10.0), at(20.0)],
            ),
            part("Washer", cube([0.0; 3], 2.0), None, vec![at(30.0)]),
        ],
        objects: vec![
            BuildObject {
                name: "Nut (Nut:1)".to_owned(),
                components: vec![(0, 0)],
            },
            BuildObject {
                name: "Nut (Nut:2)".to_owned(),
                components: vec![(0, 1)],
            },
            BuildObject {
                name: "Rest".to_owned(),
                components: vec![(0, 2), (1, 0)],
            },
        ],
        ..Model::default()
    };
    let data = write(&model).unwrap();
    let xml = model_xml(&data);
    assert_eq!(xml.matches("<item ").count(), 3);
    for id in [4, 5, 6] {
        assert!(xml.contains(&format!("<item objectid=\"{id}\"/>")), "{xml}");
    }
    assert_eq!(build_object_id(2, 0), assembly_object_id(2));
    assert!(xml.contains("<object id=\"5\" type=\"model\" name=\"Nut (Nut:2)\">"));
    let back = read(&data).unwrap();
    assert_eq!(back.objects, model.objects);
    assert_eq!(back.parts[0].placements, model.parts[0].placements);
    // One object reads back as the model's one.
    let one = Model {
        objects: Vec::new(),
        ..model.clone()
    };
    assert!(read(&write(&one).unwrap()).unwrap().objects.is_empty());
    // An object of a placement that is not there is refused.
    let wrong = Model {
        objects: vec![BuildObject {
            name: "Wrong".to_owned(),
            components: vec![(0, 3)],
        }],
        ..model.clone()
    };
    assert!(write(&wrong).is_err());
}

/// A package of `(name, bytes, declared)` entries, stored; an entry with a
/// `declared` size declares itself a deflate entry of that many bytes (in
/// its local header and the central directory), what a deflate run of its
/// bytes inflating to `declared` bytes would declare, without the test
/// holding them. Bytes of 0xff are no valid deflate stream, so a read that
/// got past the size checks would fail to inflate them.
fn declared_package(entries: &[(&str, &[u8], Option<u32>)]) -> Vec<u8> {
    let files: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|&(name, bytes, _)| (name, bytes))
        .collect();
    let mut zip = mitcad_zip::stored_zip(&files);
    let u16_at = |b: &[u8], at: usize| usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
    let u32_at =
        |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let mut central = u32_at(&zip, zip.len() - 22 + 16);
    for &(_, _, declared) in entries {
        if let Some(declared) = declared {
            let local = u32_at(&zip, central + 42);
            for (method_at, size_at) in [(local + 8, local + 22), (central + 10, central + 24)] {
                zip[method_at..method_at + 2]
                    .copy_from_slice(&mitcad_zip::Method::Deflate.code().to_le_bytes());
                zip[size_at..size_at + 4].copy_from_slice(&declared.to_le_bytes());
            }
        }
        central += 46
            + u16_at(&zip, central + 28)
            + u16_at(&zip, central + 30)
            + u16_at(&zip, central + 32);
    }
    zip
}

#[test]
fn a_package_refuses_a_model_part_over_its_budget_before_reading_it() {
    // 1 GiB declared from 1 MiB: within the zip reader's expansion ratio
    // (1024:1) and its large entry limit, but over the package's budget.
    let run = vec![0xffu8; 1 << 20];
    let bomb = declared_package(&[("3D/3dmodel.model", run.as_slice(), Some(1 << 30))]);
    let archive = mitcad_zip::Archive::new(bomb.clone()).unwrap();
    assert_eq!(archive.entries()[0].size, 1 << 30);
    let error = read_package(&bomb).unwrap_err().to_string();
    assert!(
        error.contains("3D/3dmodel.model declares 1073741824 bytes, more than"),
        "{error}"
    );
    // The model part the relationships name, and the relationships
    // themselves, are held to the budget too.
    let rels = format!(
        "<Relationships><Relationship Target=\"/3D/other.model\" \
         Type=\"{MODEL_RELATIONSHIP}\"/></Relationships>"
    );
    let named = declared_package(&[
        ("_rels/.rels", rels.as_bytes(), None),
        ("3D/other.model", run.as_slice(), Some(1 << 30)),
    ]);
    let error = read_package(&named).unwrap_err().to_string();
    assert!(error.contains("3D/other.model declares"), "{error}");
    let relationships = declared_package(&[("_rels/.rels", run.as_slice(), Some(1 << 30))]);
    let error = read_package(&relationships).unwrap_err().to_string();
    assert!(error.contains("_rels/.rels declares"), "{error}");
}

#[test]
fn a_package_of_too_many_entries_is_refused() {
    let mut zip = mitcad_zip::Writer::new();
    for i in 0..5000 {
        zip.add(&format!("a{i}.json"), b"{}", mitcad_zip::Method::Stored)
            .unwrap();
    }
    let error = read_package(&zip.finish().unwrap())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the package has 5000 entries, more than the limit"),
        "{error}"
    );
}

#[test]
fn a_package_whose_files_declare_too_much_in_all_is_refused() {
    // Five files of 60 MiB each, from 60 KiB each: each within the file
    // limit, together over the total, refused before any is read.
    let run = vec![0xffu8; 60 << 10];
    let names: Vec<String> = (0..5)
        .map(|i| format!("Metadata/plate_{i}.config"))
        .collect();
    let entries: Vec<(&str, &[u8], Option<u32>)> = names
        .iter()
        .map(|name| (name.as_str(), run.as_slice(), Some(60 << 20)))
        .collect();
    let error = read_package(&declared_package(&entries))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the package's files declare 314572800 bytes, more than the limit"),
        "{error}"
    );
    // A file over the file limit is still left out, not an error.
    let big = vec![0xffu8; 70 << 10];
    let package = read_package(&declared_package(&[
        ("Metadata/big.config", big.as_slice(), Some(70 << 20)),
        ("Metadata/project_settings.config", &b"{}"[..], None),
    ]))
    .unwrap();
    assert_eq!(package.file("Metadata/big.config"), None);
    assert_eq!(
        package.file("Metadata/project_settings.config"),
        Some(&b"{}"[..])
    );
}

/// Damaged files (mitcad#939): every cut of a package is refused or reads
/// the model as written, and what is no zip archive is refused, by both
/// readers, never with a panic.
#[test]
fn cut_packages_and_other_files_are_refused() {
    let model = Model {
        name: "Cut".to_owned(),
        parts: vec![part(
            "Body1",
            cube([0.0; 3], 10.0),
            Some([1.0, 0.5, 0.0]),
            vec![Placement::IDENTITY],
        )],
        files: vec![(
            "Metadata/project_settings.config".to_owned(),
            b"{\"layer_height\": \"0.2\"}".to_vec(),
        )],
        ..Model::default()
    };
    let data = write(&model).unwrap();
    let whole = read(&data).unwrap();
    for cut in 0..data.len() {
        let prefix = &data[..cut];
        if let Ok(back) = read(prefix) {
            assert_eq!(back.parts, whole.parts, "cut at {cut}");
        }
        let _ = read_package(prefix);
    }
    for other in [
        &b""[..],
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
        b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1\0\0",
    ] {
        assert!(read(other).is_err());
        assert!(read_package(other).is_err());
    }
}
