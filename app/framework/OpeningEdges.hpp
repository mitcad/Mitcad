// SPDX-License-Identifier: MIT
#pragma once

// Openings of surface bodies picked by their free edges (mitcad#1927).
// A free edge (an edge of one face only, on the open boundary of a surface
// body) has no name of its own, so the view cannot name it as an edge; an
// input that takes openings (InputDef::takesOpenings) takes the whole loop
// it lies on instead, named as a definition names an opening:
// {"faces": [...], "index": k}, the faces its edges bound and its place
// among the openings along the same faces (features/patch.rs, `Opening`;
// openingEdgeItem in Selection.hpp).

#include <optional>
#include <vector>

#include <QJsonObject>

#include "mitcad/geometry/shape.hpp"

namespace mitcad {

// The opening the body's edge (an index of `body`) is a free edge of; none
// when it is not a free edge, or the faces of its opening have no names.
std::optional<QJsonObject> openingOfEdge(const geometry::Shape& body, int edge);

// The free edges (indices of `body`) of the opening named so; empty when
// the body has no such opening.
std::vector<int> openingEdges(const geometry::Shape& body, const QJsonObject& opening);

} // namespace mitcad
