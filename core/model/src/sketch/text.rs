// SPDX-License-Identifier: MIT
//! Sketch text as profile curves: the outlines of
//! the characters' glyphs, laid out at an anchor, in a frame or along a
//! curve, bound profiles like any other curves.
//!
//! - The kernel gives the glyph outlines of a font in em units
//!   ([`crate::kernel::Kernel::font_glyphs`]; OCCT's font manager, with a
//!   bundled font when the requested one is not installed). Layout is here:
//!   lines, alignment, spacing, rotation, frames and paths.
//! - Every contour of every glyph is one closed curve of the profile
//!   arrangement, named `t<n>.g<k>.c<j>` (text, the character's index in
//!   the text, the contour's index in the glyph), so a text inside a
//!   rectangle cuts holes in it and the letters' counters are islands. The
//!   contour is a composite curve: its pieces (lines and Bézier curves)
//!   become the edges of profile faces, so an extruded letter has a face
//!   per piece (`side(t5.g0.c0)#k`).

use std::collections::BTreeMap;

use serde_json::Value;

use super::edit::SketchEdit;
use super::geometry::{Curve2, Nurbs, P2, add, dist, dot, perp, scale, sub, unit};
use super::regions::RegionCurve;
use super::tools::RectangleMode;
use super::{EntityIndex, EntityKind, SketchText, TextAlign, TextVAlign};
use crate::ids::EntityUid;
use crate::kernel::{FontGlyphs, FontRequest};
use crate::topo::CurveId;

/// A glyph contour placed in the sketch: its pieces in order.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedContour {
    pub id: CurveId,
    pub parts: Vec<Curve2>,
}

/// A text laid out: the font its outlines come from and its contours.
#[derive(Debug, Clone, PartialEq)]
pub struct TextOutline {
    pub id: EntityUid,
    pub family: String,
    pub fallback: bool,
    pub contours: Vec<PlacedContour>,
}

impl TextOutline {
    /// The contours as profile curves (composite curves of their pieces).
    pub fn region_curves(&self) -> Vec<RegionCurve> {
        self.contours
            .iter()
            .filter_map(|c| {
                Some(RegionCurve {
                    uid: c.id,
                    curve: composite(&c.parts)?,
                    parts: c.parts.clone(),
                })
            })
            .collect()
    }
}

/// The font request of a text.
pub fn request(text: &SketchText) -> FontRequest<'_> {
    FontRequest {
        family: &text.font,
        bold: text.bold,
        italic: text.italic,
        text: &text.text,
    }
}

/// The Bézier control points of a piece raised to `degree`.
fn elevate(points: &[P2], degree: usize) -> Vec<P2> {
    let mut p = points.to_vec();
    while p.len() - 1 < degree {
        let n = p.len() - 1;
        let mut q = Vec::with_capacity(n + 2);
        q.push(p[0]);
        for i in 1..=n {
            let a = i as f64 / (n + 1) as f64;
            q.push(add(scale(p[i - 1], a), scale(p[i], 1.0 - a)));
        }
        q.push(p[n]);
        p = q;
    }
    p
}

/// The Bézier control points of a placed piece.
fn bezier(part: &Curve2) -> Vec<P2> {
    match part {
        Curve2::Line { a, b } => vec![*a, *b],
        Curve2::Nurbs(n) => n.control.clone(),
        _ => Vec::new(),
    }
}

/// One B-spline of the pieces end to end, piece k on parameters [k, k + 1]
/// (C0 where they meet). None for fewer than two pieces or a gap.
pub fn composite(parts: &[Curve2]) -> Option<Curve2> {
    if parts.len() < 2 {
        return None;
    }
    let controls: Vec<Vec<P2>> = parts.iter().map(bezier).collect();
    if controls.iter().any(|c| c.len() < 2) {
        return None;
    }
    let degree = controls.iter().map(|c| c.len() - 1).max()?;
    let mut control: Vec<P2> = Vec::new();
    let mut knots = vec![0.0; degree + 1];
    for (k, c) in controls.iter().enumerate() {
        let raised = elevate(c, degree);
        if let Some(last) = control.last()
            && dist(*last, raised[0]) > 1e-9 * (1.0 + raised[0][0].abs() + raised[0][1].abs())
        {
            return None;
        }
        let skip = usize::from(!control.is_empty());
        control.extend_from_slice(&raised[skip..]);
        if k + 1 < controls.len() {
            knots.extend(std::iter::repeat_n((k + 1) as f64, degree));
        }
    }
    knots.extend(std::iter::repeat_n(controls.len() as f64, degree + 1));
    Some(Curve2::Nurbs(Nurbs {
        degree,
        control,
        weights: Vec::new(),
        knots,
    }))
}

