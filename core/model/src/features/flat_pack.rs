// SPDX-License-Identifier: MIT
//! Flat Pack (mitcad#1939; commands.md "Flat Pack"): a box solid turned
//! into the six plates of a laser-cut box, of the sheet's `thickness`, with
//! finger joints at every shared edge ([`super::finger_joint`]).
//!
//! The body must be a rectangular block along the component's axes (its
//! volume that of its box). Its box is the outside of the flat pack, or
//! with `inside` the room inside it (the plates then lie outside the box).
//! The plates are the bottom, the top, the front (at the box's smallest
//! y), the back, the left (smallest x) and the right, the bodies
//! `F<feature>.b0` to `.b5` in that order; each is drawn to the outside of
//! its edges, so that the plates overlap in the edges' corner prisms, and
//! the finger joint's plan divides every edge between its two plates (a
//! corner where three meet stays with the first of them in that order).
//! The block itself is removed. `finger_width` (default 2.5 times the
//! thickness) and `clearance` (default 0.1 mm) are the finger joint's.
//! Laying the plates out on a cut sheet is the command's
//! (`add_cut_sheet` with `nest`).

use serde::{Deserialize, Serialize};

use super::finger_joint::{FingerCut, FingerSizes, box_shape, plan};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{BooleanOp, BoundingBox, Kernel};
use crate::parameters::ParamId;

/// How close the block's volume must be to its box's, relative.
const BLOCK: f64 = 1e-6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct FlatPackDef<P = ParamId> {
    /// The box solid.
    pub body: BodyUid,
    /// The sheet's thickness.
    pub thickness: P,
    /// The box is the room inside the plates.
    #[serde(default, skip_serializing_if = "is_false")]
    pub inside: bool,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub finger_width: Option<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<P>,
}

impl<P> FlatPackDef<P> {
    pub const TYPE: &'static str = "flat_pack";
    pub const BASE_NAME: &'static str = "FlatPack";

    map_params! {
        FlatPackDef {
            body: copy,
            thickness: param,
            inside: copy,
            finger_width: option,
            clearance: option,
        }
    }
}

/// The six plates of a box from `min` to `max` (its outside), `t` thick:
/// bottom, top, front, back, left, right.
pub fn plates(min: [f64; 3], max: [f64; 3], t: f64) -> Vec<BoundingBox> {
    let slab = |axis: usize, low: bool| {
        let mut a = min;
        let mut b = max;
        if low {
            b[axis] = min[axis] + t;
        } else {
            a[axis] = max[axis] - t;
        }
        BoundingBox { min: a, max: b }
    };
    vec![
        slab(2, true),
        slab(2, false),
        slab(1, true),
        slab(1, false),
        slab(0, true),
        slab(0, false),
    ]
}

fn sizes(
    def: &FlatPackDef,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
) -> Result<(f64, FingerSizes), String> {
    let t = value(def.thickness)?;
    if !t.is_finite() || t <= 0.0 {
        return Err(format!(
            "the sheet's thickness must be greater than zero, got {t}"
        ));
    }
    let finger_width = def.finger_width.map(&mut *value).transpose()?;
    if let Some(w) = finger_width
        && (!w.is_finite() || w <= 0.0)
    {
        return Err(format!(
            "the finger width must be greater than zero, got {w}"
        ));
    }
    let clearance = match def.clearance {
        Some(id) => value(id)?,
        None => super::finger_joint::DEFAULT_CLEARANCE,
    };
    if !clearance.is_finite() || clearance < 0.0 {
        return Err(format!(
            "the clearance must not be negative, got {clearance}"
        ));
    }
    Ok((
        t,
        FingerSizes {
            finger_width: Some(finger_width.unwrap_or(2.5 * t)),
            count: None,
            clearance,
            end_margin: 0.0,
            flip: false,
        },
    ))
}

impl FeatureInfo for FlatPackDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        sizes(self, &mut |id| Ok(value(id))).map(|_| ())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for FlatPackDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (t, fingers) = sizes(self, &mut |id| ctx.param(id))?;
        let block = ctx.surface_body(self.body)?;
        let bounds = ctx
            .kernel
            .bounding_box(&block)
            .map_err(|e| e.to_string())?
            .ok_or("the box is empty")?;
        let volume = ctx
            .kernel
            .mass_properties(&block)
            .map_err(|e| e.to_string())?
            .volume;
        let boxed: f64 = (0..3).map(|i| bounds.max[i] - bounds.min[i]).product();
        if (volume - boxed).abs() > BLOCK * boxed.max(1.0) {
            return Err(
                "Flat Pack takes a box: the body is not a rectangular block along the \
                 component's axes"
                    .into(),
            );
        }
        let (min, max) = if self.inside {
            (bounds.min.map(|v| v - t), bounds.max.map(|v| v + t))
        } else {
            (bounds.min, bounds.max)
        };
        if (0..3).any(|i| max[i] - min[i] <= 2.0 * t) {
            return Err(
                "the box is too small for its plates: each side must be longer than twice \
                 the thickness"
                    .into(),
            );
        }
        let boxes = plates(min, max, t);
        let planned = plan(&boxes, &fingers)?;
        let mut changes = vec![BodyChange::Remove(self.body)];
        let mut curve = 1u32;
        for (plate, b) in boxes.iter().enumerate() {
            let mut shape = box_shape(ctx, b.min, b.max, curve)?;
            curve += 4;
            let cuts: Vec<K::Shape> = planned
                .iter()
                .filter(|f| f.plate == plate && f.op == FingerCut::Cut)
                .map(|f| {
                    let tool = box_shape(ctx, f.min, f.max, curve);
                    curve += 4;
                    tool
                })
                .collect::<Result<_, _>>()?;
            if !cuts.is_empty() {
                // The cuts overlap at the corners: one solid of them.
                let tool = super::finger_joint::one_tool(ctx, cuts)?;
                let result = ctx
                    .boolean(BooleanOp::Cut, &[&shape], &tool)
                    .map_err(|e| e.to_string())?;
                let mut pieces = result.pieces.into_iter();
                match (pieces.next(), pieces.next()) {
                    (Some(piece), None) => shape = piece.shape,
                    _ => {
                        return Err(
                            "the fingers cut a plate apart: make the fingers wider or the box \
                             larger"
                                .into(),
                        );
                    }
                }
            }
            let uid = ctx.new_body();
            changes.push(BodyChange::Set(uid, shape));
        }
        Ok(FeatureOutput {
            changes,
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 200 x 120 x 80 box of 6 mm plates: six plates, every edge jointed
    /// (twelve edges), no plate refused.
    #[test]
    fn a_box_becomes_six_jointed_plates() {
        let boxes = plates([0.0; 3], [200.0, 120.0, 80.0], 6.0);
        assert_eq!(boxes.len(), 6);
        assert_eq!(boxes[0].max[2], 6.0);
        assert_eq!(boxes[5].min[0], 194.0);
        let fingers = FingerSizes {
            finger_width: Some(15.0),
            count: None,
            clearance: 0.1,
            end_margin: 0.0,
            flip: false,
        };
        let planned = plan(&boxes, &fingers).unwrap();
        // Every plate loses something (its slots or the corners).
        for plate in 0..6 {
            assert!(
                planned.iter().any(|b| b.plate == plate),
                "plate {plate} has no slots"
            );
        }
        // The bottom keeps its four corner cubes: none of its slots is one.
        assert!(
            planned
                .iter()
                .filter(|b| b.plate == 0)
                .all(|b| (0..3).any(|i| b.max[i] - b.min[i] > 6.0 + 1e-9))
        );
    }
}
