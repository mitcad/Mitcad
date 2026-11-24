// SPDX-License-Identifier: MIT
//! Weld symbols on a drawing's views (mitcad#2350): a weld symbol
//! annotation ([`WeldSymbol`]) drawn by the drawing's standard.
//!
//! - **ISO 2553** (the drawing's standard ISO): the continuous reference
//!   line with the dashed identification line under it. The arrow side's
//!   weld stands on the continuous line (above it), the other side's hangs
//!   from the dashed line (below it); a symmetrical weld (both sides of
//!   one kind) has no dashed line, its two symbols on the continuous line.
//!   A fillet's size is written with its letter: `a5` (throat), `z7`
//!   (leg); a penetration `s8`; an intermittent weld `n×l (e)`, `e` the gap
//!   between its welds.
//! - **AWS A2.4** (the drawing's standard ASME): one reference line, the
//!   arrow side's weld below it, the other side's above. A fillet's leg is
//!   written bare, a groove's depth and its effective throat `6 (8)`; an
//!   intermittent weld `length-pitch`.
//!
//! In both, sizes stand left of the symbol and lengths right of it, read
//! from below whichever way the line runs from the arrow; a fillet's,
//! a bevel's and a J's upright leg is on the left. The circle of a weld
//! all around and the flag of a field weld are where the arrow's line
//! meets the reference line; the tail (a fork) with its text at the
//! line's other end. A contour (flush, convex, concave) is drawn beyond
//! the symbol, a finish's letter beyond that, a weld's toes blended as two
//! small arcs at the contour's ends. Staggered intermittent welds have the
//! other side's symbol offset along the line.
//!
//! Every part is a thin line, an arc, a filled polygon or a text of the
//! sheet ([`Item`]), so the PDF, SVG, DXF and DWG writers carry the
//! symbol as drawn.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use mitcad_vec3::vec2::{norm as len2, sub as sub2};

use super::P2;
use super::Standard;
use super::annotate::{FilletMeasure, WeldContour, WeldKind, WeldSide, WeldSymbol, unit2};
use super::compose::arrow;
use super::sheet::{Anchor, Group, Item, Pen, text_width};

/// A symbol's height (and its unit of width) per letter height.
pub const SYMBOL: f64 = 1.2;
/// How far under the reference line ISO 2553's dashed identification line
/// runs, per letter height.
pub const IDENTIFICATION_GAP: f64 = 0.4;
/// The room on the reference line at the arrow's end (for the flag and
/// the circle) and at its other end, per letter height.
const LEAD: f64 = 1.5;
const TRAIL: f64 = 0.8;
/// Between a size's text and its symbol, per letter height.
const TEXT_GAP: f64 = 0.35;
/// The shortest reference line, per letter height.
const SHORTEST: f64 = 5.0;
/// An all around circle's radius, per letter height.
const ALL_AROUND: f64 = 0.4;
/// A field weld's staff, from where the arrow's line meets the reference
/// line, and its flag's length, per letter height.
const STAFF: f64 = 1.9;
const FLAG: f64 = 1.0;
/// The tail's fork: along the line and to each side, per letter height.
const FORK: f64 = 0.9;
/// How far a staggered other side's symbol is offset, in symbol widths.
const STAGGER: f64 = 1.5;

/// A part of a basic or supplementary symbol in its own units: `u` along
/// the reference line from the symbol's middle, `v` away from the line
/// (toward the side it is drawn on), one unit a symbol's height.
#[derive(Debug, Clone, Copy)]
enum Part {
    Line(f64, f64, f64, f64),
    /// About (`u`, `v`), radius, counter-clockwise from the first angle
    /// to the second (radians, `u` toward `v`).
    Arc(f64, f64, f64, f64, f64),
}

