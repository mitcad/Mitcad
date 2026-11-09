// SPDX-License-Identifier: MIT
#pragma once

// The taper of an emboss (mitcad#1547; emboss.hpp, EmbossWrapSpec::taper
// and EmbossProjectSpec::taper): a region's loops moved in their plane by
// OCCT's offset curves, each edge to its one image. Internal to the
// geometry library.

#include <string>

#include <NCollection_DataMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Vertex.hxx>

#include "mitcad/geometry/naming.hpp"

namespace mitcad::geometry::detail {

// Each edge of a planar face and its image moved by an offset, whose
// curve runs as the edge's.
using MovedEdges = NCollection_DataMap<TopoDS_Shape, TopoDS_Edge, TopTools_ShapeMapHasher>;

// The loops of a planar face moved: each edge's image, or for an edge the
// move shrank to nothing the corner its neighbours' images meet at, and
// the face the images bound.
struct MovedLoops {
  MovedEdges moved;
  NCollection_DataMap<TopoDS_Shape, TopoDS_Vertex, TopTools_ShapeMapHasher> vanished;
  TopoDS_Face face;
};

// The planar face's loops moved in its plane by `distance` (positive:
// outwards, away from the material; holes shrink): each edge's curve
// offset by OCCT (a line's or a circle's as a line or a circle, any other
// curve's as its offset curve), the corners where neighbouring offsets
// meet (their ends where the edges join smoothly, else where they cross,
// continued past their ends as far as needed), as an offset with
// intersection joins gives. An edge whose image the corners shrink to
// nothing is gone and its neighbours meet anew. `regions` names the region
// in the error a collapse throws (std::invalid_argument): a loop keeps
// fewer than two edges, neighbours do not meet, or the moved edges cross.
MovedLoops moved_loops(const TopoDS_Face& face, double distance, const std::string& regions);

// The region keys of a list, for messages: "r1, r2".
std::string region_list(const NameList& regions);

} // namespace mitcad::geometry::detail
