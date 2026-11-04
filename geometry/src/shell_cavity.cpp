// SPDX-License-Identifier: MIT
#include "shell_cavity.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <limits>
#include <optional>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BOPAlgo_Alerts.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomLProp_SLProps.hxx>
#include <Geom_Surface.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Lin.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/naming.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kTolerance = 1.0e-6;
// Where a ray from a face starts to count (past the face itself).
constexpr double kRayStart = 1.0e-4;
// Samples per face along each parameter.
constexpr int kSamples = 5;

using History = occ::handle<BRepTools_History>;

// A point inside a face and the face's normal there, out of the material.
struct Sample {
  gp_Pnt point;
  gp_Dir normal;
};

std::vector<Sample> samples(const TopoDS_Face& face) {
  std::vector<Sample> out;
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const BRepAdaptor_Surface surface(face, false);
  const double tolerance = BRep_Tool::Tolerance(face);
  BRepClass_FaceClassifier classifier;
  for (int i = 0; i < kSamples; ++i) {
    for (int j = 0; j < kSamples; ++j) {
      const double u = u1 + (u2 - u1) * (i + 0.5) / kSamples;
      const double v = v1 + (v2 - v1) * (j + 0.5) / kSamples;
      classifier.Perform(face, gp_Pnt2d(u, v), tolerance);
      if (classifier.State() != TopAbs_IN) {
        continue;
      }
      gp_Pnt point;
      gp_Vec du, dv;
      surface.D1(u, v, point, du, dv);
      const gp_Vec normal = du.Crossed(dv);
      if (normal.Magnitude() < 1.0e-12) {
        continue;
      }
      gp_Dir out_of_material(normal);
      if (face.Orientation() == TopAbs_REVERSED) {
        out_of_material.Reverse();
      }
      out.push_back({point, out_of_material});
    }
  }
  return out;
}

// The faces a ray into a body's material meets first.
class Rays {
public:
  explicit Rays(const Shape& body) : m_body(body) { m_hits.Load(body.occt(), 1.0e-7); }

  // The face (index into the body) the ray from the sample into the
  // material meets first within `reach`, and how far it is.
  std::optional<std::pair<int, double>> first(const Sample& sample, double reach) {
    m_hits.Perform(gp_Lin(sample.point, sample.normal.Reversed()), kRayStart, reach);
    std::optional<std::pair<int, double>> nearest;
    for (int i = 1; i <= m_hits.NbPnt(); ++i) {
      const double distance = m_hits.WParameter(i);
      const int face = m_body.face_index(m_hits.Face(i));
      if (face >= 0 && distance > kRayStart && (!nearest || distance < nearest->second)) {
        nearest = std::make_pair(face, distance);
      }
    }
    return nearest;
  }

private:
  const Shape& m_body;
  IntCurvesFace_ShapeIntersector m_hits;
};

// How thin the body is under its faces, measured along their normals.
struct Thinness {
  // Kept planar faces facing each other no further apart than twice the
  // wall: the plates between them stay solid.
  std::vector<std::pair<int, int>> pairs;
  // Other kept faces no further than twice the wall from a kept face across
  // the body under at least half of them (curved or tapered parts, mitcad#1237):
  // the parts they belong to stay solid.
  std::set<int> thin;
  // Kept faces no further from a removed face than the wall everywhere:
  // their offset lies past it.
  std::set<int> under_removed;
  // How far the removed faces go out: past every kept face's offset.
  double push = 0.0;
};

Thinness thinness(const Shape& body, const std::set<int>& removed, double thickness) {
  Thinness thin;
  const double margin = 0.25 * thickness;
  thin.push = margin;
  Rays rays(body);
  for (int f = 0; f < body.face_count(); ++f) {
    if (removed.count(f) > 0) {
      continue;
    }
    const std::optional<gp_Pln> plane = face_plane(body.face(f));
    bool all_under_removed = true;
    int count = 0;
    int across = 0;
    for (const Sample& sample : samples(body.face(f))) {
      ++count;
      const auto hit = rays.first(sample, 2 * thickness + 10 * kTolerance);
      if (!hit) {
        all_under_removed = false;
        continue;
      }
      const auto [g, distance] = *hit;
      if (removed.count(g) > 0) {
        if (distance <= thickness + kTolerance) {
          thin.push = std::max(thin.push, thickness + margin - distance + margin);
        } else {
          all_under_removed = false;
        }
        continue;
      }
      all_under_removed = false;
      const std::optional<gp_Pln> other = plane && g != f ? face_plane(body.face(g)) : std::nullopt;
      if (other && plane->Axis().Direction().Dot(other->Axis().Direction()) < -1.0 + 1.0e-9) {
        const std::pair<int, int> pair = std::minmax(f, g);
        if (std::find(thin.pairs.begin(), thin.pairs.end(), pair) == thin.pairs.end()) {
          thin.pairs.push_back(pair);
        }
      } else {
        ++across;
      }
    }
    if (count > 0 && all_under_removed) {
      thin.under_removed.insert(f);
    }
    if (count > 0 && 2 * across >= count) {
      thin.thin.insert(f);
    }
  }
  return thin;
}

// The face's normal out of the material where the edge runs along it, at
// the edge's middle.
std::optional<gp_Dir> normal_along(const TopoDS_Face& face, const TopoDS_Edge& edge) {
  double first = 0, last = 0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    return std::nullopt;
  }
  const gp_Pnt2d uv = pcurve->Value((first + last) / 2);
  const BRepAdaptor_Surface surface(face, false);
  gp_Pnt point;
  gp_Vec du, dv;
  surface.D1(uv.X(), uv.Y(), point, du, dv);
  const gp_Vec normal = du.Crossed(dv);
  if (normal.Magnitude() < 1.0e-12) {
    return std::nullopt;
  }
  gp_Dir out_of_material(normal);
  if (face.Orientation() == TopAbs_REVERSED) {
    out_of_material.Reverse();
  }
  return out_of_material;
}

