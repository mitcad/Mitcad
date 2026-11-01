// SPDX-License-Identifier: MIT
//! Body ids of a split's pieces after upstream edits (mitcad#408), with the
//! mock kernel: its cuts leave pieces with the bounding boxes a test gives,
//! in that (geometric) order, and it measures them by their boxes.

use serde_json::{Value, json};

use crate::features::SketchPlane;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::BoundingBox;
use crate::test_support::{Block, block, def, extrude, num};
use crate::testing::MockKernel;
use crate::{Document, FeatureStatus};

fn bx(min: [f64; 3], max: [f64; 3]) -> BoundingBox {
    BoundingBox { min, max }
}

/// The body's main piece, moved `dx` along x.
fn main_piece(dx: f64) -> BoundingBox {
    bx([dx, 0.0, 0.0], [50.0 + dx, 40.0, 20.0])
}

/// Two slivers beside it.
fn sliver_a(dx: f64) -> BoundingBox {
    bx([54.0 + dx, 0.0, 0.0], [55.0 + dx, 1.0, 20.0])
}

fn sliver_b(dx: f64) -> BoundingBox {
    bx([57.0 + dx, 39.0, 0.0], [58.0 + dx, 40.0, 20.0])
}

/// The main piece in five flights along x.
fn flights() -> Vec<BoundingBox> {
    (0..5)
        .map(|i| {
            bx(
                [10.0 * i as f64, 0.0, 0.0],
                [10.0 * (i + 1) as f64, 40.0, 20.0],
            )
        })
        .collect()
}

fn pieces(doc: &Document<MockKernel>, boxes: Vec<BoundingBox>) {
    doc.kernel().piece_boxes.replace(Some(boxes));
}

/// Which piece of the cut (its place in geometric order, `#k` of the
/// mock's history) the body holds.
fn piece_of(doc: &Document<MockKernel>, body: BodyUid) -> Option<usize> {
    let history = &doc.body_shape(body)?.history;
    let at = history.find('#')? + 1;
    let digits: String = history[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn uids(doc: &Document<MockKernel>) -> Vec<BodyUid> {
    doc.bodies().iter().map(|b| b.uid).collect()
}

struct Split {
    b: Block,
    /// Extrude2, cutting the block (F2.b0).
    cut: FeatureUid,
    /// RemoveBody1, removing the cut's slivers.
    remove: FeatureUid,
    /// Extrude3, joining a boss to the block: it names F2.b0.
    join: FeatureUid,
}

impl Split {
    fn piece(&self, index: u32) -> BodyUid {
        BodyUid::new(self.cut, index)
    }
}

/// The block (F2.b0) cut by a circle's extrusion into `boxes`, the slivers
/// (the cut's F<cut>.b0 and .b1) removed, and a boss joined to the block.
fn split(boxes: Vec<BoundingBox>) -> Split {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    pieces(&b.doc, boxes);
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [30.0, 20.0], &num(10.0))
        .unwrap()
        .region;
    let cut = b
        .doc
        .add_feature(&extrude(sketch, &[&region], 30.0, "cut", &[b.body]), None)
        .unwrap()
        .uid;
    let slivers = [BodyUid::new(cut, 0), BodyUid::new(cut, 1)];
    let remove = b
        .doc
        .add_feature(
            &def(json!({"type": "remove_body", "bodies": slivers})),
            None,
        )
        .unwrap()
        .uid;
    let join = b
        .doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "join", &[b.body]), None)
        .unwrap()
        .uid;
    Split {
        b,
        cut,
        remove,
        join,
    }
}

/// Every feature built, with no warnings.
fn clean(s: &Split) {
    for uid in [s.cut, s.remove, s.join] {
        assert_eq!(s.b.doc.status(uid), Some(&FeatureStatus::Ok), "{uid}");
        assert!(
            s.b.doc.warnings(uid).is_empty(),
            "{uid}: {:?}",
            s.b.doc.warnings(uid)
        );
    }
}

