// SPDX-License-Identifier: MIT
#include "mitcad/geometry/surface_offsets.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <map>
#include <memory>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFill.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomConvert.hxx>
#include <GeomLib.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BoundedSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "face_select.hpp"
#include "free_loops.hpp"
#include "history.hpp"
#include "mitcad/geometry/surfaces.hpp"
#include "sheet_edges.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTolerance = 1.0e-6;
// Faces sewn after a ruled strip or an extension: their edges meet within
// this.
constexpr double kSewing = 1.0e-4;

// The faces' images in an operation's history.
std::vector<TopoDS_Shape> faces_from(const NCollection_List<TopoDS_Shape>& shapes) {
  std::vector<TopoDS_Shape> faces;
  for (const TopoDS_Shape& s : shapes) {
    if (s.ShapeType() == TopAbs_FACE) {
      faces.push_back(s);
    }
  }
  return faces;
}

// A point in the middle of the face's parameters and the unit normal
// there, the face's orientation applied; false where there is none.
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

// Whether `image` lies `distance` along the normal of `original` (the
// point that far from its middle is on the image's surface).
bool along_normal(const TopoDS_Face& original, const TopoDS_Shape& image, double distance, double tolerance) {
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

// --- Ruled Surface

// An edge of the chain and the face it leaves.
struct ChainEdge {
  int edge = -1;
  int face = -1;
};

// The edge indices in chains (each in order; a closed loop from any edge),
// with the vertex each one ends at as its chain runs. Throws where they
// branch.
std::vector<std::vector<std::pair<int, TopoDS_Vertex>>> chained(const Shape& body, const std::vector<int>& edges) {
  ShapeMap vertices;
  std::map<int, std::vector<int>> at; // vertex -> edges there
  std::map<int, std::array<int, 2>> ends;
  for (const int e : edges) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(body.edge(e), a, b);
    if (a.IsNull() || b.IsNull()) {
      throw std::invalid_argument("ruled surface: an edge has no ends");
    }
    const int ia = vertices.Add(a);
    const int ib = vertices.Add(b);
    ends[e] = {ia, ib};
    at[ia].push_back(e);
    if (ib != ia) {
      at[ib].push_back(e);
    }
  }
  for (const auto& [vertex, list] : at) {
    if (list.size() > 2) {
      throw std::invalid_argument("ruled surface: the edges branch; they must join into chains");
    }
  }
  std::vector<std::vector<std::pair<int, TopoDS_Vertex>>> chains;
  std::set<int> used;
  while (used.size() < edges.size()) {
    // Start at an end of an open chain, else anywhere.
    int start = -1;
    int from = -1;
    for (const auto& [vertex, list] : at) {
      if (list.size() == 1 && !used.count(list.front())) {
        start = list.front();
        from = vertex;
        break;
      }
    }
    if (start < 0) {
      for (const int e : edges) {
        if (!used.count(e)) {
          start = e;
          from = ends[e][0];
          break;
        }
      }
    }
    std::vector<std::pair<int, TopoDS_Vertex>> order;
    int current = start;
    int vertex = from;
    while (current >= 0 && !used.count(current)) {
      used.insert(current);
      const std::array<int, 2> e = ends[current];
      vertex = e[0] == vertex ? e[1] : e[0];
      order.emplace_back(current, TopoDS::Vertex(vertices.FindKey(vertex)));
      int next = -1;
      for (const int candidate : at[vertex]) {
        if (!used.count(candidate)) {
          next = candidate;
        }
      }
      current = next;
    }
    chains.push_back(std::move(order));
  }
  return chains;
}

