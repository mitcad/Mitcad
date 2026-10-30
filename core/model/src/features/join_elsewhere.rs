// SPDX-License-Identifier: MIT
//! What a Join's tool touches beside its participants (mitcad#958). A
//! feature changes only the bodies of its own component, so a Join (all
//! bodies or the ones listed) never joins into a body of a component placed
//! inside it. When the tool touches none of its participants but does touch
//! such a body, as a profile drawn on a child component's bracket does, the
//! error says which body it touches, that it is another component's, and
//! what to do instead (New Body, or the feature in that component).
//!
//! The bodies are those of the occurrences under the feature's component,
//! nested ones too, each moved into the component's coordinates by the
//! placements on its path at this point of the timeline; the test is the
//! one Extrude's automatic operation makes (`Kernel::tool_contacts` on the
//! bodies whose boxes the tool's reaches). Only a failed Join reads them.

use super::EvalContext;
use crate::assembly::Assembly;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::{BoundingBox, Kernel, ToolContact};
use crate::transform::Transform;

/// Boxes this far apart still meet (mm), as for the automatic operation.
const BOX_SLACK: f64 = 1.0e-5;

/// At most this many touched bodies are named.
const NAMED: usize = 3;

/// Every occurrence path under `parent`, depth first, each from `parent`.
fn occurrence_paths(
    assembly: &Assembly,
    parent: ComponentUid,
    path: &mut Vec<OccurrenceUid>,
    paths: &mut Vec<Vec<OccurrenceUid>>,
) {
    if path.len() >= 64 {
        return;
    }
    for occurrence in assembly.children(parent) {
        if path.contains(&occurrence.uid) {
            continue;
        }
        path.push(occurrence.uid);
        paths.push(path.clone());
        occurrence_paths(assembly, occurrence.component, path, paths);
        path.pop();
    }
}

/// The box `b` moved by `t`: the box of its moved corners.
fn moved_box(b: &BoundingBox, t: &Transform) -> BoundingBox {
    let corners: Vec<[f64; 3]> = b.corners().map(|c| t.apply_point(c)).collect();
    BoundingBox {
        min: std::array::from_fn(|i| corners.iter().map(|c| c[i]).fold(f64::INFINITY, f64::min)),
        max: std::array::from_fn(|i| {
            corners
                .iter()
                .map(|c| c[i])
                .fold(f64::NEG_INFINITY, f64::max)
        }),
    }
}

/// Whether two boxes meet, `slack` apart at most.
fn boxes_meet(a: &BoundingBox, b: &BoundingBox, slack: f64) -> bool {
    (0..3).all(|i| a.min[i] <= b.max[i] + slack && b.min[i] <= a.max[i] + slack)
}

/// A body of a child occurrence that the tool touches.
struct Touched {
    body: BodyUid,
    /// The occurrence that shows it (the last of its path).
    occurrence: OccurrenceUid,
    component: ComponentUid,
}

