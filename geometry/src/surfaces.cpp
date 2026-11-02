// SPDX-License-Identifier: MIT
#include "mitcad/geometry/surfaces.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <map>
#include <memory>
#include <numeric>
#include <stdexcept>
#include <string>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "free_loops.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// The offsets' tolerance, as the face operations use (faceops.cpp).
constexpr double kTolerance = 1.0e-6;
// The sewing tolerance of a stitch asked for 0.
constexpr double kSewing = 1.0e-6;
// The largest step between an open boundary's points, and the most points
// one edge gives.
constexpr double kBoundaryStep = 0.25;
constexpr int kMostBoundaryPoints = 4000;

// A shell of one face.
TopoDS_Shell shell_of(const TopoDS_Face& face) {
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  builder.Add(shell, face);
  return shell;
}

// The free edges of a body (edges of one face, not a seam of it), grouped
// into connected loops in a deterministic geometric order. Edge indices
// of `body`.
std::vector<std::vector<int>> free_loops(const Shape& body) {
  const detail::EdgeFaces edge_faces(body);
  std::vector<int> free_edges;
  for (int e = 0; e < body.edge_count(); ++e) {
    const TopoDS_Edge& edge = body.edge(e);
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const std::vector<int> faces = edge_faces.at(e);
    if (faces.size() == 1 && !BRep_Tool::IsClosed(edge, body.face(faces.front()))) {
      free_edges.push_back(e);
    }
  }
  // Edges that share a vertex are in one loop (a union of sets).
  std::vector<std::size_t> parent(free_edges.size());
  std::iota(parent.begin(), parent.end(), std::size_t{0});
  const auto root = [&parent](std::size_t i) {
    while (parent[i] != i) {
      parent[i] = parent[parent[i]];
      i = parent[i];
    }
    return i;
  };
  ShapeMap vertices;
  std::map<int, std::size_t> first_at; // vertex index -> first free edge there
  for (std::size_t i = 0; i < free_edges.size(); ++i) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(body.edge(free_edges[i]), a, b);
    for (const TopoDS_Vertex& v : {a, b}) {
      if (v.IsNull()) {
        continue;
      }
      const int index = vertices.Add(v);
      const auto found = first_at.find(index);
      if (found == first_at.end()) {
        first_at.emplace(index, i);
      } else {
        parent[root(i)] = root(found->second);
      }
    }
  }
  std::map<std::size_t, std::vector<int>> groups;
  for (std::size_t i = 0; i < free_edges.size(); ++i) {
    groups[root(i)].push_back(free_edges[i]);
  }
  std::vector<std::pair<detail::ShapeKey, std::vector<int>>> keyed;
  for (auto& entry : groups) {
    BRep_Builder builder;
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    for (const int e : entry.second) {
      builder.Add(wire, body.edge(e));
    }
    keyed.emplace_back(detail::shape_key(wire), std::move(entry.second));
  }
  std::stable_sort(keyed.begin(), keyed.end(),
                   [](const auto& x, const auto& y) { return x.first < y.first; });
  std::vector<std::vector<int>> loops;
  for (auto& entry : keyed) {
    loops.push_back(std::move(entry.second));
  }
  return loops;
}

// Whether every vertex of the edges ends two of them.
bool closes(const Shape& body, const std::vector<int>& edges) {
  ShapeMap vertices;
  std::map<int, int> ends;
  for (const int e : edges) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(body.edge(e), a, b);
    if (a.IsNull() || b.IsNull()) {
      return false;
    }
    ++ends[vertices.Add(a)];
    ++ends[vertices.Add(b)];
  }
  return !ends.empty() &&
         std::all_of(ends.begin(), ends.end(), [](const auto& entry) { return entry.second == 2; });
}

