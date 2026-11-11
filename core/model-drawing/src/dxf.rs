// SPDX-License-Identifier: MIT
//! A sheet as a DXF drawing in millimetres of paper: each pen on a layer of
//! its own (`VISIBLE`, `HIDDEN`, `THIN`, `HATCH`, `SECTION_LINE`, `FRAME`,
//! `EXTRA_THICK`; lines and arcs drawn by hand dashed or long dash dotted
//! and wide or extra wide on `HIDDEN_THICK`, `HIDDEN_EXTRA_THICK`,
//! `SECTION_LINE_THICK`, `SECTION_LINE_EXTRA_THICK`, mitcad#1447; long dash
//! double dotted on `PHANTOM`, `PHANTOM_THICK`, `PHANTOM_EXTRA_THICK`,
//! mitcad#1546), texts on `TEXT`, arrowheads and a title block's raster
//! images (mitcad#1625) as their outlines; a title block's SVG images as
//! their paths' lines, curves flattened within the drawing's deflection,
//! on a layer per colour and line weight, `IMAGE_<colour index>_<weight>`
//! (mitcad#1627). Each pen's
//! layer has its line type (`DASHED`, `CHAIN`, `PHANTOM` with the pen's
//! dashes, else `CONTINUOUS`) and its line weight (the pen's width in
//! hundredths of a millimetre) since mitcad#1546, so that another program
//! draws the lines as the sheet does.
//! `export_drawing` writes it in the file's length unit
//! (`mitcad_model::exchange::dxf_in_unit`, mitcad#1408): the sheet in
//! inches of paper for an inch design, its texts and dashes scaled with it.

use mitcad_dxf::{Drawing, Geometry, Layer, Linetype, Point2, Text, Units};

use super::P2;
use super::sheet::{Anchor, Item, Pen, Sheet, text_width};
use super::svg_image::{self, Shape};
use super::template::ImageKind;

/// The standard line weights, hundredths of a millimetre.
const LINE_WEIGHTS: [i16; 24] = [
    0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200,
    211,
];

/// The colour index nearest an sRGB colour (0 to 1 each): 7, which a
/// program draws black on white paper, for black, 255 for white.
fn colour_index(rgb: [f64; 3]) -> i16 {
    let target = rgb.map(|c| c * 255.0);
    let mut best = (7, f64::MAX);
    for i in 1..=255u8 {
        let c = if i == 7 {
            [0; 3]
        } else {
            mitcad_dwg::objects::aci_rgb(i)
        };
        let d: f64 = (0..3).map(|k| (f64::from(c[k]) - target[k]).powi(2)).sum();
        if d < best.1 {
            best = (i16::from(i), d);
        }
    }
    best.0
}

/// The layer of a title block image's lines of this colour index and
/// width (mm), `IMAGE_<colour index>_<weight>`, added when it is new.
fn image_layer(out: &mut Drawing, colour: i16, width: f64) -> String {
    let hundredths = width * 100.0;
    let weight = LINE_WEIGHTS
        .iter()
        .copied()
        .min_by(|a, b| {
            (f64::from(*a) - hundredths)
                .abs()
                .total_cmp(&(f64::from(*b) - hundredths).abs())
        })
        .unwrap_or(13);
    let name = format!("IMAGE_{colour}_{weight}");
    if out.layer(&name).is_none() {
        let mut layer = Layer::new(name.clone());
        layer.color = colour;
        layer.lineweight = Some(weight);
        out.layers.push(layer);
    }
    name
}