/// The bodies of the occurrences under the feature's component that `tool`
/// touches or goes into; none when the kernel cannot tell.
fn touched_in_children<K: Kernel>(ctx: &mut EvalContext<'_, K>, tool: &K::Shape) -> Vec<Touched> {
    let assembly = ctx.env.assembly;
    let own = ctx.component();
    let mut paths = Vec::new();
    occurrence_paths(assembly, own, &mut Vec::new(), &mut paths);
    if paths.is_empty() {
        return Vec::new();
    }
    let tool_box = match ctx.kernel.enclosing_box(tool) {
        // An empty tool touches nothing.
        Ok(None) => return Vec::new(),
        Ok(Some(tool_box)) => Some(tool_box),
        // A kernel without boxes tests every body.
        Err(_) => None,
    };
    let mut candidates: Vec<(Touched, K::Shape)> = Vec::new();
    for path in paths {
        let Some(last) = path.last().copied() else {
            continue;
        };
        let Some(component) = assembly.occurrence(last).map(|o| o.component) else {
            continue;
        };
        // A pattern's occurrence placed only by a later feature is not
        // there yet.
        let unplaced = path.iter().any(|uid| {
            assembly
                .occurrence(*uid)
                .is_some_and(|o| o.pattern.is_some())
                && !ctx.env.placements.contains_key(uid)
        });
        if unplaced || component == own {
            continue;
        }
        let mut placement = Transform::IDENTITY;
        for uid in &path {
            placement = placement.after(&ctx.placement(*uid));
        }
        for (body, shape) in ctx.bodies_in(component) {
            // Mesh bodies are never joined (mitcad#399).
            if ctx.is_mesh(&shape) {
                continue;
            }
            let near = match (&tool_box, ctx.kernel.enclosing_box(&shape)) {
                (_, Ok(None)) => false,
                (Some(tool_box), Ok(Some(body_box))) => {
                    boxes_meet(tool_box, &moved_box(&body_box, &placement), BOX_SLACK)
                }
                _ => true,
            };
            if !near {
                continue;
            }
            let Ok(moved) = ctx.kernel.transform_shape(&shape, &placement, None) else {
                continue;
            };
            candidates.push((
                Touched {
                    body,
                    occurrence: last,
                    component,
                },
                moved,
            ));
        }
    }
    if candidates.is_empty() {
        return Vec::new();
    }
    let shapes: Vec<&K::Shape> = candidates.iter().map(|(_, shape)| shape).collect();
    let Ok(contacts) = ctx.kernel.tool_contacts(tool, &shapes) else {
        return Vec::new();
    };
    candidates
        .into_iter()
        .zip(contacts)
        .filter(|(_, contact)| *contact != ToolContact::Apart)
        .map(|((touched, _), _)| touched)
        .collect()
}

/// When a Join's tool touches none of its participants but bodies of
/// components placed inside the feature's: the feature's component's name
/// ("Brace") and the rest of the error, "; it touches body F5.b0 (from
/// Extrude1) in Bracket:1, a body of another component, and a feature of
/// Brace joins only Brace's own bodies: choose New Body, or activate
/// Bracket and add the feature there". None when it touches none of those.
pub(crate) fn joined_elsewhere<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    tool: &K::Shape,
) -> Option<(String, String)> {
    let touched = touched_in_children(ctx, tool);
    let first = touched.first()?;
    let assembly = ctx.env.assembly;
    let own = assembly.name(ctx.component());
    let other = assembly.name(first.component);
    let mut named: Vec<String> = touched
        .iter()
        .take(NAMED)
        .map(|t| {
            format!(
                "body {} (from {}) in {}",
                t.body,
                ctx.feature_name(t.body.feature),
                assembly.occurrence_name(t.occurrence)
            )
        })
        .collect();
    if touched.len() > NAMED {
        named.push(format!("{} more", touched.len() - NAMED));
    }
    let bodies = if touched.len() == 1 {
        "a body of another component"
    } else {
        "bodies of other components"
    };
    let rest = format!(
        "; it touches {}, {bodies}, and a feature of {own} joins only {own}'s own bodies: \
         choose New Body, or activate {other} and add the feature there",
        named.join(", ")
    );
    Some((own, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_moved_by_a_quarter_turn_holds_the_turned_corners() {
        let b = BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 2.0, 1.0],
        };
        let turn = Transform::rotation(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap();
        let moved = moved_box(&b, &turn);
        let close = |a: [f64; 3], e: [f64; 3]| a.iter().zip(e).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(close(moved.min, [-2.0, 0.0, 0.0]), "{moved:?}");
        assert!(close(moved.max, [0.0, 10.0, 1.0]), "{moved:?}");
        assert!(boxes_meet(&moved, &b, 0.0));
        let apart = BoundingBox {
            min: [20.0, 20.0, 20.0],
            max: [21.0, 21.0, 21.0],
        };
        assert!(!boxes_meet(&moved, &apart, BOX_SLACK));
    }
}
