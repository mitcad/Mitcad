// SPDX-License-Identifier: MIT
//! 3MF files (`.3mf`) as mesh bodies and components (mitcad#2156), read by
//! `mitcad-3mf` ([`mitcad_3mf::read_for_import`]): a slicer's project too,
//! whose meshes are in other model parts of the package (the Production
//! extension). The print settings in the package are left alone (Import
//! from Slicer reads them).
//!
//! - Each part of the file (a mesh object, followed through the
//!   components) is a mesh body named after the part: its `name`, else the
//!   name the package's settings file gives its object, else `Object <id>`.
//! - A part placed once is a body in the component imported into, its
//!   placement moved into the triangles. A part placed more than once is a
//!   component named after the part, holding the mesh once in the part's
//!   own coordinates, with an occurrence per placement. A placement that
//!   scales, shears or mirrors can be no occurrence: that copy is a body
//!   moved into place (a warning says so).
//! - Lengths are in the file's unit, read in millimetres (`unit_mm` is not
//!   read).
//! - Each base material colour becomes one document appearance named after
//!   the material (made unique); the bodies also keep the colour as their
//!   colour.
//! - A mesh that is not closed comes in as a mesh body with a warning
//!   (Convert Mesh refuses it later, as for STL); a file that cannot be read
//!   (not a zip archive, no model part, an extension it requires that is
//!   not read) is refused in words.
//!
//! One undo step; nothing stays of a failed import.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::{
    BaseInput, ImportBody, ImportWrapper, Imported, ImportedComponent, ImportedFile, file_name,
    invalid,
};
use crate::appearance::{self, AppearanceChange};
use crate::document::{Document, ModelError};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::transform::Transform;

/// Whether the file at `path` is a 3MF file by its extension (`.3mf`, any
/// case).
pub fn is_3mf_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("3mf"))
}

/// A 3MF file read for the import, or the error in words.
fn read(path: &str) -> Result<mitcad_3mf::ImportRead, ModelError> {
    let source = file_name(path);
    let data = std::fs::read(path).map_err(|e| invalid(format!("cannot read {path}: {e}")))?;
    mitcad_3mf::read_for_import(&data).map_err(|e| {
        eprintln!("import: 3MF: {path}: {e}");
        invalid(format!("{source} cannot be imported as a 3MF file: {e}"))
    })
}

/// A placement as an occurrence's transform, when it is a rotation and a
/// translation.
fn rigid(placement: &mitcad_3mf::Placement) -> Option<Transform> {
    let t = Transform {
        linear: placement.linear,
        translation: placement.translation,
    };
    (t.is_finite() && crate::assembly::is_rigid(&t)).then_some(t)
}

/// A body planned: its name, its part (for its colour) and its triangles
/// in the coordinates of what holds it (mm).
struct PlannedBody {
    name: String,
    part: usize,
    mesh: mitcad_3mf::Mesh,
}

/// A part placed more than once: a component of its body, placed by each
/// occurrence.
struct PlannedComponent {
    name: String,
    body: PlannedBody,
    occurrences: Vec<Transform>,
}

/// The file's parts as bodies and components, before anything is made;
/// how many copies came in moved into place for want of a rigid placement.
fn plan(model: mitcad_3mf::Model) -> (Vec<PlannedBody>, Vec<PlannedComponent>, usize) {
    let mut top = Vec::new();
    let mut components = Vec::new();
    let mut baked = 0;
    for (index, part) in model.parts.into_iter().enumerate() {
        let (occurrences, others): (Vec<_>, Vec<_>) = part
            .placements
            .iter()
            .map(|p| (p, rigid(p)))
            .partition(|(_, r)| r.is_some());
        if occurrences.len() > 1 {
            baked += others.len();
            for (placement, _) in others {
                top.push(PlannedBody {
                    name: part.name.clone(),
                    part: index,
                    mesh: part.mesh.placed(placement),
                });
            }
            components.push(PlannedComponent {
                name: part.name.clone(),
                occurrences: occurrences.into_iter().filter_map(|(_, r)| r).collect(),
                body: PlannedBody {
                    name: part.name,
                    part: index,
                    mesh: part.mesh,
                },
            });
        } else {
            if part.placements.len() > 1 {
                baked += others.len();
            }
            for placement in &part.placements {
                top.push(PlannedBody {
                    name: part.name.clone(),
                    part: index,
                    mesh: part.mesh.placed(placement),
                });
            }
        }
    }
    (top, components, baked)
}

