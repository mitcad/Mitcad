// SPDX-License-Identifier: MIT
#include "mitcad/geometry/dressup.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <exception>
#include <functional>
#include <initializer_list>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BOPAlgo_GlueEnum.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepBndLib.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
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
#include <NCollection_DataMap.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
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
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Mat.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "blend.hpp"
#include "canonical.hpp"
#include "cored_dressup.hpp"
#include "corner.hpp"
#include "cutouts.hpp"
#include "dressup_check.hpp"
#include "dressup_strips.hpp"
#include "end_face_dressup.hpp"
#include "emboss_projection.hpp"
#include "face_join.hpp"
#include "face_select.hpp"
#include "full_round.hpp"
#include "g2_corner.hpp"
#include "groove_end.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "mitred_dressup.hpp"
#include "node_dressup.hpp"
#include "remnant_patch.hpp"
#include "ring_dressup.hpp"
#include "runout_dressup.hpp"
#include "util.hpp"
#include "vanishing_split.hpp"

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
// sharply (mitcad#107; along a knife edge, faces meeting at 0 degrees, the
// port's patch 1770 starts the ball where it fits, mitcad#1984).
constexpr double kTight[6] = {1.0e-3, 1.0e-7, 1.0e-9, 1.0e-6, 1.0e-8, 1.0e-5};
// The angle between the faces' normals across a knife edge: more than
// about 172 degrees (they fold back on each other).
constexpr double kKnife = 3.0;

// Whether a shape is closed solids only: at least one solid, every face in
// one, every shell of them closed (each edge between two faces).
bool closed_solids(const TopoDS_Shape& shape) {
  int solids = 0;
  int faces_in_solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
    for (TopExp_Explorer face(solid.Current(), TopAbs_FACE); face.More(); face.Next()) {
      ++faces_in_solids;
    }
  }
  int faces = 0;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    ++faces;
  }
  return solids > 0 && faces == faces_in_solids;
}

// A dressup of a closed solid must leave closed solids (mitcad#1182: a
// chamfer's result that healing made a valid open shell turned the body
// into a surface body without a word): throws when `result` is not.
void require_closed(const TopoDS_Shape& input, const TopoDS_Shape& result, const char* role) {
  if (closed_solids(input) && !closed_solids(result)) {
    throw std::runtime_error(std::string("the ") + role +
                             " would leave the body open: the geometry kernel's result is not a closed solid");
  }
}

// A dressup of a closed solid whose result the geometry kernel left open
// (mitcad#1819: a band that takes a narrow face away ends on that face's
// continuation where the face beyond does not reach, and the kernel leaves
// the gap): a failure like the kernel's own, so that the ways round it are
// tried (the narrow faces continued, over_narrow_faces); where none builds
// and a face beside the edges is narrower than the band, the error says so
// in words instead of this text.
class OpenResult : public std::runtime_error {
public:
  explicit OpenResult(const char* role)
      : std::runtime_error(std::string("the ") + role +
                           " would leave the body open: the geometry kernel's result is not a closed solid") {}
};

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

} // namespace

// The strips of a dressup built 0.1 % smaller, removed (dressup_strips.hpp,
// mitcad#1106): the faces of the smaller one narrower than twice (1 -
// kShrink) of `contact` (a strip's width; mitcad#1150: a fillet across a
// sharp angle touches its faces far beyond its radius) and not in the
// input, beside a face the dressup made, removed by OCCT's defeaturing,
// which extends the faces round each strip to meet. The dressup's faces
// then meet the faces beyond the strips at one edge, as at the exact size,
// and take the strips' names, as a face the dressup takes away gives its
// names to the dressup's face there (name_consumed_faces). A connected
// group of strips with more than kMostFacesBesideStrips faces beside it
// stays (mitcad#1337: the defeaturing intersects all of them, grown by the
// group's extent).
detail::Strips detail::strips_to_remove(const Shape& smaller, const Shape& input, double contact) {
  const double narrow = 2.0 * (1.0 - kShrink) * contact;
  const int face_count = smaller.face_count();
  Strips out;
  // The faces beside each face, across its edges (one pass over the edges:
  // the body may have thousands of faces).
  std::vector<std::vector<int>>& beside = out.beside;
  beside.resize(static_cast<std::size_t>(face_count));
  {
    const detail::EdgeFaces edge_faces(smaller);
    for (int e = 0; e < smaller.edge_count(); ++e) {
      const std::vector<int> faces = edge_faces.at(e);
      if (faces.size() == 2 && faces[0] != faces[1]) {
        beside[static_cast<std::size_t>(faces[0])].push_back(faces[1]);
        beside[static_cast<std::size_t>(faces[1])].push_back(faces[0]);
      }
    }
    for (std::vector<int>& faces : beside) {
      std::sort(faces.begin(), faces.end());
      faces.erase(std::unique(faces.begin(), faces.end()), faces.end());
    }
  }
  // The dressup's faces: those whose names the input has not.
  std::vector<char>& made = out.made;
  made.assign(static_cast<std::size_t>(face_count), 0);
  for (int f = 0; f < face_count; ++f) {
    const NameList& names = smaller.face_names(f);
    made[static_cast<std::size_t>(f)] = !names.empty() && input.find_faces(names.front()).empty() ? 1 : 0;
  }
  // The strips: faces of the input's beside a face the dressup made, narrow
  // where they were wide (only those beside a made face are measured).
  std::vector<char> strip(static_cast<std::size_t>(face_count), 0);
  std::vector<int> strip_faces;
  for (int f = 0; f < face_count; ++f) {
    const NameList& names = smaller.face_names(f);
    const std::vector<int>& around = beside[static_cast<std::size_t>(f)];
    if (names.empty() || made[static_cast<std::size_t>(f)] != 0 ||
        std::none_of(around.begin(), around.end(), [&made](int g) { return made[static_cast<std::size_t>(g)] != 0; }) ||
        !(face_width(smaller.face(f)) < narrow)) {
      continue;
    }
    bool was_wide = false;
    for (const int g : input.find_faces(names.front())) {
      was_wide = was_wide || face_width(input.face(g)) >= 2.0 * narrow;
    }
    if (was_wide) {
      strip[static_cast<std::size_t>(f)] = 1;
      strip_faces.push_back(f);
    }
  }
  // The connected groups of strips (OCCT's defeaturing removes each as one
  // feature) and the faces beside each: a group with too many stays.
  std::vector<char> grouped(static_cast<std::size_t>(face_count), 0);
  for (const int first : strip_faces) {
    if (grouped[static_cast<std::size_t>(first)] != 0) {
      continue;
    }
    std::vector<int> group = {first};
    grouped[static_cast<std::size_t>(first)] = 1;
    std::set<int> around;
    for (std::size_t k = 0; k < group.size(); ++k) {
      for (const int g : beside[static_cast<std::size_t>(group[k])]) {
        if (strip[static_cast<std::size_t>(g)] == 0) {
          around.insert(g);
        } else if (grouped[static_cast<std::size_t>(g)] == 0) {
          grouped[static_cast<std::size_t>(g)] = 1;
          group.push_back(g);
        }
      }
    }
    if (static_cast<int>(around.size()) > kMostFacesBesideStrips) {
      out.left.insert(out.left.end(), group.begin(), group.end());
      continue;
    }
    out.faces.insert(out.faces.end(), group.begin(), group.end());
  }
  return out;
}

