// SPDX-License-Identifier: MIT
//! Panelize (mitcad#2005): the chosen planar faces of a body split off as
//! panels of one thickness, for cabinetry. Each face's slab, the face
//! extruded `thickness` into the body and intersected with it, becomes a
//! body of its own, `<feature>.b<i>` for the i-th face; what is left of
//! the body stays the body (its id), so the next Panelize works on it.
//! Panels made first run through the corners: the order of the steps
//! decides the joints. With `remove_remainder` the body left after this
//! step goes (the block inside a cabinet, on the last step).
//!
//! The slabs of one step must not overlap (faces that meet at an edge go
//! in separate steps), else the feature fails in words, the faces painted.
//! With `result` `components` each panel goes into a component of its own:
//! the document adds a `component_from_bodies` feature after the Panelize
//! for each (`document/panelize.rs`), which keeps that in step with edits.
//! The panels are named after their face's outward normal in the
//! component's axes (−X Left, +X Right, −Y Front, +Y Back, −Z Bottom, +Z
//! Top; otherwise `Panel <n>`) and take the body's material.
//!
//! The slabs are the geometry kernel's: an extrusion of a body face
//! ([`Kernel::extrude_face`]) and booleans, so no kernel family of its own.

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{BooleanOp, FaceExtrudeSpec, Kernel, PlacedPair};
use crate::notice::NoticePlace;
use crate::parameters::ParamId;
use crate::topo::FaceName;
use crate::transform::Transform;

/// Where the panels go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelResult {
    /// Bodies in the feature's component.
    #[default]
    Bodies,
    /// Each panel in a component of its own.
    Components,
}

