// SPDX-License-Identifier: MIT
#include "mitcad/geometry/thread.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepLib.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt2d.hxx>

#include "history.hpp"
#include "local_boolean.hpp"
#include "mitcad/geometry/reference.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
constexpr double kTan30 = 0.57735026918962576451;
// Radii closer than this are the same.
constexpr double kSame = 1.0e-6;

// One face's thread: the face's cylinder and the profile on it.
struct Thread {
  // On the axis at the face's low end, the axis, and the direction the
  // thread's angle is measured from.
  gp_Pnt origin;
  gp_Dir along;
  gp_Dir reference;
  double face_radius = 0.0;
  double length = 0.0;
  bool internal = false;

  double pitch = 0.0;
  double pitch_radius = 0.0;
  double major_radius = 0.0;
  double minor_radius = 0.0;
  bool right_handed = true;
  double angle = 0.0;

  double crest() const { return internal ? minor_radius : major_radius; }
  double root() const { return internal ? major_radius : minor_radius; }

  // Half the width of the groove (the space between the teeth) at a
  // radius: a quarter pitch at the pitch diameter, wider towards the crest
  // along flanks at 30 degrees.
  double groove_half(double radius) const {
    return pitch / 4.0 + (internal ? pitch_radius - radius : radius - pitch_radius) * kTan30;
  }

  // The middle of the groove at the pitch diameter, along the axis from
  // the origin, at an angle about the axis from the reference direction.
  double groove_centre(double theta) const {
    const double turn = (right_handed ? 1.0 : -1.0) * pitch * (theta - angle) / kTwoPi;
    return (internal ? 0.75 : 0.25) * pitch + turn;
  }

  gp_Dir direction(double theta) const {
    return reference.Rotated(gp_Ax1(origin, along), theta);
  }

  gp_Pnt point(double s, double theta, double radius) const {
    return origin.Translated(gp_Vec(along) * s + gp_Vec(direction(theta)) * radius);
  }
};

// The direction a thread's angle is measured from: the X axis projected
// across the axis (Y for axes near X).
gp_Dir reference_direction(const gp_Dir& along) {
  const gp_Vec x = std::abs(along.X()) > 0.9 ? gp_Vec(0, 1, 0) : gp_Vec(1, 0, 0);
  return gp_Dir(x - gp_Vec(along) * x.Dot(gp_Vec(along)));
}

// A solid tube about the thread's axis between two radii (a cylinder from
// radius 0) and two positions along the axis.
TopoDS_Shape tube(const Thread& t, double inner, double outer, double from, double to) {
  const gp_Ax2 frame(t.origin.Translated(gp_Vec(t.along) * from), t.along, t.reference);
  TopoDS_Shape shape = BRepPrimAPI_MakeCylinder(frame, outer, to - from).Shape();
  if (inner > kSame) {
    BRepAlgoAPI_Cut cut(shape, BRepPrimAPI_MakeCylinder(frame, inner, to - from).Shape());
    if (!cut.IsDone() || cut.HasErrors()) {
      throw std::runtime_error("the thread's tube could not be built");
    }
    shape = cut.Shape();
  }
  return shape;
}

// The groove over several turns from the angle `theta`: its axial section
// between two radii swept along the helix of its middle, a spine of one
// edge per turn. (A spine of one edge over many turns makes OCCT's cut
// remove nothing or everything; separate sweeps per turn touch each other
// at their ends, which the local boolean cannot take.)
// The common of a shape and a tool, which a cancel request stops inside
// (detail::build): the grooves' common is seconds of the thread's time.
TopoDS_Shape common_of(const TopoDS_Shape& shape, const TopoDS_Shape& tool, const char* failure) {
  BRepAlgoAPI_Common common;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(shape);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(tool);
  common.SetArguments(arguments);
  common.SetTools(tools);
  detail::build(common);
  if (!common.IsDone() || common.HasErrors()) {
    throw std::runtime_error(failure);
  }
  return common.Shape();
}