// How two faces meet along an edge.
enum class Crease { Convex, Concave, Smooth };

// A point just off the edge, out of the first face and into the second's
// side: outside the body where the edge is convex, inside where it is
// concave.
Crease crease(BRepClass3d_SolidClassifier& body, const TopoDS_Edge& edge, const TopoDS_Face& a,
              const TopoDS_Face& b) {
  const std::optional<gp_Dir> na = normal_along(a, edge);
  const std::optional<gp_Dir> nb = normal_along(b, edge);
  if (!na || !nb || na->Angle(*nb) < 1.0e-2) {
    return Crease::Smooth;
  }
  const BRepAdaptor_Curve curve(edge);
  const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2);
  constexpr double kProbe = 1.0e-3;
  body.Perform(middle.Translated((gp_Vec(*na) - gp_Vec(*nb)) * kProbe), 1.0e-7);
  switch (body.State()) {
  case TopAbs_IN:
    return Crease::Concave;
  case TopAbs_OUT:
    return Crease::Convex;
  default:
    return Crease::Smooth;
  }
}

bool tighter_than(const TopoDS_Face& face, double radius);

// The parts of the body the thin faces belong to: each grows from a thin
// face across the edges where it turns away from its neighbour (convex
// edges, and smooth ones into convex roundings) and into the other thin
// faces, not across the concave edges where the part stands on the rest of
// the body. A part that reaches a removed face, or would take every kept
// face, is left out.
std::vector<std::vector<int>> thin_parts(const Shape& body, const std::set<int>& thin,
                                         const std::set<int>& removed) {
  std::vector<std::vector<int>> parts;
  if (thin.empty()) {
    return parts;
  }
  const EdgeFaces edge_faces(body);
  BRepClass3d_SolidClassifier classifier(body.occt());
  std::set<int> grouped;
  for (const int start : thin) {
    if (grouped.count(start) > 0) {
      continue;
    }
    std::vector<int> part{start};
    std::set<int> in{start};
    bool open = false;
    for (std::size_t i = 0; i < part.size(); ++i) {
      const int f = part[i];
      for (TopExp_Explorer e(body.face(f), TopAbs_EDGE); e.More(); e.Next()) {
        const int edge = body.edge_index(e.Current());
        if (edge < 0) {
          continue;
        }
        for (const int g : edge_faces.at(edge)) {
          if (g == f || in.count(g) > 0) {
            continue;
          }
          const Crease kind = crease(classifier, TopoDS::Edge(e.Current()), body.face(f), body.face(g));
          if (thin.count(g) > 0 || kind == Crease::Convex ||
              (kind == Crease::Smooth &&
               tighter_than(body.face(g), std::numeric_limits<double>::infinity()))) {
            in.insert(g);
            part.push_back(g);
            open = open || removed.count(g) > 0;
          }
        }
      }
    }
    grouped.insert(in.begin(), in.end());
    if (!open && part.size() + removed.size() < static_cast<std::size_t>(body.face_count())) {
      parts.push_back(part);
    }
  }
  return parts;
}

// The planar face swept into the material by `depth`.
TopoDS_Shape swept_in(const TopoDS_Face& face, double depth) {
  const gp_Pln plane = *face_plane(face);
  BRepPrimAPI_MakePrism prism(face, gp_Vec(plane.Axis().Direction()) * -depth, true);
  return prism.Shape();
}

// The plates between the pairs of faces: where the faces, swept into the
// material as far as the other one, overlap.
std::vector<TopoDS_Shape> plates(const Shape& body, const std::vector<std::pair<int, int>>& pairs,
                                 double thickness) {
  std::vector<TopoDS_Shape> out;
  for (const auto& [f, g] : pairs) {
    const gp_Pln a = *face_plane(body.face(f));
    const gp_Pln b = *face_plane(body.face(g));
    const double gap =
        std::abs(gp_Vec(a.Location(), b.Location()).Dot(gp_Vec(a.Axis().Direction())));
    const double depth = gap + 0.01 * thickness;
    BRepAlgoAPI_Common common(swept_in(body.face(f), depth), swept_in(body.face(g), depth));
    if (!common.IsDone() || common.HasErrors()) {
      throw Standard_Failure("the plates could not be found");
    }
    for (TopExp_Explorer it(common.Shape(), TopAbs_SOLID); it.More(); it.Next()) {
      out.push_back(it.Current());
    }
  }
  return out;
}

// The shapes the shape became through the steps (itself where a step left
// it alone).
std::vector<TopoDS_Shape> follow(const std::vector<History>& steps, const TopoDS_Shape& shape) {
  std::vector<TopoDS_Shape> current{shape};
  for (const History& step : steps) {
    if (step.IsNull()) {
      continue;
    }
    std::vector<TopoDS_Shape> next;
    for (const TopoDS_Shape& s : current) {
      if (step->IsRemoved(s)) {
        continue;
      }
      const NCollection_List<TopoDS_Shape>& modified = step->Modified(s);
      if (modified.IsEmpty()) {
        next.push_back(s);
      } else {
        next.insert(next.end(), modified.begin(), modified.end());
      }
    }
    current = std::move(next);
  }
  return current;
}

ShapeMap faces_of(const TopoDS_Shape& shape) {
  ShapeMap faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  return faces;
}

