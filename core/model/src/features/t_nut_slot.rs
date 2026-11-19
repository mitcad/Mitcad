// SPDX-License-Identifier: MIT
//! T-Nut Slot (mitcad#1939; commands.md "T-Nut Slot"): the screw-and-nut
//! joint of laser-sheet plates, beside the tab and slot of Mortise and
//! Tenon. A slot for the screw is cut into a plate from its planar end
//! face, with a pocket across it for a square or hex nut, both through the
//! plate's thickness; the plate the end face stands against gets the
//! screw's hole.
//!
//! The end face's axes are Mortise and Tenon's: x along its longest
//! straight edge, y across it (the plate's thickness), z its outward
//! normal. The slot is `screw_diameter` plus `clearance` (default 0.1 mm)
//! on each side wide along x and `slot_length` deep from the face; the
//! pocket `nut_width` plus the clearance on each side wide along x and
//! `nut_thickness` plus the clearance on each side along z, its near side
//! `nut_distance` from the face. Both run through the plate's thickness.
//! The hole in `mating_body` (left out: none) is a cylinder of the slot's
//! width along z from the face through that body. `count` slots (default 1)
//! lie evenly along the face: each in the middle of its share of the
//! face's length.
//!
//! Both bodies keep their ids. Refused with the reason: an end face that is
//! not planar, a nut no wider than the screw, a pocket past the slot's end,
//! slots wider than their share of the face, a count that is not a whole
//! number.

use serde::{Deserialize, Serialize};

use mitcad_vec3::scale;

use super::base::apply_tool;
use super::face_refs::{add_faces, check_faces, kernel_error};
use super::mapping::map_params;
use super::mortise_tenon::EndFace;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{BooleanOp, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, Kernel, SideEnd};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::topo::{FaceName, RegionKey, SegmentKey, TopoName};

/// The default clearance per side, mm.
pub const DEFAULT_CLEARANCE: f64 = 0.1;
/// The most slots of one feature.
pub const MAX_SLOTS: f64 = 100.0;
/// How far the cuts start outside the face, mm, so that they do not end on
/// it.
const OUTSIDE: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct TNutSlotDef<P = ParamId> {
    /// The plate the slot is cut into.
    pub body: BodyUid,
    /// Its planar end face the slot starts from.
    pub face: FaceName,
    /// The plate that gets the screw's hole; none: no hole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mating_body: Option<BodyUid>,
    pub screw_diameter: P,
    pub slot_length: P,
    pub nut_width: P,
    pub nut_thickness: P,
    /// From the face to the pocket's near side.
    pub nut_distance: P,
    /// Per side; default 0.1 mm.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<P>,
    /// How many slots along the face; default 1.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub count: Option<P>,
}

impl<P> TNutSlotDef<P> {
    pub const TYPE: &'static str = "t_nut_slot";
    pub const BASE_NAME: &'static str = "TNutSlot";

    map_params! {
        TNutSlotDef {
            body: copy,
            face: clone,
            mating_body: copy,
            screw_diameter: param,
            slot_length: param,
            nut_width: param,
            nut_thickness: param,
            nut_distance: param,
            clearance: option,
            count: option,
        }
    }
}

/// The slot's sizes, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TNutSizes {
    pub screw: f64,
    pub length: f64,
    pub nut_width: f64,
    pub nut_thickness: f64,
    pub nut_distance: f64,
    pub clearance: f64,
    pub count: usize,
}

impl TNutSizes {
    fn new(
        def: &TNutSlotDef,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    ) -> Result<Self, String> {
        let screw = value(def.screw_diameter)?;
        let length = value(def.slot_length)?;
        let nut_width = value(def.nut_width)?;
        let nut_thickness = value(def.nut_thickness)?;
        let nut_distance = value(def.nut_distance)?;
        let clearance = match def.clearance {
            Some(id) => value(id)?,
            None => DEFAULT_CLEARANCE,
        };
        let count = match def.count {
            Some(id) => value(id)?,
            None => 1.0,
        };
        if !count.is_finite()
            || (count - count.round()).abs() > 1e-9
            || !(1.0..=MAX_SLOTS).contains(&count.round())
        {
            return Err(mitcad_model_base::internal::parameters::count_error(
                format!(
                    "the count of slots must be a whole number from 1 to {MAX_SLOTS}, got {count}"
                ),
                count,
            ));
        }
        let sizes = Self {
            screw,
            length,
            nut_width,
            nut_thickness,
            nut_distance,
            clearance,
            count: count.round() as usize,
        };
        sizes.check()?;
        Ok(sizes)
    }

