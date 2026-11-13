// SPDX-License-Identifier: MIT
//! An SVG drawing into a sketch (mitcad#473, `sketch.import_svg`): paths,
//! rectangles, circles, ellipses, lines, polylines, polygons and texts, in
//! millimetres. Lines stay lines, Bézier curves become sketch splines of
//! their degree with the same control points, elliptical arcs become arcs
//! of circles where the drawing's transforms keep them round and arcs of
//! ellipses otherwise, so curves keep their shapes exactly. Texts become
//! sketch texts. End points closer than a micrometre's thousandth become
//! one sketch point (as a DXF drawing's).
//!
//! - Units: a user unit is a millimetre, unless the root `svg` gives its
//!   size in units (`width="80mm"`, `"3in"`, `"300px"`, a pixel being
//!   1/96 inch) together with a `viewBox`, whose units then fill that size
//!   (`preserveAspectRatio` as the SVG specification sets it: `none`, or
//!   uniformly, centred). Lengths in absolute units inside the drawing
//!   follow the same scale. The import's `scale` multiplies it all.
//! - The drawing's y runs down, the sketch's up: the drawing's lower left
//!   corner (of its viewport, or of its content without one) goes to `at`.
//! - Fill: the regions of the sketch a filled shape covers by its
//!   `fill-rule` (`nonzero` or `evenodd`) are the drawing's filled regions
//!   ([`SvgDrawing::fills`]), so an icon with holes extrudes as drawn.
//! - Strokes: with `stroke_outline`, a stroked path also becomes the band
//!   of its stroke width: its outline offset by half the width to both
//!   sides (the sketch's offset, round at corners), joined by square ends
//!   at an open path's ends; the band is filled.
//! - XML: a DOCTYPE is read (mitcad#1721), with no external DTD or entity
//!   fetched and the internal subset's entities bounded (`parse_xml`).

use std::collections::HashMap;
use std::f64::consts::TAU;

use roxmltree::{Document, Node};

use super::dxf::SharedPoints;
use super::edit::SketchEdit;
use super::geometry::{Curve2, Nurbs, P2};
use super::offset::{self, Made};
use super::{Entity, EntityKind, SketchText, TextAlign};
use crate::ids::EntityUid;

/// How a drawing goes into a sketch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgOptions {
    /// Millimetres in the sketch per millimetre of the drawing.
    pub scale: f64,
    /// Where the drawing's lower left corner goes.
    pub at: P2,
    /// Stroked paths also as the bands of their stroke width.
    pub stroke_outline: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            at: [0.0, 0.0],
            stroke_outline: false,
        }
    }
}

/// A piece of a path in sketch millimetres.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Line(P2, P2),
    /// A quadratic or cubic Bézier curve by its control points.
    Bezier(Vec<P2>),
    /// An arc of an ellipse or a circle, or the whole of one.
    Conic(Conic),
}

/// An ellipse `center + u cos t + b perp(u)/|u| sin t` (counter-clockwise),
/// run from `ta` to `tb` (either way; a whole one from 0 to 2π).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conic {
    pub center: P2,
    pub u: P2,
    pub b: f64,
    pub ta: f64,
    pub tb: f64,
    pub whole: bool,
}

impl Conic {
    fn a(&self) -> f64 {
        self.u[0].hypot(self.u[1])
    }

    fn at(&self, t: f64) -> P2 {
        let a = self.a();
        let (e1, e2) = (
            [self.u[0] / a, self.u[1] / a],
            [-self.u[1] / a, self.u[0] / a],
        );
        let (s, c) = t.sin_cos();
        [
            self.center[0] + a * c * e1[0] + self.b * s * e2[0],
            self.center[1] + a * c * e1[1] + self.b * s * e2[1],
        ]
    }

    /// A circle's (the axes the same within 1e-9).
    fn round(&self) -> bool {
        (self.a() - self.b).abs() <= 1e-9 * self.a()
    }
}

/// A subpath: its pieces in order, and whether it is closed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubPath {
    pub segments: Vec<Segment>,
    pub closed: bool,
}

/// How a filled shape decides what is inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// A shape of the drawing in sketch millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub paths: Vec<SubPath>,
    /// Filled, by this rule.
    pub fill: Option<FillRule>,
    /// Stroked, this wide (mm).
    pub stroke: Option<f64>,
}

/// A text of the drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgText {
    pub content: String,
    pub at: P2,
    pub height: f64,
    pub angle: f64,
    pub font: String,
    pub align: TextAlign,
}

/// A drawing read, in sketch millimetres.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SvgDrawing {
    pub shapes: Vec<Shape>,
    pub texts: Vec<SvgText>,
    /// The viewport's size in the sketch (mm), or the content's.
    pub size: P2,
    /// What was left out or approximated.
    pub warnings: Vec<String>,
}

/// An affine map `[a, b, c, d, e, f]`: x' = a x + c y + e, y' = b x + d y + f.
type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn compose(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

fn apply(m: &Matrix, p: P2) -> P2 {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

/// The length scale of a map (the square root of its area scale).
fn length_scale(m: &Matrix) -> f64 {
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt()
}

/// The style that reaches an element.
#[derive(Debug, Clone)]
struct Style {
    fill: bool,
    even_odd: bool,
    stroke: bool,
    stroke_width: f64,
    font_size: f64,
    font_family: String,
    anchor: TextAlign,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: true,
            even_odd: false,
            stroke: false,
            stroke_width: 1.0,
            font_size: 16.0,
            font_family: String::new(),
            anchor: TextAlign::Left,
        }
    }
}

/// The reader's state.
struct Reader<'a> {
    ids: HashMap<&'a str, Node<'a, 'a>>,
    /// User units per millimetre of the drawing (absolute lengths).
    per_mm: f64,
    drawing: SvgDrawing,
    depth: usize,
}