impl<K: Kernel> Document<K> {
    /// Imports a 3MF file (see the module): with `wrap` into a new
    /// component named after the file. One undo step (`Import <file>`).
    pub(crate) fn import_3mf(
        &mut self,
        path: &str,
        name: Option<&str>,
        wrap: bool,
    ) -> Result<(ImportedFile, Option<ImportWrapper>), ModelError> {
        let mitcad_3mf::ImportRead {
            model,
            materials,
            warnings: read_warnings,
        } = read(path)?;
        let source = file_name(path);
        for warning in &read_warnings {
            eprintln!("import: 3MF: {source}: {warning}");
        }
        let mut warnings = read_warnings;
        let colors: Vec<Option<[f64; 3]>> = model.parts.iter().map(|p| p.color).collect();
        let (top, components, baked) = plan(model);
        if baked > 0 {
            warnings.push(format!(
                "copies placed by a placement that scales or mirrors cannot be occurrences, so \
                 they came in moved into place: {baked}"
            ));
        }

        // The bodies' shapes, made before anything changes.
        let kernel = self.kernel();
        let make = |planned: PlannedBody, warnings: &mut Vec<String>| {
            if planned.mesh.triangles.is_empty() {
                warnings.push(format!("{} has no triangles: left out", planned.name));
                return None;
            }
            match kernel.triangle_body(&planned.mesh.vertices, &planned.mesh.triangles) {
                Ok(shape) => Some((
                    ImportBody {
                        name: Some(planned.name),
                        color: colors[planned.part],
                        shape,
                        face_colors: Vec::new(),
                    },
                    planned.part,
                )),
                Err(e) => {
                    eprintln!("import: 3MF: {source}: {}: {e}", planned.name);
                    warnings.push(format!(
                        "{} cannot be made a mesh body: left out",
                        planned.name
                    ));
                    None
                }
            }
        };
        let top: Vec<_> = top
            .into_iter()
            .filter_map(|b| make(b, &mut warnings))
            .collect();
        let components: Vec<_> = components
            .into_iter()
            .filter_map(|c| {
                let body = make(c.body, &mut warnings)?;
                Some((c.name, body, c.occurrences))
            })
            .collect();
        if top.is_empty() && components.is_empty() {
            let why = if warnings.is_empty() {
                String::new()
            } else {
                format!(": {}", warnings.join("; "))
            };
            return Err(invalid(format!("{source} has no triangles to import{why}")));
        }

        let label = format!("Import {source}");
        let depth = self.undo_depth();
        match self.apply_3mf(path, name, wrap, top, components, &materials) {
            Ok((file, wrapper)) => {
                self.merge_undo(depth, &label);
                let file = match file {
                    ImportedFile::Base(mut imported) => {
                        imported.warnings = warnings;
                        ImportedFile::Base(imported)
                    }
                    ImportedFile::Components {
                        components, loose, ..
                    } => ImportedFile::Components {
                        components,
                        warnings,
                        loose,
                    },
                };
                Ok((file, wrapper))
            }
            Err(e) => {
                self.undo_back_to(depth);
                Err(e)
            }
        }
    }

    /// Makes the components and their occurrences, a base feature per
    /// component and one of the bodies of the component imported into, and
    /// the colours' appearances.
    #[allow(clippy::type_complexity)]
    fn apply_3mf(
        &mut self,
        path: &str,
        name: Option<&str>,
        wrap: bool,
        top: Vec<(ImportBody<K::Shape>, usize)>,
        components: Vec<(String, (ImportBody<K::Shape>, usize), Vec<Transform>)>,
        materials: &[Option<String>],
    ) -> Result<(ImportedFile, Option<ImportWrapper>), ModelError> {
        let source = file_name(path);
        let label = format!("Import {source}");
        let wrapper = if wrap {
            Some(self.add_import_wrapper(path)?)
        } else {
            None
        };
        let parent = wrapper
            .as_ref()
            .map_or_else(|| self.active_component(), |w| w.component);

        // The components, each placed by its occurrences.
        let mut made: Vec<ImportedComponent> = Vec::new();
        let mut inputs = Vec::new();
        // Each base feature's bodies' parts, for their appearances.
        let mut parts_of: Vec<Vec<usize>> = Vec::new();
        for (component_name, (body, part), occurrences) in components {
            let mut placements = occurrences.into_iter();
            let Some(first) = placements.next() else {
                continue;
            };
            let (component, occurrence) =
                self.add_component(Some(component_name.as_str()), parent, first)?;
            let mut all = vec![occurrence];
            for placement in placements {
                all.push(self.add_occurrence(component, parent, placement)?);
            }
            made.push(ImportedComponent {
                component,
                name: self.assembly().name(component),
                occurrences: all,
                feature: None,
            });
            inputs.push(BaseInput {
                source: Some(source.clone()),
                component: Some(component),
                ..BaseInput::new(vec![body])
            });
            parts_of.push(vec![part]);
        }
        let structured = !made.is_empty();
        if !top.is_empty() {
            let (bodies, parts): (Vec<_>, Vec<_>) = top.into_iter().unzip();
            inputs.push(BaseInput {
                name: name.map(str::to_owned),
                source: Some(source.clone()),
                component: Some(parent),
                ..BaseInput::new(bodies)
            });
            parts_of.push(parts);
        }
        let added = self.add_base_features(&label, inputs)?;
        self.colors_as_appearances(&added, &parts_of, materials)?;

        let mut added = added.into_iter();
        for component in &mut made {
            component.feature = added.next();
        }
        let loose: Option<Imported> = added.next();
        let file = if structured {
            ImportedFile::Components {
                components: made,
                warnings: Vec::new(),
                loose,
            }
        } else {
            ImportedFile::Base(loose.ok_or_else(|| invalid("the import made no bodies"))?)
        };
        Ok((file, wrapper))
    }

