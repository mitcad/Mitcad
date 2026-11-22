// SPDX-License-Identifier: MIT
#include "mitcad/geometry/faceops.hpp"

#include <algorithm>
#include <cmath>
#include <array>
#include <exception>
#include <map>
#include <optional>
#include <set>
#include <stdexcept>
#include <typeinfo>

#include <BOPAlgo_Alerts.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <Geom2d_Curve.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
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
#include <gp_Cylinder.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
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

// The body with each face whose surface is only C0 continuous split at its
// creases into C1 pieces that keep the face's names, or null when no face
// has one. OCCT's offset refuses C0 surfaces (BRepOffset_C0Geometry), though
// the face is often smooth: a stored body's B-splines whose knots repeat as
// often as their degree, such as rational arcs. Each piece offsets on its
// own, which gives the same wall (mitcad#158). `removed` (faces of the body)
// becomes the pieces of those faces.
ShapePtr split_at_creases(const Shape& body, std::set<int>& removed) {
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
  step("shell failed while splitting faces at their creases", [&] { divide.Perform(); });
  const TopoDS_Shape split = divide.Result();
  if (split.IsNull()) {
    return nullptr;
  }
  const occ::handle<ShapeBuild_ReShape> context = divide.GetContext();
  std::vector<Shape::NamedFace> faces;
  std::vector<TopoDS_Shape> removed_pieces;
  for (int i = 0; i < body.face_count(); ++i) {
    for (TopExp_Explorer it(context->Apply(body.face(i)), TopAbs_FACE); it.More(); it.Next()) {
      faces.push_back({it.Current(), body.face_names(i)});
      if (removed.count(i) > 0) {
        removed_pieces.push_back(it.Current());
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
  removed = pieces;
  return out;
}

// The wall between the body's surface and its offset by `offset` (negative
// inside), with the removed faces open. The outside of the wall keeps the
// face names; the inside is offset(<face>).
ShapePtr thick_solid(const std::string& feature, const Shape& input, const std::set<int>& removed_faces,
                     double offset, GeomAbs_JoinType join) {
  // OCCT's offset widens the tolerances of its input's sub-shapes.
  const ShapePtr copy = detail::working_copy(input);
  std::set<int> removed = removed_faces;
  const ShapePtr split = split_at_creases(*copy, removed);
  const Shape& body = split ? *split : *copy;
  NCollection_List<TopoDS_Shape> closing;
  for (int f : removed) {
    closing.Append(body.face(f));
  }
  BRepOffsetAPI_MakeThickSolid maker;
  detail::interruptible([&](const Message_ProgressRange& range) {
    named_failure([&] {
      maker.MakeThickSolidByJoin(body.occt(), closing, offset, kTolerance, BRepOffset_Skin, false,
                                 false, join, false, range);
    });
  });
  if (!maker.IsDone()) {
    throw std::runtime_error(offset_failure(maker.MakeOffset()));
  }
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
ShapePtr offset_body(const std::string& feature, const Shape& input, double offset,
                     GeomAbs_JoinType join, bool inner) {
  // OCCT's offset widens the tolerances of its input's sub-shapes.
  const ShapePtr copy = detail::working_copy(input);
  std::set<int> none;
  const ShapePtr split = split_at_creases(*copy, none);
  const Shape& body = split ? *split : *copy;
  const TopoDS_Shape solid = offset_input(body.occt());
  BRepOffsetAPI_MakeOffsetShape maker;
  detail::interruptible([&](const Message_ProgressRange& range) {
    named_failure([&] {
      maker.PerformByJoin(solid, offset, kTolerance, BRepOffset_Skin, false, false, join, false, range);
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

void check_result(const ShapePtr& shape, const char* operation) {
  detail::require_valid(shape->occt(), operation);
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape->occt(), props);
  if (!(props.Mass() > kTolerance)) {
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
  std::set<int> removed = removed_faces;
  const ShapePtr split = split_at_creases(*copy, removed);
  const Shape& body = split ? *split : *copy;
  ShapePtr result = detail::shell_by_cavity(feature, body, removed, inside, join, outer ? *outer : body);
  if (result) {
    try {
      check_result(result, "the shell");
    } catch (const std::runtime_error& error) {
      detail::shell_trace(std::string("the cavity's result was refused: ") + error.what());
      throw;
    }
  }
  return result;
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
  for (int i = 0; i < body.face_count(); ++i) {
    const TopoDS_Shape image = step("draft", [&] { return maker.ModifiedShape(body.face(i)); });
    add_names(namer, image, body.face_names(i));
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

// Moves each face by its own offset; the other faces stay on their surfaces.
// `except` are faces not to carry the names of.
detail::FaceNamer move_faces(const Shape& input, const std::vector<std::pair<int, double>>& moves,
                             const std::vector<int>& except, NCollection_List<TopoDS_Shape>& moved) {
  // OCCT's offset widens the tolerances of its input's sub-shapes.
  const ShapePtr copy = detail::working_copy(input);
  const Shape& body = *copy;
  BRepOffset_MakeOffset maker;
  maker.Initialize(offset_input(body.occt()), 0.0, kTolerance, BRepOffset_Skin, false, false,
                   GeomAbs_Intersection, false, false);
  for (const auto& [face, distance] : moves) {
    maker.SetOffsetOnFace(body.face(face), distance);
  }
  detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
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


} // namespace

ShapePtr offset_faces(const std::string& feature, const Shape& body,
                      const std::vector<std::string>& faces, double distance) {
  detail::require_finite("offset distance", distance);
  if (distance == 0.0) {
    throw std::invalid_argument("the offset distance must not be zero");
  }
  detail::require_surfaces(body.occt(), "offset faces");
  const std::vector<int> chosen = detail::resolve_faces(body, faces, "faces to offset");
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
    std::vector<std::pair<int, double>> moves;
    for (int f : chosen) {
      moves.emplace_back(f, distance);
    }
    NCollection_List<TopoDS_Shape> moved;
    detail::FaceNamer namer = move_faces(body, moves, {}, moved);
    namer.finish();
    ShapePtr result = namer.shape();
    check_result(result, "the offset");
    return result;
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
      for (TopExp_Explorer it(body.face(f), TopAbs_VERTEX); it.More(); it.Next()) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(it.Current()));
        const double t = gp_Vec(p, plane.Location()).Dot(gp_Vec(up)) / facing;
        nearest = first ? t : std::min(nearest, t);
        farthest = first ? t : std::max(farthest, t);
        first = false;
      }
      if (std::abs(1.0 - std::abs(facing)) > 1.0e-9) {
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
