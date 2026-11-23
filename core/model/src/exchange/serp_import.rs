// SPDX-License-Identifier: MIT
//! `.serp` files (mitcad#1870) as bodies, read by `mitcad-serp`:
//!
//! - Each object whose shape is a B-rep becomes a body read by the kernel
//!   ([`Kernel::import_brep`], OCCT's binary B-rep format), each mesh a
//!   mesh body ([`Kernel::triangle_body`]), named after the object and
//!   coloured by its colour, else its layer's. Lengths are the file's
//!   units scaled to millimetres; the unit becomes the design's when the
//!   design has no features yet.
//! - The bodies of the design are one base feature; when they lie on more
//!   than one layer each body goes into a folder named after its layer's
//!   path (`Parts::Bolts`, [`Document::name_bodies`], mitcad#1802).
//!   Hidden objects, and objects on hidden layers, come in hidden.
//! - A block is a component named after it holding the block's shapes,
//!   placed by an occurrence per placed block. The file keeps no
//!   placement: each placed block's shape is the block's shapes moved, and
//!   the kernel finds the rigid motion ([`Kernel::rigid_copies`]). A placed
//!   block that does not match (scaled, mirrored, or changed after it was
//!   placed) comes in as a body where the file has it, with a warning.
//! - Point clouds whose points the container holds come in as point cloud
//!   items (mitcad#2240, [`crate::PointCloud`]) in the component the bodies
//!   go into, named after the file's clouds, shown as the file shows them.
//! - Curves and points (B-reps without faces), texts, hatches, dimensions,
//!   pictures, point clouds whose points cannot be read, scan paths,
//!   layouts and history records are left out: one warning line counts
//!   them by kind. An object whose data cannot be read is left out with a
//!   warning naming it in words.
//! - A version newer than the reader knows is refused in words.
//!
//! One undo step; nothing stays of a failed import.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mitcad_serp::{Object, Serp, Shape};

use super::{
    BaseInput, BodyKind, ImportBody, ImportWrapper, Imported, ImportedComponent, ImportedFile,
    LengthUnit, file_name, invalid, unique_name,
};
use crate::document::{Document, ModelError, REPEATED_PARTS_TOLERANCE};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// Whether the file at `path` is a `.serp` file by its extension (any
/// case).
pub fn is_serp_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("serp"))
}

/// A `.serp` file read, or the error in words (its detail to the log).
fn read(path: &str) -> Result<Serp, ModelError> {
    mitcad_serp::read_file(Path::new(path)).map_err(|e| {
        eprintln!("import: .serp: {path}: {}", e.detail);
        invalid(format!("{}: {}", file_name(path), e.words))
    })
}

/// The length unit of a file's symbol.
fn length_unit(symbol: &str) -> Option<LengthUnit> {
    Some(match symbol {
        "mm" => LengthUnit::Millimeter,
        "cm" => LengthUnit::Centimeter,
        "m" => LengthUnit::Meter,
        "in" => LengthUnit::Inch,
        "ft" => LengthUnit::Foot,
        _ => return None,
    })
}

/// An object in words for a person: its name and layer, never its index
/// (`the object "Bracket" on the layer Parts`).
fn describe(serp: &Serp, object: &Object) -> String {
    let mut words = match &object.name {
        Some(name) => format!("the object \"{name}\""),
        None => "an object without a name".to_owned(),
    };
    if let Some(layer) = object.layer {
        words.push_str(&format!(" on the layer {}", serp.layer_path(layer)));
    }
    words
}

/// A shape made: the kernel's, in millimetres. None for a shape without
/// faces (a curve or a point); the error says why it cannot be made.
fn make<K: Kernel>(kernel: &K, shape: &Shape, mm: f64) -> Result<Option<K::Shape>, String> {
    let made = match shape {
        Shape::Brep(data) => {
            let (made, faces) = kernel
                .import_brep(FeatureUid(0), data, 0)
                .map_err(|e| e.to_string())?;
            if faces == 0
                || kernel
                    .body_kind(&made)
                    .is_ok_and(|kind| kind == BodyKind::Empty)
            {
                return Ok(None);
            }
            if mm == 1.0 {
                made
            } else {
                kernel
                    .transform_shape(&made, &Transform::scale([0.0; 3], [mm; 3]), None)
                    .map_err(|e| e.to_string())?
            }
        }
        Shape::Mesh(mesh) => {
            if mesh.triangles.is_empty() {
                return Ok(None);
            }
            let points: Vec<[f64; 3]> = mesh.vertices.iter().map(|p| p.map(|v| v * mm)).collect();
            kernel
                .triangle_body(&points, &mesh.triangles)
                .map_err(|e| e.to_string())?
        }
        Shape::Other(_) => return Ok(None),
        Shape::Unreadable(why) => return Err(why.clone()),
    };
    Ok(Some(made))
}

