// SPDX-License-Identifier: MIT
//! `model_render`'s line art (mitcad#1736): `format: "svg"` answers the
//! camera's view as black vector lines instead of a shaded picture, for
//! figures in manuals. The bodies drawn, their placements, `explode` and
//! the camera are the PNG's ([`Document::preview_camera`]); the lines are
//! the drawings' projection ([`Kernel::project_view`], hidden lines
//! removed), so a line art and a drawing's view of the same direction show
//! the same edges.
//!
//! - An orthographic view along the camera's line of sight, framed as the
//!   orthographic PNG of the same arguments frames it: the kernel projects
//!   along parallel lines only, so a perspective is refused and a view
//!   without a projection is drawn orthographic.
//! - The edges seen and the silhouettes of curved faces, each one path in
//!   `<g id="visible">`; with `hidden_edges`, the hidden ones dashed, one
//!   path each in `<g id="hidden">` under them. Edges where faces meet
//!   tangentially are left out, as a drawing's views leave them by default.
//! - `colors` strokes a body's lines in its colour; `highlight` strokes a
//!   whole body's lines in the selection blue and draws a face's boundary
//!   and an edge as blue lines over everything (`<g id="highlight">`), as
//!   the PNG draws a highlighted edge.
//! - `background` a colour: a rectangle under the lines; `theme` and
//!   `transparent` leave the SVG clear.
//! - `section`, `overlay`, `print_colors` and `edges: false` are the
//!   PNG's: refused.
//!
//! The SVG's user units are the picture's pixels (`size`), y down; numbers
//! are written with at most three decimals, so the same arguments give the
//! same text.

use std::fmt::Write as _;
use std::time::Instant;

use mitcad_preview::raster::ACCENT;
use mitcad_preview::{Background, Camera, Projection, RenderArgs};
use mitcad_vec3::{Vec3, cross, scale, unit};

use super::{elapsed, invalid};
use crate::document::{Document, ModelError};
use crate::drawing::{P2, ViewCurve, ViewProjection, ViewSpec};
use crate::exchange::BodyKind;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::topo::FaceName;

/// Stroke widths in pixels: edges seen, hidden ones, highlighted lines.
const VISIBLE_WIDTH: &str = "1.5";
const HIDDEN_WIDTH: &str = "1";
const HIGHLIGHT_WIDTH: &str = "2.5";
/// The hidden edges' dashes, pixels.
const HIDDEN_DASHES: &str = "6 4";
/// The lines' colour unless `colors` or `highlight` give one.
const LINE_COLOR: [u8; 3] = [0, 0, 0];
/// How far a polyline may lie from the curve it stands for, in pixels.
const PIXEL_DEFLECTION: f64 = 0.25;
/// A highlighted face's boundary turns at most this much per piece.
const LINE_ANGLE_DEGREES: f64 = 12.0;

/// A line art of the design.
#[derive(Debug, Clone)]
pub struct LineArt {
    /// The camera used (always orthographic).
    pub camera: Camera,
    pub warnings: Vec<String>,
    /// The bodies projected, each once, in the order they were given.
    pub bodies: Vec<BodyUid>,
    /// The SVG document.
    pub svg: String,
    pub width: u32,
    pub height: u32,
    /// Paths: edges seen, hidden edges, highlighted lines.
    pub visible: usize,
    pub hidden: usize,
    pub highlighted: usize,
    /// The kernel projected a triangulation (the quick path, or the exact
    /// edges failed) rather than the exact edges.
    pub polygonal: bool,
    /// Milliseconds: the projection (with placing the bodies), writing.
    pub project_ms: f64,
    pub write_ms: f64,
}

/// From the view's plane (mm, y up) to the SVG's pixels (y down).
struct Pixels {
    center: P2,
    per_mm: f64,
    half: [f64; 2],
}

