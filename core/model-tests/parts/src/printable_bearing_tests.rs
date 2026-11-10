// SPDX-License-Identifier: MIT
//! Printable bearings (mitcad#577) with the mock kernel: standard sizes and
//! fitting ones, the element count, the section and its values, every
//! printability check, both operations and the component's name, the
//! groove cut into a body, the mount frames and the airsoft balls in the
//! parts list.

use serde_json::{Value, json};

use mitcad_model::features::bearing_sizes::{self, Fit};
use mitcad_model::features::printable_bearing::{
    Element, GrooveFace, Layout, PrintableBearingDef, groove_section, outer_race_section,
};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0e-6
}

fn get(values: &Value, key: &str) -> f64 {
    values[key].as_f64().unwrap_or(f64::NAN)
}

fn rejected(doc: &mut Document<MockKernel>, value: Value) -> String {
    match add(doc, value) {
        Err(error) => error.to_string(),
        Ok(uid) => match status(doc, uid) {
            FeatureStatus::Failed(message) => message.agent_text().to_owned(),
            other => panic!("expected a failure, got {other:?}"),
        },
    }
}

/// A bearing on XY with `extra` fields.
fn bearing(extra: Value) -> Value {
    let mut value = json!({"type": "printable_bearing", "size": "608", "plane": "xy",
                           "operation": "new_body"});
    for (k, v) in extra.as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}

/// The values query of a definition (without `type`).
fn values_of(doc: &Document<MockKernel>, extra: Value) -> Value {
    let mut def = bearing(extra);
    def.as_object_mut().unwrap().remove("type");
    query(doc, json!({"query": "bearing_values", "def": def}))
}

fn layout_of(value: Value) -> Layout {
    let def: PrintableBearingDef<f64> = serde_json::from_value(value).unwrap();
    def.layout(None).unwrap()
}

fn specs(doc: &Document<MockKernel>, prefix: &str) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with(prefix))
        .cloned()
        .collect()
}

fn body(doc: &Document<MockKernel>, uid: &str) -> Option<mitcad_model::testing::MockShape> {
    let uid: BodyUid = uid.parse().unwrap();
    doc.body_shape(uid).cloned()
}

