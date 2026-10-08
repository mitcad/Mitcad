// SPDX-License-Identifier: MIT
#pragma once

#include <TopoDS_Shape.hxx>

#include "render/Renderer.hpp"

namespace mitcad::render {

// The triangles of a shape's faces (their display triangulation; faces
// without one are meshed first), as a renderer's mesh in the shape's own
// coordinates: each face keeps its vertices, with normals averaged over its
// own triangles, and the faces come in the order the application's shapes
// number them (MeshData::faceTriangles). Reads the shape's triangulations,
// so like drawing it, only while no job computes the model.
MeshData meshOf(const TopoDS_Shape& shape);

} // namespace mitcad::render
