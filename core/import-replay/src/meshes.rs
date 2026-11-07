// SPDX-License-Identifier: MIT
//! Mesh bodies the file stores (mitcad#1366): the triangles of the design's
//! mesh bodies ([`StoredGeometry::take_meshes`]), which no history
//! rebuilds, come in as base features.
//!
//! The timeline item that made them (`MeshFeature`) brings its meshes in
//! as one base feature in its component, where it stands on the timeline:
//! that is its translation, as the file's own base feature's stored bodies
//! are (an item suppressed in the file comes in suppressed). The meshes no
//! item took (an import of the stored bodies alone, or an item the replay
//! did not reach) come in at the end, one base feature per component. A
//! mesh body is a body without surfaces: the comparisons with the history
//! leave it out ([`crate::history::Sig::of`]).
//!
//! [`StoredGeometry::take_meshes`]: crate::history::StoredGeometry::take_meshes

use mitcad_dump::TimelineItem;
use mitcad_model::{BaseInput, BodyUid, ComponentUid, FeatureUid, ImportBody};

use crate::history::StoredMesh;
use crate::report::{InputProvenance, Outcome};
use crate::suppressed::{ROLLED_BACK, SUPPRESSED};
use crate::{ImportKernel, Importer};

/// The timeline item type of mesh bodies.
pub(crate) const MESH_FEATURE: &str = "MeshFeature";

/// The note of a mesh body item that came in as a base feature of its
/// meshes.
pub(crate) const FILE_MESHES: &str =
    "the file's mesh bodies as it stores them, as a base feature (a mesh has no history)";

/// The note of a mesh body item the file stores no mesh for.
const NO_MESHES: &str = "no mesh body: the file stores none for it";

impl<K: ImportKernel> Importer<'_, K> {
    /// A mesh body item (report entry `at`), suppressed in the file or
    /// not: its meshes as a base feature in its component.
    pub(crate) fn mesh_item(&mut self, at: usize, item: &TimelineItem, suppressed: bool) {
        let meshes = match item.f3d.as_ref().and_then(|f| f.object_id) {
            Some(id) => self.oracle.take_meshes(Some(id)),
            None => Vec::new(),
        };
        if meshes.is_empty() {
            self.note(
                at,
                Outcome::Skipped,
                if suppressed { SUPPRESSED } else { NO_MESHES },
            );
            return;
        }
        let name = self.report.items[at].name.clone();
        match self.add_meshes(Some(&name), meshes, self.component) {
            Ok(Some(uid)) => {
                let report = &mut self.report.items[at];
                report.features.push(uid.to_string());
                report.input_provenance = Some(InputProvenance::Read);
                // After the timeline marker, the marker goes before it at
                // the end, as before the other items there.
                if item.is_rolled_back == Some(true) && self.suppressed.rolled_from.is_some() {
                    self.note(
                        at,
                        Outcome::Parametric,
                        format!("{FILE_MESHES}; {ROLLED_BACK}"),
                    );
                } else {
                    self.note(at, Outcome::Parametric, FILE_MESHES);
                }
                // Suppressed with the run it is in (mitcad#180).
                if suppressed {
                    self.suppressed_in_run(at);
                }
            }
            Ok(None) => self.note(
                at,
                Outcome::Skipped,
                "none of its mesh bodies could be read (the warnings say why)",
            ),
            Err(e) => self.note(at, Outcome::Skipped, format!("its mesh bodies: {e}")),
        }
    }

    /// The mesh bodies no item took, at the end: a base feature of them
    /// per component.
    pub(crate) fn remaining_meshes(&mut self) {
        let mut groups: Vec<(ComponentUid, Vec<StoredMesh>)> = Vec::new();
        for mesh in self.oracle.take_meshes(None) {
            let component = self.components.of_body(mesh.component);
            match groups.iter_mut().find(|(c, _)| *c == component) {
                Some((_, list)) => list.push(mesh),
                None => groups.push((component, vec![mesh])),
            }
        }
        for (component, meshes) in groups {
            if let Err(e) = self.add_meshes(None, meshes, component) {
                self.report.warnings.push(format!("mesh bodies: {e}"));
            }
        }
    }

    /// One base feature of `meshes` in `component`, named `name`; `None`
    /// when none of them builds (each that does not is a warning).
    fn add_meshes(
        &mut self,
        name: Option<&str>,
        meshes: Vec<StoredMesh>,
        component: ComponentUid,
    ) -> Result<Option<FeatureUid>, String> {
        let mut bodies = Vec::new();
        let mut sources = Vec::new();
        // Each body's mesh body record (mitcad#1376).
        let mut records = Vec::new();
        for mesh in meshes {
            let record = mesh.id;
            let built = mesh.mesh.and_then(|m| {
                self.doc
                    .kernel()
                    .triangle_body(&m.points, &m.triangles)
                    .map_err(|e| e.to_string())
            });
            match built {
                Ok(shape) => {
                    bodies.push(ImportBody {
                        name: mesh.name,
                        color: None,
                        shape,
                    });
                    sources.push(mesh.source);
                    records.push(record);
                }
                Err(e) => self
                    .report
                    .warnings
                    .push(format!("mesh body {} not brought in: {e}", mesh.source)),
            }
        }
        if bodies.is_empty() {
            return Ok(None);
        }
        let label = name.map_or_else(|| "Add Mesh Bodies".to_owned(), |n| format!("Add {n}"));
        let source = format!("{} ({})", self.report.label, sources.join(", "));
        let mut inputs = vec![BaseInput {
            name: name.map(str::to_owned),
            source: Some(source),
            component: Some(component),
            ..BaseInput::new(bodies)
        }];
        let imported = match self.doc.add_base_features(&label, inputs.clone()) {
            Ok(i) => i,
            // The name is taken: the default name.
            Err(e) if e.to_string().contains("already named") => {
                inputs[0].name = None;
                self.doc
                    .add_base_features(&label, inputs)
                    .map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
        let uid = imported.first().map(|i| i.feature.uid);
        if let Some(e) = uid.and_then(|u| self.doc.status(u).and_then(|s| s.error())) {
            let e = e.to_owned();
            self.doc.undo();
            return Err(e);
        }
        // Body i of the base feature is mesh i (mitcad#1376): its material
        // and appearance come after the replay
        // (`mitcad-import-appearances`).
        if let Some(feature) = uid {
            for (i, record) in (0u32..).zip(records) {
                if let Some(record) = record {
                    self.mesh_bodies.push((record, BodyUid::new(feature, i)));
                }
            }
        }
        Ok(uid)
    }
}