#[test]
fn standard_sizes_and_the_sizes_that_fit() {
    let size = bearing_sizes::lookup("608").unwrap();
    assert_eq!(
        (size.bore, size.outside_diameter, size.width),
        (8.0, 22.0, 7.0)
    );
    assert_eq!(size.flange, None);
    // Case and spaces do not matter; an F size has its flange.
    let flanged = bearing_sizes::lookup(" f608 ").unwrap();
    assert_eq!(flanged.designation, "F608");
    assert_eq!(flanged.flange, Some((25.0, 1.5)));
    // The inch series, converted: R8 is 1/2 x 1 1/8 x 1/4 inch.
    let r8 = bearing_sizes::lookup("R8").unwrap();
    assert!(near(r8.bore, 12.7) && near(r8.outside_diameter, 28.575) && near(r8.width, 6.35));
    let mr = bearing_sizes::lookup("MR105").unwrap();
    assert_eq!((mr.bore, mr.outside_diameter, mr.width), (5.0, 10.0, 3.0));
    for (designation, d, od, b) in [
        ("6202", 15.0, 35.0, 11.0),
        ("6305", 25.0, 62.0, 17.0),
        ("6804", 20.0, 32.0, 7.0),
        ("6901", 12.0, 24.0, 6.0),
        ("16004", 20.0, 42.0, 8.0),
        ("625", 5.0, 16.0, 5.0),
    ] {
        let s = bearing_sizes::lookup(designation).unwrap();
        assert_eq!(
            (s.bore, s.outside_diameter, s.width),
            (d, od, b),
            "{designation}"
        );
    }
    assert!(bearing_sizes::size_of("609X").is_err());
    // Every designation once, every size a ring.
    for (i, s) in bearing_sizes::SIZES.iter().enumerate() {
        assert!(s.bore < s.outside_diameter && s.width > 0.0, "{s:?}");
        assert!(
            bearing_sizes::SIZES[i + 1..]
                .iter()
                .all(|o| o.designation != s.designation),
            "{} twice",
            s.designation
        );
    }

    // An 8 mm shaft: every size with a bore within 0.5 mm, the nearest
    // first: the 8 mm bores in the table's order (608 first), then the
    // inch R1810 (5/16 inch, 7.9375 mm).
    let fits = bearing_sizes::fitting(Fit::Bore, 8.0, 0.5);
    assert_eq!(fits[0].designation, "608");
    assert!(fits.iter().all(|s| (s.bore - 8.0).abs() <= 0.5 + 1.0e-9));
    for pair in fits.windows(2) {
        assert!((pair[0].bore - 8.0).abs() <= (pair[1].bore - 8.0).abs() + 1.0e-12);
    }
    assert!(fits.iter().any(|s| s.designation == "688"));
    assert_eq!(fits.last().unwrap().designation, "R1810", "{fits:?}");
    // A 12.6 mm shaft takes R8 (12.7) within 0.5 mm, not the 12 mm bores.
    let fits = bearing_sizes::fitting(Fit::Bore, 12.6, 0.5);
    assert_eq!(fits.len(), 1, "{fits:?}");
    assert_eq!(fits[0].designation, "R8");
    // A 21.9 mm hole: the 22 mm outsides (608 first), then the inch R6
    // (7/8 inch, 22.225 mm); a 22.2 mm hole takes the R6 first.
    let fits = bearing_sizes::fitting(Fit::Outside, 21.9, 0.5);
    assert_eq!(fits[0].designation, "608");
    let (last, metric) = fits.split_last().unwrap();
    assert!(
        metric.iter().all(|s| s.outside_diameter == 22.0),
        "{fits:?}"
    );
    assert_eq!(last.designation, "R6", "{fits:?}");
    let fits = bearing_sizes::fitting(Fit::Outside, 22.2, 0.5);
    assert_eq!(fits[0].designation, "R6", "{fits:?}");
    // Nothing within 0.1 mm of a 23 mm hole.
    assert!(bearing_sizes::fitting(Fit::Outside, 23.0, 0.1).is_empty());

    // The query.
    let doc = Document::new(MockKernel::default());
    let sizes = query(
        &doc,
        json!({"query": "bearing_sizes", "fit": "outside", "diameter": 21.9}),
    );
    assert_eq!(sizes["sizes"][0]["designation"], json!("608"));
    assert_eq!(sizes["sizes"][0]["width"], json!(7.0));
    let all = query(&doc, json!({"query": "bearing_sizes", "series": "f"}));
    let all = all["sizes"].as_array().unwrap();
    assert!(all.iter().all(|s| s["series"] == json!("F")));
    assert_eq!(
        all.iter()
            .find(|s| s["designation"] == json!("F608"))
            .unwrap()["flange"],
        json!({"diameter": 25.0, "thickness": 1.5})
    );
}

