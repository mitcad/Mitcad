// SPDX-License-Identifier: MIT
//! A drawing's hole callout on a circle seen end on (mitcad#2212): its text
//! kept from the hole's centre, its arrow where the line from the leader's
//! shoulder to the centre crosses the circle, wherever the text is placed
//! or dragged; a callout written before keeps its text where it was drawn.

use super::*;

/// View (0, 0) at sheet (100, 100), 2:1.
fn map() -> ViewMap {
    ViewMap::new(2.0, [0.0, 0.0], [100.0, 100.0])
}

fn frame(map: &ViewMap) -> Frame<'_> {
    Frame {
        map,
        view_box: Bounds::empty(),
        text: DIMENSION_TEXT,
        standard: Standard::Iso,
    }
}

/// A hole of 3 mm radius at view (10, 20): sheet (120, 140), 6 mm round
/// on the sheet.
const CENTER: P2 = [120.0, 140.0];
const RADIUS: f64 = 6.0;

fn hole() -> Seen {
    Seen::Circle {
        center: [10.0, 20.0],
        radius: 3.0,
        end_on: true,
        across: [1.0, 0.0],
        arc: None,
    }
}

fn callout() -> ViewAnnotation {
    ViewAnnotation::new(1, AnnotationType::HoleCallout, Vec::new())
}

fn near(a: P2, b: P2) -> bool {
    len2(sub2(a, b)) < 1e-9
}

/// The arrow and the shoulder as the callout is drawn, checked against
/// its laid out leader line: the arrow on the circle, on the line from
/// the shoulder to the centre, between them.
fn leader(annotation: &ViewAnnotation, frame: &Frame<'_>) -> (P2, P2) {
    let seen = [hole()];
    let (tip, knee) = leader_line(annotation, &seen, frame, &[]).expect("a leader");
    let laid = lay_out(annotation, &seen, frame, Some(&ValueText::default()), &[]).unwrap();
    assert!(
        laid.group
            .items
            .iter()
            .any(|i| matches!(i, Item::Line { a, b, .. } if near(*a, tip) && near(*b, knee))),
        "laid out from {tip:?} to {knee:?}: {:?}",
        laid.group.items
    );
    assert!(
        (len2(sub2(tip, CENTER)) - RADIUS).abs() < 1e-9,
        "{tip:?} on the circle"
    );
    let (out, to_knee) = (sub2(tip, CENTER), sub2(knee, CENTER));
    assert!(
        vec2::cross(out, to_knee).abs() < 1e-9 && dot2(out, to_knee) > 0.0,
        "{tip:?} towards {knee:?} from the centre"
    );
    (tip, knee)
}

#[test]
fn a_hole_callouts_arrow_is_on_the_line_from_its_text_to_the_holes_centre() {
    let map = map();
    let frame = frame(&map);
    let seen = [hole()];
    // Placed left of, above and right of the hole: its arrow on the side
    // of its text.
    for (at, side) in [
        ([90.0, 145.0], "left"),
        ([123.0, 170.0], "above"),
        ([150.0, 132.0], "right"),
    ] {
        let mut a = callout();
        placed_at(&mut a, &seen, &frame, at);
        assert!(a.text_from_center, "{side}");
        assert_eq!(a.text_offset, Some(sub2(at, CENTER)), "{side}");
        let (tip, knee) = leader(&a, &frame);
        assert!(near(knee, at), "{side}: {knee:?}");
        match side {
            "left" => assert!(tip[0] < CENTER[0] - 5.0, "{side}: {tip:?}"),
            "above" => assert!(tip[1] > CENTER[1] + 5.0, "{side}: {tip:?}"),
            _ => assert!(tip[0] > CENTER[0] + 5.0, "{side}: {tip:?}"),
        }
    }
    // Not placed: where its text went before, right of the hole.
    let mut a = callout();
    a.text_from_center = true;
    let (tip, knee) = leader(&a, &frame);
    assert!(near(
        knee,
        add2(CENTER, [RADIUS + NOTE_SHIFT[0], NOTE_SHIFT[1]])
    ));
    assert!(tip[0] > CENTER[0]);
}

