// SPDX-License-Identifier: MIT
//! The Center of Mass marker (mitcad#1094; commands.md "Center of Mass"):
//! the `center_of_mass` query, the center of mass of the design's bodies
//! as they are placed. A body counts once per occurrence that places it,
//! at the occurrence's place in the design and with the material it has
//! there (its own, else the nearest component default up the occurrence
//! path, else the root's), so that moving a component or changing a
//! material moves the marker. A body without a material anywhere has no
//! mass and is left out (mitcad#1817: never steel unasked); the answer
//! says how many were. Over bodies that all have a material it is the
//! `properties` query's total over every body (mitcad#1110: both walk the
//! bodies' placements the same way, `physical_extent.rs`). The marker
//! itself is display state (`document/display.rs`).

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
    /// coordinates; null without bodies of a material), `mass` (kg),
    /// `volume` (mm³), `bodies` (the bodies counted, one per placement) and
    /// `without_material` (how many placements of bodies without a
    /// material were left out); `marker`: the marker's state,
    /// `{"visible": bool}`, or null when the design has none.
    fn center_of_mass_json(&self) -> Result<Value, ApiError>;
}

impl<K: Kernel> CenterOfMassOps<K> for Document<K> {
    fn center_of_mass_json(&self) -> Result<Value, ApiError> {
        // The `properties` query's walk over every body and its placements
        // (mitcad#1110): a body placed nowhere does not count here.
        let scope = self.properties_scope(&[], &[])?;
        let mut computed = PlacedProperties::new(self);
        let mut placed = Vec::new();
        let mut without_material = 0usize;
        for (body, placements) in &scope.bodies {
            for placement in placements {
                let (material, source) = self.body_material(body.uid, placement.material_path());
                // No material, no mass (mitcad#1817): left out, not steel.
                if source == MaterialSource::Default {
                    without_material += 1;
                    continue;
                }
                placed.push(computed.at(
                    body.uid,
                    body.shape,
                    &body.name,
                    material.density,
                    placement,
                )?);
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
            "without_material": without_material,
            "marker": marker,
        }))
    }
}
