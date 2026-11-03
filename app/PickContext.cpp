// SPDX-License-Identifier: MIT
#include "PickContext.hpp"
#include "view/ViewTessellation.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <functional>
#include <iterator>
#include <limits>
#include <utility>
#include <vector>

#include <SelectMgr_SelectingVolumeManager.hxx>
#include <SelectMgr_SelectionManager.hxx>
#include <SelectMgr_SortCriterion.hxx>
#include <StdSelect_BRepOwner.hxx>
#include <StdSelect_ViewerSelector3d.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Vec.hxx>

namespace mitcad {
namespace {

// The shape a body's face, edge or vertex owner stands for, or none.
const TopoDS_Shape* ownedShape(const occ::handle<SelectMgr_EntityOwner>& owner) {
  const auto brep = occ::handle<StdSelect_BRepOwner>::DownCast(owner);
  if (brep.IsNull() || !brep->HasShape()) {
    return nullptr;
  }
  return &brep->Shape();
}

// 0 for a vertex, 1 for an edge, 2 for a face, -1 for anything else.
int dimensionOf(const TopoDS_Shape& shape) {
  switch (shape.ShapeType()) {
  case TopAbs_VERTEX:
    return 0;
  case TopAbs_EDGE:
    return 1;
  case TopAbs_FACE:
    return 2;
  default:
    return -1;
  }
}

// With two edges of a face in reach, the nearer comes first only when it is
// at most this part of the other's distance from the cursor.
constexpr double kNearerEdge = 0.5;

bool bounds(const TopoDS_Shape& part, const TopoDS_Shape& whole) {
  for (TopExp_Explorer explorer(whole, part.ShapeType()); explorer.More(); explorer.Next()) {
    if (explorer.Current().IsSame(part)) {
      return true;
    }
  }
  return false;
}

class EdgeFirstSelector : public StdSelect_ViewerSelector3d {
  DEFINE_STANDARD_RTTI_INLINE(EdgeFirstSelector, StdSelect_ViewerSelector3d)

public:
  void SortResult() const override {
    StdSelect_ViewerSelector3d::SortResult();
    m_prioritized = false;
    m_movedAhead = false;
    // Only a click or the hover has one item to choose; a window takes all.
    // (With nothing picked, the indexes are left over from the last pick.)
    if (mystored.Extent() < 2 ||
        mySelectingVolumeMgr.GetActiveSelectionType() != SelectMgr_SelectionType_Point) {
      return;
    }
    coveredFirst();
    yieldingBehind();
    narrowFaceFirst();
    lastBehind();
    edgesFirst();
    aheadOfAreas();
    preferredFirst();
  }

  // setPickPriority(), pickPrioritized().
  void setPreferred(std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> preferred) {
    m_preferred = std::move(preferred);
  }
  bool prioritized() const { return m_prioritized; }
  // setPickLast().
  void setLast(std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> last) {
    m_last = std::move(last);
  }
  void setSources(PickSourceResolver resolve) { m_sources = std::move(resolve); }
  void setCovers(PickCoverResolver resolve) { m_covers = std::move(resolve); }
  // setPickYield().
  void setYield(std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> yields,
                std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> over) {
    m_yields = std::move(yields);
    m_over = std::move(over);
  }
  // setPickAhead(), pickMovedAhead().
  void setAhead(std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> ahead,
                std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> passed) {
    m_ahead = std::move(ahead);
    m_passed = std::move(passed);
  }
  bool movedAhead() const { return m_movedAhead; }

private:
  // The owners `over` behind the first yielding one (a sketch point on a
  // body's corner, mitcad#869) moved in front of it, in their order; the
  // rest after them keep theirs.
  void yieldingBehind() const {
    if (m_yields && m_over) {
      moveInFront(m_yields, m_over);
    }
  }

