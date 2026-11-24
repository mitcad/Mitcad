// SPDX-License-Identifier: MIT
//! Pictures of weld symbols (mitcad#2350) for the owner: a sheet of cells,
//! the upper half by ISO 2553 (a drawing whose standard is ISO), the lower
//! by AWS A2.4 (ASME): each basic symbol on the arrow side with a size, then
//! the other side alone, a symmetrical V, a V with a back weld, staggered
//! intermittent fillets, all around with a field weld and a tail, the
//! contours with a finish, blended toes, and a reference line running left.
//! A one-off visual check, run by `tools/look-drawing-weld.sh`; it uses the
//! crate's public layout only.
//!
//! Usage: `cargo run -p mitcad-model-drawing --example weld_look --
//! <out.svg> [<title>]`

use mitcad_model_drawing::Standard;
use mitcad_model_drawing::annotate::{
    DIMENSION_TEXT, WeldContour, WeldFinish, WeldKind, WeldSide, WeldSymbol,
};
use mitcad_model_drawing::sheet::{Anchor, Group, Pen, Sheet};
use mitcad_model_drawing::svg;
use mitcad_model_drawing::weld::weld_symbol;

/// A cell's width and height, paper mm; cells in a row.
const CELL_W: f64 = 52.0;
const CELL_H: f64 = 34.0;
const COLUMNS: usize = 8;

fn sized(kind: WeldKind, size: f64) -> WeldSide {
    WeldSide {
        size: Some(size),
        ..WeldSide::new(kind)
    }
}

/// The cells: what each shows and its symbol, and whether its line runs
/// left.
fn cells() -> Vec<(String, WeldSymbol, bool)> {
    let mut out: Vec<(String, WeldSymbol, bool)> = WeldKind::ALL
        .iter()
        .map(|kind| {
            let side = match kind {
                WeldKind::Fillet => sized(*kind, 5.0),
                kind if kind.is_groove() => WeldSide {
                    penetration: Some(8.0),
                    ..sized(*kind, 10.0)
                },
                _ => sized(*kind, 6.0),
            };
            (kind.words().to_owned(), WeldSymbol::arrow(side), false)
        })
        .collect();
    let intermittent = WeldSide {
        count: Some(4),
        length: Some(50.0),
        pitch: Some(150.0),
        ..sized(WeldKind::Fillet, 4.0)
    };
    out.extend([
        (
            "other side only".to_owned(),
            WeldSymbol {
                other_side: Some(sized(WeldKind::Fillet, 5.0)),
                ..WeldSymbol::default()
            },
            false,
        ),
        (
            "symmetrical V".to_owned(),
            WeldSymbol {
                arrow_side: Some(sized(WeldKind::V, 8.0)),
                other_side: Some(sized(WeldKind::V, 8.0)),
                ..WeldSymbol::default()
            },
            false,
        ),
        (
            "V with back weld".to_owned(),
            WeldSymbol {
                arrow_side: Some(sized(WeldKind::V, 10.0)),
                other_side: Some(WeldSide::new(WeldKind::BackWeld)),
                ..WeldSymbol::default()
            },
            false,
        ),
        (
            "staggered intermittent".to_owned(),
            WeldSymbol {
                arrow_side: Some(intermittent.clone()),
                other_side: Some(intermittent),
                staggered: true,
                ..WeldSymbol::default()
            },
            false,
        ),
        (
            "all around, field, tail".to_owned(),
            WeldSymbol {
                all_around: true,
                field: true,
                tail: Some("135".to_owned()),
                ..WeldSymbol::arrow(sized(WeldKind::Fillet, 4.0))
            },
            false,
        ),
        (
            "flush, ground".to_owned(),
            WeldSymbol::arrow(WeldSide {
                contour: Some(WeldContour::Flush),
                finish: Some(WeldFinish::Grind),
                ..sized(WeldKind::V, 10.0)
            }),
            false,
        ),
        (
            "convex / concave".to_owned(),
            WeldSymbol {
                arrow_side: Some(WeldSide {
                    contour: Some(WeldContour::Convex),
                    ..sized(WeldKind::V, 10.0)
                }),
                other_side: Some(WeldSide {
                    contour: Some(WeldContour::Concave),
                    ..sized(WeldKind::Fillet, 5.0)
                }),
                ..WeldSymbol::default()
            },
            false,
        ),
        (
            "toes blended".to_owned(),
            WeldSymbol::arrow(WeldSide {
                toes_blended: true,
                ..sized(WeldKind::Fillet, 6.0)
            }),
            false,
        ),
        (
            "line running left".to_owned(),
            WeldSymbol {
                tail: Some("111".to_owned()),
                ..WeldSymbol::arrow(sized(WeldKind::Fillet, 5.0))
            },
            true,
        ),
    ]);
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("usage: weld_look <out.svg> [<title>]");
    let title = args.next().unwrap_or_else(|| "weld symbols".to_owned());
    let cells = cells();
    let rows = cells.len().div_ceil(COLUMNS);
    let half = rows as f64 * CELL_H + 14.0;
    let width = CELL_W * COLUMNS as f64 + 20.0;
    let height = 2.0 * half + 20.0;
    let mut groups = Vec::new();
    let mut heading = Group::new("note");
    heading.text(
        [10.0, height - 10.0],
        5.0,
        0.0,
        Anchor::Start,
        title.clone(),
    );
    groups.push(heading);
    for (k, (standard, name)) in [
        (Standard::Iso, "ISO 2553 (drawing standard ISO)"),
        (Standard::Asme, "AWS A2.4 (drawing standard ASME)"),
    ]
    .into_iter()
    .enumerate()
    {
        let top = height - 18.0 - k as f64 * half;
        let mut caption = Group::new("note");
        caption.text([10.0, top - 4.0], 3.5, 0.0, Anchor::Start, name);
        groups.push(caption);
        for (i, (what, weld, left)) in cells.iter().enumerate() {
            let (row, column) = (i / COLUMNS, i % COLUMNS);
            let x0 = 10.0 + CELL_W * column as f64;
            let y0 = top - 10.0 - CELL_H * (row as f64 + 1.0);
            // The weld's edge: a thick line along the cell's bottom, the
            // arrow on it, the reference line up and to the side.
            let mut edge = Group::new("view");
            edge.line(
                Pen::Thick,
                [x0 + 4.0, y0 + 6.0],
                [x0 + CELL_W - 4.0, y0 + 6.0],
            );
            groups.push(edge);
            let (tip, knee) = if *left {
                (
                    [x0 + CELL_W - 8.0, y0 + 6.0],
                    [x0 + CELL_W - 16.0, y0 + 16.0],
                )
            } else {
                ([x0 + 8.0, y0 + 6.0], [x0 + 14.0, y0 + 16.0])
            };
            let mut symbol = Group::new("weld-symbol");
            weld_symbol(&mut symbol, tip, knee, weld, standard, DIMENSION_TEXT);
            groups.push(symbol);
            let mut label = Group::new("note");
            label.text(
                [x0 + CELL_W / 2.0, y0 + 1.0],
                2.0,
                0.0,
                Anchor::Middle,
                what.clone(),
            );
            groups.push(label);
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