/// A piece in the sketch from its control points.
fn piece(points: &[P2]) -> Option<Curve2> {
    match points.len() {
        2 => Some(Curve2::Line {
            a: points[0],
            b: points[1],
        }),
        3 | 4 => {
            let degree = points.len() - 1;
            let mut knots = vec![0.0; degree + 1];
            knots.extend(std::iter::repeat_n(1.0, degree + 1));
            Some(Curve2::Nurbs(Nurbs {
                degree,
                control: points.to_vec(),
                weights: Vec::new(),
                knots,
            }))
        }
        _ => None,
    }
}

/// Lengths along a curve, for placing characters on it.
struct ArcLength {
    params: Vec<f64>,
    lengths: Vec<f64>,
}

impl ArcLength {
    fn new(curve: &Curve2) -> Self {
        let (lo, hi) = curve.domain();
        let coarse = curve.sample_params(lo, hi);
        let mut params = Vec::new();
        for w in coarse.windows(2) {
            for k in 0..8 {
                params.push(w[0] + (w[1] - w[0]) * k as f64 / 8.0);
            }
        }
        params.push(hi);
        let mut lengths = vec![0.0];
        for w in params.windows(2) {
            let l = lengths.last().copied().unwrap_or(0.0);
            lengths.push(l + dist(curve.point(w[0]), curve.point(w[1])));
        }
        Self { params, lengths }
    }

    fn total(&self) -> f64 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    fn param(&self, s: f64) -> f64 {
        let i = self
            .lengths
            .partition_point(|l| *l < s)
            .clamp(1, self.lengths.len() - 1);
        let (l0, l1) = (self.lengths[i - 1], self.lengths[i]);
        let f = if l1 > l0 { (s - l0) / (l1 - l0) } else { 0.0 };
        self.params[i - 1] + f.clamp(0.0, 1.0) * (self.params[i] - self.params[i - 1])
    }
}

/// How a glyph's outline (em units) maps into the sketch.
type Placement = Box<dyn Fn(P2) -> P2>;

