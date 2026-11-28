// SPDX-License-Identifier: MIT
#include "mitcad/geometry/faceops.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <array>
#include <exception>
#include <map>
#include <new>
#include <optional>
#include <set>
#include <stdexcept>
#include <typeinfo>
#include <utility>

#include <BOPAlgo_Alerts.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_Analyse.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepOffset_Offset.hxx>
#include <ChFiDS_TypeOfConcavity.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Line.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Standard_Failure.hxx>
#include <TopLoc_Location.hxx>
#include <Precision.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeUpgrade_ShapeDivideContinuity.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Solid.hxx>
#include <gp.hxx>
#include <gp_Ax3.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Trsf.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec2d.hxx>

#include "face_join.hpp"
#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/split.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "replace.hpp"
#include "shell_cavity.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTolerance = 1.0e-6;
constexpr double kHalfPi = 1.57079632679489661923;

void require_not_negative(const char* what, double value) {
  if (!(value >= 0.0) || !std::isfinite(value)) {
    throw std::invalid_argument(std::string(what) + " must not be negative");
  }
}

// The faces of the result that an input face became: its modified images,
// or itself when it is kept unchanged.
template <class Operation>
std::vector<TopoDS_Shape> images_of(Operation& operation, const TopoDS_Shape& face,
                                    const TopoDS_Shape& result) {
  std::vector<TopoDS_Shape> images;
  if (operation.IsDeleted(face)) {
    return images;
  }
  for (const TopoDS_Shape& image : operation.Modified(face)) {
    if (image.ShapeType() == TopAbs_FACE) {
      images.push_back(image);
    }
  }
  if (images.empty()) {
    ShapeMap faces;
    TopExp::MapShapes(result, TopAbs_FACE, faces);
    if (faces.Contains(face)) {
      images.push_back(face);
    }
  }
  return images;
}

template <class Operation>
std::vector<TopoDS_Shape> generated_faces(Operation& operation, const TopoDS_Shape& source) {
  std::vector<TopoDS_Shape> faces;
  for (const TopoDS_Shape& image : operation.Generated(source)) {
    if (image.ShapeType() == TopAbs_FACE) {
      faces.push_back(image);
    }
  }
  return faces;
}

void add_names(detail::FaceNamer& namer, const TopoDS_Shape& face, const NameList& names) {
  for (const std::string& name : names) {
    namer.add(face, name);
  }
}

// True when the face lies on the surface of the reference face.
bool on_surface(const TopoDS_Face& face, const TopoDS_Face& reference) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const BRepAdaptor_Surface surface(face, false);
  const occ::handle<Geom_Surface> target = BRep_Tool::Surface(reference);
  for (const double s : {0.5, 0.3}) {
    const gp_Pnt p = surface.Value(u1 + (u2 - u1) * s, v1 + (v2 - v1) * (1 - s));
    GeomAPI_ProjectPointOnSurf projection(p, target);
    if (projection.NbPoints() == 0 || projection.LowerDistance() > 1.0e-6) {
      return false;
    }
  }
  return true;
}

// Runs an OCCT step, naming it in the error OCCT may raise without a message.
template <class Step>
auto step(const std::string& what, Step&& run) -> decltype(run()) {
  try {
    return run();
  } catch (const Standard_Failure& failure) {
    const std::string message = failure.what();
    throw std::runtime_error(what + (message.empty() ? "" : ": " + message));
  }
}

// Why the geometry kernel's offset of a shell failed, for the error.
std::string offset_failure(const BRepOffset_MakeOffset& offset) {
  switch (offset.Error()) {
  case BRepOffset_BadNormalsOnGeometry:
    return "shell failed; a face's normals cannot be computed everywhere";
  case BRepOffset_C0Geometry:
    return "shell failed; a face's surface has a crease (it is only C0 continuous)";
  case BRepOffset_NotConnectedShell:
    return "shell failed; the body's faces are not connected";
  case BRepOffset_CannotTrimEdges:
    return "shell failed; the offset faces' edges cannot be trimmed";
  case BRepOffset_CannotFuseVertices:
    return "shell failed; the offset faces' vertices cannot be joined";
  case BRepOffset_CannotExtentEdge:
    return "shell failed; the offset faces' edges cannot be extended";
  case BRepOffset_MixedConnectivity:
    return "shell failed; faces meet partly smoothly and partly at an angle along an edge";
  case BRepOffset_UnknownError:
    // The offset faces leave gaps or do not close up the wall: mostly a
    // face that vanishes at this thickness (a strip narrower, or a convex
    // rounding tighter, than the wall), which the kernel cannot drop.
    return "shell failed; the offset faces do not close up (a face narrower or more tightly "
           "curved than the wall may vanish at this thickness)";
  default:
    return "shell failed; the thickness may be too large for the body";
  }
}

// What named_failure says of an offset that raised an error without a
// message. A shell whose cavity cannot be built either says why in words
// instead (shell_refusal); this stays in its detail.
constexpr const char* kOffsetRaised = "the geometry kernel's offset raised an error";

// Runs the kernel's offset of a shell, giving a failure it raises without
// a message one (mitcad#158: an offset of stored B-spline roundings that
// fails to extend an edge). Other failures, out of memory among them, go
// on as they are. A test asking the kernel to refuse the shell
// (MITCAD_TEST_OCCT_REFUSES=shell, mitcad#2140) gets such a failure.
template <class Offset>
void named_failure(Offset&& offset) {
  try {
    if (detail::refusal_asked("shell")) {
      throw Standard_Failure("");
    }
    offset();
  } catch (const Standard_Failure& failure) {
    if (std::string(failure.what()).empty()) {
      throw Standard_Failure(kOffsetRaised);
    }
    throw;
  }
}

// The sine of the angle between the normals of the edge's faces `a` and `b`
// at the edge's parameter t; -1 where a normal is not defined there.
double normals_sine(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b, double t) {
  gp_Vec normals[2];
  const TopoDS_Face* faces[2] = {&a, &b};
  for (int k = 0; k < 2; ++k) {
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, *faces[k], first, last);
    if (pcurve.IsNull()) {
      return -1.0;
    }
    const gp_Pnt2d uv = pcurve->Value(t);
    const BRepAdaptor_Surface surface(*faces[k], false);
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(uv.X(), uv.Y(), p, du, dv);
    normals[k] = du.Crossed(dv);
    if (normals[k].Magnitude() <= 1.0e-12 * std::max(1.0, du.Magnitude() * dv.Magnitude())) {
      return -1.0;
    }
  }
  return normals[0].Crossed(normals[1]).Magnitude() / (normals[0].Magnitude() * normals[1].Magnitude());
}

// The named edges between two kept faces (not `removed`) that meet at an
// angle along the edge but tangentially at an end of it: a crease that
// fades out to a smooth join, such as the crotch of a Y tube where its arms
// leave their common straight part (mitcad#2140). The geometry kernel's
// offset finds the faces' offsets meeting there along several branches.
std::vector<int> fading_creases(const Shape& body, const std::set<int>& removed) {
  // (The sine of 1e-4 rad, as the port's offset patches take faces for
  // tangent at a crease's end; a crease at least ten times that in its
  // middle.)
  constexpr double kTangent = 1.0e-4;
  constexpr double kCrease = 1.0e-3;
  std::vector<int> creases;
  for (int e = 0; e < body.edge_count(); ++e) {
    const std::array<int, 2> faces = body.edge_faces(e);
    if (body.edge_name(e).empty() || faces[0] < 0 || faces[1] < 0 || faces[0] == faces[1] ||
        removed.count(faces[0]) > 0 || removed.count(faces[1]) > 0) {
      continue;
    }
    const TopoDS_Edge& edge = body.edge(e);
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    BRep_Tool::Range(edge, first, last);
    const TopoDS_Face& a = body.face(faces[0]);
    const TopoDS_Face& b = body.face(faces[1]);
    const double middle = normals_sine(edge, a, b, 0.5 * (first + last));
    const double start = normals_sine(edge, a, b, first);
    const double end = normals_sine(edge, a, b, last);
    if (middle > kCrease && ((start >= 0.0 && start <= kTangent) || (end >= 0.0 && end <= kTangent))) {
      creases.push_back(e);
    }
  }
  return creases;
}

// Why a shell fails whose offset the geometry kernel refused without a
// message and whose cavity could not be built either (mitcad#2140), in
// words: along the creases that fade out to a smooth join (their places
// painted), or else at the step that failed. The kernel's own text stays in
// the detail.
std::string shell_refusal(const Shape& body, const std::set<int>& removed) {
  const std::vector<int> creases = fading_creases(body, removed);
  const std::string kernel = std::string("; ") + kOffsetRaised + " without a message, and the cavity could not be built";
  if (!creases.empty()) {
    std::vector<ErrorPlace> places;
    std::string names;
    for (const int e : creases) {
      places.push_back(edge_place(body.edge_name(e)));
      names += (names.empty() ? "" : ", ") + body.edge_name(e);
    }
    const std::string said =
        creases.size() == 1
            ? "shell failed; the geometry kernel cannot offset the faces along an edge where their angle fades out "
              "to a smooth join at an end of it"
            : "shell failed; the geometry kernel cannot offset the faces along edges where their angle fades out to "
              "a smooth join at an end";
    return placed_error(said, said + " (" + names + ")" + kernel, places);
  }
  const std::string said =
      "shell failed; the geometry kernel cannot join the offsets of the faces into the inside of the wall at this "
      "thickness";
  return placed_error(said, said + kernel);
}

// Whether an error is the kernel's offset failure without a message.
bool said_nothing(const std::exception_ptr& failure) {
  try {
    std::rethrow_exception(failure);
  } catch (const Standard_Failure& error) {
    return std::string(error.what()) == kOffsetRaised;
  } catch (const std::exception& error) {
    return std::string(error.what()) == kOffsetRaised;
  } catch (...) {
  }
  return false;
}

// One solid of an offset or thick solid result; throws if there is none.
TopoDS_Shape single_solid(const TopoDS_Shape& shape, const char* operation) {
  TopExp_Explorer it(shape, TopAbs_SOLID);
  if (!it.More()) {
    throw std::runtime_error(std::string(operation) + " produced no solid");
  }
  const TopoDS_Shape solid = it.Current();
  it.Next();
  if (it.More()) {
    throw std::runtime_error(std::string(operation) + " produced several solids");
  }
  return solid;
}

// What OCCT's offset is given of a body: OCCT makes a solid of the offset's
// closed shell only for a solid, so a compound holding one solid (as
// dress-ups and booleans give their bodies) is offset as that solid, else
// the result has no solid (mitcad#326: moving a bolt's face to a thread's
// diameter after its chamfer failed so). Other shapes as they are.
TopoDS_Shape offset_input(const TopoDS_Shape& shape) {
  if (shape.ShapeType() != TopAbs_COMPOUND) {
    return shape;
  }
  TopoDS_Iterator it(shape);
  if (!it.More() || it.Value().ShapeType() != TopAbs_SOLID) {
    return shape;
  }
  const TopoDS_Shape only = it.Value();
  it.Next();
  return it.More() ? shape : only;
}

// Offsets of their own on faces of a body (a shell's faces with their own
// thickness, mitcad#2443), by the face's index, signed as the offset they
// replace.
using FaceOffsets = std::map<int, double>;

// The faces of the body other than `face` that the geometry kernel's
// offset gives the offset of `face` (BRepOffset_MakeOffset::
// UpdateFaceOffset): those that meet it smoothly, and so on from them;
// found by the kernel's own analysis of the faces it offsets.
std::vector<int> offset_together(const Shape& body, const BRepOffset_Analyse& analyse, int face) {
  BRep_Builder builder;
  TopoDS_Compound group;
  builder.MakeCompound(group);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
  seen.Add(body.face(face));
  analyse.AddFaces(body.face(face), group, seen, ChFiDS_Tangential);
  std::vector<int> faces;
  for (TopExp_Explorer it(group, TopAbs_FACE); it.More(); it.Next()) {
    const int f = body.face_index(it.Current());
    if (f >= 0 && f != face && std::find(faces.begin(), faces.end(), f) == faces.end()) {
      faces.push_back(f);
    }
  }
  return faces;
}

// A face the rounded offset gives the offset of another (rounded_together),
// and how: across a rounding along an edge (`corner` false) or round a
// corner of roundings.
struct RoundedTogether {
  int face = -1;
  bool corner = false;
};

// The faces of the body other than `face` that the geometry kernel's offset
// with rounded joins gives the offset of `face` (BRepOffset_MakeOffset::
// UpdateFaceOffset with OCCT port patch 2488, mitcad#2488): those that meet
// it smoothly, those a rounding joins to it (along convex edges for an offset
// `offset` out of the material, concave ones into it) where the rounding
// cannot run from one thickness to another (BRepOffset_Offset::JoinsOffsets:
// other than along a straight edge where neither wall turns, or along a
// circle about whose axis both walls turn), those round a corner of roundings
// that cannot (other than three walls square to each other), and so on from
// them.
std::vector<RoundedTogether> rounded_together(const Shape& body, const BRepOffset_Analyse& analyse, int face,
                                              double offset) {
  const ChFiDS_TypeOfConcavity rounding = offset < 0.0 ? ChFiDS_Concave : ChFiDS_Convex;
  std::vector<RoundedTogether> found;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> corners;
  std::vector<TopoDS_Face> queue{body.face(face)};
  seen.Add(body.face(face));
  const auto add = [&](const TopoDS_Shape& f, bool corner) {
    if (!seen.Add(f)) {
      return;
    }
    queue.push_back(TopoDS::Face(f));
    const int index = body.face_index(f);
    if (index >= 0 && index != face) {
      found.push_back({index, corner});
    }
  };
  for (std::size_t next = 0; next < queue.size(); ++next) {
    const TopoDS_Face current = queue[next];
    if (const NCollection_List<TopoDS_Shape>* edges = analyse.Descendants(current)) {
      for (const TopoDS_Shape& e : *edges) {
        const NCollection_List<BRepOffset_Interval>& types = analyse.Type(TopoDS::Edge(e));
        const NCollection_List<TopoDS_Shape>& faces = analyse.Ancestors(e);
        if (types.IsEmpty() || faces.Extent() != 2) {
          continue;
        }
        const ChFiDS_TypeOfConcavity type = types.First().Type();
        if (type == ChFiDS_Tangential ||
            (type == rounding && !BRepOffset_Offset::JoinsOffsets(TopoDS::Edge(e), TopoDS::Face(faces.First()),
                                                                  TopoDS::Face(faces.Last())))) {
          add(faces.First().IsSame(current) ? faces.Last() : faces.First(), false);
        }
      }
    }
    for (TopExp_Explorer v(current, TopAbs_VERTEX); v.More(); v.Next()) {
      if (!corners.Add(v.Current()) || !analyse.HasAncestor(v.Current())) {
        continue;
      }
      NCollection_List<TopoDS_Shape> edges;
      analyse.Edges(TopoDS::Vertex(v.Current()), rounding, edges);
      if (edges.Extent() < 3 || edges.Extent() != analyse.Ancestors(v.Current()).Extent()) {
        continue;
      }
      NCollection_List<TopoDS_Shape> faces;
      for (const TopoDS_Shape& e : edges) {
        for (const TopoDS_Shape& f : analyse.Ancestors(e)) {
          faces.Append(f);
        }
      }
      if (!BRepOffset_Offset::JoinsOffsets(TopoDS::Vertex(v.Current()), faces)) {
        for (const TopoDS_Shape& f : faces) {
          add(f, true);
        }
      }
    }
  }
  return found;
}

