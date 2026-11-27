// SPDX-License-Identifier: MIT
//! Curves of the automatic face outline chosen to bound profiles
//! (mitcad#2551, [`crate::Projection::boundaries`]): a face with four
//! bosses standing on it, its outline projected when the sketch was made
//! and an inset of the outline drawn 2 mm inside. The inset is one area
//! with the bosses' circles inside it; the circles chosen make holes in
//! it, only the chosen ones; without the inset the face's own area stays.

use std::f64::consts::PI;

use crate::def::SketchDef;
use crate::edit::SketchEdit;
use crate::geometry::Curve2;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::Parameters;
use crate::regions::{Region, RegionCurve, own_area_regions};
use crate::topo::{CurveId, EdgeName, FaceName};
use crate::{EntityKind, Projection, ProjectionSource, Ref};

/// The bosses' centres on the 60 x 40 face; each boss is 12 mm across.
const BOSSES: [[f64; 2]; 4] = [[15.0, 10.0], [45.0, 10.0], [15.0, 30.0], [45.0, 30.0]];
const RADIUS: f64 = 6.0;

/// The outline's stored lines (`c101`..`c104`) and circles
/// (`c111`..`c114`).
const LINES: u32 = 101;
const CIRCLES: u32 = 111;

fn corners(lo: [f64; 2], hi: [f64; 2]) -> [[f64; 2]; 4] {
    [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]]
}

fn rectangle(first: u32, lo: [f64; 2], hi: [f64; 2]) -> Vec<RegionCurve> {
    let c = corners(lo, hi);
    (0..4)
        .map(|i| {
            RegionCurve::new(
                EntityUid(first + i as u32),
                Curve2::Line {
                    a: c[i],
                    b: c[(i + 1) % 4],
                },
            )
        })
        .collect()
}

fn boss_circle(id: u32, center: [f64; 2]) -> RegionCurve {
    RegionCurve::new(
        EntityUid(id),
        Curve2::Circle {
            center,
            radius: RADIUS,
            start: 0.0,
        },
    )
}

/// The face's own edges: its outer boundary and the bosses' circles.
fn face_edges() -> Vec<RegionCurve> {
    let mut curves = rectangle(0, [0.0, 0.0], [60.0, 40.0]);
    curves.extend(BOSSES.iter().map(|c| boss_circle(0, *c)));
    for (i, curve) in curves.iter_mut().enumerate() {
        let edge = EdgeName::new(
            FaceName::new(FeatureUid(2), "end", None),
            FaceName::new(FeatureUid(2), "side", None),
        )
        .with_index(i as u32);
        curve.uid = CurveId::FaceEdge(Box::new(edge.into()));
    }
    curves
}

/// The outline projected when the sketch was made, as stored curves.
fn outline() -> Vec<RegionCurve> {
    let mut curves = rectangle(LINES, [0.0, 0.0], [60.0, 40.0]);
    curves.extend(
        BOSSES
            .iter()
            .enumerate()
            .map(|(i, c)| boss_circle(CIRCLES + i as u32, *c)),
    );
    curves
}

/// The inset 2 mm inside the face's outer boundary (`c1`..`c4`).
fn inset() -> Vec<RegionCurve> {
    rectangle(1, [2.0, 2.0], [58.0, 38.0])
}

/// A sketch whose automatic outline lists the outline's curves, with
/// `chosen` of them bounding profiles.
fn def_with(chosen: &[u32]) -> SketchDef {
    let mut def = SketchDef::default();
    def.projections.push(Projection {
        source: "F2:end".parse::<ProjectionSource>().unwrap(),
        body: None,
        entities: (LINES..LINES + 4)
            .chain(CIRCLES..CIRCLES + 4)
            .map(|id| Ref::Curve(EntityUid(id)))
            .collect(),
        link: None,
        by_geometry: true,
        section: false,
        chain: Vec::new(),
        after: None,
        outline: true,
        automatic: true,
        boundaries: chosen.iter().map(|id| EntityUid(*id)).collect(),
    });
    def
}

/// The regions as the sketch's evaluation finds them.
fn regions(def: &SketchDef, with_inset: bool) -> Vec<Region> {
    let mut curves = if with_inset { inset() } else { Vec::new() };
    curves.extend(outline());
    let (own, mut automatic) = def.split_automatic(curves);
    automatic.extend(face_edges());
    own_area_regions(&own, &automatic).regions
}