#[test]
fn the_first_recompute_keeps_the_geometric_order() {
    // The first piece in geometric order keeps the body, the others become
    // the cut's bodies in order, as before pieces were remembered.
    let s = split(vec![sliver_b(0.0), main_piece(0.0), sliver_a(0.0)]);
    let doc = &s.b.doc;
    assert_eq!(uids(doc), vec![s.b.body]);
    assert_eq!(piece_of(doc, s.b.body), Some(0));
    clean(&s);

    // So does the first recompute of a design saved without what the cut
    // remembers; the recompute takes it, and a save writes it.
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    for feature in file["features"].as_array_mut().unwrap() {
        feature.as_object_mut().unwrap().remove("split_pieces");
    }
    let kernel = MockKernel::default();
    kernel.measures.set(true);
    kernel
        .piece_boxes
        .replace(Some(vec![sliver_b(0.0), main_piece(0.0), sliver_a(0.0)]));
    let mut opened = Document::from_json(&file.to_string(), kernel).unwrap();
    opened.recompute();
    assert_eq!(piece_of(&opened, s.b.body), Some(0));
    assert_eq!(opened.to_json(), doc.to_json());
}

#[test]
fn later_features_keep_their_pieces_when_the_order_changes() {
    let mut s = split(vec![main_piece(0.0), sliver_a(0.0), sliver_b(0.0)]);
    assert_eq!(uids(&s.b.doc), vec![s.b.body]);
    assert_eq!(piece_of(&s.b.doc, s.b.body), Some(0));
    clean(&s);

    // An upstream edit moves the pieces a little and brings them in another
    // order: by geometric order the block would be sliver B and Remove Body
    // would remove the main piece.
    pieces(
        &s.b.doc,
        vec![sliver_b(1.0), main_piece(1.0), sliver_a(1.0)],
    );
    s.b.doc.set_parameter("d3", 25.0).unwrap();
    assert_eq!(uids(&s.b.doc), vec![s.b.body]);
    assert_eq!(piece_of(&s.b.doc, s.b.body), Some(1));
    clean(&s);

    // The memory the document took fits the result: nothing is evaluated
    // again for it.
    s.b.doc.recompute();
    assert!(s.b.doc.stats().evaluated.is_empty());

    // Saved and opened again, the pieces keep their ids (the file has what
    // the cut remembers); a design opened without it would give the block
    // the first piece.
    let json = s.b.doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    let saved = file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == s.cut.to_string())
        .unwrap()["split_pieces"]
        .clone();
    assert_eq!(saved[0]["body"], "F2.b0");
    let bodies: Vec<&str> = saved[0]["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, ["F2.b0", "F4.b0", "F4.b1"]);
    let kernel = MockKernel::default();
    kernel.measures.set(true);
    kernel
        .piece_boxes
        .replace(Some(vec![sliver_b(1.0), main_piece(1.0), sliver_a(1.0)]));
    let mut opened = Document::from_json(&json, kernel).unwrap();
    opened.recompute();
    assert_eq!(uids(&opened), vec![s.b.body]);
    assert_eq!(piece_of(&opened, s.b.body), Some(1));
    assert_eq!(opened.to_json(), json);
}