#[test]
fn a_608_dumbbell_bearing_its_section_and_its_count() {
    let mut doc = Document::new(MockKernel::default());
    // 8 x 22 x 7: a 7 mm section, the walls 1.2 mm (the smallest wall,
    // above a sixth of the section), the bands 0.4 mm from the elements:
    // dumbbells of 7 - 2 x 1.2 - 2 x 0.4 = 3.8 mm on the 15 mm pitch
    // circle, their waist 3.8 / 4 = 0.95 deep. 2 asin((3.8 + 0.6) / 15) =
    // 0.5954 rad apart: 10 of them.
    let values = values_of(&doc, json!({}));
    assert!(values.get("error").is_none(), "{values}");
    assert!(near(get(&values, "element_diameter"), 3.8), "{values}");
    assert!(near(get(&values, "pitch_diameter"), 15.0));
    assert_eq!(values["element_count"], json!(10));
    assert_eq!(values["max_elements"], json!(10));
    assert!(near(get(&values, "inner_wall"), 1.2));
    assert!(near(get(&values, "outer_wall"), 1.2));
    assert!(near(get(&values, "waist_diameter"), 1.9));
    // The flanks' normal gap 0.4 / sqrt(2) is the least.
    assert!(near(get(&values, "min_gap"), 0.4 / 2f64.sqrt()));
    // The ridge reaches 0.95 - 0.4 into the waist; 45 degrees at most.
    assert!(near(get(&values, "hold"), 0.55));
    assert!(near(
        get(&values, "max_overhang"),
        std::f64::consts::FRAC_PI_4
    ));
    assert_eq!(values["designation"], json!("608"));

    // The section: the bands 1.9 + 0.4 from the pitch circle, the ridge's
    // flanks from z = 3.5 - 0.95 (the waist's flank moved 0.4 along the
    // axis, met by the band) to its apex 1.9 - 0.95 + 0.4 at the middle.
    let layout = layout_of(json!({"element": "dumbbell", "size": "608"}));
    let race = &layout.race;
    assert_eq!(race.len(), 5, "{race:?}");
    for (point, (rho, z)) in race.iter().zip([
        (2.3, 0.0),
        (2.3, 2.55),
        (1.35, 3.5),
        (2.3, 4.45),
        (2.3, 7.0),
    ]) {
        assert!(near(point[0], rho) && near(point[1], z), "{race:?}");
    }

    let uid = add(&mut doc, bearing(json!({}))).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(doc.feature(uid).unwrap().name, "Bearing1");
    // The races and the first element turned, the other nine copies.
    let revolves = specs(&doc, "revolve F1 ");
    assert_eq!(revolves.len(), 3, "{revolves:?}");
    // Inner race, outer race and ten elements.
    for k in 0..12 {
        assert!(body(&doc, &format!("F1.b{k}")).is_some(), "body {k}");
    }
    assert!(body(&doc, "F1.b12").is_none());
    let inner = body(&doc, "F1.b0").unwrap();
    assert!(
        inner
            .faces
            .iter()
            .any(|f| f.to_string().starts_with("F1:side(c1")),
        "{:?}",
        inner.faces
    );
    let copy = body(&doc, "F1.b3").unwrap();
    assert!(
        copy.faces
            .iter()
            .any(|f| f.to_string().starts_with("F1:inst1(")),
        "{:?}",
        copy.faces
    );
    // The mount frames: the bore axis on the plane, the faces outwards.
    let mounts = doc
        .last_result()
        .result(uid)
        .and_then(|r| r.output.as_ref())
        .and_then(|o| o.mounts.clone())
        .expect("the frames");
    let bore = mounts["bore"];
    assert_eq!(bore.translation, [0.0, 0.0, 0.0]);
    assert!(near(bore.linear[2][2], 1.0));
    assert!(near(mounts["face_a"].linear[2][2], -1.0));
    let far = mounts["face_b"];
    assert!(near(far.translation[2], 7.0) && near(far.linear[2][2], 1.0));

    // Saved and opened again: the same definition and bodies.
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(status(&loaded, uid), FeatureStatus::Ok);
    assert_eq!(
        loaded.feature(uid).unwrap().def,
        doc.feature(uid).unwrap().def
    );
    assert!(body(&loaded, "F1.b11").is_some());
}

#[test]
fn parameters_and_custom_sizes_rebuild_the_bearing() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "n", "expression": "6", "unit": ""}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "w", "expression": "9 mm"}),
    );
    let uid = add(
        &mut doc,
        json!({"type": "printable_bearing", "element": "roller", "bore": 10,
               "outside_diameter": 30, "width": "w", "element_count": "n",
               "plane": "xy", "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // Six rollers: the races and five copies.
    assert!(body(&doc, "F1.b7").is_some());
    assert!(body(&doc, "F1.b8").is_none());
    let values = query(&doc, json!({"query": "bearing_values", "uid": uid}));
    assert_eq!(values["element_count"], json!(6));
    assert!(near(get(&values, "width"), 9.0));
    // A 10 mm section: walls max(1.2, 10 / 6), rollers 10 - 2 x 1.6667 -
    // 0.8 across, their chamfers a quarter of that.
    let d = 10.0 - 2.0 * 10.0 / 6.0 - 0.8;
    assert!(near(get(&values, "element_diameter"), d), "{values}");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "n", "expression": "8"}),
    );
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(body(&doc, "F1.b9").is_some());
    // The file keeps the expressions.
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(
        saved.to_string().contains("\"element_count\":\"n\""),
        "{saved}"
    );
}

#[test]
fn new_components_are_named_after_their_size() {
    let mut doc = Document::new(MockKernel::default());
    let made = add(
        &mut doc,
        json!({"type": "printable_bearing", "size": "608", "plane": "xy"}),
    )
    .unwrap();
    assert_eq!(status(&doc, made), FeatureStatus::Ok);
    let name = |doc: &Document<MockKernel>, uid: FeatureUid| {
        let a = doc.assembly();
        a.name(a.created_by(uid).expect("its component"))
    };
    assert_eq!(name(&doc, made), "Bearing 608 (8x22x7)");
    // A second one of the same size gets a free name; custom sizes their
    // dimensions.
    let again = add(
        &mut doc,
        json!({"type": "printable_bearing", "size": "608", "plane": "xy", "center": [40, 0]}),
    )
    .unwrap();
    assert_eq!(name(&doc, again), "Bearing 608 (8x22x7) (1)");
    let custom = add(
        &mut doc,
        json!({"type": "printable_bearing", "bore": 9.525, "outside_diameter": 22.225,
               "width": 5.556, "plane": "xy", "center": [80, 0]}),
    )
    .unwrap();
    assert_eq!(name(&doc, custom), "Bearing 9.525x22.225x5.556");
    let values = query(&doc, json!({"query": "bearing_values", "uid": made}));
    assert_eq!(values["component_name"], json!("Bearing 608 (8x22x7)"));
}

