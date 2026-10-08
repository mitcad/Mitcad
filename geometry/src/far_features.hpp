// SPDX-License-Identifier: MIT
#pragma once

// Booleans on a body whose faces hold many holes away from the tool (a
// perforated plate cut again). Internal to the geometry library.
//
// OCCT's booleans, the face merge after them and the checker go through
// every wire of a face they touch, several of them once per wire or per
// pair of wires: a plate with thousands of holes took seconds to minutes
// for a small cut at its edge, however far the holes were from it. Here
// the holes away from the tool are taken out of the body first (a "far
// feature": far faces whose boundary is whole inner wires of faces near
// the tool, such as the wall of a hole through a plate), the operation
// runs on what is left (a closed solid, with the holes filled), and the
// holes go back into the faces of its result that took the place of the
// faces they were in. The tool does not reach them (their boxes are apart),
// so the operation would have left them as they were.
//
// A feature stays in the body when the face merge after the operation
// could change it: a face of it next to a face in the same direction
// (coplanar or tangent neighbours that the merge would unite), or a vertex
// of it between only two edges along the same faces (edges the merge
// would join). A body that is itself a merged result has nothing of the
// kind left (whatever the merge could unite or join there, it did), and
// the hole's faces and edges keep their neighbours' surfaces and curves.

#include <optional>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_MakeShape.hxx>
#include <Bnd_Box.hxx>
#include <TopoDS_Shape.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// Whether booleans take far features out (on by default; off with the
// environment variable MITCAD_NO_FAR_FEATURES=1, for comparisons). Tests
// switch it to compare both ways.
bool far_features_enabled();
void enable_far_features(bool on);

class FarFeatures {
public:
  // The features of `body` (one solid of one shell; the caller's own copy,
  // whose sub-shapes the result shares) that lie away from a tool in
  // `tool_box`; none when there are too few to be worth it. `unified`: the
  // body is a merged result (Shape::unified), so nothing in a feature can
  // merge after the operation either, and the checks for that are skipped.
  static std::optional<FarFeatures> split(const TopoDS_Shape& body, const Bnd_Box& tool_box, bool unified);

  // The body without them: a closed solid, in a compound when the body was.
  const TopoDS_Shape& reduced() const { return m_reduced; }
  // The face of the reduced body that stands for a face of the body (the
  // face itself when it lost no hole); null for a face of a feature.
  TopoDS_Shape stand_in(const TopoDS_Shape& face) const;

  // The result of an operation on the reduced body (`operation`, whose
  // history leads from its faces to the result's) with the features put
  // back: the same faces, except that those which took the place of faces
  // with holes hold them again (`replaced`: each such face of the result
  // and the face it became), and the features' faces. None when the holes
  // cannot be put back (a face that held them is gone, split into pieces
  // none of which holds a hole's point, on another surface, or a result
  // that is not solids of one shell each); then the operation runs on the
  // whole body.
  struct Restored {
    TopoDS_Shape shape;
    std::vector<std::pair<TopoDS_Shape, TopoDS_Shape>> replaced;
  };
  std::optional<Restored> restore(const TopoDS_Shape& result, BRepBuilderAPI_MakeShape& operation) const;

  // The number of holes taken out.
  std::size_t holes() const { return m_holes.size(); }

private:
  // A face that lost holes: the face of the body, as the body uses it, and
  // its stand-in.
  struct Host {
    TopoDS_Shape face;
    TopoDS_Shape stand_in;
  };
  // A hole: the wire as its face's own (not located, oriented within the
  // face), its host, and the feature whose faces it bounds.
  struct Hole {
    TopoDS_Shape wire;
    std::size_t host = 0;
    std::size_t feature = 0;
  };

  TopoDS_Shape m_reduced;
  std::vector<Host> m_hosts;
  std::vector<Hole> m_holes;
  // The faces of each feature, as the body uses them.
  std::vector<std::vector<TopoDS_Shape>> m_features;
  ShapeMap m_removed;
};

} // namespace mitcad::geometry::detail
