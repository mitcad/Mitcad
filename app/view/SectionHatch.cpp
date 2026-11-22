// SPDX-License-Identifier: MIT
#include "view/SectionHatch.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include <map>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

namespace mitcad {

TopoDS_Shape sectionHatch(const TopoDS_Shape& caps, const gp_Ax3& plane, bool mirrored) {
  if (caps.IsNull()) {
    return TopoDS_Shape();
  }
  Bnd_Box box;
  BRepBndLib::Add(caps, box, false);
  if (box.IsVoid()) {
    return TopoDS_Shape();
  }
  const double size = std::sqrt(box.SquareExtent());
  const gp_Vec x(plane.XDirection());
  const gp_Vec y(plane.YDirection());
  const gp_Vec along = mirrored ? (x - y) / std::sqrt(2.0) : (x + y) / std::sqrt(2.0);
  const gp_Vec across = mirrored ? (x + y) / std::sqrt(2.0) : (x - y) / std::sqrt(2.0);
  const gp_Pnt origin = plane.Location();
  TopoDS_Compound lines;
  BRep_Builder builder;
  builder.MakeCompound(lines);
  bool any = false;
  for (TopExp_Explorer faces(caps, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face& face = TopoDS::Face(faces.Current());
    Bnd_Box faceBox;
    BRepBndLib::Add(face, faceBox, false);
    if (faceBox.IsVoid()) {
      continue;
    }
    const double spacing =
        std::max(size / 400.0, std::min(std::sqrt(faceBox.SquareExtent()) / 8.0, size / 40.0));
    // Per line k at (k + 0.5) * spacing across: its pieces along, from every
    // triangle of the face it crosses. The triangles' nodes are read in the
    // plane's own coordinates (across, along); their distance from the
    // plane is left out, so that the lines lie in the plane whatever the
    // mesh's rounding.
    std::map<long, std::vector<std::pair<double, double>>> pieces;
    TopLoc_Location location;
    occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull()) {
      BRepMesh_IncrementalMesh(face, size * 1e-3, false, 0.5);
      mesh = BRep_Tool::Triangulation(face, location);
      if (mesh.IsNull()) {
        continue;
      }
    }
    const gp_Trsf toModel = location.Transformation();
    for (int t = 1; t <= mesh->NbTriangles(); ++t) {
      int nodes[3];
      mesh->Triangle(t).Get(nodes[0], nodes[1], nodes[2]);
      double s[3];
      double u[3];
      for (int i = 0; i < 3; ++i) {
        const gp_Vec p(origin, mesh->Node(nodes[i]).Transformed(toModel));
        s[i] = p.Dot(across) / spacing - 0.5;
        u[i] = p.Dot(along);
      }
      const auto first = static_cast<long>(std::ceil(std::min({s[0], s[1], s[2]})));
      const auto last = static_cast<long>(std::floor(std::max({s[0], s[1], s[2]})));
      for (long k = first; k <= last; ++k) {
        double low = std::numeric_limits<double>::max();
        double high = -low;
        for (int i = 0; i < 3; ++i) {
          const int j = (i + 1) % 3;
          const double a = s[i] - static_cast<double>(k);
          const double b = s[j] - static_cast<double>(k);
          if ((a < 0.0 && b < 0.0) || (a > 0.0 && b > 0.0) || a == b) {
            continue;
          }
          const double at = u[i] + (u[j] - u[i]) * a / (a - b);
          low = std::min(low, at);
          high = std::max(high, at);
        }
        if (high > low) {
          pieces[k].emplace_back(low, high);
        }
      }
    }
    // Each line's pieces joined where they meet (neighbouring triangles).
    for (auto& [k, spans] : pieces) {
      std::sort(spans.begin(), spans.end());
      const gp_Pnt base = origin.Translated(across * ((static_cast<double>(k) + 0.5) * spacing));
      const double gap = size * 1e-9;
      for (std::size_t i = 0; i < spans.size();) {
        double end = spans[i].second;
        std::size_t next = i + 1;
        while (next < spans.size() && spans[next].first <= end + gap) {
          end = std::max(end, spans[next].second);
          ++next;
        }
        if (end - spans[i].first > gap) {
          builder.Add(lines, BRepBuilderAPI_MakeEdge(base.Translated(along * spans[i].first),
                                                     base.Translated(along * end))
                                 .Edge());
          any = true;
        }
        i = next;
      }
    }
  }
  return any ? TopoDS_Shape(lines) : TopoDS_Shape();
}

double sectionHatchPlaneDistance(const TopoDS_Shape& hatch, const gp_Ax3& plane) {
  if (hatch.IsNull()) {
    return 0.0;
  }
  const gp_Pln pln(plane);
  double largest = 0.0;
  for (TopExp_Explorer vertices(hatch, TopAbs_VERTEX); vertices.More(); vertices.Next()) {
    largest = std::max(largest, pln.Distance(BRep_Tool::Pnt(TopoDS::Vertex(vertices.Current()))));
  }
  return largest;
}

} // namespace mitcad
