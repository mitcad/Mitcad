// SPDX-License-Identifier: MIT
#include "mitcad/io/mesh.hpp"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <limits>
#include <map>
#include <memory>
#include <mutex>
#include <new>
#include <system_error>
#include <thread>
#include <unordered_map>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepMesh_Context.hxx>
#include <BRepMesh_Deflection.hxx>
#include <BRepMesh_DiscretAlgoFactory.hxx>
#include <BRepMesh_DiscretFactory.hxx>
#include <BRepMesh_EdgeDiscret.hxx>
#include <BRepMesh_DelabellaMeshAlgoFactory.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepMesh_MeshAlgoFactory.hxx>
#include <BRepMesh_ShapeTool.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_CurveRepresentation.hxx>
#include <BRep_TEdge.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Bnd_Box2d.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Plane.hxx>
#include <Geom_Surface.hxx>
#include <IMeshData_Curve.hxx>
#include <IMeshData_Edge.hxx>
#include <IMeshData_Face.hxx>
#include <IMeshData_Model.hxx>
#include <IMeshData_PCurve.hxx>
#include <IMeshData_Status.hxx>
#include <IMeshData_Wire.hxx>
#include <IMeshTools_CurveTessellator.hxx>
#include <IMeshTools_MeshAlgo.hxx>
#include <IMeshTools_MeshAlgoFactory.hxx>
#include <IMeshTools_ModelAlgo.hxx>
#include <IMeshTools_Parameters.hxx>
#include <Message_ProgressIndicator.hxx>
#include <OSD.hxx>
#include <OSD_Environment.hxx>
#include <TCollection_AsciiString.hxx>
#include <Message_ProgressScope.hxx>
#include <OSD_Parallel.hxx>
#include <Poly_PolygonOnTriangulation.hxx>
#include <Precision.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Circ.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Elips.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <gp_XY.hxx>

#include "mitcad/io/body.hpp"

namespace mitcad::io {

namespace {
constexpr double kPi = 3.14159265358979323846;
constexpr double kDegree = kPi / 180.0;
}

MeshOptions MeshOptions::from(Refinement refinement) {
  switch (refinement) {
  case Refinement::Low:
    return MeshOptions{0.1, 30.0 * kDegree, false, 5000, false, false};
  case Refinement::Medium:
    return MeshOptions{0.03, 15.0 * kDegree, false, 20000, false, false};
  case Refinement::High:
    return MeshOptions{0.01, 8.0 * kDegree, false, 50000, false, false};
  }
  return MeshOptions{};
}

namespace {

// How many triangles BRepMesh gives a curved face, per unit of the
// integral of the square root of the Gaussian curvature over it, at a
// deviation of 1 mm (inversely proportional to the deviation). Its grid
// on a sphere has steps of 0.7 times the arc whose sag is the deviation,
// about 5 R / d triangles on a hemisphere, whose integral is 2 pi R.
constexpr double kTrianglesPerCurvature = 0.8;
// The integral is at most about this many times the diagonal of the face's
// box unless the face is wavy (a whole sphere: 3.6): smaller faces cannot
// need a larger deviation and are not integrated.
constexpr double kCurvaturePerDiagonal = 4.0;

// Gauss-Legendre points and weights on [-1, 1], one half (symmetric).
constexpr double kGaussX[4] = {0.1834346424956498, 0.5255324099163290, 0.7966664774136267,
                               0.9602898564975363};
constexpr double kGaussW[4] = {0.3626837833783620, 0.3137066458778873, 0.2223810344533745,
                               0.1012285362903763};

// The integral of sqrt(|K|) dA over the face's parameter range (8 x 8
// Gauss points; the trimmed part counts too, which overestimates it).
// Zero for the surfaces whose Gaussian curvature is zero.
double curvature_integral(const TopoDS_Face& face) {
  try {
    const BRepAdaptor_Surface surface(face, false);
    switch (surface.GetType()) {
    case GeomAbs_Plane:
    case GeomAbs_Cylinder:
    case GeomAbs_Cone:
    case GeomAbs_SurfaceOfExtrusion:
      return 0.0;
    default:
      break;
    }
    double u1 = 0.0;
    double u2 = 0.0;
    double v1 = 0.0;
    double v2 = 0.0;
    BRepTools::UVBounds(face, u1, u2, v1, v2);
    if (!(u2 > u1) || !(v2 > v1) || Precision::IsInfinite(u2 - u1) || Precision::IsInfinite(v2 - v1)) {
      return 0.0;
    }
    const double mu = 0.5 * (u1 + u2);
    const double hu = 0.5 * (u2 - u1);
    const double mv = 0.5 * (v1 + v2);
    const double hv = 0.5 * (v2 - v1);
    double sum = 0.0;
    for (int i = 0; i < 8; ++i) {
      const double xu = (i < 4 ? -1.0 : 1.0) * kGaussX[i % 4];
      const double wu = kGaussW[i % 4];
      for (int j = 0; j < 8; ++j) {
        const double xv = (j < 4 ? -1.0 : 1.0) * kGaussX[j % 4];
        const double wv = kGaussW[j % 4];
        gp_Pnt p;
        gp_Vec du;
        gp_Vec dv;
        gp_Vec duu;
        gp_Vec dvv;
        gp_Vec duv;
        surface.D2(mu + hu * xu, mv + hv * xv, p, du, dv, duu, dvv, duv);
        const gp_Vec normal = du.Crossed(dv);
        const double area = normal.Magnitude(); // sqrt(EG - F^2)
        if (!(area > 1e-12)) {
          continue; // a pole
        }
        const gp_Vec unit = normal / area;
        const double k = (duu.Dot(unit) * dvv.Dot(unit) - duv.Dot(unit) * duv.Dot(unit)) / (area * area);
        sum += wu * wv * std::sqrt(std::abs(k)) * area;
      }
    }
    return std::isfinite(sum) ? sum * hu * hv : 0.0;
  } catch (const Standard_Failure&) {
    return 0.0;
  }
}

// How many triangles of the face bound make one straight piece of a turn
// round a cylinder or a cone (mitcad#438): the view's 20 000 give the
// deviation of a turn in 500 pieces, 2e-5 of the radius, which a circle
// filling a 4K screen shows as less than a tenth of a pixel (BRepMesh cuts
// an edge at half its deviation: about 700 points a turn). At the view's
// 0.01 mm a cylinder of metres got thousands (2 600 round a radius of
// 6.8 m), and its edges' points are its triangles' corners.
constexpr double kTrianglesPerTurnPiece = 40.0;

// The deviation of a circle of `radius` cut into a turn's pieces for the
// bound: 0 for a radius of 0, infinite for an infinite one (a line).
double turn_deviation(double radius, std::size_t face_triangles) {
  const double pieces = std::max(8.0, static_cast<double>(face_triangles) / kTrianglesPerTurnPiece);
  return std::abs(radius) * (1.0 - std::cos(kPi / pieces));
}

// The radius of a cylinder face, the largest radius of a cone face over its
// parameter range (BRepMesh's cone splitter steps round it at that radius);
// 0 for other faces.
double round_radius(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  switch (surface.GetType()) {
  case GeomAbs_Cylinder:
    return surface.Cylinder().Radius();
  case GeomAbs_Cone: {
    double u1 = 0.0;
    double u2 = 0.0;
    double v1 = 0.0;
    double v2 = 0.0;
    BRepTools::UVBounds(face, u1, u2, v1, v2);
    if (Precision::IsInfinite(v1) || Precision::IsInfinite(v2)) {
      return 0.0;
    }
    const gp_Cone cone = surface.Cone();
    const double grows = std::sin(cone.SemiAngle());
    return std::max(std::abs(cone.RefRadius() + v1 * grows), std::abs(cone.RefRadius() + v2 * grows));
  }
  default:
    return 0.0;
  }
}

// The smallest radius of curvature of an edge's curve: its radius for a
// circle, infinite for a line, sampled for other curves; 0 when it cannot
// be told.
double edge_radius(const TopoDS_Edge& edge) {
  try {
    const BRepAdaptor_Curve curve(edge);
    switch (curve.GetType()) {
    case GeomAbs_Line:
      return std::numeric_limits<double>::infinity();
    case GeomAbs_Circle:
      return curve.Circle().Radius();
    case GeomAbs_Ellipse: {
      const gp_Elips ellipse = curve.Ellipse();
      return ellipse.MajorRadius() > 0.0
                 ? ellipse.MinorRadius() * ellipse.MinorRadius() / ellipse.MajorRadius()
                 : 0.0;
    }
    default:
      break;
    }
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    if (Precision::IsInfinite(first) || Precision::IsInfinite(last) || !(last > first)) {
      return 0.0;
    }
    // |C' x C''| / |C'|^3 at 17 points along it.
    double curvature = 0.0;
    for (int i = 0; i <= 16; ++i) {
      gp_Pnt point;
      gp_Vec d1;
      gp_Vec d2;
      curve.D2(first + (last - first) * i / 16.0, point, d1, d2);
      const double speed = d1.Magnitude();
      if (!(speed > 1e-12)) {
        return 0.0;
      }
      curvature = std::max(curvature, d1.Crossed(d2).Magnitude() / (speed * speed * speed));
    }
    return curvature > 0.0 ? 1.0 / curvature : std::numeric_limits<double>::infinity();
  } catch (const Standard_Failure&) {
    return 0.0;
  }
}

// Discretizes a model's edge again at `deviation`, as BRepMesh_EdgeDiscret
// does (its own deviation being the parameters'): the finest polygon a
// triangulation of one of its faces has for it, when one meets the
// deviation, else a new tessellation of its curve.
void rediscretize(const IMeshData::IEdgeHandle& edge, double deviation, const IMeshTools_Parameters& parameters) {
  edge->SetDeflection(deviation);
  edge->UnsetStatus(IMeshData_Outdated);
  edge->Clear(false);
  double finest = std::numeric_limits<double>::max();
  int found = -1;
  for (int i = 0; i < edge->PCurvesNb(); ++i) {
    const TopoDS_Face& face = edge->GetPCurve(i)->GetFace()->GetFace();
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation>& triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      continue;
    }
    const occ::handle<Poly_PolygonOnTriangulation>& polygon =
        BRep_Tool::PolygonOnTriangulation(edge->GetEdge(), triangulation, location);
    if (polygon.IsNull()) {
      continue;
    }
    if (polygon->HasParameters() &&
        BRepMesh_Deflection::IsConsistent(polygon->Deflection(), deviation, parameters.AllowQualityDecrease)) {
      if (polygon->Deflection() < finest) {
        finest = polygon->Deflection();
        found = i;
      }
    } else {
      edge->SetStatus(IMeshData_Outdated); // its faces are meshed again
    }
  }
  occ::handle<IMeshTools_CurveTessellator> tessellator;
  if (found >= 0) {
    edge->SetDeflection(finest);
    tessellator = BRepMesh_EdgeDiscret::CreateEdgeTessellationExtractor(
        edge, IMeshData::IFaceHandle(edge->GetPCurve(found)->GetFace()));
  } else {
    const IMeshData::IPCurveHandle& pcurve = edge->GetPCurve(0);
    tessellator = BRepMesh_EdgeDiscret::CreateEdgeTessellator(
        edge, pcurve->GetOrientation(), IMeshData::IFaceHandle(pcurve->GetFace()), parameters);
  }
  BRepMesh_EdgeDiscret::Tessellate3d(edge, tessellator, true);
  BRepMesh_EdgeDiscret::Tessellate2d(edge, true);
}

// The edges of large cylinders and cones at a deviation that keeps a turn
// round them within the bound's pieces (turn_deviation, mitcad#438): their
// faces' triangles are mostly their edges' points (BRepMesh puts no points
// inside a cylinder), so an edge takes the smallest floor of the faces it
// bounds (face_deviation_floor), planes having no say, and no more than its
// own curve's turn deviation; one that bounds no cylinder or cone, or a
// face whose floor is none, keeps BRepMesh's. In parallel when the meshing
// is: each task changes only its own edge's discretization.
void coarsen_round_edges(const occ::handle<IMeshData_Model>& model, const IMeshTools_Parameters& parameters,
                         std::size_t face_triangles) {
  if (face_triangles == 0 || model.IsNull() || parameters.Relative) {
    return;
  }
  OSD_Parallel::For(
      0, model->EdgesNb(),
      [&model, &parameters, face_triangles](int i) {
        const IMeshData::IEdgeHandle& edge = model->GetEdge(i);
        if (edge->IsFree() || edge->GetDegenerated() || edge->IsSet(IMeshData_Failure)) {
          return;
        }
        const double own = edge->GetDeflection();
        try {
          bool round = false;
          double floor = std::numeric_limits<double>::infinity();
          for (int p = 0; p < edge->PCurvesNb() && floor > own; ++p) {
            const IMeshData::IFacePtr& face = edge->GetPCurve(p)->GetFace();
            const GeomAbs_SurfaceType type = face->GetSurface()->GetType();
            if (type == GeomAbs_Plane) {
              continue;
            }
            round = round || type == GeomAbs_Cylinder || type == GeomAbs_Cone;
            floor = std::min(floor, face_deviation_floor(face->GetFace(), parameters.Deflection, face_triangles));
          }
          if (!round || !(floor > own) || !std::isfinite(floor)) {
            return;
          }
          floor = std::min(floor, turn_deviation(edge_radius(edge->GetEdge()), face_triangles));
          if (!(floor > own)) {
            return;
          }
          rediscretize(edge, floor, parameters);
        } catch (const Standard_Failure&) {
          // As BRepMesh discretized it.
          try {
            rediscretize(edge, own, parameters);
          } catch (const Standard_Failure&) {
            edge->SetStatus(IMeshData_Failure);
          }
        }
      },
      !parameters.InParallel || model->EdgesNb() < 8);
}

// Small circles by their size (MeshOptions::small_circle_segments,
// mitcad#879): each edge on a circle (or an arc of one) that its deviation
// cuts more coarsely than its angle gets the angle small_circle_angle gives
// it as its own angular deflection, which BRepMesh's edge tessellator takes
// (half of it, as of the parameters' angle that BRepMesh_Deflection gave
// it), and is discretized again as coarsen_round_edges does: a polygon
// that a face's triangulation has for it at its deviation is taken again,
// as BRepMesh took it, so faces that keep their triangulations keep their
// edges. Before coarsen_round_edges, which discretizes the edges of large
// cylinders again with the angle each edge has. In parallel when the
// meshing is: each task changes only its own edge.
void widen_small_circle_edges(const occ::handle<IMeshData_Model>& model, const IMeshTools_Parameters& parameters,
                              int segments) {
  if (segments <= 0 || model.IsNull() || parameters.Relative) {
    return;
  }
  OSD_Parallel::For(
      0, model->EdgesNb(),
      [&model, &parameters, segments](int i) {
        const IMeshData::IEdgeHandle& edge = model->GetEdge(i);
        if (edge->IsFree() || edge->GetDegenerated() || edge->IsSet(IMeshData_Failure) || edge->PCurvesNb() == 0) {
          return;
        }
        const double own = edge->GetDeflection();
        const double angle = edge->GetAngularDeflection();
        try {
          const BRepAdaptor_Curve curve(edge->GetEdge());
          if (curve.GetType() != GeomAbs_Circle) {
            return;
          }
          const double wider = small_circle_angle(curve.Circle().Radius(), own, angle, segments);
          if (!(wider > angle)) {
            return;
          }
          edge->SetAngularDeflection(wider);
          rediscretize(edge, own, parameters);
        } catch (const Standard_Failure&) {
          // As BRepMesh discretized it.
          try {
            edge->SetAngularDeflection(angle);
            rediscretize(edge, own, parameters);
          } catch (const Standard_Failure&) {
            edge->SetStatus(IMeshData_Failure);
          }
        }
      },
      !parameters.InParallel || model->EdgesNb() < 8);
}

// A segment of a wire's polygon in a face's parameter plane, and its edge.
struct WireSegment {
  IMeshData::IEdgePtr edge;
  gp_XY a;
  gp_XY b;
};

// Whether two segments cross each other at a point inside both (not where
// one ends: wires meet at their vertices).
bool segments_cross(const WireSegment& s, const WireSegment& t) {
  const auto side = [](const gp_XY& from, const gp_XY& to, const gp_XY& p) { return (to - from) ^ (p - from); };
  const double d1 = side(t.a, t.b, s.a);
  const double d2 = side(t.a, t.b, s.b);
  const double d3 = side(s.a, s.b, t.a);
  const double d4 = side(s.a, s.b, t.b);
  return ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0));
}