// The faces of `shape` the body's face became through the steps.
std::vector<TopoDS_Shape> face_images(const std::vector<History>& steps, const TopoDS_Shape& face,
                                      const ShapeMap& faces) {
  std::vector<TopoDS_Shape> images;
  for (const TopoDS_Shape& image : follow(steps, face)) {
    if (image.ShapeType() == TopAbs_FACE && faces.Contains(image)) {
      images.push_back(image);
    }
  }
  return images;
}

// Whether a face curves away from its outside more tightly than `radius`
// somewhere: a convex rounding whose offset inside vanishes.
bool tighter_than(const TopoDS_Face& face, double radius) {
  const BRepAdaptor_Surface adaptor(face, false);
  if (adaptor.GetType() == GeomAbs_Plane) {
    return false;
  }
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  if (surface.IsNull()) {
    return false;
  }
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const double outward = face.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
  GeomLProp_SLProps props(surface, 2, 1.0e-9);
  for (int i = 0; i <= 4; ++i) {
    for (int j = 0; j <= 4; ++j) {
      props.SetParameters(u1 + (u2 - u1) * i / 4, v1 + (v2 - v1) * j / 4);
      if (!props.IsCurvatureDefined()) {
        continue;
      }
      // A positive curvature bends towards the surface's normal.
      for (const double k : {props.MaxCurvature(), props.MinCurvature()}) {
        if (k * outward < 0 && 1.0 / std::abs(k) <= radius + kTolerance) {
          return true;
        }
      }
    }
  }
  return false;
}

// The body without the faces, its neighbours extended to close the gap;
// null when they cannot be taken away.
TopoDS_Shape without_faces(const TopoDS_Shape& body, const std::vector<TopoDS_Shape>& faces,
                           History& history) {
  BRepAlgoAPI_Defeaturing defeature;
  defeature.SetShape(body);
  for (const TopoDS_Shape& face : faces) {
    defeature.AddFaceToRemove(face);
  }
  defeature.SetRunParallel(false);
  defeature.SetToFillHistory(true);
  build(defeature);
  if (!defeature.IsDone() || defeature.HasErrors() ||
      defeature.HasWarning(STANDARD_TYPE(BOPAlgo_AlertUnableToRemoveTheFeature)) ||
      !is_valid(defeature.Shape())) {
    return TopoDS_Shape();
  }
  history = defeature.History();
  return defeature.Shape();
}

// Adds to the faces the curved faces beside them (not in `keep`), which may
// have to go with them (a chamfer round a rounded corner). False when there
// are none.
bool grow(const TopoDS_Shape& body, std::vector<TopoDS_Shape>& faces, const ShapeMap& keep) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edge_faces;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  ShapeMap chosen;
  for (const TopoDS_Shape& face : faces) {
    chosen.Add(face);
  }
  const std::size_t before = faces.size();
  for (std::size_t i = 0; i < before; ++i) {
    for (TopExp_Explorer e(faces[i], TopAbs_EDGE); e.More(); e.Next()) {
      for (const TopoDS_Shape& next : edge_faces.FindFromKey(e.Current())) {
        if (!chosen.Contains(next) && !keep.Contains(next) &&
            BRepAdaptor_Surface(TopoDS::Face(next), false).GetType() != GeomAbs_Plane) {
          chosen.Add(next);
          faces.push_back(next);
        }
      }
    }
  }
  return faces.size() > before;
}

// A solid of the closed shells of an offset.
TopoDS_Shape as_solid(const TopoDS_Shape& shape) {
  if (TopExp_Explorer(shape, TopAbs_SOLID).More()) {
    return shape;
  }
  BRep_Builder builder;
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  bool any = false;
  for (TopExp_Explorer it(shape, TopAbs_SHELL); it.More(); it.Next()) {
    builder.Add(solid, it.Current());
    any = true;
  }
  if (!any) {
    return TopoDS_Shape();
  }
  BRepLib::OrientClosedSolid(solid);
  return solid;
}

double solid_volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

double face_area(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

// The face's surface as a face reaching past the box on every side, as far
// as the surface goes.
TopoDS_Face surface_beyond(const TopoDS_Face& face, const Bnd_Box& box) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  if (surface.IsNull() || box.IsVoid()) {
    return TopoDS_Face();
  }
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  double su1 = 0, su2 = 0, sv1 = 0, sv2 = 0;
  surface->Bounds(su1, su2, sv1, sv2);
  const double reach = std::sqrt(box.SquareExtent()) + 1.0;
  if (surface->IsUPeriodic()) {
    u1 = su1;
    u2 = su1 + surface->UPeriod();
  } else {
    u1 = std::max(su1, u1 - reach);
    u2 = std::min(su2, u2 + reach);
  }
  if (surface->IsVPeriodic()) {
    v1 = sv1;
    v2 = sv1 + surface->VPeriod();
  } else {
    v1 = std::max(sv1, v1 - reach);
    v2 = std::min(sv2, v2 + reach);
  }
  BRepBuilderAPI_MakeFace make(surface, u1, u2, v1, v2, Precision::Confusion());
  return make.IsDone() ? make.Face() : TopoDS_Face();
}

// Whether the edge lies on the shape (at its ends and its middle).
bool lies_on(const TopoDS_Edge& edge, const TopoDS_Shape& shape) {
  const BRepAdaptor_Curve curve(edge);
  const double reach = std::max(10 * kTolerance, BRep_Tool::Tolerance(edge));
  for (const double s : {0.0, 0.5, 1.0}) {
    const gp_Pnt point = curve.Value(curve.FirstParameter() + s * (curve.LastParameter() - curve.FirstParameter()));
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), shape);
    if (!distance.IsDone() || distance.Value() > reach) {
      return false;
    }
  }
  return true;
}