ShapePtr detail::without_strips(const Shape& smaller, const Shape& input, double contact) {
  const Strips found = strips_to_remove(smaller, input, contact);
  if (found.faces.empty()) {
    return nullptr;
  }
  NCollection_List<TopoDS_Shape> strips;
  for (const int f : found.faces) {
    strips.Append(smaller.face(f));
  }
  try {
    OCC_CATCH_SIGNALS
    BRepAlgoAPI_Defeaturing defeaturing;
    defeaturing.SetShape(smaller.occt());
    defeaturing.AddFacesToRemove(strips);
    defeaturing.SetRunParallel(false);
    defeaturing.SetToFillHistory(true);
    // (Stopped on a cancel request, its progress shown to the source: an
    // import's watchdog sees it is not stuck.)
    detail::build(defeaturing);
    if (!defeaturing.IsDone() || defeaturing.HasWarnings()) {
      return nullptr;
    }
    const TopoDS_Shape whole = defeaturing.Shape();
    int solids = 0;
    for (TopExp_Explorer ex(whole, TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    const double before = detail::volume_of(smaller.occt());
    // (A valid open shell is no result for a closed body, mitcad#2276.)
    if (solids != 1 || !BRepCheck_Analyzer(whole, true, true).IsValid() ||
        (closed_solids(smaller.occt()) && !closed_solids(whole)) ||
        std::abs(detail::volume_of(whole) - before) > 1.0e-4 * std::abs(before)) {
      return nullptr;
    }
    const occ::handle<BRepTools_History> history = defeaturing.History();
    detail::FaceNamer namer(whole);
    namer.carry(*history, smaller);
    // Each strip's names to the images of the dressup's faces beside it.
    for (const int f : found.faces) {
      for (const int other : found.beside[static_cast<std::size_t>(f)]) {
        if (found.made[static_cast<std::size_t>(other)] == 0) {
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

namespace {

// Runs `attempt` with the sizes as given (scale 1), then, when it fails
// (not for invalid arguments), with the sizes scaled by kShrink, without
// the strips that leaves (without_strips; `input` the body it dresses,
// `contact()` how far from its edges it touches its faces at most, asked
// only then) and with an info saying it is that much smaller (mitcad#1106:
// 0.1 % is no reason for a warning). Where a strip stays (the faces round
// it cannot be grown to meet, or too many lie beside it), a warning instead
// that says so, the strips painted (mitcad#1783). Throws the first error
// when both fail. `what`: "fillet" or "chamfer".
template <class Contact, class Attempt>
ShapePtr with_shrink_retry(const char* what, const Shape& input, Contact&& contact, Attempt&& attempt) {
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
    const double reach = contact();
    const detail::Strips strips = detail::strips_to_remove(*smaller, input, reach);
    std::vector<int> staying = strips.left;
    ShapePtr whole = strips.faces.empty() ? nullptr : detail::without_strips(*smaller, input, reach);
    if (!whole) {
      staying.insert(staying.end(), strips.faces.begin(), strips.faces.end());
    }
    std::vector<Shape::NotePlace> places;
    for (const int f : staying) {
      const NameList& names = smaller->face_names(f);
      if (!names.empty()) {
        places.push_back(Shape::NotePlace::of(Shape::NotePlace::Kind::Face, names.front()));
      }
    }
    if (whole) {
      smaller = whole;
    }
    if (staying.empty()) {
      smaller->add_info(std::string("the ") + what +
                        " is built 0.1 % smaller than asked: at its exact size it takes a face away, "
                        "which the geometry kernel cannot build");
    } else {
      smaller->add_note(std::string("the ") + what +
                            " is built 0.1 % smaller than asked and leaves a sliver of a face it should take "
                            "away: at its exact size the geometry kernel cannot build it",
                        std::move(places));
    }
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

// An error about edges of the body (mitcad#1612): `said` in plain words for
// the person, `named` with their names (describe_edge) for agents, the
// edges painted.
std::string edges_error(const Shape& body, std::initializer_list<int> edges, const std::string& said,
                        const std::string& named) {
  std::vector<ErrorPlace> places;
  for (const int edge : edges) {
    if (edge >= 0 && edge < body.edge_count()) {
      places.push_back(edge_place(body.edge_name(edge)));
    }
  }
  return placed_error(said, named, places);
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
        throw std::invalid_argument(edges_error(body, {edge}, "an edge is in two edge sets",
                                                describe_edge(body, edge) + " is in two edge sets"));
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
        throw std::invalid_argument(edges_error(
            body, {edge, seed}, "unsupported: an edge continues the tangent chain of an edge from another edge set",
            "unsupported: " + describe_edge(body, edge) + " continues the tangent chain of " +
                describe_edge(body, seed) + " from another edge set"));
      }
    }
    if (set >= 0 && unselected >= 0 && !sets[static_cast<std::size_t>(set)].tangent_chain) {
      const std::string why = "; without a tangent chain only the selected edges would be rounded, but the "
                              "geometry kernel's chain goes on into it";
      throw std::invalid_argument(edges_error(
          body, {seed, unselected}, "unsupported: a selected edge continues tangentially into an edge not selected" + why,
          "unsupported: " + describe_edge(body, seed) + " continues tangentially into " +
              describe_edge(body, unselected) + why));
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
// with the geometry kernel's chains alone (OCCT cannot round some chains it
// does not follow itself: two contours that meet end to end at a vertex
// where other sharp edges meet). Remarked only where the result lacks
// some of the edges that continue the chains, still sharp in it
// (mitcad#1327: how it was built is no warning), with those edges as the
// remark's places: no warning either (mitcad#1914), the panel points it
// out while it is open.
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
  // The edges the chains were to take on that the result still has.
  std::vector<Shape::NotePlace> sharp;
  for (int e = 0; e < input.edge_count() && e < static_cast<int>(grown.size()); ++e) {
    if (grown[static_cast<std::size_t>(e)] < 0 || owner[static_cast<std::size_t>(e)] >= 0) {
      continue;
    }
    const std::string& name = input.edge_name(e);
    if (!name.empty() && !result->find_edges(name).empty()) {
      sharp.push_back(Shape::NotePlace::of(Shape::NotePlace::Kind::Edge, name));
    }
  }
  if (!sharp.empty()) {
    const std::size_t count = sharp.size();
    result->add_remark(std::string("The ") + what + "'s tangent chain stops where other sharp edges meet: " +
                           (count == 1 ? std::string("an edge that continues it is")
                                       : std::to_string(count) + " edges that continue it are") +
                           " left sharp.",
                       std::move(sharp));
  }
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
    // A valid open shell is no repair of a closed solid (mitcad#1182).
    if (closed_solids(body.occt()) && !closed_solids(fixed)) {
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

// The error `what` of a dressup's result that OCCT's checker rejects, with
// where it is faulty (mitcad#1253): "; the fault lies in a face the fillet
// made", the faces the dressup made (<feature>:…) that the checker rejects
// painted, and for agents the first of them by name ("the corner of the
// fillet at V{…}", mitcad#1612), with how many more; `what` as it is when
// none is. Only the made faces are checked again: on a large body the
// whole check takes about as long as the dressup.
std::string with_faulty_where(const std::string& what, const Shape& result, const std::string& feature,
                              const char* role) {
  try {
    OCC_CATCH_SIGNALS
    const std::string prefix = feature + ":";
    BRep_Builder builder;
    TopoDS_Compound made_faces;
    builder.MakeCompound(made_faces);
    for (int f = 0; f < result.face_count(); ++f) {
      const NameList& names = result.face_names(f);
      if (!names.empty() && names.front().compare(0, prefix.size(), prefix) == 0) {
        builder.Add(made_faces, result.face(f));
      }
    }
    const ShapeMap faulty = faulty_parts(made_faces);
    std::vector<std::string> where;
    std::vector<ErrorPlace> places;
    for (int i = 1; i <= faulty.Extent(); ++i) {
      const int f = faulty(i).ShapeType() == TopAbs_FACE ? result.face_index(faulty(i)) : -1;
      if (f >= 0) {
        where.push_back(detail::made_face(result.face_names(f).front(), feature, role, false));
        places.push_back(face_place(result.face_names(f).front()));
      }
    }
    if (where.empty()) {
      return what;
    }
    const std::size_t more = where.size() - 1;
    const std::string others =
        more == 0 ? std::string()
                  : " (and in " + std::to_string(more) + " more " + (more == 1 ? "face" : "faces") + ")";
    return placed_error(what + "; the fault lies in a face the " + role + " made" + others,
                        what + "; the fault lies in " + where.front() + others, places);
  } catch (const Standard_Failure&) {
    return what;
  }
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

// Set while a fillet rounds a body whose edges were split where their
// roundings vanish (mitcad#2300): its own fillets split nothing again.
thread_local bool t_vanishing_split = false;

class VanishingSplitScope {
public:
  VanishingSplitScope() : m_previous(t_vanishing_split) { t_vanishing_split = true; }
  ~VanishingSplitScope() { t_vanishing_split = m_previous; }
  VanishingSplitScope(const VanishingSplitScope&) = delete;
  VanishingSplitScope& operator=(const VanishingSplitScope&) = delete;

private:
  bool m_previous;
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
  // The named contour edges in the contours' order, with their boxes.
  struct Named {
    TopoDS_Edge edge;
    std::string name;
    Bnd_Box box;
  };
  std::vector<Named> named;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      if (const auto name = body.name_of_edge(maker.Edge(ic, ie))) {
        Named n{maker.Edge(ic, ie), *name, Bnd_Box()};
        BRepBndLib::Add(n.edge, n.box);
        named.push_back(std::move(n));
      }
    }
  }
  for (int f = 1; f <= faces.Extent(); ++f) {
    if (namer.named(faces(f))) {
      continue;
    }
    // The edges by the distance of their boxes from the face's: an edge
    // whose box lies farther than the nearest edge found so far is not
    // nearer (an exact distance between a free-form face and every edge
    // took a minute on a fillet of 60 edges with a few such faces,
    // mitcad#1987). The first in the contours' order wins a tie, as when
    // every edge was measured.
    Bnd_Box face_box;
    BRepBndLib::Add(faces(f), face_box);
    std::vector<std::pair<double, std::size_t>> order;
    for (std::size_t k = 0; k < named.size(); ++k) {
      order.emplace_back(face_box.IsVoid() || named[k].box.IsVoid() ? 0.0 : face_box.Distance(named[k].box), k);
    }
    std::sort(order.begin(), order.end());
    std::optional<std::size_t> nearest;
    double distance = 0.0;
    for (const auto& [lower, k] : order) {
      if (nearest && lower > distance) {
        break;
      }
      try {
        OCC_CATCH_SIGNALS
        BRepExtrema_DistShapeShape between(faces(f), named[k].edge);
        if (between.IsDone() &&
            (!nearest || between.Value() < distance || (between.Value() == distance && k < *nearest))) {
          nearest = k;
          distance = between.Value();
        }
      } catch (const Standard_Failure&) {
      }
    }
    if (nearest && held.count(face_name(feature, role, named[*nearest].name)) == 0) {
      namer.add(faces(f), face_name(feature, role, named[*nearest].name));
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
  // The faces generated from an edge of the body outside the contours: port
  // patch 1380 splits a concave contour at an arc of its chain tighter than
  // the ball, and the corner it closes there with the arc's own ball, rolled
  // on between the roundings, is generated from the arc's edge. Unnamed, the
  // corner's edges were unnamed too, and a later sketch's curves along them
  // found no edge to follow (mitcad#1397).
  for (int i = 0; i < body.edge_count(); ++i) {
    const TopoDS_Edge& edge = body.edge(i);
    if (body.edge_name(i).empty() || maker.Contour(edge) != 0 || maker.Generated(edge).IsEmpty()) {
      continue;
    }
    namer.generated(maker, edge, face_name(feature, role, body.edge_name(i)));
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
    require_closed(body.occt(), tightened->occt(), role);
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
    // (An open result says so.)
    require_closed(body.occt(), namer.result(), role);
    // Throws with the problem, and where it lies (mitcad#1253).
    try {
      detail::require_valid(namer.result(), role);
    } catch (const Cancelled&) {
      throw;
    } catch (const std::runtime_error& error) {
      throw std::runtime_error(with_faulty_where(error.what(), *namer.shape(), feature, role));
    }
  }
  require_closed(body.occt(), namer.result(), role);
  return namer.shape();
}

// After a contour stopped where its edges turn (port patch 0180), the face
// beyond the crease grows along its edge up to where it cuts the rounding,
// and OCCT keeps the vertex where the contour stopped between the two
// pieces of that edge. The roundings in the files have one edge there: the
// pieces are joined (ShapeUpgrade_UnifySameDomain on edges, every other
// vertex kept; it joins lines and circles, and with `splines` also two
// splines that continue each other, concatenated into one), the faces
// keeping their names.
ShapePtr join_at_turn_stops(const ShapePtr& rounded,
                            const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>& stops,
                            bool splines = false) {
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
  ShapeUpgrade_UnifySameDomain unify(rounded->occt(), true, false, splines);
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
// two edges meet, for join_at_turn_stops to join. Contacts on a face beyond
// a narrow one are splines (a fin's side strip under its top's chamfer,
// whose transition piece meets the strip in two edges where the files have
// one; the ball of a later rounding rolling on the strip's edge took the
// short one for the edge it rolls on, and the corner at the strip's end
// left the piece out, mitcad#1388): they are concatenated.
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
    throw std::invalid_argument(edges_error(body, {edge}, "an edge does not lie between two faces",
                                            describe_edge(body, edge) + " does not lie between two faces"));
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
      throw std::invalid_argument(edges_error(
          body, {edge}, "unsupported: a chord length fillet on an edge where the angle between the faces varies",
          "unsupported: a chord length fillet on " + describe_edge(body, edge) +
              ", where the angle between the faces varies"));
    }
  }
  if (angle < 1.0e-3) {
    throw std::invalid_argument(edges_error(body, {edge}, "an edge joins its faces tangentially",
                                            describe_edge(body, edge) + " joins its faces tangentially"));
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
  case FilletSize::FullRound:
    // (Its radius is worked out on the body before, full_round.hpp.)
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
      throw std::invalid_argument(placed_error("the body has no such start vertex",
                                              "the body has no vertex " + set.start_vertex));
    }
    if (closed) {
      throw std::invalid_argument("unsupported: a variable radius around a closed chain");
    }
    const TopoDS_Shape& vertex = body.vertex(start.front());
    reversed = maker.LastVertex(ic).IsSame(vertex);
    if (!reversed && !maker.FirstVertex(ic).IsSame(vertex)) {
      throw std::invalid_argument(placed_error(
          "the start vertex is not an end of the edges' chain",
          "vertex " + set.start_vertex + " is not an end of the edges' chain", {vertex_place(set.start_vertex)}));
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
      throw std::invalid_argument(placed_error("the body has no such start vertex",
                                              "the body has no vertex " + set.start_vertex));
    }
    const TopoDS_Shape& vertex = body.vertex(start.front());
    reversed = maker.LastVertex(ic).IsSame(vertex);
    if (!reversed && !maker.FirstVertex(ic).IsSame(vertex)) {
      throw std::invalid_argument(placed_error(
          "the start vertex is not an end of the edges' chain",
          "vertex " + set.start_vertex + " is not an end of the edges' chain", {vertex_place(set.start_vertex)}));
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
  case FilletSize::FullRound: // (its radius worked out already)
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

// The largest ratio of a set's two chamfer distances (1 for equal ones).
double steepness(const std::vector<ChamferSet>& sets) {
  double ratio = 1.0;
  for (const ChamferSet& set : sets) {
    const ChamferSpec& spec = set.spec;
    double a = spec.distance;
    double b = spec.distance;
    if (spec.type == ChamferType::TwoDistances) {
      b = spec.distance2;
    } else if (spec.type == ChamferType::DistanceAngle) {
      b = spec.distance * std::tan(spec.angle);
    }
    if (a > 0.0 && b > 0.0) {
      ratio = std::max(ratio, std::max(a, b) / std::min(a, b));
    }
  }
  return ratio;
}

// The distance from a point to the curve an edge lies on: a line's or a
// circle's whole curve (a stripe's end past the edge's vertex stays within
// the contact of it), else the edge itself.
double distance_to_curve_of(const TopoDS_Edge& edge, const gp_Pnt& point) {
  const BRepAdaptor_Curve curve(edge);
  if (curve.GetType() == GeomAbs_Line) {
    return curve.Line().Distance(point);
  }
  if (curve.GetType() == GeomAbs_Circle) {
    const gp_Circ circle = curve.Circle();
    const gp_Vec from(circle.Location(), point);
    const double axial = from.Dot(gp_Vec(circle.Axis().Direction()));
    const double radial = std::sqrt(std::max(0.0, from.SquareMagnitude() - axial * axial));
    return std::hypot(axial, radial - circle.Radius());
  }
  BRepExtrema_DistShapeShape between(BRepBuilderAPI_MakeVertex(point).Vertex(), edge);
  return between.IsDone() ? between.Value() : 0.0;
}

// A fillet or chamfer (`role`) that the geometry kernel built (`maker`
// done) but that took away whole a face beyond its reach: a face of the
// body that touches none of the contours' edges, not even at a vertex, and
// reaches farther from all of them than the dressup touches its faces
// (`contact` of a contour's edge: its size times what the angle between
// its faces makes of it). A rounding or bevel takes away only faces within
// its section, a narrow face beside its edge and what lies between it and
// the contact lines; one that takes away a face beyond rode on faces past
// the ones it was to dress, the material between gone with it (mitcad#1953:
// a strip 2 wide closed off by a face at 45 degrees, rounded along its
// outer wall with a radius between 2 and 3, came back valid with the wall
// under the strip gone and the floor's edge beyond rounded instead). The
// failure in words, the edge nearest the face and its faces as places,
// the face and the distances in the detail; else empty. Never for an edge
// whose faces fold back on each other (no finite contact).
template <class Contact>
std::string took_face_beyond_reach(BRepFilletAPI_LocalOperation& maker, const Shape& body, const char* role,
                                   Contact&& contact) {
  ShapeMap contour_edges;
  ShapeMap contour_vertices;
  double reach = 0.0;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const TopoDS_Edge& edge = maker.Edge(ic, ie);
      const double c = contact(ic, edge);
      if (!(c > 0.0) || !std::isfinite(c)) {
        return {};
      }
      reach = std::max(reach, c);
      contour_edges.Add(edge);
      TopExp::MapShapes(edge, TopAbs_VERTEX, contour_vertices);
    }
  }
  if (contour_edges.IsEmpty()) {
    return {};
  }
  const double tolerance = 1.0e-6 * std::max(1.0, reach);
  for (int f = 0; f < body.face_count(); ++f) {
    const TopoDS_Face& face = body.face(f);
    if (!maker.IsDeleted(face)) {
      continue;
    }
    bool touches = false;
    for (TopExp_Explorer v(face, TopAbs_VERTEX); v.More() && !touches; v.Next()) {
      touches = contour_vertices.Contains(v.Current());
    }
    if (touches) {
      continue;
    }
    double farthest = 0.0;
    int nearest_edge = 0;
    for (TopExp_Explorer x(face, TopAbs_EDGE); x.More(); x.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(x.Current()));
      const double t0 = curve.FirstParameter();
      const double t1 = curve.LastParameter();
      if (!std::isfinite(t0) || !std::isfinite(t1)) {
        continue;
      }
      for (const double t : {0.0, 0.25, 0.5, 0.75, 1.0}) {
        const gp_Pnt point = curve.Value(t0 + t * (t1 - t0));
        double nearest = -1.0;
        int at = 0;
        for (int i = 1; i <= contour_edges.Extent(); ++i) {
          const double d = distance_to_curve_of(TopoDS::Edge(contour_edges(i)), point);
          if (nearest < 0.0 || d < nearest) {
            nearest = d;
            at = i;
          }
        }
        if (nearest > farthest) {
          farthest = nearest;
          nearest_edge = at;
        }
      }
    }
    if (!(farthest > reach + tolerance)) {
      continue;
    }
    const int e = body.edge_index(TopoDS::Edge(contour_edges(nearest_edge)));
    const std::string name = e >= 0 ? body.edge_name(e) : std::string();
    std::vector<ErrorPlace> places;
    if (!name.empty()) {
      places.push_back(edge_place(name));
      for (const int g : body.edge_faces(e)) {
        if (g >= 0 && !body.face_names(g).empty()) {
          places.push_back(face_place(body.face_names(g).front()));
        }
      }
    }
    const NameList& names = body.face_names(f);
    if (!names.empty()) {
      places.push_back(face_place(names.front()));
    }
    const std::string what = std::strcmp(role, "fillet") == 0 ? "rounding" : "bevel";
    const std::string said = "the " + what + " is too large for the face beside its edge: the geometry kernel " +
                             "runs it onto faces beyond and takes away a face out of its reach";
    const auto number = [](double value) {
      char text[32];
      std::snprintf(text, sizeof text, "%.6g", value);
      return std::string(text);
    };
    const std::string face_named = names.empty() ? std::string("an unnamed face") : "face " + names.front();
    return placed_error(said,
                        said + " (" + face_named + ", reaching " + number(farthest) + " mm from " +
                            (name.empty() ? std::string("an unnamed edge") : "edge " + name) + ", while the " +
                            what + " touches its faces at most " + number(reach) + " mm from its edges)",
                        places);
  }
  return {};
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
      {"a knife edge running on along its line",
       "an edge where two faces meet tangentially (a knife edge) runs on along its line into an edge whose "
       "faces meet at an angle, and the rounding's ball jumps there from two radii into the wedge to the "
       "corner, which the geometry kernel cannot round across"},
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
// that). The person reads "along an edge", the contour's edges painted;
// agents read the edge's name (mitcad#1612).
std::string fillet_failure_along(const BRepFilletAPI_MakeFillet& maker, const std::string& along);

std::string fillet_failure(const BRepFilletAPI_MakeFillet& maker, const Shape& body) {
  const int ic = maker.FailedContour();
  std::string said;
  std::string named;
  std::vector<ErrorPlace> places;
  if (ic > 0 && maker.NbEdges(ic) > 0) {
    const int edge = body.edge_index(maker.Edge(ic, 1));
    said = " along an edge";
    named = " along " + (edge >= 0 ? describe_edge(body, edge) : std::string("an edge"));
    for (int k = 1; k <= maker.NbEdges(ic); ++k) {
      const int on = body.edge_index(maker.Edge(ic, k));
      if (on >= 0) {
        places.push_back(edge_place(body.edge_name(on)));
      }
    }
    if (maker.NbEdges(ic) > 1) {
      const std::string more = " (and " + std::to_string(maker.NbEdges(ic) - 1) + " more edges of its chain)";
      said += more;
      named += more;
    }
  }
  // A corner's failure at its vertices (mitcad#1988).
  if (maker.Failure() == ChFi3d_Builder::Failure_Corner) {
    for (int k = 1; k <= maker.NbFaultyVertices(); ++k) {
      const int v = body.vertex_index(maker.FaultyVertex(k));
      if (v >= 0 && !body.vertex_name(v).empty()) {
        places.push_back(vertex_place(body.vertex_name(v)));
      }
    }
  }
  if (places.empty()) {
    return fillet_failure_along(maker, said);
  }
  return placed_error(fillet_failure_along(maker, said), fillet_failure_along(maker, named), places);
}

// fillet_failure's text with `along` (" along an edge") where it names the
// contour.
std::string fillet_failure_along(const BRepFilletAPI_MakeFillet& maker, const std::string& along) {
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
  case ChFi3d_Builder::Failure_NoResult:
    return "the geometry kernel computes the roundings but cannot rebuild the body with them";
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
  // A concave contour that meets a deferred one at a vertex where they
  // join (a corner of the two) comes after the convex roundings too: built
  // before them, its blend's end would take a part of the deferred one's
  // edge away, and the deferred one would then end against that blend
  // (a block's foot rounded along its side and along the wall behind,
  // mitcad#1987).
  const auto contour_kind = [&](int ic) {
    int kind = -1;
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const int edge = body.edge_index(maker.Edge(ic, ie));
      if (edge < 0) {
        return -2;
      }
      const auto [a, b] = edge_face_pair(body, edge);
      const ChFiDS_TypeOfConcavity type =
          ChFi3d::DefineConnectType(body.edge(edge), body.face(a), body.face(b), 1.0e-4, false);
      const int k = type == ChFiDS_Convex ? 1 : type == ChFiDS_Concave ? 0 : -2;
      if (ie > 1 && k != kind) {
        return -2;
      }
      kind = k;
    }
    return kind;
  };
  const auto ends_of = [&](int ic) {
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> ends;
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      ends.Add(TopExp::FirstVertex(maker.Edge(ic, ie)));
      ends.Add(TopExp::LastVertex(maker.Edge(ic, ie)));
    }
    return ends;
  };
  if (!deferred.empty()) {
    std::set<int> joined;
    for (const int ic : deferred) {
      const auto ends = ends_of(ic);
      for (int other = 1; other <= maker.NbContours(); ++other) {
        if (deferred.count(other) > 0 || contour_kind(other) != 0) {
          continue;
        }
        bool meets = false;
        for (int ie = 1; ie <= maker.NbEdges(other) && !meets; ++ie) {
          meets = ends.Contains(TopExp::FirstVertex(maker.Edge(other, ie))) ||
                  ends.Contains(TopExp::LastVertex(maker.Edge(other, ie)));
        }
        if (meets) {
          joined.insert(other);
        }
      }
    }
    deferred.insert(joined.begin(), joined.end());
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

// Where OCCT's fillet failed because the roundings of two chains overlap on
// a face between them (Failure_Overlap) and concave_after_convex finds no
// convex and concave pair to part them: the selected edges of the last
// failed contour, which comes after the others, where it shares no vertex
// with any other contour (two chains apart, the overlap only where their
// roundings meet on a face: a fin's top rounded with 0.5 along its outer
// and its inner sides, its arms 2 wide running down to the plate, where
// both roundings take the arms' sides away, mitcad#1926). Empty where the
// failure is another one or the chains meet.
std::vector<int> overlap_apart(const BRepFilletAPI_MakeFillet& maker, const Shape& body,
                               const std::vector<int>& owner) {
  std::vector<int> later;
  if (maker.Failure() != ChFi3d_Builder::Failure_Overlap || maker.NbFaultyContours() < 2) {
    return later;
  }
  const int last = maker.FaultyContour(maker.NbFaultyContours());
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> ends;
  for (int ie = 1; ie <= maker.NbEdges(last); ++ie) {
    ends.Add(TopExp::FirstVertex(maker.Edge(last, ie)));
    ends.Add(TopExp::LastVertex(maker.Edge(last, ie)));
  }
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    if (ic == last) {
      continue;
    }
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      if (ends.Contains(TopExp::FirstVertex(maker.Edge(ic, ie))) ||
          ends.Contains(TopExp::LastVertex(maker.Edge(ic, ie)))) {
        return later;
      }
    }
  }
  for (int ie = 1; ie <= maker.NbEdges(last); ++ie) {
    const int edge = body.edge_index(maker.Edge(last, ie));
    if (edge >= 0 && owner[static_cast<std::size_t>(edge)] >= 0) {
      later.push_back(edge);
    }
  }
  return later;
}

// Where OCCT's fillet refused a corner because a knife edge (its faces
// meeting tangentially with opposite normals) runs on along its line into
// an edge whose faces meet at an angle (port patch 1790): the ball jumps at
// their common vertex, from two radii into the wedge to the corner, and no
// corner of the kernel joins the two roundings. The selected edges of the
// contours at that vertex that are not knife edges, rounded after the knife
// edge: its rounding ends on the face at the end, which grows over the
// tip's section, and the other one then ends on that face as at any corner
// of three edges (a bite cut part of the way along a wall, mitcad#1988).
// Empty where the failure is another one.
std::vector<int> knife_line_apart(const BRepFilletAPI_MakeFillet& maker, const Shape& body,
                                  const std::vector<int>& owner) {
  std::vector<int> later;
  if (maker.Failure() != ChFi3d_Builder::Failure_Corner) {
    return later;
  }
  const detail::EdgeFaces edge_faces(body);
  for (int k = 1; k <= maker.NbFaultyVertices(); ++k) {
    if (std::string(maker.FailedCornerMessage(k).ToCString()).find("a knife edge running on along its line") ==
        std::string::npos) {
      continue;
    }
    const TopoDS_Vertex vertex = maker.FaultyVertex(k);
    for (int ic = 1; ic <= maker.NbContours(); ++ic) {
      bool at_vertex = false;
      bool knife = false;
      for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
        const TopoDS_Edge& e = maker.Edge(ic, ie);
        if (!TopExp::FirstVertex(e).IsSame(vertex) && !TopExp::LastVertex(e).IsSame(vertex)) {
          continue;
        }
        at_vertex = true;
        const int edge = body.edge_index(e);
        const std::vector<int> faces = edge >= 0 ? edge_faces.at(edge) : std::vector<int>();
        knife = faces.size() == 2 &&
                detail::normal_angle(body.edge(edge), body.face(faces[0]), body.face(faces[1])) > kKnife;
      }
      if (!at_vertex || knife) {
        continue;
      }
      for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
        const int edge = body.edge_index(maker.Edge(ic, ie));
        if (edge >= 0 && owner[static_cast<std::size_t>(edge)] >= 0 &&
            std::find(later.begin(), later.end(), edge) == later.end()) {
          later.push_back(edge);
        }
      }
    }
  }
  return later;
}

// The edges of `result` that are pieces of edge `edge` of `body`: on its
// curve, within its range (an earlier pass of a fillet cuts a selected edge
// in two where another rounding's blend meets it, and the piece beyond
// then lies between other faces, under another name).
std::vector<int> pieces_of(const Shape& body, int edge, const Shape& result) {
  std::vector<int> out;
  double first = 0.0, last = 0.0;
  const Handle(Geom_Curve) curve = BRep_Tool::Curve(body.edge(edge), first, last);
  if (curve.IsNull()) {
    return out;
  }
  Bnd_Box box;
  BRepBndLib::Add(body.edge(edge), box);
  box.Enlarge(1.0e-4);
  const double within = 1.0e-5;
  for (int e = 0; e < result.edge_count(); ++e) {
    const TopoDS_Edge& candidate = result.edge(e);
    if (BRep_Tool::Degenerated(candidate)) {
      continue;
    }
    const BRepAdaptor_Curve c(candidate);
    bool on = true;
    for (const double t : {0.5, 0.2, 0.8}) {
      const gp_Pnt p = c.Value(c.FirstParameter() + t * (c.LastParameter() - c.FirstParameter()));
      if (box.IsOut(p)) {
        on = false;
        break;
      }
      GeomAPI_ProjectPointOnCurve projection(p, curve, first, last);
      if (projection.NbPoints() == 0 || !(projection.LowerDistance() < within)) {
        on = false;
        break;
      }
    }
    if (on) {
      out.push_back(e);
    }
  }
  return out;
}

// Foot corners (mitcad#1987): a vertex of three edges where a selected
// concave edge (a leg's side standing on a plate: its foot) and a selected
// convex one (the side's edge with the leg's top sloping down to the plate)
// end together, the angle between them on their common face acute (less
// than kFootCornerAngle), the third edge (the top's with the plate)
// concave and not selected. The `.f3d` files' bodies there are the foot's
// rounding first, run on to the front and cut by the top, and the top's
// rounding on that result, running on over the foot's blend down to the
// plate (a corner patch of 2.3 mm² in GitHub #371's Fillet1, matched to
// 1e-3 mm²); OCCT's single build joins the two roundings where their
// contacts cross instead, 0.3 to 0.4 mm off. So the convex edges at such
// corners come after the others (FootCorners, round_edges).
constexpr double kFootCornerAngle = kHalfPi * 2.0 / 3.0;

class FootCorners {
public:
  FootCorners(const Shape& body, const std::vector<int>& owner)
      : m_body(body), m_owner(owner), m_edgeFaces(body) {
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_EDGE, m_edgesAt);
  }

  // The later edges of each group of foot corners: a corner's convex edge
  // with every selected convex edge that meets a later one at a vertex or
  // on a face (the same roundings' other ends, whose corners then stay as
  // the single build makes them), groups that share an edge made one.
  std::vector<std::set<int>> groups() {
    std::vector<std::set<int>> out;
    for (int v = 1; v <= m_edgesAt.Extent(); ++v) {
      const std::vector<int> at = edges_of(v);
      if (at.size() != 3) {
        continue;
      }
      int convex = -1, concave = -1, other = -1;
      for (const int e : at) {
        if (!selected(e)) {
          other = other < 0 ? e : -2;
        } else if (kind_of(e) == 1) {
          convex = convex < 0 ? e : -2;
        } else if (kind_of(e) == 0) {
          concave = concave < 0 ? e : -2;
        }
      }
      if (convex < 0 || concave < 0 || other < 0 || kind_of(other) != 0) {
        continue;
      }
      const std::vector<int> cf = m_edgeFaces.at(convex), kf = m_edgeFaces.at(concave);
      if (std::none_of(cf.begin(), cf.end(), [&](int f) { return std::find(kf.begin(), kf.end(), f) != kf.end(); })) {
        continue;
      }
      const TopoDS_Vertex& vertex = TopoDS::Vertex(m_edgesAt.FindKey(v));
      const std::optional<gp_Vec> a = leaving(m_body.edge(convex), vertex), b = leaving(m_body.edge(concave), vertex);
      if (!a || !b || !(a->Angle(*b) < kFootCornerAngle)) {
        continue;
      }
      m_feet.insert(concave);
      m_seeds.insert(convex);
      if (std::any_of(out.begin(), out.end(), [&](const std::set<int>& g) { return g.count(convex) > 0; })) {
        continue;
      }
      std::set<int> group = closure({convex});
      std::vector<std::set<int>> kept;
      for (std::set<int>& g : out) {
        if (std::any_of(g.begin(), g.end(), [&](int e) { return group.count(e) > 0; })) {
          group.insert(g.begin(), g.end());
        } else {
          kept.push_back(std::move(g));
        }
      }
      kept.push_back(std::move(group));
      out = std::move(kept);
    }
    return out;
  }

  // The passes after the first for the later edges `later`, in their
  // order. A concave edge that meets a later one at a vertex where they
  // are the only selected edges (not a foot) is built with it, in a pass
  // of its own before the other later edges: built first, the later
  // rounding would run on over its blend onto the edges beyond, as one
  // tangent chain (the legs' outer tops at the tower's front face, rounded
  // with 5 at the top's foot).
  std::vector<std::vector<int>> passes(const std::set<int>& later) {
    std::set<int> together;
    for (int v = 1; v <= m_edgesAt.Extent(); ++v) {
      std::vector<int> chosen;
      for (const int e : edges_of(v)) {
        if (selected(e)) {
          chosen.push_back(e);
        }
      }
      if (chosen.size() != 2) {
        continue;
      }
      for (std::size_t k = 0; k < 2; ++k) {
        const int a = chosen[k], b = chosen[1 - k];
        if (later.count(a) > 0 && later.count(b) == 0 && m_feet.count(b) == 0 && kind_of(b) == 0) {
          together.insert(a);
          together.insert(b);
        }
      }
    }
    std::vector<std::vector<int>> out;
    if (!together.empty()) {
      out.emplace_back(together.begin(), together.end());
    }
    std::vector<int> rest;
    for (const int e : later) {
      if (together.count(e) == 0) {
        rest.push_back(e);
      }
    }
    if (!rest.empty()) {
      out.push_back(rest);
    }
    return out;
  }

  // The corners' own convex edges (after groups()): built after the
  // others of their pass, where three roundings meet at the far ends of
  // the legs' tops (OCCT's corner there found an edge twice otherwise).
  const std::set<int>& seeds() const { return m_seeds; }

private:
  bool selected(int e) const { return e >= 0 && m_owner[static_cast<std::size_t>(e)] >= 0; }

  // Convex (1), concave (0) or neither (-2), along the whole edge.
  int kind_of(int e) {
    const auto found = m_kinds.find(e);
    if (found != m_kinds.end()) {
      return found->second;
    }
    int kind = -2;
    try {
      OCC_CATCH_SIGNALS
      const std::vector<int> faces = m_edgeFaces.at(e);
      if (faces.size() == 2 && faces[0] != faces[1]) {
        const ChFiDS_TypeOfConcavity type = ChFi3d::DefineConnectType(m_body.edge(e), m_body.face(faces[0]),
                                                                      m_body.face(faces[1]), 1.0e-4, false);
        kind = type == ChFiDS_Convex ? 1 : type == ChFiDS_Concave ? 0 : -2;
      }
    } catch (const Standard_Failure&) {
    }
    m_kinds[e] = kind;
    return kind;
  }

  // The non-degenerate edges at vertex v (its index in m_edgesAt).
  std::vector<int> edges_of(int v) const {
    std::vector<int> out;
    for (const TopoDS_Shape& e : m_edgesAt(v)) {
      const int i = m_body.edge_index(e);
      if (i >= 0 && !BRep_Tool::Degenerated(TopoDS::Edge(e)) && std::find(out.begin(), out.end(), i) == out.end()) {
        out.push_back(i);
      }
    }
    return out;
  }

  // The edge's tangent leaving the vertex.
  static std::optional<gp_Vec> leaving(const TopoDS_Edge& edge, const TopoDS_Vertex& vertex) {
    const BRepAdaptor_Curve curve(edge);
    const double t = BRep_Tool::Parameter(vertex, edge);
    gp_Pnt p;
    gp_Vec d;
    curve.D1(t, p, d);
    if (d.Magnitude() < 1.0e-9) {
      return std::nullopt;
    }
    const bool at_last = std::abs(t - curve.LastParameter()) < std::abs(t - curve.FirstParameter());
    return at_last ? -d : d;
  }

  // The selected convex edges meeting one of `later` at a vertex or on a
  // face, until there are no more, with `later`.
  std::set<int> closure(std::set<int> later) {
    for (bool grew = true; grew;) {
      grew = false;
      std::set<int> faces;
      NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
      for (const int e : later) {
        const std::vector<int> f = m_edgeFaces.at(e);
        faces.insert(f.begin(), f.end());
        vertices.Add(TopExp::FirstVertex(m_body.edge(e)));
        vertices.Add(TopExp::LastVertex(m_body.edge(e)));
      }
      for (int e = 0; e < m_body.edge_count(); ++e) {
        if (!selected(e) || later.count(e) > 0 || kind_of(e) != 1) {
          continue;
        }
        const std::vector<int> f = m_edgeFaces.at(e);
        if (vertices.Contains(TopExp::FirstVertex(m_body.edge(e))) ||
            vertices.Contains(TopExp::LastVertex(m_body.edge(e))) ||
            std::any_of(f.begin(), f.end(), [&](int x) { return faces.count(x) > 0; })) {
          later.insert(e);
          grew = true;
        }
      }
    }
    return later;
  }

  const Shape& m_body;
  const std::vector<int>& m_owner;
  const detail::EdgeFaces m_edgeFaces;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> m_edgesAt;
  std::map<int, int> m_kinds;
  std::set<int> m_feet;
  std::set<int> m_seeds;
};

// Where the rounded corners at both ends of a selected straight edge meet
// (or all but meet) in its middle (GitHub #353: a cube of 10 rounded by 5
// on all its edges, every face taken away, the body a ball): the edge
// between two corners where three selected edges of constant radius r meet
// square to each other, the corner's ball reaching r along each of them,
// within the gap the geometry kernel needs between them of 2r (2e-4 r: at a gap of 4e-4 mm on that cube its
// fillet ran for good, its walk projecting a curve onto a face that has
// none of it left, and a cancel does not reach it there; at 1e-3 mm it
// builds). A refusal in words, the edge as its place (`meeting`, where
// given, set to the edge); empty where there is no such edge. (Port patch
// 1701 makes the kernel return there, mitcad#1922; the refusal stays: such
// a fillet is built as its core swept by the ball, cored_dressup.hpp, or
// refused, mitcad#1954.)
std::string corners_meet(const Shape& body, const std::vector<FilletSet>& sets, const std::vector<int>& owner,
                         double scale, int* meeting = nullptr) {
  try {
    OCC_CATCH_SIGNALS
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edges_at;
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_at);
    const auto radius_of = [&](int e) {
      const int s = e >= 0 && e < static_cast<int>(owner.size()) ? owner[static_cast<std::size_t>(e)] : -1;
      if (s < 0 || sets[static_cast<std::size_t>(s)].size != FilletSize::Constant) {
        return -1.0;
      }
      return sets[static_cast<std::size_t>(s)].radius * scale;
    };
    const auto direction = [](const TopoDS_Edge& edge) -> std::optional<gp_Dir> {
      const BRepAdaptor_Curve curve(edge);
      if (curve.GetType() != GeomAbs_Line) {
        return std::nullopt;
      }
      return curve.Line().Direction();
    };
    // A corner of three selected straight edges square to each other, all
    // of radius r.
    const auto square_corner = [&](const TopoDS_Vertex& v, double r) {
      if (!edges_at.Contains(v)) {
        return false;
      }
      std::vector<gp_Dir> directions;
      for (const TopoDS_Shape& other : edges_at.FindFromKey(v)) {
        const int i = body.edge_index(other);
        const std::optional<gp_Dir> d = direction(TopoDS::Edge(other));
        if (!d || std::abs(radius_of(i) - r) > 1.0e-9 * r) {
          return false;
        }
        directions.push_back(*d);
      }
      if (directions.size() != 3) {
        return false;
      }
      for (std::size_t a = 0; a < 3; ++a) {
        for (std::size_t b = a + 1; b < 3; ++b) {
          if (std::abs(directions[a].Dot(directions[b])) > 1.0e-9) {
            return false;
          }
        }
      }
      return true;
    };
    for (int e = 0; e < body.edge_count(); ++e) {
      const double r = radius_of(e);
      if (!(r > 0.0)) {
        continue;
      }
      const TopoDS_Edge& edge = TopoDS::Edge(body.edge(e));
      if (!direction(edge)) {
        continue;
      }
      const TopoDS_Vertex first = TopExp::FirstVertex(edge);
      const TopoDS_Vertex last = TopExp::LastVertex(edge);
      if (first.IsNull() || last.IsNull() || !square_corner(first, r) || !square_corner(last, r)) {
        continue;
      }
      const double gap = BRep_Tool::Pnt(first).Distance(BRep_Tool::Pnt(last)) - 2.0 * r;
      if (std::abs(gap) > 2.0e-4 * r) {
        continue;
      }
      if (meeting) {
        *meeting = e;
      }
      return placed_error(
          "the roundings of the corners at both ends of an edge meet, leaving none of the edge to round between "
          "them, which the geometry kernel cannot build",
          "the roundings of the corners at both ends of " + describe_edge(body, e) +
              " meet, leaving " + std::to_string(std::max(0.0, gap)) + " mm of it to round between them",
          {edge_place(body.edge_name(e))});
    }
  } catch (const Standard_Failure&) {
  }
  return {};
}

// Where the rounded corners at both ends of a selected edge meet
// (corners_meet): the fillet built exactly as the body's core swept by the
// ball (cored_dressup.hpp: a prism of planar faces, every selected edge of
// one constant radius); where it cannot be built so, refused in words, with
// the radius below which some of the edge is left to round between the
// corners and no 0.1 % smaller retry (std::invalid_argument: the retry
// would leave a sliver of every face the fillet takes away). Null where
// no corners meet.
ShapePtr where_corners_meet(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                            const std::vector<int>& owner, double scale) {
  int meeting = -1;
  const std::string meet = corners_meet(body, sets, owner, scale, &meeting);
  if (meet.empty()) {
    return nullptr;
  }
  std::vector<double> radius(static_cast<std::size_t>(body.edge_count()), 0.0);
  bool constant = true;
  for (int e = 0; e < body.edge_count(); ++e) {
    const int s = e < static_cast<int>(owner.size()) ? owner[static_cast<std::size_t>(e)] : -1;
    if (s < 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    constant = constant && set.size == FilletSize::Constant && !set.curvature;
    radius[static_cast<std::size_t>(e)] = set.radius * scale;
  }
  detail::CoredFillet cored;
  if (constant) {
    cored = detail::cored_fillet(feature, body, radius);
    if (cored.shape) {
      return cored.shape;
    }
  }
  // The largest radius the kernel builds there: the edge longer than 2r by
  // more than corners_meet's 2e-4 r, in thousandths of a millimetre.
  double length = 0.0;
  if (meeting >= 0) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(TopoDS::Edge(body.edge(meeting)), a, b);
    if (!a.IsNull() && !b.IsNull()) {
      length = BRep_Tool::Pnt(a).Distance(BRep_Tool::Pnt(b));
    }
  }
  const double fits = std::floor(length / (2.0 + 2.0e-4) * 1000.0) / 1000.0;
  char text[32];
  std::snprintf(text, sizeof text, "%.3f", fits);
  const std::string below = std::string(": a radius of ") + text + " mm or less leaves some of the edge to round";
  std::vector<ErrorPlace> places;
  if (meeting >= 0) {
    places.push_back(edge_place(body.edge_name(meeting)));
  }
  const std::string edge = meeting >= 0 ? describe_edge(body, meeting) : std::string("an edge");
  std::string said = "the roundings of the corners at both ends of an edge meet, leaving none of the edge to round "
                     "between them";
  std::string named = "the roundings of the corners at both ends of " + edge + " meet, leaving none of it to " +
                      "round between them";
  if (!cored.inner.empty()) {
    const std::string inner = ", and an inner corner's edge is rounded with them, which cannot be built there";
    said += inner;
    named += inner + " (";
    for (std::size_t i = 0; i < cored.inner.size(); ++i) {
      named += (i == 0 ? "" : ", ") + describe_edge(body, cored.inner[i]);
      places.push_back(edge_place(body.edge_name(cored.inner[i])));
    }
    named += ")";
  } else {
    const std::string only = ", which is built exactly only on a block of flat faces whose outer edges between "
                             "the faces it rounds are all rounded by one radius";
    said += only;
    named += only;
  }
  throw std::invalid_argument("fillet failed; " + placed_error(said + below, named + below, places));
}

// The rolling ball fillets of the sets (none asymmetric or G2) that own the
// body's edges, at their sizes times `scale`, their contours added in the
// order the sets list their edges (`listed`) or in the body's.
ShapePtr round_edges(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                     const std::vector<int>& owner, bool rolling_ball_corners, double scale, bool tight,
                     bool listed = true, bool foot_corners = true, const std::vector<int>& late = {}) {
  // (Corners that meet in an edge's middle: never given to the kernel's
  // fillet, which never returns there, corners_meet; built as the body's
  // core swept by the ball, or refused in words with no smaller retry,
  // mitcad#1954.)
  if (rolling_ball_corners) {
    if (ShapePtr cored = where_corners_meet(feature, body, sets, owner, scale)) {
      return cored;
    }
  }
  // Foot corners' convex roundings after the others (FootCorners,
  // mitcad#1987), group by group as long as the passes build; with none
  // that builds, the single build as before. (The passes' own builds look
  // for no foot corners again, nor does the retry in the body's order
  // below.)
  if (rolling_ball_corners && foot_corners) {
    FootCorners corners(body, owner);
    const std::vector<std::set<int>> groups = corners.groups();
    ShapePtr best;
    std::set<int> taken;
    for (const std::set<int>& group : groups) {
      std::set<int> later = taken;
      later.insert(group.begin(), group.end());
      try {
        OCC_CATCH_SIGNALS
        const std::vector<std::vector<int>> passes = corners.passes(later);
        std::vector<int> first = owner;
        for (const std::vector<int>& pass : passes) {
          for (const int edge : pass) {
            first[static_cast<std::size_t>(edge)] = -1;
          }
        }
        ShapePtr result = round_edges(feature, body, sets, first, rolling_ball_corners, scale, tight, listed, false);
        for (const std::vector<int>& pass : passes) {
          std::vector<int> next(static_cast<std::size_t>(result->edge_count()), -1);
          std::vector<int> pass_late;
          for (const int edge : pass) {
            const std::vector<int> found = pieces_of(body, edge, *result);
            if (found.empty()) {
              throw std::runtime_error("a foot corner's edge not found again");
            }
            for (const int piece : found) {
              next[static_cast<std::size_t>(piece)] = owner[static_cast<std::size_t>(edge)];
              if (corners.seeds().count(edge) > 0) {
                pass_late.push_back(piece);
              }
            }
          }
          result = round_edges(feature, *result, sets, next, rolling_ball_corners, scale, tight, listed, false, pass_late);
        }
        best = result;
        taken = later;
      } catch (const Cancelled&) {
        throw;
      } catch (const Standard_Failure&) {
      } catch (const std::exception&) {
      }
    }
    if (best) {
      return best;
    }
  }
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
  // (The edges `late` names last: a foot corner's convex edges after the
  // others of their pass, FootCorners.)
  std::stable_partition(order.begin(), order.end(),
                        [&](int edge) { return std::find(late.begin(), late.end(), edge) == late.end(); });
  for (const int edge : order) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0 || maker.Contour(body.edge(edge)) != 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    const TopoDS_Edge& e = body.edge(edge);
    switch (set.size) {
    case FilletSize::Constant:
    case FilletSize::FullRound: // (its radius worked out already)
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
        return round_edges(feature, body, sets, owner, rolling_ball_corners, scale, tight, false, false, late);
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
    std::vector<int> later = setbacks.empty() ? concave_after_convex(maker, body, owner) : std::vector<int>();
    // Two chains of roundings that the kernel finds overlapping on a face
    // between them and cannot run over it as one, where they share no
    // vertex: the one, then the other on its result, its ball rolling on the
    // first one's faces where they meet (overlap_apart, mitcad#1926).
    if (later.empty() && setbacks.empty()) {
      later = overlap_apart(maker, body, owner);
    }
    // A knife edge and an edge running on along its line, the one, then
    // the other (knife_line_apart, mitcad#1988).
    if (later.empty() && setbacks.empty()) {
      later = knife_line_apart(maker, body, owner);
    }
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
  // (A rounding the kernel built away from its edges, a face between taken
  // away whole: a failure, never a body with a wall missing, mitcad#1953.)
  const detail::EdgeFaces contour_faces(body);
  const std::string beyond = took_face_beyond_reach(maker, body, "fillet", [&](int ic, const TopoDS_Edge& edge) {
    // (A contour the kernel's build split or added, or one carried across a
    // notch: the largest size of the sets.)
    const int e = body.edge_index(edge);
    const std::size_t c = static_cast<std::size_t>(ic - 1);
    const int s = c < contour_sets.size() ? contour_sets[c] : -1;
    const double size =
        (s < 0 ? largest_size(sets) : largest_radius(body, sets[static_cast<std::size_t>(s)], e)) * scale;
    const std::vector<int> faces = e < 0 ? std::vector<int>() : contour_faces.at(e);
    if (faces.size() != 2 || faces[0] == faces[1]) {
      return 0.0;
    }
    double angle = 0.0;
    try {
      OCC_CATCH_SIGNALS
      for (const double t : {0.0, 0.5, 1.0}) {
        angle = std::max(angle, detail::normal_angle(edge, body.face(faces[0]), body.face(faces[1]), t));
      }
    } catch (const Standard_Failure&) {
      return 0.0;
    } catch (const std::runtime_error&) {
      // (A normal undefined at an end of the edge.)
      return 0.0;
    }
    if (!(angle < 2.0 * kHalfPi - 1.0e-3)) {
      return 0.0;
    }
    return size * std::max(1.0, std::tan(angle / 2.0));
  });
  if (!beyond.empty()) {
    throw std::runtime_error("fillet failed; " + beyond);
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
      throw std::invalid_argument(edges_error(body, {edge}, "an edge has no face to measure the chamfer on",
                                              describe_edge(body, edge) + " has no face to measure on"));
    }
    return measured;
  }
  const auto [a, b] = edge_face_pair(body, edge);
  const bool on_a = face_matches(body.face_names(a), reference);
  const bool on_b = face_matches(body.face_names(b), reference);
  if (on_a == on_b) {
    throw std::invalid_argument(placed_error(
        on_a ? "an edge lies between two pieces of the face its distance is measured on"
             : "an edge does not border the face its distance is measured on",
        describe_edge(body, edge) + (on_a ? " lies between two pieces of face " : " does not border face ") +
            reference,
        {edge_place(body.edge_name(edge)), face_place(reference)}));
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
    throw std::invalid_argument(edges_error(
        body, {edge}, "unsupported: a miter corner at an edge that is neither convex nor concave along its length",
        "unsupported: a miter corner at " + describe_edge(body, edge) +
            ", which is neither convex nor concave along its length"));
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
    throw std::invalid_argument(edges_error(body, {edge}, "an edge joins its faces tangentially",
                                            describe_edge(body, edge) + " joins its faces tangentially"));
  }
  if (set.size == FilletSize::ChordLength) {
    return set.chord / (2.0 * std::cos(angle / 2.0));
  }
  return set.radius * std::tan(angle / 2.0);
}

// The curvature continuous corner at a vertex of the body where a blended
// chain meets other chains (mitcad#2332): every edge there rounded, by
// curvature continuous or constant radius (or chord length) tangent
// continuous sets; refused in words otherwise. Setback corners stop the
// roundings kSetback times the largest radius there from the vertex,
// rolling ball ones as far as the corner needs (g2_corner.hpp).
detail::G2Corner curvature_corner(const Shape& body, int vertex, const std::vector<FilletSet>& sets,
                                  const std::vector<int>& owner, const std::vector<char>& blended,
                                  bool rolling_ball_corners, double scale) {
  const TopoDS_Vertex& at = body.vertex(vertex);
  const std::string& name = body.vertex_name(vertex);
  const auto refuse = [&](const std::string& said, int edge) {
    std::vector<ErrorPlace> places{vertex_place(name)};
    if (edge >= 0) {
      places.push_back(edge_place(body.edge_name(edge)));
    }
    throw std::invalid_argument(placed_error(said, said + " (vertex " + name + ")", places));
  };
  std::vector<detail::CornerLeg> legs;
  double largest = 0.0;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const TopoDS_Edge& e = body.edge(edge);
    if (BRep_Tool::Degenerated(e) ||
        !(TopExp::FirstVertex(e).IsSame(at) || TopExp::LastVertex(e).IsSame(at))) {
      continue;
    }
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0) {
      refuse("unsupported: curvature continuous fillets that meet other rounded edges at a corner where an "
             "edge is not rounded",
             edge);
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    if (set.size == FilletSize::Asymmetric) {
      refuse("unsupported: asymmetric fillets that meet other rounded edges at a corner", edge);
    }
    if (blended[static_cast<std::size_t>(s)] == 0 && set.size != FilletSize::Constant &&
        set.size != FilletSize::ChordLength && set.size != FilletSize::FullRound) {
      refuse("unsupported: a curvature continuous corner where a rounding of a variable radius meets it", edge);
    }
    detail::CornerLeg leg;
    leg.edge = edge;
    leg.contact = contact_distance(body, edge, set) * scale;
    leg.curvature = set.curvature;
    legs.push_back(leg);
    largest = std::max(largest, largest_radius(body, set, edge) * scale);
  }
  bool curvature = false;
  for (const detail::CornerLeg& leg : legs) {
    curvature = curvature || leg.curvature;
  }
  if (!curvature) {
    refuse("unsupported: asymmetric fillets that meet other rounded edges at a corner", -1);
  }
  return detail::g2_corner(body, vertex, legs, rolling_ball_corners ? 0.0 : kSetback * largest);
}

// Fillets with asymmetric or G2 sets (`blended`): the other sets' rolling
// ball fillets first, then a chamfer of the blended sets at their contact
// distances, its bevels replaced by blend surfaces. Where a G2 chain meets
// other rounded edges at a vertex, the corner is cut back to planes across
// its edges first, the roundings end there, and a curvature continuous
// patch closes it (g2_corner.hpp, mitcad#2332); where an edge there is not
// rounded, the chains meet in a mitre. An asymmetric chain that meets
// other rounded edges where every edge is rounded is unsupported.
ShapePtr round_and_blend(const std::string& feature, const Shape& input, const std::vector<FilletSet>& sets,
                         const std::vector<int>& input_owner, const std::vector<char>& blended,
                         bool rolling_ball_corners, double scale, bool tight) {
  // The chains of all sets, to see where the blended ones end.
  BRepFilletAPI_MakeChamfer probe(input.occt());
  for (int edge = 0; edge < input.edge_count(); ++edge) {
    if (input_owner[static_cast<std::size_t>(edge)] >= 0 && probe.Contour(input.edge(edge)) == 0) {
      probe.Add(1.0, input.edge(edge));
    }
  }
  const std::vector<int> contour_sets = check_contours(probe, input, sets, input_owner);
  std::vector<ShapeMap> ends(contour_sets.size());
  for (std::size_t c = 0; c < contour_sets.size(); ++c) {
    const int ic = static_cast<int>(c) + 1;
    for (int ie = 1; ie <= probe.NbEdges(ic); ++ie) {
      TopExp::MapShapes(probe.Edge(ic, ie), TopAbs_VERTEX, ends[c]);
    }
  }
  // Where a blended chain meets others: a G2 corner where every edge at the
  // vertex is rounded, its chains then built together; a mitre where an
  // edge there is not (a box's two top edges at a corner whose upright edge
  // stays sharp), its chains rounded apart and the results intersected, so
  // that each rounding runs on to the face beyond and the two meet in a
  // crease that carries on the sharp edge, as the kernel's own roundings
  // meet there (mitcad#2332).
  const std::size_t count = contour_sets.size();
  std::vector<detail::G2Corner> corners;
  std::set<int> corner_vertices;
  std::vector<std::size_t> root(count);
  for (std::size_t c = 0; c < count; ++c) {
    root[c] = c;
  }
  const auto group_of = [&root](std::size_t c) {
    while (root[c] != c) {
      c = root[c] = root[root[c]];
    }
    return c;
  };
  std::vector<std::pair<std::size_t, std::size_t>> apart;
  std::set<int> mitre_kinds;
  for (std::size_t c = 0; c < count; ++c) {
    if (contour_sets[c] < 0 || blended[static_cast<std::size_t>(contour_sets[c])] == 0) {
      continue;
    }
    for (std::size_t other = 0; other < count; ++other) {
      for (int v = 1; other != c && v <= ends[other].Extent(); ++v) {
        const TopoDS_Shape& at = ends[other](v);
        const int vertex = input.vertex_index(at);
        if (!ends[c].Contains(at) || vertex < 0 || !corner_vertices.insert(vertex).second) {
          continue;
        }
        std::vector<std::size_t> here;
        for (std::size_t d = 0; d < count; ++d) {
          if (ends[d].Contains(at)) {
            here.push_back(d);
          }
        }
        bool every = true;
        for (int edge = 0; edge < input.edge_count() && every; ++edge) {
          const TopoDS_Edge& e = input.edge(edge);
          every = BRep_Tool::Degenerated(e) ||
                  !(TopExp::FirstVertex(e).IsSame(at) || TopExp::LastVertex(e).IsSame(at)) ||
                  input_owner[static_cast<std::size_t>(edge)] >= 0;
        }
        if (every) {
          corners.push_back(
              curvature_corner(input, vertex, sets, input_owner, blended, rolling_ball_corners, scale));
          for (const std::size_t d : here) {
            root[group_of(d)] = group_of(here.front());
          }
          continue;
        }
        for (std::size_t i = 0; i < here.size(); ++i) {
          for (std::size_t j = 0; j < i; ++j) {
            apart.emplace_back(here[i], here[j]);
          }
          const int ic = static_cast<int>(here[i]) + 1;
          for (int ie = 1; ie <= probe.NbEdges(ic); ++ie) {
            const TopoDS_Edge& e = probe.Edge(ic, ie);
            const int index = input.edge_index(e);
            if (index >= 0 && (TopExp::FirstVertex(e).IsSame(at) || TopExp::LastVertex(e).IsSame(at))) {
              mitre_kinds.insert(convex_edge(input, index) ? 1 : 0);
              break;
            }
          }
        }
      }
    }
  }
  if (!apart.empty()) {
    if (mitre_kinds.size() > 1) {
      throw std::invalid_argument("unsupported: curvature continuous fillets that meet in a mitre where convex "
                                  "and concave edges meet");
    }
    // Groups by greedy colouring of the chains built together: a group
    // takes the first colour none of the chains it meets in a mitre has.
    std::map<std::size_t, int> colour;
    for (std::size_t c = 0; c < count; ++c) {
      const std::size_t r = group_of(c);
      if (colour.count(r) > 0) {
        continue;
      }
      std::set<int> taken;
      for (const auto& [x, y] : apart) {
        const std::size_t rx = group_of(x);
        const std::size_t ry = group_of(y);
        if (rx == ry) {
          throw std::invalid_argument("unsupported: curvature continuous fillets that meet both at a corner "
                                      "where every edge is rounded and in a mitre");
        }
        const std::size_t partner = rx == r ? ry : ry == r ? rx : count;
        if (partner < count && colour.count(partner) > 0) {
          taken.insert(colour[partner]);
        }
      }
      int g = 0;
      while (taken.count(g) > 0) {
        ++g;
      }
      colour[r] = g;
    }
    int groups = 0;
    for (const auto& entry : colour) {
      groups = std::max(groups, entry.second + 1);
    }
    std::vector<ShapePtr> made;
    for (int g = 0; g < groups; ++g) {
      throw_if_cancelled();
      std::vector<int> only(static_cast<std::size_t>(input.edge_count()), -1);
      bool any_blended = false;
      for (int edge = 0; edge < input.edge_count(); ++edge) {
        const int s = input_owner[static_cast<std::size_t>(edge)];
        const int ic = s < 0 ? 0 : probe.Contour(input.edge(edge));
        if (ic > 0 && colour[group_of(static_cast<std::size_t>(ic) - 1)] == g) {
          only[static_cast<std::size_t>(edge)] = s;
          any_blended = any_blended || blended[static_cast<std::size_t>(s)] != 0;
        }
      }
      // Each group on a copy of its own: the kernel's dressups change their
      // input.
      const ShapePtr own = detail::working_copy(input);
      made.push_back(any_blended ? round_and_blend(feature, *own, sets, only, blended, rolling_ball_corners, scale,
                                                   tight)
                                 : round_edges(feature, *own, sets, only, rolling_ball_corners, scale, tight));
    }
    ShapePtr result = made.front();
    for (std::size_t g = 1; g < made.size(); ++g) {
      BooleanResult common = boolean(BooleanOp::Intersect, {result.get()}, *made[g]);
      if (!common.touched.front()) {
        continue;
      }
      if (common.pieces.size() != 1) {
        throw std::runtime_error("the mitred roundings split the body");
      }
      result = common.pieces.front().shape;
    }
    if (mitre_kinds.count(0) > 0) {
      // Inside edges add material: each group's, joined.
      for (const ShapePtr& group_result : made) {
        for (const BooleanPiece& wedge : boolean(BooleanOp::Cut, {group_result.get()}, input).pieces) {
          BooleanResult joined = boolean(BooleanOp::Join, {result.get()}, *wedge.shape);
          if (joined.pieces.size() != 1) {
            throw std::runtime_error("the mitred roundings could not be joined");
          }
          result = joined.pieces.front().shape;
        }
      }
    }
    for (const ShapePtr& group_result : made) {
      if (group_result != result) {
        result->copy_notes(*group_result);
      }
    }
    return result;
  }
  // An edge with corners at both ends must be longer than their setbacks.
  detail::share_edges(corners);
  // The corners cut back (or filled) to their planes: the roundings end
  // there, on planar faces.
  ShapePtr notched;
  std::vector<int> notched_owner;
  if (!corners.empty()) {
    notched = detail::notch_corners(feature, input, corners);
    notched_owner.assign(static_cast<std::size_t>(notched->edge_count()), -1);
    for (int edge = 0; edge < notched->edge_count(); ++edge) {
      const std::string& name = notched->edge_name(edge);
      const std::vector<int> found = name.empty() ? std::vector<int>() : input.find_edges(name);
      if (found.size() == 1) {
        notched_owner[static_cast<std::size_t>(edge)] = input_owner[static_cast<std::size_t>(found.front())];
      }
    }
  }
  const Shape& body = notched ? *notched : input;
  const std::vector<int>& owner = notched ? notched_owner : input_owner;
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
    case FilletSize::FullRound:
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
  // (As round_edges': a bevel built away from its edges, mitcad#1953.)
  const std::string beyond = took_face_beyond_reach(maker, start, "fillet", [&](int, const TopoDS_Edge& edge) {
    const auto name = start.name_of_edge(edge);
    const auto found = name ? set_of_edge.find(*name) : set_of_edge.end();
    if (found == set_of_edge.end()) {
      return 0.0;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(found->second)];
    if (set.size == FilletSize::Asymmetric) {
      return std::max(set.radius, set.radius2) * scale;
    }
    try {
      OCC_CATCH_SIGNALS
      return contact_distance(start, start.edge_index(edge), set) * scale;
    } catch (const Standard_Failure&) {
      return 0.0;
    } catch (const std::exception&) {
      return 0.0;
    }
  });
  if (!beyond.empty()) {
    throw std::runtime_error("fillet failed; " + beyond);
  }
  const ShapePtr chamfered = name_result(maker, feature, "fillet", start);
  const ShapePtr smooth = detail::blend_bevels(feature, start, *chamfered, kinds);
  return corners.empty() ? smooth : detail::g2_corner_patches(feature, *smooth, corners);
}

// A dressup built as rings gets a note only where its result is faulty
// (mitcad#1327: how it was built is no warning): the ring's check
// (dressup_check.hpp's ring_check), with its places.
void note_ring_faults(Shape& result, const Shape& body, const std::string& feature, const char* role,
                      const std::vector<detail::RingEdge>& rings) {
  double size = 0.0;
  for (const detail::RingEdge& ring : rings) {
    size = std::max({size, ring.radius, ring.distance, ring.distance2});
  }
  detail::DressupFinding found = detail::ring_check(body, result, feature, role, size);
  if (!found.empty()) {
    result.add_note(std::move(found.text), std::move(found.places));
  }
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
    case FilletSize::FullRound: // (its radius worked out already)
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
    note_ring_faults(*result, body, feature, "fillet", rings);
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

// The fillet's roundings as valleys of tubes run on through their nodes
// (node_dressup.hpp): roundings of constant radius of concave edges between
// tubes of one radius, meeting at nodes whose facets are narrower than the
// radius; null when they are not such roundings or cannot be built so.
ShapePtr node_roundings(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                        const std::vector<int>& owner) {
  std::vector<detail::NodeEdge> edges;
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
  return detail::node_fillet(feature, body, edges);
}

// Runs `dressup` (OCCT's fillet at its exact size); where the geometry
// kernel fails (not for invalid arguments), the roundings are built as
// mitred sweeps if they are roundings that take a face away round a loop of
// edges, whose corners OCCT cannot join (mitcad#1118), or as valleys of
// tubes run on through their nodes, whose facets' corners OCCT cannot
// build (mitcad#1205), else the kernel's error stands.
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
  ShapePtr node;
  try {
    OCC_CATCH_SIGNALS
    node = node_roundings(feature, *detail::working_copy(body), sets, owner);
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    node = nullptr;
  }
  if (node) {
    return node;
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
    note_ring_faults(*result, body, feature, "chamfer", rings);
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
  // (As the fillet's: a bevel built away from its edges, mitcad#1953.)
  const std::string beyond = took_face_beyond_reach(maker, body, "chamfer", [&](int ic, const TopoDS_Edge&) {
    const std::size_t c = static_cast<std::size_t>(ic - 1);
    const int s = c < contour_sets.size() ? contour_sets[c] : -1;
    return (s < 0 ? largest_size(sets) : largest_distance(sets[static_cast<std::size_t>(s)].spec)) * scale;
  });
  if (!beyond.empty()) {
    throw std::runtime_error("chamfer failed; " + beyond);
  }
  ShapePtr result = join_at_turn_stops(name_result(maker, feature, "chamfer", body), maker.ChainTurnStops());
  result = join_at_turn_stops(result, piece_vertices(*result, body), true);
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
// A result left open where the input is a closed solid is a failure of
// its try (OpenResult, mitcad#1819).
// Throws the first error when none builds.
template <class Set, class Dressup>
ShapePtr dress(const Shape& input, const std::vector<Set>& sets, const std::vector<int>& owner, double scale,
               const std::string& feature, const char* role, bool tighten, Dressup&& dressup) {
  const bool closed_input = closed_solids(input.occt());
  const auto closed = [&](ShapePtr result) {
    if (closed_input && result && !closed_solids(result->occt())) {
      throw OpenResult(role);
    }
    return result;
  };
  std::exception_ptr first;
  try {
    OCC_CATCH_SIGNALS
    // OCCT's dressups give the edges of their input p-curves on the new
    // surfaces and wider tolerances (input_check.hpp): each try works on a
    // copy of its own.
    const ShapePtr copy = detail::working_copy(input);
    return closed(dressup(*copy, owner, scale, false));
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
        return closed(dressup(*copy, owner, scale, true));
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
        return closed(
            with_piece_names(dressup(*merged->body, merged->owner, scale, tight), *merged, feature, role));
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
      return closed(with_piece_names(dressup(*merged->body, merged->owner, scale, false), *merged, feature, role));
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
      return closed(dressup(*collapsed->body, collapsed->owner, scale, false));
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
  // (No note on how it was built, mitcad#1327: the dressup check after it
  // tells what is wrong with the result, if anything.)
  return detail::cut_out_again(rounded, *cutouts, *copy);
}

// A dressup of a body some of whose faces were merged before it was built
// (fill_overflow: narrow faces of one surface joined across the space
// filled past them, a face then holding the names of several, mitcad#1783),
// with the names of the input's edges and vertices: where an edge or a
// vertex of that body is named otherwise than the input's at the same place
// (after the merged face's first name), the input's name stands for it in
// every face name of the result (the dressup's faces named after the edges
// they round).
ShapePtr with_renames(const ShapePtr& result, std::vector<std::pair<std::string, std::string>> renames);
ShapePtr with_input_names(const ShapePtr& result, const Shape& built_on, const Shape& input) {
  if (!result) {
    return result;
  }
  const auto middle = [](const TopoDS_Edge& edge) {
    const BRepAdaptor_Curve curve(edge);
    return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
  };
  std::vector<gp_Pnt> input_middles;
  for (int e = 0; e < input.edge_count(); ++e) {
    input_middles.push_back(BRep_Tool::Degenerated(input.edge(e)) ? gp_Pnt(1e300, 1e300, 1e300)
                                                                   : middle(input.edge(e)));
  }
  constexpr double kSame = 1.0e-6;
  std::vector<std::pair<std::string, std::string>> renames;
  for (int i = 0; i < built_on.edge_count(); ++i) {
    const std::string& own = built_on.edge_name(i);
    if (own.empty() || BRep_Tool::Degenerated(built_on.edge(i))) {
      continue;
    }
    const gp_Pnt p = middle(built_on.edge(i));
    for (int e = 0; e < input.edge_count(); ++e) {
      if (input_middles[static_cast<std::size_t>(e)].Distance(p) <= kSame) {
        const std::string& theirs = input.edge_name(e);
        if (!theirs.empty() && theirs != own) {
          renames.emplace_back(own, theirs);
        }
        break;
      }
    }
  }
  std::vector<std::pair<gp_Pnt, std::string>> input_vertices;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
  TopExp::MapShapes(input.occt(), TopAbs_VERTEX, vertices);
  for (int v = 1; v <= vertices.Extent(); ++v) {
    if (const auto name = input.name_of_vertex(vertices(v))) {
      input_vertices.emplace_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices(v))), *name);
    }
  }
  vertices.Clear();
  TopExp::MapShapes(built_on.occt(), TopAbs_VERTEX, vertices);
  for (int v = 1; v <= vertices.Extent(); ++v) {
    const auto own = built_on.name_of_vertex(vertices(v));
    if (!own) {
      continue;
    }
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vertices(v)));
    for (const auto& [q, theirs] : input_vertices) {
      if (q.Distance(p) <= kSame) {
        if (theirs != *own) {
          renames.emplace_back(*own, theirs);
        }
        break;
      }
    }
  }
  return with_renames(result, std::move(renames));
}

