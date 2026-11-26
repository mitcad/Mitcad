// SPDX-License-Identifier: MIT
//! Face operations: shell, draft, offset, delete, replace and split
//! (`geometry/include/mitcad/geometry/faceops.hpp`, `split.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ToolKind {
        Plane,
        Face,
        Body,
        Curves,
    }

    /// What an operation works against (`mitcad_model::ToolInput`): a plane
    /// through `origin` with `normal`, the face `face` of the first shape of
    /// the operation's tool shapes, that shape as a body, or the `curves` of
    /// the regions on the frame swept along `normal`. `extend` grows a face
    /// along its surface (splits).
    struct ToolSpec {
        kind: ToolKind,
        origin: [f64; 3],
        normal: [f64; 3],
        face: String,
        curves: Vec<String>,
        extend: bool,
    }

    /// The faces with a thickness of their own (mitcad#2443): `own_faces`
    /// with their `own_inside` and `own_outside` thickness, one each.
    struct ShellParams {
        inside: f64,
        outside: f64,
        tangent_chain: bool,
        rounded: bool,
        own_faces: Vec<String>,
        own_inside: Vec<f64>,
        own_outside: Vec<f64>,
    }

    /// `two_sided` uses `angle2` on the other side of the plane.
    struct DraftParams {
        angle: f64,
        two_sided: bool,
        angle2: f64,
        flip: bool,
        tangent_chain: bool,
    }

    unsafe extern "C++" {
        include!("bridge/faceops.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
        type Frame = crate::kernel::profile::ffi::Frame;
        type Region = crate::kernel::profile::ffi::Region;

        fn shell(
            feature: &str,
            body: &Shape,
            faces: &[String],
            params: &ShellParams,
        ) -> Result<SharedPtr<Shape>>;
        fn draft(
            feature: &str,
            body: &Shape,
            faces: &[String],
            plane: &ToolSpec,
            plane_shapes: &ShapeList,
            params: &DraftParams,
        ) -> Result<SharedPtr<Shape>>;
        fn offset_faces(
            feature: &str,
            body: &Shape,
            faces: &[String],
            distance: f64,
            tangent_chain: bool,
        ) -> Result<SharedPtr<Shape>>;
        fn delete_faces(feature: &str, body: &Shape, faces: &[String]) -> Result<SharedPtr<Shape>>;
        fn replace_faces(
            feature: &str,
            body: &Shape,
            faces: &[String],
            target: &ToolSpec,
            target_shapes: &ShapeList,
            tangent_chain: bool,
        ) -> Result<SharedPtr<Shape>>;
        /// The pieces in order.
        fn split_body(
            feature: &str,
            body: &Shape,
            tool: &ToolSpec,
            tool_shapes: &ShapeList,
            frame: &Frame,
            regions: &[Region],
        ) -> Result<UniquePtr<ShapeList>>;
        fn split_faces(
            feature: &str,
            body: &Shape,
            faces: &[String],
            tool: &ToolSpec,
            tool_shapes: &ShapeList,
            frame: &Frame,
            regions: &[Region],
        ) -> Result<SharedPtr<Shape>>;
        /// The faces a split cuts with around the box from `low` to
        /// `high`, clipped to it (the preview of an extended tool,
        /// mitcad#712).
        fn extended_tool(
            tool: &ToolSpec,
            tool_shapes: &ShapeList,
            frame: &Frame,
            regions: &[Region],
            low: [f64; 3],
            high: [f64; 3],
        ) -> Result<SharedPtr<Shape>>;
    }
}