// The pairs of edges of different wires of a face whose polygons cross in
// the face's parameter plane, each pair once.
std::vector<std::pair<IMeshData::IEdgePtr, IMeshData::IEdgePtr>> crossing_wire_edges(
    const IMeshData::IFaceHandle& face) {
  std::vector<std::pair<IMeshData::IEdgePtr, IMeshData::IEdgePtr>> pairs;
  const int wires = face->WiresNb();
  if (wires < 2) {
    return pairs;
  }
  std::vector<std::vector<WireSegment>> segments(static_cast<std::size_t>(wires));
  std::vector<Bnd_Box2d> boxes(static_cast<std::size_t>(wires));
  for (int w = 0; w < wires; ++w) {
    const IMeshData::IWireHandle& wire = face->GetWire(w);
    for (int e = 0; e < wire->EdgesNb(); ++e) {
      const IMeshData::IEdgePtr& edge = wire->GetEdge(e);
      const IMeshData::IPCurveHandle& pcurve = edge->GetPCurve(face.get(), wire->GetEdgeOrientation(e));
      for (int p = 1; p < pcurve->ParametersNb(); ++p) {
        const gp_XY a = pcurve->GetPoint(p - 1).XY();
        const gp_XY b = pcurve->GetPoint(p).XY();
        segments[static_cast<std::size_t>(w)].push_back({edge, a, b});
        boxes[static_cast<std::size_t>(w)].Add(gp_Pnt2d(a));
        boxes[static_cast<std::size_t>(w)].Add(gp_Pnt2d(b));
      }
    }
  }
  const auto box_of = [](const WireSegment& s) {
    Bnd_Box2d box;
    box.Add(gp_Pnt2d(s.a));
    box.Add(gp_Pnt2d(s.b));
    return box;
  };
  for (std::size_t w1 = 0; w1 < segments.size(); ++w1) {
    for (std::size_t w2 = w1 + 1; w2 < segments.size(); ++w2) {
      if (boxes[w1].IsVoid() || boxes[w2].IsVoid() || boxes[w1].IsOut(boxes[w2])) {
        continue;
      }
      // The segments of each wire within the other's box.
      std::vector<const WireSegment*> near1;
      std::vector<const WireSegment*> near2;
      for (const WireSegment& s : segments[w1]) {
        if (!boxes[w2].IsOut(box_of(s))) {
          near1.push_back(&s);
        }
      }
      for (const WireSegment& s : segments[w2]) {
        if (!boxes[w1].IsOut(box_of(s))) {
          near2.push_back(&s);
        }
      }
      for (const WireSegment* s : near1) {
        for (const WireSegment* t : near2) {
          if (!segments_cross(*s, *t)) {
            continue;
          }
          const auto known = [s, t](const std::pair<IMeshData::IEdgePtr, IMeshData::IEdgePtr>& pair) {
            return (pair.first == s->edge && pair.second == t->edge) ||
                   (pair.first == t->edge && pair.second == s->edge);
          };
          if (std::none_of(pairs.begin(), pairs.end(), known)) {
            pairs.emplace_back(s->edge, t->edge);
          }
        }
      }
    }
  }
  return pairs;
}

// How many times separate_close_wires cuts the edges of wires that still
// cross more finely, at most.
constexpr int kSeparationRounds = 4;

// The edges of a face's wires that lie closer together than their
// polygons' deviation (mitcad#1568): BRepMesh's healer looks for crossing
// polygons (BRepMesh_FaceChecker) and cuts their edges more finely, but it
// passes over segments that cross at less than 5 degrees, as two curves
// that touch would. Two wires that do not touch, a hole's edge 0.18 µm
// inside a plane's outer edge (a ring left between a cut circle and a
// circle joined to it), then cross each other wherever the chords of the
// outer one sag inside the inner one, and the face cannot be triangulated:
// BRepMesh leaves it without triangles, or with the outer wire's polygon
// filled and the hole's points unused. Here each pair of edges of
// different wires of a face whose polygons cross, the curves themselves
// apart (more than twice their tolerances in the model's space), is
// discretized again at a quarter of their distance (no finer than
// Precision::Confusion), so that neither polygon sags as far as the other
// curve; pairs that still cross (a curved face's parameters) are cut a
// third as finely again, at most kSeparationRounds times. Curves that touch
// are left as BRepMesh has them. After BRepMesh's healer, before the faces'
// deviations are set: the faces at an edge discretized again take its new
// polygon as their boundary.
void separate_close_wires(const occ::handle<IMeshData_Model>& model, const IMeshTools_Parameters& parameters) {
  if (model.IsNull()) {
    return;
  }
  std::vector<int> faces;
  for (int i = 0; i < model->FacesNb(); ++i) {
    const IMeshData::IFaceHandle& face = model->GetFace(i);
    if (face->WiresNb() > 1 && !face->IsSet(IMeshData_Failure)) {
      faces.push_back(i);
    }
  }
  // The distances of the pairs of edges found, by their pair.
  std::map<std::pair<const IMeshData_Edge*, const IMeshData_Edge*>, double> apart;
  for (int pass = 0; pass < kSeparationRounds && !faces.empty(); ++pass) {
    std::vector<std::vector<std::pair<IMeshData::IEdgePtr, IMeshData::IEdgePtr>>> found(faces.size());
    OSD_Parallel::For(
        0, static_cast<int>(faces.size()),
        [&model, &faces, &found](int i) {
          try {
            found[static_cast<std::size_t>(i)] = crossing_wire_edges(model->GetFace(faces[static_cast<std::size_t>(i)]));
          } catch (const Standard_Failure&) {
            // As BRepMesh has the face.
          }
        },
        !parameters.InParallel || faces.size() < 8);
    // The finest deviation each edge is to have.
    std::map<const IMeshData_Edge*, std::pair<IMeshData::IEdgePtr, double>> finer;
    std::vector<int> again;
    for (std::size_t i = 0; i < faces.size(); ++i) {
      bool changed = false;
      for (const auto& [first, second] : found[i]) {
        const IMeshData_Edge* a = first;
        const IMeshData_Edge* b = second;
        const auto key = a < b ? std::make_pair(a, b) : std::make_pair(b, a);
        auto known = apart.find(key);
        if (known == apart.end()) {
          double distance = 0.0;
          try {
            BRepExtrema_DistShapeShape extrema(first->GetEdge(), second->GetEdge());
            if (extrema.IsDone() && extrema.NbSolution() > 0) {
              distance = extrema.Value();
            }
          } catch (const Standard_Failure&) {
            distance = 0.0;
          }
          const double touching =
              2.0 * std::max(BRep_Tool::Tolerance(first->GetEdge()), BRep_Tool::Tolerance(second->GetEdge()));
          known = apart.emplace(key, distance > touching ? distance : 0.0).first;
        }
        if (!(known->second > 0.0)) {
          continue; // the curves touch
        }
        for (const IMeshData::IEdgePtr& edge : {first, second}) {
          const double now = edge->GetDeflection();
          const double wanted =
              std::max(pass == 0 ? std::min(now, 0.25 * known->second) : now / 3.0, Precision::Confusion());
          if (!(wanted < now)) {
            continue;
          }
          auto at = finer.find(edge);
          if (at == finer.end()) {
            finer.emplace(edge, std::make_pair(edge, wanted));
          } else {
            at->second.second = std::min(at->second.second, wanted);
          }
          changed = true;
        }
      }
      if (changed) {
        again.push_back(faces[i]);
      }
    }
    for (const auto& entry : finer) {
      const IMeshData::IEdgeHandle edge = entry.second.first;
      const double own = edge->GetDeflection();
      try {
        rediscretize(edge, entry.second.second, parameters);
      } catch (const Standard_Failure&) {
        try {
          rediscretize(edge, own, parameters);
        } catch (const Standard_Failure&) {
          edge->SetStatus(IMeshData_Failure);
        }
      }
    }
    faces = std::move(again);
  }
}

// The largest radius round a cone, torus or sphere face (a torus's major
// and minor radii together, a cone's at the end of its range farther from
// its apex); 0 for other faces and when it cannot be told. What small
// circles by their size widen such a face's angles by (SmallRoundFaceAlgo).
double small_round_radius(const TopoDS_Face& face) {
  try {
    const BRepAdaptor_Surface surface(face, false);
    switch (surface.GetType()) {
    case GeomAbs_Cone:
      return round_radius(face);
    case GeomAbs_Torus: {
      const gp_Torus torus = surface.Torus();
      return std::abs(torus.MajorRadius()) + std::abs(torus.MinorRadius());
    }
    case GeomAbs_Sphere:
      return surface.Sphere().Radius();
    default:
      return 0.0;
    }
  } catch (const Standard_Failure&) {
    return 0.0;
  }
}

using Clock = std::chrono::steady_clock;

double milliseconds_since(Clock::time_point start) {
  return std::chrono::duration<double, std::milli>(Clock::now() - start).count();
}

} // namespace

const char* surface_name(GeomAbs_SurfaceType type) {
  switch (type) {
  case GeomAbs_Plane:
    return "plane";
  case GeomAbs_Cylinder:
    return "cylinder";
  case GeomAbs_Cone:
    return "cone";
  case GeomAbs_Sphere:
    return "sphere";
  case GeomAbs_Torus:
    return "torus";
  case GeomAbs_BezierSurface:
    return "bezier";
  case GeomAbs_BSplineSurface:
    return "bspline";
  case GeomAbs_SurfaceOfRevolution:
    return "revolution";
  case GeomAbs_SurfaceOfExtrusion:
    return "extrusion";
  case GeomAbs_OffsetSurface:
    return "offset";
  default:
    return "other";
  }
}