/// A length attribute in user units (None when missing or a percentage).
fn length(value: Option<&str>, per_mm: f64) -> Option<f64> {
    let l: svgtypes::Length = value?.trim().parse().ok()?;
    use svgtypes::LengthUnit as U;
    let mm = match l.unit {
        U::None | U::Px => return Some(l.number),
        U::Mm => 1.0,
        U::Cm => 10.0,
        U::In => 25.4,
        U::Pt => 25.4 / 72.0,
        U::Pc => 25.4 / 6.0,
        U::Em | U::Ex | U::Percent => return None,
    };
    Some(l.number * mm * per_mm)
}

/// A root size in millimetres: a plain number is millimetres, a pixel
/// 1/96 inch.
fn size_mm(value: Option<&str>) -> Option<f64> {
    let l: svgtypes::Length = value?.trim().parse().ok()?;
    use svgtypes::LengthUnit as U;
    let mm = match l.unit {
        U::None | U::Mm => 1.0,
        U::Px => 25.4 / 96.0,
        U::Cm => 10.0,
        U::In => 25.4,
        U::Pt => 25.4 / 72.0,
        U::Pc => 25.4 / 6.0,
        U::Em | U::Ex | U::Percent => return None,
    };
    Some(l.number * mm).filter(|v| v.is_finite() && *v > 0.0)
}

/// A presentation property: its `style` declaration, else its attribute.
fn property<'a>(node: &Node<'a, 'a>, name: &str) -> Option<&'a str> {
    if let Some(style) = node.attribute("style") {
        for declaration in style.split(';') {
            if let Some((key, value)) = declaration.split_once(':')
                && key.trim() == name
            {
                return Some(value.trim());
            }
        }
    }
    node.attribute(name).map(str::trim)
}

impl<'a> Reader<'a> {
    /// The style of an element from its parent's.
    fn style(&self, node: &Node<'a, 'a>, parent: &Style) -> Style {
        let mut style = parent.clone();
        if let Some(fill) = property(node, "fill") {
            style.fill = fill != "none" && fill != "transparent";
        }
        if let Some(rule) = property(node, "fill-rule") {
            style.even_odd = rule == "evenodd";
        }
        if let Some(stroke) = property(node, "stroke") {
            style.stroke = stroke != "none" && stroke != "transparent";
        }
        if let Some(width) = length(property(node, "stroke-width"), self.per_mm) {
            style.stroke_width = width;
        }
        if let Some(size) = length(property(node, "font-size"), self.per_mm) {
            style.font_size = size;
        }
        if let Some(family) = property(node, "font-family") {
            style.font_family = family
                .split(',')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_matches(['"', '\''])
                .to_owned();
        }
        if let Some(anchor) = property(node, "text-anchor") {
            style.anchor = match anchor {
                "middle" => TextAlign::Center,
                "end" => TextAlign::Right,
                _ => TextAlign::Left,
            };
        }
        style
    }

    fn warn(&mut self, message: String) {
        if !self.drawing.warnings.contains(&message) {
            self.drawing.warnings.push(message);
        }
    }

    /// An element and what it holds.
    fn element(&mut self, node: Node<'a, 'a>, matrix: &Matrix, parent: &Style) {
        if !node.is_element() || self.depth > 64 {
            return;
        }
        if property(&node, "display") == Some("none")
            || property(&node, "visibility") == Some("hidden")
        {
            return;
        }
        let mut matrix = *matrix;
        if let Some(transform) = node.attribute("transform") {
            match transform.parse::<svgtypes::Transform>() {
                Ok(t) => matrix = compose(&matrix, &[t.a, t.b, t.c, t.d, t.e, t.f]),
                Err(_) => {
                    self.warn(format!("a transform that cannot be read: {transform}"));
                    return;
                }
            }
        }
        let style = self.style(&node, parent);
        let name = node.tag_name().name();
        let number = |attr: &str| length(node.attribute(attr), self.per_mm);
        let paths: Option<Vec<SubPath>> = match name {
            "svg" | "g" | "a" | "switch" => {
                self.depth += 1;
                for child in node.children() {
                    self.element(child, &matrix, &style);
                }
                self.depth -= 1;
                None
            }
            "use" => {
                let href = node
                    .attribute("href")
                    .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")));
                let target = href
                    .and_then(|h| h.strip_prefix('#'))
                    .and_then(|id| self.ids.get(id).copied());
                if let Some(target) = target {
                    let shift = [
                        1.0,
                        0.0,
                        0.0,
                        1.0,
                        number("x").unwrap_or(0.0),
                        number("y").unwrap_or(0.0),
                    ];
                    let moved = compose(&matrix, &shift);
                    self.depth += 1;
                    if target.tag_name().name() == "symbol" {
                        for child in target.children() {
                            self.element(child, &moved, &style);
                        }
                    } else {
                        self.element(target, &moved, &style);
                    }
                    self.depth -= 1;
                } else {
                    self.warn("a use of an element that is not in the drawing".to_owned());
                }
                None
            }
            "path" => Some(path_data(node.attribute("d").unwrap_or_default(), &matrix)),
            "rect" => {
                let (x, y) = (number("x").unwrap_or(0.0), number("y").unwrap_or(0.0));
                let (w, h) = (
                    number("width").unwrap_or(0.0),
                    number("height").unwrap_or(0.0),
                );
                if w > 0.0 && h > 0.0 {
                    let (mut rx, mut ry) = (number("rx"), number("ry"));
                    if rx.is_none() {
                        rx = ry;
                    }
                    if ry.is_none() {
                        ry = rx;
                    }
                    let rx = rx.unwrap_or(0.0).clamp(0.0, w / 2.0);
                    let ry = ry.unwrap_or(0.0).clamp(0.0, h / 2.0);
                    Some(path_data(&rect_path(x, y, w, h, rx, ry), &matrix))
                } else {
                    None
                }
            }
            "circle" | "ellipse" => {
                let (cx, cy) = (number("cx").unwrap_or(0.0), number("cy").unwrap_or(0.0));
                let (rx, ry) = if name == "circle" {
                    let r = number("r").unwrap_or(0.0);
                    (r, r)
                } else {
                    (number("rx").unwrap_or(0.0), number("ry").unwrap_or(0.0))
                };
                (rx > 0.0 && ry > 0.0).then(|| {
                    let mut conic = conic_of(&matrix, [cx, cy], rx, ry, 0.0, 0.0, TAU);
                    conic.whole = true;
                    vec![SubPath {
                        segments: vec![Segment::Conic(conic)],
                        closed: true,
                    }]
                })
            }
            "line" => {
                let a = [number("x1").unwrap_or(0.0), number("y1").unwrap_or(0.0)];
                let b = [number("x2").unwrap_or(0.0), number("y2").unwrap_or(0.0)];
                Some(vec![SubPath {
                    segments: vec![Segment::Line(apply(&matrix, a), apply(&matrix, b))],
                    closed: false,
                }])
            }
            "polyline" | "polygon" => {
                let points: Vec<P2> =
                    svgtypes::PointsParser::from(node.attribute("points").unwrap_or_default())
                        .map(|(x, y)| apply(&matrix, [x, y]))
                        .collect();
                Some(vec![SubPath {
                    segments: points
                        .windows(2)
                        .map(|w| Segment::Line(w[0], w[1]))
                        .collect(),
                    closed: name == "polygon",
                }])
            }
            "text" => {
                self.text(&node, &matrix, &style);
                None
            }
            "image" | "foreignObject" => {
                self.warn(format!("an {name} element is left out"));
                None
            }
            "style" if node.text().is_some_and(|t| !t.trim().is_empty()) => {
                self.warn(
                    "style sheets are not read: styles given in style attributes are".to_owned(),
                );
                None
            }
            _ => None,
        };
        let Some(mut paths) = paths else {
            return;
        };
        // A closed polygon or path ends where it starts.
        for path in &mut paths {
            close(path);
        }
        paths.retain(|p| !p.segments.is_empty());
        if paths.is_empty() {
            return;
        }
        // A line has no inside.
        let fill = (style.fill && name != "line").then_some(if style.even_odd {
            FillRule::EvenOdd
        } else {
            FillRule::NonZero
        });
        let stroke = style
            .stroke
            .then(|| style.stroke_width * length_scale(&matrix))
            .filter(|w| *w > 0.0);
        if fill.is_none() && stroke.is_none() {
            return;
        }
        self.drawing.shapes.push(Shape {
            paths,
            fill,
            stroke,
        });
    }

