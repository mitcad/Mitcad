// SPDX-License-Identifier: MIT
#pragma once

// The material one body has and a near copy of it lacks: the import's
// history-based guesses compare a replayed body with the body stored for
// the next state of its history (mitcad#85).
//
// The two are the same part but for what the feature changed, so almost
// every face of one lies on a face of the other, nearly but not exactly
// (the replay's own surfaces against the stored ones, a few hundredths of
// a millimetre apart on free-form faces). A boolean of the whole bodies
// intersects every such pair of faces: OCCT walks along near-coincident
// splines for seconds per pair, minutes for a part, and its result is
// often wrong. Here the boolean runs only around the faces where the two
// differ by more than a slack: the faces of `after` with a point farther
// than that from `before`'s faces (new faces: a hole's walls and bottom)
// and the faces of `before` with every point that far from `after`'s
// (faces gone). Each region is a box around such faces, grown so that the
// material removed there lies inside it; `before` is cut down to the
// boxes, `after` to the boxes grown a little more, and the removed
// material is the one minus the other. When a piece that counts (not a
// sliver, see below) reaches the side of its box, the boxes grow and the
// cut runs again; when they would take half of `before`'s box, the whole
// bodies are cut instead.

#include <optional>

#include "mitcad/geometry/boolean.hpp"

namespace mitcad::geometry {

// `before` minus `after`, as the pieces of a cut of target `before` with
// tool `after` (the result of boolean(Cut, {&before}, after) where the
// bodies differ by more than `slack`, mm). Material within `slack` of the
// faces both have (thinner than about twice `slack`) is left out, as are
// the slivers along near-coincident faces where the bodies differ.
// Pieces that count are larger than 1e-3 of the largest piece and 1e-9
// mm^3 (smaller ones are slivers along shared faces).
BooleanResult removed_material(const Shape& before, const Shape& after, double slack);

// The join of `body` with a near copy of it (mitcad#88): the mirror image
// of a nearly symmetric body lies on the body almost everywhere, nearly but
// not exactly, and a boolean of the two intersects every such pair of
// faces (minutes for a part with free-form faces, often with a wrong
// result). Here the copy is joined only where the two differ by more than
// `slack`: what the copy adds there (the copy minus the body, found as
// removed_material finds it) is joined to the body, and material of the
// copy within `slack` of the body's faces is left out. When no face
// differs by more, the result is the body itself. None when the copy is
// not a near copy (more than half the faces of either differ from the
// other's, as for a mirror image beside the body) or the region approach
// does not apply (removed_material's limits): the caller joins the whole
// shapes. The result is a join's (boolean.hpp) of the one target `body`.
std::optional<BooleanResult> join_near_copy(const Shape& body, const Shape& copy, double slack);

} // namespace mitcad::geometry
