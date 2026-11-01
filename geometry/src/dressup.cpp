// SPDX-License-Identifier: MIT
#include "mitcad/geometry/dressup.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <map>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <ChFi3d.hxx>
#include <ElCLib.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_IntSS.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Curve.hxx>
#include <Law_BSpFunc.hxx>
#include <Law_BSpline.hxx>
#include <Law_Interpolate.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Precision.hxx>
#include <Adaptor3d_CurveOnSurface.hxx>
#include <BRepLib_ValidateEdge.hxx>
#include <Geom2dAdaptor_Curve.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_Surface.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Edge.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt2d.hxx>

#include "blend.hpp"
#include "canonical.hpp"
#include "corner.hpp"
#include "cutouts.hpp"
#include "face_join.hpp"
#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitred_dressup.hpp"
#include "ring_dressup.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kHalfPi = 1.57079632679489661923;
// Tolerance on the angle across an edge that a chord length depends on.
constexpr double kAngleTolerance = 1.0e-4;
// OCCT fails where a rounding or bevel takes a neighbouring face away
// exactly: a radius as large as the face is wide (a full round), a chamfer
// as wide as a step. Imported designs often do that, so a failed dressup is
// built again with its sizes this much smaller, which leaves a strip of
// 1e-3 of the size where the face was (far below any tolerance of the
// part). A strip of 1e-4 is too thin: OCCT corrupted its memory on one of
// the corpus' fillets with it.
constexpr double kShrink = 1.0 - 1.0e-3;
// OCCT's fillet parameters (BRepFilletAPI_MakeFillet::SetParams: angular
// tolerance, tolerance of the spine, 2d tolerance, approximation
// tolerances in 3d and 2d, deflection) for a second try where the defaults
// (1e-2, 1e-4, 1e-5, 1e-4, 1e-5, 1e-3) fail: with the defaults OCCT finds no
// start for the rolling ball along some edges where the faces fold back
// sharply (mitcad#107).
constexpr double kTight[6] = {1.0e-3, 1.0e-7, 1.0e-9, 1.0e-6, 1.0e-8, 1.0e-5};
// The angle between the faces' normals across a knife edge: more than
// about 172 degrees (they fold back on each other).
constexpr double kKnife = 3.0;

// Twice a face's area over the length of its boundary: a strip's width.
double face_width(const TopoDS_Face& face) {
  GProp_GProps area;
  BRepGProp::SurfaceProperties(face, area);
  double boundary = 0.0;
  for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(ex.Current()))) {
      GProp_GProps length;
      BRepGProp::LinearProperties(ex.Current(), length);
      boundary += length.Mass();
    }
  }
  return boundary > 0.0 ? 2.0 * area.Mass() / boundary : 0.0;
}

// A dressup built with its sizes scaled by kShrink (`smaller`, of the body
// `input`, its largest size `size`) without the strips it left where the
// faces it takes away at its exact size were (mitcad#1106): the faces of
// the input that are narrower than 2e-3 of the size in it and were not
// in the input, beside a face the dressup made, removed by OCCT's
// defeaturing, which extends the faces round each strip to meet. The
// dressup's faces then meet the faces beyond the strips at one edge, as at
// the exact size, and take the strips' names, as a face the dressup takes
// away gives its names to the dressup's face there (name_consumed_faces).
// Null where it leaves no strip, or the defeaturing fails or gives no
// valid solid of nearly the same volume.
ShapePtr without_strips(const Shape& smaller, const Shape& input, double size) {
  const double narrow = 2.0e-3 * size;
  const detail::EdgeFaces edge_faces(smaller);
  // The dressup's faces: those whose names the input has not.
  const auto made = [&](int f) {
    const NameList& names = smaller.face_names(f);
    return !names.empty() && input.find_faces(names.front()).empty();
  };
  NCollection_List<TopoDS_Shape> strips;
  std::vector<int> strip_faces;
  for (int f = 0; f < smaller.face_count(); ++f) {
    const NameList& names = smaller.face_names(f);
    if (names.empty() || made(f) || !(face_width(smaller.face(f)) < narrow)) {
      continue;
    }
    bool was_wide = false;
    for (const int g : input.find_faces(names.front())) {
      was_wide = was_wide || face_width(input.face(g)) >= 2.0 * narrow;
    }
    bool beside = false;
    for (int e = 0; e < smaller.edge_count() && !beside; ++e) {
      const std::vector<int> faces = edge_faces.at(e);
      if (faces.size() == 2 && (faces[0] == f || faces[1] == f)) {
        beside = made(faces[0] == f ? faces[1] : faces[0]);
      }
    }
    if (was_wide && beside) {
      strips.Append(smaller.face(f));
      strip_faces.push_back(f);
    }
  }
  if (strips.IsEmpty()) {
    return nullptr;
  }
  try {
    OCC_CATCH_SIGNALS
    BRepAlgoAPI_Defeaturing defeaturing;
    defeaturing.SetShape(smaller.occt());
    defeaturing.AddFacesToRemove(strips);
    defeaturing.SetRunParallel(false);
    defeaturing.SetToFillHistory(true);
    defeaturing.Build();
    if (!defeaturing.IsDone() || defeaturing.HasWarnings()) {
      return nullptr;
    }
    const TopoDS_Shape whole = defeaturing.Shape();
    int solids = 0;
    for (TopExp_Explorer ex(whole, TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    const double before = detail::volume_of(smaller.occt());
    if (solids != 1 || !BRepCheck_Analyzer(whole, true, true).IsValid() ||
        std::abs(detail::volume_of(whole) - before) > 1.0e-4 * std::abs(before)) {
      return nullptr;
    }
    const occ::handle<BRepTools_History> history = defeaturing.History();
    detail::FaceNamer namer(whole);
    namer.carry(*history, smaller);
    // Each strip's names to the images of the dressup's faces beside it.
    for (const int f : strip_faces) {
      for (int e = 0; e < smaller.edge_count(); ++e) {
        const std::vector<int> faces = edge_faces.at(e);
        if (faces.size() != 2 || (faces[0] != f && faces[1] != f)) {
          continue;
        }
        const int other = faces[0] == f ? faces[1] : faces[0];
        if (!made(other)) {
          continue;
        }
        const TopoDS_Shape& face = smaller.face(other);
        if (history->IsRemoved(face)) {
          continue;
        }
        NCollection_List<TopoDS_Shape> images = history->Modified(face);
        if (images.IsEmpty()) {
          images.Append(face);
        }
        for (const TopoDS_Shape& image : images) {
          if (namer.contains(image)) {
            namer.add(image, smaller.face_names(f), -1);
          }
        }
      }
    }
    namer.finish();
    namer.set_valid(true);
    ShapePtr out = namer.shape();
    out->copy_notes(smaller);
    return out;
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

// Runs `attempt` with the sizes as given (scale 1), then, when it fails
// (not for invalid arguments), with the sizes scaled by kShrink, without
// the strips that leaves (without_strips; `input` the body it dresses,
// `size` its largest size) and with an info saying it is that much smaller
// (mitcad#1106: 0.1 % is no reason for a warning); throws the first error
// when both fail. `what`: "fillet" or "chamfer".
template <class Attempt>
ShapePtr with_shrink_retry(const char* what, const Shape& input, double size, Attempt&& attempt) {
  std::exception_ptr first;
  try {
    OCC_CATCH_SIGNALS
    return attempt(1.0);
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    first = std::current_exception();
  }
  try {
    OCC_CATCH_SIGNALS
    ShapePtr smaller = attempt(kShrink);
    if (ShapePtr whole = without_strips(*smaller, input, size)) {
      smaller = whole;
    }
    smaller->add_info(std::string("the ") + what +
                      " is built 0.1 % smaller than asked: at its exact size it takes a face away, "
                      "which the geometry kernel cannot build");
    return smaller;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
  }
  std::rethrow_exception(first);
}

std::string describe_edge(const Shape& body, int edge) {
  const std::string& name = body.edge_name(edge);
  return name.empty() ? "an unnamed edge" : "edge " + name;
}

// The set (index) each selected edge of the body belongs to; -1 for edges
// no set selects. An edge may be in one set only.
template <class Set>
std::vector<int> select_edges(const Shape& body, const std::vector<Set>& sets) {
  if (sets.empty()) {
    throw std::invalid_argument("no edges selected");
  }
  std::vector<int> owner(static_cast<std::size_t>(body.edge_count()), -1);
  for (std::size_t s = 0; s < sets.size(); ++s) {
    for (int edge : detail::resolve_edges(body, sets[s].edges, sets[s].faces)) {
      int& set = owner[static_cast<std::size_t>(edge)];
      if (set >= 0 && set != static_cast<int>(s)) {
        throw std::invalid_argument(describe_edge(body, edge) + " is in two edge sets");
      }
      set = static_cast<int>(s);
    }
  }
  return owner;
}

// Checks the contours OCCT built from the selected edges: a contour is the
// tangent chain of its edges. Every edge in it must come from one set, and
// an edge no set selects is only allowed when that set follows tangent
// chains. Returns the set of each contour.
template <class Set>
std::vector<int> check_contours(const BRepFilletAPI_LocalOperation& maker, const Shape& body,
                                const std::vector<Set>& sets, const std::vector<int>& owner) {
  std::vector<int> contour_sets;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    int set = -1;
    int unselected = -1;
    int seed = -1;
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const int edge = body.edge_index(maker.Edge(ic, ie));
      const int own = edge < 0 ? -1 : owner[static_cast<std::size_t>(edge)];
      if (own < 0) {
        unselected = edge;
      } else if (set < 0) {
        set = own;
        seed = edge;
      } else if (own != set) {
        throw std::invalid_argument("unsupported: " + describe_edge(body, edge) +
                                    " continues the tangent chain of " +
                                    describe_edge(body, seed) + " from another edge set");
      }
    }
    if (set >= 0 && unselected >= 0 && !sets[static_cast<std::size_t>(set)].tangent_chain) {
      throw std::invalid_argument(
          "unsupported: " + describe_edge(body, seed) + " continues tangentially into " +
          describe_edge(body, unselected) +
          "; without a tangent chain only the selected edges would be rounded, but the "
          "geometry kernel's chain goes on into it");
    }
    contour_sets.push_back(set);
  }
  return contour_sets;
}

// The body's edges the sets own in the order the sets list them (each
// set's edge names in turn), then the others (the edges of the faces named,
// those the tangent chains added, the pieces merged on a working copy whose
// names no longer resolve, and those no set owns) in the body's order.
template <class Set>
std::vector<int> seed_order(const Shape& body, const std::vector<Set>& sets, const std::vector<int>& owner) {
  std::vector<int> order;
  std::vector<char> placed(static_cast<std::size_t>(body.edge_count()), 0);
  for (std::size_t s = 0; s < sets.size(); ++s) {
    std::vector<int> listed;
    for (const std::string& name : sets[s].edges) {
      const std::vector<int> found = body.find_edges(name);
      listed.insert(listed.end(), found.begin(), found.end());
    }
    for (int edge : listed) {
      if (edge < 0 || edge >= body.edge_count()) {
        continue;
      }
      char& done = placed[static_cast<std::size_t>(edge)];
      if (owner[static_cast<std::size_t>(edge)] == static_cast<int>(s) && done == 0) {
        done = 1;
        order.push_back(edge);
      }
    }
  }
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (placed[static_cast<std::size_t>(edge)] == 0) {
      order.push_back(edge);
    }
  }
  return order;
}

// Whether a set's selection grows by Mitcad's own tangent chains
// (detail::follow_tangent_chains, mitcad#224). A variable radius runs along
// OCCT's chain, and asymmetric and G2 sets are built per chain (see
// dressup.hpp): those keep OCCT's chain alone.
bool follows_own_chain(const FilletSet& set) {
  return set.tangent_chain && !set.curvature &&
         (set.size == FilletSize::Constant || set.size == FilletSize::ChordLength);
}

bool follows_own_chain(const ChamferSet& set) { return set.tangent_chain; }

// Builds the dressup (`build`, of the selection of each edge: its set, -1
// for none) with Mitcad's own tangent chains of the sets added
// (detail::follow_tangent_chains, mitcad#224); where that fails, as before
// with the geometry kernel's chains alone, noting so (OCCT cannot round some
// chains it does not follow itself: two contours that meet end to end at a
// vertex where other sharp edges meet).
template <class Set, class Build>
ShapePtr with_own_chains(const char* what, const Shape& input, const std::vector<Set>& sets,
                         const std::vector<int>& owner, Build&& build) {
  std::vector<char> follows;
  for (const Set& set : sets) {
    follows.push_back(follows_own_chain(set) ? 1 : 0);
  }
  std::vector<int> grown = owner;
  if (detail::follow_tangent_chains(input, grown, follows) == 0) {
    return build(owner);
  }
  try {
    OCC_CATCH_SIGNALS
    return build(grown);
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
  }
  ShapePtr result = build(owner);
  result->add_note(std::string("the ") + what +
                   " follows the geometry kernel's tangent chains only: with the edges that continue them end to "
                   "end it could not be built");
  return result;
}

// Vertices where rounded edges of at least two contours meet, three or more
// edges in all: the corners a corner type (rolling ball or setback) shapes.
std::vector<TopoDS_Shape> corner_vertices(const BRepFilletAPI_LocalOperation& maker) {
  std::map<int, std::pair<std::set<int>, int>> at; // vertex -> (contours, edges)
  ShapeMap vertices;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      ShapeMap ends;
      TopExp::MapShapes(maker.Edge(ic, ie), TopAbs_VERTEX, ends);
      for (int k = 1; k <= ends.Extent(); ++k) {
        const int v = vertices.Add(ends(k));
        at[v].first.insert(ic);
        ++at[v].second;
      }
    }
  }
  std::vector<TopoDS_Shape> corners;
  for (const auto& [v, use] : at) {
    if (use.first.size() >= 2 && use.second >= 3) {
      corners.push_back(vertices(v));
    }
  }
  return corners;
}

