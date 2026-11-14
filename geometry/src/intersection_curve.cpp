// SPDX-License-Identifier: MIT
#include "mitcad/geometry/intersection_curve.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Surface.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "mitcad/geometry/user_error.hpp"
#include "spine.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

// The chain's wire swept along `direction` from `reach` before it to
// `reach` beyond it: a shell whose faces meet along the lines the chain's
// joints sweep.
TopoDS_Shape swept(const TopoDS_Wire& wire, const gp_Dir& direction, double reach) {
  gp_Trsf back;
  back.SetTranslation(gp_Vec(direction) * -reach);
  const TopoDS_Shape start = wire.Moved(TopLoc_Location(back));
  BRepPrimAPI_MakePrism prism(start, gp_Vec(direction) * (2.0 * reach), true);
  if (!prism.IsDone()) {
    throw std::runtime_error("a chain of curves could not be swept along its sketch's normal");
  }
  return prism.Shape();
}

} // namespace

std::vector<ModelCurve> intersection_curves(const Path& first, const gp_Dir& first_direction, const Path& second,
                                            const gp_Dir& second_direction) {
  return detail::run("intersection curve", [&] {
    if (first_direction.IsParallel(second_direction, 1e-9)) {
      throw std::invalid_argument("the sketches' normals are parallel: the swept curves do not cross in a curve");
    }
    const TopoDS_Wire a = detail::path_wire(first);
    const TopoDS_Wire b = detail::path_wire(second);
    // Far enough that each surface passes through the other chain: beyond
    // the box of both.
    Bnd_Box box;
    BRepBndLib::Add(a, box);
    BRepBndLib::Add(b, box);
    if (box.IsVoid()) {
      throw std::invalid_argument("an intersection curve needs curves of both sketches");
    }
    const double reach = box.CornerMin().Distance(box.CornerMax()) + 1.0;
    BRepAlgoAPI_Section section(swept(a, first_direction, reach), swept(b, second_direction, reach), false);
    section.Approximation(true);
    section.ComputePCurveOn1(false);
    section.ComputePCurveOn2(false);
    section.Build();
    if (!section.IsDone() || section.HasErrors()) {
      throw std::runtime_error("the swept curves' surfaces could not be intersected");
    }
    std::vector<ModelCurve> out;
    for (TopExp_Explorer it(section.Shape(), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      BRepAdaptor_Curve curve(edge);
      if (GCPnts_AbscissaPoint::Length(curve) <= 1e-7) {
        continue;
      }
      out.push_back(edge_curve(edge));
    }
    return out;
  });
}

std::vector<ModelCurve> helix_curve(const HelixCurve& helix) {
  detail::require_positive("helix radius", helix.radius);
  detail::require_positive("helix pitch", helix.pitch);
  detail::require_positive("helix revolutions", helix.revolutions);
  detail::require_finite("helix growth", helix.growth);
  if (helix.growth < 0.0 && helix.radius + helix.growth * helix.revolutions <= 1e-9) {
    throw std::invalid_argument("the helix narrows to its axis before its last turn");
  }
  return detail::run("helix curve", [&] {
    // The helix is a line on the surface its turns lie on: on a cylinder
    // a turn rises a pitch; on the cone whose half angle has the growth
    // per pitch as its tangent a turn runs pitch / cos(angle) along it.
    gp_Ax3 axes(helix.origin, helix.direction, helix.x_axis);
    if (helix.left_handed) {
      axes.YReverse();
    }
    occ::handle<Geom_Surface> surface;
    double along = helix.pitch;
    if (helix.growth == 0.0) {
      surface = new Geom_CylindricalSurface(axes, helix.radius);
    } else {
      const double angle = std::atan2(helix.growth, helix.pitch);
      surface = new Geom_ConicalSurface(axes, angle, helix.radius);
      along = helix.pitch / std::cos(angle);
    }
    const gp_Vec2d step(kTwoPi, along);
    const occ::handle<Geom2d_Line> line = new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(step));
    BRepBuilderAPI_MakeEdge make(line, surface, 0.0, helix.revolutions * step.Magnitude());
    if (!make.IsDone()) {
      throw std::runtime_error("the helix could not be built");
    }
    TopoDS_Edge edge = make.Edge();
    const int samples = std::max(32, static_cast<int>(std::ceil(helix.revolutions * 32.0)));
    BRepLib::BuildCurves3d(edge, 1.0e-7, GeomAbs_C1, 14, samples);
    return std::vector<ModelCurve>{edge_curve(edge)};
  });
}

namespace {

// The shapes of one side of a section as one compound: whole shapes, or
// the faces their names find.
TopoDS_Compound section_side(const std::vector<SectionPart>& side, const char* which) {
  if (side.empty()) {
    throw std::invalid_argument(std::string("the ") + which + " side of the section has no faces or bodies");
  }
  BRep_Builder builder;
  TopoDS_Compound all;
  builder.MakeCompound(all);
  for (const SectionPart& part : side) {
    if (!part.shape) {
      throw std::invalid_argument(std::string("a body of the ") + which + " side has no shape");
    }
    if (part.faces.empty()) {
      builder.Add(all, part.shape->occt());
      continue;
    }
    for (const std::string& name : part.faces) {
      const std::vector<int> found = part.shape->find_faces(name);
      if (found.empty()) {
        throw std::invalid_argument(placed_error(std::string("a face of the ") + which + " side is not on its body",
                                                 std::string("face ") + name + " of the " + which +
                                                     " side is not on its body"));
      }
      for (const int f : found) {
        builder.Add(all, part.shape->face(f));
      }
    }
  }
  return all;
}

} // namespace

std::vector<ModelCurve> shape_section(const std::vector<SectionPart>& first, const std::vector<SectionPart>& second) {
  return detail::run("intersection curve", [&] {
    const TopoDS_Compound a = section_side(first, "first");
    const TopoDS_Compound b = section_side(second, "second");
    BRepAlgoAPI_Section section(a, b, false);
    section.Approximation(true);
    section.ComputePCurveOn1(false);
    section.ComputePCurveOn2(false);
    section.Build();
    if (!section.IsDone() || section.HasErrors()) {
      throw std::runtime_error("the faces or bodies could not be intersected");
    }
    std::vector<ModelCurve> out;
    for (TopExp_Explorer it(section.Shape(), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      BRepAdaptor_Curve curve(edge);
      if (GCPnts_AbscissaPoint::Length(curve) <= 1e-7) {
        continue;
      }
      out.push_back(edge_curve(edge));
    }
    return out;
  });
}

} // namespace mitcad::geometry
