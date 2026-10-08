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
    }

    unsafe extern "C++" {
        include!("bridge/exchange.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// Reads B-rep data; face i is named `<feature>:import(<first_face + i>)`.
        fn import_brep(feature: &str, data: &[u8], first_face: u32) -> Result<SharedPtr<Shape>>;
        fn face_count(shape: &Shape) -> usize;
        /// OCCT binary B-rep of the shape.
        fn brep_data(shape: &Shape) -> Result<Vec<u8>>;
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
        /// (`brep_import.rs`); null when it cannot be built.
        fn f3d_build_body(data: &BrepBodyData) -> SharedPtr<Shape>;
        /// A body `f3d_build_body` built, from its B-rep data (`brep_data`):
        /// the next try of an import takes it from one the watchdog gave up
        /// while it built it (mitcad#82); null when it cannot be read.
        fn f3d_read_body(data: &[u8]) -> SharedPtr<Shape>;
        /// Crashes inside OCCT as errors from now on
        /// (`mitcad/geometry/guard.hpp`).
        fn catch_occt_crashes();

        // .ipt import (mitcad#60).
        /// Builds one body from its neutral B-rep data with OCCT and
        /// reports how it went, as `f3d_bodies` does for each body; its
        /// shape (null when not built) is appended to the list.
        fn build_brep_body(data: &BrepBodyData, shapes: Pin<&mut ShapeList>) -> F3dBody;
        /// Whether OCCT's checker finds the shape valid (the bodies of the
        /// `.ipt` import with its history, mitcad#60).
        fn shape_is_valid(shape: &Shape) -> bool;
    }

    extern "Rust" {
        /// Writes a small .f3d file for tests: Mitcad's own ASM test bodies
        /// (a 10 mm cube and a cylinder of radius 10 mm and height 20 mm)
        /// in a `.smb` blob each, without the rest of an .f3d design.
        fn f3d_write_test_file(path: &str) -> Result<()>;

        // .ipt import (mitcad#60).
        /// Writes a small .ipt part file for tests (`mitcad_ipt::testdata`):
        /// the cube and the cylinder above, each in a B-rep record of its
        /// own, part number `MITCAD-TEST-1`, material `Aluminum`, inches.
        fn ipt_write_test_file(path: &str) -> Result<()>;
        /// Writes a small .ipt part file with a design for tests
        /// (`mitcad_ipt::testdata::test_part_with_design`): the 10 mm cube
        /// and the parameter, sketch and extrusion that make it, mm.
        fn ipt_write_test_design_file(path: &str) -> Result<()>;

        // .f3d import (mitcad#82).
        /// `f3d_build_body` advanced (its healing, face by face): progress
        /// of the import running on this thread, for its watchdog.
        fn f3d_build_progress();
    }
}

/// See [`ffi::f3d_build_progress`].
fn f3d_build_progress() {
    crate::f3d_import::build_progress();
}

/// The blobs of [`ffi::f3d_write_test_file`] in a zip of stored entries.
fn f3d_write_test_file(path: &str) -> Result<(), String> {
    use mitcad_f3d::testdata::{cube_blob, cylinder_blob};
    use mitcad_f3d::zip::{Method, Writer};

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

/// The part of [`ffi::ipt_write_test_file`].
fn ipt_write_test_file(path: &str) -> Result<(), String> {
    std::fs::write(path, mitcad_ipt::testdata::test_part()).map_err(|e| format!("{path}: {e}"))
}

/// The part of [`ffi::ipt_write_test_design_file`].
fn ipt_write_test_design_file(path: &str) -> Result<(), String> {
    std::fs::write(path, mitcad_ipt::testdata::test_part_with_design())
        .map_err(|e| format!("{path}: {e}"))
}