// Points along the edge, its ends included, at most kBoundaryStep apart
// by parameter steps.
void sample_edge(const TopoDS_Edge& edge, std::vector<gp_Pnt>& points) {
  const BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  const double length = GCPnts_AbscissaPoint::Length(curve);
  int steps = 1;
  if (std::isfinite(length) && length > 0.0) {
    steps = static_cast<int>(std::min(std::ceil(length / kBoundaryStep), static_cast<double>(kMostBoundaryPoints)));
  }
  steps = std::max(steps, 4);
  for (int k = 0; k <= steps; ++k) {
    points.push_back(curve.Value(first + (last - first) * k / steps));
  }
}

GeomAbs_Shape order_of(Continuity continuity) {
  switch (continuity) {
  case Continuity::G1:
    return GeomAbs_G1;
  case Continuity::G2:
    return GeomAbs_G2;
  case Continuity::G0:
    break;
  }
  return GeomAbs_C0;
}

// The edges of the loop a patch fills, as indices of `body`.
std::vector<int> loop_edges(const Shape& body, const PatchLoop& loop) {
  std::vector<int> edges;
  if (loop.opening >= 0) {
    const std::vector<std::vector<int>> loops = free_loops(body);
    if (static_cast<std::size_t>(loop.opening) >= loops.size()) {
      throw std::invalid_argument("the body has no opening " + std::to_string(loop.opening) + " (it has " +
                                  std::to_string(loops.size()) + ")");
    }
    edges = loops[static_cast<std::size_t>(loop.opening)];
  } else {
    for (const std::string& name : loop.edges) {
      const std::vector<int> found = body.find_edges(name);
      if (found.empty()) {
        throw std::invalid_argument("the body has no edge " + name);
      }
      for (const int e : found) {
        if (std::find(edges.begin(), edges.end(), e) == edges.end()) {
          edges.push_back(e);
        }
      }
    }
  }
  if (edges.empty()) {
    throw std::invalid_argument("the patch has no edges to fill");
  }
  return edges;
}

// Whether two points of the face (by parameter) lie on the surface of
// `reference` within `tolerance`.
bool lies_on(const TopoDS_Face& face, const TopoDS_Face& reference, double tolerance) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const BRepAdaptor_Surface surface(face, false);
  const occ::handle<Geom_Surface> target = BRep_Tool::Surface(reference);
  if (target.IsNull()) {
    return false;
  }
  for (const double s : {0.5, 0.3}) {
    const gp_Pnt p = surface.Value(u1 + (u2 - u1) * s, v1 + (v2 - v1) * (1 - s));
    GeomAPI_ProjectPointOnSurf projection(p, target);
    if (projection.NbPoints() == 0 || projection.LowerDistance() > tolerance) {
      return false;
    }
  }
  return true;
}

// A point of the face in the middle of its parameters and the face's
// normal there, the face's orientation applied; false where the surface
// has no normal.
bool middle_normal(const TopoDS_Face& face, gp_Pnt& point, gp_Vec& normal) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  BRepGProp_Face props(face);
  props.Normal((u1 + u2) / 2, (v1 + v2) / 2, point, normal);
  if (normal.Magnitude() < 1.0e-12) {
    return false;
  }
  normal.Normalize();
  return true;
}

// Whether the face lies `distance` from the surface of `reference` along
// its normal, over the face: the middle of `face` is that far from the
// surface and above the face `reference` itself.
bool lies_over(const TopoDS_Face& face, const TopoDS_Face& reference, double distance, double tolerance) {
  gp_Pnt p;
  gp_Vec n;
  if (!middle_normal(face, p, n)) {
    return false;
  }
  const occ::handle<Geom_Surface> target = BRep_Tool::Surface(reference);
  if (target.IsNull()) {
    return false;
  }
  GeomAPI_ProjectPointOnSurf projection(p, target);
  if (projection.NbPoints() == 0 || std::abs(projection.LowerDistance() - std::abs(distance)) > tolerance) {
    return false;
  }
  double u = 0, v = 0;
  projection.LowerDistanceParameters(u, v);
  const BRepClass_FaceClassifier inside(reference, gp_Pnt2d(u, v), tolerance);
  return inside.State() == TopAbs_IN || inside.State() == TopAbs_ON;
}

