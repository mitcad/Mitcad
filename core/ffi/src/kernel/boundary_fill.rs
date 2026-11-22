// SPDX-License-Identifier: MIT
//! Boundary Fill: the cells tool bodies enclose
//! (`geometry/include/mitcad/geometry/boundary_fill.hpp`, mitcad#457).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    unsafe extern "C++" {
        include!("bridge/boundary_fill.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// The cells the points pick, in the order of the points.
        type BoundaryCells;

        /// The cell each point (x, y, z in turn) picks among the tools.
        fn boundary_cells(tools: &ShapeList, points: &[f64]) -> Result<UniquePtr<BoundaryCells>>;
        fn cell_count(self: &BoundaryCells) -> usize;
        fn cell(self: &BoundaryCells, index: usize) -> SharedPtr<Shape>;
        /// The tools that hold the cell's point, in increasing order.
        fn cell_holders(self: &BoundaryCells, index: usize) -> Vec<usize>;

        /// The closed regions of surface and plane tools (mitcad#2149) and
        /// where each point asked about lies.
        type BoundaryRegionList;

        /// The regions the tools' faces and the planes (origin, normal and
        /// x axis of each in turn) bound, at the points (x, y, z in turn).
        fn boundary_regions(
            tools: &ShapeList,
            planes: &[f64],
            plane_names: &Vec<String>,
            points: &[f64],
        ) -> Result<UniquePtr<BoundaryRegionList>>;
        fn region_count(self: &BoundaryRegionList) -> usize;
        fn region(self: &BoundaryRegionList, index: usize) -> SharedPtr<Shape>;
        fn region_point(self: &BoundaryRegionList, index: usize) -> Vec<f64>;
        fn place_count(self: &BoundaryRegionList) -> usize;
        /// 0: in a region, 1: on a face, 2: in an open region.
        fn place_kind(self: &BoundaryRegionList, index: usize) -> u8;
        fn place_region(self: &BoundaryRegionList, index: usize) -> usize;
        /// x, y, z of the points along an open region's gap in turn.
        fn place_gap(self: &BoundaryRegionList, index: usize) -> Vec<f64>;
    }
}