    pub fn check(&self) -> Result<(), String> {
        for (what, v) in [
            ("screw's diameter", self.screw),
            ("slot's length", self.length),
            ("nut's width", self.nut_width),
            ("nut's thickness", self.nut_thickness),
        ] {
            if !v.is_finite() || v <= 0.0 {
                return Err(format!("the {what} must be greater than zero, got {v}"));
            }
        }
        if !self.nut_distance.is_finite() || self.nut_distance < 0.0 {
            return Err(format!(
                "the nut's distance must not be negative, got {}",
                self.nut_distance
            ));
        }
        if !self.clearance.is_finite() || self.clearance < 0.0 {
            return Err(format!(
                "the clearance must not be negative, got {}",
                self.clearance
            ));
        }
        if self.nut_width <= self.screw {
            return Err("the nut must be wider than the screw".into());
        }
        if self.nut_distance + self.nut_thickness + self.clearance > self.length {
            return Err(
                "the nut's pocket reaches past the slot's end: make the slot longer".into(),
            );
        }
        Ok(())
    }

    /// Each slot's middle along the face's x from `min` to `max`.
    pub fn middles(&self, min: f64, max: f64) -> Result<Vec<f64>, String> {
        let share = (max - min) / self.count as f64;
        if share < self.nut_width + 2.0 * self.clearance {
            return Err(format!(
                "{} slots do not fit along the end face: each nut needs {} mm",
                self.count,
                super::bearing_sizes::short_number(self.nut_width + 2.0 * self.clearance)
            ));
        }
        Ok((0..self.count)
            .map(|k| min + (k as f64 + 0.5) * share)
            .collect())
    }
}

impl FeatureInfo for TNutSlotDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        if let Some(mating) = self.mating_body {
            references.body(mating);
        }
        add_faces(&mut references, std::iter::once(&self.face));
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, std::slice::from_ref(&self.face), "end face")?;
        if let Some(mating) = self.mating_body {
            ctx.body(mating)?;
            if mating == self.body {
                return Err("the screw's hole goes into another plate than the slot's".into());
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        TNutSizes::new(self, &mut |id| Ok(value(id))).map(|_| ())
    }
}