/// A body planned for the base feature of the design.
struct PlannedBody<S> {
    body: ImportBody<S>,
    /// Its layer's path.
    folder: Option<String>,
    hidden: bool,
}

/// A block planned as a component: its bodies and where each placed
/// block puts it.
struct PlannedBlock<S> {
    name: String,
    bodies: Vec<ImportBody<S>>,
    placements: Vec<Transform>,
}

/// A point cloud planned as a point cloud item (mitcad#2240): its name,
/// its points in millimetres and whether it is shown.
struct PlannedCloud {
    name: String,
    data: crate::CloudData,
    visible: bool,
}

/// What reading the file planned, before anything changes.
struct Plan<S> {
    top: Vec<PlannedBody<S>>,
    blocks: Vec<PlannedBlock<S>>,
    clouds: Vec<PlannedCloud>,
    warnings: Vec<String>,
}

/// The file's point clouds as point cloud items, in millimetres
/// (mitcad#2240): each point the origin plus the point, scaled.
fn plan_clouds(serp: &Serp) -> Vec<PlannedCloud> {
    let mm = serp.units.mm;
    serp.clouds
        .iter()
        .enumerate()
        .map(|(i, cloud)| PlannedCloud {
            name: cloud
                .name
                .clone()
                .unwrap_or_else(|| format!("Point cloud {}", i + 1)),
            data: crate::CloudData {
                points: cloud
                    .points
                    .iter()
                    .map(|p| std::array::from_fn(|k| (cloud.origin[k] + f64::from(p[k])) * mm))
                    .collect(),
                colors: cloud.colors.clone(),
                file_points: cloud.points.len(),
            },
            visible: serp.cloud_shown(cloud),
        })
        .collect()
}

/// Counts in words (`1 object`, `2 objects`).
fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The file's objects made into bodies and blocks (see the module).
fn plan<K: Kernel>(kernel: &K, serp: &Serp, source: &str) -> Plan<K::Shape> {
    let mm = serp.units.mm;
    let mut warnings: Vec<String> = Vec::new();
    for note in &serp.notes {
        eprintln!("import: .serp: {source}: {} ({})", note.words, note.detail);
        warnings.push(note.words.clone());
    }
    let mut top = Vec::new();
    let mut no_faces = 0usize;
    let mut invalid_bodies = 0usize;
    let mut unreadable = Vec::new();
    // The placed blocks made, by block: the object and its shape.
    let mut placed: BTreeMap<usize, Vec<(usize, K::Shape)>> = BTreeMap::new();
    let planned_body = |o: &Object, shape: K::Shape| PlannedBody {
        body: ImportBody {
            name: o.name.clone(),
            color: serp.color(o),
            shape,
        },
        folder: o.layer.map(|l| serp.layer_path(l)),
        hidden: !serp.shown(o),
    };
    for (i, o) in serp.objects.iter().enumerate() {
        let shape = match make(kernel, &o.shape, mm) {
            Ok(Some(shape)) => shape,
            Ok(None) => {
                if !matches!(o.shape, Shape::Other(_)) {
                    no_faces += 1;
                }
                continue;
            }
            Err(why) => {
                eprintln!("import: .serp: {source}: object {i}: {why}");
                unreadable.push(describe(serp, o));
                continue;
            }
        };
        if kernel.is_valid(&shape).is_ok_and(|valid| !valid) {
            eprintln!(
                "import: .serp: {source}: object {i}: the kernel's check finds the body not valid"
            );
            invalid_bodies += 1;
        }
        match o.block {
            // A hidden placed block comes in as its body, hidden.
            Some(b) if serp.shown(o) => placed.entry(b).or_default().push((i, shape)),
            _ => top.push(planned_body(o, shape)),
        }
    }

    // Each block's placements found by comparing its shapes with each
    // placed block's.
    let mut blocks = Vec::new();
    let mut unmatched = 0usize;
    for (b, list) in placed {
        let block = &serp.blocks[b];
        let mut shapes = Vec::new();
        let mut whole = true;
        for s in &block.shapes {
            match make(kernel, s, mm) {
                Ok(Some(shape)) => shapes.push(shape),
                Ok(None) => {}
                Err(why) => {
                    eprintln!("import: .serp: {source}: block {}: {why}", block.id);
                    whole = false;
                }
            }
        }
        // The writer places a block as a compound of its shapes.
        let copies = if whole && !shapes.is_empty() {
            kernel.compound(&shapes).ok().and_then(|definition| {
                let mut all: Vec<&K::Shape> = vec![&definition];
                all.extend(list.iter().map(|(_, s)| s));
                kernel.rigid_copies(&all, REPEATED_PARTS_TOLERANCE).ok()
            })
        } else {
            None
        };
        let mut placements = Vec::new();
        let mut color = None;
        for (k, (i, shape)) in list.into_iter().enumerate() {
            let o = &serp.objects[i];
            match copies.as_ref().and_then(|c| c.get(k + 1)) {
                Some(copy) if copy.of == 0 => {
                    placements.push(copy.transform);
                    color = color.or_else(|| serp.color(o));
                }
                _ => {
                    unmatched += 1;
                    top.push(planned_body(o, shape));
                }
            }
        }
        if placements.is_empty() {
            continue;
        }
        let several = shapes.len() > 1;
        let bodies = shapes
            .into_iter()
            .enumerate()
            .map(|(k, shape)| ImportBody {
                name: Some(if several {
                    format!("{} {}", block.name, k + 1)
                } else {
                    block.name.clone()
                }),
                color,
                shape,
            })
            .collect();
        blocks.push(PlannedBlock {
            name: block.name.clone(),
            bodies,
            placements,
        });
    }

    for what in unreadable {
        warnings.push(format!("{what} cannot be read: left out"));
    }
    if invalid_bodies > 0 {
        warnings.push(format!(
            "{} not valid as the file has {}: {} came in as {}",
            counted(invalid_bodies, "body is", "bodies are"),
            if invalid_bodies == 1 { "it" } else { "them" },
            if invalid_bodies == 1 { "it" } else { "they" },
            if invalid_bodies == 1 {
                "it is"
            } else {
                "they are"
            },
        ));
    }
    if unmatched > 0 {
        warnings.push(format!(
            "{} the shapes of {} block (scaled, mirrored or changed after placing): each came in as a body where the file has it",
            counted(unmatched, "placed block does not match", "placed blocks do not match"),
            if unmatched == 1 { "its" } else { "their" },
        ));
    }
    let used: BTreeSet<usize> = serp.objects.iter().filter_map(|o| o.block).collect();
    for (b, block) in serp.blocks.iter().enumerate() {
        if !used.contains(&b) {
            warnings.push(format!(
                "the block {} is placed nowhere and is left out",
                block.name
            ));
        }
    }
    let mut left_out = serp.skipped.parts();
    if no_faces > 0 {
        left_out.insert(0, counted(no_faces, "curve or point", "curves and points"));
    }
    if !left_out.is_empty() {
        warnings.push(format!("not imported: {}", left_out.join(", ")));
    }
    Plan {
        top,
        blocks,
        clouds: plan_clouds(serp),
        warnings,
    }
}

