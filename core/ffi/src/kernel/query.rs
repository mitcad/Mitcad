// SPDX-License-Identifier: MIT
//! Queries: names, measurements and the solids of a shape
//! (`geometry/include/mitcad/geometry/query.hpp`, `shape.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    struct FaceInfo {
        names: Vec<String>,
        surface: String,
        area: f64,
    }

    struct EdgeInfo {
        /// Empty when unnamed.
        name: String,
        curve: String,
        length: f64,
    }

    struct MassProperties {
        volume: f64,
        area: f64,
        center: [f64; 3],
    }

    struct BoundingBox {
        empty: bool,
        min: [f64; 3],
        max: [f64; 3],
    }

    unsafe extern "C++" {
        include!("bridge/query.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        fn faces(shape: &Shape) -> Result<Vec<FaceInfo>>;
        fn edges(shape: &Shape) -> Result<Vec<EdgeInfo>>;
        fn mass_properties(shape: &Shape) -> Result<MassProperties>;
        /// The measures of OCCT's fixed Gauss points (mitcad#139).
        fn fixed_point_properties(shape: &Shape) -> Result<MassProperties>;
        fn bounding_box(shape: &Shape) -> Result<BoundingBox>;
        /// A box that holds the shape, cheaper and looser than
        /// `bounding_box` on bodies of many curved faces.
        fn enclosing_box(shape: &Shape) -> Result<BoundingBox>;
        /// The number of edges or faces a name resolves to.
        fn count_edges(shape: &Shape, name: &str) -> usize;
        fn count_faces(shape: &Shape, name: &str) -> usize;
        /// The solids with their face names, in geometric order.
        fn solids(shape: &Shape) -> Result<UniquePtr<ShapeList>>;
        /// How many solids the shape holds (mitcad#658).
        fn solid_count(shape: &Shape) -> usize;
        /// The tight boxes of the faces or edges at these indices
        /// (mitcad#714); `empty` for one without geometry.
        fn face_bounding_boxes(shape: &Shape, faces: &[u32]) -> Result<Vec<BoundingBox>>;
        fn edge_bounding_boxes(shape: &Shape, edges: &[u32]) -> Result<Vec<BoundingBox>>;
        /// `bounding_box` of each shape, measured beside each other
        /// (mitcad#1997).
        fn bounding_boxes(shapes: &ShapeList) -> Result<Vec<BoundingBox>>;
    }
}
