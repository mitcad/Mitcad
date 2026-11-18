// SPDX-License-Identifier: MIT
#pragma once

// Openings of surface bodies picked by their free edges (mitcad#1927).
// A free edge (an edge of one face only, on the open boundary of a surface
// body) has no name of its own: the view names it by the opening it lies on
// and its middle point (a single free edge, freeEdgeItem in Selection.hpp,
// mitcad#1955), and an input that takes openings (InputDef::takesOpenings)
// with Whole Opening on takes the whole loop instead, named as a definition
// names an opening: {"faces": [...], "index": k}, the faces its edges bound
// and its place among the openings along the same faces
// (features/patch.rs, `Opening`; openingEdgeItem in Selection.hpp).

#include <array>
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

// The middle of the body's edge (half way along its curve), rounded to
// 1e-6 mm: the point a single free edge is named by (mitcad#1955,
// freeEdgeItem), the same at every pick of the edge.
std::array<double, 3> edgeMiddle(const geometry::Shape& body, int edge);

// The free edge of the opening nearest the point, as a definition's
// `openings` entry with `at` picks it (mitcad#1955); none when the body has
// no such opening.
std::optional<int> freeEdgeAt(const geometry::Shape& body, const QJsonObject& opening, const std::array<double, 3>& at);

} // namespace mitcad