namespace {

// What one meshing shares with the threads that triangulate its faces and
// the algorithms they run (mitcad#846). Each of them holds it, so that a
// meshing abandoned after a stop (a face that does not end, StoppableFace
// Discret) leaves them nothing that is freed under them. `mutex` orders
// the faces' results against the abandon: once `abandoned` is set, nothing
// is written into the shapes' own faces or into `times` any more.
struct MeshShared {
  std::mutex mutex;
  bool abandoned = false;
  std::size_t busy = 0;      // faces being triangulated (PrivateFaceAlgo)
  std::size_t left = 0;      // of them, abandoned
  bool timed = false;        // each face's time into `times`
  std::vector<MeshReport::FaceTime> times;
  std::atomic<std::size_t> redone{0}; // CheckedPlaneAlgo
  std::atomic<bool> stop{false};      // the meshing's range was stopped
  // An allocation failed in a face's triangulation (FaceWork, mitcad#936):
  // the meshing throws "out of memory" once it ends (BoundedMesh).
  std::atomic<bool> out_of_memory{false};
};
using MeshSharedPtr = std::shared_ptr<MeshShared>;

// BRepMesh's algorithm for a face, timed (MeshReport::slowest).
class TimedAlgo : public IMeshTools_MeshAlgo {
public:
  TimedAlgo(occ::handle<IMeshTools_MeshAlgo> inner, MeshSharedPtr shared)
      : m_inner(std::move(inner)), m_shared(std::move(shared)) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    const Clock::time_point start = Clock::now();
    try {
      m_inner->Perform(face, parameters, range);
    } catch (...) {
      record(face, start); // BRepMesh marks the face failed
      throw;
    }
    record(face, start);
  }

private:
  void record(const IMeshData::IFaceHandle& face, Clock::time_point start) {
    MeshReport::FaceTime time;
    time.milliseconds = milliseconds_since(start);
    time.face = face->GetFace();
    time.surface = surface_name(face->GetSurface()->GetType());
    time.wires = face->WiresNb();
    for (int w = 0; w < face->WiresNb(); ++w) {
      const IMeshData::IWireHandle& wire = face->GetWire(w);
      for (int e = 0; e < wire->EdgesNb(); ++e) {
        time.points += std::max(0, wire->GetEdge(e)->GetCurve()->ParametersNb() - 1);
      }
    }
    const std::lock_guard<std::mutex> lock(m_shared->mutex);
    if (!m_shared->abandoned) {
      m_shared->times.push_back(std::move(time));
    }
  }

  occ::handle<IMeshTools_MeshAlgo> m_inner;
  MeshSharedPtr m_shared;
};

// A face's algorithm run on a private copy of the face (mitcad#846): the
// same surface, location, orientation, tolerance and triangulation, no
// sub-shapes (BRepMesh takes the boundary from its own model). What the
// algorithm leaves in the copy is moved into the face itself when it ends,
// unless the meshing was abandoned meanwhile: a face that ends after that
// writes into nothing anyone else reads.
class PrivateFaceAlgo : public IMeshTools_MeshAlgo {
public:
  PrivateFaceAlgo(occ::handle<IMeshTools_MeshAlgo> inner, MeshSharedPtr shared)
      : m_inner(std::move(inner)), m_shared(std::move(shared)) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    const TopoDS_Face original = face->GetFace();
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> before = BRep_Tool::Triangulation(original, location);
    const TopoDS_Face own = TopoDS::Face(original.EmptyCopied());
    BRep_Builder().UpdateFace(own, before);
    {
      const std::lock_guard<std::mutex> lock(m_shared->mutex);
      ++m_shared->busy;
    }
    face->SetShape(own);
    try {
      m_inner->Perform(face, parameters, range);
    } catch (...) {
      finish(face, original, own, before);
      throw;
    }
    finish(face, original, own, before);
  }

private:
  void finish(const IMeshData::IFaceHandle& face, const TopoDS_Face& original, const TopoDS_Face& own,
              const occ::handle<Poly_Triangulation>& before) {
    const std::lock_guard<std::mutex> lock(m_shared->mutex);
    --m_shared->busy;
    if (m_shared->abandoned) {
      return;
    }
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> made = BRep_Tool::Triangulation(own, location);
    if (made != before) {
      BRep_Builder().UpdateFace(original, made);
    }
    face->SetShape(original);
  }

  occ::handle<IMeshTools_MeshAlgo> m_inner;
  MeshSharedPtr m_shared;
};

// The area check of MeshOptions::check_planes (plane_area_fits).
constexpr double kPlaneAreaRelative = 1e-3;
constexpr double kPlaneAreaAbsolute = 1e-3; // mm²

// The area of polygons in a plane's parameters, wire by wire: each wire's
// by the shoelace formula over its edges' polylines (relative to the
// wire's first point, so that a plane far from its origin keeps its
// digits), the outer wire taken as the largest and the others as its
// holes. Orientations are not trusted: each wire's area counts as it is.
class LoopAreas {
public:
  void start_wire() {
    finish_wire();
    m_started = false;
    m_twice = 0.0;
  }

  // One directed segment of the wire.
  void add(const gp_XY& from, const gp_XY& to) {
    if (!m_started) {
      m_origin = from;
      m_started = true;
    }
    const gp_XY a = from - m_origin;
    const gp_XY b = to - m_origin;
    m_twice += a.X() * b.Y() - b.X() * a.Y();
    m_open = true;
  }

  double area() {
    finish_wire();
    double largest = 0.0;
    double sum = 0.0;
    for (const double wire : m_wires) {
      largest = std::max(largest, wire);
      sum += wire;
    }
    return std::max(0.0, largest - (sum - largest));
  }

private:
  void finish_wire() {
    if (m_open) {
      m_wires.push_back(0.5 * std::abs(m_twice));
      m_open = false;
    }
  }

  std::vector<double> m_wires;
  gp_XY m_origin;
  double m_twice = 0.0;
  bool m_started = false;
  bool m_open = false;
};

// The area (mm²) a plane's boundary discretization encloses, from the
// mesher's model of it: each wire's edges' points in the plane's
// parameters (lengths on a plane), each edge in the direction the mesher
// links it (its parameter curve's orientation, reversed or not), the
// wires the mesher leaves out (self-intersecting) left out here too.
double boundary_area(const IMeshData::IFaceHandle& face) {
  const IMeshData::IFacePtr self = face.get();
  LoopAreas loops;
  for (int w = 0; w < face->WiresNb(); ++w) {
    const IMeshData::IWireHandle& wire = face->GetWire(w);
    if (wire->IsSet(IMeshData_SelfIntersectingWire)) {
      continue;
    }
    loops.start_wire();
    for (int e = 0; e < wire->EdgesNb(); ++e) {
      const IMeshData::IEdgePtr& edge = wire->GetEdge(e);
      const IMeshData::IPCurveHandle& pcurve = edge->GetPCurve(self, wire->GetEdgeOrientation(e));
      const TopAbs_Orientation orientation = pcurve->GetOrientation();
      if (orientation != TopAbs_FORWARD && orientation != TopAbs_REVERSED) {
        continue; // an edge inside the face, not a boundary
      }
      const bool reversed = orientation == TopAbs_REVERSED;
      for (int i = 0; i + 1 < pcurve->ParametersNb(); ++i) {
        const gp_XY p = pcurve->GetPoint(i).XY();
        const gp_XY q = pcurve->GetPoint(i + 1).XY();
        if (reversed) {
          loops.add(q, p);
        } else {
          loops.add(p, q);
        }
      }
    }
  }
  return loops.area();
}

// The area (mm²) a triangulation covers, each triangle's counted however it
// turns (a fold adds to it): in its parameter nodes when it has them (a
// plane's are lengths), else in its nodes.
double covered_area(const Poly_Triangulation& triangulation) {
  double twice = 0.0;
  const bool uv = triangulation.HasUVNodes();
  for (int t = 1; t <= triangulation.NbTriangles(); ++t) {
    int a = 0;
    int b = 0;
    int c = 0;
    triangulation.Triangle(t).Get(a, b, c);
    if (uv) {
      const gp_XY pa = triangulation.UVNode(a).XY();
      const gp_XY ab = triangulation.UVNode(b).XY() - pa;
      const gp_XY ac = triangulation.UVNode(c).XY() - pa;
      twice += std::abs(ab.X() * ac.Y() - ab.Y() * ac.X());
    } else {
      const gp_Pnt pa = triangulation.Node(a);
      twice += gp_Vec(pa, triangulation.Node(b)).Crossed(gp_Vec(pa, triangulation.Node(c))).Magnitude();
    }
  }
  return 0.5 * twice;
}

// A plane's triangulation made by `first` (Delabella's), checked
// (MeshOptions::check_planes, mitcad#644): when it leaves the face without
// a new triangulation or marks it failed, or the triangulation's area does
// not fit the area its boundary encloses (plane_area_fits), the face is
// triangulated again by `again` (the default insertion), whose result is
// kept as it comes. Each such face counts in `redone`.
class CheckedPlaneAlgo : public IMeshTools_MeshAlgo {
public:
  CheckedPlaneAlgo(occ::handle<IMeshTools_MeshAlgo> first, occ::handle<IMeshTools_MeshAlgoFactory> again,
                   MeshSharedPtr shared)
      : m_first(std::move(first)), m_again(std::move(again)), m_shared(std::move(shared)) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    const TopoDS_Face& shape = face->GetFace();
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> before = BRep_Tool::Triangulation(shape, location);
    bool fits = false;
    try {
      m_first->Perform(face, parameters, range);
      const occ::handle<Poly_Triangulation> made = BRep_Tool::Triangulation(shape, location);
      fits = !face->IsSet(IMeshData_Failure) && !made.IsNull() && made != before &&
             plane_area_fits(covered_area(*made), boundary_area(face));
    } catch (const Standard_Failure&) {
      fits = false;
    }
    if (fits) {
      return;
    }
    // The default's own start: no triangulation, nothing failed yet. A
    // stopped meshing leaves the face so, as a face it did not reach.
    BRepMesh_ShapeTool::NullifyFace(shape);
    face->UnsetStatus(IMeshData_Failure);
    if (range.UserBreak()) {
      face->SetStatus(IMeshData_UserBreak);
      return;
    }
    ++m_shared->redone;
    const occ::handle<IMeshTools_MeshAlgo> again = m_again->GetAlgo(GeomAbs_Plane, parameters);
    if (again.IsNull()) {
      face->SetStatus(IMeshData_Failure);
      return;
    }
    again->Perform(face, parameters, range);
  }

private:
  occ::handle<IMeshTools_MeshAlgo> m_first;
  occ::handle<IMeshTools_MeshAlgoFactory> m_again;
  MeshSharedPtr m_shared;
};

// How a face over the bound is triangulated again (HeldFaceAlgo): at most
// this many times, each with the deviation raised by the excess times the
// margin and the angles by its square root, the angles to at most the
// largest; one that ends with barely fewer triangles (its boundary's
// points) is not tried again.
constexpr int kHoldAttempts = 3;
constexpr double kHoldMargin = 1.25;
constexpr double kHoldLargestAngle = 30.0 * kDegree;
constexpr double kHoldBarelyFewer = 0.9;
// The bound of a small face (HeldFaceAlgo): this many triangles per square
// of the face's deviation in its area, at least kHoldLeast.
constexpr double kHoldPerDeviationArea = 0.25;
constexpr double kHoldLeast = 200.0;

// A face's triangles on its triangulation, -1 for none.
int triangles_of(const TopoDS_Face& face) {
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation>& triangulation = BRep_Tool::Triangulation(face, location);
  return triangulation.IsNull() ? -1 : triangulation->NbTriangles();
}

// The area (mm²) of a face's triangulation, from its nodes (0 for none).
double triangulated_area(const TopoDS_Face& face) {
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation>& triangulation = BRep_Tool::Triangulation(face, location);
  if (triangulation.IsNull()) {
    return 0.0;
  }
  double twice = 0.0;
  for (int t = 1; t <= triangulation->NbTriangles(); ++t) {
    int a = 0;
    int b = 0;
    int c = 0;
    triangulation->Triangle(t).Get(a, b, c);
    const gp_Pnt pa = triangulation->Node(a);
    twice += gp_Vec(pa, triangulation->Node(b)).Crossed(gp_Vec(pa, triangulation->Node(c))).Magnitude();
  }
  return 0.5 * twice;
}

