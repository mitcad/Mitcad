// SPDX-License-Identifier: MIT
//! Cell Grid (mitcad#2238) in documents with the mock kernel: one sweep of
//! all the cells and one cut, the cells' faces named by their lattice
//! index, the count in the panel's remark, trimmed cells within the
//! field's prism, a fillet on a cell's edge through edits, and the
//! refusals in words.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::features::cell_grid::cell_curve;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{def, extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, SegmentKey};
use mitcad_model::{Document, FeatureStatus, ModelError};

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn faces(doc: &Document<MockKernel>, body: BodyUid) -> Vec<String> {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn calls(doc: &Document<MockKernel>, name: &str) -> usize {
    doc.kernel().calls.borrow().get(name).copied().unwrap_or(0)
}

fn sweeps_of(doc: &Document<MockKernel>, feature: FeatureUid) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(&format!("extrude {feature} ")))
        .cloned()
        .collect()
}

/// A plate: Sketch1 (F1) with a 100 x 60 rectangle about the origin (c1..c4;
/// d1, d2) extruded 10 (d3) by Extrude1 (F2) into F2.b0; Sketch2 (F3) on
/// the same plane with the field, the same rectangle (c1..c4; d4, d5).
struct Plate {
    doc: Document<MockKernel>,
    body: BodyUid,
    field: FeatureUid,
    region: String,
}

