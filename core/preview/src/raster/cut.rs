// SPDX-License-Identifier: MIT
//! A section and highlighted edges in a preview picture (mitcad#1487).
//!
//! - A section clips every triangle and edge at its plane, keeping the side
//!   its normal points away from. A solid body (not a sheet, not
//!   transparent) is capped: where the first surface past the plane is
//!   one facing away from the eye, the ray runs inside the body there, so
//!   the sample shows the cut face. Those samples take the body's colour,
//!   lit as facing the eye, with thin 45 degree lines in a darker shade;
//!   their depth is moved onto the plane once the opaque bodies are drawn,
//!   so that edges behind the cut stay hidden. The cut's outline (where the
//!   plane crosses each triangle) is drawn with the edges.
//! - Highlighted edges are polylines drawn over everything, two pixels
//!   wide, in [`ACCENT`].

use mitcad_vec3::{Vec3, dot, scale, sub};

use super::{Corner, Paint, Projector, Target, byte, lerp_corner};
use crate::inspect::Section;

/// What is highlighted: the view's selection colour (sRGB).
pub const ACCENT: [u8; 3] = [0x1f, 0x7a, 0xff];
/// [`ACCENT`] as 0 to 1, for faces.
pub const ACCENT_RGB: [f64; 3] = [0x1f as f64 / 255.0, 0x7a as f64 / 255.0, 1.0];
/// The cap's hatch: a line every this many pixels, the line's shade.
const HATCH_PIXELS: usize = 6;
const HATCH_SHADE: f64 = 0.6;

/// What a picture shows besides the scene's bodies (mitcad#1487).
#[derive(Debug, Clone, Default)]
pub struct Inspection {
    /// Cut away the side the normal points to; solids capped.
    pub section: Option<Section>,
    /// Highlighted edges, as polylines in the design's coordinates.
    pub accent_lines: Vec<Vec<Vec3>>,
}

/// The section's plane in the camera's frame: `dot(normal, c) - offset`
/// is a point's signed distance (positive: cut away).
#[derive(Clone, Copy)]
pub(super) struct Clip {
    pub(super) world: Section,
    normal: Vec3,
    offset: f64,
}

impl Clip {
    pub(super) fn new(section: Section, projector: &Projector) -> Self {
        let normal = [
            dot(section.normal, projector.right),
            dot(section.normal, projector.up),
            dot(section.normal, projector.forward),
        ];
        let origin = projector.camera(section.origin);
        Self {
            world: section,
            normal,
            offset: dot(normal, origin),
        }
    }

    fn distance(&self, c: Vec3) -> f64 {
        dot(self.normal, c) - self.offset
    }
}

/// The paint of a solid's cut face from its colour.
pub(super) fn cap_paint(projector: &Projector, color: [f64; 3]) -> Paint {
    let flat = projector.shade(color, scale(projector.forward, -1.0));
    Paint::Cap {
        base: flat.map(byte),
        line: flat.map(|c| byte(c * HATCH_SHADE)),
    }
}

/// Whether a sample of a cap lies on its hatch's lines.
pub(super) fn hatched(x: usize, y: usize, s: usize) -> bool {
    (x + y) % (HATCH_PIXELS * s) < s
}

/// The part of a triangle (corners in the camera's frame) on the kept
/// side, as a fan of triangles.
pub(super) fn clip_triangle(clip: &Clip, corners: [Corner; 3]) -> Vec<[Corner; 3]> {
    let distances = corners.map(|c| clip.distance(c.c));
    if distances.iter().all(|d| *d <= 0.0) {
        return vec![corners];
    }
    if distances.iter().all(|d| *d > 0.0) {
        return Vec::new();
    }
    let mut kept: Vec<Corner> = Vec::with_capacity(4);
    for k in 0..3 {
        let (p, q) = (corners[k], corners[(k + 1) % 3]);
        let (dp, dq) = (distances[k], distances[(k + 1) % 3]);
        if dp <= 0.0 {
            kept.push(p);
        }
        if (dp <= 0.0) != (dq <= 0.0) {
            kept.push(lerp_corner(p, q, dp / (dp - dq)));
        }
    }
    (1..kept.len().saturating_sub(1))
        .map(|k| [kept[0], kept[k], kept[k + 1]])
        .collect()
}

fn signed(section: &Section, p: Vec3) -> f64 {
    dot(section.normal, sub(p, section.origin))
}

