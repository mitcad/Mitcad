// SPDX-License-Identifier: MIT
//! A milling machine on Estlcam's CNC controller (mitcad#2625, GitHub
//! mitcad/Mitcad#751) on the mock kernel: the controller is offered with
//! the other milling dialects, a simple setup (an outside contour and a
//! hole with a second tool) posts as a `.nc` file of the codes Estlcam
//! reads, in millimetres or inches, and what Estlcam cannot do is refused
//! in words.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The region of a sketch's first rectangle.
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// A 40 x 30 block 10 high, a router on Estlcam with a 6 mm flat end mill
/// (1) and a 5 mm drill (2), and a setup with an outside contour of the
/// block and a hole at (20, 15).
fn on_estlcam() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": {
            "id": "router",
            "machine": {
                "name": "Bench router",
                "controller": "estlcam",
                "travel": {"min": [0, 0, -120], "max": [600, 400, 0]},
                "spindle": {"min_rpm": 8000, "max_rpm": 24000, "spin_up_seconds": 2,
                            "nose_diameter": 50, "nose_length": 80},
                "max_feed": 5000,
                "rapid_feed": 6000,
                "tool_changer": {"kind": "manual"}
            },
            "tools": {"tools": [
                {"number": 1, "name": "6 mm flat end mill", "shape": {"kind": "flat"}, "diameter": 6,
                 "flute_length": 18, "length": 25,
                 "cutting": {"spindle_rpm": 18000, "feed": 1200, "plunge_feed": 300}},
                {"number": 2, "name": "5 mm drill", "shape": {"kind": "drill", "point_angle": 118},
                 "diameter": 5, "flute_length": 15, "length": 25,
                 "cutting": {"spindle_rpm": 10000, "feed": 400, "plunge_feed": 200}}
            ]}
        }}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 1}}}),
    );
    for operation in [
        json!({"kind": {"type": "contour", "side": "outside"}, "tool": 1,
               "inputs": [{"kind": "region", "sketch": "F1", "region": R}]}),
        json!({"kind": {"type": "drill"}, "tool": 2,
               "inputs": [{"kind": "point", "at": {"point": [20, 15, 10]}}]}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

#[test]
fn estlcam_is_a_milling_controller_of_its_own() {
    let doc = on_estlcam();
    let dialects = query(&doc, json!({"query": "cam_dialects"}));
    assert!(
        dialects["dialects"]
            .as_array()
            .unwrap()
            .contains(&json!({"id": "estlcam", "label": "Estlcam", "extension": "nc"})),
        "{dialects}"
    );
}

#[test]
fn a_simple_setup_posts_the_codes_estlcam_reads() {
    let mut doc = on_estlcam();
    let post = query(&doc, json!({"query": "cam_post", "setup": 1}));
    assert_eq!(post["dialect"], json!("estlcam"));
    assert_eq!(post["label"], json!("Estlcam"));
    assert_eq!(post["extension"], json!("nc"));
    assert_eq!(post["units"], json!("millimetres"));
    let text = post["text"].as_str().unwrap();
    assert!(
        text.contains("\n(Units: millimeters, feeds in mm per minute.)\n(Set Estlcam to these units before running it.)\nG21\n"),
        "{text}"
    );
    // The first tool is in the spindle; the second is changed with M06,
    // the spindle on with M03 and off with M05, no dwell, no work offset,
    // no end command.
    assert!(text.contains("(Tool 1: 6 mm flat end mill)\n"), "{text}");
    assert!(text.contains("M03 S18000\n"), "{text}");
    assert!(
        text.contains("M05\n(Change to tool 2: 5 mm drill)\nM06\n"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("G2 ") || l.starts_with("G3 ")),
        "{text}"
    );
    for code in ["G4", "G43", "G53", "G54", "G90", "M30", "M2", "M3 ", "M6\n"] {
        assert!(!text.contains(code), "{code}: {text}");
    }
    // One G code a line, the motion on every move line.
    for l in text.lines().filter(|l| !l.starts_with('(')) {
        let words: Vec<&str> = l.split_whitespace().collect();
        assert!(
            words.iter().filter(|w| w.starts_with('G')).count() <= 1,
            "{l}"
        );
        if words.iter().any(|w| w.starts_with(['X', 'Y', 'Z'])) {
            assert!(words[0].starts_with('G'), "{l}");
        }
    }

    let inches = query(
        &doc,
        json!({"query": "cam_post", "setup": 1, "units": "inches"}),
    );
    let text = inches["text"].as_str().unwrap();
    assert!(
        text.contains("\n(Units: inches, feeds in inches per minute.)\n(Set Estlcam to these units before running it.)\nG20\n"),
        "{text}"
    );

    // Estlcam has one work zero: another work offset is refused in words.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"work_offset": 2}}),
    );
    let error = doc
        .query(&json!({"query": "cam_post", "setup": 1}).to_string())
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        "Estlcam has one work zero, set in its controller; choose G54 as the setup's work offset."
    );
}