// Checks the offsets of their own (`own`, of the body's faces other than
// the `removed` ones; the rest are offset by `offset`) against the faces the
// geometry kernel offsets as one (offset_together, rounded_together): it
// would give them all one of their offsets. With `tangent_chain` a face that
// meets one with an offset of its own smoothly takes that offset (added to
// `own`); a face that meets it so with another offset, or (in a `rounded`
// offset) is joined to it with another offset by a rounding that cannot run
// from one thickness to the other, is refused, both faces painted. Where a
// rounding can (mitcad#2488), the kernel builds it between the two offsets.
void spread_own_offsets(const Shape& body, const std::set<int>& removed, double offset, bool rounded,
                        bool tangent_chain, FaceOffsets& own) {
  if (own.empty()) {
    return;
  }
  BRep_Builder builder;
  TopoDS_Compound kept;
  builder.MakeCompound(kept);
  for (int f = 0; f < body.face_count(); ++f) {
    if (removed.count(f) == 0) {
      builder.Add(kept, body.face(f));
    }
  }
  // The angle below which the kernel's offset takes two faces for meeting
  // smoothly (BRepOffset_MakeOffset::MakeOffsetShape).
  double tolerance = kTolerance;
  for (TopExp_Explorer it(body.occt(), TopAbs_VERTEX); it.More(); it.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Vertex(it.Current())));
  }
  const double angle =
      4.0 * std::asin(std::min(tolerance / (std::abs(offset * 0.5) + Precision::Confusion()), 1.0));
  const BRepOffset_Analyse analyse(kept, angle);
  const auto value = [&](int f) { return own.count(f) > 0 ? own.at(f) : offset; };
  const auto differ = [](double a, double b) { return std::abs(a - b) > kTolerance; };
  const auto refuse = [&](int a, int b, const std::string& said) {
    const std::string first = detail::primary_name(body, a);
    const std::string second = detail::primary_name(body, b);
    throw std::invalid_argument(placed_error(said, said + " (" + first + ", " + second + ")",
                                             {face_place(first), face_place(second)}));
  };
  const FaceOffsets given = own;
  for (const auto& [face, distance] : given) {
    for (const int other : offset_together(body, analyse, face)) {
      if (!differ(value(other), distance)) {
        continue;
      }
      if (tangent_chain && own.count(other) == 0) {
        own[other] = distance;
        continue;
      }
      refuse(face, other,
             tangent_chain || given.count(other) > 0
                 ? "shell failed; two faces that meet smoothly have different thicknesses of their own, and "
                   "the geometry kernel offsets such faces as one"
                 : "shell failed; a face with its own thickness meets a face of another thickness smoothly, and "
                   "the geometry kernel offsets such faces as one: give them one thickness or turn on Tangent "
                   "Chain");
    }
  }
  if (!rounded) {
    return;
  }
  for (const auto& [face, distance] : FaceOffsets(own)) {
    for (const RoundedTogether& other : rounded_together(body, analyse, face, offset)) {
      if (!differ(value(other.face), distance)) {
        continue;
      }
      refuse(face, other.face,
             other.corner
                 ? "shell failed; a rounded corner of the wall would join a face with its own thickness to a "
                   "face of another thickness, and the geometry kernel rounds a corner between walls of "
                   "different thicknesses only where three walls meet square to each other: give them one "
                   "thickness or choose the Sharp shell type"
                 : "shell failed; a rounding of the wall would join a face with its own thickness to a face of "
                   "another thickness, and the geometry kernel rounds between walls of different thicknesses "
                   "only along a straight edge where neither wall turns or along a circle about whose axis both "
                   "walls turn: give them one thickness or choose the Sharp shell type");
    }
  }
}

// OCCT's thick solid with offsets of their own on faces
// (BRepOffset_MakeOffset::SetOffsetOnFace, mitcad#2443): what
// MakeThickSolidByJoin does, which takes no such offsets, so that the
// result's history (Modified, Generated) stays the thick solid's.
class ThickSolidWithOwnOffsets : public BRepOffsetAPI_MakeThickSolid {
public:
  void make(const TopoDS_Shape& solid, const NCollection_List<TopoDS_Shape>& closing, double offset,
            GeomAbs_JoinType join, const std::vector<std::pair<TopoDS_Face, double>>& own,
            const Message_ProgressRange& range) {
    NotDone();
    myLastUsedAlgo = OffsetAlgo_JOIN;
    myOffsetShape.Initialize(solid, offset, kTolerance, BRepOffset_Skin, false, false, join, false, false);
    for (const TopoDS_Shape& face : closing) {
      myOffsetShape.AddFace(TopoDS::Face(face));
    }
    for (const auto& [face, distance] : own) {
      myOffsetShape.SetOffsetOnFace(face, distance);
    }
    myOffsetShape.MakeThickSolid(range);
    if (!myOffsetShape.IsDone()) {
      return;
    }
    myShape = myOffsetShape.Shape();
    Done();
  }
};

// OCCT's offset of a whole body with offsets of their own on faces, as
// ThickSolidWithOwnOffsets for PerformByJoin.
class OffsetShapeWithOwnOffsets : public BRepOffsetAPI_MakeOffsetShape {
public:
  void make(const TopoDS_Shape& solid, double offset, GeomAbs_JoinType join,
            const std::vector<std::pair<TopoDS_Face, double>>& own, const Message_ProgressRange& range) {
    NotDone();
    myLastUsedAlgo = OffsetAlgo_JOIN;
    myOffsetShape.Initialize(solid, offset, kTolerance, BRepOffset_Skin, false, false, join, false, false);
    for (const auto& [face, distance] : own) {
      myOffsetShape.SetOffsetOnFace(face, distance);
    }
    myOffsetShape.MakeOffsetShape(range);
    if (!myOffsetShape.IsDone()) {
      return;
    }
    myShape = myOffsetShape.Shape();
    Done();
  }
};

// The faces of `own` as the kernel's offset takes them.
std::vector<std::pair<TopoDS_Face, double>> own_faces(const Shape& body, const FaceOffsets& own) {
  std::vector<std::pair<TopoDS_Face, double>> faces;
  for (const auto& [face, distance] : own) {
    faces.emplace_back(body.face(face), distance);
  }
  return faces;
}

// Names the faces generated by an offset or thick solid from the body's
// edges and vertices (wall ends at removed faces, rounded joins) when they
// have no name yet.
template <class Operation>
void name_generated_from_edges(detail::FaceNamer& namer, Operation& operation, const Shape& body,
                               const std::string& feature, const std::set<int>& removed) {
  for (int e = 0; e < body.edge_count(); ++e) {
    const std::string& name = body.edge_name(e);
    if (name.empty()) {
      continue;
    }
    bool at_removed = false;
    for (int f : detail::faces_at_edge(body, e)) {
      at_removed = at_removed || removed.count(f) > 0;
    }
    for (const TopoDS_Shape& face : generated_faces(operation, body.edge(e))) {
      if (!namer.named(face)) {
        namer.add(face, face_name(feature, at_removed ? "offset_cap" : "offset", name));
      }
    }
  }
  for (int v = 0; v < body.vertex_count(); ++v) {
    const std::string& name = body.vertex_name(v);
    if (name.empty()) {
      continue;
    }
    for (const TopoDS_Shape& face : generated_faces(operation, body.vertex(v))) {
      if (!namer.named(face)) {
        namer.add(face, face_name(feature, "offset", name));
      }
    }
  }
}

// One direction of a B-spline surface's piece under a face: the surface's
// range [first, last], ended before each sliver (a knot span shorter than a
// millionth of the range) that one of the face's ends [lo, hi] lies in or
// at.
std::pair<double, double> span_without_end_slivers(const NCollection_Array1<double>& knots, double lo, double hi,
                                                   double first, double last) {
  const double near = 1e-6 * (last - first);
  double from = first;
  double to = last;
  for (int i = knots.Lower(); i < knots.Upper(); ++i) {
    const double a = knots(i);
    const double b = knots(i + 1);
    if (b - a >= near) {
      continue;
    }
    if (hi >= a - near && hi <= b + near) {
      to = std::min(to, a);
    }
    if (lo >= a - near && lo <= b + near) {
      from = std::max(from, b);
    }
  }
  return {from, to};
}

// The face (taken forward) put on `surface`, a surface with the same
// parameters as its own over the face (a piece of it, or the same surface
// in another form), in place (the callers' working copies): its edges'
// curves on it carried over as they are, the same parameters, so the same
// curves.
void put_on_same_parameters(const TopoDS_Face& face, const occ::handle<Geom_Surface>& surface,
                            const TopLoc_Location& location, BRep_Builder& builder) {
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(e.Current());
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (curve.IsNull()) {
      continue;
    }
    const double tolerance = BRep_Tool::Tolerance(edge);
    if (BRep_Tool::IsClosed(edge, face)) {
      const occ::handle<Geom2d_Curve> other =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Reversed()), face, first, last);
      if (edge.Orientation() == TopAbs_REVERSED) {
        builder.UpdateEdge(edge, other, curve, surface, location, tolerance);
      } else {
        builder.UpdateEdge(edge, curve, other, surface, location, tolerance);
      }
    } else {
      builder.UpdateEdge(edge, curve, surface, location, tolerance);
    }
    builder.Range(edge, surface, location, first, last);
  }
  builder.UpdateFace(face, surface, location, BRep_Tool::Tolerance(face));
}

// Each face of the shape on a B-spline surface with a sliver span at one of
// the face's ends, put on the surface's piece without it, with its edges'
// curves on it; in place (the callers' working copies). The offset evaluates
// a face's surface, its normal and its edges' curves on it up to the face's
// ends. A rounding's surface walked on past a vertex can turn back in a
// span a few ten-millionths long right where its face ends (its normal
// turned round there): which side of the turn the face's end falls on
// depends on the last bits of the walk, which differ between compilers and
// their maths libraries, and where it falls past the turn the offset of the
// face is turned round at that end and the offset faces do not close up
// (mitcad#2247: a Y tube's crotch rounded and shelled, on Windows and macOS,
// not on Linux). Without the sliver the face's end is on the span before
// it, carried on a few ten-millionths.
void cut_end_slivers(const TopoDS_Shape& shape) {
  BRep_Builder builder;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> done;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face face = TopoDS::Face(it.Current().Oriented(TopAbs_FORWARD));
    if (!done.Add(face.Located(TopLoc_Location()))) {
      continue;
    }
    TopLoc_Location location;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
    const occ::handle<Geom_BSplineSurface> spline = occ::down_cast<Geom_BSplineSurface>(surface);
    if (spline.IsNull() || spline->IsUPeriodic() || spline->IsVPeriodic()) {
      continue;
    }
    double u1 = 0.0, u2 = 0.0, v1 = 0.0, v2 = 0.0;
    spline->Bounds(u1, u2, v1, v2);
    double fu1 = 0.0, fu2 = 0.0, fv1 = 0.0, fv2 = 0.0;
    BRepTools::UVBounds(face, fu1, fu2, fv1, fv2);
    const auto [pu1, pu2] = span_without_end_slivers(spline->UKnots(), fu1, fu2, u1, u2);
    const auto [pv1, pv2] = span_without_end_slivers(spline->VKnots(), fv1, fv2, v1, v2);
    if ((pu1 == u1 && pu2 == u2 && pv1 == v1 && pv2 == v2) || !(pu1 < pu2) || !(pv1 < pv2)) {
      continue;
    }
    const occ::handle<Geom_BSplineSurface> piece = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
    try {
      piece->Segment(pu1, pu2, pv1, pv2);
    } catch (const Standard_Failure&) {
      continue;
    }
    put_on_same_parameters(face, piece, location, builder);
  }
}