/// The inset's region: keyed by its own lines.
fn inset_region(regions: &[Region]) -> &Region {
    regions
        .iter()
        .find(|r| r.profile.key.to_string().contains("c1["))
        .unwrap_or_else(|| panic!("no inset region: {regions:?}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-3 * b.abs().max(1.0)
}

const INSET_AREA: f64 = 56.0 * 36.0;

fn boss_area() -> f64 {
    PI * RADIUS * RADIUS
}

#[test]
fn without_a_choice_the_bosses_make_no_holes_in_the_inset() {
    let found = regions(&def_with(&[]), true);
    let inset = inset_region(&found);
    assert_eq!(inset.profile.loops.len(), 1, "{found:?}");
    assert!(close(inset.area, INSET_AREA), "{}", inset.area);
}

#[test]
fn the_chosen_circles_make_holes_in_the_inset() {
    let all: Vec<u32> = (CIRCLES..CIRCLES + 4).collect();
    let found = regions(&def_with(&all), true);
    let inset = inset_region(&found);
    assert_eq!(inset.profile.loops.len(), 5, "{found:?}");
    assert!(
        close(inset.area, INSET_AREA - 4.0 * boss_area()),
        "{}",
        inset.area
    );
    // The key stays the inset's: its outer loop's segments.
    let plain = regions(&def_with(&[]), true);
    assert_eq!(inset.profile.key, inset_region(&plain).profile.key);
    // Each footprint is a region of its own.
    for id in &all {
        let key = format!("r{{c{id}}}");
        let disc = found
            .iter()
            .find(|r| r.profile.key.to_string() == key)
            .unwrap_or_else(|| panic!("no {key}: {found:?}"));
        assert!(close(disc.area, boss_area()), "{}", disc.area);
    }
}

#[test]
fn only_the_chosen_circles_make_holes() {
    let found = regions(&def_with(&[CIRCLES, CIRCLES + 3]), true);
    let inset = inset_region(&found);
    assert_eq!(inset.profile.loops.len(), 3, "{found:?}");
    assert!(
        close(inset.area, INSET_AREA - 2.0 * boss_area()),
        "{}",
        inset.area
    );
}

#[test]
fn without_the_inset_the_face_bounded_area_stays() {
    for chosen in [Vec::new(), (CIRCLES..CIRCLES + 4).collect::<Vec<u32>>()] {
        let found = regions(&def_with(&chosen), false);
        let face = 60.0 * 40.0 - 4.0 * boss_area();
        assert!(
            found.iter().any(|r| close(r.area, face)),
            "{chosen:?}: {found:?}"
        );
    }
}

fn circle_entity(edit: &mut SketchEdit, at: [f64; 2]) -> EntityUid {
    let center = edit.add_point(at);
    edit.add_curve(EntityKind::Circle {
        center,
        radius: RADIUS,
        start: 0.0,
    })
}

#[test]
fn profile_boundary_takes_only_curves_of_the_automatic_outline() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(3), "Sketch3");
    let projected = circle_entity(&mut edit, BOSSES[0]);
    let drawn = circle_entity(&mut edit, BOSSES[1]);
    let mut projection = def_with(&[]).projections.remove(0);
    projection.entities = vec![Ref::Curve(projected)];
    edit.def.projections.push(projection);
    let refused = edit.set_profile_boundary(&[drawn], true).unwrap_err();
    assert!(refused.contains("outline"), "{refused}");
    assert!(edit.def.projections[0].boundaries.is_empty());
    edit.set_profile_boundary(&[projected], true).unwrap();
    assert_eq!(edit.def.projections[0].boundaries, vec![projected]);
    // Chosen twice, listed once.
    edit.set_profile_boundary(&[projected], true).unwrap();
    assert_eq!(edit.def.projections[0].boundaries, vec![projected]);
    // Saved with the design and read again.
    let saved = serde_json::to_value(&edit.def.projections[0]).unwrap();
    assert_eq!(
        saved["boundaries"],
        serde_json::json!([format!("c{}", projected.0)])
    );
    let read: Projection = serde_json::from_value(saved).unwrap();
    assert_eq!(read, edit.def.projections[0]);
    edit.set_profile_boundary(&[projected], false).unwrap();
    assert!(edit.def.projections[0].boundaries.is_empty());
    let saved = serde_json::to_value(&edit.def.projections[0]).unwrap();
    assert!(saved.get("boundaries").is_none(), "{saved}");
}

#[test]
fn a_removed_curve_is_no_longer_chosen() {
    let mut params = Parameters::default();
    let mut edit = SketchEdit::new(SketchDef::default(), &mut params, FeatureUid(3), "Sketch3");
    let a = circle_entity(&mut edit, BOSSES[0]);
    let b = circle_entity(&mut edit, BOSSES[1]);
    let mut projection = def_with(&[]).projections.remove(0);
    projection.entities = vec![Ref::Curve(a), Ref::Curve(b)];
    edit.def.projections.push(projection);
    edit.set_profile_boundary(&[a, b], true).unwrap();
    edit.remove(&[crate::modify::Item::Entity(Ref::Curve(a))])
        .unwrap();
    assert_eq!(edit.def.projections[0].boundaries, vec![b]);
}
