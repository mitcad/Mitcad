// SPDX-License-Identifier: MIT
#pragma once

// A look at the faces a fillet or chamfer made, for a warning where the
// result is suspect although OCCT's checker accepts it (mitcad#1253): a
// corner that folds back on itself or ends in a spike reaches out of the
// body it rounds, where a rounding of convex edges only takes material away
// (or into it, where one of concave edges only adds material). The result
// stays as it is; the warning tells the user to check it. A refusal would
// break designs that recompute today.

#include <optional>
#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The side of its input a dressup's faces lie on: within it (convex edges,
// material taken away) or outside it (concave edges, material added).
enum class DressupSide { Within, Outside };

// The side for the selected edges of `body` (`owner`: a set per edge, -1
// for none): Within when every one is convex, Outside when every one is
// concave, none when they are mixed or one is neither.
std::optional<DressupSide> dressup_side(const Shape& body, const std::vector<int>& owner);

// The warning for a dressup (`role`: "fillet" or "chamfer") of `input`
// whose made faces (those named <feature>:…) reach to the wrong side of
// `input` farther than 1 % of `size` (its largest radius or distance) and
// their tolerances: "the corner of the fillet at vertex V{…} reaches
// 0.43 mm out of the body …; check the result", the deepest such face
// with how many more. Empty when there is none or it cannot be told
// (an input that is not closed solids). Looks at a bounded number of
// points of the made faces only, each told by a short ray to the input's
// boundary near it (a solid classifier only where the ray cannot tell: on
// a thread's body it cost 40 times the rounding, mitcad#1257). Of concave
// edges also (mitcad#1260): a made face of an edge hanging out past the
// body's original extent, a point of it not over both of the edge's faces
// (no input boundary on a short ray to its foot on either face's surface,
// planes, cylinders, cones, spheres and tori:
// "… reaches 0.5 mm past the body's original extent …"), and a face of the
// result on the surface of the input's face it is named after but facing
// the other way ("face F… is turned over on its surface …").
std::string dressup_warning(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                            DressupSide side, double size);

// A face of a dressup's result by its name, for a message: "the fillet of
// edge E{…}", "the corner of the fillet at vertex V{…}" (without the word
// "vertex" unless `vertex_word`: the .f3d import reads a failure that
// names one as one that more edges may mend), "face <name> of the fillet"
// for another name of the feature, else "face <name>".
std::string made_face(const std::string& name, const std::string& feature, const char* role, bool vertex_word);

} // namespace mitcad::geometry::detail
