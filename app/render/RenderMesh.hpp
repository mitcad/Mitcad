// SPDX-License-Identifier: MIT
#pragma once

#include <TopoDS_Shape.hxx>

#include "render/Renderer.hpp"

namespace mitcad::render {

// The triangles of a shape's faces (their display triangulation; faces
// without one are meshed first), as a renderer's mesh in the shape's own
// coordinates: each face keeps its vertices, with normals averaged over its
// own triangles, and the faces come in the order the application's shapes
// number them (MeshData::faceTriangles). Every triangle faces out of the
// solid whatever way the triangulation winds (mitcad#319): by the outward
// normals of the face's surface, and a closed mesh body by its volume
// (MeshData::turned counts those turned round). A mesh body's texture
// coordinates (its triangulation's UV nodes, mitcad#1862) come with it
// (MeshData::uvs). Reads the shape's
// triangulations, so like drawing it, only while no job computes the model.
MeshData meshOf(const TopoDS_Shape& shape);

} // namespace mitcad::render