/// A basic symbol's parts, its width and its height (units).
fn parts(kind: WeldKind) -> (Vec<Part>, f64, f64) {
    use Part::{Arc, Line};
    let q = PI / 4.0;
    match kind {
        WeldKind::Fillet => (
            vec![Line(-0.5, 0.0, -0.5, 1.0), Line(-0.5, 1.0, 0.5, 0.0)],
            1.0,
            1.0,
        ),
        WeldKind::Square => (
            vec![Line(-0.25, 0.0, -0.25, 1.0), Line(0.25, 0.0, 0.25, 1.0)],
            0.5,
            1.0,
        ),
        WeldKind::V => (
            vec![Line(0.0, 0.0, -0.5, 1.0), Line(0.0, 0.0, 0.5, 1.0)],
            1.0,
            1.0,
        ),
        WeldKind::Bevel => (
            vec![Line(-0.3, 0.0, -0.3, 1.0), Line(-0.3, 0.0, 0.3, 1.0)],
            0.6,
            1.0,
        ),
        WeldKind::Y => (
            vec![
                Line(0.0, 0.0, 0.0, 0.4),
                Line(0.0, 0.4, -0.45, 1.0),
                Line(0.0, 0.4, 0.45, 1.0),
            ],
            0.9,
            1.0,
        ),
        WeldKind::U => (
            vec![
                Line(0.0, 0.0, 0.0, 0.3),
                Arc(0.0, 0.7, 0.4, PI, TAU),
                Line(-0.4, 0.7, -0.4, 1.0),
                Line(0.4, 0.7, 0.4, 1.0),
            ],
            0.8,
            1.0,
        ),
        WeldKind::J => (
            vec![
                Line(-0.3, 0.0, -0.3, 1.0),
                Arc(-0.3, 0.7, 0.5, 3.0 * FRAC_PI_2, TAU),
                Line(0.2, 0.7, 0.2, 1.0),
            ],
            0.6,
            1.0,
        ),
        WeldKind::FlareV => (
            vec![
                Arc(-0.85, 0.5, 0.7, -q, q),
                Arc(0.85, 0.5, 0.7, 3.0 * q, 5.0 * q),
            ],
            0.7,
            1.0,
        ),
        WeldKind::FlareBevel => (
            vec![
                Line(-0.25, 0.0, -0.25, 1.0),
                Arc(0.85, 0.5, 0.7, 3.0 * q, 5.0 * q),
            ],
            0.6,
            1.0,
        ),
        WeldKind::Plug => (
            vec![
                Line(-0.45, 0.0, -0.45, 0.5),
                Line(-0.45, 0.5, 0.45, 0.5),
                Line(0.45, 0.5, 0.45, 0.0),
            ],
            0.9,
            0.5,
        ),
        WeldKind::Spot => (vec![Arc(0.0, 0.4, 0.4, 0.0, TAU)], 0.8, 0.8),
        WeldKind::Seam => (
            vec![
                Arc(0.0, 0.4, 0.4, 0.0, TAU),
                Line(-0.6, 0.3, 0.6, 0.3),
                Line(-0.6, 0.5, 0.6, 0.5),
            ],
            1.2,
            0.8,
        ),
        WeldKind::BackWeld => (vec![Arc(0.0, 0.0, 0.45, 0.0, PI)], 0.9, 0.45),
        WeldKind::Backing => (
            vec![
                Line(-0.5, 0.0, -0.5, 0.4),
                Line(-0.5, 0.4, 0.5, 0.4),
                Line(0.5, 0.4, 0.5, 0.0),
                Line(-0.5, 0.0, 0.5, 0.0),
            ],
            1.0,
            0.4,
        ),
        WeldKind::Surfacing => (
            vec![
                Arc(-0.25, 0.0, 0.25, 0.0, PI),
                Arc(0.25, 0.0, 0.25, 0.0, PI),
            ],
            1.0,
            0.25,
        ),
    }
}

