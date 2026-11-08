// SPDX-License-Identifier: MIT
//! A face joined to a sheet in the plane of one of its panels (mitcad#242):
//! the face's regions, on the panel's front or back surface, are added to
//! that panel's material through its thickness. The added material keeps
//! the face's sketch curves as boundary identities (the trim's lineage); it
//! has no panel identity of its own, since a coplanar join leaves no seam.

use mitcad_vec3::sub;

use super::*;
use crate::features::sheet::{SheetFaceDef, SplitProfile, sheet_profile_regions};
use mitcad_vec3::dot;

pub(crate) fn join_face<K: Kernel>(
    d: &SheetFaceDef,
    body: BodyUid,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, body)?;
    if d.rules != state.rules {
        return Err(format!(
            "a face joined to sheet {body} uses that sheet's rules {}",
            state.rules
        ));
    }
    let thickness = rules.thickness;
    let sketch_uid = d.profiles.first().ok_or("no profiles selected")?.sketch;
    if d.profiles.iter().any(|p| p.sketch != sketch_uid) {
        return Err("a joined face's regions lie in one sketch".into());
    }
    let sketch = ctx.sketch(sketch_uid)?;
    let s = sketch.frame;
    // The planar panels whose front or back surface holds the sketch.
    let mut candidates = Vec::new();
    for patch in &state.charts {
        let SheetDomainRef::Panel(panel) = patch.domain else {
            continue;
        };
        let Ok((frame, parity)) = plane(patch) else {
            continue;
        };
        let f = frame.normal().map(|v| v * parity.sign());
        let offset = dot(sub(s.origin, frame.origin), f);
        if dot(s.normal(), frame.normal()).abs() < 1.0 - 1e-9
            || (offset.abs() > 1e-6 && (offset - thickness).abs() > 1e-6)
        {
            continue;
        }
        let o = sub(s.origin, frame.origin);
        let to_chart = Isometry2 {
            origin: [dot(o, frame.x_axis), dot(o, frame.y_axis)],
            x_axis: [dot(s.x_axis, frame.x_axis), dot(s.x_axis, frame.y_axis)],
            y_axis: [dot(s.y_axis, frame.x_axis), dot(s.y_axis, frame.y_axis)],
        };
        candidates.push((panel, to_chart));
    }
    if candidates.is_empty() {
        return Err(format!(
            "the joined face's sketch {sketch_uid} is not on a surface of a panel of sheet {body}"
        ));
    }
    // The regions found as a base Face finds them (mitcad#1444); the pieces
    // of a region a sketch change split are all added, as the material has
    // no panel identity of its own.
    let (_, found) = sheet_profile_regions(ctx, &d.profiles, SplitProfile::Pieces)?;
    let mut regions = Vec::new();
    for (_, region) in &found {
        regions.push(MaterialRegion::from_legacy(&SheetRegion {
            loops: region
                .loops
                .iter()
                .map(|l| SheetLoop {
                    segments: l
                        .segments
                        .iter()
                        .map(|seg| SheetSegment {
                            id: SheetCurveId::Sketch {
                                sketch: sketch_uid,
                                segment: seg.key.clone(),
                            },
                            geometry: seg.geometry.clone(),
                        })
                        .collect(),
                })
                .collect(),
        })?);
    }
    // The panel the regions touch: their union with its material is one
    // region. Several coplanar panels touched would leave the owner of the
    // added material ambiguous.
    let mut joined = None;
    for (panel, to_chart) in candidates {
        let tools = mapped_regions(&regions, to_chart);
        let mut trial = state.clone();
        let before = chart(&trial, panel)?.regions.len();
        trim_with_lineage(
            ctx,
            &mut trial,
            &[SheetDomainRef::Panel(panel)],
            SheetTrimOperation::Union,
            &tools,
            &[Isometry2::IDENTITY],
            Some(SheetDomainRef::Panel(panel)),
        )?;
        if chart(&trial, panel)?.regions.len() <= before {
            if joined.is_some() {
                return Err(format!(
                    "the joined face touches several panels of sheet {body}; join it to one"
                ));
            }
            joined = Some(trial);
        }
    }
    state = joined.ok_or_else(|| {
        format!("the joined face does not touch the material of a panel of sheet {body}")
    })?;
    publish(ctx, state, &rules, None)
}
