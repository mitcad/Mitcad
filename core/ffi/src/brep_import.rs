// SPDX-License-Identifier: MIT
//! C++ bridge for importing the bodies of .f3d files: Rust reads the file and
//! converts each body to the neutral B-rep model; C++ builds it with OCCT
//! (`geometry/include/mitcad/geometry/brep_import.hpp`, same flat layout as
//! `mitcad_f3d::flat`). The display meshes saved with a document are
//! offered too, for checking the built bodies.

use mitcad_f3d::F3dFile;
use mitcad_f3d::asm::AsmFile;
use mitcad_f3d::asm::history::History;
use mitcad_f3d::convert::{self, ConvertedBody, Options};
use mitcad_f3d::flat::FlatBody;

#[cxx::bridge(namespace = "mitcad::f3d")]
pub(crate) mod ffi {
    #[derive(Clone, Copy, Debug)]
    struct BrepGeometryData {
        kind: i32,
        int_offset: u32,
        int_count: u32,
        real_offset: u32,
        real_count: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct BrepEdgeData {
        curve: u32,
        v0: u32,
        v1: u32,
        t0: f64,
        t1: f64,
        tolerance: f64,
    }

    #[derive(Clone, Copy, Debug)]
    struct BrepFaceData {
        surface: u32,
        reversed: bool,
        double_sided: bool,
        first_loop: u32,
        loop_count: u32,
        first_point_loop: u32,
        point_loop_count: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct BrepLoopData {
        first_coedge: u32,
        coedge_count: u32,
    }

    #[derive(Clone, Copy, Debug)]
    struct BrepCoedgeData {
        edge: u32,
        forward: bool,
    }

    #[derive(Clone, Copy, Debug)]
    struct BrepShellData {
        lump: u32,
        first_face: u32,
        face_count: u32,
        closed: bool,
    }

    /// One body of a body blob as the neutral B-rep model in flat arrays.
    struct BrepBodyData {
        /// Document of an .f3z package; empty for an .f3d.
        document: String,
        /// Zip entry of the body blob.
        blob: String,
        /// Record index of the body in the blob.
        record: u32,
        /// The blob is an .smbh (carries ASM history).
        history: bool,
        /// The body is saved at the top level of the blob (otherwise it
        /// owns a face or edge saved there).
        top_level: bool,
        /// Number of ASM history states rolled back (0: the saved state).
        history_step: u32,
        asm_version: String,
        /// Geometry or topology the converter could not read.
        issues: Vec<String>,
        skipped_faces: u32,
        ints: Vec<i32>,
        reals: Vec<f64>,
        curves: Vec<BrepGeometryData>,
        surfaces: Vec<BrepGeometryData>,
        vertices: Vec<f64>,
        edges: Vec<BrepEdgeData>,
        faces: Vec<BrepFaceData>,
        loops: Vec<BrepLoopData>,
        coedges: Vec<BrepCoedgeData>,
        shells: Vec<BrepShellData>,
        shell_faces: Vec<u32>,
        /// Vertices of the faces' point loops (cone apexes).
        point_loops: Vec<u32>,
        lump_count: u32,
        transform: Vec<f64>,
    }

    /// A body of the display scene saved with a document (the writer's own
    /// tessellation, see `core/f3d/OGS_FORMAT.md`), measured in mm.
    struct DisplayMeshData {
        /// Document of an .f3z package; empty for an .f3d.
        document: String,
        /// Position of the body in the scene.
        index: u32,
        faces: u32,
        /// Faces without a mesh (all of them for a hidden body).
        faces_without_mesh: u32,
        triangles: u32,
        /// Every edge is shared by two triangles (after welding).
        closed: bool,
        volume: f64,
        area: f64,
        /// xmin ymin zmin xmax ymax zmax.
        bbox: [f64; 6],
    }

