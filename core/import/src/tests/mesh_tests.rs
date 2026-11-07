// SPDX-License-Identifier: MIT
//! Mesh bodies the file stores (mitcad#1366): a mesh body item brings its
//! meshes in as a base feature in its component, suppressed when the item
//! is; the meshes no item took come in at the end; one that cannot be read
//! is a warning.

use super::*;

/// The file's mesh bodies, each given once.
struct Meshes {
    meshes: Vec<StoredMesh>,
    taken: Vec<bool>,
}

impl Meshes {
    fn new(meshes: Vec<StoredMesh>) -> Self {
        let taken = vec![false; meshes.len()];
        Self { meshes, taken }
    }
}

impl StoredGeometry<MockShape> for Meshes {
    fn state_count(&mut self) -> usize {
        0
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
        Err(format!("there is no history state {index}"))
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
        Ok(Vec::new())
    }

    fn take_meshes(&mut self, feature: Option<u64>) -> Vec<StoredMesh> {
        let mut out = Vec::new();
        for (mesh, taken) in self.meshes.iter().zip(self.taken.iter_mut()) {
            if !*taken && (feature.is_none() || mesh.feature == feature) {
                *taken = true;
                out.push(mesh.clone());
            }
        }
        out
    }
}

/// A tetrahedron's mesh (mm) of the item `feature` in `component`.
fn tetrahedron(
    source: &str,
    feature: Option<u64>,
    component: Option<u64>,
    name: Option<&str>,
) -> StoredMesh {
    StoredMesh {
        source: source.to_owned(),
        feature,
        component,
        name: name.map(str::to_owned),
        mesh: Ok(MeshData {
            points: vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [0.0, 10.0, 0.0],
                [0.0, 0.0, 10.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }),
        id: None,
    }
}

/// A design of the root `Assembly` (object 3) and `Scanner` (object 10)
/// placed 5 cm along X; its timeline a mesh body item in Scanner (object
/// 50) and one suppressed in the file in the root (object 51).
fn scan_dump() -> Dump {
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "scan.f3d"},
        "document": {"name": "Scan", "root_component": "Assembly"},
        "components": [
            {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
            {"name": "Scanner", "_f3d": {"object_id": 10}}],
        "occurrences": [
            {"component": "Scanner", "_f3d": {"component_object": 10},
             "transform": [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                           [0.0, 0.0, 0.0, 1.0]]}],
        "timeline": {"items": [
            {"index": 0, "name": "Mesh1", "objectType": "MeshFeature",
             "_f3d": {"object_id": 50, "component": 10}},
            {"index": 1, "name": "Mesh2", "objectType": "MeshFeature", "isSuppressed": true,
             "_f3d": {"object_id": 51, "component": 3}}]}
    }))
}

#[test]
fn mesh_bodies_come_in_with_their_items_in_their_components() {
    let mut doc = Document::new(MockKernel::default());
    let mut broken = tetrahedron("broken.paramesh", None, None, None);
    broken.mesh = Err("the container ends early".to_owned());
    let mut geometry = Meshes::new(vec![
        tetrahedron("scan.paramesh", Some(50), Some(10), Some("Scan body")),
        tetrahedron("hidden.paramesh", Some(51), Some(3), None),
        // No item names it: at the end, in its component.
        tetrahedron("loose.paramesh", None, Some(10), Some("Loose body")),
        broken,
    ]);
    let report = import_design(&mut doc, &scan_dump(), &mut geometry, &Options::default());
    let text = report.text();
    let scanner = doc
        .assembly()
        .components
        .iter()
        .find(|c| c.name == "Scanner")
        .map(|c| c.uid)
        .expect("Scanner");
    // The item in Scanner: a base feature of its mesh, parametric.
    let item = &report.items[0];
    assert_eq!(item.outcome, Outcome::Parametric, "{text}");
    assert_eq!(item.input_provenance, Some(InputProvenance::Read), "{text}");
    assert_eq!(item.features.len(), 1, "{text}");
    assert_eq!(item.component.as_deref(), Some("Scanner"), "{text}");
    let bodies = doc.bodies();
    let body = |name: &str| bodies.iter().find(|b| b.name == name).map(|b| b.uid);
    let scan = body("Scan body").expect("the item's mesh body");
    assert_eq!(doc.body_component(scan), Some(scanner), "{text}");
    let shape = bodies.iter().find(|b| b.uid == scan).unwrap().shape;
    assert!(matches!(doc.kernel().body_kind(shape), Ok(BodyKind::Mesh)));
    // The suppressed item: its mesh as a suppressed base feature.
    let hidden = &report.items[1];
    assert_eq!(hidden.features.len(), 1, "{text}");
    assert!(
        hidden
            .note
            .as_deref()
            .is_some_and(|n| n.contains("suppressed in the file")),
        "{text}"
    );
    let uid: FeatureUid = hidden.features[0].parse().unwrap();
    assert!(doc.feature(uid).unwrap().suppressed, "{text}");
    // The mesh no item took: at the end, in Scanner.
    let loose = body("Loose body").expect("the loose mesh body");
    assert_eq!(doc.body_component(loose), Some(scanner), "{text}");
    assert_eq!(bodies.len(), 2, "{text}");
    // The one that cannot be read: a warning with its reason.
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("broken.paramesh") && w.contains("ends early")),
        "{text}"
    );
    // No comparison takes a mesh for a body the file does not store.
    assert!(report.extra_bodies.is_empty(), "{text}");
}