    /// Each base material colour of the bodies as one appearance named after
    /// the material (made unique among the appearances; `3MF color
    /// #RRGGBB` without a name), set on its bodies.
    fn colors_as_appearances(
        &mut self,
        added: &[Imported],
        parts_of: &[Vec<usize>],
        materials: &[Option<String>],
    ) -> Result<(), ModelError> {
        let mut taken: BTreeSet<String> = appearance::library()
            .iter()
            .chain(self.custom_appearances())
            .map(|a| a.name.clone())
            .collect();
        // By the material's name and colour (bytes), each made once.
        let mut ids: HashMap<(Option<String>, [u8; 3]), String> = HashMap::new();
        for (imported, parts) in added.iter().zip(parts_of) {
            for (i, &part) in parts.iter().enumerate() {
                let uid = BodyUid::new(imported.feature.uid, i as u32);
                let Some(color) = self.imported_color(uid) else {
                    continue;
                };
                let material = materials
                    .get(part)
                    .cloned()
                    .flatten()
                    .map(|m| m.trim().to_owned())
                    .filter(|m| !m.is_empty());
                let bytes = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                let key = (material.clone(), bytes);
                let id = match ids.get(&key) {
                    Some(id) => id.clone(),
                    None => {
                        let base = material.unwrap_or_else(|| {
                            format!(
                                "3MF color #{:02X}{:02X}{:02X}",
                                bytes[0], bytes[1], bytes[2]
                            )
                        });
                        let change = AppearanceChange {
                            name: Some(super::unique_name(&base, &mut taken)),
                            base_color: Some(color),
                            ..AppearanceChange::default()
                        };
                        let id = self.create_appearance(&change)?;
                        ids.insert(key, id.clone());
                        id
                    }
                };
                self.set_body_appearance(uid, Some(id.as_str()))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(size: f64) -> mitcad_3mf::Mesh {
        let vertices = (0..8)
            .map(|i| {
                [
                    size * f64::from(i & 1),
                    size * f64::from((i >> 1) & 1),
                    size * f64::from((i >> 2) & 1),
                ]
            })
            .collect();
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        mitcad_3mf::Mesh {
            vertices,
            triangles: quads
                .iter()
                .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
                .collect(),
            triangle_colors: Vec::new(),
        }
    }

    fn moved(x: f64) -> mitcad_3mf::Placement {
        mitcad_3mf::Placement {
            translation: [x, 0.0, 0.0],
            ..mitcad_3mf::Placement::IDENTITY
        }
    }

    #[test]
    fn a_part_placed_twice_is_a_component_and_a_scaled_copy_a_body() {
        let part = |name: &str, placements| mitcad_3mf::Part {
            name: name.to_owned(),
            color: None,
            mesh: cube(1.0),
            placements,
            metadata: Vec::new(),
        };
        let mut scaled = moved(40.0);
        scaled.linear[0][0] = 2.0;
        let model = mitcad_3mf::Model {
            parts: vec![
                part("Plate", vec![moved(5.0)]),
                part("Pin", vec![moved(10.0), moved(20.0), scaled]),
            ],
            ..mitcad_3mf::Model::default()
        };
        let (top, components, baked) = plan(model);
        assert_eq!(baked, 1);
        // The plate moved into place, the pin's scaled copy too.
        let names: Vec<&str> = top.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Plate", "Pin"]);
        assert_eq!(top[0].mesh.vertices[7], [6.0, 1.0, 1.0]);
        assert_eq!(top[1].mesh.vertices[7], [42.0, 1.0, 1.0]);
        // The pin once in its own coordinates, placed twice.
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].name, "Pin");
        assert_eq!(components[0].body.mesh, cube(1.0));
        let at: Vec<f64> = components[0]
            .occurrences
            .iter()
            .map(|t| t.translation[0])
            .collect();
        assert_eq!(at, [10.0, 20.0]);
    }

    #[test]
    fn three_mf_files_are_known_by_their_extension() {
        assert!(is_3mf_path("/prints/plate.3mf"));
        assert!(is_3mf_path("C:\\prints\\PLATE.3MF"));
        assert!(!is_3mf_path("/prints/plate.stl"));
    }
}