// A dressup's result that BRepCheck rejects, repaired by shape healing
// with its names, or null. On imported bodies (.f3d bodies, with tolerances
// up to 0.05 mm) OCCT at times builds the rounding right but turns the
// solid inside out or leaves a small defect in a wire; healing repairs
// those. Only a repair that keeps the volume near the body's (within a
// quarter) is taken: a dressup changes it little, a wrong repair a lot.
ShapePtr healed(const Shape& result, const Shape& body) {
  try {
    OCC_CATCH_SIGNALS
    // Healing adds p-curves to edges and widens tolerances in place, and
    // the faces the dressup left are the body's (input_check.hpp); it moves
    // p-curves in place too (detail::own_pcurves).
    const ShapePtr copy = detail::working_copy(result);
    detail::own_pcurves(copy->occt());
    const Shape& named = *copy;
    ShapeFix_Shape fix(named.occt());
    detail::interruptible([&](const Message_ProgressRange& range) { fix.Perform(range); });
    const TopoDS_Shape fixed = fix.Shape();
    if (fixed.IsNull() || !BRepCheck_Analyzer(fixed, true, true).IsValid()) {
      return nullptr;
    }
    const double before = detail::volume_of(body.occt());
    const double after = detail::volume_of(fixed);
    if (!(after > 0.0) || std::abs(after - before) > 0.25 * std::abs(before)) {
      return nullptr;
    }
    detail::FaceNamer namer(fixed);
    namer.carry(*fix.Context()->History(), named);
    namer.carry_by_surface(named, "healed dressup");
    // Faces the healing rebuilt that neither its history nor their surfaces
    // name (a rounding's face at a pocket's corner, mitcad#1118): the names
    // of the face of the result they lie in.
    namer.carry_by_containment(named, 1.0e-6);
    namer.finish();
    namer.set_valid(true); // checked above
    return namer.shape();
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

// The faces and edges of a shape that OCCT's checker rejects (a wire's
// problem counts for its face), in any context.
ShapeMap faulty_parts(const TopoDS_Shape& shape) {
  BRepCheck_Analyzer analyzer(shape, true, true);
  ShapeMap faulty;
  const auto bad = [](const NCollection_List<BRepCheck_Status>& statuses) {
    for (const BRepCheck_Status s : statuses) {
      if (s != BRepCheck_NoError) {
        return true;
      }
    }
    return false;
  };
  const auto check = [&](const TopoDS_Shape& part, const TopoDS_Shape& owner) {
    const occ::handle<BRepCheck_Result>& result = analyzer.Result(part);
    if (result.IsNull()) {
      return;
    }
    bool faults = bad(result->Status());
    for (result->InitContextIterator(); result->MoreShapeInContext(); result->NextShapeInContext()) {
      faults = faults || bad(result->StatusOnShape());
    }
    if (faults) {
      faulty.Add(owner);
    }
  };
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    check(f.Current(), f.Current());
    for (TopExp_Explorer w(f.Current(), TopAbs_WIRE); w.More(); w.Next()) {
      check(w.Current(), f.Current());
    }
  }
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    check(e.Current(), e.Current());
  }
  return faulty;
}

// Whether every face and edge OCCT's checker rejects in a dressup's result
// is one of its input's that the dressup left as it was, rejected there
// too: stored bodies of imported designs are at times invalid in places
// (a self-intersecting wire of a planar face, mitcad#107), and a rounding
// elsewhere on them is then as good as its input. A problem of the shell
// or solid as a whole does not count as inherited, nor one of a face the
// dressup should have changed (`touched`: the faces at its edges), which
// the result holding it unchanged leaves unfinished, nor one next to a face
// the dressup made or changed: the faults must lie away from its work.
bool inherited_faults_only(const TopoDS_Shape& result, const TopoDS_Shape& input, const ShapeMap& touched) {
  const ShapeMap now = faulty_parts(result);
  if (now.IsEmpty()) {
    return false;
  }
  const ShapeMap before = faulty_parts(input);
  ShapeMap kept; // the input's faces in the result
  TopExp::MapShapes(input, TopAbs_FACE, kept);
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(result, TopAbs_EDGE, TopAbs_FACE, faces_of);
  for (int i = 1; i <= now.Extent(); ++i) {
    const TopoDS_Shape& part = now(i);
    if (!before.Contains(part) || touched.Contains(part)) {
      return false;
    }
    for (TopExp_Explorer e(part, TopAbs_EDGE); e.More(); e.Next()) {
      const int index = faces_of.FindIndex(e.Current());
      if (index == 0) {
        continue;
      }
      for (const TopoDS_Shape& next : faces_of(index)) {
        if (!kept.Contains(next)) {
          return false;
        }
      }
    }
  }
  return true;
}

} // namespace

// (util.hpp)
int detail::widen_inherited_curve_tolerances(const TopoDS_Shape& result, const TopoDS_Shape& input) {
  using EdgeFaces =
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;
  // The largest distance between the edge's 3D curve and its curves on its
  // faces, as OCCT's checker measures it (but exactly, not at sample points).
  const auto deviation_of = [](const TopoDS_Edge& edge, const EdgeFaces& faces_of) {
    double deviation = 0.0;
    const int index = faces_of.FindIndex(edge);
    if (index == 0) {
      return deviation;
    }
    const occ::handle<Adaptor3d_Curve> curve = new BRepAdaptor_Curve(edge);
    for (const TopoDS_Shape& face : faces_of(index)) {
      for (TopExp_Explorer e(face.Oriented(TopAbs_FORWARD), TopAbs_EDGE); e.More(); e.Next()) {
        if (!e.Current().IsSame(edge)) {
          continue;
        }
        const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
        double first = 0.0, last = 0.0;
        const occ::handle<Geom2d_Curve> pcurve =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(e.Current()), forward, first, last);
        const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(forward);
        if (pcurve.IsNull() || surface.IsNull()) {
          continue;
        }
        const occ::handle<Adaptor3d_CurveOnSurface> on_surface = new Adaptor3d_CurveOnSurface(
          new Geom2dAdaptor_Curve(pcurve, first, last), new GeomAdaptor_Surface(surface));
        BRepLib_ValidateEdge validate(curve, on_surface, BRep_Tool::SameParameter(edge));
        validate.SetExactMethod(true);
        validate.Process();
        if (validate.IsDone()) {
          deviation = std::max(deviation, validate.GetMaxDistance());
        }
      }
    }
    return deviation;
  };
  std::map<const Geom_Curve*, TopoDS_Edge> input_curves;
  ShapeMap input_edges;
  TopExp::MapShapes(input, TopAbs_EDGE, input_edges);
  for (int i = 1; i <= input_edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(input_edges(i));
    double first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (!curve.IsNull()) {
      input_curves.emplace(curve.get(), edge);
    }
  }
  EdgeFaces result_faces, input_faces;
  TopExp::MapShapesAndAncestors(result, TopAbs_EDGE, TopAbs_FACE, result_faces);
  BRep_Builder builder;
  int widened = 0;
  for (int i = 1; i <= result_faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(result_faces.FindKey(i));
    if (input_edges.Contains(edge) || BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    const auto source = curve.IsNull() ? input_curves.end() : input_curves.find(curve.get());
    if (source == input_curves.end()) {
      continue;
    }
    const double deviation = deviation_of(edge, result_faces);
    if (deviation <= BRep_Tool::Tolerance(edge)) {
      continue;
    }
    if (input_faces.IsEmpty()) {
      TopExp::MapShapesAndAncestors(input, TopAbs_EDGE, TopAbs_FACE, input_faces);
    }
    if (deviation <= deviation_of(source->second, input_faces) + Precision::Confusion()) {
      builder.UpdateEdge(edge, deviation);
      ++widened;
    }
  }
  return widened;
}

namespace {

// Set while a dressup is tried again another way (dress): a result OCCT's
// checker rejects is then not healed. Healing keeps a repair within a
// quarter of the volume, too loose to take a second way's result on trust.
thread_local bool t_retry = false;

class RetryScope {
public:
  RetryScope() { t_retry = true; }
  ~RetryScope() { t_retry = false; }
  RetryScope(const RetryScope&) = delete;
  RetryScope& operator=(const RetryScope&) = delete;
};

// Whether edge `a` runs along edge `b`: its ends and its middle lie on b
// within their tolerances.
bool runs_along(const TopoDS_Edge& a, const TopoDS_Edge& b) {
  if (BRep_Tool::Degenerated(a) || BRep_Tool::Degenerated(b)) {
    return false;
  }
  const BRepAdaptor_Curve curve(a);
  const double tolerance = BRep_Tool::Tolerance(a) + BRep_Tool::Tolerance(b) + Precision::Confusion();
  for (const double t : {0.0, 0.5, 1.0}) {
    const gp_Pnt p = curve.Value(curve.FirstParameter() + t * (curve.LastParameter() - curve.FirstParameter()));
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), b);
    if (!distance.IsDone() || distance.Value() > tolerance) {
      return false;
    }
  }
  return true;
}

// A face of the body that the rounding or chamfer of one of its edges
// consumed, where the blend's contact runs along the face's far edge (a
// face exactly as wide as the contact distance, or a blend rolling on that
// edge, mitcad#121), lives on in the blend's face: it now meets the faces
// beyond in the old edges. The blend's face also takes the consumed face's
// names, so that those edges keep their names (a later rounding of the
// same item, one tangent chain at a time, still finds them). Only a face
// at the edges of one contour is named so; a blend that runs onto the face
// beyond touches none of the face's edges and takes no names.
void name_consumed_faces(BRepFilletAPI_LocalOperation& maker, const Shape& body, detail::FaceNamer& namer) {
  ShapeMap contours;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      contours.Add(maker.Edge(ic, ie));
    }
  }
  for (int f = 0; f < body.face_count(); ++f) {
    const TopoDS_Face& face = body.face(f);
    if (body.face_names(f).empty() || !maker.IsDeleted(face)) {
      continue;
    }
    int contour = 0;
    std::vector<TopoDS_Edge> at;   // the contours' edges of the face
    std::vector<TopoDS_Edge> rest; // its other edges
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      if (!contours.Contains(edge)) {
        rest.push_back(edge);
        continue;
      }
      const int ic = maker.Contour(edge);
      contour = (contour == 0 || contour == ic) ? ic : -1;
      at.push_back(edge);
    }
    if (contour <= 0) {
      continue;
    }
    for (const TopoDS_Edge& edge : at) {
      for (const TopoDS_Shape& image : maker.Generated(edge)) {
        if (image.ShapeType() != TopAbs_FACE || !namer.contains(image)) {
          continue;
        }
        bool along = false;
        for (TopExp_Explorer e(image, TopAbs_EDGE); e.More() && !along; e.Next()) {
          for (const TopoDS_Edge& old : rest) {
            if (runs_along(TopoDS::Edge(e.Current()), old)) {
              along = true;
              break;
            }
          }
        }
        if (along) {
          namer.add(image, body.face_names(f), -1);
        }
      }
    }
  }
}

// Faces of the result still without a name after the history and the
// body's surfaces named the others: faces of the dressup that OCCT's
// history gives no contour edge (a rounding of an edge at a pocket's
// corners, its stripe's ends rebuilt where its ball rolled on the far edge
// of a face it takes away, mitcad#1118). Each is named after the named
// contour edge nearest it, as the faces generated from that edge are, where
// no face holds that edge's dressup name yet (a face the history gave the
// name keeps it whole).
void name_after_nearest_edge(BRepFilletAPI_LocalOperation& maker, const std::string& feature, const char* role,
                             const Shape& body, detail::FaceNamer& namer) {
  ShapeMap faces;
  TopExp::MapShapes(maker.Shape(), TopAbs_FACE, faces);
  std::set<std::string> held;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      if (const auto name = body.name_of_edge(maker.Edge(ic, ie))) {
        const std::string dressup = face_name(feature, role, *name);
        if (namer.holds(dressup)) {
          held.insert(dressup);
        }
      }
    }
  }
  for (int f = 1; f <= faces.Extent(); ++f) {
    if (namer.named(faces(f))) {
      continue;
    }
    std::optional<std::string> nearest;
    double distance = 0.0;
    for (int ic = 1; ic <= maker.NbContours(); ++ic) {
      for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
        const auto name = body.name_of_edge(maker.Edge(ic, ie));
        if (!name) {
          continue;
        }
        try {
          OCC_CATCH_SIGNALS
          BRepExtrema_DistShapeShape between(faces(f), maker.Edge(ic, ie));
          if (between.IsDone() && (!nearest || between.Value() < distance)) {
            nearest = *name;
            distance = between.Value();
          }
        } catch (const Standard_Failure&) {
        }
      }
    }
    if (nearest && held.count(face_name(feature, role, *nearest)) == 0) {
      namer.add(faces(f), face_name(feature, role, *nearest));
    }
  }
}

// Names the result of a fillet or chamfer: the faces generated from every
// edge of the contours (the tangent chains included) and their vertices.
ShapePtr name_result(BRepFilletAPI_LocalOperation& maker, const std::string& feature,
                     const char* role, const Shape& body) {
  detail::FaceNamer namer(maker.Shape());
  namer.carry(maker, body);
  ShapeMap vertices;
  ShapeMap touched; // the faces at the contours' edges
  const detail::EdgeFaces edge_faces(body);
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const TopoDS_Edge& edge = maker.Edge(ic, ie);
      if (const auto name = body.name_of_edge(edge)) {
        namer.generated(maker, edge, face_name(feature, role, *name));
      }
      TopExp::MapShapes(edge, TopAbs_VERTEX, vertices);
      const int index = body.edge_index(edge);
      if (index >= 0) {
        for (const int f : edge_faces.at(index)) {
          touched.Add(body.face(f));
        }
      }
    }
  }
  for (int v = 1; v <= vertices.Extent(); ++v) {
    if (const auto name = body.name_of_vertex(vertices(v))) {
      namer.generated(maker, vertices(v), face_name(feature, "corner", *name));
    }
  }
  name_consumed_faces(maker, body, namer);
  namer.carry_by_surface(body, role);
  name_after_nearest_edge(maker, feature, role, body, namer);
  namer.finish();
  // Vertex tolerances OCCT inflated far beyond what the vertices' edges need
  // (detail::inflated_vertices, mitcad#235) are cut down to it: a result
  // valid with them so is the result; one that is not hid a defect at its
  // corners behind them (an edge ending off its vertex, faces that cannot
  // be oriented), which the booleans after it fail on.
  if (detail::inflated_vertices(namer.result(), false) > 0) {
    const ShapePtr tightened = detail::working_copy(*namer.shape());
    detail::inflated_vertices(tightened->occt(), true);
    if (!BRepCheck_Analyzer(tightened->occt(), true, true).IsValid()) {
      throw std::runtime_error(std::string(role) +
                               " failed; the geometry kernel leaves a corner of the rounding open, its gap hidden "
                               "by a vertex tolerance");
    }
    tightened->set_checked_valid(true);
    return tightened;
  }
  // One check of a valid result: on a large body BRepCheck takes about as
  // long as the fillet itself (on OCCT's thread pool, mitcad#181).
  bool valid = BRepCheck_Analyzer(namer.result(), true, true).IsValid();
  if (!valid && detail::widen_inherited_curve_tolerances(namer.result(), body.occt()) > 0) {
    valid = BRepCheck_Analyzer(namer.result(), true, true).IsValid();
  }
  if (valid) {
    // Found valid: a boolean on it checks only what it changes
    // (detail::is_valid_where_changed).
    namer.set_valid(true);
  } else if (!inherited_faults_only(namer.result(), body.occt(), touched)) {
    if (!t_retry) {
      if (ShapePtr repaired = healed(*namer.shape(), body)) {
        return repaired;
      }
    }
    detail::require_valid(namer.result(), role); // throws with the problem
  }
  return namer.shape();
}

