// SPDX-License-Identifier: MIT
#include "mitcad/geometry/thread.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <BRep_Builder.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt2d.hxx>

#include "history.hpp"
#include "mitcad/geometry/reference.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
constexpr double kTan30 = 0.57735026918962576451;

// The groove's section in the axial plane: (along the axis from the helix's
// start, radius) corners. It reaches a little past the cylinder, so that
// the cut leaves no sliver.
std::vector<gp_Pnt2d> groove_section(double pitch, double depth, double radius, bool internal) {
  const double past = 0.1 * pitch;
  if (!internal) {
    // Between crests P/8 wide on the cylinder, down to a root P/4 wide.
    const double outer = 7.0 * pitch / 16.0 + past * kTan30;
    return {{-outer, radius + past}, {-pitch / 8.0, radius - depth}, {pitch / 8.0, radius - depth},
            {outer, radius + past}};
  }
  // Between crests P/4 wide on the bore, out to a root P/8 wide.
  const double inner = 3.0 * pitch / 8.0 + past * kTan30;
  return {{-inner, radius - past}, {inner, radius - past}, {pitch / 16.0, radius + depth},
          {-pitch / 16.0, radius + depth}};
}

// The groove swept along a helix on the cylinder from `from` to `to` along
// its axis.
TopoDS_Shape groove(const CylinderFace& cylinder, double from, double to, const ThreadSpec& spec) {
  const gp_Dir along = cylinder.axis.Direction();
  const gp_Dir across =
      std::abs(along.X()) < 0.9 ? along.Crossed(gp_Dir(1, 0, 0)) : along.Crossed(gp_Dir(0, 1, 0));
  const gp_Pnt origin = cylinder.axis.Location().Translated(gp_Vec(along) * from);
  const double length = to - from;
  const occ::handle<Geom_CylindricalSurface> surface =
      new Geom_CylindricalSurface(gp_Ax3(origin, along, across), cylinder.radius);
  // On the unrolled cylinder the helix is a line: a turn of 2 pi about the
  // axis rises one pitch. A left-hand helix runs down from the far end as it
  // turns the same way. (Turning the other way instead, or mirroring a
  // right-hand groove, gives sweeps that OCCT's boolean cuts wrongly.)
  const gp_Pnt2d first(0.0, spec.right_handed ? 0.0 : length);
  const occ::handle<Geom2d_Line> line =
      new Geom2d_Line(first, gp_Dir2d(kTwoPi, spec.right_handed ? spec.pitch : -spec.pitch));
  const double turns = length / spec.pitch;
  BRepBuilderAPI_MakeEdge helix(line, surface, 0.0, turns * std::hypot(kTwoPi, spec.pitch));
  if (!helix.IsDone()) {
    throw std::runtime_error("the thread's helix could not be built");
  }
  TopoDS_Edge edge = helix.Edge();
  BRepLib::BuildCurves3d(edge, 1.0e-6, GeomAbs_C1, 14, std::max(30, static_cast<int>(turns * 16.0)));
  const TopoDS_Wire spine = BRepBuilderAPI_MakeWire(edge).Wire();

  // The section where the helix starts.
  BRepBuilderAPI_MakePolygon section;
  const gp_Pnt start = origin.Translated(gp_Vec(along) * first.Y());
  for (const gp_Pnt2d& corner : groove_section(spec.pitch, spec.depth, cylinder.radius, cylinder.internal)) {
    section.Add(start.Translated(gp_Vec(along) * corner.X() + gp_Vec(across) * corner.Y()));
  }
  section.Close();
  BRepOffsetAPI_MakePipeShell pipe(spine);
  // A fixed binormal along the axis keeps the section in the axial plane.
  pipe.SetMode(along);
  // Fine enough for the boolean with the body.
  pipe.SetTolerance(1.0e-6, 1.0e-6, 1.0e-4);
  pipe.SetMaxSegments(std::max(30, static_cast<int>(turns * 32.0)));
  pipe.Add(section.Wire());
  detail::build(pipe);
  if (!pipe.IsDone() || !pipe.MakeSolid()) {
    throw std::runtime_error("the thread's groove could not be swept");
  }
  return pipe.Shape();
}

