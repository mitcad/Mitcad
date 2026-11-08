// SPDX-License-Identifier: MIT
//! `model_render`'s overlay drawn over the bodies (mitcad#1488), as the 3D
//! view draws Print Analysis' triangles: flat in their colour (light does
//! not darken them), seen from both sides, each lifted along its normal by
//! its mesh's `lift` so that it lies in front of the body's own faces.
//! They are drawn after the opaque bodies and before the transparent ones,
//! against the bodies' depth, and leave that depth as it was: edges and
//! transparent bodies are drawn against the bodies, not the overlay.

use mitcad_vec3::{add, cross, norm, scale, sub};

use super::{Corner, Paint, Projector, Target, cut, draw_triangle};
use crate::overlay::OverlayMesh;

/// Draws the overlays.
/// A section (mitcad#1487) cuts them as it cuts the bodies.
pub(super) fn draw(
    target: &mut Target,
    projector: &Projector,
    overlays: &[OverlayMesh],
    clip: Option<&cut::Clip>,
) {
    if overlays.iter().all(|o| o.triangles.is_empty()) {
        return;
    }
    let depth = target.depth.clone();
    let slack = target.slack.clone();
    for overlay in overlays {
        let color = overlay.color.map(|c| f64::from(c) / 255.0);
        for &[a, b, c] in &overlay.triangles {
            let normal = cross(sub(b, a), sub(c, a));
            let length = norm(normal);
            if length.is_nan() || length <= 0.0 {
                continue;
            }
            let lift = scale(normal, overlay.lift / length);
            let corners = [a, b, c].map(|p| Corner {
                c: projector.camera(add(p, lift)),
                color,
            });
            match clip {
                Some(clip) => {
                    for part in cut::clip_triangle(clip, corners) {
                        draw_triangle(target, projector, part, Paint::Opaque);
                    }
                }
                None => draw_triangle(target, projector, corners, Paint::Opaque),
            }
        }
    }
    target.depth = depth;
    target.slack = slack;
}