// After a contour stopped where its edges turn (port patch 0180), the face
// beyond the crease grows along its edge up to where it cuts the rounding,
// and OCCT keeps the vertex where the contour stopped between the two
// pieces of that edge. The roundings in the files have one edge there: the
// pieces are joined (ShapeUpgrade_UnifySameDomain on edges, every other
// vertex kept; it joins lines and circles, not two different splines along
// one line), the faces keeping their names.
ShapePtr join_at_turn_stops(const ShapePtr& rounded,
                            const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>& stops) {
  if (stops.IsEmpty()) {
    return rounded;
  }
  ShapeMap vertices;
  TopExp::MapShapes(rounded->occt(), TopAbs_VERTEX, vertices);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> keep;
  bool any = false;
  for (int v = 1; v <= vertices.Extent(); ++v) {
    if (stops.Contains(vertices(v))) {
      any = true;
    } else {
      keep.Add(vertices(v));
    }
  }
  if (!any) {
    return rounded;
  }
  ShapeUpgrade_UnifySameDomain unify(rounded->occt(), true, false, false);
  unify.KeepShapes(keep);
  unify.Build();
  ShapeMap edges;
  TopExp::MapShapes(unify.Shape(), TopAbs_EDGE, edges);
  if (edges.Extent() == rounded->edge_count()) {
    return rounded;
  }
  detail::FaceNamer namer(unify.Shape());
  namer.carry(*unify.History(), *rounded);
  namer.carry_by_surface(*rounded, "dressup joined at turn stops");
  namer.finish();
  if (const std::optional<bool> valid = rounded->checked_valid()) {
    namer.set_valid(*valid);
  }
  ShapePtr joined = namer.shape();
  joined->copy_notes(*rounded);
  return joined;
}

// Where OCCT's chamfer is made of pieces on one surface (cut by the faces
// beyond over a gap or up to its ends, port patches 0650 and 0671), its
// contacts along the pieces meet at vertices of their own, two edges each
// between the same two faces. The chamfers in the files have one edge there
// (and OCCT's later roundings of the edges beside them fail at those
// vertices): the result's vertices that are not the input's and where only
// two edges meet, for join_at_turn_stops to join.
NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> piece_vertices(const Shape& result, const Shape& input) {
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> pieces;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edges_of;
  TopExp::MapShapesAndUniqueAncestors(result.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_of);
  ShapeMap old;
  TopExp::MapShapes(input.occt(), TopAbs_VERTEX, old);
  for (int v = 1; v <= edges_of.Extent(); ++v) {
    if (edges_of(v).Extent() != 2 || old.Contains(edges_of.FindKey(v))) {
      continue;
    }
    const TopoDS_Vertex& vertex = TopoDS::Vertex(edges_of.FindKey(v));
    const gp_Pnt p = BRep_Tool::Pnt(vertex);
    bool at_old = false;
    for (int o = 1; o <= old.Extent() && !at_old; ++o) {
      at_old = BRep_Tool::Pnt(TopoDS::Vertex(old(o))).Distance(p) <= BRep_Tool::Tolerance(vertex);
    }
    if (!at_old) {
      pieces.Add(vertex);
    }
  }
  return pieces;
}

// The two faces of a body edge; throws for a free edge.
std::pair<int, int> edge_face_pair(const Shape& body, int edge) {
  const std::vector<int> faces = detail::faces_at_edge(body, edge);
  if (faces.size() != 2) {
    throw std::invalid_argument(describe_edge(body, edge) + " does not lie between two faces");
  }
  return {faces[0], faces[1]};
}

// The fillet radius whose rounding is `chord` wide across the edge: with an
// angle a between the faces' normals, r = c / (2 sin(a / 2)).
double chord_radius(const Shape& body, int edge, double chord) {
  const auto [a, b] = edge_face_pair(body, edge);
  const TopoDS_Edge& e = body.edge(edge);
  const double angle = detail::normal_angle(e, body.face(a), body.face(b));
  for (const double t : {0.1, 0.9}) {
    if (std::abs(detail::normal_angle(e, body.face(a), body.face(b), t) - angle) >
        kAngleTolerance) {
      throw std::invalid_argument("unsupported: a chord length fillet on " +
                                  describe_edge(body, edge) +
                                  ", where the angle between the faces varies");
    }
  }
  if (angle < 1.0e-3) {
    throw std::invalid_argument(describe_edge(body, edge) + " joins its faces tangentially");
  }
  return chord / (2.0 * std::sin(angle / 2.0));
}

// A spline radius (mitcad#464): degree 1 to 3, 2 to 100 positive control
// radii, its knots rising from 0 to 1, each end repeated degree + 1 times.
void check_spline(const FilletSet& set) {
  const std::size_t n = set.radii.size();
  if (n < 2 || n > 100 || set.knots.size() < n + 2 || set.knots.size() > n + 4 ||
      set.knots.size() - n - 1 >= n) {
    throw std::invalid_argument("a spline radius has degree 1 to 3 and 2 to 100 control radii, and as "
                                "many knots as both and one more");
  }
  const std::size_t degree = set.knots.size() - n - 1;
  for (std::size_t i = 0; i < set.knots.size(); ++i) {
    const double k = set.knots[i];
    const bool end = i <= degree || i + degree + 1 >= set.knots.size();
    if (!std::isfinite(k) || (i > 0 && k < set.knots[i - 1]) ||
        (end && k != (i <= degree ? 0.0 : 1.0))) {
      throw std::invalid_argument("the knots of a spline radius rise from 0 to 1, each end repeated "
                                  "degree + 1 times");
    }
  }
  for (const double radius : set.radii) {
    detail::require_positive("fillet control radius", radius);
  }
}

void check_fillet_set(const FilletSet& set) {
  switch (set.size) {
  case FilletSize::Constant:
    detail::require_positive("fillet radius", set.radius);
    break;
  case FilletSize::ChordLength:
    detail::require_positive("fillet chord length", set.chord);
    break;
  case FilletSize::Variable: {
    detail::require_positive("fillet start radius", set.radius);
    detail::require_positive("fillet end radius", set.radius2);
    double last = 0.0;
    for (const auto& [position, radius] : set.mid) {
      if (!(position > last && position < 1.0)) {
        throw std::invalid_argument("the mid radius positions must increase between 0 and 1");
      }
      detail::require_positive("fillet mid radius", radius);
      last = position;
    }
    break;
  }
  case FilletSize::Asymmetric:
    detail::require_positive("fillet distance", set.radius);
    detail::require_positive("second fillet distance", set.radius2);
    break;
  case FilletSize::Spline:
    check_spline(set);
    break;
  }
  if (set.curvature && !(set.weight >= 0.1 && set.weight <= 2.0)) {
    throw std::invalid_argument("the tangency weight must be between 0.1 and 2");
  }
}

// The radius at an abscissa of a smooth law through (abscissa, radius)
// points, level at both ends: the interpolation OCCT makes of the radii
// along a spine (ChFiDS_FilSpine with Law_Interpol), so that radii set at
// further abscissae from it leave the law as it is.
class RadiusLaw {
public:
  explicit RadiusLaw(const std::vector<gp_Pnt2d>& points) {
    const int n = static_cast<int>(points.size());
    occ::handle<NCollection_HArray1<double>> values = new NCollection_HArray1<double>(1, n);
    occ::handle<NCollection_HArray1<double>> parameters = new NCollection_HArray1<double>(1, n);
    for (int i = 0; i < n; ++i) {
      parameters->SetValue(i + 1, points[static_cast<std::size_t>(i)].X());
      values->SetValue(i + 1, points[static_cast<std::size_t>(i)].Y());
    }
    Law_Interpolate interpolation(values, parameters, false, Precision::Confusion());
    interpolation.Load(0.0, 0.0);
    interpolation.Perform();
    if (!interpolation.IsDone()) {
      throw std::runtime_error("the radii of a variable fillet could not be interpolated");
    }
    m_law = interpolation.Curve();
  }

  double operator()(double abscissa) const { return m_law->Value(abscissa); }

private:
  occ::handle<Law_BSpline> m_law;
};

// Adds a variable radius set's contour from edge `e`: the radius from the
// start (the set's start vertex, else the spine's first vertex) to the end
// of the whole chain, through the mid radii at their fractions of its length.
void add_variable(BRepFilletAPI_MakeFillet& maker, const Shape& body, const FilletSet& set,
                  const TopoDS_Edge& e, double scale) {
  const double r1 = set.radius * scale;
  const double r2 = set.radius2 * scale;
  maker.Add(r1, r2, e);
  const int ic = maker.Contour(e);
  const bool closed = maker.FirstVertex(ic).IsSame(maker.LastVertex(ic));
  bool reversed = false;
  if (!set.start_vertex.empty()) {
    const std::vector<int> start = body.find_vertices(set.start_vertex);
    if (start.size() != 1) {
      throw std::invalid_argument("the body has no vertex " + set.start_vertex);
    }
    if (closed) {
      throw std::invalid_argument("unsupported: a variable radius around a closed chain");
    }
    const TopoDS_Shape& vertex = body.vertex(start.front());
    reversed = maker.LastVertex(ic).IsSame(vertex);
    if (!reversed && !maker.FirstVertex(ic).IsSame(vertex)) {
      throw std::invalid_argument("vertex " + set.start_vertex + " is not an end of the edges' chain");
    }
  }
  if (closed) {
    if (!set.mid.empty()) {
      throw std::invalid_argument("unsupported: mid radii around a closed chain");
    }
    return; // from the start of OCCT's spine round to it
  }
  if (maker.NbEdges(ic) == 1 && set.mid.empty()) {
    if (reversed) {
      // The law runs from the spine's first vertex; start from the other end.
      maker.Remove(e);
      maker.Add(r2, r1, e);
    }
    return;
  }
  // The radii along the spine (abscissa from its first vertex).
  const double length = maker.Length(ic);
  std::vector<gp_Pnt2d> law{{0.0, r1}};
  for (const auto& [position, radius] : set.mid) {
    law.emplace_back(position * length, radius * scale);
  }
  law.emplace_back(length, r2);
  if (reversed) {
    for (gp_Pnt2d& point : law) {
      point.SetX(length - point.X());
    }
    std::reverse(law.begin(), law.end());
  }
  const RadiusLaw radius(law);
  // OCCT takes radii per edge of the spine, at fractions of the edge: each
  // edge gets the law at its ends and the radii that fall on it.
  for (int j = 1; j <= maker.NbEdges(ic); ++j) {
    const TopoDS_Edge& edge = maker.Edge(ic, j);
    double a = maker.Abscissa(ic, TopExp::FirstVertex(edge));
    double b = maker.Abscissa(ic, TopExp::LastVertex(edge));
    if (a > b) {
      std::swap(a, b);
    }
    if (!(b - a > Precision::Confusion())) {
      continue;
    }
    std::vector<gp_Pnt2d> points{{0.0, radius(a)}};
    for (const gp_Pnt2d& point : law) {
      if (point.X() > a + Precision::Confusion() && point.X() < b - Precision::Confusion()) {
        points.emplace_back((point.X() - a) / (b - a), point.Y());
      }
    }
    points.emplace_back(1.0, radius(b));
    NCollection_Array1<gp_Pnt2d> array(1, static_cast<int>(points.size()));
    for (std::size_t k = 0; k < points.size(); ++k) {
      array(static_cast<int>(k) + 1) = points[k];
    }
    maker.SetRadius(array, ic, j);
  }
}

// Adds a spline radius set's contour from edge `e` (mitcad#464): the radius
// the set's B-spline of the position along the whole chain, by arc length
// from the start (the set's start vertex, else the spine's first vertex),
// given to OCCT as a law of the spine's abscissa (port patch 0920).
void add_spline(BRepFilletAPI_MakeFillet& maker, const Shape& body, const FilletSet& set,
                const TopoDS_Edge& e, double scale) {
  maker.Add(set.radii.front() * scale, e);
  const int ic = maker.Contour(e);
  if (maker.FirstVertex(ic).IsSame(maker.LastVertex(ic))) {
    throw std::invalid_argument("unsupported: a spline radius around a closed chain");
  }
  bool reversed = false;
  if (!set.start_vertex.empty()) {
    const std::vector<int> start = body.find_vertices(set.start_vertex);
    if (start.size() != 1) {
      throw std::invalid_argument("the body has no vertex " + set.start_vertex);
    }
    const TopoDS_Shape& vertex = body.vertex(start.front());
    reversed = maker.LastVertex(ic).IsSame(vertex);
    if (!reversed && !maker.FirstVertex(ic).IsSame(vertex)) {
      throw std::invalid_argument("vertex " + set.start_vertex + " is not an end of the edges' chain");
    }
  }
  std::vector<double> knots = set.knots;
  std::vector<double> radii = set.radii;
  if (reversed) {
    for (double& knot : knots) {
      knot = 1.0 - knot;
    }
    std::reverse(knots.begin(), knots.end());
    std::reverse(radii.begin(), radii.end());
  }
  // OCCT's knots are distinct, with multiplicities; along the spine's
  // abscissa from its first vertex.
  const double from = maker.Abscissa(ic, maker.FirstVertex(ic));
  const double length = maker.Length(ic);
  std::vector<double> distinct;
  std::vector<int> multiplicities;
  for (const double knot : knots) {
    if (!distinct.empty() && knot <= distinct.back()) {
      ++multiplicities.back();
    } else {
      distinct.push_back(knot);
      multiplicities.push_back(1);
    }
  }
  NCollection_Array1<double> poles(1, static_cast<int>(radii.size()));
  for (std::size_t i = 0; i < radii.size(); ++i) {
    poles(static_cast<int>(i) + 1) = radii[i] * scale;
  }
  NCollection_Array1<double> knot_values(1, static_cast<int>(distinct.size()));
  NCollection_Array1<int> knot_multiplicities(1, static_cast<int>(distinct.size()));
  for (std::size_t i = 0; i < distinct.size(); ++i) {
    knot_values(static_cast<int>(i) + 1) = from + distinct[i] * length;
    knot_multiplicities(static_cast<int>(i) + 1) = multiplicities[i];
  }
  const int degree = static_cast<int>(set.knots.size() - set.radii.size() - 1);
  occ::handle<Law_BSpline> curve = new Law_BSpline(poles, knot_values, knot_multiplicities, degree);
  maker.SetRadius(occ::handle<Law_Function>(new Law_BSpFunc(curve, from, from + length)), ic, 1);
}

// Setback and blend corners stop the roundings or bevels this many times
// their largest size from the vertex (Mitcad's convention until K89, K90):
// past the rolling ball's or OCCT's own corner face.
constexpr double kSetback = 1.5;

// The largest radius a set rounds an edge of the body with.
double largest_radius(const Shape& body, const FilletSet& set, int edge) {
  switch (set.size) {
  case FilletSize::Constant:
    return set.radius;
  case FilletSize::ChordLength:
    return edge < 0 ? set.chord : chord_radius(body, edge, set.chord);
  case FilletSize::Variable: {
    double largest = std::max(set.radius, set.radius2);
    for (const auto& mid : set.mid) {
      largest = std::max(largest, mid.second);
    }
    return largest;
  }
  case FilletSize::Asymmetric:
    return std::max(set.radius, set.radius2);
  case FilletSize::Spline:
    // The B-spline keeps within its control radii.
    return set.radii.empty() ? 0.0 : *std::max_element(set.radii.begin(), set.radii.end());
  }
  return set.radius;
}

