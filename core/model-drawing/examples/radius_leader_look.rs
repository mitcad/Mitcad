// SPDX-License-Identifier: MIT
//! Pictures of a radius dimension on a small fillet (mitcad#1770): a sheet
//! of cells, each a fillet's arc of 1.5 mm at 2:1 between its two straight
//! edges (a boss's wall and a flange's top, as a fillet round a turned
//! part seen from the side has them, on the right and on the left of its
//! axis; and a quarter arc the other way round) with its radius laid out as
//! the drawings lay it out, its text placed in eight directions round the
//! arc's centre: in the arc's sweep, on its hollow side and beyond either
//! end. A one-off visual check, run by `tools/look-drawing-radius-leader.sh`
//! (also against an older tree, for the pictures before a change); it uses
//! the crate's public layout only.
//!
//! Usage: `cargo run -p mitcad-model-drawing --example radius_leader_look
//! -- <out.svg> [<title>]`

use std::f64::consts::PI;

use mitcad_model_drawing::annotate::{
    AnnotationType, DIMENSION_TEXT, Frame, Seen, ValueText, ViewAnnotation, lay_out,
};
use mitcad_model_drawing::clip::ViewMap;
use mitcad_model_drawing::compose::Bounds;
use mitcad_model_drawing::sheet::{Anchor, Group, Item, Pen, Sheet};
use mitcad_model_drawing::svg;

/// The fillet's radius (model mm) and the view's scale.
const RADIUS: f64 = 1.5;
const SCALE: f64 = 2.0;
/// A cell's side and how far the text is from the arc's centre (paper mm).
const CELL: f64 = 50.0;
const REACH: f64 = 15.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .expect("usage: radius_leader_look <out.svg> [<title>]");
    let title = args.next().unwrap_or_else(|| "radius leader".to_owned());
    // Each row: an arc (degrees, counter-clockwise) and what it is.
    let rows = [
        ((180.0, 270.0), "fillet, right of the axis"),
        ((270.0, 360.0), "fillet, left of the axis"),
        ((0.0, 90.0), "quarter arc 0-90"),
    ];
    let directions = [20.0, 45.0, 110.0, 135.0, 200.0, 225.0, 290.0, 315.0];
    let width = CELL * directions.len() as f64 + 20.0;
    let height = CELL * rows.len() as f64 + 30.0;
    let mut groups = Vec::new();
    let mut heading = Group::new("note");
    heading.text(
        [10.0, height - 12.0],
        5.0,
        0.0,
        Anchor::Start,
        title.clone(),
    );
    groups.push(heading);
    let words = ValueText {
        text: "R1.5".to_owned(),
        main: "R1.5".to_owned(),
        ..ValueText::default()
    };
    for (row, ((start, end), what)) in rows.iter().enumerate() {
        let top = height - 25.0 - CELL * row as f64;
        let mut label = Group::new("note");
        label.text([10.0, top - 4.0], 2.5, 0.0, Anchor::Start, *what);
        groups.push(label);
        for (column, degrees) in directions.iter().enumerate() {
            // The arc's centre in the middle of its cell, the view's
            // origin there.
            let c = [10.0 + CELL * (column as f64 + 0.5), top - CELL / 2.0 - 3.0];
            let map = ViewMap::new(SCALE, [0.0, 0.0], c);
            let mut view_box = Bounds::empty();
            view_box.add([c[0] - 10.0, c[1] - 10.0]);
            view_box.add([c[0] + 10.0, c[1] + 10.0]);
            let frame = Frame {
                map: &map,
                view_box,
                text: DIMENSION_TEXT,
            };
            let (a, b) = (start * PI / 180.0, end * PI / 180.0);
            let r = RADIUS * SCALE;
            let mut edges = Group::new("view");
            edges.items.push(Item::Arc {
                pen: Pen::Thick,
                center: c,
                radius: r,
                start: a,
                end: b,
            });
            // The straight edges the arc rounds, on from its ends along
            // its tangents.
            let at = |t: f64| [c[0] + r * t.cos(), c[1] + r * t.sin()];
            let on = |p: [f64; 2], d: [f64; 2]| [p[0] + 12.0 * d[0], p[1] + 12.0 * d[1]];
            edges.line(Pen::Thick, at(a), on(at(a), [a.sin(), -a.cos()]));
            edges.line(Pen::Thick, at(b), on(at(b), [-b.sin(), b.cos()]));
            let mut caption = Group::new("note");
            caption.text(
                [c[0], top - CELL + 2.0],
                2.0,
                0.0,
                Anchor::Middle,
                format!("text at {degrees} deg"),
            );
            groups.push(edges);
            groups.push(caption);
            let fillet = Seen::Circle {
                center: [0.0, 0.0],
                radius: RADIUS,
                end_on: true,
                across: [1.0, 0.0],
                arc: Some((a, b)),
            };
            let mut radius = ViewAnnotation::new(1, AnnotationType::Radius, Vec::new());
            let t = degrees * PI / 180.0;
            radius.text_offset = Some([REACH * t.cos(), REACH * t.sin()]);
            match lay_out(&radius, &[fillet], &frame, Some(&words), &[]) {
                Ok(laid) => groups.push(laid.group),
                Err(e) => eprintln!("{what}, text at {degrees}: {e}"),
            }
        }
    }
    let sheet = Sheet {
        width,
        height,
        title,
        groups,
    };
    std::fs::write(&out, svg::write(&sheet)).unwrap_or_else(|e| panic!("{out}: {e}"));
    println!("{out}");
}
