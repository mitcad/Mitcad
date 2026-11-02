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

// Gives the edges of a copy (InputCopy) curves on surfaces of their own.
// The copy shares its source's p-curves, and shape healing moves p-curves
// in place (a wire on a periodic surface by whole periods, ShapeFix_Face's
// missing seam), which would move the source's too: an operation that
// heals a copy calls this on it first (mitcad#343).
void own_pcurves(const TopoDS_Shape& copy);

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
  // Exact open-edge provenance only, through the algorithm's topology
  // history. Several images/sources are retained to diagnose ambiguity.
  void carry_open_edges(BRepBuilderAPI_MakeShape& operation, const Shape& input,
                        const InputCopy* copy = nullptr, const std::string& rename = {});
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
  // Gives each face of the result that has no name the names of the input's
  // faces on its surface (the same plane facing the same way, or the same
  // kind of surface through the middles of both faces) whose names no face
  // of the result holds: faces cut by an operation whose history gives them
  // no image (a block's end rounded to a half round in one fillet,
  // mitcad#821). Call after the other naming, before finish(). With the
  // environment variable MITCAD_DEBUG_NAMING set, prints for each face left
  // without a name the input's faces it was compared with and why none was
  // taken (`operation` heads the lines).
  void carry_by_surface(const Shape& input, const char* operation);
  // Gives each face of the result that has no name the names of the
  // input's face it lies in: a few points inside it all within `tolerance`
  // (or the faces' own tolerances, where larger) of that face, the nearest
  // where several are. For an operation that rebuilds faces in place and
  // whose history misses some (shape healing of a dressup's result,
  // mitcad#1118); call after the other naming, before finish().
  void carry_by_containment(const Shape& input, double tolerance);
  // True when a face of the result has the name.
  bool holds(const std::string& name) const;
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
  // Marks the shapes it makes as found valid by OCCT's checker
  // (Shape::checked_valid): the result was checked.
  void set_valid(bool valid) { m_valid = valid; }
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
  std::vector<Shape::NamedOpenEdge> m_openEdges;
  bool m_unified = false;
  bool m_valid = false;
};

} // namespace mitcad::geometry::detail