    /// A text element: its characters (and its spans') as one text.
    fn text(&mut self, node: &Node<'a, 'a>, matrix: &Matrix, style: &Style) {
        let content: String = node
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect::<Vec<_>>()
            .join(" ");
        let content = content.split_whitespace().collect::<Vec<_>>().join(" ");
        if content.is_empty() {
            return;
        }
        // The first of each position list.
        let first = |attr: &str| {
            node.attribute(attr)
                .and_then(|v| v.split([' ', ',']).find(|s| !s.is_empty()))
                .and_then(|v| length(Some(v), self.per_mm))
                .unwrap_or(0.0)
        };
        let anchor = [first("x"), first("y")];
        let at = apply(matrix, anchor);
        let ahead = apply(matrix, [anchor[0] + 1.0, anchor[1]]);
        self.drawing.texts.push(SvgText {
            content,
            at,
            height: style.font_size * length_scale(matrix),
            angle: (ahead[1] - at[1]).atan2(ahead[0] - at[0]),
            font: style.font_family.clone(),
            align: style.anchor,
        });
    }
}

/// The path of a rectangle with its corners rounded by `rx`, `ry`.
fn rect_path(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> String {
    if rx <= 0.0 || ry <= 0.0 {
        return format!("M{x} {y}H{}V{}H{x}Z", x + w, y + h);
    }
    let (r, b) = (x + w, y + h);
    format!(
        "M{} {y}H{}A{rx} {ry} 0 0 1 {r} {}V{}A{rx} {ry} 0 0 1 {} {b}H{}A{rx} {ry} 0 0 1 {x} {}V{}A{rx} {ry} 0 0 1 {} {y}Z",
        x + rx,
        r - rx,
        y + ry,
        b - ry,
        r - rx,
        x + rx,
        b - ry,
        y + ry,
        x + rx
    )
}

/// The end of a closed path's last piece moved onto its start, or a line
/// back to it.
fn close(path: &mut SubPath) {
    if !path.closed {
        return;
    }
    let (Some(first), Some(last)) = (
        path.segments.first().map(start_of),
        path.segments.last().map(end_of),
    ) else {
        return;
    };
    if (first[0] - last[0]).hypot(first[1] - last[1]) > 1e-9 {
        path.segments.push(Segment::Line(last, first));
    }
}

fn start_of(segment: &Segment) -> P2 {
    match segment {
        Segment::Line(a, _) => *a,
        Segment::Bezier(points) => points[0],
        Segment::Conic(c) => c.at(c.ta),
    }
}

fn end_of(segment: &Segment) -> P2 {
    match segment {
        Segment::Line(_, b) => *b,
        Segment::Bezier(points) => points[points.len() - 1],
        Segment::Conic(c) => c.at(c.tb),
    }
}

/// The 2 × 2 part of a map `[[m00, m01], [m10, m11]]` as a rotation by φ,
/// a scaling by (σ1, σ2) and a rotation by θ: (φ, σ1, σ2, θ), σ1 ≥ |σ2|,
/// σ2 negative for a map that mirrors.
fn decompose(m00: f64, m01: f64, m10: f64, m11: f64) -> (f64, f64, f64, f64) {
    let (e, f, g, h) = (
        (m00 + m11) / 2.0,
        (m00 - m11) / 2.0,
        (m10 + m01) / 2.0,
        (m10 - m01) / 2.0,
    );
    let (q, r) = (e.hypot(h), f.hypot(g));
    let (a1, a2) = (g.atan2(f), h.atan2(e));
    ((a2 + a1) / 2.0, q + r, q - r, (a2 - a1) / 2.0)
}

/// The ellipse of centre `c`, radii `rx`, `ry` turned by `phi` (user
/// units), mapped and run from angle `t0` by `dt`.
fn conic_of(matrix: &Matrix, c: P2, rx: f64, ry: f64, phi: f64, t0: f64, dt: f64) -> Conic {
    let (s, co) = phi.sin_cos();
    // The ellipse's own axes, then the map: M = L R(φ) diag(rx, ry).
    let (r00, r01, r10, r11) = (co * rx, -s * ry, s * rx, co * ry);
    let m00 = matrix[0] * r00 + matrix[2] * r10;
    let m01 = matrix[0] * r01 + matrix[2] * r11;
    let m10 = matrix[1] * r00 + matrix[3] * r10;
    let m11 = matrix[1] * r01 + matrix[3] * r11;
    let (rotation, s1, s2, theta) = decompose(m00, m01, m10, m11);
    let sign = if s2 < 0.0 { -1.0 } else { 1.0 };
    let param = |t: f64| sign * (t + theta);
    Conic {
        center: apply(matrix, c),
        u: [s1 * rotation.cos(), s1 * rotation.sin()],
        b: s2.abs(),
        ta: param(t0),
        tb: param(t0 + dt),
        whole: false,
    }
}

/// An elliptical arc of a path (SVG's endpoint form), mapped; a line when a
/// radius is zero, nothing when the ends are the same point.
fn arc(
    matrix: &Matrix,
    from: P2,
    to: P2,
    radii: (f64, f64),
    phi_degrees: f64,
    large: bool,
    sweep: bool,
) -> Option<Segment> {
    if (from[0] - to[0]).hypot(from[1] - to[1]) <= 1e-12 {
        return None;
    }
    let (mut rx, mut ry) = (radii.0.abs(), radii.1.abs());
    if rx <= 1e-12 || ry <= 1e-12 {
        return Some(Segment::Line(apply(matrix, from), apply(matrix, to)));
    }
    let phi = phi_degrees.to_radians();
    let (s, c) = phi.sin_cos();
    let (dx, dy) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let (x1, y1) = (c * dx + s * dy, -s * dx + c * dy);
    let lambda = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let k = (num / den).max(0.0).sqrt() * if large == sweep { -1.0 } else { 1.0 };
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let center = [
        c * cx1 - s * cy1 + (from[0] + to[0]) / 2.0,
        s * cx1 + c * cy1 + (from[1] + to[1]) / 2.0,
    ];
    let angle = |ux: f64, uy: f64| uy.atan2(ux);
    let t0 = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let t1 = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let mut dt = (t1 - t0).rem_euclid(TAU);
    if !sweep && dt > 0.0 {
        dt -= TAU;
    }
    Some(Segment::Conic(conic_of(
        matrix, center, rx, ry, phi, t0, dt,
    )))
}

/// A path's `d` as subpaths, mapped.
fn path_data(d: &str, matrix: &Matrix) -> Vec<SubPath> {
    use svgtypes::PathSegment as S;
    let mut paths: Vec<SubPath> = Vec::new();
    let (mut pen, mut start) = ([0.0; 2], [0.0; 2]);
    // The last control point, for the smooth curves.
    let mut last_cubic: Option<P2> = None;
    let mut last_quad: Option<P2> = None;
    let map = |p: P2| apply(matrix, p);
    for segment in svgtypes::PathParser::from(d) {
        let Ok(segment) = segment else { break };
        let rel = |abs: bool, x: f64, y: f64, pen: P2| {
            if abs {
                [x, y]
            } else {
                [pen[0] + x, pen[1] + y]
            }
        };
        let (mut cubic, mut quad) = (None, None);
        let mut piece: Option<Segment> = None;
        match segment {
            S::MoveTo { abs, x, y } => {
                pen = rel(abs, x, y, pen);
                start = pen;
                paths.push(SubPath::default());
            }
            S::LineTo { abs, x, y } => {
                let to = rel(abs, x, y, pen);
                piece = Some(Segment::Line(map(pen), map(to)));
                pen = to;
            }
            S::HorizontalLineTo { abs, x } => {
                let to = [if abs { x } else { pen[0] + x }, pen[1]];
                piece = Some(Segment::Line(map(pen), map(to)));
                pen = to;
            }
            S::VerticalLineTo { abs, y } => {
                let to = [pen[0], if abs { y } else { pen[1] + y }];
                piece = Some(Segment::Line(map(pen), map(to)));
                pen = to;
            }
            S::CurveTo {
                abs,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1, c2, to) = (
                    rel(abs, x1, y1, pen),
                    rel(abs, x2, y2, pen),
                    rel(abs, x, y, pen),
                );
                piece = Some(Segment::Bezier(vec![map(pen), map(c1), map(c2), map(to)]));
                cubic = Some(c2);
                pen = to;
            }
            S::SmoothCurveTo { abs, x2, y2, x, y } => {
                let c1 = last_cubic.map_or(pen, |c| [2.0 * pen[0] - c[0], 2.0 * pen[1] - c[1]]);
                let (c2, to) = (rel(abs, x2, y2, pen), rel(abs, x, y, pen));
                piece = Some(Segment::Bezier(vec![map(pen), map(c1), map(c2), map(to)]));
                cubic = Some(c2);
                pen = to;
            }
            S::Quadratic { abs, x1, y1, x, y } => {
                let (c, to) = (rel(abs, x1, y1, pen), rel(abs, x, y, pen));
                piece = Some(Segment::Bezier(vec![map(pen), map(c), map(to)]));
                quad = Some(c);
                pen = to;
            }
            S::SmoothQuadratic { abs, x, y } => {
                let c = last_quad.map_or(pen, |c| [2.0 * pen[0] - c[0], 2.0 * pen[1] - c[1]]);
                let to = rel(abs, x, y, pen);
                piece = Some(Segment::Bezier(vec![map(pen), map(c), map(to)]));
                quad = Some(c);
                pen = to;
            }
            S::EllipticalArc {
                abs,
                rx,
                ry,
                x_axis_rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                let to = rel(abs, x, y, pen);
                piece = arc(matrix, pen, to, (rx, ry), x_axis_rotation, large_arc, sweep);
                pen = to;
            }
            S::ClosePath { .. } => {
                if let Some(path) = paths.last_mut() {
                    path.closed = true;
                }
                pen = start;
                // A path may go on from the closed one's start.
                paths.push(SubPath::default());
            }
        }
        if let Some(piece) = piece {
            if paths.last().is_none_or(|p| p.closed) {
                paths.push(SubPath::default());
            }
            if let Some(path) = paths.last_mut() {
                path.segments.push(piece);
            }
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    paths.retain(|p| !p.segments.is_empty());
    paths
}

/// The most text the entities a drawing declares in its DOCTYPE may
/// expand to, all their uses together (mitcad#1721): 16 MiB, far above
/// what a drawing program writes (a few namespace names), far below what
/// an expansion bomb asks for.
pub const ENTITY_TEXT_LIMIT: usize = 16 << 20;

/// How deep [`entity_expansion`] follows entities within entities: above
/// the parser's own limit of 10.
const ENTITY_DEPTH: usize = 16;

/// The XML of a drawing (mitcad#1721). A DOCTYPE is allowed, as drawing
/// programs write the SVG 1.1 one: the parser never fetches an external
/// DTD or entity (a reference to an external entity is an unknown one,
/// refused), and the entities of the internal subset are refused before
/// parsing when their uses would expand past [`ENTITY_TEXT_LIMIT`]
/// ([`entity_expansion`]); the parser's own limits (references nested 10
/// deep, 255 per reference) stop loops besides. The message says what is
/// wrong in words; the parser's own text goes to the detail, for agents
/// and the log.
fn parse_xml(text: &str) -> Result<Document<'_>, String> {
    const NOT_XML: &str = "it is not an SVG drawing: the file is not valid XML";
    const EXPANDS: &str =
        "it is not an SVG drawing: its entity declarations expand to too much text";
    let expansion = entity_expansion(text);
    if expansion > ENTITY_TEXT_LIMIT {
        return Err(crate::said(
            EXPANDS,
            format!(
                "it is not an SVG drawing: the entities of its DOCTYPE expand to {expansion} \
                 bytes, more than {ENTITY_TEXT_LIMIT}"
            ),
        ));
    }
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    Document::parse_with_options(text, options).map_err(|e| {
        let said = if matches!(e, roxmltree::Error::EntityReferenceLoop(_)) {
            EXPANDS
        } else {
            NOT_XML
        };
        crate::said(said, format!("it is not an SVG drawing: {e}"))
    })
}

/// How much text the uses of the entities declared in `text` expand to,
/// in bytes, saturating (mitcad#1721): each `&name;` of a declared entity,
/// anywhere in the text, counts its value with the entities in it
/// expanded in turn; one that refers back to itself counts as without
/// end. Declarations are looked for anywhere (in comments too) and a name
/// declared twice counts its longer value, so that the count is never
/// below what the parser expands. Character references and the five
/// predefined entities are not declared ones and count nothing.
pub fn entity_expansion(text: &str) -> usize {
    let declared = declared_entities(text);
    if declared.is_empty() {
        return 0;
    }
    let mut expanded: HashMap<&str, Option<usize>> = HashMap::new();
    let mut total = 0usize;
    for name in entity_references(text) {
        if declared.contains_key(name) {
            total = total.saturating_add(expanded_len(name, &declared, &mut expanded, 0));
            if total == usize::MAX {
                break;
            }
        }
    }
    total
}

/// The entities `<!ENTITY name "value">` (or `'value'`, or `% name` for a
/// parameter entity, which the parser keeps among the same names) declared
/// in `text`, by name; an external one (`SYSTEM`, `PUBLIC`) has no value
/// to expand and is left out.
fn declared_entities(text: &str) -> HashMap<&str, &str> {
    let mut declared: HashMap<&str, &str> = HashMap::new();
    let mut rest = text;
    while let Some(at) = rest.find("<!ENTITY") {
        rest = &rest[at + "<!ENTITY".len()..];
        let mut decl = rest.trim_start();
        if let Some(after) = decl.strip_prefix('%') {
            decl = after.trim_start();
        }
        let name_end = decl
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '>')
            .unwrap_or(decl.len());
        let name = &decl[..name_end];
        let def = decl[name_end..].trim_start();
        let Some(quote) = def.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let Some(end) = def[1..].find(quote) else {
            continue;
        };
        let value = &def[1..1 + end];
        if name.is_empty() {
            continue;
        }
        let longer = declared.get(name).is_none_or(|v| v.len() < value.len());
        if longer {
            declared.insert(name, value);
        }
    }
    declared
}