/// A contour's parts at `top` (units): flush a straight line, convex an
/// arc bulging away from the line, concave one bulging toward it.
fn contour_parts(contour: WeldContour, top: f64) -> Vec<Part> {
    // A chord of 1 unit, the arc's middle 0.18 units off it.
    let (rise, r): (f64, f64) = (0.6, 0.6f64.hypot(0.5));
    let a = rise.atan2(0.5);
    match contour {
        WeldContour::Flush => vec![Part::Line(-0.5, top, 0.5, top)],
        WeldContour::Convex => vec![Part::Arc(0.0, top - rise, r, a, PI - a)],
        WeldContour::Concave => vec![Part::Arc(0.0, top + rise, r, PI + a, TAU - a)],
    }
}

/// A side's place on the sheet: its symbol's middle on the line it stands
/// on, which way is away from that line (1 up, -1 down) and the symbol's
/// size (paper mm).
#[derive(Debug, Clone, Copy)]
struct Place {
    middle: P2,
    away: f64,
    s: f64,
}

impl Place {
    fn at(&self, u: f64, v: f64) -> P2 {
        [
            self.middle[0] + u * self.s,
            self.middle[1] + self.away * v * self.s,
        ]
    }

    fn draw(&self, g: &mut Group, part: Part) {
        match part {
            Part::Line(u1, v1, u2, v2) => g.line(Pen::Thin, self.at(u1, v1), self.at(u2, v2)),
            Part::Arc(u, v, r, a0, a1) => {
                // Mirrored below the line: the angles turn the other way.
                let start = if self.away > 0.0 { a0 } else { -a1 };
                let start = start.rem_euclid(TAU);
                g.items.push(Item::Arc {
                    pen: Pen::Thin,
                    center: self.at(u, v),
                    radius: r * self.s,
                    start,
                    end: start + (a1 - a0),
                });
            }
        }
    }
}

/// A size as the drawing writes it: up to two decimals, no trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// What a side's weld writes left of its symbol: its size and penetration.
pub fn size_text(side: &WeldSide, standard: Standard) -> String {
    let size = side.size.map(number);
    let penetration = side.penetration.map(number);
    match standard {
        Standard::Iso => {
            let own = match side.symbol {
                WeldKind::Fillet => size.map(|s| match side.measure.unwrap_or_default() {
                    FilletMeasure::Throat => format!("a{s}"),
                    FilletMeasure::Leg => format!("z{s}"),
                }),
                WeldKind::Spot | WeldKind::Plug => size.map(|s| format!("d{s}")),
                WeldKind::Seam => size.map(|s| format!("c{s}")),
                // A groove writes its penetration in place of its depth.
                kind if kind.is_groove() && penetration.is_some() => None,
                _ => size,
            };
            format!(
                "{}{}",
                penetration.map(|p| format!("s{p}")).unwrap_or_default(),
                own.unwrap_or_default()
            )
        }
        Standard::Asme => {
            let own = match (side.symbol, side.measure.unwrap_or_default()) {
                (WeldKind::Fillet, FilletMeasure::Throat) if side.measure.is_some() => {
                    size.map(|s| format!("({s})"))
                }
                _ => size,
            };
            match (own, penetration) {
                (Some(s), Some(p)) => format!("{s} ({p})"),
                (None, Some(p)) => format!("({p})"),
                (Some(s), None) => s,
                (None, None) => String::new(),
            }
        }
    }
}

/// What a side's weld writes right of its symbol: its length, and an
/// intermittent weld's count and pitch (ISO 2553 the gap between welds).
pub fn length_text(side: &WeldSide, standard: Standard) -> String {
    match standard {
        Standard::Iso => {
            let count = side.count.map(|n| format!("{n}×")).unwrap_or_default();
            match (side.length, side.pitch) {
                (Some(l), Some(p)) => format!("{count}{} ({})", number(l), number(p - l)),
                (Some(l), None) => format!("{count}{}", number(l)),
                _ => side.count.map(|n| n.to_string()).unwrap_or_default(),
            }
        }
        Standard::Asme => {
            let along = match (side.length, side.pitch) {
                (Some(l), Some(p)) => format!("{}-{}", number(l), number(p)),
                (Some(l), None) => number(l),
                _ => String::new(),
            };
            match (side.count, along.is_empty()) {
                (Some(n), true) => format!("({n})"),
                (Some(n), false) => format!("{along} ({n})"),
                (None, _) => along,
            }
        }
    }
}

