// SPDX-License-Identifier: MIT
//! The sheet rules' version flag of the corner relief at flange corners
//! (mitcad#2342): rules added or edited relieve flange corners, rules saved
//! without the flag keep their corners until they are edited, an explicit
//! `false` stays, and only metal folds take it. The corners' geometry is
//! covered by the CLI fixtures `sheet_flange_corner_relief` and
//! `sheet_flange_edges_corner_relief`.

use mitcad_model::features::FeatureDef;
use mitcad_model::features::sheet::SheetRulesDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use serde_json::{Value, json};

fn rules() -> Value {
    json!({"type":"sheet_rules","thickness":1,"bend_radius":1,
        "allowance":{"type":"k_factor","k":0.44},"reference_side":"front",
        "bend_relief":{"type":"rectangular","width":1,"depth":2},
        "corner_relief":{"type":"rectangular","width":1,"depth":2}})
}

fn rules_of(doc: &Document<MockKernel>, uid: FeatureUid) -> SheetRulesDef {
    let FeatureDef::SheetRules(rules) = &doc.feature(uid).unwrap().def else {
        panic!("sheet rules")
    };
    rules.clone()
}

/// `value` without the key `key` in any of its objects.
fn without(value: &mut Value, key: &str) {
    match value {
        Value::Object(map) => {
            map.remove(key);
            for v in map.values_mut() {
                without(v, key);
            }
        }
        Value::Array(items) => {
            for v in items {
                without(v, key);
            }
        }
        _ => {}
    }
}

#[test]
fn rules_added_or_edited_relieve_flange_corners_and_saved_ones_keep_theirs() {
    let mut doc = Document::new(MockKernel::default());
    let uid = doc.add_feature(&def(rules()), None).unwrap().uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    let added = rules_of(&doc, uid);
    assert_eq!(added.flange_corner_relief, Some(true));
    assert!(added.relieves_flange_corners());
    // Saved with the design and read back as it was.
    let saved = doc.to_json();
    assert!(saved.contains("flange_corner_relief"), "{saved}");
    let reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(rules_of(&reopened, uid).flange_corner_relief, Some(true));

    // A design saved before the flag: its rules keep their corners.
    let mut old: Value = serde_json::from_str(&saved).unwrap();
    without(&mut old, "flange_corner_relief");
    let mut old = Document::from_json(&old.to_string(), MockKernel::default()).unwrap();
    old.recompute();
    let kept = rules_of(&old, uid);
    assert_eq!(kept.flange_corner_relief, None);
    assert!(!kept.relieves_flange_corners());
    // ... until its rules are edited; undo brings the old corners back.
    let mut wider = rules();
    wider["corner_relief"]["width"] = json!(4);
    old.edit_feature(uid, &def(wider)).unwrap();
    assert_eq!(rules_of(&old, uid).flange_corner_relief, Some(true));
    old.undo();
    assert_eq!(rules_of(&old, uid).flange_corner_relief, None);

    // An explicit `false` stays, on adding and on editing.
    let mut off = rules();
    off["flange_corner_relief"] = json!(false);
    let other = doc.add_feature(&def(off.clone()), None).unwrap().uid;
    assert_eq!(rules_of(&doc, other).flange_corner_relief, Some(false));
    assert!(!rules_of(&doc, other).relieves_flange_corners());
    doc.edit_feature(other, &def(off)).unwrap();
    assert_eq!(rules_of(&doc, other).flange_corner_relief, Some(false));
}

#[test]
fn only_metal_rules_relieve_flange_corners() {
    let mut doc = Document::new(MockKernel::default());
    let mut metal = rules();
    metal["process"] = json!("metal");
    let metal = doc.add_feature(&def(metal), None).unwrap().uid;
    assert!(rules_of(&doc, metal).relieves_flange_corners());
    // Board folds too, but corner reliefs are metal's.
    let mut board = rules();
    board["process"] = json!("board");
    let board = doc.add_feature(&def(board), None).unwrap().uid;
    let board = rules_of(&doc, board);
    assert_eq!(board.flange_corner_relief, Some(true));
    assert!(!board.relieves_flange_corners());
}