/// The names of the references `&name;` in `text`, in order (character
/// references `&#…;` left out).
fn entity_references(text: &str) -> impl Iterator<Item = &str> {
    text.split('&').skip(1).filter_map(|after| {
        let end = after.find(';')?;
        let name = &after[..end];
        let plain = !name.is_empty()
            && !name.starts_with('#')
            && !name
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '<' | '>' | '&' | '"' | '\''));
        plain.then_some(name)
    })
}

/// The length of entity `name`'s value with the entities in it expanded,
/// saturating; `None` in `expanded` marks one being expanded, so that a
/// loop counts as without end. References nested deeper than
/// [`ENTITY_DEPTH`] count as without end too (the parser refuses them
/// sooner), which also keeps a long chain of entities from exhausting the
/// stack.
fn expanded_len<'a>(
    name: &'a str,
    declared: &HashMap<&'a str, &'a str>,
    expanded: &mut HashMap<&'a str, Option<usize>>,
    depth: usize,
) -> usize {
    if depth > ENTITY_DEPTH {
        return usize::MAX;
    }
    match expanded.get(name) {
        Some(Some(len)) => return *len,
        Some(None) => return usize::MAX,
        None => {}
    }
    let Some(value) = declared.get(name).copied() else {
        return 0;
    };
    expanded.insert(name, None);
    let mut len = value.len();
    for inner in entity_references(value) {
        if declared.contains_key(inner) {
            len = len.saturating_add(expanded_len(inner, declared, expanded, depth + 1));
        }
    }
    expanded.insert(name, Some(len));
    len
}