// The largest distance a chamfer set bevels a face to.
double largest_distance(const ChamferSpec& spec) {
  switch (spec.type) {
  case ChamferType::EqualDistance:
    return spec.distance;
  case ChamferType::TwoDistances:
    return std::max(spec.distance, spec.distance2);
  case ChamferType::DistanceAngle:
    return spec.distance * std::max(1.0, std::tan(spec.angle));
  }
  return spec.distance;
}

// The largest size of the sets: a radius, a chord, a distance.
double largest_size(const std::vector<FilletSet>& sets) {
  double size = 0.0;
  for (const FilletSet& set : sets) {
    size = std::max({size, set.radius, set.radius2, set.chord});
    for (const auto& point : set.mid) {
      size = std::max(size, point.second);
    }
    for (const double radius : set.radii) {
      size = std::max(size, radius);
    }
  }
  return size;
}

double largest_size(const std::vector<ChamferSet>& sets) {
  double size = 0.0;
  for (const ChamferSet& set : sets) {
    size = std::max(size, largest_distance(set.spec));
  }
  return size;
}

// Why OCCT's fillet did not build the corner at its first faulty vertex
// (port patch 0330, mitcad#323): which corner (a rounding's end, two or
// more roundings meeting) and why, from the computation's exception. The
// text always names a vertex: the import then tries more edges, which can
// end a rounding at a vertex.
std::string corner_failure(const BRepFilletAPI_MakeFillet& maker) {
  std::string what;
  switch (maker.FailedCorner(1)) {
  case ChFi3d_Builder::Corner_End:
  case ChFi3d_Builder::Corner_EndMitred:
    what = "cannot end the rounding at a vertex";
    break;
  case ChFi3d_Builder::Corner_EndMoreSurfdata:
    what = "cannot end the rounding at a vertex where its last pieces lie on different faces";
    break;
  case ChFi3d_Builder::Corner_EndAtIntersection:
    what = "cannot end the rounding at a vertex where more than three sharp edges meet";
    break;
  case ChFi3d_Builder::Corner_Two:
    what = "cannot join the roundings of two edges at a vertex";
    break;
  case ChFi3d_Builder::Corner_Three:
    what = "cannot join the roundings of three edges at a vertex";
    break;
  case ChFi3d_Builder::Corner_More:
    what = "cannot join the roundings at a vertex where " + std::to_string(maker.FailedCornerBlends(1)) +
           " roundings and " + std::to_string(maker.FailedCornerEdges(1)) + " sharp edges meet";
    break;
  case ChFi3d_Builder::Corner_Singular:
    what = "cannot join the roundings that end in points at a vertex";
    break;
  case ChFi3d_Builder::Corner_None:
    what = "cannot end or join the rounding at a vertex";
    break;
  }
  // The corner computation's exceptions, by their messages (OCCT's own
  // and the port's), as causes.
  static const std::pair<const char*, const char*> causes[] = {
      {"bouchon non ecrit", "the rounding's end does not meet the face at the end"},
      {"cap not written", "the rounding's end does not meet the face at the end"},
      {"fillets have too big radiuses",
       "where the rounding cuts the face at the end, it crosses another rounding's contact on that face"},
      {"echec calcul intersection", "the rounding's surface and the face at the end do not intersect"},
      {"does not cross the face at the end", "the edge beside the end does not cross the face at the end"},
      {"the max number of faces reached",
       "the walk round the vertex along the faces at the end does not come back to the rounding"},
      {"pb intersection Face", "the rounding does not intersect a face at the end"},
      {"Failed to find edge", "two faces met round the vertex share no edge"},
      {"Failed to find element", "a face round the vertex has no other edge at it"},
      {"Failed to get p-curve", "an edge at the vertex has no curve on a face the corner takes it on"},
      {"no parameters on surface", "an edge at the vertex has no curve on a face the corner takes it on"},
      {"NULL face", "the two roundings end on no common face"},
      {"corner not computed", "the pieces of the rounding at its end are not as the corner needs"},
      {"U1 == U2", "a curve of the corner has no length"},
      {"no point on arc", "the rounding's contact does not reach the edge at the vertex"},
      {"ProjLib", "a curve of the corner does not project onto a face"},
      {"SIGSEGV", "the corner's computation crashed (caught)"},
      {"ACCESS_VIOLATION", "the corner's computation crashed (caught)"},
  };
  const std::string message = maker.FailedCornerMessage(1).ToCString();
  std::string cause = message.empty() ? "the corner's computation gives no result"
                                      : "the corner's computation fails (" + message + ")";
  for (const auto& [key, text] : causes) {
    if (message.find(key) != std::string::npos) {
      cause = text;
      break;
    }
  }
  std::string more;
  if (maker.NbFaultyVertices() > 1) {
    more = " (and at " + std::to_string(maker.NbFaultyVertices() - 1) + " more vertices)";
  }
  return "the geometry kernel " + what + more + ": " + cause;
}

// Why OCCT's fillet did not build, from where it stopped (port patch 0320,
// mitcad#322): a contour's rounding (no start, its walk, a twisted surface,
// another failure), roundings of contours that overlap, cross or intersect
// each other, or a corner. The import's report names it; a text that names
// no vertex marks a failure along the edges (the import's retries rely on
// that).
std::string fillet_failure(const BRepFilletAPI_MakeFillet& maker, const Shape& body) {
  const int ic = maker.FailedContour();
  std::string along;
  if (ic > 0 && maker.NbEdges(ic) > 0) {
    const int edge = body.edge_index(maker.Edge(ic, 1));
    along = " along " + (edge >= 0 ? describe_edge(body, edge) : std::string("an edge"));
    if (maker.NbEdges(ic) > 1) {
      along += " (and " + std::to_string(maker.NbEdges(ic) - 1) + " more edges of its chain)";
    }
  }
  switch (maker.Failure()) {
  case ChFi3d_Builder::Failure_NoStart:
    return "the geometry kernel finds no start for the rounding" + along +
           ": no ball of the radius touches both faces there";
  case ChFi3d_Builder::Failure_Walk:
    return "the rounding" + along +
           " stops part of the way: the geometry kernel's walk between the two faces fails";
  case ChFi3d_Builder::Failure_WalkOnEdge:
    return "the rounding" + along +
           " stops part of the way where it rolls on an edge of a neighbouring face";
  case ChFi3d_Builder::Failure_Twisted:
    return "the rounding's surface" + along + " twists: the radius is larger than a face's curvature there";
  case ChFi3d_Builder::Failure_Stripe:
    return "the geometry kernel's computation of the rounding" + along + " fails";
  case ChFi3d_Builder::Failure_Overlap:
    return "the roundings of two edges overlap on the face between them" +
           (along.empty() ? std::string() : " (one" + along + ")") +
           ", and the geometry kernel cannot run them across that face";
  case ChFi3d_Builder::Failure_Crossing:
    return "the contacts of two roundings cross on a face" +
           (along.empty() ? std::string() : " (one" + along + ")") +
           ", and the geometry kernel cannot cut them by each other";
  case ChFi3d_Builder::Failure_Intersecting:
    return "the rounding" + along + " and the rounding of another edge intersect each other";
  case ChFi3d_Builder::Failure_Corner:
    return corner_failure(maker);
  case ChFi3d_Builder::Failure_PartialCorner:
    return "the geometry kernel builds the corner at a vertex where the roundings meet only in part";
  case ChFi3d_Builder::Failure_None:
    break;
  }
  return maker.NbFaultyVertices() > 0 ? corner_failure(maker) : "the radius may be too large for the edges";
}

// Where OCCT's fillet failed because the roundings of a convex and a
// concave edge overlap on the face between them (a step narrower than the
// two roundings are wide; OCCT runs two overlapping roundings over the face
// as one only where both are convex or both concave): the selected edges of
// the concave contours of such pairs, which the convex roundings come
// before. Empty where the failure is another one.
std::vector<int> concave_after_convex(const BRepFilletAPI_MakeFillet& maker, const Shape& body,
                                      const std::vector<int>& owner) {
  std::vector<int> later;
  if (maker.Failure() != ChFi3d_Builder::Failure_Overlap) {
    return later;
  }
  // The failed contours: whether convex (1), concave (0) or neither (-1)
  // all along, and the faces of their edges.
  struct Failed {
    int contour = 0;
    int kind = -1;
    std::set<int> faces;
  };
  std::vector<Failed> failed;
  for (int i = 1; i <= maker.NbFaultyContours(); ++i) {
    Failed f;
    f.contour = maker.FaultyContour(i);
    for (int ie = 1; ie <= maker.NbEdges(f.contour); ++ie) {
      const int edge = body.edge_index(maker.Edge(f.contour, ie));
      if (edge < 0) {
        f.kind = -2;
        break;
      }
      const auto [a, b] = edge_face_pair(body, edge);
      const ChFiDS_TypeOfConcavity type =
          ChFi3d::DefineConnectType(body.edge(edge), body.face(a), body.face(b), 1.0e-4, false);
      const int kind = type == ChFiDS_Convex ? 1 : type == ChFiDS_Concave ? 0 : -2;
      f.kind = (ie == 1 || f.kind == kind) ? kind : -2;
      f.faces.insert(a);
      f.faces.insert(b);
    }
    failed.push_back(f);
  }
  std::set<int> deferred;
  for (const Failed& concave : failed) {
    if (concave.kind != 0) {
      continue;
    }
    for (const Failed& convex : failed) {
      if (convex.kind == 1 && std::any_of(concave.faces.begin(), concave.faces.end(),
                                          [&](int f) { return convex.faces.count(f) > 0; })) {
        deferred.insert(concave.contour);
        break;
      }
    }
  }
  for (const int ic : deferred) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const int edge = body.edge_index(maker.Edge(ic, ie));
      if (edge >= 0 && owner[static_cast<std::size_t>(edge)] >= 0) {
        later.push_back(edge);
      }
    }
  }
  return later;
}

// The rolling ball fillets of the sets (none asymmetric or G2) that own the
// body's edges, at their sizes times `scale`, their contours added in the
// order the sets list their edges (`listed`) or in the body's.
ShapePtr round_edges(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                     const std::vector<int>& owner, bool rolling_ball_corners, double scale, bool tight,
                     bool listed = true) {
  BRepFilletAPI_MakeFillet maker(body.occt());
  if (tight) {
    maker.SetParams(kTight[0], kTight[1], kTight[2], kTight[3], kTight[4], kTight[5]);
  }
  // OCCT's contours stop where their edges turn, although the faces go on
  // tangentially across a shallow crease there (no other sharp edge at the
  // vertex), unless the next edge is selected too (port patch 0180,
  // mitcad#241): a tangent chain runs on only where the edges continue each
  // other, and a rounding of exactly the selected edges ends there, cut by
  // the face beyond the crease.
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] >= 0) {
      selected.Add(body.edge(edge));
    }
  }
  maker.StopChainsAtTurns(selected);
  // In the order the sets list their edges: where two roundings of a face
  // narrower than both cannot be one blend, the later one rolls on the
  // earlier one's contact line there (port patch 0016), and the bodies the
  // `.f3d` files store make the rounding of the edge listed first the whole
  // one (a plate's rim 2.5 thick rounded with 1.5 on both edges: the two
  // blends meet at 1.5 from the edge listed first, mitcad#343).
  std::vector<int> order = seed_order(body, sets, owner);
  if (!listed) {
    std::sort(order.begin(), order.end());
  }
  for (const int edge : order) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0 || maker.Contour(body.edge(edge)) != 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    const TopoDS_Edge& e = body.edge(edge);
    switch (set.size) {
    case FilletSize::Constant:
      maker.Add(set.radius * scale, e);
      break;
    case FilletSize::ChordLength:
      maker.Add(chord_radius(body, edge, set.chord) * scale, e);
      break;
    case FilletSize::Variable:
      add_variable(maker, body, set, e, scale);
      break;
    case FilletSize::Spline:
      add_spline(maker, body, set, e, scale);
      break;
    case FilletSize::Asymmetric:
      throw std::logic_error("an asymmetric set among the rolling ball fillets");
    }
  }
  // A contour of constant radius that ends at a small notch in the rim it
  // rounds, whose edges lie within the rounding's contact (a notch shallower
  // than the rounding is wide), runs on over the notch onto the next edge of
  // the rim when that edge continues the contour's edge between the same
  // surfaces (port patch 0260, mitcad#307): the rounding is one surface over
  // both edges, cut by the notch's faces, as in the files' roundings, where
  // the edge beyond the notch is rounded although no input names it. The
  // added contours belong to no set (check_contours: -1).
  maker.CarryChainsAcrossNotches();
  const std::vector<int> contour_sets = check_contours(maker, body, sets, owner);
  for (std::size_t c = 0; c < contour_sets.size(); ++c) {
    const int s = contour_sets[c];
    if (s < 0 || sets[static_cast<std::size_t>(s)].size != FilletSize::ChordLength) {
      continue;
    }
    // The chord stays, so the radius would vary with the angle.
    const int ic = static_cast<int>(c) + 1;
    const double radius = maker.Radius(ic);
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const int edge = body.edge_index(maker.Edge(ic, ie));
      if (edge >= 0 &&
          std::abs(chord_radius(body, edge, sets[static_cast<std::size_t>(s)].chord) * scale - radius) >
              1.0e-7 * std::max(1.0, radius)) {
        throw std::invalid_argument("unsupported: a chord length fillet along a chain where "
                                    "the angle between the faces varies");
      }
    }
  }
  // Setback corners: from the rolling ball ones, set back by a multiple of
  // the largest radius that meets there.
  std::vector<detail::Setback> setbacks;
  if (!rolling_ball_corners) {
    for (const TopoDS_Shape& vertex : corner_vertices(maker)) {
      double largest = 0.0;
      for (int ic = 1; ic <= maker.NbContours(); ++ic) {
        for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
          const TopoDS_Edge& edge = maker.Edge(ic, ie);
          if (TopExp::FirstVertex(edge).IsSame(vertex) || TopExp::LastVertex(edge).IsSame(vertex)) {
            const int s = contour_sets[static_cast<std::size_t>(ic - 1)];
            // (A contour carried across a notch: its constant radius.)
            largest = std::max(largest, s < 0 ? maker.Radius(ic)
                                              : largest_radius(body, sets[static_cast<std::size_t>(s)],
                                                               body.edge_index(edge)) *
                                                    scale);
          }
        }
      }
      const auto name = body.name_of_vertex(vertex);
      if (!name || !(largest > 0.0)) {
        throw std::invalid_argument("unsupported: a setback corner at an unnamed vertex");
      }
      setbacks.push_back({*name, kSetback * largest});
    }
  }
  detail::build(maker);
  if (!maker.IsDone() && listed) {
    // OCCT's corners where roundings join over a narrow face depend on which
    // contour comes first (port patch 0700's join, walked from the first
    // end it meets): the contours once more in the body's order, as before
    // they followed the sets' order (mitcad#343).
    std::vector<int> body_order = order;
    std::sort(body_order.begin(), body_order.end());
    if (body_order != order) {
      try {
        OCC_CATCH_SIGNALS
        return round_edges(feature, body, sets, owner, rolling_ball_corners, scale, tight, false);
      } catch (const Cancelled&) {
        throw;
      } catch (...) {
      }
    }
  }
  if (!maker.IsDone()) {
    // A convex and a concave rounding that overlap on the face between
    // them: the convex one first, then the concave one on that result, its
    // ball rolling on the face beyond the remaining strip of the face
    // between, the convex rounding (an S-shaped section whose concave arc
    // touches the convex one, as the files' roundings of such steps are).
    // The concave edges are found again by their names.
    const std::vector<int> later = setbacks.empty() ? concave_after_convex(maker, body, owner) : std::vector<int>();
    if (!later.empty()) {
      std::vector<int> first = owner;
      for (const int edge : later) {
        first[static_cast<std::size_t>(edge)] = -1;
      }
      const ShapePtr convex = round_edges(feature, body, sets, first, rolling_ball_corners, scale, tight);
      std::vector<int> second(static_cast<std::size_t>(convex->edge_count()), -1);
      for (const int edge : later) {
        const std::string& name = body.edge_name(edge);
        const std::vector<int> found = name.empty() ? std::vector<int>() : convex->find_edges(name);
        if (found.size() != 1) {
          throw std::runtime_error("fillet failed; " + fillet_failure(maker, body));
        }
        second[static_cast<std::size_t>(found.front())] = owner[static_cast<std::size_t>(edge)];
      }
      return round_edges(feature, *convex, sets, second, rolling_ball_corners, scale, tight);
    }
    throw std::runtime_error("fillet failed; " + fillet_failure(maker, body));
  }
  const ShapePtr rounded = join_at_turn_stops(name_result(maker, feature, "fillet", body), maker.ChainTurnStops());
  return setbacks.empty() ? rounded : detail::setback_corners(feature, "fillet", body, *rounded, setbacks);
}