// A face's triangulation held within the face bound
// (MeshOptions::hold_face_triangles, mitcad#1275): made by `first`, and
// when it has more than `limit` triangles made again by a fresh algorithm
// of `again` with the face's deviation and the angles raised (kHold...),
// until it is within the bound or an attempt barely helps. The last
// triangulation made is the one kept (the face's model, its parameter
// curves' nodes, is the last meshing's); a later attempt that fails is
// followed by the one before it made again, as it was. The face's
// deviation is set back to what it was asked for, and so is the
// triangulation's, so that a meshing with the same options keeps it.
class HeldFaceAlgo : public IMeshTools_MeshAlgo {
public:
  HeldFaceAlgo(occ::handle<IMeshTools_MeshAlgo> first, occ::handle<IMeshTools_MeshAlgoFactory> again,
               std::size_t limit)
      : m_first(std::move(first)), m_again(std::move(again)), m_limit(static_cast<double>(limit)) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    // (Taken before: BRepMesh's deviation control sets the face's
    // deviation to what it measured on the triangulation.)
    const double asked = face->GetDeflection();
    m_first->Perform(face, parameters, range);
    int last = triangles_of(face->GetFace());
    if (last < 0 || face->IsSet(IMeshData_Failure)) {
      return;
    }
    // The bound, and for a small face fewer: a triangle of the face's area
    // per square of its deviation (a B-spline face of 5 mm² got 17 000
    // triangles from its knots, deviation or not), at least kHoldLeast.
    double limit = m_limit;
    if (asked > 0.0) {
      limit = std::min(limit, std::max(kHoldLeast, kHoldPerDeviationArea * triangulated_area(face->GetFace()) /
                                                       (asked * asked)));
    }
    const auto over = [limit](int triangles) { return triangles >= 0 && static_cast<double>(triangles) > limit; };
    if (!over(last)) {
      if (mesh_tracing() && last > 5000) {
        std::fprintf(stderr, "mesh-trace: within %s face of %.3g mm2 within %.0f: %d triangles, deviation %.3g\n",
                     surface_name(face->GetSurface()->GetType()), triangulated_area(face->GetFace()), limit, last,
                     asked);
      }
      return;
    }
    const int first = last;
    double deviation = asked;
    IMeshTools_Parameters coarser = parameters;
    double lastDeviation = asked;
    IMeshTools_Parameters lastParameters = parameters;
    for (int attempt = 0; attempt < kHoldAttempts && over(last) && !range.UserBreak(); ++attempt) {
      const double excess = static_cast<double>(last) / limit * kHoldMargin;
      deviation *= excess;
      const auto wider = [excess](double angle) {
        return std::max(angle, std::min(kHoldLargestAngle, angle * std::sqrt(excess)));
      };
      coarser.Angle = wider(coarser.Angle);
      coarser.AngleInterior = wider(coarser.AngleInterior);
      // The parameters a B-spline's knots give are kept apart by at least
      // this (BRepMesh's NURBS splitter): raised with the deviation, as the
      // deviations some of its range splitters take from the parameters
      // instead of the face (the edges are cut already).
      coarser.MinSize *= excess;
      coarser.Deflection = std::max(coarser.Deflection, deviation);
      coarser.DeflectionInterior = std::max(coarser.DeflectionInterior, deviation);
      const int made = triangulate(face, coarser, deviation, range);
      if (made < 0) {
        if (!range.UserBreak()) {
          triangulate(face, lastParameters, lastDeviation, range);
        }
        break;
      }
      const bool barely = static_cast<double>(made) > kHoldBarelyFewer * static_cast<double>(last);
      last = made;
      lastDeviation = deviation;
      lastParameters = coarser;
      if (barely) {
        break;
      }
    }
    face->SetDeflection(asked);
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation>& made = BRep_Tool::Triangulation(face->GetFace(), location);
    if (!made.IsNull()) {
      made->Deflection(asked);
    }
    if (mesh_tracing()) {
      std::fprintf(stderr, "mesh-trace: held %s face of %.3g mm2 within %.0f: %d -> %d triangles, deviation %.3g -> %.3g\n",
                   surface_name(face->GetSurface()->GetType()), triangulated_area(face->GetFace()), limit, first, last,
                   asked, lastDeviation);
    }
  }

private:
  // The face triangulated again from no triangulation at `deviation`: its
  // triangles, -1 when it failed.
  int triangulate(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters, double deviation,
                  const Message_ProgressRange& range) const {
    BRepMesh_ShapeTool::NullifyFace(face->GetFace());
    face->UnsetStatus(IMeshData_Failure);
    face->SetDeflection(deviation);
    try {
      const occ::handle<IMeshTools_MeshAlgo> algorithm = m_again->GetAlgo(face->GetSurface()->GetType(), parameters);
      if (algorithm.IsNull()) {
        face->SetStatus(IMeshData_Failure);
        return -1;
      }
      algorithm->Perform(face, parameters, range);
    } catch (const Standard_Failure&) {
      face->SetStatus(IMeshData_Failure);
      return -1;
    }
    return face->IsSet(IMeshData_Failure) ? -1 : triangles_of(face->GetFace());
  }

  occ::handle<IMeshTools_MeshAlgo> m_first;
  occ::handle<IMeshTools_MeshAlgoFactory> m_again;
  double m_limit;
};

// A cone, torus or sphere face triangulated with small circles by their
// size (MeshOptions::small_circle_segments, mitcad#879): BRepMesh steps
// round such a face by the smaller of its deviation's arc step and the
// parameters' angle (its range splitters; a torus's and a sphere's
// deviation control then by the interior angle), so a face whose largest
// radius (small_round_radius) its deviation alone cuts more coarsely than
// the angle gets the angle widened as its edges are (small_circle_angle,
// at the face's deviation, which BRepMesh does not halve for faces), and
// the interior angle at least as wide. The face's own deviation is read
// before `inner` (BRepMesh's deviation control sets it to what it
// measured). Outside the face's private copy (PrivateFaceAlgo), whose
// face has no edges to bound a cone's range.
class SmallRoundFaceAlgo : public IMeshTools_MeshAlgo {
public:
  SmallRoundFaceAlgo(occ::handle<IMeshTools_MeshAlgo> inner, int segments)
      : m_inner(std::move(inner)), m_segments(segments) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    IMeshTools_Parameters own = parameters;
    const double radius = small_round_radius(face->GetFace());
    const double wider =
        0.5 * small_circle_angle(radius, 2.0 * face->GetDeflection(), 2.0 * parameters.Angle, m_segments);
    if (wider > parameters.Angle) {
      own.Angle = wider;
      own.AngleInterior = std::max(parameters.AngleInterior, wider);
    }
    m_inner->Perform(face, own, range);
  }

private:
  occ::handle<IMeshTools_MeshAlgo> m_inner;
  int m_segments;
};

// use_fast_plane_algorithms_for_tests: null for Delabella's.
std::mutex g_fastPlanesMutex;
occ::handle<IMeshTools_MeshAlgoFactory> g_fastPlanesForTests;
// use_face_algorithms_for_tests: null for the default (CSF_MeshAlgo).
occ::handle<IMeshTools_MeshAlgoFactory> g_facesForTests;

occ::handle<IMeshTools_MeshAlgoFactory> fast_plane_algorithms() {
  const std::lock_guard<std::mutex> lock(g_fastPlanesMutex);
  if (!g_fastPlanesForTests.IsNull()) {
    return g_fastPlanesForTests;
  }
  return new BRepMesh_DelabellaMeshAlgoFactory();
}

occ::handle<IMeshTools_MeshAlgoFactory> default_algorithms() {
  {
    const std::lock_guard<std::mutex> lock(g_fastPlanesMutex);
    if (!g_facesForTests.IsNull()) {
      return g_facesForTests;
    }
  }
  TCollection_AsciiString algorithm = OSD_Environment("CSF_MeshAlgo").Value();
  algorithm.LowerCase();
  if (algorithm == "delabella" || algorithm == "1") {
    return new BRepMesh_DelabellaMeshAlgoFactory();
  }
  return new BRepMesh_MeshAlgoFactory();
}

// The tori's algorithms (mitcad#1624): Delabella's base triangulation of the
// boundary's and the inside's nodes together (OCCT's
// BRepMesh_DelabellaMeshAlgoFactory, its signs exact since OCCT port patch
// 1070), whatever CSF_MeshAlgo says for the other faces;
// use_face_algorithms_for_tests' instead when it gives some. BRepMesh's
// torus range splitter puts the inside's nodes on a grid of the edges'
// parameters: round a rounding of 0.25 mm on a circle of 10 mm, columns
// 1.3 mm apart and rows 0.13 mm apart (its seam's 30 degree steps, a small
// circle's, mitcad#879) in the face's scaled parameters, every cell's four
// corners on one circle. The default's
// Watson insertion (BRepMesh_Delaun) left many of those nodes out, inserted
// after the boundary's triangulation or with it, and spanned triangles over
// the whole rounding from one of its edges to the other: a funnel's rim
// rounding was 0.12 mm off at its body's 0.04 mm, its triangles' normals
// up to 64 degrees off the surface's, and its outline in the view showed
// saw teeth. Delabella's triangulation of the same nodes keeps every one
// (0.025 mm off; 565 triangles for the 503 of the default).
occ::handle<IMeshTools_MeshAlgoFactory> torus_algorithms() {
  {
    const std::lock_guard<std::mutex> lock(g_fastPlanesMutex);
    if (!g_facesForTests.IsNull()) {
      return g_facesForTests;
    }
  }
  return new BRepMesh_DelabellaMeshAlgoFactory();
}

// The faces' algorithms: BRepMesh_Context's default (CSF_MeshAlgo), with
// planes through Delabella's base triangulation when `fast_planes`
// (MeshOptions::fast_planes), checked and made again by the default when
// `check_planes` too (CheckedPlaneAlgo, counted in the shared `redone`),
// tori through Delabella's base triangulation always (torus_algorithms),
// the other faces held within `held` triangles when that is not 0
// (HeldFaceAlgo, MeshOptions::hold_face_triangles), each on a private copy
// of its face (PrivateFaceAlgo), cones, tori and spheres with their angles
// widened by their size when `small_circles` is above 0 (SmallRoundFaceAlgo,
// MeshOptions::small_circle_segments), each timed when the meshing is.
class FaceAlgoFactory : public IMeshTools_MeshAlgoFactory {
public:
  FaceAlgoFactory(bool fast_planes, bool check_planes, std::size_t held, int small_circles, MeshSharedPtr shared)
      : m_default(default_algorithms()), m_tori(torus_algorithms()), m_check(fast_planes && check_planes),
        m_held(held), m_smallCircles(small_circles), m_shared(std::move(shared)) {
    m_planes = fast_planes ? fast_plane_algorithms() : m_default;
  }

  occ::handle<IMeshTools_MeshAlgo> GetAlgo(const GeomAbs_SurfaceType type,
                                           const IMeshTools_Parameters& parameters) const override {
    const occ::handle<IMeshTools_MeshAlgoFactory>& own = type == GeomAbs_Torus ? m_tori : m_default;
    occ::handle<IMeshTools_MeshAlgo> algorithm = (type == GeomAbs_Plane ? m_planes : own)->GetAlgo(type, parameters);
    if (algorithm.IsNull()) {
      return algorithm;
    }
    if (m_check && type == GeomAbs_Plane) {
      algorithm = new CheckedPlaneAlgo(algorithm, m_default, m_shared);
    }
    if (m_held > 0 && type != GeomAbs_Plane) {
      algorithm = new HeldFaceAlgo(algorithm, own, m_held);
    }
    algorithm = new PrivateFaceAlgo(algorithm, m_shared);
    if (m_smallCircles > 0 && (type == GeomAbs_Cone || type == GeomAbs_Torus || type == GeomAbs_Sphere)) {
      algorithm = new SmallRoundFaceAlgo(algorithm, m_smallCircles);
    }
    if (m_shared->timed) {
      algorithm = new TimedAlgo(algorithm, m_shared);
    }
    return algorithm;
  }

private:
  occ::handle<IMeshTools_MeshAlgoFactory> m_default;
  occ::handle<IMeshTools_MeshAlgoFactory> m_tori;
  occ::handle<IMeshTools_MeshAlgoFactory> m_planes;
  bool m_check;
  std::size_t m_held;
  int m_smallCircles;
  MeshSharedPtr m_shared;
};

// The stop of a face's triangulation on a worker of StoppableFaceDiscret:
// the meshing's (MeshShared::stop), which the meshing's own thread sets
// from its range. The workers never read that range: it may be gone once
// the meshing is abandoned.
class SharedStop : public Message_ProgressIndicator {
public:
  explicit SharedStop(MeshSharedPtr shared) : m_shared(std::move(shared)) {}
  bool UserBreak() override { return m_shared->stop.load(); }
  void Show(const Message_ProgressScope&, const bool) override {}

private:
  MeshSharedPtr m_shared;
};

// The workers of abandoned meshings still running (abandoned_faces).
std::atomic<std::size_t> g_abandonedWorkers{0};

// The faces of one meshing, triangulated one at a time by each worker, as
// BRepMesh_FaceDiscret triangulates them.
struct FaceWork {
  FaceWork(occ::handle<IMeshData_Model> model_, occ::handle<IMeshTools_MeshAlgoFactory> factory_,
           const IMeshTools_Parameters& parameters_, MeshSharedPtr shared_)
      : model(std::move(model_)), factory(std::move(factory_)), parameters(parameters_), shared(std::move(shared_)) {}