#[test]
fn flanges_rollers_and_printed_balls() {
    // F608: the flange 25 x 1.5 on the outer race at the face on the plane.
    let layout = layout_of(json!({"size": "F608"}));
    assert_eq!(layout.flange, Some((25.0, 1.5)));
    let outer = outer_race_section(&layout, 11.0);
    assert!(outer.contains(&[12.5, 0.0]) && outer.contains(&[12.5, 1.5]));
    assert!(outer.contains(&[11.0, 1.5]));
    // A flange of its own on a 608.
    let layout = layout_of(json!({"size": "608", "flange": {"diameter": 26, "thickness": 2}}));
    assert_eq!(layout.flange, Some((26.0, 2.0)));

    // Rollers in a 6202 (15 x 35 x 11): a 10 mm section, walls max(1.2,
    // 10 / 6), rollers 10 - 3.3333 - 0.8 = 5.8667 across with 1.4667
    // chamfers; the lips reach 1.4667 - 0.4 over them.
    let layout = layout_of(json!({"element": "roller", "size": "6202"}));
    let d = 10.0 - 2.0 * 10.0 / 6.0 - 0.8;
    assert!(near(layout.element_diameter, d));
    assert!(near(layout.relief, d / 4.0));
    assert!(near(layout.hold, d / 4.0 - 0.4));
    // The lips at the faces 0.4 along the axis (45 degrees: 0.4 across)
    // from the chamfers.
    assert!(
        near(layout.race[0][0], d / 2.0 - d / 4.0 + 0.4),
        "{:?}",
        layout.race
    );

    // A printed ball of 9 mm in a 20 x 50 x 11 bearing: a sphere with
    // 45-degree caps cut by the faces (7.778 mm at least to reach the bed,
    // 11 sin 45).
    let layout = layout_of(
        json!({"element": "ball", "bore": 20, "outside_diameter": 50,
                                  "width": 11, "element_diameter": 9}),
    );
    assert!(near(layout.element_diameter, 9.0));
    // Its flat ends: 4.5 sqrt(2) - 5.5 across the half.
    let end = 4.5 * 2f64.sqrt() - 5.5;
    assert!(near(layout.element_wall, 2.0 * end));
    // The raceway: a flat 0.4 from the equator, flanks 0.4 along the axis
    // from the caps, so 0.4 beyond the flat ends at the faces.
    let (lo, hi) = layout.race_range();
    assert!(near(hi, 4.9));
    assert!(near(lo, end + 0.4));
    assert!(near(layout.inner_wall.unwrap(), 17.5 - 4.9 - 10.0));
    assert!(!layout.section.is_empty());
}

