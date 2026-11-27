// SPDX-License-Identifier: MIT
//! The sheet's SVG (what File › Print prints, mitcad#2477): hidden edges
//! dashed by the pen table, a dashed circle's dashes starting where the
//! screen and the PDF start them.

use std::f64::consts::FRAC_PI_2;

use super::*;
use crate::sheet::Group;

fn sheet(items: Vec<Item>) -> Sheet {
    let mut group = Group::new("view");
    group.id = Some("view-1".to_owned());
    group.items = items;
    Sheet {
        width: 200.0,
        height: 100.0,
        title: "Dashes".to_owned(),
        groups: vec![group],
    }
}

#[test]
fn hidden_edges_carry_the_pen_tables_width_and_dashes() {
    let svg = write(&sheet(vec![Item::Line {
        pen: Pen::Hidden,
        a: [10.0, 20.0],
        b: [50.0, 20.0],
    }]));
    let line = svg
        .lines()
        .find(|l| l.starts_with("<line "))
        .unwrap_or_else(|| panic!("no line: {svg}"));
    // From its first end to its second (where the dashes start), y down.
    assert!(
        line.contains(r#"x1="10" y1="80" x2="50" y2="80""#),
        "{line}"
    );
    assert!(line.contains(r#"stroke-width="0.18""#), "{line}");
    assert!(line.contains(r#"stroke-dasharray="3 1""#), "{line}");
    assert!(line.contains(r#"stroke-linecap="round""#), "{line}");
    assert_eq!(Pen::Hidden.width(), 0.18);
    assert_eq!(Pen::Hidden.dashes(), &[3.0, 1.0]);
}

#[test]
fn a_dashed_circle_starts_at_its_start_and_runs_counter_clockwise() {
    // A hidden hole (radius 5 about (100, 50)) whose circle starts at 90°,
    // and a visible one, continuous.
    let svg = write(&sheet(vec![
        Item::Arc {
            pen: Pen::Hidden,
            center: [100.0, 50.0],
            radius: 5.0,
            start: FRAC_PI_2,
            end: FRAC_PI_2 + TAU,
        },
        Item::Arc {
            pen: Pen::Thick,
            center: [40.0, 50.0],
            radius: 5.0,
            start: 0.0,
            end: TAU,
        },
    ]));
    // Its top (100, 55 on paper; y 45 in the SVG), then the bottom, then
    // the top again, each half with the negative sweep (counter-clockwise
    // on paper, as the screen and the PDF draw it), not a circle element,
    // which starts at 0° and runs clockwise on paper.
    let hidden = svg
        .lines()
        .find(|l| l.contains(r#"stroke-dasharray="3 1""#))
        .unwrap_or_else(|| panic!("no dashed item: {svg}"));
    assert!(
        hidden.starts_with(r#"<path d="M 100 45 A 5 5 0 0 0 100 55 A 5 5 0 0 0 100 45""#),
        "{hidden}"
    );
    assert!(hidden.contains(r#"stroke-width="0.18""#), "{hidden}");
    // The continuous circle stays a circle.
    assert!(
        svg.lines()
            .any(|l| l.starts_with(r#"<circle cx="40" cy="50" r="5""#)
                && l.contains(r#"stroke-width="0.35""#)
                && !l.contains("stroke-dasharray")),
        "{svg}"
    );
}