// The body with each face whose surface is only C0 continuous split at its
// creases into C1 pieces that keep the face's names, or null when no face
// has one. OCCT's offset refuses C0 surfaces (BRepOffset_C0Geometry), though
// the face is often smooth: a stored body's B-splines whose knots repeat as
// often as their degree, such as rational arcs. Each piece offsets on its
// own, which gives the same wall (mitcad#158). `removed` (faces of the body)
// becomes the pieces of those faces, and so do the faces of `own` (offsets
// of their own, mitcad#2443), each piece with its face's offset. `failure`
// heads the error where the split itself raises one.
ShapePtr split_at_creases(const Shape& body, std::set<int>& removed, FaceOffsets* own = nullptr,
                          const char* failure = "shell failed while splitting faces at their creases") {
  bool creased = false;
  for (int i = 0; i < body.face_count() && !creased; ++i) {
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(body.face(i));
    creased = !surface.IsNull() && surface->Continuity() == GeomAbs_C0;
  }
  if (!creased) {
    return nullptr;
  }
  ShapeUpgrade_ShapeDivideContinuity divide(body.occt());
  divide.SetSurfaceCriterion(GeomAbs_C1);
  step(failure, [&] { divide.Perform(); });
  const TopoDS_Shape split = divide.Result();
  if (split.IsNull()) {
    return nullptr;
  }
  // The split leaves some edges' curves on their faces on ranges other than
  // their 3D curve's, with the edge still flagged same-parameter: an
  // invalid edge (BRepCheck_InvalidSameParameterFlag), harmless to a closed
  // offset, which keeps no face of the body, but carried into a thick
  // solid's outside of the wall (GitHub #645's loft with its top open,
  // mitcad#2462). Their curves on faces are put on the 3D curve's range.
  for (TopExp_Explorer it(split, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (!BRep_Tool::SameRange(edge)) {
      BRepLib::SameRange(edge, Precision::PConfusion());
    }
  }
  const occ::handle<ShapeBuild_ReShape> context = divide.GetContext();
  std::vector<Shape::NamedFace> faces;
  std::vector<TopoDS_Shape> removed_pieces;
  std::vector<std::pair<TopoDS_Shape, double>> own_pieces;
  for (int i = 0; i < body.face_count(); ++i) {
    for (TopExp_Explorer it(context->Apply(body.face(i)), TopAbs_FACE); it.More(); it.Next()) {
      faces.push_back({it.Current(), body.face_names(i)});
      if (removed.count(i) > 0) {
        removed_pieces.push_back(it.Current());
      }
      if (own != nullptr && own->count(i) > 0) {
        own_pieces.emplace_back(it.Current(), own->at(i));
      }
    }
  }
  ShapePtr out = std::make_shared<Shape>(split, faces);
  std::set<int> pieces;
  for (const TopoDS_Shape& piece : removed_pieces) {
    const int index = out->face_index(piece);
    if (index < 0) {
      return nullptr;
    }
    pieces.insert(index);
  }
  FaceOffsets offsets;
  for (const auto& [piece, offset] : own_pieces) {
    const int index = out->face_index(piece);
    if (index < 0) {
      return nullptr;
    }
    offsets[index] = offset;
  }
  removed = pieces;
  if (own != nullptr) {
    *own = offsets;
  }
  return out;
}

// Why a shell inside cannot open a removed face, in words, where the wall is
// exactly as thick as a convex circular rounding beside the face, tangent to
// it (mitcad#2494): the rounding's offset is its axis, and the offset of the
// face beyond the rounding meets the removed face's plane on the face's edge
// itself, a rim of no width, which the geometry kernel's thick solid cannot
// build (a box with its top edges rounded 2 mm, shelled 2 mm with the top
// open). Empty where there is no such rounding.
std::string rim_of_no_width(const Shape& body, const std::set<int>& removed, double offset) {
  if (offset >= 0.0) {
    return {};
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of_edge;
  TopExp::MapShapesAndAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
  for (const int f : removed) {
    const TopoDS_Face& face = body.face(f);
    for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
      const NCollection_List<TopoDS_Shape>* beside = faces_of_edge.Seek(edge);
      if (beside == nullptr || BRep_Tool::Degenerated(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : *beside) {
        const int g = body.face_index(other);
        if (other.IsSame(face) || g < 0 || removed.count(g) > 0 ||
            BRep_Tool::Continuity(edge, face, TopoDS::Face(other)) < GeomAbs_G1) {
          continue;
        }
        const BRepAdaptor_Surface surface(TopoDS::Face(other));
        if (surface.GetType() != GeomAbs_Cylinder) {
          continue;
        }
        const gp_Cylinder cylinder = surface.Cylinder();
        if (std::abs(cylinder.Radius() + offset) > 1e-4) {
          continue;
        }
        // Convex: the face's outward normal points away from the axis.
        double u1 = 0.0, u2 = 0.0, v1 = 0.0, v2 = 0.0;
        BRepTools::UVBounds(TopoDS::Face(other), u1, u2, v1, v2);
        gp_Pnt point;
        gp_Vec du, dv;
        surface.D1(0.5 * (u1 + u2), 0.5 * (v1 + v2), point, du, dv);
        gp_Vec normal = du.Crossed(dv);
        if (other.Orientation() == TopAbs_REVERSED) {
          normal.Reverse();
        }
        const gp_Ax1 axis = cylinder.Axis();
        gp_Vec radial(axis.Location(), point);
        radial -= gp_Vec(axis.Direction()) * radial.Dot(gp_Vec(axis.Direction()));
        if (normal.Dot(radial) <= 0.0) {
          continue;
        }
        char radius[32];
        std::snprintf(radius, sizeof radius, "%g", cylinder.Radius());
        // The words for the person; the rounding and the removed face are
        // the error's places, named in its detail.
        const std::string said = std::string("shell failed; the wall is exactly as thick as a rounding beside "
                                             "the removed face (radius ") +
                                 radius +
                                 " mm): the inside of the wall would meet that face on its own edge, in a rim "
                                 "of no width, which cannot be built; make the wall thinner or thicker than the "
                                 "rounding";
        const std::string rounding = detail::primary_name(body, g);
        const std::string top = detail::primary_name(body, f);
        return placed_error(said, said + " (the rounding " + rounding + ", the removed face " + top + ")",
                            {face_place(rounding), face_place(top)});
      }
    }
  }
  return {};
}

// The wall between the body's surface and its offset by `offset` (negative
// inside), with the removed faces open. The outside of the wall keeps the
// face names; the inside is offset(<face>). The faces of `own_offsets` (of
// the input) are offset by their own offsets (spread_own_offsets).
ShapePtr thick_solid(const std::string& feature, const Shape& input, const std::set<int>& removed_faces,
                     double offset, GeomAbs_JoinType join, const FaceOffsets& own_offsets = {},
                     bool tangent_chain = true) {
  // OCCT's offset widens the tolerances of its input's sub-shapes.
  const ShapePtr copy = detail::working_copy(input);
  cut_end_slivers(copy->occt());
  std::set<int> removed = removed_faces;
  FaceOffsets own = own_offsets;
  const ShapePtr split = split_at_creases(*copy, removed, &own);
  const Shape& body = split ? *split : *copy;
  spread_own_offsets(body, removed, offset, join == GeomAbs_Arc, tangent_chain, own);
  NCollection_List<TopoDS_Shape> closing;
  for (int f : removed) {
    closing.Append(body.face(f));
  }
  ThickSolidWithOwnOffsets maker;
  detail::shell_dump("thick-input", body.occt());
  detail::interruptible([&](const Message_ProgressRange& range) {
    named_failure([&] {
      if (own.empty()) {
        maker.MakeThickSolidByJoin(body.occt(), closing, offset, kTolerance, BRepOffset_Skin, false,
                                   false, join, false, range);
      } else {
        maker.make(body.occt(), closing, offset, join, own_faces(body, own), range);
      }
    });
  });
  if (!maker.IsDone()) {
    const std::string rim = rim_of_no_width(body, removed, offset);
    throw std::runtime_error(rim.empty() ? offset_failure(maker.MakeOffset()) : rim);
  }
  detail::shell_dump("thick-result", maker.Shape());
  const TopoDS_Shape result = single_solid(maker.Shape(), "the shell");
  detail::FaceNamer namer(result);
  for (int i = 0; i < body.face_count(); ++i) {
    if (removed.count(i) > 0) {
      continue;
    }
    const NameList& names = body.face_names(i);
    const std::string inner = face_name(feature, "offset", detail::primary_name(body, i));
    const bool unnamed = names.empty();
    for (const TopoDS_Shape& face : images_of(maker, body.face(i), result)) {
      if (offset < 0) {
        add_names(namer, face, names);
      } else if (!unnamed) {
        namer.add(face, inner);
      }
    }
    for (const TopoDS_Shape& face : generated_faces(maker, body.face(i))) {
      if (offset > 0) {
        add_names(namer, face, names);
      } else if (!unnamed) {
        namer.add(face, inner);
      }
    }
  }
  name_generated_from_edges(namer, maker, body, feature, removed);
  // The end of the wall where a removed face was: generated from it, or
  // else lying on its surface.
  for (int f : removed) {
    const std::string name = face_name(feature, "offset_cap", detail::primary_name(body, f));
    if (detail::primary_name(body, f).empty()) {
      continue;
    }
    std::vector<TopoDS_Shape> caps = generated_faces(maker, body.face(f));
    for (const TopoDS_Shape& image : maker.Modified(body.face(f))) {
      caps.push_back(image);
    }
    for (TopExp_Explorer it(result, TopAbs_FACE); it.More(); it.Next()) {
      if (!namer.named(it.Current()) && on_surface(TopoDS::Face(it.Current()), body.face(f))) {
        caps.push_back(it.Current());
      }
    }
    // Where the body meets the removed face only tangentially, the wall
    // ends in strips along the normal from the face's edges to their offsets
    // (OCCT port patch 2462): the faces holding its edges.
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> removed_edges;
    for (TopExp_Explorer it(body.face(f), TopAbs_EDGE); it.More(); it.Next()) {
      removed_edges.Add(it.Current());
    }
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> body_faces;
    for (TopExp_Explorer it(body.occt(), TopAbs_FACE); it.More(); it.Next()) {
      body_faces.Add(it.Current());
    }
    for (TopExp_Explorer it(result, TopAbs_FACE); it.More(); it.Next()) {
      if (namer.named(it.Current()) || body_faces.Contains(it.Current())) {
        continue;
      }
      for (TopExp_Explorer edges(it.Current(), TopAbs_EDGE); edges.More(); edges.Next()) {
        if (removed_edges.Contains(edges.Current())) {
          caps.push_back(it.Current());
          break;
        }
      }
    }
    for (const TopoDS_Shape& cap : caps) {
      if (cap.ShapeType() == TopAbs_FACE && !namer.named(cap)) {
        namer.add(cap, name);
      }
    }
  }
  namer.finish();
  return namer.shape();
}

// The body offset by `offset` as a closed solid, with each face named after
// the body face it comes from: the same names, or offset(<face>) with `inner`.
// The faces of `own_offsets` (of the input) are offset by their own offsets
// (spread_own_offsets).
ShapePtr offset_body(const std::string& feature, const Shape& input, double offset,
                     GeomAbs_JoinType join, bool inner, const FaceOffsets& own_offsets = {},
                     bool tangent_chain = true) {
  // OCCT's offset widens the tolerances of its input's sub-shapes.
  const ShapePtr copy = detail::working_copy(input);
  cut_end_slivers(copy->occt());
  std::set<int> none;
  FaceOffsets own = own_offsets;
  const ShapePtr split = split_at_creases(*copy, none, &own);
  const Shape& body = split ? *split : *copy;
  spread_own_offsets(body, none, offset, join == GeomAbs_Arc, tangent_chain, own);
  const TopoDS_Shape solid = offset_input(body.occt());
  OffsetShapeWithOwnOffsets maker;
  detail::interruptible([&](const Message_ProgressRange& range) {
    named_failure([&] {
      if (own.empty()) {
        maker.PerformByJoin(solid, offset, kTolerance, BRepOffset_Skin, false, false, join, false, range);
      } else {
        maker.make(solid, offset, join, own_faces(body, own), range);
      }
    });
  });
  if (!maker.IsDone()) {
    throw std::runtime_error(offset_failure(maker.MakeOffset()));
  }
  const TopoDS_Shape result = single_solid(maker.Shape(), "the shell");
  detail::FaceNamer namer(result);
  for (int i = 0; i < body.face_count(); ++i) {
    const NameList& names = body.face_names(i);
    if (names.empty()) {
      continue;
    }
    for (const TopoDS_Shape& face : generated_faces(maker, body.face(i))) {
      if (inner) {
        namer.add(face, face_name(feature, "offset", names.front()));
      } else {
        add_names(namer, face, names);
      }
    }
  }
  name_generated_from_edges(namer, maker, body, feature, {});
  return namer.shape();
}

// The body itself with every face renamed offset(<face>), as a void.
ShapePtr renamed_as_inner(const std::string& feature, const Shape& body) {
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < body.face_count(); ++i) {
    if (!body.face_names(i).empty()) {
      faces.push_back({body.face(i), {face_name(feature, "offset", body.face_names(i).front())}});
    }
  }
  return std::make_shared<Shape>(body.occt(), faces);
}

TopoDS_Shell outer_shell(const TopoDS_Shape& solid) {
  TopExp_Explorer it(solid, TopAbs_SHELL);
  if (!it.More()) {
    throw std::runtime_error("the shell has no surface");
  }
  return TopoDS::Shell(it.Current());
}

// A solid bounded by the outer body and, reversed, the inner one.
ShapePtr hollow(const Shape& outer, const Shape& inner) {
  BRep_Builder builder;
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  builder.Add(solid, outer_shell(outer.occt()));
  builder.Add(solid, outer_shell(inner.occt()).Reversed());
  std::vector<Shape::NamedFace> faces;
  for (const Shape* part : {&outer, &inner}) {
    for (int i = 0; i < part->face_count(); ++i) {
      faces.push_back({part->face(i), part->face_names(i)});
    }
  }
  detail::FaceNamer namer(solid);
  for (const Shape::NamedFace& face : faces) {
    add_names(namer, face.face, face.names);
  }
  namer.finish();
  return namer.shape();
}

// Fuses the inner and outer walls of a shell to both sides.
ShapePtr fuse_walls(const Shape& inner, const Shape& outer) {
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(inner.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(outer.occt());
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  detail::build(fuse);
  if (!fuse.IsDone() || fuse.HasErrors()) {
    throw std::runtime_error("shell failed; the inner and outer walls could not be joined");
  }
  fuse.SimplifyResult();
  detail::FaceNamer namer(fuse.Shape());
  namer.carry(fuse, inner);
  namer.carry(fuse, outer);
  namer.finish();
  return namer.shape();
}

// The volume of the shape measured on a copy whose faces are split where
// their surfaces are less than C2 continuous. The kernel integrates a face
// with a fixed number of Gauss points over its whole parameter range (and
// an offset surface without its basis's knots), which is far off on one
// face that runs through pieces of very different curvature joined C1: the
// side of a loft through sections merged into one smooth edge each, and
// above all its offset (mitcad#2271: a valid 2 mm wall of 85 900 mm3
// measured -10 460, so Shell called it empty). Split there, each piece is
// smooth and measured as well as any face. The volumes MitCad reports do
// not take this rule: the analysis library integrates every face span by
// span between its surface's knots (mitcad#2309).
double split_volume(const TopoDS_Shape& shape) {
  ShapeUpgrade_ShapeDivideContinuity divide(BRepBuilderAPI_Copy(shape).Shape());
  divide.SetBoundaryCriterion(GeomAbs_C2);
  divide.SetPCurveCriterion(GeomAbs_C0);
  divide.SetSurfaceCriterion(GeomAbs_C2);
  // Perform says whether it split anything; unsplit, the result is the
  // copy as it was.
  divide.Perform();
  const TopoDS_Shape split = divide.Result();
  GProp_GProps props;
  BRepGProp::VolumeProperties(split.IsNull() ? shape : split, props);
  return props.Mass();
}

void check_result(const ShapePtr& shape, const char* operation) {
  detail::require_valid(shape->occt(), operation);
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape->occt(), props);
  // Measured again with its faces split where the first measure says empty
  // (mitcad#2271, split_volume).
  if (!(props.Mass() > kTolerance) && !(split_volume(shape->occt()) > kTolerance)) {
    throw std::runtime_error(std::string(operation) + " produced an empty body");
  }
}

std::set<int> as_set(const std::vector<int>& faces) { return {faces.begin(), faces.end()}; }

// The shell `primary` builds, or where the geometry kernel fails on it, the
// body less a cavity (shell_cavity.hpp, mitcad#1233): `fallback`, null
// when that cannot be built either (then the kernel's failure stands, in
// `refused`'s words where the kernel gave none, mitcad#2140).
template <class Primary, class Fallback, class Refused>
ShapePtr or_by_cavity(Primary&& primary, Fallback&& fallback, Refused&& refused) {
  std::exception_ptr failure;
  try {
    return primary();
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_OutOfMemory&) {
    throw;
  } catch (const std::runtime_error& error) {
    detail::shell_trace(std::string("the kernel's offset failed: ") + error.what());
    failure = std::current_exception();
  } catch (const Standard_Failure& error) {
    detail::shell_trace(std::string("the kernel's offset raised ") + typeid(error).name() + ": " +
                        error.what());
    failure = std::current_exception();
  }
  ShapePtr result = fallback();
  if (!result) {
    detail::shell_trace("the cavity built nothing; the kernel's failure stands");
    if (said_nothing(failure)) {
      throw std::runtime_error(refused());
    }
    std::rethrow_exception(failure);
  }
  return result;
}

// The body hollowed inside by the cavity: `outer` the shape it is cut out
// of (null: the body); null when it cannot be built.
ShapePtr cavity_shell(const std::string& feature, const Shape& input, const std::set<int>& removed_faces,
                      double inside, GeomAbs_JoinType join, const Shape* outer) {
  // (A test asking the kernel to refuse the shell, mitcad#2140: no cavity.)
  if (detail::refusal_asked("shell")) {
    return nullptr;
  }
  // OCCT's offset widens the tolerances of its input's sub-shapes.
  const ShapePtr copy = detail::working_copy(input);
  cut_end_slivers(copy->occt());
  std::set<int> removed = removed_faces;
  const ShapePtr split = split_at_creases(*copy, removed);
  const Shape& body = split ? *split : *copy;
  detail::shell_dump("cavity-input", body.occt());
  ShapePtr result = detail::shell_by_cavity(feature, body, removed, inside, join, outer ? *outer : body);
  if (result) {
    detail::shell_dump("cavity-result", result->occt());
    try {
      check_result(result, "the shell");
    } catch (const std::runtime_error& error) {
      detail::shell_trace(std::string("the cavity's result was refused: ") + error.what());
      throw;
    }
  }
  return result;
}

// The offsets of a shell's faces with their own thickness (mitcad#2443),
// by the body's face index: into the material (negative) and out of it.
struct OwnOffsets {
  FaceOffsets inside;
  FaceOffsets outside;
};

// The faces of the shell's `own` on the body, with their offsets. Refused:
// a thickness on a side the shell's wall has not (or none where it has),
// a removed face, a face given two thicknesses.
OwnOffsets own_offsets(const Shape& body, const ShellSpec& spec, const std::set<int>& removed) {
  OwnOffsets offsets;
  for (const ShellFace& own : spec.own) {
    require_not_negative("a face's own inside thickness", own.inside);
    require_not_negative("a face's own outside thickness", own.outside);
    if ((own.inside > 0.0) != (spec.inside > 0.0) || (own.outside > 0.0) != (spec.outside > 0.0)) {
      throw std::invalid_argument(face_error(own.face,
                                             "a face's own thickness must be on the sides of the shell's wall: "
                                             "inside, outside or both, as the shell's thickness",
                                             "the own thickness of face " + own.face +
                                                 " must be on the sides of the shell's wall"));
    }
    for (const int f : detail::resolve_faces(body, {own.face}, "faces with their own thickness")) {
      const std::string name = detail::primary_name(body, f);
      if (removed.count(f) > 0) {
        throw std::invalid_argument(face_error(name, "a face the shell removes cannot have its own thickness",
                                               "face " + name + " is removed; it cannot have its own thickness"));
      }
      const bool twice = (offsets.inside.count(f) > 0 && offsets.inside.at(f) != -own.inside) ||
                         (offsets.outside.count(f) > 0 && offsets.outside.at(f) != own.outside);
      if (twice) {
        throw std::invalid_argument(face_error(name, "a face is given two thicknesses of its own",
                                               "face " + name + " is given two thicknesses of its own"));
      }
      if (own.inside > 0.0) {
        offsets.inside[f] = -own.inside;
      }
      if (own.outside > 0.0) {
        offsets.outside[f] = own.outside;
      }
    }
  }
  return offsets;
}

// Whether `build` succeeds; a cancel or a failed allocation goes on (OCCT's
// failures are std::exceptions).
template <class Build>
bool builds(Build&& build) {
  try {
    build();
    return true;
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_OutOfMemory&) {
    throw;
  } catch (const std::bad_alloc&) {
    throw;
  } catch (const std::exception&) {
    return false;
  }
}

// What an error says.
std::string what_of(const std::exception_ptr& failure) {
  try {
    std::rethrow_exception(failure);
  } catch (const std::exception& error) {
    return error.what();
  } catch (...) {
  }
  return std::string();
}

// The shell of a body with faces of their own thickness (mitcad#2443): the
// walls as shell() builds them, each face offset by its own thickness in
// the geometry kernel's offset, without the cavity (which has one
// thickness). Where the kernel's offset fails and the shell builds at its
// own thickness alone, the faces whose own thickness the walls around them
// cannot meet are found one by one, and the error paints them; where it
// fails at the shell's thickness too, that failure stands.
ShapePtr own_thickness_shell(const std::string& feature, const Shape& body, const ShellSpec& spec,
                             const std::set<int>& removed, GeomAbs_JoinType join, const OwnOffsets& own) {
  // The largest wall inside, for the notes of missing walls.
  double inside = spec.inside;
  for (const auto& [face, offset] : own.inside) {
    inside = std::max(inside, -offset);
  }
  const auto build = [&](const OwnOffsets& offsets) {
    ShapePtr result;
    if (removed.empty()) {
      const ShapePtr outer =
          spec.outside > 0.0 ? offset_body(feature, body, spec.outside, join, false, offsets.outside, spec.tangent_chain)
                             : std::make_shared<Shape>(body);
      const ShapePtr inner =
          spec.inside > 0.0 ? offset_body(feature, body, -spec.inside, join, true, offsets.inside, spec.tangent_chain)
                            : renamed_as_inner(feature, body);
      result = hollow(*outer, *inner);
      check_result(result, "the shell");
      if (spec.inside > 0.0) {
        detail::note_missing_walls(feature, body, removed, inside, *result);
      }
      return result;
    }
    ShapePtr inner;
    if (spec.inside > 0.0) {
      inner = thick_solid(feature, body, removed, -spec.inside, join, offsets.inside, spec.tangent_chain);
      check_result(inner, "the shell");
      detail::note_missing_walls(feature, body, removed, inside, *inner);
    }
    if (spec.outside == 0.0) {
      return inner;
    }
    const ShapePtr outer = thick_solid(feature, body, removed, spec.outside, join, offsets.outside, spec.tangent_chain);
    if (inner) {
      result = fuse_walls(*inner, *outer);
      result->copy_notes(*inner);
    } else {
      result = outer;
    }
    check_result(result, "the shell");
    return result;
  };
  std::exception_ptr failure;
  try {
    return build(own);
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_OutOfMemory&) {
    throw;
  } catch (const std::invalid_argument&) {
    // Refused before the offset (spread_own_offsets).
    throw;
  } catch (const std::runtime_error& error) {
    detail::shell_trace(std::string("the offset with faces of their own thickness failed: ") + error.what());
    failure = std::current_exception();
  } catch (const Standard_Failure& error) {
    detail::shell_trace(std::string("the offset with faces of their own thickness raised ") + error.what());
    failure = std::current_exception();
  }
  if (!builds([&] { build(OwnOffsets()); })) {
    std::rethrow_exception(failure);
  }
  std::set<int> faces;
  for (const FaceOffsets* side : {&own.inside, &own.outside}) {
    for (const auto& [face, offset] : *side) {
      faces.insert(face);
    }
  }
  std::vector<int> unmet;
  for (const int face : faces) {
    OwnOffsets alone;
    if (own.inside.count(face) > 0) {
      alone.inside[face] = own.inside.at(face);
    }
    if (own.outside.count(face) > 0) {
      alone.outside[face] = own.outside.at(face);
    }
    if (!builds([&] { build(alone); })) {
      unmet.push_back(face);
    }
  }
  if (unmet.empty()) {
    unmet.assign(faces.begin(), faces.end());
  }
  std::vector<ErrorPlace> places;
  std::string names;
  for (const int face : unmet) {
    places.push_back(face_place(detail::primary_name(body, face)));
    names += (names.empty() ? "" : ", ") + detail::primary_name(body, face);
  }
  const std::string said =
      unmet.size() == 1
          ? "shell failed; the wall of a face cannot be built at its own thickness: the walls of the faces "
            "around it do not meet it"
          : "shell failed; the walls of faces cannot be built at their own thicknesses: the walls of the faces "
            "around them do not meet them";
  throw std::runtime_error(placed_error(said, said + " (" + names + "); " + what_of(failure), places));
}

} // namespace