// The result with its faces' names renamed: in each name every occurrence
// of a rename's first string (the longest where several start at a place)
// replaced by its second, in one pass, so that a name put in is not renamed
// again.
ShapePtr with_renames(const ShapePtr& result, std::vector<std::pair<std::string, std::string>> renames) {
  if (!result) {
    return result;
  }
  std::sort(renames.begin(), renames.end());
  renames.erase(std::unique(renames.begin(), renames.end()), renames.end());
  if (renames.empty()) {
    return result;
  }
  std::vector<Shape::NamedFace> faces;
  for (int f = 0; f < result->face_count(); ++f) {
    NameList names = result->face_names(f);
    // (In one pass, so that a name put in is not renamed again.)
    for (std::string& name : names) {
      std::string renamed;
      for (std::size_t at = 0; at < name.size();) {
        const std::pair<std::string, std::string>* longest = nullptr;
        for (const auto& rename : renames) {
          if (name.compare(at, rename.first.size(), rename.first) == 0 &&
              (longest == nullptr || rename.first.size() > longest->first.size())) {
            longest = &rename;
          }
        }
        if (longest != nullptr) {
          renamed += longest->second;
          at += longest->first.size();
        } else {
          renamed += name[at];
          ++at;
        }
      }
      name = renamed;
    }
    faces.push_back({result->face(f), names});
  }
  ShapePtr named = std::make_shared<Shape>(result->occt(), faces);
  named->copy_notes(*result);
  return named;
}

// Whether a face of `shape` holds a name that starts with `prefix`.
bool holds_made_face(const Shape& shape, const std::string& prefix) {
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& name : shape.face_names(f)) {
      if (name.compare(0, prefix.size(), prefix) == 0) {
        return true;
      }
    }
  }
  return false;
}

