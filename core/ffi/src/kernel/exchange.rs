// SPDX-License-Identifier: MIT
//! Imported bodies and data exchange: the B-rep data of base features
//! (`geometry/include/mitcad/geometry/import.hpp`), reading and writing
//! files (`geometry/io`) and building the bodies of .f3d files
//! (`geometry/include/mitcad/geometry/brep_import.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// What a body is made of (`mitcad::geometry::BodyKind`).
    enum BodyKind {
        Empty,
        Solid,
        Sheet,
        Mesh,
    }

    /// Name and colour of a body read from or written to a file; its shape
    /// is at the same index of the shape list.
    struct BodyInfo {
        name: String,
        has_color: bool,
        /// sRGB components in [0, 1].
        color: [f64; 3],
        /// Writing: where the body goes (`mitcad::io::Placement`), once as
        /// it is when empty. Reading leaves it empty.
        placements: Vec<BodyPlacement>,
    }

    /// A placement of a body to write (mitcad#19): the rotation's rows and
    /// the translation (a rotation and a translation only), and the name of
    /// what places it (an occurrence path; may be empty).
    struct BodyPlacement {
        rotation: [f64; 9],
        translation: [f64; 3],
        name: String,
    }

    enum ExportFormat {
        Step,
        Iges,
        Stl,
        Obj,
        Brep,
    }

    enum LengthUnit {
        Millimeter,
        Centimeter,
        Meter,
        Inch,
        Foot,
    }

    struct ExportSettings {
        format: ExportFormat,
        /// STEP AP242 instead of AP214.
        ap242: bool,
        /// STEP and IGES.
        unit: LengthUnit,
        /// STL and OBJ triangulation: surface deviation (mm) and the angle
        /// between neighbouring facet normals (radians).
        deviation: f64,
        angle: f64,
        /// At most about this many triangles a doubly curved face (0: no
        /// limit; mitcad#398).
        face_triangles: u64,
        /// STL as text.
        ascii: bool,
    }

    /// A body of an .f3d file and how building it went. Its shape (null
    /// when not built) is at the same index of the shape list.
    struct F3dBody {
        /// Document of an .f3z package; empty for an .f3d.
        document: String,
        /// Zip entry of the body blob and the body's record in it.
        blob: String,
        record: u32,
        /// The blob is an .smbh (bodies with ASM history).
        history: bool,
        /// Saved at the top level of the blob (not only the owner of
        /// faces saved there).
        top_level: bool,
        built: bool,
        /// Every lump became a solid.
        solid: bool,
        /// OCCT's checker accepts the shape.
        valid: bool,
        volume: f64,
        area: f64,
        faces: i32,
        /// Converter issues and geometry the builder rejected.
        issues: u32,
        /// Why it was not built.
        error: String,
        /// The builder's messages: geometry it rejected, and what OCCT's
        /// checker finds wrong.
        messages: Vec<String>,
        /// The volume before the solids were oriented (negative when the
        /// faces have the material on their outer side).
        raw_volume: f64,
    }

    unsafe extern "C++" {
        include!("bridge/exchange.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// Reads B-rep data; face i is named `<feature>:import(<first_face + i>)`.
        fn import_brep(feature: &str, data: &[u8], first_face: u32) -> Result<SharedPtr<Shape>>;
        fn face_count(shape: &Shape) -> usize;
        /// OCCT binary B-rep of the shape.
        fn brep_data(shape: &Shape) -> Result<Vec<u8>>;
        /// What `import_brep` reads of the shape's B-rep data, made of the
        /// shape itself (mitcad#244).
        fn adopt_body(feature: &str, shape: &Shape, first_face: u32) -> Result<SharedPtr<Shape>>;
        /// A compound of the shapes, with their face names.
        fn compound(shapes: &ShapeList) -> Result<SharedPtr<Shape>>;
        fn body_kind(shape: &Shape) -> BodyKind;

        /// Reads a STEP, IGES, BRep, STL or OBJ file (by extension) into
        /// the list; `unit_mm` scales STL and OBJ.
        fn read_file(
            path: &str,
            unit_mm: f64,
            shapes: Pin<&mut ShapeList>,
        ) -> Result<Vec<BodyInfo>>;
        fn write_file(
            path: &str,
            shapes: &ShapeList,
            bodies: &[BodyInfo],
            settings: &ExportSettings,
        ) -> Result<()>;

        /// Builds the bodies of an .f3d or .f3z file with OCCT: the
        /// top-level bodies of the `.smb` blobs, with `history` also of the
        /// `.smbh` blobs and with `owners` also the bodies that only own
        /// faces saved at the top level.
        fn f3d_bodies(
            path: &str,
            history: bool,
            owners: bool,
            shapes: Pin<&mut ShapeList>,
        ) -> Result<Vec<F3dBody>>;

        // .f3d import (T1).
        #[namespace = "mitcad::f3d"]
        type BrepBodyData = crate::brep_import::ffi::BrepBodyData;
        /// Builds one body of an .f3d file from its neutral B-rep data
        /// (`brep_import.rs`); null when it cannot be built, an error when
        /// an allocation failed meanwhile (the process is out of memory,
        /// mitcad#132).
        fn f3d_build_body(data: &BrepBodyData) -> Result<SharedPtr<Shape>>;
        /// A body `f3d_build_body` built, from its B-rep data (`brep_data`):
        /// the next try of an import takes it from one the watchdog gave up
        /// while it built it (mitcad#82); null when it cannot be read.
        fn f3d_read_body(data: &[u8]) -> SharedPtr<Shape>;
        /// Crashes inside OCCT as errors from now on
        /// (`mitcad/geometry/guard.hpp`).
        fn catch_occt_crashes();

        // .f3d import: healed faces reused between history states
        // (mitcad#181).
        /// A body built by `f3d_build_body_from`, with what the builds of
        /// its other states can take from it.
        type F3dBuilt;
        /// The builds a body's build takes healed faces from.
        type F3dBases;
        fn f3d_bases() -> UniquePtr<F3dBases>;
        /// Adds a build to the bases (one that kept nothing adds nothing).
        fn f3d_bases_add(bases: Pin<&mut F3dBases>, built: &SharedPtr<F3dBuilt>);
        /// `f3d_build_body`, taking the faces whose data is the same as
        /// one of the bases' as they were healed there (the base that
        /// shares the most), and healing only the others; the body as a
        /// whole when less than half are shared or that is not valid.
        /// `keep`: keep what the builds of other states can take from it
        /// (the shape is the same either way).
        fn f3d_build_body_from(
            data: &BrepBodyData,
            bases: &F3dBases,
            keep: bool,
        ) -> Result<SharedPtr<F3dBuilt>>;
        /// The body built (null when it does not build).
        fn f3d_built_shape(built: &F3dBuilt) -> SharedPtr<Shape>;
        /// How many of its faces were taken from a base.
        fn f3d_built_reused(built: &F3dBuilt) -> u32;
        /// Whether it keeps healed faces for the builds of other states.
        fn f3d_built_kept(built: &F3dBuilt) -> bool;
        /// Why it was healed as a whole although it had bases (empty when
        /// it was not).
        fn f3d_built_note(built: &F3dBuilt) -> String;

        // .ipt import (mitcad#60).
        /// Builds one body from its neutral B-rep data with OCCT and
        /// reports how it went, as `f3d_bodies` does for each body; its
        /// shape (null when not built) is appended to the list.
        fn build_brep_body(data: &BrepBodyData, shapes: Pin<&mut ShapeList>) -> F3dBody;
        /// Whether OCCT's checker finds the shape valid (the bodies of the
        /// `.ipt` import with its history, mitcad#60); an error when an
        /// allocation failed in the checker (mitcad#132).
        fn shape_is_valid(shape: &Shape) -> Result<bool>;
    }

    extern "Rust" {
        /// Writes a small .f3d file for tests: Mitcad's own ASM test bodies
        /// (a 10 mm cube and a cylinder of radius 10 mm and height 20 mm)
        /// in a `.smb` blob each, without the rest of an .f3d design.
        fn f3d_write_test_file(path: &str) -> Result<()>;
        /// Writes a test assembly of components of other documents with its
        /// reference folder (`mitcad_f3d::testreferences`, mitcad#196) into
        /// `dir`: `Assembly v1.f3d` and `Assembly v1.refs/` with
        /// `Holder v1.f3d`, `Part v2.f3d` and `references.json`; or the
        /// two files in `<folder>/` without `references.json`.
        fn f3d_write_test_references(dir: &str, folder: &str) -> Result<()>;
        /// Writes the test assembly as `.f3z` packages into `dir`
        /// (`mitcad_f3d::testreferences::package_file`, mitcad#229):
        /// `Package.f3z` with its `Manifest.json` and
        /// `DesignDescription.json`, and `Package-plain.f3z` without them.
        fn f3d_write_test_package(dir: &str) -> Result<()>;
        /// Writes a design whose rounding deviates from the file's
        /// (mitcad#379) into `dir`: `Rounding.f3d`, whose ASM history makes
        /// a 10 mm cube and then bevels one edge by 2 mm
        /// (`mitcad_asm::testdata::bevelled_cube_history_blob`), and
        /// `Rounding.dump.json`, its timeline as a dump (a sketch, the
        /// extrusion and a rounding of that edge of radius 2 mm): replayed
        /// with it (`import_f3d`'s `dump`), the extrusion gives the file's
        /// cube and the rounding removes less than the file's bevel.
        fn f3d_write_test_deviation(dir: &str) -> Result<()>;

        // .ipt import (mitcad#60).
        /// Writes a small .ipt part file for tests (`mitcad_ipt::testdata`):
        /// the cube and the cylinder above, each in a B-rep record of its
        /// own, part number `MITCAD-TEST-1`, material `Aluminum`, inches.
        fn ipt_write_test_file(path: &str) -> Result<()>;
        /// Writes a small .ipt part file with a design for tests
        /// (`mitcad_ipt::testdata::test_part_with_design`): the 10 mm cube
        /// and the parameter, sketch and extrusion that make it, mm.
        fn ipt_write_test_design_file(path: &str) -> Result<()>;
        /// The designed cube kept hidden by its result-list entry.
        fn ipt_write_test_hidden_design_file(path: &str) -> Result<()>;
        /// Writes .ipt part files without B-rep bodies for tests: with a
        /// mesh feature's tetrahedron (`mitcad_ipt::testdata::test_mesh_part`),
        /// or empty (`test_empty_part`).
        fn ipt_write_test_mesh_file(path: &str) -> Result<()>;
        fn ipt_write_test_empty_file(path: &str) -> Result<()>;
        /// Writes an .ipt part file for tests whose result list keeps a
        /// body hidden (`mitcad_ipt::testdata::test_part_with_hidden_body`):
        /// the cube shown, the cylinder hidden, mm.
        fn ipt_write_test_hidden_file(path: &str) -> Result<()>;
        /// Writes an .ipt part file with appearances and a design for tests
        /// (`mitcad_ipt::testdata::test_part_with_designed_appearances`,
        /// mitcad#1375): the 10 mm cube in the default appearance, its top
        /// and its face x = 10 mm in another, and the design that makes it.
        fn ipt_write_test_appearance_file(path: &str) -> Result<()>;
        /// Writes a small project with .iam assemblies into a folder for
        /// tests (`mitcad_ipt::testassembly::write_test_project`): a top
        /// assembly as saved elsewhere, a sub-assembly and parts.
        fn ipt_write_test_assembly(dir: &str) -> Result<()>;

        // .f3d import (mitcad#82).
        /// `f3d_build_body` advanced (its healing, face by face): progress
        /// of the import running on this thread, for its watchdog.
        fn f3d_build_progress();
    }
}