  // BRepMesh_FaceDiscret::process, which catches OCCT's exceptions only:
  // here any exception leaves the face failed (mitcad#936). On a worker of
  // its own, one that escaped would end the process (std::terminate); on
  // OCCT's pool, it was thrown again on the meshing's thread. A failed
  // allocation (OCCT's own allocations throw Standard_OutOfMemory, plain
  // C++ ones std::bad_alloc: Delabella's, the plane check's) is noted for
  // the meshing to report.
  void process(int index, const Message_ProgressRange& range) const {
    const IMeshData::IFaceHandle& face = model->GetFace(index);
    if (face->IsSet(IMeshData_Failure) || face->IsSet(IMeshData_Reused)) {
      return;
    }
    try {
      OCC_CATCH_SIGNALS
      const occ::handle<IMeshTools_MeshAlgo> algorithm =
          factory->GetAlgo(face->GetSurface()->GetType(), parameters);
      if (algorithm.IsNull()) {
        face->SetStatus(IMeshData_Failure);
        return;
      }
      if (shared->stop.load() || !range.More()) {
        face->SetStatus(IMeshData_UserBreak);
        return;
      }
      algorithm->Perform(face, parameters, range);
    } catch (const Standard_OutOfMemory&) {
      shared->out_of_memory = true;
      face->SetStatus(IMeshData_Failure);
    } catch (const Standard_Failure&) {
      face->SetStatus(IMeshData_Failure);
    } catch (const std::bad_alloc&) {
      shared->out_of_memory = true;
      face->SetStatus(IMeshData_Failure);
    } catch (const std::exception&) {
      face->SetStatus(IMeshData_Failure);
    } catch (...) {
      face->SetStatus(IMeshData_Failure);
    }
  }

  // A worker: the next face not taken, until none is left. Returns whether
  // the meshing abandoned it meanwhile (counted in g_abandonedWorkers). An
  // exception outside a face (its stop's allocation) ends the worker; the
  // others take the faces left.
  bool run() {
    try {
      OSD::SetThreadLocalSignal(OSD::SignalMode(), catch_fpe);
      const occ::handle<SharedStop> stop = new SharedStop(shared);
      for (int index = next++; index < model->FacesNb(); index = next++) {
        process(index, stop->Start());
      }
    } catch (const Standard_OutOfMemory&) {
      shared->out_of_memory = true;
    } catch (const std::bad_alloc&) {
      shared->out_of_memory = true;
    } catch (...) {
      // The faces it did not take are the other workers'.
    }
    const std::lock_guard<std::mutex> lock(mutex);
    --running;
    idle.notify_all();
    return abandoned;
  }

  const occ::handle<IMeshData_Model> model;
  const occ::handle<IMeshTools_MeshAlgoFactory> factory;
  const IMeshTools_Parameters parameters;
  const MeshSharedPtr shared;
  const bool catch_fpe = OSD::ToCatchFloatingSignals();
  std::atomic<int> next{0};
  std::mutex mutex;
  std::condition_variable idle;
  int running = 0;
  // Set, under `mutex`, when the meshing gives up the workers still
  // running; each of them counts in g_abandonedWorkers until it ends.
  bool abandoned = false;
};

// A detached worker of StoppableFaceDiscret (mitcad#936): its faces, then
// the meshing's data released (the mesher's model, freed inside OCCT when
// this is the last holder, as for an abandoned meshing) while the worker
// still counts as abandoned, if it was.
void work_on_faces(std::shared_ptr<FaceWork> work) {
  const bool abandoned = work->run();
  work.reset();
  if (abandoned) {
    --g_abandonedWorkers;
  }
}

// BRepMesh's face discretization (BRepMesh_FaceDiscret) whose stop does not
// wait for a face that does not end (mitcad#846). OCCT's algorithms test
// the stop between faces, and inside one only in a part of the default
// insertion; BRepMesh_Delaun::frontierAdjust (mitcad#589) and Delabella's
// base triangulation not at all, and OCCT's thread pool waits for every
// face it started. With a range that can be stopped, the faces are taken
// one at a time by detached workers of the meshing's own, which hold what
// they need (FaceWork, MeshShared), while this thread watches the range.
// Once stopped, the workers take no other face and the faces under way get
// kMeshAbandonSeconds to end; then the meshing is abandoned: the faces still
// under way are counted (MeshShared::left), their results discarded when
// they end (PrivateFaceAlgo), and this returns as stopped; the workers
// still running count in abandoned_faces until they end (mitcad#936).
// Without such a range, BRepMesh's own parallel loop.
class StoppableFaceDiscret : public IMeshTools_ModelAlgo {
public:
  StoppableFaceDiscret(occ::handle<IMeshTools_MeshAlgoFactory> factory, MeshSharedPtr shared)
      : m_factory(std::move(factory)), m_shared(std::move(shared)) {}

protected:
  bool performInternal(const occ::handle<IMeshData_Model>& model, const IMeshTools_Parameters& parameters,
                       const Message_ProgressRange& range) override {
    if (model.IsNull()) {
      return false;
    }
    const int faces = model->FacesNb();
    const auto work = std::make_shared<FaceWork>(model, m_factory, parameters, m_shared);
    if (!range.IsActive()) {
      OSD_Parallel::For(
          0, faces, [&work](int index) { work->process(index, Message_ProgressRange()); },
          !parameters.InParallel || faces <= 1);
      return true;
    }
    if (!range.More()) {
      return false; // stopped before any face, as BRepMesh_FaceDiscret
    }
    const int threads = parameters.InParallel ? std::max(1, std::min(faces, OSD_Parallel::NbLogicalProcessors())) : 1;
    {
      const std::lock_guard<std::mutex> lock(work->mutex);
      work->running = threads;
    }
    int started = 0;
    for (int i = 0; i < threads; ++i) {
      try {
        std::thread(work_on_faces, work).detach();
        ++started;
      } catch (const std::system_error&) {
        const std::lock_guard<std::mutex> lock(work->mutex);
        --work->running;
      }
    }
    if (started == 0) {
      {
        const std::lock_guard<std::mutex> lock(work->mutex);
        work->running = 1;
      }
      work->run(); // no thread to be had: here, to the end
    }
    const auto grace = std::chrono::duration_cast<Clock::duration>(std::chrono::duration<double>(kMeshAbandonSeconds));
    bool stopped = false;
    Clock::time_point deadline;
    std::unique_lock<std::mutex> lock(work->mutex);
    while (work->running > 0) {
      work->idle.wait_for(lock, std::chrono::milliseconds(20));
      if (work->running == 0) {
        break;
      }
      if (!stopped) {
        lock.unlock();
        stopped = !range.More();
        lock.lock();
        if (stopped) {
          m_shared->stop = true;
          deadline = Clock::now() + grace;
        }
      } else if (Clock::now() >= deadline) {
        break;
      }
    }
    const bool abandon = work->running > 0;
    if (abandon) {
      // Counted before any of them can end (under the work's mutex, which
      // each takes to end; work_on_faces counts it off).
      work->abandoned = true;
      g_abandonedWorkers += static_cast<std::size_t>(work->running);
    }
    lock.unlock();
    if (abandon) {
      const std::lock_guard<std::mutex> guard(m_shared->mutex);
      m_shared->abandoned = true;
      m_shared->left = m_shared->busy;
      return false;
    }
    return range.More();
  }

private:
  occ::handle<IMeshTools_MeshAlgoFactory> m_factory;
  MeshSharedPtr m_shared;
};

// BRepMesh's own context, with small circles' edges cut by their size when
// the options say so (widen_small_circle_edges, mitcad#879), the edges of
// large cylinders and cones discretized at their faces' floor
// (coarsen_round_edges) and each face's
// deviation raised to its floor once the healer has computed the
// deviations from the edges' (BRepMesh's pre-processor then keeps a
// triangulation that meets it). The floors are computed over the faces in
// parallel when the meshing is (each task only reads its own face and sets
// its own deviation); a face whose floor cannot be computed keeps
// BRepMesh's deviation. Planes through Delabella with `fast_planes`,
// checked with `check_planes`, other faces held within the bound with
// `hold_face_triangles` (MeshOptions). A meshing whose deviation is the
// shape's by its size (the parameters' above `linear_deflection`,
// MeshOptions::size_deviation) records `linear_deflection` in the
// triangulations of the faces it made at that deviation (not those a floor
// or their edges made coarser). With a report, each phase is timed and so
// is each face (MeshReport).
class BoundedContext : public BRepMesh_Context {
public:
  BoundedContext(const MeshOptions& options, MeshReport* report)
      : m_faceTriangles(options.face_triangles), m_smallCircles(options.small_circle_segments),
        m_asked(options.linear_deflection), m_record(options.size_deviation > 0.0 || options.hold_face_triangles),
        m_report(report), m_shared(std::make_shared<MeshShared>()) {
    m_shared->timed = m_report != nullptr;
    const std::size_t held = options.hold_face_triangles ? options.face_triangles : 0;
    SetFaceDiscret(new StoppableFaceDiscret(
        new FaceAlgoFactory(options.fast_planes, options.check_planes, held, m_smallCircles, m_shared), m_shared));
  }

  bool BuildModel() override {
    const Clock::time_point start = Clock::now();
    const bool done = BRepMesh_Context::BuildModel();
    add(&MeshReport::build, start);
    return done;
  }

  bool DiscretizeEdges() override {
    const Clock::time_point start = Clock::now();
    if (!BRepMesh_Context::DiscretizeEdges()) {
      return false;
    }
    widen_small_circle_edges(GetModel(), GetParameters(), m_smallCircles);
    coarsen_round_edges(GetModel(), GetParameters(), m_faceTriangles);
    add(&MeshReport::edges, start);
    return true;
  }

  bool HealModel() override {
    const Clock::time_point start = Clock::now();
    if (!BRepMesh_Context::HealModel()) {
      return false;
    }
    const occ::handle<IMeshData_Model>& model = GetModel();
    separate_close_wires(model, GetParameters());
    m_floored.assign(model.IsNull() ? 0 : static_cast<std::size_t>(model->FacesNb()), 0);
    if (m_faceTriangles == 0 || model.IsNull()) {
      add(&MeshReport::heal, start);
      return true;
    }
    const std::size_t limit = m_faceTriangles;
    std::vector<char>& floored = m_floored;
    OSD_Parallel::For(
        0, model->FacesNb(),
        [&model, &floored, limit](int i) {
          const IMeshData::IFaceHandle& face = model->GetFace(i);
          try {
            const double floor = face_deviation_floor(face->GetFace(), face->GetDeflection(), limit);
            if (floor > face->GetDeflection()) {
              face->SetDeflection(floor);
              floored[static_cast<std::size_t>(i)] = 1;
            }
          } catch (const Standard_Failure&) {
            // BRepMesh's own deviation.
          }
        },
        !GetParameters().InParallel || model->FacesNb() < 8);
    add(&MeshReport::heal, start);
    return true;
  }

  bool PreProcessModel() override {
    const Clock::time_point start = Clock::now();
    const bool done = BRepMesh_Context::PreProcessModel();
    add(&MeshReport::check, start);
    return done;
  }

  bool DiscretizeFaces(const Message_ProgressRange& range) override {
    const Clock::time_point start = Clock::now();
    const bool done = BRepMesh_Context::DiscretizeFaces(range);
    add(&MeshReport::triangulate, start);
    if (m_report != nullptr) {
      const std::lock_guard<std::mutex> lock(m_shared->mutex);
      if (m_shared->abandoned) {
        m_report->abandoned += m_shared->left;
      }
    }
    return done;
  }

  bool PostProcessModel() override {
    const Clock::time_point start = Clock::now();
    const bool done = BRepMesh_Context::PostProcessModel();
    recordAsked();
    add(&MeshReport::write, start);
    count();
    return done;
  }

  // Whether an allocation failed in a face's triangulation (mitcad#936).
  bool outOfMemory() const { return m_shared->out_of_memory.load(); }

private:
  void add(double MeshReport::*phase, Clock::time_point start) {
    if (m_report != nullptr) {
      m_report->*phase += milliseconds_since(start);
    }
  }

  // The deviation asked for (MeshOptions::linear_deflection) in the
  // triangulations this meshing made with the view's options (by size or
  // held), except those of the faces a floor raised: the shape's deviation
  // by its size, the hold and BRepMesh's own raises (tolerances) are the
  // view's meshing of what was asked. Their edges' polygons keep the
  // deviation they were cut at.
  void recordAsked() {
    const occ::handle<IMeshData_Model>& model = GetModel();
    if (!m_record || model.IsNull() || GetParameters().Relative) {
      return;
    }
    for (int i = 0; i < model->FacesNb(); ++i) {
      const IMeshData::IFaceHandle& face = model->GetFace(i);
      const auto index = static_cast<std::size_t>(i);
      if (face->IsSet(IMeshData_Reused) || face->IsSet(IMeshData_Failure) ||
          (index < m_floored.size() && m_floored[index] != 0)) {
        continue;
      }
      TopLoc_Location location;
      const occ::handle<Poly_Triangulation>& made = BRep_Tool::Triangulation(face->GetFace(), location);
      if (!made.IsNull() && made->Deflection() > m_asked) {
        made->Deflection(m_asked);
      }
    }
  }