/// Reads an SVG drawing into sketch millimetres.
pub fn read(text: &str, options: &SvgOptions) -> Result<SvgDrawing, String> {
    let document = parse_xml(text)?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err("it is not an SVG drawing: its root is not an svg element".to_owned());
    }
    if !(options.scale.is_finite() && options.scale > 0.0) {
        return Err(format!(
            "the scale must be greater than zero, got {}",
            options.scale
        ));
    }
    // The viewport: user units into the drawing's millimetres, y down.
    let view_box: Option<svgtypes::ViewBox> = root
        .attribute("viewBox")
        .and_then(|v| v.parse().ok())
        .filter(|v: &svgtypes::ViewBox| v.w > 0.0 && v.h > 0.0);
    let (width, height) = (
        size_mm(root.attribute("width")),
        size_mm(root.attribute("height")),
    );
    let mut viewport: Matrix = IDENTITY;
    let mut size = [width.unwrap_or(0.0), height.unwrap_or(0.0)];
    if let Some(vb) = view_box {
        let (w, h) = match (width, height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w * vb.h / vb.w),
            (None, Some(h)) => (h * vb.w / vb.h, h),
            (None, None) => (vb.w, vb.h),
        };
        let (mut kx, mut ky) = (w / vb.w, h / vb.h);
        let (mut dx, mut dy) = (0.0, 0.0);
        let aspect = root.attribute("preserveAspectRatio").unwrap_or_default();
        if !aspect.trim_start().starts_with("none") {
            let k = kx.min(ky);
            dx = (w - vb.w * k) / 2.0;
            dy = (h - vb.h * k) / 2.0;
            kx = k;
            ky = k;
        }
        viewport = [kx, 0.0, 0.0, ky, dx - vb.x * kx, dy - vb.y * ky];
        size = [w, h];
    }
    let mut ids = HashMap::new();
    for node in document.descendants() {
        if let Some(id) = node.attribute("id") {
            ids.insert(id, node);
        }
    }
    let per_mm = 1.0 / length_scale(&viewport);
    // First in the drawing's millimetres (y down), then into the sketch.
    let mut reader = Reader {
        ids,
        per_mm,
        drawing: SvgDrawing::default(),
        depth: 0,
    };
    // The root's own presentation attributes reach its children.
    let style = reader.style(&root, &Style::default());
    let flip: Matrix = [1.0, 0.0, 0.0, -1.0, 0.0, 0.0];
    let to_flipped = compose(&flip, &viewport);
    for child in root.children() {
        reader.element(child, &to_flipped, &style);
    }
    let mut drawing = reader.drawing;
    // Lower left corner: the viewport's bottom (y = -height after the
    // flip), or the content's lowest point without a viewport height.
    let bottom = if size[1] > 0.0 {
        -size[1]
    } else {
        let lowest = drawing
            .shapes
            .iter()
            .flat_map(|s| s.paths.iter())
            .flat_map(sample_path)
            .map(|p| p[1])
            .chain(drawing.texts.iter().map(|t| t.at[1]))
            .fold(f64::INFINITY, f64::min);
        if lowest.is_finite() { lowest } else { 0.0 }
    };
    let k = options.scale;
    let place: Matrix = [k, 0.0, 0.0, k, options.at[0], options.at[1] - k * bottom];
    for shape in &mut drawing.shapes {
        for path in &mut shape.paths {
            for segment in &mut path.segments {
                *segment = moved(segment, &place);
            }
        }
        shape.stroke = shape.stroke.map(|w| w * k);
    }
    for text in &mut drawing.texts {
        text.at = apply(&place, text.at);
        text.height *= k;
    }
    if size == [0.0, 0.0] {
        let (lo, hi) = bounds(&drawing);
        size = [hi[0] - lo[0], hi[1] - lo[1]];
    }
    drawing.size = [size[0] * k, size[1] * k];
    Ok(drawing)
}

