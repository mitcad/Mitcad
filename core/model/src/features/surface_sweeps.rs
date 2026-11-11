// SPDX-License-Identifier: MIT
//! Surface sweeps (mitcad#455): Extrude, Revolve and Sweep with `surface`
//! set make surface bodies, walls without caps, from the boundaries of
//! sketch profiles and from chains of sketch curves (`curves`, open or
//! closed); a surface Loft also lofts through sections of curves. The
//! profiles' walls are the solid feature's faces without its caps (the
//! faces `start`, `end` and `mid`, [`crate::kernel::Kernel::open_faces`]),
//! so they keep the solid feature's names, `<feature>:side(<segment>)`;
//! the curves' walls are `<feature>:side(<curve>)`. A surface sweep is a
//! new body or a new component: the profiles' walls one surface body, the
//! curves' walls another. The profiles and the curves may be a 3D
//! sketch's (mitcad#1550): its regions and curves in its sketch plane,
//! one curve out of the plane failing the feature, naming the sketch
//! ([`SketchCurves::check_in_plane`]).

use super::extrude::{Operation, ProfileRef, check_profiles};
use super::path::{self, SketchCurves};
use super::{BodyChange, CheckContext, EvalContext, FeatureOutput};
use crate::kernel::Kernel;
use crate::profile::SketchFrame;
use crate::sweeps::PathCurve;

/// The roles of a sweep's caps, which its surface leaves out.
pub(crate) const CAPS: &[&str] = &["start", "end", "mid"];

/// Checks the inputs of a surface sweep (`what`: "extrusion"): a new body
/// or component, profiles or curves of one sketch; without `surface`, no
/// curves.
pub(crate) fn check_inputs(
    ctx: &CheckContext<'_>,
    surface: bool,
    operation: Operation,
    profiles: &[ProfileRef],
    curves: Option<&SketchCurves>,
    what: &str,
) -> Result<(), String> {
    if !surface {
        if curves.is_some() {
            return Err(format!(
                "only a surface {what} sweeps sketch curves; a solid one takes profiles"
            ));
        }
        return check_profiles(ctx, profiles);
    }
    if !operation.makes_bodies() {
        return Err(format!("a surface {what} is a new body or a new component"));
    }
    if profiles.is_empty() && curves.is_none() {
        return Err(format!("a surface {what} needs profiles or sketch curves"));
    }
    if !profiles.is_empty() {
        check_profiles(ctx, profiles)?;
    }
    if let Some(curves) = curves {
        // A 3D sketch's in its sketch plane too (mitcad#1550).
        curves.check_in_plane(ctx)?;
        if profiles.first().is_some_and(|p| p.sketch != curves.sketch) {
            return Err("the curves must be of the profiles' sketch".to_owned());
        }
    }
    Ok(())
}

/// The chains the curves make on the sketch placed by `frame` (see
/// `path::wall_chains`).
pub(crate) fn chains<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    curves: &SketchCurves,
    frame: &SketchFrame,
) -> Result<Vec<Vec<PathCurve>>, String> {
    path::wall_chains(curves.curves_on(ctx, frame)?, frame)
}

/// A surface sweep's walls of the profiles: the solid tool without its
/// caps.
pub(crate) fn walls<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    tool: &K::Shape,
) -> Result<K::Shape, String> {
    ctx.kernel
        .open_faces(tool, ctx.uid, CAPS)
        .map_err(|e| e.to_string())
}

/// The output of a surface sweep: a new surface body per sheet, in order;
/// the tool (for previews) all of them.
pub(crate) fn output<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    mut sheets: Vec<K::Shape>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let tool = if sheets.len() == 1 {
        sheets[0].clone()
    } else {
        ctx.kernel.compound(&sheets).map_err(|e| e.to_string())?
    };
    let mut output = FeatureOutput {
        tool: Some(tool),
        ..FeatureOutput::default()
    };
    for sheet in sheets.drain(..) {
        output.changes.push(BodyChange::Set(ctx.new_body(), sheet));
    }
    Ok(output)
}