#[test]
fn every_printability_check() {
    let mut doc = Document::new(MockKernel::default());
    let message = rejected(&mut doc, bearing(json!({"element_count": 12})));
    assert!(
        message.contains("too many elements for the pitch circle: at most 10"),
        "{message}"
    );
    let message = rejected(&mut doc, bearing(json!({"element_count": 2})));
    assert!(message.contains("at least 3"), "{message}");
    let message = rejected(&mut doc, bearing(json!({"element_diameter": 5})));
    assert!(
        message.contains("the inner race's wall is 0.6 mm, below the smallest wall of 1.2 mm"),
        "{message}"
    );
    let message = rejected(&mut doc, bearing(json!({"radial_clearance": 0.1})));
    assert!(
        message.contains("radial clearance 0.1 mm is below the 0.2 mm"),
        "{message}"
    );
    let message = rejected(&mut doc, bearing(json!({"axial_clearance": 0.15})));
    assert!(message.contains("axial clearance 0.15 mm"), "{message}");
    let message = rejected(&mut doc, bearing(json!({"element_gap": 0.1})));
    assert!(message.contains("gap between the elements"), "{message}");
    let message = rejected(&mut doc, bearing(json!({"flank_angle": "60 deg"})));
    assert!(
        message.contains("an overhang of 60 degrees, steeper than 45 degrees"),
        "{message}"
    );
    let message = rejected(&mut doc, bearing(json!({"axial_clearance": 1.0})));
    assert!(message.contains("dumbbells are not held"), "{message}");
    let message = rejected(
        &mut doc,
        bearing(json!({"element": "ball", "element_diameter": 2.5})),
    );
    assert!(
        message.contains("a printed ball of 2.5 mm is too small to print"),
        "{message}"
    );
    let message = rejected(
        &mut doc,
        bearing(json!({"element": "ball", "size": "6202", "element_diameter": 6})),
    );
    assert!(
        message.contains("does not reach the print bed in a 11 mm wide bearing: 7.778 mm"),
        "{message}"
    );
    let message = rejected(&mut doc, bearing(json!({"element": "airsoft"})));
    assert!(
        message.contains("5.95 mm airsoft balls do not fit the section"),
        "{message}"
    );
    let message = rejected(&mut doc, bearing(json!({"chamfer": 1.5})));
    assert!(message.contains("the chamfer of 1.5 mm"), "{message}");
    let message = rejected(
        &mut doc,
        bearing(json!({"flange": {"diameter": 20, "thickness": 1}})),
    );
    assert!(message.contains("flange must be larger"), "{message}");
    let message = rejected(&mut doc, bearing(json!({"size": "999"})));
    assert!(message.contains("not a standard bearing size"), "{message}");
    let message = rejected(&mut doc, bearing(json!({"bore": 8})));
    assert!(message.contains("give either"), "{message}");
    let message = rejected(&mut doc, bearing(json!({"operation": "join"})));
    assert!(message.contains("new_component or new_body"), "{message}");

    // The values query says the same instead of refusing.
    let values = values_of(&doc, json!({"element_count": 12}));
    assert!(
        values["error"]
            .as_str()
            .unwrap()
            .contains("too many elements"),
        "{values}"
    );
    // Nothing was added.
    assert_eq!(doc.features().count(), 0);
}

#[test]
fn airsoft_balls_and_their_cage_in_the_parts_list() {
    let mut doc = Document::new(MockKernel::default());
    // A 6202: balls of 5.95 mm on the 25 mm pitch circle with 2 mm between
    // them for the cage's fingers (9 round the circle), but 8 go in by
    // hand through the crescent: the lands 0.55 r = 1.636 from the pitch
    // circle, the groove bottoms' gap holds a ball where cos(phi) <= 0.4 /
    // 1.636, an arc of 3.635 rad, 0.4806 rad a ball.
    let values = values_of(&doc, json!({"element": "airsoft", "size": "6202"}));
    assert!(values.get("error").is_none(), "{values}");
    assert_eq!(values["element_count"], json!(8), "{values}");
    assert!(near(get(&values, "element_diameter"), 5.95));
    assert!(near(get(&values, "inner_wall"), 12.5 - 3.375 - 7.5));
    // The cage's ring above the balls: 11 - 5.5 - 3.375.
    assert!(near(values["cage"]["ring"].as_f64().unwrap(), 2.125));
    assert_eq!(
        values["purchased"],
        json!([{"designation": "Airsoft ball 6 mm", "quantity": 8}])
    );

    let uid = add(
        &mut doc,
        json!({"type": "printable_bearing", "element": "airsoft", "size": "6202",
               "plane": "xy"}),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // The races, eight balls (spheres) and the cage: eleven bodies.
    assert!(body(&doc, "F1.b10").is_some());
    assert!(body(&doc, "F1.b11").is_none());
    let cage = body(&doc, "F1.b10").unwrap();
    assert!(cage.history.starts_with("cut(revolve("), "{}", cage.history);
    assert_eq!(specs(&doc, "revolve F1 ").len(), 3);
    // Placed twice: 16 balls to buy.
    let list = query(&doc, json!({"query": "parts_list"}));
    let rows = list["rows"].as_array().unwrap();
    let bearing_row = rows
        .iter()
        .find(|r| r["name"] == json!("Bearing 6202 (15x35x11)"))
        .expect("the bearing's row");
    assert_eq!(bearing_row["quantity"], json!(1));
    let balls = rows
        .iter()
        .find(|r| r["purchased"] == json!(true))
        .expect("the balls' row");
    assert_eq!(balls["designation"], json!("Airsoft ball 6 mm"));
    assert_eq!(balls["quantity"], json!(8));
    assert_eq!(balls["component"], bearing_row["component"]);
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Bearing 6202 (15x35x11):1",
               "transform": {"translation": [60, 0, 0]}}),
    );
    let list = query(&doc, json!({"query": "parts_list"}));
    let balls = list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["purchased"] == json!(true))
        .cloned()
        .expect("the balls' row");
    assert_eq!(balls["quantity"], json!(16), "{list}");

    // A dumbbell bearing buys nothing.
    let mut plain = Document::new(MockKernel::default());
    add(&mut plain, bearing(json!({}))).unwrap();
    let list = query(&plain, json!({"query": "parts_list"}));
    assert!(
        list["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("purchased").is_none()),
        "{list}"
    );
    assert_eq!(
        Element::Airsoft,
        serde_json::from_value::<Element>(json!("airsoft")).unwrap()
    );
}