    extern "Rust" {
        /// Every body of every body blob of an .f3d or .f3z file.
        fn f3d_read_bodies(path: &str) -> Result<Vec<BrepBodyData>>;
        /// The top-level bodies of the .smbh blobs as they were before the
        /// operations of their ASM history: rolled back one state, two
        /// states, ... while the body exists (`history_step` 1, 2, ...),
        /// leaving out states that did not change the body.
        fn f3d_read_history_bodies(path: &str) -> Result<Vec<BrepBodyData>>;
        /// The bodies of the display scenes of an .f3d or .f3z file (none
        /// when it has no scene).
        fn f3d_read_display_meshes(path: &str) -> Result<Vec<DisplayMeshData>>;
        /// Bodies written by Mitcad's own ASM writer (a 10 mm cube, a
        /// cylinder of radius 10 mm and height 20 mm, and a cone of the
        /// same size with its apex up), for tests.
        fn f3d_test_bodies() -> Vec<BrepBodyData>;
    }
}

/// Where a body comes from.
pub(crate) struct Source<'a> {
    pub document: &'a str,
    pub blob: &'a str,
    pub asm_version: &'a str,
    pub history: bool,
    pub history_step: u32,
}

pub(crate) fn to_ffi(source: &Source, body: &ConvertedBody) -> ffi::BrepBodyData {
    let f = FlatBody::from_body(&body.body);
    let geometry = |g: &mitcad_f3d::flat::FlatGeometry| ffi::BrepGeometryData {
        kind: g.kind,
        int_offset: g.int_offset,
        int_count: g.int_count,
        real_offset: g.real_offset,
        real_count: g.real_count,
    };
    ffi::BrepBodyData {
        document: source.document.to_string(),
        blob: source.blob.to_string(),
        record: u32::try_from(body.record).unwrap_or(u32::MAX),
        history: source.history,
        top_level: body.top_level,
        history_step: source.history_step,
        asm_version: source.asm_version.to_string(),
        issues: body.issues.clone(),
        skipped_faces: u32::try_from(body.skipped_faces).unwrap_or(u32::MAX),
        curves: f.curves.iter().map(geometry).collect(),
        surfaces: f.surfaces.iter().map(geometry).collect(),
        edges: f
            .edges
            .iter()
            .map(|e| ffi::BrepEdgeData {
                curve: e.curve,
                v0: e.v0,
                v1: e.v1,
                t0: e.t0,
                t1: e.t1,
                tolerance: e.tolerance,
            })
            .collect(),
        faces: f
            .faces
            .iter()
            .map(|x| ffi::BrepFaceData {
                surface: x.surface,
                reversed: x.reversed,
                double_sided: x.double_sided,
                first_loop: x.first_loop,
                loop_count: x.loop_count,
                first_point_loop: x.first_point_loop,
                point_loop_count: x.point_loop_count,
            })
            .collect(),
        loops: f
            .loops
            .iter()
            .map(|l| ffi::BrepLoopData {
                first_coedge: l.first_coedge,
                coedge_count: l.coedge_count,
            })
            .collect(),
        coedges: f
            .coedges
            .iter()
            .map(|c| ffi::BrepCoedgeData {
                edge: c.edge,
                forward: c.forward,
            })
            .collect(),
        shells: f
            .shells
            .iter()
            .map(|s| ffi::BrepShellData {
                lump: s.lump,
                first_face: s.first_face,
                face_count: s.face_count,
                closed: s.closed,
            })
            .collect(),
        ints: f.ints,
        reals: f.reals,
        vertices: f.vertices,
        shell_faces: f.shell_faces,
        point_loops: f.point_loops,
        lump_count: f.lump_count,
        transform: f.transform,
    }
}

fn document_bodies(
    name: &str,
    doc: &F3dFile,
    out: &mut Vec<ffi::BrepBodyData>,
) -> Result<(), String> {
    let options = Options::default();
    for blob in doc.body_blobs() {
        let data = doc
            .read(&blob.entry)
            .map_err(|e| format!("{}: {e}", blob.entry))?;
        let bodies = mitcad_f3d::read_blob(&blob.entry, &data, &options)
            .map_err(|e| format!("{}: {e}", blob.entry))?;
        let source = Source {
            document: name,
            blob: &blob.entry,
            asm_version: &bodies.header.asm_version,
            history: blob.history,
            history_step: 0,
        };
        for b in &bodies.bodies {
            out.push(to_ffi(&source, b));
        }
    }
    Ok(())
}

fn document_history_bodies(
    name: &str,
    doc: &F3dFile,
    out: &mut Vec<ffi::BrepBodyData>,
) -> Result<(), String> {
    let options = Options::default();
    for blob in doc.body_blobs().into_iter().filter(|b| b.history) {
        let data = doc
            .read(&blob.entry)
            .map_err(|e| format!("{}: {e}", blob.entry))?;
        let file = AsmFile::parse(&data).map_err(|e| format!("{}: {e}", blob.entry))?;
        let Ok(Some(history)) = History::parse(&file) else {
            continue;
        };
        for (record, top) in convert::body_records(&file) {
            if !top {
                continue;
            }
            // States that leave the body as it was are skipped.
            let mut last = convert::convert_body(&file, record, &options).body;
            for step in 1..=history.states.len() {
                let view = history.view(step);
                // Stop where the body did not exist yet.
                if view.get(&record) == Some(&None) {
                    break;
                }
                let body = convert::convert_body_at(&file, record, &options, Some(&view));
                if body.body == last {
                    continue;
                }
                last = body.body.clone();
                let source = Source {
                    document: name,
                    blob: &blob.entry,
                    asm_version: &file.header.asm_version,
                    history: true,
                    history_step: u32::try_from(step).unwrap_or(u32::MAX),
                };
                out.push(to_ffi(&source, &body));
            }
        }
    }
    Ok(())
}

/// Calls `read` for the file, or for each document of an .f3z package.
fn read_documents(
    path: &str,
    read: fn(&str, &F3dFile, &mut Vec<ffi::BrepBodyData>) -> Result<(), String>,
) -> Result<Vec<ffi::BrepBodyData>, String> {
    let file = F3dFile::open(std::path::Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
    let mut out = Vec::new();
    let nested = file.documents();
    if nested.is_empty() {
        read("", &file, &mut out)?;
    } else {
        for name in nested {
            let doc = file
                .open_document(&name)
                .map_err(|e| format!("{name}: {e}"))?;
            read(&name, &doc, &mut out)?;
        }
    }
    Ok(out)
}

fn f3d_read_bodies(path: &str) -> Result<Vec<ffi::BrepBodyData>, String> {
    read_documents(path, document_bodies)
}

fn f3d_read_history_bodies(path: &str) -> Result<Vec<ffi::BrepBodyData>, String> {
    read_documents(path, document_history_bodies)
}

fn f3d_read_display_meshes(path: &str) -> Result<Vec<ffi::DisplayMeshData>, String> {
    let file = F3dFile::open(std::path::Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
    let mut docs = Vec::new();
    let nested = file.documents();
    for name in &nested {
        docs.push((
            name.clone(),
            file.open_document(name)
                .map_err(|e| format!("{name}: {e}"))?,
        ));
    }
    if nested.is_empty() {
        docs.push((String::new(), file));
    }
    let mut out = Vec::new();
    for (name, doc) in &docs {
        let Some(scene) =
            mitcad_f3d::ogs::display_scene(doc).map_err(|e| format!("{path}: {e}"))?
        else {
            continue;
        };
        for (index, body) in scene.bodies.iter().enumerate() {
            let stats = body.stats();
            out.push(ffi::DisplayMeshData {
                document: name.clone(),
                index: u32::try_from(index).unwrap_or(u32::MAX),
                faces: u32::try_from(body.faces.len()).unwrap_or(u32::MAX),
                faces_without_mesh: u32::try_from(body.faces_without_mesh()).unwrap_or(u32::MAX),
                triangles: u32::try_from(stats.triangles).unwrap_or(u32::MAX),
                closed: stats.closed,
                volume: stats.volume_mm3,
                area: stats.area_mm2,
                bbox: stats.bbox_mm,
            });
        }
    }
    Ok(out)
}

fn f3d_test_bodies() -> Vec<ffi::BrepBodyData> {
    let options = Options::default();
    let mut out = Vec::new();
    for (name, data) in [
        ("cube.smb", mitcad_f3d::testdata::cube_blob()),
        ("cylinder.smb", mitcad_f3d::testdata::cylinder_blob()),
        ("cone.smb", mitcad_f3d::testdata::cone_blob()),
    ] {
        let bodies = mitcad_f3d::read_blob(name, &data, &options).expect("test blobs parse");
        let source = Source {
            document: "",
            blob: name,
            asm_version: &bodies.header.asm_version,
            history: false,
            history_step: 0,
        };
        for b in &bodies.bodies {
            out.push(to_ffi(&source, b));
        }
    }
    out
}
