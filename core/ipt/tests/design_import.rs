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
    for major in [25, 28] {
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
