// SPDX-License-Identifier: MIT
//! Appearances and materials (mitcad#228).

use super::*;

// Appearances and materials (mitcad#228).

/// The stored bodies as the file's blob holds them: `BREP.test.smbh#7`,
/// `#8`, ... (the decoder's body map names them so).
struct BlobGeometry(TestGeometry);

impl StoredGeometry<MockShape> for BlobGeometry {
    fn state_count(&mut self) -> usize {
        self.0.state_count()
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
        Ok(self
            .0
            .state(index)?
            .into_iter()
            .enumerate()
            .map(|(k, mut b)| {
                b.source = format!("BREP.test.smbh#{}", 7 + k);
                b
            })
            .collect())
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.0.item_state(index)
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
        let last = self.0.state_count() - 1;
        self.state(last)
    }
}

/// The decoder's appearance data of one body: a red paint, the steel
/// material, and one face (`point`, cm) in the steel's satin look.
fn appearance_data(point: [f64; 3]) -> Value {
    let paint = "23303B60-6683-46D0-BC5B-B92F9F3CA4E8_Post2015";
    let satin = "703A8DBC-1CFC-4F45-A969-681245623F44_Post2015";
    json!({
        "assets": [
            {"id": paint, "schema": "PrismOpaqueSchema", "name": "Prism-093",
             "ui_name": "Prism-093", "description": "Paint - enamel red glossy",
             "properties": {"opaque_albedo": [0.7674, 0.206, 0.1517, 1.0],
                            "opaque_albedo_colorspace": 1, "opaque_f0": 0.06027025,
                            "surface_roughness": 0.0775, "opaque_translucency": false}},
            {"id": satin, "schema": "PrismMetalSchema",
             "name": "PrismMaterial-018_physmat_aspects:Prism-256", "ui_name": "Prism-256",
             "description": "Steel - satin",
             "properties": {"metal_f0": [0.6262, 0.6262, 0.6262, 1.0],
                            "surface_roughness": 0.2646}},
            {"id": "4177D984-5662-4715-B8AF-E5D65A04EDEE", "schema": "PhysMatSchema",
             "name": "PrismMaterial-018", "description": "Steel, mill finish",
             "properties": {"physmat_classification": "Metal.Ferrous.Carbon Steel"},
             "connections": {"physmat_aspects": ["F3981BFD-5548-497A-A042-21FC315C9862"]}},
            {"id": "F3981BFD-5548-497A-A042-21FC315C9862", "schema": "StructuralMetalSchema",
             "name": "PhysicalMetal_124",
             "properties": {"structural_Density": {"unit": 69633, "value": 7850.0}}}
        ],
        "bodies": [{
            "entity": 300, "name": "Block", "blob": "BREP.test.smbh", "selector": 0,
            "record": 7,
            "appearance": {"token": "23303B60-6683-46D0-BC5B-B92F9F3CA4E8", "asset": paint,
                           "preset": "Prism-093"},
            "material": {"token": "4177D984-5662-4715-B8AF-E5D65A04EDEE",
                         "asset": "4177D984-5662-4715-B8AF-E5D65A04EDEE",
                         "preset": "PrismMaterial-018"},
            "faces": [{"guid": "0438b4bf-d97c-4f0d-9ef2-713f87b07603",
                       "appearance": {"asset": satin},
                       "faces": [{"point_on_face": point}]}]
        }]
    })
}