#[test]
fn a_raceway_cut_into_a_shaft() {
    let mut doc = Document::new(MockKernel::default());
    // A 10 mm shaft 20 long (F2.b0, its face F2:side(c1)).
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10}),
    );
    let shaft = add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, shaft), FeatureStatus::Ok);
    let groove = json!({"body": "F2.b0", "face": "F2:side(c1)"});
    // The 608's outer race on it: dumbbells such that the outer race keeps
    // max(1.2, 6 / 4) = 1.5 mm: 11 - (5 + r/2 + 0.4) - (r + 0.4) = 1.5,
    // r = 2.4667.
    let mut def = json!({"size": "608", "groove": groove, "operation": "new_body"});
    let values = query(&doc, json!({"query": "bearing_values", "def": def}));
    assert!(values.get("error").is_none(), "{values}");
    let r = (5.2 - 1.5) / 1.5;
    assert!(near(get(&values, "element_diameter"), 2.0 * r), "{values}");
    assert!(near(get(&values, "outer_wall"), 1.5));
    assert!(values["inner_wall"].is_null());
    assert_eq!(values["groove"]["race"], json!("inner"));
    assert!(near(values["groove"]["diameter"].as_f64().unwrap(), 10.0));
    let count = values["element_count"].as_u64().unwrap() as usize;
    def["type"] = json!("printable_bearing");
    let uid = add(&mut doc, def.clone()).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // The shaft cut, the outer race and the elements new bodies of F3.
    let cut = body(&doc, "F2.b0").unwrap();
    assert!(cut.history.starts_with("cut("), "{}", cut.history);
    assert!(body(&doc, &format!("F3.b{count}")).is_some());
    assert!(body(&doc, &format!("F3.b{}", count + 1)).is_none());
    // The raceway's cutter, the outer race and the first element.
    assert_eq!(specs(&doc, "revolve F3 ").len(), 3);

    // Only as new bodies, on a cylindrical face.
    def["operation"] = json!("new_component");
    let message = rejected(&mut doc, def.clone());
    assert!(
        message.contains("stays in the body's component"),
        "{message}"
    );
    def["operation"] = json!("new_body");
    def["groove"] = json!({"body": "F2.b0", "face": "F2:start(r{c1})"});
    let message = rejected(&mut doc, def);
    assert!(message.contains("cylindrical face"), "{message}");
}

#[test]
fn a_raceway_cut_into_a_body_keeps_the_axial_clearance_past_the_faces() {
    // The body goes on past the bearing's faces: the cutter runs the axial
    // clearance (0.4) past each face at the raceway's radius there, so the
    // dumbbells' ends (at z = 0 and 7, out to the pitch radius + r) do not
    // touch the groove's end walls. In a 22 mm bore (the ring the outer
    // race) and on a 10 mm shaft (the shaft the inner race).
    let def: PrintableBearingDef<f64> =
        serde_json::from_value(json!({"size": "608", "operation": "new_body"})).unwrap();
    for (radius, internal) in [(11.0, true), (5.0, false)] {
        let face = GrooveFace { radius, internal };
        let layout = def.layout(Some(face)).unwrap();
        let section = groove_section(&layout, face);
        let (lo, hi) = section
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[1]), hi.max(p[1]))
            });
        assert!(near(lo, -0.4) && near(hi, 7.4), "{section:?}");
        // At the ends the cutter reaches the raceway's radius at the faces
        // (the dumbbell's band and the radial clearance).
        let rp = layout.pitch_radius;
        let band = layout.race[0][0];
        let reach = if internal { rp + band } else { rp - band };
        for z in [-0.4, 7.4] {
            assert!(
                section.iter().any(|p| near(p[1], z) && near(p[0], reach)),
                "{internal}: {section:?}"
            );
        }
        // The elements reach past the face's radius at their ends, so
        // without the overrun they would touch the body there.
        let element = layout.element_diameter / 2.0;
        assert!(if internal {
            rp + element > radius
        } else {
            rp - element < radius
        });
    }
}