TopoDS_Shape groove_sweep(const Thread& t, double theta, int turns, double low, double high) {
  const gp_Dir start_direction = t.direction(theta);
  const double centre = t.groove_centre(theta);
  const gp_Pnt base = t.origin.Translated(gp_Vec(t.along) * centre);
  const occ::handle<Geom_CylindricalSurface> surface =
      new Geom_CylindricalSurface(gp_Ax3(base, t.along, start_direction), t.pitch_radius);
  // On the unrolled cylinder the helix is a line: a turn of 2 pi about the
  // axis rises one pitch, or falls one for a left-hand thread. (Turning the
  // other way instead, or mirroring a right-hand groove, gives sweeps that
  // OCCT's boolean cuts wrongly.)
  const occ::handle<Geom2d_Line> line =
      new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(kTwoPi, t.right_handed ? t.pitch : -t.pitch));
  const double turn = std::hypot(kTwoPi, t.pitch);
  BRepBuilderAPI_MakeWire wire;
  for (int k = 0; k < turns; ++k) {
    BRepBuilderAPI_MakeEdge helix(line, surface, turn * k, turn * (k + 1));
    if (!helix.IsDone()) {
      throw std::runtime_error("the thread's helix could not be built");
    }
    TopoDS_Edge edge = helix.Edge();
    BRepLib::BuildCurves3d(edge, 1.0e-6, GeomAbs_C1, 14, 30);
    wire.Add(edge);
  }
  const TopoDS_Wire spine = wire.Wire();

  // The section in the axial plane where the helix starts.
  const auto corner = [&](double side, double radius) {
    return base.Translated(gp_Vec(t.along) * (side * t.groove_half(radius)) + gp_Vec(start_direction) * radius);
  };
  BRepBuilderAPI_MakePolygon section(corner(-1.0, low), corner(1.0, low), corner(1.0, high), corner(-1.0, high),
                                     true);
  BRepOffsetAPI_MakePipeShell pipe(spine);
  // A fixed binormal along the axis keeps the section in the axial plane.
  pipe.SetMode(t.along);
  // Fine enough for the boolean with the body and for the volume.
  pipe.SetTolerance(1.0e-6, 1.0e-6, 1.0e-4);
  pipe.SetMaxSegments(32);
  pipe.Add(section.Wire());
  detail::build(pipe);
  if (!pipe.IsDone() || !pipe.MakeSolid()) {
    throw std::runtime_error("the thread's groove could not be swept");
  }
  return pipe.Shape();
}

// The grooves between two radii, from `from` to `to` along the axis: whole
// turns covering more than a pitch past both ends, cut to the length by a
// cylinder of radius `outer`.
TopoDS_Shape grooves(const Thread& t, double low, double high, double from, double to, double outer) {
  const double sign = t.right_handed ? 1.0 : -1.0;
  // The turns start a pitch before the start of the thread's helix (the
  // low end of a right-hand thread, the high end of a left-hand one).
  const double target = t.right_handed ? from - 1.5 * t.pitch : to + 1.5 * t.pitch;
  const double first = t.angle + sign * kTwoPi * (target - t.groove_centre(t.angle)) / t.pitch;
  const int turns = static_cast<int>(std::ceil((to - from) / t.pitch)) + 3;
  return common_of(groove_sweep(t, first, turns, low, high), tube(t, 0.0, outer, from, to),
                   "the thread's grooves could not be cut to length");
}

// Whether the body has material just past the end of the threaded part
// (`side` -1 below `at`, +1 above it) where the thread's tools would
// reach: the radii between `inner` and `outer` at eight angles.
bool material_past(const TopoDS_Shape& body, const Thread& t, double at, double side, double inner,
                   double outer) {
  BRepClass3d_SolidClassifier classifier(body);
  const double s = at + side * 0.25 * t.pitch;
  for (int k = 0; k < 8; ++k) {
    for (const double f : {0.05, 0.5, 0.95}) {
      classifier.Perform(t.point(s, kTwoPi * k / 8.0, inner + f * (outer - inner)), 1.0e-7);
      if (classifier.State() != TopAbs_OUT) {
        return true;
      }
    }
  }
  return false;
}

// Whether an axis points "up": its Z component positive, or for axes
// across Z its Y component, or for the X axis X. The thread's helix is
// placed in that direction, whichever way the face's surface runs.
bool upwards(const gp_Dir& d) {
  constexpr double kAcross = 1.0e-9;
  if (std::abs(d.Z()) > kAcross) {
    return d.Z() > 0.0;
  }
  if (std::abs(d.Y()) > kAcross) {
    return d.Y() > 0.0;
  }
  return d.X() > 0.0;
}