  // The faces kept, meshed and failed, by surface kind, and the slowest
  // (added to what the report has: one meshing per deviation,
  // mesh_all_in_place).
  void count() {
    const occ::handle<IMeshData_Model>& model = GetModel();
    if (m_report == nullptr || model.IsNull()) {
      return;
    }
    std::map<std::string, std::size_t> surfaces;
    for (int i = 0; i < model->FacesNb(); ++i) {
      const IMeshData::IFaceHandle& face = model->GetFace(i);
      ++m_report->faces;
      TopLoc_Location location;
      if (face->IsSet(IMeshData_Reused)) {
        ++m_report->kept;
      } else if (face->IsSet(IMeshData_Failure) || BRep_Tool::Triangulation(face->GetFace(), location).IsNull()) {
        ++m_report->failed;
      } else {
        ++m_report->meshed;
        ++surfaces[surface_name(face->GetSurface()->GetType())];
      }
    }
    for (const auto& [surface, count] : surfaces) {
      bool known = false;
      for (std::pair<std::string, std::size_t>& entry : m_report->meshed_by_surface) {
        if (entry.first == surface) {
          entry.second += count;
          known = true;
        }
      }
      if (!known) {
        m_report->meshed_by_surface.emplace_back(surface, count);
      }
    }
    m_report->planes_redone += m_shared->redone.load();
    const std::lock_guard<std::mutex> lock(m_shared->mutex);
    const std::vector<MeshReport::FaceTime>& times = m_shared->times;
    if (m_report->every_face) {
      m_report->every_face_time.insert(m_report->every_face_time.end(), times.begin(), times.end());
    }
    std::vector<MeshReport::FaceTime>& slowest = m_report->slowest;
    slowest.insert(slowest.end(), times.begin(), times.end());
    std::sort(slowest.begin(), slowest.end(), [](const MeshReport::FaceTime& a, const MeshReport::FaceTime& b) {
      return a.milliseconds > b.milliseconds;
    });
    if (slowest.size() > 5) {
      slowest.resize(5);
    }
  }

  std::size_t m_faceTriangles;
  int m_smallCircles; // MeshOptions::small_circle_segments
  double m_asked; // MeshOptions::linear_deflection
  // Whether the faces made record m_asked (recordAsked: the view's options),
  // and which faces a floor raised (HealModel), which do not.
  bool m_record;
  std::vector<char> m_floored;
  MeshReport* m_report;
  MeshSharedPtr m_shared; // with the faces' workers and algorithms
};

// Faces the presentations' algorithm triangulated (presentation_faces_meshed).
std::atomic<std::size_t> g_presentationFaces{0};

// BRepMesh_IncrementalMesh with that context. A presentation's meshing
// (BoundedFactory) counts the faces it meshed and traces itself. An
// allocation that failed in a face's triangulation (the face is left
// failed) makes the meshing throw Error "triangulation: out of memory" once
// it ends (mitcad#936; geometry's operations report such failures alike),
// except a presentation's, which OCCT's viewer runs on the UI thread and
// which draws the faces it has.
class BoundedMesh : public BRepMesh_IncrementalMesh {
public:
  explicit BoundedMesh(const MeshOptions& options, MeshReport* report = nullptr, bool presentation = false)
      : m_options(options), m_report(report), m_presentation(presentation) {}

  using BRepMesh_IncrementalMesh::Perform;
  void Perform(const Message_ProgressRange& range = Message_ProgressRange()) override {
    MeshReport own;
    MeshReport* report = m_presentation ? &own : m_report;
    const Clock::time_point start = Clock::now();
    BoundedContext* context = new BoundedContext(m_options, report);
    const occ::handle<IMeshTools_Context> held = context;
    BRepMesh_IncrementalMesh::Perform(held, range);
    if (report != nullptr) {
      report->total += milliseconds_since(start);
    }
    if (m_presentation) {
      g_presentationFaces += own.meshed;
      if (mesh_tracing()) {
        std::fprintf(stderr, "mesh-trace: presentation %s\n", own.line().c_str());
      }
      return;
    }
    if (context->outOfMemory()) {
      throw Error("triangulation: out of memory");
    }
  }

private:
  MeshOptions m_options;
  MeshReport* m_report;
  bool m_presentation;
};

// A meshing's parameters for `options` at `deviation` (the shape's,
// shape_deviation).
IMeshTools_Parameters parameters_of(const MeshOptions& options, double deviation) {
  if (!(options.linear_deflection > 0.0) || !(options.angular_deflection > 0.0)) {
    throw Error("mesh deflections must be greater than zero");
  }
  IMeshTools_Parameters parameters;
  parameters.Deflection = std::max(deviation, options.linear_deflection);
  parameters.Angle = options.angular_deflection;
  if (options.interior_angle > 0.0) {
    parameters.AngleInterior = options.interior_angle;
  }
  parameters.Relative = options.relative;
  parameters.InParallel = true;
  return parameters;
}

// The presentations' meshing algorithm (use_bounded_presentation_meshes):
// the drawer's deviation as the options' linear_deflection, its angle not
// taken (its curves'), planes by the default insertion.
class BoundedFactory : public BRepMesh_DiscretAlgoFactory {
public:
  static constexpr const char* kName = "MitcadBounded";

  explicit BoundedFactory(const MeshOptions& options) : BRepMesh_DiscretAlgoFactory(kName), m_options(options) {
    m_options.fast_planes = false;
    m_options.check_planes = false;
  }

  occ::handle<BRepMesh_DiscretRoot> CreateAlgorithm(const TopoDS_Shape& shape, double deviation,
                                                    double /*angle*/) override {
    MeshOptions options = m_options;
    if (deviation > 0.0) {
      options.linear_deflection = deviation;
    }
    occ::handle<BoundedMesh> mesher = new BoundedMesh(options, nullptr, true);
    try {
      mesher->ChangeParameters() = parameters_of(options, shape_deviation(shape, options));
    } catch (const Error&) {
      mesher->ChangeParameters().Deflection = deviation;
      mesher->ChangeParameters().Angle = options.angular_deflection;
    }
    mesher->ChangeParameters().InParallel = BRepMesh_IncrementalMesh::IsParallelDefault();
    mesher->SetShape(shape);
    return mesher;
  }

private:
  MeshOptions m_options;
};

} // namespace

bool mesh_tracing() {
  static const bool on = [] {
#ifdef _MSC_VER
    char* value = nullptr;
    std::size_t length = 0;
    if (_dupenv_s(&value, &length, "MITCAD_MESH_TRACE") != 0 || value == nullptr) {
      return false;
    }
    std::free(value);
    return true;
#else
    return std::getenv("MITCAD_MESH_TRACE") != nullptr;
#endif
  }();
  return on;
}

std::size_t presentation_faces_meshed() { return g_presentationFaces.load(); }

std::size_t abandoned_faces() { return g_abandonedWorkers.load(); }

std::string MeshReport::line() const {
  char text[256];
  std::snprintf(text, sizeof text,
                "faces=%zu kept=%zu meshed=%zu failed=%zu build=%.1f edges=%.1f heal=%.1f check=%.1f "
                "triangulate=%.1f write=%.1f total=%.1f ms",
                faces, kept, meshed, failed, build, edges, heal, check, triangulate, write, total);
  std::string out = text;
  if (!meshed_by_surface.empty()) {
    out += "; meshed";
    for (std::size_t i = 0; i < meshed_by_surface.size(); ++i) {
      out += (i == 0 ? " " : ", ") + meshed_by_surface[i].first + " " + std::to_string(meshed_by_surface[i].second);
    }
  }
  if (!slowest.empty()) {
    out += "; slowest";
    for (std::size_t i = 0; i < slowest.size(); ++i) {
      std::snprintf(text, sizeof text, "%s %s wires=%d points=%d %.1f ms", i == 0 ? "" : ",",
                    slowest[i].surface.c_str(), slowest[i].wires, slowest[i].points, slowest[i].milliseconds);
      out += text;
    }
  }
  if (planes_redone > 0) {
    out += "; planes redone " + std::to_string(planes_redone);
  }
  if (abandoned > 0) {
    out += "; abandoned " + std::to_string(abandoned);
  }
  return out;
}

bool plane_area_fits(double triangulated, double boundary) {
  return std::isfinite(triangulated) && std::isfinite(boundary) &&
         std::abs(triangulated - boundary) <= std::max(kPlaneAreaRelative * std::abs(boundary), kPlaneAreaAbsolute);
}

PlaneMeshArea plane_mesh_area(const TopoDS_Face& face) {
  PlaneMeshArea area;
  if (face.IsNull() || BRepAdaptor_Surface(face, false).GetType() != GeomAbs_Plane) {
    return area;
  }
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation> triangulation = BRep_Tool::Triangulation(face, location);
  if (triangulation.IsNull() || !triangulation->HasUVNodes()) {
    return area;
  }
  // Each wire's edges in the face's orientation, as the mesher links them
  // (a reversed face's wires run the other way round, which the areas'
  // own signs do not mind).
  const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  LoopAreas loops;
  for (TopExp_Explorer wires(forward, TopAbs_WIRE); wires.More(); wires.Next()) {
    loops.start_wire();
    for (TopExp_Explorer edges(wires.Current(), TopAbs_EDGE); edges.More(); edges.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
      if (edge.Orientation() != TopAbs_FORWARD && edge.Orientation() != TopAbs_REVERSED) {
        continue;
      }
      const occ::handle<Poly_PolygonOnTriangulation> polygon =
          BRep_Tool::PolygonOnTriangulation(edge, triangulation, location);
      if (polygon.IsNull()) {
        return PlaneMeshArea{};
      }
      const bool reversed = edge.Orientation() == TopAbs_REVERSED;
      for (int i = 1; i < polygon->NbNodes(); ++i) {
        const gp_XY p = triangulation->UVNode(polygon->Node(i)).XY();
        const gp_XY q = triangulation->UVNode(polygon->Node(i + 1)).XY();
        if (reversed) {
          loops.add(q, p);
        } else {
          loops.add(p, q);
        }
      }
    }
  }
  area.boundary = loops.area();
  area.triangles = covered_area(*triangulation);
  return area;
}

void use_fast_plane_algorithms_for_tests(const occ::handle<IMeshTools_MeshAlgoFactory>& factory) {
  const std::lock_guard<std::mutex> lock(g_fastPlanesMutex);
  g_fastPlanesForTests = factory;
}

void use_face_algorithms_for_tests(const occ::handle<IMeshTools_MeshAlgoFactory>& factory) {
  const std::lock_guard<std::mutex> lock(g_fastPlanesMutex);
  g_facesForTests = factory;
}

double face_deviation_floor(const TopoDS_Face& face, double deviation, std::size_t face_triangles) {
  if (face_triangles == 0 || face.IsNull()) {
    return 0.0;
  }
  // A cylinder or a cone: a turn round it in the bound's pieces (mitcad#438),
  // whatever its size, so that the faces of one cylinder agree.
  try {
    const double radius = round_radius(face);
    if (radius > 0.0) {
      const double floor = turn_deviation(radius, face_triangles);
      return floor > deviation ? floor : 0.0;
    }
  } catch (const Standard_Failure&) {
    return 0.0;
  }
  const auto limit = static_cast<double>(face_triangles);
  Bnd_Box box;
  BRepBndLib::Add(face, box, true);
  if (box.IsVoid() || box.IsOpen()) {
    return 0.0;
  }
  if (kTrianglesPerCurvature * kCurvaturePerDiagonal * std::sqrt(box.SquareExtent()) / limit <= deviation) {
    return 0.0;
  }
  return kTrianglesPerCurvature * curvature_integral(face) / limit;
}

double shape_deviation(const TopoDS_Shape& shape, const MeshOptions& options) {
  const double asked = options.linear_deflection;
  if (!(options.size_deviation > 0.0) || options.relative || !(asked > 0.0) || shape.IsNull()) {
    return asked;
  }
  Bnd_Box box;
  try {
    // From the geometry, not the triangulations: the same before and after
    // the shape is meshed.
    BRepBndLib::Add(shape.Located(TopLoc_Location()), box, false);
  } catch (const Standard_Failure&) {
    return asked;
  }
  if (box.IsVoid() || box.IsOpen()) {
    return asked;
  }
  const double sized = options.size_deviation * std::sqrt(box.SquareExtent());
  if (!std::isfinite(sized) || !(sized > asked)) {
    return asked;
  }
  if (options.size_deviation_max > 0.0 && sized >= options.size_deviation_max) {
    return std::max(asked, options.size_deviation_max);
  }
  // Steps of the square root of 2 up from the deviation asked for.
  const double steps = std::floor(2.0 * std::log2(sized / asked) + 1e-9);
  return asked * std::pow(2.0, steps / 2.0);
}