/// A shaft `diameter` across and 30 long up z at the origin: its body, its
/// side face and its top end edge (a circle, as the mock's `edge_curves`
/// gives it).
fn shaft(doc: &mut Document<MockKernel>, diameter: f64) -> (String, String, String) {
    use mitcad_model::datum::CurveGeometry;
    use mitcad_model::topo::{EdgeName, FaceName, SegmentKey};
    let sketch = doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let added = doc
        .add_circle(
            sketch,
            [0.0, 0.0],
            &mitcad_model::features::ValueInput::Number(diameter),
        )
        .unwrap();
    let extrude = add(
        doc,
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": added.region}],
               "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(doc, extrude), FeatureStatus::Ok);
    let side = FaceName::side(extrude, SegmentKey::closed(added.curves[0]));
    let end = FaceName::end(extrude, added.region.clone());
    let edge = EdgeName::new(end, side.clone());
    let r = diameter / 2.0;
    doc.kernel().edge_curves.borrow_mut().insert(
        edge.to_string(),
        CurveGeometry::Circle {
            center: [0.0, 0.0, 30.0],
            normal: [0.0, 0.0, 1.0],
            radius: r,
            start: [r, 0.0, 30.0],
            end: [r, 0.0, 30.0],
        },
    );
    (format!("{extrude}.b0"), side.to_string(), edge.to_string())
}

/// A mount frame of a feature's result: its origin and its z axis.
fn mount(doc: &Document<MockKernel>, uid: FeatureUid, name: &str) -> ([f64; 3], [f64; 3]) {
    let frame = doc
        .last_result()
        .result(uid)
        .and_then(|r| r.output.as_ref())
        .and_then(|o| o.mounts.as_ref())
        .and_then(|m| m.get(name).copied())
        .unwrap_or_else(|| panic!("{uid} has no frame {name}"));
    (
        frame.translation,
        std::array::from_fn(|r| frame.linear[r][2]),
    )
}

fn near3(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| near(a[i], b[i]))
}