#[test]
fn an_item_without_its_mesh_is_skipped_with_the_reason() {
    let mut doc = Document::new(MockKernel::default());
    let mut geometry = Meshes::new(Vec::new());
    let report = import_design(&mut doc, &scan_dump(), &mut geometry, &Options::default());
    let text = report.text();
    assert_eq!(report.items[0].outcome, Outcome::Skipped, "{text}");
    assert!(
        report.items[0]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("no mesh body")),
        "{text}"
    );
    assert!(doc.bodies().is_empty(), "{text}");
}

#[test]
fn the_stored_bodies_alone_bring_the_meshes_in() {
    // No timeline (the stored bodies alone): every mesh at the end.
    let mut doc = Document::new(MockKernel::default());
    let mut geometry = Meshes::new(vec![
        tetrahedron("one.paramesh", Some(50), None, None),
        tetrahedron("two.paramesh", Some(51), None, None),
    ]);
    let bodies_only = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "scan.f3d"}
    }));
    let report = import_design(&mut doc, &bodies_only, &mut geometry, &Options::default());
    let text = report.text();
    assert_eq!(doc.bodies().len(), 2, "{text}");
    // One base feature for both, in the root.
    let bases: Vec<_> = doc
        .features()
        .filter(|f| f.def.type_name() == "base")
        .collect();
    assert_eq!(bases.len(), 1, "{text}");
}

/// The decoder's appearance data of the scan's mesh body (mesh body record
/// 70): the steel material and the satin metal appearance it brings, as
/// its presentation envelopes name them; and a record (71) of a mesh body
/// that did not come in (mitcad#1376).
fn mesh_looks() -> Value {
    let satin = "D5EA7CB4-0000-49E5-A099-8AD50EB050AB_Post2015";
    let steel = "E7566FA6-0000-468D-BFDC-383370014633";
    let aspect = "F3981BFD-0000-497A-A042-21FC315C9862";
    json!({
        "assets": [
            {"id": satin, "schema": "PrismMetalSchema",
             "name": "PrismMaterial-018_physmat_aspects:Prism-256", "ui_name": "Prism-256",
             "description": "Steel - satin",
             "properties": {"metal_f0": [0.6262, 0.6262, 0.6262, 1.0],
                            "surface_roughness": 0.2646}},
            {"id": steel, "schema": "PhysMatSchema", "name": "PrismMaterial-018",
             "properties": {"physmat_classification": "Metal.Ferrous.Steel"},
             "connections": {"physmat_aspects": [aspect]}},
            {"id": aspect, "schema": "StructuralMetalSchema", "name": "PhysicalMetal_124",
             "properties": {"structural_Density": {"unit": 69633, "value": 7850.0}}}
        ],
        "bodies": [],
        "meshes": [
            {"entity": 70,
             "appearance": {"token": "D5EA7CB4-0000-49E5-A099-8AD50EB050AB", "asset": satin,
                            "preset": "PrismMaterial-018_physmat_aspects:Prism-256"},
             "material": {"token": steel, "asset": steel, "preset": "PrismMaterial-018"}},
            {"entity": 71,
             "appearance": {"token": "D5EA7CB4-0000-49E5-A099-8AD50EB050AB", "asset": satin,
                            "preset": "PrismMaterial-018_physmat_aspects:Prism-256"}}
        ]
    })
}

/// A mesh body gets the physical material and the appearance its
/// presentation envelopes name (mitcad#1376), as the file's bodies do: the
/// appearance's asset as one document appearance, the material as
/// MitCad's; a record whose mesh body did not come in is a gap.
#[test]
fn a_mesh_body_gets_the_material_and_appearance_its_envelopes_name() {
    let mut source = serde_json::to_value(scan_dump()).unwrap();
    source["_f3d"] = json!({"appearance": mesh_looks()});
    let mut scan = tetrahedron("scan.paramesh", Some(50), Some(10), Some("Scan body"));
    scan.id = Some(70);
    let mut geometry = Meshes::new(vec![
        scan,
        tetrahedron("hidden.paramesh", Some(51), Some(3), None),
    ]);
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(&mut doc, &dump(source), &mut geometry, &Options::default());
    let text = report.text();
    let a = &report.appearances;
    assert_eq!((a.bodies_set, a.bodies), (1, 2), "{text}");
    assert_eq!((a.materials_set, a.materials), (1, 1), "{text}");
    assert_eq!(a.gaps.len(), 1, "{:?}", a.gaps);
    assert!(a.gaps[0].starts_with("mesh body 71:"), "{:?}", a.gaps);
    assert_eq!(a.physical_materials.len(), 1, "{text}");
    assert_eq!(a.physical_materials[0].density, Some(7850.0));
    assert_eq!(a.physical_materials[0].mitcad.as_deref(), Some("steel"));
    assert_eq!(a.appearances.len(), 1, "{text}");
    let uid = doc
        .bodies()
        .iter()
        .find(|b| b.name == "Scan body")
        .map(|b| b.uid)
        .expect("the scan's mesh body");
    let attributes = doc.body_attributes(uid);
    assert_eq!(attributes.material.as_deref(), Some("steel"));
    assert_eq!(attributes.appearance.as_deref(), Some("f3d_steel_satin"));
    let satin = doc.appearance("f3d_steel_satin").unwrap();
    assert_eq!(satin.name, "Steel - satin");
    assert_eq!(satin.base_color, [0.6262; 3]);
    assert_eq!(satin.metalness, 1.0);
}
