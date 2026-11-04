// SPDX-License-Identifier: MIT
#include "mitcad/geometry/thread.hpp"

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <functional>
#include <limits>
#include <new>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_List.hxx>
#include <OSD_Parallel.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_GTrsf2d.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
// OSD_Parallel.hxx brings in windows.h on Windows, whose empty near and far
// macros would remove those names below.
#ifdef near
#undef near
#endif
#ifdef far
#undef far
#endif

#include "helix_brep.hpp"
#include "history.hpp"
#include "local_boolean.hpp"
#include "mitcad/geometry/reference.hpp"
#include "sweep.hpp"
#include "thread_options.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
// The most starts a thread may have (mitcad#893).
constexpr int kMaxStarts = 64;
// The 60-degree profile's flank angle to the radius, 30 degrees, and its
// tangent and cosine.
constexpr double kFlank60 = 0.52359877559829887308;
constexpr double kTan30 = 0.57735026918962576451;
constexpr double kCos30 = 0.86602540378443864676;

// The flanks of a profile (mitcad#893): the tangent and the cosine of their
// angle to the radius, and how far from the pitch diameter the flanks of a
// tooth without relief meet, in pitches (1 / (4 tan)). The 60-degree
// profile's are the values its threads were made with before (0.433 for
// 0.4330127), so that they come out as they did.
struct Flanks {
  double tan = kTan30;
  double cos = kCos30;
  double meet = 0.433;
};

Flanks flanks_of(double angle) {
  Flanks flanks;
  if (std::abs(angle - kFlank60) > 1.0e-12) {
    flanks.tan = std::tan(angle);
    flanks.cos = std::cos(angle);
    flanks.meet = 0.25 / flanks.tan;
  }
  return flanks;
}
// Radii closer than this are the same.
constexpr double kSame = 1.0e-6;
// The narrowest crest a flank clearance may leave, in pitches.
constexpr double kMinCrest = 0.02;
// How far short of an uneven end against the body a thread's tools stop
// (mitcad#901), in pitches.
constexpr double kUnevenEndClearance = 0.01;
// How near the screw surfaces of the grooves' faces keep to the screw
// motion (mm), finer than the sweeps a turn at a time (on_screw_surfaces).
constexpr double kScrewExact = 1.0e-7;

using Clock = std::chrono::steady_clock;

double seconds_since(Clock::time_point start) {
  return std::chrono::duration<double>(Clock::now() - start).count();
}

// The value of an environment variable, if it is set.
std::optional<std::string> environment(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, name) != 0 || value == nullptr) {
    return std::nullopt;
  }
  std::string text(value);
  std::free(value);
  return text;
#else
  const char* value = std::getenv(name);
  if (value == nullptr) {
    return std::nullopt;
  }
  return std::string(value);
#endif
}

// Whether MITCAD_BOOLEAN_TRACE asks for the threads' lines on standard
// error too (mitcad#843): per face where its time goes (its tools, the
// booleans with the body, the checks of the material past its ends), per
// thread its phases.
bool tracing() {
  static const bool on = environment("MITCAD_BOOLEAN_TRACE").has_value();
  return on;
}

// Where one face's thread spends its time (tracing()).
struct FaceTrace {
  double tools = 0.0;
  double booleans = 0.0;
  double checks = 0.0;
};

double* tools_time(FaceTrace* trace) { return trace != nullptr ? &trace->tools : nullptr; }
double* booleans_time(FaceTrace* trace) { return trace != nullptr ? &trace->booleans : nullptr; }
double* checks_time(FaceTrace* trace) { return trace != nullptr ? &trace->checks : nullptr; }

// Adds the time from its making to its end to `total` when tracing.
class Timed {
public:
  explicit Timed(double* total) : m_total(tracing() ? total : nullptr), m_start(Clock::now()) {}
  ~Timed() {
    if (m_total != nullptr) {
      *m_total += seconds_since(m_start);
    }
  }
  Timed(const Timed&) = delete;
  Timed& operator=(const Timed&) = delete;

private:
  double* m_total;
  Clock::time_point m_start;
};

// The phases of one thread, traced when it ends (tracing()), as the
// booleans' "boolean-trace" lines.
class ThreadTrace {
public:
  explicit ThreadTrace(std::string what) : m_what(std::move(what)), m_start(Clock::now()), m_last(m_start) {}
  void mark(const std::string& phase) {
    if (!tracing()) {
      return;
    }
    m_line += " " + phase + "=" + std::to_string(seconds_since(m_last));
    m_last = Clock::now();
  }
  void note(const std::string& text) {
    if (tracing()) {
      m_line += " " + text;
    }
  }
  ~ThreadTrace() {
    if (tracing()) {
      mark("rest");
      std::fprintf(stderr, "thread-trace: %s total=%f%s\n", m_what.c_str(), seconds_since(m_start), m_line.c_str());
    }
  }
  ThreadTrace(const ThreadTrace&) = delete;
  ThreadTrace& operator=(const ThreadTrace&) = delete;

private:
  std::string m_what;
  std::string m_line;
  Clock::time_point m_start;
  Clock::time_point m_last;
};

void trace_face(const std::string& feature, const std::string& face, const char* how, const FaceTrace& trace) {
  if (tracing()) {
    std::fprintf(stderr, "thread-trace: %s face=%s %s tools=%f booleans=%f checks=%f\n", feature.c_str(),
                 face.c_str(), how, trace.tools, trace.booleans, trace.checks);
  }
}

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
  // The number of starts and the helices' lead, starts x pitch (mitcad#893).
  int starts = 1;
  double lead = 0.0;
  // The flanks (flanks_of): 30 degrees to the radius for the 60-degree
  // profile, 15 for the trapezoidal one.
  double flank_tan = kTan30;
  double flank_cos = kCos30;
  double flank_meet = 0.433;
  double pitch_radius = 0.0;
  double major_radius = 0.0;
  double minor_radius = 0.0;
  bool right_handed = true;
  double angle = 0.0;
  // How far the flank clearance moves each flank along the axis, into the
  // tooth (the radii above already carry the radial clearance).
  double relief = 0.0;
  // A taper pipe thread (mitcad#733): how much every radius grows per
  // length along `along`; the radii above are those at the origin (0 for a
  // straight thread).
  double slope = 0.0;

  double crest() const { return internal ? minor_radius : major_radius; }
  double root() const { return internal ? major_radius : minor_radius; }

  // Half the width of the groove (the space between the teeth) at a
  // radius: a quarter pitch at the pitch diameter, wider towards the crest
  // along the flanks (at 30 degrees to the radius, 15 for a trapezoidal
  // thread), and wider by the flank relief.
  double groove_half(double radius) const {
    return pitch / 4.0 + relief + (internal ? pitch_radius - radius : radius - pitch_radius) * flank_tan;
  }

  // How far from the pitch diameter the flanks of a tooth meet: P / (4 tan
  // of the flank angle), H/2 = 0.433 P for the 60-degree profile, less
  // where the flank relief has thinned the tooth.
  double reach() const { return flank_meet * pitch - relief / flank_tan; }

  // The middle of the first start's groove at the pitch diameter, along
  // the axis from the origin, at an angle about the axis from the reference
  // direction: it rises a lead a turn (the other starts' grooves lie a
  // pitch apart from it, mitcad#893).
  double groove_centre(double theta) const {
    const double turn = (right_handed ? 1.0 : -1.0) * lead * (theta - angle) / kTwoPi;
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
  // (A taper pipe thread's radii where the section is, mitcad#733.)
  const double grown = t.slope * centre;
  occ::handle<Geom_Surface> surface;
  // Along the surface's second parameter per turn: the lead along the axis.
  double rise = t.lead;
  if (t.slope == 0.0) {
    surface = new Geom_CylindricalSurface(gp_Ax3(base, t.along, start_direction), t.pitch_radius);
  } else {
    // The helix of a taper pipe thread on the cone of its pitch diameter:
    // the second parameter runs along the cone's line, 1 / cos of its
    // half-angle per length along the axis.
    const double half_angle = std::atan(t.slope);
    surface = new Geom_ConicalSurface(gp_Ax3(base, t.along, start_direction), half_angle, t.pitch_radius + grown);
    rise = t.lead / std::cos(half_angle);
  }
  // On the unrolled cylinder the helix is a line: a turn of 2 pi about the
  // axis rises one lead, or falls one for a left-hand thread. (Turning the
  // other way instead, or mirroring a right-hand groove, gives sweeps that
  // OCCT's boolean cuts wrongly.)
  const occ::handle<Geom2d_Line> line =
      new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(kTwoPi, t.right_handed ? rise : -rise));
  const double turn = std::hypot(kTwoPi, rise);
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
    return base.Translated(gp_Vec(t.along) * (side * t.groove_half(radius)) +
                           gp_Vec(start_direction) * (radius + grown));
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

// The surfaces groove_sweep's faces approximate a turn at a time: each
// edge of the groove's section moved by the thread's screw motion over all
// the turns (detail::screwed_surfaces with the same section, from the same
// start: for a left-hand groove its other end, where the screw motion
// rises from), as the bodies stored in files keep threads.
std::vector<occ::handle<Geom_Surface>> screw_surfaces(const Thread& t, double theta, int turns, double low,
                                                      double high) {
  const double start = t.right_handed ? theta : theta + kTwoPi * turns;
  const gp_Dir start_direction = t.direction(start);
  const gp_Pnt base = t.origin.Translated(gp_Vec(t.along) * t.groove_centre(start));
  const auto corner = [&](double side, double radius) {
    return base.Translated(gp_Vec(t.along) * (side * t.groove_half(radius)) + gp_Vec(start_direction) * radius);
  };
  BRepBuilderAPI_MakePolygon section(corner(-1.0, low), corner(1.0, low), corner(1.0, high), corner(-1.0, high),
                                     true);
  detail::NamedWire loop;
  loop.wire = section.Wire();
  return detail::screwed_surfaces(loop, gp_Ax1(t.origin, t.along), t.lead, static_cast<double>(turns),
                                  !t.right_handed, kScrewExact);
}