/// The file's appearances and material come in on the replay's body
/// (matched with the stored body the body map names) and on its face
/// (found through the stored face's point); each asset makes one document
/// appearance with the asset's parameters on Mitcad's set.
#[test]
fn appearances_and_materials_come_in_on_bodies_and_faces() {
    let (items, geometry) = block_history(Vec::new());
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    // A face of the block, as the stored design has it.
    let mut reference = Document::new(kernel());
    let mut g = BlobGeometry(block_history(Vec::new()).1);
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut g,
        &Options::default(),
    );
    let body = reference.bodies()[0].shape.clone();
    let face = body.faces[0].clone();
    let point = face_point(&face).map(|x| x / 10.0);

    let mut source = serde_json::to_value(timeline_dump(items)).unwrap();
    source["_f3d"] = json!({"appearance": appearance_data(point)});
    let mut doc = Document::new(kernel());
    let mut geometry = BlobGeometry(geometry);
    let report = import_design(&mut doc, &dump(source), &mut geometry, &Options::default());
    let a = &report.appearances;
    assert_eq!((a.bodies_set, a.bodies), (1, 1), "{}", report.text());
    assert_eq!((a.faces_set, a.faces), (1, 1), "{}", report.text());
    assert_eq!((a.materials_set, a.materials), (1, 1), "{}", report.text());
    assert!(a.gaps.is_empty(), "{:?}", a.gaps);
    assert_eq!(a.physical_materials[0].density, Some(7850.0));
    assert!(report.text().contains("appearances: bodies 1/1, faces 1/1"));

    let uid = doc.bodies()[0].uid;
    let attributes = doc.body_attributes(uid);
    assert_eq!(attributes.material.as_deref(), Some("steel"));
    let id = attributes.appearance.clone().unwrap();
    assert_eq!(id, "f3d_paint_enamel_red_glossy");
    let red = doc.appearance(&id).unwrap();
    assert_eq!(red.name, "Paint - enamel red glossy");
    assert_eq!(red.base_color, [0.7674, 0.206, 0.1517]);
    assert_eq!(red.metalness, 0.0);
    assert!((red.ior - 1.6506).abs() < 1e-3, "{}", red.ior);
    assert!((red.roughness - 0.0775).abs() < 1e-12);
    // The face, and the faces the test kernel takes for pieces of it (its
    // faces all have one normal).
    let faces = &attributes.face_appearances;
    assert_eq!(
        faces.get(&face.to_string()).map(String::as_str),
        Some("f3d_steel_satin")
    );
    assert!(faces.values().all(|a| a == "f3d_steel_satin"));
    let satin = doc.appearance("f3d_steel_satin").unwrap();
    assert_eq!(satin.metalness, 1.0);
    assert_eq!(satin.base_color, [0.6262; 3]);
    assert_eq!(doc.custom_appearances().len(), 2);
}

/// A body the file hides (its record's light bulb off, mitcad#1175) comes
/// in hidden, counted in the report; one whose light bulb is on, or not
/// given, stays shown.
#[test]
fn a_body_the_file_hides_comes_in_hidden() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let replay = |light_bulb: Option<bool>| {
        let (items, geometry) = block_history(Vec::new());
        let mut data = appearance_data([0.0; 3]);
        if let Some(on) = light_bulb {
            data["bodies"][0]["light_bulb"] = json!(on);
        }
        let mut source = serde_json::to_value(timeline_dump(items)).unwrap();
        source["_f3d"] = json!({"appearance": data});
        let mut doc = Document::new(kernel());
        let mut geometry = BlobGeometry(geometry);
        let report = import_design(&mut doc, &dump(source), &mut geometry, &Options::default());
        let visible = doc.body_attributes(doc.bodies()[0].uid).visible;
        (report, visible)
    };
    let (report, visible) = replay(Some(false));
    let a = &report.appearances;
    assert_eq!((a.hidden_set, a.hidden), (1, 1), "{}", report.text());
    assert!(!visible, "{}", report.text());
    assert!(
        report.text().contains("hidden bodies: 1/1"),
        "{}",
        report.text()
    );
    for on in [Some(true), None] {
        let (report, visible) = replay(on);
        assert_eq!(report.appearances.hidden, 0, "{}", report.text());
        assert!(visible, "{}", report.text());
    }
}