/// Lays a text out from its font's glyphs. `points` and `curves` are the
/// solved sketch (for frames and paths).
pub fn layout(
    text: &SketchText,
    glyphs: &FontGlyphs,
    points: &BTreeMap<EntityUid, P2>,
    curves: &BTreeMap<EntityUid, Curve2>,
) -> Result<TextOutline, String> {
    let what = format!("t{}", text.id.0);
    let chars: Vec<char> = text.text.chars().collect();
    if glyphs.glyphs.len() != chars.len() {
        return Err(format!(
            "{what}: the font gave no glyph for some characters"
        ));
    }
    let h = text.height;
    let stretch = 1.0 + text.spacing / 100.0;
    let advance = |k: usize| glyphs.glyphs[k].advance * stretch * h;
    // Lines of character indices.
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    for (k, c) in chars.iter().enumerate() {
        if *c == '\n' {
            lines.push(Vec::new());
        } else {
            lines.last_mut().expect("a line").push(k);
        }
    }
    let width = |line: &[usize]| line.iter().map(|k| advance(*k)).sum::<f64>();
    let (ascender, descender, spacing) = (
        glyphs.ascender * h,
        glyphs.descender * h,
        glyphs.line_spacing * h,
    );
    // Where each character's pen starts and how its outline maps into the
    // sketch: a point p of the glyph (em units) goes to `place(k, p)`.
    let mut placements: Vec<(usize, Placement)> = Vec::new();
    if let Some(path) = &text.path {
        let curve = curves
            .get(&path.curve)
            .cloned()
            .ok_or_else(|| format!("{what}: the path c{} has no geometry", path.curve.0))?;
        let table = ArcLength::new(&curve);
        let total = table.total();
        let order: Vec<usize> = lines.iter().flatten().copied().collect();
        let w = width(&order);
        let scale_fit = if path.fit && w > 0.0 { total / w } else { 1.0 };
        let start = if path.fit {
            0.0
        } else {
            match text.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => (total - w) / 2.0,
                TextAlign::Right => total - w,
            }
        };
        let closed = curve.is_closed();
        let mut x = 0.0;
        for k in order {
            let a = advance(k);
            let mut s = start + (x + a / 2.0) * scale_fit;
            if closed && total > 0.0 {
                s = s.rem_euclid(total);
            }
            let [p, d1, _] = curve.eval(table.param(s.clamp(0.0, total)));
            let t = unit(d1);
            let n = perp(t);
            let lift = if path.above { 0.0 } else { -ascender };
            let origin = add(p, add(scale(t, -a / 2.0), scale(n, lift)));
            placements.push((
                k,
                Box::new(move |g: P2| add(origin, add(scale(t, g[0] * h), scale(n, g[1] * h)))),
            ));
            x += a;
        }
    } else {
        // The block of lines in its own axes: x along the baseline, y up,
        // the first baseline at y = 0.
        let count = lines.len();
        let last_baseline = -spacing * (count - 1) as f64;
        let (origin, u, v, box_size) = if text.frame.is_empty() {
            let (s, c) = text.angle.sin_cos();
            (text.at, [c, s], [-s, c], None)
        } else {
            let corner = |i: usize| {
                points
                    .get(&text.frame[i])
                    .copied()
                    .ok_or_else(|| format!("{what}: the frame point p{} is gone", text.frame[i].0))
            };
            let (o, a, b) = (corner(0)?, corner(1)?, corner(2)?);
            let (w, hh) = (dist(o, a), dist(o, b));
            if w <= 0.0 || hh <= 0.0 {
                return Err(format!("{what}: the frame has no size"));
            }
            (o, unit(sub(a, o)), unit(sub(b, o)), Some((w, hh)))
        };
        let block_top = ascender;
        let block_bottom = last_baseline + descender;
        // The first baseline's height above the origin.
        let first = match (box_size, text.valign) {
            (None, TextVAlign::Baseline) => 0.0,
            (None, TextVAlign::Top) => -block_top,
            (None, TextVAlign::Bottom) => -block_bottom,
            (None, TextVAlign::Middle) => -(block_top + block_bottom) / 2.0,
            (Some((_, hh)), TextVAlign::Baseline | TextVAlign::Top) => hh - block_top,
            (Some(_), TextVAlign::Bottom) => -block_bottom,
            (Some((_, hh)), TextVAlign::Middle) => hh / 2.0 - (block_top + block_bottom) / 2.0,
        };
        let line_start = |w: f64| match (box_size, text.align) {
            (None, TextAlign::Left) | (Some(_), TextAlign::Left) => 0.0,
            (None, TextAlign::Center) => -w / 2.0,
            (None, TextAlign::Right) => -w,
            (Some((bw, _)), TextAlign::Center) => (bw - w) / 2.0,
            (Some((bw, _)), TextAlign::Right) => bw - w,
        };
        // Extents of the block for flips.
        let widths: Vec<f64> = lines.iter().map(|l| width(l)).collect();
        let left = widths
            .iter()
            .map(|w| line_start(*w))
            .fold(f64::INFINITY, f64::min);
        let right = widths
            .iter()
            .map(|w| line_start(*w) + w)
            .fold(f64::NEG_INFINITY, f64::max);
        let (x_mid, y_mid) = (
            (left + right) / 2.0,
            first + (block_top + block_bottom) / 2.0,
        );
        let (flip_x, flip_y) = (text.flip_x, text.flip_y);
        for (i, line) in lines.iter().enumerate() {
            let baseline = first - spacing * i as f64;
            let mut x = line_start(widths[i]);
            for &k in line {
                let pen = [x, baseline];
                placements.push((
                    k,
                    Box::new(move |g: P2| {
                        let mut q = [pen[0] + g[0] * h, pen[1] + g[1] * h];
                        if flip_x {
                            q[0] = 2.0 * x_mid - q[0];
                        }
                        if flip_y {
                            q[1] = 2.0 * y_mid - q[1];
                        }
                        add(origin, add(scale(u, q[0]), scale(v, q[1])))
                    }),
                ));
                x += advance(k);
            }
        }
    }
    let mut contours = Vec::new();
    for (k, place) in placements {
        for (j, contour) in glyphs.glyphs[k].contours.iter().enumerate() {
            let parts: Option<Vec<Curve2>> = contour
                .iter()
                .map(|points| {
                    let placed: Vec<P2> = points.iter().map(|p| place(*p)).collect();
                    piece(&placed)
                })
                .collect();
            let Some(parts) = parts else { continue };
            if composite(&parts).is_some() {
                contours.push(PlacedContour {
                    id: CurveId::Glyph {
                        text: text.id,
                        glyph: k as u32,
                        contour: j as u32,
                    },
                    parts,
                });
            }
        }
    }
    Ok(TextOutline {
        id: text.id,
        family: glyphs.family.clone(),
        fallback: glyphs.fallback,
        contours,
    })
}

/// Checks a text's placement, frame and path against the sketch.
pub fn check(text: &SketchText, index: &EntityIndex<'_>) -> Result<(), String> {
    let what = format!("text t{}", text.id.0);
    if text.text.is_empty() {
        return Err(format!("{what}: the text is empty"));
    }
    if !(text.height.is_finite() && text.height > 0.0)
        || !text.at.iter().all(|v| v.is_finite())
        || !text.angle.is_finite()
        || !(text.spacing.is_finite() && text.spacing > -100.0)
    {
        return Err(format!("{what}: invalid placement"));
    }
    if !text.frame.is_empty() {
        if text.frame.len() != 3 {
            return Err(format!("{what}: a frame has three corner points"));
        }
        if text.path.is_some() {
            return Err(format!(
                "{what}: a text is in a frame or on a path, not both"
            ));
        }
        for p in &text.frame {
            if !index.get(*p).is_some_and(|e| e.is_point()) {
                return Err(format!("{what}: the frame point p{} does not exist", p.0));
            }
        }
    }
    if let Some(path) = &text.path {
        match index.get(path.curve) {
            Some(e) if !e.is_point() => {}
            _ => {
                return Err(format!("{what}: the path c{} does not exist", path.curve.0));
            }
        }
    }
    Ok(())
}