impl<K: Kernel> Document<K> {
    /// Imports a `.serp` file (see the module): with `wrap` into a new
    /// component named after the file. One undo step (`Import <file>`).
    pub(crate) fn import_serp(
        &mut self,
        path: &str,
        name: Option<&str>,
        wrap: bool,
    ) -> Result<(ImportedFile, Option<ImportWrapper>), ModelError> {
        let serp = read(path)?;
        let source = file_name(path);
        let plan = plan(self.kernel(), &serp, &source);
        if plan.top.is_empty() && plan.blocks.is_empty() && plan.clouds.is_empty() {
            let why = if plan.warnings.is_empty() {
                String::new()
            } else {
                format!(": {}", plan.warnings.join("; "))
            };
            return Err(invalid(format!(
                "{source} has no solids, surfaces, meshes or point clouds to import{why}"
            )));
        }
        let label = format!("Import {source}");
        let depth = self.undo_depth();
        let unit = length_unit(serp.units.symbol).filter(|_| !serp.units.assumed);
        match self.apply_serp(&source, name, wrap, unit, plan) {
            Ok(done) => {
                self.merge_undo(depth, &label);
                Ok(done)
            }
            Err(e) => {
                self.undo_back_to(depth);
                Err(e)
            }
        }
    }

    /// Makes what the plan says: the design's unit (a design without
    /// features), the wrapper, the blocks' components and occurrences, a
    /// base feature per block and one for the design's bodies, then the
    /// bodies' folders and hidden bodies.
    fn apply_serp(
        &mut self,
        source: &str,
        name: Option<&str>,
        wrap: bool,
        unit: Option<LengthUnit>,
        plan: Plan<K::Shape>,
    ) -> Result<(ImportedFile, Option<ImportWrapper>), ModelError> {
        let Plan {
            top,
            blocks,
            clouds,
            warnings,
        } = plan;
        if let Some(unit) = unit
            && self.features().next().is_none()
        {
            self.set_units(match unit {
                LengthUnit::Millimeter => crate::expr::LengthUnit::Millimetre,
                LengthUnit::Centimeter => crate::expr::LengthUnit::Centimetre,
                LengthUnit::Meter => crate::expr::LengthUnit::Metre,
                LengthUnit::Inch => crate::expr::LengthUnit::Inch,
                LengthUnit::Foot => crate::expr::LengthUnit::Foot,
            })?;
        }
        let wrapper = if wrap {
            Some(self.add_import_wrapper(source)?)
        } else {
            None
        };
        let parent = wrapper
            .as_ref()
            .map_or_else(|| self.active_component(), |w| w.component);

        let mut taken: BTreeSet<String> = self.features().map(|f| f.name.clone()).collect();
        let mut components = Vec::new();
        let mut inputs = Vec::new();
        for block in blocks {
            let mut placements = block.placements.into_iter();
            let first = placements.next().unwrap_or(Transform::IDENTITY);
            let (component, occurrence) =
                self.add_component(Some(block.name.as_str()), parent, first)?;
            let mut occurrences = vec![occurrence];
            for placement in placements {
                occurrences.push(self.add_occurrence(component, parent, placement)?);
            }
            components.push(ImportedComponent {
                component,
                name: self.assembly().name(component),
                occurrences,
                feature: None,
            });
            inputs.push(BaseInput {
                name: Some(unique_name(&block.name, &mut taken)),
                source: Some(format!("{source}, block {}", block.name)),
                component: Some(component),
                ..BaseInput::new(block.bodies)
            });
        }
        let mut folders = Vec::new();
        let mut hidden = Vec::new();
        let has_top = !top.is_empty();
        if has_top {
            let layers: BTreeSet<&Option<String>> = top.iter().map(|b| &b.folder).collect();
            let foldered = layers.len() > 1;
            let mut bodies = Vec::with_capacity(top.len());
            for planned in top {
                folders.push(if foldered { planned.folder } else { None });
                hidden.push(planned.hidden);
                bodies.push(planned.body);
            }
            inputs.push(BaseInput {
                name: name.map(|n| unique_name(n, &mut taken)),
                source: Some(source.to_owned()),
                component: Some(parent),
                ..BaseInput::new(bodies)
            });
        }
        let label = format!("Import {source}");
        // A file of point clouds only makes no base feature.
        let mut added = if inputs.is_empty() {
            Vec::new()
        } else {
            self.add_base_features(&label, inputs)?
        };
        let loose = if has_top { added.pop() } else { None };
        for (component, imported) in components.iter_mut().zip(added) {
            component.feature = Some(imported);
        }

        // The design's bodies: folders by layer, hidden ones hidden.
        if let Some(imported) = &loose {
            let uid = |i: usize| BodyUid::new(imported.feature.uid, i as u32);
            let organised: Vec<(BodyUid, Option<String>, Option<String>)> = folders
                .into_iter()
                .enumerate()
                .filter_map(|(i, folder)| folder.map(|f| (uid(i), None, Some(f))))
                .collect();
            if !organised.is_empty() {
                self.name_bodies(&organised)?;
            }
            for (i, hide) in hidden.into_iter().enumerate() {
                if hide {
                    self.set_body_visible(uid(i), false)?;
                }
            }
        }

        // The point clouds (mitcad#2240) as point cloud items in the
        // component the bodies went into, each under a free name.
        let cloud_occurrence = self.component_cloud_occurrence(parent);
        for cloud in clouds {
            let name = self.free_point_cloud_name(&cloud.name);
            self.add_point_cloud(
                Some(&name),
                source,
                cloud.data,
                cloud_occurrence.clone(),
                Transform::IDENTITY,
                cloud.visible,
            )?;
        }

        let file = if components.is_empty() && loose.is_some() {
            let mut imported: Imported =
                loose.ok_or_else(|| invalid("the import made no bodies"))?;
            imported.warnings = warnings;
            ImportedFile::Base(imported)
        } else {
            ImportedFile::Components {
                components,
                warnings,
                loose,
            }
        };
        Ok((file, wrapper))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serp_files_are_known_by_their_extension() {
        assert!(is_serp_path("/models/vase.serp"));
        assert!(is_serp_path("C:\\models\\vase.SERP"));
        assert!(!is_serp_path("/models/vase.3dm"));
        assert!(!is_serp_path("/models/serp"));
    }

    #[test]
    fn units_of_the_file_are_lengths() {
        assert_eq!(length_unit("cm"), Some(LengthUnit::Centimeter));
        assert_eq!(length_unit("ft"), Some(LengthUnit::Foot));
        assert_eq!(length_unit("yd"), None);
    }

    #[test]
    fn counts_are_said_in_words() {
        assert_eq!(counted(1, "curve", "curves"), "1 curve");
        assert_eq!(counted(3, "curve", "curves"), "3 curves");
    }
}