// The face of a body edge that a distance is measured on: the face that
// matches `reference`, or without one the first face of the edge's name;
// `flip` takes the other face.
int measured_face(const Shape& body, int edge, const std::string& reference, bool flip) {
  if (reference.empty()) {
    const std::array<int, 2> faces = body.edge_faces(edge);
    const int measured = flip ? faces[1] : faces[0];
    if (measured < 0) {
      throw std::invalid_argument(describe_edge(body, edge) + " has no face to measure on");
    }
    return measured;
  }
  const auto [a, b] = edge_face_pair(body, edge);
  const bool on_a = face_matches(body.face_names(a), reference);
  const bool on_b = face_matches(body.face_names(b), reference);
  if (on_a == on_b) {
    throw std::invalid_argument(describe_edge(body, edge) +
                                (on_a ? " lies between two pieces of face " : " does not border face ") +
                                reference);
  }
  return (on_a != flip) ? a : b;
}

// The order the selected edges are given to the chamfer in. OCCT measures a
// distance-angle or two distances chamfer against one face along the whole
// tangent chain, the face of the edge it is given first (the chain's other
// faces take the same role by adjacency). So the edge a chain is added with
// decides which face of the chain is measured, and the edges' names list
// their faces in no consistent role along a chain: the seed of a chain is the
// first edge its set lists (the file's own first edge), not whichever has the
// lowest index in the body.
std::vector<int> chamfer_seed_order(const Shape& body, const std::vector<ChamferSet>& sets,
                                    const std::vector<int>& owner) {
  std::vector<int> order;
  std::vector<char> placed(static_cast<std::size_t>(body.edge_count()), 0);
  for (std::size_t s = 0; s < sets.size(); ++s) {
    if (sets[s].spec.type == ChamferType::EqualDistance) {
      continue;
    }
    for (int edge : detail::resolve_edges(body, sets[s].edges, sets[s].faces)) {
      char& done = placed[static_cast<std::size_t>(edge)];
      if (owner[static_cast<std::size_t>(edge)] == static_cast<int>(s) && done == 0) {
        done = 1;
        order.push_back(edge);
      }
    }
  }
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (placed[static_cast<std::size_t>(edge)] == 0) {
      order.push_back(edge);
    }
  }
  return order;
}

// Adds the chamfers of the edges the sets own (with `only`, of the edges it
// marks), at their sizes times `scale`.
void add_chamfers(BRepFilletAPI_MakeChamfer& maker, const Shape& body, const std::vector<ChamferSet>& sets,
                  const std::vector<int>& owner, double scale, const std::vector<char>* only = nullptr) {
  for (const int edge : chamfer_seed_order(body, sets, owner)) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0 || (only != nullptr && (*only)[static_cast<std::size_t>(edge)] == 0) ||
        maker.Contour(body.edge(edge)) != 0) {
      continue;
    }
    const ChamferSpec& spec = sets[static_cast<std::size_t>(s)].spec;
    const TopoDS_Edge& e = body.edge(edge);
    if (spec.type == ChamferType::EqualDistance) {
      maker.Add(spec.distance * scale, e);
      continue;
    }
    const int measured = measured_face(body, edge, spec.reference_face, spec.flip);
    if (spec.type == ChamferType::TwoDistances) {
      maker.Add(spec.distance * scale, spec.distance2 * scale, e, body.face(measured));
    } else {
      maker.AddDA(spec.distance * scale, spec.angle, e, body.face(measured));
    }
  }
}

// Whether the material's angle across a body edge is less than a half turn
// (a dressup of it takes material away).
bool convex_edge(const Shape& body, int edge) {
  const auto [a, b] = edge_face_pair(body, edge);
  const ChFiDS_TypeOfConcavity type =
      ChFi3d::DefineConnectType(body.edge(edge), body.face(a), body.face(b), 1.0e-4, false);
  if (type != ChFiDS_Convex && type != ChFiDS_Concave) {
    throw std::invalid_argument("unsupported: a miter corner at " + describe_edge(body, edge) +
                                ", which is neither convex nor concave along its length");
  }
  return type == ChFiDS_Convex;
}

// The chamfer with miter corners: each bevel runs on until it meets the
// others. The contours are bevelled in groups of which no two meet at a
// corner; the material the groups take away is taken away together
// (their results intersected), and the wedges they add at concave edges
// are joined. A corner where convex and concave edges meet is unsupported.
ShapePtr miter(const std::string& feature, const Shape& body, const std::vector<ChamferSet>& sets,
               const std::vector<int>& owner, const BRepFilletAPI_MakeChamfer& all,
               const std::vector<TopoDS_Shape>& corners, double scale) {
  const int contours = all.NbContours();
  std::vector<int> convex(static_cast<std::size_t>(contours) + 1, -1);
  std::vector<std::set<int>> meets(static_cast<std::size_t>(contours) + 1);
  for (const TopoDS_Shape& corner : corners) {
    std::set<int> at;
    for (int ic = 1; ic <= contours; ++ic) {
      for (int ie = 1; ie <= all.NbEdges(ic); ++ie) {
        const TopoDS_Edge& edge = all.Edge(ic, ie);
        if (TopExp::FirstVertex(edge).IsSame(corner) || TopExp::LastVertex(edge).IsSame(corner)) {
          at.insert(ic);
          if (convex[static_cast<std::size_t>(ic)] < 0) {
            const int index = body.edge_index(edge);
            if (index < 0) {
              throw std::runtime_error("a chamfered edge is not an edge of the body");
            }
            convex[static_cast<std::size_t>(ic)] = convex_edge(body, index) ? 1 : 0;
          }
        }
      }
    }
    std::set<int> kinds;
    for (int ic : at) {
      kinds.insert(convex[static_cast<std::size_t>(ic)]);
      meets[static_cast<std::size_t>(ic)].insert(at.begin(), at.end());
      meets[static_cast<std::size_t>(ic)].erase(ic);
    }
    if (kinds.size() > 1) {
      throw std::invalid_argument("unsupported: a miter corner where convex and concave edges meet");
    }
  }
  // Groups by greedy colouring: a contour takes the first group none of
  // the contours it meets is in.
  std::vector<int> group(static_cast<std::size_t>(contours) + 1, -1);
  int groups = 0;
  for (int ic = 1; ic <= contours; ++ic) {
    std::set<int> taken;
    for (int other : meets[static_cast<std::size_t>(ic)]) {
      taken.insert(group[static_cast<std::size_t>(other)]);
    }
    int g = 0;
    while (taken.count(g) > 0) {
      ++g;
    }
    group[static_cast<std::size_t>(ic)] = g;
    groups = std::max(groups, g + 1);
  }
  std::vector<ShapePtr> bevelled;
  for (int g = 0; g < groups; ++g) {
    std::vector<char> only(static_cast<std::size_t>(body.edge_count()), 0);
    for (int ic = 1; ic <= contours; ++ic) {
      if (group[static_cast<std::size_t>(ic)] != g) {
        continue;
      }
      for (int ie = 1; ie <= all.NbEdges(ic); ++ie) {
        const int index = body.edge_index(all.Edge(ic, ie));
        if (index >= 0) {
          only[static_cast<std::size_t>(index)] = 1;
        }
      }
    }
    // Each group on a copy of its own: a chamfer changes its input.
    const ShapePtr own = detail::working_copy(body);
    BRepFilletAPI_MakeChamfer maker(own->occt());
    // (Contours stop where their edges turn, as bevel's.)
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
    for (int edge = 0; edge < own->edge_count(); ++edge) {
      if (owner[static_cast<std::size_t>(edge)] >= 0) {
        selected.Add(own->edge(edge));
      }
    }
    maker.StopChainsAtTurns(selected);
    add_chamfers(maker, *own, sets, owner, scale, &only);
    detail::build(maker);
    if (!maker.IsDone()) {
      throw std::runtime_error("chamfer failed; the distance may be too large for the edges");
    }
    bevelled.push_back(name_result(maker, feature, "chamfer", *own));
  }
  ShapePtr result = bevelled.front();
  for (std::size_t g = 1; g < bevelled.size(); ++g) {
    BooleanResult common = boolean(BooleanOp::Intersect, {result.get()}, *bevelled[g]);
    if (!common.touched.front()) {
      continue; // the group takes nothing away from what is left
    }
    if (common.pieces.size() != 1) {
      throw std::runtime_error("the miter corners split the body");
    }
    result = common.pieces.front().shape;
  }
  for (const ShapePtr& group_result : bevelled) {
    for (const BooleanPiece& wedge : boolean(BooleanOp::Cut, {group_result.get()}, body).pieces) {
      BooleanResult joined = boolean(BooleanOp::Join, {result.get()}, *wedge.shape);
      if (joined.pieces.size() != 1) {
        throw std::runtime_error("the bevels of the miter corners could not be joined");
      }
      result = joined.pieces.front().shape;
    }
  }
  return result;
}

// The distance from the edge at which a G2 set touches the faces: where
// the circular rounding of its radius (or chord) would, r tan(a / 2) with a
// the angle between the faces' normals at the middle of the edge.
double contact_distance(const Shape& body, int edge, const FilletSet& set) {
  const auto [a, b] = edge_face_pair(body, edge);
  const double angle = detail::normal_angle(body.edge(edge), body.face(a), body.face(b));
  if (angle < 1.0e-3) {
    throw std::invalid_argument(describe_edge(body, edge) + " joins its faces tangentially");
  }
  if (set.size == FilletSize::ChordLength) {
    return set.chord / (2.0 * std::cos(angle / 2.0));
  }
  return set.radius * std::tan(angle / 2.0);
}

// Fillets with asymmetric or G2 sets (`blended`): the other sets' rolling
// ball fillets first, then a chamfer of the blended sets at their contact
// distances, its bevels replaced by blend surfaces. A blended chain that
// meets another rounded edge at a vertex is unsupported.
ShapePtr round_and_blend(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                         const std::vector<int>& owner, const std::vector<char>& blended,
                         bool rolling_ball_corners, double scale, bool tight) {
  // The chains of all sets, to see where the blended ones end.
  BRepFilletAPI_MakeChamfer probe(body.occt());
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] >= 0 && probe.Contour(body.edge(edge)) == 0) {
      probe.Add(1.0, body.edge(edge));
    }
  }
  const std::vector<int> contour_sets = check_contours(probe, body, sets, owner);
  std::vector<ShapeMap> ends(contour_sets.size());
  for (std::size_t c = 0; c < contour_sets.size(); ++c) {
    const int ic = static_cast<int>(c) + 1;
    for (int ie = 1; ie <= probe.NbEdges(ic); ++ie) {
      TopExp::MapShapes(probe.Edge(ic, ie), TopAbs_VERTEX, ends[c]);
    }
  }
  for (std::size_t c = 0; c < contour_sets.size(); ++c) {
    if (contour_sets[c] < 0 || blended[static_cast<std::size_t>(contour_sets[c])] == 0) {
      continue;
    }
    for (std::size_t other = 0; other < contour_sets.size(); ++other) {
      for (int v = 1; other != c && v <= ends[other].Extent(); ++v) {
        if (ends[c].Contains(ends[other](v))) {
          throw std::invalid_argument("unsupported: asymmetric and curvature continuous fillets that "
                                      "meet other rounded edges at a corner");
        }
      }
    }
  }
  // The rolling ball sets.
  ShapePtr rounded;
  std::vector<int> plain = owner;
  bool any_plain = false;
  for (int& s : plain) {
    if (s >= 0 && blended[static_cast<std::size_t>(s)] != 0) {
      s = -1;
    }
    any_plain = any_plain || s >= 0;
  }
  if (any_plain) {
    rounded = round_edges(feature, body, sets, plain, rolling_ball_corners, scale, tight);
  }
  const Shape& start = rounded ? *rounded : body;
  // The blended sets' chamfer, on the same edges by name.
  BRepFilletAPI_MakeChamfer maker(start.occt());
  std::map<std::string, int> set_of_edge;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0 || blended[static_cast<std::size_t>(s)] == 0) {
      continue;
    }
    const std::string& name = body.edge_name(edge);
    const std::vector<int> found = name.empty() ? std::vector<int>() : start.find_edges(name);
    if (found.size() != 1) {
      throw std::invalid_argument("unsupported: an asymmetric or curvature continuous fillet of an "
                                  "edge the other fillets change");
    }
    const int e = found.front();
    set_of_edge[name] = s;
    if (maker.Contour(start.edge(e)) != 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    switch (set.size) {
    case FilletSize::Asymmetric:
      maker.Add(set.radius * scale, set.radius2 * scale, start.edge(e),
                start.face(measured_face(start, e, set.reference_face, set.flip)));
      break;
    case FilletSize::Constant:
    case FilletSize::ChordLength:
      maker.Add(contact_distance(start, e, set) * scale, start.edge(e));
      break;
    case FilletSize::Variable:
    case FilletSize::Spline:
      throw std::invalid_argument("unsupported: curvature continuous fillets with a variable radius");
    }
  }
  // Every edge of the chains, with its set's cross-section.
  std::map<std::string, detail::BlendSection> kinds;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    int s = -1;
    for (int ie = 1; ie <= maker.NbEdges(ic) && s < 0; ++ie) {
      if (const auto name = start.name_of_edge(maker.Edge(ic, ie))) {
        const auto found = set_of_edge.find(*name);
        s = found == set_of_edge.end() ? -1 : found->second;
      }
    }
    if (s < 0) {
      throw std::runtime_error("a bevelled chain has no edge set");
    }
    detail::BlendSection kind;
    kind.curvature = sets[static_cast<std::size_t>(s)].curvature;
    kind.weight = sets[static_cast<std::size_t>(s)].weight;
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      if (const auto name = start.name_of_edge(maker.Edge(ic, ie))) {
        kinds[*name] = kind;
      }
    }
  }
  detail::build(maker);
  if (!maker.IsDone()) {
    throw std::runtime_error("fillet failed; the distances may be too large for the edges");
  }
  const ShapePtr chamfered = name_result(maker, feature, "fillet", start);
  return detail::blend_bevels(feature, start, *chamfered, kinds);
}

