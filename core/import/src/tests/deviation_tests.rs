// SPDX-License-Identifier: MIT
//! Results that deviate from the file's, kept at the user's choice
//! (mitcad#379), on synthetic dumps with the mock kernel: a rounding whose
//! result leaves 0.2 mm³ of the 1 mm³ the file's takes falls back and is
//! listed with its numbers by default; kept, it stays parametric, the
//! rounding after it replays on its result, the final bodies are not
//! replaced, and the report and the timeline say so.

use mitcad_model::FilletSize;

use super::*;
use crate::{AcceptDeviations, DeviationReport};

/// A block, Fillet1 on one of its edges and Fillet2 on another, taking 1
/// mm³ in the file; the replay's roundings leave `off` mm³ more (once on
/// a body). Imported with `accept`.
fn deviating_rounding(off: f64, accept: AcceptDeviations) -> (DesignReport, Document<TestKernel>) {
    deviating_roundings(off, 1.0, accept)
}

/// [`deviating_rounding`] with the file's Fillet2 taking `second` mm³.
fn deviating_roundings(
    off: f64,
    second: f64,
    accept: AcceptDeviations,
) -> (DesignReport, Document<TestKernel>) {
    let mut items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let exact = || TestKernel {
        edge_points: true,
        fillet_takes: Some(1.0),
        ..TestKernel::default()
    };
    let mut reference = Document::new(exact());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    fn set_of(edge: &EdgeName) -> FilletSet<'_> {
        FilletSet {
            edges: std::slice::from_ref(edge),
            faces: &[],
            size: FilletSize::Constant { radius: 2.0 },
            tangent_chain: true,
            curvature: false,
            weight: 0.0,
        }
    }
    let (a, b) = block[0].edges[0].clone();
    let (c, d) = block[0].edges[1].clone();
    let first = EdgeName::new(a.clone(), b.clone());
    let second_edge = EdgeName::new(c.clone(), d.clone());
    let stored1 = exact()
        .fillet(FeatureUid(3), &block[0], &[set_of(&first)], true)
        .unwrap();
    let stored2 = TestKernel {
        fillet_takes: Some(second),
        ..exact()
    }
    .fillet(FeatureUid(4), &stored1, &[set_of(&second_edge)], true)
    .unwrap();
    items.push(fillet_on(2, "Fillet1", &a, &b));
    items.push(fillet_on(3, "Fillet2", &c, &d));
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), block, vec![stored1], vec![stored2]],
        items: HashMap::from([(1, 1), (2, 2), (3, 3)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel {
        lists_new_faces: true,
        face_listing: true,
        fillet_off: off,
        ..exact()
    });
    let options = Options {
        accept_deviations: accept,
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    (report, doc)
}

/// A fillet item on the edge between faces `a` and `b`.
fn fillet_on(
    index: i64,
    name: &str,
    a: &mitcad_model::FaceName,
    b: &mitcad_model::FaceName,
) -> Value {
    let p = edge_point(a, b);
    json!({"index": index, "name": name, "objectType": "FilletFeature",
    "detail": {"edgeSets": [{"radius": param(&format!("d{}", 5 + index), "2 mm", 0.2, "mm"),
        "edges": [{"kind": "edge", "length": 0.1, "mid_point": p.map(|x| x / 10.0)}]}]}})
}

/// The deviation's numbers: the replay takes 0.8 mm³, the file 1.
fn assert_numbers(d: &DeviationReport, text: &str) {
    assert!((d.volume_change + 0.8).abs() < 1e-6, "{text}");
    assert!((d.file_volume_change + 1.0).abs() < 1e-6, "{text}");
    assert!((d.volume_difference - 0.2).abs() < 1e-6, "{text}");
    assert!(
        d.relative_difference
            .is_some_and(|r| (r - 0.2).abs() < 1e-6),
        "{text}"
    );
    assert!(
        (d.body_volume - d.stored_body_volume - 0.2).abs() < 1e-6,
        "{text}"
    );
    assert_eq!(d.tolerance, crate::DEFAULT_GEOMETRIC_TOLERANCE);
    // The mock's new faces lie on the file's: the volume tells it apart.
    assert_eq!(d.largest_distance, Some(0.0), "{text}");
    assert_eq!(d.area_beyond, Some(0.0), "{text}");
}

#[test]
fn a_deviating_result_takes_the_files_bodies_and_is_listed_by_default() {
    let (report, doc) = deviating_rounding(0.2, AcceptDeviations::None);
    let text = report.text();
    assert_eq!(report.items[1].outcome, Outcome::Parametric, "{text}");
    assert_eq!(report.items[2].outcome, Outcome::Fallback, "{text}");
    let d = report.items[2].deviation.as_ref().expect(&text);
    assert!(!d.accepted, "{text}");
    assert!(report.items[2].accepted_deviation.is_none(), "{text}");
    assert_numbers(d, &text);
    assert!(
        text.contains("deviating result (not kept): volume change"),
        "{text}"
    );
    // (Fillet2 does not find its edge on the mock's stored bodies.)
    // The JSON report carries it.
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(
        json["items"][2]["deviation"]["accepted"],
        json!(false),
        "{json}"
    );
    assert!(
        json["items"][2].get("accepted_deviation").is_none(),
        "{json}"
    );
    assert!(doc.features().any(|f| f.def.type_name() == "base"));
}

#[test]
fn an_accepted_deviation_stays_parametric_and_the_items_after_it_replay_on_it() {
    for accept in [
        AcceptDeviations::Items(BTreeSet::from([2])),
        AcceptDeviations::All,
    ] {
        let (report, doc) = deviating_rounding(0.2, accept.clone());
        let text = report.text();
        assert_eq!(
            report.items[2].outcome,
            Outcome::Parametric,
            "{accept:?}: {text}"
        );
        let d = report.items[2].accepted_deviation.as_ref().expect(&text);
        assert!(d.accepted, "{text}");
        assert_numbers(d, &text);
        assert!(report.items[2].deviation.is_none(), "{text}");
        assert!(
            report.items[2]
                .note
                .as_deref()
                .is_some_and(|n| n.contains("kept parametric at the user's choice")),
            "{text}"
        );
        // Fillet2 replays on Fillet1's result and gives its own change.
        assert_eq!(report.items[3].outcome, Outcome::Parametric, "{text}");
        assert_eq!(report.items[3].verified, Some(true), "{text}");
        // No base feature brings the file's bodies back.
        assert!(
            !doc.features().any(|f| f.def.type_name() == "base"),
            "{text}"
        );
        // The body keeps the deviation to the end.
        let volume: f64 = doc
            .bodies()
            .iter()
            .map(|b| doc.kernel().mass_properties(b.shape).unwrap().volume)
            .sum();
        let stored: f64 = report.bodies.iter().map(|b| b.file_volume).sum();
        assert!(
            (volume - stored - 0.2).abs() < 1e-6,
            "{volume} {stored}: {text}"
        );
        // The timeline's note on Fillet1's feature says so.
        let uid: FeatureUid = report.items[2].features[0].parse().unwrap();
        let note = doc.import_note(uid).unwrap_or_default();
        assert!(
            note.contains("Deviation from the file accepted at import"),
            "{note}"
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["items"][2]["outcome"], json!("parametric"), "{json}");
        assert_eq!(
            json["items"][2]["accepted_deviation"]["accepted"],
            json!(true),
            "{json}"
        );
    }
}

#[test]
fn a_later_item_that_does_not_give_its_change_there_brings_the_files_bodies_back() {
    // Fillet1 kept; the file's Fillet2 takes 1.5 mm³ where the replay's
    // takes 1: it falls back (its own deviation listed), and the file's
    // body takes the deviated one's place to the end.
    let (report, doc) = deviating_roundings(0.2, 1.5, AcceptDeviations::Items(BTreeSet::from([2])));
    let text = report.text();
    assert_eq!(report.items[2].outcome, Outcome::Parametric, "{text}");
    assert!(report.items[2].accepted_deviation.is_some(), "{text}");
    assert_eq!(report.items[3].outcome, Outcome::Fallback, "{text}");
    let d = report.items[3].deviation.as_ref().expect(&text);
    assert!((d.volume_change + 1.0).abs() < 1e-6, "{text}");
    assert!((d.file_volume_change + 1.5).abs() < 1e-6, "{text}");
    let volume: f64 = doc
        .bodies()
        .iter()
        .map(|b| doc.kernel().mass_properties(b.shape).unwrap().volume)
        .sum();
    let stored: f64 = report.bodies.iter().map(|b| b.file_volume).sum();
    assert!((volume - stored).abs() < 1e-6, "{volume} {stored}: {text}");
}

#[test]
fn the_note_of_a_deviating_result_says_why_it_was_not_taken() {
    // The decoded definition built a result whose own change is off the
    // file's: that is the item's cause in its note, before whatever the
    // definitions after it said (mitcad#1926: a fillet that built in 14 s
    // was noted only as out of time, from the guesses after it).
    let deviated =
        "its result comes close to the file's, but the volume it adds or removes is 8.4 % off";
    let note = crate::decoded_cause(
        crate::OUT_OF_TIME.to_owned(),
        None,
        Some(deviated.to_owned()),
    );
    assert_eq!(
        note,
        format!("{deviated}; the other definitions: {}", crate::OUT_OF_TIME)
    );
    // Said once where the last definition was that one.
    assert_eq!(
        crate::decoded_cause(deviated.to_owned(), None, Some(deviated.to_owned())),
        deviated
    );
    // A decoded definition that failed comes first where the guesses after
    // it only missed the state (mitcad#325), as before.
    let missed = crate::NOT_IN_HISTORY.to_owned();
    assert_eq!(
        crate::decoded_cause(missed.clone(), Some("fillet failed".to_owned()), None),
        format!("fillet failed; the other definitions: {missed}")
    );
    assert_eq!(crate::decoded_cause(missed.clone(), None, None), missed);
    // The mock's rounding, 20 % off: its note and its deviation say so.
    let (report, _) = deviating_rounding(0.2, AcceptDeviations::None);
    let text = report.text();
    let note = report.items[2].note.as_deref().unwrap_or_default();
    assert!(
        note.starts_with(
            "its result comes close to the file's, but the volume it adds or removes is 20.0 % off"
        ),
        "{note}\n{text}"
    );
    let reason = &report.items[2].deviation.as_ref().expect(&text).reason;
    assert!(reason.starts_with("its result comes close"), "{reason}");
}

#[test]
fn other_items_are_not_kept() {
    let (report, _) = deviating_rounding(0.2, AcceptDeviations::Items(BTreeSet::from([3])));
    let text = report.text();
    assert_eq!(report.items[2].outcome, Outcome::Fallback, "{text}");
    assert!(report.items[2].deviation.is_some(), "{text}");
}

#[test]
fn a_rounding_of_some_of_the_decoded_edges_leaves_the_others_out() {
    let fillet = |edges: &[&str], radius: f64| {
        json!({"type": "fillet", "body": "F2.b0",
               "sets": [{"edges": edges, "size": {"type": "constant", "radius": radius}}]})
    };
    let decoded = crate::dressup_edge_set(&Candidate::new(fillet(&["E1", "E2", "E3"], 2.0)));
    let mut some = Candidate::new(fillet(&["E1", "E3"], 2.0));
    some.guess = true;
    assert_eq!(crate::edges_left_out(decoded.as_ref(), &some), vec!["E2"]);
    // One definition per edge, all of them: nothing left out.
    let mut split = Candidate::new(fillet(&["E1"], 2.0));
    split.defs.push(fillet(&["E2"], 2.0));
    split.defs.push(fillet(&["E3"], 2.0));
    assert!(crate::edges_left_out(decoded.as_ref(), &split).is_empty());
    // One per edge, two of them: the third left out.
    split.defs.pop();
    assert_eq!(crate::edges_left_out(decoded.as_ref(), &split), vec!["E3"]);
    // Another rounding, or edges the decoded one does not have: none.
    let other = Candidate::new(fillet(&["E1"], 3.0));
    assert!(crate::edges_left_out(decoded.as_ref(), &other).is_empty());
    let more = Candidate::new(fillet(&["E1", "E4"], 2.0));
    assert!(crate::edges_left_out(decoded.as_ref(), &more).is_empty());
    assert!(crate::edges_left_out(None, &some).is_empty());
}

#[test]
fn which_items_to_keep_reads_from_text() {
    assert_eq!(AcceptDeviations::parse("all"), Some(AcceptDeviations::All));
    assert_eq!(
        AcceptDeviations::parse("none"),
        Some(AcceptDeviations::None)
    );
    assert_eq!(
        AcceptDeviations::parse(" 88, 93 "),
        Some(AcceptDeviations::Items(BTreeSet::from([88, 93])))
    );
    assert_eq!(AcceptDeviations::parse("88,,93"), None);
    assert_eq!(AcceptDeviations::parse("some"), None);
    assert!(AcceptDeviations::All.accepts(5));
    assert!(!AcceptDeviations::None.accepts(5));
    assert!(AcceptDeviations::Items(BTreeSet::from([5])).accepts(5));
}