// Whether the point lies on the face (within `tolerance` of its surface,
// inside its boundary).
bool contains_point(const TopoDS_Face& face, const gp_Pnt& point, double tolerance) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  if (surface.IsNull()) {
    return false;
  }
  GeomAPI_ProjectPointOnSurf projection(point, surface);
  if (projection.NbPoints() == 0 || projection.LowerDistance() > tolerance) {
    return false;
  }
  double u = 0, v = 0;
  projection.LowerDistanceParameters(u, v);
  const BRepClass_FaceClassifier inside(face, gp_Pnt2d(u, v), tolerance);
  return inside.State() == TopAbs_IN || inside.State() == TopAbs_ON;
}

// The faces of an operation's history from a shape of its input.
std::vector<TopoDS_Shape> faces_from(const NCollection_List<TopoDS_Shape>& shapes) {
  std::vector<TopoDS_Shape> faces;
  for (const TopoDS_Shape& s : shapes) {
    if (s.ShapeType() == TopAbs_FACE) {
      faces.push_back(s);
    }
  }
  return faces;
}

// The sheet as OCCT's offset takes it: its one face or shell as itself, a
// compound of several as it is.
TopoDS_Shape offset_input(const TopoDS_Shape& shape) {
  if (shape.ShapeType() != TopAbs_COMPOUND) {
    return shape;
  }
  TopoDS_Iterator it(shape);
  if (!it.More()) {
    return shape;
  }
  const TopoDS_Shape only = it.Value();
  it.Next();
  return it.More() ? shape : only;
}

// Whether `image`, an offset of `original`, lies `distance` along the
// normal of `original` (against it when negative): the point that far
// from the middle of `original` is on the surface of `image`. OCCT offsets
// along the surfaces' normals, which a face's orientation may reverse.
bool along_normal(const TopoDS_Face& original, const TopoDS_Shape& image, double distance,
                  double tolerance) {
  gp_Pnt p;
  gp_Vec n;
  if (!middle_normal(original, p, n)) {
    return true;
  }
  const occ::handle<Geom_Surface> target = BRep_Tool::Surface(TopoDS::Face(image));
  if (target.IsNull()) {
    return true;
  }
  GeomAPI_ProjectPointOnSurf projection(p.Translated(n * distance), target);
  return projection.NbPoints() > 0 && projection.LowerDistance() <= tolerance;
}

// The images of a face of the input in an offset's result: what the
// history says it generated or became, else the face itself when the
// result has it.
std::vector<TopoDS_Shape> images_of(const BRepTools_History& history, const TopoDS_Face& face,
                                    const detail::FaceNamer& namer) {
  std::vector<TopoDS_Shape> images = faces_from(history.Generated(face));
  for (const TopoDS_Shape& image : faces_from(history.Modified(face))) {
    images.push_back(image);
  }
  if (images.empty() && namer.contains(face)) {
    images.push_back(face);
  }
  return images;
}

// The sheet's faces offset by `distance` along their normals (the faces'
// orientations applied): a sheet whose faces keep the names of the faces
// they come from. Throws when OCCT cannot offset it.
ShapePtr offset_sheet(const Shape& sheet, double distance) {
  const double side_tolerance = 1.0e-3 + 1.0e-3 * std::abs(distance);
  // The side OCCT took is checked on the first face; the offset is made
  // the other way when it took the other.
  for (const double d : {distance, -distance}) {
    // OCCT's offset writes into its input.
    const ShapePtr copy = detail::working_copy(sheet);
    BRepOffset_MakeOffset maker;
    maker.Initialize(offset_input(copy->occt()), d, kTolerance, BRepOffset_Skin, false, false,
                     GeomAbs_Intersection, false);
    detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
    if (!maker.IsDone()) {
      throw std::runtime_error("thicken failed; the surface could not be offset by half the thickness");
    }
    const TopoDS_Shape result = maker.Shape();
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(copy->occt());
    const BRepTools_History history(arguments, maker);
    detail::FaceNamer namer(result);
    bool checked = false;
    bool right_side = true;
    for (int i = 0; i < copy->face_count(); ++i) {
      const TopoDS_Face& original = copy->face(i);
      for (const TopoDS_Shape& image : images_of(history, original, namer)) {
        if (!namer.contains(image)) {
          continue;
        }
        if (!checked) {
          checked = true;
          right_side = along_normal(original, image, distance, side_tolerance);
        }
        if (!copy->face_names(i).empty() && !namer.named(image)) {
          namer.add(image, copy->face_names(i), -1);
        }
      }
    }
    if (right_side) {
      namer.finish();
      return namer.shape();
    }
  }
  throw std::runtime_error("thicken failed; the surface could not be offset by half the thickness");
}