// The note on a dressup built as rings.
std::string ring_note(const char* role) {
  return std::string("the ") + role +
         " is built as a ring about its circle: the geometry kernel's own stops where it runs over a "
         "neighbouring face";
}

// The fillet as rings about whole circles (ring_dressup.hpp), for constant
// radius and chord length sets; null when it cannot be built so.
ShapePtr ring_fillet(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                     const std::vector<int>& owner) {
  std::vector<detail::RingEdge> rings;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    if (set.curvature) {
      return nullptr;
    }
    detail::RingEdge ring;
    ring.edge = edge;
    switch (set.size) {
    case FilletSize::Constant:
      ring.radius = set.radius;
      break;
    case FilletSize::ChordLength:
      ring.radius = chord_radius(body, edge, set.chord);
      break;
    case FilletSize::Variable:
    case FilletSize::Asymmetric:
    case FilletSize::Spline:
      return nullptr;
    }
    rings.push_back(ring);
  }
  ShapePtr result = detail::ring_dressup(feature, "fillet", body, rings);
  if (result) {
    result->add_note(ring_note("fillet"));
  }
  return result;
}

// The fillet's roundings as mitred sweeps (mitred_dressup.hpp): roundings
// of constant radius that take a face away, round a loop of edges; null
// when they are not such roundings or cannot be built so.
ShapePtr mitred_roundings(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                          const std::vector<int>& owner) {
  std::vector<detail::MitredEdge> edges;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    if (set.curvature || set.size != FilletSize::Constant) {
      return nullptr;
    }
    edges.push_back({edge, set.radius});
  }
  return detail::mitred_fillet(feature, body, edges);
}

// Runs `dressup` (OCCT's fillet at its exact size); where the geometry
// kernel fails (not for invalid arguments), the roundings are built as
// mitred sweeps if they are roundings that take a face away round a loop of
// edges, whose corners OCCT cannot join (mitcad#1118), else the kernel's
// error stands.
template <class Dressup>
ShapePtr or_mitred(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                   const std::vector<int>& owner, Dressup&& dressup) {
  std::exception_ptr first;
  try {
    OCC_CATCH_SIGNALS
    return dressup();
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    first = std::current_exception();
  }
  ShapePtr mitred;
  try {
    OCC_CATCH_SIGNALS
    mitred = mitred_roundings(feature, *detail::working_copy(body), sets, owner);
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    mitred = nullptr;
  }
  if (mitred) {
    return mitred;
  }
  std::rethrow_exception(first);
}

// The chamfer as rings about whole circles; null when it cannot be built so.
ShapePtr ring_chamfer(const std::string& feature, const Shape& body, const std::vector<ChamferSet>& sets,
                      const std::vector<int>& owner) {
  std::vector<detail::RingEdge> rings;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0) {
      continue;
    }
    const ChamferSpec& spec = sets[static_cast<std::size_t>(s)].spec;
    detail::RingEdge ring;
    ring.edge = edge;
    ring.fillet = false;
    ring.distance = spec.distance;
    ring.distance2 = spec.distance;
    if (spec.type != ChamferType::EqualDistance) {
      ring.face = measured_face(body, edge, spec.reference_face, spec.flip);
      if (spec.type == ChamferType::TwoDistances) {
        ring.distance2 = spec.distance2;
      } else {
        ring.angle = spec.angle;
      }
    }
    rings.push_back(ring);
  }
  ShapePtr result = detail::ring_dressup(feature, "chamfer", body, rings);
  if (result) {
    result->add_note(ring_note("chamfer"));
  }
  return result;
}

// Runs a dressup; when the geometry kernel fails to build it (not for
// invalid arguments), `rings` builds it as rings on a copy of the input
// if it can, else the kernel's error stands.
template <class Dressup, class Rings>
ShapePtr or_rings(const char* operation, const Shape& input, Dressup&& dressup, Rings&& rings) {
  try {
    return detail::run(operation, dressup);
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (const std::runtime_error&) {
    ShapePtr ringed;
    try {
      ringed = detail::run(operation, [&] { return rings(*detail::working_copy(input)); });
    } catch (const Cancelled&) {
      throw;
    } catch (const std::exception&) {
      ringed = nullptr;
    }
    if (ringed) {
      return ringed;
    }
    throw;
  }
}

const char* corner_name(ChamferCorner corner) {
  switch (corner) {
  case ChamferCorner::Chamfer:
    return "chamfer";
  case ChamferCorner::Miter:
    return "miter";
  case ChamferCorner::Blend:
    return "blend";
  }
  return "chamfer";
}

// The chamfer of the edges the sets own at their sizes times `scale`, with
// the corner type's treatment.
ShapePtr bevel(const std::string& feature, const Shape& body, const std::vector<ChamferSet>& sets,
               const std::vector<int>& owner, ChamferCorner corner, double scale) {
  BRepFilletAPI_MakeChamfer maker(body.occt());
  // As the fillets' (round_edges): OCCT's contours stop where their edges
  // turn, unless the next edge is selected too, and a chamfer of exactly
  // the selected edges ends there, its analytic blend running on past the
  // vertex cut by the face beyond the crease (port patches 0180, 0531).
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] >= 0) {
      selected.Add(body.edge(edge));
    }
  }
  maker.StopChainsAtTurns(selected);
  add_chamfers(maker, body, sets, owner, scale);
  const std::vector<int> contour_sets = check_contours(maker, body, sets, owner);
  const std::vector<TopoDS_Shape> corners =
      corner == ChamferCorner::Chamfer ? std::vector<TopoDS_Shape>() : corner_vertices(maker);
  if (corner == ChamferCorner::Miter && !corners.empty()) {
    return miter(feature, body, sets, owner, maker, corners, scale);
  }
  // Blend corners: OCCT's corner faces set back by a multiple of the
  // largest distance that meets there, a patch in their place.
  std::vector<detail::Setback> setbacks;
  for (const TopoDS_Shape& vertex : corners) {
    double largest = 0.0;
    for (int ic = 1; ic <= maker.NbContours(); ++ic) {
      const int s = contour_sets[static_cast<std::size_t>(ic - 1)];
      for (int ie = 1; ie <= maker.NbEdges(ic) && s >= 0; ++ie) {
        const TopoDS_Edge& edge = maker.Edge(ic, ie);
        if (TopExp::FirstVertex(edge).IsSame(vertex) || TopExp::LastVertex(edge).IsSame(vertex)) {
          largest = std::max(largest, largest_distance(sets[static_cast<std::size_t>(s)].spec) * scale);
        }
      }
    }
    const auto name = body.name_of_vertex(vertex);
    if (!name || !(largest > 0.0)) {
      throw std::invalid_argument("unsupported: a blend corner at an unnamed vertex");
    }
    setbacks.push_back({*name, kSetback * largest});
  }
  detail::build(maker);
  if (!maker.IsDone()) {
    throw std::runtime_error("chamfer failed; the distance may be too large for the edges");
  }
  ShapePtr result = join_at_turn_stops(name_result(maker, feature, "chamfer", body), maker.ChainTurnStops());
  result = join_at_turn_stops(result, piece_vertices(*result, body));
  if (!setbacks.empty()) {
    return detail::setback_corners(feature, "chamfer", body, *result, setbacks);
  }
  if (corner != ChamferCorner::Chamfer) {
    result->add_note(std::string("the ") + corner_name(corner) +
                     " corner type shapes nothing here: no three bevelled edges meet at a vertex");
  }
  return result;
}

// A body with the pieces of its edges merged, with the edge sets carried to
// the merged edges: OCCT's ShapeUpgrade_UnifySameDomain on edges only joins
// the edges between the same two faces that continue each other smoothly
// at a vertex no other edge meets (a circle cut into two arcs, a line into
// pieces). The faces stay as they are. OCCT's dressups fail on some chains
// of such pieces (a whole circle in two arcs, mitcad#107) and build the
// merged edge. `names`: per merged edge, the names of the selected pieces
// in it.
struct MergedEdges {
  ShapePtr body;
  std::vector<int> owner;
  std::map<int, std::vector<std::string>> names;
};

// Whether faces `a` and `b` lie on one surface: points inside `b` lie on
// `a`'s surface (within the confusion tolerance) and their normals agree
// there.
bool on_one_surface(const TopoDS_Face& a, const TopoDS_Face& b) {
  const occ::handle<Geom_Surface> sa = BRep_Tool::Surface(a);
  if (sa.IsNull()) {
    return false;
  }
  const BRepAdaptor_Surface sb(b);
  if (sb.GetType() != BRepAdaptor_Surface(a).GetType()) {
    return false;
  }
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(b, u0, u1, v0, v1);
  for (const double fu : {0.25, 0.5, 0.75}) {
    for (const double fv : {0.25, 0.5, 0.75}) {
      gp_Pnt p;
      gp_Vec du, dv;
      sb.D1(u0 + (u1 - u0) * fu, v0 + (v1 - v0) * fv, p, du, dv);
      GeomAPI_ProjectPointOnSurf on(p, sa);
      if (on.NbPoints() == 0 || on.LowerDistance() > Precision::Confusion()) {
        return false;
      }
      double u = 0.0, v = 0.0;
      on.LowerDistanceParameters(u, v);
      gp_Pnt q;
      gp_Vec da, db;
      sa->D1(u, v, q, da, db);
      gp_Vec nb = du.Crossed(dv), na = da.Crossed(db);
      if (nb.Magnitude() <= gp::Resolution() || na.Magnitude() <= gp::Resolution()) {
        continue;
      }
      if (b.Orientation() == TopAbs_REVERSED) {
        nb.Reverse();
      }
      if (a.Orientation() == TopAbs_REVERSED) {
        na.Reverse();
      }
      if (na.Angle(nb) > 1.0e-3) {
        return false;
      }
    }
  }
  return true;
}

// None when no selected edge merges with another, a merged edge holds
// pieces of two sets, or it holds a piece no set selects where its set does
// not follow tangent chains. With `faces`, the faces of one surface that
// meet along an edge at a vertex of the selected edges are joined as well
// (the edges between such faces elsewhere stay): none where there are no
// such faces.
template <class Set>
std::optional<MergedEdges> merge_edge_pieces(const Shape& body, const std::vector<Set>& sets,
                                             const std::vector<int>& owner, bool faces = false) {
  // First a quick look for a selected edge with an end where only one
  // other edge, between the same two faces, meets it.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edges_of;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_of);
  // The faces of every edge mapped once (faces_at_edge maps the whole body
  // for each edge: seconds per rounding of many edges on a large body).
  const detail::EdgeFaces edge_faces(body);
  // With `faces`: the edges, not selected themselves, along which a face at
  // a selected edge's vertex meets another face of its surface.
  ShapeMap joins;
  if (faces) {
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_FACE, faces_of);
    ShapeMap seen;
    for (int e = 0; e < body.edge_count(); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      for (const TopoDS_Shape& vertex : {TopExp::FirstVertex(body.edge(e)), TopExp::LastVertex(body.edge(e))}) {
        const int index = faces_of.FindIndex(vertex);
        if (index == 0) {
          continue;
        }
        for (const TopoDS_Shape& face : faces_of(index)) {
          if (seen.Contains(face)) {
            continue;
          }
          seen.Add(face);
          for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
            const int o = body.edge_index(ex.Current());
            if (o < 0 || owner[static_cast<std::size_t>(o)] >= 0 || joins.Contains(ex.Current()) ||
                BRep_Tool::Degenerated(TopoDS::Edge(ex.Current()))) {
              continue;
            }
            const std::vector<int> two = edge_faces.at(o);
            if (two.size() == 2 && two[0] != two[1] && on_one_surface(body.face(two[0]), body.face(two[1])) &&
                on_one_surface(body.face(two[1]), body.face(two[0]))) {
              joins.Add(ex.Current());
            }
          }
        }
      }
    }
  }
  if (faces && joins.IsEmpty()) {
    return std::nullopt;
  }
  bool pieces_meet = faces;
  for (int e = 0; e < body.edge_count() && !pieces_meet; ++e) {
    if (owner[static_cast<std::size_t>(e)] < 0) {
      continue;
    }
    const auto faces_of = [&](int edge) {
      std::vector<int> sorted = edge_faces.at(edge);
      std::sort(sorted.begin(), sorted.end());
      return sorted;
    };
    const std::vector<int> own = faces_of(e);
    for (const TopoDS_Shape& vertex : {TopExp::FirstVertex(body.edge(e)), TopExp::LastVertex(body.edge(e))}) {
      const int index = edges_of.FindIndex(vertex);
      if (index == 0 || edges_of(index).Extent() != 2) {
        continue;
      }
      for (const TopoDS_Shape& other : edges_of(index)) {
        const int o = body.edge_index(other);
        pieces_meet = pieces_meet || (o >= 0 && o != e && faces_of(o) == own);
      }
    }
  }
  if (!pieces_meet) {
    return std::nullopt;
  }
  ShapeUpgrade_UnifySameDomain unify(body.occt(), true, faces, false);
  if (faces) {
    // (Only the faces along the joins: every other edge keeps its faces apart.)
    for (int e = 0; e < body.edge_count(); ++e) {
      if (!joins.Contains(body.edge(e))) {
        unify.KeepShape(body.edge(e));
      }
    }
  }
  unify.Build();
  const occ::handle<BRepTools_History>& history = unify.History();
  // The merged edge of each edge of the body (-1 for none).
  bool merges = false;
  MergedEdges merged;
  detail::FaceNamer namer(unify.Shape());
  namer.carry(*history, body);
  namer.finish();
  merged.body = namer.shape();
  const Shape& m = *merged.body;
  std::vector<int> image(static_cast<std::size_t>(body.edge_count()), -1);
  std::vector<int> pieces(static_cast<std::size_t>(m.edge_count()), 0);
  for (int e = 0; e < body.edge_count(); ++e) {
    const TopoDS_Shape& edge = body.edge(e);
    if (history->IsRemoved(edge)) {
      continue;
    }
    const NCollection_List<TopoDS_Shape>& modified = history->Modified(edge);
    const int i = m.edge_index(modified.IsEmpty() ? edge : modified.First());
    if (i < 0 || modified.Extent() > 1) {
      return std::nullopt;
    }
    image[static_cast<std::size_t>(e)] = i;
    ++pieces[static_cast<std::size_t>(i)];
  }
  merged.owner.assign(static_cast<std::size_t>(m.edge_count()), -1);
  std::vector<char> unselected(static_cast<std::size_t>(m.edge_count()), 0);
  for (int e = 0; e < body.edge_count(); ++e) {
    const int i = image[static_cast<std::size_t>(e)];
    const int s = owner[static_cast<std::size_t>(e)];
    if (i < 0) {
      if (s >= 0) {
        return std::nullopt; // a selected edge gone
      }
      continue;
    }
    if (s < 0) {
      unselected[static_cast<std::size_t>(i)] = 1;
      continue;
    }
    int& own = merged.owner[static_cast<std::size_t>(i)];
    if (own >= 0 && own != s) {
      return std::nullopt;
    }
    own = s;
    merges = merges || pieces[static_cast<std::size_t>(i)] > 1;
    if (!body.edge_name(e).empty()) {
      merged.names[i].push_back(body.edge_name(e));
    }
  }
  for (int i = 0; i < m.edge_count(); ++i) {
    const int s = merged.owner[static_cast<std::size_t>(i)];
    if (s >= 0 && unselected[static_cast<std::size_t>(i)] != 0 &&
        !sets[static_cast<std::size_t>(s)].tangent_chain) {
      return std::nullopt;
    }
  }
  if (faces) {
    merges = m.face_count() < body.face_count();
  }
  if (!merges) {
    return std::nullopt;
  }
  return merged;
}