fn plate() -> Plate {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [-50.0, -30.0], &num(100.0), &num(60.0))
        .unwrap()
        .region;
    let extruded = doc
        .add_feature(&extrude(sketch, &[&region], 10.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    let field = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(field, [-50.0, -30.0], &num(100.0), &num(60.0))
        .unwrap()
        .region
        .to_string();
    Plate {
        doc,
        body: BodyUid::new(extruded, 0),
        field,
        region,
    }
}

impl Plate {
    fn grid(&self, extra: Value) -> Value {
        let mut value = json!({"type": "cell_grid",
            "profiles": [{"sketch": self.field, "region": self.region}],
            "size": 8, "web": 2, "depth": 4, "direction": "positive"});
        if let (Some(value), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            for (key, v) in extra {
                value.insert(key.clone(), v.clone());
            }
        }
        value
    }

    /// Adds the grid with `extra`.
    fn add(&mut self, extra: Value) -> Result<FeatureUid, ModelError> {
        let grid = self.grid(extra);
        add(&mut self.doc, grid)
    }
}

/// Side `side` of cell `index` of feature `feature`, with its neighbours
/// `before` and `after` around a hexagon.
fn hexagon_side(feature: FeatureUid, index: [i64; 2], side: u32) -> FaceName {
    let curve = |k: u32| cell_curve(index, k % 6);
    FaceName::side(
        feature,
        SegmentKey::between(curve(side), curve(side + 5), curve(side + 1)),
    )
}

#[test]
fn the_cells_are_swept_once_and_cut_once() {
    let mut p = plate();
    let cuts = calls(&p.doc, "cut");
    let grid = p.add(json!({})).unwrap();
    assert_eq!(status(&p.doc, grid), FeatureStatus::Ok);
    assert_eq!(p.doc.feature(grid).unwrap().name, "CellGrid1");
    // One sweep of all 61 cells up the sketch normal, one cut.
    assert_eq!(
        sweeps_of(&p.doc, grid),
        vec![format!(
            "extrude {grid} along [0.0, 0.0, 1.0] from offset 0: distance 4 taper 0"
        )]
    );
    assert_eq!(calls(&p.doc, "cut"), cuts + 1);
    assert_eq!(
        p.doc.remarks(grid)[0].0,
        "61 cells, open area 3380.96 mm² (56.3% of the field)"
    );
    // The body keeps its id and has the cells' faces, named by their
    // lattice index.
    let names = faces(&p.doc, p.body);
    for (index, side) in [([0, 0], 0), ([0, 0], 3), ([5, -1], 1), ([-4, 2], 5)] {
        let face = hexagon_side(grid, index, side).to_string();
        assert!(names.contains(&face), "{face} in {names:?}");
    }
    // Cell (6, 0) lies across the edge: not cut whole.
    let outside = hexagon_side(grid, [6, 0], 0).to_string();
    assert!(!names.contains(&outside));
}

#[test]
fn the_other_way_symmetric_and_through_all() {
    let mut p = plate();
    let grid = p.add(json!({"direction": "negative"})).unwrap();
    assert_eq!(status(&p.doc, grid), FeatureStatus::Ok);
    assert_eq!(
        sweeps_of(&p.doc, grid).last().unwrap(),
        &format!("extrude {grid} along [-0.0, -0.0, -1.0] from offset 0: distance 4 taper 0")
    );
    p.doc
        .edit_feature(grid, &def(p.grid(json!({"direction": "symmetric"}))))
        .unwrap();
    assert_eq!(
        sweeps_of(&p.doc, grid).last().unwrap(),
        &format!(
            "extrude {grid} along [0.0, 0.0, 1.0] from offset 0: distance 4 taper 0; distance 4 \
             taper 0"
        )
    );
    // Through all: past the plate's far side from the cells' box.
    let mut through = p.grid(json!({"through_all": true}));
    through.as_object_mut().unwrap().remove("depth");
    p.doc.edit_feature(grid, &def(through)).unwrap();
    assert_eq!(status(&p.doc, grid), FeatureStatus::Ok);
    let last = sweeps_of(&p.doc, grid).last().unwrap().clone();
    assert!(last.contains("along [0.0, 0.0, 1.0]"), "{last}");
    assert!(!last.contains("distance 4 "), "{last}");
}

#[test]
fn snapped_and_trimmed_edge_cells() {
    let mut p = plate();
    let grid = p.add(json!({"clip": "snap"})).unwrap();
    assert_eq!(status(&p.doc, grid), FeatureStatus::Ok);
    assert!(
        p.doc.remarks(grid)[0]
            .0
            .starts_with("81 cells (61 whole, 20 snapped), open area "),
        "{:?}",
        p.doc.remarks(grid)
    );
    // A half's cut is a side of its own, numbered 100.
    let names = faces(&p.doc, p.body);
    let chord = cell_curve([0, 3], 100).0;
    assert!(
        names.iter().any(|n| n.contains(&format!("side(c{chord}["))),
        "{names:?}"
    );

    // Trimmed: the cells within the field's prism, then one cut.
    let intersections = calls(&p.doc, "intersect");
    p.doc
        .edit_feature(grid, &def(p.grid(json!({"clip": "free"}))))
        .unwrap();
    assert_eq!(status(&p.doc, grid), FeatureStatus::Ok);
    assert!(
        p.doc.remarks(grid)[0]
            .0
            .starts_with("85 cells (61 whole, 24 trimmed), open area "),
        "{:?}",
        p.doc.remarks(grid)
    );
    assert_eq!(calls(&p.doc, "intersect"), intersections + 1);
    let history = p.doc.body_shape(p.body).unwrap().history.clone();
    assert!(
        history.starts_with("cut(") && history.contains("common("),
        "{history}"
    );
}

#[test]
fn a_fillet_on_a_cells_edge_survives_a_change_of_the_plate() {
    let mut p = plate();
    let grid = p.add(json!({})).unwrap();
    let edge = EdgeName::new(hexagon_side(grid, [0, 0], 0), hexagon_side(grid, [0, 0], 1));
    let fillet = add(
        &mut p.doc,
        json!({"type": "fillet", "body": p.body, "edges": [edge.to_string()], "radius": 0.5}),
    )
    .unwrap();
    assert_eq!(status(&p.doc, fillet), FeatureStatus::Ok);
    // The plate wider, and the field narrower: cell (0, 0) stays.
    p.doc.set_parameter("d1", 120.0).unwrap();
    p.doc.set_parameter("d4", 80.0).unwrap();
    assert_eq!(status(&p.doc, grid), FeatureStatus::Ok);
    assert_eq!(status(&p.doc, fillet), FeatureStatus::Ok);
    let fewer = &p.doc.remarks(grid)[0].0;
    assert!(!fewer.starts_with("61 cells"), "{fewer}");
    let names = faces(&p.doc, p.body);
    let rounded = format!("{fillet}:fillet({edge})");
    assert!(names.contains(&rounded), "{rounded} in {names:?}");
}

#[test]
fn a_target_the_cells_do_not_reach_is_a_warning() {
    let mut p = plate();
    let other = add(
        &mut p.doc,
        json!({"type": "box", "plane": "xy", "corner": [200, 0], "length": 10, "width": 10,
               "height": 10, "operation": "new_body"}),
    )
    .unwrap();
    let other = BodyUid::new(other, 0);
    *p.doc.kernel().touch.borrow_mut() = Some(vec![true, false]);
    let grid = p.add(json!({"targets": [p.body, other]})).unwrap();
    assert_eq!(status(&p.doc, grid), FeatureStatus::Ok);
    assert_eq!(
        p.doc.warnings(grid),
        [format!("the cells do not reach body {other}")]
    );
    *p.doc.kernel().touch.borrow_mut() = None;
}

#[test]
fn the_refusals_are_in_words() {
    let mut p = plate();
    // Values a new grid must not start with.
    let web = p.add(json!({"web": 0})).unwrap_err();
    assert!(
        web.to_string()
            .contains("the web between cells must be greater than zero, got 0"),
        "{web}"
    );
    let depth = p.add(json!({"depth": -1})).unwrap_err();
    assert!(
        depth
            .to_string()
            .contains("depth must be greater than zero"),
        "{depth}"
    );
    let sides = p
        .add(json!({"shape": "polygon", "sides": 2.5}))
        .unwrap_err();
    assert!(sides.to_string().contains("3 to 64 sides"), "{sides}");
    // Cells too large for the field: none fits.
    let grid = p.add(json!({"size": 70})).unwrap();
    assert!(
        error(&p.doc, grid).starts_with("no cell of size 70 mm fits in the field"),
        "{}",
        error(&p.doc, grid)
    );
    // Too small for the field: more cells than one grid cuts, counted.
    p.doc
        .edit_feature(grid, &def(p.grid(json!({"size": 4, "web": 1}))))
        .unwrap();
    assert_eq!(
        error(&p.doc, grid),
        "the grid has 265 cells, more than the 250 one Cell Grid cuts: use larger cells or a \
         smaller field"
    );
    // Profiles of two sketches.
    let elsewhere = p.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = p
        .doc
        .add_circle(elsewhere, [0.0, 0.0], &num(10.0))
        .unwrap()
        .region;
    let mut two = p.grid(json!({}));
    two["profiles"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sketch": elsewhere, "region": region}));
    let mixed = add(&mut p.doc, two).unwrap_err();
    assert!(mixed.to_string().contains("one sketch"), "{mixed}");
}
