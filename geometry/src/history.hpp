// SPDX-License-Identifier: MIT
#pragma once

// Carries face names through an operation's history into its result (see
// naming.hpp). Internal to the geometry library.

#include <functional>
#include <memory>
#include <set>
#include <string>
#include <unordered_map>
#include <vector>

#include <BRepBuilderAPI_MakeShape.hxx>
#include <BRepTools_History.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_TShape.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// A copy of an OCCT algorithm's input: its topology, sharing the geometry.
// Inputs are cached results that share their sub-shapes with others, and
// several algorithms write into the sub-shapes of their inputs in place
// (T0e, input_check.hpp): the face merge after a boolean gives the edges of
// the faces it merges p-curves on the merged face's surface; fillets and
// chamfers add p-curves and widen tolerances, offsets and lofts widen
// tolerances, shape healing adds p-curves. They work on copies.
//
// The copy is the input's own data in new sub-shapes: the same curves on
// surfaces, tolerances, flags and points on curves, the same structure
// (and so the same order of sub-shapes), without meshes. Not
// BRepBuilderAPI_Copy: it stores the p-curves of edges on planes, which
// OCCT otherwise computes when it needs them, and a boolean of a corpus
// design on such copies left a face with intersecting wires where the
// same boolean on the inputs did not.
class InputCopy {
public:
  explicit InputCopy(const TopoDS_Shape& shape);
  // The inputs of one operation copied together: the sub-shapes they share
  // stay shared (a boolean treats shared faces and edges as one, coincident
  // ones it has to intersect).
  explicit InputCopy(const std::vector<TopoDS_Shape>& shapes);

  // The copy of the input, or of the i-th of several.
  const TopoDS_Shape& shape(std::size_t i = 0) const { return m_shapes.at(i); }
  // The copy of a sub-shape of the input, located and oriented as it is.
  TopoDS_Shape of(const TopoDS_Shape& part) const;

private:
  TopoDS_Shape copied(const TopoDS_Shape& shape);

  // The copy of each sub-shape, without location, forward.
  std::unordered_map<const TopoDS_TShape*, TopoDS_Shape> m_copies;
  std::vector<TopoDS_Shape> m_shapes;
};

// A copy of the shape (InputCopy) with the same face names, and so the same
// face, edge and vertex numbers.
ShapePtr working_copy(const Shape& shape);

class FaceNamer {
public:
  explicit FaceNamer(TopoDS_Shape result);

  // Gives a face of the result a name.
  void add(const TopoDS_Shape& face, const std::string& name);
  // Names the faces the operation generated from an input edge or vertex.
  void generated(BRepBuilderAPI_MakeShape& operation, const TopoDS_Shape& source,
                 const std::string& name);
  // Carries the names of the input's faces into their images: a kept face
  // keeps its names, a modified or split face passes them to every piece, a
  // deleted face loses them. `source` tags the faces that come from this
  // input (see sources()); negative for none.
  void carry(BRepBuilderAPI_MakeShape& operation, const Shape& input, int source = -1);
  // The same for an operation that ran on a copy of the input.
  void carry(BRepBuilderAPI_MakeShape& operation, const Shape& input, const InputCopy& copy,
             int source = -1);
  // The same for an operation that ran on stand-ins of the input's faces
  // (`stand_in` gives each face's, null for faces it left out).
  void carry(BRepBuilderAPI_MakeShape& operation, const Shape& input,
             const std::function<TopoDS_Shape(const TopoDS_Shape&)>& stand_in, int source = -1);
  // The same with the history of an algorithm that is not a MakeShape
  // (BRepTools_History has a constructor for any algorithm with Modified,
  // Generated and IsDeleted). Faces listed in `except` are not carried.
  void carry(const BRepTools_History& history, const Shape& input,
             const std::vector<int>& except = {});
  void generated(const BRepTools_History& history, const TopoDS_Shape& source,
                 const std::string& name);
  // Gives a face of the result names, and tags it as coming from an input
  // (negative for none).
  void add(const TopoDS_Shape& face, const NameList& names, int source);
  // Takes over the names and tags of another namer's faces, each at its
  // image in this result (`image`; the face itself where it is null).
  void adopt(const FaceNamer& other, const std::function<TopoDS_Shape(const TopoDS_Shape&)>& image);
  // True when the face of the result has a name.
  bool named(const TopoDS_Shape& face) const;
  // True when the face is a face of the result.
  bool contains(const TopoDS_Shape& face) const { return m_faces.Contains(face); }

  // Numbers the pieces of split faces: a name held by several faces becomes
  // "<name>#k" on each, k in geometric order. Call once, after naming.
  void finish();

  const TopoDS_Shape& result() const { return m_result; }
  // Marks the shapes it makes as unified (Shape::unified).
  void set_unified(bool unified) { m_unified = unified; }
  // The whole result with its names.
  ShapePtr shape() const;
  // One shape per solid of the result, in geometric order, with the tags of
  // the inputs whose faces it contains.
  struct Piece {
    ShapePtr shape;
    std::set<int> sources;
  };
  std::vector<Piece> pieces() const;

private:
  std::vector<Shape::NamedFace> named_faces() const;
  void carry_face(BRepBuilderAPI_MakeShape& operation, const TopoDS_Shape& face,
                  const NameList& names, int source);

  TopoDS_Shape m_result;
  ShapeMap m_faces;
  std::vector<NameList> m_names;
  std::vector<std::set<int>> m_sources;
  bool m_unified = false;
};

} // namespace mitcad::geometry::detail