// The import's geometric tolerance (mitcad#224): stored bodies of imported
// designs hold faces narrower than this between two others (a band of
// 0.5 micrometres between two chamfers that meet in the design), which the
// files' own roundings treat as the one edge it is within the tolerance.
constexpr double kStripWidth = 0.01;

// The length of an edge's curve.
double edge_length(const TopoDS_Edge& edge) {
  if (BRep_Tool::Degenerated(edge)) {
    return 0.0;
  }
  GProp_GProps props;
  BRepGProp::LinearProperties(edge, props);
  return props.Mass();
}

// Whether every point of edge `a` lies within `width` of edge `b` (sampled).
bool within_of(const TopoDS_Edge& a, const TopoDS_Edge& b, double width) {
  const BRepAdaptor_Curve curve(a);
  constexpr int kSamples = 8;
  for (int i = 0; i <= kSamples; ++i) {
    const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / kSamples;
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(curve.Value(t)).Vertex(), b);
    if (!distance.IsDone() || distance.Value() > width) {
      return false;
    }
  }
  return true;
}

// A face that is a strip narrower than kStripWidth: four edges, two opposite
// short ones (each at most that long, joining the long ones' ends) and two
// long ones within that width of each other all along. `ends`: the short
// edges; `sides`: the long ones.
struct Strip {
  TopoDS_Face face;
  std::array<TopoDS_Edge, 2> ends;
  std::array<TopoDS_Edge, 2> sides;
};

std::optional<Strip> as_strip(const TopoDS_Face& face) {
  std::vector<TopoDS_Edge> ends, sides;
  for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(edge) || BRep_Tool::IsClosed(edge, face)) {
      return std::nullopt;
    }
    (edge_length(edge) <= kStripWidth ? ends : sides).push_back(edge);
  }
  if (ends.size() != 2 || sides.size() != 2) {
    return std::nullopt;
  }
  // Each end joins a vertex of one side to a vertex of the other.
  for (const TopoDS_Edge& end : ends) {
    TopoDS_Vertex a, b;
    TopExp::Vertices(end, a, b);
    const auto on = [](const TopoDS_Vertex& v, const TopoDS_Edge& e) {
      return v.IsSame(TopExp::FirstVertex(e)) || v.IsSame(TopExp::LastVertex(e));
    };
    if (a.IsSame(b) || !((on(a, sides[0]) && on(b, sides[1])) || (on(a, sides[1]) && on(b, sides[0])))) {
      return std::nullopt;
    }
  }
  if (!within_of(sides[0], sides[1], kStripWidth) || !within_of(sides[1], sides[0], kStripWidth)) {
    return std::nullopt;
  }
  return Strip{face, {ends[0], ends[1]}, {sides[0], sides[1]}};
}

// The edge a strip stands for: where the surfaces of the faces beyond its
// two sides meet, from the end at one of its ends to the other, with curves
// on both faces. None where they do not meet within the strip's width of
// it (a step between parallel faces is no crease) or the ends do not lie
// on the meeting curve within that width.
std::optional<TopoDS_Edge> crease_of(
    const Strip& strip,
    const NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>&
        faces_of) {
  std::array<TopoDS_Face, 2> beyond;
  for (int i = 0; i < 2; ++i) {
    for (const TopoDS_Shape& f : faces_of.FindFromKey(strip.sides[static_cast<std::size_t>(i)])) {
      if (!f.IsSame(strip.face)) {
        beyond[static_cast<std::size_t>(i)] = TopoDS::Face(f);
      }
    }
    if (beyond[static_cast<std::size_t>(i)].IsNull()) {
      return std::nullopt;
    }
  }
  if (beyond[0].IsSame(beyond[1])) {
    return std::nullopt;
  }
  // The middles of the ends and of the first side.
  std::array<gp_Pnt, 2> ends;
  for (int k = 0; k < 2; ++k) {
    TopoDS_Vertex a, b;
    TopExp::Vertices(strip.ends[static_cast<std::size_t>(k)], a, b);
    ends[static_cast<std::size_t>(k)] = gp_Pnt(0.5 * (BRep_Tool::Pnt(a).XYZ() + BRep_Tool::Pnt(b).XYZ()));
  }
  const BRepAdaptor_Curve side(strip.sides[0]);
  const gp_Pnt middle = side.Value(0.5 * (side.FirstParameter() + side.LastParameter()));
  GeomAPI_IntSS meeting(BRep_Tool::Surface(beyond[0]), BRep_Tool::Surface(beyond[1]), Precision::Confusion());
  if (!meeting.IsDone()) {
    return std::nullopt;
  }
  occ::handle<Geom_Curve> curve;
  double nearest = kStripWidth;
  for (int i = 1; i <= meeting.NbLines(); ++i) {
    GeomAPI_ProjectPointOnCurve on(middle, meeting.Line(i));
    if (on.NbPoints() > 0 && on.LowerDistance() <= nearest) {
      nearest = on.LowerDistance();
      curve = meeting.Line(i);
    }
  }
  if (curve.IsNull()) {
    return std::nullopt;
  }
  const auto parameter = [&](const gp_Pnt& p, double& t) {
    GeomAPI_ProjectPointOnCurve on(p, curve);
    if (on.NbPoints() == 0 || on.LowerDistance() > kStripWidth) {
      return false;
    }
    t = on.LowerDistanceParameter();
    return true;
  };
  double t0 = 0.0, t1 = 0.0, tm = 0.0;
  if (!parameter(ends[0], t0) || !parameter(ends[1], t1) || !parameter(middle, tm)) {
    return std::nullopt;
  }
  if (curve->IsPeriodic()) {
    // The way from the first end through the middle to the second.
    const double period = curve->Period();
    t1 = t0 + ElCLib::InPeriod(t1 - t0, 0.0, period);
    tm = t0 + ElCLib::InPeriod(tm - t0, 0.0, period);
    if (tm > t1) {
      t1 -= period;
    }
  }
  if (std::abs(t1 - t0) < Precision::PConfusion()) {
    return std::nullopt;
  }
  const double lo = std::min(t0, t1), hi = std::max(t0, t1);
  BRepBuilderAPI_MakeEdge make(curve, lo, hi);
  if (!make.IsDone()) {
    return std::nullopt;
  }
  TopoDS_Edge crease = make.Edge();
  ShapeFix_Edge fix;
  for (const TopoDS_Face& face : beyond) {
    fix.FixAddPCurve(crease, face, false, Precision::Confusion());
    double first = 0.0, last = 0.0;
    if (BRep_Tool::CurveOnSurface(crease, face, first, last).IsNull()) {
      return std::nullopt;
    }
  }
  fix.FixSameParameter(crease);
  return crease;
}

// A body with the strips at the selected edges' vertices collapsed into
// edges, within the import's tolerance (mitcad#299): the faces beyond a
// strip's two sides are made to meet in the crease the strip stands for
// (where their surfaces meet), the strip's sides replaced by it, the
// vertices of each end merged into the crease's vertex there and the ends
// and the strip removed, so that the edges that ended at the two vertices
// of an end meet at one, as in the design. The edge sets are carried to the
// edges' images. None when no strip touches a selected edge, a strip's
// edge is selected, or a strip stands for no crease.
std::optional<MergedEdges> collapse_strips(const Shape& body, const std::vector<int>& owner) {
  ShapeMap vertices;
  ShapeMap selected;
  for (int e = 0; e < body.edge_count(); ++e) {
    if (owner[static_cast<std::size_t>(e)] >= 0) {
      TopExp::MapShapes(body.edge(e), TopAbs_VERTEX, vertices);
      selected.Add(body.edge(e));
    }
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  std::vector<Strip> strips;
  ShapeMap taken; // edges of the strips found so far
  for (int f = 0; f < body.face_count(); ++f) {
    const TopoDS_Face& face = body.face(f);
    bool touches = false;
    for (TopExp_Explorer ex(face, TopAbs_VERTEX); ex.More() && !touches; ex.Next()) {
      touches = vertices.Contains(ex.Current());
    }
    if (!touches) {
      continue;
    }
    std::optional<Strip> strip = as_strip(face);
    if (!strip) {
      continue;
    }
    bool free = true;
    for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More() && free; ex.Next()) {
      const int found = faces_of.FindIndex(ex.Current());
      free = !selected.Contains(ex.Current()) && !taken.Contains(ex.Current()) && found != 0 &&
             faces_of(found).Extent() == 2;
    }
    if (!free) {
      return std::nullopt;
    }
    TopExp::MapShapes(face, TopAbs_EDGE, taken);
    strips.push_back(*strip);
  }
  if (strips.empty()) {
    return std::nullopt;
  }
  const occ::handle<ShapeBuild_ReShape> reshape = new ShapeBuild_ReShape;
  BRep_Builder builder;
  for (const Strip& strip : strips) {
    const std::optional<TopoDS_Edge> crease = crease_of(strip, faces_of);
    if (!crease) {
      return std::nullopt;
    }
    // The crease's vertices take the places of the ends' vertices, the
    // crease those of the sides.
    const TopoDS_Vertex start = TopExp::FirstVertex(*crease);
    const TopoDS_Vertex finish = TopExp::LastVertex(*crease);
    const gp_Pnt at_start = BRep_Tool::Pnt(start);
    for (const TopoDS_Edge& end : strip.ends) {
      TopoDS_Vertex a, b;
      TopExp::Vertices(end, a, b);
      const double to_start = BRep_Tool::Pnt(a).Distance(at_start) + BRep_Tool::Pnt(b).Distance(at_start);
      const double to_finish = BRep_Tool::Pnt(a).Distance(BRep_Tool::Pnt(finish)) +
                               BRep_Tool::Pnt(b).Distance(BRep_Tool::Pnt(finish));
      const TopoDS_Vertex& image = to_start < to_finish ? start : finish;
      for (const TopoDS_Vertex& v : {a, b}) {
        builder.UpdateVertex(image, std::max(BRep_Tool::Tolerance(image),
                                             BRep_Tool::Pnt(v).Distance(BRep_Tool::Pnt(image)) +
                                                 BRep_Tool::Tolerance(v) + Precision::Confusion()));
        reshape->Replace(v, image.Oriented(v.Orientation()));
      }
      reshape->Remove(end);
    }
    for (const TopoDS_Edge& side : strip.sides) {
      // The side runs as the crease where its first vertex is nearer the
      // crease's start.
      const gp_Pnt first = BRep_Tool::Pnt(TopExp::FirstVertex(side));
      const bool along = first.Distance(at_start) < first.Distance(BRep_Tool::Pnt(finish));
      reshape->Replace(side, crease->Oriented(along ? side.Orientation() : TopAbs::Reverse(side.Orientation())));
    }
    reshape->Remove(strip.face);
  }
  // Shape healing puts the creases' curves on the faces' periodic surfaces
  // in the periods of the faces' other edges (in place: the body is a copy
  // whose p-curves are the input's, detail::own_pcurves).
  ShapeFix_Shape fixer;
  const occ::handle<ShapeBuild_ReShape> fixes = new ShapeBuild_ReShape;
  fixer.SetContext(fixes);
  const TopoDS_Shape reshaped = reshape->Apply(body.occt());
  detail::own_pcurves(reshaped);
  fixer.Init(reshaped);
  fixer.Perform();
  const TopoDS_Shape collapsed = fixer.Shape();
  if (!BRepCheck_Analyzer(collapsed).IsValid()) {
    return std::nullopt;
  }
  const occ::handle<BRepTools_History> history = reshape->History();
  history->Merge(fixes->History());
  MergedEdges merged;
  detail::FaceNamer namer(collapsed);
  namer.carry(*history, body);
  namer.finish();
  merged.body = namer.shape();
  const Shape& m = *merged.body;
  merged.owner.assign(static_cast<std::size_t>(m.edge_count()), -1);
  for (int e = 0; e < body.edge_count(); ++e) {
    const int s = owner[static_cast<std::size_t>(e)];
    if (s < 0) {
      continue;
    }
    const TopoDS_Shape& edge = body.edge(e);
    const NCollection_List<TopoDS_Shape>& modified = history->Modified(edge);
    const int i = history->IsRemoved(edge) || modified.Extent() > 1
                      ? -1
                      : m.edge_index(modified.IsEmpty() ? edge : modified.First());
    if (i < 0) {
      return std::nullopt; // a selected edge gone
    }
    merged.owner[static_cast<std::size_t>(i)] = s;
  }
  return merged;
}

// A dressup of a body with merged edges, its faces also named after the
// selected pieces: a face generated from a merged edge gets the names the
// pieces' dressup would have given it (with its own "#k" of a split face).
ShapePtr with_piece_names(const ShapePtr& result, const MergedEdges& merged, const std::string& feature,
                          const char* role) {
  std::map<std::string, std::vector<std::string>> extra;
  for (const auto& [edge, names] : merged.names) {
    const std::string& own = merged.body->edge_name(edge);
    if (own.empty()) {
      continue;
    }
    for (const std::string& name : names) {
      if (name != own) {
        extra[face_name(feature, role, own)].push_back(face_name(feature, role, name));
      }
    }
  }
  std::vector<Shape::NamedFace> faces;
  for (int f = 0; f < result->face_count(); ++f) {
    NameList names = result->face_names(f);
    for (const std::string& name : result->face_names(f)) {
      const std::size_t hash = name.rfind('#');
      const bool piece = hash != std::string::npos && name.rfind(')') < hash;
      const auto found = extra.find(piece ? name.substr(0, hash) : name);
      if (found != extra.end()) {
        for (const std::string& other : found->second) {
          names.push_back(piece ? other + name.substr(hash) : other);
        }
      }
    }
    std::sort(names.begin(), names.end());
    names.erase(std::unique(names.begin(), names.end()), names.end());
    faces.push_back({result->face(f), names});
  }
  ShapePtr named = std::make_shared<Shape>(result->occt(), faces);
  named->copy_notes(*result);
  return named;
}