// The direction a strip leaves the edge at curve parameter t.
gp_Dir strip_direction(const RuledSpec& spec, const TopoDS_Edge& edge, const TopoDS_Face& face, double t) {
  const BRepAdaptor_Curve curve(edge);
  gp_Pnt p;
  gp_Vec tangent;
  curve.D1(t, p, tangent);
  if (tangent.Magnitude() < 1.0e-12) {
    throw std::runtime_error("ruled surface: an edge has no tangent");
  }
  tangent.Normalize();
  // The face's continuation away from it: across the edge in the face's
  // parameters, to the side where the face is not.
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    throw std::runtime_error("ruled surface: an edge has no curve on its face");
  }
  gp_Pnt2d uv;
  gp_Vec2d duv;
  pcurve->D1(t, uv, duv);
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const double step = 1.0e-4 * std::max(u2 - u1, v2 - v1);
  gp_Vec2d across(-duv.Y(), duv.X());
  if (across.Magnitude() < 1.0e-15) {
    throw std::runtime_error("ruled surface: an edge has no direction on its face");
  }
  across.Normalize();
  // Which side the face is on, found in the middle of the edge (at an end
  // a step across can land on the next edge).
  gp_Pnt2d mid;
  gp_Vec2d dmid;
  pcurve->D1((first + last) / 2, mid, dmid);
  gp_Vec2d side(-dmid.Y(), dmid.X());
  if (side.Magnitude() > 1.0e-15) {
    side.Normalize();
    const gp_Pnt2d left(mid.X() + side.X() * step, mid.Y() + side.Y() * step);
    if (BRepClass_FaceClassifier(face, left, kTolerance).State() == TopAbs_IN) {
      across.Reverse();
    }
  }
  const BRepAdaptor_Surface surface(face);
  gp_Pnt on;
  gp_Vec du;
  gp_Vec dv;
  surface.D1(uv.X(), uv.Y(), on, du, dv);
  gp_Vec outward = du * across.X() + dv * across.Y();
  outward -= tangent * outward.Dot(tangent);
  if (outward.Magnitude() < 1.0e-12) {
    throw std::runtime_error("ruled surface: the face has no direction across the edge");
  }
  outward.Normalize();
  gp_Vec normal = du.Crossed(dv);
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  normal.Normalize();
  gp_Vec d;
  switch (spec.mode) {
  case RuledMode::Normal:
    d = normal;
    break;
  case RuledMode::Tangent:
    d = outward;
    break;
  case RuledMode::Direction:
    d = gp_Vec(*spec.direction);
    break;
  case RuledMode::Angle: {
    gp_Vec reference = spec.direction ? gp_Vec(*spec.direction) : normal;
    reference -= tangent * reference.Dot(tangent);
    if (reference.Magnitude() < 1.0e-12) {
      throw std::invalid_argument("ruled surface: the angle's reference runs along an edge");
    }
    reference.Normalize();
    gp_Vec turned = tangent.Crossed(reference);
    if (turned.Dot(outward) < 0.0) {
      turned.Reverse();
    }
    d = reference * std::cos(spec.angle) + turned * std::sin(spec.angle);
    break;
  }
  }
  if (spec.flip) {
    d.Reverse();
  }
  return gp_Dir(d);
}

// The strip off an edge: its points moved by `distance` along the
// directions, the ends' directions `start` and `end` (their lengths
// scale the distance there).
TopoDS_Face strip_face(const RuledSpec& spec, const TopoDS_Edge& edge, const TopoDS_Face& face, const gp_Vec& start,
                       const gp_Vec& end) {
  const BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  const bool straight = curve.GetType() == GeomAbs_Line;
  const int samples = straight ? 1 : 24;
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, samples + 1);
  for (int i = 0; i <= samples; ++i) {
    const double t = first + (last - first) * i / samples;
    gp_Vec d;
    if (i == 0) {
      d = start;
    } else if (i == samples) {
      d = end;
    } else {
      d = gp_Vec(strip_direction(spec, edge, face, t));
    }
    points->SetValue(i + 1, curve.Value(t).Translated(d * spec.distance));
  }
  TopoDS_Edge far_edge;
  if (straight) {
    far_edge = BRepBuilderAPI_MakeEdge(points->Value(1), points->Value(2)).Edge();
  } else {
    GeomAPI_Interpolate interpolate(points, false, kTolerance);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error("ruled surface: the strip's far edge could not be fitted");
    }
    far_edge = BRepBuilderAPI_MakeEdge(interpolate.Curve()).Edge();
  }
  // The near edge as the curve runs, without its orientation in a wire.
  TopoDS_Edge near_edge = TopoDS::Edge(edge.Oriented(TopAbs_FORWARD));
  const TopoDS_Face strip = BRepFill::Face(near_edge, far_edge);
  if (strip.IsNull()) {
    throw std::runtime_error("ruled surface: a strip could not be built");
  }
  return strip;
}

