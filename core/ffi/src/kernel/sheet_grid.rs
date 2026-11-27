// SPDX-License-Identifier: MIT
//! Faces sampled over their surface's parameters for an approximate
//! development by triangulation (mitcad#2442), with their boundaries
//! (mitcad#2558; `core/cpp/bridge/sheet_grid.cpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// An edge of a face's boundary (see `SheetFaceEdge`): its points'
    /// places in the grid as x, y pairs, the points and the normals as x,
    /// y, z triples.
    struct SheetGridEdge {
        cells: Vec<f64>,
        points: Vec<f64>,
        normals: Vec<f64>,
    }
    /// A wire of a face's boundary (see `SheetFaceWire`).
    struct SheetGridWire {
        outer: bool,
        edges: Vec<SheetGridEdge>,
    }
    /// A face's grid (see `SheetFaceGrid`): points and unit normals out of
    /// the body as x, y, z triples, row by row.
    struct SheetGrid {
        points: Vec<f64>,
        normals: Vec<f64>,
        area: f64,
        wires: Vec<SheetGridWire>,
    }
    unsafe extern "C++" {
        include!("bridge/sheet_grid.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// Every piece of the face named `face` sampled at `columns + 1`
        /// by `rows + 1` points over its parameter bounds, with its
        /// boundary's edges at `edge_steps` steps each when that is above
        /// 0.
        fn sheet_face_grid(
            shape: &Shape,
            face: &str,
            columns: usize,
            rows: usize,
            edge_steps: usize,
        ) -> Result<Vec<SheetGrid>>;
    }
}