/// A segment under a map that scales uniformly and moves.
fn moved(segment: &Segment, m: &Matrix) -> Segment {
    match segment {
        Segment::Line(a, b) => Segment::Line(apply(m, *a), apply(m, *b)),
        Segment::Bezier(points) => Segment::Bezier(points.iter().map(|p| apply(m, *p)).collect()),
        Segment::Conic(c) => Segment::Conic(Conic {
            center: apply(m, c.center),
            u: [c.u[0] * m[0], c.u[1] * m[0]],
            b: c.b * m[0],
            ..*c
        }),
    }
}

/// Points along a segment, its start first, its end left out.
fn sample(segment: &Segment) -> Vec<P2> {
    const N: usize = 24;
    match segment {
        Segment::Line(a, _) => vec![*a],
        Segment::Bezier(points) => (0..N)
            .map(|i| de_casteljau(points, i as f64 / N as f64))
            .collect(),
        Segment::Conic(c) => (0..2 * N)
            .map(|i| c.at(c.ta + (c.tb - c.ta) * i as f64 / (2 * N) as f64))
            .collect(),
    }
}

fn de_casteljau(points: &[P2], t: f64) -> P2 {
    let mut p = points.to_vec();
    for n in (1..p.len()).rev() {
        for i in 0..n {
            p[i] = [
                p[i][0] + t * (p[i + 1][0] - p[i][0]),
                p[i][1] + t * (p[i + 1][1] - p[i][1]),
            ];
        }
    }
    p[0]
}

/// A subpath as a polyline (its end included when it is open).
fn sample_path(path: &SubPath) -> Vec<P2> {
    let mut points: Vec<P2> = path.segments.iter().flat_map(sample).collect();
    if !path.closed
        && let Some(last) = path.segments.last()
    {
        points.push(end_of(last));
    }
    points
}

