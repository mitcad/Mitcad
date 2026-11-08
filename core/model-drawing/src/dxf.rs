// SPDX-License-Identifier: MIT
//! A sheet as a DXF drawing in millimetres of paper: each pen on a layer of
//! its own (`VISIBLE`, `HIDDEN`, `THIN`, `HATCH`, `SECTION_LINE`, `FRAME`,
//! `EXTRA_THICK`; lines and arcs drawn by hand dashed or long dash dotted
//! and wide or extra wide on `HIDDEN_THICK`, `HIDDEN_EXTRA_THICK`,
//! `SECTION_LINE_THICK`, `SECTION_LINE_EXTRA_THICK`, mitcad#1447), texts on
//! `TEXT`, arrowheads as their outlines. The writer has no dashed line
//! types or line weights, so hidden edges and extra wide lines are told
//! apart by their layer.
//! `export_drawing` writes it in the file's length unit
//! (`mitcad_model::exchange::dxf_in_unit`, mitcad#1408): the sheet in
//! inches of paper for an inch design, its texts scaled with it.

use mitcad_dxf::{Drawing, Geometry, Layer, Point2, Text, Units};

use super::sheet::{Anchor, Item, Pen, Sheet, text_width};

const PENS: [Pen; 11] = [
    Pen::Thick,
    Pen::Hidden,
    Pen::Thin,
    Pen::Hatch,
    Pen::Chain,
    Pen::Frame,
    Pen::ExtraThick,
    Pen::DashedThick,
    Pen::DashedExtraThick,
    Pen::ChainThick,
    Pen::ChainExtraThick,
];

fn colour(pen: Pen) -> i16 {
    match pen {
        Pen::Thick | Pen::ExtraThick | Pen::Frame => 7,
        Pen::Hidden | Pen::DashedThick | Pen::DashedExtraThick => 8,
        Pen::Thin => 3,
        Pen::Hatch => 9,
        Pen::Chain | Pen::ChainThick | Pen::ChainExtraThick => 1,
    }
}

/// The sheet as a DXF drawing.
pub fn drawing(sheet: &Sheet) -> Drawing {
    let mut out = Drawing::new(Units::Millimeters);
    for pen in PENS {
        let mut layer = Layer::new(pen.layer());
        layer.color = colour(pen);
        out.layers.push(layer);
    }
    out.layers.push(Layer::new("TEXT"));
    let p = |q: [f64; 2]| Point2::new(q[0], q[1]);
    for group in &sheet.groups {
        for item in &group.items {
            match item {
                Item::Line { pen, a, b } => out.push(
                    pen.layer(),
                    Geometry::Line {
                        start: p(*a),
                        end: p(*b),
                    },
                ),
                Item::Arc {
                    pen,
                    center,
                    radius,
                    start,
                    end,
                } => {
                    let geometry = if end - start >= std::f64::consts::TAU - 1e-9 {
                        Geometry::Circle {
                            center: p(*center),
                            radius: *radius,
                        }
                    } else {
                        Geometry::arc(p(*center), *radius, *start, *end)
                    };
                    out.push(pen.layer(), geometry);
                }
                Item::Polyline {
                    pen,
                    points,
                    closed,
                } => {
                    let count = points.len();
                    let segments = if *closed {
                        count
                    } else {
                        count.saturating_sub(1)
                    };
                    for i in 0..segments {
                        out.push(
                            pen.layer(),
                            Geometry::Line {
                                start: p(points[i]),
                                end: p(points[(i + 1) % count]),
                            },
                        );
                    }
                }
                Item::Fill { points } => {
                    for i in 0..points.len() {
                        out.push(
                            Pen::Thin.layer(),
                            Geometry::Line {
                                start: p(points[i]),
                                end: p(points[(i + 1) % points.len()]),
                            },
                        );
                    }
                }
                // A DXF drawing of lines has no shading.
                Item::Shade { .. } => {}
                Item::Text {
                    at,
                    height,
                    angle,
                    anchor,
                    text,
                } => {
                    let shift = match anchor {
                        Anchor::Start => 0.0,
                        Anchor::Middle => text_width(text, *height) / 2.0,
                        Anchor::End => text_width(text, *height),
                    };
                    let position = [at[0] - shift * angle.cos(), at[1] - shift * angle.sin()];
                    out.push(
                        "TEXT",
                        Geometry::Text(Text {
                            position: p(position),
                            height: *height,
                            rotation: *angle,
                            content: text.clone(),
                        }),
                    );
                }
            }
        }
    }
    out
}
