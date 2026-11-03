// SPDX-License-Identifier: MIT
#include "mitcad/io/mesh_compare.hpp"

#include <algorithm>
#include <cmath>
#include <unordered_map>
#include <utility>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRep_Tool.hxx>
#include <Geom_Surface.hxx>
#include <OSD_Parallel.hxx>
#include <Poly_Triangulation.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/analysis/common.hpp"

namespace mitcad::io {

FaceMeshQuality face_mesh_quality(const TopoDS_Face& face) {
  FaceMeshQuality quality;
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
  if (mesh.IsNull()) {
    return quality;
  }
  quality.triangulated = true;
  quality.triangles = static_cast<std::size_t>(mesh->NbTriangles());
  quality.nodes = static_cast<std::size_t>(mesh->NbNodes());
  quality.recorded = mesh->Deflection();
  // Everything in the face's placement: the nodes by the triangulation's
  // location, the surface as BRep_Tool gives it located.
  const gp_Trsf placement = location.Transformation();
  std::vector<gp_Pnt> nodes(quality.nodes);
  for (int i = 1; i <= mesh->NbNodes(); ++i) {
    gp_Pnt p = mesh->Node(i);
    if (!location.IsIdentity()) {
      p.Transform(placement);
    }
    nodes[static_cast<std::size_t>(i - 1)] = p;
  }
  occ::handle<Geom_Surface> surface;
  try {
    surface = BRep_Tool::Surface(face);
  } catch (const Standard_Failure&) {
  }
  const bool parameters = mesh->HasUVNodes() && !surface.IsNull();
  // The surface's distance from a point of a triangle at its parameters.
  const auto off = [&surface](const gp_Pnt2d& uv, const gp_Pnt& point) {
    try {
      return surface->Value(uv.X(), uv.Y()).Distance(point);
    } catch (const Standard_Failure&) {
      return 0.0;
    }
  };
  std::size_t positive = 0;
  std::size_t negative = 0;
  for (int t = 1; t <= mesh->NbTriangles(); ++t) {
    int n[3] = {0, 0, 0};
    mesh->Triangle(t).Get(n[0], n[1], n[2]);
    const gp_Pnt& p0 = nodes[static_cast<std::size_t>(n[0] - 1)];
    const gp_Pnt& p1 = nodes[static_cast<std::size_t>(n[1] - 1)];
    const gp_Pnt& p2 = nodes[static_cast<std::size_t>(n[2] - 1)];
    quality.area += 0.5 * gp_Vec(p0, p1).Crossed(gp_Vec(p0, p2)).Magnitude();
    if (!parameters) {
      continue;
    }
    const gp_Pnt2d q0 = mesh->UVNode(n[0]);
    const gp_Pnt2d q1 = mesh->UVNode(n[1]);
    const gp_Pnt2d q2 = mesh->UVNode(n[2]);
    // Its turn in the parameter plane, unless it is a sliver whose sign
    // rounding decides.
    const double ux = q1.X() - q0.X();
    const double uy = q1.Y() - q0.Y();
    const double vx = q2.X() - q0.X();
    const double vy = q2.Y() - q0.Y();
    const double turn = ux * vy - uy * vx;
    const double scale =
        std::max({ux * ux + uy * uy, vx * vx + vy * vy, (vx - ux) * (vx - ux) + (vy - uy) * (vy - uy)});
    if (turn > 1e-9 * scale) {
      ++positive;
    } else if (turn < -1e-9 * scale) {
      ++negative;
    }
    const auto mid2 = [](const gp_Pnt2d& a, const gp_Pnt2d& b) { return gp_Pnt2d((a.XY() + b.XY()) * 0.5); };
    const auto mid3 = [](const gp_Pnt& a, const gp_Pnt& b) { return gp_Pnt((a.XYZ() + b.XYZ()) * 0.5); };
    const gp_Pnt2d centre2((q0.XY() + q1.XY() + q2.XY()) / 3.0);
    const gp_Pnt centre3((p0.XYZ() + p1.XYZ() + p2.XYZ()) / 3.0);
    quality.deviation = std::max({quality.deviation, off(centre2, centre3), off(mid2(q0, q1), mid3(p0, p1)),
                                  off(mid2(q1, q2), mid3(p1, p2)), off(mid2(q2, q0), mid3(p2, p0))});
  }
  quality.folded = std::min(positive, negative);
  return quality;
}

namespace {

// A face to compare: where it is, and the two copies' faces.
struct Entry {
  std::size_t body = 0;
  std::size_t index = 0;
  TopoDS_Face a;
  TopoDS_Face b;
};

bool has_surfaceless_face(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

// Each meshed face's time by its face (its TShape: a face met again in
// another placement is meshed once and takes the same time).
std::unordered_map<const TopoDS_TShape*, double> times_of(const MeshReport& report) {
  std::unordered_map<const TopoDS_TShape*, double> times;
  for (const MeshReport::FaceTime& time : report.every_face_time) {
    times[time.face.TShape().get()] += time.milliseconds;
  }
  return times;
}

} // namespace

MeshComparison compare_meshing(const std::vector<TopoDS_Shape>& shapes, const MeshOptions& a, const MeshOptions& b,
                               bool b_first) {
  MeshComparison comparison;
  std::vector<TopoDS_Shape> copies_a;
  std::vector<TopoDS_Shape> copies_b;
  std::vector<Entry> entries;
  for (std::size_t body = 0; body < shapes.size(); ++body) {
    const TopoDS_Shape& shape = shapes[body];
    if (shape.IsNull()) {
      continue;
    }
    if (has_surfaceless_face(shape)) {
      NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
      TopExp::MapShapes(shape, TopAbs_FACE, faces);
      comparison.surfaceless += static_cast<std::size_t>(faces.Extent());
      continue;
    }
    // Copies of the topology without triangulations, sharing the geometry:
    // the same faces in the same order.
    const TopoDS_Shape copy_a = BRepBuilderAPI_Copy(shape, false, false).Shape();
    const TopoDS_Shape copy_b = BRepBuilderAPI_Copy(shape, false, false).Shape();
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces_a;
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces_b;
    TopExp::MapShapes(copy_a, TopAbs_FACE, faces_a);
    TopExp::MapShapes(copy_b, TopAbs_FACE, faces_b);
    if (faces_a.Extent() != faces_b.Extent()) {
      continue; // cannot happen: the same copy twice
    }
    for (int i = 1; i <= faces_a.Extent(); ++i) {
      entries.push_back(
          Entry{body, static_cast<std::size_t>(i - 1), TopoDS::Face(faces_a(i)), TopoDS::Face(faces_b(i))});
    }
    copies_a.push_back(copy_a);
    copies_b.push_back(copy_b);
  }
  comparison.faces.resize(entries.size());
  // What does not depend on the meshing, before it (the floor's box would
  // otherwise come from the triangulation).
  OSD_Parallel::For(
      0, static_cast<int>(entries.size()),
      [&entries, &comparison, &a](int i) {
        const Entry& entry = entries[static_cast<std::size_t>(i)];
        FaceMeshComparison& face = comparison.faces[static_cast<std::size_t>(i)];
        face.body = entry.body;
        face.face = entry.index;
        try {
          face.surface = surface_name(BRepAdaptor_Surface(entry.a, false).GetType());
          face.tolerance = std::max(a.linear_deflection, face_deviation_floor(entry.a, a.linear_deflection,
                                                                              a.face_triangles));
          face.area = analysis::surface_properties(entry.a).Mass();
        } catch (const Standard_Failure&) {
          face.area = -1.0;
        }
      },
      entries.size() < 8);
  // One meshing, measured, and its copies let go before the other: a
  // large design's two sets of triangulations at once may not fit.
  const auto run = [&entries, &comparison](std::vector<TopoDS_Shape>& copies, const MeshOptions& options,
                                           bool second) {
    MeshReport& report = second ? comparison.b : comparison.a;
    report.every_face = true;
    try {
      mesh_all_in_place(copies, options, Message_ProgressRange(), &report);
    } catch (const Standard_Failure&) {
      // The faces left without a triangulation say so.
    } catch (const std::exception&) {
    }
    const std::unordered_map<const TopoDS_TShape*, double> times = times_of(report);
    OSD_Parallel::For(
        0, static_cast<int>(entries.size()),
        [&entries, &comparison, &times, second](int i) {
          Entry& entry = entries[static_cast<std::size_t>(i)];
          FaceMeshComparison& face = comparison.faces[static_cast<std::size_t>(i)];
          TopoDS_Face& meshed = second ? entry.b : entry.a;
          (second ? face.b : face.a) = face_mesh_quality(meshed);
          if (const auto found = times.find(meshed.TShape().get()); found != times.end()) {
            (second ? face.ms_b : face.ms_a) = found->second;
          }
          meshed.Nullify();
        },
        entries.size() < 8);
    copies.clear();
    report.every_face_time.clear(); // their faces hold the triangulations
    for (MeshReport::FaceTime& slow : report.slowest) {
      slow.face.Nullify();
    }
  };
  if (b_first) {
    run(copies_b, b, true);
    run(copies_a, a, false);
  } else {
    run(copies_a, a, false);
    run(copies_b, b, true);
  }
  return comparison;
}

} // namespace mitcad::io