// The solid between the sheet and its offset by `wall` along the faces'
// normals (against them when negative), named as thicken() says.
ShapePtr thick_solid(const std::string& feature, const Shape& sheet, double wall) {
  const double tolerance = std::min(1.0e-4, std::abs(wall) / 4);
  const double side_tolerance = 1.0e-3 + 1.0e-3 * std::abs(wall);
  const std::string near_role = wall > 0 ? "back" : "front";
  const std::string far_role = wall > 0 ? "front" : "back";
  // The side OCCT took is checked on the first far face found; the wall
  // is made the other way when it took the other.
  for (const double d : {wall, -wall}) {
    // OCCT's offset writes into its input.
    const ShapePtr copy = detail::working_copy(sheet);
    BRepOffset_MakeOffset maker;
    maker.Initialize(offset_input(copy->occt()), d, kTolerance, BRepOffset_Skin, false, false,
                     GeomAbs_Intersection, true);
    detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
    if (!maker.IsDone()) {
      throw std::runtime_error("thicken failed; the surface could not be thickened (a face may be "
                               "more tightly curved than the thickness, or the faces meet at an "
                               "inside corner the wall crosses)");
    }
    const TopoDS_Shape result = maker.Shape();
    if (!TopExp_Explorer(result, TopAbs_SOLID).More()) {
      throw std::runtime_error("thicken failed; the thickened surface does not close");
    }
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(copy->occt());
    const BRepTools_History history(arguments, maker);
    detail::FaceNamer namer(result);
    bool checked = false;
    bool right_side = true;
    // A face's images: on its surface its near side, else its far side.
    for (int i = 0; i < copy->face_count(); ++i) {
      const std::string face = detail::primary_name(*copy, i);
      const TopoDS_Face& original = copy->face(i);
      for (const TopoDS_Shape& image : images_of(history, original, namer)) {
        if (!namer.contains(image) || namer.named(image)) {
          continue;
        }
        const bool near_side = lies_on(TopoDS::Face(image), original, tolerance);
        if (!near_side && !checked) {
          checked = true;
          right_side = along_normal(original, image, wall, side_tolerance);
        }
        if (!face.empty()) {
          namer.add(image, face_name(feature, near_side ? near_role : far_role, face));
        }
      }
    }
    // The walls, from the free edges.
    const detail::EdgeFaces edge_faces(*copy);
    for (int e = 0; e < copy->edge_count(); ++e) {
      const std::vector<int> faces = edge_faces.at(e);
      if (faces.size() != 1) {
        continue;
      }
      const std::string face = detail::primary_name(*copy, faces.front());
      for (const TopoDS_Shape& image : faces_from(history.Generated(copy->edge(e)))) {
        if (!face.empty() && namer.contains(image) && !namer.named(image)) {
          namer.add(image, face_name(feature, "wall", face));
        }
      }
    }
    // The middles of the sheet's free edges and the names of their faces.
    std::vector<std::pair<gp_Pnt, std::string>> free_middles;
    for (int e = 0; e < copy->edge_count(); ++e) {
      const std::vector<int> faces = edge_faces.at(e);
      const std::string face = faces.size() == 1 ? detail::primary_name(*copy, faces.front()) : std::string();
      if (!face.empty() && !BRep_Tool::Degenerated(copy->edge(e))) {
        const BRepAdaptor_Curve curve(copy->edge(e));
        free_middles.emplace_back(curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2), face);
      }
    }
    // What the history did not name: a face on a face of the sheet is its
    // near side, one over it at the wall's distance its far side, others
    // walls.
    for (TopExp_Explorer it(result, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& image = TopoDS::Face(it.Current());
      if (namer.named(image)) {
        continue;
      }
      std::string name = face_name(feature, "wall");
      for (int i = 0; i < copy->face_count(); ++i) {
        const std::string face = detail::primary_name(*copy, i);
        if (face.empty()) {
          continue;
        }
        if (lies_on(image, copy->face(i), tolerance)) {
          name = face_name(feature, near_role, face);
          break;
        }
        if (lies_over(image, copy->face(i), wall, side_tolerance)) {
          if (!checked) {
            checked = true;
            right_side = along_normal(copy->face(i), image, wall, side_tolerance);
          }
          name = face_name(feature, far_role, face);
          break;
        }
      }
      if (name == face_name(feature, "wall")) {
        // A wall stands on a free edge of the sheet: the face its middle
        // lies on.
        for (const auto& [middle, face] : free_middles) {
          if (contains_point(image, middle, tolerance)) {
            name = face_name(feature, "wall", face);
            break;
          }
        }
      }
      namer.add(image, name);
    }
    if (!right_side) {
      continue;
    }
    namer.finish();
    ShapePtr thick = namer.shape();
    detail::require_valid(thick->occt(), "thicken");
    return thick;
  }
  throw std::runtime_error("thicken failed; the wall could not be put on the side asked for");
}

} // namespace