// --- Extend

// The free edges of openings, as edge indices of the body.
std::vector<int> opening_edges(const Shape& body, const std::vector<OpeningEdges>& openings) {
  std::vector<int> edges;
  if (openings.empty()) {
    return edges;
  }
  const std::vector<std::vector<int>> loops = detail::free_edge_loops(body);
  const detail::EdgeFaces edge_faces(body);
  for (const OpeningEdges& opening : openings) {
    if (opening.index < 0 || static_cast<std::size_t>(opening.index) >= loops.size()) {
      throw std::invalid_argument("the body has no opening " + std::to_string(opening.index) + " (it has " +
                                  std::to_string(loops.size()) + ")");
    }
    std::vector<int> wanted;
    for (const int e : loops[static_cast<std::size_t>(opening.index)]) {
      const std::vector<int> faces = edge_faces.at(e);
      if (opening.faces.empty() ||
          std::any_of(faces.begin(), faces.end(), [&](int f) {
            return std::find(opening.faces.begin(), opening.faces.end(), detail::primary_name(body, f)) !=
                   opening.faces.end();
          })) {
        wanted.push_back(e);
      }
    }
    if (!opening.at.empty()) {
      // The edge nearest each point.
      std::vector<int> nearest;
      for (const gp_Pnt& point : opening.at) {
        const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
        int best = -1;
        double distance = 0.0;
        for (const int e : wanted) {
          BRepExtrema_DistShapeShape measure(vertex, body.edge(e));
          if (measure.IsDone() && (best < 0 || measure.Value() < distance)) {
            best = e;
            distance = measure.Value();
          }
        }
        if (best >= 0 && std::find(nearest.begin(), nearest.end(), best) == nearest.end()) {
          nearest.push_back(best);
        }
      }
      wanted = nearest;
    }
    for (const int e : wanted) {
      if (std::find(edges.begin(), edges.end(), e) == edges.end()) {
        edges.push_back(e);
      }
    }
  }
  return edges;
}

// The named edges and the edges of the openings, each once.
std::vector<int> picked_edges(const Shape& body, const std::vector<std::string>& names,
                              const std::vector<OpeningEdges>& openings) {
  std::vector<int> edges;
  if (!names.empty()) {
    edges = detail::resolve_edges(body, names);
  }
  for (const int e : opening_edges(body, openings)) {
    if (std::find(edges.begin(), edges.end(), e) == edges.end()) {
      edges.push_back(e);
    }
  }
  if (edges.empty()) {
    throw std::invalid_argument("no edges selected");
  }
  return edges;
}

enum Side { UMin = 0, UMax = 1, VMin = 2, VMax = 3 };

// The side of the face's parameter range the edge runs along, or -1.
int side_of(const TopoDS_Edge& edge, const TopoDS_Face& face) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    return -1;
  }
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const double tu = 1.0e-7 * std::max(1.0, u2 - u1);
  const double tv = 1.0e-7 * std::max(1.0, v2 - v1);
  std::array<bool, 4> on{true, true, true, true};
  for (int i = 0; i <= 4; ++i) {
    const gp_Pnt2d uv = pcurve->Value(first + (last - first) * i / 4);
    on[UMin] = on[UMin] && std::abs(uv.X() - u1) <= tu;
    on[UMax] = on[UMax] && std::abs(uv.X() - u2) <= tu;
    on[VMin] = on[VMin] && std::abs(uv.Y() - v1) <= tv;
    on[VMax] = on[VMax] && std::abs(uv.Y() - v2) <= tv;
  }
  for (int s = 0; s < 4; ++s) {
    if (on[static_cast<std::size_t>(s)]) {
      return s;
    }
  }
  return -1;
}