  // The sketch regions on offer behind the first face, body or plane under
  // the cursor (a region inside a body, mitcad#1222) moved in front of it.
  void aheadOfAreas() const {
    if (m_ahead && m_passed) {
      m_movedAhead = moveInFront(m_passed, m_ahead);
    }
  }

  // The owners `over` behind the first owner that `yields` moved in front
  // of it, in their order; the rest after them keep theirs. Whether any
  // moved.
  bool moveInFront(const std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)>& yields,
                   const std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)>& over) const {
    std::vector<int> order;
    for (int rank = myIndexes.Lower(); rank <= myIndexes.Upper(); ++rank) {
      order.push_back(myIndexes(rank));
    }
    const auto first = std::find_if(order.begin(), order.end(),
                                    [this, &yields](const int index) { return yields(mystored.FindKey(index)); });
    if (first == order.end()) {
      return false;
    }
    std::vector<int> moved;
    std::vector<int> rest;
    for (auto it = first; it != order.end(); ++it) {
      (over(mystored.FindKey(*it)) ? moved : rest).push_back(*it);
    }
    if (moved.empty()) {
      return false;
    }
    int rank = myIndexes.Lower() + static_cast<int>(std::distance(order.begin(), first));
    for (const int index : moved) {
      myIndexes(rank++) = index;
    }
    for (const int index : rest) {
      myIndexes(rank++) = index;
    }
    return true;
  }

  // The owners put last (construction planes while a placement input takes
  // picks, mitcad#656) behind the others, each group in its order.
  void lastBehind() const {
    if (!m_last) {
      return;
    }
    std::vector<int> front;
    std::vector<int> behind;
    for (int rank = myIndexes.Lower(); rank <= myIndexes.Upper(); ++rank) {
      const int index = myIndexes(rank);
      (m_last(mystored.FindKey(index)) ? behind : front).push_back(index);
    }
    if (front.empty() || behind.empty()) {
      return;
    }
    int rank = myIndexes.Lower();
    for (const int index : front) {
      myIndexes(rank++) = index;
    }
    for (const int index : behind) {
      myIndexes(rank++) = index;
    }
  }

  // A selection overlay before the previewed body it covers (setPickCovers),
  // where it lies no farther than that body's first hit; the moved hits keep
  // their order. The rules below then rank its edges before its face.
  void coveredFirst() const {
    if (!m_covers) {
      return;
    }
    std::vector<int> order;
    for (int rank = myIndexes.Lower(); rank <= myIndexes.Upper(); ++rank) {
      order.push_back(myIndexes(rank));
    }
    bool moved = false;
    for (std::size_t i = 1; i < order.size(); ++i) {
      const SelectMgr_SelectableObject* covered = m_covers(mystored.FindKey(order[i]));
      if (covered == nullptr) {
        continue;
      }
      const SelectMgr_SortCriterion& hit = mystored.FindFromIndex(order[i]);
      for (std::size_t j = 0; j < i; ++j) {
        if (mystored.FindKey(order[j])->Selectable().get() != covered) {
          continue;
        }
        const SelectMgr_SortCriterion& bodyHit = mystored.FindFromIndex(order[j]);
        if (hit.ZLayerPosition >= bodyHit.ZLayerPosition &&
            hit.Depth - bodyHit.Depth <= hit.Tolerance + bodyHit.Tolerance) {
          const auto from = order.begin() + static_cast<std::ptrdiff_t>(j);
          const auto at = order.begin() + static_cast<std::ptrdiff_t>(i);
          std::rotate(from, at, std::next(at));
          moved = true;
        }
        break;
      }
    }
    if (moved) {
      int rank = myIndexes.Lower();
      for (const int index : order) {
        myIndexes(rank++) = index;
      }
    }
  }

