// SPDX-License-Identifier: MIT
//! The assembly structure of files (mitcad#1274): a STEP file's distinct
//! parts and assemblies once each, and the instances that place them
//! (`geometry/io` `read_step_structured`), so that the import makes
//! components with occurrences instead of a body per placed copy.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// A body read with the structure: its name and colour; its shape is
    /// at the same index of the shape list.
    struct StructureBody {
        name: String,
        has_color: bool,
        /// sRGB components in [0, 1].
        color: [f64; 3],
    }

    /// A part or an assembly of the file: a part's bodies are a run of
    /// `FileStructure::bodies`, an assembly's instances a run of
    /// `FileStructure::instances`.
    struct StructureNode {
        name: String,
        first_body: usize,
        body_count: usize,
        first_instance: usize,
        instance_count: usize,
    }

    /// An instance of a node: the rotation's rows and the translation
    /// (mm) in its parent, and its name in the file (may be empty).
    struct StructureInstance {
        node: usize,
        name: String,
        rotation: [f64; 9],
        translation: [f64; 3],
    }

    /// What `read_step_structure` read: with `assembly` the nodes and the
    /// instances placed in the design (`roots`), and the parts' bodies;
    /// without it the bodies alone, as `read_file` reads them.
    struct FileStructure {
        assembly: bool,
        bodies: Vec<StructureBody>,
        nodes: Vec<StructureNode>,
        instances: Vec<StructureInstance>,
        roots: Vec<StructureInstance>,
    }

    unsafe extern "C++" {
        include!("bridge/file_structure.hpp");
        include!("bridge/trycatch.hpp");

        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// Reads a STEP file once: its structure with `keep_assembly` when
        /// it has an assembly that places two parts or more, else its
        /// bodies. The bodies' shapes are appended to the list.
        fn read_step_structure(
            path: &str,
            keep_assembly: bool,
            shapes: Pin<&mut ShapeList>,
        ) -> Result<FileStructure>;
    }
}