/// The box of the drawing's shapes and texts' anchors.
fn bounds(drawing: &SvgDrawing) -> (P2, P2) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let points = drawing
        .shapes
        .iter()
        .flat_map(|s| s.paths.iter())
        .flat_map(sample_path)
        .chain(drawing.texts.iter().map(|t| t.at));
    for p in points {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    if lo[0] > hi[0] {
        return ([0.0; 2], [0.0; 2]);
    }
    (lo, hi)
}

impl Shape {
    /// Whether the shape paints a point: inside its fill by its rule, or
    /// within half its stroke of its outline when its band was made.
    pub fn covers(&self, p: P2, bands: bool) -> bool {
        if let Some(rule) = self.fill {
            let mut winding = 0;
            for path in &self.paths {
                let polygon = sample_path(path);
                let n = polygon.len();
                for i in 0..n {
                    let (a, b) = (polygon[i], polygon[(i + 1) % n]);
                    if (a[1] <= p[1]) != (b[1] <= p[1]) {
                        let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                        if x > p[0] {
                            winding += if b[1] > a[1] { 1 } else { -1 };
                        }
                    }
                }
            }
            let inside = match rule {
                FillRule::NonZero => winding != 0,
                FillRule::EvenOdd => winding % 2 != 0,
            };
            if inside {
                return true;
            }
        }
        if bands && let Some(width) = self.stroke {
            for path in &self.paths {
                let mut line = sample_path(path);
                if path.closed
                    && let Some(first) = line.first().copied()
                {
                    line.push(first);
                }
                if line
                    .windows(2)
                    .any(|w| distance_to_segment(p, w[0], w[1]) < width / 2.0)
                {
                    return true;
                }
            }
        }
        false
    }
}

fn distance_to_segment(p: P2, a: P2, b: P2) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx * dx + dy * dy;
    let t = if l > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// What a drawing added to a sketch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SvgInsert {
    pub curves: usize,
    pub points: usize,
    pub texts: usize,
    pub bands: usize,
    pub skipped: Vec<String>,
}

/// A Bézier piece as a clamped spline of its degree.
fn bezier_nurbs(control: &[P2]) -> Nurbs {
    let degree = control.len() - 1;
    let mut knots = vec![0.0; degree + 1];
    knots.extend(std::iter::repeat_n(1.0, degree + 1));
    Nurbs {
        degree,
        control: control.to_vec(),
        weights: Vec::new(),
        knots,
    }
}

/// A segment as a sketch curve and whether it runs along the path.
fn curve_of(segment: &Segment) -> (Curve2, bool) {
    match segment {
        Segment::Line(a, b) => (Curve2::Line { a: *a, b: *b }, true),
        Segment::Bezier(points) => (Curve2::Nurbs(bezier_nurbs(points)), true),
        Segment::Conic(c) => {
            let rotation = c.u[1].atan2(c.u[0]);
            let (lo, hi) = (c.ta.min(c.tb), c.ta.max(c.tb));
            let forward = c.tb >= c.ta;
            if c.whole {
                let curve = if c.round() {
                    Curve2::Circle {
                        center: c.center,
                        radius: c.a(),
                        start: rotation + c.ta,
                    }
                } else {
                    Curve2::Ellipse {
                        center: c.center,
                        major: c.a(),
                        minor: c.b,
                        rotation,
                    }
                };
                return (curve, forward);
            }
            let curve = if c.round() {
                Curve2::Arc {
                    center: c.center,
                    radius: c.a(),
                    start: rotation + lo,
                    end: rotation + hi,
                }
            } else {
                Curve2::EllipticalArc {
                    center: c.center,
                    major: c.a(),
                    minor: c.b,
                    rotation,
                    start: lo,
                    end: hi,
                }
            };
            (curve, forward)
        }
    }
}

/// Adds the drawing's shapes and texts to the sketch being edited.
pub fn insert(
    edit: &mut SketchEdit<'_>,
    drawing: &SvgDrawing,
    stroke_outline: bool,
) -> Result<SvgInsert, String> {
    let mut shared = SharedPoints::default();
    let mut result = SvgInsert {
        skipped: drawing.warnings.clone(),
        ..SvgInsert::default()
    };
    for shape in &drawing.shapes {
        // A stroke alone, made a band, leaves its middle line out.
        let outline = shape.fill.is_some() || !(stroke_outline && shape.stroke.is_some());
        for path in &shape.paths {
            if outline {
                for segment in &path.segments {
                    add_segment(edit, &mut shared, &mut result, segment);
                }
            }
            if stroke_outline && let Some(width) = shape.stroke {
                match band(path, width) {
                    Ok(made) => {
                        for m in &made {
                            add_made(edit, &mut shared, &mut result, m);
                        }
                        result.bands += 1;
                    }
                    Err(why) => result
                        .skipped
                        .push(format!("the band of a stroke {width} mm wide: {why}")),
                }
            }
        }
    }
    for text in &drawing.texts {
        if !(text.height.is_finite() && text.height > 0.0) {
            continue;
        }
        edit.text(SketchText {
            angle: text.angle,
            font: text.font.clone(),
            align: text.align,
            ..SketchText::new(EntityUid(0), &text.content, text.at, text.height)
        })?;
        result.texts += 1;
    }
    result.points = shared.made;
    if result.curves == 0 && result.texts == 0 {
        return Err("the drawing has nothing a sketch can hold".to_owned());
    }
    Ok(result)
}

fn add(edit: &mut SketchEdit<'_>, result: &mut SvgInsert, kind: EntityKind) {
    let id = edit.id();
    edit.push(Entity::new(id, kind));
    result.curves += 1;
}

const TOO_SHORT: f64 = 1e-6;

