// SPDX-License-Identifier: MIT
//! The learning dump (mitcad#96).

use super::*;

/// The learning dump (mitcad#96): the item the history settled is written
/// with its candidates and answer in the file's terms.
#[test]
fn the_learning_dump_keeps_what_the_history_chose() {
    let items = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..1].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let small = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": small}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        ..TestGeometry::default()
    };
    let dir = mitcad_model::testing::unique_temp_path("learn");
    let options = Options {
        learn: Some(crate::learn::LearnOptions {
            dir: dir.clone(),
            path: None,
        }),
        ..Options::default()
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    assert_eq!(report.items[1].verified, Some(true), "{}", report.text());
    let settled = std::fs::read_dir(dir.join("settled"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect::<String>();
    let _ = std::fs::remove_dir_all(&dir);
    let lines: Vec<Value> = settled
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["item"], 1);
    assert_eq!(line["type"], "ExtrudeFeature");
    assert_eq!(line["status"], "settled");
    // The sketch's regions by the file's curve ids; the answer names the
    // small rectangle by its index among them.
    let regions = line["context"]["sketches"]["T0"]["regions"]
        .as_array()
        .unwrap();
    assert_eq!(regions.len(), 2);
    let answer = &line["answer"]["defs"][0]["profiles"][0];
    assert_eq!(answer["sketch"], "T0");
    let chosen = &regions[answer["region"].as_u64().unwrap() as usize];
    let mut outer: Vec<&str> = chosen["outer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    outer.sort_unstable();
    assert_eq!(outer, ["c4", "c5", "c6", "c7"]);
    assert!(chosen["inside"].is_array());
    // The candidates in their order, the answer among them.
    let rank = line["accepted"].as_u64().unwrap() as usize;
    assert_eq!(line["candidates"][rank], line["answer"]);
    assert!(line["candidates"].as_array().unwrap().len() >= 2);
    // No file: no raw record.
    assert!(line["record"].is_null());
}

#[test]
fn the_learning_dump_captures_the_dressup_matcher_result() {
    let mut items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (a, b) = shapes(&reference)[0].edges[0].clone();
    let p = edge_point(&a, &b);
    items.push(
        json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
        "detail": {"edgeSets": [{"radius": param("d5", "2 mm", 0.2, "mm"), "edges": [{
            "kind": "edge", "length": 0.1,
            "mid_point": p.map(|x| x / 10.0),
            "_f3d": {"recipe": "edge", "found": "BREP.x.smbh before state 4"}
        }]}]}}),
    );
    let dir = mitcad_model::testing::unique_temp_path("learn-edges");
    let mut doc = Document::new(TestKernel {
        edge_points: true,
        ..TestKernel::default()
    });
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options {
            learn: Some(crate::learn::LearnOptions {
                dir: dir.clone(),
                path: None,
            }),
            ..Options::default()
        },
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let text = std::fs::read_dir(dir.join("unsettled"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect::<String>();
    let _ = std::fs::remove_dir_all(&dir);
    let line = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|line| line["item"] == 2)
        .unwrap();
    let diagnostic = &line["context"]["edge_matches"]["/detail/edgeSets/0/edges/0"];
    assert_eq!(diagnostic["status"], "matched");
    assert_eq!(diagnostic["count"], 1);
    assert_eq!(diagnostic["matches"][0]["body"], "T1.b0");
    assert_eq!(diagnostic["decoder_found"], "BREP.x.smbh before state 4");
    assert!(line.get("translation_error").is_none());
    // The additive field leaves existing readers' resolved input intact.
    assert_eq!(
        line["context"]["resolved"]["/detail/edgeSets/0/edges/0"]["kind"],
        "edge"
    );
}

#[test]
fn the_learning_dump_keeps_a_failed_dressup_without_history_or_candidates() {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
        "detail": {"edgeSets": [{"radius": param("d5", "2 mm", 0.2, "mm"), "edges": [{
            "kind": "edge", "_f3d": {"recipe": "edge",
                "found": "a face of the edge is not in the history state"}
        }]}]}}),
    ];
    let dir = mitcad_model::testing::unique_temp_path("learn-failed-edges");
    let mut doc = Document::new(TestKernel {
        edge_points: true,
        ..TestKernel::default()
    });
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options {
            learn: Some(crate::learn::LearnOptions {
                dir: dir.clone(),
                path: None,
            }),
            ..Options::default()
        },
    );
    assert_ne!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert!(report.text().contains("decoder_unresolved"));
    let text = std::fs::read_dir(dir.join("unsettled"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect::<String>();
    let _ = std::fs::remove_dir_all(&dir);
    let line = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|line| line["item"] == 2)
        .expect("candidate-free dressup must have an unsettled learning record");
    assert_eq!(line["status"], "unsettled");
    assert_eq!(line["candidates"], json!([]));
    assert_eq!(line["cost"]["candidates"], 0);
    assert!(line["accepted"].is_null());
    assert!(line["answer"].is_null());
    let error = line["translation_error"].as_str().unwrap();
    assert!(error.contains("edge set 0, input 0: decoder_unresolved"));
    assert!(error.contains("no history to find them"));
    let pointer = "/detail/edgeSets/0/edges/0";
    let diagnostic = &line["context"]["edge_matches"][pointer];
    assert_eq!(diagnostic["status"], "decoder_unresolved");
    assert_eq!(
        diagnostic["decoder_found"],
        "a face of the edge is not in the history state"
    );
    assert_eq!(diagnostic["count"], 0);
    assert_eq!(diagnostic["candidates"], json!([]));
    assert!(line["context"]["resolved"][pointer].is_null());
    assert!(!line["context"]["bodies"].as_array().unwrap().is_empty());
}