/// A title block's SVG image on the sheet (mitcad#1627): each fill as the
/// closed outlines of its path on the layer of its colour (as it looks on
/// white paper) and the narrow line's weight, each stroke as its path's
/// lines on the layer of its colour and width; curves flattened within
/// [`svg_image::DEFLECTION`]. Clip paths are not applied.
fn vector_image(out: &mut Drawing, shapes: &[Shape]) {
    let p = |q: P2| Point2::new(q[0], q[1]);
    for shape in shapes {
        // Each layer once, closed when a fill is on it.
        let mut paints: Vec<(String, bool)> = Vec::new();
        let mut add =
            |layer: String, closed: bool| match paints.iter_mut().find(|(l, _)| *l == layer) {
                Some(paint) => paint.1 |= closed,
                None => paints.push((layer, closed)),
            };
        let fill = shape
            .fill
            .map(|f| image_layer(out, colour_index(f.colour.on_white()), Pen::Thin.width()));
        let stroke = shape
            .stroke
            .as_ref()
            .map(|s| image_layer(out, colour_index(s.colour.on_white()), s.width));
        let (first, second) = if shape.stroke_first {
            ((stroke, false), (fill, true))
        } else {
            ((fill, true), (stroke, false))
        };
        for (layer, closed) in [first, second] {
            if let Some(layer) = layer {
                add(layer, closed);
            }
        }
        for (layer, fill) in &paints {
            for subpath in &shape.subpaths {
                let points = subpath.flattened(svg_image::DEFLECTION);
                let count = points.len();
                let segments = if *fill || subpath.closed {
                    count
                } else {
                    count.saturating_sub(1)
                };
                for i in 0..segments {
                    let (a, b) = (points[i], points[(i + 1) % count]);
                    if a != b {
                        out.push(
                            layer.as_str(),
                            Geometry::Line {
                                start: p(a),
                                end: p(b),
                            },
                        );
                    }
                }
            }
        }
    }
}

const PENS: [Pen; 14] = [
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
    Pen::Phantom,
    Pen::PhantomThick,
    Pen::PhantomExtraThick,
];

fn colour(pen: Pen) -> i16 {
    match pen {
        Pen::Thick | Pen::ExtraThick | Pen::Frame => 7,
        Pen::Hidden | Pen::DashedThick | Pen::DashedExtraThick => 8,
        Pen::Thin => 3,
        Pen::Hatch => 9,
        Pen::Chain | Pen::ChainThick | Pen::ChainExtraThick => 1,
        Pen::Phantom | Pen::PhantomThick | Pen::PhantomExtraThick => 6,
    }
}

/// What a line type of a pen's pattern is called in words.
fn line_type_description(name: &str) -> &'static str {
    match name {
        "DASHED" => "Dashed __ __ __",
        "CHAIN" => "Long dash dotted ____ . ____ .",
        "PHANTOM" => "Long dash double dotted ____ . . ____ . .",
        _ => "",
    }
}

/// The sheet as a DXF drawing.
pub fn drawing(sheet: &Sheet) -> Drawing {
    let mut out = Drawing::new(Units::Millimeters);
    for pen in PENS {
        let mut layer = Layer::new(pen.layer());
        layer.color = colour(pen);
        layer.linetype = pen.line_type().to_owned();
        // Hundredths of a millimetre: 13, 18, 35, 50, 70, all of them
        // standard weights.
        layer.lineweight = Some((pen.width() * 100.0).round() as i16);
        out.layers.push(layer);
        if !pen.dashes().is_empty() && !out.linetypes.iter().any(|l| l.name == pen.line_type()) {
            out.linetypes.push(Linetype::of_dashes(
                pen.line_type(),
                line_type_description(pen.line_type()),
                pen.dashes(),
            ));
        }
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
                // A title block's SVG image as its paths' lines
                // (mitcad#1627); a raster image (a DXF drawing has none),
                // or an SVG file that cannot be read, as its outline
                // (mitcad#1625), which the export warns of.
                Item::Image {
                    at,
                    size,
                    kind,
                    data,
                } => {
                    if *kind == ImageKind::Svg
                        && let Ok(svg) = svg_image::of_base64(data.base64())
                    {
                        vector_image(&mut out, &svg.placed(*at, *size));
                        continue;
                    }
                    let corners = [
                        *at,
                        [at[0] + size[0], at[1]],
                        [at[0] + size[0], at[1] + size[1]],
                        [at[0], at[1] + size[1]],
                    ];
                    for i in 0..4 {
                        out.push(
                            Pen::Thin.layer(),
                            Geometry::Line {
                                start: p(corners[i]),
                                end: p(corners[(i + 1) % 4]),
                            },
                        );
                    }
                }
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