  // The preferred owners (the active component's, mitcad#420) before the
  // others, each in their order; m_prioritized when one went before a
  // nearer other one.
  void preferredFirst() const {
    if (!m_preferred) {
      return;
    }
    std::vector<int> preferred;
    std::vector<int> others;
    for (int rank = myIndexes.Lower(); rank <= myIndexes.Upper(); ++rank) {
      const int index = myIndexes(rank);
      (m_preferred(mystored.FindKey(index)) ? preferred : others).push_back(index);
    }
    if (preferred.empty() || others.empty() || myIndexes(myIndexes.Lower()) == preferred.front()) {
      return;
    }
    int rank = myIndexes.Lower();
    for (const int index : preferred) {
      myIndexes(rank++) = index;
    }
    for (const int index : others) {
      myIndexes(rank++) = index;
    }
    m_prioritized = true;
  }

  const SelectMgr_SelectableObject* source(const occ::handle<SelectMgr_EntityOwner>& owner) const {
    return m_sources ? m_sources(owner) : owner->Selectable().get();
  }

  double cursorOffset(const SelectMgr_SortCriterion& hit) const {
    const gp_Vec toHit(mySelectingVolumeMgr.GetNearPickedPnt(), hit.Point);
    const double distance = toHit.Crossed(gp_Vec(mySelectingVolumeMgr.GetViewRayDirection())).Magnitude();
    return hit.Tolerance > 0.0 ? distance / hit.Tolerance : distance;
  }

  bool edgeOn(const SelectMgr_SortCriterion& hit) const {
    const gp_Vec normal(hit.Normal.x(), hit.Normal.y(), hit.Normal.z());
    return normal.Magnitude() > 0.0 &&
        std::abs(normal.Dot(gp_Vec(mySelectingVolumeMgr.GetViewRayDirection()))) <= 1e-6 * normal.Magnitude();
  }

  void narrowFaceFirst() const {
    const int first = myIndexes.Lower();
    const int nearestIndex = myIndexes(first);
    const auto& nearest = mystored.FindKey(nearestIndex);
    const TopoDS_Shape* nearestShape = ownedShape(nearest);
    if (!nearestShape) { return; }
    const auto& edgeHit = mystored.FindFromIndex(nearestIndex);
    const bool nearestEdge = nearestShape->ShapeType() == TopAbs_EDGE;
    if (!nearestEdge && (nearestShape->ShapeType() != TopAbs_FACE || !edgeOn(edgeHit))) { return; }
    const auto* nearestSource = source(nearest);
    int bestRank = -1;
    double bestOffset = std::numeric_limits<double>::infinity();
    // On a flat strip OCCT can put an edge first even at the face's
    // centre. Apply the same opposing-edge rule before promoting edges;
    // a face below a selected edge's layer must not take its removal click.
    for (int faceRank = first + 1; faceRank <= myIndexes.Upper(); ++faceRank) {
      const int faceIndex = myIndexes(faceRank);
      const auto& faceOwner = mystored.FindKey(faceIndex);
      const TopoDS_Shape* face = ownedShape(faceOwner);
      const auto& faceHit = mystored.FindFromIndex(faceIndex);
      if (!face || face->ShapeType() != TopAbs_FACE || source(faceOwner) != nearestSource ||
          faceHit.ZLayerPosition < edgeHit.ZLayerPosition || edgeOn(faceHit) ||
          (nearestEdge && (!bounds(*nearestShape, *face) || !onSurface(edgeHit, faceHit)))) { continue; }
      std::vector<std::pair<TopoDS_Shape, double>> edges;
      bool sharesBoundary = nearestEdge;
      for (int rank = first; rank <= myIndexes.Upper(); ++rank) {
        const int index = myIndexes(rank);
        const auto& owner = mystored.FindKey(index);
        const TopoDS_Shape* shape = ownedShape(owner);
        const auto& hit = mystored.FindFromIndex(index);
        if (!shape || shape->ShapeType() != TopAbs_EDGE || source(owner) != nearestSource ||
            !bounds(*shape, *face) || !onSurface(hit, faceHit) ||
            std::any_of(edges.begin(), edges.end(), [&](const auto& edge) { return edge.first.IsSame(*shape); })) {
          continue;
        }
        sharesBoundary = sharesBoundary || bounds(*shape, *nearestShape);
        edges.emplace_back(*shape, cursorOffset(hit));
      }
      std::sort(edges.begin(), edges.end(), [](const auto& a, const auto& b) { return a.second < b.second; });
      if (!sharesBoundary || edges.size() < 2 || edges[0].second <= kNearerEdge * edges[1].second) { continue; }
      // An edge-on side face may also lie within its pick tolerance.
      // Prefer the face actually under the cursor over that margin hit.
      const double offset = cursorOffset(faceHit);
      if (offset < bestOffset) { bestOffset = offset; bestRank = faceRank; }
    }
    if (bestRank >= 0) {
      const int faceIndex = myIndexes(bestRank);
      for (int rank = bestRank; rank > first; --rank) { myIndexes(rank) = myIndexes(rank - 1); }
      myIndexes(first) = faceIndex;
    }
  }

