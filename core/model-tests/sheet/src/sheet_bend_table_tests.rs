// SPDX-License-Identifier: MIT

use mitcad_model::sheet::BendId;
use mitcad_model::sheet::bend_table::BendTable;
use mitcad_model::sheet::flat::{FlatBend, FlatLine};
use mitcad_model::testing::{MockKernel, unique_temp_path};
use mitcad_model::{Document, ids::FeatureUid};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn bend(feature: u64, x: f64, angle: f64, direction: &'static str) -> FlatBend {
    FlatBend {
        bend: BendId {
            feature: FeatureUid(feature),
            slot: 0,
        },
        angle,
        radius: 3.0,
        k_factor: 0.3,
        allowance: angle.abs() * 3.6,
        direction,
        centerlines: vec![FlatLine {
            start: [x, 0.0],
            end: [x, 20.0],
        }],
        tangents: vec![],
        slits: vec![],
    }
}

#[test]
fn bend_table_keeps_flat_pattern_values_and_every_piece() {
    let angle = std::f64::consts::FRAC_PI_2;
    let a = bend(4, 2.827_433_388, angle, "down");
    let l = BendTable::from_bends("Bracket", "Steel", 2.0, std::slice::from_ref(&a));
    assert_eq!(l.bends, 1);
    assert_eq!(l.rows.len(), 1);
    let mut b = bend(5, 47.827_433_388, -angle, "up");
    b.centerlines[0].end[1] = 8.0;
    b.centerlines.push(FlatLine {
        start: [47.827_433_388, 12.0],
        end: [47.827_433_388, 20.0],
    });
    let flat = vec![a, b];
    let u = BendTable::from_bends("Channel", "Steel", 2.0, &flat);
    let round_trip: Value = serde_json::from_str(&serde_json::to_string(&u).unwrap()).unwrap();
    assert_eq!(round_trip["bends"], 2);
    let rows = round_trip["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    let flat_json = serde_json::to_value(&flat).unwrap();
    for (row, (bend, piece)) in rows.iter().zip([(0, 0), (1, 0), (1, 1)]) {
        let expected = &flat_json[bend];
        let line = &expected["centerlines"][piece];
        assert_eq!(row["bend"], expected["bend"]);
        assert_eq!(row["direction"], expected["direction"]);
        for (key, value) in [
            (
                "angle_deg",
                expected["angle"].as_f64().unwrap().to_degrees(),
            ),
            ("radius_mm", expected["radius"].as_f64().unwrap()),
            ("k_factor", expected["k_factor"].as_f64().unwrap()),
            ("allowance_mm", expected["allowance"].as_f64().unwrap()),
            ("start_x_mm", line["start"][0].as_f64().unwrap()),
            ("start_y_mm", line["start"][1].as_f64().unwrap()),
            ("end_x_mm", line["end"][0].as_f64().unwrap()),
            ("end_y_mm", line["end"][1].as_f64().unwrap()),
        ] {
            assert!((row[key].as_f64().unwrap() - value).abs() < 1e-6, "{key}");
        }
    }
    assert_eq!(rows[1]["length_mm"], 8.0);
    assert_eq!(rows[1]["included_angle_deg"], 90.0);
}

#[test]
fn csv_quotes_names_and_guards_spreadsheet_formulas() {
    let bends = [bend(4, 3.0, std::f64::consts::FRAC_PI_2, "up")];
    for (name, expected) in [
        ("Bracket, \"wide\"", "\"Bracket, \"\"wide\"\"\""),
        ("=1+1", "\"'=1+1\""),
        ("  +1", "\"'  +1\""),
        ("\tname", "\"'\tname\""),
    ] {
        let table = BendTable::from_bends(name, "@shop", 2.0, &bends);
        let csv = table.csv();
        assert!(
            csv.split("\r\n")
                .nth(1)
                .unwrap()
                .starts_with(&format!("{expected},\"'@shop\",2,0.3,\"F4:0\",90,\"up\""))
        );
    }
}

#[test]
fn export_bend_table_refuses_a_missing_sheet_before_writing() {
    let mut doc = Document::new(MockKernel::default());
    let path = unique_temp_path("refused-bend-table").with_extension("csv");
    let error = doc
        .command(&json!({"cmd":"export_bend_table","body":"F99.b0","path":path}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("this body is not a sheet-metal body"));
    assert!(!path.exists());
}