std::vector<ShapePtr> patch_regions(const std::string& feature, const Frame& frame,
                                    const std::vector<Region>& regions) {
  if (regions.empty()) {
    throw std::invalid_argument("patch: no regions");
  }
  return detail::run("patch", [&] {
    std::vector<ShapePtr> patches;
    for (const Region& region : regions) {
      const ProfileFace profile = make_profile(frame, region);
      const TopoDS_Shell shell = shell_of(profile.face);
      const std::string face = face_name(feature, "patch", region.name);
      auto patch = std::make_shared<Shape>(shell, std::vector<Shape::NamedFace>{{profile.face, {face}}});
      std::vector<Shape::NamedOpenEdge> sources;
      for (const auto& [edge, segment] : profile.edges) {
        sources.push_back({edge, {make_open_edge_name(face, segment)}});
      }
      patch->set_open_edge_names(sources);
      patches.push_back(std::move(patch));
    }
    return patches;
  });
}

std::vector<OpenBoundary> open_boundaries(const Shape& body) {
  return detail::run("open boundaries", [&] {
    std::vector<OpenBoundary> boundaries;
    const detail::EdgeFaces edge_faces(body);
    for (const std::vector<int>& loop : free_loops(body)) {
      OpenBoundary boundary;
      for (const int e : loop) {
        for (const int f : edge_faces.at(e)) {
          const std::string name = detail::primary_name(body, f);
          if (!name.empty()) {
            boundary.faces.push_back(name);
          }
        }
        sample_edge(body.edge(e), boundary.points);
      }
      std::sort(boundary.faces.begin(), boundary.faces.end());
      boundary.faces.erase(std::unique(boundary.faces.begin(), boundary.faces.end()), boundary.faces.end());
      boundary.closed = closes(body, loop);
      boundaries.push_back(std::move(boundary));
    }
    return boundaries;
  });
}

