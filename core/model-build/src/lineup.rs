// SPDX-License-Identifier: MIT
//! Side snaps in line with the frame's other profiles (mitcad#1886;
//! `commands.md`, "BUILD: profile assemblies", *In line with other
//! profiles*): a part placed on a side may line up with a profile that
//! does not stand on that side, one beside the target, above or below it,
//! crossing it or standing on another member: its centre plane on that
//! profile's (`inline`), or a side face flush with that profile's face
//! facing the same way (`flush_face`).

use crate::BuildRole;
use crate::gap::SideGap;
use crate::geometry::{box_along, dot, sub, unit, x_axis};
use crate::prelude::*;
use mitcad_model::Document;
use mitcad_model::ids::{ComponentUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;

/// Within this a profile's axis counts as square to the slot, and a face
/// of its box as across it.
const SQUARE: f64 = 1e-6;

/// A place along a side where the new part lines up with another profile
/// of the frame.
#[derive(Debug, Clone, PartialEq)]
pub struct LineUp {
    /// `inline` (the centre planes meet) or `flush_face` (a side face
    /// flush with the profile's face facing the same way).
    pub kind: &'static str,
    /// The new part's near face from end A of the side.
    pub offset: f64,
    /// The profile it lines up with, its path from the BUILD host and its
    /// name.
    pub component: ComponentUid,
    pub path: Vec<OccurrenceUid>,
    pub name: String,
}

/// The places along a side where a part `near`..`far` along the slot
/// (its reach from its frame) lines up with the frame's other profiles:
/// the profiles of the BUILD host, but the target and those standing on
/// the side's slot (`except`), whose axis is square to the slot and whose
/// box has faces across it. Only places where the part stays on the side
/// (`length` long) and fits in a free stretch (`gaps`) count. Sorted along
/// the side, `inline` before `flush_face` at one place, each place once.
pub(crate) fn line_ups<K: Kernel>(
    doc: &Document<K>,
    slot: &Transform,
    near: f64,
    far: f64,
    length: f64,
    gaps: &[SideGap],
    except: &[ComponentUid],
) -> Vec<LineUp> {
    let width = far - near;
    let Some(x) = unit(x_axis(slot)) else {
        return Vec::new();
    };
    let origin = slot.translation;
    let fits = |d: f64| {
        d >= -1e-9
            && d <= length - width + 1e-9
            && gaps
                .iter()
                .any(|g| g.start - 1e-9 <= d && d + width <= g.end + 1e-9)
    };
    let mut out: Vec<LineUp> = Vec::new();
    for info in doc.build_parts_info() {
        if info.meta.role != BuildRole::Profile
            || !info.is_cut_to_length()
            || except.contains(&info.component)
        {
            continue;
        }
        let (Some(a), Some(b)) = (info.mount("end_a"), info.mount("end_b")) else {
            continue;
        };
        let Some(path) = doc.placed_in_host(info.component) else {
            continue;
        };
        let Some(bounds) = doc.part_bounds(info.component) else {
            continue;
        };
        let placement = doc.path_transform(&path);
        // Its axis square to the slot (a post beside the target, a member
        // crossing it), and a pair of its box's faces across the slot.
        let Some(axis) = unit(placement.apply_vector(sub(b.translation, a.translation))) else {
            continue;
        };
        if dot(axis, x).abs() > SQUARE {
            continue;
        }
        let across = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
            .into_iter()
            .any(|e| dot(placement.apply_vector(e), x).abs() > 1.0 - SQUARE);
        if !across {
            continue;
        }
        let (lo, hi) = box_along(&bounds, &placement, origin, x);
        let centre = (lo + hi) / 2.0;
        for (kind, d) in [
            ("inline", centre - width / 2.0),
            ("flush_face", lo),
            ("flush_face", hi - width),
        ] {
            if fits(d) {
                out.push(LineUp {
                    kind,
                    offset: if d.abs() < 1e-9 { 0.0 } else { d },
                    component: info.component,
                    path: path.clone(),
                    name: info.name.clone(),
                });
            }
        }
    }
    // Along the side; at one place `inline` first, then the profile made
    // first.
    out.sort_by(|p, q| {
        p.offset
            .total_cmp(&q.offset)
            .then_with(|| (p.kind != "inline").cmp(&(q.kind != "inline")))
    });
    let mut kept: Vec<LineUp> = Vec::new();
    for l in out {
        if kept.iter().all(|k| (k.offset - l.offset).abs() > 1e-6) {
            kept.push(l);
        }
    }
    kept
}

/// The line-up a click with the near face at `face` takes: the nearest
/// within `within`, `inline` first at one distance.
pub(crate) fn nearest(lines: &[LineUp], face: f64, within: f64) -> Option<&LineUp> {
    lines
        .iter()
        .filter(|l| (l.offset - face).abs() < within)
        .min_by(|p, q| {
            (p.offset - face)
                .abs()
                .total_cmp(&(q.offset - face).abs())
                .then_with(|| (p.kind != "inline").cmp(&(q.kind != "inline")))
        })
}