/// Whether ISO 2553's dashed identification line is drawn: not for a
/// symmetrical weld (both sides of one kind), nor in AWS A2.4.
pub fn identification_line(weld: &WeldSymbol, standard: Standard) -> bool {
    if standard != Standard::Iso {
        return false;
    }
    !matches!((&weld.arrow_side, &weld.other_side),
        (Some(a), Some(o)) if a.symbol == o.symbol)
}

/// Which way from the reference line each side's weld is drawn (1 above,
/// -1 below) and how far below the line the line it stands on is (paper
/// mm): ISO 2553's arrow side on the continuous line above it, its other
/// side under the dashed line; AWS A2.4's arrow side below, other side
/// above.
fn side_places(weld: &WeldSymbol, standard: Standard, h: f64) -> [(f64, f64); 2] {
    match standard {
        Standard::Iso => {
            let other_drop = if identification_line(weld, standard) {
                IDENTIFICATION_GAP * h
            } else {
                0.0
            };
            [(1.0, 0.0), (-1.0, other_drop)]
        }
        Standard::Asme => [(-1.0, 0.0), (1.0, 0.0)],
    }
}

/// A weld symbol on the sheet (mitcad#2350): its arrow at `tip` on the
/// weld, its line to `knee`, where the reference line starts and runs
/// away from the arrow (right when the knee is right of the tip or above
/// it, else left); its welds, sizes, contours and finishes, the circle and
/// flag at the knee and the tail at the line's other end, by `standard`;
/// letters `h` high.
pub fn weld_symbol(
    g: &mut Group,
    tip: P2,
    knee: P2,
    weld: &WeldSymbol,
    standard: Standard,
    h: f64,
) {
    let dir = if knee[0] >= tip[0] { 1.0 } else { -1.0 };
    if len2(sub2(knee, tip)) > 1e-6 {
        g.line(Pen::Thin, tip, knee);
        arrow(g, tip, unit2(sub2(tip, knee)));
    }
    let s = SYMBOL * h;
    let sides: [Option<&WeldSide>; 2] = [weld.arrow_side.as_ref(), weld.other_side.as_ref()];
    let places = side_places(weld, standard, h);
    // Along the line from its left end: the room at its ends, the sizes'
    // texts, the symbols (in a column, the other side's offset when
    // staggered), the lengths' texts.
    let (first, last) = if dir > 0.0 {
        (LEAD * h, TRAIL * h)
    } else {
        (TRAIL * h, LEAD * h)
    };
    let gap = TEXT_GAP * h;
    let sizes: Vec<String> = sides
        .iter()
        .map(|side| side.map(|w| size_text(w, standard)).unwrap_or_default())
        .collect();
    let lengths: Vec<String> = sides
        .iter()
        .map(|side| side.map(|w| length_text(w, standard)).unwrap_or_default())
        .collect();
    let widths: Vec<f64> = sides
        .iter()
        .map(|side| side.map_or(0.0, |w| parts(w.symbol).1 * s))
        .collect();
    let widest = widths.iter().copied().fold(0.0, f64::max);
    let left_text = sizes.iter().map(|t| text_width(t, h)).fold(0.0, f64::max);
    let middle = first + left_text + if left_text > 0.0 { gap } else { 0.0 } + widest / 2.0;
    let middles = [
        middle,
        middle + if weld.staggered { STAGGER * s } else { 0.0 },
    ];
    let mut end: f64 = 0.0;
    for (i, side) in sides.iter().enumerate() {
        if side.is_none() {
            continue;
        }
        let right = middles[i] + widths[i] / 2.0;
        let written = text_width(&lengths[i], h);
        end = end.max(if written > 0.0 {
            right + gap + written
        } else {
            right
        });
    }
    let length = (end + last).max(SHORTEST * h);
    let (left, right) = if dir > 0.0 {
        (knee[0], knee[0] + length)
    } else {
        (knee[0] - length, knee[0])
    };
    let y = knee[1];
    g.line(Pen::Thin, [left, y], [right, y]);
    if identification_line(weld, standard) {
        let dashed = y - IDENTIFICATION_GAP * h;
        g.line(Pen::Hidden, [left, dashed], [right, dashed]);
    }
    for (i, side) in sides.iter().enumerate() {
        let Some(side) = *side else {
            continue;
        };
        let (away, drop) = places[i];
        let place = Place {
            middle: [left + middles[i], y - drop],
            away,
            s,
        };
        draw_side(g, &place, side, &sizes[i], &lengths[i], widths[i], h);
    }
    // At the knee: all around, field weld.
    let mut staff_from = y;
    if weld.all_around {
        let r = ALL_AROUND * h;
        g.items.push(Item::Arc {
            pen: Pen::Thin,
            center: knee,
            radius: r,
            start: 0.0,
            end: TAU,
        });
        staff_from = y + r;
    }
    if weld.field {
        let top = [knee[0], y + STAFF * h];
        g.line(Pen::Thin, [knee[0], staff_from], top);
        let flag = vec![
            top,
            [knee[0] + dir * FLAG * h, y + (STAFF - 0.35) * h],
            [knee[0], y + (STAFF - 0.7) * h],
        ];
        g.items.push(Item::Fill {
            points: flag.clone(),
        });
        g.items.push(Item::Polyline {
            pen: Pen::Thin,
            points: flag,
            closed: true,
        });
    }
    // The tail at the other end, its fork opening away from the line.
    if let Some(tail) = weld
        .tail
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let at = [if dir > 0.0 { right } else { left }, y];
        let f = FORK * h;
        g.line(Pen::Thin, at, [at[0] + dir * f, y + f]);
        g.line(Pen::Thin, at, [at[0] + dir * f, y - f]);
        let anchor = if dir > 0.0 {
            Anchor::Start
        } else {
            Anchor::End
        };
        g.text([at[0] + dir * (f + 0.5), y - h / 2.0], h, 0.0, anchor, tail);
    }
}

