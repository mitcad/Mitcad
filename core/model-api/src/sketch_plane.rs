// SPDX-License-Identifier: MIT
//! `sketch.redefine_plane` (mitcad#787, Redefine Sketch Plane): a sketch
//! moved to another plane or planar face and placed there by an offset, a
//! move, a rotation and a flip; see `commands.md`, "Redefine Sketch Plane".
//! The model does the change ([`Document::redefine_sketch_plane`]); here a
//! sketch on a new plane is also stood upright, as Create Sketch's are, in
//! the same undo step.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::features::sketch::{FrameDef, SketchDef};
use mitcad_model::features::{FeatureDef, OccurrencePath, SketchPlane, ValueInput};
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::ParamId;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, SketchPlaneChange};

/// The command's fields.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RedefinePlane {
    sketch: FeatureUid,
    /// The new plane, as `sketch.create` takes it; the sketch's own when
    /// left out.
    #[serde(default)]
    plane: Option<SketchPlane>,
    /// As in `sketch.create` (mitcad#100): where the plane was picked, and
    /// where the sketch's component is seen.
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    #[serde(default)]
    context: Option<OccurrencePath>,
    /// The place on the plane; a value left out is kept, a plain zero
    /// takes it away.
    #[serde(default)]
    offset: Option<ValueInput>,
    #[serde(default)]
    x: Option<ValueInput>,
    #[serde(default)]
    y: Option<ValueInput>,
    #[serde(default)]
    angle: Option<ValueInput>,
    #[serde(default)]
    flip: Option<bool>,
    /// On a new plane: the own frame that shows the design upright, as
    /// Create Sketch's `upright` (mitcad#692).
    #[serde(default)]
    upright: bool,
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait SketchPlaneOps<K: Kernel> {
    /// `sketch.redefine_plane`.
    fn redefine_plane_command(&mut self, command: RedefinePlane) -> Result<Value, ApiError>;
}

impl<K: Kernel> SketchPlaneOps<K> for Document<K> {
    fn redefine_plane_command(&mut self, command: RedefinePlane) -> Result<Value, ApiError> {
        let uid = command.sketch;
        let depth = self.undo_depth();
        let change = SketchPlaneChange {
            plane: command.plane,
            occurrence: command.occurrence.as_ref().map(|p| p.0.clone()),
            context: command.context.as_ref().map(|p| p.0.clone()),
            offset: command.offset,
            x: command.x,
            y: command.y,
            angle: command.angle,
            flip: command.flip,
        };
        let changed = self.redefine_sketch_plane(uid, &change)?;
        let mut created = changed.created;
        if command.upright && changed.plane_changed {
            let shown = command.context.as_ref().or(command.occurrence.as_ref());
            created.extend(self.stand_upright_on_plane(uid, shown)?);
            self.merge_undo(depth, &changed.label);
        }
        let entry = self
            .feature(uid)
            .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
        let FeatureDef::Sketch(def) = &entry.def else {
            return Err(ApiError(format!("{} ({uid}) is not a sketch", entry.name)));
        };
        let params = self.parameters();
        let placement = def.placement.as_ref().map(|p| {
            p.map_value(&mut |_, id: &ParamId| Ok::<_, ()>(params.name(*id)))
                .expect("names never fail")
        });
        let mut value = json!({
            "uid": uid,
            "name": entry.name,
            "plane": def.plane,
            "plane_link": def.plane_link,
            "placement": placement,
            "plane_changed": changed.plane_changed,
            "label": changed.label,
            "parameters": created,
            "warnings": self.warnings(uid),
        });
        if let Some(output) = self.sketch_output(uid) {
            value["frame"] = json!(output.frame);
        }
        Ok(value)
    }
}

/// The document's methods used only in this module.
trait SketchPlaneLocal<K: Kernel> {
    /// Gives sketch `uid`, just moved to a new plane without an own frame,
    /// the own frame that shows the design upright on it
    /// ([`FrameDef::upright`]), when that differs from the plane's frame.
    /// The design's axes are taken where the sketch's component is shown
    /// (`shown`, else its first occurrence). Returns the parameters made.
    fn stand_upright_on_plane(
        &mut self,
        uid: FeatureUid,
        shown: Option<&OccurrencePath>,
    ) -> Result<Vec<String>, ApiError>;
}

impl<K: Kernel> SketchPlaneLocal<K> for Document<K> {
    fn stand_upright_on_plane(
        &mut self,
        uid: FeatureUid,
        shown: Option<&OccurrencePath>,
    ) -> Result<Vec<String>, ApiError> {
        // A sketch rolled back by the marker has no frame yet.
        let Some(plane) = self.sketch_output(uid).map(|output| output.base) else {
            return Ok(Vec::new());
        };
        let Some(entry) = self.feature(uid) else {
            return Ok(Vec::new());
        };
        let FeatureDef::Sketch(def) = &entry.def else {
            return Ok(Vec::new());
        };
        let to_component = mitcad_model::internal::links::first_path(
            self.assembly(),
            entry.component,
            shown.map(|p| &p.0[..]),
        )
        .map(|path| mitcad_model::assembly::inverse(&self.path_transform(&path)))
        .unwrap_or(Transform::IDENTITY);
        let axis = |v: [f64; 3]| to_component.apply_vector(v);
        let Some(own) = FrameDef::upright(
            &plane,
            axis([0.0, 0.0, 1.0]),
            axis([0.0, 1.0, 0.0]),
            super::sketch::seen_from(&def.plane, axis),
        ) else {
            return Ok(Vec::new());
        };
        let params = self.parameters();
        let mut input: SketchDef<ValueInput> = def
            .map_params(&mut |_, id: &ParamId| Ok::<_, ()>(ValueInput::Name(params.name(*id))))
            .expect("names never fail");
        input.frame = Some(Box::new(own));
        Ok(self.edit_feature(uid, &FeatureDef::Sketch(input))?)
    }
}