// The face's thread with its axis pointing up (`upwards`) from the face's
// low end that way.
Thread thread_of(const CylinderFace& cylinder, const ThreadSpec& spec) {
  Thread t;
  t.origin = cylinder.axis.Location();
  t.along = cylinder.axis.Direction();
  if (!upwards(t.along)) {
    t.origin.Translate(gp_Vec(t.along) * cylinder.length);
    t.along.Reverse();
  }
  t.reference = reference_direction(t.along);
  t.face_radius = cylinder.radius;
  t.length = cylinder.length;
  t.internal = cylinder.internal;
  t.pitch = spec.pitch;
  t.pitch_radius = spec.pitch_diameter / 2.0;
  t.major_radius = spec.major / 2.0;
  t.minor_radius = spec.minor / 2.0;
  t.right_handed = spec.right_handed;
  t.angle = spec.angle;
  return t;
}

// The solid between a face's pieces and their copies on the cylinder of
// radius `radius` about the same axis (the pieces thickened along their
// radii).
TopoDS_Shape radial_band(const Shape& body, const std::string& face, double face_radius, double radius) {
  BRep_Builder builder;
  TopoDS_Compound band;
  builder.MakeCompound(band);
  for (const int i : body.find_faces(face)) {
    // The offset writes into its input. Its direction is the surface's
    // normal, whichever way the face runs: the side is checked.
    const detail::InputCopy copy(body.face(i));
    TopoDS_Shape thick;
    for (const double distance : {radius - face_radius, face_radius - radius}) {
      BRepOffset_MakeOffset maker;
      maker.Initialize(copy.shape(), distance, 1.0e-7, BRepOffset_Skin, false, false, GeomAbs_Arc, true);
      detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
      if (!maker.IsDone()) {
        continue;
      }
      bool reaches = false;
      for (TopExp_Explorer f(maker.Shape(), TopAbs_FACE); f.More() && !reaches; f.Next()) {
        const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
        reaches = surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - radius) < 1.0e-6;
      }
      if (reaches) {
        thick = maker.Shape();
        break;
      }
    }
    if (thick.IsNull()) {
      throw std::runtime_error("the thread's teeth could not be built on face " + face);
    }
    for (TopExp_Explorer it(thick, TopAbs_SOLID); it.More(); it.Next()) {
      builder.Add(band, it.Current());
    }
  }
  return band;
}

// One face's thread on the body: the teeth added where the face lies
// inside the crest, then the grooves (and what lies beyond the crest)
// removed.
ShapePtr thread_face(const Shape& body, const std::string& face_reference, const Thread& t, double from, double to,
                     const std::string& name) {
  const double margin = 0.1 * t.pitch;
  // The face inside the crest: the band between them is filled first,
  // over the face itself (its pieces moved out to the crest, along the
  // radii).
  ShapePtr current = std::make_shared<Shape>(body);
  const double beyond = t.internal ? t.face_radius - t.crest() : t.crest() - t.face_radius;
  if (beyond > kSame) {
    TopoDS_Shape band = radial_band(body, face_reference, t.face_radius, t.crest());
    if (from > kSame || to < t.length - kSame) {
      band = common_of(band, tube(t, 0.0, std::max(t.face_radius, t.crest()) + margin, from, to),
                       "the thread's teeth could not be cut to length");
    }
    current = detail::local_boolean(*current, band, detail::LocalOperation::Fuse, name).shape;
  }
  const double face = beyond > kSame ? t.crest() : t.face_radius;

  // Past the ends of the face, where nothing lies beyond them, the tools
  // run out by a pitch.
  const double inner = t.internal ? std::max(0.0, std::min(face, t.crest()) - margin) : t.minor_radius;
  const double outer = t.internal ? t.major_radius : std::max(face, t.crest()) + margin;
  if (from <= kSame && !material_past(current->occt(), t, from, -1.0, inner, outer)) {
    from -= t.pitch;
  }
  if (to >= t.length - kSame && !material_past(current->occt(), t, to, 1.0, inner, outer)) {
    to += t.pitch;
  }

  // What lies beyond the crest goes first, then the grooves from the root
  // to a little past the crest (no further: the grooves of neighbouring
  // turns must not meet).
  if (std::abs(face - t.crest()) > kSame) {
    const TopoDS_Shape beyond_crest = t.internal ? tube(t, std::max(0.0, face - margin), t.minor_radius, from, to)
                                                 : tube(t, t.major_radius, face + margin, from, to);
    current = detail::local_boolean(*current, beyond_crest, detail::LocalOperation::Cut, name).shape;
  }
  // The flanks meet H/2 = 0.433 P from the pitch diameter.
  const double reach = 0.433 * t.pitch;
  TopoDS_Shape cut;
  if (t.internal) {
    const double past = std::min(margin / 2.0, 0.5 * (t.minor_radius - (t.pitch_radius - reach)));
    cut = grooves(t, t.minor_radius - past, t.major_radius, from, to, t.major_radius + margin);
  } else {
    const double past = std::min(margin / 2.0, 0.5 * (t.pitch_radius + reach - t.major_radius));
    cut = grooves(t, t.minor_radius, t.major_radius + past, from, to, t.major_radius + margin);
  }
  return detail::local_boolean(*current, cut, detail::LocalOperation::Cut, name).shape;
}

} // namespace