// The real dressup (`dressup(body, owner, scale, tight)`) on a copy of the
// input. When OCCT fails at the sizes as asked, it is built again by the
// same algorithm on the input with the pieces of its edges merged
// (merge_edge_pieces), then, with `tighten` (fillets), with OCCT's
// tolerances tightened (kTight), on the input and on the merged one, then
// on the input with the faces of one surface at its edges' vertices joined
// as well (merge_edge_pieces with faces), and last on the input with the
// strips narrower than the import's tolerance at its vertices collapsed
// (collapse_strips).
// Throws the first error when none builds.
template <class Set, class Dressup>
ShapePtr dress(const Shape& input, const std::vector<Set>& sets, const std::vector<int>& owner, double scale,
               const std::string& feature, const char* role, bool tighten, Dressup&& dressup) {
  std::exception_ptr first;
  try {
    OCC_CATCH_SIGNALS
    // OCCT's dressups give the edges of their input p-curves on the new
    // surfaces and wider tolerances (input_check.hpp): each try works on a
    // copy of its own.
    const ShapePtr copy = detail::working_copy(input);
    return dressup(*copy, owner, scale, false);
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    if (scale != 1.0) {
      throw;
    }
    first = std::current_exception();
  }
  const RetryScope retry;
  // Tighter tolerances only help where faces fold back on each other along
  // a selected edge (a knife edge): elsewhere they would only cost the time
  // of another try.
  bool knife = false;
  std::optional<detail::EdgeFaces> edge_faces;
  for (int e = 0; tighten && e < input.edge_count() && !knife; ++e) {
    if (owner[static_cast<std::size_t>(e)] < 0) {
      continue;
    }
    if (!edge_faces) {
      edge_faces.emplace(input);
    }
    const std::vector<int> faces = edge_faces->at(e);
    try {
      OCC_CATCH_SIGNALS
      knife = faces.size() == 2 &&
              detail::normal_angle(input.edge(e), input.face(faces[0]), input.face(faces[1])) > kKnife;
    } catch (const Standard_Failure&) {
    }
  }
  tighten = tighten && knife;
  bool merges = true;
  for (const bool tight : {false, true}) {
    if (tight && tighten) {
      try {
        OCC_CATCH_SIGNALS
        const ShapePtr copy = detail::working_copy(input);
        return dressup(*copy, owner, scale, true);
      } catch (const Cancelled&) {
        throw;
      } catch (...) {
      }
    }
    if (!merges || (tight && !tighten)) {
      continue;
    }
    try {
      OCC_CATCH_SIGNALS
      const ShapePtr copy = detail::working_copy(input);
      const std::optional<MergedEdges> merged = merge_edge_pieces(*copy, sets, owner);
      merges = merged.has_value();
      if (merged) {
        return with_piece_names(dressup(*merged->body, merged->owner, scale, tight), *merged, feature, role);
      }
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
    }
  }
  // Faces of one surface that meet along an edge at the selected edges'
  // vertices joined, with the pieces of edges merged: OCCT's corner cuts a
  // rounding's end by one face at the end, not by two pieces of one surface
  // (a wall's end full-rounded in two halves, where the rounding of the
  // window's floor beside it ends across both, mitcad#343).
  try {
    OCC_CATCH_SIGNALS
    const ShapePtr copy = detail::working_copy(input);
    const std::optional<MergedEdges> merged = merge_edge_pieces(*copy, sets, owner, true);
    if (merged) {
      return with_piece_names(dressup(*merged->body, merged->owner, scale, false), *merged, feature, role);
    }
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
  }
  // Strips narrower than the import's tolerance at the selected edges'
  // vertices collapsed into the edges they are in the design (mitcad#299).
  try {
    OCC_CATCH_SIGNALS
    const ShapePtr copy = detail::working_copy(input);
    const std::optional<MergedEdges> collapsed = collapse_strips(*copy, owner);
    if (collapsed) {
      return dressup(*collapsed->body, collapsed->owner, scale, false);
    }
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
  }
  std::rethrow_exception(first);
}

// A fillet whose roundings run over cut-outs or protrusions of their faces
// (mitcad#311, cutouts.hpp): `build` rounds the body with them cleared
// back to the faces' surfaces, and they are put back. Null where none is
// found or the result is not one valid solid.
template <class Build>
ShapePtr over_cutouts(const Shape& input, const std::vector<FilletSet>& sets, const std::vector<int>& owner,
                      Build&& build) {
  // How far from its edges a rounding touches its faces (r tan(a / 2), a
  // the angle between the faces' normals) and changes the body (the
  // contact and the ball's diameter past it).
  double contact = 0.0;
  double reach = 0.0;
  const detail::EdgeFaces edge_faces(input);
  for (int e = 0; e < input.edge_count(); ++e) {
    const int s = owner[static_cast<std::size_t>(e)];
    if (s < 0) {
      continue;
    }
    const std::vector<int> faces = edge_faces.at(e);
    if (faces.size() != 2) {
      return nullptr;
    }
    double largest_angle = 0.0;
    for (const double t : {0.0, 0.5, 1.0}) {
      largest_angle = std::max(
          largest_angle, detail::normal_angle(input.edge(e), input.face(faces[0]), input.face(faces[1]), t));
    }
    if (!(largest_angle < 2.0 * kHalfPi - 1.0e-3)) {
      return nullptr;
    }
    const double radius = largest_radius(input, sets[static_cast<std::size_t>(s)], e);
    const double touch = radius * std::tan(largest_angle / 2.0);
    contact = std::max(contact, touch);
    reach = std::max(reach, touch + 2.0 * radius);
  }
  const ShapePtr copy = detail::working_copy(input);
  const std::optional<detail::Cutouts> cutouts = detail::fill_cutouts(*copy, owner, contact, reach);
  if (!cutouts) {
    return nullptr;
  }
  const ShapePtr rounded = build(*cutouts->body, cutouts->owner);
  ShapePtr result = detail::cut_out_again(rounded, *cutouts, *copy);
  if (result) {
    result->add_note("the rounding runs over cut-outs or protrusions of its faces: built with the faces' "
                     "surfaces cleared of them and with them put back");
  }
  return result;
}

// The body with the pieces of one surface joined (detail::join_surface_pieces)
// where the selected edges (`owner`, as select_edges gives it) run along
// them: a thread's flanks, root and crest, a face a turn on one screw
// surface each, as one face each over all the turns, as the bodies stored
// in files keep threads, so that a rounding along the thread walks one face
// (mitcad#343). Only where a B-spline surface is in pieces there; the faces
// away from the selected edges keep their pieces. The joined faces hold all
// their pieces' names, so the result's faces are named after them as
// before; `pieces` gets the faces joined (indices in `body`). The body
// itself where nothing is to be joined or it cannot be.
ShapePtr surface_pieces_joined(const Shape& body, const std::vector<int>& owner, std::vector<int>& pieces) {
  pieces.clear();
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edge_faces;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, edge_faces);
  const auto on_one_surface = [](const TopoDS_Shape& a, const TopoDS_Shape& b) {
    TopLoc_Location la, lb;
    const occ::handle<Geom_Surface> sa = BRep_Tool::Surface(TopoDS::Face(a), la);
    const occ::handle<Geom_Surface> sb = BRep_Tool::Surface(TopoDS::Face(b), lb);
    return la.IsEqual(lb) && a.Orientation() == b.Orientation() && detail::same_surface(sa, sb);
  };
  std::vector<char> taken(static_cast<std::size_t>(body.face_count()), 0);
  std::vector<int> faces;
  bool split_spline = false;
  for (int e = 0; e < body.edge_count(); ++e) {
    if (owner[static_cast<std::size_t>(e)] < 0 || !edge_faces.Contains(body.edge(e))) {
      continue;
    }
    for (const TopoDS_Shape& seed : edge_faces.FindFromKey(body.edge(e))) {
      const int s = body.face_index(seed);
      if (s < 0 || taken[static_cast<std::size_t>(s)] != 0 ||
          BRepAdaptor_Surface(TopoDS::Face(seed), false).GetType() == GeomAbs_Plane) {
        continue;
      }
      // The face's pieces: its neighbours on its surface, theirs, ...
      std::vector<int> group{s};
      taken[static_cast<std::size_t>(s)] = 1;
      for (std::size_t g = 0; g < group.size(); ++g) {
        for (TopExp_Explorer x(body.face(group[g]), TopAbs_EDGE); x.More(); x.Next()) {
          for (const TopoDS_Shape& neighbour : edge_faces.FindFromKey(x.Current())) {
            const int n = body.face_index(neighbour);
            if (n >= 0 && taken[static_cast<std::size_t>(n)] == 0 &&
                on_one_surface(neighbour, body.face(group[g]))) {
              taken[static_cast<std::size_t>(n)] = 1;
              group.push_back(n);
            }
          }
        }
      }
      // (Not where a piece is closed round its surface, a band about a
      // cylinder with its seam: joined with the pieces of a thread's crest
      // running on from it, it gave a face over more than the pieces.)
      const bool seamed = std::any_of(group.begin(), group.end(), [&](int g) {
        for (TopExp_Explorer x(body.face(g), TopAbs_EDGE); x.More(); x.Next()) {
          if (BRep_Tool::IsClosed(TopoDS::Edge(x.Current()), body.face(g))) {
            return true;
          }
        }
        return false;
      });
      if (group.size() > 1 && !seamed) {
        split_spline = split_spline ||
                       BRepAdaptor_Surface(body.face(s), false).GetType() == GeomAbs_BSplineSurface;
        faces.insert(faces.end(), group.begin(), group.end());
      }
    }
  }
  if (!split_spline) {
    return std::make_shared<Shape>(body);
  }
  ShapePtr joined;
  try {
    joined = detail::join_surface_pieces(body, faces);
  } catch (const Standard_Failure&) {
    joined = nullptr;
  }
  if (!joined) {
    return std::make_shared<Shape>(body);
  }
  pieces = faces;
  return joined;
}

// The dressup's result with the faces OCCT walked as B-splines where they
// are exactly analytic given their analytic surfaces (canonical.hpp,
// mitcad#328): a later rounding along them builds, as on the bodies the
// files store.
ShapePtr canonical(const Shape& input, ShapePtr result) {
  if (result) {
    if (ShapePtr analytic = detail::canonical_dressup(input, *result)) {
      return analytic;
    }
  }
  return result;
}

} // namespace

ShapePtr fillet(const std::string& feature, const Shape& given, const std::vector<FilletSet>& sets,
                bool rolling_ball_corners) {
  for (const FilletSet& set : sets) {
    check_fillet_set(set);
  }
  detail::require_surfaces(given.occt(), "fillet");
  // (A thread's faces, a turn each, joined where the edges run along them.)
  std::vector<int> pieces;
  const ShapePtr joined = surface_pieces_joined(given, select_edges(given, sets), pieces);
  const Shape& input = *joined;
  const std::vector<int> owner = select_edges(input, sets);
  std::vector<char> blended(sets.size(), 0);
  bool any_blended = false;
  for (std::size_t s = 0; s < sets.size(); ++s) {
    blended[s] = sets[s].size == FilletSize::Asymmetric || sets[s].curvature ? 1 : 0;
    any_blended = any_blended || blended[s] != 0;
  }
  ShapePtr result = canonical(input, with_own_chains("fillet", input, sets, owner,
                                                    [&](const std::vector<int>& selected) {
    const auto build = [&](const Shape& target, const std::vector<int>& own_edges) {
      const auto rings = [&](const Shape& body) { return ring_fillet(feature, body, sets, own_edges); };
      return or_rings("fillet", target, [&] {
        return with_shrink_retry("fillet", target, largest_size(sets), [&](double scale) {
          const auto rounded = [&] {
            return dress(target, sets, own_edges, scale, feature, "fillet", true,
                         [&](const Shape& body, const std::vector<int>& own, double size_scale, bool tight) {
                           if (any_blended) {
                             return round_and_blend(feature, body, sets, own, blended, rolling_ball_corners,
                                                    size_scale, tight);
                           }
                           return round_edges(feature, body, sets, own, rolling_ball_corners, size_scale, tight);
                         });
          };
          // At the exact size, before the 0.1 % smaller retry; not for
          // setback corners.
          return scale == 1.0 && !any_blended && rolling_ball_corners
                     ? or_mitred(feature, target, sets, own_edges, rounded)
                     : rounded();
        });
      }, rings);
    };
    try {
      return build(input, selected);
    } catch (const Cancelled&) {
      throw;
    } catch (const std::runtime_error&) {
      // Where the geometry kernel cannot build it, the roundings may run
      // over cut-outs or protrusions of their faces: built on the body with
      // them cleared back to the faces' surfaces, and put back
      // (mitcad#311). Else the kernel's error stands.
      ShapePtr over;
      try {
        over = over_cutouts(input, sets, selected, build);
      } catch (const Cancelled&) {
        throw;
      } catch (...) {
        over = nullptr;
      }
      if (over) {
        return over;
      }
      throw;
    }
  }));
  // (The joined pieces split again: the body keeps its thread a turn a face,
  // as later operations near it take far longer on faces of many turns.)
  if (!result || pieces.empty()) {
    return result;
  }
  double reach = 0.0;
  for (const FilletSet& set : sets) {
    reach = std::max({reach, set.radius, set.radius2, set.chord});
    for (const auto& point : set.mid) {
      reach = std::max(reach, point.second);
    }
  }
  ShapePtr split;
  try {
    split = detail::split_joined_pieces(*result, given, pieces, feature, "fillet", 3.0 * reach);
  } catch (const Standard_Failure&) {
    split = nullptr;
  }
  return split ? split : result;
}

ShapePtr fillet(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                double radius) {
  FilletSet set;
  set.edges = edges;
  set.radius = radius;
  return fillet(feature, body, {set});
}

ShapePtr chamfer(const std::string& feature, const Shape& input, const std::vector<ChamferSet>& sets,
                 ChamferCorner corner) {
  detail::require_surfaces(input.occt(), "chamfer");
  for (const ChamferSet& set : sets) {
    const ChamferSpec& spec = set.spec;
    detail::require_positive("chamfer distance", spec.distance);
    if (spec.type == ChamferType::TwoDistances) {
      detail::require_positive("second chamfer distance", spec.distance2);
    }
    if (spec.type == ChamferType::DistanceAngle && !(spec.angle > 0.0 && spec.angle < kHalfPi)) {
      throw std::invalid_argument("the chamfer angle must be between 0 and 90 degrees");
    }
  }
  const std::vector<int> owner = select_edges(input, sets);
  return canonical(input, with_own_chains("chamfer", input, sets, owner, [&](const std::vector<int>& selected) {
    const auto rings = [&](const Shape& body) { return ring_chamfer(feature, body, sets, selected); };
    return or_rings("chamfer", input, [&] {
      return with_shrink_retry("chamfer", input, largest_size(sets), [&](double scale) {
        return dress(input, sets, selected, scale, feature, "chamfer", false,
                     [&](const Shape& body, const std::vector<int>& own, double size_scale, bool) {
                       return bevel(feature, body, sets, own, corner, size_scale);
                     });
      });
    }, rings);
  }));
}

ShapePtr chamfer(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                 const ChamferSpec& spec) {
  ChamferSet set;
  set.edges = edges;
  set.spec = spec;
  return chamfer(feature, body, {set});
}

} // namespace mitcad::geometry