ShapePtr shell(const std::string& feature, const Shape& body, const ShellSpec& spec) {
  require_not_negative("the inside thickness", spec.inside);
  require_not_negative("the outside thickness", spec.outside);
  if (spec.inside == 0.0 && spec.outside == 0.0) {
    throw std::invalid_argument("the shell needs an inside or an outside thickness");
  }
  detail::require_surfaces(body.occt(), "shell");
  std::vector<int> faces;
  if (!spec.faces.empty()) {
    faces = detail::resolve_faces(body, spec.faces, "faces to remove");
    if (spec.tangent_chain) {
      faces = detail::tangent_closure(body, faces);
    }
    if (static_cast<int>(faces.size()) >= body.face_count()) {
      throw std::invalid_argument("the shell cannot remove every face of the body");
    }
  }
  const GeomAbs_JoinType join = spec.rounded ? GeomAbs_Arc : GeomAbs_Intersection;
  if (!spec.own.empty()) {
    const std::set<int> removed = as_set(faces);
    const OwnOffsets own = own_offsets(body, spec, removed);
    return detail::run("shell", [&] { return own_thickness_shell(feature, body, spec, removed, join, own); });
  }
  return detail::run("shell", [&] {
    // Where the kernel's offset fails, the inside of the wall is the body
    // less a cavity: parts no thicker than the wall stay solid.
    const std::set<int> removed = as_set(faces);
    const auto inside_wall = [&](const Shape* outer, auto&& primary) {
      return or_by_cavity(
          [&] {
            ShapePtr wall = primary();
            check_result(wall, "the shell");
            detail::note_missing_walls(feature, body, removed, spec.inside, *wall);
            return wall;
          },
          [&] { return cavity_shell(feature, body, removed, spec.inside, join, outer); },
          [&] { return shell_refusal(body, removed); });
    };
    ShapePtr result;
    if (faces.empty()) {
      const ShapePtr outer = spec.outside > 0.0
                                 ? offset_body(feature, body, spec.outside, join, false)
                                 : std::make_shared<Shape>(body);
      if (spec.inside > 0.0) {
        return inside_wall(spec.outside > 0.0 ? outer.get() : nullptr, [&] {
          return hollow(*outer, *offset_body(feature, body, -spec.inside, join, true));
        });
      }
      result = hollow(*outer, *renamed_as_inner(feature, body));
    } else if (spec.outside == 0.0) {
      return inside_wall(nullptr, [&] { return thick_solid(feature, body, removed, -spec.inside, join); });
    } else if (spec.inside == 0.0) {
      result = thick_solid(feature, body, removed, spec.outside, join);
    } else {
      const ShapePtr inner =
          inside_wall(nullptr, [&] { return thick_solid(feature, body, removed, -spec.inside, join); });
      result = fuse_walls(*inner, *thick_solid(feature, body, removed, spec.outside, join));
      result->copy_notes(*inner);
    }
    check_result(result, "the shell");
    return result;
  });
}

namespace {

// Drafts the faces of the body about the neutral plane in the pull direction.
ShapePtr draft_faces(const Shape& body, const std::vector<int>& faces, const gp_Pln& neutral,
                     const gp_Dir& pull, double angle, bool tangent_chain) {
  BRepOffsetAPI_DraftAngle maker(body.occt());
  const std::set<int> selected(faces.begin(), faces.end());
  // The faces drafted so far, the tangent ones OCCT adds included.
  std::set<int> drafted;
  for (int f : faces) {
    if (drafted.count(f) > 0) {
      continue;
    }
    const TopoDS_Face& face = body.face(f);
    const std::string name = detail::primary_name(body, f);
    step(face_error(name, "draft failed on a face", "draft failed on face " + name),
         [&] { maker.Add(face, pull, angle, neutral, true); });
    if (!maker.AddDone()) {
      const std::string why = "; only planar, cylindrical and conical faces can be drafted, and the fixed plane "
                              "must cross them";
      throw std::runtime_error(face_error(name, "draft failed on a face" + why, "draft failed on face " + name + why));
    }
    // OCCT drafts the tangent neighbours along.
    for (int index : detail::tangent_closure(body, {f})) {
      drafted.insert(index);
      if (!tangent_chain && selected.count(index) == 0) {
        const std::string why = " is tangent to a drafted face; without a tangent chain only the selected faces "
                                "would be drafted, but the geometry kernel always drafts the whole chain";
        const std::string tangent = detail::primary_name(body, index);
        throw std::invalid_argument(
            face_error(tangent, "unsupported: a face" + why, "unsupported: face " + tangent + why));
      }
    }
  }
  step("draft failed", [&] { detail::build(maker); });
  if (!maker.IsDone()) {
    throw std::runtime_error("draft failed; the neighbouring faces cannot follow the drafted faces");
  }
  // The modifier rebuilds every face it touches; ModifiedShape gives the image.
  detail::FaceNamer namer(maker.Shape());
  // Images that are no face of the result, with their names.
  std::vector<Shape::NamedFace> outside;
  for (int i = 0; i < body.face_count(); ++i) {
    const TopoDS_Shape image = step("draft", [&] { return maker.ModifiedShape(body.face(i)); });
    add_names(namer, image, body.face_names(i));
    if (!body.face_names(i).empty() && image.ShapeType() == TopAbs_FACE && !namer.contains(image)) {
      outside.push_back({image, body.face_names(i)});
    }
  }
  // After it has reported the images, the draft splits the edges of a face
  // where its seam crosses another of its wires and copies the faces with
  // those edges (a drafted cylinder that became a cone, OCCT's CorrectWires);
  // ModifiedShape still gives the faces before that copy, so the copies kept
  // no name, and a sketch or a pick could no longer find them (mitcad#2351).
  // The copies lie exactly in the images they were made of: each face
  // without a name takes the names of the image it lies in.
  if (!outside.empty()) {
    BRep_Builder builder;
    TopoDS_Compound images;
    builder.MakeCompound(images);
    for (const Shape::NamedFace& face : outside) {
      builder.Add(images, face.face);
    }
    namer.carry_by_containment(Shape(images, outside), kTolerance);
  }
  namer.finish();
  return namer.shape();
}

// The faces of the shape the references resolve to, if any.
std::vector<int> faces_in(const Shape& shape, const std::vector<std::string>& references) {
  std::vector<int> faces;
  for (const std::string& reference : references) {
    for (int f : shape.find_faces(reference)) {
      if (std::find(faces.begin(), faces.end(), f) == faces.end()) {
        faces.push_back(f);
      }
    }
  }
  return faces;
}

// Merges the pieces a split left of a name of the original body back into
// the name where one face holds them all again ("X#0", "X#1" -> "X").
ShapePtr join_pieces(const Shape& shape, const Shape& original) {
  std::set<std::string> before;
  for (int i = 0; i < original.face_count(); ++i) {
    before.insert(original.face_names(i).begin(), original.face_names(i).end());
  }
  // The name a piece name comes from, or empty.
  const auto base = [&before](const std::string& name) {
    const std::size_t mark = name.rfind('#');
    if (mark == std::string::npos || name.find_first_of(")]}", mark) != std::string::npos ||
        before.count(name.substr(0, mark)) == 0) {
      return std::string();
    }
    return name.substr(0, mark);
  };
  std::map<std::string, std::vector<int>> holders; // base name -> faces holding a piece
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      if (!base(name).empty()) {
        holders[base(name)].push_back(i);
      }
    }
  }
  detail::FaceNamer namer(shape.occt());
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      const std::size_t mark = name.rfind('#');
      std::string kept = name;
      if (!base(name).empty()) {
        const std::vector<int>& faces = holders[name.substr(0, mark)];
        if (std::all_of(faces.begin(), faces.end(), [i](int f) { return f == i; })) {
          kept = name.substr(0, mark);
        }
      }
      namer.add(shape.face(i), kept);
    }
  }
  namer.finish();
  return namer.shape();
}

} // namespace

