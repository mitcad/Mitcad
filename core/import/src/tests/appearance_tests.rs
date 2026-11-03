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