double small_circle_angle(double radius, double deviation, double angle, int segments) {
  if (segments <= 0 || !(radius > 0.0) || !(deviation > 0.0) || !std::isfinite(radius) || !std::isfinite(deviation)) {
    return angle;
  }
  // BRepMesh's step for the deviation (GCPnts_TangentialDeflection's arc
  // step at half the edge's deviation); a sag of the whole radius or more
  // allows any step.
  const double sag = 0.5 * deviation / radius;
  const double step = sag < 1.0 ? 2.0 * std::acos(1.0 - sag) : kPi;
  if (!(step > 0.5 * angle)) {
    return angle; // the deviation cuts it finer than the angle: as it is
  }
  // A hair over the step of `segments` pieces, so that a turn divided by it
  // rounds up to `segments` and not one more.
  const double least = 2.0 * kPi / static_cast<double>(segments) * (1.0 + 1e-9);
  return 2.0 * std::min(step, least);
}

bool has_triangulation(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location).IsNull()) {
      return false;
    }
  }
  return true;
}

namespace {

// What makes a face's triangulation valid for another face
// (reuse_triangulations): its surface and each edge's geometry, compared
// by identity (the copies an operation makes share them), and the edges'
// end points by value.
struct EdgeKey {
  // Which edge it is in its face (not compared): an edge may bound a face
  // twice in other placements (a prism's side between its base's edge
  // and the same edge moved to the top), with a polygon for each.
  const TopoDS_TShape* tshape = nullptr;
  TopLoc_Location placement; // compared
  TopAbs_Orientation orientation = TopAbs_FORWARD;
  const Geom_Curve* curve = nullptr;
  TopLoc_Location location;
  double first = 0.0;
  double last = 0.0;
  const Geom2d_Curve* pcurve = nullptr;
  const Geom2d_Curve* pcurve2 = nullptr; // a seam's other
  gp_Pnt start;
  gp_Pnt end;
  bool degenerated = false;

  bool same(const EdgeKey& other) const {
    return orientation == other.orientation && placement.IsEqual(other.placement) && curve == other.curve &&
           location.IsEqual(other.location) &&
           first == other.first && last == other.last && pcurve == other.pcurve && pcurve2 == other.pcurve2 &&
           start.X() == other.start.X() && start.Y() == other.start.Y() && start.Z() == other.start.Z() &&
           end.X() == other.end.X() && end.Y() == other.end.Y() && end.Z() == other.end.Z() &&
           degenerated == other.degenerated;
  }
};

struct FaceKey {
  TopoDS_Face face; // located as explored
  const Geom_Surface* surface = nullptr;
  TopLoc_Location location;
  bool natural = false;
  std::vector<EdgeKey> edges; // in the face's order, each sub-shape once

  bool same(const FaceKey& other) const {
    if (surface != other.surface || !location.IsEqual(other.location) || natural != other.natural ||
        edges.size() != other.edges.size()) {
      return false;
    }
    for (std::size_t i = 0; i < edges.size(); ++i) {
      if (!edges[i].same(other.edges[i])) {
        return false;
      }
    }
    return true;
  }
};

FaceKey face_key(const TopoDS_Face& face) {
  FaceKey key;
  key.face = face;
  TopLoc_Location location;
  key.surface = BRep_Tool::Surface(face, location).get();
  key.location = location;
  key.natural = BRep_Tool::NaturalRestriction(face);
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (std::any_of(key.edges.begin(), key.edges.end(), [&edge](const EdgeKey& seen) {
          return seen.tshape == edge.TShape().get() && seen.placement.IsEqual(edge.Location());
        })) {
      continue; // a seam's second use
    }
    EdgeKey e;
    e.tshape = edge.TShape().get();
    e.placement = edge.Location();
    e.orientation = edge.Orientation();
    TopLoc_Location curve_location;
    e.curve = BRep_Tool::Curve(edge, curve_location, e.first, e.last).get();
    e.location = curve_location;
    // The edge's parameter curve on the face; none on a plane, where it is
    // the curve's, compared already (OCCT makes one anew from it when the
    // edge stores none, and a copy may store it).
    const bool plane = key.surface != nullptr && key.surface->IsKind(STANDARD_TYPE(Geom_Plane));
    const auto stored = [&edge, &face, plane](TopAbs_Orientation orientation) -> const Geom2d_Curve* {
      if (plane) {
        return nullptr;
      }
      double f = 0.0;
      double l = 0.0;
      bool kept = false;
      const occ::handle<Geom2d_Curve> pcurve =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Oriented(orientation)), face, f, l, &kept);
      return kept ? pcurve.get() : nullptr;
    };
    e.pcurve = stored(TopAbs_FORWARD);
    e.pcurve2 = BRep_Tool::IsClosed(edge, face) ? stored(TopAbs_REVERSED) : nullptr;
    TopoDS_Vertex v1;
    TopoDS_Vertex v2;
    TopExp::Vertices(edge, v1, v2);
    if (!v1.IsNull()) {
      e.start = BRep_Tool::Pnt(v1);
    }
    if (!v2.IsNull()) {
      e.end = BRep_Tool::Pnt(v2);
    }
    e.degenerated = BRep_Tool::Degenerated(edge);
    key.edges.push_back(e);
  }
  return key;
}

// The edge's polygons on the triangulation of the face (two for a seam),
// or false when it has none there.
bool polygons_of(const TopoDS_Edge& edge, const occ::handle<Poly_Triangulation>& triangulation,
                 const TopLoc_Location& location, occ::handle<Poly_PolygonOnTriangulation>& first,
                 occ::handle<Poly_PolygonOnTriangulation>& second) {
  const auto* tedge = static_cast<const BRep_TEdge*>(edge.TShape().get());
  const TopLoc_Location relative = location.Predivided(edge.Location());
  for (const occ::handle<BRep_CurveRepresentation>& representation : tedge->Curves()) {
    if (representation->IsPolygonOnTriangulation(triangulation, relative)) {
      first = representation->PolygonOnTriangulation();
      if (representation->IsPolygonOnClosedTriangulation()) {
        second = representation->PolygonOnTriangulation2();
      }
      return !first.IsNull();
    }
  }
  return false;
}

// A copy of what a triangulation holds of the face (nodes, parameters,
// triangles, deviation), without the normals a presentation may be adding
// to the original meanwhile, its nodes moved by `move` (from the donor
// face's frame to the target's, mitcad#1243). Null for a move that mirrors
// or scales: the copy's triangles would turn the other way round, or its
// deviation would not be the donor's.
occ::handle<Poly_Triangulation> copy_triangulation(const occ::handle<Poly_Triangulation>& source,
                                                   const TopLoc_Location& move) {
  const bool moved = !move.IsIdentity();
  const gp_Trsf trsf = moved ? move.Transformation() : gp_Trsf();
  if (moved && (trsf.IsNegative() || std::abs(std::abs(trsf.ScaleFactor()) - 1.0) > 1e-12)) {
    return occ::handle<Poly_Triangulation>();
  }
  occ::handle<Poly_Triangulation> copy =
      new Poly_Triangulation(source->NbNodes(), source->NbTriangles(), source->HasUVNodes(), false);
  for (int i = 1; i <= source->NbNodes(); ++i) {
    copy->SetNode(i, moved ? source->Node(i).Transformed(trsf) : source->Node(i));
    if (source->HasUVNodes()) {
      copy->SetUVNode(i, source->UVNode(i));
    }
  }
  for (int i = 1; i <= source->NbTriangles(); ++i) {
    copy->SetTriangle(i, source->Triangle(i));
  }
  copy->Deflection(source->Deflection());
  copy->Parameters(source->Parameters());
  return copy;
}

// How many nodes of a triangulation triangulation_lies_on checks at most
// (spread over them, and the last).
constexpr int kCheckedNodes = 64;

// Whether a triangulation, given to the face, lies on it: each node checked
// (kCheckedNodes), placed by the face's location as the view places it, on
// the face's surface at the node's parameters, within twice the largest
// tolerance of the face, its edges and its vertices (an edge's nodes lie
// on its curve); a triangulation without parameters, its nodes inside the
// face's bounds so widened. One without nodes or triangles does not.
bool triangulation_lies_on(const TopoDS_Face& face, const Poly_Triangulation& triangulation) {
  const int nodes = triangulation.NbNodes();
  if (nodes <= 0 || triangulation.NbTriangles() <= 0) {
    return false;
  }
  double tolerance = std::max(BRep_Tool::Tolerance(face), Precision::Confusion());
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(it.Current())));
  }
  for (TopExp_Explorer it(face, TopAbs_VERTEX); it.More(); it.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Vertex(it.Current())));
  }
  tolerance *= 2.0;
  std::vector<int> checked;
  const int step = std::max(1, nodes / kCheckedNodes);
  for (int i = 1; i <= nodes; i += step) {
    checked.push_back(i);
  }
  if (checked.back() != nodes) {
    checked.push_back(nodes);
  }
  if (triangulation.HasUVNodes()) {
    TopLoc_Location placed;
    const occ::handle<Geom_Surface>& surface = BRep_Tool::Surface(face, placed);
    if (surface.IsNull()) {
      return false;
    }
    // The surface's points in the triangulation's frame, the face's.
    const gp_Trsf to_nodes = placed.Predivided(face.Location()).Transformation();
    for (const int i : checked) {
      const gp_Pnt2d uv = triangulation.UVNode(i);
      const gp_Pnt on = surface->Value(uv.X(), uv.Y()).Transformed(to_nodes);
      if (!(on.Distance(triangulation.Node(i)) <= tolerance)) {
        return false;
      }
    }
    return true;
  }
  Bnd_Box bounds;
  BRepBndLib::Add(face, bounds, false);
  if (bounds.IsVoid()) {
    return false;
  }
  bounds.Enlarge(tolerance);
  const gp_Trsf placement = face.Location().Transformation();
  for (const int i : checked) {
    if (bounds.IsOut(triangulation.Node(i).Transformed(placement))) {
      return false;
    }
  }
  return true;
}

// Gives `face` a copy of the donor face's triangulation and of its edges'
// polygons; false (nothing written) when an edge has no polygon there, or
// when the copy, in the target face's frame, does not lie on the target
// face (triangulation_lies_on): the meshing that follows meshes it then.
bool reuse_face(const FaceKey& target, const FaceKey& donor) {
  TopLoc_Location donor_location;
  const occ::handle<Poly_Triangulation>& source = BRep_Tool::Triangulation(donor.face, donor_location);
  if (source.IsNull()) {
    return false;
  }
  struct Polygons {
    occ::handle<Poly_PolygonOnTriangulation> first;
    occ::handle<Poly_PolygonOnTriangulation> second;
  };
  std::vector<Polygons> polygons(donor.edges.size());
  std::vector<TopoDS_Edge> donor_edges(donor.edges.size());
  std::vector<TopoDS_Edge> target_edges(target.edges.size());
  const auto edges_of = [](const FaceKey& key, std::vector<TopoDS_Edge>& edges) {
    for (TopExp_Explorer it(key.face, TopAbs_EDGE); it.More(); it.Next()) {
      for (std::size_t i = 0; i < key.edges.size(); ++i) {
        const EdgeKey& e = key.edges[i];
        const TopoDS_Shape& edge = it.Current();
        if (e.tshape == edge.TShape().get() && e.placement.IsEqual(edge.Location()) && edges[i].IsNull()) {
          edges[i] = TopoDS::Edge(it.Current());
        }
      }
    }
  };
  edges_of(donor, donor_edges);
  edges_of(target, target_edges);
  for (std::size_t i = 0; i < donor_edges.size(); ++i) {
    if (donor_edges[i].IsNull() || target_edges[i].IsNull() ||
        !polygons_of(donor_edges[i], source, donor_location, polygons[i].first, polygons[i].second)) {
      return false;
    }
  }
  // A triangulation's nodes are in its face's frame (the face's location
  // places them), and the same surface placed alike may be placed by the
  // face in the donor and by the surface in the target (mitcad#1243): a
  // prism's top is its base's face moved up by its location, and a
  // boolean that rebuilds it keeps the move in the surface's placement.
  // Copied as they were, the top's nodes stayed where the base is: the top
  // was drawn and picked there, over the bottom, and missing above. Moved
  // from the donor's frame into the target's, they lie on the face.
  const TopLoc_Location move = donor_location.Predivided(target.face.Location());
  const occ::handle<Poly_Triangulation> copy = copy_triangulation(source, move);
  if (copy.IsNull() || !triangulation_lies_on(target.face, *copy)) {
    return false;
  }
  BRep_Builder builder;
  builder.UpdateFace(target.face, copy);
  TopLoc_Location location;
  BRep_Tool::Triangulation(target.face, location);
  for (std::size_t i = 0; i < target_edges.size(); ++i) {
    if (polygons[i].second.IsNull()) {
      builder.UpdateEdge(target_edges[i], polygons[i].first->Copy(), copy, location);
    } else {
      builder.UpdateEdge(target_edges[i], polygons[i].first->Copy(), polygons[i].second->Copy(), copy, location);
    }
  }
  return true;
}

} // namespace