ShapePtr draft(const std::string& feature, const Shape& body, const DraftSpec& spec) {
  for (const std::optional<double> angle : {std::optional<double>(spec.angle), spec.angle2}) {
    if (angle && !(std::abs(*angle) < kHalfPi && *angle != 0.0)) {
      throw std::invalid_argument("the draft angle must be between -90 and 90 degrees and not zero");
    }
  }
  if (spec.plane.kind != Tool::Kind::Plane && spec.plane.kind != Tool::Kind::Face) {
    throw std::invalid_argument("the fixed plane must be a plane or a planar face");
  }
  detail::require_surfaces(body.occt(), "draft");
  if (spec.plane.shape) {
    detail::require_surfaces(spec.plane.shape->occt(), "the fixed plane");
  }
  const gp_Pln neutral = detail::tool_plane(spec.plane, "the fixed plane");
  gp_Dir pull = neutral.Axis().Direction();
  if (spec.plane.kind == Tool::Kind::Face) {
    // From the face into the material.
    pull.Reverse();
  }
  if (spec.flip) {
    pull.Reverse();
  }
  const std::vector<int> faces = detail::resolve_faces(body, spec.faces, "faces to draft");
  return detail::run("draft", [&] {
    ShapePtr result;
    if (!spec.angle2) {
      result = draft_faces(body, faces, neutral, pull, spec.angle, spec.tangent_chain);
    } else {
      // Each side of the plane tilts on its own: split the body there, draft
      // the halves (so that OCCT does not carry a draft across the split
      // into the coplanar other half) and join them again.
      const std::vector<ShapePtr> halves =
          split_body(feature, body, Tool::of_plane(neutral), true);
      std::vector<ShapePtr> drafted;
      for (const ShapePtr& half : halves) {
        GProp_GProps props;
        BRepGProp::VolumeProperties(half->occt(), props);
        const bool pull_side =
            gp_Vec(neutral.Location(), props.CentreOfMass()).Dot(gp_Vec(pull)) > 0.0;
        const std::vector<int> own = faces_in(*half, spec.faces);
        drafted.push_back(own.empty() ? half
                                      : draft_faces(*half, own, neutral,
                                                    pull_side ? pull : pull.Reversed(),
                                                    pull_side ? spec.angle : *spec.angle2,
                                                    spec.tangent_chain));
      }
      result = drafted.front();
      for (std::size_t i = 1; i < drafted.size(); ++i) {
        result = fuse_walls(*result, *drafted[i]);
      }
      result = join_pieces(*result, body);
    }
    check_result(result, "the draft");
    return result;
  });
}

namespace {

// The least and greatest of p . up over the face's points p: the face's box
// in a frame whose z axis is `up`, from its edges' exact bounds (a planar
// face; mitcad#2382).
std::pair<double, double> extent_along(const TopoDS_Face& face, const gp_Dir& up) {
  gp_Trsf into_frame;
  into_frame.SetTransformation(gp_Ax3(gp::Origin(), up));
  Bnd_Box box;
  BRepBndLib::AddOptimal(face.Moved(TopLoc_Location(into_frame)), box, false, false);
  if (box.IsVoid()) {
    throw std::runtime_error("the face has no extent");
  }
  double xmin = 0.0, ymin = 0.0, zmin = 0.0, xmax = 0.0, ymax = 0.0, zmax = 0.0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  return {zmin, zmax};
}

// Whether the curve runs straight from its point at `first` to its point at
// `last` at a constant speed (a line, or a degree-1 Bezier or B-spline
// curve): its quarter points within `tolerance` of the segment's.
bool runs_straight(const occ::handle<Geom_Curve>& curve, double first, double last, double tolerance) {
  const gp_Pnt a = curve->Value(first);
  const gp_Pnt b = curve->Value(last);
  for (int k = 1; k < 4; ++k) {
    const double s = k / 4.0;
    const gp_Pnt expected(a.XYZ() * (1.0 - s) + b.XYZ() * s);
    if (curve->Value(first + s * (last - first)).Distance(expected) > tolerance) {
      return false;
    }
  }
  return true;
}

bool runs_straight(const occ::handle<Geom2d_Curve>& curve, double first, double last, double tolerance) {
  const gp_Pnt2d a = curve->Value(first);
  const gp_Pnt2d b = curve->Value(last);
  for (int k = 1; k < 4; ++k) {
    const double s = k / 4.0;
    const gp_Pnt2d expected(a.XY() * (1.0 - s) + b.XY() * s);
    if (curve->Value(first + s * (last - first)).Distance(expected) > tolerance) {
      return false;
    }
  }
  return true;
}

// (mitcad#2478, GitHub mitcad/Mitcad#697) The seams of a face that run
// straight but are stored as other curves given the lines they are, in 3D
// and on the face, by length, in place (the working copy's edges; the
// geometry they shared with the input is replaced, not changed). A loft
// between two coaxial circles has a cone whose seam is a degree-1 Bezier
// curve over [0, 1] in 3D and on the cone: offset, the cone's seam must be
// extended or trimmed to the caps' new edges, and the geometry kernel's
// offset came out with that seam's two sides as two edges, an open shell
// that it dropped (BRepOffset_Tool::Deboucle3D) for a null result, whatever
// the distance. The same seam as a line offsets as a primitive cone's does.
// Only where the face's parameters run at the curve's length along it (a
// cone's or a cylinder's ruling, a plane), so that one parameter serves
// both; other seams stay as they are.
void straight_seams_as_lines(const TopoDS_Face& placed) {
  // Forward: the curves read as the builder stores them, the first for the
  // edge forward (BRep_Tool::CurveOnSurface turns the edge round on a
  // reversed face, BRep_Builder::UpdateEdge does not; a tapered
  // extrusion's cone is a reversed face).
  const TopoDS_Face face = TopoDS::Face(placed.Oriented(TopAbs_FORWARD));
  constexpr double kStraight = 1.0e-9;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
  BRep_Builder builder;
  for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(edges.Current().Oriented(TopAbs_FORWARD));
    if (!seen.Add(edge) || BRep_Tool::Degenerated(edge) || !BRepTools::IsReallyClosed(edge, face)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    double first1 = 0.0;
    double last1 = 0.0;
    double first2 = 0.0;
    double last2 = 0.0;
    const occ::handle<Geom2d_Curve> forward = BRep_Tool::CurveOnSurface(edge, face, first1, last1);
    const occ::handle<Geom2d_Curve> reversed =
        BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Reversed()), face, first2, last2);
    if (curve.IsNull() || forward.IsNull() || reversed.IsNull() || !(last > first) || first1 != first ||
        last1 != last || first2 != first || last2 != last) {
      continue;
    }
    if (curve->IsKind(STANDARD_TYPE(Geom_Line)) && forward->IsKind(STANDARD_TYPE(Geom2d_Line)) &&
        reversed->IsKind(STANDARD_TYPE(Geom2d_Line))) {
      continue; // a line already
    }
    const gp_Pnt a = curve->Value(first);
    const gp_Pnt b = curve->Value(last);
    const double length = a.Distance(b);
    const double straight = kStraight * std::max(1.0, length);
    if (!(length > Precision::Confusion()) || !runs_straight(curve, first, last, straight)) {
      continue;
    }
    const gp_Pnt2d p1 = forward->Value(first);
    const gp_Pnt2d q1 = forward->Value(last);
    const gp_Pnt2d p2 = reversed->Value(first);
    const gp_Pnt2d q2 = reversed->Value(last);
    // On the face at the curve's length: the same parameter in 3D and on it.
    if (std::abs(p1.Distance(q1) - length) > straight || std::abs(p2.Distance(q2) - length) > straight ||
        !runs_straight(forward, first, last, straight) || !runs_straight(reversed, first, last, straight)) {
      continue;
    }
    const double tolerance = BRep_Tool::Tolerance(edge);
    builder.UpdateEdge(edge, new Geom_Line(a, gp_Dir(gp_Vec(a, b))), tolerance);
    builder.UpdateEdge(edge, new Geom2d_Line(p1, gp_Dir2d(gp_Vec2d(p1, q1))),
                       new Geom2d_Line(p2, gp_Dir2d(gp_Vec2d(p2, q2))), face, tolerance);
    builder.Range(edge, 0.0, length);
    builder.SameRange(edge, true);
    builder.SameParameter(edge, true);
  }
}

// (mitcad#2509, GitHub mitcad/Mitcad#697) The B-spline surface with each
// knot that repeats as often as its degree (a crease by its knots, which
// makes the surface only C0 continuous to the kernel) repeated once less
// where the surface is continuous in its first derivative there all the
// same (in its homogeneous coordinates, within 1e-9 mm, so the same
// surface with the same parameters); null when no knot changed or a crease
// stays. The sides of a loft through four or more sections are piecewise
// cubic Hermite surfaces, their inner knots three times over though the
// pieces meet with one tangent (skin.hpp, HermiteInterpolation): the
// geometry kernel's offset refuses any C0 surface of the body
// (BRepOffset_C0Geometry, in BRepOffset_MakeOffset's check of its input),
// and Offset Face on such a side said "the neighbouring faces cannot follow
// the moved faces".
occ::handle<Geom_BSplineSurface> without_false_creases(const occ::handle<Geom_BSplineSurface>& surface) {
  constexpr double kSame = 1.0e-9;
  const occ::handle<Geom_BSplineSurface> smoothed = occ::down_cast<Geom_BSplineSurface>(surface->Copy());
  bool changed = false;
  try {
    for (const bool along_u : {true, false}) {
      const int degree = along_u ? smoothed->UDegree() : smoothed->VDegree();
      const bool periodic = along_u ? smoothed->IsUPeriodic() : smoothed->IsVPeriodic();
      const int first = along_u ? smoothed->FirstUKnotIndex() : smoothed->FirstVKnotIndex();
      const int last = along_u ? smoothed->LastUKnotIndex() : smoothed->LastVKnotIndex();
      if (degree < 2) {
        continue;
      }
      // The inner knots; of a periodic surface every knot but the last,
      // which is the first one again.
      for (int k = periodic ? first : first + 1; k < last; ++k) {
        const int times = along_u ? smoothed->UMultiplicity(k) : smoothed->VMultiplicity(k);
        if (times < degree) {
          continue;
        }
        const bool lowered = along_u ? smoothed->RemoveUKnot(k, degree - 1, kSame)
                                     : smoothed->RemoveVKnot(k, degree - 1, kSame);
        changed = changed || lowered;
      }
    }
  } catch (const Standard_Failure&) {
    return occ::handle<Geom_BSplineSurface>();
  }
  if (!changed || smoothed->Continuity() == GeomAbs_C0) {
    return occ::handle<Geom_BSplineSurface>();
  }
  return smoothed;
}

// Each face of the shape on a B-spline surface that is C0 only by its knots
// put on the same surface with those knots once less (without_false_creases),
// its edges' curves on it carried over; in place (the working copy). Faces
// with a true crease stay as they are.
void lower_false_creases(const TopoDS_Shape& shape) {
  BRep_Builder builder;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> done;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face face = TopoDS::Face(it.Current().Oriented(TopAbs_FORWARD));
    if (!done.Add(face.Located(TopLoc_Location()))) {
      continue;
    }
    TopLoc_Location location;
    const occ::handle<Geom_BSplineSurface> spline =
        occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(face, location));
    if (spline.IsNull() || spline->Continuity() != GeomAbs_C0) {
      continue;
    }
    if (const occ::handle<Geom_BSplineSurface> smoothed = without_false_creases(spline)) {
      put_on_same_parameters(face, smoothed, location, builder);
    }
  }
}

// The words of a refusal of the kernel's offset of the body for a face
// whose surface has a crease inside it (BRepOffset_C0Geometry), which no
// knot removal smoothed: the faces with one as its places.
std::string creased_faces_error(const Shape& body) {
  std::string names;
  std::vector<ErrorPlace> places;
  for (int i = 0; i < body.face_count(); ++i) {
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(body.face(i));
    if (surface.IsNull() || surface->Continuity() != GeomAbs_C0) {
      continue;
    }
    const std::string name = detail::primary_name(body, i);
    if (!name.empty()) {
      names += (names.empty() ? "" : ", ") + name;
      places.push_back(face_place(name));
    }
  }
  return placed_error(
      "the neighbouring faces cannot follow the moved faces: a face of the body has a crease inside it (its "
      "surface is continuous only in position there), which the geometry kernel's offset does not take",
      "the kernel's offset refuses the body: the surface of " + (names.empty() ? std::string("a face") : names) +
          " is only C0 continuous inside the face (BRepOffset_C0Geometry)",
      places);
}

// True when the body has a face whose surface is only C0 continuous and
// every such face lies on the offset of a surface that is continuous at
// least in its first derivative (Geom_OffsetSurface counts its continuity
// one order below its basis's, so such an offset is C0 to the kernel,
// though tangent-continuous; mitcad#2546).
bool offset_creases_only(const Shape& body) {
  bool any = false;
  for (int i = 0; i < body.face_count(); ++i) {
    occ::handle<Geom_Surface> surface = BRep_Tool::Surface(body.face(i));
    if (surface.IsNull() || surface->Continuity() != GeomAbs_C0) {
      continue;
    }
    if (const occ::handle<Geom_RectangularTrimmedSurface> trimmed =
            occ::down_cast<Geom_RectangularTrimmedSurface>(surface)) {
      surface = trimmed->BasisSurface();
    }
    const occ::handle<Geom_OffsetSurface> offset = occ::down_cast<Geom_OffsetSurface>(surface);
    if (offset.IsNull() || offset->BasisSurface().IsNull() || offset->BasisSurface()->Continuity() == GeomAbs_C0) {
      return false;
    }
    any = true;
  }
  return any;
}