/// A body that names its stored body by its source (an `.ipt` body,
/// mitcad#1367: `<B-rep record>/<ASM body record>`, no blob and record)
/// joins the stored body of that source.
#[test]
fn a_body_named_by_its_source_gets_its_appearance() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let (items, geometry) = block_history(Vec::new());
    let mut data = appearance_data([0.0; 3]);
    let body = data["bodies"][0].as_object_mut().unwrap();
    for key in ["blob", "selector", "record", "faces"] {
        body.remove(key);
    }
    body.insert("source".to_owned(), json!("BREP.test.smbh#7"));
    let mut source = serde_json::to_value(timeline_dump(items)).unwrap();
    source["_f3d"] = json!({"appearance": data});
    let mut doc = Document::new(kernel());
    let mut geometry = BlobGeometry(geometry);
    let report = import_design(&mut doc, &dump(source), &mut geometry, &Options::default());
    let a = &report.appearances;
    assert_eq!((a.bodies_set, a.bodies), (1, 1), "{}", report.text());
    assert!(a.gaps.is_empty(), "{:?}", a.gaps);
    let uid = doc.bodies()[0].uid;
    assert_eq!(
        doc.body_attributes(uid).appearance.as_deref(),
        Some("f3d_paint_enamel_red_glossy")
    );
}

/// A stored body of two solids comes in as two bodies, its lumps
/// (`<source>#0`, `<source>#1`, as the `.ipt` import names them,
/// mitcad#1375): the appearance and the material of the record that names
/// the stored body go on both, and its face's look on the lump that has the
/// face. Applied without a replay, as the `.ipt` import of the bodies only
/// does (`appearances::apply`).
#[test]
fn a_stored_body_of_two_lumps_gets_its_appearance_on_both() {
    let kernel = TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    // Two blocks, one body each.
    let mut doc = Document::new(kernel);
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    let region = "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}";
    add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": "d2"}, "flip": false,
               "operation": "new_body"}),
    );
    let uids: Vec<BodyUid> = doc.bodies().iter().map(|b| b.uid).collect();
    assert_eq!(uids.len(), 2);
    // A face of the second lump, as the stored design has it (cm).
    let face = doc.bodies()[1].shape.faces[0].clone();
    let point = face_point(&face).map(|x| x / 10.0);

    // The record names the stored body by its source, not a lump.
    let mut data = appearance_data(point);
    let body = data["bodies"][0].as_object_mut().unwrap();
    for key in ["blob", "selector", "record"] {
        body.remove(key);
    }
    body.insert("source".to_owned(), json!("PmBRepSegment#1/1"));
    let data: mitcad_dump::AppearanceData = serde_json::from_value(data).unwrap();
    let stored = vec![
        ("PmBRepSegment#1/1#0".to_owned(), Some(uids[0])),
        ("PmBRepSegment#1/1#1".to_owned(), Some(uids[1])),
    ];
    let components = crate::components::Components::default();
    let a = crate::appearances::apply(&mut doc, &data, &stored, &components);
    assert!(a.gaps.is_empty(), "{:?}", a.gaps);
    assert_eq!((a.bodies_set, a.bodies), (1, 1), "{a:?}");
    assert_eq!((a.materials_set, a.materials), (1, 1), "{a:?}");
    assert_eq!((a.faces_set, a.faces), (1, 1), "{a:?}");
    assert_eq!(a.not_in_design, 0);
    for &uid in &uids {
        let attributes = doc.body_attributes(uid);
        assert_eq!(
            attributes.appearance.as_deref(),
            Some("f3d_paint_enamel_red_glossy"),
            "{uid}"
        );
        assert_eq!(attributes.material.as_deref(), Some("steel"), "{uid}");
    }
    assert_eq!(
        doc.body_attributes(uids[1])
            .face_appearances
            .get(&face.to_string())
            .map(String::as_str),
        Some("f3d_steel_satin")
    );

    // A stored body whose source only begins like the record's is another
    // one: not joined, and counted as a stored body no record names.
    let mut other = Document::new(TestKernel {
        face_listing: true,
        ..TestKernel::default()
    });
    import_design(
        &mut other,
        &timeline_dump(vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        ]),
        &mut NoGeometry,
        &Options::default(),
    );
    let uid = other.bodies()[0].uid;
    let stored = vec![("PmBRepSegment#1/10".to_owned(), Some(uid))];
    let a = crate::appearances::apply(&mut other, &data, &stored, &components);
    assert_eq!((a.bodies_set, a.not_in_design), (0, 1), "{a:?}");
    assert!(
        a.gaps
            .iter()
            .any(|g| g.contains("stored body PmBRepSegment#1/10: no body record names it")),
        "{:?}",
        a.gaps
    );
    assert_eq!(other.body_attributes(uid).appearance, None);
}
