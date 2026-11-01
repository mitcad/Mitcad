// SPDX-License-Identifier: MIT
//! The Center of Mass marker (mitcad#1094; commands.md "Center of Mass"):
//! the `center_of_mass` query, the center of mass of the design's bodies
//! as they are placed. A body counts once per occurrence that places it,
//! at the occurrence's place in the design and with the material it has
//! there (its own, else the nearest component default up the occurrence
//! path, else steel), so that moving a component or changing a material
//! moves the marker. It is the `properties` query's total over every body
//! (mitcad#1110: both walk the bodies' placements the same way,
//! `physical_extent.rs`). The marker itself is display state
//! (`document/display.rs`).

use serde_json::{Value, json};

use super::ApiError;
use super::physical_extent::{PhysicalExtentOps, PlacedProperties};
use mitcad_model::analysis;
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, MaterialSource};

/// The document's methods of this module, as an extension trait
/// (mitcad#777).
pub(super) trait CenterOfMassOps<K: Kernel> {
    /// The `center_of_mass` query: `center_of_mass` (mm, in the design's
    /// coordinates; null without bodies), `mass` (kg), `volume` (mm³),
    /// `bodies` (the bodies counted, one per placement) and
    /// `default_material` (how many of them have no material and count as
    /// steel); `marker`: the marker's state, `{"visible": bool}`, or null
    /// when the design has none.
    fn center_of_mass_json(&self) -> Result<Value, ApiError>;
}

impl<K: Kernel> CenterOfMassOps<K> for Document<K> {
    fn center_of_mass_json(&self) -> Result<Value, ApiError> {
        // The `properties` query's walk over every body and its placements
        // (mitcad#1110): a body placed nowhere does not count here.
        let scope = self.properties_scope(&[], &[])?;
        let mut computed = PlacedProperties::new(self);
        let mut placed = Vec::new();
        let mut default_material = 0usize;
        for (body, placements) in &scope.bodies {
            for placement in placements {
                let (material, source) = self.body_material(body.uid, placement.material_path());
                placed.push(computed.at(
                    body.uid,
                    body.shape,
                    &body.name,
                    material.density,
                    placement,
                )?);
                if source == MaterialSource::Default {
                    default_material += 1;
                }
            }
        }
        let total = analysis::combine(&placed);
        let center = if placed.is_empty() {
            Value::Null
        } else {
            json!(total.center_of_mass)
        };
        let marker = self
            .display()
            .center_of_mass
            .map_or(Value::Null, |m| json!({"visible": m.visible}));
        Ok(json!({
            "center_of_mass": center,
            "mass": total.mass,
            "volume": total.volume,
            "bodies": placed.len(),
            "default_material": default_material,
            "marker": marker,
        }))
    }
}