// Moves each face by its own offset; the other faces stay on their surfaces.
// `except` are faces not to carry the names of.
detail::FaceNamer move_faces(const Shape& input, const std::vector<std::pair<int, double>>& given,
                             const std::vector<int>& given_except, NCollection_List<TopoDS_Shape>& moved) {
  // OCCT's offset widens the tolerances of its input's sub-shapes.
  const ShapePtr copy = detail::working_copy(input);
  for (const auto& move : given) {
    straight_seams_as_lines(copy->face(move.first));
  }
  // The kernel checks every face of the body, not only the moved ones.
  lower_false_creases(copy->occt());
  // (mitcad#2546, GitHub mitcad/Mitcad#723) Faces on the offset of a
  // surface continuous only in its first derivative are split at their
  // creases into C1 pieces that keep their names, as Shell splits its input
  // (mitcad#158): the kernel counts such an offset as only C0, though it is
  // tangent-continuous, and its offset refuses the whole body for one such
  // face (BRepOffset_C0Geometry in its check of its input), moved or not.
  // The inner wall of a shelled loft whose sections were merged into one
  // smooth edge each is the offset of the loft's side, which is continuous
  // only in its first derivative along the sections' former joins (a slot's
  // lines and arcs), so moving the outer wall of such a hollow loft failed
  // with "the neighbouring faces cannot follow the moved faces". Each piece
  // is moved by its face's offset, or stays; the pieces of a face that
  // stays lie on its surface and its edges (the wall comes out in those
  // pieces, "#k", which a reference to the face still finds together). Only
  // where every face still C0 is such an offset: a B-spline face whose
  // knots leave it continuous only in position (a crease by its knots that
  // lower_false_creases could not take out) still refuses in words
  // (creased_faces_error, mitcad#2509); split, the kernel left gaps beside
  // its pieces.
  FaceOffsets offsets;
  for (const auto& [face, distance] : given) {
    offsets[face] = distance;
  }
  std::set<int> kept_unnamed(given_except.begin(), given_except.end());
  const ShapePtr pieces =
      offset_creases_only(*copy)
          ? split_at_creases(*copy, kept_unnamed, &offsets, "the faces could not be split where their surfaces crease")
          : nullptr;
  const Shape& body = pieces ? *pieces : *copy;
  std::vector<std::pair<int, double>> moves = given;
  std::vector<int> except = given_except;
  if (pieces) {
    moves.assign(offsets.begin(), offsets.end());
    except.assign(kept_unnamed.begin(), kept_unnamed.end());
  }
  BRepOffset_MakeOffset maker;
  maker.Initialize(offset_input(body.occt()), 0.0, kTolerance, BRepOffset_Skin, false, false,
                   GeomAbs_Intersection, false, false);
  for (const auto& [face, distance] : moves) {
    maker.SetOffsetOnFace(body.face(face), distance);
  }
  detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
  // The kernel's offset rebuilds every face of the body, also those far from
  // the moved ones. Where its rebuilt faces leave gaps anywhere on the body,
  // its last step drops the shell that does not close up: OCCT port patch
  // 2457 makes that an error (BRepOffset_UnknownError) where the kernel
  // reported success with no shape at all (mitcad#2457, GitHub #678: a
  // shelled block's inner roundings moved 2 mm, the gaps at a corner of
  // small blends far from them). Either is said in words; an empty result
  // is not read.
  if ((!maker.IsDone() && maker.Error() == BRepOffset_UnknownError) ||
      (maker.IsDone() && maker.Shape().IsNull())) {
    std::string names;
    std::vector<ErrorPlace> places;
    for (const auto& [face, distance] : moves) {
      (void)distance;
      const std::string name = detail::primary_name(body, face);
      names += (names.empty() ? "" : ", ") + name;
      places.push_back(face_place(name));
    }
    throw std::runtime_error(placed_error(
        "the faces cannot be moved: the body rebuilt around them does not close up (the geometry "
        "kernel's offset leaves gaps between the body's faces)",
        "the kernel's offset of the body with " + names +
            " moved gave no shape: its rebuilt faces leave free edges and the shell that does not "
            "close up was dropped",
        places));
  }
  if (!maker.IsDone() && maker.Error() == BRepOffset_C0Geometry) {
    throw std::runtime_error(creased_faces_error(body));
  }
  if (!maker.IsDone()) {
    throw std::runtime_error("the neighbouring faces cannot follow the moved faces");
  }
  const TopoDS_Shape result = single_solid(maker.Shape(), "the offset");
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(body.occt());
  const BRepTools_History history(arguments, maker);
  detail::FaceNamer namer(result);
  namer.carry(history, body, except);
  for (int i = 0; i < body.face_count(); ++i) {
    const bool skip = std::find(except.begin(), except.end(), i) != except.end();
    const bool is_moved = std::any_of(moves.begin(), moves.end(),
                                      [i](const auto& move) { return move.first == i; });
    std::vector<TopoDS_Shape> images;
    for (const TopoDS_Shape& image : history.Generated(body.face(i))) {
      if (image.ShapeType() == TopAbs_FACE) {
        images.push_back(image);
      }
    }
    for (const TopoDS_Shape& image : history.Modified(body.face(i))) {
      if (image.ShapeType() == TopAbs_FACE) {
        images.push_back(image);
      }
    }
    if (images.empty() && !history.IsRemoved(body.face(i)) && namer.contains(body.face(i))) {
      images.push_back(body.face(i));
    }
    for (const TopoDS_Shape& image : images) {
      if (is_moved) {
        moved.Append(image);
      }
      if (!skip && !namer.named(image)) {
        add_names(namer, image, body.face_names(i));
      }
    }
  }
  return namer;
}

// Offset faces across seams (mitcad#1928, mitcad#1947): a moved face that
// continues smoothly into a face that stays, as the pieces of a split face
// do, cannot be moved by the geometry kernel's offset of the body: the
// neighbour that stays on the moved face's surface (or one tangent to it)
// cannot extend or shorten along its own surface to meet it. There only the
// chosen faces move, all by one path, flat or curved: the chosen faces that
// share edges are thickened together by the distance along their normals
// (into the body for an inward offset, out of it for an outward one). Where
// two of them meet, their offsets are extended or trimmed to each other, so
// convex and concave corners at any angle close; their free sides become
// walls along their normals. Each such cell is cut from the body (inward)
// or fused to it (outward): a seam becomes a step. Where a moved face meets
// a face that stays at a sharp edge that is not square to it, the cell's
// wall there is then removed, so that the face that stays extends or trims
// along its own surface, at any angle, as for whole faces (mitcad#1973); so
// a pocket, hole or boss inside a moved face keeps its walls on their
// surfaces and gets shallower or deeper by the distance.

// An edge where a moved face continues smoothly into a face that stays.
struct Seam {
  TopoDS_Edge edge;
  int face = -1;             // the moved face
};

std::vector<Seam> seams_of(const Shape& body, const std::vector<int>& moved) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  std::vector<Seam> seams;
  ShapeMap seen;
  for (int f : moved) {
    const TopoDS_Face& face = body.face(f);
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge) || !ancestors.Contains(edge) || seen.Contains(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : ancestors.FindFromKey(edge)) {
        const int g = body.face_index(other);
        if (g >= 0 && std::find(moved.begin(), moved.end(), g) == moved.end() &&
            detail::is_smooth(edge, face, TopoDS::Face(other))) {
          seams.push_back({edge, f});
          seen.Add(edge);
          break;
        }
      }
    }
  }
  return seams;
}

// The moved faces in groups of faces that share edges, each in the order
// they were chosen.
std::vector<std::vector<int>> sharing_edges(const Shape& body, const std::vector<int>& moved) {
  std::vector<std::size_t> parent(moved.size());
  for (std::size_t i = 0; i < moved.size(); ++i) {
    parent[i] = i;
  }
  const auto root = [&parent](std::size_t i) {
    while (parent[i] != i) {
      i = parent[i] = parent[parent[i]];
    }
    return i;
  };
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  for (std::size_t i = 0; i < moved.size(); ++i) {
    for (TopExp_Explorer it(body.face(moved[i]), TopAbs_EDGE); it.More(); it.Next()) {
      if (!ancestors.Contains(it.Current())) {
        continue;
      }
      for (const TopoDS_Shape& other : ancestors.FindFromKey(it.Current())) {
        const auto at = std::find(moved.begin(), moved.end(), body.face_index(other));
        if (at != moved.end()) {
          parent[root(static_cast<std::size_t>(at - moved.begin()))] = root(i);
        }
      }
    }
  }
  std::vector<std::vector<int>> groups;
  std::vector<std::size_t> roots;
  for (std::size_t i = 0; i < moved.size(); ++i) {
    const std::size_t r = root(i);
    const auto at = std::find(roots.begin(), roots.end(), r);
    if (at == roots.end()) {
      roots.push_back(r);
      groups.push_back({moved[i]});
    } else {
      groups[static_cast<std::size_t>(at - roots.begin())].push_back(moved[i]);
    }
  }
  return groups;
}

// A point inside the face, if one is found.
std::optional<gp_Pnt> point_inside(const TopoDS_Face& face) {
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
  const BRepAdaptor_Surface surface(face);
  for (const int cells : {5, 24}) {
    for (int a = 0; a < cells; ++a) {
      for (int b = 0; b < cells; ++b) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * (a + 0.5) / cells, v0 + (v1 - v0) * (b + 0.5) / cells);
        if (classifier.Perform(uv) == TopAbs_IN) {
          return surface.Value(uv.X(), uv.Y());
        }
      }
    }
  }
  return std::nullopt;
}

// True when the point lies on the face (within its tolerance, at least
// 1e-5 mm).
bool on_face(const gp_Pnt& point, const TopoDS_Face& face) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), face);
  return distance.IsDone() && distance.Value() <= std::max(1.0e-5, BRep_Tool::Tolerance(face));
}

// The direction from the middle of the edge into the face, along the face
// (to a point of the face a short way off the edge); empty when none is
// found.
std::optional<gp_Vec> into_face(const TopoDS_Face& face, const TopoDS_Edge& edge) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    return std::nullopt;
  }
  gp_Pnt2d uv;
  gp_Vec2d along;
  pcurve->D1((first + last) / 2.0, uv, along);
  if (along.Magnitude() < gp::Resolution()) {
    return std::nullopt;
  }
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const gp_Vec2d across = gp_Vec2d(-along.Y(), along.X()).Normalized() * (1.0e-4 * std::max(u1 - u0, v1 - v0));
  BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
  const BRepAdaptor_Surface surface(face);
  const gp_Pnt at = surface.Value(uv.X(), uv.Y());
  for (const double side : {1.0, -1.0}) {
    const gp_Pnt2d probe = uv.Translated(across * side);
    if (classifier.Perform(probe) == TopAbs_IN) {
      return gp_Vec(at, surface.Value(probe.X(), probe.Y()));
    }
  }
  return std::nullopt;
}

// Where along an edge the angle between its faces is looked at, and the
// angle (radians) up to which they meet tangentially there (as
// detail::is_smooth takes it).
constexpr std::array<double, 5> kSharpSamples{0.0, 0.25, 0.5, 0.75, 1.0};
constexpr double kTangentAngle = 1.0e-3;

// Points inside the edges where the moved faces meet faces that stay at an
// angle, at a quarter, the middle and three quarters of each, with the face
// that stays there: the sides whose walls are removed so that the face that
// stays extends or trims along its own surface. The walls run from the
// edges along the moved faces' normals, out of the body for a positive
// `distance` and into it for a negative one.
//
// Where the two faces turn tangent somewhere along their edge (an end of a
// mitre between roundings), the face that stays can be trimmed there but not
// extended: at the tangent point it turns tangent to the moved face, and
// past it to its own neighbours (the faces the rounding blends), so it has
// no surface to extend along towards the moved face's new position. So
// such a side is taken only where the face that stays, followed from the
// edge into itself, comes nearer to the moved face's new position (it
// curves towards it, as a convex rounding beside a rounding moved in does):
// there it is trimmed back along its surface to that position, and the
// step along the moved face's tangent edge closes against it at the tangent
// point. Where it would have to extend past the edge to reach it, the side
// stays a step along the moved face's normal (mitcad#1974).
std::vector<std::pair<gp_Pnt, int>> points_on_sharp_edges(const Shape& body, const std::vector<int>& moved,
                                                          double distance) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  std::vector<std::pair<gp_Pnt, int>> points;
  for (int f : moved) {
    const TopoDS_Face& face = body.face(f);
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge) || !ancestors.Contains(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : ancestors.FindFromKey(edge)) {
        const int g = body.face_index(other);
        if (g < 0 || std::find(moved.begin(), moved.end(), g) != moved.end() ||
            detail::is_smooth(edge, face, TopoDS::Face(other))) {
          continue;
        }
        const TopoDS_Face& kept = TopoDS::Face(other);
        const bool turns_tangent = std::any_of(kSharpSamples.begin(), kSharpSamples.end(), [&](double t) {
          return detail::normal_angle(edge, face, kept, t) <= kTangentAngle;
        });
        if (turns_tangent) {
          // The moved face's new position lies `distance` along its normal;
          // the face that stays comes nearer to it where it runs that way.
          const std::optional<gp_Vec> into = into_face(kept, edge);
          const double towards = into ? into->Dot(gp_Vec(detail::normal_on_edge(face, edge, 0.5))) : 0.0;
          if (towards == 0.0 || (towards < 0.0) != (distance < 0.0)) {
            break;
          }
        }
        const BRepAdaptor_Curve curve(edge);
        for (const double t : {0.25, 0.5, 0.75}) {
          points.emplace_back(curve.Value(curve.FirstParameter() + t * (curve.LastParameter() - curve.FirstParameter())), g);
        }
        break;
      }
    }
  }
  return points;
}

// The places of the faces, for an error, and their names for its detail.
std::vector<ErrorPlace> places_of(const Shape& body, const std::vector<int>& faces) {
  std::vector<ErrorPlace> places;
  for (int f : faces) {
    places.push_back(face_place(detail::primary_name(body, f)));
  }
  return places;
}

std::string names_of(const Shape& body, const std::vector<int>& faces) {
  std::string names;
  for (int f : faces) {
    names += (names.empty() ? "" : ", ") + detail::primary_name(body, f);
  }
  return names;
}