// A build is not changed once made; the builds of other states read its
// shapes (copying the faces they take) as the replay reads its body (see
// `ffi::Shape`'s).
unsafe impl Send for ffi::F3dBuilt {}
unsafe impl Sync for ffi::F3dBuilt {}

/// See [`ffi::f3d_build_progress`].
fn f3d_build_progress() {
    crate::f3d_import::build_progress();
}

/// The blobs of [`ffi::f3d_write_test_file`] in a zip of stored entries.
fn f3d_write_test_file(path: &str) -> Result<(), String> {
    use mitcad_asm::testdata::{cube_blob, cylinder_blob};
    use mitcad_zip::{Method, Writer};

    let blob = |name: &str| format!("FusionAssetName[Active]/Breps.BlobParts/BREP.{name}.smb");
    let mut writer = Writer::new();
    for (name, data) in [
        (blob("mitcad-test-cube"), cube_blob()),
        (blob("mitcad-test-cylinder"), cylinder_blob()),
    ] {
        writer
            .add(&name, &data, Method::Stored)
            .map_err(|e| e.to_string())?;
    }
    let out = writer.finish().map_err(|e| e.to_string())?;
    std::fs::write(path, out).map_err(|e| format!("{path}: {e}"))
}

/// The files of [`ffi::f3d_write_test_references`].
fn f3d_write_test_references(dir: &str, folder: &str) -> Result<(), String> {
    let dir = std::path::Path::new(dir);
    for (path, data) in mitcad_f3d::testreferences::files() {
        if !folder.is_empty() && path.ends_with("references.json") {
            continue;
        }
        let path = match path.split_once('/') {
            Some((_, file)) if !folder.is_empty() => format!("{folder}/{file}"),
            _ => path,
        };
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// The packages of [`ffi::f3d_write_test_package`].
fn f3d_write_test_package(dir: &str) -> Result<(), String> {
    let dir = std::path::Path::new(dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, described) in [("Package.f3z", true), ("Package-plain.f3z", false)] {
        let path = dir.join(name);
        std::fs::write(&path, mitcad_f3d::testreferences::package_file(described))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// The files of [`ffi::f3d_write_test_deviation`].
fn f3d_write_test_deviation(dir: &str) -> Result<(), String> {
    use mitcad_zip::{Method, Writer};
    use serde_json::json;

    let dir = std::path::Path::new(dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut writer = Writer::new();
    writer
        .add(
            "FusionAssetName[Active]/Breps.BlobParts/BREP.mitcad-test-rounding.smbh",
            &mitcad_asm::testdata::bevelled_cube_history_blob(0.2),
            Method::Stored,
        )
        .map_err(|e| e.to_string())?;
    let path = dir.join("Rounding.f3d");
    std::fs::write(&path, writer.finish().map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // The timeline in the dump's terms (core/import/SCHEMA.md; cm): its
    // items name the history's operations by their result numbers.
    let param = |name: &str, expression: &str, value: f64, unit: &str| {
        json!({"kind": "parameter", "name": name, "expression": expression, "value": value,
               "unit": unit})
    };
    let point = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let line = |id: &str, a: &str, b: &str| json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b});
    let state = |result: i32| json!({"result_no": result, "flags": "000000"});
    let dump = json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "Rounding.f3d"},
        "parameters": {"model": [
            {"name": "d1", "expression": "10 mm", "value": 1.0, "unit": "mm"},
            {"name": "d2", "expression": "0.0 deg", "value": 0.0, "unit": "deg"},
            {"name": "d3", "expression": "2 mm", "value": 0.2, "unit": "mm"}
        ]},
        "timeline": {"items": [
            {"index": 0, "name": "Sketch1", "objectType": "Sketch", "detail": {
                "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
                "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                    [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
                "points": [point("p0", 0.0, 0.0), point("p1", 1.0, 0.0), point("p2", 1.0, 1.0),
                           point("p3", 0.0, 1.0)],
                "curves": [line("c0", "p0", "p1"), line("c1", "p1", "p2"),
                           line("c2", "p2", "p3"), line("c3", "p3", "p0")]
            }},
            {"index": 1, "name": "Extrude1", "objectType": "ExtrudeFeature", "detail": {
                "operation": "NewBodyFeatureOperation",
                "profile": [{"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0}],
                "extentType": "OneSideFeatureExtentType",
                "extentOne": {"_type": "DistanceExtentDefinition",
                              "distance": param("d1", "10 mm", 1.0, "mm")},
                "taperAngleOne": param("d2", "0.0 deg", 0.0, "deg")
            }, "_f3d": state(1)},
            {"index": 2, "name": "Fillet1", "objectType": "FilletFeature", "detail": {
                "edgeSets": [{"_type": "ConstantRadiusFilletEdgeSet",
                    "radius": param("d3", "2 mm", 0.2, "mm"),
                    "edges": [{"kind": "edge", "objectType": "BRepEdge", "length": 1.0,
                               "start_point": [1.0, 0.0, 1.0], "end_point": [1.0, 1.0, 1.0],
                               "mid_point": [1.0, 0.5, 1.0]}],
                    "isTangentChain": true}],
                "isRollingBallCorner": true
            }, "_f3d": state(2)}
        ]}
    });
    let path = dir.join("Rounding.dump.json");
    let text = serde_json::to_string_pretty(&dump).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The part of [`ffi::ipt_write_test_file`].
fn ipt_write_test_file(path: &str) -> Result<(), String> {
    std::fs::write(path, mitcad_ipt::testdata::test_part()).map_err(|e| format!("{path}: {e}"))
}

/// The part of [`ffi::ipt_write_test_design_file`].
fn ipt_write_test_design_file(path: &str) -> Result<(), String> {
    std::fs::write(path, mitcad_ipt::testdata::test_part_with_design())
        .map_err(|e| format!("{path}: {e}"))
}

fn ipt_write_test_hidden_design_file(path: &str) -> Result<(), String> {
    std::fs::write(
        path,
        mitcad_ipt::testdata::test_part_with_hidden_design_body(),
    )
    .map_err(|e| format!("{path}: {e}"))
}

/// The part of [`ffi::ipt_write_test_mesh_file`].
fn ipt_write_test_mesh_file(path: &str) -> Result<(), String> {
    std::fs::write(path, mitcad_ipt::testdata::test_mesh_part()).map_err(|e| format!("{path}: {e}"))
}

/// The part of [`ffi::ipt_write_test_empty_file`].
fn ipt_write_test_empty_file(path: &str) -> Result<(), String> {
    std::fs::write(path, mitcad_ipt::testdata::test_empty_part())
        .map_err(|e| format!("{path}: {e}"))
}

/// The part of [`ffi::ipt_write_test_hidden_file`].
fn ipt_write_test_hidden_file(path: &str) -> Result<(), String> {
    std::fs::write(path, mitcad_ipt::testdata::test_part_with_hidden_body())
        .map_err(|e| format!("{path}: {e}"))
}

/// The part of [`ffi::ipt_write_test_appearance_file`].
fn ipt_write_test_appearance_file(path: &str) -> Result<(), String> {
    std::fs::write(
        path,
        mitcad_ipt::testdata::test_part_with_designed_appearances(),
    )
    .map_err(|e| format!("{path}: {e}"))
}

/// The project of [`ffi::ipt_write_test_assembly`].
fn ipt_write_test_assembly(dir: &str) -> Result<(), String> {
    mitcad_ipt::testassembly::write_test_project(std::path::Path::new(dir))
        .map_err(|e| format!("{dir}: {e}"))
}