impl Pixels {
    fn at(&self, p: P2) -> P2 {
        [
            self.half[0] + (p[0] - self.center[0]) * self.per_mm,
            self.half[1] - (p[1] - self.center[1]) * self.per_mm,
        ]
    }
}

impl<K: Kernel> Document<K> {
    /// `model_render`'s line art (see the module's text): the SVG of
    /// `args` (its `format` is not looked at).
    pub fn render_lines(&self, args: &RenderArgs) -> Result<LineArt, ModelError> {
        let started = Instant::now();
        let mut refused: Vec<&str> = Vec::new();
        if args.inspect.section.is_some() {
            refused.push("section");
        }
        if args.overlay.is_some() {
            refused.push("overlay");
        }
        if args.inspect.print_colors {
            refused.push("print_colors");
        }
        if !args.edges {
            refused.push("edges: false");
        }
        if !refused.is_empty() {
            return Err(invalid(format!(
                "{} {} drawn in a PNG only: leave {} out of an SVG",
                refused.join(", "),
                if refused.len() == 1 { "is" } else { "are" },
                if refused.len() == 1 { "it" } else { "them" }
            )));
        }
        if args.projection == Some(Projection::Perspective) {
            return Err(invalid(
                "an SVG is drawn with an orthographic projection: leave projection out or give orthographic",
            ));
        }
        let mut args = args.clone();
        args.projection = Some(Projection::Orthographic);
        let plan = self.preview_camera(&args)?;
        let mut warnings = plan.warnings;
        let looks = self.preview_looks(&plan.instances, &args.inspect, &mut warnings);
        let camera = plan.camera;

        // The view along the camera: x to the right, y up.
        let direction = camera.direction();
        let toward_eye = scale(direction, -1.0);
        let x_axis = unit(cross(direction, camera.up))
            .ok_or_else(|| invalid("the camera's up lies along its line of sight"))?;
        let [width, height] = args.size;
        let per_mm = f64::from(height) / camera.height.max(1e-9);
        let spec = ViewSpec {
            toward_eye,
            x_axis,
            hidden: args.hidden_edges,
            smooth: false,
            exact: true,
            quick: true,
            section: None,
            deflection: (PIXEL_DEFLECTION / per_mm).max(1e-4),
            scale: None,
        };
        let pixels = Pixels {
            center: spec.project(camera.target),
            per_mm,
            half: [f64::from(width) / 2.0, f64::from(height) / 2.0],
        };

        // Each placement's shape where it is drawn; `owner[i]` the
        // placement of the i-th shape.
        let mut shapes: Vec<K::Shape> = Vec::new();
        let mut owner: Vec<usize> = Vec::new();
        let mut bodies: Vec<BodyUid> = Vec::new();
        for (index, instance) in plan.instances.iter().enumerate() {
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let name = self.body_name(instance.body);
            match self.kernel().body_kind(shape) {
                Ok(BodyKind::Empty) => continue,
                Ok(_) => {}
                Err(e) => {
                    warnings.push(format!("{name} ({}) is not drawn: {e}", instance.body));
                    continue;
                }
            }
            let placed = if instance.transform.is_identity() {
                shape.clone()
            } else {
                match self
                    .kernel()
                    .transform_shape(shape, &instance.transform, None)
                {
                    Ok(placed) => placed,
                    Err(e) => {
                        warnings.push(format!("{name} ({}) is not drawn: {e}", instance.body));
                        continue;
                    }
                }
            };
            shapes.push(placed);
            owner.push(index);
            if !bodies.contains(&instance.body) {
                bodies.push(instance.body);
            }
        }
        let projection = if shapes.is_empty() {
            ViewProjection::default()
        } else {
            let refs: Vec<&K::Shape> = shapes.iter().collect();
            self.kernel()
                .project_view(&refs, &spec)
                .map_err(|e| invalid(format!("the bodies' edges could not be projected: {e}")))?
        };
        // Highlighted faces' boundaries and edges, in the design's
        // coordinates, over everything.
        let mut accent_lines: Vec<Vec<Vec3>> = looks.lines.clone();
        for (index, instance) in plan.instances.iter().enumerate() {
            let Some(look) = looks.instances.get(index) else {
                continue;
            };
            if look.accent || look.accent_faces.is_empty() {
                continue;
            }
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let names = self.face_names(instance.body);
            let faces: Vec<FaceName> = look
                .accent_faces
                .iter()
                .filter_map(|&f| names.get(f).and_then(|n| n.first()).cloned())
                .collect();
            if faces.is_empty() {
                continue;
            }
            if let Ok(lines) = self.kernel().edge_polylines(
                shape,
                &[],
                &faces,
                spec.deflection.max(1e-3),
                LINE_ANGLE_DEGREES.to_radians(),
            ) {
                accent_lines.extend(lines.into_iter().map(|line| {
                    line.into_iter()
                        .map(|p| instance.transform.apply_point(p))
                        .collect()
                }));
            }
        }
        let project_ms = elapsed(started);

        // The SVG: hidden edges under the ones seen, highlights on top.
        let writing = Instant::now();
        let stroke_of = |source: Option<usize>| -> Option<[u8; 3]> {
            let look = looks.instances.get(*owner.get(source?)?)?;
            if look.accent {
                Some(ACCENT)
            } else {
                look.color
                    .map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
            }
        };
        let mut svg = String::new();
        let _ = writeln!(svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        let _ = writeln!(
            svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
        );
        if let Background::Color(rgb) = args.background {
            let _ = writeln!(
                svg,
                r#"<rect x="0" y="0" width="{width}" height="{height}" fill="{}"/>"#,
                hex(rgb)
            );
        }
        let _ = writeln!(
            svg,
            r#"<g fill="none" stroke-linecap="round" stroke-linejoin="round">"#
        );
        let mut counts = [0usize; 2];
        for (group, seen) in [("hidden", false), ("visible", true)] {
            if !seen && !args.hidden_edges {
                continue;
            }
            let (stroke_width, dashes) = if seen {
                (VISIBLE_WIDTH, String::new())
            } else {
                (
                    HIDDEN_WIDTH,
                    format!(r#" stroke-dasharray="{HIDDEN_DASHES}""#),
                )
            };
            let _ = writeln!(
                svg,
                r#"<g id="{group}" stroke="{}" stroke-width="{stroke_width}"{dashes}>"#,
                hex(LINE_COLOR)
            );
            for edge in projection.edges.iter().filter(|e| e.visible == seen) {
                let Some(d) = path_data(&edge.curve, &pixels) else {
                    continue;
                };
                match stroke_of(edge.source) {
                    Some(rgb) => {
                        let _ = writeln!(svg, r#"<path d="{d}" stroke="{}"/>"#, hex(rgb));
                    }
                    None => {
                        let _ = writeln!(svg, r#"<path d="{d}"/>"#);
                    }
                }
                counts[usize::from(seen)] += 1;
            }
            let _ = writeln!(svg, "</g>");
        }
        let mut highlighted = 0;
        if !accent_lines.is_empty() {
            let _ = writeln!(
                svg,
                r#"<g id="highlight" stroke="{}" stroke-width="{HIGHLIGHT_WIDTH}">"#,
                hex(ACCENT)
            );
            for line in &accent_lines {
                let points: Vec<P2> = line.iter().map(|&p| spec.project(p)).collect();
                if let Some(d) = polyline_data(&points, &pixels) {
                    let _ = writeln!(svg, r#"<path d="{d}"/>"#);
                    highlighted += 1;
                }
            }
            let _ = writeln!(svg, "</g>");
        }
        let _ = writeln!(svg, "</g>");
        let _ = writeln!(svg, "</svg>");
        Ok(LineArt {
            camera,
            warnings,
            bodies,
            svg,
            width,
            height,
            visible: counts[1],
            hidden: counts[0],
            highlighted,
            polygonal: projection.polygonal,
            project_ms,
            write_ms: elapsed(writing),
        })
    }
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// A number with at most three decimals, without trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

fn point(p: P2) -> String {
    format!("{} {}", number(p[0]), number(p[1]))
}

/// A projected curve as a path's `d`; None when it has no finite points.
fn path_data(curve: &ViewCurve, pixels: &Pixels) -> Option<String> {
    match curve {
        ViewCurve::Line(a, b) => polyline_data(&[*a, *b], pixels),
        ViewCurve::Polyline(points) => polyline_data(points, pixels),
        ViewCurve::Spline(spline) => polyline_data(&spline.points, pixels),
        ViewCurve::Arc {
            center,
            radius,
            start,
            end,
        } => {
            let mut span = end - start;
            if span <= 0.0 {
                span += std::f64::consts::TAU;
            }
            let on = |angle: f64| {
                pixels.at([
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ])
            };
            let r = radius * pixels.per_mm;
            if !(r.is_finite() && span.is_finite() && r > 0.0) {
                return None;
            }
            // Counter-clockwise with y up is the SVG's negative sweep (y
            // down); a whole circle as two halves.
            let arc = |to: P2, large: bool| {
                format!(
                    " A {} {} 0 {} 0 {}",
                    number(r),
                    number(r),
                    u8::from(large),
                    point(to)
                )
            };
            let mut d = format!("M {}", point(on(*start)));
            if span >= std::f64::consts::TAU - 1e-9 {
                d.push_str(&arc(on(start + std::f64::consts::PI), false));
                d.push_str(&arc(on(*start), false));
            } else {
                d.push_str(&arc(on(start + span), span > std::f64::consts::PI));
            }
            Some(d)
        }
    }
}

fn polyline_data(points: &[P2], pixels: &Pixels) -> Option<String> {
    let points: Vec<P2> = points
        .iter()
        .map(|&p| pixels.at(p))
        .filter(|p| p.iter().all(|c| c.is_finite()))
        .collect();
    if points.len() < 2 {
        return None;
    }
    let mut d = format!("M {}", point(points[0]));
    for p in &points[1..] {
        d.push_str(" L ");
        d.push_str(&point(*p));
    }
    Some(d)
}

#[cfg(test)]
mod tests {
    use super::{Pixels, number, path_data};
    use crate::drawing::ViewCurve;

    #[test]
    fn numbers_are_short_and_stable() {
        assert_eq!(number(1.0), "1");
        assert_eq!(number(-0.0001), "0");
        assert_eq!(number(12.34567), "12.346");
        assert_eq!(number(-3.5), "-3.5");
    }

    #[test]
    fn arcs_and_circles_turn_the_right_way() {
        let pixels = Pixels {
            center: [0.0, 0.0],
            per_mm: 2.0,
            half: [50.0, 50.0],
        };
        // A quarter from +x to +y (up): to the top of the picture.
        let quarter = path_data(
            &ViewCurve::Arc {
                center: [0.0, 0.0],
                radius: 10.0,
                start: 0.0,
                end: std::f64::consts::FRAC_PI_2,
            },
            &pixels,
        )
        .unwrap();
        assert_eq!(quarter, "M 70 50 A 20 20 0 0 0 50 30");
        let circle = path_data(
            &ViewCurve::Arc {
                center: [0.0, 0.0],
                radius: 10.0,
                start: 0.0,
                end: std::f64::consts::TAU,
            },
            &pixels,
        )
        .unwrap();
        assert_eq!(circle, "M 70 50 A 20 20 0 0 0 30 50 A 20 20 0 0 0 70 50");
        let line = path_data(&ViewCurve::Line([0.0, 0.0], [5.0, 5.0]), &pixels).unwrap();
        assert_eq!(line, "M 50 50 L 60 40");
    }
}