ShapePtr modeled_thread(const Shape& body, const ThreadSpec& spec) {
  if (spec.faces.empty()) {
    throw std::invalid_argument("no faces to thread");
  }
  detail::require_positive("thread pitch", spec.pitch);
  detail::require_positive("thread major diameter", spec.major);
  detail::require_positive("thread minor diameter", spec.minor);
  detail::require_positive("thread pitch diameter", spec.pitch_diameter);
  detail::require_finite("thread angle", spec.angle);
  if (!(spec.minor < spec.pitch_diameter && spec.pitch_diameter < spec.major)) {
    throw std::invalid_argument("the thread's diameters must grow from the minor to the pitch to the major one");
  }
  // The 60-degree flanks meet H/2 = 0.433 P on either side of the pitch
  // diameter: the crests and roots must lie within that.
  const double reach = 0.4330127 * spec.pitch;
  if (spec.major - spec.pitch_diameter >= 2.0 * reach || spec.pitch_diameter - spec.minor >= 2.0 * reach) {
    throw std::invalid_argument("the thread's diameters are too far apart for its pitch");
  }
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
      if (!cylinder.internal && cylinder.radius <= spec.minor / 2.0 + kSame) {
        throw std::invalid_argument("face " + face + " is too thin for the thread");
      }
      if (cylinder.internal && cylinder.radius >= spec.major / 2.0 - kSame) {
        throw std::invalid_argument("face " + face + " is too wide for the thread");
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
        from = std::max(from, 0.0);
        to = std::min(to, cylinder.length);
      }
      const Thread thread = thread_of(cylinder, spec);
      if (!thread.along.IsEqual(cylinder.axis.Direction(), 1.0e-9)) {
        // Measured along the thread's axis.
        std::swap(from, to);
        from = cylinder.length - from;
        to = cylinder.length - to;
      }
      current = thread_face(*current, face, thread, from, to, face_name(spec.feature, "thread", face));
    }
    // Number the pieces of split names, and check the faces that changed.
    detail::FaceNamer namer(current->occt());
    for (int i = 0; i < current->face_count(); ++i) {
      for (const std::string& name : current->face_names(i)) {
        namer.add(current->face(i), name);
      }
    }
    namer.finish();
    // The faces that are not the body's.
    ShapeMap before;
    TopExp::MapShapes(body.occt(), TopAbs_FACE, before);
    BRep_Builder builder;
    TopoDS_Compound changed;
    builder.MakeCompound(changed);
    for (TopExp_Explorer f(namer.result(), TopAbs_FACE); f.More(); f.Next()) {
      if (!before.Contains(f.Current())) {
        builder.Add(changed, f.Current());
      }
    }
    detail::require_valid_near(namer.result(), changed, "the thread");
    return namer.shape();
  });
}

} // namespace mitcad::geometry
