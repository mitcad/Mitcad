// SPDX-License-Identifier: MIT
//! Bead (mitcad#2645): stiffening ribs pressed into a flat panel of a sheet
//! along sketch lines and arcs. The sketch lies on the panel's front or
//! back surface (the bead's face); its lines and arcs, joined end to end,
//! are the beads' centre lines. Across, a bead is a round, U, V or
//! trapezoid section `width` wide and `depth` deep on its face, its bends of
//! the rules' bend radius (or `radius`); it stands out of its face or is
//! pressed into it, and ends round or on a run-out ramp. The sheet keeps
//! its thickness, stays a sheet, and its Flat Pattern draws the bead as a
//! form on a layer of its own (`sheet/bead.rs`, `sheet/folded/bead.rs`).

use super::mapping::map_params;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use serde::{Deserialize, Serialize};

pub use crate::sheet::bead::{BeadDirection, BeadEnd, BeadProfile};

/// A trapezoid's flank angle when none is given: 60 degrees.
pub const DEFAULT_ANGLE: f64 = std::f64::consts::FRAC_PI_3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetBeadDef<P = ParamId> {
    pub body: BodyUid,
    /// A sketch on a flat face of the sheet (its front or back surface).
    pub sketch: FeatureUid,
    /// Its lines and arcs: the centre lines, joined end to end where they
    /// meet.
    #[serde(with = "mitcad_sketch::internal::curves_serde")]
    pub curves: Vec<EntityUid>,
    #[serde(default, skip_serializing_if = "is_round")]
    pub profile: BeadProfile,
    /// Across, between the lips' virtual sharps on the face.
    pub width: P,
    /// From the face to the bead's crown (its virtual sharp).
    pub depth: P,
    #[serde(default, skip_serializing_if = "is_out")]
    pub direction: BeadDirection,
    #[serde(default, skip_serializing_if = "is_round_end")]
    pub ends: BeadEnd,
    /// The inside radius of the bead's bends in place of the rules' bend
    /// radius.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
    /// A trapezoid's flank angle to the face (radians, default 60 degrees).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// A run-out end's length along the centre line (default: the width).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_out: Option<P>,
}

fn is_round(p: &BeadProfile) -> bool {
    *p == BeadProfile::Round
}

fn is_out(d: &BeadDirection) -> bool {
    *d == BeadDirection::Out
}

fn is_round_end(e: &BeadEnd) -> bool {
    *e == BeadEnd::Round
}

impl<P> SheetBeadDef<P> {
    pub const TYPE: &'static str = "sheet_bead";
    pub const BASE_NAME: &'static str = "SheetBead";
    map_params! {
        SheetBeadDef {
            body: copy,
            sketch: copy,
            curves: clone,
            profile: copy,
            width: param,
            depth: param,
            direction: copy,
            ends: copy,
            radius: option,
            angle: option,
            run_out: option,
        }
    }
}

impl FeatureInfo for SheetBeadDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.features.insert(self.sketch);
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        c.body(self.body)?;
        c.sketch(self.sketch)?;
        if self.curves.is_empty() {
            return Err("a bead needs at least one line or arc of a sketch".into());
        }
        for (i, curve) in self.curves.iter().enumerate() {
            if self.curves[..i].contains(curve) {
                return Err("a line or arc of the bead is listed more than once".into());
            }
        }
        Ok(())
    }
    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let positive = |v: f64| v.is_finite() && v > crate::sheet::TOLERANCE;
        if !positive(value(self.width)) || !positive(value(self.depth)) {
            return Err("a bead needs a width and a depth above zero".into());
        }
        if self.radius.is_some_and(|r| !positive(value(r))) {
            return Err("a bead's bend radius must be above zero".into());
        }
        if let Some(a) = self.angle.map(value)
            && !(a.is_finite() && a > 1e-6 && a <= std::f64::consts::FRAC_PI_2 + 1e-12)
        {
            return Err(
                "a trapezoid bead's flank angle must be above 0 and at most 90 degrees".into(),
            );
        }
        if self.run_out.is_some_and(|l| !positive(value(l))) {
            return Err("a bead's run-out must be longer than zero".into());
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for SheetBeadDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::bead(&d, ctx).map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
