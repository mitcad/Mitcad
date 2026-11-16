// SPDX-License-Identifier: MIT
//! STEP files of a design's component tree (mitcad#1915): each component
//! once, as a part or an assembly, instanced by its occurrences
//! (`geometry/io` `write_step_tree`), so that the file keeps the design's
//! sub-assemblies instead of placing every body straight in one assembly.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// A placement in a definition: the rotation's rows and the
    /// translation (a rotation and a translation only), and the name of
    /// what it places (an occurrence's; may be empty).
    struct TreePlacement {
        rotation: [f64; 9],
        translation: [f64; 3],
        name: String,
    }

    /// A body of a definition: its name, colour and placements in the
    /// definition (once as it is when empty); its shape is at the same
    /// index of the shape list.
    struct TreeBody {
        name: String,
        has_color: bool,
        /// sRGB components in [0, 1].
        color: [f64; 3],
        placements: Vec<TreePlacement>,
    }

    /// An instance of `definitions[definition]` in its parent definition.
    struct TreeInstance {
        definition: usize,
        placement: TreePlacement,
    }

    /// A definition of the tree: its name, its bodies (a run of the
    /// bodies, from `first_body`) and its instances. The first is the
    /// design.
    struct TreeDefinition {
        name: String,
        first_body: usize,
        body_count: usize,
        instances: Vec<TreeInstance>,
    }

    /// The unit of the file (`mitcad::io::LengthUnit`).
    enum TreeUnit {
        Millimeter,
        Centimeter,
        Meter,
        Inch,
        Foot,
    }

    unsafe extern "C++" {
        include!("bridge/step_tree.hpp");
        include!("bridge/trycatch.hpp");

        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// Writes the tree to a STEP file (AP242 instead of AP214 with
        /// `ap242`), the bodies' shapes from the list.
        fn write_step_tree(
            path: &str,
            shapes: &ShapeList,
            bodies: &[TreeBody],
            definitions: &[TreeDefinition],
            ap242: bool,
            unit: TreeUnit,
        ) -> Result<()>;
    }
}