// The turn about the thread's axis that takes its first start's groove to
// the k-th start's (mitcad#893): k / starts of a whole turn.
gp_Trsf start_turn(const Thread& t, int k) {
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(t.origin, t.along), kTwoPi * k / t.starts);
  return turn;
}

// The grooves between two radii, from `from` to `to` along the axis: whole
// turns covering more than a pitch past both ends, cut to the length by a
// cylinder of radius `outer`; for several starts, each start's groove (the
// first one turned about the axis). `screws` gets the screw surfaces its
// faces approximate (screw_surfaces).
TopoDS_Shape grooves(const Thread& t, double low, double high, double from, double to, double outer,
                     std::vector<occ::handle<Geom_Surface>>& screws) {
  const double sign = t.right_handed ? 1.0 : -1.0;
  // The turns start a pitch before the start of the thread's helix (the
  // low end of a right-hand thread, the high end of a left-hand one). The
  // other starts' grooves, the first one turned, lie up to a lead less a
  // pitch further back along the axis: three more turns than the length
  // takes cover them past both ends too.
  const double target = t.right_handed ? from - 1.5 * t.pitch : to + 1.5 * t.pitch;
  const double first = t.angle + sign * kTwoPi * (target - t.groove_centre(t.angle)) / t.lead;
  const int turns = static_cast<int>(std::ceil((to - from) / t.lead)) + 3;
  // (A taper pipe thread's faces lie on no screw surface.)
  if (t.slope == 0.0) {
    screws = screw_surfaces(t, first, turns, low, high);
    const std::size_t one = screws.size();
    for (int k = 1; k < t.starts; ++k) {
      const gp_Trsf turn = start_turn(t, k);
      for (std::size_t i = 0; i < one; ++i) {
        screws.push_back(occ::down_cast<Geom_Surface>(screws[i]->Transformed(turn)));
      }
    }
  }
  TopoDS_Shape sweep = groove_sweep(t, first, turns, low, high);
  if (t.starts > 1) {
    // The starts' grooves lie apart from each other (the teeth between
    // them): one argument of the common, a compound of them all.
    BRep_Builder builder;
    TopoDS_Compound all;
    builder.MakeCompound(all);
    builder.Add(all, sweep);
    for (int k = 1; k < t.starts; ++k) {
      BRepBuilderAPI_Transform turned(sweep, start_turn(t, k), true);
      if (!turned.IsDone()) {
        throw std::runtime_error("the thread's grooves could not be turned to its starts");
      }
      builder.Add(all, turned.Shape());
    }
    sweep = all;
  }
  return common_of(sweep, tube(t, 0.0, outer, from, to), "the thread's grooves could not be cut to length");
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

// How far a face covers its cylinder all round at its ends (mitcad#901),
// along the thread's axis: the highest point of its low end and the lowest
// point of its high end, angle by angle about the axis. A face that meets
// faces square to its axis at both ends gives 0 and the thread's length; a
// face whose end is uneven (a shank under a head's serrated underside, a
// wall at a slant) ends at those heights only at some angles. Only the
// face's boundary within a pitch of each end counts (an opening further
// in, a cross hole or a key seat, is not an end); angles where the face
// has no boundary at all (a face of part of a turn) are passed over, and
// so are its edges along the axis (a seam, a step of the end), whose ends
// the edges either side of them have.
struct FaceEnds {
  double low = 0.0;
  double high = 0.0;
};

FaceEnds face_ends(const Shape& body, const std::string& face, const Thread& t) {
  // Angles in bins of a tenth of a degree; the boundary sampled at most
  // half a bin's arc apart, so that each bin it crosses gets its points.
  constexpr int kBins = 3600;
  constexpr int kMostSamples = 20000;
  const double step = 0.5 * kTwoPi / kBins * t.face_radius;
  std::vector<double> lowest(kBins, std::numeric_limits<double>::infinity());
  std::vector<double> highest(kBins, -std::numeric_limits<double>::infinity());
  const gp_Vec along(t.along);
  const gp_Vec x(t.reference);
  const gp_Vec y(t.along.Crossed(t.reference));
  for (const int i : body.find_faces(face)) {
    for (TopExp_Explorer e(body.face(i), TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      if (curve.GetType() == GeomAbs_Line && curve.Line().Direction().IsParallel(t.along, 1.0e-9)) {
        continue;
      }
      const double first = curve.FirstParameter();
      const double last = curve.LastParameter();
      const double length = GCPnts_AbscissaPoint::Length(curve);
      const int samples = std::clamp(static_cast<int>(std::ceil(length / step)), 1, kMostSamples);
      for (int k = 0; k <= samples; ++k) {
        const gp_Vec v(t.origin, curve.Value(first + (last - first) * k / samples));
        const double s = v.Dot(along);
        double angle = std::atan2(v.Dot(y), v.Dot(x));
        if (angle < 0.0) {
          angle += kTwoPi;
        }
        const auto bin = static_cast<std::size_t>(std::min(kBins - 1, static_cast<int>(angle / kTwoPi * kBins)));
        lowest[bin] = std::min(lowest[bin], s);
        highest[bin] = std::max(highest[bin], s);
      }
    }
  }
  FaceEnds ends{0.0, t.length};
  for (int b = 0; b < kBins; ++b) {
    const auto bin = static_cast<std::size_t>(b);
    if (lowest[bin] <= t.pitch) {
      ends.low = std::max(ends.low, lowest[bin]);
    }
    if (highest[bin] >= t.length - t.pitch) {
      ends.high = std::min(ends.high, highest[bin]);
    }
  }
  return ends;
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
  t.starts = spec.starts;
  t.lead = spec.pitch * spec.starts;
  const Flanks flanks = flanks_of(spec.flank_angle);
  t.flank_tan = flanks.tan;
  t.flank_cos = flanks.cos;
  t.flank_meet = flanks.meet;
  // The radial clearance moves the whole profile away from the material:
  // out for a bore, in for a shaft.
  const double shift = cylinder.internal ? spec.radial_clearance : -spec.radial_clearance;
  t.pitch_radius = spec.pitch_diameter / 2.0 + shift;
  t.major_radius = spec.major / 2.0 + shift;
  t.minor_radius = spec.minor / 2.0 + shift;
  t.right_handed = spec.right_handed;
  t.angle = spec.angle;
  // A flank moved by c perpendicular to itself, at the flank angle to the
  // radius, moves c / cos(flank angle) along the axis.
  t.relief = spec.flank_clearance / t.flank_cos;
  return t;
}

// Copies of a face's pieces (InputCopy): the offset in radial_band writes
// into its input.
std::vector<TopoDS_Shape> face_pieces(const Shape& body, const std::string& face) {
  std::vector<TopoDS_Shape> pieces;
  for (const int i : body.find_faces(face)) {
    pieces.push_back(detail::InputCopy(body.face(i)).shape());
  }
  return pieces;
}

// The solid between a face's pieces (copies, face_pieces) and their copies
// on the cylinder of radius `radius` about the same axis (the pieces
// thickened along their radii).
TopoDS_Shape radial_band(const std::vector<TopoDS_Shape>& pieces, const std::string& face, double face_radius,
                         double radius) {
  BRep_Builder builder;
  TopoDS_Compound band;
  builder.MakeCompound(band);
  for (const TopoDS_Shape& piece : pieces) {
    // The offset's direction is the surface's normal, whichever way the
    // face runs: the side is checked.
    TopoDS_Shape thick;
    for (const double distance : {radius - face_radius, face_radius - radius}) {
      BRepOffset_MakeOffset maker;
      maker.Initialize(piece, distance, 1.0e-7, BRepOffset_Skin, false, false, GeomAbs_Arc, true);
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

// The end chamfers of one face (mitcad#304), in the thread's axis terms:
// whether its low and high end are to get one, the cone's radius at the
// end and the tangent of its angle to the axis, and the names of the cones.
struct EndChamfers {
  bool low = false;
  bool high = false;
  double radius = 0.0;
  double tan = 1.0;
  std::string low_name;
  std::string high_name;
};

// The solid an end chamfer takes away at `at` along the thread's axis (the
// low end with `side` -1, the high end with +1): for an external thread
// what lies outside the cone that widens from `radius` at the end into the
// material up to `outer`, for an internal one what lies inside the cone
// that narrows from `radius` down to `inner`; from a little beyond the end.
TopoDS_Shape chamfer_tool(const Thread& t, double at, double side, const EndChamfers& c, double inner, double outer) {
  // At `u` into the material from the end, at a radius.
  const auto point = [&](double u, double radius) {
    return t.origin.Translated(gp_Vec(t.along) * (at - side * u) + gp_Vec(t.reference) * radius);
  };
  BRepBuilderAPI_MakePolygon section;
  if (t.internal) {
    const double beyond = 0.1 * t.pitch;
    section.Add(point(-beyond, inner));
    section.Add(point(-beyond, c.radius + beyond * c.tan));
    section.Add(point((c.radius - inner) / c.tan, inner));
  } else {
    const double beyond = std::min(0.1 * t.pitch, 0.5 * c.radius / c.tan);
    section.Add(point(-beyond, c.radius - beyond * c.tan));
    section.Add(point(-beyond, outer));
    section.Add(point((outer - c.radius) / c.tan, outer));
  }
  section.Close();
  BRepBuilderAPI_MakeFace face(section.Wire(), true);
  if (!face.IsDone()) {
    throw std::runtime_error("the thread's end chamfer could not be built");
  }
  BRepPrimAPI_MakeRevol revol(face.Face(), gp_Ax1(t.origin, t.along), kTwoPi);
  if (!revol.IsDone()) {
    throw std::runtime_error("the thread's end chamfer could not be built");
  }
  return revol.Shape();
}

// How far into the material a chamfer's cone reaches the radius `material`
// (the face, or the thread's crest where that lies further into it), and
// checks that the cone cuts it.
double chamfer_depth(const Thread& t, const EndChamfers& c, double material, const std::string& face) {
  const double depth = (t.internal ? c.radius - material : material - c.radius) / c.tan;
  if (depth <= kSame) {
    std::ostringstream text;
    text << "the thread's chamfer diameter " << 2.0 * c.radius << " must be "
         << (t.internal ? "larger" : "smaller") << " than the diameter " << 2.0 * material << " of face " << face;
    throw std::invalid_argument(text.str());
  }
  return depth;
}

// The stages of a face's thread, in the order they change the body.
enum class Stage { Band, Taper, ChamferLow, ChamferHigh, BeyondCrest, Grooves };
constexpr Stage kStages[] = {Stage::Band,        Stage::Taper,       Stage::ChamferLow,
                             Stage::ChamferHigh, Stage::BeyondCrest, Stage::Grooves};

const char* stage_name(Stage stage) {
  switch (stage) {
  case Stage::Band:
    return "band";
  case Stage::Taper:
    return "taper";
  case Stage::ChamferLow:
    return "chamfer-low";
  case Stage::ChamferHigh:
    return "chamfer-high";
  case Stage::BeyondCrest:
    return "beyond-crest";
  case Stage::Grooves:
    return "grooves";
  }
  return "other";
}

// The band is fused to the body, the other tools are cut out of it.
detail::LocalOperation operation_of(Stage stage) {
  return stage == Stage::Band ? detail::LocalOperation::Fuse : detail::LocalOperation::Cut;
}

using Screws = std::vector<occ::handle<Geom_Surface>>;

// One tool of a face's thread: `build` makes it from the face's thread
// alone (shapes of its own, nothing of the body), a null shape where it
// takes nothing; the grooves' build gives `screws` the screw surfaces
// their faces approximate. Its faces get `name`.
struct Step {
  Stage stage;
  std::string name;
  std::function<TopoDS_Shape(Screws& screws)> build;
};

// The end chamfers at the free ends of the thread's part from `from` to
// `to` (`free_low`, `free_high`: nothing lies beyond those ends); `inner`
// and `outer` bound the material at the face, `material` the radius the
// cones cut.
void chamfer_steps(std::vector<Step>& steps, const Thread& t, double from, double to, const EndChamfers& c,
                   bool free_low, bool free_high, double inner, double outer, double material,
                   const std::string& face) {
  const bool low = c.low && free_low;
  const bool high = c.high && free_high;
  if (!low && !high) {
    return;
  }
  const double depth = chamfer_depth(t, c, material, face);
  if ((low ? depth : 0.0) + (high ? depth : 0.0) >= to - from - kSame) {
    throw std::invalid_argument("the thread's end chamfers on face " + face + " are deeper than the thread is long");
  }
  if (low) {
    steps.push_back({Stage::ChamferLow, c.low_name,
                     [t, from, c, inner, outer](Screws&) { return chamfer_tool(t, from, -1.0, c, inner, outer); }});
  }
  if (high) {
    steps.push_back({Stage::ChamferHigh, c.high_name,
                     [t, to, c, inner, outer](Screws&) { return chamfer_tool(t, to, 1.0, c, inner, outer); }});
  }
}

// A taper pipe thread's cone of radius `radius` (at the origin, growing by
// the slope along the axis) to cut out of the body over [from, to]
// (mitcad#733): for an internal thread what lies inside it, down to a
// little inside the face, for an external one what lies outside it, up to
// a little outside the face; from `beyond` past `from` and `to` (a free
// end's run-out), and only where the cone lies off the face. A null shape
// where the cone does not reach the face.
TopoDS_Shape taper_tool(const Thread& t, double radius, double from, double to, double beyond_from,
                        double beyond_to) {
  const double margin = 0.1 * t.pitch;
  const double face = t.face_radius;
  // The other radius of the tool's section: inside the face for an
  // internal thread, outside it for an external one.
  const double other = t.internal ? std::max(face - margin, 0.5 * face) : face + margin;
  const auto at = [&](double s) { return radius + t.slope * s; };
  // Where the cone crosses the other radius: the tool ends there.
  double low = from - beyond_from;
  double high = to + beyond_to;
  if (t.slope != 0.0) {
    const double crossing = (other - radius) / t.slope;
    // The cone lies off the face (inside the material) on one side of the
    // crossing: the side where the internal cone is wider, the external
    // one narrower than `other`.
    const bool before = t.internal == (t.slope < 0.0);
    if (before) {
      high = std::min(high, crossing);
    } else {
      low = std::max(low, crossing);
    }
  }
  const auto off_face = [&](double s) { return t.internal ? at(s) > face + kSame : at(s) < face - kSame; };
  if (high - low <= kSame || !(off_face(low) || off_face(high))) {
    return TopoDS_Shape();
  }
  const auto point = [&](double s, double r) {
    return t.origin.Translated(gp_Vec(t.along) * s + gp_Vec(t.reference) * r);
  };
  // (A corner where the cone meets the other radius is one point.)
  std::vector<gp_Pnt> corners{point(low, other)};
  if (std::abs(at(low) - other) > kSame) {
    corners.push_back(point(low, at(low)));
  }
  corners.push_back(point(high, at(high)));
  if (std::abs(at(high) - other) > kSame) {
    corners.push_back(point(high, other));
  }
  if (corners.size() < 3) {
    return TopoDS_Shape();
  }
  BRepBuilderAPI_MakePolygon section;
  for (const gp_Pnt& corner : corners) {
    section.Add(corner);
  }
  section.Close();
  BRepBuilderAPI_MakeFace profile(section.Wire(), true);
  if (!profile.IsDone()) {
    throw std::runtime_error("the thread's taper could not be built");
  }
  BRepPrimAPI_MakeRevol revol(profile.Face(), gp_Ax1(t.origin, t.along), kTwoPi);
  if (!revol.IsDone()) {
    throw std::runtime_error("the thread's taper could not be built");
  }
  return revol.Shape();
}

// A face's grooves from `from` to `to`, from the root to a little past the
// crest (no further: the grooves of neighbouring turns must not meet), cut
// to length within the radius `outer`.
TopoDS_Shape thread_grooves(const Thread& t, double from, double to, double outer, Screws& screws) {
  const double margin = 0.1 * t.pitch;
  // Where the flanks of a tooth meet.
  const double reach = t.reach();
  if (t.internal) {
    const double past = std::min(margin / 2.0, 0.5 * (t.minor_radius - (t.pitch_radius - reach)));
    return grooves(t, t.minor_radius - past, t.major_radius, from, to, outer, screws);
  }
  const double past = std::min(margin / 2.0, 0.5 * (t.pitch_radius + reach - t.major_radius));
  return grooves(t, t.minor_radius, t.major_radius + past, from, to, outer, screws);
}

// One face of a thread as modeled_thread reads it from the body and the
// spec (plan_face): the thread on the face's cylinder, its part along the
// axis, its end chamfers and its names.
struct FacePlan {
  std::string face;
  Thread thread;
  double from = 0.0;
  double to = 0.0;
  EndChamfers chamfers;
  // A taper pipe thread's free ends (mitcad#733), as its start was read.
  bool free_low = false;
  bool free_high = false;
  // Where a modelled straight thread's face ends all round (face_ends):
  // 0 and the thread's length for ends square to the axis, within for
  // uneven ones.
  double even_low = 0.0;
  double even_high = std::numeric_limits<double>::infinity();
  // A modelled thread's faces' name.
  std::string name;
  // Whether the thread's axis points against the face's cylinder's: the
  // low end along the thread is then the face's high end.
  bool reversed = false;
};

// The first step of a face's modelled straight thread where the face lies
// inside the crest: the band between them, filled over the face itself
// (its pieces, read from `body`, moved out to the crest along the radii).
std::vector<Step> band_steps(const Shape& body, const FacePlan& p, const ThreadSpec& spec) {
  std::vector<Step> steps;
  const Thread& t = p.thread;
  if (!spec.groove || t.slope != 0.0) {
    return steps;
  }
  const double beyond = t.internal ? t.face_radius - t.crest() : t.crest() - t.face_radius;
  if (beyond <= kSame) {
    return steps;
  }
  const std::vector<TopoDS_Shape> pieces = face_pieces(body, p.face);
  steps.push_back({Stage::Band, p.name, [t, pieces, face = p.face, from = p.from, to = p.to](Screws&) {
                     TopoDS_Shape band = radial_band(pieces, face, t.face_radius, t.crest());
                     if (from > kSame || to < t.length - kSame) {
                       const double margin = 0.1 * t.pitch;
                       band = common_of(band, tube(t, 0.0, std::max(t.face_radius, t.crest()) + margin, from, to),
                                        "the thread's teeth could not be cut to length");
                     }
                     return band;
                   }});
  return steps;
}

// The other steps of a face's thread, with the checks of the material past
// the ends of its part on `body` (the body as the band left it): a
// modelled straight thread's end chamfers, what lies beyond the crest and
// the grooves; a taper pipe thread's crest cone and grooves; a cosmetic
// thread's taper cone or end chamfers.
std::vector<Step> other_steps(const Shape& body, const FacePlan& p, const ThreadSpec& spec, FaceTrace* trace) {
  std::vector<Step> steps;
  const Thread& t = p.thread;
  double from = p.from;
  double to = p.to;
  const double margin = 0.1 * t.pitch;
  if (spec.groove && t.slope != 0.0) {
    // A taper pipe thread (mitcad#733): no teeth added (the face inside
    // the crest leaves them short, as a tap leaves a drilled bore); what
    // lies beyond the crest's cone goes, then the grooves between the
    // root's and the crest's cones, past the free ends by a pitch.
    const double inner = t.internal ? std::max(0.0, std::min(t.face_radius, t.crest()) - margin) : t.minor_radius;
    const double outer = t.internal ? t.major_radius : std::max(t.face_radius, t.crest()) + margin;
    // (The radii at either end of the part.)
    const double low_grown = t.slope * from;
    const double high_grown = t.slope * to;
    bool free_low = false;
    bool free_high = false;
    {
      const Timed timed(checks_time(trace));
      free_low = from <= kSame && !material_past(body.occt(), t, from, -1.0, inner + low_grown, outer + low_grown);
      free_high = to >= t.length - kSame &&
                  !material_past(body.occt(), t, to, 1.0, inner + high_grown, outer + high_grown);
    }
    const double run_low = free_low ? t.pitch : 0.0;
    const double run_high = free_high ? t.pitch : 0.0;
    steps.push_back({Stage::Taper, p.name, [t, from, to, run_low, run_high](Screws&) {
                       return taper_tool(t, t.crest(), from, to, run_low, run_high);
                     }});
    const double widest = std::max(std::abs(t.slope * (from - run_low)), std::abs(t.slope * (to + run_high)));
    const double reach_out = t.major_radius + widest + margin;
    steps.push_back({Stage::Grooves, p.name, [t, from, to, run_low, run_high, reach_out](Screws& screws) {
                       return thread_grooves(t, from - run_low, to + run_high, reach_out, screws);
                     }});
    return steps;
  }
  if (spec.groove) {
    const double beyond = t.internal ? t.face_radius - t.crest() : t.crest() - t.face_radius;
    const double face = beyond > kSame ? t.crest() : t.face_radius;
    // Past the ends of the face, where nothing lies beyond them, the tools
    // run out by a pitch.
    const double inner = t.internal ? std::max(0.0, std::min(face, t.crest()) - margin) : t.minor_radius;
    const double outer = t.internal ? t.major_radius : std::max(face, t.crest()) + margin;
    bool free_low = false;
    bool free_high = false;
    {
      const Timed timed(checks_time(trace));
      free_low = from <= kSame && !material_past(body.occt(), t, from, -1.0, inner, outer);
      free_high = to >= t.length - kSame && !material_past(body.occt(), t, to, 1.0, inner, outer);
    }
    // At an end against the body that is uneven (mitcad#901: a shank under
    // a head's serrated underside), the tools stop a little short of where
    // the face ends all round: up to its furthest point they would cut into
    // the material beyond the face where the end is shorter, and their end
    // would touch the body's edges there, which the booleans leave open.
    const double short_of = kUnevenEndClearance * t.pitch;
    if (!free_low && from <= kSame && p.even_low > from + kSame) {
      from = p.even_low + short_of;
    }
    if (!free_high && to >= t.length - kSame && p.even_high < to - kSame) {
      to = p.even_high - short_of;
    }
    if (to - from <= kSame) {
      throw std::invalid_argument("face " + p.face + " is too short all round for the thread");
    }
    // The end chamfers, before the grooves: the first turn starts on the
    // cone.
    const EndChamfers& chamfers = p.chamfers;
    if (chamfers.low || chamfers.high) {
      const double material = t.internal ? std::min(face, t.crest()) : std::max(face, t.crest());
      const double tool_inner = t.internal ? std::max(material - margin, 0.5 * material) : chamfers.radius;
      const double tool_outer = t.internal ? chamfers.radius : material + margin;
      chamfer_steps(steps, t, from, to, chamfers, free_low, free_high, tool_inner, tool_outer, material, p.face);
    }
    if (free_low) {
      from -= t.pitch;
    }
    if (free_high) {
      to += t.pitch;
    }
    // What lies beyond the crest goes first, then the grooves.
    if (std::abs(face - t.crest()) > kSame) {
      steps.push_back({Stage::BeyondCrest, p.name, [t, face, from, to, margin](Screws&) {
                         return t.internal ? tube(t, std::max(0.0, face - margin), t.minor_radius, from, to)
                                           : tube(t, t.major_radius, face + margin, from, to);
                       }});
    }
    const double reach_out = t.major_radius + margin;
    steps.push_back({Stage::Grooves, p.name, [t, from, to, reach_out](Screws& screws) {
                       return thread_grooves(t, from, to, reach_out, screws);
                     }});
    return steps;
  }
  if (spec.taper > 0.0) {
    // The cone of the bore's minor diameter, or of the shaft's major one,
    // past the free ends by a tenth of a pitch.
    const double run = 0.1 * t.pitch;
    const double radius = t.internal ? t.minor_radius : t.major_radius;
    const double run_low = p.free_low ? run : 0.0;
    const double run_high = p.free_high ? run : 0.0;
    steps.push_back({Stage::Taper, face_name(spec.feature, "thread_taper", p.face),
                     [t, radius, from, to, run_low, run_high](Screws&) {
                       return taper_tool(t, radius, from, to, run_low, run_high);
                     }});
    return steps;
  }
  // A cosmetic thread's end chamfers: no profile, only the cones at its
  // free ends.
  const EndChamfers& c = p.chamfers;
  if (!c.low && !c.high) {
    return steps;
  }
  const double inner = t.internal ? std::max(t.face_radius - margin, 0.5 * t.face_radius) : c.radius;
  const double outer = t.internal ? c.radius : t.face_radius + margin;
  bool free_low = false;
  bool free_high = false;
  {
    const Timed timed(checks_time(trace));
    free_low = from <= kSame && !material_past(body.occt(), t, from, -1.0, inner, outer);
    free_high = to >= t.length - kSame && !material_past(body.occt(), t, to, 1.0, inner, outer);
  }
  chamfer_steps(steps, t, from, to, c, free_low, free_high, inner, outer, t.face_radius, p.face);
  return steps;
}

// One face's thread on the body, its steps one after another: the band
// first, then the others with the checks on the body the band left.
ShapePtr thread_one_face(const Shape& body, const FacePlan& p, const ThreadSpec& spec, Screws& screws,
                         FaceTrace* trace) {
  ShapePtr current = std::make_shared<Shape>(body);
  const auto apply = [&](const std::vector<Step>& steps) {
    for (const Step& step : steps) {
      TopoDS_Shape tool;
      {
        const Timed timed(tools_time(trace));
        tool = step.build(screws);
      }
      if (tool.IsNull()) {
        continue;
      }
      const Timed timed(booleans_time(trace));
      current = detail::local_boolean(*current, tool, operation_of(step.stage), step.name).shape;
    }
  };
  std::vector<Step> band;
  {
    const Timed timed(tools_time(trace));
    band = band_steps(*current, p, spec);
  }
  apply(band);
  apply(other_steps(*current, p, spec, trace));
  return current;
}

// Checks that the extra clearance leaves a thread on an internal or an
// external face: the flank clearance a crest at least P/50 wide (it moves
// each flank flank_clearance / cos(flank angle) into the tooth, the crest
// keeping its radius), the radial clearance an external thread a core.
void check_clearance(const ThreadSpec& spec, bool internal) {
  const auto text = [](double value) {
    std::ostringstream out;
    out << value;
    return out.str();
  };
  if (spec.flank_clearance > 0.0) {
    // The crest's width before the relief: half a pitch at the pitch
    // diameter, narrowing along the flanks to the crest.
    const double depth = internal ? spec.pitch_diameter - spec.minor : spec.major - spec.pitch_diameter;
    const Flanks flanks = flanks_of(spec.flank_angle);
    const double crest = spec.pitch / 2.0 - depth * flanks.tan - 2.0 * spec.flank_clearance / flanks.cos;
    if (crest < kMinCrest * spec.pitch) {
      throw std::invalid_argument("the thread's flank clearance of " + text(spec.flank_clearance) +
                                  " mm leaves no usable tooth: its crest would be " + text(crest) +
                                  " mm wide, at least " + text(kMinCrest * spec.pitch) + " mm is needed");
    }
  }
  if (!internal && spec.minor / 2.0 - spec.radial_clearance <= kSame) {
    throw std::invalid_argument("the thread's radial clearance of " + text(spec.radial_clearance) +
                                " mm leaves the external thread no core");
  }
}

// Whether a face holds one of the threads' names, numbered or not.
bool thread_named(const NameList& names, const std::vector<std::string>& threads) {
  for (const std::string& name : names) {
    for (const std::string& thread : threads) {
      if (name.compare(0, thread.size(), thread) == 0 &&
          (name.size() == thread.size() || name[thread.size()] == '#')) {
        return true;
      }
    }
  }
  return false;
}

// A name per_turn_names numbers, with the axis of its thread. A thread's
// own faces (not the threaded face's pieces) are also counted from its
// ends (`low`, `high`: <feature>:low_thread(<face>) and
// <feature>:high_thread(<face>), the ends along the face's cylinder's axis
// as the end chamfers' cones name them), `reversed` when the thread's axis
// points against the cylinder's.
struct TurnNamed {
  std::string name;
  gp_Pnt origin;
  gp_Dir along;
  std::string low;
  std::string high;
  bool reversed = false;
};

// How far apart (mm, and mm^2 for areas) two pieces' places must lie to
// tell the pieces apart: far above the nanometres by which the booleans
// make equal turns differ between platforms (mitcad#490), far below the
// distance between two pieces of a thread.
constexpr double kPlaceTie = 1.0e-4;
// How far apart (in shape_key's units of 1e-6 mm) two pieces' centres must
// lie in X for the numbering used before mitcad#490 to order them alike on
// every platform.
constexpr long long kLegacyApart = 100;

// A piece's place on its thread: its centre of mass along the axis, the
// centre's distance from the axis and the piece's area, in that order.
struct Place {
  std::array<double, 3> key{};
  detail::ShapeKey legacy{};
  int face = 0;
};

// Orders `places` from `first` to `last` by their keys' components from
// `component` on, each telling two pieces apart only by more than
// kPlaceTie: sorted by the component, the pieces fall into runs whose
// neighbours lie within kPlaceTie of each other, and each run is ordered
// by the next component. No difference of rounding noise decides; pieces
// alike in every component (a thread has none) keep shape_key's order.
void order_places(std::vector<Place>& places, std::size_t first, std::size_t last, std::size_t component) {
  if (last - first < 2) {
    return;
  }
  const auto begin = places.begin() + static_cast<std::ptrdiff_t>(first);
  const auto end = places.begin() + static_cast<std::ptrdiff_t>(last);
  if (component == std::tuple_size<decltype(Place::key)>::value) {
    std::sort(begin, end, [](const Place& a, const Place& b) { return a.legacy < b.legacy; });
    return;
  }
  std::sort(begin, end, [component](const Place& a, const Place& b) { return a.key[component] < b.key[component]; });
  std::size_t run = first;
  for (std::size_t k = first + 1; k <= last; ++k) {
    if (k == last || places[k].key[component] - places[k - 1].key[component] > kPlaceTie) {
      order_places(places, run, k, component + 1);
      run = k;
    }
  }
}

// The faces named after each thread (its grooves' faces, a turn each, its
// crest's and its teeth's) and the threaded faces' pieces, numbered by
// their places along the thread's axis (order_places) before the pieces
// are put on their screw surfaces (on_screw_surfaces, mitcad#343): the
// same numbers on every platform (mitcad#490).
// The numbers start at the pieces' count, so that no number of the order
// used before names another piece now: that order (the centres of mass in
// X, then Y and Z, rounded to 1e-6 mm) took the pieces of whole turns,
// centred on the axis, in the order of their rounding noise, which differs
// between platforms. A piece whose number of that order was the same on
// every platform (its centre apart from all the others' in X) keeps that
// name as well, so that references saved before still find it; references
// to the other pieces find no face and are reported.
// A thread's own pieces are also named by their places from the nearer end
// of the face (mitcad#1231), `<feature>:low_thread(<face>)#k` or
// `<feature>:high_thread(<face>)#k`, k from 0 at the end: the names sort
// before the others, so the pieces' edges are named after them, and the
// edges at the thread's ends (where it meets the rest of its face, a
// chamfer's cone or the body's end) keep their names when turns come or go
// at the other end, by a length, an offset, a pitch or a profile change.
ShapePtr per_turn_names(const Shape& body, const std::vector<TurnNamed>& threads) {
  std::vector<NameList> names(static_cast<std::size_t>(body.face_count()));
  for (int i = 0; i < body.face_count(); ++i) {
    names[static_cast<std::size_t>(i)] = body.face_names(i);
  }
  for (const TurnNamed& thread : threads) {
    std::vector<Place> places;
    for (int i = 0; i < body.face_count(); ++i) {
      NameList& own = names[static_cast<std::size_t>(i)];
      const auto found = std::find(own.begin(), own.end(), thread.name);
      if (found == own.end()) {
        continue;
      }
      own.erase(found);
      GProp_GProps properties;
      BRepGProp::SurfaceProperties(body.face(i), properties);
      const gp_Vec centre(thread.origin, properties.CentreOfMass());
      const double along = centre.Dot(gp_Vec(thread.along));
      const double off = (centre - gp_Vec(thread.along) * along).Magnitude();
      places.push_back({{along, off, properties.Mass()}, detail::shape_key(body.face(i)), i});
    }
    if (places.size() == 1) {
      names[static_cast<std::size_t>(places.front().face)].push_back(thread.name);
      continue;
    }
    std::vector<Place> legacy = places;
    std::sort(legacy.begin(), legacy.end(), [](const Place& a, const Place& b) { return a.legacy < b.legacy; });
    for (std::size_t k = 0; k < legacy.size(); ++k) {
      bool apart = true;
      for (const Place& other : legacy) {
        apart = apart && (other.face == legacy[k].face ||
                          std::llabs(other.legacy[0] - legacy[k].legacy[0]) > kLegacyApart);
      }
      if (apart) {
        names[static_cast<std::size_t>(legacy[k].face)].push_back(thread.name + '#' + std::to_string(k));
      }
    }
    order_places(places, 0, places.size(), 0);
    const std::size_t count = places.size();
    for (std::size_t k = 0; k < count; ++k) {
      NameList& own = names[static_cast<std::size_t>(places[k].face)];
      // The name from the nearer end of the face (mitcad#1231).
      if (!thread.low.empty()) {
        const std::size_t from_low = thread.reversed ? count - 1 - k : k;
        const std::size_t from_high = count - 1 - from_low;
        own.push_back(from_low <= from_high ? thread.low + '#' + std::to_string(from_low)
                                            : thread.high + '#' + std::to_string(from_high));
      }
      own.push_back(thread.name + '#' + std::to_string(count + k));
    }
  }
  detail::FaceNamer namer(body.occt());
  for (int i = 0; i < body.face_count(); ++i) {
    for (const std::string& name : names[static_cast<std::size_t>(i)]) {
      namer.add(body.face(i), name);
    }
  }
  return namer.shape();
}

// The parameters of the point of surface `s` nearest to `p`, from `seed`
// near them: Newton's steps on the distance (the normal equations of the
// tangent plane's least squares), a few as the seed is close.
gp_Pnt2d nearest_on(const occ::handle<Geom_Surface>& s, const gp_Pnt2d& seed, const gp_Pnt& p) {
  double u = seed.X();
  double v = seed.Y();
  for (int k = 0; k < 8; ++k) {
    gp_Pnt q;
    gp_Vec su, sv;
    s->D1(u, v, q, su, sv);
    const gp_Vec r(q, p);
    const double a = su.Dot(su), b = su.Dot(sv), c = sv.Dot(sv);
    const double det = a * c - b * b;
    if (det <= gp::Resolution()) {
      break;
    }
    const double du = (c * r.Dot(su) - b * r.Dot(sv)) / det;
    const double dv = (a * r.Dot(sv) - b * r.Dot(su)) / det;
    u += du;
    v += dv;
    if (std::abs(du) + std::abs(dv) < 1.0e-14) {
      break;
    }
  }
  return gp_Pnt2d(u, v);
}

// The threads' faces that are pieces of the grooves' faces (each the sweep
// of a section's edge along a turn, an approximation of that edge's screw
// surface) put on the screw surface of their edge (`screws`, each over all
// the turns; each piece on its own copy of it), with the same edges: the
// pieces of one face then lie on one surface, and a rounding along them
// joins them (detail::join_surface_pieces). A piece's parameters map
// affinely onto the screw surface's (the section's edge runs as the edge
// itself does, the sweep along its turn as the screw motion's angle, or for
// a cylinder its angle and height), so its edges' p-curves are mapped so.
// The map is found by projecting three points of each piece onto the screw
// surface (from where the map of the piece before takes them) and checked
// at nine points of the piece. A piece that lies on none of them, or
// whose map does not hold, stays as it is.
ShapePtr on_screw_surfaces(const Shape& body, const std::vector<std::string>& threads,
                           const std::vector<occ::handle<Geom_Surface>>& screws) {
  // (How far the pieces may lie from the screw surfaces: the sweeps'
  // approximation is finer.)
  constexpr double kOnScrew = 1.0e-4;
  struct Screw {
    occ::handle<Geom_Surface> surface;
    occ::handle<ShapeAnalysis_Surface> analysis;
    std::optional<gp_GTrsf2d> last;
  };
  std::vector<Screw> known;
  for (const occ::handle<Geom_Surface>& s : screws) {
    known.push_back({s, nullptr, std::nullopt});
  }
  occ::handle<BRepTools_ReShape> reshape = new BRepTools_ReShape();
  bool any = false;
  for (int i = 0; i < body.face_count(); ++i) {
    if (!thread_named(body.face_names(i), threads)) {
      continue;
    }
    const TopoDS_Face face = TopoDS::Face(body.face(i).Oriented(TopAbs_FORWARD));
    const BRepAdaptor_Surface surface(face, false);
    if (surface.GetType() != GeomAbs_BSplineSurface) {
      continue;
    }
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    if (u1 - u0 < Precision::PConfusion() || v1 - v0 < Precision::PConfusion()) {
      continue;
    }
    // Whether a map takes the piece onto a surface.
    const auto holds = [&](const occ::handle<Geom_Surface>& s, const gp_GTrsf2d& map) {
      for (const double fu : {0.0, 0.5, 1.0}) {
        for (const double fv : {0.0, 0.5, 1.0}) {
          gp_XY uv(u0 + fu * (u1 - u0), v0 + fv * (v1 - v0));
          const gp_Pnt p = surface.Value(uv.X(), uv.Y());
          map.Transforms(uv);
          if (s->Value(uv.X(), uv.Y()).Distance(p) > kOnScrew) {
            return false;
          }
        }
      }
      return true;
    };
    // The piece's map onto a screw surface: its first point projected (from
    // where the surface's last map takes it, so that the projection only
    // refines it), the derivatives from steps of an eighth of the piece (a
    // periodic parameter, a cylinder's angle, taken within its period), and
    // checked.
    const Screw* screw = nullptr;
    gp_GTrsf2d map;
    for (Screw& s : known) {
      if (s.analysis.IsNull()) {
        s.analysis = new ShapeAnalysis_Surface(s.surface);
      }
      const gp_Pnt first = surface.Value(u0, v0);
      gp_Pnt2d p00;
      if (s.last) {
        gp_XY seed(u0, v0);
        s.last->Transforms(seed);
        p00 = nearest_on(s.surface, gp_Pnt2d(seed), first);
        if (s.surface->Value(p00.X(), p00.Y()).Distance(first) > kOnScrew) {
          continue;
        }
        // (The pieces of one face map alike but for a drift of micrometres:
        // the last map moved onto this piece's first point.)
        gp_GTrsf2d moved = *s.last;
        gp_XY at(u0, v0);
        moved.Transforms(at);
        moved.SetTranslationPart(moved.TranslationPart() + (p00.XY() - at));
        if (holds(s.surface, moved)) {
          s.last = moved;
          screw = &s;
          map = moved;
          break;
        }
      } else {
        // (The first piece on it: projected over the whole surface, whose
        // turns a local projection may confuse.)
        GeomAPI_ProjectPointOnSurf projection(first, s.surface);
        if (projection.NbPoints() == 0 || projection.LowerDistance() > kOnScrew) {
          continue;
        }
        double pu = 0, pv = 0;
        projection.LowerDistanceParameters(pu, pv);
        p00.SetCoord(pu, pv);
      }
      const double hu = (u1 - u0) / 8.0;
      const double hv = (v1 - v0) / 8.0;
      const auto step = [&](double u, double v) {
        gp_XY d = s.analysis->NextValueOfUV(p00, surface.Value(u, v), Precision::Confusion()).XY() - p00.XY();
        if (s.surface->IsUPeriodic()) {
          d.SetX(d.X() - s.surface->UPeriod() * std::round(d.X() / s.surface->UPeriod()));
        }
        if (s.surface->IsVPeriodic()) {
          d.SetY(d.Y() - s.surface->VPeriod() * std::round(d.Y() / s.surface->VPeriod()));
        }
        return d;
      };
      const gp_XY du = step(u0 + hu, v0) / hu;
      const gp_XY dv = step(u0, v0 + hv) / hv;
      gp_GTrsf2d m;
      m.SetValue(1, 1, du.X());
      m.SetValue(1, 2, dv.X());
      m.SetValue(2, 1, du.Y());
      m.SetValue(2, 2, dv.Y());
      m.SetTranslationPart(p00.XY() - du * u0 - dv * v0);
      if (holds(s.surface, m)) {
        s.last = m;
        screw = &s;
        map = m;
        break;
      }
    }
    if (screw == nullptr) {
      continue;
    }
    // Where the map turns the parameters' orientation round (the screw
    // surface's normal runs the other way), the loops run the other way
    // round and the face is reversed.
    const bool flipped = map.Value(1, 1) * map.Value(2, 2) - map.Value(1, 2) * map.Value(2, 1) < 0.0;
    BRep_Builder builder;
    TopoDS_Face on_screw;
    // (Each piece on its own piece of the surface: the turn it lies in and a
    // little more, as the sweeps a turn at a time were. Booleans would join
    // faces of one surface into faces of many turns, and on a surface of
    // many turns they take far longer. A rounding along the pieces joins
    // their surfaces again: detail::join_surface_pieces.)
    occ::handle<Geom_Surface> own = occ::down_cast<Geom_Surface>(screw->surface->Copy());
    if (const auto spline = occ::down_cast<Geom_BSplineSurface>(own)) {
      double v_low = RealLast(), v_high = -RealLast();
      for (const double fu : {0.0, 1.0}) {
        for (const double fv : {0.0, 1.0}) {
          gp_XY uv(u0 + fu * (u1 - u0), v0 + fv * (v1 - v0));
          map.Transforms(uv);
          v_low = std::min(v_low, uv.Y());
          v_high = std::max(v_high, uv.Y());
        }
      }
      double a0 = 0, a1 = 0, b0 = 0, b1 = 0;
      spline->Bounds(a0, a1, b0, b1);
      const double margin = 0.25 * (v_high - v_low);
      const double from = std::max(b0, v_low - margin);
      const double to = std::min(b1, v_high + margin);
      if (from < to) {
        spline->Segment(a0, a1, from, to);
      }
    }
    builder.MakeFace(on_screw, own, BRep_Tool::Tolerance(face));
    for (TopoDS_Iterator w(face); w.More(); w.Next()) {
      builder.Add(on_screw, flipped ? w.Value().Reversed() : w.Value());
    }
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      if (BRep_Tool::Degenerated(edge) || BRep_Tool::IsClosed(edge, face)) {
        return std::make_shared<Shape>(body);
      }
      double f = 0, l = 0;
      const occ::handle<Geom2d_Curve> old = BRep_Tool::CurveOnSurface(edge, face, f, l);
      if (old.IsNull()) {
        return std::make_shared<Shape>(body);
      }
      // The p-curve on the screw surface through the projections of the
      // edge's points at its parameters, each from where the map takes the
      // old p-curve's point there (the map is affine only to a few
      // micrometres: the pieces were approximations).
      double cf = 0, cl = 0;
      const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, cf, cl);
      if (curve.IsNull()) {
        return std::make_shared<Shape>(body);
      }
      const occ::handle<Geom2d_BSplineCurve> spline =
          Geom2dConvert::CurveToBSplineCurve(new Geom2d_TrimmedCurve(old, f, l));
      const int n = std::clamp(8 * spline->NbPoles() + 1, 65, 1601);
      occ::handle<NCollection_HArray1<gp_Pnt2d>> points = new NCollection_HArray1<gp_Pnt2d>(1, n);
      occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n);
      for (int k = 0; k < n; ++k) {
        const double t = f + (l - f) * k / (n - 1);
        gp_XY seed = old->Value(t).XY();
        map.Transforms(seed);
        gp_Pnt2d q = nearest_on(screw->surface, gp_Pnt2d(seed), curve->Value(t));
        if (screw->surface->IsUPeriodic()) {
          const double period = screw->surface->UPeriod();
          q.SetX(q.X() + period * std::round((seed.X() - q.X()) / period));
        }
        if (screw->surface->IsVPeriodic()) {
          const double period = screw->surface->VPeriod();
          q.SetY(q.Y() + period * std::round((seed.Y() - q.Y()) / period));
        }
        points->SetValue(k + 1, q);
        params->SetValue(k + 1, t);
      }
      Geom2dAPI_Interpolate interpolation(points, params, false, Precision::PConfusion());
      interpolation.Perform();
      if (!interpolation.IsDone()) {
        return std::make_shared<Shape>(body);
      }
      const occ::handle<Geom2d_BSplineCurve> mapped = interpolation.Curve();
      builder.UpdateEdge(edge, mapped, on_screw, BRep_Tool::Tolerance(edge));
      builder.Range(edge, on_screw, f, l);
      // (The edge's tolerance takes in how far the piece lay from the screw
      // surface.)
      builder.SameParameter(edge, false);
      BRepLib::SameParameter(edge, BRep_Tool::Tolerance(edge));
    }
    reshape->Replace(face, flipped ? on_screw.Reversed() : on_screw);
    any = true;
  }
  if (!any) {
    return std::make_shared<Shape>(body);
  }
  const TopoDS_Shape result = reshape->Apply(body.occt());
  detail::FaceNamer namer(result);
  namer.carry(*reshape->History(), body);
  return namer.shape();
}


// The options of the innermost ThreadOptionsScope on this thread.
thread_local const detail::ThreadOptions* t_thread_options = nullptr;
// thread_counts().
std::atomic<std::size_t> g_planned_faces{0};
std::atomic<std::size_t> g_ordered_faces{0};
std::atomic<std::size_t> g_merged_booleans{0};
std::atomic<std::size_t> g_merged_tools{0};

// Reads the k-th face of the spec from the body: its cylinder, the checks
// of the spec against it, its part along the thread's axis and its end
// chamfers; a taper pipe thread's free ends and slope.
FacePlan plan_face(const Shape& body, const ThreadSpec& spec, std::size_t k, FaceTrace* trace) {
  const std::string& face = spec.faces[k];
  const CylinderFace cylinder = face_cylinder(body, face);
  if (spec.groove) {
    check_clearance(spec, cylinder.internal);
    if (!cylinder.internal && cylinder.radius <= spec.minor / 2.0 - spec.radial_clearance + kSame) {
      throw std::invalid_argument("face " + face + " is too thin for the thread");
    }
    if (cylinder.internal && cylinder.radius >= spec.major / 2.0 + spec.radial_clearance - kSame) {
      throw std::invalid_argument("face " + face + " is too wide for the thread");
    }
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
  Thread thread = thread_of(cylinder, spec);
  if (!spec.angles.empty()) {
    if (spec.angles.size() != spec.faces.size()) {
      throw std::invalid_argument("the thread's angles must be given for each face");
    }
    thread.angle = spec.angles[k];
  }
  EndChamfers chamfers;
  if (spec.chamfer) {
    chamfers.low = spec.chamfer_ends[k].low;
    chamfers.high = spec.chamfer_ends[k].high;
    chamfers.radius = spec.chamfer_diameter / 2.0;
    chamfers.tan = std::tan(spec.chamfer_angle);
    chamfers.low_name = face_name(spec.feature, "thread_chamfer_low", face);
    chamfers.high_name = face_name(spec.feature, "thread_chamfer_high", face);
  }
  const bool reversed = !thread.along.IsEqual(cylinder.axis.Direction(), 1.0e-9);
  if (reversed) {
    // Measured along the thread's axis.
    std::swap(from, to);
    from = cylinder.length - from;
    to = cylinder.length - to;
    std::swap(chamfers.low, chamfers.high);
    std::swap(chamfers.low_name, chamfers.high_name);
  }
  // A taper pipe thread (mitcad#733): where it starts, and which ends
  // of its part are free (nothing of the body beyond them).
  bool free_low = false;
  bool free_high = false;
  if (spec.taper > 0.0) {
    const double margin = 0.1 * thread.pitch;
    const double inner = std::max(thread.face_radius - margin, 0.5 * thread.face_radius);
    const double outer = thread.face_radius + margin;
    {
      const Timed timed(checks_time(trace));
      free_low = from <= kSame && !material_past(body.occt(), thread, from, -1.0, inner, outer);
      free_high = to >= thread.length - kSame && !material_past(body.occt(), thread, to, 1.0, inner, outer);
    }
    bool start_high = false;
    if (!spec.full_length) {
      start_high = spec.high_end != reversed;
    } else if (free_low != free_high) {
      start_high = free_high;
    } else {
      start_high = spec.taper_high_end != reversed;
    }
    // From the start into the thread the diameters shrink inside a
    // hole and grow along a shaft.
    const double into = start_high ? -1.0 : 1.0;
    thread.slope = (thread.internal ? -1.0 : 1.0) * into * spec.taper / 2.0;
    const double start = start_high ? to : from;
    thread.pitch_radius -= thread.slope * start;
    thread.major_radius -= thread.slope * start;
    thread.minor_radius -= thread.slope * start;
  }
  FacePlan plan;
  plan.face = face;
  plan.thread = thread;
  plan.reversed = reversed;
  plan.from = from;
  plan.to = to;
  plan.chamfers = chamfers;
  plan.free_low = free_low;
  plan.free_high = free_high;
  if (spec.groove && thread.slope == 0.0) {
    const FaceEnds ends = face_ends(body, face, thread);
    plan.even_low = ends.low;
    plan.even_high = ends.high;
  }
  if (spec.groove) {
    plan.name = face_name(spec.feature, "thread", face);
  }
  return plan;
}

// The faces one by one, each read from the body the one before left (all
// faces before mitcad#843; the faces whose reaches meet since). `plans`
// gets the faces as they were read, `screws` each face's screw surfaces.
ShapePtr thread_in_order(const Shape& body, const ThreadSpec& spec, std::vector<FacePlan>& plans,
                         std::vector<Screws>& screws) {
  ShapePtr current = std::make_shared<Shape>(body);
  plans.clear();
  screws.assign(spec.faces.size(), Screws());
  for (std::size_t k = 0; k < spec.faces.size(); ++k) {
    FaceTrace trace;
    plans.push_back(plan_face(*current, spec, k, &trace));
    current = thread_one_face(*current, plans.back(), spec, screws[k], &trace);
    trace_face(spec.feature, spec.faces[k], "in-order", trace);
  }
  return current;
}

// A box round all that a face's thread can change and the points its
// checks past the ends of its part classify: about its axis out to its
// largest radius (the face's, the thread's, the chamfers' with their run
// past the end, a taper's growth) and a margin, and along it two pitches
// past both ends of its part (the tools run out by a pitch at most).
Bnd_Box reach_of(const FacePlan& p) {
  const Thread& t = p.thread;
  const double low = p.from - 2.0 * t.pitch;
  const double high = p.to + 2.0 * t.pitch;
  double radius = std::max({t.face_radius, t.major_radius, t.minor_radius});
  if (p.chamfers.low || p.chamfers.high) {
    radius = std::max(radius, p.chamfers.radius + 0.1 * t.pitch * p.chamfers.tan);
  }
  radius += std::abs(t.slope) * std::max(std::abs(low), std::abs(high)) + 0.2 * t.pitch;
  const gp_Pnt a = t.origin.Translated(gp_Vec(t.along) * low);
  const gp_Pnt b = t.origin.Translated(gp_Vec(t.along) * high);
  // How far a circle of the radius about the axis reaches across a
  // coordinate axis.
  const auto across = [&](double d) { return radius * std::sqrt(std::max(0.0, 1.0 - d * d)); };
  Bnd_Box box;
  box.Update(std::min(a.X(), b.X()) - across(t.along.X()), std::min(a.Y(), b.Y()) - across(t.along.Y()),
             std::min(a.Z(), b.Z()) - across(t.along.Z()), std::max(a.X(), b.X()) + across(t.along.X()),
             std::max(a.Y(), b.Y()) + across(t.along.Y()), std::max(a.Z(), b.Z()) + across(t.along.Z()));
  box.Enlarge(1.0e-3 * std::max(1.0, radius));
  return box;
}

// Whether an error stops the thread whichever way its faces are made: a
// cancel request, or memory running out (detail::run's "<operation>: out
// of memory" too).
bool fatal(const std::exception_ptr& error) {
  try {
    std::rethrow_exception(error);
  } catch (const Cancelled&) {
    return true;
  } catch (const Standard_OutOfMemory&) {
    return true;
  } catch (const std::bad_alloc&) {
    return true;
  } catch (const std::exception& other) {
    const std::string text = other.what();
    const std::string out = "out of memory";
    return text.size() >= out.size() && text.compare(text.size() - out.size(), out.size(), out) == 0;
  } catch (...) {
    return false;
  }
}

// The thread planned from the body as given (thread_options.hpp): every
// face read from it, the tools of the faces whose reach meets no other
// face's built at once (on all cores with `options.parallel`), then
// applied stage by stage (a stage's tools of all those faces in one local
// boolean where that can be done, with `options.merged`), then the other
// faces one by one as thread_in_order threads them. `plans` gets the faces
// as they were read, `screws` each face's screw surfaces. Nothing where a
// face cannot be planned so (an error reading it or building its tools,
// before the body changed): thread_in_order then makes the thread, and its
// errors, as before. A cancel request and memory running out stop it.
std::optional<ShapePtr> thread_planned(const Shape& body, const ThreadSpec& spec,
                                       const detail::ThreadOptions& options, std::vector<FacePlan>& plans,
                                       std::vector<Screws>& screws, ThreadTrace& trace) {
  const std::size_t count = spec.faces.size();
  std::vector<FaceTrace> traces(count);
  plans.clear();
  try {
    for (std::size_t k = 0; k < count; ++k) {
      plans.push_back(plan_face(body, spec, k, &traces[k]));
    }
  } catch (...) {
    if (fatal(std::current_exception())) {
      throw;
    }
    trace.note("planned=no");
    return std::nullopt;
  }
  // The faces whose reach meets no other face's: their tools go ahead.
  std::vector<Bnd_Box> reaches;
  for (const FacePlan& p : plans) {
    reaches.push_back(reach_of(p));
  }
  std::vector<bool> ahead(count, true);
  std::size_t ahead_count = 0;
  for (std::size_t k = 0; k < count; ++k) {
    for (std::size_t j = 0; j < count && ahead[k]; ++j) {
      ahead[k] = j == k || reaches[k].IsOut(reaches[j]);
    }
    ahead_count += ahead[k] ? 1 : 0;
  }
  trace.note("ahead=" + std::to_string(ahead_count));
  trace.mark("plan");

  // Their steps, with the checks past their ends on the body as given:
  // the faces' tools do not reach each other's checks.
  std::vector<std::vector<Step>> steps(count);
  try {
    for (std::size_t k = 0; k < count; ++k) {
      if (!ahead[k]) {
        continue;
      }
      {
        const Timed timed(tools_time(&traces[k]));
        steps[k] = band_steps(body, plans[k], spec);
      }
      for (Step& step : other_steps(body, plans[k], spec, &traces[k])) {
        steps[k].push_back(std::move(step));
      }
    }
  } catch (...) {
    if (fatal(std::current_exception())) {
      throw;
    }
    trace.note("planned=no");
    return std::nullopt;
  }
  trace.mark("prepare");

  // Their tools built, each from shapes of its own (the face pieces the
  // bands thicken are copies): the grooves, the longest, first. Each job
  // runs as an operation of its own (detail::run: OCCT's exceptions and
  // crashes are its errors) under the calling thread's cancel source.
  struct Job {
    std::size_t face;
    std::size_t step;
  };
  std::vector<Job> jobs;
  for (const bool longest : {true, false}) {
    for (std::size_t k = 0; k < count; ++k) {
      for (std::size_t i = 0; i < steps[k].size(); ++i) {
        if ((steps[k][i].stage == Stage::Grooves) == longest) {
          jobs.push_back({k, i});
        }
      }
    }
  }
  std::vector<std::vector<TopoDS_Shape>> built(count);
  for (std::size_t k = 0; k < count; ++k) {
    built[k].resize(steps[k].size());
  }
  screws.assign(count, Screws());
  std::vector<std::exception_ptr> errors(jobs.size());
  std::vector<double> seconds(jobs.size(), 0.0);
  const CancelSource* source = cancel_source();
  OSD_Parallel::For(
      0, static_cast<int>(jobs.size()),
      [&](int i) {
        const auto j = static_cast<std::size_t>(i);
        const Job& job = jobs[j];
        const Clock::time_point start = Clock::now();
        const auto work = [&] { built[job.face][job.step] = steps[job.face][job.step].build(screws[job.face]); };
        try {
          if (source != nullptr) {
            const CancelScope scope(*source);
            detail::run("thread tools", work);
          } else {
            detail::run("thread tools", work);
          }
        } catch (...) {
          errors[j] = std::current_exception();
        }
        seconds[j] = seconds_since(start);
      },
      !options.parallel || jobs.size() < 2);
  for (const std::exception_ptr& error : errors) {
    if (error && fatal(error)) {
      std::rethrow_exception(error);
    }
  }
  for (const std::exception_ptr& error : errors) {
    if (error) {
      trace.note("planned=no");
      return std::nullopt;
    }
  }
  for (std::size_t j = 0; j < jobs.size(); ++j) {
    traces[jobs[j].face].tools += seconds[j];
  }
  trace.note("jobs=" + std::to_string(jobs.size()) + (options.parallel ? " parallel" : " serial"));
  trace.mark("tools");

  // Stage by stage: the faces' tools do not reach each other's, so the
  // order of the faces within a stage changes nothing.
  ShapePtr current = std::make_shared<Shape>(body);
  for (const Stage stage : kStages) {
    std::vector<detail::NamedTool> tools;
    for (std::size_t k = 0; k < count; ++k) {
      for (std::size_t i = 0; i < steps[k].size(); ++i) {
        if (steps[k][i].stage == stage && !built[k][i].IsNull()) {
          tools.push_back({built[k][i], steps[k][i].name});
        }
      }
    }
    if (tools.empty()) {
      continue;
    }
    bool merged = false;
    if (options.merged && tools.size() > 1) {
      current = detail::local_boolean_all(*current, tools, operation_of(stage), &merged);
    } else {
      for (const detail::NamedTool& tool : tools) {
        current = detail::local_boolean(*current, tool.shape, operation_of(stage), tool.name).shape;
      }
    }
    if (merged) {
      ++g_merged_booleans;
      g_merged_tools += tools.size();
    }
    trace.note(std::string(stage_name(stage)) + "-tools=" + std::to_string(tools.size()) + (merged ? " merged" : ""));
    trace.mark(stage_name(stage));
  }
  for (std::size_t k = 0; k < count; ++k) {
    if (ahead[k]) {
      trace_face(spec.feature, spec.faces[k], "ahead", traces[k]);
    }
  }
  g_planned_faces += ahead_count;

  // The other faces one by one.
  for (std::size_t k = 0; k < count; ++k) {
    if (ahead[k]) {
      continue;
    }
    plans[k] = plan_face(*current, spec, k, &traces[k]);
    current = thread_one_face(*current, plans[k], spec, screws[k], &traces[k]);
    trace_face(spec.feature, spec.faces[k], "in-order", traces[k]);
    ++g_ordered_faces;
  }
  if (ahead_count < count) {
    trace.mark("in-order");
  }
  return current;
}
} // namespace

namespace detail {

ThreadOptions thread_options() {
  if (t_thread_options != nullptr) {
    return *t_thread_options;
  }
  ThreadOptions options;
  const std::optional<std::string> sequential = environment("MITCAD_THREAD_SEQUENTIAL");
  options.planned = !sequential || sequential->empty() || *sequential == "0";
  options.parallel = environment("MITCAD_THREAD_PARALLEL").value_or("1") != "0";
  options.merged = environment("MITCAD_THREAD_MERGE").value_or("1") != "0";
  return options;
}

ThreadOptionsScope::ThreadOptionsScope(const ThreadOptions& options)
    : m_previous(t_thread_options), m_options(options) {
  t_thread_options = &m_options;
}

ThreadOptionsScope::~ThreadOptionsScope() { t_thread_options = m_previous; }

ThreadCounts thread_counts() {
  ThreadCounts counts;
  counts.planned_faces = g_planned_faces.load();
  counts.ordered_faces = g_ordered_faces.load();
  counts.merged_booleans = g_merged_booleans.load();
  counts.merged_tools = g_merged_tools.load();
  return counts;
}

} // namespace detail

ShapePtr modeled_thread(const Shape& body, const ThreadSpec& spec) {
  if (spec.faces.empty()) {
    throw std::invalid_argument("no faces to thread");
  }
  detail::require_positive("thread pitch", spec.pitch);
  if (spec.starts < 1 || spec.starts > kMaxStarts) {
    throw std::invalid_argument("the thread's number of starts must be from 1 to " + std::to_string(kMaxStarts));
  }
  detail::require_finite("thread flank angle", spec.flank_angle);
  if (!(spec.flank_angle > 0.0 && spec.flank_angle <= kTwoPi / 8.0 + 1.0e-12)) {
    throw std::invalid_argument("the thread's flank angle must be more than 0 and at most 45 degrees to the radius");
  }
  if (spec.groove) {
    detail::require_positive("thread major diameter", spec.major);
    detail::require_positive("thread minor diameter", spec.minor);
    detail::require_positive("thread pitch diameter", spec.pitch_diameter);
    detail::require_finite("thread angle", spec.angle);
    for (const double turned : spec.angles) {
      detail::require_finite("thread angle", turned);
    }
    if (!(spec.minor < spec.pitch_diameter && spec.pitch_diameter < spec.major)) {
      throw std::invalid_argument("the thread's diameters must grow from the minor to the pitch to the major one");
    }
    // The flanks meet P / (4 tan of the flank angle) on either side of the
    // pitch diameter (H/2 = 0.433 P for the 60-degree profile, 0.933 P for
    // the trapezoidal one): the crests and roots must lie within that.
    const double reach = (std::abs(spec.flank_angle - kFlank60) > 1.0e-12 ? 0.25 / std::tan(spec.flank_angle)
                                                                            : 0.4330127) *
                         spec.pitch;
    if (spec.major - spec.pitch_diameter >= 2.0 * reach || spec.pitch_diameter - spec.minor >= 2.0 * reach) {
      throw std::invalid_argument("the thread's diameters are too far apart for its pitch");
    }
  }
  detail::require_finite("thread taper", spec.taper);
  if (spec.taper < 0.0 || spec.taper >= 0.5) {
    throw std::invalid_argument("the thread's taper must be 0 or more and less than 1:2");
  }
  if (spec.taper > 0.0 && spec.chamfer) {
    throw std::invalid_argument("a taper pipe thread has no end chamfers");
  }
  if (spec.chamfer) {
    detail::require_finite("thread chamfer angle", spec.chamfer_angle);
    detail::require_positive("thread chamfer diameter", spec.chamfer_diameter);
    if (!(spec.chamfer_angle > 0.0 && spec.chamfer_angle < kTwoPi / 4.0)) {
      throw std::invalid_argument("the thread's chamfer angle must be between 0 and 90 degrees to the axis");
    }
    if (spec.chamfer_ends.size() != spec.faces.size()) {
      throw std::invalid_argument("the thread's chamfer ends must be given for each face");
    }
  }
  if (!spec.full_length) {
    detail::require_positive("thread length", spec.length);
    detail::require_finite("thread offset", spec.offset);
    if (spec.offset < 0.0) {
      throw std::invalid_argument("the thread offset must not be negative");
    }
  }
  detail::require_finite("thread radial clearance", spec.radial_clearance);
  detail::require_finite("thread flank clearance", spec.flank_clearance);
  if (spec.radial_clearance < 0.0) {
    throw std::invalid_argument("the thread's radial clearance must not be negative");
  }
  if (spec.flank_clearance < 0.0) {
    throw std::invalid_argument("the thread's flank clearance must not be negative");
  }
  return detail::run("thread", [&] {
    ThreadTrace trace(spec.feature + " faces=" + std::to_string(spec.faces.size()));
    const detail::ThreadOptions options = detail::thread_options();
    // The faces as they were read, and the screw surfaces of each one's
    // grooves.
    std::vector<FacePlan> plans;
    std::vector<Screws> face_screws;
    ShapePtr current;
    if (options.planned) {
      if (std::optional<ShapePtr> planned = thread_planned(body, spec, options, plans, face_screws, trace)) {
        current = *planned;
      }
    } else {
      trace.note("in-order");
    }
    if (!current) {
      current = thread_in_order(body, spec, plans, face_screws);
      trace.mark("faces");
    }
    std::vector<std::string> threads;
    // The threads' names and the threaded faces', numbered along their axes.
    std::vector<TurnNamed> named;
    Screws screws;
    if (spec.groove) {
      for (std::size_t k = 0; k < plans.size(); ++k) {
        const FacePlan& plan = plans[k];
        threads.push_back(plan.name);
        screws.insert(screws.end(), face_screws[k].begin(), face_screws[k].end());
        named.push_back({plan.name, plan.thread.origin, plan.thread.along,
                         face_name(spec.feature, "low_thread", plan.face),
                         face_name(spec.feature, "high_thread", plan.face), plan.reversed});
        named.push_back({plan.face, plan.thread.origin, plan.thread.along, {}, {}, false});
      }
    }
    // The threads' faces and the threaded faces' pieces (a crest the face
    // itself) numbered a turn each along the axis, then the grooves'
    // pieces put on their screw surfaces: a rounding along the thread joins
    // the pieces of each into one face over all the turns, as the bodies
    // stored in files keep threads (fillet, detail::join_surface_pieces).
    if (!threads.empty()) {
      current = per_turn_names(*current, named);
      trace.mark("names");
      current = on_screw_surfaces(*current, threads, screws);
      trace.mark("screws");
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
    trace.mark("check");
    return namer.shape();
  });
}

} // namespace mitcad::geometry
