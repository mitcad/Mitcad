// SPDX-License-Identifier: MIT
//! Bodies the file keeps: an Import feature's STEP text and a mesh
//! import's triangles, as base features (bodies without history, as
//! SolidCraft keeps them). And body transfers between components.

use mitcad_model::features::Operation;
use mitcad_model::{BaseInput, BodyUid, ComponentUid, ImportBody, Kernel, Transform};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::kinds as k;
use serde_json::json;

use super::{Importer, MapResult, Mapped, NotMapped, Outcome, transform};

impl<K: Kernel> Importer<'_, '_, K> {
    /// The STEP text of an Import feature, read by the kernel from a
    /// temporary file.
    pub(super) fn import_step(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let i: k::Import = f.typed()?;
        if i.step.trim().is_empty() {
            return Err(NotMapped::Failed(
                "the Import feature keeps no STEP text".to_owned(),
            ));
        }
        // A file name of this read's own: imports running at the same time
        // in one process (tests on parallel threads) may each have an
        // Import feature of the same id (mitcad#1017).
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mitcad-solidcraft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("import-{}-{n}.step", f.id));
        std::fs::write(&path, i.step.as_bytes()).map_err(|e| e.to_string())?;
        let read = self.doc.kernel().read_file(
            &path.to_string_lossy(),
            &mitcad_model::exchange::ReadOptions { unit_mm: 1.0 },
        );
        let _ = std::fs::remove_file(&path);
        let bodies = read.map_err(|e| format!("{}: {e}", i.file))?;
        let mut notes = Vec::new();
        let bodies: Vec<ImportBody<K::Shape>> = bodies
            .into_iter()
            .enumerate()
            .filter(|(n, _)| !i.skip.contains(&(*n as u64)))
            .map(|(_, b)| b)
            .collect();
        if !i.skip.is_empty() {
            notes.push(format!(
                "{} instances shown by other occurrences are left out",
                i.skip.len()
            ));
        }
        self.add_bodies(f, component, Some(i.file.clone()), bodies)?;
        Ok(Mapped::partial(notes))
    }

    /// Mesh bodies of a mesh import.
    pub(super) fn import_meshes(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let m: k::MeshImport = f.typed()?;
        let mut bodies = Vec::new();
        for mesh in &m.meshes {
            let points: Vec<[f64; 3]> = mesh.positions.as_chunks::<3>().0.to_vec();
            let triangles: Vec<[u32; 3]> = mesh.triangles.as_chunks::<3>().0.to_vec();
            let shape = self
                .doc
                .kernel()
                .triangle_body(&points, &triangles)
                .map_err(|e| format!("mesh {}: {e}", mesh.name))?;
            bodies.push(ImportBody {
                name: Some(mesh.name.clone()),
                color: mesh.color.map(|c| [c[0] as f64, c[1] as f64, c[2] as f64]),
                shape,
            });
        }
        self.add_bodies(f, component, Some(m.file.clone()), bodies)?;
        Ok(Mapped::parametric())
    }

    fn add_bodies(
        &mut self,
        f: &Feature,
        component: ComponentUid,
        source: Option<String>,
        bodies: Vec<ImportBody<K::Shape>>,
    ) -> Result<(), String> {
        let before = self.body_versions();
        let imported = self
            .doc
            .add_base_feature(BaseInput {
                name: Some(f.name.clone()),
                source,
                bodies,
                operation: Operation::NewBody,
                participants: Vec::new(),
                replaces: Vec::new(),
                component: Some(component),
                cages: Vec::new(),
            })
            .map_err(|e| e.to_string())?;
        self.track_bodies(&before, imported.feature.uid);
        Ok(())
    }

    /// Bodies moved into another component on the timeline (mitcad#465): a
    /// `body_transfer` in the destination (the file's feature is the
    /// destination's), the bodies by the file's names, seen at the source's
    /// and the destination's placements. The file records the conversion
    /// it made (from the source's coordinates into the destination's); it
    /// is kept as the feature's fixed conversion where the placements now
    /// give another (an occurrence moved after the transfer), else the
    /// placements give it and it follows them.
    pub(super) fn body_transfer(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let t: k::BodyTransfer = f.typed()?;
        let from = self.component_of(t.from);
        let to = self.component_of(t.to);
        if to != component {
            return Err(NotMapped::Failed(
                "the transfer is not in its destination component".to_owned(),
            ));
        }
        let bodies: Vec<BodyUid> = t
            .bodies
            .iter()
            .map(|name| self.body(name))
            .collect::<Result<_, _>>()?;
        if bodies
            .iter()
            .all(|b| self.doc.body_component(*b) == Some(to))
        {
            return Ok(Mapped {
                outcome: Outcome::Skipped,
                notes: vec![format!(
                    "its bodies are in {} already",
                    self.doc.assembly().name(to)
                )],
            });
        }
        let assembly = self.doc.assembly();
        let path = |c: ComponentUid| -> Result<String, NotMapped> {
            let paths = assembly.paths_to(c);
            let first = paths
                .first()
                .ok_or_else(|| NotMapped::Failed(format!("{} is not placed", assembly.name(c))))?;
            Ok(first
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/"))
        };
        let (source, target) = (path(from)?, path(to)?);
        let recorded = transform(&t.transform);
        if !recorded.is_finite() || !recorded.is_rigid() {
            return Err(NotMapped::Failed(
                "the recorded conversion is not a rigid motion".to_owned(),
            ));
        }
        // The conversion the placements give now.
        let along = |c: ComponentUid| {
            assembly
                .paths_to(c)
                .first()
                .map_or(Transform::IDENTITY, |p| {
                    p.iter().fold(Transform::IDENTITY, |acc, o| {
                        acc.after(&self.doc.placement(*o).unwrap_or(Transform::IDENTITY))
                    })
                })
        };
        let placed = mitcad_model::assembly::inverse(&along(to)).after(&along(from));
        let same = (0..3).all(|i| {
            (placed.translation[i] - recorded.translation[i]).abs() < 1e-6
                && (0..3).all(|j| (placed.linear[i][j] - recorded.linear[i][j]).abs() < 1e-9)
        });
        let mut def = json!({"type": "body_transfer",
            "bodies": bodies.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "link": {"source": source, "target": target}});
        let mut notes = Vec::new();
        if !same {
            def["transform"] = json!(mitcad_model::assembly::matrix_rows(&recorded));
            notes.push(
                "keeps the conversion the file records: the occurrences are placed elsewhere \
                 now than when the bodies moved"
                    .to_owned(),
            );
        }
        self.add(&def, Some(&f.name), to)?;
        Ok(Mapped::partial(notes))
    }

    /// A mesh body rebuilt as planes and cylinders (mitcad#462): Mitcad's
    /// Convert Mesh of the body by its name, the tolerance the input's
    /// parameter. The body keeps its name and id.
    pub(super) fn convert_mesh(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let m: k::MeshConvert = f.typed()?;
        let body = self.body(&m.body)?;
        let tolerance = self.slot(f, "/tolerance")?;
        let def = serde_json::json!({"type": "mesh_convert", "body": body.to_string(),
            "tolerance": tolerance});
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }
}