  void edgesFirst() const {
    const int first = myIndexes.Lower();
    const int nearestIndex = myIndexes(first);
    const occ::handle<SelectMgr_EntityOwner>& nearest = mystored.FindKey(nearestIndex);
    const TopoDS_Shape* nearestShape = ownedShape(nearest);
    const int nearestDimension = nearestShape != nullptr ? dimensionOf(*nearestShape) : -1;
    const auto* nearestSource = source(nearest);
    if (nearestDimension < 1) {
      return; // not a body's face or edge
    }
    const SelectMgr_SortCriterion& hit = mystored.FindFromIndex(nearestIndex);
    struct Candidate {
      int index;     // into mystored
      int dimension; // vertices before edges
      double offset; // from the cursor, in the entity's pick widths
      TopoDS_Shape shape;
    };
    std::vector<Candidate> promoted;
    std::vector<int> rest;
    for (int rank = first + 1; rank <= myIndexes.Upper(); ++rank) {
      const int index = myIndexes(rank);
      const occ::handle<SelectMgr_EntityOwner>& owner = mystored.FindKey(index);
      const TopoDS_Shape* shape = ownedShape(owner);
      const int dimension = shape != nullptr ? dimensionOf(*shape) : -1;
      const SelectMgr_SortCriterion& data = mystored.FindFromIndex(index);
      if (dimension < 0 || dimension >= nearestDimension || source(owner) != nearestSource ||
          !bounds(*shape, *nearestShape) || !onSurface(data, hit)) {
        rest.push_back(index);
        continue;
      }
      // The same edge may be hit through both its overlay and the body.
      // Count it once, including for the narrow-face rule below.
      if (std::any_of(promoted.begin(), promoted.end(), [&](const Candidate& other) {
            return shape->IsSame(other.shape);
          })) {
        rest.push_back(index);
        continue;
      }
      promoted.push_back({index, dimension, cursorOffset(data),
                          *shape});
    }
    std::stable_sort(promoted.begin(), promoted.end(), [](const Candidate& a, const Candidate& b) {
      return a.dimension != b.dimension ? a.dimension < b.dimension : a.offset < b.offset;
    });
    // A face narrower on screen than two edges' pick widths (a fillet's
    // strip) has an edge in reach on each side: the edges come first only
    // where the cursor is clearly nearer one of them, so that the strip's
    // middle still picks the face.
    const auto edges = std::find_if(promoted.begin(), promoted.end(),
                                    [](const Candidate& candidate) { return candidate.dimension == 1; });
    if (nearestDimension == 2 && std::distance(edges, promoted.end()) >= 2 &&
        edges->offset > kNearerEdge * std::next(edges)->offset) {
      promoted.erase(edges, promoted.end());
      rest.clear();
      for (int rank = first + 1; rank <= myIndexes.Upper(); ++rank) {
        const int index = myIndexes(rank);
        if (std::none_of(promoted.begin(), promoted.end(),
                         [index](const Candidate& candidate) { return candidate.index == index; })) {
          rest.push_back(index);
        }
      }
    }
    if (promoted.empty()) {
      return;
    }
    int rank = first;
    for (const Candidate& candidate : promoted) {
      myIndexes(rank++) = candidate.index;
    }
    myIndexes(rank++) = nearestIndex;
    for (const int index : rest) {
      myIndexes(rank++) = index;
    }
  }