/// The anchor and angle a text in a frame has now (its corner and its
/// x side's direction), kept in the definition for display.
pub fn frame_anchor(text: &SketchText, points: &BTreeMap<EntityUid, P2>) -> Option<(P2, f64)> {
    if text.frame.len() != 3 {
        return None;
    }
    let (o, a) = (points.get(&text.frame[0])?, points.get(&text.frame[1])?);
    let d = sub(*a, *o);
    Some((*o, d[1].atan2(d[0])))
}

impl SketchEdit<'_> {
    /// The frame of a new text: three existing points `["p3", "p4", "p6"]`
    /// (corner, x corner, y corner), or `{"corner": [x, y], "diagonal":
    /// [x, y]}`, which draws the frame as a construction rectangle along
    /// `angle` (a text box) and returns its corners.
    pub fn text_frame(&mut self, frame: &Value, angle: f64) -> Result<Vec<EntityUid>, String> {
        if let Value::Array(items) = frame {
            let points = items
                .iter()
                .map(|v| match v {
                    Value::String(s) => super::point_serde::parse(s).map_err(|e| e.to_string()),
                    _ => Err("a frame is three points like \"p3\"".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            if points.len() != 3 {
                return Err("a frame is three points: corner, x corner, y corner".to_owned());
            }
            for p in &points {
                self.at(*p)?;
            }
            return Ok(points);
        }
        let corner: P2 = serde_json::from_value(frame["corner"].clone())
            .map_err(|_| "a frame needs \"corner\" [x, y]".to_owned())?;
        let diagonal: P2 = serde_json::from_value(frame["diagonal"].clone())
            .map_err(|_| "a frame needs \"diagonal\" [x, y]".to_owned())?;
        let (s, c) = angle.sin_cos();
        let (u, v) = ([c, s], [-s, c]);
        let d = sub(diagonal, corner);
        let (dx, dy) = (dot(d, u), dot(d, v));
        let o = add(corner, add(scale(u, dx.min(0.0)), scale(v, dy.min(0.0))));
        let (w, h) = (dx.abs(), dy.abs());
        if w <= 0.0 || h <= 0.0 {
            return Err("the text frame has no area".to_owned());
        }
        let mode = if angle == 0.0 {
            RectangleMode::TwoPoint {
                a: o,
                b: add(o, [w, h]),
            }
        } else {
            let b = add(o, scale(u, w));
            RectangleMode::ThreePoint {
                a: o,
                b,
                c: add(b, scale(v, h)),
            }
        };
        let lines = self.rectangle(mode, true)?;
        let ends = |edit: &Self, l: EntityUid| match edit.def.entity(l).map(|e| &e.kind) {
            Some(EntityKind::Line { start, end, .. }) => Ok((*start, *end)),
            _ => Err("the frame's lines are missing".to_owned()),
        };
        let (p0, p1) = ends(self, lines[0])?;
        let (p3, _) = ends(self, lines[3])?;
        Ok(vec![p0, p1, p3])
    }

    /// The corner and x direction of a frame now.
    pub fn frame_corner(&self, frame: &[EntityUid]) -> Option<(P2, f64)> {
        let (o, a) = (
            self.at(*frame.first()?).ok()?,
            self.at(*frame.get(1)?).ok()?,
        );
        let d = sub(a, o);
        Some((o, d[1].atan2(d[0])))
    }
}

/// The contours that bound a letter's material rather than a counter:
/// those inside an even number of the text's other contours (an outer
/// contour, or an island inside a counter).
pub fn letter_contours(outline: &TextOutline) -> std::collections::BTreeSet<CurveId> {
    let polygons: Vec<(CurveId, Vec<P2>)> = outline
        .contours
        .iter()
        .filter_map(|c| {
            let curve = composite(&c.parts)?;
            let (lo, hi) = curve.domain();
            Some((c.id, curve.sample(lo, hi)))
        })
        .collect();
    polygons
        .iter()
        .filter(|(id, points)| {
            let probe = points[0];
            let depth = polygons
                .iter()
                .filter(|(other, polygon)| {
                    other != id && super::geometry::polygon_contains(polygon, probe)
                })
                .count();
            depth % 2 == 0
        })
        .map(|(id, _)| *id)
        .collect()
}
