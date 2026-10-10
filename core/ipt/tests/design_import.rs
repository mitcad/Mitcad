// SPDX-License-Identifier: MIT
//! The design decoded from definitions records (`mitcad_ipt::testdesign`)
//! replayed by the import on the model's mock geometry kernel: the
//! parameters with their expressions, the sketch with its constraints and
//! dimensions, and the extrusion, all parametric.

use mitcad_import::{NoGeometry, Options, Outcome, import_design};
use mitcad_ipt::dc::Definitions;
use mitcad_ipt::params::DisplayUnit;
use mitcad_model::Document;
use mitcad_model::testing::MockKernel;

#[test]
fn the_cube_design_replays_parametric() {
    for major in [20, 22, 24, 25, 28] {
        let dc = Definitions::from_records(major, &mitcad_ipt::testdesign::cube_design(major));
        let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "cube.ipt", None);
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
        let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
        assert_eq!(
            outcomes,
            [Outcome::Parametric, Outcome::Parametric],
            "{}",
            report.text()
        );
        assert_eq!(report.parameters.imported, 5, "{:?}", report.parameters);
        assert!(report.parameters.mismatched.is_empty());
        assert!(report.parameters.literal.is_empty());
        let params = doc.parameters();
        let side = params.get(params.find("Side").unwrap()).unwrap();
        assert_eq!(side.expression(), "10 mm");
        assert!((side.value() - 10.0).abs() < 1e-12);
        let d0 = params.get(params.find("d0").unwrap()).unwrap();
        assert_eq!(d0.expression(), "Side");
        // The sketch owns its dimension's parameter, the extrusion its
        // distance.
        assert!(params.owner(params.find("d0").unwrap()).is_some());
        assert!(params.owner(params.find("d2").unwrap()).is_some());
        assert_eq!(doc.bodies().len(), 1);
    }
}

/// The cube from a square of lines flagged 0x40 whose profile the
/// extrusion selected: the lines bound it, and both items are parametric.
#[test]
fn flagged_lines_of_a_selected_profile_make_the_cube() {
    for major in [21, 24] {
        let dc =
            Definitions::from_records(major, &mitcad_ipt::testdesign::flagged_cube_design(major));
        let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "cube.ipt", None);
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
        let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
        assert_eq!(
            outcomes,
            [Outcome::Parametric, Outcome::Parametric],
            "{}",
            report.text()
        );
        assert_eq!(doc.bodies().len(), 1);
    }
}

/// A square and its offset, driven by the distance between a side and its
/// offset, comes in whole.
#[test]
fn the_offset_replays_parametric() {
    for major in [21, 24, 28] {
        let dc = Definitions::from_records(major, &mitcad_ipt::testdesign::offset_design(major));
        let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "offset.ipt", None);
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
        let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
        assert_eq!(outcomes, [Outcome::Parametric], "{}", report.text());
    }
}

/// A sketch with a polygon, a circular and a rectangular pattern, points
/// on horizontal and vertical lines and an arc with a point on it comes in
/// whole.
#[test]
fn the_sketch_groups_replay_parametric() {
    for major in [21, 24, 25, 28] {
        let dc = Definitions::from_records(major, &mitcad_ipt::testdesign::groups_design(major));
        let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "groups.ipt", None);
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
        let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
        assert_eq!(outcomes, [Outcome::Parametric], "{}", report.text());
    }
}

/// Work planes through work points and axes come in as Mitcad's planes
/// through points and lines, where the file has them.
#[test]
fn planes_through_points_and_axes_replay_parametric() {
    for major in [18, 24, 28] {
        let dc = Definitions::from_records(major, &mitcad_ipt::testdesign::planes_design(major));
        let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "planes.ipt", None);
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
        let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
        assert_eq!(outcomes, [Outcome::Parametric; 4], "{}", report.text());
    }
}

/// Radial lines, a line of no length, a pattern on its originals and a
/// linear diameter: the sketch comes in whole.
#[test]
fn the_sketch_fixes_replay_parametric() {
    for major in [21, 24, 28] {
        let dc =
            Definitions::from_records(major, &mitcad_ipt::testdesign::sketch_fixes_design(major));
        let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "fixes.ipt", None);
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
        let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
        assert_eq!(outcomes, [Outcome::Parametric], "{}", report.text());
    }
}

/// Functions in expressions come in as Mitcad's; a parameter nothing uses
/// whose expression does not give its value keeps the value.
#[test]
fn functions_replay_and_unused_parameters_keep_their_values() {
    let dc = Definitions::from_records(25, &mitcad_ipt::testdesign::function_design(25));
    let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "functions.ipt", None);
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
    assert!(
        report.parameters.mismatched.is_empty(),
        "{:?}",
        report.parameters
    );
    let params = doc.parameters();
    let d0 = params.get(params.find("d0").unwrap()).unwrap();
    assert_eq!(d0.expression(), "SW / cos(30 deg)");
    assert!((d0.value() - 2.0 / 0.75f64.sqrt()).abs() < 1e-9);
    let d1 = params.get(params.find("d1").unwrap()).unwrap();
    assert_eq!(d1.expression(), "SW / 2");
    let d2 = params.get(params.find("d2").unwrap()).unwrap();
    assert!((d2.value() - std::f64::consts::FRAC_PI_4).abs() < 1e-9);
}

/// Two cubes and a combine of their bodies, the bodies named by the
/// extrusions that made them: every item parametric, one body left.
#[test]
fn a_combine_joins_the_bodies_its_record_names() {
    for major in [21, 24, 28] {
        let dc =
            Definitions::from_records(major, &mitcad_ipt::testdesign::combine_design(major, false));
        let design = mitcad_ipt::design::design(&dc, DisplayUnit::Mm, "combine.ipt", None);
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, &design.dump, &mut NoGeometry, &Options::default());
        let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
        assert_eq!(outcomes, [Outcome::Parametric; 5], "{}", report.text());
        assert_eq!(doc.bodies().len(), 1, "{}", report.text());
    }
}