double face_area(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

// The face extended past the sides by `distance` along its surface.
TopoDS_Face extended_face(const TopoDS_Face& face, const std::array<bool, 4>& sides, double distance,
                          ExtendMode mode) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  if (surface.IsNull()) {
    throw std::invalid_argument("extend: a face has no surface");
  }
  // Only a face bounded by its parameter range: what lies outside its
  // trims would come back.
  int wires = 0;
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    ++wires;
  }
  const BRepBuilderAPI_MakeFace whole(surface, u1, u2, v1, v2, kTolerance);
  if (wires != 1 || !whole.IsDone() ||
      std::abs(face_area(whole.Face()) - face_area(face)) > 1.0e-6 * std::max(1.0, face_area(face))) {
    throw std::invalid_argument("unsupported: extend of a face that is trimmed inside its surface's "
                                "parameter range");
  }
  TopoDS_Face result;
  if (mode == ExtendMode::Natural && !surface->IsKind(STANDARD_TYPE(Geom_BSplineSurface))) {
    // The surface continued: the parameter range widened by the distance
    // over the speed of the parameter at each side's middle.
    const BRepAdaptor_Surface adaptor(face, false);
    std::array<double, 4> range{u1, u2, v1, v2};
    for (int s = 0; s < 4; ++s) {
      if (!sides[static_cast<std::size_t>(s)]) {
        continue;
      }
      const bool inU = s == UMin || s == UMax;
      const double u = s == UMin ? u1 : s == UMax ? u2 : (u1 + u2) / 2;
      const double v = s == VMin ? v1 : s == VMax ? v2 : (v1 + v2) / 2;
      gp_Pnt p;
      gp_Vec du;
      gp_Vec dv;
      adaptor.D1(u, v, p, du, dv);
      const double speed = (inU ? du : dv).Magnitude();
      if (speed < 1.0e-12) {
        throw std::invalid_argument("extend: the surface does not move across a side");
      }
      const double delta = distance / speed;
      range[static_cast<std::size_t>(s)] += (s == UMin || s == VMin) ? -delta : delta;
    }
    if ((surface->IsUPeriodic() && range[UMax] - range[UMin] > surface->UPeriod() + 1.0e-9) ||
        (surface->IsVPeriodic() && range[VMax] - range[VMin] > surface->VPeriod() + 1.0e-9)) {
      throw std::invalid_argument("extend: the extension would run round the surface onto itself");
    }
    const BRepBuilderAPI_MakeFace extended(surface, range[UMin], range[UMax], range[VMin], range[VMax], kTolerance);
    if (!extended.IsDone()) {
      throw std::runtime_error("extend: the surface could not be continued");
    }
    result = extended.Face();
  } else {
    // A B-spline of the face, extended by GeomLib (natural: the
    // polynomials continued, of the most derivatives GeomLib keeps).
    occ::handle<Geom_BoundedSurface> bounded =
        GeomConvert::SurfaceToBSplineSurface(new Geom_RectangularTrimmedSurface(surface, u1, u2, v1, v2));
    const int continuity = mode == ExtendMode::Tangent ? 1 : mode == ExtendMode::Curvature ? 2 : 3;
    for (int s = 0; s < 4; ++s) {
      if (sides[static_cast<std::size_t>(s)]) {
        GeomLib::ExtendSurfByLength(bounded, distance, continuity, s == UMin || s == UMax, s == UMax || s == VMax);
      }
    }
    const BRepBuilderAPI_MakeFace extended(bounded, kTolerance);
    if (!extended.IsDone()) {
      throw std::runtime_error("extend: the surface could not be extended");
    }
    result = extended.Face();
  }
  result.Location(location);
  if (face.Orientation() == TopAbs_REVERSED) {
    result.Reverse();
  }
  return result;
}

} // namespace

