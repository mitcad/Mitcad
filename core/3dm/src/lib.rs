// SPDX-License-Identifier: MIT
//! Reader for `.3dm` models (mitcad#1715): the archive format of versions
//! 5 to 8 (archive versions 50 to 80; versions 2 to 4 where their objects
//! have the same form). Written anew in Rust from the format's facts, as
//! the format's published reference source describes them; no code of
//! that source or of another reader (README.md, "Provenance").
//!
//! - [`archive`]: the chunks, their CRCs and the primitive values.
//! - [`model`]: the file ([`File3dm`]): units and tolerance, layers, block
//!   definitions, objects with their attributes.
//! - [`geom`]: the geometry classes (B-reps, extrusions, NURBS and other
//!   curves and surfaces, meshes, block instances).
//! - [`convert`]: B-reps, extrusions and surfaces as neutral bodies
//!   ([`mitcad_asm::brep::Body`]) in millimetres, which OCCT builds.
//! - [`testfile`]: a writer of small archives for the tests.
//!
//! The import (`core/ffi/src/import_3dm.rs`, `import_3dm`) builds the
//! bodies, meshes and block instances.

pub mod archive;
pub mod convert;
pub mod geom;
pub mod model;
pub mod nurbs;
pub mod testfile;

#[cfg(test)]
mod tests;

pub use archive::{Error, Uuid};
pub use model::File3dm;

use serde_json::{Value, json};
use std::collections::BTreeMap;

/// What an object is, for the report (`brep`, `extrusion`, `mesh`, …).
pub fn kind_name(g: &geom::Geometry) -> String {
    match g {
        geom::Geometry::Brep(_) => "B-rep".to_owned(),
        geom::Geometry::Extrusion(_) => "extrusion".to_owned(),
        geom::Geometry::Surface(_) => "surface".to_owned(),
        geom::Geometry::Mesh(_) => "mesh".to_owned(),
        geom::Geometry::Curve(_) => "curve".to_owned(),
        geom::Geometry::Instance(_) => "block instance".to_owned(),
        geom::Geometry::Other(name) => name.clone(),
    }
}

/// The file's summary as JSON (`mitcad-3dm-inspect`).
pub fn summary(file: &File3dm) -> Value {
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for o in &file.objects {
        *kinds.entry(kind_name(&o.geometry)).or_insert(0) += 1;
    }
    let breps: Vec<Value> = file
        .objects
        .iter()
        .filter_map(|o| match &o.geometry {
            geom::Geometry::Brep(b) => Some(json!({
                "layer": file.layer_path(o.attributes.layer),
                "faces": b.faces.len(), "edges": b.edges.len(), "vertices": b.vertices.len(),
                "curves": b.curves.len(), "surfaces": b.surfaces.len(), "is_solid": b.is_solid,
                "render_meshes": b.render_meshes.iter().flatten().count(),
            })),
            _ => None,
        })
        .collect();
    json!({
        "version": file.version,
        "writer_version": file.writer,
        "application": file.application,
        "units": file.units.name,
        "mm_per_unit": file.units.mm,
        "tolerance": file.tolerance,
        "layers": file.layers.iter().map(|l| json!({
            "index": l.index, "name": file.layer_path(l.index), "visible": l.visible,
            "color": format!("{:08X}", l.color),
        })).collect::<Vec<_>>(),
        "definitions": file.definitions.iter().map(|d| json!({
            "name": d.name, "objects": d.objects.len(),
        })).collect::<Vec<_>>(),
        "objects": kinds,
        "breps": breps,
        "failed": file.failed.iter().map(|f| json!({
            "index": f.index, "kind": f.kind, "error": f.error.0, "detail": f.error.detail(),
        })).collect::<Vec<_>>(),
        "warnings": file.warnings.iter().map(|w| json!({
            "warning": w.0, "detail": w.detail(),
        })).collect::<Vec<_>>(),
    })
}