// The radius a curved face would have after its offset by `distance`
// (along its normal out of the material), when the offset shrinks it:
// a cylinder's or a sphere's radius, a torus' minor one. Empty for other
// surfaces and for an offset that makes the face larger.
std::optional<double> shrunk_radius(const TopoDS_Face& face, double distance) {
  const BRepAdaptor_Surface surface(face);
  double radius = 0.0;
  gp_Pnt centre;
  gp_Ax1 axis;
  bool on_axis = false;
  switch (surface.GetType()) {
  case GeomAbs_Cylinder:
    radius = surface.Cylinder().Radius();
    axis = surface.Cylinder().Axis();
    on_axis = true;
    break;
  case GeomAbs_Sphere:
    radius = surface.Sphere().Radius();
    centre = surface.Sphere().Location();
    break;
  case GeomAbs_Torus: {
    radius = surface.Torus().MinorRadius();
    break;
  }
  default:
    return std::nullopt;
  }
  const double u = (surface.FirstUParameter() + surface.LastUParameter()) / 2.0;
  const double v = (surface.FirstVParameter() + surface.LastVParameter()) / 2.0;
  gp_Pnt point;
  gp_Vec du, dv;
  surface.D1(u, v, point, du, dv);
  gp_Vec normal = du.Crossed(dv);
  if (normal.Magnitude() < Precision::Confusion()) {
    return std::nullopt;
  }
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  // The direction from the centre of curvature to the point.
  gp_Vec outward;
  if (surface.GetType() == GeomAbs_Torus) {
    const gp_Torus torus = surface.Torus();
    const gp_Ax1 main = torus.Axis();
    const gp_Vec along(main.Direction());
    gp_Vec radial(main.Location(), point);
    radial -= along * radial.Dot(along);
    if (radial.Magnitude() < Precision::Confusion()) {
      return std::nullopt;
    }
    const gp_Pnt tube = main.Location().Translated(radial.Normalized() * torus.MajorRadius());
    outward = gp_Vec(tube, point);
  } else if (on_axis) {
    const gp_Vec along(axis.Direction());
    outward = gp_Vec(axis.Location(), point);
    outward -= along * outward.Dot(along);
  } else {
    outward = gp_Vec(centre, point);
  }
  if (outward.Magnitude() < Precision::Confusion()) {
    return std::nullopt;
  }
  const double after = radius + (normal.Dot(outward) > 0.0 ? distance : -distance);
  if (after >= radius) {
    return std::nullopt;
  }
  return after;
}

// Why the thickening of moved faces failed, in words.
std::string thickening_failure(const BRepOffset_MakeOffset& offset) {
  switch (offset.Error()) {
  case BRepOffset_BadNormalsOnGeometry:
    return "a moved face's normals cannot be computed everywhere";
  case BRepOffset_C0Geometry:
    return "a moved face's surface has a crease";
  case BRepOffset_MixedConnectivity:
    return "the moved faces meet partly smoothly and partly at an angle along an edge";
  case BRepOffset_CannotTrimEdges:
  case BRepOffset_CannotExtentEdge:
  case BRepOffset_CannotFuseVertices:
    return "the moved faces' new positions cannot be extended or trimmed to meet each other";
  default:
    return "the moved faces' new positions do not close up at this distance (a face narrower than "
           "the distance may vanish)";
  }
}

// The cell of one group of moved faces: the faces (of `copy`) thickened by
// the distance, a solid of positive volume, with the names of the faces
// on their offsets (for the result's faces that lie on them).
TopoDS_Shape cell_of(const Shape& body, const detail::InputCopy& copy, const std::vector<int>& group,
                     double distance, std::vector<Shape::NamedFace>& offsets) {
  for (int f : group) {
    if (const std::optional<double> radius = shrunk_radius(body.face(f), distance)) {
      if (*radius <= kTolerance) {
        throw std::runtime_error(placed_error(
            "a curved face would shrink to nothing: the distance is as large as its radius or larger",
            "offset of " + detail::primary_name(body, f) + " by " + std::to_string(distance) +
                " leaves a radius of " + std::to_string(*radius),
            {face_place(detail::primary_name(body, f))}));
      }
    }
  }
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (int f : group) {
    builder.Add(shell, copy.of(body.face(f)));
  }
  BRepOffset_MakeOffset maker;
  maker.Initialize(shell, distance, kTolerance, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                   true, false);
  detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
  TopoDS_Shape solid;
  if (maker.IsDone()) {
    TopExp_Explorer it(maker.Shape(), TopAbs_SOLID);
    if (it.More()) {
      solid = it.Current();
      it.Next();
      if (it.More()) {
        solid.Nullify();
      }
    }
  }
  if (solid.IsNull()) {
    throw std::runtime_error(placed_error(
        "the faces cannot be moved by this distance: " + thickening_failure(maker),
        "thickening " + names_of(body, group) + " by " + std::to_string(distance) +
            " failed (offset error " + std::to_string(static_cast<int>(maker.Error())) + ")",
        places_of(body, group)));
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(solid, props);
  if (props.Mass() < 0.0) {
    solid.Reverse();
  }
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(shell);
  const BRepTools_History history(arguments, maker);
  for (int f : group) {
    const TopoDS_Shape face = copy.of(body.face(f));
    for (const TopoDS_Shape& image : history.Modified(face)) {
      if (image.ShapeType() == TopAbs_FACE && !image.IsSame(face)) {
        offsets.push_back({image, body.face_names(f)});
      }
    }
    for (const TopoDS_Shape& image : history.Generated(face)) {
      if (image.ShapeType() == TopAbs_FACE && !image.IsSame(face)) {
        offsets.push_back({image, body.face_names(f)});
      }
    }
  }
  return solid;
}

ShapePtr offset_across_seams(const std::string& feature, const Shape& body,
                             const std::vector<int>& chosen, const std::vector<Seam>& seams,
                             double distance) {
  const detail::InputCopy copy(body.occt());
  std::vector<TopoDS_Shape> cells;
  std::vector<Shape::NamedFace> offsets;
  for (const std::vector<int>& group : sharing_edges(body, chosen)) {
    cells.push_back(cell_of(body, copy, group, distance, offsets));
  }
  BRep_Builder builder;
  TopoDS_Compound together;
  builder.MakeCompound(together);
  for (const TopoDS_Shape& cell : cells) {
    builder.Add(together, cell);
  }
  const Shape offset(together, offsets);
  // The body with the cells removed (inward) or added (outward), on the copy
  // whose faces the cells share.
  const bool inward = distance < 0.0;
  BRepAlgoAPI_Cut cut;
  BRepAlgoAPI_Fuse fuse;
  BRepAlgoAPI_BooleanOperation& operation = inward ? static_cast<BRepAlgoAPI_BooleanOperation&>(cut)
                                                   : static_cast<BRepAlgoAPI_BooleanOperation&>(fuse);
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(copy.shape());
  NCollection_List<TopoDS_Shape> tools;
  for (const TopoDS_Shape& cell : cells) {
    tools.Append(cell);
  }
  operation.SetArguments(arguments);
  operation.SetTools(tools);
  operation.SetNonDestructive(true);
  detail::build(operation);
  if (!operation.IsDone() || operation.HasErrors()) {
    throw std::runtime_error(placed_error(
        inward ? "the moved faces' new positions could not be cut into the body"
               : "the moved faces' new positions could not be joined to the body",
        std::string(inward ? "cut" : "fuse") + " of the thickened faces " + names_of(body, chosen) +
            " failed",
        places_of(body, chosen)));
  }
  operation.SimplifyResult();
  detail::FaceNamer namer(operation.Shape());
  namer.carry(operation, body, copy);
  // The faces without a name take those of the moved face whose offset they
  // lie on; the rest are the steps, named after the moved face whose seam
  // is nearest, <feature>:step(<face>).
  namer.carry_by_containment(offset, 1.0e-6);
  // A side of a moved face at a sharp edge to a face that stays is a wall
  // along the moved face's normal in the cell; where that face is not square
  // to the moved one, the wall is a face of the result of its own, which
  // starts at the edge. It is removed below, so that the face that stays
  // extends or trims along its own surface to the moved face's new position
  // (mitcad#1973).
  // A wall that lies on the surface of the face that stays (a face square to
  // the moved one, whose wall the kernel did not merge with it) is that
  // face's continuation already and stays.
  const std::vector<std::pair<gp_Pnt, int>> sharp = points_on_sharp_edges(body, chosen, distance);
  std::vector<TopoDS_Shape> walls;
  for (TopExp_Explorer it(namer.result(), TopAbs_FACE); it.More(); it.Next()) {
    if (namer.named(it.Current())) {
      continue;
    }
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (std::any_of(sharp.begin(), sharp.end(), [&face, &body](const std::pair<gp_Pnt, int>& at) {
          return on_face(at.first, face) && !on_surface(face, body.face(at.second));
        })) {
      walls.push_back(face);
      namer.add(face, face_name(feature, "wall", detail::primary_name(body, chosen.front())));
      continue;
    }
    const std::optional<gp_Pnt> point = point_inside(TopoDS::Face(it.Current()));
    const Seam* nearest = &seams.front();
    if (point) {
      double best = -1.0;
      for (const Seam& seam : seams) {
        BRepExtrema_DistShapeShape gap(BRepBuilderAPI_MakeVertex(*point).Vertex(), seam.edge);
        if (gap.IsDone() && (best < 0.0 || gap.Value() < best)) {
          best = gap.Value();
          nearest = &seam;
        }
      }
    }
    namer.add(it.Current(), face_name(feature, "step", detail::primary_name(body, nearest->face)));
  }
  namer.finish();
  ShapePtr moved = namer.shape();
  // The walls removed: the faces around them (the face that stays, the
  // moved face's new position, the steps at its ends) extend or trim along
  // their own surfaces until they close the gap, as when whole faces move.
  // One group of walls that share edges at a time, each on the result of the
  // one before: the kernel fills the gaps of all the groups it is given on
  // the shape before any of them is removed, so where two groups border the
  // same face (the moved face's new position, between the walls at its two
  // ends) the second gap no longer fits that face once the first is closed
  // (mitcad#1974).
  std::vector<std::vector<TopoDS_Shape>> groups;
  for (const TopoDS_Shape& wall : walls) {
    ShapeMap edges;
    TopExp::MapShapes(wall, TopAbs_EDGE, edges);
    std::vector<TopoDS_Shape> group{wall};
    for (auto it = groups.begin(); it != groups.end();) {
      const bool shares = std::any_of(it->begin(), it->end(), [&edges](const TopoDS_Shape& other) {
        for (TopExp_Explorer e(other, TopAbs_EDGE); e.More(); e.Next()) {
          if (edges.Contains(e.Current())) {
            return true;
          }
        }
        return false;
      });
      if (shares) {
        group.insert(group.end(), it->begin(), it->end());
        it = groups.erase(it);
      } else {
        ++it;
      }
    }
    groups.push_back(group);
  }
  for (std::size_t i = 0; i < groups.size(); ++i) {
    if (groups[i].empty()) {
      continue; // removed with an earlier group
    }
    BRepAlgoAPI_Defeaturing remover;
    remover.SetShape(moved->occt());
    for (const TopoDS_Shape& wall : groups[i]) {
      remover.AddFaceToRemove(wall);
    }
    remover.SetRunParallel(false);
    remover.SetToFillHistory(true);
    detail::build(remover);
    if (!remover.IsDone() || remover.HasErrors() ||
        remover.HasWarning(STANDARD_TYPE(BOPAlgo_AlertUnableToRemoveTheFeature)) ||
        std::any_of(groups[i].begin(), groups[i].end(),
                    [&remover](const TopoDS_Shape& wall) { return !remover.IsDeleted(wall); })) {
      throw std::runtime_error(placed_error(
          "the faces around the moved faces cannot be extended or trimmed along their surfaces to meet "
          "their new positions",
          "removing the walls at the sharp edges of " + names_of(body, chosen) + " failed",
          places_of(body, chosen)));
    }
    // The walls still to remove, as they are in this result.
    for (std::size_t j = i + 1; j < groups.size(); ++j) {
      std::vector<TopoDS_Shape> images;
      for (const TopoDS_Shape& wall : groups[j]) {
        if (remover.IsDeleted(wall)) {
          continue;
        }
        const NCollection_List<TopoDS_Shape>& modified = remover.Modified(wall);
        if (modified.IsEmpty()) {
          images.push_back(wall);
        }
        for (const TopoDS_Shape& image : modified) {
          images.push_back(image);
        }
      }
      groups[j] = images;
    }
    detail::FaceNamer extended(remover.Shape());
    extended.carry(remover, *moved);
    extended.finish();
    moved = extended.shape();
  }
  return moved;
}

// True when the planar face, moved `distance` along its normal out of the
// material (negative: into it), reaches the body's surface on the far side
// (mitcad#2381): the region it sweeps, continued a little past where it
// stops, does not lie inside the body. Moved there by the offset, the face
// would stay as a face of no thickness on that surface (a pocket's floor
// onto the outside face under it caps the opening) or pass through the
// body; the region between the faces and the target is removed instead
// (replace_by_slab), which opens the wall between.
bool reaches_far_side(const Shape& body, int face, double distance) {
  if (!(distance < -kTolerance)) {
    return false;
  }
  const std::optional<gp_Pln> plane = detail::face_plane(body.face(face));
  if (!plane) {
    return false;
  }
  // Far enough past the target that a wall the target lands on shows as a
  // part of the swept region outside the body (at least 1e-3 of it).
  const double past = std::max(0.01, 1.0e-3 * -distance);
  // On a copy of the face: the body stays as it is.
  BRepPrimAPI_MakePrism sweep(body.face(face), gp_Vec(plane->Axis().Direction()) * (distance - past),
                              true);
  if (!sweep.IsDone()) {
    return false;
  }
  const double swept = std::abs(detail::volume_of(sweep.Shape()));
  if (!(swept > 0.0)) {
    return false;
  }
  BRepAlgoAPI_Common common;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(body.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(sweep.Shape());
  common.SetArguments(arguments);
  common.SetTools(tools);
  common.SetNonDestructive(true);
  detail::build(common);
  if (!common.IsDone() || common.HasErrors()) {
    // Not measured: the faces move by the offset, as they did before this
    // check.
    return false;
  }
  const double inside = std::abs(detail::volume_of(common.Shape()));
  return swept - inside > 1.0e-6 * swept;
}

// How far a face beside a moved one may reach past the moved face's new
// place and still count as passed over (passes_over_neighbour): the
// rounding of positions the earlier features computed, far below any wall
// a design has.
constexpr double kPassedOver = 1.0e-4;

// True when a planar face, moved its distance along its normal, passes over
// a face beside it (mitcad#2541, GitHub #717): a face that stays, shares an
// edge with the moved face and lies wholly between the face's place and
// its new one along the normal. Such a face shrinks to nothing (the 3 mm
// wall between a shell's inner ceiling and the rim below it when either
// goes onto the other's plane, a pocket's walls when its floor goes onto
// the top) or would have to turn inside out. The geometry kernel's offset
// extends and shortens the faces beside the moved ones along their
// surfaces but never removes one: on the reported shelled body it stopped
// with an internal lookup failure ("NCollection_DataMap::Find"). The region
// between the faces and the target is added or removed instead
// (replace_by_slab), which takes the face passed over with it.
bool passes_over_neighbour(const Shape& body, const std::vector<std::pair<int, double>>& moves) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  const auto is_moved = [&moves](int g) {
    return std::any_of(moves.begin(), moves.end(), [g](const auto& move) { return move.first == g; });
  };
  for (const auto& move : moves) {
    const int face = move.first;
    const double distance = move.second;
    const std::optional<gp_Pln> plane = detail::face_plane(body.face(face));
    if (!plane || !(std::abs(distance) > kPassedOver)) {
      continue;
    }
    const gp_Dir normal = plane->Axis().Direction();
    const double level = gp_Vec(plane->Location().XYZ()).Dot(gp_Vec(normal));
    // The span the face sweeps along its normal.
    const double low = std::min(level, level + distance) - kPassedOver;
    const double high = std::max(level, level + distance) + kPassedOver;
    for (TopExp_Explorer it(body.face(face), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge) || !ancestors.Contains(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : ancestors.FindFromKey(edge)) {
        const int g = body.face_index(other);
        if (g < 0 || is_moved(g)) {
          continue;
        }
        const std::pair<double, double> extent = extent_along(TopoDS::Face(other), normal);
        // A wall (not a face in the moved face's plane) within the span.
        if (extent.second - extent.first > kPassedOver && extent.first >= low && extent.second <= high) {
          return true;
        }
      }
    }
  }
  return false;
}

} // namespace