// The one path of a fillet or chamfer (`role`) wider than a narrow face
// beside its edges, which it takes away (mitcad#1232, mitcad#1819;
// cutouts.hpp's fill_overflow): `build` dresses the picked edges at their
// full size, measured as picked, on the body with the narrow faces
// continued along their own surfaces past their far edges, and the space
// filled there is cut out again, the walls taken away put back: the faces
// beyond grow out (or are cut back) along their own surfaces to meet the
// dressup's boundary, and the narrow faces are gone where it is wider.
// `contact` is how far from its edges the dressup touches its faces at
// most, `depth_ratio` how deep it goes under a narrow face for each unit it
// runs past its far edge (a chamfer's legs' ratio; 1 for a rounding). The
// cases of mitcad#1232 (convex edges), mitcad#1260 (concave), mitcad#1238
// (walls rising past the far edge, corners) and mitcad#1783 (faces ending
// in a point) are this path's. Null where no such face is found or the
// result is not one valid solid (never throws but for cancelling); with
// `pieces_apart`, where only joining the pieces of a face beyond on its
// surface makes it invalid, the result with them apart (a chamfer reaching
// exactly to a far edge, mitcad#1613). `freeform`, where given, tells
// whether a face beside the edges stays a B-spline surface that is no
// plane, cylinder, cone, sphere or torus, which the path cannot continue
// past its far edge (mitcad#1849: the failure then says so).
template <class Build>
ShapePtr over_narrow_faces(const std::string& feature, const char* role, const Shape& given,
                           const std::vector<int>& picked, double contact, double depth_ratio, Build&& build,
                           std::string* why = nullptr, bool pieces_apart = false,
                           const detail::OverflowReach* reach = nullptr, bool* freeform = nullptr) {
  // A face beside the edges that is a B-spline surface exactly a plane,
  // cylinder, cone, sphere or torus (a body converted to B-splines) as that
  // surface, which fill_overflow can continue past its far edge; the faces
  // keep their names, the edges theirs (mitcad#1819). Where OCCT's
  // confusion tolerance converts no face beside the edges, or leaves one a
  // B-spline (a sweep's faces, approximated more loosely), the analytic
  // form within a wider tolerance, up to the 1e-4 a sweep is approximated
  // to (mitcad#1849).
  ShapePtr analytic;
  std::vector<int> analytic_owner;
  if (freeform != nullptr) {
    *freeform = false;
  }
  try {
    OCC_CATCH_SIGNALS
    bool spline = false;
    const detail::EdgeFaces edge_faces(given);
    for (int e = 0; e < given.edge_count() && e < static_cast<int>(picked.size()) && !spline; ++e) {
      if (picked[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      for (const int f : edge_faces.at(e)) {
        spline = spline || BRepAdaptor_Surface(given.face(f), false).GetType() == GeomAbs_BSplineSurface;
      }
    }
    // Whether every face beside the picked edges of `shape` (found by the
    // edges' names) is no B-spline surface.
    const auto beside_analytic = [&](const Shape& shape) {
      const detail::EdgeFaces faces_of(shape);
      for (int e = 0; e < given.edge_count() && e < static_cast<int>(picked.size()); ++e) {
        if (picked[static_cast<std::size_t>(e)] < 0 || given.edge_name(e).empty()) {
          continue;
        }
        const std::vector<int> found = shape.find_edges(given.edge_name(e));
        if (found.empty()) {
          return false;
        }
        for (const int i : found) {
          for (const int f : faces_of.at(i)) {
            const GeomAbs_SurfaceType type = BRepAdaptor_Surface(shape.face(f), false).GetType();
            if (type == GeomAbs_BSplineSurface || type == GeomAbs_BezierSurface) {
              return false;
            }
          }
        }
      }
      return true;
    };
    if (spline) {
      BRep_Builder builder;
      TopoDS_Compound none;
      builder.MakeCompound(none);
      bool whole = false;
      for (const double exact : {1e-7, 1e-6, 1e-5, 1e-4}) {
        throw_if_cancelled();
        // (The seams of the cylinders, cones, spheres and tori opposite
        // their faces: fill_overflow's regions are bounded by the surfaces
        // with their own seams.)
        const ShapePtr tried = detail::canonical_dressup(Shape(none), given, exact, true);
        if (tried && beside_analytic(*tried)) {
          analytic = tried;
          whole = true;
          break;
        }
        // (Else the first that converts anything, as before mitcad#1849.)
        if (tried && !analytic) {
          analytic = tried;
        }
      }
      if (freeform != nullptr) {
        *freeform = !whole;
      }
    }
    if (analytic) {
      analytic_owner.assign(static_cast<std::size_t>(analytic->edge_count()), -1);
      for (int e = 0; e < given.edge_count() && e < static_cast<int>(picked.size()); ++e) {
        if (picked[static_cast<std::size_t>(e)] < 0 || given.edge_name(e).empty()) {
          continue;
        }
        for (const int i : analytic->find_edges(given.edge_name(e))) {
          analytic_owner[static_cast<std::size_t>(i)] = picked[static_cast<std::size_t>(e)];
        }
      }
    }
  } catch (const Standard_Failure&) {
    analytic = nullptr;
  }
  const Shape& input = analytic ? *analytic : given;
  const std::vector<int>& owner = analytic ? analytic_owner : picked;
  // First with only the space past the far edges filled, the geometry
  // kernel cutting the chamfer by a wall it meets there; then with the
  // walls rising past the far edges taken away and put back.
  // Last, where selected edges meet at a vertex, with each edge's regions
  // on its own (mitcad#1238: an overflowing chamfer meeting others there).
  // A body converted from B-splines first tries each face's regions within
  // its own part of the chain, between the planes that part it from the
  // next faces at the chain's corners, the walls put back whole
  // (fill_overflow's `sectors`, mitcad#1849: the frames swept round a
  // closed chain, the step past the narrow face wholly within the regions'
  // reach); a chain without such a corner (one edge) leaves that try out.
  struct Mode {
    bool walls;
    bool at_corners;
    bool sectors;
  };
  std::vector<Mode> modes;
  if (analytic) {
    modes.push_back({false, false, true});
    modes.push_back({true, false, true});
  }
  for (int mode = 0; mode < 4; ++mode) {
    modes.push_back({(mode & 1) != 0, mode >= 2, false});
  }
  bool corner = false;
  {
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edges_at;
    TopExp::MapShapesAndUniqueAncestors(input.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_at);
    for (int v = 1; v <= edges_at.Extent() && !corner; ++v) {
      int count = 0;
      for (const TopoDS_Shape& edge : edges_at(v)) {
        const int e = input.edge_index(edge);
        count += e >= 0 && owner[static_cast<std::size_t>(e)] >= 0 ? 1 : 0;
      }
      corner = count >= 2;
    }
  }
  // (The dressup's own faces: "<feature>:chamfer(…" or "…:fillet(…".)
  const std::string generated = feature + ":" + role + "(";
  ShapePtr unmerged;
  for (const Mode& mode : modes) {
    const bool walls = mode.walls;
    const bool at_corners = mode.at_corners;
    if (at_corners && !corner) {
      continue;
    }
    try {
      const ShapePtr copy = detail::working_copy(input);
      const std::optional<detail::Cutouts> cleared =
          detail::fill_overflow(*copy, owner, contact, depth_ratio, walls, at_corners, reach, mode.sectors);
      if (!cleared) {
        continue;
      }
      detail::Cutouts cutouts = *cleared;
      ShapePtr chamfered;
      try {
        chamfered = build(*cutouts.body, cutouts.owner);
      } catch (const std::exception& error) {
        // (Why the dressup past the narrow faces failed at a corner.)
        if (at_corners && why != nullptr) {
          *why = error.what();
        }
        throw;
      }
      if (!chamfered || !detail::put_walls_back(cutouts, *chamfered, generated, *copy)) {
        continue;
      }
      ShapePtr* const apart = pieces_apart ? &unmerged : nullptr;
      // (Closed where the input is: never a gap where a face beyond does
      // not reach; and with a face of the dressup left: where the faces
      // beyond, grown out, cover all of it, the band reaches past them too
      // and is not built, mitcad#1819.)
      const ShapePtr result = detail::cut_out_again(chamfered, cutouts, *copy, apart);
      if (result && (!closed_solids(copy->occt()) || closed_solids(result->occt())) &&
          holds_made_face(*result, generated)) {
        return with_input_names(result, *cutouts.body, *copy);
      }
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
    }
  }
  // (Else the first result valid with the pieces of a face beyond apart,
  // mitcad#1613.)
  return unmerged;
}

// Whether a dressup of convex edges only (`own` of `target`) came out
// larger than the body it dresses (`made`): it takes material away, so the
// geometry kernel's result folds out of the body (a bevel past a narrow
// face on a body with the space past it filled, GitHub #353's owner case).
// The tries of the narrow-face paths take that as a failure, so that the
// next one runs.
bool folded_out(const Shape& target, const std::vector<int>& own, const Shape& made) {
  try {
    OCC_CATCH_SIGNALS
    const std::optional<detail::DressupSide> side = detail::dressup_side(target, own);
    if (!side || *side != detail::DressupSide::Within) {
      return false;
    }
    const double before = std::abs(detail::volume_of(target.occt()));
    return std::abs(detail::volume_of(made.occt())) > before + 1.0e-6 * std::max(1.0, before);
  } catch (const Standard_Failure&) {
    return false;
  }
}

// The middle of an edge.
gp_Pnt middle_of(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
}

// A dressup of the selected edges (`owner`) whose chain ends against a
// planar face that does not reach across the dressup's section there
// (GitHub #353, mitcad#1819: a shelf standing out from a bore's wall, flush
// with the top, the rim's chamfer ending against the shelf's end wall,
// which lies on the bore's side of the wall while the bevel cuts into the
// wall, so that the geometry kernel finds no face to end the bevel on, at
// any size). Built as a modeller would: the bevel ends on that face's plane,
// the face grown out along its plane to meet it. The material past the
// plane near each such chain end (within a box of `extent` times the
// dressup's reach round the end) is taken off, so that a face on the plane
// covers the whole section; `build` dresses the edges on that body (the
// edges found again by their middles); the material is put back and the
// faces of one surface joined. Where the edge stands at right angles to the
// plane and the end face goes on tangentially into a face beyond near the
// end (a concave rounding the end face runs into, GitHub #608), the
// material is put back less the region the dressup took away continued past
// the plane (removal_continued, mitcad#2330): its walls run on through it. A chain end counts where one face at it
// beside the edge's own two is a plane, and the material past it touches
// no selected edge but at that end. Null where there is no such end or the
// result is not one valid closed solid holding a face of the dressup
// (`generated`, its faces' names' start); never throws but for cancelling.
template <class Build>
ShapePtr ended_on_end_planes_within(const Shape& input, const std::vector<int>& owner, double contact,
                                    const std::string& generated, Build&& build, double extent) {
  try {
    OCC_CATCH_SIGNALS
    const TopoDS_Shape& solid = input.occt();
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(solid, TopAbs_VERTEX));
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edges_at,
        faces_at;
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_VERTEX, TopAbs_EDGE, edges_at);
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_VERTEX, TopAbs_FACE, faces_at);
    const detail::EdgeFaces edge_faces(input);
    BRep_Builder builder;
    TopoDS_Compound picked;
    builder.MakeCompound(picked);
    for (int e = 0; e < input.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] >= 0) {
        builder.Add(picked, input.edge(e));
      }
    }
    // The material past the end planes near the chains' ends.
    std::vector<TopoDS_Shape> pasts;
    // The ends where the dressup goes on past the plane through that
    // material (mitcad#2330): the plane, the way past it, the box's half
    // size.
    struct Continued {
      gp_Pln plane;
      gp_Dir out;
      double reach;
    };
    std::vector<Continued> ends;
    for (int e = 0; e < input.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      const TopoDS_Edge& edge = TopoDS::Edge(input.edge(e));
      const std::vector<int> own = edge_faces.at(e);
      const BRepAdaptor_Curve curve(edge);
      const double first = curve.FirstParameter();
      const double last = curve.LastParameter();
      for (const TopoDS_Vertex& vertex : {TopExp::FirstVertex(edge), TopExp::LastVertex(edge)}) {
        if (vertex.IsNull() || !edges_at.Contains(vertex) || !faces_at.Contains(vertex)) {
          continue;
        }
        int chained = 0;
        for (const TopoDS_Shape& other : edges_at.FindFromKey(vertex)) {
          const int i = input.edge_index(other);
          chained += i >= 0 && i < static_cast<int>(owner.size()) && owner[static_cast<std::size_t>(i)] >= 0 ? 1 : 0;
        }
        if (chained != 1) {
          continue;
        }
        std::vector<int> at_end;
        for (const TopoDS_Shape& face : faces_at.FindFromKey(vertex)) {
          const int f = input.face_index(face);
          if (f >= 0 && std::find(own.begin(), own.end(), f) == own.end() &&
              std::find(at_end.begin(), at_end.end(), f) == at_end.end()) {
            at_end.push_back(f);
          }
        }
        if (at_end.size() != 1) {
          continue;
        }
        const TopoDS_Face& end_face = input.face(at_end.front());
        const BRepAdaptor_Surface surface(end_face, false);
        if (surface.GetType() != GeomAbs_Plane) {
          continue;
        }
        const gp_Pln plane = surface.Plane();
        const gp_Pnt at = BRep_Tool::Pnt(vertex);
        const gp_Dir normal = plane.Axis().Direction();
        // (A point of the edge just off the end: the side the edge leaves
        // the plane to.)
        const bool at_first = vertex.IsSame(TopExp::FirstVertex(edge));
        const gp_Pnt off = curve.Value(at_first ? first + 1.0e-3 * (last - first) : last - 1.0e-3 * (last - first));
        const double side = gp_Vec(at, off).Dot(gp_Vec(normal));
        if (std::abs(side) <= 1.0e-9) {
          continue;
        }
        // (The far side: away from the edge.)
        const gp_Pnt beyond = at.Translated(gp_Vec(normal) * (side > 0.0 ? -1.0 : 1.0));
        const TopoDS_Face on_plane = BRepBuilderAPI_MakeFace(plane).Face();
        const TopoDS_Shape half = BRepPrimAPI_MakeHalfSpace(on_plane, beyond).Solid();
        const double s = extent * contact + 10.0 * tolerance;
        const TopoDS_Shape box =
            BRepPrimAPI_MakeBox(gp_Pnt(at.X() - s, at.Y() - s, at.Z() - s), gp_Pnt(at.X() + s, at.Y() + s, at.Z() + s))
                .Shape();
        BRepAlgoAPI_Common within(box, half);
        if (!within.IsDone()) {
          continue;
        }
        // (It touches the picked edges at the end only.)
        BRepExtrema_DistShapeShape near_picked(within.Shape(), picked);
        bool clear = near_picked.IsDone();
        for (int i = 1; clear && i <= near_picked.NbSolution(); ++i) {
          clear = near_picked.Value() > tolerance || near_picked.PointOnShape2(i).Distance(at) <= 10.0 * tolerance;
        }
        if (clear) {
          pasts.push_back(within.Shape());
          // Whether the dressup goes on past the plane through the
          // material put back (mitcad#2330): the edge a line at right
          // angles to the plane, and the end face going on tangentially
          // into a face beyond near the end (an earlier concave rounding
          // that the end face runs into, GitHub #608: the material under
          // the end face there is that rounding's, and the dressup's walls
          // run on down to it, as where it was built first). Not where
          // the material past the plane stands across the end face
          // (GitHub #353's shelf: the dressup ends against it).
          bool continued = curve.GetType() == GeomAbs_Line &&
                           std::abs(std::abs(curve.Line().Direction().Dot(normal)) - 1.0) <= 1.0e-9;
          if (continued) {
            continued = false;
            for (TopExp_Explorer ex(end_face, TopAbs_EDGE); ex.More() && !continued; ex.Next()) {
              const TopoDS_Edge& side_edge = TopoDS::Edge(ex.Current());
              const int i = input.edge_index(side_edge);
              if (i < 0 || BRep_Tool::Degenerated(side_edge)) {
                continue;
              }
              for (const int f : edge_faces.at(i)) {
                if (f != at_end.front() && std::find(own.begin(), own.end(), f) == own.end() &&
                    detail::is_smooth(side_edge, end_face, input.face(f)) &&
                    BRepExtrema_DistShapeShape(side_edge, vertex).Value() <= s) {
                  continued = true;
                }
              }
            }
          }
          if (continued) {
            ends.push_back({plane, gp_Dir(gp_Vec(at, beyond)), s});
          }
        }
      }
    }
    if (pasts.empty()) {
      return nullptr;
    }
    // (One tool: the regions fused where they overlap.)
    TopoDS_Shape tool = pasts.front();
    for (std::size_t i = 1; i < pasts.size(); ++i) {
      BRepAlgoAPI_Fuse fuse(tool, pasts[i]);
      if (!fuse.IsDone()) {
        return nullptr;
      }
      tool = fuse.Shape();
    }
    const Shape cutter(tool);
    const BooleanResult cleared = boolean(BooleanOp::Cut, {&input}, cutter);
    // (The material past the planes, put back after the dressup.)
    const BooleanResult taken = boolean(BooleanOp::Intersect, {&input}, cutter);
    if (cleared.pieces.size() != 1 || taken.pieces.empty()) {
      return nullptr;
    }
    const Shape& body = *cleared.pieces.front().shape;
    // The picked edges on that body, found by their middles.
    std::vector<int> moved(static_cast<std::size_t>(body.edge_count()), -1);
    for (int e = 0; e < input.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      const gp_Pnt m = middle_of(TopoDS::Edge(input.edge(e)));
      int found = -1;
      for (int i = 0; i < body.edge_count() && found < 0; ++i) {
        if (!BRep_Tool::Degenerated(TopoDS::Edge(body.edge(i))) &&
            middle_of(TopoDS::Edge(body.edge(i))).Distance(m) <= 10.0 * tolerance) {
          found = i;
        }
      }
      if (found < 0) {
        return nullptr;
      }
      moved[static_cast<std::size_t>(found)] = owner[static_cast<std::size_t>(e)];
    }
    ShapePtr dressed = build(body, moved);
    if (!dressed) {
      return nullptr;
    }
    // The material put back, less the region the dressup took away continued
    // past the plane where it goes on through it (mitcad#2330: Fillet1 of
    // GitHub #608's design after Fillet2 left Fillet2's rounding standing
    // out past its own as a lip, a step at its foot, which the other order
    // did not have).
    std::vector<ShapePtr> back_pieces;
    for (const BooleanPiece& piece : taken.pieces) {
      back_pieces.push_back(piece.shape);
    }
    for (const Continued& end : ends) {
      const std::optional<ShapePtr> on_past =
          detail::removal_continued(body, *dressed, end.plane, end.out, 2.0 * end.reach);
      if (!on_past) {
        return nullptr;
      }
      if (!*on_past) {
        continue;
      }
      std::vector<ShapePtr> kept;
      for (const ShapePtr& piece : back_pieces) {
        const BooleanResult less = boolean(BooleanOp::Cut, {piece.get()}, **on_past);
        if (less.touched.empty() || !less.touched.front()) {
          kept.push_back(piece);
          continue;
        }
        for (const BooleanPiece& part : less.pieces) {
          kept.push_back(part.shape);
        }
      }
      back_pieces = std::move(kept);
    }
    double put_back = 0.0;
    ShapePtr made = dressed;
    for (const ShapePtr& piece : back_pieces) {
      put_back += std::abs(detail::volume_of(piece->occt()));
      const BooleanResult back = boolean(BooleanOp::Join, {made.get()}, *piece);
      if (back.pieces.size() != 1) {
        return nullptr;
      }
      made = back.pieces.front().shape;
    }
    // (Its made faces named after the input's edges, not the body's.)
    ShapePtr result = with_input_names(detail::join_same_surfaces(made), body, input);
    if (!result) {
      return nullptr;
    }
    int solids = 0;
    for (TopExp_Explorer ex(result->occt(), TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    if (solids != 1 || !BRepCheck_Analyzer(result->occt()).IsValid() || !closed_solids(result->occt()) ||
        !holds_made_face(*result, generated)) {
      return nullptr;
    }
    // (The result is the dressed body and the material put back together,
    // by volume within 1e-6 of the input's; without an end going on past
    // its plane that material is what was taken off.)
    const double whole = std::abs(detail::volume_of(solid));
    const double expected = std::abs(detail::volume_of(dressed->occt())) +
                            (ends.empty() ? whole - std::abs(detail::volume_of(body.occt())) : put_back);
    if (std::abs(std::abs(detail::volume_of(result->occt())) - expected) > 1.0e-6 * std::max(1.0, whole)) {
      return nullptr;
    }
    result->copy_notes(*dressed);
    return result;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    return nullptr;
  }
}
// ended_on_end_planes_within with the material past the planes taken off
// within 1.5, 2.2 and then 3 times the dressup's reach of the chain's end:
// any box that holds the dressup's section on the plane gives the same
// shape; where the faces it cuts (a thread's flanks past a bore's rim) do
// not join again, another may (GitHub #353's owner case: a bevel of 2 and
// 2.2 mm built with the smaller boxes, one of 2.7 mm with the largest).
template <class Build>
ShapePtr ended_on_end_planes(const Shape& input, const std::vector<int>& owner, double contact,
                             const std::string& generated, Build&& build) {
  for (const double extent : {1.5, 2.2, 3.0}) {
    if (ShapePtr ended = ended_on_end_planes_within(input, owner, contact, generated, build, extent)) {
      return ended;
    }
  }
  return nullptr;
}

// A dressup (`role` "fillet" or "chamfer", its surface `what`: "rounding"
// or "bevel") of straight edges between two planes that end on a flat face
// of a rounded body smaller than the dressup's section, the face ending in
// a rounding tangent to it there (end_face_dressup.hpp, mitcad#2229, GitHub
// mitcad/Mitcad#566), whose contact lies past both faces beside such an
// edge: no such dressup exists there (it touches neither face), refused in
// words before the geometry kernel with the largest size that fits
// (std::invalid_argument: no smaller retry; `size` the size asked of an
// edge, `size_word` its word). Elsewhere nothing: the kernel builds the
// dressup, its end past such a face included.
void refuse_wider_than_the_body(const char* role, const char* what, const Shape& input,
                                const std::vector<int>& owner, const detail::SectionReach& reach, bool round,
                                const std::function<double(int)>& size, const char* size_word) {
  std::optional<detail::WiderThanBody> wider;
  try {
    OCC_CATCH_SIGNALS
    const std::optional<detail::DressupSide> side = detail::dressup_side(input, owner);
    if (!side || *side != detail::DressupSide::Within) {
      return;
    }
    wider = detail::wider_than_the_body(input, owner, reach, round);
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    return;
  }
  if (!wider) {
    return;
  }
  const int e = wider->edge;
  // (In thousandths of a millimetre, down.)
  const double fits = std::floor(wider->fits * size(e) * 1000.0 + 1.0e-6) / 1000.0;
  char text[32];
  std::snprintf(text, sizeof text, "%.3f", fits);
  const std::string largest = std::string(": the largest ") + size_word + " that fits is " + text + " mm";
  throw std::invalid_argument(
      std::string(role) + " failed; " +
      placed_error(std::string("the ") + what +
                       " is wider than the body at its edge and touches neither face beside it" + largest,
                   std::string("the ") + what + " of " + describe_edge(input, e) +
                       " is wider than the body there and touches neither face beside it" + largest,
                   {edge_place(input.edge_name(e))}));
}

// How far a dressup reaching `contact` along its faces counts as reaching
// where the geometry kernel fails at its exact size: a face whose far edge
// lies exactly as far from the edge, or up to the 0.1 % that the smaller
// retry takes off further, is taken away as a narrower face is (GitHub
// #353, mitcad#1819): the dressup at its full size on the face continued
// past that edge, the face beyond grown out to meet it, rather than the
// retry's 0.1 % smaller dressup with a sliver of the face left. A strip of
// the face left beside it at that width is taken away with it
// (without_strips, as after the retry).
double exactly_wide(double contact) {
  return contact * (2.0 - kShrink);
}

// The kernel's error `kernel` of a dressup wider than a face beside its
// edge, with `where` (overflow_failure's words): where the kernel's text
// says what it is in words of its own (a placed error: what a person reads
// ends before its places), the person reads `where` after the kernel
// error's opening clause ("fillet failed; ", "chamfer: ", which the import
// reads) and the kernel's message goes to the detail (mitcad#1819: "no
// ball of the radius touches both faces" where the rounding is wider than
// the face); else the kernel's text, then `where`.
std::string worded_failure(const std::string& kernel, const std::string& where) {
  if (kernel.find(kPlacedTail) == std::string::npos) {
    return kernel + "; " + where;
  }
  std::string start;
  for (const char* opening : {"fillet failed; ", "fillet: ", "chamfer failed; ", "chamfer: "}) {
    if (kernel.compare(0, std::strlen(opening), opening) == 0) {
      start = opening;
      break;
    }
  }
  return with_detail(start + where, " (the geometry kernel: " + error_message(kernel).substr(start.size()) + ")");
}

