// SPDX-License-Identifier: MIT
#pragma once

// Pipes and sweeps that pass through themselves (mitcad#1697): a profile
// that reaches past the centre of a bend of its path, a straight piece too
// short for the miters at its ends, or a path that comes back near itself
// sweeps a solid whose sides cross. The sweep is built in pieces instead,
// cut where it folds, each piece a sound solid, and the pieces are fused
// (OCCT's boolean union), so that the material they share counts once.
// Internal to the geometry library.

#include <functional>
#include <vector>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/shape.hpp"
#include "spine.hpp"

namespace mitcad::geometry::detail {

// A profile swept along a spine: its loops (the outer one first, then its
// holes) where the profile lies, and the names of its caps at the spine's
// start and at its end.
struct SweptProfile {
  std::vector<NamedWire> loops;
  NameList first;
  NameList last;
};

// How the profiles are swept: along `spine` (the part of the path swept),
// lying at arc length `at` along it, round the whole closed spine when
// `around`, turned by `mode`, the surfaces within `tolerance`, the spine's
// corners joined as `corners` asks. `what` names the result in errors.
struct FoldedSweep {
  const Spine* spine = nullptr;
  double at = 0.0;
  bool around = false;
  PipeMode mode;
  double tolerance = 1.0e-4;
  PipeCorners corners = PipeCorners::Mitered;
  const char* what = "the sweep";
};

// What the sweep gave: the solid (unfinished names), and whether it was
// built in pieces where it passes through itself.
struct UnfoldedSweep {
  ShapePtr shape;
  bool resolved = false;
};

// The information a solid built in pieces carries (Shape::infos; the
// model's `sweeps::FOLDS_RESOLVED` is the same text).
inline constexpr const char* kFoldsResolved =
    "the swept profile passes through itself along the path; it was built in pieces joined into one solid";

// The profiles swept along the spine. Where they pass through themselves
// (above), and where `whole` (the sweep in one piece, as it is built
// without this) fails, the sweep is cut there and built in pieces whose
// union is the result. Elsewhere, and where the spine's frames are not
// known here, `whole`: they are known along a constant binormal (a path
// in one plane, a lone helix), and (mitcad#1708) along the corrected
// Frenet frame of a spine of lines and circular arcs, or of any spine for
// profiles that are circles about the path. (mitcad#1709) A round profile
// along a tight bend of another curve in one plane, tightest at one point
// inside it, is built as the tube its rim sweeps up to where that tube
// meets itself, sewn into one solid; along a path in no plane (a tight
// bend of a helix between straight legs) likewise, where the crease's
// corners lie at the path's joints. Throws a refusal in words,
// with a place, where the sweep folds and neither way builds it.
UnfoldedSweep swept_resolving_folds(const FoldedSweep& sweep, const std::vector<SweptProfile>& profiles,
                                    const std::function<ShapePtr()>& whole);

} // namespace mitcad::geometry::detail
