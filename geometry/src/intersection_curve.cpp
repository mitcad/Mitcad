// SPDX-License-Identifier: MIT
#include "mitcad/geometry/intersection_curve.hpp"

#include <stdexcept>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBndLib.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "spine.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

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

} // namespace mitcad::geometry