ShapePtr patch_loop(const std::string& feature, const Shape& body, const PatchLoop& loop) {
  detail::require_surfaces(body.occt(), "patch");
  for (const gp_Pnt& p : loop.points) {
    detail::require_finite("a point of the patch", p.X() + p.Y() + p.Z());
  }
  return detail::run("patch", [&] {
    // The fill and the planar face give the edges they bound curves on
    // their surfaces: they work on a copy.
    const ShapePtr copy = detail::working_copy(body);
    const std::vector<int> edges = loop_edges(*copy, loop);
    if (!closes(*copy, edges)) {
      throw std::invalid_argument("the edges of the patch do not close up into one loop");
    }
    std::vector<Continuity> orders(edges.size(), loop.continuity);
    for (const auto& [name, continuity] : loop.edge_continuity) {
      for (const int e : copy->find_edges(name)) {
        const auto at = std::find(edges.begin(), edges.end(), e);
        if (at == edges.end()) {
          throw std::invalid_argument("edge " + name + " has a continuity of its own but is not an edge of the loop");
        }
        orders[static_cast<std::size_t>(at - edges.begin())] = continuity;
      }
    }
    const bool plain = loop.points.empty() &&
                       std::all_of(orders.begin(), orders.end(), [](Continuity c) { return c == Continuity::G0; });
    NCollection_List<TopoDS_Shape> list;
    for (const int e : edges) {
      list.Append(copy->edge(e));
    }
    BRepBuilderAPI_MakeWire wire;
    wire.Add(list);
    if (!wire.IsDone() || !BRep_Tool::IsClosed(wire.Wire())) {
      throw std::invalid_argument("the edges of the patch do not make one closed loop");
    }
    TopoDS_Face face;
    if (plain) {
      // A loop in a plane is filled by the plane.
      BRepBuilderAPI_MakeFace planar(wire.Wire(), true);
      if (planar.IsDone()) {
        face = planar.Face();
      }
    }
    if (face.IsNull()) {
      const detail::EdgeFaces edge_faces(*copy);
      BRepOffsetAPI_MakeFilling filling(3, 15, 2, false, 1.0e-5, 1.0e-4, 1.0e-2, 0.1, 8, 9);
      for (std::size_t i = 0; i < edges.size(); ++i) {
        const TopoDS_Edge& edge = copy->edge(edges[i]);
        if (orders[i] == Continuity::G0) {
          filling.Add(edge, GeomAbs_C0, true);
          continue;
        }
        const std::vector<int> faces = edge_faces.at(edges[i]);
        if (faces.size() != 1) {
          throw std::invalid_argument("unsupported: g1 and g2 need the one face beside an edge; edge " +
                                      copy->edge_name(edges[i]) + " lies between two faces");
        }
        filling.Add(edge, copy->face(faces.front()), order_of(orders[i]), true);
      }
      for (const gp_Pnt& p : loop.points) {
        filling.Add(p);
      }
      detail::build(filling);
      if (!filling.IsDone()) {
        throw std::runtime_error("the loop could not be filled");
      }
      face = TopoDS::Face(filling.Shape());
    }
    return std::make_shared<Shape>(shell_of(face),
                                   std::vector<Shape::NamedFace>{{face, {face_name(feature, "patch")}}});
  });
}

