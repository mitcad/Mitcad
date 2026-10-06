// SPDX-License-Identifier: MIT
#pragma once

#include <AIS_InteractiveContext.hxx>
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

} // namespace mitcad