// The groove from `from` to `to` as one sweep per turn, tiled from the
// helix's start (the low end of a right-hand thread, the high end of a
// left-hand one) so that the turns meet in the same section. A single
// sweep over many turns, or along a drilled hole's wall, makes OCCT's cut
// remove nothing or everything; one turn at a time is exact.
TopoDS_Shape groove_turns(const CylinderFace& cylinder, double from, double to, const ThreadSpec& spec) {
  BRep_Builder builder;
  TopoDS_Compound turns;
  builder.MakeCompound(turns);
  const double tolerance = 1.0e-9 * std::max(1.0, to - from);
  for (double done = 0.0; done < to - from - tolerance; done += spec.pitch) {
    const double length = std::min(spec.pitch, to - from - done);
    const double a = spec.right_handed ? from + done : to - done - length;
    builder.Add(turns, groove(cylinder, a, a + length, spec));
  }
  return turns;
}

// The body with the groove cut out; the groove's faces are named `name`.
ShapePtr cut_groove(const Shape& body, const TopoDS_Shape& groove, const std::string& name) {
  std::vector<Shape::NamedFace> names;
  for (TopExp_Explorer f(groove, TopAbs_FACE); f.More(); f.Next()) {
    names.push_back({f.Current(), {name}});
  }
  const Shape tool(groove, names);
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(body.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(groove);
  cut.SetArguments(arguments);
  cut.SetTools(tools);
  cut.SetNonDestructive(true);
  detail::build(cut);
  if (!cut.IsDone() || cut.HasErrors()) {
    throw std::runtime_error("the thread could not be cut into the body");
  }
  detail::FaceNamer namer(cut.Shape());
  namer.carry(cut, body);
  namer.carry(cut, tool);
  return namer.shape();
}

} // namespace

ShapePtr modeled_thread(const Shape& body, const ThreadSpec& spec) {
  if (spec.faces.empty()) {
    throw std::invalid_argument("no faces to thread");
  }
  detail::require_positive("thread pitch", spec.pitch);
  detail::require_positive("thread depth", spec.depth);
  if (!spec.full_length) {
    detail::require_positive("thread length", spec.length);
    detail::require_finite("thread offset", spec.offset);
    if (spec.offset < 0.0) {
      throw std::invalid_argument("the thread offset must not be negative");
    }
  }
  return detail::run("thread", [&] {
    ShapePtr current = std::make_shared<Shape>(body);
    for (const std::string& face : spec.faces) {
      const CylinderFace cylinder = face_cylinder(*current, face);
      if (!cylinder.internal && cylinder.radius - spec.depth <= 0.0) {
        throw std::invalid_argument("face " + face + " is too thin for the thread");
      }
      const double tolerance = 1.0e-6;
      double from = 0.0;
      double to = cylinder.length;
      if (!spec.full_length) {
        if (spec.high_end) {
          to = cylinder.length - spec.offset;
          from = to - spec.length;
        } else {
          from = spec.offset;
          to = from + spec.length;
        }
        if (from < -tolerance || to > cylinder.length + tolerance) {
          throw std::invalid_argument("the thread does not fit on face " + face);
        }
      }
      // Run out through the ends of the face.
      if (from <= tolerance) {
        from -= spec.pitch;
      }
      if (to >= cylinder.length - tolerance) {
        to += spec.pitch;
      }
      current = cut_groove(*current, groove_turns(cylinder, from, to, spec),
                           face_name(spec.feature, "thread", face));
    }
    return detail::finished(*current, "the thread");
  });
}

} // namespace mitcad::geometry