bool lies_on(const TopoDS_Vertex& vertex, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape distance(vertex, shape);
  return distance.IsDone() && distance.Value() <= std::max(10 * kTolerance, BRep_Tool::Tolerance(vertex));
}

// Whether the point is inside (or on) one of the solids.
bool in_any(const std::vector<TopoDS_Shape>& solids, const gp_Pnt& point, TopAbs_State state) {
  for (const TopoDS_Shape& solid : solids) {
    BRepClass3d_SolidClassifier classifier(solid, point, 1.0e-6);
    if (classifier.State() == state) {
      return true;
    }
  }
  return false;
}

// Thin parts split off at their roots (mitcad#1240), for those no face
// round can be extended over the root (a fin whose ends continue the body's
// sides): the faces a thin face stands on concavely give the surfaces the
// shape is split by, together with those of the other thin faces that share
// one of them (a fin in a corner stands on two, a curved root is split by
// its whole surface, mitcad#1241); the pieces the thin faces bound are the parts, unless they are
// thicker than the thin parts can be (more volume than the wall times
// their area), are all of the shape, or hold all of a removed face. The
// faces the shape was split by go into `seams`.
std::vector<TopoDS_Shape> parts_at_roots(const TopoDS_Shape& shape, const std::vector<TopoDS_Shape>& thin,
                                         const ShapeMap& removed, double thickness, TopoDS_Compound& seams) {
  std::vector<TopoDS_Shape> parts;
  BRep_Builder builder;
  builder.MakeCompound(seams);
  if (thin.empty()) {
    return parts;
  }
  // The thin faces as the shape holds them (their normals out of it).
  const ShapeMap all = faces_of(shape);
  ShapeMap thin_faces;
  for (const TopoDS_Shape& face : thin) {
    if (all.Contains(face)) {
      thin_faces.Add(all.FindKey(all.FindIndex(face)));
    }
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edge_faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  BRepClass3d_SolidClassifier classifier(shape);
  // Each thin face's root: its edges where it stands concavely on another
  // face, and that face.
  std::vector<std::vector<std::pair<TopoDS_Edge, TopoDS_Face>>> roots;
  for (int i = 1; i <= thin_faces.Extent(); ++i) {
    const TopoDS_Face& face = TopoDS::Face(thin_faces(i));
    std::vector<std::pair<TopoDS_Edge, TopoDS_Face>> root;
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      if (!edge_faces.Contains(e.Current())) {
        continue;
      }
      for (const TopoDS_Shape& other : edge_faces.FindFromKey(e.Current())) {
        if (thin_faces.Contains(other)) {
          continue;
        }
        const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
        if (crease(classifier, edge, face, TopoDS::Face(other)) == Crease::Concave) {
          root.emplace_back(edge, TopoDS::Face(other));
        }
      }
    }
    roots.push_back(root);
  }
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  std::vector<bool> done(roots.size(), false);
  for (std::size_t i = 0; i < roots.size(); ++i) {
    if (done[i] || roots[i].empty()) {
      continue;
    }
    done[i] = true;
    // The surfaces the root lies on (a fin in a corner stands on two,
    // mitcad#1241), each once, reaching past the body: false when one
    // cannot be made.
    std::vector<TopoDS_Face> surfaces;
    const auto add_surfaces = [&](std::size_t j) {
      for (const std::pair<TopoDS_Edge, TopoDS_Face>& root : roots[j]) {
        if (std::any_of(surfaces.begin(), surfaces.end(),
                        [&](const TopoDS_Face& surface) { return lies_on(root.first, surface); })) {
          continue;
        }
        const TopoDS_Face surface = surface_beyond(root.second, box);
        if (surface.IsNull()) {
          return false;
        }
        surfaces.push_back(surface);
      }
      return true;
    };
    if (!add_surfaces(i)) {
      continue;
    }
    // With it the thin faces whose roots share a surface with it, and
    // theirs, until none is left.
    std::vector<std::size_t> group{i};
    bool grown = true;
    bool whole = true;
    while (grown && whole) {
      grown = false;
      for (std::size_t j = i + 1; j < roots.size() && whole; ++j) {
        if (done[j] || roots[j].empty()) {
          continue;
        }
        const bool shares = std::any_of(roots[j].begin(), roots[j].end(), [&](const std::pair<TopoDS_Edge, TopoDS_Face>& root) {
          return std::any_of(surfaces.begin(), surfaces.end(),
                             [&](const TopoDS_Face& surface) { return lies_on(root.first, surface); });
        });
        if (shares) {
          group.push_back(j);
          done[j] = true;
          grown = true;
          whole = add_surfaces(j);
        }
      }
    }
    if (!whole) {
      continue;
    }
    BRepAlgoAPI_Splitter splitter;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(shape);
    NCollection_List<TopoDS_Shape> tools;
    for (const TopoDS_Face& surface : surfaces) {
      tools.Append(surface);
    }
    splitter.SetArguments(arguments);
    splitter.SetTools(tools);
    splitter.SetNonDestructive(true);
    build(splitter);
    if (!splitter.IsDone() || splitter.HasErrors()) {
      continue;
    }
    // Points just inside the thin faces.
    std::vector<gp_Pnt> points;
    for (const std::size_t j : group) {
      for (const Sample& sample : samples(TopoDS::Face(thin_faces(static_cast<int>(j) + 1)))) {
        points.push_back(sample.point.Translated(gp_Vec(sample.normal) * -0.5 * kRayStart));
      }
    }
    std::vector<TopoDS_Shape> held;
    std::vector<TopoDS_Shape> rest;
    for (TopExp_Explorer it(splitter.Shape(), TopAbs_SOLID); it.More(); it.Next()) {
      const bool holds = std::any_of(points.begin(), points.end(), [&](const gp_Pnt& point) {
        return in_any({it.Current()}, point, TopAbs_IN);
      });
      (holds ? held : rest).push_back(it.Current());
    }
    bool fits = !held.empty() && !rest.empty();
    for (const TopoDS_Shape& piece : held) {
      fits = fits && solid_volume(piece) <= thickness * face_area(piece);
    }
    for (int r = 1; fits && r <= removed.Extent(); ++r) {
      bool in_held = false;
      bool in_rest = false;
      for (const Sample& sample : samples(TopoDS::Face(removed(r)))) {
        in_held = in_held || in_any(held, sample.point, TopAbs_ON);
        in_rest = in_rest || in_any(rest, sample.point, TopAbs_ON);
      }
      fits = in_rest || !in_held;
    }
    if (fits) {
      parts.insert(parts.end(), held.begin(), held.end());
      for (const TopoDS_Face& surface : surfaces) {
        builder.Add(seams, surface);
      }
    }
  }
  return parts;
}

