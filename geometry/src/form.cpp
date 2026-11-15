// SPDX-License-Identifier: MIT
#include "mitcad/geometry/form.hpp"

#include <cmath>
#include <cstddef>
#include <map>
#include <stdexcept>
#include <string>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRep_Tool.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Surface.hxx>
#include <Message_ProgressRange.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Precision.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>

#include "history.hpp"
#include "spine.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// The most poles along one side of a patch.
constexpr int kMostPoles = 1000;

// Throws unless one direction's degree, pole count, knots and
// multiplicities make a clamped or unclamped B-spline.
void check_direction(int degree, int poles, const std::vector<double>& knots, const std::vector<int>& mults,
                     const char* which) {
  const std::string along = std::string(" along ") + which;
  if (degree < 1 || degree > 9 || poles <= degree || poles > kMostPoles) {
    throw std::invalid_argument("form: a patch has a degree or a number of poles it cannot have" + along);
  }
  if (knots.size() != mults.size() || knots.size() < 2) {
    throw std::invalid_argument("form: a patch's knots do not match their multiplicities" + along);
  }
  int sum = 0;
  for (std::size_t k = 0; k < knots.size(); ++k) {
    if (!std::isfinite(knots[k]) || (k > 0 && !(knots[k] > knots[k - 1]))) {
      throw std::invalid_argument("form: a patch's knots must increase" + along);
    }
    if (mults[k] < 1 || mults[k] > degree + 1) {
      throw std::invalid_argument("form: a patch's knot has a multiplicity it cannot have" + along);
    }
    sum += mults[k];
  }
  if (sum != poles + degree + 1) {
    throw std::invalid_argument("form: a patch's knots do not fit its poles" + along);
  }
}

void check_patch(const SurfacePatch& patch) {
  check_direction(patch.u_degree, patch.nu, patch.u_knots, patch.u_mults, "u");
  check_direction(patch.v_degree, patch.nv, patch.v_knots, patch.v_mults, "v");
  if (patch.poles.size() != static_cast<std::size_t>(patch.nu) * static_cast<std::size_t>(patch.nv)) {
    throw std::invalid_argument("form: a patch's poles do not match its counts");
  }
  for (const gp_Pnt& pole : patch.poles) {
    if (!std::isfinite(pole.X()) || !std::isfinite(pole.Y()) || !std::isfinite(pole.Z())) {
      throw std::invalid_argument("form: a patch's pole is not finite");
    }
  }
}

NCollection_Array1<double> reals(const std::vector<double>& values) {
  NCollection_Array1<double> array(1, static_cast<int>(values.size()));
  for (std::size_t i = 0; i < values.size(); ++i) {
    array.SetValue(static_cast<int>(i) + 1, values[i]);
  }
  return array;
}

NCollection_Array1<int> integers(const std::vector<int>& values) {
  NCollection_Array1<int> array(1, static_cast<int>(values.size()));
  for (std::size_t i = 0; i < values.size(); ++i) {
    array.SetValue(static_cast<int>(i) + 1, values[i]);
  }
  return array;
}

occ::handle<Geom_BSplineSurface> surface_of(const SurfacePatch& patch) {
  NCollection_Array2<gp_Pnt> poles(1, patch.nu, 1, patch.nv);
  for (int i = 0; i < patch.nu; ++i) {
    for (int j = 0; j < patch.nv; ++j) {
      const std::size_t at = static_cast<std::size_t>(i) * static_cast<std::size_t>(patch.nv) +
                             static_cast<std::size_t>(j);
      poles.SetValue(i + 1, j + 1, patch.poles[at]);
    }
  }
  return new Geom_BSplineSurface(poles, reals(patch.u_knots), reals(patch.v_knots), integers(patch.u_mults),
                                 integers(patch.v_mults), patch.u_degree, patch.v_degree);
}

} // namespace

ShapePtr patch_body(const std::vector<SurfacePatch>& patches, bool closed, double tolerance) {
  if (patches.empty()) {
    throw std::invalid_argument("form: there are no patches");
  }
  if (!(tolerance > 0.0) || !std::isfinite(tolerance)) {
    throw std::invalid_argument("form: the sewing tolerance must be greater than zero");
  }
  for (const SurfacePatch& patch : patches) {
    check_patch(patch);
  }
  return detail::run("form", [&] {
    BRepBuilderAPI_Sewing sewing(tolerance);
    // Each face is found again in the sewn result by its surface, which
    // the sewing keeps.
    std::map<const Geom_Surface*, std::string> names;
    for (const SurfacePatch& patch : patches) {
      const occ::handle<Geom_BSplineSurface> surface = surface_of(patch);
      BRepBuilderAPI_MakeFace make(surface, Precision::Confusion());
      if (!make.IsDone()) {
        throw std::runtime_error("form: a patch could not be made a face");
      }
      names[surface.get()] = patch.name;
      sewing.Add(make.Face());
    }
    detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
    const TopoDS_Shape sewn = sewing.SewedShape();
    if (sewn.IsNull()) {
      throw std::runtime_error("form: the patches could not be sewn");
    }
    TopoDS_Shape result = sewn;
    if (closed) {
      const int open = sewing.NbFreeEdges();
      if (open > 0) {
        throw std::runtime_error("form: the patches do not close into a solid: " + std::to_string(open) +
                                 " of their edges are open");
      }
      result = detail::oriented_solid(sewn, "form");
    }
    detail::FaceNamer namer(result);
    for (TopExp_Explorer it(result, TopAbs_FACE); it.More(); it.Next()) {
      TopLoc_Location location;
      const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(TopoDS::Face(it.Current()), location);
      const auto found = names.find(surface.get());
      if (found != names.end()) {
        namer.add(it.Current(), found->second);
      }
    }
    namer.finish();
    return namer.shape();
  });
}

} // namespace mitcad::geometry
