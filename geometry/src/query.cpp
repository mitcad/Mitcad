// SPDX-License-Identifier: MIT
#include "mitcad/geometry/query.hpp"

#include <algorithm>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopoDS_Edge.hxx>
#include <gp.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/analysis/common.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

const char* surface_type(const TopoDS_Face& face) {
  switch (BRepAdaptor_Surface(face).GetType()) {
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

const char* curve_type(const TopoDS_Edge& edge) {
  if (BRep_Tool::Degenerated(edge)) {
    return "degenerate";
  }
  switch (BRepAdaptor_Curve(edge).GetType()) {
  case GeomAbs_Line:
    return "line";
  case GeomAbs_Circle:
    return "circle";
  case GeomAbs_Ellipse:
    return "ellipse";
  case GeomAbs_Hyperbola:
    return "hyperbola";
  case GeomAbs_Parabola:
    return "parabola";
  case GeomAbs_BezierCurve:
    return "bezier";
  case GeomAbs_BSplineCurve:
    return "bspline";
  case GeomAbs_OffsetCurve:
    return "offset";
  default:
    return "other";
  }
}

} // namespace

MassProperties mass_properties(const Shape& shape) {
  return detail::run("mass properties", [&] {
    MassProperties result;
    if (shape.occt().IsNull()) {
      return result;
    }
    if (const auto measured = shape.measured()) {
      const auto& m = *measured;
      result.volume = m[0];
      result.area = m[1];
      result.center = gp_Pnt(m[2], m[3], m[4]);
      return result;
    }
    // The analysis library's integrals, as every reported volume.
    const GProp_GProps volume = analysis::volume_properties(shape.occt());
    const GProp_GProps surface = analysis::surface_properties(shape.occt());
    result.volume = volume.Mass();
    result.area = surface.Mass();
    result.center = result.volume > 0.0 ? volume.CentreOfMass() : surface.CentreOfMass();
    shape.set_measured({result.volume, result.area, result.center.X(), result.center.Y(),
                        result.center.Z()});
    return result;
  });
}

double volume(const Shape& shape) { return mass_properties(shape).volume; }

BoundingBox bounding_box(const Shape& shape) {
  return detail::run("bounding box", [&] {
    BoundingBox result;
    if (shape.occt().IsNull()) {
      return result;
    }
    if (const auto bounds = shape.bounds()) {
      const auto& b = *bounds;
      result.empty = false;
      result.min = gp_Pnt(b[0], b[1], b[2]);
      result.max = gp_Pnt(b[3], b[4], b[5]);
      return result;
    }
    Bnd_Box box;
    BRepBndLib::AddOptimal(shape.occt(), box, false, false);
    if (box.IsVoid()) {
      // A mesh body has only triangulations.
      BRepBndLib::Add(shape.occt(), box, true);
    }
    if (box.IsVoid()) {
      return result;
    }
    double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
    box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    result.empty = false;
    result.min = gp_Pnt(xmin, ymin, zmin);
    result.max = gp_Pnt(xmax, ymax, zmax);
    shape.set_bounds({xmin, ymin, zmin, xmax, ymax, zmax});
    return result;
  });
}

std::vector<FaceInfo> face_infos(const Shape& shape) {
  return detail::run("faces", [&] {
    std::vector<FaceInfo> faces;
    for (int i = 0; i < shape.face_count(); ++i) {
      faces.push_back({shape.face_names(i), surface_type(shape.face(i)),
                       analysis::surface_properties(shape.face(i)).Mass()});
    }
    return faces;
  });
}

std::vector<EdgeInfo> edge_infos(const Shape& shape) {
  return detail::run("edges", [&] {
    std::vector<EdgeInfo> edges;
    for (int i = 0; i < shape.edge_count(); ++i) {
      GProp_GProps props;
      BRepGProp::LinearProperties(shape.edge(i), props);
      edges.push_back({shape.edge_name(i), curve_type(shape.edge(i)), props.Mass()});
    }
    return edges;
  });
}

std::vector<EdgeMiddle> edge_middles(const Shape& shape) {
  return detail::run("edges", [&] {
    std::vector<EdgeMiddle> middles;
    for (int i = 0; i < shape.edge_count(); ++i) {
      const std::string& name = shape.edge_name(i);
      if (name.empty()) {
        continue;
      }
      TopoDS_Edge edge = shape.edge(i);
      edge.Orientation(TopAbs_FORWARD);
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      const double length = GCPnts_AbscissaPoint::Length(curve);
      const GCPnts_AbscissaPoint at(curve, length / 2.0, curve.FirstParameter());
      if (!at.IsDone()) {
        continue;
      }
      gp_Pnt point;
      gp_Vec tangent;
      curve.D1(at.Parameter(), point, tangent);
      if (tangent.Magnitude() <= gp::Resolution()) {
        continue;
      }
      middles.push_back({name, point, tangent.Normalized(), length});
    }
    return middles;
  });
}

std::vector<FacePoints> face_points(const Shape& shape, int count) {
  return detail::run("faces", [&] {
    std::vector<FacePoints> result;
    for (int i = 0; i < shape.face_count(); ++i) {
      if (shape.face_names(i).empty()) {
        continue;
      }
      const TopoDS_Face& face = shape.face(i);
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
      std::vector<gp_Pnt2d> inside;
      // A coarse grid, a fine one for narrow faces.
      for (const int cells : {6, 24}) {
        for (int a = 0; a < cells; ++a) {
          for (int b = 0; b < cells; ++b) {
            const gp_Pnt2d uv(u0 + (u1 - u0) * (a + 0.5) / cells,
                              v0 + (v1 - v0) * (b + 0.5) / cells);
            if (classifier.Perform(uv) == TopAbs_IN) {
              inside.push_back(uv);
            }
          }
        }
        if (!inside.empty()) {
          break;
        }
      }
      FacePoints points{shape.face_names(i).front(), {}};
      const BRepAdaptor_Surface surface(face);
      const std::size_t wanted = std::min(inside.size(), static_cast<std::size_t>(count));
      for (std::size_t k = 0; k < wanted; ++k) {
        const gp_Pnt2d& uv = inside[k * inside.size() / wanted];
        points.points.push_back(surface.Value(uv.X(), uv.Y()));
      }
      result.push_back(std::move(points));
    }
    return result;
  });
}

} // namespace mitcad::geometry