ShapePtr stitch(const std::vector<const Shape*>& shapes, double tolerance) {
  if (shapes.empty()) {
    throw std::invalid_argument("stitch: nothing to stitch");
  }
  if (!(tolerance >= 0.0) || !std::isfinite(tolerance)) {
    throw std::invalid_argument("stitch: the tolerance must not be negative");
  }
  for (const Shape* shape : shapes) {
    detail::require_surfaces(shape->occt(), "stitch");
  }
  return detail::run("stitch", [&] {
    // The sewing gives the faces it sews new edges: it works on copies,
    // made together so that what the inputs share stays shared.
    std::vector<TopoDS_Shape> inputs;
    for (const Shape* shape : shapes) {
      inputs.push_back(shape->occt());
    }
    const detail::InputCopy copy(inputs);
    BRepBuilderAPI_Sewing sewing(tolerance > 0.0 ? tolerance : kSewing);
    for (std::size_t i = 0; i < inputs.size(); ++i) {
      sewing.Add(copy.shape(i));
    }
    detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
    const TopoDS_Shape sewn = sewing.SewedShape();
    if (sewn.IsNull()) {
      throw std::runtime_error("the faces could not be sewn");
    }
    // Closed shells become solids; open shells and loose faces stay.
    BRep_Builder builder;
    TopoDS_Compound all;
    builder.MakeCompound(all);
    std::vector<TopoDS_Shape> parts;
    for (TopExp_Explorer it(sewn, TopAbs_SHELL); it.More(); it.Next()) {
      const TopoDS_Shell& shell = TopoDS::Shell(it.Current());
      TopoDS_Shape part = shell;
      if (BRep_Tool::IsClosed(shell)) {
        BRepBuilderAPI_MakeSolid make(shell);
        if (make.IsDone()) {
          TopoDS_Solid solid = make.Solid();
          BRepLib::OrientClosedSolid(solid);
          part = solid;
        }
      }
      parts.push_back(part);
    }
    for (TopExp_Explorer it(sewn, TopAbs_FACE, TopAbs_SHELL); it.More(); it.Next()) {
      parts.push_back(it.Current());
    }
    for (const TopoDS_Shape& part : parts) {
      builder.Add(all, part);
    }
    const TopoDS_Shape result = parts.size() == 1 ? parts.front() : TopoDS_Shape(all);
    detail::FaceNamer namer(result);
    // The faces were added as sub-shapes of the inputs: their images are
    // the sewing's modified sub-shapes. A sewn face keeps its surface (the
    // copy shares the input's geometry), which finds the faces the
    // history does not.
    std::map<const Geom_Surface*, const NameList*> by_surface;
    std::map<const Geom_Surface*, int> surface_uses;
    for (std::size_t s = 0; s < shapes.size(); ++s) {
      const Shape& input = *shapes[s];
      for (int f = 0; f < input.face_count(); ++f) {
        TopLoc_Location location;
        const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(input.face(f), location);
        ++surface_uses[surface.get()];
        if (input.face_names(f).empty()) {
          continue;
        }
        by_surface[surface.get()] = &input.face_names(f);
        const TopoDS_Shape face = copy.of(input.face(f));
        const TopoDS_Shape image =
            sewing.IsModifiedSubShape(face) ? sewing.ModifiedSubShape(face) : face;
        if (namer.contains(image) && !namer.named(image)) {
          namer.add(image, input.face_names(f), -1);
        }
      }
    }
    for (TopExp_Explorer it(result, TopAbs_FACE); it.More(); it.Next()) {
      if (namer.named(it.Current())) {
        continue;
      }
      TopLoc_Location location;
      const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(TopoDS::Face(it.Current()), location);
      const auto found = by_surface.find(surface.get());
      if (found != by_surface.end() && surface_uses[surface.get()] == 1) {
        namer.add(it.Current(), *found->second, -1);
      }
    }
    namer.finish();
    return namer.shape();
  });
}

ShapePtr thicken(const std::string& feature, const Shape& sheet, double thickness, bool symmetric) {
  detail::require_finite("the thickness", thickness);
  if (thickness == 0.0) {
    throw std::invalid_argument("thicken: the thickness must not be zero");
  }
  detail::require_surfaces(sheet.occt(), "thicken");
  if (TopExp_Explorer(sheet.occt(), TopAbs_SOLID).More()) {
    throw std::invalid_argument("thicken: the body is a solid; only surface bodies are thickened");
  }
  if (sheet.face_count() == 0) {
    throw std::invalid_argument("thicken: the body has no faces");
  }
  return detail::run("thicken", [&] {
    if (!symmetric) {
      return thick_solid(feature, sheet, thickness);
    }
    // Half the wall behind the faces, then the whole wall from there.
    const double half = std::abs(thickness) / 2;
    const ShapePtr back = offset_sheet(sheet, -half);
    return thick_solid(feature, *back, 2 * half);
  });
}

// The surface modelling features' openings (mitcad#455, free_loops.hpp).
std::vector<std::vector<int>> detail::free_edge_loops(const Shape& body) { return free_loops(body); }

} // namespace mitcad::geometry
