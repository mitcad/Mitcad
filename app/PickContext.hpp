// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <AIS_InteractiveContext.hxx>
#include <SelectMgr_EntityOwner.hxx>
#include <V3d_Viewer.hxx>

namespace mitcad {

// Pick widths in logical pixels: the whole width of the square around the
// cursor in which an entity is found (OCCT's sensitivity plus its custom
// tolerance). Edges and vertices are thin targets and get more room than
// faces (mitcad#29).
constexpr int kPickTolerance = 4;      // added to every entity's sensitivity
constexpr int kFaceSensitivity = 2;    // faces, bodies, profiles, planes
constexpr int kEdgeSensitivity = 8;    // edges and curves: 12 in all, 6 pixels on each side
constexpr int kVertexSensitivity = 12; // vertices and points: 16 in all, 8 pixels around

// An interactive context whose picks rank an edge or vertex near the
// cursor before the face it bounds (mitcad#29). OCCT sorts what lies under
// the cursor by depth, so near an edge the face beside it often wins: on
// an inside corner the faces rise towards the eye and are always closer
// than the edge. Here, when the nearest item is a body's face, the edges
// and vertices of that face that are within their pick width come first,
// whatever their depth, nearest the cursor first; the same for the
// vertices of a nearest edge. An edge only counts when it lies on the
// face's surface where the face was hit, so the edges a curved face hides
// behind itself (a cylinder's far seam) do not. The highlight under the
// cursor and a click take the same first item.
occ::handle<AIS_InteractiveContext> makePickContext(const occ::handle<V3d_Viewer>& viewer);

// Selection overlays share a pick source with their original body in the
// same occurrence. The topology stays local to that source, so its edges
// and vertices can rank before a selected face on another AIS object.
using PickSourceResolver = std::function<const SelectMgr_SelectableObject*(
    const occ::handle<SelectMgr_EntityOwner>&)>;
void setPickSources(const occ::handle<AIS_InteractiveContext>& context, PickSourceResolver resolve);

// The body object a selection overlay covers while a command's preview
// shows that body changed (the same body and occurrence, another shape),
// or null. Where the overlay is hit no farther than that body's first hit
// (within both pick tolerances), its hits come before that body's, so a
// click takes the selected item, or one of its edges, as drawn, not the
// preview's face of the same name under it (mitcad#672). The preview's
// own items still win where they lie in front.
using PickCoverResolver = PickSourceResolver;
void setPickCovers(const occ::handle<AIS_InteractiveContext>& context, PickCoverResolver resolve);

// The active component first (mitcad#420): with a preference set on a
// context makePickContext made, a click's and the hover's items under the
// cursor are ordered with the preferred owners first (each group in the
// order above), so the nearest preferred one is taken before nearer others;
// where none is preferred (or all are), the order stays. pickPrioritized():
// the last such pick put a preferred owner before a nearer other one.
using PickPreference = std::function<bool(const occ::handle<SelectMgr_EntityOwner>&)>;
void setPickPriority(const occ::handle<AIS_InteractiveContext>& context, PickPreference preferred);
bool pickPrioritized(const occ::handle<AIS_InteractiveContext>& context);

// Owners put behind the others under the cursor, before the orders above
// (mitcad#656: construction planes while a placement input takes picks,
// so that a shaft or a face the origin planes pass through is picked, not
// the plane in front of it); a plane alone under the cursor is still
// picked. An unset predicate puts none last.
void setPickLast(const occ::handle<AIS_InteractiveContext>& context, PickPreference last);

// Owners that give way to others under the cursor (mitcad#869: Measure's
// sketch curves and points to the bodies' faces, edges and vertices),
// ordered right after setPickCovers' rule and before the others above: the
// owners `over` that lie under the cursor behind the first owner that
// `yields` come before it, in their order; the others keep theirs. Where
// none of `over` is under the cursor, a yielding owner is picked as before.
// Unset predicates move nothing.
void setPickYield(const occ::handle<AIS_InteractiveContext>& context, PickPreference yields,
                  PickPreference over);

// Owners that go ahead of the areas in front of them (mitcad#1222: the
// sketch regions on offer while an input takes profiles, also where their
// plane lies inside or behind a body): the owners `ahead` that lie under
// the cursor behind the first owner that is `passed` (a body's face, a
// whole body, a plane) come before it, in their order; the others keep
// theirs. Ordered after the edge and vertex rules above, so that an edge
// they put first keeps its place, and before the active component's
// preference. Where none of `ahead` is under the cursor, the area is
// picked as before. Unset predicates move nothing. pickMovedAhead(): the
// last such pick put an `ahead` owner before a nearer area.
void setPickAhead(const occ::handle<AIS_InteractiveContext>& context, PickPreference ahead,
                  PickPreference passed);
bool pickMovedAhead(const occ::handle<AIS_InteractiveContext>& context);

} // namespace mitcad