#[test]
fn pieces_no_longer_made_warn_instead_of_taking_others() {
    let mut s = split(vec![main_piece(0.0), sliver_a(0.0), sliver_b(0.0)]);
    // An upstream edit makes the cut leave five flights and no slivers: no
    // piece matches what the cut remembers.
    pieces(&s.b.doc, flights());
    s.b.doc.set_parameter("d3", 30.0).unwrap();
    // The block takes the flight closest to its main piece (the middle one)
    // and the features that name it warn; the slivers are gone, so Remove
    // Body removes nothing, with a warning, and no flight takes their ids.
    assert_eq!(piece_of(&s.b.doc, s.b.body), Some(2));
    let others: Vec<(BodyUid, Option<usize>)> = [2, 3, 4, 5]
        .iter()
        .map(|i| (s.piece(*i), piece_of(&s.b.doc, s.piece(*i))))
        .collect();
    assert_eq!(
        others,
        vec![
            (s.piece(2), Some(0)),
            (s.piece(3), Some(1)),
            (s.piece(4), Some(3)),
            (s.piece(5), Some(4)),
        ]
    );
    assert_eq!(uids(&s.b.doc).len(), 5);
    assert_eq!(s.b.doc.status(s.remove), Some(&FeatureStatus::Ok));
    assert_eq!(
        s.b.doc.warnings(s.remove),
        [
            "body F4.b0 is a piece Extrude2 no longer makes (its pieces changed): nothing to \
             remove",
            "body F4.b1 is a piece Extrude2 no longer makes (its pieces changed): nothing to \
             remove",
        ]
    );
    assert_eq!(
        s.b.doc.warnings(s.join),
        [
            "body F2.b0 is now another piece of what Extrude2 split it into (volume 40000 → \
          8000 mm³, center moved 0 mm)"
        ]
    );
    assert!(s.b.doc.warnings(s.cut).is_empty());

    // Another feature that names a piece no longer made fails saying so.
    let combine =
        s.b.doc
            .add_feature(
                &def(
                    json!({"type": "combine", "target": s.piece(1), "tools": [s.piece(2)],
                        "operation": "join"}),
                ),
                None,
            )
            .unwrap()
            .uid;
    assert_eq!(
        s.b.doc.status(combine),
        Some(&FeatureStatus::Failed(
            "body F4.b1 is a piece Extrude2 no longer makes (its pieces changed)".to_owned()
        ))
    );
    s.b.doc.undo();

    // When the slivers come back they take their ids again, and Remove Body
    // removes them; the block is another piece again (the main one).
    pieces(
        &s.b.doc,
        vec![main_piece(0.0), sliver_a(0.0), sliver_b(0.0)],
    );
    s.b.doc.set_parameter("d3", 20.0).unwrap();
    assert_eq!(piece_of(&s.b.doc, s.b.body), Some(0));
    assert_eq!(uids(&s.b.doc), vec![s.b.body]);
    assert!(s.b.doc.warnings(s.remove).is_empty());
    assert_eq!(
        s.b.doc.warnings(s.join),
        [
            "body F2.b0 is now another piece of what Extrude2 split it into (volume 8000 → \
          40000 mm³, center moved 0 mm)"
        ]
    );

    // Undo gives the flights back as they were, from the cache.
    s.b.doc.undo();
    assert!(!s.b.doc.stats().evaluated.contains(&s.cut));
    assert_eq!(piece_of(&s.b.doc, s.b.body), Some(2));
    assert_eq!(uids(&s.b.doc).len(), 5);
}

#[test]
fn a_combine_cut_keeps_its_pieces() {
    let mut b = block();
    b.doc.kernel().measures.set(true);
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [30.0, 20.0], &num(10.0))
        .unwrap()
        .region;
    let tool = b
        .doc
        .add_feature(&extrude(sketch, &[&region], 30.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    pieces(&b.doc, vec![main_piece(0.0), sliver_a(0.0), sliver_b(0.0)]);
    let combine = b
        .doc
        .add_feature(
            &def(
                json!({"type": "combine", "target": b.body, "tools": [BodyUid::new(tool, 0)],
                        "operation": "cut"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let sliver = BodyUid::new(combine, 1);
    assert_eq!(piece_of(&b.doc, sliver), Some(2));
    pieces(&b.doc, vec![sliver_b(1.0), sliver_a(1.0), main_piece(1.0)]);
    b.doc.set_parameter("d3", 25.0).unwrap();
    assert_eq!(piece_of(&b.doc, b.body), Some(2));
    assert_eq!(piece_of(&b.doc, BodyUid::new(combine, 0)), Some(1));
    assert_eq!(piece_of(&b.doc, sliver), Some(0));
    assert!(b.doc.warnings(combine).is_empty());
}
