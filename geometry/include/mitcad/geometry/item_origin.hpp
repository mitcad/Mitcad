// SPDX-License-Identifier: MIT
#pragma once

// The item of a body before a feature that an item of the feature's result
// comes from (mitcad#1458). While a feature's panel is open the view draws
// its preview, so a click names a face, edge or vertex of the result; the
// feature's definition needs those of the body before it. A face the
// feature changed keeps its name (naming.hpp) or, split, its name with
// "#k"; an edge or vertex is named after its faces, numbered "#k" among
// those between the same faces, and that number changes when the feature
// splits one (a foot edge whose ends two side fillets round runs on in
// pieces under the roundings' end faces). Such an item is found by its
// faces on the body before; one the feature made has a face of the
// feature's own there, and none.

#include <string>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

struct ItemOrigin {
  enum class Kind {
    // The body before has it under the same name (or, where no one item
    // there is told, resolves the name as it is).
    Same,
    // The feature changed it (cut short, split, numbered anew): `name` is
    // the item before.
    Modified,
    // One of its faces is the feature's own (a fillet's rounding): it is
    // not on the body before.
    Made,
    // Its faces are on the body before, but no one item there between them
    // is told (none, or several and nothing to choose by), and the body
    // before does not resolve its name.
    Lost,
  };
  Kind kind = Kind::Lost;
  // The item's name on the body before (Same and Modified), as the body
  // names it: the first name of a face, the derived name of an edge or a
  // vertex.
  std::string name;
};

// The origin of the face ('F'), edge ('E') or vertex ('V') `name` of
// `after` (a feature's result; null when there is none to look at) on
// `before`. Where several items of `before` lie between the same faces,
// the one nearest to the item of `after` is taken (a point inside the face,
// the middle of the edge, the vertex), when one is nearest; without
// `after`, the one the name gives.
ItemOrigin item_origin(const Shape& before, const Shape* after, char kind, const std::string& name);

} // namespace mitcad::geometry
