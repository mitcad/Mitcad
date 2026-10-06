// SPDX-License-Identifier: MIT
#include "PickContext.hpp"

#include <algorithm>
#include <cmath>
#include <iterator>
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
    // Only a click or the hover has one item to choose; a window takes all.
    // (With nothing picked, the indexes are left over from the last pick.)
    if (mystored.Extent() < 2 ||
        mySelectingVolumeMgr.GetActiveSelectionType() != SelectMgr_SelectionType_Point) {
      return;
    }
    const int first = myIndexes.Lower();
    const int nearestIndex = myIndexes(first);
    const occ::handle<SelectMgr_EntityOwner>& nearest = mystored.FindKey(nearestIndex);
    const TopoDS_Shape* nearestShape = ownedShape(nearest);
    const int nearestDimension = nearestShape != nullptr ? dimensionOf(*nearestShape) : -1;
    if (nearestDimension < 1) {
      return; // not a body's face or edge
    }
    const SelectMgr_SortCriterion& hit = mystored.FindFromIndex(nearestIndex);
    const gp_Pnt rayStart = mySelectingVolumeMgr.GetNearPickedPnt();
    const gp_Dir ray = mySelectingVolumeMgr.GetViewRayDirection();
    struct Candidate {
      int index;     // into mystored
      int dimension; // vertices before edges
      double offset; // from the cursor, in the entity's pick widths
    };
    std::vector<Candidate> promoted;
    std::vector<int> rest;
    for (int rank = first + 1; rank <= myIndexes.Upper(); ++rank) {
      const int index = myIndexes(rank);
      const occ::handle<SelectMgr_EntityOwner>& owner = mystored.FindKey(index);
      const TopoDS_Shape* shape = ownedShape(owner);
      const int dimension = shape != nullptr ? dimensionOf(*shape) : -1;
      const SelectMgr_SortCriterion& data = mystored.FindFromIndex(index);
      if (dimension < 0 || dimension >= nearestDimension || owner->Selectable() != nearest->Selectable() ||
          !bounds(*shape, *nearestShape) || !onSurface(data, hit)) {
        rest.push_back(index);
        continue;
      }
      const double offset = gp_Vec(rayStart, data.Point).Crossed(gp_Vec(ray)).Magnitude();
      promoted.push_back({index, dimension, data.Tolerance > 0.0 ? offset / data.Tolerance : offset});
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

private:
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
    // Nothing is displayed yet: the selection manager can still change.
    mgrSelector = new SelectMgr_SelectionManager(new EdgeFirstSelector());
  }
};

} // namespace

occ::handle<AIS_InteractiveContext> makePickContext(const occ::handle<V3d_Viewer>& viewer) {
  return new PickContext(viewer);
}

} // namespace mitcad