ShapePtr offset_faces(const std::string& feature, const Shape& body,
                      const std::vector<std::string>& faces, double distance,
                      bool tangent_chain) {
  detail::require_finite("offset distance", distance);
  if (distance == 0.0) {
    throw std::invalid_argument("the offset distance must not be zero");
  }
  detail::require_surfaces(body.occt(), "offset faces");
  std::vector<int> chosen = detail::resolve_faces(body, faces, "faces to offset");
  if (tangent_chain) {
    // As Replace Face's: the faces the hover's chain shows (mitcad#2438).
    chosen = detail::tangent_closure(body, chosen);
  }
  return detail::run("offset faces", [&] {
    // Where a moved face continues smoothly into a face that stays (the
    // pieces of a split face, a face tangent to a rounding that stays), the
    // geometry kernel's offset of the body cannot move it: it gives faces
    // tangent to each other one distance, so beside a piece of the same
    // surface it would move the pieces that stay as well. There the seam
    // becomes a step (mitcad#1928, mitcad#1947, mitcad#1973).
    const std::vector<Seam> seams = seams_of(body, chosen);
    if (!seams.empty()) {
      ShapePtr result = offset_across_seams(feature, body, chosen, seams, distance);
      check_result(result, "the offset");
      return result;
    }
    // Planar faces in one plane (mitcad#2548): onto the level of another
    // planar face of the body where they end within detail::kPlanarLevel
    // of it, and across the body's other faces where the offset of the
    // body cannot follow (a raised block's end offset flush with the side
    // of the base under it, or past it), as Move Face's.
    std::optional<gp_Pln> plane;
    bool lands = false;
    double exact = distance;
    if (const std::optional<gp_Pln> own = detail::face_plane(body.face(chosen.front()))) {
      const gp_Vec outward(own->Axis().Direction());
      plane = detail::movable_plane(body, chosen, outward * distance);
      if (plane) {
        if (const std::optional<gp_Vec> landed = detail::landing_motion(body, chosen, *plane, outward * distance)) {
          exact = landed->Dot(outward);
          lands = true;
        }
      }
    }
    const auto own = [&] {
      std::vector<std::pair<int, double>> moves;
      for (int f : chosen) {
        moves.emplace_back(f, exact);
      }
      NCollection_List<TopoDS_Shape> moved;
      detail::FaceNamer namer = move_faces(body, moves, {}, moved);
      namer.finish();
      ShapePtr result = namer.shape();
      check_result(result, "the offset");
      return result;
    };
    if (!plane) {
      return own();
    }
    // The offset of the body keeps its topology; where the faces'
    // boundaries meet or cross other faces, the region they sweep is added
    // or removed and the faces around are trimmed where they meet. Faces
    // that land on another face's level may join it: there that comes
    // first. Where neither works, the offset's own reason.
    const auto across = [&] {
      ShapePtr result = detail::move_planar_faces(body, chosen, *plane,
                                                  gp_Vec(plane->Axis().Direction()) * exact, feature);
      check_result(result, "the offset");
      return result;
    };
    return lands ? detail::first_of(across, own, 1) : detail::first_of(own, across, 0);
  });
}

ShapePtr delete_faces(const std::string& feature, const Shape& body,
                      const std::vector<std::string>& faces) {
  (void)feature;
  detail::require_surfaces(body.occt(), "delete faces");
  const std::vector<int> chosen = detail::resolve_faces(body, faces, "faces to delete");
  return detail::run("delete faces", [&] {
    BRepAlgoAPI_Defeaturing maker;
    maker.SetShape(body.occt());
    for (int f : chosen) {
      maker.AddFaceToRemove(body.face(f));
    }
    maker.SetRunParallel(false);
    maker.SetToFillHistory(true);
    detail::build(maker);
    if (!maker.IsDone() || maker.HasErrors() ||
        maker.HasWarning(STANDARD_TYPE(BOPAlgo_AlertUnableToRemoveTheFeature))) {
      throw std::runtime_error(
          "the faces cannot be deleted: the neighbouring faces do not close the gap");
    }
    for (int f : chosen) {
      if (!maker.IsDeleted(body.face(f))) {
        const std::string name = detail::primary_name(body, f);
        throw std::runtime_error(face_error(name, "a face could not be deleted", "face " + name + " could not be deleted"));
      }
    }
    detail::FaceNamer namer(maker.Shape());
    namer.carry(maker, body);
    namer.finish();
    ShapePtr result = namer.shape();
    check_result(result, "deleting faces");
    return result;
  });
}

ShapePtr replace_faces(const std::string& feature, const Shape& body,
                       const std::vector<std::string>& faces, const Tool& target,
                       bool tangent_chain) {
  if (target.kind == Tool::Kind::Curves) {
    throw std::invalid_argument("the target must be a plane, a face or a body");
  }
  detail::require_surfaces(body.occt(), "replace faces");
  if (target.shape) {
    detail::require_surfaces(target.shape->occt(), "the target");
  }
  std::vector<int> chosen = detail::resolve_faces(body, faces, "faces to replace");
  if (tangent_chain) {
    chosen = detail::tangent_closure(body, chosen);
  }
  // Planar faces onto a plane or a planar face move there; other faces and
  // targets take the region between them (replace.cpp).
  bool planar = target.kind != Tool::Kind::Body &&
                std::all_of(chosen.begin(), chosen.end(),
                            [&](int f) { return detail::face_plane(body.face(f)).has_value(); });
  if (planar && target.kind == Tool::Kind::Face) {
    if (!target.shape) {
      throw std::invalid_argument("the target face has no body");
    }
    const std::vector<int> found =
        detail::resolve_faces(*target.shape, {target.face}, "target faces");
    planar = detail::face_plane(target.shape->face(found.front())).has_value();
  }
  if (!planar) {
    std::string replaced;
    for (int f : chosen) {
      replaced = replaced.empty() ? detail::primary_name(body, f) : replaced;
    }
    return detail::run("replace faces", [&] {
      ShapePtr result =
          detail::replace_by_slab(body, chosen, target, face_name(feature, "replace", replaced));
      check_result(result, "replacing faces");
      return result;
    });
  }
  const gp_Pln plane = detail::tool_plane(target, "the target face");
  const gp_Dir up = plane.Axis().Direction();
  return detail::run("replace faces", [&] {
    // Move each face onto the target where it is parallel; past the target
    // where it is not, and cut back to it afterwards.
    std::vector<std::pair<int, double>> moves;
    bool tilted = false;
    for (int f : chosen) {
      const gp_Dir normal = detail::face_plane(body.face(f))->Axis().Direction();
      const double facing = normal.Dot(up);
      if (std::abs(facing) < 1.0e-3) {
        const std::string name = detail::primary_name(body, f);
        throw std::invalid_argument(
            face_error(name, "the target is perpendicular to the face", "the target is perpendicular to face " + name));
      }
      // Where the face's normal line through each vertex meets the target.
      double nearest = 0.0;
      double farthest = 0.0;
      bool first = true;
      const auto reach = [&](double t) {
        nearest = first ? t : std::min(nearest, t);
        farthest = first ? t : std::max(farthest, t);
        first = false;
      };
      for (TopExp_Explorer it(body.face(f), TopAbs_VERTEX); it.More(); it.Next()) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(it.Current()));
        reach(gp_Vec(p, plane.Location()).Dot(gp_Vec(up)) / facing);
      }
      if (std::abs(1.0 - std::abs(facing)) > 1.0e-9) {
        // Tilted, the distance changes across the face, and on a curved
        // edge it is greatest between the vertices: a round top has one
        // vertex, and moved past the target there only, it still crossed
        // the target and every piece held it (mitcad#2382). So the face's
        // own extent along the target's normal, its edges' exact bounds.
        const auto [low, high] = extent_along(body.face(f), up);
        const double level = gp_Vec(plane.Location().XYZ()).Dot(gp_Vec(up));
        reach((level - low) / facing);
        reach((level - high) / facing);
        tilted = true;
        // Past the target everywhere on the face.
        moves.emplace_back(f, farthest + std::max(1.0, farthest - nearest));
      } else {
        moves.emplace_back(f, farthest);
      }
    }
    std::string replaced;
    for (int f : chosen) {
      replaced = replaced.empty() ? detail::primary_name(body, f) : replaced;
    }
    const std::string name = face_name(feature, "replace", replaced);
    // A face moved into the material up to the body's far side or past it
    // (a pocket's floor onto the outside face under it, or beyond): the
    // region between the faces and the target is removed, so the wall
    // between goes and the opening is open (mitcad#2381). The same region
    // where a face moves over a face beside it, which shrinks to nothing
    // and goes with the region (mitcad#2541).
    if (!tilted && (passes_over_neighbour(body, moves) ||
                    std::any_of(moves.begin(), moves.end(), [&](const auto& move) {
                      return reaches_far_side(body, move.first, move.second);
                    }))) {
      ShapePtr through = detail::replace_by_slab(body, chosen, target, name);
      check_result(through, "replacing faces");
      return through;
    }
    NCollection_List<TopoDS_Shape> moved;
    ShapePtr result;
    const bool still = std::all_of(moves.begin(), moves.end(), [](const auto& move) {
      return std::abs(move.second) <= kTolerance;
    });
    if (still && !tilted) {
      // Already on the target: only the names change.
      std::vector<Shape::NamedFace> named;
      for (int i = 0; i < body.face_count(); ++i) {
        const bool source = std::find(chosen.begin(), chosen.end(), i) != chosen.end();
        named.push_back({body.face(i), source ? NameList{name} : body.face_names(i)});
      }
      result = std::make_shared<Shape>(body.occt(), named);
    } else if (!tilted) {
      detail::FaceNamer namer = move_faces(body, moves, chosen, moved);
      for (const TopoDS_Shape& face : moved) {
        namer.add(face, name);
      }
      namer.finish();
      result = namer.shape();
    } else {
      // Grown past the target, then split at it; the pieces past it that hold
      // a moved face go, the rest join again.
      ShapePtr grown;
      if (still) {
        // Already past the target everywhere.
        grown = std::make_shared<Shape>(body);
        for (int f : chosen) {
          moved.Append(body.face(f));
        }
      } else {
        detail::FaceNamer grown_namer = move_faces(body, moves, chosen, moved);
        grown_namer.finish();
        grown = grown_namer.shape();
      }
      const TopoDS_Face cut = detail::plane_face(plane, detail::box_of(grown->occt()));
      BRepAlgoAPI_Splitter splitter;
      NCollection_List<TopoDS_Shape> arguments;
      arguments.Append(grown->occt());
      NCollection_List<TopoDS_Shape> tools;
      tools.Append(cut);
      splitter.SetArguments(arguments);
      splitter.SetTools(tools);
      splitter.SetNonDestructive(true);
      detail::build(splitter);
      if (!splitter.IsDone() || splitter.HasErrors()) {
        throw std::runtime_error("the body could not be cut at the target");
      }
      ShapeMap moved_images;
      for (const TopoDS_Shape& face : moved) {
        for (const TopoDS_Shape& image : images_of(splitter, face, splitter.Shape())) {
          moved_images.Add(image);
        }
      }
      detail::FaceNamer split_namer(splitter.Shape());
      split_namer.carry(splitter, *grown);
      for (const TopoDS_Shape& image : splitter.Modified(cut)) {
        split_namer.add(image, name);
      }
      split_namer.finish();
      std::vector<ShapePtr> kept;
      for (TopExp_Explorer it(splitter.Shape(), TopAbs_SOLID); it.More(); it.Next()) {
        bool holds_moved = false;
        for (TopExp_Explorer f(it.Current(), TopAbs_FACE); f.More() && !holds_moved; f.Next()) {
          holds_moved = moved_images.Contains(f.Current());
        }
        if (!holds_moved) {
          std::vector<Shape::NamedFace> named;
          const ShapePtr all = split_namer.shape();
          for (TopExp_Explorer f(it.Current(), TopAbs_FACE); f.More(); f.Next()) {
            named.push_back({f.Current(), all->names_of_face(f.Current())});
          }
          kept.push_back(std::make_shared<Shape>(it.Current(), named));
        }
      }
      if (kept.empty()) {
        throw std::runtime_error("the target removes the whole body");
      }
      result = kept.front();
      for (std::size_t i = 1; i < kept.size(); ++i) {
        result = fuse_walls(*result, *kept[i]);
      }
      // The cut faces carry the name with split numbers; number them afresh.
      detail::FaceNamer renamed(result->occt());
      for (int i = 0; i < result->face_count(); ++i) {
        for (const std::string& n : result->face_names(i)) {
          renamed.add(result->face(i), name_matches(n, name) ? name : n);
        }
      }
      renamed.finish();
      result = renamed.shape();
    }
    if (result->find_faces(name).empty()) {
      throw std::runtime_error("the target does not meet the body where the faces are");
    }
    check_result(result, "replacing faces");
    return result;
  });
}

} // namespace mitcad::geometry