#[test]
fn a_bearing_on_a_shaft_takes_the_size_whose_bore_fits() {
    let mut doc = Document::new(MockKernel::default());
    let (body, side, edge) = shaft(&mut doc, 8.0);
    // On the top end edge, 5 mm down the shaft: the 608 (bore 8), facing
    // down the shaft from z = 25.
    let on_end = json!({"shaft": {"body": body, "edge": edge}, "offset": 5});
    let values = query(
        &doc,
        json!({"query": "bearing_values", "def": {"seat": on_end, "operation": "new_body"}}),
    );
    assert!(values.get("error").is_none(), "{values}");
    assert_eq!(values["designation"], json!("608"), "{values}");
    assert_eq!(values["fits"][0], json!("608"), "{values}");
    assert!(
        values["fits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "688")
    );
    assert!(near(get(&values, "shaft_diameter"), 8.0));
    assert_eq!(values["seat"]["hole"], json!(false));
    let uid = add(
        &mut doc,
        json!({"type": "printable_bearing", "seat": on_end, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let (origin, axis) = mount(&doc, uid, "bore");
    assert!(near3(origin, [0.0, 0.0, 25.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    let (far, _) = mount(&doc, uid, "face_b");
    assert!(near3(far, [0.0, 0.0, 18.0]), "{far:?}");
    // Flipped: from the bottom end, up the shaft; another size of an 8 mm
    // bore than the nearest, as given: the 628 (8 x 24 x 8; the 688, 8 x
    // 16 x 5, leaves dumbbells only 0.4 mm at their waists).
    let flipped = add(
        &mut doc,
        json!({"type": "printable_bearing", "size": "628",
               "seat": {"shaft": {"body": body, "edge": edge}, "offset": 5, "flip": true},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, flipped), FeatureStatus::Ok);
    let (origin, axis) = mount(&doc, flipped, "bore");
    assert!(near3(origin, [0.0, 0.0, 5.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, 1.0]), "{axis:?}");
    let values = query(&doc, json!({"query": "bearing_values", "uid": flipped}));
    assert_eq!(values["designation"], json!("628"), "{values}");
    // On the side face picked near the bottom: from the bottom end.
    let on_side = add(
        &mut doc,
        json!({"type": "printable_bearing",
               "seat": {"shaft": {"body": body, "face": side}, "offset": 10, "near": [4, 0, 2]},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, on_side), FeatureStatus::Ok);
    let (origin, _) = mount(&doc, on_side, "bore");
    assert!(near3(origin, [0.0, 0.0, 10.0]), "{origin:?}");
    // A new component without a size is named before the shaft is known.
    let made = add(
        &mut doc,
        json!({"type": "printable_bearing", "seat": on_end}),
    )
    .unwrap();
    assert_eq!(status(&doc, made), FeatureStatus::Ok);
    let a = doc.assembly();
    assert_eq!(a.name(a.created_by(made).unwrap()), "Bearing");

    // An 11 mm shaft: no bore within 0.5 mm; within 1 mm the 6000 (10).
    let mut odd = Document::new(MockKernel::default());
    let (body, _, edge) = shaft(&mut odd, 11.0);
    let message = rejected(
        &mut odd,
        json!({"type": "printable_bearing", "seat": {"shaft": {"body": body, "edge": edge}},
               "operation": "new_body"}),
    );
    assert!(
        message.contains("no standard bearing's bore is within 0.5 mm of the 11 mm shaft"),
        "{message}"
    );
    let values = query(
        &odd,
        json!({"query": "bearing_values", "def": {
            "seat": {"shaft": {"body": body, "edge": edge}}, "fit_clearance": 1,
            "operation": "new_body"}}),
    );
    assert_eq!(values["designation"], json!("6000"), "{values}");

    // Refused: a seat with a plane, a fit clearance without a seat.
    let message = rejected(
        &mut doc,
        json!({"type": "printable_bearing", "plane": "xy", "seat": on_end,
               "operation": "new_body"}),
    );
    assert!(
        message.contains("takes its plane and center from it"),
        "{message}"
    );
    let message = rejected(&mut doc, bearing(json!({"fit_clearance": 1})));
    assert!(
        message.contains("a fit clearance needs a seat"),
        "{message}"
    );
}

#[test]
fn a_bearing_in_a_hole_takes_the_size_whose_outside_fits_flush_with_the_face() {
    let mut doc = Document::new(MockKernel::default());
    // A block 40 x 40 x 10 under XY (F2.b0) with a 22 mm hole down from
    // its top face (F3, its wall F3:hole0.wall).
    let region = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-20, -20],
               "width": 40, "height": 40}),
    );
    let block = add(
        &mut doc,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": -10}, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, block), FeatureStatus::Ok);
    let hole = add(
        &mut doc,
        json!({"type": "hole", "placement": {"type": "plane", "plane": "xy", "points": [[0, 0, 0]]},
               "diameter": 22, "extent": {"type": "distance", "depth": 10}}),
    )
    .unwrap();
    assert_eq!(status(&doc, hole), FeatureStatus::Ok);
    // The wall picked near the top: flush with the top face (offset 0),
    // into the hole; the 608 by its 22 mm outside.
    let seat = json!({"shaft": {"body": "F2.b0", "face": "F3:hole0.wall"}, "near": [11, 0, -1]});
    let values = query(
        &doc,
        json!({"query": "bearing_values", "def": {"seat": seat, "operation": "new_body"}}),
    );
    assert!(values.get("error").is_none(), "{values}");
    assert_eq!(values["seat"]["hole"], json!(true), "{values}");
    assert_eq!(values["designation"], json!("608"), "{values}");
    assert!(near(get(&values, "shaft_diameter"), 22.0));
    let uid = add(
        &mut doc,
        json!({"type": "printable_bearing", "seat": seat, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let (origin, axis) = mount(&doc, uid, "bore");
    assert!(near3(origin, [0.0, 0.0, 0.0]), "{origin:?}");
    assert!(near3(axis, [0.0, 0.0, -1.0]), "{axis:?}");
    // 2 mm deep.
    let mut deeper = seat.clone();
    deeper["offset"] = json!(2);
    let deep = add(
        &mut doc,
        json!({"type": "printable_bearing", "seat": deeper, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, deep), FeatureStatus::Ok);
    let (origin, _) = mount(&doc, deep, "bore");
    assert!(near3(origin, [0.0, 0.0, -2.0]), "{origin:?}");
}