impl PanelResult {
    fn is_bodies(&self) -> bool {
        *self == Self::Bodies
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct PanelizeDef<P = ParamId> {
    pub body: BodyUid,
    /// The planar faces split off, one panel each, in this order.
    pub faces: Vec<FaceName>,
    /// The panels' thickness (mm, > 0), into the body from each face.
    pub thickness: P,
    #[serde(default, skip_serializing_if = "PanelResult::is_bodies")]
    pub result: PanelResult,
    /// The body left after this step is removed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub remove_remainder: bool,
}

impl<P> PanelizeDef<P> {
    pub const TYPE: &'static str = "panelize";
    pub const BASE_NAME: &'static str = "Panelize";

    map_params! {
        PanelizeDef {
            body: copy,
            faces: clone,
            thickness: param,
            result: copy,
            remove_remainder: copy,
        }
    }
}

/// The body of the i-th face's panel.
pub fn panel_body(feature: crate::ids::FeatureUid, index: usize) -> BodyUid {
    BodyUid::new(feature, index as u32)
}

/// A panel's name by its face's outward normal (unit) in the component's
/// axes: Left, Right, Front, Back, Bottom or Top along an axis, else
/// `Panel <index + 1>`.
pub fn panel_name(normal: Option<[f64; 3]>, index: usize) -> String {
    const NAMES: [[&str; 2]; 3] = [["Left", "Right"], ["Front", "Back"], ["Bottom", "Top"]];
    normal
        .and_then(|n| {
            let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            (0..3).find_map(|k| {
                let along = n[k] / length;
                (length > 1e-12 && along.abs() > 1.0 - 1e-6)
                    .then(|| NAMES[k][usize::from(along > 0.0)].to_owned())
            })
        })
        .unwrap_or_else(|| format!("Panel {}", index + 1))
}

fn check_thickness(thickness: f64) -> Result<(), String> {
    if thickness > 0.0 && thickness.is_finite() {
        Ok(())
    } else {
        Err(crate::notice::said(
            "the panel thickness must be greater than zero",
            format!("the panel thickness must be greater than zero, got {thickness}"),
        ))
    }
}

impl FeatureInfo for PanelizeDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to panelize")
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_thickness(value(self.thickness))
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

/// One shape of a boolean's pieces: None when there are none.
fn one_shape<K: Kernel>(kernel: &K, pieces: Vec<K::Shape>) -> Result<Option<K::Shape>, String> {
    match pieces.len() {
        0 => Ok(None),
        1 => Ok(pieces.into_iter().next()),
        _ => kernel.compound(&pieces).map(Some).map_err(kernel_error),
    }
}

impl<K: Kernel> Evaluate<K> for PanelizeDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let thickness = ctx.param(self.thickness)?;
        check_thickness(thickness)?;
        let shape = ctx.surface_body(self.body)?;
        let faces = require_faces(ctx, self.body, &shape, &self.faces)?;
        let body = self.body.to_string();
        let place = |face: &FaceName| NoticePlace::face(face.to_string()).in_body(&body);
        let mut tools = Vec::with_capacity(faces.len());
        let mut panels = Vec::with_capacity(faces.len());
        for (found, key) in faces.iter().zip(&self.faces) {
            // Planar faces only: the face's plane is the panel's.
            if ctx.kernel.face_plane(&shape, found).is_err() {
                return Err(crate::notice::placed(
                    "a face to panelize is not planar: panels are made of planar faces",
                    place(key),
                    format!("face {key} is not planar: panels are made of planar faces"),
                ));
            }
            let spec = FaceExtrudeSpec {
                feature: ctx.uid,
                body: &shape,
                face: found,
                key,
                reversed: true,
                start: 0.0,
                side1: crate::kernel::SideEnd::Distance(thickness),
                side2: None,
                direction: None,
                taper1: 0.0,
                taper2: 0.0,
            };
            let slab = ctx
                .kernel
                .extrude_face(&spec)
                .map_err(|e| crate::notice::about_place(place(key), &kernel_error(e)))?;
            let common = ctx
                .kernel
                .boolean(BooleanOp::Intersect, &[&shape], &slab)
                .map_err(|e| crate::notice::about_place(place(key), &kernel_error(e)))?;
            let pieces = common.pieces.into_iter().map(|p| p.shape).collect();
            let Some(panel) = one_shape(ctx.kernel, pieces)? else {
                return Err(crate::notice::placed(
                    "a face's panel holds no material of the body",
                    place(key),
                    format!("the panel of face {key} holds no material of body {body}"),
                ));
            };
            tools.push(slab);
            panels.push(panel);
        }
        // Panels of one step must not overlap: faces that meet at an edge
        // go in separate steps, so that the order decides the joint.
        let mut pairs = Vec::new();
        let mut which = Vec::new();
        for i in 0..panels.len() {
            for j in i + 1..panels.len() {
                pairs.push(PlacedPair {
                    a: &panels[i],
                    a_placement: Transform::IDENTITY,
                    b: &panels[j],
                    b_placement: Transform::IDENTITY,
                });
                which.push((i, j));
            }
        }
        if !pairs.is_empty() {
            let volumes = ctx
                .kernel
                .overlap_volumes(&pairs, 1e-6)
                .map_err(kernel_error)?;
            if let Some(&(i, j)) = which
                .iter()
                .zip(&volumes)
                .find(|(_, volume)| **volume > 1e-6)
                .map(|(pair, _)| pair)
            {
                let (a, b) = (&self.faces[i], &self.faces[j]);
                return Err(crate::notice::said_at(
                    "the panels of two faces overlap: faces that meet at an edge are \
                     panelized in separate steps, the first one running through the corner",
                    format!(
                        "the panels of faces {a} and {b} overlap: faces that meet at an edge \
                         are panelized in separate steps"
                    ),
                    [place(a), place(b)],
                ));
            }
        }
        let mut changes = Vec::with_capacity(panels.len() + 1);
        if self.remove_remainder {
            changes.push(BodyChange::Remove(self.body));
        } else {
            let mut remainder = Some(shape);
            for tool in &tools {
                let Some(left) = remainder else { break };
                let cut = ctx
                    .kernel
                    .boolean(BooleanOp::Cut, &[&left], tool)
                    .map_err(kernel_error)?;
                remainder = if cut.touched.first().copied().unwrap_or(true) {
                    one_shape(
                        ctx.kernel,
                        cut.pieces.into_iter().map(|p| p.shape).collect(),
                    )?
                } else {
                    Some(left)
                };
            }
            changes.push(match remainder {
                Some(left) => BodyChange::Set(self.body, left),
                // Nothing is left: the panels took the whole body.
                None => BodyChange::Remove(self.body),
            });
        }
        for (i, panel) in panels.into_iter().enumerate() {
            changes.push(BodyChange::Set(panel_body(ctx.uid, i), panel));
        }
        Ok(FeatureOutput {
            changes,
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::panel_name;

    #[test]
    fn panels_are_named_by_their_normal() {
        assert_eq!(panel_name(Some([-1.0, 0.0, 0.0]), 0), "Left");
        assert_eq!(panel_name(Some([1.0, 0.0, 0.0]), 0), "Right");
        assert_eq!(panel_name(Some([0.0, -1.0, 0.0]), 0), "Front");
        assert_eq!(panel_name(Some([0.0, 2.0, 0.0]), 0), "Back");
        assert_eq!(panel_name(Some([0.0, 0.0, -1.0]), 0), "Bottom");
        assert_eq!(panel_name(Some([0.0, 0.0, 1.0]), 0), "Top");
        assert_eq!(panel_name(Some([0.6, 0.8, 0.0]), 2), "Panel 3");
        assert_eq!(panel_name(None, 0), "Panel 1");
    }
}