/// The part of a segment on the kept side, if any.
pub(super) fn clip_segment(section: &Section, a: Vec3, b: Vec3) -> Option<(Vec3, Vec3)> {
    let (da, db) = (signed(section, a), signed(section, b));
    match (da <= 0.0, db <= 0.0) {
        (true, true) => Some((a, b)),
        (false, false) => None,
        _ => {
            let t = da / (da - db);
            let cut = [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
            Some(if da <= 0.0 { (a, cut) } else { (cut, b) })
        }
    }
}

/// Where the plane crosses a mesh's triangles: the cut's outline.
pub(super) fn outline(section: &Section, mesh: &super::Mesh) -> Vec<[Vec3; 2]> {
    let mut segments = Vec::new();
    for triangle in &mesh.triangles {
        if triangle.iter().any(|&v| v as usize >= mesh.vertices.len()) {
            continue;
        }
        let corners = triangle.map(|v| mesh.vertices[v as usize]);
        let d = corners.map(|p| signed(section, p));
        let mut points: Vec<Vec3> = Vec::with_capacity(2);
        for k in 0..3 {
            let (i, j) = (k, (k + 1) % 3);
            if (d[i] < 0.0 && d[j] > 0.0) || (d[i] > 0.0 && d[j] < 0.0) {
                let t = d[i] / (d[i] - d[j]);
                points.push([0, 1, 2].map(|c| corners[i][c] + (corners[j][c] - corners[i][c]) * t));
            } else if d[i] == 0.0 {
                points.push(corners[i]);
            }
        }
        if let [a, b] = points.as_slice()
            && a != b
        {
            segments.push([*a, *b]);
        }
    }
    segments
}

/// The plane's depth key at a sample and the slack of its slope, None
/// where the line of sight runs along the plane or meets it behind the eye.
fn plane_key(projector: &Projector, clip: &Clip, x: f64, y: f64) -> Option<f64> {
    let (dx, dy) = (
        (x - projector.cx) / projector.focal,
        (projector.cy - y) / projector.focal,
    );
    let n = clip.normal;
    if projector.perspective {
        let along = n[0] * dx + n[1] * dy + n[2];
        if along.abs() < 1e-12 {
            return None;
        }
        let z = clip.offset / along;
        (z > projector.near).then(|| projector.near / z)
    } else {
        if n[2].abs() < 1e-12 {
            return None;
        }
        let z = (clip.offset - n[0] * dx - n[1] * dy) / n[2];
        Some((projector.z1 - z) / (projector.z1 - projector.z0))
    }
}

/// Moves the caps' samples onto the plane (see the module's text).
pub(super) fn settle_caps(target: &mut Target, projector: &Projector, clip: &Clip) {
    for y in 0..target.height {
        for x in 0..target.width {
            let index = y * target.width + x;
            if !target.cap[index] {
                continue;
            }
            let (sx, sy) = (x as f64 + 0.5, y as f64 + 0.5);
            let (Some(k), Some(kx), Some(ky)) = (
                plane_key(projector, clip, sx, sy),
                plane_key(projector, clip, sx + 1.0, sy),
                plane_key(projector, clip, sx, sy + 1.0),
            ) else {
                continue;
            };
            target.depth[index] = k as f32;
            target.slack[index] = (2.0 * ((kx - k).abs() + (ky - k).abs()) + 1e-5 * k.abs()) as f32;
        }
    }
}

/// Draws a highlighted segment over everything, two pixels wide; whether
/// any of it is in the picture.
pub(super) fn draw_on_top(
    target: &mut Target,
    projector: &Projector,
    a: Vec3,
    b: Vec3,
    s: usize,
) -> bool {
    // Clipped to the picture and its guard band, as an edge (mitcad#1606).
    let Some([(ax, ay, _), (bx, by, _)]) = projector.segment(a, b) else {
        return false;
    };
    let width = 2 * s;
    let length = (bx - ax).abs().max((by - ay).abs());
    if !length.is_finite() {
        return false;
    }
    let steps = ((length * 2.0).ceil() as usize).max(1);
    let reach = width as f64 / 2.0;
    let rgba = [ACCENT[0], ACCENT[1], ACCENT[2], 255];
    let mut any = false;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let x = ax + (bx - ax) * t;
        let y = ay + (by - ay) * t;
        let x0 = (x - reach).round() as i64;
        let y0 = (y - reach).round() as i64;
        for sy in y0..y0 + width as i64 {
            if sy < 0 || sy >= target.height as i64 {
                continue;
            }
            for sx in x0..x0 + width as i64 {
                if sx < 0 || sx >= target.width as i64 {
                    continue;
                }
                target.color[sy as usize * target.width + sx as usize] = rgba;
                any = true;
            }
        }
    }
    any
}