// How much further than the others faces go in.
using Depths = NCollection_DataMap<TopoDS_Shape, double, TopTools_ShapeMapHasher>;

// The cavity: each solid of the shape offset inside by the wall, the faces
// in `out` moved out by `push`, those in `deeper` in by `deep` and those in
// `extra` that much further. Null when an offset fails; `steps` gets each
// solid's history.
TopoDS_Shape offset_inside(const TopoDS_Shape& shape, double thickness, GeomAbs_JoinType join,
                           const ShapeMap& out, double push, const ShapeMap& deeper, double deep,
                           const Depths& extra, std::vector<History>& steps) {
  BRep_Builder builder;
  TopoDS_Compound cavity;
  builder.MakeCompound(cavity);
  steps.clear();
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    const TopoDS_Shape& solid = it.Current();
    BRepOffset_MakeOffset maker;
    maker.Initialize(solid, -thickness, kTolerance, BRepOffset_Skin, false, false, join, false, false);
    for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
      const double* more = extra.Seek(f.Current());
      if (out.Contains(f.Current())) {
        maker.SetOffsetOnFace(TopoDS::Face(f.Current()), push);
      } else if (deeper.Contains(f.Current())) {
        maker.SetOffsetOnFace(TopoDS::Face(f.Current()), -deep - (more ? *more : 0.0));
      } else if (more) {
        maker.SetOffsetOnFace(TopoDS::Face(f.Current()), -thickness - *more);
      }
    }
    interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
    if (!maker.IsDone() || maker.Shape().IsNull()) {
      return TopoDS_Shape();
    }
    const TopoDS_Shape piece = as_solid(maker.Shape());
    if (piece.IsNull() || !is_valid(piece) || !(solid_volume(piece) > kTolerance)) {
      return TopoDS_Shape();
    }
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(solid);
    steps.push_back(new BRepTools_History(arguments, maker));
    builder.Add(cavity, piece);
  }
  return cavity;
}

// "A, B, C, D and 3 more".
std::string listed(const std::vector<std::string>& names) {
  constexpr std::size_t kShown = 4;
  std::string out;
  for (std::size_t i = 0; i < names.size() && i < kShown; ++i) {
    if (i > 0) {
      out += i + 1 == names.size() ? " and " : ", ";
    }
    out += names[i];
  }
  if (names.size() > kShown) {
    out += " and " + std::to_string(names.size() - kShown) + " more";
  }
  return out;
}

std::string faces_phrase(const std::vector<std::string>& names) {
  return (names.size() == 1 ? "face " : "faces ") + listed(names);
}

// The faces of the shape as a compound.
TopoDS_Compound face_compound(const TopoDS_Shape& shape) {
  BRep_Builder builder;
  TopoDS_Compound faces;
  builder.MakeCompound(faces);
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    builder.Add(faces, it.Current());
  }
  return faces;
}

// How far the face is from the faces, and the faces nearest to it (those
// at the nearest points, or round them where they are on edges or
// vertices). Infinite when it cannot be measured.
std::pair<double, std::vector<TopoDS_Shape>> nearest_faces(const TopoDS_Shape& face,
                                                           const TopoDS_Compound& faces) {
  std::pair<double, std::vector<TopoDS_Shape>> out{std::numeric_limits<double>::infinity(), {}};
  BRepExtrema_DistShapeShape distance(face, faces);
  if (!distance.IsDone() || distance.NbSolution() == 0) {
    return out;
  }
  out.first = distance.Value();
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> around;
  TopExp::MapShapesAndAncestors(faces, TopAbs_EDGE, TopAbs_FACE, around);
  TopExp::MapShapesAndAncestors(faces, TopAbs_VERTEX, TopAbs_FACE, around);
  ShapeMap found;
  for (int i = 1; i <= distance.NbSolution(); ++i) {
    const TopoDS_Shape support = distance.SupportOnShape2(i);
    if (support.ShapeType() == TopAbs_FACE) {
      found.Add(support);
    } else if (around.Contains(support)) {
      for (const TopoDS_Shape& f : around.FindFromKey(support)) {
        found.Add(f);
      }
    }
  }
  for (int i = 1; i <= found.Extent(); ++i) {
    out.second.push_back(found(i));
  }
  return out;
}

} // namespace