// A rounding of concave selected edges (`owner`) wider than a narrow face
// beside them whose far edges go on tangentially into convex faces beyond
// (a step's riser running into a round face, mitcad#2208): `build` rounds,
// at the full size, the body with each such narrow face replaced by the
// faces beyond continued along their own surfaces up to the edge's other
// face (faces_beyond_grown), so that the ball takes the narrow face away and
// rolls on over the face beyond, as where the geometry kernel's own rounding
// runs onto it. A face beyond that bends away from the material (a hole's
// round wall after a short straight stretch tangent to it, the corner
// beside it rounded, mitcad#2419, GitHub #667) is continued the same way,
// the space between it and the narrow face filled, which the rounding
// covers. The result is taken where it is one closed solid holding a
// face of the rounding and the whole input lies within it (a rounding of
// concave edges only adds material: the material taken away with the
// narrow face is filled again, the narrow face consumed all along), with
// what it adds past the walls of the face the ball touches cut away
// (added_past_walls, mitcad#2330); its faces are named after the picked
// edges. Null otherwise (never throws but
// for cancelling and invalid arguments); `applies` tells whether there was
// such a narrow face with material taken away past it (not only space
// filled). Other narrow faces beside the edges, still narrow on
// that body, are continued past their far edges there (over_narrow_faces).
template <class Build>
ShapePtr over_tangent_far_edges(const std::string& feature, const Shape& input, const std::vector<int>& owner,
                                double contact, Build&& build, bool& applies) {
  applies = false;
  try {
    OCC_CATCH_SIGNALS
    const ShapePtr copy = detail::working_copy(input);
    const std::optional<detail::FacesBeyondGrown> grown = detail::faces_beyond_grown(*copy, owner, contact);
    if (!grown) {
      return nullptr;
    }
    // (Only filled past faces beyond bending away from the material,
    // mitcad#2419: where that fails, the other ways past the narrow faces
    // are tried as before.)
    applies = !grown->filled;
    ShapePtr made;
    try {
      made = build(*grown->body, grown->owner);
    } catch (const std::invalid_argument&) {
      throw;
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
      made = over_narrow_faces(feature, "fillet", *grown->body, grown->owner, contact, 1.0, build);
    }
    // What the rounding adds past the walls of the face the ball touches
    // beside the narrow one (a lip standing out past an earlier rounding of
    // the corner where the rounding's contact runs off that face onto it,
    // ended on the end face's plane: a step at the earlier rounding's foot,
    // GitHub #608) is cut away, the walls continued down past the face
    // (added_past_walls, mitcad#2330); where such a wall does not stand at
    // right angles to the face (added_past_walls throws), this gives null
    // and the rounding fails in words (past_narrow_faces).
    if (made && grown->replaced.size() == 1) {
      const std::optional<ShapePtr> lip = detail::added_past_walls(*copy, grown->replaced.front().first,
                                                                   grown->replaced.front().second, *made);
      if (lip && *lip) {
        const BooleanResult clipped = boolean(BooleanOp::Cut, {made.get()}, **lip);
        if (!clipped.touched.empty() && clipped.touched.front() && clipped.pieces.size() == 1 &&
            BRepCheck_Analyzer(clipped.pieces.front().shape->occt()).IsValid()) {
          const ShapePtr kept = clipped.pieces.front().shape;
          kept->copy_notes(*made);
          made = kept;
        } else if (std::abs(detail::volume_of((*lip)->occt())) >
                   1.0e-9 * std::max(1.0, std::abs(detail::volume_of(input.occt())))) {
          // (A lip of micrometres the cut cannot take away stays, as
          // before; a larger one is never left.)
          return nullptr;
        }
      }
    }
    if (!made || !closed_solids(made->occt()) || !holds_made_face(*made, feature + ":fillet(")) {
      return nullptr;
    }
    int solids = 0;
    for (TopExp_Explorer ex(made->occt(), TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    if (solids != 1) {
      return nullptr;
    }
    // (The input less the result: nothing.)
    BRepAlgoAPI_Cut left;
    NCollection_List<TopoDS_Shape> arguments;
    NCollection_List<TopoDS_Shape> tools;
    arguments.Append(input.occt());
    tools.Append(made->occt());
    left.SetArguments(arguments);
    left.SetTools(tools);
    left.SetNonDestructive(true);
    left.SetRunParallel(true);
    left.Build();
    const double whole = std::abs(detail::volume_of(input.occt()));
    if (!left.IsDone() || left.HasErrors() ||
        std::abs(detail::volume_of(left.Shape())) > 1.0e-6 * std::max(1.0, whole)) {
      return nullptr;
    }
    return with_renames(made, grown->renames);
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    return nullptr;
  }
}

// A rounding of convex selected edges (`owner`) wider than a narrow face
// beside them whose far edges across it go on concavely into one face
// beyond rising from it (a ledge before a spike's foot, mitcad#2352,
// GitHub #620): `build` rounds, at the full size, the body with the narrow
// face taken away there and the face beyond continued down along its own
// surface to the edge's other face (faces_beyond_grown's `rising`), so
// that the rounding is tangent to the other face and the face beyond, as
// the ball rolling past the ledge makes it. The result is taken where it
// is one closed solid holding a face of the rounding and lying within the
// input (a rounding of convex edges only takes material away); its faces
// are named after the picked edges. Null otherwise (never throws but for
// cancelling and invalid arguments).
template <class Build>
ShapePtr over_rising_far_edges(const std::string& feature, const Shape& input, const std::vector<int>& owner,
                               double contact, Build&& build) {
  try {
    OCC_CATCH_SIGNALS
    const ShapePtr copy = detail::working_copy(input);
    const std::optional<detail::FacesBeyondGrown> grown = detail::faces_beyond_grown(*copy, owner, contact, true);
    if (!grown) {
      return nullptr;
    }
    ShapePtr made;
    try {
      made = build(*grown->body, grown->owner);
    } catch (const std::invalid_argument&) {
      throw;
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
      return nullptr;
    }
    if (!made || !closed_solids(made->occt()) || !holds_made_face(*made, feature + ":fillet(")) {
      return nullptr;
    }
    int solids = 0;
    for (TopExp_Explorer ex(made->occt(), TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    if (solids != 1) {
      return nullptr;
    }
    // (The result less the input: nothing.)
    BRepAlgoAPI_Cut past;
    NCollection_List<TopoDS_Shape> arguments;
    NCollection_List<TopoDS_Shape> tools;
    arguments.Append(made->occt());
    tools.Append(input.occt());
    past.SetArguments(arguments);
    past.SetTools(tools);
    past.SetNonDestructive(true);
    past.SetRunParallel(true);
    past.Build();
    const double whole = std::abs(detail::volume_of(input.occt()));
    if (!past.IsDone() || past.HasErrors() ||
        std::abs(detail::volume_of(past.Shape())) > 1.0e-6 * std::max(1.0, whole) ||
        !(std::abs(detail::volume_of(made->occt())) < whole)) {
      return nullptr;
    }
    return with_renames(made, grown->renames);
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    return nullptr;
  }
}

// A rounding of the selected edges (`owner`) that the geometry kernel
// failed to build at its exact size, or left open (OpenResult); called
// while that error is handled. Where a face beside the edges is narrower
// than the rounding reaches along it (`contact`, overflow_failure), it is
// built as a chamfer is (over_narrow_faces, mitcad#1819): `build` rounds
// the picked edges at their full size on the body with the narrow faces
// continued, and the faces beyond grow out to meet the rounding; a strip
// of a narrow face narrower than 0.1 % of the rounding left beside it is
// taken away with it (as after the 0.1 % smaller retry). Where it cannot be
// built so, the error says in words which face the rounding would take
// away and that the faces beyond cannot meet it, the kernel's own error
// before it; where no face is narrower, the kernel's error stands (the
// 0.1 % smaller retry follows, mitcad#1106). `ends` rounds the ends of
// grooves the edges are (groove_end.hpp, mitcad#2154), null where they are
// not.
template <class Build, class Rings, class Ends>
ShapePtr past_narrow_faces(const std::string& feature, const Shape& input, const std::vector<int>& owner,
                           double given_contact, Build&& build, Rings&& rings, Ends&& ends) {
  std::string where;
  double contact = given_contact;
  try {
    where = detail::overflow_failure(*detail::working_copy(input), owner, contact, "fillet");
    // A face exactly as wide as the rounding reaches along it (within the
    // 0.1 % the smaller retry would take off) is taken away as a narrower
    // one is (exactly_wide, GitHub #353: a rounding as large as the side
    // face below its edge is high, ending on the face's far edge).
    if (where.empty()) {
      contact = exactly_wide(given_contact);
      where = detail::overflow_failure(*detail::working_copy(input), owner, contact, "fillet");
    }
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    where.clear();
  }
  if (where.empty()) {
    // No face beside the edges narrower than the rounding: a chain ending
    // against a plane that does not reach across the rounding there is
    // ended on that plane, the rounding on the body without the material
    // past it (ended_on_end_planes, GitHub #353), as below where a narrow
    // face is continued (an overhang's bottom edge rounded from the end
    // face of the block below it, which reaches up the overhang's front
    // face only to an earlier rounding's top edge and not at all behind
    // it: the kernel leaves the end open at every size, mitcad#2276).
    // Else the kernel's error stands.
    if (ShapePtr ended = ended_on_end_planes(input, owner, given_contact, feature + ":fillet(",
                                             [&](const Shape& on, const std::vector<int>& edges) -> ShapePtr {
                                               ShapePtr made = build(on, edges);
                                               if (made && folded_out(on, edges, *made)) {
                                                 return nullptr;
                                               }
                                               return made;
                                             })) {
      if (ShapePtr whole = detail::without_strips(*ended, input, given_contact)) {
        return whole;
      }
      return ended;
    }
    throw;
  }
  // A rounding of whole circles that runs over a neighbouring face is built
  // as rings first, as before this path (`rings`, as or_rings builds them:
  // a hole's rim running onto a front edge's rounding, mitcad#1327).
  ShapePtr ringed;
  try {
    ringed = rings(*detail::working_copy(input));
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    ringed = nullptr;
  }
  if (ringed) {
    return ringed;
  }
  // The end of a shallow groove (a tube ending in a flat end cap) rounded
  // wider than the end cap is high: on the body with the end cap continued
  // to the whole disc and the floor to the whole tube, the rounding closed
  // round the circle, what it adds cut by the faces beyond (mitcad#2154).
  // The rounding's ends on the faces beyond, as over_narrow_faces builds
  // them, fail there (the kernel's corner at each end of the edge meets
  // the same edge twice).
  ShapePtr grooved;
  try {
    grooved = ends(*detail::working_copy(input));
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    grooved = nullptr;
  }
  if (grooved) {
    return grooved;
  }
  // (On the body with the narrow faces continued, a chain ending against a
  // plane that does not reach across the rounding is ended on it,
  // ended_on_end_planes.)
  const auto build_or_ended = [&](const Shape& on, const std::vector<int>& edges) -> ShapePtr {
    try {
      ShapePtr made = build(on, edges);
      if (made && folded_out(on, edges, *made)) {
        throw std::runtime_error("the fillet folds out of the body");
      }
      return made;
    } catch (const std::invalid_argument&) {
      throw;
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
      if (ShapePtr ended = ended_on_end_planes(on, edges, contact, feature + ":fillet(", build)) {
        return ended;
      }
      throw;
    }
  };
  // A narrow face whose far edges go on tangentially into convex faces
  // beyond (a step's riser running into a round face), or into faces
  // bending away from the material (a hole's round wall, mitcad#2419): the
  // ball takes it away and rolls on over the faces beyond
  // (over_tangent_far_edges, mitcad#2208). Continued along its own surface (over_narrow_faces), the
  // ball would keep to that surface, and the rounding, the space filled past
  // the far edge cut out again, would hang beside the face beyond as a
  // sheet: where the rounding over such a face cannot be built, it fails
  // in words, never that sheet.
  bool tangent_far_edges = false;
  if (ShapePtr rolled = over_tangent_far_edges(feature, input, owner, contact, build_or_ended, tangent_far_edges)) {
    return rolled;
  }
  // A narrow face beside a convex edge whose far edge across it goes on
  // concavely into a face rising beyond (a ledge before a spike's foot):
  // the face beyond continued down to the edge's other face, the rounding
  // tangent to both (over_rising_far_edges, mitcad#2352). Continued along
  // its own surface, the ledge's plane would hold the ball, and the face
  // beyond grown down to meet the rounding would bury it.
  if (!tangent_far_edges) {
    if (ShapePtr risen = over_rising_far_edges(feature, input, owner, contact, build_or_ended)) {
      return risen;
    }
  }
  ShapePtr over;
  bool freeform = false;
  if (!tangent_far_edges) {
    try {
      over = over_narrow_faces(feature, "fillet", input, owner, contact, 1.0, build_or_ended, nullptr, false,
                               nullptr, &freeform);
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
      over = nullptr;
    }
  }
  // A chain ending against a plane that does not reach across the rounding
  // there, with no narrow face continued: ended on that plane, the rounding
  // on the body without the material past it built as above
  // (ended_on_end_planes, GitHub #353).
  // (Not over a narrow face with tangent far edges: its build above ended
  // chains on such planes already.)
  if (!over && !tangent_far_edges) {
    over = ended_on_end_planes(input, owner, contact, feature + ":fillet(",
                               [&](const Shape& on, const std::vector<int>& edges) -> ShapePtr {
                                 try {
                                   ShapePtr made = build(on, edges);
                                   if (made && !folded_out(on, edges, *made)) {
                                     return made;
                                   }
                                 } catch (const std::invalid_argument&) {
                                   throw;
                                 } catch (const Cancelled&) {
                                   throw;
                                 } catch (...) {
                                 }
                                 return over_narrow_faces(feature, "fillet", on, edges, contact, 1.0, build);
                               });
  }
  if (over) {
    if (ShapePtr whole = detail::without_strips(*over, input, contact)) {
      return whole;
    }
    return over;
  }
  // (A free-form B-spline face beside the edges, which cannot be continued
  // past its far edge: the words say so, mitcad#1849.)
  if (freeform) {
    try {
      const std::string worded =
          detail::overflow_failure(*detail::working_copy(input), owner, contact, "fillet", nullptr,
                                   detail::kFreeformCause);
      if (!worded.empty()) {
        where = worded;
      }
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
    }
  }
  // (Never that its result is not a closed solid where the rounding takes
  // a narrow face away.)
  std::string text = where;
  try {
    throw;
  } catch (const OpenResult&) {
  } catch (const std::exception& error) {
    text = worded_failure(error.what(), where);
  }
  throw std::runtime_error(text);
}

// Whether the sets follow their own tangent chains (follow_own_chain), by
// set.
template <class Set>
std::vector<char> chain_follows(const std::vector<Set>& sets) {
  std::vector<char> follows;
  for (const Set& set : sets) {
    follows.push_back(follows_own_chain(set) ? 1 : 0);
  }
  return follows;
}

// The input's face a name of the result is or is a piece of: the name with
// its pieces' suffixes ("#k") taken off one by one until it is one of the
// input's names (`names`); empty where none is (a face the dressup made,
// "<feature>:chamfer(E{…#0})" among them).
std::string input_face_of(const std::string& name, const std::set<std::string>& names) {
  std::string base = name;
  for (;;) {
    if (names.count(base) != 0) {
      return base;
    }
    const std::size_t hash = base.rfind('#');
    if (hash == std::string::npos || hash + 1 == base.size() ||
        !std::all_of(base.begin() + static_cast<std::ptrdiff_t>(hash) + 1, base.end(),
                     [](char c) { return c >= '0' && c <= '9'; })) {
      return {};
    }
    base.erase(hash);
  }
}

// The plane of a planar face of `solid` with its normal out of it: the
// surface's normal, reversed where the face is reversed in the solid (as
// the solid's own shells hold it: the orientation of a face in a result's
// face list is not to be trusted for it). None where the face is not
// planar or not in the solid.
std::optional<gp_Pln> outward_plane(const TopoDS_Face& face, const TopoDS_Shape& solid) {
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  for (TopExp_Explorer ex(solid, TopAbs_FACE); ex.More(); ex.Next()) {
    if (!ex.Current().IsSame(face)) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    gp_Dir normal = plane.Position().XDirection().Crossed(plane.Position().YDirection());
    if (ex.Current().Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    return gp_Pln(plane.Location(), normal);
  }
  return std::nullopt;
}

// The half-space on one side of a plane: out of the solid its normal
// points out of (`out`), or into it.
TopoDS_Shape half_space(const gp_Pln& plane, bool out) {
  const gp_Pnt reference = plane.Location().Translated(gp_Vec(plane.Axis().Direction()) * (out ? 1.0 : -1.0));
  return BRepPrimAPI_MakeHalfSpace(BRepBuilderAPI_MakeFace(plane).Face(), reference).Solid();
}

// `shape` cut down to the half-spaces (a plane, and true for the side out
// of its solid); null where a boolean fails.
std::optional<TopoDS_Shape> clipped(TopoDS_Shape shape, const std::vector<std::pair<gp_Pln, bool>>& sides) {
  for (const auto& [plane, out] : sides) {
    BRepAlgoAPI_Common common(shape, half_space(plane, out));
    common.SetRunParallel(false);
    detail::build(common);
    if (!common.IsDone() || common.HasErrors()) {
      return std::nullopt;
    }
    shape = common.Shape();
  }
  return shape;
}

// One boolean of OCCT (fuse, cut or common) of two shapes, its history
// merged into `history`; none where it fails.
std::optional<TopoDS_Shape> boolean(BRepAlgoAPI_BooleanOperation&& operation, const TopoDS_Shape& argument,
                                    const TopoDS_Shape& tool, BRepTools_History* history,
                                    BOPAlgo_GlueEnum glue = BOPAlgo_GlueOff) {
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(argument);
  tools.Append(tool);
  operation.SetArguments(arguments);
  operation.SetTools(tools);
  // (On all cores, as the model's booleans; the shapes left as they are.)
  operation.SetRunParallel(true);
  operation.SetNonDestructive(true);
  operation.SetGlue(glue);
  operation.SetToFillHistory(history != nullptr);
  detail::build(operation);
  if (!operation.IsDone() || operation.HasErrors()) {
    return std::nullopt;
  }
  if (history != nullptr) {
    history->Merge(operation.History());
  }
  return operation.Shape();
}

// The pieces of a cut face a chamfer of concave edges leaves where its
// chain turns round a corner rounded more tightly than it reaches
// (mitcad#1821), closed by mitring the bevels of the corner's two straight
// neighbours. There the kernel sweeps the bevel round the corner's arc on a
// cone whose contact on the cut face would run on a negative radius: the
// cone folds over through the arc's axis, leaving a groove between the two
// bevels down to the cut face (a flat piece of it at the bottom, 0.05 mm2
// on a nut's end chamfered 2.55 round an octagonal rim rounded 1, the cut
// face dented under it) and a sliver of the cut face past the point where
// the bevels' contact lines cross (0.003 mm2 there, 0.03 short of a bore).
// The folded face defeats the booleans and the defeaturing near it, so the
// whole corner is rebuilt: a zone round it (between two planes parallel to
// the walls' bisector, past the corner's dressup faces and the pieces on
// either side by a tenth of the reach, within their bounding box grown by
// as much) is cut out of the result and refilled with the input's
// material there and the two bevels as planes meeting at their mitre
// (under either bevel's plane, over the cut face, in front of both walls),
// less what lies under the cut face and over both bevels' planes past the
// crossing (the bevels continued down to the faces beyond, so that the
// sliver goes). The arc's wall shows down to the mitre (the cone stood a
// little higher at the wall: by r (1 - cos(a/2)) at the arc's middle, an
// arc of radius r turning by a). Pieces of a plane only, each beside two
// planar bevels named after edges of the input between the cut face and a
// planar wall. All corners in one set of booleans (their zones apart). A
// face the refill gives on the surface of a face of the result that it
// continues (a bevel's plane, a face beyond grown out over the cut face)
// takes that face's name; the input's faces in the zone keep theirs. The
// new body where it is one valid closed solid, each of its faces named,
// none of the pieces left, and its volume within the reach cubed per
// corner of `result`'s; null otherwise.
ShapePtr remnants_mitred(const Shape& result, const Shape& input, const std::vector<int>& pieces,
                         const std::vector<char>& made, const std::string& feature, double contact) {
  const detail::EdgeFaces edge_faces(result);
  const TopoDS_Shape& solid = result.occt();
  // The pieces by their pair of bevels, with the dressup's other faces
  // beside them (the corner's folded face).
  std::map<std::pair<int, int>, std::vector<int>> pairs;
  std::map<std::pair<int, int>, std::set<int>> corner_faces;
  for (const int f : pieces) {
    if (BRepAdaptor_Surface(result.face(f), false).GetType() != GeomAbs_Plane) {
      return nullptr;
    }
    std::vector<int> bevels;
    std::set<int> others;
    for (TopExp_Explorer ex(result.face(f), TopAbs_EDGE); ex.More(); ex.Next()) {
      const int e = result.edge_index(TopoDS::Edge(ex.Current()));
      if (e < 0) {
        continue;
      }
      for (const int g : edge_faces.at(e)) {
        if (g == f || made[static_cast<std::size_t>(g)] == 0) {
          continue;
        }
        if (BRepAdaptor_Surface(result.face(g), false).GetType() == GeomAbs_Plane) {
          if (std::find(bevels.begin(), bevels.end(), g) == bevels.end()) {
            bevels.push_back(g);
          }
        } else {
          others.insert(g);
        }
      }
    }
    if (bevels.size() != 2) {
      return nullptr;
    }
    const std::pair<int, int> key{std::min(bevels[0], bevels[1]), std::max(bevels[0], bevels[1])};
    pairs[key].push_back(f);
    corner_faces[key].insert(others.begin(), others.end());
  }
  BRepTools_History history;
  BRep_Builder builder;
  TopoDS_Compound zones;
  TopoDS_Compound fills;
  TopoDS_Compound slivers;
  builder.MakeCompound(zones);
  builder.MakeCompound(fills);
  builder.MakeCompound(slivers);
  bool any_sliver = false;
  std::vector<Bnd_Box> zone_boxes;
  for (const auto& [bevel_pair, place] : pairs) {
    // The bevels' planes; the walls' and the cut face's from the bevels'
    // edges in the input.
    std::optional<gp_Pln> cut;
    int cut_index = -1;
    std::vector<gp_Pln> bevel_planes;
    std::vector<gp_Pln> walls;
    const std::string piece = result.face_names(place.front()).front();
    for (const int bevel : {bevel_pair.first, bevel_pair.second}) {
      const std::optional<gp_Pln> plane = outward_plane(result.face(bevel), solid);
      const std::string edge = detail::own_edge(result.face_names(bevel).front(), feature, "chamfer");
      const std::vector<int> found = edge.empty() ? std::vector<int>() : input.find_edges(edge);
      if (!plane || found.size() != 1) {
        return nullptr;
      }
      bevel_planes.push_back(*plane);
      int wall = -1;
      int cut_face = -1;
      for (const int g : input.edge_faces(found.front())) {
        if (g < 0) {
          return nullptr;
        }
        const std::string& name = input.face_names(g).front();
        (piece.rfind(name + "#", 0) == 0 ? cut_face : wall) = g;
      }
      if (wall < 0 || cut_face < 0) {
        return nullptr;
      }
      const std::optional<gp_Pln> wall_plane = outward_plane(input.face(wall), input.occt());
      const std::optional<gp_Pln> cut_plane = outward_plane(input.face(cut_face), input.occt());
      if (!wall_plane || !cut_plane) {
        return nullptr;
      }
      walls.push_back(*wall_plane);
      cut = cut_plane;
      cut_index = cut_face;
    }
    // A chamfer of concave edges: each bevel faces away from the cut face
    // and from its wall; the walls not parallel.
    const gp_Dir& up = cut->Axis().Direction();
    const gp_Dir& wall_a = walls[0].Axis().Direction();
    const gp_Dir& wall_b = walls[1].Axis().Direction();
    for (std::size_t k = 0; k < 2; ++k) {
      const gp_Dir& n = bevel_planes[k].Axis().Direction();
      if (!(n.Dot(up) > 1.0e-3) || !(n.Dot(walls[k].Axis().Direction()) > 1.0e-3)) {
        return nullptr;
      }
    }
    if (wall_a.IsParallel(wall_b, 1.0e-3)) {
      return nullptr;
    }
    // The walls' bisector: through their line, its normal between theirs.
    const gp_Dir across(gp_Vec(wall_a) - gp_Vec(wall_b));
    const gp_Dir along = wall_a.Crossed(wall_b);
    // (A point on both walls' planes: rows of the walls' normals and the
    // line's direction.)
    gp_Mat rows(wall_a.XYZ(), wall_b.XYZ(), along.XYZ());
    rows.Transpose();
    const gp_XYZ rhs(wall_a.XYZ().Dot(walls[0].Location().XYZ()), wall_b.XYZ().Dot(walls[1].Location().XYZ()), 0.0);
    if (std::abs(rows.Determinant()) < 1.0e-9) {
      return nullptr;
    }
    const gp_Pnt on_both(rows.Inverted() * rhs);
    // The zone: past the corner's faces and the pieces on either side of the
    // bisector, within their bounding box grown by as much (not reaching
    // the faces beyond the corner's, a thin wall's outside).
    Bnd_Box box;
    double half = 0.0;
    std::vector<int> round = place;
    round.insert(round.end(), corner_faces[bevel_pair].begin(), corner_faces[bevel_pair].end());
    for (const int f : round) {
      BRepBndLib::Add(result.face(f), box);
      for (TopExp_Explorer ex(result.face(f), TopAbs_VERTEX); ex.More(); ex.Next()) {
        const gp_Pnt point = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
        half = std::max(half, std::abs(gp_Vec(on_both, point).Dot(gp_Vec(across))));
      }
      // (A curved face bulges between its vertices.)
      Bnd_Box face_box;
      BRepBndLib::Add(result.face(f), face_box);
      double x0, y0, z0, x1, y1, z1;
      face_box.Get(x0, y0, z0, x1, y1, z1);
      for (const double x : {x0, x1}) {
        for (const double y : {y0, y1}) {
          for (const double z : {z0, z1}) {
            half = std::max(half, std::abs(gp_Vec(on_both, gp_Pnt(x, y, z)).Dot(gp_Vec(across))));
          }
        }
      }
    }
    half += 0.1 * contact;
    box.Enlarge(0.1 * contact);
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    const gp_Pln side_a(on_both.Translated(gp_Vec(across) * half), across);
    const gp_Pln side_b(on_both.Translated(gp_Vec(across) * -half), across.Reversed());
    const std::optional<TopoDS_Shape> zone =
        clipped(BRepPrimAPI_MakeBox(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1)).Shape(), {{side_a, false}, {side_b, false}});
    if (!zone) {
      return nullptr;
    }
    // The bevels as planes meeting at their mitre, over the cut face (not
    // over a bore through it, where the bevels end on the bore's wall).
    const std::optional<TopoDS_Shape> over_cut = boolean(
        BRepAlgoAPI_Common(), *zone,
        BRepPrimAPI_MakePrism(input.face(cut_index), gp_Vec(up) * (2.0 * contact)).Shape(), nullptr);
    if (!over_cut) {
      return nullptr;
    }
    std::optional<TopoDS_Shape> fill;
    for (std::size_t k = 0; k < 2; ++k) {
      const std::optional<TopoDS_Shape> under =
          clipped(*over_cut, {{*cut, true}, {walls[0], true}, {walls[1], true}, {bevel_planes[k], false}});
      if (!under) {
        return nullptr;
      }
      fill = fill ? boolean(BRepAlgoAPI_Fuse(), *fill, *under, nullptr) : under;
      if (!fill) {
        return nullptr;
      }
    }
    // (The zones apart: one boolean takes them all.)
    Bnd_Box zone_box;
    BRepBndLib::Add(*zone, zone_box);
    for (const Bnd_Box& other : zone_boxes) {
      if (!zone_box.IsOut(other)) {
        return nullptr;
      }
    }
    zone_boxes.push_back(zone_box);
    builder.Add(zones, *zone);
    builder.Add(fills, *fill);
    // Under each piece past the bevels' crossing (over both bevels): the
    // column under it over both bevels' planes, cut from the input's
    // material.
    for (const int f : place) {
      bool past = true;
      for (TopExp_Explorer ex(result.face(f), TopAbs_VERTEX); ex.More() && past; ex.Next()) {
        const gp_Pnt point = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
        for (const gp_Pln& plane : bevel_planes) {
          if (gp_Vec(plane.Location(), point).Dot(gp_Vec(plane.Axis().Direction())) < -Precision::Confusion()) {
            past = false;
          }
        }
      }
      if (!past) {
        continue;
      }
      const std::optional<TopoDS_Shape> below = clipped(
          BRepPrimAPI_MakePrism(result.face(f), gp_Vec(up) * -contact).Shape(),
          {{bevel_planes[0], true}, {bevel_planes[1], true}});
      if (!below) {
        return nullptr;
      }
      builder.Add(slivers, *below);
      any_sliver = true;
    }
  }
  // All corners at once (their zones apart): the input's material in the
  // zones less the slivers' columns, with the mitred bevels, in place of
  // the result's.
  std::optional<TopoDS_Shape> kept = boolean(BRepAlgoAPI_Common(), input.occt(), zones, &history);
  if (kept && any_sliver) {
    kept = boolean(BRepAlgoAPI_Cut(), *kept, slivers, &history);
  }
  const std::optional<TopoDS_Shape> refill = kept ? boolean(BRepAlgoAPI_Fuse(), *kept, fills, &history) : std::nullopt;
  const std::optional<TopoDS_Shape> rest = refill ? boolean(BRepAlgoAPI_Cut(), solid, zones, &history) : std::nullopt;
  // (Glued: the two touch only on the zones' faces.)
  const std::optional<TopoDS_Shape> joined =
      rest ? boolean(BRepAlgoAPI_Fuse(), *rest, *refill, &history, BOPAlgo_GlueShift) : std::nullopt;
  if (!joined) {
    return nullptr;
  }
  const TopoDS_Shape whole = *joined;
  int solids = 0;
  for (TopExp_Explorer ex(whole, TopAbs_SOLID); ex.More(); ex.Next()) {
    ++solids;
  }
  const double before = detail::volume_of(solid);
  if (solids != 1 || !closed_solids(whole) || !BRepCheck_Analyzer(whole, true, true).IsValid() ||
      std::abs(detail::volume_of(whole) - before) > contact * contact * contact * static_cast<double>(pairs.size())) {
    return nullptr;
  }
  detail::FaceNamer namer(whole);
  namer.carry(history, result);
  // A face the regions gave (a bevel continued as its plane, a face beyond
  // grown out over the cut face): the name of the face of `result` on its
  // surface that it continues (touches).
  std::set<std::string> taken;
  for (int f = 0; f < result.face_count(); ++f) {
    for (const std::string& name : result.face_names(f)) {
      taken.insert(name);
    }
  }
  for (TopExp_Explorer ex(whole, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    if (namer.named(face)) {
      continue;
    }
    for (int f = 0; f < result.face_count(); ++f) {
      if (result.face_names(f).empty() || !on_one_surface(result.face(f), face)) {
        continue;
      }
      BRepExtrema_DistShapeShape distance(face, result.face(f));
      if (!distance.IsDone() || distance.Value() > Precision::Confusion() * 10.0) {
        continue;
      }
      const std::string& name = result.face_names(f).front();
      int k = 0;
      std::string next;
      do {
        next = name + "#" + std::to_string(k++);
      } while (!taken.insert(next).second);
      namer.add(face, next);
      break;
    }
  }
  // The input's faces in the zone (a wall the cone covered).
  namer.carry(history, input);
  namer.carry_by_containment(input, Precision::Confusion() * 10.0);
  namer.finish();
  ShapePtr out = namer.shape();
  out->copy_notes(result);
  for (int f = 0; f < out->face_count(); ++f) {
    if (out->face_names(f).empty()) {
      return nullptr;
    }
  }
  // None of the pieces left: a point inside each now inside the solid or
  // out of it, not on its boundary.
  BRepClass3d_SolidClassifier classifier(whole);
  for (const int p : pieces) {
    gp_Pnt point;
    gp_Vec normal;
    if (!detail::inner_point(result.face(p), point, normal)) {
      return nullptr;
    }
    classifier.Perform(point, Precision::Confusion());
    if (classifier.State() == TopAbs_ON) {
      return nullptr;
    }
  }
  return detail::join_same_surfaces(out);
}

// The pieces of a face the dressup takes away that its result still has
// (mitcad#1821): a chamfer along a chain over a corner's rounding left, at
// each corner, a triangle of the old end face between the two neighbouring
// bevels' faces (0.05 mm2 and 0.003 mm2 on a bent nut's opening), too wide
// for the sliver removal of mitcad#1106 (a strip under 0.1 % of the size)
// and a piece of a large face rather than a face of its own. Such a piece:
// a face of the result that is a new piece of one of the input's faces
// (its name the input face's with a piece's suffix), between faces the
// dressup made (beside two or more of them, as the triangle between two
// neighbouring bevels is; a piece beside one only lies where the band ends
// against other faces, a rounding's end on an earlier rounding or chamfer,
// which the stored designs' own results keep: removing those cost a
// defeaturing of the whole body, 2 to 11 s each, and pushed a design's
// fillets past the import's time, mitcad#1819), smaller than the dressup's
// reach squared, and wholly within
// the band (its vertices and its edges' points all nearer the chain's
// edges, `chain` with the tangent chains the sets follow, than `contact`,
// how far the dressup reaches along its faces): nothing of the old surface
// stays there at the dressup's exact size. Within the band means up to its
// edge (to 1e-4 of the reach), and a sliver under 0.01 mm2 may reach past
// it by less than its own size (the square root of its area): at a corner
// rounded more tightly than the dressup reaches, the neighbouring bevels'
// contact lines cross on the band's edge and the kernel leaves pieces of
// the face bounded by them (mitcad#1821: a nut's end chamfered 2.55 inside
// an octagonal rim rounded 1, at each corner a piece of 0.05 mm2 on the
// band's edge and a sliver of 0.003 mm2 reaching 0.03 past it, which a
// strict test within the band let through silently). The pieces are
// removed by the geometry kernel's defeaturing, the faces round them
// extended to meet (the bevels' faces closing between them), where those
// faces are planes,
// cylinders, cones, spheres or tori and that gives one valid solid of
// nearly the same volume. Elsewhere (beside a blend's faces, which meet the
// faces they round and each other tangentially, so that extending them to
// meet ran for minutes and failed, mitcad#1858), or where the defeaturing
// fails: a chamfer's pieces at corners rounded more tightly than it reaches
// are closed by mitring its bevels there (remnants_mitred), and otherwise
// each place is closed by a patch tangent to the dressup's faces
// round it, those cut back a setback from it (patch_remnants,
// remnant_patch.hpp). Where neither gives one valid solid the result keeps
// the pieces with a warning that says so, the pieces as its places. The
// result itself where there is no such piece.
ShapePtr without_remnants(const ShapePtr& result, const Shape& input, std::vector<int> chain,
                          const std::vector<char>& follows, double contact, const std::string& feature,
                          const char* role) {
  if (!result || !(contact > 0.0)) {
    return result;
  }
  try {
    OCC_CATCH_SIGNALS
    std::set<std::string> input_names;
    for (int f = 0; f < input.face_count(); ++f) {
      for (const std::string& name : input.face_names(f)) {
        input_names.insert(name);
      }
    }
    const int faces = result->face_count();
    std::vector<char> made(static_cast<std::size_t>(faces), 0);
    std::vector<int> candidates;
    for (int f = 0; f < faces; ++f) {
      const NameList& names = result->face_names(f);
      if (names.empty() || input_names.count(names.front()) != 0) {
        continue;
      }
      if (input_face_of(names.front(), input_names).empty()) {
        made[static_cast<std::size_t>(f)] = 1;
        continue;
      }
      GProp_GProps area;
      BRepGProp::SurfaceProperties(result->face(f), area);
      if (area.Mass() < contact * contact) {
        candidates.push_back(f);
      }
    }
    if (candidates.empty()) {
      return result;
    }
    try {
      OCC_CATCH_SIGNALS
      detail::follow_tangent_chains(input, chain, follows);
    } catch (const Standard_Failure&) {
    }
    BRep_Builder builder;
    TopoDS_Compound edges;
    builder.MakeCompound(edges);
    bool any_edge = false;
    for (int e = 0; e < input.edge_count() && e < static_cast<int>(chain.size()); ++e) {
      if (chain[static_cast<std::size_t>(e)] >= 0) {
        builder.Add(edges, input.edge(e));
        any_edge = true;
      }
    }
    if (!any_edge) {
      return result;
    }
    // Only pieces of the faces of the chain's edges, the faces the dressup
    // cuts: a face it runs onto beside them keeps its pieces (a hole's rim
    // rounded as a ring over a front edge's rounding, mitcad#1327).
    std::set<std::string> cut_faces;
    for (int e = 0; e < input.edge_count() && e < static_cast<int>(chain.size()); ++e) {
      if (chain[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      for (const int g : input.edge_faces(e)) {
        if (g >= 0) {
          for (const std::string& name : input.face_names(g)) {
            cut_faces.insert(name);
          }
        }
      }
    }
    candidates.erase(std::remove_if(candidates.begin(), candidates.end(),
                                    [&](int f) {
                                      return cut_faces.count(
                                                 input_face_of(result->face_names(f).front(), input_names)) == 0;
                                    }),
                     candidates.end());
    if (candidates.empty()) {
      return result;
    }
    const double sliver = 0.01;
    const detail::EdgeFaces edge_faces(*result);
    std::vector<int> remnants;
    for (const int f : candidates) {
      GProp_GProps area;
      BRepGProp::SurfaceProperties(result->face(f), area);
      const double within =
          contact * (1.0 + 1.0e-4) + (area.Mass() < sliver ? std::sqrt(std::max(area.Mass(), 0.0)) : 0.0);
      const auto near_chain = [&](const gp_Pnt& p) {
        BRepExtrema_DistShapeShape measured(BRepBuilderAPI_MakeVertex(p).Vertex(), edges);
        return measured.IsDone() && measured.Value() <= within;
      };
      std::set<int> made_beside;
      bool inside = true;
      for (TopExp_Explorer ex(result->face(f), TopAbs_EDGE); ex.More() && inside; ex.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
        const int e = result->edge_index(edge);
        if (e >= 0) {
          for (const int g : edge_faces.at(e)) {
            if (g != f && made[static_cast<std::size_t>(g)] != 0) {
              made_beside.insert(g);
            }
          }
        }
        if (BRep_Tool::Degenerated(edge)) {
          continue;
        }
        const BRepAdaptor_Curve curve(edge);
        for (int k = 0; k <= 4 && inside; ++k) {
          inside = near_chain(curve.Value(curve.FirstParameter() +
                                          (curve.LastParameter() - curve.FirstParameter()) * k / 4.0));
        }
      }
      if (made_beside.size() >= 2 && inside) {
        remnants.push_back(f);
      }
    }
    if (remnants.empty()) {
      return result;
    }
    NCollection_List<TopoDS_Shape> pieces;
    for (const int f : remnants) {
      pieces.Append(result->face(f));
    }
    // The defeaturing extends the faces round the pieces to meet: tried only
    // where they are all planes, cylinders, cones, spheres or tori (a
    // chamfer's bevels and the faces it cuts, mitcad#1821's case). Beside a
    // blend's B-spline faces it ran for minutes and failed (a reported nut
    // design: two roundings of 5 with 3 and 6 pieces, 155.6 s and 84.0 s,
    // both left with their pieces, mitcad#1819): those places are closed by
    // patches instead (below, mitcad#1858).
    bool analytic = true;
    for (const int f : remnants) {
      for (TopExp_Explorer ex(result->face(f), TopAbs_EDGE); ex.More() && analytic; ex.Next()) {
        const int e = result->edge_index(TopoDS::Edge(ex.Current()));
        if (e < 0) {
          continue;
        }
        for (const int g : edge_faces.at(e)) {
          if (g == f || std::find(remnants.begin(), remnants.end(), g) != remnants.end()) {
            continue;
          }
          switch (BRepAdaptor_Surface(result->face(g), false).GetType()) {
          case GeomAbs_Plane:
          case GeomAbs_Cylinder:
          case GeomAbs_Cone:
          case GeomAbs_Sphere:
          case GeomAbs_Torus:
            break;
          default:
            analytic = false;
          }
        }
      }
    }
    BRepAlgoAPI_Defeaturing defeaturing;
    defeaturing.SetShape(result->occt());
    defeaturing.AddFacesToRemove(pieces);
    defeaturing.SetRunParallel(false);
    defeaturing.SetToFillHistory(true);
    bool removed = false;
    try {
      OCC_CATCH_SIGNALS
      if (analytic) {
        detail::build(defeaturing);
        removed = defeaturing.IsDone() && !defeaturing.HasWarnings();
      }
    } catch (const Standard_Failure&) {
      removed = false;
    }
    if (removed) {
      const TopoDS_Shape whole = defeaturing.Shape();
      int solids = 0;
      for (TopExp_Explorer ex(whole, TopAbs_SOLID); ex.More(); ex.Next()) {
        ++solids;
      }
      const double before = detail::volume_of(result->occt());
      if (solids == 1 && closed_solids(whole) && BRepCheck_Analyzer(whole, true, true).IsValid() &&
          std::abs(detail::volume_of(whole) - before) <= 1.0e-4 * std::abs(before)) {
        detail::FaceNamer namer(whole);
        namer.carry(*defeaturing.History(), *result);
        namer.finish();
        ShapePtr out = namer.shape();
        out->copy_notes(*result);
        return out;
      }
    }
    // At a corner rounded more tightly than a chamfer reaches: the bevels
    // mitred.
    if (std::strcmp(role, "chamfer") == 0) {
      try {
        OCC_CATCH_SIGNALS
        if (ShapePtr mitred = remnants_mitred(*result, input, remnants, made, feature, contact)) {
          return mitred;
        }
      } catch (const Standard_Failure&) {
      }
    }
    // A patch at each place, tangent to the dressup's faces round it (cut
    // back a setback from it) and, for a fillet, to the faces it rounds:
    // the rounding closed where the kernel left the old face (mitcad#1858).
    if (ShapePtr patched = detail::patch_remnants(*result, input, remnants, made, feature, role, contact)) {
      return patched;
    }
    // (Kept, with a warning: never silently, mitcad#1821.)
    std::vector<Shape::NotePlace> places;
    for (const int f : remnants) {
      places.push_back(Shape::NotePlace::of(Shape::NotePlace::Kind::Face, result->face_names(f).front()));
    }
    const std::size_t count = remnants.size();
    result->add_note(std::string("the ") + role + " leaves " +
                         (count == 1 ? std::string("a small piece") : std::to_string(count) + " small pieces") +
                         " of a face it takes away, which could not be removed: check the result",
                     std::move(places));
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_Failure&) {
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

// How far from its edges a rounding of the edges `owner` selects (a set
// per edge, -1 for none) touches its faces at most: r tan(a / 2), a the
// angle between the faces' normals (as over_cutouts takes it), r tan(a / 2)
// far beyond the radius where the faces meet at a sharp angle; at least the
// largest size (mitcad#1150).
double largest_contact(const Shape& input, const std::vector<FilletSet>& sets, const std::vector<int>& owner) {
  double contact = largest_size(sets);
  const detail::EdgeFaces edge_faces(input);
  const int edges = std::min(input.edge_count(), static_cast<int>(owner.size()));
  for (int e = 0; e < edges; ++e) {
    const int s = owner[static_cast<std::size_t>(e)];
    if (s < 0 || s >= static_cast<int>(sets.size())) {
      continue;
    }
    const std::vector<int> faces = edge_faces.at(e);
    if (faces.size() != 2) {
      continue;
    }
    try {
      OCC_CATCH_SIGNALS
      double largest_angle = 0.0;
      bool measured = false;
      for (const double t : {0.0, 0.5, 1.0}) {
        // A face whose normal is undefined at an end of the edge (a
        // subdivision surface's patch at a corner where other than four
        // patches meet, its derivatives parallel there) is measured a
        // little inside the edge, where the normal tends to its limit.
        for (const double at : {t, t + (0.5 - t) * 1.0e-3}) {
          try {
            largest_angle = std::max(
                largest_angle, detail::normal_angle(input.edge(e), input.face(faces[0]), input.face(faces[1]), at));
            measured = true;
            break;
          } catch (const std::runtime_error&) {
          }
        }
      }
      if (!measured) {
        continue;
      }
      // Faces folding back on each other: no finite contact.
      if (!(largest_angle < 2.0 * kHalfPi - 1.0e-3)) {
        continue;
      }
      const double radius = largest_radius(input, sets[static_cast<std::size_t>(s)], e);
      contact = std::max(contact, radius * std::tan(largest_angle / 2.0));
    } catch (const Standard_Failure&) {
    }
  }
  return contact;
}

std::string millimetres(double value) {
  char text[32];
  std::snprintf(text, sizeof text, "%.3g", value);
  return text;
}

// The narrowest place of face `face` between its straight edges `a` and
// `b`, which are not parallel and share no vertex: of nine points along the
// stretch where they run beside each other (a's points whose feet on b fall
// within b), the one nearest to b, the face lying between them there. The
// width is the straight distance across: on a cylinder or a cone it is
// shorter than the way over the surface by less than a thousandth where
// the edges are within 10 degrees of each other about the axis. None where
// they share a vertex (a corner of the face, where roundings meet as usual)
// or do not run beside each other.
struct Narrowest {
  double width = 0.0;
};
std::optional<Narrowest> narrowest_between(const Shape& body, const TopoDS_Face& face, int a, int b) {
  const TopoDS_Edge& ea = body.edge(a);
  const TopoDS_Edge& eb = body.edge(b);
  TopoDS_Vertex a0, a1, b0, b1;
  TopExp::Vertices(ea, a0, a1);
  TopExp::Vertices(eb, b0, b1);
  if (a0.IsSame(b0) || a0.IsSame(b1) || a1.IsSame(b0) || a1.IsSame(b1)) {
    return std::nullopt;
  }
  const BRepAdaptor_Curve ca(ea);
  const BRepAdaptor_Curve cb(eb);
  const gp_Pnt b_start = cb.Value(cb.FirstParameter());
  const gp_Pnt b_end = cb.Value(cb.LastParameter());
  const double b_length = b_start.Distance(b_end);
  if (!(b_length > Precision::Confusion())) {
    return std::nullopt;
  }
  const gp_Vec b_along(b_start, b_end);
  std::optional<Narrowest> found;
  constexpr int kSamples = 9;
  for (int k = 0; k < kSamples; ++k) {
    // (Just inside the ends: at an end the point between them lies on the
    // face's boundary.)
    const double t = 0.001 + 0.998 * k / (kSamples - 1);
    const gp_Pnt on_a = ca.Value(ca.FirstParameter() + t * (ca.LastParameter() - ca.FirstParameter()));
    const double along = gp_Vec(b_start, on_a).Dot(b_along) / (b_length * b_length);
    if (along < 0.0 || along > 1.0) {
      continue;
    }
    const gp_Pnt on_b = b_start.Translated(b_along * along);
    const double width = on_a.Distance(on_b);
    if (!(width > Precision::Confusion()) || (found && width >= found->width)) {
      continue;
    }
    const gp_Pnt between((on_a.XYZ() + on_b.XYZ()) * 0.5);
    if (BRepClass_FaceClassifier(face, between, Precision::Confusion()).State() != TopAbs_IN) {
      continue;
    }
    found = Narrowest{width};
  }
  return found;
}

// Why a fillet that the geometry kernel cannot build fails, where a planar
// face lies between two selected straight edges parallel to each other
// whose roundings together reach farther across it than it is wide
// (mitcad#1482): a slab's eave 1.5 wide, rounded along both its edges, the
// one at its acute tip touching it 1.73 r from the edge and the other
// 0.58 r. The two roundings would meet beyond each other's contact and the
// face vanish under them; the kernel joins such roundings only in some
// cases (port patch 0016: the rims of a plate thinner than the roundings,
// both convex or both concave, with the corners at their ends and the face
// consumed between them since 1640 to 1642, mitcad#1499), and elsewhere,
// where the face vanishes, the corners at its ends have no face to end the
// roundings on (a slab's eave, mitcad#1500). Each rounding
// touches its faces r tan(a / 2) from its edge, a the angle between the
// faces' normals (as largest_contact takes it), r its largest radius. The
// text names the face where the roundings overlap most for its width, the
// two edges, how far each reaches and the face's width there, and how many
// more faces are like it; empty where no face is (or the body cannot be
// measured). Asymmetric sets, whose distances are not radii, are left
// out; so are curved edges, and curved faces but for cylinders and cones
// between straight edges that close in on each other (narrowest_between,
// mitcad#2038).
std::string narrow_face_cause(const Shape& body, const std::vector<FilletSet>& sets, const std::vector<int>& owner) {
  try {
    OCC_CATCH_SIGNALS
    const detail::EdgeFaces edge_faces(body);
    const int edges = std::min(body.edge_count(), static_cast<int>(owner.size()));
    // How far from each selected edge its rounding touches its faces; -1
    // for the edges not measured.
    std::vector<double> contact(static_cast<std::size_t>(body.edge_count()), -1.0);
    for (int e = 0; e < edges; ++e) {
      const int s = owner[static_cast<std::size_t>(e)];
      if (s < 0 || s >= static_cast<int>(sets.size()) ||
          sets[static_cast<std::size_t>(s)].size == FilletSize::Asymmetric) {
        continue;
      }
      const std::vector<int> faces = edge_faces.at(e);
      if (faces.size() != 2 || faces[0] == faces[1]) {
        continue;
      }
      try {
        OCC_CATCH_SIGNALS
        const double angle = detail::normal_angle(body.edge(e), body.face(faces[0]), body.face(faces[1]));
        if (angle > 1.0e-3 && angle < 2.0 * kHalfPi - 1.0e-3) {
          contact[static_cast<std::size_t>(e)] =
              largest_radius(body, sets[static_cast<std::size_t>(s)], e) * std::tan(angle / 2.0);
        }
      } catch (const Standard_Failure&) {
      } catch (const std::exception&) {
        // (A chord length set whose angle varies along the edge.)
      }
    }
    struct Line {
      int edge = -1;
      gp_Pnt start;
      gp_Dir direction;
      double length = 0.0;
    };
    struct Found {
      int face = -1;
      int a = -1;
      int b = -1;
      double width = 0.0;
      double ratio = 1.0;
    } worst;
    int faces_found = 0;
    for (int f = 0; f < body.face_count(); ++f) {
      const TopoDS_Face& face = body.face(f);
      const GeomAbs_SurfaceType surface = BRepAdaptor_Surface(face, false).GetType();
      if (surface != GeomAbs_Plane && surface != GeomAbs_Cone && surface != GeomAbs_Cylinder) {
        continue;
      }
      std::vector<Line> lines;
      for (TopExp_Explorer x(face, TopAbs_EDGE); x.More(); x.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(x.Current());
        const int e = body.edge_index(edge);
        if (e < 0 || contact[static_cast<std::size_t>(e)] < 0.0 ||
            std::any_of(lines.begin(), lines.end(), [e](const Line& l) { return l.edge == e; })) {
          continue;
        }
        const BRepAdaptor_Curve curve(edge);
        if (curve.GetType() != GeomAbs_Line) {
          continue;
        }
        const gp_Pnt p0 = curve.Value(curve.FirstParameter());
        const gp_Pnt p1 = curve.Value(curve.LastParameter());
        const double length = p0.Distance(p1);
        if (length > Precision::Confusion()) {
          lines.push_back({e, p0, gp_Dir(gp_Vec(p0, p1)), length});
        }
      }
      bool counted = false;
      for (std::size_t i = 0; i < lines.size(); ++i) {
        for (std::size_t j = i + 1; j < lines.size(); ++j) {
          const Line& a = lines[i];
          const Line& b = lines[j];
          const double reach =
              contact[static_cast<std::size_t>(a.edge)] + contact[static_cast<std::size_t>(b.edge)];
          if (!a.direction.IsParallel(b.direction, 1.0e-6)) {
            // Two straight edges that close in on each other across the
            // face without meeting (a rib's side, a cone, between its start
            // and end faces a few degrees apart, mitcad#2038): the width at
            // the narrowest of nine points along their common stretch.
            const std::optional<Narrowest> narrowest = narrowest_between(body, face, a.edge, b.edge);
            if (!narrowest || !(reach > narrowest->width * (1.0 + 1.0e-6))) {
              continue;
            }
            if (!counted) {
              counted = true;
              ++faces_found;
            }
            if (reach / narrowest->width > worst.ratio) {
              worst = {f, a.edge, b.edge, narrowest->width, reach / narrowest->width};
            }
            continue;
          }
          if (surface != GeomAbs_Plane) {
            continue;
          }
          // Where the two run beside each other, along a.
          const gp_Vec along(a.direction);
          double t0 = gp_Vec(a.start, b.start).Dot(along);
          double t1 = t0 + b.length * (a.direction.IsOpposite(b.direction, 1.0e-6) ? -1.0 : 1.0);
          if (t0 > t1) {
            std::swap(t0, t1);
          }
          const double from = std::max(0.0, t0);
          const double to = std::min(a.length, t1);
          if (!(to - from > Precision::Confusion())) {
            continue;
          }
          // The width across the face between them, at the middle of that
          // stretch, the face lying between them there.
          const gp_Pnt on_a = a.start.Translated(along * (0.5 * (from + to)));
          const gp_Vec to_b(a.start, b.start);
          const gp_Pnt on_b = on_a.Translated(to_b - along * to_b.Dot(along));
          const double width = on_a.Distance(on_b);
          if (!(width > Precision::Confusion()) || !(reach > width * (1.0 + 1.0e-6))) {
            continue;
          }
          const gp_Pnt between((on_a.XYZ() + on_b.XYZ()) * 0.5);
          if (BRepClass_FaceClassifier(face, between, Precision::Confusion()).State() != TopAbs_IN) {
            continue;
          }
          if (!counted) {
            counted = true;
            ++faces_found;
          }
          if (reach / width > worst.ratio) {
            worst = {f, a.edge, b.edge, width, reach / width};
          }
        }
      }
    }
    if (worst.face < 0) {
      return {};
    }
    const NameList& names = body.face_names(worst.face);
    const std::string face = names.empty() ? std::string("an unnamed face") : "face " + names.front();
    std::string more;
    if (faces_found > 1) {
      more = " (and " + std::to_string(faces_found - 1) + (faces_found == 2 ? " more face" : " more faces") +
             " like it)";
    }
    // (The edges as one list of names, which a notice reads as "the edges"
    // with their places.)
    const std::string& a = body.edge_name(worst.a);
    const std::string& b = body.edge_name(worst.b);
    const std::string edges_named = !a.empty() && !b.empty()
                                        ? "the edges " + a + " and " + b
                                        : describe_edge(body, worst.a) + " and " + describe_edge(body, worst.b);
    // In plain words for the person, the edges and the face painted; the
    // names for agents (mitcad#1612).
    const auto said = [&](const std::string& roundings, const std::string& into) {
      return "the roundings of " + roundings + " reach " + millimetres(contact[static_cast<std::size_t>(worst.a)]) +
             " and " + millimetres(contact[static_cast<std::size_t>(worst.b)]) + " mm into " + into +
             " between them, which is only " + millimetres(worst.width) +
             " mm wide there: the face would vanish under them" + more;
    };
    std::vector<ErrorPlace> places{edge_place(a), edge_place(b)};
    if (!names.empty()) {
      places.push_back(face_place(names.front()));
    }
    return placed_error(said("two edges", "the face"), said(edges_named, face), places);
  } catch (const Standard_Failure&) {
  } catch (const std::exception&) {
  }
  return {};
}

// A fillet's error with the cause narrow_face_cause found: the error's
// first clause stays its beginning ("fillet failed; ", "the fillet would
// leave the body open: ", which the import reads), then the cause, then the
// rest of the error, the geometry kernel's own words, in brackets.
std::string with_cause(const std::string& error, const std::string& cause) {
  const std::size_t p = std::min(error.find("; "), error.find(": "));
  if (p == std::string::npos) {
    return error + "; " + cause;
  }
  std::string rest = error.substr(p + 2);
  if (rest.find("geometry kernel") == std::string::npos) {
    rest = "the geometry kernel: " + rest;
  }
  return error.substr(0, p + 2) + cause + " (" + rest + ")";
}

// Sets without a tangent chain whose geometry kernel chain runs on past
// their selected edges into edges no set selects (mitcad#2037): OCCT
// cannot end the dressup at such a vertex, so it is built over the whole
// chain (`full`, the sets with their tangent chain) and kept over the
// selected edges only, ending against the face beyond the vertex
// (mitcad#2193), else running out over the next edge, else ending flat
// there (runout_dressup.hpp). `size_of` gives a chain its set's size and
// throws for sizes that cannot end so. Null when no set has such a chain;
// throws the refusal, said in words, when no way builds.
template <class Set, class Full, class SizeOf>
ShapePtr over_partial_chains(const std::string& feature, const char* what, const Shape& body,
                             const std::vector<Set>& sets, bool is_chamfer, Full&& full, SizeOf&& size_of) {
  std::vector<char> off;
  bool any = false;
  for (const Set& set : sets) {
    off.push_back(set.tangent_chain ? 0 : 1);
    any = any || !set.tangent_chain;
  }
  if (!any) {
    return nullptr;
  }
  const std::vector<int> owner = select_edges(body, sets);
  std::vector<detail::PartialChain> chains;
  try {
    OCC_CATCH_SIGNALS
    chains = detail::partial_chains(body, owner, off, is_chamfer);
  } catch (const Standard_Failure&) {
    return nullptr; // (the dressup's own path says what is wrong)
  }
  if (chains.empty()) {
    return nullptr;
  }
  // The first selected edge that continues into an unselected one, for
  // the places of the refusal.
  int seed = -1;
  int unselected = -1;
  for (const detail::PartialChain& chain : chains) {
    const std::size_t n = chain.edges.size();
    for (std::size_t i = 0; i < n && seed < 0; ++i) {
      const std::size_t j = (i + 1) % n;
      if ((j == 0 && !chain.closed) || chain.selected[i] == chain.selected[j]) {
        continue;
      }
      seed = chain.edges[chain.selected[i] != 0 ? i : j];
      unselected = chain.edges[chain.selected[i] != 0 ? j : i];
    }
  }
  if (seed < 0) {
    return nullptr; // (a chain of one edge: nothing runs on)
  }
  std::vector<Set> whole = sets;
  for (detail::PartialChain& chain : chains) {
    chain.size = size_of(chain, seed, unselected);
    whole[static_cast<std::size_t>(chain.set)].tangent_chain = true;
  }
  for (const detail::PartialChain& chain : chains) {
    std::string listed;
    for (std::size_t i = 0; i < chain.edges.size(); ++i) {
      listed += std::string(chain.selected[i] != 0 ? " [selected] " : " ") + body.edge_name(chain.edges[i]);
    }
    detail::runout_trace(std::string("chain") + (chain.closed ? " (closed):" : ":") + listed);
  }
  const ShapePtr built = full(whole);
  if (built) {
    std::string made;
    for (int f = 0; f < built->face_count(); ++f) {
      for (const std::string& name : built->face_names(f)) {
        if (name.compare(0, feature.size() + 1, feature + ":") == 0) {
          made += " " + name;
        }
      }
    }
    detail::runout_trace("the dressup over the whole chain:" + made);
    // Against the face beyond the vertex, else running out over the next
    // edge, else flat (mitcad#2193).
    for (const detail::ChainEnd end : {detail::ChainEnd::Face, detail::ChainEnd::RunOut, detail::ChainEnd::Flat}) {
      const std::string ending = end == detail::ChainEnd::Face     ? "face end"
                                 : end == detail::ChainEnd::RunOut ? "run-out"
                                                                   : "flat end";
      detail::runout_trace(ending + ":");
      try {
        OCC_CATCH_SIGNALS
        ShapePtr result = detail::over_selected_edges(feature, body, *built, chains, end);
        int solids = 0;
        for (TopExp_Explorer it(result ? result->occt() : TopoDS_Shape(), TopAbs_SOLID); it.More(); it.Next()) {
          ++solids;
        }
        if (result && solids == 1 && closed_solids(result->occt())) {
          return result;
        }
        if (result) {
          detail::runout_trace(ending + ": " + std::to_string(solids) + " solids, or not closed");
        }
      } catch (const Cancelled&) {
        throw;
      } catch (const Standard_Failure& failure) {
        detail::runout_trace(ending + ": the geometry kernel failed: " + std::string(failure.what()));
      } catch (const std::exception& error) {
        detail::runout_trace(ending + ": " + error.what());
      }
    }
  } else {
    detail::runout_trace("the dressup over the whole chain is not built");
  }
  const std::string said = std::string("unsupported: a selected edge continues smoothly into an edge not selected, "
                                       "and the ") +
                           what +
                           " could neither end against the face beyond it, run out over that edge nor end flat at it";
  throw std::invalid_argument(edges_error(body, {seed, unselected}, said,
                                          "unsupported: " + describe_edge(body, seed) + " continues smoothly into " +
                                              describe_edge(body, unselected) + ", and the " + what +
                                              " could neither end against the face beyond it, run out over it nor "
                                              "end flat at it"));
}

// The size of a fillet set's chain for its run-out; variable, asymmetric,
// spline and curvature continuous sizes cannot run out.
detail::RunOutSize fillet_run_out_size(const Shape& body, const FilletSet& set, int seed, int unselected) {
  if ((set.size != FilletSize::Constant && set.size != FilletSize::ChordLength) || set.curvature) {
    const std::string said = "unsupported: variable, asymmetric, spline and curvature continuous fillets need "
                             "Tangent Chain on where a selected edge continues smoothly into an edge not selected";
    throw std::invalid_argument(edges_error(body, {seed, unselected}, said,
                                            said + " (" + describe_edge(body, seed) + " into " +
                                                describe_edge(body, unselected) + ")"));
  }
  detail::RunOutSize size;
  size.fillet = true;
  if (set.size == FilletSize::Constant) {
    size.radius = set.radius;
  } else {
    size.chord = set.chord;
  }
  return size;
}

// The size of a chamfer set's chain for its run-out, measured on the face
// of its first selected edge the set measures on.
detail::RunOutSize chamfer_run_out_size(const Shape& body, const ChamferSet& set, const detail::PartialChain& chain) {
  detail::RunOutSize size;
  size.distance = set.spec.distance;
  if (set.spec.type == ChamferType::EqualDistance) {
    return size;
  }
  if (set.spec.type == ChamferType::TwoDistances) {
    size.distance2 = set.spec.distance2;
  } else {
    size.angle = set.spec.angle;
  }
  for (std::size_t i = 0; i < chain.edges.size(); ++i) {
    if (chain.selected[i] != 0) {
      size.measured = measured_face(body, chain.edges[i], set.spec.reference_face, set.spec.flip);
      break;
    }
  }
  return size;
}

// The ends of grooves (detail::groove_end, mitcad#2154) the sets round:
// a selected edge's name and its set's constant radius.
struct RoundedGrooveEnd {
  std::string edge;
  double radius = 0.0;
};

// Rounds the groove ends `ends` one by one (found again by their names);
// null where one cannot be built so.
ShapePtr round_groove_ends(const std::string& feature, const Shape& body, const std::vector<RoundedGrooveEnd>& ends) {
  ShapePtr made = std::make_shared<Shape>(body);
  for (const RoundedGrooveEnd& end : ends) {
    const std::vector<int> found = made->find_edges(end.edge);
    if (found.size() != 1) {
      return nullptr;
    }
    const std::optional<detail::GrooveEnd> groove = detail::groove_end(*made, found.front());
    if (!groove) {
      return nullptr;
    }
    made = detail::rounded_groove_end(feature, *made, found.front(), *groove, end.radius);
    if (!made) {
      return nullptr;
    }
  }
  return made;
}

// The rounding of selected edges (`owner`) that are all ends of grooves,
// each narrower than its groove, on the body with the end cap continued
// to the whole disc and the floor to the whole tube (past_narrow_faces'
// `ends`, mitcad#2154). Null where an edge is no groove end or its set no
// constant radius, or it cannot be built so.
ShapePtr groove_ends_rounded(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                             const std::vector<int>& owner) {
  std::vector<RoundedGrooveEnd> ends;
  for (int e = 0; e < body.edge_count() && e < static_cast<int>(owner.size()); ++e) {
    const int s = owner[static_cast<std::size_t>(e)];
    if (s < 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    if (set.size != FilletSize::Constant || body.edge_name(e).empty()) {
      return nullptr;
    }
    const std::optional<detail::GrooveEnd> groove = detail::groove_end(body, e);
    if (!groove || set.radius >= groove->circle.Radius() - detail::kGrooveRadiusTolerance) {
      return nullptr;
    }
    ends.push_back({body.edge_name(e), set.radius});
  }
  return ends.empty() ? nullptr : round_groove_ends(feature, body, ends);
}

// Roundings of groove ends as wide as their grooves or wider (mitcad#2154),
// before the geometry kernel is asked for them: wider fails in words (no
// ball of that radius fits in the groove, detail::groove_too_narrow); as
// wide (within 1e-6 mm) is the sphere of that radius on the groove's axis
// (detail::rounded_groove_end), whose ball does not move along the edge,
// which the kernel's rolling ball cannot build (it crashed). The sets' other
// edges are then rounded on that body by fillet() itself (`whole` set).
// Null where no such end is selected.
ShapePtr groove_balls(const std::string& feature, const Shape& input, const std::vector<FilletSet>& sets,
                      const std::vector<int>& owner, bool rolling_ball_corners, bool& whole) {
  whole = false;
  std::vector<RoundedGrooveEnd> balls;
  std::vector<char> ball(owner.size(), 0);
  for (int e = 0; e < input.edge_count() && e < static_cast<int>(owner.size()); ++e) {
    const int s = owner[static_cast<std::size_t>(e)];
    if (s < 0 || sets[static_cast<std::size_t>(s)].size != FilletSize::Constant) {
      continue;
    }
    const double radius = sets[static_cast<std::size_t>(s)].radius;
    const std::optional<detail::GrooveEnd> groove = detail::groove_end(input, e);
    if (!groove) {
      continue;
    }
    const double groove_radius = groove->circle.Radius();
    if (radius > groove_radius + detail::kGrooveRadiusTolerance) {
      throw std::invalid_argument(detail::groove_too_narrow(input, e, *groove, radius));
    }
    if (std::abs(radius - groove_radius) <= detail::kGrooveRadiusTolerance && !input.edge_name(e).empty()) {
      balls.push_back({input.edge_name(e), radius});
      ball[static_cast<std::size_t>(e)] = 1;
    }
  }
  if (balls.empty()) {
    return nullptr;
  }
  ShapePtr made = round_groove_ends(feature, input, balls);
  if (!made) {
    const std::string& name = balls.front().edge;
    throw std::runtime_error(
        "fillet failed; " +
        placed_error("the fillet as wide as the groove it ends cannot be built: the ball it ends the groove in does "
                     "not fit the faces round the groove's end",
                     "the fillet of edge " + name +
                         " as wide as the groove it ends cannot be built: the ball it ends the groove in does not "
                         "fit the faces round the groove's end",
                     {edge_place(name)}));
  }
  // The sets' other edges, by name, rounded on that body.
  std::vector<FilletSet> rest;
  for (std::size_t s = 0; s < sets.size(); ++s) {
    FilletSet other = sets[s];
    other.edges.clear();
    other.faces.clear();
    for (int e = 0; e < input.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] == static_cast<int>(s) && ball[static_cast<std::size_t>(e)] == 0) {
        other.edges.push_back(input.edge_name(e));
      }
    }
    if (!other.edges.empty()) {
      rest.push_back(std::move(other));
    }
  }
  if (rest.empty()) {
    return made;
  }
  whole = true;
  return fillet(feature, *made, rest, rolling_ball_corners);
}

// The selected edges (`owner`: the set of each edge, -1 for none) in
// groups that the geometry kernel rounds apart from each other: its
// contours (the edges' tangent chains) joined where they share a vertex,
// or a face on which they come within `reach` of each other (the copies
// of a pattern's rounding stand on one floor face, far apart), so that no
// two groups' roundings meet at a corner or on a face. Each group as an
// owner list of `body`'s edges; empty where the edges form one group, or
// an edge lies in no contour or has no name (the groups are found again by
// name on the bodies built in between).
std::vector<std::vector<int>> independent_edge_groups(const Shape& body, const std::vector<int>& owner,
                                                      double reach) {
  std::vector<std::vector<int>> groups;
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet maker(body.occt());
    std::vector<int> selected;
    for (int e = 0; e < body.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      if (body.edge_name(e).empty()) {
        return {};
      }
      selected.push_back(e);
      if (maker.Contour(body.edge(e)) == 0) {
        maker.Add(1.0, body.edge(e));
      }
    }
    const int contours = maker.NbContours();
    if (contours < 2) {
      return {};
    }
    std::vector<int> root(static_cast<std::size_t>(contours) + 1);
    for (int c = 0; c <= contours; ++c) {
      root[static_cast<std::size_t>(c)] = c;
    }
    const auto find = [&](int c) {
      while (root[static_cast<std::size_t>(c)] != c) {
        c = root[static_cast<std::size_t>(c)] = root[static_cast<std::size_t>(root[static_cast<std::size_t>(c)])];
      }
      return c;
    };
    const auto join = [&](int a, int b) { root[static_cast<std::size_t>(find(a))] = find(b); };
    // Each contour's edges as one compound, for the distances.
    std::vector<TopoDS_Compound> edges_of(static_cast<std::size_t>(contours) + 1);
    BRep_Builder builder;
    for (int c = 1; c <= contours; ++c) {
      builder.MakeCompound(edges_of[static_cast<std::size_t>(c)]);
      for (int i = 1; i <= maker.NbEdges(c); ++i) {
        builder.Add(edges_of[static_cast<std::size_t>(c)], maker.Edge(c, i));
      }
    }
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
    // The contours beside each face and at each vertex.
    NCollection_DataMap<TopoDS_Shape, std::vector<int>, TopTools_ShapeMapHasher> at_face;
    NCollection_DataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> at_vertex;
    for (int c = 1; c <= contours; ++c) {
      for (int i = 1; i <= maker.NbEdges(c); ++i) {
        const TopoDS_Edge& edge = maker.Edge(c, i);
        if (const NCollection_List<TopoDS_Shape>* faces = faces_of.Seek(edge)) {
          for (const TopoDS_Shape& face : *faces) {
            if (!at_face.IsBound(face)) {
              at_face.Bind(face, std::vector<int>());
            }
            std::vector<int>& beside = at_face.ChangeFind(face);
            if (std::find(beside.begin(), beside.end(), c) == beside.end()) {
              beside.push_back(c);
            }
          }
        }
        for (TopExp_Explorer v(edge, TopAbs_VERTEX); v.More(); v.Next()) {
          if (const int* seen = at_vertex.Seek(v.Current())) {
            join(*seen, c);
          } else {
            at_vertex.Bind(v.Current(), c);
          }
        }
      }
    }
    for (NCollection_DataMap<TopoDS_Shape, std::vector<int>, TopTools_ShapeMapHasher>::Iterator it(at_face);
         it.More(); it.Next()) {
      const std::vector<int>& beside = it.Value();
      for (std::size_t i = 0; i < beside.size(); ++i) {
        for (std::size_t j = i + 1; j < beside.size(); ++j) {
          if (find(beside[i]) == find(beside[j])) {
            continue;
          }
          throw_if_cancelled();
          BRepExtrema_DistShapeShape distance(edges_of[static_cast<std::size_t>(beside[i])],
                                              edges_of[static_cast<std::size_t>(beside[j])]);
          if (!distance.IsDone() || distance.Value() <= reach) {
            join(beside[i], beside[j]);
          }
        }
      }
    }
    std::vector<int> group_of(static_cast<std::size_t>(contours) + 1, -1);
    for (const int e : selected) {
      const int c = maker.Contour(body.edge(e));
      if (c <= 0) {
        return {};
      }
      int& g = group_of[static_cast<std::size_t>(find(c))];
      if (g < 0) {
        g = static_cast<int>(groups.size());
        groups.emplace_back(owner.size(), -1);
      }
      groups[static_cast<std::size_t>(g)][static_cast<std::size_t>(e)] = owner[static_cast<std::size_t>(e)];
    }
  } catch (const Standard_Failure&) {
    return {};
  }
  if (groups.size() < 2) {
    return {};
  }
  return groups;
}

// The notes, infos and remarks of `from` that `into` does not hold yet.
void add_new_messages(Shape& into, const Shape& from) {
  const auto holds = [](const std::vector<std::string>& texts, const std::string& text) {
    return std::find(texts.begin(), texts.end(), text) != texts.end();
  };
  for (std::size_t k = 0; k < from.notes().size(); ++k) {
    if (!holds(into.notes(), from.notes()[k])) {
      into.add_note(from.notes()[k],
                    k < from.note_places().size() ? from.note_places()[k] : std::vector<Shape::NotePlace>());
    }
  }
  for (const std::string& info : from.infos()) {
    if (!holds(into.infos(), info)) {
      into.add_info(info);
    }
  }
  for (std::size_t k = 0; k < from.remarks().size(); ++k) {
    if (!holds(into.remarks(), from.remarks()[k])) {
      into.add_remark(from.remarks()[k], k < from.remark_places().size() ? from.remark_places()[k]
                                                                         : std::vector<Shape::NotePlace>());
    }
  }
}

// The groups of independent_edge_groups built one after another by
// `build` (of a body and its owner list), each group's edges found by
// name on the body the groups before it made; with the messages of every
// group's result. Null where a group fails or its edges are not found.
template <class Build>
ShapePtr groups_one_by_one(const Shape& input, const std::vector<std::vector<int>>& groups, Build&& build) {
  std::vector<ShapePtr> made;
  const Shape* on = &input;
  try {
    for (const std::vector<int>& group : groups) {
      std::vector<int> owner(static_cast<std::size_t>(on->edge_count()), -1);
      for (std::size_t e = 0; e < group.size(); ++e) {
        if (group[e] < 0) {
          continue;
        }
        const std::vector<int> found = on->find_edges(input.edge_name(static_cast<int>(e)));
        if (found.empty()) {
          return nullptr;
        }
        for (const int i : found) {
          owner[static_cast<std::size_t>(i)] = group[e];
        }
      }
      ShapePtr next = build(*on, owner);
      if (!next) {
        return nullptr;
      }
      made.push_back(next);
      on = next.get();
    }
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    return nullptr;
  }
  ShapePtr result = made.back();
  for (std::size_t k = 0; k + 1 < made.size(); ++k) {
    add_new_messages(*result, *made[k]);
  }
  return result;
}

// Groups apart (mitcad#2165): at most this many groups are built one after
// the other (each group is built twice, alone and in turn).
constexpr std::size_t kMostGroupsApart = 6;
// The volume the groups built in turn change the body by matches the sum
// of what each group alone changes it by within this part of the body's
// volume (of 1 mm3 for a smaller body).
constexpr double kGroupsApartVolume = 1.0e-6;

// Whether a point inside edge `a` (at nine points along it) has its foot on
// edge `b` inside `b` (not at an end of it) within `reach`: the two run side
// by side, their roundings on the faces between them within reach of each
// other. Side by side too where a curve cannot be read.
bool side_by_side(const TopoDS_Edge& a, const TopoDS_Edge& b, double reach) {
  try {
    OCC_CATCH_SIGNALS
    double fa = 0.0, la = 0.0, fb = 0.0, lb = 0.0;
    const Handle(Geom_Curve) ca = BRep_Tool::Curve(a, fa, la);
    const Handle(Geom_Curve) cb = BRep_Tool::Curve(b, fb, lb);
    if (ca.IsNull() || cb.IsNull()) {
      return true;
    }
    const double margin = 1.0e-7 * std::max(1.0, std::abs(lb - fb));
    for (int k = 1; k <= 9; ++k) {
      const gp_Pnt p = ca->Value(fa + (la - fa) * k / 10.0);
      GeomAPI_ProjectPointOnCurve foot(p, cb, fb, lb);
      for (int i = 1; i <= foot.NbPoints(); ++i) {
        const double u = foot.Parameter(i);
        if (u > fb + margin && u < lb - margin && foot.Distance(i) <= reach) {
          return true;
        }
      }
    }
    return false;
  } catch (const Standard_Failure&) {
    return true;
  }
}

// The selected edges (`owner`: a set per edge, -1 for none) in groups whose
// roundings cannot meet (mitcad#2165): edges that share a vertex, or that
// run side by side (side_by_side) within twice `contact` (how far a
// rounding reaches its faces from its edge: two roundings that meet on the
// face between them, or take it away together), are one group, and so are
// the groups they join. Finer than independent_edge_groups (mitcad#2161),
// which joins contours on one face within that reach anywhere: the
// vertical edges of a step's upper face and of its lower face come within
// it on the end face they share only end to end, at the step, where their
// roundings end on the step's ledge. The groups in the order `order` (the
// body's edges, seed_order: as the sets list them) gives their first edges,
// each group's edges in that order.
std::vector<std::vector<int>> rounding_groups(const Shape& body, const std::vector<int>& owner,
                                              const std::vector<int>& order, double contact) {
  std::vector<int> edges;
  const int count = std::min(body.edge_count(), static_cast<int>(owner.size()));
  for (const int e : order) {
    if (e >= 0 && e < count && owner[static_cast<std::size_t>(e)] >= 0 &&
        std::find(edges.begin(), edges.end(), e) == edges.end()) {
      edges.push_back(e);
    }
  }
  std::vector<std::size_t> parent(edges.size());
  for (std::size_t i = 0; i < parent.size(); ++i) {
    parent[i] = i;
  }
  const auto root = [&](std::size_t i) {
    while (parent[i] != i) {
      parent[i] = parent[parent[i]];
      i = parent[i];
    }
    return i;
  };
  const double reach = 2.0 * contact * (1.0 + 1.0e-6) + Precision::Confusion();
  std::vector<Bnd_Box> boxes(edges.size());
  for (std::size_t i = 0; i < edges.size(); ++i) {
    BRepBndLib::Add(body.edge(edges[i]), boxes[i]);
    boxes[i].Enlarge(reach);
  }
  for (std::size_t i = 0; i < edges.size(); ++i) {
    const TopoDS_Edge& a = body.edge(edges[i]);
    TopoDS_Vertex a1, a2;
    TopExp::Vertices(a, a1, a2);
    for (std::size_t j = i + 1; j < edges.size(); ++j) {
      if (root(i) == root(j) || boxes[i].IsOut(boxes[j])) {
        continue;
      }
      throw_if_cancelled();
      const TopoDS_Edge& b = body.edge(edges[j]);
      TopoDS_Vertex b1, b2;
      TopExp::Vertices(b, b1, b2);
      const bool shared = (!a1.IsNull() && (a1.IsSame(b1) || a1.IsSame(b2))) ||
                          (!a2.IsNull() && (a2.IsSame(b1) || a2.IsSame(b2)));
      if (shared || side_by_side(a, b, reach) || side_by_side(b, a, reach)) {
        parent[root(j)] = root(i);
      }
    }
  }
  std::vector<std::vector<int>> groups;
  std::map<std::size_t, std::size_t> group_of;
  for (std::size_t i = 0; i < edges.size(); ++i) {
    const std::size_t r = root(i);
    const auto found = group_of.find(r);
    if (found == group_of.end()) {
      group_of.emplace(r, groups.size());
      groups.push_back({edges[i]});
    } else {
      groups[found->second].push_back(edges[i]);
    }
  }
  return groups;
}

double closed_volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-7, true);
  return props.Mass();
}

// The selected edges (`owner`) built group by group (rounding_groups) at
// their exact size, each group on the result of the ones before, as
// features of a group each build them, where the geometry kernel's build of
// all of them at once has failed and so has the path past narrow faces
// (mitcad#2165: the vertical edges of a step's upper and lower face, each
// pair taking its face away). `exact` builds the edges `owner` of a body at
// their exact size, or throws; the groups in the order `order` gives their
// edges. Kept only where it is the body the build of all at once would be:
// the groups do not change each other's roundings, so that the volume the
// body loses (or gains) in all is the sum of what each group alone takes
// from (or adds to) the body. Null where there are fewer than two groups
// (or more than kMostGroupsApart), a group does not build, or the volumes
// do not add up. With the messages of every group's result.
template <class Exact>
ShapePtr groups_apart(const Shape& body, const std::vector<int>& owner, const std::vector<int>& order,
                      double contact, Exact&& exact) {
  std::vector<std::vector<int>> groups;
  try {
    OCC_CATCH_SIGNALS
    groups = rounding_groups(body, owner, order, contact);
  } catch (const Standard_Failure&) {
    return nullptr;
  }
  if (groups.size() < 2 || groups.size() > kMostGroupsApart) {
    return nullptr;
  }
  const auto own = [&](const std::vector<int>& group) {
    std::vector<int> selected(owner.size(), -1);
    for (const int edge : group) {
      selected[static_cast<std::size_t>(edge)] = owner[static_cast<std::size_t>(edge)];
    }
    return selected;
  };
  try {
    OCC_CATCH_SIGNALS
    const double volume = closed_volume(body.occt());
    double alone = 0.0;
    std::vector<ShapePtr> made;
    for (std::size_t g = 0; g < groups.size(); ++g) {
      const ShapePtr single = exact(body, own(groups[g]));
      if (!single) {
        return nullptr;
      }
      alone += closed_volume(single->occt()) - volume;
      if (g == 0) {
        made.push_back(single);
        continue;
      }
      // (The group's edges on the result so far: every piece of each, as
      // FootCorners' later passes take them.)
      const Shape& on = *made.back();
      std::vector<int> next(static_cast<std::size_t>(on.edge_count()), -1);
      for (const int edge : groups[g]) {
        const std::vector<int> found = pieces_of(body, edge, on);
        if (found.empty()) {
          return nullptr;
        }
        for (const int piece : found) {
          next[static_cast<std::size_t>(piece)] = owner[static_cast<std::size_t>(edge)];
        }
      }
      ShapePtr built = exact(on, next);
      if (!built) {
        return nullptr;
      }
      made.push_back(built);
    }
    ShapePtr result = made.back();
    const double together = closed_volume(result->occt()) - volume;
    if (!(std::abs(together - alone) <= kGroupsApartVolume * std::max(1.0, std::abs(volume)))) {
      return nullptr;
    }
    for (std::size_t k = 0; k + 1 < made.size(); ++k) {
      add_new_messages(*result, *made[k]);
    }
    return result;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    return nullptr;
  }
}

} // namespace

ShapePtr fillet(const std::string& feature, const Shape& given, const std::vector<FilletSet>& sets,
                bool rolling_ball_corners) {
  // Full rounds (mitcad#2279): each set's radius worked out on the body
  // (full_round.hpp), then rounded as a constant set; the middle faces'
  // names go onto the roundings that take their place.
  if (std::any_of(sets.begin(), sets.end(), [](const FilletSet& set) { return set.size == FilletSize::FullRound; })) {
    std::vector<FilletSet> sized = sets;
    std::vector<detail::FullRound> rounds;
    for (FilletSet& set : sized) {
      if (set.size == FilletSize::FullRound) {
        rounds.push_back(detail::full_round(given, set));
        set.size = FilletSize::Constant;
        set.radius = rounds.back().radius;
      }
    }
    return detail::with_full_round_names(fillet(feature, given, sized, rolling_ball_corners), given, feature,
                                         rounds);
  }
  for (const FilletSet& set : sets) {
    check_fillet_set(set);
  }
  detail::require_surfaces(given.occt(), "fillet");
  // (Tangent Chain off on a chain that runs on: the rounding ends against
  // the face beyond the vertex, else runs out over the next edge,
  // mitcad#2037, mitcad#2193.)
  if (ShapePtr partial = over_partial_chains(
          feature, "fillet", given, sets, false,
          [&](const std::vector<FilletSet>& whole) { return fillet(feature, given, whole, rolling_ball_corners); },
          [&](const detail::PartialChain& chain, int seed, int unselected) {
            return fillet_run_out_size(given, sets[static_cast<std::size_t>(chain.set)], seed, unselected);
          })) {
    return partial;
  }
  // (A thread's faces, a turn each, joined where the edges run along them.)
  std::vector<int> pieces;
  const ShapePtr joined = surface_pieces_joined(given, select_edges(given, sets), pieces);
  const Shape& input = *joined;
  const std::vector<int> owner = select_edges(input, sets);
  // How far the roundings reach, for the joined pieces split again.
  const auto reach_of_sets = [&] {
    double reach = 0.0;
    for (const FilletSet& set : sets) {
      reach = std::max({reach, set.radius, set.radius2, set.chord});
      for (const auto& point : set.mid) {
        reach = std::max(reach, point.second);
      }
    }
    return reach;
  };
  // Edges whose faces come tangent inside them, turning from convex to
  // concave there (the rim between a loft's side and a cylinder pulled from
  // its end, mitcad#2300): split where the rounding vanishes, rounded on
  // that body, and named after the input's edges (vanishing_split.hpp).
  // On the body with the pieces of one surface joined: that join also
  // joins the edges between the same two faces that meet at vertices of no
  // other edge (join_surface_pieces), which would put the pieces of a split
  // edge together again (a loft's side with Tangent Edges Keep, pieces of
  // one periodic surface). The joined pieces are split again after it, as
  // below.
  if (!t_vanishing_split) {
    if (std::optional<detail::VanishingSplit> split = detail::split_at_vanishing_points(input, sets)) {
      const detail::UnsplitNames names = detail::unsplit_names(input, *split);
      ShapePtr rounded;
      {
        const VanishingSplitScope scope;
        try {
          rounded = fillet(feature, *split->body, split->sets, rolling_ball_corners);
        } catch (const Cancelled&) {
          throw;
        } catch (const std::invalid_argument& error) {
          throw std::invalid_argument(detail::with_unsplit_names(error.what(), names));
        } catch (const std::runtime_error& error) {
          throw std::runtime_error(detail::with_unsplit_names(error.what(), names));
        }
      }
      rounded = detail::with_unsplit_names(rounded, names, feature);
      if (!rounded || pieces.empty()) {
        return rounded;
      }
      ShapePtr resplit;
      try {
        resplit = detail::split_joined_pieces(*rounded, given, pieces, feature, "fillet", 3.0 * reach_of_sets());
      } catch (const Standard_Failure&) {
        resplit = nullptr;
      }
      return resplit ? resplit : rounded;
    }
  }
  std::vector<char> blended(sets.size(), 0);
  bool any_blended = false;
  for (std::size_t s = 0; s < sets.size(); ++s) {
    blended[s] = sets[s].size == FilletSize::Asymmetric || sets[s].curvature ? 1 : 0;
    any_blended = any_blended || blended[s] != 0;
  }
  // A groove's end rounded as wide as the groove or wider, before the
  // geometry kernel (mitcad#2154).
  bool whole = false;
  ShapePtr result = groove_balls(feature, input, sets, owner, rolling_ball_corners, whole);
  if (whole) {
    return result;
  }
  // A straight edge ending on a flat face of a rounded body smaller than
  // the rounding's section, the rounding wider than the body there (its
  // contact past both faces beside the edge): refused in words before the
  // geometry kernel (refuse_wider_than_the_body, mitcad#2229).
  bool constant = !any_blended;
  for (int e = 0; constant && e < input.edge_count() && e < static_cast<int>(owner.size()); ++e) {
    const int s = owner[static_cast<std::size_t>(e)];
    constant = s < 0 || sets[static_cast<std::size_t>(s)].size == FilletSize::Constant;
  }
  if (!result && constant) {
    const auto radius_of = [&](int e) {
      const int s = e >= 0 && e < static_cast<int>(owner.size()) ? owner[static_cast<std::size_t>(e)] : -1;
      return s < 0 ? 0.0 : sets[static_cast<std::size_t>(s)].radius;
    };
    refuse_wider_than_the_body("fillet", "rounding", input, owner, [&](int e, int) { return radius_of(e); }, true,
                               radius_of, "radius");
  }
  // The edges at their exact size alone, none of the ways round a failure
  // (the rings, the narrow faces continued, the 0.1 % smaller retry), on a
  // first pass over groups of edges the kernel rounds apart (mitcad#2161).
  bool quick = false;
  // Where the geometry kernel cannot round them, a face that the roundings
  // of two of its edges together reach across is named as the cause
  // (narrow_face_cause, mitcad#1482).
  try {
    if (!result) {
      result = canonical(input, with_own_chains("fillet", input, sets, owner,
                                                      [&](const std::vector<int>& selected) {
      const auto build = [&](const Shape& target, const std::vector<int>& own_edges) {
        // A way that leaves a closed body open is a failed try, so that the
        // next way runs (the narrow faces continued, a chain ended on its
        // end plane, the 0.1 % smaller retry, the rings), never the result
        // that ends the fillet (mitcad#2276).
        const bool closed_target = closed_solids(target.occt());
        const auto kept_closed = [closed_target](ShapePtr made) {
          if (closed_target && made && !closed_solids(made->occt())) {
            throw OpenResult("fillet");
          }
          return made;
        };
        const auto rings = [&](const Shape& body) {
          if (quick) {
            throw std::runtime_error("no rings on the first pass");
          }
          return kept_closed(ring_fillet(feature, body, sets, own_edges));
        };
        return or_rings("fillet", target, [&] {
          const auto contact = [&] { return largest_contact(target, sets, own_edges); };
          return with_shrink_retry("fillet", target, contact, [&](double scale) {
            const auto rounded_on = [&](const Shape& on, const std::vector<int>& edges) {
              return dress(on, sets, edges, scale, feature, "fillet", true,
                           [&](const Shape& body, const std::vector<int>& own, double size_scale, bool tight) {
                             if (any_blended) {
                               return round_and_blend(feature, body, sets, own, blended, rolling_ball_corners,
                                                      size_scale, tight);
                             }
                             return round_edges(feature, body, sets, own, rolling_ball_corners, size_scale, tight);
                           });
            };
            const auto rounded = [&] { return rounded_on(target, own_edges); };
            // At the exact size, before the 0.1 % smaller retry; not for
            // setback corners.
            const auto at_size = [&] {
              return scale == 1.0 && !any_blended && rolling_ball_corners
                         ? or_mitred(feature, target, sets, own_edges, rounded)
                         : rounded();
            };
            if (scale != 1.0) {
              if (quick) {
                throw std::runtime_error("no smaller retry on the first pass");
              }
              return kept_closed(at_size());
            }
            try {
              return kept_closed(at_size());
            } catch (const std::invalid_argument&) {
              throw;
            } catch (const Cancelled&) {
              throw;
            } catch (...) {
              if (quick) {
                throw;
              }
              const auto ends = [&](const Shape& body) {
                return groove_ends_rounded(feature, body, sets, own_edges);
              };
              try {
                return kept_closed(past_narrow_faces(feature, target, own_edges, contact(), rounded_on, rings, ends));
              } catch (const std::invalid_argument&) {
                throw;
              } catch (const Cancelled&) {
                throw;
              } catch (...) {
                // Groups of edges whose roundings cannot meet, one group
                // after the other at the exact size, as a feature for each
                // group builds them, where that is the body the build of
                // all at once would be (groups_apart, mitcad#2165); else
                // the error stands (the 0.1 % smaller retry follows).
                const auto exact_on = [&](const Shape& on, const std::vector<int>& edges) -> ShapePtr {
                  const auto rounded_here = [&] { return rounded_on(on, edges); };
                  try {
                    return !any_blended && rolling_ball_corners
                               ? or_mitred(feature, on, sets, edges, rounded_here)
                               : rounded_here();
                  } catch (const std::invalid_argument&) {
                    throw;
                  } catch (const Cancelled&) {
                    throw;
                  } catch (...) {
                    const auto rings_here = [&](const Shape& b) { return ring_fillet(feature, b, sets, edges); };
                    const auto ends_here = [&](const Shape& b) {
                      return groove_ends_rounded(feature, b, sets, edges);
                    };
                    return past_narrow_faces(feature, on, edges, largest_contact(on, sets, edges), rounded_on,
                                             rings_here, ends_here);
                  }
                };
                if (ShapePtr apart = groups_apart(target, own_edges, seed_order(target, sets, own_edges), contact(),
                                                  exact_on)) {
                  if (!closed_target || closed_solids(apart->occt())) {
                    return apart;
                  }
                }
                throw;
              }
            }
          });
        }, rings);
      };
      // Edges in groups the kernel rounds apart (the copies of a pattern's
      // rounding, at its elements: mitcad#2161): where they do not build at
      // their exact size together, each group on its own, one after
      // another, with all the ways round a failure; only where one of them
      // fails, all of them together with those ways, as before. The ways
      // round a failure on all the groups at once (a narrow face continued
      // at one element) took 90 s on a pattern's seven copies, where the
      // copies one by one took 5 s. Contours on one face apart by more than
      // twice the rounding's reach (its size or the farthest its contact
      // lies from its edge) are rounded apart.
      double reach = largest_size(sets);
      try {
        reach = std::max(reach, largest_contact(input, sets, selected));
      } catch (const Cancelled&) {
        throw;
      } catch (...) {
      }
      const std::vector<std::vector<int>> groups = independent_edge_groups(input, selected, 2.0 * reach);
      if (!groups.empty()) {
        quick = true;
        try {
          ShapePtr together = build(input, selected);
          quick = false;
          return together;
        } catch (const Cancelled&) {
          quick = false;
          throw;
        } catch (...) {
          quick = false;
        }
        if (ShapePtr apart = groups_one_by_one(input, groups, build)) {
          return apart;
        }
      }
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
    }
    // (Whichever way it was built.)
    if (result) {
      require_closed(given.occt(), result->occt(), "fillet");
    }
  } catch (const Cancelled&) {
    throw;
  } catch (const std::runtime_error& error) {
    const std::string cause = narrow_face_cause(input, sets, owner);
    if (cause.empty()) {
      throw;
    }
    throw std::runtime_error(with_cause(error.what(), cause));
  }
  // A result whose made faces reach to the wrong side of the body (a folded
  // or spiked corner) gets a warning, never a refusal (mitcad#1253).
  const auto warned = [&](const ShapePtr& rounded) {
    // (Faces of one surface joined, mitcad#1261; pieces of a face it takes
    // away removed, mitcad#1821.)
    const ShapePtr built = without_remnants(detail::join_same_surfaces(rounded), input, owner, chain_follows(sets),
                                            largest_contact(input, sets, owner), feature, "fillet");
    if (built) {
      if (const auto side = detail::dressup_side(input, owner)) {
        detail::DressupFinding warning =
            detail::dressup_check(given, *built, feature, "fillet", *side, largest_size(sets));
        if (!warning.empty()) {
          built->add_note(std::move(warning.text), std::move(warning.places));
        }
      }
    }
    return built;
  };
  // (The joined pieces split again: the body keeps its thread a turn a face,
  // as later operations near it take far longer on faces of many turns.)
  if (!result || pieces.empty()) {
    return warned(result);
  }
  ShapePtr split;
  try {
    split = detail::split_joined_pieces(*result, given, pieces, feature, "fillet", 3.0 * reach_of_sets());
  } catch (const Standard_Failure&) {
    split = nullptr;
  }
  return warned(split ? split : result);
}

ShapePtr fillet(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                double radius) {
  FilletSet set;
  set.edges = edges;
  set.radius = radius;
  return fillet(feature, body, {set});
}

namespace detail {
// The chamfer as the geometry kernel builds it, its errors as they come.
ShapePtr chamfer_unexplained(const std::string& feature, const Shape& input, const std::vector<ChamferSet>& sets,
                             ChamferCorner corner);
} // namespace detail

namespace {

// Whether a chamfer built past a narrow face whose far edge its contact
// touches (detail::overflow_tangent) is sound, the face beside it ending
// in a point there (mitcad#1613): one valid closed solid, and the check of
// mitcad#1253 finds nothing of the chamfer hanging out past the body or
// turned over on its surface (mitcad#1260's silent broken result).
bool sound_at_tangent(const std::string& feature, const Shape& input, const Shape& result,
                      const std::vector<int>& owner, double size) {
  int solids = 0;
  for (TopExp_Explorer ex(result.occt(), TopAbs_SOLID); ex.More(); ex.Next()) {
    ++solids;
  }
  if (solids != 1 || !closed_solids(result.occt()) || !BRepCheck_Analyzer(result.occt()).IsValid()) {
    return false;
  }
  const std::optional<detail::DressupSide> side = detail::dressup_side(input, owner);
  return !side || detail::dressup_check(input, result, feature, "chamfer", *side, size).empty();
}

// Up to three names of a list, then how many more.
std::string some_names(const std::vector<std::string>& names) {
  std::string text;
  for (std::size_t i = 0; i < names.size() && i < 3; ++i) {
    text += (i == 0 ? "" : ", ") + names[i];
  }
  if (names.size() > 3) {
    text += " and " + std::to_string(names.size() - 3) + " more";
  }
  return text;
}

// What a chamfer the geometry kernel failed to build was asked for, for
// its error (mitcad#1185): the selected edges, and the faces the chains of
// them (with the tangent chains the sets follow) end against: those at a
// chain's end vertex beside its last edge's own two faces, where a bevel
// has to stop. "edge E{…}, its chain ending against face F3:chamfer(…)".
std::string chamfer_context(const Shape& body, const std::vector<ChamferSet>& sets, const std::vector<int>& owner) {
  std::vector<std::string> selected;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] >= 0) {
      const std::string& name = body.edge_name(edge);
      selected.push_back(name.empty() ? "an unnamed edge" : name);
    }
  }
  std::string text = (selected.size() == 1 ? "edge " : std::to_string(selected.size()) + " edges (") +
                     some_names(selected) + (selected.size() == 1 ? "" : ")");
  std::vector<int> chain = owner;
  std::vector<char> follows;
  for (const ChamferSet& set : sets) {
    follows.push_back(follows_own_chain(set) ? 1 : 0);
  }
  try {
    OCC_CATCH_SIGNALS
    detail::follow_tangent_chains(body, chain, follows);
  } catch (const Standard_Failure&) {
    chain = owner;
  }
  using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;
  Ancestors edges_at;
  Ancestors faces_at;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_at);
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_FACE, faces_at);
  std::set<std::string> seen;
  std::vector<std::string> against;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (chain[static_cast<std::size_t>(edge)] < 0) {
      continue;
    }
    const std::array<int, 2> own = body.edge_faces(edge);
    const TopoDS_Edge& current = TopoDS::Edge(body.edge(edge));
    if (TopExp::FirstVertex(current).IsSame(TopExp::LastVertex(current))) {
      continue; // (a closed edge has no end)
    }
    for (const TopoDS_Vertex& vertex : {TopExp::FirstVertex(current), TopExp::LastVertex(current)}) {
      const int iv = edges_at.FindIndex(vertex);
      if (vertex.IsNull() || iv == 0) {
        continue;
      }
      int chained = 0;
      for (const TopoDS_Shape& other : edges_at.FindFromIndex(iv)) {
        const int index = body.edge_index(other);
        chained += index >= 0 && chain[static_cast<std::size_t>(index)] >= 0 ? 1 : 0;
      }
      if (chained != 1) {
        continue;
      }
      const int jv = faces_at.FindIndex(vertex);
      if (jv == 0) {
        continue;
      }
      for (const TopoDS_Shape& face : faces_at.FindFromIndex(jv)) {
        const int index = body.face_index(face);
        if (index < 0 || index == own[0] || index == own[1] || body.face_names(index).empty()) {
          continue;
        }
        const std::string& name = body.face_names(index).front();
        if (seen.insert(name).second) {
          against.push_back(name);
        }
      }
    }
  }
  if (!against.empty()) {
    text += std::string(selected.size() == 1 ? ", its chain" : ", their chains") + " ending against " +
            (against.size() == 1 ? "face " : "faces ") + some_names(against);
  }
  return text;
}

} // namespace

// A chamfer that the geometry kernel failed to build: its error says what
// was asked (chamfer_context), the kernel's own message kept as the detail.
// The message keeps its start ("chamfer: …", "chamfer failed …"), by which
// the .f3d import tells a failure of the kernel (kernel_failure).
ShapePtr chamfer(const std::string& feature, const Shape& input, const std::vector<ChamferSet>& sets,
                 ChamferCorner corner) {
  // (Tangent Chain off on a chain that runs on: the bevel ends against the
  // face beyond the vertex, else runs out over the next edge, mitcad#2037,
  // mitcad#2193.)
  if (ShapePtr partial = over_partial_chains(
          feature, "chamfer", input, sets, true,
          [&](const std::vector<ChamferSet>& whole) { return chamfer(feature, input, whole, corner); },
          [&](const detail::PartialChain& chain, int, int) {
            return chamfer_run_out_size(input, sets[static_cast<std::size_t>(chain.set)], chain);
          })) {
    return partial;
  }
  try {
    // (Faces of one surface joined, mitcad#1261.)
    // (Pieces of a face it takes away removed, mitcad#1821.)
    const ShapePtr result =
        without_remnants(detail::join_same_surfaces(detail::chamfer_unexplained(feature, input, sets, corner)),
                         input, select_edges(input, sets), chain_follows(sets), largest_size(sets), feature,
                         "chamfer");
    // A result whose made faces reach to the wrong side of the body gets a
    // warning, never a refusal (mitcad#1253).
    if (const auto side = detail::dressup_side(input, select_edges(input, sets))) {
      detail::DressupFinding warning =
          detail::dressup_check(input, *result, feature, "chamfer", *side, largest_size(sets));
      if (!warning.empty()) {
        result->add_note(std::move(warning.text), std::move(warning.places));
      }
    }
    return result;
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (const std::runtime_error& error) {
    const std::string message = error.what();
    if (message.find("out of memory") != std::string::npos) {
      throw;
    }
    std::string context;
    // The selected edges painted; the context names them for agents only
    // (mitcad#1612).
    std::vector<ErrorPlace> places;
    try {
      OCC_CATCH_SIGNALS
      const std::vector<int> owner = select_edges(input, sets);
      context = chamfer_context(input, sets, owner);
      for (int edge = 0; edge < input.edge_count() && edge < static_cast<int>(owner.size()); ++edge) {
        if (owner[static_cast<std::size_t>(edge)] >= 0) {
          places.push_back(edge_place(input.edge_name(edge)));
        }
      }
    } catch (...) {
      throw error;
    }
    const std::string raw = "chamfer: ";
    // (A kernel's failure that already says where keeps its places.)
    if (message.compare(0, raw.size(), raw) == 0 && message.find(kPlacedTail) == std::string::npos) {
      throw std::runtime_error(placed_error(raw + "the geometry kernel could not build the bevel of the selected edges",
                                            raw + "the geometry kernel could not build the bevel of " + context +
                                                " (kernel error: " + message.substr(raw.size()) + ")",
                                            places));
    }
    throw std::runtime_error(with_detail(message, " (" + context + ")", places));
  }
}

namespace detail {

namespace {
// Set while a failed chamfer tries the other corner types for its error,
// so that those tries do not try again.
thread_local bool trying_other_corners = false;
} // namespace

ShapePtr chamfer_unexplained(const std::string& feature, const Shape& input, const std::vector<ChamferSet>& sets,
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
  // A straight edge ending on a flat face of a rounded body smaller than
  // the bevel's section, the bevel wider than the body there (its contact
  // past both faces beside the edge): refused in words before the geometry
  // kernel (refuse_wider_than_the_body, mitcad#2229).
  {
    const auto set_of = [&](int e) -> const ChamferSet* {
      const int s = e >= 0 && e < static_cast<int>(owner.size()) ? owner[static_cast<std::size_t>(e)] : -1;
      return s < 0 ? nullptr : &sets[static_cast<std::size_t>(s)];
    };
    // (Each leg on its own face, as the chamfer measures it.)
    const detail::SectionReach leg_on = [&](int e, int f) {
      const ChamferSet* set = set_of(e);
      if (set == nullptr) {
        return 0.0;
      }
      const ChamferSpec& spec = set->spec;
      if (spec.type == ChamferType::EqualDistance) {
        return spec.distance;
      }
      const bool measured = measured_face(input, e, spec.reference_face, spec.flip) == f;
      if (spec.type == ChamferType::TwoDistances) {
        return measured ? spec.distance : spec.distance2;
      }
      return measured ? spec.distance : spec.distance * std::tan(spec.angle);
    };
    refuse_wider_than_the_body(
        "chamfer", "bevel", input, owner, leg_on, false,
        [&](int e) {
          const ChamferSet* set = set_of(e);
          return set == nullptr ? 0.0 : set->spec.distance;
        },
        "distance");
  }
  const ShapePtr result =
      canonical(input, with_own_chains("chamfer", input, sets, owner, [&](const std::vector<int>& selected) {
    const auto rings = [&](const Shape& body) { return ring_chamfer(feature, body, sets, selected); };
    return or_rings("chamfer", input, [&] {
      // A chamfer's distances are along its faces.
      const auto contact = [&] { return largest_size(sets); };
      return with_shrink_retry("chamfer", input, contact, [&](double scale) {
        const auto bevelled = [&](const Shape& target, const std::vector<int>& own_edges) {
          return dress(target, sets, own_edges, scale, feature, "chamfer", false,
                       [&](const Shape& body, const std::vector<int>& own, double size_scale, bool) {
                         return bevel(feature, body, sets, own, corner, size_scale);
                       });
        };
        // How far the chamfer reaches along each face of an edge: its leg
        // on that face, not the longer of its two (mitcad#1819: a face
        // narrower than a distance-and-angle chamfer's leg on the other face
        // only is no narrow face, and its chamfer takes no path past it).
        const detail::OverflowReach leg = [&](const Shape& body, int s, int e, int f) {
          if (s < 0 || s >= static_cast<int>(sets.size())) {
            return 0.0;
          }
          const ChamferSpec& spec = sets[static_cast<std::size_t>(s)].spec;
          if (spec.type == ChamferType::EqualDistance) {
            return spec.distance;
          }
          const bool measured = measured_face(body, e, spec.reference_face, spec.flip) == f;
          if (spec.type == ChamferType::TwoDistances) {
            return measured ? spec.distance : spec.distance2;
          }
          return measured ? spec.distance : spec.distance * std::tan(spec.angle);
        };
        // Where the chamfer is wider than a face beside its edges, never
        // smaller than asked (mitcad#1232): no 0.1 % smaller retry.
        const auto overflows = [&] {
          return detail::overflow_failure(*detail::working_copy(input), selected, largest_size(sets), "chamfer",
                                          &leg);
        };
        if (scale != 1.0) {
          if (!overflows().empty()) {
            throw std::runtime_error("the chamfer is wider than a face beside its edges");
          }
          return bevelled(input, selected);
        }
        // (A bevel folded out of the body, folded_out: the other tries
        // first; where none builds, it stands with the warning of
        // mitcad#1253.)
        ShapePtr folded;
        try {
          ShapePtr made = bevelled(input, selected);
          if (!made || !folded_out(input, selected, *made)) {
            return made;
          }
          folded = made;
          throw std::runtime_error("the chamfer folds out of the body");
        } catch (const std::invalid_argument&) {
          throw;
        } catch (const Cancelled&) {
          throw;
        } catch (...) {
          try {
            // At the exact size, before the 0.1 % smaller retry: a chamfer
            // wider than a narrow face beside its edges goes on past the
            // face's far edge, cut by the faces beyond (mitcad#1232). Where
            // it reaches exactly to that edge, its contact touching it, the
            // face beside it ends in a point there (mitcad#1260): valid where
            // the result is one valid closed solid that nothing of the
            // chamfer hangs out of or turns over in (the check of
            // mitcad#1253 finds nothing), else refused (mitcad#1613).
            std::string tangent;
            {
              const ShapePtr copy = detail::working_copy(input);
              tangent = detail::overflow_tangent(*copy, selected, [&](int e, int f) {
                const int s = selected[static_cast<std::size_t>(e)];
                if (s < 0) {
                  return 0.0;
                }
                const ChamferSpec& spec = sets[static_cast<std::size_t>(s)].spec;
                if (spec.type == ChamferType::EqualDistance) {
                  return spec.distance;
                }
                const bool measured = measured_face(*copy, e, spec.reference_face, spec.flip) == f;
                if (spec.type == ChamferType::TwoDistances) {
                  return measured ? spec.distance : spec.distance2;
                }
                return measured ? spec.distance : spec.distance * std::tan(spec.angle);
              });
            }
            std::string why;
            bool freeform = false;
            // (On the body with the narrow faces continued, a chain ending
            // against a plane that does not reach across the bevel is ended
            // on it, ended_on_end_planes.)
            const auto bevelled_or_ended = [&](const Shape& target, const std::vector<int>& own_edges) -> ShapePtr {
              try {
                ShapePtr made = bevelled(target, own_edges);
                if (made && folded_out(target, own_edges, *made)) {
                  throw std::runtime_error("the chamfer folds out of the body");
                }
                return made;
              } catch (const std::invalid_argument&) {
                throw;
              } catch (const Cancelled&) {
                throw;
              } catch (...) {
                if (ShapePtr ended = ended_on_end_planes(target, own_edges, largest_size(sets),
                                                         feature + ":chamfer(", bevelled)) {
                  return ended;
                }
                throw;
              }
            };
            if (ShapePtr over = over_narrow_faces(feature, "chamfer", input, selected, largest_size(sets),
                                                  steepness(sets), bevelled_or_ended, &why, !tangent.empty(), &leg,
                                                  &freeform)) {
              if (tangent.empty() || sound_at_tangent(feature, input, *over, selected, largest_size(sets))) {
                // A strip of a narrow face narrower than 0.1 % of the chamfer
                // left beside it, where the chamfer is that little narrower
                // than the face (mitcad#1783), taken away with the face, as
                // after the 0.1 % smaller retry.
                if (ShapePtr whole = detail::without_strips(*over, input, largest_size(sets))) {
                  return whole;
                }
                return over;
              }
            }
            // A chain ending against a plane that does not reach across the
            // bevel there: ended on that plane (ended_on_end_planes, GitHub
            // #353), the bevel on the body without the material past it
            // built as above.
            if (tangent.empty()) {
              const auto at_size_on = [&](const Shape& body, const std::vector<int>& own) -> ShapePtr {
                try {
                  ShapePtr made = bevelled(body, own);
                  if (made && !folded_out(body, own, *made)) {
                    return made;
                  }
                } catch (const std::invalid_argument&) {
                  throw;
                } catch (const Cancelled&) {
                  throw;
                } catch (const std::exception&) {
                }
                if (ShapePtr past = over_narrow_faces(feature, "chamfer", body, own, largest_size(sets),
                                                      steepness(sets), bevelled, nullptr, false, &leg)) {
                  if (ShapePtr whole = detail::without_strips(*past, body, largest_size(sets))) {
                    return whole;
                  }
                  return past;
                }
                return nullptr;
              };
              if (ShapePtr ended =
                      ended_on_end_planes(input, selected, largest_size(sets), feature + ":chamfer(", at_size_on)) {
                return ended;
              }
            }
            if (!tangent.empty()) {
              throw std::runtime_error(tangent);
            }
            // Else the error says where, and at a corner why the chamfer past
            // the narrow face failed there and which corner types build it
            // (mitcad#1238).
            // (Where a face beside the edges is a free-form B-spline surface,
            // the words say that it cannot be continued past its far edge,
            // mitcad#1849.)
            const std::string where =
                freeform ? detail::overflow_failure(*detail::working_copy(input), selected, largest_size(sets),
                                                    "chamfer", &leg, detail::kFreeformCause)
                         : overflows();
            if (where.empty()) {
              throw;
            }
            std::string at_corner;
            if (!why.empty()) {
              at_corner = "; past the narrow face the corner fails: " + why;
              std::vector<std::string> working;
              if (!trying_other_corners) {
                struct Guard {
                  Guard() { trying_other_corners = true; }
                  ~Guard() { trying_other_corners = false; }
                  Guard(const Guard&) = delete;
                  Guard& operator=(const Guard&) = delete;
                } guard;
                for (const ChamferCorner other : {ChamferCorner::Chamfer, ChamferCorner::Miter, ChamferCorner::Blend}) {
                  if (other == corner) {
                    continue;
                  }
                  try {
                    chamfer_unexplained(feature, input, sets, other);
                    working.emplace_back(corner_name(other));
                  } catch (const Cancelled&) {
                    throw;
                  } catch (...) {
                  }
                }
              }
              if (!working.empty()) {
                at_corner += "; the corner type " + working.front() +
                             (working.size() > 1 ? " or " + working.back() : std::string()) + " builds it";
              }
            }
            try {
              throw;
            } catch (const OpenResult&) {
              // The kernel left a gap where the chamfer takes the narrow face
              // away: the words say which face and why it cannot, never that
              // the result is not a closed solid (mitcad#1819).
              throw std::runtime_error(where + at_corner);
            } catch (const std::exception& error) {
              // (OCCT's failures, std::exceptions too, as "chamfer: …".)
              const bool kernel = dynamic_cast<const Standard_Failure*>(&error) != nullptr;
              throw std::runtime_error(worded_failure((kernel ? "chamfer: " : "") + std::string(error.what()), where) +
                                       at_corner);
            }
          } catch (const Cancelled&) {
            throw;
          } catch (...) {
            if (folded) {
              return folded;
            }
            throw;
          }
        }
      });
    }, rings);
  }));
  // (Whichever way it was built.)
  require_closed(input.occt(), result->occt(), "chamfer");
  return result;
}

} // namespace detail

ShapePtr chamfer(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                 const ChamferSpec& spec) {
  ChamferSet set;
  set.edges = edges;
  set.spec = spec;
  return chamfer(feature, body, {set});
}

} // namespace mitcad::geometry