#[test]
fn dragging_a_hole_callouts_text_moves_its_arrow_round_the_hole() {
    let map = map();
    let frame = frame(&map);
    let seen = [hole()];
    let mut a = callout();
    placed_at(&mut a, &seen, &frame, [150.0, 150.0]);
    let (before, _) = leader(&a, &frame);
    assert!(before[0] > CENTER[0] && before[1] > CENTER[1]);
    // Dragged to the lower left: the arrow goes round to that side.
    moved(&mut a, &seen, &frame, [-60.0, -40.0]);
    assert_eq!(a.text_offset, Some([-30.0, -30.0]));
    let (after, knee) = leader(&a, &frame);
    assert!(near(knee, [90.0, 110.0]), "{knee:?}");
    let expected = add2(CENTER, mul2([-FRAC_1_SQRT_2, -FRAC_1_SQRT_2], RADIUS));
    assert!(near(after, expected), "{after:?}");
    // Its leader snaps to 45° about the centre: a drag a little off it
    // lands on it, the arrow on the same line.
    let snap = snap_leader(&a, &seen, &frame, &[], [-1.0, 0.0]).expect("snapped");
    assert!((snap.angle - 225.0).abs() < 1e-9, "{snap:?}");
    let mut snapped = a.clone();
    moved(&mut snapped, &seen, &frame, snap.to);
    let (_, knee) = leader(&snapped, &frame);
    let run = sub2(knee, CENTER);
    assert!((run[0] - run[1]).abs() < 1e-9, "{run:?}");
}

#[test]
fn a_hole_callout_saved_before_keeps_its_text_where_it_was_drawn() {
    let map = map();
    let frame = frame(&map);
    let seen = [hole()];
    // Written before mitcad#2212: its text from its arrow at the circle's
    // right (no click kept), or at the point nearest where it was clicked.
    let old: ViewAnnotation = serde_json::from_value(serde_json::json!({
        "id": 1, "type": "hole_callout", "picks": [], "text_offset": [-40.0, 8.0]
    }))
    .unwrap();
    assert!(!old.text_from_center);
    let was = add2(add2(CENTER, [RADIUS, 0.0]), [-40.0, 8.0]);
    let (tip, knee) = leader(&old, &frame);
    assert!(near(knee, was), "{knee:?}");
    // Its text now left of the hole: the arrow on that side.
    assert!(tip[0] < CENTER[0], "{tip:?}");
    let mut clicked = old.clone();
    clicked.at = Some([10.0, 23.0]);
    let (_, knee) = leader(&clicked, &frame);
    assert!(near(knee, add2([120.0, 146.0], [-40.0, 8.0])), "{knee:?}");
    // Its first move stores its text from the centre, where it was drawn
    // and moved; written so from then on.
    let mut moved_once = old.clone();
    moved(&mut moved_once, &seen, &frame, [0.0, 5.0]);
    assert!(moved_once.text_from_center);
    assert_eq!(
        moved_once.text_offset,
        Some(add2(sub2(was, CENTER), [0.0, 5.0]))
    );
    let (_, knee) = leader(&moved_once, &frame);
    assert!(near(knee, add2(was, [0.0, 5.0])), "{knee:?}");
    let written = serde_json::to_value(&moved_once).unwrap();
    assert_eq!(written["text_from_center"], true);
    assert!(
        serde_json::to_value(&old)
            .unwrap()
            .get("text_from_center")
            .is_none()
    );
    // Only a hole callout keeps its text from a centre.
    let pick = ViewPick {
        body: "F2.b0".parse().unwrap(),
        occurrence: None,
        edge: Some("E{F2:start|F2:side(c1[c4,c2])}".parse().unwrap()),
        face: None,
        point: None,
        on_face: None,
        on_cut: None,
        drawn: None,
        side: None,
        outline: None,
        extreme: None,
        bend: None,
    };
    let mut kept = callout();
    kept.picks = vec![pick.clone()];
    kept.text_from_center = true;
    assert_eq!(kept.check(), Ok(()));
    let mut note = ViewAnnotation::new(2, AnnotationType::Note, vec![pick]);
    note.text = Some("A".to_owned());
    assert_eq!(note.check(), Ok(()));
    note.text_from_center = true;
    assert!(
        note.check()
            .unwrap_err()
            .contains("keeps its text from its arrow"),
        "{:?}",
        note.check()
    );
}

#[test]
fn a_hole_callout_on_a_round_face_seen_from_the_side_keeps_its_arrow() {
    let map = map();
    let frame = frame(&map);
    let side = [Seen::Axis {
        a: [10.0, 0.0],
        b: [10.0, 20.0],
        radius: 3.0,
    }];
    let mut a = callout();
    a.text_from_center = true;
    placed_at(&mut a, &side, &frame, [150.0, 130.0]);
    let tip = leader_tip(&a, &side[0], &frame);
    let (from, knee) = leader_line(&a, &side, &frame, &[]).unwrap();
    assert!(near(from, tip));
    assert!(near(knee, [150.0, 130.0]));
    moved(&mut a, &side, &frame, [5.0, 0.0]);
    let (again, knee) = leader_line(&a, &side, &frame, &[]).unwrap();
    assert!(near(again, tip), "the arrow stays: {again:?}");
    assert!(near(knee, [155.0, 130.0]));
}