fn add_segment(
    edit: &mut SketchEdit<'_>,
    shared: &mut SharedPoints,
    result: &mut SvgInsert,
    segment: &Segment,
) {
    match segment {
        Segment::Line(a, b) => {
            if (a[0] - b[0]).hypot(a[1] - b[1]) <= TOO_SHORT {
                return;
            }
            let (start, end) = (shared.at(edit, *a), shared.at(edit, *b));
            add(
                edit,
                result,
                EntityKind::Line {
                    start,
                    end,
                    centerline: false,
                },
            );
        }
        Segment::Bezier(points) => {
            let n = points.len();
            if (points[0][0] - points[n - 1][0]).hypot(points[0][1] - points[n - 1][1]) <= TOO_SHORT
                && points
                    .iter()
                    .all(|p| (p[0] - points[0][0]).hypot(p[1] - points[0][1]) <= TOO_SHORT)
            {
                return;
            }
            let control = points
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i == 0 || i == n - 1 {
                        shared.at(edit, *p)
                    } else {
                        edit.add_point(*p)
                    }
                })
                .collect();
            let nurbs = bezier_nurbs(points);
            add(
                edit,
                result,
                EntityKind::Spline {
                    degree: nurbs.degree as u32,
                    control,
                    weights: Vec::new(),
                    knots: nurbs.knots,
                },
            );
        }
        Segment::Conic(c) => {
            let center = shared.at(edit, c.center);
            if c.whole {
                let kind = if c.round() {
                    EntityKind::Circle {
                        center,
                        radius: c.a(),
                        start: 0.0,
                    }
                } else {
                    EntityKind::Ellipse {
                        center,
                        major: edit.add_point([c.center[0] + c.u[0], c.center[1] + c.u[1]]),
                        minor_radius: c.b,
                    }
                };
                add(edit, result, kind);
                return;
            }
            // Counter-clockwise from the lower parameter's end.
            let (lo, hi) = (c.ta.min(c.tb), c.ta.max(c.tb));
            let start = shared.at(edit, c.at(lo));
            let end = shared.at(edit, c.at(hi));
            if start == end {
                return;
            }
            let kind = if c.round() {
                EntityKind::Arc { center, start, end }
            } else {
                EntityKind::EllipticalArc {
                    center,
                    major: edit.add_point([c.center[0] + c.u[0], c.center[1] + c.u[1]]),
                    minor_radius: c.b,
                    start,
                    end,
                }
            };
            add(edit, result, kind);
        }
    }
}

/// The band of a stroke along a subpath: its outline offset by half the
/// width to both sides, and square ends of an open one.
fn band(path: &SubPath, width: f64) -> Result<Vec<Made>, String> {
    let sources: Vec<(Curve2, bool)> = path.segments.iter().map(curve_of).collect();
    let half = width / 2.0;
    let name = |i: usize| format!("segment {}", i + 1);
    let right = offset::compute(&sources, half, path.closed, None, &name)?;
    let left = offset::compute(&sources, -half, path.closed, None, &name)?;
    let mut made: Vec<Made> = Vec::new();
    for side in [&right, &left] {
        made.extend(side.made.iter().cloned());
        made.extend(side.corners.iter().map(|(_, m)| m.clone()));
    }
    if !path.closed {
        let (Some(r), Some(l)) = (right.ends.first(), left.ends.first()) else {
            return Err("an empty path".to_owned());
        };
        made.push(Made::Line(r.0, l.0));
        let (Some(r), Some(l)) = (right.ends.last(), left.ends.last()) else {
            return Err("an empty path".to_owned());
        };
        made.push(Made::Line(r.1, l.1));
    }
    Ok(made)
}

/// A curve of a band into the sketch.
fn add_made(
    edit: &mut SketchEdit<'_>,
    shared: &mut SharedPoints,
    result: &mut SvgInsert,
    made: &Made,
) {
    match made {
        Made::Line(a, b) => add_segment(edit, shared, result, &Segment::Line(*a, *b)),
        Made::Arc { center, start, end } => {
            let (center, start_id, end_id) = (
                shared.at(edit, *center),
                shared.at(edit, *start),
                shared.at(edit, *end),
            );
            if start_id != end_id {
                add(
                    edit,
                    result,
                    EntityKind::Arc {
                        center,
                        start: start_id,
                        end: end_id,
                    },
                );
            }
        }
        Made::Circle { center, radius } => {
            let center = shared.at(edit, *center);
            add(
                edit,
                result,
                EntityKind::Circle {
                    center,
                    radius: *radius,
                    start: 0.0,
                },
            );
        }
        Made::Spline { knots, control } => {
            let nurbs = offset::spline_nurbs(knots, control);
            let n = nurbs.control.len();
            let ids = nurbs
                .control
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i == 0 || i == n - 1 {
                        shared.at(edit, *p)
                    } else {
                        edit.add_point(*p)
                    }
                })
                .collect();
            add(
                edit,
                result,
                EntityKind::Spline {
                    degree: nurbs.degree as u32,
                    control: ids,
                    weights: Vec::new(),
                    knots: nurbs.knots,
                },
            );
        }
    }
}

/// A point inside a region's outline and outside its holes: its centroid
/// when that is, else the middle of the widest inside span of a line
/// across it.
pub fn interior_point(outline: &[P2], holes: &[Vec<P2>], centroid: P2) -> Option<P2> {
    let inside = |p: P2| {
        super::geometry::polygon_contains(outline, p)
            && !holes
                .iter()
                .any(|h| super::geometry::polygon_contains(h, p))
    };
    if inside(centroid) {
        return Some(centroid);
    }
    let (lo, hi) = outline
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    let mut best: Option<(f64, P2)> = None;
    for i in 1..16 {
        // Off the even fractions, where a polygon's vertices often lie.
        let y = lo + (hi - lo) * (f64::from(i) + 0.0717) / 16.0;
        let mut xs: Vec<f64> = Vec::new();
        for ring in std::iter::once(outline).chain(holes.iter().map(Vec::as_slice)) {
            let n = ring.len();
            for k in 0..n {
                let (a, b) = (ring[k], ring[(k + 1) % n]);
                if (a[1] <= y) != (b[1] <= y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.windows(2) {
            let p = [(pair[0] + pair[1]) / 2.0, y];
            let w = pair[1] - pair[0];
            if inside(p) && best.is_none_or(|(bw, _)| w > bw) {
                best = Some((w, p));
            }
        }
    }
    best.map(|(_, p)| p)
}