std::size_t reuse_triangulations(const TopoDS_Shape& shape, const std::vector<TopoDS_Shape>& donors) {
  if (shape.IsNull() || donors.empty()) {
    return 0;
  }
  std::vector<FaceKey> wanted;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (BRep_Tool::Triangulation(face, location).IsNull() && !BRep_Tool::Surface(face, location).IsNull()) {
      wanted.push_back(face_key(face));
    }
  }
  if (wanted.empty()) {
    return 0;
  }
  // The donors' meshed faces by their surface.
  std::unordered_map<const Geom_Surface*, std::vector<FaceKey>> offered;
  std::unordered_map<const Geom_Surface*, bool> needed;
  for (const FaceKey& key : wanted) {
    needed[key.surface] = true;
  }
  for (const TopoDS_Shape& donor : donors) {
    if (donor.IsNull()) {
      continue;
    }
    for (TopExp_Explorer it(donor, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      TopLoc_Location location;
      const Geom_Surface* surface = BRep_Tool::Surface(face, location).get();
      if (surface != nullptr && needed.count(surface) > 0 && !BRep_Tool::Triangulation(face, location).IsNull()) {
        offered[surface].push_back(face_key(face));
      }
    }
  }
  std::size_t reused = 0;
  std::vector<const TopoDS_TShape*> done;
  for (const FaceKey& key : wanted) {
    if (std::find(done.begin(), done.end(), key.face.TShape().get()) != done.end()) {
      continue; // the same face again in another placement
    }
    const auto found = offered.find(key.surface);
    if (found == offered.end()) {
      continue;
    }
    for (const FaceKey& candidate : found->second) {
      if (key.same(candidate) && reuse_face(key, candidate)) {
        done.push_back(key.face.TShape().get());
        ++reused;
        break;
      }
    }
  }
  return reused;
}

bool triangulation_on_face(const TopoDS_Face& face) {
  if (face.IsNull()) {
    return false;
  }
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation>& triangulation = BRep_Tool::Triangulation(face, location);
  return !triangulation.IsNull() && triangulation_lies_on(face, *triangulation);
}

void use_bounded_presentation_meshes(const MeshOptions& options) {
  BRepMesh_DiscretAlgoFactory::UnregisterFactory(BoundedFactory::kName);
  BRepMesh_DiscretAlgoFactory::RegisterFactory(new BoundedFactory(options), true);
  BRepMesh_DiscretFactory::Get().SetDefaultName(BoundedFactory::kName);
}

namespace {

bool has_surfaceless_face(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

} // namespace

bool is_mesh(const TopoDS_Shape& shape) {
  bool any = false;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (!BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return false;
    }
    any = true;
  }
  return any;
}

TopoDS_Shape triangulate(const TopoDS_Shape& shape, const MeshOptions& options) {
  if (shape.IsNull()) {
    throw Error("cannot triangulate a null shape");
  }
  IMeshTools_Parameters parameters = parameters_of(options, options.linear_deflection);
  if (has_surfaceless_face(shape)) {
    return shape;
  }
  parameters = parameters_of(options, shape_deviation(shape, options));
  // A topology copy without triangulations: the original keeps its own.
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(shape, false, false).Shape();
  BoundedMesh mesher(options);
  mesher.SetShape(copy);
  mesher.ChangeParameters() = parameters;
  mesher.Perform();
  if (!mesher.IsDone()) {
    throw Error("triangulation failed");
  }
  return copy;
}

bool mesh_in_place(const TopoDS_Shape& shape, const MeshOptions& options, const Message_ProgressRange& range,
                   MeshReport* report) {
  if (shape.IsNull()) {
    throw Error("cannot triangulate a null shape");
  }
  IMeshTools_Parameters parameters = parameters_of(options, options.linear_deflection);
  if (has_surfaceless_face(shape)) {
    return true;
  }
  parameters = parameters_of(options, shape_deviation(shape, options));
  BoundedMesh mesher(options, report);
  mesher.SetShape(shape);
  mesher.ChangeParameters() = parameters;
  mesher.Perform(range);
  return (mesher.GetStatusFlags() & IMeshData_UserBreak) == 0;
}

std::size_t mesh_all_in_place(const std::vector<TopoDS_Shape>& shapes, const MeshOptions& options,
                              const Message_ProgressRange& range, MeshReport* report) {
  parameters_of(options, options.linear_deflection); // throws for options that mesh nothing
  std::size_t meshed = 0;
  // The shapes by the deviation they mesh with (shape_deviation,
  // mitcad#1275), the finest first, each deviation's in one compound: it
  // holds their own sub-shapes (TShapes), so BRepMesh triangulates the
  // shapes' faces themselves, each face once (BRepMesh_ShapeExplorer skips
  // a face met again in another placement, and commits the edges' polygons
  // on one thread); a face shared with a shape of a coarser deviation keeps
  // the finer triangulation it has by then.
  std::vector<const TopoDS_Shape*> joined;
  std::map<double, std::vector<const TopoDS_Shape*>> groups;
  for (const TopoDS_Shape& shape : shapes) {
    if (shape.IsNull()) {
      continue;
    }
    if (has_surfaceless_face(shape)) {
      ++meshed; // left as it is, as mesh_in_place leaves it
      continue;
    }
    joined.push_back(&shape);
    groups[shape_deviation(shape, options)].push_back(&shape);
  }
  if (joined.empty()) {
    return meshed;
  }
  Message_ProgressScope scope(range, nullptr, static_cast<double>(groups.size() + 1));
  std::vector<const TopoDS_Shape*> left; // of the meshings that threw
  bool stopped = false;
  BRep_Builder builder;
  for (const auto& [deviation, members] : groups) {
    if (scope.UserBreak()) {
      stopped = true;
      break;
    }
    TopoDS_Compound all;
    builder.MakeCompound(all);
    for (const TopoDS_Shape* shape : members) {
      builder.Add(all, *shape);
    }
    try {
      BoundedMesh mesher(options, report);
      mesher.SetShape(all);
      mesher.ChangeParameters() = parameters_of(options, deviation);
      mesher.Perform(scope.Next());
      if ((mesher.GetStatusFlags() & IMeshData_UserBreak) != 0 || scope.UserBreak()) {
        stopped = true;
        break;
      }
    } catch (const Standard_Failure&) {
      left.insert(left.end(), members.begin(), members.end());
    } catch (const std::exception&) {
      left.insert(left.end(), members.begin(), members.end());
    }
  }
  if (stopped) {
    // The shapes whose faces were all meshed by then.
    for (const TopoDS_Shape* shape : joined) {
      meshed += has_triangulation(*shape) ? 1 : 0;
    }
    return meshed;
  }
  meshed += joined.size() - left.size();
  if (left.empty()) {
    return meshed;
  }
  // A joint meshing threw: the shapes it did not finish one by one, so that
  // one shape OCCT fails on leaves only itself without triangulations.
  Message_ProgressScope each(scope.Next(), nullptr, static_cast<double>(left.size()));
  for (const TopoDS_Shape* shape : left) {
    if (each.UserBreak()) {
      break;
    }
    const Message_ProgressRange step = each.Next();
    if (has_triangulation(*shape)) {
      ++meshed;
      continue;
    }
    try {
      if (mesh_in_place(*shape, options, step)) {
        ++meshed;
      }
    } catch (const Standard_Failure&) {
      // Left as it is.
    } catch (const std::exception&) {
    }
  }
  return meshed;
}

MeshStats mesh_stats(const TopoDS_Shape& shape) {
  MeshStats stats;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    ++stats.faces;
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh =
        BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location);
    if (!mesh.IsNull()) {
      stats.triangles += static_cast<std::size_t>(mesh->NbTriangles());
      stats.nodes += static_cast<std::size_t>(mesh->NbNodes());
    }
  }
  return stats;
}

TopoDS_Shape mesh_body(const occ::handle<Poly_Triangulation>& triangulation) {
  if (triangulation.IsNull() || triangulation->NbTriangles() == 0) {
    throw Error("empty triangulation");
  }
  TopoDS_Face face;
  BRep_Builder().MakeFace(face, triangulation);
  return face;
}

namespace {

// Vertices merged within a tolerance: each goes into a grid cell of the
// tolerance's size and is looked up in the cells around its own.
class VertexMerger {
public:
  VertexMerger(double tolerance, std::vector<std::array<double, 3>>& vertices)
      : m_tolerance(tolerance), m_vertices(vertices) {}

  std::uint32_t add(const gp_Pnt& p) {
    const Cell cell{key(p.X()), key(p.Y()), key(p.Z())};
    for (std::int64_t dx = -1; dx <= 1; ++dx) {
      for (std::int64_t dy = -1; dy <= 1; ++dy) {
        for (std::int64_t dz = -1; dz <= 1; ++dz) {
          const auto found = m_cells.find(Cell{cell.x + dx, cell.y + dy, cell.z + dz});
          if (found == m_cells.end()) {
            continue;
          }
          for (const std::uint32_t index : found->second) {
            const std::array<double, 3>& q = m_vertices[index];
            const double d2 = (p.X() - q[0]) * (p.X() - q[0]) + (p.Y() - q[1]) * (p.Y() - q[1]) +
                              (p.Z() - q[2]) * (p.Z() - q[2]);
            if (d2 <= m_tolerance * m_tolerance) {
              return index;
            }
          }
        }
      }
    }
    const auto index = static_cast<std::uint32_t>(m_vertices.size());
    m_vertices.push_back({p.X(), p.Y(), p.Z()});
    m_cells[cell].push_back(index);
    return index;
  }

private:
  struct Cell {
    std::int64_t x;
    std::int64_t y;
    std::int64_t z;
    bool operator==(const Cell& other) const { return x == other.x && y == other.y && z == other.z; }
  };
  struct CellHash {
    std::size_t operator()(const Cell& c) const {
      const auto mix = [](std::uint64_t h, std::int64_t v) {
        return (h ^ static_cast<std::uint64_t>(v)) * 0x100000001b3ull;
      };
      return static_cast<std::size_t>(mix(mix(mix(0xcbf29ce484222325ull, c.x), c.y), c.z));
    }
  };

  std::int64_t key(double v) const { return static_cast<std::int64_t>(std::floor(v / m_tolerance)); }

  double m_tolerance;
  std::vector<std::array<double, 3>>& m_vertices;
  std::unordered_map<Cell, std::vector<std::uint32_t>, CellHash> m_cells;
};

} // namespace

FaceError::FaceError(int index, std::string surface)
    : Error(face_error_message("face " + std::to_string(index), surface)), m_index(index),
      m_surface(std::move(surface)) {}

std::string face_error_message(const std::string& face, const std::string& surface) {
  return "a face could not be triangulated: " + face + " (" + surface + ")";
}

FaceError untriangulated_face(const TopoDS_Shape& shape, const TopoDS_Face& face) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  std::string surface = "unknown";
  try {
    surface = surface_name(BRepAdaptor_Surface(face, false).GetType());
  } catch (const Standard_Failure&) {
    // Its kind cannot be told.
  }
  return FaceError(faces.FindIndex(face) - 1, surface);
}

IndexedMesh indexed_mesh(const TopoDS_Shape& shape, const MeshOptions& options) {
  const TopoDS_Shape meshed = triangulate(shape, options);
  IndexedMesh mesh;
  VertexMerger merger(1e-6, mesh.vertices);
  std::vector<std::uint32_t> nodes;
  for (TopExp_Explorer it(meshed, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      throw untriangulated_face(meshed, face);
    }
    const gp_Trsf placement = location.Transformation();
    nodes.resize(static_cast<std::size_t>(triangulation->NbNodes()));
    for (int i = 1; i <= triangulation->NbNodes(); ++i) {
      gp_Pnt p = triangulation->Node(i);
      if (!location.IsIdentity()) {
        p.Transform(placement);
      }
      nodes[static_cast<std::size_t>(i - 1)] = merger.add(p);
    }
    // A face's triangles follow its surface's normal: turned round where
    // the face is reversed, or mirrored by its placement.
    const bool turned = (face.Orientation() == TopAbs_REVERSED) != placement.IsNegative();
    for (int t = 1; t <= triangulation->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      triangulation->Triangle(t).Get(a, b, c);
      if (turned) {
        std::swap(b, c);
      }
      const std::array<std::uint32_t, 3> triangle{nodes[static_cast<std::size_t>(a - 1)],
                                                  nodes[static_cast<std::size_t>(b - 1)],
                                                  nodes[static_cast<std::size_t>(c - 1)]};
      if (triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0]) {
        continue; // collapsed by the merge
      }
      mesh.triangles.push_back(triangle);
    }
  }
  return mesh;
}

} // namespace mitcad::io