impl<K: Kernel> Evaluate<K> for TNutSlotDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let sizes = TNutSizes::new(self, &mut |id| ctx.param(id))?;
        let plate = ctx.surface_body(self.body)?;
        let face = ctx.face_on(self.body, &plate, &self.face)?;
        let plane = ctx.kernel.face_plane(&plate, &face).map_err(|_| {
            crate::notice::placed(
                "the slot's end face must be planar",
                crate::notice::NoticePlace::face(face.to_string()).in_body(&self.body.to_string()),
                format!("the slot's end face {face} must be planar"),
            )
        })?;
        let z = mitcad_vec3::unit_div(plane.normal).ok_or("the slot's end face has no normal")?;
        let curves = ctx
            .kernel
            .curves_of(&plate, &TopoName::Face(face.clone()))
            .map_err(kernel_error)?;
        let end = EndFace::new(plane.origin, z, &curves, false)?;
        let frame = SketchFrame {
            origin: end.origin,
            x_axis: end.x,
            y_axis: end.y,
        };
        let middles = sizes.middles(end.min[0], end.max[0])?;
        let c = sizes.clearance;
        let (y0, y1) = (end.min[1] - OUTSIDE, end.max[1] + OUTSIDE);
        let mut slots = Vec::with_capacity(middles.len() * 2);
        for (k, u) in middles.iter().enumerate() {
            let w = sizes.screw / 2.0 + c;
            let n = sizes.nut_width / 2.0 + c;
            let first = 1 + 10 * k as u32;
            // Into the plate (against the face's normal): the start offset
            // is along the frame's normal, the face's, so outside is +.
            slots.push(extrude(
                ctx,
                frame,
                &rect(first, [u - w, y0], [u + w, y1]),
                scale(z, -1.0),
                OUTSIDE,
                sizes.length + OUTSIDE,
            )?);
            slots.push(extrude(
                ctx,
                frame,
                &rect(first + 4, [u - n, y0], [u + n, y1]),
                scale(z, -1.0),
                -(sizes.nut_distance - c),
                sizes.nut_thickness + 2.0 * c,
            )?);
        }
        // The slots and pockets overlap: one solid of them.
        let tool = super::finger_joint::one_tool(ctx, slots)?;
        let mut changes = apply_tool(ctx, BooleanOp::Cut, &[(self.body, plate)], &tool, "slot")?;
        if let Some(mating) = self.mating_body {
            let other = ctx.surface_body(mating)?;
            let reach = ctx
                .kernel
                .bounding_box(&other)
                .map_err(kernel_error)?
                .map(|b| (0..3).map(|i| (b.max[i] - b.min[i]).abs()).sum::<f64>())
                .unwrap_or(0.0)
                + 2.0 * OUTSIDE;
            let mid = (end.min[1] + end.max[1]) / 2.0;
            let mut holes = Vec::with_capacity(middles.len());
            for (k, u) in middles.iter().enumerate() {
                holes.push(extrude(
                    ctx,
                    frame,
                    &circle(101 + k as u32, [*u, mid], sizes.screw / 2.0 + c),
                    z,
                    -OUTSIDE,
                    reach,
                )?);
            }
            let hole = super::finger_joint::one_tool(ctx, holes)?;
            changes.extend(apply_tool(
                ctx,
                BooleanOp::Cut,
                &[(mating, other)],
                &hole,
                "screw's hole",
            )?);
        }
        Ok(FeatureOutput {
            changes,
            tool: Some(tool),
            ..FeatureOutput::default()
        })
    }
}

fn rect(first: u32, a: [f64; 2], b: [f64; 2]) -> ProfileRegion {
    super::finger_joint::rectangle(first, a, b)
}

fn circle(id: u32, center: [f64; 2], radius: f64) -> ProfileRegion {
    let key = SegmentKey::closed(EntityUid(id));
    ProfileRegion {
        key: RegionKey::new([key.clone()]).expect("a circle"),
        loops: vec![ProfileLoop {
            segments: vec![ProfileSegment {
                key,
                geometry: SegmentGeometry::Circle {
                    center,
                    radius,
                    start: 0.0,
                },
            }],
        }],
    }
}

fn extrude<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    frame: SketchFrame,
    region: &ProfileRegion,
    direction: [f64; 3],
    start: f64,
    distance: f64,
) -> Result<K::Shape, String> {
    ctx.kernel
        .extrude_feature(&ExtrudeFeatureSpec {
            feature: ctx.uid,
            frame,
            regions: std::slice::from_ref(region),
            direction,
            start: ExtrudeStart::Offset(start),
            side1: ExtrudeSide {
                end: SideEnd::Distance(distance),
                taper: 0.0,
                thin: None,
            },
            side2: None,
        })
        .map_err(kernel_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m3() -> TNutSizes {
        TNutSizes {
            screw: 3.0,
            length: 12.0,
            nut_width: 5.5,
            nut_thickness: 2.4,
            nut_distance: 6.0,
            clearance: 0.1,
            count: 1,
        }
    }

    #[test]
    fn an_m3_slot_fits_and_its_refusals_are_in_words() {
        assert!(m3().check().is_ok());
        assert_eq!(m3().middles(0.0, 60.0).unwrap(), vec![30.0]);
        let mut three = m3();
        three.count = 3;
        assert_eq!(three.middles(0.0, 60.0).unwrap(), vec![10.0, 30.0, 50.0]);
        three.count = 20;
        assert!(three.middles(0.0, 60.0).unwrap_err().contains("do not fit"));
        let mut thin = m3();
        thin.nut_width = 2.0;
        assert_eq!(
            thin.check().unwrap_err(),
            "the nut must be wider than the screw"
        );
        let mut short = m3();
        short.length = 8.0;
        assert!(short.check().unwrap_err().contains("past the slot's end"));
    }
}