ShapePtr shell_by_cavity(const std::string& feature, const Shape& body, const std::set<int>& removed,
                         double thickness, GeomAbs_JoinType join, const Shape& from) {
  const double margin = 0.25 * thickness;
  try {
    const Thinness thin = thinness(body, removed, thickness);

    // The trimmed body less the solids, the faces they leave merged into
    // those beside them (the removed faces' edges kept; where `seams` holds
    // faces, only across the edges on them).
    std::vector<History> steps;
    TopoDS_Shape trimmed = body.occt();
    const auto cut_away = [&](const std::vector<TopoDS_Shape>& solids, const TopoDS_Shape& seams) {
      BRepAlgoAPI_Cut cut;
      NCollection_List<TopoDS_Shape> arguments;
      arguments.Append(trimmed);
      NCollection_List<TopoDS_Shape> tools;
      for (const TopoDS_Shape& solid : solids) {
        tools.Append(solid);
      }
      cut.SetArguments(arguments);
      cut.SetTools(tools);
      cut.SetNonDestructive(true);
      build(cut);
      if (!cut.IsDone() || cut.HasErrors() || !TopExp_Explorer(cut.Shape(), TopAbs_SOLID).More()) {
        return false;
      }
      std::vector<History> through = steps;
      through.push_back(cut.History());
      ShapeUpgrade_UnifySameDomain unify(cut.Shape(), true, true, false);
      for (int f : removed) {
        for (const TopoDS_Shape& image : follow(through, body.face(f))) {
          for (TopExp_Explorer e(image, TopAbs_EDGE); e.More(); e.Next()) {
            unify.KeepShape(e.Current());
          }
        }
      }
      if (!seams.IsNull()) {
        ShapeMap edges;
        TopExp::MapShapes(cut.Shape(), TopAbs_EDGE, edges);
        for (int i = 1; i <= edges.Extent(); ++i) {
          if (!lies_on(TopoDS::Edge(edges(i)), seams)) {
            unify.KeepShape(edges(i));
          }
        }
        ShapeMap vertices;
        TopExp::MapShapes(cut.Shape(), TopAbs_VERTEX, vertices);
        for (int i = 1; i <= vertices.Extent(); ++i) {
          if (!lies_on(TopoDS::Vertex(vertices(i)), seams)) {
            unify.KeepShape(vertices(i));
          }
        }
      }
      unify.Build();
      if (unify.Shape().IsNull()) {
        return false;
      }
      through.push_back(unify.History());
      steps = std::move(through);
      trimmed = unify.Shape();
      return true;
    };

    // The body without its plates.
    const std::vector<TopoDS_Shape> cut_off = plates(body, thin.pairs, thickness);
    if (!cut_off.empty() && !cut_away(cut_off, TopoDS_Shape())) {
      return nullptr;
    }

    // The other thin parts (curved or tapered) taken away, the faces round
    // them extended over their roots: all at once, else each alone.
    std::set<int> taken_away;
    const auto take_away = [&](const std::vector<int>& faces) {
      const ShapeMap all = faces_of(trimmed);
      std::vector<TopoDS_Shape> images;
      for (const int f : faces) {
        for (const TopoDS_Shape& image : face_images(steps, body.face(f), all)) {
          images.push_back(image);
        }
      }
      History history;
      const TopoDS_Shape plain = images.empty() ? TopoDS_Shape() : without_faces(trimmed, images, history);
      if (plain.IsNull()) {
        return false;
      }
      steps.push_back(history);
      trimmed = plain;
      taken_away.insert(faces.begin(), faces.end());
      return true;
    };
    const std::vector<std::vector<int>> parts = thin_parts(body, thin.thin, removed);
    std::vector<int> every;
    for (const std::vector<int>& part : parts) {
      every.insert(every.end(), part.begin(), part.end());
    }
    if (parts.size() > 1 && !take_away(every)) {
      for (const std::vector<int>& part : parts) {
        take_away(part);
      }
    } else if (parts.size() == 1) {
      take_away(every);
    }

    // The faces of the trimmed body that move out (the removed faces) and
    // those that go deeper (under removed faces).
    const auto images_in = [&](const TopoDS_Shape& shape, const std::vector<History>& through,
                               const std::set<int>& faces) {
      ShapeMap images;
      const ShapeMap all = faces_of(shape);
      for (int f : faces) {
        for (const TopoDS_Shape& image : face_images(through, body.face(f), all)) {
          images.Add(image);
        }
      }
      return images;
    };

    // The faces under removed ones go deeper, so that their offsets do not
    // touch the body; else (where the kernel fails on that) they go as deep
    // as the others, the removed faces out past them.
    std::vector<History> offsets;
    const auto cavity_of = [&](const TopoDS_Shape& shape, const std::vector<History>& through,
                               const Depths& extra) {
      const ShapeMap out = images_in(shape, through, removed);
      const ShapeMap deeper = images_in(shape, through, thin.under_removed);
      TopoDS_Shape made =
          offset_inside(shape, thickness, join, out, thin.push, deeper, thickness + margin, extra, offsets);
      if (made.IsNull() && !deeper.IsEmpty()) {
        made = offset_inside(shape, thickness, join, out, std::max(thin.push, thickness + margin),
                             ShapeMap(), thickness, extra, offsets);
      }
      return made;
    };
    TopoDS_Shape cavity = cavity_of(trimmed, steps, Depths());
    if (cavity.IsNull()) {
      // The thin parts that could not be taken away (no face round them
      // extends over their roots) split off at their roots (mitcad#1240).
      std::vector<TopoDS_Shape> left;
      const ShapeMap all = faces_of(trimmed);
      for (const int f : thin.thin) {
        if (taken_away.count(f) == 0) {
          for (const TopoDS_Shape& image : face_images(steps, body.face(f), all)) {
            left.push_back(image);
          }
        }
      }
      TopoDS_Compound seams;
      const std::vector<TopoDS_Shape> split_off =
          parts_at_roots(trimmed, left, images_in(trimmed, steps, removed), thickness, seams);
      if (!split_off.empty() && cut_away(split_off, seams)) {
        cavity = cavity_of(trimmed, steps, Depths());
      }
    }
    if (cavity.IsNull()) {
      // Without the convex roundings tighter than the wall; where they
      // cannot go alone, the curved faces beside them go too.
      std::vector<TopoDS_Shape> taken;
      const ShapeMap removed_images = images_in(trimmed, steps, removed);
      for (TopExp_Explorer it(trimmed, TopAbs_FACE); it.More(); it.Next()) {
        if (!removed_images.Contains(it.Current()) && tighter_than(TopoDS::Face(it.Current()), thickness)) {
          taken.push_back(it.Current());
        }
      }
      for (int round = 0; round < 3 && !taken.empty() && cavity.IsNull(); ++round) {
        if (round > 0 && !grow(trimmed, taken, removed_images)) {
          break;
        }
        History history;
        const TopoDS_Shape plain = without_faces(trimmed, taken, history);
        if (plain.IsNull()) {
          continue;
        }
        std::vector<History> through = steps;
        through.push_back(history);
        cavity = cavity_of(plain, through, Depths());
        if (cavity.IsNull()) {
          continue;
        }
        steps = through;
        trimmed = plain;
      }
      if (cavity.IsNull()) {
        return nullptr;
      }
    }

    // The wall at least as thick as given from the faces the inside of the
    // wall runs past (mitcad#1237): where the cavity comes nearer to one,
    // the faces of the trimmed body the nearest part of the cavity is offset
    // from go deeper by the difference, a few rounds while it helps.
    {
      std::vector<int> left_out;
      const ShapeMap kept = faces_of(trimmed);
      for (int f = 0; f < body.face_count(); ++f) {
        if (removed.count(f) == 0 && face_images(steps, body.face(f), kept).empty()) {
          left_out.push_back(f);
        }
      }
      Depths extra;
      for (int round = 0; round < 4 && !left_out.empty(); ++round) {
        // The face of the trimmed body each face of the cavity comes from.
        NCollection_DataMap<TopoDS_Shape, TopoDS_Shape, TopTools_ShapeMapHasher> source;
        for (int i = 1; i <= kept.Extent(); ++i) {
          for (const History& offset : offsets) {
            for (const TopoDS_Shape& made : offset->Generated(kept(i))) {
              source.Bind(made, kept(i));
            }
            for (const TopoDS_Shape& made : offset->Modified(kept(i))) {
              source.Bind(made, kept(i));
            }
          }
        }
        const TopoDS_Compound inner = face_compound(cavity);
        Depths more;
        for (const int f : left_out) {
          const auto [distance, nearest] = nearest_faces(body.face(f), inner);
          if (!(distance < thickness - kTolerance)) {
            continue;
          }
          for (const TopoDS_Shape& face : nearest) {
            const TopoDS_Shape* from_face = source.Seek(face);
            if (from_face == nullptr) {
              continue;
            }
            const double need = thickness - distance + 10 * kTolerance;
            double* already = more.ChangeSeek(*from_face);
            if (already == nullptr) {
              more.Bind(*from_face, need);
            } else {
              *already = std::max(*already, need);
            }
          }
        }
        if (more.IsEmpty()) {
          break;
        }
        Depths deeper = extra;
        for (Depths::Iterator it(more); it.More(); it.Next()) {
          double* already = deeper.ChangeSeek(it.Key());
          if (already == nullptr) {
            deeper.Bind(it.Key(), it.Value());
          } else {
            *already += it.Value();
          }
        }
        std::vector<History> tried;
        std::swap(tried, offsets);
        const TopoDS_Shape thicker = cavity_of(trimmed, steps, deeper);
        if (thicker.IsNull()) {
          std::swap(tried, offsets);
          break;
        }
        cavity = thicker;
        extra = deeper;
      }
    }

    // The cavity's faces named after the body's faces they come from.
    FaceNamer cavity_names(cavity);
    const ShapeMap trimmed_faces = faces_of(trimmed);
    for (int f = 0; f < body.face_count(); ++f) {
      const NameList& names = body.face_names(f);
      if (removed.count(f) > 0 || names.empty()) {
        continue;
      }
      for (const TopoDS_Shape& image : face_images(steps, body.face(f), trimmed_faces)) {
        for (const History& offset : offsets) {
          std::vector<TopoDS_Shape> made;
          for (const TopoDS_Shape& s : offset->Generated(image)) {
            made.push_back(s);
          }
          for (const TopoDS_Shape& s : offset->Modified(image)) {
            made.push_back(s);
          }
          for (const TopoDS_Shape& face : made) {
            if (face.ShapeType() != TopAbs_FACE) {
              continue;
            }
            for (const std::string& name : names) {
              cavity_names.add(face, face_name(feature, "offset", name));
            }
          }
        }
      }
    }
    // The faces made from the body's edges and vertices (rounded joins, and
    // the ends of the wall at removed faces) when they have no name yet.
    const auto name_made = [&](const TopoDS_Shape& source, TopAbs_ShapeEnum kind, const std::string& name) {
      for (const TopoDS_Shape& image : follow(steps, source)) {
        if (image.ShapeType() != kind) {
          continue;
        }
        for (const History& offset : offsets) {
          for (const TopoDS_Shape& face : offset->Generated(image)) {
            if (face.ShapeType() == TopAbs_FACE && !cavity_names.named(face)) {
              cavity_names.add(face, name);
            }
          }
        }
      }
    };
    for (int e = 0; e < body.edge_count(); ++e) {
      if (body.edge_name(e).empty()) {
        continue;
      }
      bool at_removed = false;
      for (int f : faces_at_edge(body, e)) {
        at_removed = at_removed || removed.count(f) > 0;
      }
      name_made(body.edge(e), TopAbs_EDGE,
                face_name(feature, at_removed ? "offset_cap" : "offset", body.edge_name(e)));
    }
    for (int v = 0; v < body.vertex_count(); ++v) {
      if (!body.vertex_name(v).empty()) {
        name_made(body.vertex(v), TopAbs_VERTEX, face_name(feature, "offset", body.vertex_name(v)));
      }
    }
    const ShapePtr named_cavity = cavity_names.shape();

    // `from` less the cavity.
    BRepAlgoAPI_Cut cut;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(from.occt());
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(cavity);
    cut.SetArguments(arguments);
    cut.SetTools(tools);
    cut.SetNonDestructive(true);
    build(cut);
    if (!cut.IsDone() || cut.HasErrors()) {
      return nullptr;
    }
    const TopoDS_Shape hollowed = cut.Shape();
    const History history = cut.History();
    FaceNamer namer(hollowed);
    // The removed faces of `from` are the body's own when it is the body.
    std::vector<int> except;
    if (&from == &body) {
      except.assign(removed.begin(), removed.end());
    }
    // The cavity keeps the wall's thickness from the kept faces: one that
    // cuts into a kept face is wrong (the kernel's offset dropped faces it
    // should have kept).
    const ShapeMap hollowed_faces = faces_of(hollowed);
    for (int f = 0; f < from.face_count(); ++f) {
      if (std::find(except.begin(), except.end(), f) != except.end()) {
        continue;
      }
      double area = 0.0;
      for (const TopoDS_Shape& image : face_images({history}, from.face(f), hollowed_faces)) {
        area += face_area(image);
      }
      const double before = face_area(from.face(f));
      if (area < before - 1.0e-6 * std::max(1.0, before)) {
        return nullptr;
      }
    }
    namer.carry(*history, from, except);
    namer.carry(*history, *named_cavity);
    for (int f : except) {
      const std::string name = primary_name(body, f);
      if (name.empty()) {
        continue;
      }
      for (const TopoDS_Shape& image : face_images({history}, body.face(f), faces_of(hollowed))) {
        namer.add(image, face_name(feature, "offset_cap", name));
      }
    }
    namer.finish();
    ShapePtr result = namer.shape();

    if (!(solid_volume(from.occt()) - solid_volume(hollowed) > kTolerance)) {
      std::vector<std::string> names;
      for (int f = 0; f < body.face_count(); ++f) {
        if (removed.count(f) == 0 && !primary_name(body, f).empty()) {
          names.push_back(primary_name(body, f));
        }
      }
      std::sort(names.begin(), names.end());
      throw std::runtime_error("shell failed; the wall does not fit under " + faces_phrase(names) +
                               ": nothing can be hollowed");
    }
    note_missing_walls(feature, body, removed, thickness, *result);
    return result;
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_OutOfMemory&) {
    throw;
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

void note_missing_walls(const std::string& feature, const Shape& body, const std::set<int>& removed,
                        double thickness, Shape& result) {
  std::vector<int> missing;
  for (int f = 0; f < body.face_count(); ++f) {
    if (removed.count(f) > 0 || body.face_names(f).empty()) {
      continue;
    }
    bool offset = false;
    for (const std::string& name : body.face_names(f)) {
      offset = offset || !result.find_faces(face_name(feature, "offset", name)).empty();
    }
    if (!offset) {
      missing.push_back(f);
    }
  }
  if (missing.empty()) {
    return;
  }
  // The inside of the wall.
  const std::string inner_prefix = feature + ":offset(";
  BRep_Builder builder;
  TopoDS_Compound inner;
  builder.MakeCompound(inner);
  bool hollowed = false;
  for (int i = 0; i < result.face_count(); ++i) {
    const NameList& names = result.face_names(i);
    if (std::any_of(names.begin(), names.end(),
                    [&](const std::string& name) { return name.rfind(inner_prefix, 0) == 0; })) {
      builder.Add(inner, result.face(i));
      hollowed = true;
    }
  }
  std::vector<std::string> solid;
  std::vector<std::string> past;
  // The faces the wall is thinner than given beside, and how thin it gets.
  std::vector<std::string> thinner;
  double thinnest = thickness;
  for (int f : missing) {
    bool far = !hollowed;
    for (const Sample& sample : hollowed ? samples(body.face(f)) : std::vector<Sample>()) {
      BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(sample.point).Vertex(), inner);
      if (distance.IsDone() && distance.Value() > 2 * thickness + kTolerance) {
        far = true;
        break;
      }
    }
    (far ? solid : past).push_back(primary_name(body, f));
    if (hollowed) {
      const double distance = nearest_faces(body.face(f), inner).first;
      if (distance < thickness - kTolerance) {
        thinner.push_back(primary_name(body, f));
        thinnest = std::min(thinnest, distance);
      }
    }
  }
  std::sort(solid.begin(), solid.end());
  std::sort(past.begin(), past.end());
  if (!solid.empty()) {
    result.add_note("the wall does not fit under " + faces_phrase(solid) +
                    (solid.size() == 1 ? "; it is" : "; the body is") + " left solid there");
  }
  if (!past.empty()) {
    result.add_note("the inside of the wall runs past " + faces_phrase(past) +
                    (past.size() == 1 ? ", which is" : ", which are") +
                    " too small or too tightly curved for it to follow");
  }
  if (!thinner.empty()) {
    std::sort(thinner.begin(), thinner.end());
    char amounts[96];
    std::snprintf(amounts, sizeof amounts, "the wall is thinner than %g mm beside ", thickness);
    char least[64];
    std::snprintf(least, sizeof least, ": down to %.2f mm (%.2f mm less)", thinnest, thickness - thinnest);
    result.add_note(amounts + faces_phrase(thinner) + least);
  }
}

} // namespace mitcad::geometry::detail