/// One side's weld at its place: the symbol, its contour, finish and
/// blended toes, its size left of it and its length right of it.
fn draw_side(
    g: &mut Group,
    place: &Place,
    side: &WeldSide,
    size: &str,
    length: &str,
    width: f64,
    h: f64,
) {
    let (symbol, _, height) = parts(side.symbol);
    for part in symbol {
        place.draw(g, part);
    }
    let top = height + 0.25;
    if let Some(contour) = side.contour {
        for part in contour_parts(contour, top) {
            place.draw(g, part);
        }
    }
    if side.toes_blended {
        for u in [-0.65, 0.65] {
            place.draw(g, Part::Arc(u, top + 0.15, 0.15, PI, TAU));
        }
    }
    // A letter's baseline beyond `v` (units) on the side's side.
    let beyond = |v: f64| {
        let at = place.at(0.0, v);
        if place.away > 0.0 { at[1] } else { at[1] - h }
    };
    if let Some(finish) = side.finish {
        let v = top + if side.contour.is_some() { 0.35 } else { 0.0 };
        g.text(
            [place.middle[0], beyond(v)],
            h,
            0.0,
            Anchor::Middle,
            finish.letter(),
        );
    }
    // The texts beside the symbol, their middles at its middle's height.
    let baseline = place.at(0.0, 0.5)[1] - h / 2.0;
    let gap = TEXT_GAP * h;
    if !size.is_empty() {
        g.text(
            [place.middle[0] - width / 2.0 - gap, baseline],
            h,
            0.0,
            Anchor::End,
            size,
        );
    }
    if !length.is_empty() {
        g.text(
            [place.middle[0] + width / 2.0 + gap, baseline],
            h,
            0.0,
            Anchor::Start,
            length,
        );
    }
}

#[cfg(test)]
#[path = "weld_tests.rs"]
mod tests;