ShapePtr offset_surface(const std::string& feature, const Shape& body, const std::vector<std::string>& faces,
                        double distance, bool chain) {
  detail::require_finite("the offset distance", distance);
  detail::require_surfaces(body.occt(), "offset surface");
  return detail::run("offset surface", [&] {
    std::vector<int> picked;
    if (faces.empty()) {
      for (int i = 0; i < body.face_count(); ++i) {
        picked.push_back(i);
      }
    } else {
      picked = detail::resolve_faces(body, faces);
    }
    if (chain) {
      picked = detail::tangent_closure(body, picked);
    }
    if (picked.empty()) {
      throw std::invalid_argument("offset surface: the body has no faces");
    }
    const double tolerance = 1.0e-3 + 1.0e-3 * std::abs(distance);
    for (const double d : {distance, -distance}) {
      // A shell of copies of the faces (OCCT's offset writes into them).
      const ShapePtr copy = detail::working_copy(body);
      BRep_Builder builder;
      TopoDS_Shell shell;
      builder.MakeShell(shell);
      for (const int i : picked) {
        builder.Add(shell, copy->face(i));
      }
      if (distance == 0.0) {
        std::vector<Shape::NamedFace> named;
        for (const int i : picked) {
          const std::string face = detail::primary_name(body, i);
          if (!face.empty()) {
            named.push_back({copy->face(i), {face_name(feature, "offset", face)}});
          }
        }
        return std::make_shared<Shape>(shell, named);
      }
      BRepOffset_MakeOffset maker;
      maker.Initialize(shell, d, kTolerance, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
      detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
      if (!maker.IsDone()) {
        throw std::runtime_error("offset surface: the faces could not be offset (a face may be more tightly "
                                 "curved than the distance)");
      }
      NCollection_List<TopoDS_Shape> arguments;
      arguments.Append(shell);
      const BRepTools_History history(arguments, maker);
      detail::FaceNamer namer(maker.Shape());
      bool checked = false;
      bool right_side = true;
      for (const int i : picked) {
        const std::string face = detail::primary_name(body, i);
        std::vector<TopoDS_Shape> images = faces_from(history.Generated(copy->face(i)));
        for (const TopoDS_Shape& image : faces_from(history.Modified(copy->face(i)))) {
          images.push_back(image);
        }
        for (const TopoDS_Shape& image : images) {
          if (!namer.contains(image)) {
            continue;
          }
          if (!checked) {
            checked = true;
            right_side = along_normal(copy->face(i), image, distance, tolerance);
          }
          if (!face.empty() && !namer.named(image)) {
            namer.add(image, face_name(feature, "offset", face));
          }
        }
      }
      if (right_side) {
        namer.finish();
        return namer.shape();
      }
    }
    throw std::runtime_error("offset surface: the faces could not be offset to the side asked for");
  });
}

ShapePtr ruled_surface(const Shape& body, const RuledSpec& spec) {
  detail::require_positive("the ruled surface's distance", spec.distance);
  detail::require_finite("the ruled surface's angle", spec.angle);
  if (spec.mode == RuledMode::Direction && !spec.direction) {
    throw std::invalid_argument("ruled surface: the direction mode needs a direction");
  }
  detail::require_surfaces(body.occt(), "ruled surface");
  return detail::run("ruled surface", [&] {
    // The strips are built on a copy: BRepFill gives the edges they use
    // curves on the strips.
    const ShapePtr copy = detail::working_copy(body);
    const std::vector<std::vector<std::pair<int, TopoDS_Vertex>>> chains =
        chained(*copy, picked_edges(*copy, spec.edges, spec.openings));
    const detail::EdgeFaces edge_faces(*copy);
    int beside = -1;
    if (!spec.face.empty()) {
      const std::vector<int> found = detail::resolve_faces(*copy, {spec.face}, "face");
      beside = found.front();
    }
    std::vector<std::pair<TopoDS_Shape, std::string>> named;
    // Each chain of its own: its strips and corners.
    for (const std::vector<std::pair<int, TopoDS_Vertex>>& order : chains) {
      std::vector<ChainEdge> chain;
      for (const auto& link : order) {
        const int e = link.first;
        std::vector<int> faces = edge_faces.at(e);
        faces.erase(std::unique(faces.begin(), faces.end()), faces.end());
        ChainEdge entry{e, -1};
        if (beside >= 0 && std::find(faces.begin(), faces.end(), beside) != faces.end()) {
          entry.face = beside;
        } else if (faces.size() == 1) {
          entry.face = faces.front();
        } else {
          const std::string why = " lies between two faces; give the face beside the chain";
          throw std::invalid_argument(edge_error(copy->edge_name(e), "ruled surface: an edge" + why,
                                                 "ruled surface: edge " + copy->edge_name(e) + why));
        }
        chain.push_back(entry);
      }
      // Each edge's directions at its two ends (curve parameters first and
      // last), then mitred where neighbours meet at a sharp corner.
      struct Ends {
        gp_Vec first;
        gp_Vec last;
      };
      std::vector<Ends> ends;
      for (const ChainEdge& c : chain) {
        const TopoDS_Edge& edge = copy->edge(c.edge);
        const TopoDS_Face& face = copy->face(c.face);
        const BRepAdaptor_Curve curve(edge);
        ends.push_back({gp_Vec(strip_direction(spec, edge, face, curve.FirstParameter())),
                        gp_Vec(strip_direction(spec, edge, face, curve.LastParameter()))});
      }
      // The end of edge i at vertex v: its first parameter's or its last's.
      const auto at_vertex = [&](int i, const TopoDS_Vertex& v, bool leaving) -> gp_Vec& {
        TopoDS_Vertex a;
        TopoDS_Vertex b;
        TopExp::Vertices(TopoDS::Edge(copy->edge(chain[static_cast<std::size_t>(i)].edge).Oriented(TopAbs_FORWARD)),
                         a, b);
        Ends& e = ends[static_cast<std::size_t>(i)];
        if (a.IsSame(b)) {
          return leaving ? e.first : e.last; // a closed edge
        }
        return a.IsSame(v) ? e.first : e.last;
      };
      const int count = static_cast<int>(chain.size());
      // A closed chain ends where it starts.
      TopoDS_Vertex a0;
      TopoDS_Vertex b0;
      TopExp::Vertices(copy->edge(chain.front().edge), a0, b0);
      const TopoDS_Vertex& last_end = order.back().second;
      const TopoDS_Vertex start = a0.IsSame(order.front().second) ? b0 : a0;
      const bool closed = count > 1 && last_end.IsSame(start);
      for (int i = 0; i + 1 < count || (closed && i < count); ++i) {
        const int j = (i + 1) % count;
        // The vertex edge i ends at and edge j starts from.
        const TopoDS_Vertex& v = order[static_cast<std::size_t>(i)].second;
        gp_Vec& d1 = at_vertex(i, v, false);
        gp_Vec& d2 = at_vertex(j, v, true);
        const double cosine = std::clamp(gp_Dir(d1).Dot(gp_Dir(d2)), -1.0, 1.0);
        if (cosine > 1.0 - 1.0e-9) {
          continue;
        }
        if (cosine < -1.0 + 1.0e-6) {
          throw std::invalid_argument("ruled surface: the strips turn back on each other at a corner");
        }
        if (spec.corners == RuledCorners::Mitre) {
          const gp_Vec mitre = (d1 + d2) / (1.0 + cosine);
          d1 = mitre;
          d2 = mitre;
        } else {
          // A sector turned about the corner from one strip's end to the
          // other's.
          const gp_Pnt corner = BRep_Tool::Pnt(v);
          const gp_Vec axis = d1.Crossed(d2);
          const TopoDS_Edge radius =
              BRepBuilderAPI_MakeEdge(corner, corner.Translated(gp_Dir(d1).XYZ() * spec.distance)).Edge();
          BRepPrimAPI_MakeRevol sector(radius, gp_Ax1(corner, gp_Dir(axis)), std::acos(cosine));
          if (!sector.IsDone()) {
            throw std::runtime_error("ruled surface: a round corner could not be built");
          }
          for (TopExp_Explorer f(sector.Shape(), TopAbs_FACE); f.More(); f.Next()) {
            const std::string vertex = copy->vertex_name(copy->vertex_index(v));
            named.emplace_back(f.Current(), face_name(spec.feature, "corner", vertex));
          }
        }
      }
      for (int i = 0; i < count; ++i) {
        const ChainEdge& c = chain[static_cast<std::size_t>(i)];
        const Ends& e = ends[static_cast<std::size_t>(i)];
        const TopoDS_Face strip = strip_face(spec, copy->edge(c.edge), copy->face(c.face), e.first, e.last);
        // A free edge has no name: the strip is named after its face.
        const std::string& edge = copy->edge_name(c.edge);
        named.emplace_back(strip, face_name(spec.feature, "ruled",
                                            edge.empty() ? detail::primary_name(*copy, c.face) : edge));
      }
    }
    // One shape of the faces with their names, sewn.
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> faces;
    for (const auto& [face, name] : named) {
      parts.push_back(face);
      faces.push_back({face, {name}});
    }
    const Shape pieces(detail::compound_or_single(parts), faces);
    ShapePtr sewn = stitch({&pieces}, kSewing);
    return sewn;
  });
}

ShapePtr extend_surface(const Shape& body, const std::vector<std::string>& edges,
                        const std::vector<OpeningEdges>& openings, double distance, ExtendMode mode) {
  detail::require_positive("the extension", distance);
  detail::require_surfaces(body.occt(), "extend");
  if (TopExp_Explorer(body.occt(), TopAbs_SOLID).More()) {
    throw std::invalid_argument("extend: the body is a solid; only surface bodies are extended");
  }
  return detail::run("extend", [&] {
    const detail::EdgeFaces edge_faces(body);
    std::map<int, std::array<bool, 4>> sides; // face -> sides to extend
    for (const int e : picked_edges(body, edges, openings)) {
      const std::vector<int> faces = edge_faces.at(e);
      if (faces.size() != 1 || BRep_Tool::IsClosed(body.edge(e), body.face(faces.front()))) {
        const std::string why = " is not on the surface's open boundary";
        throw std::invalid_argument(
            edge_error(body.edge_name(e), "extend: an edge" + why, "extend: edge " + body.edge_name(e) + why));
      }
      const int side = side_of(body.edge(e), body.face(faces.front()));
      if (side < 0) {
        const std::string why = ", which does not run along a side of its face's parameter range";
        throw std::invalid_argument(edge_error(body.edge_name(e), "unsupported: extend of an edge" + why,
                                               "unsupported: extend of edge " + body.edge_name(e) + why));
      }
      sides[faces.front()][static_cast<std::size_t>(side)] = true;
    }
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> named;
    for (int f = 0; f < body.face_count(); ++f) {
      const auto found = sides.find(f);
      const TopoDS_Shape face =
          found == sides.end() ? TopoDS_Shape(body.face(f)) : TopoDS_Shape(extended_face(body.face(f), found->second, distance, mode));
      parts.push_back(face);
      named.push_back({face, body.face_names(f)});
    }
    const Shape faces(detail::compound_or_single(parts), named);
    return stitch({&faces}, kSewing);
  });
}

std::vector<int> detail::sheet_edges(const Shape& body, const std::vector<std::string>& names,
                                     const std::vector<OpeningEdges>& openings) {
  return picked_edges(body, names, openings);
}

gp_Dir detail::across_edge(const TopoDS_Edge& edge, const TopoDS_Face& face, double t) {
  RuledSpec tangent;
  tangent.mode = RuledMode::Tangent;
  return strip_direction(tangent, edge, face, t);
}

} // namespace mitcad::geometry
