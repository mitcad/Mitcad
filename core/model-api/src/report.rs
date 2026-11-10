// SPDX-License-Identifier: MIT
//! A readable summary of a document: the timeline with statuses and the
//! bodies with their measurements (`mitcad-cli info`).

use std::fmt::Write;

use super::ApiError;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::kernel::Kernel;

fn point(p: [f64; 3]) -> String {
    // + 0.0 turns -0.0 into 0.0.
    format!("[{:.3}, {:.3}, {:.3}]", p[0] + 0.0, p[1] + 0.0, p[2] + 0.0)
}

/// A document's report. An extension trait (mitcad#777), so that the api
/// can live in a crate above the model.
pub trait ReportApi<K: Kernel> {
    /// The report as text, or as the JSON of the `report` query.
    fn report(&self, as_json: bool) -> Result<String, ApiError>;
}

impl<K: Kernel> ReportApi<K> for Document<K> {
    fn report(&self, as_json: bool) -> Result<String, ApiError> {
        if as_json {
            let value: serde_json::Value =
                serde_json::from_str(&self.query(r#"{"query": "report"}"#)?).expect("valid JSON");
            return Ok(serde_json::to_string_pretty(&value).expect("serializes") + "\n");
        }
        let mut text = String::new();
        for warning in self.load_warnings() {
            let _ = writeln!(text, "Warning: {warning}");
        }
        let marker = if self.marker() == self.features().count() {
            "at the end".to_owned()
        } else {
            format!("after {} features", self.marker())
        };
        let _ = writeln!(text, "Timeline (marker {marker}):");
        for item in self.timeline() {
            let status = match (item.status, item.message()) {
                (Some(FeatureStatus::Failed(error)), _) => format!("error: {}", error.agent_text()),
                (Some(FeatureStatus::Ok), Some(warnings)) => format!("warning: {warnings}"),
                // A placeholder of an imported item (mitcad#170).
                (Some(FeatureStatus::Suppressed), Some(note)) => format!("suppressed: {note}"),
                _ => item.status_text().to_owned(),
            };
            let _ = writeln!(
                text,
                "  {} {} ({}): {status}",
                item.entry.uid,
                item.entry.name,
                item.entry.def.type_name()
            );
        }
        let bodies = self.bodies();
        let _ = writeln!(text, "Bodies: {}", bodies.len());
        for body in bodies {
            let kernel = self.kernel();
            let error =
                |e: mitcad_model::kernel::KernelError| ApiError(format!("{}: {e}", body.name));
            let mass = kernel.mass_properties(body.shape).map_err(error)?;
            let faces = kernel.faces(body.shape).map_err(error)?.len();
            let edges = kernel.edges(body.shape).map_err(error)?.len();
            let _ = writeln!(
                text,
                "  {} ({}): volume {:.3} mm3, area {:.3} mm2, {faces} faces, {edges} edges",
                body.name, body.uid, mass.volume, mass.area
            );
            let _ = writeln!(text, "    center of mass {}", point(mass.center));
            if let Some(bounds) = kernel.bounding_box(body.shape).map_err(error)? {
                let _ = writeln!(
                    text,
                    "    bounding box {} - {}",
                    point(bounds.min),
                    point(bounds.max)
                );
            }
        }
        if !self.assembly().is_empty() {
            self.assembly_report(&mut text)?;
        }
        Ok(text)
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait ReportLocal<K: Kernel> {
    /// Components, occurrences and the bodies as placed (F6).
    fn assembly_report(&self, text: &mut String) -> Result<(), ApiError>;
}

impl<K: Kernel> ReportLocal<K> for Document<K> {
    fn assembly_report(&self, text: &mut String) -> Result<(), ApiError> {
        let a = self.assembly();
        let _ = writeln!(
            text,
            "Components: {} (active {})",
            a.components.len() + 1,
            a.name(a.active)
        );
        let features = |c| self.features().filter(|f| f.component == c).count();
        let _ = writeln!(
            text,
            "  C0 {}: {} features",
            a.root_name,
            features(mitcad_model::ids::ComponentUid::ROOT)
        );
        for c in &a.components {
            let link = c
                .link
                .as_ref()
                .map_or(String::new(), |l| format!(", linked from {}", l.path));
            let _ = writeln!(
                text,
                "  {} {}: {} features, {} occurrences{link}",
                c.uid,
                c.name,
                features(c.uid),
                a.occurrences_of(c.uid).count()
            );
        }
        let instances = self.instances();
        let _ = writeln!(text, "Placed bodies: {}", instances.len());
        let kernel = self.kernel();
        for i in instances {
            let Some(shape) = self.body_shape(i.body) else {
                continue;
            };
            let error = |e: mitcad_model::kernel::KernelError| ApiError(format!("{}: {e}", i.name));
            let at = if i.path_name.is_empty() {
                a.root_name.clone()
            } else {
                i.path_name.clone()
            };
            // A body where its component has it as it is, without a copy.
            let placed = if i.transform.is_identity() {
                shape.clone()
            } else {
                match kernel.transform_shape(shape, &i.transform, None) {
                    Ok(placed) => placed,
                    // A body the kernel cannot move where the design places
                    // it (one stored invalid in a file the design refers
                    // to, mitcad#196) is reported, not the end of the
                    // report.
                    Err(e) => {
                        let _ = writeln!(text, "  {at}/{} ({}): not placed: {e}", i.name, i.body);
                        continue;
                    }
                }
            };
            let mass = kernel.mass_properties(&placed).map_err(error)?;
            let hidden = if i.visible { "" } else { " (hidden)" };
            let _ = writeln!(
                text,
                "  {at}/{} ({}){hidden}: volume {:.3} mm3, center of mass {}",
                i.name,
                i.body,
                mass.volume,
                point(mass.center)
            );
            if let Some(bounds) = kernel.bounding_box(&placed).map_err(error)? {
                let _ = writeln!(
                    text,
                    "    bounding box {} - {}",
                    point(bounds.min),
                    point(bounds.max)
                );
            }
        }
        Ok(())
    }
}
