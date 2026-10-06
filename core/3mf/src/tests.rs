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
fn numbers_have_six_decimals_at_most() {
    use crate::write::number_for_tests as number;
    assert_eq!(number(12.5), "12.5");
    assert_eq!(number(-0.000125), "-0.000125");
    assert_eq!(number(1.0 / 3.0), "0.333333");
    assert_eq!(number(-0.0000004), "0");
    assert_eq!(number(100.0), "100");
    assert_eq!(number(1e-7), "0");
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
