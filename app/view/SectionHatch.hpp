// SPDX-License-Identifier: MIT
#pragma once

// The hatch over Section Analysis' caps (mitcad#382, mitcad#2146): lines
// in the section plane across each cap face, cut at the face's
// triangulation. Plain OCCT, without the view: OcctViewer::setSectionCaps
// draws it, the unit test tests/section_hatch.cpp checks it.

#include <TopoDS_Shape.hxx>
#include <gp_Ax3.hxx>

namespace mitcad {

// A hatch over the section caps `caps` (faces in `plane`): lines at 45
// degrees to the plane's x axis (135 degrees when `mirrored`). The lines
// lie in the plane itself, not lifted off it (mitcad#2146): the view draws
// them over the caps' fill by the caps' polygon offset behind the plane
// and the layer it draws them in. Each cap face gets its own spacing:
// about eight lines across a small face, but never closer than 40 lines
// across all the caps (a large face does not turn into a solid colour)
// nor wider than a 400th of them. A face without a triangulation is meshed
// at a thousandth of the caps' size. A null shape when no line crosses a
// cap.
TopoDS_Shape sectionHatch(const TopoDS_Shape& caps, const gp_Ax3& plane, bool mirrored);

// The largest distance (mm) of a vertex of `hatch` from `plane`, 0 for a
// null or empty hatch: the section look's log line names it.
double sectionHatchPlaneDistance(const TopoDS_Shape& hatch, const gp_Ax3& plane);

} // namespace mitcad
