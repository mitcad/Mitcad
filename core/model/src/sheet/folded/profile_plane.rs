// SPDX-License-Identifier: MIT
//! The plane of a flange's profile sketch (mitcad#1776, GitHub
//! mitcad/Mitcad#310): a construction plane of the kind `flange_wall`
//! lies on the wall of the flange whose profile is a sketch on it, worked
//! out again at every recompute from the sheet before the plane and the
//! flange's boundary, angle, radius and position. The plane comes before
//! its flange in the timeline, so it reads the flange's definition, not
//! its result; the read is recorded ([`Read::WallFlange`]), so a change of
//! the flange, or a flange that comes to name the sketch, evaluates the
//! plane again.

use std::sync::Arc;

use crate::feature_index::FeatureIndex;
use crate::features::{EvalContext, FeatureDef, FeatureEntry, SketchPlane};
use crate::fingerprint::Fingerprint;
use crate::ids::FeatureUid;
use crate::kernel::Kernel;
use crate::parameters::Parameters;
use crate::profile::SketchFrame;
use crate::recompute::{Read, def_fingerprint};

/// The flange whose profile sketch lies on the construction plane `plane`:
/// the first in the timeline whose `profile` is a sketch on it (not on
/// another component's plane). None when no flange names such a sketch.
pub(crate) fn wall_flange<'f>(
    features: &'f [Arc<FeatureEntry>],
    index: &FeatureIndex,
    plane: FeatureUid,
) -> Option<&'f FeatureEntry> {
    features.iter().map(|f| &**f).find(|entry| {
        let FeatureDef::SheetFlange(flange) = &entry.def else {
            return false;
        };
        let Some(profile) = flange.profile else {
            return false;
        };
        index.find(features, profile).is_some_and(|sketch| {
            matches!(&sketch.def, FeatureDef::Sketch(s)
                if s.plane == SketchPlane::Construction(plane) && s.plane_link.is_none())
        })
    })
}

/// What a flange-wall plane read of its flange, for [`Read::WallFlange`]:
/// the flange's uid and the fingerprint of its definition (parameters by
/// name, their values are read on their own), None without a flange.
pub(crate) fn wall_flange_key(
    features: &[Arc<FeatureEntry>],
    index: &FeatureIndex,
    params: &Parameters,
    plane: FeatureUid,
) -> Option<u128> {
    wall_flange(features, index, plane).map(|entry| key(entry, params))
}

fn key(entry: &FeatureEntry, params: &Parameters) -> u128 {
    Fingerprint::new("wall flange")
        .u64(entry.uid.0)
        .u128(def_fingerprint(&entry.def, params))
        .finish()
}

/// The frame of the flange-wall plane being evaluated: the wall's sketch
/// frame of its flange ([`super::bends::flange_profile_frame`]) on the
/// sheet as it is at the plane, or None when no flange names a sketch on
/// the plane (the plane then keeps its stored frame).
pub(crate) fn flange_wall_plane<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
) -> Result<Option<SketchFrame>, String> {
    let (features, index, params) = (ctx.env.features, ctx.env.index, ctx.env.params);
    let plane = ctx.uid;
    let found = wall_flange(features, index, plane);
    ctx.reads.push(Read::WallFlange(
        plane,
        found.map(|entry| key(entry, params)),
    ));
    let Some(FeatureDef::SheetFlange(flange)) = found.map(|entry| &entry.def) else {
        return Ok(None);
    };
    let d = flange.map_params(&mut |_, p| ctx.param(*p))?;
    let (state, rules) = super::read(ctx, d.body)?;
    let (frame, _) = super::bends::flange_profile_frame(&state, &rules, &d)?;
    Ok(Some(frame))
}
