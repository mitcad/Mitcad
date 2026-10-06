// SPDX-License-Identifier: MIT
//! A readable summary of a document: the timeline with statuses and the
//! bodies with their measurements (`mitcad-cli info`).

use std::fmt::Write;

use super::ApiError;
use crate::document::Document;
use crate::kernel::Kernel;
use crate::recompute::FeatureStatus;

fn point(p: [f64; 3]) -> String {
    // + 0.0 turns -0.0 into 0.0.
    format!("[{:.3}, {:.3}, {:.3}]", p[0] + 0.0, p[1] + 0.0, p[2] + 0.0)
}

impl<K: Kernel> Document<K> {
    /// The report as text, or as the JSON of the `report` query.
    pub fn report(&self, as_json: bool) -> Result<String, ApiError> {
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
                (Some(FeatureStatus::Failed(message)), _) => format!("error: {message}"),
                (Some(FeatureStatus::Ok), Some(warnings)) => format!("warning: {warnings}"),
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
            let error = |e: crate::kernel::KernelError| ApiError(format!("{}: {e}", body.name));
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

    /// Components, occurrences and the bodies as placed (F6).
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
            features(crate::ids::ComponentUid::ROOT)
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
            let error = |e: crate::kernel::KernelError| ApiError(format!("{}: {e}", i.name));
            // A body where its component has it as it is, without a copy.
            let placed = if i.transform.is_identity() {
                shape.clone()
            } else {
                kernel
                    .transform_shape(shape, &i.transform, None)
                    .map_err(error)?
            };
            let mass = kernel.mass_properties(&placed).map_err(error)?;
            let at = if i.path_name.is_empty() {
                a.root_name.clone()
            } else {
                i.path_name.clone()
            };
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