  PickSourceResolver m_sources;
  PickCoverResolver m_covers;
  std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> m_preferred;
  std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> m_last;
  std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> m_yields;
  std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> m_over;
  std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> m_ahead;
  std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)> m_passed;
  mutable bool m_prioritized = false;
  mutable bool m_movedAhead = false;

  // Whether an edge or vertex picked with a face lies on the face's surface
  // where the face was hit (within both pick widths), not behind it: the
  // distance from the tangent plane there. Without the face's normal, the
  // depths are compared instead.
  static bool onSurface(const SelectMgr_SortCriterion& part, const SelectMgr_SortCriterion& face) {
    const double tolerance = part.Tolerance + face.Tolerance;
    if (face.Normal.Modulus() <= 0.0f) {
      return part.Depth - face.Depth <= tolerance;
    }
    const gp_Vec normal(face.Normal.x(), face.Normal.y(), face.Normal.z());
    return std::abs(gp_Vec(face.Point, part.Point).Dot(normal)) <= tolerance;
  }
};

class PickContext : public AIS_InteractiveContext {
  DEFINE_STANDARD_RTTI_INLINE(PickContext, AIS_InteractiveContext)

public:
  explicit PickContext(const occ::handle<V3d_Viewer>& viewer) : AIS_InteractiveContext(viewer) {
    setViewTessellation(DefaultDrawer());
    // Nothing is displayed yet: the selection manager can still change.
    mgrSelector = new SelectMgr_SelectionManager(new EdgeFirstSelector());
  }
};

} // namespace

occ::handle<AIS_InteractiveContext> makePickContext(const occ::handle<V3d_Viewer>& viewer) {
  return new PickContext(viewer);
}

void setPickSources(const occ::handle<AIS_InteractiveContext>& context, PickSourceResolver resolve) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  if (!selector.IsNull()) { selector->setSources(std::move(resolve)); }
}

void setPickCovers(const occ::handle<AIS_InteractiveContext>& context, PickCoverResolver resolve) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  if (!selector.IsNull()) {
    selector->setCovers(std::move(resolve));
  }
}

void setPickPriority(const occ::handle<AIS_InteractiveContext>& context, PickPreference preferred) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  if (!selector.IsNull()) {
    selector->setPreferred(std::move(preferred));
  }
}

void setPickLast(const occ::handle<AIS_InteractiveContext>& context, PickPreference last) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  if (!selector.IsNull()) {
    selector->setLast(std::move(last));
  }
}

void setPickYield(const occ::handle<AIS_InteractiveContext>& context, PickPreference yields,
                  PickPreference over) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  if (!selector.IsNull()) {
    selector->setYield(std::move(yields), std::move(over));
  }
}

bool pickPrioritized(const occ::handle<AIS_InteractiveContext>& context) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  return !selector.IsNull() && selector->prioritized();
}

void setPickAhead(const occ::handle<AIS_InteractiveContext>& context, PickPreference ahead,
                  PickPreference passed) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  if (!selector.IsNull()) {
    selector->setAhead(std::move(ahead), std::move(passed));
  }
}

bool pickMovedAhead(const occ::handle<AIS_InteractiveContext>& context) {
  const auto selector = occ::handle<EdgeFirstSelector>::DownCast(context->MainSelector());
  return !selector.IsNull() && selector->movedAhead();
}

} // namespace mitcad
